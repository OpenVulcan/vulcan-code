use super::*;
use std::fs;
use std::io::BufReader;
use std::path::Path;
use vulcan_task_session::TaskSessionMetadataInit;

/// Recursively copies one directory tree for directory-isolation tests.
/// 为目录隔离测试递归复制一棵目录树。
fn copy_directory_tree_for_test(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should be created");
    for entry_result in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry_result.expect("source directory entry should be readable");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .expect("source directory entry type should be readable")
            .is_dir()
        {
            copy_directory_tree_for_test(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path)
                .expect("source file should copy into destination directory");
        }
    }
}

/// Builds one persisted task metadata document for task-runtime tests.
/// 为任务运行时测试构建单个持久化任务元数据文档。
fn persisted_task_metadata(
    root_session_id: Uuid,
    task_id: Uuid,
    parent: ParentSessionRef,
    public_task_id: &str,
    display_title: &str,
    resolved_config: TaskSessionResolvedConfig,
    timestamp_ms: u64,
) -> TaskSessionMetadata {
    TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id: SessionNodeId::from(root_session_id),
        task_session_id: SessionNodeId::from(task_id),
        parent,
        public_task_id: public_task_id.to_string(),
        display_title: display_title.to_string(),
        requested_config: resolved_config.requested_config,
        effective_config: resolved_config.effective_config,
        depth: 1,
        timestamp_ms,
    })
}

/// Builds one persisted task metadata document with default requested and effective config.
/// 使用默认请求配置与有效配置构建单个持久化任务元数据文档。
fn default_persisted_task_metadata(
    root_session_id: Uuid,
    task_id: Uuid,
    parent: ParentSessionRef,
    public_task_id: &str,
    display_title: &str,
) -> TaskSessionMetadata {
    TaskSessionMetadata::new(TaskSessionMetadataInit {
        root_session_id: SessionNodeId::from(root_session_id),
        task_session_id: SessionNodeId::from(task_id),
        parent,
        public_task_id: public_task_id.to_string(),
        display_title: display_title.to_string(),
        requested_config: SessionRequestedConfig::default(),
        effective_config: SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 100,
    })
}

/// Verifies persisted task metadata and context are used when memory has no matching task record.
/// 验证内存没有匹配任务记录时会使用持久化任务元数据与上下文。
#[test]
fn prepare_task_execution_request_restores_persisted_context_for_resume() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let parent = ParentSessionRef::MainSession {
        root_session_id: SessionNodeId::from(root_session_id),
    };
    let mut requested_invocation = TaskInvocationConfig::main("initial prompt");
    requested_invocation.system_prompt = Some(String::from("Stored system prompt"));
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: None,
        profile: None,
        invocation: requested_invocation,
    });
    let mut metadata = persisted_task_metadata(
        root_session_id,
        task_id,
        parent.clone(),
        "agent-review",
        "Agent review",
        resolved_config,
        100,
    );
    metadata.status = TaskSessionStatus::Completed;
    store
        .create_task_session(metadata.clone())
        .expect("task metadata should store");
    let conversation_state =
        core_snapshot_to_session_conversation_state(&CoreConversationSnapshot::default());
    store
        .save_context(SessionContextDocument {
            schema_version: 1,
            node_id: SessionNodeId::from(task_id),
            provider_id: Some(String::from("persisted-provider")),
            model_id: Some(String::from("persisted-model")),
            reasoning_effort: None,
            system_prompt_layers: vec![String::from("Persisted system prompt")],
            conversation_state: serde_json::to_value(conversation_state)
                .expect("conversation state should serialize"),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("task context should store");

    let args = test_task_args("continue the review", Some("agent-review"));
    let resolved_identifier = resolve_task_session_id(&task_runtime, root_session_id, &args)
        .expect("persisted task id should resolve");
    assert!(resolved_identifier.is_resumed);
    assert_eq!(resolved_identifier.task_id, task_id);

    let request =
        prepare_task_execution_request(&task_runtime, root_session_id, resolved_identifier, args)
            .expect("persisted task should prepare");
    let controller_config = request
        .controller_config
        .expect("persisted controller config should restore");
    assert_eq!(controller_config.provider_id, "persisted-provider");
    assert_eq!(controller_config.model_id, "persisted-model");
    assert!(controller_config.system_prompt.is_empty());
    assert!(controller_config.prompt_layers.is_empty());
    assert_eq!(request.task_id, task_id);
    assert!(request.is_resumed);
}

/// Verifies list_agents restores persisted active task sessions as open stopped agents.
/// 验证 list_agents 会将持久化的活动子任务恢复为 open/stopped 子代理。
#[tokio::test]
async fn list_agents_uses_hydrated_registry_for_persisted_active_agent() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: None,
        profile: None,
        invocation: TaskInvocationConfig::main("persisted running prompt"),
    });
    let mut metadata = persisted_task_metadata(
        root_session_id,
        task_id,
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "persisted-review",
        "Persisted review",
        resolved_config,
        100,
    );
    metadata.status = TaskSessionStatus::Running;
    store
        .create_task_session(metadata)
        .expect("running task metadata should store");

    let lazy_result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should not perform session hydration");
    assert_eq!(
        lazy_result.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        0,
        "list_agents must not lazily restore persisted agents"
    );
    task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("session hydration should restore persisted subagents");
    let restored_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after hydration");
    let restored_entry = restored_registry
        .get("persisted-review")
        .expect("hydrated registry entry should exist");
    assert_eq!(restored_entry.prompt_source.as_deref(), Some("core"));
    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should read hydrated subagents");
    assert_eq!(
        result.model_output["agents"][0]["agent_name"],
        "persisted-review"
    );
    assert_eq!(result.model_output["agents"][0]["lifecycle_state"], "open");
    assert_eq!(result.model_output["agents"][0]["run_state"], "stopped");
    assert_eq!(
        result.model_output["agents"][0]["stop_reason"],
        "runtime_lost"
    );
    assert_eq!(result.model_output["agents"][0]["can_send"], true);
    assert_eq!(result.model_output["agents"][0]["can_close"], true);
    assert_eq!(result.model_output["agents"][0]["can_interrupt"], false);
    let restored_metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(restored_metadata.status, TaskSessionStatus::Stopped);
    assert_eq!(
        restored_metadata.stop_reason.as_deref(),
        Some("runtime_lost")
    );
    let sessions = task_runtime
        .sessions
        .read()
        .expect("task session registry lock");
    assert_eq!(
        sessions
            .get(&task_id)
            .expect("restored task record should exist")
            .last_status,
        "stopped"
    );
}

/// Verifies list_agents restores persisted completed agents as still-open manageable agents.
/// 验证 list_agents 会将持久化的已完成子代理恢复为仍未关闭且可管理的代理。
#[tokio::test]
async fn list_agents_uses_hydrated_registry_for_persisted_completed_agent() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: None,
        profile: None,
        invocation: TaskInvocationConfig::main("persisted completed prompt"),
    });
    let mut metadata = persisted_task_metadata(
        root_session_id,
        task_id,
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "completed-review",
        "Completed review",
        resolved_config,
        100,
    );
    metadata.status = TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("persisted completed result"));
    store
        .create_task_session(metadata)
        .expect("completed task metadata should store");

    task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("session hydration should restore completed subagents");
    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should read hydrated completed subagents");

    assert_eq!(
        result.model_output["agents"][0]["agent_name"],
        "completed-review"
    );
    assert_eq!(result.model_output["agents"][0]["lifecycle_state"], "open");
    assert_eq!(result.model_output["agents"][0]["run_state"], "completed");
    assert_eq!(
        result.model_output["agents"][0]["agent_status"]["completed"],
        "persisted completed result"
    );
    assert_eq!(result.model_output["agents"][0]["can_send"], true);
}

/// Verifies close_agent writes archived status back to persisted TaskSession metadata.
/// 验证 close_agent 会把归档状态写回持久化 TaskSession 元数据。
#[tokio::test]
async fn close_agent_persists_archived_status_for_restored_agent() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: None,
        profile: None,
        invocation: TaskInvocationConfig::main("persisted work"),
    });
    let mut metadata = persisted_task_metadata(
        root_session_id,
        task_id,
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "persisted-worker",
        "Persisted worker",
        resolved_config,
        100,
    );
    metadata.status = TaskSessionStatus::Running;
    store
        .create_task_session(metadata)
        .expect("running task metadata should store");

    task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("session hydration should restore persisted worker");
    let close_result = task_runtime
        .invoke_close_agent(root_session_id, json!({ "target": "persisted-worker" }))
        .await
        .expect("close_agent should close hydrated persisted agent");
    assert_eq!(close_result.model_output["closed"], true);
    let archived_metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(archived_metadata.status, TaskSessionStatus::Archived);
    assert_eq!(
        archived_metadata.stop_reason.as_deref(),
        Some("close_agent")
    );
    let parent_ref = ParentSessionRef::MainSession {
        root_session_id: SessionNodeId::from(root_session_id),
    };
    let mirrors = store
        .load_task_session_mirror_summaries_by_parent_for_root_tree(SessionNodeId::from(
            root_session_id,
        ))
        .expect("root task mirror should load");
    let archived_mirror = mirrors
        .get(&parent_ref)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.task_session_id == SessionNodeId::from(task_id))
        })
        .expect("closed task should remain in diagnostic root mirror");
    assert_eq!(archived_mirror.status, TaskSessionStatus::Archived);

    let late_invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("persisted-worker"),
        prompt: String::from("late completed work"),
        description: Some(String::from("Persisted worker")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("late completed work"),
        display_title: String::from("Persisted worker"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let late_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(
            &controller_config,
            &late_invocation.invocation_config,
        ),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-late")),
        final_output_text: String::from("late result"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_task_outcome(root_session_id, &late_invocation, &late_outcome)
        .expect("late outcome should not reopen archived metadata");
    let metadata_after_late_outcome = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load after late outcome")
        .expect("metadata should exist after late outcome");
    assert_eq!(
        metadata_after_late_outcome.status,
        TaskSessionStatus::Archived
    );
    assert_eq!(
        metadata_after_late_outcome.stop_reason.as_deref(),
        Some("close_agent")
    );

    let fresh_runtime = Arc::new(TaskRuntime::new());
    fresh_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("fresh store root should configure");
    fresh_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("fresh session hydration should restore non-archived agents only");
    let default_list = fresh_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("default list should succeed");
    assert_eq!(
        default_list.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        0
    );
    let diagnostic_list = fresh_runtime
        .invoke_list_agents(root_session_id, json!({ "include_closed": true }))
        .await
        .expect("diagnostic list should succeed");
    assert_eq!(
        diagnostic_list.model_output["agents"]
            .as_array()
            .expect("diagnostic agents should be an array")
            .len(),
        0
    );

    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_task_id = Arc::new(std::sync::Mutex::new(None));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_task_id_for_handler = Arc::clone(&captured_task_id);
    let _guard = fresh_runtime
        .install_backend(
            root_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                *captured_task_id_for_handler
                    .lock()
                    .expect("captured task id lock") = Some(request.task_id);
                Box::pin(async move {
                    let controller_config = build_test_controller_config("Fresh persisted prompt");
                    Ok(TaskExecutionOutcome {
                        task_id: request.task_id,
                        controller_config: controller_config.clone(),
                        resolved_config: test_resolved_config(
                            &controller_config,
                            &request.invocation_config,
                        ),
                        snapshot: CoreConversationSnapshot::default(),
                        provider_turns_executed: 1,
                        tool_calls_processed: 0,
                        final_assistant_message_id: Some(String::from("assistant-fresh-persisted")),
                        final_output_text: String::from("fresh persisted result"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");
    fresh_runtime
        .invoke_spawn_agent(
            root_session_id,
            json!({
                "prompt": "fresh persisted work",
                "task_id": "persisted-worker"
            }),
        )
        .await
        .expect("spawn_agent should reuse an archived persisted public id");
    let fresh_task_id = captured_task_id
        .lock()
        .expect("captured task id lock")
        .expect("backend should capture the fresh persisted task id");
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    assert_ne!(fresh_task_id, task_id);
    let selected_metadata = fresh_runtime
        .persisted_task_metadata_for_public_id(
            root_session_id,
            ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(root_session_id),
            },
            "persisted-worker",
        )
        .expect("public-id metadata lookup should succeed")
        .expect("fresh metadata should be selected");
    assert_eq!(
        selected_metadata.task_session_id,
        SessionNodeId::from(fresh_task_id)
    );
    assert_eq!(selected_metadata.status, TaskSessionStatus::Completed);
}

/// Verifies persisted completed metadata is returned directly when memory has no record.
/// 验证内存没有记录时会直接返回持久化的已完成元数据。
#[tokio::test]
async fn invoke_task_returns_cached_completed_metadata_without_backend_restart() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: None,
        profile: None,
        invocation: TaskInvocationConfig::main("generate persisted image"),
    });
    let mut metadata = persisted_task_metadata(
        root_session_id,
        task_id,
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "image-generator-2",
        "Image generator 2",
        resolved_config,
        100,
    );
    metadata.status = TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("persisted cached image result"));
    store
        .create_task_session(metadata)
        .expect("task metadata should store");
    let parent_ref = ParentSessionRef::MainSession {
        root_session_id: SessionNodeId::from(root_session_id),
    };
    let invocation_events_before = store
        .list_task_invocation_events(parent_ref.clone())
        .expect("parent task invocation events should load before cached lookup");
    assert_eq!(
        invocation_events_before.len(),
        1,
        "creating task metadata writes only the initial parent link event"
    );
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let _guard = task_runtime
        .install_backend(
            root_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let controller_config = build_test_controller_config("Backend prompt");
                    Ok(TaskExecutionOutcome {
                        task_id: request.task_id,
                        controller_config: controller_config.clone(),
                        resolved_config: test_resolved_config(
                            &controller_config,
                            &request.invocation_config,
                        ),
                        snapshot: CoreConversationSnapshot::default(),
                        provider_turns_executed: 1,
                        tool_calls_processed: 0,
                        final_assistant_message_id: Some(String::from("assistant-backend")),
                        final_output_text: String::from("backend result"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");

    let result = task_runtime
        .invoke_task(
            root_session_id,
            json!({
                "prompt": "check persisted image result",
                "task_id": "image-generator-2"
            }),
        )
        .await
        .expect("cached persisted task should return");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);
    assert_eq!(result.model_output["cached"], true);
    assert_eq!(result.model_output["execution_status"], "completed");
    assert_eq!(
        result.model_output["result"],
        "persisted cached image result"
    );
    let invocation_events_after = store
        .list_task_invocation_events(parent_ref)
        .expect("parent task invocation events should load after cached lookup");
    assert_eq!(
        invocation_events_after.len(),
        invocation_events_before.len(),
        "cached persisted lookup must not append another parent task invocation event"
    );
    assert!(
        task_runtime
            .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
            .expect("mailbox should drain")
            .is_empty(),
        "cached persisted lookup must not publish duplicate subagent mailbox events"
    );
    assert!(
        task_runtime
            .sessions
            .read()
            .expect("task session registry should be readable")
            .is_empty(),
        "cached persisted lookup must not materialize a resumable run record"
    );
}

/// Verifies stopped-task fallback snapshots can be restored from the persisted TaskSession context.
/// 验证停止任务的回退快照可从持久化 TaskSession 上下文恢复。
#[test]
fn current_task_snapshot_from_store_reads_persisted_context() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let parent = ParentSessionRef::MainSession {
        root_session_id: SessionNodeId::from(root_session_id),
    };
    let metadata = default_persisted_task_metadata(
        root_session_id,
        task_id,
        parent,
        "agent-review",
        "Agent review",
    );
    store
        .create_task_session(metadata)
        .expect("task metadata should store");
    let snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("assistant-from-file"),
            payload: vulcan_core::types::ConversationPayload::Assistant {
                text: String::from("persisted answer"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };
    store
        .save_context(SessionContextDocument {
            schema_version: 1,
            node_id: SessionNodeId::from(task_id),
            provider_id: Some(String::from("provider")),
            model_id: Some(String::from("model")),
            reasoning_effort: None,
            system_prompt_layers: vec![String::from("Persisted system prompt")],
            conversation_state: serde_json::to_value(core_snapshot_to_session_conversation_state(
                &snapshot,
            ))
            .expect("conversation state should serialize"),
            compaction_anchor: Some(String::from("task-summary-existing")),
            context_budget_fingerprint: Some(String::from("budget-existing")),
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("task context should store");

    let loaded = current_task_snapshot_from_store(&task_runtime, root_session_id, task_id)
        .expect("persisted snapshot load should not fail")
        .expect("persisted snapshot should exist");

    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].id, "assistant-from-file");
}

/// Verifies missing TaskSession storage is reported distinctly from corrupted storage.
/// 验证缺失的 TaskSession 存储会与损坏存储区分返回。
#[test]
fn current_task_snapshot_from_store_returns_none_without_store_root() {
    let task_runtime = TaskRuntime::new();
    let loaded = current_task_snapshot_from_store(&task_runtime, Uuid::new_v4(), Uuid::new_v4())
        .expect("missing store root should not be an error");

    assert!(loaded.is_none());
}

/// Verifies corrupted persisted context is surfaced as an error instead of a silent fallback.
/// 验证损坏的持久化上下文会作为错误暴露，而不是静默回退。
#[test]
fn current_task_snapshot_from_store_reports_invalid_persisted_context() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let parent = ParentSessionRef::MainSession {
        root_session_id: SessionNodeId::from(root_session_id),
    };
    let metadata = default_persisted_task_metadata(
        root_session_id,
        task_id,
        parent,
        "agent-review",
        "Agent review",
    );
    store
        .create_task_session(metadata)
        .expect("task metadata should store");
    store
        .save_context(SessionContextDocument {
            schema_version: 1,
            node_id: SessionNodeId::from(task_id),
            provider_id: Some(String::from("provider")),
            model_id: Some(String::from("model")),
            reasoning_effort: None,
            system_prompt_layers: vec![String::from("Persisted system prompt")],
            conversation_state: json!("invalid conversation state"),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("task context should store invalid payload");

    let error = current_task_snapshot_from_store(&task_runtime, root_session_id, task_id)
        .expect_err("invalid persisted context should fail loudly");

    assert!(error.contains("invalid conversation state"));
}

/// Verifies task outcomes materialize segmented TaskSession artifacts when a store root is configured.
/// 验证配置存储根目录后，任务结果会物化分段 TaskSession 产物。
#[test]
fn store_task_outcome_persists_segmented_task_session_artifacts() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("agent-review"),
        prompt: String::from("review this change"),
        description: Some(String::from("Review code")),
        subagent: Some(String::from("reviewer")),
        invocation_config: TaskInvocationConfig::main("review this change"),
        display_title: String::from("Review code (reviewer)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let resolved_config = test_resolved_config(&controller_config, &invocation.invocation_config);
    let provider_usage = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(100),
        output_tokens: Some(25),
        reasoning_tokens: Some(5),
        total_tokens: Some(130),
        cached_input_tokens: Some(40),
        cache_creation_input_tokens: Some(7),
    };
    let snapshot = CoreConversationSnapshot {
        messages: vec![
            vulcan_core::types::ConversationMessage {
                id: String::from("user-1"),
                payload: vulcan_core::types::ConversationPayload::UserText {
                    text: String::from("review this change"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("assistant-1"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("review complete"),
                    reasoning: String::from("checked"),
                    provider_message_id: Some(String::from("assistant-provider-1")),
                    provider_message_state: None,
                    usage: Some(provider_usage.clone()),
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            vulcan_core::types::ConversationRecord {
                id: String::from("user-record-1"),
                role: vulcan_core::types::TranscriptRole::User,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("User"),
                body: String::from("review this change"),
                metadata: Some(json!({
                    "canonical_text": "review this change",
                    "canonical_user_text": "review this change"
                })),
                created_at: String::from("1000"),
            },
            vulcan_core::types::ConversationRecord {
                id: String::from("assistant-record-1"),
                role: vulcan_core::types::TranscriptRole::Assistant,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("Model output"),
                body: String::from("review complete"),
                metadata: Some(json!({
                    "provider_usage": provider_usage.clone()
                })),
                created_at: String::from("1100"),
            },
        ],
        continuation: Default::default(),
    };
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&provider_usage),
    );
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config,
        snapshot,
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-1")),
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: vec![usage_segment],
    };

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");
    let in_memory_controller_config = task_runtime
        .sessions
        .read()
        .expect("task sessions should be readable")
        .get(&task_id)
        .and_then(|record| record.controller_config.clone())
        .expect("task record should keep a resumable controller config");
    assert!(in_memory_controller_config.system_prompt.is_empty());
    assert!(in_memory_controller_config.prompt_layers.is_empty());

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(
        metadata.root_session_id,
        SessionNodeId::from(root_session_id)
    );
    assert_eq!(metadata.status, TaskSessionStatus::Completed);
    assert_eq!(metadata.public_task_id, "agent-review");
    assert_eq!(
        metadata.last_result_summary.as_deref(),
        Some("review complete")
    );

    let mut context = store
        .load_context(SessionNodeId::from(task_id))
        .expect("context should load")
        .expect("context should exist");
    assert_eq!(context.provider_id.as_deref(), Some("provider"));
    assert_eq!(context.model_id.as_deref(), Some("model"));
    assert!(
        context.system_prompt_layers.is_empty(),
        "default TaskSession context must not persist provider-rendered controller prompts"
    );
    let conversation_state: SessionConversationState =
        serde_json::from_value(context.conversation_state.clone())
            .expect("task context conversation state should deserialize");
    let stats_record = conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("task context should include turn stats");
    let turn_stats: SessionTurnStats = serde_json::from_value(
        stats_record
            .metadata
            .as_ref()
            .expect("turn stats metadata should exist")["turn_stats"]
            .clone(),
    )
    .expect("turn stats should deserialize");
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.tool_call_count, 0);
    assert_eq!(turn_stats.input_tokens, 100);
    assert_eq!(turn_stats.output_tokens, 25);
    assert_eq!(turn_stats.reasoning_tokens, 5);
    assert_eq!(turn_stats.total_tokens, 130);
    assert_eq!(turn_stats.cached_input_tokens, Some(40));
    assert_eq!(turn_stats.cache_creation_input_tokens, Some(7));
    assert_eq!(turn_stats.usage_segments.len(), 1);
    assert_eq!(
        turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );

    let task_dir = store
        .task_session_directory(SessionNodeId::from(task_id))
        .expect("task session directory should resolve");
    // task_main_metadata is the main-session compatible metadata document written beside task segments.
    // task_main_metadata 是写在子任务分段旁边的主会话兼容元数据文档。
    let task_main_metadata = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(task_dir.join("metadata.json"))
            .expect("task main metadata should open"),
    ))
    .expect("task main metadata should parse");
    assert_eq!(task_main_metadata["session_id"], task_id.to_string());
    assert_eq!(task_main_metadata["project_id"], "test-project");
    assert_eq!(task_main_metadata["active_segment"], "s001.jsonl");
    assert_eq!(task_main_metadata["task_dir"], "task");
    assert!(
        task_main_metadata["tasks"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "task metadata should expose nested task mirrors through the main-session tasks array"
    );
    assert!(
        vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors_from_directory(
            &task_dir,
        )
        .expect("task mirror metadata should read")
        .is_empty(),
        "main-session task mirror reader should accept TaskSession metadata without a child-specific path"
    );
    assert!(
        vulcan_workspace_store::workspace_store::load_segmented_session_fork_source_from_directory(
            &task_dir,
        )
        .expect("fork source metadata should read")
        .is_none(),
        "TaskSession metadata should remain compatible with the main-session fork-source reader"
    );
    // task_state_document is the main-session compatible state.json stored inside the task directory.
    // task_state_document 是保存在子任务目录中的主会话兼容 state.json。
    let task_state_document = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(task_dir.join("state.json")).expect("task state should open"),
    ))
    .expect("task state should parse");
    assert_eq!(task_state_document["session_id"], task_id.to_string());
    assert_eq!(
        task_state_document["runtime_state"]["active_provider_id"],
        "provider"
    );
    assert_eq!(
        task_state_document["runtime_state"]["active_model_id"],
        "model"
    );
    assert!(
        task_state_document["current_context"]["conversation_state"]["messages"]
            .as_array()
            .is_some_and(|messages| messages
                .iter()
                .any(|message| message["id"] == "assistant-1")),
        "task state current_context should contain the canonical assistant message"
    );
    assert!(task_dir.join("runtime").join("r001.jsonl").exists());
    assert!(task_dir.join("runtime_metadata.json").exists());
    assert!(task_dir.join("runtime.index.json").exists());
    let runtime_conversation_state =
        vulcan_workspace_store::workspace_store::read_segmented_session_runtime_conversation_state(
            &task_dir,
        )
        .expect("task runtime log should read")
        .expect("task runtime log should exist");
    assert!(
        runtime_conversation_state
            .messages
            .iter()
            .any(|message| message.id == "assistant-1")
    );
    assert!(
        runtime_conversation_state.records.is_empty(),
        "provider-facing runtime logs must not mix display-only task statistics into model context"
    );
    let display_conversation_state =
        vulcan_workspace_store::workspace_store::read_segmented_session_display_conversation_state(
            &task_dir,
        )
        .expect("task display transcript should replay")
        .expect("task display transcript should exist");
    assert!(
        display_conversation_state
            .records
            .iter()
            .any(|record| record.body == "review complete")
    );
    assert!(
        display_conversation_state
            .records
            .iter()
            .any(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
    );
    let copied_task_dir = tempfile::tempdir().expect("copied task directory");
    copy_directory_tree_for_test(&task_dir, copied_task_dir.path());
    let copied_runtime_conversation_state =
        vulcan_workspace_store::workspace_store::read_segmented_session_runtime_conversation_state(
            copied_task_dir.path(),
        )
        .expect("copied task runtime log should read")
        .expect("copied task runtime log should exist");
    assert!(
        copied_runtime_conversation_state
            .messages
            .iter()
            .any(|message| message.id == "assistant-1"),
        "copied TaskSession directory should load through the main runtime reader without its parent directory"
    );
    let copied_display_conversation_state =
        vulcan_workspace_store::workspace_store::read_segmented_session_display_conversation_state(
            copied_task_dir.path(),
        )
        .expect("copied task display transcript should replay")
        .expect("copied task display transcript should exist");
    assert!(
        copied_display_conversation_state
            .records
            .iter()
            .any(|record| record.body == "review complete"),
        "copied TaskSession directory should load through the main display reader without its parent directory"
    );
    assert!(
        copied_display_conversation_state
            .records
            .iter()
            .any(|record| record.record_kind == SessionMessageRecordKind::TurnStats),
        "copied TaskSession directory should keep turn stats through the main display reader"
    );
    let copied_task_segment_text =
        std::fs::read_to_string(copied_task_dir.path().join("s001.jsonl"))
            .expect("copied task segment should read after main display replay");
    assert!(
        copied_task_segment_text.lines().any(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some("subagent_task_meta")
        }),
        "main display replay must not repair away the TaskSession metadata event"
    );

    context.compaction_anchor = Some(String::from("task-summary-existing"));
    context.context_budget_fingerprint = Some(String::from("budget-existing"));
    store
        .save_context(context)
        .expect("existing compacted context should store");

    let second_outcome = TaskExecutionOutcome {
        final_output_text: String::from("review complete again"),
        ..outcome.clone()
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &second_outcome)
        .expect("second task outcome should store");
    let preserved_context = store
        .load_context(SessionNodeId::from(task_id))
        .expect("preserved context should load")
        .expect("preserved context should exist");
    assert_eq!(
        preserved_context.compaction_anchor.as_deref(),
        Some("task-summary-existing")
    );
    assert_eq!(
        preserved_context.context_budget_fingerprint.as_deref(),
        Some("budget-existing")
    );

    let task_events = store
        .load_events(SessionNodeId::from(task_id))
        .expect("task events should load");
    assert!(
        task_events
            .iter()
            .any(|event| event.kind == SessionEventKind::TaskTerminal)
    );
    let parent_events = store
        .list_task_invocation_events(ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        })
        .expect("parent task events should load");
    let completed_parent_event = parent_events
        .iter()
        .find(|event| event.kind == TaskInvocationEventKind::TaskCompleted)
        .expect("completed parent event should exist");
    assert!(
        completed_parent_event
            .payload
            .get("subagent_mailbox_event")
            .is_some(),
        "parent event payload should expose the subagent mailbox event"
    );
    assert_eq!(
        completed_parent_event.payload["subagent_mailbox_event"]["kind"],
        "completed"
    );
    let root_metadata = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    ))
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["line_count"], 7);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("agent-review")
        .expect("completed registry entry should exist");
    assert_eq!(entry.segments.len(), 1);
    assert_eq!(entry.segments[0].line_count, 7);
}

/// Verifies cloned task contexts retain runtime messages but drop inherited parent display records.
/// 验证克隆任务上下文会保留运行时消息，但会丢弃继承的父级显示记录。
#[test]
fn store_task_outcome_scopes_cloned_display_records_to_task_prompt() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("clone-child"),
        prompt: String::from("child prompt"),
        description: Some(String::from("Clone child")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("child prompt"),
        display_title: String::from("clone-child"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let resolved_config = test_resolved_config(&controller_config, &invocation.invocation_config);
    let provider_usage = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(20),
        output_tokens: Some(5),
        reasoning_tokens: Some(1),
        total_tokens: Some(26),
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
    };
    let snapshot = CoreConversationSnapshot {
        messages: vec![
            vulcan_core::types::ConversationMessage {
                id: String::from("parent-user-100"),
                payload: vulcan_core::types::ConversationPayload::UserText {
                    text: String::from("parent prompt"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("parent-assistant-101"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("parent answer"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("child-user-200"),
                payload: vulcan_core::types::ConversationPayload::UserText {
                    text: String::from("child prompt"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("child-assistant-201"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("child answer"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-child")),
                    provider_message_state: None,
                    usage: Some(provider_usage.clone()),
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            vulcan_core::types::ConversationRecord {
                id: String::from("parent-user-record"),
                role: vulcan_core::types::TranscriptRole::User,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("User"),
                body: String::from("parent prompt"),
                metadata: Some(json!({ "canonical_user_text": "parent prompt" })),
                created_at: String::from("100"),
            },
            vulcan_core::types::ConversationRecord {
                id: String::from("parent-assistant-record"),
                role: vulcan_core::types::TranscriptRole::Assistant,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("Assistant"),
                body: String::from("parent answer"),
                metadata: None,
                created_at: String::from("101"),
            },
            vulcan_core::types::ConversationRecord {
                id: String::from("child-user-record"),
                role: vulcan_core::types::TranscriptRole::User,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("User"),
                body: String::from("child prompt"),
                metadata: Some(json!({ "canonical_user_text": "child prompt" })),
                created_at: String::from("200"),
            },
            vulcan_core::types::ConversationRecord {
                id: String::from("child-assistant-record"),
                role: vulcan_core::types::TranscriptRole::Assistant,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("Assistant"),
                body: String::from("child answer"),
                metadata: Some(json!({ "provider_usage": provider_usage.clone() })),
                created_at: String::from("201"),
            },
        ],
        continuation: Default::default(),
    };
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&provider_usage),
    );
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config,
        snapshot,
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("child-assistant-201")),
        final_output_text: String::from("child answer"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: vec![usage_segment],
    };

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let context = store
        .load_context(SessionNodeId::from(task_id))
        .expect("context should load")
        .expect("context should exist");
    let conversation_state: SessionConversationState =
        serde_json::from_value(context.conversation_state.clone())
            .expect("task context conversation state should deserialize");

    assert!(
        conversation_state
            .messages
            .iter()
            .any(|message| message.id == "parent-user-100"),
        "task runtime messages should retain inherited parent context"
    );
    assert!(
        conversation_state
            .records
            .iter()
            .all(|record| record.body != "parent prompt" && record.body != "parent answer"),
        "task display records should not retain inherited parent transcript rows"
    );
    assert_eq!(
        conversation_state
            .records
            .first()
            .map(|record| record.body.as_str()),
        Some("child prompt")
    );
}

/// Verifies explicit task usage fills missing token counts without losing record-derived provider calls.
/// 验证显式任务用量会补齐缺失 Token，并保留记录推导出的 Provider 调用次数。
#[test]
fn store_task_outcome_merges_explicit_usage_without_losing_record_calls() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("echo_123_agent"),
        prompt: String::from("reply 123"),
        description: Some(String::from("Echo task")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("reply 123"),
        display_title: String::from("echo_123_agent"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let resolved_config = test_resolved_config(&controller_config, &invocation.invocation_config);
    let provider_usage = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(42),
        output_tokens: Some(7),
        reasoning_tokens: Some(3),
        total_tokens: Some(52),
        cached_input_tokens: Some(11),
        cache_creation_input_tokens: None,
    };
    let snapshot = CoreConversationSnapshot {
        messages: vec![
            vulcan_core::types::ConversationMessage {
                id: String::from("user-1"),
                payload: vulcan_core::types::ConversationPayload::UserText {
                    text: String::from("reply 123"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("assistant-1"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("123"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("assistant-provider-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            vulcan_core::types::ConversationRecord {
                id: String::from("user-record-1"),
                role: vulcan_core::types::TranscriptRole::User,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("User"),
                body: String::from("reply 123"),
                metadata: Some(json!({
                    "canonical_text": "reply 123",
                    "canonical_user_text": "reply 123"
                })),
                created_at: String::from("1000"),
            },
            vulcan_core::types::ConversationRecord {
                id: String::from("assistant-record-1"),
                role: vulcan_core::types::TranscriptRole::Assistant,
                record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("Model output"),
                body: String::from("123"),
                metadata: None,
                created_at: String::from("1100"),
            },
        ],
        continuation: Default::default(),
    };
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&provider_usage),
    );
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config,
        snapshot,
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-1")),
        final_output_text: String::from("123"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: vec![usage_segment],
    };

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let context = store
        .load_context(SessionNodeId::from(task_id))
        .expect("context should load")
        .expect("context should exist");
    let conversation_state: SessionConversationState =
        serde_json::from_value(context.conversation_state.clone())
            .expect("task context conversation state should deserialize");
    let stats_record = conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("task context should include turn stats");
    let turn_stats: SessionTurnStats = serde_json::from_value(
        stats_record
            .metadata
            .as_ref()
            .expect("turn stats metadata should exist")["turn_stats"]
            .clone(),
    )
    .expect("turn stats should deserialize");

    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 42);
    assert_eq!(turn_stats.output_tokens, 7);
    assert_eq!(turn_stats.reasoning_tokens, 3);
    assert_eq!(turn_stats.total_tokens, 52);
    assert_eq!(turn_stats.cached_input_tokens, Some(11));
    assert_eq!(turn_stats.usage_segments.len(), 2);
    assert_eq!(
        turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(turn_stats.usage_segments[0].provider_call_count, 1);
    assert_eq!(
        turn_stats.usage_segments[1].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(turn_stats.usage_segments[1].provider_call_count, 0);
    assert_eq!(turn_stats.usage_segments[1].input_tokens, 42);
    assert_eq!(turn_stats.usage_groups.len(), 1);
    assert_eq!(
        turn_stats.usage_groups[0].kind,
        vulcan_workbench_model::SessionTurnUsageGroupKind::Regular
    );
    assert_eq!(turn_stats.usage_groups[0].input_tokens, 42);
}

/// Verifies terminal mailbox events are not published before segmented outcome persistence succeeds.
/// 验证分段结果持久化成功前不会提前发布终态 mailbox 事件。
#[test]
fn store_task_outcome_does_not_publish_mailbox_when_persistence_fails() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invalid_store_root = temp_dir.path().join("session-file");
    std::fs::write(&invalid_store_root, "not a directory").expect("write invalid store root");
    task_runtime
        .configure_task_session_store_root(root_session_id, invalid_store_root)
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("agent-review"),
        prompt: String::from("review this change"),
        description: Some(String::from("Review code")),
        subagent: Some(String::from("reviewer")),
        invocation_config: TaskInvocationConfig::main("review this change"),
        display_title: String::from("Review code (reviewer)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-1")),
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };

    let error = task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect_err("invalid store root should fail persistence");

    assert!(!error.trim().is_empty());
    assert!(
        !task_runtime
            .has_subagent_events_for_parent(SessionNodeId::from(root_session_id))
            .expect("mailbox pending check should succeed"),
        "terminal mailbox must not publish before persistence succeeds"
    );
    assert!(
        task_runtime
            .subagent_registry_snapshot()
            .expect("registry should read")
            .get("agent-review")
            .is_none(),
        "registry must not expose a terminal agent whose outcome failed to persist"
    );
    assert!(
        task_runtime
            .sessions
            .read()
            .expect("sessions should read")
            .get(&task_id)
            .is_none(),
        "in-memory sessions must not expose a terminal agent whose outcome failed to persist"
    );
}

/// Verifies running start state failures do not leave ghost resident sessions.
/// 验证运行态启动记录失败时不会留下幽灵常驻会话。
#[test]
fn store_running_task_does_not_commit_session_when_persistence_fails() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invalid_store_root = temp_dir.path().join("session-file");
    std::fs::write(&invalid_store_root, "not a directory").expect("write invalid store root");
    task_runtime
        .configure_task_session_store_root(root_session_id, invalid_store_root)
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("agent-review"),
        prompt: String::from("review this change"),
        description: Some(String::from("Review code")),
        subagent: Some(String::from("reviewer")),
        invocation_config: TaskInvocationConfig::main("review this change"),
        display_title: String::from("Review code (reviewer)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };

    let error = task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect_err("invalid store root should fail running persistence");

    assert!(!error.trim().is_empty());
    assert!(
        task_runtime
            .subagent_registry_snapshot()
            .expect("registry should read")
            .get("agent-review")
            .is_none(),
        "registry must not expose a running agent whose start state failed to persist"
    );
    assert!(
        task_runtime
            .sessions
            .read()
            .expect("sessions should read")
            .get(&task_id)
            .is_none(),
        "in-memory sessions must not expose a running agent whose start state failed to persist"
    );
}

/// Verifies running subagent start state is persisted once and mirrored into the native registry.
/// 验证运行中子代理的启动状态会持久化一次，并镜像到原生注册表。
#[test]
fn store_running_task_persists_start_metadata_and_registry_segments() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("image-worker-a"),
        prompt: String::from("generate one image"),
        description: Some(String::from("Generate image")),
        subagent: Some(String::from("image-agent")),
        invocation_config: TaskInvocationConfig::main("generate one image"),
        display_title: String::from("Generate image (image-agent)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };

    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("running task should store");

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(metadata.status, TaskSessionStatus::Running);
    let root_metadata = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    ))
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["status"], "running");
    assert_eq!(root_metadata["tasks"][0]["line_count"], 4);

    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("image-worker-a")
        .expect("running registry entry should exist");
    assert_eq!(entry.status, SubagentStatus::Running);
    assert_eq!(entry.segments.len(), 1);
    assert_eq!(entry.segments[0].line_count, 4);
    assert!(entry.result_summary.is_none());
}

/// Verifies persistence restore never downgrades an already resident running registry entry.
/// 验证持久化恢复不会降级已经常驻内存的运行中注册表条目。
#[tokio::test]
async fn list_agents_restore_preserves_resident_running_registry_entry() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("live-reviewer"),
        prompt: String::from("review live changes"),
        description: Some(String::from("Review live changes")),
        subagent: Some(String::from("code-reviewer")),
        invocation_config: TaskInvocationConfig::main("review live changes"),
        display_title: String::from("Review live changes (code-reviewer)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };

    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("running task should store");
    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should not downgrade resident running agents");
    assert_eq!(
        result.model_output["agents"][0]["agent_name"],
        "live-reviewer"
    );
    assert_eq!(result.model_output["agents"][0]["run_state"], "running");

    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("live-reviewer")
        .expect("running registry entry should remain resident");
    assert_eq!(entry.status, SubagentStatus::Running);

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(metadata.status, TaskSessionStatus::Running);
}

/// Verifies application shutdown stops active subagents and persists stopped metadata.
/// 验证应用关闭会停止活动子代理并持久化 stopped 元数据。
#[tokio::test]
async fn app_shutdown_stops_active_subagents_and_persists_metadata() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");

    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("shutdown-worker"),
        prompt: String::from("long running work"),
        description: Some(String::from("Shutdown worker")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("long running work"),
        display_title: String::from("Shutdown worker"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("running task should store");
    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    let _active_guard = task_runtime
        .install_active_task_run(task_id, pending_join.abort_handle())
        .expect("active task run should install");

    let stopped_task_ids = task_runtime
        .stop_active_tasks_for_app_shutdown()
        .expect("app shutdown should stop active tasks");

    assert_eq!(stopped_task_ids, vec![task_id]);
    assert!(
        pending_join
            .await
            .expect_err("shutdown-aborted task should cancel")
            .is_cancelled()
    );
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    assert_eq!(
        registry
            .get("shutdown-worker")
            .expect("shutdown worker should remain registered")
            .status,
        SubagentStatus::Stopped
    );
    assert_eq!(
        registry
            .get("shutdown-worker")
            .expect("shutdown worker should remain registered")
            .stop_reason
            .as_deref(),
        Some("app_shutdown")
    );
    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(metadata.status, TaskSessionStatus::Stopped);
    assert_eq!(metadata.stop_reason.as_deref(), Some("app_shutdown"));
    assert_eq!(
        metadata.last_result_summary.as_deref(),
        Some(STOPPED_TASK_RESULT_SUMMARY)
    );
}

/// Verifies a late running snapshot cannot reopen a completed persisted task.
/// 验证迟到的运行中快照不能重新打开已完成的持久化任务。
#[test]
fn late_running_snapshot_does_not_reopen_completed_task() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("completed-worker"),
        prompt: String::from("complete once"),
        description: Some(String::from("Completed worker")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("complete once"),
        display_title: String::from("Completed worker"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let completed_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-completed")),
        final_output_text: String::from("completed result"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &completed_outcome)
        .expect("completed outcome should store");

    let running_outcome = TaskExecutionOutcome {
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        ..completed_outcome.clone()
    };
    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("late running update should be ignored");

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(metadata.status, TaskSessionStatus::Completed);
    assert_eq!(
        metadata.last_result_summary.as_deref(),
        Some("completed result")
    );
    let root_metadata = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    ))
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["status"], "completed");
    let sessions = task_runtime
        .sessions
        .read()
        .expect("task sessions should read");
    let record = sessions
        .get(&task_id)
        .expect("completed record should remain present");
    assert_eq!(record.last_status, "completed");
    assert_eq!(record.last_result_text, "completed result");
    drop(sessions);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("completed-worker")
        .expect("completed worker should remain registered");
    assert_eq!(entry.status, SubagentStatus::Completed);
    assert_eq!(entry.result_summary.as_deref(), Some("completed result"));
}

/// Verifies a late running snapshot cannot reopen a failed persisted task.
/// 验证迟到的运行中快照不能重新打开已失败的持久化任务。
#[test]
fn late_running_snapshot_does_not_reopen_failed_task() {
    let temp_dir = tempfile::tempdir().expect("temp task-session root");
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    write_test_segmented_root_metadata(temp_dir.path(), root_session_id);
    task_runtime
        .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
        .expect("store root should configure");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("failed-worker"),
        prompt: String::from("fail once"),
        description: Some(String::from("Failed worker")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("fail once"),
        display_title: String::from("Failed worker"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    task_runtime
        .mark_task_failure(root_session_id, &invocation, "failed result")
        .expect("failed outcome should store");
    let controller_config = build_test_controller_config("Base prompt");
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &invocation.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("late running update should be ignored");

    let store = SegmentedTaskSessionStore::new(temp_dir.path());
    let metadata = store
        .load_task_session(SessionNodeId::from(task_id))
        .expect("metadata should load")
        .expect("metadata should exist");
    assert_eq!(metadata.status, TaskSessionStatus::Failed);
    assert_eq!(metadata.last_error.as_deref(), Some("failed result"));
    let root_metadata = serde_json::from_reader::<_, Value>(BufReader::new(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    ))
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["status"], "failed");
    let sessions = task_runtime
        .sessions
        .read()
        .expect("task sessions should read");
    let record = sessions
        .get(&task_id)
        .expect("failed record should remain present");
    assert_eq!(record.last_status, "failed");
    assert_eq!(record.last_result_text, "failed result");
    drop(sessions);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("failed-worker")
        .expect("failed worker should remain registered");
    assert_eq!(entry.status, SubagentStatus::Failed);
    assert_eq!(entry.result_summary.as_deref(), Some("failed result"));
}

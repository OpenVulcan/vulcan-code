use super::*;

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
    let mut metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        parent.clone(),
        "agent-review",
        "Agent review",
        resolved_config.requested_config,
        resolved_config.effective_config,
        1,
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
    assert_eq!(controller_config.system_prompt, "Persisted system prompt");
    assert_eq!(request.task_id, task_id);
    assert!(request.is_resumed);
}

/// Verifies list_agents restores persisted active task sessions as open stopped agents.
/// 验证 list_agents 会将持久化的活动子任务恢复为 open/stopped 子代理。
#[tokio::test]
async fn list_agents_restores_persisted_active_agent_as_stopped_without_runtime_handle() {
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
    let mut metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "persisted-review",
        "Persisted review",
        resolved_config.requested_config,
        resolved_config.effective_config,
        1,
        100,
    );
    metadata.status = TaskSessionStatus::Running;
    store
        .create_task_session(metadata)
        .expect("running task metadata should store");

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should restore persisted subagents");
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
async fn list_agents_restores_persisted_completed_agent_as_open() {
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
    let mut metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "completed-review",
        "Completed review",
        resolved_config.requested_config,
        resolved_config.effective_config,
        1,
        100,
    );
    metadata.status = TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("persisted completed result"));
    store
        .create_task_session(metadata)
        .expect("completed task metadata should store");

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should restore persisted completed subagents");

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
    let mut metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "persisted-worker",
        "Persisted worker",
        resolved_config.requested_config,
        resolved_config.effective_config,
        1,
        100,
    );
    metadata.status = TaskSessionStatus::Running;
    store
        .create_task_session(metadata)
        .expect("running task metadata should store");

    let close_result = task_runtime
        .invoke_close_agent(root_session_id, json!({ "target": "persisted-worker" }))
        .await
        .expect("close_agent should close restored persisted agent");
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

    let late_invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
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
        diagnostic_list.model_output["agents"][0]["lifecycle_state"],
        "closed"
    );

    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_task_id = Arc::new(std::sync::Mutex::new(None));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_task_id_for_handler = Arc::clone(&captured_task_id);
    let _guard = fresh_runtime.install_backend(
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
    );
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
    let mut metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        "image-generator-2",
        "Image generator 2",
        resolved_config.requested_config,
        resolved_config.effective_config,
        1,
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
    let _guard = task_runtime.install_backend(
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
    );

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
    let metadata = TaskSessionMetadata::new(
        SessionNodeId::from(root_session_id),
        SessionNodeId::from(task_id),
        parent,
        "agent-review",
        "Agent review",
        SessionRequestedConfig::default(),
        SessionEffectiveConfig::default(),
        1,
        100,
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
        .expect("persisted snapshot should load");

    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].id, "assistant-from-file");
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
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config,
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-1")),
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

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
            .any(|event| event.kind == SessionEventKind::TaskFeedback)
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
    let root_metadata = serde_json::from_reader::<_, Value>(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    )
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["line_count"], 3);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("agent-review")
        .expect("completed registry entry should exist");
    assert_eq!(entry.segments.len(), 1);
    assert_eq!(entry.segments[0].line_count, 3);
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
    let root_metadata = serde_json::from_reader::<_, Value>(
        std::fs::File::open(temp_dir.path().join("metadata.json"))
            .expect("root metadata should open"),
    )
    .expect("root metadata should parse");
    assert_eq!(root_metadata["tasks"][0]["status"], "running");
    assert_eq!(root_metadata["tasks"][0]["line_count"], 2);

    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get("image-worker-a")
        .expect("running registry entry should exist");
    assert_eq!(entry.status, SubagentStatus::Running);
    assert_eq!(entry.segments.len(), 1);
    assert_eq!(entry.segments[0].line_count, 2);
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
    let _active_guard = task_runtime.install_active_task_run(task_id, pending_join.abort_handle());

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

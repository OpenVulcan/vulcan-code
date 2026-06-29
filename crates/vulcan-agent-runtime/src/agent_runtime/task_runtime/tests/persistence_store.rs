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

use super::*;

/// Verifies the task backend fails loudly when the shared runtime bridge did not register a parent execution context.
/// 验证共享运行时桥未注册父级执行上下文时，任务后端会明确失败。
#[tokio::test]
async fn build_task_execution_handler_rejects_missing_shared_context() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let (sender, _receiver) = unbounded_channel::<Result<ProviderStreamDelta, String>>();
    let handler = build_task_execution_handler(
        Arc::clone(&task_runtime),
        build_test_controller_config("Base prompt"),
        sender,
    );
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

    let error = handler(invocation)
        .await
        .expect_err("missing shared context should fail");

    assert!(error.contains("shared runtime execution context"));
    assert!(error.contains("agent-review"));
}

/// Verifies stopping one task aborts its active descendant task tree without touching sibling tasks.
/// 验证停止单个任务会中止其活动后代任务树，并且不会影响旁支任务。
#[tokio::test]
async fn stop_task_tree_aborts_active_descendants_without_siblings() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let parent_task_id = Uuid::new_v4();
    let child_task_id = Uuid::new_v4();
    let grandchild_task_id = Uuid::new_v4();
    let sibling_task_id = Uuid::new_v4();

    {
        let mut sessions = task_runtime
            .sessions
            .write()
            .expect("task session registry should be writable");
        sessions.insert(
            parent_task_id,
            build_task_session_record(
                parent_task_id,
                String::from("parent"),
                root_session_id,
                None,
                1,
                None,
                None,
                TaskInvocationConfig::main("parent prompt"),
                String::from("parent"),
                None,
                CoreConversationSnapshot::default(),
                String::from("parent prompt"),
                String::from("running"),
            ),
        );
        sessions.insert(
            child_task_id,
            build_task_session_record(
                child_task_id,
                String::from("child"),
                root_session_id,
                Some(parent_task_id),
                2,
                None,
                None,
                TaskInvocationConfig::main("child prompt"),
                String::from("child"),
                None,
                CoreConversationSnapshot::default(),
                String::from("child prompt"),
                String::from("running"),
            ),
        );
        sessions.insert(
            grandchild_task_id,
            build_task_session_record(
                grandchild_task_id,
                String::from("grandchild"),
                root_session_id,
                Some(child_task_id),
                3,
                None,
                None,
                TaskInvocationConfig::main("grandchild prompt"),
                String::from("grandchild"),
                None,
                CoreConversationSnapshot::default(),
                String::from("grandchild prompt"),
                String::from("running"),
            ),
        );
        sessions.insert(
            sibling_task_id,
            build_task_session_record(
                sibling_task_id,
                String::from("sibling"),
                root_session_id,
                None,
                1,
                None,
                None,
                TaskInvocationConfig::main("sibling prompt"),
                String::from("sibling"),
                None,
                CoreConversationSnapshot::default(),
                String::from("sibling prompt"),
                String::from("running"),
            ),
        );
    }

    let parent_join = tokio::spawn(async { std::future::pending::<()>().await });
    let child_join = tokio::spawn(async { std::future::pending::<()>().await });
    let grandchild_join = tokio::spawn(async { std::future::pending::<()>().await });
    let sibling_join = tokio::spawn(async { std::future::pending::<()>().await });
    {
        let mut active_runs = task_runtime
            .active_task_runs
            .write()
            .expect("active task registry should be writable");
        active_runs.insert(parent_task_id, parent_join.abort_handle());
        active_runs.insert(child_task_id, child_join.abort_handle());
        active_runs.insert(grandchild_task_id, grandchild_join.abort_handle());
        active_runs.insert(sibling_task_id, sibling_join.abort_handle());
    }

    let stopped_task_ids = task_runtime
        .stop_task_tree(parent_task_id)
        .expect("task tree should stop");
    let stopped_task_ids = stopped_task_ids.into_iter().collect::<BTreeSet<_>>();

    assert_eq!(
        stopped_task_ids,
        BTreeSet::from([parent_task_id, child_task_id, grandchild_task_id])
    );
    {
        let sessions = task_runtime
            .sessions
            .read()
            .expect("task session registry should be readable");
        for task_id in [parent_task_id, child_task_id, grandchild_task_id] {
            let record = sessions
                .get(&task_id)
                .expect("stopped task record should still exist");
            assert_eq!(record.last_status, "stopped");
            assert_eq!(record.last_result_text, STOPPED_TASK_RESULT_SUMMARY);
        }
        assert_eq!(
            sessions
                .get(&sibling_task_id)
                .expect("sibling task record should still exist")
                .last_status,
            "running"
        );
    }
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("subagent registry should be readable");
    assert_eq!(
        registry.get("parent").expect("parent should mirror").status,
        SubagentStatus::Stopped
    );
    assert_eq!(
        registry.get("child").expect("child should mirror").status,
        SubagentStatus::Stopped
    );
    assert_eq!(
        registry
            .get("grandchild")
            .expect("grandchild should mirror")
            .status,
        SubagentStatus::Stopped
    );
    assert!(
        registry.get("sibling").is_none(),
        "sibling should not be touched by the stopped subtree"
    );
    assert!(
        parent_join
            .await
            .expect_err("parent task should be aborted")
            .is_cancelled()
    );
    assert!(
        child_join
            .await
            .expect_err("child task should be aborted")
            .is_cancelled()
    );
    assert!(
        grandchild_join
            .await
            .expect_err("grandchild task should be aborted")
            .is_cancelled()
    );
    assert!(!sibling_join.is_finished());
    sibling_join.abort();
}

/// Verifies that caller-facing task identifiers are normalized consistently before being stored or looked up.
/// 验证调用方面向任务标识在存储或查找前会被一致地规范化。
#[test]
fn normalize_public_task_identifier_trims_aliases_deterministically() {
    let first =
        normalize_public_task_identifier("explore_files").expect("alias task id should resolve");
    let second = normalize_public_task_identifier(" explore_files ")
        .expect("trimmed alias task id should resolve");

    assert_eq!(first, second);
}

/// Verifies that distinct caller-facing task identifiers allocate distinct internal task sessions on first creation.
/// 验证不同的调用方面向任务标识在首次创建时会分配不同的内部任务会话。
#[test]
fn resolve_task_session_id_keeps_distinct_aliases_unique() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let first = resolve_task_session_id(
        &task_runtime,
        parent_session_id,
        &test_task_args("count to 10", Some("count_10")),
    )
    .expect("first alias task id should resolve");
    let second = resolve_task_session_id(
        &task_runtime,
        parent_session_id,
        &test_task_args("count to 20", Some("count_20")),
    )
    .expect("second alias task id should resolve");
    let third = resolve_task_session_id(
        &task_runtime,
        parent_session_id,
        &test_task_args("count to 30", Some("count_30")),
    )
    .expect("third alias task id should resolve");

    assert_ne!(first.task_id, second.task_id);
    assert_ne!(second.task_id, third.task_id);
    assert_ne!(first.task_id, third.task_id);
    assert!(!first.is_resumed);
    assert!(!second.is_resumed);
    assert!(!third.is_resumed);
}

/// Verifies that repeated caller-facing task identifiers resume the same internal task session after the first creation.
/// 验证重复的调用方面向任务标识会在首次创建后续跑同一条内部任务会话。
#[test]
fn resolve_task_session_id_reuses_existing_public_task_identifier() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let first = resolve_task_session_id(
        &task_runtime,
        parent_session_id,
        &test_task_args("inspect readme", Some("research_readme")),
    )
    .expect("first alias task id should resolve");

    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(
            first.task_id,
            build_task_session_record(
                first.task_id,
                first.public_task_id.clone(),
                parent_session_id,
                None,
                1,
                None,
                None,
                TaskInvocationConfig::main("inspect readme"),
                String::from("research_readme"),
                None,
                CoreConversationSnapshot::default(),
                String::from("inspect readme"),
                String::from("created"),
            ),
        );

    let resumed = resolve_task_session_id(
        &task_runtime,
        parent_session_id,
        &test_task_args("inspect readme again", Some("research_readme")),
    )
    .expect("existing alias task id should resume");

    assert_eq!(resumed.task_id, first.task_id);
    assert_eq!(resumed.public_task_id, first.public_task_id);
    assert!(resumed.is_resumed);
}

/// Verifies completed task ids return cached results without restarting the backend.
/// 验证已完成任务标识会返回缓存结果，而不会重启后端。
#[tokio::test]
async fn invoke_task_returns_cached_completed_record_without_backend_restart() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let _guard = task_runtime.install_backend(
        parent_session_id,
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
    let mut record = build_task_session_record(
        task_id,
        String::from("image-generator-2"),
        parent_session_id,
        None,
        1,
        Some(String::from("Image generation")),
        Some(String::from("image-agent")),
        TaskInvocationConfig::main("generate image"),
        String::from("Image generation (image-agent)"),
        None,
        CoreConversationSnapshot::default(),
        String::from("generate image"),
        String::from("completed"),
    );
    record.last_result_text = String::from("cached image result");
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(task_id, record);

    let result = task_runtime
        .invoke_task(
            parent_session_id,
            json!({
                "prompt": "check existing image result",
                "task_id": "image-generator-2"
            }),
        )
        .await
        .expect("cached completed task should return");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);
    assert_eq!(result.model_output["cached"], true);
    assert_eq!(result.model_output["execution_status"], "completed");
    assert_eq!(result.model_output["result"], "cached image result");
    let host_result = result.host_result.expect("cached task host result");
    assert_eq!(host_result.payload["cached"], true);
    assert_eq!(
        host_result.payload[SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );
}

/// Verifies spawn_agent creates a fresh run instead of returning a cached completed task with the same public label.
/// 验证 spawn_agent 会创建全新运行，而不是返回同名已完成任务的缓存结果。
#[tokio::test]
async fn invoke_spawn_agent_allocates_fresh_task_when_public_id_exists() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let completed_task_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_task_id = Arc::new(std::sync::Mutex::new(None));
    let captured_public_task_id = Arc::new(std::sync::Mutex::new(None));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_task_id_for_handler = Arc::clone(&captured_task_id);
    let captured_public_task_id_for_handler = Arc::clone(&captured_public_task_id);
    let _guard = task_runtime.install_backend(
        parent_session_id,
        Arc::new(move |request: TaskExecutionRequest| {
            backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
            *captured_task_id_for_handler
                .lock()
                .expect("captured task id lock") = Some(request.task_id);
            *captured_public_task_id_for_handler
                .lock()
                .expect("captured public task id lock") = Some(request.public_task_id.clone());
            Box::pin(async move {
                let controller_config = build_test_controller_config("Fresh spawn prompt");
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
                    final_assistant_message_id: Some(String::from("assistant-fresh")),
                    final_output_text: String::from("fresh image result"),
                    execution_status: TaskExecutionStatus::Completed,
                    usage_segments: Vec::new(),
                })
            })
        }),
    );
    let mut completed_record = build_task_session_record(
        completed_task_id,
        String::from("image-generator-2"),
        parent_session_id,
        None,
        1,
        Some(String::from("Image generation")),
        Some(String::from("image-agent")),
        TaskInvocationConfig::main("generate image"),
        String::from("Image generation (image-agent)"),
        None,
        CoreConversationSnapshot::default(),
        String::from("generate image"),
        String::from("completed"),
    );
    completed_record.last_result_text = String::from("cached image result");
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(completed_task_id, completed_record);

    let result = task_runtime
        .invoke_spawn_agent(
            parent_session_id,
            json!({
                "prompt": "generate another image",
                "task_id": "image-generator-2"
            }),
        )
        .await
        .expect("spawn_agent should start a fresh task");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    let new_task_id = captured_task_id
        .lock()
        .expect("captured task id lock")
        .expect("backend should capture the fresh task id");
    assert_ne!(new_task_id, completed_task_id);
    let new_public_task_id = captured_public_task_id
        .lock()
        .expect("captured public task id lock")
        .clone()
        .expect("backend should capture the fresh public task id");
    assert_eq!(new_public_task_id, "image-generator-2-2");
    assert_eq!(result.model_output["cached"], Value::Null);
    assert_eq!(result.model_output["task_name"], "image-generator-2-2");
    assert_eq!(result.model_output["message"], "Subagent scheduled.");
    assert!(result.model_output.get("result").is_none());
}

/// Verifies send_message queues while followup_task resumes the same idle child agent.
/// 验证 send_message 只入队，而 followup_task 会续跑同一个空闲子代理。
#[tokio::test]
async fn inter_agent_message_queue_only_then_followup_resumes_idle_agent() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        public_task_id: String::from("worker-a"),
        prompt: String::from("initial work"),
        description: Some(String::from("Worker A")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("initial work"),
        display_title: String::from("Worker A"),
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
        final_assistant_message_id: Some(String::from("assistant-initial")),
        final_output_text: String::from("initial result"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_task_outcome(parent_session_id, &invocation, &completed_outcome)
        .expect("completed task should store");

    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_prompt = Arc::new(std::sync::Mutex::new(None::<String>));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_prompt_for_handler = Arc::clone(&captured_prompt);
    let _guard = task_runtime.install_backend(
        parent_session_id,
        Arc::new(move |request: TaskExecutionRequest| {
            backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
            *captured_prompt_for_handler
                .lock()
                .expect("captured prompt lock") = Some(request.prompt.clone());
            Box::pin(async move {
                let controller_config = build_test_controller_config("Followup prompt");
                Ok(TaskExecutionOutcome {
                    task_id: request.task_id,
                    controller_config: controller_config.clone(),
                    resolved_config: test_resolved_config(
                        &controller_config,
                        &request.invocation_config,
                    ),
                    snapshot: CoreConversationSnapshot::default(),
                    provider_turns_executed: 0,
                    tool_calls_processed: 0,
                    final_assistant_message_id: None,
                    final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
                    execution_status: TaskExecutionStatus::Running,
                    usage_segments: Vec::new(),
                })
            })
        }),
    );

    let send_result = task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "queue this context"
            }),
        )
        .await
        .expect("send_message should queue");
    assert_eq!(send_result.model_output["ok"], true);
    assert_eq!(send_result.model_output["target"], "worker-a");
    assert_eq!(
        send_result.model_output[SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );
    assert_eq!(send_result.model_output["queued"], true);
    assert_eq!(send_result.model_output["trigger_turn"], false);
    assert_eq!(send_result.model_output["status"], "completed");
    assert_eq!(
        send_result
            .host_result
            .as_ref()
            .expect("send_message host result")
            .payload["message"],
        "queue this context"
    );
    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);

    let followup_result = task_runtime
        .invoke_followup_task(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "now continue"
            }),
        )
        .await
        .expect("followup_task should resume idle task");
    assert_eq!(followup_result.model_output["ok"], true);
    assert_eq!(followup_result.model_output["target"], "worker-a");
    assert_eq!(
        followup_result.model_output[SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );
    assert_eq!(followup_result.model_output["queued"], true);
    assert_eq!(followup_result.model_output["trigger_turn"], true);
    assert_eq!(followup_result.model_output["status"], "completed");
    assert_eq!(
        followup_result
            .host_result
            .as_ref()
            .expect("followup_task host result")
            .payload["message"],
        "now continue"
    );
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    let prompt = captured_prompt
        .lock()
        .expect("captured prompt lock")
        .clone()
        .expect("backend should capture prompt");
    assert!(prompt.contains("queue this context"));
    assert!(prompt.contains("now continue"));
    assert!(prompt.contains("send_message"));
    assert!(prompt.contains("followup_task"));
    let resumed_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after followup");
    let resumed_entry = resumed_registry
        .get("worker-a")
        .expect("worker entry should remain known");
    assert_eq!(resumed_entry.status, SubagentStatus::Running);
}

/// Verifies child agents can send queue-only messages to root while followup_task rejects root.
/// 验证子代理可以向 root 发送仅入队消息，而 followup_task 会拒绝 root。
#[tokio::test]
async fn send_message_to_root_enters_parent_mailbox_and_followup_rejects_root() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let child_session_id = Uuid::new_v4();
    let record = build_task_session_record(
        child_session_id,
        String::from("worker-a"),
        parent_session_id,
        None,
        1,
        Some(String::from("Worker A")),
        None,
        TaskInvocationConfig::main("initial work"),
        String::from("Worker A"),
        None,
        CoreConversationSnapshot::default(),
        String::from("initial work"),
        String::from("running"),
    );
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(child_session_id, record);

    let result = task_runtime
        .invoke_send_message(
            child_session_id,
            json!({
                "target": "root",
                "message": "child progress"
            }),
        )
        .await
        .expect("send_message to root should succeed");
    assert_eq!(result.model_output["ok"], true);
    assert_eq!(result.model_output["target"], "root");
    assert_eq!(
        result.model_output["sender_session_id"],
        child_session_id.to_string()
    );
    assert_eq!(
        result.model_output["parent_session_id"],
        parent_session_id.to_string()
    );
    assert_eq!(result.model_output["queued"], true);
    assert_eq!(result.model_output["trigger_turn"], false);
    assert_eq!(
        result.host_result.as_ref().expect("host result").kind,
        "subagent_message"
    );

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("parent mailbox should drain");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, SubagentMailboxEventKind::Message);
    assert_eq!(events[0].payload["result_text"], "child progress");

    let error = task_runtime
        .invoke_followup_task(
            child_session_id,
            json!({
                "target": "root",
                "message": "continue root"
            }),
        )
        .await
        .expect_err("followup_task should reject root");
    assert!(error.contains("root"));
}

/// Verifies task invocation relays a terminal result when a custom backend chooses to return one.
/// 验证自定义后端选择返回终态结果时，task 调用会原样转发该结果。
#[tokio::test]
async fn invoke_task_relays_backend_terminal_result_when_backend_waits() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let (release_sender, release_receiver) = tokio::sync::oneshot::channel::<()>();
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel::<()>();
    let release_receiver = Arc::new(std::sync::Mutex::new(Some(release_receiver)));
    let entered_sender = Arc::new(std::sync::Mutex::new(Some(entered_sender)));
    let release_receiver_for_handler = Arc::clone(&release_receiver);
    let entered_sender_for_handler = Arc::clone(&entered_sender);
    let _guard = task_runtime.install_backend(
        parent_session_id,
        Arc::new(move |request: TaskExecutionRequest| {
            let release_receiver = Arc::clone(&release_receiver_for_handler);
            let entered_sender = Arc::clone(&entered_sender_for_handler);
            Box::pin(async move {
                if let Some(sender) = entered_sender
                    .lock()
                    .expect("test entered sender lock")
                    .take()
                {
                    let _ = sender.send(());
                }
                let receiver = release_receiver
                    .lock()
                    .expect("test release receiver lock")
                    .take()
                    .expect("test release receiver should exist");
                receiver.await.expect("test release should arrive");
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
                    final_output_text: String::from("terminal backend result"),
                    execution_status: TaskExecutionStatus::Completed,
                    usage_segments: Vec::new(),
                })
            })
        }),
    );
    let invoking_runtime = Arc::clone(&task_runtime);
    let invoke_task = tokio::spawn(async move {
        invoking_runtime
            .invoke_task(
                parent_session_id,
                json!({
                    "prompt": "generate one image",
                    "task_id": "image-worker-a",
                    "description": "Generate image",
                    "subagent": "image-agent"
                }),
            )
            .await
    });

    entered_receiver
        .await
        .expect("backend should be entered before release");
    tokio::task::yield_now().await;
    assert!(
        !invoke_task.is_finished(),
        "custom backend future should remain in control until it returns an outcome"
    );
    release_sender.send(()).expect("release backend");
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), invoke_task)
        .await
        .expect("task invocation should finish quickly after terminal backend output")
        .expect("task join should succeed")
        .expect("task invocation should succeed");

    assert_eq!(result.model_output["execution_status"], "completed");
    assert_eq!(result.model_output["result"], "terminal backend result");
}

/// Verifies a completed detached task stream replays buffered terminal events and closes without a slow poll loop.
/// 验证已完成的分离任务流会立即回放缓冲终态事件并关闭，不进入慢轮询队列。
#[tokio::test]
async fn detached_task_event_stream_replays_buffered_terminal_and_closes() {
    let task_runtime = TaskRuntime::new();
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let terminal_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "Subagent finished",
        "terminal result",
    );
    task_runtime.publish_detached_task_event(
        task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id,
            event: terminal_event,
        },
    );
    task_runtime.close_detached_task_event_stream(task_id);

    let mut stream = Box::pin(task_runtime.subscribe_detached_task_events(task_id));
    let first_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("buffered terminal event should replay immediately")
        .expect("buffered terminal event should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = first_event else {
        panic!("expected parent terminal runtime event");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateFinished);
    let next_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("closed stream should finish immediately after replay");
    assert!(next_event.is_none());
}

/// Verifies repeated subscriptions to an already completed task do not create a fresh empty stream.
/// 验证重复订阅已完成任务时不会创建新的空开放流。
#[tokio::test]
async fn detached_task_event_stream_closes_for_terminal_record_without_buffer() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(
            task_id,
            build_task_session_record(
                task_id,
                String::from("image-worker-a"),
                root_session_id,
                None,
                1,
                Some(String::from("生成图片")),
                Some(String::from("image-agent")),
                TaskInvocationConfig::main("generate image"),
                String::from("生成图片 (image-agent)"),
                None,
                CoreConversationSnapshot::default(),
                String::from("generate image"),
                String::from("completed"),
            ),
        );

    let mut stream = Box::pin(task_runtime.subscribe_detached_task_events(task_id));
    let event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("terminal record stream should close immediately");
    assert!(event.is_none());
}

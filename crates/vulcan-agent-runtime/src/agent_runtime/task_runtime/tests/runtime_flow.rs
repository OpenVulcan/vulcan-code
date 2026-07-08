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

    let error = handler(invocation)
        .await
        .expect_err("missing shared context should fail");

    assert!(error.contains("shared runtime execution context"));
    assert!(error.contains("agent-review"));
}

/// Verifies a child run is not spawned when the parent live stream is already closed.
/// 验证父级实时流已经关闭时不会派生子代理运行。
#[tokio::test]
async fn build_task_execution_handler_rejects_closed_parent_live_stream_without_orphan_run() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        ExecutionPreparationRequest {
            session_id: parent_session_id,
            execution_scope: ExecutionScope::MainSession {
                session_id: SessionNodeId::from(parent_session_id),
            },
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
            execution_agent_id: Some(String::from("main")),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            task_session_store_root: None,
            session_title: String::from("Parent session"),
            workspace_path: String::from("D:\\openvulcan\\test-project"),
            prompt: String::from("parent prompt"),
            provider_visible_agent_messages: Vec::new(),
            conversation_state: SessionConversationState::default(),
            context_compaction_policy: None,
            context_budget_override: None,
            execution_mode: String::from("build"),
            temperature: None,
            reasoning_effort: ReasoningEffortLevel::High,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            tool_exposure_override: ToolExposureSnapshot::default(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            tool_request_hidden_tool_ids: Vec::new(),
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
            session_prompt_skill_overrides: BTreeMap::new(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            controller_runtime: None,
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        },
    );
    let _context_guard = task_runtime
        .install_execution_context(parent_session_id, context)
        .expect("shared execution context should install");
    let (sender, receiver) = unbounded_channel::<Result<ProviderStreamDelta, String>>();
    drop(receiver);
    let handler = build_task_execution_handler(
        Arc::clone(&task_runtime),
        build_test_controller_config("Base prompt"),
        sender,
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
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

    let error = handler(invocation)
        .await
        .expect_err("closed parent live stream should reject child start");

    assert!(error.contains("parent live stream closed"));
    assert!(error.contains("agent-review"));
    assert!(
        !task_runtime
            .task_has_active_or_starting_run(task_id)
            .expect("active task registry should be readable"),
        "closed parent stream must not leave an active child run"
    );
    assert!(
        !task_runtime
            .detached_task_event_streams
            .read()
            .expect("detached stream registry should be readable")
            .contains_key(&task_id),
        "closed parent stream must not leave a detached event stream"
    );
    assert!(
        !task_runtime
            .sessions
            .read()
            .expect("task session registry should be readable")
            .contains_key(&task_id),
        "closed parent stream must not store a synthetic task session"
    );
}

/// Verifies stopping one task aborts its active descendant task tree without touching sibling tasks.
/// 验证停止单个任务会中止其活动后代任务树，并且不会影响旁支任务。
#[tokio::test]
async fn stop_task_tree_aborts_active_descendants_without_siblings() {
    let task_runtime = Arc::new(TaskRuntime::new());
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
            build_task_session_record(TaskSessionRecordInit {
                task_id: parent_task_id,
                public_task_id: String::from("parent"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("parent prompt"),
                display_title: String::from("parent"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("parent prompt"),
                last_status: String::from("running"),
            }),
        );
        sessions.insert(
            child_task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id: child_task_id,
                public_task_id: String::from("child"),
                origin_session_id: root_session_id,
                parent_task_id: Some(parent_task_id),
                depth: 2,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("child prompt"),
                display_title: String::from("child"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("child prompt"),
                last_status: String::from("running"),
            }),
        );
        sessions.insert(
            grandchild_task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id: grandchild_task_id,
                public_task_id: String::from("grandchild"),
                origin_session_id: root_session_id,
                parent_task_id: Some(child_task_id),
                depth: 3,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("grandchild prompt"),
                display_title: String::from("grandchild"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("grandchild prompt"),
                last_status: String::from("running"),
            }),
        );
        sessions.insert(
            sibling_task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id: sibling_task_id,
                public_task_id: String::from("sibling"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("sibling prompt"),
                display_title: String::from("sibling"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("sibling prompt"),
                last_status: String::from("running"),
            }),
        );
    }

    let parent_join = tokio::spawn(async { std::future::pending::<()>().await });
    let child_join = tokio::spawn(async { std::future::pending::<()>().await });
    let grandchild_join = tokio::spawn(async { std::future::pending::<()>().await });
    let sibling_join = tokio::spawn(async { std::future::pending::<()>().await });
    let _parent_guard = task_runtime
        .install_active_task_run(parent_task_id, parent_join.abort_handle())
        .expect("parent active run should install");
    let _child_guard = task_runtime
        .install_active_task_run(child_task_id, child_join.abort_handle())
        .expect("child active run should install");
    let _grandchild_guard = task_runtime
        .install_active_task_run(grandchild_task_id, grandchild_join.abort_handle())
        .expect("grandchild active run should install");
    let _sibling_guard = task_runtime
        .install_active_task_run(sibling_task_id, sibling_join.abort_handle())
        .expect("sibling active run should install");

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

/// Verifies an older active-run guard cannot unregister a newer run for the same child task.
/// 验证旧活动运行守卫不能注销同一子任务的新运行。
#[tokio::test]
async fn active_task_run_guard_preserves_newer_same_task_run() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let task_id = Uuid::new_v4();
    let first_join = tokio::spawn(async { std::future::pending::<()>().await });
    let second_join = tokio::spawn(async { std::future::pending::<()>().await });
    let first_guard = task_runtime
        .install_active_task_run(task_id, first_join.abort_handle())
        .expect("first active run should install");
    let second_guard = task_runtime
        .install_active_task_run(task_id, second_join.abort_handle())
        .expect("second active run should install");

    drop(first_guard);
    assert!(
        task_runtime
            .is_task_running(task_id)
            .expect("active task lookup should succeed"),
        "dropping the older guard must not remove the newer active run"
    );

    drop(second_guard);
    assert!(
        !task_runtime
            .is_task_running(task_id)
            .expect("active task lookup should succeed"),
        "dropping the current guard should remove the active run"
    );
    first_join.abort();
    second_join.abort();
}

/// Verifies active-run registration returns a diagnostic instead of panicking on lock poisoning.
/// 验证活动运行注册表锁污染时会返回诊断错误而不是 panic。
#[tokio::test]
async fn install_active_task_run_reports_poisoned_registry() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .active_task_runs
            .write()
            .expect("active task registry should lock before poisoning");
        panic!("poison active task registry for test");
    }));
    assert!(poison_result.is_err());

    let task_id = Uuid::new_v4();
    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    let install_result = task_runtime.install_active_task_run(task_id, pending_join.abort_handle());
    pending_join.abort();

    let error = match install_result {
        Ok(_) => panic!("poisoned active task registry should reject installation"),
        Err(error) => error,
    };
    assert!(error.contains("active task registry is locked"));
}

/// Verifies detached stream creation returns a diagnostic instead of panicking on lock poisoning.
/// 验证分离事件流锁污染时会返回诊断错误而不是 panic。
#[test]
fn open_detached_task_event_stream_reports_poisoned_registry() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .detached_task_event_streams
            .write()
            .expect("detached task stream registry should lock before poisoning");
        panic!("poison detached task stream registry for test");
    }));
    assert!(poison_result.is_err());

    let error = task_runtime
        .open_detached_task_event_stream_for_run(Uuid::new_v4())
        .expect_err("poisoned detached stream registry should reject stream open");
    assert!(error.contains("detached task stream registry is locked"));
}

/// Verifies backend registration returns a diagnostic instead of panicking on lock poisoning.
/// 验证后端注册表锁污染时会返回诊断错误而不是 panic。
#[test]
fn install_backend_reports_poisoned_registry() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .backends
            .write()
            .expect("task backend registry should lock before poisoning");
        panic!("poison task backend registry for test");
    }));
    assert!(poison_result.is_err());

    let install_result = task_runtime.install_backend(
        Uuid::new_v4(),
        Arc::new(|_request: TaskExecutionRequest| {
            Box::pin(async move { Err(String::from("unused backend")) })
        }),
    );

    let error = match install_result {
        Ok(_) => panic!("poisoned task backend registry should reject installation"),
        Err(error) => error,
    };
    assert!(error.contains("task backend registry is locked"));
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
            build_task_session_record(TaskSessionRecordInit {
                task_id: first.task_id,
                public_task_id: first.public_task_id.clone(),
                origin_session_id: parent_session_id,
                parent_task_id: None,
                depth: 1,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("inspect readme"),
                display_title: String::from("research_readme"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("inspect readme"),
                last_status: String::from("created"),
            }),
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

/// Verifies persisted metadata keeps description and subagent profile identity separate when restored.
/// 验证持久化元数据恢复时会区分描述与子代理配置身份。
#[test]
fn persisted_metadata_restore_uses_profile_id_as_subagent_label() {
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let requested_config = SessionRequestedConfig {
        subagent_profile_id: Some(String::from("engineering/code-reviewer")),
        ..Default::default()
    };
    let effective_config = SessionEffectiveConfig {
        subagent_profile_id: Some(String::from("engineering/code-reviewer")),
        ..Default::default()
    };
    let mut metadata = TaskSessionMetadata::new(vulcan_task_session::TaskSessionMetadataInit {
        root_session_id: SessionNodeId::from(root_session_id),
        task_session_id: SessionNodeId::from(task_id),
        parent: ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        },
        public_task_id: String::from("review-worker"),
        display_title: String::from("Review code"),
        requested_config,
        effective_config,
        depth: 1,
        timestamp_ms: 100,
    });
    metadata.description = Some(String::from("Review the changed Rust files"));
    metadata.last_result_summary = Some(String::from("Review complete"));

    let record = task_session_record_from_persisted_metadata(
        &metadata,
        None,
        String::from("resume review"),
        TaskInvocationConfig::main("resume review"),
        None,
        None,
    )
    .expect("persisted metadata should restore");
    assert_eq!(
        record.description.as_deref(),
        Some("Review the changed Rust files")
    );
    assert_eq!(
        record.subagent.as_deref(),
        Some("engineering/code-reviewer")
    );

    let cached_result = build_cached_task_invocation_result_from_metadata(
        root_session_id,
        &ResolvedTaskSessionIdentifier {
            task_id,
            public_task_id: String::from("review-worker"),
            is_resumed: true,
            persisted_metadata: None,
        },
        &metadata,
        "resume review",
    );
    assert_eq!(
        cached_result.model_output["description"],
        "Review the changed Rust files"
    );
    assert_eq!(
        cached_result.model_output["subagent"],
        "engineering/code-reviewer"
    );
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
    let _guard = task_runtime
        .install_backend(
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
        )
        .expect("task backend should install");
    let mut record = build_task_session_record(TaskSessionRecordInit {
        task_id,
        public_task_id: String::from("image-generator-2"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Image generation")),
        subagent: Some(String::from("image-agent")),
        invocation_config: TaskInvocationConfig::main("generate image"),
        display_title: String::from("Image generation (image-agent)"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("generate image"),
        last_status: String::from("completed"),
    });
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

/// Verifies failed task ids return cached results without restarting the backend.
/// 验证已失败任务标识会返回缓存结果，而不会重启后端。
#[tokio::test]
async fn invoke_task_returns_cached_failed_record_without_backend_restart() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let _guard = task_runtime
        .install_backend(
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
        )
        .expect("task backend should install");
    let mut record = build_task_session_record(TaskSessionRecordInit {
        task_id,
        public_task_id: String::from("failed-reviewer"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Failed review")),
        subagent: Some(String::from("review-agent")),
        invocation_config: TaskInvocationConfig::main("review code"),
        display_title: String::from("Failed review (review-agent)"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("review code"),
        last_status: String::from("failed"),
    });
    record.last_result_text = String::from("cached failure result");
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(task_id, record);

    let result = task_runtime
        .invoke_task(
            parent_session_id,
            json!({
                "prompt": "check failed review result",
                "task_id": "failed-reviewer"
            }),
        )
        .await
        .expect("cached failed task should return");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);
    assert_eq!(result.model_output["cached"], true);
    assert_eq!(result.model_output["execution_status"], "failed");
    assert_eq!(result.model_output["result"], "cached failure result");
    let host_result = result.host_result.expect("cached task host result");
    assert_eq!(host_result.payload["cached"], true);
    assert_eq!(
        host_result.payload[SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );
}

/// Verifies a pre-start cancelled spawn does not persist a failed child-agent record or reserve the public id.
/// 验证启动前取消的子代理创建不会持久化失败记录，也不会占用公开标识。
#[tokio::test]
async fn invoke_spawn_agent_prestart_cancel_does_not_reserve_public_id() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let task_runtime_for_handler = Arc::clone(&task_runtime);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                let call_index = backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                let registry = task_runtime_for_handler
                    .subagent_registry_snapshot()
                    .expect("subagent registry should be readable during backend invocation");
                let registry_entry = registry
                    .list_all()
                    .into_iter()
                    .find(|entry| entry.session_id == SessionNodeId::from(request.task_id))
                    .expect("prepared task should be visible in the runtime registry");
                assert_eq!(registry_entry.agent_id, request.public_task_id);
                assert_eq!(registry_entry.status, SubagentStatus::Created);
                Box::pin(async move {
                    if call_index == 0 {
                        return Err(task_start_parent_stream_closed_error(
                            &request.public_task_id,
                        ));
                    }
                    let controller_config = build_test_controller_config("Retried spawn prompt");
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
                        final_assistant_message_id: Some(String::from("assistant-retry")),
                        final_output_text: String::from("retry completed"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");

    let first_error = task_runtime
        .invoke_spawn_agent(
            parent_session_id,
            json!({
                "prompt": "review once",
                "task_id": "retryable-agent"
            }),
        )
        .await
        .expect_err("pre-start cancellation should surface as an error");

    assert!(first_error.contains("parent live stream closed"));
    assert!(
        !first_error.contains("[task-prestart-cancelled]"),
        "internal pre-start marker should not leak to the caller"
    );
    assert!(
        task_runtime
            .sessions
            .read()
            .expect("task session registry should be readable")
            .is_empty(),
        "pre-start cancellation must not persist a failed task record"
    );
    assert!(
        task_runtime
            .subagent_registry_snapshot()
            .expect("subagent registry should be readable")
            .list_all()
            .is_empty(),
        "pre-start cancellation must not register an agent entry"
    );

    let retry_result = task_runtime
        .invoke_spawn_agent(
            parent_session_id,
            json!({
                "prompt": "review again",
                "task_id": "retryable-agent"
            }),
        )
        .await
        .expect("same public id should be reusable after pre-start cancellation");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 2);
    assert_eq!(retry_result.model_output["execution_status"], "completed");
    let records = task_runtime
        .sessions
        .read()
        .expect("task session registry should be readable");
    let retried_record = records
        .values()
        .find(|record| record.public_task_id == "retryable-agent")
        .expect("retried task record should exist");
    assert_eq!(retried_record.last_status, "completed");
    assert_eq!(retried_record.last_result_text, "retry completed");
}

/// Verifies spawn_agent rejects duplicate public labels instead of generating suffixes.
/// 验证 spawn_agent 会拒绝重复公开标识，而不是自动生成后缀。
#[tokio::test]
async fn invoke_spawn_agent_rejects_duplicate_public_id() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let completed_task_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |_request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let controller_config = build_test_controller_config("Fresh spawn prompt");
                    Ok(TaskExecutionOutcome {
                        task_id: Uuid::new_v4(),
                        controller_config: controller_config.clone(),
                        resolved_config: test_resolved_config(
                            &controller_config,
                            &TaskInvocationConfig::main("unused"),
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
        )
        .expect("task backend should install");
    let mut completed_record = build_task_session_record(TaskSessionRecordInit {
        task_id: completed_task_id,
        public_task_id: String::from("image-generator-2"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Image generation")),
        subagent: Some(String::from("image-agent")),
        invocation_config: TaskInvocationConfig::main("generate image"),
        display_title: String::from("Image generation (image-agent)"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("generate image"),
        last_status: String::from("completed"),
    });
    completed_record.last_result_text = String::from("cached image result");
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(completed_task_id, completed_record);

    let error = task_runtime
        .invoke_spawn_agent(
            parent_session_id,
            json!({
                "prompt": "generate another image",
                "task_id": "image-generator-2"
            }),
        )
        .await
        .expect_err("spawn_agent should reject duplicate child-agent names");

    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);
    assert!(error.contains("already exists"));
}

/// Verifies close_agent releases a public child-agent id for a new spawn.
/// 验证 close_agent 会释放公开子代理标识，允许后续重新创建。
#[tokio::test]
async fn invoke_spawn_agent_reuses_public_id_after_close_agent() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let closed_task_id = Uuid::new_v4();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_task_id = Arc::new(std::sync::Mutex::new(None));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_task_id_for_handler = Arc::clone(&captured_task_id);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                *captured_task_id_for_handler
                    .lock()
                    .expect("captured task id lock") = Some(request.task_id);
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
                        final_assistant_message_id: Some(String::from("assistant-reused")),
                        final_output_text: String::from("new reusable result"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");
    let closed_record = build_task_session_record(TaskSessionRecordInit {
        task_id: closed_task_id,
        public_task_id: String::from("reusable-agent"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Reusable agent")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("old work"),
        display_title: String::from("Reusable agent"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("old work"),
        last_status: String::from("completed"),
    });
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(closed_task_id, closed_record.clone());
    task_runtime
        .upsert_subagent_registry_entry(
            &closed_record,
            TaskSessionStatus::Completed,
            Some("old result"),
        )
        .expect("closed candidate should be registered");

    task_runtime
        .invoke_close_agent(parent_session_id, json!({ "target": "reusable-agent" }))
        .await
        .expect("close_agent should close the original agent");
    assert_eq!(
        task_runtime
            .sessions
            .read()
            .expect("task session registry should read")
            .get(&closed_task_id)
            .expect("closed record should remain for history")
            .last_status,
        "archived"
    );

    let result = task_runtime
        .invoke_spawn_agent(
            parent_session_id,
            json!({
                "prompt": "run fresh reusable work",
                "task_id": "reusable-agent"
            }),
        )
        .await
        .expect("spawn_agent should reuse a closed public id");
    let fresh_task_id = captured_task_id
        .lock()
        .expect("captured task id lock")
        .expect("backend should capture the fresh task id");
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    assert_ne!(fresh_task_id, closed_task_id);
    assert_eq!(result.model_output["task_name"], "reusable-agent");

    let parsed_args = parse_task_tool_arguments(json!({
        "prompt": "continue reusable work",
        "task_id": "reusable-agent"
    }))
    .expect("follow-up task args should parse");
    let resolved = resolve_task_session_id(&task_runtime, parent_session_id, &parsed_args)
        .expect("public lookup should resolve the fresh reusable agent");
    assert_eq!(resolved.task_id, fresh_task_id);
}

/// Verifies send_message queues and resumes the same idle child agent.
/// 验证 send_message 会排队并续跑同一个空闲子代理。
#[tokio::test]
async fn inter_agent_message_queue_resumes_idle_agent() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: String::from("test-execution"),
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
    let captured_snapshot = Arc::new(std::sync::Mutex::new(None::<CoreConversationSnapshot>));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_prompt_for_handler = Arc::clone(&captured_prompt);
    let captured_snapshot_for_handler = Arc::clone(&captured_snapshot);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                *captured_prompt_for_handler
                    .lock()
                    .expect("captured prompt lock") = Some(request.prompt.clone());
                *captured_snapshot_for_handler
                    .lock()
                    .expect("captured snapshot lock") = Some(request.snapshot.clone());
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
        )
        .expect("task backend should install");

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
    assert_eq!(send_result.model_output["delivery_mode"], "queue");
    assert_eq!(send_result.model_output["interrupted"], false);
    assert_eq!(send_result.model_output["delivery_status"], "running");
    assert_eq!(send_result.model_output["execution_status"], "running");
    assert!(send_result.model_output.get("status").is_none());
    assert!(send_result.model_output.get("result").is_none());
    assert_eq!(
        send_result
            .host_result
            .as_ref()
            .expect("send_message host result")
            .payload["message"],
        "queue this context"
    );
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    let prompt = captured_prompt
        .lock()
        .expect("captured prompt lock")
        .clone()
        .expect("backend should capture prompt");
    assert!(prompt.starts_with("<agent_message>"));
    assert!(prompt.contains("queue this context"));
    assert!(!prompt.contains("[Inter-agent messages]"));
    assert!(!prompt.contains("send_message"));
    let snapshot = captured_snapshot
        .lock()
        .expect("captured snapshot lock")
        .clone()
        .expect("backend should capture snapshot");
    assert!(snapshot.messages.iter().any(|message| {
        matches!(
            &message.payload,
            CoreConversationPayload::AgentMessage {
                message_type,
                task_name,
                sender,
                recipient,
                payload,
                trigger_turn,
                ..
            } if message_type == "MESSAGE"
                && task_name == "worker-a"
                && sender == &format!("parent:{parent_session_id}")
                && recipient == "worker-a"
                && payload == "queue this context"
                && *trigger_turn
        )
    }));
    let resumed_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after followup");
    let resumed_entry = resumed_registry
        .get("worker-a")
        .expect("worker entry should remain known");
    assert_eq!(resumed_entry.status, SubagentStatus::Running);
}

/// Verifies send_message returns a terminal child result when the idle child finishes synchronously.
/// 验证空闲子代理同步完成时 send_message 会返回终态子结果。
#[tokio::test]
async fn inter_agent_message_queue_returns_immediate_terminal_result() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: String::from("initial-execution"),
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

    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
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
                        provider_turns_executed: 1,
                        tool_calls_processed: 0,
                        final_assistant_message_id: Some(String::from("assistant-followup")),
                        final_output_text: String::from("1234"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");

    let send_result = task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "output 1234"
            }),
        )
        .await
        .expect("send_message should resume idle child");

    assert_eq!(send_result.model_output["delivery_status"], "completed");
    assert_eq!(send_result.model_output["execution_status"], "completed");
    assert_eq!(send_result.model_output["result"], "1234");
    assert!(
        send_result.model_output["resumed_execution_id"]
            .as_str()
            .is_some_and(|value| !value.trim().is_empty())
    );
    let host_payload = &send_result
        .host_result
        .as_ref()
        .expect("send_message host result")
        .payload;
    assert_eq!(host_payload["delivery_status"], "completed");
    assert_eq!(host_payload["resumed_execution_status"], "completed");
    assert_eq!(
        host_payload["resumed_execution_id"],
        send_result.model_output["resumed_execution_id"]
    );
}

/// Verifies queued messages resume after a running child agent settles.
/// 验证运行中子代理结束后会续跑排队消息。
#[tokio::test]
async fn inter_agent_message_queue_resumes_after_running_agent_settles() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let running_record = build_task_session_record(TaskSessionRecordInit {
        task_id,
        public_task_id: String::from("worker-a"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Worker A")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("initial work"),
        display_title: String::from("Worker A"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("initial work"),
        last_status: String::from("running"),
    });
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(task_id, running_record.clone());
    task_runtime
        .upsert_subagent_registry_entry(&running_record, TaskSessionStatus::Running, None)
        .expect("running child should be visible in the native registry");

    let backend_calls = Arc::new(AtomicUsize::new(0));
    let captured_prompt = Arc::new(std::sync::Mutex::new(None::<String>));
    let captured_snapshot = Arc::new(std::sync::Mutex::new(None::<CoreConversationSnapshot>));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_prompt_for_handler = Arc::clone(&captured_prompt);
    let captured_snapshot_for_handler = Arc::clone(&captured_snapshot);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                *captured_prompt_for_handler
                    .lock()
                    .expect("captured prompt lock") = Some(request.prompt.clone());
                *captured_snapshot_for_handler
                    .lock()
                    .expect("captured snapshot lock") = Some(request.snapshot.clone());
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
        )
        .expect("task backend should install");

    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    let active_guard = task_runtime
        .install_active_task_run(task_id, pending_join.abort_handle())
        .expect("active run should install");
    let send_result = task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "continue after current work",
                "delivery_mode": "queue"
            }),
        )
        .await
        .expect("send_message should enqueue while the child is running");
    assert_eq!(send_result.model_output["queued"], true);
    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);

    drop(active_guard);
    pending_join.abort();
    let resumed = task_runtime
        .resume_agent_from_inbox_after_task_settled(parent_session_id, task_id)
        .await
        .expect("settled child should resume queued messages");
    assert!(resumed);
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    let prompt = captured_prompt
        .lock()
        .expect("captured prompt lock")
        .clone()
        .expect("backend should capture prompt");
    assert!(prompt.starts_with("<agent_message>"));
    assert!(prompt.contains("continue after current work"));
    assert!(!prompt.contains("[Inter-agent messages]"));
    assert!(!prompt.contains("send_message"));
    let snapshot = captured_snapshot
        .lock()
        .expect("captured snapshot lock")
        .clone()
        .expect("backend should capture snapshot");
    assert!(snapshot.messages.iter().any(|message| {
        matches!(
            &message.payload,
            CoreConversationPayload::AgentMessage {
                message_type,
                task_name,
                sender,
                recipient,
                payload,
                trigger_turn,
                ..
            } if message_type == "MESSAGE"
                && task_name == "worker-a"
                && sender == &format!("parent:{parent_session_id}")
                && recipient == "worker-a"
                && payload == "continue after current work"
                && *trigger_turn
        )
    }));

    let resumed_again = task_runtime
        .resume_agent_from_inbox_after_task_settled(parent_session_id, task_id)
        .await
        .expect("empty inbox should be a no-op");
    assert!(!resumed_again);
}

/// Verifies child agents can send queue messages to root while interrupt delivery rejects root.
/// 验证子代理可以向 root 发送排队消息，而 interrupt 投递会拒绝 root。
#[tokio::test]
async fn send_message_to_root_enters_parent_mailbox_and_interrupt_rejects_root() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let child_session_id = Uuid::new_v4();
    let record = build_task_session_record(TaskSessionRecordInit {
        task_id: child_session_id,
        public_task_id: String::from("worker-a"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Worker A")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("initial work"),
        display_title: String::from("Worker A"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("initial work"),
        last_status: String::from("running"),
    });
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
    assert_eq!(result.model_output["delivery_mode"], "queue");
    assert_eq!(result.model_output["interrupted"], false);
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
        .invoke_send_message(
            child_session_id,
            json!({
                "target": "root",
                "message": "continue root",
                "delivery_mode": "interrupt"
            }),
        )
        .await
        .expect_err("interrupt delivery should reject root");
    assert!(error.contains("root"));
}

/// Verifies child inbox delivery appends provider input and display record through separate paths.
/// 验证子智能体 inbox 投递会通过独立路径追加 Provider 输入与显示记录。
#[test]
fn child_prompt_intervention_appends_display_record_separately_from_provider_input() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let task_id = Uuid::new_v4();
    let parent_session_id = Uuid::new_v4();
    task_runtime
        .agent_inboxes
        .write()
        .expect("agent inbox should be writable")
        .insert(
            task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("delivery-visible-1"),
                author_session_id: parent_session_id,
                target_task_id: task_id,
                target_agent_id: String::from("worker-a"),
                content: String::from("请用中文重新输出结论。"),
                created_at_ms: 1_780_000_000_000,
            }]),
        );
    let intervention = build_child_agent_prompt_intervention(Arc::clone(&task_runtime), task_id);
    let mut controller = vulcan_core::controller::VulcanCoreController::new(
        build_test_controller_config("Base prompt"),
        CoreConversationSnapshot::default(),
    );

    assert!(intervention(task_id, &mut controller));

    let snapshot = controller.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    assert!(matches!(
        &snapshot.messages[0].payload,
        CoreConversationPayload::AgentMessage {
            message_type,
            task_name,
            sender,
            recipient,
            payload,
            trigger_turn,
            ..
        } if message_type == "MESSAGE"
            && task_name == "worker-a"
            && sender == &format!("parent:{parent_session_id}")
            && recipient == "worker-a"
            && payload == "请用中文重新输出结论。"
            && *trigger_turn
    ));
    let display_record = snapshot
        .records
        .iter()
        .find(|record| record.id == "delivery-visible-1")
        .expect("display record should be stored with delivery id");
    assert!(matches!(
        display_record.record_kind,
        vulcan_core::types::TranscriptRecordKind::ProjectionOnly
    ));
    assert!(display_record.visible);
    assert_eq!(display_record.title, "智能体消息");
    assert_eq!(display_record.body, "请用中文重新输出结论。");
    assert_eq!(
        display_record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("delivery_id"))
            .and_then(Value::as_str),
        Some("delivery-visible-1")
    );
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
    let _guard = task_runtime
        .install_backend(
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
        )
        .expect("task backend should install");
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
            build_task_session_record(TaskSessionRecordInit {
                task_id,
                public_task_id: String::from("image-worker-a"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: Some(String::from("生成图片")),
                subagent: Some(String::from("image-agent")),
                invocation_config: TaskInvocationConfig::main("generate image"),
                display_title: String::from("生成图片 (image-agent)"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("generate image"),
                last_status: String::from("completed"),
            }),
        );

    let mut stream = Box::pin(task_runtime.subscribe_detached_task_events(task_id));
    let event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("terminal record stream should close immediately");
    assert!(event.is_none());
}

/// Verifies a resumed child-agent run can reopen its detached stream even when the previous run completed.
/// 验证子代理续跑时即使上一轮已经完成，也能重新打开分离事件流。
#[tokio::test]
async fn detached_task_event_stream_reopens_for_resumed_run_with_terminal_record() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(
            task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id,
                public_task_id: String::from("review-worker-a"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: Some(String::from("review code")),
                subagent: Some(String::from("code-reviewer")),
                invocation_config: TaskInvocationConfig::main("review code"),
                display_title: String::from("review code (code-reviewer)"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("review code"),
                last_status: String::from("completed"),
            }),
        );

    task_runtime
        .open_detached_task_event_stream_for_run(task_id)
        .expect("detached stream should open");
    let mut stream = Box::pin(task_runtime.subscribe_detached_task_events(task_id));
    let closed_event =
        tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await;
    assert!(
        closed_event.is_err(),
        "reopened stream should wait for new run events instead of closing from the previous terminal record"
    );

    let started_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateStarted,
        "Subagent started",
        "resumed run",
    );
    task_runtime.publish_detached_task_event(
        task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: root_session_id,
            event: started_event,
        },
    );

    let received_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("reopened stream should receive fresh run events")
        .expect("fresh run event should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = received_event else {
        panic!("expected parent delegate-started runtime event");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateStarted);
}

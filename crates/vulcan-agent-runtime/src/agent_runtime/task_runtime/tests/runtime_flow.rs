use super::*;

#[path = "runtime_flow/startup_profile.rs"]
mod startup_profile;

/// Builds one shared runtime context for task-start lifecycle tests.
/// 为子任务启动生命周期测试构建共享运行时上下文。
///
/// # Parameters
/// 参数
/// `parent_session_id` identifies the parent controller session captured by the context.
/// `parent_session_id` 标识上下文捕获的父控制器会话。
///
/// # Returns
/// 返回值
/// A fully constructed context using the default test provider configuration.
/// 返回使用默认测试 Provider 配置构建的完整上下文。
fn build_task_start_test_runtime_context(
    parent_session_id: SessionId,
) -> TaskExecutionRuntimeContext {
    // providerConfig supplies the default provider catalog used by the task-start test context.
    // providerConfig 提供子任务启动测试上下文使用的默认 Provider 目录。
    let provider_config = ProviderConfigurationFile::default();
    // foundation owns the resolved runtime dependencies captured by the shared context.
    // foundation 持有共享上下文捕获的已解析运行时依赖。
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        ExecutionPreparationRequest {
            session_id: parent_session_id,
            session_storage_directory: None,
            execution_scope: ExecutionScope::MainSession {
                session_id: SessionNodeId::from(parent_session_id),
            },
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
            conversation_hook_turn_id: Some(String::from("test-hook-turn")),
            continuation_action_id: None,
            provider_request_continuation: None,
            execution_agent_id: Some(String::from("main")),
            agent_prompt_snapshot: None,
            dedicated_session: None,
            provider_id_override: None,
            model_id_override: None,
            connection_id_override: None,
            subagent_model_policy: None,
            provider_route_lease_identity: None,
            provider_tool_selections_override: Default::default(),
            provider_tool_media_understanding_selections_override: Default::default(),
            provider_tool_enabled_capabilities_override: Default::default(),
            provider_extension_target_sets_override: None,
            provider_extension_modes_override: None,
            subagent_prompt_context: None,
            session_title: String::from("Parent session"),
            workspace_path: String::from("D:\\openvulcan\\test-project"),
            project_id: None,
            prompt: String::from("parent prompt"),
            output_style_snapshot: None,
            provider_visible_agent_messages: Vec::new(),
            turn_start_mode: None,
            conversation_state: SessionConversationState::default(),
            context_compaction_policy: None,
            context_window_selection: None,
            context_budget_override: None,
            context_window_prefill_input_tokens: None,
            execution_mode: String::from("build"),
            temperature: None,
            session_request_options: Default::default(),
            reasoning_effort: ReasoningEffortLevel::High,
            tool_exposure_override: ToolExposureSnapshot::default(),
            captured_child_tool_admission: None,
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            tool_request_hidden_tool_ids: Vec::new(),
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            prompt_skill_policy_override: Default::default(),
            project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
            session_prompt_skill_overrides: BTreeMap::new(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            vmm_runtime_ready: true,
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            controller_runtime: None,
            native_goal_runtime: None,
            native_browser_runtime: None,
            native_agent_collaboration_runtime: None,
            supplemental_mcp_tools: Vec::new(),
            media_resource_runtime: None,
            knowledge_access: None,
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        },
    )
}

/// Builds one concrete child-task execution request for startup lifecycle tests.
/// 为启动生命周期测试构建具体子任务执行请求。
///
/// # Parameters
/// 参数
/// `parent_session_id` identifies the parent and root session.
/// `parent_session_id` 标识父会话与根会话。
/// `task_id` identifies the stable child task.
/// `task_id` 标识稳定子任务。
/// `execution_id` identifies the concrete execution attempt.
/// `execution_id` 标识具体执行尝试。
///
/// # Returns
/// 返回值
/// A main-profile task request ready for the shared runtime handler.
/// 返回可交给共享运行时处理器的主配置任务请求。
fn build_task_start_test_invocation(
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
) -> TaskExecutionRequest {
    TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: execution_id.to_string(),
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
    }
}

/// Failed and stopped feedback must wait for the same durable gate as successful content.
/// 失败和停止反馈必须等待与成功正文相同的可靠发布门。
/// Returns no value; verifies live mailbox, default waits and cached-terminal reads.
/// 不返回值；验证实时邮箱、默认等待及缓存终态读取。
#[tokio::test]
async fn non_success_mailbox_waits_for_durable_parent_result() {
    for stopped in [false, true] {
        // Each status has an isolated physical execution and actual task registry.
        // 每种状态具有隔离物理执行及实际任务注册表。
        let runtime = Arc::new(TaskRuntime::new());
        // Allocate one parent identity for both default and explicit wait paths.
        // 为默认与明确等待路径分配一个父身份。
        let parent = Uuid::new_v4();
        // Keep task identity independent from its execution ID.
        // 任务身份与执行 ID 保持独立。
        let task = Uuid::new_v4();
        // Use the real invocation fixture consumed by the production record builders.
        // 使用生产记录构造器消费的真实调用夹具。
        let request = build_task_start_test_invocation(parent, task, "non-success-gate");
        runtime
            .expect_durable_task_return(task, &request.execution_id)
            .unwrap();
        if stopped {
            // Normal stop has no invented error or final assistant source.
            // 正常停止不制造错误或最终助手来源。
            let config = build_test_controller_config("Base prompt");
            // Resolve the same model settings used by other offline task fixtures.
            // 解析其他离线任务夹具所用相同模型设置。
            let resolved = test_resolved_config(&config, &request.invocation_config);
            // Store a genuine stopped outcome through the production mailbox path.
            // 通过生产邮箱路径保存真实停止结果。
            let outcome = build_stopped_task_execution_outcome(
                task,
                config,
                resolved,
                CoreConversationSnapshot::default(),
            );
            runtime
                .store_task_outcome(parent, &request, &outcome)
                .unwrap();
        } else {
            runtime
                .mark_task_failure(parent, &request, "original child failure")
                .unwrap();
        }
        assert!(
            !runtime
                .has_subagent_events_for_parent(parent.into())
                .unwrap()
        );
        assert!(
            runtime
                .wait_agent_cached_terminal_events(
                    parent,
                    &BTreeSet::from([task.into()]),
                    &BTreeSet::new()
                )
                .unwrap()
                .is_empty()
        );
        assert!(
            tokio::time::timeout(
                Duration::from_millis(25),
                runtime.invoke_wait_agent(parent, json!({"timeout_ms": 10_000}))
            )
            .await
            .is_err()
        );
        // This exact host event is published only after the parent document commit.
        // 此精确宿主事件仅在父文档提交之后发布。
        let mailbox = build_subagent_mailbox_event(
            &request,
            if stopped {
                TaskSessionStatus::Stopped
            } else {
                TaskSessionStatus::Failed
            },
            7,
            None,
            if stopped {
                None
            } else {
                Some("original child failure")
            },
            None,
        );
        runtime
            .publish_durable_task_return(&request.execution_id, mailbox.clone())
            .unwrap();
        runtime
            .publish_durable_task_return(&request.execution_id, mailbox.clone())
            .unwrap();
        assert_eq!(
            runtime
                .take_subagent_events_for_parent(parent.into())
                .unwrap(),
            vec![mailbox.clone()]
        );
        assert_eq!(
            runtime
                .wait_agent_cached_terminal_events(
                    parent,
                    &BTreeSet::from([task.into()]),
                    &BTreeSet::new()
                )
                .unwrap(),
            vec![mailbox]
        );
    }
}

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
    let context = build_task_start_test_runtime_context(parent_session_id);
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
    let invocation = build_task_start_test_invocation(parent_session_id, task_id, "test-execution");

    let error = handler(invocation)
        .await
        .expect_err("closed parent live stream should reject child start");

    assert!(error.contains("parent live stream closed"), "{error}");
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
            .keys()
            .any(|stream_key| stream_key.task_id == task_id),
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
    let _parent_guard = install_test_active_task_run(
        &task_runtime,
        parent_task_id,
        "parent-execution",
        parent_join.abort_handle(),
    );
    let _child_guard = install_test_active_task_run(
        &task_runtime,
        child_task_id,
        "child-execution",
        child_join.abort_handle(),
    );
    let _grandchild_guard = install_test_active_task_run(
        &task_runtime,
        grandchild_task_id,
        "grandchild-execution",
        grandchild_join.abort_handle(),
    );
    let _sibling_guard = install_test_active_task_run(
        &task_runtime,
        sibling_task_id,
        "sibling-execution",
        sibling_join.abort_handle(),
    );

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
        registry
            .get_for_parent(root_session_id.into(), "parent")
            .expect("parent should mirror")
            .status,
        SubagentStatus::Stopped
    );
    assert_eq!(
        registry
            .get_for_parent(parent_task_id.into(), "child")
            .expect("child should mirror")
            .status,
        SubagentStatus::Stopped
    );
    assert_eq!(
        registry
            .get_for_parent(child_task_id.into(), "grandchild")
            .expect("grandchild should mirror")
            .status,
        SubagentStatus::Stopped
    );
    assert!(
        registry
            .get_for_parent(root_session_id.into(), "sibling")
            .is_none(),
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

/// Verifies a stale guard cannot remove or reclaim a newer execution reservation.
/// 验证过期守卫不能移除或夺回更新的执行预留。
#[tokio::test]
async fn active_task_run_reservation_preserves_newer_execution_owner() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let task_id = Uuid::new_v4();
    let first_join = tokio::spawn(async { std::future::pending::<()>().await });
    let first_guard = install_test_active_task_run(
        &task_runtime,
        task_id,
        "first-execution",
        first_join.abort_handle(),
    );
    // second_guard replaces the task-level owner with the newer protocol execution.
    // second_guard 使用更新的协议执行替换任务级所有者。
    let second_guard = task_runtime
        .reserve_active_task_run(task_id, "second-execution")
        .expect("second execution should reserve");
    assert!(
        !task_runtime
            .is_task_execution_current(task_id, "first-execution")
            .expect("stale execution lookup should succeed")
    );
    assert!(
        task_runtime
            .is_task_execution_current(task_id, "second-execution")
            .expect("current execution lookup should succeed")
    );
    assert!(
        first_join
            .await
            .expect_err("replacement should abort the stale provider future")
            .is_cancelled()
    );

    first_guard
        .release()
        .expect("stale active-run generation should release idempotently");
    assert!(
        task_runtime
            .is_task_running(task_id)
            .expect("active task lookup should succeed"),
        "dropping the stale guard must preserve the newer active run"
    );

    second_guard
        .release()
        .expect("current active-run generation should release");
    assert!(
        !task_runtime
            .is_task_running(task_id)
            .expect("active task lookup should succeed"),
        "dropping the current reservation owner should remove the active run"
    );
}

/// Verifies a stop request made during prestart aborts the provider future once its handle attaches.
/// 验证启动前阶段发出的停止请求会在 Provider future 句柄附加后立即中止它。
#[tokio::test]
async fn prestart_active_task_stop_is_retained_until_abort_handle_attaches() {
    // task_runtime owns the prestart reservation under test.
    // task_runtime 持有待测试的启动前预留。
    let task_runtime = Arc::new(TaskRuntime::new());
    // task_id identifies the child stopped before provider future creation.
    // task_id 标识在 Provider future 创建前被停止的子任务。
    let task_id = Uuid::new_v4();
    // active_guard retains ownership of the exact prestart execution.
    // active_guard 保持精确启动前执行的所有权。
    let active_guard = task_runtime
        .reserve_active_task_run(task_id, "prestart-stop-execution")
        .expect("prestart execution should reserve");

    assert!(
        task_runtime
            .stop_task(task_id)
            .expect("prestart task stop should succeed")
    );

    // pending_join simulates the provider future created after the stop request.
    // pending_join 模拟停止请求之后才创建的 Provider future。
    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    active_guard
        .attach_abort_handle(pending_join.abort_handle())
        .expect("late abort handle should attach");

    assert!(
        pending_join
            .await
            .expect_err("retained prestart stop should abort the provider future")
            .is_cancelled()
    );
}

/// Verifies a superseded preparation cannot start or overwrite the newer stable-record owner.
/// 验证被取代的准备不能启动，也不能覆盖更新的稳定记录所有者。
#[test]
fn superseded_task_preparation_cannot_claim_or_fail_newer_execution() {
    // task_runtime owns the stable record and per-task execution transaction.
    // task_runtime 持有稳定记录与按任务划分的执行事务。
    let task_runtime = Arc::new(TaskRuntime::new());
    // root_session_id identifies the parent session reused by both preparations.
    // root_session_id 标识两次准备共同复用的父会话。
    let root_session_id = Uuid::new_v4();
    // task_id identifies the stable task whose preparation owner changes.
    // task_id 标识准备所有者发生变化的稳定任务。
    let task_id = Uuid::new_v4();
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(
            task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id,
                public_task_id: String::from("transaction-worker"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: Some(String::from("transaction review")),
                subagent: Some(String::from("reviewer")),
                invocation_config: TaskInvocationConfig::main("initial prompt"),
                display_title: String::from("transaction review (reviewer)"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("initial prompt"),
                last_status: String::from("completed"),
            }),
        );

    // first_invocation is the preparation that will be superseded before backend claim.
    // first_invocation 是在 backend 认领前将被取代的准备。
    let first_invocation = prepare_task_execution_request(
        &task_runtime,
        root_session_id,
        ResolvedTaskSessionIdentifier {
            task_id,
            public_task_id: String::from("transaction-worker"),
            is_resumed: true,
        },
        parse_task_tool_arguments(json!({
            "task_id": "transaction-worker",
            "prompt": "first concurrent prompt"
        }))
        .expect("first task arguments should parse"),
    )
    .expect("first preparation should succeed");
    // second_invocation becomes the only stable-record owner.
    // second_invocation 成为唯一的稳定记录所有者。
    let second_invocation = prepare_task_execution_request(
        &task_runtime,
        root_session_id,
        ResolvedTaskSessionIdentifier {
            task_id,
            public_task_id: String::from("transaction-worker"),
            is_resumed: true,
        },
        parse_task_tool_arguments(json!({
            "task_id": "transaction-worker",
            "prompt": "second concurrent prompt"
        }))
        .expect("second task arguments should parse"),
    )
    .expect("second preparation should succeed");

    let stale_claim = task_runtime.claim_and_reserve_active_task_run(&first_invocation);
    let stale_error = match stale_claim {
        Ok(_) => panic!("superseded preparation should not claim the backend"),
        Err(error) => error,
    };
    assert!(stale_error.contains("lost its preparation record"));
    // current_guard proves the newer preparation can claim the runtime owner.
    // current_guard 证明更新的准备可以认领运行时所有者。
    let current_guard = task_runtime
        .claim_and_reserve_active_task_run(&second_invocation)
        .expect("newer preparation should claim the backend");
    let v1_facts = task_runtime
        .active_task_v1_execution_facts(task_id, second_invocation.execution_id.as_str())
        .expect("current execution exposes exact V1 admission facts");
    assert_eq!(v1_facts.root_session_id, root_session_id);
    assert_eq!(v1_facts.parent_session_id, root_session_id);
    assert_eq!(v1_facts.task_id, task_id);
    assert_eq!(v1_facts.execution_id, second_invocation.execution_id);
    assert_eq!(v1_facts.prompt, "second concurrent prompt");
    assert_eq!(v1_facts.display_title, second_invocation.display_title);
    // Replayed admission of the same execution cannot replace the already retained physical owner.
    // 同执行的重放准入不能替换已经保留的物理所有者。
    assert!(
        task_runtime
            .claim_and_reserve_active_task_run(&second_invocation)
            .is_err()
    );
    assert_eq!(
        task_runtime
            .active_task_v1_execution_facts(task_id, &second_invocation.execution_id)
            .unwrap()
            .admitted_at_ms,
        v1_facts.admitted_at_ms,
    );
    assert!(v1_facts.is_resumed);

    task_runtime
        .mark_task_failure(root_session_id, &first_invocation, "stale failure")
        .expect("stale failure should be ignored");

    let record = task_runtime
        .sessions
        .read()
        .expect("task session registry should be readable")
        .get(&task_id)
        .cloned()
        .expect("stable task record should remain");
    assert_eq!(
        record.execution_id.as_deref(),
        Some(second_invocation.execution_id.as_str())
    );
    assert_eq!(record.last_prompt, "second concurrent prompt");
    assert_eq!(record.last_status, "created");
    assert!(record.last_result_text.is_empty());
    drop(current_guard);
}

/// Verifies competing preparations for one parent-scoped public slot leave only the admitted task.
/// 验证同一父级范围公开槽位的竞争准备只会留下获准任务。
#[test]
fn competing_same_parent_task_preparations_roll_back_rejected_session_record() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let first_task_id = Uuid::new_v4();
    let second_task_id = Uuid::new_v4();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for (task_id, prompt) in [
        (first_task_id, "first competing prompt"),
        (second_task_id, "second competing prompt"),
    ] {
        let task_runtime = Arc::clone(&task_runtime);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            let args = parse_task_tool_arguments(json!({
                "task_id": "shared-parent-slot",
                "prompt": prompt
            }))
            .expect("competing task arguments should parse");
            barrier.wait();
            prepare_task_execution_request(
                &task_runtime,
                root_session_id,
                ResolvedTaskSessionIdentifier {
                    task_id,
                    public_task_id: String::from("shared-parent-slot"),
                    is_resumed: false,
                },
                args,
            )
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| {
            worker
                .join()
                .expect("competing preparation should not panic")
        })
        .collect::<Vec<_>>();

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    let admitted_task_id = results
        .iter()
        .find_map(|result| result.as_ref().ok().map(|request| request.task_id))
        .expect("one competing preparation should own the public slot");
    let rejected_task_id = if admitted_task_id == first_task_id {
        second_task_id
    } else {
        first_task_id
    };
    let sessions = task_runtime
        .sessions
        .read()
        .expect("task session registry should remain readable");
    assert_eq!(sessions.len(), 1);
    assert!(sessions.contains_key(&admitted_task_id));
    assert!(!sessions.contains_key(&rejected_task_id));
    drop(sessions);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("subagent registry should remain readable");
    let admitted_entry = registry
        .get_for_parent(SessionNodeId::from(root_session_id), "shared-parent-slot")
        .expect("admitted task should own the exact parent-scoped slot");
    assert_eq!(
        admitted_entry.session_id,
        SessionNodeId::from(admitted_task_id)
    );
}

/// Verifies active-run registration returns a diagnostic instead of panicking on lock poisoning.
/// 验证活动运行注册表锁污染时会返回诊断错误而不是 panic。
#[tokio::test]
async fn reserve_active_task_run_reports_poisoned_registry() {
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
    let reserve_result = task_runtime.reserve_active_task_run(task_id, "poisoned-execution");

    let error = match reserve_result {
        Ok(_) => panic!("poisoned active task registry should reject reservation"),
        Err(error) => error,
    };
    assert!(error.contains("active task registry is locked"));
}

/// Verifies explicit active-run release returns the authoritative registry-lock failure.
/// 验证显式释放活动运行会返回权威的注册表锁失败。
#[test]
fn active_task_run_guard_release_reports_poisoned_registry() {
    // taskRuntime owns the active-run reservation and the registry poisoned after reservation.
    // taskRuntime 持有活动运行预留，以及预留后被中毒的注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // taskId identifies the exact active-run reservation under test.
    // taskId 标识待测试的精确活动运行预留。
    let task_id = Uuid::new_v4();
    // guard owns the active-run generation whose release result must be observable.
    // guard 拥有活动运行世代，其释放结果必须可观测。
    let guard = task_runtime
        .reserve_active_task_run(task_id, "explicit-release-execution")
        .expect("active run should reserve before poisoning");
    // poisonResult proves the authoritative write registry is unavailable during release.
    // poisonResult 证明释放时权威写注册表不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .active_task_runs
            .write()
            .expect("active-run registry should lock before poisoning");
        panic!("poison active-run registry before explicit release");
    }));
    assert!(poison_result.is_err());

    // error is the explicit cleanup failure returned to the background finalizer.
    // error 是返回给后台终态处理器的显式清理失败。
    let error = guard
        .release()
        .expect_err("poisoned active-run registry must reject release");

    assert!(error.contains("active task registry is locked"));
    assert!(error.contains(&task_id.to_string()));
}

/// Verifies task finalization preserves both persistence and active-run release failures.
/// 验证任务终态处理会同时保留持久化失败与活动运行释放失败。
#[test]
fn task_run_finalization_results_preserve_both_failures() {
    // error is the combined diagnostic returned when neither finalization operation succeeds.
    // error 是两项终态操作均失败时返回的合并诊断。
    let error = combine_task_run_finalization_results(
        Err(String::from("terminal state write failed")),
        Err(String::from("active-run release failed")),
    )
    .expect_err("dual finalization failures must remain observable");

    assert!(error.contains("terminal state write failed"));
    assert!(error.contains("active-run release failed"));
}

/// Verifies descendant-run queries report a poisoned task-session registry instead of claiming no descendants exist.
/// 验证后代运行查询会报告任务会话注册表中毒，而不是声称不存在后代。
#[test]
fn task_has_active_descendant_runs_reports_poisoned_session_registry() {
    // taskRuntime owns the task-session registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的任务会话注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the task-subtree source can no longer provide authoritative facts.
    // poisonResult 证明任务子树来源已无法提供权威事实。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the session registry locked until the deliberate panic poisons it.
        // guard 将会话注册表保持加锁到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .sessions
            .write()
            .expect("task-session registry should lock before poisoning");
        panic!("poison task-session registry for descendant query test");
    }));
    assert!(poison_result.is_err());
    // taskId identifies the parent whose subtree fact must not be guessed.
    // taskId 标识子树事实不得被猜测的父任务。
    let task_id = Uuid::new_v4();

    // error is the explicit task-subtree diagnostic returned by the query.
    // error 是查询返回的明确任务子树诊断。
    let error = task_runtime
        .task_has_active_descendant_runs(task_id)
        .expect_err("poisoned task-session registry should reject descendant query");

    assert!(error.contains("task-session registry is locked"));
}

/// Verifies descendant-run queries report a poisoned active-run registry instead of claiming descendants remain active.
/// 验证后代运行查询会报告活动运行注册表中毒，而不是声称后代仍然活动。
#[test]
fn task_has_active_descendant_runs_reports_poisoned_active_run_registry() {
    // taskRuntime owns the active-run registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的活动运行注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the active-run source can no longer provide authoritative facts.
    // poisonResult 证明活动运行来源已无法提供权威事实。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the active-run registry locked until the deliberate panic poisons it.
        // guard 将活动运行注册表保持加锁到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .active_task_runs
            .write()
            .expect("active task registry should lock before poisoning");
        panic!("poison active-run registry for descendant query test");
    }));
    assert!(poison_result.is_err());
    // taskId identifies the parent whose active-descendant fact must not be guessed.
    // taskId 标识活动后代事实不得被猜测的父任务。
    let task_id = Uuid::new_v4();

    // error is the explicit active-run diagnostic returned by the query.
    // error 是查询返回的明确活动运行诊断。
    let error = task_runtime
        .task_has_active_descendant_runs(task_id)
        .expect_err("poisoned active-run registry should reject descendant query");

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
        .open_detached_task_event_stream_for_run(Uuid::new_v4(), "poison-test-execution")
        .expect_err("poisoned detached stream registry should reject stream open");
    assert!(error.contains("detached task stream registry is locked"));
}

/// Verifies detached stream subscription reports registry lock poisoning instead of returning a fabricated empty stream.
/// 验证分离事件流订阅会报告注册表锁中毒，而不是返回伪造的空流。
#[test]
fn subscribe_detached_task_events_reports_poisoned_registry() {
    // taskRuntime owns the detached stream registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的分离事件流注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the deliberate panic poisoned the exact subscription registry.
    // poisonResult 证明刻意触发的 panic 已使精确订阅注册表中毒。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the write lock held until the deliberate panic poisons it.
        // guard 将写锁保持到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .detached_task_event_streams
            .write()
            .expect("detached task stream registry should lock before poisoning");
        panic!("poison detached task stream registry for subscription test");
    }));
    assert!(poison_result.is_err());
    // taskId and executionId identify the subscription that must fail explicitly.
    // taskId 与 executionId 标识必须显式失败的订阅。
    let task_id = Uuid::new_v4();
    let execution_id = "poisoned-subscription";

    // subscriptionResult retains the non-Debug stream success type for explicit pattern matching.
    // subscriptionResult 保留未实现 Debug 的流成功类型，供显式模式匹配。
    let subscription_result = task_runtime.subscribe_detached_task_events(task_id, execution_id);
    // error is the authoritative lock diagnostic returned to the caller.
    // error 是返回给调用方的权威锁错误诊断。
    let error = match subscription_result {
        Ok(_) => panic!("poisoned detached stream registry must reject subscription"),
        Err(error) => error,
    };

    assert!(error.contains("detached task stream registry is locked"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
}

/// Verifies detached stream closure remains idempotent when the exact execution key is already absent.
/// 验证精确执行键已经不存在时，分离事件流关闭仍保持幂等。
#[test]
fn close_detached_task_event_stream_accepts_missing_execution_key() {
    // taskRuntime starts with no detached stream entries.
    // taskRuntime 初始不包含任何分离事件流条目。
    let task_runtime = TaskRuntime::new();
    // taskId and executionId identify an already-absent stream close request.
    // taskId 与 executionId 标识一条已经不存在的流关闭请求。
    let task_id = Uuid::new_v4();
    let execution_id = "already-absent";

    task_runtime
        .close_detached_task_event_stream(task_id, execution_id)
        .expect("closing an already-absent exact stream should be idempotent");
}

/// Verifies detached stream closure reports registry lock poisoning instead of silently claiming cleanup.
/// 验证分离事件流关闭会报告注册表锁中毒，而不是静默声称已完成清理。
#[test]
fn close_detached_task_event_stream_reports_poisoned_registry() {
    // taskRuntime owns the detached stream registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的分离事件流注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the exact close registry is unavailable.
    // poisonResult 证明精确关闭注册表已经不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the write lock held until the deliberate panic poisons it.
        // guard 将写锁保持到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .detached_task_event_streams
            .write()
            .expect("detached task stream registry should lock before poisoning");
        panic!("poison detached task stream registry for close test");
    }));
    assert!(poison_result.is_err());
    // taskId and executionId identify the exact close operation that must fail.
    // taskId 与 executionId 标识必须失败的精确关闭操作。
    let task_id = Uuid::new_v4();
    let execution_id = "poisoned-close";

    // error is the authoritative close failure returned to the caller.
    // error 是返回给调用方的权威关闭失败。
    let error = task_runtime
        .close_detached_task_event_stream(task_id, execution_id)
        .expect_err("poisoned detached stream registry should reject close");

    assert!(error.contains("detached task stream registry is locked"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
}

/// Verifies the startup owner closes an opened detached stream when task start exits unexpectedly.
/// 验证子任务启动意外退出时，启动所有者会关闭已打开的分离事件流。
#[test]
fn detached_stream_startup_guard_drop_closes_open_stream() {
    // taskRuntime owns the detached stream registry observed before and after guard destruction.
    // taskRuntime 持有在守卫销毁前后接受检查的分离流注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // taskId identifies the stable task whose startup scope exits early.
    // taskId 标识启动作用域提前退出的稳定任务。
    let task_id = Uuid::new_v4();
    // executionId identifies the exact detached stream owned by the startup guard.
    // executionId 标识由启动守卫持有的精确分离事件流。
    let execution_id = "startup-drop-execution";
    // streamKey is the exact registry identity used to verify cleanup ownership.
    // streamKey 是用于验证清理所有权的精确注册表身份。
    let stream_key = DetachedTaskExecutionKey::new(task_id, execution_id);

    {
        // startupGuard owns cleanup until this lexical startup scope ends.
        // startupGuard 在当前词法启动作用域结束前持有清理责任。
        let _startup_guard = DetachedTaskStreamStartupGuard::open(
            Arc::clone(&task_runtime),
            Arc::new(build_task_start_test_invocation(
                Uuid::new_v4(),
                task_id,
                execution_id,
            )),
            task_runtime
                .reserve_active_task_run(task_id, execution_id)
                .unwrap(),
        )
        .expect("startup guard should open the exact detached stream");
        assert!(
            task_runtime
                .detached_task_event_streams
                .read()
                .expect("detached stream registry should read")
                .contains_key(&stream_key),
            "startup guard must own an open stream before early exit"
        );
    }

    assert!(
        !task_runtime
            .detached_task_event_streams
            .read()
            .expect("detached stream registry should read after cleanup")
            .contains_key(&stream_key),
        "dropping the startup owner must remove the empty opened stream"
    );
    assert!(!task_runtime.is_task_running(task_id).unwrap());
}

/// An observed failure or cancelled startup emits one explicit abort after lease release, before EOF.
/// 已观测失败或被取消启动在租约释放之后、流结束之前发出一次明确中止。
/// Returns no value; verifies real subscriber order and absence of premature mailbox feedback.
/// 不返回值；验证真实订阅顺序及不存在提前邮箱反馈。
#[tokio::test]
async fn detached_stream_startup_guard_emits_exact_abort_before_eof() {
    for explicit in [false, true] {
        // Isolate the two terminal paths and their process-local registries.
        // 隔离两条终止路径及其进程内注册表。
        let runtime = Arc::new(TaskRuntime::new());
        // The immutable request is shared with the cleanup guard, without another history copy.
        // 不可变请求与清理守卫共享，不额外复制历史。
        let request = Arc::new(build_task_start_test_invocation(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "startup-abort-notice",
        ));
        runtime
            .expect_durable_task_return(request.task_id, &request.execution_id)
            .unwrap();
        // Startup owns both the exact lease and detached stream before subscription.
        // 订阅前，启动同时拥有精确租约与分离流。
        let guard = DetachedTaskStreamStartupGuard::open(
            runtime.clone(),
            request.clone(),
            runtime
                .reserve_active_task_run(request.task_id, &request.execution_id)
                .unwrap(),
        )
        .unwrap();
        // This is the production buffered stream, not a synthetic event list.
        // 此处使用生产缓冲事件流，而非合成事件列表。
        let mut events = Box::pin(
            runtime
                .subscribe_detached_task_events(request.task_id, &request.execution_id)
                .unwrap(),
        );
        if explicit {
            assert_eq!(
                guard.close_after_error(String::from("original admission failure")),
                "original admission failure"
            );
        } else {
            drop(guard);
        }
        assert!(!runtime.is_task_running(request.task_id).unwrap());
        // The explicit signal is mandatory; ordinary EOF cannot carry its identity or original cause.
        // 明确信号不可缺少；普通流结束无法携带身份或原始原因。
        let Some(DetachedTaskRuntimeEvent::StartupAborted {
            parent_session_id,
            event,
        }) = events.next().await
        else {
            panic!("missing startup abort");
        };
        assert_eq!(parent_session_id, request.current_session_id);
        assert_eq!(
            event.metadata.as_ref().unwrap()["payload"]["execution_id"],
            request.execution_id
        );
        assert_eq!(
            event.metadata.as_ref().unwrap()["payload"]["execution_status"],
            "failed"
        );
        if explicit {
            assert_eq!(
                event.metadata.as_ref().unwrap()["payload"]["result"],
                "original admission failure"
            );
        }
        assert!(events.next().await.is_none());
        assert!(
            !runtime
                .has_subagent_events_for_parent(request.current_session_id.into())
                .unwrap()
        );
        assert!(
            !runtime
                .detached_task_event_streams
                .read()
                .unwrap()
                .contains_key(&DetachedTaskExecutionKey::new(
                    request.task_id,
                    &request.execution_id
                ))
        );
    }
}

/// Cancelling a pending startup future releases its lease and sends the original owner an explicit abort.
/// 取消待定启动 Future 会释放租约，并向原所有者发送明确中止。
/// Returns no value; cancellation is synchronized after ownership enters the actual spawned future.
/// 不返回值；取消同步发生在所有权进入实际派生 Future 之后。
#[tokio::test]
async fn detached_stream_startup_guard_handles_cancelled_future() {
    // Use the production stream registry and request identity across task cancellation.
    // 在任务取消期间使用生产流注册表与请求身份。
    let runtime = Arc::new(TaskRuntime::new());
    // A concrete request freezes the stream owner before the future is spawned.
    // 具体请求在 Future 派生前冻结流所有者。
    let request = Arc::new(build_task_start_test_invocation(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "cancelled-startup",
    ));
    // The guard is moved into a suspended future, matching asynchronous startup ownership.
    // 守卫移入挂起 Future，与异步启动归属一致。
    let guard = DetachedTaskStreamStartupGuard::open(
        runtime.clone(),
        request.clone(),
        runtime
            .reserve_active_task_run(request.task_id, &request.execution_id)
            .unwrap(),
    )
    .unwrap();
    // Subscribe before cancellation so the host receives the explicit control event.
    // 在取消前订阅，确保宿主收到明确控制事件。
    let mut events = Box::pin(
        runtime
            .subscribe_detached_task_events(request.task_id, &request.execution_id)
            .unwrap(),
    );
    // Synchronization proves the future really started; no timing-based sleeps are needed.
    // 同步证明 Future 已实际启动，无需按时间休眠。
    let (entered, ready) = tokio::sync::oneshot::channel();
    // This pending task owns the same guard that production startup holds across awaits.
    // 此待定任务持有与生产启动跨等待期间相同的守卫。
    let startup = tokio::spawn(async move {
        // Keep cleanup ownership alive until task cancellation drops the suspended future.
        // 保留清理归属，直到任务取消丢弃挂起 Future。
        let _guard = guard;
        entered.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    ready.await.unwrap();
    startup.abort();
    assert!(startup.await.unwrap_err().is_cancelled());
    assert!(!runtime.is_task_running(request.task_id).unwrap());
    assert!(
        matches!(events.next().await, Some(DetachedTaskRuntimeEvent::StartupAborted { parent_session_id, .. }) if parent_session_id == request.current_session_id)
    );
    assert!(events.next().await.is_none());
}

/// Provider work cannot start before handoff, and withdrawn permission never polls its future.
/// Provider 工作不能先于交接开始，撤回许可不会轮询其 Future。
/// Returns no value; exercises the production launch helper with grant, withdrawal and stop.
/// 不返回值；通过生产启动辅助函数验证授权、撤回与停止。
#[tokio::test]
async fn child_run_handoff_gates_all_provider_work() {
    for action in ["grant", "withdraw", "stop"] {
        // Observing a side effect represents the first possible preparation or Provider write.
        // 观测副作用代表首次可能发生的准备或 Provider 写入。
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // The future holds a separate reference so the test can inspect whether it was polled.
        // Future 持有独立引用，使测试能检查其是否被轮询。
        let observed = started.clone();
        // Production helper must hold the child regardless of scheduler progress before handoff.
        // 无论交接前调度如何推进，生产辅助函数都必须阻止子任务。
        let (permission, child) = spawn_child_run_after_handoff(async move {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
        tokio::task::yield_now().await;
        assert!(!started.load(std::sync::atomic::Ordering::SeqCst));
        match action {
            "grant" => {
                permission.send(()).unwrap();
                child.await.unwrap().unwrap();
                assert!(started.load(std::sync::atomic::Ordering::SeqCst));
            }
            "withdraw" => {
                drop(permission);
                assert_eq!(
                    child.await.unwrap().unwrap_err(),
                    "Child startup handoff permission was withdrawn."
                );
                assert!(!started.load(std::sync::atomic::Ordering::SeqCst));
            }
            "stop" => {
                child.abort();
                assert!(child.await.unwrap_err().is_cancelled());
                assert!(permission.send(()).is_err());
                assert!(!started.load(std::sync::atomic::Ordering::SeqCst));
            }
            _ => unreachable!(),
        }
    }
}

/// Verifies explicit startup failure cleanup preserves both the primary and stream-close errors.
/// 验证显式启动失败清理会同时保留主错误与事件流关闭错误。
#[test]
fn detached_stream_startup_guard_preserves_cleanup_failure() {
    // taskRuntime owns the stream opened before its registry is deliberately poisoned.
    // taskRuntime 持有注册表被刻意中毒前打开的事件流。
    let task_runtime = Arc::new(TaskRuntime::new());
    // taskId identifies the exact startup stream whose cleanup fails.
    // taskId 标识清理失败的精确启动事件流。
    let task_id = Uuid::new_v4();
    // executionId identifies the concrete execution included in the cleanup diagnostic.
    // executionId 标识清理诊断中包含的具体执行。
    let execution_id = "startup-cleanup-failure";
    // startupGuard owns the opened stream before cleanup becomes unavailable.
    // startupGuard 在清理能力不可用前持有已打开的事件流。
    let startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        Arc::new(build_task_start_test_invocation(
            Uuid::new_v4(),
            task_id,
            execution_id,
        )),
        task_runtime
            .reserve_active_task_run(task_id, execution_id)
            .unwrap(),
    )
    .expect("startup guard should open before poisoning");
    // poisonResult proves the authoritative stream registry cannot perform cleanup.
    // poisonResult 证明权威事件流注册表无法执行清理。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .detached_task_event_streams
            .write()
            .expect("detached stream registry should lock before poisoning");
        panic!("poison detached stream registry before startup cleanup");
    }));
    assert!(poison_result.is_err());

    // error is the combined diagnostic returned through the task-start error channel.
    // error 是通过子任务启动错误通道返回的合并诊断。
    let error = startup_guard.close_after_error(String::from("running state persistence failed"));

    assert!(error.contains("running state persistence failed"));
    assert!(error.contains("detached task stream registry is locked"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
    assert!(!task_runtime.is_task_running(task_id).unwrap());
}

/// Verifies successful handoff leaves the detached stream open for the background finalizer.
/// 验证成功交接后，分离事件流会继续为后台终态处理器保持打开。
#[test]
fn detached_stream_startup_guard_handoff_preserves_live_stream() {
    // taskRuntime owns the stream that remains live across startup ownership handoff.
    // taskRuntime 持有跨启动所有权交接后继续存活的事件流。
    let task_runtime = Arc::new(TaskRuntime::new());
    // taskId identifies the stable task handed to the background finalizer.
    // taskId 标识移交给后台终态处理器的稳定任务。
    let task_id = Uuid::new_v4();
    // executionId identifies the concrete execution retained after handoff.
    // executionId 标识交接后保留的具体执行。
    let execution_id = "startup-handoff-execution";
    // streamKey is the exact registry identity expected to survive handoff.
    // streamKey 是预期在交接后继续存在的精确注册表身份。
    let stream_key = DetachedTaskExecutionKey::new(task_id, execution_id);
    // startupGuard owns cleanup only until the background finalizer is declared ready.
    // startupGuard 仅在后台终态处理器就绪前持有清理责任。
    let mut startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        Arc::new(build_task_start_test_invocation(
            Uuid::new_v4(),
            task_id,
            execution_id,
        )),
        task_runtime
            .reserve_active_task_run(task_id, execution_id)
            .unwrap(),
    )
    .expect("startup guard should open the exact detached stream");

    // Keep the transferred lease alive for the real finalizer's full lifetime.
    // 在真实终态处理器整个生命周期中保留移交的租约。
    let finalizer_run = startup_guard.take_active_run().unwrap();
    startup_guard.handoff_to_background_finalizer().unwrap();
    assert!(task_runtime.is_task_running(task_id).unwrap());

    assert!(
        task_runtime
            .detached_task_event_streams
            .read()
            .expect("detached stream registry should read after handoff")
            .contains_key(&stream_key),
        "handoff must leave the stream open for the background finalizer"
    );
    task_runtime
        .close_detached_task_event_stream(task_id, execution_id)
        .expect("background finalizer should close the handed-off stream");
    finalizer_run.release().unwrap();
}

/// Verifies lease release precedes stream closure even while the stream registry is unavailable.
/// 验证即使事件流注册表暂不可用，租约释放也先于流关闭。
/// Returns no value; a blocked stream lock makes reversed cleanup order observable.
/// 不返回值；通过占用事件流锁观察清理顺序是否颠倒。
#[test]
fn detached_stream_startup_guard_releases_lease_before_stream_lock() {
    // The shared runtime exposes independent lease and stream locks.
    // 共享运行时提供独立的租约锁和事件流锁。
    let task_runtime = Arc::new(TaskRuntime::new());
    // The task and execution identify the one startup being cancelled.
    // 任务与执行标识唯一被取消的启动。
    let task_id = Uuid::new_v4();
    // The execution remains fixed across both cleanup observations.
    // 两次清理观察使用固定执行标识。
    let execution_id = "startup-ordered-drop";
    // The guard owns both resources before cancellation.
    // 取消前守卫持有两项资源。
    let startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        Arc::new(build_task_start_test_invocation(
            Uuid::new_v4(),
            task_id,
            execution_id,
        )),
        task_runtime
            .reserve_active_task_run(task_id, execution_id)
            .unwrap(),
    )
    .unwrap();
    // Holding this lock forces stream cleanup to wait without blocking lease release.
    // 持有此锁使事件流清理等待，但不阻塞租约释放。
    let stream_lock = task_runtime.detached_task_event_streams.write().unwrap();
    // Drop executes on another thread so the observed order is not lexical destruction order.
    // 在另一线程执行销毁，确保观察到的顺序并非词法变量销毁顺序。
    let cleanup_thread = std::thread::spawn(move || drop(startup_guard));
    // A bounded wait detects release without risking a permanent test deadlock.
    // 有界等待检测租约释放，避免测试永久死锁。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while task_runtime.is_task_running(task_id).unwrap() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // Capture the ordering fact before unblocking the stream cleanup.
    // 在解除事件流清理阻塞前记录顺序事实。
    let released_before_stream_unlock = !task_runtime.is_task_running(task_id).unwrap();
    drop(stream_lock);
    cleanup_thread.join().unwrap();
    assert!(
        released_before_stream_unlock,
        "active lease must be released before acquiring the stream cleanup lock"
    );
    assert!(
        !task_runtime
            .detached_task_event_streams
            .read()
            .unwrap()
            .contains_key(&DetachedTaskExecutionKey::new(task_id, execution_id))
    );
}

/// Verifies a handoff cannot leave the active lease owned by a consumed startup guard.
/// 验证交接不能让已消费的启动守卫继续拥有活动租约。
/// Returns no value; rejection must release both the lease and its exact stream.
/// 不返回值；拒绝时必须释放租约及其精确事件流。
#[test]
fn detached_stream_startup_guard_rejects_handoff_without_lease_transfer() {
    // The shared runtime owns both resources checked after rejection.
    // 共享运行时持有拒绝后检查的两项资源。
    let task_runtime = Arc::new(TaskRuntime::new());
    // The task is dedicated to this rejected transfer.
    // 此任务专用于被拒绝的移交。
    let task_id = Uuid::new_v4();
    // The concrete execution must not survive an incomplete handoff.
    // 具体执行不能在不完整交接后存活。
    let execution_id = "startup-invalid-handoff";
    // Startup deliberately retains its lease when handoff is requested.
    // 请求交接时启动守卫刻意保留其租约。
    let startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        Arc::new(build_task_start_test_invocation(
            Uuid::new_v4(),
            task_id,
            execution_id,
        )),
        task_runtime
            .reserve_active_task_run(task_id, execution_id)
            .unwrap(),
    )
    .unwrap();
    // The explicit ownership error remains visible to the caller.
    // 显式归属错误对调用方保持可见。
    let error = startup_guard.handoff_to_background_finalizer().unwrap_err();
    assert!(error.contains("must transfer its active run"));
    assert!(!task_runtime.is_task_running(task_id).unwrap());
    assert!(
        !task_runtime
            .detached_task_event_streams
            .read()
            .unwrap()
            .contains_key(&DetachedTaskExecutionKey::new(task_id, execution_id))
    );
}

/// Verifies detached event publication reports registry lock poisoning instead of silently dropping the event.
/// 验证分离事件发布会报告注册表锁中毒，而不是静默丢弃事件。
#[test]
fn publish_detached_task_event_reports_poisoned_registry() {
    // taskRuntime owns the detached stream registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的分离事件流注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the exact publication registry is unavailable.
    // poisonResult 证明精确发布注册表已经不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the write lock held until the deliberate panic poisons it.
        // guard 将写锁保持到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .detached_task_event_streams
            .write()
            .expect("detached task stream registry should lock before poisoning");
        panic!("poison detached task stream registry for publish test");
    }));
    assert!(poison_result.is_err());
    // taskId and executionId identify the exact publication that must fail.
    // taskId 与 executionId 标识必须失败的精确发布操作。
    let task_id = Uuid::new_v4();
    let execution_id = "poisoned-publish";
    // event is a valid parent runtime event that must not be silently discarded.
    // event 是不得被静默丢弃的合法父运行时事件。
    let event = DetachedTaskRuntimeEvent::ParentRuntime {
        parent_session_id: Uuid::new_v4(),
        event: RuntimeEvent::new(
            RuntimeEventKind::DelegateFinished,
            "Subagent finished",
            "done",
        ),
    };

    // error is the authoritative publication failure returned to the caller.
    // error 是返回给调用方的权威发布失败。
    let error = task_runtime
        .publish_detached_task_event(task_id, execution_id, event)
        .expect_err("poisoned detached stream registry should reject publication");

    assert!(error.contains("detached task stream registry is locked"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
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

/// Verifies explicit backend release returns the authoritative registry-lock failure.
/// 验证显式释放后端会返回权威的注册表锁失败。
#[test]
fn backend_guard_release_reports_poisoned_registry() {
    // taskRuntime owns the backend registration and the registry poisoned after installation.
    // taskRuntime 持有后端注册项，以及安装后被中毒的注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // sessionId identifies the exact backend generation under test.
    // sessionId 标识待测试的精确后端世代。
    let session_id = Uuid::new_v4();
    // guard owns the backend generation whose release result must be observable.
    // guard 拥有后端世代，其释放结果必须可观测。
    let guard = task_runtime
        .install_backend(
            session_id,
            Arc::new(|_request: TaskExecutionRequest| {
                Box::pin(async move { Err(String::from("unused backend")) })
            }),
        )
        .expect("backend should install before poisoning");
    // poisonResult proves the authoritative write registry is unavailable during release.
    // poisonResult 证明释放时权威写注册表不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .backends
            .write()
            .expect("backend registry should lock before poisoning");
        panic!("poison backend registry before explicit release");
    }));
    assert!(poison_result.is_err());

    // error is the explicit cleanup failure returned to the provider attempt loop.
    // error 是返回给 Provider 尝试循环的显式清理失败。
    let error = guard
        .release()
        .expect_err("poisoned backend registry must reject release");

    assert!(error.contains("task backend registry is locked"));
    assert!(error.contains(&session_id.to_string()));
}

/// Verifies backend lookup reports registry lock poisoning instead of returning a fabricated missing backend.
/// 验证后端查询会报告注册表锁中毒，而不是返回伪造的后端缺失。
#[test]
fn backend_for_session_reports_poisoned_registry() {
    // taskRuntime owns the backend registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的后端注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the authoritative backend lookup registry is unavailable.
    // poisonResult 证明权威后端查询注册表已经不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the write lock held until the deliberate panic poisons it.
        // guard 将写锁保持到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .backends
            .write()
            .expect("task backend registry should lock before poisoning");
        panic!("poison task backend registry for lookup test");
    }));
    assert!(poison_result.is_err());
    // sessionId identifies the backend lookup that must fail instead of resolving to None.
    // sessionId 标识必须失败而不能解析为 None 的后端查询。
    let session_id = Uuid::new_v4();

    // lookupResult retains the handler success type for explicit result matching.
    // lookupResult 保留处理器成功类型，供显式结果匹配。
    let lookup_result = task_runtime.backend_for_session(session_id);
    // error is the authoritative backend registry-read failure returned to the caller.
    // error 是返回给调用方的权威后端注册表读取失败。
    let error = match lookup_result {
        Ok(_) => panic!("poisoned backend registry must reject lookup"),
        Err(error) => error,
    };

    assert!(error.contains("task backend registry is locked"));
    assert!(error.contains(&session_id.to_string()));
}

/// Verifies an obsolete backend guard cannot remove a newer registration under the same session key.
/// 验证过期后端守卫无法移除同一会话键下更新的注册项。
#[test]
fn backend_guard_preserves_replacement_registration() {
    // taskRuntime owns both backend generations used by this ownership test.
    // taskRuntime 持有本所有权测试使用的两代后端。
    let task_runtime = Arc::new(TaskRuntime::new());
    // sessionId is the shared registry key whose value is deliberately replaced.
    // sessionId 是刻意替换其值的共享注册表键。
    let session_id = Uuid::new_v4();
    // originalBackend is the handler owned by the guard that becomes stale.
    // originalBackend 是由即将过期的守卫拥有的处理器。
    let original_backend: TaskExecutionHandler = Arc::new(|_request: TaskExecutionRequest| {
        Box::pin(async move { Err(String::from("original backend")) })
    });
    // replacementBackend is the handler that must survive the old guard's destruction.
    // replacementBackend 是必须在旧守卫销毁后继续存在的处理器。
    let replacement_backend: TaskExecutionHandler = Arc::new(|_request: TaskExecutionRequest| {
        Box::pin(async move { Err(String::from("replacement backend")) })
    });
    // originalGuard owns only the first registration generation.
    // originalGuard 只拥有第一代注册项。
    let original_guard = task_runtime
        .install_backend(session_id, Arc::clone(&original_backend))
        .expect("original backend should install");
    // replacementGuard owns the current registration generation under the same key.
    // replacementGuard 拥有同一键下当前注册世代。
    let replacement_guard = task_runtime
        .install_backend(session_id, Arc::clone(&replacement_backend))
        .expect("replacement backend should install");

    original_guard
        .release()
        .expect("stale backend generation should release idempotently");

    // currentBackend is the value that must remain after the stale guard drops.
    // currentBackend 是过期守卫销毁后必须保留的当前值。
    let current_backend = task_runtime
        .backend_for_session(session_id)
        .expect("backend registry should remain readable")
        .expect("replacement backend should remain registered");
    assert!(Arc::ptr_eq(&current_backend, &replacement_backend));

    replacement_guard
        .release()
        .expect("current backend generation should release");

    assert!(
        task_runtime
            .backend_for_session(session_id)
            .expect("backend registry should remain readable")
            .is_none()
    );
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

/// Verifies public task identifiers cannot masquerade as multi-segment tree paths.
/// 验证公开任务标识不能伪装成多段树路径。
#[test]
fn normalize_public_task_identifier_rejects_tree_path_separators() {
    let error = normalize_public_task_identifier("branch/worker")
        .expect_err("public task id containing a path separator should be rejected");

    assert!(error.contains("one path segment"));
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
            None,
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
            None,
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
            None,
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
            None,
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

/// Verifies archived session state absorbs stale stop projections and execution claims.
/// 验证归档会话状态会吸收过期停止投影与执行认领。
#[test]
fn archived_task_session_rejects_stale_stop_and_execution_claim() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut record = build_task_session_record(TaskSessionRecordInit {
        task_id,
        public_task_id: String::from("archived-worker"),
        origin_session_id: root_session_id,
        parent_task_id: None,
        depth: 1,
        description: None,
        subagent: None,
        invocation_config: TaskInvocationConfig::main("archived work"),
        display_title: String::from("Archived worker"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("archived work"),
        last_status: String::from("archived"),
    });
    record.stop_reason = Some(String::from("close_agent"));
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(task_id, record);
    let mut entry = SubagentRegistryEntry::new(
        "archived-worker",
        SessionNodeId::from(task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    entry.status = SubagentStatus::Archived;
    entry.stop_reason = Some(String::from("close_agent"));
    task_runtime
        .subagent_registry
        .write()
        .expect("subagent registry should be writable")
        .upsert(entry)
        .expect("archived registry fixture should insert");

    task_runtime
        .mark_stopped_tasks_in_registry(&[task_id], "app_shutdown")
        .expect("stale stopped projection should be absorbed");
    let claim_error = task_runtime
        .claim_task_execution_record_owner(task_id, "stale-execution")
        .expect_err("archived session must reject a new execution claim");

    let archived_record = task_runtime
        .sessions
        .read()
        .expect("task session registry should remain readable")
        .get(&task_id)
        .cloned()
        .expect("archived record should remain");
    assert_eq!(archived_record.last_status, "archived");
    assert_eq!(archived_record.stop_reason.as_deref(), Some("close_agent"));
    assert!(claim_error.contains("is archived"));
    let restore_error = task_runtime
        .restore_inter_agent_messages_front_after_failure(
            task_id,
            vec![QueuedInterAgentMessage {
                kind: vulcan_ai_contract::HostMessageKind::TaskStart,
                task_id: String::from("stale-archived-delivery"),
                steering: false,
                delivery_id: String::from("stale-archived-delivery"),
                author_session_id: root_session_id,
                target_task_id: task_id,
                target_agent_id: String::from("archived-worker"),
                content: String::from("stale message"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }],
            "prepare_task_execution_request",
            String::from("archived preparation rejected"),
        )
        .to_string();
    assert!(restore_error.contains("were discarded"));
    assert_eq!(
        task_runtime
            .inter_agent_message_count_for_task(task_id)
            .expect("archived inbox should remain readable"),
        0
    );
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should remain readable");
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(root_session_id), "archived-worker")
            .expect("archived registry entry should remain")
            .status,
        SubagentStatus::Archived
    );
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
    let captured_execution_id = Arc::new(std::sync::Mutex::new(None::<String>));
    let backend_calls_for_handler = Arc::clone(&backend_calls);
    let captured_prompt_for_handler = Arc::clone(&captured_prompt);
    let captured_snapshot_for_handler = Arc::clone(&captured_snapshot);
    let captured_execution_id_for_handler = Arc::clone(&captured_execution_id);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                backend_calls_for_handler.fetch_add(1, Ordering::SeqCst);
                *captured_prompt_for_handler
                    .lock()
                    .expect("captured prompt lock") = Some(request.prompt.clone());
                *captured_execution_id_for_handler
                    .lock()
                    .expect("captured execution id lock") = Some(request.execution_id.clone());
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
    assert_eq!(send_result.model_output["delivery_status"], "queued");
    assert!(send_result.model_output.get("execution_status").is_none());
    assert!(send_result.model_output.get("status").is_none());
    assert!(send_result.model_output.get("result").is_none());
    let host_payload = &send_result
        .host_result
        .as_ref()
        .expect("send_message host result")
        .payload;
    assert_eq!(host_payload["message"], "queue this context");
    assert_eq!(host_payload[INTER_AGENT_CONTINUES_TASK_TURN_KEY], false);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while backend_calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("accepted idle child delivery should resume in the background");
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    let prompt = captured_prompt
        .lock()
        .expect("captured prompt lock")
        .clone()
        .expect("backend should capture prompt");
    assert!(prompt.starts_with("<vulcan_agent_message "));
    assert!(prompt.contains(&format!("source=\"{parent_session_id}\"")));
    assert!(prompt.contains(&format!("target=\"{task_id}\"")));
    assert!(prompt.contains("kind=\"task_start\""));
    assert!(prompt.contains("task_id=\"queued-task-subagent-message-"));
    assert!(prompt.contains("queue this context"));
    assert!(!prompt.contains("[Inter-agent messages]"));
    assert!(!prompt.contains("send_message"));
    // The child root input action id must name the first envelope so clients can render it as a message.
    // 子会话根输入动作身份必须指向首个信封，显示端才能将其渲染为消息。
    let execution_id = captured_execution_id
        .lock()
        .expect("captured execution id lock")
        .clone()
        .expect("backend should capture execution id");
    let event_id = prompt
        .split("event_id=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("prompt envelope should carry event_id");
    assert!(
        execution_id.starts_with(&format!("agent-message:{event_id}:")),
        "execution id {execution_id:?} must bind envelope {event_id:?}"
    );
    let snapshot = captured_snapshot
        .lock()
        .expect("captured snapshot lock")
        .clone()
        .expect("backend should capture snapshot");
    assert!(snapshot.messages.iter().any(|message| {
        matches!(
            &message.payload,
            CoreConversationPayload::AgentMessage {
                kind,
                message_type,
                task_id: message_task_id,
                task_name,
                sender,
                recipient,
                payload,
                trigger_turn,
                ..
            } if *kind == vulcan_ai_contract::HostMessageKind::TaskStart
                && !message_task_id.is_empty()
                && message_type == "MESSAGE"
                && task_name == "worker-a"
                && sender == &parent_session_id.to_string()
                && recipient == &task_id.to_string()
                && payload == "queue this context"
                && *trigger_turn
        )
    }));
    let resumed_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after followup");
    let resumed_entry = resumed_registry
        .get_for_parent(parent_session_id.into(), "worker-a")
        .expect("worker entry should remain known");
    assert_eq!(resumed_entry.status, SubagentStatus::Running);
}

/// Verifies send_message acknowledges admission before an idle child finishes its resumed run.
/// 验证 send_message 会在空闲子代理续跑完成前确认消息接纳。
#[tokio::test]
async fn inter_agent_message_queue_returns_before_background_resume_finishes() {
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

    let backend_started = Arc::new(tokio::sync::Notify::new());
    let backend_release = Arc::new(tokio::sync::Notify::new());
    let backend_started_for_handler = Arc::clone(&backend_started);
    let backend_release_for_handler = Arc::clone(&backend_release);
    let _guard = task_runtime
        .install_backend(
            parent_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                let backend_started = Arc::clone(&backend_started_for_handler);
                let backend_release = Arc::clone(&backend_release_for_handler);
                Box::pin(async move {
                    backend_started.notify_one();
                    backend_release.notified().await;
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

    assert_eq!(send_result.model_output["delivery_status"], "queued");
    assert!(send_result.model_output.get("execution_status").is_none());
    assert!(send_result.model_output.get("result").is_none());
    let host_payload = &send_result
        .host_result
        .as_ref()
        .expect("send_message host result")
        .payload;
    assert_eq!(host_payload["delivery_status"], "queued");
    assert_eq!(host_payload[INTER_AGENT_CONTINUES_TASK_TURN_KEY], false);
    assert!(host_payload.get("resumed_execution_status").is_none());
    assert!(host_payload.get("resumed_execution_id").is_none());
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        backend_started.notified(),
    )
    .await
    .expect("background child resume should start");
    backend_release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let registry = task_runtime
                .subagent_registry_snapshot()
                .expect("registry should remain readable");
            let status = registry
                .get_for_parent(parent_session_id.into(), "worker-a")
                .expect("worker should remain registered")
                .status;
            if status == SubagentStatus::Completed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("background child resume should finish after release");
}

/// Verifies a post-admission background resume failure wakes the parent with correlated feedback.
/// 验证接纳后的后台续跑失败会通过关联反馈唤醒父级。
#[tokio::test]
async fn inter_agent_message_queue_reports_background_resume_failure() {
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
    task_runtime
        .store_task_outcome(
            parent_session_id,
            &invocation,
            &TaskExecutionOutcome {
                task_id,
                controller_config: controller_config.clone(),
                resolved_config: test_resolved_config(
                    &controller_config,
                    &invocation.invocation_config,
                ),
                snapshot: CoreConversationSnapshot::default(),
                provider_turns_executed: 1,
                tool_calls_processed: 0,
                final_assistant_message_id: Some(String::from("assistant-initial")),
                final_output_text: String::from("initial result"),
                execution_status: TaskExecutionStatus::Completed,
                usage_segments: Vec::new(),
            },
        )
        .expect("completed task should store");
    task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("initial completion event should drain");

    let send_result = task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "resume without an installed backend"
            }),
        )
        .await
        .expect("message admission should remain successful");
    let delivery_id = send_result
        .host_result
        .as_ref()
        .and_then(|result| result.payload.get("delivery_id"))
        .and_then(Value::as_str)
        .expect("accepted delivery should expose its stable id")
        .to_string();

    let feedback = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let events = task_runtime
                .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
                .expect("parent mailbox should remain readable");
            if let Some(event) = events.into_iter().next() {
                break event;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("background resume failure should wake the parent");

    assert_eq!(feedback.kind, SubagentMailboxEventKind::Message);
    assert_eq!(feedback.payload["delivery_id"], delivery_id);
    assert_eq!(feedback.payload["delivery_status"], "resume_failed");
    assert!(
        feedback.payload["error_text"]
            .as_str()
            .is_some_and(|error| error.contains("does not have a task backend installed"))
    );
    assert_eq!(
        task_runtime
            .inter_agent_message_count_for_task(task_id)
            .expect("failed delivery should remain queued"),
        1
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
                assert!(
                    request.is_resumed,
                    "queued input must use durable resumed admission"
                );
                assert_ne!(request.execution_id, "queue-running-execution");
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
    let active_guard = install_test_active_task_run(
        &task_runtime,
        task_id,
        "queue-running-execution",
        pending_join.abort_handle(),
    );
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
    assert_eq!(
        send_result
            .host_result
            .as_ref()
            .expect("send_message host result")
            .payload[INTER_AGENT_CONTINUES_TASK_TURN_KEY],
        true
    );
    assert_eq!(backend_calls.load(Ordering::SeqCst), 0);

    task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "also acknowledge the second queued message",
                "delivery_mode": "queue"
            }),
        )
        .await
        .expect("second message should remain behind the active execution");
    assert!(
        !task_runtime
            .resume_agent_from_inbox_after_task_settled(parent_session_id, task_id)
            .await
            .expect("active execution must retain its inbox")
    );
    assert_eq!(
        task_runtime
            .inter_agent_message_count_for_task(task_id)
            .unwrap(),
        2,
        "only successor admission may consume queued messages"
    );
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
    assert!(prompt.starts_with("<vulcan_agent_message "));
    assert!(prompt.contains(&format!("source=\"{parent_session_id}\"")));
    assert!(prompt.contains(&format!("target=\"{task_id}\"")));
    assert!(prompt.contains("kind=\"task_start\""));
    assert!(prompt.contains("continue after current work"));
    assert!(prompt.contains("also acknowledge the second queued message"));
    assert!(
        prompt.find("continue after current work")
            < prompt.find("also acknowledge the second queued message"),
        "durable successor input must preserve delivery order"
    );
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
                kind,
                message_type,
                task_id: message_task_id,
                task_name,
                sender,
                recipient,
                payload,
                trigger_turn,
                ..
            } if *kind == vulcan_ai_contract::HostMessageKind::TaskStart
                && !message_task_id.is_empty()
                && message_type == "MESSAGE"
                && task_name == "worker-a"
                && sender == &parent_session_id.to_string()
                && recipient == &task_id.to_string()
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

/// Verifies offline resume projection rejects conflicting retained ownership without partial snapshot mutation.
/// 验证离线续跑投影会拒绝冲突的既有所有权且不局部修改快照。
#[test]
fn offline_inter_agent_snapshot_projection_is_atomic_on_identity_conflict() {
    let task_id = Uuid::new_v4();
    // message identifies all deterministic provider, hidden-record, and display projections for one delivery.
    // message 标识单次投递的全部确定性 Provider、隐藏记录与显示投影。
    let message = QueuedInterAgentMessage {
        kind: vulcan_ai_contract::HostMessageKind::TaskStart,
        task_id: String::from("offline-delivery"),
        steering: false,
        delivery_id: String::from("offline-delivery"),
        author_session_id: Uuid::new_v4(),
        target_task_id: task_id,
        target_agent_id: String::from("worker-a"),
        content: String::from("resume offline"),
        created_at_ms: 1_780_000_000_000,
        continues_task_turn: false,
    };
    // conflicting_record occupies the stable display id with unrelated retained content.
    // conflicting_record 使用无关既有内容占用稳定显示标识。
    let conflicting_record = vulcan_core::types::ConversationRecord {
        id: message.delivery_id.clone(),
        role: vulcan_core::types::TranscriptRole::Assistant,
        record_kind: vulcan_core::types::TranscriptRecordKind::ProjectionOnly,
        visible: true,
        title: String::from("Unrelated"),
        body: String::from("unrelated body"),
        metadata: None,
        created_at: String::from("1"),
    };
    let mut snapshot = CoreConversationSnapshot {
        records: vec![conflicting_record],
        ..CoreConversationSnapshot::default()
    };
    let original_snapshot =
        serde_json::to_value(&snapshot).expect("original offline snapshot should serialize");

    let error = append_inter_agent_messages_to_snapshot(&mut snapshot, &[message])
        .expect_err("conflicting offline projection must fail");

    assert!(error.contains("conflicts with retained display record"));
    assert_eq!(
        serde_json::to_value(&snapshot).expect("offline snapshot should serialize after rejection"),
        original_snapshot
    );
}

/// Verifies an exact offline delivery replay reuses all deterministic projections without duplication.
/// 验证完全相同的离线投递重放会复用全部确定性投影且不会产生重复项。
#[test]
fn offline_inter_agent_snapshot_projection_reuses_identical_delivery_owner() {
    let message = QueuedInterAgentMessage {
        kind: vulcan_ai_contract::HostMessageKind::TaskStart,
        task_id: String::from("offline-idempotent-delivery"),
        steering: false,
        delivery_id: String::from("offline-idempotent-delivery"),
        author_session_id: Uuid::new_v4(),
        target_task_id: Uuid::new_v4(),
        target_agent_id: String::from("worker-idempotent"),
        content: String::from("resume exactly once"),
        created_at_ms: 1_780_000_000_100,
        continues_task_turn: false,
    };
    let mut snapshot = CoreConversationSnapshot::default();

    append_inter_agent_messages_to_snapshot(&mut snapshot, std::slice::from_ref(&message))
        .expect("first delivery projection should append");
    let first_messages = snapshot.messages.clone();
    let first_records = snapshot.records.clone();

    append_inter_agent_messages_to_snapshot(&mut snapshot, std::slice::from_ref(&message))
        .expect("identical delivery replay should reuse retained owners");

    assert_eq!(snapshot.messages, first_messages);
    assert_eq!(snapshot.records, first_records);
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.records.len(), 2);
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
    let execution_id = "buffered-terminal-execution";
    let terminal_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "Subagent finished",
        "terminal result",
    );
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, execution_id)
        .expect("detached stream should open before terminal publication");
    task_runtime
        .publish_detached_task_event(
            task_id,
            execution_id,
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id,
                event: terminal_event,
            },
        )
        .expect("terminal detached event should publish before stream close");
    task_runtime
        .close_detached_task_event_stream(task_id, execution_id)
        .expect("detached stream should close after terminal publication");
    // lateEvent must be rejected without contaminating the buffered terminal replay.
    // lateEvent 必须被拒绝，且不得污染已缓冲的终态回放。
    let late_event = DetachedTaskRuntimeEvent::ParentRuntime {
        parent_session_id,
        event: RuntimeEvent::new(
            RuntimeEventKind::DelegateFinished,
            "Subagent failed",
            "late duplicate terminal",
        ),
    };
    // lateError proves a retained closed state is still an explicit publication boundary.
    // lateError 证明保留中的关闭状态仍是明确的发布边界。
    let late_error = task_runtime
        .publish_detached_task_event(task_id, execution_id, late_event)
        .expect_err("closed detached stream should reject duplicate terminal publication");
    assert!(late_error.contains("is closed"));

    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, execution_id)
            .expect("detached stream subscription should succeed"),
    );
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

/// Verifies queued and interrupt successor runs suppress the superseded physical-run terminal event.
/// 验证排队与插队后继执行会抑制已经被取代的物理执行终态事件。
#[tokio::test]
async fn inter_agent_successor_run_suppresses_superseded_terminal_event() {
    // task_runtime owns the exact detached stream observed by the parent listener.
    // task_runtime 持有父级监听器观察的精确分离事件流。
    let task_runtime = TaskRuntime::new();
    // parent_session_id identifies the parent that must not receive an obsolete completion.
    // parent_session_id 标识不得接收过期完成事件的父会话。
    let parent_session_id = Uuid::new_v4();
    // task_id is shared by the settled execution and its queued-message successor.
    // task_id 由已收敛执行及其排队消息后继执行共同使用。
    let task_id = Uuid::new_v4();
    // request identifies the physical execution whose terminal event is conditionally published.
    // request 标识其终态事件需要按条件发布的物理执行。
    let request = build_task_start_test_invocation(
        parent_session_id,
        task_id,
        "superseded-message-execution",
    );
    // controller_config supplies the completed outcome's exact runtime configuration.
    // controller_config 提供完成结果的精确运行时配置。
    let controller_config = build_test_controller_config("Base prompt");
    // outcome represents the old physical run that already handed control to queued input.
    // outcome 表示已经把控制权交给排队输入的旧物理执行。
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &request.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 1,
        final_assistant_message_id: Some(String::from("assistant-superseded")),
        final_output_text: String::from("obsolete result"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    // outcome_committed proves terminal persistence can complete without prematurely waking wait_agent.
    // outcome_committed 证明终态持久化可以完成且不会提前唤醒 wait_agent。
    let outcome_committed = task_runtime
        .store_task_outcome_without_parent_mailbox(parent_session_id, &request, &outcome)
        .expect("superseded terminal projection should store without mailbox publication");
    assert!(outcome_committed);
    // deferred_mailbox_events must stay empty until the resume decision authorizes publication.
    // deferred_mailbox_events 必须保持为空，直到续跑决策授权发布。
    let deferred_mailbox_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("deferred parent mailbox should remain readable");
    assert!(deferred_mailbox_events.is_empty());
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, &request.execution_id)
        .expect("superseded execution stream should open");
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, &request.execution_id)
            .expect("superseded execution stream should subscribe"),
    );

    publish_task_finished_to_parent_after_resume_decision(
        &task_runtime,
        &request,
        &outcome,
        "obsolete completion",
        outcome_committed,
        true,
    );

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
            .await
            .is_err(),
        "a started successor run must keep the obsolete terminal event invisible"
    );
    // suppressed_mailbox_events proves wait_agent cannot consume the obsolete terminal through its process-local path.
    // suppressed_mailbox_events 证明 wait_agent 无法通过进程内路径消费过期终态。
    let suppressed_mailbox_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("suppressed parent mailbox should remain readable");
    assert!(suppressed_mailbox_events.is_empty());

    publish_task_finished_to_parent_after_resume_decision(
        &task_runtime,
        &request,
        &outcome,
        "final completion",
        outcome_committed,
        false,
    );
    let event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("the final successor completion should publish")
        .expect("the final successor completion should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = event else {
        panic!("expected parent runtime completion");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateFinished);
    // final_mailbox_events proves the true terminal execution wakes wait_agent exactly once.
    // final_mailbox_events 证明真正终态执行会精确唤醒 wait_agent 一次。
    let final_mailbox_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("final parent mailbox should remain readable");
    assert_eq!(final_mailbox_events.len(), 1);
    assert_eq!(
        final_mailbox_events[0].payload["execution_id"],
        request.execution_id
    );
}

/// Closing an active child must retain its exact stopped return after the registry is archived.
/// 关闭活动子任务后，即使注册表先归档也必须保留其精确停止回执。
/// Returns nothing; assertions cover terminal publication without reopening archived state.
/// 不返回值；断言覆盖终态发布且不复活归档状态。
#[tokio::test]
async fn close_agent_archive_keeps_original_active_stop_return() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let request = build_task_start_test_invocation(parent_session_id, task_id, "close-active-run");
    let controller_config = build_test_controller_config("Base prompt");
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(&controller_config, &request.invocation_config),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(STOPPED_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Stopped,
        usage_segments: Vec::new(),
    };
    let _active = task_runtime
        .reserve_active_task_run(task_id, &request.execution_id)
        .unwrap();
    let mut running = outcome.clone();
    running.execution_status = TaskExecutionStatus::Running;
    running.final_output_text = String::from(RUNNING_TASK_RESULT_SUMMARY);
    assert!(
        task_runtime
            .store_task_outcome_without_parent_mailbox(parent_session_id, &request, &running)
            .unwrap()
    );
    {
        let mut sessions = task_runtime.sessions.write().unwrap();
        let archived = sessions.get_mut(&task_id).unwrap();
        archived.last_status = String::from("archived");
        archived.stop_reason = Some(String::from("close_agent"));
    }
    task_runtime
        .active_task_runs
        .write()
        .unwrap()
        .get_mut(&task_id)
        .unwrap()
        .stop_requested = true;
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, &request.execution_id)
        .unwrap();
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, &request.execution_id)
            .unwrap(),
    );
    let stale_request =
        build_task_start_test_invocation(parent_session_id, task_id, "replacement-run");
    assert!(
        !task_runtime
            .store_task_outcome_without_parent_mailbox(parent_session_id, &stale_request, &outcome)
            .unwrap(),
        "the archived exception must not authorize another execution"
    );
    let committed = task_runtime
        .store_task_outcome_without_parent_mailbox(parent_session_id, &request, &outcome)
        .unwrap();
    assert!(
        committed,
        "close must preserve the original stopped terminal"
    );
    publish_task_finished_to_parent_after_resume_decision(
        &task_runtime,
        &request,
        &outcome,
        "closed active child",
        committed,
        false,
    );
    let event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        DetachedTaskRuntimeEvent::ParentRuntime { .. }
    ));
    assert_eq!(
        task_runtime.sessions.read().unwrap()[&task_id].last_status,
        "archived"
    );
    task_runtime
        .active_task_runs
        .write()
        .unwrap()
        .get_mut(&task_id)
        .unwrap()
        .stop_requested = false;
    assert!(
        !task_runtime
            .store_task_outcome_without_parent_mailbox(parent_session_id, &request, &outcome)
            .unwrap(),
        "an archived execution without an active close reservation is stale"
    );
}

/// Verifies a queued successor startup failure replaces the obsolete completion with explicit parent feedback.
/// 验证排队后继启动失败会用显式父级反馈替代过期完成结果。
#[tokio::test]
async fn queued_successor_start_failure_suppresses_obsolete_terminal_feedback() {
    // task_runtime owns the running child, queued inbox, and parent feedback channels.
    // task_runtime 持有运行中子任务、排队 inbox 与父级反馈通道。
    let task_runtime = Arc::new(TaskRuntime::new());
    // parent_session_id identifies the direct parent without an installed resume backend.
    // parent_session_id 标识未安装续跑后端的直接父级。
    let parent_session_id = Uuid::new_v4();
    // task_id identifies the child whose accepted message must remain queued after startup failure.
    // task_id 标识启动失败后其已接纳消息必须继续排队的子任务。
    let task_id = Uuid::new_v4();
    // running_record supplies the authoritative open child registry entry used by send_message.
    // running_record 提供 send_message 使用的权威开放子任务注册表条目。
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
    // request identifies the old physical execution whose completion must be suppressed.
    // request 标识其完成结果必须被抑制的旧物理执行。
    let request = build_task_start_test_invocation(
        parent_session_id,
        task_id,
        "queued-resume-failure-execution",
    );
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, &request.execution_id)
        .expect("settled execution stream should open");
    // stream observes the replacement failure emitted to the visible parent timeline.
    // stream 观察发往父级可见时间线的替代失败事件。
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, &request.execution_id)
            .expect("settled execution stream should subscribe"),
    );
    // pending_join models the still-active execution during message admission.
    // pending_join 模拟消息准入时仍活动的执行。
    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    // active_guard keeps queue delivery from starting before the test releases the old execution.
    // active_guard 防止排队投递在测试释放旧执行前启动。
    let active_guard = install_test_active_task_run(
        &task_runtime,
        task_id,
        request.execution_id.as_str(),
        pending_join.abort_handle(),
    );
    task_runtime
        .invoke_send_message(
            parent_session_id,
            json!({
                "target": "worker-a",
                "message": "resume after the current execution",
                "delivery_mode": "queue"
            }),
        )
        .await
        .expect("queue message should be admitted while the old execution is active");
    drop(active_guard);
    pending_join.abort();

    assert!(
        resume_queued_messages_after_active_run_released(&task_runtime, &request).await,
        "explicit resume failure feedback must own the terminal publication decision"
    );
    assert_eq!(
        task_runtime
            .inter_agent_message_count_for_task(task_id)
            .expect("failed successor inbox should remain readable"),
        1
    );
    // mailbox_events proves wait_agent receives one explicit failure instead of the obsolete completion.
    // mailbox_events 证明 wait_agent 收到一条显式失败，而不是过期完成结果。
    let mailbox_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("parent mailbox should remain readable");
    assert_eq!(mailbox_events.len(), 1);
    assert_eq!(mailbox_events[0].kind, SubagentMailboxEventKind::Failed);
    // detached_event proves the visible parent timeline receives the same explicit failure.
    // detached_event 证明父级可见时间线收到同一条显式失败。
    let detached_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("replacement failure event should publish")
        .expect("replacement failure event should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = detached_event else {
        panic!("expected parent runtime failure");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateFinished);
    assert!(event.detail.contains("could not resume"));
}

/// Verifies one finalization failure wakes wait_agent and updates the visible parent timeline exactly once.
/// 验证一次终态处理失败会精确唤醒一次 wait_agent，并更新父级可见时间线。
#[tokio::test]
async fn task_finalization_failure_publishes_both_parent_feedback_channels_once() {
    // task_runtime owns the mailbox and detached stream used by the two parent feedback channels.
    // task_runtime 持有两条父级反馈通道使用的 mailbox 与分离事件流。
    let task_runtime = TaskRuntime::new();
    // parent_session_id identifies the parent waiter receiving the failure.
    // parent_session_id 标识接收失败的父级等待者。
    let parent_session_id = Uuid::new_v4();
    // task_id identifies the child whose finalization failed.
    // task_id 标识终态处理失败的子任务。
    let task_id = Uuid::new_v4();
    // request preserves the exact child execution identity across both publications.
    // request 在两次通道发布中保留精确的子执行身份。
    let request = build_task_start_test_invocation(
        parent_session_id,
        task_id,
        "finalization-feedback-execution",
    );
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, &request.execution_id)
        .expect("failure execution stream should open");
    // stream observes the visible parent-timeline failure event.
    // stream 观察父级可见时间线的失败事件。
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, &request.execution_id)
            .expect("failure execution stream should subscribe"),
    );

    publish_task_failure_to_parent(&task_runtime, &request, "finalization failed");

    // detached_event proves the parent timeline receives one failure lifecycle event.
    // detached_event 证明父级时间线收到一条失败生命周期事件。
    let detached_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("failure detached event should publish")
        .expect("failure detached event should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = detached_event else {
        panic!("expected parent runtime failure");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateFinished);
    // mailbox_events proves wait_agent receives exactly one matching failure notification.
    // mailbox_events 证明 wait_agent 精确收到一条匹配的失败通知。
    let mailbox_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(parent_session_id))
        .expect("failure parent mailbox should remain readable");
    assert_eq!(mailbox_events.len(), 1);
    assert_eq!(mailbox_events[0].kind, SubagentMailboxEventKind::Failed);
    assert_eq!(
        mailbox_events[0].payload["execution_id"],
        request.execution_id
    );
}

/// Verifies publication rejects an execution that never crossed the explicit stream-open boundary.
/// 验证发布会拒绝从未经过显式事件流打开边界的执行。
#[test]
fn publish_detached_task_event_rejects_unopened_execution() {
    // taskRuntime owns no stream state for the publication attempt.
    // taskRuntime 未持有本次发布尝试对应的事件流状态。
    let task_runtime = TaskRuntime::new();
    // taskId identifies the stable task whose execution was never opened.
    // taskId 标识其执行从未打开的稳定任务。
    let task_id = Uuid::new_v4();
    // executionId identifies the missing exact execution stream.
    // executionId 标识缺失的精确执行流。
    let execution_id = "unopened-execution";
    // event is valid payload that cannot authorize implicit stream creation.
    // event 是无法授权隐式创建事件流的合法载荷。
    let event = DetachedTaskRuntimeEvent::ParentRuntime {
        parent_session_id: Uuid::new_v4(),
        event: RuntimeEvent::new(
            RuntimeEventKind::DelegateStarted,
            "Subagent started",
            "unopened execution",
        ),
    };

    // error is the explicit lifecycle-contract rejection returned by publication.
    // error 是发布操作返回的明确生命周期契约拒绝错误。
    let error = task_runtime
        .publish_detached_task_event(task_id, execution_id, event)
        .expect_err("unopened execution must reject detached event publication");

    assert!(error.contains("is not open"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
    // streams proves the rejected publication did not create an orphan buffer.
    // streams 证明被拒绝的发布没有创建孤儿缓冲区。
    let streams = task_runtime
        .detached_task_event_streams
        .read()
        .expect("detached stream registry should remain readable");
    assert!(streams.is_empty());
}

/// Verifies publication cannot recreate an execution stream after close released its state.
/// 验证发布无法在关闭操作释放状态后重新创建执行流。
#[test]
fn publish_detached_task_event_rejects_released_execution() {
    // taskRuntime owns the stream that is opened and fully released before publication.
    // taskRuntime 持有在发布前已打开并完全释放的事件流。
    let task_runtime = TaskRuntime::new();
    // taskId identifies the stable task whose completed stream must stay absent.
    // taskId 标识其已完成事件流必须保持缺失的稳定任务。
    let task_id = Uuid::new_v4();
    // executionId identifies the exact stream released by close.
    // executionId 标识由关闭操作释放的精确事件流。
    let execution_id = "released-execution";
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, execution_id)
        .expect("detached stream should open before close");
    task_runtime
        .close_detached_task_event_stream(task_id, execution_id)
        .expect("empty detached stream should close and release");
    // lateEvent represents an invalid publication arriving after terminal cleanup.
    // lateEvent 表示终态清理后到达的无效发布。
    let late_event = DetachedTaskRuntimeEvent::ParentRuntime {
        parent_session_id: Uuid::new_v4(),
        event: RuntimeEvent::new(
            RuntimeEventKind::DelegateFinished,
            "Subagent finished",
            "late terminal event",
        ),
    };

    // error is the explicit rejection that prevents a ghost stream from being recreated.
    // error 是防止重新创建幽灵事件流的明确拒绝错误。
    let error = task_runtime
        .publish_detached_task_event(task_id, execution_id, late_event)
        .expect_err("released execution must reject late detached event publication");

    assert!(error.contains("is not open"));
    assert!(error.contains(&task_id.to_string()));
    assert!(error.contains(execution_id));
    // streams proves late publication did not recreate released lifecycle state.
    // streams 证明迟到发布没有重新创建已释放的生命周期状态。
    let streams = task_runtime
        .detached_task_event_streams
        .read()
        .expect("detached stream registry should remain readable");
    assert!(streams.is_empty());
}

/// Verifies a missing exact execution key cannot create an orphan detached stream.
/// 验证缺失的精确执行键不能创建孤儿分离流。
#[tokio::test]
async fn detached_task_event_stream_closes_for_missing_execution_key() {
    // task_runtime owns no opened stream for the requested execution.
    // task_runtime 未持有所请求执行的已打开流。
    let task_runtime = TaskRuntime::new();
    // task_id identifies a stable child task without relying on its latest persisted status.
    // task_id 标识一个不依赖其最近持久状态的稳定子任务。
    let task_id = Uuid::new_v4();

    // stream targets an execution that never passed the mandatory open-before-subscribe boundary.
    // stream 指向一个从未经过强制先打开后订阅边界的执行。
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, "missing-execution")
            .expect("missing execution lookup should remain a successful closed subscription"),
    );
    // event observes immediate closure instead of an open stream that can never publish.
    // event 观测立即关闭，而不是创建一条永远无法发布事件的开放流。
    let event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("missing execution stream should close immediately");
    assert!(event.is_none());
}

/// Verifies a resumed child-agent run can reopen its detached stream even when the previous run completed.
/// 验证子代理续跑时即使上一轮已经完成，也能重新打开分离事件流。
#[tokio::test]
async fn detached_task_event_stream_reopens_for_resumed_run_with_terminal_record() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let execution_id = "resumed-run-execution";
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
        .open_detached_task_event_stream_for_run(task_id, execution_id)
        .expect("detached stream should open");
    let mut stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, execution_id)
            .expect("reopened detached stream subscription should succeed"),
    );
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
    task_runtime
        .publish_detached_task_event(
            task_id,
            execution_id,
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id: root_session_id,
                event: started_event,
            },
        )
        .expect("resumed execution event should publish");

    let received_event = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
        .await
        .expect("reopened stream should receive fresh run events")
        .expect("fresh run event should exist");
    let DetachedTaskRuntimeEvent::ParentRuntime { event, .. } = received_event else {
        panic!("expected parent delegate-started runtime event");
    };
    assert_eq!(event.kind, RuntimeEventKind::DelegateStarted);
}

/// Verifies closing an old execution cannot close or deliver into a newer stream for the same task.
/// 验证关闭旧执行时不能关闭同一任务的新执行流，也不能向其投递事件。
#[tokio::test]
async fn detached_task_event_streams_are_isolated_by_execution_id() {
    // task_runtime owns two execution-scoped streams for one stable task identity.
    // task_runtime 为同一个稳定任务身份持有两条执行级事件流。
    let task_runtime = TaskRuntime::new();
    // task_id is intentionally reused across both execution attempts.
    // task_id 被两次执行尝试刻意复用。
    let task_id = Uuid::new_v4();
    // old_execution_id identifies the terminal attempt.
    // old_execution_id 标识到达终态的旧尝试。
    let old_execution_id = "execution-old";
    // new_execution_id identifies the concurrently opened replacement attempt.
    // new_execution_id 标识并发打开的替代尝试。
    let new_execution_id = "execution-new";
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, old_execution_id)
        .expect("old execution stream should open");
    // old_stream receives only events from the old execution key.
    // old_stream 仅接收旧执行键的事件。
    let mut old_stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, old_execution_id)
            .expect("old execution subscription should succeed"),
    );
    task_runtime
        .open_detached_task_event_stream_for_run(task_id, new_execution_id)
        .expect("new execution stream should open independently");
    // new_stream receives only events from the replacement execution key.
    // new_stream 仅接收替代执行键的事件。
    let mut new_stream = Box::pin(
        task_runtime
            .subscribe_detached_task_events(task_id, new_execution_id)
            .expect("replacement execution subscription should succeed"),
    );
    task_runtime
        .publish_detached_task_event(
            task_id,
            new_execution_id,
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id: Uuid::new_v4(),
                event: RuntimeEvent::new(
                    RuntimeEventKind::DelegateStarted,
                    "Subagent started",
                    "new execution",
                ),
            },
        )
        .expect("replacement execution event should publish independently");
    task_runtime
        .close_detached_task_event_stream(task_id, old_execution_id)
        .expect("old execution stream should close independently");

    let new_event = tokio::time::timeout(std::time::Duration::from_millis(50), new_stream.next())
        .await
        .expect("new execution event should arrive")
        .expect("new execution stream should remain open");
    assert!(matches!(
        new_event,
        DetachedTaskRuntimeEvent::ParentRuntime { .. }
    ));
    let old_event = tokio::time::timeout(std::time::Duration::from_millis(50), old_stream.next())
        .await
        .expect("old execution stream should close promptly");
    assert!(old_event.is_none());
}

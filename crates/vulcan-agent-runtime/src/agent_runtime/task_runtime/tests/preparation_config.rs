use super::*;

/// Builds a resolved TaskSession configuration for controller-config tests.
/// 为控制器配置测试构建已解析 TaskSession 配置。
fn test_resolved_config(
    base: &CoreControllerConfig,
    invocation: &TaskInvocationConfig,
) -> TaskSessionResolvedConfig {
    TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: Some(core_controller_config_to_effective_config(base)),
        profile: None,
        invocation: invocation.clone(),
    })
}

/// Builds one minimal parent execution request for task-session request tests.
/// 为 TaskSession 请求测试构建一份最小父级执行请求。
fn test_parent_execution_request(session_id: SessionId) -> ExecutionPreparationRequest {
    ExecutionPreparationRequest {
        session_id,
        execution_scope: ExecutionScope::MainSession {
            session_id: SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
        execution_agent_id: Some(String::from("main")),
        agent_system_prompt_override: None,
        task_session_store_root: None,
        session_title: String::from("Parent session"),
        workspace_path: String::from("D:\\openvulcan\\test-project"),
        prompt: String::from("parent prompt"),
        conversation_state: SessionConversationState::default(),
        execution_mode: String::from("build"),
        temperature: None,
        reasoning_effort: ReasoningEffortLevel::High,
        ai_tool_configuration: PersistedAiToolConfiguration::default(),
        tool_exposure_override: ToolExposureSnapshot::default(),
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
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
    }
}

/// Verifies execution-context registration returns a diagnostic instead of panicking on lock poisoning.
/// 验证执行上下文注册表锁污染时会返回诊断错误而不是 panic。
#[test]
fn install_execution_context_reports_poisoned_registry() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .execution_contexts
            .write()
            .expect("task execution context registry should lock before poisoning");
        panic!("poison task execution context registry for test");
    }));
    assert!(poison_result.is_err());

    let session_id = Uuid::new_v4();
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
        test_parent_execution_request(session_id),
    );
    let install_result = task_runtime.install_execution_context(session_id, context);

    let error = match install_result {
        Ok(_) => panic!("poisoned task execution context registry should reject installation"),
        Err(error) => error,
    };
    assert!(error.contains("task execution context registry is locked"));
}

/// Verifies child execution requests use the resolved TaskSession runtime policy exactly.
/// 验证子代理执行请求精确使用已解析的 TaskSession 运行时策略。
#[test]
fn task_runtime_preparation_request_uses_resolved_runtime_policy() {
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
        test_parent_execution_request(parent_session_id),
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
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
    let mut resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );
    resolved_config.runtime_policy = vulcan_session_core::ExecutionRuntimePolicy {
        hooks_enabled: false,
        vmm_enabled: true,
        memory_recall_enabled: false,
        memory_writeback_enabled: false,
        database_projection_enabled: false,
    };

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

    assert_eq!(child_request.runtime_policy, resolved_config.runtime_policy);
    assert_eq!(
        child_request.execution_scope,
        ExecutionScope::TaskSession {
            root_session_id: SessionNodeId::from(parent_session_id),
            task_session_id: SessionNodeId::from(task_id),
        }
    );
}

/// Verifies ordinary child-agent execution keeps subagent tool schemas but rejects calls by default.
/// 验证普通子智能体默认保留子智能体工具 Schema，但拒绝调用。
#[test]
fn task_runtime_preparation_request_rejects_subagent_tools_by_default() {
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
        test_parent_execution_request(parent_session_id),
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
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
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

    for tool_id in [
        SPAWN_AGENT_TOOL,
        AGENT_ORCHESTRATION_INFO_TOOL,
        SEND_MESSAGE_TOOL,
        WAIT_AGENT_TOOL,
        INTERRUPT_AGENT_TOOL,
        CLOSE_AGENT_TOOL,
        LIST_AGENTS_TOOL,
        TODOLIST_TOOL_ID,
    ] {
        assert!(
            !child_request
                .tool_exposure_override
                .disabled_tools
                .iter()
                .any(|disabled_tool| disabled_tool == tool_id),
            "{tool_id} should stay registered for ordinary child agents"
        );
    }
    assert_eq!(
        child_request.tool_request_rejection_reason.as_deref(),
        Some(SUBAGENT_REJECTED_TOOL_REASON)
    );
    assert!(!child_request.tool_request_rejection_all_tools);
    assert_eq!(
        child_request.tool_request_rejection_tool_ids,
        vec![
            SPAWN_AGENT_TOOL.to_string(),
            AGENT_ORCHESTRATION_INFO_TOOL.to_string(),
            SEND_MESSAGE_TOOL.to_string(),
            WAIT_AGENT_TOOL.to_string(),
            INTERRUPT_AGENT_TOOL.to_string(),
            CLOSE_AGENT_TOOL.to_string(),
            LIST_AGENTS_TOOL.to_string(),
            TODOLIST_TOOL_ID.to_string(),
        ]
    );
}

/// Verifies clone-style contexts can explicitly retain the subagent tool family for future modes.
/// 验证克隆风格上下文可为后续模式显式保留子代理工具族。
#[test]
fn task_runtime_preparation_request_can_preserve_subagent_tools_when_allowed() {
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
        test_parent_execution_request(parent_session_id),
    )
    .with_subagent_tools_allowed(true);
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
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
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

    assert!(
        !child_request
            .tool_exposure_override
            .disabled_tools
            .iter()
            .any(|disabled_tool| disabled_tool == SPAWN_AGENT_TOOL)
    );
    assert!(
        !child_request
            .tool_exposure_override
            .disabled_tools
            .iter()
            .any(|disabled_tool| disabled_tool == TODOLIST_TOOL_ID),
        "todolist must stay registered for child-agent cache stability"
    );
    assert_eq!(
        child_request.tool_request_rejection_reason.as_deref(),
        Some(SUBAGENT_REJECTED_TOOL_REASON)
    );
    assert_eq!(
        child_request.tool_request_rejection_tool_ids,
        vec![TODOLIST_TOOL_ID.to_string()]
    );
}

/// Verifies mirror child agents preserve tool schemas but reject subagent orchestration calls.
/// 验证镜像子代理会保留工具 Schema，但拒绝子代理调度调用。
#[test]
fn task_runtime_preparation_request_preserves_tools_but_rejects_calls_for_mirror() {
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
        test_parent_execution_request(parent_session_id),
    );
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_mode = Some(String::from(SUBAGENT_MODE_MIRROR));
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        public_task_id: String::from("agent-review"),
        prompt: String::from("review this change"),
        description: Some(String::from("Review code")),
        subagent: Some(String::from("reviewer")),
        invocation_config,
        display_title: String::from("Review code (reviewer)"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

    for tool_id in [
        SPAWN_AGENT_TOOL,
        AGENT_ORCHESTRATION_INFO_TOOL,
        SEND_MESSAGE_TOOL,
        WAIT_AGENT_TOOL,
        INTERRUPT_AGENT_TOOL,
        CLOSE_AGENT_TOOL,
        LIST_AGENTS_TOOL,
        TODOLIST_TOOL_ID,
    ] {
        assert!(
            !child_request
                .tool_exposure_override
                .disabled_tools
                .iter()
                .any(|disabled_tool| disabled_tool == tool_id),
            "{tool_id} should remain visible for mirror child agents"
        );
    }
    assert_eq!(
        child_request.tool_request_rejection_reason.as_deref(),
        Some(SUBAGENT_REJECTED_TOOL_REASON)
    );
    assert!(!child_request.tool_request_rejection_all_tools);
    assert_eq!(
        child_request.tool_request_rejection_tool_ids,
        vec![
            SPAWN_AGENT_TOOL.to_string(),
            AGENT_ORCHESTRATION_INFO_TOOL.to_string(),
            SEND_MESSAGE_TOOL.to_string(),
            WAIT_AGENT_TOOL.to_string(),
            INTERRUPT_AGENT_TOOL.to_string(),
            CLOSE_AGENT_TOOL.to_string(),
            LIST_AGENTS_TOOL.to_string(),
            TODOLIST_TOOL_ID.to_string(),
        ]
    );
}

/// Verifies default spawn_agent fork keeps messages but clears provider continuation.
/// 验证 spawn_agent 默认 fork 会保留消息但清空 Provider continuation。
#[test]
fn spawn_agent_default_fork_clears_parent_continuation() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let parent_snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("u1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("parent"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: vulcan_core::types::ProviderContinuation {
            provider_id: Some(String::from("openai")),
            model_id: Some(String::from("gpt-test")),
            last_provider_message_id: Some(String::from("resp_1")),
            updated_at: Some(String::from("1")),
        },
    };
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        test_parent_execution_request(parent_session_id),
    )
    .with_parent_snapshot(parent_snapshot);
    let _guard = task_runtime
        .install_execution_context(parent_session_id, context)
        .expect("execution context should install");

    let child_snapshot =
        initial_spawn_snapshot_for_fork_turns(&task_runtime, parent_session_id, Some("all"))
            .expect("default spawn_agent fork should use parent context");

    assert_eq!(child_snapshot.messages.len(), 1);
    assert!(child_snapshot.records.is_empty());
    assert!(child_snapshot.continuation.provider_id.is_none());
    assert!(
        child_snapshot
            .continuation
            .last_provider_message_id
            .is_none()
    );
}

/// Verifies new mirror child tasks append a status user prompt after inherited context.
/// 验证新建镜像子任务会在继承上下文后追加状态用户提示词。
#[test]
fn spawn_agent_mirror_fork_appends_status_user_prompt() {
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
    let parent_snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("u1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("parent"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        test_parent_execution_request(parent_session_id),
    )
    .with_parent_snapshot(parent_snapshot);
    let _guard = task_runtime
        .install_execution_context(parent_session_id, context)
        .expect("execution context should install");
    let args = parse_task_tool_arguments(json!({
        "prompt": "review this change",
        "task_id": "mirror-review",
        "agent_mode": SUBAGENT_MODE_MIRROR,
        "fork_turns": "all"
    }))
    .expect("mirror task arguments should parse");
    let resolved_identifier = ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id: String::from("mirror-review"),
        is_resumed: false,
        persisted_metadata: None,
    };

    let request =
        prepare_task_execution_request(&task_runtime, parent_session_id, resolved_identifier, args)
            .expect("mirror task execution request should be prepared");

    assert_eq!(request.snapshot.messages.len(), 2);
    assert_eq!(request.snapshot.messages[0].id, "u1");
    let Some(last_message) = request.snapshot.messages.last() else {
        panic!("mirror snapshot should contain the appended status prompt");
    };
    assert!(last_message.id.starts_with("mirror-subagent-status-"));
    assert!(matches!(
        &last_message.payload,
        CoreConversationPayload::UserText { text, attachments }
            if attachments.is_empty()
                && text.contains("mirror subagent")
                && text.contains("ordinary available tools")
                && text.contains("subagent orchestration tools")
    ));
}

/// Verifies expert child tasks always start without parent conversation turns.
/// 验证专家子任务始终不携带父对话回合启动。
#[test]
fn spawn_agent_expert_forces_empty_initial_snapshot() {
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
    let parent_snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("u1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("parent context that expert must not inherit"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        test_parent_execution_request(parent_session_id),
    )
    .with_parent_snapshot(parent_snapshot);
    let _guard = task_runtime
        .install_execution_context(parent_session_id, context)
        .expect("execution context should install");
    let args = parse_task_tool_arguments(json!({
        "prompt": "review this change",
        "task_id": "expert-review",
        "agent_mode": SUBAGENT_MODE_EXPERT,
        "subagent_profile_id": "engineering/code-reviewer",
        "fork_turns": "all"
    }))
    .expect("expert task arguments should parse");
    let resolved_identifier = ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id: String::from("expert-review"),
        is_resumed: false,
        persisted_metadata: None,
    };

    let request =
        prepare_task_execution_request(&task_runtime, parent_session_id, resolved_identifier, args)
            .expect("expert task execution request should be prepared");

    assert!(request.snapshot.messages.is_empty());
    assert_eq!(
        request.subagent.as_deref(),
        Some("engineering/code-reviewer")
    );
    let debug = &request.invocation_config.extra["_subagent_runtime_debug"];
    assert_eq!(debug["creation_mode"], "agent");
    assert_eq!(debug["agent_id"], "engineering/code-reviewer");
    assert!(debug.get("agent_mode").is_none());
    assert!(debug.get("runtime_agent_mode").is_none());
    assert!(debug.get("prompt_source").is_none());
    assert!(debug.get("fork_turns").is_none());
    assert!(debug.get("initial_snapshot_messages").is_none());
}

/// Verifies custom spawn-agent debug metadata only reports creation-facing fields.
/// 验证自定义 spawn_agent 调试元数据只上报创建侧字段。
#[test]
fn spawn_agent_custom_debug_reports_creation_mode() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mapped_args = task_tool_args_from_spawn_agent_args(json!({
        "message": "Tell the repository story.",
        "task_name": "git-storyteller",
        "creation_mode": "custom",
        "system_prompt": "You are a git historian."
    }))
    .expect("custom spawn arguments should map to task arguments");
    let args = parse_task_tool_arguments(mapped_args).expect("custom task arguments should parse");
    let resolved_identifier = ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id: String::from("git-storyteller"),
        is_resumed: false,
        persisted_metadata: None,
    };

    let request =
        prepare_task_execution_request(&task_runtime, parent_session_id, resolved_identifier, args)
            .expect("custom task execution request should be prepared");
    let debug = &request.invocation_config.extra["_subagent_runtime_debug"];

    assert_eq!(
        request.invocation_config.system_prompt.as_deref(),
        Some("You are a git historian.")
    );
    assert_eq!(
        request.invocation_config.subagent_mode.as_deref(),
        Some(SUBAGENT_MODE_DEFAULT)
    );
    assert_eq!(debug["creation_mode"], "custom");
    assert_eq!(debug["has_custom_system_prompt"], true);
    assert!(debug.get("agent_mode").is_none());
    assert!(debug.get("runtime_agent_mode").is_none());
    assert!(debug.get("prompt_source").is_none());
    assert!(debug.get("fork_turns").is_none());
    assert!(debug.get("initial_snapshot_messages").is_none());
    assert!(debug.get("inherit_parent_config").is_none());
    assert!(debug.get("is_resumed").is_none());
}

/// Verifies spawn_agent fork fails when the parent runtime context is missing.
/// 验证缺失父级运行时上下文时 spawn_agent fork 会失败。
#[test]
fn spawn_agent_fork_requires_registered_parent_context() {
    let task_runtime = TaskRuntime::new();
    let error = initial_spawn_snapshot_for_fork_turns(&task_runtime, Uuid::new_v4(), Some("all"))
        .expect_err("default spawn_agent fork should reject missing parent context");

    assert!(error.contains("registered parent runtime context"));
}

/// Verifies child execution requests carry the composed TaskSession system prompt override.
/// 验证子代理执行请求会携带已组合的 TaskSession 系统提示词覆盖。
#[test]
fn task_runtime_preparation_request_carries_controller_prompt_override() {
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
        test_parent_execution_request(parent_session_id),
    );
    let mut invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
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
    let mut controller_config = build_test_controller_config("Base prompt");
    controller_config.system_prompt = String::from("Composed TaskSession prompt");
    invocation.controller_config = Some(controller_config);
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

    assert_eq!(
        child_request.agent_system_prompt_override.as_deref(),
        Some("Composed TaskSession prompt")
    );
}

/// Verifies that Windows child task prompts always include normalized filesystem path guidance.
/// 验证 Windows 子任务提示词始终包含规范化文件系统路径指引。
#[test]
#[cfg(windows)]
fn build_task_controller_config_appends_windows_path_guidance() {
    let base = build_test_controller_config("Base prompt");
    let description = Some(String::from("Analyze the current repository."));
    let subagent = Some(String::from("explorer"));
    let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
    let resolved_config = test_resolved_config(&base, &invocation_config);

    let config = build_task_controller_config(
        &base,
        None,
        &description,
        &subagent,
        &invocation_config,
        &resolved_config,
    );

    assert!(config.system_prompt.contains("Subagent identity: explorer"));
    assert!(
        config
            .system_prompt
            .contains("Delegated task: Analyze the current repository.")
    );
    assert!(config.system_prompt.contains("[Filesystem Paths]"));
    assert!(config.system_prompt.contains(r"C:\repo\path"));
}

/// Verifies stored child-task controller configs are reused without recomposition.
/// 验证已存储的子任务控制器配置会直接复用而不会重新组合。
#[test]
fn build_task_controller_config_reuses_stored_config() {
    let base = build_test_controller_config("Base prompt");
    let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
    let resolved_config = test_resolved_config(&base, &invocation_config);
    let stored = CoreControllerConfig {
        provider_id: String::from("stored-provider"),
        model_id: String::from("stored-model"),
        system_prompt: String::from("Stored prompt"),
        prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
            String::from("Stored prompt").clone(),
        ),
        temperature: Some(0.7),
        max_turns: 3,
        additional_params: Some(json!({ "stored": true })),
        context_injection: None,
    };

    let config = build_task_controller_config(
        &base,
        Some(stored.clone()),
        &Some(String::from("Analyze the current repository.")),
        &Some(String::from("explorer")),
        &invocation_config,
        &resolved_config,
    );

    assert_eq!(config.system_prompt, stored.system_prompt);
    assert_eq!(config.provider_id, stored.provider_id);
    assert_eq!(config.max_turns, stored.max_turns);
}

/// Verifies catalog expert prompts invalidate stale stored clone controller configs.
/// 验证目录专家提示词会让陈旧的已存储克隆控制器配置失效。
#[test]
fn build_task_controller_config_rebuilds_stored_config_for_expert_profile() {
    let base = build_test_controller_config("Base prompt");
    let mut invocation_config =
        TaskInvocationConfig::with_profile("engineering/code-reviewer", "Review code");
    invocation_config.subagent_mode = Some(String::from(SUBAGENT_MODE_EXPERT));
    let mut profile = vulcan_task_session::SubagentProfile::new(
        "engineering/code-reviewer",
        "Code Review Expert",
    );
    profile.base_system_prompt = Some(String::from(
        "You are the catalog code review expert. Apply review-specific rules.",
    ));
    let resolved_config = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: Some(core_controller_config_to_effective_config(&base)),
        profile: Some(profile),
        invocation: invocation_config.clone(),
    });
    let stored = CoreControllerConfig {
        provider_id: String::from("stored-provider"),
        model_id: String::from("stored-model"),
        system_prompt: String::from("Stored parent clone prompt"),
        prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(String::from(
            "Stored parent clone prompt",
        )),
        temperature: Some(0.7),
        max_turns: 3,
        additional_params: Some(json!({ "stored": true })),
        context_injection: None,
    };

    let config = build_task_controller_config(
        &base,
        Some(stored),
        &Some(String::from("Review code")),
        &Some(String::from("reviewer")),
        &invocation_config,
        &resolved_config,
    );

    assert!(config.system_prompt.contains("Base prompt"));
    assert!(config.system_prompt.contains("catalog code review expert"));
    assert!(config.system_prompt.contains("Subagent identity: reviewer"));
    assert!(!config.system_prompt.contains("Stored parent clone prompt"));
}

/// Verifies new child-task prompts use the clone-mode prompt composer shape.
/// 验证新建子任务提示词使用分身模式提示词组合器形态。
#[test]
fn build_task_controller_config_uses_subagent_clone_composer() {
    let base = build_test_controller_config("Base prompt");
    let description = Some(String::from("Analyze the current repository."));
    let subagent = Some(String::from("explorer"));
    let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
    let resolved_config = test_resolved_config(&base, &invocation_config);

    let config = build_task_controller_config(
        &base,
        None,
        &description,
        &subagent,
        &invocation_config,
        &resolved_config,
    );

    assert!(config.system_prompt.contains("Base prompt"));
    assert!(config.system_prompt.contains("Subagent identity: explorer"));
    assert!(
        config
            .system_prompt
            .contains("Delegated task: Analyze the current repository.")
    );
    assert!(
        config
            .system_prompt
            .contains("Subagent Developer Instructions")
    );
}

/// Verifies invocation-level system prompt overrides are routed through the clone composer.
/// 验证调用级系统提示词覆盖会通过分身组合器路由。
#[test]
fn build_task_controller_config_uses_invocation_system_prompt() {
    let base = build_test_controller_config("Base prompt");
    let description = Some(String::from("Review code"));
    let subagent = Some(String::from("reviewer"));
    let mut invocation_config = TaskInvocationConfig::main("Review code");
    invocation_config.inherit_parent_config = Some(false);
    invocation_config.system_prompt = Some(String::from("You are a strict code reviewer."));
    invocation_config.system_prompt_append = Some(String::from("Focus only on correctness."));
    let resolved_config = test_resolved_config(&base, &invocation_config);

    let config = build_task_controller_config(
        &base,
        None,
        &description,
        &subagent,
        &invocation_config,
        &resolved_config,
    );

    assert!(!config.system_prompt.contains("Base prompt"));
    assert!(
        config
            .system_prompt
            .contains("You are a strict code reviewer.")
    );
    assert!(config.system_prompt.contains("Focus only on correctness."));
    assert!(config.system_prompt.contains("Subagent identity: reviewer"));
}

/// Verifies flexible task arguments ignore legacy profile fields and keep supported overrides.
/// 验证灵活 task 参数会忽略旧 profile 字段并保留受支持覆盖项。
#[test]
fn task_invocation_config_ignores_profile_and_keeps_supported_overrides() {
    let mut args = test_task_args("review code", None);
    args.provider_model = Some(json!({
        "provider": "deepseek",
        "model": "deepseek-v4",
        "reasoning": "high",
        "mode": "Plan"
    }));
    args.tools = Some(json!({
        "allowed": ["shell_command", "vulcan_file_read"],
        "disabled": ["vmm_memory_write"]
    }));
    args.skills = Some(json!({
        "enabled": ["review.skill"],
        "disabled": ["draft.skill"]
    }));
    args.luaskills = Some(json!({ "enabled": ["vulcan-codekit"] }));
    args.mcp = Some(json!({ "servers": ["local-dev"] }));
    args.context_compaction = Some(json!({
        "context_budget": 12000,
        "max_provider_requests": 4,
        "auto_compaction_enabled": true
    }));
    args.runtime_policy = Some(json!({
        "vmmEnabled": true,
        "memoryRecallEnabled": true,
        "databaseProjectionEnabled": true
    }));
    args.extra = Some(json!({ "temperature": 0.1, "max_turns": 2 }));

    let config = task_invocation_config_from_args(&args);

    assert_eq!(
        config.provider_model.provider_id.as_deref(),
        Some("deepseek")
    );
    assert_eq!(
        config.provider_model.model_id.as_deref(),
        Some("deepseek-v4")
    );
    assert_eq!(config.provider_model.reasoning_effort.as_deref(), None);
    assert_eq!(
        config.provider_model.execution_mode.as_deref(),
        Some("Plan")
    );
    assert_eq!(
        config.tool_exposure.allowed_tools,
        vec!["shell_command", "vulcan_file_read"]
    );
    assert_eq!(
        config.tool_exposure.disabled_tools,
        vec!["vmm_memory_write"]
    );
    assert_eq!(config.prompt_skills.enabled, vec!["review.skill"]);
    assert_eq!(config.prompt_skills.disabled, vec!["draft.skill"]);
    assert_eq!(config.context_compaction.context_budget, Some(12000));
    assert_eq!(config.context_compaction.max_provider_requests, Some(4));
    assert_eq!(
        config.context_compaction.auto_compaction_enabled,
        Some(true)
    );
    assert_eq!(config.runtime_policy.vmm_enabled, Some(true));
    assert_eq!(config.runtime_policy.memory_recall_enabled, Some(true));
    assert_eq!(
        config.runtime_policy.database_projection_enabled,
        Some(true)
    );
    assert_eq!(config.extra["temperature"], json!(0.1));
}

/// Verifies TaskSession compaction planning follows the resolved automatic-compaction toggle.
/// 验证 TaskSession 压缩规划会跟随已解析的自动压缩开关。
#[test]
fn task_context_compaction_policy_follows_resolved_toggle() {
    let mut snapshot = ContextCompactionPolicySnapshot {
        auto_compaction_enabled: false,
        ..Default::default()
    };

    let disabled_policy = task_context_compaction_policy_from_snapshot(&snapshot);

    assert!(!disabled_policy.auto);
    assert!(disabled_policy.prune);

    snapshot.auto_compaction_enabled = true;
    let enabled_policy = task_context_compaction_policy_from_snapshot(&snapshot);

    assert!(enabled_policy.auto);
    assert_eq!(
        enabled_policy.tail_turns,
        ContextCompactionPolicy::default().tail_turns
    );
}

/// Verifies shared-runtime task report conversion preserves the final assistant message id from the child transcript.
/// 验证共享运行时任务报告转换会保留子代理转录中的最终助手消息标识。
#[test]
fn task_execution_outcome_from_report_preserves_final_assistant_message_id() {
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("assistant-final"),
            payload: vulcan_core::types::ConversationPayload::Assistant {
                text: String::from("done"),
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
    let report = ExecutionRunReport {
        session_id: task_id,
        preparation: None,
        assistant_output: Some(String::from("done")),
        thinking_output: None,
        conversation_state: Some(core_snapshot_to_session_conversation_state(&snapshot)),
        provider_request_count: 1,
        last_successful_provider_usage: Some(vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(120),
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        }),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let outcome =
        task_execution_outcome_from_report(&invocation, controller_config, resolved_config, report)
            .expect("shared-runtime report should convert");

    assert_eq!(
        outcome.final_assistant_message_id.as_deref(),
        Some("assistant-final")
    );
    assert_eq!(outcome.final_output_text, "done");
    assert_eq!(outcome.usage_segments.len(), 1);
    let segment = &outcome.usage_segments[0];
    assert_eq!(segment.kind, SessionTurnUsageSegmentKind::SubagentTask);
    assert_eq!(segment.trigger.as_deref(), Some("agent-review"));
    assert_eq!(segment.provider_call_count, 1);
    assert_eq!(segment.input_tokens, 120);
    assert_eq!(segment.output_tokens, 30);
    assert_eq!(segment.reasoning_tokens, 10);
    assert_eq!(segment.total_tokens, 160);
    assert_eq!(segment.cached_input_tokens, Some(40));
}

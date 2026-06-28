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
    let mut invocation_config = TaskInvocationConfig::with_profile("agent-review", "Review code");
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

/// Verifies flexible task arguments populate the shared TaskSession invocation config.
/// 验证灵活 task 参数会填充共享 TaskSession 调用配置。
#[test]
fn task_invocation_config_parses_profile_tool_skill_and_compaction_overrides() {
    let mut args = test_task_args("review code", None);
    args.subagent_profile_id = Some(String::from("agent-review"));
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

    assert_eq!(config.subagent_profile_id.as_deref(), Some("agent-review"));
    assert_eq!(
        config.provider_model.provider_id.as_deref(),
        Some("deepseek")
    );
    assert_eq!(
        config.provider_model.model_id.as_deref(),
        Some("deepseek-v4")
    );
    assert_eq!(
        config.provider_model.reasoning_effort.as_deref(),
        Some("high")
    );
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
    let mut snapshot = ContextCompactionPolicySnapshot::default();
    snapshot.auto_compaction_enabled = false;

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

/// Verifies runtime-registered profiles enter the shared TaskSession config resolver.
/// 验证运行时注册的 profile 会进入共享 TaskSession 配置解析器。
#[test]
fn task_runtime_resolves_registered_subagent_profile() {
    let runtime = TaskRuntime::new();
    let base = build_test_controller_config("Base prompt");
    let mut profile = SubagentProfile::new("agent-review", "Review Agent");
    profile.base_system_prompt = Some(String::from("Profile reviewer prompt."));
    profile.tool_exposure.allowed_tools = vec![String::from("shell_command")];
    runtime
        .configure_subagent_profiles(vec![profile])
        .expect("profiles should configure");
    let invocation = TaskInvocationConfig::with_profile("agent-review", "review code");

    let resolved = runtime
        .resolve_task_session_config_for_execution(&base, &invocation)
        .expect("profile should resolve");

    assert_eq!(
        resolved.effective_config.subagent_profile_id.as_deref(),
        Some("agent-review")
    );
    assert!(
        resolved
            .effective_config
            .system_prompt_layers
            .iter()
            .any(|layer| layer.contains("Profile reviewer prompt."))
    );
    assert_eq!(
        resolved.effective_config.tool_exposure.allowed_tools,
        vec!["shell_command"]
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
        last_successful_provider_usage: None,
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
}

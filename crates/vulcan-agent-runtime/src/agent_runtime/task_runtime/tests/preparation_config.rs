use super::*;
use crate::{ExecutionKnowledgeAccessContext, ExecutionSessionGoalContext};
use vldb_controller_client::{ClientRegistration, ControllerClient, ControllerClientConfig};
use vulcan_workspace_store::data_plane::ControllerRuntimeHandle;

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
        managed_codebase_memory: None,
        execution_scope: ExecutionScope::MainSession {
            session_id: SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
        conversation_hook_turn_id: Some(String::from("test-hook-turn")),
        execution_agent_id: Some(String::from("main")),
        agent_prompt_snapshot: None,
        provider_id_override: None,
        model_id_override: None,
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
        tool_exposure_override: ToolExposureSnapshot::default(),
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
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    }
}

/// Returns the child-agent hard-disabled public tool ids expected by preparation tests.
/// 返回准备阶段测试期望的智能体硬禁用公开工具标识。
fn expected_child_forbidden_tool_ids() -> Vec<String> {
    vulcan_session_core::child_agent_forbidden_tool_ids()
        .iter()
        .map(|tool_id| (*tool_id).to_string())
        .collect()
}

/// Verifies child agents cannot use parent-owned interaction or memory tools.
/// 验证智能体不能使用父级拥有的交互或记忆工具。
#[test]
fn child_forbidden_tool_ids_include_parent_owned_tools() {
    let forbidden_tools = expected_child_forbidden_tool_ids();
    for tool_id in [
        "spawn_agent",
        "agent_orchestration_info",
        "send_message",
        "wait_agent",
        "interrupt_agent",
        "close_agent",
        "list_agents",
        "ask",
        "submit_plan",
        "todolist",
        "vmm_memory_search",
        "vmm_turn_details",
        "vmm_memory_write",
        "vmm_memory_delete",
    ] {
        assert!(
            forbidden_tools.contains(&tool_id.to_string()),
            "child-agent hard-disabled tools must include {tool_id}"
        );
    }
}

/// Verifies child preparation receives the complete prompt-skill capability policy instead of only legacy lists.
/// 验证子执行准备会收到完整提示词技能能力策略，而不只是旧版列表。
#[test]
fn task_prompt_skill_snapshot_propagates_allow_only_policy() {
    let mut request = test_parent_execution_request(Uuid::new_v4());
    let snapshot = PromptSkillSnapshot {
        policy: CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::AllowOnly,
            selectors: vec![String::from("content-plugin:review-helper")],
        },
        enabled: vec![String::from("content-plugin:review-helper")],
        disabled: Vec::new(),
    };

    apply_task_prompt_skill_snapshot(&mut request, &snapshot);

    assert_eq!(request.prompt_skill_policy_override, snapshot.policy);
    assert_eq!(
        request
            .session_prompt_skill_overrides
            .get("content-plugin:review-helper"),
        Some(&PersistedPromptSkillState::Enabled)
    );
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

/// Verifies explicit execution-context release returns the authoritative registry-lock failure.
/// 验证显式释放执行上下文会返回权威的注册表锁失败。
#[test]
fn execution_context_guard_release_reports_poisoned_registry() {
    // taskRuntime owns the execution-context registration and the registry poisoned after installation.
    // taskRuntime 持有执行上下文注册项，以及安装后被中毒的注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // sessionId identifies the exact context generation under test.
    // sessionId 标识待测试的精确上下文世代。
    let session_id = Uuid::new_v4();
    // providerConfig supplies the runtime state captured by the registered context.
    // providerConfig 提供注册上下文捕获的运行时状态。
    let provider_config = ProviderConfigurationFile::default();
    // foundation supplies the shared dependencies required by the context constructor.
    // foundation 提供上下文构造器所需的共享依赖。
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    // context is the exact runtime value installed before the registry becomes unavailable.
    // context 是注册表不可用前安装的精确运行时值。
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        test_parent_execution_request(session_id),
    );
    // guard owns the installed context generation whose release result must be observable.
    // guard 拥有已安装的上下文世代，其释放结果必须可观测。
    let guard = task_runtime
        .install_execution_context(session_id, context)
        .expect("execution context should install before poisoning");
    // poisonResult proves the authoritative write registry is unavailable during release.
    // poisonResult 证明释放时权威写注册表不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = task_runtime
            .execution_contexts
            .write()
            .expect("execution-context registry should lock before poisoning");
        panic!("poison execution-context registry before explicit release");
    }));
    assert!(poison_result.is_err());

    // error is the explicit cleanup failure returned to the normal execution path.
    // error 是返回给正常执行路径的显式清理失败。
    let error = guard
        .release()
        .expect_err("poisoned execution-context registry must reject release");

    assert!(error.contains("task execution context registry is locked"));
    assert!(error.contains(&session_id.to_string()));
}

/// Verifies execution-context lookup reports registry lock poisoning instead of returning a fabricated missing context.
/// 验证执行上下文查询会报告注册表锁中毒，而不是返回伪造的上下文缺失。
#[test]
fn execution_context_for_session_reports_poisoned_registry() {
    // taskRuntime owns the execution-context registry deliberately poisoned below.
    // taskRuntime 持有下方刻意中毒的执行上下文注册表。
    let task_runtime = Arc::new(TaskRuntime::new());
    // poisonResult proves the authoritative lookup registry is unavailable.
    // poisonResult 证明权威查询注册表已经不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the write lock held until the deliberate panic poisons it.
        // guard 将写锁保持到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .execution_contexts
            .write()
            .expect("task execution context registry should lock before poisoning");
        panic!("poison task execution context registry for lookup test");
    }));
    assert!(poison_result.is_err());
    // sessionId identifies the lookup that must fail rather than resolve to None.
    // sessionId 标识必须失败而不能解析为 None 的查询。
    let session_id = Uuid::new_v4();

    // lookupResult retains the runtime-context success type for explicit result matching.
    // lookupResult 保留运行时上下文成功类型，供显式结果匹配。
    let lookup_result = task_runtime.execution_context_for_session(session_id);
    // error is the authoritative registry-read failure returned to the caller.
    // error 是返回给调用方的权威注册表读取失败。
    let error = match lookup_result {
        Ok(_) => panic!("poisoned execution-context registry must reject lookup"),
        Err(error) => error,
    };

    assert!(error.contains("task execution context registry is locked"));
    assert!(error.contains(&session_id.to_string()));
}

/// Verifies an obsolete execution-context guard cannot remove a newer registration under the same session key.
/// 验证过期执行上下文守卫无法移除同一会话键下更新的注册项。
#[test]
fn execution_context_guard_preserves_replacement_registration() {
    // taskRuntime owns both context generations used by this ownership test.
    // taskRuntime 持有本所有权测试使用的两代上下文。
    let task_runtime = Arc::new(TaskRuntime::new());
    // sessionId is the shared registry key whose context is deliberately replaced.
    // sessionId 是刻意替换其上下文的共享注册表键。
    let session_id = Uuid::new_v4();
    // providerConfig supplies identical provider state to both context generations.
    // providerConfig 为两代上下文提供相同的供应商状态。
    let provider_config = ProviderConfigurationFile::default();
    // foundation supplies the shared runtime dependencies captured by both contexts.
    // foundation 提供两代上下文共同捕获的运行时依赖。
    let foundation = Arc::new(RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    ));
    // originalRequest identifies the context generation owned by the stale guard.
    // originalRequest 标识由过期守卫拥有的上下文世代。
    let mut original_request = test_parent_execution_request(session_id);
    original_request.prompt = String::from("original context");
    // replacementRequest identifies the context generation that must remain registered.
    // replacementRequest 标识必须继续注册的替代上下文世代。
    let mut replacement_request = test_parent_execution_request(session_id);
    replacement_request.prompt = String::from("replacement context");
    // originalContext is the first value installed under the session key.
    // originalContext 是在会话键下安装的第一个值。
    let original_context = TaskExecutionRuntimeContext::new(
        provider_config.clone(),
        Arc::clone(&foundation),
        None,
        original_request,
    );
    // replacementContext is the newer value that must survive stale cleanup.
    // replacementContext 是必须在过期清理后继续存在的新值。
    let replacement_context =
        TaskExecutionRuntimeContext::new(provider_config, foundation, None, replacement_request);
    // originalGuard owns only the first context registration generation.
    // originalGuard 只拥有第一代上下文注册项。
    let original_guard = task_runtime
        .install_execution_context(session_id, original_context)
        .expect("original execution context should install");
    // replacementGuard owns the current context registration generation.
    // replacementGuard 拥有当前上下文注册世代。
    let replacement_guard = task_runtime
        .install_execution_context(session_id, replacement_context)
        .expect("replacement execution context should install");

    original_guard
        .release()
        .expect("stale execution-context generation should release idempotently");

    // currentContext is the value that must remain after the stale guard drops.
    // currentContext 是过期守卫销毁后必须保留的当前值。
    let current_context = task_runtime
        .execution_context_for_session(session_id)
        .expect("execution-context registry should remain readable")
        .expect("replacement execution context should remain registered");
    assert_eq!(
        current_context.request_template.prompt,
        "replacement context"
    );

    replacement_guard
        .release()
        .expect("current execution-context generation should release");

    assert!(
        task_runtime
            .execution_context_for_session(session_id)
            .expect("execution-context registry should remain readable")
            .is_none()
    );
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
    resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled = false;
    resolved_config
        .effective_config
        .context_compaction
        .context_budget = Some(12_000);

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    assert_eq!(child_request.runtime_policy, resolved_config.runtime_policy);
    assert_eq!(
        child_request.execution_scope,
        ExecutionScope::TaskSession {
            root_session_id: SessionNodeId::from(parent_session_id),
            task_session_id: SessionNodeId::from(task_id),
        }
    );
    let child_compaction_policy = child_request
        .context_compaction_policy
        .expect("child execution request should carry resolved compaction policy");
    assert!(!child_compaction_policy.auto);
    assert_eq!(child_request.context_budget_override, Some(12_000));
}

/// Verifies ordinary child-agent execution hides parent-session tools by default.
/// 验证普通智能体默认隐藏父会话工具。
#[test]
fn task_runtime_preparation_request_hides_parent_tools_by_default() {
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
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    assert_eq!(
        child_request.tool_request_hidden_tool_ids,
        expected_child_forbidden_tool_ids()
    );
    assert_eq!(child_request.tool_request_rejection_reason, None);
    assert!(!child_request.tool_request_rejection_all_tools);
    assert!(child_request.tool_request_rejection_tool_ids.is_empty());
}

/// Verifies mirror child agents preserve tool schemas but reject parent-session tool calls.
/// 验证镜像智能体会保留工具 Schema，但拒绝父会话工具调用。
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
        execution_id: String::from("test-execution"),
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
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    assert!(child_request.tool_request_hidden_tool_ids.is_empty());
    assert_eq!(
        child_request.tool_request_rejection_reason.as_deref(),
        Some(SUBAGENT_REJECTED_TOOL_REASON)
    );
    assert!(!child_request.tool_request_rejection_all_tools);
    assert_eq!(
        child_request.tool_request_rejection_tool_ids,
        expected_child_forbidden_tool_ids()
    );
}

/// Verifies clone-latest runtime initialization preserves the parent runtime snapshot.
/// 验证克隆最新运行时初始化会保留父级运行时快照。
#[test]
fn spawn_agent_clone_latest_runtime_preserves_parent_snapshot() {
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
        records: vec![vulcan_core::types::ConversationRecord {
            id: String::from("r1"),
            role: vulcan_core::types::TranscriptRole::Assistant,
            record_kind: vulcan_core::types::TranscriptRecordKind::Hidden,
            visible: false,
            title: String::from("agent feedback"),
            body: String::from("parent-runtime-record"),
            metadata: Some(json!({
                "kind": "subagent_feedback"
            })),
            created_at: String::from("1"),
        }],
        continuation: vulcan_core::types::ProviderContinuation {
            provider_id: Some(String::from("openai")),
            model_id: Some(String::from("gpt-test")),
            last_provider_message_id: Some(String::from("resp_1")),
            last_local_message_id: Some(String::from("assistant-1")),
            context_cache_epoch_id: None,
            context_anchor_revision: None,
            context_rewrite_fingerprint: None,
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

    let child_snapshot = initial_spawn_snapshot_for_context_source(
        &task_runtime,
        parent_session_id,
        TaskInitialContextSource::CloneLatestRuntime,
    )
    .expect("clone-latest runtime source should use parent context");

    assert_eq!(child_snapshot.messages.len(), 1);
    assert_eq!(child_snapshot.records.len(), 1);
    assert_eq!(child_snapshot.records[0].id, "r1");
    assert_eq!(child_snapshot.records[0].body, "parent-runtime-record");
    assert_eq!(
        child_snapshot.records[0].metadata.as_ref(),
        Some(&json!({
            "kind": "subagent_feedback"
        }))
    );
    assert_eq!(
        child_snapshot.continuation.provider_id.as_deref(),
        Some("openai")
    );
    assert_eq!(
        child_snapshot
            .continuation
            .last_provider_message_id
            .as_deref(),
        Some("resp_1")
    );
}

/// Verifies new mirror child tasks inherit context without injecting display-only status prompts.
/// 验证新建镜像子任务继承上下文但不会注入仅用于显示的状态提示词。
#[test]
fn spawn_agent_mirror_uses_cloned_runtime_without_status_prompt() {
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
        "agent_mode": SUBAGENT_MODE_MIRROR
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

    assert_eq!(request.snapshot.messages.len(), 1);
    assert_eq!(request.snapshot.messages[0].id, "u1");
}

/// Verifies new task records capture the parent fixed agent prompt used by child runtime launches.
/// 验证新建任务记录会捕获子代理运行时启动使用的父级固定代理提示词。
#[test]
fn spawn_agent_record_captures_parent_agent_prompt_snapshot() {
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
    let mut parent_request = test_parent_execution_request(parent_session_id);
    parent_request.agent_prompt_snapshot =
        ExecutionAgentPromptSnapshot::fixed_parent_runtime("Parent creation-time agent prompt");
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    let _guard = task_runtime
        .install_execution_context(parent_session_id, context.clone())
        .expect("execution context should install");
    let args = parse_task_tool_arguments(json!({
        "prompt": "reply only: ok",
        "task_id": "fixed-prompt-child"
    }))
    .expect("default task arguments should parse");
    let resolved_identifier = ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id: String::from("fixed-prompt-child"),
        is_resumed: false,
        persisted_metadata: None,
    };

    let request =
        prepare_task_execution_request(&task_runtime, parent_session_id, resolved_identifier, args)
            .expect("default task execution request should be prepared");
    let record_prompt = task_runtime
        .sessions
        .read()
        .expect("task sessions should be readable")
        .get(&task_id)
        .and_then(|record| record.agent_prompt_snapshot.clone())
        .expect("task record should store the fixed parent prompt");
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Provider rendered preamble"),
        &request.invocation_config,
    );
    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &request, &resolved_config)
            .expect("valid task runtime preparation");
    let child_prompt = child_request
        .agent_prompt_snapshot
        .expect("child launch should receive the fixed parent prompt");

    assert_eq!(record_prompt.text, "Parent creation-time agent prompt");
    assert_eq!(child_prompt.text, record_prompt.text);
    assert_ne!(child_prompt.text, "Provider rendered preamble");
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
        "subagent_profile_id": "engineering/code-reviewer"
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

/// Verifies clone-latest runtime initialization fails when the parent runtime context is missing.
/// 验证缺失父级运行时上下文时克隆最新运行时初始化会失败。
#[test]
fn clone_latest_runtime_requires_registered_parent_context() {
    let task_runtime = TaskRuntime::new();
    let error = initial_spawn_snapshot_for_context_source(
        &task_runtime,
        Uuid::new_v4(),
        TaskInitialContextSource::CloneLatestRuntime,
    )
    .expect_err("clone-latest runtime source should reject missing parent context");

    assert!(error.contains("registered parent runtime context"));
}

/// Verifies child execution requests carry structured subagent prompt context.
/// 验证子智能体执行请求会携带结构化智能体提示词上下文。
#[test]
fn task_runtime_preparation_request_carries_subagent_prompt_context() {
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let mut parent_request = test_parent_execution_request(parent_session_id);
    parent_request.agent_prompt_snapshot =
        ExecutionAgentPromptSnapshot::fixed_parent_runtime("Parent fixed agent prompt");
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    let mut invocation = TaskExecutionRequest {
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
    let mut controller_config = build_test_controller_config("Base prompt");
    controller_config.system_prompt = String::from("Composed TaskSession prompt");
    invocation.controller_config = Some(controller_config);
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    let (_, child_request) =
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    let agent_prompt_snapshot = child_request
        .agent_prompt_snapshot
        .expect("child request should carry fixed parent runtime prompt snapshot");
    assert_eq!(agent_prompt_snapshot.text, "Parent fixed agent prompt");
    assert_ne!(agent_prompt_snapshot.text, "Composed TaskSession prompt");
    assert_eq!(
        agent_prompt_snapshot.source,
        crate::agent_runtime::execution_models::ExecutionAgentPromptSnapshotSource::FixedParentRuntime
    );
    let subagent_prompt_context = child_request
        .subagent_prompt_context
        .expect("child request should carry subagent prompt context");
    assert_eq!(subagent_prompt_context.identity, "reviewer");
    assert_eq!(subagent_prompt_context.delegated_task, "Review code");
    assert!(subagent_prompt_context.functional_system_prompt.is_none());
}

/// Verifies preparation reports a poisoned task registry instead of replacing the stored prompt snapshot with a fallback.
/// 验证准备阶段会报告中毒的任务注册表，而不是用回退值替换已存储的提示词快照。
#[test]
fn task_runtime_preparation_request_reports_poisoned_prompt_snapshot_registry() {
    // parentSessionId identifies the main execution context that provides the fallback snapshot.
    // parentSessionId 标识提供回退快照的主执行上下文。
    let parent_session_id = Uuid::new_v4();
    // taskId identifies the child whose authoritative task record cannot be read.
    // taskId 标识权威任务记录无法读取的子任务。
    let task_id = Uuid::new_v4();
    // providerConfig supplies the minimal runtime foundation required by request preparation.
    // providerConfig 提供请求准备所需的最小运行时基础配置。
    let provider_config = ProviderConfigurationFile::default();
    // foundation owns the task registry deliberately poisoned below.
    // foundation 持有下方刻意中毒的任务注册表。
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    // taskRuntime is the shared handle used by the preparation request to read task records.
    // taskRuntime 是准备请求读取任务记录时使用的共享句柄。
    let task_runtime = foundation.task_runtime();
    // poisonResult proves the task registry can no longer provide authoritative snapshot facts.
    // poisonResult 证明任务注册表已无法提供权威快照事实。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the registry locked until the deliberate panic poisons it.
        // guard 将注册表保持加锁到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .sessions
            .write()
            .expect("task-session registry should lock before poisoning");
        panic!("poison task-session registry for prompt snapshot preparation test");
    }));
    assert!(poison_result.is_err());
    // parentRequest contains a valid fallback that must not hide the failed authoritative lookup.
    // parentRequest 包含合法回退值，但它不得掩盖权威查询失败。
    let mut parent_request = test_parent_execution_request(parent_session_id);
    parent_request.agent_prompt_snapshot =
        ExecutionAgentPromptSnapshot::fixed_parent_runtime("Fallback parent agent prompt");
    // context carries the poisoned foundation into child request preparation.
    // context 将已中毒的运行时基础传入子请求准备流程。
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    // invocation describes the child execution whose stored prompt identity is authoritative.
    // invocation 描述已存储提示词身份具有权威性的子任务执行。
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
    // resolvedConfig supplies the effective child execution settings.
    // resolvedConfig 提供子任务执行的生效配置。
    let resolved_config = test_resolved_config(
        &build_test_controller_config("Base prompt"),
        &invocation.invocation_config,
    );

    // error is the explicit registry diagnostic returned before any fallback can be selected.
    // error 是选择任何回退值之前返回的明确注册表诊断。
    let error = build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
        .expect_err("poisoned task-session registry should reject request preparation");

    assert!(error.contains("task-session registry is locked"));
    assert!(error.contains(&task_id.to_string()));
}

/// Verifies child execution requests do not inherit provider context injection from the parent turn.
/// 验证子智能体执行请求不会继承父回合的 Provider 上下文注入。
#[test]
fn task_runtime_preparation_request_clears_parent_context_injection() {
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let mut parent_request = test_parent_execution_request(parent_session_id);
    parent_request.context_injection = Some(vulcan_core::types::ProviderContextInjection {
        persistent_blocks: vec![String::from("<memory>parent-only</memory>")],
        transient_prefix: String::new(),
        fingerprint: String::from("parent-injection"),
        ..vulcan_core::types::ProviderContextInjection::default()
    });
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("plain-child"),
        prompt: String::from("reply ok"),
        description: Some(String::from("Reply ok")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("reply ok"),
        display_title: String::from("Reply ok"),
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
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    assert!(child_request.context_injection.is_none());
}

/// Verifies TaskSession requests rebuild child-owned transient runtime state from scratch.
/// 验证 TaskSession 请求会从零重建子级拥有的临时运行态。
#[test]
fn task_runtime_preparation_request_clears_parent_transient_runtime_state() {
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let mut parent_request = test_parent_execution_request(parent_session_id);
    parent_request.tool_request_rejection_reason = Some(String::from("parent rejection"));
    parent_request.tool_request_rejection_all_tools = true;
    parent_request
        .tool_request_rejection_tool_ids
        .push(String::from("parent_tool"));
    parent_request
        .tool_request_hidden_tool_ids
        .push(String::from("parent_hidden_tool"));
    parent_request
        .boundary_events
        .push(vulcan_session_boundary::SessionBoundaryEvent::new(
            "parent-boundary",
            SessionNodeId::from(parent_session_id),
            1,
            vulcan_session_boundary::SessionBoundaryEventKind::TurnStarted,
            vulcan_session_boundary::SessionEventScope::Turn,
            vulcan_session_boundary::SessionEventVisibility::Hidden,
            1,
        ));
    parent_request
        .runtime_system_updates
        .push(vulcan_context::RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("parent-turn")),
            update_id: String::from("parent-update"),
            effective_after_event_id: Some(String::from("parent-boundary")),
            text: String::from("Parent-only dynamic system update."),
            trust: vulcan_context::RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: vulcan_context::RuntimeSystemUpdateLoweringHint::Auto,
            fingerprint: Some(String::from("parent-fingerprint")),
            source: Some(String::from("parent")),
        });
    parent_request.session_goal_context = Some(ExecutionSessionGoalContext {
        goal_id: String::from("goal-1"),
        prompt_context: String::from("Parent goal context"),
    });
    parent_request.session_plan_context = Some(String::from("Parent plan context"));
    parent_request
        .embedded_turn_usage_segments
        .push(SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::ContextCompaction,
            trigger: Some(String::from("parent")),
            execution_profile: None,
            provider_id: None,
            model_id: None,
            provider_call_count: 1,
            tool_call_count: 0,
            input_tokens: 10,
            output_tokens: 2,
            reasoning_tokens: 0,
            total_tokens: 12,
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        });
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("plain-child"),
        prompt: String::from("reply ok"),
        description: Some(String::from("Reply ok")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("reply ok"),
        display_title: String::from("Reply ok"),
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
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid task runtime preparation");

    assert!(child_request.conversation_hook_turn_id.is_none());
    assert!(child_request.tool_request_rejection_reason.is_none());
    assert!(!child_request.tool_request_rejection_all_tools);
    assert!(child_request.tool_request_rejection_tool_ids.is_empty());
    assert_eq!(
        child_request.tool_request_hidden_tool_ids,
        expected_child_forbidden_tool_ids()
    );
    assert!(child_request.boundary_events.is_empty());
    assert!(child_request.runtime_system_updates.is_empty());
    assert!(child_request.session_goal_context.is_none());
    assert!(child_request.session_plan_context.is_none());
    assert!(child_request.embedded_turn_usage_segments.is_empty());
}

/// Verifies a child request inherits the immutable knowledge snapshot without losing its storage handle.
/// 验证子请求继承不可变知识库快照且不会丢失其存储句柄。
#[test]
fn task_runtime_preparation_request_preserves_enabled_knowledge_access() {
    let parent_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let provider_config = ProviderConfigurationFile::default();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &provider_config,
        &AgentProfileCatalog::default(),
        None,
        None,
    );
    let mut parent_request = test_parent_execution_request(parent_session_id);
    let controller_client = ControllerClient::new(
        ControllerClientConfig {
            auto_spawn: false,
            ..ControllerClientConfig::default()
        },
        ClientRegistration {
            client_name: String::from("knowledge-child-test"),
            host_kind: String::from("test"),
            process_id: std::process::id(),
            process_name: String::from("vulcan-agent-runtime-test"),
            lease_ttl_secs: Some(60),
        },
    );
    parent_request.controller_runtime = Some(ControllerRuntimeHandle::new(
        controller_client,
        String::from("http://127.0.0.1:1"),
        String::from("unused"),
        std::env::temp_dir(),
        std::env::temp_dir().join("unused.sqlite3"),
        String::from("knowledge-child-test-binding"),
    ));
    parent_request.knowledge_access = Some(ExecutionKnowledgeAccessContext {
        project_id: String::from("project-a"),
        library_ids: vec![String::from("550e8400-e29b-41d4-a716-446655440000")],
    });
    let context = TaskExecutionRuntimeContext::new(
        provider_config,
        Arc::new(foundation),
        None,
        parent_request,
    );
    let invocation = TaskExecutionRequest {
        current_session_id: parent_session_id,
        root_session_id: parent_session_id,
        task_id,
        execution_id: String::from("knowledge-execution"),
        public_task_id: String::from("knowledge-child"),
        prompt: String::from("search project knowledge"),
        description: Some(String::from("Search project knowledge")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("search project knowledge"),
        display_title: String::from("Search project knowledge"),
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
        build_task_runtime_preparation_request(&context, &invocation, &resolved_config)
            .expect("valid knowledge-enabled task runtime preparation");

    let access = child_request
        .knowledge_access
        .expect("child should inherit knowledge access");
    assert_eq!(access.project_id, "project-a");
    assert_eq!(
        access.library_ids,
        vec![String::from("550e8400-e29b-41d4-a716-446655440000")]
    );
    assert!(child_request.controller_runtime.is_some());
}

/// Verifies stored child-task controller snapshots keep runtime knobs but not prompt material.
/// 验证已存储的子任务控制器快照会保留运行参数但不会保留提示词材料。
#[test]
fn build_task_controller_config_reuses_stored_controller_state_without_prompt() {
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

    assert!(config.system_prompt.is_empty());
    assert!(config.prompt_layers.is_empty());
    assert_eq!(config.provider_id, base.provider_id);
    assert_eq!(config.model_id, base.model_id);
    assert_eq!(config.temperature, stored.temperature);
    assert_eq!(config.max_turns, stored.max_turns);
    assert_eq!(config.additional_params, stored.additional_params);
}

/// Verifies parent controller preambles are not inherited as TaskSession prompt layers.
/// 验证父级控制器前导语不会作为 TaskSession 提示词层继承。
#[test]
fn task_parent_effective_config_ignores_rendered_controller_prompt() {
    let mut controller_config = build_test_controller_config("Provider rendered preamble");
    controller_config.system_prompt =
        String::from("Provider base\n\nProject rules\n\nTool catalog\n\nParent agent prompt");

    let effective_config = core_controller_config_to_effective_config(&controller_config);

    assert_eq!(
        effective_config.provider_model.provider_id.as_deref(),
        Some(controller_config.provider_id.as_str())
    );
    assert_eq!(
        effective_config.provider_model.model_id.as_deref(),
        Some(controller_config.model_id.as_str())
    );
    assert!(
        effective_config.system_prompt_layers.is_empty(),
        "provider-rendered controller preambles must not be inherited by child-agent prompt config"
    );
}

/// Verifies catalog expert prompts are extracted as functional subagent prompt text.
/// 验证目录专家提示词会提取为功能型智能体提示词文本。
#[test]
fn task_functional_system_prompt_extracts_expert_profile_prompt() {
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

    let prompt = task_functional_system_prompt(&base, &invocation_config, &resolved_config)
        .expect("expert profile prompt should become functional prompt text");

    assert!(prompt.contains("catalog code review expert"));
    assert!(!prompt.contains("Base prompt"));
}

/// Verifies default child-task controller config keeps controller concerns only.
/// 验证默认子任务控制器配置只保留控制器职责。
#[test]
fn build_task_controller_config_does_not_embed_subagent_prompt() {
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

    assert!(config.system_prompt.is_empty());
    assert!(config.prompt_layers.is_empty());
    assert!(!config.system_prompt.contains("Subagent identity"));
    assert!(!config.system_prompt.contains("Delegated task"));
}

/// Verifies invocation-level system prompt overrides become functional prompt text.
/// 验证调用级系统提示词覆盖会转为功能型提示词文本。
#[test]
fn task_functional_system_prompt_uses_invocation_system_prompt() {
    let base = build_test_controller_config("Base prompt");
    let mut invocation_config = TaskInvocationConfig::main("Review code");
    invocation_config.inherit_parent_config = Some(false);
    invocation_config.system_prompt = Some(String::from("You are a strict code reviewer."));
    invocation_config.system_prompt_append = Some(String::from("Focus only on correctness."));
    let resolved_config = test_resolved_config(&base, &invocation_config);

    let prompt = task_functional_system_prompt(&base, &invocation_config, &resolved_config)
        .expect("invocation prompt should become functional prompt text");

    assert!(!prompt.contains("Base prompt"));
    assert!(prompt.contains("You are a strict code reviewer."));
    assert!(prompt.contains("Focus only on correctness."));
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

/// Verifies provider-request counting starts after the latest completed task compaction marker.
/// 验证 Provider 请求计数会从最近一次已完成的子任务压缩标记之后开始。
#[test]
fn task_provider_request_counter_uses_latest_compaction_boundary() {
    let node_id = SessionNodeId::from(Uuid::new_v4());
    let events = vec![
        SessionEventRecord::new(
            "before-compaction",
            node_id,
            0,
            SessionEventKind::TaskTerminal,
            100,
            json!({ "provider_turns_executed": 2 }),
        ),
        SessionEventRecord::new(
            "compaction",
            node_id,
            1,
            SessionEventKind::RuntimeMarker,
            101,
            json!({
                "context_compaction_status": "completed",
                "provider_request_count": 99
            }),
        ),
        SessionEventRecord::new(
            "after-compaction-a",
            node_id,
            1,
            SessionEventKind::TaskTerminal,
            102,
            json!({ "provider_turns_executed": 3 }),
        ),
        SessionEventRecord::new(
            "after-compaction-b",
            node_id,
            2,
            SessionEventKind::TaskTerminal,
            103,
            json!({ "provider_turns_executed": 4 }),
        ),
    ];

    assert_eq!(
        task_provider_requests_since_latest_compaction_event(&events),
        7
    );
}

/// Verifies provider-request compaction requires automatic compaction and a positive threshold.
/// 验证 Provider 请求数压缩必须同时启用自动压缩并配置正数阈值。
#[test]
fn task_provider_request_limit_requires_auto_compaction_and_positive_limit() {
    let mut policy = ContextCompactionPolicySnapshot {
        auto_compaction_enabled: true,
        max_provider_requests: Some(3),
        ..Default::default()
    };

    assert!(task_provider_request_limit_is_enabled(&policy));
    assert!(!task_provider_request_limit_reached(&policy, 2));
    assert!(task_provider_request_limit_reached(&policy, 3));

    policy.auto_compaction_enabled = false;
    assert!(!task_provider_request_limit_is_enabled(&policy));
    assert!(!task_provider_request_limit_reached(&policy, 4));

    policy.auto_compaction_enabled = true;
    policy.max_provider_requests = Some(0);
    assert!(!task_provider_request_limit_is_enabled(&policy));
    assert!(!task_provider_request_limit_reached(&policy, 4));
}

/// Verifies shared-runtime task report conversion preserves the final assistant message id and cumulative usage.
/// 验证共享运行时任务报告转换会保留最终助手消息标识与累计用量。
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
    let last_provider_usage = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(120),
        output_tokens: Some(30),
        reasoning_tokens: Some(10),
        total_tokens: Some(160),
        cached_input_tokens: Some(40),
        cache_creation_input_tokens: None,
    };
    let cumulative_provider_usage = vulcan_core::types::ProviderUsageSnapshot {
        input_tokens: Some(220),
        output_tokens: Some(55),
        reasoning_tokens: Some(25),
        total_tokens: Some(300),
        cached_input_tokens: Some(80),
        cache_creation_input_tokens: None,
    };
    let report = ExecutionRunReport {
        session_id: task_id,
        preparation: None,
        assistant_output: Some(String::from("done")),
        thinking_output: None,
        conversation_state: Some(
            core_snapshot_to_session_conversation_state(&snapshot)
                .expect("valid task report conversation state"),
        ),
        provider_request_count: 2,
        last_successful_provider_usage: Some(last_provider_usage),
        cumulative_successful_provider_usage: Some(cumulative_provider_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: vec![vulcan_core::types::CoreEvent {
            kind: vulcan_core::types::CoreEventKind::ToolFinished,
            title: String::from("工具调用完成"),
            body: String::from("done"),
            metadata: None,
        }],
    };

    let outcome =
        task_execution_outcome_from_report(&invocation, controller_config, resolved_config, report)
            .expect("shared-runtime report should convert");

    assert_eq!(
        outcome.final_assistant_message_id.as_deref(),
        Some("assistant-final")
    );
    assert_eq!(outcome.final_output_text, "done");
    assert_eq!(outcome.tool_calls_processed, 1);
    assert_eq!(outcome.usage_segments.len(), 1);
    let segment = &outcome.usage_segments[0];
    assert_eq!(segment.kind, SessionTurnUsageSegmentKind::SubagentTask);
    assert_eq!(segment.trigger.as_deref(), Some("agent-review"));
    assert_eq!(segment.provider_call_count, 2);
    assert_eq!(segment.tool_call_count, 1);
    assert_eq!(segment.input_tokens, 220);
    assert_eq!(segment.output_tokens, 55);
    assert_eq!(segment.reasoning_tokens, 25);
    assert_eq!(segment.total_tokens, 300);
    assert_eq!(segment.cached_input_tokens, Some(80));
}

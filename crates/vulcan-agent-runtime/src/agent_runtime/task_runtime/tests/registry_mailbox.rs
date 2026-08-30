use super::*;
use std::path::PathBuf;

/// Builds one minimal catalog definition for orchestration-info tests.
/// 为调度信息测试构建一个最小目录定义。
fn test_subagent_catalog_definition(
    // Stable model-visible definition id.
    // 稳定的模型可见定义标识。
    id: &str,
    // Whether the definition should be exposed to spawn_agent.
    // 该定义是否应暴露给 spawn_agent。
    enabled: bool,
) -> SubagentDefinition {
    SubagentDefinition {
        id: id.to_string(),
        name: format!("{id} name"),
        description: Some(format!("{id} description")),
        locales: BTreeMap::new(),
        origin: None,
        emoji: String::from("AI"),
        color: String::from("#64748b"),
        vibe: None,
        mode: vulcan_task_session::SubagentDefinitionMode::Expert,
        system_prompt: Some(format!("{id} prompt")),
        system_prompt_append: None,
        provider_model: ProviderModelSnapshot::default(),
        tool_exposure: ToolExposureSnapshot::default(),
        prompt_skills: PromptSkillSnapshot::default(),
        context_compaction: ContextCompactionPolicyRequest::default(),
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
        max_depth: None,
        enabled,
        source: vulcan_task_session::SubagentDefinitionSource {
            scope: vulcan_task_session::SubagentDefinitionScope::System,
            file: PathBuf::from(format!("{id}.md")),
            sidecar_file: None,
            source_key: String::from("system"),
            category_id: Some(String::from("engineering")),
            package_id: None,
            remote_url: None,
            override_of: None,
            source_hash: None,
            version: None,
            etag: None,
            last_modified: None,
            updated_at: None,
        },
        extra: Value::Null,
    }
}

/// Installs a small catalog with one enabled and one disabled subagent definition.
/// 安装一个包含启用和禁用定义的小型子代理目录。
fn install_test_subagent_catalog(task_runtime: &TaskRuntime) {
    task_runtime
        .replace_subagent_catalog(SubagentCatalog {
            definitions: vec![
                test_subagent_catalog_definition("engineering/enabled-reviewer", true),
                test_subagent_catalog_definition("engineering/disabled-reviewer", false),
            ],
            ..SubagentCatalog::default()
        })
        .expect("test catalog should install");
}

/// Stores one running child-agent record so wait_agent tests exercise a real live target.
/// 保存一条运行中的子代理记录，使 wait_agent 测试覆盖真实活动目标。
///
/// # Parameters
/// 参数
/// `task_runtime` is the runtime that owns the child registry.
/// `task_runtime` 是拥有子代理注册表的运行时。
/// `root_session_id` is the direct parent session.
/// `root_session_id` 是直接父会话。
/// `public_task_id` is the stable model-visible child id.
/// `public_task_id` 是模型可见的稳定子代理标识。
///
/// # Returns
/// 返回值
/// The internal child session id stored as running.
/// 返回以运行态保存的内部子会话标识。
fn store_running_wait_agent_child(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    public_task_id: &str,
) -> SessionId {
    // task_id is the internal session identity for this live child.
    // task_id 是本活动子代理的内部会话身份。
    let task_id = Uuid::new_v4();
    // controller_config supplies the minimal executable runtime configuration required by the record.
    // controller_config 提供记录所需的最小可执行运行时配置。
    let controller_config = build_test_controller_config("Base prompt");
    // invocation owns the exact parent-child identities used by the registry projection.
    // invocation 持有注册表投影所需的精确父子身份。
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: format!("{public_task_id}-execution"),
        public_task_id: public_task_id.to_string(),
        prompt: String::from("keep running"),
        description: Some(String::from("Running wait target")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("keep running"),
        display_title: public_task_id.to_string(),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    // outcome records an admitted child that has not produced terminal feedback.
    // outcome 记录已经准入但尚未产出终态反馈的子代理。
    let outcome = TaskExecutionOutcome {
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
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &outcome)
        .expect("running wait target should store");
    task_id
}

/// Verifies agent_orchestration_info exposes enabled agents by default.
/// 验证 agent_orchestration_info 默认只暴露已启用智能体。
#[tokio::test]
async fn agent_orchestration_info_lists_enabled_agents_by_default() {
    let task_runtime = TaskRuntime::new();
    install_test_subagent_catalog(&task_runtime);

    let result = task_runtime
        .invoke_agent_orchestration_info(
            Uuid::new_v4(),
            json!({
                "sections": ["agents"]
            }),
        )
        .await
        .expect("orchestration info should query enabled catalog agents");

    let agents = result.model_output["agents"]
        .as_array()
        .expect("agents should be an array");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["agent_id"], "engineering/enabled-reviewer");
    assert_eq!(agents[0]["enabled"], true);
}

/// Verifies agent_orchestration_info rejects disabled-agent diagnostic exposure.
/// 验证 agent_orchestration_info 会拒绝暴露禁用智能体的诊断参数。
#[tokio::test]
async fn agent_orchestration_info_rejects_include_disabled() {
    let task_runtime = TaskRuntime::new();
    install_test_subagent_catalog(&task_runtime);

    let error = task_runtime
        .invoke_agent_orchestration_info(
            Uuid::new_v4(),
            json!({
                "sections": ["agents"],
                "include_disabled": true,
                "query": "reviewer"
            }),
        )
        .await
        .expect_err("include_disabled should not be model-visible");
    assert!(error.contains("unsupported keys"));
}

/// Verifies TaskRuntime publishes subagent mailbox lifecycle events on real status transitions.
/// 验证 TaskRuntime 会在真实状态变化时发布 subagent mailbox 生命周期事件。
#[test]
fn task_runtime_publishes_subagent_mailbox_events() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let controller_config = build_test_controller_config("Base prompt");
    let resolved_config = test_resolved_config(&controller_config, &invocation.invocation_config);
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: resolved_config.clone(),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(120),
            uncached_input_tokens: None,
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            ..vulcan_core::types::ProviderUsageSnapshot::default()
        }),
    );
    let completed_outcome = TaskExecutionOutcome {
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        provider_turns_executed: 1,
        final_assistant_message_id: Some(String::from("assistant-1")),
        usage_segments: vec![usage_segment],
        ..running_outcome.clone()
    };

    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("running task should store");
    assert!(
        !task_runtime
            .has_subagent_events_for_parent(SessionNodeId::from(root_session_id))
            .expect("mailbox pending check should succeed"),
        "running state should not enter the parent mailbox"
    );
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &completed_outcome)
        .expect("completed task should store");

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should drain");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, SubagentMailboxEventKind::Completed);
    assert_eq!(events[0].child_thread_id, SessionNodeId::from(task_id));
    assert_eq!(events[0].payload["agent_kind"], "parent_clone");
    assert_eq!(events[0].payload["execution_id"], "test-execution");
    assert!(events[0].payload.get("agent_profile").is_none());
    assert_eq!(events[0].payload["result_text"], "review complete");
    assert_eq!(events[0].payload["usage_segments"][0]["input_tokens"], 120);
    assert_eq!(
        events[0].payload["usage_segments"][0]["cached_input_tokens"],
        40
    );
}

/// Verifies expert profile invocations are reported as agent-profile subagents.
/// 验证专家配置调用会按 agent-profile 子代理上报。
#[test]
fn task_runtime_reports_expert_profile_mailbox_kind() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config =
        TaskInvocationConfig::with_profile("engineering/code-reviewer", "review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("completed expert task should store");

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should drain");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["agent_kind"], "agent_profile");
}

/// Verifies TaskRuntime mirrors task-session lifecycle changes into the native subagent registry.
/// 验证 TaskRuntime 会把任务会话生命周期变化镜像到原生子代理注册表。
#[test]
fn task_runtime_mirrors_lifecycle_into_subagent_registry() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let controller_config = build_test_controller_config("Base prompt");
    let resolved_config = test_resolved_config(&controller_config, &invocation.invocation_config);
    let running_outcome = TaskExecutionOutcome {
        task_id,
        controller_config: controller_config.clone(),
        resolved_config: resolved_config.clone(),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };
    let completed_outcome = TaskExecutionOutcome {
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        provider_turns_executed: 1,
        final_assistant_message_id: Some(String::from("assistant-1")),
        ..running_outcome.clone()
    };

    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &running_outcome)
        .expect("running task should store");
    let running_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let running_entry = running_registry
        .get_for_parent(SessionNodeId::from(root_session_id), "agent-review")
        .expect("running registry entry should exist");
    assert_eq!(running_entry.status, SubagentStatus::Running);
    assert_eq!(running_entry.agent_kind, SubagentKind::Clone);
    assert_eq!(running_entry.prompt_source.as_deref(), Some("core"));

    task_runtime
        .store_task_outcome(root_session_id, &invocation, &completed_outcome)
        .expect("completed task should store");
    let completed_registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let completed_entry = completed_registry
        .get_for_parent(SessionNodeId::from(root_session_id), "agent-review")
        .expect("completed registry entry should exist");
    assert_eq!(completed_entry.status, SubagentStatus::Completed);
    assert_eq!(
        completed_entry.result_summary.as_deref(),
        Some("review complete")
    );
    assert_eq!(
        completed_registry.running_count_for_parent(SessionNodeId::from(root_session_id)),
        0
    );
}

/// Verifies expert profile invocations are mirrored into the native registry as agent-md entries.
/// 验证专家配置调用会以 agent-md 条目镜像到原生注册表。
#[test]
fn task_runtime_mirrors_expert_profile_into_subagent_registry() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config =
        TaskInvocationConfig::with_profile("engineering/code-reviewer", "review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let controller_config = build_test_controller_config("Base prompt");
    let outcome = TaskExecutionOutcome {
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
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &outcome)
        .expect("running expert task should store");
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get_for_parent(SessionNodeId::from(root_session_id), "agent-review")
        .expect("running expert registry entry should exist");

    assert_eq!(entry.agent_kind, SubagentKind::AgentMd);
    assert_eq!(
        entry.prompt_source.as_deref(),
        Some("agent_md:engineering/code-reviewer")
    );
}

/// Verifies custom system-prompt subagents are mirrored as custom registry entries.
/// 验证自定义系统提示词子代理会以自定义类型镜像到注册表。
#[test]
fn task_runtime_mirrors_custom_prompt_into_subagent_registry() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.system_prompt = Some(String::from("You are a focused reviewer."));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id,
        execution_id: String::from("test-execution"),
        public_task_id: String::from("custom-review"),
        prompt: String::from("review this change"),
        description: Some(String::from("Review code")),
        subagent: Some(String::from("custom reviewer")),
        invocation_config,
        display_title: String::from("Review code (custom reviewer)"),
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
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };

    task_runtime
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &outcome)
        .expect("running custom task should store");
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read");
    let entry = registry
        .get_for_parent(SessionNodeId::from(root_session_id), "custom-review")
        .expect("running custom registry entry should exist");

    assert_eq!(entry.agent_kind, SubagentKind::Custom);
}

/// Verifies list_agents exposes the Vulcan root subagent tree with path filtering.
/// 验证 list_agents 会暴露 Vulcan 根子代理树并支持路径过滤。
#[tokio::test]
async fn list_agents_prefers_native_subagent_registry_payload() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let nested_task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let controller_config = build_test_controller_config("Base prompt");
    let outcome = TaskExecutionOutcome {
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
        .store_running_task_outcome_in_memory(root_session_id, &invocation, &outcome)
        .expect("running task outcome should store");
    let nested_invocation = TaskExecutionRequest {
        current_session_id: task_id,
        root_session_id,
        task_id: nested_task_id,
        execution_id: String::from("test-execution-nested"),
        public_task_id: String::from("agent-review"),
        prompt: String::from("nested review"),
        description: Some(String::from("Nested review")),
        subagent: Some(String::from("reviewer")),
        invocation_config: TaskInvocationConfig::main("nested review"),
        display_title: String::from("Nested review (reviewer)"),
        parent_task_id: Some(task_id),
        depth: 2,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let nested_outcome = TaskExecutionOutcome {
        task_id: nested_task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(
            &controller_config,
            &nested_invocation.invocation_config,
        ),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_running_task_outcome_in_memory(task_id, &nested_invocation, &nested_outcome)
        .expect("nested running task outcome should store");
    task_runtime
        .agent_inboxes
        .write()
        .expect("inboxes should write")
        .insert(
            task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("same-name-root-message"),
                author_session_id: root_session_id,
                target_task_id: task_id,
                target_agent_id: String::from("agent-review"),
                content: String::from("root-scoped queued work"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }]),
        );

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({ "include_closed": true }))
        .await
        .expect("list_agents should succeed");

    assert_eq!(
        result.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        2
    );
    assert_eq!(
        result.model_output["agents"][0]["agent_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["agent_status"], "running");
    assert_eq!(
        result.model_output["agents"][0]["last_task_message"],
        "review this change"
    );
    assert_eq!(result.model_output["agents"][0]["agent_id"], "agent-review");
    assert_eq!(result.model_output["agents"][0]["lifecycle_state"], "open");
    assert_eq!(result.model_output["agents"][0]["run_state"], "queued");
    assert_eq!(result.model_output["agents"][0]["status"], "running");
    assert_eq!(result.model_output["agents"][0]["queued_message_count"], 1);
    assert_eq!(
        result.model_output["agents"][0]["tree_path"],
        "/root/agent-review"
    );
    assert_eq!(
        result.model_output["agents"][1]["agent_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][1]["queued_message_count"], 0);
    assert_eq!(result.model_output["agents"][1]["run_state"], "running");
    assert_eq!(
        result.model_output["agents"][1]["tree_path"],
        "/root/agent-review/agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["can_send"], true);
    assert_eq!(result.model_output["agents"][0]["can_interrupt"], true);
    assert_eq!(result.model_output["agents"][0]["can_close"], true);
    assert_eq!(result.model_output["agents"][0]["can_wait"], true);
    assert!(result.model_output["agents"][0].get("agent_kind").is_none());
    assert!(
        result.model_output["agents"][0]
            .get("prompt_source")
            .is_none()
    );
    assert!(result.model_output["agents"][0].get("result").is_none());
    let host_result = result
        .host_result
        .as_ref()
        .expect("host result should exist");
    assert_eq!(host_result.payload["agents"][0]["agent_kind"], "clone");
    assert_eq!(host_result.payload["agents"][0]["prompt_source"], "core");
    assert_eq!(
        host_result.payload["agents"][0]["parent_session_id"],
        root_session_id.to_string()
    );
    assert_eq!(
        host_result.payload["agents"][1]["parent_session_id"],
        task_id.to_string()
    );
    assert_eq!(
        host_result.payload["agents"][0]["tree_path"],
        "/root/agent-review"
    );
    assert_eq!(
        host_result.payload["agents"][1]["tree_path"],
        "/root/agent-review/agent-review"
    );
    assert_eq!(
        host_result.payload["agents"][0][SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );

    let filtered = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_closed": true, "path_prefix": "/root/agent-review" }),
        )
        .await
        .expect("list_agents path_prefix filter should succeed");
    assert_eq!(
        filtered.model_output["agents"][0]["agent_name"],
        "agent-review"
    );
    assert_eq!(
        filtered.model_output["agents"][1]["agent_name"],
        "agent-review"
    );

    let nested_only = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_closed": true, "path_prefix": "/root/agent-review/agent-review" }),
        )
        .await
        .expect("list_agents nested path_prefix filter should succeed");
    assert_eq!(
        nested_only.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        1
    );
    assert_eq!(
        nested_only.model_output["agents"][0]["agent_name"],
        "agent-review"
    );

    let empty = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_closed": true, "path_prefix": "/root/other" }),
        )
        .await
        .expect("list_agents non-matching path_prefix should succeed");
    assert_eq!(
        empty.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        0
    );
}

/// Verifies completed registry entries remain visible to list_agents.
/// 验证完成态注册表条目仍会被 list_agents 返回。
#[tokio::test]
async fn list_agents_keeps_completed_registry_entries_visible() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("completed task outcome should store");

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({ "include_closed": true }))
        .await
        .expect("list_agents should succeed");

    assert_eq!(
        result.model_output["agents"][0]["agent_name"],
        "agent-review"
    );
    assert_eq!(
        result.model_output["agents"][0]["agent_status"]["completed"],
        "review complete"
    );
    assert_eq!(
        result.model_output["agents"][0]["last_task_message"],
        "review this change"
    );
    assert!(result.model_output["agents"][0].get("result").is_none());
}

/// Verifies list_agents returns registry status without consuming mailbox updates.
/// 验证 list_agents 返回注册表状态且不会消费 mailbox 更新。
#[tokio::test]
async fn list_agents_does_not_consume_pending_mailbox_updates() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("completed task outcome should store");

    let first_listing = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("first list_agents status query should succeed");

    assert_eq!(
        first_listing.model_output["agents"][0]["agent_name"],
        "agent-review"
    );
    assert_eq!(
        first_listing.model_output["agents"][0]["agent_status"]["completed"],
        "review complete"
    );

    let second_listing = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("second list_agents status query should succeed");

    assert_eq!(
        second_listing.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        1
    );
    assert!(
        task_runtime
            .has_subagent_events_for_parent(SessionNodeId::from(root_session_id))
            .expect("mailbox pending check should succeed")
    );
}

/// Verifies registry-only historical entries are hidden from list_agents once not resident.
/// 验证仅存在于历史注册表且不再常驻的条目不会被 list_agents 暴露。
#[tokio::test]
async fn list_agents_hides_registry_only_non_resident_entries() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let historical_task_id = Uuid::new_v4();
    let mut entry = SubagentRegistryEntry::new(
        "historical-review",
        SessionNodeId::from(historical_task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    entry.status = SubagentStatus::Completed;
    entry.result_summary = Some(String::from("historical result"));
    task_runtime
        .subagent_registry
        .write()
        .expect("registry should write")
        .upsert(entry)
        .expect("historical registry entry should insert");

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({ "include_closed": true }))
        .await
        .expect("list_agents should succeed");

    assert_eq!(
        result.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        0
    );
}

/// Verifies the registry rejects duplicate session ownership at the write boundary.
/// 验证注册表会在写入边界拒绝重复会话归属。
#[test]
fn subagent_registry_rejects_duplicate_session_ownership_at_write_boundary() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let duplicated_task_id = Uuid::new_v4();
    let timestamp_ms = current_unix_millis_u64();
    let first_entry = SubagentRegistryEntry::new(
        "first-owner",
        SessionNodeId::from(duplicated_task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        timestamp_ms,
    );
    let second_entry = SubagentRegistryEntry::new(
        "second-owner",
        SessionNodeId::from(duplicated_task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        timestamp_ms,
    );
    {
        let mut registry = task_runtime
            .subagent_registry
            .write()
            .expect("registry should write");
        registry
            .upsert(first_entry)
            .expect("first session owner should insert");
        let error = registry
            .upsert(second_entry)
            .expect_err("second session owner should be rejected");

        assert!(matches!(
            error,
            vulcan_session_core::SubagentRegistryError::DuplicateSessionId {
                session_id,
                first_parent_session_id,
                first_agent_id,
                second_parent_session_id,
                second_agent_id,
            } if session_id == SessionNodeId::from(duplicated_task_id)
                && first_parent_session_id == SessionNodeId::from(root_session_id)
                && first_agent_id == "first-owner"
                && second_parent_session_id == SessionNodeId::from(root_session_id)
                && second_agent_id == "second-owner"
        ));
        assert!(
            registry
                .get_for_parent(SessionNodeId::from(root_session_id), "first-owner")
                .is_some()
        );
        assert!(
            registry
                .get_for_parent(SessionNodeId::from(root_session_id), "second-owner")
                .is_none()
        );
    }
}

/// Verifies close-agent preparation rejects a target missing from the current registry snapshot.
/// 验证 close_agent 准备阶段会拒绝当前注册表快照中缺失的目标。
#[test]
fn close_agent_preparation_rejects_missing_registry_root() {
    let task_runtime = TaskRuntime::new();
    let missing_session_id = SessionNodeId::new_v4();

    let error = task_runtime
        .prepare_close_agent_archive_entries(missing_session_id)
        .expect_err("missing close target must not become an empty archive transaction");

    assert_eq!(
        error,
        format!("Subagent registry subtree root session `{missing_session_id}` does not exist.")
    );
}

/// Verifies close-agent commit preserves fields finalized after archive preparation.
/// 验证 close_agent 提交会保留归档准备后才完成写入的字段。
#[test]
fn close_agent_commit_derives_archive_from_latest_registry_entry() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut entry = SubagentRegistryEntry::new(
        "latest-result",
        SessionNodeId::from(task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    entry.status = SubagentStatus::Completed;
    entry.result_summary = Some(String::from("summary before stop"));
    entry.last_task_message = Some(String::from("message before stop"));
    task_runtime
        .subagent_registry
        .write()
        .expect("registry should write")
        .upsert(entry)
        .expect("initial registry entry should insert");

    let prepared_archive = task_runtime
        .prepare_close_agent_archive_entries(SessionNodeId::from(task_id))
        .expect("close archive should prepare");
    let finalized_at = prepared_archive.entries[0].updated_at_ms.saturating_add(10);
    {
        let mut registry = task_runtime
            .subagent_registry
            .write()
            .expect("registry should write latest task outcome");
        let mut latest_entry = registry
            .get_for_parent(SessionNodeId::from(root_session_id), "latest-result")
            .cloned()
            .expect("prepared entry should still exist");
        latest_entry.set_status(SubagentStatus::Completed, finalized_at);
        latest_entry.result_summary = Some(String::from("summary finalized during stop"));
        latest_entry.last_task_message = Some(String::from("message finalized during stop"));
        registry
            .upsert(latest_entry)
            .expect("latest task outcome should update the exact registry slot");
    }

    let committed_archive = task_runtime
        .commit_close_agent_runtime_archive(&prepared_archive)
        .expect("runtime archive should commit from the latest entry");

    assert_eq!(committed_archive.entries.len(), 1);
    assert_eq!(
        committed_archive.entries[0].status,
        SubagentStatus::Archived
    );
    assert_eq!(committed_archive.entries[0].updated_at_ms, finalized_at);
    assert_eq!(
        committed_archive.entries[0].result_summary.as_deref(),
        Some("summary finalized during stop")
    );
    assert_eq!(
        committed_archive.entries[0].last_task_message.as_deref(),
        Some("message finalized during stop")
    );
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should remain readable after commit");
    let archived_entry = registry
        .get_for_parent(SessionNodeId::from(root_session_id), "latest-result")
        .expect("committed archived entry should remain in its exact slot");
    assert_eq!(archived_entry.status, SubagentStatus::Archived);
    assert_eq!(
        archived_entry.result_summary.as_deref(),
        Some("summary finalized during stop")
    );
}

/// Verifies close_agent archives the selected child-agent subtree by default.
/// 验证 close_agent 默认会归档选中的子代理子树。
#[tokio::test]
async fn close_agent_archives_target_and_descendants_by_default() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let parent_task_id = Uuid::new_v4();
    let child_task_id = Uuid::new_v4();
    let mut parent_entry = SubagentRegistryEntry::new(
        "parent-review",
        SessionNodeId::from(parent_task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    parent_entry.status = SubagentStatus::Completed;
    let mut child_entry = SubagentRegistryEntry::new(
        "child-review",
        SessionNodeId::from(child_task_id),
        SessionNodeId::from(parent_task_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    child_entry.status = SubagentStatus::Completed;
    {
        let mut registry = task_runtime
            .subagent_registry
            .write()
            .expect("registry should write");
        registry
            .upsert(parent_entry)
            .expect("parent registry entry should insert");
        registry
            .upsert(child_entry)
            .expect("child registry entry should insert");
    }
    {
        let mut inboxes = task_runtime
            .agent_inboxes
            .write()
            .expect("inboxes should write");
        inboxes.insert(
            parent_task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("test-delivery-parent"),
                author_session_id: root_session_id,
                target_task_id: parent_task_id,
                target_agent_id: String::from("parent-review"),
                content: String::from("queued parent work"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }]),
        );
        inboxes.insert(
            child_task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("test-delivery-child"),
                author_session_id: parent_task_id,
                target_task_id: child_task_id,
                target_agent_id: String::from("child-review"),
                content: String::from("queued child work"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }]),
        );
    }

    let close_result = task_runtime
        .invoke_close_agent(root_session_id, json!({ "target": "parent-review" }))
        .await
        .expect("close_agent should close the selected subtree");
    assert_eq!(close_result.model_output["cleared_message_count"], 2);
    let closed_agents = close_result.model_output["closed_agents"]
        .as_array()
        .expect("closed agents should be an array")
        .iter()
        .map(|value| value.as_str().unwrap_or_default().to_string())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        closed_agents,
        BTreeSet::from([String::from("child-review"), String::from("parent-review")])
    );
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after close");
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(root_session_id), "parent-review")
            .expect("parent should exist")
            .status,
        SubagentStatus::Archived
    );
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(root_session_id), "parent-review")
            .expect("parent should exist")
            .stop_reason
            .as_deref(),
        Some("close_agent")
    );
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(parent_task_id), "child-review")
            .expect("child should exist")
            .status,
        SubagentStatus::Archived
    );
    assert!(
        task_runtime
            .agent_inboxes
            .read()
            .expect("inboxes should read")
            .is_empty()
    );
    let closed_list = task_runtime
        .invoke_list_agents(root_session_id, json!({ "include_closed": true }))
        .await
        .expect("closed agents should be visible for diagnostics");
    assert_eq!(
        closed_list.model_output["agents"][0]["lifecycle_state"],
        "closed"
    );
    assert_eq!(closed_list.model_output["agents"][0]["run_state"], "closed");
    assert_eq!(
        closed_list.model_output["agents"][0]["stop_reason"],
        "close_agent"
    );
    assert_eq!(closed_list.model_output["agents"][0]["can_send"], false);
    assert_eq!(
        closed_list.model_output["agents"][0]["can_interrupt"],
        false
    );
    assert_eq!(closed_list.model_output["agents"][0]["can_close"], false);
    assert_eq!(closed_list.model_output["agents"][0]["can_wait"], false);

    let late_invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id: parent_task_id,
        execution_id: String::from("test-execution-late"),
        public_task_id: String::from("parent-review"),
        prompt: String::from("late completed work"),
        description: Some(String::from("Parent review")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("late completed work"),
        display_title: String::from("Parent review"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let controller_config = build_test_controller_config("Base prompt");
    let late_outcome = TaskExecutionOutcome {
        task_id: parent_task_id,
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
        .expect("late outcome should not reopen archived agents");
    let registry_after_late_outcome = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after late outcome");
    assert_eq!(
        registry_after_late_outcome
            .get_for_parent(SessionNodeId::from(root_session_id), "parent-review")
            .expect("parent should still exist")
            .status,
        SubagentStatus::Archived
    );
    let default_list_after_late_outcome = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("default list should remain closed-filtered");
    assert_eq!(
        default_list_after_late_outcome.model_output["agents"]
            .as_array()
            .expect("agents should be an array")
            .len(),
        0
    );

    let repeat_error = task_runtime
        .invoke_close_agent(root_session_id, json!({ "target": "parent-review" }))
        .await
        .expect_err("closing an archived agent should fail");
    assert!(repeat_error.contains("already closed"));
}

/// Verifies close_agent rejects the removed cascade switch before closing the complete subtree.
/// 验证 close_agent 会先拒绝已移除的级联开关，再关闭完整子树。
#[tokio::test]
async fn close_agent_rejects_non_cascade_orphans_and_closes_active_subtree() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let parent_task_id = Uuid::new_v4();
    let child_task_id = Uuid::new_v4();
    let mut parent_entry = SubagentRegistryEntry::new(
        "non-cascade-parent",
        SessionNodeId::from(parent_task_id),
        SessionNodeId::from(root_session_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    parent_entry.status = SubagentStatus::Running;
    let mut child_entry = SubagentRegistryEntry::new(
        "non-cascade-child",
        SessionNodeId::from(child_task_id),
        SessionNodeId::from(parent_task_id),
        SubagentKind::Clone,
        current_unix_millis_u64(),
    );
    child_entry.status = SubagentStatus::Running;
    {
        let mut registry = task_runtime
            .subagent_registry
            .write()
            .expect("registry should write");
        registry
            .upsert(parent_entry)
            .expect("parent registry entry should insert");
        registry
            .upsert(child_entry)
            .expect("child registry entry should insert");
    }
    let _parent_run = task_runtime
        .reserve_active_task_run(parent_task_id, "non-cascade-parent-execution")
        .expect("parent active run should reserve");
    let _child_run = task_runtime
        .reserve_active_task_run(child_task_id, "non-cascade-child-execution")
        .expect("child active run should reserve");
    {
        let mut inboxes = task_runtime
            .agent_inboxes
            .write()
            .expect("inboxes should write");
        inboxes.insert(
            parent_task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("non-cascade-parent-message"),
                author_session_id: root_session_id,
                target_task_id: parent_task_id,
                target_agent_id: String::from("non-cascade-parent"),
                content: String::from("parent message"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }]),
        );
        inboxes.insert(
            child_task_id,
            VecDeque::from([QueuedInterAgentMessage {
                delivery_id: String::from("non-cascade-child-message"),
                author_session_id: parent_task_id,
                target_task_id: child_task_id,
                target_agent_id: String::from("non-cascade-child"),
                content: String::from("child message"),
                created_at_ms: current_unix_millis_u64(),
                continues_task_turn: false,
            }]),
        );
    }

    let unsupported_error = task_runtime
        .invoke_close_agent(
            root_session_id,
            json!({ "target": "non-cascade-parent", "cascade": false }),
        )
        .await
        .expect_err("removed cascade switch should be rejected");
    assert!(unsupported_error.contains("unsupported keys: cascade"));
    {
        let active_runs = task_runtime
            .active_task_runs
            .read()
            .expect("active runs should read after rejection");
        assert!(!active_runs[&parent_task_id].stop_requested);
        assert!(!active_runs[&child_task_id].stop_requested);
    }
    let registry_after_rejection = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after rejection");
    assert_eq!(
        registry_after_rejection
            .get_for_parent(SessionNodeId::from(root_session_id), "non-cascade-parent",)
            .expect("parent should remain registered after rejection")
            .status,
        SubagentStatus::Running
    );
    assert_eq!(
        registry_after_rejection
            .get_for_parent(SessionNodeId::from(parent_task_id), "non-cascade-child",)
            .expect("child should remain registered after rejection")
            .status,
        SubagentStatus::Running
    );

    let close_result = task_runtime
        .invoke_close_agent(root_session_id, json!({ "target": "non-cascade-parent" }))
        .await
        .expect("subtree close should succeed");

    assert_eq!(close_result.model_output["closed_count"], 2);
    let closed_agents = close_result.model_output["closed_agents"]
        .as_array()
        .expect("closed agents should be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        closed_agents,
        BTreeSet::from(["non-cascade-parent", "non-cascade-child"])
    );
    let host_result = close_result
        .host_result
        .expect("close result should retain host metadata");
    assert_eq!(host_result.payload["stopped_count"], 2);
    let stopped_task_ids = host_result.payload[STOPPED_SUBAGENT_TASK_IDS_KEY]
        .as_array()
        .expect("stopped task ids should be an array")
        .iter()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stopped_task_ids,
        BTreeSet::from([parent_task_id.to_string(), child_task_id.to_string()])
    );
    let active_runs = task_runtime
        .active_task_runs
        .read()
        .expect("active runs should read after close");
    assert!(
        active_runs
            .get(&parent_task_id)
            .expect("parent run should remain resident until its guard settles")
            .stop_requested
    );
    assert!(active_runs[&child_task_id].stop_requested);
    drop(active_runs);
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should read after close");
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(root_session_id), "non-cascade-parent",)
            .expect("parent should remain registered")
            .status,
        SubagentStatus::Archived
    );
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(parent_task_id), "non-cascade-child",)
            .expect("child should remain registered")
            .status,
        SubagentStatus::Archived
    );
    let inboxes = task_runtime
        .agent_inboxes
        .read()
        .expect("inboxes should read after close");
    assert!(inboxes.is_empty());
}

/// Verifies interrupt_agent reports the last known status for completed agents.
/// 验证 interrupt_agent 会返回已完成代理的最近已知状态。
#[tokio::test]
async fn interrupt_agent_reports_completed_status_for_known_completed_agent() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("completed task outcome should store");

    let result = task_runtime
        .invoke_interrupt_agent(root_session_id, json!({ "target": "agent-review" }))
        .await
        .expect("interrupt_agent should succeed for a known completed agent");

    assert_eq!(result.model_output["previous_status"], "completed");
    assert_ne!(result.model_output["previous_status"], "unknown");
}

/// Verifies interrupt_agent cannot project a stale stop over an archived child session.
/// 验证 interrupt_agent 不能用过期停止状态覆盖已归档子会话。
#[tokio::test]
async fn interrupt_agent_rejects_archived_child_session() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut entry = SubagentRegistryEntry::new(
        "archived-review",
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
        .expect("registry should write")
        .upsert(entry)
        .expect("archived child fixture should insert");

    let error = task_runtime
        .invoke_interrupt_agent(root_session_id, json!({ "target": "archived-review" }))
        .await
        .expect_err("archived child should reject interrupt_agent");

    assert!(error.contains("archived"));
    let registry = task_runtime
        .subagent_registry_snapshot()
        .expect("registry should remain readable");
    assert_eq!(
        registry
            .get_for_parent(SessionNodeId::from(root_session_id), "archived-review")
            .expect("archived child should remain registered")
            .status,
        SubagentStatus::Archived
    );
}

/// Verifies interrupt_agent reports explicit errors for invalid targets.
/// 验证 interrupt_agent 会对无效目标返回明确错误。
#[tokio::test]
async fn interrupt_agent_rejects_root_self_and_unknown_targets() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let child_task_id = Uuid::new_v4();
    {
        let mut sessions = task_runtime
            .sessions
            .write()
            .expect("task session registry should be writable");
        sessions.insert(
            child_task_id,
            build_task_session_record(TaskSessionRecordInit {
                task_id: child_task_id,
                public_task_id: String::from("worker"),
                origin_session_id: root_session_id,
                parent_task_id: None,
                depth: 1,
                description: None,
                subagent: None,
                invocation_config: TaskInvocationConfig::main("worker prompt"),
                display_title: String::from("worker"),
                controller_config: None,
                snapshot: CoreConversationSnapshot::default(),
                last_prompt: String::from("worker prompt"),
                last_status: String::from("running"),
            }),
        );
    }

    let root_error = task_runtime
        .invoke_interrupt_agent(root_session_id, json!({ "target": "/root" }))
        .await
        .expect_err("interrupt_agent should reject root");
    assert!(root_error.contains("root is not a spawned agent"));

    let self_error = task_runtime
        .invoke_interrupt_agent(child_task_id, json!({ "target": "worker" }))
        .await
        .expect_err("interrupt_agent should reject self");
    assert!(self_error.contains("cannot interrupt itself"));

    let unknown_error = task_runtime
        .invoke_interrupt_agent(root_session_id, json!({ "target": "missing-worker" }))
        .await
        .expect_err("interrupt_agent should reject unknown target");
    assert!(unknown_error.contains("Unknown child agent"));
}

/// Verifies interrupt-mode delivery aborts the current run and resumes the same child with the admitted message.
/// 验证插队模式投递会中止当前运行，并携带已接纳消息续跑同一子代理。
#[tokio::test]
async fn inter_agent_interrupt_aborts_then_resumes_with_admitted_message() {
    // task_runtime owns the child registry, active-run reservation, inbox, and resume backend under test.
    // task_runtime 持有本测试中的子代理注册表、活动运行预留、inbox 与续跑后端。
    let task_runtime = Arc::new(TaskRuntime::new());
    // root_session_id is the direct parent that sends the interrupt message.
    // root_session_id 是发送插队消息的直接父会话。
    let root_session_id = Uuid::new_v4();
    // task_id identifies the already-running child that must be interrupted and resumed.
    // task_id 标识必须被中止并续跑的既有运行中子代理。
    let task_id = store_running_wait_agent_child(&task_runtime, root_session_id, "worker-a");
    // captured_snapshot proves the admitted message reaches the successor provider context.
    // captured_snapshot 证明已接纳消息进入后继执行的 Provider 上下文。
    let captured_snapshot = Arc::new(std::sync::Mutex::new(None::<CoreConversationSnapshot>));
    // captured_snapshot_for_handler transfers the assertion sink into the exact parent backend.
    // captured_snapshot_for_handler 将断言接收器传入精确父会话后端。
    let captured_snapshot_for_handler = Arc::clone(&captured_snapshot);
    // backend_guard keeps the exact parent backend installed through the successor resume.
    // backend_guard 在后继续跑期间保持精确父会话后端已安装。
    let _backend_guard = task_runtime
        .install_backend(
            root_session_id,
            Arc::new(move |request: TaskExecutionRequest| {
                *captured_snapshot_for_handler
                    .lock()
                    .expect("captured snapshot lock") = Some(request.snapshot.clone());
                Box::pin(async move {
                    // controller_config supplies the completed successor outcome required by persistence.
                    // controller_config 提供持久化所需的已完成后继执行结果配置。
                    let controller_config = build_test_controller_config("Interrupt follow-up");
                    Ok(TaskExecutionOutcome {
                        task_id: request.task_id,
                        controller_config: controller_config.clone(),
                        resolved_config: test_resolved_config(
                            &controller_config,
                            &request.invocation_config,
                        ),
                        snapshot: request.snapshot,
                        provider_turns_executed: 1,
                        tool_calls_processed: 0,
                        final_assistant_message_id: Some(String::from("interrupt-followup")),
                        final_output_text: String::from("interrupt follow-up complete"),
                        execution_status: TaskExecutionStatus::Completed,
                        usage_segments: Vec::new(),
                    })
                })
            }),
        )
        .expect("task backend should install");
    // pending_join models the provider future owned by the current physical child execution.
    // pending_join 模拟当前物理子执行持有的 Provider future。
    let pending_join = tokio::spawn(async { std::future::pending::<()>().await });
    // active_guard keeps the interrupted execution reserved until its finalizer releases ownership.
    // active_guard 在中止执行的终态处理器释放所有权前保持该执行已预留。
    let active_guard = install_test_active_task_run(
        &task_runtime,
        task_id,
        "interrupt-running-execution",
        pending_join.abort_handle(),
    );

    // send_result is the immediate admission acknowledgement returned to the parent agent.
    // send_result 是立即返回给父代理的消息接纳确认。
    let send_result = task_runtime
        .invoke_send_message(
            root_session_id,
            json!({
                "target": "worker-a",
                "message": "replace the current work with this instruction",
                "delivery_mode": "interrupt"
            }),
        )
        .await
        .expect("interrupt message should be admitted");
    assert_eq!(send_result.model_output["queued"], true);
    assert_eq!(send_result.model_output["delivery_mode"], "interrupt");
    assert_eq!(send_result.model_output["interrupted"], true);
    assert_eq!(
        task_runtime
            .inter_agent_message_count_for_task(task_id)
            .expect("interrupted child inbox should remain readable"),
        1
    );
    // join_error confirms interrupt delivery actually aborts the previous physical execution.
    // join_error 确认插队投递确实中止了上一条物理执行。
    let join_error = tokio::time::timeout(Duration::from_secs(1), pending_join)
        .await
        .expect("interrupted provider future should settle")
        .expect_err("interrupted provider future should be cancelled");
    assert!(join_error.is_cancelled());

    drop(active_guard);
    assert!(
        task_runtime
            .resume_agent_from_inbox_after_task_settled(root_session_id, task_id)
            .await
            .expect("interrupted child should resume from its inbox")
    );
    // resumed_snapshot is the exact successor context supplied to the child backend.
    // resumed_snapshot 是传给子代理后端的精确后继执行上下文。
    let resumed_snapshot = captured_snapshot
        .lock()
        .expect("captured snapshot lock")
        .clone()
        .expect("successor snapshot should be captured");
    assert!(resumed_snapshot.messages.iter().any(|message| {
        matches!(
            &message.payload,
            CoreConversationPayload::AgentMessage { payload, .. }
                if payload == "replace the current work with this instruction"
        )
    }));
}

/// Verifies wait_agent returns an activity acknowledgement instead of child output.
/// 验证 wait_agent 返回活动确认而不是子代理输出。
#[tokio::test]
async fn wait_agent_returns_consumed_child_feedback() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let controller_config = build_test_controller_config("Base prompt");
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(120),
            uncached_input_tokens: None,
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            ..vulcan_core::types::ProviderUsageSnapshot::default()
        }),
    );
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
        usage_segments: vec![usage_segment],
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({}))
        .await
        .expect(
            "implicit wait_agent should claim terminal feedback even after the child completed",
        );

    assert_eq!(
        result.model_output["message"],
        "Subagent terminal result received."
    );
    assert_eq!(result.model_output["activity_status"], "terminal_update");
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["delivered_event_count"], 1);
    assert!(result.model_output.get("pending_event_count").is_none());
    assert_eq!(result.model_output["terminal_event_count"], 1);
    assert_eq!(
        result.model_output["agents"][0]["execution_id"],
        "test-execution"
    );
    assert_eq!(
        result.model_output["agents"][0]["result"],
        "review complete"
    );
    assert!(
        result.model_output["agents"][0]
            .get("usage_segments")
            .is_none()
    );
    let host_result = result.host_result.as_ref().expect("host result");
    assert_eq!(
        host_result.payload["agents"][0]["usage_segments"][0]["input_tokens"],
        120
    );
    assert!(result.model_output.to_string().contains("review complete"));

    let cached_result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "target": "agent-review" }))
        .await
        .expect("explicit wait_agent should return the authoritative cached terminal result");
    assert_eq!(
        cached_result.model_output["activity_status"],
        "terminal_update"
    );
    assert_eq!(cached_result.model_output["timed_out"], false);
    assert_eq!(cached_result.model_output["delivered_event_count"], 1);
    assert_eq!(
        cached_result.model_output["agents"][0]["result"],
        "review complete"
    );
    assert_eq!(
        cached_result.model_output["agents"][0]["status"],
        "completed"
    );

    // queued_message represents an admitted successor that has not started its new physical execution yet.
    // queued_message 表示已经接纳、但尚未启动新物理执行的后继投递。
    let queued_message = QueuedInterAgentMessage {
        delivery_id: String::from("queued-after-terminal"),
        author_session_id: root_session_id,
        target_task_id: task_id,
        target_agent_id: String::from("agent-review"),
        content: String::from("continue with the queued follow-up"),
        created_at_ms: current_unix_millis_u64(),
        continues_task_turn: false,
    };
    task_runtime
        .agent_inboxes
        .write()
        .expect("agent inbox should be writable")
        .insert(task_id, VecDeque::from([queued_message]));
    // target_session_ids limits the synthetic terminal lookup to the exact child under test.
    // target_session_ids 将合成终态查询限制到本测试中的精确子代理。
    let target_session_ids = BTreeSet::from([SessionNodeId::from(task_id)]);
    // cached_events must stay empty until the queued successor itself produces a terminal result.
    // cached_events 必须保持为空，直到排队的后继执行自身产生终态结果。
    let cached_events = task_runtime
        .wait_agent_cached_terminal_events(root_session_id, &target_session_ids, &BTreeSet::new())
        .expect("cached terminal lookup should remain readable");
    assert!(
        cached_events.is_empty(),
        "a queued successor must suppress the superseded cached terminal result"
    );
    assert!(
        tokio::time::timeout(
            Duration::from_millis(25),
            task_runtime.invoke_wait_agent(root_session_id, json!({"timeout_ms": 10_000})),
        )
        .await
        .is_err(),
        "implicit waiting must not report no active agents while a successor is queued"
    );
}

/// Verifies wait_agent returns immediately instead of timing out when no child can produce activity.
/// 验证不存在可产生活动的子代理时，wait_agent 会立即返回而不是等待超时。
#[tokio::test]
async fn wait_agent_returns_immediately_without_active_children() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();

    let result = tokio::time::timeout(
        Duration::from_millis(200),
        task_runtime.invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 })),
    )
    .await
    .expect("wait_agent should not enter its timeout path without active children")
    .expect("wait_agent should return a no-active-agents result");

    assert_eq!(result.model_output["activity_status"], "no_active_agents");
    assert_eq!(result.model_output["no_active_agents"], true);
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["delivered_event_count"], 0);
    assert_eq!(result.model_output["agents"], json!([]));
    assert!(
        result.model_output["message"]
            .as_str()
            .expect("wait message should be text")
            .contains("Do not call wait_agent again")
    );
}

/// Concurrent default waits must both observe child completion even though only one claims the mailbox event.
/// 并发默认等待必须都能观察子代理完成，即使只有一个调用能认领 mailbox 事件。
#[tokio::test]
async fn wait_agent_wakes_when_terminal_mailbox_event_arrives() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    let notifier_runtime = Arc::clone(&task_runtime);
    let notifier_invocation = invocation.clone();
    let notifier_outcome = outcome.clone();

    task_runtime
        .store_running_task_outcome_in_memory(
            root_session_id,
            &invocation,
            &TaskExecutionOutcome {
                execution_status: TaskExecutionStatus::Running,
                final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
                provider_turns_executed: 0,
                final_assistant_message_id: None,
                ..outcome.clone()
            },
        )
        .expect("running task should register before waiting");

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        notifier_runtime
            .store_task_outcome(root_session_id, &notifier_invocation, &notifier_outcome)
            .expect("terminal mailbox event should store");
    });

    // results exercises a mailbox claim race without waiting for the configured ten-second timeout.
    // results 覆盖 mailbox 认领竞态，无需等到配置的十秒超时。
    let results = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(
            task_runtime.invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 })),
            task_runtime.invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        )
    })
    .await
    .expect("both waits must observe completion without a second mailbox event");
    // Both results observe completion; scheduler order decides which caller owns the mailbox payload.
    // 两个结果都观察到完成；调度顺序决定哪个调用持有 mailbox 载荷。
    let result = results.0.expect("first wait should wake");
    let second_result = results.1.expect("second wait should wake");
    assert_eq!(second_result.model_output["timed_out"], false);
    assert_eq!(
        second_result.model_output["agents"][0]["result"],
        "review complete"
    );

    assert_eq!(
        result.model_output["message"],
        "Subagent terminal result received."
    );
    assert_eq!(result.model_output["activity_status"], "terminal_update");
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["delivered_event_count"], 1);
    assert!(result.model_output.get("pending_event_count").is_none());
    assert_eq!(result.model_output["terminal_event_count"], 1);
    assert!(
        [&result, &second_result]
            .iter()
            .any(|response| response.model_output["agents"][0]["execution_id"] == "test-execution"),
        "one waiter must claim the exact execution event"
    );
    assert_eq!(
        result.model_output["agents"][0]["result"],
        "review complete"
    );
    assert!(result.model_output.to_string().contains("review complete"));

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should be readable after wait");
    assert_eq!(
        events.len(),
        0,
        "wait_agent must consume its internal wake event so stale events do not wake future waits"
    );
}

/// Verifies targeted wait_agent does not consume unrelated stale child-agent feedback.
/// 验证定向 wait_agent 不会消费无关的旧智能体反馈。
#[tokio::test]
async fn targeted_wait_agent_retains_unrelated_stale_feedback() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let stale_task_id = Uuid::new_v4();
    let fresh_task_id = Uuid::new_v4();
    let controller_config = build_test_controller_config("Base prompt");
    let stale_invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id: stale_task_id,
        execution_id: String::from("test-execution-stale"),
        public_task_id: String::from("stale-agent"),
        prompt: String::from("stale work"),
        description: Some(String::from("Stale work")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("stale work"),
        display_title: String::from("Stale work"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let stale_outcome = TaskExecutionOutcome {
        task_id: stale_task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(
            &controller_config,
            &stale_invocation.invocation_config,
        ),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-stale")),
        final_output_text: String::from("STALE_DONE"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_task_outcome(root_session_id, &stale_invocation, &stale_outcome)
        .expect("stale completed task should store");

    let fresh_invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
        task_id: fresh_task_id,
        execution_id: String::from("test-execution-fresh"),
        public_task_id: String::from("fresh-agent"),
        prompt: String::from("fresh work"),
        description: Some(String::from("Fresh work")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("fresh work"),
        display_title: String::from("Fresh work"),
        parent_task_id: None,
        depth: 1,
        snapshot: CoreConversationSnapshot::default(),
        controller_config: None,
        is_resumed: false,
    };
    let fresh_completed_outcome = TaskExecutionOutcome {
        task_id: fresh_task_id,
        controller_config: controller_config.clone(),
        resolved_config: test_resolved_config(
            &controller_config,
            &fresh_invocation.invocation_config,
        ),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-fresh")),
        final_output_text: String::from("FRESH_DONE"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_running_task_outcome_in_memory(
            root_session_id,
            &fresh_invocation,
            &TaskExecutionOutcome {
                execution_status: TaskExecutionStatus::Running,
                final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
                provider_turns_executed: 0,
                final_assistant_message_id: None,
                ..fresh_completed_outcome.clone()
            },
        )
        .expect("fresh running task should store before waiting");

    let notifier_runtime = Arc::clone(&task_runtime);
    let notifier_invocation = fresh_invocation.clone();
    let notifier_outcome = fresh_completed_outcome.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        notifier_runtime
            .store_task_outcome(root_session_id, &notifier_invocation, &notifier_outcome)
            .expect("fresh terminal event should store");
    });

    let result = task_runtime
        .invoke_wait_agent(
            root_session_id,
            json!({ "target": "fresh-agent", "timeout_ms": 10_000 }),
        )
        .await
        .expect("targeted wait_agent should wake on fresh target");

    assert_eq!(result.model_output["agents"][0]["agent_id"], "fresh-agent");
    assert_eq!(result.model_output["agents"][0]["result"], "FRESH_DONE");
    assert!(!result.model_output.to_string().contains("STALE_DONE"));

    let remaining_events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should be readable after targeted wait");
    assert_eq!(remaining_events.len(), 1);
    assert_eq!(
        remaining_events[0].child_thread_id,
        SessionNodeId::from(stale_task_id)
    );
    assert_eq!(remaining_events[0].payload["result_text"], "STALE_DONE");
}

/// Verifies wait_agent wakes when the parent emits an explicit interruption.
/// 验证父会话发出显式打断时 wait_agent 会被唤醒。
#[tokio::test]
async fn wait_agent_wakes_when_parent_interruption_arrives() {
    // taskRuntime owns the waiter and the explicit parent-interruption channel.
    // taskRuntime 持有等待方与显式父级中断通道。
    let task_runtime = Arc::new(TaskRuntime::new());
    // rootSessionId identifies the parent wait that the notifier interrupts.
    // rootSessionId 标识通知方要打断的父级等待。
    let root_session_id = Uuid::new_v4();
    // runningChild keeps the wait active until the explicit parent interruption arrives.
    // runningChild 让等待保持活动，直到显式父级中断到达。
    let _running_child =
        store_running_wait_agent_child(task_runtime.as_ref(), root_session_id, "interrupt-target");
    // notifierRuntime lets the asynchronous notifier share the same interruption registry.
    // notifierRuntime 让异步通知方共享同一中断注册表。
    let notifier_runtime = Arc::clone(&task_runtime);

    // notifierTask reports notification failures to the test instead of detaching a hidden panic.
    // notifierTask 将通知失败报告给测试，避免分离任务中的隐藏 panic。
    let notifier_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        notifier_runtime
            .notify_wait_agent_parent_interruption(SessionNodeId::from(root_session_id))
            .expect("parent interruption should be recorded before notification");
    });

    // result contains the wait_agent response emitted after consuming the interruption fact.
    // result 包含消费中断事实后 wait_agent 发出的响应。
    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        .await
        .expect("wait_agent should wake on parent interruption");
    notifier_task
        .await
        .expect("parent interruption notifier task should finish");

    assert_eq!(
        result.model_output["message"],
        "Wait interrupted by new input."
    );
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["interrupted"], true);
}

/// Verifies a poisoned interruption registry rejects notification without publishing a watch change.
/// 验证中断注册表中毒时会拒绝通知，且不会发布 watch 变化。
#[test]
fn wait_agent_parent_interruption_notification_reports_poisoned_registry() {
    // taskRuntime owns both the authoritative interruption registry and its wake-only watch channel.
    // taskRuntime 同时持有权威中断注册表及仅用于唤醒的 watch 通道。
    let task_runtime = Arc::new(TaskRuntime::new());
    // interruptionReceiver observes whether the failed registry write incorrectly emits a wake signal.
    // interruptionReceiver 用于观察失败的注册表写入是否错误发出唤醒信号。
    let interruption_receiver = task_runtime.subscribe_wait_agent_parent_interruption();
    // versionBefore captures the unchanged watch state required by the fact-before-notification protocol.
    // versionBefore 捕获“事实先于通知”协议要求保持不变的 watch 状态。
    let version_before = *interruption_receiver.borrow();
    // poisonResult proves the authoritative interruption registry is unavailable.
    // poisonResult 证明权威中断注册表已经不可用。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // guard keeps the registry locked until the deliberate panic poisons it.
        // guard 将注册表保持加锁到刻意 panic，使其进入中毒状态。
        let _guard = task_runtime
            .wait_agent_parent_interruption_sessions
            .write()
            .expect("wait-agent interruption registry should lock before poisoning");
        panic!("poison wait-agent interruption registry for notification test");
    }));
    assert!(poison_result.is_err());
    // parentThreadId identifies the interruption fact that must be rejected rather than fabricated.
    // parentThreadId 标识必须被拒绝而不能伪造的中断事实。
    let parent_thread_id = SessionNodeId::from(Uuid::new_v4());

    // error is the explicit registry failure returned to the notification caller.
    // error 是返回给通知调用方的明确注册表失败。
    let error = task_runtime
        .notify_wait_agent_parent_interruption(parent_thread_id)
        .expect_err("poisoned interruption registry should reject notification");

    assert!(error.contains("wait-agent interruption registry is locked"));
    assert!(error.contains(&parent_thread_id.to_string()));
    assert_eq!(*interruption_receiver.borrow(), version_before);
    assert!(
        !interruption_receiver
            .has_changed()
            .expect("interruption watch should remain open"),
        "failed interruption storage must not publish a watch change"
    );
}

/// Verifies wait_agent still times out for a genuinely running child that emits no mailbox event.
/// 验证真实运行中的子代理没有产生 mailbox 事件时，wait_agent 仍会正常超时。
#[tokio::test]
async fn wait_agent_times_out_without_mailbox_event() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    // runningChild distinguishes a valid live wait from the zero-child immediate-return path.
    // runningChild 将有效活动等待与零子代理即时返回路径区分开。
    let _running_child =
        store_running_wait_agent_child(&task_runtime, root_session_id, "timeout-target");

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        .await
        .expect("wait_agent should return timeout summary");

    assert_eq!(result.model_output["message"], "Wait timed out.");
    assert_eq!(result.model_output["timed_out"], true);
}

/// Verifies parent spawn-agent metadata points at the TaskSession instead of embedding the child transcript.
/// 验证父级 spawn_agent 元数据只指向 TaskSession，而不会内嵌子代理完整转录。
#[test]
fn task_invocation_result_omits_embedded_conversation_state() {
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
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
    let usage_segment = vulcan_workbench_model::SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(120),
            uncached_input_tokens: None,
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            ..vulcan_core::types::ProviderUsageSnapshot::default()
        }),
    );
    let outcome = TaskExecutionOutcome {
        task_id,
        controller_config: build_test_controller_config("Base prompt"),
        resolved_config: test_resolved_config(
            &build_test_controller_config("Base prompt"),
            &invocation.invocation_config,
        ),
        snapshot: CoreConversationSnapshot::default(),
        provider_turns_executed: 1,
        tool_calls_processed: 0,
        final_assistant_message_id: Some(String::from("assistant-1")),
        final_output_text: String::from("review complete"),
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: vec![usage_segment],
    };

    let result = build_task_invocation_result(&invocation, &outcome);
    let host_result = result.host_result.expect("task host result");

    assert_eq!(host_result.kind, SUBAGENT_TASK_KIND);
    assert_eq!(
        host_result.payload[SUBAGENT_TASK_ID_KEY],
        task_id.to_string()
    );
    assert!(
        host_result.payload.get("conversation_state").is_none(),
        "new parent task metadata must not embed child transcripts"
    );
    assert_eq!(
        host_result.payload["usage_segments"][0]["input_tokens"],
        120
    );
    assert_eq!(
        host_result.payload["subagent_mailbox_event"]["payload"]["usage_segments"][0]["input_tokens"],
        120
    );
}

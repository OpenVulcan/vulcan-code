use super::*;

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
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
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
        .get("agent-review")
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
        .get("agent-review")
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
        .get("agent-review")
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
        .get("custom-review")
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
        public_task_id: String::from("nested-check"),
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
    assert_eq!(result.model_output["agents"][0]["run_state"], "running");
    assert_eq!(result.model_output["agents"][0]["status"], "running");
    assert_eq!(result.model_output["agents"][0]["queued_message_count"], 0);
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
        "nested-check"
    );

    let nested_only = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_closed": true, "path_prefix": "/root/agent-review/nested-check" }),
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
        "nested-check"
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
        .upsert(entry);

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
        registry.upsert(parent_entry);
        registry.upsert(child_entry);
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
            .get("parent-review")
            .expect("parent should exist")
            .status,
        SubagentStatus::Archived
    );
    assert_eq!(
        registry
            .get("parent-review")
            .expect("parent should exist")
            .stop_reason
            .as_deref(),
        Some("close_agent")
    );
    assert_eq!(
        registry
            .get("child-review")
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
            .get("parent-review")
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
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
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
        .invoke_wait_agent(root_session_id, json!({ "target": "agent-review" }))
        .await
        .expect("wait_agent should succeed");

    assert_eq!(
        result.model_output["message"],
        "Subagent terminal result received."
    );
    assert_eq!(result.model_output["activity_status"], "terminal_update");
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["pending_event_count"], 1);
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
}

/// Verifies wait_agent blocks until a terminal parent-mailbox event arrives and consumes the internal wake event.
/// 验证 wait_agent 会阻塞到父级 mailbox 收到终态事件，并消费内部唤醒事件。
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

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        .await
        .expect("wait_agent should wake on mailbox");

    assert_eq!(
        result.model_output["message"],
        "Subagent terminal result received."
    );
    assert_eq!(result.model_output["activity_status"], "terminal_update");
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["pending_event_count"], 1);
    assert_eq!(result.model_output["terminal_event_count"], 1);
    assert_eq!(
        result.model_output["agents"][0]["execution_id"],
        "test-execution"
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
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let notifier_runtime = Arc::clone(&task_runtime);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        notifier_runtime
            .notify_wait_agent_parent_interruption(SessionNodeId::from(root_session_id));
    });

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        .await
        .expect("wait_agent should wake on parent interruption");

    assert_eq!(
        result.model_output["message"],
        "Wait interrupted by new input."
    );
    assert_eq!(result.model_output["timed_out"], false);
    assert_eq!(result.model_output["interrupted"], true);
}

/// Verifies wait_agent times out instead of reporting completion when no mailbox event exists.
/// 验证不存在 mailbox 事件时 wait_agent 会超时而不是误报完成。
#[tokio::test]
async fn wait_agent_times_out_without_mailbox_event() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();

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
            output_tokens: Some(30),
            reasoning_tokens: Some(10),
            total_tokens: Some(160),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
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

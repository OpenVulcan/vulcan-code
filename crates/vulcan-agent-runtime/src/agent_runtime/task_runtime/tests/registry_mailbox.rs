use super::*;

/// Verifies TaskRuntime publishes subagent mailbox lifecycle events on real status transitions.
/// 验证 TaskRuntime 会在真实状态变化时发布 subagent mailbox 生命周期事件。
#[test]
fn task_runtime_publishes_subagent_mailbox_events() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_profile_id = Some(String::from("agent-review"));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &completed_outcome)
        .expect("completed task should store");

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should drain");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, SubagentMailboxEventKind::Started);
    assert_eq!(events[1].kind, SubagentMailboxEventKind::Completed);
    assert_eq!(events[1].child_thread_id, SessionNodeId::from(task_id));
    assert_eq!(events[1].payload["agent_kind"], "agent_profile");
    assert_eq!(events[1].payload["agent_profile"], "agent-review");
    assert_eq!(events[1].payload["result_text"], "review complete");
}

/// Verifies TaskRuntime mirrors task-session lifecycle changes into the native subagent registry.
/// 验证 TaskRuntime 会把任务会话生命周期变化镜像到原生子代理注册表。
#[test]
fn task_runtime_mirrors_lifecycle_into_subagent_registry() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_profile_id = Some(String::from("agent-review"));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    assert_eq!(running_entry.agent_kind, SubagentKind::AgentMd);
    assert_eq!(running_entry.prompt_source.as_deref(), Some("agent-review"));

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

/// Verifies list_agents prefers the native registry payload when registry entries exist.
/// 验证存在注册表条目时 list_agents 会优先使用原生注册表载荷。
#[tokio::test]
async fn list_agents_prefers_native_subagent_registry_payload() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_profile_id = Some(String::from("agent-review"));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let result = task_runtime
        .invoke_list_agents(root_session_id, json!({}))
        .await
        .expect("list_agents should succeed");

    assert_eq!(
        result.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["agent_kind"], "agent_md");
    assert_eq!(
        result.model_output["agents"][0]["prompt_source"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["status"], "completed");
    assert_eq!(
        result.model_output["agents"][0]["result"],
        "review complete"
    );
}

/// Verifies wait_agent returns the native registry payload once selected agents are completed.
/// 验证所选子代理完成后 wait_agent 会返回原生注册表载荷。
#[tokio::test]
async fn wait_agent_prefers_native_subagent_registry_payload_for_completed_agents() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_profile_id = Some(String::from("agent-review"));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "task_name": "agent-review" }))
        .await
        .expect("wait_agent should succeed");

    assert_eq!(result.model_output["completed"], true);
    assert_eq!(
        result.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["agent_kind"], "agent_md");
    assert_eq!(result.model_output["agents"][0]["status"], "completed");
    assert_eq!(
        result.model_output["agents"][0]["result"],
        "review complete"
    );
}

/// Verifies close_agent resolves matched agents from the native registry when available.
/// 验证存在原生注册表条目时 close_agent 会从注册表解析匹配代理。
#[tokio::test]
async fn close_agent_prefers_native_subagent_registry_payload() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let mut invocation_config = TaskInvocationConfig::main("review this change");
    invocation_config.subagent_profile_id = Some(String::from("agent-review"));
    let invocation = TaskExecutionRequest {
        current_session_id: root_session_id,
        root_session_id,
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
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let result = task_runtime
        .invoke_close_agent(root_session_id, json!({ "task_name": "agent-review" }))
        .await
        .expect("close_agent should succeed");

    assert_eq!(
        result.model_output["matched_agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(
        result.model_output["matched_agents"][0]["agent_kind"],
        "agent_md"
    );
    assert_eq!(result.model_output["stopped_count"], 0);
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
    };

    let result = build_task_invocation_result(&invocation, &outcome);
    let host_result = result.host_result.expect("task host result");

    assert_eq!(host_result.kind, "task_session");
    assert_eq!(host_result.payload["task_session_id"], task_id.to_string());
    assert!(
        host_result.payload.get("conversation_state").is_none(),
        "new parent task metadata must not embed child transcripts"
    );
}

use super::*;

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
    assert!(events[0].payload.get("agent_profile").is_none());
    assert_eq!(events[0].payload["result_text"], "review complete");
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
        .invoke_list_agents(root_session_id, json!({ "include_all": true }))
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
        result.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["agent_kind"], "clone");
    assert_eq!(result.model_output["agents"][0]["prompt_source"], "core");
    assert_eq!(result.model_output["agents"][0]["status"], "running");
    assert!(result.model_output["agents"][0]["result"].is_null());

    let filtered = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_all": true, "path_prefix": "/root/agent-review" }),
        )
        .await
        .expect("list_agents path_prefix filter should succeed");
    assert_eq!(
        filtered.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(
        filtered.model_output["agents"][1]["task_name"],
        "nested-check"
    );

    let nested_only = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_all": true, "path_prefix": "/root/agent-review/nested-check" }),
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
        nested_only.model_output["agents"][0]["task_name"],
        "nested-check"
    );

    let empty = task_runtime
        .invoke_list_agents(
            root_session_id,
            json!({ "include_all": true, "path_prefix": "/root/other" }),
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
        .invoke_list_agents(root_session_id, json!({ "include_all": true }))
        .await
        .expect("list_agents should succeed");

    assert_eq!(
        result.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(result.model_output["agents"][0]["status"], "completed");
    assert_eq!(
        result.model_output["agents"][0]["result"],
        "review complete"
    );
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
        first_listing.model_output["agents"][0]["task_name"],
        "agent-review"
    );
    assert_eq!(
        first_listing.model_output["agents"][0]["result"],
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
            build_task_session_record(
                child_task_id,
                String::from("worker"),
                root_session_id,
                None,
                1,
                None,
                None,
                TaskInvocationConfig::main("worker prompt"),
                String::from("worker"),
                None,
                CoreConversationSnapshot::default(),
                String::from("worker prompt"),
                String::from("running"),
            ),
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
async fn wait_agent_returns_activity_acknowledgement_without_child_output() {
    let task_runtime = TaskRuntime::new();
    let root_session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let invocation_config = TaskInvocationConfig::main("review this change");
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
        usage_segments: Vec::new(),
    };
    task_runtime
        .store_task_outcome(root_session_id, &invocation, &outcome)
        .expect("task outcome should store");

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({}))
        .await
        .expect("wait_agent should succeed");

    assert_eq!(result.model_output["message"], "Wait completed.");
    assert_eq!(result.model_output["timed_out"], false);
    assert!(result.model_output.get("agents").is_none());
    assert!(!result.model_output.to_string().contains("review complete"));
}

/// Verifies wait_agent blocks until a terminal parent-mailbox event arrives.
/// 验证 wait_agent 会阻塞到父级 mailbox 收到终态事件为止。
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

    assert_eq!(result.model_output["message"], "Wait completed.");
    assert_eq!(result.model_output["timed_out"], false);
    assert!(!result.model_output.to_string().contains("review complete"));

    let events = task_runtime
        .take_subagent_events_for_parent(SessionNodeId::from(root_session_id))
        .expect("mailbox should remain readable after wait");
    assert_eq!(
        events.len(),
        1,
        "wait_agent must not consume the child mailbox event"
    );
    assert!(
        events[0].payload.to_string().contains("review complete"),
        "mailbox event content must remain available for parent input delivery"
    );
}

/// Verifies wait_agent wakes when the parent receives steered user input.
/// 验证父会话收到用户插话时 wait_agent 会被唤醒。
#[tokio::test]
async fn wait_agent_wakes_when_parent_input_arrives() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let root_session_id = Uuid::new_v4();
    let notifier_runtime = Arc::clone(&task_runtime);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        notifier_runtime.notify_parent_input_activity(SessionNodeId::from(root_session_id));
    });

    let result = task_runtime
        .invoke_wait_agent(root_session_id, json!({ "timeout_ms": 10_000 }))
        .await
        .expect("wait_agent should wake on parent input");

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
        usage_segments: Vec::new(),
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
}

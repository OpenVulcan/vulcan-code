use super::*;

/// Verifies the model-facing subagent surface follows the core Vulcan tool split.
/// 验证模型可见子代理工具面遵循核心 Vulcan 工具拆分。
#[test]
fn subagent_tool_surface_exposes_session_agent_style_tools_without_visible_task() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let candidates = build_subagent_tool_candidates(task_runtime);
    let visible_ids = candidates
        .iter()
        .filter(|candidate| candidate.ai_visible)
        .map(|candidate| candidate.descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        visible_ids,
        vec![
            "spawn_agent",
            "send_message",
            "followup_task",
            "wait_agent",
            "interrupt_agent",
            "list_agents"
        ]
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.descriptor.id != "task")
    );
}

/// Verifies inter-agent message tools expose the Vulcan target/message schema.
/// 验证代理间消息工具暴露 Vulcan 的 target/message Schema。
#[test]
fn inter_agent_message_schema_exposes_target_and_message_only() {
    let schema = inter_agent_message_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("message tool properties should be an object");

    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("message"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(schema["required"], json!(["target", "message"]));
}

/// Verifies interrupt_agent follows the Vulcan target-only schema.
/// 验证 interrupt_agent 遵循 Vulcan 仅 target 参数的 Schema。
#[test]
fn interrupt_agent_schema_exposes_target_only() {
    let schema = interrupt_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("interrupt_agent properties should be an object");

    assert!(properties.contains_key("target"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(schema["required"], json!(["target"]));
}

/// Verifies list_agents follows the Vulcan path_prefix-only schema.
/// 验证 list_agents 遵循 Vulcan 仅 path_prefix 参数的 Schema。
#[test]
fn list_agents_schema_exposes_path_prefix_only() {
    let schema = list_agents_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("list_agents properties should be an object");

    assert!(properties.contains_key("path_prefix"));
    assert!(!properties.contains_key("task_name"));
}

/// Rejects legacy task_name from the Vulcan list tool.
/// 拒绝 Vulcan 列表工具中的历史 task_name 参数。
#[test]
fn list_agents_arguments_reject_task_name_alias() {
    let error = list_agents_path_prefix(&json!({
        "task_name": "agent-review"
    }))
    .expect_err("list_agents should reject task_name aliases");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

/// Rejects close-style task_name from the Vulcan interrupt tool.
/// 拒绝 Vulcan 中断工具中的 close 风格 task_name 参数。
#[test]
fn interrupt_agent_arguments_reject_task_name_alias() {
    let error = interrupt_agent_target(&json!({
        "task_name": "agent-review"
    }))
    .expect_err("interrupt_agent should reject task_name aliases");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

/// Verifies core spawn arguments map into the internal child-agent invocation payload.
/// 验证核心启动参数会映射到内部子代理调用载荷。
#[test]
fn spawn_agent_arguments_map_to_task_session_arguments() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "agent_type": "explorer",
        "model": "vulcan-model",
        "reasoning_effort": "high",
        "fork_turns": "all"
    }))
    .expect("spawn arguments should map to task arguments");

    assert_eq!(
        mapped,
        json!({
            "prompt": "Generate a lake image and return the file path.",
            "task_id": "image_worker_a",
            "description": "image_worker_a",
            "subagent": "explorer",
            "model": "vulcan-model",
            "reasoning_effort": "high",
            "fork_turns": "all"
        })
    );
}

/// Rejects invalid Vulcan fork_turns selectors from spawn_agent.
/// 拒绝 spawn_agent 中无效的 Vulcan fork_turns 选择器。
#[test]
fn spawn_agent_arguments_reject_invalid_fork_turns() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "fork_turns": "0"
    }))
    .expect_err("spawn_agent should reject zero fork_turns");

    assert!(error.contains("fork_turns must be"));
}

/// Verifies fork_turns numeric mode keeps the suffix from the last N user turns.
/// 验证 fork_turns 数字模式会保留最后 N 个用户回合开始的后缀。
#[test]
fn fork_turns_numeric_truncates_snapshot_to_recent_user_turns() {
    let snapshot = CoreConversationSnapshot {
        messages: vec![
            vulcan_core::types::ConversationMessage {
                id: String::from("u1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("first"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("a1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("first answer"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("u2"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("second"),
                    attachments: Vec::new(),
                },
            },
            vulcan_core::types::ConversationMessage {
                id: String::from("a2"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("second answer"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let truncated = truncate_snapshot_to_last_user_turns(snapshot, 1);

    assert_eq!(truncated.messages.len(), 2);
    assert_eq!(truncated.messages[0].id, "u2");
    assert_eq!(truncated.messages[1].id, "a2");
}

/// Verifies spawn_agent exposes the Vulcan task_name/message required schema.
/// 验证 spawn_agent 暴露 Vulcan 的 task_name/message 必填 Schema。
#[test]
fn spawn_agent_schema_requires_task_name_and_message() {
    let schema = spawn_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("spawn_agent properties should be an object");

    for key in [
        "message",
        "agent_type",
        "agent_mode",
        "agent_id",
        "fork_turns",
        "model",
        "reasoning_effort",
        "task_name",
    ] {
        assert!(properties.contains_key(key), "missing {key}");
    }
    assert!(!properties.contains_key("description"));
    assert_eq!(schema["required"], json!(["task_name", "message"]));
}

/// Verifies spawn_agent maps string agent ids into expert profile bindings.
/// 验证 spawn_agent 会将字符串 agent_id 映射为专家画像绑定。
#[test]
fn spawn_agent_arguments_map_agent_id_to_profile_binding() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Audit the subagent implementation.",
        "task_name": "audit-worker",
        "agent_type": "reviewer",
        "agent_mode": "expert",
        "agent_id": "code-reviewer",
        "fork_turns": "all"
    }))
    .expect("expert spawn arguments should map to task arguments");

    assert_eq!(mapped["agent_mode"], SUBAGENT_MODE_EXPERT);
    assert_eq!(mapped["subagent_profile_id"], "code-reviewer");
    assert_eq!(mapped["subagent"], "reviewer");
    assert_eq!(mapped["task_id"], "audit-worker");
    assert_eq!(mapped["fork_turns"], "none");
}

/// Rejects expert-mode spawn_agent calls that omit the string agent id.
/// 拒绝缺少字符串 agent_id 的专家模式 spawn_agent 调用。
#[test]
fn spawn_agent_arguments_require_agent_id_for_expert_mode() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Audit the subagent implementation.",
        "task_name": "audit-worker",
        "agent_mode": "expert"
    }))
    .expect_err("expert spawn should require agent_id");

    assert!(error.contains("agent_id"));
}

/// Verifies mirror-mode spawn_agent calls keep the mode without requiring an agent id.
/// 验证镜像模式 spawn_agent 会保留模式且不要求 agent_id。
#[test]
fn spawn_agent_arguments_allow_mirror_without_agent_id() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Analyze inherited context.",
        "task_name": "mirror-worker",
        "agent_mode": "mirror"
    }))
    .expect("mirror spawn should not require agent_id");

    assert_eq!(mapped["agent_mode"], SUBAGENT_MODE_MIRROR);
    assert!(mapped.get("subagent_profile_id").is_none());
}

/// Rejects legacy subagent profile and non-Vulcan override arguments from the core spawn tool.
/// 拒绝核心 spawn 工具中的旧子代理配置与非 Vulcan 覆盖参数。
#[test]
fn spawn_agent_arguments_reject_legacy_overrides() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "description": "legacy display title",
        "service_tier": "default",
        "tools": ["image_generation"]
    }))
    .expect_err("legacy spawn overrides should be rejected");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("description"));
    assert!(error.contains("service_tier"));
    assert!(error.contains("tools"));
}

/// Rejects spawn_agent calls without the Vulcan task_name field.
/// 拒绝缺少 Vulcan task_name 字段的 spawn_agent 调用。
#[test]
fn spawn_agent_arguments_require_task_name() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path."
    }))
    .expect_err("spawn_agent should require task_name");

    assert!(error.contains("task_name"));
}

/// Verifies the depth-one guard rejects child-agent attempts to spawn another child.
/// 验证一层深度护栏会拒绝子代理继续派生子代理。
#[test]
fn spawn_agent_arguments_reject_nested_spawn_at_depth_one() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let child_session_id = Uuid::new_v4();
    let child_record = build_task_session_record(
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
        .insert(child_session_id, child_record);
    let parsed_args = parse_task_tool_arguments(json!({
        "prompt": "nested work",
        "task_id": "nested-worker"
    }))
    .expect("internal task args should parse");

    let error = resolve_new_spawn_task_session_id(&task_runtime, child_session_id, &parsed_args)
        .expect_err("depth-one child agents must not spawn children");

    assert!(error.contains("maximum depth `1`"));
    assert!(error.contains("cannot create another subtask"));
}

/// Rejects alias-only spawn arguments that are not part of the tool schema.
/// 拒绝未出现在工具 Schema 中的 spawn_agent 别名参数。
#[test]
fn spawn_agent_arguments_reject_alias_only_message() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "prompt": "Generate a lake image and return the file path.",
        "task_id": "image-worker-a"
    }))
    .expect_err("alias-only spawn arguments should be rejected");

    assert!(error.contains("message"));
}

/// Verifies wait_agent follows the Vulcan timeout-only schema.
/// 验证 wait_agent 遵循 Vulcan 仅 timeout 参数的 Schema。
#[test]
fn wait_agent_schema_exposes_timeout_only() {
    let schema = wait_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("wait_agent properties should be an object");

    assert!(properties.contains_key("timeout_ms"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(properties["timeout_ms"]["minimum"], json!(10_000));
    assert_eq!(properties["timeout_ms"]["maximum"], json!(3_600_000));
}

/// Rejects legacy targeted wait arguments from the core wait tool.
/// 拒绝核心 wait 工具中的旧目标等待参数。
#[test]
fn wait_agent_arguments_reject_legacy_target() {
    let error = wait_agent_timeout_ms(&json!({
        "task_name": "agent-review",
        "timeout_ms": 1000
    }))
    .expect_err("legacy wait target should be rejected");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

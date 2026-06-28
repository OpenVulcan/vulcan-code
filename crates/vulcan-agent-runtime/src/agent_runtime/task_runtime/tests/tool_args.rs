use super::*;

/// Verifies the model-facing subagent surface follows the session-agent-style tool split.
/// 验证模型可见子代理工具面遵循 会话代理风格拆分。
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
            "list_agents",
            "close_agent"
        ]
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.descriptor.id != "task")
    );
}

/// Verifies queued child-agent messages are preserved in follow-up prompt order.
/// 验证排队子代理消息会按顺序保留到后续任务提示词中。
#[test]
fn followup_prompt_combines_queued_messages_in_order() {
    let prompt = followup_prompt_from_messages(
        vec![
            String::from("First note for the child."),
            String::from("Second note for the child."),
        ],
        String::from("Continue now."),
    );

    assert!(prompt.contains("[1]\nFirst note for the child."));
    assert!(prompt.contains("[2]\nSecond note for the child."));
    assert!(prompt.contains("[3]\nContinue now."));
}

/// Verifies session-agent-style spawn arguments map into the existing TaskSession invocation payload.
/// 验证 会话代理风格启动参数会映射到既有 TaskSession 调用载荷。
#[test]
fn spawn_agent_arguments_map_to_task_session_arguments() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image-worker-a",
        "agent_type": "image-agent",
        "model": "image-model",
        "tools": ["image_generation"]
    }))
    .expect("spawn arguments should map to task arguments");

    assert_eq!(
        mapped,
        json!({
            "prompt": "Generate a lake image and return the file path.",
            "task_id": "image-worker-a",
            "description": "image-worker-a",
            "subagent": "image-agent",
            "model": "image-model",
            "tools": ["image_generation"]
        })
    );
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

/// Rejects alias-only child-agent message arguments that are not part of the tool schema.
/// 拒绝未出现在工具 Schema 中的子代理消息别名参数。
#[test]
fn subagent_message_arguments_reject_alias_only_target_and_message() {
    let error = match parse_subagent_message_tool_arguments(
        json!({
            "task_id": "image-worker-a",
            "prompt": "Continue the image task."
        }),
        "send_message",
    ) {
        Ok(_) => panic!("alias-only message arguments should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("target") || error.contains("message"));
}

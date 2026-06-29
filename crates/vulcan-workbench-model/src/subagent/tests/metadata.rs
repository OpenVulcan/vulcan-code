use super::*;

#[test]
fn parses_task_session_payload_into_subagent_state() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "task-public",
        "subagent_task_id": task_id.to_string(),
        "execution_status": "running",
        "description": "检查工程结构",
        "subagent": "explorer",
        "result": "",
    });

    let state = parse_subagent_task_panel_payload(parent_session_id, &payload, None)
        .expect("subagent state");

    assert_eq!(state.parent_session_id, parent_session_id);
    assert_eq!(state.task_id, task_id);
    assert_eq!(state.public_task_id, "task-public");
    assert_eq!(state.subagent_label, "检查工程结构（explorer）");
    assert_eq!(state.run_state, SubagentTaskRunState::Running);
}

/// Verifies in-flight task arguments read only the declared `spawn_agent` schema fields.
/// 验证进行中的 task 参数只读取已声明的 `spawn_agent` schema 字段。
#[test]
fn extracts_task_session_fields_from_payload_or_arguments() {
    let payload_metadata = json!({
        "kind": "subagent_task",
        "payload": {
            "task_id": "public-a",
            "subagent_task_id": SessionId::new_v4().to_string(),
            "execution_status": "completed",
            "prompt": "执行检查",
            "description": "检查",
            "subagent": "worker",
        },
    });
    let argument_metadata = json!({
    "tool_name": "spawn_agent",
        "arguments": {
            "task_name": "public-b",
            "message": "执行测试",
            "description": "测试",
        },
    });

    assert_eq!(
        task_session_declared_task_id_from_tool_metadata(&payload_metadata),
        Some("public-a")
    );
    assert_eq!(
        task_session_declared_task_id_from_tool_metadata(&argument_metadata),
        Some("public-b")
    );
    assert_eq!(
        task_session_prompt_text_from_tool_metadata(&argument_metadata),
        Some("执行测试")
    );
    assert_eq!(
        task_session_description_from_tool_metadata(&argument_metadata),
        Some("测试")
    );
    assert_eq!(
        task_session_subagent_from_tool_metadata(&argument_metadata),
        None
    );
}

/// Verifies legacy alias-only spawn-agent arguments are not treated as task-session fields.
/// 验证仅包含旧别名的 spawn_agent 参数不会被当作 task-session 字段读取。
#[test]
fn ignores_alias_only_task_session_arguments() {
    let argument_metadata = json!({
        "tool_name": "spawn_agent",
        "arguments": {
            "task_id": "public-b",
            "name": "public-c",
            "prompt": "执行测试",
            "subagent": "tester",
        },
    });

    assert_eq!(
        task_session_declared_task_id_from_tool_metadata(&argument_metadata),
        None
    );
    assert_eq!(
        task_session_prompt_text_from_tool_metadata(&argument_metadata),
        None
    );
    assert_eq!(
        task_session_description_from_tool_metadata(&argument_metadata),
        None
    );
    assert_eq!(
        task_session_subagent_from_tool_metadata(&argument_metadata),
        None
    );
}

/// Verifies host-result task-session metadata uses the same shared payload helpers as direct delegate metadata.
/// 验证 host-result task-session 元数据会使用与直接委派元数据相同的共享载荷辅助入口。
#[test]
fn extracts_task_session_fields_from_host_result_metadata() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "tool_name": "spawn_agent",
        "host_result": {
            "kind": "subagent_task",
            "payload": {
                "task_id": "public-host",
                "subagent_task_id": task_id.to_string(),
                "execution_status": "running",
                "prompt": "生成图片",
                "description": "图片子代理",
                "subagent": "image-worker"
            }
        },
        "tool_display_state": "completed"
    });

    assert_eq!(
        task_session_declared_task_id_from_tool_metadata(&metadata),
        Some("public-host")
    );
    assert_eq!(
        task_session_execution_status_from_tool_metadata(&metadata),
        Some("running")
    );
    let state = parse_subagent_task_panel_metadata(parent_session_id, &metadata, None)
        .expect("host-result task-session metadata should parse");
    assert_eq!(state.task_id, task_id);
    assert_eq!(state.run_state, SubagentTaskRunState::Running);
    assert_eq!(state.subagent_label, "图片子代理（image-worker）");
}

/// Verifies the shared task-card lookup ignores prompt-only candidates and keeps exact identity matches.
/// 验证共享 task 卡片查找会忽略仅 prompt 相同的候选项，并保留精确身份匹配。
#[test]
fn finds_task_session_tool_message_id_across_messages_and_records() {
    let task_session_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "public-task",
        "subagent_task_id": task_session_id.to_string(),
        "prompt": "执行审核",
    });
    let weak_message = SessionMessageEntry {
        id: String::from("message-weak"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolCallOutput,
        visible: true,
        title: String::from("task"),
        body: String::new(),
        metadata: Some(json!({
            "tool_name": "spawn_agent",
            "arguments": {
                "message": "执行审核"
            }
        })),
        created_at: String::from("1"),
    };
    let exact_record = SessionConversationRecord {
        id: String::from("record-exact"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolCallOutput,
        visible: true,
        title: String::from("task"),
        body: String::new(),
        metadata: Some(json!({
            "tool_name": "spawn_agent",
            "kind": "subagent_task",
            "payload": {
                "task_id": "public-task",
                "subagent_task_id": task_session_id.to_string(),
                "execution_status": "completed"
            }
        })),
        created_at: String::from("2"),
    };

    let message_id = find_task_session_tool_message_id_for_payload(
        [&weak_message].into_iter(),
        [&exact_record].into_iter(),
        &payload,
    );

    assert_eq!(message_id.as_deref(), Some("record-exact"));
}

/// Verifies task-card lookup rejects text-only matches when no task identity is present.
/// 验证缺少任务身份时，task 卡片查找会拒绝仅文本相同的匹配。
#[test]
fn rejects_prompt_only_task_session_message_match() {
    let payload = json!({
        "prompt": "执行审核",
    });
    let message = SessionMessageEntry {
        id: String::from("message-prompt-only"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolCallOutput,
        visible: true,
        title: String::from("task"),
        body: String::new(),
        metadata: Some(json!({
            "tool_name": "spawn_agent",
            "arguments": {
                "message": "执行审核"
            }
        })),
        created_at: String::from("1"),
    };

    let message_id = find_task_session_tool_message_id_for_payload(
        [&message].into_iter(),
        [].into_iter(),
        &payload,
    );

    assert_eq!(message_id, None);
}

/// Verifies newly written stopped task metadata keeps only an index and summary.
/// 验证新写入的停止态 task 元数据只保留索引与摘要。
#[test]
fn stopped_task_session_metadata_omits_embedded_conversation_state() {
    let task_id = SessionId::new_v4();

    let metadata = build_stopped_task_session_metadata(None, None, task_id, "已停止", "1000");

    assert_eq!(metadata["payload"]["subagent_task_id"], task_id.to_string());
    assert_eq!(metadata["payload"]["execution_status"], "stopped");
    assert!(
        metadata["payload"].get("conversation_state").is_none(),
        "new stopped metadata must not embed child transcripts"
    );
}

/// Verifies shared tool metadata merging keeps the most advanced task-session payload.
/// 验证共享工具元数据合并会保留执行阶段最靠后的 task-session 载荷。
#[test]
fn merge_tool_display_metadata_preserves_advanced_task_session_payload() {
    let running_metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "reviewer",
            "subagent_task_id": "child-1",
            "execution_status": "running"
        }
    });
    let completed_metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "reviewer",
            "subagent_task_id": "child-1",
            "execution_status": "completed",
            "result": "done"
        }
    });

    let merged = merge_tool_display_metadata(
        Some(completed_metadata.clone()),
        Some(running_metadata.clone()),
    )
    .expect("metadata should merge");
    assert_eq!(
        merged
            .get("payload")
            .and_then(|payload| payload.get("execution_status"))
            .and_then(Value::as_str),
        Some("completed")
    );

    let merged = merge_tool_display_metadata(Some(running_metadata), Some(completed_metadata))
        .expect("metadata should merge");
    assert_eq!(
        merged
            .get("payload")
            .and_then(|payload| payload.get("execution_status"))
            .and_then(Value::as_str),
        Some("completed")
    );
}

/// Verifies metadata merging ranks host-result task-session payloads instead of trusting the generic completed display state.
/// 验证元数据合并会按 host-result task-session 载荷排名，而不是信任通用的已完成显示状态。
#[test]
fn merge_tool_display_metadata_ranks_host_result_task_session_payload() {
    let running_metadata = json!({
        "tool_name": "spawn_agent",
        "tool_display_state": "completed",
        "host_result": {
            "kind": "subagent_task",
            "payload": {
                "task_id": "reviewer",
                "subagent_task_id": "child-1",
                "execution_status": "running"
            }
        }
    });
    let completed_metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "reviewer",
            "subagent_task_id": "child-1",
            "execution_status": "completed",
            "result": "done"
        }
    });

    let merged = merge_tool_display_metadata(Some(completed_metadata), Some(running_metadata))
        .expect("metadata should merge");

    assert_eq!(
        task_session_payload_from_tool_metadata(&merged)
            .and_then(|payload| payload.get("execution_status"))
            .and_then(Value::as_str),
        Some("completed")
    );
}

/// Verifies spawn-agent card payloads cannot recover embedded child transcripts as full TaskSession panels.
/// 验证 spawn_agent 工具卡片 payload 不能把内嵌子会话转录恢复为完整 TaskSession 面板。
#[test]
fn task_card_payload_ignores_embedded_conversation_state() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "visible-task",
        "subagent_task_id": task_id.to_string(),
        "execution_status": "completed",
        "description": "旧任务",
        "subagent": "worker",
        "result": "工具卡片 payload 只保留返回摘要。",
        "conversation_state": SessionConversationState::default(),
    });

    let state = parse_subagent_task_panel_payload(parent_session_id, &payload, None)
        .expect("subagent panel state");

    assert_eq!(state.task_id, task_id);
    assert_eq!(state.result_text, "工具卡片 payload 只保留返回摘要。");
    assert!(
        state.conversation_state.is_none(),
        "tool-card payload snapshots must not become reviewable child transcripts"
    );
}

/// Verifies spawn-agent card payload parsing requires a canonical subagent_task_id.
/// 验证 spawn_agent 工具卡片 payload 解析必须具备规范 subagent_task_id。
#[test]
fn task_card_payload_requires_task_session_id() {
    let parent_session_id = SessionId::new_v4();
    let payload = json!({
        "task_id": SessionId::new_v4().to_string(),
        "execution_status": "completed",
        "result": "缺少内部会话标识。",
    });

    assert!(
        parse_subagent_task_panel_payload(parent_session_id, &payload, None).is_none(),
        "task_id is a public task identifier and must not be treated as the internal session id"
    );
}

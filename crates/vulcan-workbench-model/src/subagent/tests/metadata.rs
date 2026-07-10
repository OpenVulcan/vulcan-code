use super::*;
use crate::{SessionTurnStats, SessionTurnStatsKind, SessionTurnUsageSegmentKind};

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

/// Verifies unnamed child-task cards use the user-facing agent fallback label.
/// 验证未命名子任务卡片使用面向用户的智能体兜底标签。
#[test]
fn task_session_display_label_uses_agent_fallback() {
    let label = task_session_tool_display_label(None, None, None, None, None);

    assert_eq!(label, "智能体");
}

/// Verifies task-card payload parsing does not invent a completed state without explicit execution status.
/// 验证 task 卡片 payload 解析不会在缺少明确 execution_status 时臆造已完成状态。
#[test]
fn task_card_payload_requires_explicit_execution_status() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "task-public",
        "subagent_task_id": task_id.to_string(),
        "description": "检查工程结构",
        "result": "缺少状态。",
    });

    assert!(
        parse_subagent_task_panel_payload(parent_session_id, &payload, None).is_none(),
        "missing execution_status must not be projected as a completed subagent panel"
    );
}

/// Verifies task-card payload parsing rejects unknown execution statuses.
/// 验证 task 卡片 payload 解析会拒绝未知 execution_status。
#[test]
fn task_card_payload_rejects_unknown_execution_status() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "task-public",
        "subagent_task_id": task_id.to_string(),
        "execution_status": "almost_done",
        "description": "检查工程结构",
        "result": "未知状态。",
    });

    assert!(
        parse_subagent_task_panel_payload(parent_session_id, &payload, None).is_none(),
        "unknown execution_status must not be projected as a completed subagent panel"
    );
}

/// Verifies active TaskSession statuses still map to the running panel state.
/// 验证活动中的 TaskSession 状态仍会映射为面板运行中状态。
#[test]
fn task_card_payload_maps_active_execution_statuses_to_running() {
    let parent_session_id = SessionId::new_v4();
    for status in [
        "created",
        "running",
        "waiting_children",
        "waiting_followup",
        "compacting",
    ] {
        let task_id = SessionId::new_v4();
        let payload = json!({
            "task_id": format!("task-public-{status}"),
            "subagent_task_id": task_id.to_string(),
            "execution_status": status,
        });

        let state = parse_subagent_task_panel_payload(parent_session_id, &payload, None)
            .expect("active status should parse");
        assert_eq!(state.run_state, SubagentTaskRunState::Running);
    }
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

/// Verifies same public task names cannot override a card that already belongs to a different child session.
/// 验证相同公开任务名不能覆盖已经绑定到其他子会话的卡片。
#[test]
fn rejects_public_task_match_when_internal_task_ids_conflict() {
    let previous_task_session_id = SessionId::new_v4();
    let current_task_session_id = SessionId::new_v4();
    let payload = json!({
        "task_id": "echo_123_agent",
        "subagent_task_id": current_task_session_id.to_string(),
        "execution_status": "completed",
    });
    let metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "echo_123_agent",
            "subagent_task_id": previous_task_session_id.to_string(),
            "execution_status": "completed",
        },
    });

    assert_eq!(task_session_message_match_score(&metadata, &payload), 0);
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
    assert_eq!(
        metadata["payload"]["execution_id"],
        format!("stopped-{task_id}-1000")
    );
    assert_eq!(metadata["payload"]["execution_status"], "stopped");
    assert!(
        metadata["payload"].get("conversation_state").is_none(),
        "new stopped metadata must not embed child transcripts"
    );
}

/// Verifies task-session status metadata updates keep every display alias synchronized.
/// 验证 task-session 状态元数据更新会同步所有显示状态别名。
#[test]
fn task_session_display_metadata_update_syncs_payload_and_host_result_status() {
    let task_id = SessionId::new_v4();
    let metadata = task_session_display_metadata_update(
        json!({
            "tool_name": "spawn_agent",
            "kind": "subagent_task",
            "payload": {
                "task_id": "worker",
                "subagent_task_id": task_id.to_string(),
                "execution_status": "running"
            },
            "host_result": {
                "kind": "subagent_task",
                "payload": {
                    "task_id": "worker",
                    "subagent_task_id": task_id.to_string(),
                    "execution_status": "running"
                }
            }
        }),
        "completed",
    );

    assert_eq!(metadata["display_state"], "completed");
    assert_eq!(metadata["tool_display_state"], "completed");
    assert_eq!(metadata["payload"]["execution_status"], "completed");
    assert_eq!(
        metadata["host_result"]["payload"]["execution_status"],
        "completed"
    );
}

/// Verifies task mirror metadata updates build matching payload and synchronized display state.
/// 验证任务镜像元数据更新会构建匹配载荷并同步展示状态。
#[test]
fn task_session_mirror_display_metadata_update_builds_payload_and_metadata() {
    let task_id = SessionId::new_v4();

    let update =
        task_session_mirror_display_metadata_update(task_id, "completed", 3, "1780000000000");

    assert_eq!(update.payload["task_id"], task_id.to_string());
    assert_eq!(update.payload[SUBAGENT_TASK_ID_KEY], task_id.to_string());
    assert_eq!(update.payload["execution_status"], "completed");
    assert_eq!(update.payload["line_count"], 3);
    assert_eq!(update.payload["updated_at"], "1780000000000");
    assert_eq!(update.metadata_update["tool_name"], "spawn_agent");
    assert_eq!(update.metadata_update["kind"], SUBAGENT_TASK_KIND);
    assert_eq!(update.metadata_update["display_state"], "completed");
    assert_eq!(update.metadata_update["tool_display_state"], "completed");
    assert_eq!(
        update.metadata_update["payload"]["execution_status"],
        "completed"
    );
}

/// Verifies live task mirrors keep the runtime running state over stale terminal payloads.
/// 验证实时任务镜像会保留运行态，避免被陈旧终态 payload 覆盖。
#[test]
fn task_session_display_metadata_merge_preserves_live_running_override() {
    let task_id = SessionId::new_v4();
    let current_metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "worker",
            "subagent_task_id": task_id.to_string(),
            "execution_status": "completed"
        }
    });
    let running_update = task_session_display_metadata_update(
        json!({
            "tool_name": "spawn_agent",
            "kind": "subagent_task",
            "payload": {
                "task_id": "worker",
                "subagent_task_id": task_id.to_string(),
                "execution_status": "running"
            }
        }),
        "running",
    );

    let merged =
        merge_task_mirror_tool_display_metadata(Some(current_metadata), running_update, true)
            .expect("metadata should merge");

    assert_eq!(
        task_session_payload_from_tool_metadata(&merged)
            .and_then(|payload| payload.get("execution_status"))
            .and_then(Value::as_str),
        Some("running")
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

/// Verifies unknown task-session payload statuses do not outrank canonical payloads during merge.
/// 验证未知 task-session payload 状态不会在合并时覆盖规范状态 payload。
#[test]
fn merge_tool_display_metadata_ignores_unknown_status_rank() {
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
    let unknown_metadata = json!({
        "tool_name": "spawn_agent",
        "kind": "subagent_task",
        "payload": {
            "task_id": "reviewer",
            "subagent_task_id": "child-1",
            "execution_status": "almost_done",
            "result": "unknown"
        }
    });

    let merged = merge_tool_display_metadata(Some(completed_metadata), Some(unknown_metadata))
        .expect("metadata should merge");

    assert_eq!(
        task_session_payload_from_tool_metadata(&merged)
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

/// Verifies parent-to-child send_message host metadata parses into one child transcript projection.
/// 验证父级发往子智能体的 send_message 宿主元数据会解析为一条子转录投影。
#[test]
fn parses_subagent_send_message_projection_from_host_metadata() {
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": SessionId::new_v4().to_string(),
                "target": "worker",
                "subagent_task_id": task_id.to_string(),
                "delivery_id": "delivery-1",
                "message": "继续检查。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    });

    let projection =
        subagent_send_message_projection_from_tool_metadata(&metadata, "工具返回 · call-send")
            .expect("projection parse should succeed")
            .expect("projection should exist");

    assert_eq!(projection.task_session_id, task_id);
    assert_eq!(projection.message_id, "delivery-1");
    assert_eq!(projection.message, "继续检查。");
    assert_eq!(projection.display_metadata["tool"], "send_message");
    assert_eq!(
        projection.display_metadata["subagent_task_id"],
        task_id.to_string()
    );
}

/// Verifies malformed identified send_message metadata reports the invalid child task id.
/// 验证已识别的 send_message 元数据在子任务标识损坏时显式报错。
#[test]
fn subagent_send_message_projection_rejects_invalid_task_session_id() {
    let metadata = json!({
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "target": "worker",
                "subagent_task_id": "not-a-session-id",
                "delivery_id": "delivery-1",
                "message": "继续检查。"
            }
        }
    });

    let error =
        subagent_send_message_projection_from_tool_metadata(&metadata, "工具返回 · call-send")
            .expect_err("invalid task id should be reported");

    assert!(error.contains("invalid task_session_id"));
}

/// Verifies the migrated display-id rule still falls back to the tool call id.
/// 验证迁移后的显示标识规则仍会回退到工具调用标识。
#[test]
fn subagent_send_message_projection_falls_back_to_tool_call_id() {
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "tool_call_id": "call-send",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "target": "worker",
                "subagent_task_id": task_id.to_string(),
                "message": "继续检查。"
            }
        }
    });

    let projection =
        subagent_send_message_projection_from_tool_metadata(&metadata, "工具返回 · ignored")
            .expect("projection parse should succeed")
            .expect("projection should exist");

    assert_eq!(projection.message_id, "call-send");
}

/// Verifies the legacy standard title fallback remains available when no ids are stored.
/// 验证没有存储标识时仍保留旧标准标题回退规则。
#[test]
fn subagent_send_message_projection_falls_back_to_event_title() {
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "target": "worker",
                "subagent_task_id": task_id.to_string(),
                "message": "继续检查。"
            }
        }
    });

    let projection = subagent_send_message_projection_from_tool_metadata(
        &metadata,
        "工具返回 · call-from-title",
    )
    .expect("projection parse should succeed")
    .expect("projection should exist");

    assert_eq!(projection.message_id, "call-from-title");
}

/// Verifies canonical subagent send_message display notes are recognized by the shared classifier.
/// 验证规范智能体 send_message display note 会被共享分类器识别。
#[test]
fn session_message_is_subagent_send_message_display_note_accepts_canonical_note() {
    let mut message = SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", "继续。");
    message.record_kind = SessionMessageRecordKind::DisplayNote;
    message.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message"
    }));

    assert!(session_message_is_subagent_send_message_display_note(
        &message
    ));
}

/// Verifies normal visible rows with the same metadata are not treated as display-only notes.
/// 验证带相同元数据的普通可见行不会被视为仅显示记录。
#[test]
fn session_message_is_subagent_send_message_display_note_rejects_normal_rows() {
    let mut message = SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", "继续。");
    message.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message"
    }));

    assert!(!session_message_is_subagent_send_message_display_note(
        &message
    ));
}

/// Verifies other display notes do not share the subagent send_message identity.
/// 验证其他 display note 不会共享智能体 send_message 身份。
#[test]
fn session_message_is_subagent_send_message_display_note_rejects_other_tools() {
    let mut message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "智能体反馈", "完成。");
    message.record_kind = SessionMessageRecordKind::DisplayNote;
    message.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_feedback",
        "tool": "wait_agent"
    }));

    assert!(!session_message_is_subagent_send_message_display_note(
        &message
    ));
}

/// Verifies canonical conversation records reuse the same subagent send_message identity.
/// 验证规范会话记录复用同一套智能体 send_message 身份判定。
#[test]
fn session_conversation_record_is_subagent_send_message_display_note_accepts_canonical_note() {
    let record = SessionConversationRecord {
        id: String::from("delivery-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::DisplayNote,
        visible: true,
        title: String::from("智能体消息"),
        body: String::from("继续。"),
        metadata: Some(json!({
            "display_note": true,
            "kind": "subagent_message",
            "tool": "send_message"
        })),
        created_at: String::from("100"),
    };

    assert!(session_conversation_record_is_subagent_send_message_display_note(&record));
}

/// Verifies hidden canonical rows cannot become child-agent delivery anchors.
/// 验证隐藏规范记录不会成为子智能体投递锚点。
#[test]
fn session_conversation_record_is_subagent_send_message_display_note_rejects_hidden_note() {
    let record = SessionConversationRecord {
        id: String::from("hidden-delivery-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::DisplayNote,
        visible: false,
        title: String::from("智能体消息"),
        body: String::from("继续。"),
        metadata: Some(json!({
            "display_note": true,
            "kind": "subagent_message",
            "tool": "send_message"
        })),
        created_at: String::from("100"),
    };

    assert!(!session_conversation_record_is_subagent_send_message_display_note(&record));
}

/// Verifies child-agent feedback display records preserve result text in metadata and usage statistics.
/// 验证智能体反馈显示记录会在元数据中保留结果文本与用量统计。
#[test]
fn subagent_feedback_display_records_builds_visible_rows_and_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let mut state =
        test_subagent_state(parent_session_id, task_id, SubagentTaskRunState::Completed);
    state.public_task_id = String::from("image-worker");
    state.subagent_label = String::from("image-worker");
    state.result_text = String::from("生成完成：a.jpg");
    let metadata = json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "image-worker",
            (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
            "execution_id": "image-worker-run-1",
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "生成完成：a.jpg",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "image-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    });

    let records =
        subagent_feedback_display_records_from_tool_metadata(parent_session_id, &metadata, &state)
            .expect("feedback records should build");

    assert_eq!(records.execution_id, "image-worker-run-1");
    assert_eq!(
        records.feedback_entry.record_kind,
        SessionMessageRecordKind::DisplayNote
    );
    assert_eq!(records.feedback_entry.title, "智能体返回处理结果");
    assert_eq!(records.feedback_entry.body, "智能体返回处理结果");
    assert_eq!(
        records
            .feedback_entry
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("host_result"))
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("result"))
            .and_then(serde_json::Value::as_str),
        Some("生成完成：a.jpg")
    );
    assert!(subagent_feedback_record_matches(
        records.feedback_entry.metadata.as_ref(),
        &task_id.to_string(),
        "image-worker-run-1"
    ));
    assert_eq!(
        records.stats_entry.record_kind,
        SessionMessageRecordKind::TurnStats
    );
    let stats: SessionTurnStats = serde_json::from_value(
        records
            .stats_entry
            .metadata
            .as_ref()
            .expect("stats entry should contain metadata")["turn_stats"]
            .clone(),
    )
    .expect("stats metadata should parse");
    assert_eq!(stats.turn_kind, SessionTurnStatsKind::SubagentFeedback);
    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 100);
    assert_eq!(stats.output_tokens, 25);
    assert_eq!(stats.reasoning_tokens, 5);
    assert_eq!(stats.total_tokens, 130);
    assert_eq!(stats.cached_input_tokens, Some(40));
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::SubagentTask
    );
    assert_eq!(stats.usage_groups.len(), 1);
}

/// Verifies child-agent feedback de-duplication keys require both task and execution identity.
/// 验证智能体反馈去重键同时要求任务与执行身份匹配。
#[test]
fn subagent_feedback_record_matches_task_and_execution_id() {
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "subagent_feedback_record": true,
        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
        "execution_id": "run-1"
    });

    assert!(subagent_feedback_record_matches(
        Some(&metadata),
        &task_id.to_string(),
        "run-1"
    ));
    assert!(!subagent_feedback_record_matches(
        Some(&metadata),
        &SessionId::new_v4().to_string(),
        "run-1"
    ));
    assert!(!subagent_feedback_record_matches(
        Some(&metadata),
        &task_id.to_string(),
        "run-2"
    ));
}

/// Verifies malformed child-agent usage data is rejected instead of producing zero-token stats.
/// 验证格式错误的智能体用量数据会被拒绝，而不是产出 0-token 统计。
#[test]
fn subagent_feedback_display_records_rejects_malformed_usage_segments() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let state = test_subagent_state(parent_session_id, task_id, SubagentTaskRunState::Completed);
    let metadata = json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "bad-usage-worker",
            (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
            "execution_id": "bad-usage-run-1",
            "execution_status": "completed",
            "result": "完成但统计坏了",
            "usage_segments": {
                "kind": "subagent_task",
                "input_tokens": 100
            }
        }
    });

    let error =
        subagent_feedback_display_records_from_tool_metadata(parent_session_id, &metadata, &state)
            .expect_err("malformed usage segments must be rejected");

    assert!(error.contains("failed to parse subagent feedback usage segments"));
}

/// Verifies empty child-agent result text stays empty in the feedback metadata payload.
/// 验证空智能体结果文本会在反馈元数据载荷中保持为空。
#[test]
fn subagent_feedback_display_records_uses_empty_result_body() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let state = test_subagent_state(parent_session_id, task_id, SubagentTaskRunState::Completed);
    let metadata = json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "empty-worker",
            (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
            "execution_id": "empty-worker-run-1",
            "execution_status": "completed",
            "result": "",
            "usage_segments": []
        }
    });

    let records =
        subagent_feedback_display_records_from_tool_metadata(parent_session_id, &metadata, &state)
            .expect("feedback records should build");

    assert_eq!(records.feedback_entry.body, "智能体返回处理结果");
    assert_eq!(
        records
            .feedback_entry
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("host_result"))
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("result"))
            .and_then(serde_json::Value::as_str),
        Some("")
    );
}

/// Verifies close_agent control metadata keeps closed and stopped task sets distinct.
/// 验证 close_agent 控制元数据会区分已关闭任务集合与已停止任务集合。
#[test]
fn subagent_close_agent_projection_preserves_matched_and_stopped_task_ids() {
    let matched_task_id = SessionId::new_v4();
    let stopped_task_id = SessionId::new_v4();
    let metadata = json!({
        "host_result": {
            "kind": "subagent_control",
            "payload": {
                "tool": "close_agent",
                "matched_agents": [{
                    "agent_id": "worker",
                    "subagent_task_id": matched_task_id.to_string()
                }],
                "stopped_subagent_task_ids": [stopped_task_id.to_string()]
            }
        }
    });

    let projection = subagent_close_agent_projection_from_tool_metadata(&metadata)
        .expect("projection parse should succeed")
        .expect("projection should exist");

    assert_eq!(projection.matched_task_ids, vec![matched_task_id]);
    assert_eq!(projection.stopped_task_ids, vec![stopped_task_id]);
    let affected_task_ids = projection
        .affected_task_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        affected_task_ids,
        std::collections::BTreeSet::from([matched_task_id, stopped_task_id])
    );
}

/// Verifies malformed close_agent matched agent ids are reported instead of dropped.
/// 验证 close_agent 匹配智能体标识损坏时会报错而不是被丢弃。
#[test]
fn subagent_close_agent_projection_rejects_invalid_matched_task_id() {
    let metadata = json!({
        "host_result": {
            "kind": "subagent_control",
            "payload": {
                "tool": "close_agent",
                "matched_agents": [{
                    "agent_id": "worker",
                    "subagent_task_id": "not-a-session-id"
                }],
                "stopped_subagent_task_ids": []
            }
        }
    });

    let error = subagent_close_agent_projection_from_tool_metadata(&metadata)
        .expect_err("invalid matched task id should be reported");

    assert!(error.contains("invalid subagent_task_id"));
}

/// Verifies non-close subagent control metadata does not project as close_agent.
/// 验证非 close 的智能体控制元数据不会被投影为 close_agent。
#[test]
fn subagent_close_agent_projection_ignores_interrupt_agent_control() {
    let task_id = SessionId::new_v4();
    let metadata = json!({
        "host_result": {
            "kind": "subagent_control",
            "payload": {
                "tool": "interrupt_agent",
                "matched_agents": [{
                    "agent_id": "worker",
                    "subagent_task_id": task_id.to_string()
                }],
                "stopped_subagent_task_ids": [task_id.to_string()]
            }
        }
    });

    assert!(
        subagent_close_agent_projection_from_tool_metadata(&metadata)
            .expect("projection parse should succeed")
            .is_none()
    );
}

/// Verifies unrelated host results without payload do not report close_agent errors.
/// 验证无关宿主结果即使没有 payload 也不会误报 close_agent 错误。
#[test]
fn subagent_close_agent_projection_ignores_unrelated_host_result_without_payload() {
    let metadata = json!({
        "host_result": {
            "kind": "change_set"
        }
    });

    assert!(
        subagent_close_agent_projection_from_tool_metadata(&metadata)
            .expect("unrelated host result should not fail")
            .is_none()
    );
}

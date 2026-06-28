use super::*;
use crate::conversation_protocol::SessionConversationState;
use serde_json::json;

/// Builds one deterministic test session identifier.
/// 构建一个确定性的测试会话标识。
fn test_session_id() -> SessionId {
    "11111111-1111-1111-1111-111111111111"
        .parse()
        .expect("test session id should be a valid UUID")
}

/// Verifies canonical checkpoints do not replace visible transcript projections.
/// 验证规范检查点不会替换可见转录投影。
#[test]
fn checkpoint_preserves_visible_records() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1000", "介绍下你自己");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1001",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "我是 VulcanCode。",
        false,
    );
    let records_before_checkpoint = visible_record_projection(&runtime_state);

    apply_checkpoint_conversation_state(
        &mut runtime_state,
        SessionConversationState::default(),
        "1002",
    );

    assert_eq!(
        visible_record_projection(&runtime_state),
        records_before_checkpoint
    );
    assert_eq!(runtime_state.updated_at, "1002");
}

/// Verifies checkpoint-created guided user records are merged into visible transcript projections.
/// 验证检查点创建的引导用户记录会合并进可见转录投影。
#[test]
fn checkpoint_merges_guided_user_record() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1100", "先调用工具");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1101",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "工具已经完成。",
        false,
    );
    let mut later_output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1104",
        &mut later_output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "后续请求已经开始输出。",
        false,
    );
    let mut checkpoint_state = runtime_state.conversation_state.clone();
    checkpoint_state.records.push(SessionConversationRecord {
        id: String::from("core-guided-user-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户输入"),
        body: String::from("不需要配图了"),
        metadata: Some(json!({
            "canonical_text": "不需要配图了",
            "canonical_user_text": "不需要配图了",
            "source": "user_interrupt",
            "starts_user_turn": false,
            "turn_process": true,
            "collapsed_by_default": true,
            "editable": false,
        })),
        created_at: String::from("1102"),
    });

    apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1103");

    let visible_bodies = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        visible_bodies,
        vec![
            "先调用工具",
            "工具已经完成。",
            "不需要配图了",
            "后续请求已经开始输出。"
        ]
    );
    let guided_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == "core-guided-user-record")
        .expect("guided user record should be merged");
    let guided_metadata = guided_record
        .metadata
        .as_ref()
        .expect("guided user record metadata should be preserved");
    assert_eq!(
        guided_metadata
            .get("source")
            .and_then(serde_json::Value::as_str),
        Some("user_interrupt")
    );
    assert_eq!(
        guided_metadata
            .get("starts_user_turn")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        guided_metadata
            .get("turn_process")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        guided_metadata
            .get("editable")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

/// Verifies internal mailbox checkpoint prompts never become visible user transcript rows.
/// 验证内部 mailbox 检查点提示词不会进入可见用户转录行。
#[test]
fn checkpoint_skips_session_mailbox_user_record() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1200", "启动两个子代理");
    let mut checkpoint_state = runtime_state.conversation_state.clone();
    checkpoint_state.records.push(SessionConversationRecord {
        id: String::from("core-mailbox-user-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户输入"),
        body: String::from(
            "<subagent_mailbox_event>\n{\"kind\":\"completed\"}\n</subagent_mailbox_event>",
        ),
        metadata: None,
        created_at: String::from("1201"),
    });

    apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1202");

    let visible_bodies = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible_bodies, vec!["启动两个子代理"]);
}

/// Verifies terminal reports keep streamed records when they also carry a canonical checkpoint.
/// 验证终态报告携带规范检查点时仍会保留已流式写入的记录。
#[test]
fn terminal_report_preserves_streamed_records() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2000", "随便调用两个工具");
    let mut thinking_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "2001",
        &mut thinking_record_id,
        SessionMessageRecordKind::ThinkingOutput,
        "思考",
        "我需要先判断可用工具。",
        true,
    );
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "2002",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "已完成，两个工具均正常。",
        false,
    );

    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(String::from("报告中的完整输出不应覆盖已流式记录。")),
        thinking_output: Some(String::from("报告中的思考不应重复追加。")),
        conversation_state: Some(SessionConversationState::default()),
        provider_request_count: 1,
        last_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2003", &report);

    let visible_bodies: Vec<&str> = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect();
    assert!(visible_bodies.contains(&"随便调用两个工具"));
    assert!(visible_bodies.contains(&"我需要先判断可用工具。"));
    assert!(visible_bodies.contains(&"已完成，两个工具均正常。"));
    assert!(!visible_bodies.contains(&"报告中的完整输出不应覆盖已流式记录。"));
    let canonical_texts = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. } => Some(text.as_str()),
            SessionConversationPayload::Assistant { text, .. } => Some(text.as_str()),
            SessionConversationPayload::ToolResult { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(canonical_texts.contains(&"随便调用两个工具"));
    assert!(canonical_texts.contains(&"报告中的完整输出不应覆盖已流式记录。"));
}

/// Verifies a tool result projection preserves runtime event order instead of mutating the stored transcript order.
/// 验证工具返回投影保留运行时事件顺序，而不是改写已存储转录顺序。
#[test]
fn tool_result_projection_preserves_runtime_event_order() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "3000", "调用工具");
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-1 · vulcan_file_list",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-1",
        "tool_name": "vulcan_file_list",
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3001",
        &started,
    ));
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "3002",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "工具调用后继续输出。",
        false,
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-1",
        "## Result ID\n\n`tool-result-call-1`\n\n## Tool Result\n\nlisted files",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-1",
        "tool_result_id": "tool-result-call-1",
        "tool_name": "vulcan_file_list",
        "display_state": "completed",
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3003",
        &finished,
    ));

    let visible_kinds = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.record_kind)
        .collect::<Vec<_>>();
    assert_eq!(
        visible_kinds,
        vec![
            SessionMessageRecordKind::Transcript,
            SessionMessageRecordKind::ToolCallOutput,
            SessionMessageRecordKind::AssistantOutput,
        ]
    );
    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("tool call record should remain visible");
    assert!(tool_record.title.is_empty());
    assert!(tool_record.body.is_empty());
    let metadata = tool_record.metadata.as_ref().expect("tool metadata");
    assert_eq!(
        metadata
            .get("result")
            .and_then(|result| result.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
    assert_eq!(
        metadata
            .get("result")
            .and_then(|result| result.get("tool_result_id"))
            .and_then(serde_json::Value::as_str),
        Some("tool-result-call-1")
    );
    assert!(metadata.get("tool_result_id").is_none());
    assert_eq!(
        metadata
            .get("result")
            .and_then(|result| result.get("detail_ref"))
            .and_then(|detail_ref| detail_ref.get("message_index"))
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert!(
        metadata
            .get("result")
            .and_then(|result| result.get("preview"))
            .is_none()
    );
    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("tool detail record should be hidden");
    assert!(!detail_record.visible);
    assert_eq!(detail_record.body, "listed files");
    let detail_metadata = detail_record.metadata.as_ref().expect("detail metadata");
    assert!(detail_metadata.get("model_output").is_none());
    assert!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("kind"))
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
}

/// Verifies WebUI visible tool cards keep raw change-set host metadata for expandable file-change rendering.
/// 验证 WebUI 可见工具卡片会保留原始 change-set 宿主元数据，用于文件变更展开渲染。
#[test]
fn tool_result_projection_preserves_raw_change_set_host_result() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-change · vulcan_file_create",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-change",
        "tool_name": "vulcan_file_create",
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3100",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-change",
        "## Result ID\n\n`tool-result-call-change`\n\n## Tool Result\n\ncreated file",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-change",
        "tool_result_id": "tool-result-call-change",
        "tool_name": "vulcan_file_create",
        "display_state": "completed",
        "host_result": {
            "kind": "change_set",
            "payload": {
                "mode": "applied",
                "files": [{
                    "change": "create",
                    "path": "src/demo.lua",
                    "content_mode": "full",
                    "content": "return 1\n"
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3101",
        &finished,
    ));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("tool metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("change_set")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("files"))
            .and_then(serde_json::Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("return 1\n")
    );
}

/// Verifies ask answers are persisted as structured display data instead of requiring Markdown parsing.
/// 验证 ask 答案会以结构化展示数据持久化，而不需要从 Markdown 文本反解析。
#[test]
fn tool_result_projection_preserves_structured_ask_display() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-ask · ask",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-ask",
        "tool_name": "ask",
        "arguments": {
            "questions": [{
                "header": "确认",
                "question": "是否继续？"
            }]
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3200",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-ask",
        "## Result ID\n\n`tool-result-call-ask`\n\n## Tool Result\n\n## User Answers\n\n- 确认: 继续\n\nContinue with these answers.",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-ask",
        "tool_result_id": "tool-result-call-ask",
        "tool_name": "ask",
        "display_state": "completed",
        "host_result": {
            "kind": "ask",
            "payload": {
                "answers": [["继续"]],
                "display": {
                    "kind": "ask_question",
                    "title": "向用户提问",
                    "ignored_answer_text": "用户忽略回答",
                    "questions": [{
                        "header": "确认",
                        "question": "是否继续？",
                        "answers": ["继续"],
                        "answered": true
                    }]
                }
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3201",
        &finished,
    ));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("ask tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("ask metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("ask")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("answers"))
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_str),
        Some("继续")
    );

    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("ask detail record should be hidden");
    let detail_metadata = detail_record
        .metadata
        .as_ref()
        .expect("ask detail metadata");
    assert_eq!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("ask_question")
    );
    assert_eq!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("questions"))
            .and_then(serde_json::Value::as_array)
            .and_then(|questions| questions.first())
            .and_then(|question| question.get("answers"))
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_str),
        Some("继续")
    );
}

/// Verifies returned tool payloads are displayed as completed even when legacy host-result hints describe a semantic failure.
/// 验证已返回工具载荷会显示为完成，即使旧宿主结果提示描述了语义失败。
#[test]
fn returned_tool_payload_wins_over_host_failure_hint() {
    let metadata = json!({
        "tool_result_id": "tool-result-call-1",
        "host_result": {
            "kind": "tool_error",
            "payload": {
                "status": "failed",
            },
        },
    });
    assert_eq!(
        tool_display_state_for_finished_event(Some(&metadata), false),
        "completed"
    );
}

/// Verifies a running TaskSession acknowledgement stays running even though the spawn-agent call returned.
/// 验证 TaskSession 运行中确认在 spawn_agent 调用返回后仍保持运行态。
#[test]
fn running_task_session_result_keeps_running_display_state() {
    let metadata = json!({
        "tool_result_id": "tool-result-call-task",
        "host_result": {
            "kind": "task_session",
            "payload": {
                "task_session_id": test_session_id().to_string(),
                "execution_status": "running",
            },
        },
        "display_state": "completed",
    });

    assert_eq!(
        tool_display_state_for_finished_event(Some(&metadata), true),
        "running"
    );
}

/// Projects visible records into comparable test tuples.
/// 将可见记录投影成可比较的测试元组。
fn visible_record_projection(
    runtime_state: &SessionRuntimeStateEntry,
) -> Vec<(SessionMessageKind, SessionMessageRecordKind, String)> {
    runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| (record.kind, record.record_kind, record.body.clone()))
        .collect()
}

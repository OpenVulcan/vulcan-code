use super::*;
use crate::conversation_protocol::SessionConversationState;
use serde_json::json;
use vulcan_core::types::ProviderUsageSnapshot;

/// Builds one deterministic test session identifier.
/// 构建一个确定性的测试会话标识。
fn test_session_id() -> SessionId {
    "11111111-1111-1111-1111-111111111111"
        .parse()
        .expect("test session id should be a valid UUID")
}

/// Builds one successful terminal report with deterministic provider usage.
/// 构建一份带确定性 Provider 用量的成功终态报告。
#[allow(clippy::too_many_arguments)]
fn usage_report(
    session_id: SessionId,
    assistant_output: &str,
    input_tokens: usize,
    output_tokens: usize,
    reasoning_tokens: usize,
    total_tokens: usize,
    cached_input_tokens: Option<usize>,
) -> ExecutionRunReport {
    let provider_usage = ProviderUsageSnapshot {
        input_tokens: Some(input_tokens),
        output_tokens: Some(output_tokens),
        reasoning_tokens: Some(reasoning_tokens),
        total_tokens: Some(total_tokens),
        cached_input_tokens,
        cache_creation_input_tokens: None,
    };
    ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(assistant_output.to_string()),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: Some(provider_usage.clone()),
        cumulative_successful_provider_usage: Some(provider_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    }
}

/// Extracts the latest visible turn-statistics metadata from a runtime state.
/// 从运行态中提取最近一条可见轮次统计元数据。
fn latest_turn_stats(
    runtime_state: &SessionRuntimeStateEntry,
) -> vulcan_workbench_model::SessionTurnStats {
    let stats_record = runtime_state
        .conversation_state
        .records
        .iter()
        .rev()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("turn stats record should exist");
    let stats_value = stats_record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("turn_stats"))
        .expect("turn stats metadata should exist")
        .clone();
    serde_json::from_value(stats_value).expect("turn stats metadata should deserialize")
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
        cumulative_successful_provider_usage: None,
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
            SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(canonical_texts.contains(&"随便调用两个工具"));
    assert!(canonical_texts.contains(&"报告中的完整输出不应覆盖已流式记录。"));
}

/// Verifies terminal reports attach provider usage when they append the protocol assistant message.
/// 验证终态报告追加协议助手消息时会携带 Provider 用量。
#[test]
fn terminal_report_attaches_usage_to_appended_assistant_message() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2100", "只输出 abc");
    let report = usage_report(session_id, "abc", 31, 2, 1, 34, Some(7));

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2101", &report);

    let assistant_usage = runtime_state
        .conversation_state
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { text, usage, .. } if text == "abc" => {
                usage.as_ref()
            }
            SessionConversationPayload::Assistant { .. }
            | SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .expect("terminal assistant message should carry usage");
    assert_eq!(assistant_usage.input_tokens, Some(31));
    assert_eq!(assistant_usage.output_tokens, Some(2));
    assert_eq!(assistant_usage.reasoning_tokens, Some(1));
    assert_eq!(assistant_usage.total_tokens, Some(34));
    assert_eq!(assistant_usage.cached_input_tokens, Some(7));

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 31);
    assert_eq!(turn_stats.output_tokens, 2);
    assert_eq!(turn_stats.reasoning_tokens, 1);
    assert_eq!(turn_stats.total_tokens, 34);
    assert_eq!(turn_stats.cached_input_tokens, Some(7));
}

/// Verifies child-agent usage embedded in a parent terminal report stays display-separated.
/// 验证父级终态报告内嵌的智能体用量会保持显示分离。
#[test]
fn terminal_report_keeps_embedded_subagent_usage_out_of_parent_totals() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2150", "创建智能体");
    let mut report = usage_report(session_id, "智能体完成。", 31, 2, 1, 34, Some(7));
    report
        .embedded_turn_usage_segments
        .push(SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::SubagentTask,
            trigger: Some(String::from("worker-a")),
            execution_profile: None,
            provider_id: Some(String::from("deepseek")),
            model_id: Some(String::from("deepseek-v4-pro")),
            provider_call_count: 1,
            tool_call_count: 0,
            input_tokens: 100,
            output_tokens: 25,
            reasoning_tokens: 5,
            total_tokens: 130,
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        });

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2151", &report);

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 31);
    assert_eq!(turn_stats.output_tokens, 2);
    assert_eq!(turn_stats.reasoning_tokens, 1);
    assert_eq!(turn_stats.total_tokens, 34);
    assert_eq!(turn_stats.cached_input_tokens, Some(7));
    assert_eq!(turn_stats.usage_segments.len(), 2);
    let subagent_group = turn_stats
        .usage_groups
        .iter()
        .find(|group| group.kind == vulcan_workbench_model::SessionTurnUsageGroupKind::Subagent)
        .expect("subagent usage group should be retained");
    assert_eq!(subagent_group.provider_call_count, 1);
    assert_eq!(subagent_group.input_tokens, 100);
    assert_eq!(subagent_group.output_tokens, 25);
    assert_eq!(subagent_group.reasoning_tokens, 5);
    assert_eq!(subagent_group.total_tokens, 130);
    assert_eq!(subagent_group.cached_input_tokens, Some(40));
}

/// Verifies terminal reports fill missing usage on an existing terminal protocol message.
/// 验证终态报告会为已有终态协议消息补齐缺失用量。
#[test]
fn terminal_report_attaches_usage_to_existing_assistant_message() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2200", "只输出 xyz");
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("existing-assistant-message"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("xyz"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        });
    let report = usage_report(session_id, "xyz", 13, 5, 0, 18, None);

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2201", &report);

    let assistant_messages = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { text, usage, .. } if text == "xyz" => {
                Some(usage)
            }
            SessionConversationPayload::Assistant { .. }
            | SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assistant_messages.len(), 1);
    let usage = assistant_messages[0]
        .as_ref()
        .expect("existing assistant message should receive usage");
    assert_eq!(usage.input_tokens, Some(13));
    assert_eq!(usage.output_tokens, Some(5));
    assert_eq!(usage.total_tokens, Some(18));

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 13);
    assert_eq!(turn_stats.output_tokens, 5);
    assert_eq!(turn_stats.total_tokens, 18);
}

/// Verifies terminal reports still project provider usage when canonical messages lost per-message usage.
/// 验证规范消息丢失逐消息用量时，终态报告仍会投影 Provider 用量。
#[test]
fn terminal_report_backfills_usage_when_checkpoint_messages_lack_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2300", "只输出 123");
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: None,
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("checkpoint-assistant-without-usage"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("123"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        }),
        provider_request_count: 1,
        last_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(101),
            output_tokens: Some(3),
            reasoning_tokens: Some(2),
            total_tokens: Some(106),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        }),
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2301", &report);

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 101);
    assert_eq!(turn_stats.output_tokens, 3);
    assert_eq!(turn_stats.reasoning_tokens, 2);
    assert_eq!(turn_stats.total_tokens, 106);
    assert_eq!(turn_stats.cached_input_tokens, Some(40));
    assert_eq!(turn_stats.usage_segments.len(), 1);
    assert_eq!(
        turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(turn_stats.usage_segments[0].provider_call_count, 0);
}

/// Verifies multi-call terminal reports use cumulative usage when checkpoint messages lost per-call usage.
/// 验证多次调用终态报告在检查点消息丢失逐调用用量时会使用累计用量。
#[test]
fn terminal_report_backfills_multi_call_usage_from_cumulative_snapshot() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2400", "连续调用工具后总结");
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: None,
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("checkpoint-final-assistant-without-usage"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("完成"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        }),
        provider_request_count: 7,
        last_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(26_442),
            output_tokens: Some(171),
            reasoning_tokens: Some(20),
            total_tokens: Some(26_633),
            cached_input_tokens: Some(25_856),
            cache_creation_input_tokens: None,
        }),
        cumulative_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(185_094),
            output_tokens: Some(1_197),
            reasoning_tokens: Some(140),
            total_tokens: Some(186_431),
            cached_input_tokens: Some(180_992),
            cache_creation_input_tokens: None,
        }),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2401", &report);

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 7);
    assert_eq!(turn_stats.input_tokens, 185_094);
    assert_eq!(turn_stats.output_tokens, 1_197);
    assert_eq!(turn_stats.reasoning_tokens, 140);
    assert_eq!(turn_stats.total_tokens, 186_431);
    assert_eq!(turn_stats.cached_input_tokens, Some(180_992));
    assert_eq!(turn_stats.usage_groups.len(), 1);
    assert_eq!(turn_stats.usage_groups[0].provider_call_count, 7);
    assert_eq!(turn_stats.usage_groups[0].input_tokens, 185_094);
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

/// Verifies display summaries collapse whitespace and truncate without changing visible semantics.
/// 验证展示摘要会折叠空白并在不改变可见语义的前提下截断。
#[test]
fn compact_display_summary_collapses_whitespace_and_truncates() {
    assert_eq!(
        compact_display_summary("  alpha\n\tbeta   gamma  "),
        "alpha beta gamma"
    );
    // Long source whose compacted form exceeds the display summary limit.
    // 压缩后超过展示摘要长度上限的长文本来源。
    let long_source = format!("{} tail", "a".repeat(241));
    // Expected text mirrors the existing first-240-characters truncation rule.
    // 期望文本保持既有的前 240 个字符截断规则。
    let expected = format!("{}...", "a".repeat(240));

    assert_eq!(compact_display_summary(&long_source), expected);
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

/// Verifies command_run host metadata stays visible so shell cards can mark RTK routing before expansion.
/// 验证 command_run 宿主元数据保留在可见行中，使 shell 卡片展开前即可标记 RTK 路由。
#[test]
fn tool_result_projection_preserves_command_run_rtk_host_result() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-shell · shell_command",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-shell",
        "tool_name": "shell_command",
        "arguments": {
            "command": "git status"
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3150",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-shell",
        "## Result ID\n\n`tool-result-call-shell`\n\n## Tool Result\n\nCommand: rtk git status\nRTK: applied (git)\nOriginal command: git status\n\n[stdout]\nmain",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-shell",
        "tool_result_id": "tool-result-call-shell",
        "tool_name": "shell_command",
        "display_state": "completed",
        "host_result": {
            "kind": "command_run",
            "payload": {
                "command": "rtk git status",
                "status": "completed",
                "rtk": {
                    "status": "applied",
                    "applied": true,
                    "original_command": "git status",
                    "rewritten_command": "rtk git status",
                    "routes": ["git"]
                }
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3151",
        &finished,
    ));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("shell tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("shell metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("command_run")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("rtk"))
            .and_then(|rtk| rtk.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("applied")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("rtk"))
            .and_then(|rtk| rtk.get("rewritten_command"))
            .and_then(serde_json::Value::as_str),
        Some("rtk git status")
    );
    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("shell detail record should be hidden");
    let display = detail_record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("display"))
        .expect("shell detail display");
    assert_eq!(
        display
            .get("rtk")
            .and_then(|rtk| rtk.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("applied")
    );
    assert_eq!(
        display
            .get("effective_command")
            .and_then(serde_json::Value::as_str),
        Some("rtk git status")
    );
}

/// Verifies wait_agent keeps child usage metadata on the visible tool row without copying child output text.
/// 验证 wait_agent 会在可见工具行保留子智能体用量元数据，同时不复制子智能体输出正文。
#[test]
fn tool_result_projection_preserves_compact_subagent_wait_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-wait · wait_agent",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait",
        "tool_name": "wait_agent",
        "arguments": {
            "target": "reviewer"
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3170",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-wait",
        "## Result ID\n\n`tool-result-call-wait`\n\n## Tool Result\n\nreviewer [completed]: large child output",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait",
        "tool_result_id": "tool-result-call-wait",
        "tool_name": "wait_agent",
        "display_state": "completed",
        "host_result": {
            "kind": "subagent_wait",
            "payload": {
                "tool": "wait_agent",
                "message": "Subagent terminal result received.",
                "activity_status": "terminal_update",
                "terminal_event_count": 1,
                "message_event_count": 0,
                "status_event_count": 0,
                "result": "reviewer [completed]: large child output",
                "agents": [{
                    "agent_id": "reviewer",
                    "subagent_task_id": "22222222-2222-2222-2222-222222222222",
                    "execution_id": "exec-1",
                    "kind": "completed",
                    "status": "completed",
                    "display_title": "reviewer",
                    "result": "large child output",
                    "usage_segments": [{
                        "kind": "subagent_task",
                        "trigger": "reviewer",
                        "execution_profile": null,
                        "provider_id": "deepseek",
                        "model_id": "deepseek-v4-pro",
                        "provider_call_count": 2,
                        "tool_call_count": 1,
                        "input_tokens": 100,
                        "output_tokens": 20,
                        "reasoning_tokens": 3,
                        "total_tokens": 123,
                        "cached_input_tokens": 50,
                        "cache_creation_input_tokens": null
                    }]
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3171",
        &finished,
    ));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("wait_agent tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("wait metadata");
    let host_payload = metadata
        .get("host_result")
        .and_then(|host_result| host_result.get("payload"))
        .expect("compact host payload should be retained");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("subagent_wait")
    );
    assert_eq!(
        host_payload.get("tool").and_then(serde_json::Value::as_str),
        Some("wait_agent")
    );
    assert_eq!(
        host_payload
            .get("activity_status")
            .and_then(serde_json::Value::as_str),
        Some("terminal_update")
    );
    assert_eq!(
        host_payload
            .get("terminal_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert!(host_payload.get("result").is_none());
    assert!(host_payload.get("message").is_none());
    let agent = host_payload
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .and_then(|agents| agents.first())
        .expect("compact agent should be retained");
    assert!(agent.get("result").is_none());
    assert_eq!(
        agent
            .get("usage_segments")
            .and_then(serde_json::Value::as_array)
            .and_then(|segments| segments.first())
            .and_then(|segment| segment.get("total_tokens"))
            .and_then(serde_json::Value::as_u64),
        Some(123)
    );
}

/// Verifies wait_agent child usage contributes to the parent turn usage groups.
/// 验证 wait_agent 子智能体用量会并入父级轮次用量分组。
#[test]
fn terminal_report_groups_compact_wait_agent_child_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "3180", "等待智能体");
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-wait-stats · wait_agent",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait-stats",
        "tool_name": "wait_agent",
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3181",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-wait-stats",
        "## Result ID\n\n`tool-result-call-wait-stats`\n\n## Tool Result\n\nworker [completed]: ok",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait-stats",
        "tool_result_id": "tool-result-call-wait-stats",
        "tool_name": "wait_agent",
        "display_state": "completed",
        "host_result": {
            "kind": "subagent_wait",
            "payload": {
                "tool": "wait_agent",
                "agents": [{
                    "agent_id": "worker",
                    "subagent_task_id": "22222222-2222-2222-2222-222222222222",
                    "execution_id": "exec-2",
                    "kind": "completed",
                    "status": "completed",
                    "usage_segments": [{
                        "kind": "subagent_task",
                        "trigger": "worker",
                        "execution_profile": null,
                        "provider_id": "deepseek",
                        "model_id": "deepseek-v4-pro",
                        "provider_call_count": 1,
                        "tool_call_count": 0,
                        "input_tokens": 80,
                        "output_tokens": 10,
                        "reasoning_tokens": 1,
                        "total_tokens": 91,
                        "cached_input_tokens": 30,
                        "cache_creation_input_tokens": null
                    }]
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3182",
        &finished,
    ));
    let report = usage_report(session_id, "已收到。", 40, 4, 0, 44, Some(12));

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "3183", &report);

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.total_tokens, 44);
    let subagent_group = turn_stats
        .usage_groups
        .iter()
        .find(|group| group.kind == vulcan_workbench_model::SessionTurnUsageGroupKind::Subagent)
        .expect("wait_agent child usage should be retained");
    assert_eq!(subagent_group.provider_call_count, 1);
    assert_eq!(subagent_group.input_tokens, 80);
    assert_eq!(subagent_group.output_tokens, 10);
    assert_eq!(subagent_group.reasoning_tokens, 1);
    assert_eq!(subagent_group.total_tokens, 91);
    assert_eq!(subagent_group.cached_input_tokens, Some(30));
}

/// Verifies RTK display projection requires the explicit structured status marker.
/// 验证 RTK 展示投影必须依赖显式结构化状态标记。
#[test]
fn tool_result_projection_requires_rtk_status_marker() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-shell-no-status · shell_command",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-shell-no-status",
        "tool_name": "shell_command",
        "arguments": {
            "command": "git status"
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3160",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-shell-no-status",
        "## Result ID\n\n`tool-result-call-shell-no-status`\n\n## Tool Result\n\nCommand: rtk git status\nRTK: applied (git)\nOriginal command: git status\n\n[stdout]\nmain",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-shell-no-status",
        "tool_result_id": "tool-result-call-shell-no-status",
        "tool_name": "shell_command",
        "display_state": "completed",
        "host_result": {
            "kind": "command_run",
            "payload": {
                "command": "rtk git status",
                "status": "completed",
                "rtk": {
                    "applied": true,
                    "original_command": "git status",
                    "rewritten_command": "rtk git status",
                    "routes": ["git"]
                }
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3161",
        &finished,
    ));

    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("shell detail record should be hidden");
    let display = detail_record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("display"))
        .expect("shell detail display");
    assert!(
        display
            .get("effective_command")
            .and_then(serde_json::Value::as_str)
            .is_none()
    );
    assert!(
        display
            .get("rtk")
            .and_then(|rtk| rtk.get("status"))
            .and_then(serde_json::Value::as_str)
            .is_none()
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

/// Verifies submitted Plan artifacts stay visible so WebUI can render the confirmation card.
/// 验证已提交计划产物保留在可见元数据中，使 WebUI 能渲染确认卡片。
#[test]
fn tool_result_projection_preserves_plan_artifact_host_result() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-plan · submit_plan",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-plan",
        "tool_name": "submit_plan",
        "arguments": {
            "name": "Test Plan"
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3300",
        &started,
    ));
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-plan",
        "## Result ID\n\n`tool-result-call-plan`\n\n## Tool Result\n\nPlan artifact saved.",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-plan",
        "tool_result_id": "tool-result-call-plan",
        "tool_name": "submit_plan",
        "display_state": "completed",
        "host_result": {
            "kind": "plan_artifact",
            "payload": {
                "name": "Test Plan",
                "path": "docs/plan/test-plan.md",
                "content": "# Test Plan\n\n- Step one",
                "updatedAt": "3301"
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3301",
        &finished,
    ));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("submit_plan tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("plan metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("plan_artifact")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("# Test Plan\n\n- Step one")
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

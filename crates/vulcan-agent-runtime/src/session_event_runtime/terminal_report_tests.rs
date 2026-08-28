use super::*;
use crate::conversation_protocol::SessionConversationState;
use vulcan_core::types::ProviderUsageSnapshot;

/// Builds one deterministic test session identifier.
/// 构建一个确定性的测试会话标识。
fn test_session_id() -> SessionId {
    "11111111-1111-1111-1111-111111111111"
        .parse()
        .expect("test session id should be a valid UUID")
}

/// Builds one successful execution report with authoritative Provider usage.
/// 构建一份携带权威 Provider 用量的成功执行报告。
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
        uncached_input_tokens: None,
        output_tokens: Some(output_tokens),
        reasoning_tokens: Some(reasoning_tokens),
        total_tokens: Some(total_tokens),
        cached_input_tokens,
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
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

/// Extracts the latest process-local terminal statistics projection.
/// 提取最新一条进程内终态统计投影。
fn latest_turn_stats(runtime_state: &SessionRuntimeStateEntry) -> SessionTurnStats {
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

/// Verifies terminal checkpoints replace only canonical process-local messages.
/// 验证终态检查点只替换进程内规范消息，不覆盖既有显示记录。
#[test]
fn checkpoint_preserves_existing_process_display_records() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("display-record-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("Output"),
            body: String::from("already visible"),
            metadata: None,
            created_at: String::from("1000"),
        });
    let records_before_checkpoint = runtime_state.conversation_state.records.clone();
    let checkpoint = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("checkpoint-assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("checkpoint output"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    apply_checkpoint_conversation_state(&mut runtime_state, checkpoint, "1001")
        .expect("checkpoint identities should be valid");

    assert_eq!(
        serde_json::to_value(&runtime_state.conversation_state.records)
            .expect("checkpoint records should serialize"),
        serde_json::to_value(records_before_checkpoint)
            .expect("preserved checkpoint records should serialize")
    );
    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert_eq!(runtime_state.updated_at, "1001");
}

/// Verifies fallback usage restores both aggregate and per-segment Provider call counts.
/// 验证回填用量会同时恢复聚合统计与逐分段 Provider 调用次数。
#[test]
fn terminal_report_backfills_complete_usage_when_checkpoint_omits_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
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
            uncached_input_tokens: None,
            output_tokens: Some(3),
            reasoning_tokens: Some(2),
            total_tokens: Some(106),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        }),
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2001", &report)
        .expect("terminal report identities should be valid");

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
    assert_eq!(turn_stats.usage_segments[0].provider_call_count, 1);
}

/// Verifies child-agent usage remains separate from direct parent totals.
/// 验证子代理用量保持独立，不会污染父级直接统计。
#[test]
fn terminal_report_separates_subagent_usage_from_parent_totals() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut report = usage_report(session_id, "done", 31, 2, 1, 34, Some(7));
    report
        .embedded_turn_usage_segments
        .push(SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::SubagentTask,
            trigger: Some(String::from("worker-a")),
            execution_profile: None,
            provider_id: Some(String::from("deepseek")),
            provider_name_snapshot: Some(String::from("DeepSeek")),
            model_id: Some(String::from("deepseek-v4-pro")),
            model_name_snapshot: Some(String::from("DeepSeek V4 Pro")),
            provider_call_count: 1,
            tool_call_count: 2,
            input_tokens: 100,
            uncached_input_tokens: Some(60),
            output_tokens: 25,
            reasoning_tokens: 5,
            total_tokens: 130,
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        });

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "3001", &report)
        .expect("terminal report identities should be valid");

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 31);
    assert_eq!(turn_stats.total_tokens, 34);
    let subagent_group = turn_stats
        .usage_groups
        .iter()
        .find(|group| group.kind == SessionTurnUsageGroupKind::Subagent)
        .expect("subagent group should exist");
    assert_eq!(subagent_group.provider_call_count, 1);
    assert_eq!(subagent_group.tool_call_count, 2);
    assert_eq!(subagent_group.input_tokens, 100);
    assert_eq!(subagent_group.total_tokens, 130);
}

/// Verifies a terminal identity collision leaves the complete runtime adapter unchanged.
/// 验证终态身份冲突会使完整运行时适配状态保持不变。
#[test]
fn terminal_report_apply_is_atomic_on_assistant_identity_collision() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_session_conversation_message(
        &mut runtime_state.conversation_state,
        SessionConversationMessage {
            id: format!("session-{session_id}-protocol-assistant-4000"),
            payload: SessionConversationPayload::UserText {
                text: String::from("collision owner"),
                attachments: Vec::new(),
            },
        },
    )
    .expect("initial collision owner should be valid");
    runtime_state.updated_at = String::from("before-terminal");
    let original_state = format!("{runtime_state:#?}");
    let report = usage_report(session_id, "terminal output", 10, 2, 0, 12, None);

    let error =
        apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "4000", &report)
            .expect_err("terminal assistant identity collision must fail");

    assert!(error.contains("duplicates id"));
    assert_eq!(format!("{runtime_state:#?}"), original_state);
}

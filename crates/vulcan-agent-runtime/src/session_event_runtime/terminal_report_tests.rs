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
///
/// # Notes
/// 说明
/// A child-agent usage segment is deliberately filtered out of the parent turn entirely (both its
/// totals/groups and its retained segment list), because that usage is accounted for by the root
/// lifetime ledger instead. This test therefore asserts the exclusion on every parent-facing surface
/// rather than expecting the parent turn to carry a `Subagent` group.
/// 子代理用量分段被有意从父回合中完全过滤（既不含其总数与分组，也不保留其分段记录），因为该用量由
/// 根生命周期累计负责统计。因此本测试在父级各个可见面上断言这一排除，而不是期望父回合携带
/// `Subagent` 分组。
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
    // The parent turn retains exactly its own provider call, never the child's extra call.
    // 父回合只保留自己的供应商调用，绝不并入子代理的额外调用。
    assert_eq!(
        turn_stats
            .usage_groups
            .iter()
            .map(|group| group.kind)
            .collect::<Vec<_>>(),
        vec![SessionTurnUsageGroupKind::Regular],
        "child usage must not add a parent-turn usage group"
    );
    assert_eq!(
        turn_stats
            .usage_segments
            .iter()
            .map(|segment| segment.kind)
            .collect::<Vec<_>>(),
        vec![SessionTurnUsageSegmentKind::AssistantResponse],
        "child usage must not be retained as a parent-turn usage segment"
    );
    // The child's larger figures must not leak into any parent-facing aggregate.
    // 子代理更大的数值不得渗入任何父级聚合。
    for group in &turn_stats.usage_groups {
        assert_eq!(
            group.total_tokens, 34,
            "no parent group may carry the child's total"
        );
        assert_eq!(
            group.input_tokens, 31,
            "no parent group may carry the child's input"
        );
        assert_ne!(
            group.tool_call_count, 2,
            "no parent group may carry the child's tool calls"
        );
    }
    assert!(
        !turn_stats
            .usage_segments
            .iter()
            .any(|segment| segment.kind == SessionTurnUsageSegmentKind::SubagentTask),
        "the child segment belongs to the root lifetime ledger, not the parent turn"
    );
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

/// Auxiliary finish preserves prior business results and accepts only real checkpoint conversation content.
/// 辅助结束保留既有业务结果，只接纳真实检查点对话内容。
#[test]
fn question_report_preserves_business_records_and_does_not_synthesize_completion() {
    // A real business completion already exists before the optional question is handled.
    // 可选问题处理前已存在真实业务完成。
    let session_id = test_session_id();
    // The source contains real final output, summary and user-turn statistics from normal completion.
    // 来源包含正常完成产生的真实最终输出、摘要及用户回合统计。
    let mut source = SessionRuntimeStateEntry::default();
    apply_terminal_execution_report_to_state(
        &mut source,
        session_id,
        "5000",
        &usage_report(session_id, "Original result", 10, 2, 0, 12, None),
    )
    .unwrap();
    source
        .tool_request_record_ids
        .insert("qa-request".into(), "qa-request-record".into());
    source
        .tool_call_record_ids
        .insert("qa-call".into(), "qa-call-record".into());
    // Both success and failure must leave the original business completion intact.
    // 成功和失败均必须保持原业务完成不变。
    let before = format!("{source:#?}");
    for failed in [false, true] {
        // A report-only assistant string is not authoritative conversation history or a question answer.
        // 仅报告中的助手字符串不是权威对话历史或问题答案。
        let mut report = usage_report(session_id, "Do not synthesize this", 20, 3, 0, 23, None);
        report.error = failed.then(|| "Auxiliary failure".into());
        // The auxiliary reducer only clears ephemeral tool correlation and updates the adapter timestamp.
        // 辅助归约器仅清除临时工具关联并更新适配器时间。
        let actual =
            build_question_execution_report_state(&source, session_id, "5001", &report).unwrap();
        // The full-state comparison also protects retained memory policy and pending business preparation.
        // 完整状态比较同时保护保留的记忆策略及待处理业务准备。
        let mut expected = source.clone();
        expected.tool_request_record_ids.clear();
        expected.tool_call_record_ids.clear();
        expected.updated_at = "5001".into();
        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));
        assert_eq!(format!("{source:#?}"), before);

        // A real checkpoint may append genuine assistant content without producing another business summary.
        // 真实检查点可以追加真实助手内容，但不产生另一份业务摘要。
        let mut checkpoint = source.conversation_state.clone();
        checkpoint.messages.push(SessionConversationMessage {
            id: "qa-checkpoint-assistant".into(),
            payload: SessionConversationPayload::Assistant {
                text: "Question execution ended".into(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![],
            },
        });
        report.conversation_state = Some(checkpoint.clone());
        // Expected records remain exactly those of the business task; V1 owns auxiliary status and usage.
        // 预期记录与业务任务完全一致；辅助状态和用量由 V1 拥有。
        let projected =
            build_question_execution_report_state(&source, session_id, "5002", &report).unwrap();
        assert_eq!(
            serde_json::to_value(&projected.conversation_state.messages).unwrap(),
            serde_json::to_value(&checkpoint.messages).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&projected.conversation_state.records).unwrap(),
            serde_json::to_value(&source.conversation_state.records).unwrap()
        );
    }
}

/// Invalid auxiliary checkpoints and wrong-session reports fail without mutating retained business state.
/// 无效辅助检查点及错误会话报告失败时不修改保留业务状态。
#[test]
fn question_report_rejects_identity_collision_and_session_mismatch_atomically() {
    // Use existing valid business content to expose accidental mutation on the error path.
    // 使用既有有效业务内容暴露错误路径的意外修改。
    let session_id = test_session_id();
    // Every error must leave this complete source unchanged.
    // 每个错误都必须保持此完整来源不变。
    let mut source = SessionRuntimeStateEntry::default();
    apply_terminal_execution_report_to_state(
        &mut source,
        session_id,
        "6000",
        &usage_report(session_id, "Original result", 10, 2, 0, 12, None),
    )
    .unwrap();
    // Clone one message identity deliberately to form an invalid provider checkpoint.
    // 故意克隆一个消息身份，形成无效供应商检查点。
    let mut checkpoint = source.conversation_state.clone();
    checkpoint.messages.push(checkpoint.messages[0].clone());
    // The report otherwise names the correct target session.
    // 报告其他字段仍指定正确目标会话。
    let mut report = usage_report(session_id, "Auxiliary output", 10, 2, 0, 12, None);
    report.conversation_state = Some(checkpoint);
    // Comparison includes memory policy, tool correlation and existing history.
    // 比较涵盖记忆策略、工具关联及既有历史。
    let before = format!("{source:#?}");
    assert!(
        build_question_execution_report_state(&source, session_id, "6001", &report)
            .unwrap_err()
            .contains("duplicates id")
    );
    report.session_id = SessionId::new_v4();
    assert!(
        build_question_execution_report_state(&source, session_id, "6001", &report)
            .unwrap_err()
            .contains("different session")
    );
    assert_eq!(format!("{source:#?}"), before);
}

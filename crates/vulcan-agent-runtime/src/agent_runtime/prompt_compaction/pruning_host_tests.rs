//! Regression coverage for host identities in clone-pruned summary requests.
//! 覆盖仅克隆剪枝摘要请求中宿主身份的回归。

use super::*;
use vulcan_context::runtime_host::{RuntimeHostEventMetadata, read_runtime_host_ledger};
use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionMessageKind, SessionMessageRecordKind,
};
use vulcan_context_compaction::{
    CompactionPlanInput, estimate_compaction_source_tokens, plan_context_compaction,
};

/// Verifies retry pruning leaves host instructions to exact rewrite backfill instead of making unauthenticated message fragments.
/// 验证重试剪枝把宿主指令交给精确改写回填，不生成失去身份的消息片段。
#[test]
fn pruned_summary_omits_host_rows_but_keeps_user_lookalikes_and_reports_them() {
    // This typed ledger mirrors the producer's required contract; the human lookalike has no ledger ownership.
    // 此强类型账本对应生产者必需契约；相似用户正文不具备账本归属。
    let metadata = RuntimeHostEventMetadata {
        version: 1,
        source_id: String::from("source"),
        occurrence_id: String::from("occ"),
        occurrence: 1,
        body_hash: String::from("hash"),
        event_id: String::from("event"),
        expires_source: false,
        envelope: vulcan_ai_contract::HostMessageEnvelope {
            event_id: String::from("event"),
            kind: vulcan_ai_contract::HostMessageKind::ContextUpdate,
            source: String::from("source"),
            target: String::from("model"),
            task_id: None,
            question_id: None,
            scope: String::from("session"),
            body: String::from("active"),
        },
        compacted_out: false,
    };
    let mut state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: metadata.message_id(),
            payload: SessionConversationPayload::UserText {
                text: metadata.envelope.render().unwrap(),
                attachments: Vec::new(),
            },
        }],
        records: vec![SessionConversationRecord {
            id: metadata.record_id(),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::ExploreOutput,
            visible: false,
            title: String::new(),
            body: String::from("active"),
            metadata: Some(serde_json::to_value(metadata).unwrap()),
            created_at: String::from("0"),
        }],
        ..Default::default()
    };
    vulcan_context::append_user_text_message(
        &mut state,
        "human",
        "<vulcan_context_update>human</vulcan_context_update>",
        Vec::new(),
    )
    .unwrap();
    for round in 0..4 {
        vulcan_context::append_assistant_text_message(
            &mut state,
            format!("assistant-{round}"),
            "completed",
        )
        .unwrap();
    }
    let original = serde_json::to_value(&state).unwrap();
    let source = conversation_state_to_compaction_source_messages(&state).unwrap();
    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "pruning-host",
        messages: &source,
        all_messages: &source,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: vulcan_context::ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 5_000,
            ..Default::default()
        },
    })
    .unwrap();
    let mode = CompactionInputPruneMode::RetryAfterFailure;
    let prepared = prepared_compaction_source_for_attempt(&plan, &state, None, mode).unwrap();
    assert_eq!(prepared.prune_report.dropped_source_message_count, 1);
    assert_eq!(prepared.prune_report.mode, mode);
    assert!(
        prepared
            .source_messages
            .iter()
            .any(|message| message.id == "human")
    );
    assert!(
        prepared
            .source_messages
            .iter()
            .all(|message| message.id != "runtime-host:event")
    );
    let attempt =
        compaction_attempt_conversation_state(&state, &prepared.source_messages, mode).unwrap();
    assert!(read_runtime_host_ledger(&attempt).unwrap().is_empty());
    let full =
        compaction_attempt_conversation_state(&state, &source, CompactionInputPruneMode::None)
            .unwrap();
    assert_eq!(read_runtime_host_ledger(&full).unwrap().len(), 1);
    // A configured budget takes the other production branch and must report the same protected-host exclusion.
    // 配置预算会进入另一条生产分支，仍必须报告相同的受保护宿主排除。
    let budgeted = prepared_compaction_source_for_attempt(
        &plan,
        &state,
        Some(CompactionInputBudget {
            target_input_budget_tokens: 5_000,
            safety_reserve_tokens: 128,
        }),
        mode,
    )
    .unwrap();
    assert_eq!(budgeted.prune_report.mode, mode);
    assert_eq!(budgeted.prune_report.dropped_source_message_count, 1);
    assert_eq!(
        budgeted.prune_report.pruned_token_estimate,
        estimate_compaction_source_tokens(&budgeted.source_messages)
    );
    assert!(!budgeted.prune_report.still_exceeds_budget);
    let budgeted_attempt =
        compaction_attempt_conversation_state(&state, &budgeted.source_messages, mode).unwrap();
    assert!(
        read_runtime_host_ledger(&budgeted_attempt)
            .unwrap()
            .is_empty()
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), original);
}

/// Actual runtime attempt preparation preserves native checkpoints while shrinking dialogue and counting opaque input.
/// 实际运行时尝试准备在缩减对话时保留原生检查点，并计入不透明输入。
#[test]
fn pruned_attempt_retains_native_checkpoint_and_its_budget() {
    // Native state is supplied only by a canonical assistant carrier.
    // 原生状态只由规范助手载体提供。
    let mut state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: "native-checkpoint".into(),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: Some(
                    serde_json::json!({"vulcan_ai":{"responses_compaction":{
                        "provider_id":"provider", "connection_id":"connection",
                        "route_fingerprint":"https://provider.invalid/responses",
                        "protocol_profile_id":"openai-codex-responses.v1", "model_id":"model",
                        "items":[{"type":"compaction","encrypted_content":"opaque".repeat(2000)}]
                    }}}),
                ),
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        ..Default::default()
    };
    for round in 0..4 {
        vulcan_context::append_assistant_text_message(
            &mut state,
            format!("assistant-{round}"),
            "long-dialogue".repeat(2000),
        )
        .unwrap();
    }
    // Keep a byte-exact source baseline to detect accidental in-place pruning.
    // 保留字节精确的来源基线，以发现意外原地剪枝。
    let original = serde_json::to_value(&state).unwrap();
    // The real planner produces the prefix and retained tail before attempt preparation.
    // 真实规划器在尝试准备前产生前缀及保留尾部。
    let rows = conversation_state_to_compaction_source_messages(&state).unwrap();
    // Freeze a single newest assistant segment as the retained suffix.
    // 冻结单个最新助手执行段作为保留后缀。
    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "native-pruning",
        messages: &rows,
        all_messages: &rows,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: vulcan_context::ContextCompactionPolicy {
            tail_turns: 1,
            ..Default::default()
        },
    })
    .unwrap();
    // Opaque content alone exceeds this heuristic budget and must remain intact.
    // 不透明内容本身已超过此启发式预算，仍必须保持完整。
    let budget = CompactionInputBudget {
        target_input_budget_tokens: 512,
        safety_reserve_tokens: 0,
    };
    // The actual retry path must preserve the checkpoint even when only one text row survives.
    // 即使只剩一条文字行，实际重试路径也必须保留检查点。
    let prepared = prepared_compaction_source_for_attempt(
        &plan,
        &state,
        Some(budget),
        CompactionInputPruneMode::RetryAfterFailure,
    )
    .unwrap();
    assert_eq!(prepared.source_messages[0].id, "native-checkpoint");
    assert!(prepared.prune_report.dropped_source_message_count > 0);
    assert!(prepared.prune_report.still_exceeds_budget);
    assert!(
        prepared.prune_report.pruned_token_estimate
            > estimate_compaction_source_tokens(&prepared.source_messages)
    );
    // The shared restore must match the runtime's original native payload exactly.
    // 共用恢复必须精确匹配运行时原始原生载荷。
    let attempt = compaction_attempt_conversation_state(
        &state,
        &prepared.source_messages,
        CompactionInputPruneMode::RetryAfterFailure,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&attempt.messages[0]).unwrap(),
        original["messages"][0]
    );
    assert!(
        attempt
            .messages
            .iter()
            .all(|message| message.id != "assistant-3")
    );
    // Without observations, initial mode selection must count native bytes instead of seeing an empty checkpoint.
    // 没有观测值时，初始模式选择必须计入原生字节，不能把检查点视为空。
    let native_only = SessionConversationState {
        messages: vec![state.messages[0].clone()],
        ..Default::default()
    };
    assert_eq!(
        initial_compaction_prune_mode(&native_only, Some(budget), None, None).unwrap(),
        CompactionInputPruneMode::RequiredForWindowDowngrade
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), original);
}

use super::pruning::{
    compaction_attempt_conversation_state, compaction_prompt_mode_for_prune_mode,
    initial_compaction_prune_mode, next_compaction_retry_mode,
    prepared_compaction_source_for_attempt, prompt_compaction_error_is_context_overflow,
};
use super::*;
use vulcan_context::conversation_state_from_session_messages;

fn test_budget() -> CompactionInputBudget {
    CompactionInputBudget {
        target_input_budget_tokens: 100,
        safety_reserve_tokens: 30_000,
    }
}

/// Builds one transcript message entry for compaction helper tests.
/// 为压缩辅助函数测试构建一条转录消息。
fn test_message(id: &str, body: &str) -> vulcan_context::SessionMessageEntry {
    vulcan_context::SessionMessageEntry::new(vulcan_context::SessionMessageKind::User, "", body)
        .with_runtime_metadata(id, "t1")
}

/// Builds one minimal compaction plan from selected source messages.
/// 从选定源消息构建一个最小压缩计划。
fn test_plan(source_messages: Vec<vulcan_context::SessionMessageEntry>) -> CompactionPlan {
    CompactionPlan {
        source_messages,
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: String::from("compaction-test"),
        created_at: String::from("t1"),
        policy: Default::default(),
    }
}

/// Verifies the fast path uses the full cloned runtime conversation, not the planned source window.
/// 验证快速路径使用完整克隆运行时会话，而不是计划源窗口。
#[test]
fn full_fidelity_attempt_uses_full_context_state() {
    let old_message = test_message("old-user", "old context");
    let tail_message = test_message("tail-user", "recent tail");
    let full_context_state =
        conversation_state_from_session_messages(&[old_message.clone(), tail_message.clone()])
            .expect("valid replayed conversation identities");
    let plan = test_plan(vec![old_message]);
    let prepared = prepared_compaction_source_for_attempt(
        &plan,
        &full_context_state,
        Some(test_budget()),
        CompactionInputPruneMode::None,
    );
    let attempt_state = compaction_attempt_conversation_state(
        &full_context_state,
        &prepared.source_messages,
        CompactionInputPruneMode::None,
    )
    .expect("valid compaction attempt identities");

    assert_eq!(
        attempt_state.messages.len(),
        full_context_state.messages.len()
    );
    assert_eq!(prepared.source_messages.len(), 2);
    assert_eq!(prepared.prune_report.mode, CompactionInputPruneMode::None);
}

/// Verifies full-fidelity summaries still commit the planner boundary.
/// 验证完整保真摘要仍按规划器边界提交。
#[test]
fn full_fidelity_result_boundary_preserves_planned_tail() {
    let old_message = test_message("old-user", "old context");
    let mut plan = test_plan(vec![old_message.clone()]);
    plan.source_until_message_id = Some(old_message.id.clone());
    plan.source_until_turn_index = Some(0);
    plan.retained_tail_start_turn_index = Some(1);
    let result = CompactionResult::from_model_summary(&plan, "summary");

    assert_eq!(
        result.source_until_message_id.as_deref(),
        Some(old_message.id.as_str())
    );
    assert_eq!(result.source_until_turn_index, Some(0));
    assert_eq!(result.retained_tail_start_turn_index, Some(1));
}

/// Verifies applying the planned boundary keeps recent tail messages.
/// 验证应用规划边界后会保留最近尾部消息。
#[test]
fn full_fidelity_boundary_rewrite_keeps_planned_tail() {
    let old_message = test_message("old-user", "old context");
    let tail_message = test_message("tail-user", "recent tail");
    let mut state =
        conversation_state_from_session_messages(&[old_message.clone(), tail_message.clone()])
            .expect("valid replayed conversation identities");
    let mut plan = test_plan(vec![old_message.clone()]);
    plan.source_until_message_id = Some(old_message.id.clone());
    let result = CompactionResult::from_model_summary(&plan, "summary");

    vulcan_context_compaction::compact_conversation_with_summary_until(
        &mut state,
        "summary-message",
        vulcan_context::SessionMessageKind::User,
        "summary",
        "t2",
        result.source_until_message_id.as_deref(),
        result.policy,
    )
    .expect("valid compacted message identities");

    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].id, "summary-message");
    assert_eq!(state.messages[1].id, tail_message.id);
}

/// Verifies prompt variants follow the prepared input pruning mode.
/// 验证提示词变体会跟随已准备输入的剪枝模式。
#[test]
fn prompt_mode_follows_input_prune_mode() {
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::None),
        ContextCompactionRequestPromptMode::FullFidelityMerge
    );
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::RequiredForWindowDowngrade),
        ContextCompactionRequestPromptMode::PrunedFallback
    );
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::RetryAfterFailure),
        ContextCompactionRequestPromptMode::PrunedFallback
    );
}

/// Verifies observed provider usage decides initial downgrade before local estimates.
/// 验证 Provider 实测用量优先于本地估算决定初始降级。
#[test]
fn observed_usage_drives_initial_downgrade_decision() {
    let full_context_state =
        conversation_state_from_session_messages(&[test_message("small-user", "small")])
            .expect("valid replayed conversation identities");

    assert_eq!(
        initial_compaction_prune_mode(&full_context_state, Some(test_budget()), Some(101), Some(1),),
        CompactionInputPruneMode::RequiredForWindowDowngrade
    );
}

/// Verifies live budget estimates are used when provider usage is unavailable.
/// 验证 Provider 用量不可用时使用实时预算估算。
#[test]
fn budget_estimate_drives_initial_downgrade_without_observed_usage() {
    let full_context_state =
        conversation_state_from_session_messages(&[test_message("small-user", "small")])
            .expect("valid replayed conversation identities");

    assert_eq!(
        initial_compaction_prune_mode(&full_context_state, Some(test_budget()), None, Some(101),),
        CompactionInputPruneMode::RequiredForWindowDowngrade
    );
}

/// Verifies explicit context-overflow failures skip full-fidelity retries.
/// 验证明示上下文溢出失败会跳过完整保真重试。
#[test]
fn context_overflow_error_immediately_downgrades() {
    assert!(prompt_compaction_error_is_context_overflow(
        "maximum context length exceeded"
    ));
    assert_eq!(
        next_compaction_retry_mode(
            "maximum context length exceeded",
            CompactionInputPruneMode::None,
            1,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RetryAfterFailure)
    );
}

/// Verifies non-context failures keep retrying full-fidelity input before downgrade.
/// 验证非上下文失败会先继续重试完整保真输入。
#[test]
fn non_context_error_retries_full_fidelity_before_downgrade() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            1,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            2,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            3,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RetryAfterFailure)
    );
}

/// Verifies missing input budget allows full retries but never clone-pruned downgrade.
/// 验证缺少输入预算时允许完整重试但不会进入克隆剪枝降级。
#[test]
fn missing_budget_never_downgrades_to_pruning() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            1,
            0,
            None,
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            3,
            0,
            None,
        ),
        None
    );
}

/// Verifies clone-pruned attempts still retry transient non-context failures.
/// 验证仅克隆剪枝尝试遇到非上下文临时失败时仍会重试。
#[test]
fn pruned_attempt_retries_non_context_failures_before_stopping() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::RequiredForWindowDowngrade,
            0,
            1,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RequiredForWindowDowngrade)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::RequiredForWindowDowngrade,
            0,
            3,
            Some(test_budget()),
        ),
        None
    );
}

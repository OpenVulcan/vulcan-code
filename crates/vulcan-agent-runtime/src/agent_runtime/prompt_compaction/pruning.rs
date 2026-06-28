//! Prompt-compaction input pruning and retry helpers.
//! 提示词压缩输入剪枝与重试辅助逻辑。

use vulcan_context::{
    SessionConversationState, conversation_state_from_session_messages,
    conversation_state_to_compaction_source_messages,
};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    ContextCompactionRequestPromptMode, estimate_compaction_source_tokens,
    prepare_compaction_source_for_target_budget,
};

/// Maximum full-fidelity prompt-compaction attempts before clone-only downgrade is allowed.
/// 允许仅克隆降级前的最大完整保真提示词压缩尝试次数。
const MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS: usize = 3;

/// Selects the initial pruning mode from the target compaction input budget.
/// 根据目标压缩输入预算选择初始剪枝模式。
///
/// # Parameters
/// # 参数
/// `plan` is the immutable compaction plan whose source size is checked.
/// `plan` 是需要检查源消息大小的不可变压缩计划。
/// `input_budget` is the optional target input budget for the selected model window.
/// `input_budget` 是所选模型窗口对应的可选目标输入预算。
///
/// # Returns
/// # 返回值
/// Returns a pruning mode that either keeps full input or requires clone-only window-downgrade pruning.
/// 返回保留完整输入或要求仅克隆窗口降级剪枝的剪枝模式。
pub(super) fn initial_compaction_prune_mode(
    full_context_state: &SessionConversationState,
    input_budget: Option<CompactionInputBudget>,
    observed_input_tokens: Option<usize>,
    estimated_input_tokens: Option<usize>,
) -> CompactionInputPruneMode {
    let Some(input_budget) = input_budget else {
        return CompactionInputPruneMode::None;
    };
    let input_tokens = observed_input_tokens
        .or(estimated_input_tokens)
        .unwrap_or_else(|| {
            estimate_compaction_source_tokens(&conversation_state_to_compaction_source_messages(
                full_context_state,
            ))
        });
    if input_tokens > input_budget.target_input_budget_tokens {
        CompactionInputPruneMode::RequiredForWindowDowngrade
    } else {
        CompactionInputPruneMode::None
    }
}

/// Builds the source message list for one prompt-compaction attempt.
/// 构建单次提示词压缩尝试使用的源消息列表。
///
/// # Parameters
/// # 参数
/// `plan` is the immutable compaction plan for the current attempt.
/// `plan` 是当前尝试使用的不可变压缩计划。
/// `input_budget` is the optional target model input budget.
/// `input_budget` 是可选的目标模型输入预算。
/// `prune_mode` is the mode selected for this attempt.
/// `prune_mode` 是本次尝试选定的剪枝模式。
///
/// # Returns
/// # 返回值
/// Returns an owned prepared source and its pruning report without mutating the plan.
/// 返回拥有所有权的已准备源消息及其剪枝报告，不会修改计划。
pub(super) fn prepared_compaction_source_for_attempt(
    plan: &CompactionPlan,
    full_context_state: &SessionConversationState,
    input_budget: Option<CompactionInputBudget>,
    prune_mode: CompactionInputPruneMode,
) -> vulcan_context_compaction::PreparedCompactionSource {
    if prune_mode == CompactionInputPruneMode::None {
        return unpruned_full_context_compaction_source(full_context_state, input_budget);
    }
    let Some(input_budget) = input_budget else {
        let source_token_estimate = estimate_compaction_source_tokens(&plan.source_messages);
        return vulcan_context_compaction::PreparedCompactionSource {
            source_messages: plan.source_messages.clone(),
            prune_report: CompactionInputPruneReport {
                mode: CompactionInputPruneMode::None,
                target_input_budget_tokens: 0,
                safety_reserve_tokens: 0,
                original_token_estimate: source_token_estimate,
                pruned_token_estimate: source_token_estimate,
                pruned_tool_output_count: 0,
                preserved_tool_output_count: 0,
                dropped_source_message_count: 0,
                truncated_source_message_count: 0,
                still_exceeds_budget: false,
            },
        };
    };
    prepare_compaction_source_for_target_budget(
        &plan.source_messages,
        input_budget,
        plan.policy,
        prune_mode,
    )
}

/// Builds the full-fidelity source used by the fast compaction path.
/// 构建快速压缩路径使用的完整保真来源。
fn unpruned_full_context_compaction_source(
    full_context_state: &SessionConversationState,
    input_budget: Option<CompactionInputBudget>,
) -> vulcan_context_compaction::PreparedCompactionSource {
    let source_messages = conversation_state_to_compaction_source_messages(full_context_state);
    let source_token_estimate = estimate_compaction_source_tokens(&source_messages);
    vulcan_context_compaction::PreparedCompactionSource {
        source_messages,
        prune_report: CompactionInputPruneReport {
            mode: CompactionInputPruneMode::None,
            target_input_budget_tokens: input_budget
                .map(|budget| budget.target_input_budget_tokens)
                .unwrap_or_default(),
            safety_reserve_tokens: input_budget
                .map(|budget| budget.safety_reserve_tokens)
                .unwrap_or_default(),
            original_token_estimate: source_token_estimate,
            pruned_token_estimate: source_token_estimate,
            pruned_tool_output_count: 0,
            preserved_tool_output_count: 0,
            dropped_source_message_count: 0,
            truncated_source_message_count: 0,
            still_exceeds_budget: false,
        },
    }
}

/// Returns the conversation state for one compaction attempt.
/// 返回单次压缩尝试使用的会话状态。
pub(super) fn compaction_attempt_conversation_state(
    full_context_state: &SessionConversationState,
    source_messages: &[vulcan_context::SessionMessageEntry],
    prune_mode: CompactionInputPruneMode,
) -> SessionConversationState {
    if prune_mode == CompactionInputPruneMode::None {
        return full_context_state.clone();
    }
    conversation_state_from_session_messages(source_messages)
}

/// Selects the next retry mode after a prompt-compaction attempt fails.
/// 在一次提示词压缩尝试失败后选择下一次重试模式。
///
/// # Parameters
/// # 参数
/// `error` is the terminal error text produced by the failed attempt.
/// `error` 是失败尝试产出的终态错误文本。
/// `current_mode` is the pruning mode used by the failed attempt.
/// `current_mode` 是失败尝试使用的剪枝模式。
/// `full_fidelity_attempts` is the number of full-input attempts already made.
/// `full_fidelity_attempts` 是已经执行过的完整输入尝试次数。
/// `pruned_attempts` is the number of clone-pruned attempts already made.
/// `pruned_attempts` 是已经执行过的仅克隆剪枝尝试次数。
/// `input_budget` is the optional target input budget required for downgrade pruning.
/// `input_budget` 是降级剪枝所需的可选目标输入预算。
///
/// # Returns
/// # 返回值
/// Returns the next mode to retry, or `None` when no safe retry remains.
/// 返回下一次重试模式；当不存在安全重试时返回 `None`。
pub(super) fn next_compaction_retry_mode(
    error: &str,
    current_mode: CompactionInputPruneMode,
    full_fidelity_attempts: usize,
    pruned_attempts: usize,
    input_budget: Option<CompactionInputBudget>,
) -> Option<CompactionInputPruneMode> {
    if current_mode != CompactionInputPruneMode::None {
        return (pruned_attempts < MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS).then_some(current_mode);
    }
    if prompt_compaction_error_is_context_overflow(error) {
        return input_budget.map(|_| CompactionInputPruneMode::RetryAfterFailure);
    }
    if full_fidelity_attempts < MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS {
        return Some(CompactionInputPruneMode::None);
    }
    input_budget.map(|_| CompactionInputPruneMode::RetryAfterFailure)
}

/// Returns the clone-only pruning report only when the successful attempt used pruning.
/// 仅在成功尝试使用剪枝时返回仅克隆剪枝报告。
///
/// # Parameters
/// # 参数
/// `prune_report` is the report produced for the successful model-facing input.
/// `prune_report` 是成功的模型侧输入对应的剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns `Some` only when clone-only pruning was used; otherwise returns `None`.
/// 仅当使用了仅克隆剪枝时返回 `Some`，否则返回 `None`。
pub(super) fn input_prune_report_for_execution(
    prune_report: &CompactionInputPruneReport,
) -> Option<CompactionInputPruneReport> {
    (prune_report.mode != CompactionInputPruneMode::None).then(|| prune_report.clone())
}

/// Selects the compaction prompt variant that matches one prepared input pruning mode.
/// 选择与已准备输入剪枝模式匹配的压缩提示词变体。
///
/// # Parameters
/// # 参数
/// `prune_mode` is the actual pruning mode used by the prepared compaction source.
/// `prune_mode` 是已准备压缩源实际使用的剪枝模式。
///
/// # Returns
/// # 返回值
/// Returns the full-fidelity merge prompt for unpruned input, otherwise the pruned-fallback prompt.
/// 未剪枝输入返回完整保真合并提示词，否则返回剪枝降级提示词。
pub(super) fn compaction_prompt_mode_for_prune_mode(
    prune_mode: CompactionInputPruneMode,
) -> ContextCompactionRequestPromptMode {
    if prune_mode == CompactionInputPruneMode::None {
        ContextCompactionRequestPromptMode::FullFidelityMerge
    } else {
        ContextCompactionRequestPromptMode::PrunedFallback
    }
}

/// Returns whether one provider or preparation error clearly indicates context overflow.
/// 判断 provider 或准备错误是否明确表示上下文溢出。
///
/// # Parameters
/// # 参数
/// `error` is the provider, bootstrap, preparation, or summary extraction error text.
/// `error` 是 provider、bootstrap、准备阶段或摘要提取阶段的错误文本。
///
/// # Returns
/// # 返回值
/// Returns `true` only for known context-window overflow phrases.
/// 仅在匹配已知上下文窗口溢出短语时返回 `true`。
pub(super) fn prompt_compaction_error_is_context_overflow(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    [
        "prompt too long",
        "context length exceeded",
        "maximum context length",
        "max context length",
        "input too long",
        "context window",
        "too many tokens",
        "token limit",
        "tokens exceed",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

/// Appends clone-only pruning diagnostics to a terminal compaction failure.
/// 将仅克隆剪枝诊断追加到终态压缩失败信息中。
///
/// # Parameters
/// # 参数
/// `error` is the original terminal error text.
/// `error` 是原始终态错误文本。
/// `prune_report` is the report produced for the failed attempt input.
/// `prune_report` 是失败尝试输入对应的剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns the original error or the error with clone-only pruning diagnostics appended.
/// 返回原始错误，或追加了仅克隆剪枝诊断的错误。
pub(super) fn append_prune_report_to_error(
    error: String,
    prune_report: &CompactionInputPruneReport,
) -> String {
    if prune_report.mode == CompactionInputPruneMode::None {
        return error;
    }
    format!(
        "{error}\n\nCompaction input pruning: mode={:?}, target_budget={}, safety_reserve={}, original_tokens={}, pruned_tokens={}, pruned_tool_outputs={}, preserved_tool_outputs={}, dropped_source_messages={}, truncated_source_messages={}, still_exceeds_budget={}.",
        prune_report.mode,
        prune_report.target_input_budget_tokens,
        prune_report.safety_reserve_tokens,
        prune_report.original_token_estimate,
        prune_report.pruned_token_estimate,
        prune_report.pruned_tool_output_count,
        prune_report.preserved_tool_output_count,
        prune_report.dropped_source_message_count,
        prune_report.truncated_source_message_count,
        prune_report.still_exceeds_budget
    )
}

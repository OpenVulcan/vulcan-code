//! Retry policy for full-context compaction.
//! 完整上下文压缩的重试策略。

use vulcan_ai_contract::AiErrorCategory;

use crate::agent_runtime::provider_transport_retry::provider_error_category_is_deterministic_terminal;

/// Maximum number of attempts using the same frozen request prefix.
/// 使用同一冻结请求前缀的最大尝试次数。
const MAX_COMPACTION_ATTEMPTS: usize = 3;

/// Allows another identical compaction attempt while the retry budget remains.
/// 重试额度尚未用完时，允许再次执行同一压缩请求。
/// `category` is the terminal Provider classification, `error` is the diagnostic, and `attempts`
/// counts attempts already made. Returns false for deterministic failures and exhausted budgets.
/// `category` 是供应商终态分类，`error` 为诊断，`attempts` 为已执行次数。
/// 确定性失败或额度耗尽时返回 false。
pub(super) fn should_retry_compaction(
    category: Option<AiErrorCategory>,
    error: &str,
    attempts: usize,
) -> bool {
    if category.is_some_and(provider_error_category_is_deterministic_terminal) {
        return false;
    }
    // lowered matches the known local and upstream overflow diagnostics without modifying the error we report.
    // lowered 匹配已知的本地与上游超限诊断，同时保留原错误用于报告。
    let lowered = error.to_ascii_lowercase();
    attempts < MAX_COMPACTION_ATTEMPTS
        && ![
            "prompt too long",
            "prompt is too long",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a known payment rejection cannot spend another compaction attempt.
    /// 验证已知余额拒绝不会消耗下一次压缩尝试。
    #[test]
    fn balance_rejection_is_terminal_for_compaction() {
        assert!(!should_retry_compaction(
            Some(AiErrorCategory::PaymentRequired),
            "rejected",
            1
        ));
        assert!(should_retry_compaction(
            Some(AiErrorCategory::ProviderUnavailable),
            "unavailable",
            1
        ));
    }
}

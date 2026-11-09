//! Retry policy for full-context compaction.
//! 完整上下文压缩的重试策略。

/// Maximum number of attempts using the same frozen request prefix.
/// 使用同一冻结请求前缀的最大尝试次数。
const MAX_COMPACTION_ATTEMPTS: usize = 3;

/// Allows another identical compaction attempt while the retry budget remains.
/// 重试额度尚未用完时，允许再次执行同一压缩请求。
/// `error` is the failure diagnostic and `attempts` is the number already made. Returns false for context limits.
/// `error` 为失败诊断，`attempts` 为已执行次数。上下文超限时返回 false。
pub(super) fn should_retry_compaction(error: &str, attempts: usize) -> bool {
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

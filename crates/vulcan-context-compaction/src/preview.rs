//! Unicode-safe historical tool-output previews shared by planning and rewrite.
//! 供规划与改写共享的 Unicode 安全历史工具输出预览。

use vulcan_context::{estimate_mixed_language_text_tokens, is_cjk_character};

/// Estimated-token threshold above which one historical tool output is previewed.
/// 单条历史工具输出超过该估算 Token 阈值后应用预览。
pub const TOOL_OUTPUT_PREVIEW_THRESHOLD_TOKENS: usize = 2_500;

/// Maximum estimated tokens retained from the beginning of a historical tool output.
/// 历史工具输出头部最多保留的估算 Token 数。
pub const TOOL_OUTPUT_PREVIEW_HEAD_TOKENS: usize = 500;

/// Maximum estimated tokens retained from the end of a historical tool output.
/// 历史工具输出尾部最多保留的估算 Token 数。
pub const TOOL_OUTPUT_PREVIEW_TAIL_TOKENS: usize = 500;

/// Stable marker prefix used to detect an already previewed tool output.
/// 用于检测工具输出已经被预览过的稳定标记前缀。
pub const TOOL_OUTPUT_PREVIEW_MARKER_PREFIX: &str = "[上下文压缩省略：";

/// Model-facing preview and token accounting for one historical tool output.
/// 单条历史工具输出的模型侧预览与 Token 统计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputPreview {
    /// Original or previewed model-facing text.
    /// 原始或预览后的模型侧文本。
    pub text: String,
    /// Estimated tokens in the original text.
    /// 原始文本的估算 Token 数。
    pub original_token_estimate: usize,
    /// Estimated tokens retained from the beginning.
    /// 从头部保留的估算 Token 数。
    pub head_token_estimate: usize,
    /// Estimated tokens retained from the end.
    /// 从尾部保留的估算 Token 数。
    pub tail_token_estimate: usize,
    /// Estimated original tokens omitted from the model-facing projection.
    /// 从模型侧投影中省略的原始估算 Token 数。
    pub omitted_token_estimate: usize,
    /// Whether this call created a new preview.
    /// 本次调用是否新生成了预览。
    pub previewed: bool,
}

/// Builds an idempotent Unicode-safe head/tail preview for one historical tool output.
/// 为单条历史工具输出构建幂等且 Unicode 安全的首尾预览。
///
/// # Parameters
/// # 参数
/// `text` is the immutable original tool-result text projected to the model.
/// `text` 是投影给模型的不可变原始工具结果文本。
///
/// # Returns
/// # 返回值
/// The unchanged text below the threshold or when already marked; otherwise a head/marker/tail preview.
/// 低于阈值或已有标记时返回原文；否则返回头部、标记与尾部组成的预览。
pub fn preview_tool_output(text: &str) -> ToolOutputPreview {
    // originalTokenEstimate uses the same mixed-language estimator as context budgeting.
    // originalTokenEstimate 使用与上下文预算相同的混合语言估算器。
    let original_token_estimate = estimate_mixed_language_text_tokens(text);
    if original_token_estimate <= TOOL_OUTPUT_PREVIEW_THRESHOLD_TOKENS
        || text.contains(TOOL_OUTPUT_PREVIEW_MARKER_PREFIX)
    {
        return ToolOutputPreview {
            text: text.to_string(),
            original_token_estimate,
            head_token_estimate: original_token_estimate,
            tail_token_estimate: 0,
            omitted_token_estimate: 0,
            previewed: false,
        };
    }

    // headEnd and tailStart are always UTF-8 character boundaries produced by char_indices.
    // headEnd 与 tailStart 始终是 char_indices 生成的 UTF-8 字符边界。
    let head_end = prefix_end_for_token_budget(text, TOOL_OUTPUT_PREVIEW_HEAD_TOKENS);
    let tail_start = suffix_start_for_token_budget(text, TOOL_OUTPUT_PREVIEW_TAIL_TOKENS, head_end);
    let head = &text[..head_end];
    let tail = &text[tail_start..];
    let head_token_estimate = estimate_mixed_language_text_tokens(head);
    let tail_token_estimate = estimate_mixed_language_text_tokens(tail);
    let omitted_token_estimate = original_token_estimate
        .saturating_sub(head_token_estimate)
        .saturating_sub(tail_token_estimate);
    let marker = format!(
        "{TOOL_OUTPUT_PREVIEW_MARKER_PREFIX}该工具结果原始约 {original_token_estimate} token；保留头部约 {head_token_estimate} token 和尾部约 {tail_token_estimate} token；中间约 {omitted_token_estimate} token 已从本次模型上下文省略。]"
    );
    ToolOutputPreview {
        text: format!("{head}\n\n{marker}\n\n{tail}"),
        original_token_estimate,
        head_token_estimate,
        tail_token_estimate,
        omitted_token_estimate,
        previewed: true,
    }
}

/// Finds the longest Unicode-safe prefix whose mixed-language estimate fits one token budget.
/// 查找混合语言估算不超过指定 Token 预算的最长 Unicode 安全前缀。
///
/// # Parameters
/// # 参数
/// `text` is the immutable tool-result text and `token_budget` is the maximum estimated prefix size.
/// `text` 是不可变工具结果文本，`token_budget` 是头部允许的最大估算 Token 数。
///
/// # Returns
/// # 返回值
/// The byte index immediately after the accepted Unicode scalar boundary.
/// 返回已接受 Unicode 标量边界之后的字节索引。
fn prefix_end_for_token_budget(text: &str, token_budget: usize) -> usize {
    // acceptedEnd is the last character boundary whose incremental estimate fits the budget.
    // acceptedEnd 是增量估算仍满足预算的最后一个字符边界。
    let mut accepted_end = 0usize;
    // tokenEstimate accumulates the exact character heuristic in one forward pass.
    // tokenEstimate 通过一次正向扫描累计精确字符启发式结果。
    let mut token_estimate = 0usize;
    // asciiRunLength tracks the active ASCII run because every four ASCII characters share one token.
    // asciiRunLength 跟踪当前 ASCII 连续段，因为每四个 ASCII 字符共用一个 Token。
    let mut ascii_run_length = 0usize;
    for (index, character) in text.char_indices() {
        let candidate_end = index + character.len_utf8();
        let character_cost =
            mixed_language_incremental_token_cost(character, &mut ascii_run_length);
        if token_estimate.saturating_add(character_cost) > token_budget {
            break;
        }
        token_estimate = token_estimate.saturating_add(character_cost);
        accepted_end = candidate_end;
    }
    accepted_end
}

/// Finds the longest Unicode-safe suffix fitting one token budget without overlapping the prefix.
/// 查找不与头部重叠且不超过指定 Token 预算的最长 Unicode 安全后缀。
///
/// # Parameters
/// # 参数
/// `text` is the immutable tool-result text, `token_budget` bounds the suffix estimate,
/// and `minimum_start` prevents overlap with the retained prefix.
/// `text` 是不可变工具结果文本，`token_budget` 限制尾部估算大小，
/// `minimum_start` 用于防止尾部与已保留头部重叠。
///
/// # Returns
/// # 返回值
/// The Unicode-safe byte index where the accepted suffix begins.
/// 返回已接受尾部开始位置的 Unicode 安全字节索引。
fn suffix_start_for_token_budget(text: &str, token_budget: usize, minimum_start: usize) -> usize {
    // acceptedStart is the earliest character boundary whose reverse incremental estimate fits.
    // acceptedStart 是反向增量估算仍满足预算的最早字符边界。
    let mut accepted_start = text.len();
    // tokenEstimate accumulates the same run-length heuristic from the suffix end.
    // tokenEstimate 从后缀末尾累计相同的连续段启发式结果。
    let mut token_estimate = 0usize;
    // asciiRunLength tracks the active reverse ASCII run; run token counts are direction independent.
    // asciiRunLength 跟踪反向 ASCII 连续段；连续段 Token 数与扫描方向无关。
    let mut ascii_run_length = 0usize;
    for (index, character) in text.char_indices().rev() {
        if index < minimum_start {
            break;
        }
        let character_cost =
            mixed_language_incremental_token_cost(character, &mut ascii_run_length);
        if token_estimate.saturating_add(character_cost) > token_budget {
            break;
        }
        token_estimate = token_estimate.saturating_add(character_cost);
        accepted_start = index;
    }
    accepted_start
}

/// Returns one character's incremental cost under the shared mixed-language estimator.
/// 返回单个字符在共享混合语言估算器语义下的增量成本。
///
/// # Parameters
/// # 参数
/// `character` is the next character in one directional scan and `ascii_run_length` tracks its active ASCII run.
/// `character` 是单向扫描中的下一个字符，`ascii_run_length` 跟踪当前 ASCII 连续段。
///
/// # Returns
/// # 返回值
/// Returns the additional estimated tokens contributed by this character.
/// 返回该字符新增的估算 Token 数。
fn mixed_language_incremental_token_cost(character: char, ascii_run_length: &mut usize) -> usize {
    if character.is_ascii() {
        *ascii_run_length = ascii_run_length.saturating_add(1);
        return usize::from((*ascii_run_length - 1).is_multiple_of(4));
    }
    *ascii_run_length = 0;
    if is_cjk_character(character) { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    //! Historical tool-output preview tests.
    //! 历史工具输出预览测试。

    use super::*;

    /// Verifies representative Unicode and newline inputs remain valid and idempotent.
    /// 验证代表性的 Unicode 与换行输入保持有效且具备幂等性。
    #[test]
    fn preview_is_unicode_safe_and_idempotent() {
        let text = "开头🙂e\u{301}\r\n```rust\nlet value = 1;\n```\n".repeat(1_000) + "结尾🚀";
        let preview = preview_tool_output(&text);
        assert!(preview.previewed);
        assert!(preview.text.starts_with("开头🙂"));
        assert!(preview.text.ends_with("结尾🚀"));
        assert!(preview.text.contains(TOOL_OUTPUT_PREVIEW_MARKER_PREFIX));
        assert!(preview.head_token_estimate <= TOOL_OUTPUT_PREVIEW_HEAD_TOKENS);
        assert!(preview.tail_token_estimate <= TOOL_OUTPUT_PREVIEW_TAIL_TOKENS);

        // repeatedPreview proves second and third compaction attempts cannot nest omission markers.
        // repeatedPreview 证明第二次与第三次压缩尝试都不能嵌套省略标记。
        let mut repeated_preview = preview;
        for _ in 0..2 {
            // next is the next compaction attempt over an already previewed body.
            // next 是对已预览正文执行的下一次压缩尝试。
            let next = preview_tool_output(&repeated_preview.text);
            assert!(!next.previewed);
            assert_eq!(next.text, repeated_preview.text);
            repeated_preview = next;
        }
    }

    /// Verifies ASCII and CJK texts at or below the threshold remain unchanged.
    /// 验证阈值以内的 ASCII 与中文文本保持不变。
    #[test]
    fn preview_keeps_text_at_or_below_threshold() {
        for text in ["a".repeat(4_000), "中".repeat(2_000)] {
            assert!(estimate_mixed_language_text_tokens(&text) <= 2_500);
            let preview = preview_tool_output(&text);
            assert!(!preview.previewed);
            assert_eq!(preview.text, text);
        }
    }

    /// Verifies the strict threshold boundary for both ASCII and CJK estimates.
    /// 验证 ASCII 与中文估算的严格阈值边界。
    #[test]
    fn preview_starts_only_above_exact_threshold() {
        for (at_threshold, above_threshold) in [
            ("a".repeat(10_000), "a".repeat(10_001)),
            ("中".repeat(2_500), "中".repeat(2_501)),
        ] {
            assert_eq!(
                estimate_mixed_language_text_tokens(&at_threshold),
                TOOL_OUTPUT_PREVIEW_THRESHOLD_TOKENS
            );
            assert_eq!(
                estimate_mixed_language_text_tokens(&above_threshold),
                TOOL_OUTPUT_PREVIEW_THRESHOLD_TOKENS + 1
            );
            assert!(!preview_tool_output(&at_threshold).previewed);
            assert!(preview_tool_output(&above_threshold).previewed);
        }
    }

    /// Verifies long ASCII and CJK text both receive the same stable omission marker.
    /// 验证超长 ASCII 与中文文本都会获得同一稳定省略标记。
    #[test]
    fn preview_handles_long_ascii_and_cjk_text() {
        for text in ["x".repeat(20_000), "中文".repeat(5_000)] {
            let preview = preview_tool_output(&text);
            assert!(preview.previewed);
            assert!(preview.omitted_token_estimate > 0);
            assert_eq!(
                preview
                    .text
                    .matches(TOOL_OUTPUT_PREVIEW_MARKER_PREFIX)
                    .count(),
                1
            );
        }
    }

    /// Verifies retained head and tail never overlap and a unique middle sentinel is omitted exactly once.
    /// 验证保留头尾永不重叠，且唯一中间哨兵会被完整省略。
    #[test]
    fn preview_retained_ranges_do_not_overlap() {
        // text places a unique sentinel beyond both retained budgets with stable unique boundary anchors.
        // text 把唯一哨兵放在两个保留预算之外，并提供稳定且唯一的边界锚点。
        let text = format!(
            "HEAD-UNIQUE-{}MIDDLE-SENTINEL-OMIT-ONCE{}-TAIL-UNIQUE",
            "a".repeat(12_000),
            "b".repeat(12_000)
        );

        // preview exposes the retained anchors and omission marker for overlap assertions.
        // preview 暴露保留锚点与省略标记，供重叠断言使用。
        let preview = preview_tool_output(&text);

        assert!(preview.previewed);
        assert!(preview.text.starts_with("HEAD-UNIQUE-"));
        assert!(preview.text.ends_with("-TAIL-UNIQUE"));
        assert!(!preview.text.contains("MIDDLE-SENTINEL-OMIT-ONCE"));
        assert_eq!(
            preview
                .text
                .matches(TOOL_OUTPUT_PREVIEW_MARKER_PREFIX)
                .count(),
            1
        );
    }

    /// Verifies a million-character tool output is previewed by the linear scan without changing accounting.
    /// 验证百万字符工具输出通过线性扫描生成预览，且不改变统计语义。
    #[test]
    fn preview_handles_million_character_output() {
        let text = "abcd中文🙂".repeat(100_000);

        let preview = preview_tool_output(&text);

        assert!(preview.previewed);
        assert_eq!(
            preview.original_token_estimate,
            estimate_mixed_language_text_tokens(&text)
        );
        assert!(preview.head_token_estimate <= TOOL_OUTPUT_PREVIEW_HEAD_TOKENS);
        assert!(preview.tail_token_estimate <= TOOL_OUTPUT_PREVIEW_TAIL_TOKENS);
        assert_eq!(
            preview.omitted_token_estimate,
            preview
                .original_token_estimate
                .saturating_sub(preview.head_token_estimate)
                .saturating_sub(preview.tail_token_estimate)
        );
    }
}

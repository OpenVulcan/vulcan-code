//! Grapheme-boundary helpers adapted from Ferrite (MIT) for Unicode-safe navigation.
//! 改编自 Ferrite（MIT）的字素边界辅助工具，用于 Unicode 安全导航。

use unicode_segmentation::UnicodeSegmentation;

/// Returns the starting character column of the previous grapheme cluster.
/// 返回前一个字素簇起始处的字符列索引。
#[must_use]
pub fn prev_grapheme_boundary(line_text: &str, char_column: usize) -> usize {
    let mut previous_start = 0;
    let mut current_offset = 0;

    for grapheme in line_text.graphemes(true) {
        if current_offset >= char_column {
            return previous_start;
        }
        previous_start = current_offset;
        current_offset += grapheme.chars().count();
    }

    previous_start
}

/// Returns the ending character column of the current grapheme cluster.
/// 返回当前字素簇结束处的字符列索引。
#[must_use]
pub fn next_grapheme_boundary(line_text: &str, char_column: usize) -> usize {
    let mut current_offset = 0;

    for grapheme in line_text.graphemes(true) {
        let grapheme_end = current_offset + grapheme.chars().count();
        if char_column < grapheme_end {
            return grapheme_end;
        }
        current_offset = grapheme_end;
    }

    current_offset
}

/// Removes trailing CR and LF terminators before grapheme-aware cursor math.
/// 在执行字素级光标计算前移除末尾的回车与换行终止符。
#[must_use]
pub fn line_text_stripped(line_text: &str) -> &str {
    line_text.trim_end_matches(['\r', '\n'])
}

#[cfg(test)]
mod tests {
    //! Unicode navigation regression tests for vendored grapheme helpers.
    //! vendored 字素边界辅助工具的 Unicode 导航回归测试。

    use super::{line_text_stripped, next_grapheme_boundary, prev_grapheme_boundary};

    /// Verifies emoji ZWJ sequences behave as one grapheme when moving selection boundaries.
    /// 验证在移动选择边界时 Emoji ZWJ 序列会被视为单个字素。
    #[test]
    fn keeps_emoji_zwj_sequence_as_one_grapheme() {
        let family = "👨\u{200D}👩\u{200D}👧";

        assert_eq!(next_grapheme_boundary(family, 0), 5);
        assert_eq!(prev_grapheme_boundary(family, 5), 0);
    }

    /// Verifies line stripping removes only trailing newline terminators.
    /// 验证行裁剪只会移除结尾的换行终止符。
    #[test]
    fn strips_only_trailing_newlines() {
        assert_eq!(line_text_stripped("内容\r\n"), "内容");
        assert_eq!(line_text_stripped("内容"), "内容");
    }
}

//! Reusable code-oriented display helpers for diff rendering.
//! 用于 Diff 渲染的可复用代码显示辅助组件。

use crate::diff::{DiffHunk, DiffLineKind};
use std::path::Path;

/// Maximum number of characters rendered verbatim for one diff preview line before truncation kicks in.
/// 单条 diff 预览行在触发截断前允许原样渲染的最大字符数。
const MAX_DIFF_PREVIEW_LINE_CHARS: usize = 2_048;

/// Number of leading characters preserved when one diff preview line must be truncated.
/// 当单条 diff 预览行必须截断时保留的前导字符数量。
const DIFF_PREVIEW_HEAD_CHARS: usize = 1_024;

/// Number of trailing characters preserved when one diff preview line must be truncated.
/// 当单条 diff 预览行必须截断时保留的尾部字符数量。
const DIFF_PREVIEW_TAIL_CHARS: usize = 320;

/// Resolves a syntax token from the current file path for iced highlighter lookup.
/// 为 iced 高亮器解析当前文件路径对应的语法标记。
pub fn syntax_token(path: &str) -> String {
    let path_ref = Path::new(path);
    let extension = path_ref
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    if let Some(extension) = extension {
        let syntax = match extension.as_str() {
            "rs" => "rust",
            "md" => "markdown",
            "yml" => "yaml",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" => "typescript",
            "tsx" => "tsx",
            "jsx" => "jsx",
            "py" => "python",
            "sh" => "bash",
            "ps1" => "powershell",
            "rb" => "ruby",
            "kt" => "kotlin",
            "cs" => "csharp",
            "cc" | "cxx" | "cpp" | "hpp" | "hh" | "hxx" => "cpp",
            "h" => "c",
            "toml" => "toml",
            "json" => "json",
            "yaml" => "yaml",
            "html" => "html",
            "css" => "css",
            "sql" => "sql",
            "xml" => "xml",
            "go" => "go",
            "java" => "java",
            "php" => "php",
            "swift" => "swift",
            "lua" => "lua",
            _ => extension.as_str(),
        };

        return syntax.to_owned();
    }

    path_ref
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .map(|file_name| file_name.to_ascii_lowercase())
        .map(|file_name| match file_name.as_str() {
            "dockerfile" => String::from("dockerfile"),
            "makefile" => String::from("make"),
            _ => file_name,
        })
        .unwrap_or_else(|| String::from("txt"))
}

/// Wrapped display payload for one hunk-level diff editor.
/// 单个 hunk 级 diff 编辑器使用的折行展示载荷。
#[derive(Debug, Clone)]
pub struct WrappedDiffHunk {
    /// Cached visual text lines used by virtualized hunk slices.
    /// 供虚拟化 hunk 切片使用的已缓存可视文本列表。
    pub text_lines: Vec<String>,
    /// Visual lines generated from the original diff rows.
    /// 由原始 diff 行生成的可视行列表。
    pub visual_lines: Vec<WrappedDiffVisualLine>,
    /// Maximum visible character count found inside the cached hunk lines.
    /// 当前缓存 hunk 行中检测到的最大可视字符数量。
    pub max_line_columns: usize,
    /// Number of visual lines whose displayed text had to be truncated for safety.
    /// 因安全原因被截断展示文本的可视行数量。
    pub truncated_line_count: usize,
}

/// One visual line generated from a logical diff line.
/// 一个由逻辑 diff 行生成的可视行。
#[derive(Debug, Clone)]
pub struct WrappedDiffVisualLine {
    /// Semantic kind inherited from the original diff row.
    /// 继承自原始 diff 行的语义类型。
    pub kind: DiffLineKind,
    /// Old-side line number shown by the visual line.
    /// 由当前可视行显示的旧侧行号。
    pub old_number: Option<usize>,
    /// New-side line number shown by the visual line.
    /// 由当前可视行显示的新侧行号。
    pub new_number: Option<usize>,
    /// Zero-based anchor visual line index used by hover indicators.
    /// 供悬停指示器使用的从零开始锚点可视行索引。
    pub anchor_index: usize,
    /// Whether the visible text for this line was truncated before entering the editor widget.
    /// 该行可见文本在进入编辑器控件前是否已被截断。
    pub is_truncated: bool,
}

/// Builds the cached text content and visual-line metadata for one hunk-level diff editor.
/// 为 hunk 级 diff 编辑器构建缓存文本内容与可视行元数据。
pub fn wrap_diff_hunk(hunk: &DiffHunk, _wrap_columns: usize) -> WrappedDiffHunk {
    let mut visual_lines = Vec::new();
    let mut text_lines = Vec::new();
    let mut max_line_columns = 0usize;
    let mut truncated_line_count = 0usize;

    for line in &hunk.lines {
        let anchor_index = visual_lines.len();
        let (visible_text, is_truncated) = truncate_diff_preview_line(&line.content);
        max_line_columns = max_line_columns.max(visible_text.chars().count());
        if is_truncated {
            truncated_line_count += 1;
        }

        visual_lines.push(WrappedDiffVisualLine {
            kind: line.kind,
            old_number: line.old_number,
            new_number: line.new_number,
            anchor_index,
            is_truncated,
        });
        text_lines.push(visible_text);
    }

    WrappedDiffHunk {
        text_lines,
        visual_lines,
        max_line_columns,
        truncated_line_count,
    }
}

/// Truncates one pathological diff preview line into a stable head-tail summary string.
/// 将异常超长的单条 diff 预览行截断为稳定的首尾摘要字符串。
fn truncate_diff_preview_line(line: &str) -> (String, bool) {
    let char_count = line.chars().count();

    if char_count <= MAX_DIFF_PREVIEW_LINE_CHARS {
        return (line.to_owned(), false);
    }

    let head = line
        .chars()
        .take(DIFF_PREVIEW_HEAD_CHARS)
        .collect::<String>();
    let tail = line
        .chars()
        .rev()
        .take(DIFF_PREVIEW_TAIL_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    let omitted_count =
        char_count.saturating_sub(DIFF_PREVIEW_HEAD_CHARS + DIFF_PREVIEW_TAIL_CHARS);

    (
        format!("{head} … [truncated {omitted_count} chars] … {tail}"),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffHunk, DiffLine, DiffLineKind};

    /// Verifies normal diff preview lines stay untouched when they are comfortably below the safety threshold.
    /// 验证明显低于安全阈值的普通 diff 预览行会保持原样。
    #[test]
    fn short_diff_preview_line_remains_unchanged() {
        let source = "short line";

        let (display, truncated) = truncate_diff_preview_line(source);

        assert_eq!(display, source);
        assert!(!truncated);
    }

    /// Verifies pathological diff preview lines are shortened into a stable head-tail summary payload.
    /// 验证异常超长的 diff 预览行会被缩短为稳定的首尾摘要载荷。
    #[test]
    fn long_diff_preview_line_is_truncated_for_safe_rendering() {
        let source = "x".repeat(MAX_DIFF_PREVIEW_LINE_CHARS + 800);

        let (display, truncated) = truncate_diff_preview_line(&source);

        assert!(truncated);
        assert!(display.contains("[truncated "));
        assert!(display.len() < source.len());
    }

    /// Verifies hunk wrapping metadata reports how many lines were truncated before entering the editor widget.
    /// 验证 hunk 包装元数据会报告进入编辑器控件前被截断的行数。
    #[test]
    fn wrapped_diff_hunk_tracks_truncated_line_count() {
        let mut hunk = DiffHunk::new();
        hunk.lines
            .push(DiffLine::new(DiffLineKind::Context, Some(1), Some(1), "ok"));
        hunk.lines.push(DiffLine::new(
            DiffLineKind::Added,
            None,
            Some(2),
            "x".repeat(MAX_DIFF_PREVIEW_LINE_CHARS + 120),
        ));

        let wrapped = wrap_diff_hunk(&hunk, 0);

        assert_eq!(wrapped.truncated_line_count, 1);
        assert!(!wrapped.visual_lines[0].is_truncated);
        assert!(wrapped.visual_lines[1].is_truncated);
    }
}

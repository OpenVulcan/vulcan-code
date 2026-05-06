//! Reusable code-oriented display helpers for diff rendering.
//! 用于 Diff 渲染的可复用代码显示辅助组件。

use crate::diff::{DiffHunk, DiffLineKind};
use std::path::Path;

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
    /// Pre-wrapped text rendered by the read-only editor.
    /// 供只读编辑器渲染的预折行文本。
    pub text: String,
    /// Visual lines generated after wrapping the original diff rows.
    /// 对原始 diff 行折行后生成的可视行列表。
    pub visual_lines: Vec<WrappedDiffVisualLine>,
}

/// One visual line generated from a logical diff line after wrapping.
/// 一个由逻辑 diff 行折行后生成的可视行。
#[derive(Debug, Clone)]
pub struct WrappedDiffVisualLine {
    /// Semantic kind inherited from the original diff row.
    /// 继承自原始 diff 行的语义类型。
    pub kind: DiffLineKind,
    /// Old-side line number shown only on the first wrapped segment.
    /// 仅在首个折行片段上显示的旧侧行号。
    pub old_number: Option<usize>,
    /// New-side line number shown only on the first wrapped segment.
    /// 仅在首个折行片段上显示的新侧行号。
    pub new_number: Option<usize>,
    /// Zero-based anchor visual line index used by hover indicators.
    /// 供悬停指示器使用的从零开始锚点可视行索引。
    pub anchor_index: usize,
}

/// Builds the wrapped text content and visual-line metadata for one hunk-level diff editor.
/// 为 hunk 级 diff 编辑器构建折行文本内容与可视行元数据。
pub fn wrap_diff_hunk(hunk: &DiffHunk, wrap_columns: usize) -> WrappedDiffHunk {
    let mut visual_lines = Vec::new();
    let mut text_lines = Vec::new();

    for line in &hunk.lines {
        let wrapped_segments = wrap_text_line(&line.content, wrap_columns);
        let anchor_index = visual_lines.len();

        for (segment_index, segment) in wrapped_segments.into_iter().enumerate() {
            let is_first_segment = segment_index == 0;

            visual_lines.push(WrappedDiffVisualLine {
                kind: line.kind,
                old_number: is_first_segment.then_some(line.old_number).flatten(),
                new_number: is_first_segment.then_some(line.new_number).flatten(),
                anchor_index,
            });
            text_lines.push(segment);
        }
    }

    WrappedDiffHunk {
        text: text_lines.join("\n"),
        visual_lines,
    }
}

/// Wraps one logical code line into fixed-width visual segments using the configured column budget.
/// 使用配置好的列预算将单个逻辑代码行折分成固定宽度的可视片段。
fn wrap_text_line(line: &str, wrap_columns: usize) -> Vec<String> {
    let safe_columns = wrap_columns.max(1);

    if line.is_empty() {
        return vec![String::new()];
    }

    let characters = line.chars().collect::<Vec<_>>();
    let mut wrapped_segments = Vec::new();
    let mut start = 0usize;

    while start < characters.len() {
        let end = (start + safe_columns).min(characters.len());
        wrapped_segments.push(characters[start..end].iter().collect::<String>());
        start = end;
    }

    wrapped_segments
}

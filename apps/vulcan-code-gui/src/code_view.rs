//! Reusable code-oriented display helpers for diff rendering.
//! 用于 Diff 渲染的可复用代码显示辅助组件。

use crate::diff::DiffFile;
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

/// Builds the joined text content for one file-level diff editor.
/// 为文件级 diff 编辑器构建拼接后的文本内容。
pub fn diff_file_text(file: &DiffFile) -> String {
    file.hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

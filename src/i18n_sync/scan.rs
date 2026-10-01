// Collects the translation keys the TUI source actually uses.
// 收集 TUI 源码实际使用的文案键。
//
// Only literal `tr!("key" ...)` invocations count. Dynamic keys are deliberately unsupported so the
// synced catalog is always exactly the set of keys reachable from code. Line comments are ignored so
// documentation examples such as `tr!("key")` never become real keys.
// 只统计字面量 `tr!("key" ...)` 调用。刻意不支持动态键，使同步出的目录始终恰好是代码可达的键集合。
// 忽略行注释，避免 `tr!("key")` 之类的文档示例被当作真实键。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;

/// The `tr!` invocation pattern. `\b` keeps macros such as `include_str!(` from matching.
/// `tr!` 调用模式。`\b` 防止 `include_str!(` 等宏被误匹配。
fn key_pattern() -> Regex {
    Regex::new(r#"\btr!\(\s*"([^"\\]+)""#).expect("tr! key regex")
}

/// Scans every `.rs` file under `src_dir`, skipping `exclude` subtrees, for `tr!` keys.
/// 扫描 `src_dir` 下除 `exclude` 子树外的所有 `.rs` 文件中的 `tr!` 键。
pub fn used_keys(src_dir: &Path, exclude: &[PathBuf]) -> Result<BTreeSet<String>> {
    let pattern = key_pattern();
    let mut keys = BTreeSet::new();
    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let path = entry?.path();
            if exclude.iter().any(|skip| path.starts_with(skip)) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?;
                keys.extend(keys_in_source(&pattern, &source));
            }
        }
    }
    Ok(keys)
}

/// Returns the literal `tr!` keys in one source string, ignoring `//` comment lines.
/// 返回单个源码字符串中的字面量 `tr!` 键，忽略 `//` 注释行。
fn keys_in_source(pattern: &Regex, source: &str) -> Vec<String> {
    let code: String = source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    pattern
        .captures_iter(&code)
        .map(|caps| caps[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literal_keys_including_multiline_calls() {
        let source = "let a = tr!(\"ui.close\");\nlet b = tr!(\n    \"conversation.toolSequence.summary\",\n    count = 3\n);";
        assert_eq!(
            keys_in_source(&key_pattern(), source),
            vec!["ui.close", "conversation.toolSequence.summary"]
        );
    }

    #[test]
    fn ignores_comments_and_other_macros() {
        let source = "/// Use `tr!(\"key\")`.\nconst X: &str = include_str!(\"../a.json\");\nlet a = tr!(\"ui.close\");";
        assert_eq!(keys_in_source(&key_pattern(), source), vec!["ui.close"]);
    }
}

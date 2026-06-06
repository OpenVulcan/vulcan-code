//! Shared selector normalization helpers for session-scoped tool exposure.
//! 会话级工具可见性使用的共享选择器规范化辅助逻辑。

use std::collections::BTreeSet;

/// Normalizes one user-facing selector so ids, names, and labels can be matched consistently.
/// 规范化一个用户可见选择器，使标识、名称与标签可以稳定匹配。
pub fn normalize_tool_selector(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized = normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    (!normalized.is_empty()).then_some(normalized)
}

/// Builds a normalized selector set from user-facing selector strings.
/// 从用户可见选择器字符串构建规范化选择器集合。
pub fn normalized_tool_selector_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .filter_map(|value| normalize_tool_selector(value))
        .collect()
}

/// Returns whether any candidate string matches the normalized selector set.
/// 返回任一候选字符串是否命中规范化选择器集合。
pub fn tool_selector_matches(selectors: &BTreeSet<String>, candidates: &[&str]) -> bool {
    if selectors.is_empty() {
        return false;
    }
    candidates
        .iter()
        .filter_map(|candidate| normalize_tool_selector(candidate))
        .any(|candidate| selectors.contains(&candidate))
}

#[cfg(test)]
mod tests {
    use super::{normalized_tool_selector_set, tool_selector_matches};

    /// Verifies selector normalization lets equivalent tool ids and labels match.
    /// 验证选择器规范化可以匹配等价工具标识与标签。
    #[test]
    fn selector_matching_normalizes_separator_and_case_variants() {
        let selectors = normalized_tool_selector_set(&[
            String::from("Shell Command"),
            String::from("vulcan-file-read"),
        ]);

        assert!(tool_selector_matches(&selectors, &["shell_command"]));
        assert!(tool_selector_matches(&selectors, &["VULCAN_FILE_READ"]));
        assert!(!tool_selector_matches(&selectors, &["vmm_memory_write"]));
    }
}

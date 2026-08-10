//! Stable model-rule matchers with exact and longest-prefix semantics.
//! 具备精确与最长前缀语义的稳定模型规则匹配器。

use serde::{Deserialize, Serialize};

/// Identifies one rule matching strategy.
/// 标识一种规则匹配策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherKind {
    /// Matches one exact model identifier.
    /// 精确匹配单个模型标识。
    Exact,
    /// Matches every model identifier starting with the declared prefix.
    /// 匹配所有以声明前缀开头的模型标识。
    Prefix,
}

/// Declares one stable model-rule matcher.
/// 声明一个稳定的模型规则匹配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleMatcher {
    /// Matching strategy.
    /// 匹配策略。
    pub kind: MatcherKind,
    /// Exact model identifier or prefix value.
    /// 精确模型标识或前缀值。
    pub value: String,
}

impl RuleMatcher {
    /// Reports whether this matcher matches the given model identifier.
    /// 报告该匹配器是否匹配给定模型标识。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the exact discovered or declared model identifier.
    /// `model_id` 是精确的已发现或已声明模型标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the exact value equals the model id, or the prefix value is a
    /// non-empty textual prefix of the model id.
    /// 当精确值等于模型 ID，或前缀值是非空文本前缀时返回 `true`。
    pub fn matches(&self, model_id: &str) -> bool {
        match self.kind {
            MatcherKind::Exact => self.value == model_id,
            MatcherKind::Prefix => {
                !self.value.is_empty() && model_id.starts_with(self.value.as_str())
            }
        }
    }

    /// Returns the matcher's precedence rank: exact rules always win over prefixes.
    /// 返回匹配器的优先级排名：精确规则总是优先于前缀规则。
    pub fn precedence(&self) -> u8 {
        match self.kind {
            MatcherKind::Exact => 0,
            MatcherKind::Prefix => 1,
        }
    }
}

/// Reports whether two matchers target the same model set at the same precedence rank.
/// 报告两个匹配器是否指向同一模型集合并因此在同一优先级冲突。
///
/// # Parameters
/// 参数
/// `left` and `right` are the two candidate matchers from different rules.
/// `left` 与 `right` 是来自不同规则的两个候选匹配器。
///
/// # Returns
/// 返回值
/// `true` when both are exact with the same value, or both are prefixes with the same value.
/// A longer prefix always outranks a shorter one, so a containment pair never conflicts.
/// 当两者同为精确且值相同，或两者同为前缀且值相同时返回 `true`。
/// 更长前缀始终优先于更短前缀，因此包含关系对永不冲突。
pub fn matchers_conflict(left: &RuleMatcher, right: &RuleMatcher) -> bool {
    match (left.kind, right.kind) {
        (MatcherKind::Exact, MatcherKind::Exact) => left.value == right.value,
        (MatcherKind::Prefix, MatcherKind::Prefix) => left.value == right.value,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact and prefix matching semantics.
    /// 验证精确与前缀匹配语义。
    #[test]
    fn matcher_matches_exact_and_prefix() {
        let exact = RuleMatcher {
            kind: MatcherKind::Exact,
            value: String::from("deepseek-chat"),
        };
        assert!(exact.matches("deepseek-chat"));
        assert!(!exact.matches("deepseek-chat-2"));

        let prefix = RuleMatcher {
            kind: MatcherKind::Prefix,
            value: String::from("deepseek-v4"),
        };
        assert!(prefix.matches("deepseek-v4-flash"));
        assert!(prefix.matches("deepseek-v4-pro"));
        assert!(!prefix.matches("deepseek-chat"));
    }

    /// Verifies an empty prefix never matches.
    /// 验证空前缀永不匹配。
    #[test]
    fn empty_prefix_never_matches() {
        let empty = RuleMatcher {
            kind: MatcherKind::Prefix,
            value: String::new(),
        };
        assert!(!empty.matches("anything"));
    }

    /// Verifies exact wins over prefix and only same-rank matchers conflict.
    /// 验证精确优先于前缀，且仅同优先级匹配器冲突。
    #[test]
    fn conflict_detection_matches_only_same_rank() {
        let a = RuleMatcher {
            kind: MatcherKind::Exact,
            value: String::from("m1"),
        };
        let b = RuleMatcher {
            kind: MatcherKind::Exact,
            value: String::from("m1"),
        };
        assert!(matchers_conflict(&a, &b));

        let c = RuleMatcher {
            kind: MatcherKind::Prefix,
            value: String::from("deepseek-"),
        };
        let d = RuleMatcher {
            kind: MatcherKind::Prefix,
            value: String::from("deepseek-v4"),
        };
        // A longer prefix always outranks a shorter one; the pair never conflicts.
        // 更长前缀始终优先于更短前缀；该组合永不冲突。
        assert!(!matchers_conflict(&c, &d));

        let e = RuleMatcher {
            kind: MatcherKind::Prefix,
            value: String::from("deepseek-v4"),
        };
        assert!(matchers_conflict(&d, &e));

        assert!(!matchers_conflict(&a, &c));
        assert_eq!(a.precedence(), 0);
        assert_eq!(c.precedence(), 1);
    }
}

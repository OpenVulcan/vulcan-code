//! System-level judge choices shared by persisted settings and the service API.
//! 持久化设置与服务 API 共享的系统级判定选择。

use serde::{Deserialize, Serialize};

/// Where calls are judged. A saved backend is what makes the judge configured, and so what makes
/// auto-approve mode available.
/// 判定调用的位置。已保存的后端即表示判定器已配置，自动审批模式也因此可用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeBackend {
    /// The active registered model in a local worker process.
    /// 在本机 worker 进程中运行的使用中登记模型。
    Local,
    /// The decision route saved for the permission judge, such as jev on TypeSafe or OpenRouter.
    /// 为权限判定保存的判定路由，例如 TypeSafe 或 OpenRouter 上的 jev。
    Cloud,
}

/// Most characters of one review rule text; the settings services refuse longer text.
/// 单条复核规则文本的最大字符数；设置服务拒绝更长的文本。
pub const MAX_REVIEW_RULE_CHARS: usize = 4_000;

/// Longest wait a permission approval may be given before it is denied, in seconds.
/// 权限审批在被拒绝前可设定的最长等待时间（秒）。
pub const MAX_APPROVAL_TIMEOUT_SECONDS: u32 = 3_600;

/// The user's natural-language rules for the permission reviewer, by the verdict they call for.
/// Empty text means no rule; precedence is deny, then ask, then allow.
/// 用户为权限复核器编写的自然语言规则，按其要求的结论分组。空文本表示没有规则；优先级依次为拒绝、
/// 询问、允许。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRules {
    /// Calls the reviewer may allow without asking the user.
    /// 复核器可以不询问用户而放行的调用。
    pub allow: String,
    /// Calls the reviewer hands to the user; denied when nobody can be asked.
    /// 复核器交给用户决定的调用；无人可询问时拒绝。
    pub ask: String,
    /// Calls the reviewer refuses without asking anyone.
    /// 复核器不询问任何人即拒绝的调用。
    pub deny: String,
}

impl ReviewRules {
    /// Returns whether no rule is written.
    /// 返回是否没有编写任何规则。
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.ask.is_empty() && self.deny.is_empty()
    }

    /// The rules with surrounding whitespace removed, or why one is too long.
    /// 去除首尾空白后的规则；或某条规则过长的原因。
    pub fn normalized(&self) -> Result<Self, String> {
        let mut rules = Self::default();
        for (name, source, target) in [
            ("allow", &self.allow, &mut rules.allow),
            ("ask", &self.ask, &mut rules.ask),
            ("deny", &self.deny, &mut rules.deny),
        ] {
            let text = source.trim();
            if text.chars().count() > MAX_REVIEW_RULE_CHARS {
                return Err(format!(
                    "The {name} rules are longer than {MAX_REVIEW_RULE_CHARS} characters."
                ));
            }
            *target = text.to_string();
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_trim_and_refuse_overlong_text() {
        let rules = ReviewRules {
            allow: String::from("  reading logs  "),
            ask: String::new(),
            deny: "x".repeat(MAX_REVIEW_RULE_CHARS + 1),
        };
        assert!(rules.normalized().is_err());
        let rules = ReviewRules {
            deny: String::from("\n"),
            ..rules
        }
        .normalized()
        .unwrap();
        assert_eq!(rules.allow, "reading logs");
        assert!(rules.deny.is_empty());
        assert_eq!(
            serde_json::to_value(&rules).unwrap(),
            serde_json::json!({"allow": "reading logs", "ask": "", "deny": ""})
        );
    }
}

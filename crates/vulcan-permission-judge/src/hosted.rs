//! Hosted judges: the policy the cloud and API judges read, and the state the API judge receives
//! and the rule that labels its answer.
//! 托管判定：云端与 API 判定读取的策略，以及 API 判定接收的状态与为其回答定标签的规则。
//!
//! Neither hosted judge has a local package, so this build ships the policy they read. The version
//! equals the local package policy, so project choices keep their meaning when the backend changes.
//! The API judge is a decision model the user reaches through their own provider route.
//! 两种托管判定都没有本机模型包，因此本构建随附其读取的策略。该版本与本机模型包策略一致，使项目选择在
//! 切换后端时保持原意。API 判定是用户经自己的供应商路由访问的判定模型。

use std::sync::LazyLock;

use serde_json::{Map, Value};

use crate::{JudgeLabel, JudgePolicy};

/// Smallest `allow` probability that lets a call through; any lower allow becomes review.
/// 放行调用所需的最小 `allow` 概率；低于该值的 allow 改为 review。
///
/// This is an uncalibrated starting value: no labelled set has measured the API model's
/// probabilities against the local judge yet.
/// 这是未经校准的初始值：尚无标注集把 API 模型的概率与本机判定器对照测量。
pub const API_ALLOW_PROBABILITY: f64 = 0.8;

/// Parsed and checked hosted policy, built once from the embedded resource.
/// 从内嵌资源构建一次的已解析并已检查托管策略。
static HOSTED_POLICY: LazyLock<JudgePolicy> = LazyLock::new(|| {
    parse_hosted_policy(include_str!("../resources/hosted-policy.json"))
        .expect("embedded hosted-policy.json must be valid")
});

/// Parses a policy and applies the checks the decision rule relies on.
/// 解析策略并执行判定规则所依赖的检查。
fn parse_hosted_policy(text: &str) -> Result<JudgePolicy, String> {
    let policy: JudgePolicy = serde_json::from_str(text).map_err(|error| error.to_string())?;
    policy.validate()?;
    Ok(policy)
}

/// The policy the cloud and API judges read.
/// 云端与 API 判定读取的策略。
pub fn hosted_policy() -> &'static JudgePolicy {
    &HOSTED_POLICY
}

/// The state the API judge reads: the project root when known and the call, whose argument text
/// is passed unchanged so key order and spelling match what would run.
/// API 判定读取的状态：已知时的项目根目录与调用；参数文本原样传入，使键顺序与写法与将要执行的内容一致。
pub fn api_state(project_root: Option<&str>, tool: &str, raw_arguments: &str) -> Value {
    let mut call = Map::new();
    call.insert(String::from("tool"), Value::from(tool));
    call.insert(String::from("args"), Value::from(raw_arguments));
    let mut state = Map::new();
    if let Some(root) = project_root {
        state.insert(String::from("project_root"), Value::from(root));
    }
    state.insert(String::from("call"), Value::Object(call));
    Value::Object(state)
}

/// Labels an API answer: `allow` stands only with at least [`API_ALLOW_PROBABILITY`], otherwise
/// it becomes review; an unknown choice or a non-finite probability is an error.
/// 为 API 回答定标签：`allow` 仅在概率至少为 [`API_ALLOW_PROBABILITY`] 时成立，否则改为 review；
/// 未知选项或非有限概率是错误。
pub fn api_label(choice: &str, allow_probability: f64) -> Result<JudgeLabel, String> {
    if !allow_probability.is_finite() {
        return Err(String::from(
            "The API judge returned a non-finite allow probability.",
        ));
    }
    let label = JudgeLabel::ORDER
        .into_iter()
        .find(|label| label.key() == choice)
        .ok_or_else(|| format!("The API judge chose an unknown label `{choice}`."))?;
    Ok(match label {
        JudgeLabel::Allow if allow_probability < API_ALLOW_PROBABILITY => JudgeLabel::Review,
        label => label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_policy_parses_with_the_label_order() {
        let policy = hosted_policy();
        assert_eq!(
            policy
                .criteria
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            JudgeLabel::ORDER.map(JudgeLabel::key)
        );
        assert!(policy.resolve_settings(&Default::default()).is_ok());
    }

    #[test]
    fn allow_needs_the_threshold_and_unknown_choices_fail() {
        assert_eq!(
            api_label("allow", API_ALLOW_PROBABILITY),
            Ok(JudgeLabel::Allow)
        );
        assert_eq!(
            api_label("allow", API_ALLOW_PROBABILITY / 2.0),
            Ok(JudgeLabel::Review)
        );
        assert_eq!(api_label("review", 1.0), Ok(JudgeLabel::Review));
        assert_eq!(api_label("block", 0.0), Ok(JudgeLabel::Block));
        assert!(api_label("maybe", 1.0).is_err());
        assert!(api_label("allow", f64::NAN).is_err());
    }

    #[test]
    fn state_keeps_the_argument_text_and_omits_an_unknown_root() {
        let raw = r#"{"b":1,"a":2}"#;
        let state = api_state(None, "shell_command", raw);
        assert!(state.get("project_root").is_none());
        assert_eq!(state["call"]["args"], raw);
        assert_eq!(
            api_state(Some("/w/demo"), "shell_command", raw)["project_root"],
            "/w/demo"
        );
    }
}

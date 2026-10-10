//! Judge policy text: shipped inside a model package, or with this build for the cloud judge.
//! 判定策略文本：随模型包发布，或为云端判定随本构建发布。
//!
//! The policy must match the weights it was trained with, so the registered package is the only
//! source for the local model; the cloud judge, which has no package, reads the copy in `cloud`.
//! Options are fixed enumerations; neither the agent nor tool arguments can add policy text.
//! 策略必须与训练所用权重一致，因此已登记的模型包是本机模型的唯一来源；没有模型包的云端判定读取
//! `cloud` 中的副本。选项是固定枚举；代理与工具参数都不能添加策略文本。

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::decision::JudgeLabel;

/// Parsed policy; maps keep the order the model was trained with.
/// 已解析的策略；映射保持模型训练时的顺序。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JudgePolicy {
    /// Policy revision identifier.
    /// 策略修订标识。
    pub version: String,
    /// Fixed opening instructions.
    /// 固定的开头指令。
    pub core_instructions: String,
    /// Criterion text per label, in logit order.
    /// 每个标签的判据文本，按分数顺序排列。
    pub criteria: IndexMap<String, String>,
    /// Authorization options, in prompt order.
    /// 授权选项，按提示词顺序排列。
    pub options: IndexMap<String, PolicyOption>,
}

/// One fixed authorization option and its selectable modes.
/// 一个固定授权选项及其可选档位。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PolicyOption {
    /// Mode used when a project has not chosen one.
    /// 项目未选择时使用的档位。
    pub default: String,
    /// Fixed English sentence per mode.
    /// 每个档位的固定英文句子。
    pub modes: IndexMap<String, String>,
}

/// A complete mode choice for every option of one policy, in policy order.
/// 针对某个策略每个选项的完整档位选择，按策略顺序排列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeSettings(IndexMap<String, String>);

impl JudgeSettings {
    /// Chosen mode per option, in policy order.
    /// 每个选项的已选档位，按策略顺序排列。
    pub fn modes(&self) -> &IndexMap<String, String> {
        &self.0
    }

    /// The settings as a plain map for the wire protocol.
    /// 线协议使用的普通映射形式。
    pub fn to_map(&self) -> BTreeMap<String, String> {
        self.0
            .iter()
            .map(|(key, mode)| (key.clone(), mode.clone()))
            .collect()
    }
}

/// A settings map that does not fit the policy.
/// 与策略不匹配的设置映射。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    /// A mode that the option does not offer.
    /// 选项未提供的档位。
    #[error("Option `{option}` has no mode `{mode}`.")]
    UnknownMode {
        /// Option key.
        /// 选项键。
        option: String,
        /// Rejected mode.
        /// 被拒绝的档位。
        mode: String,
    },
    /// An exact settings map lacks an option.
    /// 精确设置映射缺少某个选项。
    #[error("Option `{0}` has no chosen mode.")]
    MissingOption(String),
    /// An exact settings map names an option the policy does not have.
    /// 精确设置映射包含策略中不存在的选项。
    #[error("Option `{0}` is not part of this policy.")]
    UnknownOption(String),
}

impl JudgePolicy {
    /// Checks the internal consistency the decision rule relies on.
    /// 检查判定规则所依赖的内部一致性。
    pub fn validate(&self) -> Result<(), String> {
        let labels: Vec<&str> = JudgeLabel::ORDER.iter().map(|label| label.key()).collect();
        let criteria: Vec<&str> = self.criteria.keys().map(String::as_str).collect();
        if criteria != labels {
            return Err(format!(
                "Policy criteria must be exactly {labels:?} in this order, found {criteria:?}."
            ));
        }
        for (key, option) in &self.options {
            if option.modes.is_empty() {
                return Err(format!("Policy option `{key}` offers no modes."));
            }
            if !option.modes.contains_key(&option.default) {
                return Err(format!(
                    "Policy option `{key}` defaults to `{}`, which it does not offer.",
                    option.default
                ));
            }
        }
        Ok(())
    }

    /// Resolves a project's choices: unchosen options take their default, choices for options this
    /// policy no longer has are ignored, and an unknown mode is a configuration error.
    /// 解析项目的选择：未选择的选项取默认值，策略已不存在的选项被忽略，未知档位是配置错误。
    pub fn resolve_settings(
        &self,
        chosen: &BTreeMap<String, String>,
    ) -> Result<JudgeSettings, SettingsError> {
        let mut modes = IndexMap::with_capacity(self.options.len());
        for (key, option) in &self.options {
            let mode = chosen.get(key).unwrap_or(&option.default);
            if !option.modes.contains_key(mode) {
                return Err(SettingsError::UnknownMode {
                    option: key.clone(),
                    mode: mode.clone(),
                });
            }
            modes.insert(key.clone(), mode.clone());
        }
        Ok(JudgeSettings(modes))
    }

    /// Accepts a settings map only when it names every option exactly once with a known mode.
    /// 只接受为每个选项恰好给出一个已知档位的设置映射。
    pub fn exact_settings(
        &self,
        given: &BTreeMap<String, String>,
    ) -> Result<JudgeSettings, SettingsError> {
        if let Some(unknown) = given.keys().find(|key| !self.options.contains_key(*key)) {
            return Err(SettingsError::UnknownOption(unknown.clone()));
        }
        if let Some(missing) = self.options.keys().find(|key| !given.contains_key(*key)) {
            return Err(SettingsError::MissingOption(missing.clone()));
        }
        self.resolve_settings(given)
    }

    /// Renders the instructions: the core text followed by each chosen mode sentence.
    /// 渲染指令：核心文本后接每个已选档位的句子。
    pub fn instructions(&self, settings: &JudgeSettings) -> String {
        let mut fragments = vec![self.core_instructions.as_str()];
        for (key, option) in &self.options {
            if let Some(sentence) = settings.0.get(key).and_then(|mode| option.modes.get(mode)) {
                fragments.push(sentence);
            }
        }
        fragments.join(" ")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A two-option policy whose criteria are not in alphabetical order.
    /// 判据不按字母顺序排列的双选项策略。
    pub(crate) const POLICY: &str = r#"{
        "version": "test-v1",
        "core_instructions": "Core.",
        "criteria": {"allow": "A.", "review": "R.", "block": "B."},
        "options": {
            "writes": {"default": "allow", "modes": {"allow": "W allow.", "review": "W review."}},
            "reset": {"default": "confirm", "modes": {"confirm": "R confirm.", "allow": "R allow."}}
        }
    }"#;

    fn policy() -> JudgePolicy {
        serde_json::from_str(POLICY).unwrap()
    }

    #[test]
    fn instructions_follow_policy_order_and_chosen_modes() {
        let policy = policy();
        policy.validate().unwrap();
        let chosen = BTreeMap::from([
            ("reset".to_string(), "allow".to_string()),
            ("retired".to_string(), "anything".to_string()),
        ]);
        let settings = policy.resolve_settings(&chosen).unwrap();
        assert_eq!(
            settings.modes().keys().collect::<Vec<_>>(),
            ["writes", "reset"]
        );
        assert_eq!(policy.instructions(&settings), "Core. W allow. R allow.");
    }

    #[test]
    fn unknown_modes_and_inexact_maps_are_rejected() {
        let policy = policy();
        let bad = BTreeMap::from([("writes".to_string(), "deny".to_string())]);
        assert!(matches!(
            policy.resolve_settings(&bad),
            Err(SettingsError::UnknownMode { .. })
        ));
        let partial = BTreeMap::from([("writes".to_string(), "allow".to_string())]);
        assert_eq!(
            policy.exact_settings(&partial),
            Err(SettingsError::MissingOption("reset".to_string()))
        );
        let extra = BTreeMap::from([
            ("writes".to_string(), "allow".to_string()),
            ("reset".to_string(), "confirm".to_string()),
            ("other".to_string(), "x".to_string()),
        ]);
        assert_eq!(
            policy.exact_settings(&extra),
            Err(SettingsError::UnknownOption("other".to_string()))
        );
    }

    #[test]
    fn criteria_must_match_the_classifier_order() {
        let mut policy = policy();
        policy.criteria.swap_indices(1, 2);
        assert!(policy.validate().is_err());
    }
}

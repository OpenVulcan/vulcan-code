use super::*;

/// A complete session override of the system subagent model policy.
/// 系统子代理模型策略的完整会话覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentModelPolicy {
    /// Whether the calling session's model is additionally granted.
    /// 是否额外授权调用会话模型。
    pub include_session_model: bool,
    /// Explicit model grants in display order.
    /// 按显示顺序排列的显式模型授权。
    pub models: Vec<SubagentModelGrant>,
}

impl ProviderConfigurationFile {
    /// Resolves one complete session override or the system policy without mixing their fields.
    /// 解析完整会话覆盖或系统策略，不混合两个层级的字段。
    /// `session` is absent when following the system; returns inclusion and the borrowed grants.
    /// `session` 在跟随系统时为空；返回是否包含会话模型及借用的授权清单。
    pub fn subagent_model_policy<'a>(
        &'a self,
        session: Option<&'a SubagentModelPolicy>,
    ) -> (bool, &'a [SubagentModelGrant]) {
        match session {
            Some(policy) => (policy.include_session_model, &policy.models),
            None => (!self.subagent_exclude_session_model, &self.subagent_models),
        }
    }
}

/// Maximum characters of one user note attached to a subagent model grant.
/// 子代理模型授权所附用户备注的最大字符数。
pub const MAX_SUBAGENT_MODEL_NOTE_CHARS: usize = 400;

/// Maximum number of models one subagent model allowlist may grant.
/// 一份子代理模型白名单可授权的最大模型数。
pub const MAX_SUBAGENT_MODEL_GRANTS: usize = 64;

/// One model that subagents may run in addition to the session's own model.
/// 子代理在会话自身模型之外可以使用的一个模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentModelGrant {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    pub provider_id: String,
    /// Provider-scoped model identifier, which may itself contain `/`.
    /// Provider 范围内的模型标识，本身可能含有 `/`。
    pub model_id: String,
    /// User guidance shown to the agent: which tasks suit this model and what to watch for.
    /// 展示给代理的用户说明：适合哪些任务以及使用注意事项。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl SubagentModelGrant {
    /// Returns the `provider/model` selector that `spawn_agent.model` accepts.
    /// 返回 `spawn_agent.model` 接受的 `provider/model` 选择器。
    pub fn selector(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

/// Validates and normalizes one allowlist before it is saved.
/// 在保存前校验并规范化一份白名单。
///
/// # Parameters
/// 参数
/// `grants` is the user's list in display order.
/// `grants` 是按显示顺序排列的用户清单。
///
/// # Returns
/// 返回值
/// The list with trimmed identifiers and notes, or an English error naming the first problem:
/// an empty identifier, a duplicate model, a note over [`MAX_SUBAGENT_MODEL_NOTE_CHARS`], or more
/// than [`MAX_SUBAGENT_MODEL_GRANTS`] entries.
/// 去除首尾空白后的清单；或返回指出首个问题的英文错误：标识为空、模型重复、备注超过
/// [`MAX_SUBAGENT_MODEL_NOTE_CHARS`]，或条目超过 [`MAX_SUBAGENT_MODEL_GRANTS`]。
pub fn normalize_subagent_model_grants(
    grants: Vec<SubagentModelGrant>,
) -> Result<Vec<SubagentModelGrant>, String> {
    if grants.len() > MAX_SUBAGENT_MODEL_GRANTS {
        return Err(format!(
            "A subagent model allowlist holds at most {MAX_SUBAGENT_MODEL_GRANTS} models."
        ));
    }
    let mut normalized = Vec::with_capacity(grants.len());
    for grant in grants {
        let grant = SubagentModelGrant {
            provider_id: grant.provider_id.trim().to_string(),
            model_id: grant.model_id.trim().to_string(),
            note: grant.note.trim().to_string(),
        };
        if grant.provider_id.is_empty() || grant.model_id.is_empty() {
            return Err(String::from(
                "A subagent model grant needs both a provider and a model.",
            ));
        }
        if grant.provider_id.contains('/') {
            return Err(format!(
                "Provider identifier `{}` cannot contain `/`.",
                grant.provider_id
            ));
        }
        if grant.note.chars().count() > MAX_SUBAGENT_MODEL_NOTE_CHARS {
            return Err(format!(
                "The note for `{}` exceeds {MAX_SUBAGENT_MODEL_NOTE_CHARS} characters.",
                grant.selector()
            ));
        }
        if normalized
            .iter()
            .any(|existing: &SubagentModelGrant| existing.selector() == grant.selector())
        {
            return Err(format!(
                "Model `{}` appears more than once in the subagent model allowlist.",
                grant.selector()
            ));
        }
        normalized.push(grant);
    }
    Ok(normalized)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    /// Default and persisted policies preserve the checkbox without rewriting missing configuration.
    /// 默认及持久策略保留复选框语义，不改写缺失配置。
    #[test]
    fn subagent_policy_defaults_and_disabled_roundtrip() {
        // Both construction and deserialization must keep the historically enabled default.
        // 构造及反序列化都必须保留历史默认开启行为。
        let default = ProviderConfigurationFile::default();
        assert!(default.subagent_model_policy(None).0);
        assert!(
            serde_json::to_value(&default)
                .unwrap()
                .get("subagentExcludeSessionModel")
                .is_none()
        );
        // Explicit false in the UI is represented as exclusion in persisted configuration.
        // 界面显式关闭在持久配置中表示为排除。
        let disabled = ProviderConfigurationFile {
            subagent_exclude_session_model: true,
            ..default
        };
        // Reload the real complete configuration format, not a test-specific settings shape.
        // 重新加载真实完整配置格式，而非测试专用设置形状。
        let reloaded: ProviderConfigurationFile =
            serde_json::from_value(serde_json::to_value(disabled).unwrap()).unwrap();
        assert!(!reloaded.subagent_model_policy(None).0);
        assert!(reloaded.subagent_model_policy(None).1.is_empty());
        // A complete session override can re-enable inheritance without any explicit grants.
        // 完整会话覆盖可以在没有显式授权时重新开启继承。
        let session = SubagentModelPolicy {
            include_session_model: true,
            models: Vec::new(),
        };
        assert!(reloaded.subagent_model_policy(Some(&session)).0);
    }
}

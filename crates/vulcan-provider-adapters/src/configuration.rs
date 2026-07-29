//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{
    PersistedConnectionGraph, ProviderDescriptor, ProviderToolCapabilityKind,
    ProviderToolTargetSelection, SecondaryRouteSelection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{ModelParameterValue, PromptCachePolicy};

/// Lists the reviewed OpenRouter models enabled by default.
/// 列出默认启用的已审核 OpenRouter 模型。
const OPENROUTER_FEATURED_MODELS: &[&str] = &[
    "openai/gpt-5.6-sol",
    "openai/gpt-5.6-terra",
    "anthropic/claude-sonnet-5",
    "anthropic/claude-opus-4.8",
    "google/gemini-3.1-pro-preview",
    "google/gemini-3.5-flash",
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
    "qwen/qwen3.6-plus",
    "z-ai/glm-5.2",
    "minimax/minimax-m3",
    "openai/text-embedding-3-small",
    "cohere/rerank-v3.5",
    "openai/gpt-image-1",
    "google/veo-3.1",
    "minimax/speech-2.8-turbo",
    "openai/whisper-large-v3-turbo",
];

/// Identifies the exact model and account selected by the user.
/// 标识用户选择的精确模型与账号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveAiModelSelection {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Explicit credential connection identifier.
    /// 显式凭据连接标识。
    pub connection_id: String,
}

/// Stores local narrowing and parameter choices for one connection-model pair.
/// 保存一个连接与模型组合的本地收窄及参数选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelExecutionOverrides {
    /// Optional local context-window narrowing.
    /// 可选本地上下文窗口收窄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional local output-token narrowing.
    /// 可选本地输出 Token 收窄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
    /// Optional model-level prompt-cache override.
    /// 可选模型级提示词缓存覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache: Option<PromptCachePolicy>,
    /// Adapter-registered model-specific values.
    /// 适配器注册的模型独立参数值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameters: BTreeMap<String, ModelParameterValue>,
}

/// Stores all new AI configuration while excluding runtime-discovered model catalogs.
/// 保存全部新 AI 配置，同时排除运行时发现的模型目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedAiConfiguration {
    /// Background account-usage refresh interval in seconds.
    /// 后台账号用量刷新间隔秒数。
    #[serde(default = "default_usage_refresh_interval_seconds")]
    pub usage_refresh_interval_seconds: u64,
    /// User-owned compatibility provider definitions without credential secrets or model catalogs.
    /// 不包含凭据秘密或模型目录的用户自有兼容供应商定义。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_providers: BTreeMap<String, ProviderDescriptor>,
    /// Exact active model and account selection.
    /// 精确的当前模型与账号选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<ActiveAiModelSelection>,
    /// Provider connections and account preferences.
    /// 供应商连接与账号偏好。
    #[serde(default)]
    pub connection_graph: PersistedConnectionGraph,
    /// Per-provider, model, and protocol overrides keyed by a collision-resistant composite key.
    /// 按抗冲突组合键索引的供应商、模型与协议覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_overrides: BTreeMap<String, ModelExecutionOverrides>,
    /// User model-access choices that differ from the provider default policy.
    /// 与供应商默认策略不同的用户模型启用选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_enabled_overrides: BTreeMap<String, bool>,
    /// Per-connection model secondary routes keyed by collision-resistant composite key.
    /// 按抗冲突组合键索引的连接模型二级路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secondary_routes: BTreeMap<String, SecondaryRouteSelection>,
    /// One explicit provider tool target per semantic capability.
    /// 每种语义能力唯一显式选择的供应商工具目标。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_tool_selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Explicitly disabled provider tool targets grouped by semantic capability.
    /// 按语义能力分组的显式禁用供应商工具目标。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_tool_disabled_targets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
}

/// Returns the default five-minute account-usage refresh interval.
/// 返回默认五分钟账号用量刷新间隔。
fn default_usage_refresh_interval_seconds() -> u64 {
    300
}

impl Default for PersistedAiConfiguration {
    /// Builds one configuration with no providers and a five-minute usage refresh interval.
    /// 构建一个不含供应商且用量刷新间隔为五分钟的配置。
    fn default() -> Self {
        Self {
            usage_refresh_interval_seconds: default_usage_refresh_interval_seconds(),
            custom_providers: BTreeMap::new(),
            active: None,
            connection_graph: PersistedConnectionGraph::default(),
            model_overrides: BTreeMap::new(),
            model_enabled_overrides: BTreeMap::new(),
            secondary_routes: BTreeMap::new(),
            provider_tool_selections: BTreeMap::new(),
            provider_tool_disabled_targets: BTreeMap::new(),
        }
    }
}

impl PersistedAiConfiguration {
    /// Builds a stable key for one provider-model-protocol scoped setting.
    /// 为一个供应商、模型与协议作用域设置构建稳定键。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `protocol_id` are exact registered identifiers.
    /// `provider_id`、`model_id` 与 `protocol_id` 是精确注册标识。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant length-prefixed key shared by every matching account.
    /// 返回由全部匹配账号共享的抗冲突长度前缀键。
    pub fn provider_model_protocol_key(
        provider_id: &str,
        model_id: &str,
        protocol_id: &str,
    ) -> String {
        format!(
            "{}:{provider_id}:{}:{model_id}:{}:{protocol_id}",
            provider_id.len(),
            model_id.len(),
            protocol_id.len()
        )
    }

    /// Builds a stable key for one connection-model scoped setting.
    /// 为一个连接模型作用域设置构建稳定键。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` are exact identifiers.
    /// `connection_id` 与 `model_id` 是精确标识。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant length-prefixed key.
    /// 返回抗冲突的长度前缀键。
    pub fn connection_model_key(connection_id: &str, model_id: &str) -> String {
        format!(
            "{}:{connection_id}:{}:{model_id}",
            connection_id.len(),
            model_id.len()
        )
    }

    /// Builds a stable key for one provider-model access choice.
    /// 为一个供应商模型启用选择构建稳定键。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` are exact registered identifiers.
    /// `provider_id` 与 `model_id` 是精确注册标识。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant provider-model key.
    /// 返回抗冲突的供应商模型键。
    pub fn provider_model_key(provider_id: &str, model_id: &str) -> String {
        format!(
            "{}:{provider_id}:{}:{model_id}",
            provider_id.len(),
            model_id.len()
        )
    }

    /// Reports whether one model is enabled after applying the persisted user override.
    /// 应用持久化用户覆盖后报告一个模型是否启用。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the exact runtime model.
    /// `provider_id` 与 `model_id` 标识精确运行时模型。
    ///
    /// # Returns
    /// 返回值
    /// The effective enabled state.
    /// 返回有效启用状态。
    pub fn model_is_enabled(&self, provider_id: &str, model_id: &str) -> bool {
        self.model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
            .unwrap_or_else(|| default_model_enabled(provider_id, model_id))
    }

    /// Stores only a model enabled choice that differs from the provider default.
    /// 仅保存与供应商默认值不同的模型启用选择。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `enabled` identify the exact user choice.
    /// `provider_id`、`model_id` 与 `enabled` 标识精确用户选择。
    pub fn set_model_enabled(&mut self, provider_id: &str, model_id: &str, enabled: bool) {
        let key = Self::provider_model_key(provider_id, model_id);
        if enabled == default_model_enabled(provider_id, model_id) {
            self.model_enabled_overrides.remove(key.as_str());
        } else {
            self.model_enabled_overrides.insert(key, enabled);
        }
    }
}

/// Returns the code-owned default model access policy.
/// 返回代码拥有的默认模型访问策略。
///
/// # Parameters
/// 参数
/// `provider_id` and `model_id` identify the exact runtime model.
/// `provider_id` 与 `model_id` 标识精确运行时模型。
///
/// # Returns
/// 返回值
/// OpenRouter is enabled only for reviewed models; every other provider defaults to enabled.
/// OpenRouter 仅启用已审核模型；其他供应商默认全部启用。
pub fn default_model_enabled(provider_id: &str, model_id: &str) -> bool {
    provider_id != "openrouter" || OPENROUTER_FEATURED_MODELS.contains(&model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies runtime model catalogs cannot appear in persisted AI JSON.
    /// 验证运行时模型目录不会出现在持久化 AI JSON 中。
    #[test]
    fn persisted_ai_configuration_has_no_model_catalog() {
        let encoded =
            serde_json::to_string(&PersistedAiConfiguration::default()).expect("must serialize");
        assert!(!encoded.contains("catalog"));
        assert!(!encoded.contains("models"));
    }

    /// Verifies model overrides are independent from credential connection identifiers.
    /// 验证模型覆盖独立于凭据连接标识。
    #[test]
    fn model_override_key_uses_provider_model_and_protocol_scope() {
        assert_eq!(
            PersistedAiConfiguration::provider_model_protocol_key(
                "minimax",
                "MiniMax-M3",
                "anthropic-messages.v1",
            ),
            "7:minimax:10:MiniMax-M3:21:anthropic-messages.v1",
        );
    }

    /// Verifies OpenRouter defaults to the curated set while other providers remain enabled.
    /// 验证 OpenRouter 默认使用精选集合，而其他供应商保持启用。
    #[test]
    fn model_access_defaults_are_provider_specific() {
        assert!(default_model_enabled(
            "openrouter",
            "deepseek/deepseek-v4-pro"
        ));
        assert!(!default_model_enabled(
            "openrouter",
            "vendor/unreviewed-model"
        ));
        assert!(default_model_enabled("deepseek", "deepseek-v4-pro"));
    }

    /// Verifies persisted access choices retain only deviations from code-owned defaults.
    /// 验证持久化访问选择仅保留相对代码默认值的偏差。
    #[test]
    fn model_access_overrides_store_only_default_deviations() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", true);
        assert!(configuration.model_is_enabled("openrouter", "vendor/unreviewed-model"));
        assert_eq!(configuration.model_enabled_overrides.len(), 1);

        configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", false);
        assert!(configuration.model_enabled_overrides.is_empty());
        assert!(!configuration.model_is_enabled("openrouter", "vendor/unreviewed-model"));
    }
}

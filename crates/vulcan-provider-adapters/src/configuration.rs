//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{
    PersistedConnectionGraph, ProviderDescriptor, ProviderToolCapabilityKind,
    ProviderToolTargetSelection, SecondaryRouteSelection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, EmbeddingEncoding, EmbeddingOutputKind, EmbeddingTask, InferenceUsageScope,
    ModelParameterValue, PromptCachePolicy, ReasoningEffort, RerankTruncation,
};

/// Identifies the current persisted AI configuration schema.
/// 标识当前持久化 AI 配置 Schema。
pub const PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION: u32 = 2;

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

/// Stores local limit overrides and parameter choices for one connection-model pair.
/// 保存一个连接与模型组合的本地限制覆盖及参数选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelExecutionOverrides {
    /// Optional context-window override, bounded by a declared provider maximum when present.
    /// 可选上下文窗口覆盖；供应商声明上限时不得超过该上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional output-token override, bounded by declared provider and effective context limits.
    /// 可选输出 Token 覆盖；受供应商已声明上限及有效上下文限制约束。
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

/// Selects one exact protocol or action binding for internal inference.
/// 为内部推理选择一个精确协议或动作绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InferenceRouteBinding {
    /// Selects one verified conversation protocol.
    /// 选择一个已验证对话协议。
    Protocol {
        /// Exact protocol profile identifier.
        /// 精确协议 Profile 标识。
        protocol_profile_id: String,
    },
    /// Selects one verified non-conversation action.
    /// 选择一个已验证非对话动作。
    Action {
        /// Exact action binding identifier.
        /// 精确动作绑定标识。
        action_binding_id: String,
    },
}

/// Persists one exact immutable-route source for configured inference.
/// 持久化一条已配置推理的精确不可变路由来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceRouteConfiguration {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact product or service-entry identifier.
    /// 精确产品或服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Exact protocol or action binding.
    /// 精确协议或动作绑定。
    pub binding: InferenceRouteBinding,
    /// Exact provider-neutral operation.
    /// 精确供应商无关操作。
    pub operation: AiOperation,
    /// Usage context this route is configured to serve.
    /// 当前路由被配置服务的使用场景。
    pub usage_scope: InferenceUsageScope,
}

/// Persists purpose-scoped LLM routes plus embedding and rerank routes for one consumer.
/// 为一个消费者持久化用途作用域的 LLM 路由及向量、重排路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InferenceProfile {
    /// Configured restricted LLM routes keyed by stable consumer-owned purpose identifier.
    /// 按消费者自有稳定用途标识索引的已配置受限 LLM 路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub llm_routes: BTreeMap<String, InferenceRouteConfiguration>,
    /// Optional configured embedding route.
    /// 可选的已配置向量路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<InferenceRouteConfiguration>,
    /// Optional configured rerank route.
    /// 可选的已配置重排路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank: Option<InferenceRouteConfiguration>,
}

/// Persists operation-specific internal model defaults without account selection.
/// 持久化不含账号选择的操作专属内部模型默认值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InternalModelOverrides {
    /// Optional context-window override for restricted LLM inference.
    /// 受限 LLM 推理可选的上下文窗口覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional output-token override for restricted LLM inference.
    /// 受限 LLM 推理可选的输出 Token 覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
    /// Optional exact reasoning effort.
    /// 可选的精确推理强度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Capability-registered model values.
    /// 能力契约已注册的模型值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameters: BTreeMap<String, ModelParameterValue>,
    /// Optional default embedding task.
    /// 可选的默认向量任务。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_task: Option<EmbeddingTask>,
    /// Optional default embedding dimensions.
    /// 可选的默认向量维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<u32>,
    /// Optional default embedding output shape.
    /// 可选的默认向量输出形态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_output_kind: Option<EmbeddingOutputKind>,
    /// Optional default embedding encoding.
    /// 可选的默认向量编码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_encoding: Option<EmbeddingEncoding>,
    /// Optional default rerank result count.
    /// 可选的默认重排结果数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_top_n: Option<u32>,
    /// Optional default rerank truncation.
    /// 可选的默认重排截断方式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_truncation: Option<RerankTruncation>,
    /// Optional default for returning rerank candidate content.
    /// 可选的重排候选内容返回默认值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_return_content: Option<bool>,
}

/// Stores all new AI configuration while excluding runtime-discovered model catalogs.
/// 保存全部新 AI 配置，同时排除运行时发现的模型目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedAiConfiguration {
    /// Explicit internal configuration schema version.
    /// 显式内部配置 Schema 版本。
    #[serde(default)]
    pub schema_version: u32,
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
    /// Internal model access choices scoped by provider, model, and operation.
    /// 按供应商、模型与操作限定作用域的内部模型访问选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub internal_model_enabled_overrides: BTreeMap<String, bool>,
    /// Independent inference routes grouped by stable consumer profile identifier.
    /// 按稳定消费者配置档标识分组的独立推理路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inference_profiles: BTreeMap<String, InferenceProfile>,
    /// Operation-specific internal model defaults.
    /// 操作专属内部模型默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub internal_model_overrides: BTreeMap<String, InternalModelOverrides>,
    /// Per-connection model secondary routes keyed by collision-resistant composite key.
    /// 按抗冲突组合键索引的连接模型二级路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secondary_routes: BTreeMap<String, SecondaryRouteSelection>,
    /// One explicit provider tool target per semantic capability.
    /// 每种语义能力唯一显式选择的供应商工具目标。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_tool_selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Semantic provider tool capabilities enabled by default for new sessions.
    /// 为新会话默认启用的语义供应商工具能力。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub provider_tool_enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Legacy target-scoped disabled state read only during schema migration.
    /// 仅在 Schema 迁移期间读取的旧逐目标禁用状态。
    #[serde(default, skip_serializing)]
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
            schema_version: PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION,
            usage_refresh_interval_seconds: default_usage_refresh_interval_seconds(),
            custom_providers: BTreeMap::new(),
            active: None,
            connection_graph: PersistedConnectionGraph::default(),
            model_overrides: BTreeMap::new(),
            model_enabled_overrides: BTreeMap::new(),
            internal_model_enabled_overrides: BTreeMap::new(),
            inference_profiles: BTreeMap::from([(
                String::from("vmm"),
                InferenceProfile::default(),
            )]),
            internal_model_overrides: BTreeMap::new(),
            secondary_routes: BTreeMap::new(),
            provider_tool_selections: BTreeMap::new(),
            provider_tool_enabled_capabilities: BTreeSet::new(),
            provider_tool_disabled_targets: BTreeMap::new(),
        }
    }
}

impl PersistedAiConfiguration {
    /// Migrates the exact supported historical schema to the current version.
    /// 将精确支持的历史 Schema 迁移到当前版本。
    ///
    /// # Returns
    /// 返回值
    /// Whether the configuration changed, or an explicit unsupported-version error.
    /// 返回配置是否变化，或明确的不支持版本错误。
    pub fn migrate_to_current_schema(&mut self) -> Result<bool, String> {
        match self.schema_version {
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION => {
                if self.inference_profiles.contains_key("vmm") {
                    Ok(false)
                } else {
                    self.inference_profiles
                        .insert(String::from("vmm"), InferenceProfile::default());
                    Ok(true)
                }
            }
            0 | 1 => {
                if self.schema_version == 0 {
                    self.provider_tool_selections
                        .remove(&ProviderToolCapabilityKind::EmbeddingCreate);
                    self.provider_tool_selections
                        .remove(&ProviderToolCapabilityKind::RerankDocuments);
                    self.provider_tool_disabled_targets
                        .remove(&ProviderToolCapabilityKind::EmbeddingCreate);
                    self.provider_tool_disabled_targets
                        .remove(&ProviderToolCapabilityKind::RerankDocuments);
                }
                self.provider_tool_enabled_capabilities = self
                    .provider_tool_selections
                    .iter()
                    .filter(|(capability, selection)| {
                        !self
                            .provider_tool_disabled_targets
                            .get(capability)
                            .is_some_and(|disabled| disabled.contains(selection))
                    })
                    .map(|(capability, _)| *capability)
                    .collect();
                self.provider_tool_disabled_targets.clear();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            unsupported => Err(format!(
                "Unsupported persisted AI configuration schema version {unsupported}; supported version is {PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION}."
            )),
        }
    }

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

    /// Builds a stable key for one provider-model-operation scoped internal setting.
    /// 为一项供应商、模型与操作作用域的内部设置构建稳定键。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `operation` identify one exact internal capability.
    /// `provider_id`、`model_id` 与 `operation` 标识一项精确内部能力。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant provider-model-operation key.
    /// 返回抗冲突的供应商、模型与操作组合键。
    pub fn internal_model_operation_key(
        provider_id: &str,
        model_id: &str,
        operation: AiOperation,
    ) -> String {
        let operation_id = operation.as_str();
        format!(
            "{}:{provider_id}:{}:{model_id}:{}:{operation_id}",
            provider_id.len(),
            model_id.len(),
            operation_id.len()
        )
    }

    /// Reports whether one model operation is enabled for internal inference.
    /// 报告一项模型操作是否已为内部推理启用。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `operation` identify one exact internal capability.
    /// `provider_id`、`model_id` 与 `operation` 标识一项精确内部能力。
    ///
    /// # Returns
    /// 返回值
    /// The explicit state, defaulting to disabled.
    /// 返回显式状态，默认禁用。
    pub fn internal_model_is_enabled(
        &self,
        provider_id: &str,
        model_id: &str,
        operation: AiOperation,
    ) -> bool {
        self.internal_model_enabled_overrides
            .get(Self::internal_model_operation_key(provider_id, model_id, operation).as_str())
            .copied()
            .unwrap_or(false)
    }

    /// Stores one explicit internal model operation state.
    /// 保存一项显式内部模型操作状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, `operation`, and `enabled` identify the exact state.
    /// `provider_id`、`model_id`、`operation` 与 `enabled` 标识精确状态。
    pub fn set_internal_model_enabled(
        &mut self,
        provider_id: &str,
        model_id: &str,
        operation: AiOperation,
        enabled: bool,
    ) {
        let key = Self::internal_model_operation_key(provider_id, model_id, operation);
        if enabled {
            self.internal_model_enabled_overrides.insert(key, true);
        } else {
            self.internal_model_enabled_overrides.remove(key.as_str());
        }
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

    /// Verifies the sole historical schema migration removes only retired inference tool choices.
    /// 验证唯一历史 Schema 迁移只删除已退役的推理工具选择。
    #[test]
    fn historical_configuration_migration_is_exact() {
        let mut configuration = PersistedAiConfiguration {
            schema_version: 0,
            ..PersistedAiConfiguration::default()
        };
        let search_target = ProviderToolTargetSelection {
            connection_id: String::from("search-connection"),
            target_id: String::from("search-target"),
        };
        configuration
            .provider_tool_selections
            .insert(ProviderToolCapabilityKind::WebSearch, search_target.clone());
        configuration.provider_tool_selections.insert(
            ProviderToolCapabilityKind::EmbeddingCreate,
            ProviderToolTargetSelection {
                connection_id: String::from("embedding-connection"),
                target_id: String::from("embedding-target"),
            },
        );
        configuration.provider_tool_disabled_targets.insert(
            ProviderToolCapabilityKind::RerankDocuments,
            BTreeSet::from([ProviderToolTargetSelection {
                connection_id: String::from("rerank-connection"),
                target_id: String::from("rerank-target"),
            }]),
        );

        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("schema zero must migrate")
        );
        assert_eq!(
            configuration.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert_eq!(
            configuration
                .provider_tool_selections
                .get(&ProviderToolCapabilityKind::WebSearch),
            Some(&search_target)
        );
        assert!(
            configuration
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::WebSearch)
        );
        assert!(
            !configuration
                .provider_tool_selections
                .contains_key(&ProviderToolCapabilityKind::EmbeddingCreate)
        );
        assert!(configuration.provider_tool_disabled_targets.is_empty());
        assert!(configuration.inference_profiles.contains_key("vmm"));
    }

    /// Verifies schema one converts only effective selected targets into capability switches.
    /// 验证 Schema 一只把实际生效的已选目标转换为能力开关。
    #[test]
    fn schema_one_migrates_target_activation_to_capability_activation() {
        let enabled_target = ProviderToolTargetSelection {
            connection_id: String::from("enabled-account"),
            target_id: String::from("enabled-target"),
        };
        let disabled_target = ProviderToolTargetSelection {
            connection_id: String::from("disabled-account"),
            target_id: String::from("disabled-target"),
        };
        let mut configuration = PersistedAiConfiguration {
            schema_version: 1,
            ..PersistedAiConfiguration::default()
        };
        configuration
            .provider_tool_selections
            .insert(ProviderToolCapabilityKind::WebSearch, enabled_target);
        configuration.provider_tool_selections.insert(
            ProviderToolCapabilityKind::WebExtract,
            disabled_target.clone(),
        );
        configuration.provider_tool_disabled_targets.insert(
            ProviderToolCapabilityKind::WebExtract,
            BTreeSet::from([disabled_target]),
        );

        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("schema one migration")
        );

        assert!(
            configuration
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::WebSearch)
        );
        assert!(
            !configuration
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::WebExtract)
        );
        assert!(configuration.provider_tool_disabled_targets.is_empty());
    }

    /// Verifies schema one migration preserves every configured VMM inference route.
    /// 验证 Schema 一迁移会保留全部已配置的 VMM 推理路由。
    #[test]
    fn schema_one_migration_preserves_configured_vmm_inference_profile() {
        let llm_route = InferenceRouteConfiguration {
            provider_id: String::from("llm-provider"),
            service_entry_id: String::from("llm-entry"),
            connection_id: String::from("llm-connection"),
            model_id: String::from("llm-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("llm-protocol"),
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        let embedding_route = InferenceRouteConfiguration {
            provider_id: String::from("embedding-provider"),
            service_entry_id: String::from("embedding-entry"),
            connection_id: String::from("embedding-connection"),
            model_id: String::from("embedding-model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("embedding-action"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        let rerank_route = InferenceRouteConfiguration {
            provider_id: String::from("rerank-provider"),
            service_entry_id: String::from("rerank-entry"),
            connection_id: String::from("rerank-connection"),
            model_id: String::from("rerank-model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("rerank-action"),
            },
            operation: AiOperation::RerankDocuments,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        let expected_profile = InferenceProfile {
            llm_routes: BTreeMap::from([(String::from("precheck_l1"), llm_route)]),
            embedding: Some(embedding_route),
            rerank: Some(rerank_route),
        };
        let mut configuration = PersistedAiConfiguration {
            schema_version: 1,
            inference_profiles: BTreeMap::from([(String::from("vmm"), expected_profile.clone())]),
            ..PersistedAiConfiguration::default()
        };

        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("schema one migration")
        );

        assert_eq!(
            configuration.inference_profiles.get("vmm"),
            Some(&expected_profile)
        );
    }

    /// Verifies unknown persisted schema generations are rejected without mutation.
    /// 验证未知持久化 Schema 代次被拒绝且不会发生变更。
    #[test]
    fn unknown_configuration_schema_is_rejected() {
        let mut configuration = PersistedAiConfiguration {
            schema_version: PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION + 1,
            ..PersistedAiConfiguration::default()
        };
        let before = configuration.clone();

        let error = configuration
            .migrate_to_current_schema()
            .expect_err("unknown version must fail");

        assert!(error.contains("Unsupported persisted AI configuration schema version"));
        assert_eq!(configuration, before);
    }
}

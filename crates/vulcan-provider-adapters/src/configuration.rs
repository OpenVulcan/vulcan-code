//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{
    CustomModelDefinition, CustomModelRequestProjection, CustomModelTemplateDecision,
    CustomProviderModelDefaults, PersistedConnectionGraph, ProviderDescriptor,
    ProviderToolCapabilityKind, ProviderToolTargetSelection, SecondaryRouteSelection,
    validate_custom_provider_reasoning_settings,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::LazyLock,
};
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, EmbeddingEncoding, EmbeddingOutputKind, EmbeddingTask,
    InferenceUsageScope, ModelCapabilities, ModelParameterValue, PromptCachePolicy,
    ReasoningControlMode, ReasoningEffort, RerankTruncation, ResourceKind,
};
use vulcan_context::{ContextCompactionPolicy, ContextWindowPolicyOverrides};

/// Identifies the current persisted AI configuration schema.
/// 标识当前持久化 AI 配置 Schema。
pub const PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION: u32 = 13;

/// Stores only user-declared differences from immutable model capability sources.
/// 仅保存用户相对于不可变模型能力来源声明的差异。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityOverrides {
    /// Optional user context-window replacement.
    /// 可选用户上下文窗口替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional user output-token replacement.
    /// 可选用户输出 Token 上限替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
    /// Optional complete non-text input resource replacement.
    /// 可选的完整非文本输入资源替换集合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_resources: Option<Vec<ResourceKind>>,
    /// Legacy non-text output replacement retained only for schema-seven decoding.
    /// 仅为读取 Schema 7 而保留的旧版非文本输出替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_resources: Option<Vec<ResourceKind>>,
    /// Optional complete reasoning support replacement.
    /// 可选的完整思考支持状态替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<CapabilitySupport>,
    /// Optional complete reasoning-control replacement.
    /// 可选的完整思考控制替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_control: Option<ReasoningControlMode>,
    /// Optional complete reasoning-level replacement.
    /// 可选的完整思考档位替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_efforts: Option<Vec<ReasoningEffort>>,
    /// Optional sparse Codex context-window policy carried with this provider/model layer.
    /// 随供应商或模型层保存的可选稀疏 Codex 上下文窗口策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_policy: Option<ContextWindowPolicyOverrides>,
    /// Optional exact request projection used by the selected model.
    /// 可选的选定模型精确请求投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_projection: Option<CustomModelRequestProjection>,
}

impl ModelCapabilityOverrides {
    /// Reports whether this record contains no user-owned capability difference.
    /// 报告此记录是否不包含任何用户持有的能力差异。
    ///
    /// # Returns
    /// 返回值
    /// `true` when every optional difference is absent.
    /// 当全部可选差异均不存在时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.context_limit.is_none()
            && self.output_limit.is_none()
            && self.input_resources.is_none()
            && self.output_resources.is_none()
            && self.reasoning.is_none()
            && self.reasoning_control.is_none()
            && self.reasoning_efforts.is_none()
            && self.context_window_policy.is_none()
            && self.request_projection.is_none()
    }

    /// Validates one direct model-setting record before it can enter persisted configuration.
    /// 在一份直接模型设置进入持久化配置前对其进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success only when output remains text-only and reasoning fields form one complete policy.
    /// 仅在输出保持纯文本且思考字段形成完整策略时返回成功。
    pub fn validate(&self) -> Result<(), String> {
        if self.context_limit == Some(0) {
            return Err(String::from(
                "A direct model context limit must be greater than zero.",
            ));
        }
        if self.output_limit == Some(0) {
            return Err(String::from(
                "A direct model output limit must be greater than zero.",
            ));
        }
        if let (Some(context_limit), Some(output_limit)) = (self.context_limit, self.output_limit)
            && output_limit > context_limit
        {
            return Err(String::from(
                "A direct model output limit cannot exceed its context limit.",
            ));
        }
        if self.output_resources.is_some() {
            return Err(String::from(
                "Non-text model output cannot be configured through the generic provider settings contract.",
            ));
        }
        // reasoning_field_count prevents partially persisted reasoning contracts.
        // reasoning_field_count 防止持久化不完整的思考能力契约。
        let reasoning_field_count = usize::from(self.reasoning.is_some())
            + usize::from(self.reasoning_control.is_some())
            + usize::from(self.reasoning_efforts.is_some());
        if reasoning_field_count != 0 && reasoning_field_count != 3 {
            return Err(String::from(
                "Model reasoning support, control, and levels must be saved together.",
            ));
        }
        if let (Some(reasoning), Some(control), Some(efforts)) = (
            self.reasoning,
            self.reasoning_control,
            self.reasoning_efforts.as_deref(),
        ) {
            validate_custom_provider_reasoning_settings(reasoning, control, efforts)?;
        }
        if let Some(context_window_policy) = self.context_window_policy {
            context_window_policy.validate()?;
        }
        if let Some(request_projection) = self.request_projection.as_ref() {
            request_projection.validate_paths()?;
        }
        Ok(())
    }
}

/// Applies only the fields explicitly owned by one sparse capability layer.
/// 仅应用一层稀疏能力配置中明确由用户持有的字段。
///
/// # Parameters
/// 参数
/// `overrides` is a provider- or model-scoped sparse layer and `capabilities` is mutated in place.
/// `overrides` 是供应商级或模型级稀疏层，`capabilities` 会被原地修改。
fn apply_capability_overrides(
    overrides: &ModelCapabilityOverrides,
    capabilities: &mut ModelCapabilities,
) {
    if let Some(context_limit) = overrides.context_limit {
        capabilities.context_limit = Some(context_limit);
    }
    if let Some(output_limit) = overrides.output_limit {
        capabilities.output_limit = Some(output_limit);
    }
    if let Some(input_resources) = overrides.input_resources.as_ref() {
        capabilities.input_resources = input_resources.clone();
    }
    if let Some(reasoning) = overrides.reasoning {
        capabilities.reasoning = reasoning;
    }
    if let Some(reasoning_control) = overrides.reasoning_control {
        capabilities.reasoning_control = reasoning_control;
    }
    if let Some(reasoning_efforts) = overrides.reasoning_efforts.as_ref() {
        capabilities.reasoning_efforts = reasoning_efforts.clone();
    }
}

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

/// Stores protocol-specific cache and adapter parameter choices for one provider model.
/// 保存一个供应商模型的协议专属缓存与适配器参数选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelExecutionOverrides {
    /// Legacy protocol-specific context limit retained only for schema-seven decoding.
    /// 仅为读取 Schema 7 而保留的旧版协议专属上下文限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Legacy protocol-specific output limit retained only for schema-seven decoding.
    /// 仅为读取 Schema 7 而保留的旧版协议专属输出限制。
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

impl ModelExecutionOverrides {
    /// Reports whether this protocol-specific record contains no user-owned setting.
    /// 报告此协议专属记录是否不包含任何用户持有设置。
    ///
    /// # Returns
    /// 返回值
    /// `true` when every legacy limit, cache choice, and model parameter is absent.
    /// 当全部旧限制、缓存选择与模型参数均不存在时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.context_limit.is_none()
            && self.output_limit.is_none()
            && self.prompt_cache.is_none()
            && self.model_parameters.is_empty()
    }
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

/// Stores one credential account's default model access and sparse per-model differences.
/// 保存一个凭据账号的默认模型访问状态与稀疏逐模型差异。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionModelAccessPolicy {
    /// Default enabled state inherited by every current and future supported model.
    /// 当前及未来所有受支持模型继承的默认启用状态。
    #[serde(default = "default_connection_model_enabled")]
    pub default_enabled: bool,
    /// Exact model decisions that differ from the account default.
    /// 与账号默认状态不同的精确模型决定。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_enabled_overrides: BTreeMap<String, bool>,
}

impl Default for ConnectionModelAccessPolicy {
    /// Builds an account policy that enables supported models by default.
    /// 构建默认启用受支持模型的账号策略。
    fn default() -> Self {
        Self {
            default_enabled: true,
            model_enabled_overrides: BTreeMap::new(),
        }
    }
}

impl ConnectionModelAccessPolicy {
    /// Resolves the effective enabled state for one exact model.
    /// 解析一个精确模型的有效启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the exact provider model identifier.
    /// `model_id` 是精确供应商模型标识。
    ///
    /// # Returns
    /// 返回值
    /// The sparse override when present, otherwise the account default.
    /// 存在时返回稀疏覆盖，否则返回账号默认值。
    pub fn model_is_enabled(&self, model_id: &str) -> bool {
        self.model_enabled_overrides
            .get(model_id)
            .copied()
            .unwrap_or(self.default_enabled)
    }

    /// Stores one model decision while retaining only differences from the account default.
    /// 保存一个模型决定，并仅保留与账号默认值不同的差异。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` identifies the exact model and `enabled` is the desired state.
    /// `model_id` 标识精确模型，`enabled` 是期望状态。
    pub fn set_model_enabled(&mut self, model_id: &str, enabled: bool) {
        if enabled == self.default_enabled {
            self.model_enabled_overrides.remove(model_id);
        } else {
            self.model_enabled_overrides
                .insert(model_id.to_string(), enabled);
        }
    }

    /// Replaces the account-wide default and removes now-redundant model differences.
    /// 替换账号级默认值并移除现已冗余的逐模型差异。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` becomes the state inherited by every supported model.
    /// `enabled` 成为所有受支持模型继承的状态。
    pub fn set_all_models_enabled(&mut self, enabled: bool) {
        self.default_enabled = enabled;
        self.model_enabled_overrides.clear();
    }
}

/// Returns the safe default account model access state.
/// 返回安全的账号模型访问默认状态。
fn default_connection_model_enabled() -> bool {
    true
}

/// Stores one explicit per-million-token billing override without using floating-point values.
/// 保存一条显式的每百万 Token 计费覆盖，并且不使用浮点数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BillingPriceOverride {
    /// Fresh input price per million tokens as a decimal string.
    /// 新鲜输入每百万 Token 价格，使用十进制字符串保存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_per_million: Option<String>,
    /// Output price per million tokens as a decimal string.
    /// 输出每百万 Token 价格，使用十进制字符串保存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_per_million: Option<String>,
    /// Cache-read price per million tokens as a decimal string.
    /// 缓存读取每百万 Token 价格，使用十进制字符串保存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_cost_per_million: Option<String>,
    /// Cache-creation price per million tokens as a decimal string.
    /// 缓存创建每百万 Token 价格，使用十进制字符串保存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_cost_per_million: Option<String>,
    /// Optional provider-specific cost multiplier as a decimal string.
    /// 可选的 Provider 专属成本倍率，使用十进制字符串保存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_multiplier: Option<String>,
    /// Human-readable price source or catalog revision.
    /// 可读的价格来源或目录版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl BillingPriceOverride {
    /// Reports whether no user-owned price field is present.
    /// 判断该记录是否没有任何用户拥有的价格字段。
    pub fn is_empty(&self) -> bool {
        self.input_cost_per_million.is_none()
            && self.output_cost_per_million.is_none()
            && self.cache_read_cost_per_million.is_none()
            && self.cache_creation_cost_per_million.is_none()
            && self.cost_multiplier.is_none()
            && self.source.is_none()
    }

    /// Validates decimal prices, multiplier, and source text without inventing defaults.
    /// 校验十进制价格、倍率与来源文本且不虚构默认值。
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            (
                "input_cost_per_million",
                self.input_cost_per_million.as_deref(),
            ),
            (
                "output_cost_per_million",
                self.output_cost_per_million.as_deref(),
            ),
            (
                "cache_read_cost_per_million",
                self.cache_read_cost_per_million.as_deref(),
            ),
            (
                "cache_creation_cost_per_million",
                self.cache_creation_cost_per_million.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                let parsed = parse_billing_decimal(field, value)?;
                if parsed.is_sign_negative() {
                    return Err(format!("Billing field `{field}` cannot be negative."));
                }
            }
        }
        if let Some(value) = self.cost_multiplier.as_deref() {
            let parsed = parse_billing_decimal("cost_multiplier", value)?;
            if parsed <= Decimal::ZERO {
                return Err(String::from(
                    "Billing cost_multiplier must be greater than zero.",
                ));
            }
        }
        if self.source.as_deref().is_some_and(|value| {
            let trimmed = value.trim();
            trimmed.is_empty() || trimmed.len() > 256
        }) {
            return Err(String::from(
                "Billing price source must contain between 1 and 256 UTF-8 bytes.",
            ));
        }
        Ok(())
    }
}

/// Stores cc-switch-compatible provider/model/route billing overrides.
/// 保存与 cc-switch 兼容的 Provider、模型和路由计费覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBillingConfiguration {
    /// Enables cost calculation while usage facts remain recorded when disabled.
    /// 启用成本计算；即使关闭，原始用量事实仍会记录。
    #[serde(default = "default_billing_enabled")]
    pub enabled: bool,
    /// Currency label attached to persisted cost records.
    /// 持久化成本记录使用的货币标签。
    #[serde(default = "default_billing_currency")]
    pub currency: String,
    /// Imported provider-neutral model prices keyed by exact model identifier.
    /// 按精确模型标识保存的导入型供应商无关模型价格。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub catalog_model_prices: BTreeMap<String, BillingPriceOverride>,
    /// User-imported model tombstones that suppress only the lower built-in catalog.
    /// 用户导入的模型删除标记，仅屏蔽更低优先级的内置目录。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub catalog_deleted_model_ids: BTreeSet<String>,
    /// Provider-wide price overrides keyed by the exact provider identifier.
    /// 按精确 Provider 标识保存的 Provider 级价格覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_prices: BTreeMap<String, BillingPriceOverride>,
    /// Provider-model price overrides keyed by `provider_billing_model_key`.
    /// 按 `provider_billing_model_key` 保存的 Provider-模型级价格覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_prices: BTreeMap<String, BillingPriceOverride>,
    /// Provider-model-route price overrides keyed by `provider_billing_route_key`.
    /// 按 `provider_billing_route_key` 保存的 Provider-模型-路由级价格覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub route_prices: BTreeMap<String, BillingPriceOverride>,
}

impl Default for ProviderBillingConfiguration {
    /// Builds an enabled billing configuration with no guessed prices.
    /// 构建启用计费但不猜测任何价格的默认配置。
    fn default() -> Self {
        Self {
            enabled: default_billing_enabled(),
            currency: default_billing_currency(),
            catalog_model_prices: BTreeMap::new(),
            catalog_deleted_model_ids: BTreeSet::new(),
            provider_prices: BTreeMap::new(),
            model_prices: BTreeMap::new(),
            route_prices: BTreeMap::new(),
        }
    }
}

impl ProviderBillingConfiguration {
    /// Reports whether this configuration has no non-default user-owned value.
    /// 判断该配置是否没有任何非默认的用户拥有值。
    pub fn is_empty(&self) -> bool {
        self.enabled
            && self.currency == default_billing_currency()
            && self.catalog_model_prices.is_empty()
            && self.catalog_deleted_model_ids.is_empty()
            && self.provider_prices.is_empty()
            && self.model_prices.is_empty()
            && self.route_prices.is_empty()
    }

    /// Resolves one price by route, model, then provider precedence.
    /// 按路由、模型、Provider 的优先级解析一条价格覆盖。
    pub fn resolve_price(
        &self,
        provider_id: &str,
        model_id: &str,
        route_id: Option<&str>,
    ) -> Option<&BillingPriceOverride> {
        self.resolve_price_with_scope(provider_id, model_id, route_id)
            .map(|(_, _, price)| price)
    }

    /// Resolves one price together with its exact precedence scope and persisted key.
    /// 解析一条价格及其精确优先级作用域与持久化键。
    pub fn resolve_price_with_scope(
        &self,
        provider_id: &str,
        model_id: &str,
        route_id: Option<&str>,
    ) -> Option<(&'static str, String, &BillingPriceOverride)> {
        if let Some(route_id) = route_id.filter(|value| !value.trim().is_empty()) {
            let key = provider_billing_route_key(provider_id, model_id, route_id);
            if let Some(price) = self.route_prices.get(&key) {
                return Some(("route", key, price));
            }
        }
        let model_key = provider_billing_model_key(provider_id, model_id);
        if let Some(price) = self.model_prices.get(&model_key) {
            return Some(("model", model_key, price));
        }
        if let Some(price) = self.provider_prices.get(provider_id) {
            return Some(("provider", provider_id.to_string(), price));
        }
        if let Some(price) = self.catalog_model_prices.get(model_id) {
            return Some(("catalog_model", model_id.to_string(), price));
        }
        if self.catalog_deleted_model_ids.contains(model_id) {
            return None;
        }
        BUILTIN_CC_SWITCH_MODEL_PRICES
            .get(model_id)
            .map(|price| ("builtin_catalog_model", model_id.to_string(), price))
    }

    /// Validates currency, exact override keys, and every decimal price record.
    /// 校验币种、精确覆盖键与每条十进制价格记录。
    pub fn validate(&self) -> Result<(), String> {
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(String::from(
                "Billing currency must be a three-letter uppercase ISO currency code.",
            ));
        }
        for (model_id, price) in &self.catalog_model_prices {
            validate_billing_identity("catalog model", model_id)?;
            validate_billing_price("catalog model", model_id, price)?;
        }
        for model_id in &self.catalog_deleted_model_ids {
            validate_billing_identity("deleted catalog model", model_id)?;
            if self.catalog_model_prices.contains_key(model_id) {
                return Err(format!(
                    "Catalog model `{model_id}` cannot be both active and deleted."
                ));
            }
        }
        for (provider_id, price) in &self.provider_prices {
            validate_billing_identity("provider", provider_id)?;
            validate_billing_price("provider", provider_id, price)?;
        }
        for (key, price) in &self.model_prices {
            let parts = billing_composite_key_parts(key, 2)
                .ok_or_else(|| format!("Malformed provider-model billing key `{key}`."))?;
            validate_billing_identity("provider", parts[0])?;
            validate_billing_identity("model", parts[1])?;
            validate_billing_price("provider-model", key, price)?;
        }
        for (key, price) in &self.route_prices {
            let parts = billing_composite_key_parts(key, 3)
                .ok_or_else(|| format!("Malformed provider-model-route billing key `{key}`."))?;
            validate_billing_identity("provider", parts[0])?;
            validate_billing_identity("model", parts[1])?;
            validate_billing_identity("route", parts[2])?;
            validate_billing_price("provider-model-route", key, price)?;
        }
        Ok(())
    }

    /// Imports the cc-switch `model-pricing.json` contract as the lowest-precedence model catalog.
    /// 将 cc-switch 的 `model-pricing.json` 契约导入为最低优先级模型价格目录。
    pub fn import_cc_switch_model_pricing_json(
        &mut self,
        source_id: &str,
        json: &str,
    ) -> Result<usize, String> {
        if source_id.trim().is_empty() || source_id.len() > 256 {
            return Err(String::from(
                "cc-switch pricing source id must contain 1 to 256 bytes.",
            ));
        }
        let (imported, deleted) = parse_cc_switch_model_pricing_json(source_id, json)?;
        let imported_count = imported.len();
        self.catalog_model_prices = imported;
        self.catalog_deleted_model_ids = deleted;
        Ok(imported_count)
    }
}

/// Exact cc-switch model-pricing file fields consumed by the compatibility importer.
/// 兼容导入器消费的精确 cc-switch 模型价格文件字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CcSwitchModelPricingFile {
    /// Exact cc-switch file version.
    /// 精确 cc-switch 文件版本。
    #[serde(default = "default_cc_switch_pricing_version")]
    version: u32,
    /// Imported model entries.
    /// 导入的模型条目。
    #[serde(default)]
    models: Vec<CcSwitchModelPricingEntry>,
    /// Model identities explicitly deleted by the cc-switch user.
    /// cc-switch 用户显式删除的模型标识。
    #[serde(default)]
    deleted_model_ids: Vec<String>,
    /// cc-switch models.dev synchronization settings retained outside Vulcan runtime semantics.
    /// 保留在 Vulcan 运行时语义之外的 cc-switch models.dev 同步设置。
    #[serde(default, rename = "modelsDevSync")]
    _models_dev_sync: Option<serde_json::Value>,
    /// Frozen-source provenance accepted only for the embedded Vulcan snapshot.
    /// 仅为 Vulcan 内嵌快照接受的冻结来源信息。
    #[serde(default)]
    _source: Option<serde_json::Value>,
}

/// Exact price fields for one cc-switch model-pricing entry.
/// 单条 cc-switch 模型价格记录的精确价格字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CcSwitchModelPricingEntry {
    /// Exact model identity.
    /// 精确模型标识。
    model_id: String,
    /// Human-readable display name accepted but not used for routing.
    /// 接受但不参与路由的可读显示名称。
    #[serde(rename = "displayName")]
    _display_name: String,
    /// Input price per million tokens.
    /// 输入每百万 Token 价格。
    input_cost_per_million: String,
    /// Output price per million tokens.
    /// 输出每百万 Token 价格。
    output_cost_per_million: String,
    /// Cache-read price per million tokens.
    /// 缓存读取每百万 Token 价格。
    cache_read_cost_per_million: String,
    /// Cache-write price per million tokens.
    /// 缓存写入每百万 Token 价格。
    cache_creation_cost_per_million: String,
}

/// Returns the sole cc-switch model-pricing file version supported by this importer.
/// 返回当前导入器支持的唯一 cc-switch 模型价格文件版本。
fn default_cc_switch_pricing_version() -> u32 {
    1
}

/// Parses one exact cc-switch price file into model-keyed Vulcan overrides.
/// 将一份精确的 cc-switch 价格文件解析为按模型键控的 Vulcan 覆盖。
fn parse_cc_switch_model_pricing_json(
    source_id: &str,
    json: &str,
) -> Result<(BTreeMap<String, BillingPriceOverride>, BTreeSet<String>), String> {
    let file: CcSwitchModelPricingFile = serde_json::from_str(json)
        .map_err(|error| format!("Invalid cc-switch model-pricing.json: {error}"))?;
    if file.version != 1 {
        return Err(format!(
            "Unsupported cc-switch model-pricing.json version {}.",
            file.version
        ));
    }
    let deleted = file.deleted_model_ids.into_iter().collect::<BTreeSet<_>>();
    for model_id in &deleted {
        validate_billing_identity("deleted catalog model", model_id)?;
    }
    let mut imported = BTreeMap::new();
    for model in file.models {
        validate_billing_identity("catalog model", model.model_id.as_str())?;
        if deleted.contains(model.model_id.as_str()) {
            continue;
        }
        let price = BillingPriceOverride {
            input_cost_per_million: Some(model.input_cost_per_million),
            output_cost_per_million: Some(model.output_cost_per_million),
            cache_read_cost_per_million: Some(model.cache_read_cost_per_million),
            cache_creation_cost_per_million: Some(model.cache_creation_cost_per_million),
            cost_multiplier: None,
            source: Some(format!("cc-switch:{source_id}")),
        };
        validate_billing_price("catalog model", model.model_id.as_str(), &price)?;
        if imported.insert(model.model_id.clone(), price).is_some() {
            return Err(format!(
                "cc-switch pricing contains duplicate model `{}`.",
                model.model_id
            ));
        }
    }
    Ok((imported, deleted))
}

/// Frozen cc-switch seed prices used only after every user-owned override scope misses.
/// 仅在所有用户覆盖作用域均未命中后使用的冻结 cc-switch 种子价格。
static BUILTIN_CC_SWITCH_MODEL_PRICES: LazyLock<BTreeMap<String, BillingPriceOverride>> =
    LazyLock::new(|| {
        parse_cc_switch_model_pricing_json(
            "builtin:0345fad6048eed65b3423bedc8ce5711320ddfc3",
            include_str!("../resources/cc-switch-model-pricing.v1.json"),
        )
        .map(|(prices, _)| prices)
        .expect("embedded cc-switch pricing snapshot must satisfy its validated contract")
    });

/// Parses one finite decimal billing value from its exact text representation.
/// 从精确文本表示解析一个有限十进制计费值。
fn parse_billing_decimal(field: &str, value: &str) -> Result<Decimal, String> {
    if value.trim() != value || value.is_empty() {
        return Err(format!(
            "Billing field `{field}` must be a non-empty canonical decimal."
        ));
    }
    Decimal::from_str(value)
        .map_err(|error| format!("Billing field `{field}` is not a valid decimal: {error}"))
}

/// Validates one exact billing identity segment.
/// 校验一个精确计费身份片段。
fn validate_billing_identity(kind: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Billing {kind} identity cannot be empty."));
    }
    Ok(())
}

/// Validates one non-empty price record in its owning scope.
/// 校验所属作用域中的一条非空价格记录。
fn validate_billing_price(
    scope: &str,
    key: &str,
    price: &BillingPriceOverride,
) -> Result<(), String> {
    if price.is_empty() {
        return Err(format!("Billing {scope} override `{key}` cannot be empty."));
    }
    price.validate()
}

/// Returns the default state for billing cost calculation.
/// 返回计费成本计算的默认状态。
fn default_billing_enabled() -> bool {
    true
}

/// Returns the default currency label for billing records.
/// 返回计费记录的默认货币标签。
fn default_billing_currency() -> String {
    String::from("USD")
}

/// Builds the exact provider-model billing key using byte-length-prefixed segments.
/// 使用字节长度前缀片段构建精确的 Provider-模型计费键。
fn billing_composite_key(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| format!("{}:{}", part.len(), part))
        .collect()
}

/// Parses one complete length-prefixed billing key into the required number of segments.
/// 将一个完整长度前缀计费键解析为所需数量的片段。
fn billing_composite_key_parts(value: &str, expected_parts: usize) -> Option<Vec<&str>> {
    let mut remainder = value;
    let mut parts = Vec::with_capacity(expected_parts);
    for _ in 0..expected_parts {
        let (part, next) = take_length_prefixed_segment(remainder)?;
        parts.push(part);
        remainder = next;
    }
    remainder.is_empty().then_some(parts)
}

/// Returns the exact key for a provider-model billing override.
/// 返回 Provider-模型计费覆盖的精确键。
pub fn provider_billing_model_key(provider_id: &str, model_id: &str) -> String {
    billing_composite_key(&[provider_id, model_id])
}

/// Returns the exact key for a provider-model-route billing override.
/// 返回 Provider-模型-路由计费覆盖的精确键。
pub fn provider_billing_route_key(provider_id: &str, model_id: &str, route_id: &str) -> String {
    billing_composite_key(&[provider_id, model_id, route_id])
}

/// Describes one bounded standalone custom-provider failure isolated during hydration.
/// 描述一条在补水期间被隔离的有界独立自定义供应商失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderManifestFailure {
    /// Bounded display identity of the exact referenced custom provider.
    /// 精确被引用自定义供应商的有界展示身份。
    pub provider_id: String,
    /// Bounded non-secret repair diagnostic from the failed read, parse, or validation step.
    /// 失败读取、解析或校验步骤产生的有界非秘密修复诊断。
    pub message: String,
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
    /// Provider-native usage billing and cc-switch price overrides.
    /// Provider 原生用量计费与 cc-switch 价格覆盖。
    #[serde(
        default,
        skip_serializing_if = "ProviderBillingConfiguration::is_empty"
    )]
    pub billing: ProviderBillingConfiguration,
    /// User-owned compatibility provider definitions without credential secrets or model catalogs.
    /// 不包含凭据秘密或模型目录的用户自有兼容供应商定义。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_providers: BTreeMap<String, ProviderDescriptor>,
    /// Custom provider identifiers whose definition bytes live in standalone manifest files.
    /// 定义字节已存于独立清单文件的自定义供应商标识。
    ///
    /// After migration the standalone `provider-manifests/custom/<encoded-id>.json` files are
    /// authoritative and the centralized map is loaded from them; this reference set keeps the
    /// centralized configuration compact and makes the migration idempotent (plan §R6.2).
    /// 迁移后独立 `provider-manifests/custom/<编码-id>.json` 文件为权威，集中式映射从
    /// 文件加载；该引用集使集中配置保持精简，并让迁移具备幂等性（方案 §R6.2）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub custom_provider_file_refs: BTreeSet<String>,
    /// Hydrated full custom manifests keyed by exact provider identifier.
    /// 按精确供应商标识索引的已补水完整自定义 Manifest。
    ///
    /// These documents are owned by standalone JSON files and deliberately excluded from the
    /// centralized configuration serialization. Runtime activation freezes their compiled
    /// form together with model catalogs.
    /// 这些文档由独立 JSON 文件持有，并有意排除在中央配置序列化之外。运行时激活会把
    /// 它们的编译结果与模型目录一同冻结。
    #[serde(default, skip)]
    pub custom_provider_manifests: BTreeMap<String, crate::ProviderManifestV1>,
    /// Runtime-only bounded failures for standalone custom providers isolated during hydration.
    /// 补水期间被隔离的独立自定义供应商运行时有界失败记录。
    ///
    /// The central reference and original standalone bytes remain authoritative while an entry
    /// is present. Cross-file persistence carries those bytes forward unchanged, so healthy
    /// provider settings remain writable without erasing the failed source (original plan §18.3).
    /// 存在记录时，中央引用与原始独立字节仍是权威事实。跨文件持久化会原样携带这些字节，
    /// 因而健康供应商设置仍可写入且不会擦除失败源（原始方案 §18.3）。
    #[serde(default, skip)]
    pub custom_provider_manifest_failures: Vec<CustomProviderManifestFailure>,
    /// Total isolated custom-provider count before the browser-facing list bound was applied.
    /// 应用面向浏览器的列表上限前，被隔离自定义供应商的总数。
    #[serde(default, skip)]
    pub custom_provider_manifest_failure_total_count: u64,
    /// Whether additional isolated-provider diagnostics were omitted by the runtime bound.
    /// 是否有额外隔离供应商诊断因运行时上限被省略。
    #[serde(default, skip)]
    pub custom_provider_manifest_failures_truncated: bool,
    /// User-owned model definitions grouped by custom provider identifier.
    /// 按自定义供应商标识分组的用户自有模型定义。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_models: BTreeMap<String, BTreeMap<String, CustomModelDefinition>>,
    /// Capability defaults grouped by custom provider identifier.
    /// 按自定义供应商标识分组的能力默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_provider_model_defaults: BTreeMap<String, CustomProviderModelDefaults>,
    /// User-reviewed common-template decisions keyed by provider and model.
    /// 按供应商与模型索引的用户审查公共模板决策。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_model_template_decisions: BTreeMap<String, CustomModelTemplateDecision>,
    /// User-owned per-model protocol preferences for custom providers keyed by provider and model.
    /// 按供应商与模型索引的自定义供应商逐模型协议偏好。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_model_protocol_preferences: BTreeMap<String, crate::CustomModelProtocolPreference>,
    /// Explicit custom-model availability differences keyed by connection and model.
    /// 按连接与模型索引的自定义模型显式可用性差异。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_model_connection_enabled_overrides: BTreeMap<String, bool>,
    /// Account-level model access policies keyed by exact connection identifier.
    /// 按精确连接标识索引的账号级模型访问策略。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connection_model_access_policies: BTreeMap<String, ConnectionModelAccessPolicy>,
    /// Credential connections isolated until one explicit paid model validation succeeds.
    /// 在一次显式付费模型验证成功前保持隔离的凭据连接。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pending_validation_connection_ids: BTreeSet<String>,
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
    /// User-owned capability differences keyed by provider and model.
    /// 按供应商与模型索引的用户自有能力差异。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_capability_overrides: BTreeMap<String, ModelCapabilityOverrides>,
    /// User-owned provider capability defaults inherited by every provider model.
    /// 由供应商全部模型继承的用户自有供应商级能力默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_capability_overrides: BTreeMap<String, ModelCapabilityOverrides>,
    /// System-wide sparse Codex context-window defaults.
    /// 系统级稀疏 Codex 上下文窗口默认值。
    #[serde(
        default,
        skip_serializing_if = "ContextWindowPolicyOverrides::is_empty"
    )]
    pub context_window_policy_overrides: ContextWindowPolicyOverrides,
    /// Provider-scoped sparse Codex context-window defaults.
    /// 供应商级稀疏 Codex 上下文窗口默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
    /// Exact provider-model sparse Codex context-window defaults.
    /// 精确供应商模型级稀疏 Codex 上下文窗口默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
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

/// Reads one byte-length-prefixed UTF-8 segment from a persisted composite key.
/// 从持久化组合键中读取一个以字节长度为前缀的 UTF-8 片段。
///
/// # Parameters
/// 参数
/// `value` is the remaining composite-key text beginning with `<byte-length>:`.
/// `value` 是以 `<字节长度>:` 开头的组合键剩余文本。
///
/// # Returns
/// 返回值
/// The exact segment and unconsumed suffix, or `None` for a malformed boundary.
/// 返回精确片段与未消费后缀；边界格式错误时返回 `None`。
fn take_length_prefixed_segment(value: &str) -> Option<(&str, &str)> {
    let (length_text, payload) = value.split_once(':')?;
    let length = length_text.parse::<usize>().ok()?;
    let segment = payload.get(..length)?;
    let remainder = payload.get(length..)?;
    Some((segment, remainder))
}

/// Converts one provider-model-protocol key into its provider-model scope.
/// 将一个供应商-模型-协议组合键转换为供应商-模型作用域。
///
/// # Parameters
/// 参数
/// `key` is produced by `provider_model_protocol_key` in an older configuration schema.
/// `key` 由旧配置 Schema 中的 `provider_model_protocol_key` 生成。
///
/// # Returns
/// 返回值
/// The canonical provider-model key when all three segments are exact and complete.
/// 当三个片段均精确且完整时返回规范供应商-模型键。
fn provider_model_key_from_protocol_key(key: &str) -> Option<String> {
    let (provider_id, model_id, _) =
        PersistedAiConfiguration::provider_model_protocol_key_parts(key)?;
    Some(PersistedAiConfiguration::provider_model_key(
        provider_id,
        model_id,
    ))
}

/// Merges one legacy protocol-specific limit into a conservative model-wide value.
/// 将一个旧版协议专属限制合并为保守的模型级值。
///
/// # Parameters
/// 参数
/// `current` is the already merged value and `candidate` is one protocol-specific value.
/// `current` 是已合并值，`candidate` 是一个协议专属值。
///
/// # Returns
/// 返回值
/// The smallest configured value so the migration never expands a previous protocol limit.
/// 返回最小已配置值，确保迁移不会扩大旧协议限制。
fn conservative_legacy_limit(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

impl Default for PersistedAiConfiguration {
    /// Builds one configuration with no providers and a five-minute usage refresh interval.
    /// 构建一个不含供应商且用量刷新间隔为五分钟的配置。
    fn default() -> Self {
        Self {
            schema_version: PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION,
            usage_refresh_interval_seconds: default_usage_refresh_interval_seconds(),
            billing: ProviderBillingConfiguration::default(),
            custom_providers: BTreeMap::new(),
            custom_provider_file_refs: BTreeSet::new(),
            custom_provider_manifests: BTreeMap::new(),
            custom_provider_manifest_failures: Vec::new(),
            custom_provider_manifest_failure_total_count: 0,
            custom_provider_manifest_failures_truncated: false,
            custom_models: BTreeMap::new(),
            custom_provider_model_defaults: BTreeMap::new(),
            custom_model_template_decisions: BTreeMap::new(),
            custom_model_protocol_preferences: BTreeMap::new(),
            custom_model_connection_enabled_overrides: BTreeMap::new(),
            connection_model_access_policies: BTreeMap::new(),
            pending_validation_connection_ids: BTreeSet::new(),
            active: None,
            connection_graph: PersistedConnectionGraph::default(),
            model_overrides: BTreeMap::new(),
            model_enabled_overrides: BTreeMap::new(),
            model_capability_overrides: BTreeMap::new(),
            provider_capability_overrides: BTreeMap::new(),
            context_window_policy_overrides: ContextWindowPolicyOverrides::default(),
            provider_context_window_policy_overrides: BTreeMap::new(),
            model_context_window_policy_overrides: BTreeMap::new(),
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
    /// Reports whether one standalone custom-provider source is currently isolated.
    /// 报告一个独立自定义供应商源当前是否已被隔离。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact provider identity referenced by the central configuration.
    /// `provider_id` 是中央配置引用的精确供应商身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the authoritative standalone reference exists but hydration produced no
    /// validated descriptor. The decision never depends on bounded or truncated diagnostic text.
    /// 仅当权威独立引用存在、但补水未产出已验证描述符时返回 `true`；该判断绝不依赖有界或
    /// 截断后的诊断文本。
    pub fn custom_provider_manifest_is_isolated(&self, provider_id: &str) -> bool {
        self.custom_provider_file_refs.contains(provider_id)
            && !self.custom_providers.contains_key(provider_id)
    }

    /// Builds an in-memory execution projection that excludes isolated custom providers.
    /// 构建一份排除已隔离自定义供应商的内存执行投影。
    ///
    /// The persisted configuration remains the repair authority. This projection removes only
    /// executable/provider-owned references from a clone, allowing healthy providers to start and
    /// remain usable without deleting the failed provider's connection or source bytes.
    /// 持久化配置仍是修复权威。该投影只从克隆中移除可执行及供应商自有引用，使健康供应商
    /// 可正常启动和使用，同时不删除失败供应商的连接或源字节。
    ///
    /// # Returns
    /// 返回值
    /// A detached runtime-only configuration whose remaining references are internally coherent.
    /// 返回一份独立、仅供运行时使用且剩余引用内部一致的配置。
    pub fn without_isolated_custom_provider_runtime_state(&self) -> Self {
        let isolated_provider_ids = self
            .custom_provider_file_refs
            .iter()
            .filter(|provider_id| !self.custom_providers.contains_key(provider_id.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>();
        self.without_provider_runtime_state(&isolated_provider_ids)
    }

    /// Builds an in-memory execution projection excluding an explicit provider identity set.
    /// 构建一份排除显式供应商身份集合的内存执行投影。
    ///
    /// The persisted configuration remains untouched. All provider-owned model settings,
    /// connections, account preferences, validation state, access policy, secondary routes, and
    /// tool selections are removed coherently from the clone.
    /// 持久化配置保持不变。全部供应商自有模型设置、连接、账号偏好、校验状态、访问策略、
    /// 二级路由与工具选择会从克隆中一致移除。
    ///
    /// # Parameters
    /// 参数
    /// `isolated_provider_ids` contains exact provider identities that have no executable runtime
    /// authority in the target manifest generation.
    /// `isolated_provider_ids` 包含目标 Manifest 代际中没有可执行运行时权威的精确供应商身份。
    ///
    /// # Returns
    /// 返回值
    /// A detached internally coherent runtime-only configuration.
    /// 返回一份独立且内部一致的仅运行时配置。
    pub fn without_provider_runtime_state(&self, isolated_provider_ids: &BTreeSet<String>) -> Self {
        if isolated_provider_ids.is_empty() {
            return self.clone();
        }

        // projection is detached so provider-owned cleanup cannot alter the persisted repair image.
        // projection 是独立克隆，因此供应商自有清理不会改变持久化修复镜像。
        let mut projection = self.clone();
        let removed_connection_ids = projection
            .connection_graph
            .connections
            .values()
            .filter(|connection| isolated_provider_ids.contains(connection.provider_id.as_str()))
            .map(|connection| connection.id.clone())
            .collect::<BTreeSet<_>>();
        for provider_id in isolated_provider_ids {
            projection.remove_provider_owned_model_settings(provider_id.as_str());
        }
        projection
            .connection_graph
            .connections
            .retain(|_, connection| {
                !isolated_provider_ids.contains(connection.provider_id.as_str())
            });
        // retainedConnectionIds closes every account preference over the projected graph only.
        // retainedConnectionIds 使每项账号偏好仅引用投影后的连接图。
        let retained_connection_ids = projection
            .connection_graph
            .connections
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        projection
            .connection_graph
            .account_preferences
            .provider_defaults
            .retain(|provider_id, connection_id| {
                !isolated_provider_ids.contains(provider_id.as_str())
                    && retained_connection_ids.contains(connection_id.as_str())
            });
        projection
            .connection_graph
            .account_preferences
            .model_last_connections
            .retain(|_, connection_id| retained_connection_ids.contains(connection_id.as_str()));
        projection
            .pending_validation_connection_ids
            .retain(|connection_id| !removed_connection_ids.contains(connection_id.as_str()));
        projection
            .connection_model_access_policies
            .retain(|connection_id, _| !removed_connection_ids.contains(connection_id.as_str()));
        projection.provider_tool_selections.retain(|_, selection| {
            !removed_connection_ids.contains(selection.connection_id.as_str())
        });
        for connection_id in &removed_connection_ids {
            let connection_model_prefix = format!("{}:{connection_id}:", connection_id.len());
            projection
                .custom_model_connection_enabled_overrides
                .retain(|key, _| !key.starts_with(connection_model_prefix.as_str()));
            projection
                .secondary_routes
                .retain(|key, _| !key.starts_with(connection_model_prefix.as_str()));
        }
        projection
    }

    /// Migrates the exact supported historical schema to the current version.
    /// 将精确支持的历史 Schema 迁移到当前版本。
    ///
    /// # Returns
    /// 返回值
    /// Whether the configuration changed, or an explicit unsupported-version error.
    /// 返回配置是否变化，或明确的不支持版本错误。
    pub fn migrate_to_current_schema(&mut self) -> Result<bool, String> {
        let changed = match self.schema_version {
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION => {
                // normalized records whether a current-version file still carried ignored legacy output fields.
                // normalized 记录当前版本文件是否仍携带已忽略的旧版输出字段。
                let normalized = self.normalize_direct_provider_model_settings();
                // inserted_vmm records whether the required VMM profile was repaired.
                // inserted_vmm 记录是否补齐了必需的 VMM 配置档。
                let inserted_vmm = if self.inference_profiles.contains_key("vmm") {
                    false
                } else {
                    self.inference_profiles
                        .insert(String::from("vmm"), InferenceProfile::default());
                    true
                };
                Ok(normalized || inserted_vmm)
            }
            10..=12 => {
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            9 => {
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            8 => {
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            7 => {
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            6 => {
                self.normalize_direct_provider_model_settings();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            3..=5 => {
                self.migrate_legacy_custom_model_capabilities();
                self.normalize_direct_provider_model_settings();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            2 => {
                self.migrate_legacy_custom_model_capabilities();
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
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
                self.migrate_legacy_custom_model_capabilities();
                self.normalize_direct_provider_model_settings();
                self.inference_profiles
                    .entry(String::from("vmm"))
                    .or_default();
                self.schema_version = PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION;
                Ok(true)
            }
            unsupported => Err(format!(
                "Unsupported persisted AI configuration schema version {unsupported}; supported version is {PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION}."
            )),
        }?;
        self.billing.validate()?;
        self.validate_context_window_policy_layers()?;
        Ok(changed)
    }

    /// Removes legacy non-text output settings and collapses records equal to the system baseline.
    /// 移除旧版非文本输出设置，并折叠与系统基线完全相同的记录。
    ///
    /// # Returns
    /// 返回值
    /// `true` when at least one persisted record changed.
    /// 当至少一项持久化记录发生变化时返回 `true`。
    fn normalize_direct_provider_model_settings(&mut self) -> bool {
        // changed tracks whether normalization must be persisted by the caller.
        // changed 跟踪调用方是否需要持久化规范化结果。
        let mut changed = false;
        for defaults in self.custom_provider_model_defaults.values_mut() {
            if !defaults.output_resources.is_empty() {
                defaults.output_resources.clear();
                changed = true;
            }
        }
        for models in self.custom_models.values_mut() {
            for model in models.values_mut() {
                if !model.capabilities.output_resources.is_empty() {
                    model.capabilities.output_resources.clear();
                    changed = true;
                }
            }
        }
        for decision in self.custom_model_template_decisions.values_mut() {
            if let CustomModelTemplateDecision::Applied { selection } = decision
                && !selection.output_resources.is_empty()
            {
                selection.output_resources.clear();
                changed = true;
            }
        }
        // default_provider_count captures entries that represent no user-owned provider layer.
        // default_provider_count 统计不代表用户供应商层设置的系统默认记录。
        let default_provider_count = self
            .custom_provider_model_defaults
            .values()
            .filter(|defaults| **defaults == CustomProviderModelDefaults::default())
            .count();
        if default_provider_count > 0 {
            self.custom_provider_model_defaults
                .retain(|_, defaults| *defaults != CustomProviderModelDefaults::default());
            changed = true;
        }
        for overrides in self.model_capability_overrides.values_mut() {
            if overrides.output_resources.take().is_some() {
                changed = true;
            }
        }
        for overrides in self.provider_capability_overrides.values_mut() {
            if overrides.output_resources.take().is_some() {
                changed = true;
            }
        }
        // legacy_limits merges protocol-specific limits into their exact provider-model scope.
        // legacy_limits 将协议专属限制合并到其精确供应商模型作用域。
        let mut legacy_limits = BTreeMap::<String, (Option<u64>, Option<u64>)>::new();
        for (key, overrides) in &self.model_overrides {
            if overrides.context_limit.is_none() && overrides.output_limit.is_none() {
                continue;
            }
            let Some(model_key) = provider_model_key_from_protocol_key(key) else {
                continue;
            };
            let entry = legacy_limits.entry(model_key).or_default();
            entry.0 = conservative_legacy_limit(entry.0, overrides.context_limit);
            entry.1 = conservative_legacy_limit(entry.1, overrides.output_limit);
        }
        for (model_key, (legacy_context, legacy_output)) in legacy_limits {
            let overrides = self
                .model_capability_overrides
                .entry(model_key)
                .or_default();
            // context_was_missing distinguishes migrated data from an existing direct model choice.
            // context_was_missing 区分迁移数据与既有直接模型选择。
            let context_was_missing = overrides.context_limit.is_none();
            // output_was_missing distinguishes migrated data from an existing direct model choice.
            // output_was_missing 区分迁移数据与既有直接模型选择。
            let output_was_missing = overrides.output_limit.is_none();
            if context_was_missing {
                overrides.context_limit = legacy_context;
                changed |= legacy_context.is_some();
            }
            if output_was_missing {
                overrides.output_limit = legacy_output.map(|output| {
                    overrides
                        .context_limit
                        .map_or(output, |context| output.min(context))
                });
                changed |= legacy_output.is_some();
            }
            if context_was_missing
                && !output_was_missing
                && let (Some(context), Some(output)) =
                    (overrides.context_limit, overrides.output_limit)
                && output > context
            {
                overrides.context_limit = Some(output);
                changed = true;
            }
        }
        for overrides in self.model_overrides.values_mut() {
            if overrides.context_limit.take().is_some() {
                changed = true;
            }
            if overrides.output_limit.take().is_some() {
                changed = true;
            }
        }
        // empty_execution_count captures protocol records that held only deprecated model limits.
        // empty_execution_count 统计仅包含已弃用模型限制的协议记录。
        let empty_execution_count = self
            .model_overrides
            .values()
            .filter(|overrides| overrides.is_empty())
            .count();
        if empty_execution_count > 0 {
            self.model_overrides
                .retain(|_, overrides| !overrides.is_empty());
            changed = true;
        }
        // empty_model_count captures records that became empty after removing legacy output settings.
        // empty_model_count 统计移除旧版输出设置后变为空的模型记录。
        let empty_model_count = self
            .model_capability_overrides
            .values()
            .filter(|overrides| overrides.is_empty())
            .count();
        if empty_model_count > 0 {
            self.model_capability_overrides
                .retain(|_, overrides| !overrides.is_empty());
            changed = true;
        }
        changed
    }

    /// Migrates legacy complete custom-model capabilities into explicit field differences.
    /// 将旧版完整自定义模型能力迁移为显式字段差异。
    fn migrate_legacy_custom_model_capabilities(&mut self) {
        let provider_ids = self.custom_models.keys().cloned().collect::<Vec<_>>();
        for provider_id in provider_ids {
            let defaults = self
                .custom_provider_model_defaults
                .entry(provider_id.clone())
                .or_default()
                .clone();
            let migrated_differences = self
                .custom_models
                .get_mut(provider_id.as_str())
                .into_iter()
                .flat_map(|models| models.values_mut())
                .filter_map(|model| {
                    let differences = ModelCapabilityOverrides {
                        context_limit: model
                            .capabilities
                            .context_limit
                            .filter(|value| *value != defaults.context_limit),
                        output_limit: model
                            .capabilities
                            .output_limit
                            .filter(|value| *value != defaults.output_limit),
                        input_resources: (model.capabilities.input_resources
                            != defaults.input_resources)
                            .then(|| model.capabilities.input_resources.clone()),
                        output_resources: None,
                        ..ModelCapabilityOverrides::default()
                    };
                    // Supported historical schemas stored resolved basic capabilities inside each
                    // model. Clear those fields after extracting differences so later
                    // provider-default edits are not shadowed by a duplicate historical snapshot.
                    // 受支持的历史 Schema 将解析后的基础能力存入每个模型。提取差异后清除这些字段，
                    // 避免后续供应商默认值变更被重复的历史快照遮蔽。
                    model.capabilities.context_limit = None;
                    model.capabilities.output_limit = None;
                    model.capabilities.input_resources.clear();
                    model.capabilities.output_resources.clear();
                    (!differences.is_empty()).then(|| {
                        (
                            Self::provider_model_key(provider_id.as_str(), model.id.as_str()),
                            differences,
                        )
                    })
                })
                .collect::<Vec<_>>();
            for (key, differences) in migrated_differences {
                self.model_capability_overrides
                    .entry(key)
                    .or_insert(differences);
            }
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

    /// Parses one canonical provider-model-protocol key without guessing delimiters inside identifiers.
    /// 解析一个规范供应商-模型-协议键，且不会猜测标识符内部的分隔符。
    ///
    /// # Parameters
    /// 参数
    /// `key` is a length-prefixed key produced by `provider_model_protocol_key`.
    /// `key` 是由 `provider_model_protocol_key` 生成的长度前缀键。
    ///
    /// # Returns
    /// 返回值
    /// The exact provider, model, and protocol slices, or `None` for a malformed key.
    /// 返回精确的供应商、模型与协议切片；键格式错误时返回 `None`。
    pub fn provider_model_protocol_key_parts(key: &str) -> Option<(&str, &str, &str)> {
        let (provider_id, remainder) = take_length_prefixed_segment(key)?;
        let (model_id, remainder) = take_length_prefixed_segment(remainder.strip_prefix(':')?)?;
        let (protocol_id, remainder) = take_length_prefixed_segment(remainder.strip_prefix(':')?)?;
        if !remainder.is_empty() {
            return None;
        }
        Some((provider_id, model_id, protocol_id))
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

    /// Parses one canonical provider-model key without guessing delimiters inside identifiers.
    /// 解析一个规范供应商模型键，且不会猜测标识符内部的分隔符。
    ///
    /// # Parameters
    /// 参数
    /// `key` is a length-prefixed key produced by `provider_model_key`.
    /// `key` 是由 `provider_model_key` 生成的长度前缀键。
    ///
    /// # Returns
    /// 返回值
    /// The exact provider and model slices, or `None` for a malformed key.
    /// 返回精确的供应商与模型切片；键格式错误时返回 `None`。
    pub fn provider_model_key_parts(key: &str) -> Option<(&str, &str)> {
        let (provider_id, remainder) = take_length_prefixed_segment(key)?;
        let (model_id, remainder) = take_length_prefixed_segment(remainder.strip_prefix(':')?)?;
        if !remainder.is_empty() {
            return None;
        }
        Some((provider_id, model_id))
    }

    /// Returns one user-owned custom model by exact provider and model identifiers.
    /// 按精确供应商与模型标识返回一个用户自有自定义模型。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify one exact custom model.
    /// `provider_id` 与 `model_id` 标识一个精确自定义模型。
    ///
    /// # Returns
    /// 返回值
    /// The stored definition when present.
    /// 存在时返回已保存定义。
    pub fn custom_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<&CustomModelDefinition> {
        self.custom_models
            .get(provider_id)
            .and_then(|models| models.get(model_id))
    }

    /// Returns the user capability differences for one exact provider model.
    /// 返回一个精确供应商模型的用户能力差异。
    pub fn model_capability_overrides(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<&ModelCapabilityOverrides> {
        self.model_capability_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
    }

    /// Returns one explicit provider-level capability layer when it exists.
    /// 返回一个存在时的显式供应商级能力层。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider whose models inherit this layer.
    /// `provider_id` 标识继承此层的精确供应商。
    ///
    /// # Returns
    /// 返回值
    /// The sparse provider-level difference, or `None` when the provider inherits catalog defaults.
    /// 返回稀疏供应商级差异；供应商继承目录默认值时返回 `None`。
    pub fn provider_capability_overrides(
        &self,
        provider_id: &str,
    ) -> Option<&ModelCapabilityOverrides> {
        self.provider_capability_overrides.get(provider_id)
    }

    /// Returns the merged sparse window policy for one exact provider-model route.
    /// 返回一条精确供应商模型路由合并后的稀疏窗口策略。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the route whose layer precedence is being resolved.
    /// `provider_id` 与 `model_id` 标识正在解析分层优先级的路由。
    ///
    /// # Returns
    /// 返回值
    /// The sparse policy after applying built-in, system, provider, and model layers.
    /// 应用内置、系统、供应商与模型层之后的稀疏策略。
    fn context_window_policy_sparse_for_route(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> ContextWindowPolicyOverrides {
        let provider_overrides = self
            .provider_context_window_policy_overrides
            .get(provider_id)
            .copied()
            .unwrap_or_default();
        let provider_capability_policy = self
            .provider_capability_overrides(provider_id)
            .and_then(|overrides| overrides.context_window_policy)
            .unwrap_or_default();
        let custom_provider_policy = self
            .custom_provider_model_defaults
            .get(provider_id)
            .map(|defaults| defaults.context_window_policy)
            .unwrap_or_default();
        let model_overrides = self
            .model_context_window_policy_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
            .unwrap_or_default();
        let model_capability_policy = self
            .model_capability_overrides(provider_id, model_id)
            .and_then(|overrides| overrides.context_window_policy)
            .unwrap_or_default();
        ContextWindowPolicyOverrides::default()
            .merged_with(self.context_window_policy_overrides)
            .merged_with(custom_provider_policy)
            .merged_with(provider_overrides)
            .merged_with(provider_capability_policy)
            .merged_with(model_overrides)
            .merged_with(model_capability_policy)
    }

    /// Validates every persisted context-window layer and every known inherited route.
    /// 校验所有已持久化上下文窗口层以及所有已知的继承路由。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when no resolved route violates the Codex window ordering rules.
    /// 当所有解析路由都符合 Codex 窗口顺序规则时返回 `Ok(())`。
    pub fn validate_context_window_policy_layers(&self) -> Result<(), String> {
        self.context_window_policy_overrides.validate()?;
        for overrides in self.provider_context_window_policy_overrides.values() {
            overrides.validate()?;
        }
        for overrides in self.model_context_window_policy_overrides.values() {
            overrides.validate()?;
        }
        for overrides in self.provider_capability_overrides.values() {
            if let Some(policy) = overrides.context_window_policy {
                policy.validate()?;
            }
        }
        for overrides in self.model_capability_overrides.values() {
            if let Some(policy) = overrides.context_window_policy {
                policy.validate()?;
            }
        }
        for defaults in self.custom_provider_model_defaults.values() {
            defaults.context_window_policy.validate()?;
        }

        let mut routes = BTreeSet::<(String, String)>::new();
        routes.insert((String::new(), String::new()));
        for provider_id in self
            .provider_context_window_policy_overrides
            .keys()
            .chain(self.provider_capability_overrides.keys())
            .chain(self.custom_provider_model_defaults.keys())
        {
            routes.insert((provider_id.clone(), String::new()));
        }
        for (provider_id, models) in &self.custom_models {
            for model_id in models.keys() {
                routes.insert((provider_id.clone(), model_id.clone()));
            }
        }
        for key in self
            .model_context_window_policy_overrides
            .keys()
            .chain(self.model_capability_overrides.keys())
        {
            let Some((provider_id, model_id)) = Self::provider_model_key_parts(key) else {
                return Err(format!(
                    "Malformed provider-model context window policy key `{key}`."
                ));
            };
            routes.insert((provider_id.to_string(), model_id.to_string()));
        }

        for (provider_id, model_id) in routes {
            let policy = self
                .context_window_policy_sparse_for_route(provider_id.as_str(), model_id.as_str())
                .resolve();
            policy.validate().map_err(|error| {
                let scope = if provider_id.is_empty() {
                    String::from("system")
                } else if model_id.is_empty() {
                    format!("provider `{provider_id}`")
                } else {
                    format!("provider `{provider_id}` model `{model_id}`")
                };
                format!("Context window policy for {scope} is invalid: {error}")
            })?;
        }
        Ok(())
    }

    /// Resolves one complete Codex-compatible compaction policy for one execution turn.
    /// 为一次执行回合解析完整的 Codex 兼容压缩策略。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the immutable route selected for the turn.
    /// `provider_id` 与 `model_id` 标识本回合选定的不可变路由。
    /// `context_limit` is the raw model-declared total context window.
    /// `context_limit` 是模型原始声明的总上下文窗口。
    /// `session_policy` is either a fully frozen session snapshot or a behavior-only child-task override.
    /// `session_policy` 可以是完整冻结的会话快照，也可以是仅覆盖子任务行为的策略。
    ///
    /// # Returns
    /// 返回值
    /// A complete policy frozen for the current execution turn.
    /// 为当前执行回合冻结的一份完整策略。
    pub fn resolve_context_compaction_policy(
        &self,
        provider_id: &str,
        model_id: &str,
        context_limit: Option<usize>,
        session_policy: Option<ContextCompactionPolicy>,
    ) -> ContextCompactionPolicy {
        let sparse = self.context_window_policy_sparse_for_route(provider_id, model_id);
        let resolved_window = sparse.resolve();
        let resolved_tail = vulcan_context::resolve_context_compaction_tail_policy(context_limit);
        if let Some(session_policy) = session_policy {
            if session_policy.window_policy_is_explicit {
                return ContextCompactionPolicy {
                    tail_turns: resolved_tail.tail_segments,
                    preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
                    tail_context_limit: resolved_tail.context_limit,
                    tail_policy_source: resolved_tail.source,
                    ..session_policy
                };
            }
            return ContextCompactionPolicy {
                tail_turns: resolved_tail.tail_segments,
                preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
                tail_context_limit: resolved_tail.context_limit,
                tail_policy_source: resolved_tail.source,
                window: resolved_window,
                window_policy_is_explicit: true,
                ..session_policy
            };
        }
        ContextCompactionPolicy {
            tail_turns: resolved_tail.tail_segments,
            preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
            tail_context_limit: resolved_tail.context_limit,
            tail_policy_source: resolved_tail.source,
            window: resolved_window,
            window_policy_is_explicit: true,
            ..ContextCompactionPolicy::default()
        }
    }

    /// Saves or clears the system-level sparse context-window policy.
    /// 保存或清除系统级稀疏上下文窗口策略。
    pub fn set_context_window_policy_overrides(
        &mut self,
        overrides: ContextWindowPolicyOverrides,
    ) -> Result<(), String> {
        overrides.validate()?;
        let mut candidate = self.clone();
        candidate.context_window_policy_overrides = overrides;
        candidate.validate_context_window_policy_layers()?;
        self.context_window_policy_overrides = overrides;
        Ok(())
    }

    /// Saves or clears one provider-level sparse context-window policy.
    /// 保存或清除一条供应商级稀疏上下文窗口策略。
    pub fn set_provider_context_window_policy_overrides(
        &mut self,
        provider_id: &str,
        overrides: ContextWindowPolicyOverrides,
    ) -> Result<(), String> {
        overrides.validate()?;
        let mut candidate = self.clone();
        if overrides.is_empty() {
            candidate
                .provider_context_window_policy_overrides
                .remove(provider_id);
        } else {
            candidate
                .provider_context_window_policy_overrides
                .insert(provider_id.to_string(), overrides);
        }
        candidate.validate_context_window_policy_layers()?;
        if overrides.is_empty() {
            self.provider_context_window_policy_overrides
                .remove(provider_id);
        } else {
            self.provider_context_window_policy_overrides
                .insert(provider_id.to_string(), overrides);
        }
        Ok(())
    }

    /// Saves or clears one exact provider-model sparse context-window policy.
    /// 保存或清除一条精确供应商模型级稀疏上下文窗口策略。
    pub fn set_model_context_window_policy_overrides(
        &mut self,
        provider_id: &str,
        model_id: &str,
        overrides: ContextWindowPolicyOverrides,
    ) -> Result<(), String> {
        overrides.validate()?;
        let key = Self::provider_model_key(provider_id, model_id);
        let mut candidate = self.clone();
        if overrides.is_empty() {
            candidate
                .model_context_window_policy_overrides
                .remove(key.as_str());
        } else {
            candidate
                .model_context_window_policy_overrides
                .insert(key, overrides);
        }
        candidate.validate_context_window_policy_layers()?;
        let key = Self::provider_model_key(provider_id, model_id);
        if overrides.is_empty() {
            self.model_context_window_policy_overrides
                .remove(key.as_str());
        } else {
            self.model_context_window_policy_overrides
                .insert(key, overrides);
        }
        Ok(())
    }

    /// Returns provider defaults for one custom provider, falling back to safe built-in values.
    /// 返回一个自定义供应商的默认值，缺失时回退到安全内置值。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies one exact custom provider.
    /// `provider_id` 标识一个精确自定义供应商。
    ///
    /// # Returns
    /// 返回值
    /// Persisted defaults or the text-first 200K/32K defaults.
    /// 返回已持久化默认值或纯文本优先的 200K/32K 默认值。
    pub fn custom_provider_model_defaults(&self, provider_id: &str) -> CustomProviderModelDefaults {
        self.custom_provider_model_defaults
            .get(provider_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns the reviewed template decision for one custom-provider model.
    /// 返回一个自定义供应商模型的已审查模板决策。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify one exact model.
    /// `provider_id` 与 `model_id` 标识一个精确模型。
    ///
    /// # Returns
    /// 返回值
    /// The persisted decision when present.
    /// 存在时返回持久化决策。
    pub fn custom_model_template_decision(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<&CustomModelTemplateDecision> {
        self.custom_model_template_decisions
            .get(Self::provider_model_key(provider_id, model_id).as_str())
    }

    /// Returns an explicit connection-scoped custom-model availability choice.
    /// 返回显式连接作用域自定义模型可用性选择。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` identify one exact account-model pair.
    /// `connection_id` 与 `model_id` 标识一个精确账号模型组合。
    ///
    /// # Returns
    /// 返回值
    /// The explicit choice when the user created one.
    /// 用户已创建时返回显式选择。
    pub fn custom_model_connection_enabled_override(
        &self,
        connection_id: &str,
        model_id: &str,
    ) -> Option<bool> {
        self.custom_model_connection_enabled_overrides
            .get(Self::connection_model_key(connection_id, model_id).as_str())
            .copied()
    }

    /// Returns one account model access policy, falling back to the enabled default.
    /// 返回一个账号模型访问策略，缺失时回退到默认启用策略。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies one exact credential account.
    /// `connection_id` 标识一个精确凭据账号。
    ///
    /// # Returns
    /// 返回值
    /// The persisted policy or a safe in-memory default.
    /// 返回持久化策略或安全的内存默认值。
    pub fn connection_model_access_policy(
        &self,
        connection_id: &str,
    ) -> ConnectionModelAccessPolicy {
        self.connection_model_access_policies
            .get(connection_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Reports whether one exact account model is enabled by user policy.
    /// 报告一个精确账号模型是否被用户策略启用。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` identify one exact account-model pair.
    /// `connection_id` 与 `model_id` 标识一个精确账号模型组合。
    ///
    /// # Returns
    /// 返回值
    /// The sparse account decision or the account default.
    /// 返回稀疏账号决定或账号默认值。
    pub fn connection_model_is_enabled(&self, connection_id: &str, model_id: &str) -> bool {
        self.connection_model_access_policies
            .get(connection_id)
            .is_none_or(|policy| policy.model_is_enabled(model_id))
    }

    /// Saves one exact account-model enabled decision using sparse persistence.
    /// 使用稀疏持久化保存一个精确账号模型启用决定。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id`, `model_id`, and `enabled` identify the exact user decision.
    /// `connection_id`、`model_id` 与 `enabled` 标识精确用户决定。
    pub fn set_connection_model_enabled(
        &mut self,
        connection_id: &str,
        model_id: &str,
        enabled: bool,
    ) {
        let policy = self
            .connection_model_access_policies
            .entry(connection_id.to_string())
            .or_default();
        policy.set_model_enabled(model_id, enabled);
        if *policy == ConnectionModelAccessPolicy::default() {
            self.connection_model_access_policies.remove(connection_id);
        }
    }

    /// Sets the account-wide model default and clears obsolete per-model differences.
    /// 设置账号级模型默认值并清理过时的逐模型差异。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies one credential and `enabled` is the new inherited state.
    /// `connection_id` 标识一个凭据，`enabled` 是新的继承状态。
    pub fn set_connection_models_enabled(&mut self, connection_id: &str, enabled: bool) {
        if enabled {
            self.connection_model_access_policies.remove(connection_id);
            return;
        }
        let policy = self
            .connection_model_access_policies
            .entry(connection_id.to_string())
            .or_default();
        policy.set_all_models_enabled(false);
    }

    /// Removes every model access decision owned by one deleted credential account.
    /// 移除一个已删除凭据账号拥有的全部模型访问决定。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the deleted credential account.
    /// `connection_id` 标识已删除凭据账号。
    pub fn remove_connection_model_access_policy(&mut self, connection_id: &str) {
        self.connection_model_access_policies.remove(connection_id);
    }

    /// Reports whether one connection is isolated pending explicit model validation.
    /// 报告一个连接是否因等待显式模型验证而处于隔离状态。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies one exact persisted credential connection.
    /// `connection_id` 标识一个精确持久化凭据连接。
    ///
    /// # Returns
    /// 返回值
    /// `true` only while the connection must remain unavailable to normal routing.
    /// 仅当该连接必须对常规路由保持不可用时返回 `true`。
    pub fn connection_is_pending_validation(&self, connection_id: &str) -> bool {
        self.pending_validation_connection_ids
            .contains(connection_id)
    }

    /// Changes the isolated validation state for one configured connection.
    /// 修改一个已配置连接的隔离验证状态。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies one exact configured connection and `pending` is the target state.
    /// `connection_id` 标识一个精确已配置连接，`pending` 是目标状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns an error when the connection does not exist.
    /// 当连接不存在时返回错误。
    pub fn set_connection_pending_validation(
        &mut self,
        connection_id: &str,
        pending: bool,
    ) -> Result<(), String> {
        if !self
            .connection_graph
            .connections
            .contains_key(connection_id)
        {
            return Err(format!("Connection `{connection_id}` is not configured."));
        }
        if pending {
            self.pending_validation_connection_ids
                .insert(connection_id.to_string());
            self.connection_graph
                .account_preferences
                .provider_defaults
                .retain(|_, value| value != connection_id);
            self.connection_graph
                .account_preferences
                .model_last_connections
                .retain(|_, value| value != connection_id);
            if self
                .active
                .as_ref()
                .is_some_and(|active| active.connection_id == connection_id)
            {
                self.active = None;
            }
        } else {
            self.pending_validation_connection_ids.remove(connection_id);
        }
        Ok(())
    }

    /// Applies provider defaults only to fields omitted by the model source evidence.
    /// 仅将供应商默认应用到模型来源证据未声明的字段。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the provider layer, `evidence` identifies explicit directory fields,
    /// and `capabilities` is mutated before source fallbacks are filled.
    /// `provider_id` 选择供应商层，`evidence` 标识目录明确字段，`capabilities` 会在来源回退填充前被修改。
    pub fn apply_provider_capability_defaults(
        &self,
        provider_id: &str,
        evidence: &crate::ModelCapabilityEvidence,
        capabilities: &mut ModelCapabilities,
    ) {
        let Some(overrides) = self.provider_capability_overrides(provider_id) else {
            return;
        };
        if !evidence.context_limit_declared
            && capabilities.context_limit.is_none()
            && let Some(context_limit) = overrides.context_limit
        {
            capabilities.context_limit = Some(context_limit);
        }
        if !evidence.output_limit_declared
            && capabilities.output_limit.is_none()
            && let Some(output_limit) = overrides.output_limit
        {
            capabilities.output_limit = Some(output_limit);
        }
        if !evidence.input_modalities_declared
            && capabilities.input_resources.is_empty()
            && let Some(input_resources) = overrides.input_resources.as_ref()
        {
            capabilities.input_resources = input_resources.clone();
        }
        // Reasoning fields are resolved independently because a directory may declare support
        // while omitting its control mode or selectable levels.
        // 思考字段按字段独立解析，因为目录可能声明支持却省略控制模式或可选档位。
        if capabilities.reasoning == CapabilitySupport::Unknown
            && let Some(reasoning) = overrides.reasoning
        {
            capabilities.reasoning = reasoning;
        }
        if capabilities.reasoning_control == ReasoningControlMode::Unknown
            && let Some(reasoning_control) = overrides.reasoning_control
        {
            capabilities.reasoning_control = reasoning_control;
        }
        if capabilities.reasoning_efforts.is_empty()
            && let Some(reasoning_efforts) = overrides.reasoning_efforts.as_ref()
        {
            capabilities.reasoning_efforts = reasoning_efforts.clone();
        }
    }

    /// Applies one model-level capability layer above the provider layer.
    /// 应用位于供应商层之上的模型级能力层。
    pub fn apply_model_capability_overrides(
        &self,
        provider_id: &str,
        model_id: &str,
        capabilities: &mut ModelCapabilities,
    ) {
        let Some(overrides) = self.model_capability_overrides(provider_id, model_id) else {
            return;
        };
        apply_capability_overrides(overrides, capabilities);
    }

    /// Removes non-route settings owned by one model after reference validation succeeds.
    /// 在引用校验成功后移除一个模型持有的非路由设置。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the removed model exactly.
    /// `provider_id` 与 `model_id` 精确标识被移除模型。
    pub fn remove_model_owned_settings(&mut self, provider_id: &str, model_id: &str) {
        let provider_model_key = Self::provider_model_key(provider_id, model_id);
        let provider_model_prefix = format!("{provider_model_key}:");
        self.model_enabled_overrides
            .remove(provider_model_key.as_str());
        self.model_capability_overrides
            .remove(provider_model_key.as_str());
        self.model_context_window_policy_overrides
            .remove(provider_model_key.as_str());
        self.custom_model_template_decisions
            .remove(provider_model_key.as_str());
        self.custom_model_protocol_preferences
            .remove(provider_model_key.as_str());
        self.connection_graph
            .account_preferences
            .model_last_connections
            .remove(provider_model_key.as_str());
        self.model_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.internal_model_enabled_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));

        let connection_ids = self
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| connection.id.clone())
            .collect::<Vec<_>>();
        for connection_id in connection_ids {
            self.secondary_routes
                .remove(Self::connection_model_key(connection_id.as_str(), model_id).as_str());
            self.custom_model_connection_enabled_overrides
                .remove(Self::connection_model_key(connection_id.as_str(), model_id).as_str());
        }
    }

    /// Reports whether a provider still owns any user-customized model or route state.
    /// 报告供应商是否仍持有任何用户自定义模型或路由状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider whose deletion is being checked.
    /// `provider_id` 标识当前正在检查是否删除的精确供应商。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the removal warning must disclose user-owned model or route information.
    /// 如果移除提示必须披露用户持有的模型或路由信息，则返回 `true`。
    pub fn provider_has_model_customizations(&self, provider_id: &str) -> bool {
        if self
            .custom_models
            .get(provider_id)
            .is_some_and(|models| !models.is_empty())
            || self
                .custom_provider_model_defaults
                .contains_key(provider_id)
            || self.provider_capability_overrides.contains_key(provider_id)
            || self
                .provider_context_window_policy_overrides
                .contains_key(provider_id)
        {
            return true;
        }

        let provider_model_prefix = format!("{}:{provider_id}:", provider_id.len());
        if self
            .custom_model_template_decisions
            .keys()
            .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .custom_model_protocol_preferences
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .model_enabled_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .model_capability_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .model_context_window_policy_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .model_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .internal_model_enabled_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .internal_model_overrides
                .keys()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
        {
            return true;
        }

        let provider_connection_ids = self
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| connection.id.as_str())
            .collect::<BTreeSet<_>>();
        let connection_model_prefixes = provider_connection_ids
            .iter()
            .map(|connection_id| format!("{}:{connection_id}:", connection_id.len()))
            .collect::<Vec<_>>();
        if self
            .custom_model_connection_enabled_overrides
            .keys()
            .chain(self.secondary_routes.keys())
            .any(|key| {
                connection_model_prefixes
                    .iter()
                    .any(|prefix| key.starts_with(prefix.as_str()))
            })
            || self
                .connection_model_access_policies
                .iter()
                .any(|(connection_id, policy)| {
                    provider_connection_ids.contains(connection_id.as_str())
                        && *policy != ConnectionModelAccessPolicy::default()
                })
        {
            return true;
        }

        self.inference_profiles.values().any(|profile| {
            profile
                .llm_routes
                .values()
                .chain(profile.embedding.iter())
                .chain(profile.rerank.iter())
                .any(|route| route.provider_id == provider_id)
        })
    }

    /// Counts protocol-scoped model settings that cannot apply to a selected provider protocol.
    /// 统计无法应用于所选供应商协议的协议作用域模型设置。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the edited provider and `protocol_id` is its selected target route.
    /// `provider_id` 标识正在编辑的供应商，`protocol_id` 是其选定目标路由。
    ///
    /// # Returns
    /// 返回值
    /// The number of exact provider-owned records bound to another protocol.
    /// 返回绑定到其他协议的精确供应商自有记录数量。
    pub fn inactive_provider_model_override_count(
        &self,
        provider_id: &str,
        protocol_id: &str,
    ) -> usize {
        self.model_overrides
            .keys()
            .filter(|key| {
                Self::provider_model_protocol_key_parts(key).is_some_and(
                    |(record_provider_id, _, record_protocol_id)| {
                        record_provider_id == provider_id && record_protocol_id != protocol_id
                    },
                )
            })
            .count()
    }

    /// Removes protocol-scoped model settings that cannot apply after a confirmed protocol switch.
    /// 在已确认协议切换后移除无法继续应用的协议作用域模型设置。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the edited provider and `protocol_id` is the retained target route.
    /// `provider_id` 标识正在编辑的供应商，`protocol_id` 是要保留的目标路由。
    pub fn remove_inactive_provider_model_overrides(
        &mut self,
        provider_id: &str,
        protocol_id: &str,
    ) {
        self.model_overrides.retain(|key, _| {
            Self::provider_model_protocol_key_parts(key).is_none_or(
                |(record_provider_id, _, record_protocol_id)| {
                    record_provider_id != provider_id || record_protocol_id == protocol_id
                },
            )
        });
    }

    /// Counts protocol-scoped model settings owned by protocols removed from an edited provider.
    /// 统计被编辑供应商移除协议所拥有的协议作用域模型设置数量。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the edited provider and `removed_protocols` is the exact set of
    /// protocols the edit deletes. New protocols never invalidate existing settings, so only
    /// the removed set participates (plan §R6.8).
    /// `provider_id` 标识正在编辑的供应商，`removed_protocols` 是编辑删除的精确协议集合。
    /// 新增协议绝不让既有设置失效，因此只统计被删除集合（方案 §R6.8）。
    ///
    /// # Returns
    /// 返回值
    /// The number of exact provider-owned records bound to a removed protocol.
    /// 返回绑定到被删协议的精确供应商自有记录数量。
    pub fn removed_protocol_model_override_count(
        &self,
        provider_id: &str,
        removed_protocols: &BTreeSet<String>,
    ) -> usize {
        if removed_protocols.is_empty() {
            return 0;
        }
        self.model_overrides
            .keys()
            .filter(|key| {
                Self::provider_model_protocol_key_parts(key).is_some_and(
                    |(record_provider_id, _, record_protocol_id)| {
                        record_provider_id == provider_id
                            && removed_protocols.contains(record_protocol_id)
                    },
                )
            })
            .count()
    }

    /// Removes protocol-scoped model settings owned by protocols removed from an edited provider.
    /// 移除被编辑供应商移除协议所拥有的协议作用域模型设置。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the edited provider and `removed_protocols` is the exact set of
    /// protocols the edit deletes. Only the removed set is cleaned; retained protocols keep
    /// every owned override (plan §R6.8).
    /// `provider_id` 标识正在编辑的供应商，`removed_protocols` 是编辑删除的精确协议集合。
    /// 只清理被删除集合；保留协议及其全部覆盖不被触碰（方案 §R6.8）。
    pub fn remove_removed_protocol_model_overrides(
        &mut self,
        provider_id: &str,
        removed_protocols: &BTreeSet<String>,
    ) {
        if removed_protocols.is_empty() {
            return;
        }
        self.model_overrides.retain(|key, _| {
            Self::provider_model_protocol_key_parts(key).is_none_or(
                |(record_provider_id, _, record_protocol_id)| {
                    record_provider_id != provider_id
                        || !removed_protocols.contains(record_protocol_id)
                },
            )
        });
    }

    /// Removes every persisted model setting and inference route owned by one provider.
    /// 移除一个供应商持有的全部持久化模型设置与推理路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact provider being permanently removed.
    /// `provider_id` 标识将被永久移除的精确供应商。
    pub fn remove_provider_owned_model_settings(&mut self, provider_id: &str) {
        let provider_model_prefix = format!("{}:{provider_id}:", provider_id.len());
        self.custom_models.remove(provider_id);
        self.custom_provider_model_defaults.remove(provider_id);
        self.provider_capability_overrides.remove(provider_id);
        self.provider_context_window_policy_overrides
            .remove(provider_id);
        self.custom_model_template_decisions
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.custom_model_protocol_preferences
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.model_enabled_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.model_capability_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.model_context_window_policy_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.model_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.internal_model_enabled_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        self.internal_model_overrides
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        for profile in self.inference_profiles.values_mut() {
            profile
                .llm_routes
                .retain(|_, route| route.provider_id != provider_id);
            if profile
                .embedding
                .as_ref()
                .is_some_and(|route| route.provider_id == provider_id)
            {
                profile.embedding = None;
            }
            if profile
                .rerank
                .as_ref()
                .is_some_and(|route| route.provider_id == provider_id)
            {
                profile.rerank = None;
            }
        }
        self.connection_graph
            .account_preferences
            .model_last_connections
            .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
        if self
            .active
            .as_ref()
            .is_some_and(|selection| selection.provider_id == provider_id)
        {
            self.active = None;
        }
    }

    /// Removes one custom provider descriptor together with every provider-owned runtime field.
    /// 移除一个自定义供应商描述，并同步清理该供应商持有的全部运行时字段。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact custom provider identity being deleted.
    /// `provider_id` 是正在删除的精确自定义供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The removed descriptor, or `None` when the provider was not configured.
    /// 返回被移除的描述；供应商未配置时返回 `None`。
    pub fn remove_custom_provider_owned_state(
        &mut self,
        provider_id: &str,
    ) -> Option<ProviderDescriptor> {
        self.remove_provider_owned_model_settings(provider_id);
        // The hydrated manifest and its disk reference belong to the same provider transaction.
        // 已补水 Manifest 与其磁盘引用属于同一供应商事务。
        self.custom_provider_manifests.remove(provider_id);
        self.custom_provider_file_refs.remove(provider_id);
        // retainedFailureCount detects whether this exact provider owned one visible isolation record.
        // retainedFailureCount 检测该精确供应商是否持有一条可见隔离记录。
        let retained_failure_count = self.custom_provider_manifest_failures.len();
        self.custom_provider_manifest_failures
            .retain(|failure| failure.provider_id != provider_id);
        let removed = self.custom_providers.remove(provider_id);
        // remainingIsolatedCount is derived from the exact authority set rather than bounded
        // diagnostics, so deleting a visible failure keeps total/truncated metadata coherent.
        // remainingIsolatedCount 从精确权威集合而非有界诊断派生，因此删除可见失败后，
        // 总数与截断元数据仍保持一致。
        let remaining_isolated_count = self
            .custom_provider_file_refs
            .iter()
            .filter(|provider_id| !self.custom_providers.contains_key(provider_id.as_str()))
            .count() as u64;
        self.custom_provider_manifest_failure_total_count = remaining_isolated_count;
        self.custom_provider_manifest_failures_truncated =
            remaining_isolated_count > self.custom_provider_manifest_failures.len() as u64;
        if self.custom_provider_manifest_failures.len() == retained_failure_count
            && remaining_isolated_count == 0
        {
            self.custom_provider_manifest_failures.clear();
        }
        removed
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
    /// The persisted user/manual-model state, defaulting to enabled when no manifest default is
    /// available. System-provider callers must use `model_is_enabled_with_default` or the runtime.
    /// 返回持久化用户/手工模型状态；无清单默认值时默认启用。系统供应商调用方必须使用
    /// `model_is_enabled_with_default` 或运行时。
    pub fn model_is_enabled(&self, provider_id: &str, model_id: &str) -> bool {
        self.model_is_enabled_with_default(provider_id, model_id, true)
    }

    /// Resolves one model's enabled state against an externally owned inherited default.
    /// 根据外部权威持有的继承默认值解析一个模型的启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the exact model; `inherited_default` comes from the
    /// immutable manifest snapshot selected for the operation.
    /// `provider_id` 与 `model_id` 标识精确模型；`inherited_default` 来自本次操作选定的不可变
    /// 清单快照。
    ///
    /// # Returns
    /// 返回值
    /// The explicit user override, then a manual model declaration, then `inherited_default`.
    /// 依次返回显式用户覆盖、手工模型声明或 `inherited_default`。
    pub fn model_is_enabled_with_default(
        &self,
        provider_id: &str,
        model_id: &str,
        inherited_default: bool,
    ) -> bool {
        self.model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
            .unwrap_or_else(|| {
                self.custom_model(provider_id, model_id)
                    .map_or_else(|| inherited_default, |model| model.enabled)
            })
    }

    /// Returns the persisted user-enabled override for one model when present.
    /// 存在时返回一个模型的持久化用户启用覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the exact runtime model.
    /// `provider_id` 与 `model_id` 标识精确运行时模型。
    ///
    /// # Returns
    /// 返回值
    /// The explicit user choice, or `None` when the user never overrode this model.
    /// 返回显式用户选择；用户从未覆盖该模型时返回 `None`。
    pub fn model_enabled_override(&self, provider_id: &str, model_id: &str) -> Option<bool> {
        self.model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
    }

    /// Stores one explicit model enabled choice, including a value equal to its inherited default.
    /// 保存一个明确的模型启用选择，即使该值与继承默认值相同也保留。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `enabled` identify the exact user choice.
    /// `provider_id`、`model_id` 与 `enabled` 标识精确用户选择。
    pub fn set_model_enabled(&mut self, provider_id: &str, model_id: &str, enabled: bool) {
        let key = Self::provider_model_key(provider_id, model_id);
        self.model_enabled_overrides.insert(key, enabled);
    }

    /// Restores one model enabled choice to its inherited provider or model default.
    /// 将一个模型启用选择恢复为继承的供应商或模型默认值。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the exact model layer to remove.
    /// `provider_id` 与 `model_id` 标识要移除的精确模型层。
    pub fn restore_model_enabled(&mut self, provider_id: &str, model_id: &str) {
        self.model_enabled_overrides
            .remove(Self::provider_model_key(provider_id, model_id).as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_provider::{
        CustomProviderAuthentication, CustomProviderDefinition, CustomProviderModelDiscoveryMode,
        CustomProviderProtocol,
    };
    use crate::{
        CUSTOM_MODEL_TEMPLATE_REVISION, CustomModelProtocolPreference, CustomModelTemplateSelection,
    };

    /// Builds one complete custom provider descriptor for configuration-cleanup tests.
    /// 为配置清理测试构建一个完整的自定义供应商描述。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact custom provider identity used by the test.
    /// `provider_id` 是测试使用的精确自定义供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// A valid single-protocol custom provider descriptor.
    /// 返回有效的单协议自定义供应商描述。
    fn custom_provider_descriptor(provider_id: &str) -> ProviderDescriptor {
        CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: provider_id.to_string(),
            base_url: String::from("https://example.com/v1"),
            protocol: Some(CustomProviderProtocol::OpenAiResponses),
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocols: Vec::new(),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("custom provider descriptor should be valid")
    }

    /// Builds one deterministic credential connection for configuration projection tests.
    /// 为配置投影测试构建一个确定性凭据连接。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `provider_id` identify the exact account and owning provider.
    /// `connection_id` 与 `provider_id` 标识精确账号及其所属供应商。
    ///
    /// # Returns
    /// 返回值
    /// A non-secret connection record using the custom provider's standard service entry.
    /// 返回使用自定义供应商标准服务入口的不含秘密连接记录。
    fn custom_provider_connection(
        connection_id: &str,
        provider_id: &str,
    ) -> crate::CredentialConnection {
        crate::CredentialConnection {
            id: connection_id.to_string(),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: crate::CredentialMode::Token,
            credential_id: format!("credential-{connection_id}"),
            display_name: connection_id.to_string(),
            secret_reference: format!("secret-{connection_id}"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([String::from("openai_responses")]),
            selected_protocol: String::from("openai_responses"),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        }
    }

    /// Verifies runtime model catalogs cannot appear in persisted AI JSON.
    /// 验证运行时模型目录不会出现在持久化 AI JSON 中。
    #[test]
    fn persisted_ai_configuration_has_no_runtime_model_catalog() {
        let encoded =
            serde_json::to_string(&PersistedAiConfiguration::default()).expect("must serialize");
        assert!(!encoded.contains("catalog"));
        assert!(!encoded.contains("connection_model_catalog"));
    }

    /// Verifies an isolated custom provider is removed only from a detached runtime projection.
    /// 验证已隔离自定义供应商只会从独立运行时投影中移除。
    #[test]
    fn isolated_custom_provider_runtime_projection_preserves_repair_authority() {
        let healthy_provider_id = "custom:healthy";
        let isolated_provider_id = "custom:isolated";
        let healthy_connection_id = "healthy-connection";
        let isolated_connection_id = "isolated-connection";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_file_refs.extend([
            healthy_provider_id.to_string(),
            isolated_provider_id.to_string(),
        ]);
        configuration.custom_providers.insert(
            healthy_provider_id.to_string(),
            custom_provider_descriptor(healthy_provider_id),
        );
        configuration.connection_graph.connections.extend([
            (
                healthy_connection_id.to_string(),
                custom_provider_connection(healthy_connection_id, healthy_provider_id),
            ),
            (
                isolated_connection_id.to_string(),
                custom_provider_connection(isolated_connection_id, isolated_provider_id),
            ),
        ]);
        configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .extend([
                (
                    healthy_provider_id.to_string(),
                    healthy_connection_id.to_string(),
                ),
                (
                    isolated_provider_id.to_string(),
                    isolated_connection_id.to_string(),
                ),
            ]);
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: isolated_provider_id.to_string(),
            model_id: String::from("isolated-model"),
            connection_id: isolated_connection_id.to_string(),
        });
        configuration.model_enabled_overrides.extend([
            (
                PersistedAiConfiguration::provider_model_key(healthy_provider_id, "healthy-model"),
                true,
            ),
            (
                PersistedAiConfiguration::provider_model_key(
                    isolated_provider_id,
                    "isolated-model",
                ),
                true,
            ),
        ]);

        let projection = configuration.without_isolated_custom_provider_runtime_state();

        assert!(
            configuration
                .custom_provider_file_refs
                .contains(isolated_provider_id)
        );
        assert!(
            configuration
                .connection_graph
                .connections
                .contains_key(isolated_connection_id)
        );
        assert!(configuration.active.is_some());
        assert_eq!(
            projection
                .connection_graph
                .connections
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![healthy_connection_id.to_string()]
        );
        assert_eq!(
            projection
                .connection_graph
                .account_preferences
                .provider_defaults,
            BTreeMap::from([(
                healthy_provider_id.to_string(),
                healthy_connection_id.to_string(),
            )])
        );
        assert!(projection.active.is_none());
        assert!(
            projection
                .custom_provider_file_refs
                .contains(isolated_provider_id)
        );
        assert_eq!(projection.model_enabled_overrides.len(), 1);
        assert!(
            projection.model_enabled_overrides.contains_key(
                PersistedAiConfiguration::provider_model_key(healthy_provider_id, "healthy-model")
                    .as_str()
            )
        );
    }

    /// Verifies explicit system-provider isolation removes only runtime-owned references.
    /// 验证显式系统供应商隔离只移除运行时自有引用。
    #[test]
    fn isolated_system_provider_runtime_projection_preserves_persisted_repair_state() {
        let healthy_provider_id = "openrouter";
        let isolated_provider_id = "deepseek";
        let healthy_connection_id = "openrouter-account";
        let isolated_connection_id = "deepseek-account";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.connection_graph.connections.extend([
            (
                healthy_connection_id.to_string(),
                custom_provider_connection(healthy_connection_id, healthy_provider_id),
            ),
            (
                isolated_connection_id.to_string(),
                custom_provider_connection(isolated_connection_id, isolated_provider_id),
            ),
        ]);
        configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .extend([
                (
                    healthy_provider_id.to_string(),
                    healthy_connection_id.to_string(),
                ),
                (
                    isolated_provider_id.to_string(),
                    isolated_connection_id.to_string(),
                ),
            ]);
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: isolated_provider_id.to_string(),
            model_id: String::from("deepseek-chat"),
            connection_id: isolated_connection_id.to_string(),
        });
        configuration.model_enabled_overrides.extend([
            (
                PersistedAiConfiguration::provider_model_key(
                    healthy_provider_id,
                    "openrouter/auto",
                ),
                true,
            ),
            (
                PersistedAiConfiguration::provider_model_key(isolated_provider_id, "deepseek-chat"),
                true,
            ),
        ]);
        let isolated_provider_ids = BTreeSet::from([isolated_provider_id.to_string()]);

        let projection = configuration.without_provider_runtime_state(&isolated_provider_ids);

        assert!(
            configuration
                .connection_graph
                .connections
                .contains_key(isolated_connection_id),
            "the authoritative image must retain the isolated account for repair"
        );
        assert_eq!(
            projection
                .connection_graph
                .connections
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![healthy_connection_id.to_string()]
        );
        assert!(projection.active.is_none());
        assert_eq!(projection.model_enabled_overrides.len(), 1);
        assert!(
            projection.model_enabled_overrides.contains_key(
                PersistedAiConfiguration::provider_model_key(
                    healthy_provider_id,
                    "openrouter/auto"
                )
                .as_str()
            )
        );
    }

    /// Verifies explicit deletion can remove an isolated reference without a hydrated descriptor.
    /// 验证显式删除可在没有已补水描述符时移除一项隔离引用。
    #[test]
    fn isolated_custom_provider_reference_can_be_explicitly_removed() {
        let provider_id = "custom:isolated-removal";
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert(provider_id.to_string());
        configuration.custom_provider_manifest_failures = vec![CustomProviderManifestFailure {
            provider_id: provider_id.to_string(),
            message: String::from("invalid standalone document"),
        }];
        configuration.custom_provider_manifest_failure_total_count = 1;

        let removed = configuration.remove_custom_provider_owned_state(provider_id);

        assert!(removed.is_none());
        assert!(
            !configuration
                .custom_provider_file_refs
                .contains(provider_id)
        );
        assert!(configuration.custom_provider_manifest_failures.is_empty());
        assert_eq!(
            configuration.custom_provider_manifest_failure_total_count,
            0
        );
        assert!(!configuration.custom_provider_manifest_failures_truncated);
    }

    /// Verifies billing overrides resolve by route, model, then provider precedence.
    /// 验证计费覆盖按路由、模型、Provider 的优先级解析。
    #[test]
    fn billing_price_resolution_uses_exact_scope_precedence() {
        let mut billing = ProviderBillingConfiguration::default();
        billing
            .catalog_model_prices
            .insert(String::from("deepseek-chat"), billing_price("0.25"));
        billing
            .provider_prices
            .insert(String::from("deepseek"), billing_price("1"));
        billing.model_prices.insert(
            provider_billing_model_key("deepseek", "deepseek-reasoner"),
            billing_price("2"),
        );
        billing.route_prices.insert(
            provider_billing_route_key("deepseek", "deepseek-reasoner", "anthropic"),
            billing_price("3"),
        );

        billing
            .validate()
            .expect("billing configuration should be valid");
        assert_eq!(
            billing
                .resolve_price("deepseek", "deepseek-reasoner", Some("anthropic"))
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("3")
        );
        assert_eq!(
            billing
                .resolve_price("deepseek", "deepseek-reasoner", None)
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("2")
        );
        assert_eq!(
            billing
                .resolve_price("deepseek", "deepseek-chat", None)
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("1")
        );
        assert_eq!(
            billing
                .resolve_price("another-provider", "deepseek-chat", None)
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("0.25")
        );
    }

    /// Verifies cc-switch pricing imports exactly while user/provider overrides retain precedence.
    /// 验证 cc-switch 价格被精确导入，且用户与 Provider 覆盖继续保持更高优先级。
    #[test]
    fn cc_switch_model_pricing_import_is_compatible_and_overridable() {
        let mut billing = ProviderBillingConfiguration::default();
        let imported = billing
            .import_cc_switch_model_pricing_json(
                "fixture-v1",
                r#"{
                    "version": 1,
                    "modelsDevSync": {"autoSyncEnabled": false},
                    "models": [{
                        "modelId": "deepseek-chat",
                        "displayName": "DeepSeek Chat",
                        "inputCostPerMillion": "0.27",
                        "outputCostPerMillion": "1.10",
                        "cacheReadCostPerMillion": "0.07",
                        "cacheCreationCostPerMillion": "0"
                    }],
                    "deletedModelIds": []
                }"#,
            )
            .expect("cc-switch pricing should import");
        assert_eq!(imported, 1);
        assert_eq!(
            billing
                .resolve_price("deepseek", "deepseek-chat", None)
                .and_then(|price| price.output_cost_per_million.as_deref()),
            Some("1.10")
        );

        billing
            .provider_prices
            .insert(String::from("deepseek"), billing_price("0.50"));
        assert_eq!(
            billing
                .resolve_price("deepseek", "deepseek-chat", None)
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("0.50")
        );
    }

    /// Verifies the frozen cc-switch seed is complete and remains below user-imported catalog data.
    /// 验证冻结的 cc-switch 种子完整，并且优先级低于用户导入的目录数据。
    #[test]
    fn builtin_cc_switch_pricing_is_complete_and_lowest_precedence() {
        assert_eq!(BUILTIN_CC_SWITCH_MODEL_PRICES.len(), 189);
        let mut billing = ProviderBillingConfiguration::default();
        let (scope, key, builtin) = billing
            .resolve_price_with_scope("openai", "gpt-5-nano", None)
            .expect("frozen cc-switch seed must contain gpt-5-nano");
        assert_eq!(scope, "builtin_catalog_model");
        assert_eq!(key, "gpt-5-nano");
        assert_eq!(builtin.input_cost_per_million.as_deref(), Some("0.05"));
        assert_eq!(
            builtin.source.as_deref(),
            Some("cc-switch:builtin:0345fad6048eed65b3423bedc8ce5711320ddfc3")
        );

        let imported = billing
            .import_cc_switch_model_pricing_json(
                "user-file",
                r#"{
                    "version": 1,
                    "models": [{
                        "modelId": "gpt-5-nano",
                        "displayName": "GPT-5 Nano",
                        "inputCostPerMillion": "0.06",
                        "outputCostPerMillion": "0.41",
                        "cacheReadCostPerMillion": "0.006",
                        "cacheCreationCostPerMillion": "0"
                    }],
                    "deletedModelIds": []
                }"#,
            )
            .expect("user cc-switch pricing should import");
        assert_eq!(imported, 1);
        let (scope, _, imported_price) = billing
            .resolve_price_with_scope("openai", "gpt-5-nano", None)
            .expect("user catalog must override the frozen seed");
        assert_eq!(scope, "catalog_model");
        assert_eq!(
            imported_price.input_cost_per_million.as_deref(),
            Some("0.06")
        );

        let imported = billing
            .import_cc_switch_model_pricing_json(
                "user-file-deleted",
                r#"{
                    "version": 1,
                    "models": [],
                    "deletedModelIds": ["gpt-5-nano"]
                }"#,
            )
            .expect("user tombstone should import");
        assert_eq!(imported, 0);
        assert!(
            billing
                .resolve_price("openai", "gpt-5-nano", None)
                .is_none(),
            "user tombstone must suppress the lower built-in seed"
        );

        billing
            .provider_prices
            .insert(String::from("openai"), billing_price("0.07"));
        assert_eq!(
            billing
                .resolve_price("openai", "gpt-5-nano", None)
                .and_then(|price| price.input_cost_per_million.as_deref()),
            Some("0.07"),
            "provider override must remain above the catalog tombstone"
        );
        let encoded = serde_json::to_string(&billing).expect("billing should serialize");
        let decoded: ProviderBillingConfiguration =
            serde_json::from_str(encoded.as_str()).expect("billing should deserialize");
        assert!(decoded.catalog_deleted_model_ids.contains("gpt-5-nano"));
    }

    /// Verifies malformed decimals and composite keys are rejected before persistence.
    /// 验证格式错误的十进制值与组合键会在持久化前被拒绝。
    #[test]
    fn billing_validation_rejects_malformed_values_and_keys() {
        let mut invalid_decimal = ProviderBillingConfiguration::default();
        invalid_decimal
            .provider_prices
            .insert(String::from("deepseek"), billing_price("NaN"));
        assert!(invalid_decimal.validate().is_err());

        let mut invalid_key = ProviderBillingConfiguration::default();
        invalid_key
            .model_prices
            .insert(String::from("deepseek/model"), billing_price("1"));
        assert!(invalid_key.validate().is_err());
    }

    /// Verifies schema eleven configurations migrate to the versioned billing schema.
    /// 验证 Schema 十一配置会迁移到版本化计费 Schema。
    #[test]
    fn schema_eleven_migrates_to_billing_configuration_schema() {
        let mut historical: PersistedAiConfiguration =
            serde_json::from_str(r#"{"schema_version":11}"#).expect("historical config");

        assert!(historical.migrate_to_current_schema().expect("migration"));
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert_eq!(historical.billing, ProviderBillingConfiguration::default());
    }

    /// Verifies schema twelve upgrades without inventing user-owned catalog tombstones.
    /// 验证 Schema 十二升级时不会虚构用户拥有的目录删除标记。
    #[test]
    fn schema_twelve_migrates_to_catalog_tombstone_schema() {
        let mut historical: PersistedAiConfiguration =
            serde_json::from_str(r#"{"schema_version":12}"#).expect("historical config");

        assert!(historical.migrate_to_current_schema().expect("migration"));
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.billing.catalog_deleted_model_ids.is_empty());
    }

    /// Builds one minimal exact billing price fixture.
    /// 构建一条最小精确计费价格夹具。
    fn billing_price(input_cost_per_million: &str) -> BillingPriceOverride {
        BillingPriceOverride {
            input_cost_per_million: Some(input_cost_per_million.to_string()),
            ..BillingPriceOverride::default()
        }
    }

    /// Verifies model overrides are independent from credential connection identifiers.
    /// 验证模型覆盖独立于凭据连接标识。
    #[test]
    fn model_override_key_uses_provider_model_and_protocol_scope() {
        let model_key = PersistedAiConfiguration::provider_model_key("custom:迁移", "model:alpha");
        assert_eq!(
            PersistedAiConfiguration::provider_model_key_parts(model_key.as_str()),
            Some(("custom:迁移", "model:alpha"))
        );
        assert!(
            PersistedAiConfiguration::provider_model_key_parts(
                format!("{model_key}:trailing").as_str()
            )
            .is_none()
        );

        let key = PersistedAiConfiguration::provider_model_protocol_key(
            "custom:迁移",
            "model:alpha",
            "anthropic-messages.v1",
        );
        assert_eq!(
            PersistedAiConfiguration::provider_model_protocol_key_parts(key.as_str()),
            Some(("custom:迁移", "model:alpha", "anthropic-messages.v1"))
        );
        assert!(
            PersistedAiConfiguration::provider_model_protocol_key_parts(
                format!("{key}:trailing").as_str()
            )
            .is_none()
        );
    }

    /// Verifies a confirmed protocol switch removes only incompatible route settings for that provider.
    /// 验证已确认协议切换只会移除该供应商不兼容的路由设置。
    #[test]
    fn protocol_switch_cleanup_preserves_target_and_other_provider_settings() {
        let mut configuration = PersistedAiConfiguration::default();
        for (provider_id, protocol_id) in [
            ("custom:test", "openai-chat-completions.v1"),
            ("custom:test", "openai-responses.v1"),
            ("custom:other", "openai-chat-completions.v1"),
        ] {
            configuration.model_overrides.insert(
                PersistedAiConfiguration::provider_model_protocol_key(
                    provider_id,
                    "private/chat",
                    protocol_id,
                ),
                ModelExecutionOverrides {
                    prompt_cache: Some(PromptCachePolicy::ProviderDefault),
                    ..ModelExecutionOverrides::default()
                },
            );
        }

        assert_eq!(
            configuration
                .inactive_provider_model_override_count("custom:test", "openai-responses.v1"),
            1
        );
        configuration
            .remove_inactive_provider_model_overrides("custom:test", "openai-responses.v1");

        assert_eq!(configuration.model_overrides.len(), 2);
        assert!(
            configuration.model_overrides.contains_key(
                PersistedAiConfiguration::provider_model_protocol_key(
                    "custom:test",
                    "private/chat",
                    "openai-responses.v1"
                )
                .as_str()
            )
        );
        assert!(
            configuration.model_overrides.contains_key(
                PersistedAiConfiguration::provider_model_protocol_key(
                    "custom:other",
                    "private/chat",
                    "openai-chat-completions.v1"
                )
                .as_str()
            )
        );
    }

    /// Verifies the removed-protocol set cleanup deletes only that protocol's overrides,
    /// while explicit preferences pointing at the removed protocol survive so binding keeps
    /// failing closed until the user re-selects (plan §R6.8).
    /// 验证被删除协议集合的清理只删除该协议拥有的覆盖；指向被删协议的显式偏好被保留，
    /// 使绑定持续失败封闭直到用户重选（方案 §R6.8）。
    #[test]
    fn removed_protocol_cleanup_is_scoped_to_the_removed_set() {
        let mut configuration = PersistedAiConfiguration::default();
        for (provider_id, protocol_id) in [
            ("custom:test", "openai-chat-completions.v1"),
            ("custom:test", "openai-responses.v1"),
            ("custom:test", "anthropic-messages.v1"),
            ("custom:other", "openai-chat-completions.v1"),
        ] {
            configuration.model_overrides.insert(
                PersistedAiConfiguration::provider_model_protocol_key(
                    provider_id,
                    "private/chat",
                    protocol_id,
                ),
                ModelExecutionOverrides {
                    prompt_cache: Some(PromptCachePolicy::ProviderDefault),
                    ..ModelExecutionOverrides::default()
                },
            );
        }
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::OpenAiChat,
            },
        );
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat2"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::OpenAiResponses,
            },
        );
        let removed =
            std::collections::BTreeSet::from([String::from("openai-chat-completions.v1")]);

        assert_eq!(
            configuration.removed_protocol_model_override_count("custom:test", &removed),
            1,
            "only the chat-completions override is owned by the removed protocol"
        );
        configuration.remove_removed_protocol_model_overrides("custom:test", &removed);
        assert_eq!(configuration.model_overrides.len(), 3);
        assert!(
            configuration.model_overrides.contains_key(
                PersistedAiConfiguration::provider_model_protocol_key(
                    "custom:test",
                    "private/chat",
                    "openai-responses.v1"
                )
                .as_str()
            ),
            "retained protocols keep their overrides"
        );
        assert!(
            configuration.model_overrides.contains_key(
                PersistedAiConfiguration::provider_model_protocol_key(
                    "custom:test",
                    "private/chat",
                    "anthropic-messages.v1"
                )
                .as_str()
            )
        );
        assert!(
            configuration.model_overrides.contains_key(
                PersistedAiConfiguration::provider_model_protocol_key(
                    "custom:other",
                    "private/chat",
                    "openai-chat-completions.v1"
                )
                .as_str()
            ),
            "other providers are never touched"
        );

        // The explicit preference for the removed protocol survives: dropping it would
        // silently fall back to Auto, while retaining it keeps the model blocked
        // (ExplicitProtocolDisabled) until the user re-selects (plan §R6.8).
        // 指向被删协议的显式偏好被保留：删除它会静默回退 Auto，保留它则使模型保持
        // blocked（ExplicitProtocolDisabled）直到用户重选（方案 §R6.8）。
        assert!(
            configuration
                .custom_model_protocol_preferences
                .contains_key(
                    PersistedAiConfiguration::provider_model_key("custom:test", "private/chat")
                        .as_str()
                ),
            "explicit preference for the removed protocol survives to keep the model blocked"
        );
        assert!(
            configuration
                .custom_model_protocol_preferences
                .contains_key(
                    PersistedAiConfiguration::provider_model_key("custom:test", "private/chat2")
                        .as_str()
                ),
            "explicit preference for a retained protocol survives"
        );
    }

    /// Verifies configuration contains no provider-owned default policy.
    /// 验证配置层不包含供应商拥有的默认策略。
    #[test]
    fn model_access_defaults_are_provider_neutral() {
        let configuration = PersistedAiConfiguration::default();
        assert!(configuration.model_is_enabled("openrouter", "vendor/unreviewed-model"));
        assert!(!configuration.model_is_enabled_with_default(
            "openrouter",
            "vendor/unreviewed-model",
            false
        ));
    }

    /// Verifies persisted access choices override either manifest-owned inherited default.
    /// 验证持久化访问选择会覆盖任一清单拥有的继承默认值。
    #[test]
    fn model_access_overrides_win_over_inherited_defaults() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", true);
        assert!(configuration.model_is_enabled_with_default(
            "openrouter",
            "vendor/unreviewed-model",
            false
        ));
        assert_eq!(configuration.model_enabled_overrides.len(), 1);

        configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", false);
        assert_eq!(configuration.model_enabled_overrides.len(), 1);
        assert!(!configuration.model_is_enabled_with_default(
            "openrouter",
            "vendor/unreviewed-model",
            true
        ));
        configuration.restore_model_enabled("openrouter", "vendor/unreviewed-model");
        assert!(configuration.model_enabled_overrides.is_empty());
    }

    /// Verifies one account stores only model decisions that differ from its inherited default.
    /// 验证一个账号仅保存与其继承默认值不同的模型决定。
    #[test]
    fn connection_model_access_policy_persists_sparse_differences() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_connection_model_enabled("account-a", "model-a", false);
        assert!(!configuration.connection_model_is_enabled("account-a", "model-a"));
        assert!(configuration.connection_model_is_enabled("account-a", "model-b"));
        assert_eq!(
            configuration
                .connection_model_access_policies
                .get("account-a")
                .expect("account policy must exist")
                .model_enabled_overrides,
            BTreeMap::from([(String::from("model-a"), false)])
        );

        configuration.set_connection_model_enabled("account-a", "model-a", true);
        assert!(configuration.connection_model_access_policies.is_empty());
    }

    /// Verifies a bulk-disabled account keeps future discovered models disabled until the user changes it.
    /// 验证批量禁用账号会让未来发现的模型继续保持禁用，直到用户主动更改。
    #[test]
    fn connection_model_access_default_applies_to_future_models() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_connection_models_enabled("account-a", false);
        assert!(!configuration.connection_model_is_enabled("account-a", "future-model"));

        configuration.set_connection_model_enabled("account-a", "future-model", true);
        assert!(configuration.connection_model_is_enabled("account-a", "future-model"));
        assert!(!configuration.connection_model_is_enabled("account-a", "another-future-model"));
    }

    /// Verifies schema six loads without account policy data and migrates to the current schema exactly.
    /// 验证 Schema 6 在没有账号策略数据时可加载，并精确迁移到当前 Schema。
    #[test]
    fn schema_six_migration_initializes_connection_model_access_policies() {
        let mut historical: PersistedAiConfiguration =
            serde_json::from_str(r#"{"schema_version":6}"#)
                .expect("schema six fixture must deserialize");
        assert!(historical.connection_model_access_policies.is_empty());
        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema six must migrate")
        );
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.connection_model_access_policies.is_empty());
    }

    /// Verifies schema seven preserves the required VMM profile repair while upgrading direct settings.
    /// 验证 Schema 7 在升级直接设置时仍会修复必需的 VMM 配置档。
    #[test]
    fn schema_seven_migration_repairs_missing_vmm_profile() {
        // historical represents a valid schema-seven file whose required profile was lost before persistence.
        // historical 表示一份持久化前丢失必需配置档的有效 Schema 7 文件。
        let mut historical = PersistedAiConfiguration {
            schema_version: 7,
            ..PersistedAiConfiguration::default()
        };
        historical.inference_profiles.remove("vmm");

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema seven must migrate")
        );

        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.inference_profiles.contains_key("vmm"));
    }

    /// Verifies schema-seven protocol limits become one conservative direct model setting.
    /// 验证 Schema 7 协议限制会转化为一份保守的直接模型设置。
    #[test]
    fn schema_seven_migrates_protocol_limits_to_direct_model_settings() {
        let mut historical = PersistedAiConfiguration {
            schema_version: 7,
            ..PersistedAiConfiguration::default()
        };
        let first_key = PersistedAiConfiguration::provider_model_protocol_key(
            "custom:迁移",
            "model:alpha",
            "openai-chat-completions.v1",
        );
        let second_key = PersistedAiConfiguration::provider_model_protocol_key(
            "custom:迁移",
            "model:alpha",
            "openai-responses.v1",
        );
        historical.model_overrides.insert(
            first_key.clone(),
            ModelExecutionOverrides {
                context_limit: Some(16_384),
                output_limit: Some(4_096),
                prompt_cache: Some(PromptCachePolicy::ProviderDefault),
                model_parameters: BTreeMap::new(),
            },
        );
        historical.model_overrides.insert(
            second_key.clone(),
            ModelExecutionOverrides {
                context_limit: Some(8_192),
                output_limit: Some(2_048),
                ..ModelExecutionOverrides::default()
            },
        );

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema seven must migrate")
        );

        let model_key = PersistedAiConfiguration::provider_model_key("custom:迁移", "model:alpha");
        let model_settings = historical
            .model_capability_overrides
            .get(model_key.as_str())
            .expect("legacy limits must become direct model settings");
        assert_eq!(model_settings.context_limit, Some(8_192));
        assert_eq!(model_settings.output_limit, Some(2_048));
        let protocol_settings = historical
            .model_overrides
            .get(first_key.as_str())
            .expect("protocol cache settings must remain");
        assert_eq!(
            protocol_settings.prompt_cache,
            Some(PromptCachePolicy::ProviderDefault)
        );
        assert!(protocol_settings.context_limit.is_none());
        assert!(protocol_settings.output_limit.is_none());
        assert!(!historical.model_overrides.contains_key(second_key.as_str()));
    }

    /// Verifies schema-seven user-owned non-text output declarations are removed during migration.
    /// 验证 Schema 7 中用户持有的非文本输出声明会在迁移时移除。
    #[test]
    fn schema_seven_removes_custom_non_text_output_declarations() {
        let provider_id = "custom:legacy-output";
        let model_id = "legacy-image-model";
        let mut historical = PersistedAiConfiguration {
            schema_version: 7,
            ..PersistedAiConfiguration::default()
        };
        historical.custom_provider_model_defaults.insert(
            String::from(provider_id),
            CustomProviderModelDefaults {
                context_limit: 150_000,
                output_resources: vec![ResourceKind::Image],
                ..CustomProviderModelDefaults::default()
            },
        );
        historical.custom_models.insert(
            String::from(provider_id),
            BTreeMap::from([(
                String::from(model_id),
                CustomModelDefinition {
                    id: String::from(model_id),
                    display_name: String::from("Legacy Image Model"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        output_resources: vec![ResourceKind::Image],
                        ..ModelCapabilities::default()
                    },
                    request_projection: Default::default(),
                },
            )]),
        );
        historical.custom_model_template_decisions.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
            CustomModelTemplateDecision::Applied {
                selection: crate::CustomModelTemplateSelection {
                    template_key: String::from("openai/gpt-image-1"),
                    template_revision: String::from(crate::CUSTOM_MODEL_TEMPLATE_REVISION),
                    context_limit: 32_768,
                    supports_text_input: true,
                    supports_text_output: false,
                    input_resources: Vec::new(),
                    output_resources: vec![ResourceKind::Image],
                },
            },
        );

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema seven must migrate")
        );

        assert!(
            historical
                .custom_provider_model_defaults
                .get(provider_id)
                .expect("non-default provider settings must remain")
                .output_resources
                .is_empty()
        );
        assert!(
            historical.custom_models[provider_id][model_id]
                .capabilities
                .output_resources
                .is_empty()
        );
        let decision = &historical.custom_model_template_decisions
            [&PersistedAiConfiguration::provider_model_key(provider_id, model_id)];
        let CustomModelTemplateDecision::Applied { selection } = decision else {
            panic!("applied template decision must remain");
        };
        assert!(selection.output_resources.is_empty());
    }

    /// Verifies custom model availability uses the definition default and persists only deviations.
    /// 验证自定义模型可用性使用定义默认值，并且仅持久化偏差。
    #[test]
    fn custom_model_access_uses_definition_default() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_models.insert(
            String::from("custom-provider"),
            BTreeMap::from([(
                String::from("private-model"),
                CustomModelDefinition {
                    id: String::from("private-model"),
                    display_name: String::from("Private Model"),
                    enabled: false,
                    capabilities: ModelCapabilities::default(),
                    request_projection: Default::default(),
                },
            )]),
        );

        assert!(!configuration.model_is_enabled("custom-provider", "private-model"));
        configuration.set_model_enabled("custom-provider", "private-model", true);
        assert!(configuration.model_is_enabled("custom-provider", "private-model"));
        assert_eq!(configuration.model_enabled_overrides.len(), 1);
        configuration.set_model_enabled("custom-provider", "private-model", false);
        assert_eq!(configuration.model_enabled_overrides.len(), 1);
        configuration.restore_model_enabled("custom-provider", "private-model");
        assert!(configuration.model_enabled_overrides.is_empty());
    }

    /// Verifies capability differences replace only declared fields on the effective model copy.
    /// 验证能力差异仅替换有效模型副本上明确声明的字段。
    #[test]
    fn capability_overrides_leave_immutable_source_fields_untouched() {
        let mut configuration = PersistedAiConfiguration::default();
        let key = PersistedAiConfiguration::provider_model_key("provider", "model");
        configuration.model_capability_overrides.insert(
            key,
            ModelCapabilityOverrides {
                context_limit: Some(32_000),
                output_limit: None,
                input_resources: Some(vec![ResourceKind::Image]),
                output_resources: None,
                ..ModelCapabilityOverrides::default()
            },
        );
        let mut effective = ModelCapabilities {
            context_limit: Some(200_000),
            input_resources: Vec::new(),
            output_resources: vec![ResourceKind::File],
            ..ModelCapabilities::default()
        };

        configuration.apply_model_capability_overrides("provider", "model", &mut effective);

        assert_eq!(effective.context_limit, Some(32_000));
        assert_eq!(effective.input_resources, vec![ResourceKind::Image]);
        assert_eq!(effective.output_resources, vec![ResourceKind::File]);
    }

    /// Verifies misspelled capability fields fail instead of appearing to persist successfully.
    /// 验证拼错的能力字段会直接失败，而不是表现为已成功持久化。
    #[test]
    fn capability_overrides_reject_unknown_fields() {
        let error = serde_json::from_str::<ModelCapabilityOverrides>(r#"{"context_limt":32000}"#)
            .expect_err("unknown capability field must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies deleting a model cannot resurrect stale account, protocol, or internal settings later.
    /// 验证删除模型后不会在未来重新激活过期账号、协议或内部设置。
    #[test]
    fn removing_model_clears_every_model_owned_non_route_setting() {
        let mut configuration = PersistedAiConfiguration::default();
        let provider_id = "custom-provider";
        let model_id = "private-model";
        let provider_model_key =
            PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        let connection_model_key =
            PersistedAiConfiguration::connection_model_key("connection", model_id);
        configuration.connection_graph.connections.insert(
            String::from("connection"),
            crate::CredentialConnection {
                id: String::from("connection"),
                provider_id: String::from(provider_id),
                service_entry_id: String::from("public-api"),
                credential_profile_id: String::from("api-key"),
                credential_mode: crate::CredentialMode::Token,
                credential_id: String::from("credential"),
                display_name: String::from("Account"),
                secret_reference: String::from("secret"),
                management_secret_reference: None,
                service_origin_override: None,
                verified_protocols: BTreeSet::from([String::from("openai_chat")]),
                selected_protocol: String::from("openai_chat"),
                prompt_cache_by_protocol: BTreeMap::new(),
                usage_scope_override: None,
            },
        );
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .insert(provider_model_key.clone(), String::from("connection"));
        configuration
            .model_enabled_overrides
            .insert(provider_model_key.clone(), true);
        configuration.model_capability_overrides.insert(
            provider_model_key.clone(),
            ModelCapabilityOverrides::default(),
        );
        configuration.model_overrides.insert(
            PersistedAiConfiguration::provider_model_protocol_key(
                provider_id,
                model_id,
                "openai_chat",
            ),
            ModelExecutionOverrides::default(),
        );
        configuration.internal_model_enabled_overrides.insert(
            PersistedAiConfiguration::internal_model_operation_key(
                provider_id,
                model_id,
                AiOperation::EmbeddingCreate,
            ),
            true,
        );
        configuration
            .secondary_routes
            .insert(connection_model_key, SecondaryRouteSelection::Automatic);
        configuration.custom_model_template_decisions.insert(
            provider_model_key.clone(),
            CustomModelTemplateDecision::Rejected {
                template_key: String::from("private-model"),
                template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
            },
        );
        configuration
            .custom_model_connection_enabled_overrides
            .insert(
                PersistedAiConfiguration::connection_model_key("connection", model_id),
                false,
            );

        configuration.remove_model_owned_settings(provider_id, model_id);

        assert!(configuration.model_enabled_overrides.is_empty());
        assert!(configuration.model_capability_overrides.is_empty());
        assert!(configuration.custom_model_template_decisions.is_empty());
        assert!(
            configuration
                .custom_model_connection_enabled_overrides
                .is_empty()
        );
        assert!(configuration.model_overrides.is_empty());
        assert!(configuration.internal_model_enabled_overrides.is_empty());
        assert!(configuration.secondary_routes.is_empty());
        assert!(
            configuration
                .connection_graph
                .account_preferences
                .model_last_connections
                .is_empty()
        );
    }

    /// Verifies provider removal warnings detect user-owned model information before cascade cleanup.
    /// 验证供应商移除提示会在级联清理前检测用户自有模型信息。
    #[test]
    fn provider_model_customization_detection_reports_owned_state() {
        let provider_id = "custom-provider";
        let model_id = "private-model";
        let mut configuration = PersistedAiConfiguration::default();
        assert!(!configuration.provider_has_model_customizations(provider_id));

        configuration.custom_models.insert(
            String::from(provider_id),
            BTreeMap::from([(
                String::from(model_id),
                CustomModelDefinition {
                    id: String::from(model_id),
                    display_name: String::from("Private Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: Default::default(),
                },
            )]),
        );
        assert!(configuration.provider_has_model_customizations(provider_id));
    }

    /// Verifies one explicit model protocol choice alone is disclosed as provider-owned state.
    /// 验证仅有一项显式模型协议选择时也会披露为供应商自有状态。
    #[test]
    fn provider_model_customization_detection_includes_protocol_preference() {
        // provider_id and model_id identify the sole provider-owned customization below.
        // provider_id 与 model_id 标识下方唯一的供应商自有定制。
        let provider_id = "custom:protocol-only";
        let model_id = "private-model";
        // configuration intentionally contains no other model or route customization.
        // configuration 有意不包含其他模型或路由定制。
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
            CustomModelProtocolPreference::Explicit {
                protocol: CustomProviderProtocol::AnthropicMessages,
            },
        );

        assert!(configuration.provider_has_model_customizations(provider_id));
        assert!(!configuration.provider_has_model_customizations("custom:peer"));
    }

    /// Verifies deleting a custom provider clears every hydrated field without touching peers.
    /// 验证删除自定义供应商会清理全部已补水字段且不会影响其他供应商。
    #[test]
    fn removing_custom_provider_clears_all_owned_state_without_touching_peers() {
        let mut configuration = PersistedAiConfiguration::default();
        let removed_provider = "custom:provider";
        let retained_provider = "custom:other-provider";
        // These descriptors own the removable and retained standalone manifest documents.
        // 这些描述分别持有待删除与保留的独立 Manifest 文档。
        let removed_descriptor = custom_provider_descriptor(removed_provider);
        let retained_descriptor = custom_provider_descriptor(retained_provider);
        // These manifests prove the hydrated runtime map follows the descriptor lifecycle.
        // 这些 Manifest 证明已补水运行时映射会遵循描述的生命周期。
        let removed_manifest = crate::legacy_custom_descriptor_to_manifest(&removed_descriptor)
            .expect("removed custom manifest should build");
        let retained_manifest = crate::legacy_custom_descriptor_to_manifest(&retained_descriptor)
            .expect("retained custom manifest should build");
        configuration
            .custom_providers
            .insert(String::from(removed_provider), removed_descriptor);
        configuration
            .custom_providers
            .insert(String::from(retained_provider), retained_descriptor);
        configuration.custom_provider_file_refs.extend([
            String::from(removed_provider),
            String::from(retained_provider),
        ]);
        configuration
            .custom_provider_manifests
            .insert(String::from(removed_provider), removed_manifest);
        configuration
            .custom_provider_manifests
            .insert(String::from(retained_provider), retained_manifest);
        let removed_key =
            PersistedAiConfiguration::provider_model_key(removed_provider, "private-model");
        let retained_key =
            PersistedAiConfiguration::provider_model_key(retained_provider, "public-model");
        // removed_internal_key and retained_internal_key cover operation-scoped model defaults.
        // removed_internal_key 与 retained_internal_key 覆盖操作作用域模型默认值。
        let removed_internal_key = PersistedAiConfiguration::internal_model_operation_key(
            removed_provider,
            "private-model",
            AiOperation::EmbeddingCreate,
        );
        let retained_internal_key = PersistedAiConfiguration::internal_model_operation_key(
            retained_provider,
            "public-model",
            AiOperation::EmbeddingCreate,
        );
        configuration.custom_models.insert(
            String::from(removed_provider),
            BTreeMap::from([(
                String::from("private-model"),
                CustomModelDefinition {
                    id: String::from("private-model"),
                    display_name: String::from("Private Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: Default::default(),
                },
            )]),
        );
        configuration.custom_models.insert(
            String::from(retained_provider),
            BTreeMap::from([(
                String::from("public-model"),
                CustomModelDefinition {
                    id: String::from("public-model"),
                    display_name: String::from("Public Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: Default::default(),
                },
            )]),
        );
        configuration
            .model_enabled_overrides
            .insert(removed_key.clone(), true);
        configuration
            .model_enabled_overrides
            .insert(retained_key.clone(), true);
        configuration
            .model_capability_overrides
            .insert(removed_key.clone(), ModelCapabilityOverrides::default());
        configuration.custom_provider_model_defaults.insert(
            String::from(removed_provider),
            CustomProviderModelDefaults::default(),
        );
        configuration.custom_model_template_decisions.insert(
            removed_key.clone(),
            CustomModelTemplateDecision::Rejected {
                template_key: String::from("private-model"),
                template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
            },
        );
        configuration.model_overrides.insert(
            PersistedAiConfiguration::provider_model_protocol_key(
                removed_provider,
                "private-model",
                "openai_chat",
            ),
            ModelExecutionOverrides::default(),
        );
        configuration
            .internal_model_enabled_overrides
            .insert(removed_internal_key.clone(), true);
        configuration
            .internal_model_overrides
            .insert(removed_internal_key, InternalModelOverrides::default());
        configuration.internal_model_overrides.insert(
            retained_internal_key.clone(),
            InternalModelOverrides::default(),
        );
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .insert(removed_key, String::from("removed-connection"));
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from(removed_provider),
            model_id: String::from("private-model"),
            connection_id: String::from("removed-connection"),
        });
        // removed_llm_route and retained_llm_route prove provider filtering preserves peer routes.
        // removed_llm_route 与 retained_llm_route 验证供应商过滤会保留其他供应商路由。
        let removed_llm_route = InferenceRouteConfiguration {
            provider_id: String::from(removed_provider),
            service_entry_id: String::from("removed-entry"),
            connection_id: String::from("removed-connection"),
            model_id: String::from("private-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("removed-protocol"),
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let retained_llm_route = InferenceRouteConfiguration {
            provider_id: String::from(retained_provider),
            service_entry_id: String::from("retained-entry"),
            connection_id: String::from("retained-connection"),
            model_id: String::from("public-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("retained-protocol"),
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        // removed_embedding_route and retained_rerank_route cover the two optional route slots.
        // removed_embedding_route 与 retained_rerank_route 覆盖两个可选路由槽位。
        let removed_embedding_route = InferenceRouteConfiguration {
            provider_id: String::from(removed_provider),
            service_entry_id: String::from("removed-entry"),
            connection_id: String::from("removed-connection"),
            model_id: String::from("private-model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("removed-embedding-action"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let retained_rerank_route = InferenceRouteConfiguration {
            provider_id: String::from(retained_provider),
            service_entry_id: String::from("retained-entry"),
            connection_id: String::from("retained-connection"),
            model_id: String::from("public-model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("retained-rerank-action"),
            },
            operation: AiOperation::RerankDocuments,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        configuration.inference_profiles.insert(
            String::from("mixed-profile"),
            InferenceProfile {
                llm_routes: BTreeMap::from([
                    (String::from("removed-purpose"), removed_llm_route),
                    (String::from("retained-purpose"), retained_llm_route),
                ]),
                embedding: Some(removed_embedding_route),
                rerank: Some(retained_rerank_route),
            },
        );

        // The removed descriptor proves the comprehensive cleanup path owned the exact provider.
        // 被移除描述证明综合清理路径持有精确供应商。
        let removed_descriptor = configuration
            .remove_custom_provider_owned_state(removed_provider)
            .expect("custom provider should be removed");

        assert_eq!(removed_descriptor.id, removed_provider);
        assert_eq!(
            configuration
                .custom_providers
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![String::from(retained_provider)]
        );
        assert_eq!(
            configuration
                .custom_provider_file_refs
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![String::from(retained_provider)]
        );
        assert_eq!(
            configuration
                .custom_provider_manifests
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![String::from(retained_provider)]
        );
        assert_eq!(
            configuration
                .custom_models
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![String::from(retained_provider)]
        );
        assert_eq!(
            configuration.model_enabled_overrides,
            BTreeMap::from([(retained_key, true)])
        );
        assert!(configuration.model_capability_overrides.is_empty());
        assert!(configuration.custom_provider_model_defaults.is_empty());
        assert!(configuration.custom_model_template_decisions.is_empty());
        assert!(configuration.model_overrides.is_empty());
        assert!(configuration.internal_model_enabled_overrides.is_empty());
        assert_eq!(
            configuration.internal_model_overrides,
            BTreeMap::from([(retained_internal_key, InternalModelOverrides::default())])
        );
        // profile is the surviving mixed profile after provider-scoped route cleanup.
        // profile 是按供应商清理路由后保留下来的混合配置档。
        let profile = configuration
            .inference_profiles
            .get("mixed-profile")
            .expect("mixed inference profile");
        assert_eq!(
            profile.llm_routes.keys().cloned().collect::<Vec<_>>(),
            vec![String::from("retained-purpose")]
        );
        assert!(profile.embedding.is_none());
        assert_eq!(
            profile
                .rerank
                .as_ref()
                .map(|route| route.provider_id.as_str()),
            Some(retained_provider)
        );
        assert!(
            configuration
                .connection_graph
                .account_preferences
                .model_last_connections
                .is_empty()
        );
        assert!(configuration.active.is_none());
        assert!(!configuration.provider_has_model_customizations(removed_provider));
        assert!(configuration.provider_has_model_customizations(retained_provider));
    }

    /// Verifies schema two upgrades without inventing model overlays and new fields round-trip exactly.
    /// 验证 Schema 2 升级时不会虚构模型覆盖，并且新增字段能够精确往返。
    #[test]
    fn schema_two_migration_and_model_overlay_round_trip_are_exact() {
        let mut historical = PersistedAiConfiguration {
            schema_version: 2,
            ..PersistedAiConfiguration::default()
        };
        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema two must migrate")
        );
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.custom_models.is_empty());
        assert!(historical.model_capability_overrides.is_empty());

        let model = CustomModelDefinition {
            id: String::from("private-model"),
            display_name: String::from("Private Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                context_limit: Some(200_000),
                input_resources: vec![ResourceKind::Image],
                ..ModelCapabilities::default()
            },
            request_projection: Default::default(),
        };
        historical.custom_models.insert(
            String::from("custom-provider"),
            BTreeMap::from([(model.id.clone(), model)]),
        );
        historical.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("custom-provider", "private-model"),
            ModelCapabilityOverrides {
                context_limit: Some(64_000),
                output_limit: None,
                input_resources: Some(Vec::new()),
                output_resources: None,
                ..ModelCapabilityOverrides::default()
            },
        );

        let encoded = serde_json::to_string(&historical).expect("configuration must serialize");
        let decoded: PersistedAiConfiguration =
            serde_json::from_str(encoded.as_str()).expect("configuration must deserialize");
        assert_eq!(decoded.custom_models, historical.custom_models);
        assert_eq!(
            decoded.model_capability_overrides,
            historical.model_capability_overrides
        );
    }

    /// R12/必测 1：从旧 schema 升级一次与重复升级两次结果相同。
    ///
    /// Every supported legacy schema (0..=12) must migrate idempotently: applying the
    /// migration a second time to an already-current configuration produces the identical
    /// snapshot and returns `Ok(false)` (no further change) (plan §R12).
    /// 每个受支持的旧 schema（0..=12）必须幂等迁移：对已是当前版本的配置再次执行迁移，
    /// 产生完全相同的快照并返回 `Ok(false)`（不再变化）（方案 §R12）。
    #[test]
    fn legacy_schema_migration_is_idempotent_across_all_supported_versions() {
        for legacy_schema in 0..=12 {
            let mut once = PersistedAiConfiguration {
                schema_version: legacy_schema,
                ..PersistedAiConfiguration::default()
            };
            assert!(
                once.migrate_to_current_schema()
                    .expect("legacy schema must migrate"),
                "schema {legacy_schema} must change on first migration"
            );
            let once_encoded = serde_json::to_vec(&once).expect("configuration must serialize");

            let mut twice = PersistedAiConfiguration {
                schema_version: legacy_schema,
                ..PersistedAiConfiguration::default()
            };
            twice
                .migrate_to_current_schema()
                .expect("first migration must succeed");
            let second_changed = twice
                .migrate_to_current_schema()
                .expect("second migration must succeed");
            assert!(
                !second_changed,
                "schema {legacy_schema} second migration must be a no-op"
            );
            let twice_encoded = serde_json::to_vec(&twice).expect("configuration must serialize");
            assert_eq!(
                once_encoded, twice_encoded,
                "schema {legacy_schema} one-time and repeated migration must be identical"
            );
        }
    }

    /// Verifies schema two migrates legacy resolved custom capabilities into explicit differences.
    /// 验证 Schema 2 会把旧版自定义模型已解析能力迁移为显式差异。
    #[test]
    fn schema_two_migrates_legacy_custom_model_capabilities() {
        let provider_id = "custom-provider";
        let model_id = "private-model";
        let mut historical = PersistedAiConfiguration {
            schema_version: 2,
            custom_models: BTreeMap::from([(
                String::from(provider_id),
                BTreeMap::from([(
                    String::from(model_id),
                    CustomModelDefinition {
                        id: String::from(model_id),
                        display_name: String::from("Private Model"),
                        enabled: true,
                        capabilities: ModelCapabilities {
                            context_limit: Some(64_000),
                            output_limit: Some(4_096),
                            input_resources: vec![ResourceKind::Image],
                            ..ModelCapabilities::default()
                        },
                        request_projection: Default::default(),
                    },
                )]),
            )]),
            ..PersistedAiConfiguration::default()
        };

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema two must migrate")
        );

        let migrated_model = historical
            .custom_model(provider_id, model_id)
            .expect("custom model identity and advanced contract should remain");
        assert_eq!(migrated_model.capabilities.context_limit, None);
        assert_eq!(migrated_model.capabilities.output_limit, None);
        assert!(migrated_model.capabilities.input_resources.is_empty());
        let differences = historical
            .model_capability_overrides(provider_id, model_id)
            .expect("legacy basic fields should become explicit differences");
        assert_eq!(differences.context_limit, Some(64_000));
        assert_eq!(differences.output_limit, Some(4_096));
        assert_eq!(differences.input_resources, Some(vec![ResourceKind::Image]));
    }

    /// Verifies schema four upgrades without inventing pending credential state.
    /// 验证 Schema 4 升级时不会凭空创建待验证凭据状态。
    #[test]
    fn schema_four_migration_initializes_pending_validation_state() {
        let mut historical = PersistedAiConfiguration {
            schema_version: 4,
            ..PersistedAiConfiguration::default()
        };

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema four must migrate")
        );
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.pending_validation_connection_ids.is_empty());
    }

    /// Verifies schema-five files produced by the earlier migration are normalized without replacing newer explicit differences.
    /// 验证早期迁移生成的 Schema 5 文件会被规范化，且不会覆盖更新的显式差异。
    #[test]
    fn schema_five_migration_repairs_duplicated_basic_capabilities() {
        let provider_id = "custom-provider";
        let model_id = "private-model";
        let provider_model_key =
            PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        let mut historical = PersistedAiConfiguration {
            schema_version: 5,
            custom_models: BTreeMap::from([(
                String::from(provider_id),
                BTreeMap::from([(
                    String::from(model_id),
                    CustomModelDefinition {
                        id: String::from(model_id),
                        display_name: String::from("Private Model"),
                        enabled: true,
                        capabilities: ModelCapabilities {
                            context_limit: Some(64_000),
                            output_limit: Some(4_096),
                            input_resources: vec![ResourceKind::Image],
                            ..ModelCapabilities::default()
                        },
                        request_projection: Default::default(),
                    },
                )]),
            )]),
            model_capability_overrides: BTreeMap::from([(
                provider_model_key.clone(),
                ModelCapabilityOverrides {
                    context_limit: Some(128_000),
                    output_limit: Some(8_192),
                    input_resources: Some(Vec::new()),
                    output_resources: None,
                    ..ModelCapabilityOverrides::default()
                },
            )]),
            ..PersistedAiConfiguration::default()
        };

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema five must migrate")
        );

        let migrated_model = historical
            .custom_model(provider_id, model_id)
            .expect("migrated custom model should remain configured");
        assert_eq!(migrated_model.capabilities.context_limit, None);
        assert_eq!(migrated_model.capabilities.output_limit, None);
        assert!(migrated_model.capabilities.input_resources.is_empty());
        assert_eq!(
            historical
                .model_capability_overrides
                .get(provider_model_key.as_str()),
            Some(&ModelCapabilityOverrides {
                context_limit: Some(128_000),
                output_limit: Some(8_192),
                input_resources: Some(Vec::new()),
                output_resources: None,
                ..ModelCapabilityOverrides::default()
            })
        );
    }

    /// Verifies schema-three complete custom-model capabilities become explicit user differences.
    /// 验证 Schema 3 的完整自定义模型能力会迁移为显式用户差异。
    #[test]
    fn schema_three_migration_preserves_custom_model_capability_differences() {
        let provider_id = "custom-provider";
        let model_id = "private-model";
        let model = CustomModelDefinition {
            id: String::from(model_id),
            display_name: String::from("Private Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                context_limit: Some(64_000),
                output_limit: Some(4_096),
                input_resources: vec![ResourceKind::Image],
                evidence_revision: String::from("legacy.advanced.v1"),
                ..ModelCapabilities::default()
            },
            request_projection: Default::default(),
        };
        let mut historical = PersistedAiConfiguration {
            schema_version: 3,
            custom_models: BTreeMap::from([(
                String::from(provider_id),
                BTreeMap::from([(String::from(model_id), model)]),
            )]),
            ..PersistedAiConfiguration::default()
        };

        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema three must migrate")
        );

        assert_eq!(
            historical
                .model_capability_overrides
                .get(PersistedAiConfiguration::provider_model_key(provider_id, model_id).as_str()),
            Some(&ModelCapabilityOverrides {
                context_limit: Some(64_000),
                output_limit: Some(4_096),
                input_resources: Some(vec![ResourceKind::Image]),
                output_resources: None,
                ..ModelCapabilityOverrides::default()
            })
        );
        let migrated_model = historical
            .custom_model(provider_id, model_id)
            .expect("migrated custom model should remain configured");
        assert_eq!(migrated_model.capabilities.context_limit, None);
        assert_eq!(migrated_model.capabilities.output_limit, None);
        assert!(migrated_model.capabilities.input_resources.is_empty());
        assert!(migrated_model.capabilities.output_resources.is_empty());
        assert_eq!(
            migrated_model.capabilities.evidence_revision,
            "legacy.advanced.v1"
        );
    }

    /// Verifies schema-three values equal to the new defaults do not create redundant overlays.
    /// 验证 Schema 3 中等于新默认值的能力不会生成冗余覆盖。
    #[test]
    fn schema_three_migration_skips_values_equal_to_provider_defaults() {
        let provider_id = "custom-provider";
        let model_id = "default-model";
        let defaults = CustomProviderModelDefaults::default();
        let model = CustomModelDefinition {
            id: String::from(model_id),
            display_name: String::from("Default Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                context_limit: Some(defaults.context_limit),
                output_limit: Some(defaults.output_limit),
                input_resources: defaults.input_resources.clone(),
                output_resources: defaults.output_resources.clone(),
                ..ModelCapabilities::default()
            },
            request_projection: Default::default(),
        };
        let mut historical = PersistedAiConfiguration {
            schema_version: 3,
            custom_models: BTreeMap::from([(
                String::from(provider_id),
                BTreeMap::from([(String::from(model_id), model)]),
            )]),
            ..PersistedAiConfiguration::default()
        };

        historical
            .migrate_to_current_schema()
            .expect("schema three must migrate");

        assert!(historical.model_capability_overrides.is_empty());
        let migrated_model = historical
            .custom_model(provider_id, model_id)
            .expect("migrated custom model should remain configured");
        assert_eq!(migrated_model.capabilities.context_limit, None);
        assert_eq!(migrated_model.capabilities.output_limit, None);
        assert!(migrated_model.capabilities.input_resources.is_empty());
        assert!(migrated_model.capabilities.output_resources.is_empty());
    }

    /// Verifies template selections preserve their exact text modality decisions through JSON.
    /// 验证模板选择通过 JSON 往返时会精确保留文本模态决策。
    #[test]
    fn custom_model_template_text_capabilities_round_trip_exactly() {
        let key = PersistedAiConfiguration::provider_model_key("custom-provider", "image-model");
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_model_template_decisions.insert(
            key.clone(),
            CustomModelTemplateDecision::Applied {
                selection: CustomModelTemplateSelection {
                    template_key: String::from("image-model"),
                    template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
                    context_limit: 200_000,
                    supports_text_input: true,
                    supports_text_output: false,
                    input_resources: vec![ResourceKind::Image],
                    output_resources: vec![ResourceKind::Image],
                },
            },
        );

        let encoded = serde_json::to_string(&configuration).expect("configuration must serialize");
        let decoded: PersistedAiConfiguration =
            serde_json::from_str(encoded.as_str()).expect("configuration must deserialize");

        assert_eq!(
            decoded.custom_model_template_decisions.get(key.as_str()),
            configuration
                .custom_model_template_decisions
                .get(key.as_str())
        );
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

    /// Verifies Codex context-window policy inherits sparsely from system to provider and model.
    /// 验证 Codex 上下文窗口策略按系统、供应商、模型逐字段稀疏继承。
    #[test]
    fn context_window_policy_inherits_by_layer_without_shrinking_raw_window() {
        let provider_id = "custom:proxy";
        let model_id = "gpt-5.6-luna";
        let mut configuration = PersistedAiConfiguration {
            context_window_policy_overrides: ContextWindowPolicyOverrides {
                effective_context_window_percent: Some(95),
                ..ContextWindowPolicyOverrides::default()
            },
            ..PersistedAiConfiguration::default()
        };
        configuration
            .provider_context_window_policy_overrides
            .insert(
                String::from(provider_id),
                ContextWindowPolicyOverrides {
                    auto_compact_percent: Some(88),
                    ..ContextWindowPolicyOverrides::default()
                },
            );
        configuration.model_context_window_policy_overrides.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
            ContextWindowPolicyOverrides {
                auto_compact_token_limit: Some(123_456),
                ..ContextWindowPolicyOverrides::default()
            },
        );

        let policy = configuration.resolve_context_compaction_policy(
            provider_id,
            model_id,
            Some(500_000),
            None,
        );
        assert_eq!(policy.window.effective_context_window_percent, 95);
        assert_eq!(policy.window.auto_compact_percent, 88);
        assert_eq!(policy.window.auto_compact_token_limit, Some(123_456));
        assert_eq!(policy.window.fallback_buffer_tokens, 0);
        assert!(policy.window_policy_is_explicit);
        assert_eq!(policy.tail_context_limit, Some(500_000));
        assert_eq!(policy.tail_turns, 10);
        assert_eq!(policy.preserve_recent_tokens, 25_000);
    }

    /// Verifies a child-task behavior override keeps the selected route window policy.
    /// 验证子任务行为覆盖只改变压缩行为而仍继承选定路由的窗口策略。
    #[test]
    fn behavior_only_session_policy_inherits_selected_route_window() {
        let provider_id = "custom:proxy";
        let model_id = "gpt-5.6-luna";
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .provider_context_window_policy_overrides
            .insert(
                String::from(provider_id),
                ContextWindowPolicyOverrides {
                    effective_context_window_percent: Some(92),
                    ..ContextWindowPolicyOverrides::default()
                },
            );

        let behavior_only = ContextCompactionPolicy {
            auto: false,
            ..ContextCompactionPolicy::default()
        };
        let policy = configuration.resolve_context_compaction_policy(
            provider_id,
            model_id,
            Some(300_000),
            Some(behavior_only),
        );

        assert!(!policy.auto);
        assert_eq!(policy.window.effective_context_window_percent, 92);
        assert!(policy.window_policy_is_explicit);
        assert_eq!(policy.tail_context_limit, Some(300_000));
        assert_eq!(policy.tail_turns, 6);
        assert_eq!(policy.preserve_recent_tokens, 15_000);
    }

    /// Verifies inherited hard and compaction percentages are validated after layer merging.
    /// 验证硬窗口与压缩百分比在分层合并后仍会进行顺序校验。
    #[test]
    fn context_window_policy_rejects_invalid_inherited_layer_ordering() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .set_context_window_policy_overrides(ContextWindowPolicyOverrides {
                effective_context_window_percent: Some(95),
                ..ContextWindowPolicyOverrides::default()
            })
            .expect("system policy should be valid");

        let error = configuration
            .set_provider_context_window_policy_overrides(
                "custom:proxy",
                ContextWindowPolicyOverrides {
                    auto_compact_percent: Some(96),
                    ..ContextWindowPolicyOverrides::default()
                },
            )
            .expect_err("a stronger inherited compacting threshold must be rejected");
        assert!(error.contains("cannot exceed"));
        assert!(
            !configuration
                .provider_context_window_policy_overrides
                .contains_key("custom:proxy")
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

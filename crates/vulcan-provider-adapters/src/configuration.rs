//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{
    CustomModelDefinition, CustomModelRequestProjection, CustomModelTemplateDecision,
    CustomProviderModelDefaults, PersistedConnectionGraph, ProviderDescriptor,
    ProviderToolCapabilityKind, ProviderToolTargetSelection, SecondaryRouteSelection,
    validate_custom_provider_reasoning_settings,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, EmbeddingEncoding, EmbeddingOutputKind, EmbeddingTask,
    InferenceUsageScope, ModelCapabilities, ModelParameterValue, PromptCachePolicy,
    ReasoningControlMode, ReasoningEffort, RerankTruncation, ResourceKind,
    StructuredOutputCapability,
};
use vulcan_context::{
    ContextCompactionPolicy, ContextCompactionWindowNodeId, ContextWindowPolicyOverrides,
};

/// Identifies the current persisted AI configuration schema.
/// 标识当前持久化 AI 配置 Schema。
pub const PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION: u32 = 19;

/// Largest integer that can cross the JSON-to-JavaScript settings boundary without precision loss.
/// 可在 JSON 到 JavaScript 设置边界间无精度损失传递的最大整数。
const JAVASCRIPT_MAX_SAFE_INTEGER_U64: u64 = 9_007_199_254_740_991;

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
    /// Optional complete structured-output capability replacement.
    /// 可选的完整结构化输出能力替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<StructuredOutputCapability>,
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
            && self.structured_output.is_none()
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

    /// Validates one override against the editable surface owned by its exact provider kind.
    /// 根据精确供应商类型所拥有的可编辑范围校验一层覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies either one system provider or one user-owned custom provider.
    /// `provider_id` 标识一个系统供应商或用户自有自定义供应商。
    ///
    /// # Returns
    /// 返回值
    /// Success only when system providers contain limit/window fields and custom providers satisfy the full direct contract.
    /// 仅当系统供应商只包含限制/窗口字段，或自定义供应商满足完整直接契约时成功。
    pub fn validate_for_provider(&self, provider_id: &str) -> Result<(), String> {
        self.validate()?;
        if provider_id.starts_with("custom:") || !self.has_provider_contract_overrides() {
            return Ok(());
        }
        Err(String::from(
            "System provider capability overrides are limited to context, output, and context-window policy settings.",
        ))
    }

    /// Reports whether this layer attempts to replace immutable provider-declared capabilities.
    /// 报告此层是否尝试替换不可变的供应商声明能力。
    ///
    /// # Returns
    /// 返回值
    /// `true` for media input, structured output, reasoning, or request-projection fields.
    /// 当存在媒体输入、结构化输出、思考或请求投影字段时返回 `true`。
    fn has_provider_contract_overrides(&self) -> bool {
        self.input_resources.is_some()
            || self.structured_output.is_some()
            || self.output_resources.is_some()
            || self.reasoning.is_some()
            || self.reasoning_control.is_some()
            || self.reasoning_efforts.is_some()
            || self.request_projection.is_some()
    }

    /// Removes fields that system providers cannot own and reports whether the record changed.
    /// 移除系统供应商不能持有的字段，并报告记录是否发生变化。
    ///
    /// # Returns
    /// 返回值
    /// `true` when at least one immutable provider-contract field was removed.
    /// 当至少移除一个不可变供应商契约字段时返回 `true`。
    fn remove_system_provider_contract_overrides(&mut self) -> bool {
        let changed = self.has_provider_contract_overrides();
        self.input_resources = None;
        self.structured_output = None;
        self.output_resources = None;
        self.reasoning = None;
        self.reasoning_control = None;
        self.reasoning_efforts = None;
        self.request_projection = None;
        changed
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
    allow_provider_contract_overrides: bool,
) {
    if let Some(context_limit) = overrides.context_limit {
        capabilities.context_limit = Some(context_limit);
    }
    if let Some(output_limit) = overrides.output_limit {
        capabilities.output_limit = Some(output_limit);
    }
    if !allow_provider_contract_overrides {
        return;
    }
    if let Some(input_resources) = overrides.input_resources.as_ref() {
        capabilities.input_resources = input_resources.clone();
    }
    if let Some(structured_output) = overrides.structured_output {
        capabilities.structured_output = structured_output;
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

/// Selects the single backend used by one media-understanding capability.
/// 选择一项媒体理解能力使用的唯一后端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaUnderstandingBackendSelection {
    /// Executes one exact Manifest-owned provider tool target.
    /// 执行一个由 Manifest 持有的精确供应商工具目标。
    ProviderTool {
        /// Exact provider target and credential selection.
        /// 精确供应商目标与凭据选择。
        selection: ProviderToolTargetSelection,
    },
    /// Executes one ordinary multimodal conversation model as the replacement backend.
    /// 使用一个普通多模态会话模型作为替代理解后端。
    ModelReplacement {
        /// Exact model and credential selection.
        /// 精确模型与凭据选择。
        selection: ActiveAiModelSelection,
        /// Optional exact reasoning level selected for the replacement model.
        /// 为替代模型选择的可选精确思考档位。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_effort: Option<ReasoningEffort>,
    },
}

/// Identifies one of the two supported media-understanding backend families.
/// 标识两种受支持的媒体理解后端类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaUnderstandingBackendMode {
    /// Use the provider-specific Manifest-owned understanding endpoint.
    /// 使用供应商专用的 Manifest 理解端点。
    ProviderTool,
    /// Use an ordinary multimodal model as the replacement backend.
    /// 使用普通多模态模型作为替代后端。
    ModelReplacement,
}

/// Stores one exact model-replacement route independently from the active backend mode.
/// 独立保存一条模型代替路由，不受当前活动后端模式影响。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaUnderstandingModelReplacementSelection {
    /// Exact provider, model, and credential route.
    /// 精确的供应商、模型与凭据路由。
    pub selection: ActiveAiModelSelection,
    /// Optional reasoning effort for the replacement model.
    /// 替代模型可选的思考级别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

/// Stores the independently configured provider and replacement routes for one media capability.
/// 保存一项媒体能力分别配置的供应商专用与模型代替路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaUnderstandingBackendOptions {
    /// Exact provider-specific target, when configured.
    /// 已配置的精确供应商专用目标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tool: Option<ProviderToolTargetSelection>,
    /// Exact multimodal replacement-model route, when configured.
    /// 已配置的精确多模态替代模型路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_replacement: Option<MediaUnderstandingModelReplacementSelection>,
}

impl MediaUnderstandingBackendOptions {
    /// Records one complete backend selection in its matching mode slot.
    /// 将一条完整后端选择写入对应的模式槽位。
    pub fn update_from_selection(&mut self, selection: &MediaUnderstandingBackendSelection) {
        match selection {
            MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                self.provider_tool = Some(selection.clone());
            }
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection,
                reasoning_effort,
            } => {
                self.model_replacement = Some(MediaUnderstandingModelReplacementSelection {
                    selection: selection.clone(),
                    reasoning_effort: *reasoning_effort,
                });
            }
        }
    }

    /// Builds the exact backend selection stored for one backend mode.
    /// 构建指定后端模式中保存的精确后端选择。
    pub fn selection_for_mode(
        &self,
        mode: MediaUnderstandingBackendMode,
    ) -> Option<MediaUnderstandingBackendSelection> {
        match mode {
            MediaUnderstandingBackendMode::ProviderTool => self
                .provider_tool
                .clone()
                .map(|selection| MediaUnderstandingBackendSelection::ProviderTool { selection }),
            MediaUnderstandingBackendMode::ModelReplacement => {
                self.model_replacement.clone().map(|route| {
                    MediaUnderstandingBackendSelection::ModelReplacement {
                        selection: route.selection,
                        reasoning_effort: route.reasoning_effort,
                    }
                })
            }
        }
    }

    /// Reports whether no backend route has been configured for this capability.
    /// 报告该能力是否尚未配置任何后端路由。
    pub fn is_empty(&self) -> bool {
        self.provider_tool.is_none() && self.model_replacement.is_none()
    }
}

impl MediaUnderstandingBackendSelection {
    /// Returns the backend family represented by this exact selection.
    /// 返回该精确后端选择所属的后端类型。
    pub fn mode(&self) -> MediaUnderstandingBackendMode {
        match self {
            Self::ProviderTool { .. } => MediaUnderstandingBackendMode::ProviderTool,
            Self::ModelReplacement { .. } => MediaUnderstandingBackendMode::ModelReplacement,
        }
    }
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
    /// Exact reasoning effort selected independently for each restricted LLM purpose.
    /// 为每个受限 LLM 用途独立选择的准确思考档位。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub llm_reasoning_efforts: BTreeMap<String, ReasoningEffort>,
    /// Optional configured embedding route.
    /// 可选的已配置向量路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<InferenceRouteConfiguration>,
    /// Exact consumer-owned embedding dimensions; absence preserves the existing consumer policy.
    /// 消费者独立选择的明确向量维度；缺省时保持该消费者原有策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<u32>,
    /// Optional configured rerank route.
    /// 可选的已配置重排路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank: Option<InferenceRouteConfiguration>,
}

#[path = "context_engine_embedding_policy.rs"]
mod context_engine_embedding_policy;
pub use context_engine_embedding_policy::{
    ContextEngineEmbeddingCompatibilityRecord, ContextEngineEmbeddingDecision,
    ContextEngineEmbeddingScope,
};

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

/// Stores one immutable-identity context-compaction window node and its editable token value.
/// 保存一个身份不可变的上下文压缩窗口节点及其可编辑 Token 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextCompactionWindowNode {
    /// Stable code-owned node identifier persisted by sessions.
    /// 由代码持有且被会话持久化的稳定节点标识。
    pub id: ContextCompactionWindowNodeId,
    /// User-editable compaction-window token value for this node.
    /// 当前节点由用户编辑的压缩窗口 Token 值。
    pub tokens: u64,
}

/// Owns the exact five system context-compaction nodes in immutable product order.
/// 按不可变产品顺序持有恰好五个系统上下文压缩节点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextCompactionWindowConfiguration {
    /// Complete fixed node array; array length prevents add and delete operations.
    /// 完整固定节点数组；数组长度从结构上禁止新增与删除。
    pub nodes: [ContextCompactionWindowNode; 5],
}

impl Default for ContextCompactionWindowConfiguration {
    /// Builds the official ascending 200K, 256K, 272K, 373K, and 500K node values.
    /// 构建官方严格递增的 200K、256K、272K、373K 与 500K 节点值。
    fn default() -> Self {
        Self {
            nodes: [
                ContextCompactionWindowNode {
                    id: ContextCompactionWindowNodeId::CompressionWindow1,
                    tokens: 200_000,
                },
                ContextCompactionWindowNode {
                    id: ContextCompactionWindowNodeId::CompressionWindow2,
                    tokens: 256_000,
                },
                ContextCompactionWindowNode {
                    id: ContextCompactionWindowNodeId::CompressionWindow3,
                    tokens: 272_000,
                },
                ContextCompactionWindowNode {
                    id: ContextCompactionWindowNodeId::CompressionWindow4,
                    tokens: 373_000,
                },
                ContextCompactionWindowNode {
                    id: ContextCompactionWindowNodeId::CompressionWindow5,
                    tokens: 500_000,
                },
            ],
        }
    }
}

impl ContextCompactionWindowConfiguration {
    /// Validates fixed identities, immutable order, safe positive values, and strict ascent.
    /// 校验固定身份、不可变顺序、安全正整数值与严格递增关系。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` for one complete usable configuration, otherwise one explicit invariant error.
    /// 完整可用时返回 `Ok(())`，否则返回一条明确的不变量错误。
    pub fn validate(&self) -> Result<(), String> {
        for (index, (node, expected_id)) in self
            .nodes
            .iter()
            .zip(ContextCompactionWindowNodeId::ALL)
            .enumerate()
        {
            if node.id != expected_id {
                return Err(format!(
                    "Context compaction window node {} must use id `{}`.",
                    index + 1,
                    expected_id.as_str()
                ));
            }
            if node.tokens == 0 || node.tokens > JAVASCRIPT_MAX_SAFE_INTEGER_U64 {
                return Err(format!(
                    "Context compaction window node `{}` tokens must be a positive JavaScript-safe integer.",
                    node.id.as_str()
                ));
            }
        }
        for pair in self.nodes.windows(2) {
            if pair[0].tokens >= pair[1].tokens {
                return Err(format!(
                    "Context compaction window nodes must be strictly ascending: `{}` must be lower than `{}`.",
                    pair[0].id.as_str(),
                    pair[1].id.as_str()
                ));
            }
        }
        Ok(())
    }

    /// Resolves one stable node identifier to its current authoritative token value.
    /// 将一个稳定节点标识解析为当前权威 Token 值。
    ///
    /// # Parameters
    /// 参数
    /// `node_id` is the exact session-persisted fixed identifier.
    /// `node_id` 是会话持久化的精确固定标识。
    ///
    /// # Returns
    /// 返回值
    /// The current token value, or an explicit invariant error if configuration identity drifted.
    /// 返回当前 Token 值；若配置身份发生漂移则返回明确的不变量错误。
    pub fn tokens_for(&self, node_id: ContextCompactionWindowNodeId) -> Result<u64, String> {
        let node = &self.nodes[node_id.index()];
        if node.id != node_id {
            return Err(format!(
                "Context compaction window configuration does not contain `{}` at its fixed index.",
                node_id.as_str()
            ));
        }
        Ok(node.tokens)
    }
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
    /// Explicit media policy; absence keeps defaults in memory without rewriting configuration.
    /// 显式媒体策略；缺失时仅在内存使用默认值，不回写配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_delivery_policy: Option<crate::MediaDeliveryPolicy>,
    /// Sparse user-defined provider priorities; missing entries inherit zero.
    /// 稀疏的用户自定义供应商优先级；缺失项继承零。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_priority_overrides: BTreeMap<String, i32>,
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
    /// Credential connections explicitly disabled while retaining their persisted secrets and metadata.
    /// 在保留持久化秘密与元数据时被显式停用的凭据连接。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_connection_ids: BTreeSet<String>,
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
    /// User conversation-publication choices that differ from the provider default policy.
    /// 与供应商默认策略不同的用户模型会话发布选择。
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
    /// Fixed system context-compaction window nodes with user-editable token values.
    /// Token 值可由用户编辑的固定系统上下文压缩窗口节点。
    #[serde(default)]
    pub context_compaction_window_nodes: ContextCompactionWindowConfiguration,
    /// Provider-scoped sparse Codex context-window defaults.
    /// 供应商级稀疏 Codex 上下文窗口默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
    /// Exact provider-model sparse Codex context-window defaults.
    /// 精确供应商模型级稀疏 Codex 上下文窗口默认值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
    /// Explicit non-conversation internal-use choices scoped by provider and model.
    /// 按供应商与模型限定作用域的显式非会话内部使用选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub internal_model_enabled_overrides: BTreeMap<String, bool>,
    /// Independent inference routes grouped by stable consumer profile identifier.
    /// 按稳定消费者配置档标识分组的独立推理路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inference_profiles: BTreeMap<String, InferenceProfile>,
    /// Exact user-reviewed ContextEngine embedding compatibility; never shared with VMM or knowledge.
    /// 用户审查的精确 ContextEngine 向量兼容关系；绝不与 VMM 或知识库共享。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_engine_embedding_compatibility: Vec<ContextEngineEmbeddingCompatibilityRecord>,
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
    /// Models explicitly allowed to appear in image, video, or audio understanding candidates.
    /// 明确允许进入图片、视频或音频理解候选列表的模型。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub multimodal_understanding_candidate_models: BTreeSet<String>,
    /// One exact backend selection per media-understanding capability.
    /// 每项媒体理解能力唯一的精确后端选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub media_understanding_selections:
        BTreeMap<ProviderToolCapabilityKind, MediaUnderstandingBackendSelection>,
    /// Independently configured provider and model-replacement routes per media capability.
    /// 每项媒体能力分别配置的供应商专用与模型代替路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub media_understanding_backend_options:
        BTreeMap<ProviderToolCapabilityKind, MediaUnderstandingBackendOptions>,
    /// Explicit native model-extension targets selected per extension capability.
    /// 按扩展能力显式选择的原生模型扩展目标。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Legacy target-scoped disabled state read only during schema migration.
    /// 仅在 Schema 迁移期间读取的旧逐目标禁用状态。
    #[serde(default, skip_serializing)]
    pub provider_tool_disabled_targets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
}

/// Returns the default thirty-minute account-usage refresh interval.
/// 返回默认三十分钟账号用量刷新间隔。
fn default_usage_refresh_interval_seconds() -> u64 {
    1_800
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
    /// Builds one configuration with no providers and a thirty-minute usage refresh interval.
    /// 构建一个不含供应商且用量刷新间隔为三十分钟的配置。
    fn default() -> Self {
        Self {
            schema_version: PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION,
            usage_refresh_interval_seconds: default_usage_refresh_interval_seconds(),
            media_delivery_policy: None,
            provider_priority_overrides: BTreeMap::new(),
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
            disabled_connection_ids: BTreeSet::new(),
            active: None,
            connection_graph: PersistedConnectionGraph::default(),
            model_overrides: BTreeMap::new(),
            model_enabled_overrides: BTreeMap::new(),
            model_capability_overrides: BTreeMap::new(),
            provider_capability_overrides: BTreeMap::new(),
            context_window_policy_overrides: ContextWindowPolicyOverrides::default(),
            context_compaction_window_nodes: ContextCompactionWindowConfiguration::default(),
            provider_context_window_policy_overrides: BTreeMap::new(),
            model_context_window_policy_overrides: BTreeMap::new(),
            internal_model_enabled_overrides: BTreeMap::new(),
            inference_profiles: BTreeMap::from([
                (String::from("vce"), InferenceProfile::default()),
                (String::from("vmm"), InferenceProfile::default()),
            ]),
            context_engine_embedding_compatibility: Vec::new(),
            internal_model_overrides: BTreeMap::new(),
            secondary_routes: BTreeMap::new(),
            provider_tool_selections: BTreeMap::new(),
            provider_tool_enabled_capabilities: BTreeSet::new(),
            multimodal_understanding_candidate_models: BTreeSet::new(),
            media_understanding_selections: BTreeMap::new(),
            media_understanding_backend_options: BTreeMap::new(),
            provider_extension_target_sets: BTreeMap::new(),
            provider_tool_disabled_targets: BTreeMap::new(),
        }
    }
}

impl PersistedAiConfiguration {
    /// Applies one session-selected media backend mode to this per-turn configuration copy.
    /// 将会话选定的媒体后端模式应用到当前回合配置副本。
    ///
    /// # Parameters
    /// 参数
    /// `capability` identifies one fixed media-understanding facade and `mode` selects its
    /// independently configured provider or model-replacement route.
    /// `capability` 标识一个固定媒体理解外壳，`mode` 选择其分别配置的供应商或模型代替路由。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the exact mode route exists; otherwise an explicit configuration error.
    /// 精确模式路由存在时返回 `Ok(())`；否则返回明确配置错误。
    pub fn apply_media_understanding_mode(
        &mut self,
        capability: ProviderToolCapabilityKind,
        mode: MediaUnderstandingBackendMode,
    ) -> Result<(), String> {
        if !matches!(
            capability,
            ProviderToolCapabilityKind::ImageUnderstand
                | ProviderToolCapabilityKind::VideoUnderstand
                | ProviderToolCapabilityKind::AudioUnderstand
        ) {
            return Err(format!(
                "Capability `{capability:?}` is not a media-understanding capability."
            ));
        }
        let selection = self
            .media_understanding_backend_options
            .get(&capability)
            .and_then(|options| options.selection_for_mode(mode))
            .ok_or_else(|| {
                format!(
                    "Media-understanding capability `{capability:?}` has no configured `{mode:?}` backend."
                )
            })?;
        self.media_understanding_selections
            .insert(capability, selection);
        Ok(())
    }

    /// Applies one explicit session reasoning override to the active model-replacement backend.
    /// 将一项显式会话推理档位覆盖应用到活动的模型代替后端。
    ///
    /// # Parameters
    /// 参数
    /// `capability` identifies the media-understanding facade and `reasoning_effort` is the exact
    /// provider-neutral override selected for the session.
    /// `capability` 标识媒体理解外壳，`reasoning_effort` 是当前会话选择的精确供应商无关覆盖值。
    ///
    /// # Returns
    /// 返回值
    /// Success when the active backend is model replacement, otherwise an explicit mode error.
    /// 活动后端为模型代替时返回成功，否则返回明确的模式错误。
    pub fn apply_media_understanding_reasoning_effort(
        &mut self,
        capability: ProviderToolCapabilityKind,
        reasoning_effort: ReasoningEffort,
    ) -> Result<(), String> {
        let selection = self
            .media_understanding_selections
            .get_mut(&capability)
            .ok_or_else(|| {
                format!("Media-understanding capability `{capability:?}` has no active backend.")
            })?;
        match selection {
            MediaUnderstandingBackendSelection::ModelReplacement {
                reasoning_effort: selected_effort,
                ..
            } => {
                *selected_effort = Some(reasoning_effort);
                Ok(())
            }
            MediaUnderstandingBackendSelection::ProviderTool { .. } => Err(format!(
                "Media-understanding capability `{capability:?}` cannot apply a reasoning override to a provider-tool backend."
            )),
        }
    }

    /// Copies active media selections into missing independent backend mode slots.
    /// 将活动媒体选择补入缺失的独立后端模式槽位。
    ///
    /// # Returns
    /// 返回值
    /// `true` when persisted configuration data was normalized.
    /// 持久化配置数据发生归一化时返回 `true`。
    fn normalize_media_understanding_backend_options(&mut self) -> bool {
        let active_selections = self.media_understanding_selections.clone();
        let mut changed = false;
        for (capability, selection) in active_selections {
            if !matches!(
                capability,
                ProviderToolCapabilityKind::ImageUnderstand
                    | ProviderToolCapabilityKind::VideoUnderstand
                    | ProviderToolCapabilityKind::AudioUnderstand
            ) {
                continue;
            }
            let options = self
                .media_understanding_backend_options
                .entry(capability)
                .or_default();
            let previous = options.clone();
            options.update_from_selection(&selection);
            changed |= *options != previous;
        }
        self.media_understanding_backend_options
            .retain(|capability, options| {
                let keep = matches!(
                    capability,
                    ProviderToolCapabilityKind::ImageUnderstand
                        | ProviderToolCapabilityKind::VideoUnderstand
                        | ProviderToolCapabilityKind::AudioUnderstand
                ) && !options.is_empty();
                if !keep {
                    changed = true;
                }
                keep
            });
        changed
    }

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
            .disabled_connection_ids
            .retain(|connection_id| !removed_connection_ids.contains(connection_id.as_str()));
        projection
            .connection_model_access_policies
            .retain(|connection_id, _| !removed_connection_ids.contains(connection_id.as_str()));
        projection.provider_tool_selections.retain(|_, selection| {
            !removed_connection_ids.contains(selection.connection_id.as_str())
        });
        projection
            .provider_extension_target_sets
            .retain(|_, selections| {
                selections.retain(|selection| {
                    !removed_connection_ids.contains(selection.connection_id.as_str())
                });
                !selections.is_empty()
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
        let source_schema_version = self.schema_version;
        let mut changed = match self.schema_version {
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
            10..=18 => {
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
        if source_schema_version < 14 {
            // Provider-tool selection semantics changed incompatibly in schema 14. Old scalar and
            // mixed-media selections are intentionally discarded instead of being guessed into
            // the new direct-selection and model-extension stores.
            // Provider Tool 选择语义在 Schema 14 中发生不兼容变化。旧标量与混合媒体选择会被
            // 有意丢弃，而不是猜测转换到新的指定直连与模型扩展存储。
            self.provider_tool_selections.clear();
            self.provider_tool_enabled_capabilities.clear();
            self.provider_extension_target_sets.clear();
            self.provider_tool_disabled_targets.clear();
        }
        if source_schema_version < 15 {
            // Schema 15 replaces operation-scoped truthy-only internal model keys with explicit
            // provider/model booleans. Historical keys are intentionally discarded because an
            // absent false state cannot be reconstructed without guessing user intent.
            // Schema 15 使用显式供应商/模型布尔值取代按操作且仅保存 true 的内部模型键。
            // 旧结构无法还原缺失的 false 状态，因此有意丢弃，避免猜测用户意图。
            self.internal_model_enabled_overrides.clear();
        }
        if source_schema_version < 18 {
            // Schema 18 separates media-understanding backend selection from direct search/extract
            // targets. Only the historical image-understanding target has an unambiguous migration;
            // video and audio remain unconfigured instead of being inferred from adjacent state.
            // Schema 18 将媒体理解后端选择从搜索/抓取直连目标中分离。只有历史图片理解目标
            // 具备无歧义迁移；视频与音频保持未配置，不从相邻状态推断。
            if let Some(selection) = self
                .provider_tool_selections
                .remove(&ProviderToolCapabilityKind::ImageUnderstand)
            {
                self.media_understanding_selections.insert(
                    ProviderToolCapabilityKind::ImageUnderstand,
                    MediaUnderstandingBackendSelection::ProviderTool { selection },
                );
            } else {
                self.provider_tool_enabled_capabilities
                    .remove(&ProviderToolCapabilityKind::ImageUnderstand);
            }
            self.provider_tool_selections
                .remove(&ProviderToolCapabilityKind::VideoUnderstand);
            self.provider_tool_selections
                .remove(&ProviderToolCapabilityKind::AudioUnderstand);
            self.provider_tool_enabled_capabilities
                .remove(&ProviderToolCapabilityKind::VideoUnderstand);
            self.provider_tool_enabled_capabilities
                .remove(&ProviderToolCapabilityKind::AudioUnderstand);
        }
        // insertedVceProfile ensures every supported schema exposes the system VCE consumer
        // without copying or guessing another consumer's user-owned routes.
        // insertedVceProfile 确保每个受支持 Schema 都暴露系统 VCE consumer，且不会复制或猜测
        // 其他 consumer 的用户自有路由。
        let inserted_vce_profile = if self.inference_profiles.contains_key("vce") {
            false
        } else {
            self.inference_profiles
                .insert(String::from("vce"), InferenceProfile::default());
            true
        };
        changed |= inserted_vce_profile;
        // normalizedMediaBackends preserves the active legacy selection while creating the
        // independent mode slot required by session-level backend selection.
        // normalizedMediaBackends 在保留旧活动选择的同时，创建会话级后端选择所需的独立模式槽位。
        changed |= self.normalize_media_understanding_backend_options();
        self.context_compaction_window_nodes.validate()?;
        self.validate_context_window_policy_layers()?;
        self.effective_media_delivery_policy().validate()?;
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
        for (key, overrides) in &mut self.model_capability_overrides {
            let system_provider = Self::provider_model_key_parts(key)
                .is_some_and(|(provider_id, _)| !provider_id.starts_with("custom:"));
            changed |= if system_provider {
                overrides.remove_system_provider_contract_overrides()
            } else {
                overrides.output_resources.take().is_some()
            };
        }
        for (provider_id, overrides) in &mut self.provider_capability_overrides {
            changed |= if provider_id.starts_with("custom:") {
                overrides.output_resources.take().is_some()
            } else {
                overrides.remove_system_provider_contract_overrides()
            };
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

    /// Reports whether one exact provider model is enabled as a multimodal-understanding candidate.
    /// 报告一个精确供应商模型是否已启用为多模态理解候选。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the exact user-owned candidate switch.
    /// `provider_id` 与 `model_id` 标识精确的用户候选开关。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the user explicitly enabled the model.
    /// 仅当用户明确启用该模型时返回 `true`。
    pub fn multimodal_understanding_candidate_is_enabled(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> bool {
        self.multimodal_understanding_candidate_models
            .contains(Self::provider_model_key(provider_id, model_id).as_str())
    }

    /// Stores one exact multimodal-understanding candidate switch and clears disabled replacement routes.
    /// 保存一个精确多模态理解候选开关，并清理已禁用模型的替代路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the model, and `enabled` is the requested state.
    /// `provider_id` 与 `model_id` 标识模型，`enabled` 是请求状态。
    pub fn set_multimodal_understanding_candidate_enabled(
        &mut self,
        provider_id: &str,
        model_id: &str,
        enabled: bool,
    ) {
        let key = Self::provider_model_key(provider_id, model_id);
        if enabled {
            self.multimodal_understanding_candidate_models.insert(key);
        } else {
            self.multimodal_understanding_candidate_models
                .remove(key.as_str());
            // A disabled model cannot remain a latent backend that silently reactivates when the
            // candidate switch is enabled later; every affected media kind requires reselection.
            // 已禁用模型不能作为潜在后端，在未来重新开启候选开关时静默恢复；所有受影响媒体类型
            // 都必须重新选择。
            self.media_understanding_selections.retain(|_, backend| {
                !matches!(
                    backend,
                    MediaUnderstandingBackendSelection::ModelReplacement { selection, .. }
                        if selection.provider_id == provider_id && selection.model_id == model_id
                )
            });
            self.media_understanding_backend_options
                .retain(|_, options| {
                    if options.model_replacement.as_ref().is_some_and(|route| {
                        route.selection.provider_id == provider_id
                            && route.selection.model_id == model_id
                    }) {
                        options.model_replacement = None;
                    }
                    !options.is_empty()
                });
        }
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

    /// Replaces all five fixed context-compaction window values after strict validation.
    /// 在严格校验后整体替换五个固定上下文压缩窗口值。
    ///
    /// # Parameters
    /// 参数
    /// `nodes` is the complete fixed configuration submitted by the settings surface.
    /// `nodes` 是设置界面提交的完整固定配置。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` after an atomic in-memory replacement, otherwise a validation error.
    /// 原子完成内存替换后返回 `Ok(())`，否则返回校验错误。
    pub fn set_context_compaction_window_nodes(
        &mut self,
        nodes: ContextCompactionWindowConfiguration,
    ) -> Result<(), String> {
        nodes.validate()?;
        self.context_compaction_window_nodes = nodes;
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
        compression_window_node_id: Option<ContextCompactionWindowNodeId>,
        session_policy: Option<ContextCompactionPolicy>,
    ) -> Result<ContextCompactionPolicy, String> {
        let sparse = self.context_window_policy_sparse_for_route(provider_id, model_id);
        let resolved_window = sparse.resolve();
        // A fully explicit seed already froze both the node identity and its resolved value.
        // 完整显式种子已经冻结节点身份及其解析值。
        let frozen_selection = session_policy
            .filter(|policy| policy.window_policy_is_explicit)
            .map(|policy| {
                (
                    policy.compression_window_node_id,
                    policy.compression_window_limit,
                )
            });
        let (resolved_node_id, resolved_node_limit) = match frozen_selection {
            Some(selection) => selection,
            None => match compression_window_node_id {
                Some(node_id) => {
                    let tokens = self.context_compaction_window_nodes.tokens_for(node_id)?;
                    let tokens = usize::try_from(tokens).map_err(|_| {
                        format!(
                            "Context compaction window node `{}` exceeds this platform's token range.",
                            node_id.as_str()
                        )
                    })?;
                    (Some(node_id), Some(tokens))
                }
                None => (None, None),
            },
        };
        if resolved_node_id.is_some() != resolved_node_limit.is_some() {
            return Err(String::from(
                "A frozen context compaction window must contain both node id and resolved token limit.",
            ));
        }
        if let (Some(node_id), Some(node_limit)) = (resolved_node_id, resolved_node_limit) {
            let model_limit = context_limit.ok_or_else(|| {
                format!(
                    "Context compaction window node `{}` requires a model-declared context limit.",
                    node_id.as_str()
                )
            })?;
            if node_limit >= model_limit {
                return Err(format!(
                    "Context compaction window node `{}` resolves to {} tokens, which must be lower than model context limit {}.",
                    node_id.as_str(),
                    node_limit,
                    model_limit
                ));
            }
        }
        let resolved_tail = vulcan_context::resolve_context_compaction_tail_policy(
            resolved_node_limit.or(context_limit),
        );
        if let Some(session_policy) = session_policy {
            if session_policy.window_policy_is_explicit {
                return Ok(ContextCompactionPolicy {
                    tail_turns: resolved_tail.tail_segments,
                    preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
                    tail_context_limit: resolved_tail.context_limit,
                    tail_policy_source: resolved_tail.source,
                    compression_window_node_id: resolved_node_id,
                    compression_window_limit: resolved_node_limit,
                    ..session_policy
                });
            }
            return Ok(ContextCompactionPolicy {
                tail_turns: resolved_tail.tail_segments,
                preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
                tail_context_limit: resolved_tail.context_limit,
                tail_policy_source: resolved_tail.source,
                compression_window_node_id: resolved_node_id,
                compression_window_limit: resolved_node_limit,
                window: resolved_window,
                window_policy_is_explicit: true,
                ..session_policy
            });
        }
        Ok(ContextCompactionPolicy {
            tail_turns: resolved_tail.tail_segments,
            preserve_recent_tokens: resolved_tail.preserve_recent_tokens,
            tail_context_limit: resolved_tail.context_limit,
            tail_policy_source: resolved_tail.source,
            compression_window_node_id: resolved_node_id,
            compression_window_limit: resolved_node_limit,
            window: resolved_window,
            window_policy_is_explicit: true,
            ..ContextCompactionPolicy::default()
        })
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

    /// Resolves account access from an explicit policy or the model's signed initial default.
    /// 从显式账号策略或模型的签名初始默认值解析账号访问。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` select one account model; `inherited_default` is frozen
    /// from its exact manifest scope when no user-owned account policy exists.
    /// `connection_id` 与 `model_id` 选定一个账号模型；没有用户自有账号策略时，
    /// `inherited_default` 来自精确清单作用域的冻结默认值。
    ///
    /// # Returns
    /// 返回值
    /// The persisted account decision when present, otherwise the signed initial default.
    /// 已持久化账号决定优先，否则返回签名初始默认值。
    pub fn connection_model_is_enabled_with_default(
        &self,
        connection_id: &str,
        model_id: &str,
        inherited_default: bool,
    ) -> bool {
        self.connection_model_access_policies
            .get(connection_id)
            .map_or(inherited_default, |policy| {
                policy.model_is_enabled(model_id)
            })
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

    /// Exposes all account models in a temporary support-only configuration.
    /// 在临时且仅供支持目录使用的配置中暴露账号全部模型。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the account whose access filter must be bypassed in a clone.
    /// `connection_id` 标识要在配置副本中绕过访问筛选的账号。
    pub fn allow_all_connection_models_for_support(&mut self, connection_id: &str) {
        self.connection_model_access_policies.insert(
            connection_id.to_string(),
            ConnectionModelAccessPolicy::default(),
        );
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

    /// Reports whether one persisted credential connection may participate in runtime work.
    /// 报告一个持久化凭据连接是否可以参与运行时工作。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` is the exact stable credential connection identifier.
    /// `connection_id` 是精确稳定的凭据连接标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` unless the user explicitly disabled this connection.
    /// 除非用户显式停用此连接，否则返回 `true`。
    pub fn connection_is_enabled(&self, connection_id: &str) -> bool {
        !self.disabled_connection_ids.contains(connection_id)
    }

    /// Persists one credential connection's reversible enabled state.
    /// 持久化一个凭据连接可逆的启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` is the configured connection and `enabled` is its requested runtime state.
    /// `connection_id` 是已配置连接，`enabled` 是请求的运行时状态。
    ///
    /// # Returns
    /// 返回值
    /// An error when the exact connection does not exist; otherwise no value.
    /// 精确连接不存在时返回错误，否则无返回值。
    pub fn set_connection_enabled(
        &mut self,
        connection_id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        if !self
            .connection_graph
            .connections
            .contains_key(connection_id)
        {
            return Err(format!("Connection `{connection_id}` is not configured."));
        }
        if enabled {
            self.disabled_connection_ids.remove(connection_id);
        } else {
            self.disabled_connection_ids
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
        }
        Ok(())
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
        // Signed system capability limits belong to the Manifest; this layer serves custom providers only.
        // 受签名系统模型的能力上限归清单所有；此层只服务于自定义供应商。
        if !provider_id.starts_with("custom:") {
            return;
        }
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
        if !evidence.structured_output_declared
            && let Some(structured_output) = overrides.structured_output
        {
            capabilities.structured_output = structured_output;
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
        // System identities and user-defined system models carry their own authoritative capabilities.
        // 系统身份与用户手工定义的系统模型各自持有权威能力声明。
        if !provider_id.starts_with("custom:") {
            return;
        }
        let Some(overrides) = self.model_capability_overrides(provider_id, model_id) else {
            return;
        };
        apply_capability_overrides(overrides, capabilities, true);
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
        self.multimodal_understanding_candidate_models
            .remove(provider_model_key.as_str());
        self.media_understanding_selections.retain(|_, backend| {
            !matches!(
                backend,
                MediaUnderstandingBackendSelection::ModelReplacement { selection, .. }
                    if selection.provider_id == provider_id && selection.model_id == model_id
            )
        });
        self.media_understanding_backend_options
            .retain(|_, options| {
                if options.model_replacement.as_ref().is_some_and(|route| {
                    route.selection.provider_id == provider_id
                        && route.selection.model_id == model_id
                }) {
                    options.model_replacement = None;
                }
                !options.is_empty()
            });
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
            .remove(provider_model_key.as_str());
        self.internal_model_overrides
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
            || self
                .multimodal_understanding_candidate_models
                .iter()
                .any(|key| key.starts_with(provider_model_prefix.as_str()))
            || self
                .media_understanding_selections
                .values()
                .any(|backend| match backend {
                    MediaUnderstandingBackendSelection::ProviderTool { selection } => self
                        .connection_graph
                        .connections
                        .get(selection.connection_id.as_str())
                        .is_some_and(|connection| connection.provider_id == provider_id),
                    MediaUnderstandingBackendSelection::ModelReplacement { selection, .. } => {
                        selection.provider_id == provider_id
                    }
                })
            || self
                .media_understanding_backend_options
                .values()
                .any(|options| {
                    options.provider_tool.as_ref().is_some_and(|selection| {
                        self.connection_graph
                            .connections
                            .get(selection.connection_id.as_str())
                            .is_some_and(|connection| connection.provider_id == provider_id)
                    }) || options
                        .model_replacement
                        .as_ref()
                        .is_some_and(|route| route.selection.provider_id == provider_id)
                })
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
        // providerConnectionIds identifies provider-tool backends owned by this provider.
        // providerConnectionIds 标识由当前供应商持有的供应商工具后端。
        let provider_connection_ids = self
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| connection.id.clone())
            .collect::<BTreeSet<_>>();
        self.custom_models.remove(provider_id);
        self.custom_provider_model_defaults.remove(provider_id);
        self.provider_capability_overrides.remove(provider_id);
        self.provider_priority_overrides.remove(provider_id);
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
        self.multimodal_understanding_candidate_models
            .retain(|key| !key.starts_with(provider_model_prefix.as_str()));
        self.media_understanding_selections
            .retain(|_, backend| match backend {
                MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                    !provider_connection_ids.contains(selection.connection_id.as_str())
                }
                MediaUnderstandingBackendSelection::ModelReplacement { selection, .. } => {
                    selection.provider_id != provider_id
                }
            });
        self.media_understanding_backend_options
            .retain(|_, options| {
                if options.provider_tool.as_ref().is_some_and(|selection| {
                    provider_connection_ids.contains(selection.connection_id.as_str())
                }) {
                    options.provider_tool = None;
                }
                if options
                    .model_replacement
                    .as_ref()
                    .is_some_and(|route| route.selection.provider_id == provider_id)
                {
                    options.model_replacement = None;
                }
                !options.is_empty()
            });
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
            // Purpose-scoped reasoning belongs to an exact surviving LLM route and must not
            // remain as detached configuration after its provider is removed.
            // 用途级思考配置属于准确存续的 LLM 路由，供应商移除后不得留下孤立配置。
            profile
                .llm_reasoning_efforts
                .retain(|purpose_id, _| profile.llm_routes.contains_key(purpose_id));
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

    /// Reports whether one model is enabled for non-conversation internal inference.
    /// 报告一个模型是否已为非会话内部推理启用。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify one exact model; `default_enabled` is the stable
    /// provider-wide suggestion aggregated from configured credential profiles.
    /// `provider_id` 与 `model_id` 标识一个精确模型；`default_enabled` 是从已配置凭据档
    /// 聚合得到的稳定供应商级建议值。
    ///
    /// # Returns
    /// 返回值
    /// The explicit user state when present, otherwise the supplied Manifest default.
    /// 存在显式用户状态时返回该状态，否则返回传入的 Manifest 默认值。
    pub fn internal_model_is_enabled(
        &self,
        provider_id: &str,
        model_id: &str,
        default_enabled: bool,
    ) -> bool {
        self.internal_model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
            .unwrap_or(default_enabled)
    }

    /// Returns one explicit internal-use override without applying Manifest defaults.
    /// 返回一项未应用 Manifest 默认值的显式内部使用覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify one exact model.
    /// `provider_id` 与 `model_id` 标识一个精确模型。
    ///
    /// # Returns
    /// 返回值
    /// The explicit state or `None` when the selected credential default remains authoritative.
    /// 返回显式状态；所选凭据默认值仍具权威时返回 `None`。
    pub fn internal_model_enabled_override(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<bool> {
        self.internal_model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
    }

    /// Stores one explicit non-conversation internal-use state.
    /// 保存一项显式非会话内部使用状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `enabled` identify the exact state.
    /// `provider_id`、`model_id` 与 `enabled` 标识精确状态。
    pub fn set_internal_model_enabled(&mut self, provider_id: &str, model_id: &str, enabled: bool) {
        let key = Self::provider_model_key(provider_id, model_id);
        self.internal_model_enabled_overrides.insert(key, enabled);
    }

    /// Restores the provider-declared internal-use default for one model.
    /// 恢复一个模型由供应商声明的内部使用默认值。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify one exact model.
    /// `provider_id` 与 `model_id` 标识一个精确模型。
    pub fn restore_internal_model_enabled(&mut self, provider_id: &str, model_id: &str) {
        self.internal_model_enabled_overrides
            .remove(Self::provider_model_key(provider_id, model_id).as_str());
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
    /// The explicit conversation choice, or `None` when the user never overrode this model.
    /// 返回显式会话选择；用户从未覆盖该模型时返回 `None`。
    pub fn model_enabled_override(&self, provider_id: &str, model_id: &str) -> Option<bool> {
        self.model_enabled_overrides
            .get(Self::provider_model_key(provider_id, model_id).as_str())
            .copied()
    }

    /// Stores one explicit conversation choice, including a value equal to its inherited default.
    /// 保存一个明确的会话选择，即使该值与继承默认值相同也保留。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `enabled` identify the exact user choice.
    /// `provider_id`、`model_id` 与 `enabled` 标识精确用户选择。
    pub fn set_model_enabled(&mut self, provider_id: &str, model_id: &str, enabled: bool) {
        let key = Self::provider_model_key(provider_id, model_id);
        self.model_enabled_overrides.insert(key, enabled);
    }

    /// Restores one model conversation choice to its inherited provider or model default.
    /// 将一个模型会话选择恢复为继承的供应商或模型默认值。
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
            protocols: vec![CustomProviderProtocol::OpenAiResponses],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("custom provider descriptor should be valid")
    }

    /// Verifies fresh configurations use the thirty-minute usage interval and sparse priorities.
    /// 验证新配置使用三十分钟用量刷新间隔与稀疏供应商优先级。
    #[test]
    fn default_configuration_uses_thirty_minute_usage_refresh_and_no_priorities() {
        // configuration is the canonical first-run configuration under test.
        // configuration 是本测试验证的权威首次运行配置。
        let configuration = PersistedAiConfiguration::default();

        assert_eq!(configuration.usage_refresh_interval_seconds, 1_800);
        assert!(configuration.provider_priority_overrides.is_empty());
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
            prompt_cache_by_protocol: BTreeMap::new(),
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

    /// Verifies schema thirteen provider-tool state is discarded without compatibility mapping.
    /// 验证 Schema 十三的 Provider Tool 状态会在不做兼容映射的情况下被丢弃。
    #[test]
    fn schema_thirteen_discards_pre_extension_provider_tool_state() {
        // historical carries every old scalar selection store plus a forged extension entry.
        // historical 携带全部旧标量选择存储以及一条伪造扩展记录。
        let mut historical = PersistedAiConfiguration::default();
        historical.schema_version = 13;
        historical.provider_tool_selections.insert(
            ProviderToolCapabilityKind::MediaAnalyze,
            ProviderToolTargetSelection {
                connection_id: String::from("old-connection"),
                target_id: String::from("old-mixed-media"),
            },
        );
        historical
            .provider_tool_enabled_capabilities
            .insert(ProviderToolCapabilityKind::MediaAnalyze);
        historical.provider_extension_target_sets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([ProviderToolTargetSelection {
                connection_id: String::from("old-connection"),
                target_id: String::from("old-image-generator"),
            }]),
        );
        historical.provider_tool_disabled_targets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([ProviderToolTargetSelection {
                connection_id: String::from("old-connection"),
                target_id: String::from("old-disabled-generator"),
            }]),
        );

        assert!(historical.migrate_to_current_schema().expect("migration"));
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.provider_tool_selections.is_empty());
        assert!(historical.provider_tool_enabled_capabilities.is_empty());
        assert!(historical.provider_extension_target_sets.is_empty());
        assert!(historical.provider_tool_disabled_targets.is_empty());
    }

    /// Verifies migration from schema fourteen discards old operation switches while preserving provider tools.
    /// 验证从 Schema 十四迁移时丢弃旧按操作开关，同时保留 Provider Tool 设置。
    #[test]
    fn schema_fourteen_replaces_internal_switches_without_erasing_provider_tools() {
        let mut historical = PersistedAiConfiguration::default();
        historical.schema_version = 14;
        historical.internal_model_enabled_overrides.insert(
            PersistedAiConfiguration::internal_model_operation_key(
                "opencode-go",
                "deepseek-v4-flash",
                AiOperation::ConversationRespond,
            ),
            true,
        );
        let target = ProviderToolTargetSelection {
            connection_id: String::from("search-connection"),
            target_id: String::from("search-target"),
        };
        historical
            .provider_tool_selections
            .insert(ProviderToolCapabilityKind::WebSearch, target.clone());
        historical
            .provider_tool_enabled_capabilities
            .insert(ProviderToolCapabilityKind::WebSearch);

        assert!(historical.migrate_to_current_schema().expect("migration"));
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.internal_model_enabled_overrides.is_empty());
        assert_eq!(
            historical
                .provider_tool_selections
                .get(&ProviderToolCapabilityKind::WebSearch),
            Some(&target),
        );
        assert!(
            historical
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::WebSearch)
        );
    }

    /// Verifies schema fifteen price configuration is ignored and never serialized into the current schema.
    /// 验证 Schema 十五价格配置会被忽略，且绝不会序列化到当前 Schema。
    #[test]
    fn schema_fifteen_discards_legacy_billing_configuration() {
        let mut historical = serde_json::to_value(PersistedAiConfiguration::default())
            .expect("default configuration should serialize");
        historical["schema_version"] = serde_json::json!(15);
        historical["billing"] = serde_json::json!({
            "catalog_path": "legacy-model-pricing.json",
            "model_overrides": {
                "gpt-5": {
                    "input_per_million": "1.25",
                    "output_per_million": "10"
                }
            }
        });

        let mut configuration: PersistedAiConfiguration = serde_json::from_value(historical)
            .expect("legacy unknown billing field should be ignored");
        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("migration")
        );
        assert_eq!(
            configuration.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        let serialized =
            serde_json::to_value(configuration).expect("migrated configuration should serialize");
        assert!(serialized.get("billing").is_none());
    }

    /// Verifies schema sixteen receives the complete default fixed-node configuration.
    /// 验证 Schema 十六会获得完整的默认固定节点配置。
    #[test]
    fn schema_sixteen_migration_adds_default_context_compaction_window_nodes() {
        let mut historical = serde_json::to_value(PersistedAiConfiguration::default())
            .expect("default configuration should serialize");
        historical["schema_version"] = serde_json::json!(16);
        historical
            .as_object_mut()
            .expect("configuration should be an object")
            .remove("context_compaction_window_nodes");

        let mut configuration: PersistedAiConfiguration = serde_json::from_value(historical)
            .expect("schema sixteen should deserialize with default nodes");

        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("migration")
        );
        assert_eq!(
            configuration.context_compaction_window_nodes,
            ContextCompactionWindowConfiguration::default()
        );
        assert_eq!(
            configuration.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
    }

    /// Verifies schema seventeen migrates only the unambiguous image backend and disables inferred media.
    /// 验证 Schema 十七只迁移无歧义图片后端，并禁用推断式媒体配置。
    #[test]
    fn schema_seventeen_migrates_only_explicit_image_understanding_backend() {
        let mut historical = PersistedAiConfiguration::default();
        historical.schema_version = 17;
        let image_selection = ProviderToolTargetSelection {
            connection_id: String::from("vision-account"),
            target_id: String::from("vision-target"),
        };
        historical.provider_tool_selections.insert(
            ProviderToolCapabilityKind::ImageUnderstand,
            image_selection.clone(),
        );
        historical.provider_tool_selections.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            ProviderToolTargetSelection {
                connection_id: String::from("legacy-video-account"),
                target_id: String::from("legacy-video-target"),
            },
        );
        historical.provider_tool_enabled_capabilities.extend([
            ProviderToolCapabilityKind::ImageUnderstand,
            ProviderToolCapabilityKind::VideoUnderstand,
        ]);

        assert!(historical.migrate_to_current_schema().expect("migration"));
        assert_eq!(
            historical
                .media_understanding_selections
                .get(&ProviderToolCapabilityKind::ImageUnderstand),
            Some(&MediaUnderstandingBackendSelection::ProviderTool {
                selection: image_selection,
            })
        );
        assert!(
            historical
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::ImageUnderstand)
        );
        assert!(
            !historical
                .provider_tool_enabled_capabilities
                .contains(&ProviderToolCapabilityKind::VideoUnderstand)
        );
        assert!(
            !historical
                .provider_tool_selections
                .contains_key(&ProviderToolCapabilityKind::VideoUnderstand)
        );
    }

    /// Verifies provider and replacement routes remain independently reusable when the active mode changes.
    /// 验证切换当前模式时供应商专用与模型代替路由仍可独立复用。
    #[test]
    fn media_understanding_backend_modes_keep_independent_routes() {
        let capability = ProviderToolCapabilityKind::VideoUnderstand;
        let provider_selection = ProviderToolTargetSelection {
            connection_id: String::from("provider-account"),
            target_id: String::from("provider-video"),
        };
        let replacement_selection = ActiveAiModelSelection {
            provider_id: String::from("replacement-provider"),
            model_id: String::from("replacement-video-model"),
            connection_id: String::from("replacement-account"),
        };
        let mut configuration = PersistedAiConfiguration::default();
        let mut backend_options = MediaUnderstandingBackendOptions::default();
        backend_options.update_from_selection(&MediaUnderstandingBackendSelection::ProviderTool {
            selection: provider_selection.clone(),
        });
        backend_options.update_from_selection(
            &MediaUnderstandingBackendSelection::ModelReplacement {
                selection: replacement_selection.clone(),
                reasoning_effort: Some(ReasoningEffort::Medium),
            },
        );
        configuration
            .media_understanding_backend_options
            .insert(capability, backend_options);

        configuration
            .apply_media_understanding_mode(
                capability,
                MediaUnderstandingBackendMode::ModelReplacement,
            )
            .expect("replacement route should be selectable");
        assert_eq!(
            configuration
                .media_understanding_selections
                .get(&capability),
            Some(&MediaUnderstandingBackendSelection::ModelReplacement {
                selection: replacement_selection,
                reasoning_effort: Some(ReasoningEffort::Medium),
            })
        );

        configuration
            .apply_media_understanding_mode(capability, MediaUnderstandingBackendMode::ProviderTool)
            .expect("provider route should remain selectable");
        assert_eq!(
            configuration
                .media_understanding_selections
                .get(&capability),
            Some(&MediaUnderstandingBackendSelection::ProviderTool {
                selection: provider_selection,
            })
        );
    }

    /// Verifies conversation publication and non-conversation internal use are independent states.
    /// 验证会话发布与非会话内部使用是相互独立的状态。
    #[test]
    fn conversation_and_internal_model_states_are_independent() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_model_enabled("opencode-go", "deepseek-v4-flash", false);
        configuration.set_internal_model_enabled("opencode-go", "deepseek-v4-flash", true);

        assert!(!configuration.model_is_enabled("opencode-go", "deepseek-v4-flash"));
        assert!(
            configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", false,)
        );

        configuration.set_model_enabled("opencode-go", "deepseek-v4-flash", true);
        configuration.set_internal_model_enabled("opencode-go", "deepseek-v4-flash", false);
        assert!(configuration.model_is_enabled("opencode-go", "deepseek-v4-flash"));
        assert!(
            !configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", true,)
        );

        configuration.restore_internal_model_enabled("opencode-go", "deepseek-v4-flash");
        assert!(configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", true,));
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

    /// Verifies candidate switches and replacement backends are removed with their owning model.
    /// 验证候选开关与替代后端会随其所属模型一并移除。
    #[test]
    fn model_removal_cleans_multimodal_candidate_and_replacement_backend() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_multimodal_understanding_candidate_enabled(
            "custom:vision",
            "vision-model",
            true,
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection: ActiveAiModelSelection {
                    provider_id: String::from("custom:vision"),
                    model_id: String::from("vision-model"),
                    connection_id: String::from("vision-account"),
                },
                reasoning_effort: Some(ReasoningEffort::Medium),
            },
        );

        configuration.remove_model_owned_settings("custom:vision", "vision-model");

        assert!(
            !configuration
                .multimodal_understanding_candidate_is_enabled("custom:vision", "vision-model")
        );
        assert!(
            !configuration
                .media_understanding_selections
                .contains_key(&ProviderToolCapabilityKind::VideoUnderstand)
        );
    }

    /// Verifies disabling one candidate clears only replacement routes owned by that exact model.
    /// 验证禁用一个候选模型只会清理该精确模型持有的替代路由。
    #[test]
    fn candidate_disable_requires_exact_model_reselection() {
        // configuration contains two admitted models and one unrelated provider-specific backend.
        // configuration 包含两个已准入模型与一个无关的供应商专用后端。
        let mut configuration = PersistedAiConfiguration::default();
        for model_id in ["disabled-model", "retained-model"] {
            configuration.set_multimodal_understanding_candidate_enabled(
                "media-provider",
                model_id,
                true,
            );
        }
        for (capability, model_id) in [
            (
                ProviderToolCapabilityKind::ImageUnderstand,
                "disabled-model",
            ),
            (
                ProviderToolCapabilityKind::VideoUnderstand,
                "retained-model",
            ),
        ] {
            configuration.media_understanding_selections.insert(
                capability,
                MediaUnderstandingBackendSelection::ModelReplacement {
                    selection: ActiveAiModelSelection {
                        provider_id: String::from("media-provider"),
                        model_id: model_id.to_string(),
                        connection_id: String::from("media-account"),
                    },
                    reasoning_effort: None,
                },
            );
        }
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::AudioUnderstand,
            MediaUnderstandingBackendSelection::ProviderTool {
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("media-account"),
                    target_id: String::from("audio-target"),
                },
            },
        );

        configuration.set_multimodal_understanding_candidate_enabled(
            "media-provider",
            "disabled-model",
            false,
        );

        assert!(
            !configuration
                .media_understanding_selections
                .contains_key(&ProviderToolCapabilityKind::ImageUnderstand)
        );
        assert!(
            configuration
                .media_understanding_selections
                .contains_key(&ProviderToolCapabilityKind::VideoUnderstand)
        );
        assert!(
            configuration
                .media_understanding_selections
                .contains_key(&ProviderToolCapabilityKind::AudioUnderstand)
        );
        assert!(
            configuration
                .multimodal_understanding_candidate_is_enabled("media-provider", "retained-model")
        );
    }

    /// Verifies provider removal cleans both provider-tool and model-replacement media backends.
    /// 验证移除供应商会同时清理供应商工具与模型代替媒体后端。
    #[test]
    fn provider_removal_cleans_every_owned_media_backend() {
        let provider_id = "custom:media-owner";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.connection_graph.connections.insert(
            String::from("media-owner-account"),
            custom_provider_connection("media-owner-account", provider_id),
        );
        configuration.set_multimodal_understanding_candidate_enabled(
            provider_id,
            "media-model",
            true,
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::ImageUnderstand,
            MediaUnderstandingBackendSelection::ProviderTool {
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("media-owner-account"),
                    target_id: String::from("image-target"),
                },
            },
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::AudioUnderstand,
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection: ActiveAiModelSelection {
                    provider_id: provider_id.to_string(),
                    model_id: String::from("media-model"),
                    connection_id: String::from("media-owner-account"),
                },
                reasoning_effort: None,
            },
        );

        assert!(configuration.provider_has_model_customizations(provider_id));
        configuration.remove_provider_owned_model_settings(provider_id);

        assert!(
            configuration
                .multimodal_understanding_candidate_models
                .is_empty()
        );
        assert!(configuration.media_understanding_selections.is_empty());
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

    /// Verifies disabling a credential is sparse, reversible, and clears every routing preference that referenced it.
    /// 验证停用凭据采用稀疏持久化、可逆，并清理引用它的全部路由偏好。
    #[test]
    fn connection_enabled_state_is_sparse_reversible_and_removes_routing_preferences() {
        // connection is a real graph member so the state API cannot accept an orphan identifier.
        // connection 是真实连接图成员，因此状态接口不会接受孤立标识。
        let connection = custom_provider_connection("account-a", "custom:provider-a");
        // configuration carries all preference shapes that must stop referring to a disabled account.
        // configuration 承载停用账号后必须停止引用它的全部偏好形状。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .connections
            .insert(connection.id.clone(), connection.clone());
        configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .insert(connection.provider_id.clone(), connection.id.clone());
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .insert(String::from("model-key"), connection.id.clone());
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: connection.provider_id.clone(),
            model_id: String::from("model-a"),
            connection_id: connection.id.clone(),
        });

        configuration
            .set_connection_enabled(connection.id.as_str(), false)
            .expect("configured connection should be disabled");
        assert!(!configuration.connection_is_enabled(connection.id.as_str()));
        assert!(configuration.active.is_none());
        assert!(
            configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .is_empty()
        );
        assert!(
            configuration
                .connection_graph
                .account_preferences
                .model_last_connections
                .is_empty()
        );

        configuration
            .set_connection_enabled(connection.id.as_str(), true)
            .expect("disabled connection should be enabled again");
        assert!(configuration.connection_is_enabled(connection.id.as_str()));
        assert!(configuration.disabled_connection_ids.is_empty());
        assert!(
            configuration
                .set_connection_enabled("missing-account", false)
                .is_err()
        );
    }

    /// Verifies schema eighteen upgrades with every existing credential enabled by default.
    /// 验证 Schema 18 升级时全部既有凭据默认启用。
    #[test]
    fn schema_eighteen_migration_initializes_disabled_connection_state() {
        // historical omits the new sparse field exactly as a version-eighteen file does.
        // historical 与 Schema 18 文件一致，省略新增的稀疏字段。
        let mut historical: PersistedAiConfiguration =
            serde_json::from_str(r#"{"schema_version":18}"#)
                .expect("schema eighteen fixture must deserialize");
        assert!(historical.disabled_connection_ids.is_empty());
        assert!(
            historical
                .migrate_to_current_schema()
                .expect("schema eighteen must migrate")
        );
        assert_eq!(
            historical.schema_version,
            PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION
        );
        assert!(historical.disabled_connection_ids.is_empty());
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
        assert!(historical.inference_profiles.contains_key("vce"));
    }

    /// Verifies explicit consumer dimensions survive serialization without changing peer profiles.
    /// 验证消费者的明确维度在序列化后保留，且不会改变其他配置档。
    ///
    /// # Returns
    /// 返回值
    /// Completes when VCE retains its numeric choice and legacy VMM keeps an absent choice.
    /// 当 VCE 保留数值选择、旧 VMM 保持未设置时正常完成。
    #[test]
    fn inference_profile_preserves_explicit_consumer_embedding_dimensions() {
        // Each profile owns its choice; a missing peer value must not inherit VCE's setting.
        // 每个配置档独立拥有选择；缺少设置的配置档不能继承 VCE 的维度。
        let profiles: BTreeMap<String, InferenceProfile> =
            serde_json::from_value(serde_json::json!({
                "vce": { "embedding_dimensions": 1024 },
                "vmm": {}
            }))
            .expect("valid consumer profiles");
        // Re-serialization exposes accidental field loss without relying on a new Rust field.
        // 再次序列化用于发现字段丢失，而不依赖新增 Rust 字段访问。
        let serialized = serde_json::to_value(&profiles).expect("serializable consumer profiles");
        assert_eq!(serialized["vce"]["embedding_dimensions"], 1024);
        assert!(serialized["vmm"].get("embedding_dimensions").is_none());
    }

    /// Verifies a current-schema file repairs the system VCE profile without copying VMM routes.
    /// 验证当前 Schema 文件会修复系统 VCE 配置档，且不会复制 VMM 路由。
    #[test]
    fn current_schema_repairs_missing_vce_profile_without_copying_vmm_routes() {
        let mut configuration = PersistedAiConfiguration::default();
        let vmm_profile = InferenceProfile {
            embedding: Some(InferenceRouteConfiguration {
                provider_id: String::from("custom:vmm-provider"),
                service_entry_id: String::from("vmm-entry"),
                connection_id: String::from("vmm-account"),
                model_id: String::from("vmm-model"),
                binding: InferenceRouteBinding::Action {
                    action_binding_id: String::from("vmm-embedding-action"),
                },
                operation: AiOperation::EmbeddingCreate,
                usage_scope: InferenceUsageScope::InternalBackground,
            }),
            ..InferenceProfile::default()
        };
        configuration
            .inference_profiles
            .insert(String::from("vmm"), vmm_profile.clone());
        configuration.inference_profiles.remove("vce");

        assert!(
            configuration
                .migrate_to_current_schema()
                .expect("current schema must repair VCE")
        );
        assert_eq!(
            configuration.inference_profiles.get("vmm"),
            Some(&vmm_profile)
        );
        assert_eq!(
            configuration.inference_profiles.get("vce"),
            Some(&InferenceProfile::default())
        );
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
                    tool_schema_projection: Default::default(),
                    system_binding: None,
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
                    tool_schema_projection: Default::default(),
                    system_binding: None,
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
    fn custom_capability_overrides_leave_immutable_source_fields_untouched() {
        let mut configuration = PersistedAiConfiguration::default();
        let key = PersistedAiConfiguration::provider_model_key("custom:provider", "model");
        configuration.model_capability_overrides.insert(
            key,
            ModelCapabilityOverrides {
                context_limit: Some(32_000),
                output_limit: None,
                input_resources: Some(vec![ResourceKind::Image]),
                structured_output: Some(StructuredOutputCapability::JsonSchema),
                output_resources: None,
                ..ModelCapabilityOverrides::default()
            },
        );
        let mut effective = ModelCapabilities {
            context_limit: Some(200_000),
            input_resources: Vec::new(),
            output_resources: vec![ResourceKind::File],
            structured_output: StructuredOutputCapability::Unsupported,
            ..ModelCapabilities::default()
        };

        configuration.apply_model_capability_overrides("custom:provider", "model", &mut effective);

        assert_eq!(effective.context_limit, Some(32_000));
        assert_eq!(effective.input_resources, vec![ResourceKind::Image]);
        assert_eq!(effective.output_resources, vec![ResourceKind::File]);
        assert_eq!(
            effective.structured_output,
            StructuredOutputCapability::JsonSchema
        );

        configuration
            .model_capability_overrides
            .get_mut(
                PersistedAiConfiguration::provider_model_key("custom:provider", "model").as_str(),
            )
            .expect("model capability overrides must remain available")
            .structured_output = Some(StructuredOutputCapability::Unsupported);
        configuration.apply_model_capability_overrides("custom:provider", "model", &mut effective);
        assert_eq!(
            effective.structured_output,
            StructuredOutputCapability::Unsupported
        );
    }

    /// Verifies system providers ignore all persisted model capability overrides.
    /// 验证系统供应商忽略全部持久化模型能力覆盖。
    #[test]
    fn system_provider_capability_contract_is_not_user_overridable() {
        let mut configuration = PersistedAiConfiguration::default();
        let key = PersistedAiConfiguration::provider_model_key("minimax", "MiniMax-M2.7");
        let stale = ModelCapabilityOverrides {
            context_limit: Some(128_000),
            input_resources: Some(vec![ResourceKind::Image]),
            structured_output: Some(StructuredOutputCapability::JsonSchema),
            reasoning: Some(CapabilitySupport::Unsupported),
            reasoning_control: Some(ReasoningControlMode::FixedOff),
            reasoning_efforts: Some(vec![ReasoningEffort::None]),
            request_projection: Some(CustomModelRequestProjection::default()),
            ..ModelCapabilityOverrides::default()
        };
        assert!(stale.validate_for_provider("minimax").is_err());
        configuration
            .model_capability_overrides
            .insert(key.clone(), stale);
        let mut effective = ModelCapabilities {
            context_limit: Some(204_800),
            input_resources: Vec::new(),
            structured_output: StructuredOutputCapability::JsonObject,
            reasoning: CapabilitySupport::Native,
            reasoning_control: ReasoningControlMode::ProviderAdaptive,
            reasoning_efforts: vec![ReasoningEffort::Adaptive],
            ..ModelCapabilities::default()
        };

        configuration.apply_model_capability_overrides("minimax", "MiniMax-M2.7", &mut effective);

        assert_eq!(effective.context_limit, Some(204_800));
        assert!(effective.input_resources.is_empty());
        assert_eq!(
            effective.structured_output,
            StructuredOutputCapability::JsonObject
        );
        assert_eq!(effective.reasoning, CapabilitySupport::Native);
        assert_eq!(
            effective.reasoning_control,
            ReasoningControlMode::ProviderAdaptive
        );
        assert_eq!(effective.reasoning_efforts, [ReasoningEffort::Adaptive]);
        assert!(configuration.normalize_direct_provider_model_settings());
        assert_eq!(
            configuration.model_capability_overrides.get(&key),
            Some(&ModelCapabilityOverrides {
                context_limit: Some(128_000),
                ..ModelCapabilityOverrides::default()
            })
        );
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
                prompt_cache_by_protocol: BTreeMap::new(),
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
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
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
                    tool_schema_projection: Default::default(),
                    system_binding: None,
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

    /// Verifies an inactive media backend route is disclosed before provider cascade cleanup.
    /// 验证未激活的媒体后端路由会在供应商级联清理前被披露。
    #[test]
    fn provider_model_customization_detection_includes_inactive_media_backend_route() {
        let provider_id = "custom:media-route";
        let model_id = "video-model";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.media_understanding_backend_options.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            MediaUnderstandingBackendOptions {
                model_replacement: Some(MediaUnderstandingModelReplacementSelection {
                    selection: ActiveAiModelSelection {
                        provider_id: String::from(provider_id),
                        model_id: String::from(model_id),
                        connection_id: String::from("media-account"),
                    },
                    reasoning_effort: None,
                }),
                ..MediaUnderstandingBackendOptions::default()
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
        let removed_manifest = crate::project_custom_descriptor_manifest(&removed_descriptor)
            .expect("removed custom manifest should build");
        let retained_manifest = crate::project_custom_descriptor_manifest(&retained_descriptor)
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
        configuration.provider_priority_overrides.extend([
            (String::from(removed_provider), 20),
            (String::from(retained_provider), 10),
        ]);
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
                    tool_schema_projection: Default::default(),
                    system_binding: None,
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
                    tool_schema_projection: Default::default(),
                    system_binding: None,
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
                llm_reasoning_efforts: BTreeMap::from([
                    (String::from("removed-purpose"), ReasoningEffort::None),
                    (String::from("retained-purpose"), ReasoningEffort::None),
                ]),
                embedding: Some(removed_embedding_route),
                embedding_dimensions: None,
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
            configuration.provider_priority_overrides,
            BTreeMap::from([(String::from(retained_provider), 10)])
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
        assert_eq!(
            profile
                .llm_reasoning_efforts
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
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
            tool_schema_projection: Default::default(),
            system_binding: None,
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
        let provider_id = "custom:provider";
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
                        tool_schema_projection: Default::default(),
                        system_binding: None,
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
        let provider_id = "custom:provider";
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
                        tool_schema_projection: Default::default(),
                        system_binding: None,
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
        let provider_id = "custom:provider";
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
            tool_schema_projection: Default::default(),
            system_binding: None,
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
            tool_schema_projection: Default::default(),
            system_binding: None,
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

    /// Verifies historical schemas discard every incompatible provider-tool selection.
    /// 验证历史 Schema 会丢弃全部不兼容的 Provider Tool 选择。
    #[test]
    fn historical_configuration_migration_discards_incompatible_provider_tool_state() {
        let mut configuration = PersistedAiConfiguration {
            schema_version: 0,
            ..PersistedAiConfiguration::default()
        };
        configuration.provider_tool_selections.insert(
            ProviderToolCapabilityKind::WebSearch,
            ProviderToolTargetSelection {
                connection_id: String::from("search-connection"),
                target_id: String::from("search-target"),
            },
        );
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
        assert!(configuration.provider_tool_selections.is_empty());
        assert!(configuration.provider_tool_enabled_capabilities.is_empty());
        assert!(configuration.provider_tool_disabled_targets.is_empty());
        assert!(configuration.provider_extension_target_sets.is_empty());
        assert!(configuration.inference_profiles.contains_key("vmm"));
        assert!(configuration.inference_profiles.contains_key("vce"));
    }

    /// Verifies schema one discards every incompatible provider-tool activation field.
    /// 验证 Schema 一会丢弃全部不兼容的 Provider Tool 启用字段。
    #[test]
    fn schema_one_discards_incompatible_provider_tool_activation() {
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

        assert!(configuration.provider_tool_selections.is_empty());
        assert!(configuration.provider_tool_enabled_capabilities.is_empty());
        assert!(configuration.provider_tool_disabled_targets.is_empty());
        assert!(configuration.provider_extension_target_sets.is_empty());
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
            llm_reasoning_efforts: BTreeMap::new(),
            embedding: Some(embedding_route),
            embedding_dimensions: None,
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

        let policy = configuration
            .resolve_context_compaction_policy(provider_id, model_id, Some(500_000), None, None)
            .expect("route policy should resolve");
        assert_eq!(policy.window.effective_context_window_percent, 95);
        assert_eq!(policy.window.auto_compact_percent, 88);
        assert_eq!(policy.window.auto_compact_token_limit, Some(123_456));
        assert_eq!(policy.window.fallback_buffer_tokens, 0);
        assert!(policy.window_policy_is_explicit);
        assert_eq!(policy.tail_context_limit, Some(500_000));
        assert_eq!(policy.tail_turns, 10);
        assert_eq!(policy.preserve_recent_tokens, 25_000);
    }

    /// Verifies the five fixed node ids and their default low-to-high token values.
    /// 验证五个固定节点标识及其默认从低到高 Token 值。
    #[test]
    fn context_compaction_window_nodes_default_to_the_fixed_contract() {
        let configuration = ContextCompactionWindowConfiguration::default();

        assert!(configuration.validate().is_ok());
        assert_eq!(
            configuration.nodes.map(|node| (node.id, node.tokens)),
            [
                (ContextCompactionWindowNodeId::CompressionWindow1, 200_000),
                (ContextCompactionWindowNodeId::CompressionWindow2, 256_000),
                (ContextCompactionWindowNodeId::CompressionWindow3, 272_000),
                (ContextCompactionWindowNodeId::CompressionWindow4, 373_000),
                (ContextCompactionWindowNodeId::CompressionWindow5, 500_000),
            ]
        );
    }

    /// Verifies fixed nodes reject id reassignment and non-ascending token values.
    /// 验证固定节点会拒绝标识重分配及非递增 Token 值。
    #[test]
    fn context_compaction_window_nodes_reject_identity_and_order_changes() {
        let mut wrong_id = ContextCompactionWindowConfiguration::default();
        wrong_id.nodes[0].id = ContextCompactionWindowNodeId::CompressionWindow2;
        assert!(
            wrong_id
                .validate()
                .expect_err("id change must fail")
                .contains("must use id")
        );

        let mut wrong_order = ContextCompactionWindowConfiguration::default();
        wrong_order.nodes[2].tokens = wrong_order.nodes[1].tokens;
        assert!(
            wrong_order
                .validate()
                .expect_err("equal node values must fail")
                .contains("strictly ascending")
        );
    }

    /// Verifies a session node changes only the resolved compaction base and retained-tail policy.
    /// 验证会话节点只改变已解析压缩基准与尾部保留策略。
    #[test]
    fn context_compaction_window_node_resolves_below_the_model_hard_window() {
        let configuration = PersistedAiConfiguration::default();

        let policy = configuration
            .resolve_context_compaction_policy(
                "provider",
                "model",
                Some(1_000_000),
                Some(ContextCompactionWindowNodeId::CompressionWindow2),
                None,
            )
            .expect("256K node should resolve below a 1M model");

        assert_eq!(
            policy.compression_window_node_id,
            Some(ContextCompactionWindowNodeId::CompressionWindow2)
        );
        assert_eq!(policy.compression_window_limit, Some(256_000));
        assert_eq!(policy.tail_context_limit, Some(256_000));
    }

    /// Verifies equal or larger node values are rejected for the selected model.
    /// 验证等于或大于所选模型窗口的节点值会被拒绝。
    #[test]
    fn context_compaction_window_node_must_be_strictly_lower_than_model_window() {
        let configuration = PersistedAiConfiguration::default();

        let error = configuration
            .resolve_context_compaction_policy(
                "provider",
                "model",
                Some(256_000),
                Some(ContextCompactionWindowNodeId::CompressionWindow2),
                None,
            )
            .expect_err("equal node and model windows must fail");

        assert!(error.contains("must be lower than model context limit"));
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
        let policy = configuration
            .resolve_context_compaction_policy(
                provider_id,
                model_id,
                Some(300_000),
                None,
                Some(behavior_only),
            )
            .expect("behavior-only policy should resolve");

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

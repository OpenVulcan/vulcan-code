//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{
    CustomModelDefinition, CustomModelRequestProjection, CustomModelTemplateDecision,
    CustomProviderModelDefaults, PersistedConnectionGraph, ProviderDescriptor,
    ProviderExtensionModeSelection, ProviderToolCapabilityKind, ProviderToolTargetSelection,
    SecondaryRouteSelection, validate_custom_provider_reasoning_settings,
    validate_system_model_reasoning_settings,
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
pub const PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION: u32 = 20;

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
            && self.reasoning.is_none()
            && self.reasoning_control.is_none()
            && self.reasoning_efforts.is_none()
            && self.request_projection.is_none()
    }

    /// Validates one direct model-setting record before it can enter persisted configuration.
    /// 在一份直接模型设置进入持久化配置前对其进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success only when limits are positive and reasoning fields form one complete policy.
    /// 仅在限制为正且思考字段形成完整策略时返回成功。
    pub fn validate(&self) -> Result<(), String> {
        self.validate_shape()?;
        if let (Some(reasoning), Some(control), Some(efforts)) = (
            self.reasoning,
            self.reasoning_control,
            self.reasoning_efforts.as_deref(),
        ) {
            validate_custom_provider_reasoning_settings(reasoning, control, efforts)?;
        }
        Ok(())
    }

    /// Validates provider-independent structure: positive limits, complete reasoning fields and safe paths.
    /// 校验与供应商无关的结构：限制为正、思考字段完整且路径安全。
    fn validate_shape(&self) -> Result<(), String> {
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
        if let Some(request_projection) = self.request_projection.as_ref() {
            request_projection.validate_paths()?;
        }
        Ok(())
    }

    /// Validates one provider-wide override against the editable surface owned by its provider kind.
    /// 根据供应商类型所拥有的可编辑范围校验一层供应商级覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies either one system provider or one user-owned custom provider.
    /// `provider_id` 标识一个系统供应商或用户自有自定义供应商。
    ///
    /// # Returns
    /// 返回值
    /// Custom providers may declare provider defaults; system providers accept only an empty override.
    /// 自定义供应商可声明供应商默认值；系统供应商仅接受空覆盖。
    pub fn validate_for_provider(&self, provider_id: &str) -> Result<(), String> {
        self.validate()?;
        if provider_id.starts_with("custom:") || self.is_empty() {
            return Ok(());
        }
        Err(String::from(
            "System provider capability defaults are owned by the signed manifest and cannot be overridden.",
        ))
    }

    /// Validates one model-level override against the editable surface owned by its provider kind.
    /// 根据供应商类型所拥有的可编辑范围校验一层模型级覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies either one system provider or one user-owned custom provider.
    /// `provider_id` 标识一个系统供应商或用户自有自定义供应商。
    ///
    /// # Returns
    /// 返回值
    /// Custom providers may override every field. System models may override limits, input
    /// resources and the reasoning control and levels; the wire mapping of each level stays with
    /// the signed route template, so structured output and request projections are rejected.
    /// 自定义供应商可覆盖全部字段。系统模型可覆盖限制、输入资源以及思考控制与档位；每个档位的
    /// 线协议映射仍归签名路由模板，因此拒绝结构化输出与请求投影。
    pub fn validate_for_model(&self, provider_id: &str) -> Result<(), String> {
        if provider_id.starts_with("custom:") {
            return self.validate();
        }
        self.validate_shape()?;
        if self.structured_output.is_some() {
            return Err(String::from(
                "System model structured-output support is owned by the signed manifest and cannot be overridden.",
            ));
        }
        if self.request_projection.is_some() {
            return Err(String::from(
                "System model request projections are owned by the signed route template and cannot be overridden.",
            ));
        }
        if let (Some(reasoning), Some(control), Some(efforts)) = (
            self.reasoning,
            self.reasoning_control,
            self.reasoning_efforts.as_deref(),
        ) {
            validate_system_model_reasoning_settings(reasoning, control, efforts)?;
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
#[serde(deny_unknown_fields)]
pub struct ModelExecutionOverrides {
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
    /// `true` when every cache choice and model parameter is absent.
    /// 当全部缓存选择与模型参数均不存在时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.prompt_cache.is_none() && self.model_parameters.is_empty()
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
    /// Optional configured decision route.
    /// 可选的已配置判定路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<InferenceRouteConfiguration>,
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

/// Stores account-wide access and explicit per-model decisions.
/// 保存账号全局访问状态与显式单模型决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionModelAccessPolicy {
    /// Provider display baseline, or the account-wide state after the user changes all models.
    /// 供应商展示基线，或用户修改全部模型后采用的账号全局状态。
    #[serde(default = "default_connection_model_enabled")]
    pub default_enabled: bool,
    /// Whether unmentioned models inherit their exact signed or manual default.
    /// 未单独指定的模型是否继承精确签名或手工默认值。
    #[serde(default = "default_connection_model_enabled")]
    pub inherit_model_defaults: bool,
    /// Explicit per-model decisions independent of the current signed baseline.
    /// 独立于当前签名基线的单模型显式决定。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_enabled_overrides: BTreeMap<String, bool>,
}

impl Default for ConnectionModelAccessPolicy {
    /// Builds an account policy that enables supported models by default.
    /// 构建默认启用受支持模型的账号策略。
    fn default() -> Self {
        Self {
            default_enabled: true,
            inherit_model_defaults: true,
            model_enabled_overrides: BTreeMap::new(),
        }
    }
}

impl ConnectionModelAccessPolicy {
    /// Applies only explicit account gates before exact catalog membership is checked.
    /// 在校验精确目录成员资格之前仅应用显式账号门控。
    ///
    /// # Parameters
    /// 参数
    /// model_id identifies the exact provider model.
    /// model_id 标识精确供应商模型。
    ///
    /// # Returns
    /// 返回值
    /// The explicit model or account-wide decision, otherwise true for signed catalog evaluation.
    /// 返回显式模型或账号全局决定，否则交由签名目录继续判定。
    pub fn model_is_enabled(&self, model_id: &str) -> bool {
        self.model_enabled_overrides
            .get(model_id)
            .copied()
            .unwrap_or(self.inherit_model_defaults || self.default_enabled)
    }

    /// Resolves a model against its signed default unless the user chose an account-wide state.
    /// 除非用户设置了账号全局状态，否则按模型签名默认值解析。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` identifies the exact model and `inherited_default` is its signed or manual state.
    /// `model_id` 标识精确模型，`inherited_default` 是其签名或手工默认状态。
    ///
    /// # Returns
    /// 返回值
    /// The individual decision, account-wide decision, or inherited model state.
    /// 返回单模型决定、账号全局决定或继承的模型状态。
    pub fn model_is_enabled_with_default(&self, model_id: &str, inherited_default: bool) -> bool {
        self.model_enabled_overrides
            .get(model_id)
            .copied()
            .unwrap_or({
                if self.inherit_model_defaults {
                    inherited_default
                } else {
                    self.default_enabled
                }
            })
    }

    /// Stores an explicit model decision even when it equals the account-wide state.
    /// 即使与账号全局状态相同也保存显式单模型决定。
    ///
    /// # Parameters
    /// 参数
    /// model_id identifies the exact model and enabled is the requested state.
    /// model_id 标识精确模型，enabled 是所选状态。
    pub fn set_model_enabled(&mut self, model_id: &str, enabled: bool) {
        self.model_enabled_overrides
            .insert(model_id.to_string(), enabled);
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
        self.inherit_model_defaults = false;
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
    /// Last activated exact signed model IDs retained for upgrade cleanup.
    /// 为升级清理保留的上一代已激活精确签名模型 ID。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub system_signed_model_inventory: BTreeMap<String, BTreeSet<String>>,
    /// Signed takeover notices pending acknowledgement by the user interface.
    /// 等待用户界面确认的签名模型接管提示。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_system_model_takeover_notices: BTreeMap<String, BTreeSet<String>>,
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
    /// Default fixed window node for newly created conversations; None follows the selected model.
    /// 新建会话默认使用的固定窗口节点；空值表示遵循所选模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_session_context_compaction_window_node_id: Option<ContextCompactionWindowNodeId>,
    /// Explicit non-conversation internal-use choices scoped by provider and model.
    /// 按供应商与模型限定作用域的显式非会话内部使用选择。
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
    /// Exposure mode and fixed single-mode target per extension capability; missing means
    /// single mode without a model.
    /// 每项扩展能力的暴露模式及单模型固定目标；缺失表示未选模型的单模型模式。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_extension_modes:
        BTreeMap<ProviderToolCapabilityKind, ProviderExtensionModeSelection>,
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
            system_signed_model_inventory: BTreeMap::new(),
            pending_system_model_takeover_notices: BTreeMap::new(),
            custom_provider_model_defaults: BTreeMap::new(),
            custom_model_template_decisions: BTreeMap::new(),
            custom_model_protocol_preferences: BTreeMap::new(),
            custom_model_connection_enabled_overrides: BTreeMap::new(),
            connection_model_access_policies: BTreeMap::new(),
            pending_validation_connection_ids: BTreeSet::new(),
            disabled_connection_ids: BTreeSet::new(),
            active: None,
            connection_graph: PersistedConnectionGraph::default(),
            model_enabled_overrides: BTreeMap::new(),
            model_capability_overrides: BTreeMap::new(),
            provider_capability_overrides: BTreeMap::new(),
            context_window_policy_overrides: ContextWindowPolicyOverrides::default(),
            context_compaction_window_nodes: ContextCompactionWindowConfiguration::default(),
            new_session_context_compaction_window_node_id: None,
            internal_model_enabled_overrides: BTreeMap::new(),
            inference_profiles: BTreeMap::from([(
                String::from("vmm"),
                InferenceProfile::default(),
            )]),
            internal_model_overrides: BTreeMap::new(),
            secondary_routes: BTreeMap::new(),
            provider_tool_selections: BTreeMap::new(),
            provider_tool_enabled_capabilities: BTreeSet::new(),
            multimodal_understanding_candidate_models: BTreeSet::new(),
            media_understanding_selections: BTreeMap::new(),
            media_understanding_backend_options: BTreeMap::new(),
            provider_extension_target_sets: BTreeMap::new(),
            provider_extension_modes: BTreeMap::new(),
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
        projection.provider_extension_modes.retain(|_, selection| {
            if selection.single_target.as_ref().is_some_and(|target| {
                removed_connection_ids.contains(target.connection_id.as_str())
            }) {
                selection.single_target = None;
            }
            !selection.is_default()
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

    /// Validates a persisted document against the current schema without rewriting it.
    /// 按当前 Schema 校验持久化文档，且不改写该文档。
    ///
    /// # Returns
    /// 返回值
    /// Success for a current document, or an explicit error for any other version or invalid record.
    /// 当前文档返回成功；其他版本或无效记录返回明确错误。
    pub fn validate_current_schema(&self) -> Result<(), String> {
        if self.schema_version != PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported persisted AI configuration schema version {}; supported version is {PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION}.",
                self.schema_version
            ));
        }
        if !self.inference_profiles.contains_key("vmm") {
            return Err(
                "Persisted AI configuration is missing the required `vmm` inference profile."
                    .to_string(),
            );
        }
        for defaults in self.custom_provider_model_defaults.values() {
            defaults.validate()?;
        }
        for model in self.custom_models.values().flat_map(BTreeMap::values) {
            model.validate()?;
        }
        for decision in self.custom_model_template_decisions.values() {
            decision.validate()?;
        }
        for (key, overrides) in &self.model_capability_overrides {
            let (provider_id, _) = Self::provider_model_key_parts(key).ok_or_else(|| {
                format!("Persisted model capability override key `{key}` is malformed.")
            })?;
            overrides.validate_for_model(provider_id)?;
        }
        for (provider_id, overrides) in &self.provider_capability_overrides {
            overrides.validate_for_provider(provider_id)?;
        }
        self.context_compaction_window_nodes.validate()?;
        self.validate_context_window_policy()?;
        self.effective_media_delivery_policy().validate()?;
        Ok(())
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

    /// Validates the single global context-window policy and its resolved percentages.
    /// 校验唯一的全局上下文窗口策略及其解析后的百分比。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when no resolved route violates the Codex window ordering rules.
    /// 当所有解析路由都符合 Codex 窗口顺序规则时返回 `Ok(())`。
    pub fn validate_context_window_policy(&self) -> Result<(), String> {
        self.context_window_policy_overrides.validate()?;
        self.context_window_policy_overrides.resolve().validate()
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

    /// Saves the default node identity for conversations created after this configuration change.
    /// 保存配置变更后新建会话使用的默认节点身份。
    ///
    /// `node_id` is a fixed node or None to follow the selected model; no existing session changes.
    /// `node_id` 是固定节点，空值表示遵循所选模型；已有会话不受影响。
    pub fn set_new_session_context_compaction_window_node_id(
        &mut self,
        node_id: Option<ContextCompactionWindowNodeId>,
    ) {
        self.new_session_context_compaction_window_node_id = node_id;
    }

    /// Resolves the new-session planning node against the selected model's real context limit.
    /// 根据所选模型的真实上下文上限解析新建会话规划节点。
    ///
    /// A node at or above the model limit falls back to the model limit; missing limits do too.
    /// 节点达到或超过模型上限时遵循模型；模型上限未知时也遵循模型。
    pub fn new_session_context_compaction_window_node_for_model(
        &self,
        model_context_limit: Option<u64>,
    ) -> Result<Option<ContextCompactionWindowNodeId>, String> {
        let Some(node_id) = self.new_session_context_compaction_window_node_id else {
            return Ok(None);
        };
        let node_tokens = self.context_compaction_window_nodes.tokens_for(node_id)?;
        Ok(model_context_limit
            .filter(|limit| node_tokens < *limit)
            .map(|_| node_id))
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
        _provider_id: &str,
        _model_id: &str,
        context_limit: Option<usize>,
        compression_window_node_id: Option<ContextCompactionWindowNodeId>,
        session_policy: Option<ContextCompactionPolicy>,
    ) -> Result<ContextCompactionPolicy, String> {
        let sparse = self.context_window_policy_overrides;
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
        candidate.validate_context_window_policy()?;
        self.context_window_policy_overrides = overrides;
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
                policy.model_is_enabled_with_default(model_id, inherited_default)
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
        let policy = self
            .connection_model_access_policies
            .entry(connection_id.to_string())
            .or_default();
        policy.set_all_models_enabled(enabled);
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
        let mut policy = ConnectionModelAccessPolicy::default();
        policy.set_all_models_enabled(true);
        self.connection_model_access_policies
            .insert(connection_id.to_string(), policy);
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
        // System models accept the same sparse layer; validation keeps request projections and
        // structured output out of it, so only limits, inputs and reasoning levels apply.
        // 系统模型接受同一稀疏层；校验已排除请求投影与结构化输出，因此只应用限制、输入与思考档位。
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
        self.custom_model_template_decisions
            .remove(provider_model_key.as_str());
        self.custom_model_protocol_preferences
            .remove(provider_model_key.as_str());
        self.connection_graph
            .account_preferences
            .model_last_connections
            .remove(provider_model_key.as_str());
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
            if let Some(policy) = self
                .connection_model_access_policies
                .get_mut(connection_id.as_str())
            {
                policy.model_enabled_overrides.remove(model_id);
                if *policy == ConnectionModelAccessPolicy::default() {
                    self.connection_model_access_policies
                        .remove(connection_id.as_str());
                }
            }
        }
    }

    /// Builds an account policy that follows each signed model default until the user acts.
    /// 构建在用户操作前逐模型遵循签名默认值的账号策略。
    ///
    /// # Parameters
    /// 参数
    /// manifest supplies the provider-wide display baseline.
    /// manifest 提供供应商级展示基线。
    ///
    /// # Returns
    /// 返回值
    /// An inheriting policy without any user decisions.
    /// 返回尚无用户决定的继承型策略。
    pub fn initial_connection_model_access_policy_from_manifest(
        manifest: &crate::provider_manifest::ResolvedProviderManifest,
    ) -> ConnectionModelAccessPolicy {
        ConnectionModelAccessPolicy {
            default_enabled: manifest.default_enabled,
            inherit_model_defaults: true,
            model_enabled_overrides: BTreeMap::new(),
        }
    }

    /// Replaces colliding user model definitions with signed identities while keeping switches.
    /// 用签名身份接管冲突的用户模型定义，同时保留开关状态。
    ///
    /// # Parameters
    /// 参数
    /// `manifest` is the active signed provider authority for this reconciliation boundary.
    /// `manifest` 是本次对账边界生效的签名供应商权威。
    ///
    /// # Returns
    /// 返回值
    /// Exact model identifiers removed from the provider-owned manual directory.
    /// 从供应商持有的手工目录移除的精确模型标识。
    pub fn reconcile_signed_manual_model_collisions(
        &mut self,
        manifest: &crate::provider_manifest::ResolvedProviderManifest,
    ) -> Vec<String> {
        let provider_id = manifest.descriptor.id.as_str();
        let collisions = self
            .custom_models
            .get(provider_id)
            .into_iter()
            .flat_map(|models| models.keys())
            .filter(|model_id| manifest.has_signed_exact_model_identity(model_id))
            .cloned()
            .collect::<Vec<_>>();
        for model_id in &collisions {
            let manual_enabled = self
                .custom_models
                .get_mut(provider_id)
                .and_then(|models| models.remove(model_id))
                .expect("collision was collected from this provider's manual directory")
                .enabled;
            let model_key = Self::provider_model_key(provider_id, model_id);
            let provider_enabled = self.model_enabled_overrides.get(&model_key).copied();
            // account_decisions captures effective manual access before owner cleanup removes it.
            // account_decisions 在归属清理移除原记录前捕获手工模型的有效账号访问状态。
            let account_decisions = self
                .connection_graph
                .connections
                .values()
                .filter(|connection| connection.provider_id == provider_id)
                .map(|connection| {
                    // inherited_decision is the account switch before manual availability gates it.
                    // inherited_decision 是应用手工可用性门控前的账号开关。
                    let inherited_decision = self
                        .connection_model_access_policies
                        .get(connection.id.as_str())
                        .map_or(manual_enabled, |policy| {
                            policy.model_is_enabled_with_default(model_id, manual_enabled)
                        });
                    // enabled preserves the effective account check across signed takeover.
                    // enabled 在签名接管时保留账号的有效勾选状态。
                    let enabled = self
                        .custom_model_connection_enabled_override(
                            connection.id.as_str(),
                            model_id.as_str(),
                        )
                        .map_or(inherited_decision, |available| {
                            available && inherited_decision
                        });
                    (connection.clone(), enabled)
                })
                .collect::<Vec<_>>();
            self.remove_model_owned_settings(provider_id, model_id);
            self.model_enabled_overrides
                .insert(model_key, provider_enabled.unwrap_or(manual_enabled));
            for (connection, enabled) in account_decisions {
                if !self
                    .connection_model_access_policies
                    .contains_key(connection.id.as_str())
                {
                    // initial_policy seeds other signed and manual choices before restoring this one.
                    // initial_policy 先播种其他签名及手工选择，再恢复当前模型状态。
                    let initial_policy =
                        Self::initial_connection_model_access_policy_from_manifest(manifest);
                    self.connection_model_access_policies
                        .insert(connection.id.clone(), initial_policy);
                }
                self.set_connection_model_enabled(connection.id.as_str(), model_id, enabled);
            }
        }
        if self
            .custom_models
            .get(provider_id)
            .is_some_and(BTreeMap::is_empty)
        {
            self.custom_models.remove(provider_id);
        }
        collisions
    }

    /// Records one signed takeover notice in the same durable configuration as the removal.
    /// 在与移除操作相同的持久配置中记录一条签名接管提示。
    ///
    /// `provider_id` and `model_id` identify the exact signed model that took ownership.
    /// `provider_id` 与 `model_id` 标识已接管的精确签名模型。
    pub fn record_pending_system_model_takeover_notice(
        &mut self,
        provider_id: &str,
        model_id: &str,
    ) {
        self.pending_system_model_takeover_notices
            .entry(provider_id.to_string())
            .or_default()
            .insert(model_id.to_string());
    }

    /// Acknowledges only takeover notices that the user interface actually displayed.
    /// 仅确认用户界面实际展示过的接管提示。
    ///
    /// `notices` contains exact provider and model identities; other pending notices remain.
    /// `notices` 包含精确供应商与模型身份；其他待提示项保持不变。
    pub fn acknowledge_system_model_takeover_notices(&mut self, notices: &[(String, String)]) {
        for (provider_id, model_id) in notices {
            if let Some(models) = self
                .pending_system_model_takeover_notices
                .get_mut(provider_id)
            {
                models.remove(model_id);
                if models.is_empty() {
                    self.pending_system_model_takeover_notices
                        .remove(provider_id);
                }
            }
        }
    }

    /// Clears decisions for signed models removed by an activated provider manifest.
    /// 清理已激活供应商清单中消失的签名模型决定。
    ///
    /// # Parameters
    /// 参数
    /// `manifest` is the exact signed provider generation being activated.
    /// `manifest` 是正在激活的精确签名供应商代际。
    ///
    /// # Returns
    /// 返回值
    /// Whether the remembered inventory or any removed model's settings changed.
    /// 返回已记录目录或消失模型的设置是否改变。
    pub fn reconcile_signed_model_inventory(
        &mut self,
        manifest: &crate::provider_manifest::ResolvedProviderManifest,
    ) -> bool {
        let provider_id = manifest.descriptor.id.as_str();
        let current_ids = manifest.signed_exact_model_ids();
        if self.system_signed_model_inventory.get(provider_id) == Some(&current_ids) {
            return false;
        }
        let removed = self
            .system_signed_model_inventory
            .get(provider_id)
            .into_iter()
            .flat_map(|previous| previous.difference(&current_ids))
            .filter(|model_id| {
                !self
                    .custom_models
                    .get(provider_id)
                    .is_some_and(|models| models.contains_key(*model_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        for model_id in &removed {
            self.remove_model_owned_settings(provider_id, model_id);
        }
        self.system_signed_model_inventory
            .insert(provider_id.to_string(), current_ids);
        true
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
                .chain(profile.decision.iter())
                .any(|route| route.provider_id == provider_id)
        })
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
            if profile
                .decision
                .as_ref()
                .is_some_and(|route| route.provider_id == provider_id)
            {
                profile.decision = None;
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
mod tests;

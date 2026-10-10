//! User-owned custom model declarations and safe request projections.
//! 用户自有自定义模型声明与安全请求投影。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    CapabilitySupport, ModelCapabilities, ReasoningControlMode, ReasoningEffort, ResourceKind,
    StructuredOutputCapability,
};

use crate::provider_manifest::ToolSchemaProjectionMode;
use crate::{
    ModelCapabilityEvidence, PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES,
    PROVIDER_MODEL_IDENTIFIER_MAX_BYTES,
};

/// Default context window used by user-defined compatible providers.
/// 用户自定义兼容供应商使用的默认上下文窗口。
pub const CUSTOM_PROVIDER_DEFAULT_CONTEXT_LIMIT: u64 = 200_000;

/// Default output ceiling used with the two-hundred-thousand-token context window.
/// 与二十万 Token 上下文窗口配套使用的默认输出上限。
pub const CUSTOM_PROVIDER_DEFAULT_OUTPUT_LIMIT: u64 = 32_000;

/// Lists the OpenAI-compatible reasoning levels used by custom providers unless explicitly changed.
/// 列出自定义供应商在未被显式修改时使用的 OpenAI 兼容思考档位。
pub const CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::None,
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
    ReasoningEffort::Max,
];

/// Identifies the immutable common-capability snapshot understood by this binary.
/// 标识当前二进制理解的不可变公共能力快照。
pub const CUSTOM_MODEL_TEMPLATE_REVISION: &str = "common-model-capabilities.v1";

/// Stores provider-wide capability defaults used only when a stronger source is absent.
/// 保存仅在更强能力来源缺失时使用的供应商级默认能力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProviderModelDefaults {
    /// Default context window in tokens.
    /// 默认上下文 Token 窗口。
    #[serde(default = "default_custom_provider_context_limit")]
    pub context_limit: u64,
    /// Default output-token ceiling.
    /// 默认输出 Token 上限。
    #[serde(default = "default_custom_provider_output_limit")]
    pub output_limit: u64,
    /// Default accepted non-text input resources.
    /// 默认接受的非文本输入资源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_resources: Vec<ResourceKind>,
    /// Provider-wide structured-output capability.
    /// 供应商级结构化输出能力。
    #[serde(default)]
    pub structured_output: StructuredOutputCapability,
    /// Provider-wide reasoning support applied above catalog declarations.
    /// 应用于目录声明之上的供应商级思考支持状态。
    #[serde(default = "default_custom_provider_reasoning")]
    pub reasoning: CapabilitySupport,
    /// Provider-wide reasoning control behavior.
    /// 供应商级思考控制行为。
    #[serde(default = "default_custom_provider_reasoning_control")]
    pub reasoning_control: ReasoningControlMode,
    /// Provider-wide selectable reasoning levels.
    /// 供应商级可选思考档位。
    #[serde(default = "default_custom_provider_reasoning_efforts")]
    pub reasoning_efforts: Vec<ReasoningEffort>,
    /// Provider-wide request projection inherited by compatible models.
    /// 由兼容模型继承的供应商级请求投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_projection: Option<CustomModelRequestProjection>,
}

impl Default for CustomProviderModelDefaults {
    /// Builds the safe text-first 200K/32K provider defaults.
    /// 构建安全的纯文本优先 200K/32K 供应商默认值。
    fn default() -> Self {
        Self {
            context_limit: CUSTOM_PROVIDER_DEFAULT_CONTEXT_LIMIT,
            output_limit: CUSTOM_PROVIDER_DEFAULT_OUTPUT_LIMIT,
            input_resources: Vec::new(),
            structured_output: StructuredOutputCapability::default(),
            reasoning: default_custom_provider_reasoning(),
            reasoning_control: default_custom_provider_reasoning_control(),
            reasoning_efforts: default_custom_provider_reasoning_efforts(),
            request_projection: None,
        }
    }
}

impl CustomProviderModelDefaults {
    /// Validates that provider defaults form a usable token budget.
    /// 校验供应商默认值能够形成可用的 Token 预算。
    ///
    /// # Returns
    /// 返回值
    /// Success or one actionable validation message.
    /// 返回成功或一条可操作的校验消息。
    pub fn validate(&self) -> Result<(), String> {
        if self.context_limit == 0 {
            return Err(String::from(
                "Custom provider default context limit must be greater than zero.",
            ));
        }
        if self.output_limit == 0 {
            return Err(String::from(
                "Custom provider default output limit must be greater than zero.",
            ));
        }
        if self.input_resources.contains(&ResourceKind::Video) {
            return Err(String::from(
                "Custom provider defaults cannot enable video input without a declared provider route.",
            ));
        }
        validate_custom_provider_reasoning_settings(
            self.reasoning,
            self.reasoning_control,
            self.reasoning_efforts.as_slice(),
        )?;
        if let Some(request_projection) = self.request_projection.as_ref() {
            request_projection.validate_paths()?;
        }
        Ok(())
    }

    /// Fills only model fields that were not declared by a stronger source.
    /// 仅填充尚未由更强来源声明的模型字段。
    ///
    /// # Parameters
    /// 参数
    /// `capabilities` is the credential-scoped capability snapshot being resolved.
    /// `capabilities` 是正在解析的凭据作用域能力快照。
    pub fn apply_missing(
        &self,
        capabilities: &mut ModelCapabilities,
        evidence: &ModelCapabilityEvidence,
    ) {
        if capabilities.context_limit.is_none() {
            capabilities.context_limit = Some(self.context_limit);
        }
        if capabilities.output_limit.is_none() {
            // effective_context_limit prevents a smaller accepted template window from inheriting an unsafe provider output ceiling.
            // effective_context_limit 防止较小的已接受模板窗口继承不安全的供应商输出上限。
            let effective_context_limit = capabilities.context_limit.unwrap_or(self.context_limit);
            capabilities.output_limit = Some(self.output_limit.min(effective_context_limit));
        }
        if !evidence.input_modalities_declared && capabilities.input_resources.is_empty() {
            capabilities.input_resources = self.input_resources.clone();
        }
        if !evidence.structured_output_declared {
            capabilities.structured_output = self.structured_output;
        }
        if capabilities.reasoning == CapabilitySupport::Unknown {
            capabilities.reasoning = self.reasoning;
        }
        if capabilities.reasoning_control == ReasoningControlMode::Unknown {
            capabilities.reasoning_control = self.reasoning_control;
        }
        if capabilities.reasoning_efforts.is_empty() {
            capabilities.reasoning_efforts = self.reasoning_efforts.clone();
        }
    }

    /// Applies the system-owned custom-provider reasoning policy above catalog evidence.
    /// 在目录证据之上应用系统持有的自定义供应商思考策略。
    ///
    /// # Parameters
    /// 参数
    /// `capabilities` is the credential-scoped capability snapshot being resolved.
    /// `capabilities` 是正在解析的凭据作用域能力快照。
    pub fn apply_system_capability_policy(&self, capabilities: &mut ModelCapabilities) {
        // Unknown reasoning fields intentionally preserve protocol-specific discovery or model evidence.
        // 未知思考字段会有意保留协议专属发现结果或模型证据。
        if self.reasoning != CapabilitySupport::Unknown {
            capabilities.reasoning = self.reasoning;
        }
        if self.reasoning_control != ReasoningControlMode::Unknown {
            capabilities.reasoning_control = self.reasoning_control;
        }
        if !self.reasoning_efforts.is_empty() {
            capabilities.reasoning_efforts = self.reasoning_efforts.clone();
        }
        capabilities.evidence_revision = format!(
            "{}+custom-provider-system-policy.v1",
            capabilities.evidence_revision
        );
    }

    /// Applies one explicit provider layer above catalog and template capability declarations.
    /// 在目录与模板能力声明之上应用一层显式供应商设置。
    ///
    /// # Parameters
    /// 参数
    /// `capabilities` is the model capability value being resolved for one credential route.
    /// `capabilities` 是为一条凭据路由解析的模型能力值。
    pub fn apply_provider_settings(
        &self,
        capabilities: &mut ModelCapabilities,
        _evidence: &ModelCapabilityEvidence,
    ) {
        capabilities.context_limit = Some(self.context_limit);
        capabilities.output_limit = Some(self.output_limit);
        capabilities.input_resources = self.input_resources.clone();
        capabilities.structured_output = self.structured_output;
        capabilities.reasoning = self.reasoning;
        capabilities.reasoning_control = self.reasoning_control;
        capabilities.reasoning_efforts = self.reasoning_efforts.clone();
        capabilities.evidence_revision = format!(
            "{}+custom-provider-settings.v1",
            capabilities.evidence_revision
        );
    }
}

/// Returns the default native reasoning support for custom compatible providers.
/// 返回自定义兼容供应商默认使用的原生思考支持状态。
fn default_custom_provider_reasoning() -> CapabilitySupport {
    CapabilitySupport::Native
}

/// Returns the default effort-selection reasoning control for custom compatible providers.
/// 返回自定义兼容供应商默认使用的档位选择思考控制。
fn default_custom_provider_reasoning_control() -> ReasoningControlMode {
    ReasoningControlMode::EffortSelection
}

/// Returns an owned copy of the default OpenAI-compatible reasoning levels.
/// 返回默认 OpenAI 兼容思考档位的独立副本。
fn default_custom_provider_reasoning_efforts() -> Vec<ReasoningEffort> {
    CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.to_vec()
}

/// Validates the closed simple reasoning policies exposed by provider and model settings.
/// 校验供应商与模型设置界面暴露的封闭简化思考策略。
///
/// # Parameters
/// 参数
/// `support`, `control`, and `efforts` form one complete reasoning capability contract.
/// `support`、`control` 与 `efforts` 共同组成一份完整思考能力契约。
///
/// # Returns
/// 返回值
/// Success only for an explicit disabled policy or an enabled selectable-level policy with at least one active level.
/// 仅在显式关闭策略，或至少包含一个启用档位的可选档位策略下返回成功。
pub fn validate_custom_provider_reasoning_settings(
    support: CapabilitySupport,
    control: ReasoningControlMode,
    efforts: &[ReasoningEffort],
) -> Result<(), String> {
    // unique_efforts rejects order-independent duplicate levels that the direct editor cannot represent.
    // unique_efforts 拒绝直接编辑器无法表达的、与顺序无关的重复档位。
    let unique_efforts = efforts.iter().copied().collect::<BTreeSet<_>>();
    if unique_efforts.len() != efforts.len() {
        return Err(String::from(
            "Custom provider reasoning levels must not contain duplicates.",
        ));
    }
    if efforts
        .iter()
        .any(|effort| !CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.contains(effort))
    {
        return Err(String::from(
            "Custom provider reasoning levels are limited to the OpenAI-compatible none, minimal, low, medium, high, xhigh, and max set.",
        ));
    }
    let disabled = support == CapabilitySupport::Unsupported
        && control == ReasoningControlMode::FixedOff
        && efforts == [ReasoningEffort::None];
    let enabled = support == CapabilitySupport::Native
        && control == ReasoningControlMode::EffortSelection
        && efforts
            .iter()
            .any(|effort| *effort != ReasoningEffort::None);
    if disabled || enabled {
        return Ok(());
    }
    Err(String::from(
        "Custom provider reasoning settings must be natively enabled with at least one OpenAI-compatible enabled level, or explicitly disabled.",
    ))
}

/// Stores the confirmed common template snapshot applied to one custom-provider model.
/// 保存应用到某个自定义供应商模型的已确认公共模板快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomModelTemplateSelection {
    /// Canonical immutable template key.
    /// 不可变模板规范键。
    pub template_key: String,
    /// Template resource revision accepted by the user.
    /// 用户接受的模板资源修订号。
    pub template_revision: String,
    /// Confirmed context window.
    /// 已确认上下文窗口。
    pub context_limit: u64,
    /// Whether the reviewed template supports text input.
    /// 已审查模板是否支持文本输入。
    #[serde(default = "default_enabled")]
    pub supports_text_input: bool,
    /// Whether the reviewed template supports text output.
    /// 已审查模板是否支持文本输出。
    #[serde(default = "default_enabled")]
    pub supports_text_output: bool,
    /// Confirmed non-text input resources.
    /// 已确认非文本输入资源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_resources: Vec<ResourceKind>,
    /// Confirmed non-text output resources.
    /// 已确认非文本输出资源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_resources: Vec<ResourceKind>,
}

impl CustomModelTemplateSelection {
    /// Validates one stable user-confirmed template snapshot.
    /// 校验一份稳定的用户确认模板快照。
    ///
    /// # Returns
    /// 返回值
    /// Success or one actionable validation message.
    /// 返回成功或一条可操作的校验消息。
    pub fn validate(&self) -> Result<(), String> {
        if self.template_key.trim().is_empty() || self.template_revision.trim().is_empty() {
            return Err(String::from(
                "Custom model template key and revision are required.",
            ));
        }
        if self.context_limit == 0 {
            return Err(String::from(
                "Custom model template context limit must be greater than zero.",
            ));
        }
        if !self.output_resources.is_empty() {
            return Err(String::from(
                "Custom model templates cannot configure non-text output for a conversation provider.",
            ));
        }
        Ok(())
    }

    /// Fills fields absent from credential evidence before provider defaults are applied.
    /// 在应用供应商默认值前填充凭据证据未声明的字段。
    ///
    /// # Parameters
    /// 参数
    /// `capabilities` is the credential-scoped capability snapshot being resolved.
    /// `capabilities` 是正在解析的凭据作用域能力快照。
    pub fn apply_missing(
        &self,
        capabilities: &mut ModelCapabilities,
        evidence: &ModelCapabilityEvidence,
    ) {
        if capabilities.context_limit.is_none() {
            capabilities.context_limit = Some(self.context_limit);
        }
        if !evidence.input_modalities_declared && capabilities.input_resources.is_empty() {
            capabilities.input_resources = self.input_resources.clone();
        }
    }
}

/// Records whether the user accepted or dismissed one capability-template recommendation.
/// 记录用户是否接受或拒绝某项能力模板建议。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum CustomModelTemplateDecision {
    /// Applies one immutable reviewed snapshot.
    /// 应用一份不可变且已审查的快照。
    Applied {
        /// Accepted template snapshot.
        /// 已接受模板快照。
        selection: CustomModelTemplateSelection,
    },
    /// Suppresses the same reviewed template revision without hiding later catalog changes.
    /// 抑制同一已审查模板修订，同时不隐藏后续目录变化。
    Rejected {
        /// Rejected canonical template key.
        /// 已拒绝的规范模板键。
        template_key: String,
        /// Rejected template resource revision.
        /// 已拒绝的模板资源修订号。
        template_revision: String,
    },
}

impl CustomModelTemplateDecision {
    /// Validates one persisted template decision without consulting mutable runtime state.
    /// 校验一项持久化模板决策，且不依赖可变运行时状态。
    ///
    /// # Returns
    /// 返回值
    /// Success or one actionable validation message.
    /// 返回成功或一条可操作的校验消息。
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Applied { selection } => selection.validate(),
            Self::Rejected {
                template_key,
                template_revision,
            } => {
                if template_key.trim().is_empty() || template_revision.trim().is_empty() {
                    return Err(String::from(
                        "Rejected custom model template key and revision are required.",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Returns the serialized default context limit.
/// 返回用于序列化的默认上下文限制。
fn default_custom_provider_context_limit() -> u64 {
    CUSTOM_PROVIDER_DEFAULT_CONTEXT_LIMIT
}

/// Returns the serialized default output limit.
/// 返回用于序列化的默认输出上限。
fn default_custom_provider_output_limit() -> u64 {
    CUSTOM_PROVIDER_DEFAULT_OUTPUT_LIMIT
}

/// Lists request-root fields reserved for protocol and host ownership.
/// 列出由协议与宿主持有的请求根字段。
const RESERVED_REQUEST_ROOTS: &[&str] = &[
    "model",
    "messages",
    "input",
    "instructions",
    "system",
    "tools",
    "tool_choice",
    "stream",
    "stream_options",
];

/// Defines one validated mutation applied to a standard protocol request.
/// 定义应用到标准协议请求的一项已校验变更。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CustomRequestMutation {
    /// Exact JSON object paths and values to set.
    /// 要设置的精确 JSON 对象路径和值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, Value>,
    /// Exact JSON object paths to delete.
    /// 要删除的精确 JSON 对象路径。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delete: Vec<String>,
}

/// Defines the closed projection contract attached to one custom model.
/// 定义附加到一个自定义模型的封闭投影契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CustomModelRequestProjection {
    /// Canonical reasoning levels to exact request mutations.
    /// 规范思考档位到精确请求变更的映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reasoning: BTreeMap<ReasoningEffort, CustomRequestMutation>,
    /// Registered model parameter identifiers to exact request paths.
    /// 已登记模型参数标识到精确请求路径的映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameter_paths: BTreeMap<String, String>,
    /// Fields added only when the standard protocol body omitted the path.
    /// 仅在标准协议请求体缺少路径时添加的字段。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, Value>,
    /// Fields that intentionally replace non-reserved standard fields.
    /// 有意替换非保留标准字段的字段。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Value>,
    /// Non-reserved standard fields removed after protocol encoding.
    /// 协议编码后删除的非保留标准字段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delete: Vec<String>,
}

/// Selects how one custom-provider model binds its execution protocol.
/// 选择一个自定义供应商模型如何绑定其执行协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CustomModelProtocolPreference {
    /// Auto-resolved by fixed priority at the configuration activation boundary.
    /// 在配置激活边界按固定优先级自动解析。
    #[default]
    Auto,
    /// Explicit protocol choice that must stay inside the enabled protocol set.
    /// 必须保持在启用协议集合内的显式协议选择。
    Explicit {
        /// The exact protocol selected for this model.
        /// 为该模型选定的精确协议。
        protocol: crate::CustomProviderProtocol,
    },
}

/// Binds one user-added system model to a verified service entry and protocol route.
/// 将一个用户新增的系统模型绑定到已验证的服务入口和协议路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemManualModelBinding {
    /// Exact system service entry that owns this model.
    /// 持有此模型的精确系统服务入口。
    pub service_entry_id: String,
    /// Exact verified protocol profile selected by the user.
    /// 用户选择的精确已验证协议 Profile。
    pub protocol_profile_id: String,
}

impl CustomModelProtocolPreference {
    /// Returns the explicitly selected protocol, or `None` for automatic resolution.
    /// 返回显式选定的协议；自动解析时返回 `None`。
    pub fn explicit_protocol(self) -> Option<crate::CustomProviderProtocol> {
        match self {
            Self::Auto => None,
            Self::Explicit { protocol } => Some(protocol),
        }
    }
}

/// Stores one user-owned model without duplicating provider or credential state.
/// 保存一个不复制供应商或凭据状态的用户自有模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomModelDefinition {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub display_name: String,
    /// Whether this model is available to workbench selection.
    /// 此模型是否可供工作台选择。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Explicit provider-neutral capability declaration.
    /// 显式供应商无关能力声明。
    pub capabilities: ModelCapabilities,
    /// Optional safe post-protocol request projection.
    /// 可选的安全协议后请求投影。
    #[serde(default)]
    pub request_projection: CustomModelRequestProjection,
    /// Provider-facing tool-schema projection selected in advanced settings.
    /// 在高级设置中选择的 Provider 可见工具 Schema 投影。
    #[serde(default)]
    pub tool_schema_projection: ToolSchemaProjectionMode,
    /// Explicit route for a user-added system-provider model; absent for custom providers.
    /// 用户新增系统供应商模型的显式路由；自定义供应商不设置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_binding: Option<SystemManualModelBinding>,
}

impl CustomModelDefinition {
    /// Validates identifiers, capabilities, and every declared request path.
    /// 校验标识、能力以及每一条已声明请求路径。
    ///
    /// # Returns
    /// 返回值
    /// Success or one actionable pre-save validation message.
    /// 返回成功或一条可操作的保存前校验消息。
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > PROVIDER_MODEL_IDENTIFIER_MAX_BYTES
            || self.id.chars().any(char::is_control)
        {
            return Err(format!(
                "Custom model identifier must be non-empty, contain no control characters, and fit within {PROVIDER_MODEL_IDENTIFIER_MAX_BYTES} UTF-8 bytes."
            ));
        }
        if self.display_name.trim().is_empty()
            || self.display_name.len() > PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES
            || self.display_name.chars().any(char::is_control)
        {
            return Err(format!(
                "Custom model display name must be non-empty, contain no control characters, and fit within {PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES} UTF-8 bytes."
            ));
        }
        if self.capabilities.context_limit == Some(0) {
            return Err(String::from(
                "Custom model context limit must be greater than zero.",
            ));
        }
        if self.capabilities.output_limit == Some(0) {
            return Err(String::from(
                "Custom model output limit must be greater than zero.",
            ));
        }
        if !self.capabilities.output_resources.is_empty() {
            return Err(String::from(
                "Custom conversation models cannot declare non-text output resources.",
            ));
        }
        if let Some(binding) = &self.system_binding
            && (binding.service_entry_id.trim().is_empty()
                || binding.protocol_profile_id.trim().is_empty())
        {
            return Err(String::from(
                "System manual model binding requires a service entry and protocol profile.",
            ));
        }
        self.validate_model_parameter_definitions()?;
        for effort in self.request_projection.reasoning.keys() {
            if !self.capabilities.reasoning_efforts.contains(effort) {
                return Err(format!(
                    "Reasoning projection `{}` is not declared by the model capability.",
                    effort.as_str()
                ));
            }
        }
        for parameter_id in self.request_projection.model_parameter_paths.keys() {
            if !self
                .capabilities
                .model_parameters
                .iter()
                .any(|definition| definition.id == *parameter_id)
            {
                return Err(format!(
                    "Request projection references undeclared model parameter `{parameter_id}`."
                ));
            }
        }
        for path in self.request_projection.all_paths() {
            validate_request_path(path)?;
        }
        self.request_projection.validate_path_ownership()?;
        Ok(())
    }

    /// Validates every custom parameter definition before it can enter persisted configuration.
    /// 在每个自定义参数定义进入持久化配置前进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success only when identifiers, defaults, dependencies, conflicts, and request paths agree.
    /// 仅当标识、默认值、依赖、冲突与请求路径一致时返回成功。
    fn validate_model_parameter_definitions(&self) -> Result<(), String> {
        vulcan_ai_contract::validate_model_parameter_definition_set(
            self.capabilities.model_parameters.as_slice(),
        )?;
        for definition in &self.capabilities.model_parameters {
            if !self
                .request_projection
                .model_parameter_paths
                .contains_key(definition.id.as_str())
            {
                return Err(format!(
                    "Custom model parameter `{}` requires an exact request path.",
                    definition.id
                ));
            }
        }
        Ok(())
    }
}

/// Validates a user-added conversation model against one active signed system provider route.
/// 按一个已激活的系统供应商签名路由校验用户新增的会话模型。
///
/// # Parameters
/// 参数
/// `manifest` owns the service and verified wire declarations; `model` supplies the explicit
/// user identity, capabilities, and selected service/protocol binding.
/// `manifest` 持有服务及已验证线协议声明；`model` 提供用户明确指定的身份、能力和服务/协议绑定。
///
/// # Returns
/// 返回值
/// Success only when this is a text-output conversation model with a usable exact route.
/// 仅当模型是文本输出的会话模型且具有可用精确路由时成功。
pub fn validate_system_manual_model_definition(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    model: &CustomModelDefinition,
) -> Result<(), String> {
    model.validate()?;
    let binding = model.system_binding.as_ref().ok_or_else(|| {
        format!(
            "System manual model `{}` requires an explicit service entry and protocol profile.",
            model.id
        )
    })?;
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == binding.service_entry_id)
        .ok_or_else(|| {
            format!(
                "System provider `{}` has no service entry `{}` for manual model `{}`.",
                manifest.descriptor.id, binding.service_entry_id, model.id
            )
        })?;
    if entry.kind != crate::ServiceEntryKind::Conversation {
        return Err(format!(
            "Service entry `{}` does not accept manual conversation models.",
            binding.service_entry_id
        ));
    }
    let route = entry
        .protocol_routes
        .iter()
        .find(|route| route.protocol_profile_id == binding.protocol_profile_id && route.verified)
        .ok_or_else(|| {
            format!(
                "Service entry `{}` has no verified protocol route `{}` for manual model `{}`.",
                binding.service_entry_id, binding.protocol_profile_id, model.id
            )
        })?;
    if manifest.has_signed_exact_model_identity(model.id.as_str()) {
        return Err(format!(
            "System manual model `{}` conflicts with a signed model identity for provider `{}`.",
            model.id, manifest.descriptor.id
        ));
    }
    if model.capabilities.operations.as_slice()
        != [vulcan_ai_contract::AiOperation::ConversationRespond]
    {
        return Err(format!(
            "System manual model `{}` must declare only the conversation_respond operation.",
            model.id
        ));
    }
    if model
        .capabilities
        .context_limit
        .is_none_or(|limit| limit == 0)
        || model
            .capabilities
            .output_limit
            .is_none_or(|limit| limit == 0)
    {
        return Err(format!(
            "System manual model `{}` requires explicit positive context and output limits.",
            model.id
        ));
    }
    if model.capabilities.output_limit > model.capabilities.context_limit {
        return Err(format!(
            "System manual model `{}` output limit cannot exceed its context limit.",
            model.id
        ));
    }
    // System request projections are owned by the signed route; accepting a user projection
    // here would silently discard it in the current execution path.
    // 系统请求投影由签名路由持有；当前执行链会静默丢弃用户投影，因此在入口明确拒绝。
    if model.request_projection != CustomModelRequestProjection::default() {
        return Err(format!(
            "System manual model `{}` cannot override the signed request projection.",
            model.id
        ));
    }
    if model.tool_schema_projection != ToolSchemaProjectionMode::Lightweight {
        return Err(format!(
            "System manual model `{}` cannot override the signed tool-schema projection.",
            model.id
        ));
    }
    for resource in &model.capabilities.input_resources {
        // A model declaration cannot make a wire projector encode a modality it does not own.
        // 模型声明不能让 Wire 投影器编码其本身不支持的输入模态。
        let mime = match resource {
            vulcan_ai_contract::ResourceKind::Image => "image/png",
            vulcan_ai_contract::ResourceKind::Audio => "audio/wav",
            vulcan_ai_contract::ResourceKind::Video => "video/mp4",
            vulcan_ai_contract::ResourceKind::File => "application/pdf",
        };
        let protocol_can_encode = [
            vulcan_ai_contract::ResourceSourceKind::InlineData,
            vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
            vulcan_ai_contract::ResourceSourceKind::ProviderFileId,
        ]
        .into_iter()
        .any(|source| {
            vulcan_llm_core::protocol_profile_supports_media_source(
                binding.protocol_profile_id.as_str(),
                *resource,
                mime,
                source,
            )
        });
        if !protocol_can_encode {
            return Err(format!(
                "Protocol `{}` cannot encode {resource:?} input for manual model `{}`.",
                binding.protocol_profile_id, model.id
            ));
        }
        if resource == &vulcan_ai_contract::ResourceKind::Video
            && !route
                .input_resource_constraints
                .iter()
                .any(|constraint| constraint.resource_kind == *resource)
        {
            return Err(format!(
                "Service entry `{}` protocol `{}` does not declare video input for manual model `{}`.",
                binding.service_entry_id, binding.protocol_profile_id, model.id
            ));
        }
    }
    Ok(())
}

impl CustomModelRequestProjection {
    /// Validates only exact request paths and ownership rules independent of a model catalog.
    /// 仅校验精确请求路径与归属规则，不依赖模型目录。
    ///
    /// # Returns
    /// 返回值
    /// Success when every configured path is safe and no projection section overlaps.
    /// 当所有配置路径安全且各投影区段不存在重叠时返回成功。
    pub fn validate_paths(&self) -> Result<(), String> {
        for path in self.all_paths() {
            validate_request_path(path)?;
        }
        self.validate_path_ownership()
    }

    /// Validates reasoning projection keys against one resolved model capability contract.
    /// 根据一份已解析的模型能力契约校验思考投影档位。
    ///
    /// # Parameters
    /// 参数
    /// `capabilities` supplies the effective declared reasoning levels for the exact model.
    /// `capabilities` 提供精确模型的有效已声明思考档位。
    ///
    /// # Returns
    /// 返回值
    /// Success when paths are safe and every mapped effort is declared by the model.
    /// 当路径安全且每个映射档位都由模型声明时返回成功。
    pub fn validate_for_capabilities(
        &self,
        capabilities: &ModelCapabilities,
    ) -> Result<(), String> {
        self.validate_paths()?;
        for effort in self.reasoning.keys() {
            if !capabilities.reasoning_efforts.contains(effort) {
                return Err(format!(
                    "Reasoning projection `{}` is not declared by the model capability.",
                    effort.as_str()
                ));
            }
        }
        Ok(())
    }

    /// Overlays one higher-precedence projection without discarding inherited rules.
    /// 将一层更高优先级投影叠加到当前投影上，同时保留可继承规则。
    ///
    /// # Parameters
    /// 参数
    /// `overlay` contains only the higher layer's explicit rules.
    /// `overlay` 仅包含更高层明确声明的规则。
    ///
    /// # Returns
    /// 返回值
    /// Success when the merged projection remains a valid closed request contract.
    /// 合并后的投影仍是有效封闭请求契约时返回成功。
    pub fn merge_overlay(&mut self, overlay: &Self) -> Result<(), String> {
        self.validate_paths()?;
        overlay.validate_paths()?;

        for (effort, mutation) in &overlay.reasoning {
            for path in mutation.set.keys().chain(mutation.delete.iter()) {
                remove_path_from_global_sections(self, path);
            }
            let merged = self.reasoning.entry(*effort).or_default();
            for path in &mutation.delete {
                remove_overlapping_paths(&mut merged.set, path);
                merged
                    .delete
                    .retain(|existing| !request_paths_overlap(existing, path));
                if !merged.delete.iter().any(|existing| existing == path) {
                    merged.delete.push(path.clone());
                }
            }
            for (path, value) in &mutation.set {
                remove_overlapping_paths(&mut merged.set, path);
                merged
                    .delete
                    .retain(|existing| !request_paths_overlap(existing, path));
                merged.set.insert(path.clone(), value.clone());
            }
        }

        for (parameter_id, path) in &overlay.model_parameter_paths {
            self.model_parameter_paths
                .remove(parameter_id)
                .into_iter()
                .for_each(|previous_path| remove_path_from_all_sections(self, &previous_path));
            remove_path_from_all_sections(self, path);
            self.model_parameter_paths
                .insert(parameter_id.clone(), path.clone());
        }
        for (path, value) in &overlay.defaults {
            remove_path_from_all_sections(self, path);
            self.defaults.insert(path.clone(), value.clone());
        }
        for (path, value) in &overlay.overrides {
            remove_path_from_all_sections(self, path);
            self.overrides.insert(path.clone(), value.clone());
        }
        for path in &overlay.delete {
            remove_path_from_all_sections(self, path);
            self.delete.push(path.clone());
        }
        self.delete.sort();
        self.delete.dedup();
        self.validate_paths()
    }

    /// Iterates every request path declared by this projection.
    /// 迭代此投影声明的每一条请求路径。
    fn all_paths(&self) -> impl Iterator<Item = &str> {
        self.defaults
            .keys()
            .chain(self.overrides.keys())
            .chain(self.delete.iter())
            .chain(self.model_parameter_paths.values())
            .chain(
                self.reasoning
                    .values()
                    .flat_map(|mutation| mutation.set.keys().chain(mutation.delete.iter())),
            )
            .map(String::as_str)
    }

    /// Rejects ambiguous exact-path ownership across mutually exclusive projection rules.
    /// 拒绝互斥投影规则之间含义不明确的相同精确路径归属。
    fn validate_path_ownership(&self) -> Result<(), String> {
        let mut owned_paths = BTreeMap::<&str, &str>::new();
        register_unique_paths(
            &mut owned_paths,
            self.defaults.keys().map(String::as_str),
            "defaults",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.model_parameter_paths.values().map(String::as_str),
            "model_parameter_paths",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.delete.iter().map(String::as_str),
            "delete",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.overrides.keys().map(String::as_str),
            "overrides",
        )?;

        for (effort, mutation) in &self.reasoning {
            // Each effort starts from the global ownership set so different mutually exclusive
            // efforts may reuse one wire path while one effort cannot collide with global rules.
            // 每个档位从全局归属集合开始，因此互斥档位可复用线级路径，单个档位不能与全局规则冲突。
            let mut effort_paths = owned_paths.clone();
            register_unique_paths(
                &mut effort_paths,
                mutation.set.keys().map(String::as_str),
                "reasoning.set",
            )
            .map_err(|error| {
                format!(
                    "Reasoning projection `{}` is invalid: {error}",
                    effort.as_str()
                )
            })?;
            register_unique_paths(
                &mut effort_paths,
                mutation.delete.iter().map(String::as_str),
                "reasoning.delete",
            )
            .map_err(|error| {
                format!(
                    "Reasoning projection `{}` is invalid: {error}",
                    effort.as_str()
                )
            })?;
        }
        Ok(())
    }
}

/// Removes one path from global projection sections before a higher layer claims it.
/// 在更高层声明一条路径前，从全局投影区段移除该路径。
fn remove_path_from_global_sections(projection: &mut CustomModelRequestProjection, path: &str) {
    projection
        .defaults
        .retain(|existing, _| !request_paths_overlap(existing, path));
    projection
        .overrides
        .retain(|existing, _| !request_paths_overlap(existing, path));
    projection
        .delete
        .retain(|existing| !request_paths_overlap(existing, path));
    projection
        .model_parameter_paths
        .retain(|_, existing| !request_paths_overlap(existing, path));
}

/// Removes one path from every inherited projection section before replacement.
/// 在替换前，从所有继承投影区段移除一条路径。
fn remove_path_from_all_sections(projection: &mut CustomModelRequestProjection, path: &str) {
    remove_path_from_global_sections(projection, path);
    for mutation in projection.reasoning.values_mut() {
        remove_overlapping_paths(&mut mutation.set, path);
        mutation
            .delete
            .retain(|existing| !request_paths_overlap(existing, path));
    }
    projection
        .reasoning
        .retain(|_, mutation| !mutation.set.is_empty() || !mutation.delete.is_empty());
}

/// Removes paths structurally overlapping one higher-precedence path.
/// 移除与一条更高优先级路径存在结构重叠的路径。
fn remove_overlapping_paths(values: &mut BTreeMap<String, Value>, path: &str) {
    values.retain(|existing, _| !request_paths_overlap(existing, path));
}

/// Registers exact paths for one mutually exclusive projection section.
/// 为一个互斥投影区段登记精确路径。
fn register_unique_paths<'a>(
    owned_paths: &mut BTreeMap<&'a str, &'static str>,
    paths: impl Iterator<Item = &'a str>,
    owner: &'static str,
) -> Result<(), String> {
    for path in paths {
        if let Some((previous_path, previous_owner)) = owned_paths
            .iter()
            .find(|(previous_path, _)| request_paths_overlap(previous_path, path))
        {
            return Err(format!(
                "Custom request path `{path}` conflicts with `{previous_path}` declared by `{previous_owner}` and `{owner}` rules."
            ));
        }
        owned_paths.insert(path, owner);
    }
    Ok(())
}

/// Reports whether two dot-separated request paths are equal or structurally nested.
/// 报告两条点分隔请求路径是否相同或具有结构嵌套关系。
///
/// # Parameters
/// 参数
/// `left` and `right` are validated exact object paths.
/// `left` 与 `right` 是已校验的精确对象路径。
///
/// # Returns
/// 返回值
/// True when assigning either path can change the parent shape required by the other.
/// 当写入任一路径可能改变另一路径所需的父级结构时返回真。
fn request_paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('.'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

/// Returns the default visible state for a newly added custom model.
/// 返回新添加自定义模型的默认可见状态。
fn default_enabled() -> bool {
    true
}

/// Validates one dot-separated request path against syntax and ownership rules.
/// 根据语法与所有权规则校验一条点分隔请求路径。
fn validate_request_path(path: &str) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
    {
        return Err(format!(
            "Custom request path `{path}` must contain dot-separated alphanumeric object keys."
        ));
    }
    if RESERVED_REQUEST_ROOTS.contains(&segments[0]) {
        return Err(format!(
            "Custom request path `{path}` targets host-owned root `{}`.",
            segments[0]
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::ModelParameterDefault;

    /// Verifies user system models keep route authority and reject undeclared media input.
    /// 验证用户系统模型遵守路由权威，并拒绝未声明的媒体输入。
    #[test]
    fn system_manual_model_requires_exact_verified_media_route() {
        let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = catalog
            .manifest("deepseek")
            .expect("bundled DeepSeek manifest");
        let mut model = CustomModelDefinition {
            id: String::from("deepseek-manual-probe"),
            display_name: String::from("Manual Probe"),
            enabled: true,
            capabilities: ModelCapabilities {
                operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
                context_limit: Some(128_000),
                output_limit: Some(8_000),
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: Some(SystemManualModelBinding {
                service_entry_id: String::from("official-api"),
                protocol_profile_id: String::from("openai-chat-completions.v1"),
            }),
        };
        validate_system_manual_model_definition(manifest, &model)
            .expect("verified text route should admit a user model");
        model.id = String::from("deepseek-flash");
        assert!(
            validate_system_manual_model_definition(manifest, &model)
                .expect_err("a signed exact identity cannot be redefined by the user")
                .contains("conflicts with a signed model identity")
        );
        model.id = String::from("deepseek-manual-probe");
        model.capabilities.output_limit = Some(256_000);
        assert!(
            validate_system_manual_model_definition(manifest, &model)
                .expect_err("output must fit the declared context")
                .contains("cannot exceed its context limit")
        );
        model.capabilities.output_limit = Some(8_000);
        model
            .request_projection
            .defaults
            .insert(String::from("temperature"), serde_json::json!(0.3));
        assert!(
            validate_system_manual_model_definition(manifest, &model)
                .expect_err("system request projection remains manifest-owned")
                .contains("cannot override the signed request projection")
        );
        model.request_projection.defaults.clear();
        model.capabilities.input_resources = vec![vulcan_ai_contract::ResourceKind::Video];
        assert!(
            validate_system_manual_model_definition(manifest, &model)
                .expect_err("video needs an entry-level declaration")
                .contains("does not declare video input")
        );
        model.capabilities.input_resources = vec![vulcan_ai_contract::ResourceKind::Audio];
        model
            .system_binding
            .as_mut()
            .expect("manual binding")
            .protocol_profile_id = String::from("anthropic-messages.v1");
        assert!(
            validate_system_manual_model_definition(manifest, &model)
                .expect_err("Anthropic has no native audio encoder")
                .contains("cannot encode Audio input")
        );
    }

    /// Verifies prefix capability rules do not claim an unlisted exact user model identity.
    /// 验证前缀能力规则不会占有未列入签名目录的精确用户模型身份。
    #[test]
    fn system_manual_model_may_choose_route_under_prefix_rule() {
        let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = catalog.manifest("openai").expect("bundled OpenAI manifest");
        let model = CustomModelDefinition {
            id: String::from("gpt-5.4-pro-manual-probe"),
            display_name: String::from("Manual Prefix Probe"),
            enabled: true,
            capabilities: ModelCapabilities {
                operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
                context_limit: Some(200_000),
                output_limit: Some(32_000),
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: Some(SystemManualModelBinding {
                service_entry_id: String::from("public-api"),
                protocol_profile_id: String::from("openai-responses.v1"),
            }),
        };
        validate_system_manual_model_definition(manifest, &model)
            .expect("a prefix rule cannot forbid an explicitly selected verified route");
    }

    /// Verifies custom-provider defaults do not imply a video route they cannot declare.
    /// 验证自定义供应商默认值不能暗示一条无法声明的视频路由。
    #[test]
    fn custom_provider_defaults_reject_undeclared_video_input() {
        let mut defaults = CustomProviderModelDefaults::default();
        defaults.input_resources.push(ResourceKind::Video);
        assert!(
            defaults
                .validate()
                .expect_err("custom route has no video input declaration")
                .contains("cannot enable video input")
        );
    }

    /// Verifies legacy-free custom model JSON still uses the declared lightweight default when the advanced field is absent.
    /// 验证自定义模型 JSON 缺少高级字段时仍使用已声明的轻量默认值。
    #[test]
    fn custom_model_projection_defaults_to_lightweight() {
        let definition: CustomModelDefinition = serde_json::from_value(serde_json::json!({
            "id": "custom-model",
            "display_name": "Custom Model",
            "enabled": true,
            "capabilities": ModelCapabilities::default(),
            "request_projection": CustomModelRequestProjection::default()
        }))
        .expect("omitted advanced projection must deserialize");

        assert_eq!(
            definition.tool_schema_projection,
            ToolSchemaProjectionMode::Lightweight
        );
    }

    /// Verifies provider defaults clamp output to a stronger effective context window.
    /// 验证供应商默认输出会收窄到更强来源给出的有效上下文窗口。
    #[test]
    fn provider_defaults_clamp_output_to_effective_context() {
        let defaults = CustomProviderModelDefaults::default();
        let mut capabilities = ModelCapabilities {
            context_limit: Some(8_192),
            ..ModelCapabilities::default()
        };

        defaults.apply_missing(&mut capabilities, &ModelCapabilityEvidence::default());

        assert_eq!(capabilities.context_limit, Some(8_192));
        assert_eq!(capabilities.output_limit, Some(8_192));
    }

    /// Verifies direct custom-provider settings cannot persist levels absent from their OpenAI-compatible editor.
    /// 验证直接自定义供应商设置不能持久化其 OpenAI 兼容编辑器无法呈现的档位。
    #[test]
    fn provider_settings_reject_non_openai_reasoning_levels() {
        let settings = CustomProviderModelDefaults {
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::Adaptive],
            ..CustomProviderModelDefaults::default()
        };

        let error = settings
            .validate()
            .expect_err("non-OpenAI-compatible level must be rejected");

        assert!(error.contains("none, minimal, low, medium, high, xhigh, and max"));
    }

    /// Verifies the off choice is optional for always-reasoning models while an enabled level remains mandatory.
    /// 验证始终思考的模型可不提供关闭选项，同时仍必须保留一个启用档位。
    #[test]
    fn provider_settings_allow_optional_reasoning_off_choice_but_require_enabled_level() {
        let settings = CustomProviderModelDefaults {
            reasoning_efforts: vec![ReasoningEffort::Low, ReasoningEffort::Medium],
            ..CustomProviderModelDefaults::default()
        };
        settings
            .validate()
            .expect("enabled reasoning without an off choice must remain valid");

        let invalid = CustomProviderModelDefaults {
            reasoning_efforts: vec![ReasoningEffort::None],
            ..CustomProviderModelDefaults::default()
        };
        let error = invalid
            .validate()
            .expect_err("enabled reasoning without an active level must be rejected");

        assert!(error.contains("at least one OpenAI-compatible enabled level"));
    }

    /// Verifies advanced manual models cannot reopen non-text output for a custom provider.
    /// 验证高级手工模型不能为自定义供应商重新开启非文本输出。
    #[test]
    fn custom_model_rejects_non_text_output() {
        let model = CustomModelDefinition {
            id: String::from("custom-model"),
            display_name: String::from("Custom Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                output_resources: vec![ResourceKind::Image],
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: None,
        };

        let error = model
            .validate()
            .expect_err("non-text output must be rejected");

        assert!(error.contains("non-text output resources"));
    }

    /// Verifies a manual model may retain an oversized declaration for later effective clamping.
    /// 验证手工模型可以保留超出上下文窗口的声明，并在有效投影阶段收窄。
    #[test]
    fn custom_model_accepts_output_larger_than_context_for_effective_clamping() {
        let model = CustomModelDefinition {
            id: String::from("custom-model"),
            display_name: String::from("Custom Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                context_limit: Some(8_192),
                output_limit: Some(9_000),
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: None,
        };

        model
            .validate()
            .expect("output larger than context must be accepted before effective clamping");
    }

    /// Verifies manual model identity fields share the bounded runtime catalog contract.
    /// 验证手工模型身份字段与运行时目录共享同一有界契约。
    #[test]
    fn custom_model_rejects_unbounded_or_controlled_identity_text() {
        let oversized = CustomModelDefinition {
            id: "x".repeat(PROVIDER_MODEL_IDENTIFIER_MAX_BYTES + 1),
            display_name: String::from("Custom Model"),
            enabled: true,
            capabilities: ModelCapabilities::default(),
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: None,
        };
        assert!(oversized.validate().is_err());

        let controlled = CustomModelDefinition {
            id: String::from("custom-model"),
            display_name: String::from("Custom\nModel"),
            enabled: true,
            capabilities: ModelCapabilities::default(),
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: None,
        };
        assert!(controlled.validate().is_err());
    }

    /// Verifies host-owned request roots cannot be changed by custom providers.
    /// 验证自定义供应商不能修改宿主持有的请求根字段。
    #[test]
    fn custom_projection_rejects_host_owned_roots() {
        let error = validate_request_path("messages.0.content").expect_err("must reject");
        assert!(error.contains("host-owned root"));
    }

    /// Verifies nested provider-owned object fields remain available.
    /// 验证嵌套的供应商自有对象字段仍然可用。
    #[test]
    fn custom_projection_accepts_safe_nested_paths() {
        validate_request_path("reasoning.budget_tokens").expect("safe path");
    }

    /// Verifies fixed projection sections cannot silently override each other by declaration order.
    /// 验证固定投影区段不能通过声明顺序静默互相覆盖。
    #[test]
    fn custom_projection_rejects_mutually_exclusive_exact_paths() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("vendor.mode"),
                Value::String(String::from("auto")),
            )]),
            delete: vec![String::from("vendor.mode")],
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("must reject conflict");

        assert!(error.contains("defaults"));
        assert!(error.contains("delete"));
        assert!(error.contains("vendor.mode"));
    }

    /// Verifies one reasoning level cannot both set and delete the same request path.
    /// 验证单个思考档位不能同时设置并删除同一请求路径。
    #[test]
    fn custom_projection_rejects_reasoning_set_delete_conflict() {
        let projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                CustomRequestMutation {
                    set: BTreeMap::from([(String::from("thinking.mode"), Value::Bool(true))]),
                    delete: vec![String::from("thinking.mode")],
                },
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("must reject conflict");

        assert!(error.contains("reasoning.set"));
        assert!(error.contains("reasoning.delete"));
        assert!(error.contains("thinking.mode"));
    }

    /// Verifies parent and child paths cannot create an order-dependent request shape.
    /// 验证父路径与子路径不能创建依赖执行顺序的请求结构。
    #[test]
    fn custom_projection_rejects_structurally_overlapping_paths() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("vendor"),
                Value::String(String::from("scalar")),
            )]),
            overrides: BTreeMap::from([(
                String::from("vendor.mode"),
                Value::String(String::from("auto")),
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("nested paths must be rejected");

        assert!(error.contains("vendor"));
        assert!(error.contains("vendor.mode"));
    }

    /// Verifies one reasoning mutation cannot collide with a global projection path.
    /// 验证单个思考变更不能与全局投影路径冲突。
    #[test]
    fn custom_projection_rejects_reasoning_and_global_path_conflicts() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("thinking.mode"),
                Value::String(String::from("disabled")),
            )]),
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking"),
                        Value::Object(Default::default()),
                    )]),
                    delete: Vec::new(),
                },
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("reasoning path must not overlap global path");

        assert!(error.contains("Reasoning projection `high`"));
        assert!(error.contains("thinking.mode"));
        assert!(error.contains("thinking"));
    }

    /// Verifies every user-declared parameter has one explicit safe wire path.
    /// 验证每个用户声明参数都有一条显式安全的线级路径。
    #[test]
    fn custom_model_parameter_requires_request_path() {
        let model = CustomModelDefinition {
            id: String::from("custom-model"),
            display_name: String::from("Custom Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                model_parameters: vec![vulcan_ai_contract::ModelParameterDefinition {
                    id: String::from("vendor.flag"),
                    display_name: String::from("Vendor flag"),
                    description: String::from("Controls one verified vendor flag."),
                    schema: vulcan_ai_contract::ModelParameterSchema::Boolean {},
                    default: ModelParameterDefault::ProviderDefault {},
                    protocol_ids: Vec::new(),
                    model_ids: Vec::new(),
                    dependent_required: Vec::new(),
                    conflicts: Vec::new(),
                    evidence_revision: String::from("user.custom-model.v1"),
                }],
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::Lightweight,
            system_binding: None,
        };

        let error = model
            .validate()
            .expect_err("parameter without a path must be rejected");

        assert!(error.contains("requires an exact request path"));
    }

    /// Verifies unknown custom projection fields fail closed instead of being silently ignored.
    /// 验证未知自定义投影字段会封闭失败而不是被静默忽略。
    #[test]
    fn custom_projection_rejects_unknown_serialized_fields() {
        let error = serde_json::from_value::<CustomModelRequestProjection>(serde_json::json!({
            "headers": {"authorization": "unsafe"}
        }))
        .expect_err("unknown projection fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies a model projection overrides one inherited field while retaining another effort.
    /// 验证模型投影只覆盖一条继承字段，同时保留另一个思考档位。
    #[test]
    fn custom_projection_overlay_preserves_inherited_reasoning_levels() {
        let mut provider_projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([
                (
                    ReasoningEffort::Low,
                    CustomRequestMutation {
                        set: BTreeMap::from([(
                            String::from("thinking.type"),
                            Value::String(String::from("enabled")),
                        )]),
                        delete: Vec::new(),
                    },
                ),
                (
                    ReasoningEffort::High,
                    CustomRequestMutation {
                        set: BTreeMap::from([(
                            String::from("reasoning_effort"),
                            Value::String(String::from("high")),
                        )]),
                        delete: Vec::new(),
                    },
                ),
            ]),
            ..CustomModelRequestProjection::default()
        };
        let model_projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::Low,
                CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("reasoning_effort"),
                        Value::String(String::from("low")),
                    )]),
                    delete: Vec::new(),
                },
            )]),
            ..CustomModelRequestProjection::default()
        };

        provider_projection
            .merge_overlay(&model_projection)
            .expect("compatible projection layers should merge");

        assert_eq!(
            provider_projection
                .reasoning
                .get(&ReasoningEffort::Low)
                .and_then(|mutation| mutation.set.get("thinking.type")),
            Some(&Value::String(String::from("enabled"))),
        );
        assert_eq!(
            provider_projection
                .reasoning
                .get(&ReasoningEffort::Low)
                .and_then(|mutation| mutation.set.get("reasoning_effort")),
            Some(&Value::String(String::from("low"))),
        );
        assert!(
            provider_projection
                .reasoning
                .contains_key(&ReasoningEffort::High)
        );
    }
}

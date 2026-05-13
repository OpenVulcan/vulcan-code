//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use crate::runtime_assets;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Provider identifiers that are intentionally excluded because their protocol or auth flow is not supported yet.
/// 因当前尚未支持其协议或认证流程而被刻意排除的供应商标识集合。
const UNSUPPORTED_PROVIDER_IDS: &[&str] = &[
    "amazon-bedrock",
    "cloudflare-ai-gateway",
    "github-copilot",
    "gitlab",
    "google-vertex",
    "google-vertex-anthropic",
    "kiro",
    "opencode",
    "opencode-go",
    "sap-ai-core",
    "v0",
    "vercel",
];

/// Field identifiers rendered in the provider configuration form.
/// 渲染在供应商配置表单中的字段标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderFieldKey {
    /// Shared API key or token input.
    /// 通用 API Key 或 Token 输入项。
    ApiKey,
    /// Optional manual base URL override input.
    /// 可选的手动基址覆盖输入项。
    BaseUrl,
    /// Azure resource name input.
    /// Azure 资源名称输入项。
    ResourceName,
    /// Cloudflare account identifier input.
    /// Cloudflare 账户标识输入项。
    AccountId,
}

impl ProviderFieldKey {
    /// Returns whether the input should be rendered as a secret field.
    /// 返回该输入是否应渲染为密文输入框。
    pub const fn is_secret(self) -> bool {
        matches!(self, Self::ApiKey)
    }

    /// Returns the stable user-facing field label.
    /// 返回稳定的面向用户字段标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "API Key",
            Self::BaseUrl => "Base URL",
            Self::ResourceName => "Resource Name",
            Self::AccountId => "Account ID",
        }
    }

    /// Returns the default placeholder text for the field.
    /// 返回该字段的默认占位提示文本。
    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::ApiKey => "请输入 API Key 或 Token",
            Self::BaseUrl => "https://api.example.com/v1",
            Self::ResourceName => "例如：my-models",
            Self::AccountId => "例如：1234567890abcdef1234567890abcdef",
        }
    }
}

/// Provider runtime family aligned with the current project and host-controlled integration model.
/// 与当前项目及宿主控制接入模型对齐的供应商运行时族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderRuntimeKind {
    /// OpenAI-compatible HTTP runtime.
    /// OpenAI 兼容 HTTP 运行时。
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
    /// Native OpenAI runtime.
    /// 原生 OpenAI 运行时。
    #[serde(rename = "openai")]
    OpenAi,
    /// Native Anthropic runtime.
    /// 原生 Anthropic 运行时。
    #[serde(rename = "anthropic")]
    Anthropic,
    /// Native Azure OpenAI runtime.
    /// 原生 Azure OpenAI 运行时。
    #[serde(rename = "azure-openai")]
    AzureOpenAi,
    /// Native Google Gemini runtime.
    /// 原生 Google Gemini 运行时。
    #[serde(rename = "google-gemini")]
    GoogleGemini,
    /// Native xAI runtime.
    /// 原生 xAI 运行时。
    #[serde(rename = "xai")]
    XAi,
    /// Native Mistral runtime.
    /// 原生 Mistral 运行时。
    #[serde(rename = "mistral")]
    Mistral,
    /// Native Groq runtime.
    /// 原生 Groq 运行时。
    #[serde(rename = "groq")]
    Groq,
    /// Native Cohere runtime.
    /// 原生 Cohere 运行时。
    #[serde(rename = "cohere")]
    Cohere,
    /// Native Together AI runtime.
    /// 原生 Together AI 运行时。
    #[serde(rename = "together-ai")]
    TogetherAi,
    /// Native Perplexity runtime.
    /// 原生 Perplexity 运行时。
    #[serde(rename = "perplexity")]
    Perplexity,
    /// Native OpenRouter runtime.
    /// 原生 OpenRouter 运行时。
    #[serde(rename = "openrouter")]
    OpenRouter,
}

impl ProviderRuntimeKind {
    /// Returns a concise runtime label for the configuration page.
    /// 返回配置页面使用的精简运行时标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-Compatible",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::AzureOpenAi => "Azure OpenAI",
            Self::GoogleGemini => "Google Gemini",
            Self::XAi => "xAI",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::Cohere => "Cohere",
            Self::TogetherAi => "Together AI",
            Self::Perplexity => "Perplexity",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

/// Unified reasoning-effort preset shared by the workbench, provider configuration, and request adaptation pipeline.
/// 由工作台、供应商配置与请求适配链路共享的统一推理档位预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningEffortLevel {
    /// Requests the closest available no-thinking mode supported by the target provider or model.
    /// 请求目标供应商或模型支持的最接近“关闭思考”的模式。
    Off,
    /// Requests a lightweight reasoning profile focused on faster responses.
    /// 请求偏向更快响应的轻量推理配置。
    Low,
    /// Requests a balanced reasoning profile.
    /// 请求平衡型推理配置。
    Medium,
    /// Requests a deeper reasoning profile.
    /// 请求更深层的推理配置。
    High,
    /// Requests the strongest reasoning profile exposed by the current workbench controls.
    /// 请求当前工作台控件暴露的最强推理配置。
    XHigh,
}

impl ReasoningEffortLevel {
    /// Shared static list of all reasoning-effort presets exposed by the workbench.
    /// 工作台暴露的全部推理档位预设静态列表。
    pub const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::XHigh];

    /// Returns whether the current preset explicitly requests more than the provider default behavior.
    /// 返回当前预设是否明确请求了高于供应商默认行为的思考模式。
    pub const fn requests_explicit_reasoning(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Returns the stable ranking used by capability fallback selection and nearest-level matching.
    /// 返回供能力回退选择与最近档位匹配使用的稳定排序值。
    pub const fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::XHigh => 4,
        }
    }

    /// Returns the default compact Chinese label used by tiered reasoning selectors and diagnostics.
    /// 返回供分档思考选择器与诊断使用的默认紧凑中文标签。
    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::Off => "关闭",
            Self::Low => "低",
            Self::Medium => "中",
            Self::High => "高",
            Self::XHigh => "超高",
        }
    }
}

impl fmt::Display for ReasoningEffortLevel {
    /// Renders the compact user-facing Chinese label for the current reasoning-effort preset.
    /// 渲染当前推理档位预设面向用户的紧凑中文标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.compact_label())
    }
}

/// One reasoning selector option rendered in the workbench according to the active model capability set.
/// 根据当前激活模型能力集合渲染在工作台中的单个思考选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningSelectionOption {
    /// Stable shared reasoning-effort value used by the runtime selection pipeline.
    /// 供运行时选择链路使用的稳定共享推理档位值。
    pub effort: ReasoningEffortLevel,
    /// Human-readable label shown in the compact selector UI.
    /// 展示在紧凑选择器界面中的人类可读标签。
    pub label: String,
}

impl fmt::Display for ReasoningSelectionOption {
    /// Renders the compact user-facing label for one reasoning selector option.
    /// 渲染单个思考选择器选项的紧凑面向用户标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Resolved reasoning selector view derived from one provider/model capability set and one remembered preferred effort.
/// 根据某个供应商/模型能力集合与一份已记忆偏好档位解析出的思考选择器视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningSelectionView {
    /// Dynamic selector options that should be visible for the active model.
    /// 当前激活模型应当可见的动态选择项集合。
    pub options: Vec<ReasoningSelectionOption>,
    /// Effective effort that should be shown as selected and sent to the runtime.
    /// 应当显示为已选中并发送给运行时的生效档位。
    pub selected_effort: ReasoningEffortLevel,
    /// Optional warning or downgrade note that explains why the preferred effort had to change.
    /// 用于解释偏好档位为何必须变化的可选警告或降级说明。
    pub note: Option<String>,
}

/// Structured capability family describing how one provider or model expects reasoning controls to be expressed.
/// 描述单个供应商或模型如何表达推理控制参数的结构化能力类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningCapabilityKind {
    /// Native multi-level reasoning controls.
    /// 原生多档位推理控制。
    Tiered,
    /// Binary on/off reasoning controls.
    /// 二值开关式推理控制。
    Binary,
    /// Binary reasoning controls that may also expose token-budget style tuning.
    /// 可能同时暴露 Token 预算调节的二值推理控制。
    BinaryWithBudget,
    /// Proxy-level tiered controls whose downstream effect may collapse into fewer effective levels.
    /// 代理层支持分档但下游实际效果可能塌缩的推理控制。
    ProxyTiered,
}

/// Structured JSON patch set that maps the shared reasoning-effort presets onto one provider- or model-specific request payload.
/// 将共享推理档位预设映射到某个供应商或模型专属请求载荷的结构化 JSON Patch 集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningProfileConfig {
    /// Optional capability-family descriptor used for diagnostics and future UI hints.
    /// 用于诊断和未来界面提示的可选能力类型描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ReasoningCapabilityKind>,
    /// Optional provider-specific payload used when the workbench explicitly requests reasoning-off behavior.
    /// 当工作台显式请求关闭思考时使用的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub off: Option<Value>,
    /// Optional provider-specific payload used for the low reasoning preset.
    /// 用于低档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<Value>,
    /// Optional provider-specific payload used for the medium reasoning preset.
    /// 用于中档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medium: Option<Value>,
    /// Optional provider-specific payload used for the high reasoning preset.
    /// 用于高档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<Value>,
    /// Optional provider-specific payload used for the strongest reasoning preset.
    /// 用于最高档推理预设的可选供应商专属载荷。
    #[serde(rename = "xHigh", skip_serializing_if = "Option::is_none")]
    pub x_high: Option<Value>,
}

impl ReasoningProfileConfig {
    /// Returns the provider- or model-specific JSON patch for one shared reasoning-effort preset.
    /// 返回某个共享推理档位预设对应的供应商或模型专属 JSON Patch。
    pub fn patch_for_effort(&self, effort: ReasoningEffortLevel) -> Option<Value> {
        if matches!(
            self.kind,
            Some(ReasoningCapabilityKind::Binary | ReasoningCapabilityKind::BinaryWithBudget)
        ) {
            return match effort {
                ReasoningEffortLevel::Off => self.off.clone(),
                _ => self.first_enabled_patch(),
            };
        }

        match effort {
            ReasoningEffortLevel::Off => self.off.clone(),
            ReasoningEffortLevel::Low => self.low.clone(),
            ReasoningEffortLevel::Medium => self.medium.clone(),
            ReasoningEffortLevel::High => self.high.clone(),
            ReasoningEffortLevel::XHigh => self.x_high.clone(),
        }
    }

    /// Returns the ordered visible effort list declared by this profile after taking binary collapsing rules into account.
    /// 在考虑二值折叠规则后，返回当前配置声明的有序可见档位列表。
    pub fn available_efforts(&self) -> Vec<ReasoningEffortLevel> {
        if matches!(
            self.kind,
            Some(ReasoningCapabilityKind::Binary | ReasoningCapabilityKind::BinaryWithBudget)
        ) {
            let mut efforts = Vec::new();
            if self.off.is_some() {
                efforts.push(ReasoningEffortLevel::Off);
            }
            if let Some(enabled_effort) = self.first_enabled_effort() {
                efforts.push(enabled_effort);
            }
            return efforts;
        }

        ReasoningEffortLevel::ALL
            .into_iter()
            .filter(|effort| self.patch_for_effort(*effort).is_some())
            .collect()
    }

    /// Returns the first available non-off patch used by binary-style profiles.
    /// 返回二值风格配置使用的第一条可用非关闭 Patch。
    fn first_enabled_patch(&self) -> Option<Value> {
        self.low
            .clone()
            .or_else(|| self.medium.clone())
            .or_else(|| self.high.clone())
            .or_else(|| self.x_high.clone())
    }

    /// Returns the first available non-off effort used by binary-style profiles.
    /// 返回二值风格配置使用的第一条可用非关闭档位。
    fn first_enabled_effort(&self) -> Option<ReasoningEffortLevel> {
        [
            (ReasoningEffortLevel::Low, self.low.as_ref()),
            (ReasoningEffortLevel::Medium, self.medium.as_ref()),
            (ReasoningEffortLevel::High, self.high.as_ref()),
            (ReasoningEffortLevel::XHigh, self.x_high.as_ref()),
        ]
        .into_iter()
        .find_map(|(effort, patch)| patch.is_some().then_some(effort))
    }
}

/// Base URL strategy used by one provider definition.
/// 单个供应商定义使用的基址策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderBaseUrlKind {
    /// The runtime resolves the endpoint internally.
    /// 由运行时内部解析接口地址。
    Native,
    /// A fixed default base URL is available.
    /// 存在固定的默认基址。
    Fixed,
    /// A base URL template is resolved from one form field.
    /// 基址模板需要从某个表单字段解析。
    Template,
}

/// Structured base URL configuration for one provider.
/// 单个供应商的结构化基址配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBaseUrlConfig {
    /// Base URL resolution kind.
    /// 基址解析方式。
    pub kind: ProviderBaseUrlKind,
    /// Fixed default base URL when the provider has one.
    /// 当供应商存在固定默认基址时使用的默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Template string used to resolve a provider-specific endpoint.
    /// 用于解析供应商特定接口地址的模板字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Field key used by the template strategy.
    /// 模板策略使用的字段键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<ProviderFieldKey>,
    /// Whether the project chooses to expose a manual override input for this provider.
    /// 当前项目是否选择为该供应商暴露手动覆盖输入项。
    pub allow_override: bool,
}

/// Structured model discovery strategy for one provider.
/// 单个供应商的结构化模型发现策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDiscoveryConfig {
    /// Discovery kind used by the provider page.
    /// 供应商页面使用的发现方式。
    pub kind: ProviderModelDiscoveryKind,
}

/// Supported model discovery kinds used by the current GUI implementation.
/// 当前 GUI 实现支持的模型发现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderModelDiscoveryKind {
    /// No remote discovery is attempted and the embedded catalog is used directly.
    /// 不尝试远端发现，直接使用内置目录。
    #[serde(rename = "none")]
    None,
    /// Uses a standard OpenAI-style `/models` endpoint.
    /// 使用标准 OpenAI 风格 `/models` 端点。
    #[serde(rename = "openai-models")]
    OpenAiModels,
}

impl ProviderModelDiscoveryKind {
    /// Returns whether the discovery strategy can actively refresh models from a remote endpoint.
    /// 返回该发现策略是否可以主动从远端端点刷新模型。
    pub const fn supports_remote_refresh(self) -> bool {
        matches!(self, Self::OpenAiModels)
    }
}

/// One selectable model item shown in the provider configuration page.
/// 展示在供应商配置页面中的单个可选模型项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    /// Stable model identifier persisted into the configuration file.
    /// 持久化到配置文件中的稳定模型标识。
    pub id: String,
    /// Human-readable model name rendered in the UI.
    /// 渲染在界面中的可读模型名称。
    pub name: String,
    /// Optional model lifecycle status.
    /// 可选的模型生命周期状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional model family label.
    /// 可选的模型家族标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional model release date string.
    /// 可选的模型发布日期字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Optional context window limit.
    /// 可选的上下文窗口上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input token limit.
    /// 可选的输入 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output token limit.
    /// 可选的输出 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning capability flag.
    /// 可选的推理能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional tool-calling capability flag.
    /// 可选的工具调用能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<bool>,
    /// Optional temperature support flag.
    /// 可选的温度参数支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    /// Optional attachment support flag.
    /// 可选的附件支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional interleaved reasoning descriptor.
    /// 可选的交错推理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<ModelInterleavedCapability>,
    /// Optional model-scoped reasoning-profile overrides that can replace provider defaults for this exact model.
    /// 可选的模型级推理档位覆盖配置，可为当前精确模型替换供应商默认策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional input modality list.
    /// 可选的输入模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modality list.
    /// 可选的输出模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
}

impl fmt::Display for ModelChoice {
    /// Renders a compact model label that keeps name and identifier inspectable.
    /// 渲染兼顾名称与标识可辨识度的紧凑模型标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name == self.id {
            formatter.write_str(&self.name)
        } else {
            write!(formatter, "{} ({})", self.name, self.id)
        }
    }
}

/// Structured interleaved reasoning capability for one model entry.
/// 单个模型条目的结构化交错推理能力描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelInterleavedCapability {
    /// Boolean flag style interleaved capability.
    /// 布尔标记风格的交错推理能力。
    Enabled(bool),
    /// Field-mapped interleaved capability.
    /// 基于字段映射的交错推理能力。
    Field(ModelInterleavedField),
}

/// Field descriptor used by structured interleaved reasoning capabilities.
/// 结构化交错推理能力使用的字段描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInterleavedField {
    /// Response field carrying interleaved reasoning payloads.
    /// 承载交错推理载荷的响应字段。
    pub field: String,
}

/// One supported provider definition loaded from the split resource catalog.
/// 从分离资源目录中载入的单个受支持供应商定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedProviderDefinition {
    /// Stable provider identifier used in config and runtime routing.
    /// 用于配置与运行时路由的稳定供应商标识。
    pub id: String,
    /// Display name shown to the user.
    /// 展示给用户的显示名称。
    pub name: String,
    /// Provider runtime family aligned with the project integration model.
    /// 与项目接入模型对齐的供应商运行时族。
    pub runtime: ProviderRuntimeKind,
    /// Explicit user-agent string used when the provider needs one fixed execution profile.
    /// 当供应商需要固定执行标识时使用的显式 User-Agent 字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Environment keys that may provide credentials for the provider.
    /// 可能为该供应商提供凭据的环境变量键集合。
    #[serde(default)]
    pub credential_env_keys: Vec<String>,
    /// Additional HTTP headers appended to real execution requests for the provider.
    /// 附加到该供应商真实执行请求中的额外 HTTP 头集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
    /// Additional JSON body parameters merged into provider requests by compatible runtimes.
    /// 由兼容运行时合并进供应商请求体中的附加 JSON Body 参数。
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra_body: serde_json::Map<String, serde_json::Value>,
    /// Optional provider-scoped reasoning profile used when the current runtime family needs non-standard or provider-tuned controls.
    /// 当当前运行时家族需要非标准或供应商特调控制时使用的可选供应商级推理档位配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Ordered field list exposed by the provider form.
    /// 供应商表单暴露的有序字段列表。
    #[serde(default)]
    pub fields: Vec<ProviderFieldKey>,
    /// Structured base URL resolution configuration.
    /// 结构化的基址解析配置。
    pub base_url: ProviderBaseUrlConfig,
    /// Structured model discovery configuration.
    /// 结构化的模型发现配置。
    pub model_discovery: ProviderModelDiscoveryConfig,
    /// Embedded fallback model catalog.
    /// 内置回退模型目录。
    #[serde(default)]
    pub models: Vec<ModelChoice>,
}

impl SupportedProviderDefinition {
    /// Returns the effective ordered field list used by the configuration form.
    /// 返回配置表单使用的生效有序字段列表。
    pub fn input_fields(&self) -> Vec<ProviderFieldKey> {
        let mut fields = self.fields.clone();

        if self.base_url.allow_override && !fields.contains(&ProviderFieldKey::BaseUrl) {
            fields.push(ProviderFieldKey::BaseUrl);
        }

        fields
    }

    /// Returns the placeholder text used by one provider-specific field.
    /// 返回某个供应商专属字段使用的占位提示文本。
    pub fn field_placeholder(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::BaseUrl => self
                .base_url
                .value
                .clone()
                .or_else(|| {
                    self.base_url
                        .template
                        .clone()
                        .map(|template| template.replace('{', "").replace('}', ""))
                })
                .unwrap_or_else(|| ProviderFieldKey::BaseUrl.placeholder().to_string()),
            _ => key.placeholder().to_string(),
        }
    }

    /// Returns the form label used by one provider-specific field.
    /// 返回某个供应商专属字段使用的表单标签。
    pub const fn field_label(&self, key: ProviderFieldKey) -> &'static str {
        key.label()
    }

    /// Returns whether the provider currently supports a remote model refresh path.
    /// 返回该供应商当前是否支持远端模型刷新路径。
    pub const fn supports_remote_model_refresh(&self) -> bool {
        self.model_discovery.kind.supports_remote_refresh()
    }

    /// Resolves the current base URL using the persisted options and provider strategy.
    /// 使用持久化选项与供应商策略解析当前基址。
    pub fn resolved_base_url(&self, options: &PersistedProviderOptions) -> Option<String> {
        if let Some(override_base_url) = options
            .base_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return Some(override_base_url.to_string());
        }

        match self.base_url.kind {
            ProviderBaseUrlKind::Native => self.base_url.value.clone(),
            ProviderBaseUrlKind::Fixed => self.base_url.value.clone(),
            ProviderBaseUrlKind::Template => {
                let template = self.base_url.template.as_ref()?;
                let parameter = self.base_url.parameter?;
                let parameter_value = options.get(parameter).trim().to_string();

                if parameter_value.is_empty() {
                    return None;
                }

                let token = format!("{{{}}}", template_parameter_name(parameter));
                Some(template.replace(&token, &parameter_value))
            }
        }
    }

    /// Resolves the dynamic reasoning selector view for the current provider/model pair and one remembered preferred effort.
    /// 为当前供应商/模型组合与一份已记忆偏好档位解析动态思考选择器视图。
    pub fn reasoning_selection_view(
        &self,
        model: Option<&ModelChoice>,
        preferred_effort: ReasoningEffortLevel,
    ) -> ReasoningSelectionView {
        let supported_efforts = self.supported_reasoning_efforts(model);
        let selected_effort = reconcile_reasoning_effort(preferred_effort, &supported_efforts);
        let options = build_reasoning_selection_options(&supported_efforts);
        let note =
            build_reasoning_selection_note(model, preferred_effort, selected_effort, &options);

        ReasoningSelectionView {
            options,
            selected_effort,
            note,
        }
    }

    /// Resolves the ordered reasoning-effort list supported by the current provider/model pair.
    /// 解析当前供应商/模型组合所支持的有序思考档位列表。
    pub fn supported_reasoning_efforts(
        &self,
        model: Option<&ModelChoice>,
    ) -> Vec<ReasoningEffortLevel> {
        let model_supports_reasoning = model.and_then(|item| item.reasoning).unwrap_or(false);
        if !model_supports_reasoning {
            return vec![ReasoningEffortLevel::Off];
        }

        if let Some(profile) = model
            .and_then(|item| item.reasoning_profile.as_ref())
            .or(self.reasoning_profile.as_ref())
        {
            let profile_efforts = profile.available_efforts();
            if !profile_efforts.is_empty() {
                return profile_efforts;
            }
        }

        infer_runtime_reasoning_efforts(self.runtime, self.id.as_str(), model)
    }
}

/// Infers one runtime-family default reasoning-effort list when no explicit provider or model profile is present.
/// 在不存在显式供应商或模型配置时推导一份运行时家族默认思考档位列表。
fn infer_runtime_reasoning_efforts(
    runtime: ProviderRuntimeKind,
    provider_id: &str,
    model: Option<&ModelChoice>,
) -> Vec<ReasoningEffortLevel> {
    let Some(model) = model else {
        return ReasoningEffortLevel::ALL.into_iter().collect();
    };

    let model_id = model.id.to_ascii_lowercase();

    match runtime {
        ProviderRuntimeKind::GoogleGemini => infer_google_gemini_reasoning_efforts(&model_id),
        ProviderRuntimeKind::Cohere => vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low],
        ProviderRuntimeKind::OpenAiCompatible if matches!(provider_id, "alibaba" | "bailing") => {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::OpenAi
        | ProviderRuntimeKind::Anthropic
        | ProviderRuntimeKind::AzureOpenAi
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity
        | ProviderRuntimeKind::OpenRouter => ReasoningEffortLevel::ALL.into_iter().collect(),
    }
}

/// Infers one Gemini-model-specific reasoning-effort list from the model identifier family when no explicit model profile is present.
/// 在不存在显式模型配置时，根据模型标识家族推导一份 Gemini 专属思考档位列表。
fn infer_google_gemini_reasoning_efforts(model_id: &str) -> Vec<ReasoningEffortLevel> {
    if model_id.starts_with("gemini-3") {
        return vec![ReasoningEffortLevel::Low, ReasoningEffortLevel::High];
    }

    if model_id.starts_with("gemini-2.5-pro") {
        return vec![
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::Medium,
            ReasoningEffortLevel::High,
        ];
    }

    if model_id.starts_with("gemini-2.5-") {
        return vec![
            ReasoningEffortLevel::Off,
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::Medium,
            ReasoningEffortLevel::High,
        ];
    }

    vec![
        ReasoningEffortLevel::Off,
        ReasoningEffortLevel::Low,
        ReasoningEffortLevel::Medium,
        ReasoningEffortLevel::High,
    ]
}

/// Builds the visible reasoning selector options from the ordered supported effort list.
/// 根据有序支持档位列表构建可见的思考选择器选项。
fn build_reasoning_selection_options(
    supported_efforts: &[ReasoningEffortLevel],
) -> Vec<ReasoningSelectionOption> {
    let uses_binary_labels = supported_efforts
        .iter()
        .filter(|effort| effort.requests_explicit_reasoning())
        .count()
        <= 1;

    supported_efforts
        .iter()
        .copied()
        .map(|effort| ReasoningSelectionOption {
            effort,
            label: if uses_binary_labels && effort.requests_explicit_reasoning() {
                String::from("开启")
            } else {
                effort.compact_label().to_string()
            },
        })
        .collect()
}

/// Reconciles one remembered preferred effort into the closest supported effort for the current model capability set.
/// 将一份已记忆偏好档位回退为当前模型能力集合中最近可用的生效档位。
fn reconcile_reasoning_effort(
    preferred_effort: ReasoningEffortLevel,
    supported_efforts: &[ReasoningEffortLevel],
) -> ReasoningEffortLevel {
    if supported_efforts.contains(&preferred_effort) {
        return preferred_effort;
    }

    let supported_non_off = supported_efforts
        .iter()
        .copied()
        .filter(|effort| effort.requests_explicit_reasoning())
        .collect::<Vec<_>>();

    if preferred_effort == ReasoningEffortLevel::Off {
        return supported_non_off
            .first()
            .copied()
            .or_else(|| supported_efforts.first().copied())
            .unwrap_or(ReasoningEffortLevel::Off);
    }

    if !supported_non_off.is_empty() {
        let preferred_rank = preferred_effort.rank();

        return supported_non_off
            .into_iter()
            .min_by_key(|effort| {
                (
                    effort.rank().abs_diff(preferred_rank),
                    effort.rank() > preferred_rank,
                    effort.rank(),
                )
            })
            .unwrap_or(ReasoningEffortLevel::Off);
    }

    supported_efforts
        .first()
        .copied()
        .unwrap_or(ReasoningEffortLevel::Off)
}

/// Builds one downgrade note when the remembered preferred effort is not supported by the newly selected model.
/// 当已记忆偏好档位不被新选模型支持时，构建一条降级提示说明。
fn build_reasoning_selection_note(
    model: Option<&ModelChoice>,
    preferred_effort: ReasoningEffortLevel,
    selected_effort: ReasoningEffortLevel,
    options: &[ReasoningSelectionOption],
) -> Option<String> {
    if preferred_effort == selected_effort {
        return None;
    }

    let model_label = model.map(|item| item.name.as_str()).unwrap_or("当前模型");
    let selected_label = options
        .iter()
        .find(|option| option.effort == selected_effort)
        .map(|option| option.label.as_str())
        .unwrap_or_else(|| selected_effort.compact_label());

    if preferred_effort == ReasoningEffortLevel::Off {
        return Some(format!(
            "当前模型 `{model_label}` 不支持关闭思考，最低可用档位为 `{selected_label}`，已自动切换。"
        ));
    }

    if !selected_effort.requests_explicit_reasoning() {
        return Some(format!(
            "当前模型 `{model_label}` 不支持思考能力，已自动切换到关闭思考。"
        ));
    }

    Some(format!(
        "当前模型 `{model_label}` 不支持 `{preferred_effort}` 思考档位，已自动切换到 `{selected_label}`。"
    ))
}

/// Provider selection option rendered in the provider pick list.
/// 渲染在供应商下拉选择框中的供应商选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderChoice {
    /// Stable provider identifier.
    /// 稳定的供应商标识。
    pub id: String,
    /// Display label shown in the picker.
    /// 展示在选择框中的显示标签。
    pub name: String,
}

impl fmt::Display for ProviderChoice {
    /// Renders a user-facing provider picker label.
    /// 渲染面向用户的供应商选择框标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// Persisted provider option values aligned with the Vulcan configuration file.
/// 与 Vulcan 配置文件对齐的持久化供应商选项值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedProviderOptions {
    /// Stored API key or token.
    /// 已保存的 API Key 或 Token。
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Stored custom base URL override when the project enables it.
    /// 当项目启用时保存的自定义基址覆盖值。
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Stored Azure resource name.
    /// 已保存的 Azure 资源名称。
    #[serde(rename = "resourceName", skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    /// Stored Cloudflare account identifier.
    /// 已保存的 Cloudflare 账户标识。
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

impl PersistedProviderOptions {
    /// Returns whether the options object is effectively empty.
    /// 返回该 options 对象是否实际上为空。
    pub fn is_empty(&self) -> bool {
        self.api_key.is_none()
            && self.base_url.is_none()
            && self.resource_name.is_none()
            && self.account_id.is_none()
    }

    /// Reads one field value using the normalized provider form key.
    /// 使用标准化的供应商表单字段键读取一个字段值。
    pub fn get(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::ApiKey => self.api_key.clone().unwrap_or_default(),
            ProviderFieldKey::BaseUrl => self.base_url.clone().unwrap_or_default(),
            ProviderFieldKey::ResourceName => self.resource_name.clone().unwrap_or_default(),
            ProviderFieldKey::AccountId => self.account_id.clone().unwrap_or_default(),
        }
    }

    /// Writes one field value using the normalized provider form key.
    /// 使用标准化的供应商表单字段键写入一个字段值。
    pub fn set(&mut self, key: ProviderFieldKey, value: String) {
        let trimmed = value.trim().to_string();
        let next = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };

        match key {
            ProviderFieldKey::ApiKey => self.api_key = next,
            ProviderFieldKey::BaseUrl => self.base_url = next,
            ProviderFieldKey::ResourceName => self.resource_name = next,
            ProviderFieldKey::AccountId => self.account_id = next,
        }
    }

    /// Resolves the stored API key into a runtime-ready credential string.
    /// 将已保存的 API Key 解析为运行时可直接使用的凭据字符串。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(self.api_key.as_deref(), "API Key")
    }
}

/// Persisted model metadata cached after a successful remote refresh.
/// 成功远端刷新后缓存下来的持久化模型元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedModelEntry {
    /// Human-readable model name.
    /// 人类可读的模型名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Persisted provider entry written into the Vulcan configuration file.
/// 写入 Vulcan 配置文件的持久化供应商条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedProviderEntry {
    /// Human-readable provider name.
    /// 人类可读的供应商名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Provider option object aligned with the current project schema.
    /// 与当前项目 Schema 对齐的供应商 options 对象。
    #[serde(default, skip_serializing_if = "PersistedProviderOptions::is_empty")]
    pub options: PersistedProviderOptions,
    /// Cached model list used when the provider supports remote refresh.
    /// 当供应商支持远端刷新时缓存下来的模型列表。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, PersistedModelEntry>,
}

/// Persisted skill family used by the runtime foundation.
/// 运行时基础架构使用的持久化技能类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedSkillKind {
    /// General built-in skill chain.
    /// 常规内建技能链。
    Regular,
    /// LuaSkills-backed capability entry.
    /// 基于 LuaSkills 的能力条目。
    LuaSkills,
    /// Automation-oriented skill entry.
    /// 面向自动化的技能条目。
    Automation,
    /// Memory-oriented skill entry.
    /// 面向记忆体系的技能条目。
    Memory,
}

/// One persisted skill entry declared inside the shared Vulcan configuration file.
/// 共享 Vulcan 配置文件中声明的单个持久化技能条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSkillEntry {
    /// Human-readable skill name.
    /// 人类可读的技能名称。
    pub name: String,
    /// Skill family used to route the definition into the right registry bucket.
    /// 用于将定义路由到正确注册表桶中的技能类型。
    pub kind: PersistedSkillKind,
    /// Optional skill description shown in configuration surfaces.
    /// 展示在配置界面中的可选技能描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the skill is currently enabled.
    /// 当前技能是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Signals that make the skill applicable.
    /// 使技能生效的信号集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trigger_signals: Vec<String>,
    /// Optional signal emitted by the skill after execution.
    /// 技能执行后可选地发出的信号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitted_signal: Option<String>,
    /// Confidence delta contributed by the skill.
    /// 技能贡献的置信度增量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_delta: Option<f32>,
}

/// Persisted agent family used by the compose runtime foundation.
/// compose 运行时基础架构使用的持久化代理类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedAgentKind {
    /// General worker agent driven by a skill chain.
    /// 由技能链驱动的常规执行代理。
    Worker,
    /// Specialist agent focused on one capability domain.
    /// 聚焦单一能力域的专家代理。
    Specialist,
    /// Coordinator agent that deterministically routes to specialists.
    /// 将任务确定性路由给专家代理的协调代理。
    Coordinator,
}

/// One deterministic routing rule used by a coordinator agent.
/// 协调代理使用的单条确定性路由规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAgentRoute {
    /// Signal name that activates the route.
    /// 激活该路由的信号名称。
    pub signal: String,
    /// Delegate agent identifier that should receive the routed investigation.
    /// 应接收该路由调查的委派代理标识。
    pub delegate: String,
}

/// One persisted tool-policy entry that scopes native and MCP tools.
/// 用于限定原生工具与 MCP 工具的单个持久化工具策略条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolPolicyEntry {
    /// Human-readable policy name.
    /// 人类可读的策略名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether every native tool should be exposed to the bound agent.
    /// 是否向绑定代理暴露全部原生工具。
    #[serde(default)]
    pub allow_all_native: bool,
    /// Explicitly allowed native tool identifiers.
    /// 显式允许的原生工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_tools: Vec<String>,
    /// Explicitly allowed MCP server identifiers.
    /// 显式允许的 MCP 服务器标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    /// Explicitly allowed MCP tool identifiers.
    /// 显式允许的 MCP 工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
}

/// One persisted agent entry stored in the shared Vulcan configuration file.
/// 存储在共享 Vulcan 配置文件中的单个持久化代理条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAgentEntry {
    /// Human-readable agent name.
    /// 人类可读的代理名称。
    pub name: String,
    /// High-level agent family.
    /// 高层代理类型。
    pub kind: PersistedAgentKind,
    /// Optional agent description shown in management surfaces.
    /// 展示在管理界面中的可选代理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional system prompt or working charter.
    /// 可选的系统提示词或工作章程。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Skill identifiers bound to the agent.
    /// 绑定到该代理的技能标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Native tool identifiers directly attached to the agent.
    /// 直接附着到该代理上的原生工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_tools: Vec<String>,
    /// MCP tool identifiers directly attached to the agent.
    /// 直接附着到该代理上的 MCP 工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
    /// Delegate agent identifiers that may be exposed as sub-agent tools later.
    /// 后续可作为子代理工具暴露的委派代理标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegates: Vec<String>,
    /// Optional shared tool-policy identifier referenced by the agent.
    /// 代理引用的可选共享工具策略标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<String>,
    /// Optional deterministic routing table used by coordinator agents.
    /// 协调代理使用的可选确定性路由表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<PersistedAgentRoute>,
    /// Optional fallback delegate used by coordinator agents.
    /// 协调代理使用的可选回退委派目标。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_delegate: Option<String>,
    /// Optional model override in `provider/model` form.
    /// 以 `provider/model` 形式声明的可选模型覆盖。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional temperature override for the agent.
    /// 该代理的可选温度参数覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Optional maximum step count or turn ceiling.
    /// 可选的最大步数或轮次数上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u16>,
}

/// Supported MCP transport families persisted inside the shared configuration file.
/// 持久化到共享配置文件中的受支持 MCP 传输类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedMcpTransportKind {
    /// Standard input/output child-process transport.
    /// 标准输入输出子进程传输。
    Stdio,
    /// Streamable HTTP client transport.
    /// Streamable HTTP 客户端传输。
    Http,
    /// Legacy server-sent-events MCP transport.
    /// 旧式基于 Server-Sent Events 的 MCP 传输。
    Sse,
}

impl PersistedMcpTransportKind {
    /// Returns the human-readable transport label used by UI surfaces.
    /// 返回 UI 界面使用的人类可读传输标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

impl fmt::Display for PersistedMcpTransportKind {
    /// Renders the transport kind into one compact UI label.
    /// 将传输类型渲染为紧凑 UI 标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One persisted MCP server entry stored in the shared configuration file.
/// 存储在共享配置文件中的单个持久化 MCP 服务器条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpServerEntry {
    /// Human-readable server name.
    /// 人类可读的服务器名称。
    pub name: String,
    /// Whether the server is enabled for runtime registration.
    /// 该服务器是否对运行时注册启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Transport family used to start the server.
    /// 用于启动该服务器的传输类型。
    pub transport: PersistedMcpTransportKind,
    /// Command used to launch the server process.
    /// 用于启动服务器进程的命令。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// Argument vector passed to the server process.
    /// 传递给服务器进程的参数列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Optional remote MCP endpoint URL used by HTTP transports.
    /// HTTP 传输使用的可选远端 MCP 端点 URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional bearer token injected into HTTP MCP requests.
    /// 注入到 HTTP MCP 请求中的可选 Bearer Token。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    /// Optional HTTP headers injected into remote MCP requests.
    /// 注入到远端 MCP 请求中的可选 HTTP 请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Optional working directory used for the server process.
    /// 服务器进程使用的可选工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional environment variables injected into the server process.
    /// 注入到服务器进程中的可选环境变量集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// MCP configuration block persisted inside the shared Vulcan configuration file.
/// 持久化在共享 Vulcan 配置文件中的 MCP 配置块。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpConfiguration {
    /// MCP server definitions keyed by stable identifier.
    /// 以稳定标识为键的 MCP 服务器定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, PersistedMcpServerEntry>,
}

impl PersistedMcpConfiguration {
    /// Returns whether the MCP configuration block is effectively empty.
    /// 返回 MCP 配置块是否实际上为空。
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// Supported controller process modes persisted in the shared configuration file.
/// 持久化到共享配置文件中的受支持控制器进程模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedControllerProcessMode {
    /// Service mode keeps the controller alive until it is stopped externally.
    /// 服务模式会让控制器保持存活，直到它被外部停止。
    Service,
    /// Managed mode lets the controller self-stop after idle windows.
    /// 托管模式允许控制器在空闲窗口后自行停止。
    #[default]
    Managed,
}

/// Persisted controller bootstrap configuration stored in the shared Vulcan file.
/// 存储在共享 Vulcan 文件中的持久化控制器引导配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedControllerConfiguration {
    /// Optional controller endpoint URL or local bind string.
    /// 可选的控制器端点 URL 或本地绑定字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Whether the host may auto-spawn the controller when unavailable.
    /// 当控制器不可用时宿主是否允许自动拉起它。
    #[serde(default = "default_true")]
    pub auto_spawn: bool,
    /// Optional explicit controller executable path.
    /// 可选的显式控制器可执行文件路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    /// Process mode used when the host auto-spawns the controller.
    /// 当宿主自动拉起控制器时使用的进程模式。
    #[serde(default)]
    pub process_mode: PersistedControllerProcessMode,
    /// Minimum uptime required before managed self-shutdown is allowed.
    /// 托管模式允许自停前必须满足的最小存活时长。
    #[serde(default = "default_controller_minimum_uptime_secs")]
    pub minimum_uptime_secs: u64,
    /// Idle timeout used by managed self-shutdown.
    /// 托管模式自停使用的空闲超时时长。
    #[serde(default = "default_controller_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// Default client lease TTL used during registration.
    /// 注册客户端时使用的默认租约 TTL。
    #[serde(default = "default_controller_default_lease_ttl_secs")]
    pub default_lease_ttl_secs: u64,
    /// Per-attempt connection timeout used by the SDK.
    /// SDK 使用的单次连接超时时长。
    #[serde(default = "default_controller_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// Total startup wait timeout used after auto-spawn.
    /// 自动拉起后使用的总体启动等待超时时长。
    #[serde(default = "default_controller_startup_timeout_secs")]
    pub startup_timeout_secs: u64,
    /// Polling interval used while waiting for the spawned controller to become ready.
    /// 等待拉起控制器就绪时使用的轮询间隔。
    #[serde(default = "default_controller_startup_retry_interval_ms")]
    pub startup_retry_interval_ms: u64,
    /// Background lease renewal interval used by the SDK.
    /// SDK 使用的后台租约续约间隔。
    #[serde(default = "default_controller_lease_renew_interval_secs")]
    pub lease_renew_interval_secs: u64,
}

impl Default for PersistedControllerConfiguration {
    /// Builds the default persisted controller configuration aligned with the official SDK defaults.
    /// 构建与官方 SDK 默认值对齐的默认持久化控制器配置。
    fn default() -> Self {
        Self {
            endpoint: None,
            auto_spawn: true,
            executable_path: None,
            process_mode: PersistedControllerProcessMode::Managed,
            minimum_uptime_secs: default_controller_minimum_uptime_secs(),
            idle_timeout_secs: default_controller_idle_timeout_secs(),
            default_lease_ttl_secs: default_controller_default_lease_ttl_secs(),
            connect_timeout_secs: default_controller_connect_timeout_secs(),
            startup_timeout_secs: default_controller_startup_timeout_secs(),
            startup_retry_interval_ms: default_controller_startup_retry_interval_ms(),
            lease_renew_interval_secs: default_controller_lease_renew_interval_secs(),
        }
    }
}

impl PersistedControllerConfiguration {
    /// Returns whether the persisted controller configuration still equals the built-in defaults.
    /// 返回持久化控制器配置是否仍然等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Root provider configuration file saved in the Vulcan home directory.
/// 保存到 Vulcan 家目录中的根级供应商配置文件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfigurationFile {
    /// Currently selected global model in `provider/model` form.
    /// 当前以 `provider/model` 形式保存的全局激活模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Remembered workbench reasoning preference selected by the user before any model-specific capability downgrade is applied.
    /// 在应用任意模型级能力回退前，由用户选定并记忆的工作台思考偏好。
    #[serde(
        rename = "workbenchReasoningEffort",
        skip_serializing_if = "Option::is_none"
    )]
    pub workbench_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Persisted provider map keyed by provider identifier.
    /// 以供应商标识为键保存的供应商映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider: BTreeMap<String, PersistedProviderEntry>,
    /// Legacy active-agent identifier retained only for one-time migration into the dedicated agent profile store.
    /// 仅为一次性迁移到独立 Agent 配置存储而保留的旧版激活代理标识。
    #[serde(rename = "active_agent", default, skip_serializing)]
    pub legacy_active_agent: Option<String>,
    /// Legacy persisted multi-agent definitions retained only for one-time migration into the dedicated agent profile store.
    /// 仅为一次性迁移到独立 Agent 配置存储而保留的旧版多代理定义集合。
    #[serde(rename = "agents", default, skip_serializing)]
    pub legacy_agents: BTreeMap<String, PersistedAgentEntry>,
    /// Persisted skill definitions keyed by stable skill id.
    /// 以稳定技能标识为键保存的技能定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, PersistedSkillEntry>,
    /// Persisted tool policy definitions keyed by stable policy id.
    /// 以稳定策略标识为键保存的工具策略定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_policies: BTreeMap<String, PersistedToolPolicyEntry>,
    /// Persisted MCP server configuration block.
    /// 持久化的 MCP 服务器配置块。
    #[serde(default, skip_serializing_if = "PersistedMcpConfiguration::is_empty")]
    pub mcp: PersistedMcpConfiguration,
    /// Persisted controller bootstrap configuration.
    /// 持久化的控制器引导配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedControllerConfiguration::is_default"
    )]
    pub controller: PersistedControllerConfiguration,
}

/// Loads the supported provider catalog derived from the split provider resources.
/// 加载从分离供应商资源目录派生出来的受支持供应商目录。
pub fn supported_providers() -> &'static [SupportedProviderDefinition] {
    static PROVIDERS: OnceLock<Vec<SupportedProviderDefinition>> = OnceLock::new();

    PROVIDERS.get_or_init(load_supported_providers)
}

/// Returns the persisted Vulcan configuration file path.
/// 返回持久化 Vulcan 配置文件路径。
pub fn configuration_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });

    home.join(".vulcan").join("vulcan-code.json")
}

/// Loads the shared controller bootstrap configuration from the runtime configuration directory.
/// 从运行时配置目录加载共享控制器引导配置。
pub fn load_controller_configuration_file() -> Result<PersistedControllerConfiguration, String> {
    let path = runtime_assets::resolve_controller_config_file_path();

    if !path.exists() {
        return Ok(PersistedControllerConfiguration::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取控制器配置文件失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析控制器配置文件失败：{}。", error))
}

/// Loads the persisted provider configuration file from disk.
/// 从磁盘加载持久化的供应商配置文件。
pub fn load_configuration_file() -> Result<ProviderConfigurationFile, String> {
    let path = configuration_file_path();

    if !path.exists() {
        return Ok(ProviderConfigurationFile::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取供应商配置文件失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析供应商配置文件失败：{}。", error))
}

/// Saves the provider configuration file back to disk, creating parent directories when needed.
/// 将供应商配置文件写回磁盘，并在需要时创建父目录。
pub fn save_configuration_file(config: &ProviderConfigurationFile) -> Result<(), String> {
    let path = configuration_file_path();
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&parent).map_err(|error| format!("创建配置目录失败：{}。", error))?;

    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("序列化供应商配置失败：{}。", error))?;

    fs::write(&path, content).map_err(|error| format!("写入供应商配置失败：{}。", error))
}

/// Resolves the supported provider definition by identifier.
/// 根据标识解析受支持的供应商定义。
pub fn supported_provider_by_id(provider_id: &str) -> Option<&'static SupportedProviderDefinition> {
    supported_providers()
        .iter()
        .find(|definition| definition.id == provider_id)
}

/// Builds picker choices from the supported provider catalog.
/// 从受支持供应商目录构建选择器选项。
pub fn supported_provider_choices() -> Vec<ProviderChoice> {
    supported_providers()
        .iter()
        .map(|definition| ProviderChoice {
            id: definition.id.clone(),
            name: definition.name.clone(),
        })
        .collect()
}

/// Resolves the effective model choices for one provider using persisted cache or embedded fallback models.
/// 使用持久化缓存或内置回退模型为某个供应商解析生效模型列表。
pub fn effective_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let embedded_models = supported_provider_by_id(provider_id)
        .map(|definition| definition.models.clone())
        .unwrap_or_default();

    if let Some(cached) = config.provider.get(provider_id) {
        if !cached.models.is_empty() {
            let items = cached
                .models
                .iter()
                .map(|(id, model)| ModelChoice {
                    id: id.clone(),
                    name: model.name.clone().unwrap_or_else(|| id.clone()),
                    status: None,
                    family: None,
                    release_date: None,
                    context_limit: None,
                    input_limit: None,
                    output_limit: None,
                    reasoning: None,
                    tool_call: None,
                    temperature: None,
                    attachment: None,
                    interleaved: None,
                    reasoning_profile: None,
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                })
                .collect::<Vec<_>>();
            let mut items = merge_embedded_model_metadata(items, &embedded_models);
            items.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
            return items;
        }
    }

    embedded_models
}

/// Parses the active provider/model tuple from the persisted global model string.
/// 从持久化的全局模型字符串中解析激活的供应商与模型二元组。
pub fn parse_active_model_reference(model: &str) -> Option<(&str, &str)> {
    let (provider_id, model_id) = model.split_once('/')?;
    Some((provider_id, model_id))
}

/// Fetches a remote model list using the provider definition and the current persisted options.
/// 使用供应商定义与当前持久化选项拉取远端模型列表。
pub async fn fetch_remote_models(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Result<Vec<ModelChoice>, String> {
    let base_url = remote_models_url(definition, options)
        .ok_or_else(|| String::from("当前供应商不支持远端模型刷新。"))?;

    let mut headers = HeaderMap::new();

    if let Some(api_key) = options.resolved_api_key()? {
        let header_value = HeaderValue::from_str(&format!("Bearer {}", api_key))
            .map_err(|error| format!("构建认证请求头失败：{}。", error))?;
        headers.insert(AUTHORIZATION, header_value);
    }

    let response = reqwest::Client::new()
        .get(base_url)
        .headers(headers)
        .send()
        .await
        .map_err(|error| format!("请求模型列表失败：{}。", error))?;

    if !response.status().is_success() {
        return Err(format!("模型列表接口返回状态异常：{}。", response.status()));
    }

    let payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| format!("解析模型列表响应失败：{}。", error))?;

    let choices = extract_model_choices_from_value(&payload);

    if choices.is_empty() {
        return Err(String::from("未从远端接口解析到任何模型。"));
    }

    Ok(choices)
}

/// Converts a successful remote model refresh into a persisted provider cache map.
/// 将一次成功的远端模型刷新结果转换为可持久化的供应商缓存映射。
pub fn persist_remote_models(models: &[ModelChoice]) -> BTreeMap<String, PersistedModelEntry> {
    models
        .iter()
        .map(|model| {
            (
                model.id.clone(),
                PersistedModelEntry {
                    name: Some(model.name.clone()),
                },
            )
        })
        .collect()
}

/// Returns a human-friendly provider summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读供应商摘要。
pub fn provider_summary(provider_id: &str) -> String {
    supported_provider_by_id(provider_id)
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| provider_id.to_string())
}

/// Returns a human-friendly model summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读模型摘要。
pub fn model_summary(
    provider_id: &str,
    model_id: &str,
    config: &ProviderConfigurationFile,
) -> String {
    effective_model_choices(provider_id, config)
        .into_iter()
        .find(|model| model.id == model_id)
        .map(|model| model.to_string())
        .unwrap_or_else(|| model_id.to_string())
}

/// Loads and sorts the split provider catalog entries from the resources directory.
/// 从资源目录中加载并排序分离的供应商目录条目。
fn load_supported_providers() -> Vec<SupportedProviderDefinition> {
    let directory = provider_catalog_directory_path();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "provider catalog resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            let definition = serde_json::from_str::<SupportedProviderDefinition>(&content)
                .unwrap_or_else(|error| {
                    panic!(
                        "provider catalog resource `{}` must stay valid: {error}",
                        path.display()
                    )
                });

            if UNSUPPORTED_PROVIDER_IDS
                .iter()
                .any(|unsupported| *unsupported == definition.id)
            {
                None
            } else {
                Some(definition)
            }
        })
        .collect::<Vec<_>>();

    definitions.sort_by(|left, right| provider_sort_key(left).cmp(&provider_sort_key(right)));
    definitions
}

/// Returns the directory path containing split provider catalog files.
/// 返回包含分离供应商目录文件的目录路径。
fn provider_catalog_directory_path() -> PathBuf {
    runtime_assets::resolve_provider_catalog_directory()
}

/// Returns a sortable provider key that keeps common providers near the top of the picker.
/// 返回可排序的供应商键值，以便将常用供应商放到选择器顶部。
fn provider_sort_key(provider: &SupportedProviderDefinition) -> (u8, String) {
    let priority = match provider.id.as_str() {
        "openai" => 0,
        "anthropic" => 1,
        "deepseek" => 2,
        "openrouter" => 3,
        "google" => 4,
        "azure" => 5,
        _ => 99,
    };

    (priority, provider.name.clone())
}

/// Returns the token name used inside one base URL template.
/// 返回单个基址模板内部使用的参数占位符名称。
fn template_parameter_name(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "apiKey",
        ProviderFieldKey::BaseUrl => "baseUrl",
        ProviderFieldKey::ResourceName => "resourceName",
        ProviderFieldKey::AccountId => "accountId",
    }
}

/// Builds the remote `/models` endpoint URL when the provider can try a standard refresh flow.
/// 当供应商可以尝试标准刷新流程时，构建远端 `/models` 端点地址。
fn remote_models_url(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Option<String> {
    if !definition.supports_remote_model_refresh() {
        return None;
    }

    let base = definition.resolved_base_url(options)?;
    let normalized = base
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .trim_end_matches("/responses")
        .trim_end_matches("/completions")
        .trim_end_matches("/messages")
        .to_string();

    Some(format!("{}/models", normalized.trim_end_matches('/')))
}

/// Extracts model choices from a loosely shaped OpenAI-style `/models` response payload.
/// 从宽松形状的 OpenAI 风格 `/models` 响应载荷中提取模型项。
fn extract_model_choices_from_value(value: &serde_json::Value) -> Vec<ModelChoice> {
    let mut choices = match value {
        serde_json::Value::Object(object) => object
            .get("data")
            .and_then(serde_json::Value::as_array)
            .map(|items| extract_models_from_array(items))
            .unwrap_or_default(),
        serde_json::Value::Array(array) => extract_models_from_array(array),
        _ => Vec::new(),
    };

    choices.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    choices.dedup_by(|left, right| left.id == right.id);
    choices
}

/// Resolves a secret field that may reference one environment variable placeholder.
/// 解析一个可能引用环境变量占位符的敏感字段。
fn resolve_env_backed_secret(
    raw_value: Option<&str>,
    field_label: &str,
) -> Result<Option<String>, String> {
    let Some(trimmed_value) = raw_value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let Some(env_name) = parse_env_placeholder(trimmed_value) else {
        if looks_like_env_placeholder(trimmed_value) {
            return Err(format!(
                "{field_label} 的环境变量占位符格式无效，请使用 `{{env:NAME}}`。"
            ));
        }

        return Ok(Some(trimmed_value.to_string()));
    };

    let env_value = std::env::var(env_name).map_err(|_| {
        format!("{field_label} 引用了环境变量 `{env_name}`，但当前进程中未找到该变量。")
    })?;
    let resolved_value = env_value.trim();

    if resolved_value.is_empty() {
        return Err(format!(
            "{field_label} 引用了环境变量 `{env_name}`，但该变量当前为空。"
        ));
    }

    Ok(Some(resolved_value.to_string()))
}

/// Parses one `{env:NAME}` placeholder into the referenced environment variable name.
/// 将一个 `{env:NAME}` 占位符解析为引用的环境变量名称。
fn parse_env_placeholder(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{env:")?.strip_suffix('}')?.trim();

    if inner.is_empty() { None } else { Some(inner) }
}

/// Returns whether one value looks like an environment placeholder envelope.
/// 返回某个值是否看起来像环境变量占位符外壳。
fn looks_like_env_placeholder(value: &str) -> bool {
    value.starts_with("{env:") && value.ends_with('}')
}

/// Returns the default boolean value used by enabled flags in persisted runtime sections.
/// 返回持久化运行时分区启用标记使用的默认布尔值。
fn default_true() -> bool {
    true
}

/// Returns the default minimum uptime used by the shared controller configuration.
/// 返回共享控制器配置使用的默认最小存活时长。
fn default_controller_minimum_uptime_secs() -> u64 {
    300
}

/// Returns the default idle timeout used by the shared controller configuration.
/// 返回共享控制器配置使用的默认空闲超时时长。
fn default_controller_idle_timeout_secs() -> u64 {
    900
}

/// Returns the default lease TTL used by the shared controller configuration.
/// 返回共享控制器配置使用的默认租约 TTL。
fn default_controller_default_lease_ttl_secs() -> u64 {
    120
}

/// Returns the default connection timeout used by the shared controller configuration.
/// 返回共享控制器配置使用的默认连接超时时长。
fn default_controller_connect_timeout_secs() -> u64 {
    5
}

/// Returns the default startup timeout used by the shared controller configuration.
/// 返回共享控制器配置使用的默认启动超时时长。
fn default_controller_startup_timeout_secs() -> u64 {
    15
}

/// Returns the default startup retry interval used by the shared controller configuration.
/// 返回共享控制器配置使用的默认启动重试间隔。
fn default_controller_startup_retry_interval_ms() -> u64 {
    250
}

/// Returns the default lease renewal interval used by the shared controller configuration.
/// 返回共享控制器配置使用的默认租约续约间隔。
fn default_controller_lease_renew_interval_secs() -> u64 {
    30
}

/// Extracts model choices from one response array node.
/// 从单个响应数组节点中提取模型项。
fn extract_models_from_array(array: &[serde_json::Value]) -> Vec<ModelChoice> {
    array
        .iter()
        .filter_map(|item| {
            if let Some(id) = item.as_str() {
                return Some(ModelChoice {
                    id: id.to_string(),
                    name: id.to_string(),
                    status: None,
                    family: None,
                    release_date: None,
                    context_limit: None,
                    input_limit: None,
                    output_limit: None,
                    reasoning: None,
                    tool_call: None,
                    temperature: None,
                    attachment: None,
                    interleaved: None,
                    reasoning_profile: None,
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                });
            }

            let object = item.as_object()?;
            let id = object.get("id")?.as_str()?.to_string();
            let name = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();

            Some(ModelChoice {
                id,
                name,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                input_modalities: Vec::new(),
                output_modalities: Vec::new(),
            })
        })
        .collect()
}

/// Merges embedded model metadata back into dynamically discovered model entries by stable model id.
/// 依据稳定模型标识将内置模型元数据重新合并到动态发现的模型条目中。
fn merge_embedded_model_metadata(
    discovered_models: Vec<ModelChoice>,
    embedded_models: &[ModelChoice],
) -> Vec<ModelChoice> {
    let embedded_lookup = embedded_models
        .iter()
        .map(|model| (model.id.as_str(), model))
        .collect::<BTreeMap<_, _>>();

    discovered_models
        .into_iter()
        .map(|model| {
            let Some(embedded) = embedded_lookup.get(model.id.as_str()) else {
                return model;
            };

            ModelChoice {
                id: model.id,
                name: model.name,
                status: embedded.status.clone(),
                family: embedded.family.clone(),
                release_date: embedded.release_date.clone(),
                context_limit: embedded.context_limit,
                input_limit: embedded.input_limit,
                output_limit: embedded.output_limit,
                reasoning: embedded.reasoning,
                tool_call: embedded.tool_call,
                temperature: embedded.temperature,
                attachment: embedded.attachment,
                interleaved: embedded.interleaved.clone(),
                reasoning_profile: embedded.reasoning_profile.clone(),
                input_modalities: embedded.input_modalities.clone(),
                output_modalities: embedded.output_modalities.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ModelChoice, ProviderBaseUrlConfig, ProviderBaseUrlKind, ProviderConfigurationFile,
        ProviderModelDiscoveryConfig, ProviderModelDiscoveryKind, ProviderRuntimeKind,
        ReasoningCapabilityKind, ReasoningEffortLevel, ReasoningProfileConfig,
        SupportedProviderDefinition,
    };
    use serde_json::{Map as JsonMap, json};

    /// Builds one minimal supported-provider definition used by the reasoning-selection tests.
    /// 构建供思考选择测试复用的最小受支持 Provider 定义。
    fn test_provider_definition(
        runtime: ProviderRuntimeKind,
        provider_id: &str,
        reasoning_profile: Option<ReasoningProfileConfig>,
    ) -> SupportedProviderDefinition {
        SupportedProviderDefinition {
            id: provider_id.to_string(),
            name: provider_id.to_string(),
            runtime,
            user_agent: None,
            credential_env_keys: Vec::new(),
            extra_headers: Default::default(),
            extra_body: JsonMap::new(),
            reasoning_profile,
            fields: Vec::new(),
            base_url: ProviderBaseUrlConfig {
                kind: ProviderBaseUrlKind::Native,
                value: None,
                template: None,
                parameter: None,
                allow_override: false,
            },
            model_discovery: ProviderModelDiscoveryConfig {
                kind: ProviderModelDiscoveryKind::None,
            },
            models: Vec::new(),
        }
    }

    /// Builds one minimal model choice used by the reasoning-selection tests.
    /// 构建供思考选择测试复用的最小模型选项。
    fn test_model_choice(
        model_id: &str,
        supports_reasoning: bool,
        reasoning_profile: Option<ReasoningProfileConfig>,
    ) -> ModelChoice {
        ModelChoice {
            id: model_id.to_string(),
            name: model_id.to_string(),
            status: None,
            family: None,
            release_date: None,
            context_limit: None,
            input_limit: None,
            output_limit: None,
            reasoning: Some(supports_reasoning),
            tool_call: Some(true),
            temperature: Some(true),
            attachment: None,
            interleaved: None,
            reasoning_profile,
            input_modalities: Vec::new(),
            output_modalities: Vec::new(),
        }
    }

    /// Verifies binary reasoning profiles collapse the selector into the expected on/off pair and remap higher remembered tiers to “开启”.
    /// 验证二值思考配置会把选择器收缩为预期的开关二项，并将更高的已记忆档位回退为“开启”。
    #[test]
    fn binary_reasoning_profile_builds_on_off_selector_and_downgrades_high_to_enabled() {
        let provider = test_provider_definition(
            ProviderRuntimeKind::OpenAiCompatible,
            "bailing",
            Some(ReasoningProfileConfig {
                kind: Some(ReasoningCapabilityKind::Binary),
                off: Some(json!({ "enable_thinking": false })),
                low: Some(json!({ "enable_thinking": true })),
                medium: Some(json!({ "enable_thinking": true })),
                ..ReasoningProfileConfig::default()
            }),
        );
        let model = test_model_choice("qwen-coder", true, None);

        let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

        assert_eq!(view.options.len(), 2);
        assert_eq!(view.options[0].label, "关闭");
        assert_eq!(view.options[1].label, "开启");
        assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
        assert_eq!(
            view.note.as_deref(),
            Some("当前模型 `qwen-coder` 不支持 `高` 思考档位，已自动切换到 `开启`。")
        );
    }

    /// Verifies a model that does not support reasoning forces the selector into a disabled off-only state and reports the downgrade.
    /// 验证不支持思考的模型会强制将选择器收缩为仅关闭状态，并报告回退说明。
    #[test]
    fn non_reasoning_model_forces_selector_to_off_only() {
        let provider = test_provider_definition(ProviderRuntimeKind::OpenAi, "openai", None);
        let model = test_model_choice("gpt-lite", false, None);

        let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::Medium);

        assert_eq!(view.options.len(), 1);
        assert_eq!(view.options[0].label, "关闭");
        assert_eq!(view.selected_effort, ReasoningEffortLevel::Off);
        assert_eq!(
            view.note.as_deref(),
            Some("当前模型 `gpt-lite` 不支持思考能力，已自动切换到关闭思考。")
        );
    }

    /// Verifies Gemini 3 model families expose only the runtime-supported low/high compatibility tiers by default.
    /// 验证 Gemini 3 模型家族默认仅暴露运行时真正支持的低/高兼容档位。
    #[test]
    fn gemini_three_defaults_only_expose_low_and_high() {
        let provider = test_provider_definition(ProviderRuntimeKind::GoogleGemini, "google", None);
        let model = test_model_choice("gemini-3-pro", true, None);

        let supported = provider.supported_reasoning_efforts(Some(&model));

        assert_eq!(
            supported,
            vec![ReasoningEffortLevel::Low, ReasoningEffortLevel::High]
        );
    }

    /// Verifies the remembered workbench reasoning preference keeps the stable camelCase field name and round-trips through the shared provider configuration file.
    /// 验证已记忆的工作台思考偏好会保持稳定的 camelCase 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
    #[test]
    fn provider_configuration_round_trips_workbench_reasoning_effort() {
        let config = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::High),
            ..ProviderConfigurationFile::default()
        };

        let value = serde_json::to_value(&config).expect("provider configuration must serialize");
        assert_eq!(value.get("workbenchReasoningEffort"), Some(&json!("high")));

        let restored: ProviderConfigurationFile =
            serde_json::from_value(value).expect("provider configuration must deserialize");
        assert_eq!(
            restored.workbench_reasoning_effort,
            Some(ReasoningEffortLevel::High)
        );
    }
}

//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use vulcan_vmm::VmmConfigLayer;

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
    /// OpenAI Responses-compatible HTTP runtime.
    /// OpenAI Responses 兼容 HTTP 运行时。
    #[serde(rename = "openai-responses-compatible")]
    OpenAiResponsesCompatible,
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

/// Canonical provider API family derived from one runtime kind and used to group truly shared protocol behavior.
/// 从运行时类型派生出的规范 Provider 接口族，用于归并真正共享的协议行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderApiFamily {
    /// OpenAI Responses API family.
    /// OpenAI Responses 接口族。
    OpenAiResponses,
    /// OpenAI Chat Completions compatible API family.
    /// OpenAI Chat Completions 兼容接口族。
    OpenAiChatCompatible,
    /// Anthropic Messages API family.
    /// Anthropic Messages 接口族。
    AnthropicMessages,
    /// Google Gemini GenerateContent API family.
    /// Google Gemini GenerateContent 接口族。
    GoogleGeminiGenerateContent,
    /// Cohere Chat API family.
    /// Cohere Chat 接口族。
    CohereChat,
    /// Reserved AWS Bedrock Converse API family.
    /// 预留的 AWS Bedrock Converse 接口族。
    BedrockConverse,
}

impl ProviderApiFamily {
    /// Returns one stable API-family key used by diagnostics, previews, and future protocol factories.
    /// 返回供诊断、预览与后续协议工厂复用的稳定接口族键值。
    pub const fn key(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompatible => "openai-chat-compatible",
            Self::AnthropicMessages => "anthropic-messages",
            Self::GoogleGeminiGenerateContent => "google-gemini-generate-content",
            Self::CohereChat => "cohere-chat",
            Self::BedrockConverse => "bedrock-converse",
        }
    }

    /// Returns one human-readable API-family label used by diagnostics and settings surfaces.
    /// 返回供诊断面板与设置界面展示的人类可读接口族标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "OpenAI Responses",
            Self::OpenAiChatCompatible => "OpenAI Chat Compatible",
            Self::AnthropicMessages => "Anthropic Messages",
            Self::GoogleGeminiGenerateContent => "Google Gemini GenerateContent",
            Self::CohereChat => "Cohere Chat",
            Self::BedrockConverse => "AWS Bedrock Converse",
        }
    }
}

impl ProviderRuntimeKind {
    /// Returns a concise runtime label for the configuration page.
    /// 返回配置页面使用的精简运行时标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-Compatible",
            Self::OpenAi => "OpenAI",
            Self::OpenAiResponsesCompatible => "OpenAI Responses-Compatible",
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

    /// Returns the canonical provider API family behind the current runtime kind.
    /// 返回当前运行时类型背后的规范 Provider 接口族。
    pub const fn api_family(self) -> ProviderApiFamily {
        match self {
            Self::OpenAi | Self::OpenAiResponsesCompatible => ProviderApiFamily::OpenAiResponses,
            Self::OpenAiCompatible
            | Self::AzureOpenAi
            | Self::XAi
            | Self::Mistral
            | Self::Groq
            | Self::TogetherAi
            | Self::Perplexity
            | Self::OpenRouter => ProviderApiFamily::OpenAiChatCompatible,
            Self::Anthropic => ProviderApiFamily::AnthropicMessages,
            Self::GoogleGemini => ProviderApiFamily::GoogleGeminiGenerateContent,
            Self::Cohere => ProviderApiFamily::CohereChat,
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
    /// Uses the provider/model default reasoning behavior without exposing any manual selector.
    /// 使用供应商或模型默认思考行为，不暴露任何手动选择器。
    DefaultOnly,
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

mod ai_tools;

pub use ai_tools::*;
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
    /// Optional user-facing label override used by the disabled/default reasoning option.
    /// 用于关闭或默认思考选项的可选界面标签覆盖。
    #[serde(rename = "offLabel", skip_serializing_if = "Option::is_none")]
    pub off_label: Option<String>,
    /// Optional user-facing label override used by the low reasoning option.
    /// 用于低档思考选项的可选界面标签覆盖。
    #[serde(rename = "lowLabel", skip_serializing_if = "Option::is_none")]
    pub low_label: Option<String>,
    /// Optional user-facing label override used by the medium reasoning option.
    /// 用于中档思考选项的可选界面标签覆盖。
    #[serde(rename = "mediumLabel", skip_serializing_if = "Option::is_none")]
    pub medium_label: Option<String>,
    /// Optional user-facing label override used by the high reasoning option.
    /// 用于高档思考选项的可选界面标签覆盖。
    #[serde(rename = "highLabel", skip_serializing_if = "Option::is_none")]
    pub high_label: Option<String>,
    /// Optional user-facing label override used by the strongest reasoning option.
    /// 用于最高档思考选项的可选界面标签覆盖。
    #[serde(rename = "xHighLabel", skip_serializing_if = "Option::is_none")]
    pub x_high_label: Option<String>,
}

impl ReasoningProfileConfig {
    /// Returns the provider- or model-specific JSON patch for one shared reasoning-effort preset.
    /// 返回某个共享推理档位预设对应的供应商或模型专属 JSON Patch。
    pub fn patch_for_effort(&self, effort: ReasoningEffortLevel) -> Option<Value> {
        if matches!(self.kind, Some(ReasoningCapabilityKind::DefaultOnly)) {
            return None;
        }

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
        if matches!(self.kind, Some(ReasoningCapabilityKind::DefaultOnly)) {
            return vec![ReasoningEffortLevel::Off];
        }

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

    /// Returns one optional UI label override for the current shared reasoning effort.
    /// 返回当前共享思考档位对应的可选界面标签覆盖。
    pub fn label_for_effort(&self, effort: ReasoningEffortLevel) -> Option<&str> {
        match effort {
            ReasoningEffortLevel::Off => self.off_label.as_deref(),
            ReasoningEffortLevel::Low => self.low_label.as_deref(),
            ReasoningEffortLevel::Medium => self.medium_label.as_deref(),
            ReasoningEffortLevel::High => self.high_label.as_deref(),
            ReasoningEffortLevel::XHigh => self.x_high_label.as_deref(),
        }
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

/// Structured provider-usage capture configuration loaded from one provider definition.
/// 从单个供应商定义中载入的结构化 Provider 用量捕获配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageCaptureConfig {
    /// Optional OpenAI-compatible streaming usage policy used by chat-completions style transports.
    /// 供 chat-completions 风格传输链路使用的可选 OpenAI 兼容流式 usage 策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_compatible_streaming: Option<OpenAiCompatibleStreamingUsageConfig>,
}

impl ProviderUsageCaptureConfig {
    /// Returns whether the current usage-capture configuration contains no explicit provider rules.
    /// 返回当前用量捕获配置是否未声明任何显式供应商规则。
    pub const fn is_empty(&self) -> bool {
        self.openai_compatible_streaming.is_none()
    }
}

/// Structured OpenAI-compatible streaming usage policy loaded from one provider definition.
/// 从单个供应商定义中载入的结构化 OpenAI 兼容流式 usage 策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleStreamingUsageConfig {
    /// Whether the host should explicitly request the terminal usage chunk through `stream_options.include_usage`.
    /// 宿主是否应通过 `stream_options.include_usage` 显式请求终态 usage 块。
    #[serde(default)]
    pub request_include_usage: bool,
    /// Ordered terminal usage sources accepted by the current provider.
    /// 当前供应商可接受的终态 usage 来源优先级列表。
    #[serde(default)]
    pub response_sources: Vec<OpenAiCompatibleStreamingUsageSource>,
}

/// Supported terminal usage source variants for OpenAI-compatible chat-completions streaming providers.
/// OpenAI 兼容 chat-completions 流式供应商支持的终态 usage 来源变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenAiCompatibleStreamingUsageSource {
    /// Uses one top-level `usage` payload carried by a terminal chunk that still includes choices.
    /// 使用仍包含 choices 的终态块上携带的顶层 `usage` 载荷。
    TopLevelTerminalChunk,
    /// Uses one top-level `usage` payload carried by a terminal chunk whose `choices` array is empty.
    /// 使用 `choices` 数组为空的终态块上携带的顶层 `usage` 载荷。
    TopLevelEmptyChoicesChunk,
    /// Uses one terminal `usage` payload nested inside a completed streamed choice.
    /// 使用嵌套在已结束流式 choice 内部的终态 `usage` 载荷。
    TerminalChoice,
}

/// Stable OpenRouter endpoint route selection persisted for one concrete model.
/// 为某个具体模型持久化保存的稳定 OpenRouter 端点路由选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRouterEndpointRouteSelection {
    /// Stable endpoint tag forwarded to the OpenRouter `provider.order` request field.
    /// 转发到 OpenRouter `provider.order` 请求字段中的稳定端点标签。
    pub tag: String,
    /// Human-readable provider label rendered in the workbench trigger and pending-change hints.
    /// 渲染在工作台触发器与待生效提示中的人类可读供应商标签。
    pub provider_name: String,
    /// Optional endpoint display name exposed by the endpoint catalog.
    /// 由端点目录暴露的可选端点显示名称。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint_name: String,
}

impl OpenRouterEndpointRouteSelection {
    /// Returns the compact display label rendered by the workbench trigger.
    /// 返回由工作台触发器渲染的紧凑显示标签。
    pub fn display_label(&self) -> String {
        if !self.provider_name.trim().is_empty() {
            return self.provider_name.trim().to_string();
        }
        if !self.endpoint_name.trim().is_empty() {
            return self.endpoint_name.trim().to_string();
        }
        self.tag.trim().to_string()
    }

    /// Returns whether the persisted selection carries no usable endpoint identity.
    /// 返回该持久化选择是否不包含可用的端点身份信息。
    pub fn is_empty(&self) -> bool {
        self.tag.trim().is_empty()
    }
}

/// Supported model discovery kinds used by the current WebUI implementation.
/// 当前 WebUI 实现支持的模型发现方式。
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    /// Optional AI-specialized tool declarations attached to this exact model.
    /// 附着在当前精确模型上的可选 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
    /// Optional input modality list.
    /// 可选的输入模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modality list.
    /// 可选的输出模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
    /// Whether the model should be checked by default the first time one provider instance is configured.
    /// 当前供应商实例首次配置时，该模型是否应默认被勾选。
    #[serde(default, skip_serializing_if = "is_false")]
    pub default_enabled: bool,
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
    /// Optional provider-specific usage capture rules reused by runtime request construction and terminal usage extraction.
    /// 供运行时请求构造与终态 usage 提取复用的可选供应商专属用量捕获规则。
    #[serde(default, skip_serializing_if = "ProviderUsageCaptureConfig::is_empty")]
    pub usage_capture: ProviderUsageCaptureConfig,
    /// Optional per-model runtime overrides for providers whose individual models use different compatible protocols.
    /// 当同一供应商下不同模型使用不同兼容协议时采用的可选模型级运行时覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_runtime_overrides: BTreeMap<String, ProviderRuntimeKind>,
    /// Optional provider-scoped reasoning profile used when the current runtime family needs non-standard or provider-tuned controls.
    /// 当当前运行时家族需要非标准或供应商特调控制时使用的可选供应商级推理档位配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional provider-scoped AI-specialized tool declarations used as model defaults.
    /// 作为模型默认值使用的可选供应商级 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
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
    /// Returns the effective runtime for one concrete model under this provider.
    /// 返回当前供应商下某个具体模型的生效运行时。
    pub fn effective_runtime_for_model(&self, model_id: &str) -> ProviderRuntimeKind {
        self.model_runtime_overrides
            .get(model_id)
            .copied()
            .unwrap_or(self.runtime)
    }

    /// Returns the effective API family for one concrete model under this provider.
    /// 返回当前供应商下某个具体模型的生效 API 接口族。
    pub fn effective_api_family_for_model(&self, model_id: Option<&str>) -> ProviderApiFamily {
        model_id
            .map(|id| self.effective_runtime_for_model(id))
            .unwrap_or(self.runtime)
            .api_family()
    }

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
        let active_profile = model
            .and_then(|item| item.reasoning_profile.as_ref())
            .or(self.reasoning_profile.as_ref());
        let supported_efforts = self.supported_reasoning_efforts(model);
        let selected_effort = reconcile_reasoning_effort(preferred_effort, &supported_efforts);
        let options = build_reasoning_selection_options(active_profile, &supported_efforts);
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
        if let Some(profile) = model
            .and_then(|item| item.reasoning_profile.as_ref())
            .or(self.reasoning_profile.as_ref())
        {
            let profile_efforts = profile.available_efforts();
            if !profile_efforts.is_empty() {
                return profile_efforts;
            }
        }

        let model_supports_reasoning = model.and_then(|item| item.reasoning).unwrap_or(false);
        if !model_supports_reasoning {
            return vec![ReasoningEffortLevel::Off];
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

    match runtime.api_family() {
        ProviderApiFamily::GoogleGeminiGenerateContent => {
            infer_google_gemini_reasoning_efforts(&model_id)
        }
        ProviderApiFamily::CohereChat => {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderApiFamily::OpenAiChatCompatible
            if runtime == ProviderRuntimeKind::OpenAiCompatible
                && matches!(
                    provider_id,
                    "alibaba"
                        | "alibaba-cn"
                        | "alibaba-coding-plan"
                        | "alibaba-coding-plan-cn"
                        | "bailing"
                ) =>
        {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderApiFamily::OpenAiResponses
        | ProviderApiFamily::OpenAiChatCompatible
        | ProviderApiFamily::AnthropicMessages
        | ProviderApiFamily::BedrockConverse => ReasoningEffortLevel::ALL.into_iter().collect(),
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

/// Applies provider-specific reasoning-mode overrides onto one mutable model list after catalog loading or remote refresh merging.
/// 在目录加载或远端刷新合并后，将供应商专属思考模式覆盖应用到可变模型列表上。
fn apply_provider_reasoning_model_overrides(provider_id: &str, models: &mut [ModelChoice]) {
    for model in models {
        let Some(mode) = provider_reasoning_mode(provider_id, model.id.as_str()) else {
            continue;
        };

        model.reasoning = Some(true);
        model.reasoning_profile = Some(match mode {
            ProviderReasoningMode::DefaultOnly => default_only_reasoning_profile(),
            ProviderReasoningMode::BinaryToggle => binary_toggle_reasoning_profile(),
        });
    }
}

/// Returns the normalized provider-specific reasoning control mode for one provider/model pair when the catalog needs an explicit override.
/// 当目录需要显式覆盖时，返回某个供应商/模型组合归一化后的专属思考控制模式。
fn provider_reasoning_mode(provider_id: &str, model_id: &str) -> Option<ProviderReasoningMode> {
    if is_alibaba_family_provider(provider_id) {
        return Some(alibaba_family_reasoning_mode(model_id));
    }

    None
}

/// Returns whether one provider belongs to the four Alibaba chat-compatible interfaces that share the same reasoning-mode matrix.
/// 返回某个供应商是否属于共享同一思考模式矩阵的四条阿里 Chat-Compatible 接口。
fn is_alibaba_family_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "alibaba" | "alibaba-cn" | "alibaba-coding-plan" | "alibaba-coding-plan-cn"
    )
}

/// Resolves the Alibaba-family reasoning control mode using the maintained model matrix while leaving all unmaintained models on provider defaults.
/// 使用维护中的模型矩阵解析阿里系思考控制模式，并让未维护模型继续沿用接口默认行为。
fn alibaba_family_reasoning_mode(model_id: &str) -> ProviderReasoningMode {
    let normalized = model_id.trim().to_ascii_lowercase();

    if alibaba_family_model_supports_binary_toggle(normalized.as_str()) {
        ProviderReasoningMode::BinaryToggle
    } else {
        ProviderReasoningMode::DefaultOnly
    }
}

/// Returns whether one Alibaba-family model currently belongs to the maintained "supports on/off toggle" matrix.
/// 返回某个阿里系模型当前是否属于已维护的“支持开启/关闭切换”矩阵。
fn alibaba_family_model_supports_binary_toggle(model_id: &str) -> bool {
    if matches!(
        model_id,
        "qwen3.6-max-preview"
            | "qwen3.6-35b-a3b"
            | "qwen3.5-397b-a17b"
            | "qwen3.5-122b-a10b"
            | "qwen3.5-27b"
            | "qwen3.5-35b-a3b"
            | "qwen3-max"
            | "qwen3-max-preview"
            | "qwen-plus"
            | "qwen-plus-latest"
            | "qwen-flash"
            | "qwen-turbo"
            | "qwen-turbo-latest"
            | "qwen3-235b-a22b"
            | "qwen3-32b"
            | "qwen3-30b-a3b"
            | "qwen3-14b"
            | "qwen3-8b"
            | "qwen3-4b"
            | "qwen3-1.7b"
            | "qwen3-0.6b"
            | "deepseek-v4-pro"
            | "deepseek-v4-flash"
            | "deepseek-v3.2"
            | "deepseek-v3.2-exp"
            | "deepseek-v3.1"
            | "siliconflow/deepseek-v3.2"
            | "siliconflow/deepseek-v3.1-terminus"
            | "vanchin/deepseek-v3.2-think"
            | "vanchin/deepseek-v3.1-terminus"
            | "glm-5.1"
            | "glm-5"
            | "glm-4.7"
            | "glm-4.6"
            | "glm-4.5"
            | "glm-4.5-air"
            | "kimi-k2.6"
            | "kimi-k2.5"
            | "kimi/kimi-k2.6"
            | "kimi/kimi-k2.5"
    ) {
        return true;
    }

    matches_snapshot_series(
        model_id,
        &[
            "qwen3.6-plus",
            "qwen3.6-flash",
            "qwen3.5-plus",
            "qwen3.5-flash",
            "qwen3-max",
            "qwen-plus",
            "qwen-flash",
            "qwen-turbo",
        ],
    )
}

/// Returns whether the current model identifier belongs to one of the maintained snapshot-backed series prefixes.
/// 返回当前模型标识是否属于某个已维护的快照系列前缀。
fn matches_snapshot_series(model_id: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        model_id == *prefix
            || model_id
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('-'))
    })
}

/// Builds the unified "follow provider/model default" reasoning profile used when the host should expose a non-editable default entry.
/// 构建统一的“沿用供应商或模型默认行为”思考配置，用于宿主仅展示不可编辑的默认项。
fn default_only_reasoning_profile() -> ReasoningProfileConfig {
    ReasoningProfileConfig {
        kind: Some(ReasoningCapabilityKind::DefaultOnly),
        ..ReasoningProfileConfig::default()
    }
}

/// Builds the unified Alibaba-style on/off reasoning profile used by models that explicitly support a host-controlled toggle.
/// 构建阿里系显式支持宿主开关控制模型使用的统一开启/关闭思考配置。
fn binary_toggle_reasoning_profile() -> ReasoningProfileConfig {
    ReasoningProfileConfig {
        kind: Some(ReasoningCapabilityKind::Binary),
        off: Some(json!({ "enable_thinking": false })),
        low: Some(json!({ "enable_thinking": true })),
        ..ReasoningProfileConfig::default()
    }
}

/// Enumerates the normalized provider-specific reasoning control modes reused by catalog overlays.
/// 枚举目录覆盖复用的规范化供应商专属思考控制模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderReasoningMode {
    /// Keeps provider/model defaults and exposes a non-editable “默认” entry.
    /// 保持供应商或模型默认行为，并暴露不可编辑的“默认”项。
    DefaultOnly,
    /// Exposes one host-controlled on/off toggle that maps to provider-native patches.
    /// 暴露一个映射到供应商原生 Patch 的宿主开启/关闭开关。
    BinaryToggle,
}

/// Builds the visible reasoning selector options from the ordered supported effort list.
/// 根据有序支持档位列表构建可见的思考选择器选项。
fn build_reasoning_selection_options(
    profile: Option<&ReasoningProfileConfig>,
    supported_efforts: &[ReasoningEffortLevel],
) -> Vec<ReasoningSelectionOption> {
    if matches!(
        profile.and_then(|item| item.kind),
        Some(ReasoningCapabilityKind::DefaultOnly)
    ) {
        return vec![ReasoningSelectionOption {
            effort: ReasoningEffortLevel::Off,
            label: String::from("默认"),
        }];
    }

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
            label: profile
                .and_then(|item| item.label_for_effort(effort))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if uses_binary_labels && effort.requests_explicit_reasoning() {
                        String::from("开启")
                    } else {
                        effort.compact_label().to_string()
                    }
                }),
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
    /// Optional lifecycle status.
    /// 可选的生命周期状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional family label.
    /// 可选的家族标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional release-date label.
    /// 可选的发布日期标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Optional context-window limit.
    /// 可选的上下文窗口上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input-token limit.
    /// 可选的输入 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output-token limit.
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
    /// Optional temperature capability flag.
    /// 可选的温度能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    /// Optional attachment capability flag.
    /// 可选的附件能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional interleaved reasoning descriptor.
    /// 可选的交错推理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<ModelInterleavedCapability>,
    /// Optional model-scoped reasoning profile.
    /// 可选的模型级推理档位配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional cached AI-specialized tool declarations.
    /// 可选的缓存 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
    /// Optional input modalities.
    /// 可选的输入模态集合。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modalities.
    /// 可选的输出模态集合。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
    /// Whether the model belongs to the provider-declared default-enabled subset.
    /// 当前模型是否属于供应商声明的默认启用子集。
    #[serde(default, skip_serializing_if = "is_false")]
    pub default_enabled: bool,
}

/// Persisted user override metadata for one model entry.
/// 单个模型条目的持久化用户覆盖元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedModelOverrideEntry {
    /// Optional reasoning capability override.
    /// 可选的推理能力覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional attachment capability override kept for diagnostics and metadata parity.
    /// 供诊断与元数据对齐使用的可选附件能力覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional context-window limit override.
    /// 可选的上下文窗口上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input-token limit override.
    /// 可选的输入 Token 上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output-token limit override.
    /// 可选的输出 Token 上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning-profile override.
    /// 可选的推理档位配置覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional input-modality override list.
    /// 可选的输入模态覆盖列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<String>>,
    /// Optional output-modality override list.
    /// 可选的输出模态覆盖列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<String>>,
}

impl PersistedModelOverrideEntry {
    /// Returns whether the override entry is effectively empty and can be skipped during serialization.
    /// 返回该覆盖条目是否实际上为空，从而可在序列化时跳过。
    pub fn is_empty(&self) -> bool {
        self.reasoning.is_none()
            && self.attachment.is_none()
            && self.context_limit.is_none()
            && self.input_limit.is_none()
            && self.output_limit.is_none()
            && self.reasoning_profile.is_none()
            && self.input_modalities.is_none()
            && self.output_modalities.is_none()
    }
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
    /// Deprecated remote model cache retained only for backward-compatible deserialization.
    /// 仅为向后兼容反序列化而保留的废弃远端模型缓存。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, PersistedModelEntry>,
    /// Explicitly enabled model identifiers, where `None` preserves legacy "all enabled" compatibility.
    /// 显式启用的模型标识集合，其中 `None` 用于保留旧版“全部启用”的兼容语义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_models: Option<Vec<String>>,
    /// User-authored per-model metadata overrides.
    /// 用户编写的逐模型元数据覆盖配置。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_overrides: BTreeMap<String, PersistedModelOverrideEntry>,
}

impl PersistedProviderEntry {
    /// Replaces the explicit enabled-model set with one stable deduplicated list.
    /// 使用一份稳定去重后的列表替换显式启用模型集合。
    pub fn set_enabled_models<I>(&mut self, model_ids: I)
    where
        I: IntoIterator<Item = String>,
    {
        let ordered = model_ids.into_iter().collect::<BTreeSet<_>>();
        self.enabled_models = Some(ordered.into_iter().collect());
    }
}

mod runtime_persistence;

pub use runtime_persistence::*;
/// Stable transcript turn-page size options exposed by the settings page.
/// 设置页面暴露的稳定转录轮次分页选项集合。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS: [usize; 4] = [20, 30, 40, 50];

/// Default transcript turn-page size used when the shared configuration file does not specify an override.
/// 当共享配置文件未指定覆盖值时使用的默认转录轮次分页大小。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT: usize = 30;

/// Persisted provider retry behavior shared by provider transports.
/// 由 Provider 传输链路共享的持久化请求重试行为配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedProviderRetryConfiguration {
    /// Whether automatic retry is enabled for retryable provider failures.
    /// 是否为可重试的 Provider 失败启用自动重试。
    #[serde(default = "default_provider_retry_enabled")]
    pub enabled: bool,
    /// Maximum number of retries after the initial request attempt.
    /// 初始请求失败后允许追加重试的最大次数。
    #[serde(rename = "maxRetries", default = "default_provider_retry_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay in milliseconds before exponential backoff is applied.
    /// 应用指数退避前的初始重试等待毫秒数。
    #[serde(
        rename = "initialDelayMs",
        default = "default_provider_retry_initial_delay_ms"
    )]
    pub initial_delay_ms: u64,
    /// Maximum retry delay in milliseconds when no provider header asks for a longer wait.
    /// 在没有 Provider 响应头要求更久等待时使用的最大重试等待毫秒数。
    #[serde(rename = "maxDelayMs", default = "default_provider_retry_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier applied after each failed attempt.
    /// 每次失败后应用的指数退避倍数。
    #[serde(
        rename = "backoffFactor",
        default = "default_provider_retry_backoff_factor"
    )]
    pub backoff_factor: f64,
    /// Deterministic jitter percentage used to avoid synchronized retry bursts.
    /// 用于避免同步重试尖峰的确定性抖动百分比。
    #[serde(
        rename = "jitterPercent",
        default = "default_provider_retry_jitter_percent"
    )]
    pub jitter_percent: u8,
    /// Whether `Retry-After` style provider hints should override local backoff.
    /// 是否允许 `Retry-After` 类型的 Provider 提示覆盖本地退避时间。
    #[serde(
        rename = "respectRetryAfter",
        default = "default_provider_retry_respect_retry_after"
    )]
    pub respect_retry_after: bool,
    /// Whether HTTP 429 should be retried when it is not a quota-depleted failure.
    /// 当 HTTP 429 不是配额耗尽错误时是否允许重试。
    #[serde(rename = "retry429", default = "default_provider_retry_429")]
    pub retry_429: bool,
}

impl Default for PersistedProviderRetryConfiguration {
    /// Builds the safe default provider retry configuration.
    /// 构建安全的默认 Provider 重试配置。
    fn default() -> Self {
        Self {
            enabled: default_provider_retry_enabled(),
            max_retries: default_provider_retry_max_retries(),
            initial_delay_ms: default_provider_retry_initial_delay_ms(),
            max_delay_ms: default_provider_retry_max_delay_ms(),
            backoff_factor: default_provider_retry_backoff_factor(),
            jitter_percent: default_provider_retry_jitter_percent(),
            respect_retry_after: default_provider_retry_respect_retry_after(),
            retry_429: default_provider_retry_429(),
        }
    }
}

impl PersistedProviderRetryConfiguration {
    /// Returns whether the persisted retry block is equal to the built-in default.
    /// 返回当前持久化重试配置块是否等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Returns the default automatic provider retry enablement.
/// 返回默认的 Provider 自动重试启用状态。
fn default_provider_retry_enabled() -> bool {
    true
}

/// Returns the default maximum provider retry count.
/// 返回默认的 Provider 最大重试次数。
fn default_provider_retry_max_retries() -> u32 {
    5
}

/// Returns the default initial provider retry delay in milliseconds.
/// 返回默认的 Provider 初始重试等待毫秒数。
fn default_provider_retry_initial_delay_ms() -> u64 {
    2_000
}

/// Returns the default maximum provider retry delay in milliseconds.
/// 返回默认的 Provider 最大重试等待毫秒数。
fn default_provider_retry_max_delay_ms() -> u64 {
    30_000
}

/// Returns the default provider retry exponential backoff factor.
/// 返回默认的 Provider 重试指数退避倍数。
fn default_provider_retry_backoff_factor() -> f64 {
    2.0
}

/// Returns the default deterministic provider retry jitter percentage.
/// 返回默认的 Provider 重试确定性抖动百分比。
fn default_provider_retry_jitter_percent() -> u8 {
    20
}

/// Returns the default provider retry-after handling switch.
/// 返回默认的 Provider Retry-After 处理开关。
fn default_provider_retry_respect_retry_after() -> bool {
    true
}

/// Returns the default provider HTTP 429 retry switch.
/// 返回默认的 Provider HTTP 429 重试开关。
fn default_provider_retry_429() -> bool {
    true
}

/// Normalizes one persisted transcript turn-page size to the supported fixed presets.
/// 将一条持久化转录轮次分页大小归一化为受支持的固定预设。
pub fn normalize_session_transcript_turn_page_size(value: Option<usize>) -> usize {
    let normalized = value.unwrap_or(SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT);
    if SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS.contains(&normalized) {
        normalized
    } else {
        SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT
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
    /// Remembered WebUI locale identifier selected by the user.
    /// 由用户选择并记忆的WebUI语言标识。
    #[serde(rename = "uiLocale", skip_serializing_if = "Option::is_none")]
    pub ui_locale: Option<String>,
    /// Remembered startup transcript page size expressed in complete user turns.
    /// 以完整用户轮次表示的已记忆启动转录分页大小。
    #[serde(
        rename = "sessionTranscriptTurnPageSize",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_transcript_turn_page_size: Option<usize>,
    /// Persisted provider retry behavior used by real provider dispatches.
    /// 真实 Provider 分发使用的持久化请求重试行为。
    #[serde(
        default,
        skip_serializing_if = "PersistedProviderRetryConfiguration::is_default"
    )]
    pub retry: PersistedProviderRetryConfiguration,
    /// Remembered default command shell selected by the user to override host-side `auto` resolution.
    /// 由用户选择并记忆、用于覆盖宿主侧 `auto` 解析的默认命令 shell。
    #[serde(
        rename = "defaultCommandShell",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_command_shell: Option<String>,
    /// Remembered OpenRouter endpoint route selections keyed by the exact OpenRouter model identifier.
    /// 以精确 OpenRouter 模型标识为键记忆的 OpenRouter 端点路由选择集合。
    #[serde(
        rename = "openrouterEndpointRoutes",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub openrouter_endpoint_routes: BTreeMap<String, OpenRouterEndpointRouteSelection>,
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
    /// Host-managed LuaSkills visibility overrides persisted independently from package lifecycle state.
    /// 与技能包生命周期状态独立持久化的宿主管理 LuaSkills 可见性覆盖配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedLuaSkillsConfiguration::is_default"
    )]
    pub luaskills: PersistedLuaSkillsConfiguration,
    /// Host-managed Prompt Skills configuration persisted independently from LuaSkills.
    /// 与 LuaSkills 独立持久化的宿主管理提示词技能配置。
    #[serde(
        default,
        rename = "promptSkills",
        skip_serializing_if = "PersistedPromptSkillsConfiguration::is_default"
    )]
    pub prompt_skills: PersistedPromptSkillsConfiguration,
    /// Legacy embedded AI tool routing and compatibility configuration kept only as a migration fallback for the standalone AI capability file.
    /// 仅作为独立 AI 能力配置文件迁移回退而保留的旧版内嵌 AI 工具路由与兼容配置。
    #[serde(
        default,
        rename = "aiTools",
        skip_serializing_if = "PersistedAiToolConfiguration::is_default"
    )]
    pub ai_tools: PersistedAiToolConfiguration,
    /// Global VMM memory integration configuration.
    /// 全局 VMM 记忆集成配置。
    #[serde(default, skip_serializing_if = "VmmConfigLayer::is_empty")]
    pub vmm: VmmConfigLayer,
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
    /// Persisted prompt-composer configuration.
    /// 持久化的提示词组合配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedPromptConfiguration::is_default"
    )]
    pub prompt: PersistedPromptConfiguration,
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

/// Returns the project-local Vulcan Code configuration file path for one workspace.
/// 返回单个工作区的项目级 Vulcan Code 配置文件路径。
pub fn project_configuration_file_path(workspace_path: &Path) -> PathBuf {
    workspace_path
        .join(".vulcan")
        .join("code")
        .join("config.json")
}

/// Loads one project-local Vulcan Code configuration file from disk.
/// 从磁盘加载单个项目级 Vulcan Code 配置文件。
pub fn load_project_configuration_file(
    workspace_path: &Path,
) -> Result<ProjectVulcanCodeConfigurationFile, String> {
    let path = project_configuration_file_path(workspace_path);
    if !path.exists() {
        return Ok(ProjectVulcanCodeConfigurationFile::default());
    }

    let text =
        fs::read_to_string(&path).map_err(|error| format!("读取项目配置文件失败：{}。", error))?;
    serde_json::from_str(&text).map_err(|error| format!("解析项目配置文件失败：{}。", error))
}

/// Saves one project-local Vulcan Code configuration file back to disk.
/// 将单个项目级 Vulcan Code 配置文件写回磁盘。
pub fn save_project_configuration_file(
    workspace_path: &Path,
    config: &ProjectVulcanCodeConfigurationFile,
) -> Result<(), String> {
    let path = project_configuration_file_path(workspace_path);
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_path.to_path_buf());
    fs::create_dir_all(&parent).map_err(|error| format!("创建项目配置目录失败：{}。", error))?;
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("序列化项目配置失败：{}。", error))?;
    fs::write(&path, content).map_err(|error| format!("写入项目配置失败：{}。", error))
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

    let normalized_config = normalize_provider_configuration_for_save(config);
    let content = serde_json::to_string_pretty(&normalized_config)
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

/// Resolves the full catalog model choices for one provider after merging embedded metadata and user overrides.
/// 合并内置元数据与用户覆盖后，为某个供应商解析完整目录模型列表。
pub fn catalog_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let mut embedded_models = supported_provider_by_id(provider_id)
        .map(|definition| definition.models.clone())
        .unwrap_or_default();
    apply_provider_reasoning_model_overrides(provider_id, &mut embedded_models);

    let mut catalog = embedded_models.clone();

    if let Some(entry) = config.provider.get(provider_id) {
        catalog = apply_model_overrides_to_catalog(catalog, &entry.model_overrides);
    }

    apply_provider_reasoning_model_overrides(provider_id, &mut catalog);
    catalog.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    catalog.dedup_by(|left, right| left.id == right.id);
    catalog
}

/// Resolves the effective enabled model choices for one provider by filtering the full catalog through explicit enabled-model preferences.
/// 通过显式启用模型偏好过滤完整目录，为某个供应商解析生效可用模型列表。
pub fn effective_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let catalog = catalog_model_choices(provider_id, config);
    let enabled_ids = resolved_enabled_model_ids(config.provider.get(provider_id), &catalog);

    catalog
        .into_iter()
        .filter(|model| enabled_ids.contains(&model.id))
        .collect()
}

/// Resolves the explicit enabled-model identifier set for one provider using persisted preferences or first-save defaults.
/// 使用持久化偏好或首次保存默认值，为某个供应商解析显式启用模型标识集合。
pub fn enabled_model_ids(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> BTreeSet<String> {
    let catalog = catalog_model_choices(provider_id, config);
    resolved_enabled_model_ids(config.provider.get(provider_id), &catalog)
}

/// Parses the active provider/model tuple from the persisted global model string.
/// 从持久化的全局模型字符串中解析激活的供应商与模型二元组。
pub fn parse_active_model_reference(model: &str) -> Option<(&str, &str)> {
    let (provider_id, model_id) = model.split_once('/')?;
    Some((provider_id, model_id))
}

/// Returns the remembered OpenRouter endpoint route selection for one exact model identifier.
/// 返回某个精确模型标识已记忆的 OpenRouter 端点路由选择。
pub fn openrouter_endpoint_route_for_model(
    config: &ProviderConfigurationFile,
    model_id: &str,
) -> Option<OpenRouterEndpointRouteSelection> {
    config
        .openrouter_endpoint_routes
        .get(model_id)
        .cloned()
        .filter(|selection| !selection.is_empty())
}

/// Persists or clears one remembered OpenRouter endpoint route selection for the given model identifier.
/// 为指定模型标识持久化保存或清除一条已记忆的 OpenRouter 端点路由选择。
pub fn set_openrouter_endpoint_route_for_model(
    config: &mut ProviderConfigurationFile,
    model_id: impl Into<String>,
    selection: Option<OpenRouterEndpointRouteSelection>,
) {
    let model_id = model_id.into();
    if let Some(selection) = selection.filter(|selection| !selection.is_empty()) {
        config
            .openrouter_endpoint_routes
            .insert(model_id, selection);
    } else {
        config.openrouter_endpoint_routes.remove(model_id.as_str());
    }
}

#[cfg(test)]
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
                    status: model.status.clone(),
                    family: model.family.clone(),
                    release_date: model.release_date.clone(),
                    context_limit: model.context_limit,
                    input_limit: model.input_limit,
                    output_limit: model.output_limit,
                    reasoning: model.reasoning,
                    tool_call: model.tool_call,
                    temperature: model.temperature,
                    attachment: model.attachment,
                    interleaved: model.interleaved.clone(),
                    reasoning_profile: model.reasoning_profile.clone(),
                    ai_tools: model.ai_tools.clone(),
                    input_modalities: model.input_modalities.clone(),
                    output_modalities: model.output_modalities.clone(),
                    default_enabled: model.default_enabled,
                },
            )
        })
        .collect()
}

/// Returns the first-save default enabled model identifiers declared inside one catalog.
/// 返回单个目录内部声明的首次保存默认启用模型标识集合。
pub fn default_enabled_model_ids(models: &[ModelChoice]) -> Vec<String> {
    let default_ids = models
        .iter()
        .filter(|model| model.default_enabled)
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>();

    if !default_ids.is_empty() {
        return default_ids.into_iter().collect();
    }

    models
        .iter()
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
/// Merges a freshly refreshed remote model catalog into one persisted provider entry while preserving explicit enablement and user overrides.
/// 在保留显式启用结果与用户覆盖的前提下，将最新远端目录刷新结果合并回持久化供应商条目。
pub fn merge_refreshed_models(
    entry: &mut PersistedProviderEntry,
    refreshed_models: &[ModelChoice],
) {
    let previous_ids = entry.models.keys().cloned().collect::<BTreeSet<_>>();
    entry.models = persist_remote_models(refreshed_models);

    let valid_ids = entry.models.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(enabled_models) = entry.enabled_models.as_mut() {
        let mut next_ids = enabled_models
            .iter()
            .filter(|model_id| valid_ids.contains(model_id.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>();
        for model in refreshed_models
            .iter()
            .filter(|model| model.default_enabled && !previous_ids.contains(model.id.as_str()))
        {
            next_ids.insert(model.id.clone());
        }
        *enabled_models = next_ids.into_iter().collect();
    }
    entry
        .model_overrides
        .retain(|model_id, _| valid_ids.contains(model_id));
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
            let mut definition = serde_json::from_str::<SupportedProviderDefinition>(&content)
                .unwrap_or_else(|error| {
                    panic!(
                        "provider catalog resource `{}` must stay valid: {error}",
                        path.display()
                    )
                });
            apply_provider_reasoning_model_overrides(
                definition.id.as_str(),
                &mut definition.models,
            );

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

#[cfg(test)]
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

    let endpoint = format!("{}/models", normalized.trim_end_matches('/'));

    if definition.id == "openrouter" {
        return Some(format!("{endpoint}?output_modalities=text"));
    }

    Some(endpoint)
}

#[cfg(test)]
/// Applies provider-specific default filtering to one remotely refreshed model list.
/// 对单次远端刷新得到的模型列表应用供应商级默认过滤规则。
fn filter_remote_model_choices(provider_id: &str, choices: Vec<ModelChoice>) -> Vec<ModelChoice> {
    if provider_id != "openrouter" {
        return choices;
    }

    choices
        .into_iter()
        .filter(openrouter_model_choice_is_conversation_model)
        .collect()
}

#[cfg(test)]
/// Returns whether one OpenRouter model entry should stay visible in the default conversation-model picker.
/// 返回某个 OpenRouter 模型条目是否应保留在默认对话模型选择器中。
fn openrouter_model_choice_is_conversation_model(model: &ModelChoice) -> bool {
    if openrouter_model_matches_non_conversation_keywords(model) {
        return false;
    }

    if !openrouter_model_supports_text_input(model) {
        return false;
    }

    if !openrouter_model_supports_text_only_output(model) {
        return false;
    }

    true
}

#[cfg(test)]
/// Returns whether one OpenRouter model exposes text as an accepted input modality.
/// 返回某个 OpenRouter 模型是否将文本声明为可接受输入模态。
fn openrouter_model_supports_text_input(model: &ModelChoice) -> bool {
    if model.input_modalities.is_empty() {
        return true;
    }

    model
        .input_modalities
        .iter()
        .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
}

#[cfg(test)]
/// Returns whether one OpenRouter model only emits text outputs suitable for chat-style turns.
/// 返回某个 OpenRouter 模型是否仅输出适合聊天回合的文本结果。
fn openrouter_model_supports_text_only_output(model: &ModelChoice) -> bool {
    if model.output_modalities.is_empty() {
        return true;
    }

    model
        .output_modalities
        .iter()
        .all(|modality| modality.trim().eq_ignore_ascii_case("text"))
        && model
            .output_modalities
            .iter()
            .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
}

#[cfg(test)]
/// Returns whether one OpenRouter model id/name clearly points to non-conversation tasks that should stay hidden by default.
/// 返回某个 OpenRouter 模型标识或名称是否明确指向默认应隐藏的非对话任务。
fn openrouter_model_matches_non_conversation_keywords(model: &ModelChoice) -> bool {
    let identifier = format!("{} {}", model.id, model.name).to_ascii_lowercase();

    [
        "rerank",
        "embedding",
        "embeddings",
        "vector",
        "ocr",
        "transcribe",
        "transcription",
        "speech",
        "tts",
        "asr",
        "moderation",
    ]
    .iter()
    .any(|keyword| identifier.contains(keyword))
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

/// Returns the default number of search results requested from external web-search providers.
/// 返回外部网页搜索供应商默认请求的搜索结果数量。
fn default_web_search_max_results() -> usize {
    5
}

/// Returns whether one value still equals its type default for compact serialization.
/// 返回某个值是否仍等于其类型默认值，以便紧凑序列化。
fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    *value == T::default()
}

/// Returns whether one boolean value is false for compact serialization.
/// 返回单个布尔值是否为 false，以便紧凑序列化。
fn is_false(value: &bool) -> bool {
    !*value
}

/// Returns whether one boolean value is true for compact serialization.
/// 返回单个布尔值是否为 true，以便紧凑序列化。
fn is_true(value: &bool) -> bool {
    *value
}

/// Returns whether one AI-tool policy is the default automatic policy.
/// 返回单个 AI 工具策略是否为默认自动策略。
fn is_default_ai_tool_policy(policy: &AiToolPolicy) -> bool {
    *policy == AiToolPolicy::Auto
}

#[cfg(test)]
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
                    ai_tools: Vec::new(),
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                    default_enabled: false,
                });
            }

            let object = item.as_object()?;
            let id = object.get("id")?.as_str()?.to_string();
            let name = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();
            let (input_modalities, output_modalities) = dynamic_model_modalities(object);

            Some(ModelChoice {
                id,
                name,
                status: dynamic_model_string(object, &["status"]),
                family: dynamic_model_string(object, &["family"]),
                release_date: dynamic_model_string(
                    object,
                    &["releaseDate", "release_date", "releasedAt", "released_at"],
                ),
                context_limit: dynamic_model_usize(
                    object,
                    &[
                        "contextLimit",
                        "context_limit",
                        "contextWindow",
                        "context_window",
                    ],
                ),
                input_limit: dynamic_model_usize(object, &["inputLimit", "input_limit"]),
                output_limit: dynamic_model_usize(object, &["outputLimit", "output_limit"]),
                reasoning: dynamic_model_bool(object, &["reasoning"]),
                tool_call: dynamic_model_bool(object, &["toolCall", "tool_call"]),
                temperature: dynamic_model_bool(object, &["temperature"]),
                attachment: dynamic_model_bool(object, &["attachment"]),
                interleaved: dynamic_model_interleaved(object, &["interleaved"]),
                reasoning_profile: dynamic_model_reasoning_profile(
                    object,
                    &["reasoningProfile", "reasoning_profile"],
                ),
                ai_tools: dynamic_model_ai_tools(object, &["aiTools", "ai_tools"]),
                input_modalities,
                output_modalities,
                default_enabled: dynamic_model_bool(object, &["defaultEnabled", "default_enabled"])
                    .unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
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
                name: if model.name.trim().is_empty() {
                    embedded.name.clone()
                } else {
                    model.name
                },
                status: model.status.or_else(|| embedded.status.clone()),
                family: model.family.or_else(|| embedded.family.clone()),
                release_date: model.release_date.or_else(|| embedded.release_date.clone()),
                context_limit: model.context_limit.or(embedded.context_limit),
                input_limit: model.input_limit.or(embedded.input_limit),
                output_limit: model.output_limit.or(embedded.output_limit),
                reasoning: model.reasoning.or(embedded.reasoning),
                tool_call: model.tool_call.or(embedded.tool_call),
                temperature: model.temperature.or(embedded.temperature),
                attachment: model.attachment.or(embedded.attachment),
                interleaved: model.interleaved.or_else(|| embedded.interleaved.clone()),
                reasoning_profile: model
                    .reasoning_profile
                    .or_else(|| embedded.reasoning_profile.clone()),
                ai_tools: if model.ai_tools.is_empty() {
                    embedded.ai_tools.clone()
                } else {
                    model.ai_tools
                },
                input_modalities: if model.input_modalities.is_empty() {
                    embedded.input_modalities.clone()
                } else {
                    model.input_modalities
                },
                output_modalities: if model.output_modalities.is_empty() {
                    embedded.output_modalities.clone()
                } else {
                    model.output_modalities
                },
                default_enabled: model.default_enabled || embedded.default_enabled,
            }
        })
        .collect()
}

/// Applies persisted user override metadata onto one complete catalog while keeping untouched models identical to their discovered entries.
/// 将持久化用户覆盖元数据应用到一份完整目录上，同时保持未修改模型与发现结果一致。
fn apply_model_overrides_to_catalog(
    models: Vec<ModelChoice>,
    overrides: &BTreeMap<String, PersistedModelOverrideEntry>,
) -> Vec<ModelChoice> {
    models
        .into_iter()
        .map(
            |model| match overrides.get(&model.id).filter(|entry| !entry.is_empty()) {
                Some(entry) => apply_model_override(model, entry),
                None => model,
            },
        )
        .collect()
}

/// Applies one persisted user override entry onto a single model choice.
/// 将单个持久化用户覆盖条目应用到一条模型选项上。
fn apply_model_override(
    mut model: ModelChoice,
    entry: &PersistedModelOverrideEntry,
) -> ModelChoice {
    if let Some(reasoning) = entry.reasoning {
        model.reasoning = Some(reasoning);
    }
    if let Some(attachment) = entry.attachment {
        model.attachment = Some(attachment);
    }
    if let Some(context_limit) = entry.context_limit {
        model.context_limit = Some(context_limit);
    }
    if let Some(input_limit) = entry.input_limit {
        model.input_limit = Some(input_limit);
    }
    if let Some(output_limit) = entry.output_limit {
        model.output_limit = Some(output_limit);
    }
    if let Some(reasoning_profile) = entry.reasoning_profile.clone() {
        model.reasoning_profile = Some(reasoning_profile);
    }
    if let Some(input_modalities) = entry.input_modalities.clone() {
        model.input_modalities = normalize_model_override_modalities(&input_modalities);
    }
    if let Some(output_modalities) = entry.output_modalities.clone() {
        model.output_modalities = normalize_model_override_modalities(&output_modalities);
    }

    model
}

/// Normalizes one modality string list into a stable lowercase deduplicated representation.
/// 将一组模态字符串规范化为稳定的小写去重表示。
fn normalize_model_override_modalities(values: &[String]) -> Vec<String> {
    let mut ordered = BTreeSet::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            ordered.insert(normalized);
        }
    }
    ordered.into_iter().collect()
}

/// Resolves the enabled model identifier set by honoring explicit persisted preferences first and falling back to the full catalog only for legacy entries without any saved enablement state.
/// 先尊重显式持久化偏好，再仅对没有保存启用状态的旧版条目回退到完整目录，以解析启用模型标识集合。
fn resolved_enabled_model_ids(
    entry: Option<&PersistedProviderEntry>,
    catalog: &[ModelChoice],
) -> BTreeSet<String> {
    let catalog_ids = catalog
        .iter()
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>();

    if let Some(explicit_ids) = entry.and_then(|item| item.enabled_models.as_ref()) {
        return explicit_ids
            .iter()
            .filter(|model_id| catalog_ids.contains(model_id.as_str()))
            .cloned()
            .collect();
    }

    catalog_ids
}

/// Builds one normalized configuration snapshot that drops deprecated remote model caches before serialization.
/// 构建一个会在序列化前丢弃废弃远端模型缓存的规范化配置快照。
fn normalize_provider_configuration_for_save(
    config: &ProviderConfigurationFile,
) -> ProviderConfigurationFile {
    let mut normalized = config.clone();
    for entry in normalized.provider.values_mut() {
        entry.models.clear();
    }
    normalized.luaskills.normalize();
    normalized
}

#[cfg(test)]
/// Returns the first matching dynamic model JSON value from a list of candidate field names.
/// 从候选字段名列表中返回第一条匹配的动态模型 JSON 值。
fn dynamic_model_value<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| object.get(*key))
}

#[cfg(test)]
/// Parses one optional dynamic model string field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型字符串字段。
fn dynamic_model_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    dynamic_model_value(object, keys)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
/// Parses one optional dynamic model boolean field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型布尔字段。
fn dynamic_model_bool(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<bool> {
    dynamic_model_value(object, keys).and_then(|value| match value {
        serde_json::Value::Bool(flag) => Some(*flag),
        serde_json::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
/// Parses one optional dynamic model numeric field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型数值字段。
fn dynamic_model_usize(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<usize> {
    dynamic_model_value(object, keys).and_then(|value| match value {
        serde_json::Value::Number(number) => number.as_u64().map(|value| value as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    })
}

#[cfg(test)]
/// Parses one optional dynamic interleaved reasoning capability from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态交错推理能力描述。
fn dynamic_model_interleaved(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ModelInterleavedCapability> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ModelInterleavedCapability>(value.clone()).ok())
}

#[cfg(test)]
/// Parses one optional dynamic reasoning profile from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态推理档位配置。
fn dynamic_model_reasoning_profile(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ReasoningProfileConfig> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ReasoningProfileConfig>(value.clone()).ok())
}

#[cfg(test)]
/// Parses dynamic AI-tool declarations from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析动态 AI 工具声明集合。
fn dynamic_model_ai_tools(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<AiToolDeclaration> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<Vec<AiToolDeclaration>>(value.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
/// Parses one ordered dynamic modality list while tolerating both flat and nested payload shapes.
/// 在兼容扁平与嵌套载荷形状的前提下解析一组有序动态模态列表。
fn dynamic_model_modalities(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (Vec<String>, Vec<String>) {
    let flat_input = dynamic_model_string_array(object, &["inputModalities", "input_modalities"]);
    let flat_output =
        dynamic_model_string_array(object, &["outputModalities", "output_modalities"]);

    let nested = object
        .get("modalities")
        .and_then(serde_json::Value::as_object);
    let nested_input = nested
        .map(|modalities| dynamic_model_string_array(modalities, &["input"]))
        .unwrap_or_default();
    let nested_output = nested
        .map(|modalities| dynamic_model_string_array(modalities, &["output"]))
        .unwrap_or_default();

    let architecture = object
        .get("architecture")
        .and_then(serde_json::Value::as_object);
    let architecture_input = architecture
        .map(|modalities| {
            dynamic_model_string_array(modalities, &["input_modalities", "inputModalities"])
        })
        .unwrap_or_default();
    let architecture_output = architecture
        .map(|modalities| {
            dynamic_model_string_array(modalities, &["output_modalities", "outputModalities"])
        })
        .unwrap_or_default();
    let (architecture_modality_input, architecture_modality_output) = architecture
        .and_then(|modalities| dynamic_model_string(modalities, &["modality"]))
        .map(|modality| dynamic_model_modalities_from_descriptor(&modality))
        .unwrap_or_default();

    let input_modalities = if flat_input.is_empty() {
        if nested_input.is_empty() {
            if architecture_input.is_empty() {
                architecture_modality_input
            } else {
                architecture_input
            }
        } else {
            nested_input
        }
    } else {
        flat_input
    };
    let output_modalities = if flat_output.is_empty() {
        if nested_output.is_empty() {
            if architecture_output.is_empty() {
                architecture_modality_output
            } else {
                architecture_output
            }
        } else {
            nested_output
        }
    } else {
        flat_output
    };

    (input_modalities, output_modalities)
}

#[cfg(test)]
/// Parses one compact architecture modality descriptor like `text+image->text` into ordered input/output lists.
/// 将 `text+image->text` 这样的紧凑架构模态描述解析为有序输入输出列表。
fn dynamic_model_modalities_from_descriptor(modality: &str) -> (Vec<String>, Vec<String>) {
    let Some((raw_input, raw_output)) = modality.split_once("->") else {
        return (Vec::new(), Vec::new());
    };

    (
        dynamic_model_modalities_from_half(raw_input),
        dynamic_model_modalities_from_half(raw_output),
    )
}

#[cfg(test)]
/// Parses one modality descriptor half like `text+image` into an ordered normalized list.
/// 将 `text+image` 这样的半段模态描述解析为有序归一化列表。
fn dynamic_model_modalities_from_half(raw_modalities: &str) -> Vec<String> {
    raw_modalities
        .split('+')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
}

#[cfg(test)]
/// Parses one ordered dynamic string-array field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个有序动态字符串数组字段。
fn dynamic_model_string_array(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<String> {
    dynamic_model_value(object, keys)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Keeps provider configuration regression tests in a sibling module so production definitions stay navigable.
/// 将供应商配置回归测试保存在兄弟模块中，让生产定义保持易导航。
#[cfg(test)]
mod tests;

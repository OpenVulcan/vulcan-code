//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use crate::ai_capability_provider_catalog;
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

/// Stable semantic kind for AI-specialized provider or host tools.
/// AI 专用 Provider 或宿主工具使用的稳定语义类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiToolKind {
    /// Web search and browsing-grounded retrieval.
    /// 网页搜索与联网检索增强能力。
    WebSearch,
    /// Image understanding delegated to one external vision-capability provider.
    /// 委派给外部视觉能力提供方的图片理解能力。
    UnderstandImage,
    /// File or knowledge-base search.
    /// 文件或知识库检索能力。
    FileSearch,
    /// Sandboxed code execution.
    /// 沙盒代码执行能力。
    CodeInterpreter,
    /// Image generation or editing.
    /// 图片生成或编辑能力。
    ImageGeneration,
    /// Browser or desktop computer-use control.
    /// 浏览器或桌面计算机控制能力。
    ComputerUse,
    /// Video generation.
    /// 视频生成能力。
    VideoGeneration,
    /// Speech synthesis or text-to-speech generation.
    /// 语音合成或文本转语音能力。
    SpeechSynthesis,
    /// Music generation.
    /// 音乐生成能力。
    MusicGeneration,
    /// Music cover generation based on one reference audio input.
    /// 基于参考音频的音乐翻唱生成能力。
    MusicCover,
    /// Audio generation.
    /// 音频生成能力。
    AudioGeneration,
}

/// Host-owned fallback tool id used by the configured web-search provider chain.
/// 宿主侧配置化网页搜索供应商链使用的回退工具标识。
pub const WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_web_search_chain";

/// Host-owned fallback tool id used by the configured image-understanding provider chain.
/// 宿主侧配置化图片理解供应商链使用的回退工具标识。
pub const UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_understand_image_chain";

/// Host-owned fallback tool id used by the configured image-generation provider chain.
/// 宿主侧配置化图片生成供应商链使用的回退工具标识。
pub const IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_image_generation_chain";

/// Host-owned fallback tool id used by the configured video-generation provider chain.
/// 宿主侧配置化视频生成供应商链使用的回退工具标识。
pub const VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_video_generation_chain";

/// Host-owned fallback tool id used by the configured speech-synthesis provider chain.
/// 宿主侧配置化语音合成供应商链使用的回退工具标识。
pub const SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_speech_synthesis_chain";

/// Host-owned fallback tool id used by the configured music-generation provider chain.
/// 宿主侧配置化音乐生成供应商链使用的回退工具标识。
pub const MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_music_generation_chain";

/// Host-owned fallback tool id used by the configured music-cover provider chain.
/// 宿主侧配置化音乐翻唱供应商链使用的回退工具标识。
pub const MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID: &str = "vulcan_music_cover_chain";

impl AiToolKind {
    /// Returns the stable public semantic key used by diagnostics and configuration.
    /// 返回诊断与配置使用的稳定公开语义键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::UnderstandImage => "understand_image",
            Self::FileSearch => "file_search",
            Self::CodeInterpreter => "code_interpreter",
            Self::ImageGeneration => "image_generation",
            Self::ComputerUse => "computer_use",
            Self::VideoGeneration => "video_generation",
            Self::SpeechSynthesis => "speech_synthesis",
            Self::MusicGeneration => "music_generation",
            Self::MusicCover => "music_cover",
            Self::AudioGeneration => "audio_generation",
        }
    }

    /// Returns the compact Chinese label used by request-preview diagnostics.
    /// 返回请求预览诊断使用的紧凑中文标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::WebSearch => "网页搜索",
            Self::UnderstandImage => "图片理解",
            Self::FileSearch => "文件检索",
            Self::CodeInterpreter => "代码解释器",
            Self::ImageGeneration => "图片生成",
            Self::ComputerUse => "计算机控制",
            Self::VideoGeneration => "视频生成",
            Self::SpeechSynthesis => "语音合成",
            Self::MusicGeneration => "音乐生成",
            Self::MusicCover => "音乐翻唱",
            Self::AudioGeneration => "音频生成",
        }
    }
}

/// User-selected routing mode for the unified web-search capability.
/// 用户为统一网页搜索能力选择的路由模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchRoutingMode {
    /// Only expose provider-native web search; unsupported providers keep the tool inactive.
    /// 仅暴露供应商原生网页搜索；不支持时保持工具未激活。
    NativeOnly,
    /// Prefer provider-native web search, then use the configured host provider chain.
    /// 优先使用供应商原生网页搜索，不支持时使用已配置的宿主供应商链。
    NativeFirst,
    /// Prefer the configured host provider chain, then fall back to provider-native search when no host chain is available.
    /// 优先使用已配置的第三方搜索链；当第三方链不可用时回退到供应商原生搜索。
    #[serde(alias = "provider-first")]
    ThirdPartyFirst,
    /// Only expose the configured host provider chain.
    /// 仅暴露已配置的第三方搜索链。
    #[serde(alias = "provider-only")]
    ThirdPartyOnly,
    /// Disable unified web search completely.
    /// 完全关闭统一网页搜索。
    Disabled,
}

impl WebSearchRoutingMode {
    /// All routing modes shown by the settings picker in stable order.
    /// 设置选择器中按稳定顺序展示的全部路由模式。
    pub const ALL: [Self; 5] = [
        Self::NativeOnly,
        Self::NativeFirst,
        Self::ThirdPartyFirst,
        Self::ThirdPartyOnly,
        Self::Disabled,
    ];

    /// Returns the compact settings label for this routing mode.
    /// 返回当前路由模式在设置界面使用的紧凑标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::NativeOnly => "仅使用供应商原生",
            Self::NativeFirst => "原生优先，第三方兜底",
            Self::ThirdPartyFirst => "第三方优先，原生兜底",
            Self::ThirdPartyOnly => "仅使用第三方搜索",
            Self::Disabled => "彻底关闭 web-search",
        }
    }

    /// Returns one explanatory sentence for settings and diagnostics surfaces.
    /// 返回用于设置与诊断界面的一句解释说明。
    pub const fn description(self) -> &'static str {
        match self {
            Self::NativeOnly => {
                "仅当当前模型声明支持 Provider Hosted Search 时，才在请求中声明启用。"
            }
            Self::NativeFirst => {
                "能用模型原生 Hosted Search 就声明启用；否则使用下方第三方搜索链。"
            }
            Self::ThirdPartyFirst => {
                "优先暴露宿主 web_search；第三方链未配置或不可用时才声明模型原生搜索。"
            }
            Self::ThirdPartyOnly => "只暴露宿主 web_search；第三方搜索未配置时不会启用搜索。",
            Self::Disabled => {
                "不注入统一 web_search，也不声明模型内建搜索；用户可自行通过 Skill 或 MCP 暴露搜索工具。"
            }
        }
    }
}

impl Default for WebSearchRoutingMode {
    /// Returns the conservative default that preserves provider-native behavior.
    /// 返回保守默认值，以保留供应商原生行为。
    fn default() -> Self {
        Self::NativeOnly
    }
}

impl fmt::Display for WebSearchRoutingMode {
    /// Formats one routing mode as a user-facing label.
    /// 将单个路由模式格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Supported external web-search provider families for the host fallback chain.
/// 宿主回退链支持的第三方网页搜索供应商类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchProviderDispatchMode {
    /// Try configured providers in order and stop on the first successful response.
    /// 按配置顺序依次尝试供应商，并在首次成功响应后停止。
    PriorityFailover,
    /// Rotate the starting provider across runs while still allowing per-run fallback to the rest of the chain.
    /// 在多次调用之间轮换起始供应商，同时允许单次调用继续回退到链上其他条目。
    Balanced,
}

#[allow(dead_code)]
impl WebSearchProviderDispatchMode {
    /// All dispatch modes shown by the settings page in stable order.
    /// 设置页面按稳定顺序展示的全部调度模式。
    pub const ALL: [Self; 2] = [Self::PriorityFailover, Self::Balanced];

    /// Returns the compact settings label for the dispatch mode.
    /// 返回当前调度模式在设置界面显示的紧凑标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::PriorityFailover => "顺序优先，失败兜底",
            Self::Balanced => "均衡轮询，失败兜底",
        }
    }

    /// Returns one explanatory sentence for settings and diagnostics surfaces.
    /// 返回用于设置与诊断界面的一句解释说明。
    pub const fn description(self) -> &'static str {
        match self {
            Self::PriorityFailover => "始终从列表顶部开始尝试；当前供应商失败后自动切换到下一个。",
            Self::Balanced => {
                "在多次调用之间轮换起始供应商；单次失败时仍继续尝试链上的其他供应商。"
            }
        }
    }
}

impl Default for WebSearchProviderDispatchMode {
    /// Returns the conservative default dispatch mode.
    /// 返回保守的默认调度模式。
    fn default() -> Self {
        Self::PriorityFailover
    }
}

impl fmt::Display for WebSearchProviderDispatchMode {
    /// Formats one dispatch mode as a user-facing label.
    /// 将单个调度模式格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Supported third-party web-search provider families for the host fallback chain.
/// 宿主回退链支持的第三方网页搜索供应商类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchProviderKind {
    /// DuckDuckGo HTML search without an API key.
    /// 无需 API Key 的 DuckDuckGo HTML 搜索。
    DuckDuckGo,
    /// Brave Search API.
    /// Brave Search API。
    Brave,
    /// Tavily Search API.
    /// Tavily Search API。
    Tavily,
    /// Exa Search API.
    /// Exa Search API。
    Exa,
    /// Firecrawl Search API.
    /// Firecrawl Search API。
    Firecrawl,
    /// Gemini grounding search via Google Gemini API.
    /// 通过 Google Gemini API 调用的 Gemini 搜索。
    Gemini,
    /// Grok web-grounded search via xAI.
    /// 通过 xAI 调用的 Grok 联网搜索。
    Grok,
    /// Kimi web search via Moonshot chat-completions.
    /// 通过 Moonshot chat-completions 调用的 Kimi 联网搜索。
    Kimi,
    /// MiniMax search endpoint backed by the Token Plan key and credits system.
    /// 由 Token Plan Key 与 Credits 体系承载的 MiniMax 搜索端点。
    MiniMax,
    /// Ollama web search endpoint exposed by the local or cloud host.
    /// 由本地或云端 Ollama 宿主暴露的联网搜索端点。
    Ollama,
    /// Perplexity structured search endpoint.
    /// Perplexity 结构化搜索端点。
    Perplexity,
    /// SearXNG self-hosted meta search.
    /// SearXNG 自托管聚合搜索。
    Searxng,
    /// SerpAPI Google search endpoint.
    /// SerpAPI Google 搜索端点。
    SerpApi,
}

/// Supported third-party image-understanding provider families for the standalone capability layer.
/// 独立能力层支持的第三方图片理解提供方类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnderstandImageProviderKind {
    /// MiniMax VLM image-understanding endpoint.
    /// MiniMax VLM 图片理解端点。
    MiniMax,
}

#[allow(dead_code)]
impl UnderstandImageProviderKind {
    /// All supported image-understanding provider kinds in stable persistence order.
    /// 全部受支持的图片理解提供方类型，按稳定持久化顺序排列。
    pub const ALL: [Self; 1] = [Self::MiniMax];

    /// Returns the stable provider kind key used for persistence and target routing.
    /// 返回持久化与目标路由使用的稳定提供方类型键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::MiniMax => "minimax",
        }
    }

    /// Returns the resource-backed provider definition associated with the current kind.
    /// 返回与当前类型关联的资源化提供方定义。
    fn definition(
        self,
    ) -> &'static ai_capability_provider_catalog::UnderstandImageProviderDefinition {
        ai_capability_provider_catalog::understand_image_provider_definition_by_id(self.key())
            .unwrap_or_else(|| {
                panic!(
                    "missing understand-image provider definition for `{}`",
                    self.key()
                )
            })
    }

    /// Returns the user-facing provider label.
    /// 返回面向用户的提供方标签。
    pub fn label(self) -> &'static str {
        self.definition().name.as_str()
    }

    /// Returns the stable default target alias exposed below one configured provider instance.
    /// 返回暴露在单个配置提供方实例下方的稳定默认目标别名。
    pub fn default_target_alias(self) -> &'static str {
        self.definition().target_alias.as_str()
    }

    /// Returns the default endpoint used when the user leaves the base URL empty.
    /// 返回用户留空基址时使用的默认端点。
    pub fn default_base_url(self) -> &'static str {
        self.definition().default_base_url.as_str()
    }

    /// Returns the label shown above the API key field.
    /// 返回显示在 API Key 字段上方的标签。
    pub fn api_key_label(self) -> &'static str {
        self.definition().api_key_label.as_str()
    }

    /// Returns the placeholder shown inside the API key field.
    /// 返回显示在 API Key 字段中的占位提示。
    pub fn api_key_placeholder(self) -> &'static str {
        self.definition().api_key_placeholder.as_str()
    }

    /// Returns whether the provider exposes one configurable region field.
    /// 返回当前提供方是否暴露可配置区域字段。
    pub fn supports_region_field(self) -> bool {
        self.definition().supports_region_field
    }

    /// Returns the label shown above the region field.
    /// 返回显示在区域字段上方的标签。
    pub fn region_label(self) -> &'static str {
        self.definition().region_label.as_str()
    }

    /// Returns the placeholder shown inside the region field.
    /// 返回显示在区域字段中的占位提示。
    pub fn region_placeholder(self) -> &'static str {
        self.definition().region_placeholder.as_str()
    }

    /// Returns the default region value suggested for the provider.
    /// 返回为当前提供方建议的默认区域值。
    pub fn default_region(self) -> &'static str {
        self.definition().default_region.as_str()
    }

    /// Returns one provider-specific configuration note shown in the settings UI.
    /// 返回显示在设置界面中的提供方专属配置说明。
    pub fn configuration_note(self) -> Option<&'static str> {
        self.definition().configuration_note.as_deref()
    }
}

impl fmt::Display for UnderstandImageProviderKind {
    /// Formats one provider kind as a user-facing label.
    /// 将单个提供方类型格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[allow(dead_code)]
impl WebSearchProviderKind {
    /// All supported third-party web-search provider kinds in stable persistence order.
    /// 全部受支持的第三方网页搜索提供方类型，按稳定持久化顺序排列。
    pub const ALL: [Self; 13] = [
        Self::DuckDuckGo,
        Self::Brave,
        Self::Tavily,
        Self::Exa,
        Self::Firecrawl,
        Self::Gemini,
        Self::Grok,
        Self::Kimi,
        Self::MiniMax,
        Self::Ollama,
        Self::Perplexity,
        Self::Searxng,
        Self::SerpApi,
    ];

    /// Returns the stable provider kind key used for generated ids.
    /// 返回生成标识使用的稳定供应商类型键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckduckgo",
            Self::Brave => "brave",
            Self::Tavily => "tavily",
            Self::Exa => "exa",
            Self::Firecrawl => "firecrawl",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::MiniMax => "minimax",
            Self::Ollama => "ollama",
            Self::Perplexity => "perplexity",
            Self::Searxng => "searxng",
            Self::SerpApi => "serpapi",
        }
    }

    /// Resolves one provider kind from its stable persisted key.
    /// 根据稳定持久化键解析单个提供方类型。
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Returns the resource-backed provider definition associated with the current kind.
    /// 返回与当前类型关联的资源化提供方定义。
    fn definition(self) -> &'static ai_capability_provider_catalog::WebSearchProviderDefinition {
        ai_capability_provider_catalog::web_search_provider_definition_by_id(self.key())
            .unwrap_or_else(|| {
                panic!(
                    "missing web-search provider definition for `{}`",
                    self.key()
                )
            })
    }

    /// Returns the user-facing provider name.
    /// 返回面向用户的供应商名称。
    pub fn label(self) -> &'static str {
        self.definition().name.as_str()
    }

    /// Returns the stable default target alias exposed beneath one concrete provider instance.
    /// 返回暴露在单个具体供应商实例下方的稳定默认目标别名。
    pub fn default_target_alias(self) -> &'static str {
        self.definition().target_alias.as_str()
    }

    /// Returns the default base URL used when the user leaves the field empty.
    /// 返回用户留空接口地址时使用的默认基址。
    pub fn default_base_url(self) -> &'static str {
        self.definition().default_base_url.as_str()
    }

    /// Returns whether the provider requires a non-empty API key before it can be used.
    /// 返回当前供应商在使用前是否要求填写非空 API Key。
    pub fn requires_api_key(self) -> bool {
        self.definition().api_key_requirement.requires_api_key()
    }

    /// Returns whether the provider can optionally use an API key even when it is not mandatory.
    /// 返回当前供应商是否允许在非强制前提下可选填写 API Key。
    pub fn api_key_optional(self) -> bool {
        self.definition().api_key_requirement.api_key_optional()
    }

    /// Returns whether the provider exposes one configurable model field.
    /// 返回当前供应商是否暴露可配置模型字段。
    pub fn supports_model_field(self) -> bool {
        self.definition().supports_model_field
    }

    /// Returns whether the provider exposes one configurable region-like field.
    /// 返回当前供应商是否暴露可配置区域类字段。
    pub fn supports_region_field(self) -> bool {
        self.definition().supports_region_field
    }

    /// Returns the user-facing label shown above the API key field.
    /// 返回显示在 API Key 字段上方的面向用户标签。
    pub fn api_key_label(self) -> &'static str {
        self.definition().api_key_label.as_str()
    }

    /// Returns the placeholder shown inside the API key field.
    /// 返回显示在 API Key 字段内的占位提示。
    pub fn api_key_placeholder(self) -> &'static str {
        self.definition().api_key_placeholder.as_str()
    }

    /// Returns the default model id suggested for model-capable providers.
    /// 返回为支持模型字段的供应商建议的默认模型标识。
    pub fn default_model(self) -> &'static str {
        self.definition().default_model.as_str()
    }

    /// Returns the placeholder shown inside the model field.
    /// 返回显示在模型字段内的占位提示。
    pub fn model_placeholder(self) -> &'static str {
        self.definition().model_placeholder.as_str()
    }

    /// Returns the label shown above the model field.
    /// 返回显示在模型字段上方的标签。
    pub fn model_label(self) -> &'static str {
        self.definition().model_label.as_str()
    }

    /// Returns the default region or locale hint for region-aware providers.
    /// 返回支持区域字段的供应商使用的默认区域或地区提示。
    pub fn default_region(self) -> &'static str {
        self.definition().default_region.as_str()
    }

    /// Returns the placeholder shown inside the region field.
    /// 返回显示在区域字段内的占位提示。
    pub fn region_placeholder(self) -> &'static str {
        self.definition().region_placeholder.as_str()
    }

    /// Returns the label shown above the region field.
    /// 返回显示在区域字段上方的标签。
    pub fn region_label(self) -> &'static str {
        self.definition().region_label.as_str()
    }

    /// Returns one provider-specific configuration hint shown in the settings UI when extra clarification is helpful.
    /// 返回在设置界面需要额外澄清时展示的供应商专属配置提示。
    pub fn configuration_note(self) -> Option<&'static str> {
        self.definition().configuration_note.as_deref()
    }

    /// Returns the per-provider hard result-count ceiling.
    /// 返回当前供应商允许的结果数量硬上限。
    pub fn max_results_limit(self) -> usize {
        self.definition().max_results_limit
    }
}

impl fmt::Display for WebSearchProviderKind {
    /// Formats one provider kind as a user-facing name.
    /// 将单个供应商类型格式化为面向用户的名称。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One configured external web-search provider entry in the fallback chain.
/// 回退链中的单个外部网页搜索供应商配置项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWebSearchProviderEntry {
    /// Stable entry id used by the settings UI.
    /// 设置界面使用的稳定条目标识。
    pub id: String,
    /// User-facing display name.
    /// 面向用户展示的显示名称。
    pub name: String,
    /// Provider implementation kind.
    /// 供应商实现类型。
    pub kind: WebSearchProviderKind,
    /// Stable target alias exposed below the current vendor target.
    /// 当前供应商目标下方暴露的稳定目标别名。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_alias: String,
    /// Whether this entry participates in the runtime chain.
    /// 当前条目是否参与运行时搜索链。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// API key or `{env:NAME}` placeholder.
    /// API Key 或 `{env:NAME}` 环境变量占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Optional endpoint override.
    /// 可选的接口地址覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Optional provider-specific model override.
    /// 可选的供应商专属模型覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Optional provider-specific region or locale override.
    /// 可选的供应商专属区域或地区覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    /// Maximum result count requested from this provider.
    /// 向当前供应商请求的最大结果数量。
    #[serde(default = "default_web_search_max_results")]
    pub max_results: usize,
}

/// One configured external image-understanding provider entry in the standalone capability layer.
/// 独立能力层中的单个外部图片理解提供方配置项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUnderstandImageProviderEntry {
    /// Stable entry id used by the settings UI.
    /// 设置界面使用的稳定条目标识。
    pub id: String,
    /// User-facing display name.
    /// 面向用户展示的显示名称。
    pub name: String,
    /// Provider implementation kind.
    /// 提供方实现类型。
    pub kind: UnderstandImageProviderKind,
    /// Stable target alias exposed below the current vendor target.
    /// 当前提供方目标下方暴露的稳定目标别名。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_alias: String,
    /// Whether this entry participates in the runtime chain.
    /// 当前条目是否参与运行时能力链。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// API key or `{env:NAME}` placeholder.
    /// API Key 或 `{env:NAME}` 环境变量占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Optional endpoint override.
    /// 可选的接口地址覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Optional provider-specific region override.
    /// 可选的提供方专属区域覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
}

#[allow(dead_code)]
impl PersistedUnderstandImageProviderEntry {
    /// Builds one default provider entry with a stable identifier supplied by the caller.
    /// 使用调用方提供的稳定标识构建默认提供方条目。
    pub fn new(id: impl Into<String>, kind: UnderstandImageProviderKind) -> Self {
        Self {
            id: id.into(),
            name: kind.label().to_string(),
            kind,
            target_alias: kind.default_target_alias().to_string(),
            enabled: true,
            api_key: String::new(),
            base_url: String::new(),
            region: kind.default_region().to_string(),
        }
    }

    /// Returns the stable instance alias used by semantic `target` selection.
    /// 返回语义 `target` 选择使用的稳定实例别名。
    pub fn instance_alias(&self) -> &str {
        self.kind.key()
    }

    /// Returns the stable target alias exposed by this provider instance.
    /// 返回当前提供方实例暴露的稳定目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.kind.default_target_alias()
        } else {
            trimmed
        }
    }

    /// Returns the fully qualified `instance/target` selector exposed to the model.
    /// 返回暴露给模型使用的完整 `实例/目标` 选择器。
    pub fn qualified_target(&self) -> String {
        format!("{}/{}", self.instance_alias(), self.target_alias())
    }

    /// Returns whether one user-specified target selector resolves to this provider instance.
    /// 返回某个用户指定的目标选择器是否解析到当前提供方实例。
    pub fn matches_target(&self, target: &str) -> bool {
        let normalized = target.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }

        normalized == self.instance_alias().to_ascii_lowercase()
            || normalized == self.qualified_target().to_ascii_lowercase()
    }

    /// Returns the resolved endpoint, falling back to the provider default.
    /// 返回解析后的接口地址，留空时回退到提供方默认地址。
    pub fn resolved_base_url(&self) -> String {
        let trimmed = self.base_url.trim();
        if trimmed.is_empty() {
            self.kind.default_base_url().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns the resolved provider region, falling back to the provider default when available.
    /// 返回解析后的提供方区域；若存在默认值则在留空时自动回退。
    pub fn resolved_region(&self) -> String {
        let trimmed = self.region.trim();
        if trimmed.is_empty() {
            self.kind.default_region().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns whether this entry is currently usable by the runtime chain.
    /// 返回当前条目是否可被运行时能力链使用。
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.api_key.trim().is_empty()
    }

    /// Resolves the stored API key into a runtime-ready credential string.
    /// 将已保存的 API Key 解析为运行时可直接使用的凭据字符串。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), self.kind.api_key_label())
    }
}

#[allow(dead_code)]
impl PersistedWebSearchProviderEntry {
    /// Builds one default provider entry with a unique id supplied by the caller.
    /// 使用调用方提供的唯一标识构建默认供应商条目。
    pub fn new(id: impl Into<String>, kind: WebSearchProviderKind) -> Self {
        Self {
            id: id.into(),
            name: kind.label().to_string(),
            kind,
            target_alias: kind.default_target_alias().to_string(),
            enabled: true,
            api_key: String::new(),
            base_url: String::new(),
            model: kind.default_model().to_string(),
            region: kind.default_region().to_string(),
            max_results: default_web_search_max_results(),
        }
    }

    /// Returns the stable instance alias used by semantic `target` selection.
    /// 返回语义 `target` 选择使用的稳定实例别名。
    pub fn instance_alias(&self) -> &str {
        self.kind.key()
    }

    /// Returns the stable target alias exposed by this search-provider instance.
    /// 返回当前搜索供应商实例暴露的稳定目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.kind.default_target_alias()
        } else {
            trimmed
        }
    }

    /// Returns the fully qualified `instance/target` selector exposed to the model.
    /// 返回暴露给模型使用的完整 `实例/目标` 选择器。
    pub fn qualified_target(&self) -> String {
        format!("{}/{}", self.instance_alias(), self.target_alias())
    }

    /// Returns whether one user-specified target selector resolves to this provider instance.
    /// 返回某个用户指定的目标选择器是否解析到当前供应商实例。
    pub fn matches_target(&self, target: &str) -> bool {
        let normalized = target.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }

        normalized == self.instance_alias().to_ascii_lowercase()
            || normalized == self.qualified_target().to_ascii_lowercase()
    }

    /// Returns the resolved endpoint, falling back to the provider default.
    /// 返回解析后的接口地址，留空时回退到供应商默认地址。
    pub fn resolved_base_url(&self) -> String {
        let trimmed = self.base_url.trim();
        if trimmed.is_empty() {
            self.kind.default_base_url().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns whether this entry is currently usable by the runtime chain.
    /// 返回当前条目是否可被运行时搜索链使用。
    pub fn is_usable(&self) -> bool {
        self.enabled
            && (!self.kind.requires_api_key() || !self.api_key.trim().is_empty())
            && match self.kind {
                WebSearchProviderKind::Searxng | WebSearchProviderKind::Ollama => {
                    !self.resolved_base_url().trim().is_empty()
                }
                _ => true,
            }
    }

    /// Returns the resolved provider model, falling back to the provider default when available.
    /// 返回解析后的供应商模型；若存在默认模型则在留空时自动回退。
    pub fn resolved_model(&self) -> String {
        let trimmed = self.model.trim();
        if trimmed.is_empty() {
            self.kind.default_model().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns the resolved provider region or locale, falling back to the provider default when available.
    /// 返回解析后的供应商区域或地区；若存在默认值则在留空时自动回退。
    pub fn resolved_region(&self) -> String {
        let trimmed = self.region.trim();
        if trimmed.is_empty() {
            self.kind.default_region().to_string()
        } else {
            trimmed.to_string()
        }
    }
}

/// One enabled capability target bound under a single AI capability vendor instance.
/// 绑定在单个 AI 能力供应商实例下的一条已启用能力目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityTargetBinding {
    /// Stable semantic capability kind owned by the binding.
    /// 当前绑定拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable target alias exposed to the model beneath the vendor id.
    /// 在供应商标识下暴露给模型的稳定目标别名。
    pub target_alias: String,
    /// Provider request model id or provider-specific target value.
    /// Provider 请求模型标识或 Provider 专属目标值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_id: String,
    /// Whether the current binding is enabled.
    /// 当前绑定是否启用。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

#[allow(dead_code)]
impl PersistedAiCapabilityTargetBinding {
    /// Builds one default enabled target binding for the provided capability and target pair.
    /// 为给定能力与目标对构建一条默认启用的目标绑定。
    pub fn new(
        capability_kind: AiToolKind,
        target_alias: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        let target_alias = target_alias.into();
        let model_id = model_id.into();
        Self {
            capability_kind,
            target_alias: target_alias.trim().to_string(),
            model_id: if model_id.trim().is_empty() {
                target_alias.trim().to_string()
            } else {
                model_id.trim().to_string()
            },
            enabled: true,
        }
    }

    /// Returns the canonical binding key used by deduplication and route-order bookkeeping.
    /// 返回供去重与路由顺序记账使用的规范绑定键。
    pub fn binding_key(&self) -> String {
        format!("{}::{}", self.capability_kind.key(), self.target_alias())
    }

    /// Returns the effective target alias exposed to the model.
    /// 返回暴露给模型的生效目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.model_id()
        } else {
            trimmed
        }
    }

    /// Returns the provider request model identifier attached to the binding.
    /// 返回绑定携带的 Provider 请求模型标识。
    pub fn model_id(&self) -> &str {
        let trimmed = self.model_id.trim();
        if trimmed.is_empty() {
            self.target_alias.trim()
        } else {
            trimmed
        }
    }
}

/// One shared AI capability vendor instance configured once and reused across multiple semantic capabilities.
/// 单次配置并可在多个语义能力之间复用的一条共享 AI 能力供应商实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityVendorEntry {
    /// Stable vendor instance id used by settings, routing, and queue serialization.
    /// 设置、路由与队列序列化使用的稳定供应商实例标识。
    pub id: String,
    /// Stable vendor catalog identifier such as `minimax` or `tavily`.
    /// 稳定供应商目录标识，例如 `minimax` 或 `tavily`。
    pub vendor_id: String,
    /// User-facing vendor instance name.
    /// 面向用户展示的供应商实例名称。
    pub name: String,
    /// Whether the current vendor instance participates in runtime routing.
    /// 当前供应商实例是否参与运行时路由。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Shared API key or `{env:NAME}` placeholder reused by all enabled targets beneath the vendor.
    /// 在该供应商下所有已启用目标之间复用的共享 API Key 或 `{env:NAME}` 占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Shared vendor-level base URL or root endpoint override.
    /// 共享的供应商级 Base URL 或根端点覆盖值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Shared region or locale override reused by region-aware targets beneath the vendor.
    /// 在该供应商下支持区域的目标之间复用的共享区域或地区覆盖值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    /// Enabled capability targets exposed by the current vendor instance.
    /// 当前供应商实例暴露的已启用能力目标集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<PersistedAiCapabilityTargetBinding>,
}

#[allow(dead_code)]
impl PersistedAiCapabilityVendorEntry {
    /// Builds one default vendor instance that can later enable one or more capability targets.
    /// 构建一条后续可启用一个或多个能力目标的默认供应商实例。
    pub fn new(vendor_id: impl Into<String>) -> Self {
        let vendor_id = normalize_ai_capability_vendor_id(vendor_id.into());
        let vendor_name = default_ai_capability_vendor_name(&vendor_id);
        Self {
            id: vendor_id.clone(),
            vendor_id,
            name: vendor_name,
            enabled: true,
            api_key: String::new(),
            base_url: String::new(),
            region: default_ai_capability_vendor_region(""),
            targets: Vec::new(),
        }
    }

    /// Returns whether the current vendor instance has at least one enabled target for the requested capability.
    /// 返回当前供应商实例是否至少为所请求能力启用了一条目标。
    pub fn has_enabled_target_for(&self, capability_kind: AiToolKind) -> bool {
        self.enabled
            && self
                .targets
                .iter()
                .any(|binding| binding.enabled && binding.capability_kind == capability_kind)
    }

    /// Returns the fully qualified `vendor/target` selectors enabled for one capability.
    /// 返回单个能力当前启用的完整 `供应商/目标` 选择器集合。
    pub fn enabled_target_strings_for(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.targets
            .iter()
            .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
            .map(|binding| format!("{}/{}", self.vendor_id, binding.target_alias()))
            .collect()
    }

    /// Returns the first enabled binding that matches the provided capability and target selector.
    /// 返回首条与给定能力及目标选择器匹配的已启用绑定。
    pub fn matching_binding(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<&PersistedAiCapabilityTargetBinding> {
        let normalized_target = target.trim().to_ascii_lowercase();
        self.targets.iter().find(|binding| {
            binding.enabled
                && binding.capability_kind == capability_kind
                && format!("{}/{}", self.vendor_id, binding.target_alias()).to_ascii_lowercase()
                    == normalized_target
        })
    }

    /// Resolves the shared API key for the current vendor instance, honoring `{env:NAME}` placeholders.
    /// 解析当前供应商实例的共享 API Key，并支持 `{env:NAME}` 环境变量占位符。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), "AI 能力供应商 API Key")
    }

    /// Returns the effective region value for the current vendor instance.
    /// 返回当前供应商实例的生效区域值。
    pub fn resolved_region(&self) -> String {
        self.region.trim().to_string()
    }

    /// Normalizes one vendor entry so it has a stable identifier, name, and deduplicated target list.
    /// 归一化单个供应商条目，使其拥有稳定标识、名称与去重后的目标列表。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let normalized_vendor_id = normalize_ai_capability_vendor_id(self.vendor_id.clone());
        self.vendor_id = normalized_vendor_id.clone();
        self.id = normalized_vendor_id.clone();
        if self.name.trim().is_empty() {
            self.name = default_ai_capability_vendor_name(&normalized_vendor_id);
        }
        if self.region.trim().is_empty() {
            self.region = default_ai_capability_vendor_region(&normalized_vendor_id);
        }
        if normalized_vendor_id == "minimax" {
            self.base_url = normalize_minimax_vendor_base_url(&self.base_url);
        }

        let mut normalized_targets = Vec::new();
        for mut binding in self.targets.drain(..) {
            binding.target_alias = binding.target_alias().to_string();
            binding.model_id = binding.model_id().to_string();
            if normalized_targets
                .iter()
                .any(|existing: &PersistedAiCapabilityTargetBinding| {
                    existing.capability_kind == binding.capability_kind
                        && existing
                            .target_alias()
                            .eq_ignore_ascii_case(binding.target_alias())
                })
            {
                continue;
            }
            normalized_targets.push(binding);
        }
        self.targets = normalized_targets;
        self != &original
    }
}

/// Persisted configuration for the unified web-search capability.
/// 统一网页搜索能力的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWebSearchConfiguration {
    /// User-selected web-search routing mode.
    /// 用户选择的网页搜索路由模式。
    #[serde(default)]
    pub mode: WebSearchRoutingMode,
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    /// Legacy ordered external provider fallback chain kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版按顺序排列的外部供应商回退链。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedWebSearchProviderEntry>,
    /// Legacy dispatch mode kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版调度模式。
    #[serde(default, skip_serializing_if = "is_default")]
    pub dispatch_mode: WebSearchProviderDispatchMode,
    /// Default result-count hint used when the model does not provide an explicit `max_results`.
    /// 当模型未显式提供 `max_results` 时使用的默认结果数量提示。
    #[serde(default = "default_web_search_max_results")]
    pub max_results: usize,
}

/// Persisted configuration for the standalone image-understanding capability.
/// 独立图片理解能力的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUnderstandImageConfiguration {
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    /// Legacy ordered external provider list kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版按顺序排列的外部提供方列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedUnderstandImageProviderEntry>,
}

/// Persisted route-order configuration shared by host-owned AI capability tools that only need ordered `vendor/target` selectors.
/// 仅需要有序 `供应商/目标` 选择器的宿主 AI 能力工具共享的持久化路由顺序配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityRouteConfiguration {
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
}

#[allow(dead_code)]
impl PersistedAiCapabilityRouteConfiguration {
    /// Returns whether the route-order configuration still equals the built-in default.
    /// 返回路由顺序配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the persisted route-order array into one deduplicated selector list.
    /// 将持久化路由顺序数组归一化为去重后的选择器列表。
    pub fn normalize(&mut self) -> bool {
        let original = self.target_order.clone();
        self.target_order = normalize_route_target_order(&self.target_order);
        self.target_order != original
    }
}

#[allow(dead_code)]
impl PersistedUnderstandImageConfiguration {
    /// Returns whether the image-understanding configuration still equals the built-in default.
    /// 返回图片理解配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Returns whether at least one configured provider can currently be used.
    /// 返回当前是否至少存在一个可用的已配置提供方。
    pub fn has_usable_provider_chain(&self) -> bool {
        !self.target_order.is_empty() || !self.providers.is_empty()
    }

    /// Returns the ordered list of currently usable semantic image-understanding targets.
    /// 返回当前可用语义图片理解目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.target_order.clone()
    }

    /// Normalizes persisted provider rows so each provider kind appears at most once with one stable identifier.
    /// 归一化持久化提供方行，确保每种提供方类型至多出现一次且拥有稳定标识。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let mut normalized = Vec::new();

        for mut provider in self.providers.drain(..) {
            if normalized
                .iter()
                .any(|existing: &PersistedUnderstandImageProviderEntry| {
                    existing.kind == provider.kind
                })
            {
                continue;
            }

            provider.id = provider.kind.key().to_string();
            if provider.target_alias.trim().is_empty() {
                provider.target_alias = provider.kind.default_target_alias().to_string();
            }
            if provider.name.trim().is_empty() {
                provider.name = provider.kind.label().to_string();
            }
            normalized.push(provider);
        }

        self.providers = normalized;
        self.target_order = normalize_route_target_order(&self.target_order);
        self != &original
    }
}

impl Default for PersistedWebSearchConfiguration {
    /// Builds the default web-search configuration.
    /// 构建默认网页搜索配置。
    fn default() -> Self {
        Self {
            mode: WebSearchRoutingMode::default(),
            target_order: Vec::new(),
            providers: Vec::new(),
            dispatch_mode: WebSearchProviderDispatchMode::default(),
            max_results: default_web_search_max_results(),
        }
    }
}

#[allow(dead_code)]
impl PersistedWebSearchConfiguration {
    /// Returns whether the web-search configuration still equals the built-in default.
    /// 返回网页搜索配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Returns whether at least one configured provider can be used.
    /// 返回是否至少存在一个可用的已配置供应商。
    pub fn has_usable_provider_chain(&self) -> bool {
        !self.target_order.is_empty() || !self.providers.is_empty()
    }

    /// Normalizes persisted provider rows so each provider kind appears at most once with one stable identifier.
    /// 归一化持久化提供方行，确保每种提供方类型至多出现一次且拥有稳定标识。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let mut normalized = Vec::new();

        for mut provider in self.providers.drain(..) {
            if normalized
                .iter()
                .any(|existing: &PersistedWebSearchProviderEntry| existing.kind == provider.kind)
            {
                continue;
            }

            provider.id = provider.kind.key().to_string();
            if provider.target_alias.trim().is_empty() {
                provider.target_alias = provider.kind.default_target_alias().to_string();
            }
            if provider.name.trim().is_empty() {
                provider.name = provider.kind.label().to_string();
            }
            normalized.push(provider);
        }

        self.providers = normalized;
        self.target_order = normalize_route_target_order(&self.target_order);
        if self.max_results == 0 {
            self.max_results = default_web_search_max_results();
        }
        self != &original
    }

    /// Returns the ordered list of currently usable semantic web-search targets.
    /// 返回当前可用语义网页搜索目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.target_order.clone()
    }
}

/// Root persisted API-native tool configuration.
/// 根级持久化 API 原生工具配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiToolConfiguration {
    /// Shared AI capability vendor instances configured once and reused across multiple semantic capabilities.
    /// 单次配置并可在多个语义能力之间复用的共享 AI 能力供应商实例集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vendors: Vec<PersistedAiCapabilityVendorEntry>,
    /// Unified web-search configuration.
    /// 统一网页搜索配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedWebSearchConfiguration::is_default"
    )]
    pub web_search: PersistedWebSearchConfiguration,
    /// Standalone image-understanding configuration.
    /// 独立图片理解配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedUnderstandImageConfiguration::is_default"
    )]
    pub understand_image: PersistedUnderstandImageConfiguration,
    /// Standalone image-generation configuration.
    /// 独立图片生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub image_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone video-generation configuration.
    /// 独立视频生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub video_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone speech-synthesis configuration.
    /// 独立语音合成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub speech_synthesis: PersistedAiCapabilityRouteConfiguration,
    /// Standalone music-generation configuration.
    /// 独立音乐生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub music_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone music-cover configuration.
    /// 独立音乐翻唱配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub music_cover: PersistedAiCapabilityRouteConfiguration,
}

#[allow(dead_code)]
impl PersistedAiToolConfiguration {
    /// Returns whether all API-native tool settings are still defaults.
    /// 返回全部 API 原生工具设置是否仍为默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes nested capability-provider sections into their canonical standalone shape.
    /// 将嵌套能力提供方分区归一化为其规范独立形态。
    pub fn normalize(&mut self) -> bool {
        let web_search_changed = self.web_search.normalize();
        let understand_image_changed = self.understand_image.normalize();
        let image_generation_changed = self.image_generation.normalize();
        let video_generation_changed = self.video_generation.normalize();
        let speech_synthesis_changed = self.speech_synthesis.normalize();
        let music_generation_changed = self.music_generation.normalize();
        let music_cover_changed = self.music_cover.normalize();
        let migration_changed = self.migrate_legacy_capability_entries_into_vendors();
        let vendor_changed = normalize_ai_capability_vendors(&mut self.vendors);
        let route_changed = self.normalize_capability_route_orders();
        web_search_changed
            || understand_image_changed
            || image_generation_changed
            || video_generation_changed
            || speech_synthesis_changed
            || music_generation_changed
            || music_cover_changed
            || migration_changed
            || vendor_changed
            || route_changed
    }

    /// Returns the ordered web-search provider entries projected from the vendor-centric configuration.
    /// 返回从供应商中心配置投影出的有序网页搜索供应商条目。
    pub fn projected_web_search_providers(&self) -> Vec<PersistedWebSearchProviderEntry> {
        self.projected_target_order_for_kind(AiToolKind::WebSearch)
            .into_iter()
            .filter_map(|target| self.projected_web_search_provider_for_target(&target))
            .collect()
    }

    /// Returns the ordered image-understanding provider entries projected from the vendor-centric configuration.
    /// 返回从供应商中心配置投影出的有序图片理解提供方条目。
    pub fn projected_understand_image_providers(
        &self,
    ) -> Vec<PersistedUnderstandImageProviderEntry> {
        self.projected_target_order_for_kind(AiToolKind::UnderstandImage)
            .into_iter()
            .filter_map(|target| self.projected_understand_image_provider_for_target(&target))
            .collect()
    }

    /// Returns whether at least one enabled vendor target currently exists for the requested semantic capability.
    /// 返回所请求语义能力当前是否至少存在一条已启用供应商目标。
    pub fn has_usable_targets_for_kind(&self, capability_kind: AiToolKind) -> bool {
        !self
            .projected_target_order_for_kind(capability_kind)
            .is_empty()
    }

    /// Returns the ordered list of enabled fully qualified `vendor/target` selectors for one semantic capability.
    /// 返回单个语义能力当前启用的完整 `供应商/目标` 选择器有序列表。
    pub fn usable_targets_for_kind(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.projected_target_order_for_kind(capability_kind)
    }

    /// Returns the persisted route-order slice configured for one semantic capability before enabled targets are reconciled.
    /// 返回在对齐启用目标前为某个语义能力配置的持久化路由顺序切片。
    pub fn configured_target_order_for_kind(&self, capability_kind: AiToolKind) -> &[String] {
        match capability_kind {
            AiToolKind::WebSearch => &self.web_search.target_order,
            AiToolKind::UnderstandImage => &self.understand_image.target_order,
            AiToolKind::ImageGeneration => &self.image_generation.target_order,
            AiToolKind::VideoGeneration => &self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &self.music_generation.target_order,
            AiToolKind::MusicCover => &self.music_cover.target_order,
            _ => &EMPTY_TARGET_ORDER,
        }
    }

    /// Reorders one fully qualified capability target before another target or to the tail inside the persisted route preference list.
    /// 将单条完整能力目标重排到另一目标之前，或重排到持久化路由偏好列表尾部。
    pub fn reorder_target_for_kind(
        &mut self,
        capability_kind: AiToolKind,
        dragged_target: &str,
        before_target: Option<&str>,
    ) -> bool {
        let route_order = match capability_kind {
            AiToolKind::WebSearch => &mut self.web_search.target_order,
            AiToolKind::UnderstandImage => &mut self.understand_image.target_order,
            AiToolKind::ImageGeneration => &mut self.image_generation.target_order,
            AiToolKind::VideoGeneration => &mut self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &mut self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &mut self.music_generation.target_order,
            AiToolKind::MusicCover => &mut self.music_cover.target_order,
            _ => return false,
        };
        let normalized_dragged_target = dragged_target.trim().to_ascii_lowercase();
        let Some(current_index) = route_order
            .iter()
            .position(|existing| existing.eq_ignore_ascii_case(&normalized_dragged_target))
        else {
            return false;
        };
        let mut target_index = if let Some(before_target) = before_target {
            let normalized_before_target = before_target.trim().to_ascii_lowercase();
            let Some(index) = route_order
                .iter()
                .position(|existing| existing.eq_ignore_ascii_case(&normalized_before_target))
            else {
                return false;
            };
            index
        } else {
            route_order.len()
        };
        let entry = route_order.remove(current_index);
        if current_index < target_index {
            target_index = target_index.saturating_sub(1);
        }
        if target_index == current_index {
            route_order.insert(current_index, entry);
            return false;
        }
        let bounded_target_index = target_index.min(route_order.len());
        route_order.insert(bounded_target_index, entry);
        true
    }

    /// Returns the ordered route entries rendered in capability settings pages for one semantic capability.
    /// 返回某个语义能力在设置页面中渲染使用的有序路由条目集合。
    pub fn resolved_route_targets_for_kind(
        &self,
        capability_kind: AiToolKind,
    ) -> Vec<ResolvedAiCapabilityRouteTarget> {
        self.projected_target_order_for_kind(capability_kind)
            .into_iter()
            .filter_map(|target| self.resolve_route_target_for_kind(capability_kind, &target))
            .collect()
    }

    /// Returns the vendor instance matching the provided identifier when one exists.
    /// 返回与给定标识匹配的供应商实例（若存在）。
    pub fn vendor_by_id(&self, vendor_id: &str) -> Option<&PersistedAiCapabilityVendorEntry> {
        let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
        self.vendors
            .iter()
            .find(|vendor| vendor.vendor_id == normalized_vendor_id)
    }

    /// Returns the mutable vendor instance matching the provided identifier when one exists.
    /// 返回与给定标识匹配的可变供应商实例（若存在）。
    pub fn vendor_by_id_mut(
        &mut self,
        vendor_id: &str,
    ) -> Option<&mut PersistedAiCapabilityVendorEntry> {
        let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
        self.vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == normalized_vendor_id)
    }

    /// Migrates legacy capability-scoped provider rows into the shared vendor list, merging shared credentials when possible.
    /// 将旧版按能力分散的提供方行迁移到共享供应商列表中，并在可能时合并共享凭据。
    fn migrate_legacy_capability_entries_into_vendors(&mut self) -> bool {
        let original = self.clone();

        for provider in self.web_search.providers.clone() {
            self.upsert_vendor_target_from_web_search_provider(provider);
        }
        for provider in self.understand_image.providers.clone() {
            self.upsert_vendor_target_from_understand_image_provider(provider);
        }
        if !self.web_search.providers.is_empty() {
            self.web_search.providers.clear();
        }
        if !self.understand_image.providers.is_empty() {
            self.understand_image.providers.clear();
        }

        self != &original
    }

    /// Ensures route-order arrays stay aligned with the currently enabled vendor targets.
    /// 确保路由顺序数组始终与当前已启用的供应商目标保持对齐。
    fn normalize_capability_route_orders(&mut self) -> bool {
        let original = (
            self.web_search.target_order.clone(),
            self.understand_image.target_order.clone(),
            self.image_generation.target_order.clone(),
            self.video_generation.target_order.clone(),
            self.speech_synthesis.target_order.clone(),
            self.music_generation.target_order.clone(),
            self.music_cover.target_order.clone(),
        );
        self.web_search.target_order = build_effective_route_target_order(
            &self.web_search.target_order,
            &self.vendors,
            AiToolKind::WebSearch,
        );
        self.understand_image.target_order = build_effective_route_target_order(
            &self.understand_image.target_order,
            &self.vendors,
            AiToolKind::UnderstandImage,
        );
        self.image_generation.target_order = build_effective_route_target_order(
            &self.image_generation.target_order,
            &self.vendors,
            AiToolKind::ImageGeneration,
        );
        self.video_generation.target_order = build_effective_route_target_order(
            &self.video_generation.target_order,
            &self.vendors,
            AiToolKind::VideoGeneration,
        );
        self.speech_synthesis.target_order = build_effective_route_target_order(
            &self.speech_synthesis.target_order,
            &self.vendors,
            AiToolKind::SpeechSynthesis,
        );
        self.music_generation.target_order = build_effective_route_target_order(
            &self.music_generation.target_order,
            &self.vendors,
            AiToolKind::MusicGeneration,
        );
        self.music_cover.target_order = build_effective_route_target_order(
            &self.music_cover.target_order,
            &self.vendors,
            AiToolKind::MusicCover,
        );
        original.0 != self.web_search.target_order
            || original.1 != self.understand_image.target_order
            || original.2 != self.image_generation.target_order
            || original.3 != self.video_generation.target_order
            || original.4 != self.speech_synthesis.target_order
            || original.5 != self.music_generation.target_order
            || original.6 != self.music_cover.target_order
    }

    /// Projects one fully qualified web-search target into the legacy provider-entry shape reused by the runtime adapter.
    /// 将单条完整网页搜索目标投影为运行时适配器复用的旧版供应商条目形态。
    fn projected_web_search_provider_for_target(
        &self,
        target: &str,
    ) -> Option<PersistedWebSearchProviderEntry> {
        let (vendor_id, _) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_binding(AiToolKind::WebSearch, target)?;
        let kind = WebSearchProviderKind::from_key(vendor_id)?;
        Some(PersistedWebSearchProviderEntry {
            id: format!("{}::{}", vendor.vendor_id, binding.target_alias()),
            name: format_ai_capability_target_name(&vendor.name, binding.target_alias()),
            kind,
            target_alias: binding.target_alias().to_string(),
            enabled: vendor.enabled && binding.enabled,
            api_key: vendor.api_key.clone(),
            base_url: vendor.base_url.clone(),
            model: binding.model_id().to_string(),
            region: vendor.region.clone(),
            max_results: self.web_search.max_results.max(1),
        })
    }

    /// Projects one fully qualified image-understanding target into the legacy provider-entry shape reused by the runtime adapter.
    /// 将单条完整图片理解目标投影为运行时适配器复用的旧版提供方条目形态。
    fn projected_understand_image_provider_for_target(
        &self,
        target: &str,
    ) -> Option<PersistedUnderstandImageProviderEntry> {
        let (vendor_id, _) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_binding(AiToolKind::UnderstandImage, target)?;
        let kind = UnderstandImageProviderKind::ALL
            .into_iter()
            .find(|kind| kind.key() == vendor_id)?;
        Some(PersistedUnderstandImageProviderEntry {
            id: format!("{}::{}", vendor.vendor_id, binding.target_alias()),
            name: format_ai_capability_target_name(&vendor.name, binding.target_alias()),
            kind,
            target_alias: binding.target_alias().to_string(),
            enabled: vendor.enabled && binding.enabled,
            api_key: vendor.api_key.clone(),
            base_url: vendor.base_url.clone(),
            region: vendor.region.clone(),
        })
    }

    /// Returns the effective target order for the requested capability by honoring configured route preference and then appending any newly enabled targets.
    /// 返回请求能力的生效目标顺序：先尊重已配置路由顺序，再补上新启用目标。
    fn projected_target_order_for_kind(&self, capability_kind: AiToolKind) -> Vec<String> {
        let route_order = match capability_kind {
            AiToolKind::WebSearch => &self.web_search.target_order,
            AiToolKind::UnderstandImage => &self.understand_image.target_order,
            AiToolKind::ImageGeneration => &self.image_generation.target_order,
            AiToolKind::VideoGeneration => &self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &self.music_generation.target_order,
            AiToolKind::MusicCover => &self.music_cover.target_order,
            _ => &EMPTY_TARGET_ORDER,
        };
        build_effective_route_target_order(route_order, &self.vendors, capability_kind)
    }

    /// Resolves one fully qualified target into the settings-facing route entry rendered in capability pages.
    /// 将单条完整目标解析成能力页面渲染使用的设置条目。
    fn resolve_route_target_for_kind(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<ResolvedAiCapabilityRouteTarget> {
        let (vendor_id, target_alias) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_binding(capability_kind, target)?;
        let definition =
            ai_capability_provider_catalog::ai_capability_provider_definition_by_target(
                vendor_id,
                capability_kind,
                target_alias,
            );
        let target_name = definition
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| binding.target_alias().to_string());
        Some(ResolvedAiCapabilityRouteTarget {
            capability_kind,
            target_selector: format!("{}/{}", vendor.vendor_id, binding.target_alias()),
            vendor_id: vendor.vendor_id.clone(),
            vendor_name: vendor.name.clone(),
            target_alias: binding.target_alias().to_string(),
            target_name,
            model_id: binding.model_id().to_string(),
        })
    }

    /// Upserts one legacy web-search provider into the shared vendor list.
    /// 将单条旧版网页搜索供应商配置上推合并到共享供应商列表中。
    fn upsert_vendor_target_from_web_search_provider(
        &mut self,
        provider: PersistedWebSearchProviderEntry,
    ) {
        let vendor_id = provider.kind.key();
        let target_alias = provider.target_alias().to_string();
        let target_selector = format!("{vendor_id}/{target_alias}");
        let vendor = ensure_ai_capability_vendor_entry(&mut self.vendors, vendor_id);
        merge_vendor_credentials_from_legacy_provider(
            vendor,
            &provider.name,
            &provider.api_key,
            &provider.base_url,
            &provider.region,
        );
        upsert_ai_capability_binding(
            &mut vendor.targets,
            PersistedAiCapabilityTargetBinding::new(
                AiToolKind::WebSearch,
                target_alias.clone(),
                provider.resolved_model(),
            ),
            provider.enabled,
        );
        if self.web_search.target_order.is_empty()
            || !self.web_search.target_order.contains(&target_selector)
        {
            self.web_search.target_order.push(target_selector);
        }
    }

    /// Upserts one legacy image-understanding provider into the shared vendor list.
    /// 将单条旧版图片理解提供方配置上推合并到共享供应商列表中。
    fn upsert_vendor_target_from_understand_image_provider(
        &mut self,
        provider: PersistedUnderstandImageProviderEntry,
    ) {
        let vendor_id = provider.kind.key();
        let target_alias = provider.target_alias().to_string();
        let target_selector = format!("{vendor_id}/{target_alias}");
        let vendor = ensure_ai_capability_vendor_entry(&mut self.vendors, vendor_id);
        merge_vendor_credentials_from_legacy_provider(
            vendor,
            &provider.name,
            &provider.api_key,
            &provider.base_url,
            &provider.region,
        );
        upsert_ai_capability_binding(
            &mut vendor.targets,
            PersistedAiCapabilityTargetBinding::new(
                AiToolKind::UnderstandImage,
                target_alias.clone(),
                target_alias.clone(),
            ),
            provider.enabled,
        );
        if self.understand_image.target_order.is_empty()
            || !self
                .understand_image
                .target_order
                .contains(&target_selector)
        {
            self.understand_image.target_order.push(target_selector);
        }
    }
}

/// Shared empty route-order slice reused by capability kinds that do not yet persist explicit route state.
/// 供尚未持久化显式路由状态的能力类型复用的共享空顺序切片。
static EMPTY_TARGET_ORDER: Vec<String> = Vec::new();

/// Returns one normalized vendor identifier used by persistence and route matching.
/// 返回供持久化与路由匹配使用的规范供应商标识。
fn normalize_ai_capability_vendor_id(vendor_id: impl Into<String>) -> String {
    vendor_id
        .into()
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_")
}

/// Returns the default vendor display name shown before the user customizes it.
/// 返回用户尚未自定义前显示的默认供应商名称。
fn default_ai_capability_vendor_name(vendor_id: &str) -> String {
    match vendor_id.trim() {
        "duckduckgo" => String::from("DuckDuckGo"),
        "brave" => String::from("Brave"),
        "tavily" => String::from("Tavily"),
        "exa" => String::from("Exa"),
        "firecrawl" => String::from("Firecrawl"),
        "gemini" => String::from("Gemini"),
        "grok" => String::from("Grok"),
        "kimi" => String::from("Kimi"),
        "minimax" => String::from("MiniMax"),
        "ollama" => String::from("Ollama"),
        "perplexity" => String::from("Perplexity"),
        "searxng" => String::from("SearXNG"),
        "serpapi" => String::from("SerpAPI"),
        other if !other.is_empty() => other.to_string(),
        _ => String::from("AI Capability Vendor"),
    }
}

/// Returns the default vendor region value used before the user fills an explicit override.
/// 返回用户尚未填写显式覆盖值前使用的默认供应商区域。
fn default_ai_capability_vendor_region(vendor_id: &str) -> String {
    if normalize_ai_capability_vendor_id(vendor_id.to_string()) == "minimax" {
        String::from("global")
    } else {
        String::new()
    }
}

/// Normalizes one MiniMax vendor-level base URL so shared credentials can drive multiple capability endpoints safely.
/// 归一化单个 MiniMax 供应商级 Base URL，使共享凭据能够安全驱动多个能力端点。
fn normalize_minimax_vendor_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut url) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if normalized_path.ends_with("/v1/coding_plan/search")
        || normalized_path.ends_with("/v1/coding_plan/vlm")
        || normalized_path.ends_with("/v1/image_generation")
        || normalized_path.ends_with("/v1/video_generation")
        || normalized_path.ends_with("/v1/query/video_generation")
        || normalized_path.ends_with("/v1/t2a_v2")
        || normalized_path.ends_with("/v1/music_generation")
    {
        url.set_path("");
        url.set_query(None);
        return url.to_string().trim_end_matches('/').to_string();
    }
    trimmed.to_string()
}

/// Normalizes one route-order array into a deduplicated, non-empty selector list.
/// 将单条路由顺序数组归一化为去重且非空的选择器列表。
fn normalize_route_target_order(target_order: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for target in target_order {
        let normalized_target = target.trim();
        if normalized_target.is_empty() {
            continue;
        }
        if normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(normalized_target))
        {
            continue;
        }
        normalized.push(normalized_target.to_string());
    }
    normalized
}

/// Normalizes all vendor instances in-place and removes duplicate vendor ids.
/// 原地归一化全部供应商实例并移除重复供应商标识。
fn normalize_ai_capability_vendors(vendors: &mut Vec<PersistedAiCapabilityVendorEntry>) -> bool {
    let original = vendors.clone();
    let mut normalized = Vec::new();
    for mut vendor in vendors.drain(..) {
        vendor.normalize();
        if normalized
            .iter()
            .any(|existing: &PersistedAiCapabilityVendorEntry| {
                existing.vendor_id == vendor.vendor_id
            })
        {
            continue;
        }
        normalized.push(vendor);
    }
    *vendors = normalized;
    *vendors != original
}

/// Returns the effective route target order by honoring the configured preference and then appending any enabled targets not yet mentioned.
/// 返回生效路由目标顺序：先尊重已配置偏好，再补齐尚未出现的已启用目标。
fn build_effective_route_target_order(
    configured_order: &[String],
    vendors: &[PersistedAiCapabilityVendorEntry],
    capability_kind: AiToolKind,
) -> Vec<String> {
    let mut natural_targets = Vec::new();
    for vendor in vendors {
        if !vendor.enabled {
            continue;
        }
        natural_targets.extend(vendor.enabled_target_strings_for(capability_kind));
    }

    let mut ordered = Vec::new();
    for target in normalize_route_target_order(configured_order) {
        if natural_targets
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&target))
            && !ordered
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    for target in natural_targets {
        if !ordered
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    ordered
}

/// Ensures one vendor entry exists inside the shared vendor list and returns a mutable reference to it.
/// 确保共享供应商列表中存在单个供应商条目，并返回其可变引用。
fn ensure_ai_capability_vendor_entry<'a>(
    vendors: &'a mut Vec<PersistedAiCapabilityVendorEntry>,
    vendor_id: &str,
) -> &'a mut PersistedAiCapabilityVendorEntry {
    let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
    if let Some(index) = vendors
        .iter()
        .position(|vendor| vendor.vendor_id == normalized_vendor_id)
    {
        return &mut vendors[index];
    }
    vendors.push(PersistedAiCapabilityVendorEntry::new(
        normalized_vendor_id.clone(),
    ));
    vendors
        .last_mut()
        .expect("vendor list should contain the newly inserted vendor entry")
}

/// Merges legacy capability-specific credential fields into one shared vendor entry without overwriting newer explicit values.
/// 将旧版按能力分散的凭据字段合并进共享供应商条目，且不覆盖更新的显式值。
fn merge_vendor_credentials_from_legacy_provider(
    vendor: &mut PersistedAiCapabilityVendorEntry,
    provider_name: &str,
    api_key: &str,
    base_url: &str,
    region: &str,
) {
    if vendor.name.trim().is_empty() {
        vendor.name = provider_name.trim().to_string();
    }
    if vendor.api_key.trim().is_empty() && !api_key.trim().is_empty() {
        vendor.api_key = api_key.trim().to_string();
    }
    if vendor.base_url.trim().is_empty() && !base_url.trim().is_empty() {
        vendor.base_url = if vendor.vendor_id == "minimax" {
            normalize_minimax_vendor_base_url(base_url)
        } else {
            base_url.trim().to_string()
        };
    }
    if vendor.region.trim().is_empty() && !region.trim().is_empty() {
        vendor.region = region.trim().to_string();
    }
}

/// Upserts one binding into the provided binding list while preserving the shared capability-target identity.
/// 将单条绑定上推写入给定绑定列表，并保持共享的能力目标身份唯一性。
fn upsert_ai_capability_binding(
    bindings: &mut Vec<PersistedAiCapabilityTargetBinding>,
    binding: PersistedAiCapabilityTargetBinding,
    enabled: bool,
) {
    if let Some(existing) = bindings.iter_mut().find(|existing| {
        existing.capability_kind == binding.capability_kind
            && existing
                .target_alias()
                .eq_ignore_ascii_case(binding.target_alias())
    }) {
        if existing.model_id.trim().is_empty() {
            existing.model_id = binding.model_id().to_string();
        }
        existing.enabled = existing.enabled || enabled;
        return;
    }

    let mut binding = binding;
    binding.enabled = enabled;
    bindings.push(binding);
}

/// Builds one compact target display name from a vendor display name plus one target alias.
/// 根据供应商显示名与目标别名构建紧凑目标显示名称。
fn format_ai_capability_target_name(vendor_name: &str, target_alias: &str) -> String {
    let normalized_target_alias = target_alias.trim();
    if normalized_target_alias.is_empty()
        || vendor_name
            .trim()
            .eq_ignore_ascii_case(normalized_target_alias)
    {
        vendor_name.trim().to_string()
    } else {
        format!("{}/{}", vendor_name.trim(), normalized_target_alias)
    }
}

/// Splits one `vendor/target` selector into its two stable components.
/// 将单条 `供应商/目标` 选择器拆分成两个稳定组成部分。
fn split_vendor_target_selector(target: &str) -> Option<(&str, &str)> {
    let trimmed = target.trim();
    let (vendor_id, target_alias) = trimmed.split_once('/')?;
    let vendor_id = vendor_id.trim();
    let target_alias = target_alias.trim();
    if vendor_id.is_empty() || target_alias.is_empty() {
        return None;
    }
    Some((vendor_id, target_alias))
}

/// Routing policy used when one AI-specialized tool has both provider-native and host fallback options.
/// 当单个 AI 专用工具同时存在 Provider 原生与宿主回退选项时使用的路由策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiToolPolicy {
    /// Prefer provider-native tools and fall back to host tools when configured.
    /// 优先使用 Provider 原生工具，并在已配置时回退到宿主工具。
    Auto,
    /// Require provider-native support.
    /// 强制要求 Provider 原生支持。
    ForceNative,
    /// Require a host fallback tool.
    /// 强制使用宿主回退工具。
    ForceHost,
    /// Do not expose the tool.
    /// 不暴露该工具。
    Disabled,
}

impl Default for AiToolPolicy {
    /// Returns the safe default AI-tool policy.
    /// 返回安全的默认 AI 工具策略。
    fn default() -> Self {
        Self::Auto
    }
}

/// Declarative AI-tool capability or override attached to one provider or model catalog entry.
/// 附着在单个供应商或模型目录条目上的声明式 AI 工具能力或覆盖配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolDeclaration {
    /// Stable semantic tool kind.
    /// 稳定语义工具类型。
    pub kind: AiToolKind,
    /// Routing policy used by the AI-tool router.
    /// AI 工具路由器使用的路由策略。
    #[serde(default, skip_serializing_if = "is_default_ai_tool_policy")]
    pub policy: AiToolPolicy,
    /// Whether provider-native execution is available for this tool.
    /// 当前工具是否可使用 Provider 原生执行。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_provider_tool: Option<bool>,
    /// Provider-native tool type such as `web_search_preview` or `google_search`.
    /// Provider 原生工具类型，例如 `web_search_preview` 或 `google_search`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_tool_type: Option<String>,
    /// Optional provider-native request patch appended to the outgoing payload when this hosted tool is enabled.
    /// 启用当前 Hosted Tool 时追加到出站请求载荷中的可选 Provider 原生请求补丁。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_request_patch: Option<Value>,
    /// Optional host fallback tool identifier used when native support is unavailable or disabled.
    /// 当原生支持不可用或被禁用时使用的可选宿主回退工具标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_tool_id: Option<String>,
    /// Whether this tool requires explicit user approval before execution.
    /// 当前工具执行前是否需要用户显式批准。
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_approval: bool,
    /// Modalities that may be returned by this tool.
    /// 当前工具可能返回的模态集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_modalities: Vec<String>,
    /// Whether the tool can return structured citations.
    /// 当前工具是否可以返回结构化引用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_support: Option<bool>,
}

impl AiToolDeclaration {
    /// Builds one provider-native declaration for a semantic AI tool.
    /// 为单个语义 AI 工具构建 Provider 原生声明。
    pub fn provider_native(kind: AiToolKind, native_tool_type: impl Into<String>) -> Self {
        Self {
            kind,
            policy: AiToolPolicy::Auto,
            native_provider_tool: Some(true),
            native_tool_type: Some(native_tool_type.into()),
            native_request_patch: None,
            fallback_tool_id: None,
            requires_approval: false,
            result_modalities: vec![String::from("text")],
            citation_support: Some(matches!(
                kind,
                AiToolKind::WebSearch | AiToolKind::FileSearch
            )),
        }
    }
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

    match runtime.api_family() {
        ProviderApiFamily::GoogleGeminiGenerateContent => {
            infer_google_gemini_reasoning_efforts(&model_id)
        }
        ProviderApiFamily::CohereChat => {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderApiFamily::OpenAiChatCompatible
            if runtime == ProviderRuntimeKind::OpenAiCompatible
                && matches!(provider_id, "alibaba" | "bailing") =>
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpOAuthConfig {
    /// Optional pre-registered OAuth client identifier.
    /// 可选的预注册 OAuth 客户端标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Optional pre-registered OAuth client secret.
    /// 可选的预注册 OAuth 客户端密钥。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Optional OAuth scopes requested during authorization.
    /// 授权过程中请求的可选 OAuth scopes 集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Optional redirect URI used by the OAuth authorization-code flow.
    /// OAuth 授权码流程使用的可选回调 URI。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
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
    /// Optional OAuth configuration used by host-driven remote authorization flows.
    /// 宿主驱动远端授权流程使用的可选 OAuth 配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<PersistedMcpOAuthConfig>,
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
    /// Legacy embedded AI tool routing and compatibility configuration kept only as a migration fallback for the standalone AI capability file.
    /// 仅作为独立 AI 能力配置文件迁移回退而保留的旧版内嵌 AI 工具路由与兼容配置。
    #[serde(
        default,
        rename = "aiTools",
        skip_serializing_if = "PersistedAiToolConfiguration::is_default"
    )]
    pub ai_tools: PersistedAiToolConfiguration,
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

/// One vendor-target choice exposed beneath a shared AI capability vendor instance.
/// 暴露在共享 AI 能力供应商实例下方的一条供应商目标选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCapabilityVendorTargetChoice {
    /// Stable semantic capability kind owned by the current target.
    /// 当前目标拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable target alias exposed below the vendor id.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// Default provider request model attached to the target.
    /// 绑定到当前目标的默认 Provider 请求模型。
    pub default_model_id: String,
    /// User-facing compact label rendered beside the target checkbox.
    /// 渲染在目标勾选项旁的人类可读紧凑标签。
    pub label: String,
    /// Whether the current target exposes one editable model field.
    /// 当前目标是否暴露可编辑模型字段。
    pub supports_model_override: bool,
}

/// One resolved settings-facing route entry rendered for an enabled semantic AI capability target.
/// 为某条已启用语义 AI 能力目标渲染使用的已解析设置路由条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAiCapabilityRouteTarget {
    /// Stable semantic capability kind owned by the route entry.
    /// 当前路由条目拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Fully qualified `vendor/target` selector exposed to the model and runtime.
    /// 暴露给模型与运行时的完整 `供应商/目标` 选择器。
    pub target_selector: String,
    /// Stable vendor identifier owning the target.
    /// 拥有当前目标的稳定供应商标识。
    pub vendor_id: String,
    /// User-facing vendor display name.
    /// 面向用户展示的供应商名称。
    pub vendor_name: String,
    /// Stable target alias exposed beneath the vendor identifier.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// User-facing target display name resolved from the capability provider catalog.
    /// 从能力提供方目录解析出的面向用户目标显示名。
    pub target_name: String,
    /// Provider request model or provider-specific request value attached to the target.
    /// 绑定到当前目标的 Provider 请求模型或 Provider 专属请求值。
    pub model_id: String,
}

#[allow(dead_code)]
impl ResolvedAiCapabilityRouteTarget {
    /// Returns the compact `Vendor / Target` label shown in enabled-target lists.
    /// 返回展示在已启用目标列表中的紧凑 `供应商 / 目标` 标签。
    pub fn display_label(&self) -> String {
        if self.vendor_name.eq_ignore_ascii_case(&self.target_name) {
            self.vendor_name.clone()
        } else {
            format!("{} / {}", self.vendor_name, self.target_name)
        }
    }
}

/// Returns the currently known capability targets exposed by one shared vendor instance.
/// 返回单个共享供应商实例当前已知的能力目标集合。
pub fn ai_capability_vendor_target_choices(vendor_id: &str) -> Vec<AiCapabilityVendorTargetChoice> {
    let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
    ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(
        &normalized_vendor_id,
    )
    .into_iter()
    .map(|definition| AiCapabilityVendorTargetChoice {
        capability_kind: definition.capability_kind,
        target_alias: definition.target_alias.clone(),
        default_model_id: if definition.supports_model_field {
            if definition.default_model.trim().is_empty() {
                definition.target_alias.clone()
            } else {
                definition.default_model.clone()
            }
        } else if definition.default_model.trim().is_empty() {
            definition.target_alias.clone()
        } else {
            definition.default_model.clone()
        },
        label: format!(
            "{} · {}",
            definition.capability_kind.label(),
            definition.name
        ),
        supports_model_override: definition.supports_model_field,
    })
    .collect()
}

/// Returns the shared vendor display name shown by the AI capability settings pages when one provider instance should not expose a custom editable alias.
/// 返回 AI 能力设置页在供应商实例不应暴露自定义别名时展示的共享供应商名称。
pub fn ai_capability_vendor_display_name(vendor_id: &str) -> String {
    default_ai_capability_vendor_name(&normalize_ai_capability_vendor_id(vendor_id.to_string()))
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
                    ai_tools: Vec::new(),
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
            }
        })
        .collect()
}

/// Returns the first matching dynamic model JSON value from a list of candidate field names.
/// 从候选字段名列表中返回第一条匹配的动态模型 JSON 值。
fn dynamic_model_value<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| object.get(*key))
}

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

/// Parses one optional dynamic interleaved reasoning capability from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态交错推理能力描述。
fn dynamic_model_interleaved(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ModelInterleavedCapability> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ModelInterleavedCapability>(value.clone()).ok())
}

/// Parses one optional dynamic reasoning profile from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态推理档位配置。
fn dynamic_model_reasoning_profile(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ReasoningProfileConfig> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ReasoningProfileConfig>(value.clone()).ok())
}

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

    let input_modalities = if flat_input.is_empty() {
        nested_input
    } else {
        flat_input
    };
    let output_modalities = if flat_output.is_empty() {
        nested_output
    } else {
        flat_output
    };

    (input_modalities, output_modalities)
}

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

#[cfg(test)]
mod tests {
    use super::{
        AiToolDeclaration, AiToolKind, ModelChoice, ModelInterleavedCapability,
        ModelInterleavedField, PersistedAiCapabilityTargetBinding,
        PersistedAiCapabilityVendorEntry, PersistedAiToolConfiguration,
        PersistedWebSearchConfiguration, PersistedWebSearchProviderEntry, ProviderApiFamily,
        ProviderBaseUrlConfig, ProviderBaseUrlKind, ProviderConfigurationFile,
        ProviderModelDiscoveryConfig, ProviderModelDiscoveryKind, ProviderRuntimeKind,
        ReasoningCapabilityKind, ReasoningEffortLevel, ReasoningProfileConfig,
        SupportedProviderDefinition, WebSearchProviderDispatchMode, WebSearchProviderKind,
        WebSearchRoutingMode, extract_models_from_array, merge_embedded_model_metadata,
    };
    use serde_json::{Map as JsonMap, json};
    use std::collections::BTreeMap;

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
            model_runtime_overrides: BTreeMap::new(),
            reasoning_profile,
            ai_tools: Vec::new(),
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
            ai_tools: Vec::new(),
            input_modalities: Vec::new(),
            output_modalities: Vec::new(),
        }
    }

    /// Verifies runtime kinds map into the expected canonical provider API families.
    /// 验证运行时类型会映射到预期的规范 Provider 接口族。
    #[test]
    fn runtime_kind_maps_into_expected_api_family() {
        assert_eq!(
            ProviderRuntimeKind::OpenAi.api_family(),
            ProviderApiFamily::OpenAiResponses
        );
        assert_eq!(
            ProviderRuntimeKind::OpenAiResponsesCompatible.api_family(),
            ProviderApiFamily::OpenAiResponses
        );
        assert_eq!(
            ProviderRuntimeKind::OpenAiCompatible.api_family(),
            ProviderApiFamily::OpenAiChatCompatible
        );
        assert_eq!(
            ProviderRuntimeKind::OpenRouter.api_family(),
            ProviderApiFamily::OpenAiChatCompatible
        );
        assert_eq!(
            ProviderRuntimeKind::Anthropic.api_family(),
            ProviderApiFamily::AnthropicMessages
        );
        assert_eq!(
            ProviderRuntimeKind::GoogleGemini.api_family(),
            ProviderApiFamily::GoogleGeminiGenerateContent
        );
        assert_eq!(
            ProviderRuntimeKind::Cohere.api_family(),
            ProviderApiFamily::CohereChat
        );
    }

    /// Verifies canonical provider API families expose stable labels and keys for diagnostics surfaces.
    /// 验证规范 Provider 接口族会暴露稳定的标签与键值，供诊断界面复用。
    #[test]
    fn provider_api_family_exposes_stable_label_and_key() {
        assert_eq!(ProviderApiFamily::OpenAiResponses.key(), "openai-responses");
        assert_eq!(
            ProviderApiFamily::OpenAiResponses.label(),
            "OpenAI Responses"
        );
        assert_eq!(
            ProviderApiFamily::AnthropicMessages.key(),
            "anthropic-messages"
        );
        assert_eq!(
            ProviderApiFamily::AnthropicMessages.label(),
            "Anthropic Messages"
        );
        assert_eq!(
            ProviderApiFamily::GoogleGeminiGenerateContent.key(),
            "google-gemini-generate-content"
        );
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

    /// Verifies API-native tool configuration keeps the stable root `aiTools.webSearch` shape through serialization.
    /// 验证 API 原生工具配置在序列化过程中保持稳定的根级 `aiTools.webSearch` 形状。
    #[test]
    fn provider_configuration_round_trips_web_search_routing_configuration() {
        let config = ProviderConfigurationFile {
            ai_tools: PersistedAiToolConfiguration {
                vendors: vec![PersistedAiCapabilityVendorEntry {
                    id: String::from("brave"),
                    vendor_id: String::from("brave"),
                    name: String::from("Brave"),
                    enabled: true,
                    api_key: String::from("{env:BRAVE_SEARCH_API_KEY}"),
                    base_url: String::new(),
                    region: String::new(),
                    targets: vec![PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "brave",
                        "brave",
                    )],
                }],
                web_search: PersistedWebSearchConfiguration {
                    mode: WebSearchRoutingMode::ThirdPartyFirst,
                    target_order: vec![String::from("brave/brave")],
                    providers: vec![PersistedWebSearchProviderEntry {
                        id: String::from("brave"),
                        name: String::from("Brave Search"),
                        kind: WebSearchProviderKind::Brave,
                        target_alias: String::from("brave"),
                        enabled: true,
                        api_key: String::from("{env:BRAVE_SEARCH_API_KEY}"),
                        base_url: String::new(),
                        model: String::new(),
                        region: String::new(),
                        max_results: 8,
                    }],
                    dispatch_mode: WebSearchProviderDispatchMode::Balanced,
                    max_results: 8,
                },
                ..PersistedAiToolConfiguration::default()
            },
            ..ProviderConfigurationFile::default()
        };

        let value = serde_json::to_value(&config).expect("provider configuration must serialize");
        assert_eq!(
            value["aiTools"]["webSearch"]["mode"],
            json!("third-party-first")
        );
        assert_eq!(
            value["aiTools"]["webSearch"]["dispatchMode"],
            json!("balanced")
        );
        assert_eq!(
            value["aiTools"]["webSearch"]["providers"][0]["kind"],
            json!("brave")
        );

        let restored: ProviderConfigurationFile =
            serde_json::from_value(value).expect("provider configuration must deserialize");
        assert_eq!(
            restored.ai_tools.web_search.mode,
            WebSearchRoutingMode::ThirdPartyFirst
        );
        assert_eq!(
            restored.ai_tools.web_search.dispatch_mode,
            WebSearchProviderDispatchMode::Balanced
        );
        assert_eq!(restored.ai_tools.web_search.providers.len(), 1);
        assert!(restored.ai_tools.web_search.providers[0].enabled);
    }

    /// Verifies MiniMax web-search hints stay aligned with the official Token Plan CLI naming.
    /// 验证 MiniMax 网页搜索提示与官方 Token Plan CLI 命名保持一致。
    #[test]
    fn minimax_web_search_configuration_hints_match_token_plan_cli() {
        assert_eq!(WebSearchProviderKind::MiniMax.label(), "MiniMax Search");
        assert_eq!(
            WebSearchProviderKind::MiniMax.api_key_label(),
            "Token Plan Key / {env:MINIMAX_API_KEY}"
        );
        assert_eq!(
            WebSearchProviderKind::MiniMax.api_key_placeholder(),
            "{env:MINIMAX_API_KEY}"
        );
        assert!(
            WebSearchProviderKind::MiniMax
                .configuration_note()
                .expect("MiniMax note must exist")
                .contains("Credits")
        );
    }

    /// Verifies web-search provider instances expose stable `instance/target` selectors for semantic routing.
    /// 验证网页搜索供应商实例会暴露稳定的 `实例/目标` 选择器以供语义路由使用。
    #[test]
    fn web_search_provider_entry_builds_qualified_target() {
        let entry = PersistedWebSearchProviderEntry::new("minimax", WebSearchProviderKind::MiniMax);

        assert_eq!(entry.instance_alias(), "minimax");
        assert_eq!(entry.target_alias(), "minimax");
        assert_eq!(entry.qualified_target(), "minimax/minimax");
        assert!(entry.matches_target("minimax/minimax"));
    }

    /// Verifies web-search configuration normalization removes duplicate provider kinds and restores canonical provider identifiers.
    /// 验证网页搜索配置归一化会移除重复提供方类型并恢复规范提供方标识。
    #[test]
    fn web_search_configuration_normalize_deduplicates_provider_kinds() {
        let mut configuration = PersistedWebSearchConfiguration {
            providers: vec![
                PersistedWebSearchProviderEntry {
                    id: String::from("tavily-2"),
                    name: String::from(""),
                    kind: WebSearchProviderKind::Tavily,
                    target_alias: String::from("tavily"),
                    enabled: true,
                    api_key: String::from("{env:TAVILY_API_KEY}"),
                    base_url: String::new(),
                    model: String::new(),
                    region: String::new(),
                    max_results: 5,
                },
                PersistedWebSearchProviderEntry {
                    id: String::from("tavily-3"),
                    name: String::from("备用 Tavily"),
                    kind: WebSearchProviderKind::Tavily,
                    target_alias: String::from("tavily"),
                    enabled: true,
                    api_key: String::from("{env:TAVILY_API_KEY}"),
                    base_url: String::new(),
                    model: String::new(),
                    region: String::new(),
                    max_results: 8,
                },
            ],
            ..PersistedWebSearchConfiguration::default()
        };

        assert!(configuration.normalize());
        assert_eq!(configuration.providers.len(), 1);
        assert_eq!(configuration.providers[0].id, "tavily");
        assert_eq!(configuration.providers[0].name, "Tavily");
        assert_eq!(
            configuration.providers[0].qualified_target(),
            "tavily/tavily"
        );
    }

    /// Verifies the shared MiniMax vendor exposes generation-capability targets through the unified target-choice catalog.
    /// 验证共享 MiniMax 供应商会通过统一目标选项目录暴露生成类能力目标。
    #[test]
    fn ai_capability_vendor_target_choices_include_minimax_generation_targets() {
        let choices = super::ai_capability_vendor_target_choices("minimax");

        assert!(choices.iter().any(|choice| {
            choice.capability_kind == AiToolKind::ImageGeneration
                && choice.target_alias == "image-01"
        }));
        assert!(choices.iter().any(|choice| {
            choice.capability_kind == AiToolKind::VideoGeneration
                && choice.target_alias == "MiniMax-Hailuo-2.3"
        }));
        assert!(choices.iter().any(|choice| {
            choice.capability_kind == AiToolKind::SpeechSynthesis
                && choice.target_alias == "speech-2.8-hd"
        }));
        assert!(choices.iter().any(|choice| {
            choice.capability_kind == AiToolKind::MusicGeneration
                && choice.target_alias == "music-2.6"
        }));
        assert!(choices.iter().any(|choice| {
            choice.capability_kind == AiToolKind::MusicCover && choice.target_alias == "music-cover"
        }));
    }

    /// Verifies remote model extraction preserves declared flat capability metadata and field-mapped interleaved settings.
    /// 验证远端模型提取会保留已声明的扁平能力元数据与字段映射型交错设置。
    #[test]
    fn extract_models_from_array_reads_flat_remote_metadata() {
        let models = extract_models_from_array(&[json!({
            "id": "deepseek-v4-pro",
            "name": "DeepSeek V4 Pro",
            "status": "stable",
            "family": "deepseek",
            "releaseDate": "2026-05-01",
            "contextLimit": 131072,
            "inputLimit": "120000",
            "outputLimit": 8192,
            "reasoning": true,
            "toolCall": "true",
            "temperature": false,
            "attachment": false,
            "interleaved": { "field": "reasoning_content" },
            "reasoningProfile": {
                "kind": "binary",
                "off": { "thinking": false },
                "low": { "thinking": true }
            },
            "aiTools": [
                {
                    "kind": "web_search",
                    "nativeProviderTool": true,
                    "nativeToolType": "web_search_preview",
                    "citationSupport": true
                }
            ],
            "inputModalities": ["text", "image"],
            "outputModalities": ["text"]
        })]);

        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(model.id, "deepseek-v4-pro");
        assert_eq!(model.name, "DeepSeek V4 Pro");
        assert_eq!(model.status.as_deref(), Some("stable"));
        assert_eq!(model.family.as_deref(), Some("deepseek"));
        assert_eq!(model.release_date.as_deref(), Some("2026-05-01"));
        assert_eq!(model.context_limit, Some(131072));
        assert_eq!(model.input_limit, Some(120000));
        assert_eq!(model.output_limit, Some(8192));
        assert_eq!(model.reasoning, Some(true));
        assert_eq!(model.tool_call, Some(true));
        assert_eq!(model.temperature, Some(false));
        assert_eq!(model.attachment, Some(false));
        assert_eq!(
            model.interleaved,
            Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_content"),
            }))
        );
        assert_eq!(model.input_modalities, vec!["text", "image"]);
        assert_eq!(model.output_modalities, vec!["text"]);
        assert!(model.reasoning_profile.is_some());
        assert_eq!(model.ai_tools.len(), 1);
        assert_eq!(model.ai_tools[0].kind, AiToolKind::WebSearch);
    }

    /// Verifies remote model extraction accepts nested modality declarations when the provider returns a `modalities` object instead of flat arrays.
    /// 验证当供应商返回 `modalities` 对象而非扁平数组时，远端模型提取仍可接受嵌套模态声明。
    #[test]
    fn extract_models_from_array_reads_nested_modalities() {
        let models = extract_models_from_array(&[json!({
            "id": "qwen3.6-plus",
            "modalities": {
                "input": ["text", "image", "video"],
                "output": ["text"]
            }
        })]);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].input_modalities, vec!["text", "image", "video"]);
        assert_eq!(models[0].output_modalities, vec!["text"]);
    }

    /// Verifies embedded metadata only fills fields that the dynamically discovered remote model did not already declare explicitly.
    /// 验证内置元数据只会补全动态发现模型未显式声明的字段，而不会覆盖远端已声明值。
    #[test]
    fn merge_embedded_model_metadata_keeps_remote_values_and_fills_blanks() {
        let discovered_models = vec![ModelChoice {
            id: String::from("qwen3.6-plus"),
            name: String::from("Qwen 3.6 Plus Remote"),
            status: None,
            family: Some(String::from("qwen")),
            release_date: None,
            context_limit: Some(262144),
            input_limit: None,
            output_limit: Some(8192),
            reasoning: Some(true),
            tool_call: None,
            temperature: Some(false),
            attachment: Some(false),
            interleaved: Some(ModelInterleavedCapability::Enabled(true)),
            reasoning_profile: None,
            ai_tools: Vec::new(),
            input_modalities: vec![String::from("text"), String::from("image")],
            output_modalities: Vec::new(),
        }];
        let embedded_models = vec![ModelChoice {
            id: String::from("qwen3.6-plus"),
            name: String::from("Qwen 3.6 Plus Embedded"),
            status: Some(String::from("preview")),
            family: Some(String::from("qwen-embedded")),
            release_date: Some(String::from("2026-04-20")),
            context_limit: Some(128000),
            input_limit: Some(120000),
            output_limit: Some(4096),
            reasoning: Some(false),
            tool_call: Some(true),
            temperature: Some(true),
            attachment: Some(true),
            interleaved: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_details"),
            })),
            reasoning_profile: Some(ReasoningProfileConfig {
                kind: Some(ReasoningCapabilityKind::Binary),
                off: Some(json!({ "thinking": false })),
                low: Some(json!({ "thinking": true })),
                ..ReasoningProfileConfig::default()
            }),
            ai_tools: vec![AiToolDeclaration::provider_native(
                AiToolKind::WebSearch,
                "web_search_preview",
            )],
            input_modalities: vec![
                String::from("text"),
                String::from("image"),
                String::from("video"),
            ],
            output_modalities: vec![String::from("text")],
        }];

        let merged = merge_embedded_model_metadata(discovered_models, &embedded_models);

        assert_eq!(merged.len(), 1);
        let model = &merged[0];
        assert_eq!(model.name, "Qwen 3.6 Plus Remote");
        assert_eq!(model.status.as_deref(), Some("preview"));
        assert_eq!(model.family.as_deref(), Some("qwen"));
        assert_eq!(model.release_date.as_deref(), Some("2026-04-20"));
        assert_eq!(model.context_limit, Some(262144));
        assert_eq!(model.input_limit, Some(120000));
        assert_eq!(model.output_limit, Some(8192));
        assert_eq!(model.reasoning, Some(true));
        assert_eq!(model.tool_call, Some(true));
        assert_eq!(model.temperature, Some(false));
        assert_eq!(model.attachment, Some(false));
        assert_eq!(
            model.interleaved,
            Some(ModelInterleavedCapability::Enabled(true))
        );
        assert!(model.reasoning_profile.is_some());
        assert_eq!(model.input_modalities, vec!["text", "image"]);
        assert_eq!(model.output_modalities, vec!["text"]);
        assert_eq!(model.ai_tools.len(), 1);
    }
}

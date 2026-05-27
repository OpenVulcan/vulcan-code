//! AI capability and hosted-tool provider configuration models.
//! AI 能力与 Hosted Tool 供应商配置模型。

use super::{
    default_true, default_web_search_max_results, is_default, is_default_ai_tool_policy, is_false,
    is_true, resolve_env_backed_secret,
};
use crate::ai_capability_provider_catalog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
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
    /// Optional finite request-model options shown as fixed child items when the target should not use free-form model text input.
    /// 当目标不应使用自由输入模型文本框时，显示为固定子项的可选请求模型集合。
    pub model_options: Vec<String>,
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
    /// Whether the current configured target is effectively enabled for routing.
    /// 当前已配置目标是否在路由中处于有效启用状态。
    pub enabled: bool,
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
    ai_capability_provider_catalog::ai_capability_provider_targets_for_vendor(&normalized_vendor_id)
        .into_iter()
        .map(|target| AiCapabilityVendorTargetChoice {
            capability_kind: target.capability_kind,
            target_alias: target.target_alias.clone(),
            default_model_id: if target.supports_model_field {
                if target.default_model.trim().is_empty() {
                    target.target_alias.clone()
                } else {
                    target.default_model.clone()
                }
            } else if target.default_model.trim().is_empty() {
                target.target_alias.clone()
            } else {
                target.default_model.clone()
            },
            label: target.name.clone(),
            supports_model_override: target.supports_model_field,
            model_options: target.model_options.clone(),
        })
        .collect()
}

/// Returns the shared vendor display name shown by the AI capability settings pages when one provider instance should not expose a custom editable alias.
/// 返回 AI 能力设置页在供应商实例不应暴露自定义别名时展示的共享供应商名称。
pub fn ai_capability_vendor_display_name(vendor_id: &str) -> String {
    default_ai_capability_vendor_name(&normalize_ai_capability_vendor_id(vendor_id.to_string()))
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

/// Maximum number of enabled targets allowed beneath one semantic AI capability.
/// 单个语义 AI 能力允许启用的目标上限数量。
pub const MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND: usize = 20;

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

    /// Returns whether the current semantic capability participates in the workbench quick-toggle persistence flow.
    /// 返回当前语义能力是否参与工作台快捷开关持久化流程。
    pub const fn supports_workbench_quick_toggle(self) -> bool {
        matches!(
            self,
            Self::WebSearch
                | Self::UnderstandImage
                | Self::ImageGeneration
                | Self::VideoGeneration
                | Self::SpeechSynthesis
                | Self::MusicGeneration
                | Self::MusicCover
        )
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
    pub const ALL: [Self; 4] = [
        Self::NativeOnly,
        Self::NativeFirst,
        Self::ThirdPartyFirst,
        Self::ThirdPartyOnly,
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

/// User-selected routing mode for native multimodal input versus one host-provided understanding bridge.
/// 用户为原生多模态输入与宿主理解桥接能力选择的路由模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeBridgeRoutingMode {
    /// Only keep the model's native multimodal understanding path.
    /// 仅保留模型原生多模态理解路径。
    NativeOnly,
    /// Prefer the model's native multimodal understanding path, then fall back to one host bridge.
    /// 优先使用模型原生多模态理解路径，再回退到宿主桥接能力。
    NativeFirst,
    /// Always expose the host bridge instead of relying on native understanding.
    /// 始终暴露宿主桥接能力，而不是依赖原生理解。
    HostOnly,
}

impl NativeBridgeRoutingMode {
    /// All routing modes shown by native-versus-host bridge pickers in stable order.
    /// 原生与宿主桥接选择器按稳定顺序展示的全部路由模式。
    pub const ALL: [Self; 3] = [Self::NativeFirst, Self::NativeOnly, Self::HostOnly];

    /// Returns the compact settings label for the current routing mode.
    /// 返回当前路由模式在设置界面使用的紧凑标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::NativeOnly => "仅使用模型原生",
            Self::NativeFirst => "原生优先，工具兜底",
            Self::HostOnly => "仅使用工具理解",
        }
    }

    /// Returns one explanatory sentence for settings and diagnostics surfaces.
    /// 返回用于设置与诊断界面的一句解释说明。
    pub const fn description(self) -> &'static str {
        match self {
            Self::NativeOnly => {
                "仅在模型自身声明支持对应输入模态时使用原生理解，不额外暴露宿主理解工具。"
            }
            Self::NativeFirst => {
                "优先依赖模型原生多模态输入；当模型不支持时，再暴露宿主理解工具补位。"
            }
            Self::HostOnly => {
                "无论模型是否具备原生多模态输入，都优先暴露宿主理解工具并通过工具链完成理解。"
            }
        }
    }
}

impl Default for NativeBridgeRoutingMode {
    /// Returns the safest default that keeps native input first and uses the host bridge only as a fallback.
    /// 返回最稳妥的默认值，优先保留原生输入，并仅在需要时使用宿主桥接作为回退。
    fn default() -> Self {
        Self::NativeFirst
    }
}

impl fmt::Display for NativeBridgeRoutingMode {
    /// Formats one native-versus-host routing mode as a user-facing label.
    /// 将单个原生与宿主路由模式格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl Default for WebSearchRoutingMode {
    /// Returns the conservative default that preserves provider-native behavior.
    /// 返回保守默认值，以保留供应商原生行为。
    fn default() -> Self {
        Self::NativeFirst
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

/// One explicit shared-vendor connection mode used by providers that can switch between local and official cloud transports.
/// 供可在本地与官方云端传输之间切换的供应商使用的显式共享连接模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiCapabilityVendorConnectionMode {
    /// Uses one self-hosted or local endpoint.
    /// 使用自托管或本地端点。
    Local,
    /// Uses the official vendor-managed cloud endpoint.
    /// 使用供应商官方托管的云端端点。
    Cloud,
}

impl AiCapabilityVendorConnectionMode {
    /// Ordered connection-mode choices shown in the shared vendor editor.
    /// 显示在共享供应商编辑器中的有序连接模式选项。
    pub const ALL: [Self; 2] = [Self::Local, Self::Cloud];

    /// Returns the stable user-facing label for the current connection mode.
    /// 返回当前连接模式的稳定面向用户标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "本地",
            Self::Cloud => "Ollama Cloud",
        }
    }
}

impl fmt::Display for AiCapabilityVendorConnectionMode {
    /// Renders the stable user-facing label used by pick lists.
    /// 渲染供下拉选择器使用的稳定面向用户标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
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
    /// Optional connection mode used by vendors that can switch between local and cloud backends.
    /// 可由支持本地与云端后端切换的供应商使用的可选连接模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_mode: Option<AiCapabilityVendorConnectionMode>,
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
            connection_mode: None,
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

    /// Returns the fully qualified `vendor/target` selectors configured for one capability regardless of the current enabled flag.
    /// 返回单个能力当前已配置的完整 `供应商/目标` 选择器集合，不受当前启用标记影响。
    pub fn configured_target_strings_for(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.targets
            .iter()
            .filter(|binding| binding.capability_kind == capability_kind)
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

    /// Returns the first configured binding that matches the provided capability and target selector regardless of the enabled flag.
    /// 返回首条与给定能力及目标选择器匹配的已配置绑定，不受启用标记影响。
    pub fn matching_configured_binding(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<&PersistedAiCapabilityTargetBinding> {
        let normalized_target = target.trim().to_ascii_lowercase();
        self.targets.iter().find(|binding| {
            binding.capability_kind == capability_kind
                && format!("{}/{}", self.vendor_id, binding.target_alias()).to_ascii_lowercase()
                    == normalized_target
        })
    }

    /// Resolves the shared API key for the current vendor instance, honoring `{env:NAME}` placeholders.
    /// 解析当前供应商实例的共享 API Key，并支持 `{env:NAME}` 环境变量占位符。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), "AI 能力供应商 API Key")
    }

    /// Returns whether the current shared vendor exposes an explicit local/cloud connection-mode switch.
    /// 返回当前共享供应商是否暴露显式的本地/云端连接模式切换。
    pub fn supports_connection_mode(self: &Self) -> bool {
        self.vendor_id.eq_ignore_ascii_case("ollama")
    }

    /// Returns the effective connection mode, inferring one sensible default for legacy vendor rows that predate the explicit selector.
    /// 返回生效连接模式，并为显式选择器落地前的旧供应商条目推断合理默认值。
    pub fn resolved_connection_mode(&self) -> AiCapabilityVendorConnectionMode {
        if !self.supports_connection_mode() {
            return AiCapabilityVendorConnectionMode::Local;
        }

        self.connection_mode.unwrap_or_else(|| {
            if self.api_key.trim().is_empty() {
                AiCapabilityVendorConnectionMode::Local
            } else {
                AiCapabilityVendorConnectionMode::Cloud
            }
        })
    }

    /// Returns the effective vendor-level base URL after applying vendor-specific connection-mode defaults.
    /// 应用供应商专属连接模式默认值后，返回生效的供应商级 Base URL。
    pub fn resolved_vendor_base_url(&self) -> String {
        let trimmed = self.base_url.trim();

        if self.vendor_id.eq_ignore_ascii_case("ollama") {
            return match self.resolved_connection_mode() {
                AiCapabilityVendorConnectionMode::Local => {
                    if trimmed.is_empty() {
                        String::from("http://127.0.0.1:11434")
                    } else {
                        trimmed.to_string()
                    }
                }
                AiCapabilityVendorConnectionMode::Cloud => String::from("https://ollama.com"),
            };
        }

        trimmed.to_string()
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
            if ai_capability_provider_catalog::ai_capability_provider_definition_by_target(
                &normalized_vendor_id,
                binding.capability_kind,
                binding.target_alias(),
            )
            .is_none()
            {
                continue;
            }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUnderstandImageConfiguration {
    /// User-selected native-versus-host routing mode for image understanding.
    /// 用户为图片理解选择的原生与宿主路由模式。
    #[serde(default)]
    pub mode: NativeBridgeRoutingMode,
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    /// Legacy ordered external provider list kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版按顺序排列的外部提供方列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedUnderstandImageProviderEntry>,
}

impl Default for PersistedUnderstandImageConfiguration {
    /// Builds the default image-understanding configuration.
    /// 构建默认图片理解配置。
    fn default() -> Self {
        Self {
            mode: NativeBridgeRoutingMode::default(),
            target_order: Vec::new(),
            providers: Vec::new(),
        }
    }
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
            if !understand_image_provider_kind_is_supported(provider.kind) {
                continue;
            }
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
            if !web_search_provider_kind_is_supported(provider.kind) {
                continue;
            }
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
        if self.mode == WebSearchRoutingMode::Disabled {
            self.mode = WebSearchRoutingMode::default();
        }
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
    /// Semantic capability kinds that stay disabled in the workbench quick-toggle surface until the user explicitly re-enables them.
    /// 在工作台快捷开关界面中保持关闭、直到用户显式重新开启的语义能力类型集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_capability_kinds: BTreeSet<AiToolKind>,
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
        let original_disabled_capability_kinds = self.disabled_capability_kinds.clone();
        self.disabled_capability_kinds
            .retain(|kind| kind.supports_workbench_quick_toggle());
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
        let disabled_capability_changed =
            self.disabled_capability_kinds != original_disabled_capability_kinds;
        web_search_changed
            || disabled_capability_changed
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

    /// Returns the total count of currently usable enabled targets for one semantic capability.
    /// 返回单个语义能力当前可用且已启用目标的总数量。
    pub fn enabled_target_count_for_kind(&self, capability_kind: AiToolKind) -> usize {
        self.vendors
            .iter()
            .filter(|vendor| vendor.enabled)
            .map(|vendor| {
                vendor
                    .targets
                    .iter()
                    .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
                    .count()
            })
            .sum()
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

    /// Returns the ordered route entries rendered in settings and quick panels for all configured targets of one semantic capability, including currently disabled items.
    /// 返回设置页与快捷面板为某个语义能力渲染的全部已配置目标有序条目，包含当前已禁用项。
    pub fn resolved_configured_targets_for_kind(
        &self,
        capability_kind: AiToolKind,
    ) -> Vec<ResolvedAiCapabilityRouteTarget> {
        build_stable_route_target_order(
            self.configured_target_order_for_kind(capability_kind),
            &self.vendors,
            capability_kind,
        )
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
        self.web_search.target_order = build_stable_route_target_order(
            &self.web_search.target_order,
            &self.vendors,
            AiToolKind::WebSearch,
        );
        self.understand_image.target_order = build_stable_route_target_order(
            &self.understand_image.target_order,
            &self.vendors,
            AiToolKind::UnderstandImage,
        );
        self.image_generation.target_order = build_stable_route_target_order(
            &self.image_generation.target_order,
            &self.vendors,
            AiToolKind::ImageGeneration,
        );
        self.video_generation.target_order = build_stable_route_target_order(
            &self.video_generation.target_order,
            &self.vendors,
            AiToolKind::VideoGeneration,
        );
        self.speech_synthesis.target_order = build_stable_route_target_order(
            &self.speech_synthesis.target_order,
            &self.vendors,
            AiToolKind::SpeechSynthesis,
        );
        self.music_generation.target_order = build_stable_route_target_order(
            &self.music_generation.target_order,
            &self.vendors,
            AiToolKind::MusicGeneration,
        );
        self.music_cover.target_order = build_stable_route_target_order(
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
            base_url: vendor.resolved_vendor_base_url(),
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
        let binding = vendor.matching_configured_binding(capability_kind, target)?;
        let target_definition =
            ai_capability_provider_catalog::ai_capability_provider_target_by_target(
                vendor_id,
                capability_kind,
                target_alias,
            );
        let target_name = target_definition
            .map(|definition| definition.name)
            .unwrap_or_else(|| binding.target_alias().to_string());
        Some(ResolvedAiCapabilityRouteTarget {
            capability_kind,
            target_selector: format!("{}/{}", vendor.vendor_id, binding.target_alias()),
            vendor_id: vendor.vendor_id.clone(),
            vendor_name: vendor.name.clone(),
            target_alias: binding.target_alias().to_string(),
            target_name,
            model_id: binding.model_id().to_string(),
            enabled: vendor.enabled && binding.enabled,
        })
    }

    /// Upserts one legacy web-search provider into the shared vendor list.
    /// 将单条旧版网页搜索供应商配置上推合并到共享供应商列表中。
    fn upsert_vendor_target_from_web_search_provider(
        &mut self,
        provider: PersistedWebSearchProviderEntry,
    ) {
        if !web_search_provider_kind_is_supported(provider.kind) {
            return;
        }
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
        if !understand_image_provider_kind_is_supported(provider.kind) {
            return;
        }
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

/// Returns whether one shared AI capability vendor identifier is still supported by the current capability catalog.
/// 返回单个共享 AI 能力供应商标识当前是否仍受现有能力目录支持。
fn ai_capability_vendor_is_supported(vendor_id: &str) -> bool {
    !ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(vendor_id)
        .is_empty()
}

/// Returns whether one legacy web-search provider kind is still supported by the current capability catalog.
/// 返回单个旧版网页搜索提供方类型当前是否仍受现有能力目录支持。
fn web_search_provider_kind_is_supported(kind: WebSearchProviderKind) -> bool {
    ai_capability_provider_catalog::web_search_provider_enabled(kind.key())
}

/// Returns whether one legacy image-understanding provider kind is still supported by the current capability catalog.
/// 返回单个旧版图片理解提供方类型当前是否仍受现有能力目录支持。
fn understand_image_provider_kind_is_supported(kind: UnderstandImageProviderKind) -> bool {
    ai_capability_provider_catalog::understand_image_provider_enabled(kind.key())
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
        if !ai_capability_vendor_is_supported(&vendor.vendor_id) {
            continue;
        }
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
    if ordered.len() > MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND {
        ordered.truncate(MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND);
    }
    ordered
}

/// Returns the stable persisted route target order by honoring the configured preference and then appending any configured targets not yet mentioned.
/// 返回稳定持久化的路由目标顺序：先尊重已配置偏好，再补齐尚未出现的全部已配置目标。
fn build_stable_route_target_order(
    configured_order: &[String],
    vendors: &[PersistedAiCapabilityVendorEntry],
    capability_kind: AiToolKind,
) -> Vec<String> {
    let mut configured_targets = Vec::new();
    for vendor in vendors {
        configured_targets.extend(vendor.configured_target_strings_for(capability_kind));
    }

    let mut ordered = Vec::new();
    for target in normalize_route_target_order(configured_order) {
        if configured_targets
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&target))
            && !ordered
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    for target in configured_targets {
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

use super::*;

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

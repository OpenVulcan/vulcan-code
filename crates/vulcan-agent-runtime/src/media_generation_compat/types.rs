//! Shared types for configured media-generation compatibility adapters.
//! 配置化媒体生成兼容适配器的共享类型。

use crate::provider_config::{
    PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
};
use vulcan_core::types::ConversationAttachment;

/// Shared schema description for caller-selected export directories used by generated media tools.
/// 生成媒体工具用于调用方选定导出目录的共享参数模式描述。
pub(super) const GENERATED_OUTPUT_DIR_DESCRIPTION: &str = "Optional export directory. Absolute paths are accepted, and relative paths are resolved from the current workspace.";

/// Shared schema description for caller-selected file-name stems whose real extension is always host-controlled.
/// 调用方选定文件名主体的共享参数模式描述，真实扩展名始终由宿主控制。
pub(super) const GENERATED_OUTPUT_FILE_STEM_DESCRIPTION: &str = "Optional output file-name stem. Any supplied extension is ignored because the host chooses the real extension from the generated media format.";

/// Output-location options shared by host-managed media generation tools.
/// 宿主管理媒体生成工具共享的输出位置选项。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct GeneratedAssetOutputOptions {
    /// Optional caller-selected export directory, expressed either as an absolute path or as a workspace-relative path.
    /// 可选的调用方选定导出目录，既可以是绝对路径，也可以是相对工作区路径。
    pub(super) output_dir: Option<String>,
    /// Optional caller-selected file-name stem without a trusted extension.
    /// 可选的调用方选定文件名主体，且其扩展名不被信任。
    pub(super) output_file_stem: Option<String>,
}

/// Parsed invocation accepted by the semantic `image_generation` tool.
/// 语义 `image_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImageGenerationInvocation {
    /// Free-form image prompt.
    /// 自由形式图片提示词。
    pub(super) prompt: String,
    /// Number of images requested from the provider.
    /// 向提供方请求的图片数量。
    pub(super) count: usize,
    /// Optional aspect-ratio hint.
    /// 可选宽高比提示。
    pub(super) aspect_ratio: Option<String>,
    /// Optional deterministic seed.
    /// 可选确定性随机种子。
    pub(super) seed: Option<u64>,
    /// Optional subject-reference image attachment selector.
    /// 可选主体参考图片附件选择器。
    pub(super) subject_image: Option<String>,
    /// Whether the provider should optimize the prompt before generation.
    /// 是否让提供方在生成前优化提示词。
    pub(super) prompt_optimizer: bool,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    pub(super) output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `video_generation` tool.
/// 语义 `video_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VideoGenerationInvocation {
    /// Free-form video prompt.
    /// 自由形式视频提示词。
    pub(super) prompt: String,
    /// Optional first-frame image attachment selector.
    /// 可选首帧图片附件选择器。
    pub(super) first_image: Option<String>,
    /// Optional last-frame image attachment selector.
    /// 可选末帧图片附件选择器。
    pub(super) last_image: Option<String>,
    /// Optional subject-reference image attachment selector.
    /// 可选主体参考图片附件选择器。
    pub(super) subject_image: Option<String>,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    pub(super) output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `speech_synthesis` tool.
/// 语义 `speech_synthesis` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpeechSynthesisInvocation {
    /// Input text to synthesize.
    /// 需要合成的输入文本。
    pub(super) text: String,
    /// Optional voice identifier.
    /// 可选语音标识。
    pub(super) voice: Option<String>,
    /// Optional language boost code.
    /// 可选语言增强代码。
    pub(super) language: Option<String>,
    /// Output audio format.
    /// 输出音频格式。
    pub(super) format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    pub(super) output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `music_generation` tool.
/// 语义 `music_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MusicGenerationInvocation {
    /// Optional style prompt.
    /// 可选风格提示词。
    pub(super) prompt: Option<String>,
    /// Optional explicit lyrics body.
    /// 可选显式歌词正文。
    pub(super) lyrics: Option<String>,
    /// Whether to request instrumental generation.
    /// 是否请求纯音乐生成。
    pub(super) instrumental: bool,
    /// Whether to request automatic lyrics generation.
    /// 是否请求自动生成歌词。
    pub(super) lyrics_optimizer: bool,
    /// Output audio format.
    /// 输出音频格式。
    pub(super) format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    pub(super) output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `music_cover` tool.
/// 语义 `music_cover` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MusicCoverInvocation {
    /// Cover-style prompt.
    /// 翻唱风格提示词。
    pub(super) prompt: String,
    /// Audio attachment selector.
    /// 音频附件选择器。
    pub(super) audio: String,
    /// Optional cover lyrics override.
    /// 可选翻唱歌词覆盖值。
    pub(super) lyrics: Option<String>,
    /// Output audio format.
    /// 输出音频格式。
    pub(super) format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    pub(super) output: GeneratedAssetOutputOptions,
}

/// One resolved vendor target used during host-side capability execution.
/// 宿主侧能力执行期间使用的一条已解析供应商目标。
#[derive(Debug, Clone)]
pub(super) struct ResolvedVendorTarget {
    /// Shared vendor entry chosen for this execution attempt.
    /// 本次执行尝试选中的共享供应商条目。
    pub(super) vendor: PersistedAiCapabilityVendorEntry,
    /// Capability target binding chosen for this execution attempt.
    /// 本次执行尝试选中的能力目标绑定。
    pub(super) binding: PersistedAiCapabilityTargetBinding,
    /// Fully qualified `vendor/target` selector frozen by the host configuration.
    /// 由宿主配置冻结的完整 `供应商/目标` 选择器。
    pub(super) target_selector: String,
}

/// One generated media output that keeps the managed attachment plus the model-visible preferred local path together.
/// 一条生成媒体输出，同时保留托管附件与面向模型可见的首选本地路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StagedGeneratedAsset {
    /// Host-managed attachment retained for transcript previews, replay, and asset indexing.
    /// 为转录预览、回放与资产索引保留的宿主管理附件。
    pub(super) attachment: ConversationAttachment,
    /// Preferred local filesystem path returned to the model for immediate follow-up actions.
    /// 返回给模型、供其立即执行后续操作的首选本地文件系统路径。
    pub(super) model_local_path: String,
}

//! Shared host-side compatibility adapters for configured image, video, speech, and music capability vendors.
//! 为已配置图片、视频、语音与音乐能力供应商提供共享宿主兼容适配器。

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, attachment_descriptor_from_path, stage_attachment_bytes,
    stage_attachment_file, write_attachment_bytes_to_directory,
};
use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration,
};
use crate::workspace_path::{canonicalize_workspace_path, normalize_workspace_path_text};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::time::{Duration, sleep};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

/// Shared schema description for caller-selected export directories used by generated media tools.
/// 生成媒体工具用于调用方选定导出目录的共享参数模式描述。
const GENERATED_OUTPUT_DIR_DESCRIPTION: &str = "Optional export directory. Absolute paths are accepted, and relative paths are resolved from the current workspace.";

/// Shared schema description for caller-selected file-name stems whose real extension is always host-controlled.
/// 调用方选定文件名主体的共享参数模式描述，真实扩展名始终由宿主控制。
const GENERATED_OUTPUT_FILE_STEM_DESCRIPTION: &str = "Optional output file-name stem. Any supplied extension is ignored because the host chooses the real extension from the generated media format.";

/// Output-location options shared by host-managed media generation tools.
/// 宿主管理媒体生成工具共享的输出位置选项。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct GeneratedAssetOutputOptions {
    /// Optional caller-selected export directory, expressed either as an absolute path or as a workspace-relative path.
    /// 可选的调用方选定导出目录，既可以是绝对路径，也可以是相对工作区路径。
    output_dir: Option<String>,
    /// Optional caller-selected file-name stem without a trusted extension.
    /// 可选的调用方选定文件名主体，且其扩展名不被信任。
    output_file_stem: Option<String>,
}

/// Parsed invocation accepted by the semantic `image_generation` tool.
/// 语义 `image_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageGenerationInvocation {
    /// Free-form image prompt.
    /// 自由形式图片提示词。
    prompt: String,
    /// Target selector chosen by the model or `auto`.
    /// 由模型选择的目标选择器或 `auto`。
    target: Option<String>,
    /// Number of images requested from the provider.
    /// 向提供方请求的图片数量。
    count: usize,
    /// Optional aspect-ratio hint.
    /// 可选宽高比提示。
    aspect_ratio: Option<String>,
    /// Optional deterministic seed.
    /// 可选确定性随机种子。
    seed: Option<u64>,
    /// Optional subject-reference image attachment selector.
    /// 可选主体参考图片附件选择器。
    subject_image: Option<String>,
    /// Whether the provider should optimize the prompt before generation.
    /// 是否让提供方在生成前优化提示词。
    prompt_optimizer: bool,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `video_generation` tool.
/// 语义 `video_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
struct VideoGenerationInvocation {
    /// Free-form video prompt.
    /// 自由形式视频提示词。
    prompt: String,
    /// Target selector chosen by the model or `auto`.
    /// 由模型选择的目标选择器或 `auto`。
    target: Option<String>,
    /// Optional first-frame image attachment selector.
    /// 可选首帧图片附件选择器。
    first_image: Option<String>,
    /// Optional last-frame image attachment selector.
    /// 可选末帧图片附件选择器。
    last_image: Option<String>,
    /// Optional subject-reference image attachment selector.
    /// 可选主体参考图片附件选择器。
    subject_image: Option<String>,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `speech_synthesis` tool.
/// 语义 `speech_synthesis` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SpeechSynthesisInvocation {
    /// Input text to synthesize.
    /// 需要合成的输入文本。
    text: String,
    /// Target selector chosen by the model or `auto`.
    /// 由模型选择的目标选择器或 `auto`。
    target: Option<String>,
    /// Optional voice identifier.
    /// 可选语音标识。
    voice: Option<String>,
    /// Optional language boost code.
    /// 可选语言增强代码。
    language: Option<String>,
    /// Output audio format.
    /// 输出音频格式。
    format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `music_generation` tool.
/// 语义 `music_generation` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MusicGenerationInvocation {
    /// Optional style prompt.
    /// 可选风格提示词。
    prompt: Option<String>,
    /// Optional explicit lyrics body.
    /// 可选显式歌词正文。
    lyrics: Option<String>,
    /// Target selector chosen by the model or `auto`.
    /// 由模型选择的目标选择器或 `auto`。
    target: Option<String>,
    /// Whether to request instrumental generation.
    /// 是否请求纯音乐生成。
    instrumental: bool,
    /// Whether to request automatic lyrics generation.
    /// 是否请求自动生成歌词。
    lyrics_optimizer: bool,
    /// Output audio format.
    /// 输出音频格式。
    format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    output: GeneratedAssetOutputOptions,
}

/// Parsed invocation accepted by the semantic `music_cover` tool.
/// 语义 `music_cover` 工具接受的已解析调用参数。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MusicCoverInvocation {
    /// Cover-style prompt.
    /// 翻唱风格提示词。
    prompt: String,
    /// Audio attachment selector.
    /// 音频附件选择器。
    audio: String,
    /// Optional cover lyrics override.
    /// 可选翻唱歌词覆盖值。
    lyrics: Option<String>,
    /// Target selector chosen by the model or `auto`.
    /// 由模型选择的目标选择器或 `auto`。
    target: Option<String>,
    /// Output audio format.
    /// 输出音频格式。
    format: String,
    /// Output-location options shared by generated media tools.
    /// 生成媒体工具共享的输出位置选项。
    output: GeneratedAssetOutputOptions,
}

/// One resolved vendor target used during host-side capability execution.
/// 宿主侧能力执行期间使用的一条已解析供应商目标。
#[derive(Debug, Clone)]
struct ResolvedVendorTarget {
    /// Shared vendor entry chosen for this execution attempt.
    /// 本次执行尝试选中的共享供应商条目。
    vendor: PersistedAiCapabilityVendorEntry,
    /// Capability target binding chosen for this execution attempt.
    /// 本次执行尝试选中的能力目标绑定。
    binding: PersistedAiCapabilityTargetBinding,
    /// Fully qualified `vendor/target` selector exposed to the model.
    /// 向模型暴露的完整 `供应商/目标` 选择器。
    target_selector: String,
}

/// One generated media output that keeps the managed attachment plus the model-visible preferred local path together.
/// 一条生成媒体输出，同时保留托管附件与面向模型可见的首选本地路径。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StagedGeneratedAsset {
    /// Host-managed attachment retained for transcript previews, replay, and asset indexing.
    /// 为转录预览、回放与资产索引保留的宿主管理附件。
    attachment: ConversationAttachment,
    /// Preferred local filesystem path returned to the model for immediate follow-up actions.
    /// 返回给模型、供其立即执行后续操作的首选本地文件系统路径。
    model_local_path: String,
}

/// Builds the parameter schema for the semantic `image_generation` tool.
/// 构建语义 `image_generation` 工具的参数模式。
pub fn configured_image_generation_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
    conversation_attachments: &[ConversationAttachment],
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::ImageGeneration))
        .collect::<Vec<_>>();
    let image_names =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Image);
    let subject_image_description = attachment_selector_description(
        "Optional reference image attachment selector.",
        "image",
        &image_names,
    );
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Prompt used for image generation." },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "count": { "type": "integer", "minimum": 1, "maximum": 4, "default": 1 },
            "aspect_ratio": { "type": "string", "description": "Optional aspect ratio such as `1:1`, `16:9`, or `9:16`." },
            "seed": { "type": "integer", "minimum": 0 },
            "subject_image": { "type": "string", "description": subject_image_description },
            "prompt_optimizer": { "type": "boolean", "default": false },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        },
        "required": ["prompt"]
    })
}

/// Builds the parameter schema for the semantic `video_generation` tool.
/// 构建语义 `video_generation` 工具的参数模式。
pub fn configured_video_generation_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
    conversation_attachments: &[ConversationAttachment],
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::VideoGeneration))
        .collect::<Vec<_>>();
    let image_names =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Image);
    let first_image_description = attachment_selector_description(
        "Optional first-frame image selector.",
        "image",
        &image_names,
    );
    let last_image_description = attachment_selector_description(
        "Optional last-frame image selector.",
        "image",
        &image_names,
    );
    let subject_image_description = attachment_selector_description(
        "Optional subject reference image selector.",
        "image",
        &image_names,
    );
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Prompt used for video generation." },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "first_image": { "type": "string", "description": first_image_description },
            "last_image": { "type": "string", "description": last_image_description },
            "subject_image": { "type": "string", "description": subject_image_description },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        },
        "required": ["prompt"]
    })
}

/// Builds the parameter schema for the semantic `speech_synthesis` tool.
/// 构建语义 `speech_synthesis` 工具的参数模式。
pub fn configured_speech_synthesis_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::SpeechSynthesis))
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Input text to synthesize into speech." },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "voice": { "type": "string", "description": "Optional `voice_id` value." },
            "language": { "type": "string", "description": "Optional `language_boost` code." },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        },
        "required": ["text"]
    })
}

/// Builds the parameter schema for the semantic `music_generation` tool.
/// 构建语义 `music_generation` 工具的参数模式。
pub fn configured_music_generation_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::MusicGeneration))
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Optional music style prompt." },
            "lyrics": { "type": "string", "description": "Optional lyrics text. If omitted, use `instrumental` or `lyrics_optimizer` instead when appropriate." },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "instrumental": { "type": "boolean", "default": false },
            "lyrics_optimizer": { "type": "boolean", "default": false },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        }
    })
}

/// Builds the parameter schema for the semantic `music_cover` tool.
/// 构建语义 `music_cover` 工具的参数模式。
pub fn configured_music_cover_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
    conversation_attachments: &[ConversationAttachment],
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::MusicCover))
        .collect::<Vec<_>>();
    let audio_names =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Audio);
    let audio_description =
        attachment_selector_description("Reference audio selector.", "audio", &audio_names);
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Style prompt for the music cover." },
            "audio": { "type": "string", "description": audio_description },
            "lyrics": { "type": "string", "description": "Optional lyrics override." },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        },
        "required": ["prompt", "audio"]
    })
}

/// Executes one host-configured image-generation request and stages the generated images into the managed attachment store.
/// 执行一次宿主配置的图片生成请求，并将生成结果暂存到托管附件存储中。
pub async fn execute_configured_image_generation(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_image_generation_invocation(args)?;
    let targets = resolve_vendor_targets(
        &configuration,
        AiToolKind::ImageGeneration,
        invocation.target.as_deref(),
    )?;
    let client = build_http_client()?;

    let mut errors = Vec::new();
    for target in targets {
        match execute_image_generation_against_target(
            &client,
            &workspace_path,
            &target,
            &invocation,
            &conversation_attachments,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{}: {error}", target.target_selector)),
        }
    }

    Err(format!(
        "All configured `image_generation` targets failed: {}.",
        errors.join("; ")
    ))
}

/// Executes one host-configured video-generation request and stages the downloaded video into the managed attachment store.
/// 执行一次宿主配置的视频生成请求，并将下载后的视频暂存到托管附件存储中。
pub async fn execute_configured_video_generation(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_video_generation_invocation(args)?;
    let targets = resolve_vendor_targets(
        &configuration,
        AiToolKind::VideoGeneration,
        invocation.target.as_deref(),
    )?;
    let client = build_http_client()?;

    let mut errors = Vec::new();
    for target in targets {
        match execute_video_generation_against_target(
            &client,
            &workspace_path,
            &target,
            &invocation,
            &conversation_attachments,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{}: {error}", target.target_selector)),
        }
    }

    Err(format!(
        "All configured `video_generation` targets failed: {}.",
        errors.join("; ")
    ))
}

/// Executes one host-configured speech-synthesis request and stages the generated audio into the managed attachment store.
/// 执行一次宿主配置的语音合成请求，并将生成音频暂存到托管附件存储中。
pub async fn execute_configured_speech_synthesis(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_speech_synthesis_invocation(args)?;
    let targets = resolve_vendor_targets(
        &configuration,
        AiToolKind::SpeechSynthesis,
        invocation.target.as_deref(),
    )?;
    let client = build_http_client()?;

    let mut errors = Vec::new();
    for target in targets {
        match execute_speech_synthesis_against_target(
            &client,
            &workspace_path,
            &target,
            &invocation,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{}: {error}", target.target_selector)),
        }
    }

    Err(format!(
        "All configured `speech_synthesis` targets failed: {}.",
        errors.join("; ")
    ))
}

/// Executes one host-configured music-generation request and stages the generated audio into the managed attachment store.
/// 执行一次宿主配置的音乐生成请求，并将生成音频暂存到托管附件存储中。
pub async fn execute_configured_music_generation(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_music_generation_invocation(args)?;
    let targets = resolve_vendor_targets(
        &configuration,
        AiToolKind::MusicGeneration,
        invocation.target.as_deref(),
    )?;
    let client = build_http_client()?;

    let mut errors = Vec::new();
    for target in targets {
        match execute_music_generation_against_target(
            &client,
            &workspace_path,
            &target,
            &invocation,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{}: {error}", target.target_selector)),
        }
    }

    Err(format!(
        "All configured `music_generation` targets failed: {}.",
        errors.join("; ")
    ))
}

/// Executes one host-configured music-cover request and stages the generated audio into the managed attachment store.
/// 执行一次宿主配置的音乐翻唱请求，并将生成音频暂存到托管附件存储中。
pub async fn execute_configured_music_cover(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_music_cover_invocation(args)?;
    let targets = resolve_vendor_targets(
        &configuration,
        AiToolKind::MusicCover,
        invocation.target.as_deref(),
    )?;
    let client = build_http_client()?;

    let mut errors = Vec::new();
    for target in targets {
        match execute_music_cover_against_target(
            &client,
            &workspace_path,
            &target,
            &invocation,
            &conversation_attachments,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{}: {error}", target.target_selector)),
        }
    }

    Err(format!(
        "All configured `music_cover` targets failed: {}.",
        errors.join("; ")
    ))
}

/// Builds one shared HTTP client used by all host-side capability adapters in this module.
/// 构建本模块全部宿主侧能力适配器共享使用的 HTTP 客户端。
fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|error| format!("Failed to build the AI capability HTTP client: {error}"))
}

/// Resolves one ordered vendor-target attempt list from the configured capability routing state.
/// 根据当前能力路由配置解析一组有序的供应商目标尝试列表。
fn resolve_vendor_targets(
    configuration: &PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    explicit_target: Option<&str>,
) -> Result<Vec<ResolvedVendorTarget>, String> {
    let target_order = if let Some(explicit_target) = explicit_target
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("auto"))
    {
        vec![explicit_target.to_string()]
    } else {
        configuration.usable_targets_for_kind(capability_kind)
    };
    if target_order.is_empty() {
        return Err(format!(
            "No usable targets are configured for `{}`.",
            capability_kind.key()
        ));
    }

    let mut resolved = Vec::new();
    for target_selector in target_order {
        let Some((vendor_id, _)) = split_vendor_target_selector(&target_selector) else {
            continue;
        };
        let Some(vendor) = configuration.vendor_by_id(vendor_id).cloned() else {
            continue;
        };
        if !vendor.enabled {
            continue;
        }
        let Some(binding) = vendor
            .matching_binding(capability_kind, &target_selector)
            .cloned()
        else {
            continue;
        };
        resolved.push(ResolvedVendorTarget {
            vendor,
            binding,
            target_selector,
        });
    }

    if resolved.is_empty() {
        return Err(format!(
            "The requested target for `{}` does not exist or is not available. Available targets: {}.",
            capability_kind.key(),
            configuration
                .usable_targets_for_kind(capability_kind)
                .join(", ")
        ));
    }

    Ok(resolved)
}

/// Executes one image-generation request against one concrete target.
/// 针对单个具体目标执行一次图片生成请求。
async fn execute_image_generation_against_target(
    client: &reqwest::Client,
    workspace_path: &str,
    target: &ResolvedVendorTarget,
    invocation: &ImageGenerationInvocation,
    conversation_attachments: &[ConversationAttachment],
) -> Result<VulcanToolInvocationResult, String> {
    ensure_minimax_vendor(target, AiToolKind::ImageGeneration)?;
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax Image Generation")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/image_generation", false)?;

    let subject_reference = if let Some(image_selector) = invocation.subject_image.as_deref() {
        let attachment = resolve_attachment_by_name_or_local_path(
            conversation_attachments,
            ConversationAttachmentKind::Image,
            image_selector,
            workspace_path,
        )
        .ok_or_else(|| {
            format!(
                "The reference image selector `{image_selector}` requested by `image_generation` does not exist."
            )
        })?;
        let payload =
            match read_provider_attachment(&attachment).map_err(|error| error.to_string())? {
                Some(payload) => payload,
                None => return Ok(missing_attachment_result(&attachment)),
            };
        Some(json!([{ "type": "character", "image_url": payload.data_url }]))
    } else {
        None
    };

    let mut request_body = json!({
        "model": target.binding.model_id(),
        "prompt": invocation.prompt,
        "n": invocation.count.max(1),
        "response_format": "base64",
    });
    if let Some(aspect_ratio) = invocation.aspect_ratio.as_deref() {
        request_body["aspect_ratio"] = Value::String(aspect_ratio.to_string());
    }
    if let Some(seed) = invocation.seed {
        request_body["seed"] = json!(seed);
    }
    if invocation.prompt_optimizer {
        request_body["prompt_optimizer"] = Value::Bool(true);
    }
    if let Some(subject_reference) = subject_reference {
        request_body["subject_reference"] = subject_reference;
    }

    let response_body = send_json_request(
        client,
        &endpoint,
        &api_key,
        &request_body,
        "MiniMax Image Generation",
    )
    .await?;
    let images = response_body
        .pointer("/data/image_base64")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if images.is_empty() {
        return Err(format!(
            "MiniMax Image Generation returned no `image_base64` payload: {}",
            truncate_error_text(&response_body.to_string(), 240)
        ));
    }

    // Total returned image count used for caller-visible file-name numbering when a shared output stem is supplied.
    // 当提供共享输出名称主体时，用于调用方可见文件名编号的返回图片总数。
    let output_count = images.len();
    let mut outputs = Vec::new();
    for (index, image_value) in images.iter().enumerate() {
        let base64_image = image_value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("MiniMax Image Generation returned empty image data."))?;
        let image_bytes = BASE64_STANDARD.decode(base64_image).map_err(|error| {
            format!("Failed to decode MiniMax Image Generation base64 image data: {error}")
        })?;
        // Provider-suggested file name reused when the caller did not supply a custom output stem.
        // 当调用方未提供自定义输出名称主体时复用的提供方建议文件名。
        let provider_file_name = if output_count > 1 {
            format!("minimax-image-{:03}.jpg", index + 1)
        } else {
            String::from("minimax-image.jpg")
        };
        outputs.push(stage_generated_asset_bytes(
            workspace_path,
            "image/jpeg",
            &provider_file_name,
            &image_bytes,
            &with_optional_output_sequence(&invocation.output, output_count, index),
        )?);
    }

    Ok(build_generated_result(
        AiToolKind::ImageGeneration,
        &target.target_selector,
        Some(json!({
            "task_id": response_body.pointer("/data/task_id").and_then(Value::as_str),
            "success_count": response_body.pointer("/data/success_count").and_then(Value::as_u64),
            "failed_count": response_body.pointer("/data/failed_count").and_then(Value::as_u64),
        })),
        outputs,
    ))
}

/// Executes one video-generation request against one concrete target.
/// 针对单个具体目标执行一次视频生成请求。
async fn execute_video_generation_against_target(
    client: &reqwest::Client,
    workspace_path: &str,
    target: &ResolvedVendorTarget,
    invocation: &VideoGenerationInvocation,
    conversation_attachments: &[ConversationAttachment],
) -> Result<VulcanToolInvocationResult, String> {
    ensure_minimax_vendor(target, AiToolKind::VideoGeneration)?;
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax Video Generation")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/video_generation", false)?;

    let first_frame_image = resolve_optional_image_data_url(
        workspace_path,
        conversation_attachments,
        invocation.first_image.as_deref(),
    )?;
    let last_frame_image = resolve_optional_image_data_url(
        workspace_path,
        conversation_attachments,
        invocation.last_image.as_deref(),
    )?;
    let subject_image = resolve_optional_image_data_url(
        workspace_path,
        conversation_attachments,
        invocation.subject_image.as_deref(),
    )?;

    let model_id = target.binding.model_id();
    if model_id.eq_ignore_ascii_case("MiniMax-Hailuo-2.3-Fast") && first_frame_image.is_none() {
        return Err(String::from(
            "MiniMax-Hailuo-2.3-Fast supports image-to-video only, so `first_image` is required.",
        ));
    }
    if last_frame_image.is_some() && first_frame_image.is_none() {
        return Err(String::from(
            "`first_image` must also be provided when `last_image` is set.",
        ));
    }
    if last_frame_image.is_some() && subject_image.is_some() {
        return Err(String::from(
            "`last_image` and `subject_image` cannot be used at the same time.",
        ));
    }
    if model_id.eq_ignore_ascii_case("MiniMax-Hailuo-02")
        && (first_frame_image.is_none() || last_frame_image.is_none())
    {
        return Err(String::from(
            "MiniMax-Hailuo-02 requires both `first_image` and `last_image`.",
        ));
    }
    if model_id.eq_ignore_ascii_case("S2V-01") && subject_image.is_none() {
        return Err(String::from("S2V-01 requires `subject_image`."));
    }

    let mut request_body = json!({
        "model": model_id,
        "prompt": invocation.prompt,
    });
    if let Some(first_frame_image) = first_frame_image {
        request_body["first_frame_image"] = Value::String(first_frame_image);
    }
    if let Some(last_frame_image) = last_frame_image {
        request_body["last_frame_image"] = Value::String(last_frame_image);
    }
    if let Some(subject_image) = subject_image {
        request_body["subject_reference"] =
            json!([{ "type": "character", "image": [subject_image] }]);
    }

    let response_body = send_json_request(
        client,
        &endpoint,
        &api_key,
        &request_body,
        "MiniMax Video Generation",
    )
    .await?;
    let task_id = response_body
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax Video Generation returned no `task_id`."))?
        .to_string();

    let task_payload = poll_minimax_video_task(client, &target.vendor, &api_key, &task_id).await?;
    let file_id = task_payload
        .get("file_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            String::from("The completed MiniMax video task did not return a `file_id`.")
        })?;
    let download_url =
        fetch_minimax_file_download_url(client, &target.vendor, &api_key, file_id).await?;
    let video_bytes = download_binary(&download_url, "MiniMax Video Download").await?;
    let output = stage_generated_asset_bytes(
        workspace_path,
        "video/mp4",
        &format!("minimax-video-{task_id}.mp4"),
        &video_bytes,
        &invocation.output,
    )?;

    Ok(build_generated_result(
        AiToolKind::VideoGeneration,
        &target.target_selector,
        Some(json!({
            "task_id": task_id,
            "status": task_payload.get("status").cloned(),
            "file_id": file_id,
        })),
        vec![output],
    ))
}

/// Executes one speech-synthesis request against one concrete target.
/// 针对单个具体目标执行一次语音合成请求。
async fn execute_speech_synthesis_against_target(
    client: &reqwest::Client,
    workspace_path: &str,
    target: &ResolvedVendorTarget,
    invocation: &SpeechSynthesisInvocation,
) -> Result<VulcanToolInvocationResult, String> {
    ensure_minimax_vendor(target, AiToolKind::SpeechSynthesis)?;
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax Speech Synthesis")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/t2a_v2", false)?;
    let mut request_body = json!({
        "model": target.binding.model_id(),
        "text": invocation.text,
        "voice_setting": {
            "voice_id": invocation.voice.clone().unwrap_or_else(|| String::from("English_expressive_narrator"))
        },
        "audio_setting": {
            "format": invocation.format,
            "sample_rate": 32000,
            "bitrate": 128000,
            "channel": 1
        },
        "output_format": "hex"
    });
    if let Some(language) = invocation.language.as_deref() {
        request_body["language_boost"] = Value::String(language.to_string());
    }

    let response_body = send_json_request(
        client,
        &endpoint,
        &api_key,
        &request_body,
        "MiniMax Speech Synthesis",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax Speech Synthesis returned no `audio` field."))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax Speech Synthesis")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let output = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-speech.{}", invocation.format),
        &audio_bytes,
        &invocation.output,
    )?;

    Ok(build_generated_result(
        AiToolKind::SpeechSynthesis,
        &target.target_selector,
        Some(json!({
            "word_count": response_body.pointer("/extra_info/word_count").and_then(Value::as_u64),
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
        })),
        vec![output],
    ))
}

/// Executes one music-generation request against one concrete target.
/// 针对单个具体目标执行一次音乐生成请求。
async fn execute_music_generation_against_target(
    client: &reqwest::Client,
    workspace_path: &str,
    target: &ResolvedVendorTarget,
    invocation: &MusicGenerationInvocation,
) -> Result<VulcanToolInvocationResult, String> {
    ensure_minimax_vendor(target, AiToolKind::MusicGeneration)?;
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax Music Generation")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/music_generation", false)?;

    if invocation.instrumental && invocation.lyrics.is_some() {
        return Err(String::from(
            "`instrumental` and `lyrics` cannot be used together.",
        ));
    }
    if invocation.lyrics_optimizer && (invocation.lyrics.is_some() || invocation.instrumental) {
        return Err(String::from(
            "`lyrics_optimizer` cannot be used together with `lyrics` or `instrumental`.",
        ));
    }
    if invocation.prompt.is_none()
        && invocation.lyrics.is_none()
        && !invocation.instrumental
        && !invocation.lyrics_optimizer
    {
        return Err(String::from(
            "`music_generation` requires at least one of `prompt`, `lyrics`, `instrumental`, or `lyrics_optimizer`.",
        ));
    }

    let request_body = json!({
        "model": target.binding.model_id(),
        "prompt": invocation.prompt,
        "lyrics": invocation.lyrics,
        "is_instrumental": invocation.instrumental.then_some(true),
        "lyrics_optimizer": invocation.lyrics_optimizer.then_some(true),
        "audio_setting": {
            "format": invocation.format,
            "sample_rate": 44100,
            "bitrate": 256000
        },
        "output_format": "hex"
    });
    let response_body = send_json_request(
        client,
        &endpoint,
        &api_key,
        &request_body,
        "MiniMax Music Generation",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax Music Generation returned no `audio` field."))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax Music Generation")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let output = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-music.{}", invocation.format),
        &audio_bytes,
        &invocation.output,
    )?;

    Ok(build_generated_result(
        AiToolKind::MusicGeneration,
        &target.target_selector,
        Some(json!({
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
            "audio_size": response_body.pointer("/extra_info/audio_size").and_then(Value::as_u64),
        })),
        vec![output],
    ))
}

/// Executes one music-cover request against one concrete target.
/// 针对单个具体目标执行一次音乐翻唱请求。
async fn execute_music_cover_against_target(
    client: &reqwest::Client,
    workspace_path: &str,
    target: &ResolvedVendorTarget,
    invocation: &MusicCoverInvocation,
    conversation_attachments: &[ConversationAttachment],
) -> Result<VulcanToolInvocationResult, String> {
    ensure_minimax_vendor(target, AiToolKind::MusicCover)?;
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax Music Cover")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/music_generation", false)?;
    let attachment = resolve_attachment_by_name_or_local_path(
        conversation_attachments,
        ConversationAttachmentKind::Audio,
        &invocation.audio,
        workspace_path,
    )
    .ok_or_else(|| {
        format!(
            "The reference audio selector `{}` requested by `music_cover` does not exist.",
            invocation.audio
        )
    })?;
    let payload = match read_provider_attachment(&attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(missing_attachment_result(&attachment)),
    };
    let request_body = json!({
        "model": target.binding.model_id(),
        "prompt": invocation.prompt,
        "lyrics": invocation.lyrics,
        "audio_base64": payload.base64_data,
        "audio_setting": {
            "format": invocation.format,
            "sample_rate": 44100,
            "bitrate": 256000,
            "channel": 2
        },
        "output_format": "hex"
    });
    let response_body = send_json_request(
        client,
        &endpoint,
        &api_key,
        &request_body,
        "MiniMax Music Cover",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax Music Cover returned no `audio` field."))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax Music Cover")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let output = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-cover.{}", invocation.format),
        &audio_bytes,
        &invocation.output,
    )?;

    Ok(build_generated_result(
        AiToolKind::MusicCover,
        &target.target_selector,
        Some(json!({
            "reference_audio": invocation.audio,
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
        })),
        vec![output],
    ))
}

/// Sends one JSON POST request to the target MiniMax endpoint and validates both HTTP and `base_resp` status fields.
/// 向目标 MiniMax 端点发送一次 JSON POST 请求，并同时校验 HTTP 与 `base_resp` 状态字段。
async fn send_json_request(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    body: &Value,
    provider_label: &str,
) -> Result<Value, String> {
    let response = client
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{provider_label} request failed: {error}"))?;
    parse_json_response(response, provider_label).await
}

/// Parses one HTTP JSON response and translates MiniMax-style `base_resp` errors into host-readable messages.
/// 解析单条 HTTP JSON 响应，并把 MiniMax 风格 `base_resp` 错误转换为宿主可读消息。
async fn parse_json_response(
    response: reqwest::Response,
    provider_label: &str,
) -> Result<Value, String> {
    let status = response.status();
    let body_text = response.text().await.map_err(|error| {
        format!("Failed to read the response body from {provider_label}: {error}")
    })?;
    if !status.is_success() {
        return Err(format!(
            "{provider_label} returned HTTP {}: {}",
            status,
            truncate_error_text(&body_text, 400)
        ));
    }
    let payload: Value = serde_json::from_str(&body_text)
        .map_err(|error| format!("Failed to parse JSON returned by {provider_label}: {error}"))?;
    if let Some(status_code) = payload
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
    {
        if status_code != 0 {
            let status_msg = payload
                .pointer("/base_resp/status_msg")
                .and_then(Value::as_str)
                .unwrap_or("Unknown error");
            return Err(format!(
                "{provider_label} returned business error {status_code}: {}",
                status_msg.trim()
            ));
        }
    }
    Ok(payload)
}

/// Polls the MiniMax video-task endpoint until the task finishes or times out.
/// 轮询 MiniMax 视频任务端点，直到任务完成或超时。
async fn poll_minimax_video_task(
    client: &reqwest::Client,
    vendor: &PersistedAiCapabilityVendorEntry,
    api_key: &str,
    task_id: &str,
) -> Result<Value, String> {
    let mut elapsed_seconds = 0u64;
    loop {
        let endpoint = resolve_minimax_file_or_task_endpoint(
            vendor,
            "/v1/query/video_generation",
            &[("task_id", task_id)],
        )?;
        let response = client
            .get(endpoint)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|error| format!("MiniMax Video Task Query failed: {error}"))?;
        let payload = parse_json_response(response, "MiniMax Video Task Query").await?;
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match status {
            "Success" => return Ok(payload),
            "Failed" => {
                return Err(format!(
                    "MiniMax video task failed: {}",
                    truncate_error_text(&payload.to_string(), 240)
                ));
            }
            _ => {}
        }
        if elapsed_seconds >= 300 {
            return Err(String::from(
                "Timed out while waiting for the MiniMax video task to finish (300 seconds).",
            ));
        }
        sleep(Duration::from_secs(5)).await;
        elapsed_seconds += 5;
    }
}

/// Fetches the final download URL for one MiniMax file identifier.
/// 获取单个 MiniMax 文件标识的最终下载 URL。
async fn fetch_minimax_file_download_url(
    client: &reqwest::Client,
    vendor: &PersistedAiCapabilityVendorEntry,
    api_key: &str,
    file_id: &str,
) -> Result<String, String> {
    let endpoint = resolve_minimax_file_or_task_endpoint(
        vendor,
        "/v1/files/retrieve",
        &[("file_id", file_id)],
    )?;
    let response = client
        .get(endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|error| format!("MiniMax File Download URL Query failed: {error}"))?;
    let payload = parse_json_response(response, "MiniMax File Download URL Query").await?;
    payload
        .pointer("/file/download_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            String::from("MiniMax file download URL response is missing `download_url`.")
        })
}

/// Downloads one binary payload from the provided URL.
/// 从给定 URL 下载一份二进制载荷。
async fn download_binary(url: &str, label: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("{label} request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("Unable to read the response body"));
        return Err(format!(
            "{label} returned HTTP {}: {}",
            status,
            truncate_error_text(&body_text, 240)
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("Failed to read response bytes from {label}: {error}"))
}

/// Resolves the correct MiniMax endpoint from one shared vendor root and one capability-specific path.
/// 根据共享供应商根地址与能力专属路径解析正确的 MiniMax 端点。
fn resolve_minimax_endpoint(
    vendor: &PersistedAiCapabilityVendorEntry,
    capability_path: &str,
    allow_regular_text_api: bool,
) -> Result<String, String> {
    let region = vendor.resolved_region();
    let host = if region.eq_ignore_ascii_case("cn") {
        "https://api.minimaxi.com"
    } else {
        "https://api.minimax.io"
    };
    let base_url = vendor.base_url.trim();
    if base_url.is_empty() {
        return Ok(format!("{host}{capability_path}"));
    }

    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| format!("MiniMax base URL is invalid: {error}"))?;
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if !allow_regular_text_api && minimax_path_looks_like_regular_text_api(&normalized_path) {
        return Err(String::from(
            "This MiniMax capability uses dedicated capability endpoints and does not support regular text-model API URLs. Provide a MiniMax root URL such as `https://api.minimax.io`, or leave the field empty to use the default.",
        ));
    }
    url.set_path(capability_path);
    url.set_query(None);
    Ok(url.to_string())
}

/// Resolves one MiniMax endpoint that appends query parameters to a capability root path.
/// 解析一个需要在能力根路径后附加查询参数的 MiniMax 端点。
fn resolve_minimax_file_or_task_endpoint(
    vendor: &PersistedAiCapabilityVendorEntry,
    capability_path: &str,
    query_pairs: &[(&str, &str)],
) -> Result<String, String> {
    let endpoint = resolve_minimax_endpoint(vendor, capability_path, false)?;
    let mut url = reqwest::Url::parse(&endpoint)
        .map_err(|error| format!("MiniMax endpoint is invalid: {error}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query_pairs {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

/// Returns whether one MiniMax path obviously points at a regular text-generation endpoint.
/// 返回单个 MiniMax 路径是否明显指向普通文本生成接口。
fn minimax_path_looks_like_regular_text_api(path: &str) -> bool {
    let normalized = path.trim().to_ascii_lowercase();
    normalized.contains("chatcompletion")
        || normalized.contains("/text/")
        || normalized.contains("/anthropic")
        || normalized.contains("/openai")
        || normalized.contains("/responses")
        || normalized.contains("/messages")
}

/// Ensures the requested target currently belongs to the MiniMax vendor and matches the expected capability kind.
/// 确保请求目标当前属于 MiniMax 供应商，且与预期能力类型一致。
fn ensure_minimax_vendor(
    target: &ResolvedVendorTarget,
    capability_kind: AiToolKind,
) -> Result<(), String> {
    if target.vendor.vendor_id != "minimax" || target.binding.capability_kind != capability_kind {
        return Err(format!(
            "Only MiniMax `{}` capability targets are currently implemented.",
            capability_kind.key()
        ));
    }
    if crate::ai_capability_provider_catalog::ai_capability_provider_definition_by_target(
        &target.vendor.vendor_id,
        capability_kind,
        target.binding.target_alias(),
    )
    .is_none()
    {
        return Err(format!(
            "The MiniMax `{}` target `{}` is not registered in the capability resource catalog.",
            capability_kind.key(),
            target.binding.target_alias()
        ));
    }
    Ok(())
}

/// Resolves and validates the shared API key for one vendor entry.
/// 解析并校验单个供应商条目的共享 API Key。
fn require_vendor_api_key(
    vendor: &PersistedAiCapabilityVendorEntry,
    provider_label: &str,
) -> Result<String, String> {
    vendor
        .resolved_api_key()?
        .ok_or_else(|| format!("{provider_label} is missing a valid API key."))
}

/// Stages one generated binary asset into the managed attachment store.
/// 将单个生成得到的二进制资产暂存到托管附件存储中。
fn stage_generated_asset_bytes(
    workspace_path: &str,
    mime_type: &str,
    suggested_file_name: &str,
    bytes: &[u8],
    output_options: &GeneratedAssetOutputOptions,
) -> Result<StagedGeneratedAsset, String> {
    let descriptor = attachment_descriptor_from_mime_type(mime_type).ok_or_else(|| {
        format!("Generated assets with MIME type `{mime_type}` are not currently supported.")
    })?;
    // Caller-visible preferred file name whose extension is normalized from the resolved media descriptor.
    // 面向调用方的首选文件名，其扩展名会根据已解析媒体描述符完成规范化。
    let requested_file_name =
        resolve_requested_output_file_name(suggested_file_name, output_options);
    if let Some(output_dir) = output_options.output_dir.as_deref() {
        // Filesystem directory chosen for the caller-visible exported copy.
        // 为调用方可见导出副本选定的文件系统目录。
        let export_directory = resolve_generated_output_directory(workspace_path, output_dir);
        // Exported file path returned to the model for immediate follow-up work.
        // 回传给模型、供其立即执行后续工作的导出文件路径。
        let exported_path = write_attachment_bytes_to_directory(
            &descriptor,
            &export_directory,
            &requested_file_name,
            bytes,
        )?;
        // Managed attachment copy retained for transcript rendering, replay, and attachment indexing.
        // 为转录渲染、回放与附件索引保留的托管附件副本。
        let attachment = stage_attachment_file(workspace_path, &exported_path)?;
        return Ok(StagedGeneratedAsset {
            attachment,
            model_local_path: normalize_workspace_path_text(&exported_path.to_string_lossy()),
        });
    }

    // Managed attachment copy used both as the durable host asset and as the model-visible path when no export directory is requested.
    // 在未请求导出目录时，同时作为宿主持久资产与模型可见路径的托管附件副本。
    let attachment = stage_attachment_bytes(
        workspace_path,
        &descriptor,
        &requested_file_name,
        None,
        bytes,
    )?;
    Ok(StagedGeneratedAsset {
        model_local_path: normalize_workspace_path_text(&attachment.local_path),
        attachment,
    })
}

/// Resolves one caller-visible requested file name by favoring the optional caller-supplied stem while still letting the host control the final extension.
/// 解析一条面向调用方的请求文件名：优先采用可选的调用方名称主体，同时仍由宿主控制最终扩展名。
fn resolve_requested_output_file_name(
    suggested_file_name: &str,
    output_options: &GeneratedAssetOutputOptions,
) -> String {
    output_options
        .output_file_stem
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| suggested_file_name.to_string())
}

/// Adds one numbered suffix to the optional caller-supplied output stem when one request emits multiple files.
/// 当一次请求会产出多个文件时，为可选的调用方输出名称主体追加编号后缀。
fn with_optional_output_sequence(
    output_options: &GeneratedAssetOutputOptions,
    output_count: usize,
    output_index: usize,
) -> GeneratedAssetOutputOptions {
    if output_count <= 1 {
        return output_options.clone();
    }

    // Cloned output options updated only when a caller-supplied file-name stem exists.
    // 仅在存在调用方文件名主体时才会被更新的克隆输出选项。
    let mut sequenced_output_options = output_options.clone();
    if let Some(output_file_stem) = output_options
        .output_file_stem
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sequenced_output_options.output_file_stem =
            Some(format!("{output_file_stem}-{:03}", output_index + 1));
    }
    sequenced_output_options
}

/// Resolves one caller-selected output directory into an absolute filesystem path by treating relative paths as workspace-relative.
/// 将调用方选定的输出目录解析为绝对文件系统路径，并把相对路径视作相对工作区路径。
fn resolve_generated_output_directory(workspace_path: &str, output_dir: &str) -> PathBuf {
    // Trimmed output-directory text used for path resolution.
    // 用于路径解析的裁剪后输出目录文本。
    let trimmed_output_dir = output_dir.trim();
    // Raw output-directory path supplied by the caller.
    // 调用方提供的原始输出目录路径。
    let requested_path = PathBuf::from(trimmed_output_dir);
    if requested_path.is_absolute() {
        return canonicalize_workspace_path(&requested_path);
    }

    // Workspace anchor used for resolving caller-relative output directories.
    // 用于解析调用方相对输出目录的工作区锚点路径。
    let workspace_anchor = resolve_workspace_anchor_directory(workspace_path);
    canonicalize_workspace_path(&workspace_anchor.join(requested_path))
}

/// Resolves the directory anchor used for workspace-relative generated-output paths.
/// 解析工作区相对生成输出路径使用的目录锚点。
fn resolve_workspace_anchor_directory(workspace_path: &str) -> PathBuf {
    // Trimmed workspace-path text captured from the active execution preparation.
    // 从当前执行准备中捕获的裁剪后工作区路径文本。
    let trimmed_workspace_path = workspace_path.trim();
    if trimmed_workspace_path.is_empty() {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

    // Canonical workspace candidate that may point either to the directory itself or to one file inside it.
    // 规范化后的工作区候选路径，它既可能直接指向目录，也可能指向目录内部的某个文件。
    let workspace_candidate = canonicalize_workspace_path(Path::new(trimmed_workspace_path));
    if workspace_candidate.is_dir() {
        return workspace_candidate;
    }
    workspace_candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(workspace_candidate)
}

/// Builds one structured tool result that includes both model-visible output and richer host metadata for generated assets.
/// 构建一条同时包含模型可见输出与更丰富宿主元数据的生成资产工具结果。
fn build_generated_result(
    capability_kind: AiToolKind,
    target_selector: &str,
    extra: Option<Value>,
    outputs: Vec<StagedGeneratedAsset>,
) -> VulcanToolInvocationResult {
    let host_outputs = outputs
        .iter()
        .map(|output| attachment_to_json(&output.attachment))
        .collect::<Vec<_>>();
    let model_outputs = outputs
        .iter()
        .map(generated_asset_to_model_output_json)
        .collect::<Vec<_>>();
    let model_output = json!({
        "ok": true,
        "capability": capability_kind.key(),
        "target": target_selector,
        "outputs": model_outputs,
        "extra": extra.clone().unwrap_or(Value::Null),
    });
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from("generated_attachments"),
            payload: json!({
                "capability": capability_kind.key(),
                "target": target_selector,
                "outputs": host_outputs,
                "extra": extra.unwrap_or(Value::Null),
            }),
        },
    )
}

/// Builds one successful-but-degraded result for attachments whose managed replay file has already disappeared.
/// 为托管回放文件已消失的附件构建一条成功但降级的结果。
fn missing_attachment_result(attachment: &ConversationAttachment) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(Value::String(
        build_missing_attachment_replay_text(attachment),
    ))
}

/// Serializes one staged attachment into a compact JSON object carried by model-visible and host-only tool results.
/// 将单条已暂存附件序列化为紧凑 JSON 对象，供模型可见与宿主专用工具结果共同复用。
fn attachment_to_json(attachment: &ConversationAttachment) -> Value {
    json!({
        "kind": attachment.kind.modality_name(),
        "file_name": attachment.file_name,
        "local_path": attachment.local_path,
        "source_path": attachment.source_path,
        "mime_type": attachment.mime_type,
        "asset_id": attachment.asset_id,
        "byte_len": attachment.byte_len,
        "preview_local_path": attachment.preview_local_path,
    })
}

/// Serializes one generated output into a model-visible JSON object that includes the preferred local filesystem path for follow-up tool calls.
/// 将单条生成输出序列化为模型可见 JSON 对象，并携带供后续工具调用使用的首选本地文件系统路径。
fn generated_asset_to_model_output_json(output: &StagedGeneratedAsset) -> Value {
    json!({
        "kind": output.attachment.kind.modality_name(),
        "file_name": output.attachment.file_name,
        "mime_type": output.attachment.mime_type,
        "asset_id": output.attachment.asset_id,
        "byte_len": output.attachment.byte_len,
        "local_path": output.model_local_path,
    })
}

/// Resolves one optional image selector into a Data URL accepted by MiniMax multimodal generation endpoints.
/// 将单个可选图片选择器解析为 MiniMax 多模态生成端点接受的 Data URL。
fn resolve_optional_image_data_url(
    workspace_path: &str,
    attachments: &[ConversationAttachment],
    image_selector: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(image_selector) = image_selector
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let attachment = resolve_attachment_by_name_or_local_path(
        attachments,
        ConversationAttachmentKind::Image,
        image_selector,
        workspace_path,
    )
    .ok_or_else(|| format!("The image selector `{image_selector}` does not exist."))?;
    let payload = match read_provider_attachment(&attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    Ok(Some(payload.data_url))
}

/// Resolves one staged attachment by user-visible file name, asset id, or managed local-path file name.
/// 根据用户可见文件名、资产标识或托管本地文件名解析单条已暂存附件。
fn resolve_attachment_by_name<'a>(
    attachments: &'a [ConversationAttachment],
    expected_kind: &ConversationAttachmentKind,
    selector: &str,
) -> Option<&'a ConversationAttachment> {
    let normalized_selector = selector.trim().to_ascii_lowercase();
    if normalized_selector.is_empty() {
        return None;
    }

    attachments.iter().find(|attachment| {
        attachment.kind == *expected_kind
            && (attachment
                .file_name
                .eq_ignore_ascii_case(&normalized_selector)
                || attachment.local_path.eq_ignore_ascii_case(selector.trim())
                || attachment
                    .asset_id
                    .as_deref()
                    .is_some_and(|asset_id| asset_id.eq_ignore_ascii_case(&normalized_selector))
                || Path::new(&attachment.local_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|file_name| file_name.eq_ignore_ascii_case(&normalized_selector)))
    })
}

/// Resolves one attachment selector either from the current host-visible attachment list or from one direct local file path returned by a previous tool call.
/// 从当前宿主可见附件列表或前序工具调用返回的直接本地文件路径中解析单条附件选择器。
fn resolve_attachment_by_name_or_local_path(
    attachments: &[ConversationAttachment],
    expected_kind: ConversationAttachmentKind,
    selector: &str,
    workspace_path: &str,
) -> Option<ConversationAttachment> {
    if let Some(attachment) = resolve_attachment_by_name(attachments, &expected_kind, selector) {
        return Some(attachment.clone());
    }
    resolve_attachment_from_local_path(expected_kind, selector, workspace_path)
}

/// Resolves one direct local file path into a synthetic attachment descriptor so follow-up media tools can reuse generated outputs inside the same turn.
/// 将一条直接本地文件路径解析为合成附件描述信息，从而让后续媒体工具能够在同一轮内复用生成结果。
fn resolve_attachment_from_local_path(
    expected_kind: ConversationAttachmentKind,
    selector: &str,
    workspace_path: &str,
) -> Option<ConversationAttachment> {
    // Candidate direct-path text supplied by the model.
    // 模型提供的候选直接路径文本。
    let trimmed_selector = selector.trim();
    if trimmed_selector.is_empty() {
        return None;
    }

    // Absolute candidate path resolved from the selector and the current workspace root when the selector is relative.
    // 当选择器是相对路径时，基于当前工作区根目录解析得到的绝对候选路径。
    let candidate_path = resolve_local_selector_path(workspace_path, trimmed_selector);
    if !candidate_path.is_file() {
        return None;
    }

    // Host attachment descriptor inferred from the local file-path extension.
    // 根据本地文件路径扩展名推断出的宿主附件描述信息。
    let descriptor = attachment_descriptor_from_path(&candidate_path)?;
    if descriptor.kind != expected_kind {
        return None;
    }

    // Metadata snapshot reused for host-side byte-length hints.
    // 为宿主侧字节长度提示复用的元数据快照。
    let metadata = fs::metadata(&candidate_path).ok();
    Some(ConversationAttachment {
        kind: descriptor.kind,
        file_name: candidate_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| candidate_path.display().to_string()),
        local_path: candidate_path.to_string_lossy().into_owned(),
        source_path: None,
        asset_id: None,
        byte_len: metadata.map(|value| value.len()),
        preview_local_path: None,
        mime_type: String::from(descriptor.mime_type),
    })
}

/// Resolves one local selector path by treating relative paths as workspace-relative and then canonicalizing any existing filesystem target.
/// 解析一条本地选择器路径：把相对路径视作相对工作区路径，并对已存在的文件系统目标做规范化。
fn resolve_local_selector_path(workspace_path: &str, selector: &str) -> PathBuf {
    // Raw selector path supplied by the model for one follow-up media input.
    // 模型为某个后续媒体输入提供的原始选择器路径。
    let selector_path = PathBuf::from(selector.trim());
    if selector_path.is_absolute() {
        return canonicalize_workspace_path(&selector_path);
    }
    canonicalize_workspace_path(
        &resolve_workspace_anchor_directory(workspace_path).join(selector_path),
    )
}

/// Builds the ordered unique file-name list visible to the current conversation for one attachment kind.
/// 针对单个附件类型构建当前会话可见的有序唯一文件名列表。
fn available_attachment_names(
    attachments: &[ConversationAttachment],
    expected_kind: ConversationAttachmentKind,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut names = Vec::new();

    for attachment in attachments {
        if attachment.kind != expected_kind {
            continue;
        }
        let name = attachment.file_name.trim();
        if name.is_empty() {
            continue;
        }
        let lowered = name.to_ascii_lowercase();
        if seen.insert(lowered) {
            names.push(name.to_string());
        }
    }

    names
}

/// Builds one selector description that allows either a visible attachment name or one direct local file path returned by a previous tool call.
/// 构建一条选择器说明，使其既允许使用可见附件名，也允许使用前序工具调用返回的直接本地文件路径。
fn attachment_selector_description(
    base_description: &str,
    modality_name: &str,
    available_names: &[String],
) -> String {
    // Human-readable preview of currently visible attachment names for the requested modality.
    // 当前对目标模态可见附件名的人类可读预览文本。
    let available_preview = if available_names.is_empty() {
        String::from("none")
    } else {
        available_names.join(", ")
    };
    format!(
        "{base_description} You may pass a visible {modality_name} attachment file name, asset id, or any existing local file path, including one returned by a previous generation tool. Available names: {available_preview}."
    )
}

/// Parses one semantic `image_generation` invocation payload.
/// 解析单条语义 `image_generation` 调用载荷。
fn parse_image_generation_invocation(args: Value) -> Result<ImageGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `image_generation` must be a JSON object.")
    })?;
    let prompt = required_trimmed_string(
        object.get("prompt"),
        "The `image_generation` tool is missing the required `prompt` argument.",
    )?;
    Ok(ImageGenerationInvocation {
        prompt,
        target: optional_trimmed_string(object.get("target")),
        count: object
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 4) as usize,
        aspect_ratio: optional_trimmed_string(object.get("aspect_ratio")),
        seed: object.get("seed").and_then(Value::as_u64),
        subject_image: optional_trimmed_string(object.get("subject_image")),
        prompt_optimizer: object
            .get("prompt_optimizer")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `video_generation` invocation payload.
/// 解析单条语义 `video_generation` 调用载荷。
fn parse_video_generation_invocation(args: Value) -> Result<VideoGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `video_generation` must be a JSON object.")
    })?;
    Ok(VideoGenerationInvocation {
        prompt: required_trimmed_string(
            object.get("prompt"),
            "The `video_generation` tool is missing the required `prompt` argument.",
        )?,
        target: optional_trimmed_string(object.get("target")),
        first_image: optional_trimmed_string(object.get("first_image")),
        last_image: optional_trimmed_string(object.get("last_image")),
        subject_image: optional_trimmed_string(object.get("subject_image")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `speech_synthesis` invocation payload.
/// 解析单条语义 `speech_synthesis` 调用载荷。
fn parse_speech_synthesis_invocation(args: Value) -> Result<SpeechSynthesisInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `speech_synthesis` must be a JSON object.")
    })?;
    Ok(SpeechSynthesisInvocation {
        text: required_trimmed_string(
            object.get("text"),
            "The `speech_synthesis` tool is missing the required `text` argument.",
        )?,
        target: optional_trimmed_string(object.get("target")),
        voice: optional_trimmed_string(object.get("voice")),
        language: optional_trimmed_string(object.get("language")),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `music_generation` invocation payload.
/// 解析单条语义 `music_generation` 调用载荷。
fn parse_music_generation_invocation(args: Value) -> Result<MusicGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `music_generation` must be a JSON object.")
    })?;
    Ok(MusicGenerationInvocation {
        prompt: optional_trimmed_string(object.get("prompt")),
        lyrics: optional_trimmed_string(object.get("lyrics")),
        target: optional_trimmed_string(object.get("target")),
        instrumental: object
            .get("instrumental")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        lyrics_optimizer: object
            .get("lyrics_optimizer")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `music_cover` invocation payload.
/// 解析单条语义 `music_cover` 调用载荷。
fn parse_music_cover_invocation(args: Value) -> Result<MusicCoverInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("The arguments for `music_cover` must be a JSON object."))?;
    Ok(MusicCoverInvocation {
        prompt: required_trimmed_string(
            object.get("prompt"),
            "The `music_cover` tool is missing the required `prompt` argument.",
        )?,
        audio: required_trimmed_string(
            object.get("audio"),
            "The `music_cover` tool is missing the required `audio` argument.",
        )?,
        lyrics: optional_trimmed_string(object.get("lyrics")),
        target: optional_trimmed_string(object.get("target")),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Extracts one required trimmed string from one JSON field.
/// 从单个 JSON 字段中提取一段必填的裁剪后字符串。
fn required_trimmed_string(value: Option<&Value>, missing_message: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| missing_message.to_string())
}

/// Extracts one optional trimmed string from one JSON field.
/// 从单个 JSON 字段中提取一段可选的裁剪后字符串。
fn optional_trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extracts shared generated-asset output options from one semantic media-generation invocation object.
/// 从单条语义媒体生成调用对象中提取共享的生成资产输出选项。
fn parse_generated_asset_output_options(
    object: &serde_json::Map<String, Value>,
) -> GeneratedAssetOutputOptions {
    GeneratedAssetOutputOptions {
        output_dir: optional_trimmed_string(object.get("output_dir")),
        output_file_stem: optional_trimmed_string(object.get("output_file_stem")),
    }
}

/// Splits one fully qualified `vendor/target` selector into its vendor and target components.
/// 将单个完整 `供应商/目标` 选择器拆分为供应商与目标两部分。
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

/// Decodes one hex string into raw bytes with a provider-specific error label.
/// 使用带有提供方标签的错误消息把十六进制字符串解码为原始字节。
fn decode_hex_bytes(hex: &str, provider_label: &str) -> Result<Vec<u8>, String> {
    let trimmed = hex.trim();
    if trimmed.is_empty() {
        return Err(format!("{provider_label} returned empty audio data."));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "{provider_label} returned audio data that is not valid hexadecimal."
        ));
    }
    if trimmed.len() % 2 != 0 {
        return Err(format!(
            "{provider_label} returned hexadecimal audio data with an odd length."
        ));
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let raw_bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < raw_bytes.len() {
        let chunk = std::str::from_utf8(&raw_bytes[index..index + 2]).map_err(|error| {
            format!("Failed to decode hexadecimal data from {provider_label}: {error}")
        })?;
        let byte = u8::from_str_radix(chunk, 16).map_err(|error| {
            format!("Failed to decode hexadecimal data from {provider_label}: {error}")
        })?;
        bytes.push(byte);
        index += 2;
    }
    Ok(bytes)
}

/// Maps one requested audio format to the MIME type expected by the managed attachment store.
/// 将单个请求的音频格式映射到托管附件存储期望的 MIME 类型。
fn audio_format_to_mime_type(format: &str) -> Result<&'static str, String> {
    match format.trim().to_ascii_lowercase().as_str() {
        "mp3" => Ok("audio/mpeg"),
        "wav" => Ok("audio/wav"),
        "m4a" => Ok("audio/mp4"),
        "aac" => Ok("audio/aac"),
        "flac" => Ok("audio/flac"),
        other => Err(format!(
            "Audio format `{other}` is not currently supported."
        )),
    }
}

/// Truncates one error body so host-visible messages stay compact and readable.
/// 截断单条错误正文，使宿主可见消息保持紧凑且可读。
fn truncate_error_text(value: &str, limit: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let shortened = trimmed.chars().take(limit).collect::<String>();
    format!("{shortened}…")
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedAssetOutputOptions, generated_asset_to_model_output_json,
        parse_image_generation_invocation, resolve_attachment_from_local_path,
        resolve_generated_output_directory, stage_generated_asset_bytes,
        with_optional_output_sequence,
    };
    use crate::attachment_assets::encode_rgba_image_to_png_bytes;
    use crate::workspace_path::normalized_workspace_path_text_from_path;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one unique temporary workspace directory path so media-generation tests stay isolated.
    /// 构建一条唯一的临时工作区目录路径，使媒体生成测试彼此隔离。
    fn unique_workspace_directory() -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("vulcan-media-generation-test-{unique_id}"));
        std::fs::create_dir_all(&directory).expect("temporary workspace directory should create");
        directory
    }

    /// Removes one temporary workspace directory after a media-generation test has finished.
    /// 在媒体生成测试结束后移除一条临时工作区目录。
    fn cleanup_workspace_directory(directory: &std::path::Path) {
        let _ = std::fs::remove_dir_all(directory);
    }

    /// Verifies caller-selected output stems are numbered only when one generation request returns multiple files.
    /// 验证只有在一次生成请求返回多个文件时，才会对调用方选定的输出名称主体追加编号。
    #[test]
    fn optional_output_stems_are_numbered_for_multi_output_requests() {
        let options = GeneratedAssetOutputOptions {
            output_dir: Some(String::from("exports")),
            output_file_stem: Some(String::from("cover")),
        };

        let single_output = with_optional_output_sequence(&options, 1, 0);
        assert_eq!(single_output.output_file_stem.as_deref(), Some("cover"));

        let multi_output = with_optional_output_sequence(&options, 3, 1);
        assert_eq!(multi_output.output_file_stem.as_deref(), Some("cover-002"));
    }

    /// Verifies shared generated-output options are parsed from the semantic image-generation invocation payload.
    /// 验证可以从语义图片生成调用载荷中解析出共享的生成输出选项。
    #[test]
    fn image_generation_invocation_parses_generated_output_options() {
        let invocation = parse_image_generation_invocation(serde_json::json!({
            "prompt": "render one skyline",
            "output_dir": "exports/images",
            "output_file_stem": "skyline.png"
        }))
        .expect("image generation invocation should parse");

        assert_eq!(
            invocation.output.output_dir.as_deref(),
            Some("exports/images")
        );
        assert_eq!(
            invocation.output.output_file_stem.as_deref(),
            Some("skyline.png")
        );
    }

    /// Verifies generated assets can be exported to a caller-selected directory while still keeping one managed attachment copy for replay.
    /// 验证可以把生成资产导出到调用方选定目录，同时仍保留一份托管附件副本用于回放。
    #[test]
    fn generated_assets_can_export_copy_and_keep_managed_attachment() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let png_bytes = encode_rgba_image_to_png_bytes(1, 1, &[255_u8, 0, 0, 255])
            .expect("png bytes should encode");
        let output = stage_generated_asset_bytes(
            &workspace_path,
            "image/png",
            "provider-output.jpg",
            &png_bytes,
            &GeneratedAssetOutputOptions {
                output_dir: Some(String::from("exports")),
                output_file_stem: Some(String::from("poster.jpeg")),
            },
        )
        .expect("generated asset should stage");

        assert!(Path::new(&output.model_local_path).is_file());
        assert!(Path::new(&output.attachment.local_path).is_file());
        assert_ne!(
            output.model_local_path,
            output.attachment.local_path.replace('\\', "/")
        );
        assert_eq!(output.attachment.file_name, "poster.png");
        assert_eq!(
            output
                .attachment
                .source_path
                .as_deref()
                .map(|value| value.replace('\\', "/"))
                .as_deref(),
            Some(output.model_local_path.as_str())
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies model-visible generated-asset payloads now expose the preferred local path required for immediate follow-up tool calls.
    /// 验证模型可见的生成资产载荷现在会暴露首选本地路径，以支持立即发起后续工具调用。
    #[test]
    fn generated_asset_model_output_exposes_preferred_local_path() {
        let output = super::StagedGeneratedAsset {
            attachment: ConversationAttachment {
                kind: ConversationAttachmentKind::Image,
                file_name: String::from("diagram.png"),
                local_path: String::from(r"D:\managed\diagram.png"),
                source_path: None,
                asset_id: Some(String::from("asset-1")),
                byte_len: Some(42),
                preview_local_path: None,
                mime_type: String::from("image/png"),
            },
            model_local_path: String::from(r"D:\exports\diagram.png"),
        };

        let value = generated_asset_to_model_output_json(&output);
        assert_eq!(
            value.get("local_path").and_then(serde_json::Value::as_str),
            Some(r"D:\exports\diagram.png")
        );
    }

    /// Verifies workspace-relative output directories resolve under the active workspace root.
    /// 验证工作区相对输出目录会被解析到当前工作区根目录下。
    #[test]
    fn relative_generated_output_directories_resolve_from_workspace_root() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let resolved_directory =
            resolve_generated_output_directory(&workspace_path, "exports/videos");
        let expected_directory = workspace_directory.join("exports").join("videos");

        assert_eq!(
            normalized_workspace_path_text_from_path(&resolved_directory),
            normalized_workspace_path_text_from_path(&expected_directory)
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies follow-up media selectors can resolve one direct local file path even when that path was not part of the original attachment snapshot.
    /// 验证即使某条本地文件路径不在原始附件快照中，后续媒体选择器仍然可以直接解析它。
    #[test]
    fn direct_local_media_selectors_resolve_without_attachment_snapshot() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let export_directory = workspace_directory.join("exports");
        std::fs::create_dir_all(&export_directory).expect("export directory should create");
        let export_path = export_directory.join("shot.png");
        let png_bytes = encode_rgba_image_to_png_bytes(1, 1, &[0_u8, 255, 0, 255])
            .expect("png bytes should encode");
        std::fs::write(&export_path, png_bytes).expect("export file should write");

        let attachment = resolve_attachment_from_local_path(
            ConversationAttachmentKind::Image,
            "exports/shot.png",
            &workspace_path,
        )
        .expect("direct local selector should resolve");

        assert_eq!(attachment.file_name, "shot.png");
        assert!(Path::new(&attachment.local_path).is_file());
        assert_eq!(attachment.mime_type, "image/png");

        cleanup_workspace_directory(&workspace_directory);
    }
}

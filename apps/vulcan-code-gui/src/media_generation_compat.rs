//! Shared host-side compatibility adapters for configured image, video, speech, and music capability vendors.
//! 为已配置图片、视频、语音与音乐能力供应商提供共享宿主兼容适配器。

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::attachment_assets::{attachment_descriptor_from_mime_type, stage_attachment_bytes};
use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use tokio::time::{Duration, sleep};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

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

/// Builds the parameter schema for the semantic `image_generation` tool.
/// 构建语义 `image_generation` 工具的参数模式。
pub fn configured_image_generation_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
    conversation_attachments: &[ConversationAttachment],
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::ImageGeneration))
        .collect::<Vec<_>>();
    let image_enum =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Image);
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "图片生成提示词。" },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "count": { "type": "integer", "minimum": 1, "maximum": 4, "default": 1 },
            "aspect_ratio": { "type": "string", "description": "例如 1:1、16:9、9:16。" },
            "seed": { "type": "integer", "minimum": 0 },
            "subject_image": { "type": "string", "enum": image_enum, "description": "可选参考图片附件名称。" },
            "prompt_optimizer": { "type": "boolean", "default": false }
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
    let image_enum =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Image);
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "视频生成提示词。" },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "first_image": { "type": "string", "enum": image_enum, "description": "可选首帧图片附件。" },
            "last_image": { "type": "string", "enum": image_enum, "description": "可选末帧图片附件。" },
            "subject_image": { "type": "string", "enum": image_enum, "description": "可选主体参考图片附件。" }
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
            "text": { "type": "string", "description": "需要合成的文本。" },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "voice": { "type": "string", "description": "可选 voice_id。" },
            "language": { "type": "string", "description": "可选 language_boost 代码。" },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" }
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
            "prompt": { "type": "string", "description": "音乐风格提示词。" },
            "lyrics": { "type": "string", "description": "可选歌词；若未提供可改用 instrumental 或 lyrics_optimizer。" },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "instrumental": { "type": "boolean", "default": false },
            "lyrics_optimizer": { "type": "boolean", "default": false },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" }
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
    let audio_enum =
        available_attachment_names(conversation_attachments, ConversationAttachmentKind::Audio);
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "翻唱风格提示词。" },
            "audio": { "type": "string", "enum": audio_enum, "description": "参考音频附件名称。" },
            "lyrics": { "type": "string", "description": "可选歌词覆盖值。" },
            "target": { "type": "string", "enum": target_enum, "default": "auto" },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" }
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
            Err(error) => errors.push(format!("{}：{error}", target.target_selector)),
        }
    }

    Err(format!(
        "所有已配置 image_generation 目标均调用失败：{}。",
        errors.join("；")
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
            Err(error) => errors.push(format!("{}：{error}", target.target_selector)),
        }
    }

    Err(format!(
        "所有已配置 video_generation 目标均调用失败：{}。",
        errors.join("；")
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
            Err(error) => errors.push(format!("{}：{error}", target.target_selector)),
        }
    }

    Err(format!(
        "所有已配置 speech_synthesis 目标均调用失败：{}。",
        errors.join("；")
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
            Err(error) => errors.push(format!("{}：{error}", target.target_selector)),
        }
    }

    Err(format!(
        "所有已配置 music_generation 目标均调用失败：{}。",
        errors.join("；")
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
            Err(error) => errors.push(format!("{}：{error}", target.target_selector)),
        }
    }

    Err(format!(
        "所有已配置 music_cover 目标均调用失败：{}。",
        errors.join("；")
    ))
}

/// Builds one shared HTTP client used by all host-side capability adapters in this module.
/// 构建本模块全部宿主侧能力适配器共享使用的 HTTP 客户端。
fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|error| format!("创建 AI 能力 HTTP 客户端失败：{error}"))
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
            "{} 当前尚未配置任何可用目标。",
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
            "{} 指定目标当前不存在或不可用。可用目标：{}。",
            capability_kind.key(),
            configuration
                .usable_targets_for_kind(capability_kind)
                .join("、")
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
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax 图片生成")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/image_generation", false)?;

    let subject_reference = if let Some(image_selector) = invocation.subject_image.as_deref() {
        let attachment = resolve_attachment_by_name(
            conversation_attachments,
            ConversationAttachmentKind::Image,
            image_selector,
        )
        .ok_or_else(|| {
            format!("image_generation 指定的参考图片 `{image_selector}` 当前不存在。")
        })?;
        let payload =
            match read_provider_attachment(attachment).map_err(|error| error.to_string())? {
                Some(payload) => payload,
                None => return Ok(missing_attachment_result(attachment)),
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
        "MiniMax 图片生成",
    )
    .await?;
    let images = response_body
        .pointer("/data/image_base64")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if images.is_empty() {
        return Err(format!(
            "MiniMax 图片生成响应缺少 image_base64：{}",
            truncate_error_text(&response_body.to_string(), 240)
        ));
    }

    let mut attachments = Vec::new();
    for (index, image_value) in images.iter().enumerate() {
        let base64_image = image_value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("MiniMax 图片生成返回了空图片数据。"))?;
        let image_bytes = BASE64_STANDARD
            .decode(base64_image)
            .map_err(|error| format!("解码 MiniMax 图片 Base64 失败：{error}"))?;
        attachments.push(stage_generated_asset_bytes(
            workspace_path,
            "image/jpeg",
            &format!("minimax-image-{:03}.jpg", index + 1),
            &image_bytes,
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
        attachments,
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
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax 视频生成")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/video_generation", false)?;

    let first_frame_image = resolve_optional_image_data_url(
        conversation_attachments,
        invocation.first_image.as_deref(),
    )?;
    let last_frame_image = resolve_optional_image_data_url(
        conversation_attachments,
        invocation.last_image.as_deref(),
    )?;
    let subject_image = resolve_optional_image_data_url(
        conversation_attachments,
        invocation.subject_image.as_deref(),
    )?;

    let model_id = target.binding.model_id();
    if model_id.eq_ignore_ascii_case("MiniMax-Hailuo-2.3-Fast") && first_frame_image.is_none() {
        return Err(String::from(
            "MiniMax-Hailuo-2.3-Fast 仅支持图生视频，请提供 first_image。",
        ));
    }
    if last_frame_image.is_some() && first_frame_image.is_none() {
        return Err(String::from("提供 last_image 时必须同时提供 first_image。"));
    }
    if last_frame_image.is_some() && subject_image.is_some() {
        return Err(String::from("last_image 与 subject_image 不能同时使用。"));
    }
    if model_id.eq_ignore_ascii_case("MiniMax-Hailuo-02")
        && (first_frame_image.is_none() || last_frame_image.is_none())
    {
        return Err(String::from(
            "MiniMax-Hailuo-02 需要同时提供 first_image 与 last_image。",
        ));
    }
    if model_id.eq_ignore_ascii_case("S2V-01") && subject_image.is_none() {
        return Err(String::from("S2V-01 需要提供 subject_image。"));
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
        "MiniMax 视频生成",
    )
    .await?;
    let task_id = response_body
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax 视频生成响应缺少 task_id。"))?
        .to_string();

    let task_payload = poll_minimax_video_task(client, &target.vendor, &api_key, &task_id).await?;
    let file_id = task_payload
        .get("file_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax 视频任务完成后未返回 file_id。"))?;
    let download_url =
        fetch_minimax_file_download_url(client, &target.vendor, &api_key, file_id).await?;
    let video_bytes = download_binary(&download_url, "MiniMax 视频下载").await?;
    let attachment = stage_generated_asset_bytes(
        workspace_path,
        "video/mp4",
        &format!("minimax-video-{task_id}.mp4"),
        &video_bytes,
    )?;

    Ok(build_generated_result(
        AiToolKind::VideoGeneration,
        &target.target_selector,
        Some(json!({
            "task_id": task_id,
            "status": task_payload.get("status").cloned(),
            "file_id": file_id,
        })),
        vec![attachment],
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
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax 语音合成")?;
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
        "MiniMax 语音合成",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax 语音合成响应缺少 audio 字段。"))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax 语音合成")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let attachment = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-speech.{}", invocation.format),
        &audio_bytes,
    )?;

    Ok(build_generated_result(
        AiToolKind::SpeechSynthesis,
        &target.target_selector,
        Some(json!({
            "word_count": response_body.pointer("/extra_info/word_count").and_then(Value::as_u64),
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
        })),
        vec![attachment],
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
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax 音乐生成")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/music_generation", false)?;

    if invocation.instrumental && invocation.lyrics.is_some() {
        return Err(String::from("instrumental 与 lyrics 不能同时使用。"));
    }
    if invocation.lyrics_optimizer && (invocation.lyrics.is_some() || invocation.instrumental) {
        return Err(String::from(
            "lyrics_optimizer 不能与 lyrics 或 instrumental 同时使用。",
        ));
    }
    if invocation.prompt.is_none()
        && invocation.lyrics.is_none()
        && !invocation.instrumental
        && !invocation.lyrics_optimizer
    {
        return Err(String::from(
            "music_generation 至少需要 prompt、lyrics、instrumental 或 lyrics_optimizer 中的一项。",
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
        "MiniMax 音乐生成",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax 音乐生成响应缺少 audio 字段。"))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax 音乐生成")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let attachment = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-music.{}", invocation.format),
        &audio_bytes,
    )?;

    Ok(build_generated_result(
        AiToolKind::MusicGeneration,
        &target.target_selector,
        Some(json!({
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
            "audio_size": response_body.pointer("/extra_info/audio_size").and_then(Value::as_u64),
        })),
        vec![attachment],
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
    let api_key = require_vendor_api_key(&target.vendor, "MiniMax 音乐翻唱")?;
    let endpoint = resolve_minimax_endpoint(&target.vendor, "/v1/music_generation", false)?;
    let attachment = resolve_attachment_by_name(
        conversation_attachments,
        ConversationAttachmentKind::Audio,
        &invocation.audio,
    )
    .ok_or_else(|| {
        format!(
            "music_cover 指定的参考音频 `{}` 当前不存在。",
            invocation.audio
        )
    })?;
    let payload = match read_provider_attachment(attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(missing_attachment_result(attachment)),
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
        "MiniMax 音乐翻唱",
    )
    .await?;
    let audio_hex = response_body
        .pointer("/data/audio")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("MiniMax 音乐翻唱响应缺少 audio 字段。"))?;
    let audio_bytes = decode_hex_bytes(audio_hex, "MiniMax 音乐翻唱")?;
    let mime_type = audio_format_to_mime_type(&invocation.format)?;
    let attachment = stage_generated_asset_bytes(
        workspace_path,
        mime_type,
        &format!("minimax-cover.{}", invocation.format),
        &audio_bytes,
    )?;

    Ok(build_generated_result(
        AiToolKind::MusicCover,
        &target.target_selector,
        Some(json!({
            "reference_audio": invocation.audio,
            "audio_length": response_body.pointer("/extra_info/audio_length").and_then(Value::as_u64),
        })),
        vec![attachment],
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
        .map_err(|error| format!("{provider_label} 请求失败：{error}"))?;
    parse_json_response(response, provider_label).await
}

/// Parses one HTTP JSON response and translates MiniMax-style `base_resp` errors into host-readable messages.
/// 解析单条 HTTP JSON 响应，并把 MiniMax 风格 `base_resp` 错误转换为宿主可读消息。
async fn parse_json_response(
    response: reqwest::Response,
    provider_label: &str,
) -> Result<Value, String> {
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|error| format!("{provider_label} 读取响应正文失败：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "{provider_label} 返回 HTTP {}：{}",
            status,
            truncate_error_text(&body_text, 400)
        ));
    }
    let payload: Value = serde_json::from_str(&body_text)
        .map_err(|error| format!("{provider_label} 响应 JSON 解析失败：{error}"))?;
    if let Some(status_code) = payload
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
    {
        if status_code != 0 {
            let status_msg = payload
                .pointer("/base_resp/status_msg")
                .and_then(Value::as_str)
                .unwrap_or("未知错误");
            return Err(format!(
                "{provider_label} 返回业务错误 {status_code}：{}",
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
            .map_err(|error| format!("MiniMax 视频任务查询失败：{error}"))?;
        let payload = parse_json_response(response, "MiniMax 视频任务查询").await?;
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match status {
            "Success" => return Ok(payload),
            "Failed" => {
                return Err(format!(
                    "MiniMax 视频任务执行失败：{}",
                    truncate_error_text(&payload.to_string(), 240)
                ));
            }
            _ => {}
        }
        if elapsed_seconds >= 300 {
            return Err(String::from("MiniMax 视频任务等待超时（300 秒）。"));
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
        .map_err(|error| format!("MiniMax 文件下载地址查询失败：{error}"))?;
    let payload = parse_json_response(response, "MiniMax 文件下载地址查询").await?;
    payload
        .pointer("/file/download_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| String::from("MiniMax 文件下载地址响应缺少 download_url。"))
}

/// Downloads one binary payload from the provided URL.
/// 从给定 URL 下载一份二进制载荷。
async fn download_binary(url: &str, label: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("{label} 请求失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("无法读取响应正文"));
        return Err(format!(
            "{label} 返回 HTTP {}：{}",
            status,
            truncate_error_text(&body_text, 240)
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("{label} 读取响应字节失败：{error}"))
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

    let mut url =
        reqwest::Url::parse(base_url).map_err(|error| format!("MiniMax Base URL 无效：{error}"))?;
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if !allow_regular_text_api && minimax_path_looks_like_regular_text_api(&normalized_path) {
        return Err(String::from(
            "当前 MiniMax 能力层使用独立能力端点，不支持普通文本模型 API 地址。请填写 MiniMax 根地址（如 `https://api.minimax.io`）或留空使用默认值。",
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
    let mut url =
        reqwest::Url::parse(&endpoint).map_err(|error| format!("MiniMax 端点无效：{error}"))?;
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
            "当前只实现了 MiniMax 的 `{}` 能力目标。",
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
            "MiniMax `{}` 目标 `{}` 当前未在能力资源目录中注册。",
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
        .ok_or_else(|| format!("{provider_label} 当前缺少有效 API Key。"))
}

/// Stages one generated binary asset into the managed attachment store.
/// 将单个生成得到的二进制资产暂存到托管附件存储中。
fn stage_generated_asset_bytes(
    workspace_path: &str,
    mime_type: &str,
    suggested_file_name: &str,
    bytes: &[u8],
) -> Result<ConversationAttachment, String> {
    let descriptor = attachment_descriptor_from_mime_type(mime_type)
        .ok_or_else(|| format!("当前暂不支持处理 MIME `{mime_type}` 的生成结果。"))?;
    stage_attachment_bytes(
        workspace_path,
        &descriptor,
        suggested_file_name,
        None,
        bytes,
    )
}

/// Builds one structured tool result that includes both model-visible output and richer host metadata for generated assets.
/// 构建一条同时包含模型可见输出与更丰富宿主元数据的生成资产工具结果。
fn build_generated_result(
    capability_kind: AiToolKind,
    target_selector: &str,
    extra: Option<Value>,
    attachments: Vec<ConversationAttachment>,
) -> VulcanToolInvocationResult {
    let host_outputs = attachments
        .iter()
        .map(attachment_to_json)
        .collect::<Vec<_>>();
    let model_outputs = attachments
        .iter()
        .map(attachment_to_model_output_json)
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
        "mime_type": attachment.mime_type,
        "asset_id": attachment.asset_id,
        "byte_len": attachment.byte_len,
    })
}

/// Serializes one staged attachment into a model-visible JSON object while hiding raw local filesystem paths from the assistant.
/// 将单条已暂存附件序列化为模型可见 JSON 对象，同时向助手隐藏原始本地文件系统路径。
fn attachment_to_model_output_json(attachment: &ConversationAttachment) -> Value {
    json!({
        "kind": attachment.kind.modality_name(),
        "file_name": attachment.file_name,
        "mime_type": attachment.mime_type,
        "asset_id": attachment.asset_id,
        "byte_len": attachment.byte_len,
    })
}

/// Resolves one optional image selector into a Data URL accepted by MiniMax multimodal generation endpoints.
/// 将单个可选图片选择器解析为 MiniMax 多模态生成端点接受的 Data URL。
fn resolve_optional_image_data_url(
    attachments: &[ConversationAttachment],
    image_selector: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(image_selector) = image_selector
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let attachment = resolve_attachment_by_name(
        attachments,
        ConversationAttachmentKind::Image,
        image_selector,
    )
    .ok_or_else(|| format!("指定图片 `{image_selector}` 当前不存在。"))?;
    let payload = match read_provider_attachment(attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    Ok(Some(payload.data_url))
}

/// Resolves one staged attachment by user-visible file name, asset id, or managed local-path file name.
/// 根据用户可见文件名、资产标识或托管本地文件名解析单条已暂存附件。
fn resolve_attachment_by_name<'a>(
    attachments: &'a [ConversationAttachment],
    expected_kind: ConversationAttachmentKind,
    selector: &str,
) -> Option<&'a ConversationAttachment> {
    let normalized_selector = selector.trim().to_ascii_lowercase();
    if normalized_selector.is_empty() {
        return None;
    }

    attachments.iter().find(|attachment| {
        attachment.kind == expected_kind
            && (attachment
                .file_name
                .eq_ignore_ascii_case(&normalized_selector)
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

/// Parses one semantic `image_generation` invocation payload.
/// 解析单条语义 `image_generation` 调用载荷。
fn parse_image_generation_invocation(args: Value) -> Result<ImageGenerationInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("image_generation 参数必须是 JSON 对象。"))?;
    let prompt = required_trimmed_string(
        object.get("prompt"),
        "image_generation 缺少必填参数 `prompt`。",
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
    })
}

/// Parses one semantic `video_generation` invocation payload.
/// 解析单条语义 `video_generation` 调用载荷。
fn parse_video_generation_invocation(args: Value) -> Result<VideoGenerationInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("video_generation 参数必须是 JSON 对象。"))?;
    Ok(VideoGenerationInvocation {
        prompt: required_trimmed_string(
            object.get("prompt"),
            "video_generation 缺少必填参数 `prompt`。",
        )?,
        target: optional_trimmed_string(object.get("target")),
        first_image: optional_trimmed_string(object.get("first_image")),
        last_image: optional_trimmed_string(object.get("last_image")),
        subject_image: optional_trimmed_string(object.get("subject_image")),
    })
}

/// Parses one semantic `speech_synthesis` invocation payload.
/// 解析单条语义 `speech_synthesis` 调用载荷。
fn parse_speech_synthesis_invocation(args: Value) -> Result<SpeechSynthesisInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("speech_synthesis 参数必须是 JSON 对象。"))?;
    Ok(SpeechSynthesisInvocation {
        text: required_trimmed_string(
            object.get("text"),
            "speech_synthesis 缺少必填参数 `text`。",
        )?,
        target: optional_trimmed_string(object.get("target")),
        voice: optional_trimmed_string(object.get("voice")),
        language: optional_trimmed_string(object.get("language")),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
    })
}

/// Parses one semantic `music_generation` invocation payload.
/// 解析单条语义 `music_generation` 调用载荷。
fn parse_music_generation_invocation(args: Value) -> Result<MusicGenerationInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("music_generation 参数必须是 JSON 对象。"))?;
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
    })
}

/// Parses one semantic `music_cover` invocation payload.
/// 解析单条语义 `music_cover` 调用载荷。
fn parse_music_cover_invocation(args: Value) -> Result<MusicCoverInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("music_cover 参数必须是 JSON 对象。"))?;
    Ok(MusicCoverInvocation {
        prompt: required_trimmed_string(
            object.get("prompt"),
            "music_cover 缺少必填参数 `prompt`。",
        )?,
        audio: required_trimmed_string(object.get("audio"), "music_cover 缺少必填参数 `audio`。")?,
        lyrics: optional_trimmed_string(object.get("lyrics")),
        target: optional_trimmed_string(object.get("target")),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
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
        return Err(format!("{provider_label} 返回了空音频数据。"));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!("{provider_label} 返回的音频数据不是合法十六进制。"));
    }
    if trimmed.len() % 2 != 0 {
        return Err(format!("{provider_label} 返回的音频十六进制长度为奇数。"));
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let raw_bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < raw_bytes.len() {
        let chunk = std::str::from_utf8(&raw_bytes[index..index + 2])
            .map_err(|error| format!("{provider_label} 解码十六进制失败：{error}"))?;
        let byte = u8::from_str_radix(chunk, 16)
            .map_err(|error| format!("{provider_label} 解码十六进制失败：{error}"))?;
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
        other => Err(format!("当前暂不支持音频格式 `{other}`。")),
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

//! MiniMax media-generation request implementations.
//! MiniMax 媒体生成请求实现。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::{Value, json};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_tools_manager::VulcanToolInvocationResult;

use crate::agent_runtime::provider_attachment::read_provider_attachment;
use crate::provider_config::AiToolKind;

use super::{
    assets::{
        build_generated_result, missing_attachment_result,
        resolve_attachment_by_name_or_local_path, resolve_optional_image_data_url,
        stage_generated_asset_bytes, with_optional_output_sequence,
    },
    codecs::{audio_format_to_mime_type, decode_hex_bytes, truncate_error_text},
    endpoints::resolve_minimax_endpoint,
    http::send_json_request,
    targets::{ensure_minimax_vendor, require_vendor_api_key},
    tasks::{download_binary, fetch_minimax_file_download_url, poll_minimax_video_task},
    types::{
        ImageGenerationInvocation, MusicCoverInvocation, MusicGenerationInvocation,
        ResolvedVendorTarget, SpeechSynthesisInvocation, VideoGenerationInvocation,
    },
};

/// Executes one image-generation request against one concrete target.
/// 针对单个具体目标执行一次图片生成请求。
pub(super) async fn execute_image_generation_against_target(
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
pub(super) async fn execute_video_generation_against_target(
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
pub(super) async fn execute_speech_synthesis_against_target(
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
pub(super) async fn execute_music_generation_against_target(
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
pub(super) async fn execute_music_cover_against_target(
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

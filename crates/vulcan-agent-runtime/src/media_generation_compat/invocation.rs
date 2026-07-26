//! Argument parsing for configured media-generation tool calls.
//! 配置化媒体生成工具调用的参数解析。

use serde_json::Value;

use super::types::{
    GeneratedAssetOutputOptions, ImageGenerationInvocation, MusicCoverInvocation,
    MusicGenerationInvocation, SpeechSynthesisInvocation, VideoGenerationInvocation,
};

/// Parses one semantic `image_generation` invocation payload.
/// 解析单条语义 `image_generation` 调用载荷。
pub(super) fn parse_image_generation_invocation(
    args: Value,
) -> Result<ImageGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `image_generation` must be a JSON object.")
    })?;
    let prompt = required_trimmed_string(
        object.get("prompt"),
        "The `image_generation` tool is missing the required `prompt` argument.",
    )?;
    Ok(ImageGenerationInvocation {
        prompt,
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
pub(super) fn parse_video_generation_invocation(
    args: Value,
) -> Result<VideoGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `video_generation` must be a JSON object.")
    })?;
    Ok(VideoGenerationInvocation {
        prompt: required_trimmed_string(
            object.get("prompt"),
            "The `video_generation` tool is missing the required `prompt` argument.",
        )?,
        first_image: optional_trimmed_string(object.get("first_image")),
        last_image: optional_trimmed_string(object.get("last_image")),
        subject_image: optional_trimmed_string(object.get("subject_image")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `speech_synthesis` invocation payload.
/// 解析单条语义 `speech_synthesis` 调用载荷。
pub(super) fn parse_speech_synthesis_invocation(
    args: Value,
) -> Result<SpeechSynthesisInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `speech_synthesis` must be a JSON object.")
    })?;
    Ok(SpeechSynthesisInvocation {
        text: required_trimmed_string(
            object.get("text"),
            "The `speech_synthesis` tool is missing the required `text` argument.",
        )?,
        voice: optional_trimmed_string(object.get("voice")),
        language: optional_trimmed_string(object.get("language")),
        format: optional_trimmed_string(object.get("format"))
            .unwrap_or_else(|| String::from("mp3")),
        output: parse_generated_asset_output_options(object),
    })
}

/// Parses one semantic `music_generation` invocation payload.
/// 解析单条语义 `music_generation` 调用载荷。
pub(super) fn parse_music_generation_invocation(
    args: Value,
) -> Result<MusicGenerationInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `music_generation` must be a JSON object.")
    })?;
    Ok(MusicGenerationInvocation {
        prompt: optional_trimmed_string(object.get("prompt")),
        lyrics: optional_trimmed_string(object.get("lyrics")),
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
pub(super) fn parse_music_cover_invocation(args: Value) -> Result<MusicCoverInvocation, String> {
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

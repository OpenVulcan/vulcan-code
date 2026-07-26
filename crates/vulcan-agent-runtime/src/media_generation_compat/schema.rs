//! JSON Schema generation for configured media-generation tools.
//! 配置化媒体生成工具的 JSON Schema 生成。

use serde_json::{Value, json};
use vulcan_core::types::ConversationAttachment;

use crate::provider_config::PersistedAiToolConfiguration;

use super::{
    assets::attachment_selector_description,
    types::{GENERATED_OUTPUT_DIR_DESCRIPTION, GENERATED_OUTPUT_FILE_STEM_DESCRIPTION},
};

/// Builds the parameter schema for the semantic `image_generation` tool.
/// 构建语义 `image_generation` 工具的参数模式。
pub fn configured_image_generation_parameters_schema(
    _configuration: &PersistedAiToolConfiguration,
    _conversation_attachments: &[ConversationAttachment],
) -> Value {
    let subject_image_description =
        attachment_selector_description("Optional reference image attachment selector.", "image");
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Prompt used for image generation." },
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
    _configuration: &PersistedAiToolConfiguration,
    _conversation_attachments: &[ConversationAttachment],
) -> Value {
    let first_image_description =
        attachment_selector_description("Optional first-frame image selector.", "image");
    let last_image_description =
        attachment_selector_description("Optional last-frame image selector.", "image");
    let subject_image_description =
        attachment_selector_description("Optional subject reference image selector.", "image");
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Prompt used for video generation." },
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
    _configuration: &PersistedAiToolConfiguration,
) -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Input text to synthesize into speech." },
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
    _configuration: &PersistedAiToolConfiguration,
) -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Optional music style prompt." },
            "lyrics": { "type": "string", "description": "Optional lyrics text. If omitted, use `instrumental` or `lyrics_optimizer` instead when appropriate." },
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
    _configuration: &PersistedAiToolConfiguration,
    _conversation_attachments: &[ConversationAttachment],
) -> Value {
    let audio_description = attachment_selector_description("Reference audio selector.", "audio");
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "Style prompt for the music cover." },
            "audio": { "type": "string", "description": audio_description },
            "lyrics": { "type": "string", "description": "Optional lyrics override." },
            "format": { "type": "string", "enum": ["mp3", "wav", "flac", "aac", "m4a"], "default": "mp3" },
            "output_dir": { "type": "string", "description": GENERATED_OUTPUT_DIR_DESCRIPTION },
            "output_file_stem": { "type": "string", "description": GENERATED_OUTPUT_FILE_STEM_DESCRIPTION }
        },
        "required": ["prompt", "audio"]
    })
}

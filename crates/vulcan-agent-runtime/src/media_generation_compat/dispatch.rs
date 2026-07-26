//! Public execution entrypoints for configured media-generation tools.
//! 配置化媒体生成工具的公开执行入口。

use serde_json::Value;
use vulcan_core::types::ConversationAttachment;
use vulcan_tools_manager::VulcanToolInvocationResult;

use crate::provider_config::{AiToolKind, PersistedAiToolConfiguration};

use super::{
    http::build_http_client,
    invocation::{
        parse_image_generation_invocation, parse_music_cover_invocation,
        parse_music_generation_invocation, parse_speech_synthesis_invocation,
        parse_video_generation_invocation,
    },
    minimax_requests::{
        execute_image_generation_against_target, execute_music_cover_against_target,
        execute_music_generation_against_target, execute_speech_synthesis_against_target,
        execute_video_generation_against_target,
    },
    targets::resolve_vendor_target,
};

/// Executes one host-configured image-generation request and stages the generated images into the managed attachment store.
/// 执行一次宿主配置的图片生成请求，并将生成结果暂存到托管附件存储中。
pub async fn execute_configured_image_generation(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_image_generation_invocation(args)?;
    let target = resolve_vendor_target(&configuration, AiToolKind::ImageGeneration)?;
    let client = build_http_client()?;
    execute_image_generation_against_target(
        &client,
        &workspace_path,
        &target,
        &invocation,
        &conversation_attachments,
    )
    .await
    .map_err(|error| {
        format!(
            "The bound `image_generation` target `{}` failed: {error}",
            target.target_selector
        )
    })
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
    let target = resolve_vendor_target(&configuration, AiToolKind::VideoGeneration)?;
    let client = build_http_client()?;
    execute_video_generation_against_target(
        &client,
        &workspace_path,
        &target,
        &invocation,
        &conversation_attachments,
    )
    .await
    .map_err(|error| {
        format!(
            "The bound `video_generation` target `{}` failed: {error}",
            target.target_selector
        )
    })
}

/// Executes one host-configured speech-synthesis request and stages the generated audio into the managed attachment store.
/// 执行一次宿主配置的语音合成请求，并将生成音频暂存到托管附件存储中。
pub async fn execute_configured_speech_synthesis(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_speech_synthesis_invocation(args)?;
    let target = resolve_vendor_target(&configuration, AiToolKind::SpeechSynthesis)?;
    let client = build_http_client()?;
    execute_speech_synthesis_against_target(&client, &workspace_path, &target, &invocation)
        .await
        .map_err(|error| {
            format!(
                "The bound `speech_synthesis` target `{}` failed: {error}",
                target.target_selector
            )
        })
}

/// Executes one host-configured music-generation request and stages the generated audio into the managed attachment store.
/// 执行一次宿主配置的音乐生成请求，并将生成音频暂存到托管附件存储中。
pub async fn execute_configured_music_generation(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    workspace_path: String,
) -> Result<VulcanToolInvocationResult, String> {
    let invocation = parse_music_generation_invocation(args)?;
    let target = resolve_vendor_target(&configuration, AiToolKind::MusicGeneration)?;
    let client = build_http_client()?;
    execute_music_generation_against_target(&client, &workspace_path, &target, &invocation)
        .await
        .map_err(|error| {
            format!(
                "The bound `music_generation` target `{}` failed: {error}",
                target.target_selector
            )
        })
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
    let target = resolve_vendor_target(&configuration, AiToolKind::MusicCover)?;
    let client = build_http_client()?;
    execute_music_cover_against_target(
        &client,
        &workspace_path,
        &target,
        &invocation,
        &conversation_attachments,
    )
    .await
    .map_err(|error| {
        format!(
            "The bound `music_cover` target `{}` failed: {error}",
            target.target_selector
        )
    })
}

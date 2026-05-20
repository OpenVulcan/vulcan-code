//! Dedicated generated-audio tool transcript projection helpers.
//! 专属音频生成工具转录投影辅助逻辑。

use super::tool_shared::{
    build_base_tool_attributes, build_tool_directive_block, generated_attachment_descriptor,
};
use super::AgentViewBuildOptions;
use crate::model::SessionMessageEntry;
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one generated-audio transcript block carrying the playback metadata needed by the transcript card and host actions.
/// 构建一条携带转录卡片与宿主动作所需播放元数据的音频生成转录区块。
pub(crate) fn build_generated_audio_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let mut attributes = build_base_tool_attributes(
        message,
        AgentViewToolPresentationKind::GeneratedAudio,
        options,
    );
    if let Some(metadata) = message.metadata.as_ref()
        && let Some(descriptor) = generated_attachment_descriptor(metadata, "audio")
    {
        attributes.insert(
            String::from("generated-media-file-name"),
            descriptor.file_name,
        );
        attributes.insert(
            String::from("generated-media-local-path"),
            descriptor.local_path,
        );
        attributes.insert(
            String::from("generated-media-count"),
            descriptor.output_count.to_string(),
        );
        if let Some(preview_local_path) = descriptor.preview_local_path {
            attributes.insert(
                String::from("generated-media-preview-local-path"),
                preview_local_path,
            );
        }
        if let Some(mime_type) = descriptor.mime_type {
            attributes.insert(String::from("generated-media-mime-type"), mime_type);
        }
    }

    build_tool_directive_block(kind, attributes)
}

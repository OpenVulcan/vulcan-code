//! Dedicated generated-document tool transcript projection helpers.
//! 专属文档生成工具转录投影辅助逻辑。

use super::AgentViewBuildOptions;
use super::tool_shared::{
    build_base_tool_attributes, build_tool_directive_block, generated_attachment_descriptor,
};
use crate::model::SessionMessageEntry;
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one generated-document transcript block carrying the metadata needed by document cards and host open/reveal actions.
/// 构建一条携带文档卡片及宿主打开或定位动作所需元数据的文档生成转录区块。
pub(crate) fn build_generated_document_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let mut attributes = build_base_tool_attributes(
        message,
        AgentViewToolPresentationKind::GeneratedDocument,
        options,
    );
    if let Some(metadata) = message.metadata.as_ref()
        && let Some(descriptor) = generated_attachment_descriptor(metadata, "document")
            .or_else(|| generated_attachment_descriptor(metadata, "pdf"))
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

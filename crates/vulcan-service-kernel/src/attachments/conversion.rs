//! Composer attachment conversion helpers.
//! 输入区附件转换辅助逻辑。

use super::*;

/// Converts a core conversation attachment into the WebUI service DTO.
/// 将核心会话附件转换为 WebUI service DTO。
///
/// # Parameters
/// 参数
/// `attachment` is the core conversation attachment to project.
/// `attachment` 是要投影的核心会话附件。
///
/// # Returns
/// 返回值
/// The WebUI service attachment summary.
/// WebUI 服务附件摘要。
pub(crate) fn composer_attachment_summary_from_core(
    attachment: &ConversationAttachment,
) -> ComposerAttachmentSummary {
    ComposerAttachmentSummary {
        kind: attachment.kind.modality_name().to_string(),
        file_name: attachment.file_name.clone(),
        local_path: attachment.local_path.clone(),
        source_path: attachment.source_path.clone(),
        asset_id: attachment.asset_id.clone(),
        byte_len: attachment.byte_len,
        preview_local_path: attachment.preview_local_path.clone(),
        mime_type: attachment.mime_type.clone(),
    }
}

/// Converts a WebUI service attachment DTO back into the core conversation attachment.
/// 将 WebUI service 附件 DTO 转回核心会话附件。
///
/// # Parameters
/// 参数
/// `summary` is the WebUI service attachment summary to convert.
/// `summary` 是要转换的 WebUI 服务附件摘要。
///
/// # Returns
/// 返回值
/// The core conversation attachment reconstructed from the summary.
/// 从摘要重建的核心会话附件。
pub(crate) fn composer_attachment_summary_to_core(
    summary: ComposerAttachmentSummary,
) -> Result<ConversationAttachment, String> {
    Ok(ConversationAttachment {
        kind: parse_composer_attachment_kind(&summary.kind)?,
        file_name: summary.file_name,
        local_path: summary.local_path,
        source_path: summary.source_path,
        asset_id: summary.asset_id,
        byte_len: summary.byte_len,
        preview_local_path: summary.preview_local_path,
        mime_type: summary.mime_type,
    })
}

/// Parses one stable composer attachment kind key.
/// 解析单个稳定输入区附件类型键。
///
/// # Parameters
/// 参数
/// `value` is the raw attachment kind key from a WebUI summary.
/// `value` 是来自 WebUI 摘要的原始附件类型键。
///
/// # Returns
/// 返回值
/// The parsed core attachment kind.
/// 解析后的核心附件类型。
fn parse_composer_attachment_kind(value: &str) -> Result<ConversationAttachmentKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "image" => Ok(ConversationAttachmentKind::Image),
        "audio" => Ok(ConversationAttachmentKind::Audio),
        "video" => Ok(ConversationAttachmentKind::Video),
        "pdf" | "document" => Ok(ConversationAttachmentKind::Pdf),
        other => Err(format!("不支持的附件类型：{other}")),
    }
}

/// Resolves an attachment descriptor from a browser file name when the MIME type is missing.
/// 当浏览器未提供 MIME 类型时，从文件名解析附件描述。
///
/// # Parameters
/// 参数
/// `file_name` is the browser-provided file name used for extension inference.
/// `file_name` 是用于扩展名推断的浏览器文件名。
///
/// # Returns
/// 返回值
/// The inferred attachment descriptor when the file extension is supported.
/// 文件扩展名受支持时返回推断出的附件描述。
pub(crate) fn attachment_descriptor_from_path_fallback(
    file_name: &str,
) -> Option<vulcan_agent_runtime::attachment_assets::AttachmentDescriptor> {
    attachment_descriptor_from_path(Path::new(file_name))
}

/// Returns the localized label used by the native file picker filter.
/// 返回原生文件选择器过滤器使用的本地化名称。
///
/// # Parameters
/// 参数
/// `kind` is the core attachment kind whose label should be rendered.
/// `kind` 是需要渲染标签的核心附件类型。
///
/// # Returns
/// 返回值
/// A localized attachment kind label.
/// 本地化的附件类型标签。
pub(crate) fn composer_attachment_kind_label(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "图片",
        ConversationAttachmentKind::Audio => "音频",
        ConversationAttachmentKind::Video => "视频",
        ConversationAttachmentKind::Pdf => "PDF",
    }
}

/// Returns the browser file picker accept list for one attachment kind.
/// 返回单种附件类型对应的浏览器文件选择 accept 列表。
///
/// # Parameters
/// 参数
/// `kind` is the core attachment kind whose browser accept list should be rendered.
/// `kind` 是需要渲染浏览器 accept 列表的核心附件类型。
///
/// # Returns
/// 返回值
/// A comma-separated browser file accept list.
/// 逗号分隔的浏览器文件 accept 列表。
pub(crate) fn composer_attachment_kind_accept(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image/png,image/jpeg,image/webp,image/gif,image/bmp",
        ConversationAttachmentKind::Audio => {
            "audio/mpeg,audio/wav,audio/mp4,audio/aac,audio/flac,audio/ogg"
        }
        ConversationAttachmentKind::Video => {
            "video/mp4,video/quicktime,video/x-matroska,video/webm,video/x-msvideo"
        }
        ConversationAttachmentKind::Pdf => "application/pdf",
    }
}

/// Extracts composer attachments from durable transcript metadata.
/// 从持久转录元数据中提取输入区附件。
///
/// # Parameters
/// 参数
/// `metadata` is the durable transcript metadata JSON object when available.
/// `metadata` 是可用时的持久转录元数据 JSON 对象。
///
/// # Returns
/// 返回值
/// Composer attachment summaries extracted from supported metadata fields.
/// 从受支持元数据字段中提取的输入区附件摘要。
pub(crate) fn composer_message_attachments_from_metadata(
    metadata: Option<&serde_json::Value>,
) -> Vec<ComposerAttachmentSummary> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let Some(attachment_value) = metadata
        .get("attachments")
        .or_else(|| metadata.get("transcript_attachments"))
    else {
        return Vec::new();
    };
    serde_json::from_value::<Vec<ConversationAttachment>>(attachment_value.clone())
        .unwrap_or_default()
        .iter()
        .map(composer_attachment_summary_from_core)
        .collect()
}

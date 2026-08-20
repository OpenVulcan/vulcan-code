//! Composer attachment conversion helpers.
//! 输入区附件转换辅助逻辑。

use super::*;

/// Static metadata for one composer attachment kind.
/// 单个输入区附件类型的静态元数据。
struct ComposerAttachmentKindMetadata {
    /// Primary stable key exchanged with the WebUI.
    /// 与 WebUI 交换的主要稳定键。
    key: &'static str,
    /// Legacy or alternate stable keys accepted from WebUI summaries.
    /// 从 WebUI 摘要接受的旧版或备用稳定键。
    aliases: &'static [&'static str],
    /// Localized label used by picker controls.
    /// 文件选择控件使用的本地化标签。
    label: &'static str,
    /// Browser file picker accept list.
    /// 浏览器文件选择器 accept 列表。
    accept: &'static str,
}

impl ComposerAttachmentKindMetadata {
    /// Returns whether one normalized key belongs to this attachment kind.
    /// 返回单个规范化键是否属于该附件类型。
    fn matches_key(&self, key: &str) -> bool {
        self.key == key || self.aliases.contains(&key)
    }
}

/// Ordered composer attachment kinds shown by the WebUI capability controls.
/// WebUI 能力控件展示的输入区附件类型顺序。
pub(crate) fn composer_attachment_kinds() -> [ConversationAttachmentKind; 4] {
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
    ]
}

/// Returns static metadata for one composer attachment kind.
/// 返回单个输入区附件类型的静态元数据。
fn composer_attachment_kind_metadata(
    kind: &ConversationAttachmentKind,
) -> ComposerAttachmentKindMetadata {
    match kind {
        ConversationAttachmentKind::Image => ComposerAttachmentKindMetadata {
            key: "image",
            aliases: &[],
            label: "图片",
            accept: "image/png,image/jpeg,image/webp,image/gif,image/bmp",
        },
        ConversationAttachmentKind::Video => ComposerAttachmentKindMetadata {
            key: "video",
            aliases: &[],
            label: "视频",
            accept: "video/mp4,video/mpeg,video/quicktime,video/mov,video/x-m4v,video/x-matroska,video/webm,video/x-msvideo,video/avi,video/x-flv,video/x-ms-wmv,video/wmv,video/3gpp,video/3gpp2",
        },
        ConversationAttachmentKind::Audio => ComposerAttachmentKindMetadata {
            key: "audio",
            aliases: &[],
            label: "音频",
            accept: "audio/mpeg,audio/mp3,audio/wav,audio/x-wav,audio/mp4,audio/m4a,audio/x-m4a,audio/aac,audio/flac,audio/ogg,audio/oga,audio/opus,audio/pcm,audio/l16,audio/basic",
        },
        ConversationAttachmentKind::Pdf => ComposerAttachmentKindMetadata {
            key: "pdf",
            aliases: &["document"],
            label: "PDF",
            accept: "application/pdf",
        },
        ConversationAttachmentKind::File => ComposerAttachmentKindMetadata {
            key: "file",
            aliases: &[],
            label: "文件",
            accept: "application/json,text/plain",
        },
    }
}

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

/// Validates one WebUI attachment summary without cloning its payload fields.
/// 在不克隆载荷字段的情况下校验单个 WebUI 附件摘要。
///
/// # Parameters
/// 参数
/// `summary` is the WebUI service attachment summary to validate.
/// `summary` 是需要校验的 WebUI 服务附件摘要。
///
/// # Returns
/// 返回值
/// `Ok(())` when the summary can be converted by the submit path.
/// 当摘要可被提交流程转换时返回 `Ok(())`。
pub(crate) fn validate_composer_attachment_summary(
    summary: &ComposerAttachmentSummary,
) -> Result<(), String> {
    parse_composer_attachment_kind(&summary.kind).map(|_| ())
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
    // Normalize once so aliases are compared with the same rules as primary keys.
    // 只规范化一次，确保别名和主键使用同一套比较规则。
    let normalized = value.trim().to_ascii_lowercase();
    for kind in composer_attachment_kinds() {
        let metadata = composer_attachment_kind_metadata(&kind);
        if metadata.matches_key(&normalized) {
            return Ok(kind);
        }
    }
    Err(format!("不支持的附件类型：{normalized}"))
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
    composer_attachment_kind_metadata(kind).label
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
    composer_attachment_kind_metadata(kind).accept
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies composer attachment kind aliases parse through shared metadata.
    /// 验证输入区附件类型别名会通过共享元数据解析。
    #[test]
    fn parse_composer_attachment_kind_accepts_pdf_document_alias() {
        // Parsed attachment kind from the legacy document key.
        // 从旧版 document 键解析出的附件类型。
        let kind = parse_composer_attachment_kind(" Document ")
            .expect("document alias should parse as pdf attachment");

        assert_eq!(kind, ConversationAttachmentKind::Pdf);
    }

    /// Verifies labels and accept lists come from the shared metadata source.
    /// 验证标签和 accept 列表来自共享元数据源。
    #[test]
    fn composer_attachment_kind_metadata_feeds_label_and_accept() {
        // Core attachment kind selected for metadata projection.
        // 用于元数据投影的核心附件类型。
        let kind = ConversationAttachmentKind::Image;

        assert_eq!(composer_attachment_kind_label(&kind), "图片");
        assert_eq!(
            composer_attachment_kind_accept(&kind),
            "image/png,image/jpeg,image/webp,image/gif,image/bmp"
        );
        assert_eq!(
            composer_attachment_kind_accept(&ConversationAttachmentKind::Video),
            "video/mp4,video/mpeg,video/quicktime,video/mov,video/x-m4v,video/x-matroska,video/webm,video/x-msvideo,video/avi,video/x-flv,video/x-ms-wmv,video/wmv,video/3gpp,video/3gpp2"
        );
        assert_eq!(
            composer_attachment_kind_accept(&ConversationAttachmentKind::Audio),
            "audio/mpeg,audio/mp3,audio/wav,audio/x-wav,audio/mp4,audio/m4a,audio/x-m4a,audio/aac,audio/flac,audio/ogg,audio/oga,audio/opus,audio/pcm,audio/l16,audio/basic"
        );
    }

    /// Verifies borrowed summary validation rejects unknown attachment kinds.
    /// 验证借用式摘要校验会拒绝未知附件类型。
    #[test]
    fn validate_composer_attachment_summary_rejects_unknown_kind() {
        let summary = ComposerAttachmentSummary {
            kind: String::from("spreadsheet"),
            file_name: String::from("data.xlsx"),
            local_path: String::from("C:/tmp/data.xlsx"),
            source_path: None,
            asset_id: Some(String::from("asset")),
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: String::from(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
        };

        let error = validate_composer_attachment_summary(&summary)
            .expect_err("unknown attachment kind should be rejected");

        assert!(error.contains("spreadsheet"));
    }

    /// Verifies composer capabilities keep the expected kind ordering source.
    /// 验证输入区能力保持预期的附件类型顺序来源。
    #[test]
    fn composer_attachment_kinds_keep_capability_order() {
        assert_eq!(
            composer_attachment_kinds()
                .iter()
                .map(ConversationAttachmentKind::modality_name)
                .collect::<Vec<_>>(),
            vec!["image", "video", "audio", "pdf"]
        );
    }
}

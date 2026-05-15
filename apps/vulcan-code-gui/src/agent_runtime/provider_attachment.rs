//! Shared attachment helpers used by provider transports that need to re-read local multimodal inputs.
//! 供需要重新读取本地多模态输入的 Provider 传输复用的共享附件辅助模块。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use std::path::Path;
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind, CoreError as VulcanCoreError,
};

/// One resolved attachment payload ready for provider-specific request encoding.
/// 一份已解析完成、可直接用于 Provider 专属请求编码的附件载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderAttachmentPayload {
    /// Stable attachment kind used by provider-specific request encoders.
    /// 供 Provider 专属请求编码器使用的稳定附件类型。
    pub(crate) kind: ConversationAttachmentKind,
    /// Human-readable attachment file name surfaced in diagnostics and preview output.
    /// 展示在诊断与预览输出中的人类可读附件文件名。
    pub(crate) file_name: String,
    /// Stable MIME type carried into provider-native multimodal payloads.
    /// 需要带入 Provider 原生多模态载荷的稳定 MIME 类型。
    pub(crate) mime_type: String,
    /// Base64-encoded attachment bytes used by providers that accept inline binary content.
    /// 供接受内联二进制内容的 Provider 使用的 Base64 编码附件字节串。
    pub(crate) base64_data: String,
    /// Data-URL wrapper used by providers that expect URI-like multimodal payloads.
    /// 供需要类 URI 多模态载荷的 Provider 使用的 Data URL 包装串。
    pub(crate) data_url: String,
}

impl ProviderAttachmentPayload {
    /// Returns the OpenAI-compatible audio format when the attachment can be replayed through `input_audio`.
    /// 返回当前附件在可通过 `input_audio` 回放时对应的 OpenAI 兼容音频格式。
    pub(crate) fn openai_compatible_audio_format(&self) -> Option<&'static str> {
        if self.kind != ConversationAttachmentKind::Audio {
            return None;
        }
        match self.mime_type.as_str() {
            "audio/mpeg" => Some("mp3"),
            "audio/wav" | "audio/x-wav" => Some("wav"),
            _ => None,
        }
    }

    /// Returns whether the current attachment should be treated as one PDF document by transports that support document blocks.
    /// 返回对支持文档块的传输而言，当前附件是否应被视为一份 PDF 文档。
    pub(crate) fn is_pdf_document(&self) -> bool {
        self.kind == ConversationAttachmentKind::Pdf
            && self.mime_type.eq_ignore_ascii_case("application/pdf")
    }
}

/// Reads one staged conversation attachment from disk and prepares the provider-ready encoded payload.
/// 从磁盘读取一条已暂存会话附件，并准备好 Provider 可用的编码后载荷。
pub(crate) fn read_provider_attachment(
    attachment: &ConversationAttachment,
) -> Result<ProviderAttachmentPayload, VulcanCoreError> {
    let local_path = attachment.local_path.trim();
    if local_path.is_empty() {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{}附件 `{}` 缺少本地文件路径。",
                attachment.kind.display_label(),
                attachment.file_name
            ),
        });
    }

    let file_path = Path::new(local_path);
    let attachment_bytes =
        std::fs::read(file_path).map_err(|error| VulcanCoreError::InvalidState {
            message: format!(
                "读取{}附件 `{}` 失败：{}",
                attachment.kind.display_label(),
                attachment.file_name,
                error
            ),
        })?;
    let mime_type = resolve_attachment_mime_type(attachment, file_path)?;
    let base64_data = BASE64_STANDARD.encode(attachment_bytes);
    let data_url = format!("data:{mime_type};base64,{base64_data}");

    Ok(ProviderAttachmentPayload {
        kind: attachment.kind.clone(),
        file_name: attachment.file_name.clone(),
        mime_type,
        base64_data,
        data_url,
    })
}

/// Builds one OpenCode-style fallback text that tells the model the current API path cannot read a staged attachment.
/// 构建一条 OpenCode 风格的回退文本，告知模型当前 API 路径无法读取某条已暂存附件。
pub(crate) fn build_attachment_transport_error_text(
    attachment: &ConversationAttachment,
    scope: &str,
) -> String {
    format!(
        "ERROR: Cannot read \"{}\" ({} does not support {} input). Inform the user.",
        attachment.file_name,
        scope.trim(),
        attachment.kind.modality_name()
    )
}

/// Resolves one stable attachment MIME type from the staged metadata or its local-path extension.
/// 从已暂存附件元数据或其本地路径扩展名中解析稳定的附件 MIME 类型。
fn resolve_attachment_mime_type(
    attachment: &ConversationAttachment,
    file_path: &Path,
) -> Result<String, VulcanCoreError> {
    let declared_mime_type = attachment.mime_type.trim();
    if !declared_mime_type.is_empty() {
        return Ok(declared_mime_type.to_string());
    }

    let Some(extension) = file_path.extension().and_then(|value| value.to_str()) else {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{}附件 `{}` 缺少可识别扩展名，无法推断 MIME 类型。",
                attachment.kind.display_label(),
                attachment.file_name
            ),
        });
    };
    let normalized_extension = extension.trim().to_ascii_lowercase();
    let inferred_mime_type = match (attachment.kind.clone(), normalized_extension.as_str()) {
        (ConversationAttachmentKind::Image, "png") => "image/png",
        (ConversationAttachmentKind::Image, "jpg" | "jpeg") => "image/jpeg",
        (ConversationAttachmentKind::Image, "webp") => "image/webp",
        (ConversationAttachmentKind::Image, "gif") => "image/gif",
        (ConversationAttachmentKind::Image, "bmp") => "image/bmp",
        (ConversationAttachmentKind::Audio, "mp3") => "audio/mpeg",
        (ConversationAttachmentKind::Audio, "wav") => "audio/wav",
        (ConversationAttachmentKind::Audio, "m4a") => "audio/mp4",
        (ConversationAttachmentKind::Audio, "aac") => "audio/aac",
        (ConversationAttachmentKind::Audio, "flac") => "audio/flac",
        (ConversationAttachmentKind::Audio, "ogg") => "audio/ogg",
        (ConversationAttachmentKind::Video, "mp4") => "video/mp4",
        (ConversationAttachmentKind::Video, "mov") => "video/quicktime",
        (ConversationAttachmentKind::Video, "mkv") => "video/x-matroska",
        (ConversationAttachmentKind::Video, "webm") => "video/webm",
        (ConversationAttachmentKind::Video, "avi") => "video/x-msvideo",
        (ConversationAttachmentKind::Pdf, "pdf") => "application/pdf",
        _ => {
            return Err(VulcanCoreError::InvalidState {
                message: format!(
                    "{}附件 `{}` 的扩展名 `{normalized_extension}` 暂不受支持。",
                    attachment.kind.display_label(),
                    attachment.file_name
                ),
            });
        }
    };

    Ok(String::from(inferred_mime_type))
}

#[cfg(test)]
mod tests {
    use super::{build_attachment_transport_error_text, read_provider_attachment};
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one temporary test attachment with the provided extension, kind, and MIME type.
    /// 使用给定扩展名、类型与 MIME 类型构建一条临时测试附件。
    fn build_test_attachment(
        extension: &str,
        kind: ConversationAttachmentKind,
        mime_type: &str,
    ) -> ConversationAttachment {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let file_path = std::env::temp_dir().join(format!(
            "vulcan-provider-attachment-{unique_id}.{extension}"
        ));
        std::fs::write(&file_path, [0_u8, 1, 2, 3]).expect("temporary attachment should write");

        ConversationAttachment {
            kind,
            file_name: format!("demo.{extension}"),
            local_path: file_path.to_string_lossy().into_owned(),
            mime_type: mime_type.to_string(),
        }
    }

    /// Verifies provider attachment reading returns both raw base64 and the matching data URL wrapper.
    /// 验证 Provider 附件读取会同时返回原始 Base64 与对应的 Data URL 包装串。
    #[test]
    fn read_provider_attachment_builds_data_url_payload() {
        let attachment =
            build_test_attachment("png", ConversationAttachmentKind::Image, "image/png");
        let payload =
            read_provider_attachment(&attachment).expect("attachment payload should build");

        assert_eq!(payload.kind, ConversationAttachmentKind::Image);
        assert_eq!(payload.mime_type, "image/png");
        assert_eq!(payload.base64_data, "AAECAw==");
        assert_eq!(payload.data_url, "data:image/png;base64,AAECAw==");

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies audio attachments expose the expected OpenAI-compatible `input_audio` format mapping.
    /// 验证音频附件会暴露期望的 OpenAI 兼容 `input_audio` 格式映射。
    #[test]
    fn read_provider_attachment_resolves_audio_format_mapping() {
        let attachment =
            build_test_attachment("mp3", ConversationAttachmentKind::Audio, "audio/mpeg");
        let payload =
            read_provider_attachment(&attachment).expect("attachment payload should build");

        assert_eq!(payload.openai_compatible_audio_format(), Some("mp3"));

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies PDF attachments keep the expected MIME type for document-style transports.
    /// 验证 PDF 附件会保留供文档类传输使用的期望 MIME 类型。
    #[test]
    fn read_provider_attachment_keeps_pdf_document_mime_type() {
        let attachment =
            build_test_attachment("pdf", ConversationAttachmentKind::Pdf, "application/pdf");
        let payload =
            read_provider_attachment(&attachment).expect("attachment payload should build");

        assert!(payload.is_pdf_document());
        assert_eq!(payload.data_url, "data:application/pdf;base64,AAECAw==");

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies the OpenCode-style fallback text keeps the file name and modality readable for the model.
    /// 验证 OpenCode 风格的回退文本会为模型保留可读的文件名与模态说明。
    #[test]
    fn build_attachment_transport_error_text_mentions_file_name_and_modality() {
        let attachment =
            build_test_attachment("mp4", ConversationAttachmentKind::Video, "video/mp4");

        let text = build_attachment_transport_error_text(&attachment, "this API path");

        assert!(text.contains("\"demo.mp4\""));
        assert!(text.contains("video input"));

        let _ = std::fs::remove_file(&attachment.local_path);
    }
}

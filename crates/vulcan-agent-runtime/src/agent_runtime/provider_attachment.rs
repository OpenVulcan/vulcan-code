//! Provider-visible diagnostics for unavailable staged attachments.
//! 已暂存附件不可用时的 Provider 可见诊断。

use vulcan_core::types::ConversationAttachment;

/// Builds fallback text when the selected public protocol cannot carry an attachment modality.
/// 当选中的公共协议无法承载某种附件模态时构建回退文本。
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

/// Builds fallback text for an attachment whose managed replay file has disappeared.
/// 为托管回放文件已经消失的附件构建回退文本。
pub(crate) fn build_missing_attachment_replay_text(attachment: &ConversationAttachment) -> String {
    format!(
        "ERROR: Attachment \"{}\" no longer exists. Please ignore it.",
        attachment.file_name
    )
}

#[cfg(test)]
mod tests {
    //! Focused tests for attachment fallback diagnostics.
    //! 附件回退诊断定向测试。

    use super::*;
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one deterministic test attachment.
    /// 构建一个确定性的测试附件。
    fn test_attachment() -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: String::from("screen.png"),
            local_path: String::from("C:/attachments/screen.png"),
            source_path: None,
            asset_id: Some(String::from("attachment-1")),
            byte_len: Some(10),
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Verifies unsupported-modality diagnostics retain the file and protocol scope.
    /// 验证不支持模态的诊断会保留文件与协议范围。
    #[test]
    fn transport_error_names_attachment_and_scope() {
        let message =
            build_attachment_transport_error_text(&test_attachment(), "anthropic-messages.v1");
        assert!(message.contains("screen.png"));
        assert!(message.contains("anthropic-messages.v1"));
    }
}

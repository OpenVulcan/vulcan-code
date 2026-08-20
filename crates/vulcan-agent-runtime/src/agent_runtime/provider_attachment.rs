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

/// Builds a provider-visible hint for an attachment that must be analyzed through a host tool.
/// 为必须通过宿主工具分析的附件构建 Provider 可见提示。
///
/// # Parameters
/// 参数
/// `attachment` supplies the retained file name and local path; `tool_id` and `path_argument` identify the visible tool contract.
/// `attachment` 提供保留的文件名与本地路径；`tool_id` 与 `path_argument` 标识可见工具契约。
///
/// # Returns
/// 返回值
/// A deterministic informational line that instructs the model to call the selected host tool.
/// 一条确定性信息，指导模型调用选中的宿主工具。
pub(crate) fn build_attachment_provider_tool_hint(
    attachment: &ConversationAttachment,
    tool_id: &str,
    path_argument: &str,
) -> String {
    format!(
        "INFO: Attachment \"{}\" is kept locally because the selected route cannot consume {} input directly. Use host AI tool `{tool_id}` with `{path_argument}` set to `{}`.",
        attachment.file_name,
        attachment.kind.modality_name(),
        attachment.local_path
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

    /// Verifies host-tool hints name the actual media modality instead of a fixed image label.
    /// 验证宿主工具提示使用实际媒体模态，而不是固定图片标签。
    #[test]
    fn provider_tool_hint_names_actual_media_modality() {
        // attachment differs from the base fixture only by its video identity.
        // attachment 仅通过视频身份区别于基础夹具。
        let mut attachment = test_attachment();
        attachment.kind = ConversationAttachmentKind::Video;
        attachment.file_name = String::from("clip.mp4");
        attachment.local_path = String::from("C:/attachments/clip.mp4");
        attachment.mime_type = String::from("video/mp4");

        let message =
            build_attachment_provider_tool_hint(&attachment, "video_understand", "video_path");

        assert!(message.contains("video input"));
        assert!(message.contains("video_understand"));
        assert!(message.contains("video_path"));
        assert!(!message.contains("image input"));
    }
}

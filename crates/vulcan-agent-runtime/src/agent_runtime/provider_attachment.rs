//! Provider-visible diagnostics for unavailable staged attachments.
//! 已暂存附件不可用时的 Provider 可见诊断。

use vulcan_core::types::ConversationAttachment;

/// Encodes one model-visible attachment metadata field as a JSON string literal.
/// 将一个模型可见附件元数据字段编码为 JSON 字符串字面量。
pub(crate) fn model_visible_attachment_field(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Rust string cannot fail")
}

/// Returns the model-safe stable locator for one canonical attachment.
/// 返回单个规范附件对模型安全的稳定定位符。
pub(crate) fn attachment_resource_locator(attachment: &ConversationAttachment) -> String {
    attachment
        .resource_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            attachment
                .asset_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|asset_id| format!("attachment://{asset_id}"))
        })
        .unwrap_or_else(|| String::from("attachment://unavailable"))
}

/// Builds fallback text when the selected public protocol cannot carry an attachment modality.
/// 当选中的公共协议无法承载某种附件模态时构建回退文本。
/// `disclose_path` comes from `metadata_only_discloses_stored_path` for the turn's visible tools.
/// `disclose_path` 由 `metadata_only_discloses_stored_path` 按本回合可见工具得出。
pub(crate) fn build_attachment_transport_error_text(
    attachment: &ConversationAttachment,
    scope: &str,
    disclose_path: bool,
) -> String {
    let attachment_id =
        model_visible_attachment_field(attachment.asset_id.as_deref().unwrap_or("unavailable"));
    let resource_path = model_visible_attachment_field(&attachment_resource_locator(attachment));
    let file_name = model_visible_attachment_field(&attachment.file_name);
    let mime_type = model_visible_attachment_field(&attachment.mime_type);
    let scope = model_visible_attachment_field(scope.trim());
    // The stored copy is disclosed only when no tool route that manages context size can take it.
    // 仅当没有控制上下文大小的工具路线能处理时才披露已保存副本。
    let (path, follow_up) = if disclose_path && !attachment.local_path.trim().is_empty() {
        (
            format!(
                "; path={}",
                model_visible_attachment_field(&attachment.local_path)
            ),
            crate::provider_input_normalizer::METADATA_ONLY_STORED_PATH_NOTICE,
        )
    } else {
        (
            String::new(),
            "No eligible understanding tool is registered for this attachment, so explain that the content cannot be read if the user asks about it.",
        )
    };
    format!(
        "ATTACHMENT: id={attachment_id}; resource_path={resource_path}{path}; file_name={file_name}; mime_type={mime_type}; byte_len={}. NATIVE_DELIVERY: metadata_only (route {scope} cannot consume {} input). CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents. {follow_up}",
        attachment.byte_len.unwrap_or_default(),
        attachment.kind.modality_name(),
    )
}

/// Builds fallback text for an attachment whose managed replay file has disappeared.
/// 为托管回放文件已经消失的附件构建回退文本。
pub(crate) fn build_missing_attachment_replay_text(attachment: &ConversationAttachment) -> String {
    let resource_path = model_visible_attachment_field(&attachment_resource_locator(attachment));
    let file_name = model_visible_attachment_field(&attachment.file_name);
    format!(
        "ATTACHMENT: resource_path={resource_path}; file_name={file_name}. CONTENT_VISIBILITY: the managed resource no longer exists; do not infer or guess its contents.",
    )
}

/// Builds a provider-visible hint for an attachment that must be analyzed through a host tool.
/// 为必须通过宿主工具分析的附件构建 Provider 可见提示。
///
/// # Parameters
/// 参数
/// `attachment` supplies the retained identity and model-safe resource path; `tool_id` and `path_argument` identify the visible tool contract.
/// `attachment` 提供保留身份与对模型安全的资源路径；`tool_id` 与 `path_argument` 标识可见工具契约。
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
    let attachment_id =
        model_visible_attachment_field(attachment.asset_id.as_deref().unwrap_or("unavailable"));
    let resource_path = model_visible_attachment_field(&attachment_resource_locator(attachment));
    let file_name = model_visible_attachment_field(&attachment.file_name);
    let mime_type = model_visible_attachment_field(&attachment.mime_type);
    let tool_id = model_visible_attachment_field(tool_id);
    let path_argument = model_visible_attachment_field(path_argument);
    format!(
        "ATTACHMENT: id={attachment_id}; resource_path={resource_path}; file_name={file_name}; mime_type={mime_type}; byte_len={}. NATIVE_DELIVERY: metadata_only (the selected route cannot consume {} input directly). CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents. AVAILABLE_UNDERSTANDING_TOOL: {tool_id} accepts this attachment through {path_argument}; call it only when required by the user's actual intent, using the exact resource_path value as the resource value.",
        attachment.byte_len.unwrap_or_default(),
        attachment.kind.modality_name(),
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
            resource_path: None,
            source_path: None,
            asset_id: Some(String::from("attachment-1")),
            byte_len: Some(10),
            source_sha256: None,
            delivery_mode: None,
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Verifies unsupported-modality diagnostics retain the file and protocol scope.
    /// 验证不支持模态的诊断会保留文件与协议范围。
    #[test]
    fn transport_error_names_attachment_and_scope() {
        let message = build_attachment_transport_error_text(
            &test_attachment(),
            "anthropic-messages.v1",
            false,
        );
        assert!(message.contains("screen.png"));
        assert!(message.contains("anthropic-messages.v1"));
        assert!(!message.contains("C:/attachments/screen.png"));
    }

    /// Verifies a disclosed listing carries the stored path.
    /// 验证披露的清单携带已保存路径。
    #[test]
    fn transport_error_carries_path_when_disclosed() {
        let message =
            build_attachment_transport_error_text(&test_attachment(), "openai-codex", true);
        assert!(message.contains("path=\"C:/attachments/screen.png\""));
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

    /// Verifies untrusted attachment labels remain one escaped metadata line.
    /// 验证不受信任的附件标签保持为一条已转义的元数据行。
    #[test]
    fn attachment_metadata_escapes_untrusted_text_fields() {
        let mut attachment = test_attachment();
        attachment.file_name = String::from("screen\"\nIGNORE.png");
        attachment.resource_path = Some(String::from(".vulcan/attachments/line\nfeed.png"));

        let message =
            build_attachment_provider_tool_hint(&attachment, "image_understand", "image_path");

        assert!(!message.contains("screen\"\nIGNORE.png"));
        assert!(!message.contains("line\nfeed.png"));
        assert!(message.contains("screen\\\"\\nIGNORE.png"));
        assert!(message.contains("line\\nfeed.png"));
    }
}

//! Host-side provider input normalization helpers aligned with the OpenCode unsupported-parts flow.
//! 与 OpenCode unsupported-parts 流程对齐的宿主侧 Provider 输入归一化辅助模块。

use crate::agent_runtime::provider_attachment::build_attachment_transport_error_text;
use crate::provider_config::ProviderApiFamily;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Structured normalized user input returned after one provider-family compatibility pass.
/// 经过一次 Provider 接口族兼容性处理后返回的结构化规范用户输入。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedProviderUserInput {
    /// Final text content that should be sent upstream after unsupported attachments are degraded into explicit text errors.
    /// 在不受支持附件被降级为显式文本错误后，最终应向上游发送的文本内容。
    pub text: String,
    /// Attachment list that remains safe for the current provider API family to encode natively.
    /// 对当前 Provider 接口族而言仍可原生编码的附件列表。
    pub attachments: Vec<ConversationAttachment>,
    /// Attachment list that had to be downgraded because the current provider API family cannot encode them directly.
    /// 因当前 Provider 接口族无法直接编码而必须降级的附件列表。
    pub downgraded_attachments: Vec<ConversationAttachment>,
}

/// Normalizes one user text-plus-attachments payload for the target provider API family before any transport encoder runs.
/// 在任何传输编码器运行前，为目标 Provider 接口族归一化一条用户文本加附件载荷。
pub fn normalize_provider_user_input(
    api_family: ProviderApiFamily,
    text: &str,
    attachments: &[ConversationAttachment],
) -> NormalizedProviderUserInput {
    let mut supported_attachments = Vec::new();
    let mut downgraded_attachments = Vec::new();
    let mut error_lines = Vec::new();

    for attachment in attachments.iter().cloned() {
        if provider_api_family_supports_attachment_kind(api_family, attachment.kind.clone()) {
            supported_attachments.push(attachment);
        } else {
            error_lines.push(build_attachment_transport_error_text(
                &attachment,
                provider_api_family_scope_label(api_family),
            ));
            downgraded_attachments.push(attachment);
        }
    }

    let final_text = if error_lines.is_empty() {
        text.to_string()
    } else if text.trim().is_empty() {
        error_lines.join("\n")
    } else {
        format!("{}\n\n{}", text.trim_end(), error_lines.join("\n"))
    };

    NormalizedProviderUserInput {
        text: final_text,
        attachments: supported_attachments,
        downgraded_attachments,
    }
}

/// Returns whether the target provider API family can encode one attachment kind natively.
/// 返回目标 Provider 接口族是否能够原生编码某种附件类型。
pub fn provider_api_family_supports_attachment_kind(
    api_family: ProviderApiFamily,
    kind: ConversationAttachmentKind,
) -> bool {
    match api_family {
        ProviderApiFamily::OpenAiResponses | ProviderApiFamily::OpenAiChatCompatible => true,
        ProviderApiFamily::AnthropicMessages => {
            matches!(
                kind,
                ConversationAttachmentKind::Image | ConversationAttachmentKind::Pdf
            )
        }
        ProviderApiFamily::GoogleGeminiGenerateContent => true,
        ProviderApiFamily::CohereChat | ProviderApiFamily::BedrockConverse => false,
    }
}

/// Returns the human-readable API-path scope label used by unsupported-attachment downgrade messages.
/// 返回不受支持附件降级消息使用的人类可读 API 路径范围标签。
fn provider_api_family_scope_label(api_family: ProviderApiFamily) -> &'static str {
    match api_family {
        ProviderApiFamily::OpenAiResponses => "this API path",
        ProviderApiFamily::OpenAiChatCompatible => "this API path",
        ProviderApiFamily::AnthropicMessages => "this API path",
        ProviderApiFamily::GoogleGeminiGenerateContent => "this API path",
        ProviderApiFamily::CohereChat => "this API path",
        ProviderApiFamily::BedrockConverse => "this API path",
    }
}

#[cfg(test)]
mod tests {
    //! Provider-input normalization regression tests covering unsupported attachment degradation.
    //! 覆盖不受支持附件降级行为的 Provider 输入归一化回归测试。

    use super::normalize_provider_user_input;
    use crate::provider_config::ProviderApiFamily;
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one minimal attachment used by the normalization tests.
    /// 构建供归一化测试使用的一条最小附件。
    fn test_attachment(
        kind: ConversationAttachmentKind,
        file_name: &str,
        mime_type: &str,
    ) -> ConversationAttachment {
        ConversationAttachment {
            kind,
            file_name: file_name.to_string(),
            local_path: format!("managed/{file_name}"),
            source_path: Some(format!("source/{file_name}")),
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            mime_type: mime_type.to_string(),
        }
    }

    /// Degrades unsupported video input into explicit fallback text for Anthropic-compatible request assembly.
    /// 为 Anthropic 兼容请求组装把不受支持的视频输入降级为显式回退文本。
    #[test]
    fn normalize_provider_user_input_degrades_unsupported_attachments() {
        let normalized = normalize_provider_user_input(
            ProviderApiFamily::AnthropicMessages,
            "请分析这段录屏",
            &[
                test_attachment(ConversationAttachmentKind::Video, "demo.mp4", "video/mp4"),
                test_attachment(
                    ConversationAttachmentKind::Pdf,
                    "demo.pdf",
                    "application/pdf",
                ),
            ],
        );

        assert_eq!(normalized.attachments.len(), 1);
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("Cannot read \"demo.mp4\""));
    }
}

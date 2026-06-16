//! Host-side provider input normalization helpers aligned with the OpenCode unsupported-parts flow.
//! 与 OpenCode unsupported-parts 流程对齐的宿主侧 Provider 输入归一化辅助模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
};
use crate::attachment_asset_index::{AttachmentReplayStatus, resolve_attachment_replay_status};
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::AiToolKind;
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
    normalize_provider_user_input_for_model(api_family, None, text, attachments)
}

/// Normalizes one user text-plus-attachments payload for both provider-family and model modality compatibility.
/// 根据 Provider 接口族与模型模态兼容性归一化一条用户文本加附件载荷。
pub fn normalize_provider_user_input_for_model(
    api_family: ProviderApiFamily,
    capabilities: Option<&ResolvedModelCapabilities>,
    text: &str,
    attachments: &[ConversationAttachment],
) -> NormalizedProviderUserInput {
    let mut supported_attachments = Vec::new();
    let mut downgraded_attachments = Vec::new();
    let mut prefix_lines = Vec::new();
    let mut suffix_lines = Vec::new();

    for attachment in attachments.iter().cloned() {
        let attachment_is_replayable = !matches!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::Missing | AttachmentReplayStatus::Purged
        );
        let api_supports_attachment =
            provider_api_family_supports_attachment_kind(api_family, attachment.kind.clone());
        let model_supports_attachment = capabilities.is_none_or(|capabilities| {
            capabilities.supports_attachment_kind(attachment.kind.clone())
        });

        if attachment_is_replayable && api_supports_attachment && model_supports_attachment {
            supported_attachments.push(attachment);
        } else if !attachment_is_replayable {
            suffix_lines.push(build_missing_attachment_replay_text(&attachment));
            downgraded_attachments.push(attachment);
        } else if !api_supports_attachment {
            suffix_lines.push(build_attachment_transport_error_text(
                &attachment,
                provider_api_family_scope_label(api_family),
            ));
            downgraded_attachments.push(attachment);
        } else if let Some(tool_hints) = capabilities.and_then(|capabilities| {
            bridgeable_ai_tool_hints_for_attachment(capabilities, &attachment)
        }) {
            prefix_lines.push(build_attachment_ai_tool_bridge_text(
                &attachment,
                &tool_hints,
            ));
            downgraded_attachments.push(attachment);
        } else {
            suffix_lines.push(build_attachment_transport_error_text(
                &attachment,
                "the selected model",
            ));
            downgraded_attachments.push(attachment);
        }
    }

    let trimmed_text = text.trim();
    let final_text = match (
        prefix_lines.is_empty(),
        trimmed_text.is_empty(),
        suffix_lines.is_empty(),
    ) {
        (true, true, true) => String::new(),
        (true, false, true) => text.to_string(),
        (true, true, false) => suffix_lines.join("\n"),
        (true, false, false) => format!("{}\n\n{}", text.trim_end(), suffix_lines.join("\n")),
        (false, true, true) => prefix_lines.join("\n"),
        (false, false, true) => format!("{}\n\n{}", prefix_lines.join("\n"), trimmed_text),
        (false, true, false) => {
            format!("{}\n\n{}", prefix_lines.join("\n"), suffix_lines.join("\n"))
        }
        (false, false, false) => format!(
            "{}\n\n{}\n\n{}",
            prefix_lines.join("\n"),
            trimmed_text,
            suffix_lines.join("\n")
        ),
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
        ProviderApiFamily::OpenAiResponses => true,
        ProviderApiFamily::OpenAiChatCompatible => {
            matches!(kind, ConversationAttachmentKind::Image)
        }
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

/// Builds one provider-visible bridge hint for an attachment that should be handled through a host AI tool instead of direct upstream multimodal encoding.
/// 为应通过宿主 AI 工具而非直接上游多模态编码处理的附件构建一条 Provider 可见桥接提示。
fn build_attachment_ai_tool_bridge_text(
    attachment: &ConversationAttachment,
    tool_hints: &[(&str, &str)],
) -> String {
    let local_path = attachment.local_path.trim();
    let selector = attachment_ai_tool_selector(attachment);
    let resource_id = attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    let tool_summary = tool_hints
        .iter()
        .map(|(tool_name, parameter_name)| {
            format!("`{tool_name}`（传 `{parameter_name}=\"{selector}\"`）")
        })
        .collect::<Vec<_>>()
        .join("、");
    format!(
        "INFO: Attachment \"{}\" is kept locally and will not be uploaded as a native multimodal attachment for the current model. Resource id: `{}`. Local path: `{}`. If you need to inspect or reuse it, use host AI tools with the resource id or full local path: {}.",
        attachment.file_name, resource_id, local_path, tool_summary,
    )
}

/// Chooses the stable selector shown in bridge hints for host AI tool calls.
/// 为宿主 AI 工具调用提示选择稳定选择器。
fn attachment_ai_tool_selector(attachment: &ConversationAttachment) -> &str {
    attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| attachment.local_path.trim())
}

/// Resolves the host AI tools that can still consume one downgraded attachment kind.
/// 解析在附件被降级后仍可消费该附件类型的宿主 AI 工具集合。
fn bridgeable_ai_tool_hints_for_attachment<'a>(
    capabilities: &'a ResolvedModelCapabilities,
    attachment: &ConversationAttachment,
) -> Option<Vec<(&'a str, &'a str)>> {
    let mut hints = Vec::new();

    match attachment.kind {
        ConversationAttachmentKind::Image => {
            if capabilities.has_enabled_ai_tool(AiToolKind::UnderstandImage) {
                hints.push(("understand_image", "image"));
            }
            if capabilities.has_enabled_ai_tool(AiToolKind::ImageGeneration) {
                hints.push(("image_generation", "subject_image"));
            }
            if capabilities.has_enabled_ai_tool(AiToolKind::VideoGeneration) {
                hints.push(("video_generation", "first_image"));
            }
        }
        ConversationAttachmentKind::Audio => {
            if capabilities.has_enabled_ai_tool(AiToolKind::MusicCover) {
                hints.push(("music_cover", "audio"));
            }
        }
        ConversationAttachmentKind::Video | ConversationAttachmentKind::Pdf => {}
    }

    (!hints.is_empty()).then_some(hints)
}

#[cfg(test)]
mod tests {
    //! Provider-input normalization regression tests covering unsupported attachment degradation.
    //! 覆盖不受支持附件降级行为的 Provider 输入归一化回归测试。

    use super::{normalize_provider_user_input, normalize_provider_user_input_for_model};
    use crate::provider_capabilities::ResolvedModelCapabilities;
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
            local_path: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("Cargo.toml")
                .to_string_lossy()
                .to_string(),
            source_path: Some(format!("source/{file_name}")),
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            preview_local_path: None,
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

    /// Degrades API-supported attachments when the selected model does not declare the matching input modality.
    /// 当选中模型未声明对应输入模态时，即使 API 支持该附件也会降级。
    #[test]
    fn normalize_provider_user_input_degrades_model_unsupported_attachments() {
        let capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text")],
            ..ResolvedModelCapabilities::default()
        };
        let normalized = normalize_provider_user_input_for_model(
            ProviderApiFamily::OpenAiChatCompatible,
            Some(&capabilities),
            "请看图",
            &[test_attachment(
                ConversationAttachmentKind::Image,
                "demo.png",
                "image/png",
            )],
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("selected model"));
    }

    /// Bridges image attachments into the host `understand_image` tool when the API path supports images but the selected model does not.
    /// 当接口路径支持图片但所选模型不支持时，将图片附件桥接到宿主 `understand_image` 工具。
    #[test]
    fn normalize_provider_user_input_bridges_image_to_understand_image_tool() {
        let capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text")],
            ai_tools: vec![crate::provider_capabilities::ResolvedAiToolCapability {
                kind: crate::provider_config::AiToolKind::UnderstandImage,
                policy: crate::provider_config::AiToolPolicy::ForceHost,
                native_provider_tool: false,
                native_tool_type: None,
                native_request_patch: None,
                fallback_tool_id: Some(String::from("vulcan_understand_image_chain")),
                requires_approval: false,
                result_modalities: vec![String::from("text")],
                citation_support: false,
                source_label: String::from("用户配置"),
            }],
            ..ResolvedModelCapabilities::default()
        };
        let normalized = normalize_provider_user_input_for_model(
            ProviderApiFamily::OpenAiChatCompatible,
            Some(&capabilities),
            "请看图",
            &[test_attachment(
                ConversationAttachmentKind::Image,
                "demo.png",
                "image/png",
            )],
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.starts_with("INFO: Attachment"));
        assert!(normalized.text.contains("understand_image"));
        assert!(normalized.text.contains("Resource id: `asset-demo.png`"));
        assert!(normalized.text.contains("image=\"asset-demo.png\""));
        assert!(
            normalized
                .text
                .contains("will not be uploaded as a native multimodal attachment")
        );
        assert!(normalized.text.contains("Cargo.toml"));
        assert!(normalized.text.ends_with("请看图"));
    }

    /// Degrades non-image attachments for strict OpenAI-compatible Chat endpoints.
    /// 为严格 OpenAI 兼容 Chat 端点降级非图片附件。
    #[test]
    fn normalize_provider_user_input_degrades_openai_chat_non_image_attachments() {
        let normalized = normalize_provider_user_input(
            ProviderApiFamily::OpenAiChatCompatible,
            "请分析这些附件",
            &[
                test_attachment(ConversationAttachmentKind::Audio, "demo.mp3", "audio/mpeg"),
                test_attachment(
                    ConversationAttachmentKind::Pdf,
                    "demo.pdf",
                    "application/pdf",
                ),
                test_attachment(ConversationAttachmentKind::Video, "demo.mp4", "video/mp4"),
            ],
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 3);
        assert!(normalized.text.contains("this API path"));
        assert!(normalized.text.contains("demo.mp3"));
        assert!(normalized.text.contains("demo.pdf"));
        assert!(normalized.text.contains("demo.mp4"));
    }
}

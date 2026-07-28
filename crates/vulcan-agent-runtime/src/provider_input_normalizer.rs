//! Host-side attachment normalization for the public Vulcan AI contract.
//! 面向 Vulcan AI 公共契约的宿主附件归一化模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
};
use crate::attachment_asset_index::{AttachmentReplayStatus, resolve_attachment_replay_status};
use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Holds the exact user payload that remains valid after protocol and model capability checks.
/// 保存经过协议与模型能力检查后仍然有效的精确用户载荷。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedProviderUserInput {
    /// Contains final text including explicit rejection messages for unusable attachments.
    /// 包含最终文本以及不可用附件的显式拒绝消息。
    pub text: String,
    /// Contains attachments accepted by both the selected protocol profile and model.
    /// 包含同时被选定协议档案与模型接受的附件。
    pub attachments: Vec<ConversationAttachment>,
    /// Contains attachments excluded from native upstream projection.
    /// 包含从原生上游投影中排除的附件。
    pub downgraded_attachments: Vec<ConversationAttachment>,
}

/// Normalizes one user payload against the selected protocol profile.
/// 根据选定协议档案归一化一条用户载荷。
///
/// `protocol_profile_id` identifies the immutable wire profile selected for this execution.
/// `protocol_profile_id` 标识本次执行选定的不可变 Wire 协议档案。
///
/// Returns the text and attachment subset safe to project upstream.
/// 返回可安全投影到上游的文本和附件子集。
pub fn normalize_provider_user_input(
    protocol_profile_id: &str,
    text: &str,
    attachments: &[ConversationAttachment],
) -> NormalizedProviderUserInput {
    normalize_provider_user_input_for_model(protocol_profile_id, None, text, attachments)
}

/// Normalizes one user payload against both the protocol profile and public model capabilities.
/// 根据协议档案与公共模型能力归一化一条用户载荷。
///
/// `capabilities` is the exact in-memory model capability snapshot captured for the execution.
/// `capabilities` 是为本次执行捕获的精确内存模型能力快照。
///
/// Returns a deterministic payload without legacy host-tool fallback instructions.
/// 返回不包含旧宿主工具回退提示的确定性载荷。
pub fn normalize_provider_user_input_for_model(
    protocol_profile_id: &str,
    capabilities: Option<&ModelCapabilities>,
    text: &str,
    attachments: &[ConversationAttachment],
) -> NormalizedProviderUserInput {
    let mut supported_attachments = Vec::new();
    let mut downgraded_attachments = Vec::new();
    let mut rejection_lines = Vec::new();

    for attachment in attachments.iter().cloned() {
        let replayable = !matches!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::Missing | AttachmentReplayStatus::Purged
        );
        let protocol_supported =
            protocol_profile_supports_attachment_kind(protocol_profile_id, attachment.kind.clone());
        let model_supported = capabilities
            .map(|value| {
                value
                    .input_resources
                    .contains(&attachment_resource_kind(attachment.kind.clone()))
            })
            .unwrap_or(true);

        if replayable && protocol_supported && model_supported {
            supported_attachments.push(attachment);
        } else {
            let rejection = if !replayable {
                build_missing_attachment_replay_text(&attachment)
            } else if !protocol_supported {
                build_attachment_transport_error_text(&attachment, "the selected protocol profile")
            } else {
                build_attachment_transport_error_text(&attachment, "the selected model")
            };
            rejection_lines.push(rejection);
            downgraded_attachments.push(attachment);
        }
    }

    let text = match (text.trim().is_empty(), rejection_lines.is_empty()) {
        (true, true) => String::new(),
        (false, true) => text.to_string(),
        (true, false) => rejection_lines.join("\n"),
        (false, false) => format!("{}\n\n{}", text.trim_end(), rejection_lines.join("\n")),
    };

    NormalizedProviderUserInput {
        text,
        attachments: supported_attachments,
        downgraded_attachments,
    }
}

/// Reports whether a protocol profile has a real encoder for one attachment kind.
/// 报告某个协议档案是否具有指定附件类型的真实编码器。
///
/// Unknown profiles are rejected because the public protocol registry is the sole authority.
/// 未知档案会被拒绝，因为公共协议注册表是唯一权威来源。
pub fn protocol_profile_supports_attachment_kind(
    protocol_profile_id: &str,
    kind: ConversationAttachmentKind,
) -> bool {
    match protocol_profile_id {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => true,
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
            matches!(kind, ConversationAttachmentKind::Image)
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(
                kind,
                ConversationAttachmentKind::Image | ConversationAttachmentKind::Video
            )
        }
        _ => false,
    }
}

/// Maps a conversation attachment kind to the public resource contract.
/// 将会话附件类型映射到公共资源契约。
fn attachment_resource_kind(kind: ConversationAttachmentKind) -> ResourceKind {
    match kind {
        ConversationAttachmentKind::Image => ResourceKind::Image,
        ConversationAttachmentKind::Audio => ResourceKind::Audio,
        ConversationAttachmentKind::Video => ResourceKind::Video,
        ConversationAttachmentKind::Pdf => ResourceKind::File,
    }
}

#[cfg(test)]
mod tests {
    //! Public protocol attachment normalization regression tests.
    //! 公共协议附件归一化回归测试。

    use super::{normalize_provider_user_input, normalize_provider_user_input_for_model};
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one replayable attachment fixture.
    /// 构建一个可重放附件夹具。
    fn test_attachment(
        kind: ConversationAttachmentKind,
        file_name: &str,
        mime_type: &str,
    ) -> ConversationAttachment {
        ConversationAttachment {
            kind,
            file_name: file_name.to_string(),
            local_path: std::env::current_dir()
                .expect("current directory should be available")
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

    /// Keeps every resource implemented by the OpenAI Chat wire profile.
    /// 保留 OpenAI Chat Wire 档案已经实现的全部资源。
    #[test]
    fn openai_chat_keeps_implemented_resources() {
        let attachments = vec![
            test_attachment(ConversationAttachmentKind::Image, "a.png", "image/png"),
            test_attachment(ConversationAttachmentKind::Audio, "a.mp3", "audio/mpeg"),
            test_attachment(ConversationAttachmentKind::Video, "a.mp4", "video/mp4"),
            test_attachment(ConversationAttachmentKind::Pdf, "a.pdf", "application/pdf"),
        ];
        let normalized = normalize_provider_user_input(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            "inspect",
            &attachments,
        );

        assert_eq!(normalized.attachments, attachments);
        assert!(normalized.downgraded_attachments.is_empty());
    }

    /// Rejects non-image resources from the currently implemented Responses wire profile.
    /// 从当前已实现的 Responses Wire 档案中拒绝非图片资源。
    #[test]
    fn responses_rejects_non_image_resources() {
        let normalized = normalize_provider_user_input(
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "inspect",
            &[
                test_attachment(ConversationAttachmentKind::Image, "a.png", "image/png"),
                test_attachment(ConversationAttachmentKind::Audio, "a.mp3", "audio/mpeg"),
            ],
        );

        assert_eq!(normalized.attachments.len(), 1);
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("selected protocol profile"));
    }

    /// Rejects non-image resources from the currently implemented Anthropic wire profile.
    /// 从当前已实现的 Anthropic Wire 档案中拒绝非图片资源。
    #[test]
    fn anthropic_rejects_non_image_resources() {
        let normalized = normalize_provider_user_input(
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
            "inspect",
            &[test_attachment(
                ConversationAttachmentKind::Pdf,
                "a.pdf",
                "application/pdf",
            )],
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
    }

    /// Applies the exact public model capability snapshot after protocol validation.
    /// 在协议验证后应用精确的公共模型能力快照。
    #[test]
    fn model_capability_snapshot_rejects_unlisted_resources() {
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Image],
            ..ModelCapabilities::default()
        };
        let normalized = normalize_provider_user_input_for_model(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            Some(&capabilities),
            "inspect",
            &[test_attachment(
                ConversationAttachmentKind::Audio,
                "a.mp3",
                "audio/mpeg",
            )],
        );

        assert!(normalized.attachments.is_empty());
        assert!(normalized.text.contains("selected model"));
    }

    /// Rejects every attachment for an unregistered protocol profile.
    /// 对未注册协议档案拒绝全部附件。
    #[test]
    fn unknown_protocol_profile_rejects_resources() {
        let normalized = normalize_provider_user_input(
            "unregistered",
            "inspect",
            &[test_attachment(
                ConversationAttachmentKind::Image,
                "a.png",
                "image/png",
            )],
        );

        assert!(normalized.attachments.is_empty());
        assert!(normalized.text.contains("selected protocol profile"));
    }
}

//! Host-side attachment normalization for the public Vulcan AI contract.
//! 面向 Vulcan AI 公共契约的宿主附件归一化模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_provider_tool_hint, build_attachment_transport_error_text,
    build_missing_attachment_replay_text,
};
use crate::attachment_asset_index::{
    AttachmentReplayStatus, is_project_resource_attachment, resolve_attachment_replay_status,
};
use vulcan_ai_contract::{ModelCapabilities, ResourceKind, StructuredOutputCapability};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentDeliveryMode, ConversationAttachmentKind,
};

/// Instruction following the stored path that a metadata-only listing discloses.
/// 仅元数据清单披露已保存路径后附带的说明。
pub const METADATA_ONLY_STORED_PATH_NOTICE: &str = "The stored copy of this attachment is at `path`; read or process it with other available tools such as file reads, shell commands or scripts. The stored copy may have no file extension: use file_name and mime_type to choose a parser, and copy it to a name with the extension if a tool needs one. Do not modify the stored copy.";

/// Reports whether a metadata-only listing of `kind` discloses the stored file path.
/// 报告`kind`类的仅元数据清单是否披露已保存文件路径。
/// `tool_ids` are the tools visible this turn; `model_accepts` tells whether the model takes a resource kind natively.
/// `tool_ids`是本回合可见工具；`model_accepts`说明模型是否原生接受某资源类型。
/// Documents always disclose it. Images, audio and video disclose it only when neither their understanding tool
/// nor `read_media` can take them, because those routes manage context size.
/// 文档总是披露。图片、音频与视频仅在其理解工具及`read_media`都不能处理时披露，因为这些路线负责控制上下文大小。
pub fn metadata_only_discloses_stored_path(
    kind: &ConversationAttachmentKind,
    tool_ids: &[String],
    model_accepts: impl Fn(ResourceKind) -> bool,
) -> bool {
    let Some((understanding_tool, _)) = media_understanding_tool_for_kind(kind) else {
        return true;
    };
    let visible = |id: &str| tool_ids.iter().any(|tool| tool == id);
    let read_media = visible(crate::native_media_input::READ_MEDIA_TOOL_ID)
        && model_accepts(attachment_resource_kind(kind.clone()));
    !visible(understanding_tool) && !read_media
}

/// Returns the fixed understanding tool id and its path argument for a media `kind`, or `None` for documents.
/// 返回媒体`kind`对应的固定理解工具标识及其路径参数；文档返回`None`。
fn media_understanding_tool_for_kind(
    kind: &ConversationAttachmentKind,
) -> Option<(&'static str, &'static str)> {
    match kind {
        ConversationAttachmentKind::Image => Some(("image_understand", "image_path")),
        ConversationAttachmentKind::Video => Some(("video_understand", "video_path")),
        ConversationAttachmentKind::Audio => Some(("audio_understand", "audio_path")),
        ConversationAttachmentKind::File | ConversationAttachmentKind::Pdf => None,
    }
}

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
    normalize_provider_user_input_for_model_with_tool_bridges(
        protocol_profile_id,
        capabilities,
        text,
        attachments,
        &[],
    )
}

/// Normalizes one user payload while allowing exact registered media-understanding bridges.
/// 在允许精确注册的媒体理解桥接工具时归一化一条用户载荷。
///
/// `bridge_tool_ids` contains only the tool identifiers visible in the current frozen execution.
/// `bridge_tool_ids` 只包含当前冻结执行中对模型可见的工具标识。
///
/// # Returns
/// 返回值
/// A normalized payload with bridged media removed from native attachments and described in text.
/// 返回从原生附件中移除桥接媒体并在文本中描述的规范载荷。
pub fn normalize_provider_user_input_for_model_with_tool_bridges(
    protocol_profile_id: &str,
    capabilities: Option<&ModelCapabilities>,
    text: &str,
    attachments: &[ConversationAttachment],
    bridge_tool_ids: &[String],
) -> NormalizedProviderUserInput {
    let mut supported_attachments = Vec::new();
    let mut downgraded_attachments = Vec::new();
    let mut diagnostic_lines = Vec::new();

    for attachment in attachments.iter().cloned() {
        // A V1 project resource has no managed copy; its media boundary delivers the bytes.
        // V1 项目资源没有托管副本；其字节由媒体边界投递。
        let replayable = is_project_resource_attachment(&attachment)
            || !matches!(
                resolve_attachment_replay_status(&attachment),
                AttachmentReplayStatus::Missing | AttachmentReplayStatus::Purged
            );
        // protocolSupported includes both content-block shape and the actual media MIME encoder.
        // protocolSupported 同时包含内容块形态与实际媒体 MIME 编码器。
        let protocol_supported =
            protocol_profile_supports_attachment(protocol_profile_id, &attachment);
        let model_supported = capabilities
            .map(|value| {
                value
                    .input_resources
                    .contains(&attachment_resource_kind(attachment.kind.clone()))
            })
            .unwrap_or(true);
        // metadataOnly is a request-local host decision and must override otherwise available native encoders.
        // metadataOnly 是单次请求局部宿主决策，必须覆盖原本可用的原生编码器。
        let metadata_only =
            attachment.delivery_mode == Some(ConversationAttachmentDeliveryMode::MetadataOnly);

        if replayable && protocol_supported && model_supported && !metadata_only {
            supported_attachments.push(attachment);
        } else {
            // Unknown capabilities count as native acceptance, matching model_supported above.
            // 未知能力按原生接受处理，与上方model_supported一致。
            let disclose_path =
                metadata_only_discloses_stored_path(&attachment.kind, bridge_tool_ids, |kind| {
                    capabilities
                        .map(|value| value.input_resources.contains(&kind))
                        .unwrap_or(true)
                });
            let diagnostic = if !replayable {
                build_missing_attachment_replay_text(&attachment)
            } else if let Some((tool_id, path_argument)) =
                media_understanding_bridge_for_attachment(&attachment, bridge_tool_ids)
            {
                // a host bridge replaces both missing model input and missing wire encoding.
                // 宿主桥接同时替代缺失的模型输入能力与 Wire 编码能力。
                build_attachment_provider_tool_hint(&attachment, tool_id, path_argument)
            } else if metadata_only {
                build_attachment_transport_error_text(
                    &attachment,
                    "the host delivery plan",
                    disclose_path,
                )
            } else if !protocol_supported {
                build_attachment_transport_error_text(
                    &attachment,
                    "the selected protocol profile",
                    disclose_path,
                )
            } else {
                build_attachment_transport_error_text(
                    &attachment,
                    "the selected model",
                    disclose_path,
                )
            };
            diagnostic_lines.push(diagnostic);
            downgraded_attachments.push(attachment);
        }
    }

    let text = match (text.trim().is_empty(), diagnostic_lines.is_empty()) {
        (true, true) => String::new(),
        (false, true) => text.to_string(),
        (true, false) => diagnostic_lines.join("\n"),
        (false, false) => format!("{}\n\n{}", text.trim_end(), diagnostic_lines.join("\n")),
    };

    NormalizedProviderUserInput {
        text,
        attachments: supported_attachments,
        downgraded_attachments,
    }
}

/// Selects the exact path argument contract for one registered media-understanding bridge.
/// 选择一个已注册媒体理解桥接工具的精确路径参数契约。
///
/// # Parameters
/// 参数
/// `attachment` is the retained conversation attachment; `bridge_tool_ids` are the exact visible tool identifiers.
/// `attachment` 是保留的会话附件；`bridge_tool_ids` 是精确可见的工具标识。
///
/// # Returns
/// 返回值
/// The visible tool identifier and its path argument name, or `None` when no exact bridge applies.
/// 返回可见工具标识及其路径参数名；没有精确桥接时返回 `None`。
fn media_understanding_bridge_for_attachment<'a>(
    attachment: &ConversationAttachment,
    bridge_tool_ids: &'a [String],
) -> Option<(&'a str, &'static str)> {
    media_understanding_tool_for_kind(&attachment.kind).and_then(|(expected_tool_id, path_argument)| {
        bridge_tool_ids
            .iter()
            .find(|tool_id| tool_id.as_str() == expected_tool_id)
            .map(|tool_id| (tool_id.as_str(), path_argument))
    })
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
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID
        | vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID
        | vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID => true,
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
        | vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID => {
            matches!(kind, ConversationAttachmentKind::Image)
        }
        vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(
                kind,
                ConversationAttachmentKind::Image | ConversationAttachmentKind::Video
            )
        }
        _ => false,
    }
}

/// Reports whether one frozen protocol can encode an attachment's exact kind and MIME type.
/// 报告一个冻结协议能否编码附件的精确类型与 MIME。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` identifies the exact encoder and `attachment` contains the concrete
/// attachment category and normalized MIME type.
/// `protocol_profile_id` 标识精确编码器，`attachment` 包含具体附件类别与规范 MIME。
///
/// # Returns
/// 返回值
/// `true` only when both the content-block projector and format-sensitive encoder are registered.
/// 仅当内容块投影器与格式敏感编码器均已注册时返回 `true`。
pub fn protocol_profile_supports_attachment(
    protocol_profile_id: &str,
    attachment: &ConversationAttachment,
) -> bool {
    // resourceKind normalizes the core attachment category for the shared protocol registry.
    // resourceKind 归一化核心附件类别，用于共享协议注册表。
    let resource_kind = attachment_resource_kind(attachment.kind.clone());
    // formatSupported preserves the existing generic-file projector while media uses exact MIME admission.
    // formatSupported 保留既有通用文件投影器，媒体则使用精确 MIME 准入。
    let format_supported = resource_kind == ResourceKind::File
        || vulcan_llm_core::protocol_profile_supports_local_media_type(
            protocol_profile_id,
            resource_kind,
            attachment.mime_type.as_str(),
        );
    protocol_profile_supports_attachment_kind(protocol_profile_id, attachment.kind.clone())
        && format_supported
}

/// Reports whether one frozen protocol can encode the declared structured-output policy.
/// 报告一个冻结协议能否编码声明的结构化输出策略。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` identifies the exact encoder and `capability` is the model-declared output mode.
/// `protocol_profile_id` 标识精确编码器，`capability` 是模型声明的输出模式。
///
/// # Returns
/// 返回值
/// `true` only for a registered encoder and an output mode it can represent without fallback.
/// 仅当编码器已注册且可在不降级的情况下表达该输出模式时返回 `true`。
pub fn protocol_profile_supports_structured_output(
    protocol_profile_id: &str,
    capability: StructuredOutputCapability,
) -> bool {
    if capability == StructuredOutputCapability::Unsupported {
        return false;
    }
    // registeredProfile prevents unknown future protocols from inheriting an unverified JSON contract.
    // registeredProfile 防止未知未来协议继承未经验证的 JSON 契约。
    let registered_profile = matches!(
        protocol_profile_id,
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID
            | vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
            | vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
            | vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID
            | vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
            | vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID
            | vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID
            | vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID
            | vulcan_llm_core::COHERE_CHAT_PROFILE_ID
    );
    if !registered_profile {
        return false;
    }
    // Anthropic-family encoders require an explicit JSON Schema and reject schema-free JSON Object mode.
    // Anthropic 系编码器要求显式 JSON Schema，并拒绝无 Schema 的 JSON Object 模式。
    if matches!(
        protocol_profile_id,
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID
            | vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
    ) {
        return capability == StructuredOutputCapability::JsonSchema;
    }
    true
}

/// Maps a conversation attachment kind to the public resource contract.
/// 将会话附件类型映射到公共资源契约。
fn attachment_resource_kind(kind: ConversationAttachmentKind) -> ResourceKind {
    match kind {
        ConversationAttachmentKind::Image => ResourceKind::Image,
        ConversationAttachmentKind::Audio => ResourceKind::Audio,
        ConversationAttachmentKind::Video => ResourceKind::Video,
        ConversationAttachmentKind::Pdf => ResourceKind::File,
        ConversationAttachmentKind::File => ResourceKind::File,
    }
}

#[cfg(test)]
mod tests {
    //! Public protocol attachment normalization regression tests.
    //! 公共协议附件归一化回归测试。

    use super::{
        normalize_provider_user_input, normalize_provider_user_input_for_model,
        normalize_provider_user_input_for_model_with_tool_bridges,
        protocol_profile_supports_attachment, protocol_profile_supports_attachment_kind,
        protocol_profile_supports_structured_output,
    };
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind, StructuredOutputCapability};
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
            resource_path: None,
            source_path: Some(format!("source/{file_name}")),
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            source_sha256: None,
            delivery_mode: None,
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
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
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
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
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

    /// Keeps image input for the Codex Responses profile while rejecting unregistered media kinds.
    /// 为 Codex Responses 协议保留图片输入，同时拒绝未注册的媒体类型。
    #[test]
    fn codex_responses_keeps_images_and_rejects_non_image_resources() {
        let normalized = normalize_provider_user_input(
            vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
            "inspect",
            &[
                test_attachment(ConversationAttachmentKind::Image, "a.png", "image/png"),
                test_attachment(ConversationAttachmentKind::Audio, "a.mp3", "audio/mpeg"),
            ],
        );

        assert_eq!(normalized.attachments.len(), 1);
        assert_eq!(
            normalized.attachments[0].kind,
            ConversationAttachmentKind::Image
        );
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert_eq!(
            normalized.downgraded_attachments[0].kind,
            ConversationAttachmentKind::Audio
        );
    }

    /// Rejects non-image resources from the currently implemented Anthropic wire profile.
    /// 从当前已实现的 Anthropic Wire 档案中拒绝非图片资源。
    #[test]
    fn anthropic_rejects_non_image_resources() {
        let normalized = normalize_provider_user_input(
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
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
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
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

    /// Keeps an attachment-only V1 project resource as a blank seed instead of a missing-file notice.
    /// 将仅附件的 V1 项目资源保留为空白播种，而不是缺失文件提示。
    #[test]
    fn project_resource_attachment_stays_a_blank_seed() {
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Image],
            ..ModelCapabilities::default()
        };
        let mut attachment =
            test_attachment(ConversationAttachmentKind::Image, "shot.png", "image/png");
        attachment.local_path = String::new();
        attachment.source_path = None;
        attachment.resource_path = Some(String::from("resources/shot.png"));
        attachment.asset_id = Some(crate::attachment_asset_index::project_resource_asset_id(1));
        let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            Some(&capabilities),
            "",
            &[attachment],
            &[],
        );

        assert!(normalized.text.is_empty());
        assert_eq!(normalized.attachments.len(), 1);
    }

    /// Downgrades unsupported image input to the exact visible image-understanding host tool.
    /// 将模型不支持的图片输入降级为精确可见的图片理解宿主工具。
    #[test]
    fn image_understanding_bridge_keeps_local_path_and_tool_contract() {
        let capabilities = ModelCapabilities::default();
        let bridge_tool_ids = vec![String::from("image_understand")];
        let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            Some(&capabilities),
            "inspect",
            &[test_attachment(
                ConversationAttachmentKind::Image,
                "a.png",
                "image/png",
            )],
            &bridge_tool_ids,
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("image_understand"));
        assert!(normalized.text.contains("image_path"));
        assert!(!normalized.text.contains("ERROR: Cannot read"));
    }

    /// Downgrades unsupported video and audio input to their exact fixed host tools.
    /// 将不支持的视频与音频输入降级到各自精确固定的宿主工具。
    #[test]
    fn video_and_audio_understanding_bridges_keep_fixed_contracts() {
        let capabilities = ModelCapabilities::default();
        let bridge_tool_ids = vec![
            String::from("video_understand"),
            String::from("audio_understand"),
        ];
        let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            Some(&capabilities),
            "inspect",
            &[
                test_attachment(ConversationAttachmentKind::Video, "a.mp4", "video/mp4"),
                test_attachment(ConversationAttachmentKind::Audio, "a.mp3", "audio/mpeg"),
            ],
            &bridge_tool_ids,
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 2);
        assert!(normalized.text.contains("video_understand"));
        assert!(normalized.text.contains("video_path"));
        assert!(normalized.text.contains("audio_understand"));
        assert!(normalized.text.contains("audio_path"));
        assert!(!normalized.text.contains("selected protocol profile"));
    }

    /// Routes a model-declared but format-incompatible media attachment through the exact host bridge.
    /// 将模型已声明但格式不兼容的媒体附件路由到精确宿主桥接。
    #[test]
    fn format_incompatible_audio_uses_understanding_bridge() {
        // capabilities proves model support so only exact protocol MIME admission can select the bridge.
        // capabilities 证明模型支持该模态，因此只有精确协议 MIME 准入能选择桥接。
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Audio],
            ..ModelCapabilities::default()
        };
        // attachment uses Opus, which the OpenAI Chat input-audio projector does not register.
        // attachment 使用 OpenAI Chat input-audio 投影器未注册的 Opus。
        let attachment = test_attachment(
            ConversationAttachmentKind::Audio,
            "voice.opus",
            "audio/opus",
        );
        // bridgeToolIds exposes only the fixed audio-understanding facade.
        // bridgeToolIds 仅公开固定音频理解外壳。
        let bridge_tool_ids = vec![String::from("audio_understand")];

        assert!(!protocol_profile_supports_attachment(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            &attachment,
        ));
        let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            Some(&capabilities),
            "inspect",
            &[attachment],
            &bridge_tool_ids,
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("audio_understand"));
        assert!(normalized.text.contains("audio_path"));
    }

    /// Verifies the retired aggregate media tool cannot replace any fixed understanding facade.
    /// 验证已退役的聚合媒体工具不能替代任一固定理解外壳。
    #[test]
    fn legacy_aggregate_media_tool_is_not_an_understanding_bridge() {
        // capabilities deliberately lacks media input so only an explicitly matching bridge could handle it.
        // capabilities 有意不含媒体输入，因此只有显式匹配的桥接才能处理附件。
        let capabilities = ModelCapabilities::default();
        let normalized = normalize_provider_user_input_for_model_with_tool_bridges(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            Some(&capabilities),
            "inspect",
            &[test_attachment(
                ConversationAttachmentKind::Video,
                "a.mp4",
                "video/mp4",
            )],
            &[String::from("vulcan_media_analyze")],
        );

        assert!(normalized.attachments.is_empty());
        assert_eq!(normalized.downgraded_attachments.len(), 1);
        assert!(normalized.text.contains("selected model"));
        assert!(!normalized.text.contains("vulcan_media_analyze"));
        assert!(!normalized.text.contains("media_paths"));
    }

    /// Verifies Anthropic-family protocols require JSON Schema while OpenAI Chat accepts both JSON modes.
    /// 验证 Anthropic 系协议要求 JSON Schema，而 OpenAI Chat 接受两种 JSON 模式。
    #[test]
    fn structured_output_support_is_protocol_exact() {
        assert!(!protocol_profile_supports_structured_output(
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            StructuredOutputCapability::JsonObject,
        ));
        assert!(protocol_profile_supports_structured_output(
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            StructuredOutputCapability::JsonSchema,
        ));
        assert!(protocol_profile_supports_structured_output(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            StructuredOutputCapability::JsonObject,
        ));
        assert!(protocol_profile_supports_structured_output(
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            StructuredOutputCapability::JsonSchema,
        ));
    }

    /// Verifies registered Gemini media encoders participate in attachment admission.
    /// 验证已注册的 Gemini 媒体编码器参与附件准入。
    #[test]
    fn attachment_support_includes_registered_gemini_encoders() {
        for protocol_profile_id in [
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID,
            vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID,
        ] {
            for kind in [
                ConversationAttachmentKind::Image,
                ConversationAttachmentKind::Video,
                ConversationAttachmentKind::Audio,
            ] {
                assert!(protocol_profile_supports_attachment_kind(
                    protocol_profile_id,
                    kind,
                ));
            }
        }
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

    /// Documents always disclose their path; media disclose it only without an understanding or read_media route.
    /// 文档总是披露路径；媒体仅在没有理解工具或read_media路线时披露。
    #[test]
    fn stored_path_disclosure_follows_available_tool_routes() {
        use super::metadata_only_discloses_stored_path as discloses;
        let tools = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let image = ConversationAttachmentKind::Image;
        assert!(discloses(&ConversationAttachmentKind::Pdf, &tools(&["read_media"]), |_| true));
        assert!(discloses(&image, &tools(&[]), |_| true));
        assert!(!discloses(&image, &tools(&["image_understand"]), |_| false));
        assert!(!discloses(&image, &tools(&["read_media"]), |kind| kind == ResourceKind::Image));
        assert!(discloses(&image, &tools(&["read_media"]), |kind| kind != ResourceKind::Image));
    }
}

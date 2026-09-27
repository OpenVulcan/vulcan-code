//! Composer attachment conversion helpers.
//! 输入区附件转换辅助逻辑。

use super::*;
use vulcan_service_api::ComposerAttachmentDeliveryPlan;

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
pub(crate) fn composer_attachment_kinds() -> [ConversationAttachmentKind; 5] {
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
        ConversationAttachmentKind::File,
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
            accept: "image/png,image/jpeg,image/webp,image/gif,image/bmp,image/tiff,image/x-icon,.tif,.tiff,.ico,.pbm,.pgm,.ppm,.pnm,.pam,.tga,.qoi",
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
            accept: "*/*",
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
        prompt_source: None,
        kind: attachment.kind.modality_name().to_string(),
        file_name: attachment.file_name.clone(),
        local_path: attachment.local_path.clone(),
        project_resource_id: None,
        resource_path: attachment.resource_path.clone(),
        source_path: attachment.source_path.clone(),
        asset_id: attachment.asset_id.clone(),
        byte_len: attachment.byte_len,
        source_sha256: attachment.source_sha256.clone(),
        delivery_plan: attachment
            .delivery_mode
            .map(core_delivery_mode_to_service_plan),
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
        resource_path: summary.resource_path,
        source_path: summary.source_path,
        asset_id: summary.asset_id,
        byte_len: summary.byte_len,
        source_sha256: summary.source_sha256,
        delivery_mode: None,
        preview_local_path: summary.preview_local_path,
        mime_type: summary.mime_type,
    })
}

/// Rebuilds one browser attachment summary from the authoritative project-scoped asset index.
/// 从权威项目范围资产索引重建一条浏览器附件摘要。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the project asset store and `summary` contributes only the opaque asset id plus its transient delivery plan.
/// `workspace_path` 标识项目资产存储，`summary` 仅贡献不透明资产标识及其临时传输规划。
///
/// # Returns
/// 返回值
/// An integrity-verified canonical summary whose paths, names, MIME type, length, digest, and preview all come from host-owned state.
/// 返回一条通过完整性校验的规范摘要，其路径、名称、MIME 类型、长度、摘要与预览全部来自宿主持有状态。
pub(crate) fn resolve_composer_attachment_summary_for_workspace(
    workspace_path: &str,
    summary: ComposerAttachmentSummary,
) -> Result<ComposerAttachmentSummary, String> {
    if summary.project_resource_id.is_some() {
        return Err(String::from(
            "Project ResourceId attachments require the V1 session resource authority.",
        ));
    }
    if summary.prompt_source.is_some() {
        return Err(String::from(
            "A staged attachment cannot claim a project prompt source.",
        ));
    }
    let expected_kind = parse_composer_attachment_kind(&summary.kind)?;
    let asset_id = summary
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Composer attachment is missing its stable asset id."))?;
    let mut attachment = resolve_indexed_attachment_resource(
        workspace_path,
        format!("attachment://{asset_id}").as_str(),
        expected_kind,
    )?;
    attachment.preview_local_path = resolve_attachment_image_preview_sidecar(&attachment);
    let mut authoritative = composer_attachment_summary_from_core(&attachment);
    authoritative.delivery_plan = summary.delivery_plan;
    Ok(authoritative)
}

/// Converts a WebUI attachment DTO into one conservative request-local Provider preference.
/// 将 WebUI 附件 DTO 转换为单次请求局部的保守 Provider 偏好。
///
/// # Parameters
/// 参数
/// `summary` carries host-rebuilt canonical resource fields plus one browser-reported draft outcome.
/// `summary` 携带由宿主重建的规范资源字段以及一项浏览器报告的草稿结果。
///
/// # Returns
/// 返回值
/// A core attachment copy that retains only metadata-only as a safe reduction; every direct-delivery
/// mode is recomputed from the current authoritative route before Provider preparation.
/// 返回一份仅保留“仅元数据”安全降级的核心附件副本；每种直接传输模式都会在 Provider 准备前
/// 根据当前权威路线重新计算。
pub(crate) fn composer_attachment_summary_to_provider_core(
    summary: ComposerAttachmentSummary,
) -> Result<ConversationAttachment, String> {
    let delivery_mode = (summary.delivery_plan
        == Some(ComposerAttachmentDeliveryPlan::MetadataOnly))
    .then_some(vulcan_core::types::ConversationAttachmentDeliveryMode::MetadataOnly);
    let mut attachment = composer_attachment_summary_to_core(summary)?;
    attachment.delivery_mode = delivery_mode;
    Ok(attachment)
}

/// Converts one core request-local delivery mode into its browser service representation.
/// 将单个核心单次请求局部传输模式转换为浏览器服务表示。
fn core_delivery_mode_to_service_plan(
    mode: vulcan_core::types::ConversationAttachmentDeliveryMode,
) -> ComposerAttachmentDeliveryPlan {
    use vulcan_core::types::ConversationAttachmentDeliveryMode as CoreMode;
    match mode {
        CoreMode::InlineOriginal => ComposerAttachmentDeliveryPlan::InlineOriginal,
        CoreMode::UploadReference => ComposerAttachmentDeliveryPlan::UploadReference,
        CoreMode::InlineDerivedPreview => ComposerAttachmentDeliveryPlan::InlineDerivedPreview,
        CoreMode::MetadataOnly => ComposerAttachmentDeliveryPlan::MetadataOnly,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_agent_runtime::attachment_assets::stage_attachment_bytes;

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

    /// Verifies browser-controlled attachment fields are replaced by project-index authority before planning or submission.
    /// 验证在规划或提交前，浏览器可控附件字段会被项目索引权威状态替换。
    #[test]
    fn resolve_composer_attachment_summary_rebuilds_authoritative_fields() {
        let workspace = tempfile::tempdir().expect("temporary workspace should create");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &attachment_descriptor_from_mime_type("image/png")
                .expect("png descriptor should resolve"),
            "canonical.png",
            None,
            b"canonical immutable bytes",
        )
        .expect("canonical image should stage");
        let preview_path =
            write_attachment_image_preview_sidecar(&attachment, &[0xff_u8, 0xd8, 0xff, 0xd9])
                .expect("canonical preview should stage");
        let forged = ComposerAttachmentSummary {
            prompt_source: None,
            kind: String::from("image"),
            file_name: String::from("forged.jpg"),
            local_path: String::from("C:/forged/source.jpg"),
            project_resource_id: None,
            resource_path: Some(String::from("../forged")),
            source_path: Some(String::from("C:/forged/original.jpg")),
            asset_id: attachment.asset_id.clone(),
            byte_len: Some(999),
            source_sha256: Some(String::from("forged")),
            delivery_plan: Some(ComposerAttachmentDeliveryPlan::InlineDerivedPreview),
            preview_local_path: Some(String::from("C:/forged/preview.jpg")),
            mime_type: String::from("image/jpeg"),
        };

        let resolved =
            resolve_composer_attachment_summary_for_workspace(workspace_path.as_ref(), forged)
                .expect("indexed attachment should resolve");

        assert_eq!(resolved.file_name, attachment.file_name);
        assert_eq!(resolved.local_path, attachment.local_path);
        assert_eq!(resolved.resource_path, attachment.resource_path);
        assert_eq!(resolved.byte_len, attachment.byte_len);
        assert_eq!(resolved.source_sha256, attachment.source_sha256);
        assert_eq!(resolved.mime_type, attachment.mime_type);
        assert_eq!(
            resolved.preview_local_path.as_deref(),
            Some(preview_path.as_str())
        );
        assert_eq!(
            resolved.delivery_plan,
            Some(ComposerAttachmentDeliveryPlan::InlineDerivedPreview)
        );
    }

    /// Verifies browser-reported direct modes cannot choose Provider-visible bytes or paths.
    /// 验证浏览器报告的直接模式无法选择 Provider 可见字节或路径。
    #[test]
    fn provider_conversion_discards_browser_direct_delivery_modes() {
        let summary = ComposerAttachmentSummary {
            prompt_source: None,
            kind: String::from("image"),
            file_name: String::from("canonical.png"),
            local_path: String::from("D:/workspace/.vulcan/attachments/canonical.png"),
            project_resource_id: None,
            resource_path: Some(String::from(".vulcan/attachments/canonical.png")),
            source_path: None,
            asset_id: Some(String::from("asset-image")),
            byte_len: Some(128),
            source_sha256: Some(String::from("sha256-source")),
            delivery_plan: Some(ComposerAttachmentDeliveryPlan::InlineDerivedPreview),
            preview_local_path: Some(String::from(
                "D:/workspace/.vulcan/attachments/canonical.preview.jpg",
            )),
            mime_type: String::from("image/png"),
        };

        let attachment = composer_attachment_summary_to_provider_core(summary)
            .expect("direct browser plan should convert conservatively");

        assert_eq!(attachment.delivery_mode, None);
        assert_eq!(
            attachment.local_path,
            "D:/workspace/.vulcan/attachments/canonical.png"
        );
        assert_eq!(attachment.mime_type, "image/png");
    }

    /// Verifies a browser may explicitly reduce one failed preparation to metadata only.
    /// 验证浏览器可以把一次失败准备显式降级为仅元数据。
    #[test]
    fn provider_conversion_retains_metadata_only_reduction() {
        let summary = ComposerAttachmentSummary {
            prompt_source: None,
            kind: String::from("video"),
            file_name: String::from("clip.mp4"),
            local_path: String::from("D:/workspace/.vulcan/attachments/clip.mp4"),
            project_resource_id: None,
            resource_path: Some(String::from(".vulcan/attachments/clip.mp4")),
            source_path: None,
            asset_id: Some(String::from("asset-video")),
            byte_len: Some(1_024),
            source_sha256: Some(String::from("sha256-video")),
            delivery_plan: Some(ComposerAttachmentDeliveryPlan::MetadataOnly),
            preview_local_path: None,
            mime_type: String::from("video/mp4"),
        };

        let attachment = composer_attachment_summary_to_provider_core(summary)
            .expect("metadata-only browser reduction should convert");

        assert_eq!(
            attachment.delivery_mode,
            Some(vulcan_core::types::ConversationAttachmentDeliveryMode::MetadataOnly)
        );
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
            vec!["image", "video", "audio", "pdf", "file"]
        );
    }
}

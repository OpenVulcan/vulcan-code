//! Provisional metadata and shared replacement-policy regressions.
//! 临时元数据及共享替代策略回归。

use super::*;
use vulcan_ai_contract::{ResourceKind, ResourceSourceKind};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    MediaDeliveryDecision, MediaDeliveryFacts, MediaDeliveryOrigin, MediaDeliveryPlanner,
    MediaDeliveryPolicy, MediaDeliveryPurpose, MediaDeliveryRejection, MediaUploadAvailability,
};

/// Supplies explicit display facts and private fields that must never reach provisional model text.
/// 提供显式显示事实及绝不能进入临时模型文本的私有字段。
fn attachment(name: &str) -> ConversationAttachment {
    ConversationAttachment {
        kind: ConversationAttachmentKind::Image,
        file_name: name.into(),
        mime_type: "image/png".into(),
        local_path: "C:/private/not-readable.png".into(),
        source_path: Some("C:/private/original.png".into()),
        preview_local_path: Some("C:/private/preview.png".into()),
        asset_id: Some("asset-1".into()),
        resource_path: Some(".vulcan/code/attachments/image.png".into()),
        byte_len: Some(3),
        source_sha256: Some("private-digest".into()),
        delivery_mode: None,
    }
}

/// Extracts only model-facing text from the production provisional projector for the given inputs.
/// 从生产临时投影器中为所给输入提取仅模型可见文本。
fn projected_text(attachments: &[ConversationAttachment]) -> String {
    serde_json::to_string(&unprepared_user_message("describe", attachments))
        .expect("public message")
}

/// Supplies exact verified inline facts to the shared production planner for `raw_bytes`.
/// 为`raw_bytes`向共享生产规划器提供精确已验证内联事实。
fn inline_facts(raw_bytes: u64) -> MediaDeliveryFacts {
    MediaDeliveryFacts {
        kind: ResourceKind::Image,
        raw_bytes,
        source_available: true,
        target_supported: true,
        target_max_bytes: None,
        inline_supported: true,
        upload: MediaUploadAvailability::Absent,
        origin: MediaDeliveryOrigin::NewIntent,
    }
}

/// All modalities remain admitted as attachments without guessing native delivery from display facts.
/// 全部模态仍以附件准入，不从显示事实猜测原生投递。
#[test]
fn provisional_resources_do_not_read_files_or_guess_original_delivery() {
    for (kind, mime) in [
        (ConversationAttachmentKind::Image, "image/png"),
        (ConversationAttachmentKind::Audio, "audio/wav"),
        (ConversationAttachmentKind::Video, "video/mp4"),
        (ConversationAttachmentKind::Pdf, "application/pdf"),
        (ConversationAttachmentKind::File, "application/octet-stream"),
    ] {
        // The nonexistent private path must never be opened by this pure projector.
        // 本纯投影器绝不能打开不存在的私有路径。
        let mut source = attachment("original");
        source.kind = kind;
        source.mime_type = mime.into();
        source.byte_len = Some(u64::MAX);
        // Large files remain attached even when native delivery has not yet been prepared.
        // 即使原生投递尚未准备，大文件仍保留附件。
        let projected = unprepared_user_message("describe", &[source]);
        assert_eq!(projected.content.len(), 2);
        assert!(
            projected
                .content
                .iter()
                .all(|item| matches!(item, ContentItem::Text { .. }))
        );
        assert!(
            serde_json::to_string(&projected)
                .unwrap()
                .contains("resource bytes were not provided")
        );
    }
}

/// JSON escaping preserves untrusted filenames as inert data and never reveals local paths or digests.
/// JSON转义将不可信文件名保持为纯数据，绝不泄露本地路径或摘要。
#[test]
fn resource_projection_fallback_uses_the_safe_resource_path() {
    // Names may contain quoting or newline characters, but cannot inject a metadata instruction.
    // 名称可能含引号或换行，但不能注入元数据指令。
    let source = attachment("screen\"\nINJECTED=1.png");
    let projected = unprepared_user_message("describe", &[source]);
    let ContentItem::Text { text } = &projected.content[1] else {
        panic!("metadata");
    };
    assert!(!text.contains("\nINJECTED=1"));
    assert!(text.contains(".vulcan/code/attachments/image.png"));
    assert!(!text.contains("C:/private"));
    assert!(!text.contains("private-digest"));
}

/// Private upstream URLs and traversal strings cannot become displayed resource locators.
/// 私有上游URL和穿越字符串不能变成显示资源定位符。
#[test]
fn private_or_unsafe_locators_are_not_disclosed() {
    for locator in [
        "https://provider.invalid/secret-upload",
        "data:image/png;base64,SECRET",
        "C:/private/file",
        "/private/file",
        "../private/file",
        "project-resource://01",
    ] {
        // No staged identity is supplied, so invalid source metadata has no alternate disclosed owner.
        // 不提供暂存身份，因此无效来源元数据没有另一披露所有者。
        let mut source = attachment("image.png");
        source.resource_path = Some(locator.into());
        source.asset_id = None;
        assert_eq!(safe_attachment_locator(&source), "attachment://unavailable");
        assert!(!projected_text(&[source]).contains(locator));
    }
}

/// Explicit staged IDs remain usable when old metadata has no project-relative path.
/// 当旧元数据没有项目相对路径时，显式暂存ID仍可使用。
#[test]
fn missing_resource_path_uses_only_the_exact_staged_identity() {
    // Original source paths cannot substitute for a missing disclosed address.
    // 原来源路径不能替代缺失的披露地址。
    let mut source = attachment("image.png");
    source.resource_path = None;
    assert_eq!(safe_attachment_locator(&source), "attachment://asset-1");
    source.asset_id = None;
    assert_eq!(safe_attachment_locator(&source), "attachment://unavailable");
}

/// Canonical project-resource IDs are retained without opening the physical entity.
/// 保留规范项目资源ID，不打开物理实体。
#[test]
fn project_locator_is_retained_without_staged_identity() {
    // A project reference is independent of staging lifetime and private entity paths.
    // 项目引用独立于暂存生命周期及私有实体路径。
    let mut source = attachment("document.pdf");
    source.resource_path = Some("project-resource://7".into());
    source.asset_id = None;
    assert_eq!(safe_attachment_locator(&source), "project-resource://7");
}

/// Native reference encoding is owned by the shared prepared-media contract, not the removed last-cache projector.
/// 原生引用编码由共享已准备媒体契约持有，而非已移除的最近缓存投影器。
#[test]
fn prepared_reference_encoding_uses_shared_source_contract() {
    assert_eq!(
        vulcan_provider_adapters::media_upload_source_kind(
            vulcan_provider_adapters::UploadReferenceEncoding::ProviderFileId
        ),
        ResourceSourceKind::ProviderFileId
    );
    assert_eq!(
        vulcan_provider_adapters::media_upload_source_kind(
            vulcan_provider_adapters::UploadReferenceEncoding::ResponseUrl
        ),
        ResourceSourceKind::RemoteUrl
    );
}

/// Exact Base64 expansion comes from the shared planner, including overflow rejection.
/// 精确Base64膨胀来自共享规划器，包括溢出拒绝。
#[test]
fn inline_resource_budget_accounts_for_exact_base64_expansion() {
    assert_eq!(
        vulcan_provider_adapters::media_base64_encoded_len(1),
        Some(4)
    );
    assert_eq!(
        vulcan_provider_adapters::media_base64_encoded_len(4),
        Some(8)
    );
    assert_eq!(
        vulcan_provider_adapters::media_base64_encoded_len(u64::MAX),
        None
    );
}

/// The configured aggregate budget replaces the old hard-coded prebuilder budget.
/// 可配置累计预算替代旧预构建器硬编码预算。
#[test]
fn inline_resource_budget_uses_the_shared_frozen_policy() {
    // A deliberately tiny valid budget makes exact reservation observable without allocating media.
    // 故意使用微小有效预算，使精确预留可观察而无需分配媒体。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.max_total_encoded_bytes = 8;
    let mut planner =
        MediaDeliveryPlanner::new(policy, MediaDeliveryPurpose::Conversation).unwrap();
    assert_eq!(
        planner.plan(inline_facts(4)).unwrap(),
        MediaDeliveryDecision::Inline { encoded_bytes: 8 }
    );
    assert_eq!(
        planner.plan(inline_facts(1)).unwrap(),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::EncodedBudget
        }
    );
    assert_eq!(planner.encoded_bytes_reserved(), 8);
}

/// Repeated resources retain chronological occurrences rather than a request-global count shortcut.
/// 重复资源保留时序出现，不使用请求级数量捷径。
#[test]
fn provisional_resource_order_keeps_duplicate_occurrences() {
    // Equal asset IDs do not collapse distinct positions or display names.
    // 相同资产ID不会折叠不同位置或显示名称。
    let projected = unprepared_user_message(
        "first text",
        &[
            attachment("first"),
            attachment("second"),
            attachment("first"),
        ],
    );
    assert_eq!(projected.content.len(), 4);
    for (index, name) in [(1, "first"), (2, "second"), (3, "first")] {
        let ContentItem::Text { text } = &projected.content[index] else {
            panic!("metadata");
        };
        assert!(text.contains(name));
    }
}

/// Generic files use the actual registered source-form matrix instead of a blanket rejection.
/// 普通文件使用真实已注册来源形态矩阵，而不是一律拒绝。
#[test]
fn protocol_resource_kind_gate_uses_registered_file_support() {
    assert!(vulcan_llm_core::protocol_profile_supports_media_source(
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
        ResourceKind::File,
        "application/pdf",
        ResourceSourceKind::InlineData
    ));
    assert!(!vulcan_llm_core::protocol_profile_supports_media_source(
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
        ResourceKind::Video,
        "video/mp4",
        ResourceSourceKind::InlineData
    ));
    assert!(!vulcan_llm_core::protocol_profile_supports_media_source(
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
        ResourceKind::File,
        "application/pdf",
        ResourceSourceKind::HostResolved
    ));
}

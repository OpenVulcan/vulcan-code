//! Conversation policy boundaries and migrated native-count/Base64-budget regressions.
//! 主会话策略边界及迁移后的原生数量和Base64预算回归。

use super::*;
use vulcan_provider_adapters::{
    ConversationMediaUploadContract, UploadReferenceEncoding, UploadReferenceScope,
};

/// Builds one all-kind exact target with optional `upload`, without any source or network access.
/// 构建一个带可选`upload`的全类型精确目标，不访问来源或网络。
fn target(upload: Option<&ConversationMediaUploadContract>) -> MediaDeliveryTarget<'_> {
    MediaDeliveryTarget {
        profile_id: vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
        model_inputs: &[
            ResourceKind::Image,
            ResourceKind::Audio,
            ResourceKind::Video,
            ResourceKind::File,
        ],
        constraint: None,
        upload,
    }
}

/// Builds one real URL-returning upload with a fixed hard size and native-count bound.
/// 构建一个返回URL、固定硬大小及原生数量边界的真实上传。
fn upload() -> ConversationMediaUploadContract {
    ConversationMediaUploadContract {
        timeout_ms: 60_000,
        upload_channel_id: "files".into(),
        contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
        reference_scope: UploadReferenceScope::Session,
        cache_duration_ms: Some(60_000),
        reference_encoding: UploadReferenceEncoding::ResponseUrl,
        max_bytes: Some(12 * 1024 * 1024),
        max_count: Some(1),
    }
}

/// Verifies all four modalities share the main default boundary without imposing the independent extension's threshold.
/// 验证四种模态使用主会话默认边界，不混入独立扩展阈值。
#[test]
fn composer_policy_uses_four_inclusive_conversation_thresholds() -> Result<(), String> {
    for (kind, mime) in [
        (ResourceKind::Image, "image/png"),
        (ResourceKind::Audio, "audio/wav"),
        (ResourceKind::Video, "video/mp4"),
        (ResourceKind::File, "application/pdf"),
    ] {
        // Each case owns a fresh batch so aggregate state cannot obscure its exact raw boundary.
        // 每项拥有新批次，避免累计状态掩盖精确原始字节边界。
        let mut planner = ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?;
        assert_eq!(
            planner
                .plan_target(&target(None), kind, mime, 512 * 1024)?
                .0,
            ComposerAttachmentDeliveryPlan::InlineOriginal
        );
        assert_eq!(
            planner
                .plan_target(&target(None), kind, mime, 512 * 1024 + 1)?
                .0,
            ComposerAttachmentDeliveryPlan::MetadataOnly
        );
    }
    Ok(())
}

/// Verifies configurable small inline preference and interface hard limits are both applied to original bytes.
/// 验证可配置小文件内联偏好和接口硬限制均作用于原始字节。
#[test]
fn composer_policy_prefers_upload_unless_configured_and_never_bypasses_hard_size()
-> Result<(), String> {
    // One-byte video uploads by default because a real upload exists, not merely because it is video.
    // 单字节视频默认上传是因为存在真实上传，而非仅因为它是视频。
    let mut upload = upload();
    assert_eq!(
        ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?
            .plan_target(&target(Some(&upload)), ResourceKind::Video, "video/mp4", 1)?
            .0,
        ComposerAttachmentDeliveryPlan::UploadReference
    );
    // Positive preference applies only to the selected purpose and media kind.
    // 正数偏好仅作用于选中用途和媒体类型。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.video.with_upload_max_bytes = 8;
    assert_eq!(
        ComposerMediaDeliveryPlanner::new(policy)?
            .plan_target(&target(Some(&upload)), ResourceKind::Video, "video/mp4", 8)?
            .0,
        ComposerAttachmentDeliveryPlan::InlineOriginal
    );
    assert_eq!(
        ComposerMediaDeliveryPlanner::new(policy)?
            .plan_target(&target(Some(&upload)), ResourceKind::Video, "video/mp4", 9)?
            .0,
        ComposerAttachmentDeliveryPlan::UploadReference
    );
    upload.max_bytes = Some(3);
    assert_eq!(
        ComposerMediaDeliveryPlanner::new(policy)?
            .plan_target(&target(Some(&upload)), ResourceKind::Video, "video/mp4", 4)?
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    Ok(())
}

/// Verifies native-count rejection preserves the already admitted total and reserves no encoded bytes.
/// 验证原生数量拒绝保留已准入总数，并且不预留编码字节。
#[test]
fn composer_direct_resource_count_limit_preserves_admitted_total() -> Result<(), String> {
    // The model permits one image per message; each physical upload independently contains one file.
    // 模型每条消息允许一张图片；每次物理上传独立包含一个文件。
    let upload = upload();
    // The model's per-message resource constraint is separate from per-upload limits.
    // 模型逐消息资源约束独立于逐上传限制。
    let constraint = vulcan_provider_adapters::InputResourceConstraint {
        resource_kind: ResourceKind::Image,
        accepted_mime_types: vec!["image/png".into()],
        max_bytes: None,
        max_count: Some(1),
        reference_policy: vulcan_provider_adapters::ResourceReferencePolicy::Any,
        upload_channel_refs: vec!["files".into()],
    };
    // The exact protocol target is independent of the selected policy values.
    // 精确协议目标独立于选中策略值。
    let mut target = target(Some(&upload));
    target.constraint = Some(&constraint);
    // A fresh request planner isolates native counts and encoded reservations.
    // 新请求规划器隔离原生数量及编码预留。
    let mut planner = ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?;
    assert_eq!(
        planner
            .plan_target(&target, ResourceKind::Image, "image/png", 1)?
            .0,
        ComposerAttachmentDeliveryPlan::UploadReference
    );
    assert_eq!(
        planner
            .plan_target(&target, ResourceKind::Image, "image/png", 1)?
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    assert_eq!(planner.counts.get(&ResourceKind::Image), Some(&1));
    assert_eq!(planner.planner.encoded_bytes_reserved(), 0);
    Ok(())
}

/// Verifies single-file upload limits do not prohibit two separate uploads admitted in one model message.
/// 验证单文件上传限制不会禁止同一模型消息中已准入的两次独立上传。
#[test]
fn composer_upload_count_is_per_physical_upload_not_per_message() -> Result<(), String> {
    // There is no model count restriction, while the selected channel accepts one file per call.
    // 模型没有数量限制，而选中通道每次调用接受一个文件。
    let upload = upload();
    // A fresh request planner isolates native counts and encoded reservations.
    // 新请求规划器隔离原生数量及编码预留。
    let mut planner = ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?;
    for _ in 0..2 {
        assert_eq!(
            planner
                .plan_target(&target(Some(&upload)), ResourceKind::Image, "image/png", 1)?
                .0,
            ComposerAttachmentDeliveryPlan::UploadReference
        );
    }
    assert_eq!(planner.counts.get(&ResourceKind::Image), Some(&2));
    Ok(())
}

/// Verifies exact padded Base64 accounting admits the configured boundary and rejects the next byte without consuming a slot.
/// 验证精确带填充Base64计数准入配置边界，并拒绝后续一个字节而不消耗名额。
#[test]
fn composer_inline_aggregate_budget_uses_exact_base64_length() -> Result<(), String> {
    // This explicit threshold isolates the same two-MiB aggregate boundary covered by the old test.
    // 此显式阈值隔离旧测试覆盖的同一个二MiB累计边界。
    let mut policy = MediaDeliveryPolicy::default();
    // The frozen policy owns the aggregate encoded-byte boundary.
    // 冻结策略持有累计编码字节边界。
    let budget = policy.conversation.max_total_encoded_bytes;
    // The raw boundary expands exactly to the selected padded Base64 budget.
    // 原始边界精确膨胀为选中带填充Base64预算。
    let raw_boundary = budget / 4 * 3;
    policy.conversation.image.without_upload_max_bytes = raw_boundary;
    // A fresh request planner isolates native counts and encoded reservations.
    // 新请求规划器隔离原生数量及编码预留。
    let mut planner = ComposerMediaDeliveryPlanner::new(policy)?;
    assert_eq!(
        planner
            .plan_target(
                &target(None),
                ResourceKind::Image,
                "image/png",
                raw_boundary
            )?
            .0,
        ComposerAttachmentDeliveryPlan::InlineOriginal
    );
    assert_eq!(planner.planner.encoded_bytes_reserved(), budget);
    assert_eq!(
        planner
            .plan_target(&target(None), ResourceKind::File, "application/pdf", 1)?
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    assert_eq!(planner.counts.get(&ResourceKind::Image), Some(&1));
    assert!(!planner.counts.contains_key(&ResourceKind::File));
    assert_eq!(planner.planner.encoded_bytes_reserved(), budget);
    Ok(())
}

/// Keeps media unsupported by the exact target as an attachment instead of disguising it as another modality.
/// 将精确目标不支持的媒体保留为附件，不伪装为其他模态。
#[test]
fn composer_policy_rejects_unsupported_wire_source_without_relabeling() -> Result<(), String> {
    // Standard Responses cannot acquire video support just because a local upload contract exists.
    // 标准Responses不能仅因存在本地上传契约就获得视频支持。
    let upload = upload();
    // The exact protocol target is independent of the selected policy values.
    // 精确协议目标独立于选中策略值。
    let mut target = target(Some(&upload));
    target.profile_id = vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID;
    // A fresh request planner isolates native counts and encoded reservations.
    // 新请求规划器隔离原生数量及编码预留。
    let mut planner = ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?;
    assert_eq!(
        planner
            .plan_target(&target, ResourceKind::Video, "video/mp4", 1)?
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    assert!(planner.counts.is_empty());
    assert_eq!(planner.planner.encoded_bytes_reserved(), 0);
    Ok(())
}

/// Preserves the former Opus regression at the real shared composer admission boundary.
/// 在真正共用的输入区准入边界保留原Opus回归。
#[test]
fn understanding_bridge_preserves_format_incompatible_audio_as_local() -> Result<(), String> {
    // Model-level audio support alone cannot make the exact Chat encoder accept Opus inline.
    // 仅模型级音频支持不能让精确Chat编码器接受Opus内联。
    let mut target = target(None);
    target.profile_id = vulcan_llm_core::OPENAI_CHAT_PROFILE_ID;
    // A fresh request planner isolates native counts and encoded reservations.
    // 新请求规划器隔离原生数量及编码预留。
    let mut planner = ComposerMediaDeliveryPlanner::new(MediaDeliveryPolicy::default())?;
    assert_eq!(
        planner
            .plan_target(&target, ResourceKind::Audio, "audio/opus", 8)?
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    assert!(planner.counts.is_empty());
    assert_eq!(planner.planner.encoded_bytes_reserved(), 0);
    Ok(())
}

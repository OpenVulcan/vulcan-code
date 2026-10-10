//! Real asset-index draft provenance and lease lifetime regressions.
//! 真实资产索引草稿来源及租约生命周期回归。

use super::*;
use crate::attachment_assets::{generic_attachment_descriptor, stage_attachment_bytes};
use vulcan_ai_contract::{ResourceKind, ResourceSourceKind};
use vulcan_workspace_store::workspace_store::ProjectResourceUploadReceipt;

mod handoff;
mod source_hold;

/// Rejects altered host proofs before persistence and leaves the original pending upload unchanged.
/// 持久化前拒绝被改动宿主证明，并保持原待上传状态不变。
#[test]
fn draft_media_validates_complete_handoff_proof_before_any_write()
-> Result<(), Box<dyn std::error::Error>> {
    // Real index and source facts remain isolated for every rejection case.
    // 真实索引及来源事实在每项拒绝用例中保持隔离。
    let (_workspace, attachment) = fixture("composer-draft:v1:new:validate")?;
    // The authoritative pending intention is the immutable before-image.
    // 权威待处理意图是不可变前镜像。
    let original = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:validate",
        upload_origin(&attachment)?,
    )?;
    for variant in 0..5 {
        // Alter exactly one typed receipt fact, never the canonical source.
        // 每次只改动一项强类型回执事实，绝不改动规范来源。
        let mut changed = receipt(&attachment, "private-reference-must-not-leak");
        match variant {
            0 => changed.provider = "another-provider".into(),
            1 => changed.connection_id = "another-connection".into(),
            2 => changed.upload_receipt.service_entry_id = "another-entry".into(),
            3 => changed.upload_receipt.upload_channel_id = "another-channel".into(),
            _ => changed.upload_receipt.resource_kind = ResourceKind::Image,
        }
        // A failed receipt must not become the original upload nor leak its opaque provider value.
        // 失败回执不能成为原上传，也不能泄漏其不透明供应商值。
        let error = record_attachment_draft_upload(
            &attachment,
            "composer-draft:v1:new:validate",
            &original.intent_id,
            changed,
        )
        .expect_err("invalid proof");
        assert!(!error.contains("private-reference-must-not-leak"));
        assert_eq!(
            load_attachment_draft_media(&attachment, "composer-draft:v1:new:validate")?,
            Some(original.clone())
        );
    }
    // The public handoff validator also rejects a forged source while the source owner remains unchanged.
    // 公开交接校验器也拒绝伪造来源，而来源所有者保持不变。
    let mut invalid = original;
    invalid.source_sha256 = "b".repeat(64);
    assert!(invalid.validate_attachment(&attachment).is_err());
    // Even equal caller-supplied malformed digests are not valid identities.
    // 即使调用方提供相同的错误格式摘要，也不能成为有效身份。
    let mut malformed = attachment;
    malformed.source_sha256 = Some("not-a-sha256".into());
    assert!(
        AttachmentDraftMediaOrigin::new(
            &malformed,
            route("A"),
            AttachmentDraftMediaDelivery::Inline
        )
        .is_err()
    );
    Ok(())
}

/// Creates one indexed managed fixture and live exact draft `scope`, returning its temporary workspace and attachment.
/// 创建一个已索引托管夹具及精确活动草稿`scope`，返回临时工作区及附件。
fn fixture(scope: &str) -> Result<(tempfile::TempDir, ConversationAttachment), String> {
    // All persistence is isolated from user projects and configuration.
    // 全部持久化均与用户项目及配置隔离。
    let workspace = tempfile::tempdir().map_err(|error| error.to_string())?;
    // Staging produces the real unique asset identity and authoritative digest.
    // 暂存产出真实唯一资产身份及权威摘要。
    let attachment = stage_attachment_bytes(
        &workspace.path().to_string_lossy(),
        &generic_attachment_descriptor(),
        "source.bin",
        None,
        b"abc",
    )?;
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    Ok((workspace, attachment))
}

/// Builds a non-secret exact route for fixture `model`, keeping provider, entry and connection deliberately shared.
/// 为夹具`model`构建非秘密精确路线，故意共享供应商、入口及连接。
fn route(model: &str) -> UserPromptMediaRoute {
    UserPromptMediaRoute {
        provider_id: "provider".into(),
        service_entry_id: "entry".into(),
        connection_id: "connection".into(),
        model_id: model.into(),
        protocol_profile_id: "openai-responses.v1".into(),
    }
}

/// Builds a new upload intention for the original model A and one exact immutable contract.
/// 为原模型A及一个精确不可变契约构建新上传意图。
fn upload_origin(
    attachment: &ConversationAttachment,
) -> Result<AttachmentDraftMediaOrigin, String> {
    AttachmentDraftMediaOrigin::new(
        attachment,
        route("A"),
        AttachmentDraftMediaDelivery::Upload {
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            receipt: None,
        },
    )
}

/// Builds one private fixture receipt identified by `reference`, retaining the exact original source MIME and route.
/// 构建由`reference`标识的私有夹具回执，保留精确原来源MIME及路线。
fn receipt(attachment: &ConversationAttachment, reference: &str) -> ProviderBinding {
    ProviderBinding {
        provider: "provider".into(),
        connection_id: "connection".into(),
        media_id: reference.into(),
        upload_receipt: ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: "entry".into(),
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            resource_kind: ResourceKind::File,
            mime_type: attachment.mime_type.clone(),
            source_kind: ResourceSourceKind::ProviderFileId,
            session_scope_id: None,
            created_at_unix_millis: 1,
            expires_at_unix_millis: 1000,
        },
    }
}

/// Preserves the first model and mode across later model plans while keeping separate draft leases independent.
/// 跨后续模型规划保留首次模型及方式，同时保持不同草稿租约独立。
#[test]
fn draft_media_original_is_per_live_lease_not_latest_model()
-> Result<(), Box<dyn std::error::Error>> {
    // The same asset can be retained by two explicit composer leases.
    // 同一资产可由两个明确输入区租约保留。
    let (workspace, attachment) = fixture("composer-draft:v1:new:first")?;
    // Model A selected inline before model B's later planning call.
    // 模型A在模型B后续规划调用之前选择内联。
    let first = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:first",
        AttachmentDraftMediaOrigin::new(
            &attachment,
            route("A"),
            AttachmentDraftMediaDelivery::Inline,
        )?,
    )?;
    assert_eq!(
        record_attachment_draft_media(
            &attachment,
            "composer-draft:v1:new:first",
            AttachmentDraftMediaOrigin::new(
                &attachment,
                route("B"),
                AttachmentDraftMediaDelivery::Inline
            )?
        )?,
        first
    );
    assert_eq!(
        load_attachment_draft_media(&attachment, "composer-draft:v1:new:first")?,
        Some(first.clone())
    );
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        "composer-draft:v1:new:second",
        std::slice::from_ref(&attachment),
    )?;
    // An independent lease is a new explicit attachment intention, not a history rewrite.
    // 独立租约是新的显式附件意图，而非历史改写。
    let second = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:second",
        AttachmentDraftMediaOrigin::new(
            &attachment,
            route("B"),
            AttachmentDraftMediaDelivery::Inline,
        )?,
    )?;
    assert_ne!(first.intent_id, second.intent_id);
    assert_eq!(first.original_route.model_id, "A");
    assert_eq!(second.original_route.model_id, "B");
    assert!(load_attachment_draft_media(&attachment, "composer-draft:v1:new:unknown").is_err());
    Ok(())
}

/// Fills an upload receipt once and hands only its opaque identity into the prompt; replacement fails atomically.
/// 只填充一次上传回执，并只将其不透明身份交入提示词；替换原子失败。
#[test]
fn draft_media_original_upload_receipt_is_write_once() -> Result<(), Box<dyn std::error::Error>> {
    // Keep the real temporary workspace alive for all index reloads.
    // 在全部索引重载期间保留真实临时工作区。
    let (_workspace, attachment) = fixture("composer-draft:v1:new:upload")?;
    // No private receipt exists before the first upload completion.
    // 首次上传完成之前不存在私有回执。
    let pending = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:upload",
        upload_origin(&attachment)?,
    )?;
    // Handoff keeps the exact unfinished upload choice instead of converting it into permanent address-only intent.
    // 交接保留精确未完成上传选择，而非转换为永久仅地址意图。
    let pending_prompt = pending.prompt_delivery(ResourceId::try_new(7)?)?;
    assert_eq!(pending_prompt.original_route, pending.original_route);
    // The selected contract comes from this actual indexed intention, not a guessed fixture channel name.
    // 所选契约来自本实际索引意图，而非猜测夹具通道名。
    let AttachmentDraftMediaDelivery::Upload {
        upload_channel_id,
        upload_contract_fingerprint,
        ..
    } = &pending.delivery
    else {
        panic!("upload intent")
    };
    assert_eq!(
        pending_prompt.delivery,
        UserPromptResourceDeliveryKind::UploadPending {
            draft_intent_id: Some(pending.intent_id.clone().into_boxed_str()),
            upload_channel_id: upload_channel_id.clone(),
            upload_contract_fingerprint: upload_contract_fingerprint.clone()
        }
    );
    // Store the exact first reference, then reject a newer upload for the same bytes.
    // 存储精确首次引用，然后拒绝相同字节的较新上传。
    let completed = record_attachment_draft_upload(
        &attachment,
        "composer-draft:v1:new:upload",
        &pending.intent_id,
        receipt(&attachment, "file-original"),
    )?;
    assert!(
        record_attachment_draft_upload(
            &attachment,
            "composer-draft:v1:new:upload",
            &pending.intent_id,
            receipt(&attachment, "file-later")
        )
        .is_err()
    );
    assert_eq!(
        load_attachment_draft_media(&attachment, "composer-draft:v1:new:upload")?,
        Some(completed.clone())
    );
    // The resource occurrence remains model A and contains no raw provider reference.
    // 资源出现仍属于模型A，且不包含原始供应商引用。
    let prompt = completed.prompt_delivery(ResourceId::try_new(7)?)?;
    assert_eq!(prompt.original_route.model_id, "A");
    assert!(matches!(
        prompt.delivery,
        UserPromptResourceDeliveryKind::Uploaded { .. }
    ));
    assert!(!serde_json::to_string(&prompt)?.contains("file-original"));
    Ok(())
}

/// Rejects late completion after the same asset is removed and re-added under a fresh lease lifetime.
/// 拒绝同一资产移除后在新租约生命周期重新添加时的迟到完成。
#[test]
fn draft_media_remove_and_readd_invalidates_old_completion()
-> Result<(), Box<dyn std::error::Error>> {
    // A message reference keeps the entity alive while its draft lease is released.
    // 消息引用在草稿租约释放期间保留实体。
    let (workspace, attachment) = fixture("composer-draft:v1:new:reused")?;
    promote_staged_attachment_to_message_ref(
        &workspace.path().to_string_lossy(),
        "session",
        "message",
        std::slice::from_ref(&attachment),
        &[],
    )?;
    // Capture the old lifetime identity before releasing its exact scope.
    // 释放精确作用域前捕获旧生命周期身份。
    let old = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:reused",
        upload_origin(&attachment)?,
    )?;
    release_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        "composer-draft:v1:new:reused",
        std::slice::from_ref(&attachment),
    )?;
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        "composer-draft:v1:new:reused",
        std::slice::from_ref(&attachment),
    )?;
    assert!(load_attachment_draft_media(&attachment, "composer-draft:v1:new:reused")?.is_none());
    // A new intention must not accept the old transfer's eventual receipt.
    // 新意图不能接受旧传输最终返回的回执。
    let new = record_attachment_draft_media(
        &attachment,
        "composer-draft:v1:new:reused",
        upload_origin(&attachment)?,
    )?;
    assert_ne!(old.intent_id, new.intent_id);
    assert!(
        record_attachment_draft_upload(
            &attachment,
            "composer-draft:v1:new:reused",
            &old.intent_id,
            receipt(&attachment, "file-late")
        )
        .is_err()
    );
    assert_eq!(
        load_attachment_draft_media(&attachment, "composer-draft:v1:new:reused")?,
        Some(new)
    );
    Ok(())
}

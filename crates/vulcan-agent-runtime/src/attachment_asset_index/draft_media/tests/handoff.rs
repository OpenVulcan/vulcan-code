//! Submission-order and remove/re-add protection for actual asset-index upload handoffs.
//! 实际资产索引上传交接的提交顺序及移除重加保护。

use super::*;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceCatalogRegistry, load_project_resource_catalog, project_resource_catalog_path,
};

/// Keeps the same receipt for completion before registration, before promotion, or after promotion, including durable reload and exact retry.
/// 对登记前、提升前或提升后完成保留同一回执，包含可靠重载及精确重试。
#[test]
fn draft_upload_handoff_preserves_each_completion_order() -> Result<(), Box<dyn std::error::Error>>
{
    for completion_order in 0..3 {
        // Each ordering owns independent real managed bytes, leases and project storage.
        // 每种顺序持有独立真实托管字节、租约及项目存储。
        let scope = "composer-draft:v1:new:handoff";
        let (workspace, attachment) = fixture(scope)?;
        let project = tempfile::tempdir()?;
        let registry = ProjectResourceCatalogRegistry::default();
        let original =
            record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
        let ids = registry.resolve_or_register_managed_attachments(
            project.path(),
            std::slice::from_ref(&attachment),
        )?;
        let pending = original.prompt_delivery(ids[0])?;
        if completion_order == 0 {
            complete_attachment_draft_upload(
                &attachment,
                scope,
                &original,
                receipt(&attachment, "file-original"),
                &registry,
            )?;
        }
        register_attachment_draft_upload_handoff(
            &attachment,
            &original,
            project.path(),
            ids[0],
            &registry,
        )?;
        if completion_order == 1 {
            complete_attachment_draft_upload(
                &attachment,
                scope,
                &original,
                receipt(&attachment, "file-original"),
                &registry,
            )?;
        }
        promote_staged_attachment_to_message_ref(
            &workspace.path().to_string_lossy(),
            "session",
            "prompt",
            std::slice::from_ref(&attachment),
            &[scope.into()],
        )?;
        assert!(load_attachment_draft_media(&attachment, scope).is_err());
        if completion_order == 2 {
            assert!(
                load_project_resource_catalog(project.path())?.entries[0]
                    .provider_bindings
                    .is_empty()
            );
        }
        // The exact completion is idempotent even after the original live draft lease is gone.
        // 即使原活动草稿租约已消失，精确完成仍然幂等。
        complete_attachment_draft_upload(
            &attachment,
            scope,
            &original,
            receipt(&attachment, "file-original"),
            &registry,
        )?;
        let catalog = load_project_resource_catalog(project.path())?;
        let entry = &catalog.entries[0];
        assert_eq!(entry.provider_bindings.len(), 1);
        assert!(entry.provider_bindings[0].matches_prompt_upload(
            &pending.delivery,
            &entry.sha256,
            entry.size
        ));
        assert_eq!(entry.provider_bindings[0].media_id, "file-original");
        assert!(
            complete_attachment_draft_upload(
                &attachment,
                scope,
                &original,
                receipt(&attachment, "file-replacement"),
                &registry
            )
            .is_err()
        );
        assert_eq!(load_project_resource_catalog(project.path())?, catalog);
        // Completion does not rewrite the immutable pending parameters captured by the original prompt.
        // 完成不改写原提示词捕获的不可变待回执参数。
        assert_eq!(original.prompt_delivery(ids[0])?, pending);
    }
    Ok(())
}

/// Retries catalog publication from the receipt already accepted by the exact asset intent.
/// 从精确资产意图已经接纳的回执重试目录发布。
#[test]
fn draft_upload_handoff_recovers_after_catalog_publication_failure()
-> Result<(), Box<dyn std::error::Error>> {
    // The fixture owns both stores, so replacing only its catalog file is a deterministic local fault.
    // 夹具持有两类存储，因此仅替换其目录文件可形成确定性的本地故障。
    let scope = "composer-draft:v1:new:publication-retry";
    let (_workspace, attachment) = fixture(scope)?;
    let project = tempfile::tempdir()?;
    let registry = ProjectResourceCatalogRegistry::default();
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let ids = registry.resolve_or_register_managed_attachments(
        project.path(),
        std::slice::from_ref(&attachment),
    )?;
    register_attachment_draft_upload_handoff(
        &attachment,
        &original,
        project.path(),
        ids[0],
        &registry,
    )?;
    let catalog_path = project_resource_catalog_path(project.path());
    let saved_path = project.path().join("saved-resource-catalog.json");
    std::fs::rename(&catalog_path, &saved_path)?;
    std::fs::create_dir(&catalog_path)?;
    assert!(
        complete_attachment_draft_upload(
            &attachment,
            scope,
            &original,
            receipt(&attachment, "file-publication-retry"),
            &registry,
        )
        .is_err()
    );
    // Cross-store failure retains the accepted receipt in the original asset owner for exact retry.
    // 跨存储失败在原资产所有者中保留已接纳回执，以供精确重试。
    assert!(matches!(
        load_attachment_draft_media(&attachment, scope)?
            .expect("live original")
            .delivery,
        AttachmentDraftMediaDelivery::Upload {
            receipt: Some(_),
            ..
        }
    ));
    std::fs::remove_dir(&catalog_path)?;
    std::fs::rename(&saved_path, &catalog_path)?;
    complete_attachment_draft_upload(
        &attachment,
        scope,
        &original,
        receipt(&attachment, "file-publication-retry"),
        &registry,
    )?;
    let entry = load_project_resource_catalog(project.path())?
        .entries
        .remove(0);
    assert_eq!(entry.provider_bindings.len(), 1);
    assert_eq!(
        entry.provider_bindings[0].media_id,
        "file-publication-retry"
    );
    Ok(())
}

/// Keeps a new lease untouched by old completion, both with and without an earlier registered prompt destination.
/// 保持新租约不受旧完成影响，覆盖存在及不存在更早已登记提示词目标两种情况。
#[test]
fn draft_upload_handoff_never_overwrites_readded_intent() -> Result<(), Box<dyn std::error::Error>>
{
    for registered in [false, true] {
        // Reuse only the same managed asset, never its old intent UUID.
        // 仅复用同一托管资产，绝不复用其旧意图UUID。
        let scope = "composer-draft:v1:new:readd";
        let (workspace, attachment) = fixture(scope)?;
        let project = tempfile::tempdir()?;
        let registry = ProjectResourceCatalogRegistry::default();
        let original =
            record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
        let ids = registry.resolve_or_register_managed_attachments(
            project.path(),
            std::slice::from_ref(&attachment),
        )?;
        if registered {
            register_attachment_draft_upload_handoff(
                &attachment,
                &original,
                project.path(),
                ids[0],
                &registry,
            )?;
        }
        // Another legitimate lease keeps this asset alive, isolating same-asset remove/re-add from normal unreferenced-file cleanup.
        // 另一合法租约保留本资产，将同资产移除重加与正常无引用文件清理解耦。
        acquire_staged_attachment_ref(
            &workspace.path().to_string_lossy(),
            "composer-draft:v1:new:other-owner",
            std::slice::from_ref(&attachment),
        )?;
        release_staged_attachment_ref(
            &workspace.path().to_string_lossy(),
            scope,
            std::slice::from_ref(&attachment),
        )?;
        acquire_staged_attachment_ref(
            &workspace.path().to_string_lossy(),
            scope,
            std::slice::from_ref(&attachment),
        )?;
        let replacement =
            record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
        assert_ne!(replacement.intent_id, original.intent_id);
        let completed = complete_attachment_draft_upload(
            &attachment,
            scope,
            &original,
            receipt(&attachment, "file-original"),
            &registry,
        );
        assert_eq!(completed.is_ok(), registered);
        assert_eq!(
            load_attachment_draft_media(&attachment, scope)?,
            Some(replacement.clone())
        );
        let entry = load_project_resource_catalog(project.path())?
            .entries
            .remove(0);
        assert_eq!(entry.provider_bindings.len(), usize::from(registered));
        assert!(entry.provider_bindings.iter().all(|binding| {
            !binding.matches_prompt_upload(
                &replacement
                    .prompt_delivery(ids[0])
                    .expect("valid new intention")
                    .delivery,
                &entry.sha256,
                entry.size,
            )
        }));
    }
    Ok(())
}

/// Rejects a forged original route before registering any handoff and preserves the live original proof.
/// 登记任何交接前拒绝伪造原路线，并保留活动原证明。
#[test]
fn draft_upload_handoff_rejects_changed_original_identity() -> Result<(), Box<dyn std::error::Error>>
{
    let scope = "composer-draft:v1:new:identity";
    let (_workspace, attachment) = fixture(scope)?;
    let project = tempfile::tempdir()?;
    let registry = ProjectResourceCatalogRegistry::default();
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let ids = registry.resolve_or_register_managed_attachments(
        project.path(),
        std::slice::from_ref(&attachment),
    )?;
    let mut forged = original.clone();
    forged.original_route.model_id = "B".into();
    assert!(
        register_attachment_draft_upload_handoff(
            &attachment,
            &forged,
            project.path(),
            ids[0],
            &registry
        )
        .is_err()
    );
    assert_eq!(
        load_attachment_draft_media(&attachment, scope)?,
        Some(original)
    );
    Ok(())
}

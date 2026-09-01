//! Real managed-file lifetimes across draft replacement, purge, reattachment and concurrent source acquisition.
//! 草稿替换、清理、重新附加与并发来源取得期间的真实托管文件寿命。

use super::*;

/// Releasing the physical owner never deletes bytes already promoted to a durable message reference.
/// 释放实体所有者绝不删除已经提升为持久消息引用的字节。
#[test]
fn draft_source_hold_preserves_promoted_message_source() -> Result<(), Box<dyn std::error::Error>> {
    let scope = "composer-draft:v1:new:promoted-source";
    let (workspace, attachment) = fixture(scope)?;
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let (_, holding) = retain_draft_attachment_source(&attachment, scope, &original.intent_id)?;
    promote_staged_attachment_to_message_ref(
        &workspace.path().to_string_lossy(),
        "session-a",
        "message-a",
        std::slice::from_ref(&attachment),
        &[scope.to_owned()],
    )?;
    drop(holding);
    assert_eq!(fs::read(&attachment.local_path)?, b"abc");
    let root = resolve_asset_root_from_attachment(&attachment).ok_or("asset root")?;
    let document = load_attachment_asset_index_document(&root)?;
    assert_eq!(document.assets[0].session_ids, ["session-a"]);
    assert_eq!(document.assets[0].message_ids, ["message-a"]);
    assert!(document.assets[0].staging_refs.is_empty());
    assert_eq!(
        execute_attachment_cleanup(
            &workspace.path().to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled()
        )?
        .purged_asset_count,
        1
    );
    Ok(())
}

/// Two physical readers protect an orphan without writing persistent lease or provenance fields.
/// 两个实体读取者保护孤立资产，但不写入持久租约或来源字段。
#[test]
fn draft_source_holds_keep_bytes_until_last_reader_exits() -> Result<(), Box<dyn std::error::Error>>
{
    // The canonical source and original UUID come from real staging and draft transactions.
    // 规范来源与原UUID来自真实暂存及草稿事务。
    let scope = "composer-draft:v1:new:held-source";
    let (workspace, attachment) = fixture(scope)?;
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let root = resolve_asset_root_from_attachment(&attachment).ok_or("asset root")?;
    let index = storage::resolve_attachment_index_file(&root);
    let before = fs::read(&index)?;
    let (retained, first) =
        retain_draft_attachment_source(&attachment, scope, &original.intent_id)?;
    let (_, second) = retain_draft_attachment_source(&attachment, scope, &original.intent_id)?;
    assert!(
        crate::attachment_asset_index::source_hold::test_attachment_source_is_held(&attachment)?
    );
    assert_eq!(retained, original);
    assert_eq!(
        fs::read(&index)?,
        before,
        "physical ownership is not a persisted draft lease"
    );

    // Removing the visible draft really releases its persistent owner, but cannot destroy actively used bytes.
    // 移除可见草稿会真正释放其持久所有者，但不能销毁正在使用的字节。
    reconcile_staged_attachment_ref_scope(&workspace.path().to_string_lossy(), scope, &[])?;
    assert!(
        crate::attachment_asset_index::source_hold::test_attachment_source_is_held(&attachment)?
    );
    let document = load_attachment_asset_index_document(&root)?;
    assert!(document.assets[0].staging_refs.is_empty());
    assert!(document.assets[0].draft_media.is_empty());
    assert_eq!(fs::read(&attachment.local_path)?, b"abc");
    assert!(!discard_unreferenced_staged_attachment(
        &workspace.path().to_string_lossy(),
        &attachment
    )?);
    let selection = AttachmentCleanupSelection::all_enabled();
    assert_eq!(
        preview_attachment_cleanup(&workspace.path().to_string_lossy(), "9999-12-31", selection)?
            .matched_asset_count,
        0
    );
    assert_eq!(
        execute_attachment_cleanup(&workspace.path().to_string_lossy(), "9999-12-31", selection)?
            .purged_asset_count,
        0
    );
    drop(first);
    assert_eq!(fs::read(&attachment.local_path)?, b"abc");
    drop(second);
    assert!(!Path::new(&attachment.local_path).exists());
    assert!(
        load_attachment_asset_index_document(&root)?
            .assets
            .is_empty()
    );
    Ok(())
}

/// Physical retention cannot grant an obsolete original UUID access to a replacement draft or its receipt authority.
/// 实体保留不能授权过期原UUID访问替换草稿或其回执权威。
#[test]
fn draft_source_hold_does_not_revive_replaced_intent() -> Result<(), Box<dyn std::error::Error>> {
    let scope = "composer-draft:v1:new:replaced-source";
    let (workspace, attachment) = fixture(scope)?;
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let (_, holding) = retain_draft_attachment_source(&attachment, scope, &original.intent_id)?;
    release_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    assert!(retain_draft_attachment_source(&attachment, scope, &original.intent_id).is_err());
    assert!(
        record_attachment_draft_upload(
            &attachment,
            scope,
            &original.intent_id,
            receipt(&attachment, "old-upload")
        )
        .is_err()
    );
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    let replacement =
        record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    assert_ne!(replacement.intent_id, original.intent_id);
    assert!(retain_draft_attachment_source(&attachment, scope, &original.intent_id).is_err());
    drop(holding);
    assert_eq!(
        fs::read(&attachment.local_path)?,
        b"abc",
        "the replacement draft keeps its own source"
    );
    assert_eq!(
        load_attachment_draft_media(&attachment, scope)?,
        Some(replacement)
    );
    release_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    assert!(!Path::new(&attachment.local_path).exists());
    Ok(())
}

/// Rejected source facts cannot increment a hidden reader count that prevents subsequent orphan cleanup.
/// 被拒绝的来源事实不能增加隐藏读取计数，从而阻止后续孤立资产清理。
#[test]
fn invalid_draft_source_hold_leaves_no_reader() -> Result<(), Box<dyn std::error::Error>> {
    let scope = "composer-draft:v1:new:invalid-source";
    let (workspace, attachment) = fixture(scope)?;
    let original = record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
    let mut invalid = attachment.clone();
    invalid.source_sha256 = Some("f".repeat(64));
    assert!(retain_draft_attachment_source(&invalid, scope, &original.intent_id).is_err());
    assert!(
        retain_draft_attachment_source(&attachment, scope, &uuid::Uuid::new_v4().to_string())
            .is_err()
    );
    release_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    assert!(!Path::new(&attachment.local_path).exists());
    Ok(())
}

/// Acquisition and draft release have one linearization point: success owns existing bytes, failure grants no hold.
/// 取得来源与释放草稿拥有同一线性化点：成功持有存在的字节，失败不授予持有。
#[test]
fn draft_source_acquisition_is_atomic_with_removal() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..12 {
        // Each race owns an isolated index and starts both operations at the same barrier.
        // 每次竞争拥有隔离索引，并在同一屏障启动两项操作。
        let scope = "composer-draft:v1:new:racing-source";
        let (workspace, attachment) = fixture(scope)?;
        let original =
            record_attachment_draft_media(&attachment, scope, upload_origin(&attachment)?)?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let reader_barrier = barrier.clone();
        let reader_attachment = attachment.clone();
        let reader = std::thread::spawn(move || {
            reader_barrier.wait();
            retain_draft_attachment_source(&reader_attachment, scope, &original.intent_id)
        });
        barrier.wait();
        reconcile_staged_attachment_ref_scope(&workspace.path().to_string_lossy(), scope, &[])?;
        match reader.join().expect("source reader did not panic") {
            Ok((_, held)) => {
                assert_eq!(fs::read(&attachment.local_path)?, b"abc");
                drop(held);
            }
            Err(_) => assert!(!Path::new(&attachment.local_path).exists()),
        }
        assert!(!Path::new(&attachment.local_path).exists());
    }
    Ok(())
}

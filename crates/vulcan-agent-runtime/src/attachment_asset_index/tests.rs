//! Attachment asset-index regression tests covering staging records, cleanup previews, purge execution, and replay-state derivation.
//! 覆盖暂存记录、清理预览、清理执行与回放状态推导的附件资产索引回归测试。

use super::{
    AttachmentCleanupSelection, AttachmentReplayStatus, acquire_staged_attachment_ref,
    current_attachment_timestamp_parts, discard_unreferenced_staged_attachment,
    execute_attachment_cleanup, load_attachment_asset_index_document,
    load_attachment_asset_record_for_attachment, parse_attachment_cleanup_cutoff_date,
    preview_attachment_cleanup, promote_staged_attachment_to_message_ref, record_staged_attachment,
    release_staged_attachment_ref, resolve_attachment_replay_status,
    save_attachment_asset_index_document,
};
use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, resolve_attachment_replay_path,
    resolve_managed_attachment_root, stage_attachment_bytes,
};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Builds one unique temporary workspace directory path for attachment index tests.
/// 为附件索引测试构建一条唯一的临时工作区目录路径。
fn unique_workspace_directory() -> std::path::PathBuf {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("vulcan-attachment-index-{unique_id}"));
    std::fs::create_dir_all(&directory).expect("temporary workspace directory should create");
    directory
}

/// Stages one temporary managed attachment inside a unique workspace directory for testing.
/// 在唯一临时工作区目录中暂存一条供测试使用的临时托管附件。
fn build_test_attachment(
    workspace_directory: &std::path::Path,
    file_name: &str,
    kind: ConversationAttachmentKind,
    mime_type: &str,
) -> ConversationAttachment {
    let descriptor = attachment_descriptor_from_mime_type(mime_type)
        .expect("descriptor should resolve for the provided MIME type");
    let attachment = stage_attachment_bytes(
        &workspace_directory.to_string_lossy(),
        &descriptor,
        file_name,
        None,
        &[0_u8, 1, 2, 3],
    )
    .expect("attachment bytes should stage successfully");
    assert_eq!(attachment.kind, kind);
    attachment
}

/// Verifies staged attachments are indexed and later enriched with session/message references.
/// 验证已暂存附件会写入索引，并可在后续补充会话/消息引用关系。
#[test]
fn staged_attachment_records_can_be_enriched_with_references() {
    let workspace_directory = unique_workspace_directory();
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let timestamp = current_attachment_timestamp_parts();

    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");
    promote_staged_attachment_to_message_ref(
        &workspace_directory.to_string_lossy(),
        "session-1",
        "message-1",
        std::slice::from_ref(&attachment),
        &[],
    )
    .expect("attachment references should record");

    let record =
        load_attachment_asset_record_for_attachment(&attachment).expect("asset record should load");
    assert_eq!(
        record.asset_id,
        attachment.asset_id.expect("asset id should exist")
    );
    assert_eq!(record.session_ids, vec![String::from("session-1")]);
    assert_eq!(record.message_ids, vec![String::from("message-1")]);

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies transient lease acquisition fails when a managed attachment no longer has an index record.
/// 验证当托管附件不再拥有索引记录时，瞬时租约获取会失败。
#[test]
fn acquire_staged_attachment_ref_rejects_missing_index_record() {
    let workspace_directory = unique_workspace_directory();
    let mut attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    attachment.asset_id = Some(String::from("missing-index-asset"));

    let error = acquire_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer:test",
        std::slice::from_ref(&attachment),
    )
    .expect_err("missing index record should reject lease acquisition");

    assert!(error.contains("附件资产索引缺少"));
    assert!(std::path::Path::new(&attachment.local_path).is_file());
    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies durable message promotion fails when the managed asset index is missing.
/// 验证当托管资产索引缺失时，持久消息提升会失败。
#[test]
fn promote_staged_attachment_rejects_missing_index_record() {
    let workspace_directory = unique_workspace_directory();
    let mut attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    attachment.asset_id = Some(String::from("missing-index-asset"));

    let error = promote_staged_attachment_to_message_ref(
        &workspace_directory.to_string_lossy(),
        "session-1",
        "message-1",
        std::slice::from_ref(&attachment),
        &[],
    )
    .expect_err("missing index record should reject message promotion");

    assert!(error.contains("附件资产索引缺少"));
    assert!(std::path::Path::new(&attachment.local_path).is_file());
    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies staging leases protect accepted composer assets until the final scope is released.
/// 验证暂存租约会保护已进入输入框的资产，直到最后一个作用域被释放。
#[test]
fn staging_refs_protect_assets_until_release() {
    let workspace_directory = unique_workspace_directory();
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");

    acquire_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer:test",
        std::slice::from_ref(&attachment),
    )
    .expect("staging ref should acquire");
    assert!(
        !discard_unreferenced_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
        )
        .expect("leased attachment should not discard")
    );
    assert!(std::path::Path::new(&attachment.local_path).is_file());

    release_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer:test",
        std::slice::from_ref(&attachment),
    )
    .expect("staging ref should release");

    assert!(load_attachment_asset_record_for_attachment(&attachment).is_none());
    assert!(!std::path::Path::new(&attachment.local_path).exists());
    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies promoting staged assets releases only the dispatch lease while preserving unrelated transient owners.
/// 验证提升暂存资产只释放派发租约，并保留无关瞬时所有者。
#[test]
fn promote_staged_attachment_releases_only_requested_staging_refs() {
    let workspace_directory = unique_workspace_directory();
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.pdf",
        ConversationAttachmentKind::Pdf,
        "application/pdf",
    );
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");
    acquire_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "dispatch:test",
        std::slice::from_ref(&attachment),
    )
    .expect("staging ref should acquire");
    acquire_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer-undo:test",
        std::slice::from_ref(&attachment),
    )
    .expect("unrelated staging ref should acquire");

    promote_staged_attachment_to_message_ref(
        &workspace_directory.to_string_lossy(),
        "session-1",
        "message-1",
        std::slice::from_ref(&attachment),
        &[String::from("dispatch:test")],
    )
    .expect("staged attachment should promote");

    let record = load_attachment_asset_record_for_attachment(&attachment)
        .expect("promoted asset record should remain");
    assert_eq!(
        record.staging_refs,
        vec![String::from("composer-undo:test")]
    );
    assert_eq!(record.session_ids, vec![String::from("session-1")]);
    assert_eq!(record.message_ids, vec![String::from("message-1")]);
    release_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer-undo:test",
        std::slice::from_ref(&attachment),
    )
    .expect("unrelated staging ref should release");
    assert!(
        !discard_unreferenced_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
        )
        .expect("message-referenced asset should not discard")
    );
    assert!(std::path::Path::new(&attachment.local_path).is_file());
    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies cleanup preview counts one staged asset before a future cutoff date.
/// 验证当截止日期位于未来时，清理预览会统计一条已暂存资产。
#[test]
fn cleanup_preview_counts_matching_assets() {
    let workspace_directory = unique_workspace_directory();
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.pdf",
        ConversationAttachmentKind::Pdf,
        "application/pdf",
    );
    let timestamp = current_attachment_timestamp_parts();

    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");
    let preview = preview_attachment_cleanup(
        &workspace_directory.to_string_lossy(),
        "9999-12-31",
        AttachmentCleanupSelection::all_enabled(),
    )
    .expect("cleanup preview should build");

    assert_eq!(preview.matched_asset_count, 1);
    assert_eq!(preview.per_kind_counts.get("pdf"), Some(&1));

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies cleanup can purge durable history attachments while keeping transient staging leases protected.
/// 验证清理可以清除持久历史附件，同时继续保护瞬时暂存租约。
#[test]
fn cleanup_purges_durable_history_but_skips_transient_staging_refs() {
    let workspace_directory = unique_workspace_directory();
    let historical_attachment = build_test_attachment(
        &workspace_directory,
        "history.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let staged_attachment = build_test_attachment(
        &workspace_directory,
        "composer.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &historical_attachment,
        &timestamp,
    )
    .expect("historical attachment should record");
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &staged_attachment,
        &timestamp,
    )
    .expect("staged attachment should record");
    promote_staged_attachment_to_message_ref(
        &workspace_directory.to_string_lossy(),
        "session-1",
        "message-1",
        std::slice::from_ref(&historical_attachment),
        &[],
    )
    .expect("historical attachment references should record");
    acquire_staged_attachment_ref(
        &workspace_directory.to_string_lossy(),
        "composer:test",
        std::slice::from_ref(&staged_attachment),
    )
    .expect("composer staging ref should acquire");

    let preview = preview_attachment_cleanup(
        &workspace_directory.to_string_lossy(),
        "9999-12-31",
        AttachmentCleanupSelection::all_enabled(),
    )
    .expect("cleanup preview should build");
    assert_eq!(preview.matched_asset_count, 1);
    assert_eq!(preview.impacted_session_count, 1);

    let report = execute_attachment_cleanup(
        &workspace_directory.to_string_lossy(),
        "9999-12-31",
        AttachmentCleanupSelection::all_enabled(),
    )
    .expect("cleanup execution should succeed");

    assert_eq!(report.purged_asset_count, 1);
    assert_eq!(report.impacted_session_count, 1);
    assert_eq!(
        resolve_attachment_replay_status(&historical_attachment),
        AttachmentReplayStatus::Purged
    );
    assert_eq!(
        resolve_attachment_replay_status(&staged_attachment),
        AttachmentReplayStatus::Available
    );
    assert!(!std::path::Path::new(&historical_attachment.local_path).exists());
    assert!(std::path::Path::new(&staged_attachment.local_path).is_file());

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies purge execution marks the asset as purged and stops replay even when an original source path still exists.
/// 验证清理执行会把资产标记为已清理，并且即使原始来源路径仍存在，也会停止回放。
#[test]
fn purge_execution_prevents_source_fallback_replay() {
    let workspace_directory = unique_workspace_directory();
    let original_source_path = workspace_directory.join("original-diagram.png");
    std::fs::write(&original_source_path, [0_u8, 1, 2, 3]).expect("original source should write");
    let descriptor =
        attachment_descriptor_from_mime_type("image/png").expect("png descriptor should resolve");
    let attachment = stage_attachment_bytes(
        &workspace_directory.to_string_lossy(),
        &descriptor,
        "diagram.png",
        Some(original_source_path.as_path()),
        &[0_u8, 1, 2, 3],
    )
    .expect("attachment should stage");
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");

    let report = execute_attachment_cleanup(
        &workspace_directory.to_string_lossy(),
        "9999-12-31",
        AttachmentCleanupSelection::all_enabled(),
    )
    .expect("cleanup execution should succeed");

    assert_eq!(report.purged_asset_count, 1);
    assert_eq!(
        resolve_attachment_replay_status(&attachment),
        AttachmentReplayStatus::Purged
    );
    assert!(!std::path::Path::new(&attachment.local_path).exists());
    assert!(original_source_path.is_file());

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies date parsing rejects malformed cleanup cutoff values early.
/// 验证日期解析会尽早拒绝格式错误的清理截止值。
#[test]
fn cleanup_cutoff_date_parser_rejects_malformed_input() {
    assert!(parse_attachment_cleanup_cutoff_date("2026-05-15").is_ok());
    assert!(parse_attachment_cleanup_cutoff_date("20260515").is_err());
}

/// Verifies missing managed copies fall back to the original source path only when the asset was not explicitly purged.
/// 验证仅当资产未被显式清理时，缺失的托管副本才会回退到原始来源路径。
#[test]
fn replay_status_uses_source_fallback_when_asset_was_not_purged() {
    let workspace_directory = unique_workspace_directory();
    let original_source_path = workspace_directory.join("original-audio.mp3");
    std::fs::write(&original_source_path, [0_u8, 1, 2, 3]).expect("original source should write");
    let descriptor = attachment_descriptor_from_mime_type("audio/mpeg")
        .expect("audio descriptor should resolve");
    let attachment = stage_attachment_bytes(
        &workspace_directory.to_string_lossy(),
        &descriptor,
        "clip.mp3",
        Some(original_source_path.as_path()),
        &[0_u8, 1, 2, 3],
    )
    .expect("attachment should stage");
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");
    std::fs::remove_file(&attachment.local_path).expect("managed copy should remove");

    assert_eq!(
        resolve_attachment_replay_status(&attachment),
        AttachmentReplayStatus::FallbackSourceOnly
    );

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies cleanup refuses corrupted index records that point outside the current project attachment store.
/// 验证清理会拒绝指向当前项目附件存储之外的损坏索引记录。
#[test]
fn cleanup_rejects_index_paths_outside_attachment_store() {
    let workspace_directory = unique_workspace_directory();
    let outside_file = workspace_directory.join("outside.png");
    std::fs::write(&outside_file, [9_u8]).expect("outside file should write");
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let timestamp = current_attachment_timestamp_parts();
    record_staged_attachment(
        &workspace_directory.to_string_lossy(),
        &attachment,
        &timestamp,
    )
    .expect("staged attachment should record");

    let asset_root = resolve_managed_attachment_root(&workspace_directory.to_string_lossy());
    let mut document =
        load_attachment_asset_index_document(&asset_root).expect("index should load");
    let asset_id = attachment
        .asset_id
        .as_deref()
        .expect("staged attachment should have an asset id");
    let record = document
        .assets
        .iter_mut()
        .find(|record| record.asset_id == asset_id)
        .expect("asset record should exist");
    record.staged_date_text = String::from("1970-01-01");
    record.managed_path = outside_file.to_string_lossy().into_owned();
    save_attachment_asset_index_document(&asset_root, &document)
        .expect("tampered index should save");

    let error = execute_attachment_cleanup(
        &workspace_directory.to_string_lossy(),
        "9999-12-31",
        AttachmentCleanupSelection::all_enabled(),
    )
    .expect_err("cleanup should reject unsafe managed paths");

    assert!(error.contains("不在当前项目附件目录内"));
    assert!(outside_file.is_file());
    let record = load_attachment_asset_record_for_attachment(&attachment)
        .expect("asset record should still load");
    assert!(record.purged_at_unix_millis.is_none());

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

/// Verifies an explicit purge marker blocks replay even if the managed file still exists after a failed delete.
/// 验证即使删除失败后托管文件仍然存在，显式清理标记也会阻止附件继续回放。
#[test]
fn purged_marker_prevents_replay_when_managed_file_still_exists() {
    let workspace_directory = unique_workspace_directory();
    let attachment = build_test_attachment(
        &workspace_directory,
        "diagram.png",
        ConversationAttachmentKind::Image,
        "image/png",
    );
    let asset_root = resolve_managed_attachment_root(&workspace_directory.to_string_lossy());
    let mut document =
        load_attachment_asset_index_document(&asset_root).expect("index should load");
    let asset_id = attachment
        .asset_id
        .as_deref()
        .expect("staged attachment should have an asset id");
    let record = document
        .assets
        .iter_mut()
        .find(|record| record.asset_id == asset_id)
        .expect("asset record should exist");
    record.purged_at_local_text = Some(String::from("2026-05-16 00:00:00"));
    record.purged_at_unix_millis = Some(1_779_000_000_000);
    record.purge_reason = Some(String::from("purged-before-2026-05-16; delete-failed"));
    save_attachment_asset_index_document(&asset_root, &document).expect("purged index should save");

    assert!(std::path::Path::new(&attachment.local_path).is_file());
    assert_eq!(
        resolve_attachment_replay_status(&attachment),
        AttachmentReplayStatus::Purged
    );
    assert!(resolve_attachment_replay_path(&attachment).is_none());

    let _ = std::fs::remove_dir_all(&workspace_directory);
}

//! Physical asset lifetime remains owned by registration through cancellation and known-result publication.
//! 实体资产寿命由登记在取消及已知结果发布期间持续持有。

use super::*;
use vulcan_agent_runtime::attachment_asset_index::{
    AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin, acquire_staged_attachment_ref,
    reconcile_staged_attachment_ref_scope, record_attachment_draft_media,
};
use vulcan_core::types::ConversationAttachment;

/// Builds an isolated canonical file and original upload intention; returns their workspace, scope and original proof.
/// 构造隔离规范文件及原上传意图；返回其工作区、作用域与原证明。
fn source_fixture() -> (
    tempfile::TempDir,
    ConversationAttachment,
    String,
    AttachmentDraftMediaOrigin,
) {
    // All paths and credentials are fixture-owned; no user runtime is opened.
    // 全部路径及凭据均属夹具；不打开用户运行时。
    let directory = tempfile::tempdir().expect("source workspace");
    let attachment = vulcan_agent_runtime::attachment_assets::stage_attachment_bytes(
        &directory.path().to_string_lossy(),
        &vulcan_agent_runtime::attachment_assets::generic_attachment_descriptor(),
        "source.bin",
        None,
        b"abc",
    )
    .expect("stage canonical file");
    let scope = format!("composer-draft:v1:new:{}", uuid::Uuid::new_v4());
    acquire_staged_attachment_ref(
        &directory.path().to_string_lossy(),
        &scope,
        std::slice::from_ref(&attachment),
    )
    .expect("draft owner");
    let original = AttachmentDraftMediaOrigin::new(
        &attachment,
        vulcan_session_core::v1::UserPromptMediaRoute {
            provider_id: String::from("provider"),
            service_entry_id: String::from("entry"),
            connection_id: String::from("connection"),
            model_id: String::from("model"),
            protocol_profile_id: String::from("openai-responses.v1"),
        },
        AttachmentDraftMediaDelivery::Upload {
            upload_channel_id: String::from("files"),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            receipt: None,
        },
    )
    .expect("original intention");
    let original =
        record_attachment_draft_media(&attachment, &scope, original).expect("persist original");
    (directory, attachment, scope, original)
}

/// A cancelled upload releases its physical file hold before acknowledging settlement, even after draft replacement.
/// 即使草稿已经替换，已取消上传也在确认结算前释放实体文件持有。
#[tokio::test]
async fn media_registration_source_survives_until_cancel_settles() {
    let (directory, attachment, scope, original) = source_fixture();
    let id = format!("source-cancel:{}", uuid::Uuid::new_v4());
    let mut registration = register_media_upload(&id).expect("register physical owner");
    assert_eq!(
        registration
            .retain_draft_source(&attachment, &scope, &original.intent_id)
            .expect("retain source"),
        original
    );
    let token = registration.cancellation.clone();
    reconcile_staged_attachment_ref_scope(&directory.path().to_string_lossy(), &scope, &[])
        .expect("replace draft");
    assert_eq!(
        std::fs::read(&attachment.local_path).expect("held source"),
        b"abc"
    );
    let cancellation_id = id.clone();
    let cancelling = tokio::spawn(async move { cancel_media_upload(&cancellation_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("cancel signal");
    assert!(!cancelling.is_finished());
    assert!(std::path::Path::new(&attachment.local_path).exists());
    drop(registration);
    assert!(cancelling.await.expect("cancel task").expect("settlement"));
    assert!(
        !std::path::Path::new(&attachment.local_path).exists(),
        "orphan cleanup precedes settlement acknowledgement"
    );
    assert!(!is_registered(&id, &token));
}

/// Aborting an RPC cannot remove source bytes still used by its already-known result publisher.
/// 中止RPC不能移除其已知结果发布器仍在使用的来源字节。
#[tokio::test]
async fn media_registration_known_publisher_retains_source_after_rpc_abort() {
    let (directory, attachment, scope, original) = source_fixture();
    let id = format!("source-publisher:{}", uuid::Uuid::new_v4());
    let mut registration = register_media_upload(&id).expect("register physical owner");
    registration
        .retain_draft_source(&attachment, &scope, &original.intent_id)
        .expect("retain source");
    let token = registration.cancellation.clone();
    let publisher_path = attachment.local_path.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(1);
    let task = tokio::spawn(async move {
        registration
            .spawn_known_completion(move || {
                started_tx.send(()).expect("publisher start");
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("publisher release");
                assert_eq!(
                    std::fs::read(publisher_path).expect("source survives until publication"),
                    b"abc"
                );
            })
            .await
            .expect("publisher task");
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), started_rx)
        .await
        .expect("publisher bound")
        .expect("publisher started");
    reconcile_staged_attachment_ref_scope(&directory.path().to_string_lossy(), &scope, &[])
        .expect("replace visible draft");
    task.abort();
    assert!(task.await.expect_err("aborted observer").is_cancelled());
    assert_eq!(
        std::fs::read(&attachment.local_path).expect("still physically owned"),
        b"abc"
    );
    let cancellation_id = id.clone();
    let cancelling = tokio::spawn(async move { cancel_media_upload(&cancellation_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("cancel signal");
    assert!(!cancelling.is_finished());
    release_tx.send(()).expect("allow publication");
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), cancelling)
            .await
            .expect("settlement bound")
            .expect("cancel task")
            .expect("settled")
    );
    assert!(!std::path::Path::new(&attachment.local_path).exists());
    assert!(!is_registered(&id, &token));
}

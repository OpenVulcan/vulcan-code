//! Exact pending-upload cleanup across cancellation, RPC abort and known-result settlement.
//! 取消、RPC中止及已知结果结算期间的精确待上传清理。

use super::super::{cancel_media_upload, register_media_upload};
use super::*;
mod source_hold;

/// Returns whether exact `id` still belongs to `token` in the real pending registry, without changing either owner.
/// 返回真实待处理注册表中精确`id`是否仍属于`token`，不修改任一所有者。
fn is_registered(id: &str, token: &MediaUploadCancellation) -> bool {
    pending_media_uploads()
        .lock()
        .expect("pending owner")
        .get(id)
        .is_some_and(|current| current.cancellation.same_operation(token))
}

/// Dropping an ordinary preparation cancels its background token and removes only that pending registration.
/// 丢弃普通准备会取消其后台令牌，并仅移除该待处理登记。
#[tokio::test]
async fn media_registration_rpc_abort_releases_exact_owner() {
    // Each test uses an opaque unique id inside the production pending registry.
    // 每个测试在生产待处理注册表内使用不透明唯一ID。
    let id = format!("registration-abort:{}", uuid::Uuid::new_v4());
    let registration = register_media_upload(&id).expect("register");
    let token = registration.cancellation.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _registration = registration;
        started_tx.send(()).expect("started observer");
        std::future::pending::<()>().await;
    });
    started_rx.await.expect("preparation started");
    assert!(is_registered(&id, &token));
    task.abort();
    assert!(task.await.expect_err("aborted RPC").is_cancelled());
    assert!(token.is_cancelled());
    assert!(!is_registered(&id, &token));
    assert!(
        !cancel_media_upload(&id)
            .await
            .expect("no surviving registration")
    );
}

/// Explicit cancellation signals the operation without releasing its entry before the worker actually exits.
/// 显式取消通知操作，但不会在工作线程真正退出之前释放其条目。
#[tokio::test]
async fn media_registration_cancel_is_not_premature_cleanup() {
    // Cancellation request and terminal settlement deliberately remain different events.
    // 取消请求与终态结算有意保持为不同事件。
    let id = format!("registration-cancel:{}", uuid::Uuid::new_v4());
    let registration = register_media_upload(&id).expect("register");
    let token = registration.cancellation.clone();
    // The actual cancellation API must stay pending until its operation owner has left.
    // 实际取消API必须保持待完成，直到其操作所有者退出。
    let cancelling_id = id.clone();
    let cancelling = tokio::spawn(async move { cancel_media_upload(&cancelling_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("cancel request reached owner");
    assert!(!cancelling.is_finished());
    assert!(token.is_cancelled());
    assert!(is_registered(&id, &token));
    assert!(register_media_upload(&id).is_err());
    drop(registration);
    assert!(!is_registered(&id, &token));
    assert!(
        cancelling
            .await
            .expect("cancel waiter")
            .expect("terminal acknowledgement")
    );
}

/// An older guard cannot remove a replacement with the same id, regardless of either token's cancellation value.
/// 无论两个令牌的取消值如何，较早守卫都不能移除使用相同ID的替换登记。
#[test]
fn media_registration_old_guard_cannot_remove_replacement() {
    for cancel_replacement in [false, true] {
        // Replace the test registry entry explicitly to exercise generation-safe cleanup under a reused textual id.
        // 显式替换测试注册条目，以在文本ID复用时验证代际安全清理。
        let id = format!("registration-replaced:{}", uuid::Uuid::new_v4());
        let original = register_media_upload(&id).expect("original");
        let original_token = original.cancellation.clone();
        pending_media_uploads()
            .lock()
            .expect("pending owner")
            .remove(&id);
        let replacement = register_media_upload(&id).expect("replacement");
        let replacement_token = replacement.cancellation.clone();
        if cancel_replacement {
            replacement_token.cancel();
        }
        drop(original);
        assert!(original_token.is_cancelled());
        assert_eq!(replacement_token.is_cancelled(), cancel_replacement);
        assert!(is_registered(&id, &replacement_token));
        drop(replacement);
        assert!(!is_registered(&id, &replacement_token));
    }
}

/// A validated-result worker retains the production registration after its RPC is aborted and releases it only after settlement.
/// 已验证结果工作线程在RPC中止后保留生产登记，并仅在结算后释放。
#[tokio::test]
async fn media_registration_known_completion_outlives_rpc_abort() {
    // The exact same production handoff method owns the background publisher in this deterministic race.
    // 本确定性竞争由完全相同的生产交接方法持有后台发布器。
    let id = format!("registration-complete:{}", uuid::Uuid::new_v4());
    let registration = register_media_upload(&id).expect("register");
    let token = registration.cancellation.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(1);
    let task = tokio::spawn(async move {
        registration
            .spawn_known_completion(move || {
                started_tx.send(()).expect("worker observer");
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("release known result");
            })
            .await
            .expect("known publisher");
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), started_rx)
        .await
        .expect("worker start bound")
        .expect("worker started");
    task.abort();
    assert!(task.await.expect_err("aborted RPC").is_cancelled());
    assert!(is_registered(&id, &token));
    assert!(!token.is_cancelled());
    // An explicit cancellation of already-known publication also waits for settlement instead of claiming the file can be released immediately.
    // 显式取消已知发布同样等待结算，而非声称文件可立即释放。
    let cancelling_id = id.clone();
    let cancelling = tokio::spawn(async move { cancel_media_upload(&cancelling_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("cancel known publisher request");
    assert!(!cancelling.is_finished());
    assert!(is_registered(&id, &token));
    release_tx.send(()).expect("release");
    // The separate settlement signal, not the earlier cancellation bit, proves cleanup has completed.
    // 独立结算信号而非更早取消位证明清理已经完成。
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), cancelling)
            .await
            .expect("settlement bound")
            .expect("cancel task")
            .expect("settled acknowledgement")
    );
    assert!(!is_registered(&id, &token));
}

/// The real RPC dispatcher returns cancellation success only after the registered upload owner settles, leaving its isolated workspace snapshot unchanged.
/// 真实RPC分发器仅在已登记上传所有者结算后返回取消成功，保持隔离工作区快照不变。
#[tokio::test]
async fn media_registration_cancel_rpc_acknowledges_settlement() {
    // The isolated state has no external runtime or provider; the real cancellation branch only needs the pending registry.
    // 隔离状态没有外部运行时或供应商；真实取消分支只需要待处理注册表。
    let state = std::sync::Arc::new(std::sync::Mutex::new(
        crate::tests::test_support::test_service_state_with_parent_session(
            vulcan_workbench_model::SessionId::new_v4(),
        ),
    ));
    // WorkspaceSnapshot is a Debug-derived in-memory type, not a serialization DTO; compare this isolated fixture without adding a production serializer.
    // WorkspaceSnapshot是派生Debug的内存类型而非序列化DTO；比较本隔离夹具，不为生产类型新增序列化。
    let before = format!("{:?}", state.lock().expect("state").workspace_snapshot);
    let id = format!("registration-rpc:{}", uuid::Uuid::new_v4());
    let registration = register_media_upload(&id).expect("register");
    let token = registration.cancellation.clone();
    let rpc_state = std::sync::Arc::clone(&state);
    let rpc_id = id.clone();
    let rpc = tokio::spawn(async move {
        crate::rpc_dispatch::dispatch_service_rpc(
            crate::ServiceEventEmitter::new(Vec::new()),
            rpc_state,
            "service_cancel_conversation_submission".into(),
            Some(serde_json::json!({"clientSubmissionId":rpc_id})),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("RPC cancellation signal");
    assert!(
        !rpc.is_finished(),
        "signal alone cannot acknowledge safe lease release"
    );
    assert!(is_registered(&id, &token));
    drop(registration);
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), rpc)
            .await
            .expect("RPC settles")
            .expect("RPC task")
            .expect("RPC response"),
        serde_json::json!(true)
    );
    assert!(!is_registered(&id, &token));
    assert_eq!(
        crate::rpc_dispatch::dispatch_service_rpc(
            crate::ServiceEventEmitter::new(Vec::new()),
            std::sync::Arc::clone(&state),
            "service_cancel_conversation_submission".into(),
            Some(serde_json::json!({"clientSubmissionId":id}))
        )
        .await
        .expect("already settled RPC"),
        serde_json::json!(false)
    );
    assert_eq!(
        format!("{:?}", state.lock().expect("state").workspace_snapshot),
        before
    );
}

/// Publication failure or panic must still release the exact registry owner and settle every cancellation waiter.
/// 发布失败或异常仍必须释放精确注册所有者并结算所有取消等待者。
#[tokio::test]
async fn media_registration_failed_publisher_still_settles() {
    for panic_in_publisher in [false, true] {
        // Panic occurs outside registry locks, so this fault injection cannot poison unrelated test owners.
        // 异常发生在注册表锁之外，因此本故障注入不会毒化其他测试所有者。
        let id = format!("registration-publish-failure:{}", uuid::Uuid::new_v4());
        let registration = register_media_upload(&id).expect("register");
        let token = registration.cancellation.clone();
        let worker = registration.spawn_known_completion(move || {
            assert!(!panic_in_publisher, "isolated publisher panic");
            Err::<(), &'static str>("isolated publication failure")
        });
        let result = worker.await;
        if panic_in_publisher {
            assert!(result.expect_err("publisher panic").is_panic());
        } else {
            assert_eq!(
                result.expect("worker return"),
                Err("isolated publication failure")
            );
        }
        assert!(token.is_cancelled());
        assert!(!is_registered(&id, &token));
        assert!(
            !cancel_media_upload(&id)
                .await
                .expect("no phantom active work")
        );
    }
}

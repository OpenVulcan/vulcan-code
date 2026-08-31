//! Regression tests for close ownership, diagnostic retention, and retry claims.
//! 关闭所有权、诊断保留与重试领取的回归测试。

use super::super::VulcanPluginInvocationRequest;
use super::super::tests::{test_manager, write_idle_session_package};
use super::{RetainedVulcanPluginLease, lock_sessions};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Verifies an explicit close failure is projected as a visible cleanup diagnostic.
/// 验证显式关闭失败会投影为可见的清理诊断。
#[test]
fn explicit_close_failure_publishes_cleanup_diagnostic() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    let opened = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_open"),
            arguments: json!({}),
            owner_session_id: String::from("owner-close-failure"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("open close-failure fixture session");
    let session_id = opened["session_id"]
        .as_str()
        .expect("read opened session id")
        .to_owned();
    let session = lock_sessions(&manager.state.sessions)
        .expect("lock close-failure sessions")
        .get(&session_id)
        .cloned()
        .expect("resolve close-failure session");
    runtime
        .block_on(manager.close_raw_lease(&session.lease_id, &session.sid, session.generation))
        .expect("close fixture lease before explicit close");

    let error = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_close"),
            arguments: json!({ "session_id": session_id }),
            owner_session_id: String::from("owner-close-failure"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect_err("reject explicit close after lease disappeared");
    assert!(
        error
            .message
            .contains("lease cleanup failed and remains active")
    );
    let plugin = &manager.snapshot().plugins["idle-plugin"];
    assert_eq!(plugin.active_session_count, 1);
    assert!(
        plugin
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.contains("lease cleanup failed and remains active") })
    );

    lock_sessions(&manager.state.sessions)
        .expect("lock fixture sessions for cleanup")
        .remove(&session_id);
    manager.release_session_slot("idle-plugin");
}

/// Verifies a competing lifecycle drain waits for the unique session close owner.
/// 验证竞争的生命周期排空会等待唯一会话关闭所有者。
#[test]
fn concurrent_session_drain_waits_for_close_owner() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    runtime.block_on(async {
        let opened = manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: String::from("idle_open"),
                arguments: json!({}),
                owner_session_id: String::from("owner-race"),
                workspace_root: temp.path().to_path_buf(),
            })
            .await
            .expect("open race fixture session");
        let session_id = opened["session_id"]
            .as_str()
            .expect("read race session id")
            .to_owned();
        let session = lock_sessions(&manager.state.sessions)
            .expect("lock race sessions")
            .get(&session_id)
            .cloned()
            .expect("resolve race session");
        assert!(!session.closing.load(Ordering::Acquire));
        manager
            .record_cleanup_failure("idle-plugin", String::from("race cleanup fixture"))
            .expect("record race cleanup diagnostic");
        let close_owner = session.close_gate.lock().await;
        let drain_manager = manager.clone();
        let drain =
            tokio::spawn(async move { drain_manager.drain_owner_sessions("owner-race").await });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!drain.is_finished());
        drop(close_owner);
        let report = drain
            .await
            .expect("join competing drain")
            .expect("complete competing drain");
        let still_registered = lock_sessions(&manager.state.sessions)
            .expect("inspect race sessions")
            .contains_key(&session_id);
        assert_eq!(
            report.closed_sessions,
            1,
            "closing={} registered={still_registered}",
            session.closing.load(Ordering::Acquire)
        );
        assert!(report.failures.is_empty());
        assert!(
            !manager.snapshot().plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "race cleanup fixture")
        );
    });
}

/// Verifies lifecycle rollback cannot hide cleanup diagnostics while resources remain active.
/// 验证生命周期回滚无法在资源仍活动时隐藏清理诊断。
#[test]
fn lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    runtime.block_on(async {
        manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: String::from("idle_open"),
                arguments: json!({}),
                owner_session_id: String::from("owner-rollback"),
                workspace_root: temp.path().to_path_buf(),
            })
            .await
            .expect("open rollback fixture session");
        manager
            .record_cleanup_failure("idle-plugin", String::from("rollback cleanup fixture"))
            .expect("record rollback cleanup diagnostic");
        manager
            .begin_draining("idle-plugin")
            .expect("begin rollback fixture transition");
        let rolled_back = manager
            .finish_lifecycle_transition(
                "idle-plugin",
                &BTreeSet::new(),
                &BTreeSet::new(),
                Vec::new(),
            )
            .expect("finish rollback fixture transition");
        assert!(
            rolled_back.plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "rollback cleanup fixture")
        );

        let report = manager
            .drain_owner_sessions("owner-rollback")
            .await
            .expect("drain rollback fixture session");
        assert_eq!(report.closed_sessions, 1);
        assert!(
            !manager.snapshot().plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "rollback cleanup fixture")
        );
    });
}

/// Verifies retained lease retry ownership is atomically claimed and restored after failure.
/// 验证保留租约的重试所有权会被原子领取，并可在失败后恢复。
#[test]
fn retained_lease_retry_claim_is_atomic() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .retain_failed_lease(
            RetainedVulcanPluginLease {
                plugin_id: String::from("claim-plugin"),
                owner_session_id: Some(String::from("claim-owner")),
                lease_id: String::from("claim-lease"),
                sid: String::from("claim-sid"),
                generation: 1,
                reserved_session_slot: false,
            },
            String::from("claim fixture"),
        )
        .expect("retain fixture lease");

    let first = manager
        .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
        .expect("claim retained lease");
    let second = manager
        .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
        .expect("repeat retained lease claim");
    assert_eq!(first.len(), 1);
    assert!(second.is_empty());
    manager
        .restore_retained_lease(first[0].clone())
        .expect("restore claimed lease");
    assert_eq!(
        manager
            .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
            .expect("claim restored lease")
            .len(),
        1
    );
}

//! Regression tests for close ownership, diagnostic retention, and retry claims.
//! 关闭所有权、诊断保留与重试领取的回归测试。

use super::super::VulcanPluginInvocationRequest;
use super::super::tests::{test_manager, write_idle_session_package};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;

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
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal session parent");
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

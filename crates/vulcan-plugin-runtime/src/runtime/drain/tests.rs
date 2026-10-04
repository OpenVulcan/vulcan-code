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
            manager
                .observe_catalog(&rolled_back)
                .expect("actual retained rollback diagnostic projection")
                .plugins["idle-plugin"]
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
            manager
                .observe_catalog(&manager.snapshot())
                .expect("ordinary owner drain must retain unknown failure")
                .plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic == "rollback cleanup fixture")
        );
        // An ordinary empty finish is publication, not acknowledgement of an unknown failure.
        // 普通空 finish 是发布，不是对未知失败的确认。
        manager
            .finish_lifecycle_transition(
                "idle-plugin",
                &BTreeSet::new(),
                &BTreeSet::new(),
                Vec::new(),
            )
            .expect("finish after actual retirement");
        assert!(
            manager
                .has_cleanup_diagnostics("idle-plugin")
                .expect("unknown failure remains after finish")
        );
    });
}

/// Exact duplicate messages retain all failure sources and only the explicitly acknowledged domain can leave.
/// 精确重复消息保留全部失败来源，仅显式确认的域可以移除。
/// No arguments or return value; exercises the sole map without substituting for real native retirement tests.
/// 无参数及返回值；验证唯一映射，不替代真实原生退役测试。
#[test]
fn cleanup_failure_priority_and_domain_acknowledgement_preserve_unrelated_sources() {
    // Isolated empty manager makes physical reconciliation independent of native fixture timing.
    // 隔离空管理器使物理对账独立于原生夹具时序。
    let root = tempfile::tempdir().expect("private accounting root");
    // Manager owns the sole diagnostics map and the real publication gate.
    // manager 拥有唯一诊断映射及实际发布门。
    let manager = test_manager(root.path(), root.path().join("plugins"));
    manager
        .record_cleanup_pending("plugin", "shared".into())
        .expect("finite pending first");
    {
        // Registration serializes the real source merge and explicit owner acknowledgement.
        // registration 串行化实际来源合并及显式所有者确认。
        let registration = manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        manager
            .record_domain_cleanup_locked(
                "plugin",
                "shared".into(),
                "domain-a",
                true,
                &registration,
            )
            .expect("domain failure dominates pending");
        manager
            .record_domain_cleanup_locked(
                "plugin",
                "shared".into(),
                "domain-b",
                true,
                &registration,
            )
            .expect("independent identical failure");
        manager
            .acknowledge_domain_cleanup_locked("plugin", "domain-a", &registration)
            .expect("ack only first domain");
    }
    manager
        .record_cleanup_pending("plugin", "shared".into())
        .expect("pending cannot downgrade failure");
    manager
        .clear_cleanup_diagnostics_if_fully_drained("plugin")
        .expect("empty physical reconciliation");
    assert!(
        manager
            .has_cleanup_diagnostics("plugin")
            .expect("second domain survives")
    );
    manager
        .record_cleanup_failure("plugin", "shared".into())
        .expect("unknown identical failure");
    {
        // Registration protects acknowledgement without treating unknown evidence as domain-owned.
        // registration 保护确认，不把未知证据视为域所有。
        let registration = manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        manager
            .acknowledge_domain_cleanup_locked("plugin", "domain-b", &registration)
            .expect("ack second domain");
    }
    assert!(
        manager
            .has_cleanup_diagnostics("plugin")
            .expect("unknown source survives both domain acknowledgements")
    );
    manager
        .record_cleanup_pending("finite", "pending only".into())
        .expect("separate finite observation");
    manager
        .clear_cleanup_diagnostics_if_fully_drained("finite")
        .expect("no physical owner");
    assert!(
        !manager
            .has_cleanup_diagnostics("finite")
            .expect("finite observation reconciled")
    );
}

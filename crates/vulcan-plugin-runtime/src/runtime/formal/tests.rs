//! Exact manager ownership and real bundle use of the formal parent runtime.
//! 管理器精确所有权及正式父运行时的真实 bundle 消费。

use super::*;
use crate::runtime::tests::{test_manager, write_hook_package};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedPluginConfig, ExecutionBackend, InstanceReuse, ModuleFinalizer,
    OperationPhase, PluginPoolConfig, PoolKind,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

mod draining;
mod pinned;
mod stateless;

/// Observe complete shutdown with the same configured finite test budget.
/// 使用同一已配置有限测试预算观测完整关闭。
/// `manager` retains the owner; returns only after actual core termination, or fails the test.
/// `manager` 保留所有者；仅在核心真实终止后返回，否则令测试失败。
fn wait_closed(manager: &VulcanPluginRuntimeManager, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !manager
        .poll_formal_runtime_closed()
        .expect("observe formal shutdown")
    {
        assert!(
            Instant::now() < deadline,
            "formal runtime did not actually drain"
        );
        std::thread::yield_now();
    }
}

/// Manager clones share one fixed parent, invalid attachment leaves no owner, and real modules use that parent.
/// 管理器克隆共享单个固定父级，无效附加不留下所有者，真实模块使用该父级。
#[test]
fn formal_manager_owns_one_parent_and_closes_real_bundle() {
    let root = tempfile::tempdir().expect("create manager fixture");
    let bundled = root.path().join("bundled");
    write_hook_package(&bundled);
    let manager = test_manager(root.path(), bundled);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load real catalog");
    assert!(
        manager
            .formal_runtime_status()
            .expect("unattached status")
            .is_none()
    );
    let mut invalid = VulcanPluginRuntimeSettings::default();
    invalid.parent.max_registered_capabilities = 1;
    assert!(manager.initialize_formal_runtime(invalid).is_err());
    assert!(
        manager
            .formal_runtime_status()
            .expect("failed attachment status")
            .is_none()
    );
    let settings = VulcanPluginRuntimeSettings::default();
    let timeout = Duration::from_millis(settings.shutdown_timeout_ms);
    manager
        .initialize_formal_runtime(settings.clone())
        .expect("attach formal parent");
    let clone = manager.clone();
    let owner = manager
        .formal_runtime_owner()
        .expect("owner lookup")
        .expect("attached parent");
    let cloned_owner = clone
        .formal_runtime_owner()
        .expect("cloned lookup")
        .expect("same parent");
    assert!(Arc::ptr_eq(&owner, &cloned_owner));
    assert!(manager.initialize_formal_runtime(settings.clone()).is_err());
    // Plugin budgets in this fixture derive from its actual parent's limits, not copied literals.
    // 此夹具插件预算派生自实际父级上限，不复制字面量。
    let parent = &settings.parent;
    owner
        .host
        .runtime()
        .register_plugin(
            "hook-plugin".to_string(),
            EmbeddedPluginConfig {
                max_registered_pools: parent.max_registered_pools,
                max_sessions: parent.max_sessions,
                max_resident_vms: parent.max_resident_vms,
                max_running_calls: parent.max_running_calls,
                max_queued_calls: parent.max_queued_calls,
                max_queued_bytes: parent.max_queued_bytes,
                max_operations: parent.max_operations,
            },
        )
        .expect("register actual plugin aggregate");
    let pool = owner
        .host
        .register_bundle_pool(VulcanPluginBundlePoolRequest {
            plugin_id: "hook-plugin".to_string(),
            plugin_generation: manager.snapshot().plugins["hook-plugin"].generation,
            workspace_root: root.path().to_path_buf(),
            security_partition: "manager-fixture".to_string(),
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            policy: PluginPoolConfig {
                kind: PoolKind::Shared,
                min_resident_vms: 0,
                max_resident_vms: 1,
                max_running_calls: 1,
                max_queued_calls: parent.max_queued_calls,
                reuse: InstanceReuse::SingleCall,
                serial: true,
                backend: ExecutionBackend::InProcess,
                idle_ttl_ms: None,
                max_uses: None,
            },
            finalizer: Some(ModuleFinalizer {
                export: "shutdown".to_string(),
                arguments: json!({}),
                timeout_ms: settings.shutdown_timeout_ms,
            }),
        })
        .expect("register manager-owned bundle");
    let operation = owner
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool,
                export: "dispatch".to_string(),
                arguments: json!({}),
                context: LuaInvocationContext::default(),
            },
            timeout,
        )
        .expect("submit real bundle");
    let completed = operation.wait(timeout).expect("wait real bundle");
    assert_eq!(completed.phase, OperationPhase::Succeeded, "{completed:?}");
    assert!(
        completed
            .finalization
            .expect("automatic shutdown")
            .outcome
            .expect("shutdown result")
            .result()
            .is_ok()
    );
    let status = clone
        .formal_runtime_status()
        .expect("observe cloned parent")
        .expect("attached status");
    assert_eq!(status.runtime_id, owner.host.runtime().id());
    assert_eq!(status.settings, settings);
    manager
        .request_formal_runtime_close()
        .expect("close manager parent");
    wait_closed(&clone, timeout);
    let status = manager
        .formal_runtime_status()
        .expect("closed status")
        .expect("retained status");
    assert!(status.operations.closing);
    assert_eq!(status.resources.resident, 0);
    // Complete closure still does not allow silently replacing the manager's frozen runtime identity.
    // 完整关闭后仍不允许静默替换管理器冻结的运行时身份。
    assert!(manager.initialize_formal_runtime(settings).is_err());
}

/// Last manager release stops admission even when a control observer retains the formal owner.
/// 最后管理器释放时停止入场，即使控制观测者仍保留正式所有者。
#[test]
fn formal_manager_drop_requests_close_for_retained_observer() {
    let root = tempfile::tempdir().expect("create drop fixture");
    let manager = test_manager(root.path(), root.path().join("bundled"));
    let settings = VulcanPluginRuntimeSettings::default();
    let timeout = Duration::from_millis(settings.shutdown_timeout_ms);
    manager
        .initialize_formal_runtime(settings)
        .expect("attach drop fixture");
    let owner = manager
        .formal_runtime_owner()
        .expect("owner lookup")
        .expect("retained owner");
    drop(manager);
    assert!(
        owner
            .host
            .runtime()
            .usage()
            .expect("closed admission")
            .closing
    );
    let deadline = Instant::now() + timeout;
    while !owner
        .host
        .runtime()
        .poll_closed()
        .expect("actual runtime shutdown")
    {
        assert!(
            Instant::now() < deadline,
            "retained formal owner did not close"
        );
        std::thread::yield_now();
    }
}

/// Content-only managers report absence explicitly and cannot create formal workers without an engine.
/// 纯内容管理器显式报告缺失，不能在没有引擎时创建正式工作线程。
#[test]
fn metadata_only_manager_does_not_allocate_formal_runtime() {
    let root = tempfile::tempdir().expect("create metadata fixture");
    let manager = VulcanPluginRuntimeManager::without_runtime(
        root.path().join("bundled"),
        root.path().join("user"),
        root.path().join("bridge"),
        root.path().join("managed"),
        root.path().join("state"),
        root.path().join("runtime"),
    );
    assert!(
        manager
            .formal_runtime_status()
            .expect("metadata status")
            .is_none()
    );
    manager.request_formal_runtime_close().expect("empty close");
    // Public capability access cannot create an implicit registry for a metadata-only manager.
    // 公开能力访问不能为纯元数据管理器隐式创建注册表。
    assert!(manager.formal_capabilities().is_err());
    assert!(
        manager
            .poll_formal_runtime_closed()
            .expect("empty termination")
    );
    assert!(
        manager
            .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
            .is_err()
    );
    assert!(
        manager
            .formal_runtime_status()
            .expect("rejected attachment")
            .is_none()
    );
}

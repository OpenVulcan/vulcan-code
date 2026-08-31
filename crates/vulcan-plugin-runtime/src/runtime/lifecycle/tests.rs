//! Regression tests for plugin lifecycle compensation and package generations.
//! 插件生命周期补偿与包代次的回归测试。

use super::super::tests::{test_manager, write_hook_package};
use super::{VulcanPluginLifecycleEvent, VulcanPluginRuntimeState};
use std::collections::BTreeSet;
use std::fs;

/// Verifies enable rollback invokes the disable callback while runtime state remains enabling.
/// 验证启用回滚会在运行时仍处于启用中状态时调用停用回调。
#[test]
fn enable_rollback_uses_declared_disable_callback_in_enabling_scope() {
    // temp owns the isolated bundled plugin and bridge fixture.
    // temp 持有隔离的内建插件与桥接测试环境。
    let temp = tempfile::tempdir().expect("create lifecycle rollback directory");
    // runtimeRoot is the deterministic manager root for this lifecycle test.
    // runtimeRoot 是当前生命周期测试使用的确定性管理器根目录。
    let runtime_root = temp.path().join("runtime");
    // vulcanPluginRoot contains the lifecycle-enabled hook fixture.
    // vulcanPluginRoot 包含支持生命周期的 Hook 测试插件。
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_hook_package(&vulcan_plugin_root);
    // manager owns the exact catalog transition tested below.
    // manager 持有下方测试的精确目录转换。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    // disabled makes the fixture eligible to enter a host-controlled enabling transition.
    // disabled 使测试插件能够进入宿主控制的启用中转换。
    let disabled = BTreeSet::from([String::from("hook-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled lifecycle fixture");
    manager
        .begin_enabling("hook-plugin")
        .expect("begin lifecycle enabling transition");
    // runtime executes the asynchronous compensation lease deterministically.
    // runtime 确定性执行异步补偿租约。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create lifecycle Tokio runtime");
    // error proves the manifest disable callback ran through the rollback scope.
    // error 证明清单停用回调通过回滚作用域得到执行。
    let error = runtime
        .block_on(
            manager.invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::EnableRollback),
        )
        .expect_err("surface rollback marker from disable callback");
    assert!(error.to_string().contains("enable-rollback-marker"));
}

/// Verifies disable compensation can re-enter the only state authorized for enable probing.
/// 验证停用补偿能够重新进入唯一允许启用探测的状态。
#[test]
fn draining_rollback_reenters_enabling_before_probe() {
    // temp owns the isolated plugin package and fixed bridge runtime.
    // temp 持有隔离的插件包与固定桥接运行时。
    let temp = tempfile::tempdir().expect("create draining rollback directory");
    // runtime_root is the deterministic root for every test-owned runtime asset.
    // runtime_root 是所有测试自有运行时资产的确定性根目录。
    let runtime_root = temp.path().join("runtime");
    // vulcan_plugin_root contains the enabled hook fixture selected by the transition.
    // vulcan_plugin_root 包含转换选择的已启用 Hook 测试插件。
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_hook_package(&vulcan_plugin_root);
    // manager owns the exact Enabled -> Draining -> Enabling sequence under test.
    // manager 持有本测试的精确 Enabled -> Draining -> Enabling 序列。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load enabled lifecycle fixture");
    manager
        .begin_draining("hook-plugin")
        .expect("begin draining transition");
    manager
        .rollback_draining_to_enabling("hook-plugin")
        .expect("re-enter enabling transition");
    assert_eq!(
        manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    // runtime proves the enabling probe lease is accepted after the reverse transition.
    // runtime 证明反向转换后允许创建启用探测租约。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create lifecycle Tokio runtime");
    runtime
        .block_on(manager.probe_plugin("hook-plugin"))
        .expect("probe plugin while re-entering enabling");
}

/// Verifies same-version package edits advance the lifecycle generation on refresh.
/// 验证同版本插件编辑会在刷新时推进生命周期代次。
#[test]
fn same_version_package_change_advances_plugin_generation() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_hook_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root.clone());
    let first = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load first catalog");
    let first_generation = first.plugins["hook-plugin"].generation;
    fs::write(
        vulcan_plugin_root.join("hook-plugin/dependencies.yaml"),
        "{}\n# changed without version bump\n",
    )
    .expect("update package content");
    let second = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load second catalog");
    assert!(second.plugins["hook-plugin"].generation > first_generation);
}

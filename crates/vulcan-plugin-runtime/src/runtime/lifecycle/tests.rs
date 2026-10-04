//! Regression tests for plugin lifecycle compensation and package generations.
//! 插件生命周期补偿与包代次的回归测试。

use super::super::tests::{test_manager, write_hook_package};
use super::{VulcanPluginLifecycleEvent, VulcanPluginRuntimeState};
use std::collections::BTreeSet;
use std::fs;

mod approval;

/// Physical replacement and reactivation invalidate routing even when version and bytes are identical.
/// 即使版本及字节相同，物理替换及重新激活也会使旧路由失效。
#[test]
fn managed_catalog_changes_invalidate_old_routing_generations() {
    // Own the actual manager and all immutable package generations.
    // 持有实际管理器及所有不可变插件包代次。
    let temp = tempfile::tempdir().expect("create managed routing fixture");
    // Production manager fixture supplies a real engine and independent user root.
    // 生产管理器夹具提供实际引擎及独立用户根。
    let runtime_root = temp.path().join("runtime");
    // An empty bundled root leaves the managed user identity visible.
    // 空内置根使受管用户身份可见。
    let bundled = runtime_root.join("plugins");
    fs::create_dir_all(&bundled).expect("create routing bundled root");
    // No installation callback is involved in selecting this explicit test generation.
    // 选择此显式测试代次不涉及安装回调。
    let manager = test_manager(&runtime_root, bundled);
    // Source inspection, preparation, and activation use production APIs.
    // 来源检查、准备及激活使用生产接口。
    let source = temp.path().join("source/local-plugin");
    super::super::tests::write_user_plugin_package(&source, "local-plugin");
    // Freeze the source digest shared by both physically distinct generations.
    // 固定两个物理不同代次共用的来源摘要。
    let inspected = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect managed source");
    // Store uses the same configured root as the manager catalog.
    // 存储使用与管理器目录相同的配置根。
    let store = crate::VulcanPluginGenerationStore::open(&runtime_root.join("user_plugins"))
        .expect("open routing store");
    // The original generation and its routing id establish the stale-handle baseline.
    // 原代次及其路由标识建立陈旧句柄基线。
    let first = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("prepare original routing package");
    store
        .activate(&first, 0)
        .expect("activate original routing package");
    // Route ids are read from the real manager projection, not constructed by the fixture.
    // 路由标识取自实际管理器投影，而非夹具构造。
    let original = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project original routing");
    assert_eq!(
        original.plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
    manager
        .authorize_user_plugin("local-plugin", inspected.permissions.clone())
        .expect("authorize exact selected content");
    assert_eq!(
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("project authorized selection")
            .plugins["local-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    // A new UUID must advance routing even though all file bytes remain identical.
    // 所有文件字节相同，新 UUID 仍必须推进路由。
    let second = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("prepare identical replacement");
    store
        .activate(&second, 1)
        .expect("activate identical replacement");
    // Physical identity is distinct from content authorization identity.
    // 物理身份不同于内容授权身份。
    let replaced = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project replacement routing");
    assert!(
        replaced.plugins["local-plugin"].generation > original.plugins["local-plugin"].generation
    );
    assert_eq!(
        replaced.plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    store
        .activate(&second, 2)
        .expect("publish a new activation of identical physical generation");
    // Reusing a physical generation still creates a new routing incarnation.
    // 复用物理代次仍会创建新路由实例。
    let reactivated = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project reactivated routing");
    assert!(
        reactivated.plugins["local-plugin"].generation
            > replaced.plugins["local-plugin"].generation
    );
    store
        .deactivate("local-plugin", 3)
        .expect("remove managed selection");
    assert!(
        !manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("project tombstone")
            .plugins
            .contains_key("local-plugin")
    );
    store
        .activate(&second, 4)
        .expect("reactivate after catalog absence");
    // An absent intermediate snapshot must not reset the process-local routing identity.
    // 中间快照缺失该插件也不能重置进程内路由身份。
    let returned = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project returned routing");
    assert!(
        returned.plugins["local-plugin"].generation
            > reactivated.plugins["local-plugin"].generation
    );
    // Changed bytes cannot inherit authorization merely because the semantic version is unchanged.
    // 即使语义版本未改变，字节改变后也不能继承授权。
    let authorization_path = runtime_root.join("plugin_state/local-plugin/authorization.json");
    // Preserve evidence that refresh never rewrites the user's prior authorization.
    // 保存刷新绝不改写用户先前授权的证据。
    let authorization = fs::read(&authorization_path).expect("read original authorization");
    fs::write(source.join("changed.txt"), b"new code identity")
        .expect("change same-version content");
    // Re-inspect actual new bytes before preparing their immutable generation.
    // 准备不可变代次之前重新检查实际新字节。
    let changed_preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect changed source");
    // The new generation has the same version but requires new content authorization.
    // 新代次版本相同，但需要新的内容授权。
    let changed = store
        .prepare(&source, &changed_preview.content_fingerprint)
        .expect("prepare changed package");
    store.activate(&changed, 5).expect("select changed package");
    assert_eq!(
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("project unauthorized changed package")
            .plugins["local-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
    assert_eq!(
        fs::read(&authorization_path).expect("read preserved authorization"),
        authorization
    );
    // Exhaustion is a hard error instead of reusing the last routing id for a new activation.
    // 耗尽是明确错误，不能把最后一个路由标识复用于新激活。
    manager
        .state
        .plugin_generations
        .lock()
        .expect("lock routing fixture counter")
        .insert("local-plugin".into(), u64::MAX);
    store
        .activate(&changed, 6)
        .expect("publish selection at routing exhaustion");
    assert!(
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect_err("reject routing counter exhaustion")
            .to_string()
            .contains("routing generation is exhausted")
    );
}

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
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal lifecycle parent");
    // runtime executes the asynchronous compensation lease deterministically.
    // runtime 确定性执行异步补偿租约。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
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
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal probe parent");
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
    // runtime proves formal initialization is admitted after the reverse transition.
    // runtime 证明反向转换后允许正式初始化入场。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
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

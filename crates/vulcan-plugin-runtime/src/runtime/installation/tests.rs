//! User-plugin package transaction regressions with exact authorization paths.
//! 使用精确授权路径的用户插件包事务回归。

use super::super::tests::{test_manager, write_user_plugin_package};
use crate::VulcanPluginPackageOrigin;
use std::collections::BTreeSet;
use std::fs;

/// Legacy installation rejects both selected and removed managed namespaces without writing a hidden package.
/// 旧安装流程拒绝选中及已移除的受管命名空间，不写入隐藏包。
#[test]
fn managed_catalog_installation_rejects_legacy_publication_behind_managed_records() {
    // Use the real manager installation API and a separate immutable storage root.
    // 使用实际管理器安装接口及独立不可变存储根。
    let temp = tempfile::tempdir().expect("create legacy installation guard fixture");
    // Manager paths match the production fixture conventions.
    // 管理器路径匹配生产夹具约定。
    let runtime_root = temp.path().join("runtime");
    // Empty bundled root leaves the test identity user-owned.
    // 空内置根使测试身份归用户所有。
    let bundled = runtime_root.join("plugins");
    fs::create_dir_all(&bundled).expect("create legacy guard bundled root");
    // All invocation and state validation use the actual manager.
    // 全部调用及状态校验使用实际管理器。
    let manager = test_manager(&runtime_root, bundled);
    // Inspected source remains outside the installed user root.
    // 已检查来源始终位于已安装用户根之外。
    let source = temp.path().join("source/local-plugin");
    write_user_plugin_package(&source, "local-plugin");
    // Permission and content identities are frozen before both attempted installations.
    // 两次安装尝试之前固定权限及内容身份。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect legacy guard source");
    // Production generation store publishes exact records without creating legacy state files.
    // 生产代次存储发布精确记录，不创建旧状态文件。
    let store = crate::VulcanPluginGenerationStore::open(&runtime_root.join("user_plugins"))
        .expect("open legacy guard store");
    // Keep the selected physical generation available across both rejection paths.
    // 在两个拒绝路径之间保持选中的物理代次可用。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("prepare legacy guard generation");
    store
        .activate_for_test(&generation, 0, generation.required_permissions_for_test())
        .expect("select managed generation");
    for removed in [false, true] {
        if removed {
            store
                .deactivate("local-plugin", 1)
                .expect("publish managed tombstone");
        }
        // The old write path must fail before creating a destination or authorization/install records.
        // 旧写入路径必须在创建目标或授权及安装记录之前失败。
        let error = manager
            .install_local_user_plugin(
                &source,
                &preview.content_fingerprint,
                preview.permissions.clone(),
                false,
            )
            .expect_err("reject legacy install into managed namespace");
        assert!(error.to_string().contains("has managed generation state"));
        assert!(!runtime_root.join("user_plugins/local-plugin").exists());
        assert!(!runtime_root.join("plugin_state/local-plugin").exists());
    }
}

/// Verifies installation rejects a package whose contents changed after authorization inspection.
/// 验证安装会拒绝在授权检查后内容发生变化的插件包。
#[test]
fn user_plugin_install_binds_authorization_to_exact_content_fingerprint() {
    // temp owns the isolated source package and all runtime state for this authorization test.
    // temp 持有当前授权测试的隔离来源包与全部运行时状态。
    let temp = tempfile::tempdir().expect("create installation authorization directory");
    // runtime_root is the deterministic state root used to detect forbidden publication.
    // runtime_root 是用于检测违规发布的确定性状态根目录。
    let runtime_root = temp.path().join("runtime");
    // vulcan_plugin_root provides an empty bundled-plugin source for the manager.
    // vulcan_plugin_root 为管理器提供空的内置插件来源。
    let vulcan_plugin_root = runtime_root.join("plugins");
    // source_package is the user-selected package whose inspected contents are later changed.
    // source_package 是用户选择并在检查后被修改内容的插件包。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manager owns the exact catalog, authorization, and installation transaction state.
    // manager 持有精确目录、授权与安装事务状态。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial plugin catalog");
    // preview captures the package identity the user actually reviewed.
    // preview 捕获用户实际复核的插件包身份。
    let preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect installation candidate");
    fs::write(
        source_package.join("runtime/plugin.bundle.lua"),
        "return { changed_after_inspection = true }\n",
    )
    .expect("change installation candidate after inspection");

    // error proves the stale authorization cannot publish a different package generation.
    // error 证明过期授权无法发布不同的插件包代次。
    let error = manager
        .install_local_user_plugin(
            &source_package,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect_err("reject changed installation candidate");
    assert!(
        error
            .to_string()
            .contains("changed after authorization inspection")
    );
    assert!(!runtime_root.join("user_plugins/local-plugin").exists());
    assert!(
        !runtime_root
            .join("plugin_state/local-plugin/authorization.json")
            .exists()
    );
}

/// Verifies local package installation, private environment publication, and uninstall are closed transactions.
/// 验证本地插件包安装、私有环境发布与卸载构成闭环事务。
#[test]
fn user_plugin_install_environment_and_uninstall_are_transactional() {
    let temp = tempfile::tempdir().expect("create user plugin transaction directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial plugin catalog");
    let preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect local user plugin");
    assert_eq!(preview.plugin_id, "local-plugin");
    assert!(preview.permissions.is_empty());
    let installation = manager
        .install_local_user_plugin(
            &source_package,
            &preview.content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("install local user plugin");
    assert_eq!(installation.plugin_id, "local-plugin");
    // The real record must exist before its removal is checked, preventing a vacuous cleanup assertion.
    // 检查移除之前真实记录必须存在，避免清理断言空通过。
    assert!(
        runtime_root
            .join("plugin_state/local-plugin/authorization.json")
            .is_file()
    );
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    let catalog = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load installed user plugin");
    assert_eq!(
        catalog.plugins["local-plugin"].origin,
        VulcanPluginPackageOrigin::UserInstalled
    );
    let environment = manager
        .prepare_plugin_environment("local-plugin")
        .expect("prepare private plugin environment");
    assert!(
        environment
            .join(".vulcan-plugin-environment.json")
            .is_file()
    );
    // A prepared resource owner must block uninstall before package and state mutations begin.
    // 准备资源所有者必须在插件包及状态变更开始前阻止卸载。
    let environment_owner = crate::runtime::environment::retain_plugin_execution_environment(
        &manager.state,
        "local-plugin",
    )
    .expect("retain execution environment");
    manager
        .uninstall_user_plugin("local-plugin")
        .expect_err("live environment blocks uninstall");
    assert!(runtime_root.join("user_plugins/local-plugin").is_dir());
    assert!(
        runtime_root
            .join("plugin_state/local-plugin/authorization.json")
            .is_file()
    );
    assert!(
        environment
            .join(".vulcan-plugin-environment.json")
            .is_file()
    );
    drop(environment_owner);
    manager
        .uninstall_user_plugin("local-plugin")
        .expect("uninstall local user plugin");
    assert!(!runtime_root.join("user_plugins/local-plugin").exists());
    assert!(!runtime_root.join("plugin_runtime/local-plugin").exists());
    assert!(
        !runtime_root
            .join("plugin_state/local-plugin/authorization.json")
            .exists()
    );
}

//! User-plugin package transaction regressions with exact authorization paths.
//! 使用精确授权路径的用户插件包事务回归。

use super::super::tests::{test_manager, write_user_plugin_package};
use crate::VulcanPluginPackageOrigin;
use std::collections::BTreeSet;
use std::fs;

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

/// Verifies update authorization cannot be reused after the inspected source package changes.
/// 验证检查后的来源包发生变化时不能复用更新授权。
#[test]
fn user_plugin_update_binds_authorization_to_exact_content_fingerprint() {
    // temp owns the installed package, mutable update source, and transaction state.
    // temp 持有已安装包、可变更新来源与事务状态。
    let temp = tempfile::tempdir().expect("create update authorization directory");
    // runtime_root is the deterministic root containing the committed version-one package.
    // runtime_root 是包含已提交 1.0.0 插件包的确定性根目录。
    let runtime_root = temp.path().join("runtime");
    // vulcan_plugin_root provides an empty bundled-plugin source for the manager.
    // vulcan_plugin_root 为管理器提供空的内置插件来源。
    let vulcan_plugin_root = runtime_root.join("plugins");
    // source_package is reused as the independently inspected update candidate.
    // source_package 被复用为独立检查的更新候选包。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manager owns the exact installed generation and update gate.
    // manager 持有精确已安装代次与更新门禁。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial plugin catalog");
    // initial_preview binds the first installation to its exact source contents.
    // initial_preview 将首次安装绑定到精确来源内容。
    let initial_preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect initial plugin package");
    manager
        .install_local_user_plugin(
            &source_package,
            &initial_preview.content_fingerprint,
            initial_preview.permissions,
            false,
        )
        .expect("install initial plugin package");
    // disabled is the exact policy state required before an update transaction may begin.
    // disabled 是更新事务开始前要求的精确策略状态。
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled initial plugin");
    // manifest_path selects the sole version declaration changed for the candidate package.
    // manifest_path 选择候选包中唯一被修改的版本声明。
    let manifest_path = source_package.join("plugin.yaml");
    // version_two_manifest advances the candidate without changing its capability set.
    // version_two_manifest 在不改变能力集合的情况下推进候选版本。
    let version_two_manifest = fs::read_to_string(&manifest_path)
        .expect("read initial source manifest")
        .replace("version: 1.0.0", "version: 2.0.0");
    fs::write(&manifest_path, version_two_manifest).expect("write update candidate manifest");
    // update_preview captures the exact version-two package reviewed by the user.
    // update_preview 捕获用户复核的精确 2.0.0 插件包。
    let update_preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect update candidate");
    fs::write(
        source_package.join("runtime/plugin.bundle.lua"),
        "return { replaced_after_update_inspection = true }\n",
    )
    .expect("change update candidate after inspection");

    // error proves the installed package is untouched when the authorization fingerprint is stale.
    // error 证明授权指纹过期时已安装插件包保持不变。
    let error = manager
        .begin_local_user_plugin_update(
            &source_package,
            &update_preview.content_fingerprint,
            update_preview.permissions,
            false,
        )
        .expect_err("reject changed update candidate");
    assert!(
        error
            .to_string()
            .contains("changed after update authorization inspection")
    );
    let catalog = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("reload unchanged installed plugin");
    assert_eq!(
        catalog.plugins["local-plugin"].version.as_deref(),
        Some("1.0.0")
    );
    assert!(
        fs::read_dir(runtime_root.join("user_plugins"))
            .expect("list user plugin root")
            .all(|entry| !entry
                .expect("read user plugin entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".update-"))
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

/// Verifies user-plugin updates retain the previous package until commit and restore it exactly on rollback.
/// 验证用户插件更新会保留旧插件包直到提交，并在回滚时精确恢复旧包。
#[test]
fn user_plugin_update_supports_exact_rollback_and_commit() {
    // temp owns package, state, and private environment generations for both update outcomes.
    // temp 持有两种更新结果使用的插件包、状态与私有环境代次。
    let temp = tempfile::tempdir().expect("create user plugin update directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial plugin catalog");
    manager
        .install_local_user_plugin(
            &source_package,
            &manager
                .inspect_local_user_plugin(&source_package)
                .expect("inspect initial user plugin")
                .content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("install initial user plugin");
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled initial plugin");
    let previous_environment = manager
        .prepare_plugin_environment("local-plugin")
        .expect("prepare previous plugin environment");

    // version_two_manifest changes both the semantic version and package fingerprint.
    // version_two_manifest 同时变更语义版本与插件包指纹。
    let manifest_path = source_package.join("plugin.yaml");
    let version_two_manifest = fs::read_to_string(&manifest_path)
        .expect("read source manifest")
        .replace("version: 1.0.0", "version: 2.0.0");
    fs::write(&manifest_path, version_two_manifest).expect("write version two manifest");
    let version_two_preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect version two user plugin");

    let rollback_transaction = manager
        .begin_local_user_plugin_update(
            &source_package,
            &version_two_preview.content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("begin rollback update candidate");
    assert_eq!(rollback_transaction.plugin_id(), "local-plugin");
    assert_eq!(rollback_transaction.permission_preview().version, "2.0.0");
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load rollback candidate catalog");
    let rolled_back_environment = manager
        .prepare_plugin_environment("local-plugin")
        .expect("prepare rollback candidate environment");
    manager
        .verify_local_user_plugin_update_candidate(&rollback_transaction)
        .expect("verify unchanged rollback candidate");
    fs::write(
        rollback_transaction
            .destination
            .join("tampered-after-validation.txt"),
        b"changed",
    )
    .expect("mutate rollback candidate after preparation");
    assert!(
        manager
            .verify_local_user_plugin_update_candidate(&rollback_transaction)
            .is_err(),
        "candidate mutation must be rejected before extension publication"
    );
    manager
        .rollback_local_user_plugin_update(rollback_transaction)
        .expect("rollback candidate package");
    let restored_catalog = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("reload restored plugin catalog");
    assert_eq!(
        restored_catalog.plugins["local-plugin"].version.as_deref(),
        Some("1.0.0")
    );
    assert!(previous_environment.exists());
    assert!(!rolled_back_environment.exists());

    let commit_transaction = manager
        .begin_local_user_plugin_update(
            &source_package,
            &version_two_preview.content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("begin committed update candidate");
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load committed candidate catalog");
    let committed_environment = manager
        .prepare_plugin_environment("local-plugin")
        .expect("prepare committed candidate environment");
    manager
        .verify_local_user_plugin_update_candidate(&commit_transaction)
        .expect("verify committed candidate before publication");
    manager
        .commit_local_user_plugin_update(commit_transaction)
        .expect("commit candidate package");
    let committed_catalog = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("reload committed plugin catalog");
    assert_eq!(
        committed_catalog.plugins["local-plugin"].version.as_deref(),
        Some("2.0.0")
    );
    assert!(committed_environment.exists());
    assert!(!previous_environment.exists());
    assert!(
        fs::read_dir(runtime_root.join("user_plugins"))
            .expect("list user plugin root")
            .all(|entry| !entry
                .expect("read user plugin entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".update-"))
    );
}

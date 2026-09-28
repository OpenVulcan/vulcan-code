//! User-plugin package transaction regressions with exact authorization paths.
//! 使用精确授权路径的用户插件包事务回归。

use super::super::tests::{test_manager, write_user_plugin_package};
use crate::VulcanPluginPackageOrigin;
use std::collections::BTreeSet;
use std::fs;

/// A failed public rollback retains original state after consuming its only live transaction handle.
/// 公开回滚消费唯一存活事务句柄后失败，仍保留原始状态。
#[test]
fn user_plugin_failed_rollback_keeps_disk_snapshots_after_handle_consumption() {
    // Exercise package-identity rejection and state-write failure through the same public API.
    // 经同一公开接口验证插件包身份拒绝及状态写入失败。
    for change_original in [false, true] {
        // Isolate the real manager, package copies, authorization, and installation records.
        // 隔离真实管理器、插件包副本、授权及安装记录。
        let temp = tempfile::tempdir().expect("create failed rollback directory");
        // All installed and recovery files remain under this exact fixture root.
        // 所有安装及恢复文件均位于此精确夹具根目录内。
        let runtime_root = temp.path().join("runtime");
        // The empty bundled root ensures the package uses the actual user-installation path.
        // 空内置根目录确保插件使用实际用户安装路径。
        let bundled_root = runtime_root.join("plugins");
        fs::create_dir_all(&bundled_root).expect("create bundled root");
        // Keep the update source independent of the installed package.
        // 保持更新来源与已安装插件包独立。
        let source = temp.path().join("source/local-plugin");
        write_user_plugin_package(&source, "local-plugin");
        // The manager executes production installation and rollback functions.
        // 管理器执行生产安装与回滚函数。
        let manager = test_manager(&runtime_root, bundled_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial catalog");
        // Pin the first authorized content identity before installation.
        // 安装前固定第一次授权的内容身份。
        let initial = manager
            .inspect_local_user_plugin(&source)
            .expect("inspect original package");
        manager
            .install_local_user_plugin(
                &source,
                &initial.content_fingerprint,
                initial.permissions,
                false,
            )
            .expect("install original package");
        manager
            .refresh_catalog(&BTreeSet::from(["local-plugin".into()]), &BTreeSet::new())
            .expect("load disabled original package");
        fs::write(source.join("update-version.txt"), b"candidate generation")
            .expect("write changed candidate");
        // Pin the second authorized identity before the real update begins.
        // 实际更新开始前固定第二次授权的身份。
        let candidate = manager
            .inspect_local_user_plugin(&source)
            .expect("inspect changed candidate");
        // This is the sole live transaction handle; the public rollback will consume it even on failure.
        // 此为唯一存活事务句柄；公开回滚即使失败也将其消费。
        let transaction = manager
            .begin_local_user_plugin_update(
                &source,
                &candidate.content_fingerprint,
                candidate.permissions,
                false,
            )
            .expect("begin real update");
        // Retain only verification paths and expected bytes, not an operational transaction handle.
        // 仅保留验证路径及预期字节，不保留可操作事务句柄。
        let recovery_root = transaction.transaction_root.clone();
        // Compare the original byte arrays after all production transaction ownership has been consumed.
        // 生产事务所有权全部消费后比较原始字节数组。
        let expected_authorization = transaction.previous_authorization.clone();
        // Installation restoration must independently retain the original exact record.
        // 安装记录恢复必须独立保留原始精确记录。
        let expected_installation = transaction.previous_installation.clone();
        if change_original {
            fs::write(
                recovery_root.join("previous/foreign-change.txt"),
                b"changed original",
            )
            .expect("alter retained original package");
        } else {
            // A directory at the authorization file path creates a real atomic-write rejection.
            // 在授权文件路径放置目录，造成真实原子写入拒绝。
            let authorization_path =
                runtime_root.join("plugin_state/local-plugin/authorization.json");
            fs::remove_file(&authorization_path).expect("remove authorization fixture file");
            fs::create_dir(&authorization_path).expect("create authorization obstruction");
        }
        // The consumed handle must still leave exact evidence for either failure stage.
        // 句柄被消费后，两种失败阶段均必须留下精确证据。
        let error = manager
            .rollback_local_user_plugin_update(transaction)
            .expect_err("reject real rollback conflict");
        if change_original {
            assert!(
                error
                    .to_string()
                    .contains("differs from the original installed contents")
            );
        } else {
            assert!(error.to_string().contains("not a regular file"));
        }
        drop(manager);

        // Reload recovery evidence from disk after dropping the entire manager.
        // 整个管理器被丢弃后，从磁盘重新读取恢复证据。
        let records: serde_json::Value = serde_json::from_slice(
            &fs::read(recovery_root.join(super::rollback::ROLLBACK_RECORDS_FILE))
                .expect("read retained rollback records"),
        )
        .expect("decode retained rollback records");
        assert_eq!(
            records["authorization"],
            serde_json::json!(expected_authorization)
        );
        assert_eq!(
            records["installation"],
            serde_json::json!(expected_installation)
        );
        if change_original {
            assert!(
                runtime_root
                    .join("user_plugins/local-plugin/update-version.txt")
                    .is_file()
            );
            assert_eq!(
                fs::read(recovery_root.join("previous/foreign-change.txt"))
                    .expect("read altered original"),
                b"changed original"
            );
            assert!(!recovery_root.join("failed-candidate").exists());
        } else {
            assert!(
                !runtime_root
                    .join("user_plugins/local-plugin/update-version.txt")
                    .exists()
            );
            assert!(
                runtime_root
                    .join("user_plugins/local-plugin/plugin.yaml")
                    .is_file()
            );
            assert_eq!(
                fs::read(recovery_root.join("failed-candidate/update-version.txt"))
                    .expect("read retained failed candidate"),
                b"candidate generation"
            );
        }
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
    // Verify the real begin-update entry persisted both original records before returning its handle.
    // 验证真实更新开始入口在返回句柄前已持久保存两份原始记录。
    let rollback_records: serde_json::Value = serde_json::from_slice(
        &fs::read(
            rollback_transaction
                .transaction_root
                .join(super::rollback::ROLLBACK_RECORDS_FILE),
        )
        .expect("read original rollback records from disk"),
    )
    .expect("decode original rollback records");
    assert_eq!(rollback_records["plugin_id"], "local-plugin");
    assert_eq!(rollback_records["schema_version"], 1);
    assert_eq!(
        rollback_records["authorization"],
        serde_json::json!(rollback_transaction.previous_authorization)
    );
    assert_eq!(
        rollback_records["installation"],
        serde_json::json!(rollback_transaction.previous_installation)
    );
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

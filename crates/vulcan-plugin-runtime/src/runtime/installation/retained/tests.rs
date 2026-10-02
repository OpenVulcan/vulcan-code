//! Real original package and file ownership regressions cover partial copies and changed records.
//! 真实原插件包及文件所有权回归覆盖部分复制及记录变更。

use super::*;
use crate::runtime::tests::{test_manager, write_user_plugin_package};
use std::collections::BTreeSet;

/// Create an actual isolated manager and approved source without publishing a plugin package.
/// 创建实际隔离管理器及已批准来源，不发布插件包。
/// root owns every fixture path; returns the manager and its sole exact source path.
/// root 拥有每个夹具路径；返回管理器及其唯一精确来源路径。
fn fixture(root: &Path) -> (VulcanPluginRuntimeManager, PathBuf) {
    // The configured manager uses production user and state directory conventions.
    // 已配置管理器使用生产用户及状态目录约定。
    let runtime_root = root.join("runtime");
    // A real empty bundled root leaves this identity user-owned.
    // 真实空内置根使此身份归用户所有。
    let bundled = runtime_root.join("plugins");
    fs::create_dir_all(&bundled).expect("create original install bundled root");
    // This actual manager performs every package and namespace operation.
    // 此实际管理器执行每个插件包及命名空间操作。
    let manager = test_manager(&runtime_root, bundled);
    // The source is independent from the fixed user destination.
    // 来源独立于固定用户目标。
    let source = root.join("source/local-plugin");
    write_user_plugin_package(&source, "local-plugin");
    (manager, source)
}

/// An actual changed-source failure keeps the original lock, refuses replay and permits borrowed partial-copy cleanup.
/// 实际来源变更失败保留原锁、拒绝重放，并允许借用式部分副本清理。
#[test]
fn initial_install_original_partial_copy_retains_actual_namespace_until_drop() {
    // Every actual disk and lock operation is isolated below this temporary directory.
    // 每个实际磁盘及锁操作均隔离于此临时目录下。
    let root = tempfile::tempdir().expect("original partial copy fixture");
    // Actual production inspection binds the immutable approval before reservation.
    // 实际生产检查在预留之前绑定不可变批准。
    let (runtime, source) = fixture(root.path());
    // Complete permissions and fingerprint come from the actual source.
    // 完整权限及指纹来自实际来源。
    let preview = runtime
        .inspect_local_user_plugin(&source)
        .expect("original copy preview");
    // This is the same concrete retained original before and after failure.
    // 此为失败之前及之后同一具体保留原实例。
    let mut original = runtime
        .reserve_local_user_plugin_installation(
            &source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("reserve original copy");
    // An independent actual Store uses its own physical namespace-lock attempt.
    // 独立实际存储使用自身物理命名空间锁尝试。
    let contender = crate::VulcanPluginGenerationStore::open(runtime.user_plugin_root())
        .expect("independent original contender");
    assert!(!original.destination.exists() && !original.staging_root.exists());
    assert_eq!(
        contender
            .acquire_legacy_mutation_guard("local-plugin", None)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("actual original fence")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
    fs::write(
        source.join("changed.txt"),
        "changed after exact authorization",
    )
    .expect("real changed source");
    assert!(
        runtime
            .publish_local_user_plugin_installation(&mut original)
            .expect_err("reject changed source after actual copy")
            .to_string()
            .contains("changed while")
    );
    assert!(original.staging_root.exists() && !original.destination.exists());
    assert!(
        runtime
            .publish_local_user_plugin_installation(&mut original)
            .expect_err("never replay original copy")
            .to_string()
            .contains("cannot be replayed")
    );
    runtime
        .abort_local_user_plugin_installation(&mut original)
        .expect("actual borrowed partial cleanup");
    assert!(!original.staging_root.exists() && !original.destination.exists());
    assert_eq!(
        contender
            .acquire_legacy_mutation_guard("local-plugin", None)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("actual guard survives terminal observation")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
    drop(original);
    contender
        .acquire_legacy_mutation_guard("local-plugin", None)
        .expect("actual original drop releases its file");
}

/// Changed original records reject destructive cleanup, keeping the same owner available after the real conflict is resolved.
/// 原记录变更拒绝破坏性清理，在真实冲突解决之后使同一所有者仍可用。
#[test]
fn initial_install_original_record_conflict_keeps_borrowed_recovery_and_guarded_catalog() {
    // Isolated real package and manager paths carry no mock storage.
    // 隔离真实插件包及管理器路径不包含模拟存储。
    let root = tempfile::tempdir().expect("original record conflict fixture");
    // Source approval is obtained through the actual public inspection API.
    // 来源批准通过实际公开检查接口取得。
    let (runtime, source) = fixture(root.path());
    // Actual preview fixes both the digest and complete capability set.
    // 实际预览固定摘要及完整能力集合。
    let preview = runtime
        .inspect_local_user_plugin(&source)
        .expect("original record preview");
    // The original owns publication, conflict and retry without a consuming transfer.
    // 原实例拥有发布、冲突及重试，不进行消费式移交。
    let mut original = runtime
        .reserve_local_user_plugin_installation(
            &source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("reserve original records");
    runtime
        .publish_local_user_plugin_installation(&mut original)
        .expect("publish actual original records");
    // The real catalog reader borrows this actual original file instead of reacquiring it.
    // 真实目录读取器借用此实际原文件，而非重新获取它。
    let catalog = runtime
        .refresh_catalog_with_legacy_mutation(
            &BTreeSet::from(["local-plugin".into()]),
            &BTreeSet::new(),
            "local-plugin",
            original.mutation_guard(),
        )
        .expect("guarded original catalog");
    assert_eq!(
        catalog
            .plugins
            .get("local-plugin")
            .expect("actual original disabled descriptor")
            .runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    // The exact published installation path and bytes are retained before a foreign modification.
    // 精确已发布安装路径及字节在外部修改之前保留。
    let path = plugin_installation_state_path(&runtime.state.plugin_state_root, "local-plugin");
    // This byte array is evidence only; it is not a replacement transaction authority.
    // 此字节数组仅为证据；不是替代事务权威。
    let expected = fs::read(&path).expect("actual original installation bytes");
    fs::write(&path, b"changed external record").expect("real record conflict");
    assert!(
        runtime
            .abort_local_user_plugin_installation(&mut original)
            .expect_err("reject foreign record removal")
            .to_string()
            .contains("record changed")
    );
    assert!(original.destination.exists());
    assert_eq!(
        fs::read(&path).expect("preserved changed record"),
        b"changed external record"
    );
    fs::write(&path, expected).expect("resolve real original record conflict");
    runtime
        .abort_local_user_plugin_installation(&mut original)
        .expect("same borrowed original retry");
    assert!(!original.destination.exists() && !path.exists());
}

/// Actual immutable update reservation and legacy uninstall reject a retained installation before package mutation.
/// 实际不可变更新预留及旧卸载在插件包变更前拒绝保留安装。
#[test]
fn initial_install_original_excludes_other_flat_mutation_before_effects() {
    // This actual fixture owns all original package, state and namespace files.
    // 此实际夹具拥有全部原插件包、状态及命名空间文件。
    let root = tempfile::tempdir().expect("original install competing mutation fixture");
    // The actual manager performs every tested production entry point.
    // 实际管理器执行每个被测试生产入口。
    let (runtime, source) = fixture(root.path());
    // The initial immutable approval predates all original writes.
    // 初始不可变批准早于全部原写入。
    let preview = runtime
        .inspect_local_user_plugin(&source)
        .expect("initial exact approval");
    // Keep the same original owner through both attempted competing operations.
    // 在两次竞争操作尝试期间保持相同原所有者。
    let mut original = runtime
        .reserve_local_user_plugin_installation(
            &source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("reserve initial namespace owner");
    runtime
        .publish_local_user_plugin_installation(&mut original)
        .expect("original package publication");
    runtime
        .refresh_catalog_with_legacy_mutation(
            &BTreeSet::from(["local-plugin".into()]),
            &BTreeSet::new(),
            "local-plugin",
            original.mutation_guard(),
        )
        .expect("actual private disabled catalog");
    // Exact original record bytes prove that neither failed competing entry mutates installed state.
    // 精确原记录字节证明两次失败竞争入口均未修改安装状态。
    let installation = fs::read(plugin_installation_state_path(
        &runtime.state.plugin_state_root,
        "local-plugin",
    ))
    .expect("original installation image");
    // A valid distinct candidate reaches the current immutable reservation's actual namespace fence.
    // 有效不同候选抵达现行不可变预留的实际命名空间屏障。
    fs::write(source.join("next-version.txt"), b"actual next candidate")
        .expect("change independent candidate");
    let next = runtime
        .inspect_local_user_plugin(&source)
        .expect("actual next approval");
    // The sole store defines the actual Busy diagnostic; competing public entries must propagate this same rejection.
    // 唯一存储定义实际忙碌诊断；竞争公共入口必须传递此相同拒绝。
    let conflict = crate::VulcanPluginGenerationStore::open(runtime.user_plugin_root())
        .expect("independent actual conflicting store")
        .acquire_legacy_mutation_guard("local-plugin", None)
        // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
        // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
        .map(|_| ())
        .expect_err("original actual namespace stays locked");
    assert_eq!(conflict.code, crate::VulcanPluginStoreErrorCode::Busy);
    assert_eq!(
        runtime
            .uninstall_user_plugin("local-plugin")
            .expect_err("original fence excludes uninstall")
            .to_string(),
        conflict.to_string()
    );
    // Freeze policy from the actual disabled catalog and the original manifest baseline.
    // 按实际停用目录及原清单基线固定策略。
    let policy = crate::VulcanPluginUpdatePolicy {
        explicitly_enabled: false,
        explicitly_disabled: true,
        predecessor_default_enabled: runtime
            .snapshot()
            .plugins
            .get("local-plugin")
            .expect("original disabled descriptor")
            .manifest
            .as_ref()
            .expect("original manifest")
            .default_enabled,
    };
    // Invoke the actual production update reservation, retaining no substitute update authority.
    // 调用实际生产更新预留，不保留替代更新权威。
    let update_conflict = crate::VulcanPluginGenerationStore::open(runtime.user_plugin_root())
        .expect("actual immutable contender")
        .reserve_activation_update_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &next.content_fingerprint,
            None,
            next.permissions,
            policy,
        )
        // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
        // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
        .map(|_| ())
        .expect_err("original fence excludes actual immutable reservation");
    assert_eq!(
        update_conflict.code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(update_conflict.to_string(), conflict.to_string());
    assert_eq!(
        crate::catalog::compute_package_fingerprint(&original.destination)
            .expect("unchanged original package"),
        preview.content_fingerprint
    );
    assert_eq!(
        fs::read(plugin_installation_state_path(
            &runtime.state.plugin_state_root,
            "local-plugin"
        ))
        .expect("unchanged actual record"),
        installation
    );
    assert!(
        fs::read_dir(runtime.user_plugin_root())
            .expect("actual untouched namespace")
            .all(|entry| {
                // Match semantic private-directory names without depending on ordering or numeric indexes.
                // 匹配具有语义的私有目录名，不依赖顺序或数字下标。
                let name = entry.expect("actual namespace entry").file_name();
                !name.to_string_lossy().starts_with(".update-")
                    && !name.to_string_lossy().starts_with(".uninstall-")
            })
    );
    runtime
        .abort_local_user_plugin_installation(&mut original)
        .expect("same original recovery after rejected competitors");
}

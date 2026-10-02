//! Actual original flat package and state ownership tests never substitute for service Full publication acceptance.
//! 实际原平铺包及状态所有权测试绝不替代服务完整发布验收。

use super::*;
use crate::runtime::tests::{test_manager, write_user_plugin_package};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Create and approve a real flat package with the existing producer, then project its actual disabled descriptor.
/// 使用既有生产者创建及批准真实平铺包，然后投影实际停用描述符。
/// root owns every fixture file; returns the actual manager with no retained installation owner.
/// root 拥有每个夹具文件；返回不保留安装所有者的实际管理器。
pub(super) fn fixture(root: &Path) -> VulcanPluginRuntimeManager {
    // Actual host paths match the existing production manager fixture conventions.
    // 实际宿主路径匹配既有生产管理器夹具约定。
    let runtime_root = root.join("runtime");
    // No bundled identity can shadow this actual user-installed package.
    // 没有内置身份可以覆盖此实际用户安装包。
    let bundled = runtime_root.join("plugins");
    fs::create_dir_all(&bundled).expect("create original uninstall bundled root");
    // Every operation uses this actual original manager.
    // 每个操作使用此实际原管理器。
    let manager = test_manager(&runtime_root, bundled);
    // Source bytes remain independent from the user-installed destination.
    // 来源字节保持独立于用户安装目标。
    let source = root.join("source/local-plugin");
    write_user_plugin_package(&source, "local-plugin");
    // Actual inspection produces both fingerprint and complete approval inputs.
    // 实际检查生产指纹及完整批准输入。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("actual original preview");
    manager
        .install_local_user_plugin(
            &source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("actual flat installation producer");
    manager
        .refresh_catalog(&BTreeSet::from(["local-plugin".into()]), &BTreeSet::new())
        .expect("actual disabled original catalog");
    manager
}

/// Physical namespace exclusion is observed through a separately opened actual store, without sharing original state.
/// 通过单独打开实际存储观察物理命名空间互斥，不共享原状态。
/// manager fixes the real root; returns no replacement ownership and requires an actual Busy result.
/// manager 固定真实根；不返回替代所有权，并要求实际忙碌结果。
fn assert_busy(manager: &VulcanPluginRuntimeManager) {
    // This actual independent store does not borrow the original guard or registry.
    // 此实际独立存储不借用原守卫或注册表。
    let contender = crate::VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("independent original uninstall store");
    assert_eq!(
        contender
            .acquire_legacy_mutation_guard("local-plugin", None)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("same actual original file blocks contender")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
}

/// Reservation is inert, captures every actual preimage before callbacks, and refuses hiding while the original call remains owned.
/// 预留未启动，在回调之前捕获每个实际前像，并在原调用仍被拥有时拒绝隐藏。
#[test]
fn legacy_uninstallation_original_reserves_preimages_and_blocks_actual_callback_borrowers() {
    // Every file and actual lock belongs to a disposable real fixture.
    // 每个文件及实际锁属于一次性真实夹具。
    let temp = tempfile::tempdir().expect("inert original uninstall fixture");
    // Actual installation and refresh establish the disabled current source.
    // 实际安装及刷新建立停用当前来源。
    let manager = fixture(temp.path());
    // Original configuration bytes exist before any removal callback is prepared.
    // 原配置字节在任何移除回调准备之前存在。
    let config = plugin_config_state_path(&manager.state.plugin_state_root, "local-plugin");
    fs::write(&config, b"original pre-callback configuration")
        .expect("actual original configuration bytes");
    // The same host-created nonnil UUID fixes the actual hidden target.
    // 相同宿主创建非空 UUID 固定实际隐藏目标。
    let request = Uuid::new_v4();
    // This original is kept alive independently of any observer or callback wrapper.
    // 此原实例独立于任何观测者或回调包装对象保持存活。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(request, "local-plugin")
        .expect("actual original flat removal reservation");
    assert_eq!(original.request_id(), request);
    assert!(original.source.package_root.is_dir() && !original.tombstone.exists());
    assert_eq!(
        original
            .original_state_preimages()
            .find(|(path, _)| *path == config)
            .expect("actual configuration preimage exists")
            .1,
        Some(&b"original pre-callback configuration"[..])
    );
    assert_busy(&manager);
    // No declaration still retains the actual source guard until explicit same-call retirement acknowledgement.
    // 缺失声明仍保留实际来源守卫，直至明确同调用退役确认。
    let mut call = manager
        .prepare_legacy_lifecycle_call(
            "local-plugin",
            crate::VulcanPluginLegacyLifecycleEvent::Uninstall,
            Arc::clone(original.mutation_guard()),
        )
        .expect("prepare actual guarded original uninstall call");
    call.start().expect("start original absent declaration");
    assert!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(!original.hide_started && original.source.package_root.exists());
    call.confirm_retired()
        .expect("confirm same actual source retirement")
        .expect("original absent declaration result");
    assert_eq!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .expect("hide after actual borrower release"),
        VulcanPluginLegacyUninstallationObservation::Hidden
    );
    assert!(!original.source.package_root.exists() && original.tombstone.is_dir());
    assert_eq!(
        original
            .original_state_preimages()
            .find(|(path, _)| *path == config)
            .expect("same preimage remains after hide")
            .1,
        Some(&b"original pre-callback configuration"[..])
    );
    // This is manager catalog preparation only, not actual service Full pointer commitment.
    // 此处仅为管理器目录准备，并非实际服务完整指针提交。
    let prepared = manager
        .refresh_catalog_with_legacy_mutation(
            &BTreeSet::new(),
            &BTreeSet::new(),
            "local-plugin",
            original.mutation_guard(),
        )
        .expect("same original file prepares removal catalog");
    assert!(!prepared.plugins.contains_key("local-plugin"));
    assert!(config.exists() && original.tombstone.exists());
    assert_busy(&manager);
}

/// Complete core cleanup retains its actual namespace and environment files, while terminal recovery refuses foreign state.
/// 完整核心清理保留实际命名空间及环境文件，而终态恢复拒绝外来状态。
#[test]
fn legacy_uninstallation_original_collects_and_confirms_without_deleting_resurrected_state() {
    // Real physical cleanup is verified without claiming a service commit or system credential acceptance.
    // 验证真实物理清理，不宣称服务提交或系统凭据验收。
    let temp = tempfile::tempdir().expect("completed original cleanup fixture");
    // The actual installed source and its records supply every original identity.
    // 实际安装来源及记录提供每个原身份。
    let manager = fixture(temp.path());
    // This retained original remains the same through both collection and terminal observation.
    // 此保留原实例跨回收及终态观测保持相同。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    assert!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    manager
        .hide_legacy_user_plugin_uninstallation(&mut original)
        .expect("hide original package");
    assert_eq!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .expect("core physical cleanup only"),
        VulcanPluginLegacyUninstallationObservation::Collected
    );
    assert!(!original.tombstone.exists());
    assert!(original.states.iter().all(|state| !state.path.exists()));
    assert_eq!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .expect("same terminal absence readback"),
        VulcanPluginLegacyUninstallationObservation::Collected
    );
    assert_busy(&manager);
    assert!(
        crate::runtime::environment::retain_plugin_execution_environment(
            &manager.state,
            "local-plugin"
        )
        .is_err()
    );
    // A newly introduced state file must remain foreign to the completed original cleanup.
    // 新引入状态文件必须继续作为已完成原清理的外来数据。
    let config = plugin_config_state_path(&manager.state.plugin_state_root, "local-plugin");
    fs::write(&config, b"foreign state after actual completion")
        .expect("introduce actual foreign state");
    assert!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert_eq!(
        fs::read(&config).expect("foreign state is not deleted"),
        b"foreign state after actual completion"
    );
    drop(original);
    crate::VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("reopen original namespace")
        .acquire_legacy_mutation_guard("local-plugin", None)
        .expect("original drop releases actual File");
    crate::runtime::environment::retain_plugin_execution_environment(
        &manager.state,
        "local-plugin",
    )
    .expect("original drop releases actual environment File");
}

/// Missing or changed state cannot be adopted as cleanup authority after the original post-callback snapshot was frozen.
/// 原回调后快照固定之后，缺失或改变状态不能被接纳为清理权威。
#[test]
fn legacy_uninstallation_original_refuses_foreign_state_before_any_deletion() {
    // Ordinary files exercise actual state comparison and removal ordering.
    // 普通文件验证实际状态比较及移除顺序。
    let temp = tempfile::tempdir().expect("foreign original state fixture");
    // Actual producer creates the original authorization and installation records.
    // 实际生产者创建原授权及安装记录。
    let manager = fixture(temp.path());
    // This file was absent before both reservation and the original hide.
    // 此文件在预留及原隐藏之前均缺失。
    let config = plugin_config_state_path(&manager.state.plugin_state_root, "local-plugin");
    // The first original freezes explicit absence instead of a guessed default configuration.
    // 首个原实例固定明确缺失，而非猜测默认配置。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    manager
        .hide_legacy_user_plugin_uninstallation(&mut original)
        .expect("freeze actual cleanup snapshot and hide");
    fs::write(&config, b"foreign state before cleanup")
        .expect("actual noncooperating state introduction");
    assert!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(!original.cleanup_started);
    assert!(original.tombstone.exists());
    assert!(
        original
            .states
            .iter()
            .filter(|state| state.original.is_some())
            .all(
                |state| fs::read(&state.path).expect("prior records remain before any deletion")
                    == *state.original.as_ref().expect("actual preimage")
            )
    );
    assert_eq!(
        fs::read(&config).expect("foreign configuration preserved"),
        b"foreign state before cleanup"
    );
    assert_busy(&manager);
    fs::remove_file(&config).expect("fixture explicitly resolves foreign introduction");
    manager
        .collect_legacy_user_plugin_uninstallation(&mut original)
        .expect("same original continues after actual conflict resolution");
}

/// Wrong managers, changed source bytes and managed selections fail before actual hiding or state deletion.
/// 错误管理器、改变来源字节及受管选择在实际隐藏或状态删除之前失败。
#[test]
fn legacy_uninstallation_original_refuses_foreign_manager_and_changed_source() {
    // Separate real managers have unrelated service incarnations and actual state roots.
    // 独立真实管理器具有无关服务实例及实际状态根。
    let temp = tempfile::tempdir().expect("foreign manager fixture");
    // First actual manager owns the original reservation.
    // 首个实际管理器拥有原预留。
    let manager = fixture(&temp.path().join("original"));
    // The other actual manager cannot use an original just because the plugin identifier is the same.
    // 另一实际管理器不能仅因插件标识相同而使用原实例。
    let other = fixture(&temp.path().join("other"));
    // Frozen ownership is non-cloneable and remains outside all failed calls.
    // 固定所有权不可克隆，并保持位于全部失败调用之外。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    assert!(
        other
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(!original.hide_started);
    fs::write(
        original.source.package_root.join("foreign.txt"),
        b"changed source bytes",
    )
    .expect("actual changed package");
    assert!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(!original.hide_started && !original.tombstone.exists());
    assert_busy(&manager);
    drop(original);
    assert!(
        manager
            .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
            .is_err()
    );
    crate::VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual store after rejection")
        .acquire_legacy_mutation_guard("local-plugin", None)
        .expect("failed verification releases actual namespace");
}

/// Actual managed selection remains confined to its explicit removal API rather than acquiring flat path authority.
/// 实际受管选择继续限定于明确移除接口，而非取得平铺路径权威。
#[test]
fn legacy_uninstallation_original_refuses_actual_managed_selection() {
    // The real manager initially owns an ordinary flat package.
    // 真实管理器起初拥有普通平铺包。
    let temp = tempfile::tempdir().expect("managed source refusal fixture");
    // This actual source is also valid input to immutable package preparation.
    // 此实际来源也可作为不可变包准备的有效输入。
    let manager = fixture(temp.path());
    // The actual flat descriptor supplies the exact current distributable bytes.
    // 实际平铺描述符提供精确当前可分发字节。
    let source = manager
        .snapshot()
        .plugins
        .get("local-plugin")
        .expect("actual flat source")
        .clone();
    // The real immutable producer, not a fabricated managed descriptor, selects the new source.
    // 真实不可变生产者，而非虚构受管描述符，选择新来源。
    let store = crate::VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual managed store");
    // Actual generation ownership remains live while testing the flat API refusal.
    // 测试平铺接口拒绝期间，实际代次所有权保持存活。
    let package = store
        .prepare(&source.package_root, &source.content_fingerprint)
        .expect("actual immutable source");
    store
        .activate_authorized(&package, 0, Vec::new())
        .expect("actual managed selection");
    manager
        .refresh_catalog(&BTreeSet::from(["local-plugin".into()]), &BTreeSet::new())
        .expect("actual managed catalog");
    assert!(
        manager
            .snapshot()
            .plugins
            .get("local-plugin")
            .expect("actual managed descriptor")
            .managed_package()
            .is_some()
    );
    assert!(
        manager
            .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
            .is_err()
    );
    assert!(source.package_root.exists() && package.package_root().exists());
}

/// Actual Windows directory delete-sharing refusal leaves the admitted original rename recoverable under its same File.
/// 实际 Windows 目录删除共享拒绝使已准入原重命名在相同文件内可恢复。
#[cfg(windows)]
#[test]
fn legacy_uninstallation_original_resumes_real_windows_hide_failure() {
    use std::os::windows::fs::OpenOptionsExt;
    // The actual operating system denies rename, without injecting a mocked filesystem result.
    // 实际操作系统拒绝重命名，不注入模拟文件系统结果。
    let temp = tempfile::tempdir().expect("actual original hide failure fixture");
    // Actual flat installation establishes all original bytes and records.
    // 实际平铺安装建立全部原字节及记录。
    let manager = fixture(temp.path());
    // Retain the same exact original across the failed system rename and explicit recovery.
    // 跨失败系统重命名及明确恢复保留相同精确原实例。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    // Backup semantics opens the actual directory while read/write sharing deliberately excludes delete sharing.
    // 备份语义打开实际目录，读写共享刻意排除删除共享。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x0200_0000)
        .share_mode(0x1 | 0x2)
        .open(&original.source.package_root)
        .expect("hold real Windows directory rename blocker");
    assert!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(
        original.hide_started
            && original.source.package_root.exists()
            && !original.tombstone.exists()
    );
    assert_eq!(
        manager
            .observe_legacy_user_plugin_uninstallation(&original)
            .expect("actual original path survived failure"),
        VulcanPluginLegacyUninstallationObservation::Original
    );
    assert_busy(&manager);
    drop(blocker);
    assert_eq!(
        manager
            .resume_legacy_user_plugin_uninstallation(&mut original)
            .expect("same admitted original hide recovery"),
        VulcanPluginLegacyUninstallationObservation::Hidden
    );
    assert_eq!(
        manager
            .resume_legacy_user_plugin_uninstallation(&mut original)
            .expect("actual prior success is read-only"),
        VulcanPluginLegacyUninstallationObservation::Hidden
    );
    assert!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
}

/// Actual partial Windows package deletion remains hidden, retains original preimages and physical environment ownership, then resumes.
/// 实际 Windows 包部分删除保持隐藏，保留原前像及物理环境所有权，然后恢复。
#[cfg(windows)]
#[test]
fn legacy_uninstallation_original_keeps_partial_delete_hidden_and_recovers_same_request() {
    use std::os::windows::fs::OpenOptionsExt;
    // All failures come from real Windows delete-sharing restrictions on original files.
    // 全部失败来自原文件真实 Windows 删除共享限制。
    let temp = tempfile::tempdir().expect("actual partial original cleanup fixture");
    // The installed source supplies actual authorization and installation preimages.
    // 安装来源提供实际授权及安装前像。
    let manager = fixture(temp.path());
    // Original bytes remain inside this reservation after actual state deletion occurs.
    // 实际状态删除发生之后，原字节仍位于此预留内。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    manager
        .hide_legacy_user_plugin_uninstallation(&mut original)
        .expect("hide actual original before cleanup");
    // The exact original bundle path comes from the existing actual source fixture, after its fixed rename.
    // 精确原包路径来自既有实际来源夹具，位于其固定重命名之后。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(original.tombstone.join("runtime/plugin.bundle.lua"))
        .expect("retain real original deletion blocker");
    assert!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(original.cleanup_started && !original.collected);
    assert!(!original.source.package_root.exists() && original.tombstone.exists());
    assert!(original.states.iter().all(|state| !state.path.exists()));
    assert!(
        original
            .original_state_preimages()
            .any(|(_, bytes)| bytes.is_some())
    );
    assert_eq!(
        manager
            .observe_legacy_user_plugin_uninstallation(&original)
            .expect("actual partial outcome remains recoverable"),
        VulcanPluginLegacyUninstallationObservation::CleanupPending
    );
    assert_busy(&manager);
    assert!(
        crate::runtime::environment::retain_plugin_execution_environment(
            &manager.state,
            "local-plugin"
        )
        .is_err()
    );
    drop(blocker);
    assert_eq!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .expect("resume same actual partial deletion"),
        VulcanPluginLegacyUninstallationObservation::Collected
    );
    assert!(!original.source.package_root.exists() && !original.tombstone.exists());
}

/// A real environment reader prevents every state or package deletion while the same hidden original remains retained.
/// 真实环境读者阻止每个状态或包删除，同时相同隐藏原实例保持保留。
#[test]
fn legacy_uninstallation_original_requires_actual_environment_reader_release_before_state_deletion()
{
    // The actual filesystem lease is distinct from any manager snapshot or observer counter.
    // 实际文件系统租约独立于任何管理器快照或观察者计数。
    let temp = tempfile::tempdir().expect("actual environment reader fixture");
    // The actual installed source supplies all original package and state bytes.
    // 实际安装来源提供全部原包及状态字节。
    let manager = fixture(temp.path());
    // An actual shared File remains physically owned before removal reservation or hiding.
    // 实际共享文件在移除预留或隐藏之前仍被物理拥有。
    let reader = crate::runtime::environment::retain_plugin_execution_environment(
        &manager.state,
        "local-plugin",
    )
    .expect("actual environment reader");
    // Only the same original owner is reused after the actual reader later retires.
    // 实际读者随后退役之后，仅复用相同原所有者。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    manager
        .hide_legacy_user_plugin_uninstallation(&mut original)
        .expect("hide original while environment still owned");
    assert!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    assert!(!original.cleanup_started && original.environment_cleanup.is_none());
    assert!(
        original
            .states
            .iter()
            .filter(|state| state.original.is_some())
            .all(|state| state.path.exists())
    );
    assert!(original.tombstone.is_dir() && !original.source.package_root.exists());
    assert_busy(&manager);
    drop(reader);
    assert_eq!(
        manager
            .collect_legacy_user_plugin_uninstallation(&mut original)
            .expect("same original resumes after actual environment File release"),
        VulcanPluginLegacyUninstallationObservation::Collected
    );
}

/// Real successful rename or terminal confirmation can lose acknowledgement without replaying any original effect or consuming ownership.
/// 真实成功重命名或终态确认可丢失确认，而不重放任何原效果或消费所有权。
#[test]
fn legacy_uninstallation_original_recovers_actual_post_success_error_and_panic() {
    for checkpoint in [
        observation::Checkpoint::Hidden,
        observation::Checkpoint::Collected,
    ] {
        for panic in [false, true] {
            // Every fault binds a separate actual root, package and retained original UUID.
            // 每个故障绑定独立实际根、包及保留原 UUID。
            let temp = tempfile::tempdir().expect("actual success acknowledgement fixture");
            // Actual installation supplies the complete original source and records.
            // 实际安装提供完整原来源及记录。
            let manager = fixture(temp.path());
            // The outer original owns both physical Files across real unwinding.
            // 外部原实例跨真实展开拥有两个物理文件。
            let mut original = manager
                .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
                .expect("reserve actual original");
            if checkpoint == observation::Checkpoint::Collected {
                manager
                    .hide_legacy_user_plugin_uninstallation(&mut original)
                    .expect("actual hide before terminal observer");
            }
            // A concrete observer scope owns no production File and does not borrow the original.
            // 具体观察者作用域不拥有生产文件，也不借用原实例。
            let scope = observation::arm(&original, checkpoint, panic);
            // Invoke the actual sole producer and catch only the deliberately raised post-success panic.
            // 调用实际唯一生产者，仅捕获刻意引发成功后 panic。
            let result = catch_unwind(AssertUnwindSafe(|| match checkpoint {
                observation::Checkpoint::Hidden => {
                    manager.hide_legacy_user_plugin_uninstallation(&mut original)
                }
                observation::Checkpoint::Collected => {
                    manager.collect_legacy_user_plugin_uninstallation(&mut original)
                }
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.expect("post-success error must not panic").is_err());
            }
            assert_eq!(manager.state.session_registration_gate.is_poisoned(), panic);
            assert_busy(&manager);
            match checkpoint {
                observation::Checkpoint::Hidden => {
                    assert_eq!(
                        manager
                            .observe_legacy_user_plugin_uninstallation(&original)
                            .expect("actual successful hidden readback"),
                        VulcanPluginLegacyUninstallationObservation::Hidden
                    );
                    assert_eq!(observation::counts(&original), (1, 0));
                    manager
                        .continue_legacy_user_plugin_uninstallation(&mut original)
                        .expect("same actual hide intent continues read-only recovery");
                    assert_eq!(observation::counts(&original), (1, 0));
                }
                observation::Checkpoint::Collected => {
                    assert!(original.collected);
                    assert_eq!(observation::counts(&original), (0, 1));
                    assert_eq!(
                        manager
                            .collect_legacy_user_plugin_uninstallation(&mut original)
                            .expect("same retained terminal receipt is read-only recovery"),
                        VulcanPluginLegacyUninstallationObservation::Collected
                    );
                    assert_eq!(observation::counts(&original), (0, 1));
                }
            }
            assert!(!manager.state.session_registration_gate.is_poisoned());
            drop(scope);
            assert_busy(&manager);
        }
    }
}

/// Sequencing-only recovery never clears a poisoned data-bearing catalog or mutates original package/state bytes.
/// 仅排序恢复绝不清除携带数据目录污染，也不变更原包／状态字节。
#[test]
fn legacy_uninstallation_original_refuses_poisoned_catalog_during_gate_recovery() {
    // Actual mutex and RwLock poisoning is triggered by real unwinding, without fabricated status flags.
    // 实际互斥锁及读写锁污染由真实展开触发，不虚构状态标志。
    let temp = tempfile::tempdir().expect("actual data poison fixture");
    // The actual source and complete preimages enter the original before any poison.
    // 实际来源及完整前像在任何污染之前进入原实例。
    let manager = fixture(temp.path());
    // The outer retained owner remains available independently of the failing worker's stack.
    // 外部保留所有者独立于失败工作者栈保持可用。
    let mut original = manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
        .expect("reserve actual original");
    // Hold the actual data-bearing and sequencing locks when the worker unwinds, without altering protected values.
    // 工作者展开时持有实际携带数据及排序锁，不改变受保护值。
    let result = catch_unwind(AssertUnwindSafe(|| {
        // This is the actual unit registration mutex, not a replacement test lock.
        // 此为实际空元组登记互斥锁，而非替代测试锁。
        let _registration = manager
            .state
            .session_registration_gate
            .lock()
            .expect("actual registration guard");
        // This actual catalog lock protects data and must never be repaired as a unit gate.
        // 此实际目录锁保护数据，绝不能作为空元组门修复。
        let _catalog = manager
            .state
            .snapshot
            .write()
            .expect("actual catalog write guard");
        panic!("actual worker unwinds with data-bearing catalog lock");
    }));
    assert!(result.is_err());
    assert!(
        manager.state.snapshot.is_poisoned()
            && manager.state.session_registration_gate.is_poisoned()
    );
    assert!(
        manager
            .hide_legacy_user_plugin_uninstallation(&mut original)
            .expect_err("data poison prevents sequencing repair")
            .to_string()
            .contains("catalog lock is poisoned")
    );
    assert!(
        manager.state.snapshot.is_poisoned()
            && manager.state.session_registration_gate.is_poisoned()
    );
    assert!(
        !original.hide_started
            && original.source.package_root.exists()
            && !original.tombstone.exists()
    );
    assert!(
        original
            .states
            .iter()
            .filter(|state| state.original.is_some())
            .all(
                |state| fs::read(&state.path).expect("preimages remain physically unchanged")
                    == *state.original.as_ref().expect("original bytes")
            )
    );
    assert_busy(&manager);
}

/// An actual sparse oversized configuration fails before original reservation or hide and retains every package and state byte.
/// 实际稀疏超限配置在原预留或隐藏之前失败，并保留每个包及状态字节。
#[test]
fn legacy_uninstallation_original_rejects_oversized_state_before_effects() {
    // Sparse length exceeds the real sole snapshot budget without constructing a large in-memory test value.
    // 稀疏长度超过真实唯一快照额度，不构造大型内存测试值。
    let temp = tempfile::tempdir().expect("oversized original state fixture");
    // Actual installation produces the original package and bounded ordinary records.
    // 实际安装生产原包及有界普通记录。
    let manager = fixture(temp.path());
    // This exact host-owned configuration path belongs to the original plugin.
    // 此精确宿主所有配置路径属于原插件。
    let config = plugin_config_state_path(&manager.state.plugin_state_root, "local-plugin");
    fs::File::create(&config)
        .expect("create actual sparse configuration")
        .set_len(MAX_PLUGIN_STATE_SNAPSHOT_BYTES + 1)
        .expect("set actual oversized file length");
    assert!(
        manager
            .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "local-plugin")
            .expect_err("oversized actual state must fail before any callback or hide")
            .to_string()
            .contains("snapshot budget")
    );
    assert_eq!(
        fs::metadata(&config)
            .expect("oversized state remains")
            .len(),
        MAX_PLUGIN_STATE_SNAPSHOT_BYTES + 1
    );
    assert!(manager.user_plugin_root().join("local-plugin").is_dir());
    assert!(
        plugin_authorization_state_path(&manager.state.plugin_state_root, "local-plugin").is_file()
    );
    assert!(
        plugin_installation_state_path(&manager.state.plugin_state_root, "local-plugin").is_file()
    );
    crate::VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual store after reservation refusal")
        .acquire_legacy_mutation_guard("local-plugin", None)
        .expect("failed snapshot does not strand namespace ownership");
}

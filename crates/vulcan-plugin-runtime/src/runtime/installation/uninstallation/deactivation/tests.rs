//! Actual managed local removal tests exercise the real selection producer, callbacks, physical leases and uncertain cleanup.
//! 实际受管本地移除测试覆盖真实选择生产者、回调、物理租约及不确定清理。

use super::*;
use crate::{
    VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore,
};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

mod flat_predecessor;

/// The real source declares no credential slots; every unexpected secure call fails rather than fabricating absence.
/// 真实来源未声明凭据槽位；每个意外安全调用失败，而非虚构缺失。
struct NoCredentialSlots;
impl VulcanPluginCredentialBackend for NoCredentialSlots {
    /// plugin_id/revision/key are unexpected for this actual empty inventory; returns an explicit fixture failure.
    /// plugin_id／revision／key 对实际空清单为意外；返回明确夹具失败。
    fn read(
        &self,
        _plugin_id: &str,
        _revision: &VulcanPluginCredentialRevision,
        _key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err("No credential slots were declared by the actual fixture.".into())
    }
    /// plugin_id/revision/key/value must never be sent for the empty declaration; returns explicit failure.
    /// 空声明绝不能发送 plugin_id／revision／key／value；返回明确失败。
    fn write(
        &self,
        _plugin_id: &str,
        _revision: &VulcanPluginCredentialRevision,
        _key: &VulcanPluginCredentialKey,
        _value: &str,
    ) -> Result<(), String> {
        Err("No credential slots were declared by the actual fixture.".into())
    }
    /// plugin_id/revision/key cannot imply an invented secure deletion; returns explicit failure.
    /// plugin_id／revision／key 不能暗示虚构安全删除；返回明确失败。
    fn delete(
        &self,
        _plugin_id: &str,
        _revision: &VulcanPluginCredentialRevision,
        _key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err("No credential slots were declared by the actual fixture.".into())
    }
}

/// Publish a real managed successor of the actual flat fixture without retaining a package pin in the returned tuple.
/// 发布实际平铺夹具真实受管后继，不在返回元组内保留包固定。
/// root owns all bytes and versioned chooses the actual credential-staging producer; returns actual manager/store/complete selection.
/// root 拥有全部字节，versioned 选择实际凭据暂存生产者；返回实际管理器／存储／完整选择。
fn fixture(
    root: &Path,
    versioned: bool,
) -> (
    VulcanPluginRuntimeManager,
    VulcanPluginGenerationStore,
    VulcanPluginActivation,
) {
    // Reuse the same real installation and raw state producers as flat removal acceptance.
    // 复用与平铺移除验收相同真实安装及原始状态生产者。
    let manager = super::super::tests::fixture(root);
    // Prepare distributable bytes from the actual independent source created by that fixture.
    // 从该夹具创建的实际独立来源准备可分发字节。
    let source = root.join("source/local-plugin");
    // Actual inspection supplies the fingerprint rather than constructing an immutable identity.
    // 实际检查提供指纹，而非构造不可变身份。
    let inspected =
        crate::inspect_user_plugin_package(&source).expect("actual managed local source");
    // This store is rooted at the actual manager's user directory.
    // 此存储以实际管理器用户目录为根。
    let store = VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual managed local store");
    // The actual temporary package pin drops at function return.
    // 实际临时包固定在函数返回时释放。
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("actual immutable preparation");
    // Complete permission authority is produced from the inspected source.
    // 完整权限权威从已检查来源产生。
    let permissions =
        crate::runtime::plugin_required_permissions(&inspected).expect("actual source permissions");
    // Both legacy and versioned selections come from production writers, never hand-built records.
    // 旧式及版本化选择均来自生产写入器，绝非手工构造记录。
    let selected = if versioned {
        // Empty revision still has a real verified journal and actual package retention.
        // 空修订仍具有真实已验证日志及实际包保留。
        let revision = VulcanPluginCredentialRevision::allocate();
        store
            .begin_credential_staging(
                &package,
                revision.clone(),
                &VulcanPluginCredentialValues::new(),
            )
            .expect("actual empty staging")
            .write_and_finish(&NoCredentialSlots, &VulcanPluginCredentialValues::new())
            .expect("actual verified empty journal");
        store
            .activate_with_credentials(&package, 0, permissions, revision)
            .expect("actual versioned selection")
    } else {
        store
            .activate_authorized(&package, 0, permissions)
            .expect("actual legacy selection")
    };
    manager
        .refresh_catalog(
            &BTreeSet::from([selected.plugin_id.clone()]),
            &BTreeSet::new(),
        )
        .expect("actual managed disabled catalog");
    (manager, store, selected)
}

/// Reserve the real selection File and all actual local preimages before any removal effects.
/// 在任何移除效果之前预留真实选择文件及全部实际本地前像。
/// manager/store/selected are actual fixture producers; returns the non-cloneable retained pair.
/// manager／store／selected 为实际夹具生产者；返回不可克隆保留配对。
fn reserve(
    manager: &VulcanPluginRuntimeManager,
    store: &VulcanPluginGenerationStore,
    selected: &VulcanPluginActivation,
) -> (
    VulcanPluginDeactivationReservation,
    VulcanPluginDeactivationStateReservation,
) {
    // The caller retains this allocation independently of any local observer wrapper.
    // 调用方独立于任何本地观察者包装对象保留此分配。
    let mut original = store
        .reserve_deactivation(Uuid::new_v4(), selected)
        .expect("actual selection original");
    // Weak identity binds this local original without adding an extra strong namespace borrower.
    // 弱身份绑定此本地原实例，不添加额外强命名空间借用者。
    let state = manager
        .reserve_deactivation_state(&mut original)
        .expect("actual local state original");
    (original, state)
}

/// Physical namespace exclusion comes from an independent real store open, rather than weak metadata or status bits.
/// 物理命名空间互斥来自独立真实存储打开，而非弱元数据或状态位。
fn assert_busy(manager: &VulcanPluginRuntimeManager) {
    // A distinct instance competes for the same actual File.
    // 独立实例竞争相同实际文件。
    let contender = VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual contender store");
    assert_eq!(
        contender
            .acquire_legacy_mutation_guard("local-plugin", None)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("actual file remains busy")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
}

/// Managed preimages precede the actual callback, while weak binding permits publication and does not block physical package GC.
/// 受管前像先于实际回调，而弱绑定允许发布且不阻挡物理包回收。
#[test]
fn deactivation_state_original_captures_callback_preimages_without_blocking_selection_or_package_gc()
 {
    // Each protocol version is exercised through its actual source and credential producer.
    // 每个协议版本通过实际来源及凭据生产者被覆盖。
    for versioned in [false, true] {
        // The temporary root outlives every real original and physical lease.
        // 临时根比每个真实原实例及物理租约存活更久。
        let temp = tempfile::tempdir().expect("actual managed preimage fixture");
        // The tuple carries no retained immutable package pin.
        // 元组不携带保留不可变包固定。
        let (manager, store, selected) = fixture(temp.path(), versioned);
        // Actual raw compatibility state is captured separately from versioned selection configuration.
        // 实际原始兼容状态独立于版本化选择配置被捕获。
        let config = plugin_config_state_path(manager.plugin_state_root(), "local-plugin");
        fs::write(&config, b"before original callback").expect("actual pre-callback bytes");
        // Both objects remain strongly retained throughout all failures and acknowledgements.
        // 两个对象跨全部失败及确认继续强保留。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        assert_eq!(state.request_id(), original.request_id());
        assert_eq!(
            manager
                .observe_deactivation_state(&state, &original)
                .expect("inert local observation"),
            VulcanPluginDeactivationStateObservation::Reserved
        );
        assert_eq!(
            state
                .original_state_preimages()
                .find(|(path, _)| *path == config)
                .expect("actual config slot")
                .1,
            Some(&b"before original callback"[..])
        );
        // Even an absent callback declaration retains the actual original namespace until explicit retirement.
        // 即使回调声明缺失，也在明确退役之前保留实际原命名空间。
        let mut call = manager
            .prepare_deactivation_lifecycle_call(&original)
            .expect("actual original callback");
        call.start().expect("actual absent callback start");
        assert!(
            manager
                .freeze_deactivation_state(&mut state, &mut original)
                .is_err()
        );
        assert!(!state.cleanup_frozen && state.states.iter().all(|file| file.cleanup.is_none()));
        assert!(store.publish_deactivation_retained(&mut original).is_err());
        call.confirm_retired()
            .expect("actual callback retirement")
            .expect("actual callback result");
        fs::write(&config, b"after original callback").expect("actual post-callback bytes");
        assert_eq!(
            manager
                .freeze_deactivation_state(&mut state, &mut original)
                .expect("exact post-callback freeze"),
            VulcanPluginDeactivationStateObservation::Frozen
        );
        assert!(
            manager
                .freeze_deactivation_state(&mut state, &mut original)
                .is_err()
        );
        assert!(
            manager
                .collect_deactivation_state(&mut state, &original)
                .is_err()
        );
        assert!(config.exists() && !state.cleanup_started);
        assert_eq!(
            store
                .publish_deactivation_retained(&mut original)
                .expect("weak state identity cannot block actual publication"),
            VulcanPluginDeactivationObservation::Published
        );
        // Only the actual manager catalog is removed here; this test does not claim a service Full pointer commit.
        // 此处仅移除实际管理器目录；此测试不宣称服务完整指针提交。
        manager
            .refresh_catalog_with_deactivation(&BTreeSet::new(), &BTreeSet::new(), &original)
            .expect("actual same-file tombstone catalog");
        assert!(
            store
                .collect_deactivation_package_retained(&original)
                .expect("no local-state package pin blocks real GC")
        );
        assert_eq!(
            manager
                .collect_deactivation_state(&mut state, &original)
                .expect("actual local physical cleanup"),
            VulcanPluginDeactivationStateObservation::Collected
        );
        assert_eq!(
            state
                .original_state_preimages()
                .find(|(path, _)| *path == config)
                .expect("same config preimage")
                .1,
            Some(&b"before original callback"[..])
        );
        assert_eq!(
            manager
                .collect_deactivation_state(&mut state, &original)
                .expect("terminal absence confirmation only"),
            VulcanPluginDeactivationStateObservation::Collected
        );
        assert_busy(&manager);
        // Dormant flat predecessor bytes are outside this local API and cannot be counted as whole-uninstall completion.
        // 休眠平铺前驱字节位于此本地接口之外，不能被计为整个卸载完成。
        assert!(manager.user_plugin_root().join("local-plugin").is_dir());
    }
}

/// A replacement real File with the same UUID and full record cannot adopt the old local original after its real owner is dropped.
/// 原真实所有者释放之后，具有相同 UUID 及完整记录的替代真实文件不能接纳旧本地原实例。
#[test]
fn deactivation_state_original_refuses_same_uuid_record_replacement_owner() {
    // Actual weak identity remains allocated while its original File authority is explicitly dropped.
    // 原文件权威明确释放期间，实际弱身份仍保持分配。
    let temp = tempfile::tempdir().expect("actual same UUID replacement fixture");
    // All replacement evidence comes from the same actual selected producer record.
    // 全部替代证据来自相同实际已选生产者记录。
    let (manager, store, selected) = fixture(temp.path(), false);
    // The old local object cannot reacquire ownership on its own.
    // 旧本地对象不能自行重新获取所有权。
    let (original, mut state) = reserve(&manager, &store, &selected);
    // Metadata is deliberately preserved to prove it is insufficient.
    // 刻意保留元数据，以证明其不足。
    let request = original.request_id();
    drop(original);
    // This real replacement can acquire the File but has a distinct allocation identity.
    // 此真实替代可取得文件，但具有不同分配身份。
    let mut replacement = store
        .reserve_deactivation(request, &selected)
        .expect("actual replacement File");
    assert_eq!(replacement.request_id(), state.request_id());
    assert_eq!(replacement.original(), &state.selection);
    assert!(
        manager
            .observe_deactivation_state(&state, &replacement)
            .is_err()
    );
    assert!(
        manager
            .freeze_deactivation_state(&mut state, &mut replacement)
            .is_err()
    );
    assert!(
        manager
            .collect_deactivation_state(&mut state, &replacement)
            .is_err()
    );
    assert!(!state.cleanup_frozen && !state.cleanup_started);
    // A newly reserved pair may use its own real File, while the older pair remains rejected.
    // 新预留配对可以使用自身真实文件，同时较旧配对仍被拒绝。
    let mut fresh = manager
        .reserve_deactivation_state(&mut replacement)
        .expect("new actual pair");
    manager
        .freeze_deactivation_state(&mut fresh, &mut replacement)
        .expect("fresh exact pair freeze");
    assert!(
        state
            .states
            .iter()
            .filter(|file| file.original.is_some())
            .all(|file| file.path.exists())
    );
}

/// Another manager, even sharing literal roots, cannot operate the original local resources or repair its sequencing gate.
/// 另一管理器即使共用字面根，也不能操作原本地资源或修复其排序门。
#[test]
fn deactivation_state_original_refuses_other_actual_manager() {
    // Original paths remain live throughout both manager incarnations.
    // 原路径跨两个管理器实例保持存活。
    let temp = tempfile::tempdir().expect("actual other manager fixture");
    // This actual instance alone owns the pair.
    // 仅此实际实例拥有配对。
    let (manager, store, selected) = fixture(temp.path(), true);
    // Namespace allocation and actual manager allocation are independent exact binding requirements.
    // 命名空间分配及实际管理器分配为独立精确绑定要求。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    // A second real manager opens the same actual directories rather than changing a field on the original instance.
    // 第二真实管理器打开相同实际目录，而非改变原实例字段。
    let other = crate::runtime::tests::test_manager(
        &temp.path().join("runtime"),
        temp.path().join("runtime/plugins"),
    );
    assert_eq!(other.user_plugin_root(), manager.user_plugin_root());
    assert!(other.observe_deactivation_state(&state, &original).is_err());
    assert!(
        other
            .freeze_deactivation_state(&mut state, &mut original)
            .is_err()
    );
    assert!(
        other
            .collect_deactivation_state(&mut state, &original)
            .is_err()
    );
    assert!(!state.cleanup_frozen && state.environment_cleanup.is_none());
    assert_busy(&manager);
}

/// An actual environment reader prevents all state deletion; frozen foreign state and terminal resurrection are preserved.
/// 实际环境读者阻止全部状态删除；固定后外来状态及终态复活保持保留。
#[test]
fn deactivation_state_original_requires_real_environment_release_and_preserves_foreign_state() {
    // This fixture uses a real versioned selected journal and independent filesystem environment File.
    // 此夹具使用真实版本化已选日志及独立文件系统环境文件。
    let temp = tempfile::tempdir().expect("actual local environment refusal fixture");
    // Actual raw flat-compatibility preimages remain owned beside the versioned source.
    // 实际原始平铺兼容前像与版本化来源共同保持拥有。
    let (manager, store, selected) = fixture(temp.path(), true);
    // Shared lease blocks the same actual exclusive cleanup file.
    // 共享租约阻挡相同实际排他清理文件。
    let reader = crate::runtime::environment::retain_plugin_execution_environment(
        &manager.state,
        "local-plugin",
    )
    .expect("actual shared environment File");
    // This strongly retained pair survives both independent actual refusal causes.
    // 此强保留配对跨两个独立实际拒绝原因存续。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    manager
        .freeze_deactivation_state(&mut state, &mut original)
        .expect("freeze actual cleanup files");
    store
        .publish_deactivation_retained(&mut original)
        .expect("actual tombstone publication");
    assert!(
        manager
            .collect_deactivation_state(&mut state, &original)
            .is_err()
    );
    assert!(!state.cleanup_started && state.environment_cleanup.is_none());
    assert!(
        state
            .states
            .iter()
            .filter(|file| file.original.is_some())
            .all(|file| file.path.exists())
    );
    drop(reader);
    // Actual configuration was explicitly absent at freeze, so later bytes are foreign and must not be adopted.
    // 固定时实际配置明确缺失，因此随后字节为外来且不得被接纳。
    let config = plugin_config_state_path(manager.plugin_state_root(), "local-plugin");
    fs::write(&config, b"foreign frozen-state replacement").expect("actual foreign bytes");
    assert!(
        manager
            .collect_deactivation_state(&mut state, &original)
            .is_err()
    );
    assert!(!state.cleanup_started);
    assert_eq!(
        fs::read(&config).expect("preserved foreign state"),
        b"foreign frozen-state replacement"
    );
    assert!(
        state
            .states
            .iter()
            .filter(|file| file.original.is_some())
            .all(|file| file.path.exists())
    );
    fs::remove_file(&config).expect("fixture explicitly resolves foreign conflict");
    manager
        .collect_deactivation_state(&mut state, &original)
        .expect("same original local cleanup");
    fs::write(&config, b"terminal foreign resurrection").expect("actual later foreign bytes");
    assert!(
        manager
            .observe_deactivation_state(&state, &original)
            .is_err()
    );
    assert!(
        manager
            .collect_deactivation_state(&mut state, &original)
            .is_err()
    );
    assert_eq!(
        fs::read(&config).expect("terminal foreign bytes preserved"),
        b"terminal foreign resurrection"
    );
    assert_busy(&manager);
    assert!(
        crate::runtime::environment::retain_plugin_execution_environment(
            &manager.state,
            "local-plugin"
        )
        .is_err()
    );
}

/// Actual partial Windows environment deletion retains every original and resumes without recapturing post-callback state.
/// 实际 Windows 环境部分删除保留每个原实例，并在不重新捕获回调后状态的情况下恢复。
#[cfg(windows)]
#[test]
fn deactivation_state_original_resumes_real_windows_partial_environment_cleanup() {
    use std::os::windows::fs::OpenOptionsExt;
    // Failure is imposed by actual operating-system sharing, never a mocked deletion result.
    // 失败由实际操作系统共享限制施加，绝非模拟删除结果。
    let temp = tempfile::tempdir().expect("actual partial environment fixture");
    // Actual producer selections supply every original identifier.
    // 实际生产者选择提供每个原标识。
    let (manager, store, selected) = fixture(temp.path(), false);
    // The physical child is under the exact host environment root used by its real lease.
    // 物理子级位于真实租约使用的精确宿主环境根内。
    let environment = manager.state.plugin_runtime_root.join("local-plugin");
    fs::create_dir_all(&environment).expect("actual owned environment directory");
    fs::write(environment.join("held.txt"), b"actual environment resource")
        .expect("actual environment bytes");
    // Read/write sharing deliberately excludes delete sharing for this real original file.
    // 此真实原文件的读写共享刻意排除删除共享。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(environment.join("held.txt"))
        .expect("actual Windows environment deletion blocker");
    // Neither original is moved into a disposable worker result.
    // 任一原实例均不移动进入一次性工作者结果。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    manager
        .freeze_deactivation_state(&mut state, &mut original)
        .expect("actual freeze");
    store
        .publish_deactivation_retained(&mut original)
        .expect("actual publication");
    assert!(
        manager
            .collect_deactivation_state(&mut state, &original)
            .is_err()
    );
    assert!(state.cleanup_started && !state.collected && state.environment_cleanup.is_some());
    assert!(state.states.iter().all(|file| !file.path.exists()));
    assert!(
        state
            .original_state_preimages()
            .any(|(_, bytes)| bytes.is_some())
    );
    assert_eq!(
        manager
            .observe_deactivation_state(&state, &original)
            .expect("same real partial observation"),
        VulcanPluginDeactivationStateObservation::CleanupPending
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
            .collect_deactivation_state(&mut state, &original)
            .expect("same retained local recovery"),
        VulcanPluginDeactivationStateObservation::Collected
    );
    assert!(!environment.exists());
}

/// Lost acknowledgement after true local completion retains the same terminal original through error and real panic unwinding.
/// 真实本地完成后丢失确认，使相同终态原实例跨错误及真实 panic 展开保持保留。
#[test]
fn deactivation_state_original_recovers_true_terminal_error_and_panic_without_replaying_cleanup() {
    // Both actual failure forms occur after the same real physical cleanup producer succeeds.
    // 两种实际失败形式均发生于相同真实物理清理生产者成功之后。
    for panic in [false, true] {
        // Resource roots remain owned outside the unwinding operation.
        // 资源根保持位于展开操作之外拥有。
        let temp = tempfile::tempdir().expect("actual terminal uncertainty fixture");
        // The actual managed source establishes all state and physical ownership.
        // 实际受管来源建立全部状态及物理所有权。
        let (manager, store, selected) = fixture(temp.path(), true);
        // Strong originals outlive the cancelled acknowledgement wrapper.
        // 强原实例比被取消确认包装对象存活更久。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        manager
            .freeze_deactivation_state(&mut state, &mut original)
            .expect("actual frozen local state");
        store
            .publish_deactivation_retained(&mut original)
            .expect("actual tombstone");
        // Exact test-only observation retains no production File or borrow of either original.
        // 精确仅测试观测不保留生产文件，也不借用任一原实例。
        let _fault = observation::arm_exact(
            &state.package_root,
            state.plugin_id(),
            state.request_id(),
            observation::Checkpoint::Collected,
            panic,
        );
        // Only the synchronous actual cleanup invocation unwinds; originals remain allocated in this outer scope.
        // 仅同步实际清理调用展开；原实例仍在此外层作用域保持分配。
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            manager.collect_deactivation_state(&mut state, &original)
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            assert!(
                outcome
                    .expect("error acknowledgement did not panic")
                    .is_err()
            );
        }
        assert!(state.collected && state.cleanup_started);
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (0, 1)
        );
        assert_eq!(
            manager
                .observe_deactivation_state(&state, &original)
                .expect("true terminal original observation"),
            VulcanPluginDeactivationStateObservation::Collected
        );
        assert_eq!(
            manager
                .collect_deactivation_state(&mut state, &original)
                .expect("same-original read-only terminal recovery"),
            VulcanPluginDeactivationStateObservation::Collected
        );
        assert!(!manager.state.session_registration_gate.is_poisoned());
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (0, 1)
        );
        assert_busy(&manager);
        assert!(
            crate::runtime::environment::retain_plugin_execution_environment(
                &manager.state,
                "local-plugin"
            )
            .is_err()
        );
    }
}

/// Same-file ownership permits one local preimage owner only, including after that local object is accidentally dropped.
/// 同文件所有权仅允许单个本地前像所有者，包括该本地对象意外释放之后。
#[test]
fn deactivation_state_original_rejects_duplicate_preimage_owner_and_unfrozen_publication() {
    // Real selection admission is checked separately from opaque preimage ownership.
    // 真实选择准入独立于不透明前像所有权被检查。
    let temp = tempfile::tempdir().expect("actual single preimage owner fixture");
    // The actual producer selects a versioned package with real empty credential journal.
    // 实际生产者选择具有真实空凭据日志的版本化包。
    let (manager, store, selected) = fixture(temp.path(), true);
    // This actual selection File remains strongly retained even after the local object is dropped.
    // 即使本地对象释放，此实际选择文件仍继续强保留。
    let (mut original, state) = reserve(&manager, &store, &selected);
    assert!(manager.reserve_deactivation_state(&mut original).is_err());
    assert!(store.publish_deactivation_retained(&mut original).is_err());
    assert_eq!(
        store
            .reconcile_deactivation_retained(&original)
            .expect("no publication was admitted"),
        VulcanPluginDeactivationObservation::Original
    );
    assert!(
        state
            .states
            .iter()
            .filter(|file| file.original.is_some())
            .all(|file| file.path.exists())
    );
    drop(state);
    assert!(manager.reserve_deactivation_state(&mut original).is_err());
    assert!(store.publish_deactivation_retained(&mut original).is_err());
    assert_busy(&manager);
}

/// An oversized actual cleanup file leaves every freeze slot unchanged and publication blocked until explicit original recovery.
/// 实际超限清理文件保持每个固定槽未变，并在明确原恢复之前阻挡发布。
#[test]
fn deactivation_state_original_failed_bounded_freeze_does_not_adopt_partial_snapshots() {
    // Actual opaque reads share the same sole resource budget as flat original removal.
    // 实际不透明读取与平铺原移除共用相同唯一资源额度。
    let temp = tempfile::tempdir().expect("actual bounded managed freeze fixture");
    // Actual package and state producers establish a valid original before later state growth.
    // 实际包及状态生产者在随后状态增长之前建立有效原实例。
    let (manager, store, selected) = fixture(temp.path(), false);
    // Both originals remain retained while all cleanup reads are staged before assignment.
    // 全部清理读取先于赋值暂存期间，两个原实例继续保持保留。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    // This exact actual path was absent at original capture, rather than a guessed configuration default.
    // 此精确实际路径在原捕获时缺失，而非猜测配置默认值。
    let config = plugin_config_state_path(manager.plugin_state_root(), "local-plugin");
    // Actual sparse growth exceeds the sole snapshot budget without allocating a comparable in-memory fixture.
    // 实际稀疏增长超出唯一快照额度，不分配相近大小内存夹具。
    let oversized = File::create(&config).expect("actual oversized cleanup file");
    oversized
        .set_len(super::super::super::MAX_PLUGIN_STATE_SNAPSHOT_BYTES + 1)
        .expect("actual bounded overflow");
    drop(oversized);
    assert!(
        manager
            .freeze_deactivation_state(&mut state, &mut original)
            .is_err()
    );
    assert!(!state.cleanup_frozen && state.states.iter().all(|file| file.cleanup.is_none()));
    assert!(store.publish_deactivation_retained(&mut original).is_err());
    assert_eq!(
        store
            .reconcile_deactivation_retained(&original)
            .expect("same original remained selected"),
        VulcanPluginDeactivationObservation::Original
    );
    fs::remove_file(&config).expect("fixture explicitly removes oversized foreign introduction");
    manager
        .freeze_deactivation_state(&mut state, &mut original)
        .expect("same owner retries complete bounded freeze");
    store
        .publish_deactivation_retained(&mut original)
        .expect("only true full freeze permits publication");
    assert_busy(&manager);
}

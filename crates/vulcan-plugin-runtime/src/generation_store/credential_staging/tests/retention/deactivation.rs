//! Actual revision leases and journals verify exact original removal cleanup; secure values alone use a memory fixture.
//! 实际修订租约及日志验证精确原移除清理；仅安全值使用内存夹具。

use super::*;
use crate::{VulcanPluginConfigurationSelection, VulcanPluginDeactivationReservation};
use std::panic::{AssertUnwindSafe, catch_unwind};
use uuid::Uuid;

/// Observe actual fixture backend deletions and optionally lose acknowledgement after the first real side effect.
/// 观察实际夹具后端删除，并可在首个真实副作用后丢失确认。
struct CleanupBackend<'a> {
    /// The original secure tuple map remains separate from actual filesystem ownership.
    /// 原安全元组映射独立于实际文件系统所有权。
    inner: &'a Backend,
    /// Count actual deletion attempts to detect terminal recovery replay.
    /// 统计实际删除尝试，以检测终态恢复重放。
    deletes: Cell<usize>,
    /// One post-effect fault selects an error or panic and is consumed before returning control.
    /// 单次效果后故障选择错误或 panic，并在返回控制前消费。
    fault: Cell<Option<bool>>,
    /// A false-success fixture leaves the actual value present for mandatory readback.
    /// 假成功夹具使实际值仍存在，以供强制回读。
    skip_delete: bool,
}

impl VulcanPluginCredentialBackend for CleanupBackend<'_> {
    /// Read the exact original fixture tuple; returns actual map presence or absence.
    /// 读取精确原夹具元组；返回实际映射存在或缺失。
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        self.inner.read(plugin_id, revision, key)
    }

    /// Delegate only explicitly requested fixture writes; returns the underlying actual result.
    /// 仅委托明确请求夹具写入；返回底层实际结果。
    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        self.inner.write(plugin_id, revision, key, value)
    }

    /// Delete this exact original tuple unless the test explicitly injects false success; returns a possible post-effect fault.
    /// 删除此精确原元组，除非测试明确注入假成功；返回可能效果后故障。
    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        self.deletes.set(self.deletes.get() + 1);
        if !self.skip_delete {
            self.inner.delete(plugin_id, revision, key)?;
        }
        if let Some(panic) = self.fault.take() {
            if panic {
                panic!("fixture lost acknowledgement after actual deletion");
            }
            return Err("fixture-sensitive post-deletion failure".into());
        }
        Ok(())
    }
}

/// Select a real staged configured source and reserve its actual removal; returns one original without fabricated metadata.
/// 选择真实已暂存已配置来源，并预留实际移除；返回单个原实例，不虚构元数据。
/// store and package are actual producers and revision is fully staged; no backend cleanup or selection effect occurs here.
/// store 及 package 为实际生产者，revision 已完全暂存；此处不发生后端清理或移除选择效果。
fn reserve(
    store: &VulcanPluginGenerationStore,
    package: &PinnedVulcanPluginGeneration,
    revision: &VulcanPluginCredentialRevision,
) -> VulcanPluginDeactivationReservation {
    // The sole activation writer supplies every approved field consumed by the removal reservation.
    // 唯一激活写入者提供移除预留消费的每个已批准字段。
    let selected = store
        .activate_configured(
            package,
            0,
            package.required_permissions.clone(),
            revision.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select actual original staged credentials");
    store
        .reserve_deactivation(Uuid::new_v4(), &selected)
        .expect("reserve actual verified original inventory")
}

/// Physical credential readers block every backend effect, while frozen inventory survives independent package collection.
/// 物理凭据读者阻挡每个后端效果，固定清单跨独立包回收仍存活。
#[test]
fn retained_deactivation_credentials_require_real_reader_release_and_keep_optional_slots() {
    // Actual files remain alive independently of the fixture secure values.
    // 实际文件独立于夹具安全值保持存活。
    let temp = tempfile::tempdir().expect("original credential removal fixture");
    // The inspected manifest supplies all three keys, including the absent optional entry.
    // 已检查清单提供全部三个键，包括缺失可选条目。
    let (store, package) = fixture(temp.path());
    // Only secure backend bytes are represented in memory.
    // 仅安全后端字节以内存表示。
    let backend = Backend::default();
    // Durable inventory and actual staging complete before selection or removal reservation.
    // 持久清单及实际暂存在选择或移除预留之前完成。
    let revision = stage(&store, &package, &backend);
    // The actual shared revision File is distinct from the package lease.
    // 实际共享修订文件独立于包租约。
    let reader = store
        .pin_credentials(&package, &revision)
        .expect("retain actual credential reader");
    // One original captures the full verified journal before any cleanup.
    // 单个原实例在任何清理之前捕获完整已验证日志。
    let mut original = reserve(&store, &package, &revision);
    // Counters cannot substitute for the actual lease; they merely prove no backend call occurred.
    // 计数不能替代实际租约；其仅证明没有发生后端调用。
    let observed = CleanupBackend {
        inner: &backend,
        deletes: Cell::new(0),
        fault: Cell::new(None),
        skip_delete: false,
    };
    assert_eq!(
        original.credential_keys(),
        &[key("first"), key("optional"), key("second")]
    );
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("unpublished original cannot delete credentials")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    store
        .publish_deactivation_retained(&mut original)
        .expect("publish actual original tombstone");
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("actual credential reader must block every backend effect")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(observed.deletes.get(), 0);
    drop(package);
    assert!(
        store
            .collect_deactivation_package_retained(&original)
            .expect("collect package independently of credential reader")
    );
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("reader remains real after package bytes disappear")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(reader);
    store
        .collect_deactivation_credentials_retained(&original, &observed)
        .expect("collect full frozen inventory after real reader release");
    assert_eq!(observed.deletes.get(), 3);
    assert!(backend.values.borrow().is_empty());
    store
        .collect_deactivation_credentials_retained(&original, &observed)
        .expect("confirm actual absence without deletion replay");
    assert_eq!(observed.deletes.get(), 3);
    assert_eq!(
        store
            .read_credential_record_locked("staging-plugin", &revision)
            .expect("read actual journal")
            .expect("original journal persists")
            .phase,
        VulcanPluginCredentialPhase::Discarded
    );
    // A noncooperating resurrection cannot authorize new deletion under an already completed cleanup.
    // 非协作复活不能在已完成清理内授权新删除。
    backend.values.borrow_mut().insert(
        (revision.clone(), key("first")),
        "foreign-reintroduced-value".into(),
    );
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("terminal confirmation must refuse actual present value")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(observed.deletes.get(), 3);
    assert!(
        backend
            .values
            .borrow()
            .contains_key(&(revision, key("first")))
    );
    // The current and historical entry points share the same terminal watermark and cannot alternate to replay deletes.
    // 当前及历史入口共享相同终态水位，不能交替重放删除。
    let revision = original
        .credential_inventory()
        .iter()
        .find(|record| record.phase() == VulcanPluginCredentialPhase::Staged)
        .expect("actual staged original remains captured")
        .revision()
        .clone();
    // A full original staged record is restored only as an explicit noncooperating mutation test.
    // 完整原已暂存记录仅作为明确非协作变更测试被恢复。
    let staged_record = original
        .credential_inventory()
        .iter()
        .find(|record| record.revision() == &revision)
        .expect("actual original complete staged record")
        .clone();
    store
        .write_credential_record_locked(&staged_record)
        .expect("deliberate rollback to original staged phase");
    assert_eq!(
        store
            .collect_deactivation_inventory_credentials_retained(&original, &revision, &observed)
            .expect_err("historical entry cannot reset current entry terminal progress")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("current entry cannot replay after terminal rollback")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(observed.deletes.get(), 3);
}

/// A post-effect error or panic keeps the same original namespace and resumes only its durable exact inventory.
/// 效果后错误或 panic 保持相同原命名空间，仅恢复其持久精确清单。
#[test]
fn retained_deactivation_credentials_resume_partial_delete_without_replacing_original() {
    for panic in [false, true] {
        // Each fault uses separate actual package, journal and namespace files.
        // 每个故障使用独立实际包、日志及命名空间文件。
        let temp = tempfile::tempdir().expect("partial deletion fixture");
        // One inspected original supplies the same complete tuple inventory throughout recovery.
        // 单个已检查原来源跨恢复提供相同完整元组清单。
        let (store, package) = fixture(temp.path());
        // Backend state records the real fixture deletion before the fault is raised.
        // 后端状态在故障引发之前记录真实夹具删除。
        let backend = Backend::default();
        // The exact original revision remains unchanged across both attempts.
        // 精确原修订跨两次尝试保持未变。
        let revision = stage(&store, &package, &backend);
        // The original remains outside the unwinding cleanup call.
        // 原实例保持位于展开清理调用之外。
        let mut original = reserve(&store, &package, &revision);
        store
            .publish_deactivation_retained(&mut original)
            .expect("publish exact original tombstone");
        // A one-shot fault occurs after actual deletion of the first original key.
        // 单次故障在实际删除首个原键之后发生。
        let observed = CleanupBackend {
            inner: &backend,
            deletes: Cell::new(0),
            fault: Cell::new(Some(panic)),
            skip_delete: false,
        };
        // Catching the real panic proves the outer original File was not consumed by backend unwinding.
        // 捕获真实 panic 证明外部原文件未被后端展开消费。
        let result = catch_unwind(AssertUnwindSafe(|| {
            store.collect_deactivation_credentials_retained(&original, &observed)
        }));
        if panic {
            assert!(result.is_err());
        } else {
            // Secret-bearing fixture failures must be redacted by the production cleanup path.
            // 包含秘密的夹具失败必须由生产清理路径脱敏。
            let error = result
                .expect("error path does not panic")
                .expect_err("lose actual deletion acknowledgement");
            assert!(!error.to_string().contains("fixture-sensitive"));
        }
        assert_eq!(observed.deletes.get(), 1);
        assert!(
            !backend
                .values
                .borrow()
                .contains_key(&(revision.clone(), key("first")))
        );
        assert!(
            backend
                .values
                .borrow()
                .contains_key(&(revision.clone(), key("second")))
        );
        assert_eq!(
            store
                .read_credential_record_locked("staging-plugin", &revision)
                .expect("retained journal")
                .expect("actual durable intent survives fault")
                .phase,
            VulcanPluginCredentialPhase::Discarding
        );
        // A separately opened contender proves physical exclusion, without sharing a registry with the original.
        // 单独打开的竞争者证明物理互斥，不与原实例共享注册表。
        let contender =
            VulcanPluginGenerationStore::open(temp.path()).expect("independent actual contender");
        assert_eq!(
            contender
                .activation("staging-plugin")
                .expect_err("same original File survives fault")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect("resume exact original Discarding inventory");
        assert_eq!(observed.deletes.get(), 4);
        assert!(backend.values.borrow().is_empty());
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect("terminal reconciliation performs no deletes");
        assert_eq!(observed.deletes.get(), 4);
    }
}

/// A backend reporting deletion success cannot advance a journal when the actual exact value remains present.
/// 后端报告删除成功时，若实际精确值仍存在，不能推进日志。
#[test]
fn retained_deactivation_credentials_verify_actual_absence_before_terminal_checkpoint() {
    // Real journal transitions are separate from the explicitly dishonest memory backend.
    // 实际日志转换独立于明确不诚实的内存后端。
    let temp = tempfile::tempdir().expect("false-success deletion fixture");
    // Actual source verification fixes the original complete key list.
    // 实际来源验证固定原完整键清单。
    let (store, package) = fixture(temp.path());
    // Both configured values remain actually present in this backend.
    // 两个已配置值在此后端均实际存在。
    let backend = Backend::default();
    // No invented journal or selection grants deletion authority.
    // 没有虚构日志或选择授予删除权威。
    let revision = stage(&store, &package, &backend);
    // This original retains its actual namespace before the dishonest effect.
    // 此原实例在不诚实效果之前保留实际命名空间。
    let mut original = reserve(&store, &package, &revision);
    store
        .publish_deactivation_retained(&mut original)
        .expect("publish exact original removal");
    // The explicit test mode skips deletion while returning success, exercising production readback.
    // 明确测试模式跳过删除但返回成功，以验证生产回读。
    let observed = CleanupBackend {
        inner: &backend,
        deletes: Cell::new(0),
        fault: Cell::new(None),
        skip_delete: true,
    };
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("present original key prevents terminal journal")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(observed.deletes.get(), 1);
    assert_eq!(backend.values.borrow().len(), 2);
    assert_eq!(
        store
            .read_credential_record_locked("staging-plugin", &revision)
            .expect("read actual checkpoint")
            .expect("same original journal persists")
            .phase,
        VulcanPluginCredentialPhase::Discarding
    );
}

/// Foreign complete inventory from a real producer fails before secure effects and is never relabeled by the original.
/// 来自真实生产者的外来完整清单在安全效果之前失败，且绝不由原实例重新标记。
#[test]
fn retained_deactivation_credentials_reject_foreign_actual_journal_before_backend_effects() {
    // The original store owns its actual canonical namespace independently of the foreign producer.
    // 原存储独立于外来生产者拥有实际规范命名空间。
    let temp = tempfile::tempdir().expect("foreign inventory original fixture");
    // Original metadata comes from the same inspected source used for activation.
    // 原元数据来自激活使用的相同已检查来源。
    let (store, package) = fixture(temp.path());
    // Actual backend values belong only to the original secure revision.
    // 实际后端值仅属于原安全修订。
    let backend = Backend::default();
    // Keep the sole original revision across the deliberate noncooperating journal replacement.
    // 跨刻意非协作日志替换保持唯一原修订。
    let revision = stage(&store, &package, &backend);
    // Actual original removal captures the complete verified metadata before replacement.
    // 实际原移除在替换之前捕获完整已验证元数据。
    let mut original = reserve(&store, &package, &revision);
    store
        .publish_deactivation_retained(&mut original)
        .expect("publish original removal");
    // A real separate producer generates structurally valid metadata with its own physical package identity.
    // 真实独立生产者生成结构有效元数据，携带其自己的物理包身份。
    let foreign_temp = tempfile::tempdir().expect("real foreign producer fixture");
    // Matching manifest bytes do not confer original physical package ownership.
    // 匹配清单字节不授予原物理包所有权。
    let (foreign_store, foreign_package) = fixture(foreign_temp.path());
    // Actual staging writes the foreign record using the original revision identifier, without inventing record fields.
    // 实际暂存使用原修订标识写入外来记录，不虚构记录字段。
    let mut foreign_writer = foreign_store
        .begin_credential_staging(&foreign_package, revision.clone(), &values())
        .expect("real foreign staging producer");
    foreign_writer
        .write_and_finish(&Backend::default(), &values())
        .expect("finish actual foreign inventory");
    drop(foreign_writer);
    // The strict actual foreign journal is copied only as a noncooperating mutation test fixture.
    // 严格实际外来日志仅作为非协作变更测试夹具被复制。
    let foreign = foreign_store
        .read_credential_record_locked("staging-plugin", &revision)
        .expect("read real foreign journal")
        .expect("actual foreign journal exists");
    assert_ne!(foreign.generation(), Some(package.identity()));
    store
        .write_credential_record_locked(&foreign)
        .expect("inject actual foreign inventory");
    // Observe that complete metadata refusal precedes every secure deletion.
    // 观察完整元数据拒绝先于每个安全删除。
    let observed = CleanupBackend {
        inner: &backend,
        deletes: Cell::new(0),
        fault: Cell::new(None),
        skip_delete: false,
    };
    assert_eq!(
        store
            .collect_deactivation_credentials_retained(&original, &observed)
            .expect_err("foreign package identity is not the original inventory")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(observed.deletes.get(), 0);
    assert_eq!(backend.values.borrow().len(), 2);
    assert_eq!(
        store
            .read_credential_record_locked("staging-plugin", &revision)
            .expect("read untouched foreign journal"),
        Some(foreign)
    );
}

/// Historical unfinished cleanup retains its exact original across actual post-delete errors/panics and never repeats terminal effects.
/// 历史未完成清理跨实际删除后错误／panic 保留精确原实例，并绝不重复终态效果。
#[test]
fn retained_deactivation_inventory_credentials_resume_actual_partial_preparing_history() {
    for panic in [false, true] {
        // Each fault has independent physical package, journal and namespace ownership.
        // 每个故障具有独立物理包、日志及命名空间所有权。
        let temp = tempfile::tempdir().expect("actual partial historical cleanup fixture");
        // The actual inspector supplies complete required and optional slots.
        // 实际检查器提供完整必需及可选槽位。
        let (store, package) = fixture(temp.path());
        // The secure map records one actual value before the staging failure.
        // 安全映射在暂存失败之前记录单个实际值。
        let backend = Backend::default();
        *backend.fail_write.borrow_mut() = Some("second".into());
        // This real writer persists the full journal before any secure write and remains Preparing after the failure.
        // 此真实写入者在任何安全写入之前持久保存完整日志，并在失败之后保持准备中。
        let revision = VulcanPluginCredentialRevision::allocate();
        // The actual outer writer survives its failed write and is explicitly released before historical collection.
        // 实际外部写入者跨失败写入存续，并在历史回收之前被明确释放。
        let mut writer = store
            .begin_credential_staging(&package, revision.clone(), &values())
            .expect("actual historical journal preparation");
        writer
            .write_and_finish(&backend, &values())
            .expect_err("actual partial staging write");
        drop(writer);
        assert_eq!(backend.values.borrow().len(), 1);
        // The ordinary selection owns a separate revision, leaving the failed one as pure history.
        // 普通选择拥有独立修订，使失败修订成为纯历史。
        let selected = store
            .activate_for_test(&package, 0, package.required_permissions.clone())
            .expect("actual ordinary source selection");
        // One concrete original retains the entire historical inventory outside backend unwinding.
        // 单个具体原实例在后端展开之外保留整个历史清单。
        let mut original = store
            .reserve_deactivation(Uuid::new_v4(), &selected)
            .expect("actual historical original capture");
        store
            .publish_deactivation_retained(&mut original)
            .expect("actual removal publication");
        // The fixture fault consumes only the acknowledgement after an actual exact tuple deletion.
        // 夹具故障仅在实际精确元组删除之后消费确认。
        let observed = CleanupBackend {
            inner: &backend,
            deletes: Cell::new(0),
            fault: Cell::new(Some(panic)),
            skip_delete: false,
        };
        assert_eq!(
            store
                .confirm_deactivation_inventory_collected_retained(&original, &observed)
                .expect_err("terminal confirmation cannot start unfinished cleanup")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(observed.deletes.get(), 0);
        // Catch real backend unwinding while the actual original remains strongly retained by this outer scope.
        // 捕获真实后端展开，同时实际原实例由此外部作用域强保留。
        let result = catch_unwind(AssertUnwindSafe(|| {
            store.collect_deactivation_inventory_credentials_retained(
                &original, &revision, &observed,
            )
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result
                    .expect("error path returns")
                    .expect_err("actual lost deletion acknowledgement")
                    .code,
                VulcanPluginStoreErrorCode::Storage
            );
        }
        assert_eq!(observed.deletes.get(), 1);
        // Save the actual irreversible checkpoint, then restore original Preparing bytes to test preterminal regression exclusion.
        // 保存实际不可逆检查点，再恢复原准备中字节，以验证终态前回退互斥。
        let discarding = store
            .read_credential_record_locked("staging-plugin", &revision)
            .expect("actual retained progress readback")
            .expect("actual journal survives failure");
        assert_eq!(discarding.phase(), VulcanPluginCredentialPhase::Discarding);
        store
            .write_credential_record_locked(
                original
                    .credential_inventory()
                    .iter()
                    .find(|record| record.revision() == &revision)
                    .expect("actual complete original Preparing record"),
            )
            .expect("deliberate preterminal phase regression");
        assert_eq!(
            store
                .collect_deactivation_inventory_credentials_retained(
                    &original, &revision, &observed
                )
                .expect_err("same original rejects rollback below acknowledged Discarding")
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        assert_eq!(observed.deletes.get(), 1);
        store
            .write_credential_record_locked(&discarding)
            .expect("restore actual checkpoint for explicit test recovery");
        // An independent actual store cannot acquire the retained namespace after an error or panic.
        // 错误或 panic 之后，独立实际存储不能取得保留命名空间。
        let contender = VulcanPluginGenerationStore::open(temp.path())
            .expect("actual independent history contender");
        assert_eq!(
            contender
                .activation("staging-plugin")
                .expect_err("same actual original File survives failure")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        store
            .collect_deactivation_inventory_credentials_retained(&original, &revision, &observed)
            .expect("resume exact historical Discarding journal");
        assert_eq!(observed.deletes.get(), 4);
        // The selected revision retires through its own entry point, outside the observed fault backend.
        // 已选修订经自身入口退役，位于被观察故障后端之外。
        store
            .collect_deactivation_credentials_retained(&original, &backend)
            .expect("collect selected revision");
        drop(package);
        store
            .collect_deactivation_package_retained(&original)
            .expect("actual package collection");
        store
            .confirm_deactivation_inventory_collected_retained(&original, &observed)
            .expect("complete actual terminal histories");
        store
            .collect_deactivation_inventory_credentials_retained(&original, &revision, &observed)
            .expect("terminal read-only reconciliation");
        assert_eq!(observed.deletes.get(), 4);
        // A resurrected optional value is still part of the complete frozen inventory and cannot authorize terminal deletion replay.
        // 复活可选值仍属于完整固定清单，不能授权终态删除重放。
        backend.values.borrow_mut().insert(
            (revision.clone(), key("optional")),
            "foreign optional value".into(),
        );
        assert_eq!(
            store
                .confirm_deactivation_inventory_collected_retained(&original, &observed)
                .expect_err("terminal readback must reject optional resurrection")
                .code,
            VulcanPluginStoreErrorCode::Storage
        );
        assert_eq!(observed.deletes.get(), 4);
        assert!(
            backend
                .values
                .borrow()
                .contains_key(&(revision, key("optional")))
        );
    }
}

/// Complete historical metadata and final membership are required before terminal success, with no backend deletes on conflict.
/// 终态成功之前要求完整历史元数据及最终成员关系，冲突时不发生后端删除。
#[test]
fn retained_deactivation_inventory_credentials_refuse_changed_keys_and_added_actual_journals() {
    // Both original and foreign records come from actual independent production writers.
    // 原及外来记录均来自实际独立生产写入者。
    let temp = tempfile::tempdir().expect("actual full historical metadata fixture");
    // The immutable original fixes complete package identity and manifest-declared optional keys.
    // 不可变原包固定完整包身份及清单声明可选键。
    let (store, package) = fixture(temp.path());
    // Only secure values use memory while all journals and leases are real.
    // 仅安全值使用内存，全部日志及租约均为真实。
    let backend = Backend::default();
    // This actual staged revision is the sole journal and the current selection's credential authority.
    // 此实际已暂存修订是唯一日志，也是当前选择的凭据权威。
    let revision = stage(&store, &package, &backend);
    // The actual selection producer supplies all original removal fields.
    // 实际选择生产者提供全部原移除字段。
    let selected = store
        .activate_with_credentials_for_test(
            &package,
            0,
            package.required_permissions.clone(),
            revision.clone(),
        )
        .expect("actual current source");
    // One actual original captures the journal before any mutation.
    // 单个实际原实例在任何变更之前捕获日志。
    let mut original = store
        .reserve_deactivation(Uuid::new_v4(), &selected)
        .expect("actual full history original");
    store
        .publish_deactivation_retained(&mut original)
        .expect("actual original publication");
    // Preserve real complete metadata as evidence to restore only this test's deliberate mutation.
    // 保留真实完整元数据作为证据，仅恢复此测试刻意变更。
    let expected = store
        .read_credential_record_locked("staging-plugin", &revision)
        .expect("actual complete journal")
        .expect("journal exists");
    // Removing an optional slot remains structurally valid but must fail the complete original metadata comparison.
    // 移除可选槽位保持结构有效，但必须未通过完整原元数据比较。
    let mut changed = expected.clone();
    changed.keys.retain(|entry| entry != &key("optional"));
    store
        .write_credential_record_locked(&changed)
        .expect("deliberate actual optional inventory mutation");
    // The observer proves conflict occurs before any secure effect.
    // 观察者证明冲突发生于任何安全效果之前。
    let observed = CleanupBackend {
        inner: &backend,
        deletes: Cell::new(0),
        fault: Cell::new(None),
        skip_delete: false,
    };
    assert_eq!(
        store
            .collect_deactivation_inventory_credentials_retained(&original, &revision, &observed)
            .expect_err("complete optional inventory cannot change")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(observed.deletes.get(), 0);
    store
        .write_credential_record_locked(&expected)
        .expect("restore actual producer metadata for test");
    store
        .collect_deactivation_inventory_credentials_retained(&original, &revision, &observed)
        .expect("actual full historical cleanup");
    drop(package);
    store
        .collect_deactivation_package_retained(&original)
        .expect("actual original package collection");
    store
        .confirm_deactivation_inventory_collected_retained(&original, &observed)
        .expect("actual terminal census before foreign history");
    // A second actual producer creates a valid final journal that was absent from the admitted original inventory.
    // 第二实际生产者创建有效最终日志，该日志未出现在已准入原清单中。
    let foreign_temp = tempfile::tempdir().expect("actual foreign historical producer");
    // Physical foreign ownership stays independent from the retained original namespace.
    // 物理外来所有权独立于保留原命名空间。
    let (foreign_store, foreign_package) = fixture(foreign_temp.path());
    // The actual foreign writer supplies a complete schema-valid journal rather than hand-built recovery metadata.
    // 实际外来写入者提供完整结构有效日志，而非手写恢复元数据。
    let foreign_revision = VulcanPluginCredentialRevision::allocate();
    drop(
        foreign_store
            .begin_credential_staging(&foreign_package, foreign_revision.clone(), &values())
            .expect("actual foreign final journal"),
    );
    // Sole strict reading supplies exactly the actual foreign producer fields.
    // 唯一严格读取提供精确实际外来生产者字段。
    let foreign_record = foreign_store
        .read_credential_record_locked("staging-plugin", &foreign_revision)
        .expect("actual foreign journal read")
        .expect("foreign journal exists");
    store
        .write_credential_record_locked(&foreign_record)
        .expect("noncooperating introduction of actual foreign history");
    assert_eq!(
        store
            .confirm_deactivation_inventory_collected_retained(&original, &observed)
            .expect_err("full census rejects history added after capture")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(observed.deletes.get(), 3);
    assert_eq!(
        store
            .read_credential_record_locked("staging-plugin", &foreign_revision)
            .expect("foreign history remains intact"),
        Some(foreign_record)
    );
}

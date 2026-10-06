//! Actual predecessor sources share the independent reservation without writing or opening business admission.
//! 实际前驱来源共用独立预留权，不写入记录或开放业务入场。

use super::*;

/// Sign and retry one actual source without rewriting Pending or substituting target configuration.
/// 签发及重试单个实际来源，不重写 Pending 或替换为目标配置。
/// No inputs or return value; real file contention, exact Arc identity and old successor inputs are asserted.
/// 无输入与返回值；断言真实文件争用、精确 Arc 身份及旧代后继输入。
#[test]
fn predecessor_physical_source_retries_same_arc_without_rewriting_pending() {
    // Reuse the existing production preparation and completed old resource drain.
    // 复用既有生产准备及已完成旧资源排空。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("real independent old reservation");
    // The old routing generation is frozen before actual abort produces its closed successor.
    // 在实际中止产生关闭后继前，固定旧路由代次。
    let predecessor_generation = restoration.predecessor().generation;
    // Derive both physical paths solely from the actual retained reservation.
    // 两个物理路径仅从实际保留预留权派生。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("Pending path");
    // A separately opened descriptor must contend with the independent original File.
    // 独立打开的描述符必须与独立原文件争用。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("lease path"),
        )
        .expect("actual independent reservation file");
    // The actual original transaction, not reconstructed identity, performs abort staging.
    // 实际原事务执行中止暂存，而非重建身份。
    let staged = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual closed predecessor successor");
    // Capture existing bytes after abort to detect any forbidden factory bookkeeping.
    // 中止后捕获已有字节，以检测任何禁止的工厂记录变更。
    let pending_bytes = std::fs::read(&record_path).expect("existing Pending bytes");
    // This actual writer obstruction would fail the call if source signing rewrote Pending.
    // 若来源签发重写 Pending，此实际写入器阻挡会使调用失败。
    let no_rewrite = store.obstruct_predecessor_handoff_write_once(restoration.request_id());
    // The first factory returns authority only after it retained the existing physical owner.
    // 首次工厂仅在保留既有物理所有者后返回权威。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("sign source without writing");
    // Retry must return the same allocation rather than another equivalent reservation.
    // 重试必须返回相同分配，而非另一等价预留权。
    let retry = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same source without writing");
    drop(no_rewrite);
    assert!(Arc::ptr_eq(&source, &retry));
    assert!(Arc::ptr_eq(
        &source,
        restoration
            .source
            .as_ref()
            .expect("retained original source")
    ));
    assert!(Arc::ptr_eq(source.staged_catalog(), &staged));
    assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &staged));
    assert_eq!(source.request_id(), transaction.record().request_id());
    assert_eq!(source.original(), transaction.record());
    assert!(source.plugin().generation > predecessor_generation);
    assert_eq!(
        source.plugin().generation,
        staged.plugins["hook-plugin"].generation
    );
    assert_eq!(
        source.plugin().runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    assert_eq!(
        source
            .plugin()
            .managed_package()
            .expect("old immutable pins")
            .configuration()
            .value(),
        Some(&json!({"label":"old-published"}))
    );
    assert!(
        source
            .plugin()
            .managed_package()
            .expect("old managed selection")
            .matches_activation(
                transaction
                    .record()
                    .predecessor()
                    .expect("original old selection")
            )
    );
    assert_eq!(
        std::fs::read(&record_path).expect("same Pending bytes"),
        pending_bytes
    );
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(staged.enabled_tools.is_empty());
    assert!(staged.enabled_hooks.is_empty());
    assert!(staged.enabled_mcp_servers.is_empty());
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("Pending admission fence")
    );
}

/// Dropping original service handles leaves the source's physical File owned until its last Arc is released.
/// 释放原服务句柄后，来源的物理文件保持自有，直至其最后 Arc 被释放。
/// No inputs or return value; final physical release still leaves durable Pending blocking ordinary admission.
/// 无输入与返回值；最终物理释放后，持久 Pending 仍阻挡普通入场。
#[test]
fn predecessor_physical_source_outlives_restoration_and_preserves_pending_fence() {
    // Obtain actual original owners without inventing a replacement restoration lease.
    // 取得实际原所有者，不创建替代恢复租约。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("independent physical owner");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual Aborted staging");
    // This descriptor observes the original independent reservation after every owner drop.
    // 此描述符在每次所有者释放后观测原独立预留权。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("original lease path"),
        )
        .expect("separate physical File");
    // The source retains the actual owner independently of both original non-cloneable handles.
    // 来源独立于两个原不可克隆句柄，保留实际所有者。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("physical source");
    // Capture the real selected activation revision before dropping its source.
    // 释放来源前捕获真实所选激活修订。
    let old_revision = source
        .original()
        .predecessor()
        .expect("real old selection")
        .revision;
    // Retain only a second source Arc to distinguish one drop from the final physical release.
    // 仅保留第二个来源 Arc，以区分一次释放与最终物理释放。
    let last_source = Arc::clone(&source);
    drop(restoration);
    drop(transaction);
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(source);
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("source remains closed")
    );
    drop(last_source);
    competing
        .try_lock()
        .expect("final source drop releases the independent File");
    competing.unlock().expect("release only the probe lock");
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("durable Pending survives owner loss")
    );
    assert_eq!(
        store
            .deactivate("hook-plugin", old_revision)
            .expect_err("ordinary writer cannot bypass ownerless Pending")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    assert!(fixture.manager.snapshot().enabled_tools.is_empty());
}

/// Foreign managers, foreign actual originals and equal-field catalog allocations cannot replace a signed source.
/// 外来管理器、外来实际原请求及字段相同目录分配，均不能替换已签发来源。
/// No inputs or return value; restoring only the real original catalog makes the same source retry valid again.
/// 无输入与返回值；仅恢复真实原目录后，相同来源重试再次有效。
#[test]
fn predecessor_physical_source_rejects_foreign_owners_and_replaced_catalog_arc() {
    // The original fixture owns an actual fully drained managed predecessor.
    // 原夹具拥有实际完全排空的托管前驱。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("original Pending");
    // Preserve the exact Arc emitted by actual original abort staging.
    // 保留实际原中止暂存产出的精确 Arc。
    let staged = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("original closed staging");
    // Sign the source once so rejected retries must preserve its already retained allocation.
    // 仅签发一次来源，使被拒绝重试必须保留其已保留分配。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("original physical source");
    // This foreign fixture supplies its own real transaction and manager incarnation.
    // 此外来夹具提供其自有真实事务及管理器实例。
    let (foreign, _foreign_store, mut foreign_transaction, mut foreign_restoration) =
        private_owner_fixture();
    foreign
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut foreign_transaction,
            &mut foreign_restoration,
            false,
            false,
        )
        .expect("foreign actual Pending");
    foreign
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut foreign_transaction,
            &mut foreign_restoration,
            false,
            false,
        )
        .expect("foreign actual Aborted original");
    assert!(
        foreign
            .manager
            .prepare_activation_predecessor_source(&transaction, &mut restoration,)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_source(&foreign_transaction, &mut restoration,)
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &source,
        restoration
            .source
            .as_ref()
            .expect("original source retained")
    ));
    // A deliberate actual snapshot replacement has equal descriptor fields but a different allocation identity.
    // 有意替换实际快照，其描述符字段相同，但分配身份不同。
    let different_arc = Arc::new(staged.as_ref().clone());
    assert!(!Arc::ptr_eq(&different_arc, &staged));
    *fixture
        .manager
        .state
        .snapshot
        .write()
        .expect("actual manager snapshot writer") = Arc::clone(&different_arc);
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_source(&transaction, &mut restoration,)
            .is_err()
    );
    assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &different_arc));
    assert!(Arc::ptr_eq(
        &source,
        restoration.source.as_ref().expect("source never replaced")
    ));
    *fixture
        .manager
        .state
        .snapshot
        .write()
        .expect("restore actual original snapshot") = Arc::clone(&staged);
    // Only restoration of the previously emitted original Arc allows the same authority to be reused.
    // 仅恢复先前产出的原 Arc，才允许复用相同权威。
    let retry = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same source after original Arc restoration");
    assert!(Arc::ptr_eq(&retry, &source));
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("still Pending")
    );
}

/// Prepared original transactions cannot sign a source before actual abort and closed successor staging.
/// 实际中止及关闭后继暂存前，Prepared 原事务不能签发来源。
/// No inputs or return value; the real Pending bytes, original transaction and manager Arc remain unchanged.
/// 无输入与返回值；真实 Pending 字节、原事务及管理器 Arc 保持不变。
#[test]
fn predecessor_physical_source_rejects_prepared_original_without_mutation() {
    // Keep the real original Prepared while reserving its actual independent old resources.
    // 在预留实际独立旧资源时，保持真实原 Prepared。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual Pending before abort");
    // Existing bytes and the full typed original are the mutation observations.
    // 已有字节及完整类型化原请求是变更观测依据。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // Read physical bytes rather than trusting an equivalent parsed record.
    // 读取物理字节，而非依赖等价解析记录。
    let pending_bytes = std::fs::read(&record_path).expect("actual existing bytes");
    // Preserve every actual original field for strict before/after equality.
    // 保留每个实际原字段，以严格比较前后相等性。
    let original = transaction.record().clone();
    // The actual manager Arc must not be replaced by a speculative staged successor.
    // 实际管理器 Arc 不得被推测暂存后继替换。
    let current = fixture.manager.snapshot();
    assert_eq!(
        original.phase(),
        crate::VulcanPluginActivationTransactionPhase::Prepared
    );
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_source(&transaction, &mut restoration,)
            .is_err()
    );
    assert!(restoration.source.is_none());
    assert!(restoration.staged_catalog().is_none());
    assert_eq!(transaction.record(), &original);
    assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &current));
    assert_eq!(
        std::fs::read(&record_path).expect("unchanged physical bytes"),
        pending_bytes
    );
    assert_eq!(
        store
            .activation_transaction("hook-plugin", original.request_id())
            .expect("actual original on disk")
            .expect("original exists"),
        original
    );
}

/// Missing actual Pending rejects a source retry without rewriting or replacing its retained physical ownership.
/// 实际 Pending 缺失时拒绝来源重试，不重写或替换已保留物理所有权。
/// No inputs or return value; restoring the same renamed file permits only the same retained source Arc.
/// 无输入与返回值；恢复相同已重命名文件后，仅允许复用相同保留来源 Arc。
#[test]
fn predecessor_physical_source_missing_pending_does_not_repair_disk() {
    // Reuse the exact original completed drain and independent old resource reservation.
    // 复用精确原已完成排空及独立旧资源预留权。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual old Pending");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual Aborted staging");
    // Sign once before removing only the fixture's real sidecar file from its exact expected path.
    // 仅从精确预期路径移走夹具真实附属文件前，签发一次来源。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("original retained source");
    // Obtain the actual sidecar path from its existing physical reservation.
    // 从既有物理预留权取得实际附属记录路径。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // This sibling path retains the same actual file for deterministic restoration without reconstructing bytes.
    // 此同级路径保留相同实际文件，供确定性恢复，不重建字节。
    let retained_path = record_path.with_extension("source-fixture-retained");
    // Preserve original bytes to prove the restored file and rejected factory remain unchanged.
    // 保留原字节，以证明恢复文件及被拒绝工厂保持不变。
    let pending_bytes = std::fs::read(&record_path).expect("original Pending bytes");
    // A separate actual descriptor proves source rejection does not release its lease.
    // 独立实际描述符证明来源拒绝不会释放其租约。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual lease path"),
        )
        .expect("real separate File");
    assert!(!retained_path.exists());
    std::fs::rename(&record_path, &retained_path)
        .expect("retain same actual Pending file off-path");
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_source(&transaction, &mut restoration,)
            .is_err()
    );
    assert!(
        !record_path.exists(),
        "factory must not rewrite missing Pending"
    );
    assert_eq!(
        std::fs::read(&retained_path).expect("same retained file bytes"),
        pending_bytes
    );
    assert!(Arc::ptr_eq(
        &source,
        restoration
            .source
            .as_ref()
            .expect("same retained original source")
    ));
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    std::fs::rename(&retained_path, &record_path).expect("restore the same actual Pending file");
    // Arm the existing writer obstruction so a retry can succeed only through read-only validation.
    // 启用既有写入器阻挡，使重试仅能通过只读校验成功。
    let no_rewrite = store.obstruct_predecessor_handoff_write_once(restoration.request_id());
    // The factory must return the original source after exact physical evidence is restored.
    // 精确物理证据恢复后，工厂必须返回原来源。
    let retry = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same source after restoring exact Pending file");
    drop(no_rewrite);
    assert!(Arc::ptr_eq(&source, &retry));
    assert_eq!(
        std::fs::read(&record_path).expect("unchanged restored bytes"),
        pending_bytes
    );
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("actual restored fence")
    );
}

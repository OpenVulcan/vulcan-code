//! Real restoration handles retain Pending reservation files without callback replay or public admission.
//! 真实恢复句柄保留 Pending 预留文件，不重放回调或开放公共入场。

use super::callbacks::callback_fixture;
use super::*;
use crate::VulcanPluginPredecessorRestoration;

mod hooks;
mod mcp;
mod publication;
mod source;

/// Build a physically drained managed predecessor without an old callback declaration.
/// 构建物理已排空且未声明旧回调的托管前驱。
/// Return actual fixture/store/transaction/restoration owners; no readiness lease remains borrowed.
/// 返回实际夹具、存储、事务及恢复所有者；不保留借用的就绪租约。
fn private_owner_fixture() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    VulcanPluginPredecessorRestoration,
) {
    // Every returned handle comes from production immutable preparation and actual drain.
    // 每个返回句柄均来自生产不可变准备及实际排空。
    let (fixture, store, transaction, drain) = callback_fixture(false, false, false);
    // Borrow the real original just long enough to validate its already completed restoration decision.
    // 仅短暂借用真实原请求，以校验其已完成恢复决定。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("actual candidate");
    // The absent old callback declaration remains explicit evidence rather than fabricated execution.
    // 旧回调声明缺失保持为明确证据，而非伪造执行。
    let restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("actual old restoration authority");
    drop(candidate);
    (fixture, store, transaction, restoration)
}

/// Lost Pending observations retain the same exclusive reservation and exact old callback receipt.
/// 丢失 Pending 观测时保留相同排他预留权及精确旧回调回执。
/// No parameters or return value; a real cross-handle file probe proves physical ownership after both calls.
/// 无参数与返回值；真实跨句柄文件探测证明两次调用后的物理所有权。
#[test]
fn predecessor_private_owner_unknown_pending_retains_same_file_and_does_not_replay() {
    // The declared old callback executes once before storage reservation starts.
    // 已声明旧回调在存储预留开始前仅执行一次。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, true);
    // Keep the exact original readiness only through the real old callbacks.
    // 仅在真实旧回调期间保留精确原就绪状态。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .is_err()
        );
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("one old restore");
    });
    // The actual typed restoration replaces borrowed candidate readiness before reservation.
    // 预留前，实际类型化恢复替代借用候选就绪状态。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("restoration");
    drop(candidate);
    // Arm only the actual successful sidecar write's two lost immediate observations.
    // 仅启用实际成功附属记录写入的两个即时观测丢失。
    let fault = store.lose_predecessor_handoff_observation_once(restoration.request_id());
    let failure = fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect_err("actual Pending write and immediate observations lost");
    assert!(
        failure
            .message
            .contains("Controlled loss of predecessor handoff immediate readback"),
        "{}",
        failure.message
    );
    // The real owner was installed before the fallible writer, not reconstructed after the error.
    // 真实所有者在可失败写入器前安装，不是在错误后重建。
    let original_owner = Arc::clone(
        restoration
            .private_owner
            .as_ref()
            .expect("owner retained after Unknown"),
    );
    // Probe the exact independent file declared by production storage; failed locking proves actual contention.
    // 探测生产存储声明的精确独立文件；锁定失败证明实际争用。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual retained lease path"),
        )
        .expect("existing exact reservation file");
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("closed admission")
    );
    drop(fault);
    // Arm a real writer obstruction that would fail if the confirmed same-owner retry attempted another write.
    // 启用真实写入器阻挡；若已确认相同所有者重试再次写入，则会失败。
    let no_rewrite = store.obstruct_predecessor_handoff_write_once(restoration.request_id());
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("same owner reads back Pending");
    drop(no_rewrite);
    assert!(Arc::ptr_eq(
        &original_owner,
        restoration.private_owner.as_ref().expect("same owner")
    ));
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(restoration.staged_catalog().is_none());
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Prepared
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Draining
    );
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("actual callbacks")
            .iter()
            .filter(|event| event.topic == "enable")
            .count(),
        1
    );
    // Even dropping the restoration cannot release resources while another actual owner Arc retains them.
    // 另一实际所有者 Arc 保留资源时，即使释放恢复句柄也不能释放资源。
    drop(restoration);
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(original_owner);
    competing
        .try_lock()
        .expect("only final actual owner drop releases file");
}

/// A real precommit filesystem obstruction preserves the actual reservation and permits its same-owner first write later.
/// 真实提交前文件系统阻挡保留实际预留权，并允许其相同所有者随后首次写入。
/// No parameters or return value; removing only the fixture's actual empty obstruction directory makes Pending observable.
/// 无参数与返回值；仅移除夹具实际空阻挡目录，使 Pending 可观测。
#[test]
fn predecessor_private_owner_precommit_write_failure_retries_same_actual_owner() {
    // Production preparation supplies immutable old pins and completed actual callback-absence evidence.
    // 生产准备提供不可变旧固定句柄及已完成实际回调缺失证据。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    // The scoped hook acts only at this exact original writer, after real owner retention and before committing bytes.
    // 作用域钩子仅作用于此精确原写入器，在真实所有者保留后、提交字节前发生。
    let obstruction = store.obstruct_predecessor_handoff_write_once(restoration.request_id());
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect_err("real empty directory obstructs the actual Pending writer");
    // The same actual owner survives the filesystem failure with its original exclusive File and immutable pins.
    // 相同实际所有者携带原排他文件及不可变固定句柄跨文件系统失败存活。
    let original_owner = Arc::clone(
        restoration
            .private_owner
            .as_ref()
            .expect("actual precommit owner retained"),
    );
    // Paths come only from the already retained actual reservation and the production path derivations.
    // 路径仅来自已保留实际预留权及生产路径派生。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("actual obstruction destination");
    assert!(
        std::fs::metadata(&record_path)
            .expect("real obstruction")
            .is_dir()
    );
    // A separate actual descriptor proves no replacement lease was allocated or silently released.
    // 独立实际描述符证明未分配替代租约或静默释放。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual lease path"),
        )
        .expect("original exclusive file exists");
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(obstruction);
    // Remove only the exact empty directory created by the scoped fixture; original lease files remain untouched.
    // 仅移除作用域夹具创建的精确空目录；原租约文件保持不变。
    std::fs::remove_dir(&record_path).expect("remove actual empty fixture obstruction");
    assert!(
        store
            .predecessor_handoff_record("hook-plugin", restoration.request_id())
            .expect("strict inspection after removing fixture obstruction")
            .is_none()
    );
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("same actual owner performs its first successful Pending write");
    assert!(Arc::ptr_eq(
        &original_owner,
        restoration
            .private_owner
            .as_ref()
            .expect("same actual owner")
    ));
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        std::fs::metadata(&record_path)
            .expect("actual Pending bytes")
            .is_file()
    );
    assert_eq!(
        store
            .predecessor_handoff_record("hook-plugin", restoration.request_id())
            .expect("confirmed original Pending")
            .expect("actual receipt")
            .phase(),
        crate::VulcanPluginPredecessorHandoffPhase::Pending
    );
    assert!(restoration.staged_catalog().is_none());
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Prepared
    );
}

/// A foreign manager or a different complete transaction cannot allocate a predecessor owner.
/// 外来管理器或不同完整事务不能分配前驱所有者。
/// No parameters or return value; both rejected paths preserve the original restoration and omit Pending evidence.
/// 无参数与返回值；两个拒绝路径均保留原恢复并不产生 Pending 证据。
#[test]
fn predecessor_private_owner_foreign_manager_and_original_are_rejected() {
    // Both fixtures use real independent manager epochs, stores, immutable packages and transactions.
    // 两个夹具使用真实独立管理器实例、存储、不可变包及事务。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    // The foreign original keeps its actual request lock; no UUID-only replacement is constructed.
    // 外来原请求保留实际请求锁；不构造仅基于 UUID 的替代者。
    let (foreign, _foreign_store, mut foreign_transaction, _foreign_restoration) =
        private_owner_fixture();
    assert!(
        foreign
            .manager
            .prepare_activation_predecessor_private_owner(
                &mut transaction,
                &mut restoration,
                false,
                false,
            )
            .expect_err("foreign incarnation")
            .message
            .contains("authority changed")
    );
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_private_owner(
                &mut foreign_transaction,
                &mut restoration,
                false,
                false,
            )
            .expect_err("different full original")
            .message
            .contains("exact unpublished original")
    );
    assert!(restoration.private_owner.is_none());
    assert!(
        store
            .predecessor_handoff_record("hook-plugin", restoration.request_id())
            .expect("read original")
            .is_none()
    );
}

/// Changed explicit membership is rejected before any real reservation or sidecar write.
/// 明确成员关系变化在任何真实预留或附属记录写入前被拒绝。
/// No parameters or return value; later original-policy preparation succeeds with the same restored owner.
/// 无参数与返回值；随后原策略准备使用相同恢复所有者成功。
#[test]
fn predecessor_private_owner_policy_change_preserves_original_unallocated() {
    // The original policy carries default enabled with no explicit enabled or disabled membership.
    // 原策略携带默认启用，且无明确启用或停用成员关系。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    assert!(
        fixture
            .manager
            .prepare_activation_predecessor_private_owner(
                &mut transaction,
                &mut restoration,
                true,
                false,
            )
            .expect_err("policy changed")
            .message
            .contains("policy changed")
    );
    assert!(restoration.private_owner.is_none());
    assert!(
        store
            .predecessor_handoff_record("hook-plugin", restoration.request_id())
            .expect("no pending allocation")
            .is_none()
    );
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("same original policy");
    assert!(restoration.private_owner.is_some());
}

/// The unique full-record matcher rejects schema/intent drift and reversed or published phase edges.
/// 唯一完整记录匹配器拒绝模式、意图变化及逆向或已发布阶段边。
/// No parameters or return value; serialized fixtures vary only declared fields and do not grant execution ownership.
/// 无参数与返回值；序列化夹具仅改变声明字段，不授予执行所有权。
#[test]
fn predecessor_private_owner_full_original_matcher_preserves_schema_and_intent() {
    // Obtain the full typed original from actual immutable update preparation.
    // 从实际不可变更新准备取得完整类型化原记录。
    let (_fixture, _store, transaction, _restoration) = private_owner_fixture();
    // Keep every complete source field while varying only the explicit phase contract.
    // 仅改变明确阶段契约时保留每个完整来源字段。
    let original = transaction.record();
    // Typed decoding preserves the exact known schema fields used by the matcher.
    // 类型化解码保留匹配器使用的精确已知模式字段。
    let encoded = serde_json::to_value(original).expect("encode original");
    for phase in ["prepared", "committing", "aborted"] {
        // A typed phase image is a comparison fixture, never a fabricated physical transaction owner.
        // 类型化阶段映像是比较夹具，绝不是伪造物理事务所有者。
        let mut value = encoded.clone();
        value["phase"] = json!(phase);
        // Decode the full comparison record without constructing any File lease.
        // 解码完整比较记录，不构造任何文件租约。
        let current: crate::VulcanPluginActivationTransactionRecord =
            serde_json::from_value(value).expect("typed phase");
        assert!(current.matches_predecessor_restoration_original(original));
    }
    // Committing cannot become Prepared merely by reusing the original UUID.
    // Committing 不能仅通过复用原 UUID 变为 Prepared。
    let mut committing = encoded.clone();
    committing["phase"] = json!("committing");
    // Bind the complete Committing original for the explicit reverse-edge rejection.
    // 绑定完整 Committing 原记录，以明确拒绝逆向边。
    let committing: crate::VulcanPluginActivationTransactionRecord =
        serde_json::from_value(committing).expect("Committing original");
    assert!(!original.matches_predecessor_restoration_original(&committing));
    for field in ["schema_version", "preparation"] {
        // Drift detection covers complete preparation, beyond the old public policy-only comparison.
        // 变化检测覆盖完整准备，超出旧公开仅策略比较。
        let mut changed = encoded.clone();
        if field == "schema_version" {
            changed[field] = json!(999);
        } else {
            changed[field]["activation_revision"] = json!(u64::MAX);
        }
        // Comparison accepts typed structure only; it never validates this fixture as disk authority.
        // 比较仅接受类型化结构；绝不将此夹具校验为磁盘权威。
        let changed: crate::VulcanPluginActivationTransactionRecord =
            serde_json::from_value(changed).expect("typed drift");
        assert!(!changed.matches_predecessor_restoration_original(original));
    }
    for phase in ["published", "completed"] {
        // Target publication cannot be interpreted as a predecessor restoration successor.
        // 目标发布不能解释为前驱恢复后继。
        let mut value = encoded.clone();
        value["phase"] = json!(phase);
        // Only exact typed fields participate in this phase rejection.
        // 此阶段拒绝仅使用精确类型化字段。
        let current: crate::VulcanPluginActivationTransactionRecord =
            serde_json::from_value(value).expect("typed target phase");
        assert!(!current.matches_predecessor_restoration_original(original));
    }
}

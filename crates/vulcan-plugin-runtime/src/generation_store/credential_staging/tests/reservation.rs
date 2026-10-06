//! Actual reserved files survive first inventory and final secure-write acknowledgement loss.
//! 实际预留文件在首份清单及最终安全写入确认丢失后仍然存续。

use super::*;
use crate::{VulcanPluginActivationPreparation, VulcanPluginConfigurationSelection};

mod inspection;

/// Prepare one real parent and immutable package beneath root; return their unchanged physical owners.
/// 在 root 下准备真实父级及不可变插件包；返回未改变的物理所有者。
fn parent_fixture(
    root: &std::path::Path,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationPreparation,
    PinnedVulcanPluginGeneration,
) {
    // Reuse the production inspector and actual generation publication fixture.
    // 复用生产检查器及实际代次发布夹具。
    let (store, initial) = fixture(root);
    drop(initial);
    // This exact source is created by fixture and supplies all manifest permissions.
    // 此精确来源由夹具创建，并提供全部清单权限。
    let source = root.join("source/staging-plugin");
    // Inspection binds the approved fingerprint to real package bytes.
    // 检查将已批准指纹绑定到真实插件包字节。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("inspect original source");
    // Original parent is durable before any credential inventory or backend operation.
    // 原父级先于任何凭据清单或后端操作持久化。
    let preparation = store
        .begin_activation_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &descriptor.content_fingerprint,
            None,
            crate::runtime::plugin_required_permissions(&descriptor).expect("declared permissions"),
        )
        .expect("persist original parent");
    // The pinned package uses the original parent's immutable allocation.
    // 固定插件包使用原父级不可变分配。
    let generation = store
        .prepare_activation_package(&preparation, &source)
        .expect("publish parent-bound package");
    (store, preparation, generation)
}

/// Derive the one canonical inventory path from store/revision; return no alternate location.
/// 从 store／revision 派生唯一规范清单路径；不返回备用位置。
fn inventory_path(
    store: &VulcanPluginGenerationStore,
    revision: &VulcanPluginCredentialRevision,
) -> std::path::PathBuf {
    store
        .root
        .join("credentials/staging-plugin")
        .join(format!("{}.json", revision.as_str()))
}

/// Keep the actual inventory open without Windows delete sharing; return its physical read handle.
/// 不授予 Windows 删除共享地保持实际清单打开；返回物理读取句柄。
fn pin_inventory(path: &std::path::Path) -> std::fs::File {
    // Atomic replacement would fail on Windows while the readonly retry must still succeed.
    // 此时 Windows 原子替换会失败，而只读重试仍必须成功。
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 2);
    }
    options
        .open(path)
        .expect("pin actual inventory without replacement sharing")
}

/// Reservation exposes no durable record, retains the namespace, and owns the original revision before any first write.
/// 预留不暴露持久记录，在首次写入之前保留命名空间并拥有原修订。
#[test]
fn credential_reservation_precedes_inventory_and_holds_actual_files() {
    // Every physical owner belongs to this isolated filesystem fixture.
    // 每个物理所有者均属于此隔离文件系统夹具。
    let temp = tempfile::tempdir().expect("reservation fixture");
    // The parent and package are production allocations rather than fabricated records.
    // 父级及插件包来自生产分配，而非伪造记录。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // Reservation preserves the actual writer before publishing its intended inventory.
    // 预留在发布预定清单之前保留实际写入者。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve before first inventory");
    // The canonical path must still be absent despite initialized optional directories.
    // 即使可选目录已初始化，规范路径仍必须不存在。
    let path = inventory_path(&store, reservation.revision());
    assert!(!path.exists());
    assert!(!format!("{reservation:?}").contains("Preparing"));
    assert_eq!(
        reservation
            .reconcile_owned()
            .expect_err("namespace still held")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // Another manager observes the same physical namespace, even without any inventory file.
    // 另一管理器即使尚无清单文件，也观察到相同物理命名空间。
    let other = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("existing store");
    assert_eq!(
        other
            .namespace_lock("staging-plugin", false)
            .expect_err("original namespace excludes other manager")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        other
            .credential_lease("staging-plugin", reservation.revision(), false)
            .expect_err("actual revision already owned")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("confirm original inventory");
    assert!(path.exists());
    drop(
        other
            .namespace_lock("staging-plugin", false)
            .expect("publication retired namespace"),
    );
    assert_eq!(
        other
            .credential_lease("staging-plugin", reservation.revision(), false)
            .expect_err("same revision file remains owned")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
}

/// Real first writer errors and panics retain all owners, then exact readback retries never replace the actual file.
/// 真实首个写入器错误及 panic 保留全部所有者，随后精确回读重试绝不替换实际文件。
#[test]
fn credential_reservation_first_writer_unknown_is_readonly_retry() {
    for panic in [false, true] {
        // Each fault arm has distinct physical files and no shared process resources.
        // 每次故障启用拥有独立物理文件，不共享进程资源。
        let temp = tempfile::tempdir().expect("first writer fixture");
        // The original parent remains owned through writer unwinding.
        // 原父级在写入器展开期间始终被持有。
        let (store, preparation, generation) = parent_fixture(temp.path());
        // Keep the actual reservation as caller-owned state, including on panic.
        // 将实际预留保留为调用方状态，包括 panic 时。
        let mut reservation = store
            .reserve_activation_credential_staging(&preparation, &generation, &values())
            .expect("reserve");
        // The immutable revision keys the true writer observer and physical File proof.
        // 不可变修订定位真实写入器观察及物理 File 证明。
        let revision = reservation.revision().clone();
        // Only a real successful Preparing writer activates acknowledgement and readback losses.
        // 仅真实成功 Preparing 写入器启用确认及回读丢失。
        let _fault = store.lose_owned_credential_checkpoint_once_for_test(
            "staging-plugin",
            &revision,
            VulcanPluginCredentialPhase::Preparing,
            panic,
            true,
        );
        // Unwinding occurs after the original bounded production writer already succeeded.
        // 展开发生于原有界生产写入器已经成功之后。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store
                .publish_activation_credential_staging_retained(&mut reservation, &preparation)
                .map(|_| ())
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            assert_eq!(
                outcome
                    .expect("returned error")
                    .expect_err("lost acknowledgement")
                    .code,
                VulcanPluginStoreErrorCode::Storage
            );
        }
        assert_eq!(
            store.owned_credential_checkpoint_write_counts_for_test("staging-plugin", &revision),
            (1, 0)
        );
        assert_eq!(
            store
                .credential_lease("staging-plugin", &revision, false)
                .expect_err("original revision survived loss")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .namespace_lock("staging-plugin", false)
                .expect_err("original namespace survived loss")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        // Physical deny-delete sharing independently detects replay through atomic replacement.
        // 物理拒绝删除共享独立检测通过原子替换进行的重放。
        let _inventory_pin = pin_inventory(&inventory_path(&store, &revision));
        assert_eq!(
            store
                .publish_activation_credential_staging_retained(&mut reservation, &preparation)
                .expect_err("first original readback also lost")
                .code,
            VulcanPluginStoreErrorCode::Storage
        );
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect("readonly original Preparing confirmation");
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect("repeat confirmation after namespace retired");
        assert_eq!(
            store.owned_credential_checkpoint_write_counts_for_test("staging-plugin", &revision),
            (1, 0)
        );
        assert_eq!(
            reservation
                .reconcile_owned()
                .expect("same actual Preparing")
                .phase(),
            VulcanPluginCredentialPhase::Preparing
        );
    }
}

/// The parent cannot promote while the same embedded writer retains it, even after its namespace has retired.
/// 即使命名空间已退役，只要同一内嵌写入者保留父级，父级便不能晋级。
#[test]
fn credential_reservation_parent_count_blocks_promotion_until_actual_drop() {
    // Actual package copying and backend work run within one disposable store.
    // 实际插件包复制及后端工作在单个一次性存储中运行。
    let temp = tempfile::tempdir().expect("parent count fixture");
    // The parent is borrowed rather than consumed on the Busy path.
    // Busy 路径借用父级，而不消费父级。
    let (store, mut preparation, generation) = parent_fixture(temp.path());
    // Reserving the writer adds the one required actual original parent Arc.
    // 预留写入者增加唯一所需实际原父 Arc。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve");
    drop(generation);
    // Deterministic backend counts only real configured secret writes.
    // 确定性后端仅统计真实已配置秘密写入。
    let backend = Backend::default();
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("publish")
        .write_and_finish(&backend, &values())
        .expect("stage actual credentials");
    assert_eq!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("actual writer retains parent")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        reservation
            .reconcile_owned()
            .expect("same Staged confirmation")
            .phase(),
        VulcanPluginCredentialPhase::Staged
    );
    assert_eq!(backend.writes.get(), 2);
    drop(reservation);
    store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("actual writer retired before promotion");
    assert_eq!(backend.writes.get(), 2);
}

/// A true Staged writer acknowledgement loss is confirmed through the same embedded owner without backend replay.
/// 真实 Staged 写入确认丢失通过同一内嵌所有者确认，不重放后端。
#[test]
fn credential_reservation_staged_unknown_reconciles_without_backend_replay() {
    for panic in [false, true] {
        // Each actual terminal checkpoint loss uses a fresh isolated revision.
        // 每次实际终端检查点丢失均使用全新隔离修订。
        let temp = tempfile::tempdir().expect("Staged unknown fixture");
        // The exact original parent stays owned through the read-only recovery.
        // 精确原父级在只读恢复期间始终被持有。
        let (store, mut preparation, generation) = parent_fixture(temp.path());
        // Publication transfers no inner writer File out of the retained reservation.
        // 发布不将内部写入者 File 转移出保留预留。
        let mut reservation = store
            .reserve_activation_credential_staging(&preparation, &generation, &values())
            .expect("reserve");
        // The original revision remains the same after actual Staged publication.
        // 实际 Staged 发布后，原修订保持相同。
        let revision = reservation.revision().clone();
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect("publish first inventory");
        drop(generation);
        // Only final checkpoint acknowledgement and its first owned readback are lost.
        // 仅丢失最终检查点确认及其首次已持有所有者回读。
        let _fault = store.lose_owned_credential_checkpoint_once_for_test(
            "staging-plugin",
            &revision,
            VulcanPluginCredentialPhase::Staged,
            panic,
            true,
        );
        // The backend counter proves that no later reconciliation invokes writes.
        // 后端计数器证明后续确认不调用写入。
        let backend = Backend::default();
        // The same borrow executes the real MissingKeys writer before its final observer loses acknowledgement.
        // 同一借用在最终观察器丢失确认之前执行真实 MissingKeys 写入器。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store
                .publish_activation_credential_staging_retained(&mut reservation, &preparation)
                .expect("borrow same writer")
                .write_and_finish(&backend, &values())
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            assert_eq!(
                outcome
                    .expect("returned error")
                    .expect_err("lost Staged acknowledgement")
                    .code,
                VulcanPluginStoreErrorCode::Storage
            );
        }
        assert_eq!(backend.writes.get(), 2);
        assert_eq!(
            store
                .credential_record("staging-plugin", &revision)
                .expect("actual inventory")
                .expect("exists")
                .phase(),
            VulcanPluginCredentialPhase::Staged
        );
        assert_eq!(
            store
                .credential_lease("staging-plugin", &revision, false)
                .expect_err("same credential File retained")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            reservation
                .reconcile_owned()
                .expect_err("owned readback also lost")
                .code,
            VulcanPluginStoreErrorCode::Storage
        );
        // Holding actual bytes prevents a hidden replay of the Staged checkpoint on Windows.
        // 持有实际字节阻止 Windows 上隐藏重放 Staged 检查点。
        let _inventory_pin = pin_inventory(&inventory_path(&store, &revision));
        assert_eq!(
            reservation
                .reconcile_owned()
                .expect("confirm unique actual Staged edge")
                .phase(),
            VulcanPluginCredentialPhase::Staged
        );
        assert_eq!(
            reservation
                .reconcile_owned()
                .expect("confirm exact same Staged")
                .phase(),
            VulcanPluginCredentialPhase::Staged
        );
        assert_eq!(
            store
                .publish_activation_credential_staging_retained(&mut reservation, &preparation)
                .expect_err("Staged cannot reenter Preparing writer")
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        assert_eq!(
            store.owned_credential_checkpoint_write_counts_for_test("staging-plugin", &revision),
            (0, 1)
        );
        assert_eq!(backend.writes.get(), 2);
        assert_eq!(
            store
                .finish_activation_preparation_retained(
                    &mut preparation,
                    VulcanPluginConfigurationSelection::NotDeclared {}
                )
                .expect_err("parent still physically retained")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        drop(reservation);
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect("promote after whole reservation retires");
        assert_eq!(backend.writes.get(), 2);
    }
}

/// Read-only reconciliation rejects every full-inventory substitution and preserves original physical ownership.
/// 只读确认拒绝每种完整清单替换，并保留原物理所有权。
#[test]
fn credential_reservation_owned_readback_rejects_full_inventory_substitutions() {
    // Corruptions are limited to a disposable real inventory file.
    // 损坏限定于一次性真实清单文件。
    let temp = tempfile::tempdir().expect("inventory substitution fixture");
    // Valid original package and parent allow testing strict inventory equality.
    // 有效原插件包及父级使严格清单相等测试成为可能。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // The original File remains owned while external bytes are deliberately replaced.
    // 外部字节被故意替换时，原 File 仍被持有。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve");
    // Freeze the acknowledged original record before injecting substitutions.
    // 注入替换之前固定已确认原记录。
    let original = store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("publish")
        .record()
        .clone();
    // This is the single canonical inventory location for all cases.
    // 此处是所有情况的唯一规范清单位置。
    let path = inventory_path(&store, reservation.revision());
    // Start each full structural substitution from the exact original serialized record.
    // 每次完整结构替换均从精确原序列化记录开始。
    let original_json = serde_json::to_value(&original).expect("serialize non-secret record");
    for variant in [
        "parent",
        "schema",
        "package",
        "revision",
        "keys",
        "configured",
        "discarding",
        "discarded",
        "unknown",
    ] {
        // Every changed field has a known authoritative location in the strict record schema.
        // 每个改变字段在严格记录结构中都有已知权威位置。
        let mut changed = original_json.clone();
        match variant {
            "parent" => changed["activation_request"] = serde_json::json!(uuid::Uuid::new_v4()),
            "schema" => {
                changed["schema_version"] = serde_json::json!(1);
                changed
                    .as_object_mut()
                    .expect("record object")
                    .remove("activation_request");
            }
            "package" => changed["package"]["version"] = serde_json::json!("2.0.0"),
            "revision" => {
                changed["revision"] =
                    serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                        .expect("revision")
            }
            "keys" => {
                changed["keys"] = serde_json::json!([]);
                changed["configured_keys"] = serde_json::json!([]);
            }
            "configured" => changed["configured_keys"] = serde_json::json!([]),
            "discarding" => changed["phase"] = serde_json::json!("discarding"),
            "discarded" => changed["phase"] = serde_json::json!("discarded"),
            "unknown" => changed["schema_version"] = serde_json::json!(999),
            _ => unreachable!("declared substitution"),
        }
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("substitution bytes"),
        )
        .expect("write isolated substitution");
        assert_eq!(
            reservation.reconcile_owned().expect_err(variant).code,
            VulcanPluginStoreErrorCode::Conflict,
            "full original inventory substitution: {variant}"
        );
        assert_eq!(
            store
                .publish_activation_credential_staging_retained(&mut reservation, &preparation)
                .expect_err("publication rejects the same full substitution")
                .code,
            VulcanPluginStoreErrorCode::Conflict,
            "publication full original inventory substitution: {variant}"
        );
        assert_eq!(
            store
                .credential_lease("staging-plugin", reservation.revision(), false)
                .expect_err("original revision remains owned")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
    }
    fs::write(
        &path,
        serde_json::to_vec(&original).expect("original bytes"),
    )
    .expect("restore original inventory");
    assert_eq!(
        reservation
            .reconcile_owned()
            .expect("exact original Preparing")
            .phase(),
        VulcanPluginCredentialPhase::Preparing
    );
    fs::remove_file(&path).expect("remove original inventory");
    assert_eq!(
        reservation
            .reconcile_owned()
            .expect_err("absence is conflict")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect_err("retired namespace cannot recreate disappeared inventory")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(!path.exists());
}

/// Foreign stores and replacement original-parent intents cannot publish through the retained actual reservation.
/// 外部存储及替换原父意图不能通过保留实际预留发布。
#[test]
fn credential_reservation_publication_rejects_foreign_store_and_parent_bytes() {
    // Both original and foreign roots are genuinely initialized stores.
    // 原根及外部根均是真实初始化存储。
    let temp = tempfile::tempdir().expect("original root");
    // The original parent and revision are retained before any inventory exists.
    // 在清单存在之前保留原父级及修订。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // One reservation remains owned after all rejection paths.
    // 单个预留在全部拒绝路径之后仍被持有。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve");
    // A different actual store cannot borrow the original publication namespace.
    // 不同实际存储不能借用原发布命名空间。
    let foreign_root = tempfile::tempdir().expect("foreign root");
    // The foreign store is never used as a speculative recovery source.
    // 外部存储绝不作为猜测性恢复来源。
    let foreign = VulcanPluginGenerationStore::open(foreign_root.path()).expect("foreign store");
    assert_eq!(
        foreign
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect_err("foreign root")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // Replace only valid original journal bytes to test complete intent equality rather than UUID equality.
    // 仅替换有效原日志字节，以测试完整意图相等而非 UUID 相等。
    let journal = store
        .root
        .join("transactions/staging-plugin")
        .join(format!("{}.json", preparation.request_id()));
    // Exact original bytes are restored before a successful publication.
    // 成功发布前恢复精确原字节。
    let bytes = fs::read(&journal).expect("original parent bytes");
    // The strict record's intent is the authoritative source of approved package fingerprint.
    // 严格记录的意图是已批准插件包指纹的权威来源。
    let mut changed: serde_json::Value =
        serde_json::from_slice(&bytes).expect("original record JSON");
    changed["preparation"]["credential_revision"] =
        serde_json::to_value(VulcanPluginCredentialRevision::allocate())
            .expect("different revision");
    fs::write(
        &journal,
        serde_json::to_vec(&changed).expect("changed parent bytes"),
    )
    .expect("replace original parent intent");
    assert_eq!(
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .expect_err("changed full parent intent")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(!inventory_path(&store, reservation.revision()).exists());
    fs::write(&journal, bytes).expect("restore original parent bytes");
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("exact original publication");
}

/// The historic standalone API uses the unique retained first writer and preserves partial MissingKeys recovery.
/// 历史独立 API 使用唯一保留首次写入器，并保留部分 MissingKeys 恢复。
#[test]
fn credential_reservation_standalone_api_preserves_original_partial_recovery() {
    // Standalone fixtures remain schema one without a synthetic activation parent.
    // 独立夹具保持结构版本一，不合成激活父级。
    let temp = tempfile::tempdir().expect("standalone reservation fixture");
    // Immutable package ownership is separate from credential File ownership.
    // 不可变插件包所有权独立于凭据 File 所有权。
    let (store, generation) = fixture(temp.path());
    // One original revision survives the old public begin API and resume API.
    // 单个原修订在旧公开 begin API 及 resume API 之间存续。
    let revision = VulcanPluginCredentialRevision::allocate();
    // The successful real first writer loses its acknowledgement inside the old consuming wrapper.
    // 成功真实首个写入器在旧消费包装器内丢失确认。
    let _fault = store.lose_owned_credential_checkpoint_once_for_test(
        "staging-plugin",
        &revision,
        VulcanPluginCredentialPhase::Preparing,
        false,
        false,
    );
    assert_eq!(
        store
            .begin_credential_staging(&generation, revision.clone(), &values())
            .expect_err("old API delegates actual first writer")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(
        store.owned_credential_checkpoint_write_counts_for_test("staging-plugin", &revision),
        (1, 0)
    );
    drop(_fault);
    // Reopening the standalone writer is valid after the consuming caller's owner was dropped.
    // 消费调用方所有者被丢弃后，可以重新打开独立写入者。
    let mut owner = store
        .resume_credential_staging("staging-plugin", &revision)
        .expect("resume same original standalone inventory");
    assert!(owner.record().activation_request().is_none());
    // Failure on the second configured key leaves exactly one successful original backend write.
    // 第二个已配置键失败时，仅保留一次成功原后端写入。
    let backend = Backend::default();
    *backend.fail_write.borrow_mut() = Some("second".into());
    assert!(owner.write_and_finish(&backend, &values()).is_err());
    assert_eq!(backend.writes.get(), 1);
    assert_eq!(
        owner
            .reconcile_owned()
            .expect("same original Preparing")
            .phase(),
        VulcanPluginCredentialPhase::Preparing
    );
    *backend.fail_write.borrow_mut() = None;
    owner
        .write_and_finish(&backend, &values())
        .expect("only missing original key is written");
    assert_eq!(backend.writes.get(), 2);
    assert_eq!(
        owner
            .reconcile_owned()
            .expect("same original Staged")
            .phase(),
        VulcanPluginCredentialPhase::Staged
    );
    assert_eq!(backend.writes.get(), 2);
}

/// An acknowledged Staged owner rejects phase regression while retaining its exact same physical revision File.
/// 已确认 Staged 所有者拒绝阶段回退，同时保留精确相同物理修订 File。
#[test]
fn credential_reservation_staged_owner_rejects_backward_phase() {
    // This original standalone owner also verifies the shared read-only primitive without a parent reservation wrapper.
    // 此原独立所有者同时验证共享只读原语，不依赖父预留包装器。
    let temp = tempfile::tempdir().expect("backward phase fixture");
    // Production preparation supplies the immutable package and declared slots.
    // 生产准备提供不可变插件包及声明槽位。
    let (store, generation) = fixture(temp.path());
    // One real exclusive File owns this immutable revision throughout external journal replacement.
    // 单个真实排他 File 在外部日志替换期间始终持有此不可变修订。
    let revision = VulcanPluginCredentialRevision::allocate();
    // The old consuming API returns the same underlying retained writer.
    // 旧消费 API 返回相同底层保留写入者。
    let mut owner = store
        .begin_credential_staging(&generation, revision.clone(), &values())
        .expect("standalone original writer");
    // A deterministic backend proves read-only rejection adds no credential write.
    // 确定性后端证明只读拒绝不增加凭据写入。
    let backend = Backend::default();
    // Keep the actual acknowledged Staged record for exact restoration.
    // 保留实际已确认 Staged 记录，以便精确恢复。
    let staged = owner
        .write_and_finish(&backend, &values())
        .expect("actual Staged checkpoint");
    // The phase-only backward replacement is syntactically valid but not a permitted owned transition.
    // 仅改变阶段的回退替换语法有效，但不属于允许所有者转换。
    let mut preparing = staged.clone();
    preparing.phase = VulcanPluginCredentialPhase::Preparing;
    // All external edits target the one actual inventory rather than an alternate revision.
    // 全部外部编辑针对唯一实际清单，而非备用修订。
    let path = inventory_path(&store, &revision);
    fs::write(
        &path,
        serde_json::to_vec(&preparing).expect("backward bytes"),
    )
    .expect("replace phase");
    assert_eq!(
        owner
            .reconcile_owned()
            .expect_err("Staged cannot return to Preparing")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(owner.record(), &staged);
    assert_eq!(
        store
            .credential_lease("staging-plugin", &revision, false)
            .expect_err("original File still owns revision")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    fs::write(&path, serde_json::to_vec(&staged).expect("original bytes")).expect("restore Staged");
    assert_eq!(
        owner.reconcile_owned().expect("exact original Staged"),
        staged
    );
    assert_eq!(backend.writes.get(), 2);
}

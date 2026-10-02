//! Held first-inventory namespace files provide sealed actual-history reads without publishing or cloning parent ownership.
//! 首次清单保留命名空间文件提供密封实际历史读取，不发布或克隆父所有权。

use super::*;
use crate::{VulcanPluginActivationRequestRecord, VulcanPluginActivationTransactionPhase};
use std::sync::Arc;
use std::time::SystemTime;

/// Identify only canonical non-nil lease filenames directly under the fixture lock directory.
/// 仅识别夹具锁目录直属的规范非空租约文件名。
fn is_fixture_lease(path: &std::path::Path, lock_directory: &std::path::Path) -> bool {
    if path.parent() != Some(lock_directory) {
        return false;
    }
    // Namespace ownership uses one literal filename; unknown lock-directory files remain readable evidence.
    // 命名空间所有权使用唯一字面文件名；未知锁目录文件仍是可读证据。
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture UTF-8 filename");
    if name == "staging-plugin.lock" {
        return true;
    }
    for (prefix, suffix) in [
        ("staging-plugin.transaction.", ".lock"),
        ("staging-plugin-credential-", ".lease"),
        ("staging-plugin-", ".lease"),
    ] {
        // Parsing alone is insufficient: nil and alternate UUID spellings are ordinary file content.
        // 仅解析不足：空 UUID 及其他 UUID 拼写仍属于普通文件内容。
        if let Some(identity) = name
            .strip_prefix(prefix)
            .and_then(|name| name.strip_suffix(suffix))
            && let Ok(identity_uuid) = uuid::Uuid::parse_str(identity)
            && !identity_uuid.is_nil()
            && identity_uuid.to_string() == identity
        {
            return true;
        }
    }
    false
}

/// Probe the existing original request file without creating a lease or requesting a new production owner.
/// 探测已存在原请求文件，不创建租约或请求新生产所有者。
fn assert_request_owned(store: &VulcanPluginGenerationStore, request_id: uuid::Uuid) {
    // The source-defined canonical path names the exact original request file.
    // 源码定义规范路径定位精确原请求文件。
    let request = std::fs::File::open(store.transaction_lease_path("staging-plugin", request_id))
        .expect("open existing actual request File");
    assert!(
        matches!(request.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "same actual original request File remains exclusive"
    );
}

/// Snapshot actual store bytes and modification times, recording locked lease metadata without reading Windows-locked bytes.
/// 快照实际存储字节及修改时间，记录加锁租约元数据而不读取 Windows 加锁字节。
fn snapshot(
    store: &VulcanPluginGenerationStore,
) -> BTreeMap<std::path::PathBuf, (Option<Vec<u8>>, SystemTime, u64)> {
    // Only this canonical store is traversed; recognized actual leases belong to its direct locks directory.
    // 仅遍历此规范存储；已识别实际租约属于其直接 locks 目录。
    let mut pending = vec![store.root.clone()];
    // The canonical directory constrains the four source-defined lease filename formats.
    // 规范目录约束四种源码定义租约文件名格式。
    let lock_directory = store.root.join("locks");
    // Actual file length and modification time accompany every durable byte snapshot and opaque lease.
    // 实际文件长度及修改时间伴随每份持久字节快照和不透明租约。
    let mut actual = BTreeMap::new();
    while let Some(path) = pending.pop() {
        // Each real child is inspected once without aliases or read-error fallbacks.
        // 每个真实子项仅检查一次，不使用别名或读取错误备用逻辑。
        let metadata = fs::symlink_metadata(&path).expect("actual snapshot metadata");
        assert!(
            !metadata.file_type().is_symlink(),
            "fixture must contain regular actual store components"
        );
        // Only canonical lease bytes are opaque because Windows locks cover their byte ranges.
        // 仅规范租约字节不透明，因为 Windows 锁覆盖其字节范围。
        let bytes = if metadata.is_dir() {
            for child in fs::read_dir(&path).expect("actual snapshot directory") {
                pending.push(child.expect("actual snapshot child").path());
            }
            None
        } else if is_fixture_lease(&path, &lock_directory) {
            None
        } else {
            Some(fs::read(&path).expect("actual durable snapshot bytes"))
        };
        actual.insert(
            path,
            (
                bytes,
                metadata.modified().expect("actual modification time"),
                metadata.len(),
            ),
        );
    }
    actual
}

/// Probe the three actual original Files independently of deliberately damaged durable records.
/// 独立于故意损坏持久记录，探测三个实际原 File。
fn assert_physical_owned(
    store: &VulcanPluginGenerationStore,
    preparation: &VulcanPluginActivationPreparation,
    reservation: &VulcanPluginCredentialStagingReservation,
) {
    assert!(reservation.holds_publication_namespace());
    assert_eq!(
        store
            .namespace_lock("staging-plugin", false)
            .expect_err("actual original namespace File remains exclusive")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_request_owned(store, preparation.request_id());
    assert_eq!(
        store
            .credential_lease("staging-plugin", reservation.revision(), false)
            .expect_err("actual original credential File remains exclusive")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
}

/// Assert physical ownership and borrow the valid original parent without adding a third strong Arc.
/// 断言物理所有权，并借用有效原父级而不增加第三份强 Arc。
fn assert_owned(
    store: &VulcanPluginGenerationStore,
    preparation: &VulcanPluginActivationPreparation,
    reservation: &VulcanPluginCredentialStagingReservation,
) {
    assert_physical_owned(store, preparation, reservation);
    // Only valid actual records may support this borrowed parent authority check.
    // 仅有效实际记录可支持此借用父级权威检查。
    let parent: &Arc<std::fs::File> = store
        .preparation_credential_parent_locked(
            preparation,
            preparation.intent().generation(),
            reservation.revision(),
        )
        .expect("borrow same validated original parent Arc");
    assert_eq!(Arc::strong_count(parent), 2);
}

/// Prepare one actual selected predecessor and new original, returning the real immutable successor pin.
/// 准备实际已选前驱及新原请求，返回真实不可变后继固定句柄。
fn selected_fixture(
    root: &std::path::Path,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationPreparation,
    PinnedVulcanPluginGeneration,
) {
    // Production preparation fixes all actual selected-package identity fields.
    // 生产准备固定全部实际已选插件包身份字段。
    let (store, predecessor) = fixture(root);
    // A separate real secure revision belongs solely to the configured predecessor.
    // 独立真实安全修订仅属于已配置前驱。
    let revision = VulcanPluginCredentialRevision::allocate();
    // The deterministic backend performs the predecessor's true secure writes before selection.
    // 确定性后端在选择前执行前驱真实安全写入。
    let backend = Backend::default();
    // Original standalone staging provides the required actual Staged inventory.
    // 原独立暂存提供所需实际 Staged 清单。
    let mut writer = store
        .begin_credential_staging(&predecessor, revision.clone(), &values())
        .expect("predecessor writer");
    writer
        .write_and_finish(&backend, &values())
        .expect("predecessor secure checkpoint");
    drop(writer);
    // Manifest-derived permissions remain the only authority for the selected predecessor's complete approval.
    // 清单派生权限仍是已选前驱完整批准的唯一权威。
    let descriptor = crate::inspect_user_plugin_package(predecessor.package_root())
        .expect("actual predecessor manifest");
    store
        .activate_configured(
            &predecessor,
            0,
            crate::runtime::plugin_required_permissions(&descriptor).expect("permissions"),
            revision,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select full configured predecessor");
    drop(predecessor);
    // Exact source is already created by the same production package fixture.
    // 精确来源已由相同生产插件包夹具创建。
    let source = root.join("source/staging-plugin");
    // The full selected predecessor is captured without guessing its serialized version or fields.
    // 完整已选前驱被捕获，不猜测其序列化版本或字段。
    let selected = store
        .activation("staging-plugin")
        .expect("actual selected predecessor");
    // Authoritative source bytes fix the new original's fingerprint and complete permissions.
    // 权威来源字节固定新原请求指纹及完整权限。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("new source");
    // The successor has a distinct true original request File and generation identity.
    // 后继具有独立真实原请求 File 及代次身份。
    let preparation = store
        .begin_activation_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &descriptor.content_fingerprint,
            selected.as_ref(),
            crate::runtime::plugin_required_permissions(&descriptor).expect("original permissions"),
        )
        .expect("original with actual selected predecessor");
    // Real copying produces the successor's actual immutable package.
    // 真实复制产生后继实际不可变插件包。
    let generation = store
        .prepare_activation_package(&preparation, &source)
        .expect("actual successor package");
    (store, preparation, generation)
}

/// Genuine inventory absence is observable through the retained namespace without synthesizing phase or changing actual files.
/// 真实清单缺失可通过保留命名空间观察，不合成阶段或改变实际文件。
#[test]
fn credential_reservation_inspection_reads_before_first_inventory_without_writes_or_clone() {
    // Real package and original parent evidence precede credential reservation.
    // 真实插件包及原父级证据先于凭据预留。
    let temp = tempfile::tempdir().expect("unwritten inventory fixture");
    // Fixture allocates through existing production APIs only.
    // 夹具仅通过既有生产 API 分配。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // The actual original UUID identifies its one durable Preparing parent.
    // 实际原 UUID 定位其唯一持久 Preparing 父级。
    let request_id = preparation.request_id();
    // The original parent body is captured before holding the first credential namespace.
    // 原父级正文在持有首次凭据命名空间之前捕获。
    let expected = store
        .activation_request("staging-plugin", request_id)
        .expect("actual original read")
        .expect("durable original");
    // No first inventory publication has occurred when the real namespace is retained.
    // 真实命名空间保留时尚未发生首份清单发布。
    let reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve actual credential owner");
    // Successful writer counts are observed with no requested loss or secure backend invocation.
    // 成功写入次数被观察，不请求丢失或调用安全后端。
    let _fault = store.lose_owned_credential_checkpoint_once_for_test(
        "staging-plugin",
        reservation.revision(),
        VulcanPluginCredentialPhase::Staged,
        false,
        false,
    );
    // The backend remains outside this sealed observer API entirely.
    // 后端完全处于此密封观察 API 之外。
    let backend = Backend::default();
    // Unknown ordinary lock-directory content must retain actual byte and length evidence.
    // 未知普通锁目录内容必须保留实际字节及长度证据。
    let ordinary_path = store.root.join("locks/readonly-evidence.txt");
    fs::write(&ordinary_path, b"original evidence").expect("ordinary fixture content");
    // A real external size change proves the helper includes these files instead of exempting the whole directory.
    // 真实外部大小变动证明辅助器包含此些文件，而非豁免整个目录。
    let original_evidence = snapshot(&store);
    assert_eq!(
        original_evidence[&ordinary_path].0.as_deref(),
        Some(b"original evidence".as_slice())
    );
    assert_eq!(original_evidence[&ordinary_path].2, 17);
    fs::write(&ordinary_path, b"changed ordinary evidence").expect("changed fixture content");
    // Actual changed bytes and length are independently observed before the read-only API runs.
    // 只读 API 运行之前，实际变动字节及长度被独立观察。
    let changed_evidence = snapshot(&store);
    assert_eq!(
        changed_evidence[&ordinary_path].0.as_deref(),
        Some(b"changed ordinary evidence".as_slice())
    );
    assert_eq!(changed_evidence[&ordinary_path].2, 25);
    assert_ne!(
        changed_evidence[&ordinary_path].0,
        original_evidence[&ordinary_path].0
    );
    fs::write(&ordinary_path, b"original evidence").expect("restore fixture content");
    // Snapshot starts after legitimate reservation allocations and covers every actual ordinary file.
    // 快照在合法预留分配之后开始，覆盖每份实际普通文件。
    let before = snapshot(&store);
    assert_owned(&store, &preparation, &reservation);
    for _ in 0..2 {
        // The sealed observation reuses the already held namespace and returns the actual parent, not a credential phase.
        // 密封观察复用已经持有命名空间，返回实际父级，而非凭据阶段。
        let actual = store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation,
            )
            .expect("sealed actual original inspection");
        assert!(actual.original().is_some());
        assert_eq!(actual.indexed(), Some(&expected));
        assert_eq!(actual.requests().get(&request_id), Some(&expected));
        assert_eq!(actual.effects().get(&request_id), Some(&None));
        assert_eq!(actual.installations().get(&request_id), Some(&None));
        assert_eq!(actual.target_hooks().get(&request_id), Some(&None));
        assert_eq!(actual.target_mcp().get(&request_id), Some(&None));
        assert_owned(&store, &preparation, &reservation);
    }
    assert!(!inventory_path(&store, reservation.revision()).exists());
    assert_eq!(snapshot(&store), before);
    assert_eq!(
        store.owned_credential_checkpoint_write_counts_for_test(
            "staging-plugin",
            reservation.revision()
        ),
        (0, 0)
    );
    assert_eq!(backend.writes.get(), 0);
}

/// A real successful Preparing writer followed by Err or panic leaves sealed inspection read-only and physically owned.
/// 真实成功 Preparing 写入后 Err 或 panic，使密封检查保持只读及物理所有权。
#[test]
fn credential_reservation_inspection_reads_true_first_writer_error_and_panic() {
    for panic in [false, true] {
        // Every acknowledgement-loss scenario has unique actual store Files.
        // 每种确认丢失场景拥有唯一实际存储 File。
        let temp = tempfile::tempdir().expect("true first writer loss fixture");
        // Production parent and package remain owned across the genuine unknown result.
        // 生产父级及插件包在真实未知结果期间保持被持有。
        let (store, preparation, generation) = parent_fixture(temp.path());
        // Reservation itself stays outside the unwinding publication closure.
        // 预留本体保持在展开发布闭包之外。
        let mut reservation = store
            .reserve_activation_credential_staging(&preparation, &generation, &values())
            .expect("actual reservation");
        // Only the true production Preparing writer can activate this observation loss.
        // 仅真实生产 Preparing 写入器可启用此观察丢失。
        let _fault = store.lose_owned_credential_checkpoint_once_for_test(
            "staging-plugin",
            reservation.revision(),
            VulcanPluginCredentialPhase::Preparing,
            panic,
            false,
        );
        // Secure work must not begin until the original first inventory is confirmed.
        // 原首次清单被确认之前，安全工作不能开始。
        let backend = Backend::default();
        // Catching actual post-write panic preserves the same original reservation and physical namespace.
        // 捕获实际写入后 panic 保留相同原预留及物理命名空间。
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
                    .expect("error return")
                    .expect_err("actual writer acknowledgement lost")
                    .code,
                VulcanPluginStoreErrorCode::Storage
            );
        }
        assert_eq!(
            store.owned_credential_checkpoint_write_counts_for_test(
                "staging-plugin",
                reservation.revision()
            ),
            (1, 0)
        );
        // The true bounded inventory writer already produced its full original Preparing body.
        // 真实有界清单写入器已产生完整原 Preparing 正文。
        let record = store
            .read_credential_record_locked("staging-plugin", reservation.revision())
            .expect("actual sealed inventory read")
            .expect("actual inventory exists");
        assert_eq!(record.phase(), VulcanPluginCredentialPhase::Preparing);
        // Physical replacement denial proves inspection never replays the inventory writer.
        // 物理替换拒绝证明检查绝不重放清单写入器。
        let _pin = pin_inventory(&inventory_path(&store, reservation.revision()));
        // Every actual record and file modification time stays fixed through repeated sealed queries.
        // 重复密封查询期间，每份实际记录及文件修改时间保持固定。
        let before = snapshot(&store);
        for _ in 0..2 {
            assert_owned(&store, &preparation, &reservation);
            assert_eq!(
                store
                    .inspect_activation_preparation_credential_staging_reservation(
                        &preparation,
                        &reservation
                    )
                    .expect("actual first Preparing history")
                    .requests()
                    .get(&preparation.request_id())
                    .expect("original body")
                    .phase(),
                VulcanPluginActivationTransactionPhase::Preparing
            );
        }
        assert_eq!(snapshot(&store), before);
        assert_eq!(
            store.owned_credential_checkpoint_write_counts_for_test(
                "staging-plugin",
                reservation.revision()
            ),
            (1, 0)
        );
        assert_eq!(backend.writes.get(), 0);
    }
}

/// Actual namespace presence changes only after confirmed publication; the retired branch rejects sealed inspection without reacquisition.
/// 实际命名空间存在性仅在确认发布后改变；退役分支拒绝密封检查而不重新取得锁。
#[test]
fn credential_reservation_inspection_rejects_retired_namespace_and_keeps_revision_parent_owned() {
    // The reservation survives both successful publication and later backend staging.
    // 预留在成功发布及后续后端暂存之后均存续。
    let temp = tempfile::tempdir().expect("retired namespace fixture");
    // Original request ownership is distinct from actual namespace retirement.
    // 原请求所有权独立于实际命名空间退役。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // No shadow flag is needed to observe the current real File presence.
    // 观察当前真实 File 存在性无需影子标记。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve");
    assert!(reservation.holds_publication_namespace());
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("actual inventory confirmed");
    assert!(!reservation.holds_publication_namespace());
    // Hold another actual namespace owner to detect any forbidden attempt to reacquire it in the retired query branch.
    // 持有另一实际命名空间所有者，以检测退役查询分支中任何禁止重新取得锁尝试。
    let namespace = store
        .namespace_lock("staging-plugin", false)
        .expect("actual namespace released");
    assert_eq!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation
            )
            .expect_err("retired namespace is explicitly ineligible")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_request_owned(&store, preparation.request_id());
    assert_eq!(
        store
            .credential_lease("staging-plugin", reservation.revision(), false)
            .expect_err("same credential revision remains exclusive")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(namespace);
    // Actual backend completion cannot restore the retired first-publication namespace or change query selection.
    // 实际后端完成不能恢复退役首次发布命名空间或改变查询选择。
    let backend = Backend::default();
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("borrow same writer")
        .write_and_finish(&backend, &values())
        .expect("actual Staged inventory");
    assert!(!reservation.holds_publication_namespace());
    assert_eq!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation
            )
            .expect_err("Staged reservation still uses retired namespace branch")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(backend.writes.get(), 2);
    // Both retained parents must be dropped before the same existing request file can be locked again.
    // 两份保留父级均释放之后，相同已存在请求文件才能再次加锁。
    let request_id = preparation.request_id();
    drop(reservation);
    drop(preparation);
    // This terminal probe opens the existing file and neither allocates nor substitutes ownership.
    // 此终态探测打开已存在文件，既不分配也不替换所有权。
    let released_request =
        std::fs::File::open(store.transaction_lease_path("staging-plugin", request_id))
            .expect("open original released request File");
    released_request
        .try_lock()
        .expect("same original request File is released");
}

/// Sealed inspection rejects foreign roots/parents and all complete original authority substitutions while preserving physical owners.
/// 密封检查拒绝外部根／父级及全部完整原权威替换，同时保留物理所有者。
#[test]
fn credential_reservation_inspection_rejects_changed_original_authority() {
    // A real version-four predecessor makes unchanged numeric revision insufficient for this test.
    // 真实第四版前驱使未变数值修订不足以通过此测试。
    let temp = tempfile::tempdir().expect("full authority fixture");
    // Actual selection and original intent originate from production APIs.
    // 实际选择及原意图来自生产 API。
    let (store, preparation, generation) = selected_fixture(temp.path());
    // Another independent original cannot substitute its UUID, File or root.
    // 另一独立原请求不能替换 UUID、File 或根。
    let foreign_temp = tempfile::tempdir().expect("foreign fixture");
    // The foreign parent's actual physical ownership is independently held.
    // 外部父级实际物理所有权被独立持有。
    let (foreign, foreign_preparation, _foreign_generation) = parent_fixture(foreign_temp.path());
    // The original reservation has not published any credential inventory.
    // 原预留尚未发布任何凭据清单。
    let reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("original reservation");
    assert_eq!(
        foreign
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation
            )
            .expect_err("foreign root")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &foreign_preparation,
                &reservation
            )
            .expect_err("foreign original parent")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // These paths are the actual strict source-defined original journal, index and selected activation.
    // 此些路径是实际严格源码定义的原日志、索引及已选激活。
    let journal_path = store
        .root
        .join("transactions/staging-plugin")
        .join(format!("{}.json", preparation.request_id()));
    // Exact original journal bytes independently preserve intent and original predecessor.
    // 精确原日志字节独立保留意图及原前驱。
    let journal = fs::read(&journal_path).expect("original journal");
    // Unlike the earlier preparation reservation protocol, this credential query requires a durable actual parent.
    // 与较早准备预留协议不同，此凭据查询要求持久实际父级。
    fs::remove_file(&journal_path).expect("remove actual parent journal");
    assert!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation,
            )
            .is_err(),
        "credential inventory absence cannot project parent journal absence as success"
    );
    assert!(!journal_path.exists());
    fs::write(&journal_path, &journal).expect("restore exact actual parent journal");
    for variant in ["intent", "phase", "journal_predecessor"] {
        // Each substitution starts from the full strict production journal.
        // 每次替换从完整严格生产日志开始。
        let mut changed: serde_json::Value =
            serde_json::from_slice(&journal).expect("original JSON");
        match variant {
            "intent" => {
                changed["preparation"]["credential_revision"] =
                    serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                        .expect("foreign revision")
            }
            "phase" => changed["phase"] = serde_json::json!("aborted"),
            "journal_predecessor" => {
                changed["predecessor"]["approved_permissions"] = serde_json::json!([])
            }
            _ => unreachable!("declared original substitution"),
        }
        fs::write(
            &journal_path,
            serde_json::to_vec(&changed).expect("changed journal bytes"),
        )
        .expect("replace journal");
        assert_eq!(
            store
                .inspect_activation_preparation_credential_staging_reservation(
                    &preparation,
                    &reservation
                )
                .expect_err(variant)
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        fs::write(&journal_path, &journal).expect("restore exact original journal");
    }
    // Missing index must never be repaired by sealed observation.
    // 缺失索引绝不能由密封观察修复。
    let index_path = store.root.join("transactions/staging-plugin/current");
    // Retain exact actual encoding rather than assume any index JSON shape.
    // 保留精确实际编码，而非假定索引 JSON 结构。
    let index = fs::read(&index_path).expect("actual index");
    fs::remove_file(&index_path).expect("remove original index");
    assert_eq!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation
            )
            .expect_err("missing original index")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(!index_path.exists());
    fs::write(index_path, index).expect("restore exact original index");
    // Full selected predecessor approval is independently authoritative even with unchanged revision and package.
    // 即使修订及插件包未变，完整已选前驱批准也独立具有权威性。
    let selected_path = store.root.join("active/staging-plugin.json");
    // Original configured selection bytes are preserved for exact restoration.
    // 原已配置选择字节被保留以便精确恢复。
    let selected = fs::read(&selected_path).expect("actual full predecessor");
    // Change a valid complete approval field without changing the numeric revision.
    // 改变有效完整批准字段，而不改变数值修订。
    let mut changed: serde_json::Value = serde_json::from_slice(&selected).expect("selected JSON");
    changed["approved_permissions"] = serde_json::json!([]);
    fs::write(
        &selected_path,
        serde_json::to_vec(&changed).expect("changed selected bytes"),
    )
    .expect("replace selected predecessor");
    assert_eq!(
        store
            .inspect_activation_preparation_credential_staging_reservation(
                &preparation,
                &reservation
            )
            .expect_err("full selected predecessor changed")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    fs::write(selected_path, selected).expect("restore exact predecessor");
    store
        .inspect_activation_preparation_credential_staging_reservation(&preparation, &reservation)
        .expect("exact restored original history");
    assert_owned(&store, &preparation, &reservation);
    assert!(!inventory_path(&store, reservation.revision()).exists());
}

/// Staged, changed full fields and corrupt inventories are rejected even when the original namespace File is still held.
/// 即使原命名空间 File 仍被持有，Staged、改变完整字段及损坏清单仍被拒绝。
#[test]
fn credential_reservation_inspection_rejects_inventory_substitutions() {
    // Begin from actual production Preparing bytes written before the real acknowledgement loss.
    // 从真实确认丢失之前写入的实际生产 Preparing 字节开始。
    let temp = tempfile::tempdir().expect("inventory substitution fixture");
    // The original parent is fixed independently of inventory structure.
    // 原父级独立于清单结构固定。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // Original namespace survives the actual first-write error.
    // 原命名空间在实际首次写入错误后存续。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("reserve");
    // True writer acknowledgement is lost after all original Preparing bytes are persisted.
    // 真实写入确认在全部原 Preparing 字节持久化之后丢失。
    let _fault = store.lose_owned_credential_checkpoint_once_for_test(
        "staging-plugin",
        reservation.revision(),
        VulcanPluginCredentialPhase::Preparing,
        false,
        false,
    );
    assert!(
        store
            .publish_activation_credential_staging_retained(&mut reservation, &preparation)
            .is_err()
    );
    // The one actual path contains non-secret original inventory bytes only.
    // 唯一实际路径仅包含非秘密原清单字节。
    let path = inventory_path(&store, reservation.revision());
    // Exact true writer output is restored after each external substitution.
    // 每次外部替换后恢复精确真实写入器输出。
    let bytes = fs::read(&path).expect("actual original inventory");
    for variant in [
        "staged",
        "schema",
        "parent",
        "package",
        "revision",
        "keys",
        "configured",
        "corrupt",
    ] {
        // Every structural location comes from the strict production credential record schema.
        // 每个结构位置来自严格生产凭据记录模式。
        let mut changed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("original inventory JSON");
        match variant {
            "staged" => changed["phase"] = serde_json::json!("staged"),
            "schema" => changed["schema_version"] = serde_json::json!(999),
            "parent" => changed["activation_request"] = serde_json::json!(uuid::Uuid::new_v4()),
            "package" => changed["package"]["version"] = serde_json::json!("2.0.0"),
            "revision" => {
                changed["revision"] =
                    serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                        .expect("other revision")
            }
            "keys" => {
                changed["keys"] = serde_json::json!([]);
                changed["configured_keys"] = serde_json::json!([]);
            }
            "configured" => changed["configured_keys"] = serde_json::json!([]),
            "corrupt" => {}
            _ => unreachable!("declared inventory substitution"),
        }
        fs::write(
            &path,
            if variant == "corrupt" {
                b"{corrupt inventory".to_vec()
            } else {
                serde_json::to_vec(&changed).expect("changed inventory bytes")
            },
        )
        .expect("replace isolated actual inventory");
        // A rejected observer must preserve both external evidence and actual File ownership.
        // 被拒绝观察器必须保留外部证据及实际 File 所有权。
        let before = snapshot(&store);
        assert_eq!(
            store
                .inspect_activation_preparation_credential_staging_reservation(
                    &preparation,
                    &reservation
                )
                .expect_err(variant)
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        assert_eq!(snapshot(&store), before);
        fs::write(&path, &bytes).expect("restore true original inventory");
        assert_owned(&store, &preparation, &reservation);
    }
    store
        .inspect_activation_preparation_credential_staging_reservation(&preparation, &reservation)
        .expect("exact true Preparing restored");
    assert_eq!(
        store.owned_credential_checkpoint_write_counts_for_test(
            "staging-plugin",
            reservation.revision()
        ),
        (1, 0)
    );
}

/// All four strict sidecar formats reject impossible early, orphan and noncanonical evidence through the sole history scanner.
/// 全部四种严格附属格式通过唯一历史扫描器拒绝不可能早期、孤立及非规范证据。
#[test]
fn credential_reservation_inspection_reuses_four_strict_sidecar_rules() {
    for extension in ["effects", "install", "target-hook", "target-mcp"] {
        for scenario in ["early", "orphan", "noncanonical"] {
            // Each actual directory observation is confined to one independently isolated real original owner.
            // 每次实际目录观察限定于单个独立隔离真实原所有者。
            let temp = tempfile::tempdir().expect("sidecar fixture");
            // No prior synthetic receipt obscures which strict physical rule rejects the injected file.
            // 没有先前合成回执混淆拒绝注入文件的严格物理规则。
            let (store, preparation, generation) = parent_fixture(temp.path());
            // The real namespace is held without publishing any credential inventory.
            // 真实命名空间被持有，而不发布凭据清单。
            let reservation = store
                .reserve_activation_credential_staging(&preparation, &generation, &values())
                .expect("reserve");
            // Every known suffix is deliberately tested as actual evidence, not as a fallback decoder candidate.
            // 每个已知后缀被有意作为实际证据测试，而非备用解码候选。
            let request_id = if scenario == "orphan" {
                uuid::Uuid::new_v4()
            } else {
                preparation.request_id()
            };
            // Uppercase ASCII aliases are rejected by the existing unique canonical sidecar scanner.
            // 大写 ASCII 别名被既有唯一规范附属扫描器拒绝。
            let suffix = if scenario == "noncanonical" {
                extension.to_uppercase()
            } else {
                extension.to_owned()
            };
            // Original fixture bodies cannot legally own any of these sidecars before configured preparation.
            // 原夹具正文在已配置准备之前不能合法拥有任何此类附属文件。
            let path = store
                .root
                .join("transactions/staging-plugin")
                .join(format!("{request_id}.{suffix}"));
            fs::write(&path, b"{}\n").expect("inject isolated sidecar evidence");
            // Durable bytes and all modification times must survive rejection without normalization or repair.
            // 持久字节及全部修改时间必须在拒绝后存续，不作规范化或修复。
            let before = snapshot(&store);
            assert_eq!(
                store
                    .inspect_activation_preparation_credential_staging_reservation(
                        &preparation,
                        &reservation
                    )
                    .expect_err("strict sidecar rejection")
                    .code,
                VulcanPluginStoreErrorCode::Invalid,
                "sidecar {extension}/{scenario}"
            );
            assert_eq!(snapshot(&store), before);
            // Invalid sidecar evidence must not release any original physical owner.
            // 无效附属证据绝不能释放任何原物理所有者。
            assert_physical_owned(&store, &preparation, &reservation);
            fs::remove_file(&path).expect("remove only injected invalid sidecar");
            // Validate borrowed parent identity only after the exact valid durable state is restored.
            // 仅在精确有效持久状态恢复之后，校验借用父级身份。
            assert_owned(&store, &preparation, &reservation);
        }
    }
}

/// Real historical effect and installation writers are projected with their terminal full original while current credential inventory is absent.
/// 当前凭据清单缺失时，真实历史副作用及安装写入器与其终态完整原请求一同投影。
#[test]
fn credential_reservation_inspection_projects_actual_historical_receipts() {
    // Historical receipts originate from actual typed storage APIs without executing an external plugin callback.
    // 历史回执来自实际类型化存储 API，不执行外部插件回调。
    let temp = tempfile::tempdir().expect("actual history fixture");
    // The first real original has a complete package and secure credential inventory.
    // 首个真实原请求具有完整插件包及安全凭据清单。
    let (store, preparation, generation) = parent_fixture(temp.path());
    // This backend counter records actual writes and remains unchanged during later sealed history reads.
    // 此后端计数器记录实际写入，在后续密封历史读取期间保持未变。
    let backend = Backend::default();
    // The first old writer must physically release its parent before configured promotion.
    // 首个旧写入者必须在已配置晋级之前物理释放父级。
    let mut writer = store
        .begin_activation_credential_staging(&preparation, &generation, &values())
        .expect("historical writer");
    writer
        .write_and_finish(&backend, &values())
        .expect("actual historical secure checkpoint");
    drop(writer);
    drop(generation);
    // The old configured transaction retains its same real original File through receipt publication.
    // 旧已配置事务在回执发布期间保留相同真实原 File。
    let mut transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("old configured transaction");
    // Stable original identity links every historical sidecar to the same full configured body.
    // 稳定原身份将每份历史附属文件连接到相同完整已配置正文。
    let old = transaction.record().request_id();
    store
        .inspect_activation_candidate(&transaction)
        .expect("old candidate")
        .begin_effect_attempt(crate::VulcanPluginActivationEffect::Probe)
        .expect("true Probe Started writer")
        .finish(true)
        .expect("true Probe Succeeded writer");
    store
        .inspect_activation_candidate(&transaction)
        .expect("old probed candidate")
        .begin_installation_attempt()
        .expect("true installation Started writer")
        .finish(true)
        .expect("true installation Succeeded writer");
    store
        .abort_activation_transaction(&mut transaction)
        .expect("old terminal abort");
    drop(transaction);
    // Expected real receipts are read before the new namespace is retained.
    // 预期真实回执在新命名空间被保留之前读取。
    let effects = store
        .activation_effects("staging-plugin", old)
        .expect("actual old effects")
        .expect("real effects receipt");
    // Installation remains an independent actual receipt with its full original validator.
    // 安装仍是独立实际回执，具有完整原请求校验器。
    let installation = store
        .installation_receipt("staging-plugin", old)
        .expect("actual old installation")
        .expect("real installation receipt");
    // The new original is derived from the same authoritative source under the real current selection.
    // 新原请求从相同权威来源在真实当前选择下派生。
    let source = temp.path().join("source/staging-plugin");
    // Current predecessor remains actual absence because the old target was never committed.
    // 当前前驱保持实际缺失，因为旧目标从未提交。
    let selected = store
        .activation("staging-plugin")
        .expect("actual selection");
    // The source inspector supplies full approved byte and permission identities.
    // 来源检查器提供完整已批准字节及权限身份。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("source inspection");
    // New allocation uses a new actual original request rather than reusing terminal history.
    // 新分配使用新实际原请求，而非复用终态历史。
    let preparation = store
        .begin_activation_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &descriptor.content_fingerprint,
            selected.as_ref(),
            crate::runtime::plugin_required_permissions(&descriptor).expect("permissions"),
        )
        .expect("new actual original");
    // Actual copy binds the new original's package independently of its historical predecessor request.
    // 实际复制独立于历史前驱请求绑定新原请求插件包。
    let generation = store
        .prepare_activation_package(&preparation, &source)
        .expect("new package");
    // Actual first inventory remains absent while all terminal history is inspected.
    // 检查全部终态历史时，实际首份清单保持缺失。
    let reservation = store
        .reserve_activation_credential_staging(&preparation, &generation, &values())
        .expect("new retained credential reservation");
    // Freeze every actual historical record and sidecar before read-only projection.
    // 只读投影之前固定每份实际历史记录及附属文件。
    let before = snapshot(&store);
    // The single complete history decoder includes all actual terminal and current original bodies.
    // 唯一完整历史解码器包含全部实际终态及当前原正文。
    let actual = store
        .inspect_activation_preparation_credential_staging_reservation(&preparation, &reservation)
        .expect("actual full history projection");
    assert_eq!(actual.effects().get(&old), Some(&Some(effects)));
    assert_eq!(actual.installations().get(&old), Some(&Some(installation)));
    assert_eq!(actual.target_hooks().get(&old), Some(&None));
    assert_eq!(actual.target_mcp().get(&old), Some(&None));
    assert_eq!(
        actual
            .requests()
            .get(&old)
            .expect("actual historical original")
            .phase(),
        VulcanPluginActivationTransactionPhase::Aborted
    );
    assert!(matches!(
        actual.indexed(),
        Some(VulcanPluginActivationRequestRecord::Preparation(_))
    ));
    assert_eq!(snapshot(&store), before);
    assert_eq!(backend.writes.get(), 2);
    assert_owned(&store, &preparation, &reservation);
}

//! Real original namespace/request files cover first publication, lost worker results and exact baseline readback.
//! 真实原命名空间／请求文件覆盖首次发布、工作结果丢失及精确基线回读。

use super::*;
use crate::VulcanPluginActivationPreparationReservation;

/// Reserve a genuine version-three original under root, optionally retaining a real older terminal index.
/// 在 root 下预留真实第三版原请求，可选保留真实较旧终态索引。
/// Return the actual store/source/owner before any new journal, package copy or credential allocation is written.
/// 在任何新日志、插件包复制或凭据分配写入之前返回实际存储／来源／所有者。
fn reservation_fixture(
    root: &Path,
    existing_index: bool,
) -> (
    VulcanPluginGenerationStore,
    PathBuf,
    VulcanPluginActivationPreparationReservation,
) {
    // The optional real predecessor has a completed original terminal request, rather than a forged index fixture.
    // 可选真实前驱具有已经终结的原终态请求，而非伪造索引夹具。
    let (store, source) = if existing_index {
        // No copied package or credential inventory belongs to this earlier unallocated fixture intent.
        // 此较早未分配夹具意图不具有已复制插件包或凭据清单。
        let (store, source, preparation) = early_fixture(root);
        // Explicit old abandonment occurs only after its actual request owner retires.
        // 明确旧放弃仅在其实际请求所有者退役之后发生。
        let request = preparation.request_id();
        drop(preparation);
        store
            .abort_activation_preparation_request("sample-plugin", request)
            .expect("old terminal baseline");
        (store, source)
    } else {
        // An empty real store supplies genuine journal and index absence.
        // 空真实存储提供真实日志及索引缺失。
        let store = VulcanPluginGenerationStore::open(root).expect("empty store");
        // Source inspection follows the production plugin validator.
        // 来源检查遵循生产插件校验器。
        let source = write_package(&root.join("source"), "sample-plugin", "new");
        (store, source)
    };
    // The complete current selection is observed before the reservation takes its real namespace file.
    // 在预留取得其真实命名空间文件之前观察完整当前选择。
    let selected = store
        .activation("sample-plugin")
        .expect("original selection");
    // Exact approved content is frozen independently from physical copying.
    // 精确已批准内容独立于物理复制被固定。
    let fingerprint = crate::inspect_user_plugin_package(&source)
        .expect("source")
        .content_fingerprint;
    // The version-three policy belongs to this original before either journal or index publication.
    // 第三版策略在日志或索引发布之前属于此原请求。
    let reservation = store
        .reserve_activation_update_preparation(
            Uuid::new_v4(),
            &source,
            &fingerprint,
            selected.as_ref(),
            vec![],
            crate::VulcanPluginUpdatePolicy {
                explicitly_enabled: false,
                explicitly_disabled: true,
                predecessor_default_enabled: true,
            },
        )
        .expect("reserve actual original before first journal");
    (store, source, reservation)
}

/// A real reservation without a journal fences another manager and exposes genuine absence through its own observer.
/// 无日志真实预留阻挡另一管理器，并通过自身观察器暴露真实缺失。
#[test]
fn preparation_reservation_fences_other_store_before_first_journal() {
    // Isolate all real filesystem locks from user-installed plugins.
    // 将全部真实文件系统锁与用户已安装插件隔离。
    let temp = tempfile::tempdir().expect("fixture");
    // No historical record can accidentally account for this original's exclusion.
    // 不存在历史记录意外解释此原请求排他。
    let (store, source, reservation) = reservation_fixture(temp.path(), false);
    // Original identity is fixed before publication and never reconstructed from directory order.
    // 原身份在发布之前固定，绝不按目录顺序重建。
    let request = reservation.request_id();
    assert!(
        store
            .activation_preparation_reservation_record(&reservation)
            .expect("owned observation")
            .is_none()
    );
    assert!(!record_path(temp.path(), request).exists());
    assert!(
        !record_path(temp.path(), request)
            .with_file_name("current")
            .exists()
    );
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect_err("ordinary observation cannot reacquire the owner's namespace")
            .code,
        Code::Busy
    );
    // A separately opened store must respect the original real namespace file even without a durable journal.
    // 独立打开的存储必须尊重原真实命名空间文件，即使不存在持久日志。
    let other = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("store");
    assert_eq!(
        other
            .begin_activation_preparation(
                Uuid::new_v4(),
                &source,
                &reservation.intent().generation().content_fingerprint,
                None,
                vec![]
            )
            .expect_err("another UUID cannot begin before this original publishes")
            .code,
        Code::Busy
    );
    drop(reservation);
    assert!(
        other
            .begin_activation_preparation(
                Uuid::new_v4(),
                &source,
                &crate::inspect_user_plugin_package(&source)
                    .expect("source")
                    .content_fingerprint,
                None,
                vec![]
            )
            .is_ok(),
        "dropping an unpersisted reservation creates no fictitious pending journal"
    );
}

/// Actual journal/index acknowledgement loss retains original files and retries only the missing publication step.
/// 实际日志／索引确认丢失保留原文件，并仅重试缺失发布步骤。
#[test]
fn preparation_reservation_recovers_real_journal_and_index_observation_loss() {
    for journal in [true, false] {
        for existing_index in [true, false] {
            // Each real checkpoint and baseline has an independent original owner.
            // 每个真实检查点及基线具有独立原所有者。
            let temp = tempfile::tempdir().expect("fixture");
            // Both a genuine absent baseline and a full previous terminal baseline must be accepted precisely.
            // 真实缺失基线及完整较旧终态基线均须被精确接受。
            let (store, _, mut reservation) = reservation_fixture(temp.path(), existing_index);
            // Failure is bound to this canonical original request after an actual successful writer.
            // 失败在实际成功写入器之后绑定到此规范原请求。
            let request = reservation.request_id();
            // The selected real writer loses acknowledgement, followed by one lost actual owned observation.
            // 选定真实写入器丢失确认，随后丢失一次实际已持有所有者观测。
            let _fault = store.observe_preparation_reservation_writes_for_test(
                request,
                journal.then_some(false),
                (!journal).then_some(false),
                true,
            );
            assert_eq!(
                store
                    .publish_activation_preparation_retained(&mut reservation)
                    .expect_err("actual successful writer lost acknowledgement")
                    .code,
                Code::Storage
            );
            assert_eq!(
                store
                    .activation_preparation_reservation_record(&reservation)
                    .expect_err("first owned readback is also lost")
                    .code,
                Code::Storage
            );
            assert!(
                store
                    .activation_preparation_reservation_record(&reservation)
                    .expect("later exact observation")
                    .is_some()
            );
            assert_eq!(
                store.preparation_reservation_write_counts_for_test(request),
                if journal { (1, 0) } else { (1, 1) }
            );
            // Another real manager remains fenced through both failed observations.
            // 另一真实管理器在两次失败观测期间持续被阻挡。
            let other = VulcanPluginGenerationStore::open_existing(temp.path())
                .expect("other")
                .expect("store");
            assert_eq!(
                other
                    .activation("sample-plugin")
                    .expect_err("same actual namespace stays owned")
                    .code,
                Code::Busy
            );
            // The exact Prepared-style journal bytes cannot be replaced again on Windows during confirmation.
            // Windows 确认期间不能再次替换精确准备式日志字节。
            #[cfg(windows)]
            let _journal_blocker = {
                use std::os::windows::fs::OpenOptionsExt;
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0x0000_0001 | 0x0000_0002)
                    .open(record_path(temp.path(), request))
                    .expect("deny real journal replacement")
            };
            // An already published index cannot be replaced either when both writers really completed.
            // 两个写入器实际完成时，已经发布的索引同样不能被替换。
            #[cfg(windows)]
            let _index_blocker = if !journal {
                use std::os::windows::fs::OpenOptionsExt;
                Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(0x0000_0001 | 0x0000_0002)
                        .open(record_path(temp.path(), request).with_file_name("current"))
                        .expect("deny real index replacement"),
                )
            } else {
                None
            };
            // Retry uses the same original intended body and actual request lease.
            // 重试使用同一原预定正文及实际请求租约。
            let preparation = store
                .publish_activation_preparation_retained(&mut reservation)
                .expect("confirm original and publish only missing index");
            assert_eq!(preparation.request_id(), request);
            assert_eq!(
                store.preparation_reservation_write_counts_for_test(request),
                (1, 1)
            );
        }
    }
}

/// Worker unwinding after either actual successful writer cannot drop the externally retained reservation.
/// 任一实际成功写入器之后的工作展开不能释放外部保留预留。
#[test]
fn preparation_reservation_survives_real_post_write_worker_panic() {
    for journal in [true, false] {
        // Each failing worker owns only a temporary borrow of its authentic external reservation.
        // 每个失败工作者仅拥有其真实外部预留的临时借用。
        let temp = tempfile::tempdir().expect("fixture");
        // Retaining a strong typed slot models the owner surviving beyond the failing local worker frame.
        // 保留强类型槽模拟所有者跨失败局部工作帧存活。
        let (store, _, reservation) = reservation_fixture(temp.path(), true);
        // Stable identity is independent from the temporary worker return.
        // 稳定身份独立于临时工作返回。
        let request = reservation.request_id();
        // The actual files live inside an external strong slot throughout catch_unwind.
        // 实际文件在 catch_unwind 全程位于外部强槽中。
        let owner = std::sync::Arc::new(std::sync::Mutex::new(reservation));
        // Test-only failure happens after the chosen real successful writer, never instead of it.
        // 仅测试失败发生在选定真实成功写入器之后，绝不代替写入器。
        let _fault = store.observe_preparation_reservation_writes_for_test(
            request,
            journal.then_some(true),
            (!journal).then_some(true),
            false,
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Poisoning a standard test mutex is observed explicitly; the actual original resources remain present.
                // 标准测试互斥锁中毒被明确观察；实际原资源继续存在。
                let mut reservation = owner.lock().expect("original slot");
                store.publish_activation_preparation_retained(&mut reservation)
            }))
            .is_err()
        );
        // The real native namespace stays exclusive despite the failed worker frame.
        // 即使工作帧失败，真实原生命名空间仍保持排他。
        let other = VulcanPluginGenerationStore::open_existing(temp.path())
            .expect("other")
            .expect("store");
        assert_eq!(
            other
                .activation("sample-plugin")
                .expect_err("worker panic retains real namespace")
                .code,
            Code::Busy
        );
        // Recover only this explicitly poisoned standard test mutex, without recreating any original resource.
        // 仅恢复此明确中毒标准测试互斥锁，不重新创建任何原资源。
        let mut reservation = owner
            .lock()
            .expect_err("test mutex records unwinding")
            .into_inner();
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect("same original confirms post-panic bytes");
        assert_eq!(
            store.preparation_reservation_write_counts_for_test(request),
            (1, 1)
        );
    }
}

/// A readback loss immediately after the real journal writer retains genuine Preparing and does not replay that writer.
/// 真实日志写入器之后立即发生回读丢失，保留真实 Preparing 且不重放该写入器。
#[test]
fn preparation_reservation_recovers_actual_first_readback_loss() {
    // No error is returned by the writer itself in this distinct observation boundary.
    // 此独立观测边界中，写入器本身不返回错误。
    let temp = tempfile::tempdir().expect("fixture");
    // Original journal absence must not be replaced by an intended durable-phase projection.
    // 原日志缺失不能被预定持久阶段投影替代。
    let (store, _, mut reservation) = reservation_fixture(temp.path(), false);
    // Scope activates readback loss only after the actual first successful original writer.
    // 作用域仅在实际首次成功原写入器之后启用回读丢失。
    let request = reservation.request_id();
    // Both real writers remain unchanged; only the immediate observation is lost once.
    // 两个真实写入器保持未变；仅即时观测丢失一次。
    let _fault = store.observe_preparation_reservation_writes_for_test(request, None, None, true);
    assert_eq!(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect_err("actual first readback loss")
            .code,
        Code::Storage
    );
    assert_eq!(
        store.preparation_reservation_write_counts_for_test(request),
        (1, 0)
    );
    assert!(
        store
            .activation_preparation_reservation_record(&reservation)
            .expect("later actual Preparing")
            .is_some()
    );
    store
        .publish_activation_preparation_retained(&mut reservation)
        .expect("retry only the original missing index");
    assert_eq!(
        store.preparation_reservation_write_counts_for_test(request),
        (1, 1)
    );
}

/// Dropping an unobserved returned preparation leaves the same reservation available for fully read-only confirmation.
/// 释放未观察的已返回准备对象，会保留同一预留以供完全只读确认。
#[test]
fn preparation_reservation_confirms_return_loss_and_transfers_before_copying() {
    // This real transfer window precedes the first package copy and secure inventory write.
    // 此真实移交窗口先于首次插件包复制及安全清单写入。
    let temp = tempfile::tempdir().expect("fixture");
    // Both original native files remain owned outside the disappearing worker return.
    // 两个原原生文件均在消失工作返回之外被持有。
    let (store, source, mut reservation) = reservation_fixture(temp.path(), true);
    // Count real writers while confirming the same returned original twice.
    // 两次确认同一返回原请求时统计真实写入器。
    let request = reservation.request_id();
    // No injected writer failure masks the actual local returned-owner drop.
    // 不注入写入器失败来掩盖实际局部已返回所有者释放。
    let _observation =
        store.observe_preparation_reservation_writes_for_test(request, None, None, false);
    // A returned preparation temporarily shares the same real original request file.
    // 返回准备对象临时共享同一真实原请求文件。
    let preparation = store
        .publish_activation_preparation_retained(&mut reservation)
        .expect("first actual Preparing return");
    assert_eq!(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect_err("returned owner must retire before another transfer")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .prepare_activation_package(&preparation, &source)
            .expect_err("service must drop reservation before normal copying reacquires namespace")
            .code,
        Code::Busy
    );
    drop(preparation);
    // Exact already-published bytes are confirmed without another write.
    // 精确已发布字节被确认，不进行另一写入。
    let preparation = store
        .publish_activation_preparation_retained(&mut reservation)
        .expect("same-owner read-only recovery of returned result");
    assert_eq!(
        store.preparation_reservation_write_counts_for_test(request),
        (1, 1)
    );
    drop(reservation);
    // Actual service-style resource replacement retires the namespace before normal package preparation.
    // 类似服务的实际资源替换在普通插件包准备之前退役命名空间。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy after actual reservation retirement");
    // The sole transferred request owner remains available for its genuine original credential writer.
    // 唯一已移交请求所有者仍可供其真实原凭据写入者使用。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("same original credential writer");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual original staging");
    drop(staging);
    store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("original transferred owner can promote");
}

/// Full original index changes cannot be normalized merely because the replacement remains a valid terminal record.
/// 不能仅因替换仍为有效终态记录，就归一化完整原索引变更。
#[test]
fn preparation_reservation_rejects_valid_changed_full_terminal_baseline() {
    // The old terminal index is produced through actual explicit original abandonment.
    // 旧终态索引通过实际明确原放弃产生。
    let temp = tempfile::tempdir().expect("fixture");
    // New owner captures the entire old terminal record, not only its UUID or phase.
    // 新所有者捕获整个旧终态记录，而非仅 UUID 或阶段。
    let (store, _, mut reservation) = reservation_fixture(temp.path(), true);
    // The canonical current index supplies the explicit historical record identity for this corruption fixture.
    // 规范当前索引为此损坏夹具提供明确历史记录身份。
    let current: serde_json::Value = serde_json::from_slice(
        &fs::read(record_path(temp.path(), reservation.request_id()).with_file_name("current"))
            .expect("real baseline pointer"),
    )
    .expect("index wire");
    // Read one explicit validated canonical UUID; do not pick a historical record from enumeration.
    // 读取单个明确有效规范 UUID；不从枚举选择历史记录。
    let baseline_id = Uuid::parse_str(
        current["request_id"]
            .as_str()
            .expect("declared canonical index UUID"),
    )
    .expect("canonical baseline");
    // Mutate only that actual indexed body's structurally valid reserved credential identity.
    // 仅修改该实际已索引正文的结构有效预留凭据身份。
    let path = record_path(temp.path(), baseline_id);
    // The older terminal request remains a valid preparation-shaped journal after this full-body replacement.
    // 此完整正文替换后，较旧终态请求仍为有效准备形状日志。
    let mut wire: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("baseline bytes")).expect("baseline wire");
    wire["preparation"]["credential_revision"] =
        serde_json::to_value(VulcanPluginCredentialRevision::allocate())
            .expect("valid different revision");
    fs::write(
        path,
        serde_json::to_vec(&wire).expect("valid changed full baseline"),
    )
    .expect("replace baseline evidence");
    assert_eq!(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect_err("full baseline changed despite same UUID and terminal phase")
            .code,
        Code::Conflict
    );
    assert!(
        store
            .activation_preparation_reservation_record(&reservation)
            .expect("no actual own journal was written")
            .is_none()
    );
}

/// Read-only ownership observation rejects foreign stores and substituted Preparing or terminal journals without repairs.
/// 只读所有权观察拒绝外部存储及替换 Preparing 或终态日志，不执行修复。
#[test]
fn preparation_reservation_observer_rejects_foreign_and_substituted_originals() {
    // Each corruption retains its own authentic real original file owners.
    // 每种损坏保留各自真实原文件所有者。
    let temp = tempfile::tempdir().expect("fixture");
    // Original version-three policy is part of every exact durable-body comparison.
    // 原第三版策略属于每次精确持久正文比较。
    let (store, _, mut reservation) = reservation_fixture(temp.path(), false);
    // A separate existing store cannot use this original namespace authority.
    // 独立既有存储不能使用此原命名空间权威。
    let foreign = tempfile::tempdir().expect("foreign");
    // Foreign observation and publication both reject before touching the original disk authority.
    // 外部观察及发布均在触及原磁盘权威之前拒绝。
    let other = VulcanPluginGenerationStore::open(foreign.path()).expect("foreign store");
    assert_eq!(
        other
            .activation_preparation_reservation_record(&reservation)
            .expect_err("foreign observation")
            .code,
        Code::Conflict
    );
    assert_eq!(
        other
            .publish_activation_preparation_retained(&mut reservation)
            .expect_err("foreign publication")
            .code,
        Code::Conflict
    );
    drop(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect("original complete publication"),
    );
    // Preserve exact genuine bytes for independent, structurally valid changed-body cases.
    // 为独立结构有效变更正文场景保留精确真实字节。
    let path = record_path(temp.path(), reservation.request_id());
    // The observer never writes these bytes back or normalizes a changed body.
    // 观察器绝不回写这些字节，也不归一化变更正文。
    let bytes = fs::read(&path).expect("actual published Preparing");
    for terminal in [false, true] {
        // Each case changes only a declared field while preserving valid canonical original identity.
        // 每种情况仅改变声明字段，同时保留有效规范原身份。
        let mut wire: serde_json::Value = serde_json::from_slice(&bytes).expect("original wire");
        if terminal {
            wire["phase"] = serde_json::json!("aborted");
        } else {
            wire["preparation"]["update_policy"]["explicitly_enabled"] = serde_json::json!(true);
        }
        fs::write(
            &path,
            serde_json::to_vec(&wire).expect("valid substitution"),
        )
        .expect("inject substituted original");
        assert_eq!(
            store
                .activation_preparation_reservation_record(&reservation)
                .expect_err("observer rejects substituted actual original")
                .code,
            Code::Conflict
        );
        assert_eq!(
            store
                .publish_activation_preparation_retained(&mut reservation)
                .expect_err("publisher cannot normalize substituted original")
                .code,
            Code::Conflict
        );
    }
}

/// Full selected predecessor and exact original index absence cannot change while a reservation is being published.
/// 预留发布期间，完整已选前驱及精确原索引缺失状态不能改变。
#[test]
fn preparation_reservation_rejects_changed_selection_and_missing_original_index() {
    for selection in [true, false] {
        // Each explicit authority corruption has an independent genuine previous terminal index.
        // 每种明确权威损坏具有独立真实较旧终态索引。
        let temp = tempfile::tempdir().expect("fixture");
        // Reserve the complete original authority before modifying one isolated fixture artifact.
        // 修改单个隔离夹具产物之前预留完整原权威。
        let (store, _, mut reservation) = reservation_fixture(temp.path(), true);
        if selection {
            // The actual production selection path is already established by the existing transaction fixtures.
            // 实际生产选择路径已由既有事务夹具确立。
            let path = store.root.join("active/sample-plugin.json");
            // Keep revision and all resource identities identical while changing a structurally valid configuration.
            // 改变结构有效配置时，保持修订及全部资源身份相同。
            let mut wire: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).expect("actual selected predecessor"))
                    .expect("selection wire");
            wire["settings"]["configuration"] =
                serde_json::to_value(VulcanPluginConfigurationSelection::Document {
                    value: serde_json::Value::Null,
                })
                .expect("valid different complete selection");
            fs::write(path, serde_json::to_vec(&wire).expect("changed selection"))
                .expect("inject complete selection mismatch");
        } else {
            fs::remove_file(
                record_path(temp.path(), reservation.request_id()).with_file_name("current"),
            )
            .expect("remove exact previous index");
        }
        assert_eq!(
            store
                .publish_activation_preparation_retained(&mut reservation)
                .expect_err("changed original authority cannot be repaired by first publication")
                .code,
            Code::Conflict
        );
        assert!(
            store
                .activation_preparation_reservation_record(&reservation)
                .expect("read-only actual own absence")
                .is_none()
        );
        // Another store still cannot bypass the retained real original namespace after rejection.
        // 拒绝之后，另一存储仍不能绕过保留真实原命名空间。
        let other = VulcanPluginGenerationStore::open_existing(temp.path())
            .expect("other")
            .expect("store");
        assert_eq!(
            other
                .activation("sample-plugin")
                .expect_err("rejection preserves original namespace")
                .code,
            Code::Busy
        );
    }
}

/// Real index replacement failure leaves the original journal owned and retries only the same missing index.
/// 真实索引替换失败保留被持有原日志，并仅重试同一缺失索引。
#[cfg(windows)]
#[test]
fn preparation_reservation_recovers_actual_windows_index_write_failure() {
    use std::os::windows::fs::OpenOptionsExt;
    // The old terminal pointer exists physically before this original writer begins.
    // 此原写入器开始前，旧终态指针物理存在。
    let temp = tempfile::tempdir().expect("fixture");
    // Reserve real namespace and request files independently from the blocked index file.
    // 独立于被阻挡索引文件，预留真实命名空间及请求文件。
    let (store, _, mut reservation) = reservation_fixture(temp.path(), true);
    // Deny atomic deletion/replacement while permitting bounded canonical reads.
    // 允许有界规范读取，同时拒绝原子删除／替换。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001 | 0x0000_0002)
        .open(record_path(temp.path(), reservation.request_id()).with_file_name("current"))
        .expect("block actual index replacement");
    // Count successful genuine writers, without injecting any artificial acknowledgement error.
    // 统计成功真实写入器，不注入任何人工确认错误。
    let request = reservation.request_id();
    // The journal must succeed once while the real index writer fails at the OS boundary.
    // 日志必须成功一次，而真实索引写入器在操作系统边界失败。
    let _observation =
        store.observe_preparation_reservation_writes_for_test(request, None, None, false);
    assert_eq!(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect_err("real Windows index replacement denied")
            .code,
        Code::Storage
    );
    assert!(
        store
            .activation_preparation_reservation_record(&reservation)
            .expect("actual unindexed original survives")
            .is_some()
    );
    assert_eq!(
        store.preparation_reservation_write_counts_for_test(request),
        (1, 0)
    );
    drop(blocker);
    store
        .publish_activation_preparation_retained(&mut reservation)
        .expect("publish only same original missing index");
    assert_eq!(
        store.preparation_reservation_write_counts_for_test(request),
        (1, 1)
    );
}

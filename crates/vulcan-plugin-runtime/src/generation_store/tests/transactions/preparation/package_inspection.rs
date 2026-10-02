//! Real package writer losses and exact immutable files verify original-owner read-only pin recovery.
//! 真实插件包写入结果丢失及精确不可变文件验证原所有者只读固定恢复。

use super::*;
use crate::VulcanPluginGenerationPhase;
use std::collections::BTreeMap;
use std::time::SystemTime;

/// Derive the single actual allocated generation directory from the store and original owner; return its fixed path.
/// 从存储及原所有者派生唯一实际分配代次目录；返回固定路径。
fn generation_root(
    store: &VulcanPluginGenerationStore,
    preparation: &VulcanPluginActivationPreparation,
) -> PathBuf {
    store
        .root
        .join("packages")
        .join(&preparation.intent().generation().plugin_id)
        .join(&preparation.intent().generation().generation_id)
}

/// Snapshot actual files/directories beneath root, including bytes and modification times; return exact read-only evidence.
/// 快照 root 下实际文件／目录，包含字节及修改时间；返回精确只读证据。
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, SystemTime)> {
    // Exact immutable package locations never contain lease files whose byte ranges Windows locks.
    // 精确不可变插件包位置不包含 Windows 字节范围加锁的租约文件。
    let mut entries = BTreeMap::new();
    // Traverse only this real package generation rather than unrelated managed stores or source packages.
    // 仅遍历此真实插件包代次，不遍历无关受管存储或来源插件包。
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        // Actual metadata records directory identity and modification time without inferred contents.
        // 实际元数据记录目录身份及修改时间，不推断内容。
        let metadata = fs::metadata(&path).expect("snapshot actual metadata");
        // Only actual files have bytes; directory entries are recorded separately.
        // 仅实际文件拥有字节；目录条目单独记录。
        let bytes = if metadata.is_dir() {
            for child in fs::read_dir(&path).expect("snapshot actual directory") {
                pending.push(child.expect("actual child").path());
            }
            None
        } else {
            Some(fs::read(&path).expect("snapshot immutable bytes"))
        };
        entries.insert(
            path,
            (
                bytes,
                metadata.modified().expect("actual modification time"),
            ),
        );
    }
    entries
}

/// Real Prepared writer errors and panics are recovered by the same original owner without recopying any file.
/// 真实 Prepared 写入器错误及 panic 由同一原所有者恢复，不重新复制任何文件。
#[test]
fn preparation_package_inspection_recovers_true_prepared_writer_loss_readonly() {
    for panic in [false, true] {
        // Every true writer failure operates on isolated real managed package files.
        // 每次真实写入失败均操作隔离真实受管插件包文件。
        let temp = tempfile::tempdir().expect("actual writer loss fixture");
        // This original request owns a full configured predecessor and an unallocated target.
        // 此原请求持有完整已配置前驱及未分配目标。
        let (store, source, preparation) = early_fixture(temp.path());
        // Keep the exact full generation identity independently of foreground result delivery.
        // 独立于前台结果传递保留精确完整代次身份。
        let identity = preparation.intent().generation().clone();
        // Observation fires only after actual final Prepared marker publication succeeds.
        // 观察仅在实际最终 Prepared 标记发布成功之后触发。
        let _fault = store.observe_package_preparation_for_test(
            &identity,
            VulcanPluginGenerationPhase::Prepared,
            Some(panic),
        );
        // The caller retains its original exclusive request through both error return and unwinding.
        // 调用方在错误返回及展开期间均保留原排他请求。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.prepare_activation_package(&preparation, &source)
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            // True successful write loss follows the existing exact retained-generation error projection.
            // 真实成功写入丢失遵循既有精确保留代次错误投影。
            let error = outcome
                .expect("returned loss")
                .expect_err("post-success Prepared acknowledgement loss");
            assert_eq!(error.code, Code::Storage);
            assert_eq!(error.retained_generation.as_deref(), Some(&identity));
        }
        assert_eq!(
            store
                .generation_phase(&identity)
                .expect("actual final marker"),
            VulcanPluginGenerationPhase::Prepared
        );
        assert_eq!(
            store
                .resume_activation_preparation("sample-plugin", preparation.request_id())
                .expect_err("same original physical request still owned")
                .code,
            Code::Busy
        );
        // The copied generation tree and original journal provide exact unchanged evidence for every subsequent read.
        // 已复制代次树及原日志为后续每次读取提供精确未变证据。
        let root = generation_root(&store, &preparation);
        // Freeze all actual package bytes and modification times before any recovery inspection.
        // 任何恢复检查之前固定全部实际插件包字节及修改时间。
        let before = snapshot(&root);
        // The original journal must also remain byte-identical during read-only package recovery.
        // 只读插件包恢复期间，原日志也必须保持字节相同。
        let journal_path = record_path(temp.path(), preparation.request_id());
        // Keep complete original request evidence, rather than only UUID or numeric predecessor revision.
        // 保留完整原请求证据，而非仅 UUID 或数值前驱修订。
        let journal = fs::read(&journal_path).expect("original journal");
        assert!(
            store
                .prepare_activation_package(&preparation, &source)
                .is_err(),
            "ordinary copy still refuses existing immutable directory"
        );
        // Recovery accepts no source argument and must remain valid after original source retirement.
        // 恢复不接受来源参数，且必须在原来源退役后保持有效。
        fs::remove_dir_all(&source).expect("retire isolated original source");
        // The returned pin is acquired from the actual prepared marker and verified actual immutable bytes.
        // 返回固定句柄从实际已准备标记及已验证实际不可变字节取得。
        let pin = store
            .inspect_activation_preparation_package(&preparation)
            .expect("recover actual completed package pin");
        assert_eq!(pin.identity(), &identity);
        drop(
            store
                .inspect_activation_preparation_package(&preparation)
                .expect("repeat exact readonly inspection"),
        );
        assert_eq!(snapshot(&root), before);
        assert_eq!(
            fs::read(journal_path).expect("unchanged original journal"),
            journal
        );
        assert_eq!(
            store.package_preparation_checkpoint_counts_for_test(&identity),
            (1, 1)
        );
    }
}

/// The recovered pin physically excludes collection even after the parent terminates, until its last real clone drops.
/// 即使父级终结，恢复固定句柄仍物理排除回收，直到最后真实克隆释放。
#[test]
fn preparation_package_inspection_recovers_real_gc_retention() {
    // Actual generation locks, rather than pending request retention, must explain the final Busy observation.
    // 实际代次锁，而非待定请求保留，必须解释最终 Busy 观察。
    let temp = tempfile::tempdir().expect("actual pin retention fixture");
    // Production allocation creates a real original whose successor is not the selected predecessor.
    // 生产分配创建真实原请求，其后继不是已选前驱。
    let (store, source, preparation) = early_fixture(temp.path());
    // True package completion loses its returned pin after its actual bounded writer succeeds.
    // 真实插件包完成在实际有界写入器成功后丢失返回固定句柄。
    let identity = preparation.intent().generation().clone();
    // The fault acts solely at the successful final Prepared marker.
    // 故障仅作用于成功最终 Prepared 标记。
    let _fault = store.observe_package_preparation_for_test(
        &identity,
        VulcanPluginGenerationPhase::Prepared,
        Some(false),
    );
    assert!(
        store
            .prepare_activation_package(&preparation, &source)
            .is_err()
    );
    // Recovery obtains a new real generation File without changing the original preparation File.
    // 恢复取得新真实代次 File，而不改变原准备 File。
    let pin = store
        .inspect_activation_preparation_package(&preparation)
        .expect("recover actual pin");
    // The clone shares the same actual retained File rather than merely copying identifiers.
    // 克隆共享相同实际保留 File，而非仅复制标识。
    let clone = pin.clone();
    // Actual parent retirement and durable abort remove transaction-based retention before GC proof.
    // 实际父级退役及持久中止在 GC 证明之前移除事务保留。
    let request_id = preparation.request_id();
    drop(preparation);
    store
        .abort_activation_preparation_request("sample-plugin", request_id)
        .expect("terminate original parent");
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("actual recovered pin excludes GC")
            .code,
        Code::Busy
    );
    drop(pin);
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("last real clone still excludes GC")
            .code,
        Code::Busy
    );
    drop(clone);
    assert!(
        store
            .collect(&identity)
            .expect("actual GC succeeds after final pin retirement")
    );
}

/// Actual mkdir before the first marker may lose acknowledgement; markerless evidence never becomes a completed package.
/// 首标记前实际 mkdir 可能丢失确认；无标记证据绝不成为完成插件包。
#[test]
fn preparation_package_inspection_rejects_true_allocated_markerless_loss() {
    for panic in [false, true] {
        // This failure window uses actual unique mkdir with no synthetic durable generation state.
        // 此失败窗口使用实际唯一 mkdir，不合成持久代次状态。
        let temp = tempfile::tempdir().expect("markerless allocation fixture");
        // The original parent remains the unique durable resource intent.
        // 原父级仍是唯一持久资源意图。
        let (store, source, preparation) = early_fixture(temp.path());
        // Original identity names the genuine directory created before the first marker writer.
        // 原身份定位首标记写入器之前创建的真实目录。
        let identity = preparation.intent().generation().clone();
        // Preparing checkpoint here is explicitly actual allocation before the first marker, not a writer substitution.
        // 此处 Preparing 检查点明确表示首标记前实际分配，而非写入器替代。
        let _fault = store.observe_package_preparation_for_test(
            &identity,
            VulcanPluginGenerationPhase::Preparing,
            Some(panic),
        );
        // The existing prepared closure projects actual allocation errors onto the original retained-generation identity.
        // 既有 prepared 闭包将实际分配错误投影到原保留代次身份。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.prepare_activation_package(&preparation, &source)
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            // Error projection remains exactly the production retained-generation contract.
            // 错误投影保持精确生产保留代次契约。
            let error = outcome
                .expect("returned allocation loss")
                .expect_err("before-marker acknowledgement loss");
            assert_eq!(error.code, Code::Storage);
            assert_eq!(error.retained_generation.as_deref(), Some(&identity));
        }
        // Physical directory existence is evidence of allocation only, with no durable generation marker.
        // 物理目录存在仅是分配证据，不具有持久代次标记。
        let root = generation_root(&store, &preparation);
        assert!(root.is_dir());
        assert!(!root.join("generation.json").exists());
        // Freeze the actual markerless tree so rejected inspection and copy cannot silently repair it.
        // 固定实际无标记树，使被拒绝检查及复制不能静默修复它。
        let before = snapshot(&root);
        assert!(
            store
                .inspect_activation_preparation_package(&preparation)
                .is_err()
        );
        assert!(
            store
                .prepare_activation_package(&preparation, &source)
                .is_err()
        );
        assert_eq!(snapshot(&root), before);
        assert_eq!(
            store.package_preparation_checkpoint_counts_for_test(&identity),
            (1, 0)
        );
        assert_eq!(
            store
                .resume_activation_preparation("sample-plugin", preparation.request_id())
                .expect_err("original request survived markerless loss")
                .code,
            Code::Busy
        );
        // Markerless directory cleanup needs a separate typed owned cleanup contract and is deliberately not asserted here.
        // 无标记目录清理需要独立类型化所有者清理契约，此处有意不作断言。
    }
}

/// Missing, partial, corrupt or foreign completion markers and changed immutable bytes never produce an actual pin.
/// 缺失、部分、损坏或外部完成标记及改变的不可变字节绝不产生实际固定句柄。
#[test]
fn preparation_package_inspection_rejects_marker_and_package_substitutions_readonly() {
    // Substitutions affect only one isolated production-completed generation.
    // 替换仅影响单个隔离生产完成代次。
    let temp = tempfile::tempdir().expect("package substitution fixture");
    // All original parent authority remains unchanged while generation evidence changes.
    // 代次证据改变时，全部原父级权威保持未变。
    let (store, source, preparation) = early_fixture(temp.path());
    drop(
        store
            .prepare_activation_package(&preparation, &source)
            .expect("actual package completion"),
    );
    // The exact canonical marker path is derived from the original immutable identity.
    // 精确规范标记路径由原不可变身份派生。
    let root = generation_root(&store, &preparation);
    // Preserve original marker bytes independently of structural mutations.
    // 独立于结构变动保留原标记字节。
    let marker_path = root.join("generation.json");
    // The original marker is generated by the actual production writer.
    // 原标记由实际生产写入器生成。
    let marker_bytes = fs::read(&marker_path).expect("original marker");
    for variant in ["preparing", "missing", "corrupt", "schema", "identity"] {
        // Every variant starts from the full exact original identity marker.
        // 每种变体从完整精确原身份标记开始。
        let mut marker: serde_json::Value =
            serde_json::from_slice(&marker_bytes).expect("actual original marker JSON");
        match variant {
            "preparing" => {
                marker["state"] = serde_json::json!("preparing");
                fs::write(
                    &marker_path,
                    serde_json::to_vec(&marker).expect("partial marker"),
                )
                .expect("replace partial marker");
            }
            "missing" => {
                fs::remove_file(&marker_path).expect("remove marker");
            }
            "corrupt" => {
                fs::write(&marker_path, b"{broken original marker").expect("corrupt marker");
            }
            "schema" => {
                marker["schema_version"] = serde_json::json!(999);
                fs::write(
                    &marker_path,
                    serde_json::to_vec(&marker).expect("unknown schema"),
                )
                .expect("replace unknown schema");
            }
            "identity" => {
                marker["identity"]["generation_id"] = serde_json::json!(Uuid::new_v4().to_string());
                fs::write(
                    &marker_path,
                    serde_json::to_vec(&marker).expect("foreign identity"),
                )
                .expect("replace full identity");
            }
            _ => unreachable!("declared marker case"),
        }
        // Capture the actually substituted files before checking that inspection performs no repair.
        // 检查读取不作修复之前，捕获实际被替换文件。
        let before = snapshot(&root);
        assert!(
            store
                .inspect_activation_preparation_package(&preparation)
                .is_err(),
            "reject actual marker substitution: {variant}"
        );
        assert_eq!(
            snapshot(&root),
            before,
            "inspection changed actual files: {variant}"
        );
        fs::write(&marker_path, &marker_bytes).expect("restore exact original marker");
    }
    // Actual immutable content corruption must be checked beyond the valid Prepared marker.
    // 实际不可变内容损坏必须在有效 Prepared 标记之外继续检查。
    let skill_path = root.join("sample-plugin/skills/review-helper/SKILL.md");
    // Original skill bytes supply the exact actual source digest before alteration.
    // 原技能字节在改动之前提供精确实际来源摘要。
    let skill_bytes = fs::read(&skill_path).expect("original skill bytes");
    fs::write(&skill_path, b"changed immutable contents").expect("corrupt copied bytes");
    assert!(
        store
            .inspect_activation_preparation_package(&preparation)
            .is_err()
    );
    fs::write(&skill_path, skill_bytes).expect("restore copied bytes");
    drop(
        store
            .inspect_activation_preparation_package(&preparation)
            .expect("exact full marker and package restoration"),
    );
}

/// Root, index, complete original intent and full selected predecessor must remain exact before any pin is returned.
/// 返回任何固定句柄之前，根、索引、完整原意图及完整已选前驱必须保持精确。
#[test]
fn preparation_package_inspection_rejects_changed_original_authority() {
    // Genuine completion ensures each failure concerns original authority instead of missing package files.
    // 真实完成保证每项失败涉及原权威，而非缺失插件包文件。
    let temp = tempfile::tempdir().expect("original authority fixture");
    // Production original preparation contains the full version-four selected predecessor.
    // 生产原准备包含完整第四版已选前驱。
    let (store, source, preparation) = early_fixture(temp.path());
    drop(
        store
            .prepare_activation_package(&preparation, &source)
            .expect("actual completed package"),
    );
    // A different initialized store cannot use the physical original owner's UUID as authority.
    // 不同初始化存储不能将物理原所有者 UUID 作为权威使用。
    let foreign_temp = tempfile::tempdir().expect("foreign root");
    // The foreign root is one real initialized store, not a recovery fallback.
    // 外部根是单个真实初始化存储，而非恢复备用。
    let foreign = VulcanPluginGenerationStore::open(foreign_temp.path()).expect("foreign store");
    assert_eq!(
        foreign
            .inspect_activation_preparation_package(&preparation)
            .expect_err("foreign root")
            .code,
        Code::Conflict
    );
    // All original authority bytes are retained for exact independent restoration after each mutation.
    // 全部原权威字节被保留，以便每次变动后精确独立恢复。
    let journal_path = record_path(temp.path(), preparation.request_id());
    // Exact original full intent differs from a record carrying the same UUID with different resource approval.
    // 精确原完整意图不同于携带相同 UUID 但不同资源批准的记录。
    let journal = fs::read(&journal_path).expect("original full journal");
    // The current index is one original canonical UUID plus the existing protocol's serialized shape.
    // 当前索引是单个原规范 UUID 及既有协议序列化形状。
    let index_path = store.root.join("transactions/sample-plugin/current");
    // Preserve the exact real index shape without guessing any alternate encoding.
    // 保留精确真实索引形状，不猜测备用编码。
    let index = fs::read(&index_path).expect("actual current index");
    fs::remove_file(&index_path).expect("remove original index");
    assert_eq!(
        store
            .inspect_activation_preparation_package(&preparation)
            .expect_err("missing exact index")
            .code,
        Code::Conflict
    );
    fs::write(&index_path, index).expect("restore original index");
    // The known strict original preparation field owns the complete approved permission vector.
    // 已知严格原 preparation 字段持有完整已批准权限向量。
    let mut changed: serde_json::Value = serde_json::from_slice(&journal).expect("original JSON");
    changed["preparation"]["approved_permissions"] = serde_json::json!(["network_https"]);
    fs::write(
        &journal_path,
        serde_json::to_vec(&changed).expect("changed full original intent"),
    )
    .expect("replace intent");
    assert_eq!(
        store
            .inspect_activation_preparation_package(&preparation)
            .expect_err("changed full original intent")
            .code,
        Code::Conflict
    );
    fs::write(&journal_path, journal).expect("restore original full journal");
    // Actual selected predecessor includes configuration independently of its unchanged numeric revision.
    // 实际已选前驱包含独立于未变数值修订的配置。
    let selected_path = store.root.join("active/sample-plugin.json");
    // Exact selected bytes preserve the real predecessor configuration contract.
    // 精确已选字节保留真实前驱配置契约。
    let selected = fs::read(&selected_path).expect("actual selected predecessor");
    // The existing configured selection's explicit discriminator is inspected from production generated bytes.
    // 既有已配置选择显式判别器从生产生成字节检查。
    let mut changed: serde_json::Value = serde_json::from_slice(&selected).expect("selected JSON");
    changed["approved_permissions"] = serde_json::json!(["network_https"]);
    fs::write(
        &selected_path,
        serde_json::to_vec(&changed).expect("changed full selected predecessor"),
    )
    .expect("replace predecessor approval");
    assert_eq!(
        store
            .inspect_activation_preparation_package(&preparation)
            .expect_err("changed full predecessor with same revision")
            .code,
        Code::Conflict
    );
    fs::write(selected_path, selected).expect("restore original selected predecessor");
    drop(
        store
            .inspect_activation_preparation_package(&preparation)
            .expect("exact original authority restored"),
    );
}

/// A recovered actual original whose stored approval differs from immutable manifest requirements cannot return a pin.
/// 恢复实际原请求的存储批准不同于不可变清单要求时，不能返回固定句柄。
#[test]
fn preparation_package_inspection_checks_complete_approved_permissions() {
    // A valid package and Preparing journal isolate the extra exact permission comparison.
    // 有效插件包及 Preparing 日志隔离额外精确权限比较。
    let temp = tempfile::tempdir().expect("approval mismatch fixture");
    // The initial complete approval is empty because the authoritative fixture manifest requests none.
    // 初始完整批准为空，因为权威夹具清单没有要求权限。
    let (store, source, preparation) = early_fixture(temp.path());
    drop(
        store
            .prepare_activation_package(&preparation, &source)
            .expect("actual completed package"),
    );
    // Original UUID remains fixed when actual owner recovery loads strict altered approval bytes.
    // 实际所有者恢复加载严格改变批准字节时，原 UUID 保持固定。
    let request_id = preparation.request_id();
    drop(preparation);
    // Actual journal is the sole source for the resumed owner's complete expected intent.
    // 实际日志是恢复所有者完整预期意图的唯一来源。
    let path = record_path(temp.path(), request_id);
    // Every other full original intent field and predecessor remain unchanged.
    // 其余每个完整原意图字段及前驱保持未变。
    let mut changed: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("original journal")).expect("original JSON");
    changed["preparation"]["approved_permissions"] = serde_json::json!(["network_https"]);
    fs::write(
        path,
        serde_json::to_vec(&changed).expect("valid altered approval"),
    )
    .expect("replace approval");
    // The real recovery path reacquires the original File and freezes the actual strict journal as its expected record.
    // 真实恢复路径重新取得原 File，并将实际严格日志固定为预期记录。
    let original = store
        .resume_activation_preparation("sample-plugin", request_id)
        .expect("actual resumed original owner");
    assert_eq!(
        store
            .inspect_activation_preparation_package(&original)
            .expect_err("full actual manifest requirements differ from original approval")
            .code,
        Code::Conflict
    );
}

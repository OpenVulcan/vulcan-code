//! Actual historical journal discovery is frozen under original removal without backend access or namespace reacquisition.
//! 实际历史日志发现于原移除内固定，不访问后端或重新取得命名空间。

use super::*;
use uuid::Uuid;

/// A renamed real journal with an uppercase JSON extension cannot disappear from complete removal inventory discovery.
/// 带大写 JSON 扩展名的改名真实日志不能从完整移除清单发现中消失。
#[test]
fn retained_deactivation_inventory_rejects_actual_json_extension_alias_before_effects() {
    // Filesystem casing is exercised on the actual platform rather than simulated with a path comparison.
    // 文件系统大小写在实际平台上被验证，而非用路径比较模拟。
    let temp = tempfile::tempdir().expect("actual journal casing fixture");
    // The actual inspected package and producer provide a complete legitimate journal.
    // 实际已检查包及生产者提供完整合法日志。
    let (store, package) = fixture(temp.path());
    // The ordinary selection owns its own journal, so the aliased history is never the selected one.
    // 普通选择拥有自身日志，因此被改名历史绝不是已选日志。
    let selected = store
        .activate_for_test(&package, 0, package.required_permissions.clone())
        .expect("actual ordinary casing source");
    // The historical revision is produced after the selection, with no secure writes.
    // 历史修订在选择之后产生，不发生安全写入。
    let revision = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(&package, revision.clone(), &values())
            .expect("actual casing source journal"),
    );
    // A two-step rename changes the real directory spelling even on Windows case-insensitive volumes.
    // 两步改名改变真实目录拼写，即使在 Windows 大小写不敏感卷上。
    let directory = store
        .credential_directory("staging-plugin")
        .expect("actual credential directory")
        .expect("directory exists");
    // The original bytes are preserved as evidence of the inert failure.
    // 原字节作为未启动失败证据被保留。
    let canonical = directory.join(format!("{}.json", revision.as_str()));
    // This test-only intermediate filename cannot collide with a real journal name.
    // 此测试专用中间文件名不能与真实日志名碰撞。
    let intermediate = directory.join("casing-test.tmp");
    // The actual alias is the sole renamed journal, not a second invented record.
    // 实际别名为唯一改名日志，而非第二虚构记录。
    let alias = directory.join(format!("{}.JSON", revision.as_str()));
    // Raw actual bytes remain unchanged across renaming and rejected removal admission.
    // 原始实际字节跨改名及被拒绝移除准入保持未变。
    let bytes = fs::read(&canonical).expect("actual source journal bytes");
    fs::rename(&canonical, &intermediate).expect("actual casing first rename");
    fs::rename(&intermediate, &alias).expect("actual casing final rename");
    assert!(store.credential_revisions("staging-plugin").is_err());
    // Real atomic temporary files remain explicitly excluded while the JSON alias fails closed.
    // 真实原子临时文件保持明确排除，同时 JSON 别名封闭失败。
    fs::write(directory.join("unacknowledged.tmp"), b"unfinished bytes")
        .expect("actual atomic temporary fixture");
    // Selected-journal membership cannot catch the historical alias; the full census must.
    // 已选日志成员关系不能捕获历史别名；必须由完整清点捕获。
    assert!(
        store
            .reserve_deactivation(Uuid::new_v4(), &selected)
            .is_err()
    );
    assert_eq!(
        store
            .activation("staging-plugin")
            .expect("inert casing failure releases namespace"),
        Some(selected.clone())
    );
    assert_eq!(
        fs::read(&alias).expect("actual aliased journal remains intact"),
        bytes
    );
    fs::rename(&alias, &intermediate).expect("restore casing through actual intermediate");
    fs::rename(&intermediate, &canonical).expect("restore actual canonical journal");
    assert_eq!(
        store
            .credential_revisions("staging-plugin")
            .expect("genuine temporary files remain excluded"),
        BTreeSet::from([
            selected
                .credential_revision()
                .expect("selected revision")
                .clone(),
            revision
        ])
    );
}

/// Actual first-writer input at the durable envelope boundary is refused before journal publication, and older boundary records fail inert removal.
/// 位于持久封套边界的实际首写入输入在日志发布之前被拒绝，旧边界记录使移除未启动失败。
#[test]
fn retained_deactivation_inventory_reserves_actual_durable_phase_growth_before_effects() {
    // The sole durable limit and actual producer encoding derive the boundary input without copied size constants.
    // 唯一持久上限及实际生产者编码推导边界输入，不复制大小常量。
    let temp = tempfile::tempdir().expect("actual durable cleanup capacity fixture");
    // The ordinary package supplies the original selected source and baseline producer metadata.
    // 普通包提供原已选来源及基线生产者元数据。
    let (store, original_package) = fixture(temp.path());
    // The first real Preparing journal supplies exact non-secret field encoding sizes.
    // 首个真实准备中日志提供精确非秘密字段编码大小。
    let first = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(&original_package, first.clone(), &values())
            .expect("actual baseline Preparing journal"),
    );
    // The sole strict reader supplies the actual record rather than a hand-built JSON shape.
    // 唯一严格读取器提供实际记录，而非手写 JSON 形状。
    let baseline = store
        .credential_record("staging-plugin", &first)
        .expect("actual baseline journal read")
        .expect("journal exists");
    // Grow only one manifest-declared absent optional key, whose single journal occurrence determines actual capacity.
    // 仅扩展单个清单声明缺失可选键，其日志内单次出现确定实际容量。
    let key_size = crate::generation_store::MAX_RECORD_BYTES as usize
        - serde_json::to_vec_pretty(&baseline)
            .expect("actual baseline durable encoding")
            .len()
        + "optional".len();
    // Plain ASCII expansion avoids escaping or multi-byte guesses in the exact boundary input.
    // 普通 ASCII 扩展避免精确边界输入中的转义或多字节猜测。
    let optional_key = "a".repeat(key_size);
    // This is the exact source manifest written by the existing package fixture.
    // 此为既有包夹具写入的精确来源清单。
    let source = temp.path().join("source/staging-plugin");
    // The original immutable package remains untouched while the independent source declaration evolves.
    // 独立来源声明演进期间，原不可变包保持未变。
    let manifest =
        fs::read_to_string(source.join("plugin.yaml")).expect("actual capacity source manifest");
    assert!(
        manifest.contains("key: optional"),
        "expected fixture optional declaration may have evolved"
    );
    fs::write(
        source.join("plugin.yaml"),
        manifest.replace("key: optional", &format!("key: {optional_key}")),
    )
    .expect("actual capacity source input");
    // Production inspection/preparation derives the real historical physical identity and exact declared key list.
    // 生产检查／准备推导真实历史物理身份及精确声明键列表。
    let descriptor =
        crate::inspect_user_plugin_package(&source).expect("actual boundary source inspection");
    // The new package is an independent history, not a mutation of current selection bytes.
    // 新包为独立历史，而非当前选择字节变更。
    let history_package = store
        .prepare(&source, &descriptor.content_fingerprint)
        .expect("actual boundary package preparation");
    // Complete values preserve both configured keys and the expanded absent optional slot.
    // 完整值保留两个已配置键及扩展缺失可选槽位。
    let mut supplied = values();
    supplied.remove(&key("optional"));
    supplied.insert(key(&optional_key), None);
    // This actual first writer is refused before publication because Discarding needs more durable bytes than Preparing.
    // 此实际首写入者在发布之前被拒绝，因为丢弃中比准备中要求更多持久字节。
    let boundary_revision = VulcanPluginCredentialRevision::allocate();
    assert!(
        store
            .begin_credential_staging(&history_package, boundary_revision.clone(), &supplied)
            .is_err()
    );
    assert!(
        store
            .credential_record("staging-plugin", &boundary_revision)
            .expect("refused writer publishes no journal")
            .is_none()
    );
    // Explicit older-writer fixture reuses actual producer fields and real manifest inputs to persist the formerly accepted boundary record.
    // 明确旧写入器夹具复用实际生产者字段及真实清单输入，持久保存先前可接受边界记录。
    let mut older_record = baseline.clone();
    older_record.package = VulcanPluginCredentialPackage::Generation {
        identity: history_package.identity().clone(),
    };
    older_record.revision = boundary_revision.clone();
    older_record.keys = supplied.keys().cloned().collect();
    assert_eq!(
        serde_json::to_vec_pretty(&older_record)
            .expect("actual older capacity encoding")
            .len() as u64,
        crate::generation_store::MAX_RECORD_BYTES
    );
    crate::generation_store::files::write_record(
        &store
            .credential_directory("staging-plugin")
            .expect("actual older journal directory")
            .expect("directory exists")
            .join(format!("{}.json", boundary_revision.as_str())),
        &older_record,
    )
    .expect("older bounded writer fixture publication");
    assert_eq!(
        store
            .credential_record("staging-plugin", &boundary_revision)
            .expect("actual older record remains bounded and valid"),
        Some(older_record.clone())
    );
    // Current selection stays ordinary and valid, so failure proves historical cleanup capacity rather than active-source corruption.
    // 当前选择保持普通且有效，因此失败证明历史清理容量，而非有效来源损坏。
    let selected = store
        .activate_for_test(
            &original_package,
            0,
            original_package.required_permissions.clone(),
        )
        .expect("actual ordinary capacity selection");
    assert!(
        store
            .reserve_deactivation(Uuid::new_v4(), &selected)
            .is_err()
    );
    assert_eq!(
        store
            .activation("staging-plugin")
            .expect("capacity failure releases inert namespace"),
        Some(selected)
    );
    assert_eq!(
        store
            .credential_record("staging-plugin", &boundary_revision)
            .expect("older history preserved without cleanup"),
        Some(older_record)
    );
}

/// Removal retains the selected and all actual prepared, staged and discarded versioned journals and complete optional slots.
/// 移除保留已选及全部实际准备中、已暂存及已丢弃版本化日志及完整可选槽位。
#[test]
fn retained_deactivation_inventory_captures_all_actual_credential_phases() {
    // Every physical revision is produced by the actual staging state machine.
    // 每个物理修订均由实际暂存状态机产生。
    let temp = tempfile::tempdir().expect("actual historical credential fixture");
    // The source declares two configured keys and one explicitly absent optional slot.
    // 来源声明两个已配置键及一个明确缺失可选槽位。
    let (store, package) = fixture(temp.path());
    // Only values use the memory fixture; journals and all Files are actual production owners.
    // 仅值使用内存夹具；日志及全部文件均为实际生产所有者。
    let backend = Backend::default();
    // The fully staged revision becomes the current selection's credential authority.
    // 完全暂存修订成为当前选择的凭据权威。
    let staged = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(&package, staged.clone(), &values())
        .expect("actual staged journal begin")
        .write_and_finish(&backend, &values())
        .expect("actual staged journal finish");
    // An unfinished actual writer retains its own revision File while read-only removal metadata is captured.
    // 未完成实际写入者在只读移除元数据捕获期间保留自身修订文件。
    let preparing = VulcanPluginCredentialRevision::allocate();
    // The actual writer is kept outside the original removal snapshot and cannot be reconstructed from it.
    // 实际写入者保持位于原移除快照之外，不能从快照重建。
    let pending = store
        .begin_credential_staging(&package, preparing.clone(), &values())
        .expect("actual unfinished revision");
    // A terminal real journal retains keys and identity after its actual secure values have been removed.
    // 终态真实日志在实际安全值移除之后保留键及身份。
    let discarded = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(&package, discarded.clone(), &values())
        .expect("actual terminal journal begin")
        .write_and_finish(&backend, &values())
        .expect("actual terminal journal finish");
    store
        .collect_credentials("staging-plugin", &discarded, &backend)
        .expect("actual terminal credential collection");
    // Only the staged revision is selected; the other journals are pure history.
    // 仅已暂存修订被选定；其他日志均为纯历史。
    let selected = store
        .activate_with_credentials_for_test(
            &package,
            0,
            package.required_permissions.clone(),
            staged.clone(),
        )
        .expect("actual current selection");
    // Actual backend write counts remain unchanged; removal census receives no backend parameter or secure-operation authority.
    // 实际后端写入计数保持未变；移除清单不接收后端参数或安全操作权威。
    let writes_before = backend.writes.get();
    // One actual original namespace File supplies the whole deterministic frozen journal inventory.
    // 单个实际原命名空间文件提供完整确定性固定日志清单。
    let mut original = store
        .reserve_deactivation(Uuid::new_v4(), &selected)
        .expect("actual historical removal original");
    assert_eq!(original.credential_inventory().len(), 3);
    for (revision, phase) in [
        (&preparing, VulcanPluginCredentialPhase::Preparing),
        (&staged, VulcanPluginCredentialPhase::Staged),
        (&discarded, VulcanPluginCredentialPhase::Discarded),
    ] {
        // Identity-based assertions remain stable when the inventory gains unrelated entries.
        // 基于身份的断言在清单增加无关条目时保持稳定。
        let record = original
            .credential_inventory()
            .iter()
            .find(|record| record.revision() == revision)
            .expect("actual original revision remains present");
        assert_eq!(record.phase(), phase);
        assert_eq!(
            record.keys(),
            &[key("first"), key("optional"), key("second")]
        );
        assert_eq!(record.configured_keys(), &[key("first"), key("second")]);
        assert_eq!(record.generation(), Some(package.identity()));
    }
    assert_eq!(backend.writes.get(), writes_before);
    assert!(
        pending
            .store
            .clone()
            .credential_revisions("staging-plugin")
            .is_err()
    );
    store
        .publish_deactivation_retained(&mut original)
        .expect("publish same actual removal original");
    assert_eq!(
        store
            .collect_deactivation_inventory_credentials_retained(&original, &preparing, &backend)
            .expect_err("actual unfinished writer prevents historical deletion")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        store
            .confirm_deactivation_inventory_collected_retained(&original, &backend)
            .expect_err("nonterminal actual histories cannot be confirmed")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(backend.values.borrow().len(), 2);
    // Releasing the actual unfinished writer admits cleanup of its original Preparing journal without promoting it.
    // 释放实际未完成写入者准入其原准备中日志清理，不将其晋级。
    drop(pending);
    for revision in [&preparing, &staged, &discarded] {
        store
            .collect_deactivation_inventory_credentials_retained(&original, revision, &backend)
            .expect("collect each real frozen historical revision");
        assert_eq!(
            store
                .read_credential_record_locked("staging-plugin", revision)
                .expect("read actual historical checkpoint")
                .expect("historical journal persists")
                .phase(),
            VulcanPluginCredentialPhase::Discarded
        );
    }
    assert!(backend.values.borrow().is_empty());
    // The inventory owns no extra package reader, and whole storage confirmation requires actual package bytes to be collected.
    // 清单不拥有额外包读者，完整存储确认要求实际包字节被回收。
    drop(package);
    store
        .collect_deactivation_package_retained(&original)
        .expect("collect actual package after real pin release");
    store
        .confirm_deactivation_inventory_collected_retained(&original, &backend)
        .expect("every actual package and credential history is terminal");
    // A revision allocated after capture has no retained cleanup authority even if its UUID is syntactically valid.
    // 捕获之后分配的修订没有保留清理权威，即使其 UUID 语法有效。
    let foreign = VulcanPluginCredentialRevision::allocate();
    assert_eq!(
        store
            .collect_deactivation_inventory_credentials_retained(&original, &foreign, &backend)
            .expect_err("outside revision cannot be adopted")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
}

/// Actual producer histories exceeding the sole combined budget fail before removal admission without truncating journals or changing selection.
/// 超出唯一合并额度的实际生产者历史在移除准入之前失败，不截断日志或改变选择。
#[test]
fn retained_deactivation_inventory_rejects_actual_oversized_history_before_effects() {
    // Many real bounded records exercise aggregate retention instead of mocking a budget counter.
    // 多个真实有界记录覆盖合并保留，而非模拟额度计数器。
    let temp = tempfile::tempdir().expect("actual oversized historical inventory fixture");
    // The ordinary source and original pin remain intact while a distinct historical package is prepared.
    // 普通来源及原固定在独立历史包准备期间保持完整。
    let (store, original_package) = fixture(temp.path());
    // Only the ordinary original is selected, so the failure proves aggregate historical capture rather than current-source corruption.
    // 仅普通原实例被选定，因此失败证明合并历史捕获，而非当前来源损坏。
    let selected = store
        .activate_for_test(
            &original_package,
            0,
            original_package.required_permissions.clone(),
        )
        .expect("actual ordinary selection");
    // Actual producer input expands one optional key while preserving the same known manifest structure.
    // 实际生产者输入扩展单个可选键，同时保留相同已知清单结构。
    let source = temp.path().join("source/staging-plugin");
    // This is the actual source manifest written by the existing fixture producer.
    // 此为既有夹具生产者写入的实际来源清单。
    let manifest =
        fs::read_to_string(source.join("plugin.yaml")).expect("actual original source manifest");
    assert!(
        manifest.contains("key: optional"),
        "expected fixture optional declaration may have evolved"
    );
    // A large valid optional key fits each existing record envelope but makes aggregate history retention significant.
    // 大型有效可选键符合每个既有记录封套，但使合并历史保留具有显著大小。
    let optional_key = "historical_optional_".repeat(800);
    fs::write(
        source.join("plugin.yaml"),
        manifest.replace("key: optional", &format!("key: {optional_key}")),
    )
    .expect("actual historical source declaration");
    // The production inspector and preparer supply every actual immutable identity and complete key list.
    // 生产检查器及准备器提供每个实际不可变身份及完整键清单。
    let descriptor =
        crate::inspect_user_plugin_package(&source).expect("actual large-key source inspection");
    // This second pin is independent from the ordinary current source.
    // 此第二固定独立于普通当前来源。
    let history_package = store
        .prepare(&source, &descriptor.content_fingerprint)
        .expect("actual historical package preparation");
    // Complete values include the deliberately absent large optional slot, without writing any backend entries.
    // 完整值包含刻意缺失大型可选槽位，不写入任何后端条目。
    let mut supplied = values();
    supplied.remove(&key("optional"));
    supplied.insert(key(&optional_key), None);
    // The first true producer record provides the measured byte size for count derivation.
    // 首个真实生产者记录提供用于推导数量的实测字节大小。
    let first = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(&history_package, first.clone(), &supplied)
            .expect("actual first bounded large journal"),
    );
    // Existing strict public reading confirms the marker fits its own bounded record envelope.
    // 既有严格公开读取确认标记符合自身有界记录封套。
    let record = store
        .credential_record("staging-plugin", &first)
        .expect("actual bounded journal read")
        .expect("journal exists");
    // Derive enough real producer entries from the sole semantic budget, avoiding a copied literal or fixed dataset index.
    // 从唯一语义额度推导足够真实生产者条目，避免复制字面量或固定数据下标。
    let count = crate::generation_store::deactivation::inventory::MAX_REMOVAL_INVENTORY_BYTES
        / serde_json::to_vec(&record)
            .expect("actual journal encoding")
            .len()
        + 1;
    for _ in 1..count {
        // Each revision is allocated and persisted by the same production preparation primitive; no secure effect is started.
        // 每个修订由相同生产准备原语分配及持久保存；不启动安全效果。
        let revision = VulcanPluginCredentialRevision::allocate();
        drop(
            store
                .begin_credential_staging(&history_package, revision, &supplied)
                .expect("actual bounded historical journal"),
        );
    }
    // No callback, selection successor or collection is admitted after bounded original construction fails.
    // 有界原构造失败之后，不准入回调、选择后继或回收。
    let error = store
        .reserve_deactivation(Uuid::new_v4(), &selected)
        .expect_err("actual aggregate inventory must exceed retained budget");
    assert!(error.to_string().contains("retained metadata budget"));
    assert_eq!(
        store
            .activation("staging-plugin")
            .expect("inert failure releases namespace"),
        Some(selected)
    );
    assert_eq!(
        store
            .credential_revisions("staging-plugin")
            .expect("every actual journal remains preserved")
            .len(),
        count + 1,
        "the selected journal joins every historical journal"
    );
    assert_eq!(
        store
            .credential_record("staging-plugin", &first)
            .expect("first actual journal unchanged")
            .expect("journal remains"),
        record
    );
}

/// Metadata from another real revision cannot replace a historical journal and cannot change the original current selection.
/// 另一真实修订元数据不能替代历史日志，也不能改变原当前选择。
#[test]
fn retained_deactivation_inventory_rejects_foreign_actual_credential_history_before_effects() {
    // Actual producer bytes supply both the original and foreign complete journal shapes.
    // 实际生产者字节提供原及外来完整日志形状。
    let temp = tempfile::tempdir().expect("actual foreign historical journal fixture");
    // Both historical revisions belong to the same real package but retain distinct exact secure namespaces.
    // 两个历史修订属于相同真实包，但保留不同精确安全命名空间。
    let (store, package) = fixture(temp.path());
    // These inert actual owners produce complete original journals without backend effects.
    // 这些未启动实际所有者产生完整原日志，不产生后端效果。
    let first = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(&package, first.clone(), &values())
            .expect("first actual historical journal"),
    );
    // The second actual final journal carries its own UUID and full original identity.
    // 第二实际最终日志携带自身 UUID 及完整原身份。
    let second = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(&package, second.clone(), &values())
            .expect("second actual historical journal"),
    );
    // The existing sole directory helper fixes every allowed journal path.
    // 既有唯一目录辅助方法固定每个允许日志路径。
    let directory = store
        .credential_directory("staging-plugin")
        .expect("actual credential directory")
        .expect("directory exists");
    fs::copy(
        directory.join(format!("{}.json", second.as_str())),
        directory.join(format!("{}.json", first.as_str())),
    )
    .expect("actual foreign journal bytes replacement");
    // The actual selected source is deliberately legacy so only history validation encounters the mismatch.
    // 实际已选来源刻意为旧式，因此仅历史校验遇到不匹配。
    let selected = store
        .activate_for_test(&package, 0, package.required_permissions.clone())
        .expect("actual legacy selection");
    assert!(
        store
            .reserve_deactivation(Uuid::new_v4(), &selected)
            .is_err()
    );
    assert_eq!(
        store
            .activation("staging-plugin")
            .expect("inert failure releases namespace"),
        Some(selected)
    );
    assert_eq!(
        fs::read(directory.join(format!("{}.json", first.as_str())))
            .expect("foreign bytes preserved"),
        fs::read(directory.join(format!("{}.json", second.as_str())))
            .expect("actual foreign producer bytes")
    );
}

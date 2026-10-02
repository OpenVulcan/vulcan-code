//! Real retained namespace files verify read-only inspection before and after uncertain first publication.
//! 真实保留命名空间文件验证不确定首次发布前后的只读检查。

use super::*;
use crate::VulcanPluginActivationPreparationReservation;
use std::collections::BTreeMap;

/// Reserve an exact original from this store's actual current selection and approved source.
/// 从此存储实际当前选择及批准来源预留精确原请求。
/// Return the actual owner before its original journal writer executes.
/// 在原日志写入器执行前返回实际所有者。
fn reserve(
    store: &VulcanPluginGenerationStore,
    source: &Path,
) -> VulcanPluginActivationPreparationReservation {
    // The full selected predecessor is captured before the namespace file is exclusively retained.
    // 完整已选前驱在排他保留命名空间文件前捕获。
    let selected = store
        .activation("sample-plugin")
        .expect("selected predecessor");
    // Approved content comes from the production package inspector.
    // 批准内容来自生产插件包检查器。
    let fingerprint = crate::inspect_user_plugin_package(source)
        .expect("source")
        .content_fingerprint;
    store
        .reserve_activation_update_preparation(
            Uuid::new_v4(),
            source,
            &fingerprint,
            selected.as_ref(),
            vec![],
            crate::VulcanPluginUpdatePolicy {
                explicitly_enabled: false,
                explicitly_disabled: true,
                predecessor_default_enabled: true,
            },
        )
        .expect("actual original reservation")
}

/// Distinguish durable bytes from actual lease files whose Windows byte ranges remain locked.
/// 区分持久字节及 Windows 字节范围仍被锁定的实际租约文件。
#[derive(Debug, PartialEq, Eq)]
enum TreeContents {
    /// Actual directory entry, whose descendants are inspected separately.
    /// 实际目录条目，其后代单独检查。
    Directory,
    /// Actual namespace/request/package/credential lease; retain its metadata without reading locked bytes.
    /// 实际命名空间、请求、插件包、凭据租约；保留元数据而不读取锁定字节。
    Lease,
    /// Exact actual durable bytes, including every journal, receipt and package marker.
    /// 精确实际持久字节，包括每份日志、回执及插件包标记。
    Durable(Vec<u8>),
}

/// Recognize only this fixture's four source-defined lease layouts directly under its actual store lock directory.
/// 仅识别此夹具实际存储锁目录直属、源码定义的四种租约布局。
/// Return true for canonical sample-plugin namespace/request/generation/credential lock files; never suppress a read error.
/// 对规范 sample-plugin 命名空间、请求、代际、凭据锁文件返回真；绝不隐藏读取错误。
fn is_fixture_lease(path: &Path, lock_directory: &Path) -> bool {
    if path.parent() != Some(lock_directory) {
        return false;
    }
    // Operations::namespace_lock defines the exact namespace filename.
    // Operations::namespace_lock 定义精确命名空间文件名。
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture UTF-8 filename");
    if name == "sample-plugin.lock" {
        return true;
    }
    // Transaction operations define the request suffix; package and credential operations define the two lease prefixes.
    // 事务操作定义请求后缀；插件包及凭据操作定义两种租约前缀。
    for (prefix, suffix) in [
        ("sample-plugin.transaction.", ".lock"),
        ("sample-plugin-credential-", ".lease"),
        ("sample-plugin-", ".lease"),
    ] {
        // Only canonical actual UUID identities belong to these declared fixture lease formats.
        // 仅规范实际 UUID 身份属于这些声明夹具租约格式。
        if let Some(identity) = name
            .strip_prefix(prefix)
            .and_then(|name| name.strip_suffix(suffix))
            && let Ok(identity_uuid) = Uuid::parse_str(identity)
            && !identity_uuid.is_nil()
            && identity_uuid.to_string() == identity
        {
            return true;
        }
    }
    false
}

/// Capture the actual fixture tree while retaining all owners and reading every durable file's exact bytes.
/// 保留全部所有者并读取每份持久文件精确字节，同时捕获实际夹具目录树。
/// Return entries, contents, modification times and lengths; only source-defined lock files use metadata alone.
/// 返回条目、内容、修改时间及长度；仅源码定义锁文件只使用元数据。
fn tree(root: &Path) -> BTreeMap<PathBuf, (TreeContents, std::time::SystemTime, u64)> {
    // The store has one authoritative lease directory; recursive inspection retains that exact root.
    // 存储具有唯一权威租约目录；递归检查保留此精确根。
    let lock_directory = root.join(STORE_DIRECTORY).join("locks");
    tree_entries(root, &lock_directory)
}

/// Inspect directory's actual descendants with the original fixture lock root, without reopening or releasing any lease.
/// 按原夹具锁根检查目录实际后代，不重新打开或释放任何租约。
/// Return byte snapshots for durable files and metadata snapshots for declared lease files; every other read failure fails the test.
/// 返回持久文件字节快照及声明租约文件元数据快照；所有其他读取失败均令测试失败。
fn tree_entries(
    root: &Path,
    lock_directory: &Path,
) -> BTreeMap<PathBuf, (TreeContents, std::time::SystemTime, u64)> {
    // Every entry belongs to this isolated fixture, so no user storage is enumerated.
    // 每个条目均属于此隔离夹具，因此不枚举用户存储。
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).expect("fixture directory") {
        // Literal fixture paths avoid alternate storage-path inference.
        // 字面夹具路径避免推断替代存储路径。
        let path = entry.expect("entry").path();
        // Modification times detect attempted replacement even when bytes are identical.
        // 修改时间检测字节相同时的替换尝试。
        let metadata = fs::metadata(&path).expect("entry metadata");
        if metadata.is_dir() {
            result.insert(
                path.clone(),
                (
                    TreeContents::Directory,
                    metadata.modified().expect("directory time"),
                    metadata.len(),
                ),
            );
            result.extend(tree_entries(&path, lock_directory));
        } else if is_fixture_lease(&path, lock_directory) {
            result.insert(
                path.clone(),
                (
                    TreeContents::Lease,
                    metadata.modified().expect("lease time"),
                    metadata.len(),
                ),
            );
        } else {
            result.insert(
                path.clone(),
                (
                    TreeContents::Durable(fs::read(&path).expect("durable entry bytes")),
                    metadata.modified().expect("file time"),
                    metadata.len(),
                ),
            );
        }
    }
    result
}

/// Missing original journals remain None while complete real terminal history and both file fences survive inspection.
/// 原日志缺失保持 None，完整真实终态历史及两类文件屏障跨检查存续。
#[test]
fn preparation_inspection_preserves_absence_history_and_original_locks_without_writes() {
    // Actual abandonment creates the historical baseline through its production API.
    // 实际放弃通过生产接口创建历史基线。
    let temp = tempfile::tempdir().expect("fixture");
    // Earlier journal ownership is genuinely retired before explicit terminal publication.
    // 较早日志所有权在明确终态发布前真实退役。
    let (store, source, early) = early_fixture(temp.path());
    // The historical identity never comes from directory ordering.
    // 历史身份绝不来自目录排序。
    let old = early.request_id();
    drop(early);
    store
        .abort_activation_preparation_request("sample-plugin", old)
        .expect("terminal history");
    // Freeze the actual full old body before acquiring the new namespace owner.
    // 在取得新命名空间所有者前固定实际完整旧正文。
    let baseline = store
        .activation_request("sample-plugin", old)
        .expect("old read")
        .expect("old journal");
    // This actual new original has a request file but no journal.
    // 此实际新原请求具有请求文件，但没有日志。
    let reservation = reserve(&store, &source);
    // The tree is observed after reservation creation, excluding its legitimate file allocation.
    // 目录树在预留创建后观察，排除其合法文件分配。
    let before = tree(temp.path());
    // The sealed observer consumes no lease and makes no inferred Preparing request.
    // 密封观察者不消耗租约，也不推导 Preparing 请求。
    let actual = store
        .inspect_activation_preparation_reservation(&reservation)
        .expect("actual inspection");
    assert!(actual.original().is_none());
    assert_eq!(actual.indexed(), Some(&baseline));
    assert_eq!(actual.requests().get(&old), Some(&baseline));
    assert!(!actual.requests().contains_key(&reservation.request_id()));
    assert_eq!(actual.effects().get(&old), Some(&None));
    assert_eq!(actual.installations().get(&old), Some(&None));
    assert_eq!(actual.target_hooks().get(&old), Some(&None));
    assert_eq!(actual.target_mcp().get(&old), Some(&None));
    assert_eq!(
        tree(temp.path()),
        before,
        "inspection must never write or repair storage"
    );
    assert_eq!(
        store
            .activation_transaction_ids("sample-plugin")
            .expect_err("retained namespace")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .activation_request("sample-plugin", old)
            .expect_err("retained namespace")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .activation_effects("sample-plugin", old)
            .expect_err("retained namespace")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .installation_receipt("sample-plugin", old)
            .expect_err("retained namespace")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .namespace_lock("sample-plugin", false)
            .expect_err("original namespace file")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .resume_activation_preparation("sample-plugin", reservation.request_id())
            .expect_err("public competitor cannot replace the original reserved owner")
            .code,
        Code::Busy
    );
}

/// Actual successful journal/index writers remain observable after their acknowledgement returns Err or panics.
/// 实际成功日志、索引写入器在其确认返回 Err 或 panic 后仍可观察。
#[test]
fn preparation_inspection_reads_true_publication_after_error_and_panic_without_rewriting() {
    for journal in [true, false] {
        for panic in [true, false] {
            // Each unknown checkpoint owns a fresh actual namespace and original request file.
            // 每个未知检查点持有全新实际命名空间及原请求文件。
            let temp = tempfile::tempdir().expect("fixture");
            // Genuine terminal baseline proves an unindexed successful journal is not confused with own index.
            // 真实终态基线证明未索引成功日志不会与自有索引混淆。
            let (store, source, early) = early_fixture(temp.path());
            // Only original identity is retained after its real owner retires.
            // 实际所有者退役后仅保留原身份。
            let old = early.request_id();
            drop(early);
            store
                .abort_activation_preparation_request("sample-plugin", old)
                .expect("baseline");
            // The mutable original remains outside the panicking publication closure.
            // 可变原所有者保留在 panic 发布闭包之外。
            let mut reservation = reserve(&store, &source);
            // Scoped faults run after the actual successful writer, never replace it.
            // 作用域故障在实际成功写入器之后运行，绝不替代写入器。
            let _fault = store.observe_preparation_reservation_writes_for_test(
                reservation.request_id(),
                journal.then_some(panic),
                (!journal).then_some(panic),
                false,
            );
            // Catching worker failure preserves the exact namespace/request owner for sealed readback.
            // 捕获工作器失败，保留精确命名空间、请求所有者用于密封回读。
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store.publish_activation_preparation_retained(&mut reservation)
            }));
            if panic {
                assert!(outcome.is_err());
            } else {
                assert!(outcome.expect("Err outcome").is_err());
            }
            // Counters record real writes before inspection, not proposed writes.
            // 计数器记录检查前的真实写入，而不是拟议写入。
            let writes =
                store.preparation_reservation_write_counts_for_test(reservation.request_id());
            // Actual durable bytes are compared across owned observation.
            // 跨自有观察比较实际持久字节。
            let before = tree(temp.path());
            // Neither original journal nor current index is synthesized from the reserved intent.
            // 原日志及当前索引均不从预留意图合成。
            let actual = store
                .inspect_activation_preparation_reservation(&reservation)
                .expect("owned true checkpoint");
            assert!(actual.original().is_some(), "real journal must be observed");
            assert_eq!(
                actual
                    .requests()
                    .get(&reservation.request_id())
                    .expect("actual own journal")
                    .request_id(),
                reservation.request_id()
            );
            assert_eq!(
                actual.indexed().expect("real index").request_id(),
                if journal {
                    old
                } else {
                    reservation.request_id()
                }
            );
            assert!(actual.requests().contains_key(&old));
            assert!(actual.requests().contains_key(&reservation.request_id()));
            assert_eq!(tree(temp.path()), before);
            assert_eq!(
                store.preparation_reservation_write_counts_for_test(reservation.request_id()),
                writes
            );
        }
    }
}

/// Foreign store roots and a valid but changed complete original policy cannot borrow the reserved observer.
/// 外来存储根及有效但已变的完整原策略不能借用预留观察者。
#[test]
fn preparation_inspection_rejects_foreign_root_and_changed_full_original() {
    // Two real roots cannot exchange namespace ownership merely through matching plugin identities.
    // 两个真实根不能仅通过匹配插件身份交换命名空间所有权。
    let temp = tempfile::tempdir().expect("fixture");
    // The actual source is validated by the store before reservation.
    // 实际来源在预留之前由存储校验。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("store");
    // A separately validated package supplies the exact frozen fingerprint.
    // 独立校验插件包提供精确固定指纹。
    let source = write_package(&temp.path().join("source"), "sample-plugin", "new");
    // The original retains both real files while tests introduce bounded fixture corruption.
    // 测试引入有界夹具损坏时，原所有者保留两个真实文件。
    let mut reservation = reserve(&store, &source);
    // A foreign store has its own actual storage root.
    // 外来存储具有自身实际存储根。
    let foreign_temp = tempfile::tempdir().expect("foreign fixture");
    // Opening the foreign root cannot confer authority over the first root.
    // 打开外来根不能授予第一根的权威。
    let foreign = VulcanPluginGenerationStore::open(foreign_temp.path()).expect("foreign store");
    assert_eq!(
        foreign
            .inspect_activation_preparation_reservation(&reservation)
            .expect_err("foreign owner")
            .code,
        Code::Conflict
    );
    // Publish only the intended original, then retire the returned shared observer.
    // 仅发布预定原请求，随后退役返回的共享观察者。
    drop(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect("actual publication"),
    );
    // Exact actual bytes are the only source of the corruption fixture.
    // 精确实际字节是损坏夹具的唯一来源。
    let path = record_path(temp.path(), reservation.request_id());
    // Policy changes remain valid schema fields but differ from the complete frozen original.
    // 策略变化仍为有效模式字段，但与完整固定原请求不同。
    let mut body: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("actual original")).expect("JSON");
    body["preparation"]["update_policy"]["explicitly_disabled"] = serde_json::json!(false);
    fs::write(
        &path,
        serde_json::to_vec(&body).expect("changed full original"),
    )
    .expect("fixture corruption");
    // Even valid replacement bytes cannot be normalized into the reserved intent.
    // 即使替代字节有效，也不能规范化为预留意图。
    let before = tree(temp.path());
    assert_eq!(
        store
            .inspect_activation_preparation_reservation(&reservation)
            .expect_err("complete policy changed")
            .code,
        Code::Conflict
    );
    assert_eq!(tree(temp.path()), before);
}

/// A missing exact historical index is rejected rather than repaired while the original has no journal.
/// 原请求没有日志时，缺失精确历史索引会被拒绝而非修复。
#[test]
fn preparation_inspection_rejects_changed_index_baseline_without_repair() {
    // A genuine terminal original supplies the indexed baseline.
    // 真实终态原请求提供已索引基线。
    let temp = tempfile::tempdir().expect("fixture");
    // Retire original ownership before explicit abort publication.
    // 在明确中止发布之前退役原所有权。
    let (store, source, early) = early_fixture(temp.path());
    // The known original identity determines the canonical index parent.
    // 已知原身份确定规范索引父目录。
    let old = early.request_id();
    drop(early);
    store
        .abort_activation_preparation_request("sample-plugin", old)
        .expect("old terminal");
    // This reservation retains the full baseline, rather than an optional UUID alone.
    // 此预留保留完整基线，而不是仅可选 UUID。
    let reservation = reserve(&store, &source);
    fs::remove_file(record_path(temp.path(), old).with_file_name("current"))
        .expect("remove fixture index");
    // Inspection must leave the real absent index untouched.
    // 检查必须保持真实缺失索引不变。
    let before = tree(temp.path());
    assert_eq!(
        store
            .inspect_activation_preparation_reservation(&reservation)
            .expect_err("changed original baseline")
            .code,
        Code::Conflict
    );
    assert_eq!(tree(temp.path()), before);
}

/// Actual historical effect receipts survive terminal abort and are decoded through the retained reservation.
/// 实际历史副作用回执跨终态中止存续，并通过保留预留解码。
#[test]
fn preparation_inspection_reads_actual_historical_effect_sidecar() {
    // Production APIs create the configured original and its actual typed effect receipt.
    // 生产接口创建已配置原请求及其实际类型化副作用回执。
    let temp = tempfile::tempdir().expect("fixture");
    // Real distinct packages and secure revisions preserve the full original identity.
    // 真实不同插件包及安全修订保留完整原身份。
    let (store, _first, second, _first_revision, second_revision) = fixture(temp.path());
    // This configured predecessor history has a real original exclusive request file.
    // 此已配置前驱历史具有真实原排他请求文件。
    let mut transaction = begin(&store, &second, &second_revision, Uuid::new_v4());
    // Historical identity is captured before terminal retirement.
    // 历史身份在终态退役之前捕获。
    let old = transaction.record().request_id();
    // Persist real Started and Succeeded observations without executing a plugin callback in this storage test.
    // 在此存储测试中不执行插件回调，持久保存真实 Started 及 Succeeded 观察。
    store
        .inspect_activation_candidate(&transaction)
        .expect("candidate")
        .begin_effect_attempt(crate::VulcanPluginActivationEffect::Probe)
        .expect("actual effect writer")
        .finish(true)
        .expect("actual success writer");
    // Installation uses its independent real receipt writer after the successful probe prerequisite.
    // 安装在成功探测前置条件之后使用独立真实回执写入器。
    store
        .inspect_activation_candidate(&transaction)
        .expect("candidate after probe")
        .begin_installation_attempt()
        .expect("actual installation writer")
        .finish(true)
        .expect("actual installation success writer");
    store
        .abort_activation_transaction(&mut transaction)
        .expect("terminal original");
    drop(transaction);
    // Freeze the complete actual effect before acquiring the reserved namespace.
    // 在取得预留命名空间前固定完整实际副作用。
    let expected = store
        .activation_effects("sample-plugin", old)
        .expect("effect read")
        .expect("real sidecar");
    // The independent installation observation must be read from its own actual sidecar as well.
    // 独立安装观察也必须从自身实际附属文件读取。
    let expected_installation = store
        .installation_receipt("sample-plugin", old)
        .expect("installation read")
        .expect("real installation sidecar");
    // A new validated source reserves the next original after actual historical termination.
    // 新已校验来源在实际历史终结后预留下一个原请求。
    let source = write_package(&temp.path().join("third"), "sample-plugin", "third");
    // The new original acquires the only actual namespace fence.
    // 新原请求取得唯一实际命名空间屏障。
    let reservation = reserve(&store, &source);
    // Exact tree preservation covers every historical sidecar and original file.
    // 精确目录树保留覆盖每份历史附属文件及原文件。
    let before = tree(temp.path());
    // The effect is decoded from actual bytes with the original full configured body.
    // 副作用结合原完整已配置正文，从实际字节解码。
    let actual = store
        .inspect_activation_preparation_reservation(&reservation)
        .expect("history inspection");
    assert_eq!(actual.effects().get(&old), Some(&Some(expected)));
    assert_eq!(
        actual.installations().get(&old),
        Some(&Some(expected_installation))
    );
    assert_eq!(
        actual
            .requests()
            .get(&old)
            .expect("historical original")
            .phase(),
        Phase::Aborted
    );
    assert_eq!(tree(temp.path()), before);
}

/// Every declared sidecar rejects orphan, noncanonical and impossible early-original evidence without hiding it.
/// 每种声明附属格式均拒绝孤立、非规范及不可能早期原证据，不隐藏证据。
#[test]
fn preparation_inspection_rejects_actual_early_orphan_and_noncanonical_sidecars() {
    for extension in ["effects", "install", "target-hook", "target-mcp"] {
        for scenario in ["early", "orphan", "noncanonical"] {
            // Each corruption case has one independently isolated real original owner.
            // 每个损坏用例具有一个独立隔离真实原所有者。
            let temp = tempfile::tempdir().expect("fixture");
            // Empty storage makes all observed sidecars attributable to this exact injected fixture.
            // 空存储使全部已观察附属文件归属于此精确注入夹具。
            let store = VulcanPluginGenerationStore::open(temp.path()).expect("store");
            // Source content follows the normal validator.
            // 来源内容遵循正常校验器。
            let source = write_package(&temp.path().join("source"), "sample-plugin", "new");
            // Physical namespace/request exclusion remains real during malformed evidence inspection.
            // 损坏证据检查期间，物理命名空间、请求排他仍保持真实。
            let mut reservation = reserve(&store, &source);
            drop(
                store
                    .publish_activation_preparation_retained(&mut reservation)
                    .expect("actual early journal"),
            );
            // The declared corruption branch determines one exact sidecar identity, with no path probing.
            // 声明损坏分支确定一个精确附属身份，不探测路径。
            let name = match scenario {
                "early" => reservation.request_id().to_string(),
                "orphan" => Uuid::new_v4().to_string(),
                "noncanonical" => "not-a-canonical-request".to_owned(),
                _ => unreachable!("declared test scenarios"),
            };
            // An impossible early/orphan file is itself corruption; it must not be ignored as absent.
            // 不可能早期、孤立文件本身即损坏；不得作为缺失忽略。
            let path = record_path(temp.path(), reservation.request_id())
                .with_file_name(format!("{name}.{extension}"));
            fs::write(&path, b"{}").expect("actual corrupt sidecar");
            // No failure path may repair, delete or normalize this actual evidence.
            // 任何失败路径均不能修复、删除或规范化此实际证据。
            let before = tree(temp.path());
            assert!(
                store
                    .inspect_activation_preparation_reservation(&reservation)
                    .is_err(),
                "{scenario} {extension} must be rejected"
            );
            assert_eq!(tree(temp.path()), before);
        }
    }
}

/// A true published original renamed to an ASCII journal alias must never disappear from its historical inventory.
/// 真正已发布原请求重命名为 ASCII 日志别名后，绝不能从其历史清单消失。
#[cfg(windows)]
#[test]
fn preparation_inspection_rejects_published_original_json_case_alias() {
    // A real first writer publishes the exact original before its filename alone changes.
    // 真实首次写入器在仅改变文件名之前发布精确原请求。
    let temp = tempfile::tempdir().expect("fixture");
    // This isolated store owns no unrelated user history.
    // 此隔离存储不持有无关用户历史。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("store");
    // Production validation establishes the approved source.
    // 生产校验确定批准来源。
    let source = write_package(&temp.path().join("source"), "sample-plugin", "new");
    // The same original request/namespace owner survives the true publication and rename.
    // 同一原请求、命名空间所有者跨真实发布及重命名存续。
    let mut reservation = reserve(&store, &source);
    drop(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect("true original writer"),
    );
    // The sole canonical resolver determines the actual journal being renamed.
    // 唯一规范解析器确定实际被重命名日志。
    let canonical = record_path(temp.path(), reservation.request_id());
    // Preserve the true body's bytes so filename rejection cannot hide an accidental content mutation.
    // 保留真实正文字节，避免文件名拒绝隐藏意外内容变更。
    let bytes = fs::read(&canonical).expect("published original bytes");
    // Only ASCII case changes, with no alternate request identity or fabricated journal.
    // 仅改变 ASCII 大小写，不替换请求身份或伪造日志。
    let alias = canonical.with_extension("JSON");
    fs::rename(&canonical, &alias).expect("actual original case rename");
    assert_eq!(fs::read(&alias).expect("renamed true body"), bytes);
    assert!(
        fs::read_dir(alias.parent().expect("journal parent"))
            .expect("actual filenames")
            .any(|entry| entry.expect("actual entry").file_name()
                == alias.file_name().expect("alias name")),
        "the actual directory must contain the noncanonical spelling"
    );
    // On this Windows filesystem, canonical lookup still sees the actual aliased original.
    // 在此 Windows 文件系统上，规范查找仍可看到实际别名原请求。
    #[cfg(windows)]
    assert!(
        store
            .activation_preparation_reservation_record(&reservation)
            .expect("canonical alias lookup")
            .is_some()
    );
    // Public sealed inspection must report the unique inventory's exact filename rejection after checkpoint lookup succeeds.
    // 检查点查找成功后，公开密封检查须报告唯一清单的精确文件名拒绝。
    let before = tree(temp.path());
    assert_eq!(
        store
            .inspect_activation_preparation_reservation(&reservation)
            .expect_err("aliased actual original")
            .message,
        "Noncanonical activation transaction extension."
    );
    assert_eq!(tree(temp.path()), before);
}

/// A terminal historical journal renamed to mixed ASCII case cannot be silently omitted from complete history.
/// 终态历史日志重命名为混合 ASCII 大小写后，不能从完整历史静默遗漏。
#[test]
fn preparation_inspection_rejects_historical_terminal_json_case_alias() {
    // Actual terminal publication precedes the next original reservation.
    // 实际终态发布先于下一个原请求预留。
    let temp = tempfile::tempdir().expect("fixture");
    // The historical body is generated by real early-intent and abort writers.
    // 历史正文由真实早期意图及中止写入器生成。
    let (store, source, early) = early_fixture(temp.path());
    // Retain identity alone before retiring its actual original owner.
    // 退役实际原所有者之前仅保留身份。
    let historical = early.request_id();
    drop(early);
    store
        .abort_activation_preparation_request("sample-plugin", historical)
        .expect("actual terminal history");
    // Capture the baseline before changing only its literal filename inside the held namespace.
    // 在保留命名空间内仅改变字面文件名之前捕获基线。
    let mut reservation = reserve(&store, &source);
    // A true new original/index checkpoint keeps historical alias rejection independent of platform lookup semantics.
    // 真实新原请求、索引检查点使历史别名拒绝独立于平台查找语义。
    drop(
        store
            .publish_activation_preparation_retained(&mut reservation)
            .expect("actual new original checkpoint"),
    );
    // Actual terminal bytes remain intact under the aliased suffix.
    // 实际终态字节在别名后缀下保持完整。
    let canonical = record_path(temp.path(), historical);
    // The mixed-case alias is a real rename, not an additional speculative file.
    // 混合大小写别名属于真实重命名，而非额外投机文件。
    let alias = canonical.with_extension("JsOn");
    fs::rename(&canonical, &alias).expect("actual historical case rename");
    // No inspection path may normalize the filename or rewrite historical evidence.
    // 任何检查路径均不能规范化文件名或重写历史证据。
    let before = tree(temp.path());
    assert_eq!(
        store
            .inspect_activation_preparation_reservation(&reservation)
            .expect_err("aliased historical original")
            .message,
        "Noncanonical activation transaction extension."
    );
    assert_eq!(tree(temp.path()), before);
}

/// Four declared sidecar formats reject actual ASCII aliases for early originals, orphans and configured historical UUIDs.
/// 四种声明附属格式拒绝早期原请求、孤立文件及已配置历史 UUID 的实际 ASCII 别名。
#[test]
fn preparation_inspection_rejects_sidecar_case_aliases_for_early_orphan_and_history() {
    for extension in ["EFFECTS", "INSTALL", "TARGET-HOOK", "TARGET-MCP"] {
        for scenario in ["early", "orphan", "history"] {
            // Every case has a real independent original request and namespace file owner.
            // 每个用例均具有真实独立原请求及命名空间文件所有者。
            let temp = tempfile::tempdir().expect("fixture");
            // The historical branch uses a true configured terminal request, rather than merely a well-shaped UUID.
            // 历史分支使用真实已配置终态请求，而不仅是形状正确的 UUID。
            let (store, source, historical) = if scenario == "history" {
                // Actual immutable packages and credentials establish the configured historical original.
                // 实际不可变插件包及凭据确定已配置历史原请求。
                let (store, _first, second, _old_revision, revision) = fixture(temp.path());
                // Its actual request owner retires only after real terminal abort publication.
                // 其实际请求所有者仅在真实终态中止发布后退役。
                let mut transaction = begin(&store, &second, &revision, Uuid::new_v4());
                // Retain the known configured identity without extending original ownership.
                // 保留已知已配置身份，不延长原所有权。
                let historical = transaction.record().request_id();
                store
                    .abort_activation_transaction(&mut transaction)
                    .expect("historical terminal");
                drop(transaction);
                // Distinct source content supplies the new original reservation.
                // 不同来源内容提供新原请求预留。
                let source = write_package(&temp.path().join("third"), "sample-plugin", "third");
                (store, source, Some(historical))
            } else {
                // Actual absence isolates early and orphan evidence from unrelated historical receipts.
                // 实际缺失将早期及孤立证据与无关历史回执隔离。
                let store = VulcanPluginGenerationStore::open(temp.path()).expect("store");
                // Validate the source through the production package inspector.
                // 通过生产插件包检查器校验来源。
                let source = write_package(&temp.path().join("source"), "sample-plugin", "new");
                (store, source, None)
            };
            // Publish actual Preparing before introducing any impossible sidecar evidence.
            // 在引入任何不可能附属证据之前发布实际 Preparing。
            let mut reservation = reserve(&store, &source);
            drop(
                store
                    .publish_activation_preparation_retained(&mut reservation)
                    .expect("actual original journal"),
            );
            // The declared fixture branch gives one exact sidecar identity, without fallback selection.
            // 声明夹具分支提供一个精确附属身份，不使用兜底选择。
            let sidecar_request = match scenario {
                "early" => reservation.request_id(),
                "orphan" => Uuid::new_v4(),
                "history" => historical.expect("true configured historical identity"),
                _ => unreachable!("declared test scenarios"),
            };
            // An actual noncanonical sidecar must be rejected before being treated as absent or normalized into success.
            // 实际非规范附属文件须被拒绝，不能视为缺失或规范化为成功。
            let alias = record_path(temp.path(), sidecar_request).with_extension(extension);
            fs::write(&alias, b"{}").expect("actual aliased sidecar fixture");
            // Both successful and rejecting observations must leave all original bytes and modification times unchanged.
            // 成功及拒绝观察均须保持全部原字节及修改时间不变。
            let before = tree(temp.path());
            // Orphans and history reach suffix validation; early Hook/MCP can fail earlier at the unchanged original checkpoint.
            // 孤立及历史文件进入后缀校验；早期 Hook、MCP 可在未变原检查点提前失败。
            let error = store
                .inspect_activation_preparation_reservation(&reservation)
                .expect_err("actual aliased sidecar must never become an absent projection");
            if scenario != "early" || matches!(extension, "EFFECTS" | "INSTALL") {
                assert_eq!(
                    error.message, "Noncanonical reserved request sidecar extension.",
                    "{scenario} {extension} must reject spelling before any decoder could mask normalization"
                );
            }
            assert_eq!(tree(temp.path()), before);
        }
    }
}

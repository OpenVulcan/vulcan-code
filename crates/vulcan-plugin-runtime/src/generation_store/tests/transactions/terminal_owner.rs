//! Cross-manager successors wait for actual original terminal file ownership to retire after uncertain writes.
//! 不确定写入后，跨管理器后继等待实际原终态文件所有权退役。

use super::*;

/// Exercise actual `phase` terminal write loss, same-owner readback and physical release before a new UUID begins.
/// 验证实际 `phase` 终态写入丢失、相同所有者回读及新 UUID 开始前物理释放。
fn verify_terminal_owner_fences_successor(phase: Phase) {
    // Each actual terminal case owns independent managed files, revisions and complete selected authority.
    // 每个实际终态场景拥有独立托管文件、修订及完整所选权威。
    let temp = tempfile::tempdir().expect("create original terminal owner fixture");
    // Actual predecessor and target bytes remain pinned independently from original request ownership.
    // 实际前驱及目标字节独立于原请求所有权保持固定。
    let (store, _first, second, _old_credentials, credentials) = fixture(temp.path());
    // A separate real handle reproduces another manager, without sharing a service mutex or transaction object.
    // 独立真实句柄复现另一管理器，不共享服务互斥锁或事务对象。
    let competing =
        VulcanPluginGenerationStore::open(temp.path()).expect("open competing terminal store");
    // One actual original request survives the real write's missing result and immediate readback.
    // 单个实际原请求在真实写入结果及即时回读缺失期间存续。
    let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    if phase == Phase::Completed {
        store
            .commit_activation_transaction(&mut transaction)
            .expect("publish actual target before terminal completion");
    }
    // Freeze complete local authority and the sole original index before invoking the real terminal writer.
    // 调用真实终态写入者前固定完整局部权威及唯一原索引。
    let original = transaction.record().clone();
    // The original index cannot be reassigned while the terminal owner's actual file remains busy.
    // 终态所有者实际文件仍繁忙时，原索引不能重新分配。
    let index_path = store.root.join("transactions/sample-plugin/current");
    // Preserve exact index bytes rather than checking only a reported original UUID.
    // 保留精确索引字节，而非仅检查报告的原 UUID。
    let index_before = fs::read(&index_path).expect("read exact original terminal index");
    // Actual scoped loss runs only after the selected real checkpoint write has succeeded.
    // 实际作用域丢失仅在选定真实检查点写入成功后发生。
    let _loss = store.lose_activation_checkpoint_observation_once(original.request_id(), phase);
    match phase {
        Phase::Completed => assert_eq!(
            store
                .complete_activation_transaction(&mut transaction)
                .expect_err("real Completed write observation lost")
                .code,
            Code::Storage
        ),
        Phase::Aborted => assert_eq!(
            store
                .abort_activation_transaction(&mut transaction)
                .expect_err("real Aborted write observation lost")
                .code,
            Code::Storage
        ),
        _ => panic!("terminal owner fixture only supports actual Completed or Aborted"),
    }
    assert_eq!(
        transaction.record(),
        &original,
        "unknown write keeps the complete original local observation"
    );
    assert_eq!(
        store
            .activation_transaction("sample-plugin", original.request_id())
            .expect("read actual terminal checkpoint")
            .expect("actual original checkpoint exists")
            .phase(),
        phase
    );
    // Preserve the real durable terminal bytes across competing rejection and same-owner confirmation.
    // 在拒绝竞争及相同所有者确认期间保留真实持久终态字节。
    let journal_before = fs::read(record_path(temp.path(), original.request_id()))
        .expect("read real terminal original bytes");
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut transaction)
            .expect_err("immediate same-owner readback observation remains unknown")
            .code,
        Code::Storage
    );
    // The competing update uses a genuinely different new UUID, never a replacement owner token for the original.
    // 竞争更新使用真正不同新 UUID，绝不使用原请求的替换所有者令牌。
    let successor_request = Uuid::new_v4();
    assert_ne!(successor_request, original.request_id());
    // The current exact revision is known from the actual selected activation for this terminal branch.
    // 此终态分支的当前精确修订来自实际所选激活。
    let expected_revision = if phase == Phase::Completed { 2 } else { 1 };
    assert_eq!(
        competing
            .begin_activation_transaction(
                successor_request,
                &second,
                expected_revision,
                vec![],
                credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("terminal original's actual live File excludes new UUID")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .deactivate("sample-plugin", expected_revision)
            .expect_err("another selection writer also waits for original owner")
            .code,
        Code::Busy
    );
    assert_eq!(
        fs::read(&index_path).expect("unchanged original index after competing rejection"),
        index_before
    );
    assert_eq!(
        fs::read(record_path(temp.path(), original.request_id()))
            .expect("unchanged terminal bytes after competing rejection"),
        journal_before
    );
    assert!(
        competing
            .activation_transaction("sample-plugin", successor_request)
            .expect("new UUID did not allocate a request")
            .is_none()
    );
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut transaction)
            .expect("same original owner confirms exact terminal checkpoint"),
        phase
    );
    assert_eq!(transaction.record().request_id(), original.request_id());
    assert_eq!(
        competing
            .begin_activation_transaction(
                successor_request,
                &second,
                expected_revision,
                vec![],
                credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("confirmation alone does not retire actual original File")
            .code,
        Code::Busy
    );
    // Retiring the actual owner releases the same original exclusive file without touching route or package pins.
    // 退役实际所有者释放相同原排他文件，不触及路由或包固定句柄。
    drop(transaction);
    // The same rejected new UUID may now begin once real original ownership has retired.
    // 真实原所有权退役后，相同先前被拒绝新 UUID 现在可以开始。
    let successor = competing
        .begin_activation_transaction(
            successor_request,
            &second,
            expected_revision,
            vec![],
            credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("new UUID begins only after actual original retirement");
    assert_eq!(successor.record().request_id(), successor_request);
    assert_eq!(
        fs::read(record_path(temp.path(), original.request_id()))
            .expect("historical original terminal bytes remain exact"),
        journal_before
    );
    assert!(
        store
            .activation_effects("sample-plugin", original.request_id())
            .expect("no old callback replay")
            .is_none()
    );
}

/// A real Completed write with two lost observations keeps the original actual file busy across another manager.
/// 两次观测丢失的真实 Completed 写入跨另一管理器保留原实际繁忙文件。
#[test]
fn terminal_owner_completed_unknown_blocks_new_uuid_until_actual_original_release() {
    verify_terminal_owner_fences_successor(Phase::Completed);
}

/// A real Aborted write with two lost observations cannot release new update authority before original owner drop.
/// 两次观测丢失的真实 Aborted 写入不能在原所有者释放前授予新更新权威。
#[test]
fn terminal_owner_aborted_unknown_blocks_new_uuid_until_actual_original_release() {
    verify_terminal_owner_fences_successor(Phase::Aborted);
}

/// A known terminal request missing its actual original file fails closed without creating a substitute lease.
/// 已知终态请求缺失实际原文件时保持关闭，不创建替代租约。
#[test]
fn terminal_owner_missing_actual_lease_is_invalid_and_never_recreated() {
    // Complete a genuine original request through the real writer before removing its retired fixture file.
    // 删除已退役夹具文件前，通过真实写入者完成真实原请求。
    let temp = tempfile::tempdir().expect("create missing terminal lease fixture");
    // Actual immutable target identity stays available without retaining request ownership.
    // 实际不可变目标身份保持可用，不保留请求所有权。
    let (store, _first, second, _old_credentials, credentials) = fixture(temp.path());
    // The exact original owner is retired explicitly before the fixture deletes its physical lease entry.
    // 夹具删除物理租约条目前，精确原所有者被明确退役。
    let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    store
        .abort_activation_transaction(&mut transaction)
        .expect("write actual terminal original");
    // Resolve the same unique path used by acquisition and read-only retirement observation.
    // 解析取得及只读退役观测使用的相同唯一路径。
    let path = store.transaction_lease_path("sample-plugin", transaction.record().request_id());
    drop(transaction);
    fs::remove_file(&path).expect("remove known retired original lease fixture");
    assert_eq!(
        store
            .begin_activation_transaction(
                Uuid::new_v4(),
                &second,
                1,
                vec![],
                credentials,
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("missing terminal original lease is corruption")
            .code,
        Code::Invalid
    );
    assert!(
        !path.exists(),
        "read-only terminal probe must not manufacture a replacement File"
    );
}

/// Restore a genuine older unindexed full Prepared body while the current original's real Aborted write is unknown.
/// 当前原请求真实 Aborted 写入未知时，恢复真实较旧未索引完整 Prepared 正文。
#[test]
fn terminal_owner_historical_unindexed_transaction_recovery_waits_for_original_release() {
    // This explicitly reconstructed historical storage boundary is not claimed to arise from the corrected normal flow.
    // 此明确重建历史存储边界不宣称来自已修正正常流程。
    let temp = tempfile::tempdir().expect("create historical full request recovery fixture");
    // Every historical body comes from actual public preparation and retains valid immutable package authority.
    // 每个历史正文都来自实际公开准备，并保留有效不可变包权威。
    let (store, _first, second, _old_credentials, credentials) = fixture(temp.path());
    // The older request is genuinely prepared and owns its actual file before its bytes are preserved.
    // 较旧请求在保存字节前被真实准备，并拥有实际文件。
    let mut historical = begin(&store, &second, &credentials, Uuid::new_v4());
    // Preserve its exact actual Prepared body, never synthesize a UUID-only owner or manually edit its phase.
    // 保留精确实际 Prepared 正文，绝不合成仅 UUID 所有者或手工修改阶段。
    let historical_id = historical.record().request_id();
    // Canonical bytes are saved before its genuine abort and physical owner retirement.
    // 在真实中止及物理所有者退役前保存规范字节。
    let historical_bytes = fs::read(record_path(temp.path(), historical_id))
        .expect("save genuine historical Prepared bytes");
    store
        .abort_activation_transaction(&mut historical)
        .expect("real older abort before allocating current original");
    drop(historical);
    // The current request is a genuinely different original with its own actual file and unchanged predecessor.
    // 当前请求是真正不同原请求，具有自有实际文件及未变前驱。
    let mut current = begin(&store, &second, &credentials, Uuid::new_v4());
    // Exact current full authority must survive the historical recovery attempt.
    // 精确当前完整权威必须在历史恢复尝试后保留。
    let current_id = current.record().request_id();
    fs::write(record_path(temp.path(), historical_id), &historical_bytes)
        .expect("reconstruct the canonical historical unindexed boundary from genuine bytes");
    // Only the current real terminal writer supplies Aborted evidence; this fixture never edits its phase.
    // 仅当前真实终态写入者提供 Aborted 证据；此夹具绝不修改其阶段。
    let _loss = store.lose_activation_checkpoint_observation_once(current_id, Phase::Aborted);
    assert_eq!(
        store
            .abort_activation_transaction(&mut current)
            .expect_err("actual current Aborted observation lost")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut current)
            .expect_err("actual current immediate readback observation lost")
            .code,
        Code::Storage
    );
    // Another manager attempts explicit recovery of the older actual Prepared request rather than normal new begin.
    // 另一管理器尝试明确恢复较旧实际 Prepared 请求，而非普通新开始。
    let competing =
        VulcanPluginGenerationStore::open(temp.path()).expect("open historical recovery observer");
    // Save original current bytes so failed recovery cannot quietly replace the still-owned terminal index.
    // 保存原 current 字节，使失败恢复不能静默替换仍自有终态索引。
    let index_path = store.root.join("transactions/sample-plugin/current");
    // Full current index bytes bind the still-live actual terminal owner.
    // 完整 current 索引字节绑定仍存活实际终态所有者。
    let current_index = fs::read(&index_path).expect("save exact current original index");
    assert_eq!(
        competing
            .resume_activation_transaction("sample-plugin", historical_id)
            .expect_err("historical UUID must not replace actual current terminal owner")
            .code,
        Code::Busy
    );
    assert_eq!(
        fs::read(&index_path).expect("current original index unchanged"),
        current_index
    );
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut current)
            .expect("current same owner still confirms exact real Aborted"),
        Phase::Aborted
    );
    drop(current);
    // Real original file retirement now permits the known historical request to recover its own index.
    // 真实原文件退役现在允许已知历史请求恢复自有索引。
    let recovered = competing
        .resume_activation_transaction("sample-plugin", historical_id)
        .expect("explicit historical full recovery after actual original retirement");
    assert_eq!(recovered.record().request_id(), historical_id);
    assert_eq!(recovered.record().phase(), Phase::Prepared);
    assert_eq!(
        fs::read(record_path(temp.path(), historical_id))
            .expect("historical full body stays exact"),
        historical_bytes
    );
}

/// Canonical older early-preparation recovery cannot replace a current original whose real terminal file stays owned.
/// 规范较旧早期准备恢复不能替换真实终态文件仍自有的当前原请求。
#[test]
fn terminal_owner_historical_unindexed_preparation_recovery_waits_for_original_release() {
    // Genuine API-produced historical bytes reproduce the storage boundary without claiming a new ordinary flow.
    // 真实 API 产出历史字节复现存储边界，不宣称新普通流程。
    let temp = tempfile::tempdir().expect("create historical early recovery fixture");
    // The immutable selected predecessor and candidate inventory use the actual configured shared fixture.
    // 不可变所选前驱及候选清单使用实际已配置共享夹具。
    let (store, _first, second, _old_credentials, credentials) = fixture(temp.path());
    // Early preparation reads the known inspected source that produced the actual candidate package.
    // 早期准备读取产出实际候选包的已知已检查来源。
    let source = temp.path().join("second/sample-plugin");
    // Complete actual predecessor is retained exactly, rather than inferred from a revision number.
    // 完整实际前驱被精确保留，而非从修订号推断。
    let predecessor = store
        .activation("sample-plugin")
        .expect("read exact early predecessor");
    // A genuine earlier original receives its own request file and durable Preparing body.
    // 真实较早原请求取得自有请求文件及持久 Preparing 正文。
    let historical = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &second.identity().content_fingerprint,
            predecessor.as_ref(),
            vec![],
        )
        .expect("begin genuine historical early request");
    // Preserve canonical bytes before retiring this genuine earlier owner and its actual abort writer.
    // 退役此真实较早所有者及执行实际中止写入者前保留规范字节。
    let historical_id = historical.request_id();
    // Only original API-generated bytes are restored for the explicit historical boundary fixture.
    // 仅为明确历史边界夹具恢复原 API 生成字节。
    let historical_bytes = fs::read(record_path(temp.path(), historical_id))
        .expect("save genuine historical early bytes");
    drop(historical);
    store
        .abort_activation_preparation_request("sample-plugin", historical_id)
        .expect("real older early abort before current request");
    // The current full original has a different actual file and a genuine complete target.
    // 当前完整原请求具有不同实际文件及真实完整目标。
    let mut current = begin(&store, &second, &credentials, Uuid::new_v4());
    // The current original's UUID stays unchanged across true writer loss and exact owned reconciliation.
    // 当前原 UUID 在真实写入者丢失及精确自有核对期间保持未变。
    let current_id = current.record().request_id();
    fs::write(record_path(temp.path(), historical_id), &historical_bytes)
        .expect("restore exact genuine unindexed early historical bytes");
    // Real Aborted bytes come only from the current actual writer, never from fixture phase mutation.
    // 真实 Aborted 字节仅来自当前实际写入者，绝不来自夹具阶段变更。
    let _loss = store.lose_activation_checkpoint_observation_once(current_id, Phase::Aborted);
    assert_eq!(
        store
            .abort_activation_transaction(&mut current)
            .expect_err("actual current terminal write result lost")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut current)
            .expect_err("actual current immediate readback lost")
            .code,
        Code::Storage
    );
    // A separate manager tries the existing early resumption and sole index-repair path.
    // 独立管理器尝试既有早期恢复及唯一索引修复路径。
    let competing = VulcanPluginGenerationStore::open(temp.path())
        .expect("open competing early recovery observer");
    // Preserve exact current index to distinguish denial from hidden replacement followed by an error.
    // 保留精确 current 索引，以区分拒绝与隐藏替换后报错。
    let index_path = store.root.join("transactions/sample-plugin/current");
    // The byte snapshot belongs to the current original rather than the older historical request.
    // 字节快照属于当前原请求，而非较旧历史请求。
    let current_index = fs::read(&index_path).expect("save exact current early-recovery index");
    assert_eq!(
        competing
            .resume_activation_preparation("sample-plugin", historical_id)
            .expect_err("historical early owner must not steal terminal current index")
            .code,
        Code::Busy
    );
    assert_eq!(
        fs::read(&index_path).expect("actual current index remains exact"),
        current_index
    );
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut current)
            .expect("actual current owner confirms exact original abort"),
        Phase::Aborted
    );
    drop(current);
    // The actual retired current file now permits recovery of the explicitly named original early request.
    // 实际已退役当前文件现在允许恢复明确命名原早期请求。
    let recovered = competing
        .resume_activation_preparation("sample-plugin", historical_id)
        .expect("historical early recovery after real current retirement");
    assert_eq!(recovered.request_id(), historical_id);
    assert_eq!(
        fs::read(record_path(temp.path(), historical_id))
            .expect("historical early body remains exact"),
        historical_bytes
    );
}

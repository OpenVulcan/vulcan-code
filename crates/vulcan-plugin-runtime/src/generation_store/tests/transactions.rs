//! Actual files and request owners verify uncertain publication, crash retention and original-request recovery.
//! 实际文件及请求所有者验证不确定发布、崩溃保留及原请求恢复。

use super::credentials::{EmptyCredentialBackend, stage_empty_credentials};
use super::*;
use VulcanPluginActivationTransactionPhase as Phase;
use VulcanPluginStoreErrorCode as Code;
use uuid::Uuid;

mod candidates;
mod recovery;

/// Build two distinct immutable packages for one namespace and return their separately staged credential identities.
/// 为同一命名空间构建两个不同不可变插件包，并返回分别暂存的凭据身份。
fn fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    PinnedVulcanPluginGeneration,
    PinnedVulcanPluginGeneration,
    VulcanPluginCredentialRevision,
    VulcanPluginCredentialRevision,
) {
    // Each test controls a fresh store without touching installed plugins or user secrets.
    // 每项测试控制全新存储，不触及已安装插件或用户秘密。
    let store = VulcanPluginGenerationStore::open(root).expect("open transaction store");
    // The two actual package digests distinguish predecessor and candidate ownership.
    // 两个实际插件包摘要区分前驱及候选所有权。
    let first = prepare(
        &store,
        &write_package(&root.join("first"), "sample-plugin", "old"),
    );
    // Separate physical identity prevents same-package tests from hiding premature reclamation.
    // 独立物理身份避免同包测试掩盖提前回收。
    let second = prepare(
        &store,
        &write_package(&root.join("second"), "sample-plugin", "new"),
    );
    // Independent staged revisions retain no actual secret slots in this fixture.
    // 独立已暂存修订在此夹具中不持有实际秘密槽位。
    let first_credentials = VulcanPluginCredentialRevision::allocate();
    // The target revision must remain protected even before activation selects it.
    // 即使激活尚未选择目标修订，它仍须受保护。
    let second_credentials = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &first, &first_credentials);
    stage_empty_credentials(&store, &second, &second_credentials);
    store
        .activate_configured(
            &first,
            0,
            vec![],
            first_credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select original complete authority");
    (store, first, second, first_credentials, second_credentials)
}

/// Prepare the target through the public API at revision one; return its actual exclusive owner.
/// 在修订一通过公开接口准备目标；返回实际排他所有者。
fn begin(
    store: &VulcanPluginGenerationStore,
    package: &PinnedVulcanPluginGeneration,
    credentials: &VulcanPluginCredentialRevision,
    request_id: Uuid,
) -> VulcanPluginActivationTransaction {
    store
        .begin_activation_transaction(
            request_id,
            package,
            1,
            vec![],
            credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("prepare exact target transaction")
}

/// Resolve this fixture's original request record path; no current-index inference is involved.
/// 解析此夹具的原请求记录路径；不涉及当前索引推断。
fn record_path(root: &Path, request_id: Uuid) -> PathBuf {
    root.join(STORE_DIRECTORY)
        .join("transactions/sample-plugin")
        .join(format!("{request_id}.json"))
}

/// Verify exclusive ownership, all-writer fencing, crash-surviving retention and durable historical receipts.
/// 验证排他所有权、全部写入者屏障、跨崩溃保留及持久历史回执。
#[test]
fn activation_transaction_fences_writers_and_retains_both_sides_until_handoff() {
    // Actual file locks and records survive dropping every original Rust observer.
    // 实际文件锁及记录在释放全部原 Rust 观察者后仍存续。
    let temp = tempfile::tempdir().expect("create transaction fixture");
    // Actual predecessor and target packages have separately staged credential identities.
    // 实际前驱及目标插件包具有分别暂存的凭据身份。
    let (store, first, second, first_credentials, second_credentials) = fixture(temp.path());
    // Retain identity alone so later assertions cannot accidentally keep a live package lease.
    // 仅保留身份，避免后续断言意外持有活动插件包租约。
    let first_identity = first.identity().clone();
    // Keep the exact candidate identity independently of its live owner.
    // 独立于活动所有者保留精确候选身份。
    let second_identity = second.identity().clone();
    // The canonical request identity remains stable across discovery and retries.
    // 规范请求身份在发现及重试过程中保持稳定。
    let request_id = Uuid::new_v4();
    // Hold the real request owner until this test explicitly releases or advances it.
    // 持有实际请求所有者，直到测试明确释放或推进它。
    let transaction = begin(&store, &second, &second_credentials, request_id);
    // A separate store object must recover solely from persistent records and OS locks.
    // 独立存储对象必须仅按持久记录及 OS 锁恢复。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("open second observer")
        .expect("store exists");
    assert_eq!(
        reopened
            .resume_activation_transaction("sample-plugin", request_id)
            .expect_err("live owner excludes resumption")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("direct removal cannot bypass pending request")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .activate_configured(
                &second,
                1,
                vec![],
                second_credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("direct configured write is fenced")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .begin_activation_transaction(
                Uuid::new_v4(),
                &second,
                1,
                vec![],
                second_credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("second request is fenced")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("read prepared predecessor")
            .expect("original selection")
            .revision,
        1
    );
    drop(transaction);
    drop(first);
    drop(second);
    assert_eq!(
        reopened
            .collect(&second_identity)
            .expect_err("candidate retained without live owner")
            .code,
        Code::Busy
    );
    assert_eq!(
        reopened
            .collect_credentials(
                "sample-plugin",
                &second_credentials,
                &EmptyCredentialBackend
            )
            .expect_err("candidate credentials retained after owner drop")
            .code,
        Code::Busy
    );
    // Recovery obtains original identities, never a regenerated request or activation revision.
    // 恢复取得原身份，绝不重新生成请求或激活修订。
    let mut transaction = reopened
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("resume prepared request");
    assert!(
        reopened
            .complete_activation_transaction(&mut transaction)
            .is_err()
    );
    // The returned target must be the exact unique successor, even on retry.
    // 即使重试，返回目标也必须为精确唯一后继。
    let published = reopened
        .commit_activation_transaction(&mut transaction)
        .expect("publish exact target");
    assert_eq!(published.revision, 2);
    assert_eq!(
        reopened
            .commit_activation_transaction(&mut transaction)
            .expect("idempotent publication readback"),
        published
    );
    assert!(
        reopened
            .abort_activation_transaction(&mut transaction)
            .is_err()
    );
    drop(transaction);
    assert_eq!(
        store
            .collect(&first_identity)
            .expect_err("old package retained through unacknowledged handoff")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect_credentials("sample-plugin", &first_credentials, &EmptyCredentialBackend)
            .expect_err("old credentials retained through unacknowledged handoff")
            .code,
        Code::Busy
    );
    // Hold the real request owner until this test explicitly releases or advances it.
    // 持有实际请求所有者，直到测试明确释放或推进它。
    let mut transaction = store
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("resume published handoff");
    store
        .complete_activation_transaction(&mut transaction)
        .expect("acknowledge actual host handoff");
    drop(transaction);
    assert!(
        store
            .collect(&first_identity)
            .expect("collect old package after handoff")
    );
    assert!(
        store
            .collect_credentials("sample-plugin", &first_credentials, &EmptyCredentialBackend)
            .expect("collect old credential inventory after handoff")
    );
    // A subsequent request can abort without erasing the original completed receipt.
    // 后续请求可以中止，而不擦除原已完成回执。
    let current_package = store.pin(&second_identity).expect("pin selected successor");
    // A later request has its own identity and cannot overwrite historical receipts.
    // 后续请求具有自身身份，不能覆盖历史回执。
    let next_id = Uuid::new_v4();
    // The later request exercises ownership independently of the historical receipt.
    // 后续请求独立于历史回执验证所有权。
    let mut next = store
        .begin_activation_transaction(
            next_id,
            &current_package,
            2,
            vec![],
            second_credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("prepare next independent request");
    store
        .abort_activation_transaction(&mut next)
        .expect("abort before any activation change");
    assert_eq!(
        store
            .activation_transaction("sample-plugin", request_id)
            .expect("query historical receipt")
            .expect("retained receipt")
            .phase(),
        Phase::Completed
    );
    assert_eq!(
        store
            .activation_transaction("sample-plugin", next_id)
            .expect("query aborted receipt")
            .expect("retained abort")
            .phase(),
        Phase::Aborted
    );
    assert_eq!(
        store
            .activation_transaction_ids("sample-plugin")
            .expect("discover original requests"),
        [request_id, next_id].into_iter().collect()
    );
    assert_eq!(
        store
            .activation_transaction_plugin_ids()
            .expect("discover namespaces"),
        ["sample-plugin".to_owned()].into_iter().collect()
    );
}

/// Reproduce both persisted commit windows without test-only production hooks or replaying readiness work.
/// 不使用生产测试钩子、不重放就绪工作，重现两个持久提交窗口。
#[test]
fn activation_transaction_recovers_original_commit_before_and_after_selection_write() {
    for selected in [false, true] {
        // All filesystem mutations are confined to this disposable fixture.
        // 全部文件系统变更限制在此可丢弃夹具中。
        let temp = tempfile::tempdir().expect("create recovery fixture");
        // Actual predecessor and target packages have separately staged credential identities.
        // 实际前驱及目标插件包具有分别暂存的凭据身份。
        let (store, first, second, _, credentials) = fixture(temp.path());
        // The canonical request identity remains stable across discovery and retries.
        // 规范请求身份在发现及重试过程中保持稳定。
        let request_id = Uuid::new_v4();
        // Hold the real request owner until this test explicitly releases or advances it.
        // 持有实际请求所有者，直到测试明确释放或推进它。
        let transaction = begin(&store, &second, &credentials, request_id);
        // Keep the original complete target for exact post-recovery comparison.
        // 保留原完整目标，用于恢复后精确比较。
        let target = transaction.record().target().clone();
        // Write the exact checkpoint a process would leave before the final acknowledgement.
        // 写入进程在最终确认前会留下的精确检查点。
        let mut record =
            serde_json::to_value(transaction.record()).expect("encode original journal");
        record["phase"] = serde_json::json!("committing");
        fs::write(
            record_path(temp.path(), request_id),
            serde_json::to_vec(&record).expect("encode commit checkpoint"),
        )
        .expect("persist interrupted commit checkpoint");
        if selected {
            fs::write(
                store.root.join("active/sample-plugin.json"),
                serde_json::to_vec(&target).expect("encode published target"),
            )
            .expect("persist exact selected target");
        }
        drop(transaction);
        drop(first);
        drop(second);
        // Reopen and resume from disk rather than preserving an in-memory phase machine.
        // 从磁盘重开并恢复，而不保留内存阶段状态机。
        let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
            .expect("reopen crashed store")
            .expect("store exists");
        // Retain actual selection bytes to detect a second unnecessary publication.
        // 保留实际选择字节，以检测不必要的再次发布。
        let before = fs::read(store.root.join("active/sample-plugin.json"))
            .expect("read pre-recovery selection bytes");
        // Resume the original request from its durable checkpoint.
        // 从持久检查点恢复原请求。
        let mut resumed = reopened
            .resume_activation_transaction("sample-plugin", request_id)
            .expect("resume original committing request");
        assert_eq!(resumed.record().phase(), Phase::Committing);
        assert_eq!(
            reopened
                .commit_activation_transaction(&mut resumed)
                .expect("reconcile original commit"),
            target
        );
        if selected {
            assert_eq!(
                fs::read(store.root.join("active/sample-plugin.json"))
                    .expect("read reconciled selection"),
                before
            );
        }
        assert_eq!(resumed.record().phase(), Phase::Published);
        reopened
            .complete_activation_transaction(&mut resumed)
            .expect("finish recovered handoff");
    }
}

/// Strict corrupt journals cannot unblock competing mutations or echo document content, and no read repairs their bytes.
/// 严格损坏日志不能放行竞争变更或回显文档内容，且任何读取都不修复其字节。
#[test]
fn activation_transaction_rejects_corruption_and_changed_predecessors() {
    // All filesystem mutations are confined to this disposable fixture.
    // 全部文件系统变更限制在此可丢弃夹具中。
    let temp = tempfile::tempdir().expect("create corruption fixture");
    // Actual predecessor and target packages have separately staged credential identities.
    // 实际前驱及目标插件包具有分别暂存的凭据身份。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // The canonical request identity remains stable across discovery and retries.
    // 规范请求身份在发现及重试过程中保持稳定。
    let request_id = Uuid::new_v4();
    // Hold the real request owner until this test explicitly releases or advances it.
    // 持有实际请求所有者，直到测试明确释放或推进它。
    let transaction = begin(&store, &second, &credentials, request_id);
    // Preserve the valid original record while each corruption case changes one authority.
    // 保留有效原记录，每项损坏用例仅改变一项权威。
    let original = serde_json::to_value(transaction.record()).expect("retain valid record");
    // Persistent selection must remain byte-for-byte unchanged on rejected operations.
    // 拒绝操作时，持久选择必须逐字节保持不变。
    let original_activation =
        fs::read(store.root.join("active/sample-plugin.json")).expect("retain activation bytes");
    drop(transaction);
    for (field, replacement) in [
        ("schema_version", serde_json::json!(99)),
        ("request_id", serde_json::json!(Uuid::nil())),
        ("phase", serde_json::json!("private-invalid-phase")),
        ("target", serde_json::Value::Null),
        (
            "extra-private-property",
            serde_json::json!("private-invalid-value"),
        ),
    ] {
        // Each isolated malformed record must be rejected without automatic repair.
        // 每个隔离损坏记录都须被拒绝，不自动修复。
        let mut corrupt = original.clone();
        corrupt[field] = replacement;
        // Keep the exact malformed bytes to prove reads never silently rewrite them.
        // 保留精确损坏字节，证明读取绝不静默改写它们。
        let bytes = serde_json::to_vec(&corrupt).expect("encode corrupt journal");
        fs::write(record_path(temp.path(), request_id), &bytes).expect("write corrupt journal");
        // Inspect the classified failure without allowing private values into diagnostics.
        // 检查分类失败，不允许私密值进入诊断。
        let error = store
            .activation_transaction("sample-plugin", request_id)
            .expect_err("strict journal decoder rejects corruption");
        assert!(!error.message.contains("private-invalid"));
        assert_eq!(
            store
                .deactivate("sample-plugin", 1)
                .expect_err("corruption blocks direct selection writes")
                .code,
            Code::Invalid
        );
        assert_eq!(
            fs::read(record_path(temp.path(), request_id)).expect("read corrupt bytes"),
            bytes
        );
        assert_eq!(
            fs::read(store.root.join("active/sample-plugin.json"))
                .expect("read unchanged activation"),
            original_activation
        );
    }
    // Missing predecessor differs from explicit null and must fail strict decoding.
    // 缺失前驱不同于显式空值，须使严格解码失败。
    let mut missing_predecessor = original.clone();
    missing_predecessor
        .as_object_mut()
        .expect("record is an object")
        .remove("predecessor");
    fs::write(
        record_path(temp.path(), request_id),
        serde_json::to_vec(&missing_predecessor).expect("encode missing predecessor"),
    )
    .expect("persist missing predecessor");
    assert!(
        store
            .activation_transaction("sample-plugin", request_id)
            .is_err()
    );
    fs::write(
        record_path(temp.path(), request_id),
        serde_json::to_vec(&original).expect("restore original journal"),
    )
    .expect("persist original journal");
    // Equal revision alone cannot authorize a changed document or different selected package.
    // 仅相同修订不能授权已经改变的文档或不同选定插件包。
    let mut changed = serde_json::from_slice::<serde_json::Value>(&original_activation)
        .expect("decode actual predecessor");
    changed["generation"] =
        serde_json::to_value(second.identity()).expect("encode another physical identity");
    fs::write(
        store.root.join("active/sample-plugin.json"),
        serde_json::to_vec(&changed).expect("encode conflicting predecessor"),
    )
    .expect("write conflicting predecessor");
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request_id)
            .expect_err("complete predecessor comparison rejects conflict")
            .code,
        Code::Conflict
    );
    fs::write(
        store.root.join("active/sample-plugin.json"),
        original_activation,
    )
    .expect("restore actual predecessor");
    // Resume the original request from its durable checkpoint.
    // 从持久检查点恢复原请求。
    let mut resumed = store
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("resume restored request");
    store
        .abort_activation_transaction(&mut resumed)
        .expect("explicitly abort intact predecessor");
}

/// A missing index never grants write or collection authority; only explicit original prepared-request recovery may restore it.
/// 缺失索引绝不授予写入或回收权；仅明确恢复原准备请求才能重建它。
#[test]
fn activation_transaction_recovers_unindexed_preparation_without_guessing_a_request() {
    // The isolated fixture can reproduce the exact record-before-index publication window.
    // 隔离夹具可以重现记录已写入而索引尚未发布的精确窗口。
    let temp = tempfile::tempdir().expect("create unindexed fixture");
    // Actual predecessor and target packages have separately staged credential identities.
    // 实际前驱及目标插件包具有分别暂存的凭据身份。
    let (store, first, second, _, credentials) = fixture(temp.path());
    // The canonical request identity remains stable across discovery and retries.
    // 规范请求身份在发现及重试过程中保持稳定。
    let request_id = Uuid::new_v4();
    // Hold the real request owner until this test explicitly releases or advances it.
    // 持有实际请求所有者，直到测试明确释放或推进它。
    let transaction = begin(&store, &second, &credentials, request_id);
    // Use the exact physical package identity, including its inspected content fingerprint.
    // 使用精确物理插件包身份，包含已检查内容指纹。
    let identity = second.identity().clone();
    drop(transaction);
    drop(first);
    drop(second);
    // Retain the original body while removing only this fixture's index to simulate the interrupted boundary.
    // 保留原正文，仅移除此夹具索引以模拟被中断的边界。
    let index = store.root.join("transactions/sample-plugin/current");
    fs::remove_file(&index).expect("leave original unindexed preparation");
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("unindexed intent blocks writes")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("unindexed intent blocks package collection")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect_credentials("sample-plugin", &credentials, &EmptyCredentialBackend)
            .expect_err("unindexed intent blocks credential collection")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .activation_transaction_ids("sample-plugin")
            .expect("discover exact original ID"),
        [request_id].into_iter().collect()
    );
    assert!(!index.exists());
    // Explicit recovery selects the caller's known request and revalidates its exact predecessor and candidate.
    // 明确恢复选择调用方已知请求，并复核精确前驱及候选。
    let mut recovered = store
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("recover original preparation");
    assert!(index.is_file());
    store
        .abort_activation_transaction(&mut recovered)
        .expect("abandon original intent without publishing");
    drop(recovered);
    assert!(
        store
            .collect(&identity)
            .expect("aborted candidate can be reclaimed")
    );
    assert!(
        store
            .collect_credentials("sample-plugin", &credentials, &EmptyCredentialBackend)
            .expect("aborted staged credentials can be reclaimed")
    );
}

/// Actual Windows sharing violations prove recovery on both sides of the activation replacement, without simulated I/O errors.
/// 实际 Windows 共享冲突证明激活替换两侧的恢复，不使用模拟 I/O 错误。
#[cfg(windows)]
#[test]
fn activation_transaction_windows_recovers_both_atomic_write_failures() {
    use std::os::windows::fs::OpenOptionsExt;
    // Dedicated files isolate actual filesystem sharing violations from user data.
    // 专用文件将实际文件系统共享冲突与用户数据隔离。
    let temp = tempfile::tempdir().expect("create Windows commit fixture");
    // Actual predecessor and target packages have separately staged credential identities.
    // 实际前驱及目标插件包具有分别暂存的凭据身份。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // The canonical request identity remains stable across discovery and retries.
    // 规范请求身份在发现及重试过程中保持稳定。
    let request_id = Uuid::new_v4();
    // Hold the real request owner until this test explicitly releases or advances it.
    // 持有实际请求所有者，直到测试明确释放或推进它。
    let mut transaction = begin(&store, &second, &credentials, request_id);
    // This fixture's exact activation file is the sole publication authority.
    // 此夹具的精确激活文件是唯一发布权威。
    let activation_path = store.root.join("active/sample-plugin.json");
    // Preserve the original selected bytes before inducing a real sharing violation.
    // 引发实际共享冲突前保留原选定字节。
    let predecessor = fs::read(&activation_path).expect("retain exact predecessor bytes");
    // Reads and writes may share, but rename/delete cannot replace this actual open activation file.
    // 读取及写入可以共享，但重命名／删除不能替换此实际已打开激活文件。
    let activation_blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&activation_path)
        .expect("hold activation without delete sharing");
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("actual selection replacement fails")
            .code,
        Code::Storage
    );
    assert_eq!(transaction.record().phase(), Phase::Committing);
    assert_eq!(
        fs::read(&activation_path).expect("read original blocked activation"),
        predecessor
    );
    drop(activation_blocker);
    // The persisted committing checkpoint now lets the next attempt write selection before touching the journal again.
    // 已持久化的提交中检查点允许下一次尝试先写选择，再次触碰日志。
    let journal_path = record_path(temp.path(), request_id);
    // The actual file handle prevents checkpoint replacement while still permitting readback.
    // 实际文件句柄阻止检查点替换，同时仍允许回读。
    let journal_blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&journal_path)
        .expect("hold journal without delete sharing");
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("actual final checkpoint replacement fails")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("read successfully written target")
            .as_ref(),
        Some(transaction.record().target())
    );
    assert_eq!(
        store
            .activation_transaction("sample-plugin", request_id)
            .expect("read old checkpoint")
            .expect("original journal exists")
            .phase(),
        Phase::Committing
    );
    // Exact published bytes must survive reconciliation without a second replacement.
    // 精确已发布字节须在核对后保留，不发生再次替换。
    let target_bytes = fs::read(&activation_path).expect("retain published bytes before recovery");
    drop(journal_blocker);
    drop(transaction);
    // Recovery must acknowledge the already-published exact target without incrementing or replacing it a second time.
    // 恢复必须确认已发布精确目标，不递增或再次替换它。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen after failed acknowledgement")
        .expect("existing store");
    // Recovery owns the original request and must not replay candidate preparation.
    // 恢复持有原请求，不能重放候选准备。
    let mut recovered = reopened
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("resume original uncertain commit");
    assert_eq!(
        reopened
            .commit_activation_transaction(&mut recovered)
            .expect("reconcile actual uncertain publication")
            .revision,
        2
    );
    assert_eq!(
        fs::read(&activation_path).expect("read unchanged target bytes"),
        target_bytes
    );
    reopened
        .complete_activation_transaction(&mut recovered)
        .expect("confirm recovered route handoff");
}

/// Explicit cancellation needs no valid candidate, while historical receipts cannot repair an index over newer unfinished work.
/// 明确取消不需要有效候选，但历史回执不能越过更新的未完成工作修复索引。
#[test]
fn activation_transaction_can_abandon_invalid_candidate_without_hiding_newer_work() {
    // All filesystem mutations are confined to this disposable fixture.
    // 全部文件系统变更限制在此可丢弃夹具中。
    let temp = tempfile::tempdir().expect("create candidate abandonment fixture");
    // Actual predecessor and target packages have separately staged credential identities.
    // 实际前驱及目标插件包具有分别暂存的凭据身份。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // Keep an independently identifiable historical request for stale-receipt checks.
    // 保留可独立标识的历史请求，用于旧回执检查。
    let old_id = Uuid::new_v4();
    // The old owner must be released before cancellation through another entry point.
    // 通过另一入口取消之前必须释放旧所有者。
    let mut old = begin(&store, &second, &credentials, old_id);
    assert_eq!(
        store
            .abort_activation_request("sample-plugin", old_id)
            .expect_err("a live owner cannot be bypassed by cancellation")
            .code,
        Code::Busy
    );
    store
        .abort_activation_transaction(&mut old)
        .expect("retain old aborted receipt");
    drop(old);
    // A later request has its own identity and cannot overwrite historical receipts.
    // 后续请求具有自身身份，不能覆盖历史回执。
    let next_id = Uuid::new_v4();
    // The later request exercises ownership independently of the historical receipt.
    // 后续请求独立于历史回执验证所有权。
    let next = begin(&store, &second, &credentials, next_id);
    drop(next);
    // Only the isolated fixture's index is removed to reproduce interrupted publication.
    // 仅移除隔离夹具索引以重现中断发布。
    let index = store.root.join("transactions/sample-plugin/current");
    fs::remove_file(&index).expect("simulate lost newer index");
    assert_eq!(
        store
            .abort_activation_request("sample-plugin", old_id)
            .expect_err("old terminal request cannot replace a newer unfinished owner")
            .code,
        Code::Busy
    );
    assert!(!index.exists());
    // The later request exercises ownership independently of the historical receipt.
    // 后续请求独立于历史回执验证所有权。
    let next = store
        .resume_activation_transaction("sample-plugin", next_id)
        .expect("explicit original newer request recovers its index");
    drop(next);
    // Mutating an isolated package proves cancellation does not require successfully recreating candidate execution.
    // 修改隔离插件包证明取消不要求成功重建候选执行。
    let source = second.package_root().join("skills/review-helper/SKILL.md");
    // Restore the candidate's original bytes after proving cancellation needs no valid package.
    // 证明取消不需要有效插件包后，恢复候选原字节。
    let original_bytes = fs::read(&source).expect("retain candidate bytes");
    fs::write(&source, b"changed candidate")
        .expect("make candidate fail exact fingerprint validation");
    assert!(
        store
            .resume_activation_transaction("sample-plugin", next_id)
            .is_err()
    );
    // The durable abort receipt must remain queryable and idempotent.
    // 持久中止回执必须保持可查询及幂等。
    let aborted = store
        .abort_activation_request("sample-plugin", next_id)
        .expect("abandon invalid unpublished candidate");
    assert_eq!(aborted.phase(), Phase::Aborted);
    assert_eq!(
        store
            .abort_activation_request("sample-plugin", next_id)
            .expect("query original abort idempotently"),
        aborted
    );
    fs::write(source, original_bytes).expect("restore fixture bytes for normal reclamation");
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("read untouched predecessor")
            .expect("original selection")
            .revision,
        1
    );
}

//! Real journals and fresh managers verify startup admission and explicit original-request recovery.
//! 真实日志及全新管理器验证启动入场和明确原请求恢复。

use super::*;
use crate::{VulcanPluginRuntimeManager, VulcanPluginRuntimeState as State};
use std::collections::BTreeSet;

/// Create the host-owned user directory under `root`; return the literal store parent.
/// 在 `root` 下创建宿主自有用户目录；返回字面存储父目录。
fn prepare_user_root(root: &Path) -> PathBuf {
    // Production bootstrap creates this parent before opening managed generation storage.
    // 生产启动在打开受管代次存储前创建此父目录。
    let user = root.join("user");
    fs::create_dir_all(&user).expect("host creates user root");
    user
}

/// Build a new manager for `root` with a healthy bundled peer; return the uninitialized manager.
/// 为 `root` 构建含健康内建同伴的新管理器；返回未初始化管理器。
fn manager(root: &Path) -> VulcanPluginRuntimeManager {
    write_package(&root.join("bundled"), "healthy-plugin", "healthy");
    VulcanPluginRuntimeManager::without_runtime(
        root.join("bundled"),
        root.join("user"),
        root.join("bridge"),
        root.join("distributions"),
        root.join("state"),
        root.join("runtime"),
    )
}

/// Inspect `runtime` with unchanged default-enabled policy; return its sample state after checking peer admission.
/// 按未变的默认启用策略检查 `runtime`；检查同伴入场后返回样例状态。
fn sample_state(runtime: &VulcanPluginRuntimeManager) -> State {
    // A full publication exercises content surfaces as well as the management descriptor.
    // 完整发布同时验证内容暴露面及管理描述符。
    let snapshot = runtime
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh");
    assert_eq!(
        snapshot.plugins["healthy-plugin"].runtime_state,
        State::Enabled
    );
    assert!(
        snapshot.plugins["sample-plugin"].desired_enabled,
        "recovery must preserve user policy"
    );
    snapshot.plugins["sample-plugin"].runtime_state
}

/// Fresh managers gate every unfinished phase and permit only explicit completion or abort of the original.
/// 全新管理器门控每个未完成阶段，仅允许明确完成或中止原请求后恢复。
#[test]
fn activation_recovery_restart_preserves_originals_and_unrelated_plugins() {
    for published in [false, true] {
        // Each phase uses independently persisted package and credential generations.
        // 每个阶段使用独立持久化的插件包及凭据代次。
        let temp = tempfile::tempdir().expect("recovery fixture");
        // Real store operations produce the original predecessor and target.
        // 实际存储操作产生原前驱及目标。
        let (store, _first, second, _old_credentials, credentials) =
            fixture(&prepare_user_root(temp.path()));
        // The same original request survives releasing its live exclusive owner.
        // 释放活动排他所有者后，相同原请求继续存续。
        let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
        if published {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("publish");
        }
        // Preserve identity without accidentally retaining exclusive ownership.
        // 保留身份，不意外保留排他所有权。
        let request_id = transaction.record().request_id();
        drop(transaction);
        // A fresh manager recreates all process-local state from actual disk.
        // 全新管理器从实际磁盘重新创建全部进程内状态。
        let runtime = manager(temp.path());
        runtime
            .initialize_activation_recovery()
            .expect("initialize");
        assert_eq!(sample_state(&runtime), State::Unavailable);
        assert!(runtime.begin_enabling("sample-plugin").is_err());
        assert!(
            runtime
                .snapshot()
                .enabled_skills
                .values()
                .all(|skill| skill.plugin_id() != Some("sample-plugin"))
        );
        // Explicit resumption reacquires the exact original owner, never a new request.
        // 明确恢复重新取得精确原所有者，绝不创建新请求。
        let mut resumed = store
            .resume_activation_transaction("sample-plugin", request_id)
            .expect("resume");
        if published {
            store
                .complete_activation_transaction(&mut resumed)
                .expect("explicit route acknowledgement");
        } else {
            store
                .abort_activation_transaction(&mut resumed)
                .expect("explicit abort");
        }
        assert_eq!(sample_state(&runtime), State::Enabled);
        assert!(
            runtime
                .snapshot()
                .enabled_skills
                .values()
                .any(|skill| skill.plugin_id() == Some("sample-plugin"))
        );
        // Terminal history alone never gates a subsequent process startup.
        // 仅终态历史绝不门控后续进程启动。
        assert_eq!(sample_state(&manager(temp.path())), State::Enabled);
    }
}

/// Both sides of an interrupted committing checkpoint stay gated until exact original readback is resolved.
/// 提交中检查点被中断的两侧都保持门控，直到精确原请求回读得到解决。
#[test]
fn activation_recovery_committing_checkpoint_requires_explicit_readback() {
    for target_selected in [false, true] {
        // Each case models one side of the existing atomic selection publication boundary.
        // 每个场景模拟既有原子选择发布边界的一侧。
        let temp = tempfile::tempdir().expect("recovery fixture");
        // Complete authority comes from the production store, not a fabricated package descriptor.
        // 完整权威来自生产存储，而非伪造插件包描述符。
        let (store, _first, second, _old_credentials, credentials) =
            fixture(&prepare_user_root(temp.path()));
        // Original identity survives the injected checkpoint interruption.
        // 原始身份在注入的检查点中断后继续存续。
        let request_id = Uuid::new_v4();
        // Retain a genuine record with exact predecessor and target identities.
        // 保留具有精确前驱及目标身份的真实记录。
        let mut transaction = begin(&store, &second, &credentials, request_id);
        if target_selected {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("select target");
        }
        // Fault injection changes only the declared durable checkpoint, preserving both complete selections.
        // 故障注入仅更改声明的持久检查点，保留两份完整选择。
        let mut interrupted = serde_json::to_value(transaction.record()).expect("original record");
        interrupted["phase"] = serde_json::to_value(Phase::Committing).expect("declared phase");
        drop(transaction);
        fs::write(
            record_path(&temp.path().join("user"), request_id),
            serde_json::to_vec(&interrupted).expect("checkpoint bytes"),
        )
        .expect("interrupt checkpoint");
        // Startup sees an unresolved request regardless of which selection actually reached disk.
        // 无论哪个选择实际到达磁盘，启动都看到未解决请求。
        let runtime = manager(temp.path());
        assert_eq!(sample_state(&runtime), State::Unavailable);
        // The explicit original owner uses exact current selection readback to resolve its next step.
        // 明确原所有者按精确当前选择回读确定下一步。
        let mut resumed = store
            .resume_activation_transaction("sample-plugin", request_id)
            .expect("resume committing original");
        if target_selected {
            store
                .commit_activation_transaction(&mut resumed)
                .expect("confirm selected target");
            store
                .complete_activation_transaction(&mut resumed)
                .expect("acknowledge route handoff");
        } else {
            store
                .abort_activation_transaction(&mut resumed)
                .expect("abort unchanged predecessor");
        }
        assert_eq!(sample_state(&runtime), State::Enabled);
    }
}

/// Disabled handoff permits are exact-owner-only and cannot authorize enable before durable completion.
/// 停用交接许可仅属于精确所有者，不能在持久完成之前授权启用。
#[test]
fn activation_recovery_disabled_publication_keeps_original_gate_until_completion() {
    // Real storage starts with one selected generation and one independently prepared target.
    // 真实存储开始时具有一个已选代次及一个独立准备目标。
    let temp = tempfile::tempdir().expect("recovery fixture");
    // Preserve the genuine transaction authority rather than fabricating a permit.
    // 保留真实事务权威，而非伪造许可。
    let (store, _first, second, _old_credentials, credentials) =
        fixture(&prepare_user_root(temp.path()));
    // Publication alone deliberately leaves the original route obligation pending.
    // 仅发布故意保留原路由义务为待完成。
    let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    store
        .commit_activation_transaction(&mut transaction)
        .expect("publish");
    // This manager first sees the already-published unfinished request at startup.
    // 此管理器在启动时首次看到已发布但未完成的请求。
    let runtime = manager(temp.path());
    assert_eq!(sample_state(&runtime), State::Unavailable);
    assert!(
        runtime
            .refresh_catalog_for_activation_recovery(
                &transaction,
                &BTreeSet::new(),
                &BTreeSet::new()
            )
            .is_err()
    );
    // Explicit policy is the only allowed disabled recovery projection.
    // 明确策略是唯一允许的停用恢复投影。
    let disabled = BTreeSet::from(["sample-plugin".to_owned()]);
    // The complete target matches the durable original and retains its package and credentials.
    // 完整目标匹配持久原请求，并保留其插件包及凭据。
    let catalog = runtime
        .refresh_catalog_for_activation_recovery(&transaction, &disabled, &BTreeSet::new())
        .expect("disabled handoff");
    assert_eq!(
        catalog.plugins["sample-plugin"].runtime_state,
        State::Disabled
    );
    assert!(
        catalog.plugins["sample-plugin"]
            .managed_package()
            .expect("managed target")
            .matches_activation(transaction.record().target())
    );
    assert!(
        runtime
            .begin_enabling("sample-plugin")
            .expect_err("pending original still gates effects")
            .to_string()
            .contains("recovery")
    );
    assert_eq!(
        sample_state(&runtime),
        State::Unavailable,
        "permit must not persist across refresh"
    );
    store
        .complete_activation_transaction(&mut transaction)
        .expect("complete original");
    assert_eq!(sample_state(&runtime), State::Enabled);
}

/// Unindexed preparation remains discoverable and can be repaired only by explicit original ownership.
/// 未索引准备仍可被发现，仅能由明确原所有权修复。
#[test]
fn activation_recovery_unindexed_preparation_requires_original_resume() {
    // Actual records permit simulating the precise crash boundary before index publication.
    // 实际记录允许模拟索引发布前的精确崩溃边界。
    let temp = tempfile::tempdir().expect("recovery fixture");
    // The store has complete pinned predecessor and target authority.
    // 存储具有完整固定前驱及目标权威。
    let (store, _first, second, _old_credentials, credentials) =
        fixture(&prepare_user_root(temp.path()));
    // Save the original UUID before releasing exclusive ownership.
    // 释放排他所有权前保存原 UUID。
    let request_id = Uuid::new_v4();
    drop(begin(&store, &second, &credentials, request_id));
    fs::remove_file(record_path(&temp.path().join("user"), request_id).with_file_name("current"))
        .expect("interrupt index publication");
    // Startup discovery must not synthesize the missing index.
    // 启动发现不能合成缺失索引。
    let runtime = manager(temp.path());
    assert_eq!(sample_state(&runtime), State::Unavailable);
    assert!(
        !record_path(&temp.path().join("user"), request_id)
            .with_file_name("current")
            .exists()
    );
    // Explicit original resumption owns the repair and later terminal acknowledgement.
    // 明确原请求恢复拥有修复及后续终态确认。
    let mut transaction = store
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("explicit original index repair");
    store
        .abort_activation_transaction(&mut transaction)
        .expect("abort unchanged predecessor");
    assert_eq!(sample_state(&runtime), State::Enabled);
}

/// Corrupt indexes, missing originals and orphan receipts preserve a visible gate across refresh attempts.
/// 损坏索引、缺失原请求及孤立回执在反复刷新后保留可见门控。
#[test]
fn activation_recovery_corruption_never_becomes_successful_empty_inventory() {
    for corruption in ["index", "record", "receipt", "terminal-index"] {
        // Each mutation targets only this isolated journal's exact fixture path.
        // 每次变更仅针对隔离日志的精确夹具路径。
        let temp = tempfile::tempdir().expect("recovery fixture");
        // Authentic targets keep catalog validation independent from journal corruption.
        // 真实目标使目录校验独立于日志损坏。
        let (store, _first, second, _old_credentials, credentials) =
            fixture(&prepare_user_root(temp.path()));
        // Terminal and pending histories use the same genuine request mechanism.
        // 终态及待处理历史使用相同真实请求机制。
        let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
        // Resolve the exact journal path without choosing a latest request.
        // 解析精确日志路径，不选择最新请求。
        let original = record_path(&temp.path().join("user"), transaction.record().request_id());
        if matches!(corruption, "terminal-index" | "receipt") {
            store
                .abort_activation_transaction(&mut transaction)
                .expect("terminal history");
        }
        drop(transaction);
        // Observe the valid original first when testing deletion after startup.
        // 测试启动后删除时，先观察有效原请求。
        let runtime = manager(temp.path());
        if corruption == "record" {
            runtime
                .initialize_activation_recovery()
                .expect("capture original");
        }
        match corruption {
            "index" => fs::write(original.with_file_name("current"), b"{").expect("corrupt index"),
            "record" => {
                fs::remove_file(&original).expect("remove known original");
                fs::remove_file(original.with_file_name("current")).expect("remove original index");
            }
            "receipt" => fs::write(
                original.with_file_name(format!("{}.install", Uuid::new_v4())),
                b"{}",
            )
            .expect("orphan receipt"),
            "terminal-index" => {
                fs::remove_file(original.with_file_name("current")).expect("missing terminal index")
            }
            _ => unreachable!("enumerated fixture cases"),
        }
        assert_eq!(
            sample_state(&runtime),
            State::Unavailable,
            "corruption: {corruption}"
        );
        runtime
            .initialize_activation_recovery()
            .expect("cannot reset captured obligation");
        assert_eq!(
            sample_state(&runtime),
            State::Unavailable,
            "repeat: {corruption}"
        );
        assert!(runtime.begin_enabling("sample-plugin").is_err());
    }
}

/// Missing optional storage stays absent, while unreadable global inventories gate all plugin execution.
/// 缺失可选存储保持缺失，不可读全局清单则门控全部插件执行。
#[test]
fn activation_recovery_absence_is_read_only_and_root_errors_are_fail_closed() {
    // No generation directory should be created by startup inventory or ordinary catalog reads.
    // 启动清单或普通目录读取均不应创建代次目录。
    let temp = tempfile::tempdir().expect("recovery fixture");
    // The first manager sees an ordinary content plugin without managed storage.
    // 首个管理器看到无受管存储的普通内容插件。
    let runtime = manager(temp.path());
    runtime
        .initialize_activation_recovery()
        .expect("optional absence");
    assert_eq!(
        runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("catalog")
            .plugins["healthy-plugin"]
            .runtime_state,
        State::Enabled
    );
    assert!(!temp.path().join("user").join(STORE_DIRECTORY).exists());
    // A new process must distinguish corrupted managed storage from optional absence.
    // 新进程必须区分损坏受管存储与可选缺失。
    let store = VulcanPluginGenerationStore::open(&prepare_user_root(temp.path())).expect("store");
    fs::write(store.root.join("transactions"), b"invalid directory type")
        .expect("malformed inventory");
    // Fresh admission is frozen from the unreadable global namespace inventory.
    // 全新入场按不可读全局命名空间清单冻结。
    let restarted = manager(temp.path());
    restarted
        .initialize_activation_recovery()
        .expect("management startup remains available");
    // Management retains a descriptor and diagnostic while business surfaces remain empty.
    // 管理保留描述符及诊断，业务暴露面保持为空。
    let snapshot = restarted
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("management catalog");
    assert_eq!(
        snapshot.plugins["healthy-plugin"].runtime_state,
        State::Unavailable
    );
    assert!(snapshot.enabled_skills.is_empty());
    assert!(
        snapshot.plugins["healthy-plugin"]
            .diagnostics
            .iter()
            .any(|message| message.contains("recovery inventory"))
    );
}

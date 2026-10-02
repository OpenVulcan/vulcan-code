//! Old restoration is checked against real callbacks, durable receipts and physical native ownership.
//! 使用真实回调、持久回执及物理原生所有权检查旧恢复。

use super::callbacks::{callback_fixture, callback_fixture_with_target};
use super::*;

/// An absent old restore declaration needs no attached formal parent and creates no execution evidence.
/// 缺失旧恢复声明无需附加正式父级，也不创建执行证据。
#[test]
fn formal_predecessor_restore_absent_declaration_needs_no_formal_parent() {
    // Both old lifecycle declarations are absent under the manifest's existing paired declaration contract.
    // 按清单既有成对声明契约，两个旧生命周期声明均缺失。
    let (fixture, store, transaction, drain) = callback_fixture(false, false, false);
    executor().block_on(async {
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("close real unused parent");
        tokio::time::timeout(WAIT, async {
            while !fixture
                .manager
                .poll_formal_runtime_closed()
                .expect("actual closure")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual parent retired");
        // Detach only after the real public closure API proves complete native retirement.
        // 仅在真实公开关闭 API 证明原生完全退役后移除注册。
        drop(
            fixture
                .manager
                .state
                .formal_runtime
                .lock()
                .expect("parent slot")
                .take(),
        );
        assert!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("no parent")
                .is_none()
        );
        // The same original view still validates package and request authority without constructing a new executor.
        // 相同原视图仍校验插件包及请求权威，不构造新执行器。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("unchanged original");
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("explicit no old callback");
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("explicit no old disable");
    });
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("no execution")
            .is_empty()
    );
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("evidence")
            .expect("receipt")
            .observations(),
        vec![(Effect::DrainPredecessorLua, Observation::Succeeded)]
    );
}

use crate::runtime::candidates::PredecessorOperation;
use crate::{VulcanPluginActivationEffect as Effect, VulcanPluginEffectObservation as Observation};

/// Successful old restoration excludes target publication even when every recorded phase succeeded.
/// 即使每个记录阶段均成功，旧恢复成功仍排除目标发布。
#[test]
fn formal_predecessor_restore_success_permanently_excludes_original_target() {
    // The old and target packages are distinct and remain immutable.
    // 旧包及目标包不同，并保持不可变。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, false);
    // Retain the exact original target until all denial assertions complete.
    // 在全部拒绝断言完成前保留精确原目标。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    executor().block_on(async {
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("old disable");
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("old restore");
        assert!(
            store
                .activation_effects("hook-plugin", drain.request_id())
                .expect("evidence")
                .expect("receipt")
                .observations()
                .iter()
                .all(|(_, observation)| *observation == Observation::Succeeded)
        );
        assert!(
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("restore excludes new probe")
                .message
                .contains("restore was attempted")
        );
        assert!(
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .enable_activation_candidate(&candidate)
                .await
                .is_err()
        );
    });
    drop(candidate);
    assert!(
        store
            .commit_activation_transaction(&mut transaction)
            .is_err()
    );
    assert_eq!(
        store
            .activation("hook-plugin")
            .expect("old selected")
            .as_ref(),
        transaction.record().predecessor()
    );
}

/// Strict restoration evidence rejects null, impossible ordering and unknown installation without repairing bytes.
/// 严格恢复证据拒绝空值、不可能顺序及未知安装，不修复字节。
#[test]
fn formal_predecessor_restore_invalid_and_cross_sidecar_evidence_is_rejected() {
    // Real old callbacks establish the valid evidence before corruption.
    // 真实旧回调在损坏前建立有效证据。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, false);
    executor().block_on(async {
        // One real original owns both old callbacks.
        // 单个真实原请求拥有两个旧回调。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("original");
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("disable");
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("restore");
    });
    // Exact declared journal paths stay inside the actual fixture root.
    // 精确声明日志路径留在实际夹具根内。
    let path = fixture
        .manager
        .user_plugin_root()
        .join(".vulcan-generations/transactions/hook-plugin")
        .join(format!("{}.effects", drain.request_id()));
    // Preserve the real receipt before independent invalid mutations.
    // 在独立无效变更前保留真实回执。
    let original = std::fs::read(&path).expect("real receipt");
    // Parse only generated fixture data without emitting raw callback or credential content.
    // 仅解析生成夹具数据，不输出原始回调或凭据内容。
    let record: Value = serde_json::from_slice(&original).expect("JSON");
    for (field, value) in [
        ("predecessor_restore", json!(null)),
        ("predecessor_restore", json!("unknown")),
        ("predecessor_disable", json!("started")),
        ("predecessor_lua_drain", json!("observation_failed")),
        ("probe", json!("started")),
        ("enable", json!("succeeded")),
        ("enable_rollback", json!("started")),
        ("predecessor_drain_identity", json!(null)),
    ] {
        // Change only one declared field per rejection.
        // 每次拒绝仅改变一个声明字段。
        let mut changed = record.clone();
        changed[field] = value;
        // Byte preservation proves fail-closed inspection rather than normalization.
        // 字节保留证明失败关闭式检查，而非规范化。
        let encoded = serde_json::to_vec(&changed).expect("encode");
        std::fs::write(&path, &encoded).expect("inject fixture corruption");
        assert!(
            store
                .activation_effects("hook-plugin", drain.request_id())
                .is_err()
        );
        assert!(store.activation_recovery_records("hook-plugin").is_err());
        assert!(
            store
                .commit_activation_transaction(&mut transaction)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).expect("unchanged bytes"), encoded);
    }
    // A duplicate known field cannot hide earlier unconfirmed restore intent.
    // 重复已知字段不能隐藏先前未确认恢复意图。
    let duplicate = String::from_utf8(original.clone()).expect("UTF8").replacen(
        "{",
        "{\"predecessor_restore\":\"started\",",
        1,
    );
    std::fs::write(&path, &duplicate).expect("duplicate fixture");
    assert!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .is_err()
    );
    assert_eq!(
        std::fs::read(&path).expect("preserved duplicate"),
        duplicate.as_bytes()
    );
    std::fs::write(&path, &original).expect("restore fixture-only corruption");
    assert!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .is_ok()
    );

    // A separate real request stops after durable installation intent, before Lua execution.
    // 独立真实请求在持久安装意图后、Lua 执行前停止。
    let (pending_fixture, pending_store, pending_transaction, pending_drain) =
        callback_fixture(false, true, false);
    // The original candidate owns the actual unfinished installation receipt.
    // 原候选拥有实际未完成安装回执。
    let pending_candidate = pending_store
        .inspect_activation_candidate(&pending_transaction)
        .expect("pending original");
    executor().block_on(async {
        pending_fixture
            .manager
            .disable_activation_predecessor(&pending_candidate, &pending_drain)
            .await
            .expect("old disabled");
        // Dropping only the observer leaves Started and never claims that a callback ran or completed.
        // 仅销毁观察者保留 Started，绝不宣称回调已经运行或完成。
        let installation = pending_candidate
            .begin_installation_attempt()
            .expect("real install intent");
        drop(installation);
        assert!(
            pending_fixture
                .manager
                .restore_activation_predecessor(&pending_candidate, &pending_drain)
                .await
                .expect_err("unobserved installation blocks restoration")
                .message
                .contains("unobserved installation")
        );
    });
    // Observe exact cross-sidecar evidence through the same production journal reader.
    // 经相同生产日志读取器观测精确跨附属文件证据。
    let pending_path = pending_fixture
        .manager
        .user_plugin_root()
        .join(".vulcan-generations/transactions/hook-plugin")
        .join(format!("{}.effects", pending_drain.request_id()));
    // Invalid coexistence is injected only into temporary test bytes.
    // 仅向临时测试字节注入无效共存关系。
    let mut pending_record: Value =
        serde_json::from_slice(&std::fs::read(&pending_path).expect("pending effects"))
            .expect("JSON");
    pending_record["predecessor_restore"] = json!("started");
    std::fs::write(
        &pending_path,
        serde_json::to_vec(&pending_record).expect("encode"),
    )
    .expect("impossible cross-sidecar ordering");
    assert!(
        pending_store
            .activation_effects("hook-plugin", pending_drain.request_id())
            .is_err()
    );
    assert!(
        pending_store
            .activation_recovery_records("hook-plugin")
            .is_err()
    );
}

/// Target callbacks prove the selected target configuration and emit effects distinct from old callbacks.
/// 目标回调证明所选目标配置，并发出与旧回调不同的副作用。
const VALID_TARGET: &str = r#"
-- Verify target configuration before its initialization completes.
-- 在目标初始化完成前验证目标配置。
assert(vulcan.host.call('vulcan.plugin.config.get', {}).label == 'new-selected')
-- One module owns one explicit lifecycle dispatch.
-- 单个模块拥有单个显式生命周期分发。
local M = {}
-- args contains the original event and request; return one successful frame after a real native event.
-- args 包含原事件及请求；在真实原生事件后返回单个成功帧。
function M.dispatch(args)
    assert(type(args.request_id) == 'string' and #args.request_id == 36)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='target-' .. args.event,revision=0})
    return '{"ok":true,"result":{}}'
end
M.shutdown, M.shutdown_hooks, M.on_precheck, M.ui_action, M.on_lifecycle = M.dispatch, M.dispatch, M.dispatch, M.dispatch, M.dispatch
return M
"#;

/// An observed old disable failure may restore the exact old configuration once, without admitting the target.
/// 已观测旧停用失败可单次恢复精确旧配置，不接纳目标。
#[test]
fn formal_predecessor_restore_preserves_old_authority_and_failed_evidence() {
    // Fail only the old disable, after its real native event.
    // 仅在真实原生事件后使旧停用失败。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, true);
    // Keep the original target private throughout restoration.
    // 在整个恢复期间保持原目标私有。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original candidate");
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
            .expect("observed old failure restores exact old callback");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("restore cannot replay")
                .message
                .contains("already attempted")
        );
        assert!(
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .enable_activation_candidate(&candidate)
                .await
                .is_err()
        );
    });
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("evidence")
            .expect("receipt")
            .observations(),
        vec![
            (Effect::DrainPredecessorLua, Observation::Succeeded),
            (Effect::DisablePredecessor, Observation::ObservationFailed),
            (Effect::RestorePredecessor, Observation::Succeeded)
        ]
    );
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("old effects")
            .iter()
            .filter(|event| event.topic == "enable")
            .count(),
        1
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Draining
    );
    drop(candidate);
    assert!(
        store
            .commit_activation_transaction(&mut transaction)
            .is_err()
    );
    assert_eq!(
        store
            .activation("hook-plugin")
            .expect("old still selected")
            .as_ref(),
        transaction.record().predecessor()
    );
}

/// Logical history removal cannot allow old restoration while its actual native disable still runs.
/// 实际原生停用仍运行时，逻辑历史移除不能允许旧恢复。
#[test]
fn formal_predecessor_restore_checks_actual_domains_before_consuming_attempt() {
    // The actual native gate survives a finite disable observer.
    // 实际原生门比有限停用观察者存续更久。
    let (fixture, store, transaction, drain) = callback_fixture(true, true, false);
    // Preserve exact ownership independently from the deliberately removed logical call.
    // 独立于刻意移除的逻辑调用保留精确所有权。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .invoke_formal_predecessor_disable(&candidate, &drain, Instant::now() + WAIT / 10)
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("tracked old call is alive")
                .message
                .contains("actual retirement")
        );
        // Observe only this request's actual authority; no counter or surrogate busy flag is introduced.
        // 仅观测此请求实际权威；不引入计数器或替代繁忙标记。
        let (call_id, retained) = {
            // The actual parent owns the original retained call.
            // 实际父级拥有原保留调用。
            let owner = fixture.owner();
            // Freeze the exact call before deleting only its logical registry entry.
            // 仅删除逻辑注册表条目前固定精确调用。
            let calls = owner.calls.lock().expect("registry");
            // The original typed authority is the unique source of request identity.
            // 原类型化权威是请求身份的唯一来源。
            let call = calls
                .values()
                .find(|call| {
                    call.source
                        .activation()
                        .is_some_and(|authority| authority.request_id == drain.request_id())
                })
                .expect("retained old call");
            (
                call.id.clone(),
                Arc::downgrade(call.source.activation().expect("actual original authority")),
            )
        };
        fixture
            .owner()
            .calls
            .lock()
            .expect("registry")
            .remove(&call_id);
        assert!(
            fixture
                .manager
                .activation_predecessor_calls(drain.request_id())
                .expect("logical removal")
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("actual native domain is still alive")
                .message
                .contains("actual retirement")
        );
        assert!(
            !store
                .activation_effects("hook-plugin", drain.request_id())
                .expect("receipt")
                .expect("evidence")
                .observations()
                .iter()
                .any(|(effect, _)| *effect == Effect::RestorePredecessor)
        );
        fixture.events.release();
        authority_retired(&retained).await;
        fixture.empty().await;
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("old native work actually retired");
        assert!(
            store
                .activation_effects("hook-plugin", drain.request_id())
                .expect("evidence")
                .expect("receipt")
                .observations()
                .contains(&(Effect::RestorePredecessor, Observation::Succeeded))
        );
    });
}

/// A timed-out old restore keeps its original OS lock through actual native completion.
/// 超时旧恢复在实际原生完成前保持原系统锁。
#[test]
fn formal_predecessor_restore_timeout_retains_original_request() {
    // Only the old enable event blocks; no old disable or target callback is executed first.
    // 仅旧启用事件阻塞；此前不执行旧停用或目标回调。
    let (fixture, store, transaction, drain) = callback_fixture(false, true, false);
    executor().block_on(async {
        // The original view grants only typed old restoration.
        // 原视图仅授予类型化旧恢复。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        assert!(
            fixture
                .manager
                .invoke_formal_predecessor_operation(
                    &candidate,
                    &drain,
                    PredecessorOperation::Restore,
                    Instant::now() + WAIT / 10
                )
                .await
                .expect_err("restore deadline")
                .message
                .contains("timed out")
        );
        assert!(fixture.events.entered.load(Ordering::Acquire));
        assert_eq!(
            fixture
                .manager
                .activation_predecessor_calls(drain.request_id())
                .expect("real old restore")
                .len(),
            1
        );
        assert!(
            fixture
                .manager
                .activation_candidate_calls(drain.request_id())
                .expect("candidate excludes old restore")
                .is_empty()
        );
        assert_eq!(
            fixture
                .manager
                .drain_activation_candidate_calls(drain.request_id())
                .await
                .expect("candidate-only scope")
                .closed_formal_calls,
            0
        );
        assert!(
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .is_err()
        );
        // Retain the original identity after dropping every foreground transaction owner.
        // 销毁全部前台事务所有者后保留原身份。
        let request_id = drain.request_id();
        drop(candidate);
        drop(transaction);
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("real native ownership")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("evidence")
                .expect("receipt")
                .observations()
                .contains(&(Effect::RestorePredecessor, Observation::ObservationFailed))
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_predecessor_calls(request_id)
            .await
            .expect("physical old restore retirement");
        fixture.empty().await;
        // Reopening the original changes ownership, never the prior observation.
        // 重开原请求改变所有权，绝不改变先前观测。
        let resumed = store
            .resume_activation_transaction("hook-plugin", request_id)
            .expect("real lock retired");
        // The exact borrowed original remains permanently excluded from target work.
        // 精确借用原请求保持永久排除目标工作。
        let candidate = store
            .inspect_activation_candidate(&resumed)
            .expect("same original");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("failed restore cannot replay")
                .message
                .contains("already attempted")
        );
    });
}

/// Restoring old code after target enable requires real successful target compensation and remains noncommittable.
/// 目标启用后恢复旧代码要求真实成功目标补偿，且保持不可提交。
#[test]
fn formal_predecessor_restore_requires_target_enable_compensation() {
    // The two real packages and configurations emit distinct native events.
    // 两个真实插件包及配置发出不同原生事件。
    let (fixture, store, mut transaction, drain) =
        callback_fixture_with_target(false, true, false, VALID_TARGET);
    // All phases borrow the same original prepared target.
    // 全部阶段借用相同原准备目标。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original");
    executor().block_on(async {
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("old disabled");
        fixture
            .manager
            .probe_activation_candidate(&candidate)
            .await
            .expect("new initialization");
        fixture
            .manager
            .install_activation_candidate(&candidate)
            .await
            .expect("new install");
        fixture
            .manager
            .enable_activation_candidate(&candidate)
            .await
            .expect("new enable");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("target enable must be compensated first")
                .message
                .contains("confirmed target enable compensation")
        );
        assert!(
            !store
                .activation_effects("hook-plugin", drain.request_id())
                .expect("no restore intent")
                .expect("receipt")
                .observations()
                .iter()
                .any(|(effect, _)| *effect == Effect::RestorePredecessor)
        );
        fixture
            .manager
            .rollback_activation_candidate_enable(&candidate)
            .await
            .expect("real new disable compensation");
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("old enable after target compensation");
    });
    // Event membership proves each real business export, without binding assertions to evolving positions.
    // 事件成员关系证明每个真实业务导出，不将断言绑定至会变化的位置。
    let topics = fixture
        .events
        .values
        .lock()
        .expect("real effects")
        .iter()
        .map(|event| event.topic.clone())
        .collect::<Vec<_>>();
    for expected in [
        "target-install",
        "target-enable",
        "target-enable_rollback",
        "disable",
        "enable",
    ] {
        assert_eq!(
            topics
                .iter()
                .filter(|topic| topic.as_str() == expected)
                .count(),
            1,
            "expected real callback topic {expected}"
        );
    }
    assert!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("all evidence")
            .expect("receipt")
            .observations()
            .contains(&(Effect::EnableRollback, Observation::Succeeded))
    );
    assert!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("all evidence")
            .expect("receipt")
            .observations()
            .contains(&(Effect::RestorePredecessor, Observation::Succeeded))
    );
    drop(candidate);
    assert!(
        store
            .commit_activation_transaction(&mut transaction)
            .is_err()
    );
}

/// Cancelling a restore observer preserves Started and actual ownership without authorizing replay.
/// 取消恢复观察者保留 Started 及实际所有权，不授予重放。
#[test]
fn formal_predecessor_restore_cancelled_observer_keeps_unknown_evidence() {
    // The actual native enable callback blocks independently of its observer.
    // 实际原生启用回调独立于观察者阻塞。
    let (fixture, store, transaction, drain) = callback_fixture(false, true, false);
    // The completed drain is readable without pinning the original request lock.
    // 已完成排空可读，但不固定原请求锁。
    let observing_drain = drain.clone();
    // Preserve the exact original request outside the cancelled owner.
    // 在被取消所有者之外保留精确原请求。
    let request_id = drain.request_id();
    executor().block_on(async {
        // Reuse the existing manager, never allocate a replacement executor.
        // 复用既有管理器，绝不分配替代执行器。
        let manager = fixture.manager.clone();
        // The store clone does not own a second request lock.
        // 存储克隆不拥有第二份请求锁。
        let observing_store = store.clone();
        // Cancel only this external observation after actual admission.
        // 在实际入场后仅取消此外部观测。
        let observer = tokio::spawn(async move {
            // The foreground task owns the real transaction until cancellation.
            // 前台任务在取消前拥有真实事务。
            let candidate = observing_store
                .inspect_activation_candidate(&transaction)
                .expect("original");
            manager
                .restore_activation_predecessor(&candidate, &observing_drain)
                .await
        });
        fixture.entered().await;
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("cancelled observer")
                .is_cancelled()
        );
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("actual restore owns request")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_predecessor_calls(request_id)
            .await
            .expect("retire actual restore");
        fixture.empty().await;
        // Resume the original only after native retirement.
        // 仅在原生退役后继续原请求。
        let resumed = store
            .resume_activation_transaction("hook-plugin", request_id)
            .expect("retired lock");
        // Reinspection does not reset the interrupted effect.
        // 重新检查不重置被中断副作用。
        let candidate = store
            .inspect_activation_candidate(&resumed)
            .expect("same original");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("unknown restore cannot replay")
                .message
                .contains("already attempted")
        );
        assert!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("unchanged unknown")
                .expect("receipt")
                .observations()
                .contains(&(Effect::RestorePredecessor, Observation::Started))
        );
    });
}

/// Unknown old disable remains a restore blocker even after every actual worker has retired.
/// 即使全部实际工作者已退役，未知旧停用仍阻止恢复。
#[test]
fn formal_predecessor_restore_rejects_unobserved_old_disable() {
    // The fixture blocks inside the original disable's real native event.
    // 夹具在原停用的真实原生事件内阻塞。
    let (fixture, store, transaction, drain) = callback_fixture(true, true, false);
    // Keep only the completed immutable drain identity outside its cancelled observer.
    // 在被取消观察者之外仅保留已完成不可变排空身份。
    let observing_drain = drain.clone();
    // Freeze the original UUID for durable inspection.
    // 固定原 UUID，用于持久检查。
    let request_id = drain.request_id();
    executor().block_on(async {
        // The existing manager retains actual native ownership.
        // 既有管理器保留实际原生所有权。
        let manager = fixture.manager.clone();
        // The path handle cannot synthesize transaction readiness.
        // 路径句柄不能伪造事务就绪。
        let observing_store = store.clone();
        // Interrupt the old disable only after real callback admission.
        // 仅在真实回调入场后中断旧停用。
        let observer = tokio::spawn(async move {
            // The borrowed view carries actual original authority.
            // 借用视图携带实际原权威。
            let candidate = observing_store
                .inspect_activation_candidate(&transaction)
                .expect("original");
            manager
                .disable_activation_predecessor(&candidate, &observing_drain)
                .await
        });
        fixture.entered().await;
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("old observer cancelled")
                .is_cancelled()
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_predecessor_calls(request_id)
            .await
            .expect("old native work retired");
        fixture.empty().await;
        // The real request lock is released, while the unknown effect still remains authoritative.
        // 真实请求锁已释放，但未知副作用仍保持权威。
        let resumed = store
            .resume_activation_transaction("hook-plugin", request_id)
            .expect("retired request");
        // No new request or callback attempt is allocated during this refusal.
        // 此拒绝期间不分配新请求或回调尝试。
        let candidate = store
            .inspect_activation_candidate(&resumed)
            .expect("same original");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("unknown old disable")
                .message
                .contains("observed prior work")
        );
        assert!(
            !store
                .activation_effects("hook-plugin", request_id)
                .expect("evidence")
                .expect("receipt")
                .observations()
                .iter()
                .any(|(effect, _)| *effect == Effect::RestorePredecessor)
        );
    });
}

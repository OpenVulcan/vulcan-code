//! Old callbacks must execute frozen predecessor bytes under one original update identity.
//! 旧回调必须在单个原更新身份下执行固定前驱字节。

use super::*;

/// Retain the deliberate target failure used to detect accidental old-package dispatch.
/// 保留用于检测意外旧包分发的刻意目标失败。
const WRONG_PREDECESSOR_TARGET: &str = r#"
-- New bytes remain private while the old callback runs.
-- 旧回调运行时，新字节保持私有。
local M = {}
-- Reject any accidental old callback dispatch; no arguments or successful result are accepted.
-- 拒绝任何意外旧回调分发；不接受参数或成功返回值。
function M.dispatch() error('target must not run as predecessor') end
M.shutdown, M.shutdown_hooks, M.on_precheck, M.ui_action, M.on_lifecycle = M.dispatch, M.dispatch, M.dispatch, M.dispatch, M.dispatch
return M
"#;

/// Build block/declared/fail's old fixture with the deliberately distinct default target; return its original owners.
/// 用刻意不同的默认目标构建 block／declared／fail 指定的旧夹具；返回原所有者。
pub(super) fn callback_fixture(
    block: bool,
    declared: bool,
    fail: bool,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    crate::VulcanPluginPredecessorDrain,
) {
    callback_fixture_with_target(block, declared, fail, WRONG_PREDECESSOR_TARGET)
}

/// An observed old business failure stays distinct from native retirement and cannot authorize target effects.
/// 已观测旧业务失败与原生退役保持区分，不能授权目标副作用。
#[test]
fn formal_predecessor_callback_business_failure_blocks_target_and_replay() {
    // The old callback commits its native event before returning the fixture's explicit error frame.
    // 旧回调在返回夹具显式错误帧之前提交原生事件。
    let (fixture, store, transaction, drain) = callback_fixture(false, true, true);
    // Retain the unchanged original candidate view for exact subsequent denial checks.
    // 保留未变原候选视图，用于精确后续拒绝检查。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("old business failure")
                .message
                .contains("lifecycle-business-marker")
        );
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("old callback cannot replay")
                .message
                .contains("already attempted")
        );
        assert!(
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("failed old callback blocks target initialization")
                .message
                .contains("predecessor disable")
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
        fixture
            .events
            .values
            .lock()
            .expect("old side effects")
            .iter()
            .filter(|event| event.topic == "disable")
            .count(),
        1
    );
    assert!(
        fixture
            .manager
            .activation_predecessor_calls(drain.request_id())
            .expect("actual cleanup completed")
            .is_empty()
    );
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("durable failed callback")
            .expect("receipt")
            .observations(),
        vec![
            (Effect::DrainPredecessorLua, Observation::Succeeded),
            (Effect::DisablePredecessor, Observation::ObservationFailed)
        ]
    );
}

/// Construct distinct old and new packages; optionally block or omit the old disable callback.
/// 构造不同的新旧插件包；可选择阻塞或省略旧停用回调。
/// fail selects observed old disable failure; target_bundle supplies the exact future immutable target bytes.
/// fail 选择可观测旧停用失败；target_bundle 提供未来不可变目标的精确字节。
/// Return the real fixture, store, original transaction and completed Lua drain handle.
/// 返回真实夹具、存储、原事务及已完成 Lua 排空句柄。
pub(super) fn callback_fixture_with_target(
    block: bool,
    declared: bool,
    fail: bool,
    target_bundle: &str,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    crate::VulcanPluginPredecessorDrain,
) {
    // The original real lifecycle fixture supplies native services and declared process resolution.
    // 原真实生命周期夹具提供原生服务及声明进程解析。
    let fixture = Fixture::new();
    // All edits happen in the private fixture before immutable publication.
    // 全部编辑均在不可变发布之前于私有夹具内执行。
    let package = fixture._root.path().join("bundled/hook-plugin");
    // The old configuration is declared explicitly and receives a different selected value from the target.
    // 显式声明旧配置，并为其选择与目标不同的值。
    let mut manifest = std::fs::read_to_string(package.join("plugin.yaml")).expect("old manifest")
        .replace("permissions: [process_binary, ui_host_context]", "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
    if !declared {
        manifest = manifest
            .replace("    enable: on_lifecycle\n", "")
            .replace("    disable: on_lifecycle\n", "");
    }
    std::fs::write(package.join("plugin.yaml"), manifest).expect("old callback manifest");
    std::fs::write(package.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("config schema");
    std::fs::write(
        package.join("config.defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("config defaults");
    // The frozen old module proves its selected configuration at initialization and exact request at callback entry.
    // 固定旧模块在初始化时证明选定配置，在回调入口证明精确请求。
    let mut bundle = std::fs::read_to_string(package.join("runtime/plugin.bundle.lua")).expect("old bundle")
        .replace("assert(args.plugin_id == 'hook-plugin' and type(args.environment_root) == 'string')",
            "assert(args.plugin_id == 'hook-plugin' and type(args.environment_root) == 'string')\n    assert(type(args.request_id) == 'string' and #args.request_id == 36)\n    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.request_id,revision=0})");
    if block {
        bundle = bundle.replace("vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event,revision=count})",
            "vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='enable',revision=count})\n    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event,revision=count})");
    }
    if fail {
        bundle = bundle.replace(
            "if args.event == 'enable_rollback' then return",
            "if args.event == 'disable' then return",
        );
    }
    bundle = format!(
        "-- Assert original selected configuration before any business callback.\n-- 在任何业务回调前断言原选定配置。\nassert(vulcan.host.call('vulcan.plugin.config.get', {{}}).label == 'old-published')\n{bundle}"
    );
    std::fs::write(package.join("runtime/plugin.bundle.lua"), bundle)
        .expect("old configured callback");
    // Move only this known temporary package outside bundled discovery.
    // 仅将此已知临时插件包移动到内置发现范围之外。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
    std::fs::rename(package, &source).expect("private source");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled original");
    // Production publication freezes old bytes, configuration, grants and credential revision.
    // 生产发布固定旧字节、配置、授权及凭据修订。
    let (store, initial) = transaction_for(&fixture, &source, true);
    // Close the unused fixture-only request without changing its published predecessor.
    // 关闭未使用夹具专属请求，不改变已发布前驱。
    let initial_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", initial_id)
        .expect("finish unused request");
    // Write the caller's exact target bytes before immutable preparation.
    // 在不可变准备前写入调用方精确目标字节。
    std::fs::write(source.join("runtime/plugin.bundle.lua"), target_bundle)
        .expect("distinct private target");
    // Capture the still-enabled old catalog before creating the policy-bearing update.
    // 在创建携带策略更新之前捕获仍启用旧目录。
    let original = fixture.manager.snapshot();
    // Target configuration differs even though its schema is unchanged.
    // 即使模式未变，目标配置仍不同。
    let transaction = prepare_configured(
        &fixture,
        &store,
        VulcanPluginConfigurationSelection::Document {
            value: json!({"label":"new-selected"}),
        },
    );
    // The original request and its completed resource drain authorize only the old callback.
    // 原请求及已完成资源排空仅授权旧回调。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    // Actual drain starts before any callback side effects.
    // 实际排空在任何回调副作用之前开始。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("old admission closed");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("old resources retired")
                .failures
                .is_empty()
        );
    });
    assert_ne!(
        candidate.package().content_fingerprint,
        drain.identity().content_fingerprint
    );
    drop(candidate);
    (fixture, store, transaction, drain)
}

/// Old bytes and old selected configuration execute once, while the candidate remains private.
/// 旧字节及旧选定配置仅执行一次，候选保持私有。
#[test]
fn formal_predecessor_callback_uses_old_configuration_and_original_uuid() {
    // No external callback blocking is needed for successful original execution.
    // 成功原执行不需要外部回调阻塞。
    let (fixture, store, transaction, drain) = callback_fixture(false, true, false);
    // Retain exact original request through callback and replay checks.
    // 在回调及重放检查期间保留精确原请求。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    // Old execution cannot be widened into target enable or retargeted to private candidate bytes.
    // 旧执行不能扩权为目标启用或重定向至私有候选字节。
    let authority = ActivationExecution::predecessor(
        &fixture.manager.state,
        &candidate,
        &drain,
        crate::runtime::candidates::PredecessorOperation::Disable,
    )
    .expect("exact old authority");
    assert!(
        authority
            .validate(
                &fixture.manager.state,
                &authority.plugin,
                VulcanPluginBridgeLeaseScope::LifecycleEnable
            )
            .is_err()
    );
    assert!(
        authority
            .validate(
                &fixture.manager.state,
                candidate.package(),
                VulcanPluginBridgeLeaseScope::LifecycleDisable
            )
            .is_err()
    );
    // Another real manager cannot accept this host-issued transition or its original store.
    // 另一真实管理器不能接纳此宿主签发转换或其原存储。
    let foreign = Fixture::new();
    assert!(
        ActivationExecution::predecessor(
            &foreign.manager.state,
            &candidate,
            &drain,
            crate::runtime::candidates::PredecessorOperation::Disable
        )
        .is_err()
    );
    drop(authority);
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .invoke_lifecycle("hook-plugin", crate::VulcanPluginLifecycleEvent::Disable)
                .await
                .is_err()
        );
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("only frozen old callback");
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .expect_err("no replay")
                .message
                .contains("already attempted")
        );
    });
    // Events prove both the canonical request envelope and old disable export ran exactly once.
    // 事件证明规范请求信封及旧停用导出均恰好执行一次。
    let events = fixture
        .events
        .values
        .lock()
        .expect("actual old native events");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.topic == drain.request_id().to_string())
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.topic == "disable")
            .count(),
        1
    );
    assert!(!events.iter().any(|event| event.topic == "wrong-finalizer"));
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("effects")
            .expect("receipt")
            .observations(),
        vec![
            (Effect::DrainPredecessorLua, Observation::Succeeded),
            (Effect::DisablePredecessor, Observation::Succeeded)
        ]
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Draining
    );
    assert_eq!(
        store
            .activation("hook-plugin")
            .expect("old selection")
            .as_ref(),
        transaction.record().predecessor()
    );
}

/// Absent old callbacks create no Lua initialization, execution lease or false callback receipt.
/// 缺失旧回调不会创建 Lua 初始化、执行租约或虚假回调回执。
#[test]
fn formal_predecessor_callback_absence_has_no_execution() {
    // Both old enable and disable are explicitly omitted to preserve the manifest's paired callback contract.
    // 显式省略旧启用及停用，以保留清单配对回调契约。
    let (fixture, store, transaction, drain) = callback_fixture(false, false, false);
    // A real candidate view does not itself create runtime work.
    // 真实候选视图本身不创建运行时工作。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    executor().block_on(async {
        fixture
            .manager
            .disable_activation_predecessor(&candidate, &drain)
            .await
            .expect("explicit no callback");
    });
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("no native events")
            .is_empty()
    );
    assert!(
        fixture
            .manager
            .activation_predecessor_calls(drain.request_id())
            .expect("no old calls")
            .is_empty()
    );
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("effects")
            .expect("receipt")
            .observations(),
        vec![(Effect::DrainPredecessorLua, Observation::Succeeded)]
    );
}

/// A timed-out old callback remains request-owned until real native work retires, and blocks target effects.
/// 超时旧回调保持请求归属直至实际原生工作退役，并阻止目标副作用。
#[test]
fn formal_predecessor_callback_timeout_retains_actual_native_ownership() {
    // Blocking occurs in the actual old native callback, independently from Tokio observation.
    // 阻塞位于实际旧原生回调，独立于 Tokio 观测。
    let (fixture, store, transaction, drain) = callback_fixture(true, true, false);
    executor().block_on(async {
        // The same original view owns both the started callback and subsequent denied target attempt.
        // 同一原视图拥有已开始回调及后续被拒目标尝试。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        assert!(
            fixture
                .manager
                .invoke_formal_predecessor_disable(&candidate, &drain, Instant::now() + WAIT / 10)
                .await
                .expect_err("old callback deadline")
                .message
                .contains("timed out")
        );
        assert!(fixture.events.entered.load(Ordering::Acquire));
        assert_eq!(
            fixture
                .manager
                .activation_predecessor_calls(drain.request_id())
                .expect("live old callback")
                .len(),
            1
        );
        assert!(
            fixture
                .manager
                .activation_candidate_calls(drain.request_id())
                .expect("candidate query excludes old")
                .is_empty()
        );
        assert_eq!(
            fixture
                .manager
                .drain_activation_candidate_calls(drain.request_id())
                .await
                .expect("candidate-only drain")
                .closed_formal_calls,
            0
        );
        assert!(
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("old disable uncertain")
                .message
                .contains("predecessor disable")
        );
        drop(candidate);
        // Drop the foreground transaction to leave only actual execution ownership.
        // 释放前台事务，仅留下实际执行所有权。
        let request_id = transaction.record().request_id();
        drop(transaction);
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("real native request lock")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("effects")
                .expect("receipt")
                .observations(),
            vec![
                (Effect::DrainPredecessorLua, Observation::Succeeded),
                (Effect::DisablePredecessor, Observation::ObservationFailed)
            ]
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_predecessor_calls(request_id)
            .await
            .expect("retire exact original callback");
        fixture.empty().await;
        drop(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect("real lock released"),
        );
        assert!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("failed stays failed")
                .expect("receipt")
                .observations()
                .contains(&(Effect::DisablePredecessor, Observation::ObservationFailed))
        );
    });
}

/// Foreground cancellation leaves Started, while read-only original queries retain the live old operation.
/// 前台取消保留 Started，只读原查询继续保留活动旧操作。
#[test]
fn formal_predecessor_callback_cancelled_observer_does_not_replay() {
    // The real native gate permits deterministic cancellation after callback admission.
    // 真实原生门允许在回调入场后确定性取消。
    let (fixture, store, transaction, drain) = callback_fixture(true, true, false);
    // The original UUID remains independent from the task owning the transaction.
    // 原 UUID 独立于拥有事务的任务。
    let request_id = drain.request_id();
    executor().block_on(async {
        // The task owns the actual transaction; cancelling it cannot release the core's cloned request lock.
        // 任务拥有实际事务；取消它不能释放核心克隆的请求锁。
        let manager = fixture.manager.clone();
        // Store handles carry paths, not independent request ownership.
        // 存储句柄携带路径，不携带独立请求所有权。
        let observing_store = store.clone();
        // Only this external observer is cancelled, never the real native worker.
        // 仅取消此外部观察者，绝不伪造真实原生工作者终止。
        let observer = tokio::spawn(async move {
            // The borrowed original binds old callback execution throughout the operation.
            // 借用原请求在整个操作期间绑定旧回调执行。
            let candidate = observing_store
                .inspect_activation_candidate(&transaction)
                .expect("candidate");
            manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
        });
        fixture.entered().await;
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("foreground cancellation")
                .is_cancelled()
        );
        assert_eq!(
            fixture
                .manager
                .activation_predecessor_calls(request_id)
                .expect("original live call")
                .len(),
            1
        );
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("actual callback retains lock")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("effects")
                .expect("receipt")
                .observations()
                .contains(&(Effect::DisablePredecessor, Observation::Started))
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_predecessor_calls(request_id)
            .await
            .expect("actual retirement");
        fixture.empty().await;
        drop(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect("retired request"),
        );
        assert!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("no synthetic success")
                .expect("receipt")
                .observations()
                .contains(&(Effect::DisablePredecessor, Observation::Started))
        );
    });
}

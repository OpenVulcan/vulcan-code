//! Production stateless entry, detached observation and actual resource drain evidence.
//! 生产无状态入口、分离观测及真实资源排空证据。

use super::*;
use crate::runtime::tests::write_idle_session_package;
use crate::{VulcanPluginHostServices, VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

/// Bounded test observation deadline, independent of the deliberately short cleanup wait budget.
/// 有界测试观测截止时间，独立于刻意缩短的清理等待预算。
const WAIT: Duration = Duration::from_secs(10);

/// One controllable real native callback and its committed event history.
/// 单个可控真实原生回调及其已提交事件历史。
#[derive(Default)]
struct Events {
    /// Actual entry is observed before any test requests cancellation or lifecycle change.
    /// 测试请求取消或生命周期变化前先观测实际入场。
    entered: AtomicBool,
    /// This explicit release models a native operation that ignores cooperative cancellation.
    /// 此显式释放模拟忽略协作取消的原生操作。
    released: Mutex<bool>,
    /// Wake the exact native waiter without sleep-based synchronization.
    /// 唤醒精确原生等待者，不使用基于睡眠的同步。
    changed: Condvar,
    /// Only effects after the controlled wait enter this ledger.
    /// 仅受控等待之后的副作用进入此账本。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}

impl Events {
    /// Release the actual native callback, including during test-unwind cleanup.
    /// 释放真实原生回调，包含测试展开清理期间。
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.changed.notify_all();
    }
}

impl VulcanPluginUiEventPublisher for Events {
    /// Block only the declared `block` topic, then commit `event`; return lock failures explicitly.
    /// 仅阻塞声明的 `block` 主题，然后提交 `event`；明确返回锁失败。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "block" {
            self.entered.store(true, Ordering::Release);
            let mut released = self.released.lock().map_err(|error| error.to_string())?;
            while !*released {
                released = self
                    .changed
                    .wait(released)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

/// Real production fixture; fields retain package bytes through runtime cleanup.
/// 真实生产夹具；字段在运行时清理期间保留包字节。
struct Fixture {
    /// Manager exposes the same public invocation path used in production.
    /// 管理器暴露生产使用的相同公开调用路径。
    manager: VulcanPluginRuntimeManager,
    /// Actual native observer used to prove side effects and blocking ownership.
    /// 用于证明副作用及阻塞所有权的真实原生观测者。
    events: Arc<Events>,
    /// Runtime remains alive until all test calls and cleanup have completed.
    /// 运行时保持存活至所有测试调用及清理完成。
    runtime: tokio::runtime::Runtime,
    /// Files outlive the manager and its callbacks.
    /// 文件比管理器及回调存活更久。
    root: tempfile::TempDir,
}

impl Drop for Fixture {
    /// Unblock native work before the Tokio runtime is dropped, including assertion failure paths.
    /// 在丢弃 Tokio 运行时前解除原生阻塞，包含断言失败路径。
    fn drop(&mut self) {
        self.events.release();
    }
}

impl Fixture {
    /// Construct one real enabled stateless package using the explicit parent `settings`.
    /// 使用明确父级 `settings` 构造单个真实启用的无状态包。
    /// Returns a bounded runtime whose blocking pool can be deliberately saturated by a test.
    /// 返回阻塞池可由测试刻意占满的有界运行时。
    fn new(settings: VulcanPluginRuntimeSettings) -> Self {
        let root = tempfile::tempdir().expect("create stateless fixture");
        let bundled = root.path().join("bundled");
        write_idle_session_package(&bundled);
        let package = bundled.join("idle-plugin");
        let mut manifest =
            std::fs::read_to_string(package.join("plugin.yaml")).expect("read fixture manifest");
        manifest.push_str("permissions: [ui_host_context]\n");
        std::fs::write(package.join("plugin.yaml"), manifest).expect("declare actual permission");
        std::fs::write(package.join("runtime/plugin.bundle.lua"), r#"
-- Module state must remain local to one stateless physical instance.
-- 模块状态必须局限于单次无状态物理实例。
local M, count, business_id, closing_error = {}, 0, nil, false
-- Decode the host envelope and return a version-one result or requested failure.
-- 解码宿主信封并返回版本一结果或请求的失败。
-- args contains payload_json and host_timeout_ms; successful results expose correlation for verification.
-- args 包含 payload_json 及 host_timeout_ms；成功结果暴露关联供验证。
function M.dispatch(args)
    local p = vulcan.json.decode(args.payload_json)
    assert(p.op == 'ping' and p.host_timeout_ms == args.host_timeout_ms)
    count, business_id, closing_error = count + 1, p.request_id, p.mode == 'shutdown_error'
    if p.mode == 'block' then vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='block',revision=count}) end
    if p.mode == 'business_error' then return vulcan.json.encode({ok=false,error='business-marker'}) end
    if p.mode == 'lua_error' then error('lua-marker') end
    return vulcan.json.encode({ok=true,result={count=count,request_id=business_id}})
end
-- Validate independent shutdown correlation and observe the same physical counter.
-- 校验独立关闭关联并观测同一物理计数器。
-- args is the frozen shutdown envelope; return an explicit closing success or failure frame.
-- args 为冻结关闭信封；返回明确关闭成功或失败帧。
function M.shutdown(args)
    local p = vulcan.json.decode(args.payload_json)
    assert(p.op == 'shutdown' and p.request_id ~= business_id and p.host_timeout_ms == args.host_timeout_ms)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='closed',revision=count})
    if closing_error then return vulcan.json.encode({ok=false,error='shutdown-marker'}) end
    return vulcan.json.encode({ok=true,result={closed=true}})
end
return M
"#).expect("write production bundle");
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish stateless tool");
        let events = Arc::new(Events::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(events.clone()),
            })
            .expect("install native service");
        manager
            .initialize_formal_runtime(settings)
            .expect("attach formal runtime");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(1)
            .build()
            .expect("build observer runtime");
        Self {
            manager,
            events,
            runtime,
            root,
        }
    }

    /// Build the unchanged production request shape for `mode` under one fixed controller owner.
    /// 为 `mode` 在单个固定控制器所有者下构造未改变的生产请求形状。
    fn request(&self, mode: &str) -> crate::VulcanPluginInvocationRequest {
        crate::VulcanPluginInvocationRequest {
            tool_id: "idle_ping".to_string(),
            arguments: json!({"mode":mode}),
            owner_session_id: "owner".to_string(),
            workspace_root: self.root.path().to_path_buf(),
        }
    }

    /// Observe actual native entry with a bounded deadline; returns no inferred completion.
    /// 使用有界截止时间观测真实原生入场；不返回推测完成。
    async fn wait_entered(&self) {
        tokio::time::timeout(WAIT, async {
            while !self.events.entered.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("native callback must enter");
    }

    /// Wait for the single retained record to retire physically; return its stable public evidence.
    /// 等待单个保留记录物理退役；返回其稳定公开证据。
    async fn retired(&self) -> VulcanPluginFormalCallStatus {
        tokio::time::timeout(WAIT, async {
            loop {
                let records = self
                    .manager
                    .formal_calls("owner")
                    .expect("query owned records");
                assert_eq!(records.len(), 1, "one retained invocation expected");
                let record = records.into_iter().next().expect("retained record");
                if record.retired && (record.value.is_some() || record.error.is_some()) {
                    return record;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual retirement must finish")
    }
}

/// Normal production calls use fresh instances, execute same-instance shutdown, and release acknowledged records.
/// 普通生产调用使用全新实例、执行同实例关闭，并释放已确认记录。
#[test]
fn formal_stateless_production_route_preserves_fresh_state_and_envelopes() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let mut ids = BTreeSet::new();
    for _ in 0..2 {
        let value = fixture
            .runtime
            .block_on(fixture.manager.invoke(fixture.request("ok")))
            .expect("production stateless call");
        assert_eq!(value["count"], 1);
        assert!(
            ids.insert(
                value["request_id"]
                    .as_str()
                    .expect("opaque correlation")
                    .to_string()
            )
        );
        assert!(
            fixture
                .manager
                .formal_calls("owner")
                .expect("acknowledged calls")
                .is_empty()
        );
        assert_eq!(
            fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("core usage")
                .expect("attached")
                .resources
                .resident,
            0
        );
    }
    let events = fixture.events.values.lock().expect("actual closing events");
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|event| event.topic == "closed" && event.revision == 1)
    );
}

/// Business and closing failures retain exact core evidence and reject a foreign controller's query or removal.
/// 业务及关闭失败保留精确核心证据，并拒绝外来控制器查询或移除。
#[test]
fn formal_stateless_failures_remain_queryable_without_replay() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    for mode in ["business_error", "lua_error", "shutdown_error"] {
        let error = fixture
            .runtime
            .block_on(fixture.manager.invoke(fixture.request(mode)))
            .expect_err("explicit failure");
        let status = fixture.runtime.block_on(fixture.retired());
        assert!(error.message.contains(&status.invocation_id));
        let core = status.operation.expect("core identity");
        assert!(
            core.finalization.is_some(),
            "independent closing evidence must remain"
        );
        assert!(
            fixture
                .manager
                .formal_call_status(&status.invocation_id, "foreign")
                .is_err()
        );
        assert!(
            fixture
                .manager
                .forget_formal_call(&status.invocation_id, "foreign")
                .is_err()
        );
        fixture
            .manager
            .forget_formal_call(&status.invocation_id, "owner")
            .expect("acknowledge exact result");
    }
    assert_eq!(
        fixture.events.values.lock().expect("closing effects").len(),
        3
    );
}

/// Caller abortion and plugin draining cannot release capacity before an uninterruptible native call returns.
/// 调用方中断及插件排空不能在不可中断原生调用返回前释放容量。
#[test]
fn formal_stateless_abandonment_stays_owned_through_actual_drain() {
    let settings = VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: 100,
        ..VulcanPluginRuntimeSettings::default()
    };
    let fixture = Fixture::new(settings);
    fixture.runtime.block_on(async {
        let manager = fixture.manager.clone();
        let request = fixture.request("block");
        let waiter = tokio::spawn(async move { manager.invoke(request).await });
        fixture.wait_entered().await;
        waiter.abort();
        assert!(waiter.await.expect_err("caller was aborted").is_cancelled());
        fixture
            .manager
            .begin_draining("idle-plugin")
            .expect("publish drain");
        let report = fixture
            .manager
            .drain_plugin_sessions("idle-plugin")
            .await
            .expect("observe bounded drain");
        assert_eq!(report.closed_formal_calls, 0);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
            1
        );
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("live resources")
                .expect("attached")
                .resources
                .resident,
            1
        );
        fixture.events.release();
        let status = fixture.retired().await;
        assert!(
            status
                .operation
                .as_ref()
                .expect("core evidence")
                .finalization
                .as_ref()
                .expect("automatic shutdown")
                .outcome
                .as_ref()
                .expect("closing outcome")
                .result()
                .is_ok()
        );
        assert_eq!(
            fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
        let retry = fixture
            .manager
            .drain_owner_sessions("owner")
            .await
            .expect("drain formal-only owner after native completion");
        assert_eq!(retry.closed_formal_calls, 1);
        assert!(retry.failures.is_empty(), "{retry:?}");
        assert!(
            fixture.manager.snapshot().plugins["idle-plugin"]
                .diagnostics
                .iter()
                .all(|message| !message.contains("has not physically retired")),
            "completed owner drain must clear its previous physical-retirement warning"
        );
        fixture
            .manager
            .forget_formal_call(&status.invocation_id, "owner")
            .expect("acknowledge abandoned result");
    });
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("actual native effects")
            .len(),
        2
    );
}

/// Pre-submission cancellation remains visible even when the bounded preparation worker has not started.
/// 有界准备工作线程尚未启动时，提交前取消仍保持可见。
#[test]
fn formal_stateless_preparation_is_tracked_before_blocking_work() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        let blocker = Arc::clone(&fixture.events);
        let blocked = tokio::task::spawn_blocking(move || {
            blocker.publish(VulcanPluginUiInvalidation {
                plugin_id: "test".to_string(),
                topic: "block".to_string(),
                revision: 0,
            })
        });
        fixture.wait_entered().await;
        let manager = fixture.manager.clone();
        let request = fixture.request("ok");
        let waiter = tokio::spawn(async move { manager.invoke(request).await });
        let status = tokio::time::timeout(WAIT, async {
            loop {
                if let Some(status) = fixture
                    .manager
                    .formal_calls("owner")
                    .expect("preparing calls")
                    .into_iter()
                    .next()
                {
                    break status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("preparation must be registered");
        assert!(status.operation.is_none());
        assert!(!status.retired);
        fixture
            .manager
            .cancel_formal_call(&status.invocation_id, "owner")
            .expect("cancel before worker admission");
        fixture.events.release();
        blocked
            .await
            .expect("blocking task completed")
            .expect("test blocker released");
        assert!(waiter.await.expect("caller completed").is_err());
        let completed = fixture.retired().await;
        assert!(
            completed.operation.is_none(),
            "cancelled preparation must never submit Lua"
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("only test blocker committed")
                .len(),
            1
        );
        fixture
            .manager
            .forget_formal_call(&completed.invocation_id, "owner")
            .expect("acknowledge pre-submission cancellation");
    });
}

/// Retained failures and oversized inputs enforce explicit admission limits without starting another VM.
/// 保留失败及超大输入执行明确入场上限，不启动另一 VM。
#[test]
fn formal_stateless_retention_and_input_budgets_are_bounded() {
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_operations = 2;
    settings.parent.max_running_calls = settings.parent.max_operations;
    settings.parent.max_queued_calls = settings.parent.max_operations;
    settings.parent.max_value_bytes = 4096;
    let fixture = Fixture::new(settings);
    for _ in 0..2 {
        assert!(
            fixture
                .runtime
                .block_on(fixture.manager.invoke(fixture.request("business_error")))
                .is_err()
        );
    }
    let failure = fixture
        .runtime
        .block_on(fixture.manager.invoke(fixture.request("ok")))
        .expect_err("retention must reject admission");
    assert!(failure.message.contains("retention capacity"));
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("no third execution")
            .len(),
        2
    );
    for status in fixture
        .manager
        .formal_calls("owner")
        .expect("bounded history")
    {
        fixture
            .manager
            .forget_formal_call(&status.invocation_id, "owner")
            .expect("release retained result");
    }
    let mut oversized = fixture.request("ok");
    oversized.arguments["text"] = json!("x".repeat(4096));
    assert!(
        fixture
            .runtime
            .block_on(fixture.manager.invoke(oversized))
            .is_err()
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("no oversized retention")
            .is_empty()
    );
    assert!(
        fixture
            .runtime
            .block_on(fixture.manager.invoke(fixture.request("ok")))
            .is_ok()
    );
}

/// Replacing the observer executor requires cleanup recovery but never a second Lua submission.
/// 替换观测执行器需要清理恢复，但绝不产生第二次 Lua 提交。
#[test]
fn formal_stateless_executor_shutdown_retains_recoverable_ownership() {
    let mut fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let manager = fixture.manager.clone();
    let request = fixture.request("block");
    let waiter = fixture
        .runtime
        .spawn(async move { manager.invoke(request).await });
    fixture.runtime.block_on(fixture.wait_entered());
    let replacement = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .expect("create recovery executor");
    let previous = std::mem::replace(&mut fixture.runtime, replacement);
    previous.shutdown_timeout(WAIT);
    drop(waiter);
    let interrupted = fixture
        .manager
        .formal_calls("owner")
        .expect("retained interrupted call")
        .into_iter()
        .next()
        .expect("original host identity");
    assert!(interrupted.requires_cleanup_recovery);
    assert!(!interrupted.retired);
    assert!(
        !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("pending host ownership")
    );
    fixture.events.release();
    let report = fixture
        .runtime
        .block_on(fixture.manager.drain_owner_sessions("owner"))
        .expect("recover original cleanup");
    assert_eq!(report.closed_formal_calls, 1);
    assert!(report.failures.is_empty(), "{report:?}");
    let completed = fixture.runtime.block_on(fixture.retired());
    assert_eq!(completed.invocation_id, interrupted.invocation_id);
    assert_eq!(
        completed
            .operation
            .as_ref()
            .expect("completed core")
            .operation_id,
        interrupted
            .operation
            .as_ref()
            .expect("original core")
            .operation_id
    );
    fixture
        .manager
        .forget_formal_call(&completed.invocation_id, "owner")
        .expect("acknowledge recovered result");
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("one business effect and one close")
            .len(),
        2
    );
}

/// Host accounting failure retains cleanup ownership after real VM release; retry never forgets the pool twice.
/// 真实 VM 释放后宿主记账失败仍保留清理所有权；重试绝不二次遗忘池。
#[test]
fn formal_stateless_accounting_failure_does_not_claim_retirement() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        let manager = fixture.manager.clone();
        let request = fixture.request("block");
        let waiter = tokio::spawn(async move { manager.invoke(request).await });
        fixture.wait_entered().await;
        let counts = &fixture.manager.state.active_session_counts;
        let poisoning = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = counts.lock().expect("inject accounting lock failure");
            panic!("test accounting interruption");
        }));
        assert!(poisoning.is_err());
        fixture.events.release();
        let failure = waiter
            .await
            .expect("caller finished")
            .expect_err("host count release must report failure");
        assert!(failure.to_string().contains("counter lock is poisoned"));
        let status = fixture
            .manager
            .formal_calls("owner")
            .expect("query failed release")
            .into_iter()
            .next()
            .expect("retained call");
        assert!(!status.retired);
        assert!(status.requires_cleanup_recovery);
        assert_eq!(
            fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
            1
        );
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("core usage")
                .expect("attached")
                .resources
                .resident,
            0
        );
        assert!(
            fixture
                .manager
                .forget_formal_call(&status.invocation_id, "owner")
                .is_err()
        );
        // The fixture explicitly repairs its injected lock failure; production never clears poison automatically.
        // 夹具显式修复自身注入的锁故障；生产绝不自动清除中毒状态。
        counts.clear_poison();
        let report = fixture
            .manager
            .drain_owner_sessions("owner")
            .await
            .expect("retry host cleanup");
        assert_eq!(report.closed_formal_calls, 1);
        assert!(report.failures.is_empty(), "{report:?}");
        assert_eq!(
            fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
        fixture
            .manager
            .forget_formal_call(&status.invocation_id, "owner")
            .expect("acknowledge recovered call");
        assert_eq!(fixture.events.values.lock().expect("no replay").len(), 2);
    });
}

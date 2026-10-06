//! Real production Hook calls prove state, workspace authority and physical cleanup.
//! 真实生产 Hook 调用证明状态、工作区权威及物理清理。

use super::*;

mod diagnostics;
mod ui;

use crate::runtime::tests::{test_manager, write_hook_package};
use crate::{VulcanPluginHostServices, VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation};
use std::collections::BTreeSet;
use std::sync::Condvar;
use std::time::Duration;
use vulcan_hooks::{
    HookActor, HookCapabilitySet, HookContext, HookInvocation, HookMode, HookPayload,
    HookRegistration, HookScope,
};

/// Test observation bound remains independent of deliberately short business deadlines.
/// 测试观测上限独立于刻意缩短的业务截止时间。
const WAIT: Duration = Duration::from_secs(10);
/// Finite drain budget deliberately expires before the test releases native work.
/// 有限排空预算刻意在测试释放原生工作前到期。
const DRAIN_WAIT_MS: u64 = 100;

/// A real native callback can remain physically active after cooperative cancellation.
/// 真实原生回调可在协作取消后继续物理活跃。
#[derive(Default)]
struct Events {
    /// Entry is published before the controlled native wait.
    /// 入场在受控原生等待前发布。
    entered: AtomicBool,
    /// Explicit release never derives from cancellation intent.
    /// 显式释放绝不来自取消意图。
    released: Mutex<bool>,
    /// Wake the exact blocked callback.
    /// 唤醒精确阻塞回调。
    changed: Condvar,
    /// Committed native events prove shutdown count and original instance state.
    /// 已提交原生事件证明关闭次数及原实例状态。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}

impl Events {
    /// Release native work, including test-unwind cleanup; no result is fabricated.
    /// 释放原生工作，包含测试展开清理；不伪造结果。
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.changed.notify_all();
    }
}

impl VulcanPluginUiEventPublisher for Events {
    /// Block the explicit block topic, then retain event; return real lock errors.
    /// 阻塞明确 block 主题，再保留 event；返回真实锁错误。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "block" {
            self.entered.store(true, Ordering::Release);
            // Hold the native wait condition until explicit release is observed.
            // 持有原生等待条件，直至观测到显式释放。
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

/// One isolated package, manager and native recorder exercise the actual Hook production entry.
/// 一个隔离包、管理器及原生记录器覆盖实际 Hook 生产入口。
struct Fixture {
    /// Keep executable files alive until all assertions finish.
    /// 保持可执行文件存活至全部断言结束。
    root: tempfile::TempDir,
    /// Production manager owns the sole formal parent.
    /// 生产管理器拥有唯一正式父级。
    manager: VulcanPluginRuntimeManager,
    /// Real host service records actual effects.
    /// 真实宿主服务记录实际副作用。
    events: Arc<Events>,
}

impl Drop for Fixture {
    /// Unblock native work if an assertion panics.
    /// 断言恐慌时解除原生工作阻塞。
    fn drop(&mut self) {
        self.events.release();
    }
}

impl Fixture {
    /// Create a declared bundle with settings and hook_ms; return its production manager and recorder.
    /// 以 settings 及 hook_ms 创建已声明包；返回生产管理器及记录器。
    fn new(settings: VulcanPluginRuntimeSettings, hook_ms: u64) -> Self {
        // Own all fixture files through test completion.
        // 持有全部夹具文件直至测试完成。
        let root = tempfile::tempdir().expect("Hook fixture directory");
        // Create the same bundled package layout consumed by catalog discovery.
        // 创建目录发现所消费的相同内建包布局。
        let bundled = root.path().join("bundled");
        write_hook_package(&bundled);
        // Extend the known package before any runtime domain is registered.
        // 在注册任何运行时域之前扩展已知包。
        let package = bundled.join("hook-plugin");
        std::fs::write(package.join("fixture.bin"), b"never executed")
            .expect("declared resolver file");
        // Use the real host target for the declared native binary.
        // 为已声明原生程序使用真实宿主目标。
        let target = crate::runtime::process::current_plugin_target_triple().expect("target");
        // Extend the authoritative manifest text with exact native permissions and test budgets.
        // 以精确原生权限及测试预算扩展权威清单文本。
        let manifest = std::fs::read_to_string(package.join("plugin.yaml")).expect("manifest")
            .replace("execution:\n", &format!("permissions: [process_binary, ui_host_context]\nexecution:\n  native_binaries:\n    - id: fixture\n      allow_workspace_cwd: true\n      targets:\n        {target}: fixture.bin\n"))
            .replace("timeout_ms: 1000", &format!("timeout_ms: {hook_ms}"));
        std::fs::write(package.join("plugin.yaml"), manifest).expect("publish manifest");
        std::fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"
-- One counter belongs to the physical Hook instance.
-- 一个计数器属于物理 Hook 实例。
local M, count, fail_close = {}, 0, false
-- Decode the real Hook envelope; return state and the actual process authorization result.
-- 解码真实 Hook 信封；返回状态及实际进程授权结果。
-- args contains invocation_json and host_timeout_ms; result follows the Hook payload shape.
-- args 包含 invocation_json 及 host_timeout_ms；结果遵循 Hook 载荷形状。
function M.on_precheck(args)
    -- Decode exactly the host-owned Hook envelope.
    -- 精确解码宿主持有的 Hook 信封。
    local invocation = vulcan.json.decode(args.invocation_json)
    assert(invocation.registration.point == 'conversation.turn.precheck')
    assert(invocation.context.run_id and args.host_timeout_ms > 0)
    -- Business input chooses only the test operation, never its authority.
    -- 业务输入仅选择测试操作，绝不选择其权威。
    local value = invocation.payload.value
    count, fail_close = count + 1, value.mode == 'close_error'
    if value.mode == 'block' then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='block',revision=count})
    end
    -- Optional process evidence is absent when the test does not request resolution.
    -- 测试未请求解析时，可选进程证据缺省。
    local resolved = nil
    if value.cwd then
        -- Catch the real restricted bridge error so denied paths remain observable results.
        -- 捕获真实受限桥错误，使拒绝路径保持为可观测结果。
        local allowed, result = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
            {entry_id='fixture',mode='exec',options={cwd=value.cwd},workspace_root=value.cwd})
        resolved = {allowed=allowed,value=result}
    end
    return vulcan.json.encode({ok=true,result={payload={value={count=count,resolved=resolved}}}})
end
-- Validate the independent frozen closing envelope and emit one same-instance effect.
-- 校验独立冻结关闭信封并发出一次同实例副作用。
-- args supplies payload_json; return explicit success or failure without replay.
-- args 提供 payload_json；返回明确成功或失败，不重放。
function M.shutdown_hooks(args)
    -- Independent shutdown consumes its frozen envelope instead of the last business request.
    -- 独立关闭消费冻结信封，不消费最近业务请求。
    local closing = vulcan.json.decode(args.payload_json)
    assert(closing.event == 'hook_runtime_shutdown' and closing.plugin_id == 'hook-plugin')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='closed',revision=count})
    if fail_close then return '{"ok":false,"error":"hook-close-marker"}' end
    return '{"ok":true,"result":{}}'
end
-- Other declared exports remain valid but are not Hook lifecycle aliases.
-- 其他已声明导出保持有效，但不是 Hook 生命周期别名。
-- No arguments are needed; return an empty version-one result.
-- 不需要参数；返回空版本一结果。
function M.dispatch() return '{"ok":true,"result":{}}' end
M.shutdown, M.ui_action, M.on_disable = M.dispatch, M.dispatch, M.dispatch
return M
"#,
        )
        .expect("real Hook bundle");
        // Retain the production manager for the exact invocation under test.
        // 为待测精确调用保留生产管理器。
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("catalog");
        // Observe actual native service effects, including finalization.
        // 观测实际原生服务副作用，包含关闭。
        let events = Arc::new(Events::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(events.clone()),
            })
            .expect("native recorder");
        manager
            .initialize_formal_runtime(settings)
            .expect("formal parent");
        Self {
            root,
            manager,
            events,
        }
    }

    /// Return the currently declared Hook, never a fabricated generation.
    /// 返回当前声明 Hook，绝不伪造代次。
    fn hook(&self) -> VulcanPluginHookDescriptor {
        self.manager
            .snapshot()
            .enabled_hooks
            .values()
            .next()
            .expect("Hook declaration")
            .clone()
    }

    /// Construct the actual host envelope from value and optional workspace.
    /// 从 value 及可选 workspace 构造实际宿主信封。
    fn invocation(&self, value: Value, workspace: Option<&Path>) -> HookInvocation {
        // Read the live catalog registration and its current generation.
        // 读取实时目录注册及其当前代次。
        let hook = self.hook();
        // Build the real Hook envelope before asynchronous dispatch.
        // 在异步分发前构建真实 Hook 信封。
        let mut invocation = HookInvocation {
            registration: HookRegistration::new(
                hook.registration_id,
                "conversation.turn.precheck",
                HookMode::Transform,
            ),
            context: HookContext {
                run_id: Uuid::new_v4().to_string(),
                point: "conversation.turn.precheck".into(),
                scope: HookScope::Session,
                actor: HookActor::new("formal-hook-test"),
                workspace_id: None,
                project_id: None,
                session_id: Some("session".into()),
                conversation_id: None,
                user_id: None,
                capabilities: HookCapabilitySet::default(),
                metadata: BTreeMap::new(),
            },
            payload: HookPayload::empty_object(),
        };
        invocation.payload.value = value;
        if let Some(workspace) = workspace {
            invocation
                .context
                .metadata
                .insert("workspace_root".into(), json!(workspace));
        }
        invocation
    }

    /// Return the actual retained parent for resource and operation assertions.
    /// 返回实际保留父级，用于资源及操作断言。
    fn owner(&self) -> Arc<ManagedFormalRuntime> {
        self.manager
            .formal_runtime_owner()
            .expect("parent lock")
            .expect("parent")
    }
}

/// Build one controlled observer executor; return the runtime with a single preparation worker.
/// 构建一个受控观测执行器；返回仅有一个准备工作线程的运行时。
fn executor() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .expect("executor")
}

/// Wait for actual native entry; a timeout fails instead of assuming entry from elapsed time.
/// 等待实际原生入场；超时则失败，不根据经过时间假定入场。
async fn entered(events: &Events) {
    // Bound test observation without treating elapsed time as completion evidence.
    // 限定测试观测时间，不将经过时间当作完成证据。
    let expires = Instant::now() + WAIT;
    while !events.entered.load(Ordering::Acquire) {
        assert!(
            Instant::now() < expires,
            "native Hook callback never entered"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
}

/// Production Hook state spans workspaces and calls beyond history capacity, and closes exactly once.
/// 生产 Hook 状态跨工作区及超过历史容量的调用保持，并且仅关闭一次。
#[test]
fn formal_hook_state_workspace_and_history_reclamation() {
    // Configure explicit test budgets while retaining all other validated defaults.
    // 配置明确测试预算，同时保留其他已校验默认值。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_operations = 8;
    settings.parent.max_queued_calls = settings.parent.max_operations;
    // Retain the isolated production fixture until actual cleanup is observed.
    // 保留隔离生产夹具至观测到实际清理。
    let fixture = Fixture::new(settings, 1000);
    // Use the explicitly constructed observer executor for this test.
    // 为此测试使用显式构建的观测执行器。
    let runtime = executor();
    runtime.block_on(async {
        // The first workspace must not authorize its sibling.
        // 第一个工作区不得授权其相邻目录。
        let first = fixture.root.path().join("first");
        // The second workspace must share the same Lua counter.
        // 第二个工作区必须共享同一个 Lua 计数器。
        let second = fixture.root.path().join("second");
        std::fs::create_dir(&first).expect("first workspace");
        std::fs::create_dir(&second).expect("second workspace");
        assert!(
            fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect("prewarm")
        );
        for index in 1..=20 {
            let (workspace, requested, allowed) = match index % 3 {
                1 => (&first, &first, true),
                2 => (&second, &second, true),
                _ => (&first, &second, false),
            };
            // Observe the result returned through the production Hook entry.
            // 观测通过生产 Hook 入口返回的结果。
            let result = fixture
                .manager
                .invoke_hook(
                    &fixture.hook(),
                    fixture.invocation(json!({"cwd":requested}), Some(workspace)),
                )
                .await
                .expect("Hook call");
            // Inspect the typed Hook payload returned by real Lua execution.
            // 检查真实 Lua 执行返回的类型化 Hook 载荷。
            let value = result.payload.expect("Hook payload").value;
            assert_eq!(value["count"], index);
            assert_eq!(value["resolved"]["allowed"], allowed, "{value}");
            if allowed {
                assert_eq!(
                    value["resolved"]["value"]["cwd"],
                    vulcan_runtime_assets::native_host_path_text_from_path(
                        &requested
                            .canonicalize()
                            .expect("canonical requested workspace")
                    )
                );
            }
        }
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("begin drain");
        // Check physical retirement separately from graceful shutdown diagnostics.
        // 将物理退役与优雅关闭诊断分开检查。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("drain");
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("repeat drain")
                .closed_hook_runtimes,
            0
        );
        // Observe actual native service effects, including finalization.
        // 观测实际原生服务副作用，包含关闭。
        let events = fixture.events.values.lock().expect("events");
        // Select shutdown events by their semantic topic instead of a mutable position.
        // 按语义主题选择关闭事件，不按可变位置选择。
        let closed: Vec<_> = events
            .iter()
            .filter(|event| event.topic == "closed")
            .collect();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].revision, 20);
        drop(events);
        // Inspect the actual formal parent used by production dispatch.
        // 检查生产分发使用的实际正式父级。
        let owner = fixture.owner();
        assert_eq!(
            owner
                .host
                .runtime()
                .list_operations(None, None, 8)
                .expect("history")
                .operation_ids
                .len(),
            0
        );
        assert!(owner.instances_drained().expect("Hook drained"));
    });
}

/// Business timeout and a finite drain cannot release a callback that is still physically executing.
/// 业务超时及有限排空不能释放仍在物理执行的回调。
#[test]
fn formal_hook_timeout_retains_physical_owner_until_native_return() {
    // Configure explicit test budgets while retaining all other validated defaults.
    // 配置明确测试预算，同时保留其他已校验默认值。
    let settings = VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: DRAIN_WAIT_MS,
        ..VulcanPluginRuntimeSettings::default()
    };
    // Retain the isolated production fixture until actual cleanup is observed.
    // 保留隔离生产夹具至观测到实际清理。
    let fixture = Fixture::new(settings, 100);
    // Use the explicitly constructed observer executor for this test.
    // 为此测试使用显式构建的观测执行器。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("prewarm");
        // Retain the production manager for the exact invocation under test.
        // 为待测精确调用保留生产管理器。
        let manager = fixture.manager.clone();
        // Read the live catalog registration and its current generation.
        // 读取实时目录注册及其当前代次。
        let hook = fixture.hook();
        // Build the real Hook envelope before asynchronous dispatch.
        // 在异步分发前构建真实 Hook 信封。
        let invocation = fixture.invocation(json!({"mode":"block"}), None);
        // Retain the original asynchronous caller while native work blocks.
        // 原生工作阻塞时保留原异步调用方。
        let call = tokio::spawn(async move { manager.invoke_hook(&hook, invocation).await });
        entered(&fixture.events).await;
        // A second real Hook queues behind the blocked first call and must expire without executing.
        // 第二个真实 Hook 排在阻塞首调用之后，必须超时且不执行。
        let queued_manager = fixture.manager.clone();
        // Use the same declared Hook to exercise its serial queue.
        // 使用同一已声明 Hook 覆盖其串行队列。
        let queued_hook = fixture.hook();
        // Keep queued business distinguishable from the blocked first call.
        // 使排队业务可与阻塞首调用区分。
        let queued_invocation = fixture.invocation(json!({"mode":"queued"}), None);
        // Observe queue expiry without releasing the blocked first callback.
        // 不释放阻塞首回调而观测队列到期。
        let queued = tokio::spawn(async move {
            queued_manager
                .invoke_hook(&queued_hook, queued_invocation)
                .await
        });
        assert!(call.await.expect("supervisor").is_err());
        assert!(queued.await.expect("queued supervisor").is_err());
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("begin drain");
        // Check physical retirement separately from graceful shutdown diagnostics.
        // 将物理退役与优雅关闭诊断分开检查。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("finite drain");
        assert_eq!(report.closed_hook_runtimes, 0);
        assert!(!report.failures.is_empty());
        // Inspect the actual formal parent used by production dispatch.
        // 检查生产分发使用的实际正式父级。
        let owner = fixture.owner();
        assert!(!owner.instances_drained().expect("actual ownership"));
        // Keep the original Hook record to prove later cleanup does not replace it.
        // 保留原 Hook 记录，以证明后续清理未将其替换。
        let retained = owner.hooks.lock().expect("Hook registry")["hook-plugin"].clone();
        assert!(!retained.lock().expect("evidence").operations.is_empty());
        fixture.events.release();
        // Bound test observation without treating elapsed time as completion evidence.
        // 限定测试观测时间，不将经过时间当作完成证据。
        let expires = Instant::now() + WAIT;
        while !retained.lock().expect("evidence").retired {
            assert!(Instant::now() < expires, "Hook did not physically retire");
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        // Check physical retirement separately from graceful shutdown diagnostics.
        // 将物理退役与优雅关闭诊断分开检查。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("resume drain");
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(
            fixture
                .events
                .values
                .lock()
                .expect("events")
                .iter()
                .filter(|event| event.topic == "closed")
                .all(|event| event.revision == 1)
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("events")
                .iter()
                .filter(|event| event.topic == "closed")
                .count(),
            1
        );
    });
}

/// Dropping the caller's executor leaves supervision running, and drain retains shutdown failures.
/// 丢弃调用方执行器不影响监督继续运行，排空保留关闭失败。
#[test]
fn formal_hook_supervision_survives_caller_executor_and_shutdown_failure_is_retained() {
    // Retain the isolated production fixture until actual cleanup is observed.
    // 保留隔离生产夹具至观测到实际清理。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default(), 1000);
    // Use the explicitly constructed observer executor for this test.
    // 为此测试使用显式构建的观测执行器。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .invoke_hook(
                &fixture.hook(),
                fixture.invocation(json!({"mode":"close_error"}), None),
            )
            .await
            .expect("business succeeds");
    });
    // Inspect the actual formal parent used by production dispatch.
    // 检查生产分发使用的实际正式父级。
    let owner = fixture.owner();
    // Freeze the identity that must survive executor destruction.
    // 冻结必须在执行器销毁后存活的身份。
    let original = owner.hooks.lock().expect("registry")["hook-plugin"].clone();
    drop(runtime);
    // The monitor belongs to the manager's supervisor, not to the executor that admitted the call.
    // 监控属于管理器监督器，而非接纳调用的执行器。
    assert!(original.lock().expect("evidence").monitored);
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("begin drain");
    // Drain from a new executor without creating a new plugin instance.
    // 从新执行器排空，不创建新插件实例。
    let replacement = executor();
    replacement.block_on(async {
        // Check physical retirement separately from graceful shutdown diagnostics.
        // 将物理退役与优雅关闭诊断分开检查。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("recover original");
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(
            report
                .failures
                .iter()
                .any(|error| error.contains("hook-close-marker")),
            "{:?}",
            report.failures
        );
        assert!(original.lock().expect("evidence").retired);
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("events")
                .iter()
                .filter(|event| event.topic == "closed")
                .count(),
            1
        );
        assert!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("repeat")
                .failures
                .is_empty()
        );
    });
}

/// Preparation is registered before the blocked worker starts, and draining prevents business publication.
/// 准备在阻塞工作线程开始前注册，且排空阻止业务发布。
#[test]
fn formal_hook_preparation_is_owned_before_blocking_and_rejected_after_drain() {
    // Configure explicit test budgets while retaining all other validated defaults.
    // 配置明确测试预算，同时保留其他已校验默认值。
    let settings = VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: DRAIN_WAIT_MS,
        ..VulcanPluginRuntimeSettings::default()
    };
    // Retain the isolated production fixture until actual cleanup is observed.
    // 保留隔离生产夹具至观测到实际清理。
    let fixture = Fixture::new(settings, 1000);
    // Preparation runs on the manager's supervisor, so its single blocking worker is the one held.
    // 准备在管理器监督器上运行，因此被占据的是其唯一阻塞工作线程。
    let supervisor = fixture
        .manager
        .state
        .supervisor
        .start_with_blocking_limit(1)
        .expect("start bounded supervisor");
    // Use the explicitly constructed observer executor for this test.
    // 为此测试使用显式构建的观测执行器。
    let runtime = executor();
    let (release, receive) = std::sync::mpsc::channel();
    runtime.block_on(async {
        // Occupy the sole preparation worker until the test explicitly releases it.
        // 占据唯一准备工作线程，直至测试显式释放。
        let blocker =
            supervisor.spawn_blocking(move || receive.recv().expect("release preparation worker"));
        // Retain the production manager for the exact invocation under test.
        // 为待测精确调用保留生产管理器。
        let manager = fixture.manager.clone();
        // Start actual prewarming while its blocking worker is unavailable.
        // 阻塞工作线程不可用时启动实际预热。
        let task = tokio::spawn(async move { manager.prewarm_plugin_runtime("hook-plugin").await });
        // Inspect the actual formal parent used by production dispatch.
        // 检查生产分发使用的实际正式父级。
        let owner = fixture.owner();
        // Bound test observation without treating elapsed time as completion evidence.
        // 限定测试观测时间，不将经过时间当作完成证据。
        let expires = Instant::now() + WAIT;
        // Keep the original Hook record to prove later cleanup does not replace it.
        // 保留原 Hook 记录，以证明后续清理未将其替换。
        let retained = loop {
            if let Some(retained) = owner
                .hooks
                .lock()
                .expect("registry")
                .get("hook-plugin")
                .cloned()
            {
                break retained;
            }
            assert!(Instant::now() < expires, "preparation was not registered");
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        };
        assert!(retained.lock().expect("evidence").preparing);
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("begin drain");
        // Check physical retirement separately from graceful shutdown diagnostics.
        // 将物理退役与优雅关闭诊断分开检查。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("bounded drain");
        assert_eq!(report.closed_hook_runtimes, 0);
        assert!(!report.failures.is_empty());
        release.send(()).expect("release");
        blocker.await.expect("worker completed");
        assert!(task.await.expect("prewarm supervisor").is_err());
        while !retained.lock().expect("evidence").retired {
            assert!(
                Instant::now() < expires,
                "cancelled preparation did not retire"
            );
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        assert!(retained.lock().expect("evidence").pool.is_none());
        assert!(fixture.events.values.lock().expect("events").is_empty());
        assert_eq!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("acknowledge")
                .closed_hook_runtimes,
            1
        );
    });
}

/// Core initialization failure remains specific even when its monitor has already started retirement.
/// 即使监控已开始退役，核心初始化失败仍保留具体原因。
#[test]
fn formal_hook_initialization_error_is_not_hidden_by_closing() {
    // No execution domain exists until the first production Hook call.
    // 首次生产 Hook 调用之前不存在执行域。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default(), 1000);
    // Publish valid Lua text that fails during actual module initialization.
    // 发布在实际模块初始化期间失败的有效 Lua 文本。
    let bundle = fixture
        .root
        .path()
        .join("bundled/hook-plugin/runtime/plugin.bundle.lua");
    std::fs::write(bundle, "error('hook-init-marker')").expect("initialization failure fixture");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish source");
    // The original error must be available through the normal production result.
    // 原错误必须可通过正常生产结果获取。
    let runtime = executor();
    runtime.block_on(async {
        let error = fixture
            .manager
            .invoke_hook(&fixture.hook(), fixture.invocation(json!({}), None))
            .await
            .expect_err("initialization fails");
        assert!(error.message.contains("hook-init-marker"), "{error}");
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("drain failed instance");
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(fixture.events.values.lock().expect("events").is_empty());
    });
}

/// Missing optional Hook shutdown does not invoke the tool finalizer, and parent close waits for physical retirement.
/// 缺少可选 Hook 关闭时不调用工具关闭器，且父级关闭等待物理退役。
#[test]
fn formal_hook_without_finalizer_participates_in_parent_shutdown() {
    // Change the authoritative manifest before creating any execution domain.
    // 在创建任何执行域之前修改权威清单。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default(), 1000);
    // Select the fixture's authoritative manifest file.
    // 选择夹具的权威清单文件。
    let manifest = fixture.root.path().join("bundled/hook-plugin/plugin.yaml");
    // Prepare the explicit manifest change before catalog publication.
    // 目录发布之前准备明确清单变更。
    let source = std::fs::read_to_string(&manifest)
        .expect("manifest source")
        .replace("  hook_shutdown_function: shutdown_hooks\n", "");
    std::fs::write(manifest, source).expect("remove optional finalizer");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("new declaration");
    // Use the explicitly constructed observer executor for this test.
    // 为此测试使用显式构建的观测执行器。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .invoke_hook(&fixture.hook(), fixture.invocation(json!({}), None))
            .await
            .expect("Hook business");
        // Inspect the actual formal parent used by production dispatch.
        // 检查生产分发使用的实际正式父级。
        let owner = fixture.owner();
        // Keep the original Hook record to prove later cleanup does not replace it.
        // 保留原 Hook 记录，以证明后续清理未将其替换。
        let retained = owner.hooks.lock().expect("registry")["hook-plugin"].clone();
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("request parent shutdown");
        // Bound test observation without treating elapsed time as completion evidence.
        // 限定测试观测时间，不将经过时间当作完成证据。
        let expires = Instant::now() + WAIT;
        while !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("physical parent status")
        {
            assert!(Instant::now() < expires, "parent did not physically close");
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        assert!(retained.lock().expect("evidence").retired);
        assert!(retained.lock().expect("evidence").finalization.is_none());
        assert!(fixture.events.values.lock().expect("events").is_empty());
    });
}

/// Supervisor scheduling delay consumes the original Hook deadline and cannot execute expired business.
/// 监督任务调度延迟消耗原 Hook 截止时间，不能执行已到期业务。
#[test]
fn formal_hook_supervisor_queue_cannot_renew_deadline() {
    // One short declared budget covers both supervisor scheduling and core execution.
    // 一个较短声明预算同时覆盖监督调度及核心执行。
    let timeout_ms = 100;
    // Prewarm removes initialization timing from the scheduling regression.
    // 预热从调度回归中排除初始化耗时。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default(), timeout_ms);
    // The caller's executor only observes; supervision is delayed on the manager's supervisor.
    // 调用方执行器只负责观测；监督在管理器监督器上被延迟。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("prewarm");
        // Keep declaration and envelope alive through manual first polling.
        // 手动首次轮询期间保持声明及信封存活。
        let hook = fixture.hook();
        // Hold every supervisor worker beyond the original budget before the hook is queued,
        // without affecting core workers.
        // 在 Hook 排队前让每个监督工作线程阻塞超过原预算，不影响核心工作线程。
        fixture
            .manager
            .state
            .supervisor
            .occupy_workers(Duration::from_millis(timeout_ms * 2))
            .expect("occupy supervisor");
        let mut invocation = Box::pin(
            fixture
                .manager
                .invoke_hook(&hook, fixture.invocation(json!({}), None)),
        );
        std::future::poll_fn(|context| {
            match std::future::Future::poll(invocation.as_mut(), context) {
                std::task::Poll::Pending => std::task::Poll::Ready(()),
                std::task::Poll::Ready(result) => {
                    panic!("Hook completed before its supervisor ran: {result:?}")
                }
            }
        })
        .await;
        assert!(
            invocation.await.is_err(),
            "expired supervisor must reject business"
        );
        // The next accepted call must still be the original instance's first actual business invocation.
        // 下一次接纳调用仍须为原实例的首次实际业务调用。
        let result = fixture
            .manager
            .invoke_hook(&hook, fixture.invocation(json!({}), None))
            .await
            .expect("next live call");
        assert_eq!(result.payload.expect("payload").value["count"], 1);
        assert!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("drain")
                .failures
                .is_empty()
        );
    });
}

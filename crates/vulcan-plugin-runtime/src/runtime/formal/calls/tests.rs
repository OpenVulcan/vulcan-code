//! Lifecycle production migration tests use real Lua initialization and native callbacks.
//! 生命周期生产迁移测试使用真实 Lua 初始化及原生回调。

use super::*;

mod candidates;
mod completion;
mod deactivation;
mod removal;
mod transient;
use crate::runtime::tests::{test_manager, write_hook_package};
use crate::{
    VulcanPluginHostServices, VulcanPluginLifecycleEvent as Event, VulcanPluginUiEventPublisher,
    VulcanPluginUiInvalidation,
};
use std::collections::BTreeSet;
use std::sync::Condvar;

/// Exercise the real private projection with typed terminal evidence, without claiming native execution or authority.
/// 使用类型化终态证据验证真实私有投影，不声称原生执行或运行时权威。
/// No parameters; verify owned results, missing evidence precedence, independent parsing and ordered dual errors.
/// 无参数；验证拥有结果、缺失证据优先级、独立解析及双错误顺序。
#[test]
fn formal_result_projection_preserves_owned_values_and_independent_failures() {
    use luaskills::runtime::embedded::capabilities::CapabilityCaller;
    use luaskills::runtime::embedded::{
        EffectState, EmbeddedError, ModuleOperationContext, OperationFinalization, OperationPhase,
    };

    // Build only projection inputs from the actual public snapshot shape; no operation is registered or retired.
    // 仅依据实际公开快照形状构造投影输入；不登记或退役任何操作。
    let snapshot_for = |value, error, finalization| OperationSnapshot {
        finalization,
        context: OperationContext::Unbound,
        host_effects: vec![],
        operation_id: "projection-contract".to_owned(),
        phase: OperationPhase::Succeeded,
        cancellation_requested: false,
        effects: EffectState::NotApplicable,
        value,
        error,
    };
    // Keep a complete structured business value whose returned ownership must outlive its original string frame.
    // 保留完整结构化业务值，返回值的拥有权必须长于其原始字符串帧。
    let expected = json!({"nested":[{"payload":"x".repeat(8192)},null],"flag":true});
    // The ordinary path receives a string frame and produces the same complete business subtree.
    // 普通路径接收字符串帧并产出相同完整业务子树。
    let frame = Value::String(json!({"ok":true,"result":expected}).to_string());
    // This typed snapshot owns the frame independently from every projected result.
    // 此类型化快照独立于每个投影结果持有原始帧。
    let ordinary = snapshot_for(Some(frame.clone()), None, None);
    assert_eq!(
        project_result(&ordinary).expect("ordinary complete result"),
        expected
    );
    assert_eq!(ordinary.value.as_ref(), Some(&frame));

    // A successful closing payload is deliberately not a business string frame and must be discarded unchanged.
    // 成功关闭载荷刻意不是业务字符串帧，必须按原契约直接丢弃。
    let mut finalized = snapshot_for(
        None,
        None,
        Some(OperationFinalization {
            export: "shutdown".to_owned(),
            business: OperationOutcome::Succeeded {
                value: frame.clone(),
            },
            business_effect_count: 0,
            outcome: Some(OperationOutcome::Succeeded {
                value: json!({"closing_payload":[null,1,"unused"]}),
            }),
        }),
    );
    assert_eq!(
        project_result(&finalized).expect("finalized complete result"),
        expected
    );
    // Mutate only the projection fixture; native lifecycle invariants remain covered by the integration tests.
    // 仅变更投影夹具；原生生命周期不变量继续由集成测试覆盖。
    let finalization = finalized.finalization.as_mut().expect("typed finalization");
    finalization.business = OperationOutcome::Succeeded {
        value: Value::Bool(false),
    };
    finalization.outcome = None;
    assert_eq!(
        project_result(&finalized)
            .expect_err("missing closing evidence precedes parsing")
            .message,
        "Terminal finalization has no outcome."
    );

    // Preserve exact structured core diagnostics while combining them after business parsing.
    // 在业务解析之后合并错误，同时保留精确结构化核心诊断。
    let closing_error = EmbeddedError::new(EmbeddedErrorCode::Internal, "closing-contract-marker");
    finalized
        .finalization
        .as_mut()
        .expect("typed finalization")
        .outcome = Some(OperationOutcome::Failed {
        error: closing_error.clone(),
    });
    assert_eq!(
        project_result(&finalized)
            .expect_err("closing failure cannot skip business parsing")
            .message,
        format!("Vulcan Plugin runtime eval returned no JSON frame.; {closing_error}")
    );
    // Core business and closing failures retain their original order, independently from frame decoding.
    // 核心业务及关闭失败保持原顺序，独立于帧解码。
    let business_error =
        EmbeddedError::new(EmbeddedErrorCode::Internal, "business-contract-marker");
    finalized
        .finalization
        .as_mut()
        .expect("typed finalization")
        .business = OperationOutcome::Failed {
        error: business_error.clone(),
    };
    assert_eq!(
        project_result(&finalized)
            .expect_err("both original stage errors")
            .message,
        format!("{business_error}; {closing_error}")
    );
    // Missing or contradictory ordinary evidence cannot be interpreted as JSON null or a successful frame.
    // 缺失或矛盾的普通证据不能被解释为JSON空值或成功帧。
    for inconsistent in [
        snapshot_for(None, None, None),
        snapshot_for(Some(frame), Some(business_error), None),
    ] {
        assert_eq!(
            project_result(&inconsistent)
                .expect_err("inconsistent ordinary evidence")
                .message,
            "Terminal operation has inconsistent result evidence."
        );
    }

    // Only an explicit typed prewarm context bypasses business-frame parsing and returns an owned native value.
    // 仅明确类型化预热上下文跳过业务帧解析，并返回拥有的原生值。
    let mut prewarm = snapshot_for(Some(expected.clone()), None, None);
    prewarm.context = OperationContext::Module(Box::new(ModuleOperationContext {
        prewarm: true,
        finalization_instance_id: None,
        pool_id: "projection-contract-pool".to_owned(),
        caller: CapabilityCaller {
            runtime_id: "projection-contract-runtime".to_owned(),
            plugin_id: "projection-contract-plugin".to_owned(),
            package_generation: "projection-contract-generation".to_owned(),
            execution_revision: "projection-contract-revision".to_owned(),
            security_partition: "projection-contract-partition".to_owned(),
            operation_id: prewarm.operation_id.clone(),
            request_id: None,
            session_id: None,
            workspace_root: None,
        },
        capability_revision: "projection-contract-capabilities".to_owned(),
        export: None,
    }));
    // Dropping the original snapshot proves the public return contract still owns its complete value.
    // 丢弃原始快照，证明公开返回契约仍拥有其完整值。
    let owned = project_result(&prewarm).expect("native prewarm value");
    drop(prewarm);
    assert_eq!(owned, expected);
}

/// Bounded test observation budget, shared by all controlled waits.
/// 有界测试观测预算，由全部受控等待共享。
const WAIT: Duration = Duration::from_secs(10);
/// Finite production drain observation while native work deliberately remains blocked.
/// 原生工作刻意保持阻塞时使用的有限生产排空观测预算。
const DRAIN_MS: u64 = 50;

/// A real publisher can stop inside a callback without falsely claiming cancellation.
/// 真实发布器可停在回调内，不会错误声明已取消。
#[derive(Default)]
struct Events {
    /// Actual callback entry precedes ownership assertions.
    /// 真实回调入场先于所有权断言。
    entered: AtomicBool,
    /// Exact release condition remains independent of any observer task.
    /// 精确释放条件独立于任何观测任务。
    released: Mutex<bool>,
    /// Wake the actual host worker after assertions complete.
    /// 断言完成后唤醒真实宿主工作者。
    changed: Condvar,
    /// Committed effects distinguish fresh state, replay and incorrect finalizers.
    /// 已提交副作用区分全新状态、重放与错误关闭器。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}
impl Events {
    /// Release the real worker, including assertion-unwind cleanup; no result is fabricated.
    /// 释放真实工作者，包含断言展开清理；不伪造结果。
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.changed.notify_all();
    }
}
impl VulcanPluginUiEventPublisher for Events {
    /// Commit the exact event after an optional native wait; return synchronization errors.
    /// 可选原生等待后提交精确事件；返回同步错误。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "enable" {
            self.entered.store(true, Ordering::Release);
            // The release condition belongs to the actual native worker.
            // 释放条件属于真实原生工作者。
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

/// Own package bytes and a formal manager across original and replacement executors.
/// 跨原始与替代执行器持有包字节及正式管理器。
struct Fixture {
    /// The public lifecycle API is the tested production entry.
    /// 公开生命周期 API 为被测生产入口。
    manager: VulcanPluginRuntimeManager,
    /// The real host service records side effects and controls one callback.
    /// 真实宿主服务记录副作用并控制一个回调。
    events: Arc<Events>,
    /// Files outlive all native work and retained descriptors.
    /// 文件比全部原生工作及保留描述符存活更久。
    _root: tempfile::TempDir,
}
impl Drop for Fixture {
    /// Unblock native work before fixture fields drop, even when a test assertion fails.
    /// 即使测试断言失败，也在夹具字段销毁前解除原生工作阻塞。
    fn drop(&mut self) {
        self.events.release();
    }
}
impl Fixture {
    /// Build a disabled real plugin with all declared lifecycle exports and bounded history.
    /// 构建声明全部生命周期导出且历史容量有界的真实停用插件。
    /// Returns a production manager attached to one formal parent.
    /// 返回附加到单个正式父级的生产管理器。
    fn new() -> Self {
        // Own the isolated filesystem used by this real runtime fixture.
        // 持有此真实运行时夹具使用的隔离文件系统。
        let root = tempfile::tempdir().expect("lifecycle fixture directory");
        // Use the production catalog's bundled-package layout.
        // 使用生产目录的内建包布局。
        let bundled = root.path().join("bundled");
        write_hook_package(&bundled);
        // Modify only the exact fixture package before catalog publication.
        // 仅在目录发布前修改精确夹具包。
        let package = bundled.join("hook-plugin");
        // Resolve the actual host target for a declared, non-executed binary.
        // 为已声明且不执行的二进制解析真实宿主目标。
        let target =
            crate::runtime::process::current_plugin_target_triple().expect("actual target");
        std::fs::write(package.join("fixture.bin"), b"never executed")
            .expect("resolver-only binary");
        // Extend the known fixture manifest before publishing its catalog generation.
        // 发布目录代次之前扩展已知夹具清单。
        let manifest = std::fs::read_to_string(package.join("plugin.yaml")).expect("original manifest")
            .replace("execution:\n", &format!("permissions: [process_binary, ui_host_context]\nexecution:\n  native_binaries:\n    - id: fixture\n      allow_workspace_cwd: true\n      targets:\n        {target}: fixture.bin\n"))
            .replace("    disable: on_disable", "    install: on_lifecycle\n    enable: on_lifecycle\n    disable: on_lifecycle\n    uninstall: on_lifecycle");
        std::fs::write(package.join("plugin.yaml"), manifest).expect("lifecycle declarations");
        std::fs::write(package.join("runtime/plugin.bundle.lua"), r#"
-- Each lifecycle call owns a fresh module counter.
-- 每次生命周期调用持有全新模块计数器。
local M, count = {}, 0
-- Ordinary exports exist but cannot act as lifecycle shutdown.
-- 普通导出存在，但不能充当生命周期关闭器。
-- No input is needed; return one successful version-one frame.
-- 不需要输入；返回一个成功的版本一帧。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- Wrong finalizer selection emits an observable failure instead of hiding it.
-- 错误关闭器选择发出可观测失败，不会被隐藏。
-- No input is needed; return no success.
-- 不需要输入；不返回成功。
function M.shutdown()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='wrong-finalizer',revision=0})
    error('lifecycle must not invoke tool or Hook shutdown')
end
M.shutdown_hooks, M.on_precheck, M.ui_action = M.shutdown, M.dispatch, M.dispatch
-- args is the original event envelope; verify native workspace authority and fresh state.
-- args 为原事件信封；校验原生工作区权威及全新状态。
-- Return a version-one success, or the intentional rollback error after one committed event.
-- 返回版本一成功，或在一次事件提交后返回刻意设置的回滚错误。
function M.on_lifecycle(args)
    assert(args.plugin_id == 'hook-plugin' and type(args.environment_root) == 'string')
    assert(args.event == 'install' or args.event == 'enable' or args.event == 'enable_rollback'
        or args.event == 'disable' or args.event == 'uninstall')
    assert(args.payload_json == nil and args.arguments == nil)
    assert(count == 0)
    count = count + 1
    -- Resolution uses the fixed lifecycle environment; no binary is executed.
    -- 解析使用固定生命周期环境；不执行二进制。
    local ok, reason = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
        {entry_id='fixture',mode='exec',options={cwd=args.environment_root}})
    assert(ok, tostring(reason))
    -- The parent directory is outside this exact workspace and the immutable package.
    -- 父目录位于此精确工作区及不可变包之外。
    local escaped = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
        {entry_id='fixture',mode='exec',options={cwd=args.environment_root .. '/..'}})
    assert(not escaped, 'lifecycle workspace must not authorize its parent')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event,revision=count})
    if args.event == 'enable_rollback' then return '{"ok":false,"error":"lifecycle-business-marker"}' end
    return '{"ok":true,"result":{}}'
end
return M
"#).expect("real lifecycle bundle");
        // Retain the original manager identity for the operation being observed.
        // 为被观测操作保留原管理器身份。
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(
                &BTreeSet::from(["hook-plugin".to_string()]),
                &BTreeSet::new(),
            )
            .expect("disabled catalog");
        // Retain the native callback recorder across observation and cleanup.
        // 跨观测及清理保留原生回调记录器。
        let events = Arc::new(Events::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(events.clone()),
            })
            .expect("actual native service");
        // Retention and queue bounds derive from one local capacity, below the invocation count.
        // 保留及队列边界派生自同一本地容量，低于调用次数。
        let capacity = 8;
        // Apply explicit parent history and drain budgets to the real runtime.
        // 将明确的父级历史及排空预算应用于真实运行时。
        let settings = VulcanPluginRuntimeSettings {
            shutdown_timeout_ms: DRAIN_MS,
            parent: luaskills::runtime::embedded::EmbeddedRuntimeConfig {
                max_operations: capacity,
                max_queued_calls: capacity,
                ..VulcanPluginRuntimeSettings::default().parent
            },
            ..VulcanPluginRuntimeSettings::default()
        };
        manager
            .initialize_formal_runtime(settings)
            .expect("formal parent");
        Self {
            manager,
            events,
            _root: root,
        }
    }

    /// Return the exact attached parent without creating another runtime.
    /// 返回精确已附加父级，不创建另一个运行时。
    fn owner(&self) -> Arc<ManagedFormalRuntime> {
        self.manager
            .formal_runtime_owner()
            .expect("parent query")
            .expect("attached parent")
    }

    /// Wait for actual native entry within the shared test deadline.
    /// 在共享测试截止时间内等待真实原生入场。
    async fn entered(&self) {
        tokio::time::timeout(WAIT, async {
            while !self.events.entered.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual lifecycle callback entered");
    }

    /// Return the exact single registered lifecycle record, including before core submission.
    /// 返回精确的单个已登记生命周期记录，包含核心提交之前。
    async fn admitted(&self) -> Arc<FormalCall> {
        tokio::time::timeout(WAIT, async {
            loop {
                // Inspect the original attached formal parent.
                // 检查原已附加正式父级。
                let owner = self.owner();
                {
                    // Hold the shared call registry while inspecting or changing retained ownership.
                    // 检查或变更保留所有权期间持有共享调用注册表。
                    let calls = owner.calls.lock().expect("call registry");
                    if let Some(call) = calls.values().next() {
                        return call.clone();
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("lifecycle owner admitted")
    }

    /// Observe actual empty ownership after acknowledged completion; return no inferred cleanup.
    /// 已确认完成后观测真实空所有权；不返回推测清理结果。
    async fn empty(&self) {
        tokio::time::timeout(WAIT, async {
            while !self.owner().calls.lock().expect("call registry").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("physically completed lifecycle history reclaimed");
        assert_eq!(
            self.manager
                .active_session_count("hook-plugin")
                .expect("actual live reservation count"),
            0
        );
        assert_eq!(
            self.manager
                .formal_runtime_status()
                .expect("actual core usage")
                .expect("attached parent")
                .resources
                .resident,
            0
        );
    }
}

/// Build a deterministic observer whose one blocking slot can be occupied explicitly.
/// 构建单个阻塞槽位可被显式占用的确定性观测执行器。
fn executor() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .expect("lifecycle executor")
}

/// All five event scopes retain their original envelope and fresh state without extra finalizers.
/// 全部五种事件作用域保留原信封及全新状态，不附加错误关闭器。
#[test]
fn formal_lifecycle_routes_preserve_scopes_and_reclaim_results() {
    // Own the production manager, package bytes and native observer for this scenario.
    // 为此场景持有生产管理器、包字节及原生观测者。
    let fixture = Fixture::new();
    fixture.events.release();
    // Use a deterministic observer executor independent of core workers.
    // 使用独立于核心工作者的确定性观测执行器。
    let runtime = executor();
    runtime.block_on(async {
        // Install and uninstall run while disabled; enable and compensation run while enabling.
        // 安装及卸载在停用时运行；启用及补偿在启用中运行。
        for event in [Event::Install, Event::Uninstall] {
            fixture
                .manager
                .invoke_lifecycle("hook-plugin", event)
                .await
                .expect("disabled callback");
        }
        assert!(
            fixture
                .manager
                .invoke_lifecycle("hook-plugin", Event::Enable)
                .await
                .is_err()
        );
        fixture
            .manager
            .begin_enabling("hook-plugin")
            .expect("enabling scope");
        fixture
            .manager
            .invoke_lifecycle("hook-plugin", Event::Enable)
            .await
            .expect("enable callback");
        // More failures than retained-history capacity must remain consumable after real cleanup.
        // 超过保留历史容量的失败次数必须在真实清理后仍可继续消费。
        for _ in 0..20 {
            // Preserve the actual failure returned by the public lifecycle route.
            // 保留公开生命周期路由返回的真实失败。
            let error = fixture
                .manager
                .invoke_lifecycle("hook-plugin", Event::EnableRollback)
                .await
                .expect_err("intentional business failure");
            assert!(
                error.message.contains("lifecycle-business-marker"),
                "{error}"
            );
            fixture.empty().await;
        }
        fixture
            .manager
            .finish_lifecycle_transition("hook-plugin", &BTreeSet::new(), &BTreeSet::new(), vec![])
            .expect("enabled catalog");
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("draining scope");
        fixture
            .manager
            .invoke_lifecycle("hook-plugin", Event::Disable)
            .await
            .expect("disable callback");
        fixture.empty().await;
        // Inspect committed native effects after actual completion.
        // 在真实完成后检查已提交原生副作用。
        let values = fixture.events.values.lock().expect("committed events");
        assert_eq!(values.len(), 24);
        assert!(
            values
                .iter()
                .all(|event| event.plugin_id == "hook-plugin" && event.revision == 1)
        );
        assert!(!values.iter().any(|event| event.topic == "wrong-finalizer"));
    });
    fixture
        .manager
        .request_formal_runtime_close()
        .expect("parent close");
    runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            while !fixture
                .manager
                .poll_formal_runtime_closed()
                .expect("real parent status")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("parent actually closed");
    });
}

/// Waiter or executor loss cannot release a still-running lifecycle callback or impersonate controller ownership.
/// 等待者或执行器丢失不能释放仍在运行的生命周期回调，也不能冒充控制器归属。
#[test]
fn formal_lifecycle_interruption_retains_actual_identity_and_capacity() {
    for stop_executor in [false, true] {
        // Own the production manager, package bytes and native observer for this scenario.
        // 为此场景持有生产管理器、包字节及原生观测者。
        let fixture = Fixture::new();
        fixture
            .manager
            .begin_enabling("hook-plugin")
            .expect("begin enabling");
        // Use a deterministic observer executor independent of core workers.
        // 使用独立于核心工作者的确定性观测执行器。
        let mut runtime = executor();
        // Retain the original manager identity for the operation being observed.
        // 为被观测操作保留原管理器身份。
        let manager = fixture.manager.clone();
        // Keep the invocation observer separate from the actual registered owner.
        // 将调用观测者与实际已登记所有者分离。
        let waiter = runtime
            .spawn(async move { manager.invoke_lifecycle("hook-plugin", Event::Enable).await });
        runtime.block_on(fixture.entered());
        // Retain the original host identity across cancellation or executor loss.
        // 跨取消或执行器丢失保留原宿主身份。
        let original = runtime.block_on(fixture.admitted());
        // Capture the actual core-issued identity before any interruption.
        // 在任何中断之前捕获核心真实签发的身份。
        let operation = original
            .lock()
            .expect("original state")
            .operation
            .clone()
            .expect("actual operation");
        assert!(
            fixture
                .manager
                .formal_calls("")
                .expect("empty controller selector")
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .formal_call_status(&original.id, "")
                .is_err()
        );
        if stop_executor {
            // Stop the original observer executor while keeping the same manager and core.
            // 停止原观测执行器，同时保留相同管理器及核心。
            let previous = std::mem::replace(&mut runtime, executor());
            previous.shutdown_timeout(WAIT);
        } else {
            waiter.abort();
            assert!(
                runtime
                    .block_on(waiter)
                    .expect_err("waiter aborted")
                    .is_cancelled()
            );
        }
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("close requested during native work");
        // Observe bounded drain evidence without assuming physical completion.
        // 观测有界排空证据，不假定物理完成。
        let report = runtime
            .block_on(fixture.manager.drain_plugin_sessions("hook-plugin"))
            .expect("bounded drain");
        assert!(
            !report.failures.is_empty(),
            "native work must still own capacity"
        );
        assert_eq!(
            fixture
                .manager
                .active_session_count("hook-plugin")
                .expect("actual live reservation count"),
            1
        );
        assert!(!original.lock().expect("retained state").retired);
        assert_eq!(
            original
                .lock()
                .expect("original identity")
                .operation
                .as_ref()
                .expect("operation")
                .id(),
            operation.id()
        );
        assert!(
            !fixture
                .manager
                .poll_formal_runtime_closed()
                .expect("parent pending")
        );
        fixture.events.release();
        runtime.block_on(async {
            // Resume the same cleanup, then allow actual native completion and pool retirement.
            // 恢复同一次清理，再等待真实原生完成及池退役。
            let _ = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("recovery drain");
            fixture.empty().await;
        });
        assert!(original.lock().expect("physical retirement").retired);
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("one committed callback")
                .len(),
            1
        );
    }
}

/// Preparation delay cannot renew the deadline or bypass an intervening catalog generation change.
/// 准备延迟不能续期截止时间，也不能绕过期间发生的目录代次变更。
#[test]
fn formal_lifecycle_preparation_preserves_deadline_and_generation() {
    for change_generation in [false, true] {
        // Own the production manager, package bytes and native observer for this scenario.
        // 为此场景持有生产管理器、包字节及原生观测者。
        let fixture = Fixture::new();
        fixture
            .manager
            .begin_enabling("hook-plugin")
            .expect("begin enabling");
        // Preparation runs on the manager's supervisor, so its single blocking worker is the one held.
        // 准备在管理器监督器上运行，因此被占据的是其唯一阻塞工作线程。
        let supervisor = fixture
            .manager
            .state
            .supervisor
            .start_with_blocking_limit(1)
            .expect("start bounded supervisor");
        // Use a deterministic observer executor independent of core workers.
        // 使用独立于核心工作者的确定性观测执行器。
        let runtime = executor();
        runtime.block_on(async {
            // Occupy only Tokio package preparation; the core has not been allowed to initialize.
            // 仅占用 Tokio 包准备；尚未允许核心初始化。
            let events = fixture.events.clone();
            // Occupy the sole preparation worker until the test releases the condition.
            // 占用唯一准备工作者，直至测试释放条件。
            let blocked = supervisor.spawn_blocking(move || {
                events.publish(VulcanPluginUiInvalidation {
                    plugin_id: "blocker".into(),
                    topic: "enable".into(),
                    revision: 0,
                })
            });
            fixture.entered().await;
            // Retain the original manager identity for the operation being observed.
            // 为被观测操作保留原管理器身份。
            let manager = fixture.manager.clone();
            // Freeze the exact catalog generation supplied to lifecycle admission.
            // 冻结提供给生命周期入场的精确目录代次。
            let plugin = manager.snapshot().plugins["hook-plugin"].clone();
            // Freeze one deadline before dispatch; queue delay cannot extend it.
            // 派发前冻结单个截止时间；队列延迟不能延长它。
            let deadline = Instant::now()
                + if change_generation {
                    WAIT
                } else {
                    Duration::from_millis(DRAIN_MS)
                };
            // Keep the invocation observer separate from the actual registered owner.
            // 将调用观测者与实际已登记所有者分离。
            let waiter = tokio::spawn(async move {
                manager
                    .invoke_formal_lifecycle(
                        plugin,
                        Event::Enable,
                        "on_lifecycle".into(),
                        json!({"event":"enable","plugin_id":"hook-plugin"}),
                        deadline,
                    )
                    .await
            });
            // Retain the original host identity across cancellation or executor loss.
            // 跨取消或执行器丢失保留原宿主身份。
            let original = fixture.admitted().await;
            assert!(original.lock().expect("before worker").operation.is_none());
            assert!(
                !fixture.manager.state.plugin_runtime_root.exists(),
                "environment work must wait behind registered preparation"
            );
            if change_generation {
                fixture
                    .manager
                    .finish_lifecycle_transition(
                        "hook-plugin",
                        &BTreeSet::new(),
                        &BTreeSet::new(),
                        vec![],
                    )
                    .expect("publish newer enabled generation");
            } else {
                tokio::time::sleep_until(deadline.into()).await;
            }
            fixture.events.release();
            blocked
                .await
                .expect("blocker joined")
                .expect("blocker released");
            assert!(waiter.await.expect("lifecycle result").is_err());
            assert!(
                original
                    .lock()
                    .expect("no business submitted")
                    .operation
                    .is_none()
            );
            fixture.empty().await;
            assert_eq!(
                fixture
                    .events
                    .values
                    .lock()
                    .expect("only blocker effect")
                    .len(),
                1
            );
        });
    }
}

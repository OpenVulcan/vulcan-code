//! Production UI Action ownership remains separate from waiter and executor lifetime.
//! 生产 UI Action 所有权独立于等待者及执行器寿命。

use super::*;
use crate::{VulcanPluginActionCaller, VulcanPluginActionIdentity, VulcanPluginActionRequest};

impl Fixture {
    /// Create an actual UI bundle with timeout and optional blocking initialization; return the fixture.
    /// 使用 timeout 及可选阻塞初始化创建真实 UI 包；返回夹具。
    fn ui(timeout: u64, block_initialization: bool, settings: VulcanPluginRuntimeSettings) -> Self {
        // The Hook fixture provides the same declared native permissions and real host recorder.
        // Hook 夹具提供相同已声明原生权限及真实宿主记录器。
        let fixture = Self::new(settings, timeout);
        // Extend the exact bundle before any instance has been admitted.
        // 在任何实例入场之前扩展精确包。
        let bundle = fixture
            .root
            .path()
            .join("bundled/hook-plugin/runtime/plugin.bundle.lua");
        // Replace the known UI alias, retaining all declared exports.
        // 替换已知 UI 别名，保留全部声明导出。
        let source = std::fs::read_to_string(&bundle).expect("read original fixture");
        assert!(source.contains(
            "M.shutdown, M.ui_action, M.on_disable = M.dispatch, M.dispatch, M.dispatch"
        ));
        // Real business state and permissions are inspected through the production Action envelope.
        // 通过生产 Action 信封检查真实业务状态及权限。
        let source = source.replace(
            "M.shutdown, M.ui_action, M.on_disable = M.dispatch, M.dispatch, M.dispatch",
            r#"
-- A UI instance must never execute the tool finalizer.
-- UI 实例绝不能执行工具关闭器。
-- No arguments are required; any call emits an observable wrong-finalizer effect.
-- 不需要参数；任何调用都发出可观测的错误关闭器副作用。
function M.shutdown()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='wrong-tool-finalizer',revision=count})
    return '{"ok":true,"result":{}}'
end
-- The host supplies arguments; return persistent state and exact process authorization evidence.
-- 宿主提供 arguments；返回持久状态及精确进程授权证据。
function M.ui_action(args)
    -- Only the declared arguments object chooses business operations.
    -- 只有已声明 arguments 对象选择业务操作。
    local value = args.arguments
    assert(type(value) == 'table')
    count = count + 100
    if value.block then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='block',revision=count})
    end
    -- Optional resolution proves the fixed package workspace without executing a binary.
    -- 可选解析证明固定包工作区，不执行二进制。
    local allowed = nil
    if value.cwd then
        allowed = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
            {entry_id='fixture',mode=value.mode or 'exec',options={cwd=value.cwd}})
    end
    return vulcan.json.encode({ok=true,result={count=count,allowed=allowed}})
end
M.on_disable = M.dispatch
"#,
        );
        // Initialization is a real native effect, independently of later business admission.
        // 初始化属于真实原生副作用，独立于后续业务入场。
        let source = if block_initialization {
            format!(
                "vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='block',revision=0}})\n{source}"
            )
        } else {
            source
        };
        std::fs::write(bundle, source).expect("write UI fixture");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish exact UI generation");
        fixture
    }

    /// Build a UI request with arguments and trusted request workspace; persistent authority remains package-scoped.
    /// 使用 arguments 及可信请求工作区构造 UI 请求；持久权威仍限定于包。
    fn ui_request(&self, arguments: Value) -> VulcanPluginActionRequest {
        VulcanPluginActionRequest {
            plugin_id: "hook-plugin".into(),
            action_id: "ui-action".into(),
            arguments,
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: self.root.path().to_owned(),
            reuse_enabled_runtime: true,
        }
    }

    /// Return fresh caller ownership and the actual current UI generation identity.
    /// 返回新的调用方所有权及真实当前 UI 代次身份。
    fn ui_identity(&self) -> (VulcanPluginActionIdentity, String) {
        // Generation comes only from the currently published catalog.
        // 代次只来自当前已发布目录。
        let snapshot = self.manager.snapshot();
        (
            VulcanPluginActionIdentity {
                client_id: Uuid::new_v4().to_string(),
                invocation_id: Uuid::new_v4().to_string(),
            },
            format!(
                "{}:{}",
                snapshot.service_epoch, snapshot.plugins["hook-plugin"].generation
            ),
        )
    }
}

/// Actual initialization and business callbacks retain capacity after executor destruction and bounded drain.
/// 真实初始化及业务回调在执行器销毁和有界排空后保留容量。
#[test]
fn formal_ui_executor_loss_retains_initialization_and_running_ownership() {
    for initialization in [false, true] {
        // Cleanup must expire while the native callback is deliberately blocked.
        // 原生回调刻意阻塞时，清理必须到期。
        let fixture = Fixture::ui(
            1000,
            initialization,
            VulcanPluginRuntimeSettings {
                shutdown_timeout_ms: DRAIN_WAIT_MS,
                ..VulcanPluginRuntimeSettings::default()
            },
        );
        // Preserve exact diagnostic ownership across destruction of the first executor.
        // 跨首个执行器销毁保留精确诊断所有权。
        let (identity, incarnation) = fixture.ui_identity();
        // The observer executor does not own the core's physical callback worker.
        // 观测执行器不拥有核心物理回调工作线程。
        let runtime = executor();
        runtime.block_on(async {
            // Move cloned request authority into the real public UI entry.
            // 将克隆请求权威移入真实公开 UI 入口。
            let manager = fixture.manager.clone();
            // Freeze host-selected request authority before asynchronous execution.
            // 在异步执行前冻结宿主选择的请求权威。
            let request = fixture.ui_request(json!({"block": !initialization}));
            // Clone the authenticated caller identity into the spawned invocation.
            // 将已认证调用方身份克隆至派生调用。
            let caller = identity.clone();
            // Freeze the actual service epoch and catalog generation.
            // 冻结真实服务纪元及目录代次。
            let generation = incarnation.clone();
            tokio::spawn(
                async move { manager.invoke_ui_action(request, &generation, caller).await },
            );
            entered(&fixture.events).await;
        });
        // Preserve original core identity for later physical retirement assertions.
        // 保留原核心身份，供后续物理退役断言。
        let owner = fixture.owner();
        // Retain the original instance identity across interruption and cleanup.
        // 跨中断及清理保留原实例身份。
        let original = owner.ui_actions.lock().expect("UI registry")["hook-plugin"].clone();
        drop(runtime);
        // Abnormal supervision is pending, not proof of physical completion.
        // 异常监督退出仍待完成，不是物理完成证明。
        let status = fixture
            .manager
            .action_status("hook-plugin", &incarnation, &identity, false)
            .expect("owned status");
        assert!(!status.terminal);
        assert_eq!(status.phase, "cleanup_wait");
        assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
        if !initialization {
            assert!(original.operation_gate.try_lock().is_err());
        }
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("stop old generation");
        // Restart supervision on the same physical owner, without replaying business work.
        // 在同一物理所有者上重启监督，不重放业务。
        let recovery = executor();
        recovery.block_on(async {
            // Inspect physical retirement separately from logical execution outcomes.
            // 将物理退役与逻辑执行结果分开检查。
            let report = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("bounded drain");
            assert_eq!(report.closed_ui_action_runtimes, 0);
            assert!(!report.failures.is_empty());
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
            fixture.events.release();
            // Wait for actual monitor evidence instead of assuming release means completion.
            // 等待真实监控证据，不假定释放就代表完成。
            let expires = Instant::now() + WAIT;
            while !original.lock().expect("actual state").retired {
                assert!(
                    Instant::now() < expires,
                    "UI instance did not physically retire"
                );
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
            // Query the original invocation without replaying its business effects.
            // 查询原调用，不重放其业务副作用。
            let status = fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("final status");
            assert!(status.terminal);
            assert_eq!(status.phase, "supervisor_failed");
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_ok());
            // Inspect physical retirement separately from logical execution outcomes.
            // 将物理退役与逻辑执行结果分开检查。
            let report = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("acknowledge retirement");
            assert_eq!(report.closed_ui_action_runtimes, 1);
            assert!(report.failures.is_empty(), "{:?}", report.failures);
            assert!(
                fixture
                    .events
                    .values
                    .lock()
                    .expect("effects")
                    .iter()
                    .all(|event| event.topic == "block")
            );
            assert_eq!(fixture.events.values.lock().expect("effects").len(), 1);
        });
    }
}

/// Cancelling a read during real initialization retains capacity, but never executes the cancelled business.
/// 在真实初始化期间取消读取会保留容量，但绝不执行已取消业务。
#[test]
fn formal_ui_cancelled_initializer_retains_capacity_until_native_completion() {
    // The original initialization can complete successfully while its read observer withdraws.
    // 读取观察者撤回时，原初始化仍可成功完成。
    let fixture = Fixture::ui(1000, true, VulcanPluginRuntimeSettings::default());
    let (identity, incarnation) = fixture.ui_identity();
    // Own the controlled observer executor for this test phase.
    // 为此测试阶段拥有受控观察执行器。
    let runtime = executor();
    runtime.block_on(async {
        // Retain the production manager while dispatching the isolated call.
        // 分发隔离调用期间保留生产管理器。
        let manager = fixture.manager.clone();
        // Freeze host-selected request authority before asynchronous execution.
        // 在异步执行前冻结宿主选择的请求权威。
        let request = fixture.ui_request(json!({}));
        // Clone the authenticated caller identity into the spawned invocation.
        // 将已认证调用方身份克隆至派生调用。
        let caller = identity.clone();
        // Freeze the actual service epoch and catalog generation.
        // 冻结真实服务纪元及目录代次。
        let generation = incarnation.clone();
        // Observe the real public call without owning its physical execution.
        // 观测真实公开调用，不拥有其物理执行。
        let pending =
            tokio::spawn(
                async move { manager.invoke_ui_action(request, &generation, caller).await },
            );
        entered(&fixture.events).await;
        fixture
            .manager
            .action_status("hook-plugin", &incarnation, &identity, true)
            .expect("cancel original read");
        assert!(pending.await.expect("join original observer").is_err());
        assert!(
            !fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("active status")
                .terminal
        );
        assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
        fixture.events.release();
        // Admission becomes available only after original physical initialization finishes.
        // 只有原物理初始化完成之后，入场才重新可用。
        let expires = Instant::now() + WAIT;
        loop {
            // Query the original invocation without replaying its business effects.
            // 查询原调用，不重放其业务副作用。
            let status = fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("observe original");
            if status.terminal {
                assert_eq!(status.phase, "cancelled");
                break;
            }
            assert!(
                Instant::now() < expires,
                "cancelled initializer never retired"
            );
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        assert_eq!(
            fixture
                .manager
                .invoke_action(fixture.ui_request(json!({})))
                .await
                .expect("next real action")["count"],
            100
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("drain");
        assert!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("closed")
                .failures
                .is_empty()
        );
    });
}

/// Repeated UI calls preserve state, reclaim actual operations, and never expand the package workspace.
/// 重复 UI 调用保留状态、回收真实操作，并且绝不扩大包工作区。
#[test]
fn formal_ui_state_workspace_history_and_no_implicit_finalizer() {
    // Deliberately small operation history must not cap lifetime calls.
    // 刻意缩小操作历史，不能限制整个生命周期调用数量。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_operations = 8;
    settings.parent.max_queued_calls = settings.parent.max_operations;
    // Retain the isolated package and real native recorder through verification.
    // 跨验证保留隔离包及真实原生记录器。
    let fixture = Fixture::ui(1000, false, settings);
    // Own the controlled observer executor for this test phase.
    // 为此测试阶段拥有受控观察执行器。
    let runtime = executor();
    runtime.block_on(async {
        // Only the declared package and descendants are valid persistent UI process directories.
        // 只有已声明包及其后代才是有效持久 UI 进程目录。
        let package = fixture.root.path().join("bundled/hook-plugin");
        // Inspect the first real action result and its fixed workspace authority.
        // 检查首个真实 Action 结果及其固定工作区权威。
        let first = fixture
            .manager
            .invoke_action(fixture.ui_request(json!({"cwd":package})))
            .await
            .expect("package cwd");
        assert_eq!(first["allowed"], true);
        assert_eq!(first["count"], 100);
        // Inspect the denied sibling workspace without changing instance state ownership.
        // 检查被拒绝的相邻工作区，不改变实例状态归属。
        let second = fixture
            .manager
            .invoke_action(fixture.ui_request(json!({"cwd":fixture.root.path()})))
            .await
            .expect("denied cwd result");
        assert_eq!(second["allowed"], false);
        for number in 3..=20 {
            // Verify persistent state after each completed production call.
            // 每次生产调用完成后验证持久状态。
            let result = fixture
                .manager
                .invoke_action(fixture.ui_request(json!({})))
                .await
                .expect("long-lived UI");
            assert_eq!(result["count"], number * 100);
        }
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("parent close");
        // Real parent closure includes UI preparation, work and physical retirement.
        // 真实父级关闭包含 UI 准备、工作和物理退役。
        let expires = Instant::now() + WAIT;
        while !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("parent physical state")
        {
            assert!(Instant::now() < expires, "UI prevented real parent closure");
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        assert!(
            fixture
                .events
                .values
                .lock()
                .expect("finalizer effects")
                .is_empty()
        );
    });
}

/// A supervisor dropped before its first poll still completes its record and returns unused capacity.
/// 监督任务首次轮询前被丢弃时，仍完成记录并归还未使用容量。
#[test]
fn formal_ui_unpolled_supervisor_returns_unused_admission() {
    // Retain the isolated package and real native recorder through verification.
    // 跨验证保留隔离包及真实原生记录器。
    let fixture = Fixture::ui(1000, false, VulcanPluginRuntimeSettings::default());
    let (identity, incarnation) = fixture.ui_identity();
    // Own the controlled observer executor for this test phase.
    // 为此测试阶段拥有受控观察执行器。
    let runtime = executor();
    {
        // Enter without driving spawned tasks, then poll only the external admission once.
        // 进入执行器但不驱动派生任务，然后只轮询外部入场一次。
        let _entered = runtime.enter();
        // Freeze the declared export, arguments and host correlation together.
        // 共同冻结已声明导出、参数及宿主关联。
        let mut call = Box::pin(fixture.manager.invoke_ui_action(
            fixture.ui_request(json!({})),
            &incarnation,
            identity.clone(),
        ));
        // Poll only the external future to isolate admission from supervisor execution.
        // 仅轮询外部 Future，使入场与监督执行隔离。
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(call.as_mut(), &mut context).is_pending());
    }
    drop(runtime);
    // Query the original invocation without replaying its business effects.
    // 查询原调用，不重放其业务副作用。
    let status = fixture
        .manager
        .action_status("hook-plugin", &incarnation, &identity, false)
        .expect("unpolled status");
    assert!(status.terminal);
    assert_eq!(status.phase, "supervisor_failed");
    assert!(fixture.manager.admit_action("hook-plugin", 1).is_ok());
    assert!(
        fixture
            .owner()
            .ui_actions
            .lock()
            .expect("no preparation")
            .is_empty()
    );
}

/// A business deadline cannot release UI admission while its native callback is still executing.
/// 业务截止时间不能在原生回调仍执行时释放 UI 入场。
#[test]
fn formal_ui_deadline_retains_actual_work_and_then_reports_timeout() {
    // Use an explicit short business budget after preparing the original fixed instance.
    // 原固定实例准备完成后，使用明确较短的业务预算。
    let fixture = Fixture::ui(DRAIN_WAIT_MS, false, VulcanPluginRuntimeSettings::default());
    // Stable ownership makes late completion queryable without replay.
    // 稳定所有权使迟到完成可查询，无需重放。
    let (identity, incarnation) = fixture.ui_identity();
    // The controlled executor stays alive for both timeout and actual completion.
    // 受控执行器跨超时及真实完成保持存活。
    let runtime = executor();
    runtime.block_on(async {
        // Prepare with a separate test bound before testing the declared business deadline.
        // 测试已声明业务截止时间前，用独立测试上限完成准备。
        let original = fixture
            .manager
            .ensure_formal_instance(
                "hook-plugin",
                fixture.hook().plugin_generation,
                VulcanPluginPersistentRuntimeKind::UiAction,
                Instant::now() + WAIT,
            )
            .await
            .expect("prepare original UI instance");
        // Move exact caller ownership and request into the public entry.
        // 将精确调用方所有权及请求移入公开入口。
        let manager = fixture.manager.clone();
        // Freeze host-selected request authority before asynchronous execution.
        // 在异步执行前冻结宿主选择的请求权威。
        let request = fixture.ui_request(json!({"block":true}));
        // Clone the authenticated caller identity into the spawned invocation.
        // 将已认证调用方身份克隆至派生调用。
        let caller = identity.clone();
        // Freeze the actual service epoch and catalog generation.
        // 冻结真实服务纪元及目录代次。
        let generation = incarnation.clone();
        // Observe the real public call without owning its physical execution.
        // 观测真实公开调用，不拥有其物理执行。
        let pending =
            tokio::spawn(
                async move { manager.invoke_ui_action(request, &generation, caller).await },
            );
        entered(&fixture.events).await;
        assert!(pending.await.expect("join timed-out waiter").is_err());
        assert!(
            !fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("active timed-out action")
                .terminal
        );
        assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
        assert!(original.operation_gate.try_lock().is_err());
        fixture.events.release();
        // A release signal alone is insufficient; wait for recorded physical completion.
        // 仅释放信号还不足够；等待已记录物理完成。
        let expires = Instant::now() + WAIT;
        loop {
            // Query the original invocation without replaying its business effects.
            // 查询原调用，不重放其业务副作用。
            let status = fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("observe physical completion");
            if status.terminal {
                assert_eq!(status.phase, "timed_out");
                assert!(status.deadline_exceeded);
                assert!(status.stages_ms.contains_key("host_callback"));
                break;
            }
            assert!(
                Instant::now() < expires,
                "deadline action retained after actual completion"
            );
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        assert!(original.operation_gate.try_lock().is_ok());
        assert!(fixture.manager.admit_action("hook-plugin", 1).is_ok());
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("retire original generation");
        assert!(
            fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("finish physical drain")
                .failures
                .is_empty()
        );
    });
}

/// Revoking one process subtype or session mode cannot borrow another still-granted process permission.
/// 撤回进程子类型或会话模式时，不能借用另一仍获授权进程权限。
#[test]
fn formal_ui_process_subtype_and_session_permissions_are_current() {
    for permission in [
        crate::VulcanPluginPermission::ProcessBinary,
        crate::VulcanPluginPermission::ProcessSession,
    ] {
        // Freeze both broad process alternatives before the original instance opens.
        // 在原实例打开前冻结两种宽泛进程选择。
        let fixture = Fixture::ui(1000, false, VulcanPluginRuntimeSettings::default());
        // Only permission metadata is modified; declarations and package identities remain exact.
        // 仅修改权限元数据；声明及包身份保持精确。
        let mut snapshot = fixture.manager.snapshot().as_ref().clone();
        let plugin = snapshot
            .plugins
            .get_mut("hook-plugin")
            .expect("published plugin");
        let mut manifest = plugin
            .manifest
            .as_ref()
            .expect("original manifest")
            .as_ref()
            .clone();
        manifest.permissions.extend([
            crate::VulcanPluginPermission::ProcessNode,
            crate::VulcanPluginPermission::ProcessSession,
        ]);
        plugin.manifest = Some(Arc::new(manifest));
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("initial authorization") = Arc::new(snapshot);
        // Resolve only a fixture file, never launch the returned process specification.
        // 仅解析夹具文件，绝不启动返回的进程规格。
        let package = fixture.root.path().join("bundled/hook-plugin");
        let runtime = executor();
        runtime.block_on(async {
            let initial = fixture
                .manager
                .invoke_action(fixture.ui_request(json!({"cwd":package, "mode":"session"})))
                .await
                .expect("original process grant");
            assert_eq!(initial["allowed"], true);
            // Withdrawing current authority must not mutate the frozen module declarations.
            // 撤回当前权威不能修改已冻结模块声明。
            let mut snapshot = fixture.manager.snapshot().as_ref().clone();
            let plugin = snapshot
                .plugins
                .get_mut("hook-plugin")
                .expect("current plugin");
            let mut manifest = plugin
                .manifest
                .as_ref()
                .expect("current manifest")
                .as_ref()
                .clone();
            manifest
                .permissions
                .retain(|declared| *declared != permission);
            plugin.manifest = Some(Arc::new(manifest));
            *fixture
                .manager
                .state
                .snapshot
                .write()
                .expect("withdraw exact permission") = Arc::new(snapshot);
            // Binary withdrawal is tested with exec, session withdrawal with session.
            // 二进制撤回使用 exec 验证，会话撤回使用 session 验证。
            let mode = if permission == crate::VulcanPluginPermission::ProcessSession {
                "session"
            } else {
                "exec"
            };
            let denied = fixture
                .manager
                .invoke_action(fixture.ui_request(json!({"cwd":package, "mode":mode})))
                .await
                .expect("explicit process denial");
            assert_eq!(denied["allowed"], false);
            if permission == crate::VulcanPluginPermission::ProcessSession {
                // Revoking sessions alone must not remove an independently valid exec permission.
                // 仅撤回会话不能移除独立有效的 exec 权限。
                let allowed = fixture
                    .manager
                    .invoke_action(fixture.ui_request(json!({"cwd":package, "mode":"exec"})))
                    .await
                    .expect("retained exec grant");
                assert_eq!(allowed["allowed"], true);
            }
            fixture
                .manager
                .begin_draining("hook-plugin")
                .expect("retire fixture");
            assert!(
                fixture
                    .manager
                    .drain_plugin_sessions("hook-plugin")
                    .await
                    .expect("drain without replay")
                    .failures
                    .is_empty()
            );
        });
    }
}

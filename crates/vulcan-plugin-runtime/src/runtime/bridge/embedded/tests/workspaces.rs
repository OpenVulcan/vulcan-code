//! Actual workspace authorization across shared Lua state and callback lifetimes.
//! 跨共享 Lua 状态与回调寿命的真实工作区授权验证。

use super::*;
use luaskills::RuntimeRequestContext;
use luaskills::runtime::embedded::{HostEffectPhase, ModuleFinalizer};

/// Build a real binary-resolution fixture; return its catalog and runtime without executing a process.
/// 构造真实二进制解析夹具；返回其目录与运行时，不执行进程。
fn process_fixture() -> Fixture {
    // Keep package, declared workspace and request workspaces under one test owner.
    // 将包、声明工作区及请求工作区置于同一个测试所有者之下。
    let root = tempfile::tempdir().expect("workspace fixture");
    // Reuse the authoritative plugin fixture shape before extending its execution contract.
    // 扩展执行契约之前复用权威插件夹具形状。
    let bundled = root.path().join("bundled");
    write_hook_package(&bundled);
    // This file is resolved, never launched, by the process capability under test.
    // 待测进程能力只解析此文件，绝不启动它。
    let package = bundled.join("hook-plugin");
    fs::write(package.join("fixture.bin"), b"not an executable").expect("binary path fixture");
    // Target declarations follow the same host target resolution as real plugin processes.
    // 目标声明遵循与真实插件进程相同的宿主目标解析。
    let target = crate::runtime::process::current_plugin_target_triple().expect("supported target");
    // Replace the known execution header without duplicating YAML ownership.
    // 替换已知执行标题，不重复 YAML 归属。
    let manifest = fs::read_to_string(package.join("plugin.yaml"))
        .expect("read fixture manifest")
        .replace("execution:\n", &format!("permissions: [process_binary, ui_host_context]\nexecution:\n  native_binaries:\n    - id: fixture\n      allow_workspace_cwd: true\n      targets:\n        {target}: fixture.bin\n"));
    fs::write(package.join("plugin.yaml"), manifest).expect("publish process declaration");
    Fixture::from_catalog(root, bundled, "hook-plugin")
}

/// Build one host-owned request for `pool`, with exact `request_id` and business `arguments`.
/// 为 `pool` 构造带精确 `request_id` 和业务 `arguments` 的宿主请求。
/// Return a call whose correlation can be frozen by native admission.
/// 返回可由原生入场冻结关联的调用。
fn call(pool: &str, request_id: &str, arguments: Value) -> EmbeddedCall {
    EmbeddedCall {
        pool_id: pool.to_owned(),
        export: "call".into(),
        arguments,
        context: LuaInvocationContext {
            request_context: Some(RuntimeRequestContext {
                request_id: Some(request_id.to_owned()),
                ..RuntimeRequestContext::default()
            }),
            ..LuaInvocationContext::default()
        },
    }
}

/// Verify two workspaces share one Lua state while forged paths and contexts cannot replace authority.
/// 验证两个工作区共享同一 Lua 状态，伪造路径及上下文不能替换权威。
#[test]
fn native_workspace_requests_preserve_state_and_independent_closing() {
    for reuse in [InstanceReuse::Reusable, InstanceReuse::Session] {
        // Each reuse policy gets independent physical resources and exact native evidence.
        // 每种复用策略取得独立物理资源与精确原生证据。
        let fixture = process_fixture();
        // Fixed initialization/closing authority is separate from both business workspaces.
        // 固定初始化／关闭权威独立于两个业务工作区。
        let fixed = fixture.root.path().join("fixed");
        // First caller's workspace cannot authorize the second caller's sibling path.
        // 第一个调用方的工作区不能授权第二个调用方的相邻路径。
        let first = fixture.root.path().join("first");
        // The second host workspace must reuse the same Lua counter.
        // 第二个宿主工作区必须复用同一个 Lua 计数器。
        let second = fixture.root.path().join("second");
        for directory in [&fixed, &first, &second] {
            fs::create_dir(directory).expect("create exact workspace");
        }
        // Initialization exercises the fast native callback race during request admission.
        // 初始化覆盖请求入场期间快速原生回调竞争。
        let mut module = fixture.definition(r#"
local initialized = vulcan.host.call('vulcan.plugin.ui.invalidate',{topic='initialized',revision=1})
assert(initialized.ok)
local count = 0
return {
 call=function(a)
  count=count+1
  vulcan.context.request.request_id='lua-forged'
  local resolved=vulcan.host.call('vulcan.plugin.process.resolve',{entry_id='fixture',mode='exec',options={cwd=a.cwd},workspace_root=a.cwd,request_id='argument-forged'})
  return {count=count,resolved=resolved}
 end,
 shutdown=function(a)
  local resolved=vulcan.host.call('vulcan.plugin.process.resolve',{entry_id='fixture',mode='exec',options={cwd=a.cwd}})
  assert(resolved.ok)
  return resolved.value
 end
}
"#);
        module.workspace_root = Some(
            fixed
                .canonicalize()
                .expect("fixed workspace")
                .to_string_lossy()
                .into_owned(),
        );
        module.exports.push(ModuleExport {
            name: "shutdown".into(),
            input_schema: json!(true),
            output_schema: json!(true),
        });
        module.finalizer = Some(ModuleFinalizer {
            export: "shutdown".into(),
            arguments: json!({"cwd":fixed}),
            timeout_ms: WAIT.as_millis().try_into().expect("closing budget"),
        });
        // Preserve serial reuse semantics while varying only the tested instance policy.
        // 保留串行复用语义，仅改变待测实例策略。
        let mut config = policy();
        config.reuse = reuse;
        // Core ownership pins the original fixed module declaration throughout the test.
        // 核心所有权在整个测试中固定原模块声明。
        let pool = fixture
            .host
            .register_pool(module, config, VulcanPluginBridgeLeaseScope::Enabled)
            .expect("register workspace module");
        // Session opening has no business correlation and therefore uses fixed initialization authority.
        // 会话开启没有业务关联，因此使用固定初始化权威。
        let session = if reuse == InstanceReuse::Session {
            // Wait on the actual opening operation before publishing business requests.
            // 发布业务请求之前等待实际开启操作。
            let opening = fixture
                .host
                .runtime()
                .open_session(&pool, WAIT)
                .expect("open fixed session");
            assert_eq!(
                opening
                    .operation
                    .wait(WAIT)
                    .expect("actual initialization")
                    .phase,
                OperationPhase::Succeeded
            );
            Some(opening.session_id)
        } else {
            None
        };
        for (index, (workspace, requested, allowed)) in [
            (&first, &first, true),
            (&second, &second, true),
            (&first, &second, false),
        ]
        .into_iter()
        .enumerate()
        {
            // Explicit unique correlations remain distinct from all Lua-supplied values.
            // 显式唯一关联独立于全部 Lua 提供值。
            let request = call(&pool, &format!("host-{index}"), json!({"cwd":requested}));
            // Dispatch through the policy's real core queue using the same host workspace registry.
            // 通过策略的真实核心队列分发，使用同一宿主工作区注册表。
            let operation = match &session {
                Some(session) => fixture
                    .host
                    .submit_session_in_workspace(session, request, workspace, WAIT),
                None => fixture.host.submit_in_workspace(request, workspace, WAIT),
            }
            .expect("admit exact request workspace");
            // A denied process path is a business result and must not reset the shared Lua state.
            // 被拒绝的进程路径是业务结果，不得重置共享 Lua 状态。
            let result = operation.wait(WAIT).expect("actual request completion");
            assert_eq!(result.phase, OperationPhase::Succeeded, "{result:?}");
            // Inspect actual native resolver output rather than a locally calculated workspace.
            // 检查真实原生解析器输出，不使用本地计算工作区。
            let value = result.value.expect("Lua result");
            assert_eq!(value["count"], json!(index + 1));
            assert_eq!(value["resolved"]["ok"], allowed);
            if allowed {
                assert_eq!(
                    value["resolved"]["value"]["cwd"],
                    vulcan_runtime_assets::native_host_path_text_from_path(
                        &requested.canonicalize().expect("expected workspace")
                    )
                );
            }
            assert!(
                result
                    .host_effects
                    .iter()
                    .all(|effect| effect.caller.request_id.as_deref()
                        == Some(format!("host-{index}").as_str()))
            );
            assert_eq!(
                fixture
                    .host
                    .reap_workspace_requests()
                    .expect("reap completed request"),
                1
            );
        }
        assert_eq!(
            fixture
                .services
                .events
                .lock()
                .expect("initialization events")
                .len(),
            1
        );
        fixture.close();
        // Closing is independently scheduled and must not reuse any previously removed business scope.
        // 关闭独立调度，不得复用任何此前已移除的业务作用域。
        let mut saw_closing = false;
        for id in fixture
            .host
            .runtime()
            .list_operations(Some(&pool), None, 16)
            .expect("retained operations")
            .operation_ids
        {
            // Read the real closing evidence by its stable operation identity.
            // 按稳定操作身份读取真实关闭证据。
            let record = fixture
                .host
                .runtime()
                .operation(&id)
                .expect("retained operation")
                .snapshot()
                .expect("retained result");
            if let Some(finalization) = record.finalization.as_ref()
                && record.host_effects.iter().any(|effect| {
                    effect.capability_name == VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL
                        && effect.caller.request_id.is_none()
                })
            {
                assert_eq!(record.phase, OperationPhase::Succeeded, "{record:?}");
                // The independent closing outcome, not the business baseline, proves authorization succeeded.
                // 独立关闭结果证明授权成功，不能用业务基线代替。
                let closed = finalization
                    .outcome
                    .as_ref()
                    .expect("actual closing outcome")
                    .result()
                    .expect("closing callback succeeded");
                assert_eq!(
                    closed["cwd"],
                    vulcan_runtime_assets::native_host_path_text_from_path(
                        &fixed.canonicalize().expect("fixed closing workspace")
                    )
                );
                saw_closing = true;
            }
        }
        assert!(saw_closing, "independent closing callback evidence missing");
    }
}

/// Verify active scopes survive observer loss and cancellation, and cannot be borrowed by another operation.
/// 验证活动作用域在观察者离开及取消后仍保留，且不能由另一操作借用。
#[test]
fn native_workspace_requests_retain_cancelled_owner_and_reject_borrowing() {
    // A service lookup gate makes the real native callback remain physically active.
    // 服务查询门使真实原生回调保持物理活动。
    let fixture = Fixture::new();
    // Two independent pools allow a foreign operation to reach admission while the first remains blocked.
    // 两个独立池允许外来操作在首个操作阻塞期间到达入场。
    let first = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // The second pool has identical permissions but a different execution revision and operation.
    // 第二个池具有相同权限，但执行修订与操作不同。
    let second = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Keep the service closed while observing actual ownership through the core.
    // 通过核心观察实际所有权期间保持服务关闭。
    let gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("pause native lookup");
    // The host-owned registry publishes the exact operation before allowing its callback through.
    // 宿主持有注册表在允许回调通过之前发布精确操作。
    let original = fixture
        .host
        .submit_in_workspace(
            call(&first, "retained", json!({})),
            fixture.root.path(),
            WAIT,
        )
        .expect("original scoped admission");
    // A bounded observation loop uses actual running evidence rather than elapsed sleeps.
    // 有界观测循环使用实际运行证据，不使用经过睡眠的推测。
    let deadline = Instant::now() + WAIT;
    while !original
        .snapshot()
        .expect("active callback")
        .host_effects
        .iter()
        .any(|effect| effect.phase == HostEffectPhase::Running)
    {
        assert!(Instant::now() < deadline, "callback did not become active");
        std::thread::yield_now();
    }
    assert!(
        fixture
            .host
            .submit_in_workspace(
                call(&second, "retained", json!({})),
                fixture.root.path(),
                WAIT
            )
            .is_err()
    );
    for correlation in ["retained", "unknown"] {
        // Direct core calls cannot borrow a registered or unknown host request's workspace.
        // 直接核心调用不能借用已登记或未知宿主请求的工作区。
        let foreign = fixture
            .host
            .runtime()
            .submit(call(&second, correlation, json!({})), WAIT)
            .expect("foreign core admission")
            .wait(WAIT)
            .expect("explicit authority refusal");
        assert_eq!(foreign.value.as_ref().expect("Lua denial")["ok"], false);
        assert!(
            foreign
                .host_effects
                .iter()
                .all(|effect| effect.effects == EffectState::NotStarted)
        );
    }
    original.cancel().expect("request cancellation");
    // Keep only the stable ID after the original observer disappears.
    // 原观察者离开后仅保留稳定 ID。
    let operation_id = original.id().to_owned();
    drop(original);
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("active scope remains"),
        0
    );
    drop(gate);
    assert_eq!(
        fixture
            .host
            .runtime()
            .operation(&operation_id)
            .expect("original operation survives")
            .wait(WAIT)
            .expect("actual cancelled completion")
            .phase,
        OperationPhase::Cancelled
    );
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("actual terminal cleanup"),
        1
    );
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("idempotent reaping"),
        0
    );
    assert!(
        fixture
            .services
            .storage
            .lock()
            .expect("no unauthorized storage entry")
            .is_empty()
    );
    fixture.close();
}

/// Verify missing identities, invalid workspaces and core rejections publish no workspace scope.
/// 验证缺失身份、非法工作区及核心拒绝不会发布工作区作用域。
#[test]
fn native_workspace_requests_reject_before_publication() {
    // The initializer fails if any rejected request is accidentally executed.
    // 如果任一已拒绝请求被意外执行，初始化器会失败。
    let fixture = Fixture::new();
    // Only valid metadata is registered; rejected requests must not start this source.
    // 仅注册有效元数据；拒绝的请求不得启动此源码。
    let pool = fixture.pool("error('must not execute')");
    // Absence is legal for ordinary fixed calls, but not for explicit workspace admission.
    // 缺省对普通固定调用合法，但对显式工作区入场不合法。
    let mut missing = call(&pool, "unused", json!({}));
    missing.context.request_context = None;
    assert!(
        fixture
            .host
            .submit_in_workspace(missing, fixture.root.path(), WAIT)
            .is_err()
    );
    assert!(
        fixture
            .host
            .submit_in_workspace(
                call(&pool, "missing-path", json!({})),
                &fixture.root.path().join("absent"),
                WAIT
            )
            .is_err()
    );
    assert!(
        fixture
            .host
            .submit_in_workspace(
                call(&pool, "zero-budget", json!({})),
                fixture.root.path(),
                Duration::ZERO
            )
            .is_err()
    );
    for request_id in ["", " ", "embedded\0zero"] {
        assert!(
            fixture
                .host
                .submit_in_workspace(
                    call(&pool, request_id, json!({})),
                    fixture.root.path(),
                    WAIT
                )
                .is_err()
        );
    }
    // An undeclared export fails core validation after host preparation but before publication.
    // 未声明导出在宿主准备后、发布前被核心校验拒绝。
    let mut invalid = call(&pool, "invalid-export", json!({}));
    invalid.export = "absent".into();
    assert!(
        fixture
            .host
            .submit_in_workspace(invalid, fixture.root.path(), WAIT)
            .is_err()
    );
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("no retained request scopes"),
        0
    );
    assert!(
        fixture
            .host
            .runtime()
            .list_operations(Some(&pool), None, 1)
            .expect("no admitted operations")
            .operation_ids
            .is_empty()
    );
    assert_eq!(
        fixture
            .host
            .runtime()
            .resources()
            .expect("no allocated VM")
            .resident,
        0
    );
    fixture.close();
}

/// Verify one-shot closing retains its original scope through cancellation and multiple actual host callbacks.
/// 验证单次关闭在取消及多个实际宿主回调期间保留原作用域。
#[test]
fn native_workspace_requests_keep_single_call_finalizer_scope() {
    // A fixed service gate exposes finalization before its physical callbacks finish.
    // 固定服务门在物理回调完成之前暴露关闭阶段。
    let fixture = Fixture::new();
    // Both closing callbacks require the same original correlation, even after the business caller cancels.
    // 即使业务调用方取消，两个关闭回调也要求同一原始关联。
    let mut module = fixture.definition("return {call=function() return true end, shutdown=function() for i=1,2 do local r=vulcan.host.call('vulcan.plugin.storage.sqlite.query',{probe=i}); assert(r.ok) end return true end}");
    module.exports.push(ModuleExport {
        name: "shutdown".into(),
        input_schema: json!(true),
        output_schema: json!(true),
    });
    module.finalizer = Some(ModuleFinalizer {
        export: "shutdown".into(),
        arguments: json!({}),
        timeout_ms: WAIT.as_millis().try_into().expect("bounded close"),
    });
    // Single-call state must retire after closing instead of becoming a reusable idle instance.
    // 单次状态必须在关闭后退役，而非成为可复用空闲实例。
    let mut config = policy();
    config.reuse = InstanceReuse::SingleCall;
    // Only the formal core owns the automatic closing sequence.
    // 仅正式核心拥有自动关闭序列。
    let pool = fixture
        .host
        .register_pool(module, config, VulcanPluginBridgeLeaseScope::Enabled)
        .expect("single-call pool");
    // Prevent the first closing callback from entering the service until ownership is observed.
    // 在观察所有权之前阻止首个关闭回调进入服务。
    let gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("pause closing service");
    // Request scope and operation identity are published together before any closing callback can run.
    // 在任何关闭回调可运行之前共同发布请求作用域与操作身份。
    let operation = fixture
        .host
        .submit_in_workspace(
            call(&pool, "single-close", json!({})),
            fixture.root.path(),
            WAIT,
        )
        .expect("single scoped operation");
    // Poll real core evidence with the same fixture observation bound.
    // 使用同一夹具观测边界轮询真实核心证据。
    let deadline = Instant::now() + WAIT;
    loop {
        // Preserve a terminal failure immediately instead of reducing it to a polling timeout.
        // 立即保留终态失败，不将其简化为轮询超时。
        let snapshot = operation.snapshot().expect("closing observation");
        if snapshot
            .host_effects
            .iter()
            .any(|effect| effect.phase == HostEffectPhase::Running)
        {
            break;
        }
        assert!(
            !snapshot.phase.is_terminal() && Instant::now() < deadline,
            "closing callback did not begin: {snapshot:?}"
        );
        std::thread::yield_now();
    }
    operation.cancel().expect("late business cancellation");
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("closing remains active"),
        0
    );
    drop(gate);
    // The independent finalization result must include both successful host callbacks.
    // 独立关闭结果必须包含两个成功宿主回调。
    let terminal = operation.wait(WAIT).expect("actual closing completion");
    assert_eq!(
        terminal
            .finalization
            .as_ref()
            .expect("closing evidence")
            .outcome
            .as_ref()
            .expect("closing result")
            .result()
            .expect("successful close"),
        true
    );
    assert_eq!(
        fixture
            .services
            .storage
            .lock()
            .expect("actual callbacks")
            .len(),
        2
    );
    assert!(
        terminal
            .host_effects
            .iter()
            .all(|effect| effect.caller.request_id.as_deref() == Some("single-close"))
    );
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("release after finalization"),
        1
    );
    fixture.close();
}

/// Verify running and queued workspace metadata share a real bounded admission budget with no ghost operation.
/// 验证运行中与排队工作区元数据共享真实有界入场预算，且没有幽灵操作。
#[test]
fn native_workspace_requests_bound_retained_bytes_before_core_admission() {
    // Two native workers can stay active while a third request waits in a serial pool.
    // 两个原生工作者保持活动期间，第三个请求可在串行池中等待。
    let fixture = Fixture::new();
    // Separate pools ensure both first requests reach physical host callbacks.
    // 独立池确保前两个请求均到达物理宿主回调。
    let first = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // The second pool consumes the other real parent worker.
    // 第二个池消费另一个真实父级工作者。
    let second = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Freeze both callbacks without assuming execution speed.
    // 冻结两个回调，不假定执行速度。
    let gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("hold service entry");
    // Derive payload size from the single parent byte budget; paths add the remaining overhead.
    // 从唯一父级字节预算派生载荷大小；路径增加剩余开销。
    let identity = "r".repeat(QUEUED_BYTES / 4);
    // Keep handles only to control and verify the actual accepted operations.
    // 仅为控制和验证真实已接纳操作而保留句柄。
    let mut operations = Vec::new();
    for (index, pool) in [&first, &second].into_iter().enumerate() {
        // Each request has a distinct live identity and an independently running VM.
        // 每个请求具有不同存活身份与独立运行 VM。
        let operation = fixture
            .host
            .submit_in_workspace(
                call(pool, &format!("{identity}-{index}"), json!({})),
                fixture.root.path(),
                WAIT,
            )
            .expect("running workspace admission");
        // Native running evidence proves its queue bytes have been released before the next admission.
        // 原生运行证据证明其队列字节在下次入场之前已经释放。
        let deadline = Instant::now() + WAIT;
        while !operation
            .snapshot()
            .expect("running callback")
            .host_effects
            .iter()
            .any(|effect| effect.phase == HostEffectPhase::Running)
        {
            assert!(Instant::now() < deadline, "callback did not begin");
            std::thread::yield_now();
        }
        operations.push(operation);
    }
    operations.push(
        fixture
            .host
            .submit_in_workspace(
                call(&first, &format!("{identity}-queued"), json!({})),
                fixture.root.path(),
                WAIT,
            )
            .expect("one queued scope fits"),
    );
    // This request fits the core queue but exceeds retained host scope bytes across active and queued work.
    // 此请求符合核心队列预算，但超过活动及排队工作的保留宿主作用域字节预算。
    let rejected = fixture.host.submit_in_workspace(
        call(&second, &format!("{identity}-excess"), json!({})),
        fixture.root.path(),
        WAIT,
    );
    assert!(
        matches!(rejected, Err(ref failure) if failure.code == EmbeddedErrorCode::CapacityExceeded)
    );
    assert_eq!(
        fixture
            .host
            .runtime()
            .list_operations(None, None, 16)
            .expect("only accepted operations")
            .operation_ids
            .len(),
        operations.len()
    );
    for operation in &operations {
        operation.cancel().expect("cancel accepted work");
    }
    drop(gate);
    for operation in &operations {
        assert_eq!(
            operation
                .wait(WAIT)
                .expect("actual operation completion")
                .phase,
            OperationPhase::Cancelled
        );
    }
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("release original byte reservations"),
        operations.len()
    );
    // An exhausted registry accepts a new request only after actual completion has released its bytes.
    // 耗尽注册表仅在实际完成释放字节之后接纳新请求。
    let resumed = fixture
        .host
        .submit_in_workspace(
            call(&second, "after-completion", json!({})),
            fixture.root.path(),
            WAIT,
        )
        .expect("reuse actual released capacity")
        .wait(WAIT)
        .expect("new callback completes");
    assert_eq!(resumed.phase, OperationPhase::Succeeded);
    assert_eq!(
        fixture
            .host
            .reap_workspace_requests()
            .expect("reap new scope"),
        1
    );
    fixture.close();
}

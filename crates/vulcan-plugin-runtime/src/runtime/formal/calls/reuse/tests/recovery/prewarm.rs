//! Interrupted prewarming retains original initialization across catalog updates and owner drainage.
//! 中断预热在目录更新及所有者排空期间保留原初始化。

use super::*;

/// Cancellation and a newer package generation cannot release or replay a still-running prewarm callback.
/// 取消及更新包代次不能释放或重放仍在运行的预热回调。
#[tokio::test]
async fn hot_reuse_prewarm_cancel_and_update_keep_actual_initializer_owned() {
    // The existing fixture blocks a real initialization callback and releases it even during test unwinding.
    // 既有夹具阻塞真实初始化回调，并在测试展开时仍释放它。
    let recovery = RecoveryFixture::new(true);
    // Retain the published generation before queueing this exact prewarm request.
    // 排队此精确预热请求前保留已发布代次。
    let generation =
        recovery.fixture.manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    // The original manager and authorized workspace move into the interruptible observation task.
    // 原管理器及已授权工作区移入可中断观测任务。
    let manager = recovery.fixture.manager.clone();
    let workspace = recovery.fixture.root.path().to_path_buf();
    // Only this observer is aborted; the formal supervisor and real native worker retain ownership.
    // 仅中止此观测者；正式监督者及真实原生工作者保留归属。
    let task = tokio::spawn(async move {
        manager
            .prewarm_reusable_tool_instance(
                "idle-plugin",
                "idle_ping",
                generation,
                "owner".to_owned(),
                workspace,
            )
            .await
    });
    recovery.events.wait_entered().await;
    // Capture actual core identity before changing the catalog or requesting cancellation.
    // 更改目录或请求取消前捕获真实核心身份。
    let original = recovery
        .fixture
        .manager
        .formal_calls("owner")
        .expect("prewarm calls")
        .into_iter()
        .find(|call| !call.retired)
        .expect("live prewarm");
    let operation = original.operation.as_ref().expect("native operation");
    assert!(matches!(
        &operation.context,
        OperationContext::Module(context) if context.prewarm
    ));
    // Publish a new source generation while the original callback is still running.
    // 原回调仍在运行时发布新的源码代次。
    let source_path = recovery
        .fixture
        .root
        .path()
        .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
    let mut source = std::fs::read_to_string(&source_path).expect("old source");
    source.push_str("\n-- New package generation.\n-- 新包代次。\n");
    std::fs::write(&source_path, source).expect("new source");
    recovery
        .fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("new generation");
    assert_ne!(
        recovery.fixture.manager.snapshot().enabled_tools["idle_ping"].plugin_generation,
        generation
    );
    task.abort();
    assert!(task.await.expect_err("aborted observer").is_cancelled());
    // Finite drain reports pending ownership rather than pretending that cancellation returned the callback.
    // 有限排空报告待完成归属，不假装取消已使回调返回。
    let pending = recovery
        .fixture
        .manager
        .drain_owner_sessions("owner")
        .await
        .expect("finite drain");
    assert!(!pending.failures.is_empty());
    assert_eq!(
        recovery
            .fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("physical occupancy")
            .resident,
        1
    );
    let retained = recovery
        .fixture
        .manager
        .formal_call_status(&original.invocation_id, "owner")
        .expect("retained call");
    assert_eq!(retained.plugin_generation, generation);
    assert!(!retained.retired);
    assert_eq!(
        retained.operation.expect("same operation").operation_id,
        operation.operation_id
    );
    recovery.events.release();
    // All subsequent passes resume the same cleanup and never submit another initialization.
    // 全部后续轮次恢复同一清理，绝不提交另一初始化。
    let deadline = Instant::now() + WAIT;
    loop {
        let report = recovery
            .fixture
            .manager
            .drain_owner_sessions("owner")
            .await
            .expect("resume cleanup");
        if report.failures.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "{report:?}");
    }
    let terminal = recovery
        .fixture
        .manager
        .formal_call_status(&original.invocation_id, "owner")
        .expect("terminal evidence");
    assert!(terminal.retired);
    assert_eq!(
        terminal
            .operation
            .expect("original terminal operation")
            .operation_id,
        operation.operation_id
    );
    recovery
        .fixture
        .manager
        .forget_formal_call(&original.invocation_id, "owner")
        .expect("acknowledge original operation");
    recovery.fixture.close().await;
    // Committed callbacks prove initialization was executed once and business was never invoked.
    // 已提交回调证明初始化只执行一次且业务从未调用。
    let values = recovery.events.values.lock().expect("actual effects");
    assert_eq!(
        values.iter().filter(|event| event.topic == "block").count(),
        1
    );
    assert!(!values.iter().any(|event| event.topic == "business"));
}

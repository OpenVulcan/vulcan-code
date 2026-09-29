//! Live policy revocation fences additional initialization while retaining already-running native work.
//! 活动策略撤销隔离额外初始化，同时保留已经运行的原生工作。

use super::*;

/// Disabling a two-instance target inside its first callback preserves that operation and prevents a second one.
/// 在双实例目标的首个回调期间关闭策略，保留该操作并阻止第二个操作。
#[tokio::test]
async fn hot_reuse_automatic_policy_change_preserves_native_work_without_refill() {
    // The actual host callback holds the first VM until the test explicitly releases it.
    // 实际宿主回调持有首个 VM，直到测试明确释放。
    let recovery = RecoveryFixture::new(true);
    recovery.fixture.target_capacity(2);
    let origin = recovery.fixture.automatic_policy(Some(2));
    let manager = &recovery.fixture.manager;
    let prepared = manager
        .prepare_automatic_tool_prewarm(recovery.fixture.target(2), origin.clone())
        .expect("prepare automatic target");
    manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start first initializer");
    recovery.events.wait_entered().await;
    let running = manager
        .formal_call_status(&prepared.invocation_id, "owner")
        .expect("actual running operation");
    let operation_id = running
        .operation
        .as_ref()
        .expect("native operation")
        .operation_id
        .clone();
    assert!(
        !running
            .operation
            .as_ref()
            .expect("native operation")
            .phase
            .is_terminal()
    );
    recovery.fixture.automatic_policy(None);
    assert_eq!(
        recovery
            .fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("actual ownership")
            .resident,
        1
    );
    assert_eq!(
        manager
            .prepare_automatic_tool_prewarm(recovery.fixture.target(2), origin)
            .expect("same in-flight receipt")
            .invocation_id,
        prepared.invocation_id
    );
    recovery.events.release();
    let finished = targets::wait_target(&recovery.fixture, &prepared.invocation_id).await;
    assert!(
        finished
            .error
            .as_ref()
            .expect("revoked target")
            .contains("policy changed")
    );
    assert_eq!(
        finished
            .operation
            .as_ref()
            .expect("original native evidence")
            .operation_id,
        operation_id
    );
    assert!(
        finished
            .prewarm
            .as_ref()
            .expect("original target")
            .completed
            .is_empty()
    );
    assert!(
        finished
            .prewarm
            .as_ref()
            .expect("original target")
            .satisfied
            .is_none()
    );
    assert_eq!(
        recovery
            .events
            .values
            .lock()
            .expect("real events")
            .iter()
            .filter(|event| event.topic == "block")
            .count(),
        1
    );
    // The initialized VM may still serve ordinary authorized business; policy revocation is not pool destruction.
    // 已初始化 VM 仍可服务普通授权业务；策略撤销不是销毁池。
    assert_eq!(
        recovery
            .fixture
            .invoke("owner", recovery.fixture.root.path().to_path_buf(), "ok")
            .await
            .expect("reuse confirmed VM")["count"],
        1
    );
    manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge");
    recovery.fixture.close().await;
}

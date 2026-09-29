//! Lifecycle permissions fence native entry independently from mutable policy and retained receipt identity.
//! 生命周期权限独立于可变策略及保留回执身份隔离原生入场。

use super::*;

/// Closing an admitted trigger preserves its receipt but cannot be bypassed by a replacement permit or forgetting.
/// 关闭已接纳触发保留其回执，但不能通过替代许可或遗忘绕过。
#[tokio::test]
async fn hot_reuse_automatic_closed_trigger_cannot_be_reauthorized() {
    let fixture = Fixture::new(10_000, 100);
    let origin = fixture.automatic_policy(Some(1));
    let trigger = VulcanPluginAutomaticPrewarmTrigger::default();
    let permit = trigger.permit();
    let prepared = fixture
        .manager
        .prepare_automatic_tool_prewarm(fixture.target(1), origin.clone(), permit.clone())
        .expect("admit original");
    trigger.close();
    assert!(permit.is_closed(), "original permit must observe closure");
    assert_eq!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin.clone(), permit.clone())
            .expect("observe retained original")
            .invocation_id,
        prepared.invocation_id
    );
    let replacement = VulcanPluginAutomaticPrewarmTrigger::default();
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin.clone(), replacement.permit())
            .is_err(),
        "same origin cannot acquire a replacement permission"
    );
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("original supervisor");
    let failed = targets::wait_target(&fixture, &prepared.invocation_id).await;
    assert!(
        failed
            .error
            .as_ref()
            .expect("closed trigger error")
            .contains("lifecycle is closed")
    );
    assert!(
        failed.operation.is_none(),
        "closed permission cannot admit a native initializer"
    );
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("physical state")
            .resident,
        0
    );
    fixture
        .manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge original");
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin, permit)
            .is_err(),
        "forgetting cannot reopen the closed permission"
    );
    fixture.close().await;
}

/// Dropping the sole host owner revokes all cloned permits even before any target registration.
/// 甚至目标登记之前，丢弃唯一宿主所有者也会撤销全部克隆许可。
#[tokio::test]
async fn hot_reuse_automatic_dropped_trigger_rejects_late_preparation() {
    let fixture = Fixture::new(10_000, 100);
    let origin = fixture.automatic_policy(Some(1));
    let trigger = VulcanPluginAutomaticPrewarmTrigger::default();
    let permit = trigger.permit();
    drop(trigger);
    assert!(
        permit.is_closed(),
        "owner drop must close outstanding permission"
    );
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin, permit)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("no registration")
            .is_empty()
    );
    fixture.close().await;
}

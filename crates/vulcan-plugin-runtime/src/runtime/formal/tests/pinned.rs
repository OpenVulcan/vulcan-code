//! Real production pinned-session routing and physical ownership regression tests.
//! 真实生产固定会话路由及物理所有权回归测试。

use super::stateless::{Fixture, WAIT};
use super::*;
use crate::{VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation};
use serde_json::json;

/// A committed aggregate policy change preserves the exact pinned VM, its counter and original finalizer.
/// 已提交聚合策略变化保留精确固定 VM、计数器及原关闭器。
#[test]
fn formal_pinned_capacity_policy_preserves_original_state() {
    // The existing production fixture dispatches real stateful session operations.
    // 既有生产夹具分发真实有状态会话操作。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        // Opening commits the first private counter value before any policy change.
        // 开启在任何策略变化之前提交首个私有计数值。
        let id = opened(&fixture).await;
        let original = fixture
            .manager
            .formal_session_status(&id, "owner")
            .expect("original session")
            .core
            .expect("native session");
        let policy = fixture
            .manager
            .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolSession)
            .expect("capacity")
            .expect("registered");
        // Convert the same aggregate owner to an explicit smaller dedicated policy.
        // 将同一聚合所有者转换为明确较小专用策略。
        let mut config = policy.snapshot.config.clone();
        config.resources.kind = luaskills::runtime::embedded::PoolKind::Dedicated;
        config.resources.min_resident_vms = 1;
        config.resources.max_resident_vms = 1;
        config.resources.max_running_calls = 1;
        let change = crate::VulcanPluginCapacityPolicyUpdate {
            update_id: uuid::Uuid::new_v4().to_string(),
            runtime_id: policy.runtime_id,
            capacity_id: policy.snapshot.capacity_id,
            expected_revision: policy.policy_revision,
            plugin_id: "idle-plugin".into(),
            purpose: VulcanPluginCapacityPurpose::ToolSession,
            config: Some(config),
        };
        assert_eq!(
            fixture
                .manager
                .begin_capacity_policy_update(change.clone())
                .expect("commit")
                .phase,
            crate::VulcanPluginCapacityPolicyPhase::Committed
        );
        for expected in 2..=3 {
            // State continuity proves the original instance was neither reset nor migrated.
            // 状态连续证明原实例既未重置也未迁移。
            let value = fixture
                .manager
                .invoke(request(&fixture, "idle_call", Some(&id), "ok"))
                .await
                .expect("original state");
            assert_eq!(value["result"]["count"], expected);
        }
        let current = fixture
            .manager
            .formal_session_status(&id, "owner")
            .expect("current session")
            .core
            .expect("native session");
        assert_eq!(current.session_id, original.session_id);
        assert_eq!(current.pool_id, original.pool_id);
        fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
            .await
            .expect("original close");
        // Successful shutdown acknowledges its sole finalization record automatically.
        // 成功关闭会自动确认唯一关闭记录。
        assert!(
            fixture
                .manager
                .formal_sessions("owner")
                .expect("healthy session acknowledged")
                .is_empty()
        );
        fixture
            .manager
            .forget_capacity_policy_update(&change.update_id)
            .expect("policy acknowledgement");
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("one original finalizer")
                .len(),
            1
        );
    });
}

/// Wait for the original `id` in `fixture` to physically retire; returns its retained evidence.
/// 等待 `fixture` 中原始 `id` 物理退役；返回其保留证据。
async fn retired(fixture: &Fixture, id: &str) -> VulcanPluginFormalSessionStatus {
    tokio::time::timeout(WAIT, async {
        loop {
            let status = fixture
                .manager
                .formal_session_status(id, "owner")
                .expect("original session");
            if status.retired {
                return status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("physical retirement completes within test budget")
}

/// Rejected input preserves a healthy VM, while a business failure stays queryable until explicit acknowledgement.
/// 拒绝的输入保留健康 VM，而业务失败在显式确认前始终可查询。
#[test]
fn formal_pinned_rejection_preserves_state_and_failure_evidence() {
    let settings = VulcanPluginRuntimeSettings::default();
    let oversized = "x".repeat(settings.parent.max_value_bytes + 1);
    let fixture = Fixture::new(settings);
    fixture.runtime.block_on(async {
        let id = opened(&fixture).await;
        assert!(
            fixture
                .manager
                .invoke(request(&fixture, "idle_call", Some(&id), &oversized))
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .cancel_formal_session(&id, "foreign")
                .is_err()
        );
        let healthy = fixture
            .manager
            .invoke(request(&fixture, "idle_call", Some(&id), "ok"))
            .await
            .expect("rejection must preserve session");
        assert_eq!(healthy["result"]["count"], 2);
        let failure = fixture
            .manager
            .invoke(request(&fixture, "idle_call", Some(&id), "business_error"))
            .await
            .expect_err("explicit business failure");
        assert!(failure.to_string().contains("business-marker"));
        let still_healthy = fixture
            .manager
            .invoke(request(&fixture, "idle_call", Some(&id), "ok"))
            .await
            .expect("business rejection keeps VM");
        assert_eq!(still_healthy["result"]["count"], 4);
        fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
            .await
            .expect("actual closing success");
        let status = retired(&fixture, &id).await;
        assert!(status.finalization.is_some());
        let records = fixture
            .manager
            .formal_session_operations(&id, "owner")
            .expect("business failure retained");
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| {
            record
                .value
                .as_ref()
                .is_some_and(|value| value.to_string().contains("business-marker"))
        }));
        fixture
            .manager
            .forget_formal_session(&id, "owner")
            .expect("explicit acknowledgement");
        assert!(
            fixture
                .manager
                .formal_sessions("owner")
                .expect("empty registry")
                .is_empty()
        );
        assert_eq!(
            fixture.events.values.lock().expect("one finalizer").len(),
            1
        );
    });
}

/// A failed shutdown reports its own frame after physical retirement and is never replayed by another close.
/// 关闭失败在物理退役后报告自身帧，另一次关闭不会重放。
#[test]
fn formal_pinned_shutdown_failure_retains_independent_evidence() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        let opened = fixture
            .manager
            .invoke(request(&fixture, "idle_open", None, "shutdown_error"))
            .await
            .expect("open with failing finalizer");
        let id = opened["session_id"]
            .as_str()
            .expect("host session identity");
        for _ in 0..2 {
            let closed = fixture
                .manager
                .invoke(request(&fixture, "idle_close", Some(id), "ok"))
                .await
                .expect("physical close response");
            assert_eq!(closed["closed"], true);
            assert_eq!(closed["result"]["graceful_shutdown"], false);
            assert!(
                closed["result"]["diagnostic"]
                    .as_str()
                    .expect("closing diagnostic")
                    .contains("shutdown-marker")
            );
        }
        assert!(retired(&fixture, id).await.finalization.is_some());
        assert_eq!(
            fixture
                .manager
                .formal_session_operations(id, "owner")
                .expect("one independent closing record")
                .len(),
            1
        );
        fixture
            .manager
            .forget_formal_session(id, "owner")
            .expect("acknowledge failed close");
        assert_eq!(fixture.events.values.lock().expect("no replay").len(), 1);
    });
}

/// Preparation is owned before blocking admission and cancellation never dispatches a Lua function.
/// 阻塞入场前已登记准备所有权，取消后不会分发 Lua 函数。
#[test]
fn formal_pinned_preparation_cancellation_keeps_exact_ownership() {
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
        let business = request(&fixture, "idle_open", None, "ok");
        let waiter = tokio::spawn(async move { manager.invoke(business).await });
        let status = tokio::time::timeout(WAIT, async {
            loop {
                if let Some(status) = fixture
                    .manager
                    .formal_sessions("owner")
                    .expect("preparing sessions")
                    .into_iter()
                    .next()
                {
                    break status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("preparation registered before blocking worker");
        assert!(status.core.is_none());
        assert!(!status.retired);
        fixture
            .manager
            .cancel_formal_session(&status.session_id, "owner")
            .expect("cancel exact preparation");
        assert!(
            fixture
                .manager
                .forget_formal_session(&status.session_id, "owner")
                .is_err()
        );
        fixture.events.release();
        blocked
            .await
            .expect("blocker task")
            .expect("blocker returned");
        assert!(waiter.await.expect("opening observer returned").is_err());
        let completed = retired(&fixture, &status.session_id).await;
        assert!(completed.core.is_none());
        assert!(completed.finalization.is_none());
        assert!(
            fixture
                .manager
                .formal_session_operations(&status.session_id, "owner")
                .expect("no Lua operations admitted")
                .is_empty()
        );
        fixture
            .manager
            .forget_formal_session(&status.session_id, "owner")
            .expect("acknowledge cancelled preparation");
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("only test blocker committed")
                .len(),
            1
        );
    });
}

/// Executor shutdown retains the original core identity and explicit drain resumes cleanup without replay.
/// 执行器退出保留原核心身份，显式排空只恢复清理而不重放。
#[test]
fn formal_pinned_executor_shutdown_recovers_original_session() {
    let mut fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let id = fixture.runtime.block_on(opened(&fixture));
    let manager = fixture.manager.clone();
    let business = request(&fixture, "idle_call", Some(&id), "block");
    let waiter = fixture
        .runtime
        .spawn(async move { manager.invoke(business).await });
    fixture.runtime.block_on(fixture.wait_entered());
    let original = fixture
        .manager
        .formal_session_status(&id, "owner")
        .expect("active session")
        .core
        .expect("core identity")
        .session_id;
    let replacement = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .expect("replacement executor");
    let previous = std::mem::replace(&mut fixture.runtime, replacement);
    previous.shutdown_timeout(WAIT);
    drop(waiter);
    let interrupted = fixture
        .manager
        .formal_session_status(&id, "owner")
        .expect("interrupted supervision");
    assert!(interrupted.requires_cleanup_recovery);
    assert!(!interrupted.retired);
    assert_eq!(
        interrupted.core.expect("same core identity").session_id,
        original
    );
    assert!(
        !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("actual capacity retained")
    );
    fixture.events.release();
    let report = fixture
        .runtime
        .block_on(fixture.manager.drain_owner_sessions("owner"))
        .expect("recover original cleanup");
    assert_eq!(report.closed_sessions, 1);
    assert!(report.failures.is_empty(), "{report:?}");
    fixture.runtime.block_on(retired(&fixture, &id));
    fixture
        .manager
        .forget_formal_session(&id, "owner")
        .expect("acknowledge recovered records");
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("one business effect and one closing effect")
            .len(),
        2
    );
}

/// Accounting failure after VM retirement preserves host capacity and allows exact cleanup recovery.
/// VM 退役后的记账失败保留宿主容量，并允许恢复精确清理。
#[test]
fn formal_pinned_accounting_failure_preserves_recoverable_owner() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        let id = opened(&fixture).await;
        let counts = &fixture.manager.state.active_session_counts;
        let poisoning = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = counts.lock().expect("inject count lock interruption");
            panic!("test accounting interruption");
        }));
        assert!(poisoning.is_err());
        let failure = fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
            .await
            .expect_err("count release must report failure");
        assert!(failure.to_string().contains("counter lock is poisoned"));
        let status = fixture
            .manager
            .formal_session_status(&id, "owner")
            .expect("failed release evidence");
        assert!(!status.retired);
        assert!(status.requires_cleanup_recovery);
        assert!(
            fixture
                .manager
                .active_session_count("idle-plugin")
                .expect_err("actual poisoned accounting must not yield a live count")
                .message
                .contains("counter lock is poisoned")
        );
        // Inspect only the fixture-injected poisoned guard to prove its real original reservation was not released.
        // 仅检查夹具注入的中毒守卫，证明其真实原预留没有释放。
        assert_eq!(
            counts
                .lock()
                .expect_err("actual injected accounting poison")
                .into_inner()
                .get("idle-plugin")
                .copied(),
            Some(1)
        );
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("usage")
                .expect("parent")
                .resources
                .resident,
            0
        );
        assert!(fixture.manager.forget_formal_session(&id, "owner").is_err());
        // Repair only the fixture's injected poison; production preserves the explicit failure.
        // 只修复夹具注入的中毒；生产保留明确故障。
        counts.clear_poison();
        let report = fixture
            .manager
            .drain_owner_sessions("owner")
            .await
            .expect("resume remaining accounting");
        assert_eq!(report.closed_sessions, 1);
        assert!(report.failures.is_empty(), "{report:?}");
        assert_eq!(
            fixture
                .manager
                .active_session_count("idle-plugin")
                .expect("actual live reservation count"),
            0
        );
        fixture
            .manager
            .forget_formal_session(&id, "owner")
            .expect("acknowledge recovered ownership");
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("closing never replayed")
                .len(),
            1
        );
    });
}

/// Build the existing host request for exact `tool`, optional owned `session`, and fixture behavior `mode`.
/// 为精确 `tool`、可选所属 `session` 及夹具行为 `mode` 构造既有宿主请求。
fn request(
    fixture: &Fixture,
    tool: &str,
    session: Option<&str>,
    mode: &str,
) -> crate::VulcanPluginInvocationRequest {
    let mut request = fixture.request(mode);
    request.tool_id = tool.to_owned();
    request.arguments = if tool == "idle_close" {
        json!({"session_id":session.expect("close needs identity")})
    } else if let Some(id) = session {
        json!({"session_id":id,"mode":mode})
    } else {
        json!({"mode":mode})
    };
    request
}

/// Open the real declared tool and return its host-owned identity, checking the first instance-local count.
/// 打开真实声明工具并返回宿主所属身份，同时检查首个实例局部计数。
async fn opened(fixture: &Fixture) -> String {
    let opened = fixture
        .manager
        .invoke(request(fixture, "idle_open", None, "ok"))
        .await
        .expect("open real pinned tool");
    assert_eq!(opened["result"]["count"], 1);
    opened["session_id"]
        .as_str()
        .expect("host session identity")
        .to_owned()
}

/// Serial session calls share the original VM while owner rejection has no business side effect.
/// 串行会话调用共享原 VM，所有者拒绝没有业务副作用。
#[test]
fn formal_pinned_production_route_preserves_state_owner_and_shutdown() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        let id = opened(&fixture).await;
        let mut foreign = request(&fixture, "idle_call", Some(&id), "ok");
        foreign.owner_session_id = "foreign".to_string();
        assert!(fixture.manager.invoke(foreign).await.is_err());
        for expected in 2..=3 {
            let value = fixture
                .manager
                .invoke(request(&fixture, "idle_call", Some(&id), "ok"))
                .await
                .expect("call original instance");
            assert_eq!(value["result"]["count"], expected);
            assert_eq!(value["session_id"], id);
        }
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("physical runtime")
                .expect("attached")
                .resources
                .resident,
            1
        );
        let closed = fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
            .await
            .expect("close exact instance");
        assert_eq!(closed["closed"], true);
        assert_eq!(closed["result"]["closed"], true);
        assert!(
            fixture
                .manager
                .formal_sessions("owner")
                .expect("successful close acknowledged")
                .is_empty()
        );
        assert_eq!(
            fixture
                .manager
                .active_session_count("idle-plugin")
                .expect("actual live reservation count"),
            0
        );
        assert_eq!(
            fixture
                .manager
                .formal_runtime_status()
                .expect("physical runtime")
                .expect("attached")
                .resources
                .resident,
            0
        );
        let events = fixture.events.values.lock().expect("one actual close");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].revision, 3);
    });
}

/// Caller abandonment and concurrent drains keep one VM and one finalizer until native work truly returns.
/// 调用方放弃与并发排空保留单个 VM 及单个关闭回调，直至原生工作真实返回。
#[test]
fn formal_pinned_abandonment_and_competing_drain_keep_actual_owner() {
    let fixture = Fixture::new(VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: 100,
        ..VulcanPluginRuntimeSettings::default()
    });
    fixture.runtime.block_on(async {
        let id = opened(&fixture).await;
        let manager = fixture.manager.clone();
        let business = request(&fixture, "idle_call", Some(&id), "block");
        let waiter = tokio::spawn(async move { manager.invoke(business).await });
        fixture.wait_entered().await;
        waiter.abort();
        assert!(waiter.await.expect_err("observer left").is_cancelled());
        fixture
            .manager
            .begin_draining("idle-plugin")
            .expect("grant original closing authority");
        let (first, second) = tokio::join!(
            fixture.manager.drain_owner_sessions("owner"),
            fixture.manager.drain_plugin_sessions("idle-plugin")
        );
        for report in [first.expect("owner drain"), second.expect("plugin drain")] {
            assert_eq!(report.closed_sessions, 0);
            assert!(!report.failures.is_empty());
        }
        assert_eq!(
            fixture
                .manager
                .active_session_count("idle-plugin")
                .expect("actual live reservation count"),
            1
        );
        assert!(
            !fixture
                .manager
                .formal_session_status(&id, "owner")
                .expect("retained identity")
                .retired
        );
        fixture.events.release();
        tokio::time::timeout(WAIT, async {
            loop {
                if fixture
                    .manager
                    .formal_session_status(&id, "owner")
                    .expect("actual ownership")
                    .retired
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual native return permits retirement");
        let records = fixture
            .manager
            .formal_session_operations(&id, "owner")
            .expect("retained business and closing evidence");
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.finalization.is_some()));
        fixture
            .manager
            .forget_formal_session(&id, "owner")
            .expect("acknowledge original session");
        assert_eq!(fixture.events.values.lock().expect("no replay").len(), 2);
    });
}

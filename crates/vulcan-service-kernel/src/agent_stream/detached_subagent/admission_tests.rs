//! Production detached-stream admission regression tests.
//! 生产分离事件流准入回归验证。

use super::*;
use crate::tests::test_support::{
    TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::{
    attach_test_runtime_event_projection_barrier, set_runtime_event_projection_barriers_enabled,
};

/// Restores the process-wide barrier policy even if an assertion fails.
/// 即使断言失败也恢复进程级屏障策略。
struct BarrierPolicyGuard;

impl Drop for BarrierPolicyGuard {
    /// Releases pending test receipts and restores the disabled test default.
    /// 释放待处理测试回执并恢复测试默认的禁用状态。
    fn drop(&mut self) {
        let _ = set_runtime_event_projection_barriers_enabled(false);
    }
}

/// Verifies nested starts cannot bypass parent identity or persistent listener admission.
/// 验证嵌套启动不能绕过父任务身份或持久监听器准入。
#[tokio::test]
async fn nested_delegate_start_failure_rejects_production_barrier() {
    // Isolated storage owns the child sink without launching any Provider request.
    // 隔离存储持有子任务接收器，不启动任何 Provider 请求。
    let root_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let root = TestHostRuntimeRoot::new(root_id);
    let controller = root.attached_host_runtime_handle().await;
    let directory = tempfile::tempdir().unwrap();
    let locator = vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
        directory.path(),
        &task_id.to_string(),
        1,
    )
    .unwrap();
    let document = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator)
        .unwrap()
        .document;
    let child_handle = SessionHandle::new(task_id, locator, document);
    let mut service = test_service_state_with_parent_session(root_id);
    service
        .detached_subagent_task_listeners
        .insert(task_id, String::from("parent-execution"));
    let catalog = service.project_resource_catalog_registry.clone();
    let state = Arc::new(std::sync::Mutex::new(service));
    let runtime = test_agent_runtime_bridge();
    let mut sink = ProviderV1StreamSink::new(child_handle.clone(), catalog);
    let mut telemetry = ProviderTelemetryHookGuard::new_with_persistence(state.clone(), None);
    set_runtime_event_projection_barriers_enabled(true).unwrap();
    let _guard = BarrierPolicyGuard;
    // Every rejection must reach the real runtime waiter before any success projection.
    // 每一种拒绝都必须在任何成功投影之前传达到真实运行时等待器。
    for (event_session, parent_execution, expected_error) in [
        (
            root_id,
            "parent-execution",
            "does not belong to the owning task session",
        ),
        (
            task_id,
            "parent-execution",
            "invalid authoritative task metadata",
        ),
        (task_id, "old-execution", "superseded parent execution"),
    ] {
        let (event, waiter) = attach_test_runtime_event_projection_barrier(RuntimeEvent::new(
            RuntimeEventKind::DelegateStarted,
            "Nested start",
            "Missing child identity",
        ))
        .unwrap();
        let error = handle_webui_detached_subagent_task_event(
            &ServiceEventEmitter::new(vec![]),
            &state,
            &controller,
            &runtime,
            task_id,
            parent_execution,
            &child_handle,
            &mut telemetry,
            &mut sink,
            DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
                session_id: event_session,
                event,
            }),
        )
        .await
        .expect_err("nested start must fail closed");
        assert!(error.contains(expected_error), "{error}");
        let rejection = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("nested admission must reject immediately")
            .expect_err("runtime must not advance");
        assert!(rejection.contains(expected_error), "{rejection}");
        let listeners = &state.lock().unwrap().detached_subagent_task_listeners;
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners.get(&task_id).unwrap(), "parent-execution");
    }
}

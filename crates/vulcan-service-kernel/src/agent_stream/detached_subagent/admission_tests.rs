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

/// EOF or cancellation releases exact listener ownership while preserving published and replacement results.
/// 流结束或取消释放精确监听归属，同时保留已发布及替代执行结果。
/// Returns no value; exercises the same captured guard used by production spawned listeners.
/// 无返回值；覆盖生产派生监听器使用的同一捕获守卫。
#[tokio::test]
async fn detached_listener_exit_releases_only_unpublished_exact_owner() {
    for exit in ["eof", "cancel_before_poll", "replacement", "published"] {
        // Every case owns an isolated registry and runtime without a Provider request.
        // 每个用例拥有隔离注册表与运行时，不发起 Provider 请求。
        let parent = SessionId::new_v4();
        // Stable child identity is reused only inside this isolated case.
        // 稳定子身份只在本隔离用例内部复用。
        let child = SessionId::new_v4();
        // The listener registration must match the gate before its future is captured.
        // 捕获 Future 前，监听注册必须与门控匹配。
        let state = Arc::new(std::sync::Mutex::new(
            test_service_state_with_parent_session(parent),
        ));
        // This runtime retains both unpublished and published return evidence.
        // 此运行时保留尚未发布与已发布回流凭据。
        let runtime = test_agent_runtime_bridge();
        state
            .lock()
            .unwrap()
            .detached_subagent_task_listeners
            .insert(child, "original".into());
        runtime
            .task_runtime
            .expect_durable_task_return(child, "original")
            .unwrap();
        // Capture the guard outside the future, as production admission does.
        // 与生产准入一致，在 Future 外捕获守卫。
        let owner = listener_owner::ListenerOwner::new(
            state.clone(),
            runtime.clone(),
            child,
            "original".into(),
        );
        if exit == "replacement" {
            state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .insert(child, "replacement".into());
            runtime
                .task_runtime
                .expect_durable_task_return(child, "replacement")
                .unwrap();
        }
        if exit == "published" {
            runtime
                .task_runtime
                .publish_durable_task_return(
                    "original",
                    vulcan_session_boundary::SubagentMailboxEvent {
                        parent_thread_id: parent.into(),
                        parent_turn_id: None,
                        child_thread_id: child.into(),
                        kind: vulcan_session_boundary::SubagentMailboxEventKind::Failed,
                        timestamp_ms: 7,
                        payload: serde_json::json!({"status":"failed", "execution_id":"original"}),
                    },
                )
                .unwrap();
        }
        // A stream without a terminal exits naturally; an unpolled future is also cancellable.
        // 没有终态的流自然退出；未轮询的 Future 同样可以取消。
        let listener = async move {
            let _owner = owner;
            let mut stream = futures_util::stream::empty::<DetachedTaskRuntimeEvent>();
            while stream.next().await.is_some() {}
        };
        if exit == "cancel_before_poll" {
            drop(listener);
        } else {
            listener.await;
        }
        if exit == "replacement" {
            assert_eq!(
                state
                    .lock()
                    .unwrap()
                    .detached_subagent_task_listeners
                    .get(&child)
                    .unwrap(),
                "replacement"
            );
            assert!(
                runtime
                    .task_runtime
                    .release_unpublished_task_return(child, "replacement")
                    .unwrap()
            );
        } else {
            assert!(
                !state
                    .lock()
                    .unwrap()
                    .detached_subagent_task_listeners
                    .contains_key(&child)
            );
            // A leaked gate would make this bounded wait fail instead of producing an outcome.
            // 门控泄漏会使此有界等待失败，而非产生结果。
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                runtime
                    .task_runtime
                    .wait_for_durable_task_return(child, "original"),
            )
            .await
            .expect("exited listener must not retain an unpublished return gate");
            assert_eq!(result.is_ok(), exit == "published");
        }
    }
}

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

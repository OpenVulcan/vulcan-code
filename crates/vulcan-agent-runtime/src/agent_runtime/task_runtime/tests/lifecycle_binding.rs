//! Native spawn-tool identity propagation and transient binding ownership tests.
//! 原生派生工具身份传递与临时绑定归属验证。

use super::*;
use crate::tooling::collaboration::{AgentCollaborationCommand, NativeAgentCollaborationRuntime};
use collaboration_lifecycle::*;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};
use vulcan_tools_manager::VulcanToolInvocationContext;

/// Cancels only the exact unpublished return gate and leaves replacement executions intact.
/// 仅取消精确未发布回流门，并保持替代执行不变。
/// Parameters: none. Returns: none; checks stale cleanup, current cleanup and idempotent retry.
/// 参数：无。返回值：无；检查过期清理、当前清理及幂等重试。
#[test]
fn rejected_startup_return_gate_is_execution_scoped() {
    // The gate registry is the same one used by wait_agent and final mailbox publication.
    // 此门注册表与 wait_agent 及最终邮箱发布使用的是同一份。
    let runtime = TaskRuntime::new();
    // One reusable physical child receives two distinct execution generations.
    // 一个可复用物理子会话接收两个不同执行代次。
    let task_id = Uuid::new_v4();
    runtime
        .expect_durable_task_return(task_id, "first")
        .unwrap();
    runtime
        .expect_durable_task_return(task_id, "replacement")
        .unwrap();
    assert!(
        !runtime
            .cancel_rejected_task_return(task_id, "first")
            .unwrap()
    );
    assert_eq!(
        runtime.durable_return_mailboxes.read().unwrap()[&task_id].0,
        "replacement"
    );
    assert!(
        runtime
            .cancel_rejected_task_return(task_id, "replacement")
            .unwrap()
    );
    assert!(
        !runtime
            .cancel_rejected_task_return(task_id, "replacement")
            .unwrap()
    );
    assert!(runtime.durable_return_mailboxes.read().unwrap().is_empty());
}

/// Rejects actual host effects while allowing the real native tool to carry an opaque port.
/// 拒绝实际宿主效果，同时允许真实原生工具携带不透明端口。
#[derive(Debug)]
struct RejectingHost;

#[async_trait::async_trait]
impl NativeAgentCollaborationRuntime for RejectingHost {
    /// Rejects compaction in this lifecycle fixture; no snapshot is returned.
    /// 在此生命周期夹具中拒绝压缩；不返回快照。
    async fn compact_child_task(
        &self,
        _: crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
    ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
        Err(String::from("Unexpected child compaction in test host."))
    }
    /// Rejects unexpected initial creation in this narrowly scoped test host.
    /// 在此限定范围的测试宿主中拒绝非预期初始创建。
    async fn create_child_task(
        &self,
        _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskReservationRequest,
        _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<
        crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskLifecycleReceipt,
        String,
    > {
        Err(String::from("Unexpected initial child creation."))
    }
    /// No profile capture is expected; returns an explicit test boundary error.
    /// 不应捕获配置；返回明确测试边界错误。
    async fn capture_child_execution_profile(
        &self,
        _: ChildExecutionProfileCaptureRequest,
    ) -> Result<(), String> {
        Err("unexpected capture".into())
    }
    /// No collaboration command is expected; returns an explicit test boundary error.
    /// 不应执行协作命令；返回明确测试边界错误。
    async fn invoke(
        &self,
        _: VulcanToolInvocationContext,
        _: AgentCollaborationCommand,
    ) -> Result<Value, String> {
        Err("unexpected command".into())
    }
    /// No wait is expected; returns an explicit test boundary error.
    /// 不应执行等待；返回明确测试边界错误。
    async fn wait_for_incoming_question(
        &self,
        _: VulcanToolInvocationContext,
    ) -> Result<Value, String> {
        Err("unexpected wait".into())
    }
    /// Rejects effects beyond the native dispatch boundary.
    /// 拒绝原生分发边界以外的效果。
    async fn reserve_child_task(
        &self,
        _: ChildTaskReservationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err("unexpected reservation".into())
    }
    /// Rejects effects beyond the native dispatch boundary.
    /// 拒绝原生分发边界以外的效果。
    async fn activate_child_task(
        &self,
        _: ChildTaskActivationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err("unexpected activation".into())
    }
    /// Rejects effects beyond the native dispatch boundary.
    /// 拒绝原生分发边界以外的效果。
    async fn terminal_child_task(
        &self,
        _: ChildTaskTerminalRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err("unexpected terminal".into())
    }
}

/// Creates exact tool coordinates for the isolated native surface; no Provider is used.
/// 为隔离原生工具入口创建精确工具坐标，不使用 Provider。
fn context(parent: Uuid) -> VulcanToolInvocationContext {
    VulcanToolInvocationContext {
        session_id: parent.to_string(),
        request_stream_id: RequestStreamId::try_new("native-spawn-stream").unwrap(),
        api_call_id: ApiCallId::try_new(1).unwrap(),
        tool_call_id: ToolCallId::try_new(1).unwrap(),
        tool_seq: Seq::try_new(3).unwrap(),
        controller_tool_call_id: String::from("spawn-call"),
        provider_tool_call_id: Some(String::from("provider-spawn")),
    }
}

/// Exercises the model-visible closure and proves its backend receives the exact original context.
/// 执行模型可见闭包，证明后端收到精确原始上下文。
#[tokio::test]
async fn native_spawn_propagates_trusted_lifecycle_binding() {
    let runtime = Arc::new(TaskRuntime::new());
    let parent = Uuid::new_v4();
    let original = context(parent);
    let expected = original.clone();
    let owner = runtime.clone();
    let observed = Arc::new(AtomicUsize::new(0));
    let observations = observed.clone();
    let _backend = runtime
        .install_backend(
            parent,
            Arc::new(move |request: TaskExecutionRequest| {
                let binding = owner
                    .take_child_task_lifecycle_binding(&request.execution_id)
                    .unwrap()
                    .unwrap();
                assert_eq!(binding.reservation.invocation, expected);
                assert_eq!(
                    binding.reservation.execution.task_session_id,
                    request.task_id
                );
                assert_eq!(
                    binding.reservation.execution.execution_id,
                    request.execution_id
                );
                assert_eq!(
                    binding.reservation.operation_id.as_str(),
                    crate::tooling::collaboration::stable_collaboration_operation_id(
                        &expected,
                        SPAWN_AGENT_TOOL
                    )
                );
                observations.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(String::from("verified native dispatch")) })
            }),
        )
        .unwrap();
    let candidate =
        build_spawn_agent_tool_candidate(runtime.clone(), Some(Arc::new(RejectingHost)));
    let error = (candidate.invoke)(
        json!({"task_name":"bound-child", "message":"inspect"}),
        original,
    )
    .await
    .unwrap_err();
    assert_eq!(error, "verified native dispatch");
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(
        runtime
            .collaboration_lifecycle_bindings
            .read()
            .unwrap()
            .is_empty()
    );
}

/// A native backend pre-start rejection preserves cleanup semantics and permits name reuse.
/// 原生后端启动前拒绝保留清理语义，并允许复用名称。
/// Parameters: none. Returns: none; verifies the model-visible path twice without a Provider.
/// 参数：无。返回值：无；不使用 Provider，验证两次模型可见路径。
#[tokio::test]
async fn native_spawn_unclaimed_rejection_preserves_prestart_cleanup() {
    // The real native closure registers ownership before the rejecting backend is entered.
    // 真实原生闭包在进入拒绝后端之前登记归属。
    let runtime = Arc::new(TaskRuntime::new());
    // The original parent supplies the native tool's trusted session coordinate.
    // 原父会话提供原生工具的可信会话坐标。
    let parent = Uuid::new_v4();
    // Count backend admission to prove the same public name remains reusable.
    // 统计后端准入，以证明同一公开名称保持可复用。
    let calls = Arc::new(AtomicUsize::new(0));
    // Share only the counter with the rejecting backend closure.
    // 仅与拒绝后端闭包共享计数器。
    let observed = calls.clone();
    // Keep this exact backend installed throughout both calls.
    // 在两次调用期间持续安装此精确后端。
    let _backend = runtime
        .install_backend(
            parent,
            Arc::new(move |request: TaskExecutionRequest| {
                observed.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    Err(task_start_parent_stream_closed_error(
                        &request.public_task_id,
                    ))
                })
            }),
        )
        .unwrap();
    // Invoke the actual model-visible native tool and lifecycle registration.
    // 调用实际模型可见原生工具及生命周期登记。
    let candidate =
        build_spawn_agent_tool_candidate(runtime.clone(), Some(Arc::new(RejectingHost)));
    for _ in 0..2 {
        // The visible cause must retain the backend rejection after internal cleanup.
        // 内部清理之后的可见原因必须保留后端拒绝。
        let error = (candidate.invoke)(
            json!({"task_name":"retry-native", "message":"inspect"}),
            context(parent),
        )
        .await
        .unwrap_err();
        assert!(error.contains("parent live stream closed"));
        assert!(!error.contains("[task-prestart-cancelled]"));
        assert!(
            runtime
                .collaboration_lifecycle_bindings
                .read()
                .unwrap()
                .is_empty()
        );
        assert!(runtime.sessions.read().unwrap().is_empty());
        assert!(
            runtime
                .subagent_registry_snapshot()
                .unwrap()
                .list_all()
                .is_empty()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// Cancelling before backend claim must release the original transient binding.
/// 在后端认领前取消时必须释放原始临时绑定。
#[tokio::test]
async fn native_spawn_cancellation_clears_unclaimed_binding() {
    let runtime = Arc::new(TaskRuntime::new());
    let parent = Uuid::new_v4();
    let entered = Arc::new(tokio::sync::Notify::new());
    let entered_backend = entered.clone();
    let _backend = runtime
        .install_backend(
            parent,
            Arc::new(move |_: TaskExecutionRequest| {
                entered_backend.notify_one();
                Box::pin(std::future::pending())
            }),
        )
        .unwrap();
    let candidate =
        build_spawn_agent_tool_candidate(runtime.clone(), Some(Arc::new(RejectingHost)));
    let run = tokio::spawn(async move {
        (candidate.invoke)(
            json!({"task_name":"cancelled-child", "message":"inspect"}),
            context(parent),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .unwrap();
    assert_eq!(
        runtime
            .collaboration_lifecycle_bindings
            .read()
            .unwrap()
            .len(),
        1
    );
    run.abort();
    assert!(run.await.unwrap_err().is_cancelled());
    assert!(
        runtime
            .collaboration_lifecycle_bindings
            .read()
            .unwrap()
            .is_empty()
    );
}

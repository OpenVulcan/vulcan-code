//! Streaming host-owned tool executor used by provider transport wrappers.
//! 供 Provider 传输包装器使用的流式宿主工具执行器模块。

use crate::agent_runtime::events::{
    RuntimeEvent, RuntimeEventKind, ToolExecutionRecoveryBoundary, ToolExecutionRecoveryRequest,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_tool_event_format::{
    attach_tool_name_to_runtime_metadata, build_tool_result_runtime_metadata,
    format_host_tool_result_runtime_detail, format_tool_call_runtime_detail,
};
use crate::agent_runtime::tool_runtime_bridge::{
    VulcanManagedToolRuntimeBridge, attach_runtime_event_projection_barrier,
    build_tool_failure_invocation_result, build_tool_rejection_invocation_result,
    wait_runtime_event_projection_barrier, wait_tool_batch_projection_barrier,
};
use async_trait::async_trait;
use futures_util::FutureExt;
use serde_json::json;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, DispatchOutcome};
use vulcan_core::tool::VulcanToolExecutor;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};

use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;

/// Streaming tool executor that forwards host-side tool boundary events live while delegating the real tool invocation to the shared bridge.
/// 在把真实工具调用委派给共享桥接层的同时，实时转发宿主侧工具边界事件的流式工具执行器。
#[derive(Clone)]
pub(crate) struct StreamingRuntimeToolExecutor {
    /// Shared host-managed bridge that owns the real tool dispatch and metadata normalization.
    /// 持有真实工具调度与元数据规范化能力的共享宿主管理桥接层。
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Live sender used to forward tool boundary runtime events into the host stream.
    /// 用于把工具边界运行时事件转发进宿主流中的实时发送器。
    live_sender: ProviderLiveDeltaSender,
    /// Optional reason used to reject provider-requested tools before host execution.
    /// 在宿主执行前拒绝 Provider 请求工具时使用的可选原因。
    tool_request_rejection_reason: Option<String>,
    /// Whether the rejection reason applies to every tool.
    /// 拒绝原因是否应用到全部工具。
    tool_request_rejection_all_tools: bool,
    /// Optional scoped tool ids rejected with the rejection reason.
    /// 使用拒绝原因拒绝的可选限定工具标识。
    tool_request_rejection_tool_ids: Vec<String>,
}

impl StreamingRuntimeToolExecutor {
    /// Creates one streaming runtime tool executor from the shared bridge and live sender.
    /// 根据共享桥接层与实时发送器创建一份流式运行时工具执行器。
    pub(crate) fn new(
        runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
        live_sender: ProviderLiveDeltaSender,
        tool_request_rejection_reason: Option<String>,
        tool_request_rejection_all_tools: bool,
        tool_request_rejection_tool_ids: Vec<String>,
    ) -> Self {
        Self {
            runtime_bridge,
            live_sender,
            tool_request_rejection_reason,
            tool_request_rejection_all_tools,
            tool_request_rejection_tool_ids,
        }
    }

    /// Returns the runtime-policy rejection reason for one tool request when it is scoped out.
    /// 当单次工具请求被运行时策略排除时返回对应拒绝原因。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the provider-requested tool invocation.
    /// `request` 是 Provider 请求的工具调用。
    ///
    /// # Returns
    /// 返回值
    /// `Some(reason)` when the request should return a policy-rejected tool result.
    /// 当请求应返回策略拒绝工具结果时返回 `Some(reason)`。
    fn tool_request_rejection_message(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Option<String> {
        let reason = self.tool_request_rejection_reason.as_ref()?;
        let should_reject = self.tool_request_rejection_all_tools
            || self
                .tool_request_rejection_tool_ids
                .iter()
                .any(|tool_id| tool_id == &request.tool_name);
        should_reject.then(|| reason.clone())
    }
}

#[async_trait]
impl VulcanToolExecutor for StreamingRuntimeToolExecutor {
    /// Publishes one complete durable recovery batch before any concurrent tool future starts.
    /// 在任何并发工具 Future 启动前发布一批完整的可靠恢复请求。
    async fn prepare_tool_batch(
        &self,
        requests: &[CoreToolInvocationRequest],
    ) -> Result<(), VulcanCoreError> {
        let recovery_requests = requests
            .iter()
            .map(|request| {
                let request_stream_id = request.request_stream_id.clone();
                self.runtime_bridge
                    .binding_fingerprint_for_request(request)
                    .map(|binding_fingerprint| ToolExecutionRecoveryRequest {
                        request_stream_id,
                        request: request.clone(),
                        binding_fingerprint,
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| VulcanCoreError::InvalidState { message })?;
        let (batch_event, projection_barrier) = attach_runtime_event_projection_barrier(
            RuntimeEvent::new(
                RuntimeEventKind::ToolBatchPrepared,
                "Tool batch recovery boundary",
                "The complete tool batch is waiting for durable recovery persistence.",
            )
            .with_tool_execution_boundary(
                ToolExecutionRecoveryBoundary::BatchPrepared(recovery_requests),
            ),
        )
        .map_err(|message| VulcanCoreError::InvalidState { message })?;
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RuntimeEvent(batch_event)));
        let identities = wait_tool_batch_projection_barrier(projection_barrier)
            .await
            .map_err(|error| VulcanCoreError::ToolDurabilityFailure {
                tool_name: String::from("tool_batch"),
                message: error.to_string(),
                details: None,
            })?;
        self.runtime_bridge
            .install_prepared_invocation_identities(requests, identities)
            .map_err(|message| VulcanCoreError::InvalidState { message })
    }

    /// Executes one host-managed tool request, forwards start and finish runtime events, and returns the normalized tool result to the controller.
    /// 执行一次宿主管理工具请求，转发开始与完成运行时事件，并将规范化工具结果返回给控制器。
    async fn execute_tool(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let (tool_started_event, projection_barrier) = attach_runtime_event_projection_barrier(
            RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!(
                    "工具调用 · {} · {}",
                    request.tool_call_id, request.tool_name
                ),
                format_tool_call_runtime_detail(&request.tool_name, &request.arguments),
            )
            .with_tool_execution_boundary(ToolExecutionRecoveryBoundary::Started {
                request: request.clone(),
                binding_fingerprint: self
                    .runtime_bridge
                    .binding_fingerprint_for_request(request)
                    .map_err(|message| VulcanCoreError::InvalidState { message })?,
            })
            .with_metadata(Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            }))),
        )
        .map_err(|message| VulcanCoreError::InvalidState { message })?;
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RuntimeEvent(tool_started_event)));
        // Wait for the host bridge to project the start card before the real tool body can begin.
        // 在真实工具主体开始前等待宿主桥接层完成开始卡片投影。
        let has_projection_barrier = projection_barrier.is_some();
        wait_runtime_event_projection_barrier(projection_barrier)
            .await
            .map_err(|error| VulcanCoreError::ToolDurabilityFailure {
                tool_name: request.tool_name.clone(),
                message: error.to_string(),
                details: None,
            })?;
        if !has_projection_barrier {
            tokio::task::yield_now().await;
        }
        let (result, display_state) =
            if let Some(reason) = self.tool_request_rejection_message(request) {
                (
                    build_tool_rejection_invocation_result(request, reason),
                    "completed",
                )
            } else {
                match AssertUnwindSafe(self.runtime_bridge.execute_tool(request))
                    .catch_unwind()
                    .await
                {
                    Ok(Ok(result)) => {
                        let display_state = if result.is_error {
                            "failed"
                        } else {
                            "completed"
                        };
                        (result, display_state)
                    }
                    Ok(Err(error)) => (
                        build_tool_failure_invocation_result(request, error.to_string()),
                        "failed",
                    ),
                    Err(payload) => (
                        build_tool_failure_invocation_result(
                            request,
                            format!(
                                "Tool runtime panicked before returning a normal result: {}",
                                panic_payload_message(payload.as_ref())
                            ),
                        ),
                        "failed",
                    ),
                }
            };
        let event_metadata = attach_tool_name_to_runtime_metadata(
            Some(build_tool_result_runtime_metadata(
                &request.tool_call_id,
                result.host_result.as_ref(),
                display_state,
            )),
            &request.tool_name,
        );
        let (tool_finished_event, projection_barrier) = attach_runtime_event_projection_barrier(
            RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", request.tool_call_id),
                format_host_tool_result_runtime_detail(&result.model_output),
            )
            .with_tool_execution_boundary(ToolExecutionRecoveryBoundary::Finished(result.clone()))
            .with_metadata(event_metadata),
        )
        .map_err(|message| tool_result_persistence_failure(&result, message))?;
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RuntimeEvent(tool_finished_event)));
        // Wait for the host bridge to observe terminal tool state before Core enters the next provider turn.
        // 在 Core 进入下一次 Provider 回合前，等待宿主桥接层先观察到工具终态。
        wait_runtime_event_projection_barrier(projection_barrier)
            .await
            .map_err(|error| tool_result_persistence_failure(&result, error.to_string()))?;
        Ok(result)
    }
}

/// Preserves execution evidence when persisting a returned tool result fails.
/// 工具返回结果的持久化失败时，保留已执行事实。
/// `result` is the actual returned receipt; `cause` is the persistence rejection.
/// `result` 是实际返回的回执；`cause` 是持久化拒绝原因。
/// Returns a terminal error that cannot trigger automatic replay of applied changes.
/// 返回禁止自动重放已应用变更的终态错误。
pub(crate) fn tool_result_persistence_failure(
    result: &CoreToolInvocationResult,
    cause: String,
) -> VulcanCoreError {
    // Only an applied ChangeSet proves file effects; preview and ordinary results do not.
    // 仅已应用的变更集能够证明文件副作用；预览和普通结果不能证明。
    let applied = result
        .host_result
        .as_ref()
        .is_some_and(|host| host["kind"] == "change_set" && host["payload"]["mode"] == "applied");
    // Keep the recovery instruction separate from the original rejection diagnostic.
    // 将恢复说明与原始拒绝诊断分离。
    let instruction = if applied {
        "The file changes were already applied. Do not repeat the operation to recover its receipt."
    } else {
        "The tool already returned. Inspect its actual outcome before retrying; persistence failure does not prove that execution had no effect."
    };
    // The route wrapper supplies provider identity; this boundary owns only execution facts.
    // 路由包装器负责供应商标识；当前边界仅负责执行事实。
    let message = format!(
        "Tool `{}` result persistence failed: {cause}. {instruction}",
        result.tool_name
    );
    VulcanCoreError::ToolDurabilityFailure {
        tool_name: result.tool_name.clone(),
        message: message.clone(),
        details: Some(Box::new(AiExecutionError {
            category: AiErrorCategory::Unclassified,
            subcategory: Some(String::from("tool_result_persistence")),
            stage: String::from("terminal"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: Some(String::from("tool_result_persistence_failed")),
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: Some(cause),
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message,
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: applied,
            terminal: true,
            retryable: false,
            retry_after: None,
        })),
    }
}

/// Converts one panic payload into a stable diagnostic string that can be exposed as a tool error.
/// 将一个 panic 载荷转换为可作为工具错误暴露的稳定诊断字符串。
fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    String::from("unknown panic payload")
}

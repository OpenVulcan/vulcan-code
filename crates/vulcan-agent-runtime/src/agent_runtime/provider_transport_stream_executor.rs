//! Streaming host-owned tool executor used by provider transport wrappers.
//! 供 Provider 传输包装器使用的流式宿主工具执行器模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider_protocol_bridge::{
    attach_tool_name_to_runtime_metadata, build_host_tool_result_id,
    build_tool_result_runtime_metadata, format_host_tool_result_runtime_detail,
    format_tool_call_runtime_detail,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::tool_runtime_bridge::{
    VulcanManagedToolRuntimeBridge, attach_runtime_event_projection_barrier,
    build_tool_failure_invocation_result, wait_runtime_event_projection_barrier,
};
use async_trait::async_trait;
use futures_util::FutureExt;
use serde_json::json;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
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
}

impl StreamingRuntimeToolExecutor {
    /// Creates one streaming runtime tool executor from the shared bridge and live sender.
    /// 根据共享桥接层与实时发送器创建一份流式运行时工具执行器。
    pub(crate) fn new(
        runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
        live_sender: ProviderLiveDeltaSender,
    ) -> Self {
        Self {
            runtime_bridge,
            live_sender,
        }
    }
}

#[async_trait]
impl VulcanToolExecutor for StreamingRuntimeToolExecutor {
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
            .with_metadata(Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            }))),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RuntimeEvent(tool_started_event)));
        // Wait for the host bridge to project the start card before the real tool body can begin.
        // 在真实工具主体开始前等待宿主桥接层完成开始卡片投影。
        let has_projection_barrier = projection_barrier.is_some();
        wait_runtime_event_projection_barrier(projection_barrier).await;
        if !has_projection_barrier {
            tokio::task::yield_now().await;
        }
        let (result, display_state) =
            match AssertUnwindSafe(self.runtime_bridge.execute_tool(request))
                .catch_unwind()
                .await
            {
                Ok(Ok(result)) => (result, "completed"),
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
            };
        let tool_result_id = build_host_tool_result_id(&request.tool_call_id);
        let event_metadata = attach_tool_name_to_runtime_metadata(
            build_tool_result_runtime_metadata(
                &request.tool_call_id,
                &tool_result_id,
                result.host_result.as_ref(),
                display_state,
            ),
            &request.tool_name,
        );
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
            RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", request.tool_call_id),
                format_host_tool_result_runtime_detail(&tool_result_id, &result.model_output),
            )
            .with_metadata(event_metadata),
        )));
        Ok(result)
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

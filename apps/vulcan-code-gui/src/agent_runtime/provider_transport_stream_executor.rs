//! Streaming host-owned tool executor used by provider transport wrappers.
//! 供 Provider 传输包装器使用的流式宿主工具执行器模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider_protocol_bridge::{
    attach_tool_name_to_runtime_metadata, build_host_tool_result_id,
    build_tool_result_runtime_metadata, format_host_tool_result_runtime_detail,
    format_tool_call_runtime_detail,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use async_trait::async_trait;
use serde_json::json;
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
    /// Live sender used to forward tool boundary runtime events into the GUI stream.
    /// 用于把工具边界运行时事件转发进图形界面流中的实时发送器。
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
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
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
        )));
        let result = self.runtime_bridge.execute_tool(request).await?;
        let tool_result_id = build_host_tool_result_id(&request.tool_call_id);
        let event_metadata = attach_tool_name_to_runtime_metadata(
            build_tool_result_runtime_metadata(
                &request.tool_call_id,
                &tool_result_id,
                result.host_result.as_ref(),
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

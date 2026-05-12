//! Rig-facing dynamic tool adapters backed by the standalone Vulcan tools manager crate.
//! 基于独立 VulcanToolsManager crate 的面向 rig 的动态工具适配器。

use crate::model::SessionId;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationResult, VulcanToolsManager,
};

/// Lightweight tool-call error wrapper used when one host-managed tool fails inside rig.
/// 当单个宿主管理工具在 rig 内部执行失败时使用的轻量工具错误包装器。
#[derive(Debug)]
struct VulcanRigToolError {
    /// Human-readable error message forwarded to the rig tool runtime.
    /// 转发给 rig 工具运行时的人类可读错误消息。
    message: String,
}

impl fmt::Display for VulcanRigToolError {
    /// Render the rig-facing host-managed tool error as one human-readable message.
    /// 将面向 rig 的宿主管理工具错误渲染为一条人类可读消息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for VulcanRigToolError {}

/// Recorded host-side tool outcome captured during one rig-backed execution turn.
/// 在单次 rig 驱动执行回合中捕获的宿主侧工具结果记录。
#[derive(Debug, Clone)]
pub struct RecordedRigToolOutcome {
    /// Optional host-only structured metadata preserved outside the model-visible text.
    /// 保留在模型可见文本之外的可选宿主专用结构化元数据。
    pub metadata: Option<Value>,
}

/// Shared FIFO capture used to bridge host-only tool metadata back into GUI runtime events.
/// 用于把宿主专用工具元数据桥接回 GUI 运行时事件的共享先进先出捕获器。
#[derive(Debug, Clone, Default)]
pub struct RigToolOutcomeCapture {
    /// Shared queue that preserves tool execution order within one run.
    /// 在单次运行内保留工具执行顺序的共享队列。
    queue: Arc<Mutex<VecDeque<RecordedRigToolOutcome>>>,
}

impl RigToolOutcomeCapture {
    /// Records one completed tool outcome so the executor can attach metadata to the matching runtime event later.
    /// 记录一条已完成工具结果，便于执行器稍后把元数据附着到对应运行时事件上。
    pub fn record(&self, definition: &ResolvedToolDefinition, result: &VulcanToolInvocationResult) {
        let metadata = result.host_result.as_ref().map(|host_result| {
            json!({
                "tool_name": definition.descriptor.id.clone(),
                "tool_source": format!("{:?}", definition.descriptor.source).to_ascii_lowercase(),
                "host_result": {
                    "kind": host_result.kind.clone(),
                    "payload": host_result.payload.clone(),
                },
            })
        });
        let outcome = RecordedRigToolOutcome { metadata };
        let mut queue = self.queue.lock().unwrap_or_else(|poison| poison.into_inner());
        queue.push_back(outcome);
    }

    /// Records one empty placeholder so tool-result ordering remains stable even when host execution fails before metadata exists.
    /// 记录一条空占位结果，确保宿主执行在元数据生成前失败时工具返回顺序仍然稳定。
    pub fn record_placeholder(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(|poison| poison.into_inner());
        queue.push_back(RecordedRigToolOutcome { metadata: None });
    }

    /// Takes the next recorded outcome in execution order for one incoming tool-result runtime event.
    /// 按执行顺序取出下一条已记录结果，供单条工具返回运行时事件使用。
    pub fn take_next(&self) -> Option<RecordedRigToolOutcome> {
        let mut queue = self.queue.lock().unwrap_or_else(|poison| poison.into_inner());
        queue.pop_front()
    }
}

/// One dynamic rig tool backed by one resolved host-managed tool definition.
/// 由单个已解析宿主管理工具定义驱动的动态 rig 工具。
pub struct VulcanManagedRigTool {
    /// Shared host-owned unified tools manager used for the final dispatch.
    /// 用于最终调度的共享宿主自有统一工具管理器。
    manager: Arc<VulcanToolsManager>,
    /// Stable resolved tool definition mirrored into the rig tool definition.
    /// 被镜像到 rig 工具定义中的稳定已解析工具定义。
    definition: ResolvedToolDefinition,
    /// Optional GUI session identifier used for request-scoped diagnostics.
    /// 用于请求级诊断的可选图形界面会话标识。
    session_id: SessionId,
    /// Shared host-only outcome capture used to reattach metadata after rig emits the tool result.
    /// 在 rig 发出工具结果后重新附着元数据时使用的共享宿主专用结果捕获器。
    outcome_capture: RigToolOutcomeCapture,
}

impl VulcanManagedRigTool {
    /// Builds one rig-facing dynamic tool adapter from the host-managed tool definition.
    /// 根据宿主管理工具定义构建单个面向 rig 的动态工具适配器。
    pub fn new(
        manager: Arc<VulcanToolsManager>,
        definition: ResolvedToolDefinition,
        session_id: SessionId,
        outcome_capture: RigToolOutcomeCapture,
    ) -> Self {
        Self {
            manager,
            definition,
            session_id,
            outcome_capture,
        }
    }
}

impl ToolDyn for VulcanManagedRigTool {
    /// Return the stable host-managed tool identifier as the rig tool identifier.
    /// 返回稳定宿主管理工具标识，作为 rig 工具标识。
    fn name(&self) -> String {
        self.definition.descriptor.id.clone()
    }

    /// Build one rig tool definition from the host-managed resolved tool definition.
    /// 根据宿主管理的已解析工具定义构建单个 rig 工具定义。
    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.definition.descriptor.id.clone(),
                description: self.definition.descriptor.description.clone(),
                parameters: self.definition.parameters_schema.clone(),
            }
        })
    }

    /// Dispatch the tool call through the shared host-owned tools manager and serialize the structured result for rig.
    /// 通过共享宿主自有工具管理器调度工具调用，并为 rig 序列化结构化结果。
    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let args_json: Value = match serde_json::from_str(&args) {
                Ok(args_json) => args_json,
                Err(error) => {
                    self.outcome_capture.record_placeholder();
                    return Err(ToolError::JsonError(error));
                }
            };
            let result = self
                .manager
                .invoke_tool(
                    &self.definition,
                    args_json,
                    Some(self.session_id.to_string()),
                )
                .await
                .map_err(|error| {
                    self.outcome_capture.record_placeholder();
                    ToolError::ToolCallError(Box::new(VulcanRigToolError { message: error }))
                })?;
            self.outcome_capture.record(&self.definition, &result);
            match result.model_output {
                Value::String(text) => Ok(text),
                other => serde_json::to_string(&other).map_err(ToolError::JsonError),
            }
        })
    }
}

/// Build the rig-facing unified dynamic tool set for one GUI session.
/// 为单个图形界面会话构建面向 rig 的统一动态工具集合。
pub fn build_vulcan_rig_tools(
    manager: VulcanToolsManager,
    session_id: SessionId,
    definitions: &[ResolvedToolDefinition],
    outcome_capture: RigToolOutcomeCapture,
) -> Vec<Box<dyn ToolDyn>> {
    let manager = Arc::new(manager);

    definitions
        .iter()
        .cloned()
        .map(|definition| {
            Box::new(VulcanManagedRigTool::new(
                manager.clone(),
                definition,
                session_id,
                outcome_capture.clone(),
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

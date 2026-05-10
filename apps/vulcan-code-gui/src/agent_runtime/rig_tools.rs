//! Rig-facing dynamic tool adapters backed by the standalone Vulcan tools manager crate.
//! 基于独立 VulcanToolsManager crate 的面向 rig 的动态工具适配器。

use crate::model::SessionId;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::Value;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

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
}

impl VulcanManagedRigTool {
    /// Builds one rig-facing dynamic tool adapter from the host-managed tool definition.
    /// 根据宿主管理工具定义构建单个面向 rig 的动态工具适配器。
    pub fn new(
        manager: Arc<VulcanToolsManager>,
        definition: ResolvedToolDefinition,
        session_id: SessionId,
    ) -> Self {
        Self {
            manager,
            definition,
            session_id,
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
            let args_json: Value = serde_json::from_str(&args)?;
            let result = self
                .manager
                .invoke_tool(
                    &self.definition,
                    args_json,
                    Some(self.session_id.to_string()),
                )
                .await
                .map_err(|error| {
                    ToolError::ToolCallError(Box::new(VulcanRigToolError { message: error }))
                })?;
            match result {
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
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

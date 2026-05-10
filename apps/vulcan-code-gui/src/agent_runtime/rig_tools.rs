//! Rig-facing dynamic tool adapters backed by the embedded LuaSkills runtime.
//! 基于嵌入式 LuaSkills 运行时的面向 rig 的动态工具适配器。

use crate::luaskills_runtime::{
    LuaSkillsRuntimeHandle, build_luaskills_tool_invocation_context, call_luaskills_tool_entry,
    luaskills_entry_args_schema, serialize_luaskills_invocation_result,
};
use crate::mcp::runtime::{McpRuntimeHandle, McpRuntimeToolDescriptor};
use luaskills::RuntimeEntryDescriptor;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::Value;
use std::error::Error;
use std::fmt;

/// Lightweight tool-call error wrapper used when one LuaSkills entry fails inside rig.
/// 当单个 LuaSkills 入口在 rig 内部执行失败时使用的轻量工具错误包装器。
#[derive(Debug)]
struct LuaSkillsRigToolError {
    /// Human-readable error message forwarded to the rig tool runtime.
    /// 转发给 rig 工具运行时的人类可读错误消息。
    message: String,
}

impl fmt::Display for LuaSkillsRigToolError {
    /// Render the rig-facing LuaSkills tool error as one human-readable message.
    /// 将面向 rig 的 LuaSkills 工具错误渲染为一条人类可读消息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for LuaSkillsRigToolError {}

/// One dynamic rig tool backed by a single LuaSkills runtime entry.
/// 由单个 LuaSkills 运行时入口驱动的动态 rig 工具。
pub struct LuaSkillsRigTool {
    /// Shared embedded LuaSkills runtime handle reused across multiple tool calls.
    /// 在多个工具调用间复用的共享嵌入式 LuaSkills 运行时句柄。
    handle: LuaSkillsRuntimeHandle,
    /// Runtime entry descriptor mirrored into the rig tool definition.
    /// 被镜像到 rig 工具定义中的运行时入口描述。
    entry: RuntimeEntryDescriptor,
    /// Optional GUI session identifier used for request-scoped audit metadata.
    /// 用于请求级审计元数据的可选图形界面会话标识。
    session_id: Option<usize>,
}

impl LuaSkillsRigTool {
    /// Build one rig-facing LuaSkills tool adapter from the shared runtime handle and entry descriptor.
    /// 根据共享运行时句柄与入口描述构建单个面向 rig 的 LuaSkills 工具适配器。
    pub fn new(
        handle: LuaSkillsRuntimeHandle,
        entry: RuntimeEntryDescriptor,
        session_id: Option<usize>,
    ) -> Self {
        Self {
            handle,
            entry,
            session_id,
        }
    }
}

impl ToolDyn for LuaSkillsRigTool {
    /// Return the canonical LuaSkills runtime entry name as the rig tool identifier.
    /// 返回 canonical LuaSkills 运行时入口名称，作为 rig 工具标识。
    fn name(&self) -> String {
        self.entry.canonical_name.clone()
    }

    /// Build one rig tool definition from the LuaSkills runtime entry descriptor.
    /// 根据 LuaSkills 运行时入口描述构建单个 rig 工具定义。
    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.entry.canonical_name.clone(),
                description: self.entry.description.clone(),
                parameters: luaskills_entry_args_schema(&self.entry),
            }
        })
    }

    /// Invoke the LuaSkills runtime entry through the embedded host runtime and serialize the structured result for rig.
    /// 通过嵌入式宿主运行时调用 LuaSkills 运行时入口，并为 rig 序列化结构化结果。
    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let args_json: Value = serde_json::from_str(&args)?;
            let invocation_context = build_luaskills_tool_invocation_context(
                self.session_id,
                "rig-tool",
                &self.entry.canonical_name,
            );
            let result = call_luaskills_tool_entry(
                self.handle.clone(),
                self.entry.canonical_name.clone(),
                args_json,
                Some(invocation_context),
            )
            .await
            .map_err(|error| {
                ToolError::ToolCallError(Box::new(LuaSkillsRigToolError { message: error }))
            })?;
            serde_json::to_string(&serialize_luaskills_invocation_result(&result))
                .map_err(ToolError::JsonError)
        })
    }
}

/// Build the rig-facing LuaSkills dynamic tool set for one GUI session.
/// 为单个图形界面会话构建面向 rig 的 LuaSkills 动态工具集合。
pub fn build_luaskills_rig_tools(
    handle: Option<LuaSkillsRuntimeHandle>,
    session_id: usize,
    entries: &[RuntimeEntryDescriptor],
) -> Vec<Box<dyn ToolDyn>> {
    let Some(handle) = handle else {
        return Vec::new();
    };

    entries
        .iter()
        .cloned()
        .map(|entry| {
            Box::new(LuaSkillsRigTool::new(
                handle.clone(),
                entry,
                Some(session_id),
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

/// One dynamic rig tool backed by one live MCP runtime descriptor.
/// 由单个实时 MCP 运行时描述驱动的动态 rig 工具。
pub struct McpRigTool {
    /// Shared MCP runtime handle used to resolve the latest live transport at call time.
    /// 用于在调用时解析最新活动传输的共享 MCP 运行时句柄。
    handle: McpRuntimeHandle,
    /// Stable MCP tool descriptor mirrored into the rig definition.
    /// 被镜像到 rig 定义中的稳定 MCP 工具描述。
    tool: McpRuntimeToolDescriptor,
    /// Optional GUI session identifier used for future request-scoped diagnostics.
    /// 供未来请求级诊断使用的可选图形界面会话标识。
    session_id: usize,
}

impl McpRigTool {
    /// Build one rig-facing MCP tool adapter from the shared runtime handle and tool descriptor.
    /// 根据共享运行时句柄与工具描述构建单个面向 rig 的 MCP 工具适配器。
    pub fn new(
        handle: McpRuntimeHandle,
        tool: McpRuntimeToolDescriptor,
        session_id: usize,
    ) -> Self {
        Self {
            handle,
            tool,
            session_id,
        }
    }
}

impl ToolDyn for McpRigTool {
    /// Return the remote MCP tool name as the rig tool identifier.
    /// 返回远端 MCP 工具名，作为 rig 工具标识。
    fn name(&self) -> String {
        self.tool.schema.name.clone()
    }

    /// Build one rig tool definition from the discovered MCP schema.
    /// 根据已发现的 MCP schema 构建单个 rig 工具定义。
    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.tool.schema.name.clone(),
                description: self.tool.schema.description.clone(),
                parameters: self.tool.schema.args_schema.clone(),
            }
        })
    }

    /// Resolve the latest live transport for the owning server and invoke the remote MCP tool.
    /// 为所属服务器解析最新活动传输并调用远端 MCP 工具。
    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let args_json: Value = serde_json::from_str(&args)?;
            let Some(transport) = self.handle.transport_for_server(&self.tool.server_id) else {
                return Err(ToolError::ToolCallError(Box::new(LuaSkillsRigToolError {
                    message: format!(
                        "MCP 服务器 `{}` 当前未连接，无法调用工具 `{}`（session={}）。",
                        self.tool.server_name, self.tool.schema.name, self.session_id
                    ),
                })));
            };
            let result = transport
                .call_tool(&self.tool.schema.name, args_json)
                .await
                .map_err(|error| {
                    ToolError::ToolCallError(Box::new(LuaSkillsRigToolError {
                        message: format!(
                            "MCP 工具 `{}` 调用失败：{}。",
                            self.tool.schema.name, error
                        ),
                    }))
                })?;
            serde_json::to_string(&result).map_err(ToolError::JsonError)
        })
    }
}

/// Build the rig-facing MCP dynamic tool set for one GUI session.
/// 为单个图形界面会话构建面向 rig 的 MCP 动态工具集合。
pub fn build_mcp_rig_tools(
    handle: Option<McpRuntimeHandle>,
    session_id: usize,
    entries: &[McpRuntimeToolDescriptor],
) -> Vec<Box<dyn ToolDyn>> {
    let Some(handle) = handle else {
        return Vec::new();
    };

    entries
        .iter()
        .cloned()
        .map(|entry| {
            Box::new(McpRigTool::new(handle.clone(), entry, session_id)) as Box<dyn ToolDyn>
        })
        .collect()
}

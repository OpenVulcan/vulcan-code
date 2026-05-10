//! Host-owned Model Context Protocol transport abstraction.
//! 宿主自有的 Model Context Protocol 传输抽象。

use crate::McpRuntimeError;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::broadcast;

/// Stable MCP tool schema exposed by one remote endpoint.
/// 单个远端端点暴露出的稳定 MCP 工具 Schema。
#[derive(Debug, Clone)]
pub struct McpToolSchema {
    /// Stable tool name.
    /// 稳定工具名称。
    pub name: String,
    /// Human-readable tool description.
    /// 人类可读工具描述。
    pub description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub args_schema: Value,
    /// JSON Schema describing the tool result when the remote server provides it.
    /// 当远端服务器提供结果 schema 时，对应的 JSON Schema。
    pub result_schema: Value,
}

/// MCP transport lifecycle event broadcast to host runtimes.
/// 广播给宿主运行时的 MCP 传输生命周期事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransportEvent {
    /// Remote server tool list changed and should be re-discovered.
    /// 远端服务器工具列表已变化，需要重新发现。
    ToolsChanged,
    /// Remote server transport disconnected and its registered tools should be withdrawn.
    /// 远端服务器传输已断开，其已注册工具应被撤回。
    Disconnected {
        /// Human-readable disconnect reason surfaced for diagnostics.
        /// 供诊断使用的人类可读断开原因。
        reason: String,
    },
}

/// Bidirectional host-owned MCP transport abstraction.
/// 宿主自有的双向 MCP 传输抽象。
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Stable identifier for this transport instance.
    /// 当前传输实例的稳定标识。
    fn endpoint(&self) -> &str;

    /// Discover the tools exposed by the remote endpoint.
    /// 发现远端端点暴露的工具集合。
    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError>;

    /// Subscribe to transport lifecycle events emitted after the initial connection succeeds.
    /// 订阅初始连接成功之后发出的传输生命周期事件。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent>;

    /// Invoke one named tool with a JSON argument object.
    /// 使用一个 JSON 参数对象调用单个具名工具。
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError>;
}

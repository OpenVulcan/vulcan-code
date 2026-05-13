//! Host-owned Model Context Protocol transport abstraction.
//! 宿主自有的 Model Context Protocol 传输抽象。

use crate::McpRuntimeError;
use crate::auth::{McpAuthChallenge, McpAuthStatus};
use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
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
    /// Remote server prompt list changed and should be re-discovered.
    /// 远端服务器 prompt 列表已变化，需要重新发现。
    PromptsChanged,
    /// Remote server resource list changed and should be re-discovered.
    /// 远端服务器资源列表已变化，需要重新发现。
    ResourcesChanged,
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

    /// Discover the prompts exposed by the remote endpoint.
    /// 发现远端端点暴露的 prompt 集合。
    async fn list_prompts(&self) -> Result<Vec<Prompt>, McpRuntimeError> {
        Ok(Vec::new())
    }

    /// Resolve one named prompt with an optional JSON-object argument payload.
    /// 使用可选 JSON 对象参数载荷解析单个具名 prompt。
    async fn get_prompt(
        &self,
        _name: &str,
        _args: Value,
    ) -> Result<GetPromptResult, McpRuntimeError> {
        Err(McpRuntimeError::transport(format!(
            "MCP 传输 `{}` 不支持 prompts/get。",
            self.endpoint()
        )))
    }

    /// Discover the resources exposed by the remote endpoint.
    /// 发现远端端点暴露的资源集合。
    async fn list_resources(&self) -> Result<Vec<Resource>, McpRuntimeError> {
        Ok(Vec::new())
    }

    /// Read one resource by URI from the remote endpoint.
    /// 通过 URI 从远端端点读取单个资源。
    async fn read_resource(&self, _uri: &str) -> Result<ReadResourceResult, McpRuntimeError> {
        Err(McpRuntimeError::transport(format!(
            "MCP 传输 `{}` 不支持 resources/read。",
            self.endpoint()
        )))
    }

    /// Subscribe to transport lifecycle events emitted after the initial connection succeeds.
    /// 订阅初始连接成功之后发出的传输生命周期事件。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent>;

    /// Returns whether the transport supports a host-driven OAuth lifecycle.
    /// 返回当前传输是否支持宿主驱动的 OAuth 生命周期。
    async fn supports_oauth(&self) -> bool {
        false
    }

    /// Returns whether the transport currently retains stored OAuth tokens.
    /// 返回当前传输是否保留了已存储的 OAuth 令牌。
    async fn has_stored_tokens(&self) -> Result<bool, McpRuntimeError> {
        Ok(false)
    }

    /// Returns the current host-side OAuth status for the transport.
    /// 返回当前传输对应的宿主侧 OAuth 状态。
    async fn auth_status(&self) -> Result<McpAuthStatus, McpRuntimeError> {
        Ok(McpAuthStatus::NotConfigured)
    }

    /// Starts one host-driven OAuth authorization flow and returns the browser challenge payload.
    /// 启动一条宿主驱动的 OAuth 授权流程，并返回浏览器挑战载荷。
    async fn start_auth(&self, _client_name: &str) -> Result<McpAuthChallenge, McpRuntimeError> {
        Err(McpRuntimeError::transport(format!(
            "MCP 传输 `{}` 不支持 OAuth 授权。",
            self.endpoint()
        )))
    }

    /// Finishes one host-driven OAuth authorization flow with the returned code and state.
    /// 使用回传的授权码与 state 完成一条宿主驱动的 OAuth 授权流程。
    async fn finish_auth(
        &self,
        _authorization_code: &str,
        _oauth_state: &str,
    ) -> Result<McpAuthStatus, McpRuntimeError> {
        Err(McpRuntimeError::transport(format!(
            "MCP 传输 `{}` 不支持 OAuth 授权完成。",
            self.endpoint()
        )))
    }

    /// Removes every stored OAuth credential retained by the transport.
    /// 移除当前传输保留的全部 OAuth 凭据。
    async fn remove_auth(&self) -> Result<(), McpRuntimeError> {
        Err(McpRuntimeError::transport(format!(
            "MCP 传输 `{}` 不支持 OAuth 凭据清理。",
            self.endpoint()
        )))
    }

    /// Invoke one named tool with a JSON argument object.
    /// 使用一个 JSON 参数对象调用单个具名工具。
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError>;
}

//! Host-owned Model Context Protocol transport abstraction.
//! 宿主自有的 Model Context Protocol 传输抽象。

use crate::McpRuntimeError;
use crate::auth::{McpAuthChallenge, McpAuthStatus};
use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use serde_json::Value;
use tokio::sync::broadcast;

/// Selects a result projection without changing MCP requests or machine-call contracts.
/// 选择结果投影，不改变 MCP 请求或机器调用契约。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolResultAudience {
    /// Preserve the existing structured result for programmatic consumers.
    /// 为程序消费者保留既有结构化结果。
    Machine,
    /// Preserve all conversation content blocks for model reading.
    /// 保留全部会话内容块供模型阅读。
    Conversation,
}

/// Negotiated MCP server identity returned by the initialize handshake.
/// MCP initialize 握手返回的已协商服务器身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerIdentity {
    /// Negotiated MCP protocol version.
    /// 已协商的 MCP 协议版本。
    pub protocol_version: String,
    /// Server implementation product name.
    /// 服务器实现的产品名称。
    pub product: String,
    /// Server implementation binary version.
    /// 服务器实现的二进制版本。
    pub version: String,
}

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

    /// Returns a bounded sanitized diagnostic log snapshot owned by this transport.
    /// 返回当前传输持有的有界脱敏诊断日志快照。
    fn recent_logs(&self) -> Vec<String> {
        Vec::new()
    }

    /// Returns the identity negotiated during initialize when the transport exposes it.
    /// 当传输能够提供时，返回 initialize 阶段协商得到的服务器身份。
    fn server_identity(&self) -> Option<McpServerIdentity> {
        None
    }

    /// Gracefully closes this exact transport and waits for owned resources to be released.
    /// 优雅关闭当前精确传输，并等待其持有资源完成释放。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        Ok(())
    }

    /// Requests shutdown and waits for the final cleanup result without a caller observation deadline.
    /// 请求关闭并等待最终清理结果，不附加调用方观测截止时间。
    ///
    /// No inputs; returns final success or cleanup failure, never an observation-only timeout.
    /// 无输入；返回最终成功或清理失败，不能返回仅属于观测方的超时。
    /// The default preserves custom transports whose shutdown already reports final cleanup.
    /// 默认实现沿用已经报告最终清理结果的自定义传输关闭接口。
    /// Implementations with bounded shutdown observations must override this method to await the original operation.
    /// 关闭观测有界的实现必须覆盖此方法，继续等待原操作。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.shutdown().await
    }

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

    /// Invokes one tool for the selected consumer; opaque custom transports retain their contract.
    /// 为选定消费者调用工具；不透明自定义传输保留自身契约。
    /// Inputs identify the tool, arguments and audience; returns one result without a second call.
    /// 输入标识工具、参数与消费者；返回单份结果，不进行二次调用。
    async fn call_tool_with_audience(
        &self,
        name: &str,
        args: Value,
        _audience: McpToolResultAudience,
    ) -> Result<Value, McpRuntimeError> {
        self.call_tool(name, args).await
    }
}

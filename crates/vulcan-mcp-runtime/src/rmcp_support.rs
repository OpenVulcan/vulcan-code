//! Shared rmcp-side support helpers reused by multiple MCP client transports.
//! 被多个 MCP 客户端传输复用的共享 rmcp 侧支持辅助模块。

use std::time::Duration;

use rmcp::ClientHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, JsonObject,
    Prompt, ReadResourceRequestParams, ReadResourceResult, Resource, ServerCapabilities,
    Tool as RmcpTool,
};
use rmcp::service::{NotificationContext, Peer, RoleClient};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::error::McpRuntimeError;
use crate::transport::{McpToolSchema, McpTransportEvent};

/// Shared rmcp notification adapter that forwards tool-list changes into the host runtime event stream.
/// 将工具列表变化转发到宿主运行时事件流的共享 rmcp 通知适配器。
#[derive(Clone)]
pub(crate) struct EventingClientHandler {
    /// Broadcast channel used to emit MCP transport lifecycle events.
    /// 用于发出 MCP 传输生命周期事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl EventingClientHandler {
    /// Create one eventing rmcp client handler from a shared broadcast sender.
    /// 根据共享广播发送端创建一个带事件能力的 rmcp 客户端处理器。
    pub(crate) fn new(event_sender: broadcast::Sender<McpTransportEvent>) -> Self {
        Self { event_sender }
    }
}

impl ClientHandler for EventingClientHandler {
    /// Forward `tools/list_changed` notifications into the host runtime event stream.
    /// 将 `tools/list_changed` 通知转发到宿主运行时事件流。
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::ToolsChanged);
    }

    /// Forward `prompts/list_changed` notifications into the host runtime event stream.
    /// 将 `prompts/list_changed` 通知转发到宿主运行时事件流。
    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::PromptsChanged);
    }

    /// Forward `resources/list_changed` notifications into the host runtime event stream.
    /// 将 `resources/list_changed` 通知转发到宿主运行时事件流。
    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::ResourcesChanged);
    }
}

/// Spawn one shared rmcp disconnect watcher that emits a typed disconnect reason once the peer closes.
/// 启动一个共享 rmcp 断连监视器，并在 peer 关闭后发出带类型的断连原因。
pub(crate) fn spawn_rmcp_disconnect_watch(
    endpoint: String,
    transport_label: &'static str,
    peer: Peer<RoleClient>,
    event_sender: broadcast::Sender<McpTransportEvent>,
) {
    tokio::spawn(async move {
        loop {
            if peer.is_transport_closed() {
                let _ = event_sender.send(McpTransportEvent::Disconnected {
                    reason: format!("MCP {transport_label} 传输 `{endpoint}` 已关闭。"),
                });
                break;
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}

/// Build one rmcp tool-call request parameter object from the host-owned JSON argument contract.
/// 根据宿主自有 JSON 参数契约构建一个 rmcp 工具调用参数对象。
pub(crate) fn build_rmcp_call_tool_request_params(
    name: &str,
    args: Value,
) -> Result<CallToolRequestParams, McpRuntimeError> {
    let arguments = match args {
        Value::Object(map) => Some(map),
        Value::Null => None,
        other => {
            return Err(McpRuntimeError::invalid_argument(format!(
                "tools/call requires an object or null arguments, got {other}"
            )));
        }
    };

    let mut params = CallToolRequestParams::new(name.to_string());
    params.arguments = arguments;
    Ok(params)
}

/// Convert one rmcp `CallToolResult` into the host-owned JSON value contract.
/// 将单个 rmcp `CallToolResult` 转换为宿主自有 JSON 值契约。
pub(crate) fn rmcp_call_tool_result_to_value(
    result: CallToolResult,
) -> Result<Value, McpRuntimeError> {
    if result.is_error.unwrap_or(false) {
        let message = result
            .content
            .iter()
            .find_map(|content| content.as_text().map(|text| text.text.clone()))
            .unwrap_or_else(|| "tool returned error".to_string());
        return Err(McpRuntimeError::transport(message));
    }

    if let Some(value) = result.structured_content {
        return Ok(value);
    }

    if let Some(text) = result
        .content
        .iter()
        .find_map(|content| content.as_text().map(|text| text.text.clone()))
    {
        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            return Ok(parsed);
        }
        return Ok(Value::String(text));
    }

    Ok(Value::Null)
}

/// Convert one rmcp tool schema into the host-owned MCP tool schema.
/// 将单个 rmcp 工具 schema 转换为宿主自有 MCP 工具 schema。
pub(crate) fn rmcp_tool_to_schema(tool: RmcpTool) -> McpToolSchema {
    McpToolSchema {
        name: tool.name.to_string(),
        description: tool
            .description
            .map(|value| value.to_string())
            .unwrap_or_default(),
        args_schema: Value::Object((*tool.input_schema).clone()),
        result_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone()))
            .unwrap_or(Value::Null),
    }
}

/// Returns whether one negotiated rmcp server capability snapshot advertises prompt support.
/// 返回单份已协商 rmcp 服务器能力快照是否声明了 prompt 支持。
pub(crate) fn rmcp_capabilities_support_prompts(capabilities: &ServerCapabilities) -> bool {
    capabilities.prompts.is_some()
}

/// Returns whether one negotiated rmcp server capability snapshot advertises resource support.
/// 返回单份已协商 rmcp 服务器能力快照是否声明了资源支持。
pub(crate) fn rmcp_capabilities_support_resources(capabilities: &ServerCapabilities) -> bool {
    capabilities.resources.is_some()
}

/// Returns whether one connected rmcp peer advertises prompt support after initialize.
/// 返回单个已连接 rmcp peer 在 initialize 后是否声明了 prompt 支持。
fn rmcp_peer_supports_prompts(peer: &Peer<RoleClient>) -> bool {
    peer.peer_info()
        .map(|info| rmcp_capabilities_support_prompts(&info.capabilities))
        .unwrap_or(false)
}

/// Returns whether one connected rmcp peer advertises resource support after initialize.
/// 返回单个已连接 rmcp peer 在 initialize 后是否声明了资源支持。
fn rmcp_peer_supports_resources(peer: &Peer<RoleClient>) -> bool {
    peer.peer_info()
        .map(|info| rmcp_capabilities_support_resources(&info.capabilities))
        .unwrap_or(false)
}

/// List tools from one rmcp client peer and convert them into the host-owned MCP schema contract.
/// 从单个 rmcp 客户端 peer 拉取工具列表，并将其转换为宿主自有 MCP schema 契约。
pub(crate) async fn list_rmcp_tools(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
    let tools = peer.list_all_tools().await.map_err(|error| {
        McpRuntimeError::transport(format!("tools/list({error_context}): {error}"))
    })?;
    Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
}

/// Convert one host-owned JSON prompt-argument payload into the rmcp prompt request contract.
/// 将一份宿主自有 JSON prompt 参数载荷转换为 rmcp prompt 请求契约。
fn build_rmcp_prompt_arguments(args: Value) -> Result<Option<JsonObject>, McpRuntimeError> {
    match args {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map)),
        other => Err(McpRuntimeError::invalid_argument(format!(
            "prompts/get requires an object or null arguments, got {other}"
        ))),
    }
}

/// List prompts from one rmcp client peer.
/// 从单个 rmcp 客户端 peer 拉取 prompt 列表。
pub(crate) async fn list_rmcp_prompts(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<Prompt>, McpRuntimeError> {
    if !rmcp_peer_supports_prompts(peer) {
        return Ok(Vec::new());
    }
    peer.list_all_prompts().await.map_err(|error| {
        McpRuntimeError::transport(format!("prompts/list({error_context}): {error}"))
    })
}

/// Resolve one rmcp prompt and return the typed prompt result.
/// 解析单个 rmcp prompt 并返回类型化 prompt 结果。
pub(crate) async fn get_rmcp_prompt(
    peer: &Peer<RoleClient>,
    error_context: &str,
    name: &str,
    args: Value,
) -> Result<GetPromptResult, McpRuntimeError> {
    if !rmcp_peer_supports_prompts(peer) {
        return Err(McpRuntimeError::transport(format!(
            "prompts/get({error_context}): server does not advertise prompts capability"
        )));
    }
    let mut params = GetPromptRequestParams::new(name.to_string());
    params.arguments = build_rmcp_prompt_arguments(args)?;
    peer.get_prompt(params).await.map_err(|error| {
        McpRuntimeError::transport(format!("prompts/get({error_context}): {error}"))
    })
}

/// List resources from one rmcp client peer.
/// 从单个 rmcp 客户端 peer 拉取资源列表。
pub(crate) async fn list_rmcp_resources(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<Resource>, McpRuntimeError> {
    if !rmcp_peer_supports_resources(peer) {
        return Ok(Vec::new());
    }
    peer.list_all_resources().await.map_err(|error| {
        McpRuntimeError::transport(format!("resources/list({error_context}): {error}"))
    })
}

/// Read one rmcp resource by URI and return the typed resource result.
/// 通过 URI 读取单个 rmcp 资源并返回类型化资源结果。
pub(crate) async fn read_rmcp_resource(
    peer: &Peer<RoleClient>,
    error_context: &str,
    uri: &str,
) -> Result<ReadResourceResult, McpRuntimeError> {
    if !rmcp_peer_supports_resources(peer) {
        return Err(McpRuntimeError::transport(format!(
            "resources/read({error_context}): server does not advertise resources capability"
        )));
    }
    peer.read_resource(ReadResourceRequestParams::new(uri.to_string()))
        .await
        .map_err(|error| {
            McpRuntimeError::transport(format!("resources/read({error_context}): {error}"))
        })
}

/// Call one rmcp tool through the provided peer and convert the result into the host-owned JSON value contract.
/// 通过给定 peer 调用单个 rmcp 工具，并将结果转换为宿主自有 JSON 值契约。
pub(crate) async fn call_rmcp_tool(
    peer: &Peer<RoleClient>,
    error_context: &str,
    name: &str,
    args: Value,
) -> Result<Value, McpRuntimeError> {
    let params = build_rmcp_call_tool_request_params(name, args)?;
    let result = peer.call_tool(params).await.map_err(|error| {
        McpRuntimeError::transport(format!("tools/call({error_context}): {error}"))
    })?;
    rmcp_call_tool_result_to_value(result)
}

#[cfg(test)]
mod tests {
    use super::{
        build_rmcp_call_tool_request_params, rmcp_call_tool_result_to_value,
        rmcp_capabilities_support_prompts, rmcp_capabilities_support_resources,
    };
    use rmcp::model::{CallToolResult, ServerCapabilities};
    use serde_json::{Value, json};

    #[test]
    fn rmcp_call_tool_request_builder_rejects_non_object_arguments() {
        let error = build_rmcp_call_tool_request_params("demo", json!(["bad"]))
            .expect_err("non-object arguments must be rejected");

        assert_eq!(
            error.to_string(),
            String::from("tools/call requires an object or null arguments, got [\"bad\"]")
        );
    }

    #[test]
    fn rmcp_call_tool_result_prefers_structured_content() {
        let mut result = CallToolResult::default();
        result.structured_content = Some(json!({"ok": true}));

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("structured content should decode"),
            json!({"ok": true})
        );
    }

    #[test]
    fn rmcp_call_tool_result_falls_back_to_plain_text() {
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::Content::text("plain-text")];

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("plain text should decode"),
            Value::String(String::from("plain-text"))
        );
    }

    #[test]
    fn rmcp_prompt_capability_detection_requires_explicit_support() {
        assert!(!rmcp_capabilities_support_prompts(
            &ServerCapabilities::default()
        ));
        assert!(rmcp_capabilities_support_prompts(
            &ServerCapabilities::builder().enable_prompts().build()
        ));
    }

    #[test]
    fn rmcp_resource_capability_detection_requires_explicit_support() {
        assert!(!rmcp_capabilities_support_resources(
            &ServerCapabilities::default()
        ));
        assert!(rmcp_capabilities_support_resources(
            &ServerCapabilities::builder().enable_resources().build()
        ));
    }
}

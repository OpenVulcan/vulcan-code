//! Streamable HTTP MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 Streamable HTTP MCP 客户端传输。

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use http::{HeaderName, HeaderValue};
use rmcp::ClientHandler;
use rmcp::model::{CallToolRequestParams, Tool as RmcpTool};
use rmcp::service::{NotificationContext, Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::{
    StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::error::McpRuntimeError;
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

/// Production streamable-HTTP MCP client.
/// 生产级 streamable-HTTP MCP 客户端。
pub struct HttpTransport {
    /// Stable endpoint label surfaced to the host runtime.
    /// 暴露给宿主运行时的稳定端点标签。
    endpoint: String,
    /// Cloneable rmcp client peer used for schema discovery and calls.
    /// 用于 schema 发现与调用的可克隆 rmcp 客户端 peer。
    peer: Peer<RoleClient>,
    /// Keeps the rmcp service alive for the lifetime of the transport.
    /// 在传输生命周期内保持 rmcp 服务存活。
    _service: Arc<RunningService<RoleClient, Arc<EventingClientHandler>>>,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl HttpTransport {
    /// Connect to one streamable-HTTP MCP endpoint.
    /// 连接到单个 streamable-HTTP MCP 端点。
    pub async fn connect(
        endpoint: impl Into<String>,
        uri: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
    ) -> Result<Self, McpRuntimeError> {
        let endpoint = endpoint.into();
        let uri = uri.into();
        let mut config = StreamableHttpClientTransportConfig::with_uri(uri.clone());
        let parsed_headers = parse_http_headers(custom_headers)?;

        if let Some(token) = bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            config = config.auth_header(token.to_string());
        }

        if !parsed_headers.is_empty() {
            config = config.custom_headers(parsed_headers);
        }

        let transport = StreamableHttpClientTransport::from_config(config);
        let (event_sender, _) = broadcast::channel(32);
        let handler = Arc::new(EventingClientHandler::new(event_sender.clone()));
        let service = handler
            .serve(transport)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.connect(http): {error}")))?;
        let peer = service.peer().clone();
        spawn_transport_disconnect_watch(endpoint.clone(), peer.clone(), event_sender.clone());

        Ok(Self {
            endpoint,
            peer,
            _service: Arc::new(service),
            event_sender,
        })
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.event_sender.subscribe()
    }

    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        let tools =
            self.peer.list_all_tools().await.map_err(|error| {
                McpRuntimeError::transport(format!("tools/list(http): {error}"))
            })?;
        Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError> {
        let arguments = match args {
            Value::Object(map) => Some(map),
            Value::Null => None,
            other => {
                return Err(McpRuntimeError::invalid_argument(format!(
                    "tools/call requires an object or null arguments, got {other}"
                )));
            }
        };
        let params = {
            #[allow(clippy::field_reassign_with_default)]
            let mut request = CallToolRequestParams::default();
            request.name = name.to_string().into();
            request.arguments = arguments;
            request
        };
        let result =
            self.peer.call_tool(params).await.map_err(|error| {
                McpRuntimeError::transport(format!("tools/call(http): {error}"))
            })?;

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
}

/// Client-side rmcp notification adapter that forwards tool-list changes to the host runtime.
/// 将工具列表变化转发给宿主运行时的客户端侧 rmcp 通知适配器。
#[derive(Clone)]
struct EventingClientHandler {
    /// Broadcast channel used to emit MCP transport lifecycle events.
    /// 用于发出 MCP 传输生命周期事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl EventingClientHandler {
    /// Create one eventing client handler from a shared broadcast sender.
    /// 根据共享广播发送端创建一个具备事件能力的客户端处理器。
    fn new(event_sender: broadcast::Sender<McpTransportEvent>) -> Self {
        Self { event_sender }
    }
}

impl ClientHandler for EventingClientHandler {
    /// Forward `tools/list_changed` notifications into the host runtime event stream.
    /// 将 `tools/list_changed` 通知转发到宿主运行时事件流。
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::ToolsChanged);
    }
}

/// Poll the rmcp peer until the underlying transport closes and then emit a disconnect event.
/// 轮询 rmcp peer 直到其底层传输关闭，然后发出断连事件。
fn spawn_transport_disconnect_watch(
    endpoint: String,
    peer: Peer<RoleClient>,
    event_sender: broadcast::Sender<McpTransportEvent>,
) {
    tokio::spawn(async move {
        loop {
            if peer.is_transport_closed() {
                let _ = event_sender.send(McpTransportEvent::Disconnected {
                    reason: format!("MCP HTTP 传输 `{endpoint}` 已关闭。"),
                });
                break;
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}

/// Converts custom string headers into typed HTTP headers.
/// 将自定义字符串请求头转换为类型化 HTTP 请求头。
fn parse_http_headers(
    custom_headers: HashMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, McpRuntimeError> {
    let mut parsed = HashMap::with_capacity(custom_headers.len());

    for (raw_name, raw_value) in custom_headers {
        let header_name = HeaderName::from_bytes(raw_name.trim().as_bytes()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!(
                "invalid MCP HTTP header name `{}`: {}",
                raw_name, error
            ))
        })?;
        let header_value = HeaderValue::from_str(raw_value.trim()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!(
                "invalid MCP HTTP header value for `{}`: {}",
                raw_name, error
            ))
        })?;
        parsed.insert(header_name, header_value);
    }

    Ok(parsed)
}

/// Converts one rmcp tool schema into the host-owned MCP tool schema.
/// 将单个 rmcp 工具 schema 转换为宿主自有 MCP 工具 schema。
fn rmcp_tool_to_schema(tool: RmcpTool) -> McpToolSchema {
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

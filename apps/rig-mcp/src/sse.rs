//! Legacy SSE MCP transport for servers that still expose the older
//! server-sent-events message bridge.
//!
//! This transport is intentionally separate from streamable HTTP. It opens one
//! long-lived SSE stream, consumes the legacy `endpoint` control event, and
//! sends JSON-RPC requests to the advertised message endpoint.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{
    Client, Response, Url,
    header::{ACCEPT, HeaderMap, HeaderName, HeaderValue},
};
use sse_stream::{Sse, SseStream};
use tokio::{
    sync::{Mutex, broadcast, oneshot, watch},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, ClientCapabilities,
    ClientJsonRpcMessage, ClientNotification, ClientRequest, Implementation, InitializeRequest,
    InitializeRequestParams, InitializedNotification, JsonRpcMessage, ListToolsRequest,
    ListToolsResult, NumberOrString, RequestId, ServerJsonRpcMessage, ServerNotification,
    ServerResult,
};

use crate::transport::{McpTransport, McpTransportEvent};
use rig_compose::registry::KernelError;
use rig_compose::tool::ToolSchema;

/// Background response channel sender keyed by JSON-RPC request id.
/// 按 JSON-RPC 请求标识索引的后台响应通道发送端。
type PendingResponseMap =
    Arc<Mutex<HashMap<String, oneshot::Sender<Result<ServerJsonRpcMessage, KernelError>>>>>;

/// Legacy SSE MCP client transport.
/// 旧式 SSE MCP 客户端传输。
pub struct SseTransport {
    /// Stable endpoint label surfaced to the registry layer.
    /// 暴露给注册表层的稳定端点标签。
    endpoint: String,
    /// Shared HTTP client used by the stream and message posts.
    /// 被事件流和消息 POST 复用的共享 HTTP 客户端。
    http_client: Client,
    /// Optional bearer token attached to outgoing requests.
    /// 附加到外发请求中的可选 Bearer Token。
    bearer_token: Option<String>,
    /// Additional HTTP headers attached to outgoing requests.
    /// 附加到外发请求中的额外 HTTP 请求头。
    custom_headers: HeaderMap,
    /// Latest legacy message endpoint advertised by the SSE stream.
    /// SSE 事件流公布的最新旧式消息端点。
    message_endpoint_rx: watch::Receiver<Option<Url>>,
    /// Pending response senders waiting for one matching JSON-RPC response.
    /// 正在等待匹配 JSON-RPC 响应的挂起发送端集合。
    pending_responses: PendingResponseMap,
    /// Monotonic JSON-RPC request id source.
    /// 单调递增的 JSON-RPC 请求标识源。
    next_request_id: AtomicU64,
    /// Cooperative shutdown token for the background SSE loop.
    /// 后台 SSE 循环使用的协作式关闭令牌。
    shutdown_token: CancellationToken,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl SseTransport {
    /// Connects to one legacy SSE MCP endpoint and performs the initialize handshake.
    /// 连接到一个旧式 SSE MCP 端点并完成 initialize 握手。
    pub async fn connect(
        endpoint: impl Into<String>,
        sse_url: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
    ) -> Result<Self, KernelError> {
        let sse_url = Url::parse(&sse_url.into()).map_err(|error| {
            KernelError::InvalidArgument(format!("invalid legacy SSE MCP url: {error}"))
        })?;
        let http_client = Client::builder()
            .build()
            .map_err(|error| KernelError::ToolFailed(format!("build SSE client: {error}")))?;
        let custom_headers = parse_http_headers(custom_headers)?;
        let pending_responses = Arc::new(Mutex::new(HashMap::new()));
        let (message_endpoint_tx, message_endpoint_rx) = watch::channel(None);
        let shutdown_token = CancellationToken::new();
        let (event_sender, _) = broadcast::channel(32);

        spawn_sse_listener(
            http_client.clone(),
            sse_url.clone(),
            bearer_token.clone(),
            custom_headers.clone(),
            message_endpoint_tx,
            pending_responses.clone(),
            shutdown_token.clone(),
            event_sender.clone(),
        );

        let transport = Self {
            endpoint: endpoint.into(),
            http_client,
            bearer_token,
            custom_headers,
            message_endpoint_rx,
            pending_responses,
            next_request_id: AtomicU64::new(1),
            shutdown_token,
            event_sender,
        };

        transport.initialize().await?;
        Ok(transport)
    }

    /// Performs the MCP initialize handshake on the legacy message endpoint.
    /// 在旧式消息端点上执行 MCP initialize 握手。
    async fn initialize(&self) -> Result<(), KernelError> {
        let request_id = self.next_request_id();
        let initialize_request = InitializeRequest::new(InitializeRequestParams::new(
            ClientCapabilities::default(),
            Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        ));
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(
                    ClientRequest::InitializeRequest(initialize_request),
                    request_id,
                ),
            )
            .await?;

        let ServerResult::InitializeResult(_) = extract_server_result(response)? else {
            return Err(KernelError::ToolFailed(
                "legacy SSE initialize did not return initialize result".to_string(),
            ));
        };

        let initialized_notification = ClientJsonRpcMessage::notification(
            ClientNotification::InitializedNotification(InitializedNotification {
                method: Default::default(),
                extensions: Default::default(),
            }),
        );
        self.send_notification(initialized_notification).await
    }

    /// Sends one JSON-RPC notification without waiting for a response.
    /// 发送一条无需等待响应的 JSON-RPC 通知。
    async fn send_notification(&self, message: ClientJsonRpcMessage) -> Result<(), KernelError> {
        let message_endpoint = self.resolve_message_endpoint().await?;
        let response = post_client_message(
            &self.http_client,
            &message_endpoint,
            &self.bearer_token,
            &self.custom_headers,
            &message,
        )
        .await?;
        consume_post_response(response).await.map(|_| ())
    }

    /// Sends one JSON-RPC request and waits for the matching response.
    /// 发送一条 JSON-RPC 请求并等待对应响应。
    async fn send_request_and_wait(
        &self,
        request_id: RequestId,
        message: ClientJsonRpcMessage,
    ) -> Result<ServerJsonRpcMessage, KernelError> {
        let request_key = request_id_key(&request_id);
        let (sender, receiver) = oneshot::channel();
        self.pending_responses
            .lock()
            .await
            .insert(request_key.clone(), sender);

        let message_endpoint = self.resolve_message_endpoint().await?;
        let post_result = post_client_message(
            &self.http_client,
            &message_endpoint,
            &self.bearer_token,
            &self.custom_headers,
            &message,
        )
        .await;

        let response = match post_result {
            Ok(response) => match consume_post_response(response).await {
                Ok(Some(immediate)) => {
                    self.pending_responses.lock().await.remove(&request_key);
                    immediate
                }
                Ok(None) => timeout(Duration::from_secs(30), receiver)
                    .await
                    .map_err(|_| {
                        KernelError::ToolFailed(format!(
                            "legacy SSE request `{}` timed out while waiting for response",
                            request_key
                        ))
                    })?
                    .map_err(|_| {
                        KernelError::ToolFailed(format!(
                            "legacy SSE request `{}` lost its response channel",
                            request_key
                        ))
                    })??,
                Err(error) => {
                    self.pending_responses.lock().await.remove(&request_key);
                    return Err(error);
                }
            },
            Err(error) => {
                self.pending_responses.lock().await.remove(&request_key);
                return Err(error);
            }
        };

        Ok(response)
    }

    /// Resolves the current legacy message endpoint advertised by the SSE stream.
    /// 解析当前由 SSE 事件流公布的旧式消息端点。
    async fn resolve_message_endpoint(&self) -> Result<Url, KernelError> {
        if let Some(url) = self.message_endpoint_rx.borrow().clone() {
            return Ok(url);
        }

        let mut receiver = self.message_endpoint_rx.clone();
        timeout(Duration::from_secs(10), receiver.changed())
            .await
            .map_err(|_| {
                KernelError::ToolFailed(
                    "legacy SSE endpoint did not advertise a message endpoint within 10 seconds"
                        .to_string(),
                )
            })?
            .map_err(|error| {
                KernelError::ToolFailed(format!(
                    "legacy SSE message endpoint watch channel closed: {error}"
                ))
            })?;

        receiver.borrow().clone().ok_or_else(|| {
            KernelError::ToolFailed(
                "legacy SSE stream did not provide a valid message endpoint".to_string(),
            )
        })
    }

    /// Allocates the next monotonically increasing JSON-RPC request id.
    /// 分配下一个单调递增的 JSON-RPC 请求标识。
    fn next_request_id(&self) -> RequestId {
        NumberOrString::Number(self.next_request_id.fetch_add(1, Ordering::Relaxed) as i64)
    }
}

impl Drop for SseTransport {
    /// Cancels the background SSE listener when the transport is dropped.
    /// 在传输对象析构时取消后台 SSE 监听器。
    fn drop(&mut self) {
        self.shutdown_token.cancel();
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.event_sender.subscribe()
    }

    async fn list_tools(&self) -> Result<Vec<ToolSchema>, KernelError> {
        let request_id = self.next_request_id();
        let request = ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        };
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(ClientRequest::ListToolsRequest(request), request_id),
            )
            .await?;
        let ServerResult::ListToolsResult(ListToolsResult { tools, .. }) =
            extract_server_result(response)?
        else {
            return Err(KernelError::ToolFailed(
                "legacy SSE tools/list did not return a listTools result".to_string(),
            ));
        };
        Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
    }

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, KernelError> {
        let arguments = match args {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => {
                return Err(KernelError::InvalidArgument(format!(
                    "tools/call requires an object or null arguments, got {other}"
                )));
            }
        };
        let request_id = self.next_request_id();
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = arguments;
        let request = CallToolRequest::new(params);
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(ClientRequest::CallToolRequest(request), request_id),
            )
            .await?;
        let ServerResult::CallToolResult(result) = extract_server_result(response)? else {
            return Err(KernelError::ToolFailed(
                "legacy SSE tools/call did not return a callTool result".to_string(),
            ));
        };
        call_tool_result_to_value(result)
    }
}

/// Spawns the background SSE listener loop.
/// 启动后台 SSE 监听循环。
fn spawn_sse_listener(
    http_client: Client,
    sse_url: Url,
    bearer_token: Option<String>,
    custom_headers: HeaderMap,
    message_endpoint_tx: watch::Sender<Option<Url>>,
    pending_responses: PendingResponseMap,
    shutdown_token: CancellationToken,
    event_sender: broadcast::Sender<McpTransportEvent>,
) {
    tokio::spawn(async move {
        let mut observed_connection = false;
        loop {
            if shutdown_token.is_cancelled() {
                break;
            }

            let response =
                match open_sse_stream(&http_client, &sse_url, &bearer_token, &custom_headers).await
                {
                    Ok(response) => response,
                    Err(_) => {
                        if observed_connection {
                            let _ = event_sender.send(McpTransportEvent::Disconnected {
                                reason: format!(
                                    "MCP SSE 传输 `{}` 已断开，正在等待重连。",
                                    sse_url
                                ),
                            });
                            observed_connection = false;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };

            observed_connection = true;
            let mut stream = SseStream::from_byte_stream(response.bytes_stream());
            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                    event = stream.next() => {
                        let Some(event) = event else {
                            break;
                        };
                        let Ok(event) = event else {
                            break;
                        };
                        handle_sse_event(
                            &sse_url,
                            event,
                            &message_endpoint_tx,
                            &pending_responses,
                            &event_sender,
                        )
                        .await;
                    }
                }
            }

            if observed_connection {
                let _ = event_sender.send(McpTransportEvent::Disconnected {
                    reason: format!("MCP SSE 传输 `{}` 已断开。", sse_url),
                });
                observed_connection = false;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

/// Opens one legacy SSE stream with the required request headers.
/// 使用所需请求头打开一条旧式 SSE 事件流。
async fn open_sse_stream(
    http_client: &Client,
    sse_url: &Url,
    bearer_token: &Option<String>,
    custom_headers: &HeaderMap,
) -> Result<Response, KernelError> {
    let mut request = http_client
        .get(sse_url.clone())
        .header(ACCEPT, "text/event-stream");

    if let Some(token) = bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(token);
    }

    for (name, value) in custom_headers {
        request = request.header(name, value);
    }

    request
        .send()
        .await
        .map_err(|error| KernelError::ToolFailed(format!("open legacy SSE stream: {error}")))?
        .error_for_status()
        .map_err(|error| {
            KernelError::ToolFailed(format!("legacy SSE stream returned error: {error}"))
        })
}

/// Handles one SSE event, including legacy `endpoint` control frames.
/// 处理单个 SSE 事件，包括旧式 `endpoint` 控制帧。
async fn handle_sse_event(
    sse_url: &Url,
    event: Sse,
    message_endpoint_tx: &watch::Sender<Option<Url>>,
    pending_responses: &PendingResponseMap,
    event_sender: &broadcast::Sender<McpTransportEvent>,
) {
    if event.event.as_deref() == Some("endpoint") {
        if let Some(url) = parse_legacy_message_endpoint(sse_url, event.data.as_deref()) {
            let _ = message_endpoint_tx.send(Some(url));
        }
        return;
    }

    let Some(payload) = event
        .data
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    let Ok(message) = serde_json::from_str::<ServerJsonRpcMessage>(payload) else {
        return;
    };

    if let JsonRpcMessage::Notification(notification) = &message {
        if matches!(
            notification.notification,
            ServerNotification::ToolListChangedNotification(_)
        ) {
            let _ = event_sender.send(McpTransportEvent::ToolsChanged);
        }
    }

    deliver_server_message(message, pending_responses).await;
}

/// Parses the legacy message endpoint from one control-event payload.
/// 从一条控制事件载荷中解析旧式消息端点。
fn parse_legacy_message_endpoint(base_url: &Url, payload: Option<&str>) -> Option<Url> {
    let raw = payload?.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };

    Url::parse(normalized)
        .ok()
        .or_else(|| base_url.join(normalized).ok())
}

/// Sends one client message through the legacy message endpoint.
/// 通过旧式消息端点发送一条客户端消息。
async fn post_client_message(
    http_client: &Client,
    message_endpoint: &Url,
    bearer_token: &Option<String>,
    custom_headers: &HeaderMap,
    message: &ClientJsonRpcMessage,
) -> Result<Response, KernelError> {
    let mut request = http_client.post(message_endpoint.clone());

    if let Some(token) = bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(token);
    }

    for (name, value) in custom_headers {
        request = request.header(name, value);
    }

    request
        .json(message)
        .send()
        .await
        .map_err(|error| KernelError::ToolFailed(format!("post legacy SSE MCP message: {error}")))
}

/// Consumes an HTTP POST response and extracts an immediate JSON-RPC result when present.
/// 消费 HTTP POST 响应，并在存在即时 JSON-RPC 结果时将其提取出来。
async fn consume_post_response(
    response: Response,
) -> Result<Option<ServerJsonRpcMessage>, KernelError> {
    let response = response
        .error_for_status()
        .map_err(|error| KernelError::ToolFailed(format!("legacy SSE MCP post failed: {error}")))?;

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if content_type.starts_with("application/json") {
        let message = response
            .json::<ServerJsonRpcMessage>()
            .await
            .map_err(|error| {
                KernelError::ToolFailed(format!(
                    "decode legacy SSE MCP immediate JSON response: {error}"
                ))
            })?;
        return Ok(Some(message));
    }

    Ok(None)
}

/// Delivers one parsed server JSON-RPC message to any waiting request future.
/// 将一条已解析的服务端 JSON-RPC 消息投递给等待中的请求 future。
async fn deliver_server_message(
    message: ServerJsonRpcMessage,
    pending_responses: &PendingResponseMap,
) {
    let request_key = match &message {
        JsonRpcMessage::Response(response) => request_id_key(&response.id),
        JsonRpcMessage::Error(error) => request_id_key(&error.id),
        _ => return,
    };

    if let Some(sender) = pending_responses.lock().await.remove(&request_key) {
        let _ = sender.send(Ok(message));
    }
}

/// Extracts the typed server result or turns one JSON-RPC error into `KernelError`.
/// 提取类型化服务端结果，或将 JSON-RPC 错误转换成 `KernelError`。
fn extract_server_result(message: ServerJsonRpcMessage) -> Result<ServerResult, KernelError> {
    match message {
        JsonRpcMessage::Response(response) => Ok(response.result),
        JsonRpcMessage::Error(error) => {
            Err(KernelError::ToolFailed(error.error.message.to_string()))
        }
        other => Err(KernelError::ToolFailed(format!(
            "unexpected legacy SSE MCP response message: {other:?}"
        ))),
    }
}

/// Converts one `CallToolResult` into the local kernel JSON value contract.
/// 将单个 `CallToolResult` 转换为本地内核 JSON 值契约。
fn call_tool_result_to_value(result: CallToolResult) -> Result<serde_json::Value, KernelError> {
    if result.is_error.unwrap_or(false) {
        let message = result
            .content
            .iter()
            .find_map(|content| content.as_text().map(|text| text.text.clone()))
            .unwrap_or_else(|| "tool returned error".to_string());
        return Err(KernelError::ToolFailed(message));
    }

    if let Some(value) = result.structured_content {
        return Ok(value);
    }

    if let Some(text) = result
        .content
        .iter()
        .find_map(|content| content.as_text().map(|text| text.text.clone()))
    {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            return Ok(parsed);
        }
        return Ok(serde_json::Value::String(text));
    }

    Ok(serde_json::Value::Null)
}

/// Converts one rmcp tool schema into the local kernel schema.
/// 将单个 rmcp 工具 schema 转换为本地内核 schema。
fn rmcp_tool_to_schema(tool: rmcp::model::Tool) -> ToolSchema {
    ToolSchema {
        name: tool.name.to_string(),
        description: tool
            .description
            .map(|value| value.to_string())
            .unwrap_or_default(),
        args_schema: serde_json::Value::Object((*tool.input_schema).clone()),
        result_schema: tool
            .output_schema
            .map(|schema| serde_json::Value::Object((*schema).clone()))
            .unwrap_or(serde_json::Value::Null),
    }
}

/// Converts one request id into a stable map key.
/// 将单个请求标识转换为稳定映射键。
fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        NumberOrString::Number(value) => value.to_string(),
        NumberOrString::String(value) => value.to_string(),
    }
}

/// Converts custom string headers into typed HTTP headers.
/// 将自定义字符串请求头转换为类型化 HTTP 请求头。
fn parse_http_headers(custom_headers: HashMap<String, String>) -> Result<HeaderMap, KernelError> {
    let mut parsed = HeaderMap::with_capacity(custom_headers.len());

    for (raw_name, raw_value) in custom_headers {
        let header_name = HeaderName::from_bytes(raw_name.trim().as_bytes()).map_err(|error| {
            KernelError::InvalidArgument(format!(
                "invalid MCP HTTP header name `{}`: {}",
                raw_name, error
            ))
        })?;
        let header_value = HeaderValue::from_str(raw_value.trim()).map_err(|error| {
            KernelError::InvalidArgument(format!(
                "invalid MCP HTTP header value for `{}`: {}",
                raw_name, error
            ))
        })?;
        parsed.append(header_name, header_value);
    }

    Ok(parsed)
}

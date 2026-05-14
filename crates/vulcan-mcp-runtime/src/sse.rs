//! Legacy SSE MCP client transport for older MCP servers.
//! 面向旧式 MCP 服务器的 legacy SSE MCP 客户端传输。

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
    header::{ACCEPT, HeaderMap},
};
use rmcp::model::{
    CallToolRequest, ClientCapabilities, ClientJsonRpcMessage, ClientNotification, ClientRequest,
    GetPromptRequest, GetPromptRequestParams, GetPromptResult, Implementation, InitializeRequest,
    InitializeRequestParams, InitializedNotification, JsonRpcMessage, ListPromptsRequest,
    ListPromptsResult, ListResourcesRequest, ListResourcesResult, ListToolsRequest,
    ListToolsResult, NumberOrString, Prompt, ReadResourceRequest, ReadResourceRequestParams,
    ReadResourceResult, RequestId, Resource, ServerCapabilities, ServerJsonRpcMessage,
    ServerNotification, ServerResult,
};
use serde_json::Value;
use sse_stream::{Sse, SseStream};
use tokio::{
    sync::{Mutex, broadcast, oneshot, watch},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::auth::{McpAuthChallenge, McpAuthStatus, McpOAuthSession};
use crate::error::McpRuntimeError;
use crate::http_support::parse_http_headers_to_header_map;
use crate::rmcp_support::{
    build_rmcp_call_tool_request_params, rmcp_call_tool_result_to_value,
    rmcp_capabilities_support_prompts, rmcp_capabilities_support_resources, rmcp_tool_to_schema,
};
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

/// Background response channel sender keyed by JSON-RPC request id.
/// 按 JSON-RPC 请求标识索引的后台响应通道发送端。
type PendingResponseMap =
    Arc<Mutex<HashMap<String, oneshot::Sender<Result<ServerJsonRpcMessage, McpRuntimeError>>>>>;

/// Legacy SSE MCP client transport.
/// 旧式 SSE MCP 客户端传输。
pub struct SseTransport {
    /// Stable endpoint label surfaced to the host runtime.
    /// 暴露给宿主运行时的稳定端点标签。
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
    /// Optional shared host-owned OAuth session reused across reconnects.
    /// 在重连之间复用的可选共享宿主自有 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
    /// Negotiated server capabilities captured from the initialize handshake.
    /// 从 initialize 握手中捕获的已协商服务器能力。
    capabilities: ServerCapabilities,
}

impl SseTransport {
    /// Connect to one legacy SSE MCP endpoint and perform the initialize handshake.
    /// 连接到一个旧式 SSE MCP 端点并完成 initialize 握手。
    pub async fn connect(
        endpoint: impl Into<String>,
        sse_url: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
    ) -> Result<Self, McpRuntimeError> {
        let sse_url = Url::parse(&sse_url.into()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!("invalid legacy SSE MCP url: {error}"))
        })?;
        let http_client = Client::builder()
            .build()
            .map_err(|error| McpRuntimeError::transport(format!("build SSE client: {error}")))?;
        let custom_headers = parse_http_headers_to_header_map(custom_headers)?;
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
            auth_session.clone(),
        );

        let mut transport = Self {
            endpoint: endpoint.into(),
            http_client,
            bearer_token,
            custom_headers,
            message_endpoint_rx,
            pending_responses,
            next_request_id: AtomicU64::new(1),
            shutdown_token,
            event_sender,
            auth_session,
            capabilities: ServerCapabilities::default(),
        };

        transport.capabilities = transport.initialize().await?;
        Ok(transport)
    }

    /// Perform the MCP initialize handshake on the legacy message endpoint.
    /// 在旧式消息端点上执行 MCP initialize 握手。
    async fn initialize(&self) -> Result<ServerCapabilities, McpRuntimeError> {
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

        let ServerResult::InitializeResult(result) = extract_server_result(response)? else {
            return Err(McpRuntimeError::transport(
                "legacy SSE initialize did not return initialize result".to_string(),
            ));
        };

        let capabilities = result.capabilities;
        let initialized_notification = ClientJsonRpcMessage::notification(
            ClientNotification::InitializedNotification(InitializedNotification {
                method: Default::default(),
                extensions: Default::default(),
            }),
        );
        self.send_notification(initialized_notification).await?;
        Ok(capabilities)
    }

    /// Send one JSON-RPC notification without waiting for a response.
    /// 发送一条无需等待响应的 JSON-RPC 通知。
    async fn send_notification(
        &self,
        message: ClientJsonRpcMessage,
    ) -> Result<(), McpRuntimeError> {
        let message_endpoint = self.resolve_message_endpoint().await?;
        let response = post_client_message(
            &self.http_client,
            &message_endpoint,
            &self.bearer_token,
            &self.custom_headers,
            &message,
            self.auth_session.as_ref(),
        )
        .await?;
        consume_post_response(response).await.map(|_| ())
    }

    /// Send one JSON-RPC request and wait for the matching response.
    /// 发送一条 JSON-RPC 请求并等待对应响应。
    async fn send_request_and_wait(
        &self,
        request_id: RequestId,
        message: ClientJsonRpcMessage,
    ) -> Result<ServerJsonRpcMessage, McpRuntimeError> {
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
            self.auth_session.as_ref(),
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
                        McpRuntimeError::transport(format!(
                            "legacy SSE request `{}` timed out while waiting for response",
                            request_key
                        ))
                    })?
                    .map_err(|_| {
                        McpRuntimeError::transport(format!(
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

    /// Resolve the current legacy message endpoint advertised by the SSE stream.
    /// 解析当前由 SSE 事件流公布的旧式消息端点。
    async fn resolve_message_endpoint(&self) -> Result<Url, McpRuntimeError> {
        if let Some(url) = self.message_endpoint_rx.borrow().clone() {
            return Ok(url);
        }

        let mut receiver = self.message_endpoint_rx.clone();
        timeout(Duration::from_secs(10), receiver.changed())
            .await
            .map_err(|_| {
                McpRuntimeError::transport(
                    "legacy SSE endpoint did not advertise a message endpoint within 10 seconds"
                        .to_string(),
                )
            })?
            .map_err(|error| {
                McpRuntimeError::transport(format!(
                    "legacy SSE message endpoint watch channel closed: {error}"
                ))
            })?;

        receiver.borrow().clone().ok_or_else(|| {
            McpRuntimeError::transport(
                "legacy SSE stream did not provide a valid message endpoint".to_string(),
            )
        })
    }

    /// Allocate the next monotonically increasing JSON-RPC request id.
    /// 分配下一个单调递增的 JSON-RPC 请求标识。
    fn next_request_id(&self) -> RequestId {
        NumberOrString::Number(self.next_request_id.fetch_add(1, Ordering::Relaxed) as i64)
    }

    /// Returns whether the negotiated legacy SSE server capabilities include prompt support.
    /// 返回已协商 legacy SSE 服务器能力中是否包含 prompt 支持。
    fn supports_prompt_capability(&self) -> bool {
        rmcp_capabilities_support_prompts(&self.capabilities)
    }

    /// Returns whether the negotiated legacy SSE server capabilities include resource support.
    /// 返回已协商 legacy SSE 服务器能力中是否包含资源支持。
    fn supports_resource_capability(&self) -> bool {
        rmcp_capabilities_support_resources(&self.capabilities)
    }
}

impl Drop for SseTransport {
    /// Cancel the background SSE listener when the transport is dropped.
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

    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
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
            return Err(McpRuntimeError::transport(
                "legacy SSE tools/list did not return a listTools result".to_string(),
            ));
        };
        Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>, McpRuntimeError> {
        if !self.supports_prompt_capability() {
            return Ok(Vec::new());
        }
        let request_id = self.next_request_id();
        let request = ListPromptsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        };
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(
                    ClientRequest::ListPromptsRequest(request),
                    request_id,
                ),
            )
            .await?;
        let ServerResult::ListPromptsResult(ListPromptsResult { prompts, .. }) =
            extract_server_result(response)?
        else {
            return Err(McpRuntimeError::transport(
                "legacy SSE prompts/list did not return a listPrompts result".to_string(),
            ));
        };
        Ok(prompts)
    }

    async fn get_prompt(
        &self,
        name: &str,
        args: Value,
    ) -> Result<GetPromptResult, McpRuntimeError> {
        if !self.supports_prompt_capability() {
            return Err(McpRuntimeError::transport(format!(
                "prompts/get({}): server does not advertise prompts capability",
                self.endpoint()
            )));
        }
        let request_id = self.next_request_id();
        let request = GetPromptRequest::new(build_legacy_prompt_request_params(name, args)?);
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(ClientRequest::GetPromptRequest(request), request_id),
            )
            .await?;
        let ServerResult::GetPromptResult(result) = extract_server_result(response)? else {
            return Err(McpRuntimeError::transport(
                "legacy SSE prompts/get did not return a getPrompt result".to_string(),
            ));
        };
        Ok(result)
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpRuntimeError> {
        if !self.supports_resource_capability() {
            return Ok(Vec::new());
        }
        let request_id = self.next_request_id();
        let request = ListResourcesRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        };
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(
                    ClientRequest::ListResourcesRequest(request),
                    request_id,
                ),
            )
            .await?;
        let ServerResult::ListResourcesResult(ListResourcesResult { resources, .. }) =
            extract_server_result(response)?
        else {
            return Err(McpRuntimeError::transport(
                "legacy SSE resources/list did not return a listResources result".to_string(),
            ));
        };
        Ok(resources)
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpRuntimeError> {
        if !self.supports_resource_capability() {
            return Err(McpRuntimeError::transport(format!(
                "resources/read({}): server does not advertise resources capability",
                self.endpoint()
            )));
        }
        let request_id = self.next_request_id();
        let request = ReadResourceRequest::new(ReadResourceRequestParams::new(uri.to_string()));
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(
                    ClientRequest::ReadResourceRequest(request),
                    request_id,
                ),
            )
            .await?;
        let ServerResult::ReadResourceResult(result) = extract_server_result(response)? else {
            return Err(McpRuntimeError::transport(
                "legacy SSE resources/read did not return a readResource result".to_string(),
            ));
        };
        Ok(result)
    }

    async fn supports_oauth(&self) -> bool {
        self.auth_session.is_some()
    }

    async fn has_stored_tokens(&self) -> Result<bool, McpRuntimeError> {
        Ok(match self.auth_session.as_ref() {
            Some(auth_session) => auth_session.has_stored_tokens().await,
            None => false,
        })
    }

    async fn auth_status(&self) -> Result<McpAuthStatus, McpRuntimeError> {
        Ok(match self.auth_session.as_ref() {
            Some(auth_session) => auth_session.auth_status().await,
            None => McpAuthStatus::NotConfigured,
        })
    }

    async fn start_auth(&self, client_name: &str) -> Result<McpAuthChallenge, McpRuntimeError> {
        let Some(auth_session) = self.auth_session.as_ref() else {
            return Err(McpRuntimeError::transport(format!(
                "MCP SSE 传输 `{}` 未配置 OAuth。",
                self.endpoint
            )));
        };
        auth_session.start_authorization(client_name).await
    }

    async fn finish_auth(
        &self,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, McpRuntimeError> {
        let Some(auth_session) = self.auth_session.as_ref() else {
            return Err(McpRuntimeError::transport(format!(
                "MCP SSE 传输 `{}` 未配置 OAuth。",
                self.endpoint
            )));
        };
        auth_session
            .finish_authorization(authorization_code, oauth_state)
            .await
    }

    async fn remove_auth(&self) -> Result<(), McpRuntimeError> {
        if let Some(auth_session) = self.auth_session.as_ref() {
            auth_session.remove_authorization().await;
        }
        Ok(())
    }

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        let params = build_rmcp_call_tool_request_params(name, args)?;
        let request_id = self.next_request_id();
        let request = CallToolRequest::new(params);
        let response = self
            .send_request_and_wait(
                request_id.clone(),
                ClientJsonRpcMessage::request(ClientRequest::CallToolRequest(request), request_id),
            )
            .await?;
        let ServerResult::CallToolResult(result) = extract_server_result(response)? else {
            return Err(McpRuntimeError::transport(
                "legacy SSE tools/call did not return a callTool result".to_string(),
            ));
        };
        rmcp_call_tool_result_to_value(result)
    }
}

/// Spawn the background SSE listener loop.
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
    auth_session: Option<Arc<McpOAuthSession>>,
) {
    tokio::spawn(async move {
        let mut observed_connection = false;
        loop {
            if shutdown_token.is_cancelled() {
                break;
            }

            let response = match open_sse_stream(
                &http_client,
                &sse_url,
                &bearer_token,
                &custom_headers,
                auth_session.as_ref(),
            )
            .await
            {
                Ok(response) => response,
                Err(_) => {
                    if observed_connection {
                        let _ = event_sender.send(McpTransportEvent::Disconnected {
                            reason: format!("MCP SSE 传输 `{}` 已断开，正在等待重连。", sse_url),
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

/// Open one legacy SSE stream with the required request headers.
/// 使用所需请求头打开一条旧式 SSE 事件流。
async fn open_sse_stream(
    http_client: &Client,
    sse_url: &Url,
    bearer_token: &Option<String>,
    custom_headers: &HeaderMap,
    auth_session: Option<&Arc<McpOAuthSession>>,
) -> Result<Response, McpRuntimeError> {
    let mut request = http_client
        .get(sse_url.clone())
        .header(ACCEPT, "text/event-stream");

    if let Some(token) = resolve_effective_bearer_token(bearer_token, auth_session).await? {
        request = request.bearer_auth(token);
    }

    for (name, value) in custom_headers {
        request = request.header(name, value);
    }

    request
        .send()
        .await
        .map_err(|error| McpRuntimeError::transport(format!("open legacy SSE stream: {error}")))?
        .error_for_status()
        .map_err(|error| {
            McpRuntimeError::transport(format!("legacy SSE stream returned error: {error}"))
        })
}

/// Handle one SSE event, including legacy `endpoint` control frames.
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
        } else if matches!(
            notification.notification,
            ServerNotification::PromptListChangedNotification(_)
        ) {
            let _ = event_sender.send(McpTransportEvent::PromptsChanged);
        } else if matches!(
            notification.notification,
            ServerNotification::ResourceListChangedNotification(_)
        ) {
            let _ = event_sender.send(McpTransportEvent::ResourcesChanged);
        }
    }

    deliver_server_message(message, pending_responses).await;
}

/// Parse the legacy message endpoint from one control-event payload.
/// 从单条控制事件载荷中解析旧式消息端点。
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

/// Send one client message through the legacy message endpoint.
/// 通过旧式消息端点发送一条客户端消息。
async fn post_client_message(
    http_client: &Client,
    message_endpoint: &Url,
    bearer_token: &Option<String>,
    custom_headers: &HeaderMap,
    message: &ClientJsonRpcMessage,
    auth_session: Option<&Arc<McpOAuthSession>>,
) -> Result<Response, McpRuntimeError> {
    let mut request = http_client.post(message_endpoint.clone());

    if let Some(token) = resolve_effective_bearer_token(bearer_token, auth_session).await? {
        request = request.bearer_auth(token);
    }

    for (name, value) in custom_headers {
        request = request.header(name, value);
    }

    request.json(message).send().await.map_err(|error| {
        McpRuntimeError::transport(format!("post legacy SSE MCP message: {error}"))
    })
}

/// Consume an HTTP POST response and extract an immediate JSON-RPC result when present.
/// 消费 HTTP POST 响应，并在存在即时 JSON-RPC 结果时将其提取出来。
async fn consume_post_response(
    response: Response,
) -> Result<Option<ServerJsonRpcMessage>, McpRuntimeError> {
    let response = response.error_for_status().map_err(|error| {
        McpRuntimeError::transport(format!("legacy SSE MCP post failed: {error}"))
    })?;

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
                McpRuntimeError::transport(format!(
                    "decode legacy SSE MCP immediate JSON response: {error}"
                ))
            })?;
        return Ok(Some(message));
    }

    Ok(None)
}

/// Deliver one parsed server JSON-RPC message to any waiting request future.
/// 将单条已解析的服务端 JSON-RPC 消息投递给等待中的请求 future。
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

/// Extract the typed server result or turn one JSON-RPC error into runtime error.
/// 提取类型化服务端结果，或将单条 JSON-RPC 错误转换成运行时错误。
fn extract_server_result(message: ServerJsonRpcMessage) -> Result<ServerResult, McpRuntimeError> {
    match message {
        JsonRpcMessage::Response(response) => Ok(response.result),
        JsonRpcMessage::Error(error) => {
            Err(McpRuntimeError::transport(error.error.message.to_string()))
        }
        other => Err(McpRuntimeError::transport(format!(
            "unexpected legacy SSE MCP response message: {other:?}"
        ))),
    }
}

/// Convert one request id into a stable map key.
/// 将单个请求标识转换为稳定映射键。
fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        NumberOrString::Number(value) => value.to_string(),
        NumberOrString::String(value) => value.to_string(),
    }
}

/// Builds one legacy SSE prompt request parameter payload from a host-owned JSON argument contract.
/// 根据宿主自有 JSON 参数契约构建一份旧式 SSE prompt 请求参数载荷。
fn build_legacy_prompt_request_params(
    name: &str,
    args: Value,
) -> Result<GetPromptRequestParams, McpRuntimeError> {
    let mut params = GetPromptRequestParams::new(name.to_string());
    params.arguments = match args {
        Value::Null => None,
        Value::Object(map) => Some(map),
        other => {
            return Err(McpRuntimeError::invalid_argument(format!(
                "prompts/get requires an object or null arguments, got {other}"
            )));
        }
    };
    Ok(params)
}

/// Resolves the effective bearer token by preferring the live OAuth session token and falling back to the configured static token.
/// 通过优先使用实时 OAuth 会话令牌并回退到已配置的静态令牌，解析生效 bearer token。
async fn resolve_effective_bearer_token(
    bearer_token: &Option<String>,
    auth_session: Option<&Arc<McpOAuthSession>>,
) -> Result<Option<String>, McpRuntimeError> {
    if let Some(auth_session) = auth_session {
        if let Some(token) = auth_session.access_token().await? {
            return Ok(Some(token));
        }
    }

    Ok(bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

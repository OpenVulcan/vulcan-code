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

use crate::McpTransportResources;
use crate::auth::McpOAuthSession;
use crate::error::McpRuntimeError;
use crate::http_support::parse_http_headers_to_header_map;
use crate::resources::RetainedResource;
use crate::rmcp_support::{
    build_rmcp_call_tool_request_params, project_rmcp_tool_result,
    rmcp_capabilities_support_prompts, rmcp_capabilities_support_resources, rmcp_tool_to_schema,
};
use crate::shutdown::TransportShutdown;
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

/// Background response channel sender keyed by JSON-RPC request id.
/// 按 JSON-RPC 请求标识索引的后台响应通道发送端。
type PendingResponseMap =
    Arc<Mutex<HashMap<String, oneshot::Sender<Result<ServerJsonRpcMessage, McpRuntimeError>>>>>;

/// Sole reconnect delay for the legacy listener; shutdown interrupts this wait.
/// 旧式监听器重连延迟的唯一定义；关闭中断此等待。
const SSE_RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Legacy SSE MCP client transport.
/// 旧式 SSE MCP 客户端传输。
pub struct SseTransport {
    /// Stable endpoint label surfaced to the host runtime.
    /// 暴露给宿主运行时的稳定端点标签。
    endpoint: String,
    /// Shared HTTP client used by the stream and message posts.
    /// 被事件流和消息 POST 复用的共享 HTTP 客户端。
    http_client: RetainedResource<Client>,
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
    /// Shared close operation retaining the listener until its actual completion.
    /// 保留监听器直至实际结束的共享关闭操作。
    shutdown: TransportShutdown,
    /// Optional shared host-owned OAuth session reused across reconnects.
    /// 在重连之间复用的可选共享宿主自有 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
    /// Negotiated server capabilities captured from the initialize handshake.
    /// 从 initialize 握手中捕获的已协商服务器能力。
    capabilities: ServerCapabilities,
}

impl SseTransport {
    /// Connects while retaining exact host resources in the client, listener and response stream.
    /// 连接并在客户端、监听器及响应流中保留精确宿主资源。
    /// Endpoint labels the server; sse_url, token, headers and auth_session define the request policy.
    /// endpoint 标识服务器；sse_url、令牌、请求头及 auth_session 定义请求策略。
    /// Resources owns the frozen host authority; returns the initialized transport or an explicit error.
    /// resources 拥有冻结宿主权威；返回已初始化传输或明确错误。
    pub async fn connect_with_resources(
        endpoint: impl Into<String>,
        sse_url: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
        resources: McpTransportResources,
    ) -> Result<Self, McpRuntimeError> {
        let sse_url = Url::parse(&sse_url.into()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!("invalid legacy SSE MCP url: {error}"))
        })?;
        let http_client =
            resources.retain(Client::builder().build().map_err(|error| {
                McpRuntimeError::transport(format!("build SSE client: {error}"))
            })?);
        let custom_headers = parse_http_headers_to_header_map(custom_headers)?;
        let pending_responses = Arc::new(Mutex::new(HashMap::new()));
        let (message_endpoint_tx, message_endpoint_rx) = watch::channel(None);
        let shutdown_token = CancellationToken::new();
        let (event_sender, _) = broadcast::channel(32);

        let listener_task = spawn_sse_listener(SseListenerInput {
            http_client: http_client.clone(),
            sse_url: sse_url.clone(),
            bearer_token: bearer_token.clone(),
            custom_headers: custom_headers.clone(),
            message_endpoint_tx,
            pending_responses: pending_responses.clone(),
            shutdown_token: shutdown_token.clone(),
            event_sender: event_sender.clone(),
            auth_session: auth_session.clone(),
        });

        // The close worker retains the original listener even if its observer goes away.
        // 即使观察者退出，关闭工作任务仍保留原监听器。
        let close_token = shutdown_token.clone();
        let shutdown = TransportShutdown::new("mcp.shutdown(sse)", async move {
            close_token.cancel();
            listener_task.await.map_err(|error| {
                McpRuntimeError::transport(format!("mcp.shutdown(sse).join: {error}"))
            })
        });
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
            shutdown,
            auth_session,
            capabilities: ServerCapabilities::default(),
        };

        transport.capabilities = transport.initialize().await?;
        Ok(transport)
    }

    /// Connect to one legacy SSE MCP endpoint and perform the initialize handshake.
    /// 连接到一个旧式 SSE MCP 端点并完成 initialize 握手。
    pub async fn connect(
        endpoint: impl Into<String>,
        sse_url: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
    ) -> Result<Self, McpRuntimeError> {
        // Empty host ownership preserves the original public connection API.
        // 空宿主所有权保留原公开连接接口。
        let (resources, _) = McpTransportResources::new(());
        Self::connect_with_resources(
            endpoint,
            sse_url,
            bearer_token,
            custom_headers,
            auth_session,
            resources,
        )
        .await
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
            self.http_client.as_ref(),
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
            self.http_client.as_ref(),
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

    /// Cancels the legacy SSE listener and waits for its exact background task to exit.
    /// 取消 legacy SSE 监听器并等待其精确后台任务退出。
    /// Returns the shared listener result; timeout and cancellation do not discard its join.
    /// 返回共享监听器结果；超时和取消不丢弃其等待句柄。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.shutdown_token.cancel();
        self.shutdown.wait().await
    }

    /// Waits for the exact cancelled SSE listener to finish without discarding its join handle.
    /// 等待已取消的精确 SSE 监听器结束，不丢弃其等待句柄。
    /// No inputs; returns the listener's terminal result or explicit interruption.
    /// 无输入；返回监听器终态结果或明确中断。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.shutdown_token.cancel();
        self.shutdown.wait_until_complete().await
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

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        self.call_tool_with_audience(name, args, crate::McpToolResultAudience::Machine)
            .await
    }

    /// Sends one legacy SSE call, retaining consumer-specific projection after the response.
    /// 发送一次旧式 SSE 调用，在响应后保留消费者对应投影。
    async fn call_tool_with_audience(
        &self,
        name: &str,
        args: serde_json::Value,
        audience: crate::McpToolResultAudience,
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
        project_rmcp_tool_result(result, audience)
    }
}

/// Spawn the background SSE listener loop.
/// 启动后台 SSE 监听循环。
struct SseListenerInput {
    /// HTTP client reused for reconnecting the SSE stream.
    /// 用于重连 SSE 流的复用 HTTP 客户端。
    http_client: RetainedResource<Client>,
    /// Legacy SSE endpoint URL.
    /// 旧式 SSE 端点地址。
    sse_url: Url,
    /// Optional configured static bearer token.
    /// 可选的静态 bearer token 配置。
    bearer_token: Option<String>,
    /// Custom HTTP headers applied to stream requests.
    /// 应用于流请求的自定义 HTTP 头。
    custom_headers: HeaderMap,
    /// Channel used to publish the discovered message endpoint.
    /// 用于发布已发现消息端点的通道。
    message_endpoint_tx: watch::Sender<Option<Url>>,
    /// Shared map of JSON-RPC responses waiting for request ids.
    /// 按请求标识等待 JSON-RPC 响应的共享映射。
    pending_responses: PendingResponseMap,
    /// Shutdown token observed by the listener loop.
    /// 监听循环观察的关闭令牌。
    shutdown_token: CancellationToken,
    /// Transport event broadcaster.
    /// 传输事件广播器。
    event_sender: broadcast::Sender<McpTransportEvent>,
    /// Optional OAuth session that can refresh live bearer tokens.
    /// 可刷新实时 bearer token 的可选 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
}

/// Spawn the background SSE listener loop.
/// 启动后台 SSE 监听循环。
fn spawn_sse_listener(input: SseListenerInput) -> tokio::task::JoinHandle<()> {
    let SseListenerInput {
        http_client,
        sse_url,
        bearer_token,
        custom_headers,
        message_endpoint_tx,
        pending_responses,
        shutdown_token,
        event_sender,
        auth_session,
    } = input;
    tokio::spawn(async move {
        let mut observed_connection = false;
        loop {
            if shutdown_token.is_cancelled() {
                break;
            }

            // Opening a response can wait indefinitely for headers; cancellation must cover that wait.
            // 打开响应可能无限等待响应头；取消必须覆盖此等待。
            let open_result = tokio::select! {
                biased;
                _ = shutdown_token.cancelled() => return,
                result = open_sse_stream(
                http_client.as_ref(),
                &sse_url,
                &bearer_token,
                &custom_headers,
                auth_session.as_ref(),
                ) => result,
            };
            let response = match open_result {
                Ok(response) => response,
                Err(_) => {
                    if observed_connection {
                        let _ = event_sender.send(McpTransportEvent::Disconnected {
                            reason: format!(
                                "MCP SSE transport `{}` disconnected; waiting to reconnect.",
                                sse_url
                            ),
                        });
                        observed_connection = false;
                    }
                    tokio::select! {
                        biased;
                        _ = shutdown_token.cancelled() => return,
                        _ = tokio::time::sleep(SSE_RECONNECT_DELAY) => {}
                    }
                    continue;
                }
            };

            observed_connection = true;
            // The response has independent ownership even when its creating client is dropped.
            // 创建客户端释放后，响应仍有独立所有权。
            let mut stream =
                http_client.retain(SseStream::from_bytes_stream(response.bytes_stream()));
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
                    reason: format!("MCP SSE transport `{}` disconnected.", sse_url),
                });
                observed_connection = false;
            }
            tokio::select! {
                biased;
                _ = shutdown_token.cancelled() => return,
                _ = tokio::time::sleep(SSE_RECONNECT_DELAY) => {}
            }
        }
    })
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
        JsonRpcMessage::Error(error) => {
            // An error response without an id cannot be correlated with any pending request.
            // 不含请求标识的错误响应无法与任何待处理请求建立关联。
            let Some(request_id) = error.id.as_ref() else {
                return;
            };
            request_id_key(request_id)
        }
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
    if let Some(auth_session) = auth_session
        && let Some(token) = auth_session.access_token().await?
    {
        return Ok(Some(token));
    }

    Ok(bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

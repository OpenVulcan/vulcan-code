//! Streamable HTTP MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 Streamable HTTP MCP 客户端传输。

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use rmcp::service::{Peer, RoleClient, ServiceExt};
use rmcp::transport::{
    StreamableHttpClientTransport,
    streamable_http_client::{
        StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
        StreamableHttpError, StreamableHttpPostResponse,
    },
};
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod resource_tests;

use crate::McpTransportResources;
use crate::auth::McpOAuthSession;
use crate::error::McpRuntimeError;
use crate::http_support::parse_http_headers_to_hash_map;
use crate::resources::RetainedResource;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, get_rmcp_prompt, list_rmcp_prompts, list_rmcp_resources,
    list_rmcp_tools, read_rmcp_resource, spawn_rmcp_disconnect_watch,
};
use crate::shutdown::{TransportShutdown, observe_backend_close};
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

/// Auth-aware streamable HTTP client that delegates the wire protocol to rmcp's reqwest implementation while resolving live OAuth headers.
/// 具备鉴权能力的 streamable HTTP 客户端；它将线协议委托给 rmcp 的 reqwest 实现，同时解析实时 OAuth 请求头。
#[derive(Debug, Clone)]
struct AuthenticatedStreamableHttpClient {
    /// The exact official worker token cancels requests and streams, including initialization.
    /// 精确官方工作任务令牌取消请求及流，包括初始化。
    cancellation: CancellationToken,
    /// Shared reqwest client reused across stream and request operations.
    /// 在事件流与请求操作之间复用的共享 reqwest 客户端。
    http_client: RetainedResource<reqwest::Client>,
    /// Optional shared host-owned OAuth session used to resolve live bearer tokens.
    /// 用于解析实时 bearer token 的可选共享宿主自有 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
}

impl AuthenticatedStreamableHttpClient {
    /// Retains and cancels a real response stream using the original client authority and worker token.
    /// 使用原客户端权威及工作任务令牌保留并取消真实响应流。
    /// Stream owns the response; returns the same event sequence until the declared worker shuts down.
    /// stream 拥有响应；返回相同事件序列，直至声明工作任务关闭。
    fn retain_response_stream(
        &self,
        stream: futures::stream::BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>,
    ) -> futures::stream::BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>> {
        // Startup can be waiting inside an SSE body after headers have already arrived.
        // 响应头已到达后，启动仍可能等待 SSE 响应体。
        let stream = stream
            .take_until(self.cancellation.clone().cancelled_owned())
            .boxed();
        Box::pin(self.http_client.retain(stream))
    }

    /// Builds one auth-aware streamable HTTP client from an optional shared OAuth session.
    /// 根据可选共享 OAuth 会话构建单个具备鉴权能力的 streamable HTTP 客户端。
    /// Auth_session supplies live tokens; resources owns the frozen host authority across client clones.
    /// auth_session 提供实时令牌；resources 在客户端克隆间拥有冻结宿主权威。
    /// Returns a lease-bound client, or the original client construction failure.
    /// 返回绑定租约的客户端，或原客户端构建失败。
    fn new(
        auth_session: Option<Arc<McpOAuthSession>>,
        resources: McpTransportResources,
    ) -> Result<Self, McpRuntimeError> {
        let http_client = reqwest::Client::builder()
            .build()
            .map_err(|error| McpRuntimeError::transport(format!("build HTTP client: {error}")))?;

        Ok(Self {
            http_client: resources.retain(http_client),
            auth_session,
            cancellation: CancellationToken::new(),
        })
    }

    /// Resolves the effective bearer token by preferring the live OAuth session token and falling back to the configured static token.
    /// 通过优先使用实时 OAuth 会话令牌并回退到已配置的静态令牌，解析生效 bearer token。
    async fn resolve_auth_header(
        &self,
        configured_auth_header: Option<String>,
    ) -> Result<
        Option<String>,
        rmcp::transport::streamable_http_client::StreamableHttpError<reqwest::Error>,
    > {
        let Some(auth_session) = self.auth_session.as_ref() else {
            return Ok(configured_auth_header);
        };

        match auth_session.access_token().await {
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => Ok(configured_auth_header),
            Err(error) => Err(
                rmcp::transport::streamable_http_client::StreamableHttpError::UnexpectedServerResponse(
                    Cow::Owned(error.to_string()),
                ),
            ),
        }
    }
}

impl StreamableHttpClient for AuthenticatedStreamableHttpClient {
    /// Shared reqwest client error type delegated from rmcp's built-in reqwest transport implementation.
    /// 从 rmcp 内建 reqwest 传输实现委托出来的共享 reqwest 客户端错误类型。
    type Error = reqwest::Error;

    /// Opens one SSE stream using rmcp's reqwest implementation with the resolved effective auth header.
    /// 使用解析后的生效鉴权头，通过 rmcp 的 reqwest 实现打开一条 SSE 事件流。
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<
        BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>,
        StreamableHttpError<Self::Error>,
    > {
        // The upstream initialization await does not select worker cancellation on its own.
        // 上游初始化等待自身未选择工作任务取消。
        let stream = self
            .cancellation
            .run_until_cancelled(async {
                let auth_header = self.resolve_auth_header(auth_header).await?;
                // The response stream can outlive this client, so it receives its own original lease reference.
                // 响应流可以比此客户端存续更久，因此取得自身原租约引用。
                <reqwest::Client as StreamableHttpClient>::get_stream(
                    self.http_client.as_ref(),
                    uri,
                    session_id,
                    last_event_id,
                    auth_header,
                    custom_headers,
                )
                .await
            })
            .await
            .ok_or(StreamableHttpError::TransportChannelClosed)??;
        Ok(self.retain_response_stream(stream))
    }

    /// Deletes one streamable HTTP session using rmcp's reqwest implementation with the resolved effective auth header.
    /// 使用解析后的生效鉴权头，通过 rmcp 的 reqwest 实现删除单条 streamable HTTP 会话。
    async fn delete_session(
        &self,
        uri: std::sync::Arc<str>,
        session_id: std::sync::Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<(), rmcp::transport::streamable_http_client::StreamableHttpError<Self::Error>> {
        let auth_header = self.resolve_auth_header(auth_header).await?;
        <reqwest::Client as StreamableHttpClient>::delete_session(
            self.http_client.as_ref(),
            uri,
            session_id,
            auth_header,
            custom_headers,
        )
        .await
    }

    /// Posts one MCP client message using rmcp's reqwest implementation with the resolved effective auth header.
    /// 使用解析后的生效鉴权头，通过 rmcp 的 reqwest 实现发送单条 MCP 客户端消息。
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: rmcp::model::ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        // The upstream initialization await does not select worker cancellation on its own.
        // 上游初始化等待自身未选择工作任务取消。
        let response = self
            .cancellation
            .run_until_cancelled(async {
                let auth_header = self.resolve_auth_header(auth_header).await?;
                // Streaming POST responses become independent worker inputs after this request returns.
                // 此请求返回后，流式 POST 响应成为独立工作任务输入。
                <reqwest::Client as StreamableHttpClient>::post_message(
                    self.http_client.as_ref(),
                    uri,
                    message,
                    session_id,
                    auth_header,
                    custom_headers,
                )
                .await
            })
            .await
            .ok_or(StreamableHttpError::TransportChannelClosed)??;
        match response {
            StreamableHttpPostResponse::Sse(stream, session) => Ok(
                StreamableHttpPostResponse::Sse(self.retain_response_stream(stream), session),
            ),
            response @ (StreamableHttpPostResponse::Accepted
            | StreamableHttpPostResponse::Json(..)) => Ok(response),
            // The upstream enum is non-exhaustive; a new resource-bearing variant requires explicit ownership review.
            // 上游枚举非穷尽；新增携带资源的变体必须明确审核所有权。
            _ => Err(StreamableHttpError::UnexpectedServerResponse(
                Cow::Borrowed("Unsupported MCP HTTP response ownership variant."),
            )),
        }
    }
}

/// Production streamable-HTTP MCP client.
/// 生产级 streamable-HTTP MCP 客户端。
pub struct HttpTransport {
    /// Stable endpoint label surfaced to the host runtime.
    /// 暴露给宿主运行时的稳定端点标签。
    endpoint: String,
    /// Cloneable rmcp client peer used for schema discovery and calls.
    /// 用于 schema 发现与调用的可克隆 rmcp 客户端 peer。
    peer: Peer<RoleClient>,
    /// Owns the rmcp service and its single shared close operation.
    /// 拥有 rmcp 服务及其唯一共享关闭操作。
    shutdown: TransportShutdown,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl HttpTransport {
    /// Connects with exact host resources retained by all clients and response streams.
    /// 连接并由全部客户端及响应流保留精确宿主资源。
    /// Endpoint identifies the connection; uri, tokens, headers and auth_session define its request policy.
    /// endpoint 标识连接；uri、令牌、请求头及 auth_session 定义请求策略。
    /// Resources owns the frozen host authority; returns an initialized transport or the original error.
    /// resources 拥有冻结宿主权威；返回已初始化传输或原错误。
    pub async fn connect_with_resources(
        endpoint: impl Into<String>,
        uri: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
        resources: McpTransportResources,
    ) -> Result<Self, McpRuntimeError> {
        let endpoint = endpoint.into();
        let uri = uri.into();
        let mut config = StreamableHttpClientTransportConfig::with_uri(uri.clone());
        let parsed_headers = parse_http_headers_to_hash_map(custom_headers)?;

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

        // The official worker's token must reach startup requests as well as its steady-state loop.
        // 官方工作任务令牌必须同时传至启动请求及其稳定运行循环。
        let client = AuthenticatedStreamableHttpClient::new(auth_session, resources)?;
        let cancellation = client.cancellation.clone();
        let transport = StreamableHttpClientTransport::spawn_with_ct(
            StreamableHttpClientWorker::new(client, config),
            cancellation,
        );
        // Keep transport cleanup failures even when the official service task exits normally.
        // 官方服务任务正常退出时，仍保留传输清理失败。
        let (transport, backend_close) = observe_backend_close(transport, "mcp.shutdown(http)");
        let (event_sender, _) = broadcast::channel(32);
        let handler = Arc::new(EventingClientHandler::new(event_sender.clone()));
        let service = handler
            .serve(transport)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.connect(http): {error}")))?;
        let peer = service.peer().clone();
        spawn_rmcp_disconnect_watch(endpoint.clone(), "HTTP", peer.clone(), event_sender.clone());

        Ok(Self {
            endpoint,
            peer,
            shutdown: TransportShutdown::for_service("mcp.shutdown(http)", service, backend_close),
            event_sender,
        })
    }

    /// Connect to one streamable-HTTP MCP endpoint.
    /// 连接到单个 streamable-HTTP MCP 端点。
    pub async fn connect(
        endpoint: impl Into<String>,
        uri: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
    ) -> Result<Self, McpRuntimeError> {
        // Empty host ownership preserves the original public connection API.
        // 空宿主所有权保留原公开连接接口。
        let (resources, _) = McpTransportResources::new(());
        Self::connect_with_resources(
            endpoint,
            uri,
            bearer_token,
            custom_headers,
            auth_session,
            resources,
        )
        .await
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

    /// Gracefully closes the streamable-HTTP rmcp service and waits for cleanup.
    /// 优雅关闭 Streamable HTTP rmcp 服务并等待清理完成。
    /// Returns the original close result or an observation error without restarting cleanup.
    /// 返回原关闭结果或观测错误，不重新启动清理。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.shutdown.wait().await
    }

    /// Waits for the original HTTP close operation without restarting it after an observer timeout.
    /// 等待原 HTTP 关闭操作，不在观察者超时后重新启动。
    /// No inputs; returns its final cleanup result or an explicit worker interruption.
    /// 无输入；返回最终清理结果或明确的工作任务中断。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.shutdown.wait_until_complete().await
    }

    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        list_rmcp_tools(&self.peer, "http").await
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>, McpRuntimeError> {
        list_rmcp_prompts(&self.peer, "http").await
    }

    async fn get_prompt(
        &self,
        name: &str,
        args: Value,
    ) -> Result<GetPromptResult, McpRuntimeError> {
        get_rmcp_prompt(&self.peer, "http", name, args).await
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpRuntimeError> {
        list_rmcp_resources(&self.peer, "http").await
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpRuntimeError> {
        read_rmcp_resource(&self.peer, "http", uri).await
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError> {
        call_rmcp_tool(&self.peer, "http", name, args).await
    }

    /// Projects one completed call for its declared consumer without altering transport behavior.
    /// 为声明消费者投影一次已完成调用，不改变传输行为。
    async fn call_tool_with_audience(
        &self,
        name: &str,
        args: Value,
        audience: crate::McpToolResultAudience,
    ) -> Result<Value, McpRuntimeError> {
        crate::rmcp_support::call_rmcp_tool_with_audience(&self.peer, "http", name, args, audience)
            .await
    }
}

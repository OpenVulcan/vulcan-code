//! Streamable HTTP MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 Streamable HTTP MCP 客户端传输。

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use rmcp::service::{Peer, RoleClient, ServiceExt};
use rmcp::transport::{
    StreamableHttpClientTransport,
    streamable_http_client::{StreamableHttpClient, StreamableHttpClientTransportConfig},
};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::auth::{McpAuthChallenge, McpAuthStatus, McpOAuthSession};
use crate::error::McpRuntimeError;
use crate::http_support::parse_http_headers_to_hash_map;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, get_rmcp_prompt, list_rmcp_prompts, list_rmcp_resources,
    list_rmcp_tools, read_rmcp_resource, spawn_rmcp_disconnect_watch,
};
use crate::shutdown::TransportShutdown;
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

/// Auth-aware streamable HTTP client that delegates the wire protocol to rmcp's reqwest implementation while resolving live OAuth headers.
/// 具备鉴权能力的 streamable HTTP 客户端；它将线协议委托给 rmcp 的 reqwest 实现，同时解析实时 OAuth 请求头。
#[derive(Debug, Clone)]
struct AuthenticatedStreamableHttpClient {
    /// Shared reqwest client reused across stream and request operations.
    /// 在事件流与请求操作之间复用的共享 reqwest 客户端。
    http_client: reqwest::Client,
    /// Optional shared host-owned OAuth session used to resolve live bearer tokens.
    /// 用于解析实时 bearer token 的可选共享宿主自有 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
}

impl AuthenticatedStreamableHttpClient {
    /// Builds one auth-aware streamable HTTP client from an optional shared OAuth session.
    /// 根据可选共享 OAuth 会话构建单个具备鉴权能力的 streamable HTTP 客户端。
    fn new(auth_session: Option<Arc<McpOAuthSession>>) -> Result<Self, McpRuntimeError> {
        let http_client = reqwest::Client::builder()
            .build()
            .map_err(|error| McpRuntimeError::transport(format!("build HTTP client: {error}")))?;

        Ok(Self {
            http_client,
            auth_session,
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
        uri: std::sync::Arc<str>,
        session_id: Option<std::sync::Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<
        futures::stream::BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>,
        rmcp::transport::streamable_http_client::StreamableHttpError<Self::Error>,
    > {
        let auth_header = self.resolve_auth_header(auth_header).await?;
        <reqwest::Client as StreamableHttpClient>::get_stream(
            &self.http_client,
            uri,
            session_id,
            last_event_id,
            auth_header,
            custom_headers,
        )
        .await
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
            &self.http_client,
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
        uri: std::sync::Arc<str>,
        message: rmcp::model::ClientJsonRpcMessage,
        session_id: Option<std::sync::Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<
        rmcp::transport::streamable_http_client::StreamableHttpPostResponse,
        rmcp::transport::streamable_http_client::StreamableHttpError<Self::Error>,
    > {
        let auth_header = self.resolve_auth_header(auth_header).await?;
        <reqwest::Client as StreamableHttpClient>::post_message(
            &self.http_client,
            uri,
            message,
            session_id,
            auth_header,
            custom_headers,
        )
        .await
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
    /// Optional shared host-owned OAuth session reused across reconnects.
    /// 在重连之间复用的可选共享宿主自有 OAuth 会话。
    auth_session: Option<Arc<McpOAuthSession>>,
}

impl HttpTransport {
    /// Connect to one streamable-HTTP MCP endpoint.
    /// 连接到单个 streamable-HTTP MCP 端点。
    pub async fn connect(
        endpoint: impl Into<String>,
        uri: impl Into<String>,
        bearer_token: Option<String>,
        custom_headers: HashMap<String, String>,
        auth_session: Option<Arc<McpOAuthSession>>,
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

        let transport = StreamableHttpClientTransport::with_client(
            AuthenticatedStreamableHttpClient::new(auth_session.clone())?,
            config,
        );
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
            shutdown: TransportShutdown::for_service("mcp.shutdown(http)", service),
            event_sender,
            auth_session,
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

    /// Gracefully closes the streamable-HTTP rmcp service and waits for cleanup.
    /// 优雅关闭 Streamable HTTP rmcp 服务并等待清理完成。
    /// Returns the original close result or an observation error without restarting cleanup.
    /// 返回原关闭结果或观测错误，不重新启动清理。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.shutdown.wait().await
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
                "MCP HTTP 传输 `{}` 未配置 OAuth。",
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
                "MCP HTTP 传输 `{}` 未配置 OAuth。",
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

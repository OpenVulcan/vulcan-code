//! Streamable HTTP MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 Streamable HTTP MCP 客户端传输。

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::{
    StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::error::McpRuntimeError;
use crate::http_support::parse_http_headers_to_hash_map;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, list_rmcp_tools, spawn_rmcp_disconnect_watch,
};
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

        let transport = StreamableHttpClientTransport::from_config(config);
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
        list_rmcp_tools(&self.peer, "http").await
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError> {
        call_rmcp_tool(&self.peer, "http", name, args).await
    }
}

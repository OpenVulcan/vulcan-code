//! MCP registry glue used by the desktop runtime foundation.
//! 桌面运行时基础架构使用的 MCP 注册表胶水层。

use crate::mcp::config::{McpServerConfig, McpTransportKind, runtime_mcp_server_configs};
use crate::provider_config::ProviderConfigurationFile;
use std::sync::Arc;
use vulcan_mcp_runtime::{HttpTransport, McpTransport, SseTransport, StdioTransport};

/// Runtime registry of configured MCP servers.
/// 已配置 MCP 服务器的运行时注册表。
#[derive(Debug, Clone, Default)]
pub struct McpServerRegistry {
    /// Runtime-facing server entries.
    /// 面向运行时的服务器条目集合。
    servers: Vec<McpServerConfig>,
}

impl McpServerRegistry {
    /// Builds the runtime registry from the shared provider configuration file.
    /// 根据共享供应商配置文件构建运行时注册表。
    pub fn from_provider_configuration(config: &ProviderConfigurationFile) -> Self {
        Self {
            servers: runtime_mcp_server_configs(config),
        }
    }

    /// Returns every configured MCP server entry.
    /// 返回全部已配置的 MCP 服务器条目。
    pub fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }

    /// Returns the number of configured MCP servers.
    /// 返回已配置的 MCP 服务器数量。
    pub fn configured_count(&self) -> usize {
        self.servers.len()
    }

    /// Returns the number of enabled MCP servers.
    /// 返回已启用的 MCP 服务器数量。
    pub fn enabled_count(&self) -> usize {
        self.servers.iter().filter(|server| server.enabled).count()
    }

    /// Returns the enabled MCP server entries.
    /// 返回已启用的 MCP 服务器条目。
    pub fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers
            .iter()
            .filter(|server| server.enabled)
            .collect::<Vec<_>>()
    }

    /// Returns lightweight human-readable row labels for UI placeholder surfaces.
    /// 返回供 UI 占位面板使用的轻量人类可读行文本。
    pub fn summary_rows(&self) -> Vec<String> {
        self.servers
            .iter()
            .map(|server| {
                format!(
                    "{} · {} · {} · {}",
                    if server.enabled { "启用" } else { "禁用" },
                    server.name,
                    server.transport.label(),
                    server.command_summary()
                )
            })
            .collect()
    }

    /// Spawns one MCP transport from the stored server entry.
    /// 根据存储的服务器条目启动一个 MCP 传输实例。
    pub async fn spawn_transport(&self, server_id: &str) -> Result<Arc<dyn McpTransport>, String> {
        let server = self
            .servers
            .iter()
            .find(|server| server.id == server_id)
            .ok_or_else(|| format!("未找到 MCP 服务器 `{server_id}`。"))?;

        if !server.enabled {
            return Err(format!("MCP 服务器 `{}` 当前未启用。", server.name));
        }

        match server.transport {
            McpTransportKind::Stdio => {
                let args = server.args.clone();
                let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                let cwd = server.cwd.as_deref().map(std::path::Path::new);
                let transport = StdioTransport::spawn(
                    server.command_summary(),
                    &server.command,
                    &arg_refs,
                    cwd,
                    &server.env,
                )
                .await
                .map_err(|error| format!("启动 MCP 服务器 `{}` 失败：{}。", server.name, error))?;
                Ok(Arc::new(transport))
            }
            McpTransportKind::Http => {
                let url = server
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 HTTP URL。", server.name))?;
                let transport = HttpTransport::connect(
                    server.command_summary(),
                    url,
                    server.bearer_token.clone(),
                    server.headers.clone().into_iter().collect(),
                )
                .await
                .map_err(|error| {
                    format!("连接 MCP HTTP 服务器 `{}` 失败：{}。", server.name, error)
                })?;
                Ok(Arc::new(transport))
            }
            McpTransportKind::Sse => {
                let url = server
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 SSE URL。", server.name))?;
                let transport = SseTransport::connect(
                    server.command_summary(),
                    url,
                    server.bearer_token.clone(),
                    server.headers.clone().into_iter().collect(),
                )
                .await
                .map_err(|error| {
                    format!("连接 MCP SSE 服务器 `{}` 失败：{}。", server.name, error)
                })?;
                Ok(Arc::new(transport))
            }
        }
    }
}

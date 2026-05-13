//! MCP registry glue used by the desktop runtime foundation.
//! 桌面运行时基础架构使用的 MCP 注册表胶水层。

use crate::mcp::config::{McpServerConfig, runtime_mcp_server_configs};
use crate::provider_config::ProviderConfigurationFile;
use std::sync::Arc;
use vulcan_mcp_runtime::McpTransport;

/// Runtime registry of configured MCP servers.
/// 已配置 MCP 服务器的运行时注册表。
#[derive(Debug, Clone, Default)]
pub struct McpServerRegistry {
    /// Runtime-facing server entries.
    /// 面向运行时的服务器条目集合。
    servers: Vec<McpServerConfig>,
}

/// Lightweight configured MCP summary retained by GUI surfaces that only need counts and fallback rows.
/// 仅供需要计数与回退展示行的 GUI 表面保留的轻量已配置 MCP 摘要。
#[derive(Debug, Clone, Default)]
pub struct McpRegistrySummary {
    /// Number of configured MCP servers.
    /// 已配置 MCP 服务器数量。
    configured_count: usize,
    /// Number of enabled MCP servers.
    /// 已启用 MCP 服务器数量。
    enabled_count: usize,
    /// Human-readable summary rows for all configured servers.
    /// 面向全部已配置服务器的人类可读摘要行。
    summary_rows: Vec<String>,
    /// Human-readable summary rows for enabled configured servers only.
    /// 仅面向已启用已配置服务器的人类可读摘要行。
    enabled_summary_rows: Vec<String>,
}

impl McpRegistrySummary {
    /// Returns the number of configured MCP servers.
    /// 返回已配置 MCP 服务器数量。
    pub fn configured_count(&self) -> usize {
        self.configured_count
    }

    /// Returns the number of enabled MCP servers.
    /// 返回已启用 MCP 服务器数量。
    pub fn enabled_count(&self) -> usize {
        self.enabled_count
    }

    /// Returns lightweight summary rows for all configured servers.
    /// 返回全部已配置服务器的轻量摘要行。
    pub fn summary_rows(&self) -> Vec<String> {
        self.summary_rows.clone()
    }

    /// Returns lightweight summary rows for enabled configured servers only.
    /// 返回仅已启用已配置服务器的轻量摘要行。
    pub fn enabled_summary_rows(&self) -> Vec<String> {
        self.enabled_summary_rows.clone()
    }
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

    /// Returns lightweight summary rows for enabled MCP servers only.
    /// 仅返回已启用 MCP 服务器的轻量摘要行集合。
    pub fn enabled_summary_rows(&self) -> Vec<String> {
        self.servers
            .iter()
            .filter(|server| server.enabled)
            .map(|server| {
                format!(
                    "{} · {} · {} · {}",
                    "启用",
                    server.name,
                    server.transport.label(),
                    server.command_summary()
                )
            })
            .collect()
    }

    /// Builds one lightweight summary snapshot for GUI layers that do not need transport-spawning capabilities.
    /// 为不需要传输启动能力的 GUI 层构建一个轻量摘要快照。
    pub fn summary(&self) -> McpRegistrySummary {
        McpRegistrySummary {
            configured_count: self.configured_count(),
            enabled_count: self.enabled_count(),
            summary_rows: self.summary_rows(),
            enabled_summary_rows: self.enabled_summary_rows(),
        }
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

        let launch_spec = server.launch_spec()?;
        launch_spec
            .launch()
            .await
            .map_err(|error| server.launch_error_text(error))
    }
}

#[cfg(test)]
mod tests {
    use super::{McpRegistrySummary, McpServerRegistry};
    use crate::mcp::config::{McpServerConfig, McpTransportKind};
    use std::collections::BTreeMap;

    #[test]
    fn enabled_summary_rows_skip_disabled_servers() {
        let registry = McpServerRegistry {
            servers: vec![
                McpServerConfig {
                    id: String::from("enabled"),
                    name: String::from("Enabled Server"),
                    enabled: true,
                    transport: McpTransportKind::Stdio,
                    command: String::from("enabled-mcp"),
                    args: vec![String::from("--stdio")],
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    cwd: None,
                    env: BTreeMap::new(),
                },
                McpServerConfig {
                    id: String::from("disabled"),
                    name: String::from("Disabled Server"),
                    enabled: false,
                    transport: McpTransportKind::Http,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(String::from("https://disabled.test/mcp")),
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    cwd: None,
                    env: BTreeMap::new(),
                },
            ],
        };

        assert_eq!(
            registry.enabled_summary_rows(),
            vec![String::from(
                "启用 · Enabled Server · stdio · enabled-mcp --stdio"
            )]
        );
    }

    #[test]
    fn registry_summary_preserves_counts_and_rows() {
        let registry = McpServerRegistry {
            servers: vec![
                McpServerConfig {
                    id: String::from("enabled"),
                    name: String::from("Enabled Server"),
                    enabled: true,
                    transport: McpTransportKind::Stdio,
                    command: String::from("enabled-mcp"),
                    args: vec![String::from("--stdio")],
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    cwd: None,
                    env: BTreeMap::new(),
                },
                McpServerConfig {
                    id: String::from("disabled"),
                    name: String::from("Disabled Server"),
                    enabled: false,
                    transport: McpTransportKind::Http,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(String::from("https://disabled.test/mcp")),
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    cwd: None,
                    env: BTreeMap::new(),
                },
            ],
        };

        let summary: McpRegistrySummary = registry.summary();

        assert_eq!(summary.configured_count(), 2);
        assert_eq!(summary.enabled_count(), 1);
        assert_eq!(summary.summary_rows().len(), 2);
        assert_eq!(
            summary.enabled_summary_rows(),
            vec![String::from(
                "启用 · Enabled Server · stdio · enabled-mcp --stdio"
            )]
        );
    }
}

//! MCP registry glue used by the desktop runtime foundation.
//! 桌面运行时基础架构使用的 MCP 注册表胶水层。

use crate::mcp::config::McpServerConfig;
use crate::mcp::runtime::{McpRuntimeDiscoveryReport, McpRuntimeOverview};
use std::sync::Arc;
use vulcan_mcp_runtime::{McpOAuthSession, McpTransport};

/// Runtime registry of configured MCP servers.
/// 已配置 MCP 服务器的运行时注册表。
#[derive(Debug, Clone, Default)]
pub(crate) struct McpServerRegistry {
    /// Runtime-facing server entries.
    /// 面向运行时的服务器条目集合。
    servers: Vec<McpServerConfig>,
}

impl McpServerRegistry {
    /// Builds a source-neutral registry from already validated server definitions.
    /// 根据已经校验的中立服务器定义构建注册表。
    pub(crate) fn new(servers: Vec<McpServerConfig>) -> Self {
        Self { servers }
    }

    /// Returns every configured MCP server entry.
    /// 返回全部已配置的 MCP 服务器条目。
    pub(crate) fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }

    /// Resolves one configured MCP server by stable identifier.
    /// 根据稳定标识解析单个已配置 MCP 服务器。
    pub(crate) fn server_by_id(&self, server_id: &str) -> Option<&McpServerConfig> {
        self.servers.iter().find(|server| server.id == server_id)
    }

    /// Returns the enabled MCP server entries.
    /// 返回已启用的 MCP 服务器条目。
    pub(crate) fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers
            .iter()
            .filter(|server| server.enabled)
            .collect::<Vec<_>>()
    }

    /// Returns lightweight human-readable row labels for UI placeholder surfaces.
    /// 返回供 UI 占位面板使用的轻量人类可读行文本。
    pub(crate) fn summary_rows(&self) -> Vec<String> {
        self.servers
            .iter()
            .map(|server| {
                format!(
                    "{} · {} · {} · {}",
                    if server.enabled { "启用" } else { "禁用" },
                    server.name,
                    server.transport_kind().label(),
                    server.command_summary()
                )
            })
            .collect()
    }

    /// Returns lightweight summary rows for enabled MCP servers only.
    /// 仅返回已启用 MCP 服务器的轻量摘要行集合。
    pub(crate) fn enabled_summary_rows(&self) -> Vec<String> {
        self.servers
            .iter()
            .filter(|server| server.enabled)
            .map(|server| {
                format!(
                    "{} · {} · {} · {}",
                    "启用",
                    server.name,
                    server.transport_kind().label(),
                    server.command_summary()
                )
            })
            .collect()
    }

    /// Builds one lightweight overview snapshot for service layers that do not need transport-spawning capabilities.
    /// 为不需要传输启动能力的服务层构建一个轻量概览快照。
    pub(crate) fn overview(&self) -> McpRuntimeOverview {
        McpRuntimeOverview::new(
            self.summary_rows(),
            self.enabled_summary_rows(),
            McpRuntimeDiscoveryReport::default(),
        )
    }

    /// Spawns one MCP transport from the stored server entry.
    /// 根据存储的服务器条目启动一个 MCP 传输实例。
    /// Server_id selects the exact declaration; auth_session supplies OAuth, resources owns its frozen host authority.
    /// server_id 选择精确声明；auth_session 提供 OAuth，resources 拥有其冻结宿主权威。
    /// Returns an initialized transport or the sanitized launch error; the caller keeps the independent release receipt.
    /// 返回已初始化传输或脱敏启动错误；调用方保留独立释放回执。
    pub(crate) async fn spawn_transport(
        &self,
        server_id: &str,
        auth_session: Option<Arc<McpOAuthSession>>,
        resources: vulcan_mcp_runtime::McpTransportResources,
    ) -> Result<Arc<dyn McpTransport>, String> {
        let server = self
            .server_by_id(server_id)
            .ok_or_else(|| format!("未找到 MCP 服务器 `{server_id}`。"))?;

        if !server.enabled {
            return Err(format!("MCP 服务器 `{}` 当前未启用。", server.name));
        }

        let launch_spec = server.launch_spec()?;
        launch_spec
            .launch_with_resources(auth_session, resources)
            .await
            .map_err(|error| server.launch_error_text(error))
    }
}

/// Builds one lightweight MCP overview from source-neutral server definitions.
/// 根据中立服务器定义构建一个轻量 MCP 概览。
pub(crate) fn configured_mcp_registry_overview(
    servers: Vec<McpServerConfig>,
) -> McpRuntimeOverview {
    McpServerRegistry::new(servers).overview()
}

#[cfg(test)]
mod tests {
    use super::{McpServerRegistry, configured_mcp_registry_overview};
    use crate::mcp::config::{McpServerConfig, McpServerSource, McpServerTransportConfig};
    use crate::provider_config::{
        PersistedMcpConfiguration, PersistedMcpServerEntry, PersistedMcpTransportKind,
        ProviderConfigurationFile,
    };
    use std::collections::BTreeMap;

    /// Builds one deterministic source-neutral test server.
    /// 构建一个确定性的中立测试服务器。
    fn test_server(id: &str, name: &str, enabled: bool, remote: bool) -> McpServerConfig {
        McpServerConfig {
            source: McpServerSource::UserConfigured,
            id: id.to_owned(),
            name: name.to_owned(),
            enabled,
            transport: if remote {
                McpServerTransportConfig::Http {
                    url: String::from("https://disabled.test/mcp"),
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    headers_from_env: BTreeMap::new(),
                    oauth: None,
                }
            } else {
                McpServerTransportConfig::Stdio {
                    command: String::from("enabled-mcp"),
                    args: vec![String::from("--stdio")],
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                    inherit_environment: true,
                }
            },
        }
    }

    #[test]
    fn enabled_summary_rows_skip_disabled_servers() {
        let registry = McpServerRegistry {
            servers: vec![
                test_server("enabled", "Enabled Server", true, false),
                test_server("disabled", "Disabled Server", false, true),
            ],
        };

        assert_eq!(
            registry.enabled_summary_rows(),
            vec![String::from(
                "启用 · Enabled Server · stdio · enabled-mcp <1 arguments>"
            )]
        );
    }

    #[test]
    fn registry_overview_preserves_rows_and_discovery_defaults() {
        let registry = McpServerRegistry {
            servers: vec![
                test_server("enabled", "Enabled Server", true, false),
                test_server("disabled", "Disabled Server", false, true),
            ],
        };

        let overview = registry.overview();

        assert_eq!(overview.summary_rows().len(), 2);
        assert_eq!(
            overview.active_summary_rows(),
            &[String::from(
                "启用 · Enabled Server · stdio · enabled-mcp <1 arguments>"
            )]
        );
        assert!(overview.discovery_report().is_empty());
    }

    #[test]
    fn configured_registry_overview_builds_from_provider_configuration() {
        let config = ProviderConfigurationFile {
            mcp: PersistedMcpConfiguration {
                servers: BTreeMap::from([(
                    String::from("server-1"),
                    PersistedMcpServerEntry {
                        name: String::from("Configured Server"),
                        enabled: true,
                        transport: PersistedMcpTransportKind::Stdio,
                        command: String::from("configured-mcp"),
                        args: vec![String::from("--stdio")],
                        url: None,
                        bearer_token: None,
                        headers: BTreeMap::new(),
                        headers_from_env: BTreeMap::new(),
                        oauth: None,
                        cwd: None,
                        env: BTreeMap::new(),
                        env_passthrough: Vec::new(),
                    },
                )]),
            },
            ..ProviderConfigurationFile::default()
        };

        let overview = configured_mcp_registry_overview(
            crate::mcp::config::runtime_mcp_server_configs(&config)
                .expect("persisted fixture should map"),
        );

        assert_eq!(
            overview.summary_rows(),
            &[String::from(
                "启用 · Configured Server · stdio · configured-mcp <1 arguments>"
            )]
        );
        assert!(overview.discovery_report().is_empty());
    }
}

//! Minimal MCP runtime bridge used by the Core Service runtime.
//! Core Service 运行时使用的最小 MCP 运行时桥接层。

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::Mutex;
use vulcan_mcp_runtime::{McpTransport, McpTransportLaunchSpec, McpTransportOAuthSpec};
use vulcan_provider_config::config::{
    PersistedMcpOAuthConfig, PersistedMcpServerEntry, PersistedMcpTransportKind,
    ProviderConfigurationFile,
};

/// Long-lived MCP runtime handle retained by the service runtime.
/// 由服务运行时保留的长生命周期 MCP 运行时句柄。
#[derive(Clone)]
pub struct TauriMcpRuntimeHandle {
    /// Runtime server snapshots keyed by stable server id.
    /// 以稳定服务器标识为键保存的运行时服务器快照。
    servers: BTreeMap<String, TauriMcpServerSnapshot>,
    /// Active transports retained for later tool invocation.
    /// 为后续工具调用保留的活动传输实例。
    _transports: Arc<Mutex<BTreeMap<String, Arc<dyn McpTransport>>>>,
}

impl std::fmt::Debug for TauriMcpRuntimeHandle {
    /// Formats MCP runtime diagnostics without dumping opaque transports.
    /// 在不展开不透明传输对象的前提下格式化 MCP 运行时诊断。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TauriMcpRuntimeHandle")
            .field("servers", &self.servers)
            .finish_non_exhaustive()
    }
}

/// Lightweight MCP runtime catalog exposed to the WebUI snapshot.
/// 暴露给 WebUI 快照的轻量 MCP 运行时目录。
#[derive(Debug, Clone)]
pub struct TauriMcpRuntimeCatalog {
    /// Configured server count.
    /// 已配置服务器数量。
    pub configured_server_count: usize,
    /// Enabled server count.
    /// 已启用服务器数量。
    pub enabled_server_count: usize,
    /// Connected server count.
    /// 已连接服务器数量。
    pub connected_server_count: usize,
    /// Discovered tool count.
    /// 已发现工具数量。
    pub tool_count: usize,
    /// Discovered prompt count.
    /// 已发现 prompt 数量。
    pub prompt_count: usize,
    /// Discovered resource count.
    /// 已发现 resource 数量。
    pub resource_count: usize,
    /// Human-readable summary rows.
    /// 人类可读摘要行。
    pub summary_rows: Vec<String>,
}

/// Runtime snapshot for one configured MCP server.
/// 单个已配置 MCP 服务器的运行时快照。
#[derive(Debug, Clone)]
struct TauriMcpServerSnapshot {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    name: String,
    /// Whether the server is enabled.
    /// 服务器是否已启用。
    enabled: bool,
    /// Transport family label.
    /// 传输类型标签。
    transport_label: String,
    /// Compact endpoint or command summary.
    /// 紧凑端点或命令摘要。
    endpoint_summary: String,
    /// Current runtime status label.
    /// 当前运行时状态标签。
    status: String,
    /// Discovered tool count.
    /// 已发现工具数量。
    tool_count: usize,
    /// Discovered prompt count.
    /// 已发现 prompt 数量。
    prompt_count: usize,
    /// Discovered resource count.
    /// 已发现 resource 数量。
    resource_count: usize,
    /// Latest connection or discovery error.
    /// 最近一次连接或发现错误。
    last_error: Option<String>,
}

impl TauriMcpRuntimeHandle {
    /// Returns the current lightweight MCP catalog.
    /// 返回当前轻量 MCP 目录。
    pub fn catalog(&self) -> TauriMcpRuntimeCatalog {
        let configured_server_count = self.servers.len();
        let enabled_server_count = self
            .servers
            .values()
            .filter(|server| server.enabled)
            .count();
        let connected_server_count = self
            .servers
            .values()
            .filter(|server| server.status == "在线")
            .count();
        let tool_count = self.servers.values().map(|server| server.tool_count).sum();
        let prompt_count = self
            .servers
            .values()
            .map(|server| server.prompt_count)
            .sum();
        let resource_count = self
            .servers
            .values()
            .map(|server| server.resource_count)
            .sum();
        let summary_rows = self
            .servers
            .values()
            .map(|server| {
                let base = format!(
                    "{} · {} · {} · {} · tools={} prompts={} resources={}",
                    server.status,
                    server.name,
                    server.transport_label,
                    server.endpoint_summary,
                    server.tool_count,
                    server.prompt_count,
                    server.resource_count
                );
                match &server.last_error {
                    Some(error) => format!("{base} · {error}"),
                    None => base,
                }
            })
            .collect();

        TauriMcpRuntimeCatalog {
            configured_server_count,
            enabled_server_count,
            connected_server_count,
            tool_count,
            prompt_count,
            resource_count,
            summary_rows,
        }
    }
}

/// Boots one minimal MCP runtime from the persisted provider configuration.
/// 根据持久化供应商配置引导一个最小 MCP 运行时。
pub async fn bootstrap_mcp_runtime(
    config: ProviderConfigurationFile,
) -> Result<TauriMcpRuntimeHandle, String> {
    let mut servers = BTreeMap::new();
    let transports = Arc::new(Mutex::new(BTreeMap::new()));

    for (server_id, entry) in config.mcp.servers {
        let server = TauriMcpServerConfig::from_persisted(server_id.clone(), entry);
        if !server.enabled {
            servers.insert(server_id, server.disabled_snapshot());
            continue;
        }

        let snapshot = match connect_server(&server).await {
            Ok((transport, tool_count, prompt_count, resource_count)) => {
                transports.lock().await.insert(server_id.clone(), transport);
                server.connected_snapshot(tool_count, prompt_count, resource_count)
            }
            Err(error) => server.disconnected_snapshot(error),
        };
        servers.insert(server_id, snapshot);
    }

    Ok(TauriMcpRuntimeHandle {
        servers,
        _transports: transports,
    })
}

/// Connects one MCP server and discovers lightweight counts.
/// 连接单个 MCP 服务器并发现轻量统计数量。
async fn connect_server(
    server: &TauriMcpServerConfig,
) -> Result<(Arc<dyn McpTransport>, usize, usize, usize), String> {
    tokio::time::timeout(
        Duration::from_secs(8),
        connect_server_without_timeout(server),
    )
    .await
    .map_err(|_| format!("连接 MCP 服务器 `{}` 超时。", server.name))?
}

/// Connects one MCP server without applying the outer startup timeout.
/// 在不应用外层启动超时的情况下连接单个 MCP 服务器。
async fn connect_server_without_timeout(
    server: &TauriMcpServerConfig,
) -> Result<(Arc<dyn McpTransport>, usize, usize, usize), String> {
    let transport = server
        .launch_spec()?
        .launch(None)
        .await
        .map_err(|error| format!("连接 MCP 服务器 `{}` 失败：{}", server.name, error))?;
    let tools = transport
        .list_tools()
        .await
        .map_err(|error| format!("发现 MCP 工具失败：{error}"))?;
    let prompts = transport
        .list_prompts()
        .await
        .map_err(|error| format!("发现 MCP prompts 失败：{error}"))?;
    let resources = transport
        .list_resources()
        .await
        .map_err(|error| format!("发现 MCP resources 失败：{error}"))?;
    Ok((transport, tools.len(), prompts.len(), resources.len()))
}

/// Runtime-facing MCP server configuration entry.
/// 面向运行时的 MCP 服务器配置条目。
#[derive(Debug, Clone)]
struct TauriMcpServerConfig {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    name: String,
    /// Whether the server is enabled.
    /// 当前服务器是否启用。
    enabled: bool,
    /// Transport family.
    /// 传输类型。
    transport: PersistedMcpTransportKind,
    /// Process command.
    /// 进程命令。
    command: String,
    /// Process argument list.
    /// 进程参数列表。
    args: Vec<String>,
    /// Remote endpoint URL used by HTTP transports.
    /// HTTP 传输使用的远端端点 URL。
    url: Option<String>,
    /// Optional bearer token used by HTTP transports.
    /// HTTP 传输使用的可选 Bearer Token。
    bearer_token: Option<String>,
    /// Additional HTTP headers used by HTTP transports.
    /// HTTP 传输使用的附加 HTTP 请求头。
    headers: BTreeMap<String, String>,
    /// Optional HTTP-header values sourced from host environment variables.
    /// 从宿主环境变量取值的可选 HTTP 请求头映射。
    headers_from_env: BTreeMap<String, String>,
    /// Optional OAuth configuration used by remote authorization flows.
    /// 远端授权流程使用的可选 OAuth 配置。
    oauth: Option<PersistedMcpOAuthConfig>,
    /// Optional working directory.
    /// 可选工作目录。
    cwd: Option<String>,
    /// Additional environment variables.
    /// 附加环境变量集合。
    env: BTreeMap<String, String>,
    /// Optional environment-variable names passed through from the host process.
    /// 从宿主进程透传的可选环境变量名称集合。
    env_passthrough: Vec<String>,
}

impl TauriMcpServerConfig {
    /// Converts one persisted MCP server entry into the runtime config.
    /// 将单条持久化 MCP 服务器配置转换为运行时配置。
    fn from_persisted(_id: String, entry: PersistedMcpServerEntry) -> Self {
        Self {
            name: entry.name,
            enabled: entry.enabled,
            transport: entry.transport,
            command: entry.command,
            args: entry.args,
            url: entry.url,
            bearer_token: entry.bearer_token,
            headers: entry.headers,
            headers_from_env: entry.headers_from_env,
            oauth: entry.oauth,
            cwd: entry.cwd,
            env: entry.env,
            env_passthrough: entry.env_passthrough,
        }
    }

    /// Builds one launch spec for the configured transport.
    /// 为已配置传输构建启动规格。
    fn launch_spec(&self) -> Result<McpTransportLaunchSpec, String> {
        match self.transport {
            PersistedMcpTransportKind::Stdio => Ok(McpTransportLaunchSpec::Stdio {
                endpoint: self.endpoint_summary(),
                program: self.command.clone(),
                args: self.args.clone(),
                cwd: self.cwd.clone().map(PathBuf::from),
                env: self.resolved_env(),
            }),
            PersistedMcpTransportKind::Http => Ok(McpTransportLaunchSpec::Http {
                endpoint: self.endpoint_summary(),
                uri: self
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 HTTP URL。", self.name))?,
                bearer_token: self.bearer_token.clone(),
                headers: self.resolved_headers(),
                oauth: self.oauth.clone().map(mcp_oauth_spec_from_persisted),
            }),
            PersistedMcpTransportKind::Sse => Ok(McpTransportLaunchSpec::Sse {
                endpoint: self.endpoint_summary(),
                uri: self
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 SSE URL。", self.name))?,
                bearer_token: self.bearer_token.clone(),
                headers: self.resolved_headers(),
                oauth: self.oauth.clone().map(mcp_oauth_spec_from_persisted),
            }),
        }
    }

    /// Builds a compact endpoint or command summary.
    /// 构建紧凑端点或命令摘要。
    fn endpoint_summary(&self) -> String {
        match self.transport {
            PersistedMcpTransportKind::Stdio => {
                if self.args.is_empty() {
                    self.command.clone()
                } else {
                    format!("{} {}", self.command, self.args.join(" "))
                }
            }
            PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => self
                .url
                .clone()
                .unwrap_or_else(|| String::from("(missing url)")),
        }
    }

    /// Builds the disabled server snapshot.
    /// 构建禁用服务器快照。
    fn disabled_snapshot(&self) -> TauriMcpServerSnapshot {
        self.snapshot("禁用", 0, 0, 0, None)
    }

    /// Builds the connected server snapshot.
    /// 构建已连接服务器快照。
    fn connected_snapshot(
        &self,
        tool_count: usize,
        prompt_count: usize,
        resource_count: usize,
    ) -> TauriMcpServerSnapshot {
        self.snapshot("在线", tool_count, prompt_count, resource_count, None)
    }

    /// Builds the disconnected server snapshot.
    /// 构建已断开服务器快照。
    fn disconnected_snapshot(&self, error: String) -> TauriMcpServerSnapshot {
        self.snapshot("离线", 0, 0, 0, Some(error))
    }

    /// Builds one server snapshot with common fields.
    /// 使用通用字段构建单个服务器快照。
    fn snapshot(
        &self,
        status: &str,
        tool_count: usize,
        prompt_count: usize,
        resource_count: usize,
        last_error: Option<String>,
    ) -> TauriMcpServerSnapshot {
        TauriMcpServerSnapshot {
            name: self.name.clone(),
            enabled: self.enabled,
            transport_label: self.transport.label().to_string(),
            endpoint_summary: self.endpoint_summary(),
            status: status.to_string(),
            tool_count,
            prompt_count,
            resource_count,
            last_error,
        }
    }

    /// Resolves child-process environment variables.
    /// 解析子进程环境变量。
    fn resolved_env(&self) -> BTreeMap<String, String> {
        let mut resolved_env = self.env.clone();
        for env_name in self.env_passthrough.iter().map(|value| value.trim()) {
            if resolved_env.contains_key(env_name) {
                continue;
            }
            if let Ok(env_value) = std::env::var(env_name) {
                resolved_env.insert(env_name.to_string(), env_value);
            }
        }
        resolved_env
    }

    /// Resolves HTTP request headers.
    /// 解析 HTTP 请求头。
    fn resolved_headers(&self) -> BTreeMap<String, String> {
        let mut resolved_headers = self.headers.clone();
        for (header_name, env_name) in &self.headers_from_env {
            if resolved_headers.contains_key(header_name) {
                continue;
            }
            if let Ok(env_value) = std::env::var(env_name.trim()) {
                resolved_headers.insert(header_name.trim().to_string(), env_value);
            }
        }
        resolved_headers
    }
}

/// Converts persisted OAuth config into MCP runtime OAuth spec.
/// 将持久化 OAuth 配置转换为 MCP 运行时 OAuth 规格。
fn mcp_oauth_spec_from_persisted(value: PersistedMcpOAuthConfig) -> McpTransportOAuthSpec {
    McpTransportOAuthSpec {
        client_id: value.client_id,
        client_secret: value.client_secret,
        scopes: value.scopes,
        redirect_uri: value.redirect_uri,
    }
}

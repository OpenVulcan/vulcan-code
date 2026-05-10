//! MCP configuration mapping shared by the desktop runtime foundation.
//! 桌面运行时基础架构共享的 MCP 配置映射层。

use crate::provider_config::{
    PersistedMcpServerEntry, PersistedMcpTransportKind, ProviderConfigurationFile,
};
use std::collections::BTreeMap;

/// Runtime-facing MCP transport family.
/// 面向运行时的 MCP 传输类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportKind {
    /// Standard input/output child-process transport.
    /// 标准输入输出子进程传输。
    Stdio,
    /// Streamable HTTP client transport.
    /// Streamable HTTP 客户端传输。
    Http,
    /// Legacy server-sent-events MCP transport.
    /// 旧式基于 Server-Sent Events 的 MCP 传输。
    Sse,
}

impl McpTransportKind {
    /// Returns a compact human-readable label.
    /// 返回紧凑的人类可读标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

impl From<PersistedMcpTransportKind> for McpTransportKind {
    /// Converts one persisted transport kind into its runtime counterpart.
    /// 将一个持久化传输类型转换为对应的运行时传输类型。
    fn from(value: PersistedMcpTransportKind) -> Self {
        match value {
            PersistedMcpTransportKind::Stdio => Self::Stdio,
            PersistedMcpTransportKind::Http => Self::Http,
            PersistedMcpTransportKind::Sse => Self::Sse,
        }
    }
}

#[allow(dead_code)]
/// Runtime-facing MCP server configuration entry.
/// 面向运行时的 MCP 服务器配置条目。
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Stable server identifier.
    /// 稳定服务器标识。
    pub id: String,
    /// Human-readable server name.
    /// 人类可读的服务器名称。
    pub name: String,
    /// Whether the server is enabled.
    /// 当前服务器是否启用。
    pub enabled: bool,
    /// Transport family.
    /// 传输类型。
    pub transport: McpTransportKind,
    /// Process command.
    /// 进程命令。
    pub command: String,
    /// Process argument list.
    /// 进程参数列表。
    pub args: Vec<String>,
    /// Remote endpoint URL used by HTTP transports.
    /// HTTP 传输使用的远端端点 URL。
    pub url: Option<String>,
    /// Optional bearer token used by HTTP transports.
    /// HTTP 传输使用的可选 Bearer Token。
    pub bearer_token: Option<String>,
    /// Additional HTTP headers used by HTTP transports.
    /// HTTP 传输使用的附加 HTTP 请求头。
    pub headers: BTreeMap<String, String>,
    /// Optional working directory.
    /// 可选工作目录。
    pub cwd: Option<String>,
    /// Additional environment variables.
    /// 附加环境变量集合。
    pub env: BTreeMap<String, String>,
}

impl McpServerConfig {
    /// Returns a compact command-line summary.
    /// 返回紧凑的命令行摘要。
    pub fn command_summary(&self) -> String {
        match self.transport {
            McpTransportKind::Stdio => {
                if self.args.is_empty() {
                    self.command.clone()
                } else {
                    format!("{} {}", self.command, self.args.join(" "))
                }
            }
            McpTransportKind::Http | McpTransportKind::Sse => self
                .url
                .clone()
                .unwrap_or_else(|| String::from("(missing url)")),
        }
    }
}

/// Converts the persisted MCP server map into runtime-facing server entries.
/// 将持久化 MCP 服务器映射转换为面向运行时的服务器条目集合。
pub fn runtime_mcp_server_configs(config: &ProviderConfigurationFile) -> Vec<McpServerConfig> {
    config
        .mcp
        .servers
        .iter()
        .map(|(id, entry)| runtime_mcp_server_from_persisted(id, entry))
        .collect()
}

/// Converts one persisted MCP server entry into its runtime counterpart.
/// 将单条持久化 MCP 服务器条目转换为对应的运行时表示。
fn runtime_mcp_server_from_persisted(id: &str, entry: &PersistedMcpServerEntry) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        name: entry.name.clone(),
        enabled: entry.enabled,
        transport: entry.transport.into(),
        command: entry.command.clone(),
        args: entry.args.clone(),
        url: entry.url.clone(),
        bearer_token: entry.bearer_token.clone(),
        headers: entry.headers.clone(),
        cwd: entry.cwd.clone(),
        env: entry.env.clone(),
    }
}

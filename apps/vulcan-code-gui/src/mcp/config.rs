//! MCP configuration mapping shared by the desktop runtime foundation.
//! 桌面运行时基础架构共享的 MCP 配置映射层。

use crate::provider_config::{
    PersistedMcpServerEntry, PersistedMcpTransportKind, ProviderConfigurationFile,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use vulcan_mcp_runtime::McpTransportLaunchSpec;

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

    /// Converts the runtime-facing server configuration into a host-owned transport launch specification.
    /// 将面向运行时的服务器配置转换为宿主自有传输启动规格。
    pub fn launch_spec(&self) -> Result<McpTransportLaunchSpec, String> {
        let endpoint = self.command_summary();
        match self.transport {
            McpTransportKind::Stdio => Ok(McpTransportLaunchSpec::Stdio {
                endpoint,
                program: self.command.clone(),
                args: self.args.clone(),
                cwd: self.cwd.clone().map(PathBuf::from),
                env: self.env.clone(),
            }),
            McpTransportKind::Http => Ok(McpTransportLaunchSpec::Http {
                endpoint,
                uri: self
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 HTTP URL。", self.name))?,
                bearer_token: self.bearer_token.clone(),
                headers: self.headers.clone(),
            }),
            McpTransportKind::Sse => Ok(McpTransportLaunchSpec::Sse {
                endpoint,
                uri: self
                    .url
                    .clone()
                    .ok_or_else(|| format!("MCP 服务器 `{}` 缺少 SSE URL。", self.name))?,
                bearer_token: self.bearer_token.clone(),
                headers: self.headers.clone(),
            }),
        }
    }

    /// Converts one low-level launch error into user-facing transport diagnostics.
    /// 将底层启动错误转换为面向用户的传输诊断文本。
    pub fn launch_error_text(&self, error: impl std::fmt::Display) -> String {
        match self.transport {
            McpTransportKind::Stdio => {
                format!("启动 MCP 服务器 `{}` 失败：{}。", self.name, error)
            }
            McpTransportKind::Http => {
                format!("连接 MCP HTTP 服务器 `{}` 失败：{}。", self.name, error)
            }
            McpTransportKind::Sse => {
                format!("连接 MCP SSE 服务器 `{}` 失败：{}。", self.name, error)
            }
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

#[cfg(test)]
mod tests {
    use super::{McpServerConfig, McpTransportKind};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use vulcan_mcp_runtime::McpTransportLaunchSpec;

    fn sample_server(transport: McpTransportKind) -> McpServerConfig {
        McpServerConfig {
            id: String::from("server-1"),
            name: String::from("Sample Server"),
            enabled: true,
            transport,
            command: String::from("sample-mcp"),
            args: vec![String::from("--stdio"), String::from("--verbose")],
            url: Some(String::from("https://example.test/mcp")),
            bearer_token: Some(String::from("token-123")),
            headers: BTreeMap::from([(String::from("X-Test"), String::from("1"))]),
            cwd: Some(String::from("D:/mock/mcp")),
            env: BTreeMap::from([(String::from("VULCAN_MODE"), String::from("stdio"))]),
        }
    }

    #[test]
    fn stdio_launch_spec_preserves_cwd_and_env() {
        let server = sample_server(McpTransportKind::Stdio);

        assert_eq!(
            server.launch_spec().expect("stdio server should build"),
            McpTransportLaunchSpec::Stdio {
                endpoint: String::from("sample-mcp --stdio --verbose"),
                program: String::from("sample-mcp"),
                args: vec![String::from("--stdio"), String::from("--verbose")],
                cwd: Some(PathBuf::from("D:/mock/mcp")),
                env: BTreeMap::from([(String::from("VULCAN_MODE"), String::from("stdio"))]),
            }
        );
    }

    #[test]
    fn http_launch_spec_requires_remote_url() {
        let mut server = sample_server(McpTransportKind::Http);
        server.url = None;

        let error = server
            .launch_spec()
            .expect_err("http server must require URL");

        assert_eq!(
            error,
            String::from("MCP 服务器 `Sample Server` 缺少 HTTP URL。")
        );
    }
}

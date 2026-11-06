//! MCP configuration mapping shared by the desktop runtime foundation.
//! 桌面运行时基础架构共享的 MCP 配置映射层。

use crate::provider_config::{
    PersistedMcpOAuthConfig, PersistedMcpServerEntry, PersistedMcpTransportKind,
    ProviderConfigurationFile,
};
use std::collections::BTreeMap;
use std::fmt;

use std::path::PathBuf;
use vulcan_mcp_runtime::{McpTransportLaunchSpec, McpTransportOAuthSpec};

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
#[derive(Clone, Default)]
pub struct McpServerOAuthConfig {
    /// Optional pre-registered OAuth client identifier.
    /// 可选的预注册 OAuth 客户端标识。
    pub client_id: Option<String>,
    /// Optional pre-registered OAuth client secret.
    /// 可选的预注册 OAuth 客户端密钥。
    pub client_secret: Option<String>,
    /// Optional OAuth scopes requested during authorization.
    /// 授权过程中请求的可选 OAuth scopes 集合。
    pub scopes: Vec<String>,
    /// Optional redirect URI used by the OAuth authorization-code flow.
    /// OAuth 授权码流程使用的可选回调 URI。
    pub redirect_uri: Option<String>,
}

impl From<PersistedMcpOAuthConfig> for McpServerOAuthConfig {
    /// Converts one persisted OAuth configuration block into the runtime-facing representation.
    /// 将单个持久化 OAuth 配置块转换为面向运行时的表示。
    fn from(value: PersistedMcpOAuthConfig) -> Self {
        Self {
            client_id: value.client_id,
            client_secret: value.client_secret,
            scopes: value.scopes,
            redirect_uri: value.redirect_uri,
        }
    }
}

impl From<McpServerOAuthConfig> for McpTransportOAuthSpec {
    /// Converts one runtime-facing OAuth configuration block into the transport launch specification.
    /// 将单个面向运行时的 OAuth 配置块转换为传输启动规格。
    fn from(value: McpServerOAuthConfig) -> Self {
        Self {
            client_id: value.client_id,
            client_secret: value.client_secret,
            scopes: value.scopes,
            redirect_uri: value.redirect_uri,
        }
    }
}

/// Origin of one runtime-facing MCP server definition.
/// 单个面向运行时的 MCP 服务器定义来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpServerSource {
    /// Server declared by the user's persisted MCP configuration.
    /// 由用户持久化 MCP 配置声明的服务器。
    UserConfigured,
    /// Server contributed by one immutable plugin generation.
    /// 由某个不可变插件代次贡献的服务器。
    Plugin {
        /// Stable owning plugin identifier.
        /// 稳定的所属插件标识。
        plugin_id: String,
        /// Owning plugin generation.
        /// 所属插件代次。
        generation: u64,
    },
}

impl fmt::Debug for McpServerOAuthConfig {
    /// Formats only OAuth structure metadata and never credential-bearing values.
    /// 仅格式化 OAuth 结构元数据，绝不输出可能承载凭据的值。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the redacted structural representation.
    /// `formatter` 接收脱敏后的结构表示。
    ///
    /// # Returns
    /// 返回值
    /// Returns the formatter result after all sensitive fields have been reduced to presence flags.
    /// 在所有敏感字段均被收敛为存在性标记后返回格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpServerOAuthConfig")
            .field("has_client_id", &self.client_id.is_some())
            .field("has_client_secret", &self.client_secret.is_some())
            .field("scope_count", &self.scopes.len())
            .field("has_redirect_uri", &self.redirect_uri.is_some())
            .finish()
    }
}

impl McpServerSource {
    /// Returns the stable runtime partition key for this source.
    /// 返回当前来源的稳定运行时分区键。
    pub fn partition_key(&self) -> String {
        match self {
            Self::UserConfigured => String::from("user"),
            Self::Plugin {
                plugin_id,
                generation,
            } => format!("plugin:{plugin_id}:{generation}"),
        }
    }
}

/// Runtime-facing MCP server transport configuration.
/// 面向运行时的 MCP 服务器传输配置。
#[derive(Clone)]
pub enum McpServerTransportConfig {
    /// Local child process connected through standard input and output.
    /// 通过标准输入输出连接的本地子进程。
    Stdio {
        /// Exact executable path or persisted user command.
        /// 精确可执行文件路径或用户持久化命令。
        command: String,
        /// Ordered process arguments.
        /// 有序进程参数。
        args: Vec<String>,
        /// Optional working directory.
        /// 可选工作目录。
        cwd: Option<String>,
        /// Explicit child environment values.
        /// 显式子进程环境变量值。
        env: BTreeMap<String, String>,
        /// Host environment names passed through when present.
        /// 存在时从宿主透传的环境变量名称。
        env_passthrough: Vec<String>,
        /// Whether undeclared host environment variables remain visible to the child process.
        /// 未声明的宿主环境变量是否继续对该子进程可见。
        inherit_environment: bool,
    },
    /// Streamable HTTP connection.
    /// Streamable HTTP 连接。
    Http {
        /// Exact remote endpoint URL.
        /// 精确远端端点 URL。
        url: String,
        /// Optional bearer token.
        /// 可选 Bearer Token。
        bearer_token: Option<String>,
        /// Explicit request headers.
        /// 显式请求 Header。
        headers: BTreeMap<String, String>,
        /// Header values sourced from named host environment variables.
        /// 从具名宿主环境变量获取值的 Header。
        headers_from_env: BTreeMap<String, String>,
        /// Optional host-driven OAuth settings.
        /// 可选宿主驱动 OAuth 设置。
        oauth: Option<McpServerOAuthConfig>,
    },
    /// Legacy Server-Sent Events connection.
    /// 旧式 Server-Sent Events 连接。
    Sse {
        /// Exact remote endpoint URL.
        /// 精确远端端点 URL。
        url: String,
        /// Optional bearer token.
        /// 可选 Bearer Token。
        bearer_token: Option<String>,
        /// Explicit request headers.
        /// 显式请求 Header。
        headers: BTreeMap<String, String>,
        /// Header values sourced from named host environment variables.
        /// 从具名宿主环境变量获取值的 Header。
        headers_from_env: BTreeMap<String, String>,
        /// Optional host-driven OAuth settings.
        /// 可选宿主驱动 OAuth 设置。
        oauth: Option<McpServerOAuthConfig>,
    },
}

impl fmt::Debug for McpServerTransportConfig {
    /// Formats transport topology while redacting arguments, environment values, headers, and OAuth values.
    /// 格式化传输拓扑，同时脱敏参数、环境变量值、Header 与 OAuth 值。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives only safe transport metadata and key names.
    /// `formatter` 仅接收安全的传输元数据与键名。
    ///
    /// # Returns
    /// 返回值
    /// Returns the formatter result without serializing any dynamic value source result.
    /// 返回不序列化任何动态值来源结果的格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio {
                command,
                args,
                cwd,
                env,
                ..
            } => formatter
                .debug_struct("Stdio")
                .field("command", &redacted_stdio_program(command))
                .field("argument_count", &args.len())
                .field("has_cwd", &cwd.is_some())
                .field("environment_keys", &env.keys().collect::<Vec<_>>())
                .finish(),
            Self::Http {
                url,
                bearer_token,
                headers,
                headers_from_env,
                oauth,
            } => formatter
                .debug_struct("Http")
                .field("endpoint", &redacted_remote_endpoint(url))
                .field("has_bearer_token", &bearer_token.is_some())
                .field("header_names", &headers.keys().collect::<Vec<_>>())
                .field(
                    "environment_header_names",
                    &headers_from_env.keys().collect::<Vec<_>>(),
                )
                .field("oauth", oauth)
                .finish(),
            Self::Sse {
                url,
                bearer_token,
                headers,
                headers_from_env,
                oauth,
            } => formatter
                .debug_struct("Sse")
                .field("endpoint", &redacted_remote_endpoint(url))
                .field("has_bearer_token", &bearer_token.is_some())
                .field("header_names", &headers.keys().collect::<Vec<_>>())
                .field(
                    "environment_header_names",
                    &headers_from_env.keys().collect::<Vec<_>>(),
                )
                .field("oauth", oauth)
                .finish(),
        }
    }
}

/// Runtime-facing MCP server configuration entry.
/// 面向运行时的 MCP 服务器配置条目。
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Host-owned origin used for partitioning and lifecycle isolation.
    /// 宿主持有且用于分区和生命周期隔离的来源。
    pub source: McpServerSource,
    /// Stable server identifier.
    /// 稳定服务器标识。
    pub id: String,
    /// Human-readable server name.
    /// 人类可读的服务器名称。
    pub name: String,
    /// Whether the server is enabled.
    /// 当前服务器是否启用。
    pub enabled: bool,
    /// Strict transport-specific runtime settings.
    /// 严格区分的传输专属运行时设置。
    pub transport: McpServerTransportConfig,
}

impl McpServerConfig {
    /// Returns the transport family represented by the strict transport settings.
    /// 返回严格传输设置所表示的传输类型。
    pub const fn transport_kind(&self) -> McpTransportKind {
        match &self.transport {
            McpServerTransportConfig::Stdio { .. } => McpTransportKind::Stdio,
            McpServerTransportConfig::Http { .. } => McpTransportKind::Http,
            McpServerTransportConfig::Sse { .. } => McpTransportKind::Sse,
        }
    }

    /// Returns the remote OAuth settings when the selected transport supports OAuth.
    /// 当所选传输支持 OAuth 时返回远端 OAuth 设置。
    pub fn oauth(&self) -> Option<&McpServerOAuthConfig> {
        match &self.transport {
            McpServerTransportConfig::Http { oauth, .. }
            | McpServerTransportConfig::Sse { oauth, .. } => oauth.as_ref(),
            McpServerTransportConfig::Stdio { .. } => None,
        }
    }

    /// Returns the remote endpoint URL when the selected transport is remote.
    /// 当所选传输为远端传输时返回端点 URL。
    pub fn remote_url(&self) -> Option<&str> {
        match &self.transport {
            McpServerTransportConfig::Http { url, .. }
            | McpServerTransportConfig::Sse { url, .. } => Some(url),
            McpServerTransportConfig::Stdio { .. } => None,
        }
    }

    /// Returns a compact redacted endpoint summary that never includes arguments, query strings, or credentials.
    /// 返回不包含参数、查询字符串或凭据的紧凑脱敏端点摘要。
    pub fn command_summary(&self) -> String {
        match &self.transport {
            McpServerTransportConfig::Stdio { command, args, .. } => {
                let program = redacted_stdio_program(command);
                if args.is_empty() {
                    program
                } else {
                    format!("{program} <{} arguments>", args.len())
                }
            }
            McpServerTransportConfig::Http { url, .. }
            | McpServerTransportConfig::Sse { url, .. } => redacted_remote_endpoint(url),
        }
    }

    /// Redacts every exact dynamic transport value from one diagnostic before it enters runtime state or service payloads.
    /// 在单条诊断进入运行时状态或服务载荷前，脱敏其中每个精确动态传输值。
    ///
    /// # Parameters
    /// 参数
    /// `diagnostic` is transport or protocol text that may contain resolved configuration values.
    /// `diagnostic` 是可能包含已解析配置值的传输或协议文本。
    ///
    /// # Returns
    /// 返回值
    /// Returns owned text with credential-bearing values replaced and remote URLs reduced to their public origin.
    /// 返回将含凭据值替换、并把远程 URL 收敛到公开源站后的自有文本。
    pub(crate) fn sanitize_diagnostic(&self, diagnostic: &str) -> String {
        let (mut sanitized, sensitive_values) =
            match &self.transport {
                McpServerTransportConfig::Stdio {
                    args,
                    env,
                    env_passthrough,
                    ..
                } => {
                    let values = args
                        .iter()
                        .chain(env.values())
                        .cloned()
                        .chain(
                            env_passthrough
                                .iter()
                                .filter_map(|name| std::env::var(name).ok()),
                        )
                        .collect::<Vec<_>>();
                    (diagnostic.to_string(), values)
                }
                McpServerTransportConfig::Http {
                    url,
                    bearer_token,
                    headers,
                    headers_from_env,
                    oauth,
                }
                | McpServerTransportConfig::Sse {
                    url,
                    bearer_token,
                    headers,
                    headers_from_env,
                    oauth,
                } => {
                    let mut text = diagnostic.replace(url, &redacted_remote_endpoint(url));
                    let mut values = bearer_token.iter().cloned().collect::<Vec<_>>();
                    values.extend(headers.values().cloned());
                    values.extend(headers_from_env.values().filter_map(|name| {
                        super::environment_overlay::lookup_mcp_environment(name)
                    }));
                    if let Some(oauth) = oauth {
                        values.extend(oauth.client_id.iter().cloned());
                        values.extend(oauth.client_secret.iter().cloned());
                        values.extend(oauth.redirect_uri.iter().cloned());
                    }
                    (std::mem::take(&mut text), values)
                }
            };
        // sensitive_values is longest-first so overlapping credentials cannot expose an unmatched suffix.
        // sensitive_values 按长度降序排列，防止重叠凭据暴露未匹配后缀。
        let mut sensitive_values = sensitive_values
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        sensitive_values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        sensitive_values.dedup();
        for value in sensitive_values {
            sanitized = sanitized.replace(&value, "[REDACTED]");
        }
        sanitized
    }

    /// Resolves the effective child-process environment by merging explicit variables with optional host passthrough entries.
    /// A server that inherits the host environment also receives the global Vulcan variables;
    /// explicit and passthrough entries win.
    /// 通过合并显式变量与可选宿主透传项解析最终子进程环境变量集合。继承宿主环境的服务还会收到全局
    /// Vulcan 变量；显式项与透传项优先。
    fn resolved_env(&self) -> BTreeMap<String, String> {
        match &self.transport {
            McpServerTransportConfig::Stdio {
                env,
                env_passthrough,
                inherit_environment,
                ..
            } => {
                let mut resolved =
                    Self::resolved_env_with_lookup(env, env_passthrough, |env_name| {
                        super::environment_overlay::lookup_mcp_environment(env_name)
                    });
                if *inherit_environment {
                    for (name, value) in super::environment_overlay::mcp_environment_overlay() {
                        let taken = resolved
                            .keys()
                            .any(|existing| existing.eq_ignore_ascii_case(&name));
                        if !taken {
                            resolved.insert(name, value);
                        }
                    }
                }
                resolved
            }
            McpServerTransportConfig::Http { .. } | McpServerTransportConfig::Sse { .. } => {
                BTreeMap::new()
            }
        }
    }

    /// Resolves the effective request headers by merging explicit values with optional host environment lookups.
    /// 通过合并显式值与可选宿主环境变量查找解析最终请求头集合。
    fn resolved_headers(&self) -> BTreeMap<String, String> {
        match &self.transport {
            McpServerTransportConfig::Http {
                headers,
                headers_from_env,
                ..
            }
            | McpServerTransportConfig::Sse {
                headers,
                headers_from_env,
                ..
            } => Self::resolved_headers_with_lookup(headers, headers_from_env, |env_name| {
                super::environment_overlay::lookup_mcp_environment(env_name)
            }),
            McpServerTransportConfig::Stdio { .. } => BTreeMap::new(),
        }
    }

    /// Resolves the effective child-process environment using one injectable host-environment lookup.
    /// 使用可注入的宿主环境查找函数解析最终子进程环境变量集合。
    fn resolved_env_with_lookup<F>(
        explicit_env: &BTreeMap<String, String>,
        passthrough_names: &[String],
        mut lookup_env: F,
    ) -> BTreeMap<String, String>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut resolved_env = explicit_env.clone();
        for env_name in passthrough_names
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            if resolved_env.contains_key(env_name) {
                continue;
            }
            if let Some(env_value) = lookup_env(env_name) {
                resolved_env.insert(env_name.to_string(), env_value);
            }
        }
        resolved_env
    }

    /// Resolves the effective request headers using one injectable host-environment lookup.
    /// 使用可注入的宿主环境查找函数解析最终请求头集合。
    fn resolved_headers_with_lookup<F>(
        explicit_headers: &BTreeMap<String, String>,
        headers_from_env: &BTreeMap<String, String>,
        mut lookup_env: F,
    ) -> BTreeMap<String, String>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut resolved_headers = explicit_headers.clone();
        for (header_name, env_name) in headers_from_env {
            let trimmed_header_name = header_name.trim();
            let trimmed_env_name = env_name.trim();
            if trimmed_header_name.is_empty() || trimmed_env_name.is_empty() {
                continue;
            }
            // HTTP header names are case-insensitive, so an explicit header wins in any spelling.
            // HTTP 请求头名称不区分大小写，因此显式请求头无论何种写法都优先。
            if resolved_headers
                .keys()
                .any(|existing| existing.trim().eq_ignore_ascii_case(trimmed_header_name))
            {
                continue;
            }
            if let Some(env_value) = lookup_env(trimmed_env_name) {
                resolved_headers.insert(trimmed_header_name.to_string(), env_value);
            }
        }
        resolved_headers
    }

    /// Converts the runtime-facing server configuration into a host-owned transport launch specification.
    /// 将面向运行时的服务器配置转换为宿主自有传输启动规格。
    pub fn launch_spec(&self) -> Result<McpTransportLaunchSpec, String> {
        let endpoint = self.command_summary();
        match &self.transport {
            McpServerTransportConfig::Stdio {
                command,
                args,
                cwd,
                inherit_environment,
                ..
            } => Ok(McpTransportLaunchSpec::Stdio {
                endpoint,
                program: command.clone(),
                args: args.clone(),
                cwd: cwd.clone().map(PathBuf::from),
                env: self.resolved_env(),
                inherit_environment: *inherit_environment,
            }),
            McpServerTransportConfig::Http {
                url,
                bearer_token,
                oauth,
                ..
            } => Ok(McpTransportLaunchSpec::Http {
                endpoint,
                uri: url.clone(),
                bearer_token: bearer_token.clone(),
                headers: self.resolved_headers(),
                oauth: oauth.clone().map(Into::into),
            }),
            McpServerTransportConfig::Sse {
                url,
                bearer_token,
                oauth,
                ..
            } => Ok(McpTransportLaunchSpec::Sse {
                endpoint,
                uri: url.clone(),
                bearer_token: bearer_token.clone(),
                headers: self.resolved_headers(),
                oauth: oauth.clone().map(Into::into),
            }),
        }
    }

    /// Converts one low-level launch error into transport diagnostics.
    /// 将底层启动错误转换为传输诊断文本。
    /// The text becomes the server's last error, which tool failures can return to the model, so it is English.
    /// 该文本成为服务器最近错误，工具失败可能将其返回给模型，因此使用英文。
    pub fn launch_error_text(&self, error: impl std::fmt::Display) -> String {
        match self.transport_kind() {
            McpTransportKind::Stdio => {
                format!("failed to start MCP server `{}`: {}", self.name, error)
            }
            McpTransportKind::Http => {
                format!(
                    "failed to connect to MCP HTTP server `{}`: {}",
                    self.name, error
                )
            }
            McpTransportKind::Sse => {
                format!(
                    "failed to connect to MCP SSE server `{}`: {}",
                    self.name, error
                )
            }
        }
    }
}

/// Converts the persisted MCP server map into runtime-facing server entries.
/// 将持久化 MCP 服务器映射转换为面向运行时的服务器条目集合。
pub fn runtime_mcp_server_configs(
    config: &ProviderConfigurationFile,
) -> Result<Vec<McpServerConfig>, String> {
    config
        .mcp
        .servers
        .iter()
        .map(|(id, entry)| runtime_mcp_server_from_persisted(id, entry))
        .collect()
}

/// Reduces one stdio executable path to its non-sensitive final program name.
/// 将单个 stdio 可执行路径收敛为不敏感的最终程序名。
///
/// # Parameters
/// 参数
/// `command` is the exact executable path used by the runtime.
/// `command` 是运行时使用的精确可执行路径。
///
/// # Returns
/// 返回值
/// Returns the final path component or a fixed redacted label when it cannot be represented safely.
/// 返回最终路径段；无法安全表示时返回固定脱敏标签。
pub(crate) fn redacted_stdio_program(command: &str) -> String {
    PathBuf::from(command)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| String::from("<stdio program>"))
}

/// Reduces one remote URL to its public origin without user info, path, query, or fragment.
/// 将单个远程 URL 收敛为不含用户信息、路径、查询或片段的公开源站。
///
/// # Parameters
/// 参数
/// `url` is the exact resolved endpoint used by the transport.
/// `url` 是传输使用的精确已解析端点。
///
/// # Returns
/// 返回值
/// Returns a safe origin summary or a fixed invalid-endpoint label.
/// 返回安全源站摘要；解析失败时返回固定无效端点标签。
pub(crate) fn redacted_remote_endpoint(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .map(|parsed| parsed.origin().ascii_serialization())
        .filter(|origin| origin != "null")
        .unwrap_or_else(|| String::from("<invalid remote endpoint>"))
}

/// Converts one persisted MCP server entry into its runtime counterpart.
/// 将单条持久化 MCP 服务器条目转换为对应的运行时表示。
fn runtime_mcp_server_from_persisted(
    id: &str,
    entry: &PersistedMcpServerEntry,
) -> Result<McpServerConfig, String> {
    if id.contains(':') {
        return Err(format!(
            "User-configured MCP server id `{id}` uses the reserved plugin namespace separator `:`."
        ));
    }
    let transport = match entry.transport {
        PersistedMcpTransportKind::Stdio => McpServerTransportConfig::Stdio {
            command: entry.command.clone(),
            args: entry.args.clone(),
            cwd: entry.cwd.clone(),
            env: entry.env.clone(),
            env_passthrough: entry.env_passthrough.clone(),
            inherit_environment: true,
        },
        PersistedMcpTransportKind::Http => McpServerTransportConfig::Http {
            url: entry
                .url
                .clone()
                .ok_or_else(|| format!("MCP HTTP server `{id}` requires an explicit URL."))?,
            bearer_token: entry.bearer_token.clone(),
            headers: entry.headers.clone(),
            headers_from_env: entry.headers_from_env.clone(),
            oauth: entry.oauth.clone().map(Into::into),
        },
        PersistedMcpTransportKind::Sse => McpServerTransportConfig::Sse {
            url: entry
                .url
                .clone()
                .ok_or_else(|| format!("MCP SSE server `{id}` requires an explicit URL."))?,
            bearer_token: entry.bearer_token.clone(),
            headers: entry.headers.clone(),
            headers_from_env: entry.headers_from_env.clone(),
            oauth: entry.oauth.clone().map(Into::into),
        },
    };
    Ok(McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: id.to_string(),
        name: entry.name.clone(),
        enabled: entry.enabled,
        transport,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        McpServerConfig, McpServerOAuthConfig, McpServerSource, McpServerTransportConfig,
        McpTransportKind, runtime_mcp_server_configs,
    };
    use crate::provider_config::{
        PersistedMcpConfiguration, PersistedMcpServerEntry, PersistedMcpTransportKind,
        ProviderConfigurationFile,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use vulcan_mcp_runtime::{McpTransportLaunchSpec, McpTransportOAuthSpec};

    fn sample_server(transport: McpTransportKind) -> McpServerConfig {
        let oauth = Some(McpServerOAuthConfig {
            client_id: Some(String::from("demo-client")),
            client_secret: Some(String::from("demo-secret")),
            scopes: vec![String::from("read"), String::from("offline_access")],
            redirect_uri: Some(String::from("http://localhost:8787/callback")),
        });
        let remote = || {
            (
                String::from("https://example.test/mcp"),
                Some(String::from("token-123")),
                BTreeMap::from([(String::from("X-Test"), String::from("1"))]),
                BTreeMap::new(),
                oauth.clone(),
            )
        };
        let transport = match transport {
            McpTransportKind::Stdio => McpServerTransportConfig::Stdio {
                command: String::from("sample-mcp"),
                args: vec![String::from("--stdio"), String::from("--verbose")],
                cwd: Some(String::from(r"D:\mock\mcp")),
                env: BTreeMap::from([(String::from("VULCAN_MODE"), String::from("stdio"))]),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
            McpTransportKind::Http => {
                let (url, bearer_token, headers, headers_from_env, oauth) = remote();
                McpServerTransportConfig::Http {
                    url,
                    bearer_token,
                    headers,
                    headers_from_env,
                    oauth,
                }
            }
            McpTransportKind::Sse => {
                let (url, bearer_token, headers, headers_from_env, oauth) = remote();
                McpServerTransportConfig::Sse {
                    url,
                    bearer_token,
                    headers,
                    headers_from_env,
                    oauth,
                }
            }
        };
        McpServerConfig {
            source: McpServerSource::UserConfigured,
            id: String::from("server-1"),
            name: String::from("Sample Server"),
            enabled: true,
            transport,
        }
    }

    #[test]
    fn stdio_launch_spec_preserves_cwd_and_env() {
        let server = sample_server(McpTransportKind::Stdio);

        assert_eq!(
            server.launch_spec().expect("stdio server should build"),
            McpTransportLaunchSpec::Stdio {
                endpoint: String::from("sample-mcp <2 arguments>"),
                program: String::from("sample-mcp"),
                args: vec![String::from("--stdio"), String::from("--verbose")],
                cwd: Some(PathBuf::from(r"D:\mock\mcp")),
                env: BTreeMap::from([(String::from("VULCAN_MODE"), String::from("stdio"))]),
                inherit_environment: true,
            }
        );
    }

    #[test]
    fn persisted_http_mapping_requires_remote_url() {
        let config = ProviderConfigurationFile {
            mcp: PersistedMcpConfiguration {
                servers: BTreeMap::from([(
                    String::from("missing-url"),
                    PersistedMcpServerEntry {
                        name: String::from("Missing URL"),
                        enabled: true,
                        transport: PersistedMcpTransportKind::Http,
                        command: String::new(),
                        args: Vec::new(),
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

        let error = runtime_mcp_server_configs(&config)
            .expect_err("HTTP mapping must reject a missing URL");

        assert_eq!(
            error,
            String::from("MCP HTTP server `missing-url` requires an explicit URL.")
        );
    }

    #[test]
    fn http_launch_spec_preserves_oauth_configuration() {
        let server = sample_server(McpTransportKind::Http);

        assert_eq!(
            server.launch_spec().expect("http server should build"),
            McpTransportLaunchSpec::Http {
                endpoint: String::from("https://example.test"),
                uri: String::from("https://example.test/mcp"),
                bearer_token: Some(String::from("token-123")),
                headers: BTreeMap::from([(String::from("X-Test"), String::from("1"))]),
                oauth: Some(McpTransportOAuthSpec {
                    client_id: Some(String::from("demo-client")),
                    client_secret: Some(String::from("demo-secret")),
                    scopes: vec![String::from("read"), String::from("offline_access")],
                    redirect_uri: Some(String::from("http://localhost:8787/callback")),
                }),
            }
        );
    }

    #[test]
    fn resolved_env_and_headers_merge_optional_host_passthrough_without_overwriting_explicit_values()
     {
        let resolved_env = McpServerConfig::resolved_env_with_lookup(
            &BTreeMap::from([(String::from("STATIC_ENV"), String::from("static"))]),
            &[
                String::from("HOST_TOKEN"),
                String::from("STATIC_ENV"),
                String::from("MISSING_TOKEN"),
            ],
            |env_name| match env_name {
                "HOST_TOKEN" => Some(String::from("host-value")),
                "STATIC_ENV" => Some(String::from("should-not-override")),
                _ => None,
            },
        );
        assert_eq!(
            resolved_env,
            BTreeMap::from([
                (String::from("HOST_TOKEN"), String::from("host-value")),
                (String::from("STATIC_ENV"), String::from("static")),
            ])
        );

        let resolved_headers = McpServerConfig::resolved_headers_with_lookup(
            &BTreeMap::from([(String::from("X-Static"), String::from("1"))]),
            &BTreeMap::from([
                (String::from("Authorization"), String::from("ACCESS_TOKEN")),
                (
                    String::from("X-Static"),
                    String::from("STATIC_HEADER_TOKEN"),
                ),
                (String::from("X-Missing"), String::from("MISSING_TOKEN")),
            ]),
            |env_name| match env_name {
                "ACCESS_TOKEN" => Some(String::from("Bearer host")),
                "STATIC_HEADER_TOKEN" => Some(String::from("should-not-override")),
                _ => None,
            },
        );
        assert_eq!(
            resolved_headers,
            BTreeMap::from([
                (String::from("Authorization"), String::from("Bearer host")),
                (String::from("X-Static"), String::from("1")),
            ])
        );
    }

    /// Later lookup values cannot alter a previously resolved launch projection.
    /// 后续查找值不能改变先前已经解析的启动投影。
    /// Both lookups are injected fixtures; this test does not mutate the Rust host environment.
    /// 两次查找均为注入夹具；此测试不修改 Rust 宿主环境。
    #[test]
    fn launch_capture_resolves_passthrough_once_per_original_projection() {
        // The original resolved map is the exact explicit input subsequently passed to native launch.
        // 原解析映射是随后传入原生启动的精确显式输入。
        let original = McpServerConfig::resolved_env_with_lookup(
            &BTreeMap::new(),
            &[String::from("PASSTHROUGH_SECRET")],
            |_| Some(String::from("original-host-lookup")),
        );
        let later = McpServerConfig::resolved_env_with_lookup(
            &BTreeMap::new(),
            &[String::from("PASSTHROUGH_SECRET")],
            |_| Some(String::from("later-host-lookup")),
        );
        assert_eq!(original["PASSTHROUGH_SECRET"], "original-host-lookup");
        assert_eq!(later["PASSTHROUGH_SECRET"], "later-host-lookup");
        let mut server = sample_server(McpTransportKind::Stdio);
        if let McpServerTransportConfig::Stdio {
            env,
            env_passthrough,
            ..
        } = &mut server.transport
        {
            *env = original;
            env_passthrough.clear();
        }
        let passed = server.launch_spec().expect("original explicit projection");
        if let McpServerTransportConfig::Stdio { env, .. } = &mut server.transport {
            *env = later;
        }
        assert!(matches!(passed, McpTransportLaunchSpec::Stdio { env, .. }
            if env["PASSTHROUGH_SECRET"] == "original-host-lookup"));
        assert!(
            matches!(server.launch_spec().expect("separate later projection"),
            McpTransportLaunchSpec::Stdio { env, .. }
            if env["PASSTHROUGH_SECRET"] == "later-host-lookup")
        );
    }

    /// Verifies every debug and endpoint-summary surface excludes resolved credential values and sensitive URL components.
    /// 验证每个调试与端点摘要能力面均排除已解析凭据值及敏感 URL 组成部分。
    #[test]
    fn mcp_configuration_diagnostics_are_structurally_redacted() {
        // remote contains credentials in every field class that must stay outside diagnostics.
        // remote 在每类必须与诊断隔离的字段中都包含凭据。
        let remote = McpServerConfig {
            source: McpServerSource::UserConfigured,
            id: String::from("redacted"),
            name: String::from("Redacted Server"),
            enabled: true,
            transport: McpServerTransportConfig::Http {
                url: String::from("https://user:password@example.test/private?token=query#secret"),
                bearer_token: Some(String::from("bearer-secret")),
                headers: BTreeMap::from([(
                    String::from("Authorization"),
                    String::from("header-secret"),
                )]),
                headers_from_env: BTreeMap::new(),
                oauth: Some(McpServerOAuthConfig {
                    client_id: Some(String::from("client-secret-id")),
                    client_secret: Some(String::from("oauth-secret")),
                    scopes: vec![String::from("private-scope")],
                    redirect_uri: Some(String::from("https://callback.test/private")),
                }),
            },
        };
        let debug_text = format!("{remote:?}");
        for sensitive in [
            "password",
            "private",
            "query",
            "secret#",
            "bearer-secret",
            "header-secret",
            "client-secret-id",
            "oauth-secret",
            "private-scope",
            "callback.test",
        ] {
            assert!(!debug_text.contains(sensitive));
        }
        assert_eq!(remote.command_summary(), "https://example.test");
        let sanitized_error = remote.sanitize_diagnostic(
            "failed https://user:password@example.test/private?token=query#secret bearer-secret header-secret client-secret-id oauth-secret",
        );
        assert!(sanitized_error.contains("https://example.test"));
        for sensitive in [
            "password",
            "private",
            "query",
            "bearer-secret",
            "header-secret",
            "client-secret-id",
            "oauth-secret",
        ] {
            assert!(!sanitized_error.contains(sensitive));
        }

        // stdio proves arguments, working-directory parents, and environment values are also absent.
        // stdio 证明参数、工作目录父路径与环境变量值同样不会出现。
        // root is a private absolute directory in the host's own path form.
        // root 是宿主自身路径形式的私有绝对目录。
        let root = if cfg!(windows) {
            r"C:\private"
        } else {
            "/private"
        };
        let separator = std::path::MAIN_SEPARATOR;
        let stdio = McpServerConfig {
            source: McpServerSource::UserConfigured,
            id: String::from("stdio-redacted"),
            name: String::from("Stdio Redacted"),
            enabled: true,
            transport: McpServerTransportConfig::Stdio {
                command: format!("{root}{separator}bin{separator}server.exe"),
                args: vec![String::from("secret")],
                cwd: Some(format!("{root}{separator}workspace")),
                env: BTreeMap::from([(
                    String::from("ACCESS_TOKEN"),
                    String::from("environment-secret"),
                )]),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let stdio_debug = format!("{stdio:?}");
        assert!(!stdio_debug.contains("secret"));
        assert!(!stdio_debug.contains("environment-secret"));
        assert!(!stdio_debug.contains("workspace"));
        assert_eq!(stdio.command_summary(), "server.exe <1 arguments>");
        let sanitized_stdio = stdio.sanitize_diagnostic("failed environment-secret secret");
        assert_eq!(sanitized_stdio, "failed [REDACTED] [REDACTED]");
    }
}

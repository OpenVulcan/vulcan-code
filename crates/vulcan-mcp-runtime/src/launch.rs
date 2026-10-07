//! Host-owned MCP transport launch specifications and factory helpers.
//! 宿主自有 MCP 传输启动规格与工厂辅助模块。

use crate::auth::{McpOAuthSession, McpTransportOAuthSpec};
use crate::{HttpTransport, McpRuntimeError, McpTransport, SseTransport, StdioTransport};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod resource_tests;

/// Stable host-owned MCP transport launch specification.
/// 宿主自有 MCP 传输启动规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransportLaunchSpec {
    /// Child-process stdio launch specification.
    /// 子进程 stdio 启动规格。
    Stdio {
        /// Stable endpoint label surfaced to the host runtime.
        /// 暴露给宿主运行时的稳定端点标签。
        endpoint: String,
        /// Child process program path or command name.
        /// 子进程程序路径或命令名。
        program: String,
        /// Child process argument vector.
        /// 子进程参数向量。
        args: Vec<String>,
        /// Optional child process working directory.
        /// 子进程可选工作目录。
        cwd: Option<PathBuf>,
        /// Additional child process environment overrides.
        /// 附加子进程环境变量覆盖。
        env: BTreeMap<String, String>,
        /// Whether the child inherits host variables not present in the explicit environment map.
        /// 子进程是否继承显式环境映射之外的宿主变量。
        inherit_environment: bool,
    },
    /// Streamable HTTP launch specification.
    /// Streamable HTTP 启动规格。
    Http {
        /// Stable endpoint label surfaced to the host runtime.
        /// 暴露给宿主运行时的稳定端点标签。
        endpoint: String,
        /// Remote streamable HTTP endpoint URI.
        /// 远端 streamable HTTP 端点 URI。
        uri: String,
        /// Optional bearer token attached to outgoing requests.
        /// 附加到外发请求中的可选 Bearer Token。
        bearer_token: Option<String>,
        /// Additional custom HTTP headers.
        /// 附加自定义 HTTP 请求头。
        headers: BTreeMap<String, String>,
        /// Optional OAuth configuration used by host-driven remote authorization flows.
        /// 宿主驱动远端授权流程使用的可选 OAuth 配置。
        oauth: Option<McpTransportOAuthSpec>,
    },
    /// Legacy SSE launch specification.
    /// 旧式 SSE 启动规格。
    Sse {
        /// Stable endpoint label surfaced to the host runtime.
        /// 暴露给宿主运行时的稳定端点标签。
        endpoint: String,
        /// Remote legacy SSE endpoint URI.
        /// 远端旧式 SSE 端点 URI。
        uri: String,
        /// Optional bearer token attached to outgoing requests.
        /// 附加到外发请求中的可选 Bearer Token。
        bearer_token: Option<String>,
        /// Additional custom HTTP headers.
        /// 附加自定义 HTTP 请求头。
        headers: BTreeMap<String, String>,
        /// Optional OAuth configuration used by host-driven remote authorization flows.
        /// 宿主驱动远端授权流程使用的可选 OAuth 配置。
        oauth: Option<McpTransportOAuthSpec>,
    },
}

impl McpTransportLaunchSpec {
    /// Launches the declared transport with resources fixed before any initialization work.
    /// 使用任何初始化工作前已固定的资源启动声明传输。
    /// Shared_auth_session optionally reuses OAuth; resources owns the exact frozen host authority.
    /// shared_auth_session 可复用 OAuth；resources 拥有精确冻结宿主权威。
    /// Returns the initialized transport; callers retain a separate original release receipt on errors or cancellation.
    /// 返回已初始化传输；失败或取消时调用方保留独立原释放回执。
    pub async fn launch_with_resources(
        self,
        shared_auth_session: Option<Arc<McpOAuthSession>>,
        resources: crate::McpTransportResources,
    ) -> Result<Arc<dyn McpTransport>, McpRuntimeError> {
        match self {
            Self::Stdio {
                endpoint,
                program,
                args,
                cwd,
                env,
                inherit_environment,
            } => {
                let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                let transport = StdioTransport::spawn_with_resources(
                    endpoint,
                    &program,
                    &arg_refs,
                    cwd.as_deref(),
                    &env,
                    inherit_environment,
                    resources,
                )
                .await?;
                Ok(Arc::new(transport))
            }
            Self::Http {
                endpoint,
                uri,
                bearer_token,
                headers,
                oauth,
            } => {
                let auth_session = resolve_auth_session(&uri, oauth, shared_auth_session)?;
                let transport = HttpTransport::connect_with_resources(
                    endpoint,
                    uri,
                    bearer_token,
                    headers.into_iter().collect::<HashMap<_, _>>(),
                    auth_session,
                    resources,
                )
                .await?;
                Ok(Arc::new(transport))
            }
            Self::Sse {
                endpoint,
                uri,
                bearer_token,
                headers,
                oauth,
            } => {
                let auth_session = resolve_auth_session(&uri, oauth, shared_auth_session)?;
                let transport = SseTransport::connect_with_resources(
                    endpoint,
                    uri,
                    bearer_token,
                    headers.into_iter().collect::<HashMap<_, _>>(),
                    auth_session,
                    resources,
                )
                .await?;
                Ok(Arc::new(transport))
            }
        }
    }

    /// Launch one concrete MCP transport from the host-owned specification.
    /// 根据宿主自有规格启动一个具体 MCP 传输实例。
    pub async fn launch(
        self,
        shared_auth_session: Option<Arc<McpOAuthSession>>,
    ) -> Result<Arc<dyn McpTransport>, McpRuntimeError> {
        // All launch variants use the same resource-aware dispatch with an empty host bundle.
        // 全部启动变体使用相同资源感知分派及空宿主资源包。
        let (resources, _) = crate::McpTransportResources::new(());
        self.launch_with_resources(shared_auth_session, resources)
            .await
    }
}

/// Requires the host-provided OAuth session for OAuth-enabled transports.
/// 要求启用 OAuth 的传输使用宿主提供的 OAuth 会话。
///
/// A per-launch session would hold tokens nobody else can observe or persist, so its absence is an error.
/// 每次启动临时创建的会话持有的令牌无人可观察或持久化，因此缺少会话视为错误。
fn resolve_auth_session(
    uri: &str,
    oauth: Option<McpTransportOAuthSpec>,
    shared_auth_session: Option<Arc<McpOAuthSession>>,
) -> Result<Option<Arc<McpOAuthSession>>, McpRuntimeError> {
    match (oauth, shared_auth_session) {
        (Some(_), None) => Err(McpRuntimeError::invalid_argument(format!(
            "mcp.oauth.session-missing: OAuth-enabled MCP transport `{uri}` was launched without its host OAuth session"
        ))),
        (_, session) => Ok(session),
    }
}

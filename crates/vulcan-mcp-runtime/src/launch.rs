//! Host-owned MCP transport launch specifications and factory helpers.
//! 宿主自有 MCP 传输启动规格与工厂辅助模块。

use crate::auth::{McpOAuthSession, McpTransportOAuthSpec};
use crate::{HttpTransport, McpRuntimeError, McpTransport, SseTransport, StdioTransport};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Launch one concrete MCP transport from the host-owned specification.
    /// 根据宿主自有规格启动一个具体 MCP 传输实例。
    pub async fn launch(
        self,
        shared_auth_session: Option<Arc<McpOAuthSession>>,
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
                let transport = StdioTransport::spawn(
                    endpoint,
                    &program,
                    &arg_refs,
                    cwd.as_deref(),
                    &env,
                    inherit_environment,
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
                let auth_session = resolve_auth_session(&uri, oauth, shared_auth_session).await?;
                let transport = HttpTransport::connect(
                    endpoint,
                    uri,
                    bearer_token,
                    headers.into_iter().collect::<HashMap<_, _>>(),
                    auth_session,
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
                let auth_session = resolve_auth_session(&uri, oauth, shared_auth_session).await?;
                let transport = SseTransport::connect(
                    endpoint,
                    uri,
                    bearer_token,
                    headers.into_iter().collect::<HashMap<_, _>>(),
                    auth_session,
                )
                .await?;
                Ok(Arc::new(transport))
            }
        }
    }
}

/// Resolves the effective shared OAuth session by preferring the caller-provided long-lived session and lazily creating one from launch metadata when necessary.
/// 通过优先使用调用方提供的长期 OAuth 会话，并在需要时根据启动元数据惰性创建新会话，解析生效的共享 OAuth 会话。
async fn resolve_auth_session(
    uri: &str,
    oauth: Option<McpTransportOAuthSpec>,
    shared_auth_session: Option<Arc<McpOAuthSession>>,
) -> Result<Option<Arc<McpOAuthSession>>, McpRuntimeError> {
    if let Some(auth_session) = shared_auth_session {
        return Ok(Some(auth_session));
    }

    match oauth {
        Some(spec) => Ok(Some(Arc::new(McpOAuthSession::new(uri, spec).await?))),
        None => Ok(None),
    }
}

//! Process-wide OAuth credential backend shared by every MCP runtime generation.
//! 被所有 MCP 运行时代次共享的进程级 OAuth 凭据后端。
//!
//! Runtime partitions are rebuilt whenever MCP configuration changes, so tokens must live
//! outside any single handle. The host installs a persistent backend once at startup; without
//! one, credentials stay in process memory.
//! MCP 配置变化时运行时分区会重建，因此令牌必须存放在单个句柄之外。宿主在启动时安装一次
//! 持久化后端；未安装时凭据只保存在进程内存中。

use std::sync::{Arc, OnceLock};
use vulcan_mcp_runtime::{McpOAuthCredentialBackend, McpOAuthMemoryCredentialBackend};

/// Backend selected for this process; fixed on first install or first use.
/// 本进程选定的后端；首次安装或首次使用时固定。
static MCP_OAUTH_CREDENTIAL_BACKEND: OnceLock<Arc<dyn McpOAuthCredentialBackend>> = OnceLock::new();

/// Installs the persistent OAuth credential backend before any MCP OAuth session is created.
/// 在创建任何 MCP OAuth 会话之前安装持久化 OAuth 凭据后端。
///
/// # Returns
/// 返回值
/// An error when a backend was already installed or already chosen by first use.
/// 后端已安装或已因首次使用而选定时返回错误。
pub fn install_mcp_oauth_credential_backend(
    backend: Arc<dyn McpOAuthCredentialBackend>,
) -> Result<(), String> {
    MCP_OAUTH_CREDENTIAL_BACKEND.set(backend).map_err(|_| {
        String::from("the MCP OAuth credential backend was already selected for this process")
    })
}

/// Returns the process backend, choosing the in-memory backend when none was installed.
/// 返回进程后端；未安装时选定内存后端。
pub fn mcp_oauth_credential_backend() -> Arc<dyn McpOAuthCredentialBackend> {
    MCP_OAUTH_CREDENTIAL_BACKEND
        .get_or_init(|| Arc::new(McpOAuthMemoryCredentialBackend::default()))
        .clone()
}

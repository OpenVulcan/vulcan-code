//! Host-owned OAuth lifecycle helpers reused by remote MCP transports.
//! 被远端 MCP 传输复用的宿主自有 OAuth 生命周期辅助模块。

use crate::McpRuntimeError;
use async_trait::async_trait;
use reqwest::Url;
use rmcp::transport::auth::{
    AuthError as RmcpAuthError, AuthorizationManager, CredentialStore, OAuthClientConfig,
    StoredCredentials,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// Stable host-owned OAuth configuration attached to one remote MCP transport.
/// 附加到单个远端 MCP 传输上的稳定宿主自有 OAuth 配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTransportOAuthSpec {
    /// Optional pre-registered OAuth client identifier.
    /// 可选的预注册 OAuth 客户端标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Optional pre-registered OAuth client secret.
    /// 可选的预注册 OAuth 客户端密钥。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Optional OAuth scopes requested during authorization.
    /// 授权过程中请求的可选 OAuth scopes 集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Optional redirect URI used by the OAuth authorization-code flow.
    /// OAuth 授权码流程使用的可选回调 URI。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// Stable host-owned OAuth status surfaced to runtime diagnostics and UI bridges.
/// 暴露给运行时诊断与界面桥接层的稳定宿主自有 OAuth 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAuthStatus {
    /// OAuth is not configured for the current transport.
    /// 当前传输未配置 OAuth。
    NotConfigured,
    /// OAuth is configured but the host currently has no stored credentials.
    /// 已配置 OAuth，但宿主当前没有已存储凭据。
    NotAuthenticated,
    /// OAuth credentials are present and can currently produce one access token.
    /// OAuth 凭据已存在，且当前可以产出访问令牌。
    Authenticated,
    /// OAuth credentials are present but have expired and require re-authorization.
    /// OAuth 凭据已存在，但已经过期并需要重新授权。
    Expired,
}

impl McpAuthStatus {
    /// Returns a compact human-readable label for UI projections.
    /// 返回供界面投影使用的紧凑人类可读标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotConfigured => "未配置",
            Self::NotAuthenticated => "未认证",
            Self::Authenticated => "已认证",
            Self::Expired => "已过期",
        }
    }
}

/// Stable OAuth authorization challenge returned to upper layers when the user must visit one browser URL.
/// 当用户必须访问浏览器 URL 时返回给上层的稳定 OAuth 授权挑战。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuthChallenge {
    /// Authorization URL that the user should open in the browser.
    /// 用户应在浏览器中打开的授权 URL。
    authorization_url: String,
    /// OAuth state value echoed back during the callback completion step.
    /// 在回调完成阶段需要回传的 OAuth state 值。
    oauth_state: String,
}

impl McpAuthChallenge {
    /// Builds one stable OAuth authorization challenge from raw URL and state values.
    /// 根据原始 URL 与 state 值构建单个稳定 OAuth 授权挑战。
    pub fn new(authorization_url: impl Into<String>, oauth_state: impl Into<String>) -> Self {
        Self {
            authorization_url: authorization_url.into(),
            oauth_state: oauth_state.into(),
        }
    }

    /// Returns the authorization URL that the user should open.
    /// 返回用户应打开的授权 URL。
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    /// Returns the OAuth state value expected during callback completion.
    /// 返回回调完成阶段期望的 OAuth state 值。
    pub fn oauth_state(&self) -> &str {
        &self.oauth_state
    }
}

/// Cloneable in-memory credential store that lets the host inspect and clear OAuth credentials.
/// 允许宿主检查并清空 OAuth 凭据的可克隆内存凭据存储。
#[derive(Debug, Clone, Default)]
struct TrackedCredentialStore {
    /// Shared credential snapshot retained entirely in memory.
    /// 完全保存在内存中的共享凭据快照。
    inner: std::sync::Arc<RwLock<Option<StoredCredentials>>>,
}

impl TrackedCredentialStore {
    /// Builds one empty tracked credential store.
    /// 构建一个空的可跟踪凭据存储。
    fn new() -> Self {
        Self::default()
    }

    /// Returns the latest stored credential snapshot, if any.
    /// 返回最新已存储的凭据快照（如果存在）。
    async fn load_snapshot(&self) -> Option<StoredCredentials> {
        self.inner.read().await.clone()
    }

    /// Clears every stored OAuth credential from the in-memory store.
    /// 从内存存储中清空全部 OAuth 凭据。
    async fn clear_snapshot(&self) {
        *self.inner.write().await = None;
    }
}

#[async_trait]
impl CredentialStore for TrackedCredentialStore {
    /// Loads the current tracked OAuth credential snapshot.
    /// 载入当前可跟踪的 OAuth 凭据快照。
    async fn load(&self) -> Result<Option<StoredCredentials>, RmcpAuthError> {
        Ok(self.load_snapshot().await)
    }

    /// Saves one tracked OAuth credential snapshot in memory.
    /// 在内存中保存一份可跟踪的 OAuth 凭据快照。
    async fn save(&self, credentials: StoredCredentials) -> Result<(), RmcpAuthError> {
        *self.inner.write().await = Some(credentials);
        Ok(())
    }

    /// Clears the tracked OAuth credential snapshot from memory.
    /// 从内存中清除可跟踪的 OAuth 凭据快照。
    async fn clear(&self) -> Result<(), RmcpAuthError> {
        self.clear_snapshot().await;
        Ok(())
    }
}

/// Shared host-owned OAuth session that survives transport reconnects inside one runtime process.
/// 在单个运行时进程内跨传输重连保活的共享宿主自有 OAuth 会话。
pub struct McpOAuthSession {
    /// Shared mutable rmcp authorization manager.
    /// 共享可变的 rmcp 授权管理器。
    manager: std::sync::Arc<Mutex<AuthorizationManager>>,
    /// Shared credential store exposed to host-side inspection and clearing flows.
    /// 暴露给宿主侧检查与清理流程的共享凭据存储。
    credential_store: TrackedCredentialStore,
    /// Stable host-owned OAuth configuration.
    /// 稳定的宿主自有 OAuth 配置。
    spec: McpTransportOAuthSpec,
}

impl std::fmt::Debug for McpOAuthSession {
    /// Renders one redacted debug representation that avoids exposing the inner authorization manager state.
    /// 渲染一份脱敏调试表示，避免暴露内部授权管理器状态。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpOAuthSession")
            .field("manager", &"...")
            .field("credential_store", &self.credential_store)
            .field("spec", &self.spec)
            .finish()
    }
}

impl McpOAuthSession {
    /// Builds one host-owned OAuth session from the remote MCP base URL and stable OAuth configuration.
    /// 根据远端 MCP 基础 URL 与稳定 OAuth 配置构建单个宿主自有 OAuth 会话。
    pub async fn new(
        base_url: impl AsRef<str>,
        spec: McpTransportOAuthSpec,
    ) -> Result<Self, McpRuntimeError> {
        let credential_store = TrackedCredentialStore::new();
        let mut manager = AuthorizationManager::new(base_url.as_ref())
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.init: {error}")))?;
        manager.set_credential_store(credential_store.clone());

        Ok(Self {
            manager: std::sync::Arc::new(Mutex::new(manager)),
            credential_store,
            spec,
        })
    }

    /// Returns one authorization challenge for the authorization-code flow, creating or configuring the OAuth client as needed.
    /// 返回授权码流程的单个授权挑战，并在需要时创建或配置 OAuth 客户端。
    pub async fn start_authorization(
        &self,
        client_name: &str,
    ) -> Result<McpAuthChallenge, McpRuntimeError> {
        let redirect_uri = self.spec.redirect_uri.as_deref().ok_or_else(|| {
            McpRuntimeError::invalid_argument("OAuth 授权需要配置 redirectUri。".to_string())
        })?;
        let scope_refs = self
            .spec
            .scopes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut manager = self.manager.lock().await;
        let metadata = manager
            .discover_metadata()
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.discover: {error}")))?;
        manager.set_metadata(metadata);

        if let Some(client_id) = self.spec.client_id.as_deref() {
            let mut client_config =
                OAuthClientConfig::new(client_id.to_string(), redirect_uri.to_string())
                    .with_scopes(self.spec.scopes.clone());
            if let Some(client_secret) = self.spec.client_secret.clone() {
                client_config = client_config.with_client_secret(client_secret);
            }
            manager.configure_client(client_config).map_err(|error| {
                McpRuntimeError::transport(format!("mcp.oauth.configure: {error}"))
            })?;
        } else {
            manager
                .register_client(client_name, redirect_uri, &scope_refs)
                .await
                .map_err(|error| {
                    McpRuntimeError::transport(format!("mcp.oauth.register: {error}"))
                })?;
        }

        let authorization_url = manager
            .get_authorization_url(&scope_refs)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.start: {error}")))?;
        let oauth_state = extract_oauth_state(&authorization_url)?;

        Ok(McpAuthChallenge::new(authorization_url, oauth_state))
    }

    /// Completes one authorization-code callback by exchanging the code for tokens and storing the resulting credentials.
    /// 通过交换授权码并保存所得凭据来完成一次授权码回调。
    pub async fn finish_authorization(
        &self,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, McpRuntimeError> {
        self.manager
            .lock()
            .await
            .exchange_code_for_token(authorization_code, oauth_state)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.finish: {error}")))?;

        Ok(McpAuthStatus::Authenticated)
    }

    /// Clears every stored OAuth credential retained by the host session.
    /// 清空该宿主会话保留的全部 OAuth 凭据。
    pub async fn remove_authorization(&self) {
        self.credential_store.clear_snapshot().await;
    }

    /// Returns whether the host currently has at least one stored OAuth token payload.
    /// 返回宿主当前是否至少持有一份已存储的 OAuth 令牌载荷。
    pub async fn has_stored_tokens(&self) -> bool {
        self.credential_store
            .load_snapshot()
            .await
            .is_some_and(|credentials| credentials.token_response.is_some())
    }

    /// Returns the current OAuth status by combining the stored-credential snapshot with a live access-token check.
    /// 通过组合已存储凭据快照与实时访问令牌检查返回当前 OAuth 状态。
    pub async fn auth_status(&self) -> McpAuthStatus {
        if !self.has_stored_tokens().await {
            return McpAuthStatus::NotAuthenticated;
        }

        match self.access_token().await {
            Ok(Some(_)) => McpAuthStatus::Authenticated,
            Ok(None) => McpAuthStatus::Expired,
            Err(_) => McpAuthStatus::Expired,
        }
    }

    /// Resolves one dynamic OAuth access token when credentials are available, returning `None` when the user still needs to authenticate.
    /// 当凭据可用时解析一个动态 OAuth 访问令牌；若用户仍需认证则返回 `None`。
    pub async fn access_token(&self) -> Result<Option<String>, McpRuntimeError> {
        let manager = self.manager.lock().await;
        match manager.get_access_token().await {
            Ok(token) => Ok(Some(token)),
            Err(RmcpAuthError::AuthorizationRequired) => Ok(None),
            Err(error) => Err(McpRuntimeError::transport(format!(
                "mcp.oauth.access-token: {error}"
            ))),
        }
    }

    /// Returns the configured redirect URI, if any.
    /// 返回已配置的回调 URI（如果存在）。
    pub fn redirect_uri(&self) -> Option<&str> {
        self.spec.redirect_uri.as_deref()
    }
}

/// Extracts the OAuth state query parameter from one authorization URL.
/// 从单个授权 URL 中提取 OAuth state 查询参数。
fn extract_oauth_state(authorization_url: &str) -> Result<String, McpRuntimeError> {
    let parsed_url = Url::parse(authorization_url).map_err(|error| {
        McpRuntimeError::transport(format!("mcp.oauth.authorization-url.parse: {error}"))
    })?;
    parsed_url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.to_string()))
        .ok_or_else(|| {
            McpRuntimeError::transport(
                "mcp.oauth.authorization-url.missing-state: authorization URL 缺少 state 参数。"
                    .to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{McpAuthChallenge, McpAuthStatus, extract_oauth_state};

    #[test]
    fn auth_status_labels_match_expected_chinese_text() {
        assert_eq!(McpAuthStatus::NotConfigured.label(), "未配置");
        assert_eq!(McpAuthStatus::NotAuthenticated.label(), "未认证");
        assert_eq!(McpAuthStatus::Authenticated.label(), "已认证");
        assert_eq!(McpAuthStatus::Expired.label(), "已过期");
    }

    #[test]
    fn auth_challenge_preserves_url_and_state() {
        let challenge = McpAuthChallenge::new(
            "https://auth.example.test/authorize?state=demo-state",
            "demo-state",
        );

        assert_eq!(
            challenge.authorization_url(),
            "https://auth.example.test/authorize?state=demo-state"
        );
        assert_eq!(challenge.oauth_state(), "demo-state");
    }

    #[test]
    fn extract_oauth_state_reads_query_parameter() {
        let state = extract_oauth_state(
            "https://auth.example.test/authorize?client_id=demo&state=oauth-state-123",
        )
        .expect("state should be extracted from the authorization URL");

        assert_eq!(state, String::from("oauth-state-123"));
    }
}

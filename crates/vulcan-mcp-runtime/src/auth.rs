//! Host-owned OAuth lifecycle helpers reused by remote MCP transports.
//! 被远端 MCP 传输复用的宿主自有 OAuth 生命周期辅助模块。

use crate::McpRuntimeError;
use async_trait::async_trait;
use reqwest::Url;
use rmcp::transport::auth::{
    AuthError as RmcpAuthError, AuthorizationCallback, AuthorizationManager, CredentialStore,
    OAuthClientConfig, StoredCredentials,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, PoisonError};
use tokio::sync::{Mutex, watch};

/// Stable host-owned OAuth configuration attached to one remote MCP transport.
/// 附加到单个远端 MCP 传输上的稳定宿主自有 OAuth 配置。
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// Stable machine-readable OAuth status; user-facing labels are derived from the serialized code.
/// 稳定的机器可读 OAuth 状态；面向用户的标签由序列化编码派生。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAuthStatus {
    /// OAuth is not configured for the current server.
    /// 当前服务器未配置 OAuth。
    NotConfigured,
    /// OAuth is configured but the host currently has no stored tokens.
    /// 已配置 OAuth，但宿主当前没有已存储令牌。
    NotAuthenticated,
    /// Stored credentials can currently produce one access token.
    /// 已存储凭据当前可以产出访问令牌。
    Authenticated,
    /// Stored tokens expired and could not be refreshed, so the user must authorize again.
    /// 已存储令牌已过期且无法刷新，用户必须重新授权。
    Expired,
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

/// One persisted OAuth credential record bound to the server URL it was issued for.
/// 绑定到签发时服务器 URL 的单条持久化 OAuth 凭据记录。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthCredentialRecord {
    /// Remote MCP URL the credentials were issued for; a different configured URL treats the record as absent.
    /// 凭据签发时对应的远端 MCP URL；配置 URL 不同时视为记录不存在。
    pub server_url: String,
    /// rmcp credential payload, including the client identifier and token response.
    /// rmcp 凭据载荷，包含客户端标识与令牌响应。
    pub credentials: StoredCredentials,
}

/// Host persistence backend for OAuth credentials, keyed by the stable MCP server identifier.
/// 以稳定 MCP 服务器标识为键的宿主 OAuth 凭据持久化后端。
///
/// Calls are synchronous and may block on the platform secure store; the session runs them on the blocking pool.
/// 调用是同步的，可能阻塞在平台安全存储上；会话在阻塞线程池中执行它们。
pub trait McpOAuthCredentialBackend: Send + Sync + 'static {
    /// Loads the record for `server_id`, returning `None` when nothing is stored.
    /// 读取 `server_id` 的记录；未存储时返回 `None`。
    fn load(&self, server_id: &str) -> Result<Option<McpOAuthCredentialRecord>, String>;

    /// Replaces the record for `server_id`.
    /// 替换 `server_id` 的记录。
    fn save(&self, server_id: &str, record: &McpOAuthCredentialRecord) -> Result<(), String>;

    /// Removes the record for `server_id`; removing an absent record succeeds.
    /// 移除 `server_id` 的记录；移除不存在的记录同样成功。
    fn clear(&self, server_id: &str) -> Result<(), String>;
}

/// Process-local credential backend used when the host installs no persistent store.
/// 宿主未安装持久化存储时使用的进程内凭据后端。
#[derive(Default)]
pub struct McpOAuthMemoryCredentialBackend {
    /// Records keyed by server identifier.
    /// 以服务器标识为键的记录。
    records: std::sync::Mutex<BTreeMap<String, McpOAuthCredentialRecord>>,
}

impl McpOAuthCredentialBackend for McpOAuthMemoryCredentialBackend {
    fn load(&self, server_id: &str) -> Result<Option<McpOAuthCredentialRecord>, String> {
        Ok(self.records().get(server_id).cloned())
    }

    fn save(&self, server_id: &str, record: &McpOAuthCredentialRecord) -> Result<(), String> {
        self.records().insert(server_id.to_string(), record.clone());
        Ok(())
    }

    fn clear(&self, server_id: &str) -> Result<(), String> {
        self.records().remove(server_id);
        Ok(())
    }
}

impl McpOAuthMemoryCredentialBackend {
    /// Locks the record map; a poisoned lock still holds consistent whole-record values.
    /// 锁定记录映射；中毒的锁仍持有一致的整条记录值。
    fn records(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, McpOAuthCredentialRecord>> {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Sign-out epoch of one server in one backend, shared by every runtime generation in the process.
/// 某后端中某服务器的退出登录代次，由进程内所有运行时代次共享。
///
/// Each runtime generation owns its own session and manager lock, so a refresh in an older
/// generation is not ordered against a sign-out in the current one. Clearing bumps the epoch and
/// saving requires the epoch its operation started under, both while holding this lock, so a save
/// that began before a sign-out can never restore the removed credentials.
/// 每个运行时代次拥有自己的会话与管理器锁，因此旧代次中的刷新与当前代次中的退出登录之间没有先后
/// 约束。清除会递增代次，保存要求其操作开始时的代次，二者都在持有此锁时进行，因此退出登录前开始的
/// 保存绝不会还原已移除的凭据。
type CredentialEpoch = Arc<std::sync::Mutex<u64>>;

/// Sign-out epochs keyed by backend instance and server identifier.
/// 按后端实例与服务器标识索引的退出登录代次。
static CREDENTIAL_EPOCHS: LazyLock<std::sync::Mutex<HashMap<(usize, String), CredentialEpoch>>> =
    LazyLock::new(Default::default);

/// Returns the sign-out epoch shared by every session of `server_id` over `backend`.
/// 返回 `backend` 上 `server_id` 的所有会话共享的退出登录代次。
fn credential_epoch(
    backend: &Arc<dyn McpOAuthCredentialBackend>,
    server_id: &str,
) -> CredentialEpoch {
    let instance = Arc::as_ptr(backend).cast::<()>() as usize;
    CREDENTIAL_EPOCHS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((instance, server_id.to_owned()))
        .or_default()
        .clone()
}

/// Clears the stored credentials of `server_id` and invalidates every save that started before,
/// in any runtime generation.
/// 清除 `server_id` 的已存储凭据，并使之前在任意运行时代次中开始的保存全部失效。
pub fn clear_mcp_oauth_credentials(
    backend: &Arc<dyn McpOAuthCredentialBackend>,
    server_id: &str,
) -> Result<(), String> {
    let epoch = credential_epoch(backend, server_id);
    let mut current = epoch.lock().unwrap_or_else(PoisonError::into_inner);
    *current += 1;
    backend.clear(server_id)
}

/// rmcp credential store that scopes one shared backend to a single server URL and client.
/// 将共享后端限定到单个服务器 URL 与客户端的 rmcp 凭据存储。
#[derive(Clone)]
struct ScopedCredentialStore {
    /// Shared host backend.
    /// 共享宿主后端。
    backend: Arc<dyn McpOAuthCredentialBackend>,
    /// Stable server identifier used as the backend key.
    /// 用作后端键的稳定服务器标识。
    server_id: String,
    /// Configured remote URL that stored records must match.
    /// 已存储记录必须匹配的已配置远端 URL。
    server_url: String,
    /// Pre-registered client identifier that stored records must match, when configured.
    /// 已配置时，已存储记录必须匹配的预注册客户端标识。
    expected_client_id: Option<String>,
    /// Sign-out epoch shared with the other generations' sessions of this server.
    /// 与该服务器其他代次会话共享的退出登录代次。
    epoch: CredentialEpoch,
    /// Epoch the current manager operation started under; a save is accepted only while it holds.
    /// 当前管理器操作开始时的代次；仅当其仍有效时才接受保存。
    save_epoch: Arc<AtomicU64>,
}

impl ScopedCredentialStore {
    /// Runs one backend call on the blocking pool so secure-store latency never stalls async workers.
    /// 在阻塞线程池中执行一次后端调用，使安全存储延迟不会阻塞异步工作线程。
    async fn run<T: Send + 'static>(
        &self,
        call: impl FnOnce(&dyn McpOAuthCredentialBackend, &str) -> Result<T, String> + Send + 'static,
    ) -> Result<T, RmcpAuthError> {
        let backend = self.backend.clone();
        let server_id = self.server_id.clone();
        tokio::task::spawn_blocking(move || call(backend.as_ref(), &server_id))
            .await
            .map_err(|error| RmcpAuthError::InternalError(error.to_string()))?
            .map_err(RmcpAuthError::InternalError)
    }

    /// Returns the current sign-out epoch.
    /// 返回当前的退出登录代次。
    fn current_epoch(&self) -> u64 {
        *self.epoch.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets saves of the manager operation requested under `started` succeed until the next sign-out.
    /// 允许在 `started` 代次下请求的管理器操作进行保存，直到下一次退出登录。
    fn begin_operation(&self, started: u64) {
        self.save_epoch.store(started, Ordering::Release);
    }

    /// Returns whether a stored record belongs to the currently configured server and client.
    /// 返回已存储记录是否属于当前配置的服务器与客户端。
    fn matches(&self, record: &McpOAuthCredentialRecord) -> bool {
        record.server_url == self.server_url
            && self
                .expected_client_id
                .as_deref()
                .is_none_or(|client_id| record.credentials.client_id == client_id)
    }
}

#[async_trait]
impl CredentialStore for ScopedCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, RmcpAuthError> {
        let record = self
            .run(|backend, server_id| backend.load(server_id))
            .await?;
        Ok(record
            .filter(|record| self.matches(record))
            .map(|record| record.credentials))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), RmcpAuthError> {
        let record = McpOAuthCredentialRecord {
            server_url: self.server_url.clone(),
            credentials,
        };
        let epoch = self.epoch.clone();
        let started = self.save_epoch.load(Ordering::Acquire);
        let saved = self
            .run(move |backend, server_id| {
                let current = epoch.lock().unwrap_or_else(PoisonError::into_inner);
                if *current != started {
                    return Ok(false);
                }
                backend.save(server_id, &record).map(|()| true)
            })
            .await?;
        // A sign-out since the operation began wins; the caller must authorize again.
        // 操作开始后发生的退出登录优先；调用方必须重新授权。
        if saved {
            Ok(())
        } else {
            Err(RmcpAuthError::AuthorizationRequired)
        }
    }

    async fn clear(&self) -> Result<(), RmcpAuthError> {
        let backend = self.backend.clone();
        let server_id = self.server_id.clone();
        tokio::task::spawn_blocking(move || clear_mcp_oauth_credentials(&backend, &server_id))
            .await
            .map_err(|error| RmcpAuthError::InternalError(error.to_string()))?
            .map_err(RmcpAuthError::InternalError)
    }
}

/// Host-owned OAuth session for one configured server, restoring persisted tokens on first use.
/// 单个已配置服务器的宿主自有 OAuth 会话，首次使用时还原持久化令牌。
pub struct McpOAuthSession {
    /// Shared mutable rmcp authorization manager.
    /// 共享可变的 rmcp 授权管理器。
    manager: Arc<Mutex<AuthorizationManager>>,
    /// Backend-scoped credential store shared with the manager.
    /// 与管理器共享的后端限定凭据存储。
    credential_store: ScopedCredentialStore,
    /// Stable host-owned OAuth configuration.
    /// 稳定的宿主自有 OAuth 配置。
    spec: McpTransportOAuthSpec,
    /// Whether the manager holds a configured client for the stored tokens.
    /// 管理器是否已为已存储令牌配置好客户端。
    ///
    /// Stays false while nothing is stored, so tokens another session saves for the same server are picked up later.
    /// 未存储任何内容时保持 false，使其他会话为同一服务器保存的令牌之后仍能被读取。
    restored: AtomicBool,
    /// Generation bumped whenever authorization completes or is removed.
    /// 授权完成或被移除时递增的代次。
    authorization_generation: watch::Sender<u64>,
}

impl std::fmt::Debug for McpOAuthSession {
    /// Renders one redacted debug representation that avoids exposing credentials or manager state.
    /// 渲染一份脱敏调试表示，避免暴露凭据或管理器状态。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpOAuthSession")
            .field("server_id", &self.credential_store.server_id)
            .field("server_url", &self.credential_store.server_url)
            .field("client_id", &self.spec.client_id)
            .field("scopes", &self.spec.scopes)
            .finish_non_exhaustive()
    }
}

impl McpOAuthSession {
    /// Builds one OAuth session for `server_id` at `base_url`; construction performs no network or store access.
    /// 为位于 `base_url` 的 `server_id` 构建 OAuth 会话；构建过程不访问网络或存储。
    pub async fn new(
        server_id: impl Into<String>,
        base_url: impl Into<String>,
        spec: McpTransportOAuthSpec,
        backend: Arc<dyn McpOAuthCredentialBackend>,
    ) -> Result<Self, McpRuntimeError> {
        let server_url = base_url.into();
        let server_id = server_id.into();
        let credential_store = ScopedCredentialStore {
            epoch: credential_epoch(&backend, &server_id),
            backend,
            server_id,
            server_url: server_url.clone(),
            expected_client_id: spec.client_id.clone(),
            save_epoch: Arc::new(AtomicU64::new(0)),
        };
        let mut manager = AuthorizationManager::new(server_url.as_str())
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.init: {error}")))?;
        manager.set_credential_store(credential_store.clone());
        let (authorization_generation, _) = watch::channel(0);

        Ok(Self {
            manager: Arc::new(Mutex::new(manager)),
            credential_store,
            spec,
            restored: AtomicBool::new(false),
            authorization_generation,
        })
    }

    /// Locks the manager for one operation whose saves count only until the next sign-out.
    /// 为一次操作锁定管理器，该操作的保存只在下一次退出登录之前有效。
    ///
    /// The epoch is read before waiting for the lock, so an operation requested before a sign-out,
    /// such as a code exchange of a flow the sign-out discarded, cannot save after it.
    /// 代次在等待锁之前读取，因此退出登录之前请求的操作（例如被退出登录丢弃的流程的授权码交换）
    /// 之后无法保存。
    async fn lock_manager(&self) -> tokio::sync::MutexGuard<'_, AuthorizationManager> {
        let started = self.credential_store.current_epoch();
        let manager = self.manager.lock().await;
        self.credential_store.begin_operation(started);
        manager
    }

    /// Restores stored tokens into the manager, rediscovering metadata only when tokens exist.
    /// 将已存储令牌还原到管理器，仅在存在令牌时重新发现元数据。
    ///
    /// Failed or empty restores run again on the next use; a successful one is kept.
    /// 失败或为空的还原会在下次使用时重新执行；成功的还原会被保留。
    async fn ensure_restored(&self) -> Result<(), McpRuntimeError> {
        if self.restored.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut manager = self.lock_manager().await;
        if self.restored.load(Ordering::Acquire) {
            return Ok(());
        }
        let restored = manager
            .initialize_from_store()
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.restore: {error}")))?;
        if !restored {
            return Ok(());
        }
        // Restoration configures only the client id; a pre-registered confidential client needs its secret to refresh.
        // 还原只配置客户端标识；预注册的机密客户端刷新时需要其密钥。
        if let Some(client_config) = self.preregistered_client_config(None) {
            manager.configure_client(client_config).map_err(|error| {
                McpRuntimeError::transport(format!("mcp.oauth.configure: {error}"))
            })?;
        }
        self.restored.store(true, Ordering::Release);
        Ok(())
    }

    /// Builds the pre-registered client configuration, if one is configured.
    /// 构建预注册客户端配置（如已配置）。
    fn preregistered_client_config(&self, redirect_uri: Option<&str>) -> Option<OAuthClientConfig> {
        let client_id = self.spec.client_id.as_deref()?;
        let redirect_uri = redirect_uri
            .or(self.spec.redirect_uri.as_deref())
            .unwrap_or(self.credential_store.server_url.as_str());
        let mut config =
            OAuthClientConfig::new(client_id, redirect_uri).with_scopes(self.spec.scopes.clone());
        if let Some(client_secret) = self.spec.client_secret.as_deref() {
            config = config.with_client_secret(client_secret);
        }
        Some(config)
    }

    /// Returns one authorization challenge whose callback must reach `redirect_uri`.
    /// 返回一个授权挑战，其回调必须到达 `redirect_uri`。
    ///
    /// `client_name` is used for dynamic registration when no client id is configured.
    /// 未配置客户端标识时，`client_name` 用于动态注册。
    pub async fn start_authorization(
        &self,
        client_name: &str,
        redirect_uri: &str,
    ) -> Result<McpAuthChallenge, McpRuntimeError> {
        // Restore first so a later restore cannot reconfigure the client this flow sets up.
        // 先还原，避免之后的还原重新配置本流程建立的客户端。
        self.ensure_restored().await?;
        let scope_refs = self
            .spec
            .scopes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut manager = self.lock_manager().await;
        let metadata_resolution = manager
            .resolve_metadata()
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.discover: {error}")))?;
        manager.set_metadata(metadata_resolution.metadata);

        if let Some(client_config) = self.preregistered_client_config(Some(redirect_uri)) {
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

    /// Completes authorization from the full redirect URL, validating state and the optional RFC 9207 issuer.
    /// 根据完整回调 URL 完成授权，校验 state 与可选的 RFC 9207 签发者。
    pub async fn finish_authorization(&self, callback_url: &str) -> Result<(), McpRuntimeError> {
        if let Some(denial) = authorization_denial(callback_url) {
            return Err(McpRuntimeError::transport(format!(
                "mcp.oauth.denied: {denial}"
            )));
        }
        let callback = AuthorizationCallback::from_redirect_url(callback_url).map_err(|error| {
            McpRuntimeError::invalid_argument(format!("mcp.oauth.callback: {error}"))
        })?;
        self.lock_manager()
            .await
            .exchange_code_for_token_with_issuer(
                &callback.code,
                &callback.csrf_token,
                callback.issuer.as_deref(),
            )
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.finish: {error}")))?;
        // The flow configured the client before exchanging, so the stored tokens are usable as-is.
        // 流程在交换前已配置客户端，因此已存储令牌可直接使用。
        self.restored.store(true, Ordering::Release);
        self.notify_authorization_changed();
        Ok(())
    }

    /// Clears every stored credential for this server and wakes connections waiting on authorization.
    /// 清除此服务器的全部已存储凭据，并唤醒等待授权的连接。
    pub async fn remove_authorization(&self) -> Result<(), McpRuntimeError> {
        // Clearing advances the sign-out epoch, so a refresh in flight in this or any other
        // runtime generation cannot save its tokens afterwards.
        // 清除会推进退出登录代次，因此本代次或任何其他运行时代次中进行中的刷新之后都无法保存其令牌。
        let manager = self.manager.lock().await;
        self.credential_store
            .clear()
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.remove: {error}")))?;
        self.restored.store(false, Ordering::Release);
        drop(manager);
        self.notify_authorization_changed();
        Ok(())
    }

    /// Wakes connections waiting on this session after another session changed the shared credentials.
    /// 其他会话修改共享凭据后，唤醒等待本会话的连接。
    pub fn notify_authorization_changed(&self) {
        self.authorization_generation
            .send_modify(|generation| *generation += 1);
    }

    /// Subscribes to authorization changes; the receiver observes only changes made after subscribing.
    /// 订阅授权变化；接收端只观察订阅之后发生的变化。
    pub fn subscribe_authorization_changes(&self) -> watch::Receiver<u64> {
        self.authorization_generation.subscribe()
    }

    /// Returns the current OAuth status, refreshing an expiring token when a refresh token allows it.
    /// 返回当前 OAuth 状态；当刷新令牌允许时会刷新即将过期的令牌。
    pub async fn auth_status(&self) -> Result<McpAuthStatus, McpRuntimeError> {
        let has_tokens = self
            .credential_store
            .load()
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.oauth.load: {error}")))?
            .is_some_and(|credentials| credentials.token_response.is_some());
        if !has_tokens {
            return Ok(McpAuthStatus::NotAuthenticated);
        }
        Ok(match self.access_token().await? {
            Some(_) => McpAuthStatus::Authenticated,
            None => McpAuthStatus::Expired,
        })
    }

    /// Resolves one access token, returning `None` when the user must authorize (again).
    /// 解析一个访问令牌；用户必须（重新）授权时返回 `None`。
    pub async fn access_token(&self) -> Result<Option<String>, McpRuntimeError> {
        self.ensure_restored().await?;
        let manager = self.lock_manager().await;
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

    /// Returns whether a client id was pre-registered instead of using dynamic registration.
    /// 返回是否预注册了客户端标识，而不是使用动态注册。
    pub fn has_preregistered_client(&self) -> bool {
        self.spec.client_id.is_some()
    }
}

/// Returns the provider's denial text when the redirect carries an OAuth `error` parameter.
/// 当回调携带 OAuth `error` 参数时返回供应商的拒绝说明。
fn authorization_denial(callback_url: &str) -> Option<String> {
    let parsed = Url::parse(callback_url).ok()?;
    let mut error = None;
    let mut description = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "error" => error = Some(value.into_owned()),
            "error_description" => description = Some(value.into_owned()),
            _ => {}
        }
    }
    let error = error?;
    Some(match description {
        Some(description) => format!("{error}: {description}"),
        None => error,
    })
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
                "mcp.oauth.authorization-url.missing-state: the authorization URL has no state parameter"
                    .to_string(),
            )
        })
}

#[cfg(test)]
mod tests;

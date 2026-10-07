//! Host-driven OAuth sign-in for remote MCP servers.
//! 远端 MCP 服务器的宿主驱动 OAuth 登录。
//!
//! A flow starts from the settings page, opens the system browser, and completes either through a
//! loopback receiver bound to the redirect URI or through the callback URL the user pastes. The
//! session that started the flow holds its PKCE state, so completion always uses that session and
//! then wakes the session of the current runtime generation when the two differ.
//! 流程从设置页启动并打开系统浏览器，通过绑定到回调 URI 的本机回环接收器、或用户粘贴的回调 URL
//! 完成。发起流程的会话持有 PKCE 状态，因此完成时总是使用该会话；若当前运行时代次的会话不同，
//! 再唤醒当前会话。

mod credential_store;
mod loopback;

#[cfg(test)]
mod tests;

pub(crate) use credential_store::OsMcpOAuthCredentialBackend;

use crate::{
    SharedServiceBridgeState, agent_mcp_runtime_from_state, current_unix_millis_value,
    shell_host_from_state,
};
use loopback::{CallbackReceiver, plan_redirect, receive_callback, respond};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::task::AbortHandle;
use tokio::time::Instant;
use vulcan_agent_runtime::mcp::{McpAuthStatus, McpOAuthSession, mcp_oauth_credential_backend};

/// Lifetime of one sign-in flow, for both loopback and pasted completion.
/// 单个登录流程的有效期，同时适用于回环与粘贴完成方式。
const MCP_OAUTH_FLOW_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Client name sent during dynamic client registration.
/// 动态客户端注册时发送的客户端名称。
const MCP_OAUTH_CLIENT_NAME: &str = "Vulcan Code";

/// Phase of one sign-in flow.
/// 单个登录流程的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpOAuthFlowPhase {
    /// Waiting for the browser callback or a pasted callback URL.
    /// 等待浏览器回调或粘贴的回调 URL。
    AwaitingCallback,
    /// Exchanging the authorization code for tokens.
    /// 正在用授权码交换令牌。
    Exchanging,
    /// Tokens were stored.
    /// 令牌已存储。
    Succeeded,
    /// The flow ended with an error.
    /// 流程以错误结束。
    Failed,
    /// No callback arrived before the flow expired.
    /// 流程过期前没有收到回调。
    Expired,
}

/// How the callback reaches the host.
/// 回调到达宿主的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpOAuthCallbackMode {
    /// A loopback receiver completes the flow automatically.
    /// 本机回环接收器自动完成流程。
    Loopback,
    /// The user pastes the final callback URL.
    /// 用户粘贴最终回调 URL。
    Manual,
}

/// Public view of one sign-in flow; carries no tokens or PKCE material.
/// 单个登录流程的公开视图；不含令牌或 PKCE 材料。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpOAuthFlowSnapshot {
    /// Opaque flow identifier echoed by finish and cancel.
    /// 由完成与取消操作回传的不透明流程标识。
    flow_id: String,
    /// Current phase.
    /// 当前阶段。
    phase: McpOAuthFlowPhase,
    /// Callback delivery mode.
    /// 回调送达方式。
    callback_mode: McpOAuthCallbackMode,
    /// URL the user opens to authorize.
    /// 用户打开以完成授权的 URL。
    authorization_url: String,
    /// Redirect URI registered for this flow.
    /// 本流程登记的回调 URI。
    redirect_uri: String,
    /// Failure detail for failed flows.
    /// 失败流程的错误详情。
    error: Option<String>,
    /// Expiry time in Unix milliseconds.
    /// 以 Unix 毫秒表示的过期时间。
    expires_at_unix_ms: i128,
}

/// OAuth view of one MCP server for the settings page.
/// 设置页使用的单个 MCP 服务器 OAuth 视图。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpOAuthStatusSnapshot {
    /// Server identifier.
    /// 服务器标识。
    server_id: String,
    /// Credential status, or `None` when it could not be determined.
    /// 凭据状态；无法确定时为 `None`。
    auth_status: Option<McpAuthStatus>,
    /// Why the credential status could not be determined.
    /// 无法确定凭据状态的原因。
    status_error: Option<String>,
    /// Latest sign-in flow, if one is retained.
    /// 最近的登录流程（如保留）。
    flow: Option<McpOAuthFlowSnapshot>,
}

/// Request completing a flow with a pasted callback URL.
/// 使用粘贴的回调 URL 完成流程的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct McpOAuthFinishRequest {
    /// Server identifier.
    /// 服务器标识。
    server_id: String,
    /// Flow identifier returned by start.
    /// 启动时返回的流程标识。
    flow_id: String,
    /// Full URL the browser was redirected to.
    /// 浏览器被重定向到的完整 URL。
    callback_url: String,
}

/// Request cancelling one flow.
/// 取消单个流程的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct McpOAuthCancelRequest {
    /// Server identifier.
    /// 服务器标识。
    server_id: String,
    /// Flow identifier returned by start.
    /// 启动时返回的流程标识。
    flow_id: String,
}

/// One retained sign-in flow.
/// 单个保留的登录流程。
struct McpOAuthFlow {
    /// Opaque flow identifier.
    /// 不透明流程标识。
    flow_id: String,
    /// Session holding this flow's PKCE state.
    /// 持有本流程 PKCE 状态的会话。
    session: Arc<McpOAuthSession>,
    /// OAuth state parameter expected on the callback.
    /// 回调上期望的 OAuth state 参数。
    oauth_state: String,
    /// Current phase.
    /// 当前阶段。
    phase: McpOAuthFlowPhase,
    /// Callback delivery mode.
    /// 回调送达方式。
    callback_mode: McpOAuthCallbackMode,
    /// Authorization URL.
    /// 授权 URL。
    authorization_url: String,
    /// Redirect URI.
    /// 回调 URI。
    redirect_uri: String,
    /// Failure detail.
    /// 失败详情。
    error: Option<String>,
    /// Monotonic expiry.
    /// 单调时钟过期时间。
    expires_at: Instant,
    /// Wall-clock expiry for display.
    /// 用于展示的墙钟过期时间。
    expires_at_unix_ms: i128,
    /// Loopback receiver task, aborted when the flow is replaced, cancelled or completed manually.
    /// 回环接收任务；流程被替换、取消或手动完成时中止。
    receiver_task: Option<AbortHandle>,
}

impl McpOAuthFlow {
    /// Marks an awaiting flow expired once its deadline passed.
    /// 等待中的流程超过期限后标记为过期。
    fn refresh_expiry(&mut self) {
        if self.phase == McpOAuthFlowPhase::AwaitingCallback && Instant::now() >= self.expires_at {
            self.phase = McpOAuthFlowPhase::Expired;
            self.stop_receiver();
        }
    }

    /// Aborts the loopback receiver, releasing its port.
    /// 中止回环接收器并释放其端口。
    fn stop_receiver(&mut self) {
        if let Some(task) = self.receiver_task.take() {
            task.abort();
        }
    }

    /// Builds the public snapshot.
    /// 构建公开快照。
    fn snapshot(&self) -> McpOAuthFlowSnapshot {
        McpOAuthFlowSnapshot {
            flow_id: self.flow_id.clone(),
            phase: self.phase,
            callback_mode: self.callback_mode,
            authorization_url: self.authorization_url.clone(),
            redirect_uri: self.redirect_uri.clone(),
            error: self.error.clone(),
            expires_at_unix_ms: self.expires_at_unix_ms,
        }
    }
}

/// Retained flows keyed by server identifier; at most one flow per server.
/// 以服务器标识为键保留的流程；每个服务器最多一个流程。
static MCP_OAUTH_FLOWS: LazyLock<Mutex<BTreeMap<String, McpOAuthFlow>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Locks the flow registry; entries are replaced whole, so a poisoned lock still holds consistent flows.
/// 锁定流程注册表；条目整体替换，因此中毒的锁仍持有一致的流程。
fn flows() -> MutexGuard<'static, BTreeMap<String, McpOAuthFlow>> {
    MCP_OAUTH_FLOWS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Updates the flow for `server_id` only when it is still the flow `flow_id`.
/// 仅当 `server_id` 的流程仍为 `flow_id` 时更新它。
fn update_flow(server_id: &str, flow_id: &str, update: impl FnOnce(&mut McpOAuthFlow)) {
    if let Some(flow) = flows()
        .get_mut(server_id)
        .filter(|flow| flow.flow_id == flow_id)
    {
        update(flow);
    }
}

/// Resolves the OAuth session of the current runtime generation for one server.
/// 解析当前运行时代次中某服务器的 OAuth 会话。
async fn current_session(
    state: &SharedServiceBridgeState,
    server_id: &str,
) -> Result<Arc<McpOAuthSession>, String> {
    agent_mcp_runtime_from_state(state)?
        .server_oauth_session(server_id)
        .await?
        .ok_or_else(|| {
            format!("mcp_oauth_not_configured: MCP server `{server_id}` has no OAuth configuration")
        })
}

/// Wakes the current generation's session when a different session completed authorization.
/// 当不同会话完成授权时，唤醒当前代次的会话。
async fn wake_current_session(
    state: &SharedServiceBridgeState,
    server_id: &str,
    completed: &Arc<McpOAuthSession>,
) {
    if let Ok(current) = current_session(state, server_id).await
        && !Arc::ptr_eq(&current, completed)
    {
        current.notify_authorization_changed();
    }
}

/// Returns the OAuth status and latest flow for one server.
/// 返回某服务器的 OAuth 状态与最近流程。
pub(crate) async fn service_mcp_oauth_status(
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<McpOAuthStatusSnapshot, String> {
    let (auth_status, status_error) = match agent_mcp_runtime_from_state(&state)?
        .server_oauth_session(&server_id)
        .await
    {
        Ok(None) => (Some(McpAuthStatus::NotConfigured), None),
        Ok(Some(session)) => match session.auth_status().await {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error.to_string())),
        },
        Err(error) => (None, Some(error)),
    };
    let flow = flows().get_mut(&server_id).map(|flow| {
        flow.refresh_expiry();
        flow.snapshot()
    });
    Ok(McpOAuthStatusSnapshot {
        server_id,
        auth_status,
        status_error,
        flow,
    })
}

/// Starts a sign-in flow, replacing any earlier flow for the server, and opens the browser.
/// 启动登录流程，替换该服务器先前的流程，并打开浏览器。
///
/// A browser that fails to open does not fail the flow: the snapshot carries the URL to open manually.
/// 浏览器打开失败不会使流程失败：快照携带可手动打开的 URL。
pub(crate) async fn service_start_mcp_oauth(
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<McpOAuthStatusSnapshot, String> {
    let session = current_session(&state, &server_id).await?;
    let plan = plan_redirect(session.redirect_uri(), session.has_preregistered_client()).await?;
    let challenge = session
        .start_authorization(MCP_OAUTH_CLIENT_NAME, &plan.redirect_uri)
        .await
        .map_err(|error| error.to_string())?;
    ensure_openable_authorization_url(challenge.authorization_url())?;
    let flow_id = uuid::Uuid::new_v4().to_string();
    let expires_at = Instant::now() + MCP_OAUTH_FLOW_TIMEOUT;
    let expires_at_unix_ms =
        current_unix_millis_value() + i128::from(duration_millis(MCP_OAUTH_FLOW_TIMEOUT));
    let (callback_mode, receiver_task) = match plan.receiver {
        CallbackReceiver::Manual => (McpOAuthCallbackMode::Manual, None),
        CallbackReceiver::Loopback {
            listener,
            path,
            origin,
        } => {
            let task = tokio::spawn(run_loopback_flow(
                state.clone(),
                server_id.clone(),
                flow_id.clone(),
                session.clone(),
                challenge.oauth_state().to_string(),
                listener,
                path,
                origin,
                expires_at,
            ));
            (McpOAuthCallbackMode::Loopback, Some(task.abort_handle()))
        }
    };
    let flow = McpOAuthFlow {
        flow_id,
        session,
        oauth_state: challenge.oauth_state().to_string(),
        phase: McpOAuthFlowPhase::AwaitingCallback,
        callback_mode,
        authorization_url: challenge.authorization_url().to_string(),
        redirect_uri: plan.redirect_uri,
        error: None,
        expires_at,
        expires_at_unix_ms,
        receiver_task,
    };
    if let Some(mut previous) = flows().insert(server_id.clone(), flow) {
        previous.stop_receiver();
    }
    if let Err(error) = shell_host_from_state(&state)?.open_url(challenge.authorization_url()) {
        tracing::warn!("[mcp-oauth] failed to open the authorization URL: {error}");
    }
    service_mcp_oauth_status(state, server_id).await
}

/// Rejects authorization URLs the host must not hand to the system browser.
/// 拒绝宿主不得交给系统浏览器的授权 URL。
///
/// The authorization endpoint comes from remote server metadata, and the system opener dispatches
/// any scheme to its registered handler, so only the shared external-URL rule (HTTPS) is accepted.
/// 授权端点来自远端服务器元数据，而系统打开器会把任意协议分派给其注册处理程序，
/// 因此只接受共享外部 URL 规则（HTTPS）。
fn ensure_openable_authorization_url(url: &str) -> Result<(), String> {
    if crate::service_bridge_io::external_url_is_safe(url) {
        return Ok(());
    }
    Err(String::from(
        "mcp_oauth_authorization_url_unsafe: the authorization server returned an authorization URL that is not HTTPS or embeds credentials; refusing to open it",
    ))
}

/// Receives the loopback callback and completes the flow.
/// 接收回环回调并完成流程。
#[allow(clippy::too_many_arguments)]
async fn run_loopback_flow(
    state: SharedServiceBridgeState,
    server_id: String,
    flow_id: String,
    session: Arc<McpOAuthSession>,
    oauth_state: String,
    listener: tokio::net::TcpListener,
    path: String,
    origin: String,
    deadline: Instant,
) {
    let (callback_url, mut stream) =
        match receive_callback(&listener, &path, &origin, &oauth_state, deadline).await {
            Ok(received) => received,
            Err(error) => {
                update_flow(&server_id, &flow_id, |flow| {
                    flow.receiver_task = None;
                    if flow.phase == McpOAuthFlowPhase::AwaitingCallback {
                        flow.phase = McpOAuthFlowPhase::Expired;
                        flow.error = Some(error);
                    }
                });
                return;
            }
        };
    // Release the port before the exchange; the flow can no longer receive another callback.
    // 交换前释放端口；流程不会再接收其他回调。
    drop(listener);
    update_flow(&server_id, &flow_id, |flow| {
        flow.receiver_task = None;
        flow.phase = McpOAuthFlowPhase::Exchanging;
    });
    let result = session.finish_authorization(&callback_url).await;
    let _ = respond(&mut stream, 200, result.is_ok()).await;
    complete_flow(&state, &server_id, &flow_id, &session, result).await;
}

/// Records the outcome of a code exchange and wakes the current session on success.
/// 记录授权码交换结果，成功时唤醒当前会话。
async fn complete_flow(
    state: &SharedServiceBridgeState,
    server_id: &str,
    flow_id: &str,
    session: &Arc<McpOAuthSession>,
    result: Result<(), vulcan_agent_runtime::mcp::McpRuntimeError>,
) {
    let error = result.as_ref().err().map(ToString::to_string);
    update_flow(server_id, flow_id, |flow| {
        flow.phase = if error.is_some() {
            McpOAuthFlowPhase::Failed
        } else {
            McpOAuthFlowPhase::Succeeded
        };
        flow.error = error;
    });
    if result.is_ok() {
        wake_current_session(state, server_id, session).await;
    }
}

/// Completes a flow with the callback URL the user pasted.
/// 使用用户粘贴的回调 URL 完成流程。
pub(crate) async fn service_finish_mcp_oauth(
    state: SharedServiceBridgeState,
    request: McpOAuthFinishRequest,
) -> Result<McpOAuthStatusSnapshot, String> {
    let session = claim_flow_for_exchange(&request)?;
    let result = session
        .finish_authorization(request.callback_url.trim())
        .await;
    let outcome = result.as_ref().err().map(ToString::to_string);
    complete_flow(
        &state,
        &request.server_id,
        &request.flow_id,
        &session,
        result,
    )
    .await;
    if let Some(error) = outcome {
        return Err(error);
    }
    service_mcp_oauth_status(state, request.server_id).await
}

/// Moves an awaiting flow to the exchange phase after checking its identity and callback state.
/// 校验流程身份与回调 state 后，把等待中的流程移入交换阶段。
fn claim_flow_for_exchange(
    request: &McpOAuthFinishRequest,
) -> Result<Arc<McpOAuthSession>, String> {
    let mut flows = flows();
    let flow = flows
        .get_mut(&request.server_id)
        .filter(|flow| flow.flow_id == request.flow_id)
        .ok_or_else(|| {
            String::from(
                "mcp_oauth_flow_not_found: start sign-in again; this flow is no longer current",
            )
        })?;
    flow.refresh_expiry();
    if flow.phase != McpOAuthFlowPhase::AwaitingCallback {
        return Err(String::from(
            "mcp_oauth_flow_not_awaiting: this sign-in flow is no longer waiting for a callback",
        ));
    }
    let state = reqwest::Url::parse(request.callback_url.trim())
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        });
    if state.as_deref() != Some(flow.oauth_state.as_str()) {
        return Err(String::from(
            "mcp_oauth_state_mismatch: the pasted URL does not belong to this sign-in flow",
        ));
    }
    flow.stop_receiver();
    flow.phase = McpOAuthFlowPhase::Exchanging;
    Ok(flow.session.clone())
}

/// Cancels one flow and releases its loopback port.
/// 取消单个流程并释放其回环端口。
pub(crate) async fn service_cancel_mcp_oauth(
    state: SharedServiceBridgeState,
    request: McpOAuthCancelRequest,
) -> Result<McpOAuthStatusSnapshot, String> {
    {
        let mut flows = flows();
        if flows
            .get(&request.server_id)
            .is_some_and(|flow| flow.flow_id == request.flow_id)
            && let Some(mut flow) = flows.remove(&request.server_id)
        {
            flow.stop_receiver();
        }
    }
    service_mcp_oauth_status(state, request.server_id).await
}

/// Signs out: removes stored credentials and any flow for the server.
/// 退出登录：移除该服务器的已存储凭据与任何流程。
///
/// A server without a live runtime (for example a disabled one) is cleared directly in the backend.
/// 没有实时运行时的服务器（例如已禁用）直接在后端清除。
pub(crate) async fn service_remove_mcp_oauth(
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<McpOAuthStatusSnapshot, String> {
    discard_flow(&server_id);
    match agent_mcp_runtime_from_state(&state)?
        .server_oauth_session(&server_id)
        .await
    {
        Ok(Some(session)) => session
            .remove_authorization()
            .await
            .map_err(|error| error.to_string())?,
        Ok(None) | Err(_) => clear_stored_credentials(&server_id).await?,
    }
    service_mcp_oauth_status(state, server_id).await
}

/// Drops the retained flow for one server, releasing its port.
/// 丢弃某服务器保留的流程并释放其端口。
fn discard_flow(server_id: &str) {
    if let Some(mut flow) = flows().remove(server_id) {
        flow.stop_receiver();
    }
}

/// Clears stored credentials for one server straight from the process backend.
/// 直接从进程后端清除某服务器的已存储凭据。
async fn clear_stored_credentials(server_id: &str) -> Result<(), String> {
    let backend = mcp_oauth_credential_backend();
    let server_id = server_id.to_string();
    tokio::task::spawn_blocking(move || backend.clear(&server_id))
        .await
        .map_err(|error| error.to_string())?
}

/// Forgets every OAuth trace of a server whose configuration was removed.
/// 忘记已移除配置的服务器的全部 OAuth 痕迹。
///
/// Removal is already persisted, so a failed credential deletion is reported but not propagated.
/// 移除已经持久化，因此凭据删除失败只记录，不向上传播。
pub(crate) async fn forget_removed_mcp_server_oauth(server_id: &str) {
    discard_flow(server_id);
    if let Err(error) = clear_stored_credentials(server_id).await {
        tracing::warn!("[mcp-oauth] failed to delete credentials of removed server: {error}");
    }
}

/// Converts a duration to whole milliseconds, saturating.
/// 将时长转换为整毫秒，溢出时饱和。
fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

//! Internal Core grants connecting one trusted AI owner to one supervised browser host.
//! Core 内部授权，把一个可信 AI 所有者连接到一个受监督浏览器宿主。

use super::{
    BROWSER_ERROR_AUTOMATION_CONFLICT, BROWSER_ERROR_INVALID_REQUEST, BrowserAutomationState,
    BrowserHostRequestOutcome, BrowserLeaseState, BrowserManager, BrowserSessionProjection,
    BrowserSessionStatus,
};
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

/// Short-lived, one-use endpoint kept inside Core and never returned by browser RPC.
/// 仅在 Core 内部保存、绝不通过浏览器 RPC 返回的短期一次性端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserAutomationGrant {
    /// Core browser session selected by the trusted AI conversation.
    /// 由可信 AI 会话选中的 Core 浏览器会话。
    pub session_id: String,
    /// Core-generated connection identity used for exact revocation.
    /// Core 生成、用于精确撤销的连接身份。
    pub connection_id: String,
    /// One-use loopback WebSocket endpoint injected into an owner-scoped MCP child.
    /// 注入所有者范围 MCP 子进程的一次性回环 WebSocket 端点。
    pub endpoint: String,
    /// Host-reported Unix millisecond expiry for the unused endpoint.
    /// 宿主报告的未使用端点 Unix 毫秒过期时刻。
    pub expires_at_ms: u64,
}

impl BrowserManager {
    /// Selects one running browser owned by the trusted AI conversation.
    /// 选中一个由可信 AI 会话持有且正在运行的浏览器。
    ///
    /// `owner_session_id` is injected by the host, and `session_id` is a Core browser identity.
    /// `owner_session_id` 由宿主注入；`session_id` 是 Core 浏览器身份。
    ///
    /// Returns the selected browser projection or one indistinguishable ownership error.
    /// 返回选中浏览器的投影，或一项不可区分的归属错误。
    pub(crate) fn select_ai_browser(
        &self,
        owner_session_id: &str,
        session_id: &str,
    ) -> Result<BrowserSessionProjection, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        let session = state
            .sessions
            .get(session_id)
            .filter(|session| session.projection.owner_session_id == owner_session_id)
            .filter(|session| session.projection.status == BrowserSessionStatus::Running)
            .ok_or_else(|| String::from(super::BROWSER_ERROR_UNKNOWN_SESSION))?;
        let projection = session.projection.clone();
        state
            .ai_selected_sessions
            .insert(String::from(owner_session_id), String::from(session_id));
        Ok(projection)
    }

    /// Reads the selected live browser for exactly one trusted AI conversation.
    /// 读取精确属于一个可信 AI 会话的当前活动浏览器。
    ///
    /// `owner_session_id` comes from the host invocation context, never model arguments.
    /// `owner_session_id` 来自宿主调用上下文，绝不来自模型参数。
    ///
    /// Returns no browser when nothing is selected or the selected browser stopped.
    /// 未选择或选中浏览器已停止时返回空值。
    pub(crate) fn selected_ai_browser(
        &self,
        owner_session_id: &str,
    ) -> Result<Option<BrowserSessionProjection>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?;
        Ok(state
            .ai_selected_sessions
            .get(owner_session_id)
            .and_then(|session_id| state.sessions.get(session_id))
            .filter(|session| session.projection.owner_session_id == owner_session_id)
            .filter(|session| session.projection.status == BrowserSessionStatus::Running)
            .map(|session| session.projection.clone()))
    }

    /// Opens one Core-only host gateway after checking conversation ownership and the exclusive lease.
    /// 校验会话归属与独占租约后打开一个仅供 Core 使用的宿主网关。
    ///
    /// `owner_session_id` comes from the trusted AI invocation context; `lease_secret` is held by
    /// Core's control scheduler; `connection_id` is fresh for this single MCP process.
    /// `owner_session_id` 来自可信 AI 调用上下文；`lease_secret` 由 Core 控制调度器持有；
    /// `connection_id` 为本次 MCP 进程新生成。
    ///
    /// Returns a one-use endpoint retained only by Core, or a typed diagnostic.
    /// 返回仅由 Core 保存的一次性端点，或强类型诊断。
    pub(crate) fn open_automation_gateway(
        &self,
        session_id: &str,
        owner_session_id: &str,
        lease_secret: &str,
        connection_id: &str,
    ) -> Result<BrowserAutomationGrant, String> {
        if connection_id.trim().is_empty() {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        // now_ms is resolved before host I/O so a clock encoding error cannot abandon an opened grant.
        // now_ms 在宿主输入输出前解析，避免时钟编码错误使已打开的授权无人管理。
        let now_ms = super::current_unix_millis_text()
            .parse::<u64>()
            .map_err(|error| format!("failed to decode Core clock: {error}"))?;
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.require_owner(session_id, owner_session_id)?;
        self.authorize_host_write(session_id, lease_secret)?;
        if self.session_projection(session_id)?.automation_state == BrowserAutomationState::Attached
        {
            return Err(String::from(BROWSER_ERROR_AUTOMATION_CONFLICT));
        }
        self.attach_automation(session_id, lease_secret, connection_id)?;
        let outcome = self.request_host(
            session_id,
            "automation.open",
            json!({ "connectionId": connection_id }),
        );
        match outcome {
            Ok(BrowserHostRequestOutcome::Completed(value)) => {
                let endpoint = value.get("endpoint").and_then(serde_json::Value::as_str);
                let expires_at_ms = value
                    .get("expiresAt")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| value.parse::<u64>().ok());
                match (endpoint, expires_at_ms) {
                    (Some(endpoint), Some(expires_at_ms))
                        if automation_endpoint_is_local_gateway(endpoint)
                            && expires_at_ms > now_ms =>
                    {
                        Ok(BrowserAutomationGrant {
                            session_id: String::from(session_id),
                            connection_id: String::from(connection_id),
                            endpoint: String::from(endpoint),
                            expires_at_ms,
                        })
                    }
                    _ => {
                        self.mark_operation_unconfirmed(
                            session_id,
                            "automation.open invalid host grant",
                        )?;
                        Err(String::from(
                            "unknown: automation.open returned an invalid grant",
                        ))
                    }
                }
            }
            Ok(BrowserHostRequestOutcome::Failed { code, message }) => {
                self.detach_automation(session_id, connection_id)?;
                Err(format!("{code}: {message}"))
            }
            Ok(BrowserHostRequestOutcome::Unknown { reason }) => {
                self.mark_operation_unconfirmed(session_id, "automation.open outcome unknown")?;
                Err(format!("unknown: {reason}"))
            }
            Err(reason) => {
                self.mark_operation_unconfirmed(
                    session_id,
                    "automation.open transport unavailable",
                )?;
                Err(reason)
            }
        }
    }

    /// Revokes the exact gateway and detaches automation only after host confirmation.
    /// 仅在宿主确认后撤销精确网关并分离自动化状态。
    ///
    /// `owner_session_id` is the trusted conversation, and `connection_id` must match the grant.
    /// `owner_session_id` 是可信会话，`connection_id` 必须与授权一致。
    ///
    /// Returns the updated browser projection on confirmed revocation.
    /// 确认撤销后返回更新的浏览器投影。
    pub(crate) fn revoke_automation_gateway(
        &self,
        session_id: &str,
        owner_session_id: &str,
        connection_id: &str,
    ) -> Result<super::BrowserSessionProjection, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.require_owner(session_id, owner_session_id)?;
        let projection = self.session_projection(session_id)?;
        if projection.automation_state != BrowserAutomationState::Attached {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        // A wrong connection identity is rejected before touching the live gateway.
        // 错误的连接身份在触及活动网关前即被拒绝。
        let current_connection_matches = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock browser manager: {error}"))?
            .sessions
            .get(session_id)
            .and_then(|session| session.automation_connection_id.as_deref())
            == Some(connection_id);
        if !current_connection_matches {
            return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
        }
        match self.request_host(
            session_id,
            "automation.close",
            json!({ "connectionId": connection_id }),
        ) {
            Ok(BrowserHostRequestOutcome::Completed(value))
                if value.get("revoked").and_then(serde_json::Value::as_bool) == Some(true)
                    && value.get("status").and_then(serde_json::Value::as_str)
                        == Some("revoked") =>
            {
                self.detach_automation(session_id, connection_id)?;
                // Only a host-confirmed gateway revocation resolves the uncertainty created by
                // an expired lease while Playwright was still attached.
                // 只有宿主确认撤销网关，才能解除 Playwright 挂接期间租约到期所造成的不确定状态。
                self.confirm_operations_stopped(
                    session_id,
                    Some(super::BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION),
                )
            }
            Ok(BrowserHostRequestOutcome::Completed(value))
                if value.get("status").and_then(serde_json::Value::as_str) == Some("inFlight") =>
            {
                // Socket termination cannot prove Chrome stopped a command already forwarded.
                // 终止套接字不能证明 Chrome 已停止此前转发的命令。
                self.mark_operation_unconfirmed(session_id, "automation.close in-flight CDP")?;
                Err(String::from(super::BROWSER_ERROR_LEASE_BLOCKED))
            }
            Ok(BrowserHostRequestOutcome::Failed { code, message }) => {
                self.mark_operation_unconfirmed(session_id, "automation.close refused by host")?;
                Err(format!("{code}: {message}"))
            }
            Ok(outcome) => {
                self.mark_operation_unconfirmed(session_id, "automation.close outcome unknown")?;
                Err(format!("unknown: {outcome:?}"))
            }
            Err(reason) => {
                self.mark_operation_unconfirmed(
                    session_id,
                    "automation.close transport unavailable",
                )?;
                Err(reason)
            }
        }
    }

    /// Ends a blocked AI browser only after its host confirms the Chromium context is closed.
    /// 仅在宿主确认 Chromium 上下文已关闭后结束被阻塞的 AI 浏览器。
    ///
    /// `session_id` is the browser, `owner_session_id` is its trusted conversation, and
    /// `connection_id` must still name the attached automation grant. Returns the closed session.
    /// `session_id` 是浏览器，`owner_session_id` 是可信会话，`connection_id` 必须仍指向已附着的
    /// 自动化授权；返回已关闭的会话。
    pub(crate) fn close_uncertain_automation_session(
        &self,
        session_id: &str,
        owner_session_id: &str,
        connection_id: &str,
    ) -> Result<super::BrowserSessionProjection, String> {
        let gate = self.dispatch_gate(session_id)?;
        let _dispatch = gate
            .lock()
            .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
        self.require_owner(session_id, owner_session_id)?;
        let context_closed = {
            let state = self
                .inner
                .lock()
                .map_err(|error| format!("failed to lock browser manager: {error}"))?;
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| String::from(super::BROWSER_ERROR_UNKNOWN_SESSION))?;
            if session.projection.automation_state != BrowserAutomationState::Attached
                || session.automation_connection_id.as_deref() != Some(connection_id)
                || session.projection.lease_state != BrowserLeaseState::Blocked
            {
                return Err(String::from(super::BROWSER_ERROR_INVALID_REQUEST));
            }
            session.recovery_context_closed
        };
        if !context_closed {
            match self.request_host(session_id, "host.closeContext", json!({}))? {
                BrowserHostRequestOutcome::Completed(value)
                    if value.get("closed").and_then(serde_json::Value::as_bool) == Some(true) =>
                {
                    self.inner
                        .lock()
                        .map_err(|error| format!("failed to lock browser manager: {error}"))?
                        .sessions
                        .get_mut(session_id)
                        .ok_or_else(|| String::from(super::BROWSER_ERROR_UNKNOWN_SESSION))?
                        .recovery_context_closed = true;
                }
                outcome => {
                    return Err(format!(
                        "{}: browser host did not confirm context closure: {outcome:?}",
                        super::BROWSER_ERROR_LEASE_BLOCKED
                    ));
                }
            }
        }
        // The host may acknowledge shutdown before its process exits. Keep the exact process
        // handle and the blocked lease until try_wait observes termination.
        // 宿主可能先确认关闭请求再退出；在 try_wait 观察到退出之前保留精确进程句柄和阻塞租约。
        let _ = self.request_host(session_id, "host.shutdown", json!({}));
        if !self.wait_owned_host_exit(session_id, super::BROWSER_HOST_SHUTDOWN_EXIT_WAIT)? {
            self.owned_host(session_id)?
                .lock()
                .map_err(|error| format!("failed to lock browser host: {error}"))?
                .process
                .kill();
            if !self.wait_owned_host_exit(session_id, super::BROWSER_HOST_KILL_EXIT_WAIT)? {
                return Err(format!(
                    "{}: browser host process did not exit after context closure",
                    super::BROWSER_ERROR_LEASE_BLOCKED
                ));
            }
        }
        self.close_session_under_gate(session_id)
    }
}

/// Accepts only a browser-level WebSocket capability at an IPv4 loopback socket.
/// 仅接受 IPv4 回环套接字上的浏览器级 WebSocket 能力端点。
///
/// `endpoint` is the host response being checked; returns true only for the exact gateway shape.
/// `endpoint` 是待校验的宿主响应；仅在精确网关结构下返回 true。
fn automation_endpoint_is_local_gateway(endpoint: &str) -> bool {
    let Some(address_and_path) = endpoint.strip_prefix("ws://") else {
        return false;
    };
    let Some((address, capability)) = address_and_path.split_once("/devtools/browser/") else {
        return false;
    };
    let Ok(socket) = address.parse::<SocketAddr>() else {
        return false;
    };
    socket.ip() == IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        && socket.port() != 0
        && !capability.is_empty()
        && !capability.contains('/')
        && !capability.contains('?')
        && !capability.contains('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies one AI conversation cannot select or read another conversation's browser.
    /// 验证一个 AI 会话无法选择或读取其他会话的浏览器。
    #[test]
    fn ai_browser_selection_stays_with_its_original_conversation() {
        let manager = BrowserManager::new();
        let first = manager
            .create_owned_session(
                "conversation-a",
                super::super::BrowserProfileMode::Ephemeral,
            )
            .expect("create first browser");
        let second = manager
            .create_owned_session(
                "conversation-b",
                super::super::BrowserProfileMode::Ephemeral,
            )
            .expect("create second browser");
        assert_eq!(
            manager
                .selected_ai_browser("conversation-a")
                .expect("read A"),
            None
        );
        assert!(
            manager
                .select_ai_browser("conversation-a", &second.session_id)
                .is_err()
        );
        assert_eq!(
            manager
                .selected_ai_browser("conversation-a")
                .expect("read A"),
            None
        );
        assert_eq!(
            manager
                .select_ai_browser("conversation-a", &first.session_id)
                .expect("select A"),
            first
        );
        assert_eq!(
            manager
                .selected_ai_browser("conversation-a")
                .expect("read A"),
            Some(first.clone())
        );
        assert_eq!(
            manager
                .selected_ai_browser("conversation-b")
                .expect("read B"),
            None
        );
        manager
            .close_session(&first.session_id)
            .expect("close selected browser");
        assert_eq!(
            manager
                .selected_ai_browser("conversation-a")
                .expect("read after close"),
            None
        );
    }

    /// Verifies malformed and non-loopback endpoints cannot be injected into an owner MCP child.
    /// 验证格式错误或非回环端点不能注入所有者 MCP 子进程。
    #[test]
    fn automation_gateway_accepts_only_one_local_browser_capability() {
        assert!(automation_endpoint_is_local_gateway(
            "ws://127.0.0.1:35123/devtools/browser/secret"
        ));
        for endpoint in [
            "ws://192.168.1.2:35123/devtools/browser/secret",
            "ws://127.0.0.1:0/devtools/browser/secret",
            "ws://127.0.0.1:35123/devtools/browser/",
            "ws://127.0.0.1:35123/devtools/browser/secret/extra",
            "ws://127.0.0.1:35123/devtools/page/secret",
            "wss://127.0.0.1:35123/devtools/browser/secret",
        ] {
            assert!(
                !automation_endpoint_is_local_gateway(endpoint),
                "{endpoint}"
            );
        }
    }
}

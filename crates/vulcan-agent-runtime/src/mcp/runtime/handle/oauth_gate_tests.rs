//! An OAuth-configured server without tokens waits for authorization instead of failing.
//! 配置了 OAuth 但没有令牌的服务器会等待授权，而不是失败。

use super::*;

/// One bounded deadline for observing the connection task.
/// 观察连接任务的有界期限。
const OBSERVE: Duration = Duration::from_secs(5);

/// Polls the single server state until `accept` holds or the deadline passes.
/// 轮询唯一服务器状态，直至 `accept` 成立或超过期限。
async fn wait_for_state(
    runtime: &McpRuntimeHandle,
    accept: impl Fn(&McpRuntimeServerState) -> bool,
) -> McpRuntimeServerState {
    let deadline = tokio::time::Instant::now() + OBSERVE;
    loop {
        let state = runtime
            .server_states()
            .into_iter()
            .next()
            .expect("one configured server");
        if accept(&state) || tokio::time::Instant::now() >= deadline {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn unauthorized_oauth_server_waits_without_consuming_retries() {
    // The port is never contacted: the gate holds the launch before any connection attempt.
    // 该端口从不被访问：门控在任何连接尝试前拦住启动。
    let configuration = McpServerConfig {
        source: crate::mcp::config::McpServerSource::UserConfigured,
        id: "oauth-gate-http".into(),
        name: "OAuth gate".into(),
        enabled: true,
        transport: crate::mcp::config::McpServerTransportConfig::Http {
            url: "http://127.0.0.1:9/mcp".into(),
            bearer_token: None,
            headers: BTreeMap::new(),
            headers_from_env: BTreeMap::new(),
            oauth: Some(crate::mcp::config::McpServerOAuthConfig {
                client_id: None,
                client_secret: None,
                scopes: Vec::new(),
                redirect_uri: None,
            }),
        },
    };
    let (resources, _release) = vulcan_mcp_runtime::McpTransportResources::new(());
    let runtime = create_mcp_runtime(vec![configuration], resources, McpRuntimePurpose::Managed)
        .expect("runtime");

    let state = wait_for_state(&runtime, |state| {
        state.status == McpRuntimeServerStatus::AwaitingAuthorization
    })
    .await;
    assert_eq!(state.status, McpRuntimeServerStatus::AwaitingAuthorization);
    assert_eq!(state.retry_count, 0);

    // Host flows receive the exact session the connection task waits on.
    // 宿主流程拿到的正是连接任务等待的那个会话。
    let first = runtime
        .auth_session_for_server("oauth-gate-http")
        .await
        .expect("known server")
        .expect("OAuth configured");
    let second = runtime
        .auth_session_for_server("oauth-gate-http")
        .await
        .expect("known server")
        .expect("OAuth configured");
    assert!(Arc::ptr_eq(&first, &second));

    // An authorization change wakes the task, which re-checks and keeps waiting without tokens.
    // 授权变化会唤醒任务，任务重新检查后在没有令牌时继续等待。
    first.remove_authorization().await.expect("remove");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let state = wait_for_state(&runtime, |state| {
        state.status == McpRuntimeServerStatus::AwaitingAuthorization
    })
    .await;
    assert_eq!(state.status, McpRuntimeServerStatus::AwaitingAuthorization);
    assert_eq!(state.retry_count, 0);

    runtime.shutdown();
    let stopped = wait_for_state(&runtime, |state| {
        state.status == McpRuntimeServerStatus::Stopped
    })
    .await;
    assert_eq!(stopped.status, McpRuntimeServerStatus::Stopped);
}

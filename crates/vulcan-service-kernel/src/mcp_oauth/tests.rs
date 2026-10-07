//! Sign-in flow registry behavior with offline sessions.
//! 使用离线会话的登录流程注册表行为测试。

use super::*;
use vulcan_agent_runtime::mcp::McpOAuthCredentialBackend;

/// Builds an offline session; construction contacts no server.
/// 构建离线会话；构建过程不访问任何服务器。
async fn session(server_id: &str) -> Arc<McpOAuthSession> {
    let backend: Arc<dyn McpOAuthCredentialBackend> =
        Arc::new(vulcan_agent_runtime::mcp::McpOAuthMemoryCredentialBackend::default());
    Arc::new(
        McpOAuthSession::new(
            server_id,
            "https://mcp.example.test/mcp",
            Default::default(),
            backend,
        )
        .await
        .expect("offline session"),
    )
}

/// Registers an awaiting manual flow for `server_id` with the given state and expiry.
/// 为 `server_id` 登记一个等待中的手动流程，使用给定 state 与过期时间。
async fn register(server_id: &str, flow_id: &str, oauth_state: &str, expires_at: Instant) {
    let flow = McpOAuthFlow {
        flow_id: flow_id.to_string(),
        session: session(server_id).await,
        oauth_state: oauth_state.to_string(),
        phase: McpOAuthFlowPhase::AwaitingCallback,
        callback_mode: McpOAuthCallbackMode::Manual,
        authorization_url: String::from("https://auth.example.test/authorize?state=s"),
        redirect_uri: String::from("https://app.example.test/callback"),
        error: None,
        expires_at,
        expires_at_unix_ms: 0,
        receiver_task: None,
    };
    flows().insert(server_id.to_string(), flow);
}

/// Builds a finish request.
/// 构建完成请求。
fn finish(server_id: &str, flow_id: &str, callback_url: &str) -> McpOAuthFinishRequest {
    McpOAuthFinishRequest {
        server_id: server_id.to_string(),
        flow_id: flow_id.to_string(),
        callback_url: callback_url.to_string(),
    }
}

/// Returns the phase of the retained flow.
/// 返回保留流程的阶段。
fn phase(server_id: &str) -> Option<McpOAuthFlowPhase> {
    flows().get(server_id).map(|flow| flow.phase)
}

#[tokio::test]
async fn claiming_requires_the_current_flow_and_its_state() {
    let server = "claim-checks";
    register(
        server,
        "flow-1",
        "state-1",
        Instant::now() + MCP_OAUTH_FLOW_TIMEOUT,
    )
    .await;

    let stale = finish(
        server,
        "flow-0",
        "https://app.example.test/callback?code=c&state=state-1",
    );
    assert!(claim_flow_for_exchange(&stale).is_err());
    let foreign = finish(
        server,
        "flow-1",
        "https://app.example.test/callback?code=c&state=other",
    );
    assert!(claim_flow_for_exchange(&foreign).is_err());
    assert_eq!(phase(server), Some(McpOAuthFlowPhase::AwaitingCallback));

    let matching = finish(
        server,
        "flow-1",
        "https://app.example.test/callback?code=c&state=state-1",
    );
    assert!(claim_flow_for_exchange(&matching).is_ok());
    assert_eq!(phase(server), Some(McpOAuthFlowPhase::Exchanging));
    // A second completion of the same flow cannot exchange the code again.
    // 同一流程的第二次完成不能再次交换授权码。
    assert!(claim_flow_for_exchange(&matching).is_err());
    discard_flow(server);
}

#[tokio::test]
async fn expired_flows_cannot_be_completed() {
    let server = "claim-expired";
    register(server, "flow-1", "state-1", Instant::now()).await;

    let request = finish(
        server,
        "flow-1",
        "https://app.example.test/callback?code=c&state=state-1",
    );

    assert!(claim_flow_for_exchange(&request).is_err());
    assert_eq!(phase(server), Some(McpOAuthFlowPhase::Expired));
    discard_flow(server);
}

#[tokio::test]
async fn updates_for_a_replaced_flow_are_ignored() {
    let server = "replaced-flow";
    register(
        server,
        "flow-2",
        "state-2",
        Instant::now() + MCP_OAUTH_FLOW_TIMEOUT,
    )
    .await;

    update_flow(server, "flow-1", |flow| {
        flow.phase = McpOAuthFlowPhase::Succeeded;
    });

    assert_eq!(phase(server), Some(McpOAuthFlowPhase::AwaitingCallback));
    discard_flow(server);
    assert_eq!(phase(server), None);
}

#[tokio::test]
async fn snapshots_use_stable_codes_and_hide_the_oauth_state() {
    let server = "snapshot-shape";
    register(
        server,
        "flow-1",
        "secret-state-value",
        Instant::now() + MCP_OAUTH_FLOW_TIMEOUT,
    )
    .await;

    let snapshot = flows()
        .get(server)
        .map(McpOAuthFlow::snapshot)
        .expect("flow");
    let json = serde_json::to_value(&snapshot).unwrap();

    assert_eq!(json["phase"], "awaiting_callback");
    assert_eq!(json["callbackMode"], "manual");
    assert!(json.get("flowId").is_some());
    assert!(!json.to_string().contains("secret-state-value"));
    discard_flow(server);
}

//! Offline behavior tests for the host-owned MCP OAuth session.
//! 宿主自有 MCP OAuth 会话的离线行为测试。

use super::*;

/// Server identifier shared by the fixtures.
/// 测试夹具共用的服务器标识。
const SERVER_ID: &str = "remote-docs";

/// Server URL shared by the fixtures.
/// 测试夹具共用的服务器 URL。
const SERVER_URL: &str = "https://mcp.example.test/mcp";

/// Builds a credential record without tokens for the given URL and client.
/// 为给定 URL 与客户端构建不含令牌的凭据记录。
fn record(server_url: &str, client_id: &str) -> McpOAuthCredentialRecord {
    McpOAuthCredentialRecord {
        server_url: server_url.to_string(),
        credentials: StoredCredentials::new(client_id.to_string(), None, Vec::new(), None),
    }
}

/// Builds a session over the given backend and spec.
/// 基于给定后端与配置构建会话。
async fn session(
    backend: Arc<dyn McpOAuthCredentialBackend>,
    spec: McpTransportOAuthSpec,
) -> McpOAuthSession {
    McpOAuthSession::new(SERVER_ID, SERVER_URL, spec, backend)
        .await
        .expect("session construction is offline")
}

#[test]
fn auth_status_serializes_as_snake_case_codes() {
    let codes = [
        McpAuthStatus::NotConfigured,
        McpAuthStatus::NotAuthenticated,
        McpAuthStatus::Authenticated,
        McpAuthStatus::Expired,
    ]
    .map(|status| serde_json::to_value(status).expect("status serializes"));
    for code in codes {
        let text = code.as_str().expect("status is a string code");
        assert!(
            text.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "unexpected status code {text}"
        );
    }
}

#[tokio::test]
async fn scoped_store_ignores_records_for_another_url_or_client() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let spec = McpTransportOAuthSpec {
        client_id: Some(String::from("configured-client")),
        ..McpTransportOAuthSpec::default()
    };
    let session = session(backend.clone(), spec).await;

    backend
        .save(
            SERVER_ID,
            &record("https://other.example.test/mcp", "configured-client"),
        )
        .unwrap();
    assert!(session.credential_store.load().await.unwrap().is_none());

    backend
        .save(SERVER_ID, &record(SERVER_URL, "stale-client"))
        .unwrap();
    assert!(session.credential_store.load().await.unwrap().is_none());

    backend
        .save(SERVER_ID, &record(SERVER_URL, "configured-client"))
        .unwrap();
    assert!(session.credential_store.load().await.unwrap().is_some());
}

#[tokio::test]
async fn dynamic_registration_accepts_any_stored_client() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend.clone(), McpTransportOAuthSpec::default()).await;
    backend
        .save(SERVER_ID, &record(SERVER_URL, "registered-client"))
        .unwrap();

    assert!(session.credential_store.load().await.unwrap().is_some());
}

#[tokio::test]
async fn saves_bind_the_record_to_the_configured_url() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend.clone(), McpTransportOAuthSpec::default()).await;

    session
        .credential_store
        .save(StoredCredentials::new(
            String::from("client"),
            None,
            Vec::new(),
            None,
        ))
        .await
        .unwrap();

    let stored = backend.load(SERVER_ID).unwrap().expect("record saved");
    assert_eq!(stored.server_url, SERVER_URL);
}

#[tokio::test]
async fn status_without_stored_tokens_is_not_authenticated() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend.clone(), McpTransportOAuthSpec::default()).await;
    assert_eq!(
        session.auth_status().await.unwrap(),
        McpAuthStatus::NotAuthenticated
    );

    // A record without a token response is still unauthenticated.
    // 不含令牌响应的记录仍是未认证。
    backend
        .save(SERVER_ID, &record(SERVER_URL, "client"))
        .unwrap();
    assert_eq!(
        session.auth_status().await.unwrap(),
        McpAuthStatus::NotAuthenticated
    );
}

#[tokio::test]
async fn removing_authorization_clears_the_backend_and_notifies_subscribers() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend.clone(), McpTransportOAuthSpec::default()).await;
    backend
        .save(SERVER_ID, &record(SERVER_URL, "client"))
        .unwrap();
    let changes = session.subscribe_authorization_changes();

    session.remove_authorization().await.unwrap();

    assert!(backend.load(SERVER_ID).unwrap().is_none());
    assert!(changes.has_changed().unwrap());
}

#[tokio::test]
async fn denied_callback_fails_without_contacting_the_server() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend, McpTransportOAuthSpec::default()).await;
    let changes = session.subscribe_authorization_changes();

    let error = session
        .finish_authorization(
            "http://127.0.0.1:4000/callback?error=access_denied&error_description=User+declined&state=s",
        )
        .await
        .expect_err("a denied callback cannot complete authorization");

    assert!(error.to_string().contains("access_denied"));
    assert!(!changes.has_changed().unwrap());
}

#[tokio::test]
async fn callback_without_code_is_rejected_as_invalid_input() {
    let backend = Arc::new(McpOAuthMemoryCredentialBackend::default());
    let session = session(backend, McpTransportOAuthSpec::default()).await;

    let error = session
        .finish_authorization("http://127.0.0.1:4000/callback?state=s")
        .await
        .expect_err("a callback without code cannot complete authorization");

    assert!(matches!(error, McpRuntimeError::InvalidArgument(_)));
}

#[test]
fn extract_oauth_state_reads_query_parameter() {
    let state = extract_oauth_state(
        "https://auth.example.test/authorize?client_id=demo&state=oauth-state-123",
    )
    .expect("state should be extracted from the authorization URL");

    assert_eq!(state, "oauth-state-123");
}

#[test]
fn debug_output_never_includes_the_client_secret() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let session = runtime.block_on(session(
        Arc::new(McpOAuthMemoryCredentialBackend::default()),
        McpTransportOAuthSpec {
            client_id: Some(String::from("client")),
            client_secret: Some(String::from("very-secret-value")),
            ..McpTransportOAuthSpec::default()
        },
    ));

    assert!(!format!("{session:?}").contains("very-secret-value"));
}

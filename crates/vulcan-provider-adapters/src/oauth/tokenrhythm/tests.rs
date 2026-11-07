//! Behavior of TokenRhythm login against synthetic local servers only.
//! 仅针对本地合成服务验证 TokenRhythm 登录行为。

use super::*;
use crate::open_design::test_server;

/// Builds a synthetic token response; `session` lets refresh tests change identity.
/// 构建合成令牌响应；`session` 供刷新测试改变身份。
fn token_body(scope: &str, session: &str) -> String {
    serde_json::json!({
        "token_type": "Bearer",
        "expires_in": 3600,
        "access_token": "synthetic-access",
        "refresh_token": "synthetic-refresh-next",
        "scope": scope,
        "account_id": "acct_demo",
        "session_id": session,
    })
    .to_string()
}

/// Returns the full requested scope list.
/// 返回完整的请求权限范围。
fn all_scopes() -> String {
    SCOPES.join(" ")
}

/// Builds a synthetic managed-key envelope for `account` and `app`.
/// 为 `account` 与 `app` 构建合成托管密钥封装。
fn key_body(account: &str, app: &str) -> String {
    serde_json::json!({
        "code": 0,
        "data": {"secret": "sk_synthetic", "key_id": "key_1", "generation": 1,
                 "account_id": account, "app_id": app, "status": "active"}
    })
    .to_string()
}

/// Wraps a request head as raw loopback bytes.
/// 将请求行包装为原始环回字节。
fn request(line: &str) -> Vec<u8> {
    format!("{line}\r\nHost: 127.0.0.1\r\n\r\n").into_bytes()
}

#[test]
fn callback_accepts_only_this_transaction() {
    assert!(matches!(
        parse_callback(&request("GET /oauth/callback?code=abc&state=s1 HTTP/1.1"), "s1"),
        Some(Callback::Code(code)) if code == "abc"
    ));
    assert!(matches!(
        parse_callback(
            &request("GET /oauth/callback?error=access_denied&state=s1 HTTP/1.1"),
            "s1"
        ),
        Some(Callback::Denied)
    ));
    for line in [
        "GET /oauth/callback?code=abc&state=other HTTP/1.1",
        "GET /favicon.ico HTTP/1.1",
        "POST /oauth/callback?code=abc&state=s1 HTTP/1.1",
        "GET /oauth/callback?code=a&code=b&state=s1 HTTP/1.1",
        "GET /oauth/callback?code=a&state=s1&state=s1 HTTP/1.1",
        "GET /oauth/callback?code=a&error=x&state=s1 HTTP/1.1",
    ] {
        assert!(parse_callback(&request(line), "s1").is_none(), "{line}");
    }
}

#[test]
fn token_response_must_grant_every_scope() {
    let token: TokenResponse =
        serde_json::from_str(&token_body("profile.read wallet.read", "sess_1")).unwrap();
    assert!(token_set(token, CLIENT_ID).is_err());
    let token: TokenResponse = serde_json::from_str(&token_body(&all_scopes(), "sess_1")).unwrap();
    let (tokens, session) = token_set(token, CLIENT_ID).unwrap();
    assert_eq!(tokens.account_subject.as_deref(), Some("acct_demo"));
    assert_eq!(session, "sess_1");
}

#[test]
fn token_response_rejects_another_client_or_scheme() {
    let mut body: serde_json::Value =
        serde_json::from_str(&token_body(&all_scopes(), "sess_1")).unwrap();
    body["client_id"] = serde_json::json!("someone-else");
    let token: TokenResponse = serde_json::from_value(body.clone()).unwrap();
    assert!(token_set(token, CLIENT_ID).is_err());
    body["client_id"] = serde_json::Value::Null;
    body["token_type"] = serde_json::json!("Basic");
    let token: TokenResponse = serde_json::from_value(body).unwrap();
    assert!(token_set(token, CLIENT_ID).is_err());
}

#[tokio::test]
async fn login_exchanges_the_code_then_obtains_the_managed_key() {
    let (origin, server) = test_server(vec![
        (200, token_body(&all_scopes(), "sess_1"), Duration::ZERO),
        (200, key_body("acct_demo", INTEGRATION_APP), Duration::ZERO),
    ])
    .await;
    let material = complete_login(
        &crate::tokenrhythm::client().unwrap(),
        &origin,
        "code_1",
        "http://127.0.0.1:1/oauth/callback",
        "verifier_1",
    )
    .await
    .unwrap();
    let CredentialSecretMaterial::TokenRhythmAuth {
        tokens,
        session_id,
        client_id,
        managed_key,
    } = material
    else {
        panic!("unexpected material kind");
    };
    assert_eq!(managed_key, "sk_synthetic");
    assert_eq!(session_id, "sess_1");
    assert_eq!(client_id, CLIENT_ID);
    assert_eq!(tokens.access_token, "synthetic-access");
    let requests = server.await.unwrap();
    assert!(requests[0].starts_with("POST /api/oauth/token "));
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(requests[0].contains("code_verifier=verifier_1"));
    assert!(requests[1].starts_with(&format!(
        "POST /api/v1/integrations/{INTEGRATION_APP}/managed-key/credential "
    )));
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-access")
    );
}

#[tokio::test]
async fn login_rejects_a_key_of_another_application() {
    let (origin, _server) = test_server(vec![
        (200, token_body(&all_scopes(), "sess_1"), Duration::ZERO),
        (200, key_body("acct_demo", "other-app"), Duration::ZERO),
    ])
    .await;
    let result = complete_login(
        &crate::tokenrhythm::client().unwrap(),
        &origin,
        "code_1",
        "http://127.0.0.1:1/oauth/callback",
        "verifier_1",
    )
    .await;
    assert!(result.is_err());
}

/// Builds stored material whose session is `sess_1`.
/// 构建会话为 `sess_1` 的已存储材料。
fn stored_material() -> CredentialSecretMaterial {
    CredentialSecretMaterial::TokenRhythmAuth {
        tokens: OAuthTokenSet {
            access_token: String::from("old-access"),
            refresh_token: String::from("old-refresh"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: SCOPES.iter().map(|scope| scope.to_string()).collect(),
            account_subject: Some(String::from("acct_demo")),
            email: None,
        },
        session_id: String::from("sess_1"),
        client_id: String::from("stored-client"),
        managed_key: String::from("sk_kept"),
    }
}

#[tokio::test]
async fn refresh_keeps_the_key_and_uses_the_stored_client() {
    let (origin, server) = test_server(vec![(
        200,
        token_body(&all_scopes(), "sess_1"),
        Duration::ZERO,
    )])
    .await;
    let refreshed = refresh_at(&stored_material(), &origin).await.unwrap();
    let CredentialSecretMaterial::TokenRhythmAuth {
        tokens,
        managed_key,
        client_id,
        ..
    } = refreshed
    else {
        panic!("unexpected material kind");
    };
    assert_eq!(managed_key, "sk_kept");
    assert_eq!(client_id, "stored-client");
    assert_eq!(tokens.refresh_token, "synthetic-refresh-next");
    let requests = server.await.unwrap();
    assert!(requests[0].contains("client_id=stored-client"));
    assert!(requests[0].contains("refresh_token=old-refresh"));
}

#[tokio::test]
async fn refresh_rejects_a_changed_session() {
    let (origin, _server) = test_server(vec![(
        200,
        token_body(&all_scopes(), "sess_2"),
        Duration::ZERO,
    )])
    .await;
    assert!(refresh_at(&stored_material(), &origin).await.is_err());
}

#[tokio::test]
async fn refresh_reports_an_unusable_grant() {
    let (origin, _server) = test_server(vec![(
        400,
        String::from(r#"{"error":"invalid_grant"}"#),
        Duration::ZERO,
    )])
    .await;
    assert!(refresh_at(&stored_material(), &origin).await.is_err());
}

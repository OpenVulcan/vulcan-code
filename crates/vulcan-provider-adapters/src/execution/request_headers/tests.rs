//! Regression coverage for header ownership and native protocol identity.
//! 请求头归属与原生协议身份的回归覆盖。

use super::super::ProviderExecutionClient;
use super::super::test_support::{
    bundled_manifest_route, minimal_request, pinned_openrouter_route,
};
use super::*;
use std::collections::BTreeSet;
use std::sync::Arc;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID, ProtocolRegistry,
};

/// Verifies protocol-required headers are merged while credential headers remain adapter-owned.
/// 验证协议必需请求头会被合并，同时凭据请求头仍由适配器独占管理。
#[test]
fn execution_headers_merge_protocol_headers_and_reject_credential_overrides() {
    let route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let encoded = client
        .encode_request(&route, &minimal_request())
        .expect("Anthropic request should encode");
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let headers = execution_headers(&route, &encoded.headers, &secret)
        .expect("non-secret protocol headers should merge");
    assert_eq!(
        headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok()),
        Some("2023-06-01")
    );
    assert_eq!(
        headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer secret")
    );
    assert_eq!(
        headers
            .get(reqwest::header::ACCEPT_ENCODING)
            .and_then(|value| value.to_str().ok()),
        Some("identity")
    );
    assert_eq!(
        headers
            .get(reqwest::header::USER_AGENT)
            .and_then(|value| value.to_str().ok()),
        Some(VULCAN_CODE_USER_AGENT)
    );
    assert_eq!(
        headers
            .get("http-referer")
            .and_then(|value| value.to_str().ok()),
        Some("https://code.openvulcan.com")
    );
    assert_eq!(
        headers
            .get("x-openrouter-title")
            .and_then(|value| value.to_str().ok()),
        Some("Vulcan Code")
    );
    assert_eq!(
        headers
            .get("x-openrouter-categories")
            .and_then(|value| value.to_str().ok()),
        Some("programming-app")
    );
    assert!(!encoded.headers.contains_key("originator"));
    assert!(!encoded.headers.contains_key("user-agent"));

    let forbidden = BTreeMap::from([(String::from("authorization"), String::from("Bearer other"))]);
    assert!(execution_headers(&route, &forbidden, &secret).is_err());
}

/// Verifies only the native Codex protocol preserves the exact adopted Codex user agent.
/// 验证仅原生 Codex 协议保留精确的已采纳 Codex 用户代理。
#[test]
fn native_codex_protocol_owns_exact_codex_user_agent() {
    // route freezes the official Codex subscription and native protocol identity.
    // route 冻结官方 Codex 订阅与原生协议身份。
    let route = bundled_manifest_route(
        "openai",
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    );
    // client executes the same canonical encoder used by production dispatch.
    // client 执行与生产分派相同的规范编码器。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // encoded carries the protocol-owned Codex identity before credential projection.
    // encoded 在凭据投影前携带协议持有的 Codex 身份。
    let encoded = client
        .encode_request(&route, &minimal_request())
        .expect("native Codex request should encode");
    // secret models one valid Codex OAuth credential with its required account identity.
    // secret 模拟一个包含必要账号身份的有效 Codex OAuth 凭据。
    let secret =
        crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenAiCodexOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: String::from("access-token"),
                refresh_token: String::from("refresh-token"),
                issued_at_unix_ms: 1,
                expires_at_unix_ms: 2,
                scopes: BTreeSet::new(),
                account_subject: None,
                email: None,
            },
            id_token: None,
            account_id: Some(String::from("account-id")),
        })
        .expect("Codex credential should validate")
        .execution_secret()
        .expect("Codex credential should resolve");
    // headers are the final HTTP headers used by both HTTP and WebSocket transports.
    // headers 是 HTTP 与 WebSocket 传输共同使用的最终 HTTP 请求头。
    let headers = execution_headers(&route, &encoded.headers, &secret)
        .expect("native Codex headers should resolve");

    assert_eq!(
        headers
            .get(reqwest::header::USER_AGENT)
            .and_then(|value| value.to_str().ok()),
        Some(CODEX_RESPONSES_USER_AGENT)
    );
    assert_eq!(
        headers
            .get("originator")
            .and_then(|value| value.to_str().ok()),
        Some("codex_cli_rs")
    );
}

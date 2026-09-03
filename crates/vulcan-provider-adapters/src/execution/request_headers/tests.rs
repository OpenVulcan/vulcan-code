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
    let headers = execution_headers(&route, &encoded.headers, &secret, None)
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
    assert!(execution_headers(&route, &forbidden, &secret, None).is_err());
}

/// Verifies a configured session header receives only the current request session scope.
/// 验证已配置的会话请求头只接收当前请求的会话作用域。
#[test]
fn configured_session_header_projects_current_session_scope() {
    let route = bundled_manifest_route("opencode-go", "go-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("session-123"));
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let encoded = client
        .encode_request(&route, &request)
        .expect("session-header request should encode");
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");

    let headers = execution_headers(
        &route,
        &encoded.headers,
        &secret,
        request.cache_scope_id.as_deref(),
    )
    .expect("configured session header should resolve");
    assert_eq!(
        headers
            .get("x-opencode-session")
            .and_then(|value| value.to_str().ok()),
        Some("session-123")
    );

    let without_session = execution_headers(&route, &encoded.headers, &secret, None)
        .expect("missing optional session scope should remain valid");
    assert!(!without_session.contains_key("x-opencode-session"));

    let mut unconfigured_route = route;
    unconfigured_route.entry.session_header = None;
    let without_configuration = execution_headers(
        &unconfigured_route,
        &encoded.headers,
        &secret,
        request.cache_scope_id.as_deref(),
    )
    .expect("unconfigured session header should remain absent");
    assert!(!without_configuration.contains_key("x-opencode-session"));
}

/// Verifies session-header projection never replaces an existing or host-owned header.
/// 验证会话请求头投影绝不会替换已有请求头或宿主持有的请求头。
#[test]
fn session_header_never_overrides_existing_or_host_owned_headers() {
    // fixed_route contains a provider-owned header with the same name as the session projection.
    // fixed_route 包含与会话投影同名的供应商固定请求头。
    let mut fixed_route =
        bundled_manifest_route("opencode-go", "go-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    fixed_route.entry.request_headers.insert(
        String::from("x-opencode-session"),
        String::from("fixed-session"),
    );
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("session-123"));
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let encoded = client
        .encode_request(&fixed_route, &request)
        .expect("fixed-header request should encode");
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let headers = execution_headers(
        &fixed_route,
        &encoded.headers,
        &secret,
        request.cache_scope_id.as_deref(),
    )
    .expect("existing fixed header should remain valid");
    assert_eq!(
        headers
            .get("x-opencode-session")
            .and_then(|value| value.to_str().ok()),
        Some("fixed-session")
    );

    // host_owned_route proves host-owned identity remains authoritative even without a fixed entry header.
    // host_owned_route 证明即使没有固定入口请求头，宿主持有的身份请求头仍然权威。
    let mut host_owned_route =
        bundled_manifest_route("opencode-go", "go-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    host_owned_route.entry.session_header = Some(String::from("user-agent"));
    let host_owned_encoded = client
        .encode_request(&host_owned_route, &request)
        .expect("host-owned session-header request should encode");
    let host_owned_headers = execution_headers(
        &host_owned_route,
        &host_owned_encoded.headers,
        &secret,
        request.cache_scope_id.as_deref(),
    )
    .expect("host-owned header should remain valid");
    assert_eq!(
        host_owned_headers
            .get(reqwest::header::USER_AGENT)
            .and_then(|value| value.to_str().ok()),
        Some(VULCAN_CODE_USER_AGENT)
    );

    // A defensive runtime route-kind check keeps malformed provider-tool routes from receiving session scope.
    // 防御性运行时入口类型校验可避免异常 Provider Tools 路由接收会话作用域。
    let mut provider_tools_route =
        bundled_manifest_route("opencode-go", "go-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    provider_tools_route.entry.kind = crate::ServiceEntryKind::ProviderTools;
    let provider_tools_encoded = client
        .encode_request(&provider_tools_route, &request)
        .expect("provider-tools route should still encode for the runtime guard test");
    let provider_tools_headers = execution_headers(
        &provider_tools_route,
        &provider_tools_encoded.headers,
        &secret,
        request.cache_scope_id.as_deref(),
    )
    .expect("provider-tools route should remain valid without session projection");
    assert!(!provider_tools_headers.contains_key("x-opencode-session"));
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
    let headers = execution_headers(&route, &encoded.headers, &secret, None)
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

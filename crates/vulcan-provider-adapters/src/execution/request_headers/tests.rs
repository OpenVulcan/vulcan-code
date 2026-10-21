//! Regression coverage for header ownership and native protocol identity.
//! 请求头归属与原生协议身份的回归覆盖。

use super::super::ProviderExecutionClient;
use super::super::test_support::{
    bundled_manifest_route, minimal_request, pinned_openrouter_route,
};
use super::*;
use std::collections::BTreeSet;
use std::sync::Arc;
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID, ProtocolRegistry,
};

/// Verifies Console OAuth uses Vulcan identity, exact workspace headers, and the declared authority.
/// 验证 Console OAuth 使用 Vulcan 身份、精确工作区请求头与声明授权端。
#[test]
fn opencode_oauth_headers_preserve_client_and_workspace_identity() {
    // secret contains a workspace distinct from the authenticated human user.
    // secret 包含与已鉴权自然人用户不同的工作区。
    let secret = crate::CredentialSecretEnvelope::new(
        crate::CredentialSecretMaterial::OpencodeConsoleOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: String::from("synthetic-access"),
                refresh_token: String::from("synthetic-refresh"),
                issued_at_unix_ms: 1,
                expires_at_unix_ms: 2,
                scopes: BTreeSet::new(),
                account_subject: Some(String::from("user_demo")),
                email: None,
            },
            organization_id: String::from("org_demo"),
        },
    )
    .unwrap()
    .execution_secret()
    .unwrap();
    // protocol and expected_url pin the published Go paths rather than the pay-as-you-go gateway.
    // protocol 与 expected_url 固定已公布的 Go 路径，而非按量计费网关。
    for (protocol, expected_url) in [
        (
            "openai-chat-completions.v1",
            "https://opencode.ai/inference/go/openai/v1/chat/completions",
        ),
        (
            "openai-responses.v1",
            "https://opencode.ai/inference/go/openai/v1/responses",
        ),
        (
            "anthropic-messages.v1",
            "https://opencode.ai/inference/go/anthropic/v1/messages",
        ),
    ] {
        // route comes from the actual OAuth service entry rather than a duplicated test descriptor.
        // route 来自真实 OAuth 服务入口，而非重复的测试描述符。
        let mut route = bundled_manifest_route("opencode-go", "go-api", protocol);
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().unwrap()),
        );
        let encoded = client.encode_request(&route, &minimal_request()).unwrap();
        assert_eq!(route.request_url(encoded.path.as_str()), expected_url);
        let headers =
            execution_headers(&route, &encoded.headers, &secret, Some("vulcan-session")).unwrap();
        assert_eq!(headers["user-agent"], VULCAN_CODE_USER_AGENT);
        assert_eq!(headers["x-opencode-session"], "vulcan-session");
        assert_eq!(headers["x-org-id"], "org_demo");
        assert_eq!(headers["authorization"], "Bearer synthetic-access");
        assert!(!headers.contains_key("chatgpt-account-id"));
        // Even the same-origin pay-as-you-go base must not replace the selected Go product.
        // 即使同源的按量计费基础地址，也不得替换已选择的 Go 产品。
        route.entry.base_url = String::from("https://opencode.ai/inference");
        assert!(execution_headers(&route, &encoded.headers, &secret, None).is_err());
        // A changed destination must fail before any private token is dispatched.
        // 目的地址改变时，必须在分派任何私有令牌前失败。
        route.entry.base_url = String::from("https://example.invalid/inference");
        assert!(execution_headers(&route, &encoded.headers, &secret, None).is_err());
    }
}

/// Verifies the Console API-key profile preserves Vulcan identity without adding OAuth workspace headers.
/// 验证 Console API Key 凭据档保留 Vulcan 身份，且不添加 OAuth 工作区请求头。
/// No parameters; checks every registered inference protocol using the compiled service declaration.
/// 无参数；使用已编译服务声明检查全部已注册推理协议。
#[test]
fn opencode_api_key_headers_preserve_client_identity_without_workspace() {
    // secret is synthetic static material rather than an OAuth credential with a workspace.
    // secret 是合成静态材料，而非带工作区的 OAuth 凭据。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("oc_sk_synthetic"))
        .unwrap()
        .execution_secret()
        .unwrap();
    // protocol and expected_url keep key-based inference on the same declared Go product paths.
    // protocol 与 expected_url 将密钥推理保留在相同声明的 Go 产品路径上。
    for (protocol, expected_url) in [
        (
            "openai-chat-completions.v1",
            "https://opencode.ai/inference/go/openai/v1/chat/completions",
        ),
        (
            "openai-responses.v1",
            "https://opencode.ai/inference/go/openai/v1/responses",
        ),
        (
            "anthropic-messages.v1",
            "https://opencode.ai/inference/go/anthropic/v1/messages",
        ),
    ] {
        // route retains the shared Console endpoint and selects its explicitly declared key profile.
        // route 保留共享 Console 端点，并选择其明确声明的密钥凭据档。
        let mut route = bundled_manifest_route("opencode-go", "go-api", protocol);
        // profile comes from the compiled manifest, avoiding a duplicated credential declaration.
        // profile 来自已编译清单，避免重复定义凭据声明。
        let profile = route
            .entry
            .credential_profile("api-key")
            .expect("expected Go API-key profile may be outdated");
        route.connection.credential_profile_id = profile.id.clone();
        route.connection.credential_mode = profile.credential_mode;
        // client applies the same request encoding and header projection as live inference.
        // client 应用与在线推理相同的请求编码及请求头投影。
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().unwrap()),
        );
        // encoded keeps protocol-owned headers in the normal execution path.
        // encoded 将协议持有的请求头保留在正常执行路径中。
        let encoded = client.encode_request(&route, &minimal_request()).unwrap();
        assert_eq!(route.request_url(encoded.path.as_str()), expected_url);
        // headers must use the key and public application/session identities, with no organization guess.
        // headers 必须使用密钥及公开应用和会话身份，不猜测组织。
        let headers =
            execution_headers(&route, &encoded.headers, &secret, Some("vulcan-session")).unwrap();
        assert_eq!(headers["user-agent"], VULCAN_CODE_USER_AGENT);
        assert_eq!(headers["x-opencode-session"], "vulcan-session");
        assert_eq!(headers["authorization"], "Bearer oc_sk_synthetic");
        assert!(!headers.contains_key("x-org-id"));
        assert!(!headers.contains_key("chatgpt-account-id"));
    }
}

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
        Some(VULCAN_CODE_USER_AGENT)
    );
    assert_eq!(
        headers
            .get("originator")
            .and_then(|value| value.to_str().ok()),
        Some(vulcan_llm_core::VULCAN_CODE_CLIENT_NAME)
    );
    assert!(!headers.contains_key(CODEX_RESPONSES_LITE_HEADER_NAME));

    // lite_route freezes the same native route with the adopted Lite model dialect.
    // lite_route 在同一原生路由上冻结已采纳的 Lite 模型方言。
    let mut lite_route = route;
    lite_route.codex_behavior = Some(crate::provider_manifest::CodexModelBehavior {
        wire_dialect: crate::provider_manifest::CodexWireDialect::Lite,
        ..Default::default()
    });
    // lite_headers prove the common HTTP header map also feeds WebSocket and compact requests.
    // lite_headers 证明公共 HTTP 请求头映射同时供 WebSocket 与压缩请求使用。
    let lite_headers = execution_headers(&lite_route, &encoded.headers, &secret, None)
        .expect("native Codex Lite headers should resolve");
    assert_eq!(
        lite_headers
            .get(CODEX_RESPONSES_LITE_HEADER_NAME)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    // websocket_map is the exact secret-safe projection used by the WebSocket handshake.
    // websocket_map 是 WebSocket 握手使用的精确不泄密投影。
    let websocket_map =
        websocket_headers(&lite_headers).expect("Lite WebSocket headers should project");
    assert_eq!(
        websocket_map
            .get(CODEX_RESPONSES_LITE_HEADER_NAME)
            .map(String::as_str),
        Some("true")
    );

    // spoofed_route proves provider-owned declarations cannot replace the host selector.
    // spoofed_route 证明供应商声明不能替换宿主选择头。
    let mut spoofed_route = lite_route;
    spoofed_route.entry.request_headers.insert(
        String::from(CODEX_RESPONSES_LITE_HEADER_NAME),
        String::from("false"),
    );
    assert!(execution_headers(&spoofed_route, &encoded.headers, &secret, None).is_err());
}

/// Verifies Kimi receives the provider-observed operating-system label.
/// 验证 Kimi 收到供应商已验证的操作系统标签。
#[test]
fn kimi_device_model_uses_provider_operating_system_label() {
    let expected_operating_system = match std::env::consts::OS {
        "darwin" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    assert_eq!(
        coding_device_model(),
        format!("{expected_operating_system} {}", std::env::consts::ARCH)
    );
}

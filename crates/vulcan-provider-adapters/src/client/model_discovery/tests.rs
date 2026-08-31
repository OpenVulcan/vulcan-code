//! Regression tests for model_discovery.
//! model_discovery 的回归测试。

use super::super::{ProviderMetadataClient, route_is_available_for_discovery};
use super::*;
use crate::{
    CredentialMode, CredentialSecretEnvelope, CustomProviderAuthentication,
    CustomProviderDefinition, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
    ModelCapabilityEvidence,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Starts one local HTTP fixture that captures the request and returns one JSON response.
/// 启动一个本地 HTTP 夹具，捕获请求并返回一项 JSON 响应。
///
/// # Parameters
/// 参数
/// `status` is the complete HTTP status text and `body` is the exact response body.
/// `status` 是完整 HTTP 状态文本，`body` 是精确响应正文。
///
/// # Returns
/// 返回值
/// The server origin, captured-request receiver, and bounded server task.
/// 返回服务源地址、请求捕获接收器与有界服务任务。
async fn start_metadata_fixture(
    status: &str,
    body: &str,
) -> (String, oneshot::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local metadata fixture should bind");
    let address = listener
        .local_addr()
        .expect("local metadata fixture address");
    let status = status.to_string();
    let body = body.to_string();
    let (request_sender, request_receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("metadata fixture should receive one request");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream
                .read(&mut buffer)
                .await
                .expect("metadata fixture should read request headers");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let _ = request_sender.send(String::from_utf8_lossy(&request).into_owned());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("metadata fixture should write response");
    });
    (format!("http://{address}"), request_receiver, task)
}

/// Starts one local HTTP fixture that returns a bounded sequence of paginated responses.
/// 启动一个返回有界分页响应序列的本地 HTTP 夹具。
///
/// # Parameters
/// 参数
/// `responses` contains exact HTTP status text and response bodies in request order.
/// `responses` 按请求顺序包含精确 HTTP 状态文本和响应正文。
///
/// # Returns
/// 返回值
/// The server origin, captured request list, and bounded server task.
/// 返回服务源地址、已捕获请求列表与有界服务任务。
async fn start_metadata_sequence_fixture(
    responses: Vec<(&str, &str)>,
) -> (String, oneshot::Receiver<Vec<String>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local metadata sequence fixture should bind");
    let address = listener
        .local_addr()
        .expect("local metadata sequence fixture address");
    let responses = responses
        .into_iter()
        .map(|(status, body)| (status.to_string(), body.to_string()))
        .collect::<Vec<_>>();
    let (request_sender, request_receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut requests = Vec::with_capacity(responses.len());
        for (status, body) in responses {
            let Ok(accepted) =
                tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept()).await
            else {
                break;
            };
            let (mut stream, _) =
                accepted.expect("metadata sequence fixture should receive one request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .expect("metadata sequence fixture should read request headers");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            requests.push(String::from_utf8_lossy(&request).into_owned());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("metadata sequence fixture should write response");
        }
        let _ = request_sender.send(requests);
    });
    (format!("http://{address}"), request_receiver, task)
}

/// Builds one exact custom OpenAI-compatible connection for HTTP discovery tests.
/// 为 HTTP 发现测试构建一个精确的自定义 OpenAI 兼容连接。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact custom provider identity and `base_url` is the
/// versioned endpoint base URL used by the local fixture.
/// `provider_id` 是精确自定义供应商身份，`base_url` 是本地夹具使用的
/// 带版本端点基础 URL。
///
/// # Returns
/// 返回值
/// A validated descriptor and matching credential connection.
/// 返回已验证描述及匹配的凭据连接。
fn custom_openai_fixture(
    provider_id: &str,
    base_url: &str,
) -> (crate::ProviderDescriptor, CredentialConnection) {
    let provider = CustomProviderDefinition {
        id: provider_id.to_string(),
        display_name: String::from("Local Fixture"),
        base_url: base_url.to_string(),
        protocols: vec![CustomProviderProtocol::OpenAiChat],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
    }
    .into_descriptor()
    .expect("custom provider fixture should project");
    let connection = CredentialConnection {
        id: String::from("connection"),
        provider_id: provider_id.to_string(),
        service_entry_id: String::from("default"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential"),
        display_name: String::from("Fixture account"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    (provider, connection)
}

/// Builds one exact custom Anthropic-compatible connection for pagination tests.
/// 为分页测试构建一个精确的自定义 Anthropic 兼容连接。
///
/// # Parameters
/// 参数
/// `provider_id` and `base_url` fix identity and endpoint; `model_discovery_path` selects an optional same-origin catalog path.
/// `provider_id` 与 `base_url` 固定身份和端点；`model_discovery_path` 选择可选的同源目录路径。
///
/// # Returns
/// 返回值
/// A validated descriptor and matching credential connection.
/// 返回已验证描述及匹配的凭据连接。
fn custom_anthropic_fixture(
    provider_id: &str,
    base_url: &str,
    model_discovery_path: Option<&str>,
) -> (crate::ProviderDescriptor, CredentialConnection) {
    let provider = CustomProviderDefinition {
        id: provider_id.to_string(),
        display_name: String::from("Local Anthropic Fixture"),
        base_url: base_url.to_string(),
        protocols: vec![CustomProviderProtocol::AnthropicMessages],
        authentication: CustomProviderAuthentication::XApiKey,
        model_discovery_mode: if model_discovery_path.is_some() {
            CustomProviderModelDiscoveryMode::CustomPath
        } else {
            CustomProviderModelDiscoveryMode::Standard
        },
        model_discovery_path: model_discovery_path.map(str::to_string),
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
    }
    .into_descriptor()
    .expect("custom Anthropic provider fixture should project");
    let connection = CredentialConnection {
        id: String::from("connection"),
        provider_id: provider_id.to_string(),
        service_entry_id: String::from("default"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential"),
        display_name: String::from("Fixture account"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    (provider, connection)
}

/// Runs one complete custom Anthropic discovery against a bounded local page sequence.
/// 针对有界本地分页序列运行一次完整自定义 Anthropic 发现。
///
/// # Parameters
/// 参数
/// `responses` contains exact ordered HTTP pages.
/// `responses` 包含精确有序 HTTP 分页。
///
/// # Returns
/// 返回值
/// The typed discovery result and every captured request.
/// 返回强类型发现结果与全部已捕获请求。
async fn discover_custom_anthropic_pages(
    responses: Vec<(&str, &str)>,
) -> (
    Result<crate::ConnectionModelCatalog, super::ModelDiscoveryError>,
    Vec<String>,
) {
    let (origin, request_receiver, server_task) = start_metadata_sequence_fixture(responses).await;
    let (provider, connection) = custom_anthropic_fixture(
        "custom:anthropic-fixture",
        format!("{origin}/v1").as_str(),
        None,
    );
    let entry = provider
        .service_entries
        .first()
        .expect("custom Anthropic provider entry");
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");
    let result = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, entry, &connection, &secret, None, 1)
        .await;
    let requests = request_receiver
        .await
        .expect("metadata sequence fixture should publish requests");
    server_task
        .await
        .expect("metadata sequence fixture should finish");
    (result, requests)
}

/// Builds one minimal model-catalog record for pagination helper tests.
/// 为分页辅助函数测试构建一个最小模型目录记录。
fn pagination_model(model_id: &str) -> ConnectionModel {
    ConnectionModel {
        id: model_id.to_string(),
        display_name: model_id.to_string(),
        capabilities: ModelCapabilities::default(),
        capability_evidence: ModelCapabilityEvidence::default(),
        protocol_variants: BTreeMap::new(),
        resolved_protocol_binding: None,
    }
}

/// Verifies duplicate identities reject the complete incoming page without partial mutation.
/// 验证重复标识会拒绝完整的新分页且不会发生部分变更。
#[test]
fn paginated_model_append_is_atomic_and_unique() {
    let mut target = vec![pagination_model("existing")];
    let mut seen = BTreeSet::from([String::from("existing")]);
    let error = append_unique_discovered_models(
        &mut target,
        &mut seen,
        vec![pagination_model("new"), pagination_model("existing")],
        "Test discovery",
    )
    .expect_err("duplicate model must reject the complete page");

    assert!(error.message.contains("duplicate model `existing`"));
    assert_eq!(target.len(), 1);
    assert!(!seen.contains("new"));
}

/// Verifies non-adjacent cursor cycles are rejected deterministically.
/// 验证非相邻游标循环会被确定性拒绝。
#[test]
fn pagination_cursor_registry_rejects_non_adjacent_cycles() {
    let mut seen = BTreeSet::new();
    assert_eq!(
        register_pagination_cursor(&mut seen, String::from("A"), "Test discovery")
            .expect("first cursor must register"),
        "A"
    );
    register_pagination_cursor(&mut seen, String::from("B"), "Test discovery")
        .expect("second cursor must register");
    let error = register_pagination_cursor(&mut seen, String::from("A"), "Test discovery")
        .expect_err("a non-adjacent cursor cycle must fail");
    assert!(error.message.contains("repeated pagination cursor `A`"));
}

/// Verifies a provider cannot continue beyond the explicit model-catalog page boundary.
/// 验证供应商不能在明确模型目录分页边界之后继续。
#[test]
fn pagination_boundary_rejects_unbounded_unique_cursors() {
    assert!(validate_pagination_boundary(1_023, true, "Test discovery").is_ok());
    assert!(validate_pagination_boundary(1_024, false, "Test discovery").is_ok());
    // error proves a unique-cursor stream still terminates at the fixed safety boundary.
    // error 证明即使游标始终唯一，数据流也会在固定安全边界终止。
    let error = validate_pagination_boundary(1_024, true, "Test discovery")
        .expect_err("continued traversal at the boundary must fail");
    assert!(error.message.contains("1024-page safety boundary"));
}

/// Verifies non-paginated catalogs cannot bypass aggregate model and identity validation.
/// 验证非分页目录无法绕过聚合模型与身份校验。
#[test]
fn discovered_catalog_bounds_reject_duplicate_and_oversized_identities() {
    let duplicate = ConnectionModelCatalog {
        connection_id: String::from("account"),
        models: vec![
            ConnectionModel {
                id: String::from("same"),
                display_name: String::from("One"),
                capabilities: ModelCapabilities::default(),
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            },
            ConnectionModel {
                id: String::from("same"),
                display_name: String::from("Two"),
                capabilities: ModelCapabilities::default(),
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            },
        ],
        generation: 1,
        page_count: 1,
        discovery_schema_revision: String::from("test"),
    };
    let duplicate_error = validate_discovered_catalog_bounds(duplicate, "account")
        .expect_err("duplicate identities must fail");
    assert!(
        duplicate_error
            .message
            .contains("duplicate model identifier")
    );

    let oversized = ConnectionModelCatalog {
        connection_id: String::from("account"),
        models: vec![ConnectionModel {
            id: "x".repeat(PROVIDER_MODEL_IDENTIFIER_MAX_BYTES + 1),
            display_name: String::from("Model"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        }],
        generation: 1,
        page_count: 1,
        discovery_schema_revision: String::from("test"),
    };
    let oversized_error = validate_discovered_catalog_bounds(oversized, "account")
        .expect_err("oversized identities must fail");
    assert!(oversized_error.message.contains("invalid model identifier"));
}

/// Verifies Anthropic-compatible discovery follows every page and forwards the exact cursor.
/// 验证 Anthropic 兼容发现会读取全部分页并转发精确游标。
#[tokio::test]
async fn custom_anthropic_discovery_collects_multiple_pages() {
    let (result, requests) = discover_custom_anthropic_pages(vec![
        (
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"claude-a"}"#,
        ),
        (
            "200 OK",
            r#"{"data":[{"id":"claude-b","display_name":"Claude B"}],"has_more":false,"last_id":"claude-b"}"#,
        ),
    ])
    .await;
    let catalog = result.expect("two valid pages should discover successfully");

    assert_eq!(catalog.page_count, 2);
    assert_eq!(
        catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-a", "claude-b"]
    );
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /v1/models?limit=1000 HTTP/1.1\r\n"));
    assert!(requests[1].contains("after_id=claude-a"));
}

/// Verifies Anthropic-compatible discovery honors one reviewed same-origin catalog path.
/// 验证 Anthropic 兼容发现会遵守已审核的同源模型目录路径。
#[tokio::test]
async fn custom_anthropic_discovery_uses_custom_catalog_path() {
    // fixture records the exact HTTP path requested by the discovery client.
    // fixture 记录发现客户端请求的精确 HTTP 路径。
    let (origin, request_receiver, server_task) = start_metadata_sequence_fixture(vec![(
        "200 OK",
        r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":false,"last_id":"claude-a"}"#,
    )])
    .await;
    // provider selects a non-standard catalog path under the configured versioned base URL.
    // provider 选择位于已配置版本化基础 URL 下的非标准目录路径。
    let (provider, connection) = custom_anthropic_fixture(
        "custom:anthropic-custom-path",
        format!("{origin}/v1").as_str(),
        Some("/catalog/models"),
    );
    // entry is the exact projected service entry used by the metadata client.
    // entry 是元数据客户端使用的精确投影服务入口。
    let entry = provider
        .service_entries
        .first()
        .expect("custom Anthropic provider entry");
    // secret supplies the exact X-API-Key projection expected by the custom provider.
    // secret 提供自定义供应商所需的精确 X-API-Key 投影。
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");

    // catalog is the completed one-page discovery result.
    // catalog 是已完成的单页发现结果。
    let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, entry, &connection, &secret, None, 1)
        .await
        .expect("custom Anthropic discovery should honor the configured path");
    // requests contains every HTTP request observed by the bounded fixture.
    // requests 包含有界夹具观测到的每个 HTTP 请求。
    let requests = request_receiver
        .await
        .expect("metadata sequence fixture should publish requests");
    server_task
        .await
        .expect("metadata sequence fixture should finish");

    assert_eq!(catalog.models.len(), 1);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /v1/catalog/models?limit=1000 HTTP/1.1\r\n"));
}

/// Verifies a duplicate Anthropic model identity across pages rejects the complete traversal.
/// 验证 Anthropic 跨页重复模型标识会拒绝完整遍历。
#[tokio::test]
async fn custom_anthropic_discovery_rejects_duplicate_models() {
    let (result, requests) = discover_custom_anthropic_pages(vec![
        (
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"cursor-a"}"#,
        ),
        (
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":false,"last_id":"cursor-b"}"#,
        ),
    ])
    .await;
    let error = result.expect_err("duplicate model identity must reject discovery");

    assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
    assert!(
        error.message.contains("duplicate model `claude-a`"),
        "{}",
        error.message
    );
    assert_eq!(requests.len(), 2);
}

/// Verifies `has_more=true` without a cursor is an invalid response rather than an empty page.
/// 验证缺少游标的 `has_more=true` 属于无效响应而非空分页。
#[tokio::test]
async fn custom_anthropic_discovery_requires_cursor_for_more_pages() {
    let (result, requests) = discover_custom_anthropic_pages(vec![(
        "200 OK",
        r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true}"#,
    )])
    .await;
    let error = result.expect_err("missing cursor must reject discovery");

    assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
    assert!(error.message.contains("without `last_id`"));
    assert_eq!(requests.len(), 1);
}

/// Verifies a repeated Anthropic cursor terminates before a third request can loop forever.
/// 验证 Anthropic 重复游标会在第三次请求前终止，避免无限循环。
#[tokio::test]
async fn custom_anthropic_discovery_rejects_cursor_cycles() {
    let (result, requests) = discover_custom_anthropic_pages(vec![
        (
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"cursor-a"}"#,
        ),
        (
            "200 OK",
            r#"{"data":[{"id":"claude-b","display_name":"Claude B"}],"has_more":true,"last_id":"cursor-a"}"#,
        ),
    ])
    .await;
    let error = result.expect_err("repeated cursor must reject discovery");

    assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
    assert!(
        error
            .message
            .contains("repeated pagination cursor `cursor-a`")
    );
    assert_eq!(requests.len(), 2);
}

/// Verifies custom discovery calls the standard endpoint, sends Bearer auth, and tolerates extensions.
/// 验证自定义发现会调用标准端点、发送 Bearer 鉴权并容忍扩展字段。
#[tokio::test]
async fn custom_openai_discovery_uses_standard_models_endpoint() {
    let (origin, request_receiver, server_task) = start_metadata_fixture(
        "200 OK",
        r#"{"object":"list","first_id":"model-a","data":[{"id":"model-a","object":"model","owned_by":"fixture"}]}"#,
    )
    .await;
    let (provider, connection) =
        custom_openai_fixture("custom:local-fixture", format!("{origin}/v1").as_str());
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider entry");
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");
    let client = HttpProviderMetadataClient::new(reqwest::Client::new());

    let catalog = client
        .discover_models(&provider, entry, &connection, &secret, None, 7)
        .await
        .expect("custom model discovery should succeed");
    let request = request_receiver
        .await
        .expect("metadata fixture should capture the request");
    server_task.await.expect("metadata fixture should finish");

    assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-key")
    );
    assert_eq!(catalog.connection_id, "connection");
    assert_eq!(catalog.generation, 7);
    assert_eq!(catalog.page_count, 1);
    assert_eq!(catalog.models.len(), 1);
    assert_eq!(catalog.models[0].id, "model-a");
}

/// Verifies a provider HTTP authentication failure remains observable for typed classification.
/// 验证供应商 HTTP 鉴权失败会保持可观测，以供类型化分类。
#[tokio::test]
async fn custom_openai_discovery_preserves_authentication_status() {
    let (origin, _request_receiver, server_task) = start_metadata_fixture(
        "401 Unauthorized",
        r#"{"error":{"message":"invalid credential"}}"#,
    )
    .await;
    let (provider, connection) =
        custom_openai_fixture("custom:auth-fixture", format!("{origin}/v1").as_str());
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider entry");
    let secret = CredentialSecretEnvelope::new_api_key(String::from("invalid-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");
    let client = HttpProviderMetadataClient::new(reqwest::Client::new());

    let error = client
        .discover_models(&provider, entry, &connection, &secret, None, 1)
        .await
        .expect_err("authentication failure must reject discovery");
    server_task.await.expect("metadata fixture should finish");

    assert_eq!(error.kind, ModelDiscoveryFailureKind::AuthenticationFailed);
    assert!(error.message.contains("HTTP 401"));
    assert!(error.message.contains("invalid credential"));
}

/// Verifies every documented HTTP discovery class is assigned at the response boundary.
/// 验证每一种文档化 HTTP 发现分类都会在响应边界直接赋值。
#[tokio::test]
async fn custom_discovery_http_statuses_have_exact_typed_failures() {
    for (status, expected) in [
        (
            "403 Forbidden",
            ModelDiscoveryFailureKind::AuthenticationFailed,
        ),
        ("404 Not Found", ModelDiscoveryFailureKind::Unsupported),
        (
            "405 Method Not Allowed",
            ModelDiscoveryFailureKind::Unsupported,
        ),
        (
            "429 Too Many Requests",
            ModelDiscoveryFailureKind::RateLimited,
        ),
        (
            "408 Request Timeout",
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
        ),
        (
            "500 Internal Server Error",
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
        ),
        (
            "503 Service Unavailable",
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
        ),
    ] {
        let (origin, _request_receiver, server_task) =
            start_metadata_fixture(status, r#"{"error":"controlled"}"#).await;
        let (provider, connection) =
            custom_openai_fixture("custom:status-fixture", format!("{origin}/v1").as_str());
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let error = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(&provider, entry, &connection, &secret, None, 1)
            .await
            .expect_err("non-success status must reject discovery");
        server_task.await.expect("metadata fixture should finish");

        assert_eq!(error.kind, expected, "unexpected class for {status}");
    }
}

/// Verifies malformed success payloads fail while an explicit empty catalog remains successful.
/// 验证格式错误的成功载荷会失败，而显式空目录仍保持成功。
#[tokio::test]
async fn custom_discovery_distinguishes_invalid_json_from_empty_catalog() {
    let (invalid_origin, _request_receiver, invalid_server) =
        start_metadata_fixture("200 OK", "{not-json").await;
    let (invalid_provider, invalid_connection) = custom_openai_fixture(
        "custom:invalid-fixture",
        format!("{invalid_origin}/v1").as_str(),
    );
    let invalid_entry = invalid_provider
        .service_entries
        .first()
        .expect("custom provider entry");
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");
    let client = HttpProviderMetadataClient::new(reqwest::Client::new());
    let error = client
        .discover_models(
            &invalid_provider,
            invalid_entry,
            &invalid_connection,
            &secret,
            None,
            1,
        )
        .await
        .expect_err("malformed JSON must reject discovery");
    invalid_server
        .await
        .expect("invalid metadata fixture should finish");
    assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);

    let (empty_origin, _request_receiver, empty_server) =
        start_metadata_fixture("200 OK", r#"{"object":"list","data":[]}"#).await;
    let (empty_provider, empty_connection) = custom_openai_fixture(
        "custom:empty-fixture",
        format!("{empty_origin}/v1").as_str(),
    );
    let empty_entry = empty_provider
        .service_entries
        .first()
        .expect("custom provider entry");
    let catalog = client
        .discover_models(
            &empty_provider,
            empty_entry,
            &empty_connection,
            &secret,
            None,
            2,
        )
        .await
        .expect("empty catalog is a successful observed state");
    empty_server
        .await
        .expect("empty metadata fixture should finish");
    assert!(catalog.models.is_empty());
    assert_eq!(catalog.page_count, 1);
}

/// Verifies a transport failure is retryable and never masquerades as unsupported discovery.
/// 验证传输失败可重试，且绝不会伪装成目录不受支持。
#[tokio::test]
async fn custom_discovery_transport_failure_is_temporarily_unavailable() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("temporary endpoint should bind");
    let address = listener.local_addr().expect("temporary endpoint address");
    drop(listener);
    let (provider, connection) = custom_openai_fixture(
        "custom:transport-fixture",
        format!("http://{address}/v1").as_str(),
    );
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider entry");
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");
    let error = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, entry, &connection, &secret, None, 1)
        .await
        .expect_err("closed endpoint must reject discovery");

    assert_eq!(
        error.kind,
        ModelDiscoveryFailureKind::TemporarilyUnavailable
    );
}

/// Verifies the active Manifest is the only route authority used by discovery.
/// 验证活动 Manifest 是模型发现使用的唯一路由权威。
#[test]
fn discovery_uses_only_active_manifest_routes() {
    let registry = crate::ProviderRegistry::first_batch().expect("provider registry");
    // provider is the cut-over APIKEY.FUN descriptor compiled from the bundled Manifest.
    // provider 是从内置 Manifest 编译得到的已切换 APIKEY.FUN 描述器。
    let provider = registry
        .provider("apikeyfun")
        .expect("APIKEY.FUN provider must register");
    // entry owns every route that the signed system Manifest authorizes for this credential kind.
    // entry 持有已签名系统 Manifest 为此凭据类型授权的全部路由。
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == "public-api")
        .expect("APIKEY.FUN public entry must exist");
    assert_eq!(
        entry
            .protocol_routes
            .iter()
            .filter(|route| route_is_available_for_discovery(route))
            .count(),
        4
    );
}

/// Verifies multi-protocol discovery groups identical discovery signatures into one request
/// set and fills per-protocol model variants on every merged model.
/// 验证多协议发现把相同发现签名分组为一次请求集合，并为每个合并模型填充按协议变体。
#[tokio::test]
async fn multi_protocol_discovery_merges_shared_signature_into_per_protocol_variants() {
    let registry = crate::ProviderRegistry::first_batch().expect("provider registry");
    let provider = registry
        .provider("openrouter")
        .cloned()
        .expect("openrouter provider must register");
    let mut entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == "public-api")
        .expect("openrouter public-api entry must exist")
        .clone();
    // routes captures every Manifest-authorized discovery protocol for variant assertions.
    // routes 捕获每个经 Manifest 授权的发现协议，用于变体断言。
    let routes = entry
        .protocol_routes
        .iter()
        .filter(|route| route_is_available_for_discovery(route))
        .map(|route| route.protocol_profile_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        routes.len(),
        3,
        "openrouter public entry exposes three verified routes"
    );

    let model_json = r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"OpenAI: GPT-5.6 Sol","context_length":1050000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":128000}}]}"#;
    let (origin, request_receiver, server_task) = start_metadata_sequence_fixture(vec![
        ("200 OK", model_json),
        ("200 OK", r#"{"data":[]}"#),
        ("200 OK", model_json),
    ])
    .await;
    entry.base_url = origin;

    let connection = CredentialConnection {
        id: String::from("openrouter-account"),
        provider_id: String::from("openrouter"),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential"),
        display_name: String::from("Account"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");

    let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, &entry, &connection, &secret, None, 7)
        .await
        .expect("multi-protocol discovery should merge shared-signature routes");
    let requests = request_receiver
        .await
        .expect("sequence fixture should publish requests");
    server_task.await.expect("sequence fixture should finish");

    // Three identical-signature routes share exactly one discovery set (three OpenRouter
    // catalog requests), never one request set per protocol.
    // 三条相同签名路由恰好共享一次发现集合（三个 OpenRouter 目录请求），绝不按协议各发一次。
    assert_eq!(
        requests.len(),
        3,
        "shared signature must discover exactly once"
    );

    let model = catalog
        .models
        .iter()
        .find(|model| model.id == "openai/gpt-5.6-sol")
        .expect("merged model must exist");
    assert_eq!(model.protocol_variants.len(), 3);
    for route in &routes {
        let variant = model
            .protocol_variants
            .get(route)
            .unwrap_or_else(|| panic!("missing protocol variant for `{route}`"));
        assert_eq!(variant.catalog_generation, 7);
        assert_eq!(
            variant.capabilities.context_limit,
            Some(1_050_000),
            "variant must carry the discovered capability for `{route}`"
        );
    }
    // Identical signature means every variant shares one signature hash.
    // 相同签名意味着每个变体共享一个签名哈希。
    let signatures = model
        .protocol_variants
        .values()
        .map(|variant| variant.discovery_signature.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        signatures.len(),
        1,
        "shared signature group must share one hash"
    );
}

/// Verifies discovery projects every protocol route verified by the active Manifest.
/// 验证发现过程会投影活动 Manifest 验证的每条协议路由。
#[tokio::test]
async fn discovery_projects_all_manifest_verified_protocol_variants() {
    // registry supplies the signed multi-protocol OpenRouter entry.
    // registry 提供已签名的多协议 OpenRouter 入口。
    let registry = crate::ProviderRegistry::first_batch().expect("provider registry");
    let provider = registry
        .provider("openrouter")
        .cloned()
        .expect("openrouter provider must register");
    // entry is isolated so its origin can point at the bounded local fixture.
    // entry 被隔离，以便把服务源指向有界本地夹具。
    let mut entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == "public-api")
        .expect("openrouter public-api entry must exist")
        .clone();
    // modelJson is the one credential-visible model returned by both required catalog calls.
    // modelJson 是所需目录调用共同返回的唯一凭据可见模型。
    let model_json = r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"OpenAI: GPT-5.6 Sol","context_length":1050000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":128000}}]}"#;
    // fixture serves exactly one shared OpenRouter discovery request set.
    // fixture 恰好提供一组共享 OpenRouter 发现请求。
    let (origin, request_receiver, server_task) = start_metadata_sequence_fixture(vec![
        ("200 OK", model_json),
        ("200 OK", r#"{"data":[]}"#),
        ("200 OK", model_json),
    ])
    .await;
    entry.base_url = origin;
    // connection owns credentials only; it carries no protocol state.
    // connection 只持有凭据，不携带协议状态。
    let connection = CredentialConnection {
        id: String::from("openrouter-responses-only-account"),
        provider_id: String::from("openrouter"),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential"),
        display_name: String::from("Responses-only account"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // secret is confined to the local fixture request boundary.
    // secret 被限制在本地夹具请求边界内。
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");

    // catalog contains one variant for every Manifest-verified service route.
    // catalog 为每条 Manifest 已验证服务路由包含一个变体。
    let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, &entry, &connection, &secret, None, 8)
        .await
        .expect("credential-scoped discovery should succeed");
    let requests = request_receiver
        .await
        .expect("sequence fixture should publish requests");
    server_task.await.expect("sequence fixture should finish");
    assert_eq!(requests.len(), 3);
    let model = catalog
        .models
        .iter()
        .find(|model| model.id == "openai/gpt-5.6-sol")
        .expect("credential-visible model should exist");
    assert_eq!(
        model.protocol_variants.keys().cloned().collect::<Vec<_>>(),
        vec![
            String::from(ANTHROPIC_MESSAGES_PROFILE_ID),
            String::from(OPENAI_CHAT_PROFILE_ID),
            String::from(OPENAI_RESPONSES_PROFILE_ID),
        ],
    );
}

/// Verifies signed supported sets, not route enumeration, authorize discovered variants.
/// 验证发现变体由签名支持集合授权，而不是由路由枚举授权。
#[test]
fn manifest_variant_authorization_is_model_and_policy_scoped() {
    let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
    let deepseek = manifests
        .manifest("deepseek")
        .expect("DeepSeek manifest must exist");
    assert!(manifest_protocol_authorizes_model_variant(
        deepseek,
        "official-api",
        "api-key",
        ANTHROPIC_MESSAGES_PROFILE_ID,
        "deepseek-v4-pro",
    ));
    assert!(manifest_protocol_authorizes_model_variant(
        deepseek,
        "official-api",
        "api-key",
        OPENAI_RESPONSES_PROFILE_ID,
        "deepseek-v4-pro",
    ));

    let openrouter = manifests
        .manifest("openrouter")
        .expect("OpenRouter manifest must exist");
    assert!(!manifest_protocol_authorizes_model_variant(
        openrouter,
        "public-api",
        "api-key",
        OPENAI_CHAT_PROFILE_ID,
        "vendor/unmapped-model",
    ));
}

/// Verifies distinct discovery signatures discover separately and merge per model identity
/// with per-protocol variants carrying their own signatures.
/// 验证不同发现签名分别发现并按模型身份合并，按协议变体携带各自签名。
#[tokio::test]
async fn multi_protocol_discovery_merges_distinct_signatures_separately() {
    let registry = crate::ProviderRegistry::first_batch().expect("provider registry");
    let provider = registry
        .provider("openrouter")
        .cloned()
        .expect("openrouter provider must register");
    let mut entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == "public-api")
        .expect("openrouter public-api entry must exist")
        .clone();
    // Split one route onto a distinct discovery signature (OpenAI kind, separate origin).
    // 把一条路由拆分为不同发现签名（OpenAI 契约、独立源地址）。
    let model_json = r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"OpenAI: GPT-5.6 Sol","context_length":1050000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":128000}}]}"#;
    let (openrouter_origin, or_receiver, or_task) = start_metadata_sequence_fixture(vec![
        ("200 OK", model_json),
        ("200 OK", r#"{"data":[]}"#),
        ("200 OK", model_json),
    ])
    .await;
    let (openai_origin, openai_receiver, openai_task) =
        start_metadata_sequence_fixture(vec![("200 OK", model_json)]).await;
    entry.base_url = openrouter_origin.clone();
    let responses_route = entry
        .protocol_routes
        .iter_mut()
        .find(|route| route.protocol_profile_id == "openai-responses.v1")
        .expect("responses route must exist");
    responses_route.model_discovery = Some(crate::ModelDiscoveryKind::OpenAi);
    responses_route.model_discovery_path = Some(String::from("/models"));
    responses_route.base_url_override = Some(openai_origin.clone());

    let connection = CredentialConnection {
        id: String::from("openrouter-account"),
        provider_id: String::from("openrouter"),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential"),
        display_name: String::from("Account"),
        secret_reference: String::from("secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
        .expect("test credential envelope")
        .execution_secret()
        .expect("test execution secret");

    let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
        .discover_models(&provider, &entry, &connection, &secret, None, 9)
        .await
        .expect("distinct signatures should discover separately and merge");
    let or_requests = or_receiver.await.expect("openrouter fixture requests");
    or_task.await.expect("openrouter fixture should finish");
    let openai_requests = openai_receiver.await.expect("openai fixture requests");
    openai_task.await.expect("openai fixture should finish");
    assert_eq!(or_requests.len(), 3);
    assert_eq!(openai_requests.len(), 1);

    let model = catalog
        .models
        .iter()
        .find(|model| model.id == "openai/gpt-5.6-sol")
        .expect("merged model must exist");
    assert_eq!(model.protocol_variants.len(), 3);
    // Distinct signatures keep distinct signature hashes on the variants.
    // 不同签名在变体上保留不同签名哈希。
    let signatures = model
        .protocol_variants
        .values()
        .map(|variant| variant.discovery_signature.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        signatures.len(),
        2,
        "two discovery groups must keep two signatures"
    );
}

/// Verifies a published manifest inventory can add and remove static models without code changes.
/// 验证已发布 Manifest 清单可在不修改代码的情况下增删静态模型。
#[test]
fn authoritative_manifest_inventory_replaces_static_catalog_membership() {
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let mut manifest = catalog
        .manifest_arc("alibaba")
        .expect("Alibaba manifest must exist")
        .as_ref()
        .clone();
    manifest.catalog_models.insert(
        String::from("coding-plan-cn"),
        vec![crate::provider_manifest::ManifestCatalogModel {
            model_id: String::from("future-model-from-json"),
            display_name: String::from("Future Model From JSON"),
        }],
    );
    let static_catalog =
        build_manifest_static_catalog(Some(&manifest), "connection", "coding-plan-cn", 7)
            .expect("manifest static catalog must build");

    assert_eq!(static_catalog.models.len(), 1);
    assert_eq!(static_catalog.models[0].id, "future-model-from-json");
    assert_eq!(
        static_catalog.models[0].display_name,
        "Future Model From JSON"
    );
    assert_eq!(
        static_catalog.discovery_schema_revision,
        "provider-manifest.static-models.v1"
    );
    assert_eq!(
        static_catalog.models[0].capabilities,
        ModelCapabilities::default(),
        "static discovery must not preserve code-owned capability seeds"
    );
}

/// Verifies manifest-static discovery fails closed without exact snapshot authority.
/// 验证 Manifest 静态发现缺少精确快照权威时封闭失败。
#[test]
fn manifest_static_catalog_requires_snapshot_and_service_inventory() {
    // missing_manifest_error proves a system static route cannot silently use binary defaults.
    // missing_manifest_error 证明系统静态路由不能静默使用二进制默认值。
    let missing_manifest_error =
        build_manifest_static_catalog(None, "connection", "coding-plan-cn", 7)
            .expect_err("missing manifest must fail closed");
    assert_eq!(
        missing_manifest_error.kind,
        ModelDiscoveryFailureKind::Failed
    );

    // manifest_without_inventory removes the exact entry while preserving all other facts.
    // manifest_without_inventory 在保留其他事实的同时移除精确入口清单。
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let mut manifest_without_inventory = catalog
        .manifest_arc("alibaba")
        .expect("Alibaba manifest must exist")
        .as_ref()
        .clone();
    manifest_without_inventory
        .catalog_models
        .remove("coding-plan-cn");
    // missing_inventory_error proves another entry cannot supply guessed identities.
    // missing_inventory_error 证明其他入口不能提供猜测身份。
    let missing_inventory_error = build_manifest_static_catalog(
        Some(&manifest_without_inventory),
        "connection",
        "coding-plan-cn",
        7,
    )
    .expect_err("missing exact service inventory must fail closed");
    assert_eq!(
        missing_inventory_error.kind,
        ModelDiscoveryFailureKind::Failed
    );
}

/// Verifies MiniMax model discovery is expressed only through provider-neutral manifest rules.
/// 验证 MiniMax 模型发现仅通过供应商无关的 Manifest 规则表达。
#[test]
fn minimax_model_discovery_uses_generic_manifest_contracts() {
    // catalog is the exact bundled snapshot shipped to the runtime.
    // catalog 是交付给运行时的精确内置快照。
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    // manifest freezes the reviewed MiniMax service and protocol-route declarations.
    // manifest 冻结已审核的 MiniMax 服务与协议路由声明。
    let manifest = catalog
        .manifest_arc("minimax")
        .expect("MiniMax manifest must exist");
    assert_eq!(manifest.revision, 14);
    // expectations bind each region to its exact reviewed discovery origins.
    // expectations 把每个地区绑定到其精确已审核发现来源。
    let expectations = [
        (
            "global",
            "https://api.minimax.io/anthropic/v1/models",
            "https://api.minimax.io/v1/models",
        ),
        (
            "cn",
            "https://api.minimaxi.com/anthropic/v1/models",
            "https://api.minimaxi.com/v1/models",
        ),
    ];
    for (service_entry_id, anthropic_url, openai_url) in expectations {
        // entry is the unique service entry for the reviewed region.
        // entry 是已审核地区的唯一服务入口。
        let entry = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .expect("MiniMax service entry must exist");
        assert_eq!(
            entry.model_discovery,
            ModelDiscoveryKind::AnthropicCompatible
        );
        // anthropic_route owns the provider-neutral compatible discovery declaration.
        // anthropic_route 持有供应商无关的兼容发现声明。
        let anthropic_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == ANTHROPIC_MESSAGES_PROFILE_ID)
            .expect("MiniMax Anthropic route must exist");
        assert_eq!(
            anthropic_route.resolved_model_discovery(entry.model_discovery),
            ModelDiscoveryKind::AnthropicCompatible
        );
        assert_eq!(
            anthropic_route.model_discovery_url(
                entry,
                default_discovery_path(ModelDiscoveryKind::AnthropicCompatible),
            ),
            anthropic_url
        );
        // openai_route owns the generic OpenAI discovery override for the same service.
        // openai_route 持有同一服务的通用 OpenAI 发现覆盖。
        let openai_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == OPENAI_CHAT_PROFILE_ID)
            .expect("MiniMax OpenAI route must exist");
        assert_eq!(
            openai_route.resolved_model_discovery(entry.model_discovery),
            ModelDiscoveryKind::OpenAi
        );
        assert_eq!(
            openai_route
                .model_discovery_url(entry, default_discovery_path(ModelDiscoveryKind::OpenAi),),
            openai_url
        );
    }
}

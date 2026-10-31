//! Single-catalog HTTP regressions for independently enabled conversation protocols.
//! 独立启用会话协议时的唯一目录 HTTP 回归测试。

use super::*;
use crate::{CustomProtocolRoute, CustomProviderModelDiscoveryProtocol};

/// Builds a shared catalog at `origin` using `protocol`, with unreachable conversation overrides.
/// 使用 `protocol` 在 `origin` 构建共享目录，并为会话配置不可达的独立覆盖。
/// `catalog_path` selects an explicit path; `None` exercises the protocol's standard directory.
/// `catalog_path` 选择显式路径；`None` 验证协议的标准目录。
/// Returns the descriptor and its exact synthetic credential connection.
/// 返回描述符及其精确合成凭据连接。
fn fixture(
    origin: &str,
    protocol: CustomProviderModelDiscoveryProtocol,
    catalog_path: Option<&str>,
) -> (ProviderDescriptor, CredentialConnection) {
    // connection reuses the existing credential fixture without consulting any live settings.
    // connection 复用既有凭据夹具，不访问任何真实设置。
    let (_, connection) = custom_openai_fixture("custom:single-catalog", origin);
    // protocols remain executable declarations; none owns catalog discovery.
    // protocols 保留执行声明；其中任何一项均不持有目录发现。
    let protocols = vec![
        CustomProviderProtocol::AnthropicMessages,
        CustomProviderProtocol::OpenAiResponses,
        CustomProviderProtocol::OpenAiChat,
    ];
    // provider keeps catalog authentication separate from every conversation override.
    // provider 将目录认证与全部会话覆盖保持分离。
    let provider = CustomProviderDefinition {
        id: connection.provider_id.clone(),
        display_name: String::from("Catalog fixture"),
        base_url: origin.to_string(),
        protocol_routes: protocols
            .iter()
            .map(|protocol| CustomProtocolRoute {
                protocol: *protocol,
                base_url: Some(String::from("https://127.0.0.1:9/conversation")),
                endpoint_path: None,
                authentication: Some(CustomProviderAuthentication::BearerAuthorization),
                request_headers: BTreeMap::from([(
                    String::from("x-route-only"),
                    String::from("conversation"),
                )]),
            })
            .collect(),
        protocols,
        authentication: match protocol {
            CustomProviderModelDiscoveryProtocol::OpenAi => {
                CustomProviderAuthentication::BearerAuthorization
            }
            CustomProviderModelDiscoveryProtocol::Anthropic => {
                CustomProviderAuthentication::XApiKey
            }
            CustomProviderModelDiscoveryProtocol::Gemini => {
                CustomProviderAuthentication::GoogleApiKey
            }
        },
        model_discovery_mode: if catalog_path.is_some() {
            CustomProviderModelDiscoveryMode::CustomPath
        } else {
            CustomProviderModelDiscoveryMode::Standard
        },
        model_discovery_protocol: protocol,
        model_discovery_path: catalog_path.map(str::to_string),
        request_headers: BTreeMap::from([(String::from("x-catalog"), String::from("shared"))]),
        session_header: None,
    }
    .into_descriptor()
    .expect("single catalog definition must validate");
    (provider, connection)
}

/// Verifies every selected catalog format produces one request and all declared conversation variants.
/// 验证每种选定目录格式只发起一次请求，并产出全部已声明会话协议变体。
#[tokio::test]
async fn custom_catalog_fetches_only_the_selected_format() {
    for (protocol, body, query, auth) in [
        (
            CustomProviderModelDiscoveryProtocol::OpenAi,
            r#"{"object":"list","data":[{"id":"catalog-model"}]}"#,
            "",
            "authorization: bearer test-key",
        ),
        (
            CustomProviderModelDiscoveryProtocol::Anthropic,
            r#"{"data":[{"id":"catalog-model","display_name":"Catalog Model"}],"has_more":false,"last_id":"catalog-model"}"#,
            "?limit=1000",
            "x-api-key: test-key",
        ),
        (
            CustomProviderModelDiscoveryProtocol::Gemini,
            r#"{"models":[{"name":"models/catalog-model","displayName":"Catalog Model","supportedGenerationMethods":["generateContent"]}]}"#,
            "?pageSize=1000",
            "x-goog-api-key: test-key",
        ),
    ] {
        // server accepts exactly one request, so any per-protocol retry fails this complete discovery.
        // server 仅接受一次请求，因此任何逐协议重试都会使完整发现失败。
        let (origin, received, server) = start_metadata_fixture("200 OK", body).await;
        // provider and connection declare the selected catalog independently from conversation transports.
        // provider 与 connection 独立于会话传输声明所选目录。
        let (provider, connection) = fixture(&origin, protocol, Some("/catalog"));
        // manifest round-trips the selected kind and path through the production persistence projection.
        // manifest 通过生产持久化投影往返验证所选格式与路径。
        let manifest =
            crate::project_custom_descriptor_manifest(&provider).expect("project manifest");
        // resolved uses the same manifest compilation as a persisted custom provider.
        // resolved 使用与已持久化自定义供应商相同的清单编译流程。
        let resolved = crate::compile_manifest(&manifest, crate::ManifestOwnership::Custom)
            .expect("compile manifest");
        // entry is the single compiled catalog owner.
        // entry 是编译后的唯一目录归属入口。
        let entry = resolved
            .provider()
            .service_entries
            .first()
            .expect("custom entry");
        // secret is a synthetic fixture key that never contacts a real provider.
        // secret 是不会访问真实供应商的合成夹具密钥。
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .unwrap()
            .execution_secret()
            .unwrap();
        // catalog exercises the public discovery pipeline and per-route result projection.
        // catalog 验证公开发现管线及逐路由结果投影。
        let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(
                resolved.provider(),
                entry,
                &connection,
                &secret,
                Some(std::sync::Arc::new(resolved.clone())),
                7,
            )
            .await
            .expect("one selected catalog must serve every conversation protocol");
        // request is the only HTTP request accepted by the bounded fixture.
        // request 是有界夹具接受的唯一 HTTP 请求。
        let request = received.await.expect("captured request");
        server.await.expect("fixture completion");
        assert!(request.starts_with(&format!("GET /catalog{query} HTTP/1.1\r\n")));
        // headers normalizes HTTP's case-insensitive field names for transport assertions.
        // headers 规范化 HTTP 大小写不敏感的字段名以验证传输。
        let headers = request.to_ascii_lowercase();
        assert!(headers.contains(auth));
        assert!(headers.contains("x-catalog: shared"));
        assert!(!headers.contains("x-route-only"));
        assert_eq!(
            headers.contains("anthropic-version:"),
            protocol == CustomProviderModelDiscoveryProtocol::Anthropic
        );
        assert_eq!(catalog.page_count, 1);
        // model is located by stable identity rather than a catalog position.
        // model 按稳定身份定位，不依赖目录位置。
        let model = catalog
            .models
            .iter()
            .find(|model| model.id == "catalog-model")
            .expect("returned model must remain in catalog");
        assert_eq!(model.protocol_variants.len(), 3);
        // signatures proves that all declared variants share one catalog request contract.
        // signatures 验证全部已声明变体共享同一个目录请求契约。
        let signatures = model
            .protocol_variants
            .values()
            .map(|variant| &variant.discovery_signature)
            .collect::<BTreeSet<_>>();
        assert_eq!(signatures.len(), 1);
    }
}

/// Verifies parser and HTTP failures never switch from the selected Anthropic catalog to OpenAI.
/// 验证解析及 HTTP 失败绝不从选定 Anthropic 目录切换到 OpenAI。
#[tokio::test]
async fn custom_catalog_failure_never_probes_another_protocol() {
    for (status, body, expected) in [
        (
            "200 OK",
            r#"{"object":"list","data":[{"id":"catalog-model"}]}"#,
            ModelDiscoveryFailureKind::InvalidResponse,
        ),
        (
            "401 Unauthorized",
            r#"{"error":{"message":"denied"}}"#,
            ModelDiscoveryFailureKind::AuthenticationFailed,
        ),
    ] {
        // fixture exposes the chosen catalog once; every conversation route remains unreachable.
        // fixture 只暴露一次所选目录；全部会话路由保持不可达。
        let (origin, received, server) = start_metadata_fixture(status, body).await;
        // provider and connection keep Anthropic selected despite the advertised OpenAI routes.
        // provider 与 connection 即使声明 OpenAI 路由也保持选定 Anthropic 目录。
        let (provider, connection) = fixture(
            &origin,
            CustomProviderModelDiscoveryProtocol::Anthropic,
            Some("/catalog"),
        );
        // entry carries the authoritative shared catalog declaration.
        // entry 承载权威共享目录声明。
        let entry = provider.service_entries.first().expect("custom entry");
        // secret is restricted to the local HTTP fixture.
        // secret 仅供本地 HTTP 夹具使用。
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .unwrap()
            .execution_secret()
            .unwrap();
        // error must retain the selected request's failure rather than a fallback route's result.
        // error 必须保留所选请求的失败，而不能替换为回退路由结果。
        let error = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(&provider, entry, &connection, &secret, None, 1)
            .await
            .expect_err("selected catalog failure must be retained");
        assert_eq!(error.kind, expected);
        assert!(
            received
                .await
                .unwrap()
                .starts_with("GET /catalog?limit=1000 HTTP/1.1\r\n")
        );
        server.await.unwrap();
    }
}

/// Verifies Gemini discovery preserves the shared base and explicit directory across pagination.
/// 验证 Gemini 目录发现跨分页保留共享基础地址和显式目录。
/// Uses only local synthetic responses and checks the resulting model variants and request signatures.
/// 仅使用本地合成响应，并检查生成的模型变体与请求签名。
#[tokio::test]
async fn custom_gemini_catalog_preserves_base_and_explicit_paths_across_pages() {
    // Each case fixes the expected HTTP path independently of the production URL builder.
    // 每个用例独立于生产 URL 构建器固定预期 HTTP 路径。
    for (base_path, catalog_path, expected_path) in [
        ("/v1beta", None, "/v1beta/models"),
        ("/v1beta/", None, "/v1beta/models"),
        ("/proxy/v1/", None, "/proxy/v1/models"),
        ("/", None, "/models"),
        (
            "/proxy/v1beta/",
            Some("/catalog/models"),
            "/proxy/v1beta/catalog/models",
        ),
    ] {
        // The fixture serves exactly two pages, so per-conversation protocol requests cannot pass.
        // 夹具仅提供两页，因此逐会话协议重复请求无法通过。
        let (origin, received, server) = start_metadata_sequence_fixture(vec![
            (
                "200 OK",
                r#"{"models":[{"name":"models/catalog-first","displayName":"First Catalog Model","supportedGenerationMethods":["generateContent"]}],"nextPageToken":"second-page"}"#,
            ),
            (
                "200 OK",
                r#"{"models":[{"name":"models/catalog-second","displayName":"Second Catalog Model","supportedGenerationMethods":["generateContent"]}]}"#,
            ),
        ])
        .await;
        // provider and connection enter through the same definition as a newly configured provider.
        // provider 与 connection 使用和新配置供应商相同的定义入口。
        let (provider, connection) = fixture(
            &format!("{origin}{base_path}"),
            CustomProviderModelDiscoveryProtocol::Gemini,
            catalog_path,
        );
        // manifest and resolved exercise the production save/reload projection before any HTTP call.
        // manifest 与 resolved 在 HTTP 调用前验证生产保存及重载投影。
        let manifest = crate::project_custom_descriptor_manifest(&provider)
            .expect("Gemini catalog manifest must project");
        let resolved = crate::compile_manifest(&manifest, crate::ManifestOwnership::Custom)
            .expect("Gemini catalog manifest must compile");
        // entry is located by credential ownership rather than its position in the manifest.
        // entry 按凭据归属定位，不依赖其在清单中的位置。
        let entry = resolved
            .provider()
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .expect("credential service entry must exist");
        // secret is a synthetic API key used only by the local fixture.
        // secret 是仅供本地夹具使用的合成 API Key。
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .unwrap()
            .execution_secret()
            .unwrap();
        // catalog is discovered through the public pipeline with all three conversation routes enabled.
        // catalog 通过公开管线发现，同时启用全部三种会话路由。
        let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(
                resolved.provider(),
                entry,
                &connection,
                &secret,
                Some(std::sync::Arc::new(resolved.clone())),
                7,
            )
            .await
            .expect("Gemini directory must retain its base path across pages");
        // requests records the actual wire paths, including the cursor on the second page.
        // requests 记录实际线上路径，包括第二页游标。
        let requests = received.await.expect("captured Gemini requests");
        server.await.expect("Gemini fixture completion");
        assert_eq!(requests.len(), 2);
        for (request, query) in requests
            .iter()
            .zip(["?pageSize=1000", "?pageSize=1000&pageToken=second-page"])
        {
            assert!(
                request.starts_with(&format!("GET {expected_path}{query} HTTP/1.1\r\n")),
                "unexpected Gemini request for base {base_path}: {request}"
            );
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-goog-api-key: test-key")
            );
        }
        assert_eq!(catalog.page_count, 2);
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["catalog-first", "catalog-second"])
        );
        // expected_signature binds every variant to the exact directory that was actually requested.
        // expected_signature 将每个变体绑定到实际请求的精确目录。
        let expected_signature = crate::discovery_signature::discovery_signature(
            ModelDiscoveryKind::Gemini,
            &format!("{origin}{expected_path}"),
            &connection.credential_profile_id,
            &entry.authentication,
            &entry.request_headers,
        );
        for model in &catalog.models {
            assert_eq!(model.protocol_variants.len(), 3);
            assert!(
                model
                    .protocol_variants
                    .values()
                    .all(|variant| { variant.discovery_signature == expected_signature })
            );
        }
    }
}

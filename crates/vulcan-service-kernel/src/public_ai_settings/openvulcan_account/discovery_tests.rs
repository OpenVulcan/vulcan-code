//! Regional native-login discovery tests across HTTPS, SQLite and connection publication.
//! 跨 HTTPS、SQLite 和连接发布的区域原生登录发现测试。

use super::*;
use crate::ai_credential_secret_store::repair_tests::CredentialRepairFixture;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vulcan_ai_contract::{
    AiOperation, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent, EmbeddingOutputKind,
    EmbeddingRequest, EmbeddingTask, InferenceUsageScope,
};
use vulcan_provider_adapters::{
    CredentialSecretResolver, InferenceRouteBinding, InferenceRouteConfiguration,
};

/// Shared deadline for the bounded local server and its injected HTTP client.
/// 有界本地服务器及注入 HTTP 客户端共用的截止时限。
const DISCOVERY_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Bounds each HTTP header or request body handled by the private test server.
/// 约束私有测试服务器处理的每个 HTTP 头部或请求正文。
const TEST_HTTP_PART_MAX_BYTES: usize = 8192;

/// Takes no inputs and returns synthetic model facts in the published native wire format.
/// 无需参数，返回使用已发布原生线协议格式的合成模型事实。
fn catalog_body() -> String {
    serde_json::json!({
        "contract_version": "2026-08-30",
        "models": [
            {
                "model_id": "synthetic-vector",
                "display_name": "Synthetic Vector",
                "kind": "embedding",
                "capabilities": {
                    "max_input_tokens_per_item": 128,
                    "max_batch_size": 3,
                    "default_dimensions": 4,
                    "allowed_dimensions": [4, 8],
                    "allowed_tasks": ["retrieval_query", "retrieval_document"],
                    "allowed_output_kinds": ["dense"],
                    "allowed_encodings": ["float"]
                }
            },
            {
                "model_id": "synthetic-search",
                "display_name": "Synthetic Search",
                "kind": "embedding",
                "capabilities": {
                    "max_input_tokens_per_item": 128,
                    "max_batch_size": 3,
                    "default_dimensions": 4,
                    "allowed_dimensions": [4, 8],
                    "allowed_tasks": ["search_query", "search_document"],
                    "allowed_output_kinds": ["dense"],
                    "allowed_encodings": ["float"]
                }
            },
            {
                "model_id": "synthetic-decision",
                "display_name": "Synthetic Decision",
                "kind": "decision",
                "decision_capabilities": { "max_total_input_tokens": 256 }
            }
        ]
    })
    .to_string()
}

/// Returns synthetic usable material for `region`; no credential belongs to a real account.
/// 返回 `region` 的合成可用材料；所有凭据均不属于真实账户。
fn account_secret(region: OpenVulcanRegion) -> CredentialSecretEnvelope {
    CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenVulcanAuth {
        access_token: "a".repeat(43),
        authorization_id: uuid::Uuid::new_v4().to_string(),
        region,
        expires_at_unix_ms: 2_000_000_000_000,
        rotation_pending: false,
    })
    .expect("synthetic native credential")
}

/// Serves `status`/`body`, then validates each `(model, task)` in `embedding_calls`; returns the client and task.
/// 提供 `status`／`body`，随后校验 `embedding_calls` 中每个（模型，任务）；返回客户端与服务任务。
/// `region` keeps the real declared hostname and certificate validation while DNS points to loopback.
/// `region` 保留真实声明主机名及证书校验，同时将 DNS 指向回环地址。
async fn catalog_transport(
    region: OpenVulcanRegion,
    status: u16,
    body: String,
    embedding_calls: Vec<(&'static str, &'static str)>,
) -> (reqwest::Client, tokio::task::JoinHandle<()>) {
    vulcan_provider_adapters::install_process_rustls_crypto_provider().expect("TLS provider");
    // host is derived from the same regional authority used by native credential projection.
    // host 从原生凭据投影使用的同一区域权威地址派生。
    let host = reqwest::Url::parse(region.auth_origin())
        .expect("regional URL")
        .host_str()
        .expect("regional hostname")
        .to_string();
    // api_host is separately declared by the same region and receives only inference requests.
    // api_host 由同一区域单独声明，且仅接收推理请求。
    let api_host = reqwest::Url::parse(region.api_origin())
        .expect("regional API URL")
        .host_str()
        .expect("regional API hostname")
        .to_string();
    // listener and address retain one private ephemeral socket throughout the request.
    // listener 和 address 在请求期间保留一个私有临时端口套接字。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local listener");
    let address = listener.local_addr().expect("local address");
    // key and certificate are generated solely for this hostname-bound test transport.
    // key 和 certificate 仅为当前绑定主机名的测试传输生成。
    let key = rcgen::KeyPair::generate().expect("fixture certificate key");
    let certificate = rcgen::CertificateParams::new(vec![host.clone(), api_host.clone()])
        .expect("fixture certificate parameters")
        .self_signed(&key)
        .expect("fixture certificate");
    // root extends only this client's trust store; TLS hostname verification remains enabled.
    // root 仅扩展此客户端的信任库；TLS 主机名校验保持启用。
    let root = reqwest::Certificate::from_der(certificate.der().as_ref()).expect("fixture root");
    // tls serves only the generated certificate without changing any production security settings.
    // tls 仅提供生成的证书，不改变任何生产安全设置。
    let tls = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.der().clone()],
            tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der())
                .expect("private key DER"),
        )
        .expect("fixture TLS");
    // client disables environment proxies and redirects and maps the declared authority to this socket.
    // client 禁用环境代理及重定向，将声明的授权服务映射至当前套接字。
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(root)
        .resolve(&host, address)
        .resolve(&api_host, address)
        .timeout(DISCOVERY_TEST_TIMEOUT)
        .build()
        .expect("local discovery client");
    // acceptor owns the exact TLS configuration shared by discovery and inference.
    // acceptor 持有发现与推理共用的准确 TLS 配置。
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    // server verifies the real outbound request and never forwards anything to an external host.
    // server 验证真实发出请求，绝不将任何内容转发至外部主机。
    let server = tokio::spawn(async move {
        tokio::time::timeout(DISCOVERY_TEST_TIMEOUT, async move {
            for embedding_call in std::iter::once(None).chain(embedding_calls.into_iter().map(Some)) {
            // socket and stream accept each ordered metadata or inference request.
            // socket 和 stream 接收每个有序元数据或推理请求。
            let (socket, _) = listener.accept().await.expect("accept discovery connection");
            let mut stream = acceptor.accept(socket).await.expect("accept authenticated TLS");
            // head reads only bounded HTTP headers and leaves any JSON body on the stream.
            // head 仅读取有界 HTTP 头，将可能存在的 JSON 正文保留在流中。
            let mut head = Vec::new();
            // byte prevents reading past the header terminator.
            // byte 防止读取越过头部终止符。
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).await.expect("read discovery header"), 1);
                head.push(byte[0]);
                assert!(head.len() <= TEST_HTTP_PART_MAX_BYTES, "fixture request headers must remain bounded");
            }
            // head is inspected without including its synthetic credential in failure diagnostics.
            // 检查 head 时不将其合成凭据写入失败诊断。
            let head = String::from_utf8(head).expect("HTTP headers");
            assert!(head.lines().any(|line| line.split_once(':').is_some_and(|(name, value)|
                name.eq_ignore_ascii_case("authorization") && value.trim() == format!("Auth {}", "a".repeat(43)))));
            // reply preserves the metadata failure or echoes a validated native vector response.
            // reply 保留元数据失败，或回显经过校验的原生向量响应。
            let (reply_status, reply_body) = match embedding_call {
                None => {
                    assert!(head.starts_with("GET /api/v1/vulcancode/models HTTP/1.1\r\n"));
                    assert!(head.lines().any(|line| line.split_once(':').is_some_and(|(name, value)|
                        name.eq_ignore_ascii_case("host") && value.trim() == host)));
                    (status, body.clone())
                }
                Some((model_id, wire_task)) => {
                    assert!(head.starts_with("POST /api/v1/vulcancode/embeddings HTTP/1.1\r\n"));
                    assert!(head.lines().any(|line| line.split_once(':').is_some_and(|(name, value)|
                        name.eq_ignore_ascii_case("host") && value.trim() == api_host)));
                    // length bounds the exact JSON body before any allocation or decoding.
                    // length 在任何分配或解码之前限制准确的 JSON 正文大小。
                    let length = head.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().expect("body length"))
                    }).expect("native JSON body length");
                    assert!(length <= TEST_HTTP_PART_MAX_BYTES);
                    // encoded and request retain the actual serialized production request for contract checks.
                    // encoded 和 request 保留实际序列化的生产请求，用于契约检查。
                    let mut encoded = vec![0_u8; length];
                    stream.read_exact(&mut encoded).await.expect("native request body");
                    let request: serde_json::Value = serde_json::from_slice(&encoded).expect("native JSON");
                    assert_eq!(request["model_id"], model_id);
                    assert_eq!(request["task"], wire_task);
                    assert_eq!(request["inputs"].as_array().expect("text inputs").len(), 1);
                    assert_eq!(request["dimensions"], 4);
                    (200, serde_json::json!({
                        "contract_version": request["contract_version"],
                        "request_id": request["request_id"],
                        "model_id": request["model_id"],
                        "dimensions": 4,
                        "output_kind": "dense",
                        "encoding": "float",
                        "data": [{"index": 0, "dense": [1.0, 0.0, 0.0, 0.0]}],
                        "usage": {"input_tokens": 1, "charged_micro_points": 1, "ledger": "free"}
                    }).to_string())
                }
            };
            // response frames the complete controlled reply and closes the private connection.
            // response 对完整受控回复分帧，并关闭私有连接。
            let response = format!("HTTP/1.1 {reply_status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply_body}", reply_body.len());
            stream.write_all(response.as_bytes()).await.expect("write discovery response");
            let _ = stream.shutdown().await;
            }
        }).await.expect("bounded catalog exchange");
    });
    (client, server)
}

/// Builds an isolated bridge around `runtime`; returns the real shared service state without persistence work.
/// 围绕 `runtime` 构建隔离桥接，返回真实共享服务状态，不执行持久化工作。
fn service_state(runtime: Arc<PublicAiRuntime>) -> SharedServiceBridgeState {
    // bridge publishes through the production runtime and its signed provider generation.
    // bridge 通过生产运行时及其签名供应商代际执行发布。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.public_ai_runtime = runtime;
    // state owns only a synthetic session and leaves project snapshot publication disabled.
    // state 仅持有合成会话，并关闭工程快照发布。
    let mut state =
        crate::tests::test_support::test_service_state_with_parent_session(uuid::Uuid::new_v4());
    state.agent_runtime = Some(bridge);
    state.workspace_snapshot = None;
    Arc::new(std::sync::Mutex::new(state))
}

/// With no inputs, verifies both regions persist and publish a login through real HTTPS discovery.
/// 无需参数，验证两区域通过真实 HTTPS 发现保存并发布登录，成功时正常返回。
#[tokio::test]
async fn openvulcan_login_discovers_and_publishes_both_regions() {
    for (region, entry_id) in [
        (OpenVulcanRegion::China, "china"),
        (OpenVulcanRegion::Global, "global"),
    ] {
        // fixture isolates the actual SQLite store and all connection configuration files.
        // fixture 隔离实际 SQLite 仓库与全部连接配置文件。
        let fixture = CredentialRepairFixture::new().await;
        // http and server replace only network transport, preserving the original bundled manifest.
        // http 和 server 仅替换网络传输，保留原始随包清单。
        let (http, server) = catalog_transport(
            region,
            200,
            catalog_body(),
            vec![
                ("synthetic-vector", "retrieval_query"),
                ("synthetic-vector", "retrieval_document"),
                ("synthetic-search", "search_query"),
                ("synthetic-search", "search_document"),
            ],
        )
        .await;
        // runtime and state exercise the same connection transaction invoked by account commit.
        // runtime 和 state 执行账户提交调用的同一连接事务。
        let runtime = Arc::new(
            PublicAiRuntime::first_batch_with_http_client_for_testing(fixture.store.clone(), http)
                .expect("native runtime"),
        );
        let state = service_state(runtime.clone());
        // secret contains no live account material.
        // secret 不包含真实账户材料。
        let secret = account_secret(region);
        crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
            .scope(fixture.directory.path().to_path_buf(), async {
                // saved is the browser-facing result of the real post-authorization save transaction.
                // saved 是真实授权后保存事务返回的浏览器结果。
                let saved = add_public_ai_connection_with_secret(
                    ServiceEventEmitter::new(Vec::new()),
                    state,
                    "openvulcan".into(),
                    entry_id.into(),
                    "account".into(),
                    CredentialMode::Auth,
                    "Synthetic account".into(),
                    secret.clone(),
                    None,
                    None,
                )
                .await
                .expect("native login must discover models before saving");
                // configuration is read from disk, independently of the in-memory activation state.
                // configuration 从磁盘读取，独立于内存激活状态。
                let configuration =
                    vulcan_provider_config::config::load_public_ai_connections_file_at_path(
                        &fixture.directory.path().join("ai-connections.toml"),
                    )
                    .expect("persisted account")
                    .configuration;
                assert_eq!(configuration.connection_graph.connections.len(), 1);
                // connection is located by its stable provider and service identities, never a list position.
                // connection 按稳定供应商及入口身份定位，不使用列表位置。
                let connection = configuration
                    .connection_graph
                    .connections
                    .values()
                    .find(|connection| {
                        connection.provider_id == "openvulcan"
                            && connection.service_entry_id == entry_id
                    })
                    .expect("saved regional account");
                assert!(
                    fixture
                        .store
                        .read_secret(&connection.secret_reference)
                        .await
                        .expect("stored account material")
                        == secret
                );
                assert_eq!(fixture.record_count(), 1);
                // catalogs contain effective models after the complete manifest overlay and validation pipeline.
                // catalogs 包含经过完整清单覆盖与校验管线后的生效模型。
                let catalogs = runtime.catalog_snapshot().expect("effective catalogs");
                let catalog = catalogs
                    .get(&connection.id)
                    .expect("published regional catalog");
                for (model_id, operation) in [
                    ("synthetic-vector", AiOperation::EmbeddingCreate),
                    ("synthetic-search", AiOperation::EmbeddingCreate),
                    ("synthetic-decision", AiOperation::DecisionEvaluate),
                ] {
                    // model must retain the native operation and its executable protocol binding.
                    // model 必须保留原生操作及其可执行协议绑定。
                    let model = catalog
                        .models
                        .iter()
                        .find(|model| model.id == model_id)
                        .expect("discovered native model must remain published");
                    assert!(model.capabilities.operations.contains(&operation));
                    assert!(
                        !model
                            .capabilities
                            .operations
                            .contains(&AiOperation::ConversationRespond)
                    );
                    assert!(model.resolved_protocol_binding.is_some());
                }
                // published verifies the same browser snapshot used to expose the core system provider.
                // published 验证用于显示核心系统供应商的同一浏览器快照。
                let published = serde_json::to_value(saved).expect("browser snapshot");
                assert!(
                    published["providers"]
                        .as_array()
                        .expect("providers")
                        .iter()
                        .any(|provider| provider["id"] == "openvulcan")
                );
                assert!(
                    published["catalogErrors"]
                        .as_object()
                        .expect("catalog errors")
                        .is_empty()
                );
                verify_embedding_tasks(&runtime, &configuration, connection).await;
                server
                    .await
                    .expect("actual regional discovery and inference requests");
            })
            .await;
    }
}

/// Executes both semantic tasks through each discovered model in `configuration` and `connection`.
/// 通过 `configuration` 和 `connection` 中每个发现模型执行两种语义任务。
/// `runtime` uses the private transport; returns normally only after native response validation succeeds.
/// `runtime` 使用私有传输；仅在原生响应校验成功后正常返回。
async fn verify_embedding_tasks(
    runtime: &PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    connection: &vulcan_provider_adapters::CredentialConnection,
) {
    for model_id in ["synthetic-vector", "synthetic-search"] {
        // route chooses a native operation without assigning a conversation model or changing user settings.
        // route 选择原生操作，不指定对话模型或改变用户设置。
        let route = InferenceRouteConfiguration {
            provider_id: connection.provider_id.clone(),
            service_entry_id: connection.service_entry_id.clone(),
            connection_id: connection.id.clone(),
            model_id: model_id.into(),
            binding: InferenceRouteBinding::Action {
                action_binding_id: "openvulcan.embeddings.v1".into(),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        // selection passes through production protocol freezing and retains discovery task evidence.
        // selection 通过生产协议冻结流程，并保留发现任务证据。
        let selection = runtime
            .resolve_internal_inference_selection(configuration, &route)
            .expect("native embedding selection");
        for task in [EmbeddingTask::Query, EmbeddingTask::Document] {
            // request uses host semantics, which the outgoing wire assertion independently checks against the catalog.
            // request 使用宿主语义，发出请求的线协议断言会独立将其与目录核对。
            let request = EmbeddingRequest {
                contract_version: vulcan_ai_contract::INFERENCE_CONTRACT_VERSION,
                request_id: uuid::Uuid::new_v4().to_string(),
                idempotency_key: None,
                consumer_profile_id: "catalog.fixture".into(),
                inputs: vec![EmbeddingInput {
                    input_id: "fixture-input".into(),
                    content: EmbeddingInputContent::Text {
                        text: "synthetic input".into(),
                    },
                }],
                task,
                dimensions: Some(4),
                output_kind: EmbeddingOutputKind::Dense,
                encoding: EmbeddingEncoding::Float,
                invalid_input_policy: Default::default(),
            };
            // response is produced only after real request projection, HTTPS exchange and native decoding.
            // response 仅在真实请求投影、HTTPS 交换和原生解码之后产生。
            let response = runtime
                .execute_internal_embedding_with_wire_request_capture(
                    &selection,
                    &request,
                    route.usage_scope,
                    None,
                )
                .await
                .expect("native embedding exchange");
            assert_eq!(response.request_id, request.request_id);
            assert_eq!(response.fingerprint.task, task);
            assert_eq!(response.results.len(), request.inputs.len());
        }
    }
}

/// With no inputs, verifies rejected discovery leaves neither a saved connection nor an orphan secret.
/// 无需参数，验证被拒绝的发现不会留下已保存连接或孤立凭据，成功时正常返回。
#[tokio::test]
async fn openvulcan_login_discovery_failure_rolls_back_storage() {
    for (region, entry_id, status, body) in [
        (
            OpenVulcanRegion::China,
            "china",
            401,
            String::from("{\"error\":\"unauthorized\"}"),
        ),
        (
            OpenVulcanRegion::Global,
            "global",
            200,
            String::from("{\"contract_version\":\"unsupported\",\"models\":[]}"),
        ),
    ] {
        // fixture retains an empty baseline against which compensation is checked.
        // fixture 保留空基线，用于检查补偿结果。
        let fixture = CredentialRepairFixture::new().await;
        // http and server emit an actual provider rejection or invalid native contract.
        // http 和 server 发出真实供应商拒绝或无效原生契约响应。
        let (http, server) = catalog_transport(region, status, body, Vec::new()).await;
        // runtime and state keep production credential validation and publication enabled.
        // runtime 和 state 保持生产凭据校验及发布逻辑启用。
        let runtime = Arc::new(
            PublicAiRuntime::first_batch_with_http_client_for_testing(fixture.store.clone(), http)
                .expect("native runtime"),
        );
        let state = service_state(runtime.clone());
        crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
            .scope(fixture.directory.path().to_path_buf(), async {
                assert!(
                    add_public_ai_connection_with_secret(
                        ServiceEventEmitter::new(Vec::new()),
                        state,
                        "openvulcan".into(),
                        entry_id.into(),
                        "account".into(),
                        CredentialMode::Auth,
                        "Synthetic rejected account".into(),
                        account_secret(region),
                        None,
                        None,
                    )
                    .await
                    .is_err()
                );
                server
                    .await
                    .expect("failed discovery still reaches the regional authority");
                assert_eq!(fixture.record_count(), 0);
                assert!(
                    !fixture
                        .directory
                        .path()
                        .join("ai-connections.toml")
                        .exists()
                );
                assert!(
                    runtime
                        .configuration_snapshot()
                        .expect("unchanged configuration")
                        .connection_graph
                        .connections
                        .is_empty()
                );
                assert!(
                    runtime
                        .catalog_snapshot()
                        .expect("unchanged catalogs")
                        .is_empty()
                );
            })
            .await;
    }
}

//! Verifies instructions and input projection on manifest-selected Responses routes.
//! 验证清单所选 Responses 线路的指令和输入投影。

use crate::execution::test_support::{
    bundled_manifest_route, minimal_request, read_test_http_request,
};
use crate::runtime::test_support::{install_source_catalog, with_protocol_variants};
use crate::{
    CredentialSecretEnvelope, CredentialSecretMaterial, OAuthTokenSet, PersistedAiConfiguration,
    ProviderExecutionClient, PublicAiRuntime, ResponsesRouteFeatures, ResponsesWebsocketPolicy,
    parse_openai_model_catalog,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, net::TcpListener};
use vulcan_ai_contract::{
    AiMessage, AiStreamEvent, ContentItem, MessageRole, ResourceKind, ResourceReference,
    ResourceSourceKind,
};
use vulcan_llm_core::{
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry,
};

/// Returns a single synthetic text message with the supplied author and unmodified text.
/// 根据指定角色及原始文本返回一条合成消息。
fn message(role: MessageRole, text: &str) -> AiMessage {
    AiMessage {
        role,
        content: vec![ContentItem::Text { text: text.into() }],
        provider_context_items: Vec::new(),
    }
}

/// Reads one complete HTTP body from `listener`, acknowledges the request and returns the parsed JSON.
/// 从 `listener` 读取一次完整 HTTP 正文，确认请求并返回解析后的 JSON。
async fn receive_responses_body(listener: Arc<TcpListener>) -> Value {
    // The peer receives the final serialized request after credentials and every projector were applied.
    // 此接收端读取经过凭据与全部投影处理的最终序列化请求。
    let (mut socket, _) = listener.accept().await.unwrap();
    // Read the full HTTP frame, including its declared body length.
    // 读取完整 HTTP 请求及其声明长度的正文。
    let request = read_test_http_request(&mut socket).await;
    // Split the HTTP framing from the JSON payload at the protocol delimiter.
    // 按协议分隔符分离 HTTP 头和 JSON 正文。
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /responses HTTP/"));
    assert!(!headers.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("upgrade"))
    }));
    // The synthetic upstream completes the real transport stream without a live account.
    // 合成上游完成真实传输流，无需使用线上账号。
    let event =
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_fixture\"}}\n\n";
    // Return a successful SSE response to finish the physical request.
    // 返回成功的 SSE 响应以完成物理请求。
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{event}",
        event.len()
    );
    socket.write_all(response.as_bytes()).await.unwrap();
    serde_json::from_str(body).unwrap()
}

/// Registers the bundled `entry` and `protocol` with synthetic discovery for `models`. Returns its selectable runtime and configuration.
/// 用合成的 `models` 目录注册随包 `entry` 与 `protocol`，返回可选择的运行时及配置。
fn model_fixture(
    entry: &str,
    protocol: &str,
    models: &[&str],
) -> (PublicAiRuntime, PersistedAiConfiguration) {
    // Use the same bundled entry and credential profile as the production selector.
    // 使用与生产选择器相同的随包入口和凭据档。
    let runtime = PublicAiRuntime::unavailable().unwrap();
    // Keep account and model selections local to this fixture.
    // 将账号和模型选择仅保存在本夹具中。
    let mut config = PersistedAiConfiguration::default();
    // Use the credential profile declared by the bundled service.
    // 使用随包服务声明的凭据档。
    let mut account = bundled_manifest_route("openai", entry, protocol).connection;
    account.id = "fixture".into();
    // Use the matching discovery schema so the plan supplies only evidence present in its account catalog.
    // 使用对应的发现结构，使套餐只提供其账号目录实际包含的证据。
    let discovered = if entry == crate::OPENAI_CHATGPT_PLAN_ENTRY_ID {
        // Reproduce visible entries from the account-scoped plan directory.
        // 复现套餐账号目录中的可见条目。
        let discovery = json!({"models": models.iter().map(|id| json!({
            "slug": id, "display_name": id, "visibility": "list"
        })).collect::<Vec<_>>()});
        // Raw discovery proves visibility but does not supply the model's input modalities.
        // 原始发现证明可见性，但不会提供模型的输入模态。
        let mut catalog = crate::discovery::parse_openai_chatgpt_plan_catalog(
            &account.id,
            1,
            &discovery.to_string(),
        )
        .unwrap();
        // Apply the metadata client's manifest enrichment before publishing protocol variants.
        // 发布协议变体前应用元数据客户端的清单补全逻辑。
        let manifests = runtime.bundled_manifests.read().unwrap();
        crate::discovery::enrich_compatible_model_catalog_with_manifest(
            &account.provider_id,
            entry,
            Some(&account.credential_profile_id),
            protocol,
            &mut catalog,
            manifests.manifest(&account.provider_id).unwrap(),
        );
        catalog
    } else {
        // Reproduce ID-only OpenAI catalog entries used by the other selector tests.
        // 复现其他选择测试使用的仅含 ID 的 OpenAI 目录条目。
        let discovery =
            json!({"data": models.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()});
        parse_openai_model_catalog(&account.id, 1, &discovery.to_string()).unwrap()
    };
    config
        .connection_graph
        .add_connection(&runtime.providers().unwrap(), account)
        .unwrap();
    // Install discovery evidence for the protocol this endpoint actually speaks.
    // 为此端点实际使用的协议安装发现证据。
    let catalog = with_protocol_variants(discovered, &[protocol]);
    install_source_catalog(&runtime, catalog.clone());
    runtime.replace_catalog(catalog).unwrap();
    for model in models {
        config.set_model_enabled("openai", model, true);
    }
    runtime.reapply_model_catalog_overlays(&config).unwrap();
    (runtime, config)
}

/// Verifies both declared public models across requests with and without a one-shot instruction.
/// 验证两个公开模型在含有和省略单次指令的连续请求中的实际行为。
#[tokio::test]
async fn public_responses_instruction_placement_on_the_wire() {
    // The older public rule provides a control on the same account and endpoint.
    // 旧版公开模型规则提供同一账号与端点上的对照。
    let models = ["gpt-6.2-sol", "gpt-6.2-astra", "gpt-6.1-sol"];
    // Exercise the production model binding for this service entry.
    // 执行此服务入口的生产模型绑定。
    let (runtime, config) = model_fixture("public-api", OPENAI_RESPONSES_PROFILE_ID, &models);
    // Only the endpoint is redirected; the selected transport, encoding and model contract stay live.
    // 仅重定向服务地址，使用实际选中的传输、编码与模型契约。
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.unwrap());
    // Change only the network destination of the selected route.
    // 仅修改所选线路的网络目的地。
    let origin = format!("http://{}", listener.local_addr().unwrap());
    // Use the production encoder and transport with a direct loopback connection.
    // 使用生产编码器和传输直连本地回环服务。
    let client = ProviderExecutionClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    // Use a synthetic credential of the type required by the selected service.
    // 使用所选服务要求的凭据类型构造测试身份。
    let secret = CredentialSecretEnvelope::new_api_key("fixture-key".into())
        .unwrap()
        .execution_secret()
        .unwrap();
    for model in models {
        // Resolve each model through the active manifest and its selected account.
        // 通过活动清单和所选账号解析每个模型。
        let mut route = runtime
            .resolve_connection_model_selection(&config, "fixture", "openai", model)
            .unwrap()
            .route;
        route.protocol_route.base_url_override = Some(origin.clone());
        // Historical capability fields cannot select a transport after removal.
        // 移除传输实现后，历史能力字段无法选择传输方式。
        route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
            websocket: ResponsesWebsocketPolicy::Verified,
            recovery: Default::default(),
        });
        // Keep messages independent of the optional instruction for a single dispatch.
        // 让消息历史独立于单次分派的可选指令。
        let mut request = minimal_request();
        request.model_id = model.into();
        request.messages = vec![
            message(MessageRole::System, "fixed-a"),
            message(MessageRole::System, "fixed-b"),
            message(MessageRole::Developer, "developer-history"),
            message(MessageRole::User, "user-input"),
        ];
        for once in [None, Some("this call only"), None] {
            request.request_instructions = once.map(str::to_owned);
            if model == "gpt-6.1-sol" && once.is_some() {
                assert!(client.encode_request(&route, &request).is_err());
                continue;
            }
            // Read the transmitted body at the receiving end of the connection.
            // 在连接的接收端读取实际发送的正文。
            let peer = tokio::spawn(receive_responses_body(listener.clone()));
            // Consume the production stream through its terminal event.
            // 消费生产响应流直到终态事件。
            let mut events = client.execute(
                route.clone(),
                request.clone(),
                secret.clone(),
                "wire-fixture".into(),
            );
            while !matches!(
                events.next().await.expect("missing completion").unwrap(),
                AiStreamEvent::ResponseCompleted
            ) {}
            // Inspect the JSON received by the peer.
            // 检查接收端收到的 JSON。
            let body = peer.await.unwrap();
            assert_eq!(body["model"], model);
            if model == "gpt-6.1-sol" {
                assert_eq!(body["instructions"], "fixed-a\n\nfixed-b");
                assert_eq!(body["input"][0]["content"][0]["text"], "developer-history");
                continue;
            }
            assert_eq!(body.get("instructions"), once.map(Value::from).as_ref());
            // Read the ordered input array from the transmitted body.
            // 从已发送正文读取有序输入数组。
            let input = body["input"].as_array().unwrap();
            assert_eq!(input.len(), 3);
            // Compare the full message order including pre-existing developer guidance.
            // 核对完整消息顺序，包括已有开发者指引。
            let roles_and_text: Vec<_> = input
                .iter()
                .map(|item| {
                    (
                        item["role"].as_str().unwrap(),
                        item["content"][0]["text"].as_str().unwrap(),
                    )
                })
                .collect();
            assert_eq!(
                roles_and_text,
                [
                    ("developer", "fixed-a\n\nfixed-b"),
                    ("developer", "developer-history"),
                    ("user", "user-input")
                ]
            );
        }
    }
}

/// Verifies a saved Codex account honors its toggle and sends native GPT-6.2 requests over HTTP SSE.
/// 验证已保存的 Codex 账号尊重启停状态，并以 HTTP SSE 发送原生 GPT-6.2 请求。
#[tokio::test]
async fn saved_codex_account_executes_over_http_responses() {
    // Resolve the model through the bundled Codex entry and existing account graph.
    // 通过随包 Codex 入口和已有账号图解析模型。
    let model = "gpt-6.2-sol";
    let (runtime, mut config) = model_fixture(
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
        &[model],
    );
    assert!(config.connection_is_enabled("fixture"));
    config.set_connection_enabled("fixture", false).unwrap();
    runtime.reapply_model_catalog_overlays(&config).unwrap();
    assert!(
        runtime
            .resolve_connection_model_selection(&config, "fixture", "openai", model)
            .is_err()
    );
    config.set_connection_enabled("fixture", true).unwrap();
    runtime.reapply_model_catalog_overlays(&config).unwrap();
    // Redirect physical traffic to a local peer while retaining the selected protocol and model.
    // 把真实网络流量重定向至本地，同时保留所选协议及模型。
    let mut route = runtime
        .resolve_connection_model_selection(&config, "fixture", "openai", model)
        .unwrap()
        .route;
    assert_eq!(
        route.protocol_route.protocol_profile_id,
        OPENAI_CODEX_RESPONSES_PROFILE_ID
    );
    // Frozen routes from past requests may still carry the former capability flag.
    // 过去请求冻结的路由仍可能携带原有的能力标志。
    route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
        websocket: ResponsesWebsocketPolicy::Verified,
        recovery: Default::default(),
    });
    // Serve one complete stream so the test observes execution, not just encoding.
    // 提供完整响应流，使测试覆盖实际执行及编码。
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.unwrap());
    route.protocol_route.base_url_override =
        Some(format!("http://{}", listener.local_addr().unwrap()));
    // Use a synthetic native OAuth session with a Codex account identifier.
    // 使用带有 Codex 账号标识的合成原生 OAuth 会话。
    let secret = CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenAiCodexOAuth {
        tokens: OAuthTokenSet {
            access_token: "fixture-access".into(),
            refresh_token: "fixture-refresh".into(),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: Default::default(),
            account_subject: None,
            email: None,
        },
        id_token: None,
        account_id: Some("fixture-account".into()),
    })
    .unwrap()
    .execution_secret()
    .unwrap();
    // Reuse the real encoder and execution client for the selected route.
    // 对所选路由复用真实编码器和执行客户端。
    let client = ProviderExecutionClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    // Supply instructions to distinguish the native Lite request from public Responses.
    // 提供指令以验证原生 Lite 请求的编码不同于公开 Responses。
    let mut request = minimal_request();
    request.model_id = model.into();
    request.cache_scope_id = Some("00000000-0000-0000-0000-000000000123".into());
    request.messages = vec![
        message(MessageRole::System, "fixture system"),
        message(MessageRole::User, "fixture user"),
    ];
    // The peer asserts that the connection starts with an HTTP POST without a protocol upgrade.
    // 接收端断言连接以普通 HTTP POST 开始，且不升级协议。
    let peer = tokio::spawn(receive_responses_body(listener));
    // Consume events through the successful terminal state.
    // 消费事件直至成功终态。
    let mut events = client.execute(route, request, secret, "codex-wire-fixture".into());
    while !matches!(
        events.next().await.expect("missing completion").unwrap(),
        AiStreamEvent::ResponseCompleted
    ) {}
    // Check the selected model and native Lite instructions contract on the wire.
    // 核对线上请求中的所选模型与原生 Lite 指令契约。
    let body = peer.await.unwrap();
    assert_eq!(body["model"], model);
    assert_eq!(body["stream"], true);
    assert!(body.get("instructions").is_none());
}

/// Verifies account-scoped model choices, GPT-6.2 prefix placement, and plan wire restrictions.
/// 验证账号专属的模型选择、GPT‑6.2 提示词前缀及套餐线路限制。
#[test]
fn chatgpt_plan_projects_stateless_responses_and_no_image_generation() {
    let account_models = crate::discovery::parse_openai_chatgpt_plan_catalog(
        "fixture",
        1,
        r#"{"models":[
            {"slug":"gpt-6.2-astra","display_name":"Astra","visibility":"list"},
            {"slug":"gpt-image-2.6-sunburst","display_name":"Image","visibility":"list"},
            {"slug":"gpt-6.1-sol","visibility":"hidden"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        account_models
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["gpt-6.2-astra"]
    );
    let (runtime, config) = model_fixture(
        crate::OPENAI_CHATGPT_PLAN_ENTRY_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        &["gpt-6.2-astra"],
    );
    let route = runtime
        .resolve_connection_model_selection(&config, "fixture", "openai", "gpt-6.2-astra")
        .unwrap()
        .route;
    assert_eq!(route.entry.base_url, crate::OPENAI_CHATGPT_PLAN_BASE_URL);
    assert!(route.responses_developer_prefix);
    assert!(
        runtime
            .resolve_connection_model_selection(&config, "fixture", "openai", "gpt-6.2-sol")
            .is_err()
    );
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    let mut request = minimal_request();
    request.model_id = "gpt-6.2-astra".into();
    request
        .messages
        .insert(0, message(MessageRole::System, "fixed-system"));
    request.request_instructions = Some("only-this-call".into());
    request.tools.push(
        serde_json::from_value(json!({
            "name": "list_files",
            "description": "List files in the workspace.",
            "input_schema": {"type": "object"}
        }))
        .unwrap(),
    );
    let encoded = client.encode_request(&route, &request).unwrap();
    assert_eq!(encoded.body["store"], false);
    assert_eq!(encoded.body["stream"], true);
    assert_eq!(encoded.body["input"][0]["role"], "developer");
    assert_eq!(encoded.body["instructions"], "only-this-call");
    assert_eq!(encoded.body["tools"][0]["type"], "namespace");
    assert_eq!(encoded.body["tools"][0]["tools"][0]["name"], "list_files");
    assert!(encoded.body.get("max_output_tokens").is_none());
    assert!(encoded.body.get("previous_response_id").is_none());
}

/// Verifies that discovered plan models expose their declared input modalities and serialize inline attachments.
/// 验证套餐发现的模型公开已声明的输入模态，并将附件序列化为内联输入。
#[test]
fn chatgpt_plan_discovery_enables_declared_image_and_file_inputs() {
    // Discovery includes the affected older models, the existing GPT-6.2 rules and one unrecognized model.
    // 发现包含受影响的旧模型、现有 GPT-6.2 规则和一个未知模型。
    let models = [
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-5.6-luna",
        "gpt-6-astra",
        "gpt-6-sol",
        "gpt-6-luna",
        "gpt-6.1-sol",
        "gpt-6.1-terra",
        "gpt-6.2-sol",
        "gpt-6.2-astra",
    ];
    // An account-visible ID alone carries no evidence about additional modalities.
    // 账号可见的 ID 本身不提供额外模态的证据。
    let unknown_model = "unrecognized-plan-model";
    // Install the account's entire visible directory before applying exact rules.
    // 在应用精确规则前安装该账号的完整可见目录。
    let discovered_models = [models.as_slice(), &[unknown_model]].concat();
    // The test exercises the bundled provider manifest with the production selector.
    // 测试通过生产选择器使用随包供应商清单。
    let (runtime, config) = model_fixture(
        crate::OPENAI_CHATGPT_PLAN_ENTRY_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        &discovered_models,
    );
    // Check the capability snapshot that feeds the account model directory.
    // 检查账号模型目录使用的能力快照。
    let unknown = runtime
        .resolve_connection_model_selection(&config, "fixture", "openai", unknown_model)
        .unwrap();
    assert!(unknown.model.capabilities.input_resources.is_empty());

    // Encode actual plan request bodies without using a live account.
    // 使用套餐的真实请求编码构造正文，无需在线账号。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    // Carry one text part and two request-local resources supplied by the host.
    // 携带一条文本及两个由宿主提供的本次请求资源。
    let mut prompt = message(MessageRole::User, "Review both attachments.");
    for (kind, mime, filename) in [
        (ResourceKind::Image, "image/png", "image.png"),
        (ResourceKind::File, "application/pdf", "document.pdf"),
    ] {
        prompt.content.push(ContentItem::Resource {
            resource: ResourceReference {
                id: filename.into(),
                kind,
                media_type: Some(mime.into()),
                source_kind: ResourceSourceKind::InlineData,
                file_name: Some(filename.into()),
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(format!("data:{mime};base64,AA==")),
            },
        });
    }
    // Reuse the same input across the affected models to isolate catalog and route differences.
    // 对相关模型复用同一份输入，以隔离目录和线路差异。
    let mut request = minimal_request();
    request.messages = vec![prompt];
    for model in models {
        // Resolve the model as a conversation and as the account directory would see it.
        // 按会话与账号目录看到的结果解析模型。
        let selected = runtime
            .resolve_connection_model_selection(&config, "fixture", "openai", model)
            .unwrap();
        for kind in [ResourceKind::Image, ResourceKind::File] {
            assert!(
                selected.model.capabilities.input_resources.contains(&kind),
                "ChatGPT Plan model {model} is missing its declared {kind:?} input"
            );
        }
        request.model_id = model.into();
        // Inspect the fully projected provider content for the user message.
        // 检查用户消息完整投影后的供应商内容。
        let encoded = client.encode_request(&selected.route, &request).unwrap();
        // Find the input by role rather than relying on a prefix's exact length.
        // 按角色查找输入，不依赖前缀的固定长度。
        let parts = encoded.body["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["role"] == "user")
            .unwrap()["content"]
            .as_array()
            .unwrap();
        assert!(parts.contains(&json!({"type": "input_text", "text": "Review both attachments."})));
        assert!(parts.contains(&json!({
            "type": "input_image", "image_url": "data:image/png;base64,AA=="
        })));
        assert!(parts.contains(&json!({
            "type": "input_file", "file_data": "data:application/pdf;base64,AA==", "filename": "document.pdf"
        })));
    }
}

//! Regression coverage for compact request fields and replayable checkpoints.
//! 压缩请求字段与可重放检查点的回归覆盖。

use super::super::test_support::{bundled_manifest_route, minimal_request};
use super::*;
use serde_json::json;
use vulcan_ai_contract::{APPLY_PATCH_TOOL_NAME, ToolDefinition};

/// Runs the real single-attempt compact transport against a gated loopback response.
/// 对带正文闸门的环回响应运行真实单次压缩传输。
///
/// `status` selects success or unauthorized response; returns after checking wire identity,
/// header-before-body delivery, secret redaction, and the absence of a hidden retry.
/// `status` 选择成功或未授权响应；验证线路身份、先头后体、秘密脱敏及无隐藏重试后返回。
async fn verify_compact_attempt_boundary(status: u16) {
    use super::super::super::test_support::read_test_http_request;
    use std::sync::Arc;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use vulcan_ai_protocols::ProtocolRegistry;

    // listener is the only reachable transport endpoint in this test.
    // listener 是本测试唯一可达的传输端点。
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    // address scopes the immutable route to the isolated socket.
    // address 将不可变路由限定到隔离 Socket。
    let address = listener.local_addr().unwrap();
    // releaseBody ensures header observation cannot depend on receiving the unary result.
    // releaseBody 确保响应头观测不依赖接收一元结果。
    let (release_body, body_ready) = oneshot::channel::<()>();
    // server independently captures bytes and withholds the body until the host acknowledges headers.
    // server 独立捕获字节，并在宿主确认响应头前扣留正文。
    let server = tokio::spawn(async move {
        // socket owns the sole physical request accepted by this server.
        // socket 持有该服务器接受的唯一物理请求。
        let (mut socket, _) = listener.accept().await.unwrap();
        // received records what was actually sent after the trace acknowledgement.
        // received 记录追踪确认后真正发送的内容。
        let received = read_test_http_request(&mut socket).await;
        // body deliberately echoes a fake secret on failure to exercise production redaction.
        // body 在失败时故意回显假秘密，以验证生产脱敏。
        let body = if status == 200 {
            json!({"id":"compact-result", "output":[{"type":"compaction", "encrypted_content":"opaque"}], "usage":{"input_tokens":3}}).to_string()
        } else {
            json!({"error":{"message":"rejected compact-fake-token"}}).to_string()
        };
        socket.write_all(format!(
            "HTTP/1.1 {status} Test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: compact-attempt\r\nconnection: close\r\n\r\n", body.len(),
        ).as_bytes()).await.unwrap();
        body_ready.await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        (received, listener)
    });
    // client uses production encoding, credential projection, HTTP and bounded unary decoding.
    // client 使用生产编码、凭据投影、HTTP 及有界一元解码。
    let client = ProviderExecutionClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    // route retains real native Codex declarations while replacing only its endpoint.
    // route 保留真实原生 Codex 声明，仅替换端点。
    let mut route = bundled_manifest_route(
        "openai",
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    );
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    // secret is synthetic OAuth material; no credential store or refresh endpoint is accessed.
    // secret 是合成 OAuth 材料，不访问凭据仓库或刷新端点。
    let secret =
        crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenAiCodexOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: String::from("compact-fake-token"),
                refresh_token: String::from("compact-fake-refresh"),
                issued_at_unix_ms: 1,
                expires_at_unix_ms: 2,
                scopes: Default::default(),
                account_subject: None,
                email: None,
            },
            id_token: None,
            account_id: Some(String::from("compact-fake-account")),
        })
        .unwrap()
        .execution_secret()
        .unwrap();
    // captureReceiver acknowledges final wire tracing independently of header observation.
    // captureReceiver 独立于响应头观测确认最终线路追踪。
    let (capture_sender, capture_receiver) = oneshot::channel::<ProviderWireRequestTraceDispatch>();
    // captureTask retains the exact body for comparison with bytes observed by the server.
    // captureTask 保留精确正文以便与服务器观察到的字节比较。
    let capture_task = tokio::spawn(async move {
        // dispatch carries the actual compact endpoint and filtered final request.
        // dispatch 携带实际 compact 端点及过滤后的最终请求。
        let dispatch = capture_receiver.await.unwrap();
        dispatch.persistence_result.send(Ok(())).unwrap();
        dispatch.request
    });
    // stream exposes one attempt and has no access to OAuth renewal.
    // stream 暴露一次尝试且无法访问 OAuth 续期。
    let mut stream = client.stream_responses_compact(
        route,
        minimal_request(),
        secret,
        Some(capture_sender),
        // This fixture observes the request wire boundary only; paired artifact capture is covered
        // by the session-owned compaction trace regression.
        // 本夹具只观察请求线路边界；成对制品捕获由会话持有的压缩追踪回归覆盖。
        None,
    );
    // observed must arrive while the response body is still unavailable.
    // observed 必须在响应正文仍不可用时到达。
    let observed = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match observed {
        ResponsesCompactEvent::TransportObserved {
            dispatched_at_ms,
            headers_at_ms,
            http_status,
            provider_request_id,
            ..
        } => {
            assert!(headers_at_ms >= dispatched_at_ms);
            assert_eq!(http_status, status);
            assert_eq!(provider_request_id.as_deref(), Some("compact-attempt"));
        }
        ResponsesCompactEvent::Completed(_) => panic!("headers must precede the native result"),
    }
    release_body.send(()).unwrap();
    // terminal is either the real native result or the original physical failure.
    // terminal 是真实原生结果或原始物理失败。
    let terminal = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .unwrap()
        .unwrap();
    if status == 200 {
        // result contains opaque output and usage without a synthetic assistant message.
        // result 包含不透明输出和用量，不生成虚构助手消息。
        let ResponsesCompactEvent::Completed(result) = terminal.unwrap() else {
            panic!("one terminal native result expected")
        };
        assert_eq!(
            result.output_items,
            vec![json!({"type":"compaction", "encrypted_content":"opaque"})]
        );
        assert_eq!(result.usage, Some(json!({"input_tokens":3})));
        assert_eq!(result.response_id.as_deref(), Some("compact-result"));
    } else {
        // failure keeps dispatch and status facts while removing the fake secret.
        // failure 保留分派及状态事实，同时移除假秘密。
        let failure = terminal.unwrap_err();
        assert_eq!(failure.error.http_status, Some(status));
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
        assert!(!failure.error.message.contains("compact-fake-token"));
    }
    assert!(stream.next().await.is_none());
    // captured and received independently prove final filtering and endpoint selection.
    // captured 与 received 独立证明最终过滤和端点选择。
    let captured = capture_task.await.unwrap();
    let (received, listener) = server.await.unwrap();
    assert!(received.starts_with("POST /responses/compact HTTP/1.1\r\n"));
    assert_eq!(
        captured.body,
        serde_json::from_str::<Value>(received.split_once("\r\n\r\n").unwrap().1).unwrap()
    );
    assert!(captured.body.get("stream").is_none());
    assert!(captured.body.get("previous_response_id").is_none());
    assert_eq!(captured.transport, "responses_compact");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

/// Verifies real successful compact HTTP exposes headers before native result decoding.
/// 验证真实成功 compact HTTP 在原生结果解码前暴露响应头。
#[tokio::test]
async fn compact_attempt_stream_preserves_wire_and_native_result_boundaries() {
    verify_compact_attempt_boundary(200).await;
}

/// Verifies a physical 401 remains observable and never triggers a hidden second request.
/// 验证物理 401 保持可观测且不会触发隐藏的第二次请求。
#[tokio::test]
async fn compact_attempt_stream_reports_unauthorized_without_retry() {
    verify_compact_attempt_boundary(401).await;
}

/// Verifies compact encoding consumes the same route-owned Apply Patch contract as live streaming.
/// 验证 compact 编码与实时流请求消费同一份路由自有 Apply Patch 契约。
#[test]
fn codex_compact_projection_uses_route_owned_freeform_apply_patch_contract() {
    // route mirrors one native Lite model whose Apply Patch input is provider-declared freeform text.
    // route 模拟一个原生 Lite 模型，其 Apply Patch 输入由供应商声明为自由文本。
    let mut route = bundled_manifest_route(
        "openai",
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    );
    route.codex_behavior = Some(crate::provider_manifest::CodexModelBehavior {
        wire_dialect: crate::provider_manifest::CodexWireDialect::Lite,
        apply_patch_tool_type: crate::provider_manifest::CodexApplyPatchToolType::Freeform,
        ..Default::default()
    });
    // request starts from the canonical JSON-schema declaration used outside Codex Lite.
    // request 从 Codex Lite 之外使用的规范 JSON Schema 声明开始。
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("compact-session"));
    request.tools.push(ToolDefinition {
        name: String::from(APPLY_PATCH_TOOL_NAME),
        description: String::from("Apply one patch"),
        input_schema: json!({"type":"object","properties":{"patch":{"type":"string"}}}),
        input_format: Default::default(),
        strict: false,
    });

    // effectiveRequest is the exact immutable snapshot used by the production compact path.
    // effectiveRequest 是生产 compact 路径使用的精确不可变快照。
    let effective_request = request_with_codex_tool_input_contract(&route, &request);
    // encoded follows the production order through native encoding and provider projection.
    // encoded 按生产顺序经过原生编码和供应商投影。
    let encoded = encode_codex_responses_request(&effective_request)
        .expect("Codex compact request should encode");
    let mut encoded = project_provider_request(&route, &effective_request, encoded)
        .expect("Codex Lite compact request should project");
    prepare_responses_compact_body(&mut encoded.body)
        .expect("Codex Lite compact body should normalize");

    // tool is the exact Apply Patch declaration retained inside the Lite namespace prefix.
    // tool 是 Lite 命名空间前缀内保留的精确 Apply Patch 声明。
    let tool = &encoded.body["input"][0]["tools"][0]["tools"][0];
    assert_eq!(tool["type"], "custom");
    assert_eq!(tool["name"], APPLY_PATCH_TOOL_NAME);
    assert_eq!(tool["format"]["syntax"], "lark");
}

/// Verifies the compact request body follows the official Codex `CompactionInput` shape.
/// 验证 compact 请求体遵循官方 Codex `CompactionInput` 结构。
#[test]
fn codex_compact_body_removes_stream_only_and_unknown_fields() {
    let mut body = json!({
        "model": "gpt-codex",
        "input": [],
        "instructions": "",
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "previous_response_id": "resp-old",
        "unsupported_vendor_field": true,
        "parallel_tool_calls": true,
        "tools": [{
            "type": "function",
            "name": "inspect_workspace",
            "parameters": {"type": "object", "properties": {}}
        }],
    });

    prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

    assert_eq!(body["model"], "gpt-codex");
    assert!(body["input"].is_array());
    assert!(body.get("instructions").is_none());
    assert_eq!(body["parallel_tool_calls"], true);
    assert_eq!(body["tools"][0]["name"], "inspect_workspace");
    assert!(body.get("stream").is_none());
    assert!(body.get("store").is_none());
    assert!(body.get("include").is_none());
    assert!(body.get("previous_response_id").is_none());
    assert!(body.get("unsupported_vendor_field").is_none());
}

/// Verifies compact requests retain non-empty ordinary-turn base instructions.
/// 验证压缩请求会保留非空的普通回合基础指令。
#[test]
fn codex_compact_body_preserves_nonempty_base_instructions() {
    let mut body = json!({
        "model": "gpt-codex",
        "input": [],
        "instructions": "ordinary Codex base instructions",
        "stream": true,
    });

    prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

    assert_eq!(
        body["instructions"],
        Value::String(String::from("ordinary Codex base instructions"))
    );
    assert!(body.get("stream").is_none());
}

/// Verifies compact responses preserve metadata and reject output without an opaque checkpoint.
/// 验证 compact 响应保留元数据，并拒绝不含不透明检查点的输出。
#[test]
fn codex_compact_response_requires_a_compaction_item() {
    let valid = decode_responses_compact_result(
        &json!({
            "id": "resp-compact",
            "usage": {"input_tokens": 4},
            "output": [{"type": "compaction_summary", "encrypted_content": "opaque"}]
        }),
        "https://example.test/responses/compact",
    )
    .expect("compaction output should decode");
    assert_eq!(valid.response_id.as_deref(), Some("resp-compact"));
    assert_eq!(valid.usage, Some(json!({"input_tokens": 4})));
    assert_eq!(valid.output_items[0]["type"], "compaction");

    let failure = decode_responses_compact_result(
        &json!({"output": [{"type": "message", "content": []}]}),
        "https://example.test/responses/compact",
    )
    .expect_err("message-only output must not become a checkpoint");
    assert!(failure.error.message.contains("replayable output items"));
    assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);

    // empty_checkpoint proves a typed compaction marker still needs a usable opaque payload.
    // empty_checkpoint 证明带类型的压缩标记仍然必须拥有可用的不透明载荷。
    let empty_checkpoint = decode_responses_compact_result(
        &json!({"output": [{"type": "compaction", "encrypted_content": ""}]}),
        "https://example.test/responses/compact",
    )
    .expect_err("empty opaque checkpoints must be rejected before persistence");
    assert!(
        empty_checkpoint
            .error
            .message
            .contains("replayable output items")
    );

    let mixed = decode_responses_compact_result(
        &json!({
            "output": [
                {"type": "compaction", "encrypted_content": "opaque"},
                {"type": "message", "content": []}
            ]
        }),
        "https://example.test/responses/compact",
    )
    .expect("ordered replayable output items should be preserved");
    assert_eq!(mixed.output_items.len(), 2);
    let unknown_failure = decode_responses_compact_result(
        &json!({
            "output": [
                {"type": "compaction", "encrypted_content": "opaque"},
                {"type": "provider_private_item"}
            ]
        }),
        "https://example.test/responses/compact",
    )
    .expect_err("unknown output items must not become a checkpoint");
    assert!(
        unknown_failure
            .error
            .message
            .contains("replayable output items")
    );
}

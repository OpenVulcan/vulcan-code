//! Regression coverage for socket ownership, bounded reuse, and physical retry boundaries.
//! Socket归属、有界复用与真实重试边界的回归覆盖。

use super::super::test_support::{
    deepseek_route, minimal_request, pinned_openrouter_route, read_test_http_request,
};
use super::*;
use crate::{ResponsesRemoteCompactionPolicy, ResponsesRouteFeatures, ResponsesWebsocketPolicy};
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{
        Message,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};
use vulcan_ai_protocols::{
    OPENAI_CHAT_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    ProtocolRegistry,
};

use vulcan_ai_contract::AiContinuation;

/// Adds one exact Codex continuation boundary to a minimal test request.
/// 为最小测试请求增加一项精确 Codex 延续边界。
fn request_with_codex_continuation() -> AiExecutionRequest {
    let mut request = minimal_request();
    request.continuation = Some(AiContinuation {
        provider_id: String::from("openrouter"),
        protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
        model_id: request.model_id.clone(),
        provider_message_id: String::from("response-1"),
        covered_local_message_id: String::from("message-1"),
        covered_message_count: 1,
    });
    request
}

/// Verifies WebSocket reuse keys are opaque and change with the exact endpoint snapshot.
/// 验证 WebSocket 复用键不透明，并会随精确 endpoint 快照变化。
#[test]
fn responses_websocket_cache_key_is_bounded_and_endpoint_bound() {
    let mut route = deepseek_route(OPENAI_CHAT_PROFILE_ID);
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("session-sensitive-value"));
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("test-secret"))
        .expect("test secret envelope should validate")
        .execution_secret()
        .expect("test execution secret should resolve");
    let first = responses_websocket_cache_key(&route, &request, &secret)
        .expect("session-bound request should produce a key");
    assert_eq!(first.len(), "sha256:".len() + 64);
    assert!(!first.contains("session-sensitive-value"));
    assert!(!first.contains(route.entry.base_url.as_str()));

    route.entry.base_url = String::from("https://changed.example.invalid/v1");
    let changed = responses_websocket_cache_key(&route, &request, &secret)
        .expect("changed endpoint should still produce a key");
    assert_ne!(first, changed);
}

/// Verifies sticky WebSocket breakers retain a hard LRU bound and refresh existing keys.
/// 验证粘性 WebSocket 熔断器保持硬性 LRU 上限，并刷新既有键。
#[test]
fn responses_websocket_breakers_are_bounded_and_lru() {
    let base = Instant::now();
    let mut breakers = BTreeMap::new();
    for index in 0..RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES {
        insert_bounded_responses_websocket_breaker(
            &mut breakers,
            format!("key-{index:04}").as_str(),
            base + Duration::from_millis(index as u64),
        );
    }
    insert_bounded_responses_websocket_breaker(
        &mut breakers,
        "key-0000",
        base + Duration::from_secs(10),
    );
    insert_bounded_responses_websocket_breaker(
        &mut breakers,
        "key-new",
        base + Duration::from_secs(11),
    );

    assert_eq!(breakers.len(), RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES);
    assert!(breakers.contains_key("key-0000"));
    assert!(!breakers.contains_key("key-0001"));
    assert!(breakers.contains_key("key-new"));
}

/// Verifies store=false HTTP never emits a cursor and WS requires exact socket ownership.
/// 验证 store=false HTTP 永不发送游标且 WS 要求精确 Socket 归属。
#[tokio::test]
async fn nonpersistent_responses_continuation_is_scoped_to_the_active_websocket() {
    let request = request_with_codex_continuation();
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);

    let http_request = request_without_nonpersistent_responses_continuation(&route, &request);
    assert!(http_request.continuation.is_none());

    let fresh_socket = responses_websocket_request_for_active_session(&request, None);
    assert!(fresh_socket.continuation.is_none());
    let wrong_socket =
        responses_websocket_request_for_active_session(&request, Some("response-other"));
    assert!(wrong_socket.continuation.is_none());
    let matching_socket =
        responses_websocket_request_for_active_session(&request, Some("response-1"));
    assert_eq!(
        matching_socket
            .continuation
            .as_ref()
            .map(|continuation| continuation.provider_message_id.as_str()),
        Some("response-1")
    );

    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let effective = client
        .encode_effective_request_with_credential(&route, &request, &secret)
        .await
        .expect("HTTP preview should encode");
    assert!(effective.body.get("previous_response_id").is_none());
}

/// Verifies only official recoverable WebSocket controls become replay-safe failures.
/// 验证仅官方可恢复 WebSocket 控制错误会转换为可安全重放失败。
#[test]
fn websocket_retry_controls_preserve_code_and_semantic_safety() {
    for code in [
        RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE,
        RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE,
    ] {
        let (actual_code, message) = responses_websocket_retry_control(&json!({
            "type": "error",
            "error": {"code": code, "message": "retry the request"}
        }))
        .expect("official control should be recognized");
        let failure = responses_websocket_control_failure(actual_code, message, false);
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.transport,
            Some(AiTransportKind::ResponsesWebsocket)
        );
        assert_eq!(failure.error.upstream_code.as_deref(), Some(code));
        assert!(!failure.error.semantic_output_observed);
    }

    assert!(
        responses_websocket_retry_control(&json!({
            "type": "error",
            "error": {"code": "invalid_prompt", "message": "bad prompt"}
        }))
        .is_none()
    );
    let failure = responses_websocket_control_failure(
        String::from(RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE),
        String::from("retry"),
        true,
    );
    assert!(!failure.error.retryable);
    assert!(failure.error.semantic_output_observed);
}

/// Verifies a 1013 pre-semantic WS failure activates session-scoped HTTP fallback.
/// 验证语义输出前的 1013 WS 失败会激活会话作用域 HTTP 回退。
#[tokio::test]
async fn websocket_1013_failure_falls_back_to_full_history_http_attempt() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("WS client should connect");
        let mut websocket = accept_async(stream)
            .await
            .expect("WS upgrade should succeed");
        let _ = websocket.next().await.expect("WS request should arrive");
        websocket
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Again,
                reason: "try HTTP".into(),
            })))
            .await
            .expect("1013 close should send");
        drop(websocket);

        let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
        let request = read_test_http_request(&mut stream).await;
        assert!(
            !request.contains("previous_response_id"),
            "store=false HTTP fallback must replay complete history"
        );
        let body = [
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-http\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"message-http\",\"delta\":\"OK\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-http\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
        ]
        .concat();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("HTTP response should send");
    });

    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
        remote_compaction: ResponsesRemoteCompactionPolicy::Disabled,
        websocket: ResponsesWebsocketPolicy::Verified,
    });
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let mut request = request_with_codex_continuation();
    request.cache_scope_id = Some(String::from("session-1"));

    let mut websocket_attempt = client.execute(
        route.clone(),
        request.clone(),
        secret.clone(),
        String::from("execution-ws"),
    );
    let mut websocket_observed = false;
    let failure = loop {
        match websocket_attempt.next().await {
            Some(Ok(AiStreamEvent::TransportObserved { transport, .. })) => {
                websocket_observed |= transport == AiTransportKind::ResponsesWebsocket;
            }
            Some(Ok(_)) => {}
            Some(Err(failure)) => break failure,
            None => panic!("WS attempt must fail"),
        }
    };
    assert!(websocket_observed);
    assert_eq!(failure.error.websocket_close_code, Some(1013));
    assert!(!failure.error.semantic_output_observed);
    drop(websocket_attempt);

    let mut http_attempt = client.execute(route, request, secret, String::from("execution-http"));
    let mut http_observed = false;
    let mut completed = false;
    while let Some(event) = http_attempt.next().await {
        match event.expect("HTTP fallback should succeed") {
            AiStreamEvent::TransportObserved { transport, .. } => {
                http_observed |= transport == AiTransportKind::Http;
            }
            AiStreamEvent::ResponseCompleted => completed = true,
            _ => {}
        }
    }
    assert!(http_observed);
    assert!(completed);
    server.await.expect("server should finish");
}

/// Verifies a missing socket-local response retries full history on the same live connection.
/// 验证 Socket 局部响应缺失后会在同一活动连接上用完整历史重试。
#[tokio::test]
async fn websocket_previous_response_not_found_retries_without_stale_cursor() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("WS client should connect");
        let mut websocket = accept_async(stream)
            .await
            .expect("WS upgrade should succeed");

        let first = websocket
            .next()
            .await
            .expect("first request should arrive")
            .expect("first request should read");
        let Message::Text(first) = first else {
            panic!("first request must be text");
        };
        let first: Value = serde_json::from_str(&first).expect("first request should be JSON");
        assert!(first.get("previous_response_id").is_none());
        websocket
            .send(Message::Text(
                json!({"type":"response.completed","response":{"id":"response-1"}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("first completion should send");

        let second = websocket
            .next()
            .await
            .expect("incremental request should arrive")
            .expect("incremental request should read");
        let Message::Text(second) = second else {
            panic!("incremental request must be text");
        };
        let second: Value =
            serde_json::from_str(&second).expect("incremental request should be JSON");
        assert_eq!(second["previous_response_id"], "response-1");
        websocket
            .send(Message::Text(
                json!({
                    "type":"error",
                    "error":{
                        "code":RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE,
                        "message":"chain expired"
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("missing-chain control should send");

        let third = websocket
            .next()
            .await
            .expect("full retry should arrive")
            .expect("full retry should read");
        let Message::Text(third) = third else {
            panic!("full retry must be text");
        };
        let third: Value = serde_json::from_str(&third).expect("full retry should be JSON");
        assert!(third.get("previous_response_id").is_none());
        websocket
            .send(Message::Text(
                json!({"type":"response.completed","response":{"id":"response-2"}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("retry completion should send");
    });

    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
        remote_compaction: ResponsesRemoteCompactionPolicy::Disabled,
        websocket: ResponsesWebsocketPolicy::Verified,
    });
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let mut request = request_with_codex_continuation();
    request.cache_scope_id = Some(String::from("session-previous-response"));

    let mut first = client.execute(
        route.clone(),
        request.clone(),
        secret.clone(),
        String::from("execution-first"),
    );
    let mut first_completed = false;
    while let Some(event) = first.next().await {
        first_completed |= matches!(
            event.expect("first request should succeed"),
            AiStreamEvent::ResponseCompleted
        );
    }
    assert!(first_completed);

    let mut incremental = client.execute(
        route.clone(),
        request.clone(),
        secret.clone(),
        String::from("execution-incremental"),
    );
    let failure = loop {
        match incremental.next().await {
            Some(Ok(_)) => {}
            Some(Err(failure)) => break failure,
            None => panic!("missing-chain request must fail"),
        }
    };
    assert_eq!(
        failure.error.upstream_code.as_deref(),
        Some(RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE)
    );
    assert!(failure.error.retryable);
    drop(incremental);

    let mut retry = client.execute(route, request, secret, String::from("execution-retry"));
    let mut retry_completed = false;
    while let Some(event) = retry.next().await {
        retry_completed |= matches!(
            event.expect("full-history retry should succeed"),
            AiStreamEvent::ResponseCompleted
        );
    }
    assert!(retry_completed);
    server.await.expect("server should finish");
}

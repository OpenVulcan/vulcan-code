//! Regression coverage for failure categories, dispatch evidence, retries, and secret-safe fields.
//! 错误类别、分派证据、重试与无秘密字段的回归覆盖。

use super::*;

/// Verifies an HTTP stream-idle timeout stays explicitly retryable after discarded partial output.
/// 验证 HTTP 流空闲超时在丢弃局部输出后仍显式保持可重试。
#[test]
fn stream_idle_timeout_remains_retryable_after_semantic_output() {
    // failure carries the typed idle-timeout subtype while retaining partial-output evidence.
    // failure 携带强类型空闲超时子类型，同时保留局部输出证据。
    let failure = stream_idle_timeout_failure("stream idle timeout", true);

    assert_eq!(failure.error.category, AiErrorCategory::Timeout);
    assert_eq!(
        failure.error.subcategory.as_deref(),
        Some("stream_idle_timeout")
    );
    assert_eq!(failure.error.stage, "response_stream");
    assert!(failure.error.semantic_output_observed);
    assert!(!failure.error.side_effect_observed);
    assert!(failure.error.retryable);
}

/// Verifies an exact upstream context code is distinct from generic invalid requests.
/// 验证精确上游上下文代码与通用无效请求具有不同类别。
#[test]
fn http_context_length_code_is_typed() {
    let failure = http_failure(
        400,
        None,
        r#"{"error":{"type":"invalid_request_error","code":"context_length_exceeded","message":"too large"}}"#,
    );

    assert_eq!(
        failure.error.category,
        AiErrorCategory::ContextLengthExceeded
    );
    assert_eq!(
        failure.error.upstream_code.as_deref(),
        Some("context_length_exceeded")
    );
}

/// Verifies every public execution diagnostic is bounded and cannot retain the exact secret.
/// 验证每个公共执行诊断均有界且无法保留精确秘密。
#[test]
fn execution_failure_boundary_redacts_exact_secret_and_provider_urls() {
    // secret models an opaque value whose shape alone would not trigger generic redaction.
    // secret 模拟一个仅凭形态不会触发通用脱敏的不透明值。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from(
        "opaque-execution-credential-value",
    ))
    .expect("test credential should validate")
    .execution_secret()
    .expect("test execution secret should resolve");
    // failure carries the secret in a provider message and a signed URL in another public field.
    // failure 在供应商消息中携带秘密，并在另一公共字段中携带签名 URL。
    let mut failure = http_failure(
        401,
        None,
        r#"{"error":{"type":"authentication_error","message":"echo opaque-execution-credential-value"}}"#,
    );
    failure.error.model_id = Some(String::from(
        "https://provider.example/result?signature=private",
    ));
    failure.error.request_id = Some("r".repeat(8_192));

    // sanitized preserves typed semantics while closing every public diagnostic field.
    // sanitized 在封闭每个公共诊断字段的同时保留强类型语义。
    let sanitized = sanitize_provider_execution_failure(failure, &secret);
    assert_eq!(sanitized.error.category, AiErrorCategory::Authentication);
    assert!(!sanitized.error.message.contains(secret.expose_value()));
    assert!(
        !sanitized
            .error
            .upstream_message
            .as_deref()
            .unwrap_or_default()
            .contains(secret.expose_value())
    );
    assert_eq!(sanitized.error.model_id.as_deref(), Some("<redacted-url>"));
    assert!(
        sanitized
            .error
            .request_id
            .as_deref()
            .is_some_and(|value| value.chars().count() <= 2_048)
    );
}

/// Verifies only a proven connection-establishment failure is safe to replay.
/// 验证只有已证明的连接建立失败才可安全重放。
#[tokio::test]
async fn http_send_failure_uses_reqwest_connection_evidence() {
    // listener reserves one loopback port so the subsequent closed-port connection fails deterministically.
    // listener 预留一个回环端口，使随后对已关闭端口的连接确定性失败。
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener should bind");
    // port is the exact operating-system-assigned loopback port.
    // port 是操作系统精确分配的回环端口。
    let port = listener
        .local_addr()
        .expect("loopback address should resolve")
        .port();
    drop(listener);

    // connect_error is produced before an HTTP connection can be established.
    // connect_error 产生于 HTTP 连接建立之前。
    let connect_error = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("loopback test client should build")
        .post(format!("http://127.0.0.1:{port}/responses"))
        .send()
        .await
        .expect_err("closed loopback port should reject the connection");
    assert!(connect_error.is_connect());
    // connect_failure is therefore explicitly safe for structured retry.
    // connect_failure 因而可明确安全地进入结构化重试。
    let connect_failure = http_send_transport_failure(&connect_error, "connect failed");
    assert_eq!(
        connect_failure.error.dispatch_outcome,
        DispatchOutcome::NotDispatched
    );
    assert_eq!(connect_failure.error.transport, Some(AiTransportKind::Http));
    assert!(connect_failure.error.retryable);

    // builder_error proves that unrelated request-send failures retain the conservative outcome.
    // builder_error 证明无关的请求发送失败会保留保守结果。
    let builder_error = reqwest::Client::new()
        .post("not a valid provider URL")
        .send()
        .await
        .expect_err("invalid URL should fail before transport");
    assert!(!builder_error.is_connect());
    // builder_failure must not be promoted to replay-safe without connection evidence.
    // builder_failure 在缺少连接证据时不得被提升为可安全重放。
    let builder_failure = http_send_transport_failure(&builder_error, "builder failed");
    assert_eq!(
        builder_failure.error.dispatch_outcome,
        DispatchOutcome::Unknown
    );
    assert_eq!(builder_failure.error.transport, Some(AiTransportKind::Http));
}

/// Verifies HTTP stream failures cannot be mislabeled as Responses WebSocket failures.
/// 验证 HTTP 流失败不会被错误标记为 Responses WebSocket 失败。
#[test]
fn http_stream_failure_retains_http_transport() {
    let failure = stream_transport_failure("stream ended", false);

    assert_eq!(failure.error.transport, Some(AiTransportKind::Http));
    assert_eq!(failure.error.websocket_close_code, None);
}

/// Verifies WebSocket decoder failures retain the WebSocket transport classification.
/// 验证 WebSocket 解码失败保留 WebSocket 传输分类。
#[test]
fn websocket_protocol_failure_retains_websocket_transport() {
    let failure = stream_protocol_failure_for_transport(
        ProtocolConversionError::new("bad frame"),
        false,
        AiTransportKind::ResponsesWebsocket,
    );

    assert_eq!(
        failure.error.transport,
        Some(AiTransportKind::ResponsesWebsocket)
    );
}

/// Verifies an empty upstream error body retains the exact HTTP status.
/// 验证空上游错误正文仍会保留精确 HTTP 状态。
#[test]
fn empty_http_error_response_reports_status() {
    let failure = http_failure(404, None, "");

    assert_eq!(failure.error.http_status, Some(404));
    assert_eq!(
        failure.error.message,
        "Provider returned HTTP 404 with an empty error response."
    );
}

/// Verifies OpenAI's HTTP 429 exhausted quota is a terminal payment failure, not a retryable rate limit.
/// 验证 OpenAI 的 HTTP 429 额度耗尽是终止的付费失败，而不是可重试的限流。
#[test]
fn http_insufficient_quota_is_payment_required() {
    let failure = http_failure(
        429,
        None,
        r#"{"error":{"type":"insufficient_quota","code":"insufficient_quota","message":"quota exhausted"}}"#,
    );

    assert_eq!(failure.error.category, AiErrorCategory::PaymentRequired);
    assert!(!failure.error.retryable);
}

/// Verifies the official Codex overload code overrides a generic HTTP proxy error type.
/// 验证官方 Codex 过载代码会覆盖 HTTP 代理返回的通用错误类型。
#[test]
fn http_server_overloaded_code_is_retryable() {
    // failure models the structured 503 body used by official Codex and compatible proxies.
    // failure 模拟官方 Codex 与兼容代理使用的结构化 503 正文。
    let failure = http_failure(
        503,
        None,
        r#"{"error":{"type":"server","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later."}}"#,
    );

    assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
    assert!(failure.error.retryable);
    assert_eq!(
        failure.error.upstream_code.as_deref(),
        Some("server_is_overloaded")
    );
}

/// Verifies the official overload message remains retryable when a proxy returns plain text.
/// 验证代理返回纯文本时，官方过载消息仍会被归入可重试类别。
#[test]
fn http_server_overloaded_plain_text_is_retryable() {
    let failure = http_failure(
        400,
        None,
        "Our servers are currently overloaded. Please try again later.",
    );

    assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
    assert!(failure.error.retryable);
    assert_eq!(
        failure.error.upstream_message.as_deref(),
        Some("Our servers are currently overloaded. Please try again later.")
    );
}

/// Verifies HTTP decoder completion distinguishes transient truncation from malformed payloads.
/// 验证 HTTP 解码器结束阶段会区分瞬态截断与畸形载荷。
#[test]
fn responses_http_completion_failure_preserves_retry_safety() {
    let truncated = stream_completion_failure(
        ProtocolConversionError::truncated_stream("stream ended before terminal"),
        false,
    );
    assert_eq!(
        truncated.error.category,
        AiErrorCategory::ProviderUnavailable
    );
    assert_eq!(
        truncated.error.dispatch_outcome,
        DispatchOutcome::Dispatched
    );
    assert!(truncated.error.retryable);

    let post_semantic_truncation = stream_completion_failure(
        ProtocolConversionError::truncated_stream("stream ended after output"),
        true,
    );
    assert!(!post_semantic_truncation.error.retryable);
    assert!(post_semantic_truncation.error.semantic_output_observed);

    let malformed = stream_completion_failure(
        ProtocolConversionError::new("invalid Responses payload"),
        false,
    );
    assert_eq!(malformed.error.category, AiErrorCategory::InvalidRequest);
    assert_eq!(
        malformed.error.subcategory.as_deref(),
        Some("protocol_conversion")
    );
    assert!(!malformed.error.retryable);
}

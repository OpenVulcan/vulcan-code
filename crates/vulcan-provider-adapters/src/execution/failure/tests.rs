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

/// Verifies a WebSocket lost after partial output stays retryable, unlike a plain HTTP loss.
/// 验证局部输出后丢失的 WebSocket 仍可重试，与普通 HTTP 丢失不同。
#[test]
fn websocket_stream_loss_remains_retryable_after_semantic_output() {
    let failure = websocket_stream_loss_failure("socket closed", true, Some(1012));

    assert_eq!(
        failure.error.transport,
        Some(AiTransportKind::ResponsesWebsocket)
    );
    assert_eq!(failure.error.subcategory.as_deref(), Some("transport"));
    assert_eq!(failure.error.websocket_close_code, Some(1012));
    assert!(failure.error.semantic_output_observed);
    assert!(failure.error.retryable);
    assert!(
        !stream_transport_failure("stream lost", true)
            .error
            .retryable
    );
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

/// Verifies a credential straddling the diagnostic length bound leaves no fragment behind.
/// 验证跨越诊断长度上限的凭据不会留下任何片段。
#[test]
fn credential_across_the_length_bound_leaves_no_fragment() {
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from(
        "opaque-execution-credential-value",
    ))
    .expect("test credential should validate")
    .execution_secret()
    .expect("test execution secret should resolve");
    // The credential starts a few characters before the field bound, so truncating first would
    // keep its prefix.
    // 凭据起点位于字段上限之前数个字符，若先截断就会保留其前缀。
    let message = format!("{}{}", "x".repeat(2_040), secret.expose_value());
    let body = serde_json::json!({ "error": { "message": message } }).to_string();
    let failure = http_failure(
        401,
        None,
        redact_exact_credential(body.as_str(), &secret).as_str(),
    );
    let upstream_message = failure.error.upstream_message.unwrap_or_default();
    assert!(!upstream_message.contains("opaque-exec"));
    let mut event_error =
        sanitize_provider_execution_failure(http_failure(401, None, body.as_str()), &secret).error;
    event_error.upstream_message = Some(message);
    sanitize_ai_execution_error(&mut event_error, &secret);
    assert!(
        !event_error
            .upstream_message
            .unwrap_or_default()
            .contains("opaque-exec")
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

    // connect_evidence keeps the phase label and the bounded cause chain for later diagnosis.
    // connect_evidence 为后续诊断保留阶段标签与受限原因链。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("fixture-send-key"))
        .expect("test credential should validate")
        .execution_secret()
        .expect("test execution secret should resolve");
    // Use the same synthetic credential for both transport failure categories.
    // 两类传输失败共用同一组合成凭据。
    let connect_evidence = http_send_failure_evidence(&connect_error, &secret);
    assert_eq!(connect_evidence.phase, TransportFailurePhase::Connect);
    assert!(!connect_evidence.causes.is_empty());
    assert!(connect_evidence.causes.len() <= TRANSPORT_FAILURE_MAX_CAUSES);
    assert!(
        connect_evidence
            .causes
            .iter()
            .all(|cause| cause.chars().count() <= TRANSPORT_FAILURE_MAX_CAUSE_CHARACTERS)
    );

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

    // builder_evidence must never claim a proven connection-establishment failure.
    // builder_evidence 绝不声称已证明连接建立失败。
    let builder_evidence = http_send_failure_evidence(&builder_error, &secret);
    assert_ne!(builder_evidence.phase, TransportFailurePhase::Connect);
    assert!(builder_evidence.causes.len() <= TRANSPORT_FAILURE_MAX_CAUSES);
}

/// Verifies a transport cause retains the endpoint and reason while hiding an echoed credential.
/// 验证传输原因保留端点与故障原因，同时遮蔽回显凭据。
#[test]
fn http_send_causes_preserve_endpoint_and_mask_exact_credential() {
    // The synthetic cause includes a normal URL and an opaque credential.
    // 合成原因同时包含普通 URL 与不透明凭据。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("fixture-cause-key"))
        .expect("test credential should validate")
        .execution_secret()
        .expect("test execution secret should resolve");
    // The cause reaches the same formatter used for reqwest's source chain.
    // 此原因进入 reqwest 错误来源链使用的同一格式化逻辑。
    let cause = std::io::Error::other(format!(
        "https://example.test/v1/v1/responses connection refused for {}",
        secret.expose_value()
    ));
    // The resulting trace cause keeps the diagnostic context.
    // 生成的追踪原因保留诊断上下文。
    let causes = http_send_causes(Some(&cause), &secret);
    assert!(
        causes
            .iter()
            .any(|value| value.contains("/v1/v1/responses connection refused"))
    );
    assert!(
        causes
            .iter()
            .all(|value| !value.contains(secret.expose_value()))
    );
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

/// Checks the real Anthropic HTTP 400 billing envelope while preserving the upstream error type.
/// 检查真实 Anthropic HTTP 400 余额错误封装，同时保留上游错误类型。
#[test]
fn http_anthropic_credit_rejection_is_payment_required() {
    // Anthropic reuses invalid_request_error for unrelated failures, so only its billing message is special.
    // Anthropic 对无关失败复用 invalid_request_error，因此只特殊识别余额消息。
    let failure = http_failure(
        400,
        None,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits."}}"#,
    );
    assert_eq!(failure.error.category, AiErrorCategory::PaymentRequired);
    assert_eq!(
        failure.error.upstream_type.as_deref(),
        Some("invalid_request_error")
    );
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

/// Verifies the response-header bound produces timeout evidence without a cause chain.
/// 验证响应头时限生成不含原因链的超时证据。
#[test]
fn response_header_timeout_evidence_carries_the_registered_bound() {
    // evidence records only the bound that expired; it never invents a cause chain.
    // evidence 只记录真正到期的时限，不会凭空构造原因链。
    let evidence = response_headers_timeout_evidence(60_000);

    assert_eq!(
        evidence.phase,
        TransportFailurePhase::ResponseHeadersTimeout
    );
    assert_eq!(evidence.phase.as_str(), "response_headers_timeout");
    assert!(evidence.causes.is_empty());
    assert_eq!(evidence.timeout_ms, Some(60_000));
}

/// Verifies the host timeout crosses the public error boundary with its safe endpoint and specific code.
/// 验证宿主超时携带安全端点与具体代码通过公开错误边界。
#[test]
fn response_header_timeout_keeps_the_public_http_endpoint() {
    // Use distinct secrets in every URL component that the diagnostic must remove.
    // 在诊断必须移除的每种 URL 组件里放入不同的测试秘密。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("fixture-key"))
        .expect("fixture key should validate")
        .execution_secret()
        .expect("fixture key should resolve");
    // This is the same host construction and sanitization path used by the HTTP deadline.
    // 此处走与 HTTP 截止时间相同的宿主构造与清理路径。
    let failure = response_headers_timeout_failure(60_000).with_host_http_endpoint(
        "https://fixture-user:fixture-password@provider.invalid/v1/fixture-key/chat/completions?token=fixture-query#fixture-fragment",
        &secret,
    );
    let error = sanitize_provider_execution_failure(failure, &secret).error;

    assert_eq!(error.transport, Some(AiTransportKind::Http));
    assert_eq!(
        error.subcategory.as_deref(),
        Some("response_headers_timeout")
    );
    assert_eq!(error.upstream_code, None);
    assert_eq!(error.dispatch_outcome, DispatchOutcome::Unknown);
    assert!(error.retryable);
    assert!(
        error
            .message
            .contains("https://provider.invalid/v1/<redacted>/chat/completions")
    );
    assert!(error.message.contains("60000"));
    for secret in [
        "fixture-user",
        "fixture-password",
        "fixture-key",
        "fixture-query",
        "fixture-fragment",
        "<redacted-url>",
    ] {
        assert!(!error.message.contains(secret));
    }
}

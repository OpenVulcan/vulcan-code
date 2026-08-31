//! Regression coverage for bounded declared headers, content types, and retry provenance.
//! 有界声明响应头、内容类型与重试来源的回归覆盖。

use super::*;
use reqwest::header::HeaderValue;
use std::time::Duration;
use vulcan_ai_contract::DispatchOutcome;

/// Verifies provider request identifiers follow the route-declared header order exactly.
/// 验证供应商请求标识严格遵循路由声明的响应头顺序。
#[test]
fn provider_request_id_uses_declared_order() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", "primary".parse().expect("valid header"));
    headers.insert(
        "x-oai-request-id",
        "secondary".parse().expect("valid header"),
    );
    let declared_names = vec![
        String::from("x-oai-request-id"),
        String::from("x-request-id"),
    ];

    assert_eq!(
        provider_request_id_from_headers(&headers, declared_names.as_slice())
            .expect("declared request-id headers should decode"),
        Some(String::from("secondary"))
    );
    assert_eq!(
        provider_request_id_from_headers(&headers, &[])
            .expect("an undeclared route should not inspect headers"),
        None
    );
}

/// Verifies provider-controlled diagnostic header values remain explicitly bounded.
/// 验证供应商控制的诊断响应头值保持显式有界。
#[test]
fn provider_diagnostic_header_values_are_bounded() {
    // oversized_value exceeds the complete retained telemetry budget.
    // oversized_value 超过遥测完整保留预算。
    let oversized_value = "x".repeat(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES + 128);
    // headers carries the same adversarial value through both diagnostic extraction paths.
    // headers 通过两条诊断提取路径携带同一恶意超长值。
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-request-id",
        HeaderValue::from_bytes(oversized_value.as_bytes())
            .expect("oversized ASCII header must parse"),
    );
    headers.insert(
        "retry-after",
        HeaderValue::from_bytes(oversized_value.as_bytes())
            .expect("oversized ASCII header must parse"),
    );
    // request_id is the bounded route-declared diagnostic value.
    // request_id 是有界的路由声明诊断值。
    let request_id = provider_request_id_from_headers(&headers, &[String::from("x-request-id")])
        .expect("valid diagnostic text must decode")
        .expect("declared response header must be present");
    assert_eq!(request_id.len(), PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
    assert!(request_id.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));

    // rate_limit_headers contains only the fixed allowlist and the same bounded representation.
    // rate_limit_headers 仅包含固定允许列表与相同的有界表示。
    let rate_limit_headers = allowlisted_rate_limit_headers(&headers);
    let retry_after = rate_limit_headers
        .get("retry-after")
        .expect("allowlisted rate-limit header must be retained");
    assert_eq!(
        retry_after.len(),
        PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES
    );
    assert!(retry_after.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));

    // unicode_value proves the shared truncator never splits a UTF-8 scalar.
    // unicode_value 证明共享截断器绝不会切开 UTF-8 标量。
    let unicode_value = "测".repeat(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
    let bounded_unicode = bounded_provider_diagnostic_header_value(&unicode_value);
    assert!(bounded_unicode.len() <= PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
    assert!(bounded_unicode.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));
}

/// Verifies successful responses require one exact registered media type.
/// 验证成功响应必须使用一个精确注册的媒体类型。
#[test]
fn response_content_type_is_strict_and_parameter_tolerant() {
    let profile = ProtocolTransportProfile::bounded_sse();
    let mut valid = HeaderMap::new();
    valid.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    validate_response_content_type(&valid, &profile, "https://example.test")
        .expect("registered media type should pass");

    let mut invalid = HeaderMap::new();
    invalid.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let failure = validate_response_content_type(&invalid, &profile, "https://example.test")
        .expect_err("JSON must not enter the SSE decoder");
    assert_eq!(
        failure.error.subcategory.as_deref(),
        Some("response_content_type")
    );
    assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
}

/// Verifies numeric Retry-After values retain fractional-second precision.
/// 验证数值型 Retry-After 保留小数秒精度。
#[test]
fn retry_after_numeric_values_preserve_milliseconds() {
    assert_eq!(parse_decimal_seconds_milliseconds("5"), Some(5_000));
    assert_eq!(parse_decimal_seconds_milliseconds("2.5"), Some(2_500));
    assert_eq!(parse_decimal_seconds_milliseconds("0.0001"), Some(1));
    assert_eq!(parse_decimal_seconds_milliseconds("1.0019"), Some(1_002));
    assert_eq!(parse_decimal_seconds_milliseconds("-1"), None);
    assert_eq!(parse_decimal_seconds_milliseconds("1.2.3"), None);
}

/// Verifies Retry-After HTTP dates preserve their source and exact relative delay.
/// 验证 Retry-After HTTP 日期保留来源与精确相对延迟。
#[test]
fn retry_after_http_date_preserves_source() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let retry_at = now + Duration::from_secs(17);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::RETRY_AFTER,
        HeaderValue::from_str(httpdate::fmt_http_date(retry_at).as_str()).expect("valid HTTP date"),
    );

    let hint = parse_retry_after_headers(&headers, now).expect("valid retry hint");
    assert_eq!(hint.delay_milliseconds, 17_000);
    assert_eq!(hint.source, RetryAfterSource::HeaderHttpDate);
}

/// Verifies malformed Retry-After values are rejected instead of guessed.
/// 验证畸形 Retry-After 值会被拒绝而不是被猜测。
#[test]
fn retry_after_malformed_value_is_rejected() {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::RETRY_AFTER,
        HeaderValue::from_static("later"),
    );
    assert!(parse_retry_after_headers(&headers, SystemTime::now()).is_none());
}

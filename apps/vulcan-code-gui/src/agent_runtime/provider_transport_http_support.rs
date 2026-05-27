//! Shared HTTP transport support helpers reused by host-owned provider transports.
//! 供宿主自有 Provider 传输复用的共享 HTTP 传输支持辅助模块。

use crate::agent_runtime::provider_transport_adapter_support::map_provider_client_build_result;
use reqwest::{
    StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_core::types::CoreError as VulcanCoreError;

/// Ensures one bearer authorization header exists on the request-header map, inserting it only when the caller did not already provide one.
/// 确保请求头映射中存在一条 Bearer 鉴权头，并且仅在调用方尚未提供时才自动插入。
pub(crate) fn ensure_bearer_authorization_header(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    request_headers: &mut HeaderMap,
) -> Result<(), VulcanCoreError> {
    if request_headers.contains_key(AUTHORIZATION) {
        return Ok(());
    }

    let bearer = format!("Bearer {api_key}");
    let value =
        HeaderValue::from_str(&bearer).map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 鉴权头构建失败：{error}"),
        })?;
    request_headers.insert(AUTHORIZATION, value);
    Ok(())
}

/// Ensures one dynamic string header exists on the request-header map, inserting it only when the caller did not already provide one.
/// 确保请求头映射中存在一条动态字符串头，并且仅在调用方尚未提供时才自动插入。
pub(crate) fn ensure_string_header(
    provider_id: &str,
    provider_label: &str,
    request_headers: &mut HeaderMap,
    header_name: &'static str,
    header_value: &str,
    header_label: &str,
) -> Result<(), VulcanCoreError> {
    if request_headers.contains_key(header_name) {
        return Ok(());
    }

    let value =
        HeaderValue::from_str(header_value).map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} {header_label}头构建失败：{error}"),
        })?;
    request_headers.insert(header_name, value);
    Ok(())
}

/// Ensures one static header exists on the request-header map, inserting it only when the caller did not already provide one.
/// 确保请求头映射中存在一条静态头，并且仅在调用方尚未提供时才自动插入。
pub(crate) fn ensure_static_header(
    request_headers: &mut HeaderMap,
    header_name: &'static str,
    header_value: &'static str,
) {
    if request_headers.contains_key(header_name) {
        return;
    }

    request_headers.insert(header_name, HeaderValue::from_static(header_value));
}

/// Ensures the standard JSON content-type header exists on the request-header map.
/// 确保请求头映射中存在标准 JSON Content-Type 头。
pub(crate) fn ensure_json_content_type(request_headers: &mut HeaderMap) {
    request_headers
        .entry(CONTENT_TYPE)
        .or_insert(HeaderValue::from_static("application/json"));
}

/// Builds one JSON HTTP client from the merged request-header map.
/// 根据合并后的请求头映射构建一个 JSON HTTP 客户端。
pub(crate) fn build_json_http_client(
    provider_id: &str,
    provider_label: &str,
    request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    map_provider_client_build_result(
        provider_id,
        provider_label,
        reqwest::Client::builder()
            .default_headers(request_headers)
            .build(),
    )
}

/// Builds one REST endpoint from an optional base URL override, one default base URL, and one trailing path.
/// 根据可选基础地址覆盖值、默认基础地址与尾随路径构建一个 REST 端点。
pub(crate) fn build_rest_endpoint(
    base_url: Option<&str>,
    default_base_url: &str,
    path: &str,
) -> String {
    let normalized_base = base_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default_base_url)
        .trim_end_matches('/');
    format!("{normalized_base}/{path}")
}

/// Extracts one human-readable error message from a provider HTTP response body when the payload follows either `{ \"error\": { \"message\": ... } }` or `{ \"message\": ... }`.
/// 当 Provider HTTP 响应正文遵循 `{ \"error\": { \"message\": ... } }` 或 `{ \"message\": ... }` 形状时，提取其中的人类可读错误消息。
pub(crate) fn extract_standard_error_message(body: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(body).ok()?;
    parsed
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get("message")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

/// Formats one provider HTTP error response into a detailed compact summary that keeps status, structured error fields, and selected metadata.
/// 将单条 Provider HTTP 错误响应格式化为紧凑但细节充分的摘要，保留状态码、结构化错误字段与关键元数据。
pub(crate) fn format_standard_error_response(status: StatusCode, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("HTTP {status}");
    }

    let Some(parsed) = serde_json::from_str::<Value>(trimmed).ok() else {
        return format!("HTTP {status}: {}", truncate_error_detail(trimmed, 600));
    };

    let error_root = parsed.get("error").unwrap_or(&parsed);
    let mut segments = vec![format!("HTTP {status}")];
    let mut seen_segments = BTreeSet::new();

    if let Some(code) = error_root
        .get("code")
        .or_else(|| parsed.get("code"))
        .and_then(json_value_to_error_text)
    {
        let normalized_status_code = status.as_u16().to_string();
        if code != normalized_status_code {
            push_error_segment(
                &mut segments,
                &mut seen_segments,
                format!("code={}", truncate_error_detail(&code, 120)),
            );
        }
    }

    if let Some(kind) = error_root
        .get("type")
        .or_else(|| parsed.get("type"))
        .and_then(json_value_to_error_text)
    {
        push_error_segment(
            &mut segments,
            &mut seen_segments,
            format!("type={}", truncate_error_detail(&kind, 120)),
        );
    }

    if let Some(param) = error_root
        .get("param")
        .or_else(|| parsed.get("param"))
        .and_then(json_value_to_error_text)
    {
        push_error_segment(
            &mut segments,
            &mut seen_segments,
            format!("param={}", truncate_error_detail(&param, 160)),
        );
    }

    if let Some(message) =
        extract_standard_error_message(trimmed).filter(|value| !value.trim().is_empty())
    {
        push_error_segment(
            &mut segments,
            &mut seen_segments,
            truncate_error_detail(&message, 400),
        );
    }

    for (label, candidate) in [
        ("details", error_root.get("details")),
        ("raw", error_root.get("raw")),
        ("request_id", error_root.get("request_id")),
        ("request_id", parsed.get("request_id")),
        ("user_id", parsed.get("user_id")),
    ] {
        if let Some(value) = candidate.and_then(json_value_to_error_text) {
            push_error_segment(
                &mut segments,
                &mut seen_segments,
                format!("{label}={}", truncate_error_detail(&value, 240)),
            );
        }
    }

    if let Some(metadata) = error_root.get("metadata") {
        collect_standard_error_metadata_segments(
            metadata,
            &mut segments,
            &mut seen_segments,
            &[
                "raw",
                "reason",
                "details",
                "provider_name",
                "upstream_provider",
                "upstream_model",
                "model",
                "endpoint",
                "request_id",
            ],
        );
    }

    if segments.len() == 1 {
        return format!("HTTP {status}: {}", truncate_error_detail(trimmed, 600));
    }

    segments.join(" | ")
}

/// Formats one provider HTTP error response and appends retry-related headers when present.
/// 格式化单条 Provider HTTP 错误响应，并在存在时追加重试相关响应头。
pub(crate) fn format_standard_error_response_with_headers(
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
) -> String {
    let mut message = format_standard_error_response(status, body);
    if let Some(retry_after_ms) = retry_after_millis_from_headers(headers) {
        message.push_str(&format!(" | retry_after_ms={retry_after_ms}"));
    }
    if let Some(request_id) = request_id_from_headers(headers) {
        message.push_str(&format!(" | request_id={request_id}"));
    }
    message
}

/// Extracts a retry-after delay from provider response headers in milliseconds.
/// 从 Provider 响应头中提取以毫秒为单位的 retry-after 等待时间。
pub(crate) fn retry_after_millis_from_headers(headers: &HeaderMap) -> Option<u64> {
    if let Some(value) = header_to_string(headers, "retry-after-ms")
        && let Ok(parsed) = value.parse::<f64>()
    {
        return Some(parsed.max(0.0).round() as u64);
    }
    let value = header_to_string(headers, "retry-after")?;
    if let Ok(seconds) = value.parse::<f64>() {
        return Some((seconds.max(0.0) * 1000.0).ceil() as u64);
    }
    None
}

/// Extracts one provider request id from common response-header names.
/// 从常见响应头名称中提取一条 Provider 请求标识。
fn request_id_from_headers(headers: &HeaderMap) -> Option<String> {
    [
        "x-request-id",
        "request-id",
        "x-amzn-requestid",
        "x-amz-request-id",
        "x-goog-request-id",
        "cf-ray",
    ]
    .iter()
    .find_map(|name| header_to_string(headers, name))
}

/// Converts one header value to a displayable string.
/// 将单个响应头取值转换为可展示字符串。
fn header_to_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// Converts one JSON error fragment into a single-line display string while preserving non-string values through compact JSON serialization.
/// 将单个 JSON 错误片段转换为单行展示字符串，并通过紧凑 JSON 序列化保留非字符串取值。
fn json_value_to_error_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        other => serde_json::to_string(other)
            .ok()
            .map(|serialized| serialized.trim().to_string())
            .filter(|serialized| !serialized.is_empty() && serialized != "null"),
    }
}

/// Collects selected metadata fields from one structured provider error object and appends them as labeled summary segments.
/// 从单条结构化 Provider 错误对象中收集指定元数据字段，并把它们追加为带标签的摘要片段。
fn collect_standard_error_metadata_segments(
    metadata: &Value,
    segments: &mut Vec<String>,
    seen_segments: &mut BTreeSet<String>,
    field_names: &[&str],
) {
    let Some(object) = metadata.as_object() else {
        if let Some(value) = json_value_to_error_text(metadata) {
            push_error_segment(
                segments,
                seen_segments,
                format!("metadata={}", truncate_error_detail(&value, 240)),
            );
        }
        return;
    };

    for field_name in field_names {
        let Some(value) = object.get(*field_name).and_then(json_value_to_error_text) else {
            continue;
        };
        push_error_segment(
            segments,
            seen_segments,
            format!("{field_name}={}", truncate_error_detail(&value, 240)),
        );
    }
}

/// Appends one normalized error-summary segment only when it is non-empty and not already present.
/// 仅在片段非空且尚未出现时追加一条已归一化的错误摘要片段。
fn push_error_segment(
    segments: &mut Vec<String>,
    seen_segments: &mut BTreeSet<String>,
    segment: String,
) {
    let normalized = segment.trim();
    if normalized.is_empty() || !seen_segments.insert(normalized.to_string()) {
        return;
    }
    segments.push(normalized.to_string());
}

/// Truncates one verbose error detail to a stable maximum length so upstream payloads stay inspectable without flooding the transcript.
/// 将冗长错误细节截断到稳定上限，以便既能保留排查信息，又不会让上游载荷淹没转录内容。
fn truncate_error_detail(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_rest_endpoint, ensure_bearer_authorization_header, ensure_json_content_type,
        ensure_static_header, ensure_string_header, extract_standard_error_message,
        format_standard_error_response,
    };
    use reqwest::StatusCode;
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap};

    /// Verifies the shared REST endpoint builder trims trailing slashes and appends the final relative path exactly once.
    /// 验证共享 REST 端点构建器会裁掉尾部斜杠，并且只追加一次最终相对路径。
    #[test]
    fn builds_rest_endpoint_from_optional_base_url() {
        assert_eq!(
            build_rest_endpoint(
                Some("https://api.example.com/v1/"),
                "https://fallback/v1",
                "chat/completions"
            ),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            build_rest_endpoint(None, "https://fallback/v1", "responses"),
            "https://fallback/v1/responses"
        );
    }

    /// Verifies shared header helpers inject missing values without overwriting caller-provided headers.
    /// 验证共享请求头辅助逻辑会补入缺失值，同时不会覆盖调用方已提供的请求头。
    #[test]
    fn injects_missing_headers_without_overwriting_existing_values() {
        let mut headers = HeaderMap::new();
        ensure_bearer_authorization_header("openai", "OpenAI", "secret", &mut headers)
            .expect("bearer header should build");
        ensure_string_header(
            "anthropic",
            "Anthropic",
            &mut headers,
            "x-api-key",
            "anthropic-secret",
            "API Key ",
        )
        .expect("custom header should build");
        ensure_static_header(&mut headers, "anthropic-version", "2023-06-01");
        ensure_json_content_type(&mut headers);

        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("anthropic-secret")
        );
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    /// Verifies standard provider error extraction supports both nested `error.message` and top-level `message` payloads.
    /// 验证标准 Provider 错误提取同时支持嵌套 `error.message` 与顶层 `message` 两种载荷。
    #[test]
    fn extracts_standard_error_messages() {
        assert_eq!(
            extract_standard_error_message(r#"{ "error": { "message": "bad request" } }"#)
                .as_deref(),
            Some("bad request")
        );
        assert_eq!(
            extract_standard_error_message(r#"{ "message": "unauthorized" }"#).as_deref(),
            Some("unauthorized")
        );
    }

    /// Verifies shared HTTP error formatting keeps status, code, message, and useful metadata fields instead of collapsing everything into one generic sentence.
    /// 验证共享 HTTP 错误格式化会保留状态码、错误码、消息与有价值元数据，而不会把全部信息塌缩成一句泛化文案。
    #[test]
    fn formats_structured_provider_error_response_with_metadata() {
        let summary = format_standard_error_response(
            StatusCode::BAD_GATEWAY,
            r#"{
                "error": {
                    "message": "Provider returned error.",
                    "code": 502,
                    "type": "provider_error",
                    "metadata": {
                        "provider_name": "Google",
                        "raw": "INVALID_ARGUMENT: assistant message contains unsupported reasoning_details"
                    }
                },
                "user_id": "user_123"
            }"#,
        );

        assert!(summary.contains("HTTP 502"));
        assert!(summary.contains("type=provider_error"));
        assert!(summary.contains("Provider returned error."));
        assert!(summary.contains("provider_name=Google"));
        assert!(summary.contains(
            "raw=INVALID_ARGUMENT: assistant message contains unsupported reasoning_details"
        ));
        assert!(summary.contains("user_id=user_123"));
    }

    /// Verifies shared HTTP error formatting falls back to a compact raw-body summary when the response body is not JSON.
    /// 验证当响应正文不是 JSON 时，共享 HTTP 错误格式化会回退到紧凑原始正文摘要。
    #[test]
    fn formats_plaintext_provider_error_response() {
        let summary =
            format_standard_error_response(StatusCode::BAD_REQUEST, "plain upstream failure body");

        assert_eq!(summary, "HTTP 400 Bad Request: plain upstream failure body");
    }
}

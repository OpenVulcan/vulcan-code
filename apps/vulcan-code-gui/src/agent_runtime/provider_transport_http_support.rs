//! Shared HTTP transport support helpers reused by host-owned provider transports.
//! 供宿主自有 Provider 传输复用的共享 HTTP 传输支持辅助模块。

use crate::agent_runtime::provider_transport_adapter_support::map_provider_client_build_result;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::Value;
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

#[cfg(test)]
mod tests {
    use super::{
        build_rest_endpoint, ensure_bearer_authorization_header, ensure_json_content_type,
        ensure_static_header, ensure_string_header, extract_standard_error_message,
    };
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
}

//! HTTP client and endpoint helpers for Google Gemini.
//! Google Gemini 的 HTTP 客户端与端点辅助逻辑。

use super::GOOGLE_GEMINI_DEFAULT_BASE_URL;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_json_content_type, ensure_string_header,
};
use reqwest::header::HeaderMap;
use vulcan_core::types::CoreError as VulcanCoreError;

/// Builds one direct Google Gemini HTTP client with merged API key and caller-supplied request headers.
/// 构建一个直连 Google Gemini HTTP 客户端，并合并 API Key 与调用方提供的请求头。
pub(super) fn build_google_gemini_http_client(
    provider_id: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_string_header(
        provider_id,
        "Google Gemini",
        &mut request_headers,
        "x-goog-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, "Google Gemini", request_headers)
}

/// Builds the Google Gemini native streaming endpoint from one optional base URL and one concrete model identifier.
/// 根据可选基础地址与具体模型标识构建 Google Gemini 原生流式端点。
pub(super) fn build_google_gemini_streaming_endpoint(
    base_url: Option<&str>,
    model_id: &str,
) -> String {
    build_rest_endpoint(
        base_url,
        GOOGLE_GEMINI_DEFAULT_BASE_URL,
        format!("v1beta/models/{model_id}:streamGenerateContent?alt=sse").as_str(),
    )
}

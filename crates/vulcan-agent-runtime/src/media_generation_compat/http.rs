//! HTTP request helpers for configured media-generation providers.
//! 配置化媒体生成供应商的 HTTP 请求辅助能力。

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use super::codecs::truncate_error_text;

/// Builds one shared HTTP client used by all host-side capability adapters in this module.
/// 构建本模块全部宿主侧能力适配器共享使用的 HTTP 客户端。
pub(super) fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|error| format!("Failed to build the AI capability HTTP client: {error}"))
}

/// Sends one JSON POST request to the target MiniMax endpoint and validates both HTTP and `base_resp` status fields.
/// 向目标 MiniMax 端点发送一次 JSON POST 请求，并同时校验 HTTP 与 `base_resp` 状态字段。
pub(super) async fn send_json_request(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    body: &Value,
    provider_label: &str,
) -> Result<Value, String> {
    let response = client
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{provider_label} request failed: {error}"))?;
    parse_json_response(response, provider_label).await
}

/// Parses one HTTP JSON response and translates MiniMax-style `base_resp` errors into host-readable messages.
/// 解析单条 HTTP JSON 响应，并把 MiniMax 风格 `base_resp` 错误转换为宿主可读消息。
pub(super) async fn parse_json_response(
    response: reqwest::Response,
    provider_label: &str,
) -> Result<Value, String> {
    let status = response.status();
    let body_text = response.text().await.map_err(|error| {
        format!("Failed to read the response body from {provider_label}: {error}")
    })?;
    if !status.is_success() {
        return Err(format!(
            "{provider_label} returned HTTP {}: {}",
            status,
            truncate_error_text(&body_text, 400)
        ));
    }
    let payload: Value = serde_json::from_str(&body_text)
        .map_err(|error| format!("Failed to parse JSON returned by {provider_label}: {error}"))?;
    if let Some(status_code) = payload
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        && status_code != 0
    {
        let status_msg = payload
            .pointer("/base_resp/status_msg")
            .and_then(Value::as_str)
            .unwrap_or("Unknown error");
        return Err(format!(
            "{provider_label} returned business error {status_code}: {}",
            status_msg.trim()
        ));
    }
    Ok(payload)
}

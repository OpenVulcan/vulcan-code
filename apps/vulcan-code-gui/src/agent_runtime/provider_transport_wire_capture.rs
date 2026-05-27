//! Provider wire-request capture and dry-run helpers for transport adapters.
//! Provider 传输适配器使用的最终请求捕获与 dry-run 辅助模块。

use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use reqwest::header::{HeaderMap, HeaderName};
use serde_json::{Map as JsonMap, Value, json};
use vulcan_core::types::ProviderTurnResponse as CoreProviderTurnResponse;

/// Environment variable that makes provider transports return the final request without sending it.
/// 让 Provider 传输层返回最终请求而不实际发送的环境变量。
const PROVIDER_REQUEST_DRY_RUN_ENV: &str = "VULCAN_PROVIDER_REQUEST_DRY_RUN";

/// Emits the final provider-specific HTTP request shape to the live runtime stream.
/// 将最终 Provider 专用 HTTP 请求形态发送到实时运行时流。
///
/// # Parameters
/// 参数
/// `live_sender` receives the captured wire request for GUI inspection.
/// `live_sender` 接收用于 GUI 检查的最终请求。
/// `provider_id` is the stable provider identifier.
/// `provider_id` 是稳定 Provider 标识。
/// `model_id` is the concrete provider model identifier.
/// `model_id` 是具体 Provider 模型标识。
/// `method` is the HTTP method used by the transport adapter.
/// `method` 是传输适配器使用的 HTTP 方法。
/// `endpoint` is the final URL passed to the HTTP client.
/// `endpoint` 是传给 HTTP 客户端的最终 URL。
/// `headers` are the final request headers, with sensitive values redacted in the captured copy.
/// `headers` 是最终请求头，捕获副本会隐藏敏感值。
/// `body` is the exact JSON body passed to `reqwest`.
/// `body` 是传给 `reqwest` 的精确 JSON 请求体。
///
/// # Returns
/// 返回值
/// The captured JSON object sent to the runtime stream.
/// 已发送到运行时流的捕获 JSON 对象。
pub(crate) fn emit_provider_wire_request(
    live_sender: &ProviderLiveDeltaSender,
    provider_id: &str,
    model_id: &str,
    method: &str,
    endpoint: &str,
    headers: &HeaderMap,
    body: &Value,
) -> Value {
    let wire_request = json!({
        "providerId": provider_id,
        "modelId": model_id,
        "method": method,
        "endpoint": endpoint,
        "headers": serialize_headers_for_wire_capture(headers),
        "body": body,
        "dryRun": provider_request_dry_run_enabled(),
    });
    let _ = live_sender.send(Ok(ProviderStreamDelta::WireRequest(wire_request.clone())));
    wire_request
}

/// Returns a synthetic provider response when request dry-run is enabled.
/// 当请求 dry-run 启用时返回一份合成 Provider 响应。
///
/// # Parameters
/// 参数
/// `wire_request` is the captured final request that would have been sent.
/// `wire_request` 是原本即将发送的最终请求。
///
/// # Returns
/// 返回值
/// `Some` response when dry-run is enabled; otherwise `None`.
/// dry-run 启用时返回 `Some` 响应，否则返回 `None`。
pub(crate) fn provider_request_dry_run_response(
    wire_request: &Value,
) -> Option<CoreProviderTurnResponse> {
    if !provider_request_dry_run_enabled() {
        return None;
    }

    let pretty_request =
        serde_json::to_string_pretty(wire_request).unwrap_or_else(|_| wire_request.to_string());
    Some(CoreProviderTurnResponse {
        assistant_text: pretty_request,
        assistant_reasoning: String::new(),
        provider_message_id: None,
        provider_message_state: None,
        usage: None,
        tool_calls: Vec::new(),
        continuation: None,
    })
}

/// Returns whether provider request dry-run mode is enabled for this process.
/// 返回当前进程是否启用了 Provider 请求 dry-run 模式。
///
/// # Returns
/// 返回值
/// `true` when the environment variable is set to a common truthy value.
/// 当环境变量设置为常见真值时返回 `true`。
pub(crate) fn provider_request_dry_run_enabled() -> bool {
    std::env::var(PROVIDER_REQUEST_DRY_RUN_ENV)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Serializes request headers while redacting credentials and tokens.
/// 序列化请求头，同时隐藏凭据与令牌。
///
/// # Parameters
/// 参数
/// `headers` contains the final adapter-level request headers.
/// `headers` 包含适配器层面的最终请求头。
///
/// # Returns
/// 返回值
/// A JSON object keyed by header name.
/// 以请求头名称为键的 JSON 对象。
fn serialize_headers_for_wire_capture(headers: &HeaderMap) -> Value {
    let mut object = JsonMap::new();
    for (name, value) in headers.iter() {
        object.insert(
            name.as_str().to_string(),
            Value::String(serialize_header_value(name, value)),
        );
    }
    Value::Object(object)
}

/// Serializes one header value, redacting sensitive header names.
/// 序列化单个请求头值，并对敏感请求头名称脱敏。
///
/// # Parameters
/// 参数
/// `name` is the header name used for sensitivity detection.
/// `name` 是用于敏感性判断的请求头名称。
/// `value` is the raw header value from the request map.
/// `value` 是请求映射中的原始请求头值。
///
/// # Returns
/// 返回值
/// A display-safe header value.
/// 一个适合显示的请求头值。
fn serialize_header_value(name: &HeaderName, value: &reqwest::header::HeaderValue) -> String {
    if is_sensitive_header_name(name.as_str()) {
        return String::from("<redacted>");
    }
    value
        .to_str()
        .map(str::to_string)
        .unwrap_or_else(|_| String::from("<non-utf8>"))
}

/// Returns whether a header name should be redacted in captured request JSON.
/// 返回一个请求头名称在捕获 JSON 中是否需要脱敏。
///
/// # Parameters
/// 参数
/// `name` is the raw HTTP header name.
/// `name` 是原始 HTTP 请求头名称。
///
/// # Returns
/// 返回值
/// `true` when the header commonly carries credentials or tokens.
/// 当请求头通常携带凭据或令牌时返回 `true`。
fn is_sensitive_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("authorization")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("token")
        || lower.contains("secret")
}

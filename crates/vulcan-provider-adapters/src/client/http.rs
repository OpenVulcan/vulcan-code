//! Bounded metadata HTTP transport and controlled diagnostics.
//! 有界元数据 HTTP 传输与受控诊断。

use super::{ModelDiscoveryError, ModelDiscoveryFailureKind};
use crate::{
    AuthenticationProjection, CredentialExecutionSecret, MODEL_DISCOVERY_MAX_RESPONSE_BYTES,
    ServiceEntry,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Maximum wall time for one provider metadata or account-usage HTTP request.
/// 单次供应商元数据或账号用量 HTTP 请求的最大墙钟时间。
pub(super) const PROVIDER_METADATA_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30);

/// Maximum decoded bytes accepted from one non-catalog provider metadata response.
/// 单个非目录供应商元数据响应允许接受的最大解码字节数。
const PROVIDER_METADATA_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Closed failure variants produced while reading a bounded HTTP response body.
/// 读取有界 HTTP 响应正文时产出的封闭失败变体。
#[derive(Debug)]
enum BoundedResponseReadError {
    /// The transport failed before the complete response body arrived.
    /// 完整响应正文到达前传输失败。
    Transport(String),
    /// The response violated its byte or UTF-8 boundary.
    /// 响应违反字节或 UTF-8 边界。
    Invalid(String),
}

impl BoundedResponseReadError {
    /// Converts the shared body-reader failure into the model-discovery error contract.
    /// 将共享正文读取失败转换为模型发现错误契约。
    ///
    /// # Returns
    /// 返回值
    /// A transport or invalid-response model-discovery failure preserving the controlled message.
    /// 返回保留受控消息的传输失败或无效响应模型发现错误。
    fn into_model_discovery_error(self) -> ModelDiscoveryError {
        match self {
            Self::Transport(message) => {
                ModelDiscoveryError::new(ModelDiscoveryFailureKind::TemporarilyUnavailable, message)
            }
            Self::Invalid(message) => ModelDiscoveryError::invalid_response(message),
        }
    }
}

/// Sends one authenticated metadata GET and returns a controlled response body.
/// 发送一次已鉴权元数据 GET 并返回受控响应正文。
pub(super) async fn send_metadata_get(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    url: &str,
    secret: &CredentialExecutionSecret,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    send_metadata_get_with_authentication(http, entry, entry.authentication, url, secret, operation)
        .await
}

/// Sends one metadata GET with the exact protocol-route authentication projection.
/// 使用精确协议路由鉴权投影发送一次元数据 GET。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `entry` contributes trusted headers,
/// `authentication` is resolved from the selected route, `url` is exact,
/// `secret` is the credential snapshot, and `operation` labels controlled errors.
/// `http` 是共享客户端，`entry` 提供可信请求头，`authentication` 来自所选路由，
/// `url` 是精确地址，`secret` 是凭据快照，`operation` 标记受控错误。
///
/// # Returns
/// 返回值
/// The successful response body or a controlled metadata failure.
/// 返回成功响应正文或受控元数据失败。
pub(super) async fn send_metadata_get_with_authentication(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    authentication: AuthenticationProjection,
    url: &str,
    secret: &CredentialExecutionSecret,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    let mut request = http.get(url).timeout(PROVIDER_METADATA_REQUEST_TIMEOUT);
    for (name, value) in &entry.request_headers {
        let normalized_name = name.to_ascii_lowercase();
        if matches!(
            normalized_name.as_str(),
            "authorization"
                | "x-api-key"
                | "x-goog-api-key"
                | "chatgpt-account-id"
                | "x-goog-user-project"
        ) {
            return Err(ModelDiscoveryError::failed(format!(
                "Service entry must not override reserved metadata header `{name}`."
            )));
        }
        request = request.header(name, value);
    }
    request = match authentication {
        AuthenticationProjection::OpenVulcanAuth => request.header(
            "Authorization",
            crate::openvulcan::authorization_header(secret, url)
                .map_err(ModelDiscoveryError::failed)?,
        ),
        AuthenticationProjection::XApiKey => request.header("x-api-key", secret.expose_value()),
        AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::CodingDeviceBearer
        | AuthenticationProjection::OpenAiCodexBearer
        | AuthenticationProjection::GoogleCodeAssistBearer
        | AuthenticationProjection::GoogleGeminiBearer => {
            request.bearer_auth(secret.expose_value())
        }
        AuthenticationProjection::GoogleApiKey => {
            request.header("x-goog-api-key", secret.expose_value())
        }
    };
    match authentication {
        AuthenticationProjection::OpenAiCodexBearer => {
            let account_id = secret.account_id().ok_or_else(|| {
                ModelDiscoveryError::failed(
                    "OpenAI Codex metadata request requires a verified account identity.",
                )
            })?;
            request = request.header("chatgpt-account-id", account_id);
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            if let Some(quota_project_id) = secret.quota_project_id() {
                request = request.header("x-goog-user-project", quota_project_id);
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                ModelDiscoveryError::failed(
                    "Public Gemini OAuth metadata request requires a Google Cloud project id.",
                )
            })?;
            request = request.header("x-goog-user-project", quota_project_id);
        }
        AuthenticationProjection::XApiKey
        | AuthenticationProjection::OpenVulcanAuth
        | AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::CodingDeviceBearer
        | AuthenticationProjection::GoogleApiKey => {}
    }
    let response = request.send().await.map_err(|error| {
        ModelDiscoveryError::new(
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
            format!("{operation} request to `{url}` failed: {error}"),
        )
    })?;
    let status = response.status();
    let body_result = read_bounded_model_discovery_body(response, operation, url).await;
    if !status.is_success() {
        let diagnostic = body_result
            .as_ref()
            .map(|body| controlled_error_summary(body.as_str(), secret.expose_value()))
            .unwrap_or_else(|error| error.message.clone());
        return Err(ModelDiscoveryError::from_http_status(
            status.as_u16(),
            format!(
                "{operation} request to `{url}` returned HTTP {}: {}",
                status.as_u16(),
                diagnostic
            ),
        ));
    }
    body_result
}

/// Reads one model-discovery response through a strict decoded-byte boundary.
/// 通过严格的解码字节边界读取一个模型发现响应。
///
/// # Parameters
/// 参数
/// `response` is the exact upstream response, while `operation` and `url` scope controlled errors.
/// `response` 是精确上游响应，`operation` 与 `url` 用于限定受控错误范围。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body that fits the catalog response boundary.
/// 返回符合目录响应边界的严格 UTF-8 正文。
pub(super) async fn read_bounded_model_discovery_body(
    response: reqwest::Response,
    operation: &str,
    url: &str,
) -> Result<String, ModelDiscoveryError> {
    read_bounded_utf8_response_body(
        response,
        operation,
        url,
        MODEL_DISCOVERY_MAX_RESPONSE_BYTES,
        "catalog",
    )
    .await
    .map_err(BoundedResponseReadError::into_model_discovery_error)
}

/// Reads one non-catalog provider metadata response through a strict decoded-byte boundary.
/// 通过严格的解码字节边界读取一个非目录供应商元数据响应。
///
/// # Parameters
/// 参数
/// `response` is the exact upstream response, while `operation` and `url` scope controlled errors.
/// `response` 是精确上游响应，`operation` 与 `url` 用于限定受控错误范围。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body within the metadata boundary, or a controlled diagnostic.
/// 返回元数据边界内的严格 UTF-8 正文，或受控诊断。
pub(super) async fn read_bounded_provider_metadata_body(
    response: reqwest::Response,
    operation: &str,
    url: &str,
) -> Result<String, String> {
    read_bounded_utf8_response_body(
        response,
        operation,
        url,
        PROVIDER_METADATA_MAX_RESPONSE_BYTES,
        "metadata",
    )
    .await
    .map_err(|error| match error {
        BoundedResponseReadError::Transport(message)
        | BoundedResponseReadError::Invalid(message) => message,
    })
}

/// Parses one provider metadata response through shared byte, depth, and node-count bounds.
/// 通过共享字节、深度与节点数量上限解析一份供应商元数据响应。
///
/// # Parameters
/// 参数
/// `body` is the provider-controlled document and `label` scopes the controlled diagnostic.
/// `body` 是供应商控制的文档，`label` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// The requested closed metadata type or a deterministic size, syntax, shape, depth, or count
/// failure.
/// 返回所需封闭元数据类型，或确定性的大小、语法、结构、深度或数量失败。
pub(super) fn parse_provider_metadata_json<Response>(
    body: &str,
    label: &str,
) -> Result<Response, String>
where
    Response: DeserializeOwned,
{
    if body.len() > PROVIDER_METADATA_MAX_RESPONSE_BYTES {
        return Err(format!(
            "{label} exceeds the {PROVIDER_METADATA_MAX_RESPONSE_BYTES}-byte metadata boundary."
        ));
    }
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("{label} is invalid: {error}"))?;
    crate::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    serde_json::from_value(value).map_err(|error| format!("{label} is invalid: {error}"))
}

/// Reads one decoded HTTP response without allowing content length or compression to bypass limits.
/// 读取一个解码后的 HTTP 响应，不允许内容长度或压缩绕过限制。
///
/// # Parameters
/// 参数
/// `response` is consumed exactly once; `operation`, `url`, and `boundary_label` scope diagnostics;
/// `limit` is the maximum decoded byte count.
/// `response` 仅消费一次；`operation`、`url` 与 `boundary_label` 限定诊断；`limit`
/// 是最大解码字节数。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body or a typed transport/invalid-response failure.
/// 返回严格 UTF-8 正文，或强类型传输/无效响应失败。
async fn read_bounded_utf8_response_body(
    mut response: reqwest::Response,
    operation: &str,
    url: &str,
    limit: usize,
    boundary_label: &str,
) -> Result<String, BoundedResponseReadError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(BoundedResponseReadError::Invalid(format!(
            "{operation} response from `{url}` exceeds the {limit}-byte {boundary_label} boundary."
        )));
    }
    // body accumulates decoded chunks, so compressed responses cannot bypass the in-memory limit.
    // body 累积解码后的数据块，因此压缩响应无法绕过内存限制。
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        BoundedResponseReadError::Transport(format!(
            "{operation} response from `{url}` failed: {error}"
        ))
    })? {
        append_bounded_response_chunk(&mut body, chunk.as_ref(), limit, operation, boundary_label)?;
    }
    String::from_utf8(body).map_err(|_| {
        BoundedResponseReadError::Invalid(format!(
            "{operation} response from `{url}` is not valid UTF-8 JSON."
        ))
    })
}

/// Appends one decoded response chunk without crossing the explicit memory boundary.
/// 在不越过显式内存边界的前提下追加一个解码响应块。
///
/// # Parameters
/// 参数
/// `target` owns accepted bytes, `chunk` is the next decoded fragment, `limit` is explicit, and
/// `operation` scopes the controlled diagnostic.
/// `target` 持有已接受字节，`chunk` 是下一段解码数据，`limit` 是显式边界，`operation`
/// 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// Success after the atomic append, or an invalid-response failure before allocation growth.
/// 原子追加后返回成功；扩容前发现越界时返回无效响应失败。
#[cfg(test)]
fn append_bounded_model_discovery_chunk(
    target: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    append_bounded_response_chunk(target, chunk, limit, operation, "catalog")
        .map_err(BoundedResponseReadError::into_model_discovery_error)
}

/// Appends one decoded response chunk without crossing the caller-owned byte boundary.
/// 在不越过调用方持有的字节边界前提下追加一个解码响应块。
///
/// # Parameters
/// 参数
/// `target` owns accepted bytes, `chunk` is the next fragment, `limit` is explicit, and the
/// operation and boundary label scope diagnostics.
/// `target` 持有已接受字节，`chunk` 是下一段数据，`limit` 是显式边界，操作与边界标签限定诊断。
///
/// # Returns
/// 返回值
/// Success after the atomic append, or an invalid-response failure before allocation growth.
/// 原子追加后返回成功；扩容前发现越界时返回无效响应失败。
fn append_bounded_response_chunk(
    target: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    operation: &str,
    boundary_label: &str,
) -> Result<(), BoundedResponseReadError> {
    if target
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > limit)
    {
        return Err(BoundedResponseReadError::Invalid(format!(
            "{operation} response exceeds the {limit}-byte {boundary_label} boundary."
        )));
    }
    target.extend_from_slice(chunk);
    Ok(())
}

/// Extracts registered diagnostic fields without exposing one raw upstream response body.
/// 提取已登记诊断字段且不公开原始上游响应正文。
///
/// # Parameters
/// 参数
/// `body` is the exact provider response and `sensitive_value` is the credential used by the request.
/// `body` 是精确供应商响应，`sensitive_value` 是请求所使用的凭据。
///
/// # Returns
/// 返回值
/// A bounded structured summary with the exact credential redacted, or a fixed omission notice.
/// 返回已精确遮盖凭据的有界结构化摘要；无法识别时返回固定省略提示。
pub(super) fn controlled_error_summary(body: &str, sensitive_value: &str) -> String {
    // payload accepts only structured JSON before any provider text may enter a public diagnostic.
    // payload 只接受结构化 JSON，之后供应商文本才可能进入公开诊断。
    let Ok(payload) = parse_provider_metadata_json::<Value>(body, "Provider error response") else {
        return String::from("Provider returned an unstructured error response; body omitted.");
    };
    // diagnostics preserves only the explicitly registered common error fields in stable order.
    // diagnostics 只按稳定顺序保留显式登记的公共错误字段。
    let mut diagnostics = Vec::new();
    if let Some(error) = payload.get("error") {
        match error {
            Value::String(message) => diagnostics.push(format!("error={message}")),
            Value::Object(fields) => {
                append_controlled_error_field(&mut diagnostics, "error.code", fields.get("code"));
                append_controlled_error_field(&mut diagnostics, "error.type", fields.get("type"));
                append_controlled_error_field(
                    &mut diagnostics,
                    "error.message",
                    fields.get("message"),
                );
            }
            _ => {}
        }
    }
    append_controlled_error_field(&mut diagnostics, "code", payload.get("code"));
    append_controlled_error_field(&mut diagnostics, "type", payload.get("type"));
    append_controlled_error_field(&mut diagnostics, "message", payload.get("message"));
    // summary never falls back to serialized JSON, so unregistered response fields remain private.
    // summary 绝不回退到序列化 JSON，因此未登记响应字段保持私密。
    let summary = if diagnostics.is_empty() {
        String::from(
            "Provider returned a structured error response without registered diagnostics; body omitted.",
        )
    } else {
        diagnostics.join(" | ")
    };
    // redacted removes the exact request credential before applying the public length boundary.
    // redacted 在应用公开长度边界前移除精确请求凭据。
    let redacted = if sensitive_value.is_empty() {
        summary
    } else {
        summary.replace(sensitive_value, "[REDACTED]")
    };
    redacted.chars().take(2_000).collect()
}

/// Appends one registered scalar error field to a controlled diagnostic summary.
/// 向受控诊断摘要追加一个已登记的标量错误字段。
///
/// # Parameters
/// 参数
/// `target` owns ordered diagnostic fragments, `label` is code-owned, and `value` is one JSON field.
/// `target` 持有有序诊断片段，`label` 由代码持有，`value` 是一个 JSON 字段。
///
/// # Returns
/// 返回值
/// This helper mutates `target` only when `value` is a non-empty scalar.
/// 仅当 `value` 是非空标量时，此辅助函数才修改 `target`。
fn append_controlled_error_field(target: &mut Vec<String>, label: &str, value: Option<&Value>) {
    // text accepts only scalar JSON values; objects and arrays are intentionally omitted.
    // text 只接受 JSON 标量值；对象与数组会被有意省略。
    let text = match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(boolean)) => Some(boolean.to_string()),
        _ => None,
    };
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        target.push(format!("{label}={text}"));
    }
}

#[cfg(test)]
mod tests;

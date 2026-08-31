//! Shared bounded response-body reads with optional pre-parser wire capture.
//! 共用有界响应正文读取与可选解析前线路捕获。

use super::failure::{ProviderExecutionFailure, http_failure, stream_transport_failure};
use super::wire_capture::ProviderWireResponseCaptureGuard;
use futures_util::StreamExt;
use std::time::Duration;
use vulcan_ai_contract::RetryAfterHint;

/// Reads a non-success response body without exceeding the registered memory limit.
/// 在不超过已注册内存上限的情况下读取非成功响应体。
///
/// # Parameters
/// 参数
/// `response` is the exact response, `max_bytes` is the profile limit, `url` is diagnostic
/// context, and `status` plus `retry_after` preserve typed retry semantics on overflow.
/// `response` 是精确响应，`max_bytes` 是 Profile 上限，`url` 是诊断上下文，
/// `status` 与 `retry_after` 在溢出时保留强类型重试语义。
///
/// # Returns
/// 返回值
/// The complete UTF-8-lossy body within the limit, or a structured failure.
/// 返回上限内完整的 UTF-8 宽容解码正文，或结构化失败。
pub(crate) async fn read_bounded_response_text(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    idle_timeout: Duration,
) -> Result<String, ProviderExecutionFailure> {
    read_bounded_response_text_with_capture(
        response,
        max_bytes,
        url,
        status,
        retry_after,
        idle_timeout,
        None,
    )
    .await
}

/// Reads one bounded non-success body while optionally retaining every pre-parser response chunk.
/// 读取一份有界非成功正文，并可选保留每个进入解析器前的响应 chunk。
///
/// # Parameters
/// 参数
/// Ordinary response parameters retain the public error-body contract; `capture` receives raw
/// bytes before the smaller public error limit is enforced.
/// 常规响应参数保留公开错误正文契约；`capture` 会在执行更小的公开错误上限前接收原始字节。
///
/// # Returns
/// 返回值
/// The bounded UTF-8-lossy public body, while diagnostic capture remains independent.
/// 返回有界 UTF-8 宽容解码的公开正文，同时诊断捕获保持独立。
pub(super) async fn read_bounded_response_text_with_capture(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    idle_timeout: Duration,
    mut capture: Option<&mut ProviderWireResponseCaptureGuard>,
) -> Result<String, ProviderExecutionFailure> {
    let mut bytes = response.bytes_stream();
    let mut body = Vec::new();
    loop {
        let next_chunk = tokio::time::timeout(idle_timeout, bytes.next())
            .await
            .map_err(|_| {
                stream_transport_failure(
                    format!(
                        "Provider response body from `{url}` exceeded the registered {} ms idle timeout.",
                        idle_timeout.as_millis()
                    ),
                    false,
                )
            })?;
        let Some(chunk) = next_chunk else {
            break;
        };
        let chunk = chunk.map_err(|error| {
            stream_transport_failure(
                format!("Provider error response from `{url}` could not be read: {error}"),
                false,
            )
        })?;
        if let Some(capture) = capture.as_deref_mut() {
            capture.observe_chunk(chunk.as_ref());
        }
        if body.len().saturating_add(chunk.len()) > max_bytes {
            let mut failure = http_failure(status, retry_after, "");
            failure.error.subcategory = Some(String::from("error_body_too_large"));
            failure.error.stage = String::from("error_response_body");
            failure.error.message = format!(
                "Provider error response from `{url}` exceeded the registered {max_bytes} byte limit."
            );
            return Err(failure);
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(String::from_utf8_lossy(body.as_slice()).into_owned())
}

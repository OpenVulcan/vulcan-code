#![allow(dead_code)]

//! OpenAI-side client protocol DTOs owned by the host runtime.
//! 宿主管理的 OpenAI 客户端侧协议 DTO。

use serde::Deserialize;

/// OpenAI-style API error payload used by provider-specific request adapters.
/// 供各 Provider 请求适配器使用的 OpenAI 风格 API 错误载荷。
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct ApiErrorResponse {
    /// Human-readable provider error message.
    /// 可读的 Provider 错误消息。
    pub message: String,
}

/// OpenAI-style untagged API response used when the server may return either data or an error payload.
/// 当服务器可能返回数据或错误载荷时使用的 OpenAI 风格未标记 API 响应。
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub(crate) enum ApiResponse<T> {
    /// Successful data payload.
    /// 成功的数据载荷。
    Ok(T),
    /// Error payload returned by the provider.
    /// Provider 返回的错误载荷。
    Err(ApiErrorResponse),
}

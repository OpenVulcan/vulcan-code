#![allow(dead_code)]

//! OpenRouter-side client protocol DTOs owned by the host runtime.
//! 宿主管理的 OpenRouter 客户端侧协议 DTO。

use serde::{Deserialize, Serialize};

/// OpenRouter-style API error payload used by provider-specific request adapters.
/// 供各 Provider 请求适配器使用的 OpenRouter 风格 API 错误载荷。
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct ApiErrorResponse {
    /// Human-readable provider error message.
    /// 可读的 Provider 错误消息。
    pub message: String,
}

/// OpenRouter-style untagged API response used when the server may return either data or an error payload.
/// 当服务器可能返回数据或错误载荷时使用的 OpenRouter 风格未标记 API 响应。
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

/// OpenRouter token usage payload preserved as a standalone DTO for later request/response migration.
/// 作为独立 DTO 保留的 OpenRouter token 用量载荷，便于后续请求/响应迁移。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Usage {
    /// Number of prompt tokens consumed by the request.
    /// 本次请求消耗的提示词 token 数。
    pub prompt_tokens: usize,
    /// Number of completion tokens produced by the provider.
    /// Provider 生成的补全 token 数。
    #[serde(default)]
    pub completion_tokens: usize,
    /// Total token count reported by the provider.
    /// Provider 报告的总 token 数。
    pub total_tokens: usize,
    /// Optional cost field reported by the provider.
    /// Provider 报告的可选成本字段。
    #[serde(default)]
    pub cost: f64,
}

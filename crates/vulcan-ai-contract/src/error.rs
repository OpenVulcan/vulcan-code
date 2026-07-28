//! Provider-neutral execution errors.
//! 供应商无关的执行错误。

use serde::{Deserialize, Serialize};

/// Identifies a stable execution error category.
/// 标识一个稳定的执行错误类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorCategory {
    /// Request authentication failed.
    /// 请求鉴权失败。
    Authentication,
    /// Request is not authorized.
    /// 请求未获授权。
    PermissionDenied,
    /// Account has insufficient balance or quota.
    /// 账号余额或额度不足。
    PaymentRequired,
    /// Request was rate limited.
    /// 请求受到限流。
    RateLimited,
    /// Provider is overloaded.
    /// 供应商过载。
    ProviderOverloaded,
    /// Provider is unavailable.
    /// 供应商不可用。
    ProviderUnavailable,
    /// Request timed out.
    /// 请求超时。
    Timeout,
    /// Request validation failed.
    /// 请求校验失败。
    InvalidRequest,
    /// Content policy rejected the request or output.
    /// 内容策略拒绝请求或输出。
    ContentPolicy,
    /// User cancelled the execution.
    /// 用户取消执行。
    Cancelled,
    /// Error could not be classified.
    /// 错误无法分类。
    Unclassified,
}

/// Identifies whether transport dispatch is known to have reached the upstream boundary.
/// 标识传输分派是否已知到达上游边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOutcome {
    /// Validation failed before any network dispatch.
    /// 在任何网络分派前校验失败。
    NotDispatched,
    /// An upstream HTTP or stream response proves dispatch occurred.
    /// 上游 HTTP 或流响应证明已经分派。
    Dispatched,
    /// A transport failure occurred after dispatch began but delivery cannot be proven.
    /// 分派开始后发生传输失败，但无法证明送达情况。
    Unknown,
}

/// Carries one normalized provider or runtime error.
/// 承载一个归一化供应商或运行时错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiExecutionError {
    /// Stable category.
    /// 稳定类别。
    pub category: AiErrorCategory,
    /// Provider-specific stable subcategory when available.
    /// 可用时的供应商稳定子类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    /// Execution stage.
    /// 执行阶段。
    pub stage: String,
    /// Optional HTTP status.
    /// 可选 HTTP 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Optional upstream code.
    /// 可选上游错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
    /// Controlled user-facing message.
    /// 受控的用户可见消息。
    pub message: String,
    /// Exact known dispatch outcome.
    /// 精确已知分派结果。
    pub dispatch_outcome: DispatchOutcome,
    /// Whether semantic output has already been observed.
    /// 是否已经观察到语义输出。
    pub semantic_output_observed: bool,
    /// Whether the current execution is terminal.
    /// 当前执行是否终结。
    pub terminal: bool,
    /// Whether a client may create a new execution.
    /// 客户端是否可以创建新执行。
    pub retryable: bool,
    /// Provider-requested retry delay in seconds when reported.
    /// 供应商报告时的建议重试秒数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

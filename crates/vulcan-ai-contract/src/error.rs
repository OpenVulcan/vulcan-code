//! Provider-neutral execution errors.
//! 供应商无关的执行错误。

use serde::{Deserialize, Serialize};

/// Identifies the exact upstream transport used by one physical attempt.
/// 标识一次真实尝试所使用的精确上游传输。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTransportKind {
    /// HTTP request whose response is consumed as JSON or SSE.
    /// 响应按 JSON 或 SSE 消费的 HTTP 请求。
    Http,
    /// OpenAI Responses WebSocket request.
    /// OpenAI Responses WebSocket 请求。
    ResponsesWebsocket,
}

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
    /// Provider rejected the request because its context window was exceeded.
    /// 供应商因请求超过上下文窗口而拒绝请求。
    ContextLengthExceeded,
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

/// Identifies the authoritative source of one provider retry delay.
/// 标识一项供应商重试延迟的权威来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAfterSource {
    /// Standard Retry-After header expressed as seconds.
    /// 以秒数表达的标准 Retry-After 响应头。
    HeaderSeconds,
    /// Standard Retry-After header expressed as an HTTP date.
    /// 以 HTTP 日期表达的标准 Retry-After 响应头。
    HeaderHttpDate,
    /// Provider-registered millisecond response header.
    /// 供应商已注册的毫秒响应头。
    HeaderMilliseconds {
        /// Exact registered header name.
        /// 精确注册的响应头名称。
        header_name: String,
    },
    /// Provider-registered response-body field.
    /// 供应商已注册的响应体字段。
    ResponseBody {
        /// Exact registered JSON field path.
        /// 精确注册的 JSON 字段路径。
        field_path: String,
    },
}

/// Carries one normalized retry delay without losing sub-second precision or provenance.
/// 承载一项归一化重试延迟，且不丢失亚秒精度或来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryAfterHint {
    /// Delay in milliseconds.
    /// 延迟毫秒数。
    pub delay_milliseconds: u64,
    /// Authoritative source of the delay.
    /// 延迟的权威来源。
    pub source: RetryAfterSource,
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
    /// Exact upstream transport when dispatch reached a transport boundary.
    /// 分派到达传输边界时使用的精确上游传输。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<AiTransportKind>,
    /// WebSocket close code when the peer supplied a close frame.
    /// 对端提供关闭帧时的 WebSocket 关闭码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket_close_code: Option<u16>,
    /// Optional upstream code.
    /// 可选上游错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
    /// Optional upstream error type.
    /// 可选上游错误类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_type: Option<String>,
    /// Optional upstream parameter associated with the error.
    /// 与错误关联的可选上游参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_parameter: Option<String>,
    /// Optional controlled upstream message retained separately from the display message.
    /// 与展示消息分开保留的可选受控上游消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_message: Option<String>,
    /// Optional upstream or client request identifier.
    /// 可选上游或客户端请求编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Exact provider identifier when the route has been resolved.
    /// 路由解析后使用的精确供应商标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Exact provider entry identifier when the route has been resolved.
    /// 路由解析后使用的精确供应商入口标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    /// Exact model identifier when the route has been resolved.
    /// 路由解析后使用的精确模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Controlled user-facing message.
    /// 受控的用户可见消息。
    pub message: String,
    /// Exact known dispatch outcome.
    /// 精确已知分派结果。
    pub dispatch_outcome: DispatchOutcome,
    /// Whether semantic output has already been observed.
    /// 是否已经观察到语义输出。
    pub semantic_output_observed: bool,
    /// Whether a tool or external side effect has already occurred.
    /// 是否已经发生工具或外部副作用。
    #[serde(default)]
    pub side_effect_observed: bool,
    /// Whether the current execution is terminal.
    /// 当前执行是否终结。
    pub terminal: bool,
    /// Whether a client may create a new execution.
    /// 客户端是否可以创建新执行。
    pub retryable: bool,
    /// Provider-requested retry delay with exact provenance when reported.
    /// 供应商报告时带精确来源的建议重试延迟。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<RetryAfterHint>,
}

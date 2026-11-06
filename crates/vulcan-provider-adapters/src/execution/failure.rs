//! Typed provider execution failures with explicit dispatch and replay safety facts.
//! 带显式分派与重放安全事实的强类型供应商执行失败。

use crate::CredentialExecutionSecret;
use serde_json::Value;
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiTransportKind, DispatchOutcome, RetryAfterHint,
};
use vulcan_llm_core::LlmCoreError;
use vulcan_llm_core::ProtocolConversionError;

/// Carries one structured provider execution failure.
/// 承载一个结构化供应商执行失败。
#[derive(Debug, Clone)]
pub struct ProviderExecutionFailure {
    /// Normalized public error.
    /// 归一化公共错误。
    pub error: Box<AiExecutionError>,
}

impl std::fmt::Display for ProviderExecutionFailure {
    /// Formats the controlled execution message.
    /// 格式化受控执行消息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.error.message.as_str())
    }
}

impl std::error::Error for ProviderExecutionFailure {}

/// Creates a structured pre-dispatch failure.
/// 创建结构化分派前失败。
pub(crate) fn pre_dispatch_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("route_validation")),
            stage: String::from("prepare"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Stable subcategory for a session whose persisted reasoning history cannot reach this route.
/// 持久化推理历史无法抵达当前路由时的稳定子类别。
///
/// # Remarks
/// 说明
/// The code covers missing replay evidence, invalid same-route native items, and reasoning lost
/// by final wire projection. A valid model switch projects portable text before this boundary;
/// foreign opaque-only blocks are omitted from that request while their original history is kept.
/// 本错误码覆盖缺失回放证据、无效的同路由原生项及最终线路投影丢失推理。
/// 合法模型切换在此边界前投影可移植文本；外来纯不透明块不进入该请求，原始历史仍保留。
pub const REASONING_REPLAY_UNREPLAYABLE_SUBCATEGORY: &str = "reasoning_replay_unreplayable";

/// Fixed English runtime diagnostic for an unreplayable history; UI translation belongs to the display layer.
/// 无法回放历史的固定英文运行时诊断；界面翻译由展示层负责。
pub const REASONING_REPLAY_UNREPLAYABLE_MESSAGE: &str = "This route cannot safely replay the session's reasoning history. Start a new session or switch back to the route that produced it.";

/// Creates the structured pre-dispatch failure for an unreplayable reasoning history.
/// 为无法回放的推理历史创建结构化分派前失败。
///
/// # Parameters
/// 参数
/// `technical_reason` is the English diagnostic retained for logs and tests; it never reaches the
/// user-facing `message`, which stays the fixed copy.
/// `technical_reason` 是保留给日志与测试的英文诊断；它绝不进入用户可见的 `message`，
/// 后者始终保持固定文案。
///
/// # Returns
/// 返回值
/// A terminal, non-dispatched failure that no retry and no protocol fallback may replace.
/// 终态、未分派且不允许任何重试或协议回退替代的失败。
pub(crate) fn unreplayable_history_failure(
    technical_reason: impl Into<String>,
) -> ProviderExecutionFailure {
    let mut failure = pre_dispatch_failure(REASONING_REPLAY_UNREPLAYABLE_MESSAGE);
    failure.error.subcategory = Some(String::from(REASONING_REPLAY_UNREPLAYABLE_SUBCATEGORY));
    // The diagnostic travels in the field reserved for a controlled upstream message, which the
    // display path reads separately from `message`.
    // 诊断信息走“受控上游消息”这一预留字段，展示路径会把它与 `message` 分开读取。
    failure.error.upstream_message = Some(technical_reason.into());
    failure
}

/// Converts a deterministic protocol failure.
/// 转换确定性协议失败。
pub(super) fn protocol_failure(error: ProtocolConversionError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts an LLM-core protocol-runtime failure without restoring adapter-side registry logic.
/// 将 LLM 核心协议运行时失败转换为适配层错误，但不恢复适配层注册表逻辑。
pub(super) fn llm_core_protocol_failure(error: LlmCoreError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
            transport: None,
            websocket_close_code: error.websocket_close_code,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts one protocol failure observed after the upstream request was dispatched.
/// 转换一项在上游请求已经分派后观察到的协议失败。
///
/// # Parameters
/// 参数
/// `error` is the decoder or lifecycle failure and `semantic_output_observed` records whether
/// user-visible or tool-visible semantic output was already emitted.
/// `error` 是解码器或生命周期失败，`semantic_output_observed` 记录是否已经发出
/// 用户可见或工具可见的语义输出。
///
/// # Returns
/// 返回值
/// A terminal, non-retryable protocol failure with exact dispatch state.
/// 返回携带精确分派状态的终态、不可重试协议失败。
pub(super) fn stream_protocol_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    stream_protocol_failure_for_transport(error, semantic_output_observed, AiTransportKind::Http)
}

/// Converts one malformed stream into a terminal failure for the exact transport.
/// 将一个畸形流转换为精确传输对应的终态失败。
///
/// # Parameters
/// 参数
/// `error` is the decoder failure, `semantic_output_observed` protects replay, and `transport` is exact.
/// `error` 是解码失败，`semantic_output_observed` 保护重放，`transport` 是精确传输。
///
/// # Returns
/// 返回值
/// A non-retryable dispatched protocol failure retaining the observed transport.
/// 返回保留已观察传输的不可重试已分派协议失败。
pub(super) fn stream_protocol_failure_for_transport(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
    transport: AiTransportKind,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("response_stream"),
            http_status: None,
            transport: Some(transport),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts end-of-stream validation into a retry-safe transport failure or a protocol failure.
/// 将流结束校验转换为可安全重试的传输失败或协议失败。
///
/// # Parameters
/// 参数
/// `error` is the typed decoder completion failure and `semantic_output_observed` records whether
/// replay would duplicate user-visible or tool-visible semantics.
/// `error` 是强类型解码器结束失败；`semantic_output_observed` 记录重放是否会重复用户或工具可见语义。
///
/// # Returns
/// 返回值
/// A retryable transport failure only for a pre-semantic truncated stream; malformed protocol
/// payloads and post-semantic truncation remain terminal.
/// 仅语义输出前的截断流返回可重试传输失败；畸形协议载荷与语义输出后的截断仍保持终态。
pub(super) fn stream_completion_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    stream_completion_failure_for_transport(error, semantic_output_observed, AiTransportKind::Http)
}

/// Converts end-of-stream validation for one exact transport.
/// 转换一个精确传输的流结束校验结果。
///
/// # Parameters
/// 参数
/// `error` is the typed completion failure, `semantic_output_observed` protects replay, and `transport` is exact.
/// `error` 是强类型结束失败，`semantic_output_observed` 保护重放，`transport` 是精确传输。
///
/// # Returns
/// 返回值
/// A retryable truncation or terminal protocol failure retaining the exact transport.
/// 返回保留精确传输的可重试截断或终态协议失败。
pub(super) fn stream_completion_failure_for_transport(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
    transport: AiTransportKind,
) -> ProviderExecutionFailure {
    if error.is_truncated_stream() {
        return dispatched_stream_transport_failure(
            error.message,
            semantic_output_observed,
            transport,
            None,
        );
    }
    stream_protocol_failure_for_transport(error, semantic_output_observed, transport)
}

/// Creates a structured transport failure.
/// 创建结构化传输失败。
pub(crate) fn transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("transport"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Classifies one HTTP request-send failure without guessing whether an established request arrived upstream.
/// 对一次 HTTP 请求发送失败进行分类，且不猜测已建立连接的请求是否到达上游。
///
/// # Parameters
/// 参数
/// `error` is the exact reqwest send error and `message` is the controlled diagnostic.
/// `error` 是精确的 reqwest 发送错误，`message` 是受控诊断信息。
///
/// # Returns
/// 返回值
/// A retryable transport failure marked not dispatched only when reqwest proves connection
/// establishment failed; every other send failure keeps the conservative unknown outcome.
/// 仅当 reqwest 证明连接建立失败时返回标记为未分派的可重试传输失败；其他发送失败均保守保持未知结果。
pub(crate) fn http_send_transport_failure(
    error: &reqwest::Error,
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    // failure starts with the conservative unknown dispatch outcome used for ambiguous send errors.
    // failure 初始采用用于模糊发送错误的保守未知分派结果。
    let mut failure = transport_failure(message);
    failure.error.transport = Some(AiTransportKind::Http);
    if error.is_connect() {
        // A DNS, TCP, or TLS connection-establishment failure occurs before an HTTP request can be dispatched.
        // DNS、TCP 或 TLS 连接建立失败发生在 HTTP 请求可被分派之前。
        failure.error.dispatch_outcome = DispatchOutcome::NotDispatched;
    }
    failure
}

/// Creates one transport failure after response dispatch while preserving semantic-output safety.
/// 创建一项响应分派后的传输失败，并保留语义输出安全状态。
///
/// # Parameters
/// 参数
/// `message` is the controlled transport diagnostic and `semantic_output_observed` records
/// whether transparent replay would duplicate visible semantics.
/// `message` 是受控传输诊断，`semantic_output_observed` 记录透明重放是否会重复可见语义。
///
/// # Returns
/// 返回值
/// A structured transport failure retryable only before semantic output.
/// 返回仅在语义输出前可重试的结构化传输失败。
pub(super) fn stream_transport_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    dispatched_stream_transport_failure(
        message,
        semantic_output_observed,
        AiTransportKind::Http,
        None,
    )
}

/// Creates one dispatched HTTP stream-idle timeout that the host may replay from its clean checkpoint.
/// 创建一项已分派的 HTTP 流空闲超时，允许宿主从干净检查点重放。
///
/// # Parameters
/// 参数
/// `message` is controlled diagnostic text and `semantic_output_observed` records discarded partial output.
/// `message` 是受控诊断文本，`semantic_output_observed` 记录已丢弃的局部输出。
///
/// # Returns
/// 返回值
/// A typed timeout whose exact subcategory authorizes bounded host retry without hiding semantic-output evidence.
/// 返回一项强类型超时；其精确子类别允许有界宿主重试，同时不隐藏语义输出证据。
pub(super) fn stream_idle_timeout_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    dispatched_stream_failure(
        message,
        semantic_output_observed,
        AiTransportKind::Http,
        None,
        AiErrorCategory::Timeout,
        "stream_idle_timeout",
        true,
    )
}

/// Creates one dispatched transport failure with an optional exact WebSocket close code.
/// 创建一项已分派传输失败，并可携带精确 WebSocket 关闭码。
pub(super) fn stream_transport_failure_with_websocket_close_code(
    message: impl Into<String>,
    semantic_output_observed: bool,
    websocket_close_code: Option<u16>,
) -> ProviderExecutionFailure {
    dispatched_stream_transport_failure(
        message,
        semantic_output_observed,
        AiTransportKind::ResponsesWebsocket,
        websocket_close_code,
    )
}

/// Creates one failure for a Responses WebSocket lost before its terminal event.
/// 为终态事件之前丢失的 Responses WebSocket 创建一项失败。
///
/// Like a stream idle timeout, the loss stays retryable after partial output: the host replays
/// from its retained checkpoint on a fresh socket, and consecutive losses pin the session to HTTP.
/// 与流空闲超时一致，局部输出之后该丢失仍可重试：宿主在新 Socket 上从保留检查点重放，
/// 连续丢失会使会话固定改走 HTTP。
pub(super) fn websocket_stream_loss_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
    websocket_close_code: Option<u16>,
) -> ProviderExecutionFailure {
    dispatched_stream_failure(
        message,
        semantic_output_observed,
        AiTransportKind::ResponsesWebsocket,
        websocket_close_code,
        AiErrorCategory::ProviderUnavailable,
        "transport",
        true,
    )
}

/// Creates one dispatched stream failure for the exact observed transport.
/// 为精确观察到的传输创建一项已分派流失败。
///
/// # Parameters
/// 参数
/// `message` is controlled text, `semantic_output_observed` protects replay, `transport` is exact, and `websocket_close_code` is optional WS evidence.
/// `message` 是受控文本，`semantic_output_observed` 保护重放，`transport` 是精确传输，`websocket_close_code` 是可选 WS 证据。
///
/// # Returns
/// 返回值
/// A typed terminal transport failure retaining exact replay and transport facts.
/// 返回保留精确重放与传输事实的强类型终态传输失败。
fn dispatched_stream_transport_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
    transport: AiTransportKind,
    websocket_close_code: Option<u16>,
) -> ProviderExecutionFailure {
    dispatched_stream_failure(
        message,
        semantic_output_observed,
        transport,
        websocket_close_code,
        AiErrorCategory::ProviderUnavailable,
        "transport",
        false,
    )
}

/// Creates one dispatched stream failure with explicit retry semantics for its stable subtype.
/// 创建一项已分派流失败，并为其稳定子类型声明明确的重试语义。
///
/// # Parameters
/// 参数
/// `message` is controlled text; the remaining fields retain exact transport, category, subtype, and replay facts.
/// `message` 是受控文本；其余字段保留精确传输、类别、子类型与重放事实。
///
/// # Returns
/// 返回值
/// A typed terminal stream failure suitable for host retry classification.
/// 返回一项可供宿主重试分类使用的强类型终态流失败。
fn dispatched_stream_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
    transport: AiTransportKind,
    websocket_close_code: Option<u16>,
    category: AiErrorCategory,
    subcategory: &str,
    retry_after_semantic_output: bool,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: Some(String::from(subcategory)),
            stage: String::from("response_stream"),
            http_status: None,
            transport: Some(transport),
            websocket_close_code,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: retry_after_semantic_output || !semantic_output_observed,
            retry_after: None,
        }),
    }
}

/// Creates a dispatched protocol failure for invalid successful response headers.
/// 为无效的成功响应头创建已分派协议失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled diagnostic.
/// `message` 是受控诊断。
///
/// # Returns
/// 返回值
/// A terminal non-retryable protocol failure.
/// 返回终结且不可重试的协议失败。
pub(crate) fn response_headers_protocol_failure(
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("response_content_type")),
            stage: String::from("response_headers"),
            http_status: Some(200),
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Recognizes the official Codex overload message in either JSON or plain-text responses.
/// 识别 JSON 或纯文本响应中的官方 Codex 过载消息。
fn is_known_overload_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("our servers are currently overloaded. please try again later")
}

/// Converts one non-success HTTP response.
/// 转换一个非成功 HTTP 响应。
pub(crate) fn http_failure(
    status: u16,
    retry_after: Option<RetryAfterHint>,
    body: &str,
) -> ProviderExecutionFailure {
    let parsed = serde_json::from_str::<Value>(body).ok().filter(|value| {
        crate::provider_manifest::validate_runtime_json_bounds(value, "Provider error response")
            .is_ok()
    });
    let error = parsed
        .as_ref()
        .map(|value| value.get("error").unwrap_or(value));
    let upstream_code = error.and_then(|value| {
        value
            .get("code")
            .and_then(|code| match code {
                Value::String(code) => Some(code.clone()),
                Value::Number(code) => Some(code.to_string()),
                _ => None,
            })
            .or_else(|| {
                value
                    .get("error_type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    });
    let error_type = error
        .and_then(|value| value.get("error_type").and_then(Value::as_str))
        .or_else(|| error.and_then(|value| value.get("type").and_then(Value::as_str)));
    // upstream_message keeps plain-text provider bodies available to the shared retry classifier.
    // upstream_message 保留纯文本供应商响应，供共享重试分类器继续识别。
    let upstream_message = error
        .and_then(|value| value.get("message").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| (!body.trim().is_empty()).then(|| body.trim().to_string()));
    // Official Codex overload codes remain authoritative when a proxy reports a generic error type.
    // 当代理返回通用错误类型时，官方 Codex 过载代码仍作为权威分类依据。
    let known_overload = matches!(
        upstream_code.as_deref(),
        Some("server_is_overloaded" | "slow_down")
    ) || upstream_message
        .as_deref()
        .is_some_and(is_known_overload_message);
    // knownContextOverflow uses only provider-owned stable codes or types, never display-message guessing.
    // knownContextOverflow 仅使用供应商自有稳定代码或类型，绝不猜测展示消息。
    let known_context_overflow = matches!(
        upstream_code.as_deref().or(error_type),
        Some("context_length_exceeded" | "max_tokens_exceeded" | "token_limit_exceeded")
    );
    let category = if known_overload {
        AiErrorCategory::ProviderOverloaded
    } else if known_context_overflow {
        AiErrorCategory::ContextLengthExceeded
    } else {
        match error_type {
            Some("authentication" | "authentication_error") => AiErrorCategory::Authentication,
            Some("permission_denied" | "permission_error") => AiErrorCategory::PermissionDenied,
            // OpenAI sends `insufficient_quota` with HTTP 429; it is an exhausted balance, not a rate limit.
            // OpenAI 以 HTTP 429 发送 `insufficient_quota`；这是余额耗尽，不是限流。
            Some("payment_required" | "insufficient_quota") => AiErrorCategory::PaymentRequired,
            Some("rate_limit_exceeded" | "rate_limit_error") => AiErrorCategory::RateLimited,
            Some("provider_overloaded" | "overloaded_error") => AiErrorCategory::ProviderOverloaded,
            Some("provider_unavailable" | "server" | "api_error") => {
                AiErrorCategory::ProviderUnavailable
            }
            Some("timeout") => AiErrorCategory::Timeout,
            Some("content_policy_violation") => AiErrorCategory::ContentPolicy,
            Some("invalid_request" | "invalid_request_error" | "string_too_long") => {
                AiErrorCategory::InvalidRequest
            }
            _ => match status {
                401 => AiErrorCategory::Authentication,
                403 => AiErrorCategory::PermissionDenied,
                402 => AiErrorCategory::PaymentRequired,
                408 => AiErrorCategory::Timeout,
                429 => AiErrorCategory::RateLimited,
                500..=599 => AiErrorCategory::ProviderUnavailable,
                _ => AiErrorCategory::InvalidRequest,
            },
        }
    };
    // public_field sanitizes provider-owned scalars only after raw values have completed typed
    // classification, so redaction cannot alter retry or terminal semantics.
    // public_field 仅在原始值完成强类型分类后清理供应商字段，确保脱敏不会改变重试或终止语义。
    let public_field = |value: String| {
        crate::provider_tool_execution::sanitize_provider_diagnostic_text(value.as_str())
    };
    let public_upstream_message = upstream_message.map(&public_field);
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: error_type
                .map(str::to_string)
                .map(&public_field)
                .or_else(|| Some(String::from("http"))),
            stage: String::from("response_headers"),
            http_status: Some(status),
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: upstream_code.map(&public_field),
            upstream_type: error_type.map(str::to_string).map(&public_field),
            upstream_parameter: error
                .and_then(|value| value.get("param").and_then(Value::as_str))
                .map(str::to_string)
                .map(&public_field),
            upstream_message: public_upstream_message.clone(),
            request_id: parsed
                .as_ref()
                .and_then(|value| {
                    value
                        .get("request_id")
                        .and_then(Value::as_str)
                        .or_else(|| value.pointer("/error/request_id").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .map(&public_field),
            provider_id: None,
            entry_id: None,
            model_id: parsed
                .as_ref()
                .and_then(|value| {
                    value
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .map(&public_field),
            message: public_upstream_message.unwrap_or_else(|| {
                if body.is_empty() {
                    format!("Provider returned HTTP {status} with an empty error response.")
                } else {
                    public_field(body.trim().to_string())
                }
            }),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: matches!(
                category,
                AiErrorCategory::RateLimited
                    | AiErrorCategory::ProviderOverloaded
                    | AiErrorCategory::ProviderUnavailable
                    | AiErrorCategory::Timeout
            ),
            retry_after,
        }),
    }
}

/// Redacts the exact execution credential and bounds every public failure string.
/// 脱敏精确执行凭据并限制每个公共错误字符串。
///
/// # Parameters
/// 参数
/// `failure` is one typed local or provider failure and `secret` is the exact credential used by
/// its physical request.
/// `failure` 是一个强类型本地或供应商错误，`secret` 是其物理请求使用的精确凭据。
///
/// # Returns
/// 返回值
/// The same typed failure semantics with secret-safe bounded diagnostic fields.
/// 返回错误语义不变、诊断字段不含秘密且有界的同一强类型错误。
pub(crate) fn sanitize_provider_execution_failure(
    mut failure: ProviderExecutionFailure,
    secret: &CredentialExecutionSecret,
) -> ProviderExecutionFailure {
    sanitize_ai_execution_error(&mut failure.error, secret);
    failure
}

/// Replaces exact credential occurrences in raw provider text before any bounding.
/// 在任何限长之前替换原始供应商文本中的精确凭据。
///
/// # Parameters
/// 参数
/// `value` is raw provider-controlled text and `secret` is the credential of its request.
/// `value` 是原始供应商控制文本，`secret` 是其请求的凭据。
///
/// # Returns
/// 返回值
/// The text with every exact credential occurrence replaced; length is not bounded here.
/// 返回所有精确凭据都被替换的文本；此处不限制长度。
pub(crate) fn redact_exact_credential(value: &str, secret: &CredentialExecutionSecret) -> String {
    let sensitive_value = secret.expose_value();
    if sensitive_value.is_empty() {
        value.to_string()
    } else {
        value.replace(sensitive_value, "<redacted>")
    }
}

/// Removes secrets from and bounds every diagnostic field of one typed error in place.
/// 就地移除一个强类型错误中每个诊断字段的秘密并限制其长度。
///
/// # Parameters
/// 参数
/// `error` is a returned error or an in-stream failure event payload and `secret` is the exact
/// credential used by its physical request.
/// `error` 是返回的错误或流内失败事件负载，`secret` 是其物理请求使用的精确凭据。
pub(crate) fn sanitize_ai_execution_error(
    error: &mut AiExecutionError,
    secret: &CredentialExecutionSecret,
) {
    // sanitize replaces exact credential occurrences before the generic provider-controlled
    // URL and credential-shape filter executes.
    // sanitize 先替换精确凭据，再执行通用供应商 URL 与凭据形态过滤。
    let sanitize = |value: String| {
        let redacted = redact_exact_credential(value.as_str(), secret);
        crate::provider_tool_execution::sanitize_provider_diagnostic_text(redacted.as_str())
    };
    error.subcategory = error.subcategory.take().map(&sanitize);
    error.stage = sanitize(std::mem::take(&mut error.stage));
    error.upstream_code = error.upstream_code.take().map(&sanitize);
    error.upstream_type = error.upstream_type.take().map(&sanitize);
    error.upstream_parameter = error.upstream_parameter.take().map(&sanitize);
    error.upstream_message = error.upstream_message.take().map(&sanitize);
    error.request_id = error.request_id.take().map(&sanitize);
    error.provider_id = error.provider_id.take().map(&sanitize);
    error.entry_id = error.entry_id.take().map(&sanitize);
    error.model_id = error.model_id.take().map(&sanitize);
    error.message = sanitize(std::mem::take(&mut error.message));
}

#[cfg(test)]
mod tests;

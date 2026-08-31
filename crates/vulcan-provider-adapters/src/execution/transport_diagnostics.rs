//! Secret-safe provider transport diagnostics and host telemetry timestamps.
//! 不含秘密的供应商传输诊断与宿主遥测时间戳。

use super::failure::{ProviderExecutionFailure, transport_failure};
use serde_json::Value;
use std::time::{Instant, SystemTime};

/// Opt-in environment switch for secret-safe provider transport timing diagnostics.
/// 用于启用不含密钥的供应商传输计时诊断的可选环境变量。
const PROVIDER_TRANSPORT_TRACE_ENV: &str = "VULCAN_PROVIDER_TRANSPORT_TRACE";

/// Emits one bounded provider transport trace event.
/// 发出一条有界供应商传输跟踪事件。
pub(super) fn emit_provider_transport_trace(
    execution_id: &str,
    transport: &'static str,
    stage: &'static str,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage={stage} elapsed_ms={}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in HTTP status without response bodies, headers, or credentials.
/// 输出一项可选 HTTP 状态，且不包含响应正文、请求头或凭据。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `transport` is a fixed internal label,
/// `status` is the numeric HTTP response code, and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`transport` 是固定内部标签，`status` 是数字 HTTP
/// 响应码，`started_at` 是尝试开始时的单调时钟。
pub(super) fn emit_provider_transport_http_status_trace(
    execution_id: &str,
    transport: &'static str,
    status: u16,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=response_http_status elapsed_ms={} status={status}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in normalized failure classification without the possibly sensitive error message.
/// 输出一项可选标准化失败分类，且不包含可能敏感的错误消息。
///
/// # Parameters
/// 参数
/// `execution_id` and `transport` identify the physical attempt, `failure` supplies only normalized
/// classification fields, and `started_at` is the monotonic attempt start.
/// `execution_id` 与 `transport` 标识真实尝试，`failure` 仅提供标准化分类字段，`started_at`
/// 是尝试开始时的单调时钟。
pub(super) fn emit_provider_transport_failure_trace(
    execution_id: &str,
    transport: &'static str,
    failure: &ProviderExecutionFailure,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=execution_failed elapsed_ms={} category={:?} subcategory={:?} error_stage={:?} http_status={:?} websocket_close_code={:?} dispatch_outcome={:?} retryable={}",
        started_at.elapsed().as_millis(),
        failure.error.category,
        failure.error.subcategory,
        failure.error.stage,
        failure.error.http_status,
        failure.error.websocket_close_code,
        failure.error.dispatch_outcome,
        failure.error.retryable
    );
}

/// Emits a secret-safe failure trace and returns the unchanged typed failure.
/// 输出不含秘密的失败追踪，并原样返回强类型失败。
///
/// # Parameters
/// 参数
/// `execution_id`, `transport`, and `started_at` identify the physical attempt; `failure` is the
/// authoritative structured failure passed back to retry accounting.
/// `execution_id`、`transport` 与 `started_at` 标识真实尝试；`failure` 是返回重试统计的权威
/// 结构化失败。
///
/// # Returns
/// 返回值
/// The same failure after its normalized, non-secret fields have been traced.
/// 在记录标准化且不含秘密的字段后，返回同一失败。
pub(super) fn trace_provider_transport_failure(
    execution_id: &str,
    transport: &'static str,
    failure: ProviderExecutionFailure,
    started_at: &Instant,
) -> ProviderExecutionFailure {
    emit_provider_transport_failure_trace(execution_id, transport, &failure, started_at);
    failure
}

/// Emits one opt-in Responses compact JSON shape without response values or user content.
/// 输出一项可选 Responses 压缩 JSON 形状，且不包含响应值或用户内容。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `value` is the decoded JSON response,
/// and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`value` 是已解码 JSON 响应，`started_at` 是尝试开始时的单调时钟。
pub(super) fn emit_responses_compact_response_shape_trace(
    execution_id: &str,
    value: &Value,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    // topLevelKeys lists bounded structural names only and never serializes their values.
    // topLevelKeys 仅列出有界结构名称，绝不序列化对应值。
    let top_level_keys = value
        .as_object()
        .map(|object| {
            object
                .keys()
                .take(32)
                .map(|key| bounded_trace_label(key))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // outputKind distinguishes absent, array, object, and scalar shapes without exposing content.
    // outputKind 在不暴露内容的情况下区分缺失、数组、对象与标量形状。
    let output_kind = match value.get("output") {
        None => "absent",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
        Some(_) => "scalar",
    };
    // outputLen reports the exact array length while item details remain bounded below.
    // outputLen 报告精确数组长度，而下方 item 详情仍保持有界。
    let output_len = value
        .get("output")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    // outputItems captures only bounded item type labels and object key names.
    // outputItems 仅捕获有界 item 类型标签与对象键名。
    let output_items = value
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(16)
                .map(|item| {
                    // itemType is a bounded structural discriminator rather than response content.
                    // itemType 是有界结构判别符，而不是响应内容。
                    let item_type = item
                        .get("type")
                        .and_then(Value::as_str)
                        .map(bounded_trace_label)
                        .unwrap_or_else(|| String::from("<missing>"));
                    // itemKeys lists object member names only.
                    // itemKeys 仅列出对象成员名称。
                    let item_keys = item
                        .as_object()
                        .map(|object| {
                            object
                                .keys()
                                .take(32)
                                .map(|key| bounded_trace_label(key))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    format!("{item_type}:{item_keys:?}")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport=responses_compact stage=response_json_shape elapsed_ms={} top_level_keys={top_level_keys:?} output_kind={output_kind} output_len={} output_items={output_items:?}",
        started_at.elapsed().as_millis(),
        output_len,
    );
}

/// Normalizes one provider-controlled structural label into bounded ASCII trace text.
/// 把单个供应商控制的结构标签规范化为有界 ASCII 追踪文本。
///
/// # Parameters
/// 参数
/// `value` is a JSON object key or item type discriminator.
/// `value` 是 JSON 对象键或 item 类型判别符。
///
/// # Returns
/// 返回值
/// At most 64 safe ASCII characters with unsupported characters replaced by `_`.
/// 返回最多 64 个安全 ASCII 字符，不支持的字符替换为 `_`。
fn bounded_trace_label(value: &str) -> String {
    value
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Returns whether secret-safe provider transport tracing is explicitly enabled.
/// 返回是否已经显式启用不含密钥的供应商传输追踪。
fn provider_transport_trace_enabled() -> bool {
    std::env::var(PROVIDER_TRANSPORT_TRACE_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Returns the current host Unix timestamp in milliseconds for transport telemetry.
/// 返回用于传输遥测的当前宿主 Unix 毫秒时间戳。
pub(super) fn provider_transport_unix_millis() -> Result<i64, ProviderExecutionFailure> {
    let millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| transport_failure(format!("Host clock precedes Unix epoch: {error}")))?
        .as_millis();
    i64::try_from(millis)
        .map_err(|_| transport_failure("Host Unix timestamp exceeds SQLite INTEGER."))
}

#[cfg(test)]
mod tests;

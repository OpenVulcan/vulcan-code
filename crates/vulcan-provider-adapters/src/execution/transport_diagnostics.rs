//! Secret-safe provider transport diagnostics and host telemetry timestamps.
//! 不含秘密的供应商传输诊断与宿主遥测时间戳。

use super::failure::{ProviderExecutionFailure, transport_failure};
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

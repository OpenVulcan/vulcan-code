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

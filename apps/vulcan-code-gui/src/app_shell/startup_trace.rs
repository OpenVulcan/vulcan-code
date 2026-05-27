//! Opt-in startup timing trace helpers for diagnosing slow first-frame paths.
//! 用于诊断慢首帧路径的可选启动耗时追踪辅助模块。

use std::sync::OnceLock;
use std::time::Instant;

/// Environment variable that enables startup timing logs on stderr.
/// 启用 stderr 启动耗时日志的环境变量。
const STARTUP_TRACE_ENV: &str = "VULCAN_STARTUP_TRACE";

/// Shared startup trace origin captured by the first emitted mark.
/// 由第一条追踪标记捕获的共享启动追踪原点。
static STARTUP_TRACE_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Cached decision for whether startup tracing is enabled.
/// 是否启用启动追踪的缓存判定。
static STARTUP_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();

/// Emits one startup timing mark when `VULCAN_STARTUP_TRACE` is enabled.
/// 当 `VULCAN_STARTUP_TRACE` 启用时输出一条启动耗时标记。
///
/// # Parameters
/// 参数
/// `label` describes the startup phase that just started or completed.
/// `label` 描述刚刚开始或完成的启动阶段。
pub(crate) fn mark(label: impl AsRef<str>) {
    if !startup_trace_enabled() {
        return;
    }

    let now = Instant::now();
    let origin = STARTUP_TRACE_ORIGIN.get_or_init(|| now);
    eprintln!(
        "[vulcan-startup] +{:>8.3} ms {}",
        now.duration_since(*origin).as_secs_f64() * 1000.0,
        label.as_ref()
    );
}

/// Returns whether startup tracing is enabled for this process.
/// 返回当前进程是否启用了启动追踪。
///
/// # Returns
/// 返回值
/// `true` when the environment variable is set to a common truthy value.
/// 当环境变量被设置为常见真值时返回 `true`。
fn startup_trace_enabled() -> bool {
    *STARTUP_TRACE_ENABLED.get_or_init(|| {
        std::env::var(STARTUP_TRACE_ENV)
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

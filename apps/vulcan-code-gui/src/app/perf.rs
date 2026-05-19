//! Transcript performance diagnostics for GUI-side streaming update phases.
//! 图形界面侧流式更新阶段使用的转录性能诊断辅助。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TRANSCRIPT_PERF_STDOUT_ENV_KEY: &str = "VULCAN_TRANSCRIPT_PROFILE_STDOUT";
const TRANSCRIPT_PERF_THRESHOLD_MS_ENV_KEY: &str = "VULCAN_TRANSCRIPT_PROFILE_THRESHOLD_MS";
const DEFAULT_TRANSCRIPT_PERF_THRESHOLD_MS: u64 = 200;

/// Immutable runtime configuration that decides whether GUI transcript profiling should emit stdout diagnostics.
/// 决定图形界面转录性能分析是否输出 stdout 诊断的不可变运行时配置。
#[derive(Debug, Clone, Copy)]
struct TranscriptPerfConfig {
    /// Whether GUI-side transcript profiling is enabled.
    /// 是否启用图形界面侧转录性能分析。
    enabled: bool,
    /// Minimum total duration that must be exceeded before one trace is printed.
    /// 打印一条追踪日志前必须超过的最小总耗时。
    threshold: Duration,
}

/// One staged GUI transcript performance trace that collects phase timings before optionally printing one stdout summary.
/// 一条分阶段收集耗时并在必要时输出 stdout 汇总的图形界面转录性能追踪。
pub(super) struct TranscriptPerfTrace {
    /// Static operation label identifying the measured GUI phase.
    /// 标识当前被测图形界面阶段的静态操作标签。
    label: &'static str,
    /// Human-readable contextual payload attached to the stdout summary.
    /// 附加到 stdout 汇总中的人类可读上下文载荷。
    context: String,
    /// Monotonic start time of the whole trace.
    /// 整条追踪的单调起始时间。
    started_at: Instant,
    /// Per-phase timing samples collected in execution order.
    /// 按执行顺序收集的各阶段耗时样本。
    phases: Vec<(&'static str, Duration)>,
    /// Whether the trace should stay silent.
    /// 当前追踪是否应保持静默。
    enabled: bool,
}

impl TranscriptPerfTrace {
    /// Starts one GUI transcript performance trace that stays silent unless stdout profiling is enabled.
    /// 启动一条图形界面转录性能追踪；除非启用 stdout 分析，否则它会保持静默。
    pub(super) fn begin(label: &'static str, context: impl Into<String>) -> Self {
        let config = transcript_perf_config();
        Self {
            label,
            context: context.into(),
            started_at: Instant::now(),
            phases: Vec::new(),
            enabled: config.enabled,
        }
    }

    /// Records one completed phase using the provided phase start time.
    /// 使用提供的阶段开始时间记录一段已完成阶段。
    pub(super) fn checkpoint(&mut self, phase_label: &'static str, phase_started_at: Instant) {
        if !self.enabled {
            return;
        }

        self.phases.push((phase_label, phase_started_at.elapsed()));
    }

    /// Finishes the trace and prints one stdout summary only when the configured slow-path threshold is exceeded.
    /// 结束当前追踪，并且仅在超过配置的慢路径阈值时打印一条 stdout 汇总。
    pub(super) fn finish(self) {
        if !self.enabled {
            return;
        }

        let total = self.started_at.elapsed();
        let config = transcript_perf_config();
        if total < config.threshold {
            return;
        }

        let phases = self
            .phases
            .iter()
            .map(|(phase_label, duration)| format!("{phase_label}={:.2}ms", duration.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "[transcript-prof][gui][{}] total={:.2}ms {}{}",
            self.label,
            total.as_secs_f64() * 1000.0,
            self.context,
            if phases.is_empty() {
                String::new()
            } else {
                format!(" phases=[{phases}]")
            }
        );
    }
}

/// Returns the lazily initialized GUI transcript profiling configuration derived from environment variables.
/// 返回根据环境变量惰性初始化得到的图形界面转录性能分析配置。
fn transcript_perf_config() -> &'static TranscriptPerfConfig {
    static CONFIG: OnceLock<TranscriptPerfConfig> = OnceLock::new();
    CONFIG.get_or_init(|| TranscriptPerfConfig {
        enabled: transcript_perf_env_enabled(TRANSCRIPT_PERF_STDOUT_ENV_KEY),
        threshold: Duration::from_millis(
            std::env::var(TRANSCRIPT_PERF_THRESHOLD_MS_ENV_KEY)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(DEFAULT_TRANSCRIPT_PERF_THRESHOLD_MS),
        ),
    })
}

/// Returns whether one boolean-like environment variable should be treated as enabled.
/// 返回某个类布尔环境变量是否应被视为已启用。
fn transcript_perf_env_enabled(env_key: &str) -> bool {
    std::env::var(env_key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

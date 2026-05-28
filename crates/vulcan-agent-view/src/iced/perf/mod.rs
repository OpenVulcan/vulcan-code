//! Transcript widget performance diagnostics for render-cache and layout phases.
//! 渲染缓存与布局阶段使用的转录组件性能诊断辅助。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TRANSCRIPT_PERF_STDOUT_ENV_KEY: &str = "VULCAN_TRANSCRIPT_PROFILE_STDOUT";
const TRANSCRIPT_PERF_THRESHOLD_MS_ENV_KEY: &str = "VULCAN_TRANSCRIPT_PROFILE_THRESHOLD_MS";
const DEFAULT_TRANSCRIPT_PERF_THRESHOLD_MS: u64 = 200;

/// Immutable runtime configuration controlling whether agent-view transcript profiling emits stdout diagnostics.
/// 控制 agent-view 转录性能分析是否输出 stdout 诊断的不可变运行时配置。
#[derive(Debug, Clone, Copy)]
struct TranscriptPerfConfig {
    /// Whether agent-view transcript profiling is enabled.
    /// 是否启用 agent-view 转录性能分析。
    enabled: bool,
    /// Minimum total duration required before one trace is printed.
    /// 打印单条追踪日志前所需的最小总耗时。
    threshold: Duration,
}

/// One staged transcript performance trace used to collect render-cache and layout timings before printing one slow-path stdout summary.
/// 一条用于收集渲染缓存与布局耗时，并在慢路径时输出 stdout 汇总的转录性能追踪。
pub(super) struct TranscriptPerfTrace {
    /// Static operation label identifying the measured transcript phase.
    /// 标识当前被测转录阶段的静态操作标签。
    label: &'static str,
    /// Human-readable contextual payload printed with the trace.
    /// 随追踪一起打印的人类可读上下文载荷。
    context: String,
    /// Monotonic start time of the whole trace.
    /// 整条追踪的单调起始时间。
    started_at: Instant,
    /// Ordered phase timing samples collected during execution.
    /// 执行过程中按顺序收集的阶段耗时样本。
    phases: Vec<(&'static str, Duration)>,
    /// Extra contextual notes appended by the measured operation.
    /// 由被测操作追加的额外上下文备注。
    notes: Vec<String>,
    /// Whether the trace should stay silent.
    /// 当前追踪是否应保持静默。
    enabled: bool,
}

impl TranscriptPerfTrace {
    /// Starts one transcript performance trace that remains silent unless stdout profiling is enabled.
    /// 启动一条转录性能追踪；除非启用 stdout 分析，否则它会保持静默。
    pub(super) fn begin(label: &'static str, context: impl Into<String>) -> Self {
        let config = transcript_perf_config();
        Self {
            label,
            context: context.into(),
            started_at: Instant::now(),
            phases: Vec::new(),
            notes: Vec::new(),
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

    /// Appends one free-form diagnostic note that will be printed if this trace crosses the slow-path threshold.
    /// 追加一条自由格式诊断备注；如果该追踪超过慢路径阈值则会输出。
    pub(super) fn note(&mut self, note: impl Into<String>) {
        if !self.enabled {
            return;
        }

        self.notes.push(note.into());
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
            .map(|(phase_label, duration)| {
                format!("{phase_label}={:.2}ms", duration.as_secs_f64() * 1000.0)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let notes = if self.notes.is_empty() {
            String::new()
        } else {
            format!(" notes=[{}]", self.notes.join(", "))
        };
        println!(
            "[transcript-prof][agent-view][{}] total={:.2}ms {}{}{}",
            self.label,
            total.as_secs_f64() * 1000.0,
            self.context,
            if phases.is_empty() {
                String::new()
            } else {
                format!(" phases=[{phases}]")
            },
            notes
        );
    }
}

/// Returns the lazily initialized transcript profiling configuration derived from environment variables.
/// 返回根据环境变量惰性初始化得到的转录性能分析配置。
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

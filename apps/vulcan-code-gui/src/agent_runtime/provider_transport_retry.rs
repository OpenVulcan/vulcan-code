//! Provider retry policy and retryability classification for host-controlled transports.
//! 宿主控制 Provider 传输使用的重试策略与可重试性分类。

use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::provider_config::PersistedProviderRetryConfiguration;
use std::time::Duration;
use vulcan_core::types::CoreError as VulcanCoreError;

/// Runtime provider retry policy derived from persisted user configuration.
/// 从用户持久化配置派生出的运行时 Provider 重试策略。
#[derive(Debug, Clone)]
pub(crate) struct ProviderRetryPolicy {
    /// Whether automatic retry is enabled for this execution path.
    /// 当前执行路径是否启用自动重试。
    enabled: bool,
    /// Maximum retry count after the first attempt.
    /// 初始尝试后的最大重试次数。
    max_retries: u32,
    /// Initial delay before the first retry.
    /// 第一次重试前的初始等待时间。
    initial_delay: Duration,
    /// Maximum local backoff delay.
    /// 本地退避等待时间上限。
    max_delay: Duration,
    /// Exponential backoff factor.
    /// 指数退避倍数。
    backoff_factor: f64,
    /// Deterministic jitter percentage.
    /// 确定性抖动百分比。
    jitter_percent: u8,
    /// Whether provider retry-after hints should be honored.
    /// 是否遵守 Provider 的 retry-after 提示。
    respect_retry_after: bool,
    /// Whether HTTP 429 failures may be retried.
    /// 是否允许重试 HTTP 429 失败。
    retry_429: bool,
}

impl ProviderRetryPolicy {
    /// Builds a runtime retry policy from persisted configuration.
    /// 根据持久化配置构建运行时重试策略。
    pub(crate) fn from_config(config: &PersistedProviderRetryConfiguration) -> Self {
        Self {
            enabled: config.enabled,
            max_retries: config.max_retries.min(100),
            initial_delay: Duration::from_millis(config.initial_delay_ms.max(1)),
            max_delay: Duration::from_millis(
                config.max_delay_ms.max(config.initial_delay_ms.max(1)),
            ),
            backoff_factor: if config.backoff_factor.is_finite() {
                config.backoff_factor.clamp(1.0, 8.0)
            } else {
                2.0
            },
            jitter_percent: config.jitter_percent.min(80),
            respect_retry_after: config.respect_retry_after,
            retry_429: config.retry_429,
        }
    }

    /// Builds a disabled retry policy for capture-only or explicitly non-retryable paths.
    /// 为仅捕获或显式不可重试路径构建禁用的重试策略。
    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            max_retries: 0,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
            backoff_factor: 1.0,
            jitter_percent: 0,
            respect_retry_after: false,
            retry_429: false,
        }
    }

    /// Classifies one failed attempt and returns the next retry decision when retry is still allowed.
    /// 对一次失败尝试进行分类，并在仍允许重试时返回下一次重试决策。
    pub(crate) fn next_retry(
        &self,
        error: &VulcanCoreError,
        completed_retries: u32,
        unsafe_delta_seen: bool,
    ) -> Option<ProviderRetryDecision> {
        if !self.enabled || completed_retries >= self.max_retries || unsafe_delta_seen {
            return None;
        }

        let reason = classify_retry_reason(error, self.retry_429)?;
        let retry_after = self
            .respect_retry_after
            .then(|| retry_after_from_error(error))
            .flatten();
        let delay = retry_after.unwrap_or_else(|| self.local_delay(completed_retries + 1));
        Some(ProviderRetryDecision {
            retry_number: completed_retries + 1,
            max_retries: self.max_retries,
            delay,
            reason,
        })
    }

    /// Calculates the local exponential backoff delay for one 1-based retry number.
    /// 计算某个从 1 开始的重试序号对应的本地指数退避等待时间。
    fn local_delay(&self, retry_number: u32) -> Duration {
        let exponent = retry_number.saturating_sub(1) as i32;
        let base_ms = self.initial_delay.as_millis() as f64 * self.backoff_factor.powi(exponent);
        let jitter = deterministic_jitter_multiplier(retry_number, self.jitter_percent);
        let delayed_ms = (base_ms * jitter).round() as u64;
        Duration::from_millis(delayed_ms).min(self.max_delay)
    }
}

/// One concrete retry decision emitted after a failed provider attempt.
/// 一次 Provider 尝试失败后产出的具体重试决策。
#[derive(Debug, Clone)]
pub(crate) struct ProviderRetryDecision {
    /// 1-based retry number that will be executed next.
    /// 即将执行的从 1 开始的重试序号。
    pub(crate) retry_number: u32,
    /// Configured retry limit.
    /// 已配置的重试上限。
    pub(crate) max_retries: u32,
    /// Delay before the next retry.
    /// 下一次重试前的等待时间。
    pub(crate) delay: Duration,
    /// Human-readable retry reason.
    /// 人类可读的重试原因。
    pub(crate) reason: String,
}

/// Returns whether one streamed delta makes automatic transparent retry unsafe.
/// 判断某个已流式发出的增量是否会让透明自动重试变得不安全。
pub(crate) fn provider_delta_makes_retry_unsafe(delta: &ProviderStreamDelta) -> bool {
    matches!(
        delta,
        ProviderStreamDelta::Thinking(_)
            | ProviderStreamDelta::Output(_)
            | ProviderStreamDelta::RuntimeEvent(_)
            | ProviderStreamDelta::FinalConversationState(_, _)
    )
}

/// Builds a stable jitter multiplier without depending on global randomness.
/// 构建不依赖全局随机数的稳定抖动倍数。
fn deterministic_jitter_multiplier(retry_number: u32, jitter_percent: u8) -> f64 {
    if jitter_percent == 0 {
        return 1.0;
    }
    let span = jitter_percent as f64 / 100.0;
    let bucket = ((retry_number as u64 * 37 + 17) % 101) as f64 / 100.0;
    1.0 - span + bucket * span * 2.0
}

/// Classifies one provider error into a retry reason.
/// 将单个 Provider 错误分类为重试原因。
fn classify_retry_reason(error: &VulcanCoreError, retry_429: bool) -> Option<String> {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();

    if matches!(
        error,
        VulcanCoreError::InvalidState { .. } | VulcanCoreError::ToolFailure { .. }
    ) {
        return None;
    }
    if contains_non_retryable_marker(&lower) {
        return None;
    }
    if retry_429 && contains_status(&lower, 429) {
        return Some(String::from("Provider rate limit"));
    }
    for status in [408, 425, 500, 502, 503, 504, 529] {
        if contains_status(&lower, status) {
            return Some(format!("Provider HTTP {status}"));
        }
    }
    if contains_transport_marker(&lower) {
        return Some(String::from("Provider transport failure"));
    }
    None
}

/// Returns whether the error text contains a known non-retryable marker.
/// 判断错误文本是否包含已知不可重试标记。
fn contains_non_retryable_marker(lower: &str) -> bool {
    [
        "请求预算门阻止发送",
        "context window",
        "context overflow",
        "上下文",
        "schema",
        "invalid request",
        "bad request",
        "unauthorized",
        "forbidden",
        "api key",
        "鉴权",
        "配额耗尽",
        "quota",
        "content policy",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || [400, 401, 403, 404, 413, 422]
            .iter()
            .any(|status| contains_status(lower, *status))
}

/// Returns whether the error text contains a known transient transport marker.
/// 判断错误文本是否包含已知瞬态传输错误标记。
fn contains_transport_marker(lower: &str) -> bool {
    [
        "请求发送失败",
        "sse 解码失败",
        "timeout",
        "timed out",
        "connection reset",
        "connection closed",
        "connection refused",
        "network",
        "tcp",
        "dns",
        "broken pipe",
        "unexpected eof",
        "stream closed",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Returns whether an error string contains one concrete HTTP status.
/// 判断错误字符串是否包含某个具体 HTTP 状态码。
fn contains_status(lower: &str, status: u16) -> bool {
    let status_text = status.to_string();
    lower.contains(&format!("http {status_text}"))
        || lower.contains(&format!("status {status_text}"))
        || lower.contains(&format!("status={status_text}"))
}

/// Extracts a retry-after delay from one provider error string when available.
/// 在可用时从 Provider 错误字符串中提取 retry-after 等待时间。
fn retry_after_from_error(error: &VulcanCoreError) -> Option<Duration> {
    let text = error.to_string();
    let marker = "retry_after_ms=";
    let index = text.find(marker)? + marker.len();
    let value = text[index..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let millis = value.parse::<u64>().ok()?;
    Some(Duration::from_millis(millis))
}

#[cfg(test)]
mod tests {
    //! Focused retry policy tests for provider transport failure classification.
    //! 面向 Provider 传输失败分类的定向重试策略测试。

    use super::{ProviderRetryPolicy, provider_delta_makes_retry_unsafe};
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::provider_config::PersistedProviderRetryConfiguration;
    use std::time::Duration;
    use vulcan_core::types::CoreError as VulcanCoreError;

    /// Builds a deterministic retry policy for assertions.
    /// 构建便于断言的确定性重试策略。
    fn policy() -> ProviderRetryPolicy {
        ProviderRetryPolicy::from_config(&PersistedProviderRetryConfiguration {
            enabled: true,
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 1_000,
            backoff_factor: 2.0,
            jitter_percent: 0,
            respect_retry_after: true,
            retry_429: true,
        })
    }

    /// Builds one provider failure with stable provider metadata.
    /// 构建一条带稳定 Provider 元数据的 Provider 失败。
    fn provider_error(message: &str) -> VulcanCoreError {
        VulcanCoreError::ProviderFailure {
            provider_id: String::from("test"),
            message: message.to_string(),
        }
    }

    /// Verifies exponential backoff for retryable transient HTTP failures.
    /// 验证可重试瞬态 HTTP 失败会使用指数退避。
    #[test]
    fn retries_transient_http_failures_with_exponential_backoff() {
        let first = policy()
            .next_retry(&provider_error("HTTP 503 Service Unavailable"), 0, false)
            .expect("503 should be retryable");
        let second = policy()
            .next_retry(&provider_error("HTTP 503 Service Unavailable"), 1, false)
            .expect("503 should be retryable again");

        assert_eq!(first.retry_number, 1);
        assert_eq!(first.delay, Duration::from_millis(100));
        assert_eq!(second.retry_number, 2);
        assert_eq!(second.delay, Duration::from_millis(200));
    }

    /// Verifies provider retry-after hints override local backoff.
    /// 验证 Provider 的 retry-after 提示会覆盖本地退避。
    #[test]
    fn honors_retry_after_hint_when_present() {
        let decision = policy()
            .next_retry(
                &provider_error("HTTP 429 Too Many Requests | retry_after_ms=2500"),
                0,
                false,
            )
            .expect("429 with retry-after should be retryable");

        assert_eq!(decision.delay, Duration::from_millis(2_500));
    }

    /// Verifies permanent failures and unsafe visible deltas disable transparent retry.
    /// 验证永久失败和不安全可见增量会禁用透明重试。
    #[test]
    fn does_not_retry_bad_request_or_unsafe_stream_output() {
        assert!(
            policy()
                .next_retry(&provider_error("HTTP 400 Bad Request"), 0, false)
                .is_none()
        );
        assert!(
            policy()
                .next_retry(&provider_error("HTTP 503 Service Unavailable"), 0, true)
                .is_none()
        );
    }

    /// Verifies visible assistant output marks the attempt as unsafe to replay.
    /// 验证可见助手输出会把本次尝试标记为不适合重放。
    #[test]
    fn marks_visible_model_output_as_retry_unsafe() {
        assert!(provider_delta_makes_retry_unsafe(
            &ProviderStreamDelta::Output(String::from("hello"))
        ));
    }
}

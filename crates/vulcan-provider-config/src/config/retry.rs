//! Provider retry configuration retained as a host execution policy.
//! 作为宿主执行策略保留的 Provider 重试配置。

use serde::{Deserialize, Serialize};

/// Stable transcript turn-page size options exposed by the settings page.
/// 设置页面暴露的稳定转录轮次分页选项集合。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS: [usize; 4] = [20, 30, 40, 50];

/// Default transcript turn-page size used when no override exists.
/// 未提供覆盖值时使用的默认转录轮次分页大小。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT: usize = 30;

/// Persisted provider retry behavior shared by public-contract transports.
/// 公共契约传输链路共享的持久化 Provider 重试行为配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedProviderRetryConfiguration {
    /// Whether automatic retry is enabled for retryable failures.
    /// 是否为可重试失败启用自动重试。
    #[serde(default = "default_provider_retry_enabled")]
    pub enabled: bool,
    /// Maximum retries after the initial request.
    /// 初始请求后的最大重试次数。
    #[serde(rename = "maxRetries", default = "default_provider_retry_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay in milliseconds.
    /// 初始重试等待毫秒数。
    #[serde(
        rename = "initialDelayMs",
        default = "default_provider_retry_initial_delay_ms"
    )]
    pub initial_delay_ms: u64,
    /// Maximum retry delay in milliseconds.
    /// 最大重试等待毫秒数。
    #[serde(rename = "maxDelayMs", default = "default_provider_retry_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier.
    /// 指数退避倍数。
    #[serde(
        rename = "backoffFactor",
        default = "default_provider_retry_backoff_factor"
    )]
    pub backoff_factor: f64,
    /// Deterministic jitter percentage.
    /// 确定性抖动百分比。
    #[serde(
        rename = "jitterPercent",
        default = "default_provider_retry_jitter_percent"
    )]
    pub jitter_percent: u8,
    /// Whether provider Retry-After hints override local backoff.
    /// Provider 的 Retry-After 提示是否覆盖本地退避。
    #[serde(
        rename = "respectRetryAfter",
        default = "default_provider_retry_respect_retry_after"
    )]
    pub respect_retry_after: bool,
    /// Whether retryable HTTP 429 failures are retried.
    /// 是否重试可重试的 HTTP 429 失败。
    #[serde(rename = "retry429", default = "default_provider_retry_429")]
    pub retry_429: bool,
    /// Rate-limit-specific retry schedule.
    /// 限流专用重试调度规则。
    #[serde(
        rename = "rateLimit",
        default,
        skip_serializing_if = "PersistedProviderRateLimitRetryConfiguration::is_default"
    )]
    pub rate_limit: PersistedProviderRateLimitRetryConfiguration,
}

impl Default for PersistedProviderRetryConfiguration {
    /// Builds the default host retry policy.
    /// 构建默认宿主重试策略。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The complete default retry configuration.
    /// 返回完整默认重试配置。
    fn default() -> Self {
        Self {
            enabled: default_provider_retry_enabled(),
            max_retries: default_provider_retry_max_retries(),
            initial_delay_ms: default_provider_retry_initial_delay_ms(),
            max_delay_ms: default_provider_retry_max_delay_ms(),
            backoff_factor: default_provider_retry_backoff_factor(),
            jitter_percent: default_provider_retry_jitter_percent(),
            respect_retry_after: default_provider_retry_respect_retry_after(),
            retry_429: default_provider_retry_429(),
            rate_limit: PersistedProviderRateLimitRetryConfiguration::default(),
        }
    }
}

impl PersistedProviderRetryConfiguration {
    /// Returns whether this block equals the built-in default.
    /// 返回当前配置块是否等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Backend-configurable schedule for rate-limit retries.
/// 后台可配置的限流重试调度规则。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedProviderRateLimitRetryConfiguration {
    /// Maximum retries, where zero means retry until cancellation.
    /// 最大重试次数，零表示持续重试直至取消。
    #[serde(rename = "maxRetries", default)]
    pub max_retries: u32,
    /// Delay before the first rate-limit retry in milliseconds.
    /// 首次限流重试前的等待毫秒数。
    #[serde(
        rename = "initialDelayMs",
        default = "default_provider_rate_limit_retry_initial_delay_ms"
    )]
    pub initial_delay_ms: u64,
    /// Configured delay ceiling in milliseconds.
    /// 配置的等待毫秒数上限。
    #[serde(
        rename = "maxDelayMs",
        default = "default_provider_rate_limit_retry_max_delay_ms"
    )]
    pub max_delay_ms: u64,
    /// Backoff multiplier, where one selects a fixed interval.
    /// 退避倍数，一表示固定间隔。
    #[serde(
        rename = "backoffFactor",
        default = "default_provider_rate_limit_retry_backoff_factor"
    )]
    pub backoff_factor: f64,
}

impl Default for PersistedProviderRateLimitRetryConfiguration {
    /// Builds the cancellation-bound default rate-limit schedule.
    /// 构建默认的仅由取消终止的限流重试调度规则。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The complete default rate-limit schedule.
    /// 返回完整默认限流重试调度规则。
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_delay_ms: default_provider_rate_limit_retry_initial_delay_ms(),
            max_delay_ms: default_provider_rate_limit_retry_max_delay_ms(),
            backoff_factor: default_provider_rate_limit_retry_backoff_factor(),
        }
    }
}

impl PersistedProviderRateLimitRetryConfiguration {
    /// Returns whether this schedule equals the built-in default.
    /// 返回当前调度规则是否等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Returns the default automatic retry switch.
/// 返回默认自动重试开关。
fn default_provider_retry_enabled() -> bool {
    true
}

/// Returns the default general retry count.
/// 返回默认常规重试次数。
fn default_provider_retry_max_retries() -> u32 {
    5
}

/// Returns the default general retry initial delay.
/// 返回默认常规重试初始等待时间。
fn default_provider_retry_initial_delay_ms() -> u64 {
    2_000
}

/// Returns the default general retry delay ceiling.
/// 返回默认常规重试等待上限。
fn default_provider_retry_max_delay_ms() -> u64 {
    30_000
}

/// Returns the default general retry backoff multiplier.
/// 返回默认常规重试退避倍数。
fn default_provider_retry_backoff_factor() -> f64 {
    2.0
}

/// Returns the default general retry jitter percentage.
/// 返回默认常规重试抖动百分比。
fn default_provider_retry_jitter_percent() -> u8 {
    20
}

/// Returns the default Retry-After handling switch.
/// 返回默认 Retry-After 处理开关。
fn default_provider_retry_respect_retry_after() -> bool {
    true
}

/// Returns the default HTTP 429 retry switch.
/// 返回默认 HTTP 429 重试开关。
fn default_provider_retry_429() -> bool {
    true
}

/// Returns the default first rate-limit retry delay.
/// 返回默认首次限流重试等待时间。
fn default_provider_rate_limit_retry_initial_delay_ms() -> u64 {
    5_000
}

/// Returns the default rate-limit retry delay ceiling.
/// 返回默认限流重试等待上限。
fn default_provider_rate_limit_retry_max_delay_ms() -> u64 {
    1_800_000
}

/// Returns the default rate-limit retry multiplier.
/// 返回默认限流重试倍数。
fn default_provider_rate_limit_retry_backoff_factor() -> f64 {
    2.0
}

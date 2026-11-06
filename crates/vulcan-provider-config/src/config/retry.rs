//! Provider retry configuration retained as a host execution policy.
//! 作为宿主执行策略保留的 Provider 重试配置。

use serde::{Deserialize, Serialize};

/// Hard host ceiling for ordinary transient retries after the initial request.
/// 初始请求后的普通瞬态重试所使用的宿主硬上限。
pub const PROVIDER_RETRY_MAX_RETRIES: u32 = 100;

/// Hard host ceiling for rate-limit retries after the initial request.
/// 初始请求后的限流重试所使用的宿主硬上限。
pub const PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES: u32 = 100_000;

/// Hard host ceiling for one rate-limit retry wait, shared by the settings page and the runtime.
/// 单次限流重试等待的宿主硬上限，由设置页与运行时共享。
pub const PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS: u64 = 1_800_000;

/// Built-in error keywords that mark a dispatched failure as a replayable transient interruption.
/// 将已分派失败标记为可重放瞬态中断的内置错误关键词。
///
/// The entries are the exact `reqwest` display texts for an interrupted response body and a
/// failed body decode, which carry no request-level rejection.
/// 这些条目是 `reqwest` 对响应体中断与响应体解码失败的精确展示文本，不代表请求级拒绝。
pub const PROVIDER_RETRY_DEFAULT_KEYWORDS: &[&str] = &[
    "request or response body error",
    "error decoding response body",
];

/// Maximum number of configured retry keywords.
/// 已配置重试关键词的最大数量。
pub const PROVIDER_RETRY_KEYWORD_MAX_COUNT: usize = 64;

/// Maximum character length of one configured retry keyword.
/// 单个已配置重试关键词的最大字符长度。
pub const PROVIDER_RETRY_KEYWORD_MAX_CHARACTERS: usize = 200;

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
    /// Case-insensitive error keywords whose match makes a dispatched failure replayable.
    /// 不区分大小写的错误关键词；命中时已分派失败可重放。
    ///
    /// Omitted while equal to the built-in list so later default updates still apply.
    /// 与内置列表相同时省略，使后续默认值更新仍能生效。
    #[serde(
        rename = "retryKeywords",
        default = "default_provider_retry_keywords",
        skip_serializing_if = "provider_retry_keywords_are_default"
    )]
    pub retry_keywords: Vec<String>,
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
            retry_keywords: default_provider_retry_keywords(),
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
    /// Maximum finite retries within `1..=PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES`.
    /// 位于 `1..=PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES` 的最大有限重试次数。
    #[serde(
        rename = "maxRetries",
        default = "default_provider_rate_limit_retry_max_retries"
    )]
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
    /// Builds the finite default rate-limit schedule.
    /// 构建默认的有限限流重试调度规则。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The complete default rate-limit schedule.
    /// 返回完整默认限流重试调度规则。
    fn default() -> Self {
        Self {
            max_retries: default_provider_rate_limit_retry_max_retries(),
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

/// Clamps one persisted rate-limit retry count to the finite host contract.
/// 将一个持久化限流重试次数钳制到有限宿主契约。
///
/// # Parameters
/// 参数
///
/// `configured_max_retries` is the persisted count.
/// `configured_max_retries` 是持久化次数。
///
/// # Returns
/// 返回值
///
/// A retry count within `1..=PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES`.
/// 返回位于 `1..=PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES` 的重试次数。
pub fn normalize_provider_rate_limit_retry_max_retries(configured_max_retries: u32) -> u32 {
    configured_max_retries.clamp(1, PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES)
}

/// Returns the default rate-limit retry count.
/// 返回默认限流重试次数。
fn default_provider_rate_limit_retry_max_retries() -> u32 {
    PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES
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

/// Returns the built-in retry keyword list.
/// 返回内置重试关键词列表。
pub fn default_provider_retry_keywords() -> Vec<String> {
    PROVIDER_RETRY_DEFAULT_KEYWORDS
        .iter()
        .map(|keyword| String::from(*keyword))
        .collect()
}

/// Returns whether a keyword list equals the built-in list.
/// 返回关键词列表是否等于内置列表。
fn provider_retry_keywords_are_default(keywords: &[String]) -> bool {
    keywords
        .iter()
        .map(String::as_str)
        .eq(PROVIDER_RETRY_DEFAULT_KEYWORDS.iter().copied())
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

#[cfg(test)]
mod tests {
    //! Focused tests for finite retry-count normalization.
    //! 有限重试次数规范化的定向测试。

    use super::*;

    /// Verifies defaults use the finite host ceiling and persisted counts stay within it.
    /// 验证默认值使用有限宿主上限，且持久化次数保持在该范围内。
    #[test]
    fn rate_limit_retry_counts_are_always_finite() {
        let default_schedule = PersistedProviderRateLimitRetryConfiguration::default();
        let omitted: PersistedProviderRateLimitRetryConfiguration =
            serde_json::from_str("{}").expect("an empty schedule must use defaults");

        assert_eq!(
            default_schedule.max_retries,
            PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES
        );
        assert_eq!(omitted, default_schedule);
        assert_eq!(normalize_provider_rate_limit_retry_max_retries(0), 1);
        assert_eq!(normalize_provider_rate_limit_retry_max_retries(12), 12);
        assert_eq!(
            normalize_provider_rate_limit_retry_max_retries(u32::MAX),
            PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES
        );
    }

    /// Verifies the default keyword list is omitted on disk while a customized list persists.
    /// 验证默认关键词列表不写入磁盘，而自定义列表会被持久化。
    #[test]
    fn retry_keywords_persist_only_when_customized() {
        let default_json = serde_json::to_value(PersistedProviderRetryConfiguration::default())
            .expect("default retry configuration must serialize");
        assert!(default_json.get("retryKeywords").is_none());
        let omitted: PersistedProviderRetryConfiguration =
            serde_json::from_str("{}").expect("an empty block must use defaults");
        assert_eq!(omitted.retry_keywords, default_provider_retry_keywords());

        let customized = PersistedProviderRetryConfiguration {
            retry_keywords: vec![String::from("connection reset")],
            ..PersistedProviderRetryConfiguration::default()
        };
        let customized_json =
            serde_json::to_value(&customized).expect("customized configuration must serialize");
        assert_eq!(
            customized_json.get("retryKeywords"),
            Some(&serde_json::json!(["connection reset"]))
        );
        let empty = PersistedProviderRetryConfiguration {
            retry_keywords: Vec::new(),
            ..PersistedProviderRetryConfiguration::default()
        };
        let round_trip: PersistedProviderRetryConfiguration = serde_json::from_value(
            serde_json::to_value(&empty).expect("empty keyword list must serialize"),
        )
        .expect("empty keyword list must deserialize");
        assert!(round_trip.retry_keywords.is_empty());
    }
}

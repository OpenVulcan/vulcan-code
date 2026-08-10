//! Provider retry settings exposed to browser-facing clients.
//! 向浏览器客户端公开的 Provider 重试设置。

use serde::{Deserialize, Serialize};

/// Provider retry settings projected from the shared backend configuration.
/// 从共享后台配置投影出的 Provider 重试设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRetrySettingsSummary {
    /// Whether retryable Provider failures may be retried automatically.
    /// 是否允许自动重试可重试的 Provider 失败。
    pub enabled: bool,
    /// Maximum ordinary transient retry count.
    /// 普通瞬态错误的最大重试次数。
    pub max_retries: u32,
    /// Initial ordinary retry delay in milliseconds.
    /// 普通重试的初始等待毫秒数。
    pub initial_delay_ms: u64,
    /// Maximum ordinary retry delay in milliseconds.
    /// 普通重试的最大等待毫秒数。
    pub max_delay_ms: u64,
    /// Ordinary retry backoff multiplier represented in thousandths.
    /// 以千分数表示的普通重试退避倍数。
    pub backoff_factor_milli: u32,
    /// Deterministic retry jitter percentage.
    /// 确定性重试抖动百分比。
    pub jitter_percent: u8,
    /// Whether Provider Retry-After hints may increase the wait.
    /// 是否允许 Provider Retry-After 提示增加等待时间。
    pub respect_retry_after: bool,
    /// Whether HTTP 429 and structured rate-limit errors may be retried.
    /// 是否允许重试 HTTP 429 与结构化限流错误。
    pub retry_429: bool,
    /// Dedicated rate-limit retry schedule.
    /// 专用限流重试调度规则。
    pub rate_limit: ProviderRateLimitRetrySettingsSummary,
}

impl Default for ProviderRetrySettingsSummary {
    /// Builds the browser-facing default that mirrors backend configuration defaults.
    /// 构建与后台配置默认值一致的浏览器侧默认设置。
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 5,
            initial_delay_ms: 2_000,
            max_delay_ms: 30_000,
            backoff_factor_milli: 2_000,
            jitter_percent: 20,
            respect_retry_after: true,
            retry_429: true,
            rate_limit: ProviderRateLimitRetrySettingsSummary::default(),
        }
    }
}

/// Dedicated rate-limit retry schedule projected to the settings page.
/// 投影到设置页面的专用限流重试调度规则。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRateLimitRetrySettingsSummary {
    /// Maximum finite retry count after host normalization.
    /// 宿主规范化后的最大有限重试次数。
    pub max_retries: u32,
    /// Delay before the first rate-limit retry in milliseconds.
    /// 第一次限流重试前的等待毫秒数。
    pub initial_delay_ms: u64,
    /// Configured rate-limit retry delay ceiling in milliseconds.
    /// 配置的限流重试等待毫秒上限。
    pub max_delay_ms: u64,
    /// Rate-limit backoff multiplier represented in thousandths.
    /// 以千分数表示的限流退避倍数。
    pub backoff_factor_milli: u32,
}

impl Default for ProviderRateLimitRetrySettingsSummary {
    /// Builds the default finite rate-limit schedule.
    /// 构建默认的有限限流调度规则。
    fn default() -> Self {
        Self {
            max_retries: 100_000,
            initial_delay_ms: 5_000,
            max_delay_ms: 1_800_000,
            backoff_factor_milli: 2_000,
        }
    }
}

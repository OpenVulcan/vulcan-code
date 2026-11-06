//! Provider retry policy and retryability classification for host-controlled transports.
//! 宿主控制 Provider 传输使用的重试策略与可重试性分类。

use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_vulcan::NO_ANSWER_COMPLETION_SUBCATEGORY;
use crate::provider_config::{
    PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS, PROVIDER_RETRY_MAX_RETRIES,
    PersistedProviderRetryConfiguration, normalize_provider_rate_limit_retry_max_retries,
};
use std::time::Duration;
use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, AiTransportKind, DispatchOutcome};
use vulcan_core::types::CoreError as VulcanCoreError;

/// Hard safety ceiling for every backend-configured rate-limit retry wait.
/// 每次后台配置限流重试等待使用的硬安全上限。
const MAX_RATE_LIMIT_RETRY_DELAY: Duration =
    Duration::from_millis(PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS);
/// Legacy terminal subtype for a Provider response whose streamed tool arguments never became valid JSON.
/// Provider 响应的流式工具参数始终未形成有效 JSON 时使用的旧版终态子类型。
const TERMINAL_TOOL_CALL_STREAM_FAILURE_SUBCATEGORY: &str = "tool_call_stream_failure";

/// Runtime provider retry policy derived from persisted user configuration.
/// 从用户持久化配置派生出的运行时 Provider 重试策略。
#[derive(Debug, Clone)]
pub struct ProviderRetryPolicy {
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
    /// Whether a statusless typed rate-limit failure may be retried; observed HTTP 4xx is terminal.
    /// 是否允许重试不带状态码的强类型限流失败；已观察到的 HTTP 4xx 保持终态。
    retry_429: bool,
    /// Maximum finite rate-limit retry count after legacy-value normalization.
    /// 旧值规范化后的最大有限限流重试次数。
    rate_limit_max_retries: u32,
    /// Initial delay before the first rate-limit retry.
    /// 第一次限流重试前的初始等待时间。
    rate_limit_initial_delay: Duration,
    /// Backend-configured rate-limit delay ceiling after applying the hard safety cap.
    /// 应用硬安全上限后的后台限流等待上限。
    rate_limit_max_delay: Duration,
    /// Rate-limit backoff multiplier, where one selects a fixed interval.
    /// 限流退避倍数；一表示固定间隔。
    rate_limit_backoff_factor: f64,
    /// Lowercase configured error keywords whose match makes a dispatched failure replayable.
    /// 已配置的小写错误关键词；命中时已分派失败可重放。
    retry_keywords: Vec<String>,
}

impl ProviderRetryPolicy {
    /// Builds a runtime retry policy from persisted configuration.
    /// 根据持久化配置构建运行时重试策略。
    pub fn from_config(config: &PersistedProviderRetryConfiguration) -> Self {
        // rateLimitInitialMs is bounded before deriving the matching maximum delay.
        // rateLimitInitialMs 在派生对应最大等待前先受硬上限约束。
        let rate_limit_initial_ms = config
            .rate_limit
            .initial_delay_ms
            .max(1)
            .min(MAX_RATE_LIMIT_RETRY_DELAY.as_millis() as u64);
        Self {
            enabled: config.enabled,
            max_retries: config.max_retries.min(PROVIDER_RETRY_MAX_RETRIES),
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
            rate_limit_max_retries: normalize_provider_rate_limit_retry_max_retries(
                config.rate_limit.max_retries,
            ),
            rate_limit_initial_delay: Duration::from_millis(rate_limit_initial_ms),
            rate_limit_max_delay: Duration::from_millis(
                config
                    .rate_limit
                    .max_delay_ms
                    .max(rate_limit_initial_ms)
                    .min(MAX_RATE_LIMIT_RETRY_DELAY.as_millis() as u64),
            ),
            rate_limit_backoff_factor: if config.rate_limit.backoff_factor.is_finite() {
                config.rate_limit.backoff_factor.clamp(1.0, 8.0)
            } else {
                2.0
            },
            retry_keywords: config
                .retry_keywords
                .iter()
                .map(|keyword| keyword.trim().to_lowercase())
                .filter(|keyword| !keyword.is_empty())
                .collect(),
        }
    }

    /// Classifies one failed attempt and returns the next retry decision when retry is still allowed.
    /// 对一次失败尝试进行分类，并在仍允许重试时返回下一次重试决策。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the typed transport failure, `completed_retries` counts already executed
    /// retries, and `unsafe_delta_seen` reports whether replay could duplicate semantic output.
    /// `error` 是强类型传输失败，`completed_retries` 统计已执行重试，`unsafe_delta_seen`
    /// 报告重放是否可能复制语义输出。
    ///
    /// # Returns
    /// 返回值
    /// A bounded same-route retry decision only when replay remains safe; otherwise `None`.
    /// 仅在重放仍安全时返回有界同路由重试决策，否则返回 `None`。
    pub(crate) fn next_retry(
        &self,
        error: &VulcanCoreError,
        completed_retries: u32,
        unsafe_delta_seen: bool,
    ) -> Option<ProviderRetryDecision> {
        if !self.enabled
            || (unsafe_delta_seen
                && !core_error_is_replayable_stream_failure(error, &self.retry_keywords))
        {
            return None;
        }

        // classification separates rate-limit scheduling from ordinary transient retry rules.
        // classification 将限流调度与普通瞬态重试规则分离。
        let classification = classify_retry_reason(error, self.retry_429, &self.retry_keywords)?;
        // retryLimit selects the independent backend ceiling for rate limits or ordinary transient failures.
        // retryLimit 为限流或普通瞬态失败选择彼此独立的后台次数上限。
        // isRateLimit selects the dedicated backend interval and count configuration.
        // isRateLimit 选择专用的后台间隔与次数配置。
        let is_rate_limit = classification.category == ProviderRetryCategory::RateLimit;
        let retry_limit = if is_rate_limit {
            self.rate_limit_max_retries
        } else {
            self.max_retries
        };
        if completed_retries >= retry_limit {
            return None;
        }
        // retryNumber saturates only after the representable telemetry range is exhausted.
        // retryNumber 仅在可表示的遥测序号范围耗尽后进入饱和。
        let retry_number = completed_retries.saturating_add(1);
        let retry_after = self
            .respect_retry_after
            .then(|| retry_after_from_error(error))
            .flatten();
        let delay = if is_rate_limit {
            self.rate_limit_delay(retry_number, retry_after)
        } else {
            retry_after
                .map(|delay| delay.min(self.max_delay))
                .unwrap_or_else(|| self.local_delay(retry_number))
        };
        Some(ProviderRetryDecision {
            retry_number,
            max_retries: retry_limit,
            unbounded: false,
            delay,
            reason: classification.reason,
        })
    }

    /// Classifies one typed public provider failure and returns the next safe retry decision.
    /// 对一个强类型公共供应商失败进行分类，并返回下一项安全重试决策。
    ///
    /// # Parameters
    /// 参数
    /// `error` preserves authoritative dispatch and semantic-output facts while
    /// `completed_retries` counts already executed retries after the initial request.
    /// `error` 保留权威分派与语义输出事实，`completed_retries` 统计初始请求后已执行的重试。
    ///
    /// # Returns
    /// 返回值
    /// A count-bounded decision only when transparent replay is proven safe.
    /// 仅在透明重放已被证明安全时返回有次数上限的决策。
    pub fn next_structured_retry(
        &self,
        error: &AiExecutionError,
        completed_retries: u32,
    ) -> Option<ProviderRetryDecision> {
        if !self.enabled {
            return None;
        }
        let classification =
            classify_structured_retry_reason(error, self.retry_429, &self.retry_keywords)?;
        let is_rate_limit = classification.category == ProviderRetryCategory::RateLimit;
        let retry_limit = if is_rate_limit {
            self.rate_limit_max_retries
        } else {
            self.max_retries
        };
        if completed_retries >= retry_limit {
            return None;
        }
        let retry_number = completed_retries.saturating_add(1);
        let retry_after = self
            .respect_retry_after
            .then(|| {
                error
                    .retry_after
                    .as_ref()
                    .map(|hint| Duration::from_millis(hint.delay_milliseconds))
            })
            .flatten();
        let delay = if is_rate_limit {
            self.rate_limit_delay(retry_number, retry_after)
        } else {
            retry_after
                .map(|delay| delay.min(self.max_delay))
                .unwrap_or_else(|| self.local_delay(retry_number))
        };
        Some(ProviderRetryDecision {
            retry_number,
            max_retries: retry_limit,
            unbounded: false,
            delay,
            reason: classification.reason,
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

    /// Calculates one backend-configured fixed or exponential rate-limit retry delay.
    /// 计算一次由后台配置的固定或指数限流重试等待时间。
    ///
    /// `retry_number` is the 1-based retry index.
    /// `retry_number` 是从一开始的重试序号。
    /// `retry_after` is the optional Provider-requested minimum wait.
    /// `retry_after` 是 Provider 可选要求的最短等待时间。
    /// Returns a delay that never exceeds `MAX_RATE_LIMIT_RETRY_DELAY`.
    /// 返回绝不超过 `MAX_RATE_LIMIT_RETRY_DELAY` 的等待时间。
    fn rate_limit_delay(&self, retry_number: u32, retry_after: Option<Duration>) -> Duration {
        // exponent is bounded before floating-point exponentiation because the final delay is capped independently.
        // exponent 在浮点指数计算前受限，因为最终等待时间会独立封顶。
        let exponent = retry_number.saturating_sub(1).min(63) as i32;
        let delayed_ms = self.rate_limit_initial_delay.as_millis() as f64
            * self.rate_limit_backoff_factor.powi(exponent);
        // localDelay is capped before conversion so oversized floating-point values cannot overflow milliseconds.
        // localDelay 在转换前封顶，避免过大的浮点值溢出毫秒范围。
        let local_delay = Duration::from_millis(
            delayed_ms
                .min(self.rate_limit_max_delay.as_millis() as f64)
                .round() as u64,
        );
        local_delay
            .max(retry_after.unwrap_or_default())
            .min(self.rate_limit_max_delay)
    }
}

/// One concrete retry decision emitted after a failed provider attempt.
/// 一次 Provider 尝试失败后产出的具体重试决策。
#[derive(Debug, Clone)]
pub struct ProviderRetryDecision {
    /// 1-based retry number that will be executed next.
    /// 即将执行的从 1 开始的重试序号。
    pub retry_number: u32,
    /// Effective finite retry limit after host normalization.
    /// 宿主规范化后的有效有限重试上限。
    pub max_retries: u32,
    /// Legacy transcript compatibility flag; new runtime decisions always set it to false.
    /// 旧转录兼容标志；新的运行时决策始终将其设为 false。
    pub unbounded: bool,
    /// Delay before the next retry.
    /// 下一次重试前的等待时间。
    pub delay: Duration,
    /// Human-readable retry reason.
    /// 人类可读的重试原因。
    pub reason: String,
}

/// Stable retry category derived from one concrete Provider failure.
/// 从一个具体 Provider 失败派生的稳定重试类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderRetryCategory {
    /// Rate-limit failure governed by the dedicated backend schedule.
    /// 受专用后台调度规则约束的限流失败。
    RateLimit,
    /// Ordinary transient failure governed by the configured retry count.
    /// 受已配置重试次数约束的普通瞬态失败。
    Transient,
}

/// Classified retry reason and its scheduling category.
/// 已分类的重试原因及其调度类别。
struct ProviderRetryClassification {
    /// Scheduling category that controls retry count and delay policy.
    /// 控制重试次数与等待策略的调度类别。
    category: ProviderRetryCategory,
    /// Human-readable reason shown in runtime status records.
    /// 显示在运行状态记录中的人类可读原因。
    reason: String,
}

/// Returns whether one streamed delta makes automatic transparent retry unsafe.
/// 判断某个已流式发出的增量是否会让透明自动重试变得不安全。
///
/// A response identity from `response.created` is metadata only: the next physical request
/// clears it, so a transport close right after it remains replayable.
/// 来自 `response.created` 的响应身份只是元数据：下一次真实请求会清除它，因此紧随其后的
/// 传输关闭仍可重放。
pub(crate) fn provider_delta_makes_retry_unsafe(delta: &ProviderStreamDelta) -> bool {
    matches!(
        delta,
        ProviderStreamDelta::Thinking(_)
            | ProviderStreamDelta::SummaryThinking(_)
            | ProviderStreamDelta::Output(_)
            | ProviderStreamDelta::ToolRequestFragment(_)
            | ProviderStreamDelta::RuntimeEvent(_)
            | ProviderStreamDelta::UsageUpdated(_)
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
fn classify_retry_reason(
    error: &VulcanCoreError,
    retry_429: bool,
    retry_keywords: &[String],
) -> Option<ProviderRetryClassification> {
    if let VulcanCoreError::ProviderFailure {
        details: Some(details),
        ..
    } = error
    {
        return classify_structured_retry_reason(details, retry_429, retry_keywords);
    }
    classify_legacy_retry_reason(error, retry_429)
}

/// Classifies one typed public-contract failure without inspecting display text.
/// 在不检查展示文本的前提下分类一个强类型公共契约失败。
///
/// # Parameters
/// 参数
/// `error` is the normalized execution failure, `retry_429` controls rate-limit retry, and
/// `retry_keywords` are the lowercase configured keywords.
/// `error` 是归一化执行失败，`retry_429` 控制是否重试限流，`retry_keywords` 是已配置的小写关键词。
///
/// # Returns
/// 返回值
/// A retry classification only when the typed safety and retryability flags permit replay.
/// 仅当强类型安全标志与可重试标志允许重放时返回重试分类。
fn classify_structured_retry_reason(
    error: &AiExecutionError,
    retry_429: bool,
    retry_keywords: &[String],
) -> Option<ProviderRetryClassification> {
    // A provider-owned reconciliation state forbids transparent replay even if a future generic
    // classifier learns to recognize its HTTP status or surrounding error type.
    // 供应商自有待对账状态禁止透明重放，即使未来通用分类器识别了其 HTTP 状态或外围错误类型。
    if error.confirmation_state().is_some() {
        return None;
    }
    // An observed 4xx response is an authoritative request/application rejection. It remains
    // terminal even when an adapter supplied an overly broad retryable category or overload
    // marker (original plan §13.5).
    // 已观察到的 4xx 响应是权威的请求/应用拒绝。即使适配器错误提供了宽泛可重试类别或过载
    // 标记，它仍保持终态（原始方案 §13.5）。
    if error
        .http_status
        .is_some_and(|status| (400..=499).contains(&status))
    {
        return None;
    }
    if structured_error_is_deterministic_terminal(error) {
        return None;
    }
    // The official Codex overload code and message are authoritative when an older adapter
    // accidentally leaves the generic retryable bit unset.
    // 旧适配器误将通用 retryable 标志置空时，官方 Codex 过载代码和消息仍是权威依据。
    let known_overload = structured_error_is_known_overload(error);
    // The exact Responses stream error code is authoritative for transient stream recovery.
    // 精确的 Responses 流错误代码是瞬态流恢复的权威依据。
    let known_stream_error = structured_error_is_known_stream_error(error);
    // replayableStreamIdleTimeout keeps the existing timeout-specific retry classification.
    // replayableStreamIdleTimeout 保留现有的流空闲超时专用重试分类。
    let replayable_stream_idle_timeout = structured_error_is_replayable_stream_idle_timeout(error);
    // replayableStreamFailure contains only the registered stream subtypes allowed to replace discarded partial output.
    // replayableStreamFailure 仅包含允许替换已丢弃局部输出的已注册流子类型。
    let replayable_stream_failure =
        structured_error_is_replayable_stream_failure(error, retry_keywords);
    // keywordMatch is the user-configured transient marker; it still respects every safety gate.
    // keywordMatch 是用户配置的瞬态标记；它仍遵守全部安全守卫。
    let keyword_match = structured_error_matches_retry_keyword(error, retry_keywords);
    if !error.retryable && !known_overload && !known_stream_error && !keyword_match
        || (error.semantic_output_observed && !replayable_stream_failure)
        || error.side_effect_observed
        || error.dispatch_outcome == DispatchOutcome::Unknown
    {
        return None;
    }
    if known_overload {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider overloaded"),
        });
    }
    if known_stream_error {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider stream error"),
        });
    }
    if structured_error_is_no_answer_completion(error) {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider completed without an answer"),
        });
    }
    if keyword_match {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider error matched a retry keyword"),
        });
    }
    match error.category {
        AiErrorCategory::RateLimited if retry_429 => Some(ProviderRetryClassification {
            category: ProviderRetryCategory::RateLimit,
            reason: String::from("Provider rate limit"),
        }),
        AiErrorCategory::ProviderOverloaded => Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider overloaded"),
        }),
        AiErrorCategory::ProviderUnavailable => Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider unavailable"),
        }),
        AiErrorCategory::Timeout if replayable_stream_idle_timeout => {
            Some(ProviderRetryClassification {
                category: ProviderRetryCategory::Transient,
                reason: String::from("Provider stream idle timeout"),
            })
        }
        AiErrorCategory::Timeout => Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider timeout"),
        }),
        AiErrorCategory::Unclassified => {
            classify_structured_http_status(error.http_status, retry_429)
        }
        _ => None,
    }
}

/// Reports whether a Core provider failure carries an explicitly replayable stream failure.
/// 报告 Core Provider 失败是否携带明确可重放的流失败。
///
/// # Parameters
/// 参数
/// `error` is the controller failure retaining optional structured Provider details and
/// `retry_keywords` are the lowercase configured keywords.
/// `error` 是保留可选强类型 Provider 详情的控制器失败，`retry_keywords` 是已配置的小写关键词。
///
/// # Returns
/// 返回值
/// `true` only for a registered, dispatched, side-effect-free stream failure subtype.
/// 仅当强类型失败属于已注册、已分派且无副作用的流失败子类型时返回 `true`。
fn core_error_is_replayable_stream_failure(
    error: &VulcanCoreError,
    retry_keywords: &[String],
) -> bool {
    matches!(
        error,
        VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } if structured_error_is_replayable_stream_failure(details, retry_keywords)
    )
}

/// Reports whether typed facts authorize replacing one stalled stream from the retained checkpoint.
/// 报告强类型事实是否允许从保留检查点替换一次停滞流。
///
/// # Parameters
/// 参数
/// `error` is the normalized public Provider failure.
/// `error` 是归一化公共 Provider 失败。
///
/// # Returns
/// 返回值
/// `true` only for the registered response-stream idle timeout with no observed side effect.
/// 仅对已注册且未观察到副作用的响应流空闲超时返回 `true`。
fn structured_error_is_replayable_stream_idle_timeout(error: &AiExecutionError) -> bool {
    error.category == AiErrorCategory::Timeout
        && error.subcategory.as_deref() == Some("stream_idle_timeout")
        && error.stage == "response_stream"
        && error.dispatch_outcome == DispatchOutcome::Dispatched
        && error.retryable
        && !error.side_effect_observed
}

/// Returns whether the exact Responses stream error permits replay from a clean checkpoint.
/// 判断精确的 Responses 流错误是否允许从干净检查点重放。
fn structured_error_is_known_stream_error(error: &AiExecutionError) -> bool {
    error.upstream_code.as_deref() == Some("STREAM_ERROR")
        && error.stage == "stream"
        && error.dispatch_outcome == DispatchOutcome::Dispatched
        && error.terminal
        && !error.side_effect_observed
}

/// Returns whether a Responses WebSocket lost mid-stream permits replay on a fresh socket.
/// 判断流中途丢失的 Responses WebSocket 是否允许在新 Socket 上重放。
/// The adapter counts each loss and pins the session to HTTP after its consecutive limit.
/// 适配层记录每次丢失，达到连续上限后将会话固定改走 HTTP。
fn structured_error_is_replayable_websocket_loss(error: &AiExecutionError) -> bool {
    error.transport == Some(AiTransportKind::ResponsesWebsocket)
        && error.category == AiErrorCategory::ProviderUnavailable
        && error.subcategory.as_deref() == Some("transport")
        && error.stage == "response_stream"
        && error.dispatch_outcome == DispatchOutcome::Dispatched
        && error.retryable
        && !error.side_effect_observed
}

/// Returns whether a successful terminal without an answer permits replacing its visible reasoning.
/// 判断没有回答的成功终态是否允许替换其已显示的思考并重放。
///
/// # Parameters
/// 参数
/// `error` is the normalized public Provider failure.
/// `error` 是归一化公共 Provider 失败。
///
/// # Returns
/// 返回值
/// `true` only for the registered dispatched, side-effect-free no-answer completion.
/// 仅对已注册、已分派且无副作用的无回答完成返回 `true`。
fn structured_error_is_no_answer_completion(error: &AiExecutionError) -> bool {
    error.subcategory.as_deref() == Some(NO_ANSWER_COMPLETION_SUBCATEGORY)
        && error.stage == "response_projection"
        && error.dispatch_outcome == DispatchOutcome::Dispatched
        && error.retryable
        && !error.side_effect_observed
}

/// Returns whether one structured failure is an explicitly replayable stream failure.
/// 判断一项结构化失败是否属于明确可重放的流失败。
fn structured_error_is_replayable_stream_failure(
    error: &AiExecutionError,
    retry_keywords: &[String],
) -> bool {
    structured_error_is_replayable_stream_idle_timeout(error)
        || structured_error_is_known_stream_error(error)
        || structured_error_is_replayable_websocket_loss(error)
        || structured_error_is_no_answer_completion(error)
        || (error.dispatch_outcome == DispatchOutcome::Dispatched
            && !error.side_effect_observed
            && structured_error_matches_retry_keyword(error, retry_keywords))
}

/// Reports whether any diagnostic text of one failure contains a configured retry keyword.
/// 报告一项失败的任一诊断文本是否包含已配置的重试关键词。
///
/// # Parameters
/// 参数
/// `error` is the normalized failure and `retry_keywords` are lowercase, non-empty keywords.
/// `error` 是归一化失败，`retry_keywords` 是小写且非空的关键词。
///
/// # Returns
/// 返回值
/// `true` when the message, upstream message, or upstream code contains one keyword.
/// 当消息、上游消息或上游代码包含任一关键词时返回 `true`。
fn structured_error_matches_retry_keyword(
    error: &AiExecutionError,
    retry_keywords: &[String],
) -> bool {
    if retry_keywords.is_empty() {
        return false;
    }
    [
        Some(error.message.as_str()),
        error.upstream_message.as_deref(),
        error.upstream_code.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::to_lowercase)
    .any(|text| {
        retry_keywords
            .iter()
            .any(|keyword| text.contains(keyword.as_str()))
    })
}

/// Reports whether typed category/subcategory facts prove a deterministic terminal failure.
/// 报告强类型类别/子类别事实是否证明一项确定性终态失败。
///
/// # Parameters
/// 参数
/// `error` is the normalized provider failure; display text is intentionally ignored.
/// `error` 是归一化供应商失败；展示文本被有意忽略。
///
/// # Returns
/// 返回值
/// `true` for request, policy, schema, unsupported-feature, or decoder/protocol failures.
/// 对请求、策略、Schema、不支持能力或解码/协议失败返回 `true`。
fn structured_error_is_deterministic_terminal(error: &AiExecutionError) -> bool {
    if matches!(
        error.category,
        AiErrorCategory::Authentication
            | AiErrorCategory::PermissionDenied
            | AiErrorCategory::PaymentRequired
            | AiErrorCategory::InvalidRequest
            | AiErrorCategory::ContextLengthExceeded
            | AiErrorCategory::ContentPolicy
            | AiErrorCategory::Cancelled
    ) {
        return true;
    }
    error.subcategory.as_deref().is_some_and(|subcategory| {
        matches!(
            subcategory,
            "route_validation"
                | "protocol_conversion"
                | "response_content_type"
                | "schema"
                | "unsupported_feature"
                | "decoder"
                | TERMINAL_TOOL_CALL_STREAM_FAILURE_SUBCATEGORY
        )
    })
}

/// Recognizes only the registered Codex overload markers that are safe to replay before semantic output.
/// 仅识别已注册且在语义输出前可安全重放的 Codex 过载标记。
fn structured_error_is_known_overload(error: &AiExecutionError) -> bool {
    let code_matches = error.upstream_code.as_deref().is_some_and(|code| {
        code.eq_ignore_ascii_case("server_is_overloaded") || code.eq_ignore_ascii_case("slow_down")
    });
    let message_matches = error.upstream_message.as_deref().is_some_and(|message| {
        message
            .to_ascii_lowercase()
            .contains("our servers are currently overloaded. please try again later")
    });
    code_matches || message_matches
}

/// Classifies an exact HTTP status retained by a typed but otherwise unclassified error.
/// 分类由强类型但尚未归类的错误保留的精确 HTTP 状态。
///
/// # Parameters
/// 参数
/// `status` is the upstream status and `retry_429` controls rate-limit retry.
/// `status` 是上游状态码，`retry_429` 控制是否重试限流。
///
/// # Returns
/// 返回值
/// A rate-limit or transient classification for the registered retryable statuses.
/// 对已注册的可重试状态返回限流或瞬态分类。
fn classify_structured_http_status(
    status: Option<u16>,
    _retry_429: bool,
) -> Option<ProviderRetryClassification> {
    match status {
        Some(status @ (500 | 502 | 503 | 504 | 529)) => Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: format!("Provider HTTP {status}"),
        }),
        _ => None,
    }
}

/// Classifies a pre-public-contract provider error retained for legacy controller adapters.
/// 分类为公共契约迁移前控制器适配器保留的旧 Provider 错误。
///
/// # Parameters
/// 参数
/// `error` is a Core failure without structured details and `retry_429` controls rate limits.
/// `error` 是不带结构化细节的 Core 失败，`retry_429` 控制限流重试。
///
/// # Returns
/// 返回值
/// A conservative compatibility classification, or `None`.
/// 返回保守的兼容分类，否则返回 `None`。
fn classify_legacy_retry_reason(
    error: &VulcanCoreError,
    retry_429: bool,
) -> Option<ProviderRetryClassification> {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();

    if matches!(
        error,
        VulcanCoreError::InvalidState { .. }
            | VulcanCoreError::ContextCompactionRequired { .. }
            | VulcanCoreError::ContextCompactionPolicyRefreshRequired { .. }
            | VulcanCoreError::ProviderRouteSwitchRequired { .. }
            | VulcanCoreError::ToolFailure { .. }
            | VulcanCoreError::ToolDurabilityFailure { .. }
            | VulcanCoreError::ToolProtocolFailure { .. }
    ) {
        return None;
    }
    if contains_non_retryable_marker(&lower) {
        return None;
    }
    if (400..=499).any(|status| contains_status(&lower, status)) {
        return None;
    }
    if retry_429 && (contains_status(&lower, 429) || contains_provider_rate_limit_marker(&lower)) {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::RateLimit,
            reason: String::from("Provider rate limit"),
        });
    }
    for status in [408, 425, 500, 502, 503, 504, 529] {
        if contains_status(&lower, status) {
            return Some(ProviderRetryClassification {
                category: ProviderRetryCategory::Transient,
                reason: format!("Provider HTTP {status}"),
            });
        }
    }
    if contains_transport_marker(&lower) {
        return Some(ProviderRetryClassification {
            category: ProviderRetryCategory::Transient,
            reason: String::from("Provider transport failure"),
        });
    }
    None
}

/// Returns whether a structured Provider error identifies a rate-limit failure without exposing its HTTP status.
/// 判断结构化 Provider 错误是否在未暴露 HTTP 状态码时仍明确标识限流失败。
fn contains_provider_rate_limit_marker(lower: &str) -> bool {
    lower.contains("rate_limit_error") || lower.contains("rate limit error")
}

/// Returns whether the error text contains a known non-retryable marker.
/// 判断错误文本是否包含已知不可重试标记。
fn contains_non_retryable_marker(lower: &str) -> bool {
    [
        "request budget gate blocked sending",
        "context window",
        "context overflow",
        "context length",
        "schema",
        "decoder",
        "decode error",
        "failed to decode",
        "unsupported",
        "not supported",
        "invalid request",
        "bad request",
        "unauthorized",
        "forbidden",
        "api key",
        "authentication",
        "quota exceeded",
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
        "failed to send request",
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
    if let VulcanCoreError::ProviderFailure {
        details: Some(details),
        ..
    } = error
    {
        return details
            .retry_after
            .as_ref()
            .map(|hint| Duration::from_millis(hint.delay_milliseconds));
    }
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

    use super::{
        NO_ANSWER_COMPLETION_SUBCATEGORY, ProviderRetryPolicy,
        TERMINAL_TOOL_CALL_STREAM_FAILURE_SUBCATEGORY, classify_retry_reason,
        provider_delta_makes_retry_unsafe,
    };
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
    use crate::provider_config::{
        PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES, PersistedProviderRateLimitRetryConfiguration,
        PersistedProviderRetryConfiguration,
    };
    use std::time::Duration;
    use vulcan_ai_contract::{
        AiErrorCategory, AiExecutionError, AiTransportKind, DispatchOutcome, RetryAfterHint,
        RetryAfterSource,
    };
    use vulcan_core::types::{
        CoreError as VulcanCoreError, ProviderUsageSnapshot as CoreProviderUsageSnapshot,
    };

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
            retry_keywords: vec![String::from("Error Decoding Response Body")],
            rate_limit: PersistedProviderRateLimitRetryConfiguration {
                max_retries: PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES,
                initial_delay_ms: 100,
                max_delay_ms: 1_800_000,
                backoff_factor: 2.0,
            },
        })
    }

    /// Builds one provider failure with stable provider metadata.
    /// 构建一条带稳定 Provider 元数据的 Provider 失败。
    fn provider_error(message: &str) -> VulcanCoreError {
        VulcanCoreError::ProviderFailure {
            provider_id: String::from("test"),
            message: message.to_string(),
            details: None,
        }
    }

    /// Builds one typed public-contract provider failure for retry assertions.
    /// 构建一条供重试断言使用的强类型公共契约 Provider 失败。
    ///
    /// # Parameters
    /// 参数
    /// `category` and `retryable` are the authoritative typed retry inputs.
    /// `category` 与 `retryable` 是权威的强类型重试输入。
    ///
    /// # Returns
    /// 返回值
    /// A Core provider failure carrying normalized execution details.
    /// 返回携带归一化执行细节的 Core Provider 失败。
    fn structured_provider_error(category: AiErrorCategory, retryable: bool) -> VulcanCoreError {
        VulcanCoreError::ProviderFailure {
            provider_id: String::from("test"),
            message: String::from("HTTP 400 misleading display text"),
            details: Some(Box::new(AiExecutionError {
                category,
                subcategory: None,
                stage: String::from("response_headers"),
                http_status: None,
                transport: None,
                websocket_close_code: None,
                upstream_code: None,
                upstream_type: None,
                upstream_parameter: None,
                upstream_message: None,
                request_id: None,
                provider_id: Some(String::from("test")),
                entry_id: Some(String::from("global")),
                model_id: Some(String::from("model")),
                message: String::from("safe display text"),
                dispatch_outcome: DispatchOutcome::Dispatched,
                semantic_output_observed: false,
                side_effect_observed: false,
                terminal: true,
                retryable,
                retry_after: None,
            })),
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
        assert!(!second.unbounded);
    }

    /// Verifies provider retry-after hints override local backoff.
    /// 验证 Provider 的 retry-after 提示会覆盖本地退避。
    #[test]
    fn honors_retry_after_hint_when_present() {
        let decision = policy()
            .next_retry(
                &provider_error("type=rate_limit_error | retry_after_ms=2500"),
                0,
                false,
            )
            .expect("a statusless rate limit with retry-after should be retryable");

        assert_eq!(decision.delay, Duration::from_millis(2_500));
    }

    /// Verifies ordinary transient Retry-After hints cannot bypass the configured maximum delay.
    /// 验证普通瞬态 Retry-After 提示不能绕过已配置的最大等待时间。
    #[test]
    fn transient_retry_after_is_capped_by_configured_maximum() {
        let error = provider_error("HTTP 503 Service Unavailable | retry_after_ms=2500");
        let decision = policy()
            .next_retry(&error, 0, false)
            .expect("503 with retry-after should retry");

        assert_eq!(decision.delay, Duration::from_millis(1_000));
    }

    /// Verifies typed retry data takes precedence over contradictory display text.
    /// 验证强类型重试数据优先于相互矛盾的展示文本。
    #[test]
    fn structured_errors_never_depend_on_display_text() {
        let decision = policy()
            .next_retry(
                &structured_provider_error(AiErrorCategory::ProviderUnavailable, true),
                0,
                false,
            )
            .expect("typed provider-unavailable error should retry");
        assert_eq!(decision.reason, "Provider unavailable");

        assert!(
            policy()
                .next_retry(
                    &structured_provider_error(AiErrorCategory::InvalidRequest, false),
                    0,
                    false,
                )
                .is_none()
        );
    }

    /// Verifies exact 4xx and deterministic protocol categories remain terminal despite bad flags.
    /// 验证精确 4xx 与确定性协议类别即使携带错误标志也保持终态。
    #[test]
    fn structured_4xx_and_protocol_failures_are_terminal() {
        let mut four_hundred =
            structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut four_hundred
        else {
            panic!("structured error expected");
        };
        details.http_status = Some(400);
        details.upstream_code = Some(String::from("server_is_overloaded"));
        assert!(policy().next_retry(&four_hundred, 0, false).is_none());

        let mut decoder = structured_provider_error(AiErrorCategory::InvalidRequest, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut decoder
        else {
            panic!("structured error expected");
        };
        details.subcategory = Some(String::from("protocol_conversion"));
        details.stage = String::from("response_stream");
        details.upstream_code = Some(String::from("slow_down"));
        assert!(policy().next_retry(&decoder, 0, false).is_none());
    }

    /// Verifies legacy schema, unsupported-feature, and decoder diagnostics never retry.
    /// 验证旧兼容 Schema、不支持能力与解码诊断绝不重试。
    #[test]
    fn legacy_deterministic_failures_are_terminal() {
        for message in [
            "HTTP 429 Too Many Requests",
            "Provider schema validation failed",
            "Provider feature is unsupported",
            "SSE 解码失败: invalid event payload",
        ] {
            assert!(
                policy()
                    .next_retry(&provider_error(message), 0, false)
                    .is_none(),
                "deterministic failure must be terminal: {message}"
            );
        }
    }

    /// Verifies typed retry-after milliseconds flow directly into scheduling.
    /// 验证强类型 Retry-After 毫秒值直接进入调度。
    #[test]
    fn structured_retry_after_preserves_subsecond_precision() {
        let mut error = structured_provider_error(AiErrorCategory::RateLimited, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut error
        else {
            panic!("structured error expected");
        };
        details.retry_after = Some(RetryAfterHint {
            delay_milliseconds: 2_501,
            source: RetryAfterSource::HeaderSeconds,
        });

        let decision = policy()
            .next_retry(&error, 0, false)
            .expect("typed rate-limit error should retry");
        assert_eq!(decision.delay, Duration::from_millis(2_501));
    }

    /// Verifies typed semantic output and side effects independently block transparent replay.
    /// 验证强类型语义输出与副作用分别阻止透明重放。
    #[test]
    fn structured_output_and_side_effect_flags_block_retry() {
        let mut semantic = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut semantic
        else {
            panic!("structured error expected");
        };
        details.semantic_output_observed = true;
        assert!(policy().next_retry(&semantic, 0, false).is_none());

        let mut side_effect = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut side_effect
        else {
            panic!("structured error expected");
        };
        details.side_effect_observed = true;
        assert!(policy().next_retry(&side_effect, 0, false).is_none());
    }

    /// Verifies only the exact side-effect-free stream-idle timeout may replace discarded partial output.
    /// 验证只有精确且无副作用的流空闲超时可以替换已丢弃的局部输出。
    #[test]
    fn stream_idle_timeout_retries_after_semantic_output_but_not_side_effects() {
        // idleTimeout mirrors the typed adapter failure after one stalled partial response.
        // idleTimeout 对应局部响应停滞后适配器生成的强类型失败。
        let mut idle_timeout = structured_provider_error(AiErrorCategory::Timeout, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut idle_timeout
        else {
            panic!("structured error expected");
        };
        details.subcategory = Some(String::from("stream_idle_timeout"));
        details.stage = String::from("response_stream");
        details.semantic_output_observed = true;

        // decision proves the exact subtype bypasses only the semantic-output replay guard.
        // decision 证明精确子类型只绕过语义输出重放守卫。
        let decision = policy()
            .next_retry(&idle_timeout, 0, true)
            .expect("an exact stream-idle timeout should replace partial output");
        assert_eq!(decision.reason, "Provider stream idle timeout");

        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut idle_timeout
        else {
            panic!("structured error expected");
        };
        details.side_effect_observed = true;
        assert!(policy().next_retry(&idle_timeout, 0, true).is_none());
    }

    /// Verifies the exact Responses stream error can replace semantic output but never side effects.
    /// 验证精确的 Responses 流错误可以替换语义输出，但绝不能替换副作用。
    #[test]
    fn responses_stream_error_retries_after_semantic_output_but_not_side_effects() {
        let mut stream_error = structured_provider_error(AiErrorCategory::Unclassified, false);
        {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut stream_error
            else {
                panic!("structured error expected");
            };
            details.stage = String::from("stream");
            details.upstream_code = Some(String::from("STREAM_ERROR"));
            details.semantic_output_observed = true;
        }

        let decision = policy()
            .next_retry(&stream_error, 0, true)
            .expect("the exact Responses stream error should retry");
        assert_eq!(decision.reason, "Provider stream error");

        {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut stream_error
            else {
                panic!("structured error expected");
            };
            details.side_effect_observed = true;
        }
        assert!(policy().next_retry(&stream_error, 0, true).is_none());
    }

    /// Verifies a no-answer completion replaces visible reasoning but never side effects.
    /// 验证无回答完成可以替换已显示的思考，但绝不能替换副作用。
    #[test]
    fn no_answer_completion_retries_after_reasoning_but_not_side_effects() {
        let mut no_answer = structured_provider_error(AiErrorCategory::Unclassified, true);
        {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut no_answer
            else {
                panic!("structured error expected");
            };
            details.subcategory = Some(String::from(NO_ANSWER_COMPLETION_SUBCATEGORY));
            details.stage = String::from("response_projection");
        }

        let decision = policy()
            .next_retry(&no_answer, 0, true)
            .expect("a no-answer completion should replace its reasoning");
        assert_eq!(decision.reason, "Provider completed without an answer");

        {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut no_answer
            else {
                panic!("structured error expected");
            };
            details.side_effect_observed = true;
        }
        assert!(policy().next_retry(&no_answer, 0, true).is_none());
    }

    /// Verifies a configured keyword replays an interrupted stream yet keeps every safety gate.
    /// 验证已配置关键词可重放中断的流，同时保留全部安全守卫。
    #[test]
    fn retry_keyword_replays_interrupted_stream_but_respects_safety_gates() {
        // keywordError mirrors a body-transport failure after usage made the attempt semantic.
        // keywordError 对应用量使尝试变为语义输出后的响应体传输失败。
        let keyword_error = || {
            let mut error = structured_provider_error(AiErrorCategory::ProviderUnavailable, false);
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut error
            else {
                panic!("structured error expected");
            };
            details.subcategory = Some(String::from("transport"));
            details.stage = String::from("response_stream");
            details.message = String::from(
                "Provider stream from `<redacted-url>` failed: error decoding response body",
            );
            details.semantic_output_observed = true;
            error
        };

        let decision = policy()
            .next_retry(&keyword_error(), 0, true)
            .expect("a keyword match should replay the interrupted stream");
        assert_eq!(decision.reason, "Provider error matched a retry keyword");
        assert!(
            policy().next_retry(&keyword_error(), 3, true).is_none(),
            "keyword retries must stay within the ordinary retry count"
        );

        let mut side_effect = keyword_error();
        let mut four_hundred = keyword_error();
        let mut unknown_dispatch = keyword_error();
        for (error, mutate) in [
            (
                &mut side_effect,
                (|details: &mut AiExecutionError| details.side_effect_observed = true)
                    as fn(&mut AiExecutionError),
            ),
            (&mut four_hundred, |details: &mut AiExecutionError| {
                details.http_status = Some(400)
            }),
            (&mut unknown_dispatch, |details: &mut AiExecutionError| {
                details.dispatch_outcome = DispatchOutcome::Unknown
            }),
        ] {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = error
            else {
                panic!("structured error expected");
            };
            mutate(details);
        }
        assert!(policy().next_retry(&side_effect, 0, true).is_none());
        assert!(policy().next_retry(&four_hundred, 0, true).is_none());
        assert!(policy().next_retry(&unknown_dispatch, 0, true).is_none());

        // emptyKeywords proves the same failure stays terminal once the user clears the table.
        // emptyKeywords 证明用户清空关键词表后同一失败保持终态。
        let empty_keywords =
            ProviderRetryPolicy::from_config(&PersistedProviderRetryConfiguration {
                retry_keywords: Vec::new(),
                ..PersistedProviderRetryConfiguration::default()
            });
        assert!(
            empty_keywords
                .next_retry(&keyword_error(), 0, true)
                .is_none()
        );
    }

    /// Verifies the legacy malformed-tool subtype remains terminal even when an adapter marks it retryable.
    /// 验证即使适配器将旧版畸形工具子类型标记为可重试，它仍保持终态。
    #[test]
    fn tool_call_stream_failure_never_retries_malformed_arguments() {
        // toolStreamFailure mirrors the legacy error that previously replayed identical malformed arguments.
        // toolStreamFailure 对应旧版错误；该错误此前会重复重放相同的畸形参数。
        let mut tool_stream_failure =
            structured_provider_error(AiErrorCategory::Unclassified, true);
        {
            let VulcanCoreError::ProviderFailure {
                details: Some(details),
                ..
            } = &mut tool_stream_failure
            else {
                panic!("structured error expected");
            };
            details.subcategory = Some(String::from(TERMINAL_TOOL_CALL_STREAM_FAILURE_SUBCATEGORY));
            details.stage = String::from("response_stream");
            details.semantic_output_observed = true;
        }

        // A malformed completed argument payload is deterministic model output, not a transient transport failure.
        // 已完成但畸形的参数载荷属于确定性的模型输出，而不是瞬态传输失败。
        assert!(policy().next_retry(&tool_stream_failure, 0, true).is_none());
    }

    /// Verifies configured inference uses only typed safe dispatch facts for retry.
    /// 验证已配置推理仅依据强类型安全分派事实进行重试。
    #[test]
    fn structured_retry_entrypoint_retries_dispatched_transient_failures() {
        let error = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = error
        else {
            panic!("structured error expected");
        };

        let decision = policy()
            .next_structured_retry(details.as_ref(), 0)
            .expect("typed dispatched transient failure should retry");

        assert_eq!(decision.retry_number, 1);
        assert_eq!(decision.reason, "Provider unavailable");
    }

    /// Verifies the reported APIKEY.FUN 502 remains retryable with empty text and a capped Retry-After delay.
    /// 验证已报告的 APIKEY.FUN 502 在文本为空且 Retry-After 受上限约束时仍可重试。
    #[test]
    fn retryable_502_with_empty_message_uses_same_route_retry_budget() {
        let mut error = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut error
        else {
            panic!("structured error expected");
        };
        details.subcategory = Some(String::from("http"));
        details.http_status = Some(502);
        details.upstream_message = Some(String::new());
        details.message = String::new();
        details.retry_after = Some(RetryAfterHint {
            delay_milliseconds: 60_000,
            source: RetryAfterSource::HeaderSeconds,
        });

        // runtimePolicy mirrors the built-in policy used whenever the persisted retry block is omitted.
        // runtimePolicy 对应持久化 retry 配置块省略时使用的内置策略。
        let runtime_policy =
            ProviderRetryPolicy::from_config(&PersistedProviderRetryConfiguration::default());
        let decision = runtime_policy
            .next_retry(&error, 0, false)
            .expect("safe dispatched 502 should retry");

        assert_eq!(decision.retry_number, 1);
        assert_eq!(decision.max_retries, 5);
        assert_eq!(decision.delay, Duration::from_millis(30_000));
        assert_eq!(decision.reason, "Provider unavailable");
    }

    /// Verifies a normalized provider overload enters the configured transient retry budget.
    /// 验证归一化后的供应商过载错误会进入已配置的瞬态重试预算。
    #[test]
    fn structured_retry_entrypoint_retries_provider_overload() {
        // error mirrors the safe structured facts produced by the Responses decoder.
        // error 对应 Responses 解码器产出的安全结构化事实。
        let error = structured_provider_error(AiErrorCategory::ProviderOverloaded, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = error
        else {
            panic!("structured error expected");
        };

        // decision must use the ordinary configured retry schedule for overload failures.
        // decision 必须对过载失败使用普通配置重试调度。
        let decision = policy()
            .next_structured_retry(details.as_ref(), 0)
            .expect("typed provider overload should retry");

        assert_eq!(decision.retry_number, 1);
        assert_eq!(decision.max_retries, 3);
        assert_eq!(decision.reason, "Provider overloaded");
    }

    /// Verifies the official overload code remains retryable when a legacy adapter leaves the generic bit unset.
    /// 验证旧适配器未设置通用重试标志时，官方过载代码仍会进入重试流程。
    #[test]
    fn structured_retry_entrypoint_recovers_legacy_overload_marker() {
        let mut error = structured_provider_error(AiErrorCategory::Unclassified, false);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut error
        else {
            panic!("structured error expected");
        };
        details.upstream_code = Some(String::from("server_is_overloaded"));
        details.upstream_message = Some(String::from(
            "Our servers are currently overloaded. Please try again later.",
        ));

        let decision = policy()
            .next_structured_retry(details.as_ref(), 0)
            .expect("the registered overload marker should retry");

        assert_eq!(decision.reason, "Provider overloaded");
        assert_eq!(decision.retry_number, 1);
    }

    /// Verifies unknown provider dispatch never triggers transparent replay.
    /// 验证未知供应商分派结果绝不会触发透明重放。
    #[test]
    fn structured_retry_entrypoint_rejects_unknown_dispatch() {
        let mut error = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut error
        else {
            panic!("structured error expected");
        };
        details.dispatch_outcome = DispatchOutcome::Unknown;

        assert!(
            policy()
                .next_structured_retry(details.as_ref(), 0)
                .is_none()
        );
    }

    /// Verifies a legacy zero rate-limit ceiling becomes the finite host ceiling on the typed path.
    /// 验证旧版零限流次数上限会在强类型路径转换为有限宿主上限。
    #[test]
    fn structured_retry_entrypoint_bounds_legacy_zero_rate_limits() {
        let error = structured_provider_error(AiErrorCategory::RateLimited, true);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = error
        else {
            panic!("structured error expected");
        };

        let decision = policy()
            .next_structured_retry(details.as_ref(), 50)
            .expect("typed rate-limit retry should remain inside the finite host budget");

        assert_eq!(decision.retry_number, 51);
        assert!(!decision.unbounded);
        assert_eq!(decision.max_retries, 100_000);
        assert!(
            policy()
                .next_structured_retry(details.as_ref(), 100_000)
                .is_none()
        );
    }

    /// Verifies Anthropic-compatible structured rate-limit errors remain retryable when the proxy omits HTTP 429 from the formatted message.
    /// 验证 Anthropic 兼容代理未在格式化消息中保留 HTTP 429 时，结构化限流错误仍可重试。
    #[test]
    fn retries_structured_anthropic_rate_limit_error_without_http_status() {
        let decision = policy()
            .next_retry(
                &provider_error(
                    "type=rate_limit_error | The engine is currently overloaded, please retry later",
                ),
                0,
                false,
            )
            .expect("structured Anthropic rate-limit errors should be retryable");

        assert_eq!(decision.retry_number, 1);
        assert_eq!(decision.reason, "Provider rate limit");
        assert!(!decision.unbounded);
        assert_eq!(decision.max_retries, 100_000);
    }

    /// Verifies rate-limit retries use their finite host ceiling and cap each wait at 1800 seconds.
    /// 验证限流重试使用有限宿主次数上限，并把每次等待封顶在 1800 秒。
    #[test]
    fn rate_limit_retries_use_host_ceiling_and_cap_delay_at_thirty_minutes() {
        // exhaustedDecision proves the sixth retry is still scheduled after the configured ceiling of three.
        // exhaustedDecision 证明在配置上限三次之后，第六次重试仍会被调度。
        let exhausted_decision = policy()
            .next_retry(&provider_error("type=rate_limit_error"), 5, false)
            .expect("rate-limit retries should ignore the ordinary retry ceiling");
        // cappedDecision proves long-running retries retain the stream while respecting the wait ceiling.
        // cappedDecision 证明长期重试会保持流存活，同时遵守等待上限。
        let capped_decision = policy()
            .next_retry(&provider_error("type=rate_limit_error"), 63, false)
            .expect("rate-limit retries should remain active");

        assert_eq!(exhausted_decision.retry_number, 6);
        assert!(!exhausted_decision.unbounded);
        assert_eq!(exhausted_decision.max_retries, 100_000);
        assert_eq!(capped_decision.delay, Duration::from_secs(1_800));
        assert!(
            policy()
                .next_retry(&provider_error("type=rate_limit_error"), 100_000, false,)
                .is_none()
        );
    }

    /// Verifies ordinary transient failures still stop at the configured retry count.
    /// 验证普通瞬态失败仍会在配置的重试次数后停止。
    #[test]
    fn ordinary_transient_retries_remain_bounded() {
        assert!(
            policy()
                .next_retry(&provider_error("HTTP 503 Service Unavailable"), 3, false)
                .is_none()
        );
    }

    /// Verifies backend rate-limit settings support fixed intervals and a finite retry ceiling.
    /// 验证后台限流设置支持固定间隔与有限重试上限。
    #[test]
    fn backend_rate_limit_rules_support_fixed_and_finite_retries() {
        // config selects a fixed seven-second wait and exactly two rate-limit retries.
        // config 选择固定七秒等待与恰好两次限流重试。
        let config = PersistedProviderRetryConfiguration {
            rate_limit: PersistedProviderRateLimitRetryConfiguration {
                max_retries: 2,
                initial_delay_ms: 7_000,
                max_delay_ms: 30_000,
                backoff_factor: 1.0,
            },
            ..PersistedProviderRetryConfiguration::default()
        };
        // configuredPolicy applies only validated backend values to runtime decisions.
        // configuredPolicy 仅把已校验的后台值应用到运行时决策。
        let configured_policy = ProviderRetryPolicy::from_config(&config);
        // secondDecision verifies the configured fixed interval on the last allowed retry.
        // secondDecision 验证最后一次允许重试仍采用配置的固定间隔。
        let second_decision = configured_policy
            .next_retry(&provider_error("type=rate_limit_error"), 1, false)
            .expect("second configured retry should be allowed");

        assert_eq!(second_decision.retry_number, 2);
        assert_eq!(second_decision.max_retries, 2);
        assert!(!second_decision.unbounded);
        assert_eq!(second_decision.delay, Duration::from_secs(7));
        assert!(
            configured_policy
                .next_retry(&provider_error("type=rate_limit_error"), 2, false)
                .is_none()
        );
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

    /// Verifies a failed mandatory tool durability boundary is never replayed transparently.
    /// 验证失败的强制工具持久化边界绝不会被透明重放。
    #[test]
    fn tool_durability_failure_is_terminal() {
        // error is the exact Core category emitted after a tool side effect cannot be confirmed durably.
        // error 是工具副作用无法被可靠确认后发出的精确 Core 分类。
        let error = VulcanCoreError::ToolDurabilityFailure {
            tool_name: String::from("file"),
            message: String::from("canonical ChangeSet rejection"),
            details: None,
        };
        assert!(policy().next_retry(&error, 0, false).is_none());
        assert!(classify_retry_reason(&error, true, &[]).is_none());
    }

    /// Verifies repeated malformed tool arguments remain a terminal tool-protocol failure.
    /// 验证重复的非法工具参数保持为终态工具协议失败。
    #[test]
    fn repeated_malformed_tool_arguments_are_terminal() {
        // error is the exact Core category emitted after the single automatic correction round is exhausted.
        // error 是唯一一次自动纠错轮耗尽后发出的精确 Core 分类。
        let error = VulcanCoreError::ToolProtocolFailure {
            tool_name: String::from("file_edit"),
            message: String::from(
                "tool_argument_parse_failure_exhausted: invalid arguments twice in one user turn",
            ),
        };
        assert!(policy().next_retry(&error, 0, false).is_none());
        assert!(classify_retry_reason(&error, true, &[]).is_none());
    }

    /// Verifies an unreplayable reasoning history is never retried or routed elsewhere.
    /// 验证无法回放的推理历史绝不会被重试或改走其他路由。
    ///
    /// # Remarks
    /// 说明
    /// The repair protocol accepts exactly one compatibility consequence: a session whose reasoning
    /// cannot be replayed fails visibly. Retrying it would re-send identical history, and switching
    /// protocol would silently drop the reasoning this contract exists to protect.
    /// 修复协议只接受一项兼容后果：推理无法回放的会话可见失败。重试会重发同样的历史，
    /// 切换协议则会静默丢弃本契约存在所要保护的推理。
    #[test]
    fn unreplayable_reasoning_history_is_terminal_and_never_switches_route() {
        let mut error = structured_provider_error(AiErrorCategory::InvalidRequest, false);
        let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut error
        else {
            panic!("structured error expected");
        };
        details.subcategory = Some(String::from(
            vulcan_provider_adapters::REASONING_REPLAY_UNREPLAYABLE_SUBCATEGORY,
        ));
        details.stage = String::from("prepare");
        details.message =
            String::from(vulcan_provider_adapters::REASONING_REPLAY_UNREPLAYABLE_MESSAGE);
        details.dispatch_outcome = DispatchOutcome::NotDispatched;

        assert!(policy().next_retry(&error, 0, false).is_none());
        assert!(classify_retry_reason(&error, true, &[]).is_none());
    }

    /// Verifies visible assistant output marks the attempt as unsafe to replay.
    /// 验证可见助手输出会把本次尝试标记为不适合重放。
    #[test]
    fn marks_visible_model_output_as_retry_unsafe() {
        assert!(provider_delta_makes_retry_unsafe(
            &ProviderStreamDelta::Output(String::from("hello"))
        ));
    }

    /// Verifies model tool fragments and observed billing usage both prohibit transparent replay.
    /// 验证模型工具碎片与已观察到的计费用量都会禁止透明重放。
    #[test]
    fn marks_tool_fragments_and_usage_as_retry_unsafe() {
        let tool_fragment =
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::ArgumentsDelta {
                request_stream_id: String::from("request-1"),
                delta: String::from("{}"),
            });
        assert!(provider_delta_makes_retry_unsafe(&tool_fragment));
        assert!(provider_delta_makes_retry_unsafe(
            &ProviderStreamDelta::UsageUpdated(CoreProviderUsageSnapshot {
                total_tokens: Some(1),
                ..CoreProviderUsageSnapshot::default()
            })
        ));
    }

    /// Verifies a response identity alone keeps a dispatched transport close replayable.
    /// 验证仅有响应身份时，已分派的传输关闭仍可重放。
    #[test]
    fn websocket_loss_after_semantic_output_replays_but_respects_side_effects() {
        let mut loss = structured_provider_error(AiErrorCategory::ProviderUnavailable, true);
        if let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut loss
        {
            details.transport = Some(AiTransportKind::ResponsesWebsocket);
            details.subcategory = Some(String::from("transport"));
            details.stage = String::from("response_stream");
            details.semantic_output_observed = true;
        }
        assert!(policy().next_retry(&loss, 0, true).is_some());
        if let VulcanCoreError::ProviderFailure {
            details: Some(details),
            ..
        } = &mut loss
        {
            details.transport = Some(AiTransportKind::Http);
        }
        assert!(policy().next_retry(&loss, 0, true).is_none());
    }

    #[test]
    fn response_identity_alone_keeps_transport_close_replayable() {
        assert!(!provider_delta_makes_retry_unsafe(
            &ProviderStreamDelta::ResponseIdentityObserved(String::from("resp_1"))
        ));
        let decision = policy().next_retry(
            &structured_provider_error(AiErrorCategory::ProviderUnavailable, true),
            0,
            false,
        );
        assert!(decision.is_some());
    }
}

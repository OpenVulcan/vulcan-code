//! Provider retry settings services for the WebUI settings surface.
//! WebUI 设置界面的供应商重试设置服务。

use serde::Deserialize;
use vulcan_agent_runtime::provider_config::{
    PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS, PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES,
    PROVIDER_RETRY_KEYWORD_MAX_COUNT, PROVIDER_RETRY_MAX_BACKOFF_FACTOR_MILLI,
    PROVIDER_RETRY_MAX_JITTER_PERCENT, PROVIDER_RETRY_MAX_RETRIES,
    PROVIDER_RETRY_MIN_BACKOFF_FACTOR_MILLI, PROVIDER_RETRY_REGEX_PREFIX,
    PersistedProviderRateLimitRetryConfiguration, PersistedProviderRetryConfiguration,
    ProviderConfigurationFile, ProviderRetryRule, default_provider_retry_keywords,
    normalize_provider_rate_limit_retry_max_retries, normalize_provider_retry_backoff_factor,
};
use vulcan_service_api::{
    ProviderRateLimitRetrySettingsSummary, ProviderRetrySettingsSummary, ServiceEventEmitter,
    WorkbenchSnapshot,
};
use vulcan_workspace_store::workspace_store::ProviderApiTraceStore;

use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use crate::{
    ServiceBridgeState, SharedServiceBridgeState, emit_workbench_event,
    mutate_provider_configuration, refresh_settings_projection_locked, shell_host_from_state,
};

/// Provider retry settings save request sent by the WebUI settings page.
/// WebUI 设置页发送的 Provider 重试设置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderRetrySettingsSaveRequest {
    /// Whether automatic Provider retries are enabled.
    /// 是否启用 Provider 自动重试。
    enabled: bool,
    /// Maximum ordinary transient retry count.
    /// 普通瞬态错误的最大重试次数。
    max_retries: u32,
    /// Initial ordinary retry delay in milliseconds.
    /// 普通重试的初始等待毫秒数。
    initial_delay_ms: u64,
    /// Maximum ordinary retry delay in milliseconds.
    /// 普通重试的最大等待毫秒数。
    max_delay_ms: u64,
    /// Ordinary retry backoff multiplier represented in thousandths.
    /// 以千分数表示的普通重试退避倍数。
    backoff_factor_milli: u32,
    /// Deterministic jitter percentage.
    /// 确定性抖动百分比。
    jitter_percent: u8,
    /// Whether Retry-After hints may increase waits.
    /// 是否允许 Retry-After 提示增加等待时间。
    respect_retry_after: bool,
    /// Whether HTTP 429 and structured rate-limit errors may be retried.
    /// 是否允许重试 HTTP 429 与结构化限流错误。
    retry_429: bool,
    /// Complete retry rule list; each line is a literal keyword or a prefixed regex.
    /// 完整的重试规则列表；每行是字面关键词或带前缀的正则。
    retry_keywords: Vec<String>,
    /// Dedicated rate-limit retry schedule.
    /// 专用限流重试调度规则。
    rate_limit: ProviderRateLimitRetrySettingsSaveRequest,
}

/// Dedicated rate-limit scheduling values submitted by the settings page.
/// 设置页面提交的专用限流调度值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRateLimitRetrySettingsSaveRequest {
    /// Maximum finite retry count; legacy zero requests migrate to the host hard ceiling.
    /// 最大有限重试次数；旧版零值请求会迁移到宿主硬上限。
    max_retries: u32,
    /// Delay before the first rate-limit retry in milliseconds.
    /// 第一次限流重试前的等待毫秒数。
    initial_delay_ms: u64,
    /// Maximum rate-limit delay in milliseconds.
    /// 最大限流等待毫秒数。
    max_delay_ms: u64,
    /// Rate-limit backoff multiplier represented in thousandths.
    /// 以千分数表示的限流退避倍数。
    backoff_factor_milli: u32,
}

/// Provider request-trace settings submitted by the runtime diagnostics page.
/// 运行诊断页面提交的 Provider 请求追踪设置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderRequestTraceSettingsSaveRequest {
    /// Whether real Provider dispatches write bounded wire-request traces into session temp directories.
    /// 真实 Provider 分派是否将有界线上请求追踪写入会话临时目录。
    enabled: bool,
    /// Updates error-only capture when supplied; older clients preserve the existing preference.
    /// 传入时更新仅错误捕获；旧客户端保留原有偏好。
    #[serde(default)]
    errors_enabled: Option<bool>,
}

/// Saves Provider retry settings from the WebUI settings page.
/// 从 WebUI 设置页保存 Provider 重试设置。
pub(super) async fn service_save_provider_retry_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderRetrySettingsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    validate_provider_retry_settings_request(&request)?;
    let persisted_retry = PersistedProviderRetryConfiguration {
        enabled: request.enabled,
        max_retries: request.max_retries,
        initial_delay_ms: request.initial_delay_ms,
        max_delay_ms: request.max_delay_ms,
        backoff_factor: request.backoff_factor_milli as f64 / 1_000.0,
        jitter_percent: request.jitter_percent,
        respect_retry_after: request.respect_retry_after,
        retry_429: request.retry_429,
        retry_keywords: normalized_retry_keywords(&request.retry_keywords),
        rate_limit: PersistedProviderRateLimitRetryConfiguration {
            max_retries: request.rate_limit.max_retries,
            initial_delay_ms: request.rate_limit.initial_delay_ms,
            max_delay_ms: request.rate_limit.max_delay_ms,
            backoff_factor: request.rate_limit.backoff_factor_milli as f64 / 1_000.0,
        },
    };
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the provider retry settings",
        |configuration| {
            configuration.retry = persisted_retry.clone();
            Ok(())
        },
        project_saved_provider_retry_settings,
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Appends a single rule to the current retry policy in one configuration transaction.
/// 在一次配置事务中将单条规则追加到当前重试策略。
///
/// `app` publishes the updated workbench, `state` owns the policy, and `rule` is the submitted rule line.
/// `app` 发布更新后的工作台，`state` 持有策略，`rule` 是提交的一行规则。
/// Returns success after persistence, or a validation or persistence error.
/// 持久化完成后返回成功，校验或持久化失败则返回错误。
pub(super) async fn service_add_provider_retry_rule(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    rule: String,
) -> Result<(), String> {
    // Read and append under the same lock so simultaneous additions cannot erase each other.
    // 在同一把锁内读取并追加，避免同时添加时相互覆盖。
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to add the provider retry rule",
        |configuration| append_provider_retry_rule(&mut configuration.retry, &rule),
        project_saved_provider_retry_settings,
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(())
}

/// Appends one nonblank rule to `retry` and returns an error if `rule` would make the list invalid.
/// 将一条非空规则追加到 `retry`，若 `rule` 使列表无效则返回错误。
fn append_provider_retry_rule(
    retry: &mut PersistedProviderRetryConfiguration,
    rule: &str,
) -> Result<(), String> {
    if rule.trim().is_empty() {
        return Err(String::from("A provider retry rule cannot be blank."));
    }
    // Stage the deduplicated list before replacing the current rules.
    // 先构造去重后的列表，通过校验后才替换当前规则。
    let mut rules = retry.retry_keywords.clone();
    rules.push(rule.to_owned());
    validate_retry_keywords(&rules)?;
    retry.retry_keywords = normalized_retry_keywords(&rules);
    Ok(())
}

/// Projects a committed `configuration` into the runtime and returns the refreshed workbench from `state`.
/// 将已提交的 `configuration` 应用到运行时，并从 `state` 返回刷新的工作台。
fn project_saved_provider_retry_settings(
    state: &mut ServiceBridgeState,
    configuration: &ProviderConfigurationFile,
) -> WorkbenchSnapshot {
    if let Some(runtime) = state.agent_runtime.as_ref() {
        runtime
            .configured_inference_service
            .update_retry_configuration(&configuration.retry);
    }
    refresh_settings_projection_locked(state, configuration);
    state.workbench.clone()
}

/// Saves the bounded Provider wire-request trace diagnostic switch from system settings.
/// 从系统设置保存有界 Provider 线上请求追踪诊断开关。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench, `state` owns shared configuration, and `request` carries the exact boolean selection.
/// `app` 发出刷新后的工作台，`state` 持有共享配置，`request` 承载精确布尔选择。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after durable configuration persistence.
/// 持久化配置完成后返回刷新的工作台快照。
pub(super) async fn service_save_provider_request_trace_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderRequestTraceSettingsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the provider request trace settings",
        |configuration| {
            configuration.provider_request_trace_enabled = request.enabled;
            if let Some(enabled) = request.errors_enabled {
                configuration.provider_error_trace_enabled = enabled;
            }
            Ok(())
        },
        |guard, configuration| {
            if let Some(runtime) = guard.agent_runtime.as_ref() {
                runtime
                    .configured_inference_service
                    .update_provider_request_trace_enabled(
                        configuration.provider_request_trace_enabled,
                    );
            }
            refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Opens the selected retained session's exact Provider API trace directory.
/// 打开当前选中保留会话的精确 Provider API 追踪目录。
///
/// # Parameters
/// 参数
/// `state` owns the selected conversation, retained locator, and host shell authority.
/// `state` 持有当前会话、保留定位信息与宿主 Shell 权限。
///
/// # Returns
/// 返回值
/// The canonical directory path after the host file manager accepts the request.
/// 宿主文件管理器接受请求后返回 canonical 目录路径。
pub(super) fn service_open_selected_provider_api_trace_directory(
    state: SharedServiceBridgeState,
) -> Result<String, String> {
    let trace_store = selected_provider_trace_store(&state)?;
    let trace_directory = trace_store
        .prepare_directory()
        .map_err(provider_trace_diagnostic_message)?;
    let trace_path = trace_directory.to_string_lossy().into_owned();
    shell_host_from_state(&state)?.open_path(&trace_path)?;
    Ok(trace_path)
}

/// Clears the selected retained session's exact Provider API trace directory.
/// 清除当前选中保留会话的精确 Provider API 追踪目录。
///
/// # Parameters
/// 参数
/// `state` owns the selected conversation and retained storage locator.
/// `state` 持有当前会话与保留存储定位信息。
///
/// # Returns
/// 返回值
/// `true` when trace evidence existed and was removed, otherwise `false`.
/// 存在追踪证据且已移除时返回 `true`，否则返回 `false`。
pub(super) fn service_clear_selected_provider_api_trace_directory(
    state: SharedServiceBridgeState,
) -> Result<bool, String> {
    selected_provider_trace_store(&state)?
        .clear_directory()
        .map_err(provider_trace_diagnostic_message)
}

/// Resolves the exact V1 trace-store owner for the selected hydrated session.
/// 为当前选中的已 hydration 会话解析精确 V1 追踪存储 owner。
fn selected_provider_trace_store(
    state: &SharedServiceBridgeState,
) -> Result<ProviderApiTraceStore, String> {
    let selected_conversation_id = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .workbench
            .selected_conversation_id
            .clone()
            .ok_or_else(|| String::from("No conversation is selected for Provider API traces."))?
    };
    let session_id = crate::SessionId::parse_str(&selected_conversation_id).map_err(|error| {
        format!(
            "Selected conversation id `{selected_conversation_id}` is not a valid session id: {error}"
        )
    })?;
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    handle
        .provider_api_trace_store()
        .ok_or_else(|| format!("V1 session `{session_id}` has no Provider API trace-store owner."))
}

/// Formats one non-fatal trace diagnostic for an explicit user maintenance action.
/// 为显式用户维护操作格式化一条非致命追踪诊断。
fn provider_trace_diagnostic_message(
    diagnostic: vulcan_workspace_store::workspace_store::ProviderApiTraceDiagnostic,
) -> String {
    format!(
        "Provider API trace operation failed [{}]: {}",
        diagnostic.code.as_str(),
        diagnostic.detail
    )
}

/// Builds the browser-facing retry settings summary from shared Provider configuration.
/// 从共享 Provider 配置构建浏览器侧重试设置摘要。
pub(crate) fn provider_retry_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> ProviderRetrySettingsSummary {
    let retry = &provider_configuration.retry;
    let rate_limit_initial_delay_ms = retry
        .rate_limit
        .initial_delay_ms
        .clamp(1, PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS);
    let rate_limit_max_delay_ms = retry.rate_limit.max_delay_ms.clamp(
        rate_limit_initial_delay_ms,
        PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS,
    );
    ProviderRetrySettingsSummary {
        enabled: retry.enabled,
        max_retries: retry.max_retries.min(PROVIDER_RETRY_MAX_RETRIES),
        initial_delay_ms: retry.initial_delay_ms.max(1),
        max_delay_ms: retry.max_delay_ms.max(retry.initial_delay_ms.max(1)),
        backoff_factor_milli: normalized_backoff_factor_milli(retry.backoff_factor),
        jitter_percent: retry.jitter_percent.min(PROVIDER_RETRY_MAX_JITTER_PERCENT),
        respect_retry_after: retry.respect_retry_after,
        retry_429: retry.retry_429,
        retry_keywords: normalized_retry_keywords(&retry.retry_keywords),
        default_retry_keywords: default_provider_retry_keywords(),
        rate_limit: ProviderRateLimitRetrySettingsSummary {
            max_retries: normalize_provider_rate_limit_retry_max_retries(
                retry.rate_limit.max_retries,
            ),
            initial_delay_ms: rate_limit_initial_delay_ms,
            max_delay_ms: rate_limit_max_delay_ms,
            backoff_factor_milli: normalized_backoff_factor_milli(retry.rate_limit.backoff_factor),
        },
    }
}

/// Validates retry settings before any shared configuration mutation occurs.
/// 在修改共享配置前校验重试设置。
fn validate_provider_retry_settings_request(
    request: &ProviderRetrySettingsSaveRequest,
) -> Result<(), String> {
    if request.max_retries > PROVIDER_RETRY_MAX_RETRIES {
        return Err(format!(
            "The maximum ordinary retry count must be between 0 and {PROVIDER_RETRY_MAX_RETRIES}."
        ));
    }
    validate_retry_delay_range(
        "ordinary retry",
        request.initial_delay_ms,
        request.max_delay_ms,
        None,
    )?;
    validate_backoff_factor_milli("ordinary retry", request.backoff_factor_milli)?;
    if request.jitter_percent > PROVIDER_RETRY_MAX_JITTER_PERCENT {
        return Err(format!(
            "The ordinary retry jitter percentage must be between 0 and {PROVIDER_RETRY_MAX_JITTER_PERCENT}."
        ));
    }
    validate_retry_keywords(&request.retry_keywords)?;
    if !(1..=PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES).contains(&request.rate_limit.max_retries) {
        return Err(format!(
            "The maximum rate-limit retry count must be between 1 and {PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES}."
        ));
    }
    validate_retry_delay_range(
        "rate-limit retry",
        request.rate_limit.initial_delay_ms,
        request.rate_limit.max_delay_ms,
        Some(PROVIDER_RATE_LIMIT_RETRY_MAX_DELAY_MS),
    )?;
    validate_backoff_factor_milli("rate-limit retry", request.rate_limit.backoff_factor_milli)
}

/// Validates the submitted retry keyword list against the persisted count and length ceilings.
/// 按持久化数量与长度上限校验提交的重试关键词列表。
///
/// # Parameters
/// 参数
/// `keywords` is the complete list submitted by the settings page.
/// `keywords` 是设置页提交的完整列表。
///
/// # Returns
/// 返回值
/// `Ok(())` when every non-blank keyword fits; blank entries are dropped during normalization.
/// 每个非空关键词都符合上限时返回 `Ok(())`；空白条目在规范化时被丢弃。
fn validate_retry_keywords(keywords: &[String]) -> Result<(), String> {
    let normalized = normalized_retry_keywords(keywords);
    if normalized.len() > PROVIDER_RETRY_KEYWORD_MAX_COUNT {
        return Err(format!(
            "At most {PROVIDER_RETRY_KEYWORD_MAX_COUNT} retry rules are allowed."
        ));
    }
    for (index, rule) in normalized.iter().enumerate() {
        ProviderRetryRule::parse(rule)
            .map_err(|error| format!("Retry rule {}: {error}", index + 1))?;
    }
    Ok(())
}

/// Trims rules and removes blank or duplicate entries, preserving order and regex case.
/// 修剪规则并移除空白或重复条目，保留顺序及正则表达式的大小写。
///
/// # Parameters
/// 参数
/// `keywords` is one submitted or persisted keyword list.
/// `keywords` 是一份提交或持久化的关键词列表。
///
/// # Returns
/// 返回值
/// The canonical keyword list stored and projected by the settings surface.
/// 返回设置界面存储与投影的规范关键词列表。
fn normalized_retry_keywords(keywords: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|rule| {
            // Literal rules fold case for deduplication; a regex preserves every character.
            // 字面规则去重时折叠大小写，正则表达式保留每个字符。
            let key = match rule.strip_prefix(PROVIDER_RETRY_REGEX_PREFIX) {
                Some(pattern) => (true, pattern.to_owned()),
                None => (false, rule.to_lowercase()),
            };
            !rule.is_empty() && seen.insert(key)
        })
        .map(String::from)
        .collect()
}

/// Validates one initial and maximum retry-delay pair in milliseconds.
/// 校验一组以毫秒表示的初始与最大重试等待。
fn validate_retry_delay_range(
    label: &str,
    initial_ms: u64,
    max_ms: u64,
    delay_ceiling_ms: Option<u64>,
) -> Result<(), String> {
    if initial_ms == 0 {
        return Err(format!(
            "The {label} initial delay must be greater than 0 milliseconds."
        ));
    }
    if max_ms < initial_ms {
        return Err(format!(
            "The {label} maximum delay must not be smaller than the initial delay."
        ));
    }
    if let Some(ceiling_ms) = delay_ceiling_ms.filter(|ceiling_ms| max_ms > *ceiling_ms) {
        return Err(format!(
            "The {label} maximum delay must not exceed {} seconds.",
            ceiling_ms / 1_000
        ));
    }
    Ok(())
}

/// Validates one fixed-point backoff multiplier submitted by the browser.
/// 校验浏览器提交的定点退避倍数。
fn validate_backoff_factor_milli(label: &str, factor_milli: u32) -> Result<(), String> {
    if !(PROVIDER_RETRY_MIN_BACKOFF_FACTOR_MILLI..=PROVIDER_RETRY_MAX_BACKOFF_FACTOR_MILLI)
        .contains(&factor_milli)
    {
        return Err(format!(
            "The {label} backoff multiplier must be between {:.1} and {:.1}.",
            f64::from(PROVIDER_RETRY_MIN_BACKOFF_FACTOR_MILLI) / 1_000.0,
            f64::from(PROVIDER_RETRY_MAX_BACKOFF_FACTOR_MILLI) / 1_000.0,
        ));
    }
    Ok(())
}

/// Normalizes one persisted floating-point multiplier into the browser fixed-point contract.
/// 将一个持久化浮点倍数规范化为浏览器定点契约。
fn normalized_backoff_factor_milli(factor: f64) -> u32 {
    (normalize_provider_retry_backoff_factor(factor) * 1_000.0).round() as u32
}

#[cfg(test)]
mod tests {
    //! Focused tests for Provider retry settings projection and validation.
    //! Provider 重试设置投影与校验的定向测试。

    use super::*;

    /// Builds one valid browser retry-settings request.
    /// 构建一份有效的浏览器重试设置请求。
    fn valid_retry_request() -> ProviderRetrySettingsSaveRequest {
        ProviderRetrySettingsSaveRequest {
            enabled: true,
            max_retries: 5,
            initial_delay_ms: 2_000,
            max_delay_ms: 30_000,
            backoff_factor_milli: 2_000,
            jitter_percent: 20,
            respect_retry_after: true,
            retry_429: true,
            retry_keywords: default_provider_retry_keywords(),
            rate_limit: ProviderRateLimitRetrySettingsSaveRequest {
                max_retries: PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES,
                initial_delay_ms: 5_000,
                max_delay_ms: 1_800_000,
                backoff_factor_milli: 2_000,
            },
        }
    }

    /// Verifies the settings summary exposes the exact backend retry configuration.
    /// 验证设置摘要会公开精确的后台重试配置。
    #[test]
    fn retry_settings_summary_projects_backend_configuration() {
        let mut configuration = ProviderConfigurationFile::default();
        configuration.retry.rate_limit = PersistedProviderRateLimitRetryConfiguration {
            max_retries: 12,
            initial_delay_ms: 9_000,
            max_delay_ms: 60_000,
            backoff_factor: 1.0,
        };
        let summary = provider_retry_settings(&configuration);
        assert_eq!(summary.rate_limit.max_retries, 12);
        assert_eq!(summary.rate_limit.initial_delay_ms, 9_000);
        assert_eq!(summary.rate_limit.max_delay_ms, 60_000);
        assert_eq!(summary.rate_limit.backoff_factor_milli, 1_000);
    }

    /// Verifies validation accepts a finite fixed schedule and rejects an inverted delay range.
    /// 验证校验会接受有限固定间隔调度，并拒绝倒置的等待范围。
    #[test]
    fn retry_settings_validation_enforces_cross_field_delay_order() {
        let mut valid_request = valid_retry_request();
        valid_request.rate_limit.backoff_factor_milli = 1_000;
        assert!(validate_provider_retry_settings_request(&valid_request).is_ok());
        let mut invalid_request = valid_request;
        invalid_request.rate_limit.max_delay_ms = 4_999;
        assert!(validate_provider_retry_settings_request(&invalid_request).is_err());
    }

    /// Verifies only rate-limit retries apply the thirty-minute hard delay ceiling.
    /// 验证只有限流重试会应用三十分钟的硬等待上限。
    #[test]
    fn retry_settings_validation_limits_only_rate_limit_delays() {
        let mut ordinary_request = valid_retry_request();
        ordinary_request.initial_delay_ms = 1_800_001;
        ordinary_request.max_delay_ms = 3_600_000;
        assert!(validate_provider_retry_settings_request(&ordinary_request).is_ok());
        let mut rate_limit_request = valid_retry_request();
        rate_limit_request.rate_limit.max_delay_ms = 1_800_001;
        assert!(validate_provider_retry_settings_request(&rate_limit_request).is_err());
    }

    /// Verifies keyword normalization and the count and length ceilings.
    /// 验证关键词规范化以及数量与长度上限。
    #[test]
    fn retry_keywords_are_normalized_and_bounded() {
        let submitted = vec![
            String::from("  Connection Reset "),
            String::from("connection reset"),
            String::from("   "),
            String::from("broken pipe"),
        ];
        assert_eq!(
            normalized_retry_keywords(&submitted),
            vec![
                String::from("Connection Reset"),
                String::from("broken pipe")
            ]
        );

        let mut too_many = valid_retry_request();
        too_many.retry_keywords = (0..=PROVIDER_RETRY_KEYWORD_MAX_COUNT)
            .map(|index| format!("keyword-{index}"))
            .collect();
        assert!(validate_provider_retry_settings_request(&too_many).is_err());
        let mut too_long = valid_retry_request();
        too_long.retry_keywords = vec!["k".repeat(
            vulcan_agent_runtime::provider_config::PROVIDER_RETRY_KEYWORD_MAX_CHARACTERS + 1,
        )];
        assert!(validate_provider_retry_settings_request(&too_long).is_err());
        let mut cleared = valid_retry_request();
        cleared.retry_keywords = Vec::new();
        assert!(validate_provider_retry_settings_request(&cleared).is_ok());

        let summary = provider_retry_settings(&ProviderConfigurationFile::default());
        assert_eq!(summary.retry_keywords, default_provider_retry_keywords());
        assert_eq!(
            summary.default_retry_keywords,
            default_provider_retry_keywords()
        );
    }

    /// Verifies settings reject malformed regexes and preserve case-distinct valid patterns.
    /// 验证设置拒绝无效正则，并保留大小写不同的有效表达式。
    #[test]
    fn retry_settings_validate_regex_rules() {
        // These patterns differ only by case and must remain independently editable.
        // 两个表达式仅大小写不同，必须各自可编辑。
        let mut request = valid_retry_request();
        request.retry_keywords = vec!["regex:Timeout".into(), "regex:timeout".into()];
        assert!(validate_provider_retry_settings_request(&request).is_ok());
        assert_eq!(
            normalized_retry_keywords(&request.retry_keywords),
            request.retry_keywords
        );
        request.retry_keywords.push("regex:[".into());
        assert!(validate_provider_retry_settings_request(&request).is_err());
    }

    /// Verifies appending a rule preserves settings and rejects invalid input without mutation.
    /// 验证追加规则保留设置，且拒绝无效输入时不产生变更。
    #[test]
    fn append_retry_rule_keeps_existing_policy_and_deduplicates() {
        // Start with disabled automatic retry to verify adding a rule cannot enable it.
        // 从关闭自动重试开始，验证添加规则无法开启它。
        let mut retry = PersistedProviderRetryConfiguration {
            enabled: false,
            ..PersistedProviderRetryConfiguration::default()
        };
        // Extend the default rules with one expected expression.
        // 在默认规则后追加一条预期正则。
        let mut expected = default_provider_retry_keywords();
        expected.push("regex:^temporary [0-9]+$".into());
        append_provider_retry_rule(&mut retry, "regex:^temporary [0-9]+$").unwrap();
        append_provider_retry_rule(&mut retry, " regex:^temporary [0-9]+$ ").unwrap();
        assert!(append_provider_retry_rule(&mut retry, "regex:[").is_err());
        assert!(!retry.enabled);
        assert_eq!(retry.retry_keywords, expected);
    }
}

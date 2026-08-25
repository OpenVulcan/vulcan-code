//! Provider retry settings services for the WebUI settings surface.
//! WebUI 设置界面的供应商重试设置服务。

use serde::Deserialize;
use vulcan_agent_runtime::provider_config::{
    PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES, PROVIDER_RETRY_MAX_RETRIES,
    PersistedProviderRateLimitRetryConfiguration, PersistedProviderRetryConfiguration,
    ProviderConfigurationFile, normalize_provider_rate_limit_retry_max_retries,
};
use vulcan_service_api::{
    ProviderRateLimitRetrySettingsSummary, ProviderRetrySettingsSummary, ServiceEventEmitter,
    WorkbenchSnapshot,
};

use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_provider_configuration,
    refresh_settings_projection_locked, require_segmented_session_directory_for_retained_session,
    shell_host_from_state,
};

/// Hard delay ceiling shared by the retry settings page and runtime rate-limit policy.
/// 重试设置页面与运行时限流策略共享的硬等待上限。
const PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS: u64 = 1_800_000;

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
        rate_limit: PersistedProviderRateLimitRetryConfiguration {
            max_retries: normalize_provider_rate_limit_retry_max_retries(
                request.rate_limit.max_retries,
            ),
            initial_delay_ms: request.rate_limit.initial_delay_ms,
            max_delay_ms: request.rate_limit.max_delay_ms,
            backoff_factor: request.rate_limit.backoff_factor_milli as f64 / 1_000.0,
        },
    };
    let workbench = mutate_provider_configuration(
        &state,
        "保存 Provider 重试设置失败",
        |configuration| {
            configuration.retry = persisted_retry.clone();
            Ok(())
        },
        |guard, configuration| {
            if let Some(runtime) = guard.agent_runtime.as_ref() {
                runtime
                    .configured_inference_service
                    .update_retry_configuration(&configuration.retry);
            }
            refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
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
        "保存 Provider 请求追踪设置失败",
        |configuration| {
            configuration.provider_request_trace_enabled = request.enabled;
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
    let (session_id, session_directory) = selected_provider_trace_session(&state)?;
    let trace_directory =
        vulcan_workspace_store::workspace_store::prepare_provider_api_trace_directory(
            &session_directory,
            session_id,
        )?;
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
    let (session_id, session_directory) = selected_provider_trace_session(&state)?;
    vulcan_workspace_store::workspace_store::clear_provider_api_trace_directory(
        &session_directory,
        session_id,
    )
}

/// Resolves the exact selected session identity and retained directory without filesystem scans.
/// 在不扫描文件系统的情况下解析当前会话的精确身份与保留目录。
fn selected_provider_trace_session(
    state: &SharedServiceBridgeState,
) -> Result<(uuid::Uuid, std::path::PathBuf), String> {
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let selected_conversation_id = guard
        .workbench
        .selected_conversation_id
        .as_deref()
        .ok_or_else(|| String::from("No conversation is selected for Provider API traces."))?;
    let session_id = uuid::Uuid::parse_str(selected_conversation_id).map_err(|error| {
        format!(
            "Selected conversation id `{selected_conversation_id}` is not a valid session id: {error}"
        )
    })?;
    let session_directory =
        require_segmented_session_directory_for_retained_session(&guard, &data_root, session_id)?;
    Ok((session_id, session_directory))
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
        .clamp(1, PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS);
    let rate_limit_max_delay_ms = retry.rate_limit.max_delay_ms.clamp(
        rate_limit_initial_delay_ms,
        PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS,
    );
    ProviderRetrySettingsSummary {
        enabled: retry.enabled,
        max_retries: retry.max_retries.min(PROVIDER_RETRY_MAX_RETRIES),
        initial_delay_ms: retry.initial_delay_ms.max(1),
        max_delay_ms: retry.max_delay_ms.max(retry.initial_delay_ms.max(1)),
        backoff_factor_milli: normalized_backoff_factor_milli(retry.backoff_factor),
        jitter_percent: retry.jitter_percent.min(80),
        respect_retry_after: retry.respect_retry_after,
        retry_429: retry.retry_429,
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
            "普通重试最大次数必须位于 0 到 {PROVIDER_RETRY_MAX_RETRIES} 之间。"
        ));
    }
    validate_retry_delay_range(
        "普通重试",
        request.initial_delay_ms,
        request.max_delay_ms,
        None,
    )?;
    validate_backoff_factor_milli("普通重试", request.backoff_factor_milli)?;
    if request.jitter_percent > 80 {
        return Err(String::from("普通重试抖动百分比必须位于 0 到 80 之间。"));
    }
    if request.rate_limit.max_retries > PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES {
        return Err(format!(
            "限流重试最大次数必须位于 0 到 {PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES} 之间。"
        ));
    }
    validate_retry_delay_range(
        "限流重试",
        request.rate_limit.initial_delay_ms,
        request.rate_limit.max_delay_ms,
        Some(PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS),
    )?;
    validate_backoff_factor_milli("限流重试", request.rate_limit.backoff_factor_milli)
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
        return Err(format!("{label}首次等待必须大于 0 毫秒。"));
    }
    if max_ms < initial_ms {
        return Err(format!("{label}最大等待必须不小于首次等待。"));
    }
    if delay_ceiling_ms.is_some_and(|ceiling_ms| max_ms > ceiling_ms) {
        return Err(format!("{label}最大等待不能超过 1800 秒。"));
    }
    Ok(())
}

/// Validates one fixed-point backoff multiplier submitted by the browser.
/// 校验浏览器提交的定点退避倍数。
fn validate_backoff_factor_milli(label: &str, factor_milli: u32) -> Result<(), String> {
    if !(1_000..=8_000).contains(&factor_milli) {
        return Err(format!("{label}递增倍数必须位于 1.0 到 8.0 之间。"));
    }
    Ok(())
}

/// Normalizes one persisted floating-point multiplier into the browser fixed-point contract.
/// 将一个持久化浮点倍数规范化为浏览器定点契约。
fn normalized_backoff_factor_milli(factor: f64) -> u32 {
    let normalized = if factor.is_finite() {
        factor.clamp(1.0, 8.0)
    } else {
        2.0
    };
    (normalized * 1_000.0).round() as u32
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

    /// Verifies a legacy zero retry sentinel is projected as the finite host ceiling.
    /// 验证旧版零重试哨兵会被投影为有限宿主上限。
    #[test]
    fn retry_settings_summary_normalizes_legacy_zero_retry_sentinel() {
        let mut configuration = ProviderConfigurationFile::default();
        configuration.retry.rate_limit.max_retries = 0;

        let summary = provider_retry_settings(&configuration);

        assert_eq!(
            summary.rate_limit.max_retries,
            PROVIDER_RATE_LIMIT_RETRY_MAX_RETRIES
        );
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
}

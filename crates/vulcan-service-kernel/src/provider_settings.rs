//! Provider configuration save services for the WebUI settings surface.
//! WebUI 设置界面的供应商配置保存服务。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use vulcan_agent_runtime::{
    agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_tool_visibility_cache,
    provider_config::{
        self, PersistedModelOverrideEntry, PersistedProviderRateLimitRetryConfiguration,
        PersistedProviderRetryConfiguration, ProviderConfigurationFile, ReasoningProfileConfig,
    },
};
use vulcan_provider_config::config::{
    PersistedProviderEntry, PersistedProviderOptions, ProviderFieldKey,
};
use vulcan_service_api::{
    ProviderRateLimitRetrySettingsSummary, ProviderRetrySettingsSummary, ServiceEventEmitter,
    WorkbenchSnapshot,
};

use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_provider_configuration,
    provider_field_request_key, refresh_settings_projection_locked,
};

/// Hard delay ceiling shared by the retry settings page and runtime rate-limit policy.
/// 重试设置页面与运行时限流策略共享的硬等待上限。
const PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS: u64 = 1_800_000;

/// Maximum finite rate-limit retry count accepted from the settings page.
/// 设置页面允许提交的最大有限限流重试次数。
const PROVIDER_RATE_LIMIT_SETTINGS_MAX_RETRIES: u32 = 100_000;

/// Provider configuration save request sent by the WebUI settings page.
/// WebUI 设置页发送的 Provider 配置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderConfigurationSaveRequest {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    provider_id: String,
    /// Editable field values keyed by provider field key.
    /// 按 Provider 字段键保存的可编辑字段值。
    fields: BTreeMap<String, String>,
}

/// Provider enabled-model save request sent by the WebUI settings page.
/// WebUI 设置页发送的 Provider 启用模型保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderEnabledModelsSaveRequest {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    provider_id: String,
    /// Full enabled model identifier list.
    /// 完整启用模型标识列表。
    enabled_model_ids: Vec<String>,
}

/// Provider model capability override request sent by the WebUI settings page.
/// WebUI 设置页发送的 Provider 模型能力覆盖请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderModelOverrideSaveRequest {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    model_id: String,
    /// Optional reasoning capability override.
    /// 可选推理能力覆盖值。
    reasoning: Option<bool>,
    /// Optional attachment marker override.
    /// 可选附件标记覆盖值。
    attachment: Option<bool>,
    /// Optional context-window limit override.
    /// 可选上下文窗口上限覆盖值。
    context_limit: Option<usize>,
    /// Optional input-token limit override.
    /// 可选输入 Token 上限覆盖值。
    input_limit: Option<usize>,
    /// Optional output-token limit override.
    /// 可选输出 Token 上限覆盖值。
    output_limit: Option<usize>,
    /// Optional input-modality override list.
    /// 可选输入模态覆盖列表。
    input_modalities: Option<Vec<String>>,
    /// Optional output-modality override list.
    /// 可选输出模态覆盖列表。
    output_modalities: Option<Vec<String>>,
    /// Optional reasoning profile JSON override.
    /// 可选推理档位配置 JSON 覆盖值。
    reasoning_profile_json: Option<String>,
}

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
    /// Maximum retry count, where zero means retry until cancellation.
    /// 最大重试次数；零表示持续重试直至取消。
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

/// Saves Provider retry settings from the WebUI settings page.
/// 从 WebUI 设置页保存 Provider 重试设置。
pub(super) async fn service_save_provider_retry_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderRetrySettingsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    validate_provider_retry_settings_request(&request)?;
    // persistedRetry converts validated fixed-point form values into the shared configuration shape.
    // persistedRetry 将已校验的定点表单值转换为共享配置结构。
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
            max_retries: request.rate_limit.max_retries,
            initial_delay_ms: request.rate_limit.initial_delay_ms,
            max_delay_ms: request.rate_limit.max_delay_ms,
            backoff_factor: request.rate_limit.backoff_factor_milli as f64 / 1_000.0,
        },
    };
    let workbench = mutate_provider_configuration(
        &state,
        "保存 Provider 重试设置失败",
        |configuration| {
            configuration.retry = persisted_retry;
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Builds the browser-facing retry settings summary from shared Provider configuration.
/// 从共享 Provider 配置构建浏览器侧重试设置摘要。
pub(crate) fn provider_retry_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> ProviderRetrySettingsSummary {
    // retry contains the sole persisted source used by both runtime dispatch and settings projection.
    // retry 包含运行时分发与设置投影共同使用的唯一持久化事实源。
    let retry = &provider_configuration.retry;
    // rateLimitInitialDelayMs normalizes legacy or manually edited values before deriving the maximum.
    // rateLimitInitialDelayMs 在推导最大等待前归一化旧版或手工编辑的值。
    let rate_limit_initial_delay_ms = retry
        .rate_limit
        .initial_delay_ms
        .clamp(1, PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS);
    // rateLimitMaxDelayMs preserves the cross-field order after applying the runtime ceiling.
    // rateLimitMaxDelayMs 在应用运行时上限后保持跨字段的先后关系。
    let rate_limit_max_delay_ms = retry.rate_limit.max_delay_ms.clamp(
        rate_limit_initial_delay_ms,
        PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS,
    );
    ProviderRetrySettingsSummary {
        enabled: retry.enabled,
        max_retries: retry.max_retries.min(100),
        initial_delay_ms: retry.initial_delay_ms.max(1),
        max_delay_ms: retry.max_delay_ms.max(retry.initial_delay_ms.max(1)),
        backoff_factor_milli: normalized_backoff_factor_milli(retry.backoff_factor),
        jitter_percent: retry.jitter_percent.min(80),
        respect_retry_after: retry.respect_retry_after,
        retry_429: retry.retry_429,
        rate_limit: ProviderRateLimitRetrySettingsSummary {
            max_retries: retry.rate_limit.max_retries,
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
    if request.max_retries > 100 {
        return Err(String::from("普通重试最大次数必须位于 0 到 100 之间。"));
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
    if request.rate_limit.max_retries > PROVIDER_RATE_LIMIT_SETTINGS_MAX_RETRIES {
        return Err(format!(
            "限流重试最大次数必须位于 0 到 {PROVIDER_RATE_LIMIT_SETTINGS_MAX_RETRIES} 之间。"
        ));
    }
    validate_retry_delay_range(
        "限流重试",
        request.rate_limit.initial_delay_ms,
        request.rate_limit.max_delay_ms,
        Some(PROVIDER_RETRY_SETTINGS_MAX_DELAY_MS),
    )?;
    validate_backoff_factor_milli("限流重试", request.rate_limit.backoff_factor_milli)?;
    Ok(())
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

/// Saves one model provider configuration from the WebUI settings page.
/// 从 WebUI 设置页保存单个模型供应商配置。
pub(super) async fn service_save_provider_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderConfigurationSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = request.provider_id.trim().to_ascii_lowercase();
    let definition = provider_config::supported_provider_by_id(&provider_id)
        .ok_or_else(|| format!("未找到供应商 `{provider_id}` 的目录定义。"))?;
    let mut options = PersistedProviderOptions::default();
    for field in definition.input_fields() {
        let value = request
            .fields
            .get(provider_field_request_key(field))
            .or_else(|| request.fields.get(provider_field_legacy_key(field)))
            .cloned()
            .unwrap_or_default();
        if value.trim().is_empty() {
            return Err(format!("请先填写 `{}`。", definition.field_label(field)));
        }
        options.set(field, value);
    }

    let workbench = mutate_provider_configuration(
        &state,
        "保存供应商配置失败",
        |configuration| {
            let is_new = !configuration.provider.contains_key(&provider_id);
            let entry = configuration
                .provider
                .entry(provider_id.clone())
                .or_insert_with(PersistedProviderEntry::default);
            entry.name = Some(definition.name.clone());
            entry.options = options;
            if is_new {
                entry.set_enabled_models(Vec::<String>::new());
            }
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Removes one model provider configuration from the WebUI settings page.
/// 从 WebUI 设置页移除单个模型供应商配置。
pub(super) async fn service_remove_provider_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = provider_id.trim().to_ascii_lowercase();
    let workbench = mutate_provider_configuration(
        &state,
        "移除供应商配置失败",
        |configuration| {
            configuration.provider.remove(&provider_id);
            if configuration
                .model
                .as_deref()
                .and_then(provider_config::parse_active_model_reference)
                .is_some_and(|(active_provider_id, _)| active_provider_id == provider_id)
            {
                configuration.model = None;
            }
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Saves the enabled model list for one provider from the WebUI settings page.
/// 从 WebUI 设置页保存单个供应商的启用模型列表。
pub(super) async fn service_save_provider_enabled_models(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderEnabledModelsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = request.provider_id.trim().to_ascii_lowercase();
    let workbench = mutate_provider_configuration(
        &state,
        "保存启用模型失败",
        |configuration| {
            let catalog_ids = provider_config::catalog_model_choices(&provider_id, configuration)
                .into_iter()
                .map(|model| model.id)
                .collect::<BTreeSet<_>>();
            let next_ids = normalize_provider_enabled_model_ids_for_save(
                &provider_id,
                request.enabled_model_ids,
                &catalog_ids,
            )?;
            let entry = configuration
                .provider
                .get_mut(&provider_id)
                .ok_or_else(|| format!("供应商 `{provider_id}` 尚未保存，无法配置模型。"))?;
            entry.set_enabled_models(next_ids);
            reconcile_active_model_after_enabled_models_change(configuration, &provider_id);
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Saves one model capability override for a provider from the WebUI settings page.
/// 从 WebUI 设置页保存单个供应商模型能力覆盖。
pub(super) async fn service_save_provider_model_override(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ProviderModelOverrideSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = request.provider_id.trim().to_ascii_lowercase();
    let model_id = request.model_id.trim().to_string();
    if provider_id.is_empty() || model_id.is_empty() {
        return Err(String::from("供应商与模型标识不能为空。"));
    }
    let reasoning_profile = match request.reasoning_profile_json.as_deref().map(str::trim) {
        Some("") | None => None,
        Some(raw_json) => Some(
            serde_json::from_str::<ReasoningProfileConfig>(raw_json)
                .map_err(|error| format!("推理模式附加配置 JSON 无效：{error}"))?,
        ),
    };
    let workbench = mutate_provider_configuration(
        &state,
        "保存模型能力失败",
        |configuration| {
            let catalog_ids = provider_config::catalog_model_choices(&provider_id, configuration)
                .into_iter()
                .map(|model| model.id)
                .collect::<BTreeSet<_>>();
            if !catalog_ids.contains(&model_id) {
                return Err(format!("模型 `{model_id}` 不属于供应商 `{provider_id}`。"));
            }
            let entry = configuration
                .provider
                .get_mut(&provider_id)
                .ok_or_else(|| format!("供应商 `{provider_id}` 尚未保存，无法编辑模型能力。"))?;
            let override_entry = PersistedModelOverrideEntry {
                reasoning: request.reasoning,
                attachment: request.attachment,
                context_limit: request.context_limit,
                input_limit: request.input_limit,
                output_limit: request.output_limit,
                reasoning_profile,
                input_modalities: request
                    .input_modalities
                    .map(normalize_model_override_text_list)
                    .filter(|items| !items.is_empty()),
                output_modalities: request
                    .output_modalities
                    .map(normalize_model_override_text_list)
                    .filter(|items| !items.is_empty()),
            };
            if override_entry.is_empty() {
                entry.model_overrides.remove(&model_id);
            } else {
                entry.model_overrides.insert(model_id, override_entry);
            }
            Ok(())
        },
        |guard, configuration| {
            invalidate_runtime_prompt_tool_visibility_cache();
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Reconciles the active model after one provider enabled-model list changed.
/// 在单个供应商启用模型列表变化后对齐当前激活模型。
fn reconcile_active_model_after_enabled_models_change(
    configuration: &mut ProviderConfigurationFile,
    provider_id: &str,
) {
    let active_matches_provider = configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .is_some_and(|(active_provider_id, _)| active_provider_id == provider_id);
    if !active_matches_provider {
        return;
    }
    let active_model_id = configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .map(|(_, model_id)| model_id.to_string());
    let next_models = provider_config::effective_model_choices(provider_id, configuration);
    if let Some(active_model_id) = active_model_id
        && next_models.iter().any(|model| model.id == active_model_id)
    {
        return;
    }
    configuration.model = next_models
        .first()
        .map(|model| format!("{provider_id}/{}", model.id));
}

/// Normalizes one enabled-model save request and rejects non-empty model ids missing from the catalog.
/// 归一化单次启用模型保存请求，并拒绝目录中不存在的非空模型标识。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the provider being edited.
/// `provider_id` 标识正在编辑的供应商。
/// `requested_model_ids` contains the raw WebUI/RPC model identifiers.
/// `requested_model_ids` 包含 WebUI/RPC 传入的原始模型标识。
/// `catalog_ids` contains every model currently known for the provider.
/// `catalog_ids` 包含当前供应商目录中已知的全部模型。
///
/// # Returns
/// 返回值
/// A stable deduplicated model list, or an error listing the rejected identifiers.
/// 返回稳定去重后的模型列表；存在被拒绝标识时返回错误。
pub(crate) fn normalize_provider_enabled_model_ids_for_save(
    provider_id: &str,
    requested_model_ids: Vec<String>,
    catalog_ids: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let mut accepted = BTreeSet::new();
    let mut rejected = BTreeSet::new();
    for model_id in requested_model_ids {
        let normalized = model_id.trim().to_string();
        if normalized.is_empty() {
            continue;
        }
        if catalog_ids.contains(&normalized) {
            accepted.insert(normalized);
        } else {
            rejected.insert(normalized);
        }
    }
    if !rejected.is_empty() {
        let rejected = rejected.into_iter().collect::<Vec<_>>().join("、");
        return Err(format!(
            "供应商 `{provider_id}` 的启用模型列表包含未知模型：{rejected}。"
        ));
    }
    Ok(accepted.into_iter().collect())
}

/// Returns a legacy-compatible request key for one provider field.
/// 返回单个 Provider 字段的旧版兼容请求键。
fn provider_field_legacy_key(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "api_key",
        ProviderFieldKey::BaseUrl => "base_url",
        ProviderFieldKey::ResourceName => "resource_name",
        ProviderFieldKey::AccountId => "account_id",
    }
}

/// Normalizes a user-authored string list used by model override fields.
/// 归一化模型覆盖字段使用的用户字符串列表。
fn normalize_model_override_text_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
                max_retries: 0,
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
        // configuration contains one fixed-interval finite rate-limit policy.
        // configuration 包含一份固定间隔且次数有限的限流策略。
        let mut configuration = ProviderConfigurationFile::default();
        configuration.retry.rate_limit = PersistedProviderRateLimitRetryConfiguration {
            max_retries: 12,
            initial_delay_ms: 9_000,
            max_delay_ms: 60_000,
            backoff_factor: 1.0,
        };

        // summary is the browser-facing value used to initialize the settings form.
        // summary 是用于初始化设置表单的浏览器侧值。
        let summary = provider_retry_settings(&configuration);
        assert_eq!(summary.rate_limit.max_retries, 12);
        assert_eq!(summary.rate_limit.initial_delay_ms, 9_000);
        assert_eq!(summary.rate_limit.max_delay_ms, 60_000);
        assert_eq!(summary.rate_limit.backoff_factor_milli, 1_000);
    }

    /// Verifies validation accepts fixed unbounded retries and rejects an inverted delay range.
    /// 验证校验会接受固定间隔的持续重试，并拒绝倒置的等待范围。
    #[test]
    fn retry_settings_validation_enforces_cross_field_delay_order() {
        // validRequest uses maxRetries zero and factor one to select fixed cancellation-bound retries.
        // validRequest 使用最大次数零与倍数一选择固定间隔、仅由取消终止的重试。
        let mut valid_request = valid_retry_request();
        valid_request.rate_limit.backoff_factor_milli = 1_000;
        assert!(validate_provider_retry_settings_request(&valid_request).is_ok());

        // invalidRequest puts the maximum wait below the initial wait.
        // invalidRequest 将最大等待设置为小于首次等待。
        let mut invalid_request = valid_request;
        invalid_request.rate_limit.max_delay_ms = 4_999;
        assert!(validate_provider_retry_settings_request(&invalid_request).is_err());
    }

    /// Verifies only rate-limit retries apply the thirty-minute hard delay ceiling.
    /// 验证只有限流重试会应用三十分钟的硬等待上限。
    #[test]
    fn retry_settings_validation_limits_only_rate_limit_delays() {
        // ordinaryRequest retains the existing unrestricted ordinary-retry delay semantics.
        // ordinaryRequest 保留普通重试等待不受三十分钟上限约束的既有语义。
        let mut ordinary_request = valid_retry_request();
        ordinary_request.initial_delay_ms = 1_800_001;
        ordinary_request.max_delay_ms = 3_600_000;
        assert!(validate_provider_retry_settings_request(&ordinary_request).is_ok());

        // rateLimitRequest exceeds the runtime's documented thirty-minute ceiling.
        // rateLimitRequest 超出运行时约定的三十分钟限流等待上限。
        let mut rate_limit_request = valid_retry_request();
        rate_limit_request.rate_limit.max_delay_ms = 1_800_001;
        assert!(validate_provider_retry_settings_request(&rate_limit_request).is_err());
    }
}

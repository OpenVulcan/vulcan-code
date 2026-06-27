//! Provider configuration save services for the WebUI settings surface.
//! WebUI 设置界面的供应商配置保存服务。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use vulcan_agent_runtime::{
    agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_tool_visibility_cache,
    provider_config::{
        self, PersistedModelOverrideEntry, ProviderConfigurationFile, ReasoningProfileConfig,
    },
};
use vulcan_provider_config::config::{
    PersistedProviderEntry, PersistedProviderOptions, ProviderFieldKey,
};
use vulcan_service_api::{ServiceEventEmitter, WorkbenchSnapshot};

use crate::{
    SharedServiceBridgeState, emit_workbench_event, provider_field_request_key,
    refresh_settings_projection_locked,
};

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

    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        let is_new = !next_configuration.provider.contains_key(&provider_id);
        let entry = next_configuration
            .provider
            .entry(provider_id.clone())
            .or_insert_with(PersistedProviderEntry::default);
        entry.name = Some(definition.name.clone());
        entry.options = options;
        if is_new {
            entry.set_enabled_models(Vec::<String>::new());
        }
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存供应商配置失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };
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
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.provider.remove(&provider_id);
        if next_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .is_some_and(|(active_provider_id, _)| active_provider_id == provider_id)
        {
            next_configuration.model = None;
        }
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("移除供应商配置失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };
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
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        let catalog_ids = provider_config::catalog_model_choices(&provider_id, &next_configuration)
            .into_iter()
            .map(|model| model.id)
            .collect::<BTreeSet<_>>();
        let next_ids = request
            .enabled_model_ids
            .into_iter()
            .map(|model_id| model_id.trim().to_string())
            .filter(|model_id| !model_id.is_empty() && catalog_ids.contains(model_id))
            .collect::<Vec<_>>();
        let entry = next_configuration
            .provider
            .get_mut(&provider_id)
            .ok_or_else(|| format!("供应商 `{provider_id}` 尚未保存，无法配置模型。"))?;
        entry.set_enabled_models(next_ids);
        reconcile_active_model_after_enabled_models_change(&mut next_configuration, &provider_id);
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存启用模型失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };
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
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        let catalog_ids = provider_config::catalog_model_choices(&provider_id, &next_configuration)
            .into_iter()
            .map(|model| model.id)
            .collect::<BTreeSet<_>>();
        if !catalog_ids.contains(&model_id) {
            return Err(format!("模型 `{model_id}` 不属于供应商 `{provider_id}`。"));
        }
        let entry = next_configuration
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
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存模型能力失败：{error}"))?;
        invalidate_runtime_prompt_tool_visibility_cache();
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };
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

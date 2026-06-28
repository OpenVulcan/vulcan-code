//! AI capability services helpers.
//! AI 能力 services 辅助逻辑。

use super::*;

/// Shared AI capability vendor save request sent by the WebUI settings page.
/// WebUI 设置页发送的共享 AI 能力供应商保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCapabilityVendorSaveRequest {
    /// Stable vendor catalog identifier.
    /// 稳定供应商目录标识。
    vendor_id: String,
    /// Whether the vendor participates in runtime routing.
    /// 该供应商是否参与运行时路由。
    enabled: bool,
    /// Shared API key or environment placeholder.
    /// 共享 API Key 或环境变量占位符。
    api_key: String,
    /// Shared base URL override.
    /// 共享 Base URL 覆盖值。
    base_url: String,
    /// Shared region override.
    /// 共享区域覆盖值。
    region: String,
    /// Optional connection mode key.
    /// 可选连接模式键。
    connection_mode: Option<String>,
}

/// AI capability target enablement request sent by the WebUI settings page.
/// WebUI 设置页发送的 AI 能力目标启用状态请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCapabilityTargetEnabledRequest {
    /// Stable semantic capability key.
    /// 稳定语义能力键。
    capability_kind: String,
    /// Stable vendor identifier.
    /// 稳定供应商标识。
    vendor_id: String,
    /// Stable target alias under the vendor.
    /// 供应商下的稳定目标别名。
    target_alias: String,
    /// Next enabled state.
    /// 下一启用状态。
    enabled: bool,
}

/// AI capability target-list save request sent by the WebUI settings page.
/// WebUI 设置页发送的 AI 能力目标列表保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCapabilityTargetsSaveRequest {
    /// Stable semantic capability kind.
    /// 稳定语义能力类型。
    capability_kind: String,
    /// Ordered target bindings to keep for this capability.
    /// 当前能力需要保留的有序目标绑定集合。
    targets: Vec<AiCapabilityTargetSaveInput>,
}

/// AI capability target binding submitted by the WebUI settings page.
/// WebUI 设置页提交的 AI 能力目标绑定。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiCapabilityTargetSaveInput {
    /// Stable vendor identifier.
    /// 稳定供应商标识。
    vendor_id: String,
    /// Stable target alias under the vendor.
    /// 供应商下的稳定目标别名。
    target_alias: String,
    /// Provider request model id.
    /// Provider 请求模型标识。
    model_id: String,
    /// Whether the binding is enabled.
    /// 该绑定是否启用。
    enabled: bool,
}

/// Saves one shared AI capability vendor from the WebUI settings page.
/// 从 WebUI 设置页保存单个共享 AI 能力供应商。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to the WebUI.
/// `app` 用于向 WebUI 发送刷新的工作台快照。
/// `state` owns the current service bridge runtime and settings snapshot.
/// `state` 持有当前服务桥接运行态与设置快照。
/// `request` contains the vendor fields submitted by the settings page.
/// `request` 包含设置页提交的供应商字段。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the vendor configuration has been persisted.
/// 供应商配置持久化后的刷新工作台快照。
pub(crate) async fn service_save_ai_capability_vendor(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: AiCapabilityVendorSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let vendor_id = request.vendor_id.trim().to_ascii_lowercase();
    if vendor_id.is_empty() {
        return Err(String::from("AI 能力供应商标识不能为空。"));
    }
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_ai_tool_configuration = agent_runtime.ai_tool_configuration.clone();
        if next_ai_tool_configuration
            .vendor_by_id(&vendor_id)
            .is_none()
        {
            next_ai_tool_configuration
                .vendors
                .push(PersistedAiCapabilityVendorEntry::new(vendor_id.clone()));
        }
        let vendor = next_ai_tool_configuration
            .vendor_by_id_mut(&vendor_id)
            .ok_or_else(|| format!("无法创建 AI 能力供应商 `{vendor_id}`。"))?;
        vendor.name = provider_config::ai_capability_vendor_display_name(&vendor_id);
        vendor.enabled = request.enabled;
        let requested_api_key = request.api_key.trim();
        if requested_api_key != WEBUI_REDACTED_SECRET_PLACEHOLDER {
            vendor.api_key = requested_api_key.to_string();
        }
        vendor.base_url = request.base_url.trim().to_string();
        vendor.region = request.region.trim().to_string();
        vendor.connection_mode =
            parse_ai_capability_connection_mode(request.connection_mode.as_deref())?;
        next_ai_tool_configuration.normalize();
        vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
            &next_ai_tool_configuration,
        )
        .map_err(|error| format!("保存 AI 能力供应商失败：{error}"))?;
        invalidate_runtime_prompt_tool_visibility_cache();
        agent_runtime.ai_tool_configuration = next_ai_tool_configuration.clone();
        let provider_configuration = agent_runtime.provider_configuration.clone();
        refresh_settings_projection_locked(
            &mut guard,
            &provider_configuration,
            Some(&next_ai_tool_configuration),
        );
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Removes one shared AI capability vendor from the WebUI settings page.
/// 从 WebUI 设置页移除单个共享 AI 能力供应商。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to the WebUI.
/// `app` 用于向 WebUI 发送刷新的工作台快照。
/// `state` owns the current service bridge runtime and settings snapshot.
/// `state` 持有当前服务桥接运行态与设置快照。
/// `vendor_id` is the stable vendor identifier selected for removal.
/// `vendor_id` 是被选择移除的稳定供应商标识。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the vendor list has been persisted.
/// 供应商列表持久化后的刷新工作台快照。
pub(crate) async fn service_remove_ai_capability_vendor(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    vendor_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let vendor_id = vendor_id.trim().to_ascii_lowercase();
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_ai_tool_configuration = agent_runtime.ai_tool_configuration.clone();
        next_ai_tool_configuration
            .vendors
            .retain(|vendor| vendor.vendor_id != vendor_id);
        next_ai_tool_configuration.normalize();
        vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
            &next_ai_tool_configuration,
        )
        .map_err(|error| format!("移除 AI 能力供应商失败：{error}"))?;
        invalidate_runtime_prompt_tool_visibility_cache();
        agent_runtime.ai_tool_configuration = next_ai_tool_configuration.clone();
        let provider_configuration = agent_runtime.provider_configuration.clone();
        refresh_settings_projection_locked(
            &mut guard,
            &provider_configuration,
            Some(&next_ai_tool_configuration),
        );
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Toggles one configured AI capability target from the WebUI settings page.
/// 从 WebUI 设置页切换单个已配置 AI 能力目标。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to the WebUI.
/// `app` 用于向 WebUI 发送刷新的工作台快照。
/// `state` owns the current service bridge runtime and settings snapshot.
/// `state` 持有当前服务桥接运行态与设置快照。
/// `request` identifies the capability target and the next enabled state.
/// `request` 标识能力目标及下一启用状态。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the target state has been persisted.
/// 目标状态持久化后的刷新工作台快照。
pub(crate) async fn service_set_ai_capability_target_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: AiCapabilityTargetEnabledRequest,
) -> Result<WorkbenchSnapshot, String> {
    let capability_kind = parse_ai_tool_kind(&request.capability_kind)?;
    let vendor_id = request.vendor_id.trim().to_ascii_lowercase();
    let target_alias = request.target_alias.trim().to_ascii_lowercase();
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_ai_tool_configuration = agent_runtime.ai_tool_configuration.clone();
        let vendor = next_ai_tool_configuration
            .vendor_by_id_mut(&vendor_id)
            .ok_or_else(|| format!("未找到 AI 能力供应商 `{vendor_id}`。"))?;
        let binding = vendor
            .targets
            .iter_mut()
            .find(|binding| {
                binding.capability_kind == capability_kind
                    && binding.target_alias().eq_ignore_ascii_case(&target_alias)
            })
            .ok_or_else(|| format!("未找到 AI 能力目标 `{vendor_id}/{target_alias}`。"))?;
        binding.enabled = request.enabled;
        next_ai_tool_configuration.normalize();
        vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
            &next_ai_tool_configuration,
        )
        .map_err(|error| format!("保存 AI 能力目标失败：{error}"))?;
        invalidate_runtime_prompt_tool_visibility_cache();
        agent_runtime.ai_tool_configuration = next_ai_tool_configuration.clone();
        let provider_configuration = agent_runtime.provider_configuration.clone();
        refresh_settings_projection_locked(
            &mut guard,
            &provider_configuration,
            Some(&next_ai_tool_configuration),
        );
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Saves the full ordered target list for one AI capability from the WebUI settings page.
/// 从 WebUI 设置页保存单个 AI 能力的完整有序目标列表。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to the WebUI.
/// `app` 用于向 WebUI 发送刷新的工作台快照。
/// `state` owns the current service bridge runtime and settings snapshot.
/// `state` 持有当前服务桥接运行态与设置快照。
/// `request` contains the capability kind and ordered target bindings to persist.
/// `request` 包含要持久化的能力类型与有序目标绑定。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the target bindings have been persisted.
/// 目标绑定持久化后的刷新工作台快照。
pub(crate) async fn service_save_ai_capability_targets(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: AiCapabilityTargetsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let capability_kind = parse_ai_tool_kind(&request.capability_kind)?;
    let mut target_inputs = Vec::new();
    let mut ordered_selectors = Vec::new();
    for target in request.targets {
        let vendor_id = target.vendor_id.trim().to_ascii_lowercase();
        let target_alias = target.target_alias.trim().to_ascii_lowercase();
        if vendor_id.is_empty() || target_alias.is_empty() {
            continue;
        }
        let choice = provider_config::ai_capability_vendor_target_choices(&vendor_id)
            .into_iter()
            .find(|choice| {
                choice.capability_kind == capability_kind
                    && choice.target_alias.eq_ignore_ascii_case(&target_alias)
            })
            .ok_or_else(|| format!("供应商 `{vendor_id}` 不支持目标 `{target_alias}`。"))?;
        let model_id = if target.model_id.trim().is_empty() {
            choice.default_model_id
        } else {
            target.model_id.trim().to_string()
        };
        ordered_selectors.push(format!("{vendor_id}/{target_alias}"));
        target_inputs.push((vendor_id, target_alias, model_id, target.enabled));
    }
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_ai_tool_configuration = agent_runtime.ai_tool_configuration.clone();
        for vendor in &mut next_ai_tool_configuration.vendors {
            vendor
                .targets
                .retain(|binding| binding.capability_kind != capability_kind);
        }
        let vendor_ids = target_inputs
            .iter()
            .map(|(vendor_id, _, _, _)| vendor_id.clone())
            .collect::<BTreeSet<_>>();
        for vendor_id in vendor_ids {
            let vendor = next_ai_tool_configuration
                .vendor_by_id_mut(&vendor_id)
                .ok_or_else(|| format!("未找到 AI 能力供应商 `{vendor_id}`。"))?;
            for (_, target_alias, model_id, enabled) in target_inputs
                .iter()
                .filter(|(input_vendor_id, _, _, _)| input_vendor_id == &vendor_id)
            {
                let mut binding = PersistedAiCapabilityTargetBinding::new(
                    capability_kind,
                    target_alias.clone(),
                    model_id.clone(),
                );
                binding.enabled = *enabled;
                vendor.targets.push(binding);
            }
        }
        set_ai_capability_target_order_for_kind(
            &mut next_ai_tool_configuration,
            capability_kind,
            ordered_selectors,
        )?;
        next_ai_tool_configuration.normalize();
        vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
            &next_ai_tool_configuration,
        )
        .map_err(|error| format!("保存 AI 能力目标列表失败：{error}"))?;
        invalidate_runtime_prompt_tool_visibility_cache();
        agent_runtime.ai_tool_configuration = next_ai_tool_configuration.clone();
        let provider_configuration = agent_runtime.provider_configuration.clone();
        refresh_settings_projection_locked(
            &mut guard,
            &provider_configuration,
            Some(&next_ai_tool_configuration),
        );
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

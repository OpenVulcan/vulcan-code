//! Shared AI capability vendor and target settings services.
//! 共享 AI 能力供应商与目标设置服务。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use vulcan_agent_runtime::{
    agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_tool_visibility_cache,
    provider_capabilities::{ResolvedModelCapabilities, resolve_effective_model_capabilities},
    provider_config,
    workbench_ai_capability_router::WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS,
};
use vulcan_provider_config::config::{
    AiCapabilityVendorConnectionMode, AiToolKind, PersistedAiCapabilityTargetBinding,
    PersistedAiCapabilityVendorEntry, PersistedAiToolConfiguration, ProviderConfigurationFile,
};
use vulcan_service_api::{
    AiCapabilitySummary, AiCapabilityTargetSummary, AiCapabilityVendorCatalogSummary,
    AiCapabilityVendorSummary, AiCapabilityVendorTargetChoiceSummary, ServiceEventEmitter,
    WEBUI_REDACTED_SECRET_PLACEHOLDER, WorkbenchSnapshot,
};

use crate::{
    SharedServiceBridgeState, emit_workbench_event, parse_ai_tool_kind,
    refresh_settings_projection_locked,
};

/// Shared AI capability vendor save request sent by the WebUI settings page.
/// WebUI 设置页发送的共享 AI 能力供应商保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AiCapabilityVendorSaveRequest {
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
pub(super) struct AiCapabilityTargetEnabledRequest {
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
pub(super) struct AiCapabilityTargetsSaveRequest {
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
pub(super) async fn service_save_ai_capability_vendor(
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
pub(super) async fn service_remove_ai_capability_vendor(
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
pub(super) async fn service_set_ai_capability_target_enabled(
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
pub(super) async fn service_save_ai_capability_targets(
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

/// Applies a complete selector order for one AI capability kind.
/// 为单个 AI 能力类型应用完整目标选择器顺序。
///
/// # Parameters
/// 参数
/// `configuration` is the mutable persisted AI tool configuration.
/// `configuration` 是可变的持久化 AI 工具配置。
/// `capability_kind` selects which capability-specific order list should be replaced.
/// `capability_kind` 选择要替换的能力专属排序列表。
/// `ordered_selectors` is the complete selector order supplied by the caller.
/// `ordered_selectors` 是调用方提供的完整选择器顺序。
///
/// # Returns
/// 返回值
/// `Ok(())` after the capability order is replaced, or an error for unsupported kinds.
/// 能力顺序替换后返回 `Ok(())`；不支持的能力类型返回错误。
pub(super) fn set_ai_capability_target_order_for_kind(
    configuration: &mut PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    ordered_selectors: Vec<String>,
) -> Result<(), String> {
    let target_order = match capability_kind {
        AiToolKind::WebSearch => &mut configuration.web_search.target_order,
        AiToolKind::UnderstandImage => &mut configuration.understand_image.target_order,
        AiToolKind::ImageGeneration => &mut configuration.image_generation.target_order,
        AiToolKind::VideoGeneration => &mut configuration.video_generation.target_order,
        AiToolKind::SpeechSynthesis => &mut configuration.speech_synthesis.target_order,
        AiToolKind::MusicGeneration => &mut configuration.music_generation.target_order,
        AiToolKind::MusicCover => &mut configuration.music_cover.target_order,
        _ => {
            return Err(format!(
                "AI 能力 `{}` 不支持目标排序。",
                capability_kind.key()
            ));
        }
    };
    *target_order = ordered_selectors;
    Ok(())
}

/// Parses one AI capability vendor connection mode key.
/// 解析单个 AI 能力供应商连接模式键。
///
/// # Parameters
/// 参数
/// `value` is the optional raw connection-mode key from the WebUI payload.
/// `value` 是 WebUI 载荷中的可选原始连接模式键。
///
/// # Returns
/// 返回值
/// The parsed connection mode, or `None` when no explicit mode was submitted.
/// 解析后的连接模式；未提交显式模式时返回 `None`。
fn parse_ai_capability_connection_mode(
    value: Option<&str>,
) -> Result<Option<AiCapabilityVendorConnectionMode>, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some("local") => Ok(Some(AiCapabilityVendorConnectionMode::Local)),
        Some("cloud") | Some("ollama-cloud") => Ok(Some(AiCapabilityVendorConnectionMode::Cloud)),
        Some(other) => Err(format!("不支持的 AI 能力连接模式：{other}")),
    }
}

/// Parses prompt-level AI capability target order overrides from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标顺序覆盖。
///
/// # Parameters
/// 参数
/// `raw_overrides` contains capability-keyed target selector arrays from the WebUI.
/// `raw_overrides` 包含 WebUI 传入的按能力键分组的目标选择器数组。
///
/// # Returns
/// 返回值
/// A semantic capability keyed map accepted by prompt and session capability routing.
/// 提示词与会话能力路由可接受的语义能力映射。
pub(super) fn parse_ai_capability_target_order_overrides(
    raw_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<BTreeMap<AiToolKind, Vec<String>>, String> {
    let mut overrides = BTreeMap::new();
    for (raw_kind, selectors) in raw_overrides.unwrap_or_default() {
        let Some(kind) =
            vulcan_agent_runtime::workbench_ai_capability_router::parse_workbench_ai_tool_kind(
                &raw_kind,
            )
        else {
            return Err(format!("未知的扩展能力排序类型：{raw_kind}"));
        };
        let cleaned_selectors = selectors
            .into_iter()
            .map(|selector| selector.trim().to_string())
            .filter(|selector| !selector.is_empty())
            .collect::<Vec<_>>();
        if !cleaned_selectors.is_empty() {
            overrides.insert(kind, cleaned_selectors);
        }
    }
    Ok(overrides)
}

/// Parses prompt-level AI capability target selectors from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标选择器。
///
/// # Parameters
/// 参数
/// `raw_selectors` contains optional target selector strings from the WebUI.
/// `raw_selectors` 包含 WebUI 传入的可选目标选择器字符串。
///
/// # Returns
/// 返回值
/// A trimmed selector set accepted by prompt and session capability routing.
/// 提示词与会话能力路由可接受的已清洗选择器集合。
pub(super) fn parse_ai_capability_target_selectors(
    raw_selectors: Option<Vec<String>>,
) -> BTreeSet<String> {
    raw_selectors
        .unwrap_or_default()
        .into_iter()
        .map(|selector| selector.trim().to_string())
        .filter(|selector| !selector.is_empty())
        .collect()
}

/// Creates composer AI capability summaries from the shared provider configuration.
/// 从共享 Provider 配置创建输入区 AI 能力摘要。
///
/// # Parameters
/// 参数
/// `configuration` is the persisted AI tool configuration.
/// `configuration` 是持久化的 AI 工具配置。
/// `model_capabilities` describes the active model's provider-native capabilities when available.
/// `model_capabilities` 描述可用时当前模型的 Provider 原生能力。
///
/// # Returns
/// 返回值
/// Ordered AI capability summaries matching the WebUI workbench quick-toggle surface.
/// 与 WebUI 工作台快捷开关界面顺序一致的 AI 能力摘要。
pub(super) fn ai_capabilities(
    configuration: &PersistedAiToolConfiguration,
    model_capabilities: Option<&ResolvedModelCapabilities>,
) -> Vec<AiCapabilitySummary> {
    WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS
        .into_iter()
        .map(|kind| {
            let targets = configuration
                .resolved_configured_targets_for_kind(kind)
                .into_iter()
                .map(|target| AiCapabilityTargetSummary {
                    target_selector: target.target_selector,
                    vendor_id: target.vendor_id,
                    vendor_name: target.vendor_name,
                    target_alias: target.target_alias,
                    target_name: target.target_name,
                    model_id: target.model_id,
                    enabled: target.enabled,
                })
                .collect::<Vec<_>>();
            let native = ai_capability_has_native_path(kind, model_capabilities);
            let bridged = ai_capability_has_active_bridge_path(kind, configuration, native);
            let available = native || bridged;
            let enabled =
                native || (bridged && !configuration.disabled_capability_kinds.contains(&kind));
            AiCapabilitySummary {
                kind: kind.key().to_string(),
                label: kind.label().to_string(),
                available,
                enabled,
                native,
                bridged,
                targets,
            }
        })
        .collect()
}

/// Returns whether one semantic AI capability should expose an active host bridge for the current model.
/// 返回指定语义 AI 能力在当前模型下是否应暴露有效宿主桥接路径。
///
/// # Parameters
/// 参数
/// `kind` is the semantic AI capability being evaluated.
/// `kind` 是正在评估的语义 AI 能力。
/// `configuration` contains the persisted bridge targets.
/// `configuration` 包含持久化的桥接目标。
/// `native_available` indicates whether the active model already provides a native path.
/// `native_available` 表示当前模型是否已经提供原生路径。
///
/// # Returns
/// 返回值
/// `true` when a configured host bridge should be exposed for the capability.
/// 当该能力应暴露已配置宿主桥接路径时返回 `true`。
fn ai_capability_has_active_bridge_path(
    kind: AiToolKind,
    configuration: &PersistedAiToolConfiguration,
    native_available: bool,
) -> bool {
    configuration.has_usable_targets_for_kind(kind) && !native_available
}

/// Returns whether one semantic AI capability has a provider-native path for the active model snapshot.
/// 返回指定语义 AI 能力在当前模型快照中是否存在 Provider 原生路径。
///
/// # Parameters
/// 参数
/// `kind` is the semantic AI capability being evaluated.
/// `kind` 是正在评估的语义 AI 能力。
/// `model_capabilities` describes the active model's resolved capabilities when available.
/// `model_capabilities` 描述可用时当前模型的已解析能力。
///
/// # Returns
/// 返回值
/// `true` when the active model can serve the capability without a host bridge.
/// 当前模型无需宿主桥接即可提供该能力时返回 `true`。
pub(super) fn ai_capability_has_native_path(
    kind: AiToolKind,
    model_capabilities: Option<&ResolvedModelCapabilities>,
) -> bool {
    let Some(model_capabilities) = model_capabilities else {
        return false;
    };
    if kind == AiToolKind::UnderstandImage {
        return model_capabilities.supports_input_modality("image");
    }
    model_capabilities.ai_tools.iter().any(|tool| {
        tool.kind == kind
            && tool.native_provider_tool
            && tool.policy != vulcan_provider_config::config::AiToolPolicy::Disabled
    })
}

/// Creates shared AI capability vendor summaries from the standalone AI tool configuration.
/// 从独立 AI 工具配置创建共享 AI 能力供应商摘要。
///
/// # Parameters
/// 参数
/// `configuration` is the persisted AI tool configuration.
/// `configuration` 是持久化的 AI 工具配置。
///
/// # Returns
/// 返回值
/// Vendor summaries rendered by the WebUI settings page.
/// WebUI 设置页渲染使用的供应商摘要。
pub(super) fn ai_capability_vendors(
    configuration: &PersistedAiToolConfiguration,
) -> Vec<AiCapabilityVendorSummary> {
    configuration
        .vendors
        .iter()
        .map(|vendor| AiCapabilityVendorSummary {
            target_choices: provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                .into_iter()
                .map(|choice| AiCapabilityVendorTargetChoiceSummary {
                    capability_kind: choice.capability_kind.key().to_string(),
                    target_alias: choice.target_alias,
                    default_model_id: choice.default_model_id,
                    label: choice.label,
                    supports_model_override: choice.supports_model_override,
                    model_options: choice.model_options,
                })
                .collect(),
            id: vendor.id.clone(),
            vendor_id: vendor.vendor_id.clone(),
            name: vendor.name.clone(),
            enabled: vendor.enabled,
            api_key: vendor.api_key.clone(),
            base_url: vendor.base_url.clone(),
            region: vendor.region.clone(),
            connection_mode: match vendor.resolved_connection_mode() {
                AiCapabilityVendorConnectionMode::Local => String::from("local"),
                AiCapabilityVendorConnectionMode::Cloud => String::from("cloud"),
            },
            target_count: vendor.targets.len(),
        })
        .collect()
}

/// Creates supported AI capability vendor catalog choices for the add-vendor picker.
/// 为添加供应商选择器创建受支持 AI 能力供应商目录选项。
///
/// # Parameters
/// 参数
/// This function does not take parameters; it reads the static provider capability catalog.
/// 此函数不接收参数；它读取静态 Provider 能力目录。
///
/// # Returns
/// 返回值
/// Supported vendor catalog entries available to the WebUI add-vendor picker.
/// WebUI 添加供应商选择器可用的受支持供应商目录条目。
pub(super) fn ai_capability_vendor_catalog() -> Vec<AiCapabilityVendorCatalogSummary> {
    vulcan_provider_config::ai_capability_provider_catalog::supported_ai_capability_vendor_ids()
        .into_iter()
        .map(|vendor_id| {
            let capabilities = provider_config::ai_capability_vendor_target_choices(&vendor_id)
                .into_iter()
                .map(|choice| choice.capability_kind.label().to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            AiCapabilityVendorCatalogSummary {
                name: provider_config::ai_capability_vendor_display_name(&vendor_id),
                vendor_id,
                capabilities,
            }
        })
        .collect()
}

/// Resolves the active workbench model capabilities from persisted provider settings.
/// 从持久化 Provider 设置解析当前工作台模型能力。
///
/// # Parameters
/// 参数
/// `provider_configuration` contains the active provider/model selection and model overrides.
/// `provider_configuration` 包含当前 Provider/模型选择与模型覆盖配置。
/// `ai_tool_configuration` contains the persisted AI tool bridge configuration.
/// `ai_tool_configuration` 包含持久化的 AI 工具桥接配置。
///
/// # Returns
/// 返回值
/// The resolved active model capabilities, or `None` when no active model is configured.
/// 已解析的当前模型能力；未配置当前模型时返回 `None`。
pub(super) fn active_workbench_model_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ResolvedModelCapabilities> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        ai_tool_configuration,
    ))
}

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
    /// Whether the vendor remains available for explicit selection.
    /// 该供应商是否仍可供显式选择。
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

/// Normalized identity for one AI capability target selector.
/// 单个 AI 能力目标选择器的规范化身份。
#[derive(Debug, Clone, Eq, PartialEq)]
struct NormalizedAiCapabilityTargetSelector {
    /// Lowercase stable vendor identifier.
    /// 小写的稳定供应商标识。
    vendor_id: String,
    /// Lowercase stable target alias under the vendor.
    /// 供应商下的小写稳定目标别名。
    target_alias: String,
}

impl NormalizedAiCapabilityTargetSelector {
    /// Builds a normalized target selector identity from raw WebUI fields.
    /// 根据 WebUI 原始字段构建规范化目标选择器身份。
    ///
    /// # Parameters
    /// 参数
    /// `vendor_id` is the raw vendor id from a service request.
    /// `vendor_id` 是 service 请求中的原始供应商标识。
    /// `target_alias` is the raw target alias from a service request.
    /// `target_alias` 是 service 请求中的原始目标别名。
    ///
    /// # Returns
    /// 返回值
    /// A normalized identity, or `None` when either component is blank.
    /// 返回规范化身份；任一组成部分为空白时返回 `None`。
    fn from_raw(vendor_id: &str, target_alias: &str) -> Option<Self> {
        let vendor_id = vendor_id.trim().to_ascii_lowercase();
        let target_alias = target_alias.trim().to_ascii_lowercase();
        (!vendor_id.is_empty() && !target_alias.is_empty()).then_some(Self {
            vendor_id,
            target_alias,
        })
    }

    /// Builds a required normalized target selector identity from raw WebUI fields.
    /// 根据 WebUI 原始字段构建必填的规范化目标选择器身份。
    ///
    /// # Parameters
    /// 参数
    /// `vendor_id` is the raw vendor id from a service request.
    /// `vendor_id` 是 service 请求中的原始供应商标识。
    /// `target_alias` is the raw target alias from a service request.
    /// `target_alias` 是 service 请求中的原始目标别名。
    ///
    /// # Returns
    /// 返回值
    /// A normalized identity, or an error when either component is blank.
    /// 返回规范化身份；任一组成部分为空白时返回错误。
    fn required_from_raw(vendor_id: &str, target_alias: &str) -> Result<Self, String> {
        Self::from_raw(vendor_id, target_alias)
            .ok_or_else(|| String::from("AI 能力目标标识不能为空。"))
    }

    /// Returns the normalized vendor id.
    /// 返回规范化供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The lowercase vendor id component.
    /// 小写供应商标识组成部分。
    fn vendor_id(&self) -> &str {
        &self.vendor_id
    }

    /// Returns the normalized target alias.
    /// 返回规范化目标别名。
    ///
    /// # Returns
    /// 返回值
    /// The lowercase target alias component.
    /// 小写目标别名组成部分。
    fn target_alias(&self) -> &str {
        &self.target_alias
    }

    /// Formats the normalized selector used by route ordering and diagnostics.
    /// 格式化路由排序与诊断使用的规范化选择器。
    ///
    /// # Returns
    /// 返回值
    /// The `vendor_id/target_alias` selector string.
    /// `vendor_id/target_alias` 形式的选择器字符串。
    fn selector(&self) -> String {
        format!("{}/{}", self.vendor_id, self.target_alias)
    }
}

/// Mutates, persists, and projects the shared AI capability configuration.
/// 变更、持久化并投影共享 AI 能力配置。
///
/// # Parameters
/// 参数
/// `state` owns the Agent Runtime configuration snapshot and workbench projection.
/// `state` 持有 Agent Runtime 配置快照与工作台投影。
/// `save_error_label` is the user-facing save action label used in diagnostics.
/// `save_error_label` 是诊断中使用的面向用户保存动作标签。
/// `mutate` applies the service-specific configuration changes before normalization.
/// `mutate` 在归一化前应用服务入口专属配置变更。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after persistence and settings projection refresh.
/// 返回完成持久化与设置投影刷新后的工作台快照。
fn mutate_shared_ai_capability_configuration<Mutation>(
    state: &SharedServiceBridgeState,
    save_error_label: &str,
    mutate: Mutation,
) -> Result<WorkbenchSnapshot, String>
where
    Mutation: FnOnce(&mut PersistedAiToolConfiguration) -> Result<(), String>,
{
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
    let mut next_ai_tool_configuration = agent_runtime.ai_tool_configuration.clone();
    mutate(&mut next_ai_tool_configuration)?;
    next_ai_tool_configuration.normalize();
    vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
        &next_ai_tool_configuration,
    )
    .map_err(|error| format!("{save_error_label}：{error}"))?;
    invalidate_runtime_prompt_tool_visibility_cache();
    agent_runtime.ai_tool_configuration = next_ai_tool_configuration.clone();
    let provider_configuration = agent_runtime.provider_configuration.clone();
    refresh_settings_projection_locked(
        &mut guard,
        &provider_configuration,
        Some(&next_ai_tool_configuration),
    );
    Ok(guard.workbench.clone())
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
    let workbench = mutate_shared_ai_capability_configuration(
        &state,
        "保存 AI 能力供应商失败",
        |next_ai_tool_configuration| {
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
            Ok(())
        },
    )?;
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
    let workbench = mutate_shared_ai_capability_configuration(
        &state,
        "移除 AI 能力供应商失败",
        |next_ai_tool_configuration| {
            next_ai_tool_configuration
                .vendors
                .retain(|vendor| vendor.vendor_id != vendor_id);
            Ok(())
        },
    )?;
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
    let target_selector = NormalizedAiCapabilityTargetSelector::required_from_raw(
        &request.vendor_id,
        &request.target_alias,
    )?;
    let workbench = mutate_shared_ai_capability_configuration(
        &state,
        "保存 AI 能力目标失败",
        |next_ai_tool_configuration| {
            let vendor = next_ai_tool_configuration
                .vendor_by_id_mut(target_selector.vendor_id())
                .ok_or_else(|| {
                    format!("未找到 AI 能力供应商 `{}`。", target_selector.vendor_id())
                })?;
            let binding = vendor
                .targets
                .iter_mut()
                .find(|binding| {
                    binding.capability_kind == capability_kind
                        && binding
                            .target_alias()
                            .eq_ignore_ascii_case(target_selector.target_alias())
                })
                .ok_or_else(|| format!("未找到 AI 能力目标 `{}`。", target_selector.selector()))?;
            binding.enabled = request.enabled;
            Ok(())
        },
    )?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Saves the configured target list for one AI capability from the WebUI settings page.
/// 从 WebUI 设置页保存单个 AI 能力的已配置目标列表。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot to the WebUI.
/// `app` 用于向 WebUI 发送刷新的工作台快照。
/// `state` owns the current service bridge runtime and settings snapshot.
/// `state` 持有当前服务桥接运行态与设置快照。
/// `request` contains the capability kind and target bindings to persist.
/// `request` 包含要持久化的能力类型与目标绑定。
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
    for target in request.targets {
        let Some(target_selector) =
            NormalizedAiCapabilityTargetSelector::from_raw(&target.vendor_id, &target.target_alias)
        else {
            continue;
        };
        let choice =
            provider_config::ai_capability_vendor_target_choices(target_selector.vendor_id())
                .into_iter()
                .find(|choice| {
                    choice.capability_kind == capability_kind
                        && choice
                            .target_alias
                            .eq_ignore_ascii_case(target_selector.target_alias())
                })
                .ok_or_else(|| {
                    format!(
                        "供应商 `{}` 不支持目标 `{}`。",
                        target_selector.vendor_id(),
                        target_selector.target_alias()
                    )
                })?;
        let model_id = if target.model_id.trim().is_empty() {
            choice.default_model_id
        } else {
            target.model_id.trim().to_string()
        };
        target_inputs.push((target_selector, model_id, target.enabled));
    }
    let workbench = mutate_shared_ai_capability_configuration(
        &state,
        "保存 AI 能力目标列表失败",
        |next_ai_tool_configuration| {
            // selected_target preserves only the user's existing exact binding when that target remains enabled.
            // selected_target 仅在用户已有的精确绑定目标仍保持启用时予以保留。
            let selected_target = next_ai_tool_configuration
                .configured_selected_target_for_kind(capability_kind)
                .filter(|selected| {
                    target_inputs.iter().any(|(target_selector, _, enabled)| {
                        *enabled && target_selector.selector().eq_ignore_ascii_case(selected)
                    })
                })
                .map(ToString::to_string);
            for vendor in &mut next_ai_tool_configuration.vendors {
                vendor
                    .targets
                    .retain(|binding| binding.capability_kind != capability_kind);
            }
            let vendor_ids = target_inputs
                .iter()
                .map(|(target_selector, _, _)| target_selector.vendor_id().to_string())
                .collect::<BTreeSet<_>>();
            for vendor_id in vendor_ids {
                let vendor = next_ai_tool_configuration
                    .vendor_by_id_mut(&vendor_id)
                    .ok_or_else(|| format!("未找到 AI 能力供应商 `{vendor_id}`。"))?;
                for (target_selector, model_id, enabled) in
                    target_inputs.iter().filter(|(target_selector, _, _)| {
                        target_selector.vendor_id() == vendor_id.as_str()
                    })
                {
                    let mut binding = PersistedAiCapabilityTargetBinding::new(
                        capability_kind,
                        target_selector.target_alias().to_string(),
                        model_id.clone(),
                    );
                    binding.enabled = *enabled;
                    vendor.targets.push(binding);
                }
            }
            set_ai_capability_selected_target_for_kind(
                next_ai_tool_configuration,
                capability_kind,
                selected_target,
            )?;
            Ok(())
        },
    )?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies target selector identity normalization trims and lowercases both components.
    /// 验证目标选择器身份规范化会修剪并小写化两个组成部分。
    #[test]
    fn normalized_ai_capability_target_selector_trims_lowercases_and_formats() {
        // target_selector captures the service-layer identity submitted by WebUI fields.
        // target_selector 表示 WebUI 字段提交到 service 层后的目标身份。
        let target_selector =
            NormalizedAiCapabilityTargetSelector::from_raw("  MiniMax  ", "  Vision  ")
                .expect("non-blank selector parts should normalize");

        assert_eq!(target_selector.vendor_id(), "minimax");
        assert_eq!(target_selector.target_alias(), "vision");
        assert_eq!(target_selector.selector(), "minimax/vision");
    }

    /// Verifies target selector identity normalization rejects blank components.
    /// 验证目标选择器身份规范化会拒绝空白组成部分。
    #[test]
    fn normalized_ai_capability_target_selector_drops_blank_parts() {
        assert_eq!(
            NormalizedAiCapabilityTargetSelector::from_raw("  ", "vision"),
            None
        );
        assert_eq!(
            NormalizedAiCapabilityTargetSelector::from_raw("minimax", "  "),
            None
        );
    }
}

//! AI capability summaries helpers.
//! AI 能力 summaries 辅助逻辑。

use super::*;

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
pub(crate) fn ai_capabilities(
    configuration: &PersistedAiToolConfiguration,
    model_capabilities: Option<&ResolvedModelCapabilities>,
) -> Vec<AiCapabilitySummary> {
    WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS
        .into_iter()
        .map(|kind| {
            let selected_target = configuration.configured_selected_target_for_kind(kind);
            let targets = configuration
                .resolved_configured_targets_for_kind(kind)
                .into_iter()
                .map(|target| {
                    let selected = selected_target == Some(target.target_selector.as_str());
                    AiCapabilityTargetSummary {
                        target_selector: target.target_selector,
                        vendor_id: target.vendor_id,
                        vendor_name: target.vendor_name,
                        target_alias: target.target_alias,
                        target_name: target.target_name,
                        model_id: target.model_id,
                        enabled: target.enabled,
                        selected,
                    }
                })
                .collect::<Vec<_>>();
            let native = ai_capability_has_native_path(kind, model_capabilities);
            let bridged = ai_capability_has_active_bridge_path(kind, configuration);
            let available = native || bridged;
            let enabled = available && !configuration.disabled_capability_kinds.contains(&kind);
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
/// # Returns
/// 返回值
/// `true` when a configured host bridge should be exposed for the capability.
/// 当该能力应暴露已配置宿主桥接路径时返回 `true`。
fn ai_capability_has_active_bridge_path(
    kind: AiToolKind,
    configuration: &PersistedAiToolConfiguration,
) -> bool {
    configuration.usable_targets_for_kind(kind).len() == 1
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
pub(crate) fn ai_capability_has_native_path(
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
pub(crate) fn ai_capability_vendors(
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
pub(crate) fn ai_capability_vendor_catalog() -> Vec<AiCapabilityVendorCatalogSummary> {
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
pub(crate) fn active_workbench_model_capabilities(
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

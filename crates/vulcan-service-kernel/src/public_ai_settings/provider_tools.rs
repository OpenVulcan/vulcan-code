//! Global provider-tool targets, media backends, extension sets, and shared availability validation.
//! 全局供应商工具目标、媒体后端、扩展集合与共享可用性校验。

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, ManifestSourceLayer, MediaUnderstandingBackendSelection,
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionResult,
    ProviderToolTargetSelection, PublicAiRuntime,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    inference_validation::validate_internal_reasoning_effort,
    media_candidates::{
        PublicAiMediaUnderstandingModelCandidate, build_media_understanding_model_candidates,
        media_understanding_resource_kind, model_route_supports_media_understanding,
    },
    public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Replaces the single selected target for one provider tool capability.
/// 替换一项供应商工具能力唯一选中的目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderToolSelectionSaveRequest {
    /// Exact semantic capability.
    /// 精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// One exact target, or absence to disable the capability.
    /// 一个精确目标，缺失时禁用该能力。
    selection: Option<ProviderToolTargetSelection>,
}

/// Enables or disables one provider model as a multimodal understanding candidate.
/// 启用或禁用一个供应商模型作为多模态理解候选。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiMultimodalUnderstandingCandidateSaveRequest {
    /// Exact provider identifier shared by every account route for this switch.
    /// 此开关覆盖全部账号路由的精确供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Replacement admission state.
    /// 替换后的准入状态。
    enabled: bool,
}

/// Saves or clears one provider-tool or model-replacement media-understanding backend.
/// 保存或清除一项供应商工具或模型代替媒体理解后端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiMediaUnderstandingSelectionSaveRequest {
    /// Exact image, video, or audio understanding capability.
    /// 精确的图片、视频或音频理解能力。
    capability: ProviderToolCapabilityKind,
    /// One exact backend, or absence to clear only the backend selection.
    /// 一个精确后端；缺失时仅清除后端选择，不改写独立能力开关。
    selection: Option<MediaUnderstandingBackendSelection>,
}

/// Replaces the complete native target set for one model-extension capability.
/// 替换一项模型扩展能力的完整原生目标集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderExtensionTargetSetSaveRequest {
    /// Exact generative model-extension capability.
    /// 精确生成类模型扩展能力。
    capability: ProviderToolCapabilityKind,
    /// Complete replacement set of exact native targets.
    /// 精确原生目标的完整替换集合。
    selections: BTreeSet<ProviderToolTargetSelection>,
}

/// Executes one exact search or extraction target for settings diagnostics.
/// 为设置诊断执行一个精确的搜索或网页抓取目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderToolTestRequest {
    /// Exact semantic capability under test.
    /// 正在测试的精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Exact connection and provider-tool target selected by the operator.
    /// 操作员选择的精确连接与供应商工具目标。
    selection: ProviderToolTargetSelection,
    /// Provider-native arguments already shaped by the target's declared schema.
    /// 已按照目标声明 Schema 组织的供应商原生参数。
    arguments: serde_json::Value,
}

/// Changes whether one semantic provider tool capability is enabled by default.
/// 修改一项语义供应商工具能力是否默认启用。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderToolCapabilityEnabledSaveRequest {
    /// Exact semantic capability.
    /// 精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Saves or clears the single selected target for one provider tool capability.
/// 保存或清除一项供应商工具能力唯一选中的目标。
///
/// # Parameters
/// 参数
/// `app` publishes settings; `state` owns runtime/configuration; `request` selects or clears one exact direct target.
/// `app` 发布设置；`state` 持有运行时与配置；`request` 选择或清除一个精确直连目标。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after target validation and tool-only publication, or an explicit failure.
/// 目标校验及工具专用发布完成后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_provider_tool_selection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolSelectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if media_understanding_resource_kind(request.capability).is_some() {
        return Err(format!(
            "Media-understanding capability `{:?}` must be configured through its dedicated backend selection.",
            request.capability
        ));
    }
    if !request.capability.is_direct_selection() {
        return Err(format!(
            "Provider tool capability `{:?}` must be configured through its model-extension target set.",
            request.capability
        ));
    }
    // transaction serializes target validation through Workbench publication.
    // transaction 把目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if let Some(selection) = request.selection.as_ref() {
        let available = runtime.available_provider_tool_targets(&current_configuration)?;
        let valid = available.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target.target_id == selection.target_id
                && candidate.target.contract.capability == request.capability
        });
        if !valid {
            return Err(format!(
                "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
                selection.target_id, selection.connection_id, request.capability
            ));
        }
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 工具目标失败",
        |configuration| {
            if let Some(selection) = request.selection.clone() {
                configuration
                    .provider_tool_selections
                    .insert(request.capability, selection);
            } else {
                configuration
                    .provider_tool_selections
                    .remove(&request.capability);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-model admission switch for all multimodal understanding lists.
/// 保存一个覆盖全部多模态理解列表的供应商模型准入开关。
///
/// # Parameters
/// 参数
/// `app` publishes the resulting settings, `state` owns runtime state, and `request` is the replacement switch.
/// `app` 发布结果设置，`state` 持有运行时状态，`request` 是替换后的开关值。
///
/// # Returns
/// 返回值
/// The authoritative settings snapshot after persistence and publication.
/// 持久化并发布后的权威设置快照。
pub(crate) async fn service_save_public_ai_multimodal_understanding_candidate(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMultimodalUnderstandingCandidateSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes candidate validation through authoritative settings publication.
    // transaction 把候选校验串行化到权威设置发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    // runtime and currentConfiguration freeze validation against one persisted generation.
    // runtime 与 currentConfiguration 将校验冻结在同一持久化代际。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if request.enabled {
        // settingsCatalog freezes effective route capabilities independently from conversation switches.
        // settingsCatalog 独立于会话开关冻结生效路由能力。
        let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
        // candidates contains every currently executable account route before the user admission switch.
        // candidates 包含用户准入开关之前当前可执行的全部账号路由。
        let candidates = build_media_understanding_model_candidates(
            &settings_catalog.runtime_configuration,
            &settings_catalog.providers,
            &settings_catalog.catalogs,
        )?;
        // eligible requires at least one exact account route for the shared provider-model switch.
        // eligible 要求共享供应商模型开关至少存在一条精确账号路由。
        let eligible = candidates.iter().any(|candidate| {
            candidate.selection.provider_id == request.provider_id
                && candidate.selection.model_id == request.model_id
        });
        if !eligible {
            return Err(format!(
                "Provider `{}` model `{}` has no executable multimodal conversation route with stable structured output.",
                request.provider_id, request.model_id
            ));
        }
    }
    // snapshot is rebuilt only after the replacement switch is persisted successfully.
    // snapshot 仅在替换后的开关成功持久化后重建。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存多模态理解模型候选开关失败",
        |configuration| {
            configuration.set_multimodal_understanding_candidate_enabled(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.enabled,
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one image, video, or audio understanding backend selection.
/// 保存或清除一项图片、视频或音频理解后端选择。
///
/// # Parameters
/// 参数
/// `app` publishes settings, `state` owns runtime state, and `request` selects one closed backend mode.
/// `app` 发布设置，`state` 持有运行时状态，`request` 选择一个封闭后端模式。
///
/// # Returns
/// 返回值
/// The authoritative settings snapshot after strict target validation.
/// 严格目标校验后的权威设置快照。
pub(crate) async fn service_save_public_ai_media_understanding_selection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMediaUnderstandingSelectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if media_understanding_resource_kind(request.capability).is_none() {
        return Err(format!(
            "Capability `{:?}` is not an image, video, or audio understanding capability.",
            request.capability
        ));
    }
    // transaction serializes backend validation through authoritative settings publication.
    // transaction 把后端校验串行化到权威设置发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    // runtime and currentConfiguration freeze targets and candidates to one generation.
    // runtime 与 currentConfiguration 将目标和候选冻结到同一代际。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // settingsCatalog supplies provider targets and resolved model capabilities from one generation.
    // settingsCatalog 从同一代际提供供应商目标与解析后的模型能力。
    let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
    // modelCandidates remains access-switch independent but still requires executable credentials.
    // modelCandidates 与会话访问开关独立，但仍要求凭据可执行。
    let model_candidates = build_media_understanding_model_candidates(
        &settings_catalog.runtime_configuration,
        &settings_catalog.providers,
        &settings_catalog.catalogs,
    )?;
    if let Some(selection) = request.selection.as_ref() {
        match selection {
            MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                // validTarget binds the exact credential target to the requested semantic capability.
                // validTarget 将精确凭据目标绑定到请求的语义能力。
                let valid_target =
                    settings_catalog
                        .available_provider_tool_targets
                        .iter()
                        .any(|candidate| {
                            candidate.connection_id == selection.connection_id
                                && candidate.target.target_id == selection.target_id
                                && candidate.target.contract.capability == request.capability
                        });
                if !valid_target {
                    return Err(format!(
                        "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
                        selection.target_id, selection.connection_id, request.capability
                    ));
                }
            }
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection,
                reasoning_effort,
            } => {
                if !settings_catalog
                    .runtime_configuration
                    .multimodal_understanding_candidate_is_enabled(
                        selection.provider_id.as_str(),
                        selection.model_id.as_str(),
                    )
                {
                    return Err(format!(
                        "Provider `{}` model `{}` is not enabled as a multimodal understanding candidate.",
                        selection.provider_id, selection.model_id
                    ));
                }
                // candidate is the one immutable account route selected by the user.
                // candidate 是用户选择的唯一不可变账号路由。
                let candidate = model_candidates
                    .iter()
                    .find(|candidate| candidate.selection == *selection)
                    .ok_or_else(|| {
                        format!(
                            "Provider `{}` model `{}` is not executable through connection `{}`.",
                            selection.provider_id, selection.model_id, selection.connection_id
                        )
                    })?;
                if !model_route_supports_media_understanding(
                    &candidate.capabilities,
                    request.capability,
                ) {
                    return Err(format!(
                        "Provider `{}` model `{}` does not support capability `{:?}` with stable structured output.",
                        selection.provider_id, selection.model_id, request.capability
                    ));
                }
                if let Some(reasoning_effort) = reasoning_effort {
                    validate_internal_reasoning_effort(&candidate.capabilities, *reasoning_effort)?;
                }
            }
        }
    }
    // snapshot is rebuilt only after the complete closed backend selection is persisted.
    // snapshot 仅在完整封闭后端选择成功持久化后重建。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存媒体理解后端选择失败",
        |configuration| {
            if let Some(selection) = request.selection.clone() {
                configuration
                    .media_understanding_backend_options
                    .entry(request.capability)
                    .or_default()
                    .update_from_selection(&selection);
                configuration
                    .media_understanding_selections
                    .insert(request.capability, selection);
            } else {
                configuration
                    .media_understanding_selections
                    .remove(&request.capability);
                configuration
                    .media_understanding_backend_options
                    .remove(&request.capability);
            }
            // Remove any legacy global direct target for the same media capability.
            // 同步移除同一媒体能力遗留的全局直连目标。
            configuration
                .provider_tool_selections
                .remove(&request.capability);
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the complete native target set for one model-extension capability.
/// 保存一项模型扩展能力的完整原生目标集合。
///
/// # Parameters
/// 参数
/// `app` publishes settings; `state` owns runtime/configuration; `request` replaces one capability's native target set.
/// `app` 发布设置；`state` 持有运行时与配置；`request` 替换一项能力的原生目标集合。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validating the complete set and publishing, or an explicit failure.
/// 校验完整集合并发布后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_provider_extension_target_set(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderExtensionTargetSetSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes native-target validation through Workbench publication.
    // transaction 把原生目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let replacement = BTreeMap::from([(request.capability, request.selections.clone())]);
    validate_provider_extension_target_sets(
        runtime.as_ref(),
        &current_configuration,
        &replacement,
    )?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 模型扩展目标集合失败",
        |configuration| {
            if request.selections.is_empty() {
                configuration
                    .provider_extension_target_sets
                    .remove(&request.capability);
            } else {
                configuration
                    .provider_extension_target_sets
                    .insert(request.capability, request.selections.clone());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Runs one real search or extraction diagnostic through the selected row without changing defaults.
/// 通过所选行运行一次真实搜索或网页抓取诊断且不修改默认项。
///
/// # Parameters
/// 参数
/// `state` owns the current runtime and configuration; `request` freezes the exact target and inputs.
/// `state` 持有当前运行时与配置；`request` 固定精确目标与输入。
///
/// # Returns
/// 返回值
/// A unified search or extraction result, or an explicit validation/provider failure.
/// 返回统一搜索或网页抓取结果，或明确的校验与供应商错误。
pub(crate) async fn service_test_public_ai_provider_tool(
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolTestRequest,
) -> Result<ProviderToolExecutionResult, String> {
    if !matches!(
        request.capability,
        ProviderToolCapabilityKind::WebSearch | ProviderToolCapabilityKind::WebExtract
    ) {
        return Err(format!(
            "Provider tool settings diagnostics do not support capability `{:?}`.",
            request.capability
        ));
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    runtime
        .execute_provider_tool_target_for_owner(
            &configuration,
            request.capability,
            &request.selection,
            request.arguments,
            None,
        )
        .await
}

/// Saves the default enabled state of one semantic provider tool capability.
/// 保存一项语义供应商工具能力的默认启用状态。
///
/// # Parameters
/// 参数
/// `app` publishes settings; `state` owns runtime/configuration; `request` selects one direct capability's default enabled state.
/// `app` 发布设置；`state` 持有运行时与配置；`request` 选择一项直连能力的默认启用状态。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validating enabled backends and publishing, or an explicit failure.
/// 校验所启用后端并发布后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_provider_tool_capability_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolCapabilityEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !request.capability.is_direct_selection() {
        return Err(format!(
            "Provider tool capability `{:?}` is enabled by selecting its model-extension targets.",
            request.capability
        ));
    }
    // transaction serializes capability validation through Workbench publication.
    // transaction 把能力校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if request.enabled {
        // settingsCatalog freezes provider-tool and conversation-model inventories to one generation.
        // settingsCatalog 将供应商工具与会话模型目录冻结在同一代际。
        let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
        // mediaModelCandidates is derived only for fixed media facades; search and fetch keep their direct-target rule.
        // mediaModelCandidates 仅为固定媒体外壳派生；搜索与抓取继续遵循直连目标规则。
        let media_model_candidates =
            if media_understanding_resource_kind(request.capability).is_some() {
                build_media_understanding_model_candidates(
                    &settings_catalog.runtime_configuration,
                    &settings_catalog.providers,
                    &settings_catalog.catalogs,
                )?
            } else {
                Vec::new()
            };
        if !provider_tool_capability_backend_is_available(
            &settings_catalog.runtime_configuration,
            request.capability,
            &settings_catalog.available_provider_tool_targets,
            &media_model_candidates,
        ) {
            return Err(format!(
                "Provider tool capability `{:?}` has no available configured backend.",
                request.capability
            ));
        }
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 工具能力启用状态失败",
        |configuration| {
            if request.enabled {
                configuration
                    .provider_tool_enabled_capabilities
                    .insert(request.capability);
            } else {
                configuration
                    .provider_tool_enabled_capabilities
                    .remove(&request.capability);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Reports whether one configured media-understanding backend remains exactly executable.
/// 报告一项已配置媒体理解后端是否仍可精确执行。
///
/// # Parameters
/// 参数
/// `configuration` owns the closed backend choice and candidate switch, `capability` identifies
/// one fixed media tool, and the two slices are frozen provider-tool and conversation inventories.
/// `configuration` 持有封闭后端选择与候选开关，`capability` 标识一个固定媒体工具，两个切片分别是
/// 冻结的供应商工具目录与会话模型目录。
///
/// # Returns
/// 返回值
/// `true` only when the exact provider target or admitted structured-output model route still exists.
/// 仅当精确供应商目标或已准入的结构化输出模型路由仍然存在时返回 `true`。
pub(super) fn media_understanding_backend_is_available(
    configuration: &PersistedAiConfiguration,
    capability: ProviderToolCapabilityKind,
    available_provider_targets: &[AvailableProviderToolTarget],
    model_candidates: &[PublicAiMediaUnderstandingModelCandidate],
) -> bool {
    let Some(backend) = configuration
        .media_understanding_selections
        .get(&capability)
    else {
        return false;
    };
    match backend {
        MediaUnderstandingBackendSelection::ProviderTool { selection } => {
            available_provider_targets.iter().any(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == capability
            })
        }
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection,
            reasoning_effort,
        } => {
            configuration.multimodal_understanding_candidate_is_enabled(
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            ) && model_candidates.iter().any(|candidate| {
                candidate.selection == *selection
                    && model_route_supports_media_understanding(&candidate.capabilities, capability)
                    && reasoning_effort.is_none_or(|effort| {
                        validate_internal_reasoning_effort(&candidate.capabilities, effort).is_ok()
                    })
            })
        }
    }
}

/// Reports whether one direct capability has an executable provider or replacement backend.
/// 报告一项直连能力是否具有可执行的供应商后端或模型代替后端。
///
/// # Parameters
/// 参数
/// `configuration` owns media selections, `capability` selects the validation rule, and the two
/// inventories belong to one settings-catalog generation.
/// `configuration` 持有媒体选择，`capability` 选择校验规则，两份目录归属于同一设置目录代际。
///
/// # Returns
/// 返回值
/// `true` for an exact executable media backend or at least one matching ordinary direct target.
/// 精确媒体后端可执行，或至少存在一个匹配的普通直连目标时返回 `true`。
fn provider_tool_capability_backend_is_available(
    configuration: &PersistedAiConfiguration,
    capability: ProviderToolCapabilityKind,
    available_provider_targets: &[AvailableProviderToolTarget],
    model_candidates: &[PublicAiMediaUnderstandingModelCandidate],
) -> bool {
    if media_understanding_resource_kind(capability).is_some() {
        return media_understanding_backend_is_available(
            configuration,
            capability,
            available_provider_targets,
            model_candidates,
        );
    }
    available_provider_targets
        .iter()
        .any(|candidate| candidate.target.contract.capability == capability)
}

/// Filters historical selections against the exact current target inventory.
/// 针对精确当前目标目录过滤历史选择。
///
/// # Parameters
/// 参数
/// `available` is the target inventory captured with the settings generation; `selections` owns
/// the persisted capability choices to validate against that exact inventory.
/// `available` 是与设置代际共同捕获的目标目录；`selections` 持有要针对该精确目录校验的
/// 持久化能力选择。
///
/// # Returns
/// 返回值
/// Only exact connection/target/capability triples present in the supplied inventory.
/// 仅返回传入目录中存在的精确连接、目标与能力三元组。
pub(super) fn filter_current_provider_tool_selections(
    available: &[AvailableProviderToolTarget],
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
) -> BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection> {
    selections
        .iter()
        .filter(|(capability, selection)| {
            available.iter().any(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == **capability
            })
        })
        .map(|(capability, selection)| (*capability, selection.clone()))
        .collect()
}

/// Validates complete native model-extension target sets against one frozen runtime inventory.
/// 针对一份冻结运行时目录校验完整原生模型扩展目标集合。
///
/// # Parameters
/// 参数
/// `runtime` supplies the current target inventory; `configuration` fixes its generation; `target_sets` contains exact native selections.
/// `runtime` 提供当前目标目录；`configuration` 固定其代际；`target_sets` 包含精确原生选择。
///
/// # Returns
/// 返回值
/// Returns success only for available native targets with reviewed profiles and unique public model identifiers.
/// 仅当原生目标可用、具有已审查说明且公共模型标识唯一时返回成功。
pub(super) fn validate_provider_extension_target_sets(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    target_sets: &BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
) -> Result<(), String> {
    let available = runtime.available_provider_tool_targets(configuration)?;
    for (capability, selections) in target_sets {
        if !capability.is_model_extension() {
            return Err(format!(
                "Provider tool capability `{:?}` must use direct single-target selection.",
                capability
            ));
        }
        let mut public_model_ids = BTreeSet::new();
        for selection in selections {
            let candidate = available
                .iter()
                .find(|candidate| {
                    candidate.connection_id == selection.connection_id
                        && candidate.target.target_id == selection.target_id
                        && candidate.target.contract.capability == *capability
                })
                .ok_or_else(|| {
                    format!(
                        "Model-extension target `{}` is not available through connection `{}` for capability `{:?}`.",
                        selection.target_id, selection.connection_id, capability
                    )
                })?;
            if candidate.manifest_source_layer == ManifestSourceLayer::Custom {
                return Err(format!(
                    "Custom provider target `{}` cannot be selected as a model extension.",
                    selection.target_id
                ));
            }
            if candidate.target.contract.extension_profile.is_none() {
                return Err(format!(
                    "Native model-extension target `{}` has no reviewed extension profile.",
                    selection.target_id
                ));
            }
            let public_model_id = format!(
                "{}/{}",
                candidate.target.provider_id, candidate.target.upstream_target_id
            );
            if !public_model_ids.insert(public_model_id.clone()) {
                return Err(format!(
                    "Model-extension capability `{:?}` exposes duplicate public model id `{public_model_id}`.",
                    capability
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

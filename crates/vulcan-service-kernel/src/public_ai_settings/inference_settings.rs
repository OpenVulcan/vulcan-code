//! Internal inference candidates, route diagnostics, operation settings, and library-owned profiles.
//! 内部推理候选、路由诊断、操作设置与资料库独立配置档。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{AiOperation, InferenceUsageScope, ModelCapabilities};
use vulcan_provider_adapters::{
    ConnectionModelCatalog, InferenceRouteBinding, InferenceRouteConfiguration,
    InternalModelOverrides, PersistedAiConfiguration, PublicAiRuntime,
    registered_native_action_binding_id,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    effective_catalog_model_protocol_id,
    inference_validation::{validate_internal_model_overrides, validate_vmm_llm_reasoning_policy},
    public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_in_transaction,
    validate_non_empty,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Stable LLM purpose key used by the library-owned configured-inference profile.
/// 资料库独立配置推理档案使用的稳定 LLM 用途键。
pub(crate) const KNOWLEDGE_CURATION_PURPOSE_ID: &str = "knowledge_display_curation";

/// Browser-facing diagnostic for one persisted inference-profile route.
/// 面向浏览器的一条持久化推理配置档路由诊断。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiInferenceRouteDiagnostic {
    /// Stable consumer profile that owns the route.
    /// 持有该路由的稳定消费者配置档。
    consumer_profile_id: String,
    /// Optional stable LLM purpose within the consumer profile.
    /// 消费者配置档内可选的稳定 LLM 用途。
    purpose_id: Option<String>,
    /// Expected operation of the owning profile slot.
    /// 所属配置槽位期望的操作。
    operation: AiOperation,
    /// Exact retained route that failed current runtime resolution.
    /// 当前运行时解析失败但仍保留的精确路由。
    route: InferenceRouteConfiguration,
    /// Exact backend failure produced by strict route validation.
    /// 严格路由校验产生的精确后端失败原因。
    reason: String,
}

/// Browser-facing exact route candidate for one internal inference operation.
/// 面向浏览器的一项内部推理操作精确路由候选。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiInternalInferenceCandidate {
    /// User-visible provider name.
    /// 用户可见供应商名称。
    provider_display_name: String,
    /// User-visible service-entry name.
    /// 用户可见服务入口名称。
    service_entry_display_name: String,
    /// User-visible credential connection name.
    /// 用户可见凭据连接名称。
    connection_display_name: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    model_display_name: String,
    /// Exact immutable route persisted when selected.
    /// 选中时持久化的精确不可变路由。
    pub(crate) route: InferenceRouteConfiguration,
    /// Process-local catalog generation that established the model route.
    /// 建立该模型路由的进程内目录代次。
    pub(crate) catalog_generation: u64,
    /// Whether the model operation is enabled for internal use.
    /// 当前模型操作是否已为内部使用启用。
    pub(crate) enabled: bool,
    /// Exact usage scopes permitted by product, credential, and user narrowing.
    /// 产品、凭据与用户收窄共同允许的精确用途范围。
    usage_scopes: BTreeSet<InferenceUsageScope>,
    /// Exact model capability contract used to generate internal controls.
    /// 用于生成内部控件的精确模型能力契约。
    pub(crate) capabilities: ModelCapabilities,
    /// Current operation-scoped model defaults.
    /// 当前操作作用域模型默认值。
    overrides: InternalModelOverrides,
}

/// Saves one provider-model internal-use choice independent of conversation publication.
/// 保存一项独立于会话发布的供应商模型内部使用选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiInternalModelEnabledSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces one exact consumer-profile inference route.
/// 替换一条精确消费者配置档推理路由。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiInferenceRouteSaveRequest {
    /// Stable consumer profile identifier.
    /// 稳定消费者配置档标识。
    consumer_profile_id: String,
    /// Consumer-owned purpose required only for restricted LLM routes.
    /// 仅受限 LLM 路由要求的消费者自有用途。
    #[serde(default)]
    purpose_id: Option<String>,
    /// Exact operation being replaced.
    /// 正在替换的精确操作。
    operation: AiOperation,
    /// Exact route, or absence to clear the configured operation.
    /// 精确路由，缺失时清除已配置操作。
    route: Option<InferenceRouteConfiguration>,
}

/// Replaces operation-scoped internal defaults for one provider model.
/// 替换一个供应商模型的操作作用域内部默认值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiInternalModelOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact internal operation.
    /// 精确内部操作。
    operation: AiOperation,
    /// Complete replacement override value.
    /// 完整替换覆盖值。
    overrides: InternalModelOverrides,
}

/// Saves one model-level internal-use switch without changing conversation availability.
/// 保存一项模型级内部使用开关且不改变会话可用性。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify the settings transaction and exact model.
/// `app`、`state` 与 `request` 标识设置事务及精确模型。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot.
/// 返回完整刷新后的设置快照。
pub(crate) async fn service_save_public_ai_internal_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInternalModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&current_configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let exists =
        build_internal_inference_candidates(&current_configuration, &providers, &catalogs)?
            .iter()
            .any(|candidate| {
                candidate.route.provider_id == request.provider_id
                    && candidate.route.model_id == request.model_id
            });
    if !exists {
        return Err(format!(
            "Provider `{}` model `{}` does not expose a registered internal inference operation.",
            request.provider_id, request.model_id
        ));
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理模型启用状态失败",
        |configuration| {
            configuration.set_internal_model_enabled(
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

/// Saves operation-scoped internal defaults after validating every exact runtime route.
/// 在校验全部精确运行时路由后保存操作作用域内部默认值。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify one provider-model-operation replacement.
/// `app`、`state` 与 `request` 标识一个供应商、模型、操作的替换请求。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot.
/// 返回完整刷新后的设置快照。
pub(crate) async fn service_save_public_ai_internal_model_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInternalModelOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !matches!(
        request.operation,
        AiOperation::ConversationRespond
            | AiOperation::EmbeddingCreate
            | AiOperation::RerankDocuments
    ) {
        return Err(format!(
            "Operation `{}` is not configurable as an internal inference model.",
            request.operation.as_str()
        ));
    }
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&current_configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let matching_candidates =
        build_internal_inference_candidates(&current_configuration, &providers, &catalogs)?
            .into_iter()
            .filter(|candidate| {
                candidate.route.provider_id == request.provider_id
                    && candidate.route.model_id == request.model_id
                    && candidate.route.operation == request.operation
            })
            .collect::<Vec<_>>();
    if matching_candidates.is_empty() {
        return Err(format!(
            "Provider `{}` model `{}` does not expose registered internal operation `{}`.",
            request.provider_id,
            request.model_id,
            request.operation.as_str()
        ));
    }
    for candidate in &matching_candidates {
        validate_internal_model_overrides(
            &candidate.capabilities,
            &candidate.route,
            &request.overrides,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理模型覆盖失败",
        |configuration| {
            let key = PersistedAiConfiguration::internal_model_operation_key(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.operation,
            );
            if request.overrides == InternalModelOverrides::default() {
                configuration.internal_model_overrides.remove(key.as_str());
            } else {
                configuration
                    .internal_model_overrides
                    .insert(key, request.overrides.clone());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one exact route in a consumer-isolated inference profile.
/// 在消费者隔离推理配置档内保存或清除一条精确路由。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify the settings transaction and route replacement.
/// `app`、`state` 与 `request` 标识设置事务及路由替换。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot after strict route validation.
/// 严格校验路由后返回完整刷新设置快照。
pub(crate) async fn service_save_public_ai_inference_route(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInferenceRouteSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("consumerProfileId", request.consumer_profile_id.as_str())?;
    match request.operation {
        AiOperation::ConversationRespond => {
            let purpose_id = request.purpose_id.as_deref().ok_or_else(|| {
                String::from("Restricted LLM inference route requires purposeId.")
            })?;
            validate_non_empty("purposeId", purpose_id)?;
        }
        AiOperation::EmbeddingCreate | AiOperation::RerankDocuments => {
            if request.purpose_id.is_some() {
                return Err(String::from(
                    "Embedding and rerank inference routes do not accept purposeId.",
                ));
            }
        }
        _ => {
            return Err(format!(
                "Operation `{}` is not configurable through the inference profile.",
                request.operation.as_str()
            ));
        }
    }
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if let Some(route) = request.route.as_ref() {
        if route.operation != request.operation {
            return Err(String::from(
                "Inference route operation differs from the requested profile operation.",
            ));
        }
        // Route selection consumes the model-level state owned by Model List and must never
        // mutate or bypass it from a consumer-specific settings surface.
        // 路由选择只消费“模型列表”持有的模型级状态，绝不能从消费者专属设置界面修改或绕过它。
        let selection =
            runtime.resolve_internal_inference_selection(&current_configuration, route)?;
        validate_vmm_llm_reasoning_policy(
            request.consumer_profile_id.as_str(),
            request.operation,
            &selection.model.capabilities,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理路由失败",
        |configuration| {
            let profile = configuration
                .inference_profiles
                .entry(request.consumer_profile_id.clone())
                .or_default();
            match request.operation {
                AiOperation::ConversationRespond => {
                    let purpose_id = request.purpose_id.as_deref().ok_or_else(|| {
                        String::from("Restricted LLM inference route requires purposeId.")
                    })?;
                    if let Some(route) = request.route.clone() {
                        profile.llm_routes.insert(purpose_id.to_string(), route);
                    } else {
                        profile.llm_routes.remove(purpose_id);
                    }
                }
                AiOperation::EmbeddingCreate => {
                    profile.embedding = request.route.clone();
                }
                AiOperation::RerankDocuments => {
                    profile.rerank = request.route.clone();
                }
                _ => {
                    return Err(format!(
                        "Operation `{}` is not configurable through the inference profile.",
                        request.operation.as_str()
                    ));
                }
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Diagnoses every retained consumer route with the same strict resolver used for execution.
/// 使用与真实执行相同的严格解析器诊断每条保留的消费者路由。
///
/// # Parameters
/// 参数
/// `runtime` resolves current credentials, models, bindings, and usage scopes while
/// `configuration` owns the retained consumer routes.
/// `runtime` 解析当前凭据、模型、绑定与用途范围，`configuration` 持有保留的消费者路由。
///
/// # Returns
/// 返回值
/// Only failed routes, each paired with its exact profile slot and backend reason.
/// 仅返回失败路由，并携带其精确配置槽位与后端原因。
pub(super) fn build_internal_inference_route_diagnostics(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Vec<PublicAiInferenceRouteDiagnostic> {
    let mut diagnostics = Vec::new();
    for (consumer_profile_id, profile) in &configuration.inference_profiles {
        for (purpose_id, route) in &profile.llm_routes {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                Some(purpose_id.as_str()),
                AiOperation::ConversationRespond,
                route,
            );
        }
        if let Some(route) = profile.embedding.as_ref() {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                None,
                AiOperation::EmbeddingCreate,
                route,
            );
        }
        if let Some(route) = profile.rerank.as_ref() {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                None,
                AiOperation::RerankDocuments,
                route,
            );
        }
    }
    diagnostics
}

/// Appends one diagnostic only when a retained profile route is not currently executable.
/// 仅在保留的配置档路由当前不可执行时追加一项诊断。
///
/// # Parameters
/// 参数
/// The profile slot supplies trusted expected identity and `runtime` performs exact resolution.
/// 配置档槽位提供可信期望身份，`runtime` 执行精确解析。
///
/// # Returns
/// 返回值
/// This function mutates `diagnostics` and has no independent return value.
/// 本函数修改 `diagnostics`，不返回独立值。
#[allow(clippy::too_many_arguments)]
fn push_internal_inference_route_diagnostic(
    diagnostics: &mut Vec<PublicAiInferenceRouteDiagnostic>,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    purpose_id: Option<&str>,
    expected_operation: AiOperation,
    route: &InferenceRouteConfiguration,
) {
    let resolution = if route.operation != expected_operation {
        Err(format!(
            "Persisted inference route operation `{}` does not match profile slot operation `{}`.",
            route.operation.as_str(),
            expected_operation.as_str()
        ))
    } else {
        runtime
            .resolve_internal_inference_selection(configuration, route)
            .and_then(|selection| {
                validate_vmm_llm_reasoning_policy(
                    consumer_profile_id,
                    expected_operation,
                    &selection.model.capabilities,
                )
            })
    };
    if let Err(reason) = resolution {
        diagnostics.push(PublicAiInferenceRouteDiagnostic {
            consumer_profile_id: consumer_profile_id.to_string(),
            purpose_id: purpose_id.map(str::to_string),
            operation: expected_operation,
            route: route.clone(),
            reason,
        });
    }
}

/// Builds exact internal inference route candidates from current connection catalogs.
/// 从当前连接目录构建精确内部推理路由候选。
///
/// # Parameters
/// 参数
/// `configuration`, `providers`, and `catalogs` are one coherent settings snapshot.
/// `configuration`、`providers` 与 `catalogs` 构成一份一致设置快照。
///
/// # Returns
/// 返回值
/// Stable candidates that preserve product, credential, binding, operation, and usage scope.
/// 返回保留产品、凭据、绑定、操作与用途范围的稳定候选。
pub(super) fn build_internal_inference_candidates(
    configuration: &PersistedAiConfiguration,
    providers: &vulcan_provider_adapters::ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Vec<PublicAiInternalInferenceCandidate>, String> {
    // defaultInternalEnabledByProvider resolves each provider-wide suggestion once so every
    // credential route for the same model exposes one stable shared switch.
    // defaultInternalEnabledByProvider 只解析一次各供应商级建议，使同一模型的所有凭据路由
    // 公开一个稳定的共享开关。
    let default_internal_enabled_by_provider = providers
        .providers()
        .into_iter()
        .map(|provider| {
            provider
                .default_internal_use_for_connections(
                    configuration
                        .connection_graph
                        .connections
                        .values()
                        .filter(|connection| {
                            connection.provider_id == provider.id
                                && !configuration
                                    .connection_is_pending_validation(connection.id.as_str())
                        }),
                )
                .map(|enabled| (provider.id.clone(), enabled))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    // internalUsageScopes preserves runtime scheduling and authorization classifications while
    // model enablement remains shared across Knowledge and VMM consumers.
    // internalUsageScopes 保留运行时调度与鉴权分类，同时让知识库与 VMM 消费方共享模型启用状态。
    let internal_usage_scopes = BTreeSet::from([
        InferenceUsageScope::InternalBackground,
        InferenceUsageScope::ExternalService,
    ]);
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider `{}` referenced by connection `{}` is not registered.",
                    connection.provider_id, connection.id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` referenced by connection `{}` is not registered.",
                    connection.service_entry_id, connection.id
                )
            })?;
        let default_internal_enabled = default_internal_enabled_by_provider
            .get(provider.id.as_str())
            .copied()
            .ok_or_else(|| {
                format!(
                    "Provider `{}` is missing its internal-use default projection.",
                    provider.id
                )
            })?;
        for model in &catalog.models {
            for operation in model
                .capabilities
                .operations
                .iter()
                .copied()
                .filter(|operation| {
                    matches!(
                        operation,
                        AiOperation::ConversationRespond
                            | AiOperation::EmbeddingCreate
                            | AiOperation::RerankDocuments
                    )
                })
            {
                let binding = match operation {
                    AiOperation::ConversationRespond => {
                        // Internal LLM consumers must persist the same frozen model binding used
                        // by interactive execution; connection credentials never resolve protocols.
                        // 内部 LLM 消费方必须持久化交互执行所使用的同一冻结模型绑定；连接凭据
                        // 绝不解析协议。
                        let Some(protocol_profile_id) = effective_catalog_model_protocol_id(model)
                        else {
                            continue;
                        };
                        InferenceRouteBinding::Protocol {
                            protocol_profile_id: protocol_profile_id.to_string(),
                        }
                    }
                    AiOperation::EmbeddingCreate | AiOperation::RerankDocuments => {
                        let Some(action_binding_id) =
                            registered_native_action_binding_id(entry, operation)
                        else {
                            continue;
                        };
                        InferenceRouteBinding::Action {
                            action_binding_id: action_binding_id.to_string(),
                        }
                    }
                    _ => continue,
                };
                for usage_scope in internal_usage_scopes.iter().copied() {
                    candidates.push(PublicAiInternalInferenceCandidate {
                        provider_display_name: provider.display_name.clone(),
                        service_entry_display_name: entry.display_name.clone(),
                        connection_display_name: connection.display_name.clone(),
                        model_display_name: model.display_name.clone(),
                        route: InferenceRouteConfiguration {
                            provider_id: connection.provider_id.clone(),
                            service_entry_id: connection.service_entry_id.clone(),
                            connection_id: connection.id.clone(),
                            model_id: model.id.clone(),
                            binding: binding.clone(),
                            operation,
                            usage_scope,
                        },
                        catalog_generation: catalog.generation,
                        enabled: configuration.internal_model_is_enabled(
                            connection.provider_id.as_str(),
                            model.id.as_str(),
                            default_internal_enabled,
                        ),
                        usage_scopes: internal_usage_scopes.clone(),
                        capabilities: model.capabilities.clone(),
                        overrides: configuration
                            .internal_model_overrides
                            .get(
                                PersistedAiConfiguration::internal_model_operation_key(
                                    connection.provider_id.as_str(),
                                    model.id.as_str(),
                                    operation,
                                )
                                .as_str(),
                            )
                            .cloned()
                            .unwrap_or_default(),
                    });
                }
            }
        }
    }
    candidates.sort_by(|left, right| {
        (
            left.route.operation,
            left.provider_display_name.as_str(),
            left.model_display_name.as_str(),
            left.connection_display_name.as_str(),
            left.route.usage_scope,
        )
            .cmp(&(
                right.route.operation,
                right.provider_display_name.as_str(),
                right.model_display_name.as_str(),
                right.connection_display_name.as_str(),
                right.route.usage_scope,
            ))
    });
    Ok(candidates)
}

/// Resolves one exact registered internal route for a knowledge-library setting.
/// 为资料库配置解析一条精确的已注册内部路由。
///
/// # Parameters
/// 参数
/// `state` owns the current settings snapshot; provider/service/model ids and `binding` identify the exact registered route.
/// `state` 持有当前设置快照；供应商、服务、模型标识及 `binding` 指定精确已注册路由。
/// `connection_id` optionally narrows the credential; `operation` and `usage_scope` identify the required consumer capability.
/// `connection_id` 可选地收窄凭据；`operation` 与 `usage_scope` 指定所需消费者能力。
///
/// # Returns
/// 返回值
/// Returns one enabled exact candidate, or an explicit missing/disabled-route error.
/// 返回一个已启用精确候选，或显式缺失/禁用路由错误。
pub(crate) fn resolve_knowledge_internal_inference_candidate(
    state: &SharedServiceBridgeState,
    provider_id: &str,
    service_entry_id: &str,
    connection_id: Option<&str>,
    model_id: &str,
    binding: &InferenceRouteBinding,
    operation: AiOperation,
    usage_scope: InferenceUsageScope,
) -> Result<PublicAiInternalInferenceCandidate, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let candidate = build_internal_inference_candidates(&configuration, &providers, &catalogs)?
        .into_iter()
        .find(|candidate| {
            candidate.route.provider_id == provider_id
                && candidate.route.service_entry_id == service_entry_id
                && connection_id.is_none_or(|value| candidate.route.connection_id == value)
                && candidate.route.model_id == model_id
                && candidate.route.binding == *binding
                && candidate.route.operation == operation
                && candidate.route.usage_scope == usage_scope
        })
        .ok_or_else(|| {
            format!(
                "未找到可执行的官方知识库模型路由：provider={provider_id}, service={service_entry_id}, model={model_id}, operation={}",
                operation.as_str()
            )
        })?;
    if !candidate.enabled {
        return Err(format!(
            "知识库模型当前未在模型列表启用内部使用：provider={provider_id}, model={model_id}"
        ));
    }
    Ok(candidate)
}

/// Synchronizes one library-owned inference profile with its validated model routes.
/// 将一个资料库独立拥有的推理配置档同步为经过校验的模型路由。
///
/// The profile is intentionally keyed by the immutable library identifier so two libraries
/// can select different models without overwriting one another or the VMM profile.
/// 配置档故意使用不可变资料库标识作为键，确保不同资料库可以选择不同模型，且不会覆盖 VMM 配置档。
///
/// # Parameters
/// 参数
/// `app` publishes changes; `state` owns runtime/configuration; `consumer_profile_id` identifies the independent library profile.
/// `app` 发布变更；`state` 持有运行时与配置；`consumer_profile_id` 标识独立资料库配置档。
/// `curation_route` and `embedding_route` replace their slots or clear them when absent.
/// `curation_route` 与 `embedding_route` 替换对应槽位，缺失时清除该槽位。
///
/// # Returns
/// 返回值
/// Returns success after validated profile persistence and publication, or an explicit failure.
/// 已校验配置档持久化并发布后返回成功，或返回显式失败。
pub(crate) async fn sync_knowledge_inference_profile(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    consumer_profile_id: &str,
    curation_route: Option<InferenceRouteConfiguration>,
    embedding_route: Option<InferenceRouteConfiguration>,
) -> Result<(), String> {
    validate_non_empty("consumerProfileId", consumer_profile_id)?;
    // transaction serializes route validation through profile and Workbench publication.
    // transaction 把路由校验串行化到配置档与工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    for route in [curation_route.as_ref(), embedding_route.as_ref()]
        .into_iter()
        .flatten()
    {
        if !matches!(
            route.operation,
            AiOperation::ConversationRespond | AiOperation::EmbeddingCreate
        ) {
            return Err(format!(
                "资料库配置档不支持内部操作 `{}`",
                route.operation.as_str()
            ));
        }
        let selection =
            runtime.resolve_internal_inference_selection(&current_configuration, route)?;
        validate_vmm_llm_reasoning_policy(
            consumer_profile_id,
            route.operation,
            &selection.model.capabilities,
        )?;
    }
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存资料库独立模型配置失败",
        |configuration| {
            let profile = configuration
                .inference_profiles
                .entry(consumer_profile_id.to_string())
                .or_default();
            if let Some(route) = curation_route.clone() {
                profile
                    .llm_routes
                    .insert(String::from(KNOWLEDGE_CURATION_PURPOSE_ID), route);
            } else {
                profile.llm_routes.remove(KNOWLEDGE_CURATION_PURPOSE_ID);
            }
            profile.embedding = embedding_route.clone();
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(())
}

/// Removes one library-owned inference profile after the library is deleted.
/// 删除资料库后移除该资料库独立拥有的推理配置档。
///
/// # Parameters
/// 参数
/// `app` publishes changes; `state` owns runtime/configuration; `consumer_profile_id` identifies the exact profile to remove.
/// `app` 发布变更；`state` 持有运行时与配置；`consumer_profile_id` 标识待移除的精确配置档。
///
/// # Returns
/// 返回值
/// Returns success after removing only that profile and publishing, or an explicit failure.
/// 只移除该配置档并发布后返回成功，或返回显式失败。
pub(crate) async fn remove_knowledge_inference_profile(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    consumer_profile_id: &str,
) -> Result<(), String> {
    validate_non_empty("consumerProfileId", consumer_profile_id)?;
    // transaction serializes profile removal through Workbench publication.
    // transaction 把配置档删除串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "删除资料库独立模型配置失败",
        |configuration| {
            configuration.inference_profiles.remove(consumer_profile_id);
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(())
}

#[cfg(test)]
mod tests;

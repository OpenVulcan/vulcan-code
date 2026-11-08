//! User-owned custom-model definitions, reviewed template settings, and explicit validation.
//! 用户自有模型定义、已审查模板设置与显式验证。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, ParallelToolCallPolicy,
    ReasoningEffort, ReasoningOutputPolicy, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_provider_adapters::{
    CommonModelTemplateSuggestion, CustomModelDefinition, CustomModelProtocolPreference,
    CustomModelTemplateDecision, CustomProviderModelDefaults, PersistedAiConfiguration,
    ProviderKind, PublicAiRuntime, validate_system_manual_model_definition,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    custom_providers::validate_custom_provider_identity,
    inference_testing::configured_inference_service_for_settings,
    public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_in_transaction, validate_non_empty,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Maximum number of offline template lookups accepted by one browser request.
/// 单次浏览器请求允许执行的离线模板查询数量上限。
const MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE: usize = 2_000;

/// Requests one explicit, potentially billable custom-provider model validation.
/// 请求一次显式且可能计费的自定义供应商模型验证。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelValidationRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact credential connection under validation.
    /// 接受验证的精确凭据连接。
    connection_id: String,
    /// Exact bidirectional-text model identifier.
    /// 精确双向文本模型标识。
    model_id: String,
}

/// Returns compact facts after one explicit custom-model validation succeeds.
/// 在一次显式自定义模型验证成功后返回紧凑事实。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiCustomModelValidationResult {
    /// Exact provider validated by the request.
    /// 请求验证的精确供应商。
    provider_id: String,
    /// Exact credential connection validated by the request.
    /// 请求验证的精确凭据连接。
    connection_id: String,
    /// Exact model validated by the request.
    /// 请求验证的精确模型。
    model_id: String,
    /// Trimmed provider text returned by the validation request.
    /// 验证请求返回的已去除首尾空白供应商文本。
    response_text: String,
    /// Unicode scalar count in the returned validation text.
    /// 返回验证文本中的 Unicode 标量数量。
    response_characters: usize,
}

/// Replaces the complete user-owned model set for one custom provider.
/// 替换一个自定义供应商的完整用户自有模型集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelsSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete replacement model set.
    /// 完整替换模型集合。
    models: Vec<CustomModelDefinition>,
    /// Explicit reviewed template decisions committed with the replacement model set.
    /// 与替换模型集合一同提交的显式已审查模板决策。
    #[serde(default)]
    template_decisions: Vec<PublicAiCustomModelTemplateDecisionBatchItem>,
}

/// One per-model protocol preference item for a custom provider.
/// 一个自定义供应商的逐模型协议偏好条目。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelProtocolPreferenceItem {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Auto or explicit protocol preference for this model.
    /// 该模型的自动或显式协议偏好。
    preference: CustomModelProtocolPreference,
}

/// Replaces the per-model protocol preferences for one custom provider.
/// 替换一个自定义供应商的逐模型协议偏好。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelProtocolPreferencesSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete replacement preference set.
    /// 完整替换偏好集合。
    preferences: Vec<PublicAiCustomModelProtocolPreferenceItem>,
}

/// Saves or restores the direct provider-wide model settings for one custom provider.
/// 保存或还原一个自定义供应商的直接供应商级模型设置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomProviderModelDefaultsSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete provider layer, or null to restore the in-memory system baseline.
    /// 完整供应商层；为 null 时还原内存中的系统基线。
    defaults: Option<CustomProviderModelDefaults>,
}

/// Saves or clears one reviewed common-template decision.
/// 保存或清除一项已审查公共模板决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelTemplateDecisionSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Replacement decision, or null to return to an undecided state.
    /// 替换决策；为 null 时恢复未决定状态。
    decision: Option<CustomModelTemplateDecision>,
}

/// Queries offline common-capability template suggestions for one model identifier.
/// 查询一个模型标识的离线公共能力模板建议。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelTemplateSuggestionsRequest {
    /// Exact model identifier entered by the user or discovered from the provider.
    /// 用户输入或供应商发现的精确模型标识。
    model_id: String,
}

/// Queries offline template suggestions for a bounded set of model identifiers.
/// 为一组有界模型标识查询离线模板建议。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelTemplateSuggestionsBatchRequest {
    /// Exact model identifiers shown by one custom-provider catalog union.
    /// 一个自定义供应商目录并集中显示的精确模型标识。
    model_ids: Vec<String>,
}

/// Carries offline suggestions for one exact model without map-key parsing.
/// 承载一个精确模型的离线建议且无需解析映射键。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiCustomModelTemplateSuggestionsRecord {
    /// Exact model identifier used for the lookup.
    /// 用于查询的精确模型标识。
    model_id: String,
    /// Ordered immutable template candidates.
    /// 按顺序排列的不可变模板候选。
    suggestions: Vec<CommonModelTemplateSuggestion>,
}

/// Stores one model decision inside an atomic provider-scoped batch.
/// 在一个供应商作用域原子批次中保存一项模型决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelTemplateDecisionBatchItem {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Explicit reviewed decision to persist.
    /// 需要持久化的显式已审查决策。
    decision: CustomModelTemplateDecision,
}

/// Saves a bounded set of reviewed template decisions atomically.
/// 原子保存一组有界的已审查模板决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelTemplateDecisionsBatchSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Distinct reviewed decisions.
    /// 互不重复的已审查决策。
    decisions: Vec<PublicAiCustomModelTemplateDecisionBatchItem>,
}

/// Saves or clears one per-account manual-model availability difference.
/// 保存或清除一项逐账号手工模型可用性差异。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomModelConnectionAvailabilitySaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact manually declared model identifier.
    /// 精确手工声明模型标识。
    model_id: String,
    /// Explicit state, or null to restore discovery-derived behavior.
    /// 显式状态；为 null 时恢复发现派生行为。
    enabled: Option<bool>,
}

/// Executes one explicit minimal model request and releases a pending credential only on success.
/// 执行一次显式最小模型请求，并且仅在成功后解除待验证凭据隔离。
///
/// # Parameters
/// 参数
/// `app` publishes the eligible workbench after success, `state` owns runtime/configuration, and
/// `request` freezes the exact provider, credential, and model selected by the user.
/// `app` 在成功后发布可用工作台，`state` 持有运行时与配置，`request` 冻结用户选择的精确供应商、凭据与模型。
///
/// # Returns
/// 返回值
/// Returns compact response facts only after a terminal stream with visible text.
/// 仅在流以可见文本成功终结后返回紧凑响应事实。
pub(crate) async fn service_validate_public_ai_custom_model(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelValidationRequest,
) -> Result<PublicAiCustomModelValidationResult, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    if connection.provider_id != request.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to custom provider `{}`.",
            request.connection_id, request.provider_id
        ));
    }

    // Explicit validation resolves against one captured generation without publishing temporary access.
    // 显式验证在一个已捕获代际上解析，不发布临时访问权限。
    let selection = runtime.resolve_connection_model_validation_selection(
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    let output_limit = selection
        .model
        .capabilities
        .output_limit
        .unwrap_or(256)
        .clamp(1, 256);
    let reasoning_effort = selection
        .model
        .capabilities
        .reasoning_efforts
        .contains(&ReasoningEffort::None)
        .then_some(ReasoningEffort::None);
    // Hidden reasoning is meaningful only when the exact route activates a reasoning effort.
    // 仅当精确路由启用了思考档位时，隐藏思考才有明确语义。
    let reasoning_output = if reasoning_effort.is_some() {
        ReasoningOutputPolicy::Hidden
    } else {
        ReasoningOutputPolicy::ProviderDefault
    };
    // Native attempt persistence requires the logical execution identity to remain a canonical UUID.
    // 原生真实尝试持久化要求逻辑执行标识保持为规范 UUID。
    let execution_id = uuid::Uuid::new_v4().to_string();
    let execution_request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        // Validation belongs to no session, so it is its own scope: providers that route by a
        // session header or derive prefix identity from the cache scope reject a request without one.
        // 验证不属于任何会话，因此以自身为作用域：按会话请求头路由或从缓存作用域派生前缀标识的
        // 供应商会拒绝没有作用域的请求。
        cache_scope_id: Some(execution_id.clone()),
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: String::from("Reply with exactly OK."),
            }],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: selection.overrides.prompt_cache.clone().unwrap_or_default(),
        reasoning_effort,
        reasoning_output,
        model_parameters: selection.overrides.model_parameters.clone(),
        resources: Vec::new(),
        output_token_limit: Some(output_limit),
    };
    let configured_inference_service = configured_inference_service_for_settings(&state)?;
    let response_text = configured_inference_service
        .execute_exact_selection_text(
            crate::configured_inference::ExactSelectionTextPurpose::ModelValidation,
            &selection,
            execution_request,
            execution_id,
            None,
        )
        .await
        .map_err(|error| error.to_string())?;

    let provider_id = request.provider_id.clone();
    let connection_id = request.connection_id.clone();
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom model validation result",
        |configuration| {
            let current_connection = configuration
                .connection_graph
                .connections
                .get(connection_id.as_str())
                .ok_or_else(|| format!("Connection `{connection_id}` is no longer configured."))?;
            if current_connection.provider_id != provider_id
                || current_connection.service_entry_id
                    != selection.route.connection.service_entry_id
                || current_connection.credential_profile_id
                    != selection.route.connection.credential_profile_id
            {
                return Err(format!(
                    "Connection `{connection_id}` changed while model validation was running."
                ));
            }
            configuration.set_connection_pending_validation(connection_id.as_str(), false)?;
            configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .entry(provider_id.clone())
                .or_insert_with(|| connection_id.clone());
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(PublicAiCustomModelValidationResult {
        provider_id: request.provider_id,
        connection_id: request.connection_id,
        model_id: request.model_id,
        response_characters: response_text.chars().count(),
        response_text,
    })
}

/// Replaces the user-owned model definitions for one configured provider.
/// 替换一个已配置供应商的用户自有模型定义。
///
/// # Parameters
/// 参数
///
/// `app` publishes the refreshed workbench, `state` owns persisted configuration and runtime
/// catalogs, and `request` carries the complete replacement set.
/// `app` 发布刷新的工作台，`state` 持有持久化配置与运行时目录，`request` 携带完整替换集合。
///
/// # Returns
/// 返回值
///
/// Returns the rebuilt browser snapshot after validation and overlay reapplication.
/// 校验并重新应用覆盖后返回重建的浏览器快照。
pub(crate) async fn service_save_public_ai_custom_models(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelsSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let system_manifest = runtime.system_manifest(request.provider_id.as_str());
    if system_manifest.is_none() {
        let provider = current_configuration
            .custom_providers
            .get(request.provider_id.as_str())
            .ok_or_else(|| format!("Provider `{}` is not configured.", request.provider_id))?;
        if provider.kind != ProviderKind::Custom {
            return Err(format!(
                "Provider `{}` is not a user-defined provider.",
                request.provider_id
            ));
        }
    }

    let mut replacement = BTreeMap::new();
    for model in request.models {
        if let Some(manifest) = system_manifest.as_deref() {
            validate_system_manual_model_definition(manifest, &model)?;
        } else {
            model.validate()?;
            if model.system_binding.is_some() {
                return Err(format!(
                    "Custom provider model `{}` cannot declare a system service binding.",
                    model.id
                ));
            }
            // Custom-provider routes currently publish no provider-specific video input contract.
            // 自定义供应商路由目前不发布供应商专属的视频输入契约。
            if model
                .capabilities
                .input_resources
                .contains(&vulcan_ai_contract::ResourceKind::Video)
            {
                return Err(format!(
                    "Custom provider model `{}` cannot enable video input without a declared provider route.",
                    model.id
                ));
            }
        }
        let model_id = model.id.clone();
        if replacement.insert(model_id.clone(), model).is_some() {
            return Err(format!(
                "Custom provider `{}` contains duplicate model `{model_id}`.",
                request.provider_id
            ));
        }
    }
    if system_manifest.is_some() && !request.template_decisions.is_empty() {
        return Err(String::from(
            "System manual models do not accept custom-provider template decisions.",
        ));
    }
    // seen_template_decisions rejects ambiguous duplicate decisions before persistence begins.
    // seen_template_decisions 在开始持久化前拒绝含义不明确的重复决策。
    let mut seen_template_decisions = BTreeSet::new();
    // template_decisions contains validated composite keys ready for the single configuration transaction.
    // template_decisions 包含已校验的组合键，可直接进入单次配置事务。
    let mut template_decisions = Vec::with_capacity(request.template_decisions.len());
    for item in request.template_decisions {
        validate_non_empty("templateDecisions[].modelId", item.model_id.as_str())?;
        if !seen_template_decisions.insert(item.model_id.clone()) {
            return Err(format!(
                "templateDecisions contains duplicate model `{}`.",
                item.model_id
            ));
        }
        if !replacement.contains_key(item.model_id.as_str()) {
            return Err(format!(
                "templateDecisions references model `{}` outside the replacement model set.",
                item.model_id
            ));
        }
        validate_custom_template_decision(item.model_id.as_str(), &item.decision)?;
        template_decisions.push((
            PersistedAiConfiguration::provider_model_key(
                request.provider_id.as_str(),
                item.model_id.as_str(),
            ),
            item.decision,
        ));
    }
    let removed_model_ids = current_configuration
        .custom_models
        .get(request.provider_id.as_str())
        .into_iter()
        .flat_map(|models| models.keys())
        .filter(|model_id| !replacement.contains_key(model_id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let provider_id = request.provider_id;
    // candidate_configuration is authoritative for deciding whether a removed manual overlay
    // also removes the executable model from an exact credential catalog.
    // candidate_configuration 是判断移除手工覆盖后模型是否也从精确凭据目录消失的权威状态。
    let mut candidate_configuration = current_configuration.clone();
    apply_custom_model_replacement(
        &mut candidate_configuration,
        provider_id.as_str(),
        &replacement,
        &removed_model_ids,
        &template_decisions,
    );
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let referenced_removed_models = referenced_custom_model_ids(
        runtime.as_ref(),
        &candidate_configuration,
        provider_id.as_str(),
        &removed_model_ids,
    )?;
    // Session locks are runtime-owned and must block deletion even when another account still exposes the same model.
    // 会话锁定关系由运行时持有，即使其他账号仍暴露同名模型，也必须阻止删除。
    // locked_session_model_ids is copied while holding service state so catalog resolution never
    // nests runtime locks under the workspace mutex.
    // locked_session_model_ids 在持有服务状态锁时完成复制，避免在工作区互斥锁内嵌套目录锁。
    let locked_session_model_ids = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .into_iter()
            .flat_map(|workspace| {
                workspace
                    .session_runtime_state
                    .values()
                    .chain(workspace.archived_session_runtime_state.values())
            })
            .filter(|runtime_state| {
                runtime_state.active_provider_id.as_deref() == Some(provider_id.as_str())
                    && runtime_state
                        .active_model_id
                        .as_deref()
                        .is_some_and(|model_id| removed_model_ids.contains(model_id))
            })
            .filter_map(|runtime_state| runtime_state.active_model_id.clone())
            .collect::<BTreeSet<_>>()
    };
    if !locked_session_model_ids.is_empty() {
        return Err(format!(
            "Custom models are locked by active or archived sessions and cannot be removed: {}. Switch those sessions to another model first.",
            locked_session_model_ids
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !referenced_removed_models.is_empty() {
        return Err(format!(
            "Custom models still referenced by an active or internal route cannot be removed: {}.",
            referenced_removed_models
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom models",
        |configuration| {
            apply_custom_model_replacement(
                configuration,
                provider_id.as_str(),
                &replacement,
                &removed_model_ids,
                &template_decisions,
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Replaces the per-model protocol preferences for one custom provider.
/// 替换一个自定义供应商的逐模型协议偏好。
///
/// # Parameters
/// 参数
/// `app` publishes the refreshed workbench, `state` owns persisted configuration and runtime
/// catalogs, and `request` carries the complete replacement preference set.
/// `app` 发布刷新的工作台，`state` 持有持久化配置与运行时目录，`request` 携带完整替换偏好集合。
///
/// # Returns
/// 返回值
/// Returns the rebuilt browser snapshot after validation and persistence.
/// 校验并持久化后返回重建的浏览器快照。
pub(crate) async fn service_save_public_ai_custom_model_protocol_preferences(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelProtocolPreferencesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let provider = current_configuration
        .custom_providers
        .get(request.provider_id.as_str())
        .ok_or_else(|| {
            format!(
                "Custom provider `{}` is not configured.",
                request.provider_id
            )
        })?;
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Provider `{}` is not a user-defined provider.",
            request.provider_id
        ));
    }
    // enabled_protocols is the authoritative set every explicit preference must reference.
    // enabled_protocols 是每个显式偏好必须引用的权威协议集合。
    let enabled_protocols = provider
        .service_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .map(|route| route.protocol_profile_id.clone())
        .collect::<BTreeSet<_>>();
    if enabled_protocols.is_empty() {
        return Err(String::from(
            "Custom provider must declare at least one enabled protocol before saving model preferences.",
        ));
    }
    let preferences = validate_custom_model_protocol_preferences(
        request.provider_id.as_str(),
        &enabled_protocols,
        &request.preferences,
    )?;
    let provider_id = request.provider_id;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom model protocol preferences",
        |configuration| {
            let provider_model_prefix = format!("{}:{provider_id}:", provider_id.len());
            configuration
                .custom_model_protocol_preferences
                .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
            configuration
                .custom_model_protocol_preferences
                .extend(preferences.clone());
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    // Rebuilding the catalogs re-freezes every custom protocol binding at the activation
    // boundary so the saved preferences take effect without a manual refresh (plan §13.6).
    // 重建目录会在激活边界重新冻结全部自定义协议绑定，使已保存偏好无需手动刷新即生效
    // （方案 §13.6）。
    drop(transaction);
    Ok(snapshot)
}

/// Applies one complete custom-model replacement to a candidate or durable configuration.
/// 将一份完整自定义模型替换应用到候选配置或持久配置。
///
/// # Parameters
/// 参数
/// `configuration` is the mutation target, `provider_id` owns all records, `replacement` is the
/// complete model set, `removed_model_ids` scopes cleanup, and `template_decisions` contains
/// already validated composite keys.
/// `configuration` 是修改目标，`provider_id` 持有全部记录，`replacement` 是完整模型集合，
/// `removed_model_ids` 限定清理范围，`template_decisions` 包含已经校验的组合键。
///
/// # Returns
/// 返回值
/// Updates the supplied configuration in place without publishing or returning a value.
/// 原地更新传入配置，不发布状态且不返回值。
fn apply_custom_model_replacement(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    replacement: &BTreeMap<String, CustomModelDefinition>,
    removed_model_ids: &BTreeSet<String>,
    template_decisions: &[(String, CustomModelTemplateDecision)],
) {
    if replacement.is_empty() {
        configuration.custom_models.remove(provider_id);
    } else {
        configuration
            .custom_models
            .insert(provider_id.to_string(), replacement.clone());
    }
    for model_id in removed_model_ids {
        configuration.remove_model_owned_settings(provider_id, model_id.as_str());
    }
    for (key, decision) in template_decisions {
        configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision.clone());
    }
}

/// Saves provider-wide model defaults and rebuilds every affected in-memory catalog.
/// 保存供应商级模型默认值并重建所有受影响的内存目录。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` replaces the provider model layer.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 替换供应商模型设置层。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validation and persistence, or an explicit failure.
/// 校验并持久化后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_custom_provider_model_defaults(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderModelDefaultsSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    if let Some(defaults) = request.defaults.as_ref() {
        defaults.validate()?;
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    // provider_id identifies the exact custom-provider settings layer being replaced.
    // provider_id 标识要被替换的精确自定义供应商设置层。
    let provider_id = request.provider_id;
    // defaults collapses a value equal to the system baseline into an absent provider layer.
    // defaults 将与系统基线相同的值折叠为不存在的供应商层。
    let defaults = request
        .defaults
        .filter(|defaults| *defaults != CustomProviderModelDefaults::default());
    let mut candidate_configuration = current_configuration.clone();
    replace_custom_provider_model_defaults(
        &mut candidate_configuration,
        provider_id.as_str(),
        defaults.as_ref(),
    );
    candidate_configuration.validate_context_window_policy()?;
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom provider model defaults",
        |configuration| {
            replace_custom_provider_model_defaults(
                configuration,
                provider_id.as_str(),
                defaults.as_ref(),
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Replaces or removes one explicit custom-provider model-settings layer.
/// 替换或移除一层显式自定义供应商模型设置。
///
/// # Parameters
/// 参数
/// `configuration` owns persisted state, `provider_id` identifies the layer, and `defaults`
/// contains the replacement or is absent for restoration.
/// `configuration` 持有持久化状态，`provider_id` 标识设置层，`defaults` 包含替换值或在还原时为空。
///
/// # Returns
/// 返回值
/// Updates or removes the supplied provider layer in place without returning a value.
/// 原地更新或移除传入的供应商层，不返回值。
fn replace_custom_provider_model_defaults(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    defaults: Option<&CustomProviderModelDefaults>,
) {
    // A record equal to the system baseline carries no user choice and is stored as absence.
    // 与系统基线相同的记录不含用户选择，按缺省保存。
    if let Some(defaults) =
        defaults.filter(|defaults| **defaults != CustomProviderModelDefaults::default())
    {
        configuration
            .custom_provider_model_defaults
            .insert(provider_id.to_string(), defaults.clone());
    } else {
        configuration
            .custom_provider_model_defaults
            .remove(provider_id);
    }
}

/// Returns offline template suggestions without network access or persistent mutation.
/// 在不访问网络且不修改持久化状态的情况下返回离线模板建议。
///
/// # Parameters
/// 参数
/// `request` carries the exact model identifier for the immutable local lookup.
/// `request` 携带不可变本地查询使用的精确模型标识。
///
/// # Returns
/// 返回值
/// Returns ordered offline suggestions, or a validation error for an invalid identifier.
/// 返回有序离线建议，或无效标识的校验错误。
pub(crate) fn service_public_ai_custom_model_template_suggestions(
    request: PublicAiCustomModelTemplateSuggestionsRequest,
) -> Result<Vec<CommonModelTemplateSuggestion>, String> {
    validate_non_empty("modelId", request.model_id.as_str())?;
    Ok(vulcan_provider_adapters::suggest_common_model_templates(
        request.model_id.as_str(),
    ))
}

/// Returns offline template suggestions for distinct model identifiers in one bounded request.
/// 在一次有界请求中返回互不重复模型标识的离线模板建议。
///
/// # Parameters
/// 参数
/// `request` contains exact model identifiers from one browser-visible provider union.
/// `request` 包含一个浏览器可见供应商并集中的精确模型标识。
///
/// # Returns
/// 返回值
/// Ordered suggestion records without network access or persistent mutation.
/// 返回有序建议记录，且不访问网络、不修改持久化状态。
pub(crate) fn service_public_ai_custom_model_template_suggestions_batch(
    request: PublicAiCustomModelTemplateSuggestionsBatchRequest,
) -> Result<Vec<PublicAiCustomModelTemplateSuggestionsRecord>, String> {
    if request.model_ids.is_empty() {
        return Err(String::from(
            "modelIds must contain at least one model identifier.",
        ));
    }
    if request.model_ids.len() > MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE {
        return Err(format!(
            "modelIds exceeds the maximum batch size of {MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE}."
        ));
    }
    let mut seen = BTreeSet::new();
    let mut records = Vec::with_capacity(request.model_ids.len());
    for model_id in request.model_ids {
        validate_non_empty("modelIds[]", model_id.as_str())?;
        if !seen.insert(model_id.clone()) {
            return Err(format!("modelIds contains duplicate model `{model_id}`."));
        }
        records.push(PublicAiCustomModelTemplateSuggestionsRecord {
            suggestions: vulcan_provider_adapters::suggest_common_model_templates(
                model_id.as_str(),
            ),
            model_id,
        });
    }
    Ok(records)
}

/// Saves one explicit template decision and rebuilds affected model snapshots.
/// 保存一项显式模板决策并重建受影响的模型快照。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` declares one reviewed decision or restoration.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 声明一项已审查决策或还原操作。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after one validated atomic update, or an explicit failure.
/// 一次已校验原子更新后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_custom_model_template_decision(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelTemplateDecisionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    validate_custom_provider_model_exists(
        runtime.as_ref(),
        &current_configuration,
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    if let Some(decision) = request.decision.as_ref() {
        validate_custom_template_decision(request.model_id.as_str(), decision)?;
    }
    let key = PersistedAiConfiguration::provider_model_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    let decision = request.decision;
    let mut candidate_configuration = current_configuration.clone();
    if let Some(decision) = decision.clone() {
        candidate_configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision);
    } else {
        candidate_configuration
            .custom_model_template_decisions
            .remove(key.as_str());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom model template decision",
        |configuration| {
            if let Some(decision) = decision.clone() {
                configuration
                    .custom_model_template_decisions
                    .insert(key.clone(), decision);
            } else {
                configuration
                    .custom_model_template_decisions
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Atomically saves a bounded set of explicit template decisions and rebuilds catalogs once.
/// 原子保存一组有界显式模板决策，并且只重建一次目录。
///
/// # Parameters
/// 参数
/// `app` publishes the rebuilt workbench, `state` owns runtime and persistence, and `request`
/// identifies one custom provider plus distinct reviewed decisions.
/// `app` 发布重建后的工作台，`state` 持有运行时与持久化状态，`request`
/// 标识一个自定义供应商及互不重复的已审查决策。
///
/// # Returns
/// 返回值
/// The rebuilt browser snapshot after all decisions pass validation and persist atomically.
/// 所有决策通过校验并原子持久化后返回重建的浏览器快照。
pub(crate) async fn service_save_public_ai_custom_model_template_decisions_batch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelTemplateDecisionsBatchSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    if request.decisions.is_empty() {
        return Err(String::from(
            "decisions must contain at least one reviewed template decision.",
        ));
    }
    if request.decisions.len() > MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE {
        return Err(format!(
            "decisions exceeds the maximum batch size of {MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE}."
        ));
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    let mut seen = BTreeSet::new();
    let mut decisions = Vec::with_capacity(request.decisions.len());
    for item in request.decisions {
        validate_non_empty("decisions[].modelId", item.model_id.as_str())?;
        if !seen.insert(item.model_id.clone()) {
            return Err(format!(
                "decisions contains duplicate model `{}`.",
                item.model_id
            ));
        }
        validate_custom_provider_model_exists(
            runtime.as_ref(),
            &current_configuration,
            request.provider_id.as_str(),
            item.model_id.as_str(),
        )?;
        validate_custom_template_decision(item.model_id.as_str(), &item.decision)?;
        decisions.push((
            PersistedAiConfiguration::provider_model_key(
                request.provider_id.as_str(),
                item.model_id.as_str(),
            ),
            item.decision,
        ));
    }
    let mut candidate_configuration = current_configuration.clone();
    for (key, decision) in &decisions {
        candidate_configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision.clone());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom model template decisions in batch",
        |configuration| {
            for (key, decision) in decisions.clone() {
                configuration
                    .custom_model_template_decisions
                    .insert(key, decision);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one per-account manual-model availability difference and rebuilds effective catalogs.
/// 保存一项逐账号手工模型可用性差异并重建有效目录。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` selects one account/model availability difference.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 选择一项账号模型可用性差异。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validating provider ownership and publication, or an explicit failure.
/// 校验供应商归属与发布状态后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_custom_model_connection_availability(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelConnectionAvailabilitySaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential connection `{}` is not configured.",
                request.connection_id
            )
        })?;
    validate_custom_provider_identity(&current_configuration, connection.provider_id.as_str())?;
    if current_configuration
        .custom_model(connection.provider_id.as_str(), request.model_id.as_str())
        .is_none()
    {
        return Err(format!(
            "Custom provider `{}` has no manual model `{}`.",
            connection.provider_id, request.model_id
        ));
    }
    validate_custom_model_connection_availability_choice(
        &current_configuration,
        connection.provider_id.as_str(),
        request.model_id.as_str(),
        request.enabled,
    )?;
    let key = PersistedAiConfiguration::connection_model_key(
        request.connection_id.as_str(),
        request.model_id.as_str(),
    );
    let enabled = request.enabled;
    let mut candidate_configuration = current_configuration.clone();
    if let Some(enabled) = enabled {
        candidate_configuration
            .custom_model_connection_enabled_overrides
            .insert(key.clone(), enabled);
    } else {
        candidate_configuration
            .custom_model_connection_enabled_overrides
            .remove(key.as_str());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the custom model account availability",
        |configuration| {
            if let Some(enabled) = enabled {
                configuration
                    .custom_model_connection_enabled_overrides
                    .insert(key.clone(), enabled);
            } else {
                configuration
                    .custom_model_connection_enabled_overrides
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Rejects account membership choices that would reopen a provider-disabled custom model.
/// 拒绝会重新开启供应商已关闭自定义模型的账号成员关系选择。
///
/// # Parameters
/// 参数
/// `configuration` owns provider publication state; `provider_id`, `model_id`, and `enabled`
/// identify the exact requested account membership difference.
/// `configuration` 持有供应商发布状态；`provider_id`、`model_id` 与 `enabled` 标识精确的账号成员关系差异。
///
/// # Returns
/// 返回值
/// Success for restoration or disablement, and for enablement only while the provider publishes the model.
/// 还原或关闭时返回成功；仅当供应商发布该模型时允许开启。
fn validate_custom_model_connection_availability_choice(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    enabled: Option<bool>,
) -> Result<(), String> {
    if enabled == Some(true) && !configuration.model_is_enabled(provider_id, model_id) {
        return Err(format!(
            "Provider `{provider_id}` has disabled model `{model_id}`; an account cannot enable it. Enable the model at provider level first."
        ));
    }
    Ok(())
}

/// Finds removed custom models still owned by active or internal inference routes.
/// 查找仍被当前选择或内部推理路由持有的已移除自定义模型。
///
/// # Parameters
/// 参数
/// `runtime` resolves exact catalogs, `configuration` is the candidate state, and `provider_id` with `removed_model_ids` scopes the removal.
/// `runtime` 解析精确目录，`configuration` 是候选状态，`provider_id` 与 `removed_model_ids` 限定移除范围。
///
/// # Returns
/// 返回值
/// Returns removed models still required by exact active/internal routes, or a catalog resolution error.
/// 返回仍被精确当前选择或内部路由需要的已移除模型，或目录解析错误。
fn referenced_custom_model_ids(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    removed_model_ids: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let mut referenced = BTreeSet::new();
    if let Some(active) = configuration.active.as_ref()
        && active.provider_id == provider_id
        && removed_model_ids.contains(active.model_id.as_str())
        && !model_remains_on_connection(
            runtime,
            configuration,
            Some(active.connection_id.as_str()),
            active.model_id.as_str(),
        )?
    {
        referenced.insert(active.model_id.clone());
    }
    for profile in configuration.inference_profiles.values() {
        let routes = profile
            .llm_routes
            .values()
            .chain(profile.embedding.iter())
            .chain(profile.rerank.iter())
            .chain(profile.decision.iter());
        for route in routes {
            if route.provider_id == provider_id
                && removed_model_ids.contains(route.model_id.as_str())
                && !model_remains_on_connection(
                    runtime,
                    configuration,
                    Some(route.connection_id.as_str()),
                    route.model_id.as_str(),
                )?
            {
                referenced.insert(route.model_id.clone());
            }
        }
    }
    Ok(referenced)
}

/// Checks whether a candidate configuration keeps a model on one exact referenced connection.
/// 检查候选配置是否在一个精确被引用连接上保留模型。
///
/// # Parameters
/// 参数
/// `runtime` owns immutable source catalogs, `configuration` is the candidate state,
/// `connection_id` is the referenced credential when available, and `model_id` is exact.
/// `runtime` 持有不可变源目录，`configuration` 是候选状态，`connection_id` 是可用时的
/// 被引用凭据，`model_id` 为精确模型标识。
///
/// # Returns
/// 返回值
/// `true` only when the connection still exists and its fully resolved catalog contains the model.
/// 仅当连接仍存在且其完整解析目录包含该模型时返回 `true`。
fn model_remains_on_connection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: Option<&str>,
    model_id: &str,
) -> Result<bool, String> {
    let Some(connection_id) = connection_id else {
        return Ok(false);
    };
    if !configuration
        .connection_graph
        .connections
        .contains_key(connection_id)
    {
        return Ok(false);
    }
    runtime.connection_catalog_contains_model(configuration, connection_id, model_id)
}

/// Validates that one custom-provider model exists in a current catalog or manual declaration.
/// 校验一个自定义供应商模型存在于当前目录或手工声明中。
///
/// # Parameters
/// 参数
/// `runtime` resolves support from `configuration`; `provider_id` and `model_id` identify the exact model.
/// `runtime` 根据 `configuration` 解析支持状态；`provider_id` 与 `model_id` 标识精确模型。
///
/// # Returns
/// 返回值
/// Returns success only when the provider currently supports the model, otherwise an explicit error.
/// 仅当供应商当前支持该模型时返回成功，否则返回显式错误。
fn validate_custom_provider_model_exists(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<(), String> {
    if runtime.provider_model_is_supported(configuration, provider_id, model_id)? {
        Ok(())
    } else {
        Err(format!(
            "Custom provider `{provider_id}` does not expose model `{model_id}`."
        ))
    }
}

/// Validates a persisted template decision against the immutable local suggestion catalog.
/// 根据不可变本地建议目录校验持久化模板决策。
///
/// # Parameters
/// 参数
/// `model_id` selects local suggestions; `decision` supplies the reviewed key, revision, and optional selection.
/// `model_id` 选择本地建议；`decision` 提供已审查的键、修订号及可选选择内容。
///
/// # Returns
/// 返回值
/// Returns success only when the validated decision still matches the immutable suggestion catalog.
/// 仅当已校验决策仍匹配不可变建议目录时返回成功。
fn validate_custom_template_decision(
    model_id: &str,
    decision: &CustomModelTemplateDecision,
) -> Result<(), String> {
    decision.validate()?;
    let suggestions = vulcan_provider_adapters::suggest_common_model_templates(model_id);
    let matches = match decision {
        CustomModelTemplateDecision::Applied { selection } => suggestions.iter().any(|candidate| {
            candidate.template_key == selection.template_key
                && candidate.template_revision == selection.template_revision
                && candidate.selection() == *selection
        }),
        CustomModelTemplateDecision::Rejected {
            template_key,
            template_revision,
        } => suggestions.iter().any(|candidate| {
            candidate.template_key == *template_key
                && candidate.template_revision == *template_revision
        }),
    };
    if matches {
        Ok(())
    } else {
        Err(format!(
            "The reviewed common template decision no longer matches model `{model_id}`."
        ))
    }
}

/// Validates and keys one complete per-model protocol preference set.
/// 校验并建键一组完整的逐模型协议偏好。
///
/// # Parameters
/// 参数
/// `provider_id` labels the exact custom provider, `enabled_protocols` is its authoritative
/// enabled protocol set, and `preferences` is the complete replacement set.
/// `provider_id` 标识精确自定义供应商，`enabled_protocols` 是其权威启用协议集合，
/// `preferences` 是完整替换集合。
///
/// # Returns
/// 返回值
/// A collision-resistant provider-model keyed map, or one actionable validation message.
/// 返回抗冲突的供应商模型组合键映射，或一条可操作的校验消息。
fn validate_custom_model_protocol_preferences(
    provider_id: &str,
    enabled_protocols: &BTreeSet<String>,
    preferences: &[PublicAiCustomModelProtocolPreferenceItem],
) -> Result<BTreeMap<String, CustomModelProtocolPreference>, String> {
    let mut keyed = BTreeMap::new();
    for item in preferences {
        validate_non_empty("preferences[].modelId", item.model_id.as_str())?;
        if let Some(protocol) = item.preference.explicit_protocol()
            && !enabled_protocols.contains(protocol.profile_id())
        {
            return Err(format!(
                "Model `{}` selects protocol `{}` which is not enabled for provider `{}`.",
                item.model_id,
                protocol.profile_id(),
                provider_id
            ));
        }
        let key = PersistedAiConfiguration::provider_model_key(provider_id, item.model_id.as_str());
        if keyed.insert(key, item.preference).is_some() {
            return Err(format!(
                "Custom provider `{provider_id}` contains duplicate preference for model `{}`.",
                item.model_id
            ));
        }
    }
    Ok(keyed)
}

#[cfg(test)]
mod tests;

//! Model publication, account access, capability layers, and protocol execution settings.
//! 模型发布、账号访问、能力层与协议执行设置。

use serde::Deserialize;
use std::collections::BTreeSet;
use vulcan_provider_adapters::{
    ConnectionModel, ConnectionModelAccessPolicy, ConnectionModelCatalog, ModelCapabilityOverrides,
    ModelExecutionOverrides, PersistedAiConfiguration, PublicAiRuntime,
    model_capability_limit_issue,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    catalog_model_supports_text_conversation, first_enabled_model_selection,
    preferred_replacement_model_selection, public_ai_runtime_and_configuration,
    publication::{
        mutate_public_ai_configuration_and_publish_in_transaction,
        mutate_public_ai_configuration_and_publish_sessions_in_transaction,
    },
    reconcile_disabled_connection_sessions, reconcile_explicitly_revoked_session_model_selections,
    validate_non_empty,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Saves one exact model access choice for one credential account.
/// 保存一个凭据账号的一项精确模型访问选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionModelEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifier from that account's source membership.
    /// 来自该账号源成员关系的精确上游模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces the inherited model access state for one credential account.
/// 替换一个凭据账号继承的模型访问状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionModelsEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// State inherited by every current and future supported model.
    /// 当前与未来每个受支持模型继承的状态。
    enabled: bool,
}

/// Saves or clears one provider-model capability difference.
/// 保存或清除一个供应商模型能力差异。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelCapabilityOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Replacement difference, or null to restore immutable defaults.
    /// 替换差异；为 null 时恢复不可变默认值。
    overrides: Option<ModelCapabilityOverrides>,
}

/// Saves or restores one provider-level capability layer.
/// 保存或恢复一层供应商级能力配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderCapabilityOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Replacement difference, or null to restore inherited catalog defaults.
    /// 替换差异；为 null 时恢复继承的目录默认值。
    overrides: Option<ModelCapabilityOverrides>,
}

/// Saves local narrowing and model parameter values.
/// 保存本地收窄与模型参数值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact protocol profile identifier.
    /// 精确协议配置档标识。
    protocol_id: String,
    /// Complete replacement override value, or null to restore protocol defaults.
    /// 完整替换覆盖值；为 null 时还原协议默认值。
    overrides: Option<ModelExecutionOverrides>,
}

/// Saves one provider-model conversation choice independent from credentials and protocols.
/// 保存一个独立于凭据和协议的供应商模型会话选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiModelEnabledSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
    /// Restores the inherited value instead of creating or replacing a model layer.
    /// 恢复继承值，而不是创建或替换模型层。
    #[serde(default)]
    restore: bool,
}

/// Saves one conversation choice for several models of one provider.
/// 为同一供应商的多个模型保存一项会话选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelSetEnabledSaveRequest {
    /// Exact provider identifier owning every listed model.
    /// 持有全部所列模型的精确供应商标识。
    provider_id: String,
    /// Exact model identifiers; duplicates are ignored and the list must not be empty.
    /// 精确模型标识；重复项被忽略，列表不得为空。
    model_ids: Vec<String>,
    /// Requested enabled state for every listed model.
    /// 每个所列模型请求的启用状态。
    enabled: bool,
}

/// Saves one access choice for several models of one credential account.
/// 为一个凭据账号的多个模型保存一项访问选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionModelSetEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifiers; duplicates are ignored and the list must not be empty.
    /// 精确上游模型标识；重复项被忽略，列表不得为空。
    model_ids: Vec<String>,
    /// Requested access state for every listed model.
    /// 每个所列模型请求的访问状态。
    enabled: bool,
}

/// Reads one account model from the management catalog independently from its access switch.
/// 从独立于访问开关的管理目录读取一个账号模型。
///
/// # Parameters
/// 参数
/// `runtime` owns immutable source evidence; `configuration` owns manual membership;
/// `connection_id` and `model_id` select the exact account-model pair.
/// `runtime` 持有不可变源证据；`configuration` 持有手工成员关系；`connection_id` 与 `model_id`
/// 选择精确账号模型组合。
///
/// # Returns
/// 返回值
/// The resolved model, including invalid limits, or `None` when the identity is absent.
/// 返回包含无效限制证据的已解析模型；身份不存在时返回 `None`。
fn connection_management_model(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    model_id: &str,
) -> Result<Option<ConnectionModel>, String> {
    Ok(runtime
        .settings_catalog_snapshot(configuration)?
        .model_support_catalogs
        .get(connection_id)
        .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
        .cloned())
}

/// Reads one account's signed initial checkbox policy from a coherent settings generation.
/// 从一致的设置代际读取一个账号的签名初始勾选策略。
///
/// # Parameters
/// 参数
/// `runtime` owns the published manifest and source catalog; `configuration` owns the account;
/// `connection_id` selects its exact initial policy.
/// `runtime` 持有已发布清单与来源目录；`configuration` 持有账号；`connection_id` 选定其初始策略。
///
/// # Returns
/// 返回值
/// The signed account policy, or an explicit missing-account error.
/// 返回签名账号策略，账号缺失时显式报错。
fn initial_connection_model_policy(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
) -> Result<ConnectionModelAccessPolicy, String> {
    runtime
        .settings_catalog_snapshot(configuration)?
        .connection_model_initial_policies
        .remove(connection_id)
        .ok_or_else(|| format!("Account `{connection_id}` has no initial model policy."))
}

/// Applies an account-wide model default without allowing provider-disabled models to re-enter it.
/// 应用账号范围模型默认值，同时不允许供应商已关闭的模型重新进入账号默认值。
///
/// # Parameters
/// 参数
/// `runtime` owns manifest defaults; `configuration` is mutated in place; `connection_id` and
/// `provider_id` identify the exact account route; `enabled` is the requested account default;
/// `support_catalog` is the account's provider-resolved membership before access policies.
/// `runtime` 持有清单默认值；`configuration` 会被原地修改；`connection_id` 与 `provider_id`
/// 标识精确账号路由；`enabled` 是请求的账号默认值；`support_catalog` 是应用访问策略前的成员关系。
///
/// # Returns
/// 返回值
/// Returns success after applying the exact account policy, or an ownership/resolution error.
/// 应用精确账号策略后返回成功，或返回归属/解析错误。
fn set_connection_models_enabled_respecting_provider_access(
    runtime: &PublicAiRuntime,
    configuration: &mut PersistedAiConfiguration,
    connection_id: &str,
    provider_id: &str,
    enabled: bool,
    support_catalog: &ConnectionModelCatalog,
) -> Result<(), String> {
    // connection_provider_id validates the exact caller scope before mutating sparse access state.
    // connection_provider_id 在修改稀疏访问状态前校验精确调用方作用域。
    let connection_provider_id = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .map(|connection| connection.provider_id.as_str())
        .ok_or_else(|| format!("Credential connection `{connection_id}` is not configured."))?;
    if connection_provider_id != provider_id {
        return Err(format!(
            "Credential connection `{connection_id}` does not belong to provider `{provider_id}`."
        ));
    }
    configuration.set_connection_models_enabled(connection_id, enabled);
    if !enabled {
        return Ok(());
    }
    for model in &support_catalog.models {
        // Enable All never grants access to a model quarantined by its resolved limits.
        // “全部启用”绝不授予最终限制异常模型的访问资格。
        if model_capability_limit_issue(
            connection_id,
            model.id.as_str(),
            &model.capabilities,
            &model.capability_evidence,
        )
        .is_some()
            || !runtime.model_is_enabled_for_connection(
                configuration,
                connection_id,
                model.id.as_str(),
            )?
        {
            configuration.set_connection_model_enabled(connection_id, model.id.as_str(), false);
        }
    }
    Ok(())
}

/// Saves one credential-scoped model access decision and revokes affected exact session leases.
/// 保存一项凭据作用域模型访问决定并撤销受影响的精确会话租约。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` identifies exact account/model access.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 标识精确账号模型访问选择。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validated persistence and exact lease revocation, or an explicit failure.
/// 校验持久化并完成精确租约撤销后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_connection_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    save_public_ai_connection_model_access(
        app,
        state,
        request.connection_id.as_str(),
        &[request.model_id],
        request.enabled,
    )
    .await
}

/// Saves one access choice for several models of one credential account in a single transaction.
/// 在单个事务中为一个凭据账号的多个模型保存访问选择。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` lists the exact models and their new state.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 列出精确模型及其新状态。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot, or an explicit failure that leaves every model unchanged.
/// 返回重建快照；失败时显式报错且所有模型保持不变。
pub(crate) async fn service_save_public_ai_connection_model_set_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionModelSetEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    save_public_ai_connection_model_access(
        app,
        state,
        request.connection_id.as_str(),
        &request.model_ids,
        request.enabled,
    )
    .await
}

/// Applies one access state to a set of models of one credential account atomically.
/// 将一项访问状态原子地应用到一个凭据账号的一组模型。
///
/// # Parameters
/// 参数
/// `connection_id` owns every model in `model_ids`; `enabled` is applied to each of them.
/// `connection_id` 持有 `model_ids` 中的全部模型；`enabled` 应用到其中每个模型。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validated persistence and exact lease revocation, or an explicit failure.
/// 校验持久化并完成精确租约撤销后返回重建快照，或返回显式失败。
async fn save_public_ai_connection_model_access(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    connection_id: &str,
    model_ids: &[String],
    enabled: bool,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", connection_id)?;
    let model_ids = deduplicated_model_ids(model_ids)?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(connection_id)
        .ok_or_else(|| format!("Credential connection `{connection_id}` is not configured."))?;
    for model_id in &model_ids {
        let Some(management_model) = connection_management_model(
            runtime.as_ref(),
            &current_configuration,
            connection_id,
            model_id,
        )?
        else {
            return Err(format!(
                "Credential connection `{connection_id}` does not support model `{model_id}`."
            ));
        };
        if enabled
            && let Some(issue) = model_capability_limit_issue(
                connection_id,
                model_id,
                &management_model.capabilities,
                &management_model.capability_evidence,
            )
        {
            return Err(format!(
                "Model `{model_id}` cannot be enabled until its configuration is repaired: {issue}"
            ));
        }
        if enabled
            && !runtime.model_is_enabled_for_connection(
                &current_configuration,
                connection_id,
                model_id,
            )?
        {
            return Err(format!(
                "Provider `{}` has disabled model `{model_id}`; enable the provider model before enabling it for a credential connection.",
                connection.provider_id
            ));
        }
    }

    let mut candidate_configuration = current_configuration.clone();
    if !candidate_configuration
        .connection_model_access_policies
        .contains_key(connection_id)
    {
        let initial_policy = initial_connection_model_policy(
            runtime.as_ref(),
            &current_configuration,
            connection_id,
        )?;
        candidate_configuration
            .connection_model_access_policies
            .insert(connection_id.to_owned(), initial_policy);
    }
    for model_id in &model_ids {
        candidate_configuration.set_connection_model_enabled(connection_id, model_id, enabled);
    }
    let candidate_catalogs = runtime.resolved_model_catalog_snapshot(&candidate_configuration)?;
    // disabled_active_model_id names the active route model only when this save revokes it.
    // disabled_active_model_id 仅在本次保存撤销当前路由模型时给出该模型。
    let disabled_active_model_id = if enabled {
        None
    } else {
        candidate_configuration
            .active
            .as_ref()
            .filter(|selection| {
                selection.connection_id == connection_id
                    && model_ids.contains(selection.model_id.as_str())
            })
            .map(|selection| selection.model_id.clone())
    };
    if let Some(disabled_model_id) = disabled_active_model_id {
        candidate_configuration.active = preferred_replacement_model_selection(
            runtime.as_ref(),
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            disabled_model_id.as_str(),
        )?;
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "Failed to save the credential model enable state",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                let mut changed_session_ids = BTreeSet::new();
                if !enabled {
                    for model_id in &model_ids {
                        changed_session_ids.extend(reconcile_disabled_connection_sessions(
                            workspace,
                            connection_id,
                            Some(model_id),
                        ));
                    }
                }
                Ok(changed_session_ids)
            },
            "connection_model_access_revoked",
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one account-wide inherited model access state and revokes affected exact session leases.
/// 保存一项账号范围继承模型访问状态并撤销受影响的精确会话租约。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` selects the account-wide inherited access state.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 选择账号范围继承访问状态。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after provider-respecting access persistence and exact lease revocation, or an explicit failure.
/// 遵守供应商发布权威地保存访问状态并精确撤销租约后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_connection_models_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionModelsEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
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
    let support_catalog = runtime
        .settings_catalog_snapshot(&current_configuration)?
        .model_support_catalogs
        .get(request.connection_id.as_str())
        .cloned();
    let Some(support_catalog) = support_catalog else {
        return Err(format!(
            "Credential connection `{}` has no supported models.",
            request.connection_id
        ));
    };
    if support_catalog.models.is_empty() {
        return Err(format!(
            "Credential connection `{}` has no supported models.",
            request.connection_id
        ));
    }

    let mut candidate_configuration = current_configuration.clone();
    set_connection_models_enabled_respecting_provider_access(
        runtime.as_ref(),
        &mut candidate_configuration,
        request.connection_id.as_str(),
        connection.provider_id.as_str(),
        request.enabled,
        &support_catalog,
    )?;
    let candidate_catalogs = runtime.resolved_model_catalog_snapshot(&candidate_configuration)?;
    let active_was_disabled = !request.enabled
        && candidate_configuration
            .active
            .as_ref()
            .is_some_and(|selection| selection.connection_id == request.connection_id);
    let replacement = if active_was_disabled {
        let active_model_id = candidate_configuration
            .active
            .as_ref()
            .map(|selection| selection.model_id.clone())
            .unwrap_or_default();
        preferred_replacement_model_selection(
            runtime.as_ref(),
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            active_model_id.as_str(),
        )?
    } else {
        None
    };
    if active_was_disabled {
        candidate_configuration.active = replacement;
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "Failed to save the credential model enable states in batch",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if request.enabled {
                    BTreeSet::new()
                } else {
                    reconcile_disabled_connection_sessions(
                        workspace,
                        request.connection_id.as_str(),
                        None,
                    )
                })
            },
            "connection_model_access_revoked",
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one user-owned model capability difference.
/// 保存或清除一项用户自有模型能力差异。
///
/// # Parameters
/// 参数
///
/// `app` publishes the refreshed workbench, `state` owns catalogs and configuration, and
/// `request` identifies the provider model plus the optional replacement overlay.
/// `app` 发布刷新的工作台，`state` 持有目录与配置，`request` 标识供应商模型及可选替换覆盖。
///
/// # Returns
/// 返回值
///
/// Returns the rebuilt browser snapshot after the immutable source catalog is reprojected.
/// 在不可变源目录上重新投影后返回重建的浏览器快照。
pub(crate) async fn service_save_public_ai_model_capability_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelCapabilityOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    // overrides collapses an empty browser record into restoration semantics.
    // overrides 将空浏览器记录折叠为还原语义。
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if overrides.is_some()
        && !runtime.provider_conversation_model_is_supported(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
        )?
    {
        return Err(format!(
            "Provider `{}` does not support model `{}`.",
            request.provider_id, request.model_id
        ));
    }
    if overrides.is_some() {
        runtime.validate_model_capability_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
            overrides.as_ref(),
        )?;
    }
    let key = PersistedAiConfiguration::provider_model_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the model capability overrides",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration
                    .model_capability_overrides
                    .insert(key.clone(), overrides);
            } else {
                configuration
                    .model_capability_overrides
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-wide sparse capability layer or restores inherited defaults.
/// 保存一层供应商级稀疏能力配置，或恢复继承默认值。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` replaces or restores the provider capability layer.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 替换或还原供应商能力层。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after candidate validation and persistence, or an explicit failure.
/// 候选校验并持久化后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_provider_capability_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderCapabilityOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if overrides.is_some() {
        runtime.validate_provider_capability_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            overrides.as_ref(),
        )?;
    }
    let provider_id = request.provider_id.clone();
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the provider capability overrides",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration
                    .provider_capability_overrides
                    .insert(provider_id.clone(), overrides);
            } else {
                configuration
                    .provider_capability_overrides
                    .remove(provider_id.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-model-protocol override set after validating every matching account route.
/// 在校验全部匹配账号路由后保存一组供应商、模型与协议覆盖。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` identifies one provider/model/protocol override.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 标识一组供应商模型协议覆盖。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after validating matching routes and persisting the override, or an explicit failure.
/// 校验匹配路由并保存覆盖后返回重建快照，或返回显式失败。
pub(crate) async fn service_save_public_ai_model_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // overrides collapses an empty browser record into restoration semantics.
    // overrides 将空浏览器记录折叠为还原语义。
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let key = PersistedAiConfiguration::provider_model_protocol_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
        request.protocol_id.as_str(),
    );
    if let Some(overrides) = overrides.as_ref() {
        runtime.validate_model_execution_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
            request.protocol_id.as_str(),
            overrides,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the public AI model overrides",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration.model_overrides.insert(key.clone(), overrides);
            } else {
                configuration.model_overrides.remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Requested conversation publication change applied to every model of one save.
/// 一次保存中应用到每个模型的会话发布变更。
#[derive(Debug, Clone, Copy)]
enum ModelPublicationChange {
    /// Persists an explicit enabled state.
    /// 持久化显式启用状态。
    Set(bool),
    /// Restores the inherited value instead of creating or replacing a model layer.
    /// 恢复继承值，而不是创建或替换模型层。
    Restore,
}

/// Saves one provider-model conversation choice and reconciles invalid active selections.
/// 保存一个供应商模型会话选择，并协调失效的当前选择。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` declares model publication or restoration.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 声明模型发布选择或恢复操作。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after default resolution, global selection repair, and exact lease revocation.
/// 解析默认值、修正全局选择并精确撤销租约后返回重建快照。
pub(crate) async fn service_save_public_ai_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let change = if request.restore {
        ModelPublicationChange::Restore
    } else {
        ModelPublicationChange::Set(request.enabled)
    };
    save_public_ai_model_publication(
        app,
        state,
        request.provider_id.as_str(),
        &[request.model_id],
        change,
    )
    .await
}

/// Saves one conversation choice for several models of one provider in a single transaction.
/// 在单个事务中为同一供应商的多个模型保存会话选择。
///
/// # Parameters
/// 参数
/// `app` publishes changes, `state` owns configuration/runtime, and `request` lists the exact models and their new state.
/// `app` 发布变更，`state` 持有配置与运行时，`request` 列出精确模型及其新状态。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot, or an explicit failure that leaves every model unchanged.
/// 返回重建快照；失败时显式报错且所有模型保持不变。
pub(crate) async fn service_save_public_ai_model_set_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelSetEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    save_public_ai_model_publication(
        app,
        state,
        request.provider_id.as_str(),
        &request.model_ids,
        ModelPublicationChange::Set(request.enabled),
    )
    .await
}

/// Applies one conversation publication change to a set of provider models atomically.
/// 将一项会话发布变更原子地应用到一组供应商模型。
///
/// # Parameters
/// 参数
/// `provider_id` owns every model in `model_ids`; `change` is applied to each of them.
/// `provider_id` 持有 `model_ids` 中的全部模型；`change` 应用到其中每个模型。
///
/// # Returns
/// 返回值
/// Returns the rebuilt snapshot after default resolution, global selection repair, and exact lease revocation.
/// 解析默认值、修正全局选择并精确撤销租约后返回重建快照。
async fn save_public_ai_model_publication(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: &str,
    model_ids: &[String],
    change: ModelPublicationChange,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", provider_id)?;
    let model_ids = deduplicated_model_ids(model_ids)?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    // transaction keeps manifest publication, support validation, persistence, and response projection ordered.
    // transaction 让 Manifest 发布、支持校验、持久化与响应投影保持有序。
    let _transaction = begin_public_ai_configuration_transaction().await;
    // current_configuration is re-read only after transaction serialization.
    // current_configuration 仅在事务串行化后重新读取。
    let current_configuration = runtime.configuration_snapshot()?;
    // settings_catalog captures conversation model membership from one generation.
    // settings_catalog 从同一代际捕获会话模型成员关系。
    let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
    for model_id in &model_ids {
        // conversation_model_supported accepts only manual or catalog models that explicitly expose
        // text conversation. Provider extension targets have their own selection boundary.
        // conversation_model_supported 仅接受手工模型或明确公开文本会话的目录模型；Provider 扩展目标
        // 拥有独立选择边界。
        let conversation_model_supported = current_configuration
            .custom_model(provider_id, model_id)
            .is_some_and(|model| {
                current_configuration
                    .custom_providers
                    .contains_key(provider_id)
                    || vulcan_provider_adapters::model_supports_text_conversation(
                        &model.capabilities,
                    )
            })
            || current_configuration
                .connection_graph
                .connections
                .values()
                .filter(|connection| connection.provider_id == provider_id)
                .any(|connection| {
                    settings_catalog
                        .model_support_catalogs
                        .get(connection.id.as_str())
                        .is_some_and(|catalog| {
                            catalog.models.iter().any(|model| {
                                model.id == *model_id
                                    && catalog_model_supports_text_conversation(model)
                            })
                        })
                });
        if !conversation_model_supported {
            return Err(format!(
                "Provider `{provider_id}` does not expose model `{model_id}` for text conversation."
            ));
        }
    }
    // catalogs retains the same generation used for support validation and fallback selection.
    // catalogs 保留用于支持校验与回退选择的同一代际。
    let catalogs = settings_catalog.catalogs;
    // candidateConfiguration freezes restore/default semantics before any durable activation.
    // candidateConfiguration 在任何持久激活前冻结恢复与默认值语义。
    let mut candidate_configuration = current_configuration.clone();
    for model_id in &model_ids {
        match change {
            ModelPublicationChange::Restore => {
                candidate_configuration.restore_model_enabled(provider_id, model_id);
            }
            ModelPublicationChange::Set(enabled) => {
                candidate_configuration.set_model_enabled(provider_id, model_id, enabled);
            }
        }
    }
    // disabled_model_ids follows the resolved manifest default after a restore request.
    // disabled_model_ids 在恢复请求后遵循已解析 Manifest 默认值。
    let disabled_model_ids = model_ids
        .iter()
        .copied()
        .filter(|model_id| {
            !runtime.model_is_enabled(&candidate_configuration, provider_id, model_id)
        })
        .collect::<BTreeSet<_>>();
    let active_model_is_disabled =
        candidate_configuration
            .active
            .as_ref()
            .is_some_and(|selection| {
                selection.provider_id == provider_id
                    && disabled_model_ids.contains(selection.model_id.as_str())
            });
    if active_model_is_disabled {
        // fallback is one deterministic enabled route and never probes another protocol.
        // fallback 是一个确定性的已启用路由，绝不探测其他协议。
        candidate_configuration.active =
            first_enabled_model_selection(runtime.as_ref(), &candidate_configuration, &catalogs)?;
    }
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "Failed to save the public AI model enable state",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if disabled_model_ids.is_empty() {
                    BTreeSet::new()
                } else {
                    reconcile_explicitly_revoked_session_model_selections(
                        workspace,
                        |_session_id, runtime_state| {
                            runtime_state.active_provider_id.as_deref() == Some(provider_id)
                                && runtime_state
                                    .active_model_id
                                    .as_deref()
                                    .is_some_and(|model_id| disabled_model_ids.contains(model_id))
                        },
                    )
                })
            },
            "provider_model_access_revoked",
        )?;
    Ok(snapshot)
}

/// Validates and deduplicates the model identities of one set-based save.
/// 校验并去重一次集合保存中的模型身份。
///
/// # Parameters
/// 参数
/// `model_ids` is the caller-supplied list of exact model identifiers.
/// `model_ids` 是调用方提供的精确模型标识列表。
///
/// # Returns
/// 返回值
/// The sorted unique identities, or an error for an empty list or an empty identifier.
/// 返回已排序的唯一身份；列表为空或标识为空时返回错误。
pub(super) fn deduplicated_model_ids(model_ids: &[String]) -> Result<BTreeSet<&str>, String> {
    if model_ids.is_empty() {
        return Err(String::from("modelIds must list at least one model."));
    }
    for model_id in model_ids {
        validate_non_empty("modelId", model_id.as_str())?;
    }
    Ok(model_ids.iter().map(String::as_str).collect())
}

#[cfg(test)]
mod set_request_tests {
    use super::*;

    #[test]
    fn set_saves_collapse_duplicates_and_reject_empty_identities() {
        let ids = vec!["b".to_owned(), "a".to_owned(), "b".to_owned()];
        assert_eq!(
            deduplicated_model_ids(&ids).expect("valid ids"),
            BTreeSet::from(["a", "b"])
        );
        assert!(deduplicated_model_ids(&[]).is_err());
        assert!(deduplicated_model_ids(&["a".to_owned(), " ".to_owned()]).is_err());
    }

    #[test]
    fn set_requests_accept_only_the_list_shape() {
        serde_json::from_value::<PublicAiModelSetEnabledSaveRequest>(serde_json::json!({
            "providerId": "p", "modelIds": ["m"], "enabled": true
        }))
        .expect("list request should deserialize");
        assert!(
            serde_json::from_value::<PublicAiConnectionModelSetEnabledSaveRequest>(
                serde_json::json!({ "connectionId": "c", "modelId": "m", "enabled": true })
            )
            .is_err(),
            "a single-model field must not be accepted by a set request"
        );
    }
}

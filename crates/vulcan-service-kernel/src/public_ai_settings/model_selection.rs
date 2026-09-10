//! Exact model selection, global account fallback, and session lease-revocation rules.
//! 精确模型选择、全局账号回退与会话租约撤销规则。

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_provider_adapters::{
    ActiveAiModelSelection, ConnectionModelCatalog, PersistedAiConfiguration, PublicAiRuntime,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_workbench_model::SessionPendingModelSelection;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    public_ai_runtime_and_configuration,
    publication::{
        PublicAiWorkspaceReconciliation,
        mutate_public_ai_configuration_and_publish_using_in_transaction,
        publish_public_ai_workbench_with_workspace_reconciliation_and_commit,
    },
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Selects one exact public AI model and account.
/// 选择一个精确公共 AI 模型与账号。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelSelectionRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Optional persisted conversation receiving this model selection.
    /// 接收本次模型选择的可选持久化会话。
    #[serde(default)]
    conversation_id: Option<String>,
}

/// Plans an in-memory pending session selection without disturbing the currently frozen route.
/// 规划内存中的会话待生效选择，不干扰当前冻结路由。
///
/// # Parameters
/// 参数
/// `workspace` is the detached legacy mirror; `session_id` identifies its owner; `selection`
/// contains the exact provider, credential, and model requested for the next Provider boundary.
/// `workspace` 是独立的旧链镜像；`session_id` 标识其所有者；`selection`
/// 包含下一次 Provider 边界请求使用的精确供应商、凭据和模型。
///
/// # Returns
/// 返回值
/// An empty reconciliation plan because durable request authority is committed separately in V1 metadata.
/// 返回空协调计划，因为可靠请求权威由独立的 V1 metadata 提交持有。
/// An unchanged future target leaves the mirror untouched; missing session ownership is an explicit error.
/// 未来目标不变时不修改镜像；缺少会话所有权时返回显式错误。
fn plan_session_model_selection_for_next_provider_request(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    session_id: crate::SessionId,
    selection: &SessionPendingModelSelection,
) -> Result<PublicAiWorkspaceReconciliation, String> {
    // futureSelectionAlreadyMatches compares against pending authority first, then the active tuple.
    // futureSelectionAlreadyMatches 优先比较待生效权威，再比较当前活动三元组。
    let future_selection_already_matches = {
        // runtimeState is read before mutation so an idempotent picker action creates no duplicate record.
        // runtimeState 在修改前读取，确保幂等选择器操作不会创建重复记录。
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        if let Some(pending) = runtime_state.pending_model_selection.as_ref() {
            pending == selection
        } else {
            runtime_state.active_provider_id.as_deref() == Some(selection.provider_id.as_str())
                && runtime_state.active_connection_id.as_deref()
                    == Some(selection.connection_id.as_str())
                && runtime_state.active_model_id.as_deref() == Some(selection.model_id.as_str())
        }
    };
    if future_selection_already_matches {
        return Ok(PublicAiWorkspaceReconciliation::default());
    }
    // runtimeState receives only pending authority; the active tuple and route lease remain frozen.
    // runtimeState 仅接收待生效权威；当前活动三元组与路由租约保持冻结。
    let runtime_state = workspace
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    runtime_state.pending_model_selection = Some(selection.clone());
    runtime_state.updated_at = crate::current_unix_millis_text();
    // The legacy pending row is an in-memory WebUI adapter only. V1 metadata is the durable authority,
    // and the typed request-switch display fact is appended only when Core promotes the pending request.
    // 旧待生效行仅作为内存 WebUI 适配器。V1 metadata 是可靠权威，强类型请求切换显示事实只在 Core
    // 提升待生效请求时追加。
    Ok(PublicAiWorkspaceReconciliation::default())
}

/// Selects a global model or an exact pending session route and remembers its account.
/// 选择全局模型或精确的会话待生效路由，并记住其账号。
///
/// # Parameters
/// 参数
/// `app` publishes the result; `state` owns runtime and sessions; `request` identifies a global or session-scoped exact route.
/// `app` 发布结果；`state` 持有运行时和会话；`request` 标识全局或会话作用域的精确路由。
///
/// # Returns
/// 返回值
/// Returns the rebuilt settings snapshot after preferences and any V1 pending request commit successfully.
/// 账号偏好及可选 V1 待生效请求成功提交后返回重建的设置快照。
pub(crate) async fn service_select_public_ai_model(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelSelectionRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // conversation_id is the validated optional session scope for this selection.
    // conversation_id 是本次选择经过校验的可选会话作用域。
    let conversation_id = request
        .conversation_id
        .as_deref()
        .map(|value| {
            value
                .parse::<crate::SessionId>()
                .map_err(|error| format!("Invalid conversation id `{value}`: {error}"))
        })
        .transpose()?;
    // session_context_compaction_window_node_id preserves the current session lock across models.
    // session_context_compaction_window_node_id 在模型之间保留当前会话锁定节点。
    let (session_context_compaction_window_node_id, session_request_fallback) =
        if let Some(session_id) = conversation_id {
            let guard = state
                .lock()
                .map_err(|error| format!("Failed to lock service state: {error}"))?;
            // session_exists prevents account-preference persistence when the target session is already gone.
            // session_exists 防止目标会话已经消失时仍持久化账号偏好。
            let session_exists = guard
                .workspace_snapshot
                .as_ref()
                .is_some_and(|workspace| workspace.session_runtime_state.contains_key(&session_id));
            if !session_exists {
                return Err(format!("Conversation `{session_id}` has no runtime state."));
            }
            let runtime_state = guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
            (
                runtime_state.context_compaction_window_node_id,
                crate::session_request_config_v1::SessionRequestFallback {
                    provider: runtime_state.active_provider_id.clone(),
                    connection_id: runtime_state.active_connection_id.clone(),
                    model: runtime_state.active_model_id.clone(),
                    temperature: runtime_state.active_temperature,
                },
            )
        } else {
            (
                None,
                crate::session_request_config_v1::SessionRequestFallback::default(),
            )
        };
    let session_handle = if let Some(session_id) = conversation_id {
        Some(crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id).await?)
    } else {
        None
    };
    // transaction serializes validation, durable preferences, pending intent, and publication.
    // transaction 串行化校验、持久偏好、待生效意图与发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    // current_configuration is the immutable validation baseline before persistent mutation.
    // current_configuration 是持久化修改前使用的不可变校验基线。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // selected_connection is the exact credential row requested by the composer account selector.
    // selected_connection 是输入区账号选择器请求的精确凭据行。
    let selected_connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    if selected_connection.provider_id != request.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to provider `{}`.",
            request.connection_id, request.provider_id
        ));
    }
    if !current_configuration.connection_is_enabled(request.connection_id.as_str()) {
        return Err(format!(
            "Connection `{}` is disabled and cannot be selected.",
            request.connection_id
        ));
    }
    if current_configuration.connection_is_pending_validation(request.connection_id.as_str()) {
        return Err(format!(
            "Connection `{}` is pending explicit model validation and cannot be selected for a conversation.",
            request.connection_id
        ));
    }
    if !runtime.model_is_enabled_for_connection(
        &current_configuration,
        request.connection_id.as_str(),
        request.model_id.as_str(),
    )? {
        return Err(format!(
            "Provider `{}` model `{}` is disabled.",
            request.provider_id, request.model_id
        ));
    }
    // selected_connection_exposes_model rejects mismatched account/model requests before mutation.
    // selected_connection_exposes_model 在修改前拒绝账号与模型不匹配的请求。
    let selected_connection_exposes_model = runtime
        .catalog_snapshot()?
        .get(request.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog
                .models
                .iter()
                .any(|model| model.id == request.model_id)
        });
    if !selected_connection_exposes_model {
        return Err(format!(
            "Connection `{}` does not expose model `{}`.",
            request.connection_id, request.model_id
        ));
    }
    // selected_model is the exact target used to validate an existing session compression lock.
    // selected_model 是用于校验既有会话压缩锁定的精确目标。
    let selected_model = runtime.resolve_connection_model_selection(
        &current_configuration,
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    if let Some(node_id) = session_context_compaction_window_node_id {
        // node_tokens resolves the stable persisted id against the current global configuration.
        // node_tokens 根据当前全局配置解析持久化的稳定标识。
        let node_tokens = current_configuration
            .context_compaction_window_nodes
            .tokens_for(node_id)?;
        // model_context_limit is the unchanged hard window declared by the selected model.
        // model_context_limit 是所选模型声明且保持不变的硬窗口。
        let model_context_limit =
            selected_model
                .model
                .capabilities
                .context_limit
                .ok_or_else(|| {
                    String::from("目标模型未声明上下文窗口，无法保留当前会话的上下文压缩锁定。")
                })?;
        if node_tokens >= model_context_limit {
            return Err(format!(
                "当前会话锁定的上下文压缩节点 `{}` 为 {} Token，必须严格小于目标模型上下文窗口 {} Token。请先切回默认窗口。",
                node_id.as_str(),
                node_tokens,
                model_context_limit
            ));
        }
    }
    // A model-picker action is the only model-change boundary allowed to replace an incompatible
    // reasoning level. Supported levels remain byte-for-byte session authority.
    // 模型选择器动作是模型变化时唯一允许替换不兼容思考档位的边界；受支持档位逐字保持会话权威。
    let reasoning_override = session_handle
        .as_ref()
        .map(|handle| -> Result<_, String> {
            let next_request =
                crate::session_request_config_v1::capture_next_request_config_v1(handle.as_ref())?;
            let Some(current_reasoning) = next_request
                .thinking_level
                .as_deref()
                .map(|level| {
                    crate::workspace_persistence_versions::parse_reasoning_effort(level)
                        .ok_or_else(|| format!("Invalid V1 reasoning level: {level}"))
                })
                .transpose()?
            else {
                return Ok(None);
            };
            if current_reasoning
                == vulcan_agent_runtime::provider_config::ReasoningEffortLevel::Off
            {
                return Ok(None);
            }
            let resolved_reasoning = vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
                &selected_model.model.capabilities,
                Some(current_reasoning),
            );
            Ok((resolved_reasoning != current_reasoning).then_some(resolved_reasoning))
        })
        .transpose()?
        .flatten();
    // selectedTarget is the exact future session tuple or process-wide active tuple.
    // selectedTarget 是精确的未来会话三元组或进程级活动三元组。
    let selected_target = ActiveAiModelSelection {
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        connection_id: request.connection_id.clone(),
    };
    // pendingTarget keeps the session-scoped selection distinct from the current turn's active route.
    // pendingTarget 让会话作用域选择与当前回合的活动路由保持分离。
    let pending_target = SessionPendingModelSelection {
        provider_id: request.provider_id.clone(),
        connection_id: request.connection_id.clone(),
        model_id: request.model_id.clone(),
    };
    let pending_requested_at_ms = crate::session_goal_lifecycle::current_v1_goal_unix_millis()?;
    let v1_pending_target = session_handle.map(|handle| {
        (
            handle,
            request.provider_id.clone(),
            request.connection_id.clone(),
            request.model_id.clone(),
            reasoning_override,
            session_request_fallback,
            pending_requested_at_ms,
        )
    });
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_using_in_transaction(
            &state,
            runtime.as_ref(),
            &current_configuration,
            "选择公共 AI 模型失败",
            |configuration| {
                configuration.connection_graph.remember_model_connection(
                    request.provider_id.as_str(),
                    request.model_id.as_str(),
                    request.connection_id.as_str(),
                )?;
                if conversation_id.is_none() {
                    configuration.active = Some(selected_target.clone());
                }
                // Validate the exact target against the published generation; the draft active
                // selection cannot be resolved until this transaction is committed.
                // 使用已发布代际校验精确目标；事务提交前不能解析草稿中的活动选择。
                runtime.resolve_connection_model_selection(
                    configuration,
                    request.connection_id.as_str(),
                    request.provider_id.as_str(),
                    request.model_id.as_str(),
                )?;
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |configuration| {
                publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
                    &app,
                    &state,
                    runtime.as_ref(),
                    configuration,
                    |workspace| {
                        if let Some(session_id) = conversation_id {
                            plan_session_model_selection_for_next_provider_request(
                                workspace,
                                session_id,
                                &pending_target,
                            )
                        } else {
                            Ok(PublicAiWorkspaceReconciliation::default())
                        }
                    },
                    false,
                    "model_selection",
                    move || {
                        if let Some((
                            handle,
                            provider_id,
                            connection_id,
                            model_id,
                            reasoning_override,
                            fallback,
                            now_ms,
                        )) = v1_pending_target
                        {
                            crate::session_request_config_v1::set_pending_model_request_v1(
                                handle.as_ref(),
                                provider_id,
                                connection_id,
                                model_id,
                                reasoning_override,
                                fallback,
                                now_ms,
                            )?;
                        }
                        Ok(())
                    },
                )
                .map(|_| ())
            },
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Reports whether one exact provider-model-account selection is still executable.
/// 报告一个精确的供应商、模型、账号选择是否仍可执行。
///
/// # Parameters
/// 参数
/// `runtime` supplies the authoritative scoped access and lease validation services.
/// `runtime` 提供权威的作用域访问与租约校验服务。
/// `configuration` owns model access and connections; `catalogs` owns process-local models; `selection`
/// identifies the exact persisted choice.
/// `configuration` 持有模型访问与连接；`catalogs` 持有进程内模型；`selection` 标识精确持久化选择。
///
/// # Returns
/// 返回值
/// `true` only when the exact connection, provider, scoped enabled state, and text conversation
/// model remain available; runtime snapshot failures are returned explicitly.
/// 仅当精确连接、供应商、作用域启用状态及文本会话模型仍可用时返回 `true`；运行时快照失败会显式返回。
fn model_selection_is_available(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    selection: &ActiveAiModelSelection,
) -> Result<bool, String> {
    let Some(connection) = configuration
        .connection_graph
        .connections
        .get(selection.connection_id.as_str())
    else {
        return Ok(false);
    };
    if connection.provider_id != selection.provider_id
        || !configuration.connection_is_enabled(selection.connection_id.as_str())
        || configuration.connection_is_pending_validation(selection.connection_id.as_str())
        || !configuration.connection_model_is_enabled(
            selection.connection_id.as_str(),
            selection.model_id.as_str(),
        )
    {
        return Ok(false);
    }
    if !runtime.model_is_enabled_for_connection(
        configuration,
        selection.connection_id.as_str(),
        selection.model_id.as_str(),
    )? {
        return Ok(false);
    }
    Ok(catalogs
        .get(selection.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog.models.iter().any(|model| {
                model.id == selection.model_id
                    && vulcan_provider_adapters::model_supports_text_conversation(
                        &model.capabilities,
                    )
            })
        }))
}

/// Resolves the persisted active model or the first stable enabled model as a fallback.
/// 解析持久化当前模型；若其失效，则回退到稳定排序的首个启用模型。
///
/// # Parameters
/// 参数
/// `runtime` validates model access; `configuration` supplies the active choice; `catalogs` supplies exact account inventory.
/// `runtime` 校验模型访问；`configuration` 提供当前选择；`catalogs` 提供精确账号目录。
///
/// # Returns
/// 返回值
/// Returns the available active route or the stable first enabled route, with failures propagated.
/// 返回可用当前路由或稳定首个启用路由，并显式传播失败。
#[cfg(test)]
fn preferred_fallback_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Option<ActiveAiModelSelection>, String> {
    if let Some(selection) = configuration.active.as_ref()
        && model_selection_is_available(runtime, configuration, catalogs, selection)?
    {
        return Ok(Some(selection.clone()));
    }
    first_enabled_model_selection(runtime, configuration, catalogs)
}

/// Rebinds an invalid global account to the same model before considering a stable fallback.
/// 全局账号失效时先重绑同一模型，再考虑稳定替代项。
///
/// # Parameters
/// 参数
/// `runtime` validates model access; `configuration` owns the mutable global choice; `catalogs` supplies account inventory.
/// `runtime` 校验模型访问；`configuration` 持有可变全局选择；`catalogs` 提供账号目录。
///
/// # Returns
/// 返回值
/// Keeps a valid or absent global choice unchanged; otherwise rebinds the same model before stable fallback.
/// 保留有效或不存在的全局选择；否则先重绑同一模型，再选择稳定替代项。
pub(super) fn ensure_active_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &mut PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<(), String> {
    // activeSelection is cloned so the invalid exact account can be replaced without extending
    // an immutable borrow of the configuration.
    // activeSelection 通过克隆避免在替换无效精确账号时延长配置的不可变借用。
    let Some(active_selection) = configuration.active.clone() else {
        return Ok(());
    };
    if model_selection_is_available(runtime, configuration, catalogs, &active_selection)? {
        return Ok(());
    }
    // sameModelSelection preserves the user's model choice when another credential account still
    // exposes it; only a provider/model disappearance falls back to the first enabled model.
    // sameModelSelection 在其他凭据账号仍公开该模型时保留用户模型选择；只有供应商/模型消失时
    // 才回退到首个启用模型。
    let same_model_selection = preferred_available_connection_for_model(
        runtime,
        configuration,
        catalogs,
        active_selection.provider_id.as_str(),
        active_selection.model_id.as_str(),
    )?;
    configuration.active = match same_model_selection {
        Some(selection) => Some(selection),
        None => first_enabled_model_selection(runtime, configuration, catalogs)?,
    };
    Ok(())
}

/// Resolves the preferred available credential for one existing provider/model choice.
/// 为一个既有供应商/模型选择解析首选的可用凭据。
///
/// # Parameters
/// 参数
/// `runtime` validates provider-level access, `configuration` owns account preferences,
/// `catalogs` is the effective account inventory, and the identifiers name the model.
/// `runtime` 校验供应商级访问，`configuration` 持有账号偏好，`catalogs` 是有效账号目录，
/// 两个标识用于指定模型。
///
/// # Returns
/// 返回值
/// The exact preferred available selection, or `None` when no account exposes the model.
/// 返回精确的首选可用选择；没有账号公开该模型时返回 `None`。
fn preferred_available_connection_for_model(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<ActiveAiModelSelection>, String> {
    // eligibleCatalogs excludes credentials that cannot participate in normal conversation routing.
    // eligibleCatalogs 排除不能参与常规会话路由的凭据。
    let mut eligible_catalogs = catalogs.clone();
    eligible_catalogs.retain(|connection_id, _| {
        configuration.connection_is_enabled(connection_id.as_str())
            && !configuration.connection_is_pending_validation(connection_id.as_str())
    });
    let eligible_catalogs =
        vulcan_provider_adapters::RuntimeConnectionCatalogs::from_snapshot(eligible_catalogs);
    // modelExists proves resolveConnection has a valid deterministic candidate rather than
    // converting its no-candidate error into control flow.
    // modelExists 先证明 resolveConnection 存在确定候选，避免把无候选错误当作控制流吞掉。
    let model_exists = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .any(|connection| {
            eligible_catalogs
                .get(connection.id.as_str())
                .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
        });
    if !model_exists {
        return Ok(None);
    }
    let connection = configuration.connection_graph.resolve_connection(
        provider_id,
        model_id,
        &eligible_catalogs,
    )?;
    let selection = ActiveAiModelSelection {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        connection_id: connection.id.clone(),
    };
    model_selection_is_available(runtime, configuration, catalogs, &selection)
        .map(|available| available.then_some(selection))
}

/// Preserves every explicitly selected session model across catalog-only reconciliation.
/// 在仅目录协调期间保留每个显式选择的会话模型。
///
/// # Parameters
/// 参数
/// `workspace` owns active and archived session runtime selections; the remaining arguments identify
/// the catalog generation that must not rewrite those user-owned tuples.
/// `workspace` 持有活动及归档会话运行态选择；其余参数标识不得改写这些用户自有三元组的目录代际。
///
/// # Returns
/// 返回值
/// An unchanged detached workspace candidate and an empty changed-session set.
/// 返回一份未改写的独立工作区候选与空的会话变更集合。
pub(super) fn plan_reconciled_session_model_selections(
    workspace: vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    _runtime: &PublicAiRuntime,
    _configuration: &PersistedAiConfiguration,
    _catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<
    (
        vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
        BTreeSet<crate::SessionId>,
    ),
    String,
> {
    // Catalog refresh is observational for established sessions. An unavailable exact tuple is
    // hidden by projection and requires explicit reselection; it is never rewritten to another
    // provider, account, or model.
    // 对已有会话而言，目录刷新只负责观测。不可用的精确三元组由投影隐藏并要求显式重选，
    // 绝不会被改写成其他供应商、账号或模型。
    Ok((workspace, BTreeSet::new()))
}

/// Applies observational catalog reconciliation without rewriting explicit session selections.
/// 应用仅观测的目录协调，不改写显式会话选择。
///
/// # Parameters
/// 参数
/// `runtime` supplies the authoritative scoped access and lease validation services.
/// `runtime` 提供权威的作用域访问与租约校验服务。
/// `workspace` owns active and archived session runtime selections; `configuration` and `catalogs`
/// identify the observed generation whose availability is handled by projection.
/// `workspace` 持有活动及归档会话运行态选择；`configuration` 与 `catalogs` 标识由投影处理
/// 可用性的观测代际。
///
/// # Returns
/// 返回值
/// An empty changed-session set while preserving every exact tuple. Any planning or lease-lock
/// failure leaves the caller's workspace unchanged.
/// 返回空的会话变更集合并保留每个精确三元组；规划或租约锁失败时，调用方工作区保持不变。
#[cfg(test)]
pub(crate) fn reconcile_missing_session_model_selections(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<BTreeSet<crate::SessionId>, String> {
    // plannedWorkspace is detached until every fallible lease operation succeeds.
    // plannedWorkspace 在全部可失败租约操作成功前保持独立。
    let (planned_workspace, changed_session_ids) = plan_reconciled_session_model_selections(
        workspace.clone(),
        runtime,
        configuration,
        catalogs,
    )?;
    // leaseScopeIds materializes stable strings for one exact absence/exclusion transaction.
    // leaseScopeIds 为一次精确的缺失校验/排除事务物化稳定字符串。
    let lease_scope_ids = changed_session_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    // leaseGuard prevents a concurrent turn from installing a route between planning and commit.
    // leaseGuard 防止并发回合在规划与提交之间安装路由。
    let _lease_guard = runtime.acquire_session_selection_lease_absence_guard(&lease_scope_ids)?;
    *workspace = planned_workspace;
    Ok(changed_session_ids)
}

/// Selects the first stable enabled provider-model-account tuple from runtime catalogs.
/// 从运行时目录选择首个稳定的已启用供应商模型账号组合。
///
/// # Parameters
/// 参数
/// `runtime` supplies the authoritative scoped access and lease validation services.
/// `runtime` 提供权威的作用域访问与租约校验服务。
/// `configuration` owns model access and connections; `catalogs` owns process-local models.
/// `configuration` 持有模型访问与连接；`catalogs` 持有进程内模型。
///
/// # Returns
/// 返回值
/// The lexicographically first enabled tuple, `None` when no model remains, or an explicit runtime
/// snapshot error.
/// 返回按字典序排列的首个启用组合、没有剩余模型时返回 `None`，或显式运行时快照错误。
pub(super) fn first_enabled_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Option<ActiveAiModelSelection>, String> {
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if !configuration.connection_is_enabled(connection.id.as_str())
            || configuration.connection_is_pending_validation(connection.id.as_str())
        {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        for model in &catalog.models {
            if runtime.model_is_enabled_for_connection(
                configuration,
                connection.id.as_str(),
                model.id.as_str(),
            )? && configuration
                .connection_model_is_enabled(connection.id.as_str(), model.id.as_str())
                && vulcan_provider_adapters::model_supports_text_conversation(&model.capabilities)
            {
                candidates.push(ActiveAiModelSelection {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    connection_id: connection.id.clone(),
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.connection_id.cmp(&right.connection_id))
    });
    Ok(candidates.into_iter().next())
}

/// Selects an enabled replacement while preferring the same provider and model on another account.
/// 选择一个已启用替代项，并优先保留相同供应商与模型的其他账号。
///
/// # Parameters
/// 参数
/// `runtime` supplies the authoritative scoped access and lease validation services.
/// `runtime` 提供权威的作用域访问与租约校验服务。
/// `configuration` and `catalogs` describe the candidate state; `provider_id` and `model_id`
/// identify the user's current model intent.
/// `configuration` 与 `catalogs` 描述候选状态；`provider_id` 与 `model_id` 标识用户当前模型意图。
///
/// # Returns
/// 返回值
/// The first same-model account, otherwise the stable global fallback; scoped runtime failures are
/// returned explicitly.
/// 返回首个同模型账号，否则返回稳定的全局降级项；作用域运行时失败会显式返回。
pub(super) fn preferred_replacement_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<ActiveAiModelSelection>, String> {
    let mut same_model_candidates = Vec::new();
    for connection in configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
    {
        if !configuration.connection_is_enabled(connection.id.as_str())
            || configuration.connection_is_pending_validation(connection.id.as_str())
            || !runtime.model_is_enabled_for_connection(
                configuration,
                connection.id.as_str(),
                model_id,
            )?
            || !configuration.connection_model_is_enabled(connection.id.as_str(), model_id)
        {
            continue;
        }
        let supports_model = catalogs.get(connection.id.as_str()).is_some_and(|catalog| {
            catalog.models.iter().any(|model| {
                model.id == model_id
                    && vulcan_provider_adapters::model_supports_text_conversation(
                        &model.capabilities,
                    )
            })
        });
        if supports_model {
            same_model_candidates.push(ActiveAiModelSelection {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                connection_id: connection.id.clone(),
            });
        }
    }
    same_model_candidates.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
    if let Some(selection) = same_model_candidates.into_iter().next() {
        return Ok(Some(selection));
    }
    first_enabled_model_selection(runtime, configuration, catalogs)
}

/// Revokes exact loaded-session leases for one credential and an optional model.
/// 为一个凭据及可选模型撤销已加载会话的精确租约。
///
/// # Parameters
/// 参数
/// `workspace` is the detached loaded-session candidate; `connection_id` selects the affected account;
/// `model_id` narrows revocation to one model when present.
/// `workspace` 是独立的已加载会话候选；`connection_id` 选择受影响账号；
/// 存在 `model_id` 时将撤销范围收窄到一个模型。
///
/// # Returns
/// 返回值
/// The complete active and archived session set whose exact connection route was revoked.
/// 返回精确连接路由已被撤销的完整活动与归档会话集合。
pub(super) fn reconcile_disabled_connection_sessions(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    connection_id: &str,
    model_id: Option<&str>,
) -> BTreeSet<crate::SessionId> {
    let mut changed_session_ids = reconcile_explicitly_revoked_session_model_selections(
        workspace,
        |_session_id, runtime_state| {
            let exact_connection =
                runtime_state.active_connection_id.as_deref() == Some(connection_id);
            let exact_model = model_id
                .is_none_or(|model_id| runtime_state.active_model_id.as_deref() == Some(model_id));
            exact_connection && exact_model
        },
    );
    // revokePending removes future authority for the disabled connection even when the current
    // session route belongs to another account and therefore keeps its live lease.
    // revokePending 即使当前会话路由属于另一个账号并保留其活动租约，也会移除已停用连接的未来权限。
    let mut revoke_pending = |states: &mut BTreeMap<
        crate::SessionId,
        vulcan_workbench_model::SessionRuntimeStateEntry,
    >| {
        for (session_id, runtime_state) in states.iter_mut() {
            let pending_matches =
                runtime_state
                    .pending_model_selection
                    .as_ref()
                    .is_some_and(|selection| {
                        selection.connection_id == connection_id
                            && model_id
                                .is_none_or(|model_id| selection.model_id.as_str() == model_id)
                    });
            if pending_matches {
                runtime_state.pending_model_selection = None;
                changed_session_ids.insert(*session_id);
            }
        }
    };
    revoke_pending(&mut workspace.session_runtime_state);
    revoke_pending(&mut workspace.archived_session_runtime_state);
    changed_session_ids
}

/// Revokes process-local route leases without rewriting exact session-owned route facts.
/// 撤销进程内路由租约，同时不改写会话持有的精确路由事实。
///
/// # Parameters
/// 参数
/// `workspace` is the detached candidate; `matches` identifies the complete set whose current
/// route authority was revoked.
/// `workspace` 是独立候选；`matches` 标识当前路由权威已被撤销的完整会话集合。
///
/// # Returns
/// 返回值
/// The exact active and archived session identifiers whose process-local lease was revoked.
/// 返回进程内租约已被撤销的精确活动及归档会话标识集合。
pub(super) fn reconcile_explicitly_revoked_session_model_selections<Matches>(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    matches: Matches,
) -> BTreeSet<crate::SessionId>
where
    Matches: Fn(crate::SessionId, &vulcan_workbench_model::SessionRuntimeStateEntry) -> bool,
{
    // changedSessionIds records the complete lease-revocation and persistence scope.
    // changedSessionIds 记录完整租约撤销与持久化范围。
    let mut changed_session_ids = BTreeSet::new();
    // reconcileStates applies the same closed transition to active and archived stores.
    // reconcileStates 对活动与归档存储应用相同的封闭转换。
    let mut reconcile_states = |states: &mut BTreeMap<
        crate::SessionId,
        vulcan_workbench_model::SessionRuntimeStateEntry,
    >| {
        for (session_id, runtime_state) in states.iter_mut() {
            if !matches(*session_id, runtime_state) {
                continue;
            }
            // The exact V1 route is user-owned and may only change through an explicit pending
            // request selection at an admitted user-input or Continue boundary. Revocation removes only the
            // process-local lease so projection becomes unavailable and requires reselection.
            // 精确 V1 路由由用户持有，只能通过显式待生效请求在已准入用户输入或 Continue 边界修改。
            // 撤销仅移除进程内租约，使投影变为不可用并要求用户重新选择。
            runtime_state.provider_route_lease = None;
            changed_session_ids.insert(*session_id);
        }
    };
    reconcile_states(&mut workspace.session_runtime_state);
    reconcile_states(&mut workspace.archived_session_runtime_state);
    changed_session_ids
}

#[cfg(test)]
mod tests;

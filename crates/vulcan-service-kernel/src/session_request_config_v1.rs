//! Durable V1 request-configuration mutation helpers.
//! 可靠 V1 请求配置修改辅助逻辑。

use vulcan_provider_config::config::ReasoningEffortLevel;
use vulcan_session_core::v1::config::{
    ActiveRequestConfig, AnswerMode, KnowledgeLibrarySelectionConfig, PendingRequestConfig,
    ProviderToolSelectionConfig, VmmSessionPolicyConfig,
};
use vulcan_session_core::v1::document::SessionSelectionBundleInput;
use vulcan_workbench_model::SessionRuntimeStateEntry;

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};

/// Metadata-only capability access that never creates a conversation runtime.
/// 纯元数据能力访问，绝不创建会话运行时。
pub(crate) struct SessionProviderToolConfiguration {
    /// Exact indexed storage authority for an unloaded session.
    /// 未加载会话的精确索引存储权威。
    locator: vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    /// Shared registry serializes configuration access with physical loading.
    /// 共享注册表串行化配置访问与物理加载。
    registry: std::sync::Arc<crate::session_runtime_registry_v1::SessionRuntimeRegistry>,
    /// Stable identity used for the resident owner and unloaded admission slot.
    /// 常驻 owner 与未加载准入槽使用的稳定身份。
    session_id: crate::SessionId,
    /// Prevents archive/delete from retiring the owner during configuration publication.
    /// 防止配置发布过程中归档/删除退役该 owner。
    _lifecycle: crate::session_runtime_registry_v1::SessionLifecycleTransitionGuard,
}

impl SessionProviderToolConfiguration {
    /// Resolves one configuration owner from `state` and `session_id`, without loading history.
    /// 从 `state` 与 `session_id` 解析配置 owner，不加载历史。
    /// Returns an access guard retained through configuration validation and publication.
    /// 返回贯穿配置校验与发布过程的访问守卫。
    pub(crate) async fn resolve(
        state: &crate::SharedServiceBridgeState,
        session_id: crate::SessionId,
    ) -> Result<Self, String> {
        // Capture only the index controller and registry, never the interaction runtime.
        // 仅捕获索引控制器和注册表，不获取交互运行时。
        let (controller, registry) = {
            let guard = state
                .lock()
                .map_err(|error| format!("Failed to lock service state: {error}"))?;
            (
                guard
                    .controller_runtime
                    .clone()
                    .ok_or_else(|| String::from("Controller runtime is not ready."))?,
                std::sync::Arc::clone(&guard.session_runtime_registry),
            )
        };
        // Lifecycle exclusion precedes exact index lookup, closing the delete/read race.
        // 生命周期排除先于精确索引查询，封闭删除/读取竞争窗口。
        let lifecycle = registry
            .acquire_lifecycle_transition(session_id)
            .await
            .map_err(|error| error.to_string())?;
        let locator = vulcan_workspace_store::workspace_store::locate_session_v1_storage(
            &controller,
            &session_id.to_string(),
        )
        .await
        .map_err(|error| {
            format!("Failed to locate the session capability configuration: {error}")
        })?;
        Ok(Self {
            locator,
            registry,
            session_id,
            _lifecycle: lifecycle,
        })
    }

    /// Reads the current capability configuration; returns no transcript or Turn-derived data.
    /// 读取当前能力配置；不返回转录或回合派生数据。
    pub(crate) fn read(&self) -> Result<ProviderToolSelectionConfig, String> {
        self.registry.with_configuration_access(
            self.session_id,
            |handle| {
                handle
                    .read(|document| document.metadata.config.selection.provider_tools.clone())
                    .map_err(|error| {
                        format!("Failed to read the session capability configuration: {error}")
                    })
            },
            || {
                self.locator
                    .read_provider_tool_selection()
                    .map_err(|error| {
                        format!("Failed to read the session capability configuration: {error}")
                    })
            },
        )
    }

    /// Saves `selection` at `now_ms`; returns whether the persisted configuration changed.
    /// 在 `now_ms` 保存 `selection`；返回持久化配置是否变化。
    /// Resident mutations share the existing writer; cold writes reserve the physical load slot.
    /// 常驻修改复用现有写入器；冷写入预留物理加载槽。
    pub(crate) fn save(
        &self,
        selection: ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, String> {
        self.registry.with_configuration_access(
            self.session_id,
            |handle| set_provider_tool_selection_v1(handle, selection.clone(), now_ms),
            || {
                self.locator
                    .save_unloaded_provider_tool_selection(selection.clone(), now_ms)
                    .map_err(|error| {
                        format!("Failed to save the session capability configuration: {error}")
                    })
            },
        )
    }

    /// Explicitly rebuilds capability settings from `selection` at `now_ms`, returning whether changed.
    /// 于 `now_ms` 从 `selection` 显式重建能力设置，返回是否变化。
    /// Uses the resident writer or the cold configuration gate; never reconstructs a Turn.
    /// 使用常驻写入器或冷配置门控；绝不重建回合。
    pub(crate) fn rebuild(
        &self,
        selection: ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, String> {
        self.registry.with_configuration_access(
            self.session_id,
            |handle| set_provider_tool_selection_v1(handle, selection.clone(), now_ms),
            || {
                self.locator
                    .rebuild_unloaded_provider_tool_selection(selection.clone(), now_ms)
                    .map_err(|error| {
                        format!("Failed to rebuild the session capability configuration: {error}")
                    })
            },
        )
    }
}

/// Applies one initialized V1 Provider-tool selection to the temporary runtime projection.
/// 把一份已初始化的 V1 Provider 工具选择应用到阶段内运行态投影。
pub(crate) fn apply_provider_tool_selection_v1(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    selection: &vulcan_session_core::v1::config::ProviderToolSelectionConfig,
) {
    if !selection.initialized {
        return;
    }
    runtime_state.provider_tool_selections = selection
        .selections
        .iter()
        .map(|entry| {
            (
                entry.capability.clone(),
                vulcan_workbench_model::SessionProviderToolTargetSelection {
                    connection_id: entry.target.connection_id.clone(),
                    target_id: entry.target.target_id.clone(),
                },
            )
        })
        .collect();
    runtime_state.provider_tool_enabled_capabilities =
        selection.enabled_capabilities.iter().cloned().collect();
    runtime_state.media_understanding_selections = selection
        .media_understanding_selections
        .iter()
        .map(|entry| {
            let route = match &entry.selection {
                vulcan_session_core::v1::config::ProviderMediaUnderstandingRouteConfig::ProviderTool(
                    target,
                ) => vulcan_workbench_model::SessionMediaUnderstandingSelection::ProviderTool {
                    selection: vulcan_workbench_model::SessionProviderToolTargetSelection {
                        connection_id: target.connection_id.clone(),
                        target_id: target.target_id.clone(),
                    },
                },
                vulcan_session_core::v1::config::ProviderMediaUnderstandingRouteConfig::ModelReplacement(
                    target,
                ) => vulcan_workbench_model::SessionMediaUnderstandingSelection::ModelReplacement {
                    selection: vulcan_workbench_model::SessionMediaUnderstandingModelSelection {
                        provider_id: target.provider_id.clone(),
                        model_id: target.model_id.clone(),
                        connection_id: target.connection_id.clone(),
                    },
                    reasoning_effort: entry.reasoning_effort.map(Into::into),
                },
            };
            (entry.capability.clone(), route)
        })
        .collect();
    runtime_state.provider_extension_target_sets = selection
        .extension_target_sets
        .iter()
        .map(|entry| {
            (
                entry.capability.clone(),
                entry
                    .targets
                    .iter()
                    .map(
                        |target| vulcan_workbench_model::SessionProviderToolTargetSelection {
                            connection_id: target.connection_id.clone(),
                            target_id: target.target_id.clone(),
                        },
                    )
                    .collect(),
            )
        })
        .collect();
    runtime_state.provider_tool_settings_initialized = true;
    runtime_state.provider_tool_selections_overridden = selection.overridden;
}

/// Immutable fallback facts captured from the currently admitted legacy execution projection.
/// 从当前已准入旧执行投影捕获的不可变回退事实。
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionRequestFallback {
    /// Exact Provider id used when V1 has not yet initialized its active request.
    /// V1 尚未初始化活动请求时使用的精确 Provider 标识。
    pub(crate) provider: Option<String>,
    /// Exact non-secret connection id used before the first V1 request boundary.
    /// 首个 V1 请求边界前使用的精确非秘密连接标识。
    pub(crate) connection_id: Option<String>,
    /// Exact model id used when the V1 root is still fresh.
    /// V1 根仍为 fresh 时使用的精确模型标识。
    pub(crate) model: Option<String>,
    /// Exact optional temperature selected for the session.
    /// 当前会话选择的精确可选温度。
    pub(crate) temperature: Option<f32>,
}

impl SessionRequestFallback {
    /// Captures the exact legacy route and temperature from one admitted runtime projection.
    /// 从一份已准入运行时投影捕获精确的旧路由与温度值。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the exact session-owned process projection already selected by the caller.
    /// `runtime_state` 是调用方已经选定、由精确会话持有的进程内投影。
    ///
    /// # Returns
    /// 返回值
    /// A fallback that prefers the pending route over the active route without carrying reasoning.
    /// 返回一份严格优先使用待生效路由、其次使用活动路由且不携带思考档位的回退事实。
    pub(crate) fn from_runtime_state(runtime_state: &SessionRuntimeStateEntry) -> Self {
        let pending_route = runtime_state.pending_model_selection.as_ref();
        Self {
            provider: pending_route
                .map(|selection| selection.provider_id.clone())
                .or_else(|| runtime_state.active_provider_id.clone()),
            connection_id: pending_route
                .map(|selection| selection.connection_id.clone())
                .or_else(|| runtime_state.active_connection_id.clone()),
            model: pending_route
                .map(|selection| selection.model_id.clone())
                .or_else(|| runtime_state.active_model_id.clone()),
            temperature: runtime_state.active_temperature,
        }
    }
}

/// Durably records a complete next-request model route while preserving reasoning controls.
/// 可靠记录完整的下一请求模型路由，同时保留思考控制项。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document; route ids are the validated target; `fallback` supplies only
/// temperature for sessions predating initialized temperature Meta.
/// `handle` 持有 V1 文档；路由标识是已校验目标；`fallback` 仅为温度 Meta 初始化之前的会话
/// 提供温度值。
/// `reasoning_override` is present only when a user-selected target model cannot retain the previous level.
/// `reasoning_override` 仅在用户选择的目标模型无法保留先前档位时存在。
///
/// # Returns
/// 返回值
/// `true` only when metadata was durably changed.
/// 仅当 metadata 已可靠修改时返回 `true`。
pub(crate) fn set_pending_model_request_v1(
    handle: &SessionHandle,
    provider: String,
    connection_id: String,
    model: String,
    reasoning_override: Option<ReasoningEffortLevel>,
    fallback: SessionRequestFallback,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let active = &document.metadata.config.active_request;
                let pending = document.metadata.config.pending_request.as_ref();
                let preserved_reasoning = pending
                    .map(|value| value.thinking_level.clone())
                    .unwrap_or_else(|| active.thinking_level.clone());
                let request = PendingRequestConfig {
                    provider: Some(provider.clone()),
                    connection_id: Some(connection_id.clone()),
                    model: Some(model.clone()),
                    thinking_level: reasoning_override
                        .map(reasoning_effort_level_key)
                        .or(preserved_reasoning),
                    temperature: pending
                        .and_then(|value| value.temperature)
                        .or(active.temperature)
                        .or_else(|| fallback.temperature.map(f64::from)),
                    set_at_ms: now_ms,
                };
                set_pending_request_if_effectively_changed(document, request, now_ms)
            },
        )
        .map_err(|error| {
            format!("Failed to save the V1 next-request model configuration: {error}")
        })?;
    Ok(receipt.changed)
}

/// Captures the exact request configuration to bind when a new user node is admitted.
/// 捕获新用户节点准入时要绑定的精确请求配置。
///
/// # Parameters / 参数
/// `handle` owns the authoritative active and pending request configuration.
/// `handle` 持有权威活动请求配置与待生效请求配置。
///
/// # Returns / 返回值
/// Returns pending intent when present, otherwise the current active configuration. No provider
/// default or process-local mirror participates.
/// 存在待生效意图时返回该意图，否则返回当前活动配置；不参与任何 Provider 默认值或进程内镜像。
pub(crate) fn capture_next_request_config_v1(
    handle: &SessionHandle,
) -> Result<ActiveRequestConfig, String> {
    handle
        .read(
            |document| match document.metadata.config.pending_request.as_ref() {
                Some(pending) => ActiveRequestConfig {
                    provider: pending.provider.clone(),
                    connection_id: pending.connection_id.clone(),
                    model: pending.model.clone(),
                    thinking_level: pending.thinking_level.clone(),
                    temperature: pending.temperature,
                },
                None => document.metadata.config.active_request.clone(),
            },
        )
        .map_err(|error| format!("Read V1 next request configuration failed: {error}"))
}

/// Reconciles a removed reasoning level immediately before a new user boundary is admitted.
/// 在新的用户边界准入之前校正已经消失的思考档位。
///
/// # Parameters / 参数
/// `handle` owns durable active and pending request Meta; `runtime` resolves the exact current
/// model contract. No process-local fallback participates.
/// `handle` 持有可靠活动与待生效请求 Meta；`runtime` 解析精确的当前模型契约；不参与任何
/// 进程内 fallback。
///
/// # Returns / 返回值
/// Returns the exact request snapshot to bind to the new input. A removed level is corrected only
/// in this snapshot; the Core admission transaction decides whether it becomes durable.
/// 返回要绑定到新输入的精确请求快照。已消失档位只在此快照中校正；是否持久化由 Core 准入事务决定。
pub(crate) fn reconcile_pending_reasoning_for_next_user_boundary_v1(
    handle: &SessionHandle,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
) -> Result<ActiveRequestConfig, String> {
    let mut next_request = capture_next_request_config_v1(handle)?;
    let Some(stored_reasoning) = next_request.thinking_level.as_deref() else {
        return Ok(next_request);
    };
    let current_reasoning =
        crate::workspace_persistence_versions::parse_reasoning_effort(stored_reasoning)
            .ok_or_else(|| format!("Invalid V1 reasoning level: {stored_reasoning}"))?;
    if current_reasoning == ReasoningEffortLevel::Off {
        return Ok(next_request);
    }
    let selection = match (
        next_request.provider.as_deref(),
        next_request.connection_id.as_deref(),
        next_request.model.as_deref(),
    ) {
        (None, None, None) => return Ok(next_request),
        (Some(provider), Some(connection_id), Some(model)) => {
            let configuration = runtime.configuration_snapshot()?;
            runtime.resolve_connection_model_selection(
                &configuration,
                connection_id,
                provider,
                model,
            )?
        }
        (Some(provider), None, Some(model)) => {
            let configuration = runtime.configuration_snapshot()?;
            runtime.resolve_model_selection(&configuration, provider, model)?
        }
        _ => {
            return Err(String::from(
                "The V1 next request model selection is incomplete.",
            ));
        }
    };
    let resolved_reasoning =
        vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
            &selection.model.capabilities,
            Some(current_reasoning),
        );
    if resolved_reasoning != current_reasoning {
        next_request.thinking_level = Some(reasoning_effort_level_key(resolved_reasoning));
    }
    Ok(next_request)
}

/// Persists a capability-driven reasoning correction after a user boundary was accepted.
/// 在用户边界被接纳后持久化由能力变化驱动的思考档位校正。
///
/// # Parameters / 参数
/// `handle` and `runtime` identify the same authority used to bind the accepted input; `now_ms`
/// timestamps the post-admission Meta write.
/// `handle` 与 `runtime` 标识绑定已接纳输入时使用的同一权威；`now_ms` 为准入后的 Meta 写入
/// 提供时间戳。
///
/// # Returns / 返回值
/// Returns `true` only when the expected next-request snapshot was still current and its removed
/// reasoning level was replaced. A concurrent picker change wins unchanged.
/// 仅当预期下一请求快照仍然有效且其中已消失档位被替换时返回 `true`；并发选择器变更保持优先。
pub(crate) fn persist_missing_reasoning_reconciliation_after_user_admission_v1(
    handle: &SessionHandle,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    now_ms: i64,
) -> Result<bool, String> {
    let expected = capture_next_request_config_v1(handle)?;
    let reconciled = reconcile_pending_reasoning_for_next_user_boundary_v1(handle, runtime)?;
    if expected.thinking_level == reconciled.thinking_level {
        return Ok(false);
    }
    let reconciled_active = reconciled.clone();
    let request = PendingRequestConfig {
        provider: reconciled.provider,
        connection_id: reconciled.connection_id,
        model: reconciled.model,
        thinking_level: reconciled.thinking_level,
        temperature: reconciled.temperature,
        set_at_ms: now_ms,
    };
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let current = match document.metadata.config.pending_request.as_ref() {
                    Some(pending) => ActiveRequestConfig {
                        provider: pending.provider.clone(),
                        connection_id: pending.connection_id.clone(),
                        model: pending.model.clone(),
                        thinking_level: pending.thinking_level.clone(),
                        temperature: pending.temperature,
                    },
                    None => document.metadata.config.active_request.clone(),
                };
                if current != expected {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                if document.metadata.config.active_request == reconciled_active {
                    return document
                        .clear_pending_request_if_matches(&expected, now_ms)
                        .map(|changed| {
                            if changed {
                                SessionMutationChange::Changed(())
                            } else {
                                SessionMutationChange::Unchanged(())
                            }
                        });
                }
                set_pending_request_if_effectively_changed(document, request, now_ms)
            },
        )
        .map_err(|error| {
            format!("Failed to save the V1 missing reasoning level reconciliation: {error}")
        })?;
    Ok(receipt.changed)
}

/// Binds one physical Provider request to the already active durable session selection.
/// 将一次物理 Provider 请求绑定到已经活动的可靠会话选择。
/// `handle` owns the configuration, `runtime` owns process route leases, and `runtime_state` is only a request-local mirror.
/// `handle` 持有配置，`runtime` 持有进程路由租约，`runtime_state` 仅为请求局部镜像。
/// Returns after replacing stale mirror fields. Pending intent is intentionally ignored because
/// only user-input and Continue boundaries may activate it.
/// 替换过期镜像字段后返回。待生效意图会被有意忽略，因为只有用户输入与 Continue 边界可以激活它。
pub(crate) fn synchronize_v1_dispatch_model_selection(
    handle: &SessionHandle,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    runtime_state: &mut SessionRuntimeStateEntry,
) -> Result<(), String> {
    // One document read selects only the complete active route. A pending picker change must not
    // alter another physical API inside the currently running node.
    // 单次文档读取只选择完整活动路由。待生效选择器变更不得改变当前运行节点内的另一物理 API。
    let (session_id, route) = handle
        .read(|document| {
            let route = document.metadata.config.active_request.clone();
            (document.metadata.session_id.clone(), route)
        })
        .map_err(|error| format!("Read V1 dispatch model selection failed: {error}"))?;
    // Controls and route come from one revision so a simultaneous picker change cannot mix configurations.
    // 控制项和路由来自同一修订，避免同时发生的选择器变更混合配置。
    let ActiveRequestConfig {
        provider,
        connection_id,
        model,
        thinking_level,
        temperature,
    } = route;
    let reasoning_effort = thinking_level
        .as_deref()
        .map(|level| {
            crate::workspace_persistence_versions::parse_reasoning_effort(level)
                .ok_or_else(|| format!("Invalid V1 reasoning level: {level}"))
        })
        .transpose()?;
    // An empty route is valid only for a fresh session whose first API will capture the selected global default.
    // 空路由仅用于尚未初始化的会话，其首次 API 会捕获已选择的全局默认值。
    if provider.is_some() != model.is_some() || (provider.is_none() && connection_id.is_some()) {
        return Err(String::from(
            "The V1 dispatch model selection is incomplete.",
        ));
    }
    // A changed tuple is an authorized model switch, never a reason to reject the next request.
    // 三元组变化代表已授权模型切换，绝不是拒绝下一请求的理由。
    let mirror_changed = runtime_state.active_provider_id != provider
        || runtime_state.active_connection_id != connection_id
        || runtime_state.active_model_id != model;
    let lease = runtime.session_selection_lease(&session_id)?;
    let lease_changed = lease.as_ref().is_some_and(|lease| {
        Some(lease.route.provider.id.as_str()) != provider.as_deref()
            || Some(lease.model.id.as_str()) != model.as_deref()
            || connection_id
                .as_deref()
                .is_some_and(|id| id != lease.route.connection.id)
    });
    if lease_changed {
        runtime.clear_session_selection_leases(&session_id)?;
    }
    if lease_changed || (mirror_changed && lease.is_none()) {
        runtime_state.provider_route_lease = None;
    } else if let Some(lease) = lease {
        // A matching live lease remains immutable even if an older process mirror still names the previous model.
        // 即使旧进程镜像仍标记先前模型，匹配的实时租约也保持不可变。
        runtime_state.provider_route_lease = Some(
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(&lease),
        );
    }
    runtime_state.active_provider_id = provider;
    runtime_state.active_connection_id = connection_id;
    runtime_state.active_model_id = model;
    if runtime_state.active_provider_id.is_some() {
        runtime_state.active_reasoning_effort =
            Some(reasoning_effort.unwrap_or(ReasoningEffortLevel::Off));
        runtime_state.active_temperature = temperature.map(|value| value as f32);
    }
    runtime_state.pending_model_selection = None;
    Ok(())
}

/// Durably records a complete next-request reasoning selection while preserving its route.
/// 可靠记录完整的下一请求思考选择，同时保留其路由。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document; `thinking_level` is the selected stable key; `fallback` is used only for a fresh root.
/// `handle` 持有 V1 文档；`thinking_level` 是已选择的稳定键；`fallback` 仅用于 fresh 根。
///
/// # Returns
/// 返回值
/// `true` only when metadata was durably changed.
/// 仅当 metadata 已可靠修改时返回 `true`。
pub(crate) fn set_pending_reasoning_request_v1(
    handle: &SessionHandle,
    thinking_level: ReasoningEffortLevel,
    fallback: SessionRequestFallback,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let active = &document.metadata.config.active_request;
                let pending = document.metadata.config.pending_request.as_ref();
                let request = PendingRequestConfig {
                    provider: pending
                        .and_then(|value| value.provider.clone())
                        .or_else(|| active.provider.clone())
                        .or_else(|| fallback.provider.clone()),
                    connection_id: pending
                        .and_then(|value| value.connection_id.clone())
                        .or_else(|| active.connection_id.clone())
                        .or_else(|| fallback.connection_id.clone()),
                    model: pending
                        .and_then(|value| value.model.clone())
                        .or_else(|| active.model.clone())
                        .or_else(|| fallback.model.clone()),
                    thinking_level: Some(reasoning_effort_level_key(thinking_level)),
                    temperature: pending
                        .and_then(|value| value.temperature)
                        .or(active.temperature)
                        .or_else(|| fallback.temperature.map(f64::from)),
                    set_at_ms: now_ms,
                };
                set_pending_request_if_effectively_changed(document, request, now_ms)
            },
        )
        .map_err(|error| {
            format!("Failed to save the V1 next-request reasoning configuration: {error}")
        })?;
    Ok(receipt.changed)
}

/// Durably records one complete session-owned Provider tool selection.
/// 可靠记录一份由当前会话持有的完整 Provider 工具选择。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` is already canonical and complete, and `now_ms` is
/// the semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 已经规范且完整，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable selection changed.
/// 仅当精确持久化选择真实变化时返回 `true`。
pub(crate) fn set_provider_tool_selection_v1(
    handle: &SessionHandle,
    selection: ProviderToolSelectionConfig,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.set_provider_tool_selection(selection, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("Failed to save the V1 provider tool selection: {error}"))?;
    Ok(receipt.changed)
}

/// Durably commits one complete session-selection bundle as a single metadata revision.
/// 把一份完整会话选择配置作为单个 metadata 修订可靠提交。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` contains all first-turn selections, and `now_ms`
/// is the semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 包含全部首轮选择，`now_ms` 是语义修改时间。
/// `answer_mode` is the draft choice or the copied global default, applied in the same transaction.
/// `answer_mode` 为草稿选择或复制的全局默认值，在同一事务中应用。
///
/// # Returns
/// 返回值
/// `true` only when at least one durable selection changed.
/// 仅当至少一项持久化选择真实变化时返回 `true`。
pub(crate) fn set_session_selection_bundle_v1(
    handle: &SessionHandle,
    selection: SessionSelectionBundleInput,
    answer_mode: AnswerMode,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document.apply_atomic_transition(|document| {
                    let mut changed = document.set_session_selection_bundle(selection, now_ms)?;
                    let revision = document.metadata.config.answer_mode.revision;
                    changed |= document.set_answer_mode(answer_mode, revision, now_ms)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                })
            },
        )
        .map_err(|error| {
            format!("Failed to save the V1 session selection configuration: {error}")
        })?;
    Ok(receipt.changed)
}

/// Durably records one complete session-owned knowledge-library selection.
/// 可靠记录一份由当前会话持有的完整知识库选择。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` is authorized and canonical, and `now_ms` is the
/// semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 已授权且规范，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable selection changed.
/// 仅在精确持久化选择真实变化时返回 `true`。
pub(crate) fn set_knowledge_library_selection_v1(
    handle: &SessionHandle,
    selection: KnowledgeLibrarySelectionConfig,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.set_knowledge_library_selection(selection, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("Failed to save the V1 knowledge library selection: {error}"))?;
    Ok(receipt.changed)
}

/// Identifies one independently mutable session VMM policy field.
/// 标识一项可独立修改的会话 VMM 策略字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmmSessionPolicyField {
    /// Precheck-memory injection override.
    /// 预检记忆注入覆盖。
    Precheck,
    /// Completed-turn memory writeback override.
    /// 已完成回合记忆回写覆盖。
    MemoryWriteback,
}

/// Durably updates one VMM tri-state value while preserving its sibling in the same owner lock.
/// 在同一 owner 锁内可靠更新一项 VMM 三态值并保留另一项。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `field` selects the exact policy, `value` is inherit/enabled/
/// disabled, and `now_ms` is the semantic mutation time.
/// `handle` 持有 V1 文档，`field` 选择精确策略，`value` 表示继承/开启/关闭，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable policy pair changed.
/// 仅在精确持久化策略对真实变化时返回 `true`。
pub(crate) fn set_vmm_session_policy_value_v1(
    handle: &SessionHandle,
    field: VmmSessionPolicyField,
    value: Option<bool>,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let mut config: VmmSessionPolicyConfig =
                    document.metadata.config.selection.vmm.clone();
                match field {
                    VmmSessionPolicyField::Precheck => config.precheck_enabled = value,
                    VmmSessionPolicyField::MemoryWriteback => {
                        config.memory_writeback_enabled = value;
                    }
                }
                if document.set_vmm_session_policy(config, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("Failed to save the V1 VMM session policy: {error}"))?;
    Ok(receipt.changed)
}

/// Applies one complete pending request only when it differs from both pending and active facts.
/// 仅当完整待生效请求不同于现有待生效及活动事实时应用修改。
fn set_pending_request_if_effectively_changed(
    document: &mut vulcan_session_core::v1::document::SessionDocument,
    request: PendingRequestConfig,
    now_ms: i64,
) -> Result<SessionMutationChange<()>, vulcan_session_core::v1::errors::DomainError> {
    if document.metadata.config.pending_request.is_none()
        && active_request_matches_pending(&document.metadata.config.active_request, &request)
    {
        return Ok(SessionMutationChange::Unchanged(()));
    }
    if document.set_pending_request(request, now_ms)? {
        Ok(SessionMutationChange::Changed(()))
    } else {
        Ok(SessionMutationChange::Unchanged(()))
    }
}

/// Compares the stable effective fields while deliberately ignoring the pending timestamp.
/// 比较稳定生效字段，并有意忽略待生效时间戳。
fn active_request_matches_pending(
    active: &ActiveRequestConfig,
    pending: &PendingRequestConfig,
) -> bool {
    active.provider == pending.provider
        && active.connection_id == pending.connection_id
        && active.model == pending.model
        && active.thinking_level == pending.thinking_level
        && active.temperature == pending.temperature
}

/// Converts one provider-config reasoning enum into its stable kebab-case protocol key.
/// 将 Provider 配置思考枚举转换为稳定的 kebab-case 协议键。
pub(crate) fn reasoning_effort_level_key(level: ReasoningEffortLevel) -> String {
    String::from(match level {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::None => "none",
        ReasoningEffortLevel::Adaptive => "adaptive",
        ReasoningEffortLevel::Minimal => "minimal",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
        ReasoningEffortLevel::Max => "max",
        ReasoningEffortLevel::Ultra => "ultra",
    })
}

#[cfg(test)]
mod tests {
    //! Durable V1 request and metadata-only capability service tests.
    //! 可靠 V1 请求与纯元数据能力服务测试。

    /// Exercises cold and resident saves through the same service configuration boundary.
    /// 通过同一服务配置边界验证冷会话与常驻会话保存。
    #[tokio::test]
    async fn capability_metadata_survives_loading_and_later_runtime_flush() {
        use crate::session_runtime_registry_v1::SessionRuntimeRegistry;
        use vulcan_workspace_store::workspace_store::{
            create_session_v1_storage_fixture, load_session_v1_tail,
        };
        // The real file backend detects stale-owner overwrites that in-memory mocks would miss.
        // 真实文件后端可发现内存模拟无法发现的旧 owner 覆盖问题。
        let temporary = tempfile::tempdir().unwrap();
        let session_id = SessionId::from_u128(0x62c1);
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .unwrap();
        let registry = Arc::new(SessionRuntimeRegistry::default());
        let access = SessionProviderToolConfiguration {
            locator: locator.clone(),
            registry: registry.clone(),
            session_id,
            _lifecycle: registry
                .acquire_lifecycle_transition(session_id)
                .await
                .unwrap(),
        };
        let first = ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            ..Default::default()
        };
        assert!(access.save(first.clone(), 2).unwrap());
        assert_eq!(access.read().unwrap(), first);
        assert!(registry.get(session_id).is_err());
        let handle = registry
            .get_or_load(session_id, || {
                let loaded = load_session_v1_tail(&locator).unwrap();
                Ok(SessionHandle::new(
                    session_id,
                    locator.clone(),
                    loaded.document,
                ))
            })
            .unwrap();
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.provider_tools.clone())
                .unwrap(),
            first
        );
        let second = ProviderToolSelectionConfig {
            enabled_capabilities: vec![String::from("search")],
            ..first
        };
        assert!(access.save(second.clone(), 3).unwrap());
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.provider_tools.clone())
                .unwrap(),
            second
        );
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    if document.set_output_style(Some(String::from("concise")), 4)? {
                        Ok(SessionMutationChange::Changed(()))
                    } else {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                },
            )
            .unwrap();
        handle.flush_all().unwrap();
        assert_eq!(locator.read_provider_tool_selection().unwrap(), second);
        assert_eq!(
            load_session_v1_tail(&locator)
                .unwrap()
                .document
                .metadata
                .config
                .selection
                .provider_tools,
            second
        );
        // Rebuild a resident owner through its writer so later flushes cannot resurrect old overrides.
        // 通过常驻 owner 的写入器重建，避免后续 flush 复活旧覆盖值。
        let defaults = ProviderToolSelectionConfig {
            initialized: true,
            ..Default::default()
        };
        assert!(access.rebuild(defaults.clone(), 5).unwrap());
        assert_eq!(access.read().unwrap(), defaults);
        handle.flush_all().unwrap();
        assert_eq!(locator.read_provider_tool_selection().unwrap(), defaults);
        assert_eq!(
            load_session_v1_tail(&locator)
                .unwrap()
                .document
                .metadata
                .config
                .selection
                .output_style
                .as_deref(),
            Some("concise")
        );
    }

    use super::*;
    use crate::session_runtime_registry_v1::SessionPersistenceBackend;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
    use vulcan_workbench_model::SessionId;
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    /// Counts accepted metadata flushes for deterministic idempotency assertions.
    /// 统计已接纳 metadata flush，以进行确定性幂等断言。
    struct RequestConfigTestBackend {
        /// Number of durable candidates accepted by this backend.
        /// 当前后端接纳的可靠候选数量。
        flush_count: AtomicU64,
    }

    impl RequestConfigTestBackend {
        /// Creates one empty request-configuration persistence recorder.
        /// 创建一个空的请求配置持久化记录器。
        fn new() -> Self {
            Self {
                flush_count: AtomicU64::new(0),
            }
        }
    }

    impl SessionPersistenceBackend for RequestConfigTestBackend {
        /// Accepts one full V1 candidate and publishes its exact metadata.
        /// 接纳一份完整 V1 候选并发布其精确 metadata。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            self.flush_count.fetch_add(1, Ordering::SeqCst);
            Ok(SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            })
        }
    }

    /// Builds one handle with optional initialized active request facts.
    /// 构建一份带可选已初始化活动请求事实的句柄。
    fn request_config_handle(
        session_id: SessionId,
        active_request: ActiveRequestConfig,
    ) -> (Arc<SessionHandle>, Arc<RequestConfigTestBackend>) {
        let mut document =
            SessionDocument::new(session_id.to_string(), "project-a", "Request config", 1);
        document.metadata.config.active_request = active_request;
        let backend = Arc::new(RequestConfigTestBackend::new());
        (
            SessionHandle::with_backend(session_id, document, backend.clone()),
            backend,
        )
    }

    /// Verifies model selection preserves reasoning controls and repeated selection is a true no-op.
    /// 验证模型选择保留思考控制，并且重复选择是真正的无操作。
    #[test]
    fn pending_model_request_preserves_reasoning_and_is_idempotent() {
        let session_id = SessionId::from_u128(0x6201);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("high")),
                temperature: Some(0.25),
            },
        );
        let fallback = SessionRequestFallback::default();

        assert!(
            set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-b"),
                String::from("connection-b"),
                String::from("model-b"),
                None,
                fallback.clone(),
                2,
            )
            .expect("new pending model route should persist")
        );
        assert!(
            !set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-b"),
                String::from("connection-b"),
                String::from("model-b"),
                None,
                fallback,
                3,
            )
            .expect("same pending model route should succeed idempotently")
        );
        let (active, pending) = handle
            .read(|document| {
                (
                    document.metadata.config.active_request.clone(),
                    document.metadata.config.pending_request.clone(),
                )
            })
            .expect("request metadata should read");
        assert_eq!(active.model.as_deref(), Some("model-a"));
        let pending = pending.expect("pending route should exist");
        assert_eq!(pending.model.as_deref(), Some("model-b"));
        assert_eq!(pending.thinking_level.as_deref(), Some("high"));
        assert_eq!(pending.temperature, Some(0.25));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// Verifies model selection preserves the provider-default session reasoning level.
    /// 验证模型选择会保留会话中的供应商默认思考档位。
    #[test]
    fn pending_model_request_preserves_provider_default_reasoning() {
        let session_id = SessionId::from_u128(0x6206);
        let (handle, _) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: None,
                temperature: None,
            },
        );

        assert!(
            set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-b"),
                String::from("connection-b"),
                String::from("model-b"),
                None,
                SessionRequestFallback::default(),
                2,
            )
            .expect("new pending model route should persist")
        );
        let pending = handle
            .read(|document| document.metadata.config.pending_request.clone())
            .expect("request metadata should read")
            .expect("pending route should exist");
        assert_eq!(pending.thinking_level, None);
    }

    /// Verifies provider-default Meta remains unchanged without consulting model capabilities.
    /// 验证供应商默认 Meta 保持不变，并且不会查询模型能力。
    #[test]
    fn provider_default_reasoning_skips_capability_reconciliation() {
        let active_request = ActiveRequestConfig {
            provider: Some(String::from("provider-a")),
            connection_id: Some(String::from("connection-a")),
            model: Some(String::from("model-a")),
            thinking_level: None,
            temperature: None,
        };
        let (handle, _) =
            request_config_handle(SessionId::from_u128(0x6207), active_request.clone());
        let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable()
            .expect("unavailable runtime should construct");

        assert_eq!(
            reconcile_pending_reasoning_for_next_user_boundary_v1(handle.as_ref(), &runtime)
                .expect("provider-default reasoning should not require capability resolution"),
            active_request
        );
    }

    /// Exercises changed providers, models and accounts through the same V1 promotion and API validation used by dispatch.
    /// 使用分发采用的同一 V1 提升与 API 校验，验证供应商、模型和账号变化。
    /// Returns nothing; each case proves the old mirror fails and the synchronized selection starts with one switch record.
    /// 无返回值；每个场景证明旧镜像失败，而同步后的选择可启动并仅生成一条切换记录。
    #[test]
    fn v1_dispatch_model_switch_waits_for_user_admission_before_starting_api() {
        use vulcan_session_core::v1::context::LockedBaselineProjection;
        use vulcan_session_core::v1::event::ApiRequestSnapshot;
        use vulcan_session_core::v1::projection::provider_visible_fingerprint;
        use vulcan_session_core::v1::{
            ApiPurposeKind, ClientActionId, ClientInputSubmission, EventBody,
            PrepareAndStartApiInput, ProviderProjectionProof, Seq,
        };
        // These tuples cover both reported directions and same-provider model/account changes.
        // 这些三元组覆盖报告中的双向切换，以及同供应商的模型和账号变化。
        let cases = [
            (
                ("opencode-go", "go-account", "deepseek-v4-flash-vision-exp"),
                (
                    "deepseek",
                    "deepseek-account",
                    "deepseek-v4-flash-vision-exp",
                ),
            ),
            (
                (
                    "deepseek",
                    "deepseek-account",
                    "deepseek-v4-flash-vision-exp",
                ),
                ("opencode-go", "go-account", "deepseek-v4-flash-vision-exp"),
            ),
            (
                ("deepseek", "deepseek-account", "deepseek-v4-flash"),
                ("deepseek", "deepseek-account", "deepseek-v4-pro"),
            ),
            (
                ("deepseek", "account-a", "deepseek-v4-flash"),
                ("deepseek", "account-b", "deepseek-v4-flash"),
            ),
        ];
        for (previous, selected) in cases {
            // The process mirror deliberately remains on the previous selection after the durable picker write.
            // 可靠选择器写入后，故意让进程镜像仍停留在先前选择。
            let session_id = SessionId::new_v4();
            let (handle, _) = request_config_handle(
                session_id,
                ActiveRequestConfig {
                    provider: Some(previous.0.into()),
                    connection_id: Some(previous.1.into()),
                    model: Some(previous.2.into()),
                    thinking_level: Some("max".into()),
                    temperature: None,
                },
            );
            let mut mirror = SessionRuntimeStateEntry {
                active_provider_id: Some(previous.0.into()),
                active_connection_id: Some(previous.1.into()),
                active_model_id: Some(previous.2.into()),
                ..Default::default()
            };
            set_pending_model_request_v1(
                &handle,
                selected.0.into(),
                selected.1.into(),
                selected.2.into(),
                None,
                SessionRequestFallback::default(),
                2,
            )
            .unwrap();
            let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable().unwrap();
            synchronize_v1_dispatch_model_selection(&handle, &runtime, &mut mirror).unwrap();
            assert_eq!(mirror.active_provider_id.as_deref(), Some(previous.0));
            assert_eq!(mirror.active_connection_id.as_deref(), Some(previous.1));
            assert_eq!(mirror.active_model_id.as_deref(), Some(previous.2));
            // Reproduce the picker projection retained by submission before V1 root admission.
            // 复现 V1 根输入准入前，提交过程保留的选择器投影。
            handle
                .read(|document| {
                    crate::session_model_projection_v1::apply_request_model_projection_v1(
                        &mut mirror,
                        &document.metadata.config.active_request,
                        document.metadata.config.pending_request.as_ref(),
                    );
                })
                .unwrap();
            assert!(mirror.pending_model_selection.is_some());
            // A detached document runs the production user-bound activation and API admission
            // without any network or user-file writes.
            // 分离文档运行生产用户边界激活与 API 准入，不发网络请求、不写用户文件。
            let selected_request = ActiveRequestConfig {
                provider: Some(selected.0.into()),
                connection_id: Some(selected.1.into()),
                model: Some(selected.2.into()),
                thinking_level: Some("max".into()),
                temperature: None,
            };
            handle
                .submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("switch-root").unwrap(),
                        "continue",
                        vec![],
                    )
                    .with_bound_request_config(selected_request),
                    3,
                )
                .unwrap();
            // The real submission boundary receives no picker echo for an established session,
            // even though its detached workspace still has the pre-admission pending selection.
            // 真实提交边界不会收到既有会话的选择器回显，尽管分离工作区仍保留准入前待生效选择。
            let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
                task_projects: vec![],
                archived_task_projects: vec![],
                sessions: vec![],
                archived_sessions: vec![],
                session_runtime_state: [(session_id, mirror)].into_iter().collect(),
                archived_session_runtime_state: Default::default(),
                session_run_phases: Default::default(),
                expanded_task_projects: Default::default(),
                selected_session: session_id,
                hydrated_session_ids: Default::default(),
            };
            crate::conversation_submission::apply_submission_model_selection(
                &runtime,
                &mut workspace,
                session_id,
                crate::PromptSessionMaterializationKind::Existing,
                None,
            )
            .expect("V1-owned model switch must not require a legacy picker echo");
            // Only V1 dispatch synchronization may update the retained mirror, not the picker guard.
            // 只有 V1 分发同步可以更新保留镜像，而不是选择器校验。
            let mut mirror = workspace.session_runtime_state.remove(&session_id).unwrap();
            assert_eq!(mirror.active_model_id.as_deref(), Some(previous.2));
            assert!(mirror.pending_model_selection.is_some());
            synchronize_v1_dispatch_model_selection(&handle, &runtime, &mut mirror).unwrap();
            assert_eq!(mirror.active_provider_id.as_deref(), Some(selected.0));
            assert_eq!(mirror.active_connection_id.as_deref(), Some(selected.1));
            assert_eq!(mirror.active_model_id.as_deref(), Some(selected.2));
            let mut document = handle.read(Clone::clone).unwrap();
            let through_seq = document.events().last().unwrap().seq;
            let input = PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot {
                    provider: mirror.active_provider_id.clone().unwrap(),
                    profile: None,
                    connection_id: mirror.active_connection_id.clone(),
                    model: mirror.active_model_id.clone().unwrap(),
                    thinking_level: Some("max".into()),
                    temperature: None,
                },
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 5,
            };
            let mut stale = input.clone();
            stale.request.provider = previous.0.into();
            stale.request.connection_id = Some(previous.1.into());
            stale.request.model = previous.2.into();
            assert!(document.prepare_and_start_api(stale).is_err());
            document.prepare_and_start_api(input).unwrap();
            assert_eq!(
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(event.event, EventBody::RequestConfigChanged(_)))
                    .count(),
                1
            );
            assert_eq!(document.active_api().unwrap().request.provider, selected.0);
        }
    }

    /// An already-promoted durable choice repairs a stale cache on the next user turn too.
    /// 已提升的可靠选择在下一用户回合也能修正过期缓存。
    #[test]
    fn v1_dispatch_restores_active_model_without_pending_or_global_fallback() {
        let (handle, _) = request_config_handle(
            SessionId::new_v4(),
            ActiveRequestConfig {
                provider: Some("deepseek".into()),
                connection_id: Some("account".into()),
                model: Some("model".into()),
                ..Default::default()
            },
        );
        let mut mirror = SessionRuntimeStateEntry {
            active_provider_id: Some("opencode-go".into()),
            ..Default::default()
        };
        let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable().unwrap();
        synchronize_v1_dispatch_model_selection(&handle, &runtime, &mut mirror).unwrap();
        assert_eq!(mirror.active_provider_id.as_deref(), Some("deepseek"));
        assert_eq!(mirror.active_model_id.as_deref(), Some("model"));
        assert_eq!(handle.read(|document| document.events().len()).unwrap(), 0);
    }

    /// Verifies reasoning selection preserves the pending route and does not mutate the active request.
    /// 验证思考选择保留待生效路由，并且不修改活动请求。
    #[test]
    fn pending_reasoning_request_preserves_pending_route_until_next_boundary() {
        let session_id = SessionId::from_u128(0x6202);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("medium")),
                temperature: None,
            },
        );
        set_pending_model_request_v1(
            handle.as_ref(),
            String::from("provider-b"),
            String::from("connection-b"),
            String::from("model-b"),
            None,
            SessionRequestFallback::default(),
            2,
        )
        .expect("pending route should persist");

        assert!(
            set_pending_reasoning_request_v1(
                handle.as_ref(),
                ReasoningEffortLevel::XHigh,
                SessionRequestFallback::default(),
                3,
            )
            .expect("pending reasoning should persist")
        );
        let (active, pending) = handle
            .read(|document| {
                (
                    document.metadata.config.active_request.clone(),
                    document.metadata.config.pending_request.clone(),
                )
            })
            .expect("request metadata should read");
        assert_eq!(active.model.as_deref(), Some("model-a"));
        assert_eq!(active.thinking_level.as_deref(), Some("medium"));
        let pending = pending.expect("combined pending request should exist");
        assert_eq!(pending.provider.as_deref(), Some("provider-b"));
        assert_eq!(pending.connection_id.as_deref(), Some("connection-b"));
        assert_eq!(pending.model.as_deref(), Some("model-b"));
        assert_eq!(pending.thinking_level.as_deref(), Some("x-high"));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 2);
    }

    /// Verifies selecting the already-active complete request does not create pending metadata.
    /// 验证选择已经生效的完整请求不会创建待生效 metadata。
    #[test]
    fn active_request_selection_is_a_durable_noop() {
        let session_id = SessionId::from_u128(0x6203);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("high")),
                temperature: Some(0.5),
            },
        );

        assert!(
            !set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-a"),
                String::from("connection-a"),
                String::from("model-a"),
                None,
                SessionRequestFallback {
                    temperature: Some(0.5),
                    ..SessionRequestFallback::default()
                },
                2,
            )
            .expect("active route selection should succeed idempotently")
        );
        assert!(
            handle
                .read(|document| document.metadata.config.pending_request.is_none())
                .expect("pending request state should read")
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 0);
    }

    /// Verifies Provider-tool selection is durably stored once and exact replay does not flush.
    /// 验证 Provider 工具选择只可靠写入一次，精确重放不会再次 flush。
    #[test]
    fn provider_tool_selection_persists_once_and_replays_idempotently() {
        let session_id = SessionId::from_u128(0x6204);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let selection = ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            enabled_capabilities: vec![
                String::from("video_understand"),
                String::from("web_search"),
            ],
            selections: vec![
                vulcan_session_core::v1::config::ProviderToolCapabilitySelection {
                    capability: String::from("web_search"),
                    target: vulcan_session_core::v1::config::ProviderToolTargetConfig {
                        connection_id: String::from("connection-a"),
                        target_id: String::from("target-a"),
                    },
                },
            ],
            media_understanding_selections: vec![
                vulcan_session_core::v1::config::ProviderMediaUnderstandingSelection {
                    capability: String::from("video_understand"),
                    mode: vulcan_session_core::v1::config::ProviderMediaUnderstandingMode::ModelReplacement,
                    selection: vulcan_session_core::v1::config::ProviderMediaUnderstandingRouteConfig::ModelReplacement(
                            vulcan_session_core::v1::config::ProviderMediaUnderstandingModelTargetConfig {
                                provider_id: String::from("provider-video"),
                                model_id: String::from("model-video"),
                                connection_id: String::from("connection-video"),
                            },
                        ),
                    reasoning_effort: Some(
                        vulcan_session_core::v1::config::ProviderMediaUnderstandingReasoningEffort::Adaptive,
                    ),
                },
            ],
            extension_target_sets: Vec::new(),
        };

        assert!(
            set_provider_tool_selection_v1(handle.as_ref(), selection.clone(), 2)
                .expect("new Provider-tool selection should persist")
        );
        assert!(
            !set_provider_tool_selection_v1(handle.as_ref(), selection.clone(), 3)
                .expect("same Provider-tool selection should replay idempotently")
        );
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.provider_tools.clone())
                .expect("Provider-tool metadata should read"),
            selection
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
        apply_provider_tool_selection_v1(&mut runtime_state, &selection);
        assert_eq!(
            runtime_state
                .media_understanding_selections
                .get("video_understand"),
            Some(
                &vulcan_workbench_model::SessionMediaUnderstandingSelection::ModelReplacement {
                    selection: vulcan_workbench_model::SessionMediaUnderstandingModelSelection {
                        provider_id: String::from("provider-video"),
                        model_id: String::from("model-video"),
                        connection_id: String::from("connection-video"),
                    },
                    reasoning_effort: Some(
                        vulcan_workbench_model::SessionMediaUnderstandingReasoningEffort::Adaptive,
                    ),
                }
            )
        );
    }

    /// The complete first-turn selection bundle is one durable flush and exact replay is a no-op.
    /// 完整首轮选择配置只产生一次可靠 flush，精确重放保持无操作。
    #[test]
    fn session_selection_bundle_persists_as_one_revision_and_one_flush() {
        let session_id = SessionId::from_u128(0x6206);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let input = SessionSelectionBundleInput {
            output_style: Some(String::from("concise")),
            provider_tools: ProviderToolSelectionConfig {
                initialized: true,
                overridden: false,
                enabled_capabilities: vec![String::from("web_search")],
                selections: Vec::new(),
                media_understanding_selections: Vec::new(),
                extension_target_sets: Vec::new(),
            },
            knowledge_libraries: KnowledgeLibrarySelectionConfig {
                mode: vulcan_session_core::v1::config::KnowledgeLibrarySelectionMode::Explicit,
                library_ids: vec![String::from("library-a")],
                retrieval_mode: Some(
                    vulcan_session_core::v1::config::KnowledgeRetrievalMode::DisplayBm25,
                ),
            },
            vmm: VmmSessionPolicyConfig {
                precheck_enabled: Some(true),
                memory_writeback_enabled: Some(false),
            },
        };

        assert!(
            set_session_selection_bundle_v1(
                handle.as_ref(),
                input.clone(),
                AnswerMode::Standard,
                2
            )
            .expect("new selection bundle should persist")
        );
        assert!(
            !set_session_selection_bundle_v1(
                handle.as_ref(),
                input.clone(),
                AnswerMode::Standard,
                3
            )
            .expect("same selection bundle should replay idempotently")
        );
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.clone())
                .expect("selection metadata should read")
                .knowledge_libraries,
            input.knowledge_libraries
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// Draft answer mode commits with the selection bundle once and preserves unrelated settings.
    /// 草稿回答模式与选择包一次提交，并保留无关设置。
    #[test]
    fn answer_mode_draft_bundle_is_atomic_and_idempotent() {
        let (handle, backend) =
            request_config_handle(SessionId::new_v4(), ActiveRequestConfig::default());
        let selection = SessionSelectionBundleInput {
            output_style: Some("concise".into()),
            provider_tools: Default::default(),
            knowledge_libraries: Default::default(),
            vmm: Default::default(),
        };
        let before = handle.read(SessionDocument::memory_revision).unwrap();
        assert!(
            set_session_selection_bundle_v1(&handle, selection.clone(), AnswerMode::Interactive, 2)
                .unwrap()
        );
        assert_eq!(
            handle.read(SessionDocument::memory_revision).unwrap(),
            before + 1
        );
        assert_eq!(
            handle
                .read(|document| document.metadata.config.answer_mode.mode)
                .unwrap(),
            AnswerMode::Interactive
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
        assert!(
            !set_session_selection_bundle_v1(&handle, selection, AnswerMode::Interactive, 3)
                .unwrap()
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// Knowledge and VMM selections persist exactly once and preserve independent VMM fields.
    /// 知识库与 VMM 选择只持久化一次，并保留相互独立的 VMM 字段。
    #[test]
    fn knowledge_and_vmm_selections_persist_once_and_replay_idempotently() {
        let session_id = SessionId::from_u128(0x6205);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let knowledge = KnowledgeLibrarySelectionConfig {
            mode: vulcan_session_core::v1::config::KnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec![String::from("library-a"), String::from("library-b")],
            retrieval_mode: Some(
                vulcan_session_core::v1::config::KnowledgeRetrievalMode::DisplayHybrid,
            ),
        };

        assert!(
            set_knowledge_library_selection_v1(handle.as_ref(), knowledge.clone(), 2)
                .expect("new knowledge selection should persist")
        );
        assert!(
            !set_knowledge_library_selection_v1(handle.as_ref(), knowledge.clone(), 3)
                .expect("same knowledge selection should replay idempotently")
        );
        assert!(
            set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::Precheck,
                Some(false),
                4,
            )
            .expect("precheck override should persist")
        );
        assert!(
            set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::MemoryWriteback,
                Some(true),
                5,
            )
            .expect("writeback override should persist")
        );
        assert!(
            !set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::Precheck,
                Some(false),
                6,
            )
            .expect("same precheck override should replay idempotently")
        );
        let (persisted_knowledge, persisted_vmm) = handle
            .read(|document| {
                (
                    document
                        .metadata
                        .config
                        .selection
                        .knowledge_libraries
                        .clone(),
                    document.metadata.config.selection.vmm.clone(),
                )
            })
            .expect("selection metadata should read");
        assert_eq!(persisted_knowledge, knowledge);
        assert_eq!(persisted_vmm.precheck_enabled, Some(false));
        assert_eq!(persisted_vmm.memory_writeback_enabled, Some(true));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 3);
    }
}

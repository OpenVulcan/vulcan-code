use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use time::OffsetDateTime;
use uuid::Uuid;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_provider_adapters::{AccountUsageQueryKind, PersistedAiConfiguration, PublicAiRuntime};
use vulcan_workbench_model::{
    SessionProviderBudgetAccounting, SessionProviderBudgetAccountingSource,
    SessionProviderCallLedger, SessionProviderCallRecord, SessionProviderRequestContextBreakdown,
    SessionProviderRequestTextMeasurement,
};
use vulcan_workspace_store::workspace_store::{
    ProviderAccountUsageSnapshotRecord, ProviderRateLimitSnapshotRecord,
    ProviderUsageAttemptRecord, ProviderUsageDashboard, load_provider_usage_dashboard,
    persist_provider_account_usage_snapshot, persist_provider_rate_limit_snapshot,
    persist_provider_usage_attempt_and_rebuild_daily,
};

/// Loads the native provider usage dashboard through the service-owned VLDB controller.
/// 通过服务持有的 VLDB 控制器加载原生供应商用量仪表盘。
pub(crate) async fn service_provider_usage_dashboard(
    state: SharedServiceBridgeState,
) -> Result<ProviderUsageDashboard, String> {
    let controller = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("VLDB controller runtime is not ready"))?;
    load_provider_usage_dashboard(&controller).await
}

/// Persists the latest supported account-usage observations through the existing VLDB controller.
/// 通过现有 VLDB 控制器持久化最新受支持账户用量观测。
pub(crate) async fn persist_native_account_usage_snapshots(
    controller_runtime: &ControllerRuntimeHandle,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    snapshots: &BTreeMap<String, AccountUsageSnapshot>,
) -> Result<(), String> {
    let providers = runtime.effective_providers(configuration)?;
    for (connection_id, snapshot) in snapshots {
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| {
                format!("account-usage snapshot references unknown connection `{connection_id}`")
            })?;
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown provider `{}`",
                    connection.provider_id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown service entry `{}`",
                    connection.service_entry_id
                )
            })?;
        let credential_profile = entry
            .credential_profiles
            .iter()
            .find(|profile| profile.id == connection.credential_profile_id)
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown credential profile `{}`",
                    connection.credential_profile_id
                )
            })?;
        let revision_material = serde_json::to_vec(&(entry, credential_profile))
            .map_err(|error| format!("failed to serialize account endpoint profile: {error}"))?;
        let endpoint_profile_revision =
            format!("sha256:{}", hex::encode(Sha256::digest(&revision_material)));
        let payload_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("failed to serialize account-usage snapshot: {error}"))?;
        if payload_json.len() > 64 * 1024 {
            return Err(format!(
                "account-usage snapshot for connection `{connection_id}` exceeds 65536 bytes"
            ));
        }
        persist_provider_account_usage_snapshot(
            controller_runtime,
            &ProviderAccountUsageSnapshotRecord {
                snapshot_id: Uuid::new_v4().to_string(),
                provider_id: connection.provider_id.clone(),
                connection_id: connection_id.clone(),
                snapshot_kind: account_usage_snapshot_kind(credential_profile.account_usage_query)
                    .to_string(),
                endpoint_profile_revision,
                payload_json,
                observed_at_ms: current_provider_usage_unix_millis(),
            },
        )
        .await?;
    }
    Ok(())
}

/// Returns the stable snapshot category for one declared account query contract.
/// 返回一个已声明账户查询契约对应的稳定快照类别。
fn account_usage_snapshot_kind(query: AccountUsageQueryKind) -> &'static str {
    match query {
        AccountUsageQueryKind::MiniMaxTokenPlan | AccountUsageQueryKind::KimiCodingPlan => {
            "token_plan"
        }
        AccountUsageQueryKind::OpenRouterCredits
        | AccountUsageQueryKind::TavilyCredits
        | AccountUsageQueryKind::CommandCodeUsage => "quota",
        AccountUsageQueryKind::DeepSeekBalance => "balance",
        AccountUsageQueryKind::Unsupported => "unsupported",
    }
}

/// Returns the current Unix timestamp in milliseconds for native usage persistence.
/// 返回原生用量持久化使用的当前 Unix 毫秒时间戳。
fn current_provider_usage_unix_millis() -> i64 {
    i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

/// Exact in-memory state required to undo a Provider boundary whose durable write failed.
/// 可靠写入失败时撤销 Provider 边界所需的精确内存状态。
#[derive(Debug, Clone)]
struct ProviderUsageBoundaryRollback {
    /// Whether the session lives in the archived runtime-state map.
    /// 会话是否位于归档运行态映射中。
    archived: bool,
    /// Whether the owning runtime-state entry existed before the attempted boundary.
    /// 尝试写入边界前所属运行态条目是否已经存在。
    previous_runtime_state_existed: bool,
    /// Ledger value that was authoritative before the attempted mutation.
    /// 尝试变更前的权威账本值。
    previous_ledger: SessionProviderCallLedger,
    /// Runtime timestamp that accompanied the previous ledger.
    /// 与旧账本配套的运行态时间戳。
    previous_updated_at: String,
}

/// Captures one session ledger immediately before applying a durable Provider boundary.
/// 在应用可靠 Provider 边界前立即捕获一个会话账本。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative workspace and `session_id` selects the exact owner.
/// `state` 持有权威工作区，`session_id` 选择精确归属。
///
/// # Returns
/// 返回值
/// Returns the rollback facts or an explicit workspace-lock/readiness error.
/// 返回回滚事实，或显式的工作区锁定/就绪错误。
fn capture_provider_usage_boundary_rollback(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<ProviderUsageBoundaryRollback, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    let archived = workspace
        .archived_sessions
        .iter()
        .any(|session| session.id == session_id);
    let runtime_state = if archived {
        workspace.archived_session_runtime_state.get(&session_id)
    } else {
        workspace.session_runtime_state.get(&session_id)
    };
    Ok(ProviderUsageBoundaryRollback {
        archived,
        previous_runtime_state_existed: runtime_state.is_some(),
        previous_ledger: runtime_state
            .map(|runtime_state| runtime_state.provider_call_ledger.clone())
            .unwrap_or_default(),
        previous_updated_at: runtime_state
            .map(|runtime_state| runtime_state.updated_at.clone())
            .unwrap_or_default(),
    })
}

/// Restores only the attempted ledger mutation after durable persistence fails.
/// 在可靠持久化失败后仅恢复本次尝试的账本变更。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace, `session_id` selects the owner, `attempted_ledger` proves
/// no newer ledger mutation intervened, and `rollback` supplies the exact previous values.
/// `state` 持有当前工作区，`session_id` 选择归属，`attempted_ledger` 证明没有更新账本变更插入，
/// `rollback` 提供精确旧值。
///
/// # Returns
/// 返回值
/// Returns success after the precise rollback, or a fail-closed concurrency error.
/// 在精确回滚后返回成功，或返回封闭失败的并发错误。
fn restore_provider_usage_boundary_after_persist_failure(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    attempted_ledger: &SessionProviderCallLedger,
    rollback: ProviderUsageBoundaryRollback,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state for Provider rollback: {error}"))?;
    let workspace = guard
        .workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("workspace snapshot disappeared during Provider rollback"))?;
    let runtime_state_map = if rollback.archived {
        &mut workspace.archived_session_runtime_state
    } else {
        &mut workspace.session_runtime_state
    };
    if !rollback.previous_runtime_state_existed {
        let current_ledger = runtime_state_map
            .get(&session_id)
            .map(|runtime_state| &runtime_state.provider_call_ledger)
            .ok_or_else(|| {
                format!(
                    "Provider rollback could not find newly created runtime state for session `{session_id}`"
                )
            })?;
        if current_ledger != attempted_ledger {
            return Err(String::from(
                "Provider ledger advanced before a newly created runtime state could be rolled back",
            ));
        }
        runtime_state_map.remove(&session_id);
        return Ok(());
    }
    let runtime_state = runtime_state_map.get_mut(&session_id).ok_or_else(|| {
        format!("Provider rollback lost the pre-existing runtime state for session `{session_id}`")
    })?;
    restore_provider_usage_runtime_state_after_persist_failure(
        runtime_state,
        attempted_ledger,
        rollback,
    )
}

/// Restores one runtime entry only when it still contains the exact attempted ledger.
/// 仅当运行态条目仍包含精确尝试账本时才执行恢复。
///
/// # Parameters
/// 参数
/// `runtime_state` is the current session entry, `attempted_ledger` is the failed write value,
/// and `rollback` owns the exact previous ledger and timestamp.
/// `runtime_state` 是当前会话条目，`attempted_ledger` 是写入失败的值，`rollback` 持有精确旧账本与时间戳。
///
/// # Returns
/// 返回值
/// Returns success after restoration or rejects a newer intervening ledger mutation.
/// 在恢复后返回成功，或拒绝覆盖其间发生的更新账本变更。
fn restore_provider_usage_runtime_state_after_persist_failure(
    runtime_state: &mut SessionRuntimeStateEntry,
    attempted_ledger: &SessionProviderCallLedger,
    rollback: ProviderUsageBoundaryRollback,
) -> Result<(), String> {
    if &runtime_state.provider_call_ledger != attempted_ledger {
        return Err(String::from(
            "Provider ledger advanced before a failed durable boundary could be rolled back",
        ));
    }
    runtime_state.provider_call_ledger = rollback.previous_ledger;
    runtime_state.updated_at = rollback.previous_updated_at;
    Ok(())
}

/// Applies one Provider request telemetry event to the authoritative session runtime state.
/// 将一条 Provider 请求遥测事件应用到权威会话运行态。
///
/// # Parameters
/// 参数
/// `state` owns the mutable workspace snapshot and persisted session runtime maps.
/// `state` 持有可变工作区快照与持久化会话运行态映射。
/// `event` is the exact ordered telemetry event emitted by the Provider execution state machine.
/// `event` 是 Provider 执行状态机按序发出的精确遥测事件。
///
/// # Returns
/// 返回值
/// Returns a changed workspace snapshot when this event changed accounting, otherwise `None`.
/// 当事件改变统计时返回新的工作区快照，否则返回 `None`。
pub(crate) fn apply_provider_request_usage_telemetry(
    state: &SharedServiceBridgeState,
    event: &ProviderRequestTelemetryEvent,
) -> Result<Option<WorkspaceSnapshot>, String> {
    let provider_call_id = telemetry_string(event, "request_id")?;
    let identity = provider_call_identity_from_event(event)?;
    let changed_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let runtime_state = session_runtime_state_mut(workspace, event.session_id);
        // The context-start identity is frozen with the prepared call before any network dispatch.
        // 上下文起始身份会在任何网络分派前随 prepared 调用一起冻结。
        let context_start_record_id = runtime_state.context_start_message_id.clone();
        let changed = match event.kind {
            ProviderRequestTelemetryEventKind::Started => {
                let started = SessionProviderCallStarted {
                    provider_call_id,
                    identity,
                    prepared_at_ms: telemetry_required_i64(event, "started_at_ms")?,
                    request_assembly_fingerprint: telemetry_optional_string(
                        event,
                        "request_assembly_fingerprint",
                    ),
                    context_generation_id: telemetry_optional_string(
                        event,
                        "context_generation_id",
                    ),
                    context_start_record_id,
                    model_route_fingerprint: telemetry_optional_string(
                        event,
                        "model_route_fingerprint",
                    ),
                    toolset_fingerprint: telemetry_optional_string(event, "toolset_fingerprint"),
                    system_prompt_fingerprint: telemetry_optional_string(
                        event,
                        "system_prompt_fingerprint",
                    ),
                    request_text_measurement: Some(SessionProviderRequestTextMeasurement {
                        character_count: telemetry_required_usize(
                            event,
                            "request_character_count",
                        )?,
                        utf8_byte_count: telemetry_required_usize(
                            event,
                            "request_utf8_byte_count",
                        )?,
                        structural_units: telemetry_required_usize(
                            event,
                            "request_structural_units",
                        )?,
                        history_message_count: telemetry_required_usize(
                            event,
                            "request_history_message_count",
                        )?,
                        prompt_in_history: telemetry_required_bool(
                            event,
                            "request_prompt_in_history",
                        )?,
                        history_fingerprint: telemetry_optional_string(
                            event,
                            "request_history_fingerprint",
                        ),
                        prefix_invariant_fingerprint: telemetry_optional_string(
                            event,
                            "request_prefix_invariant_fingerprint",
                        ),
                        context_breakdown: request_context_breakdown_from_started_event(event)?,
                    }),
                    budget_accounting: budget_accounting_from_started_event(event)?,
                };
                let changed = runtime_state
                    .provider_call_ledger
                    .observe_started(started.clone());
                if !changed
                    && !runtime_state
                        .provider_call_ledger
                        .started_event_is_idempotent(&started)
                {
                    return Err(format!(
                        "Provider call `{}` repeated Started with conflicting immutable facts",
                        started.provider_call_id
                    ));
                }
                changed
            }
            ProviderRequestTelemetryEventKind::FirstToken => {
                runtime_state.provider_call_ledger.observe_first_event(
                    provider_call_id.as_str(),
                    telemetry_i64(event, "first_event_at_ms"),
                    telemetry_i64(event, "first_token_at_ms"),
                )
            }
            ProviderRequestTelemetryEventKind::Dispatched => {
                let changed = runtime_state.provider_call_ledger.observe_dispatched(
                    provider_call_id.as_str(),
                    telemetry_required_i64(event, "dispatched_at_ms")?,
                );
                let dispatch_is_already_persisted = runtime_state
                    .provider_call_ledger
                    .calls
                    .get(provider_call_id.as_str())
                    .is_some_and(|record| {
                        record.status
                            == vulcan_workbench_model::SessionProviderCallStatus::Dispatched
                    });
                if !changed && !dispatch_is_already_persisted {
                    return Err(format!(
                        "Provider call `{provider_call_id}` could not advance to dispatched state"
                    ));
                }
                changed
            }
            ProviderRequestTelemetryEventKind::Completed => {
                let status_text = telemetry_string(event, "status")?;
                let dispatch_outcome = telemetry_optional_string(event, "dispatch_outcome");
                let (_, dispatch_outcome_known) =
                    dispatch_facts(status_text.as_str(), dispatch_outcome.as_deref());
                if !runtime_state
                    .provider_call_ledger
                    .calls
                    .contains_key(provider_call_id.as_str())
                {
                    return Err(format!(
                        "Provider call `{provider_call_id}` reached Completed without a durably retained Started boundary"
                    ));
                }
                let completed = SessionProviderCallCompleted {
                    provider_call_id,
                    identity,
                    status: completed_call_status(status_text.as_str())?,
                    provider_usage_evidence: provider_usage_evidence_from_event(event),
                    prepared_at_ms: telemetry_required_i64(event, "started_at_ms")?,
                    dispatched_at_ms: telemetry_i64(event, "dispatched_at_ms"),
                    first_event_at_ms: telemetry_i64(event, "first_event_at_ms"),
                    first_token_at_ms: telemetry_i64(event, "first_token_at_ms"),
                    completed_at_ms: telemetry_required_i64(event, "completed_at_ms")?,
                    dispatch_outcome,
                    dispatch_outcome_known: Some(dispatch_outcome_known),
                    semantic_output_observed: telemetry_optional_bool(
                        event,
                        "semantic_output_observed",
                    ),
                    semantic_output_observed_known: Some(true),
                    replay_safe: telemetry_optional_bool(event, "replay_safe"),
                    error_category: telemetry_optional_string(event, "error_category"),
                    error_code: telemetry_optional_string(event, "error_code"),
                };
                let changed = runtime_state
                    .provider_call_ledger
                    .observe_completed(completed.clone());
                if !changed
                    && !runtime_state
                        .provider_call_ledger
                        .completed_event_is_idempotent(&completed)
                {
                    return Err(format!(
                        "Provider call `{}` repeated Completed with conflicting terminal facts",
                        completed.provider_call_id
                    ));
                }
                changed
            }
            ProviderRequestTelemetryEventKind::Retrying => false,
        };
        if !changed {
            return Ok(None);
        }
        runtime_state.updated_at = current_unix_millis_text();
        Some(workspace.clone())
    };
    Ok(changed_snapshot)
}

/// Accounts for one physical Provider boundary, persists the changed snapshot, emits the live add-on delta, and forwards the Hook event.
/// 处理一次真实 Provider 边界，持久化变化快照、发出实时附加信息增量，并转发 Hook 事件。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative in-memory workspace snapshot.
/// `state` 持有权威的内存工作区快照。
/// `controller_runtime` persists every changed regular-session snapshot.
/// `controller_runtime` 持久化每个发生变化的普通会话快照。
/// `app` optionally emits the WebUI delta; background service paths may omit it.
/// `app` 可选地发出 WebUI 增量；后台服务路径可以不提供它。
/// `provider_telemetry_guard` keeps ordered Hook delivery and cancellation tracking.
/// `provider_telemetry_guard` 负责有序 Hook 投递与取消跟踪。
/// `event` is the exact physical Provider request telemetry event.
/// `event` 是真实物理 Provider 请求的精确遥测事件。
/// `persist_reason` records why the snapshot was persisted.
/// `persist_reason` 记录快照持久化原因。
///
/// # Returns
/// 返回值
/// Returns an error only when authoritative state accounting or WebUI projection fails; Hook delivery remains best-effort.
/// 仅当权威状态统计或 WebUI 投影失败时返回错误；Hook 投递仍保持尽力而为。
pub(crate) async fn process_provider_request_telemetry_event(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    app: Option<&ServiceEventEmitter>,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ProviderRequestTelemetryEvent,
    persist_reason: &str,
) -> Result<(), String> {
    let session_id = event.session_id;
    let is_usage_boundary = matches!(
        event.kind,
        ProviderRequestTelemetryEventKind::Started
            | ProviderRequestTelemetryEventKind::Dispatched
            | ProviderRequestTelemetryEventKind::FirstToken
            | ProviderRequestTelemetryEventKind::Completed
    );
    let accounting_result = if is_usage_boundary {
        let rollback = capture_provider_usage_boundary_rollback(state, session_id)?;
        // Keep accounting errors separate so the ordered Hook event is still delivered.
        // 将状态记账错误与 Hook 事件投递分离，确保有序 Hook 事件仍然发送。
        match apply_provider_request_usage_telemetry(state, &event) {
            Err(error) => Err(error),
            Ok(changed_snapshot) => {
                let persistence_result = if let Some(snapshot) = changed_snapshot {
                    let attempted_ledger = snapshot
                        .session_runtime_state
                        .get(&session_id)
                        .or_else(|| snapshot.archived_session_runtime_state.get(&session_id))
                        .map(|runtime_state| runtime_state.provider_call_ledger.clone())
                        .ok_or_else(|| {
                            format!(
                                "Session `{session_id}` lost its Provider ledger before persistence."
                            )
                        })?;
                    persist_provider_call_boundary(
                        state,
                        controller_runtime,
                        &snapshot,
                        session_id,
                    )
                    .await
                    .map_err(|error| {
                        format!(
                            "{persist_reason}: failed to durably persist Provider call boundary: {error}"
                        )
                    })
                    .and_then(|()| {
                        if event.kind == ProviderRequestTelemetryEventKind::Completed {
                            event.replace_durable_budget_baseline(
                                durable_provider_usage_baseline(&snapshot, session_id)?,
                            )?;
                        }
                        Ok(())
                    })
                    .or_else(|error| {
                        restore_provider_usage_boundary_after_persist_failure(
                            state,
                            session_id,
                            &attempted_ledger,
                            rollback,
                        )
                        .map_err(|rollback_error| {
                            format!("{error}; in-memory Provider rollback failed: {rollback_error}")
                        })?;
                        Err(error)
                    })
                } else {
                    Ok(())
                };
                persistence_result.and_then(|()| {
                    if let Some(app) = app {
                        emit_session_addons_changed_delta(app, state, session_id)?;
                    }
                    Ok(())
                })
            }
        }
    } else {
        Ok(())
    };
    provider_telemetry_guard.forward(event);
    accounting_result
}

/// Resolves the exact stable call record named by one durably persisted session ledger.
/// 解析一份已持久化会话账本所指向的精确稳定调用记录。
fn durable_provider_usage_baseline(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<Option<SessionProviderCallRecord>, String> {
    let ledger = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| format!("Session `{session_id}` has no projected Provider call ledger."))?
        .provider_call_ledger;
    let Some(baseline_ref) = ledger.last_stable_context_baseline.as_ref() else {
        return Ok(None);
    };
    ledger
        .calls
        .get(baseline_ref.provider_call_id.as_str())
        .cloned()
        .map(Some)
        .ok_or_else(|| {
            format!(
                "Session `{session_id}` Provider baseline `{}` is missing from its durable ledger.",
                baseline_ref.provider_call_id
            )
        })
}

/// Persists one physical-call boundary into the exact owning session store.
/// 将一次物理调用边界持久化到精确所属的会话存储。
///
/// # Parameters
/// 参数
/// `state` supplies the resident TaskRuntime when the owner is a child task.
/// `state` 在所有者是子任务时提供常驻 TaskRuntime。
/// `controller_runtime` owns ordinary WebUI session persistence.
/// `controller_runtime` 持有普通 WebUI 会话持久化能力。
/// `workspace_snapshot` contains the already-projected ledger revision.
/// `workspace_snapshot` 包含已完成投影的账本修订版。
/// `session_id` is the exact main-session or TaskSession owner.
/// `session_id` 是精确的主会话或 TaskSession 所有者。
///
/// # Returns
/// 返回值
/// Returns only after the owner-specific durable boundary has completed.
/// 仅在所属会话的专用持久化边界完成后返回。
async fn persist_provider_call_boundary(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
        return persist_workspace_session_update(
            Some(state),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
        )
        .await;
    }
    let provider_call_ledger = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| format!("TaskSession `{session_id}` has no projected runtime state."))?
        .provider_call_ledger
        .clone();
    let task_runtime = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.task_runtime.clone())
        .ok_or_else(|| {
            format!(
                "TaskSession `{session_id}` has no installed TaskRuntime for Usage persistence."
            )
        })?;
    task_runtime.persist_task_provider_call_ledger(session_id, &provider_call_ledger)
}

/// Persists one started or terminal Provider boundary and always releases its execution gate.
/// 持久化一个 Provider 开始或终态边界，并始终释放其执行闸门。
///
/// # Parameters
/// 参数
/// The arguments are the same authoritative telemetry inputs plus the single-use completion handle.
/// 参数为相同的权威遥测输入以及一次性完成句柄。
///
/// # Returns
/// 返回值
/// Returns the exact accounting or persistence result delivered to the waiting Provider runtime.
/// 返回传递给等待中 Provider 运行时的精确记账或持久化结果。
pub(crate) async fn process_provider_request_durability_boundary(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    app: Option<&ServiceEventEmitter>,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ProviderRequestTelemetryEvent,
    completion: ProviderRequestDurabilityCompletion,
    persist_reason: &str,
) -> Result<(), String> {
    let result = process_provider_request_telemetry_event(
        state,
        controller_runtime,
        app,
        provider_telemetry_guard,
        event,
        persist_reason,
    )
    .await;
    let _ = completion.complete(result.clone());
    result
}

/// Persists one terminal physical Provider attempt in the native controller-backed ledger.
/// 将一条终态真实 Provider attempt 持久化到控制器承载的原生账本。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the existing application SQLite binding and `event` is the exact terminal telemetry event.
/// `controller_runtime` 持有既有应用 SQLite 绑定，`event` 是精确终态遥测事件。
///
/// # Returns
/// 返回值
/// Success after idempotent persistence, or a controlled validation/database error.
/// 幂等持久化后返回成功，否则返回受控校验或数据库错误。
pub(crate) async fn persist_native_provider_usage_attempt(
    controller_runtime: &ControllerRuntimeHandle,
    event: &ProviderRequestTelemetryEvent,
) -> Result<(), String> {
    if !matches!(event.kind, ProviderRequestTelemetryEventKind::Completed) {
        return Ok(());
    }
    let status = telemetry_string(event, "status")?;
    let usage = event.completed_usage.as_ref();
    let usage_availability = usage_availability(usage).to_string();
    // dispatchOutcome keeps proven dispatch, proven pre-dispatch rejection, and unknown delivery distinct.
    // dispatchOutcome 将已证明分派、已证明分派前拒绝与未知送达状态明确区分。
    let dispatch_outcome = telemetry_optional_string(event, "dispatch_outcome");
    let (dispatched, dispatched_known) =
        dispatch_facts(status.as_str(), dispatch_outcome.as_deref());
    let semantic_output_observed =
        telemetry_optional_bool(event, "semantic_output_observed").unwrap_or(false);
    let record = ProviderUsageAttemptRecord {
        logical_request_id: telemetry_string(event, "execution_id")?,
        attempt_id: telemetry_string(event, "request_id")?,
        attempt_index: telemetry_required_i64(event, "attempt_index")?,
        failover_index: None,
        consumer_id: telemetry_string(event, "owner_id")?,
        operation: telemetry_string(event, "operation")?,
        provider_id: telemetry_string(event, "provider_key")?,
        service_entry_id: telemetry_string(event, "service_entry_id")?,
        connection_id: telemetry_string(event, "connection_id")?,
        route_id: telemetry_optional_string(event, "secondary_route_id"),
        protocol_profile_id: telemetry_string(event, "protocol_profile_id")?,
        route_revision: telemetry_optional_string(event, "route_revision"),
        transport: telemetry_optional_string(event, "transport"),
        requested_model: telemetry_string(event, "model_key")?,
        mapped_model: telemetry_string(event, "model_key")?,
        response_model: usage.and_then(|value| value.response_model.clone()),
        response_id: usage.and_then(|value| value.response_id.clone()),
        provider_request_id: telemetry_optional_string(event, "provider_request_id"),
        dispatched_at_ms: dispatched
            .then(|| {
                telemetry_i64(event, "dispatched_at_ms")
                    .or_else(|| telemetry_i64(event, "started_at_ms"))
            })
            .flatten(),
        headers_at_ms: telemetry_i64(event, "headers_at_ms"),
        first_semantic_at_ms: telemetry_i64(event, "first_event_at_ms"),
        terminal_at_ms: telemetry_required_i64(event, "completed_at_ms")?,
        status,
        dispatched,
        dispatched_known,
        semantic_output_observed,
        semantic_output_observed_known: true,
        replay_safe: telemetry_optional_bool(event, "replay_safe"),
        http_status: telemetry_i64(event, "http_status"),
        websocket_close_code: telemetry_i64(event, "websocket_close_code"),
        error_category: telemetry_optional_string(event, "error_category"),
        error_code: telemetry_optional_string(event, "error_code"),
        usage_availability,
        input_tokens: usage_i64(usage.and_then(|value| value.input_tokens), "input_tokens")?,
        uncached_input_tokens: usage_i64(
            usage.and_then(|value| value.uncached_input_tokens),
            "uncached_input_tokens",
        )?,
        cache_read_tokens: usage_i64(
            usage.and_then(|value| value.cached_input_tokens),
            "cached_input_tokens",
        )?,
        cache_write_tokens: usage_i64(
            usage.and_then(|value| value.cache_creation_input_tokens),
            "cache_creation_input_tokens",
        )?,
        output_tokens: usage_i64(usage.and_then(|value| value.output_tokens), "output_tokens")?,
        reasoning_tokens: usage_i64(
            usage.and_then(|value| value.reasoning_tokens),
            "reasoning_tokens",
        )?,
        audio_input_tokens: usage_i64(
            usage.and_then(|value| value.audio_input_tokens),
            "audio_input_tokens",
        )?,
        audio_output_tokens: usage_i64(
            usage.and_then(|value| value.audio_output_tokens),
            "audio_output_tokens",
        )?,
        total_tokens: usage_i64(usage.and_then(|value| value.total_tokens), "total_tokens")?,
        usage_profile_id: usage.and_then(|value| value.extraction_profile_id.clone()),
        usage_profile_revision: usage
            .and_then(|value| value.extraction_profile_revision)
            .map(|value| {
                i64::try_from(value)
                    .map_err(|_| String::from("usage profile revision exceeds SQLite INTEGER"))
            })
            .transpose()?,
        usage_source_paths_json: usage
            .filter(|value| !value.source_paths.is_empty())
            .map(|value| serde_json::to_string(&value.source_paths))
            .transpose()
            .map_err(|error| format!("failed to serialize usage source paths: {error}"))?,
        usage_validation_flags_json: usage
            .filter(|value| !value.validation_flags.is_empty())
            .map(|value| serde_json::to_string(&value.validation_flags))
            .transpose()
            .map_err(|error| format!("failed to serialize usage validation flags: {error}"))?,
        provider_billing_details_json: usage
            .filter(|value| !value.billing_details.is_empty())
            .map(|value| serde_json::to_string(&value.billing_details))
            .transpose()
            .map_err(|error| format!("failed to serialize provider billing details: {error}"))?,
        calendar_date: telemetry_string(event, "calendar_date")?,
    };
    persist_provider_usage_attempt_and_rebuild_daily(
        controller_runtime,
        &record,
        telemetry_required_i64(event, "completed_at_ms")?,
    )
    .await?;
    let retry_after = event
        .payload
        .get("retry_after")
        .filter(|value| !value.is_null());
    let rate_limit_headers = event
        .payload
        .get("rate_limit_headers")
        .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()));
    let provider_request_id = telemetry_optional_string(event, "provider_request_id");
    if retry_after.is_some() || rate_limit_headers.is_some() || provider_request_id.is_some() {
        let retry_after_ms = retry_after
            .cloned()
            .unwrap_or(Value::Null)
            .get("delay_milliseconds")
            .and_then(Value::as_u64)
            .map(|value| {
                i64::try_from(value)
                    .map_err(|_| String::from("retry-after delay exceeds SQLite INTEGER"))
            })
            .transpose()?;
        let headers_json = serde_json::to_string(&serde_json::json!({
            "retry_after": retry_after,
            "rate_limit_headers": rate_limit_headers,
            "provider_request_id": provider_request_id,
        }))
        .map_err(|error| format!("failed to serialize normalized rate-limit snapshot: {error}"))?;
        persist_provider_rate_limit_snapshot(
            controller_runtime,
            &ProviderRateLimitSnapshotRecord {
                snapshot_id: Uuid::new_v4().to_string(),
                attempt_id: record.attempt_id.clone(),
                headers_json,
                retry_after_ms,
                observed_at_ms: record.terminal_at_ms,
            },
        )
        .await?;
    }
    Ok(())
}

/// Returns the active or archived runtime entry for one session without guessing ownership.
/// 在不猜测归属的前提下返回一个会话的活动或归档运行态条目。
fn session_runtime_state_mut(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> &mut SessionRuntimeStateEntry {
    if workspace
        .archived_sessions
        .iter()
        .any(|session| session.id == session_id)
    {
        workspace
            .archived_session_runtime_state
            .entry(session_id)
            .or_default()
    } else {
        workspace
            .session_runtime_state
            .entry(session_id)
            .or_default()
    }
}

/// Reads one required string field from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个必需字符串字段。
fn telemetry_string(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<String, String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("provider telemetry payload is missing non-empty {key}"))
}

/// Reads one optional signed millisecond field from the telemetry payload.
/// 从遥测载荷读取一个可选的有符号毫秒字段。
fn telemetry_i64(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<i64> {
    event.payload.get(key).and_then(Value::as_i64)
}

/// Reads one required signed integer from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个必需有符号整数。
fn telemetry_required_i64(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<i64, String> {
    telemetry_i64(event, key)
        .ok_or_else(|| format!("provider telemetry payload is missing integer {key}"))
}

/// Reads one required unsigned 32-bit integer from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个必需的无符号 32 位整数。
fn telemetry_required_u32(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<u32, String> {
    let value = event
        .payload
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("provider telemetry payload is missing unsigned integer {key}"))?;
    u32::try_from(value).map_err(|_| format!("provider telemetry payload field {key} exceeds u32"))
}

/// Reads one required unsigned 64-bit integer from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个必需的无符号 64 位整数。
fn telemetry_required_u64(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<u64, String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("provider telemetry payload is missing unsigned integer {key}"))
}

/// Reads one required host-sized unsigned integer from the telemetry payload.
/// 从遥测载荷读取一个必需的宿主尺寸无符号整数。
fn telemetry_required_usize(
    event: &ProviderRequestTelemetryEvent,
    key: &str,
) -> Result<usize, String> {
    let value = telemetry_required_u64(event, key)?;
    usize::try_from(value)
        .map_err(|_| format!("provider telemetry payload field {key} exceeds usize"))
}

/// Reads one required boolean from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个必需布尔值。
fn telemetry_required_bool(
    event: &ProviderRequestTelemetryEvent,
    key: &str,
) -> Result<bool, String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("provider telemetry field `{key}` must be a boolean"))
}

/// Projects request-admission accounting without converting it into Provider usage evidence.
/// 投影请求准入记账且不把它转换为 Provider 用量证据。
fn budget_accounting_from_started_event(
    event: &ProviderRequestTelemetryEvent,
) -> Result<Option<SessionProviderBudgetAccounting>, String> {
    let Some(source) = telemetry_optional_string(event, "budget_accounting_source") else {
        return Ok(None);
    };
    let source = match source.as_str() {
        "provider_reported" => SessionProviderBudgetAccountingSource::ProviderReported,
        "provider_anchor_plus_character_delta" => {
            SessionProviderBudgetAccountingSource::ProviderAnchorPlusCharacterDelta
        }
        "character_estimate" => SessionProviderBudgetAccountingSource::CharacterEstimate,
        value => {
            return Err(format!(
                "unknown Provider request budget accounting source: {value}"
            ));
        }
    };
    Ok(Some(SessionProviderBudgetAccounting {
        source,
        input_tokens: event
            .payload
            .get("budget_input_tokens")
            .and_then(Value::as_u64)
            .map(usize::try_from)
            .transpose()
            .map_err(|_| String::from("budget_input_tokens exceeds usize"))?,
        output_tokens: None,
        total_tokens: event
            .payload
            .get("budget_total_tokens")
            .and_then(Value::as_u64)
            .map(usize::try_from)
            .transpose()
            .map_err(|_| String::from("budget_total_tokens exceeds usize"))?,
        warning_code: telemetry_optional_string(event, "budget_warning_code"),
    }))
}

/// Projects optional request-category weights from one Started telemetry event.
/// 从单条 Started 遥测事件投影可选请求分类权重。
fn request_context_breakdown_from_started_event(
    event: &ProviderRequestTelemetryEvent,
) -> Result<Option<SessionProviderRequestContextBreakdown>, String> {
    const KEYS: [&str; 6] = [
        "request_message_units",
        "request_system_tool_units",
        "request_mcp_tool_units",
        "request_skill_units",
        "request_system_prompt_units",
        "request_other_units",
    ];
    if KEYS.iter().all(|key| event.payload.get(*key).is_none()) {
        return Ok(None);
    }
    Ok(Some(SessionProviderRequestContextBreakdown {
        message_units: telemetry_required_usize(event, KEYS[0])?,
        system_tool_units: telemetry_required_usize(event, KEYS[1])?,
        mcp_tool_units: telemetry_required_usize(event, KEYS[2])?,
        skill_units: telemetry_required_usize(event, KEYS[3])?,
        system_prompt_units: telemetry_required_usize(event, KEYS[4])?,
        other_units: telemetry_required_usize(event, KEYS[5])?,
    }))
}

/// Parses one ordered projection-profile revision array from its exact telemetry key.
/// 从精确遥测键解析一组有序投影 Profile 修订。
fn telemetry_profile_revisions(
    event: &ProviderRequestTelemetryEvent,
    key: &str,
) -> Result<Vec<SessionProviderProfileRevision>, String> {
    let values = event
        .payload
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("provider telemetry payload is missing array {key}"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let profile_id = value
                .get("profile_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!("provider telemetry payload {key}[{index}] is missing profile_id")
                })?
                .to_string();
            let revision = value
                .get("revision")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("provider telemetry payload {key}[{index}] is missing revision")
                })?;
            Ok(SessionProviderProfileRevision {
                profile_id,
                revision,
            })
        })
        .collect()
}

/// Projects the exact typed telemetry identity into one immutable session-ledger identity.
/// 将精确强类型遥测身份投影为不可变会话账本身份。
fn provider_call_identity_from_event(
    event: &ProviderRequestTelemetryEvent,
) -> Result<SessionProviderCallIdentity, String> {
    let execution_id = telemetry_string(event, "execution_id")?;
    Ok(SessionProviderCallIdentity {
        // The current telemetry contract defines execution_id as the logical run shared by attempts.
        // 当前遥测契约将 execution_id 定义为多个 attempt 共享的逻辑运行身份。
        logical_request_id: execution_id.clone(),
        execution_id,
        turn_id: telemetry_optional_string(event, "turn_id"),
        attempt_index: telemetry_required_u32(event, "attempt_index")?,
        retry_number: telemetry_required_u32(event, "retry_count")?,
        owner_kind: telemetry_string(event, "owner_kind")?,
        owner_id: telemetry_string(event, "owner_id")?,
        owner_name_snapshot: telemetry_string(event, "owner_name")?,
        operation: telemetry_string(event, "operation")?,
        provider_id: telemetry_string(event, "provider_key")?,
        provider_name_snapshot: telemetry_string(event, "provider_name")?,
        model_id: telemetry_string(event, "model_key")?,
        model_name_snapshot: telemetry_string(event, "model_name")?,
        connection_id: telemetry_string(event, "connection_id")?,
        service_entry_id: telemetry_string(event, "service_entry_id")?,
        protocol_profile_id: telemetry_string(event, "protocol_profile_id")?,
        manifest_revision: telemetry_required_u64(event, "manifest_revision")?,
        manifest_generation: telemetry_required_u64(event, "manifest_generation")?,
        binding_source: telemetry_string(event, "binding_source")?,
        binding_rule: telemetry_optional_string(event, "binding_rule_id"),
        route_revision: telemetry_string(event, "route_revision")?,
        request_profiles: telemetry_profile_revisions(event, "request_profiles")?,
        response_profiles: telemetry_profile_revisions(event, "response_profiles")?,
    })
}

/// Converts one optional host token count into an exact SQLite integer without saturation.
/// 将一个可选宿主 Token 数精确转换为 SQLite 整数且不进行饱和处理。
fn usage_i64(value: Option<usize>, field: &str) -> Result<Option<i64>, String> {
    value
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| format!("provider usage field {field} exceeds SQLite INTEGER"))
        })
        .transpose()
}

/// Resolves missing, partial, or final availability from explicit provider merge evidence.
/// 根据显式 Provider 合并证据解析缺失、部分或终态可用性。
fn usage_availability(usage: Option<&ProviderRequestCompletedUsage>) -> &'static str {
    match usage {
        Some(value)
            if matches!(
                value.update_kind,
                Some(vulcan_ai_contract::UsageUpdateKind::Final)
            ) =>
        {
            "final"
        }
        Some(_) => "partial",
        None => "missing",
    }
}

/// Reads one optional non-empty string field from the telemetry payload.
/// 从遥测载荷读取一个可选的非空字符串字段。
fn telemetry_optional_string(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Reads one optional boolean field from the versioned telemetry payload.
/// 从版本化遥测载荷读取一个可选布尔字段。
fn telemetry_optional_bool(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<bool> {
    event.payload.get(key).and_then(Value::as_bool)
}

/// Resolves dispatch value and certainty from one terminal telemetry fact set.
/// 从一组终态遥测事实解析分派值与确定性。
///
/// # Parameters
/// 参数
/// `status` is the terminal attempt status and `dispatch_outcome` is the typed transport outcome.
/// `status` 是 attempt 终态，`dispatch_outcome` 是强类型传输结果。
///
/// # Returns
/// 返回值
/// Returns `(dispatched, known)` without converting unknown delivery into a proven negative.
/// 返回 `(是否分派, 是否已知)`，且不会把未知送达转换为已证明的否定。
fn dispatch_facts(status: &str, dispatch_outcome: Option<&str>) -> (bool, bool) {
    let dispatched = status == "succeeded" || dispatch_outcome == Some("dispatched");
    let known =
        status == "succeeded" || matches!(dispatch_outcome, Some("dispatched" | "not_dispatched"));
    (dispatched, known)
}

/// Converts the exact terminal payload status into the persisted call state.
/// 将精确终态载荷状态转换为持久化调用状态。
fn completed_call_status(status: &str) -> Result<SessionProviderCallStatus, String> {
    match status {
        "succeeded" => Ok(SessionProviderCallStatus::Succeeded),
        "failed" => Ok(SessionProviderCallStatus::Failed),
        "cancelled" => Ok(SessionProviderCallStatus::Cancelled),
        status => Err(format!(
            "unknown provider telemetry terminal status: {status}"
        )),
    }
}

/// Converts typed terminal usage into Provider-only ledger evidence.
/// 将强类型终态用量转换为仅含 Provider 事实的账本证据。
fn provider_usage_evidence_from_event(
    event: &ProviderRequestTelemetryEvent,
) -> SessionProviderUsageEvidence {
    let availability = match usage_availability(event.completed_usage.as_ref()) {
        "final" => SessionProviderUsageAvailability::Final,
        "partial" => SessionProviderUsageAvailability::Partial,
        _ => SessionProviderUsageAvailability::Missing,
    };
    SessionProviderUsageEvidence {
        availability,
        provider_usage: provider_usage_snapshot_from_event(event),
    }
}

/// Converts normalized telemetry usage into the persisted Provider usage snapshot.
/// 将规范化遥测用量转换为持久化 Provider 用量快照。
fn provider_usage_snapshot_from_event(
    event: &ProviderRequestTelemetryEvent,
) -> Option<ProviderUsageSnapshot> {
    let usage = event.completed_usage.as_ref()?;
    Some(ProviderUsageSnapshot {
        update_kind: usage.update_kind,
        input_tokens: usage.input_tokens,
        uncached_input_tokens: usage.uncached_input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage.reasoning_tokens,
        total_tokens: usage.total_tokens,
        audio_input_tokens: usage.audio_input_tokens,
        audio_output_tokens: usage.audio_output_tokens,
        extraction_profile_id: usage.extraction_profile_id.clone(),
        extraction_profile_revision: usage.extraction_profile_revision,
        source_paths: usage.source_paths.clone(),
        validation_flags: usage.validation_flags.clone(),
        response_model: usage.response_model.clone(),
        response_id: usage.response_id.clone(),
        billing_details: usage.billing_details.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Shell host used by isolated Provider ledger service-state tests.
    /// 隔离 Provider 账本服务状态测试使用的宿主壳。
    struct ProviderUsageTestShellHost;

    impl ServiceShellHost for ProviderUsageTestShellHost {
        /// Rejects local path opening because Provider ledger tests never invoke shell integration.
        /// 拒绝打开本地路径，因为 Provider 账本测试不会调用壳集成。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from(
                "path opening is unavailable in Provider usage tests",
            ))
        }

        /// Rejects URL opening because Provider ledger tests never invoke browser integration.
        /// 拒绝打开 URL，因为 Provider 账本测试不会调用浏览器集成。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from(
                "URL opening is unavailable in Provider usage tests",
            ))
        }
    }

    /// Builds shared service state with an empty workspace for one candidate TaskSession.
    /// 为一个候选 TaskSession 构建带空工作区的共享服务状态。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` becomes the selected placeholder identity without declaring a regular session.
    /// `session_id` 成为选中占位身份，但不声明普通会话。
    ///
    /// # Returns
    /// 返回值
    /// A shared state whose runtime maps are empty before the first Provider boundary.
    /// 返回在首个 Provider 边界前运行态映射为空的共享状态。
    fn provider_usage_test_state(session_id: SessionId) -> SharedServiceBridgeState {
        let state = create_service_bridge_state(Arc::new(ProviderUsageTestShellHost));
        let workspace = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        state
            .lock()
            .expect("test service state should lock")
            .workspace_snapshot = Some(workspace);
        state
    }

    /// Builds one complete telemetry payload shared by started and completed event tests.
    /// 构建开始与完成事件测试共享的一份完整遥测载荷。
    ///
    /// # Parameters
    /// 参数
    /// `status` supplies a terminal status only for completed events.
    /// `status` 仅为完成事件提供终态。
    ///
    /// # Returns
    /// 返回值
    /// A privacy-safe payload satisfying the current typed telemetry contract.
    /// 返回满足当前强类型遥测契约且不含敏感信息的载荷。
    fn provider_usage_test_payload(status: Option<&str>) -> Value {
        let mut payload = serde_json::json!({
            "request_id": "call-1",
            "execution_id": "execution-1",
            "attempt_index": 0,
            "retry_count": 0,
            "owner_kind": "subagent",
            "owner_id": "task-1",
            "owner_name": "Task One",
            "operation": "conversation",
            "provider_key": "provider-1",
            "provider_name": "Provider One",
            "model_key": "model-1",
            "model_name": "Model One",
            "connection_id": "connection-1",
            "service_entry_id": "public-api",
            "protocol_profile_id": "openai.responses",
            "manifest_revision": 1,
            "manifest_generation": 1,
            "binding_source": "system_manifest",
            "route_revision": "sha256:route",
            "request_profiles": [],
            "response_profiles": [],
            "started_at_ms": 1,
            "request_character_count": 10,
            "request_utf8_byte_count": 10,
            "request_structural_units": 1,
            "request_history_message_count": 1,
            "request_prompt_in_history": true
        });
        if let Some(status) = status {
            payload["status"] = Value::String(status.to_string());
            payload["completed_at_ms"] = Value::from(2);
            payload["semantic_output_observed"] = Value::Bool(status == "succeeded");
        }
        payload
    }

    /// Builds one detached Provider telemetry event for service-state accounting tests.
    /// 为服务状态记账测试构建一条分离的 Provider 遥测事件。
    ///
    /// # Parameters
    /// 参数
    /// `kind` selects the lifecycle boundary, `session_id` selects its owner, and `status` is terminal-only.
    /// `kind` 选择生命周期边界，`session_id` 选择归属，`status` 仅用于终态。
    ///
    /// # Returns
    /// 返回值
    /// A detached event with no Provider Usage payload.
    /// 返回不含 Provider Usage 载荷的分离事件。
    fn provider_usage_test_event(
        kind: ProviderRequestTelemetryEventKind,
        session_id: SessionId,
        status: Option<&str>,
    ) -> ProviderRequestTelemetryEvent {
        ProviderRequestTelemetryEvent::detached(
            kind,
            session_id,
            provider_usage_test_payload(status),
            None,
        )
    }

    /// Verifies telemetry usage conversion preserves separate cached and uncached inputs.
    /// 验证遥测用量转换会保留缓存与非缓存输入分项。
    #[test]
    fn telemetry_usage_snapshot_preserves_input_components() {
        let event = ProviderRequestTelemetryEvent::detached(
            ProviderRequestTelemetryEventKind::Completed,
            SessionId::new_v4(),
            serde_json::json!({
                "request_id": "call-1",
                "owner_kind": "main_agent",
                "status": "succeeded",
            }),
            Some(
                vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestCompletedUsage {
                    uncached_input_tokens: Some(4),
                    cached_input_tokens: Some(6),
                    cache_creation_input_tokens: Some(3),
                    output_tokens: Some(2),
                    reasoning_tokens: Some(1),
                    total_tokens: Some(12),
                    billing_details: vec![vulcan_ai_contract::UsageBillingDetail {
                        billing_type: String::from("response_api"),
                        input_tokens: Some(4),
                        output_tokens: Some(2),
                        total_tokens: Some(6),
                        text_tokens: Some(4),
                        image_tokens: None,
                        reasoning_tokens: Some(1),
                    }],
                    ..Default::default()
                },
            ),
        );
        let usage = provider_usage_snapshot_from_event(&event).expect("usage should exist");
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.uncached_input_tokens, Some(4));
        assert_eq!(usage.cached_input_tokens, Some(6));
        assert_eq!(usage.cache_creation_input_tokens, Some(3));
        assert_eq!(usage.total_tokens, Some(12));
        assert_eq!(usage.billing_details.len(), 1);
        assert_eq!(usage.billing_details[0].billing_type, "response_api");
    }

    /// Verifies unknown delivery never becomes a proven negative dispatch fact.
    /// 验证未知送达绝不会变成已证明的未分派事实。
    #[test]
    fn dispatch_facts_preserve_unknown_delivery() {
        assert_eq!(dispatch_facts("failed", Some("dispatched")), (true, true));
        assert_eq!(
            dispatch_facts("failed", Some("not_dispatched")),
            (false, true)
        );
        assert_eq!(dispatch_facts("failed", Some("unknown")), (false, false));
        assert_eq!(dispatch_facts("failed", None), (false, false));
        assert_eq!(dispatch_facts("succeeded", None), (true, true));
    }

    /// Verifies a successful request cannot promote snapshot-only usage into final evidence.
    /// 验证成功请求不能把仅有快照的用量提升为终态证据。
    #[test]
    fn usage_availability_requires_explicit_final_evidence() {
        let mut usage = ProviderRequestCompletedUsage {
            update_kind: Some(vulcan_ai_contract::UsageUpdateKind::Snapshot),
            input_tokens: Some(10),
            ..Default::default()
        };
        assert_eq!(usage_availability(Some(&usage)), "partial");
        usage.update_kind = Some(vulcan_ai_contract::UsageUpdateKind::Delta);
        assert_eq!(usage_availability(Some(&usage)), "partial");
        usage.update_kind = Some(vulcan_ai_contract::UsageUpdateKind::Final);
        assert_eq!(usage_availability(Some(&usage)), "final");
        assert_eq!(usage_availability(None), "missing");
    }

    /// Verifies live completion requires a retained start and conflicting duplicates fail closed.
    /// 验证实时完成事件必须已有保留的开始边界，且冲突重复事件会封闭失败。
    #[test]
    fn live_provider_boundaries_require_exact_idempotent_lifecycle_facts() {
        let session_id = SessionId::new_v4();
        let state = provider_usage_test_state(session_id);
        let started =
            provider_usage_test_event(ProviderRequestTelemetryEventKind::Started, session_id, None);
        let completed = provider_usage_test_event(
            ProviderRequestTelemetryEventKind::Completed,
            session_id,
            Some("succeeded"),
        );

        let missing_start_error = apply_provider_request_usage_telemetry(&state, &completed)
            .expect_err("completed event without started boundary must fail");
        assert!(missing_start_error.contains("without a durably retained Started boundary"));
        assert!(
            apply_provider_request_usage_telemetry(&state, &started)
                .expect("first started event should apply")
                .is_some()
        );
        assert!(
            apply_provider_request_usage_telemetry(&state, &started)
                .expect("exact repeated started event should be idempotent")
                .is_none()
        );

        let mut conflicting_started = started.clone();
        conflicting_started.payload["model_key"] = Value::String(String::from("model-2"));
        let started_error = apply_provider_request_usage_telemetry(&state, &conflicting_started)
            .expect_err("conflicting repeated started event must fail");
        assert!(started_error.contains("conflicting immutable facts"));

        assert!(
            apply_provider_request_usage_telemetry(&state, &completed)
                .expect("first completed event should apply")
                .is_some()
        );
        assert!(
            apply_provider_request_usage_telemetry(&state, &completed)
                .expect("exact repeated completed event should be idempotent")
                .is_none()
        );

        let conflicting_completed = provider_usage_test_event(
            ProviderRequestTelemetryEventKind::Completed,
            session_id,
            Some("failed"),
        );
        let completed_error =
            apply_provider_request_usage_telemetry(&state, &conflicting_completed)
                .expect_err("conflicting repeated completed event must fail");
        assert!(completed_error.contains("conflicting terminal facts"));
    }

    /// Verifies rollback removes a runtime entry created only for the failed boundary.
    /// 验证回滚会删除仅为失败边界临时创建的运行态条目。
    #[test]
    fn failed_boundary_removes_new_runtime_state_entry() {
        let session_id = SessionId::new_v4();
        let state = provider_usage_test_state(session_id);
        let rollback = capture_provider_usage_boundary_rollback(&state, session_id)
            .expect("empty runtime map should still produce rollback facts");
        assert!(!rollback.previous_runtime_state_existed);
        let started =
            provider_usage_test_event(ProviderRequestTelemetryEventKind::Started, session_id, None);
        let changed_snapshot = apply_provider_request_usage_telemetry(&state, &started)
            .expect("started event should create projected task state")
            .expect("started event should change the workspace");
        let attempted_ledger = changed_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("new task runtime state should exist")
            .provider_call_ledger
            .clone();

        restore_provider_usage_boundary_after_persist_failure(
            &state,
            session_id,
            &attempted_ledger,
            rollback,
        )
        .expect("failed boundary should remove its temporary runtime state");
        let guard = state.lock().expect("test service state should lock");
        assert!(
            !guard
                .workspace_snapshot
                .as_ref()
                .expect("test workspace should remain installed")
                .session_runtime_state
                .contains_key(&session_id)
        );
    }

    /// Verifies a failed durable write rolls back only the exact attempted ledger revision.
    /// 验证可靠写入失败只会回滚精确的尝试账本修订。
    #[test]
    fn failed_durable_boundary_rollback_is_exact_and_fail_closed() {
        let previous_ledger = SessionProviderCallLedger::default();
        let mut attempted_ledger = previous_ledger.clone();
        assert!(attempted_ledger.observe_started(
            vulcan_workbench_model::SessionProviderCallStarted {
                provider_call_id: String::from("rollback-call"),
                identity: vulcan_workbench_model::SessionProviderCallIdentity {
                    logical_request_id: String::from("rollback-logical"),
                    execution_id: String::from("rollback-execution"),
                    turn_id: Some(String::from("rollback-turn")),
                    attempt_index: 0,
                    retry_number: 0,
                    owner_kind: String::from("main_agent"),
                    owner_id: String::from("main-agent"),
                    owner_name_snapshot: String::from("Main Agent"),
                    operation: String::from("conversation"),
                    provider_id: String::from("provider"),
                    provider_name_snapshot: String::from("Provider"),
                    model_id: String::from("model"),
                    model_name_snapshot: String::from("Model"),
                    connection_id: String::from("connection"),
                    service_entry_id: String::from("public-api"),
                    protocol_profile_id: String::from("responses"),
                    manifest_revision: 1,
                    manifest_generation: 1,
                    binding_source: String::from("system_manifest"),
                    binding_rule: None,
                    route_revision: String::from("route"),
                    request_profiles: Vec::new(),
                    response_profiles: Vec::new(),
                },
                prepared_at_ms: 1,
                request_assembly_fingerprint: None,
                context_generation_id: None,
                context_start_record_id: None,
                model_route_fingerprint: None,
                toolset_fingerprint: None,
                system_prompt_fingerprint: None,
                request_text_measurement: None,
                budget_accounting: None,
            }
        ));
        let rollback = ProviderUsageBoundaryRollback {
            archived: false,
            previous_runtime_state_existed: true,
            previous_ledger: previous_ledger.clone(),
            previous_updated_at: String::from("before"),
        };
        let mut runtime_state = SessionRuntimeStateEntry {
            provider_call_ledger: attempted_ledger.clone(),
            updated_at: String::from("attempted"),
            ..SessionRuntimeStateEntry::default()
        };

        restore_provider_usage_runtime_state_after_persist_failure(
            &mut runtime_state,
            &attempted_ledger,
            rollback.clone(),
        )
        .expect("exact attempted ledger should roll back");
        assert_eq!(runtime_state.provider_call_ledger, previous_ledger);
        assert_eq!(runtime_state.updated_at, "before");

        let mut advanced_runtime_state = SessionRuntimeStateEntry {
            provider_call_ledger: attempted_ledger.clone(),
            ..SessionRuntimeStateEntry::default()
        };
        assert!(
            advanced_runtime_state
                .provider_call_ledger
                .observe_dispatched("rollback-call", 2)
        );
        let advanced_snapshot = advanced_runtime_state.provider_call_ledger.clone();
        let error = restore_provider_usage_runtime_state_after_persist_failure(
            &mut advanced_runtime_state,
            &attempted_ledger,
            rollback,
        )
        .expect_err("newer ledger state must not be overwritten");
        assert!(error.contains("advanced"));
        assert_eq!(
            advanced_runtime_state.provider_call_ledger,
            advanced_snapshot
        );
    }
}

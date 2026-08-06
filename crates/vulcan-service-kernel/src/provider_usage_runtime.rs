use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use time::OffsetDateTime;
use uuid::Uuid;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_provider_adapters::{AccountUsageQueryKind, PersistedAiConfiguration, PublicAiRuntime};
use vulcan_workspace_store::workspace_store::{
    ProviderAccountUsageSnapshotRecord, ProviderPricingProfileRecord,
    ProviderRateLimitSnapshotRecord, ProviderUsageAttemptRecord, ProviderUsageDashboard,
    load_provider_usage_dashboard, persist_provider_account_usage_snapshot,
    persist_provider_pricing_profile, persist_provider_rate_limit_snapshot,
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
        let endpoint_profile_revision = format!("sha256:{:x}", Sha256::digest(&revision_material));
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
        AccountUsageQueryKind::OpenRouterCredits | AccountUsageQueryKind::TavilyCredits => "quota",
        AccountUsageQueryKind::DeepSeekBalance => "balance",
        AccountUsageQueryKind::Unsupported => "unsupported",
    }
}

/// Returns the current Unix timestamp in milliseconds for native usage persistence.
/// 返回原生用量持久化使用的当前 Unix 毫秒时间戳。
fn current_provider_usage_unix_millis() -> i64 {
    i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
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
    let owner_kind = telemetry_string(event, "owner_kind")?;
    let changed_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let runtime_state = session_runtime_state_mut(workspace, event.session_id);
        let changed = match event.kind {
            ProviderRequestTelemetryEventKind::Started => {
                runtime_state.provider_usage.observe_started(
                    provider_call_id,
                    owner_kind,
                    telemetry_i64(event, "started_at_ms"),
                )
            }
            ProviderRequestTelemetryEventKind::Completed => {
                runtime_state.provider_usage.observe_completed(
                    provider_call_id,
                    owner_kind,
                    completed_segment_status(event)?,
                    provider_usage_snapshot_from_event(event),
                    telemetry_i64(event, "started_at_ms"),
                    telemetry_i64(event, "completed_at_ms"),
                    telemetry_optional_string(event, "error_category"),
                    telemetry_optional_string(event, "error_code"),
                )
            }
            ProviderRequestTelemetryEventKind::FirstToken
            | ProviderRequestTelemetryEventKind::Retrying => false,
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
pub(crate) fn process_provider_request_telemetry_event(
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
        ProviderRequestTelemetryEventKind::Started | ProviderRequestTelemetryEventKind::Completed
    );
    let accounting_result = if is_usage_boundary {
        // Keep accounting errors separate so the ordered Hook event is still delivered.
        // 将状态记账错误与 Hook 事件投递分离，确保有序 Hook 事件仍然发送。
        (|| {
            let changed_snapshot = apply_provider_request_usage_telemetry(state, &event)?;
            if let Some(snapshot) = changed_snapshot {
                spawn_workspace_session_persist_task(
                    state.clone(),
                    controller_runtime.clone(),
                    snapshot,
                    session_id,
                    Some(persist_reason.to_string()),
                );
            }
            if let Some(app) = app {
                emit_session_addons_changed_delta(app, state, session_id)?;
            }
            Ok(())
        })()
    } else {
        Ok(())
    };
    provider_telemetry_guard.forward(event);
    accounting_result
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
    if let Some(profile) = usage.and_then(|value| value.pricing_profile.as_ref()) {
        persist_provider_pricing_profile(
            controller_runtime,
            &ProviderPricingProfileRecord {
                pricing_revision: profile.pricing_revision.clone(),
                provider_id: profile.provider_id.clone(),
                service_entry_id: profile.service_entry_id.clone(),
                model_id: profile.model_id.clone(),
                currency: profile.currency.clone(),
                input_per_million: profile.input_per_million.clone(),
                output_per_million: profile.output_per_million.clone(),
                cache_read_per_million: profile.cache_read_per_million.clone(),
                cache_write_per_million: profile.cache_write_per_million.clone(),
                source_kind: profile.source_kind.clone(),
                source_id: profile.source_id.clone(),
                content_hash: profile.content_hash.clone(),
                created_at_ms: telemetry_required_i64(event, "completed_at_ms")?,
            },
        )
        .await?;
    }
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
        reported_cost: usage.and_then(|value| value.reported_cost.clone()),
        estimated_cost: usage.and_then(|value| value.estimated_cost.clone()),
        currency: usage.and_then(|value| value.cost_currency.clone()),
        pricing_revision: usage.and_then(|value| value.pricing_revision.clone()),
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

/// Converts the exact terminal payload status into the persisted segment state.
/// 将精确终态载荷状态转换为持久化分段状态。
fn completed_segment_status(
    event: &ProviderRequestTelemetryEvent,
) -> Result<SessionProviderUsageSegmentStatus, String> {
    match telemetry_string(event, "status")?.as_str() {
        "succeeded" => Ok(SessionProviderUsageSegmentStatus::Succeeded),
        "failed" | "cancelled" => Ok(SessionProviderUsageSegmentStatus::Failed),
        status => Err(format!(
            "unknown provider telemetry terminal status: {status}"
        )),
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
        reported_cost: usage.reported_cost.clone(),
        estimated_cost: usage.estimated_cost.clone(),
        cost_currency: usage.cost_currency.clone(),
        pricing_revision: usage.pricing_revision.clone(),
        pricing_profile: usage.pricing_profile.clone(),
        billing_details: usage.billing_details.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies telemetry usage conversion preserves separate cached and uncached inputs.
    /// 验证遥测用量转换会保留缓存与非缓存输入分项。
    #[test]
    fn telemetry_usage_snapshot_preserves_input_components() {
        let event = ProviderRequestTelemetryEvent {
            kind: ProviderRequestTelemetryEventKind::Completed,
            session_id: SessionId::new_v4(),
            payload: serde_json::json!({
                "request_id": "call-1",
                "owner_kind": "main_agent",
                "status": "succeeded",
            }),
            completed_usage: Some(
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
        };
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
}

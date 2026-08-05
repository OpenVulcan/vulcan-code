use super::*;

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

/// Converts the exact terminal payload status into the persisted segment state.
/// 将精确终态载荷状态转换为持久化分段状态。
fn completed_segment_status(
    event: &ProviderRequestTelemetryEvent,
) -> Result<SessionProviderUsageSegmentStatus, String> {
    match telemetry_string(event, "status")?.as_str() {
        "succeeded" => Ok(SessionProviderUsageSegmentStatus::Succeeded),
        "failed" => Ok(SessionProviderUsageSegmentStatus::Failed),
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
    let input_tokens = usage
        .uncached_input_tokens
        .zip(usage.cached_input_tokens)
        .and_then(|(uncached, cached)| uncached.checked_add(cached));
    Some(ProviderUsageSnapshot {
        input_tokens,
        uncached_input_tokens: usage.uncached_input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage.reasoning_tokens,
        total_tokens: usage.total_tokens,
        cache_creation_input_tokens: None,
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
                    output_tokens: Some(2),
                    reasoning_tokens: Some(1),
                    total_tokens: Some(12),
                },
            ),
        };
        let usage = provider_usage_snapshot_from_event(&event).expect("usage should exist");
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.uncached_input_tokens, Some(4));
        assert_eq!(usage.cached_input_tokens, Some(6));
        assert_eq!(usage.total_tokens, Some(12));
    }
}

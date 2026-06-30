//! Command session service handlers.
//! 命令会话服务处理器。

use super::*;

pub(super) async fn service_stop_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<Vec<CommandSessionSummary>, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?,
        None => agent_runtime
            .command_session_runtime
            .try_snapshot_session_by_id(command_session_id)
            .map(|snapshot| snapshot.owner_session_id)
            .ok_or_else(|| {
                format!("Command session `{command_session_id}` was not found in the live runtime.")
            })?,
    };
    webui_trace(
        "stop_command_session_requested",
        format!("owner={owner_session_id} command_session={command_session_id}"),
    );
    agent_runtime
        .command_session_runtime
        .stop_session(owner_session_id, command_session_id, Some(1500))
        .await?;
    emit_command_sessions_changed_delta(&app, &state, &controller_runtime, &agent_runtime)
}

/// Reads one live command-session snapshot without rebuilding the full WebUI workbench.
/// 读取单条实时命令会话快照，但不重建完整 WebUI 工作台。
pub(super) async fn service_get_command_session_snapshot(
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionSummary, String> {
    let (_, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let snapshot = agent_runtime
        .command_session_runtime
        .try_snapshot_session_by_id(command_session_id)
        .ok_or_else(|| {
            format!("Command session `{command_session_id}` was not found in the live runtime.")
        })?;
    if let Some(owner_session_id) = owner_session_id {
        let owner_session_id = owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?;
        if snapshot.owner_session_id != owner_session_id {
            return Err(format!(
                "Command session `{command_session_id}` does not belong to owner `{owner_session_id}`."
            ));
        }
    }
    Ok(command_session_summary_from_snapshot(snapshot))
}

/// Request body for sending structured input to one live command session.
/// 向单条实时命令会话发送结构化输入的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandSessionInputRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Ordered terminal input events.
    /// 保序终端输入事件。
    inputs: Vec<CommandSessionInputEvent>,
}

/// Request body for resizing one live command session.
/// 调整单条实时命令会话尺寸的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandSessionResizeRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Desired terminal row count.
    /// 期望终端行数。
    rows: u16,
    /// Desired terminal column count.
    /// 期望终端列数。
    cols: u16,
}

/// Resolves command-session ids and owner ids for service terminal actions.
/// 为服务终端动作解析命令会话标识与归属标识。
pub(super) fn resolve_service_command_session_ids(
    agent_runtime: &AgentRuntimeBridge,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<(SessionId, SessionId), String> {
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?,
        None => agent_runtime
            .command_session_runtime
            .try_snapshot_session_by_id(command_session_id)
            .map(|snapshot| snapshot.owner_session_id)
            .ok_or_else(|| {
                format!("Command session `{command_session_id}` was not found in the live runtime.")
            })?,
    };
    Ok((command_session_id, owner_session_id))
}

/// Sends structured input to one live command session and emits a command-session delta.
/// 向单条实时命令会话发送结构化输入并发送命令会话增量。
pub(super) async fn service_send_command_session_input(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionInputRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let (command_session_id, owner_session_id) = resolve_service_command_session_ids(
        &agent_runtime,
        request.command_session_id,
        request.owner_session_id,
    )?;
    agent_runtime
        .command_session_runtime
        .write_session_input_events(owner_session_id, command_session_id, request.inputs)
        .await?;
    emit_command_sessions_changed_delta(&app, &state, &controller_runtime, &agent_runtime)
}

/// Resizes one live command session and emits a command-session delta.
/// 调整单条实时命令会话大小并发送命令会话增量。
pub(super) async fn service_resize_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionResizeRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let (command_session_id, owner_session_id) = resolve_service_command_session_ids(
        &agent_runtime,
        request.command_session_id,
        request.owner_session_id,
    )?;
    agent_runtime
        .command_session_runtime
        .resize_session(
            owner_session_id,
            command_session_id,
            request.rows,
            request.cols,
        )
        .await?;
    emit_command_sessions_changed_delta(&app, &state, &controller_runtime, &agent_runtime)
}

/// Emits the current command-session summaries without rebuilding the full workbench snapshot.
/// 不重建完整工作台快照，发送当前命令会话摘要。
fn emit_command_sessions_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
) -> Result<Vec<CommandSessionSummary>, String> {
    let command_sessions = command_sessions(
        controller_runtime,
        Some(agent_runtime.command_session_runtime.as_ref()),
    );
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.command_sessions = command_sessions.clone();
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::CommandSessionsChanged(CommandSessionsChangedDelta {
                command_sessions: command_sessions.clone(),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_command_sessions_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(command_sessions)
}

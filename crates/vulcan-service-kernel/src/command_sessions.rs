//! Command session service handlers.
//! 命令会话服务处理器。

use super::*;

/// Runtime and identity context shared by command-session mutating actions.
/// 命令会话变更类动作共享的运行时与身份上下文。
struct CommandSessionActionContext {
    /// Controller runtime used to project command-session summaries.
    /// 用于投影命令会话摘要的控制器运行时。
    controller_runtime: ControllerRuntimeHandle,
    /// Agent runtime that owns the live command-session runtime.
    /// 持有实时命令会话运行时的 Agent runtime。
    agent_runtime: AgentRuntimeBridge,
    /// Parsed target command-session identifier.
    /// 已解析的目标命令会话标识。
    command_session_id: SessionId,
    /// Parsed owner conversation/session identifier.
    /// 已解析的归属会话标识。
    owner_session_id: SessionId,
}

pub(super) async fn service_stop_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context =
        load_service_command_session_action_context(&state, command_session_id, owner_session_id)?;
    webui_trace(
        "stop_command_session_requested",
        format!(
            "owner={} command_session={}",
            context.owner_session_id, context.command_session_id
        ),
    );
    context
        .agent_runtime
        .command_session_runtime
        .stop_session(
            context.owner_session_id,
            context.command_session_id,
            Some(1500),
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
}

/// Reads one live command-session snapshot without rebuilding the full WebUI workbench.
/// 读取单条实时命令会话快照，但不重建完整 WebUI 工作台。
pub(super) async fn service_get_command_session_snapshot(
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionSummary, String> {
    let (_, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = parse_service_command_session_id(command_session_id)?;
    let snapshot = agent_runtime
        .command_session_runtime
        .try_snapshot_session_by_id(command_session_id)
        .ok_or_else(|| {
            format!("Command session `{command_session_id}` was not found in the live runtime.")
        })?;
    if let Some(owner_session_id) = owner_session_id {
        let owner_session_id = parse_service_owner_session_id(owner_session_id)?;
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
    let command_session_id = parse_service_command_session_id(command_session_id)?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => parse_service_owner_session_id(owner_session_id)?,
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

/// Loads runtime handles and resolved ids for one command-session mutating action.
/// 为单个命令会话变更动作加载运行时句柄与已解析标识。
///
/// # Parameters
/// 参数
/// `state` owns the retained service runtime handles.
/// `state` 持有保留的 service 运行时句柄。
/// `command_session_id` is the raw command-session id from the WebUI request.
/// `command_session_id` 是 WebUI 请求中的原始命令会话标识。
/// `owner_session_id` optionally contains the raw owner session id from the WebUI request.
/// `owner_session_id` 可选包含 WebUI 请求中的原始归属会话标识。
///
/// # Returns
/// 返回值
/// A command-session action context ready for runtime mutation and delta emission.
/// 返回可用于运行时变更与增量发送的命令会话动作上下文。
fn load_service_command_session_action_context(
    state: &SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionActionContext, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(state)?;
    let (command_session_id, owner_session_id) =
        resolve_service_command_session_ids(&agent_runtime, command_session_id, owner_session_id)?;
    Ok(CommandSessionActionContext {
        controller_runtime,
        agent_runtime,
        command_session_id,
        owner_session_id,
    })
}

/// Parses one command-session id from a WebUI service request.
/// 从 WebUI 服务请求中解析单个命令会话标识。
///
/// # Parameters
/// 参数
/// `command_session_id` is the raw request identifier text.
/// `command_session_id` 是原始请求标识文本。
///
/// # Returns
/// 返回值
/// The parsed command-session identifier or a request-specific parse error.
/// 返回解析后的命令会话标识，或请求专属解析错误。
fn parse_service_command_session_id(command_session_id: String) -> Result<SessionId, String> {
    parse_service_session_id(command_session_id, "command session")
}

/// Parses one command-session owner id from a WebUI service request.
/// 从 WebUI 服务请求中解析单个命令会话归属标识。
///
/// # Parameters
/// 参数
/// `owner_session_id` is the raw optional owner identifier text.
/// `owner_session_id` 是原始可选归属标识文本。
///
/// # Returns
/// 返回值
/// The parsed owner-session identifier or a request-specific parse error.
/// 返回解析后的归属会话标识，或请求专属解析错误。
fn parse_service_owner_session_id(owner_session_id: String) -> Result<SessionId, String> {
    parse_service_session_id(owner_session_id, "owner session")
}

/// Parses one WebUI service session id with a caller-specific error label.
/// 使用调用方专属错误标签解析单个 WebUI 服务会话标识。
///
/// # Parameters
/// 参数
/// `raw_session_id` is the raw request identifier text.
/// `raw_session_id` 是原始请求标识文本。
/// `error_label` names the request field in parse errors.
/// `error_label` 在解析错误中标识请求字段。
///
/// # Returns
/// 返回值
/// The parsed session identifier or a labelled parse error.
/// 返回解析后的会话标识，或带标签的解析错误。
fn parse_service_session_id(
    raw_session_id: String,
    error_label: &'static str,
) -> Result<SessionId, String> {
    raw_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid {error_label} id '{raw_session_id}': {error}"))
}

/// Sends structured input to one live command session and emits a command-session delta.
/// 向单条实时命令会话发送结构化输入并发送命令会话增量。
pub(super) async fn service_send_command_session_input(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionInputRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context = load_service_command_session_action_context(
        &state,
        request.command_session_id,
        request.owner_session_id,
    )?;
    context
        .agent_runtime
        .command_session_runtime
        .write_session_input_events(
            context.owner_session_id,
            context.command_session_id,
            request.inputs,
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
}

/// Resizes one live command session and emits a command-session delta.
/// 调整单条实时命令会话大小并发送命令会话增量。
pub(super) async fn service_resize_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionResizeRequest,
) -> Result<Vec<CommandSessionSummary>, String> {
    let context = load_service_command_session_action_context(
        &state,
        request.command_session_id,
        request.owner_session_id,
    )?;
    context
        .agent_runtime
        .command_session_runtime
        .resize_session(
            context.owner_session_id,
            context.command_session_id,
            request.rows,
            request.cols,
        )
        .await?;
    emit_command_sessions_changed_delta(
        &app,
        &state,
        &context.controller_runtime,
        &context.agent_runtime,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies command-session id parse errors keep their request-specific label.
    /// 验证命令会话标识解析错误会保留请求专属标签。
    #[test]
    fn parse_service_command_session_id_reports_command_label() {
        let error = parse_service_command_session_id(String::from("not-a-session-id"))
            .expect_err("invalid command-session id should report a parse error");

        assert!(error.starts_with("invalid command session id 'not-a-session-id':"));
    }

    /// Verifies owner-session id parse errors keep their request-specific label.
    /// 验证归属会话标识解析错误会保留请求专属标签。
    #[test]
    fn parse_service_owner_session_id_reports_owner_label() {
        let error = parse_service_owner_session_id(String::from("not-a-session-id"))
            .expect_err("invalid owner-session id should report a parse error");

        assert!(error.starts_with("invalid owner session id 'not-a-session-id':"));
    }
}

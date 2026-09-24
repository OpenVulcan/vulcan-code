//! Runtime-event identity and wait-agent coordination helpers.
//! 运行时事件身份与 wait-agent 协调辅助函数。

use super::*;

/// Describes whether one wait-agent runtime event changed retained coordination state.
/// 描述单条 wait-agent 运行时事件是否修改了保留协调状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitAgentRuntimeStateCommit {
    /// The event is unrelated to wait-agent coordination.
    /// 该事件与 wait-agent 协调无关。
    Ignored,
    /// The exact active root task committed the coordination update.
    /// 精确活跃根任务已提交协调更新。
    Committed,
    /// A replacement root task owns the session, so the stale event was rejected.
    /// 替代根任务已持有会话，因此旧事件被拒绝。
    Superseded,
}

/// Updates retained wait-agent coordination state for one exact active root task.
/// 为单个精确活跃根任务更新保留的 wait-agent 协调状态。
///
/// # Parameters
/// # 参数
/// `state` owns active-stream and wait-agent coordination state.
/// `state` 持有活动流与 wait-agent 协调状态。
/// `session_id` identifies the root session receiving the runtime event.
/// `session_id` 标识接收运行时事件的根会话。
/// `active_stream_task_id` identifies the exact root task allowed to commit the update.
/// `active_stream_task_id` 标识获准提交更新的精确根任务。
/// `event` is the authoritative runtime tool lifecycle event emitted by that task.
/// `event` 是该任务发出的权威运行时工具生命周期事件。
///
/// # Returns
/// # 返回值
/// Returns an explicit commit outcome, or an error when shared state cannot be locked.
/// 返回明确的提交结果；共享状态无法加锁时返回错误。
pub(crate) fn update_webui_wait_agent_runtime_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    event: &RuntimeEvent,
) -> Result<WaitAgentRuntimeStateCommit, String> {
    let Some(tool_name) = runtime_event_tool_name(event) else {
        return Ok(WaitAgentRuntimeStateCommit::Ignored);
    };
    if tool_name != vulcan_agent_runtime::tooling::collaboration::WAIT_AGENTS_RESULT_TOOL_ID
        || !matches!(
            event.kind,
            RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
        )
    {
        return Ok(WaitAgentRuntimeStateCommit::Ignored);
    }

    // The guard keeps exact task ownership stable across every coordination mutation.
    // 该锁守卫保证精确任务所有权在全部协调变更期间保持稳定。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
        return Ok(WaitAgentRuntimeStateCommit::Superseded);
    }

    match event.kind {
        RuntimeEventKind::ToolStarted => {
            let scope_id = wait_agent_scope_id(event);
            let kernel = session_runtime_kernel_mut(&mut guard, session_id);
            let removed_stale_wait_scopes = kernel.clear_wait_scopes();
            if removed_stale_wait_scopes > 0 {
                webui_trace(
                    "wait_agent_stale_scopes_cleared",
                    format!("session={session_id} removed_wait_scopes={removed_stale_wait_scopes}"),
                );
            }
            // Clearing prior scopes guarantees the replacement identifier is unique.
            // 清空旧范围可保证替代标识唯一。
            kernel
                .register_wait_scope(scope_id, Vec::<String>::new(), current_unix_millis_u64())
                .expect("cleared wait scopes guarantee a unique replacement scope identifier");
            guard
                .parent_sessions_waiting_for_subagents
                .insert(session_id);
            sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
        }
        RuntimeEventKind::ToolFinished => {
            guard
                .parent_sessions_waiting_for_subagents
                .remove(&session_id);
            if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
                let scope_id = wait_agent_scope_id(event);
                let _ = kernel.clear_wait_scope(&scope_id);
            }
            sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
        }
        _ => unreachable!("wait-agent event kind was validated before acquiring state"),
    }
    Ok(WaitAgentRuntimeStateCommit::Committed)
}

/// Returns the stable wait-scope identifier for one wait-agent runtime event.
/// 返回单条 wait-agent 运行时事件对应的稳定等待范围标识。
///
/// # Parameters
/// # 参数
/// `event` is the runtime event whose local tool-call identity owns the scope.
/// `event` 是其本地工具调用身份持有等待范围的运行时事件。
///
/// # Returns
/// # 返回值
/// Returns the tool-call-derived scope id, or the stable current-call fallback.
/// 返回由工具调用派生的范围标识，缺失身份时返回稳定的当前调用标识。
fn wait_agent_scope_id(event: &RuntimeEvent) -> String {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_call_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|tool_call_id| format!("wait-agent-{tool_call_id}"))
        .unwrap_or_else(|| String::from("wait-agent-current"))
}

/// Extracts the canonical tool name carried by one runtime event metadata payload.
/// 提取单条运行时事件元数据中携带的规范工具名称。
///
/// # Parameters
/// # 参数
/// `event` is the runtime event carrying optional structured metadata.
/// `event` 是携带可选结构化元数据的运行时事件。
///
/// # Returns
/// # 返回值
/// Returns a non-empty canonical tool name when metadata contains one.
/// 元数据包含非空规范工具名称时返回该名称。
pub(crate) fn runtime_event_tool_name(event: &RuntimeEvent) -> Option<&str> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

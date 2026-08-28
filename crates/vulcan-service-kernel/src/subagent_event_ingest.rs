//! subagent event ingest helpers.
//! subagent event ingest 辅助逻辑。

use super::*;

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
pub(crate) fn ingest_webui_subagent_runtime_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    ingest_webui_subagent_runtime_event_for_listener_execution(
        app,
        state,
        _agent_runtime,
        parent_session_id,
        event,
        None,
        true,
    )
}

/// Consumes one delegate event only while the optional detached listener still owns its execution.
/// 仅当可选分离监听器仍拥有其执行时吸收委派事件。
pub(crate) fn ingest_webui_subagent_runtime_event_for_listener_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
    listener_execution_id: Option<&str>,
    _project_parent_feedback: bool,
) -> Result<bool, String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished
    ) {
        return Ok(false);
    }
    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(true);
    };
    subagent_feedback_execution_id_from_tool_metadata(metadata)?;
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(true);
    };
    webui_trace(
        "subagent_runtime_event_ingest",
        format!(
            "parent={parent_session_id} task={} kind={:?} is_finished={is_finished} public_task_id={} label={}",
            panel_state.task_id, event.kind, panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    // The retained transaction owns only subagent lifecycle and navigation state.
    // 保留事务仅拥有子代理生命周期与导航状态。
    let normalized_state = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if listener_execution_id.is_some_and(|execution_id| {
            guard
                .detached_subagent_task_listeners
                .get(&panel_state.task_id)
                .is_none_or(|current_execution_id| current_execution_id != execution_id)
        }) {
            return Ok(true);
        }
        // Navigation projection requires an initialized workspace; reject before lifecycle state changes.
        // 导航投影要求工作区已经初始化；必须在生命周期状态发生变化前拒绝请求。
        if guard.workspace_snapshot.is_none() {
            return Err(String::from("workspace snapshot is not ready"));
        }
        // normalizedState remains transaction-local until workspace preflight and projection succeed.
        // normalizedState 在工作区预检与投影成功前始终保持为事务局部状态。
        let normalized_state = if is_finished {
            terminal_webui_subagent_task_state(panel_state)
        } else {
            panel_state
        };
        webui_trace(
            "subagent_runtime_event_normalized",
            format!(
                "parent={parent_session_id} task={} run_state={} has_conversation_state={} result_chars={} terminal={is_finished}",
                normalized_state.task_id,
                normalized_state.run_state.metadata_key(),
                normalized_state.conversation_state.is_some(),
                normalized_state.result_text.chars().count()
            ),
        );
        // Shared navigation and listener ownership are committed only after every fallible projection step has succeeded.
        // 共享导航与监听器所有权仅在全部可失败投影步骤成功后提交。
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        if is_finished && listener_execution_id.is_none() {
            guard
                .detached_subagent_task_listeners
                .remove(&normalized_state.task_id);
        }
        let cache_access_ms = current_unix_millis_value();
        guard.subagent_surface_detail_cache.insert(
            normalized_state.task_id,
            WebuiSubagentSurfaceDetailCacheEntry {
                task_state: normalized_state.clone(),
                last_access_ms: cache_access_ms,
            },
        );
        prune_webui_subagent_surface_detail_cache(&mut guard, cache_access_ms);
        normalized_state
    };
    emit_webui_subagent_navigation_changed_delta_for_parent(app, state, parent_session_id)?;
    if is_finished {
        webui_trace(
            "subagent_runtime_event_projection_finished",
            format!(
                "parent={parent_session_id} task={}",
                normalized_state.task_id
            ),
        );
    }
    Ok(true)
}

/// Converts one delegate-finished panel payload into an authoritative terminal state.
/// 将单条 delegate-finished 面板载荷转换为权威终态状态。
fn terminal_webui_subagent_task_state(mut state: SubagentTaskPanelState) -> SubagentTaskPanelState {
    if state.run_state.is_running() {
        state.run_state = SubagentTaskRunState::Completed;
    }
    state
}

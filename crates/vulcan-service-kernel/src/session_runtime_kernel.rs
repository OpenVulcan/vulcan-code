//! WebUI session runtime kernel helpers.
//! WebUI 会话运行时内核辅助函数。

use super::*;

/// Converts the local millisecond clock into the session-core timestamp domain.
/// 将本地毫秒时钟转换到 session-core 使用的时间戳域。
///
/// # Returns
/// 返回值
/// Current Unix milliseconds as an unsigned value.
/// 当前 Unix 毫秒无符号值。
pub(crate) fn current_unix_millis_u64() -> u64 {
    u64::try_from(current_unix_millis_value()).unwrap_or_default()
}

/// Returns the runtime kernel for one WebUI session, creating it on first use.
/// 返回单个 WebUI 会话的运行时内核，并在首次使用时创建。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the root WebUI session.
/// `session_id` 标识 WebUI 根会话。
///
/// # Returns
/// 返回值
/// Mutable access to the session runtime kernel.
/// 对会话运行时内核的可变访问。
pub(crate) fn session_runtime_kernel_mut(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> &mut SessionRuntimeKernel {
    state
        .session_runtime_kernels
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeKernel::new(SessionNodeId::from(session_id)))
}

/// Captures the parent session turn binding for a detached WebUI subagent task.
/// 为一个 WebUI 分离子代理任务捕获父会话回合绑定。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and subagent bindings.
/// `state` 持有运行时内核与子代理绑定。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the detached child task.
/// `task_id` 标识分离子任务。
pub(crate) fn bind_webui_subagent_parent_turn(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
) {
    let (source_generation, parent_turn_id) = {
        let kernel = session_runtime_kernel_mut(state, parent_session_id);
        (
            kernel.generation(),
            kernel.active_turn().map(|turn| turn.turn_id.clone()),
        )
    };
    state
        .subagent_parent_turn_bindings
        .entry(task_id)
        .or_insert_with(|| WebuiSubagentParentTurnBinding {
            parent_session_id,
            source_generation,
            parent_turn_id: parent_turn_id.clone(),
        });
    webui_trace(
        "subagent_parent_turn_bound",
        format!(
            "parent={parent_session_id} task={task_id} generation={source_generation} parent_turn_id={}",
            parent_turn_id.as_deref().unwrap_or("<none>")
        ),
    );
}

/// Clears the captured parent-turn binding for a detached WebUI subagent task.
/// 清理一个 WebUI 分离子代理任务捕获的父回合绑定。
///
/// # Parameters
/// 参数
/// `state` owns subagent parent-turn bindings.
/// `state` 持有子代理父回合绑定。
/// `task_id` identifies the detached child task.
/// `task_id` 标识分离子任务。
pub(crate) fn clear_webui_subagent_parent_turn_binding(
    state: &mut ServiceBridgeState,
    task_id: SessionId,
) {
    if let Some(binding) = state.subagent_parent_turn_bindings.remove(&task_id) {
        webui_trace(
            "subagent_parent_turn_binding_cleared",
            format!(
                "parent={} task={task_id} generation={}",
                binding.parent_session_id, binding.source_generation
            ),
        );
    }
}

/// Starts one WebUI user-prompt turn in the session runtime kernel.
/// 在会话运行时内核中启动一个 WebUI 用户提示词回合。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
///
/// # Returns
/// 返回值
/// `Ok(())` when the turn is accepted; otherwise a single-active-turn error.
/// 回合被接受时返回 `Ok(())`，否则返回单活动回合错误。
pub(crate) fn start_webui_session_kernel_user_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> Result<(), String> {
    start_webui_session_kernel_turn(state, session_id, turn_id, ActiveTurnKind::UserPrompt)
}

/// Starts one WebUI turn in the session runtime kernel.
/// 在会话运行时内核中启动一个 WebUI 回合。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
/// `kind` is the runtime kernel turn kind.
/// `kind` 是运行时内核回合类型。
///
/// # Returns
/// 返回值
/// `Ok(())` when the turn is accepted; otherwise a single-active-turn error.
/// 回合被接受时返回 `Ok(())`，否则返回单活动回合错误。
pub(crate) fn start_webui_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
    kind: ActiveTurnKind,
) -> Result<(), String> {
    if kind == ActiveTurnKind::UserPrompt {
        clear_webui_session_wait_agent_coordination(state, session_id, "new_user_turn");
        if let Some(agent_runtime) = state.agent_runtime.as_ref() {
            agent_runtime
                .task_runtime
                .notify_parent_input_activity(SessionNodeId::from(session_id));
        }
    }
    let kernel = session_runtime_kernel_mut(state, session_id);
    let turn = kernel
        .start_turn(turn_id.to_string(), kind, current_unix_millis_u64())
        .map_err(|error| {
            format!(
                "会话 `{session_id}` 无法启动运行内核回合：{}",
                error.message
            )
        })?;
    webui_trace(
        "session_kernel_turn_started",
        format!(
            "session={session_id} turn_id={} generation={}",
            turn.turn_id, turn.generation
        ),
    );
    Ok(())
}

/// Finishes one WebUI session kernel turn if it is still active.
/// 如果 WebUI 会话内核回合仍处于活动状态则结束它。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `status` is the terminal status to record for diagnostics.
/// `status` 是用于诊断记录的终态状态。
pub(crate) fn finish_webui_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    status: TurnTerminalStatus,
) {
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return;
    };
    let finished = kernel.finish_turn(status, current_unix_millis_u64());
    if let Some(turn) = finished {
        webui_trace(
            "session_kernel_turn_finished",
            format!(
                "session={session_id} turn_id={} generation={} status={status:?}",
                turn.turn_id, turn.generation
            ),
        );
    }
}

/// Stops the current WebUI session kernel generation if one exists.
/// 如果当前 WebUI 会话内核代际存在则停止它。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when an active kernel turn was stopped.
/// 停止了活动内核回合时返回 `true`。
pub(crate) fn stop_webui_session_kernel_generation(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return false;
    };
    let stopped = kernel.stop_current_generation(current_unix_millis_u64());
    if let Some(turn) = stopped {
        webui_trace(
            "session_kernel_generation_stopped",
            format!(
                "session={session_id} turn_id={} generation={}",
                turn.turn_id, turn.generation
            ),
        );
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation");
        if let Some(agent_runtime) = state.agent_runtime.as_ref() {
            agent_runtime
                .task_runtime
                .notify_parent_input_activity(SessionNodeId::from(session_id));
        }
        true
    } else {
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation_idle");
        if let Some(agent_runtime) = state.agent_runtime.as_ref() {
            agent_runtime
                .task_runtime
                .notify_parent_input_activity(SessionNodeId::from(session_id));
        }
        false
    }
}

/// Clears wait-agent coordination state that must not survive into a new parent turn.
/// 清理不应跨入新父级回合的 wait_agent 协调状态。
pub(crate) fn clear_webui_session_wait_agent_coordination(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    reason: &str,
) {
    let removed_waiting = state
        .parent_sessions_waiting_for_subagents
        .remove(&session_id);
    let removed_wait_scopes = state
        .session_runtime_kernels
        .get_mut(&session_id)
        .map(SessionRuntimeKernel::clear_wait_scopes)
        .unwrap_or(0);
    if removed_waiting || removed_wait_scopes > 0 {
        webui_trace(
            "wait_agent_coordination_cleared",
            format!(
                "session={session_id} reason={reason} removed_waiting={removed_waiting} removed_wait_scopes={removed_wait_scopes}"
            ),
        );
    }
}

/// Returns whether a WebUI session has an active kernel turn.
/// 返回 WebUI 会话是否存在活动的内核回合。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when the session kernel currently owns an active turn.
/// 当会话内核当前拥有活动回合时返回 `true`。
pub(crate) fn webui_session_kernel_has_active_turn(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    state
        .session_runtime_kernels
        .get(&session_id)
        .is_some_and(SessionRuntimeKernel::has_active_turn)
}

/// Returns whether a WebUI session should be treated as actively running.
/// 返回 WebUI 会话是否应被视为正在运行。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns stream handles and runtime kernels.
/// `state` 是持有流任务句柄与运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when the kernel has an active turn.
/// 当内核存在活动回合时返回 `true`。
pub(crate) fn webui_session_has_active_run(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    state
        .session_runtime_kernels
        .get(&session_id)
        .is_some_and(SessionRuntimeKernel::has_active_turn)
}

/// Reads the current kernel generation for one WebUI session.
/// 读取单个 WebUI 会话当前的内核代际。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// The current generation when a kernel exists.
/// 当内核存在时返回当前代际。
pub(crate) fn webui_session_kernel_generation(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> Option<u64> {
    state
        .session_runtime_kernels
        .get(&session_id)
        .map(SessionRuntimeKernel::generation)
}

/// Returns the session ids whose runtime kernels currently own active turns.
/// 返回当前运行时内核拥有活动回合的会话标识集合。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
///
/// # Returns
/// 返回值
/// Session ids with active kernel turns.
/// 存在活动内核回合的会话标识。
pub(crate) fn active_kernel_session_ids(state: &ServiceBridgeState) -> BTreeSet<SessionId> {
    state
        .session_runtime_kernels
        .iter()
        .filter_map(|(session_id, kernel)| kernel.has_active_turn().then_some(*session_id))
        .collect()
}

/// Returns whether one WebUI session is idle enough for destructive user-prompt editing.
/// 返回单个 WebUI 会话是否空闲到足以执行破坏性用户提示词编辑。
pub(crate) fn webui_session_allows_user_prompt_editing(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    if state.active_stream_tasks.contains_key(&session_id)
        || webui_session_kernel_has_active_turn(state, session_id)
        || webui_session_has_active_run(state, session_id)
    {
        return false;
    }
    running_webui_subagent_task_states_for_tree_from_live_cache(state, session_id).is_empty()
}

/// Returns whether one workspace surface is visibly idle for user-prompt editing.
/// 返回单个工作区表面从可见状态看是否空闲到允许用户提示词编辑。
pub(crate) fn workspace_surface_allows_user_prompt_editing(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    subagent_navigation: Option<&SubagentNavigationSnapshot>,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    if workspace_snapshot
        .sessions
        .iter()
        .chain(workspace_snapshot.archived_sessions.iter())
        .find(|session| session.id == session_id)
        .is_none_or(|session| session.status == SessionState::Running)
    {
        return false;
    }
    if workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Preparing)
    {
        return false;
    }
    if workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(|runtime_state| {
            runtime_state_has_queued_pending_prompt_work(runtime_state)
                || runtime_state
                    .active_precheck_turn_id
                    .as_deref()
                    .is_some_and(|turn_id| !turn_id.trim().is_empty())
        })
    {
        return false;
    }
    !subagent_navigation
        .is_some_and(|navigation| navigation.tasks.iter().any(|task| task.is_running))
}

/// Returns the owning session id for one execution stream event.
/// 返回单个执行流事件所属的会话标识。
///
/// # Parameters
/// 参数
/// `event` is the execution stream event being routed.
/// `event` 是正在路由的执行流事件。
///
/// # Returns
/// 返回值
/// The WebUI session id carried by the event.
/// 事件携带的 WebUI 会话标识。
pub(crate) fn execution_stream_event_session_id(event: &ExecutionStreamEvent) -> SessionId {
    match event {
        ExecutionStreamEvent::Prepared(preparation) => preparation.session_id,
        ExecutionStreamEvent::ProviderRequestCheckpoint { session_id, .. }
        | ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id }
        | ExecutionStreamEvent::ProviderLoweringRepair { session_id, .. }
        | ExecutionStreamEvent::RequestStarted { session_id }
        | ExecutionStreamEvent::ThinkingDelta { session_id, .. }
        | ExecutionStreamEvent::ThinkingCompleted { session_id }
        | ExecutionStreamEvent::OutputDelta { session_id, .. }
        | ExecutionStreamEvent::RuntimeEvent { session_id, .. }
        | ExecutionStreamEvent::ConversationCheckpoint { session_id, .. } => *session_id,
        ExecutionStreamEvent::Finished(report) => report.session_id,
    }
}

/// Returns whether one stream event still belongs to the current kernel generation.
/// 返回单个流事件是否仍属于当前内核代际。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns runtime kernels.
/// `state` 是持有运行时内核的共享服务状态。
/// `event` is the stream event about to be applied.
/// `event` 是即将应用的流事件。
/// `expected_generation` is the generation captured when the stream task started.
/// `expected_generation` 是流任务启动时捕获的代际。
///
/// # Returns
/// 返回值
/// `true` when the event is still current, or no kernel generation was captured.
/// 当事件仍属于当前代际或没有捕获内核代际时返回 `true`。
pub(crate) fn webui_stream_event_matches_kernel_generation(
    state: &SharedServiceBridgeState,
    event: &ExecutionStreamEvent,
    expected_generation: Option<u64>,
) -> bool {
    let Some(expected_generation) = expected_generation else {
        return true;
    };
    let session_id = execution_stream_event_session_id(event);
    let Ok(guard) = state.lock() else {
        return false;
    };
    let Some(kernel) = guard.session_runtime_kernels.get(&session_id) else {
        return true;
    };
    kernel.generation() == expected_generation && !kernel.is_stopped()
}

/// Enqueues one subagent completion event into the parent session kernel mailbox.
/// 将一条子代理完成事件加入父会话内核 mailbox。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the completed child task session.
/// `task_id` 标识已完成的子任务会话。
/// `metadata` carries the structured child-agent event metadata.
/// `metadata` 携带结构化子代理事件元数据。
///
/// # Returns
/// 返回值
/// `true` when the mailbox accepted the event.
/// mailbox 接受事件时返回 `true`。
pub(crate) fn enqueue_webui_subagent_mailbox_input(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    metadata: &Value,
) -> bool {
    let timestamp_ms = current_unix_millis_u64();
    let binding = state.subagent_parent_turn_bindings.get(&task_id).cloned();
    if let Some(binding) = binding.as_ref()
        && binding.parent_session_id != parent_session_id
    {
        webui_trace(
            "subagent_mailbox_input_rejected_parent_mismatch",
            format!(
                "parent={parent_session_id} task={task_id} binding_parent={}",
                binding.parent_session_id
            ),
        );
        return false;
    }
    let kernel = session_runtime_kernel_mut(state, parent_session_id);
    let source_generation = binding
        .as_ref()
        .map(|binding| binding.source_generation)
        .unwrap_or_else(|| kernel.generation());
    let parent_turn_id = binding
        .and_then(|binding| binding.parent_turn_id)
        .or_else(|| kernel.active_turn().map(|turn| turn.turn_id.clone()));
    let mailbox_event = metadata
        .get("payload")
        .and_then(|payload| payload.get("subagent_mailbox_event"))
        .cloned()
        .unwrap_or_else(|| metadata.clone());
    let parent_turn_id_for_payload = parent_turn_id.clone();
    let mut input = TurnInput::new(
        format!("subagent-mailbox-{parent_session_id}-{task_id}"),
        SessionNodeId::from(parent_session_id),
        TurnInputKind::SubagentMessage,
        true,
        timestamp_ms,
        json!({
            "task_id": task_id.to_string(),
            "parent_session_id": parent_session_id.to_string(),
            "source_generation": source_generation,
            "parent_turn_id": parent_turn_id_for_payload,
            "mailbox_event": mailbox_event,
        }),
    );
    if let Some(parent_turn_id) = parent_turn_id.clone() {
        input = input.with_parent_turn_id(parent_turn_id);
    }
    let accepted = kernel.enqueue_mailbox_input_for_generation(input, source_generation);
    webui_trace(
        "subagent_mailbox_input_enqueued",
        format!(
            "parent={parent_session_id} task={task_id} accepted={accepted} generation={source_generation}"
        ),
    );
    accepted
}

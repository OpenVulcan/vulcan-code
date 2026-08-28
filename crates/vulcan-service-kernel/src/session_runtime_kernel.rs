//! WebUI session runtime kernel helpers.
//! WebUI 会话运行时内核辅助函数。

use super::*;
use std::collections::btree_map::Entry;

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
    let restored_kernel = (!state.session_runtime_kernels.contains_key(&session_id))
        .then(|| restored_webui_session_runtime_kernel(state, session_id));
    match state.session_runtime_kernels.entry(session_id) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let Some(restored_kernel) = restored_kernel else {
                webui_trace(
                    "session_kernel_entry_invariant_repaired",
                    format!("session={session_id} missing live kernel after preflight hit"),
                );
                return entry.insert(SessionRuntimeKernel::new(SessionNodeId::from(session_id)));
            };
            entry.insert(restored_kernel)
        }
    }
}

/// Restores one session runtime kernel from persisted runtime state or creates an empty kernel.
/// 从持久化运行态恢复单个会话运行时内核，缺失时创建空内核。
fn restored_webui_session_runtime_kernel(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> SessionRuntimeKernel {
    let restored = state
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
        })
        .and_then(|runtime_state| runtime_state.session_runtime_kernel.clone());
    let Some(mut kernel) = restored else {
        return SessionRuntimeKernel::new(SessionNodeId::from(session_id));
    };
    let cleared_active_turn = kernel.restore_idle_after_process_restart(current_unix_millis_u64());
    webui_trace(
        "session_kernel_restored",
        format!(
            "session={session_id} cleared_active_turn={cleared_active_turn} mailbox_inputs={} consumed_inputs={}",
            kernel.input_queue().mailbox_len(),
            kernel.input_queue().consumed_input_count()
        ),
    );
    kernel
}

/// Mirrors the in-memory runtime kernel into the retained workspace runtime-state document.
/// 将内存运行时内核镜像到保留工作区运行态文档中。
pub(crate) fn sync_webui_session_runtime_kernel_to_workspace_state(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) {
    let Some(kernel) = state.session_runtime_kernels.get(&session_id).cloned() else {
        return;
    };
    let Some(workspace) = state.workspace_snapshot.as_mut() else {
        return;
    };
    let now = current_unix_millis_text();
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.session_runtime_kernel = Some(kernel);
    runtime_state.updated_at = now;
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
    start_webui_session_kernel_turn_with_admission(
        state,
        session_id,
        turn_id,
        ActiveTurnKind::UserPrompt,
        None,
    )
}

/// Starts one reserved WebUI user-prompt turn using exact submission admission ownership.
/// 使用精确提交准入所有权启动一个已预留的 WebUI 用户提示词回合。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and the authoritative reservation row.
/// `state` 持有运行时 kernel 与权威预留行。
/// `session_id` identifies the reserved existing session.
/// `session_id` 标识已预留的已有会话。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
/// `reservation_id` is the exact submission owner allowed through the admission gate.
/// `reservation_id` 是允许通过准入闸门的精确提交所有者。
///
/// # Returns
/// 返回值
/// `Ok(())` only when the reservation is current and the session kernel is idle.
/// 仅当预留仍为当前所有者且会话 kernel 空闲时返回 `Ok(())`。
pub(crate) fn start_reserved_webui_session_kernel_user_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
    reservation_id: Uuid,
) -> Result<(), String> {
    start_reserved_webui_session_kernel_turn(
        state,
        session_id,
        turn_id,
        ActiveTurnKind::UserPrompt,
        reservation_id,
    )
}

/// Starts one reserved WebUI kernel Turn with its exact semantic kind.
/// 使用精确语义类型启动一个已预留的 WebUI kernel 回合。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and reservations; `session_id`, `turn_id`, and `kind` identify
/// the exact runtime Turn; `reservation_id` proves the current PromptSubmission owner.
/// `state` 持有运行时 kernel 与预留；`session_id`、`turn_id` 与 `kind` 标识精确运行回合；
/// `reservation_id` 证明当前 PromptSubmission 所有者。
///
/// # Returns
/// 返回值
/// Starts the requested Turn only while the supplied reservation remains current.
/// 仅在所提供预留仍为当前所有者时启动请求的回合。
pub(crate) fn start_reserved_webui_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
    kind: ActiveTurnKind,
    reservation_id: Uuid,
) -> Result<(), String> {
    start_webui_session_kernel_turn_with_admission(
        state,
        session_id,
        turn_id,
        kind,
        Some(reservation_id),
    )
}

/// Returns whether one parent session is actively waiting inside wait_agent.
/// 返回一个父会话是否正处于 wait_agent 主动等待状态。
///
/// # Parameters
/// 参数
/// `state` owns WebUI runtime coordination state.
/// `state` 持有 WebUI 运行时协调状态。
/// `session_id` identifies the parent WebUI session.
/// `session_id` 标识父级 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` only when a wait_agent invocation is currently registered as waiting.
/// 仅当当前已注册 wait_agent 等待调用时返回 `true`。
fn webui_session_wait_agent_is_waiting(state: &ServiceBridgeState, session_id: SessionId) -> bool {
    state
        .parent_sessions_waiting_for_subagents
        .contains(&session_id)
        || state
            .session_runtime_kernels
            .get(&session_id)
            .is_some_and(SessionRuntimeKernel::has_wait_scopes)
}

/// Notifies an active wait_agent invocation about a real parent-side interruption.
/// 向活跃 wait_agent 调用通知真实的父级插队或停止事件。
///
/// # Parameters
/// 参数
/// `state` owns WebUI runtime coordination state and the agent runtime bridge.
/// `state` 持有 WebUI 运行时协调状态与智能体运行时桥。
/// `session_id` identifies the parent WebUI session.
/// `session_id` 标识父级 WebUI 会话。
/// `reason` describes the interruption source for diagnostics.
/// `reason` 描述用于诊断的打断来源。
///
/// # Returns
/// 返回值
/// Returns `Ok(true)` when a current wait_agent invocation was notified, `Ok(false)` when no
/// notification target exists, or an Agent Runtime diagnostic when the interruption fact fails.
/// 当前 wait_agent 调用已通知时返回 `Ok(true)`，不存在通知目标时返回 `Ok(false)`，中断事实写入失败时返回智能体运行时诊断。
pub(crate) fn notify_webui_wait_agent_parent_interruption_if_waiting(
    state: &ServiceBridgeState,
    session_id: SessionId,
    reason: &str,
) -> Result<bool, String> {
    if !webui_session_wait_agent_is_waiting(state, session_id) {
        return Ok(false);
    }
    if let Some(agent_runtime) = state.agent_runtime.as_ref() {
        agent_runtime
            .task_runtime
            .notify_wait_agent_parent_interruption(SessionNodeId::from(session_id))?;
        webui_trace(
            "wait_agent_parent_interruption_notified",
            format!("session={session_id} reason={reason}"),
        );
        return Ok(true);
    }
    Ok(false)
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
    start_webui_session_kernel_turn_with_admission(state, session_id, turn_id, kind, None)
}

/// Starts one WebUI turn after validating optional exact admission ownership.
/// 校验可选精确准入所有权后启动一个 WebUI 回合。
///
/// # Parameters
/// 参数
/// `state` owns reservation rows, wait coordination, and runtime kernels.
/// `state` 持有预留行、等待协调状态与运行时 kernel。
/// `session_id` identifies the session whose turn should start.
/// `session_id` 标识需要启动回合的会话。
/// `turn_id` is the unique logical turn identifier.
/// `turn_id` 是唯一逻辑回合标识。
/// `kind` classifies the runtime turn semantics.
/// `kind` 对运行时回合语义进行分类。
/// `allowed_reservation_id` optionally names the exact PromptSubmission owner allowed through the gate.
/// `allowed_reservation_id` 可选指定允许通过闸门的精确 PromptSubmission 所有者。
///
/// # Returns
/// 返回值
/// The started idle kernel turn, or an ownership, reservation, or active-turn diagnostic.
/// 返回已启动的空闲 kernel 回合，或所有权、预留、活动回合诊断。
fn start_webui_session_kernel_turn_with_admission(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
    kind: ActiveTurnKind,
    allowed_reservation_id: Option<Uuid>,
) -> Result<(), String> {
    if state
        .session_terminal_persistence_owners
        .contains_key(&session_id)
    {
        return Err(format!(
            "Session `{session_id}` is committing its terminal state; a new runtime kernel turn cannot start"
        ));
    }
    match (
        state.session_turn_admission_reservations.get(&session_id),
        allowed_reservation_id,
    ) {
        (Some(reservation), Some(allowed_reservation_id))
            if reservation.reservation_id == allowed_reservation_id
                && reservation.kind == SessionTurnAdmissionReservationKind::PromptSubmission => {}
        (Some(reservation), _) => {
            return Err(format!(
                "Session `{session_id}` is reserved by {}; a new runtime kernel turn cannot start",
                reservation.kind.diagnostic_label()
            ));
        }
        (None, Some(_)) => {
            return Err(format!(
                "Submission admission for session `{session_id}` is no longer current"
            ));
        }
        (None, None) => {}
    }
    if kind == ActiveTurnKind::UserPrompt {
        clear_webui_session_wait_agent_coordination(state, session_id, "new_user_turn");
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
    sync_webui_session_runtime_kernel_to_workspace_state(state, session_id);
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
    sync_webui_session_runtime_kernel_to_workspace_state(state, session_id);
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
    let notify_wait_agent = webui_session_wait_agent_is_waiting(state, session_id);
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
        if notify_wait_agent {
            // notificationError preserves stop as the primary operation while exposing signal failure.
            // notificationError 保持停止为主操作，同时显式暴露信号失败。
            if let Err(notification_error) = notify_webui_wait_agent_parent_interruption_if_waiting(
                state,
                session_id,
                "stop_generation",
            ) {
                eprintln!(
                    "[webui-service] failed to notify wait_agent parent interruption for session {session_id} reason=stop_generation: {notification_error}"
                );
            }
        }
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation");
        sync_webui_session_runtime_kernel_to_workspace_state(state, session_id);
        true
    } else {
        if notify_wait_agent {
            // notificationError exposes an idle-stop signal failure without changing stop semantics.
            // notificationError 显式暴露空闲停止信号失败，同时不改变停止语义。
            if let Err(notification_error) = notify_webui_wait_agent_parent_interruption_if_waiting(
                state,
                session_id,
                "stop_generation_idle",
            ) {
                eprintln!(
                    "[webui-service] failed to notify wait_agent parent interruption for session {session_id} reason=stop_generation_idle: {notification_error}"
                );
            }
        }
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation_idle");
        sync_webui_session_runtime_kernel_to_workspace_state(state, session_id);
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
/// `true` when the kernel has an active turn or an accepted terminal report is still being persisted.
/// 当 kernel 存在活动回合，或已接纳终态报告仍在持久化时返回 `true`。
pub(crate) fn webui_session_has_active_run(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    state
        .session_runtime_kernels
        .get(&session_id)
        .is_some_and(SessionRuntimeKernel::has_active_turn)
        || state
            .session_terminal_persistence_owners
            .contains_key(&session_id)
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

/// Returns the exact active kernel generation owned by one prompt-preparation turn.
/// 返回单个提示词准备回合精确拥有的活动 kernel 代际。
///
/// # Parameters
/// 参数
/// `state` owns the kernel, terminal-persistence owners, and admission reservations.
/// `state` 持有 kernel、终态持久化所有者与准入预留。
/// `session_id` identifies the session being prepared.
/// `session_id` 标识正在准备的会话。
/// `turn_id` is the immutable logical turn identity captured when the kernel started.
/// `turn_id` 是 kernel 启动时捕获的不可变逻辑回合身份。
///
/// # Returns
/// 返回值
/// The current generation only while the same active turn still owns preparation; otherwise `None`.
/// 仅当同一活动回合仍拥有准备流程时返回当前代际；否则返回 `None`。
pub(crate) fn current_webui_prompt_preparation_generation(
    state: &ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> Option<u64> {
    if state
        .session_terminal_persistence_owners
        .contains_key(&session_id)
    {
        return None;
    }
    if state
        .session_turn_admission_reservations
        .get(&session_id)
        .is_some_and(|reservation| {
            reservation.kind != SessionTurnAdmissionReservationKind::PromptSubmission
        })
    {
        return None;
    }
    // kernel is the sole generation owner; matching only the logical id would permit an ABA restart of the same Continue turn.
    // kernel 是唯一代际所有者；仅匹配逻辑标识会错误放行同一 Continue 回合的 ABA 重启。
    let kernel = state.session_runtime_kernels.get(&session_id)?;
    if kernel.is_stopped()
        || !kernel
            .active_turn()
            .is_some_and(|active_turn| active_turn.turn_id == turn_id)
    {
        return None;
    }
    Some(kernel.generation())
}

/// Returns whether one immutable prompt-preparation owner still matches live service state.
/// 返回单个不可变提示词准备所有者是否仍与实时服务状态匹配。
///
/// # Parameters
/// 参数
/// `state`, `session_id`, and `turn_id` select the live prompt-preparation owner.
/// `state`、`session_id` 与 `turn_id` 选择实时提示词准备所有者。
/// `expected_generation` is the kernel generation captured before asynchronous preparation began.
/// `expected_generation` 是异步准备开始前捕获的 kernel 代际。
///
/// # Returns
/// 返回值
/// `true` only while terminal settlement, Stop, lifecycle mutation, and replacement generations have not taken ownership.
/// 仅当终态结算、Stop、生命周期变更及替代代际均未取得所有权时返回 `true`。
pub(crate) fn webui_prompt_preparation_owner_is_current(
    state: &ServiceBridgeState,
    session_id: SessionId,
    expected_generation: u64,
    turn_id: &str,
) -> bool {
    current_webui_prompt_preparation_generation(state, session_id, turn_id)
        == Some(expected_generation)
}

/// Captures one authoritative existing-session slice together with its exact prompt-preparation generation.
/// 捕获一份权威已有会话切片及其精确提示词准备代际。
///
/// # Parameters
/// 参数
/// `state` owns the live kernel and retained workspace.
/// `state` 持有实时 kernel 与保留工作区。
/// `session_id` and `turn_id` identify the accepted preparation turn.
/// `session_id` 与 `turn_id` 标识已接纳的准备回合。
///
/// # Returns
/// 返回值
/// `Some((session_slice, generation))` for the current owner, `None` after Stop/replacement, or a lock/state error.
/// 当前所有者返回 `Some((session_slice, generation))`；Stop/替代后返回 `None`；加锁或状态异常时返回错误。
pub(crate) fn capture_webui_prompt_preparation_workspace(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> Result<Option<(WorkspaceSnapshot, u64)>, String> {
    // guard makes owner validation and exact-session capture one indivisible read boundary.
    // guard 让所有者校验与精确会话捕获成为不可分割的读取边界。
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(generation) = current_webui_prompt_preparation_generation(&guard, session_id, turn_id)
    else {
        return Ok(None);
    };
    // workspace_snapshot contains only the target summary, owning project, and canonical runtime context.
    // workspace_snapshot 只包含目标摘要、所属项目与规范运行时上下文。
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))
        .and_then(|workspace_snapshot| {
            crate::conversation_submission::existing_submission_workspace_slice(
                workspace_snapshot,
                session_id,
            )
        })?;
    Ok(Some((workspace_snapshot, generation)))
}

/// Captures one prompt-preparation generation without cloning the retained workspace.
/// 在不克隆保留工作区的情况下捕获一个提示词准备代际。
///
/// # Parameters
/// 参数
/// `state` owns the live kernel, while `session_id` and `turn_id` identify the accepted turn.
/// `state` 持有实时 kernel，`session_id` 与 `turn_id` 标识已接纳回合。
///
/// # Returns
/// 返回值
/// The current generation, `None` after Stop/replacement, or an explicit lock/readiness error.
/// 返回当前代际；Stop/替代后返回 `None`；否则返回明确的加锁或就绪错误。
pub(crate) fn capture_webui_prompt_preparation_generation(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> Result<Option<u64>, String> {
    // guard makes owner validation and readiness observation one lock-bounded read.
    // guard 让所有者校验与就绪观察成为一次受锁约束的读取。
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let generation = current_webui_prompt_preparation_generation(&guard, session_id, turn_id);
    if generation.is_some() && guard.workspace_snapshot.is_none() {
        return Err(String::from("workspace snapshot is not ready"));
    }
    Ok(generation)
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
        | ExecutionStreamEvent::TransientReasoningStatusChanged { session_id, .. }
        | ExecutionStreamEvent::ThinkingCompleted { session_id }
        | ExecutionStreamEvent::OutputDelta { session_id, .. }
        | ExecutionStreamEvent::RuntimeEvent { session_id, .. }
        | ExecutionStreamEvent::ConversationCheckpoint { session_id, .. }
        | ExecutionStreamEvent::ContextCompactionRequired { session_id, .. }
        | ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired { session_id, .. }
        | ExecutionStreamEvent::ProviderRouteSwitchRequired { session_id, .. } => *session_id,
        ExecutionStreamEvent::ProviderRequestTelemetry(event)
        | ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, .. }
        | ExecutionStreamEvent::ProviderRequestPreparationBoundary { event, .. } => {
            event.session_id
        }
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

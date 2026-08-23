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

/// Builds the stable parent mailbox input id for one child-agent execution.
/// 为单次子代理执行构建稳定的父级 mailbox 输入标识。
///
/// # Parameters
/// 参数
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the child task session.
/// `task_id` 标识子任务会话。
/// `execution_id` identifies the concrete child execution.
/// `execution_id` 标识具体子执行。
///
/// # Returns
/// 返回值
/// The stable mailbox input id used by enqueue and consume paths.
/// 返回入队与消费路径共用的稳定 mailbox 输入标识。
fn subagent_mailbox_input_id(
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
) -> String {
    format!("subagent-mailbox-{parent_session_id}-{task_id}-{execution_id}")
}

/// Marks a child-agent execution mailbox input as consumed before it is enqueued.
/// 在子代理执行 mailbox 输入入队之前将其标记为已消费。
///
/// # Parameters
/// 参数
/// `state` owns session runtime kernels.
/// `state` 持有会话运行时内核。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the child task session.
/// `task_id` 标识子任务会话。
/// `execution_id` identifies the concrete child execution.
/// `execution_id` 标识具体子执行。
///
/// # Returns
/// 返回值
/// `true` when the consume marker was newly inserted.
/// 新增消费标记时返回 `true`。
pub(crate) fn consume_webui_subagent_mailbox_input_for_execution(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
) -> bool {
    let execution_id = execution_id.trim();
    if execution_id.is_empty() {
        return false;
    }
    let input_id = subagent_mailbox_input_id(parent_session_id, task_id, execution_id);
    let consumed = session_runtime_kernel_mut(state, parent_session_id)
        .input_queue_mut()
        .mark_input_consumed(input_id.clone());
    sync_webui_session_runtime_kernel_to_workspace_state(state, parent_session_id);
    webui_trace(
        "subagent_mailbox_input_preconsumed",
        format!(
            "parent={parent_session_id} task={task_id} input_id={input_id} consumed={consumed}"
        ),
    );
    consumed
}

/// Returns whether one child-agent execution mailbox input was already consumed by wait_agent.
/// 返回某次智能体执行的 mailbox 输入是否已被 wait_agent 消费。
pub(crate) fn webui_subagent_mailbox_input_consumed_for_execution(
    state: &ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
) -> bool {
    let execution_id = execution_id.trim();
    if execution_id.is_empty() {
        return false;
    }
    let input_id = subagent_mailbox_input_id(parent_session_id, task_id, execution_id);
    state
        .session_runtime_kernels
        .get(&parent_session_id)
        .is_some_and(|kernel| kernel.input_queue().is_input_consumed(&input_id))
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
/// `execution_id` identifies the concrete child execution attempt.
/// `execution_id` 标识具体子执行尝试。
pub(crate) fn bind_webui_subagent_parent_turn(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
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
        .entry(SubagentExecutionKey::new(task_id, execution_id))
        .or_insert_with(|| WebuiSubagentParentTurnBinding {
            parent_session_id,
            source_generation,
            parent_turn_id: parent_turn_id.clone(),
        });
    webui_trace(
        "subagent_parent_turn_bound",
        format!(
            "parent={parent_session_id} task={task_id} execution_id={execution_id} generation={source_generation} parent_turn_id={}",
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
/// `execution_id` identifies the exact binding owner to clear.
/// `execution_id` 标识要清理的精确绑定所有者。
pub(crate) fn clear_webui_subagent_parent_turn_binding(
    state: &mut ServiceBridgeState,
    task_id: SessionId,
    execution_id: &str,
) {
    if let Some(binding) = state
        .subagent_parent_turn_bindings
        .remove(&SubagentExecutionKey::new(task_id, execution_id))
    {
        webui_trace(
            "subagent_parent_turn_binding_cleared",
            format!(
                "parent={} task={task_id} execution_id={execution_id} generation={}",
                binding.parent_session_id, binding.source_generation
            ),
        );
    }
}

/// Clears every parent-turn binding for one stable child task during explicit stop or close.
/// 在显式停止或关闭期间清理单个稳定子任务的全部父回合绑定。
///
/// # Parameters
/// 参数
/// `state` owns every execution-scoped parent-turn binding.
/// `state` 持有全部执行级父回合绑定。
/// `task_id` identifies the stable child task whose executions are all being retired.
/// `task_id` 标识全部执行均被退役的稳定子任务。
pub(crate) fn clear_all_webui_subagent_parent_turn_bindings_for_task(
    state: &mut ServiceBridgeState,
    task_id: SessionId,
) {
    state
        .subagent_parent_turn_bindings
        .retain(|key, _| key.task_id != task_id);
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
    start_webui_session_kernel_turn_with_admission(
        state,
        session_id,
        turn_id,
        ActiveTurnKind::UserPrompt,
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

/// Starts one Goal-guided WebUI turn only while its captured dispatch authority is still current.
/// 仅在捕获的分发权威仍为当前版本时启动 Goal 引导的 WebUI 回合。
///
/// # Parameters
/// 参数
/// `state` owns both Goal dispatch versions and runtime kernels under one shared lock.
/// `state` 在同一共享锁下持有 Goal 分发版本与运行时 kernel。
/// `session_id` identifies the Goal session.
/// `session_id` 标识 Goal 会话。
/// `expected_goal_dispatch_version` is the stable version captured around the authoritative Goal read.
/// `expected_goal_dispatch_version` 是围绕权威 Goal 读取捕获的稳定版本。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
/// `kind` is the runtime kernel turn kind.
/// `kind` 是运行时 kernel 回合类型。
///
/// # Returns
/// 返回值
/// Starts the turn atomically with the version check, or rejects retired Goal work.
/// 与版本校验原子地启动回合，或拒绝已退役的 Goal 工作。
pub(crate) fn start_webui_goal_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    expected_goal_dispatch_version: u64,
    turn_id: &str,
    kind: ActiveTurnKind,
) -> Result<(), String> {
    ensure_webui_goal_session_kernel_turn_start_allowed(
        state,
        session_id,
        expected_goal_dispatch_version,
    )?;
    start_webui_session_kernel_turn(state, session_id, turn_id, kind)
}

/// Validates Goal turn authority before any prompt reducer or hook side effect is allowed.
/// 在允许任何提示词归约器或 Hook 副作用前校验 Goal 回合权威。
///
/// # Parameters
/// 参数
/// `state` owns Goal versions, lifecycle exclusions, and runtime kernels.
/// `state` 持有 Goal 版本、生命周期互斥与运行时 kernel。
/// `session_id` identifies the Goal session.
/// `session_id` 标识 Goal 会话。
/// `expected_goal_dispatch_version` is the version captured with the durable Goal read.
/// `expected_goal_dispatch_version` 是随持久 Goal 读取捕获的版本。
///
/// # Returns
/// 返回值
/// Returns success only when the caller may keep the same state lock through reducer execution and registration.
/// 仅当调用方可以持续持有同一状态锁完成归约与注册时返回成功。
pub(crate) fn ensure_webui_goal_session_kernel_turn_start_allowed(
    state: &ServiceBridgeState,
    session_id: SessionId,
    expected_goal_dispatch_version: u64,
) -> Result<(), String> {
    let current_goal_dispatch_version = state
        .session_goal_dispatch_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default();
    if current_goal_dispatch_version != expected_goal_dispatch_version {
        return Err(format!(
            "Goal dispatch authority changed for session `{session_id}` before turn registration"
        ));
    }
    if let Some(reservation) = state.session_turn_admission_reservations.get(&session_id) {
        return Err(format!(
            "Session `{session_id}` is reserved by {}",
            reservation.kind.diagnostic_label()
        ));
    }
    if webui_session_kernel_has_active_turn(state, session_id) {
        return Err(format!(
            "Session `{session_id}` already owns an active runtime kernel turn"
        ));
    }
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
    if workspace_session_with_archive_status(workspace_snapshot, session_id)
        .map(|(session, _)| session)
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
        | ExecutionStreamEvent::TransientReasoningStatusChanged { session_id, .. }
        | ExecutionStreamEvent::ThinkingCompleted { session_id }
        | ExecutionStreamEvent::OutputDelta { session_id, .. }
        | ExecutionStreamEvent::RuntimeEvent { session_id, .. }
        | ExecutionStreamEvent::ConversationCheckpoint { session_id, .. }
        | ExecutionStreamEvent::ContextCompactionRequired { session_id, .. }
        | ExecutionStreamEvent::ProviderRouteSwitchRequired { session_id, .. } => *session_id,
        ExecutionStreamEvent::ProviderRequestTelemetry(event)
        | ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, .. } => event.session_id,
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

/// Outcome returned after attempting to enqueue one subagent mailbox input.
/// 尝试加入一条子代理 mailbox 输入后的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubagentMailboxEnqueueOutcome {
    /// The input was accepted into the parent mailbox.
    /// 输入已进入父级 mailbox。
    Queued,
    /// The same input was already queued or consumed by an earlier delivery path.
    /// 相同输入已经由更早的投递路径入队或消费。
    AlreadyPresentOrConsumed {
        /// Stable mailbox input identifier that caused the idempotent skip.
        /// 导致幂等跳过的稳定 mailbox 输入标识。
        input_id: String,
    },
    /// The input was rejected because it cannot safely belong to this parent turn.
    /// 输入被拒绝，因为它无法安全归属到当前父级回合。
    Rejected(SubagentMailboxEnqueueRejection),
}

impl SubagentMailboxEnqueueOutcome {
    /// Returns whether the input was newly queued.
    /// 返回输入是否新进入队列。
    #[cfg(test)]
    pub(crate) fn is_queued(&self) -> bool {
        matches!(self, Self::Queued)
    }

    /// Returns a stable short label for diagnostics and trace output.
    /// 返回用于诊断和 trace 输出的稳定短标签。
    pub(crate) fn trace_label(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::AlreadyPresentOrConsumed { .. } => "already_present_or_consumed",
            Self::Rejected(rejection) => rejection.reason_code(),
        }
    }
}

/// Hard rejection reason for a subagent mailbox input.
/// 子代理 mailbox 输入的硬拒绝原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubagentMailboxEnqueueRejection {
    /// The task is bound to a different parent session.
    /// 该任务绑定到了不同的父会话。
    ParentMismatch {
        /// Parent session captured when the child task was spawned.
        /// 子任务创建时捕获到的父会话。
        bound_parent_session_id: SessionId,
    },
    /// The captured parent generation is no longer open for delivery.
    /// 捕获的父级代际已经不再接受投递。
    GenerationClosed {
        /// Parent generation captured when the child task was spawned.
        /// 子任务创建时捕获的父级代际。
        source_generation: u64,
        /// Current parent runtime generation.
        /// 当前父级运行时代际。
        current_generation: u64,
        /// Whether the current generation is explicitly stopped.
        /// 当前代际是否已经显式停止。
        stopped: bool,
    },
    /// The child-agent event did not carry an explicit terminal delivery status.
    /// 子代理事件没有携带明确的终态投递状态。
    InvalidTerminalStatus {
        /// Raw status text found on the mailbox event, if any.
        /// mailbox 事件中找到的原始状态文本，如果存在。
        status: Option<String>,
    },
    /// The child-agent event did not carry a valid per-execution identifier.
    /// 智能体事件没有携带有效的单次执行标识。
    InvalidExecutionId {
        /// Raw execution id value found on the task payload, if any.
        /// task 载荷中找到的原始执行标识，如果存在。
        execution_id: Option<String>,
    },
}

impl SubagentMailboxEnqueueRejection {
    /// Returns a stable machine-readable reason code.
    /// 返回稳定的机器可读原因码。
    pub(crate) fn reason_code(&self) -> &'static str {
        match self {
            Self::ParentMismatch { .. } => "parent_mismatch",
            Self::GenerationClosed { .. } => "generation_closed",
            Self::InvalidTerminalStatus { .. } => "invalid_terminal_status",
            Self::InvalidExecutionId { .. } => "invalid_execution_id",
        }
    }

    /// Returns a compact human-readable diagnostic string.
    /// 返回紧凑的人类可读诊断字符串。
    pub(crate) fn diagnostic(&self) -> String {
        match self {
            Self::ParentMismatch {
                bound_parent_session_id,
            } => format!("parent_mismatch bound_parent={bound_parent_session_id}"),
            Self::GenerationClosed {
                source_generation,
                current_generation,
                stopped,
            } => format!(
                "generation_closed source_generation={source_generation} current_generation={current_generation} stopped={stopped}"
            ),
            Self::InvalidTerminalStatus { status } => match status {
                Some(status) => format!("invalid_terminal_status status={status}"),
                None => String::from("invalid_terminal_status status=<missing>"),
            },
            Self::InvalidExecutionId { execution_id } => match execution_id {
                Some(execution_id) => format!("invalid_execution_id execution_id={execution_id}"),
                None => String::from("invalid_execution_id execution_id=<missing>"),
            },
        }
    }
}

/// Extracts the required per-execution identifier for mailbox delivery.
/// 提取 mailbox 投递所必需的单次执行标识。
///
/// # Parameters
/// 参数
/// `metadata` is the original child-agent event metadata.
/// `metadata` 是原始智能体事件元数据。
///
/// # Returns
/// 返回值
/// The trimmed execution id or a hard rejection for malformed events.
/// 返回清理后的执行标识，或对畸形事件给出硬拒绝。
fn subagent_mailbox_execution_id_for_enqueue(
    metadata: &Value,
) -> Result<String, SubagentMailboxEnqueueRejection> {
    let execution_id_value = metadata
        .get("payload")
        .filter(|payload| payload.is_object())
        .and_then(|payload| payload.get("execution_id"));
    let Some(execution_id_value) = execution_id_value else {
        return Err(SubagentMailboxEnqueueRejection::InvalidExecutionId { execution_id: None });
    };
    let Some(execution_id) = execution_id_value.as_str() else {
        return Err(SubagentMailboxEnqueueRejection::InvalidExecutionId {
            execution_id: Some(execution_id_value.to_string()),
        });
    };
    let execution_id = execution_id.trim();
    if execution_id.is_empty() {
        return Err(SubagentMailboxEnqueueRejection::InvalidExecutionId {
            execution_id: Some(String::new()),
        });
    }
    Ok(execution_id.to_string())
}

/// Normalizes an explicit child-agent mailbox terminal status.
/// 规范化明确的智能体 mailbox 终态状态。
///
/// # Parameters
/// 参数
/// `status` is the raw status text carried by the child-agent event.
/// `status` 是智能体事件携带的原始状态文本。
///
/// # Returns
/// 返回值
/// A canonical terminal status label when the input is terminal.
/// 当输入为终态时返回规范终态标签。
pub(crate) fn canonical_subagent_mailbox_terminal_status_label(
    status: &str,
) -> Option<&'static str> {
    match status.trim().to_ascii_lowercase().as_str() {
        "completed" | "complete" | "success" | "succeeded" => Some("completed"),
        "failed" | "error" | "errored" => Some("failed"),
        "stopped" | "cancelled" | "canceled" | "aborted" => Some("stopped"),
        _ => None,
    }
}

/// Extracts the explicit terminal status used for parent mailbox delivery.
/// 提取用于父级 mailbox 投递的明确终态状态。
///
/// # Parameters
/// 参数
/// `metadata` is the original child-agent event metadata.
/// `metadata` 是原始智能体事件元数据。
/// `mailbox_event` is the normalized mailbox event copied into the parent queue.
/// `mailbox_event` 是复制进父级队列的规范 mailbox 事件。
///
/// # Returns
/// 返回值
/// The canonical terminal status or a hard rejection for malformed events.
/// 返回规范终态状态，或对畸形事件给出硬拒绝。
fn subagent_mailbox_terminal_status_for_enqueue(
    metadata: &Value,
    mailbox_event: &Value,
) -> Result<&'static str, SubagentMailboxEnqueueRejection> {
    let event_payload = mailbox_event
        .get("payload")
        .filter(|payload| payload.is_object());
    let status = mailbox_event
        .get("delivery_status")
        .or_else(|| mailbox_event.get("status"))
        .or_else(|| mailbox_event.get("execution_status"))
        .or_else(|| event_payload.and_then(|payload| payload.get("status")))
        .or_else(|| event_payload.and_then(|payload| payload.get("execution_status")))
        .or_else(|| mailbox_event.get("kind"))
        .or_else(|| {
            metadata
                .get("payload")
                .and_then(|payload| payload.get("execution_status"))
        })
        .and_then(Value::as_str);
    let Some(status) = status else {
        return Err(SubagentMailboxEnqueueRejection::InvalidTerminalStatus { status: None });
    };
    canonical_subagent_mailbox_terminal_status_label(status).ok_or_else(|| {
        SubagentMailboxEnqueueRejection::InvalidTerminalStatus {
            status: Some(status.to_string()),
        }
    })
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
/// Structured enqueue outcome that distinguishes queued, idempotent skip, and hard rejection.
/// 区分已入队、幂等跳过和硬拒绝的结构化入队结果。
pub(crate) fn enqueue_webui_subagent_mailbox_input(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    metadata: &Value,
) -> SubagentMailboxEnqueueOutcome {
    let timestamp_ms = current_unix_millis_u64();
    let execution_id = match subagent_mailbox_execution_id_for_enqueue(metadata) {
        Ok(execution_id) => execution_id,
        Err(rejection) => {
            webui_trace(
                "subagent_mailbox_input_rejected_execution_id",
                format!(
                    "parent={parent_session_id} task={task_id} {}",
                    rejection.diagnostic()
                ),
            );
            return SubagentMailboxEnqueueOutcome::Rejected(rejection);
        }
    };
    let binding = state
        .subagent_parent_turn_bindings
        .get(&SubagentExecutionKey::new(task_id, &execution_id))
        .cloned();
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
        return SubagentMailboxEnqueueOutcome::Rejected(
            SubagentMailboxEnqueueRejection::ParentMismatch {
                bound_parent_session_id: binding.parent_session_id,
            },
        );
    }
    let parent_is_waiting_for_subagents = state
        .parent_sessions_waiting_for_subagents
        .contains(&parent_session_id);
    let kernel = session_runtime_kernel_mut(state, parent_session_id);
    let deliver_to_active_parent_turn = kernel
        .active_turn()
        .is_some_and(|turn| turn.kind == ActiveTurnKind::MailboxTriggered)
        && !kernel.has_wait_scopes()
        && kernel.input_queue().delivery_phase() != MailboxDeliveryPhase::Closed;
    if deliver_to_active_parent_turn {
        kernel
            .input_queue_mut()
            .accept_mailbox_delivery_for_current_turn();
    }
    // Terminal child-agent messages should wake an idle parent unless wait_agent owns delivery.
    // 终态子代理消息应唤醒空闲父会话，除非当前由 wait_agent 接管投递。
    let trigger_parent_turn = !parent_is_waiting_for_subagents && !kernel.has_wait_scopes();
    let source_generation = binding
        .as_ref()
        .map(|binding| binding.source_generation)
        .unwrap_or_else(|| kernel.generation());
    let parent_turn_id = binding
        .and_then(|binding| binding.parent_turn_id)
        .or_else(|| kernel.active_turn().map(|turn| turn.turn_id.clone()));
    if source_generation != kernel.generation() || kernel.is_stopped() {
        let rejection = SubagentMailboxEnqueueRejection::GenerationClosed {
            source_generation,
            current_generation: kernel.generation(),
            stopped: kernel.is_stopped(),
        };
        webui_trace(
            "subagent_mailbox_input_rejected_generation",
            format!(
                "parent={parent_session_id} task={task_id} {}",
                rejection.diagnostic()
            ),
        );
        return SubagentMailboxEnqueueOutcome::Rejected(rejection);
    }
    let mailbox_event = metadata
        .get("payload")
        .and_then(|payload| payload.get("subagent_mailbox_event"))
        .cloned()
        .unwrap_or_else(|| metadata.clone());
    let terminal_status =
        match subagent_mailbox_terminal_status_for_enqueue(metadata, &mailbox_event) {
            Ok(status) => status,
            Err(rejection) => {
                webui_trace(
                    "subagent_mailbox_input_rejected_terminal_status",
                    format!(
                        "parent={parent_session_id} task={task_id} {}",
                        rejection.diagnostic()
                    ),
                );
                return SubagentMailboxEnqueueOutcome::Rejected(rejection);
            }
        };
    let parent_turn_id_for_payload = parent_turn_id.clone();
    let input_id = subagent_mailbox_input_id(parent_session_id, task_id, execution_id.as_str());
    let mut input = TurnInput::new(
        input_id.clone(),
        SessionNodeId::from(parent_session_id),
        TurnInputKind::SubagentMessage,
        trigger_parent_turn,
        timestamp_ms,
        json!({
            "task_id": task_id.to_string(),
            "execution_id": execution_id,
            "parent_session_id": parent_session_id.to_string(),
            "source_generation": source_generation,
            "parent_turn_id": parent_turn_id_for_payload,
            "mailbox_event": mailbox_event,
            "delivery_status": terminal_status,
            "trigger_parent_turn": trigger_parent_turn,
            "deliver_to_active_parent_turn": deliver_to_active_parent_turn,
        }),
    );
    if let Some(parent_turn_id) = parent_turn_id.clone() {
        input = input.with_parent_turn_id(parent_turn_id);
    }
    let accepted = kernel.enqueue_mailbox_input_for_generation(input, source_generation);
    sync_webui_session_runtime_kernel_to_workspace_state(state, parent_session_id);
    let outcome = if accepted {
        SubagentMailboxEnqueueOutcome::Queued
    } else {
        SubagentMailboxEnqueueOutcome::AlreadyPresentOrConsumed {
            input_id: input_id.clone(),
        }
    };
    webui_trace(
        "subagent_mailbox_input_enqueued",
        format!(
            "parent={parent_session_id} task={task_id} input_id={input_id} outcome={} generation={source_generation} trigger_parent_turn={trigger_parent_turn} deliver_to_active_parent_turn={deliver_to_active_parent_turn}",
            outcome.trace_label()
        ),
    );
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Builds a workspace snapshot that contains one archived session.
    /// 构建一个包含单个归档会话的工作区快照。
    fn workspace_with_archived_session(
        session_id: SessionId,
        status: SessionState,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "归档会话",
                "",
                status,
                0,
                0,
                "刚刚",
            )],
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
        }
    }

    /// Verifies archived prompt-editing surfaces still obey the session status.
    /// 验证归档提示词编辑表面仍然遵循会话状态。
    #[test]
    fn workspace_surface_allows_user_prompt_editing_uses_archived_session_status() {
        // archived_session_id identifies the archived surface being checked.
        // archived_session_id 标识正在检查的归档表面。
        let archived_session_id = SessionId::new_v4();
        // completed_workspace proves terminal archived sessions may be edited.
        // completed_workspace 证明终态归档会话允许编辑。
        let completed_workspace =
            workspace_with_archived_session(archived_session_id, SessionState::Completed);
        // running_workspace proves still-running archived sessions stay locked.
        // running_workspace 证明仍在运行的归档会话保持锁定。
        let running_workspace =
            workspace_with_archived_session(archived_session_id, SessionState::Running);

        assert!(workspace_surface_allows_user_prompt_editing(
            &completed_workspace,
            archived_session_id,
            None,
        ));
        assert!(!workspace_surface_allows_user_prompt_editing(
            &running_workspace,
            archived_session_id,
            None,
        ));
    }
}

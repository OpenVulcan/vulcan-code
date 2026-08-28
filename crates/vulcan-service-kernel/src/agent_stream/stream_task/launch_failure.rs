//! Active stream task launch failure handling.
//! 活动流任务启动失败处理。

use super::*;

/// Ownership result after attempting to terminalize one failed stream launch.
/// 尝试终结一次失败流启动后的所有权结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamTaskLaunchFailureOutcome {
    /// The failed registration never proved ownership of an installed task entry.
    /// 失败注册从未证明其拥有已安装任务条目。
    OwnershipNotAcquired,
    /// No launch-time kernel generation was captured, so terminal ownership was not provable.
    /// 未捕获启动期内核代际，因此无法证明终态所有权。
    OwnershipUnverified,
    /// A replacement task or kernel generation now owns the session.
    /// 替代任务或内核代际当前拥有会话。
    StaleOwner,
    /// The failed owner atomically applied Failed phase and terminalized its kernel turn.
    /// 失败所有者以原子方式应用 Failed 阶段并终结其内核回合。
    FailedOwnerFinalized,
}

/// Result of applying a caller-owned domain commit at an exact failed stream launch boundary.
/// 在精确失败流启动边界应用调用方领域提交后的结果。
pub(crate) enum StreamTaskLaunchFailureCommit<T> {
    /// Exact ownership was not available, so the caller commit was not executed.
    /// 精确所有权不可用，因此未执行调用方提交。
    Skipped(StreamTaskLaunchFailureOutcome),
    /// Exact ownership was proven and the caller value committed with kernel terminalization.
    /// 精确所有权已证明，调用方值与内核终结共同提交。
    Committed(T),
}

/// Applies a caller-owned domain failure commit for an exact failed registration owner.
/// 为精确注册失败所有者应用调用方领域失败提交。
///
/// # Parameters
/// 参数
/// `task_state`, `session_id`, and `expected_kernel_generation` identify retained launch ownership.
/// `task_state`、`session_id` 与 `expected_kernel_generation` 标识保留启动所有权。
/// `registration_error` proves exact registry cleanup, `commit` mutates domain state, and `project` derives the return value after kernel terminalization.
/// `registration_error` 证明精确注册表清理，`commit` 修改领域状态，`project` 在内核终结后派生返回值。
///
/// # Returns
/// 返回值
/// A committed caller value or the exact reason the caller commit was skipped.
/// 返回已提交的调用方值，或跳过调用方提交的精确原因。
pub(crate) fn commit_stream_task_registration_failure<C, T, F, P>(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
    registration_error: &ActiveStreamTaskRegistrationError,
    commit: F,
    project: P,
) -> Result<StreamTaskLaunchFailureCommit<T>, String>
where
    F: FnOnce(&mut ServiceBridgeState) -> Result<C, String>,
    P: FnOnce(&ServiceBridgeState, C) -> Result<T, String>,
{
    if !registration_error.owned_entry_removed() {
        return Ok(StreamTaskLaunchFailureCommit::Skipped(
            StreamTaskLaunchFailureOutcome::OwnershipNotAcquired,
        ));
    }
    commit_owned_stream_task_launch_failure(
        task_state,
        session_id,
        expected_kernel_generation,
        commit,
        project,
    )
}

/// Applies failed registration terminal state only when exact task and kernel ownership remain provable.
/// 仅在精确任务与内核所有权仍可证明时应用注册失败终态。
///
/// # Parameters
/// # 参数
/// `app` emits the Failed phase delta after the atomic retained mutation commits.
/// `app` 在原子保留状态变更提交后发送 Failed 阶段增量。
/// `task_state` owns the active task registry, session kernel, workspace, and workbench.
/// `task_state` 持有活动任务注册表、会话内核、工作区与工作台。
/// `session_id` identifies the session whose registration failed.
/// `session_id` 标识注册失败的会话。
/// `expected_kernel_generation` is the launch-time kernel generation bound to the failed task.
/// `expected_kernel_generation` 是绑定到失败任务的启动期内核代际。
/// `registration_error` carries proof of whether exact start-gate cleanup removed the installed entry.
/// `registration_error` 携带精确启动闸门清理是否移除已安装条目的证明。
///
/// # Returns
/// # 返回值
/// The strong ownership outcome, or an explicit retained-state projection error.
/// 返回强类型所有权结果，或显式保留状态投影错误。
pub(crate) fn handle_stream_task_registration_failure(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
    registration_error: &ActiveStreamTaskRegistrationError,
) -> Result<StreamTaskLaunchFailureOutcome, String> {
    // Only exact removal by start-gate cleanup proves this failed attempt ever owned the registry slot.
    // 只有启动闸门清理的精确移除才能证明本次失败尝试曾拥有注册表槽位。
    if !registration_error.owned_entry_removed() {
        return Ok(StreamTaskLaunchFailureOutcome::OwnershipNotAcquired);
    }
    finalize_owned_stream_task_launch_failure(
        app,
        task_state,
        session_id,
        expected_kernel_generation,
        true,
    )
}

/// Terminalizes an execution stream that failed before active-task registration and closes its continuation ledger.
/// 终结在活动任务注册前失败的执行流，并闭合其 Continue 账本。
///
/// # Parameters
/// 参数
/// `app` emits the Failed phase delta and `task_state` owns the exact launch-time state.
/// `app` 发送 Failed 阶段增量，`task_state` 持有精确启动期状态。
/// `session_id` identifies the failed execution and `expected_kernel_generation` proves its kernel ownership.
/// `session_id` 标识失败执行，`expected_kernel_generation` 证明其内核所有权。
///
/// # Returns
/// 返回值
/// The strong ownership outcome after phase, kernel, and continuation terminalization, or an explicit state error.
/// 返回阶段、内核与 Continue 终结后的强所有权结果，或明确的状态错误。
pub(crate) fn handle_pre_registration_execution_stream_task_launch_failure(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
) -> Result<StreamTaskLaunchFailureOutcome, String> {
    finalize_owned_stream_task_launch_failure(
        app,
        task_state,
        session_id,
        expected_kernel_generation,
        true,
    )
}

/// Terminalizes a stream turn that failed before any active-task registration was attempted.
/// 终结在尝试注册活动任务之前失败的流回合。
///
/// # Parameters
/// 参数
/// `app` emits the Failed phase delta and `task_state` owns retained launch state.
/// `app` 发送 Failed 阶段增量，`task_state` 持有保留启动状态。
/// `session_id` identifies the failed session and `expected_kernel_generation` binds the exact started turn.
/// `session_id` 标识失败会话，`expected_kernel_generation` 绑定精确的已启动回合。
///
/// # Returns
/// 返回值
/// The strong ownership outcome, or an explicit retained-state projection error.
/// 返回强类型所有权结果，或显式保留状态投影错误。
pub(crate) fn handle_pre_registration_stream_task_launch_failure(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
) -> Result<StreamTaskLaunchFailureOutcome, String> {
    finalize_owned_stream_task_launch_failure(
        app,
        task_state,
        session_id,
        expected_kernel_generation,
        false,
    )
}

/// Finalizes one exact failed stream owner when no active task or replacement kernel exists.
/// 在不存在活动任务或替代内核时终结精确的失败流所有者。
///
/// # Parameters
/// 参数
/// `app` emits the aligned phase delta, `task_state` owns launch state, and `session_id` selects the session.
/// `app` 发送对齐的阶段增量，`task_state` 持有启动状态，`session_id` 选择会话。
/// `expected_kernel_generation` proves ownership of the exact launch-time kernel turn.
/// `expected_kernel_generation` 证明对精确启动期内核回合的所有权。
/// `terminalize_continuation` selects whether this failure belongs to a resumable execution stream.
/// `terminalize_continuation` 选择当前失败是否属于可恢复的执行流。
///
/// # Returns
/// 返回值
/// The verified ownership result, or an explicit retained-state projection error.
/// 返回已验证的所有权结果，或显式保留状态投影错误。
fn finalize_owned_stream_task_launch_failure(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
    terminalize_continuation: bool,
) -> Result<StreamTaskLaunchFailureOutcome, String> {
    if terminalize_continuation {
        // expected_kernel_generation uniquely owns both the failed kernel turn and its terminal-persistence marker.
        // expected_kernel_generation 唯一拥有失败 kernel 回合及其终态持久化标记。
        let Some(expected_kernel_generation) = expected_kernel_generation else {
            return Ok(StreamTaskLaunchFailureOutcome::OwnershipUnverified);
        };
        // terminal_persistence_owner remains stable while the asynchronous retry finalizer is queued.
        // terminal_persistence_owner 在异步重试终结器排队期间保持稳定。
        let terminal_persistence_owner =
            SessionTerminalPersistenceOwner::KernelGeneration(expected_kernel_generation);
        // terminal_phase_event is optional because a proven owner must still close its kernel and retry token when phase projection fails.
        // terminal_phase_event 为可选值，因为阶段投影失败时，已证明所有者仍必须闭合 kernel 与重试令牌。
        let terminal_phase_event = {
            // guard protects ownership validation, marker registration, terminal projection, and kernel closure as one transaction.
            // guard 将所有权验证、标记登记、终态投影与 kernel 闭合作为一个事务保护。
            let mut guard = task_state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            // still_owns_failed_turn excludes replacement tasks, generations, stopped kernels, and another terminal settlement.
            // still_owns_failed_turn 排除替代任务、替代代际、已停止 kernel 与其他终态结算。
            let still_owns_failed_turn = !guard.active_stream_tasks.contains_key(&session_id)
                && !guard
                    .session_terminal_persistence_owners
                    .contains_key(&session_id)
                && guard
                    .session_runtime_kernels
                    .get(&session_id)
                    .is_some_and(|kernel| {
                        kernel.generation() == expected_kernel_generation
                            && kernel.has_active_turn()
                            && !kernel.is_stopped()
                    });
            if !still_owns_failed_turn {
                return Ok(StreamTaskLaunchFailureOutcome::StaleOwner);
            }
            register_session_terminal_persistence_owner(
                &mut guard,
                session_id,
                terminal_persistence_owner,
            )?;
            // phase_event may fail when retained projection state is unavailable; retry invalidation below remains mandatory.
            // phase_event 可能因保留投影状态不可用而失败；下方重试失效仍为强制步骤。
            let phase_event = match apply_session_phase_to_state(
                &mut guard,
                session_id,
                SessionRunPhase::Failed,
            ) {
                Ok(event) => Some(event),
                Err(error) => {
                    eprintln!(
                        "[webui-service] execution stream launch failure phase projection failed: session={session_id} generation={expected_kernel_generation} error={error}"
                    );
                    None
                }
            };
            finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Failed);
            phase_event
        };
        if let Some(terminal_phase_event) = terminal_phase_event {
            emit_service_event_with_trace(
                app,
                "workbench_phase_delta_emit",
                ServiceEvent::WorkbenchDelta(Box::new(terminal_phase_event)),
            );
        }
        return Ok(StreamTaskLaunchFailureOutcome::FailedOwnerFinalized);
    }
    // commit_result carries either the aligned Failed delta or the exact ownership rejection.
    // commit_result 携带对齐的 Failed 增量或精确所有权拒绝。
    let commit_result = commit_owned_stream_task_launch_failure(
        task_state,
        session_id,
        expected_kernel_generation,
        |state| {
            // phaseEvent and the terminal continuation generation commit under the same exact-owner lock.
            // phaseEvent 与终态 Continue 代际在同一个精确所有者锁内提交。
            let phase_event =
                apply_session_phase_to_state(state, session_id, SessionRunPhase::Failed)?;
            Ok(phase_event)
        },
        |_, event| Ok(event),
    )?;
    // event exists only when the failed owner committed phase and kernel state atomically.
    // event 仅在失败所有者原子提交阶段与内核状态时存在。
    let event = match commit_result {
        StreamTaskLaunchFailureCommit::Skipped(outcome) => return Ok(outcome),
        StreamTaskLaunchFailureCommit::Committed(event) => event,
    };
    emit_service_event_with_trace(
        app,
        "workbench_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(StreamTaskLaunchFailureOutcome::FailedOwnerFinalized)
}

/// Executes one domain failure commit only while the exact stream-launch owner remains current.
/// 仅在精确流启动所有者仍为当前所有者时执行领域失败提交。
///
/// # Parameters
/// 参数
/// `task_state`, `session_id`, and `expected_kernel_generation` identify exact retained ownership.
/// `task_state`、`session_id` 与 `expected_kernel_generation` 标识精确保留所有权。
/// `commit` applies the caller's domain failure state before infallible kernel terminalization.
/// `commit` 在不会失败的内核终结前应用调用方领域失败状态。
/// `project` derives the caller result from terminalized retained state and the committed domain value.
/// `project` 从已终结保留状态与已提交领域值派生调用方结果。
///
/// # Returns
/// 返回值
/// The committed caller value, a precise skipped outcome, or a state/commit error.
/// 返回已提交的调用方值、精确跳过结果或状态/提交错误。
fn commit_owned_stream_task_launch_failure<C, T, F, P>(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: Option<u64>,
    commit: F,
    project: P,
) -> Result<StreamTaskLaunchFailureCommit<T>, String>
where
    F: FnOnce(&mut ServiceBridgeState) -> Result<C, String>,
    P: FnOnce(&ServiceBridgeState, C) -> Result<T, String>,
{
    // expected_kernel_generation is required to distinguish this failed turn from later work in the same session.
    // expected_kernel_generation 用于区分当前失败回合与同一会话中的后续工作，因此不可缺失。
    let Some(expected_kernel_generation) = expected_kernel_generation else {
        return Ok(StreamTaskLaunchFailureCommit::Skipped(
            StreamTaskLaunchFailureOutcome::OwnershipUnverified,
        ));
    };

    // committed_value is built under the same lock as ownership checks and kernel terminalization.
    // committed_value 与所有权检查和内核终结在同一锁内构建。
    let committed_value = {
        // guard protects task absence, kernel generation, phase mutation, and terminalization as one transaction.
        // guard 将任务缺失、内核代际、阶段变更与终结保护为一个事务。
        let mut guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // still_owns_failed_turn requires no replacement task and the exact active kernel generation.
        // still_owns_failed_turn 要求不存在替代任务且活动内核代际精确匹配。
        let still_owns_failed_turn = !guard.active_stream_tasks.contains_key(&session_id)
            && guard
                .session_runtime_kernels
                .get(&session_id)
                .is_some_and(|kernel| {
                    kernel.generation() == expected_kernel_generation
                        && kernel.has_active_turn()
                        && !kernel.is_stopped()
                });
        if !still_owns_failed_turn {
            return Ok(StreamTaskLaunchFailureCommit::Skipped(
                StreamTaskLaunchFailureOutcome::StaleOwner,
            ));
        }
        // commit_result is captured before terminalization so a domain error cannot leak the proven kernel owner.
        // commit_result 在内核终结前捕获，确保领域错误不会泄漏已证明归属的内核所有者。
        let commit_result = commit(&mut guard);
        finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Failed);
        // committed_domain is projected only after kernel state has been synchronized into retained workspace state.
        // committed_domain 仅在内核状态同步到保留工作区后进行投影。
        let committed_domain = commit_result?;
        project(&guard, committed_domain)?
    };
    Ok(StreamTaskLaunchFailureCommit::Committed(committed_value))
}

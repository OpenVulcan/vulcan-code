//! Agent stream task exit ownership handling.
//! Agent 流任务退出所有权处理。

use super::*;

/// Exact reason why the root stream polling loop exited.
/// 根流轮询循环退出的精确原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamTaskExitReason {
    /// A Finished event already ran the terminal ownership handler.
    /// Finished 事件已经运行终态所有权处理器。
    FinishedEventHandled,
    /// The stream ended naturally without producing a Finished event.
    /// 流在未产生 Finished 事件时自然结束。
    StreamExhausted,
    /// A newer or stopped session-kernel generation invalidated the stream.
    /// 更新或已停止的会话内核代际使当前流失效。
    StaleKernelGeneration,
}

impl StreamTaskExitReason {
    /// Returns the stable diagnostic label for an early stream termination.
    /// 返回流提前终止的稳定诊断标签。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the exit reason.
    /// 此方法除退出原因外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// The exact trace label, or `None` when Finished already handled terminal cleanup.
    /// 返回精确追踪标签；Finished 已处理终态清理时返回 `None`。
    fn early_termination_label(self) -> Option<&'static str> {
        match self {
            Self::FinishedEventHandled => None,
            Self::StreamExhausted => Some("stream_exhausted"),
            Self::StaleKernelGeneration => Some("stale_kernel_generation"),
        }
    }
}

/// Ownership result after the root stream polling loop exits.
/// 根流轮询循环退出后的所有权处理结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamTaskExitOutcome {
    /// Finished already removed or rejected the active task entry.
    /// Finished 已经移除或拒绝活动任务条目。
    FinishedAlreadyHandled,
    /// The exiting task owned the active entry and completed stopped cleanup.
    /// 退出任务拥有活动条目并完成停止清理。
    EarlyTerminationCleaned,
    /// A replacement task owns the session slot, so no state was changed.
    /// 替代任务拥有会话槽位，因此未修改状态。
    StaleOwner,
    /// The service-state mutex was poisoned and ownership could not be checked.
    /// 服务状态互斥锁已中毒，无法检查所有权。
    OwnershipUnavailable,
    /// The exact owner was removed but its terminal workspace projection could not be committed.
    /// 精确所有者已移除，但其终态工作区投影无法提交。
    StateUnavailable,
}

/// Finalizes root stream-task ownership according to the exact polling-loop exit reason.
/// 根据轮询循环的精确退出原因结束根流任务所有权。
///
/// # Parameters
/// # 参数
/// `task_state` owns the authoritative active-task registry and session kernel.
/// `task_state` 持有权威活动任务注册表与会话内核。
/// `session_id` identifies the session slot owned by the exiting task.
/// `session_id` 标识退出任务拥有的会话槽位。
/// `active_stream_task_id` is the unique identity captured when the task was registered.
/// `active_stream_task_id` 是任务注册时捕获的唯一身份。
/// `exit_reason` distinguishes prior Finished handling from each early termination source.
/// `exit_reason` 区分先前的 Finished 处理与各类提前终止来源。
///
/// # Returns
/// # 返回值
/// A strong ownership outcome describing the exact cleanup decision.
/// 返回描述精确清理决策的强类型所有权结果。
pub(crate) fn handle_stream_task_exit(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    exit_reason: StreamTaskExitReason,
) -> StreamTaskExitOutcome {
    // Finished owns its terminal cleanup inside handle_finished_stream_event and must never be finalized twice here.
    // Finished 在 handle_finished_stream_event 内拥有终态清理，不得在此重复结束。
    // early_termination_label is present only for exits that still require ownership cleanup.
    // early_termination_label 仅在仍需执行所有权清理的退出路径中存在。
    let Some(early_termination_label) = exit_reason.early_termination_label() else {
        return StreamTaskExitOutcome::FinishedAlreadyHandled;
    };

    webui_trace(
        "stream_task_ended_without_finished",
        format!("session={session_id} reason={early_termination_label}"),
    );

    // terminalPhase classifies natural exhaustion as a failure and stopped-generation invalidation as stopped.
    // terminalPhase 将自然耗尽归类为失败，并将已停止代际失效归类为已停止。
    let terminal_phase = match exit_reason {
        StreamTaskExitReason::StreamExhausted => SessionRunPhase::Failed,
        StreamTaskExitReason::StaleKernelGeneration => SessionRunPhase::Stopped,
        StreamTaskExitReason::FinishedEventHandled => unreachable!("Finished exits return above"),
    };
    let terminal_status = match terminal_phase {
        SessionRunPhase::Failed => TurnTerminalStatus::Failed,
        SessionRunPhase::Stopped => TurnTerminalStatus::Stopped,
        _ => unreachable!("early stream exits use terminal phases"),
    };
    // terminal_persistence_owner keeps replacement work closed after this exact active task is removed.
    // terminal_persistence_owner 在当前精确活动任务移除后保持替代工作关闭。
    let terminal_persistence_owner =
        SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id);
    // guard protects phase projection, identity removal, and kernel terminalization as one ownership transaction.
    // guard 将阶段投影、身份移除与内核终结保护为一个所有权事务。
    let mut guard = match task_state.lock() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("[webui-service] early stream termination ownership check failed: {error}");
            webui_trace(
                "stream_task_exit_ownership_unavailable",
                format!("session={session_id} reason={early_termination_label}"),
            );
            return StreamTaskExitOutcome::OwnershipUnavailable;
        }
    };
    // The exact owner is validated before phase projection so a stale task cannot terminalize its replacement.
    // 在阶段投影前验证精确所有者，避免过期任务终结替代任务。
    if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
        webui_trace(
            "stream_task_exit_stale_owner",
            format!("session={session_id} reason={early_termination_label}"),
        );
        return StreamTaskExitOutcome::StaleOwner;
    }
    if let Err(error) = register_session_terminal_persistence_owner(
        &mut guard,
        session_id,
        terminal_persistence_owner,
    ) {
        eprintln!(
            "[webui-service] early stream termination terminal-persistence registration failed: session={session_id} reason={early_termination_label} error={error}"
        );
        return StreamTaskExitOutcome::StateUnavailable;
    }
    // terminalPhaseEvent is retained before ownership cleanup can make a later Stop appear idle.
    // terminalPhaseEvent 在所有权清理可能使后续 Stop 看似空闲之前完成保留。
    let terminal_phase_event = match apply_session_phase_to_state(
        &mut guard,
        session_id,
        terminal_phase,
    ) {
        Ok(event) => Some(event),
        Err(error) => {
            eprintln!(
                "[webui-service] early stream termination phase projection failed: session={session_id} reason={early_termination_label} error={error}"
            );
            None
        }
    };
    // removedTask consumes the exact owner only after every retained status surface reaches terminal state.
    // removedTask 仅在所有保留状态面进入终态后消费精确所有者。
    let removed_task =
        remove_active_stream_task_if_id(&mut guard, session_id, active_stream_task_id);
    if removed_task.is_none() {
        let _ = rollback_session_terminal_persistence_owner(
            &mut guard,
            session_id,
            terminal_persistence_owner,
        );
        eprintln!(
            "[webui-service] early stream termination exact owner disappeared inside terminal transaction: session={session_id} task={active_stream_task_id}"
        );
        return StreamTaskExitOutcome::StateUnavailable;
    }
    finish_webui_session_kernel_turn(&mut guard, session_id, terminal_status);
    drop(guard);
    if let Some(terminal_phase_event) = terminal_phase_event {
        emit_service_event_with_trace(
            app,
            "workbench_early_stream_terminal_phase_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(terminal_phase_event)),
        );
    }
    StreamTaskExitOutcome::EarlyTerminationCleaned
}

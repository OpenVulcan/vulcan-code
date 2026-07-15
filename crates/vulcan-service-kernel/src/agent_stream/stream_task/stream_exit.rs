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

    // guard protects the identity check and kernel terminalization as one ownership transaction.
    // guard 将身份检查与内核终结保护为一个所有权事务。
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
    // removed_task is present only when this exact task still owns the session slot.
    // removed_task 仅在当前精确任务仍拥有会话槽位时存在。
    let removed_task =
        remove_active_stream_task_if_id(&mut guard, session_id, active_stream_task_id);
    if removed_task.is_none() {
        webui_trace(
            "stream_task_exit_stale_owner",
            format!("session={session_id} reason={early_termination_label}"),
        );
        return StreamTaskExitOutcome::StaleOwner;
    }

    finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Stopped);
    StreamTaskExitOutcome::EarlyTerminationCleaned
}

//! Reconcile inactive journal turns only under exact physical submission ownership.
//! 仅在精确物理提交所有权下对账已停止的日志回合。

use super::*;
use crate::{SessionTurnAdmissionReservationKind, webui_session_has_active_run};

/// Submits `submission` through `handle` while `reservation_id` excludes replacement execution.
/// 当 `reservation_id` 排除替代执行时，通过 `handle` 提交 `submission`。
/// `state` proves runtime ownership and `now_ms` timestamps admission; returns a durable receipt or an error.
/// `state` 证明运行时所有权，`now_ms` 标记准入时间；返回可靠回执或错误。
pub(crate) async fn submit_root_input_when_idle(
    state: &SharedServiceBridgeState,
    handle: &Arc<SessionHandle>,
    reservation_id: Uuid,
    submission: ClientInputSubmission,
    now_ms: i64,
) -> Result<(), String> {
    // Settle completed preparation workers before inspecting activity; never close across their writes.
    // 检查活动之前结算已完成准备工作器，绝不跨其写入关闭回合。
    let _preparation = handle.acquire_request_preparation_settlement().await;
    // The exact reservation survives this state lock and protects the following durable transaction.
    // 精确预留在状态锁释放后仍存活，保护后续可靠事务。
    let session_id = handle.session_id();
    {
        // Runtime owners, not historical turn markers, determine whether replacement is safe.
        // 运行时所有者决定能否安全替代，而非历史回合标记。
        let guard = state.lock().map_err(|error| error.to_string())?;
        if !guard
            .session_turn_admission_reservations
            .get(&session_id)
            .is_some_and(|reservation| {
                reservation.reservation_id == reservation_id
                    && reservation.kind == SessionTurnAdmissionReservationKind::PromptSubmission
            })
        {
            return Err(String::from(
                "Idle root admission requires its exact submission reservation.",
            ));
        }
        if webui_session_has_active_run(&guard, session_id)
            || guard.active_stream_tasks.contains_key(&session_id)
            || guard.question_preparation_tasks.contains_key(&session_id)
        {
            return Err(String::from(
                "Root input cannot replace a live execution or terminal persistence owner.",
            ));
        }
    }
    handle
        .submit_root_input_after_execution_ended(submission, now_ms)
        .map_err(|error| error.to_string())?;
    Ok(())
}

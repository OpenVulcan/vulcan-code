//! Durable settlement of errors between user-input admission and execution startup.
//! 用户输入准入与执行启动之间错误的可靠结算。

use crate::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    SharedServiceBridgeState, register_session_terminal_persistence_owner,
    webui_session_kernel_has_active_turn,
};
use uuid::Uuid;
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::TurnId;
use vulcan_workbench_model::{SessionId, SessionRunPhase};

/// Persists a real submission error and closes only its admitted, undispatched logical turn.
/// 持久化真实提交错误，并只闭合它已准入但尚未分发的逻辑回合。
/// `app` publishes phase changes; `state` owns live admission; `session_id` and `turn_id` pin the failed input.
/// `app` 发布阶段变化；`state` 持有实时准入；`session_id` 与 `turn_id` 固定失败输入。
/// `failure` is the original service diagnostic. The caller must retain its submission admission guard.
/// `failure` 是原始服务诊断；调用者必须保持提交准入守卫存活。
/// Returns success after durable error publication; persistence failure retains the terminal lock for explicit recovery.
/// 可靠发布错误后返回成功；持久化失败保留终态锁，等待显式恢复。
pub(crate) async fn settle_admitted_submission_failure(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: TurnId,
    failure: &str,
) -> Result<(), String> {
    // Terminal ownership prevents another submit/Continue from racing error persistence.
    // 终态所有权避免其他提交或继续执行与错误落盘竞争。
    let terminal_owner = SessionTerminalPersistenceOwner::PromptPreparation(Uuid::new_v4());
    {
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        if guard.active_stream_tasks.contains_key(&session_id)
            || webui_session_kernel_has_active_turn(&guard, session_id)
        {
            return Err(String::from(
                "Submission failure cannot settle a running execution; new input must remain queued.",
            ));
        }
        register_session_terminal_persistence_owner(&mut guard, session_id, terminal_owner)?;
    }
    let terminal_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        terminal_owner,
    )?;
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // The captured turn must still own the document and must not have started a physical API.
    // 捕获的回合必须仍拥有文档，并且不能已经启动物理 API。
    let owns_undispatched_turn = handle
        .read(|document| {
            document
                .active_turn()
                .is_some_and(|turn| turn.turn_id == turn_id)
                && document.active_api().is_none()
        })
        .map_err(|error| error.to_string())?;
    if !owns_undispatched_turn {
        terminal_guard.release()?;
        return Err(String::from(
            "Submission failure no longer owns its admitted V1 turn.",
        ));
    }
    // This appends a UI-only error and terminal facts, not an assistant answer or a fabricated API result.
    // 此处追加仅界面可见的错误和终态事实，不生成助手回答，也不伪造 API 结果。
    // The original UserInput remains in Provider context alongside the next independently admitted UserInput.
    // 原 UserInput 会与下一条独立准入的 UserInput 一起保留在 Provider 上下文中。
    crate::persist_v1_execution_failure_boundary(
        state,
        session_id,
        "submission_preparation_failed",
        Some(failure.to_owned()),
    )
    .await?;
    // A failed phase projection must not keep a durably settled turn locked forever.
    // 阶段投影失败不能使已可靠结算的回合永久锁定。
    let phase_result = crate::commit_retained_workspace_session_phase_mutation_and_emit_delta(
        app,
        state,
        session_id,
        SessionRunPhase::Stopped,
        |workspace| {
            for session in &mut workspace.sessions {
                if session.id == session_id {
                    session.status = vulcan_workbench_model::SessionState::Blocked;
                }
            }
            Ok(())
        },
    );
    terminal_guard.release()?;
    phase_result
}

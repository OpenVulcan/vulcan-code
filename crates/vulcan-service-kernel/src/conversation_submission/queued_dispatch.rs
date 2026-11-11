//! Dispatches accepted queued turns using their original durable identities.
//! 使用原始可靠身份派发已接收的排程回合。

use std::sync::Arc;
use tokio::sync::Notify;
use vulcan_service_api::{QueuedTurnDispatchStatus, ServiceEventEmitter};
use vulcan_session_core::v1::{
    ClientActionId, TurnId,
    collaboration::CollaborationRunControl,
    document::TurnOrigin,
    event::{DisplayNoticeKind, EventBody, NoticeSeverity, UserInputSource},
};
use vulcan_workbench_model::SessionId;

use crate::{
    ServiceBridgeState, SharedServiceBridgeState,
    project_conversation_services::{
        ExistingSubmissionAdmissionAuthorization, try_begin_pending_prompt_dispatch_exclusion,
    },
    runtime_parts_from_state,
    session_goal_lifecycle::session_v1_handle_from_state,
    session_has_pending_plan_artifact,
    session_runtime_registry_v1::{
        SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
    },
    spawn_service_task,
    webui_prompt_lifecycle::queued_dispatch_authority_for_client_action,
    webui_session_has_active_run,
    webui_turn_chaining::start_v1_queued_turn_with_admission,
};

/// Tracks a process-local wakeup for one durably accepted, undispatched turn.
/// 跟踪一个已可靠接收但尚未派发回合的进程内唤醒。
pub(crate) struct WaitingQueuedTurn {
    /// Exact turn whose durable input may be dispatched.
    /// 可以派发其可靠输入的精确回合。
    turn_id: TurnId,
    /// A single permit covers changes that occur before the waiter parks.
    /// 单个许可覆盖等待者休眠前发生的变化。
    wake: Arc<Notify>,
}

impl Drop for WaitingQueuedTurn {
    /// Releases a parked task when its registration or the service is dropped.
    /// 等待登记或服务销毁时释放休眠任务。
    fn drop(&mut self) {
        self.wake.notify_one();
    }
}

/// Signals an existing waiter after a session-local admission condition changes.
/// 会话内准入条件改变后唤醒已有等待者。
/// The caller holds service state and provides the changed session. Returns no value.
/// 调用方持有服务状态并提供发生变化的会话。无返回值。
pub(crate) fn wake_waiting_queued_turn(state: &ServiceBridgeState, session_id: SessionId) {
    if let Some(waiting) = state.waiting_queued_turns.get(&session_id) {
        waiting.wake.notify_one();
    }
}

/// Finds the original queued root before any business API has started for it.
/// 在业务 API 启动前查找原排程根回合。
/// The handle owns the durable history. Returns the original turn and action or no eligible root.
/// 句柄持有可靠历史。返回原回合和动作，或表示没有符合条件的根回合。
pub(super) fn undispatched_queued_root(
    handle: &SessionHandle,
) -> Result<Option<(TurnId, ClientActionId)>, String> {
    handle
        .read(|document| {
            // The root event is authoritative even when the process-local queue panel has been rebuilt.
            // 即使进程内队列面板已经重建，根事件仍是权威。
            let Some(turn) = document.active_turn().filter(|turn| {
                !turn.closed
                    && turn.origin == TurnOrigin::UserRoot
                    && document.metadata.archived_at_ms.is_none()
                    && !document.business_input_dispatch_started(turn.turn_id, turn.start_seq)
            }) else {
                return Ok(None);
            };
            // A missing root event is a history error, so never infer its identity from the draft.
            // 缺少根事件属于历史错误，不能从草稿推断其身份。
            let root = document
                .events()
                .iter()
                .find(|event| event.seq == turn.start_seq && event.turn_id == Some(turn.turn_id))
                .ok_or_else(|| {
                    String::from("The open user turn is missing its durable root input.")
                })?;
            match &root.event {
                EventBody::UserInput(input)
                    if input.input_source == UserInputSource::QueuedScheduled =>
                {
                    Ok(Some((turn.turn_id, input.client_action_id.clone())))
                }
                EventBody::UserInput(_) => Ok(None),
                _ => Err(String::from(
                    "The open user turn does not start with a user input.",
                )),
            }
        })
        .map_err(|error| error.to_string())?
}

/// Reports a temporary live blocker, excluding the reservation owned by this submission.
/// 返回暂时的实时阻塞原因，排除本次提交持有的精确预留。
/// The session and optional authorization select the owner. Returns a stable reason or readiness.
/// 会话与可选授权确定所有者。返回稳定原因，或表示已经就绪。
fn dispatch_blocker(
    state: &ServiceBridgeState,
    session_id: SessionId,
    authorization: Option<ExistingSubmissionAdmissionAuthorization>,
) -> Option<&'static str> {
    if state.active_stream_tasks.contains_key(&session_id)
        || webui_session_has_active_run(state, session_id)
    {
        return Some("active_execution");
    }
    if state
        .workspace_snapshot
        .as_ref()
        .is_some_and(|workspace| session_has_pending_plan_artifact(workspace, session_id))
    {
        return Some("pending_plan");
    }
    if state
        .session_turn_admission_reservations
        .get(&session_id)
        .is_some_and(|reservation| {
            !authorization.is_some_and(|owner| {
                owner.session_id == session_id && owner.reservation_id == reservation.reservation_id
            })
        })
    {
        return Some("admission_reserved");
    }
    None
}

/// Attempts to dispatch the original accepted turn without adding another input.
/// 尝试派发原已接收回合，不追加输入。
/// The arguments bind the live service and original input. Returns a wait reason, completion, or failure.
/// 参数绑定实时服务和原始输入。返回等待原因、完成或失败。
async fn try_dispatch(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: TurnId,
    client_action_id: &ClientActionId,
    authorization: Option<ExistingSubmissionAdmissionAuthorization>,
) -> Result<Option<&'static str>, String> {
    // Runtime parts and handle come from the live service, while the input comes from V1.
    // 运行组件与句柄来自实时服务，输入则来自 V1。
    let (controller, agent, _, _, _) = runtime_parts_from_state(state)?;
    let handle = session_v1_handle_from_state(state, session_id).await?;
    // expected pins the original action even if another turn takes its place while waiting.
    // expected 固定原始动作，即使等待期间另一个回合取代它。
    let expected = (turn_id, client_action_id.clone());
    if undispatched_queued_root(handle.as_ref())?.as_ref() != Some(&expected) {
        return Ok(None);
    }
    // A stop is durable and may have won since the input was accepted.
    // 停止是可靠状态，可能在输入接收后已经生效。
    let runnable = handle
        .read(|document| {
            matches!(
                document.collaboration_run_control(),
                CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle,
            )
        })
        .map_err(|error| error.to_string())?;
    if !runnable {
        return Ok(None);
    }
    // Check before launching, then check again if admission refused.
    // 启动前检查，准入拒绝后再复核。
    {
        let guard = state.lock().map_err(|error| error.to_string())?;
        if let Some(reason) = dispatch_blocker(&guard, session_id, authorization) {
            return Ok(Some(reason));
        }
    }
    // A fresh caller acquires the same exact reservation before reading dispatch authority.
    // 新调用方在读取派发权威前获取同一精确预留。
    let acquired;
    let authorization = if authorization.is_some() {
        authorization
    } else {
        acquired = try_begin_pending_prompt_dispatch_exclusion(state, session_id)?;
        let Some(owner) = acquired.as_ref() else {
            return Ok(Some("admission_busy"));
        };
        Some(owner.existing_submission_authorization(session_id)?)
    };
    // authority revalidates the turn, input sequence, and collaboration constraints.
    // authority 重新校验回合、输入序号及协作限制。
    let authority = queued_dispatch_authority_for_client_action(handle.as_ref(), client_action_id)?;
    if start_v1_queued_turn_with_admission(
        app,
        state,
        &controller,
        &agent,
        session_id,
        authority,
        authorization,
    )
    .await?
    {
        return Ok(None);
    }
    if undispatched_queued_root(handle.as_ref())?.as_ref() != Some(&expected) {
        return Ok(None);
    }
    // Only a known temporary owner can leave an accepted turn waiting automatically.
    // 只有已知的暂时占用者才能使已接收回合继续自动等待。
    let guard = state.lock().map_err(|error| error.to_string())?;
    dispatch_blocker(&guard, session_id, authorization)
        .map(Some)
        .ok_or_else(|| {
            String::from("The accepted queued turn could not acquire dispatch admission.")
        })
}

/// Records a sanitized, durable failure on the original open queued turn.
/// 将脱敏后的失败可靠记录到原开放排程回合。
/// Returns no value. A superseded or started turn cannot receive the note.
/// 无返回值。已被取代或已经启动的回合不接收该通知。
fn report_dispatch_failure(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: TurnId,
    action: &ClientActionId,
) {
    tracing::warn!(%session_id, turn_id = turn_id.value(), action = %action.as_str(),
        reason = "queued_turn_dispatch_failed", "Accepted queued turn could not start");
    // Only the registered durable owner can write a diagnostic after admission.
    // 只有已注册的可靠所有者才能在准入后写入诊断。
    let result = (|| -> Result<(), String> {
        let now = crate::current_v1_unix_millis()?;
        let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
            state, session_id,
        )?;
        handle.mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                // Check the original input inside the same mutation as the diagnostic.
                // 在写入诊断的同一变更内检查原始输入。
                if document.active_turn().is_none_or(|turn| turn.closed || turn.turn_id != turn_id
                    || document.business_input_dispatch_started(turn_id, turn.start_seq)
                    || !document.events().iter().any(|event| event.seq == turn.start_seq
                        && event.turn_id == Some(turn_id)
                        && matches!(&event.event, EventBody::UserInput(input) if input.client_action_id == *action
                            && input.input_source == UserInputSource::QueuedScheduled)))
                    || document.metadata.archived_at_ms.is_some()
                    || document.events().iter().any(|event| event.turn_id == Some(turn_id)
                        && matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "queued_turn_dispatch_failed"))
                {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                document.append_display_note(Some(turn_id), DisplayNoticeKind::SystemError, NoticeSeverity::Warning,
                    "queued_turn_dispatch_failed", None, None, None, None, now)?;
                Ok(SessionMutationChange::Changed(()))
            },
        ).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        tracing::warn!(%session_id, turn_id = turn_id.value(), "Failed to publish queued dispatch diagnostic");
    }
}

/// Keeps one waiter for an accepted turn and rechecks immediately after registration.
/// 为已接收回合保留一个等待者，并在登记后立即重新检查。
/// The caller supplies the durable turn and action. Returns an error if registration fails.
/// 调用方提供可靠回合与动作。登记失败时返回错误。
fn wait_for_dispatch(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: TurnId,
    client_action_id: ClientActionId,
) -> Result<(), String> {
    // wake stores a permit when an owner releases before the task reaches its await.
    // 所有者在任务开始等待前释放时，wake 会保存一个许可。
    let wake = Arc::new(Notify::new());
    {
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        if guard
            .waiting_queued_turns
            .get(&session_id)
            .is_some_and(|waiting| waiting.turn_id == turn_id)
        {
            return Ok(());
        }
        if let Some(previous) = guard.waiting_queued_turns.insert(
            session_id,
            WaitingQueuedTurn {
                turn_id,
                wake: wake.clone(),
            },
        ) {
            previous.wake.notify_one();
        }
    }
    // The waiting task does not hold the service alive while parked.
    // 等待任务休眠时不会延长服务的生命周期。
    let weak_state = Arc::downgrade(state);
    let app = app.clone();
    spawn_service_task(async move {
        loop {
            {
                let Some(state) = weak_state.upgrade() else {
                    return;
                };
                // Recheck immediately to cover release before waiter registration.
                // 立即复核，以覆盖阻塞条件在等待者登记前解除的情况。
                let result =
                    try_dispatch(&app, &state, session_id, turn_id, &client_action_id, None).await;
                if !matches!(result, Ok(Some(_))) {
                    if result.is_err() {
                        report_dispatch_failure(&state, session_id, turn_id, &client_action_id);
                    }
                    if let Ok(mut guard) = state.lock()
                        && guard
                            .waiting_queued_turns
                            .get(&session_id)
                            .is_some_and(|waiting| Arc::ptr_eq(&waiting.wake, &wake))
                    {
                        guard.waiting_queued_turns.remove(&session_id);
                    }
                    return;
                }
            }
            wake.notified().await;
        }
    });
    Ok(())
}

/// Starts an accepted turn or registers an event-driven wait and returns its receipt state.
/// 启动已接收回合，或登记事件驱动等待并返回回执状态。
/// The input identity is already durable, so failures remain attached to its receipt.
/// 输入身份已可靠保存，因此失败仍附在原回执上。
pub(super) async fn dispatch_accepted_queued_turn(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: TurnId,
    client_action_id: ClientActionId,
    authorization: Option<ExistingSubmissionAdmissionAuthorization>,
) -> Option<QueuedTurnDispatchStatus> {
    match try_dispatch(
        app,
        state,
        session_id,
        turn_id,
        &client_action_id,
        authorization,
    )
    .await
    {
        Ok(None) => None,
        Ok(Some(reason)) => {
            tracing::info!(%session_id, turn_id = turn_id.value(), action = %client_action_id.as_str(), reason,
                "Accepted queued turn is waiting for dispatch");
            match wait_for_dispatch(app, state, session_id, turn_id, client_action_id) {
                Ok(()) => Some(QueuedTurnDispatchStatus::Waiting),
                Err(_) => Some(QueuedTurnDispatchStatus::Failed),
            }
        }
        Err(_) => {
            report_dispatch_failure(state, session_id, turn_id, &client_action_id);
            Some(QueuedTurnDispatchStatus::Failed)
        }
    }
}

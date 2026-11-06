//! Wakes an idle root session after its background shell jobs finished.
//! 后台 shell 任务结束后唤醒空闲的根会话。
//!
//! Finish notices stay process-local in the shell job runtime and are delivered by the provider
//! sink before the next request of any turn. When the owning session is idle, this module opens one
//! host-initiated continuation turn so that delivery happens without waiting for the user; a busy
//! session keeps receiving its notices before the next request of the running turn.
//! 结束通知只在 shell 任务运行时的进程内保存，并由 Provider sink 在任一回合的下一次请求前
//! 送达。所属会话空闲时，本模块开启一个宿主发起的续跑回合，使送达不必等待用户；繁忙会话
//! 仍在当前回合的下一次请求前收到通知。

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use tokio::sync::broadcast;
use vulcan_host_tools::shell_job::{ShellJobEventKind, ShellJobRuntime};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::collaboration::{
    CollaborationQuestionExecutionState, CollaborationRunControl,
};
use vulcan_session_core::v1::document::{SessionDocument, ShellJobContinuationStart, TurnOrigin};
use vulcan_session_core::v1::ids::{Seq, TurnId};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};
use crate::{
    AgentRuntimeBridge, ServiceBridgeState, SharedServiceBridgeState, current_v1_unix_millis,
    session_has_pending_plan_artifact, webui_trace,
};

/// Provider-visible start content of one shell-job wake turn.
/// 一个 shell 任务唤醒回合的 Provider 可见起点内容。
pub(crate) const SHELL_JOB_CONTINUATION_CONTENT: &str = "Background shell jobs started earlier in this session finished while the session was idle. Their completion notices follow. Review the results and continue the work they belong to; if nothing remains to do, reply briefly.";

/// Exact durable identity of one open shell-job wake turn.
/// 一个开放 shell 任务唤醒回合的精确可靠身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionShellJobDispatchAuthority {
    /// Exact history generation; local turn and sequence identifiers may repeat after replacement.
    /// 精确历史代次；替换后局部回合与序号标识可能重复。
    pub(crate) generation: u64,
    /// Exact persisted open wake turn.
    /// 精确持久化的开放唤醒回合。
    pub(crate) turn_id: TurnId,
    /// Provider-visible context seq that opened the wake turn.
    /// 开启唤醒回合的 Provider 可见上下文 seq。
    pub(crate) context_seq: Seq,
}

/// Derives the wake authority from one caller-owned canonical snapshot.
/// 从调用方持有的一份规范快照推导唤醒权威。
///
/// # Returns
/// 返回值
/// `None` while archived, stopped, occupied or when the open turn is not a wake turn.
/// 归档、停止、被占用或开放回合不是唤醒回合时返回 `None`。
pub(crate) fn shell_job_dispatch_authority_from_document(
    document: &SessionDocument,
) -> Option<SessionShellJobDispatchAuthority> {
    if document.metadata.archived_at_ms.is_some()
        || document.collaboration_run_control() != CollaborationRunControl::Running
        || document.active_api().is_some()
        || document
            .collaboration_state()
            .question_executions
            .values()
            .any(|execution| execution.state == CollaborationQuestionExecutionState::Running)
    {
        return None;
    }
    let turn = document.active_turn()?;
    (!turn.closed && turn.origin == TurnOrigin::ShellJobContinuation).then_some(
        SessionShellJobDispatchAuthority {
            generation: document.segment_generation(),
            turn_id: turn.turn_id,
            context_seq: turn.start_seq,
        },
    )
}

/// Reads the wake authority from one registered session handle.
/// 从一个已注册会话句柄读取唤醒权威。
pub(crate) fn shell_job_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
) -> Result<Option<SessionShellJobDispatchAuthority>, String> {
    handle
        .read(shell_job_dispatch_authority_from_document)
        .map_err(|error| format!("Failed to read the V1 shell-job wake identity: {error}"))
}

/// Number of finish notices of `session_id` that no request has delivered yet.
/// `session_id` 尚未被任何请求送达的结束通知数量。
fn pending_notice_count(session_id: SessionId) -> usize {
    ShellJobRuntime::global().pending_notice_count(
        &vulcan_agent_runtime::tooling::native::canonical_shell_job_session_id(
            &session_id.to_string(),
        ),
    )
}

/// Sessions with a wake attempt in progress, each with whether another attempt was requested.
/// 正在进行唤醒尝试的会话，以及是否又请求了一次尝试。
static WAKE_ATTEMPTS: LazyLock<Mutex<HashMap<SessionId, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Exclusive wake slot of one session; dropping it ends the session's attempt series.
/// 某会话独占的唤醒槽；丢弃即结束该会话的尝试序列。
struct WakeSlot(SessionId);

impl WakeSlot {
    /// Claims the slot, or records a rerun request for the attempt already holding it.
    /// 取得唤醒槽；已被占用时为持有者记录一次重试请求。
    fn claim(session_id: SessionId) -> Option<Self> {
        let mut attempts = WAKE_ATTEMPTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(rerun) = attempts.get_mut(&session_id) {
            *rerun = true;
            return None;
        }
        attempts.insert(session_id, false);
        Some(Self(session_id))
    }

    /// Takes the rerun request recorded while the current attempt ran.
    /// 取出当前尝试运行期间记录的重试请求。
    fn take_rerun(&self) -> bool {
        WAKE_ATTEMPTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&self.0)
            .is_some_and(std::mem::take)
    }
}

impl Drop for WakeSlot {
    fn drop(&mut self) {
        WAKE_ATTEMPTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.0);
    }
}

/// Starts one wake turn when `session_id` is an idle root session with undelivered finish notices.
/// 当 `session_id` 是带有未送达结束通知的空闲根会话时，启动一个唤醒回合。
///
/// Concurrent triggers for the same session collapse into one attempt series, so several jobs
/// finishing together start at most one turn whose first request delivers all their notices.
/// 同一会话的并发触发合并为一个尝试序列，因此同时结束的多个任务最多启动一个回合，
/// 其首次请求送达全部通知。
///
/// # Returns
/// 返回值
/// `true` when a wake turn started its formal dispatch.
/// 唤醒回合启动正式分发时返回 `true`。
pub(crate) async fn maybe_start_shell_job_continuation_if_idle(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    if pending_notice_count(session_id) == 0 {
        return Ok(false);
    }
    let Some(slot) = WakeSlot::claim(session_id) else {
        return Ok(false);
    };
    let mut started = false;
    loop {
        started |= attempt_shell_job_continuation(
            app,
            state,
            controller_runtime,
            agent_runtime,
            session_id,
        )
        .await?;
        if started || !slot.take_rerun() {
            return Ok(started);
        }
    }
}

/// Opens and dispatches one wake turn; an opened turn that cannot dispatch is stopped again.
/// 开启并分发一个唤醒回合；已开启却无法分发的回合会被再次停止。
async fn attempt_shell_job_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let _dispatch_owner =
        crate::agent_collaboration::dispatch_owner::DispatchOwner::acquire(state, session_id)?;
    if !root_session_can_wake(state, session_id)? {
        return Ok(false);
    }
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(state, session_id).await?;
    let Some(start) = open_shell_job_continuation_turn(state, &handle).await? else {
        return Ok(false);
    };
    webui_trace(
        "shell_job_continuation_opened",
        format!("session={session_id} turn={}", start.turn_id.value()),
    );
    let authority =
        shell_job_dispatch_authority_from_v1_handle(handle.as_ref())?.filter(|authority| {
            authority.turn_id == start.turn_id && authority.context_seq == start.context_seq
        });
    let dispatched = match authority {
        Some(authority) => {
            crate::webui_prompt_lifecycle::maybe_prepare_webui_shell_job_continuation(
                app,
                state,
                controller_runtime,
                agent_runtime,
                session_id,
                authority,
            )
            .await
        }
        None => Ok(false),
    };
    if !matches!(dispatched, Ok(true)) {
        // The notices stay pending for the next request of whichever turn runs next.
        // 通知保持待送达，交给下一个运行回合的下一次请求。
        stop_undispatched_shell_job_continuation_turn(&handle, start.turn_id).await?;
    }
    webui_trace(
        "shell_job_continuation_dispatched",
        format!("session={session_id} result={dispatched:?}"),
    );
    dispatched
}

/// Returns whether the service owns `session_id` as an idle, listed root session.
/// 返回服务是否将 `session_id` 作为空闲且已列出的根会话持有。
///
/// Child sessions are never listed and keep their notices for their own next request.
/// 子会话永不列出，其通知保留到它们自己的下一次请求。
fn root_session_can_wake(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<bool, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(root_session_idle_in(&guard, session_id))
}

/// Lock-held form of [`root_session_can_wake`].
/// [`root_session_can_wake`] 的持锁形式。
fn root_session_idle_in(state: &ServiceBridgeState, session_id: SessionId) -> bool {
    let Some(workspace) = state.workspace_snapshot.as_ref() else {
        return false;
    };
    workspace
        .sessions
        .iter()
        .any(|session| session.id == session_id)
        && !state.active_stream_tasks.contains_key(&session_id)
        && !session_has_pending_plan_artifact(workspace, session_id)
}

/// Opens one wake turn under turn-admission exclusion when notices are pending and the session is idle.
/// 当通知待送达且会话空闲时，在回合准入互斥下开启一个唤醒回合。
///
/// # Returns
/// 返回值
/// The opened turn, or `None` when nothing is pending or another owner holds the session.
/// 返回开启的回合；没有待送达通知或会话被其他所有者持有时返回 `None`。
pub(crate) async fn open_shell_job_continuation_turn(
    state: &SharedServiceBridgeState,
    handle: &Arc<SessionHandle>,
) -> Result<Option<ShellJobContinuationStart>, String> {
    let session_id = handle.session_id();
    if pending_notice_count(session_id) == 0 {
        return Ok(None);
    }
    // The exclusion also rejects an active run or kernel turn and keeps submissions out meanwhile.
    // 该互斥同时拒绝活动运行或内核回合，并在此期间阻止新的提交。
    let Some(_admission) =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            state, session_id,
        )?
    else {
        return Ok(None);
    };
    let now_ms = current_v1_unix_millis()?;
    Arc::clone(handle)
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::ContextChanged,
            move |document| {
                if !document.shell_job_continuation_admissible() {
                    return Ok(SessionMutationChange::Unchanged(None));
                }
                document
                    .start_shell_job_continuation_turn(SHELL_JOB_CONTINUATION_CONTENT, now_ms)
                    .map(|start| SessionMutationChange::Changed(Some(start)))
            },
        )
        .await
        .map(|receipt| receipt.value)
        .map_err(|error| error.to_string())
}

/// Stops `turn_id` when it is still an undispatched wake turn; anything else is left alone.
/// 当 `turn_id` 仍是未分发的唤醒回合时将其停止；其他情况不做改动。
async fn stop_undispatched_shell_job_continuation_turn(
    handle: &Arc<SessionHandle>,
    turn_id: TurnId,
) -> Result<(), String> {
    let now_ms = current_v1_unix_millis()?;
    Arc::clone(handle)
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::TurnCompleted,
            move |document| {
                if !document.shell_job_continuation_undispatched(turn_id) {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                document
                    .stop_undispatched_shell_job_continuation_turn(turn_id, now_ms)
                    .map(|stopped| {
                        if stopped {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        }
                    })
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Wakes the owning session whenever a background job finishes with an undelivered notice.
/// 每当后台任务结束且带有未送达通知时唤醒所属会话。
pub(crate) fn install_shell_job_continuation_wake(
    runtime: &tokio::runtime::Handle,
    app: ServiceEventEmitter,
    state: Weak<Mutex<ServiceBridgeState>>,
) {
    let mut events = ShellJobRuntime::global().subscribe_events();
    runtime.spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                // Missed events only delay the wake to the next finish or the next turn end.
                // 漏掉的事件只会把唤醒推迟到下一次结束或下一个回合结束。
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if event.kind != ShellJobEventKind::Finished {
                continue;
            }
            let Some(state) = state.upgrade() else {
                break;
            };
            let Ok(session_id) = event.owner_session_id.parse::<SessionId>() else {
                continue;
            };
            if pending_notice_count(session_id) == 0 {
                continue;
            }
            let app = app.clone();
            crate::spawn_service_task(async move {
                let (controller_runtime, agent_runtime) =
                    match crate::runtime_workspace_helpers::runtime_parts_from_state(&state) {
                        Ok((controller_runtime, agent_runtime, ..)) => {
                            (controller_runtime, agent_runtime)
                        }
                        Err(error) => {
                            tracing::warn!(%session_id, %error, "Shell-job wake skipped");
                            return;
                        }
                    };
                if let Err(error) = maybe_start_shell_job_continuation_if_idle(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                )
                .await
                {
                    tracing::warn!(%session_id, %error, "Shell-job wake failed");
                }
            });
        }
    });
}

#[cfg(test)]
mod tests;

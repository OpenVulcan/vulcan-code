//! Retryable parent terminal processing with immutable assistant evidence.
//! 使用不可变助手凭据处理可重试父终态。

use super::*;

/// Applies the exact parent event using the original child and completed assistant receipt.
/// 使用原子会话及已完成助手回执处理精确父事件。
/// Service owners authorize publication and continuation; returns an error without discarding retry ownership.
/// 服务所有者授权发布与续跑；返回错误时不丢弃重试归属。
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(super) async fn handle_parent_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    task_id: SessionId,
    execution_id: &str,
    child_handle: &SessionHandle,
    parent_session_id: SessionId,
    event: RuntimeEvent,
    completed_assistant: Option<&ProviderV1CompletedAssistantText>,
) -> Result<(), String> {
    // Recheck the live execution owner at each attempt, never trusting a captured UI flag.
    // 每次尝试重查实时执行所有者，绝不信任捕获的界面标记。
    let listener_is_current = state
        .lock()
        .map_err(|error| error.to_string())?
        .detached_subagent_task_listeners
        .get(&task_id)
        .is_some_and(|current| current == execution_id);
    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
    webui_trace(
        "detached_parent_event_received",
        format!(
            "listener_task={task_id} execution_id={execution_id} parent={parent_session_id} kind={:?} is_finished={is_finished} listener_is_current={listener_is_current}",
            event.kind
        ),
    );
    let parent_state_check_started_at = current_unix_millis_value();
    let (parent_is_regular_session, parent_is_stopped) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        match guard.workspace_snapshot.as_ref() {
            Some(workspace_snapshot) => (
                workspace_snapshot_contains_regular_session(workspace_snapshot, parent_session_id),
                workspace_snapshot
                    .session_run_phases
                    .get(&parent_session_id)
                    == Some(&SessionRunPhase::Stopped),
            ),
            None => (false, false),
        }
    };
    webui_trace_if_slow(
        "detached_parent_event_state_check_slow",
        parent_state_check_started_at,
        format!(
            "listener_task={task_id} parent={parent_session_id} kind={:?}",
            event.kind
        ),
    );
    // Parse the host terminal once, before any child or parent state can be changed.
    // 在任何子或父状态改变前，一次性解析宿主终态。
    let terminal_mailbox = if listener_is_current && is_finished {
        Some(non_success::terminal_mailbox(
            &event,
            parent_session_id,
            task_id,
            execution_id,
        )?)
    } else {
        None
    };
    // Successful content and non-success proofs have separate durable publication contracts.
    // 成功正文与非成功凭据使用独立的可靠发布契约。
    let v1_return_result = if listener_is_current && is_finished {
        // The parsed terminal is already bound to this listener and its two session owners.
        // 已解析终态已经绑定此监听器及其双方会话归属。
        let mailbox = terminal_mailbox
            .as_ref()
            .ok_or("Child terminal feedback is missing.")?;
        Some(
            if mailbox.kind == vulcan_session_boundary::SubagentMailboxEventKind::Completed {
                append_exact_v1_subagent_return(
                    state,
                    parent_session_id,
                    task_id,
                    execution_id,
                    child_handle,
                    completed_assistant,
                )
                .map(Some)
            } else {
                // Resolve the already registered canonical parent without a service lock across I/O.
                // 解析已经登记的规范父句柄，不让服务锁跨越 I/O。
                let registry = state
                    .lock()
                    .map_err(|error| error.to_string())?
                    .session_runtime_registry
                    .clone();
                // Keep the registered owner alive through its durable result commit.
                // 在可靠结果提交期间保持已登记所有者存活。
                let parent = registry
                    .get(parent_session_id)
                    .map_err(|error| error.to_string())?;
                non_success::publish_non_success(
                    &parent,
                    child_handle,
                    execution_id,
                    mailbox,
                    current_v1_unix_millis()?,
                )
                .map(|()| None)
            },
        )
    } else {
        None
    };
    if let Some(Ok(_)) = &v1_return_result {
        // Publish the unchanged host feedback only after its canonical result commit.
        // 仅在规范结果提交后，原样发布宿主反馈。
        let mailbox = terminal_mailbox
            .clone()
            .ok_or("Child terminal feedback is missing.")?;
        agent_runtime
            .task_runtime
            .publish_durable_task_return(execution_id, mailbox)?;
    }
    if let Some(Err(error)) = &v1_return_result {
        // Keep the exact listener and pending return gate for recovery; storage errors are not child outcomes.
        // 保留精确监听器及待回流门以便恢复；存储错误不属于子任务结果。
        return Err(format!("Child terminal publication failed: {error}"));
    }
    let closed_by_runtime = listener_is_current
        && is_finished
        && agent_runtime
            .task_runtime
            .subagent_registry_snapshot()?
            .list_all()
            .iter()
            .any(|entry| {
                entry.session_id.as_uuid() == task_id
                    && entry.status == vulcan_session_core::SubagentStatus::Archived
            });
    let mut question_return_settled = true;
    if closed_by_runtime
        || terminal_mailbox.as_ref().is_some_and(|mailbox| {
            mailbox.kind == vulcan_session_boundary::SubagentMailboxEventKind::Stopped
        })
    {
        // Question failure delivery is independent of the child's business result publication.
        // 问题失败回传独立于子任务业务结果发布。
        if let Err(error) =
            crate::agent_collaboration::reconcile_stopped_question_deliveries_for_session(
                state, task_id,
            )
            .await
        {
            question_return_settled = false;
            tracing::warn!(%task_id, %error, "Stopped child question return remains pending");
        }
    }
    let consumed = if listener_is_current {
        match ingest_webui_subagent_runtime_event_for_listener_execution(
            app,
            state,
            agent_runtime,
            parent_session_id,
            &event,
            Some(execution_id),
        ) {
            Ok(consumed) => consumed,
            Err(error) => {
                tracing::warn!("[webui-service] ingest subagent parent event failed: {error}");
                false
            }
        }
    } else {
        true
    };
    webui_trace(
        "detached_parent_event_ingested",
        format!(
            "listener_task={task_id} parent={parent_session_id} kind={:?} consumed={consumed}",
            event.kind
        ),
    );
    if !consumed && !is_finished {
        append_runtime_event_projection_and_release_barrier(app, state, parent_session_id, &event)?;
    } else {
        release_runtime_event_projection_barrier_after_attempt(&event)?;
    }
    if is_finished {
        // Only durably published terminals reach continuation selection and listener cleanup.
        // 只有可靠发布的终态才能进入续跑选择与监听器清理。
        let v1_return_receipt = v1_return_result.transpose()?;
        if let Some(receipt) = v1_return_receipt.flatten()
            && parent_is_regular_session
            && !parent_is_stopped
        {
            let started = start_exact_v1_subagent_return_turn(
                app,
                state,
                controller_runtime,
                agent_runtime,
                parent_session_id,
                receipt,
            )
            .await?;
            webui_trace(
                "detached_parent_event_v1_return_dispatch",
                format!("parent={parent_session_id} task={task_id} started={started}"),
            );
            if !started {
                webui_trace(
                    "detached_parent_event_v1_return_deferred_to_parent_terminal",
                    format!("parent={parent_session_id} task={task_id}"),
                );
            }
        } else if terminal_mailbox.as_ref().is_some_and(|mailbox| {
            mailbox.kind != vulcan_session_boundary::SubagentMailboxEventKind::Completed
        }) && parent_is_regular_session
            && !parent_is_stopped
        {
            crate::webui_turn_chaining::start_exact_v1_collaboration_continuation(
                app,
                state,
                controller_runtime,
                agent_runtime,
                parent_session_id,
            )
            .await?;
        }
        let cleaned_current_listener = {
            let mut guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            let cleaned = clear_webui_detached_listener_execution_if_current(
                &mut guard,
                task_id,
                execution_id,
            );
            if !cleaned && listener_is_current {
                clear_webui_subagent_completion_gate_ignored(&mut guard, task_id, execution_id);
            }
            cleaned
        };
        if cleaned_current_listener {
            if closed_by_runtime
                && question_return_settled
                && let Err(error) =
                    crate::agent_collaboration::release_closed_child_handle(state, task_id).await
            {
                tracing::warn!(%task_id, %error, "Closed child V1 handle remains registered");
            }
            emit_webui_subagent_navigation_changed_delta_for_parent(app, state, parent_session_id)?;
        }
    }
    if is_finished && parent_is_regular_session && parent_is_stopped {
        webui_trace(
            "detached_parent_event_v1_return_preserved_while_stopped",
            format!("parent={parent_session_id} task={task_id}"),
        );
    }
    Ok(())
}

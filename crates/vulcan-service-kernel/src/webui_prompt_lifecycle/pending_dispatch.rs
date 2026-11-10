//! WebUI prompt lifecycle pending dispatch helpers.
//! WebUI 提示词生命周期 pending dispatch 辅助逻辑。

use super::*;
use crate::notify_webui_wait_agent_parent_interruption_if_waiting;
use crate::pending_prompt_services::{
    pending_prompt_action_id, pending_prompt_attachment_workspace_path,
    pending_prompt_queue_identity, rebuild_pending_prompt_panel_from_v1,
    reconcile_pending_prompt_attachment_leases,
};
use crate::session_goal_lifecycle::{current_v1_goal_unix_millis, session_v1_handle_from_state};
use vulcan_workspace_store::workspace_store::load_project_resource_catalog;

// STOPPED_INSERTION_ERROR explains why the exact queue entry remains available.
// STOPPED_INSERTION_ERROR 说明精确队列条目为何保持可用。
const STOPPED_INSERTION_ERROR: &str = "The conversation is stopped. Continue it before inserting a message; the message remains queued.";

/// Inserts one visible queued prompt at a future boundary of the current business execution.
/// 将可见排队提示词插入当前业务执行的后续边界。
/// A manual summary retains the queue entry until successful settlement authorizes dispatch.
/// 手动摘要期间保留排队条目，直到成功结算授权分发。
///
/// # Parameters
/// 参数
/// `app` emits pending-prompt and dispatch deltas.
/// `app` 发出待发送队列与分发增量。
/// `state` owns retained service state and runtime handles.
/// `state` 持有保留的服务状态与运行时句柄。
/// `entry_id` selects the visible queue row to insert.
/// `entry_id` 选择要插入的可见队列行。
///
/// # Returns
/// 返回值
/// `true` when the prompt was accepted for insertion.
/// 当提示词已被接受插入时返回 `true`。
pub(crate) async fn service_insert_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    // session_id freezes the selection used by the existing queue-row action.
    // session_id 固定现有队列行操作使用的会话选择。
    let session_id = selected_session_id_from_state(&state)?;
    insert_pending_prompt_for_session(app, state, session_id, entry_id).await
}

/// Inserts the exact queued entry in its owning session without consulting the selected conversation.
/// 在精确队列项的所属会话中执行插队，无需读取当前选中会话。
/// `app` emits deltas, `state` holds the runtime, and `session_id` and `entry_id` identify the owner and action.
/// `app` 发出增量，`state` 持有运行时，`session_id` 和 `entry_id` 标识所属会话和操作。
/// Returns true on acceptance, including deferred insertion during manual compaction.
/// 接受插队时返回 true，包括手动压缩期间延后插入的情况。
pub(crate) async fn insert_pending_prompt_for_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_id: SessionId,
    entry_id: String,
) -> Result<bool, String> {
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    {
        // Hold the same service lock as compaction settlement so an accepted request cannot miss the handoff.
        // 持有与压缩结算相同的服务锁，使已接收请求不会错过交接。
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        if guard
            .session_runtime_kernels
            .get(&session_id)
            .and_then(|kernel| kernel.active_turn())
            .is_some_and(|turn| turn.kind == crate::ActiveTurnKind::ContextCompaction)
            && let Some(task) = guard.active_stream_tasks.get_mut(&session_id)
        {
            // Check current queue membership rather than an action identity recovered from terminal history.
            // 校验当前队列成员资格，而不是仅依赖从终态历史恢复的动作身份。
            let (queued, stopped) = handle
                .read(|document| {
                    (document
                        .metadata
                        .input_state
                        .queued_inputs
                        .iter()
                        .any(|item| {
                            item.input_id == identity.input_id
                                && item.client_action_id == identity.client_action_id
                        }), !matches!(document.collaboration_run_control(),
                            vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
                            | vulcan_session_core::v1::collaboration::CollaborationRunControl::NaturallyIdle))
                })
                .map_err(|error| error.to_string())?;
            if stopped {
                return Err(STOPPED_INSERTION_ERROR.to_owned());
            }
            if !queued {
                return Err(String::from(
                    "The message is no longer queued for insertion.",
                ));
            }
            if !task.deferred_compaction_insertions.contains(&identity) {
                task.deferred_compaction_insertions.push(identity);
            }
            return Ok(true);
        }
    }
    let attachment_workspace_path = pending_prompt_attachment_workspace_path(&state, session_id)?;
    handle
        .promote_queued_input_to_pending_turn(identity, current_v1_goal_unix_millis()?)
        .map_err(|error| match error {
            crate::session_runtime_registry_v1::SessionRuntimeError::Domain(ref cause)
                if matches!(
                    cause.as_ref(),
                    vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                        field: "pending_input.stopped_turn",
                    }
                ) =>
            {
                STOPPED_INSERTION_ERROR.to_owned()
            }
            error => {
                format!("Failed to insert the V1 pending message into the current turn: {error}")
            }
        })?;
    // Wake waiters only after Core accepts the input; rejection must leave runtime work untouched.
    // Core 接受输入后才唤醒等待者；拒绝准入不能改变运行时工作。
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        notify_webui_wait_agent_parent_interruption_if_waiting(
            &guard,
            session_id,
            "user_interrupt_prompt",
        )?;
    }
    // An interjected message moves a running foreground command to the background so the model can answer;
    // queueing a message, without inserting it, leaves the command in the foreground.
    // 插队的消息会把运行中的前台命令转到后台，让模型能够回复；仅排队而不插入的消息不影响前台命令。
    vulcan_host_tools::shell_job::ShellJobRuntime::global().notify_interjection(
        &vulcan_agent_runtime::tooling::native::canonical_shell_job_session_id(
            &session_id.to_string(),
        ),
    );
    // The V1 promotion is already durable; later panel failures cannot retain its queue lease.
    // V1 提升已可靠提交；后续面板故障不能保留该队列租约。
    reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        session_id,
        &attachment_workspace_path,
    )
    .map_err(|error| format!("The pending message was promoted into the V1 current turn, but attachment cleanup is pending a retry: {error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("Failed to read the V1 session resource location: {error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("Failed to load the V1 project resource catalog: {error}"))?;
    rebuild_pending_prompt_panel_from_v1(
        &state,
        session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, session_id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::{TestHostRuntimeRoot, test_service_state_with_parent_session};
    use std::sync::{Arc, Mutex};
    use vulcan_session_core::v1::{
        ClientActionId, ClientInputSubmission, CollaborationOperationId, CollaborationRunControl,
        SetRunControlInput,
    };

    /// Verifies that a stopped turn retains its queued input and that a resumed turn accepts its exact RPC target.
    /// 验证停止回合保留排队消息，恢复后通过 RPC 精确目标接收插队。
    /// Takes no parameters and returns after both durable outcomes are verified.
    /// 无参数，验证两个可靠结果后返回。
    #[tokio::test]
    async fn stopped_pending_insertion_preserves_queued_input() {
        let session_id = SessionId::new_v4();
        let root = TestHostRuntimeRoot::new(session_id);
        let mut service = test_service_state_with_parent_session(session_id);
        service.controller_runtime = Some(root.attached_host_runtime_handle().await);
        service.workbench.selected_conversation_id = Some(session_id.to_string());
        service
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .task_projects
            .push(vulcan_workbench_model::TaskProjectSummary::new(
                "project-a",
                "Fixture",
                ".",
                false,
                0,
                None,
                None,
                Vec::new(),
            ));
        let state = Arc::new(Mutex::new(service));
        let handle = session_v1_handle_from_state(&state, session_id)
            .await
            .unwrap();
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("stopped-insert-root").unwrap(),
                    "Original business",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        handle
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stopped-insert-control").unwrap(),
                state: CollaborationRunControl::MainAndSubagents,
                changed_at_ms: 3,
            })
            .unwrap();
        let action = ClientActionId::try_new("stopped-insert-message").unwrap();
        handle
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), "Keep queued", Vec::new()),
                4,
            )
            .unwrap();
        pending_prompt_attachment_workspace_path(&state, session_id).unwrap();
        let before = handle
            .read_full_durable(|document| {
                serde_json::to_value((&document.metadata, document.events())).unwrap()
            })
            .unwrap();
        let result = service_insert_pending_prompt(
            ServiceEventEmitter::new(Vec::new()),
            state.clone(),
            action.as_str().to_owned(),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            handle
                .read_full_durable(|document| {
                    serde_json::to_value((&document.metadata, document.events())).unwrap()
                })
                .unwrap(),
            before
        );
        assert_eq!(state.lock().unwrap().workbench_generation, 0);
        assert!(state.lock().unwrap().active_stream_tasks.is_empty());

        handle
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("resume-before-shortcut-insert")
                    .unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: 5,
            })
            .unwrap();
        // shortcut_action is the newly submitted item; the older queued item must retain its place.
        // shortcut_action 是新提交的消息，较早的排队消息必须保留原有位置。
        let shortcut_action = ClientActionId::try_new("shortcut-insert-message").unwrap();
        handle
            .enqueue_input(
                ClientInputSubmission::new(shortcut_action.clone(), "Insert this", Vec::new()),
                6,
            )
            .unwrap();
        state.lock().unwrap().workbench.selected_conversation_id = None;
        // inserted records the explicit RPC response after the user has left the target conversation.
        // inserted 记录用户离开目标会话后，显式 RPC 返回的结果。
        let inserted = crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            state,
            String::from("service_insert_pending_prompt"),
            Some(serde_json::json!({
                "conversationId": session_id,
                "entryId": shortcut_action.as_str(),
            })),
        )
        .await
        .unwrap();
        assert_eq!(inserted, serde_json::Value::Bool(true));
        handle
            .read_full_durable(|document| {
                // input_state verifies the exact new message was promoted and the older one retained.
                // input_state 用于验证精确的新消息已插入，较早的消息仍留在队列中。
                let input_state = &document.metadata.input_state;
                assert_eq!(input_state.queued_inputs.len(), 1);
                assert!(
                    input_state
                        .queued_inputs
                        .iter()
                        .any(|input| input.client_action_id == action)
                );
                assert_eq!(input_state.pending_in_turn_inputs.len(), 1);
                assert!(
                    input_state
                        .pending_in_turn_inputs
                        .iter()
                        .any(|input| input.client_action_id == shortcut_action)
                );
            })
            .unwrap();
    }
}

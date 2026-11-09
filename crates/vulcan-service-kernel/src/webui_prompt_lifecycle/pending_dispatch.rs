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

/// Inserts one visible queued prompt at a future boundary of the current business execution.
/// 将可见排队提示词插入当前业务执行的后续边界。
/// This does not resume a stopped conversation or start a new model request.
/// 此操作不恢复已停止会话，也不启动新的模型请求。
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
    let selected_session_id = selected_session_id_from_state(&state)?;
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    let attachment_workspace_path =
        pending_prompt_attachment_workspace_path(&state, selected_session_id)?;
    handle
        .promote_queued_input_to_pending_turn(identity, current_v1_goal_unix_millis()?)
        .map_err(|error| match error {
            crate::session_runtime_registry_v1::SessionRuntimeError::Domain(ref cause)
                if matches!(cause.as_ref(),
                    vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                        field: "pending_input.stopped_turn",
                    }) => String::from(
                "The conversation is stopped. Continue it before inserting a message; the message remains queued.",
            ),
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
            selected_session_id,
            "user_interrupt_prompt",
        )?;
    }
    // An interjected message moves a running foreground command to the background so the model can answer;
    // queueing a message, without inserting it, leaves the command in the foreground.
    // 插队的消息会把运行中的前台命令转到后台，让模型能够回复；仅排队而不插入的消息不影响前台命令。
    vulcan_host_tools::shell_job::ShellJobRuntime::global().notify_interjection(
        &vulcan_agent_runtime::tooling::native::canonical_shell_job_session_id(
            &selected_session_id.to_string(),
        ),
    );
    // The V1 promotion is already durable; later panel failures cannot retain its queue lease.
    // V1 提升已可靠提交；后续面板故障不能保留该队列租约。
    reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        selected_session_id,
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
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
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

    /// A stopped insertion RPC keeps the durable message and never claims to have delivered it.
    /// 已停止会话的插队 RPC 保留可靠消息，绝不声称已经投递。
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
    }
}

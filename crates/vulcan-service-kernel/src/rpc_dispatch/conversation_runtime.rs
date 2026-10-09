//! Conversation runtime RPC routing.
//! 会话运行时 RPC 路由。

use super::*;
/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_submit_conversation_prompt",
    "service_continue_conversation",
    "service_cancel_conversation_submission",
    "service_plan_composer_attachments",
    "service_discard_composer_attachment_plans",
    "service_prepare_composer_attachment",
    "service_stop_conversation_run",
    "service_stop_command_session",
    "service_get_command_session_snapshot",
    "service_refresh_command_sessions",
    "service_attach_command_session_display",
    "service_detach_command_session_display",
    "service_send_command_session_input",
    "service_resize_command_session",
    "service_list_project_terminals",
    "service_create_project_terminal",
    "service_send_project_terminal_input",
    "service_resize_project_terminal",
    "service_rename_project_terminal",
    "service_stop_project_terminal",
    "service_close_project_terminal",
    "service_attach_project_terminal_display",
    "service_detach_project_terminal_display",
    "service_save_session_goal",
    "service_resume_session_goal",
    "service_clear_session_goal",
    "service_update_session_goal_status",
    "service_set_session_plan_mode",
    "service_set_session_answer_mode",
    "service_set_session_vmm_policy",
    "service_set_session_context_compaction_window",
    "service_set_session_output_style",
    "service_compact_selected_conversation_context",
    "service_reply_pending_question",
    "service_reject_pending_question",
    "service_open_subagent_panel",
    "service_stop_subagent_task",
    "service_close_subagent_task",
    "service_edit_pending_prompt",
    "service_insert_pending_prompt",
    "service_remove_pending_prompt",
    "service_reorder_pending_prompts",
    "service_stage_composer_attachment_paths",
    "service_stage_composer_attachment_bytes",
    "service_stage_composer_attachment_preview_bytes",
    "service_reconcile_composer_attachment_draft",
];

/// Dispatches one RPC method owned by this domain.
/// 分发本领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `state` owns service data, `app` emits frontend effects, `method` is the exact wire name, and `params` is the normalized object.
/// `state` 持有服务数据，`app` 发出前端副作用，`method` 是精确线协议名称，`params` 是归一化对象。
///
/// # Returns
/// 返回值
/// Returns `Handled` with the service result, `Unknown` for another domain, or a parameter parsing error.
/// 返回携带服务结果的 `Handled`、表示其他领域的 `Unknown`，或参数解析错误。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // result preserves service failures inside Handled so the top-level return trace still runs.
    // result 把服务失败保留在 Handled 内，使顶层返回追踪仍会执行。
    let result = match method {
        "service_set_session_answer_mode" => rpc_json_result(
            crate::answer_mode::service_set_session_answer_mode(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_submit_conversation_prompt" => rpc_json_result(
            service_submit_conversation_prompt_rpc(
                app,
                state,
                rpc_required_param(params, "clientSubmissionId")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "draftSlotKey")?,
                rpc_required_param(params, "prompt")?,
                rpc_optional_param(params, "hiddenModelGuidance")?,
                rpc_optional_param(params, "executionMode")?,
                rpc_optional_param(params, "reasoningEffort")?,
                rpc_optional_param(params, "modelSelection")?,
                rpc_optional_param(params, "sessionSettings")?,
                rpc_optional_param(params, "attachments")?,
            )
            .await,
        ),
        "service_continue_conversation" => rpc_json_result(
            service_continue_conversation(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
                rpc_required_param(params, "clientContinuationId")?,
                rpc_optional_param(params, "prompt")?,
                rpc_optional_param(params, "hiddenModelGuidance")?,
                rpc_optional_param(params, "executionMode")?,
                rpc_optional_param(params, "reasoningEffort")?,
                rpc_optional_param(params, "modelSelection")?,
                rpc_optional_param(params, "attachments")?,
            )
            .await,
        ),
        "service_cancel_conversation_submission" => {
            // The browser's draft lease barrier requires settled upload ownership, not merely a sent cancellation signal.
            // 浏览器草稿租约屏障要求上传所有权已结算，而非仅发送取消信号。
            let client_submission_id = rpc_required_param::<String>(params, "clientSubmissionId")?;
            rpc_json_result(cancel_media_upload(client_submission_id.as_str()).await)
        }
        "service_plan_composer_attachments" => rpc_json_result(
            service_plan_composer_attachments(
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "draftSlotKey")?,
                rpc_required_param(params, "generation")?,
                rpc_required_param(params, "modelSelection")?,
                rpc_required_param(params, "attachments")?,
                rpc_optional_param(params, "retryUploads")?.unwrap_or(false),
                rpc_required_param(params, "compressImages")?,
            )
            .await,
        ),
        "service_discard_composer_attachment_plans" => {
            rpc_json_result(crate::media_upload::discard_composer_attachment_plans(
                rpc_required_param(params, "transferIds")?,
            ))
        }
        "service_prepare_composer_attachment" => rpc_json_result(
            service_prepare_composer_attachment(
                app,
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "modelSelection")?,
                rpc_required_param(params, "attachment")?,
                rpc_required_param(params, "preparation")?,
            )
            .await,
        ),
        "service_stop_conversation_run" => rpc_json_result(
            service_stop_conversation_run(
                app,
                state,
                rpc_required_param(params, "conversationId")?,
                rpc_optional_param(params, "mode")?,
            )
            .await,
        ),
        "service_stop_command_session" => rpc_json_result(
            service_stop_command_session(
                app,
                state,
                rpc_required_param(params, "commandSessionId")?,
                rpc_optional_param(params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_get_command_session_snapshot" => rpc_json_result(
            service_get_command_session_snapshot(
                state,
                rpc_required_param(params, "commandSessionId")?,
                rpc_optional_param(params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_refresh_command_sessions" => {
            rpc_json_result(service_refresh_command_sessions(app, state))
        }
        "service_attach_command_session_display" => rpc_json_result(
            service_attach_command_session_display(
                app,
                state,
                rpc_required_param(params, "commandSessionId")?,
                rpc_optional_param(params, "ownerSessionId")?,
                rpc_required_param(params, "displayClientId")?,
            )
            .await,
        ),
        "service_detach_command_session_display" => rpc_json_result(
            service_detach_command_session_display(rpc_required_param(params, "subscriptionId")?),
        ),
        "service_send_command_session_input" => rpc_json_result(
            service_send_command_session_input(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_resize_command_session" => rpc_json_result(
            service_resize_command_session(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_list_project_terminals" => rpc_json_result(
            service_list_project_terminals(state, rpc_required_param(params, "projectId")?).await,
        ),
        "service_create_project_terminal" => rpc_json_result(
            service_create_project_terminal(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_send_project_terminal_input" => rpc_json_result(
            service_send_project_terminal_input(rpc_required_param(params, "request")?).await,
        ),
        "service_resize_project_terminal" => rpc_json_result(
            service_resize_project_terminal(rpc_required_param(params, "request")?).await,
        ),
        "service_rename_project_terminal" => rpc_json_result(
            service_rename_project_terminal(state, rpc_required_param(params, "request")?).await,
        ),
        "service_stop_project_terminal" => rpc_json_result(
            service_stop_project_terminal(
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "terminalId")?,
            )
            .await,
        ),
        "service_close_project_terminal" => rpc_json_result(
            service_close_project_terminal(
                app,
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "terminalId")?,
            )
            .await,
        ),
        "service_attach_project_terminal_display" => rpc_json_result(
            service_attach_project_terminal_display(
                app,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "terminalId")?,
                rpc_required_param(params, "displayClientId")?,
            )
            .await,
        ),
        "service_detach_project_terminal_display" => rpc_json_result(
            service_detach_project_terminal_display(rpc_required_param(params, "subscriptionId")?),
        ),
        "service_save_session_goal" => rpc_json_result(
            service_save_session_goal(app, state, rpc_required_param(params, "request")?).await,
        ),
        "service_resume_session_goal" => rpc_json_result(
            service_resume_session_goal(app, state, rpc_required_param(params, "request")?).await,
        ),
        "service_clear_session_goal" => rpc_json_result(
            service_clear_session_goal(app, state, rpc_required_param(params, "request")?).await,
        ),
        "service_update_session_goal_status" => rpc_json_result(
            service_update_session_goal_status(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_set_session_plan_mode" => rpc_json_result(
            service_set_session_plan_mode(app, state, rpc_required_param(params, "request")?).await,
        ),
        "service_set_session_vmm_policy" => rpc_json_result(
            service_set_session_vmm_policy(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_set_session_context_compaction_window" => rpc_json_result(
            service_set_session_context_compaction_window(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_set_session_output_style" => rpc_json_result(
            service_set_session_output_style(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_compact_selected_conversation_context" => {
            rpc_json_result(service_compact_selected_conversation_context(app, state).await)
        }
        "service_reply_pending_question" => rpc_json_result(service_reply_pending_question(
            state,
            rpc_required_param(params, "requestId")?,
            rpc_required_param(params, "answers")?,
        )),
        "service_reject_pending_question" => rpc_json_result(service_reject_pending_question(
            state,
            rpc_required_param(params, "requestId")?,
        )),
        "service_open_subagent_panel" => rpc_json_result(
            service_open_subagent_panel(
                state,
                rpc_required_param(params, "parentConversationId")?,
                rpc_required_param(params, "taskId")?,
            )
            .await,
        ),
        "service_stop_subagent_task" => rpc_json_result(
            service_stop_subagent_task(
                app,
                state,
                rpc_required_param(params, "parentConversationId")?,
                rpc_required_param(params, "taskId")?,
            )
            .await,
        ),
        "service_close_subagent_task" => rpc_json_result(
            service_close_subagent_task(
                app,
                state,
                rpc_required_param(params, "parentConversationId")?,
                rpc_required_param(params, "taskId")?,
            )
            .await,
        ),
        "service_edit_pending_prompt" => rpc_json_result(
            service_edit_pending_prompt(app, state, rpc_required_param(params, "entryId")?).await,
        ),
        "service_insert_pending_prompt" => rpc_json_result(
            service_insert_pending_prompt(app, state, rpc_required_param(params, "entryId")?).await,
        ),
        "service_remove_pending_prompt" => rpc_json_result(
            service_remove_pending_prompt(app, state, rpc_required_param(params, "entryId")?).await,
        ),
        "service_reorder_pending_prompts" => rpc_json_result(
            service_reorder_pending_prompts(
                app,
                state,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "draggedEntryId")?,
                rpc_optional_param(params, "targetEntryId")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_paths" => rpc_json_result(
            service_stage_composer_attachment_paths(
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "paths")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_bytes" => rpc_json_result(
            service_stage_composer_attachment_bytes(
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "fileName")?,
                rpc_required_param(params, "mimeType")?,
                rpc_required_param(params, "dataBase64")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_preview_bytes" => rpc_json_result(
            service_stage_composer_attachment_preview_bytes(
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "attachmentId")?,
                rpc_required_param(params, "dataBase64")?,
            )
            .await,
        ),
        "service_reconcile_composer_attachment_draft" => rpc_json_result(
            service_reconcile_composer_attachment_draft(
                state,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "draftSlotKey")?,
                rpc_required_param(params, "attachments")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

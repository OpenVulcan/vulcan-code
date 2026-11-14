//! Conversation runtime RPC routing.
//! 会话运行时 RPC 路由。

use super::*;
use crate::media_upload::{classify_dropped_composer_files, stage_dropped_composer_file};
/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_submit_conversation_prompt",
    "service_continue_conversation",
    "service_cancel_conversation_submission",
    "service_plan_composer_attachments",
    "service_classify_dropped_composer_files",
    crate::media_upload::STAGE_DROPPED_COMPOSER_FILE_RPC,
    "service_discard_composer_attachment_plans",
    "service_prepare_composer_attachment",
    "service_model_attachment_capabilities",
    "service_stop_conversation_run",
    "service_stop_command_session",
    "service_get_command_session_snapshot",
    "service_refresh_command_sessions",
    "service_attach_command_session_display",
    "service_detach_command_session_display",
    "service_attach_shell_command_output",
    "service_detach_shell_command_output",
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
    "service_set_dedicated_memory_lock",
    "service_convert_conversation_to_assistant",
    "service_restore_assistant_conversation",
    "service_draft_dedicated_prompt",
    "service_set_session_permission_mode",
    "service_set_session_unattended",
    "service_session_unattended_default",
    "service_set_session_vmm_policy",
    "service_get_session_request_options",
    "service_set_session_request_options",
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
        "service_model_attachment_capabilities" => {
            rpc_json_result(crate::attachments::service_model_attachment_capabilities(
                state,
                rpc_required_param(params, "providerId")?,
                rpc_required_param(params, "connectionId")?,
                rpc_required_param(params, "modelId")?,
            ))
        }
        "service_set_dedicated_memory_lock" => rpc_json_result(
            crate::dedicated_session::service_set_dedicated_memory_lock(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_convert_conversation_to_assistant" => rpc_json_result(
            crate::dedicated_session::service_convert_conversation_to_assistant(
                state,
                rpc_required_param(params, "conversationId")?,
                rpc_required_param(params, "name")?,
            )
            .await,
        ),
        "service_restore_assistant_conversation" => rpc_json_result(
            crate::dedicated_session::service_restore_assistant_conversation(
                state,
                rpc_required_param(params, "conversationId")?,
            )
            .await,
        ),
        "service_draft_dedicated_prompt" => rpc_json_result(
            crate::dedicated_session::service_draft_dedicated_prompt(
                app.clone(),
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_set_session_permission_mode" => rpc_json_result(
            crate::permission_mode::service_set_session_permission_mode(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_set_session_unattended" => rpc_json_result(
            crate::permission_mode::service_set_session_unattended(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_session_unattended_default" => {
            rpc_json_result(crate::permission_judge::service_session_unattended_default(
                state,
                rpc_required_param(params, "request")?,
            ))
        }
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
                rpc_optional_param(params, "droppedPlanId")?,
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
                rpc_optional_param(params, "queuedInputActionId")?,
                rpc_optional_param(params, "draftSlotKey")?,
                rpc_optional_param(params, "droppedPlanId")?,
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
        "service_classify_dropped_composer_files" => {
            // Decode only file descriptors so route preflight never consumes uploaded bytes.
            // 仅解析文件描述，确保路线预检不接收上传字节。
            rpc_json_result(
                classify_dropped_composer_files(&state, rpc_required_param(params, "request")?)
                    .await,
            )
        }
        crate::media_upload::STAGE_DROPPED_COMPOSER_FILE_RPC => {
            let plan_id = rpc_required_param(params, "planId")?;
            let index = rpc_required_param(params, "index")?;
            let data_base64 = rpc_required_param(params, "dataBase64")?;
            rpc_json_result(
                crate::spawn_service_blocking_task(move || {
                    stage_dropped_composer_file(&state, plan_id, index, data_base64)
                })
                .await
                .map_err(|error| error.to_string())?,
            )
        }
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
        "service_attach_shell_command_output" => {
            rpc_json_result(service_attach_shell_command_output(
                app,
                rpc_required_param(params, "sessionId")?,
                rpc_required_param(params, "apiCallId")?,
                rpc_required_param(params, "toolCallId")?,
                rpc_required_param(params, "displayClientId")?,
            ))
        }
        "service_detach_shell_command_output" => rpc_json_result(
            service_detach_shell_command_output(rpc_required_param(params, "subscriptionId")?),
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
        "service_get_session_request_options" => rpc_json_result(
            service_get_session_request_options(state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_set_session_request_options" => rpc_json_result(
            service_set_session_request_options(state, rpc_required_param(params, "request")?)
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
        "service_insert_pending_prompt" => {
            // entry_id comes from the queue row or the acknowledged submission's client action id.
            // entry_id 来自队列行或已确认提交的客户端动作标识。
            let entry_id = rpc_required_param(params, "entryId")?;
            rpc_json_result(
                if let Some(session_id) = rpc_optional_param(params, "conversationId")? {
                    crate::webui_prompt_lifecycle::insert_pending_prompt_for_session(
                        app, state, session_id, entry_id,
                    )
                    .await
                } else {
                    service_insert_pending_prompt(app, state, entry_id).await
                },
            )
        }
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

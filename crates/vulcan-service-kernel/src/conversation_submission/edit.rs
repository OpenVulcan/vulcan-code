//! User prompt edit submission helpers.
//! 用户提示词编辑提交辅助逻辑。

use super::*;

/// Applies a user prompt edit and resubmits the edited prompt into the conversation.
/// 应用用户提示词编辑，并将编辑后的提示词重新提交到会话。
///
/// # Parameters
/// 参数
/// `app` is the service event emitter used by downstream submission updates.
/// `app` 是下游提交流程用于发送更新的服务事件发射器。
/// `state` owns the shared service bridge state and retained workbench snapshot.
/// `state` 持有共享服务桥状态与保留工作台快照。
/// `request` contains the edit target, replacement prompt, and attachment summaries.
/// `request` 包含编辑目标、替换提示词与附件摘要。
///
/// # Returns
/// 返回值
/// Returns a lightweight acknowledgement after the edited prompt is resubmitted.
/// 返回编辑后的提示词重新提交后的轻量确认信息。
pub(crate) async fn service_submit_user_prompt_edit(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: UserPromptEditSubmitRequest,
) -> Result<SubmitConversationPromptAck, String> {
    if request.stop_running {
        return Err(String::from("ERR_USER_PROMPT_EDIT_SESSION_BUSY"));
    }
    let session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_INVALID_CONVERSATION"))?;
    request
        .attachments
        .iter()
        .try_for_each(validate_composer_attachment_summary)?;
    let controller_runtime = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        if !webui_session_allows_user_prompt_editing(&mut guard, session_id) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_SESSION_BUSY"));
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        if !workspace_user_prompt_message_allows_destructive_edit(
            workspace,
            session_id,
            &request.message_id,
        ) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_EDITABLE"));
        }
        controller_runtime
    };
    let rollback = rollback_persisted_session_before_user_prompt(
        &controller_runtime,
        session_id,
        &request.message_id,
    )
    .await?;
    let message_patch_event = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let conversation_id = session_id.to_string();
        let before_conversation_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        workspace
            .session_messages
            .insert(session_id, rollback.retained_messages.clone());
        workspace
            .session_runtime_state
            .insert(session_id, rollback.runtime_state);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Ready);
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Completed;
            session.focus = String::new();
            session.last_updated_label = String::new();
            session.updated_at = current_unix_millis_text();
        }
        let retained_conversation_messages = conversation_messages_from_session(
            &conversation_id,
            rollback.retained_messages.as_slice(),
            false,
        );
        let message_patch = build_conversation_messages_patched_delta(
            &conversation_id,
            before_conversation_messages.as_slice(),
            retained_conversation_messages.as_slice(),
        );
        replace_conversation_messages_in_workbench_window(
            &mut guard.workbench.messages,
            &conversation_id,
            retained_conversation_messages,
        );
        guard.workbench = workbench_with_live_pending_question(&guard);
        message_patch.map(|message_patch| {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            }
        })
    };
    if let Some(event) = message_patch_event {
        emit_service_event_with_trace(
            &app,
            "workbench_user_prompt_edit_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    service_submit_conversation_prompt(
        app,
        state,
        None,
        Some(request.conversation_id),
        request.prompt,
        None,
        Some(String::from("build")),
        None,
        Some(request.attachments),
        None,
        None,
        None,
    )
    .await
}

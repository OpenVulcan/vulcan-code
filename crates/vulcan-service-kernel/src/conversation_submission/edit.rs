//! User prompt edit submission helpers.
//! 用户提示词编辑提交辅助逻辑。

use super::*;
use crate::{ServiceBridgeState, owning_project_id_for_session};
use vulcan_service_api::{ConversationMessage, ConversationSummary};
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionRunPhase, SessionRuntimeStateEntry, SessionSummary,
};

/// Session-scoped in-memory state captured before applying a destructive prompt edit.
/// 应用破坏性提示词编辑前捕获的会话级内存状态。
#[derive(Clone)]
struct UserPromptEditWorkspaceBackup {
    /// Original retained transcript messages.
    /// 原始保留转录消息。
    session_messages: Option<Vec<SessionMessageEntry>>,
    /// Original canonical runtime state.
    /// 原始规范运行态。
    runtime_state: Option<SessionRuntimeStateEntry>,
    /// Original visible run phase.
    /// 原始可见运行阶段。
    run_phase: Option<SessionRunPhase>,
    /// Original session summary.
    /// 原始会话摘要。
    session: SessionSummary,
    /// Whether the session was marked dialogue-ready.
    /// 会话是否被标记为对话就绪。
    hydrated: bool,
    /// Original projected workbench messages for this conversation.
    /// 此会话原始投影后的工作台消息。
    conversation_messages: Vec<ConversationMessage>,
    /// Original projected workbench conversation summary.
    /// 原始投影后的工作台会话摘要。
    conversation_summary: Option<ConversationSummary>,
}

impl UserPromptEditWorkspaceBackup {
    /// Captures the exact session-scoped state that destructive editing may replace.
    /// 捕获破坏性编辑可能替换的精确会话级状态。
    ///
    /// # Parameters
    /// 参数
    /// `guard` is the locked service state before edit projection.
    /// `guard` 是编辑投影前已加锁的服务状态。
    /// `session_id` identifies the edited session.
    /// `session_id` 标识被编辑的会话。
    /// `session` is the original authoritative session summary.
    /// `session` 是原始权威会话摘要。
    ///
    /// # Returns
    /// 返回值
    /// Returns a restorable session-scoped snapshot.
    /// 返回可恢复的会话级快照。
    fn capture(
        guard: &ServiceBridgeState,
        session_id: SessionId,
        session: SessionSummary,
    ) -> Result<Self, String> {
        let conversation_id = session_id.to_string();
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        Ok(Self {
            session_messages: workspace.session_messages.get(&session_id).cloned(),
            runtime_state: workspace.session_runtime_state.get(&session_id).cloned(),
            run_phase: workspace.session_run_phases.get(&session_id).copied(),
            session,
            hydrated: workspace.hydrated_session_ids.contains(&session_id),
            conversation_messages: guard
                .workbench
                .messages
                .iter()
                .filter(|message| message.conversation_id == conversation_id)
                .cloned()
                .collect(),
            conversation_summary: guard
                .workbench
                .conversations
                .iter()
                .find(|conversation| conversation.id == conversation_id)
                .cloned(),
        })
    }

    /// Restores the captured session state without overwriting unrelated live sessions.
    /// 恢复已捕获的会话状态，同时不覆盖其他无关实时会话。
    ///
    /// # Parameters
    /// 参数
    /// `guard` is the locked service state receiving the restoration.
    /// `guard` 是接收恢复内容的已加锁服务状态。
    /// `session_id` identifies the edited session.
    /// `session_id` 标识被编辑的会话。
    ///
    /// # Returns
    /// 返回值
    /// Returns ordered workbench events for the restored transcript and conversation summary.
    /// 返回用于恢复后转录与会话摘要的有序工作台事件。
    fn restore(
        &self,
        guard: &mut ServiceBridgeState,
        session_id: SessionId,
    ) -> Result<Vec<WorkbenchDeltaEnvelope>, String> {
        let conversation_id = session_id.to_string();
        let current_conversation_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let current_conversation_summary = guard
            .workbench
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .cloned();
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        match &self.session_messages {
            Some(messages) => {
                workspace
                    .session_messages
                    .insert(session_id, messages.clone());
            }
            None => {
                workspace.session_messages.remove(&session_id);
            }
        }
        match &self.runtime_state {
            Some(runtime_state) => {
                workspace
                    .session_runtime_state
                    .insert(session_id, runtime_state.clone());
            }
            None => {
                workspace.session_runtime_state.remove(&session_id);
            }
        }
        match self.run_phase {
            Some(run_phase) => {
                workspace.session_run_phases.insert(session_id, run_phase);
            }
            None => {
                workspace.session_run_phases.remove(&session_id);
            }
        }
        if self.hydrated {
            workspace.hydrated_session_ids.insert(session_id);
        } else {
            workspace.hydrated_session_ids.remove(&session_id);
        }
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            *session = self.session.clone();
        }
        let message_patch = build_conversation_messages_patched_delta(
            &conversation_id,
            current_conversation_messages.as_slice(),
            self.conversation_messages.as_slice(),
        );
        replace_conversation_messages_in_workbench_window(
            &mut guard.workbench.messages,
            &conversation_id,
            self.conversation_messages.clone(),
        );
        if let Some(original_summary) = &self.conversation_summary
            && let Some(conversation) = guard
                .workbench
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == conversation_id)
        {
            *conversation = original_summary.clone();
        }
        if let Ok(workbench) = workbench_with_live_pending_question(guard) {
            guard.workbench = workbench;
        }
        let mut events = Vec::new();
        if let Some(message_patch) = message_patch {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            });
        }
        if let Some(original_summary) = &self.conversation_summary
            && current_conversation_summary.as_ref().is_none_or(|current| {
                current.run_phase != original_summary.run_phase
                    || current.status != original_summary.status
                    || current.last_updated_label != original_summary.last_updated_label
            })
        {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::UpdateConversationRunPhase(
                    UpdateConversationRunPhaseDelta {
                        conversation_id,
                        run_phase: original_summary.run_phase.clone(),
                        status: Some(original_summary.status.clone()),
                        last_updated_label: Some(original_summary.last_updated_label.clone()),
                    },
                ),
            });
        }
        Ok(events)
    }
}

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
    // edit_admission_guard excludes every competing turn from validation through rollback or committed replay.
    // edit_admission_guard 从校验开始排除所有竞争回合，直至回滚恢复或重放提交完成。
    let edit_admission_guard =
        begin_existing_prompt_submission_exclusion(&state, session_id, "编辑提示词")?;
    // edit_submission_authorization lets only this transaction's nested replay pass normal admission.
    // edit_submission_authorization 仅允许当前事务的嵌套重放通过普通准入。
    let edit_submission_authorization =
        edit_admission_guard.existing_submission_authorization(session_id)?;
    let (controller_runtime, vmm_controller_registry, source_session, project_id) = {
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
        let source_session = workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .cloned()
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_SESSION_NOT_FOUND"))?;
        let project_id = owning_project_id_for_session(workspace, session_id)
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_PROJECT_NOT_FOUND"))?;
        (
            controller_runtime,
            guard.vmm_controller_registry.clone(),
            source_session,
            project_id,
        )
    };
    // rewrite_fence blocks stale and replacement controllers until disk and retained state agree.
    // rewrite_fence 在磁盘与保留状态一致前阻止旧控制器与替代控制器。
    let rewrite_fence = vmm_controller_registry.begin_lifecycle_transition([session_id])?;
    let rollback = rollback_persisted_session_before_user_prompt(
        &controller_runtime,
        &source_session,
        &project_id,
        &request.message_id,
    )
    .await?;
    let (message_patch_event, workspace_backup) = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let conversation_id = session_id.to_string();
        let workspace_backup =
            UserPromptEditWorkspaceBackup::capture(&guard, session_id, source_session.clone())?;
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
            .insert(session_id, rollback.runtime_state.clone());
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
        match workbench_with_live_pending_question(&guard) {
            Ok(workbench) => {
                guard.workbench = workbench;
            }
            Err(error) => {
                return match workspace_backup.restore(&mut guard, session_id) {
                    Ok(_) => Err(error),
                    Err(restore_error) => Err(format!(
                        "{error}; ERR_USER_PROMPT_EDIT_WORKSPACE_RESTORE_FAILED: {restore_error}"
                    )),
                };
            }
        }
        let message_patch_event = message_patch.map(|message_patch| {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
            }
        });
        (message_patch_event, workspace_backup)
    };
    if let Some(event) = message_patch_event {
        emit_service_event_with_trace(
            &app,
            "workbench_user_prompt_edit_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    rewrite_fence.release()?;
    let submission_result = service_submit_conversation_prompt(
        app.clone(),
        state.clone(),
        None,
        Some(request.conversation_id),
        request.prompt,
        None,
        Some(String::from("build")),
        None,
        None,
        None,
        Some(request.attachments),
        Some(edit_submission_authorization),
        ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
    )
    .await;
    match submission_result {
        Ok(acknowledgement) => {
            if let Err(error) = rollback.commit() {
                webui_submit_trace(
                    "user_prompt_edit_backup_cleanup_failed",
                    format!("session={session_id} error={error}"),
                );
            }
            Ok(acknowledgement)
        }
        Err(submission_error) => {
            // restore_fence prevents the failed replacement controller from racing old-history restoration.
            // restore_fence 防止失败的替代控制器与旧历史恢复发生竞争。
            let restore_fence = vmm_controller_registry.begin_lifecycle_transition([session_id])?;
            let disk_restore_result = rollback.restore();
            let workspace_restore_result = state
                .lock()
                .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))
                .and_then(|mut guard| workspace_backup.restore(&mut guard, session_id));
            if let Ok(events) = workspace_restore_result.as_ref() {
                for event in events {
                    emit_service_event_with_trace(
                        &app,
                        "workbench_user_prompt_edit_restore_messages_patch_emit",
                        ServiceEvent::WorkbenchDelta(Box::new(event.clone())),
                    );
                }
            }
            let fence_release_result = restore_fence.release();
            match (
                disk_restore_result,
                workspace_restore_result,
                fence_release_result,
            ) {
                (Ok(()), Ok(_), Ok(())) => Err(submission_error),
                (disk_result, workspace_result, fence_result) => Err(format!(
                    "{submission_error}; ERR_USER_PROMPT_EDIT_RESTORE_FAILED: disk={}; workspace={}; vmm_fence={}",
                    disk_result.err().unwrap_or_else(|| String::from("ok")),
                    workspace_result.err().unwrap_or_else(|| String::from("ok")),
                    fence_result.err().unwrap_or_else(|| String::from("ok"))
                )),
            }
        }
    }
}

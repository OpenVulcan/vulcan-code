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

/// Moves one conversation's projected messages out of the retained workbench without cloning bodies.
/// 将一个会话的投影消息从保留 Workbench 中移出，且不复制正文。
///
/// # Parameters
/// 参数
/// `messages` owns the mixed retained Workbench message list.
/// `messages` 持有混合的保留 Workbench 消息列表。
/// `conversation_id` identifies the sole conversation whose rows are extracted.
/// `conversation_id` 标识唯一需要提取记录的会话。
///
/// # Returns
/// 返回值
/// The extracted rows in original order while unrelated rows remain in `messages` in original order.
/// 按原顺序返回提取记录，同时 `messages` 中无关记录保持原顺序。
fn take_workbench_conversation_messages(
    messages: &mut Vec<ConversationMessage>,
    conversation_id: &str,
) -> Vec<ConversationMessage> {
    // retained_messages preserves unrelated row order during ownership partitioning.
    // retained_messages 在所有权分区期间保留无关记录顺序。
    let mut retained_messages = Vec::with_capacity(messages.len());
    // conversation_messages receives only rows owned by the edited conversation.
    // conversation_messages 只接收被编辑会话拥有的记录。
    let mut conversation_messages = Vec::new();
    for message in std::mem::take(messages) {
        if message.conversation_id == conversation_id {
            conversation_messages.push(message);
        } else {
            retained_messages.push(message);
        }
    }
    *messages = retained_messages;
    conversation_messages
}

/// Preserves the live composer model selection across a historical prompt rollback.
/// 在历史提示词回滚期间保留输入区当前生效的模型选择。
///
/// # Parameters
/// 参数
/// `live_runtime_state` is the session state after the user's latest provider, account, model, or protocol selection.
/// `live_runtime_state` 是用户最近选择供应商、账号、模型或协议后的会话状态。
/// `replay_runtime_state` is the historical state restored immediately before the edited prompt.
/// `replay_runtime_state` 是在被编辑提示词之前恢复出的历史状态。
#[cfg(test)]
fn preserve_live_model_selection_for_prompt_replay(
    live_runtime_state: &SessionRuntimeStateEntry,
    replay_runtime_state: &mut SessionRuntimeStateEntry,
) {
    replay_runtime_state.active_provider_id = live_runtime_state.active_provider_id.clone();
    replay_runtime_state.active_connection_id = live_runtime_state.active_connection_id.clone();
    replay_runtime_state.active_model_id = live_runtime_state.active_model_id.clone();
    replay_runtime_state.pending_model_selection =
        live_runtime_state.pending_model_selection.clone();
    replay_runtime_state.provider_route_lease = live_runtime_state.provider_route_lease.clone();
}

/// Builds the explicit model-selection snapshot used by an edited prompt replay.
/// 构建编辑提示词重放使用的显式模型选择快照。
///
/// # Parameters
/// 参数
/// `live_runtime_state` is the authoritative composer selection captured before rollback.
/// `live_runtime_state` 是回滚前捕获的权威输入区选择。
///
/// # Returns
/// 返回值
/// The complete selection, no selection when all fields are absent, or an explicit partial-state error.
/// 返回完整选择；三个字段均缺失时返回无选择；部分缺失时返回显式状态错误。
fn prompt_replay_model_selection(
    live_runtime_state: &SessionRuntimeStateEntry,
) -> Result<Option<SubmissionModelSelection>, String> {
    if let Some(pending_selection) = live_runtime_state.pending_model_selection.as_ref() {
        // An edited prompt is a real user submission, so it must promote the exact pending picker choice.
        // 编辑后的提示词属于真实用户提交，因此必须提升精确的待生效选择器选择。
        return Ok(Some(SubmissionModelSelection {
            provider_id: pending_selection.provider_id.clone(),
            connection_id: pending_selection.connection_id.clone(),
            model_id: pending_selection.model_id.clone(),
        }));
    }
    match (
        live_runtime_state.active_provider_id.as_ref(),
        live_runtime_state.active_connection_id.as_ref(),
        live_runtime_state.active_model_id.as_ref(),
    ) {
        (Some(provider_id), Some(connection_id), Some(model_id)) => {
            Ok(Some(SubmissionModelSelection {
                provider_id: provider_id.clone(),
                connection_id: connection_id.clone(),
                model_id: model_id.clone(),
            }))
        }
        (None, None, None) if live_runtime_state.provider_route_lease.is_none() => Ok(None),
        _ => Err(String::from(
            "ERR_USER_PROMPT_EDIT_MODEL_SELECTION_INCOMPLETE",
        )),
    }
}

impl UserPromptEditWorkspaceBackup {
    /// Takes ownership of the exact session-scoped state that destructive editing will replace.
    /// 取得破坏性编辑将替换的精确会话级状态所有权。
    ///
    /// # Parameters
    /// 参数
    /// `guard` is the locked mutable service state before edit projection.
    /// `guard` 是编辑投影前已加锁的可变服务状态。
    /// `session_id` identifies the edited session.
    /// `session_id` 标识被编辑的会话。
    /// `session` is the original authoritative session summary.
    /// `session` 是原始权威会话摘要。
    ///
    /// # Returns
    /// 返回值
    /// Returns a restorable ownership backup without cloning transcript or canonical history.
    /// 返回不复制转录或规范历史的可恢复所有权备份。
    fn capture(
        guard: &mut ServiceBridgeState,
        session_id: SessionId,
        session: SessionSummary,
    ) -> Result<Self, String> {
        // conversation_id selects projected rows before ownership is partitioned.
        // conversation_id 在所有权分区前选择投影记录。
        let conversation_id = session_id.to_string();
        // session_backup_fields move the heavy exact-session maps before Workbench rows are partitioned.
        // session_backup_fields 在 Workbench 记录分区前移动精确会话的重映射字段。
        let (session_messages, runtime_state, run_phase, hydrated) = {
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
            (
                workspace.session_messages.remove(&session_id),
                workspace.session_runtime_state.remove(&session_id),
                workspace.session_run_phases.remove(&session_id),
                workspace.hydrated_session_ids.contains(&session_id),
            )
        };
        // conversation_messages moves only the edited conversation's projected rows.
        // conversation_messages 只移动被编辑会话的投影记录。
        let conversation_messages =
            take_workbench_conversation_messages(&mut guard.workbench.messages, &conversation_id);
        // conversation_summary is small identity/status metadata retained for rollback projection.
        // conversation_summary 是为回滚投影保留的小型身份与状态元数据。
        let conversation_summary = guard
            .workbench
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .cloned();
        Ok(Self {
            session_messages,
            runtime_state,
            run_phase,
            session,
            hydrated,
            conversation_messages,
            conversation_summary,
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
        self,
        guard: &mut ServiceBridgeState,
        session_id: SessionId,
    ) -> Result<Vec<WorkbenchDeltaEnvelope>, String> {
        // backup fields are destructured so restoration moves every heavy value exactly once.
        // backup 字段被解构，使恢复过程只移动每个重值一次。
        let Self {
            session_messages,
            runtime_state,
            run_phase,
            session,
            hydrated,
            conversation_messages,
            conversation_summary,
        } = self;
        // conversation_id selects the current replacement rows being rolled back.
        // conversation_id 选择正在回滚的当前替代记录。
        let conversation_id = session_id.to_string();
        // current_conversation_messages moves failed replacement rows out before the original rows return.
        // current_conversation_messages 在原始记录恢复前移出失败的替代记录。
        let current_conversation_messages =
            take_workbench_conversation_messages(&mut guard.workbench.messages, &conversation_id);
        // current_conversation_summary is small status metadata used to decide whether a phase delta is required.
        // current_conversation_summary 是用于判断是否需要阶段增量的小型状态元数据。
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
        match session_messages {
            Some(messages) => {
                workspace.session_messages.insert(session_id, messages);
            }
            None => {
                workspace.session_messages.remove(&session_id);
            }
        }
        match runtime_state {
            Some(runtime_state) => {
                workspace
                    .session_runtime_state
                    .insert(session_id, runtime_state);
            }
            None => {
                workspace.session_runtime_state.remove(&session_id);
            }
        }
        match run_phase {
            Some(run_phase) => {
                workspace.session_run_phases.insert(session_id, run_phase);
            }
            None => {
                workspace.session_run_phases.remove(&session_id);
            }
        }
        if hydrated {
            workspace.hydrated_session_ids.insert(session_id);
        } else {
            workspace.hydrated_session_ids.remove(&session_id);
        }
        if let Some(retained_session) = workspace
            .sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
        {
            *retained_session = session;
        }
        let message_patch = build_conversation_messages_patched_delta(
            &conversation_id,
            current_conversation_messages.as_slice(),
            conversation_messages.as_slice(),
        );
        replace_conversation_messages_in_workbench_window(
            &mut guard.workbench.messages,
            &conversation_id,
            conversation_messages,
        );
        if let Some(original_summary) = &conversation_summary
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
        if let Some(original_summary) = &conversation_summary
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
    let (
        controller_runtime,
        vmm_controller_registry,
        source_session,
        project_id,
        live_runtime_state,
        visible_user_prompt_prefix,
        replay_model_selection,
    ) = {
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
        // liveRuntimeState freezes the composer route before any destructive disk rollback starts.
        // liveRuntimeState 在任何破坏性磁盘回滚开始前冻结输入区路由。
        let live_runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_RUNTIME_STATE_NOT_FOUND"))?;
        // visibleUserPromptPrefix retains only the hydrated display prefix through the validated edit target.
        // visibleUserPromptPrefix 仅保留截至已校验编辑目标的水合展示前缀。
        let visible_user_prompt_prefix = workspace
            .session_messages
            .get(&session_id)
            .and_then(|messages| {
                messages
                    .iter()
                    .position(|message| message.id == request.message_id)
                    .map(|target_index| messages[..=target_index].to_vec())
            })
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"))?;
        let replay_model_selection = prompt_replay_model_selection(&live_runtime_state)?;
        (
            controller_runtime,
            guard.vmm_controller_registry.clone(),
            source_session,
            project_id,
            live_runtime_state,
            visible_user_prompt_prefix,
            replay_model_selection,
        )
    };
    // rewrite_fence blocks stale and replacement controllers until disk and retained state agree.
    // rewrite_fence 在磁盘与保留状态一致前阻止旧控制器与替代控制器。
    let rewrite_fence = vmm_controller_registry.begin_lifecycle_transition([session_id])?;
    let mut rollback = rollback_persisted_session_before_user_prompt(
        &controller_runtime,
        &source_session,
        &project_id,
        &request.message_id,
        live_runtime_state,
        visible_user_prompt_prefix.as_slice(),
    )
    .await?;
    let (retained_messages, replay_runtime_state) = rollback.take_rewritten_state();
    let (message_patch_event, workspace_backup) = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let conversation_id = session_id.to_string();
        // workspace_backup moves old heavy history out before the rewritten state is installed.
        // workspace_backup 在安装重写状态前移出旧的重历史。
        let workspace_backup =
            UserPromptEditWorkspaceBackup::capture(&mut guard, session_id, source_session.clone())?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        let retained_conversation_messages = conversation_messages_from_session(
            &conversation_id,
            retained_messages.as_slice(),
            false,
        );
        workspace
            .session_messages
            .insert(session_id, retained_messages);
        workspace
            .session_runtime_state
            .insert(session_id, replay_runtime_state);
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
        let message_patch = build_conversation_messages_patched_delta(
            &conversation_id,
            workspace_backup.conversation_messages.as_slice(),
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
        Uuid::new_v4().to_string(),
        None,
        Some(request.conversation_id),
        None,
        request.prompt,
        None,
        Some(String::from("build")),
        None,
        replay_model_selection,
        None,
        Some(request.attachments),
        Some(edit_submission_authorization),
        ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ConversationSubmissionMode::UserPrompt,
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

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_workbench_model::SessionProviderRouteLeaseIdentity;

    /// Verifies an edited replay keeps the newly selected model and does not revive its historical protocol lease.
    /// 验证编辑重放保留新选择的模型，且不会复活历史协议租约。
    #[test]
    fn prompt_replay_preserves_live_model_selection_and_existing_lease() {
        // liveRuntimeState represents the active composer target with its established hot lease.
        // liveRuntimeState 表示输入区当前目标及其已建立的热租约。
        let live_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("custom:new-provider")),
            active_connection_id: Some(String::from("connection-new")),
            active_model_id: Some(String::from("model-new")),
            pending_model_selection: Some(vulcan_workbench_model::SessionPendingModelSelection {
                provider_id: String::from("command-code"),
                connection_id: String::from("command-code-account"),
                model_id: String::from("deepseek"),
            }),
            provider_route_lease: Some(SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("custom:new-provider"),
                connection_id: String::from("connection-new"),
                model_id: String::from("model-new"),
                protocol_profile_id: String::from("google.gemini.generate_content"),
                manifest_revision: 4,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-live"),
                route_revision: String::from("route-live-old"),
            }),
            ..SessionRuntimeStateEntry::default()
        };
        // replayRuntimeState represents the historical route restored from before the edited prompt.
        // replayRuntimeState 表示从被编辑提示词之前恢复出的历史路由。
        let mut replay_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("provider-old")),
            active_connection_id: Some(String::from("connection-old")),
            active_model_id: Some(String::from("model-old")),
            provider_route_lease: Some(SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("provider-old"),
                connection_id: String::from("connection-old"),
                model_id: String::from("model-old"),
                protocol_profile_id: String::from("openai-chat-completions.v1"),
                manifest_revision: 1,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-old"),
                route_revision: String::from("route-old"),
            }),
            ..SessionRuntimeStateEntry::default()
        };

        preserve_live_model_selection_for_prompt_replay(
            &live_runtime_state,
            &mut replay_runtime_state,
        );

        assert_eq!(
            replay_runtime_state.active_provider_id.as_deref(),
            Some("custom:new-provider")
        );
        assert_eq!(
            replay_runtime_state.active_connection_id.as_deref(),
            Some("connection-new")
        );
        assert_eq!(
            replay_runtime_state.active_model_id.as_deref(),
            Some("model-new")
        );
        assert_eq!(
            replay_runtime_state.provider_route_lease,
            live_runtime_state.provider_route_lease
        );
        assert_eq!(
            replay_runtime_state.pending_model_selection,
            live_runtime_state.pending_model_selection
        );
        assert_eq!(
            prompt_replay_model_selection(&live_runtime_state)
                .expect("complete live selection should project"),
            Some(SubmissionModelSelection {
                provider_id: String::from("command-code"),
                connection_id: String::from("command-code-account"),
                model_id: String::from("deepseek"),
            })
        );
    }
}

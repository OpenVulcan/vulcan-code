//! Conversation action service handlers.
//! 会话操作服务处理器。

use super::*;

/// Submits answers for the oldest visible ask-tool request and resumes the blocked tool call.
/// 提交当前可见 ask 工具请求的答案，并恢复被阻塞的工具调用。
pub(crate) fn service_reply_pending_question(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request_id: String,
    answers: Vec<Vec<String>>,
) -> Result<bool, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    let answered = agent_runtime
        .question_runtime
        .reply(&request_id, answers)
        .map_err(|error| format!("failed to submit ask answers: {error}"))?;
    if !answered {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    emit_pending_question_changed_delta(&app, &state)?;
    Ok(true)
}

/// Rejects the visible ask-tool request after explicit user confirmation.
/// 在用户明确确认后拒绝当前可见 ask 工具请求。
pub(crate) fn service_reject_pending_question(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request_id: String,
) -> Result<bool, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    let rejected = agent_runtime
        .question_runtime
        .reject(&request_id)
        .map_err(|error| format!("failed to reject ask request: {error}"))?;
    if !rejected {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    emit_pending_question_changed_delta(&app, &state)?;
    Ok(true)
}

/// Loads one backend-validated draft for a user prompt edit dialog.
/// 加载一份供用户提示词编辑弹窗使用的后端校验草稿。
pub(crate) async fn service_get_user_prompt_edit_draft(
    state: SharedServiceBridgeState,
    conversation_id: String,
    message_id: String,
) -> Result<UserPromptEditDraftSummary, String> {
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_INVALID_CONVERSATION"))?;
    let (controller_runtime, source_session, project_id) = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
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
            &message_id,
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
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        (controller_runtime, source_session, project_id)
    };
    let target = load_persisted_user_prompt_edit_draft(
        &controller_runtime,
        &source_session,
        &project_id,
        &message_id,
    )
    .await?;
    Ok(UserPromptEditDraftSummary {
        conversation_id,
        message_id,
        prompt: target.prompt,
        attachments: target
            .attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        running: false,
    })
}

/// Submits one user prompt edit by rolling back the tail and immediately replaying the edited prompt.
/// 通过回滚尾部并立即重放编辑后提示词来提交一次用户提示词编辑。
/// Forks one persisted conversation into a new idle session and opens the clone.
/// 将一个持久化会话分叉为新的空闲会话并打开克隆。
pub(crate) async fn service_fork_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ConversationForkRequest,
) -> Result<WorkbenchSnapshot, String> {
    let fork_total_started_at = current_unix_millis_value();
    let parse_started_at = current_unix_millis_value();
    let source_session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_INVALID_CONVERSATION"))?;
    webui_fork_trace(
        "service.parse_request",
        parse_started_at,
        format!(
            "source_session={} anchor={}",
            source_session_id,
            request.anchor_message_id.as_deref().unwrap_or("<latest>")
        ),
    );
    let state_lookup_started_at = current_unix_millis_value();
    let (controller_runtime, source_session, project_id) = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        if !webui_session_allows_user_prompt_editing(&mut guard, source_session_id) {
            return Err(String::from("ERR_SESSION_FORK_SESSION_BUSY"));
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        let source_session = workspace
            .sessions
            .iter()
            .find(|session| session.id == source_session_id)
            .cloned()
            .ok_or_else(|| String::from("ERR_SESSION_FORK_SOURCE_NOT_FOUND"))?;
        let project_id = owning_project_id_for_session(workspace, source_session_id)
            .ok_or_else(|| String::from("ERR_SESSION_FORK_PROJECT_NOT_FOUND"))?;
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        (controller_runtime, source_session, project_id)
    };
    webui_fork_trace(
        "service.resolve_source",
        state_lookup_started_at,
        format!("source_session={} project={project_id}", source_session.id),
    );
    let target_session_id = SessionId::new_v4();
    let fork_title = if request.title.trim().is_empty() {
        source_session.title.clone()
    } else {
        request.title.trim().to_string()
    };
    let persist_started_at = current_unix_millis_value();
    let fork_result = fork_persisted_session(
        &controller_runtime,
        &source_session,
        &project_id,
        target_session_id,
        fork_title,
        request.anchor_message_id.as_deref(),
    )
    .await?;
    webui_fork_trace(
        "service.persist_fork",
        persist_started_at,
        format!(
            "source_session={} target_session={} total_ms={}",
            source_session.id,
            fork_result.session.id,
            current_unix_millis_value().saturating_sub(fork_total_started_at)
        ),
    );
    let target_session_id_text = fork_result.session.id.to_string();
    let hydration_started_at = current_unix_millis_value();
    let hydrated = load_hydrated_session_workbench_for_fork_trace(
        &app,
        &state,
        fork_result.session.id,
        &target_session_id_text,
    )
    .await?;
    webui_fork_trace(
        "service.hydrate_fork",
        hydration_started_at,
        format!(
            "target_session={} total_ms={}",
            target_session_id_text,
            current_unix_millis_value().saturating_sub(fork_total_started_at)
        ),
    );
    webui_fork_trace(
        "service.total",
        fork_total_started_at,
        format!("target_session={target_session_id_text}"),
    );
    Ok(hydrated.workbench)
}

/// Emits only the ask-modal pending question projection as a workbench delta.
/// 仅以工作台增量发送 ask 弹窗待回答问题投影。
pub(crate) fn emit_pending_question_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref());
        let pending_question = pending_questions.first().cloned();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.pending_question = pending_question.clone();
        guard.workbench.pending_questions = pending_questions.clone();
        let ServiceBridgeState {
            workspace_snapshot,
            workbench,
            ..
        } = &mut *guard;
        let workspace_snapshot = workspace_snapshot.as_ref();
        for conversation in &mut workbench.conversations {
            let Ok(session_id) = conversation.id.parse::<SessionId>() else {
                continue;
            };
            let session_plan_mode = workspace_snapshot
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .and_then(|runtime_state| {
                    session_plan_mode_summary_from_runtime_state(Some(runtime_state))
                });
            refresh_conversation_attention_summary(
                conversation,
                session_id,
                &pending_questions,
                session_plan_mode.as_ref(),
            );
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::PendingQuestionChanged(PendingQuestionChangedDelta {
                conversation_id: pending_question
                    .as_ref()
                    .map(|question| question.session_id.clone()),
                pending_question,
                pending_questions,
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "pending_question_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Installs the WebUI projection callback used by the ask-question runtime.
/// 安装 ask 提问运行时使用的 WebUI 投影回调。
pub(crate) fn install_webui_question_runtime_event_bridge(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    question_runtime: &Arc<QuestionRuntime>,
) -> Result<(), String> {
    let app = app.clone();
    let state = Arc::clone(state);
    let callback: PendingQuestionChangeCallback = Arc::new(move || {
        if let Err(error) = emit_pending_question_changed_delta(&app, &state) {
            webui_trace(
                "pending_question_projection_failed",
                format!("error={error}"),
            );
        }
    });
    question_runtime.set_change_callback(Some(callback))
}

/// Returns the retained workbench with the latest live ask-runtime pending request attached.
/// 返回附加最新实时 ask 运行时待处理请求的保留工作台。
pub(crate) fn workbench_with_live_pending_question(
    guard: &ServiceBridgeState,
) -> WorkbenchSnapshot {
    let mut workbench = guard.workbench.clone();
    workbench.hydrated_conversation_windows = hydrated_conversation_window_summaries(
        &guard.hydrated_conversation_windows,
        &guard.webui_preloaded_unclosed_subagent_roots,
    );
    workbench.pending_questions =
        pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref());
    workbench.pending_question =
        workbench
            .selected_conversation_id
            .as_ref()
            .and_then(|conversation_id| {
                workbench
                    .pending_questions
                    .iter()
                    .find(|question| question.session_id == *conversation_id)
                    .cloned()
            });
    workbench
}

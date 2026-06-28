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
) -> Result<WorkbenchSnapshot, String> {
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
    store_and_emit_pending_question_workbench_snapshot(&app, &state)
}

/// Rejects the visible ask-tool request after explicit user confirmation.
/// 在用户明确确认后拒绝当前可见 ask 工具请求。
pub(crate) fn service_reject_pending_question(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request_id: String,
) -> Result<WorkbenchSnapshot, String> {
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
    store_and_emit_pending_question_workbench_snapshot(&app, &state)
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
    let controller_runtime = {
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
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?
    };
    let target =
        load_persisted_user_prompt_edit_draft(&controller_runtime, session_id, &message_id).await?;
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
    let source_session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_INVALID_CONVERSATION"))?;
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
    let target_session_id = SessionId::new_v4();
    let fork_title = if request.title.trim().is_empty() {
        source_session.title.clone()
    } else {
        request.title.trim().to_string()
    };
    let fork_result = fork_persisted_session(
        &controller_runtime,
        &source_session,
        &project_id,
        target_session_id,
        fork_title,
    )
    .await?;
    if let Some(before_message_id) = request
        .before_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        rollback_persisted_session_before_user_prompt(
            &controller_runtime,
            fork_result.session.id,
            before_message_id,
        )
        .await?;
    }
    record_persisted_session_fork_source_marker(
        &controller_runtime,
        fork_result.session.id,
        &source_session,
        &project_id,
    )?;
    let target_session_id_text = fork_result.session.id.to_string();
    load_hydrated_session_workbench(
        &app,
        &state,
        fork_result.session.id,
        &target_session_id_text,
    )
    .await
    .map(|hydrated| hydrated.workbench)
}

/// Rebuilds only the ask-modal projection on the retained workbench snapshot and emits it.
/// 仅重建保留工作台快照上的 ask 弹窗投影并发送事件。
pub(crate) fn store_and_emit_pending_question_workbench_snapshot(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_question =
            pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.pending_question = pending_question;
        guard.workbench.clone()
    };
    emit_service_event_with_trace(
        app,
        "pending_question_workbench_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
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
        if let Err(error) = store_and_emit_pending_question_workbench_snapshot(&app, &state) {
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
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() {
        workbench.hydrated_conversation_windows =
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
        append_hydrated_workspace_messages(&mut workbench.messages, workspace_snapshot);
    }
    workbench.pending_question =
        pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
    workbench
}

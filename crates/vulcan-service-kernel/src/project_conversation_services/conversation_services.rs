//! Conversation list service handlers.
//! 会话列表服务处理器。

use super::*;

/// Applies a conversation display name and returns the refreshed workbench snapshot.
/// 应用会话显示名称并返回刷新后的工作台快照。
pub(crate) async fn service_rename_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_name = normalize_non_empty_name(&name)?;
    let parsed_session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let updated_at = current_unix_millis_text();
    let (controller_runtime, project_id, session) =
        controller_runtime_and_visible_session_project_from_state(&state, parsed_session_id)?;
    vulcan_workspace_store::workspace_store::rename_single_session_metadata(
        controller_runtime,
        &project_id,
        &session,
        &normalized_name,
        &updated_at,
    )
    .await
    .map_err(|error| format!("重命名会话失败：{error}"))?;
    rename_conversation_in_retained_state(&state, parsed_session_id, &normalized_name, &updated_at)
}

/// Renames one retained visible conversation without reloading the workspace snapshot.
/// 不重新加载工作区快照，重命名一个保留的可见会话。
pub(super) fn rename_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut()
        && let Some(session) = workspace_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
    {
        session.title = title.to_string();
        session.title_source = SessionTitleSource::Manual;
        session.updated_at = updated_at.to_string();
        session.last_updated_label = String::from("刚刚");
    }
    let conversation_id = session_id.to_string();
    if let Some(conversation) = guard
        .workbench
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
    {
        conversation.title = title.to_string();
        conversation.last_updated_at = updated_at.to_string();
        conversation.last_updated_label = String::from("刚刚");
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(guard.workbench.clone())
}

/// Archives one visible conversation and returns the refreshed workbench snapshot.
/// 归档单个可见会话并返回刷新后的工作台快照。
pub(crate) async fn service_archive_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (controller_runtime, project_id, session) =
        controller_runtime_and_visible_session_project_from_state(&state, session_id)?;
    vulcan_workspace_store::workspace_store::archive_single_session_for_project(
        controller_runtime,
        &project_id,
        &session,
    )
    .await?;
    archive_conversation_in_retained_state(&state, session_id, &project_id)
}

/// Returns the controller runtime and owning project id for one retained visible session.
/// 返回单个保留可见会话的控制器运行时与所属项目标识。
pub(super) fn controller_runtime_and_visible_session_project_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(ControllerRuntimeHandle, String, SessionSummary), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法归档会话。"))?;
    let session = workspace_session_summary_for_session(workspace_snapshot, session_id)
        .cloned()
        .ok_or_else(|| format!("未找到要归档的会话：{session_id}"))?;
    let project_id = session
        .project_id
        .clone()
        .or_else(|| owning_project_id_for_session(workspace_snapshot, session_id))
        .ok_or_else(|| format!("会话缺少所属项目，无法归档：{session_id}"))?;
    Ok((controller_runtime, project_id, session))
}

/// Moves one archived conversation from retained active state into archived metadata state.
/// 将一个已归档会话从保留的活动状态移动到归档元数据状态。
pub(super) fn archive_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    project_id: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut archived_session = {
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("工作区快照尚未加载，无法更新归档状态。"))?;
        let session_index = workspace_snapshot
            .sessions
            .iter()
            .position(|candidate| candidate.id == session_id)
            .ok_or_else(|| format!("未找到要归档的会话：{session_id}"))?;
        let mut archived_session = workspace_snapshot.sessions.remove(session_index);
        if archived_session.project_id.is_none() {
            archived_session.project_id = Some(project_id.to_string());
        }
        for project in &mut workspace_snapshot.task_projects {
            project
                .session_ids
                .retain(|candidate| *candidate != session_id);
        }
        workspace_snapshot.session_messages.remove(&session_id);
        workspace_snapshot.session_compactions.remove(&session_id);
        workspace_snapshot.session_runtime_state.remove(&session_id);
        workspace_snapshot.session_run_phases.remove(&session_id);
        workspace_snapshot.hydrated_session_ids.remove(&session_id);
        workspace_snapshot
            .archived_sessions
            .retain(|candidate| candidate.id != session_id);
        workspace_snapshot
            .archived_sessions
            .push(archived_session.clone());
        workspace_snapshot
            .archived_sessions
            .sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());
        if workspace_snapshot.selected_session == session_id {
            workspace_snapshot.selected_session = workspace_snapshot
                .sessions
                .first()
                .map(|session| session.id)
                .unwrap_or_else(SessionId::nil);
        }
        archived_session
    };
    if archived_session.project_id.is_none() {
        archived_session.project_id = Some(project_id.to_string());
    }
    let archived_conversation = archived_conversation_summary_from_session(&archived_session);
    let archived_conversation_id = session_id.to_string();
    guard
        .workbench
        .conversations
        .retain(|conversation| conversation.id != archived_conversation_id);
    guard
        .workbench
        .messages
        .retain(|message| message.conversation_id != archived_conversation_id);
    guard
        .workbench
        .archived_conversations
        .retain(|conversation| conversation.id != archived_conversation_id);
    guard
        .workbench
        .archived_conversations
        .push(archived_conversation);
    guard
        .workbench
        .archived_conversations
        .sort_by_key(|conversation| {
            -conversation
                .last_updated_at
                .parse::<i64>()
                .unwrap_or_default()
        });
    if guard.workbench.selected_conversation_id.as_deref() == Some(&archived_conversation_id) {
        guard.workbench.selected_conversation_id = guard
            .workbench
            .conversations
            .first()
            .map(|conversation| conversation.id.clone());
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(guard.workbench.clone())
}

/// Builds a WebUI archived-conversation summary from persisted session metadata.
/// 根据持久化会话元数据构建 WebUI 归档会话摘要。
pub(super) fn archived_conversation_summary_from_session(
    session: &SessionSummary,
) -> ConversationSummary {
    ConversationSummary {
        id: session.id.to_string(),
        title: session.title.clone(),
        status: String::from("Archived"),
        run_phase: SessionRunPhase::Idle.to_string(),
        last_updated_label: session.last_updated_label.clone(),
        last_updated_at: session.updated_at.clone(),
        unread: false,
        attention: None,
        project_id: session_summary_project_id_owned(session),
        fork_source: conversation_fork_source_summary_from_session(session),
    }
}

/// Marks one conversation as read and returns the retained workbench snapshot.
/// 将单个会话标记为已读并返回保留的工作台快照。
pub(crate) async fn service_mark_conversation_read(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (controller_runtime, runtime_state_update, workbench) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let mut runtime_state_update = None;
        if let Some(runtime_state) =
            guard
                .workspace_snapshot
                .as_mut()
                .and_then(|workspace_snapshot| {
                    workspace_snapshot
                        .session_runtime_state
                        .get_mut(&session_id)
                })
        {
            runtime_state.ui_state.unread = false;
            runtime_state.updated_at = current_unix_millis_text();
            runtime_state_update = Some(runtime_state.clone());
        }
        for conversation in guard
            .workbench
            .conversations
            .iter_mut()
            .filter(|conversation| conversation.id == conversation_id)
        {
            conversation.unread = false;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        (
            controller_runtime,
            runtime_state_update,
            guard.workbench.clone(),
        )
    };

    if let Some(runtime_state_update) = runtime_state_update {
        vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
            controller_runtime,
            session_id,
            &runtime_state_update,
        )
        .await?;
    }
    Ok(workbench)
}

/// Selects one conversation in the retained WebUI workspace state.
/// 在 WebUI 保留的工作区状态中选中一个会话。
pub(crate) async fn service_select_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    if let Some(workbench) =
        select_conversation_in_retained_state(&state, session_id, &conversation_id)?
    {
        return Ok(workbench);
    }
    refresh_workbench_snapshot_for_selected_session(state, session_id).await
}

/// Selects one retained conversation without reading segmented files or rebuilding the full workbench.
/// 不读取分段文件且不重建完整工作台，选中一个已保留的会话。
///
/// # Parameters
/// 参数
/// `state` is the shared WebUI service state that owns the retained workspace snapshot.
/// `state` 是持有保留工作区快照的共享 WebUI 服务状态。
/// `session_id` identifies the conversation selected by the sidebar.
/// `session_id` 标识侧栏选中的会话。
/// `conversation_id` is the string form returned to the WebUI snapshot.
/// `conversation_id` 是返回给 WebUI 快照的字符串形式。
/// `subagent_surface_preload` is an optional lock-free surface preload captured before selection.
/// `subagent_surface_preload` 是选择前在锁外完成的可选界面预载结果。
///
/// # Returns
/// 返回值
/// `Some(workbench)` when the retained workspace contains the session, or `None` when a full refresh is required.
/// 当保留工作区包含该会话时返回 `Some(workbench)`；需要完整刷新时返回 `None`。
fn select_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    conversation_id: &str,
) -> Result<Option<WorkbenchSnapshot>, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(mut workspace_snapshot) = guard.workspace_snapshot.take() else {
        return Err(String::from(
            "service is still starting; workspace snapshot is not ready",
        ));
    };
    let workspace_contains_session = workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id);
    let workbench_contains_conversation = guard
        .workbench
        .conversations
        .iter()
        .any(|conversation| conversation.id == conversation_id);
    if !workspace_contains_session || !workbench_contains_conversation {
        guard.workspace_snapshot = Some(workspace_snapshot);
        return Ok(None);
    }

    workspace_snapshot.selected_session = session_id;
    guard.active_subagent_panel = None;
    retain_hydrated_conversation_windows_for_workspace(
        &mut guard.hydrated_conversation_windows,
        &workspace_snapshot,
    );
    let selected_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        restore_webui_runtime_controls_for_session(
            &mut agent_runtime.provider_configuration,
            selected_runtime_state,
        );
    }
    let messages =
        retained_selected_conversation_messages(&workspace_snapshot, session_id, conversation_id);
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot);
    let hydrated_windows = hydrated_conversation_window_summaries(
        &guard.hydrated_conversation_windows,
        &guard.webui_preloaded_unclosed_subagent_roots,
    );

    guard.workbench.selected_conversation_id = Some(conversation_id.to_string());
    guard.workbench.messages = messages;
    guard.workbench.subagent_navigation = subagent_navigation;
    guard.workbench.hydrated_conversation_windows = hydrated_windows;
    guard.workbench.conversation_hydration = guard
        .workbench
        .conversation_hydration
        .take()
        .filter(|progress| progress.session_id == conversation_id);
    sync_selected_conversation_fields_for_retained_workbench(&mut guard.workbench, conversation_id);
    guard.workspace_snapshot = Some(workspace_snapshot);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(Some(guard.workbench.clone()))
}

/// Projects only the retained message window for the selected conversation.
/// 仅投影当前选中会话已保留的消息窗口。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains retained hydrated message windows.
/// `workspace_snapshot` 包含已保留的水合消息窗口。
/// `session_id` identifies the selected root conversation.
/// `session_id` 标识选中的根会话。
/// `conversation_id` is the string identifier used by WebUI messages.
/// `conversation_id` 是 WebUI 消息使用的字符串标识。
///
/// # Returns
/// 返回值
/// A message list containing the selected hydrated body when present.
/// 返回只包含已存在选中正文的消息列表。
fn retained_selected_conversation_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    conversation_id: &str,
) -> Vec<ConversationMessage> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .map(|session_messages| {
            conversation_messages_from_session(
                conversation_id,
                session_messages,
                workspace_surface_allows_user_prompt_editing(workspace_snapshot, session_id, None),
            )
        })
        .unwrap_or_default()
}

/// Reprojects selected-only workbench fields from retained per-conversation caches.
/// 从按会话保留的缓存重新投影仅当前会话使用的工作台字段。
///
/// # Parameters
/// 参数
/// `workbench` is the retained snapshot being updated in place.
/// `workbench` 是正在原地更新的保留快照。
/// `conversation_id` identifies the newly selected conversation.
/// `conversation_id` 标识新选中的会话。
fn sync_selected_conversation_fields_for_retained_workbench(
    workbench: &mut WorkbenchSnapshot,
    conversation_id: &str,
) {
    workbench.pending_question = workbench
        .pending_questions
        .iter()
        .find(|question| question.session_id == conversation_id)
        .cloned();
    workbench.pending_prompts = workbench
        .pending_prompts_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .map(|entry| entry.pending_prompts.clone())
        .unwrap_or_default();
    let selected_addons = workbench
        .session_addons_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id);
    workbench.session_goal = selected_addons.and_then(|entry| entry.session_goal.clone());
    workbench.session_todolist = selected_addons.and_then(|entry| entry.session_todolist.clone());
    workbench.session_plan_mode = selected_addons.and_then(|entry| entry.session_plan_mode.clone());
    workbench.session_vmm_policy = workbench
        .session_vmm_policies_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .map(|entry| entry.session_vmm_policy.clone())
        .or_else(|| selected_addons.and_then(|entry| entry.session_vmm_policy.clone()));
    workbench.context_injection = workbench
        .context_injection_by_conversation
        .iter()
        .find(|entry| entry.conversation_id == conversation_id)
        .and_then(|entry| entry.context_injection.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one session summary for archived conversation projection tests.
    /// 为归档会话投影测试构建单个会话摘要。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the optional project id stored on the session.
    /// `project_id` 是写入会话的可选项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A completed session summary with the supplied project id.
    /// 返回携带给定项目标识的已完成会话摘要。
    fn archived_projection_session(project_id: Option<String>) -> SessionSummary {
        SessionSummary::new(
            SessionId::new_v4(),
            project_id,
            "归档投影测试会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )
    }

    /// Verifies archived conversation summaries do not expose blank project ids.
    /// 验证归档会话摘要不会暴露空白项目标识。
    #[test]
    fn archived_conversation_summary_ignores_blank_project_id() {
        // session stores a blank project id that should be filtered before UI projection.
        // session 保存一个投影到 UI 前应被过滤的空白项目标识。
        let session = archived_projection_session(Some(String::from("   ")));

        // summary is the WebUI sidebar projection built from the archived session.
        // summary 是从归档会话构建出的 WebUI 侧栏投影。
        let summary = archived_conversation_summary_from_session(&session);

        assert_eq!(summary.project_id, None);
    }
}

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
    let session = workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
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
        project_id: session.project_id.clone(),
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
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        mut provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        pending_questions,
        mut workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref());
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            pending_questions,
            workspace_snapshot,
        )
    };
    let exists = workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id);
    if !exists {
        return refresh_workbench_snapshot(state).await;
    }
    workspace_snapshot.selected_session = session_id;
    hydrate_selected_session_runtime_if_needed(
        controller_runtime.clone(),
        &mut workspace_snapshot,
        session_id,
    )
    .await?;
    restore_webui_runtime_controls_for_session(
        &mut provider_configuration,
        workspace_snapshot.session_runtime_state.get(&session_id),
    );
    sync_selected_session_goal_summary_from_store(&state, controller_runtime.clone(), session_id)
        .await?;
    let (
        surface_messages,
        subagent_navigation,
        session_goal_summaries,
        command_session_runtime,
        hydrated_window_summaries,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.active_subagent_panel = None;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
            agent_runtime.provider_configuration = provider_configuration.clone();
        }
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            guard.session_goal_summaries.clone(),
            command_session_runtime,
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    let mut workbench = workbench_from_runtime(
        &controller_runtime,
        command_session_runtime.as_deref(),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_questions,
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    Ok(workbench)
}

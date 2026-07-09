//! Project list service handlers.
//! 项目列表服务处理器。

use super::conversation_services::archived_conversation_summary_from_session;
use super::*;

/// Opens one project directory in the host system file manager.
/// 在宿主系统文件管理器中打开单个项目文件夹。
pub(crate) fn service_open_project_directory(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<String, String> {
    let shell_host = shell_host_from_state(&state)?;
    let project = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    shell_host.open_path(&project.path)?;
    Ok(format!("已请求打开项目目录：{}。", project.name))
}

/// Opens one arbitrary project directory path in the host system file manager.
/// 在宿主系统文件管理器中打开任意项目目录路径。
pub(crate) fn service_open_project_directory_path(
    state: SharedServiceBridgeState,
    path: String,
) -> Result<String, String> {
    shell_host_from_state(&state)?.open_path(&path)?;
    Ok(format!("已请求打开目录：{path}。"))
}

/// Returns the best initial directory for the native project folder picker.
/// 返回原生项目文件夹选择器使用的最佳初始目录。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the current workbench snapshot.
/// `state` 是持有当前工作台快照的共享服务状态。
///
/// # Returns
/// 返回值
/// A normalized directory path string suitable for the host folder picker.
/// 返回适合宿主文件夹选择器使用的标准化目录路径字符串。
pub fn service_add_project_initial_directory(
    state: &SharedServiceBridgeState,
) -> Result<String, String> {
    Ok(state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .first()
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}

/// Toggles one project pinned state without rebuilding the full workbench snapshot.
/// 切换单个项目置顶状态，但不重建完整工作台快照。
pub(crate) async fn service_toggle_project_pinned(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<(), String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;
    update_project_pinned_local_state(&state, &project_id)?;
    Ok(())
}

/// Reorders visible projects inside one pinned or regular section.
/// 在同一个置顶或普通分区内重排可见项目。
pub(crate) async fn service_reorder_projects(
    state: SharedServiceBridgeState,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    if project_ids.is_empty() {
        return refresh_workbench_snapshot(state).await;
    }

    let snapshot = service_workbench_snapshot(state.clone())?;
    let first_project = snapshot
        .projects
        .iter()
        .find(|project| project.id == project_ids[0])
        .ok_or_else(|| String::from("重排项目失败：第一个项目不存在。"))?;
    let section_pinned = first_project.pinned;

    for project_id in &project_ids {
        let Some(project) = snapshot
            .projects
            .iter()
            .find(|candidate| candidate.id == *project_id)
        else {
            return Err(format!("重排项目失败：项目不存在：{project_id}"));
        };
        if project.pinned != section_pinned {
            return Err(String::from("重排项目失败：不能跨置顶与普通分区拖拽。"));
        }
    }

    let controller_runtime = controller_runtime_from_state(&state)?;
    let updated_at = current_unix_millis_text();
    let rows = project_ids
        .into_iter()
        .enumerate()
        .map(|(index, project_id)| {
            vec![
                ControllerSqliteValue::Int64(index as i64),
                ControllerSqliteValue::String(updated_at.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect();
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            rows,
        )
        .await
        .map_err(|error| format!("重排项目失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project display name and returns the refreshed workbench snapshot.
/// 应用项目显示名称并返回刷新后的工作台快照。
pub(crate) async fn service_rename_project(
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::rename_project(controller_runtime, &project_id, &name)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project icon override without rebuilding the full workbench snapshot.
/// 应用项目图标覆盖，但不重建完整工作台快照。
pub(crate) async fn service_set_project_icon(
    state: SharedServiceBridgeState,
    project_id: String,
    icon_pack: String,
    icon_key: String,
) -> Result<(), String> {
    let normalized_icon_pack = normalize_non_empty_name(&icon_pack)?;
    let normalized_icon_key = normalize_non_empty_name(&icon_key)?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = ?1, icon_key = ?2, updated_at = ?3 WHERE id = ?4 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_icon_pack.clone()),
                ControllerSqliteValue::String(normalized_icon_key.clone()),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("设置项目图标失败：{error}"))?;
    update_project_icon_local_state(
        &state,
        &project_id,
        Some(normalized_icon_pack),
        Some(normalized_icon_key),
    )?;
    Ok(())
}

/// Clears a project icon override without rebuilding the full workbench snapshot.
/// 清除项目图标覆盖，但不重建完整工作台快照。
pub(crate) async fn service_reset_project_icon(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<(), String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = NULL, icon_key = NULL, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("还原项目图标失败：{error}"))?;
    update_project_icon_local_state(&state, &project_id, None, None)?;
    Ok(())
}

/// Persists the expanded project identifiers and returns the retained workbench snapshot.
/// 持久化展开项目标识集合并返回保留的工作台快照。
pub(crate) async fn service_set_expanded_task_projects(
    state: SharedServiceBridgeState,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, collapsed_project_ids, expanded_project_ids) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let visible_project_ids = guard
            .workbench
            .projects
            .iter()
            .map(|project| project.id.clone())
            .collect::<BTreeSet<_>>();
        let expanded_project_ids = project_ids
            .into_iter()
            .filter(|project_id| visible_project_ids.contains(project_id))
            .collect::<BTreeSet<_>>();
        let collapsed_project_ids = visible_project_ids
            .iter()
            .filter(|project_id| !expanded_project_ids.contains(*project_id))
            .cloned()
            .collect::<Vec<_>>();
        (
            controller_runtime,
            collapsed_project_ids,
            expanded_project_ids,
        )
    };
    vulcan_workspace_store::workspace_store::persist_collapsed_task_project_ids(
        controller_runtime,
        &collapsed_project_ids,
    )
    .await?;

    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot.expanded_task_projects = expanded_project_ids.clone();
    }
    guard.workbench.expanded_project_ids = guard
        .workbench
        .projects
        .iter()
        .filter(|project| expanded_project_ids.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(guard.workbench.clone())
}

/// Archives all visible conversations under one project and keeps the project active.
/// 归档单个项目下的全部可见对话，并保留项目本身。
pub(crate) async fn service_archive_project_conversations(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, _project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "归档")?;

    if project_sessions.is_empty() {
        return retained_workbench_snapshot_from_state(&state);
    }

    vulcan_workspace_store::workspace_store::archive_project_sessions(
        controller_runtime,
        &project_id,
        &project_sessions,
    )
    .await?;
    archive_project_sessions_in_retained_state(&state, &project_id, &project_sessions)
}

/// Archives all project conversations, removes the active project row, and refreshes the snapshot.
/// 归档项目全部对话、移除活动项目记录，并刷新工作台快照。
pub(crate) async fn service_remove_project(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "移除")?;
    let removal_effect = project_removal_default_project_effect(&state, &project_id, &project)?;

    vulcan_workspace_store::workspace_store::remove_project_after_archiving_sessions(
        controller_runtime.clone(),
        &project_id,
        &project_sessions,
    )
    .await?;
    if removal_effect == ProjectRemovalDefaultProjectEffect::RestoreDefault {
        vulcan_workspace_store::workspace_store::ensure_default_workspace_project(
            controller_runtime,
        )
        .await?;
    }
    refresh_workbench_snapshot(state).await
}

/// Describes whether project removal must restore the default project afterward.
/// 描述项目移除后是否必须恢复默认项目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectRemovalDefaultProjectEffect {
    /// No default-project repair is needed after removal.
    /// 移除后不需要修复默认项目。
    None,
    /// Removing this project leaves no active projects, so the default project must be restored.
    /// 移除此项目会导致没有活动项目，因此必须恢复默认项目。
    RestoreDefault,
}

/// Validates default-project removal policy and returns the needed post-removal repair effect.
/// 校验默认项目移除策略，并返回移除后需要执行的修复效果。
///
/// # Parameters
/// 参数
/// `state` owns the current retained workspace snapshot.
/// `state` 持有当前保留的工作区快照。
/// `project_id` is the active project being removed.
/// `project_id` 是正在被移除的活动项目。
/// `project` is the resolved active project summary.
/// `project` 是已解析的活动项目摘要。
///
/// # Returns
/// 返回值
/// Returns the default-project repair effect or a user-facing rejection message.
/// 返回默认项目修复效果，或面向用户的拒绝信息。
fn project_removal_default_project_effect(
    state: &SharedServiceBridgeState,
    project_id: &str,
    project: &TaskProjectSummary,
) -> Result<ProjectRemovalDefaultProjectEffect, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法处理项目。"))?;
    project_removal_default_project_effect_from_snapshot(workspace_snapshot, project_id, project)
}

/// Computes the default-project repair effect from a concrete workspace snapshot.
/// 根据具体工作区快照计算默认项目修复效果。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained snapshot containing active projects.
/// `workspace_snapshot` 是包含活动项目的保留快照。
/// `project_id` is the active project being removed.
/// `project_id` 是正在被移除的活动项目。
/// `project` is the resolved active project summary.
/// `project` 是已解析的活动项目摘要。
///
/// # Returns
/// 返回值
/// Returns the repair effect or rejects removing the only default project.
/// 返回修复效果，或拒绝移除唯一默认项目。
fn project_removal_default_project_effect_from_snapshot(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
    project: &TaskProjectSummary,
) -> Result<ProjectRemovalDefaultProjectEffect, String> {
    let remaining_active_project_count = workspace_snapshot
        .task_projects
        .iter()
        .filter(|candidate| candidate.id != project_id)
        .count();
    if remaining_active_project_count > 0 {
        return Ok(ProjectRemovalDefaultProjectEffect::None);
    }
    if vulcan_workspace_store::workspace_store::is_default_workspace_project(project)? {
        return Err(String::from("默认项目是当前唯一项目，不能移除。"));
    }
    Ok(ProjectRemovalDefaultProjectEffect::RestoreDefault)
}

/// Returns the controller runtime and one retained active project without reloading the workspace.
/// 返回控制器运行时与一个保留的活动项目，不重新加载工作区。
pub(super) fn controller_runtime_and_active_project_from_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    action_label: &str,
) -> Result<
    (
        ControllerRuntimeHandle,
        TaskProjectSummary,
        Vec<SessionSummary>,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法处理项目。"))?;
    let project = active_project_from_workspace(workspace_snapshot, project_id, action_label)?;
    let project_sessions = active_sessions_for_project_action(workspace_snapshot, &project);
    Ok((controller_runtime, project, project_sessions))
}

/// Collects active sessions that belong to one project action target.
/// 收集属于单个项目操作目标的活动会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains active session summaries and project membership.
/// `workspace_snapshot` 包含活动会话摘要与项目成员关系。
/// `project` is the active project selected by the user action.
/// `project` 是用户操作选中的活动项目。
///
/// # Returns
/// 返回值
/// Sessions listed by project membership plus sessions whose own project id points to this project.
/// 返回项目成员列出的会话，以及自身项目标识指向该项目的会话。
fn active_sessions_for_project_action(
    workspace_snapshot: &WorkspaceSnapshot,
    project: &TaskProjectSummary,
) -> Vec<SessionSummary> {
    let mut seen_session_ids = BTreeSet::new();
    let mut project_sessions = Vec::new();

    for session_id in &project.session_ids {
        let Some(session) = workspace_snapshot
            .sessions
            .iter()
            .find(|candidate| candidate.id == *session_id)
        else {
            continue;
        };
        if seen_session_ids.insert(session.id) {
            project_sessions.push(session.clone());
        }
    }

    for session in &workspace_snapshot.sessions {
        if session_summary_project_id(session) != Some(project.id.as_str()) {
            continue;
        }
        if seen_session_ids.insert(session.id) {
            project_sessions.push(session.clone());
        }
    }

    project_sessions
}

/// Resolves one active workspace project for a project-level action.
/// 为项目级操作解析单个活动工作区项目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains active project summaries.
/// `workspace_snapshot` 包含活动项目摘要。
/// `project_id` is the requested project identifier.
/// `project_id` 是请求的项目标识。
/// `action_label` labels the user-facing error.
/// `action_label` 标记面向用户的错误信息。
///
/// # Returns
/// 返回值
/// The matching active project summary.
/// 返回匹配的活动项目摘要。
fn active_project_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
    action_label: &str,
) -> Result<TaskProjectSummary, String> {
    workspace_task_project_for_project_id(workspace_snapshot, project_id)
        .cloned()
        .ok_or_else(|| format!("未找到要{action_label}的项目：{project_id}"))
}

/// Returns the retained workbench snapshot after a no-op project archive command.
/// 在项目归档命令无需改动时返回保留的工作台快照。
pub(super) fn retained_workbench_snapshot_from_state(
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard.workbench.clone())
}

/// Moves project sessions between retained active and archived metadata without reloading catalogs.
/// 在保留内存状态中迁移项目会话到归档元数据，不重新加载目录目录表。
pub(super) fn archive_project_sessions_in_retained_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    sessions: &[SessionSummary],
) -> Result<WorkbenchSnapshot, String> {
    let session_ids = sessions
        .iter()
        .map(|session| session.id)
        .collect::<BTreeSet<_>>();
    let conversation_ids = session_ids
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    let mut archived_sessions = sessions
        .iter()
        .cloned()
        .map(|mut session| {
            if session.project_id.is_none() {
                session.project_id = Some(project_id.to_string());
            }
            session
        })
        .collect::<Vec<_>>();
    archived_sessions.sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());

    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot
            .sessions
            .retain(|session| !session_ids.contains(&session.id));
        workspace_snapshot
            .archived_sessions
            .retain(|session| !session_ids.contains(&session.id));
        workspace_snapshot
            .archived_sessions
            .extend(archived_sessions.clone());
        workspace_snapshot
            .archived_sessions
            .sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());
        for project in &mut workspace_snapshot.task_projects {
            project
                .session_ids
                .retain(|session_id| !session_ids.contains(session_id));
        }
        for session_id in &session_ids {
            workspace_snapshot.session_messages.remove(session_id);
            workspace_snapshot.session_compactions.remove(session_id);
            workspace_snapshot.session_runtime_state.remove(session_id);
            workspace_snapshot.session_run_phases.remove(session_id);
            workspace_snapshot.hydrated_session_ids.remove(session_id);
        }
        if session_ids.contains(&workspace_snapshot.selected_session) {
            workspace_snapshot.selected_session = workspace_snapshot
                .sessions
                .first()
                .map(|session| session.id)
                .unwrap_or_else(SessionId::nil);
        }
    }

    guard
        .workbench
        .conversations
        .retain(|conversation| !conversation_ids.contains(&conversation.id));
    guard
        .workbench
        .messages
        .retain(|message| !conversation_ids.contains(&message.conversation_id));
    guard
        .workbench
        .archived_conversations
        .retain(|conversation| !conversation_ids.contains(&conversation.id));
    guard.workbench.archived_conversations.extend(
        archived_sessions
            .iter()
            .map(archived_conversation_summary_from_session),
    );
    guard
        .workbench
        .archived_conversations
        .sort_by_key(|conversation| {
            -conversation
                .last_updated_at
                .parse::<i64>()
                .unwrap_or_default()
        });
    if guard
        .workbench
        .selected_conversation_id
        .as_ref()
        .is_some_and(|selected| conversation_ids.contains(selected))
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_workbench_model::SessionId;

    /// Builds one workspace snapshot from active project rows.
    /// 使用活动项目行构建一个工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `task_projects` contains the active project rows visible to project actions.
    /// `task_projects` 包含项目操作可见的活动项目行。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot with only the supplied active projects populated.
    /// 返回仅填充给定活动项目的工作区快照。
    fn workspace_snapshot_with_projects(
        task_projects: Vec<TaskProjectSummary>,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects,
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Builds one project summary for active-project resolver tests.
    /// 为活动项目解析器测试构建单个项目摘要。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the persisted project identifier.
    /// `project_id` 是持久化项目标识。
    /// `title` is the displayed project title.
    /// `title` 是项目展示标题。
    ///
    /// # Returns
    /// 返回值
    /// A task project summary without nested sessions.
    /// 返回不包含嵌套会话的任务项目摘要。
    fn project_summary(project_id: &str, title: &str) -> TaskProjectSummary {
        TaskProjectSummary::new(
            project_id,
            title,
            format!("D:/workspace/{project_id}"),
            false,
            0,
            None,
            None,
            Vec::new(),
        )
    }

    /// Builds one active session summary for project action tests.
    /// 为项目操作测试构建单个活动会话摘要。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the stable session identifier.
    /// `session_id` 是稳定会话标识。
    /// `project_id` is the optional project id stored on the session.
    /// `project_id` 是写入会话的可选项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A visible idle session with the requested project id.
    /// 返回带指定项目标识的可见空闲会话。
    fn session_summary(session_id: SessionId, project_id: Option<String>) -> SessionSummary {
        SessionSummary::new(
            session_id,
            project_id,
            format!("Session {session_id}"),
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )
    }

    /// Builds the default workspace project summary used by removal policy tests.
    /// 构建移除策略测试使用的默认工作区项目摘要。
    ///
    /// # Returns
    /// 返回值
    /// A project summary whose path matches the canonical default workspace path.
    /// 返回路径匹配标准默认工作区路径的项目摘要。
    fn default_project_summary() -> TaskProjectSummary {
        TaskProjectSummary::new(
            "default-project",
            "VulcanProject",
            vulcan_workspace_store::workspace_store::default_workspace_project_path_text()
                .expect("test host should expose a home directory"),
            false,
            0,
            None,
            None,
            Vec::new(),
        )
    }

    /// Resolves active projects through the shared workspace project lookup.
    /// 通过共享工作区项目查找解析活动项目。
    #[test]
    fn active_project_from_workspace_resolves_active_project() {
        // Workspace snapshot containing one active project row.
        // 包含一个活动项目行的工作区快照。
        let workspace_snapshot =
            workspace_snapshot_with_projects(vec![project_summary("project-a", "Project A")]);

        // Resolved project returned by the project action helper.
        // 项目操作辅助函数返回的已解析项目。
        let project = active_project_from_workspace(&workspace_snapshot, "project-a", "移除")
            .expect("active project should resolve through shared lookup");

        assert_eq!(project.id, "project-a");
        assert_eq!(project.title, "Project A");
    }

    /// Rejects blank project identifiers instead of matching blank project rows.
    /// 拒绝空白项目标识，而不是匹配空白项目行。
    #[test]
    fn active_project_from_workspace_rejects_blank_project_id() {
        // Workspace snapshot containing a malformed blank active project row.
        // 包含一个异常空白活动项目行的工作区快照。
        let workspace_snapshot =
            workspace_snapshot_with_projects(vec![project_summary("   ", "Blank Project")]);

        // Error emitted by the shared blank-project-id semantics.
        // 共享空白项目标识语义返回的错误。
        let error = active_project_from_workspace(&workspace_snapshot, "   ", "移除")
            .expect_err("blank project id should be rejected by shared lookup");

        assert!(error.contains("未找到要移除的项目"));
    }

    /// Includes sessions that point to the project even when project membership is stale.
    /// 即使项目成员表过期，也包含自身指向该项目的会话。
    #[test]
    fn active_sessions_for_project_action_includes_session_project_id_matches() {
        // Project membership contains only one session while another session still stores the same project id.
        // 项目成员表只包含一个会话，而另一个会话自身仍写着相同项目标识。
        let listed_session_id = SessionId::new_v4();
        let inferred_session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
        let mut project = project_summary("project-a", "Project A");
        project.session_ids.push(listed_session_id);
        let mut workspace_snapshot = workspace_snapshot_with_projects(vec![project.clone()]);
        workspace_snapshot.sessions = vec![
            session_summary(listed_session_id, Some(String::from("project-a"))),
            session_summary(inferred_session_id, Some(String::from("project-a"))),
            session_summary(unrelated_session_id, Some(String::from("project-b"))),
        ];

        // Project actions must archive both owned sessions so deleting the project cannot leave an unlinked active conversation.
        // 项目操作必须归档两个归属会话，避免删除项目后留下未关联的活动会话。
        let project_sessions = active_sessions_for_project_action(&workspace_snapshot, &project);

        assert_eq!(
            project_sessions
                .iter()
                .map(|session| session.id)
                .collect::<Vec<_>>(),
            vec![listed_session_id, inferred_session_id]
        );
    }

    /// Rejects removing the default project when it is the only active project.
    /// 当默认项目是唯一活动项目时拒绝移除。
    #[test]
    fn project_removal_rejects_only_default_project() {
        // Workspace snapshot containing only the built-in default project.
        // 仅包含内置默认项目的工作区快照。
        let default_project = default_project_summary();
        let workspace_snapshot = workspace_snapshot_with_projects(vec![default_project.clone()]);

        // Removal policy refuses to leave the workspace without any active project.
        // 移除策略拒绝让工作区进入没有活动项目的状态。
        let error = project_removal_default_project_effect_from_snapshot(
            &workspace_snapshot,
            &default_project.id,
            &default_project,
        )
        .expect_err("the only default project should not be removable");

        assert!(error.contains("默认项目是当前唯一项目"));
    }

    /// Requests default-project restoration after removing the only non-default project.
    /// 移除唯一非默认项目后要求恢复默认项目。
    #[test]
    fn project_removal_restores_default_after_last_non_default_project() {
        // Workspace snapshot containing one ordinary active project.
        // 包含一个普通活动项目的工作区快照。
        let project = project_summary("project-a", "Project A");
        let workspace_snapshot = workspace_snapshot_with_projects(vec![project.clone()]);

        // Removal is allowed, but the default project must be restored afterward.
        // 允许移除，但之后必须恢复默认项目。
        let effect = project_removal_default_project_effect_from_snapshot(
            &workspace_snapshot,
            &project.id,
            &project,
        )
        .expect("ordinary last project removal should be repairable");

        assert_eq!(effect, ProjectRemovalDefaultProjectEffect::RestoreDefault);
    }

    /// Allows removing the default project when another active project remains.
    /// 当仍有其他活动项目时允许移除默认项目。
    #[test]
    fn project_removal_allows_default_project_when_another_project_remains() {
        // Workspace snapshot containing the default project and one ordinary project.
        // 包含默认项目和一个普通项目的工作区快照。
        let default_project = default_project_summary();
        let workspace_snapshot = workspace_snapshot_with_projects(vec![
            default_project.clone(),
            project_summary("project-a", "Project A"),
        ]);

        // Another project remains visible, so no default restoration is required.
        // 仍有另一个项目可见，因此无需恢复默认项目。
        let effect = project_removal_default_project_effect_from_snapshot(
            &workspace_snapshot,
            &default_project.id,
            &default_project,
        )
        .expect("default project removal should be allowed when another project remains");

        assert_eq!(effect, ProjectRemovalDefaultProjectEffect::None);
    }
}

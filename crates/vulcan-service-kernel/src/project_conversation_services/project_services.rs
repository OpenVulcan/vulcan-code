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
    let (controller_runtime, project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "归档")?;

    if project.session_ids.is_empty() {
        return retained_workbench_snapshot_from_state(&state);
    }

    vulcan_workspace_store::workspace_store::archive_project_sessions(
        controller_runtime,
        &project_id,
        &project_sessions,
    )
    .await?;
    archive_project_sessions_in_retained_state(&state, &project_id, &project_sessions, false)
}

/// Archives all project conversations, removes the active project row, and refreshes the snapshot.
/// 归档项目全部对话、移除活动项目记录，并刷新工作台快照。
pub(crate) async fn service_remove_project(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "移除")?;

    vulcan_workspace_store::workspace_store::remove_project_after_archiving_sessions(
        controller_runtime,
        &project_id,
        &project_sessions,
    )
    .await?;
    if project.session_ids.is_empty() {
        return remove_empty_project_from_retained_state(&state, &project_id);
    }
    archive_project_sessions_in_retained_state(&state, &project_id, &project_sessions, true)
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
    let project_sessions = project
        .session_ids
        .iter()
        .filter_map(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == *session_id)
                .cloned()
        })
        .collect::<Vec<_>>();
    Ok((controller_runtime, project, project_sessions))
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

/// Removes an empty active project from retained workspace and workbench state.
/// 从保留的工作区与工作台状态中移除一个空活动项目。
pub(super) fn remove_empty_project_from_retained_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot
            .task_projects
            .retain(|project| project.id != project_id);
        workspace_snapshot.expanded_task_projects.remove(project_id);
    }
    guard
        .workbench
        .projects
        .retain(|project| project.id != project_id);
    guard
        .workbench
        .expanded_project_ids
        .retain(|candidate| candidate != project_id);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(guard.workbench.clone())
}

/// Moves project sessions between retained active and archived metadata without reloading catalogs.
/// 在保留内存状态中迁移项目会话到归档元数据，不重新加载目录目录表。
pub(super) fn archive_project_sessions_in_retained_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    sessions: &[SessionSummary],
    remove_project: bool,
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
        if remove_project {
            workspace_snapshot
                .task_projects
                .retain(|project| project.id != project_id);
            workspace_snapshot.expanded_task_projects.remove(project_id);
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
    if remove_project {
        guard
            .workbench
            .projects
            .retain(|project| project.id != project_id);
        guard
            .workbench
            .expanded_project_ids
            .retain(|candidate| candidate != project_id);
    } else {
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
    }
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
}

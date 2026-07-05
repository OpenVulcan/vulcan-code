use super::*;

/// Loads or seeds the full workspace snapshot used by runtime startup.
/// 加载或初始化运行时启动使用的完整工作区快照。
///
/// # Parameters
/// 参数
/// `controller` provides access to the runtime data paths and shared controller services.
/// `controller` 提供运行时数据路径与共享控制器服务访问能力。
///
/// # Returns
/// 返回值
/// Returns the hydrated workspace snapshot, or a descriptive storage error.
/// 返回已水合的工作区快照，或描述性的存储错误。
pub async fn load_or_seed_workspace(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot(controller).await
}

/// Loads the persisted workspace index without eagerly hydrating every session runtime state.
/// 加载持久化工作区索引，但不急切水合每个会话运行态。
pub async fn load_or_seed_workspace_index(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_index_snapshot(controller).await
}

/// Loads the current persisted workspace snapshot from project config and segmented text metadata.
/// 从项目配置与分段文本元数据加载当前持久化工作台快照。
pub async fn load_workspace_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot_with_runtime_state(controller, true).await
}

/// Loads the current persisted workspace index without full per-session runtime state.
/// 加载当前持久化工作区索引，不包含每个会话的完整运行态。
pub async fn load_workspace_index_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot_with_runtime_state(controller, false).await
}

/// Loads the current persisted workspace snapshot with an optional runtime-state pass.
/// 按可选运行态加载过程读取当前持久化工作区快照。
async fn load_workspace_snapshot_with_runtime_state(
    controller: ControllerRuntimeHandle,
    load_runtime_state: bool,
) -> Result<WorkspaceSnapshot, String> {
    let project_rows = query_project_rows(&controller).await?;
    let session_rows = query_session_index_rows(&controller).await?;

    let mut task_projects = Vec::new();
    let mut archived_task_projects = Vec::new();
    for row in &project_rows {
        let project = TaskProjectSummary::new(
            row.id.clone(),
            row.name.clone(),
            row.root_path.clone(),
            row.pinned != 0,
            row.sort_order.max(0) as usize,
            row.icon_pack.clone(),
            row.icon_key.clone(),
            Vec::new(),
        );

        if row.archived != 0 {
            archived_task_projects.push(project);
        } else {
            task_projects.push(project);
        }
    }

    let transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let archived_transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();
    let archived_compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();

    let mut runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let archived_runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    let mut session_updated_at = BTreeMap::new();
    for row in &session_rows {
        let session_id = parse_session_id(&row.id)?;
        let project_id = if row.project_id.trim().is_empty() {
            row.project_tag.clone()
        } else {
            row.project_id.clone()
        };
        let project_tag = if row.project_tag.trim().is_empty() {
            project_id.clone()
        } else {
            row.project_tag.clone()
        };
        let archived = row.archived != 0;
        let session_dir = segmented_session_directory(
            &controller.data_root,
            archived,
            &project_tag,
            &segmented_session_date_from_unix_millis_text(&row.created_at),
            &row.id,
        );
        if !session_dir.exists() {
            continue;
        }
        let project_name = project_rows
            .iter()
            .find(|row| row.id == project_id)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| project_id.clone());
        let project_row_exists = project_rows.iter().any(|row| row.id == project_id);
        if archived && project_row_exists {
            ensure_project_summary_for_session(
                &mut archived_task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        } else if !archived {
            ensure_project_summary_for_session(
                &mut task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        }
        if !archived && load_runtime_state {
            let mut runtime_state = if let Some(state_document) = read_state(&session_dir)? {
                if state_document.runtime_state.is_null() {
                    None
                } else {
                    let mut runtime_state = serde_json::from_value::<SessionRuntimeStateEntry>(
                        state_document.runtime_state,
                    )
                    .map_err(|error| {
                        format!("解析分段会话 `{}` 的 state.json 失败：{error}", row.id)
                    })?;
                    if runtime_state.updated_at.trim().is_empty()
                        && !state_document.updated_at.trim().is_empty()
                    {
                        runtime_state.updated_at = state_document.updated_at;
                    }
                    Some(runtime_state)
                }
            } else {
                None
            };
            if let Some(addon_state) = read_session_addon_state(&session_dir)? {
                addon_state.apply_to_runtime_state(
                    runtime_state.get_or_insert_with(SessionRuntimeStateEntry::default),
                );
            }
            if let Some(mut runtime_state) = runtime_state {
                let _ = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);
                runtime_state_map.insert(session_id, runtime_state);
            }
        }
        let focus = session_focus_preview(&[]);
        let title = row
            .title
            .clone()
            .unwrap_or_else(|| String::from("Untitled"));
        let session_summary = SessionSummary::new(
            session_id,
            Some(project_id),
            title,
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(
            row.title_source.as_deref().unwrap_or("manual"),
        ))
        .with_created_at(row.created_at.clone())
        .with_updated_at(row.updated_at.clone());
        session_updated_at.insert(
            session_id,
            row.updated_at.parse::<i64>().unwrap_or_default(),
        );

        if archived {
            archived_sessions.push(session_summary);
        } else {
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    visible_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
    });

    archived_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
    });

    let collapsed_task_projects = query_collapsed_task_project_ids(&controller)
        .await?
        .unwrap_or_default();
    let expanded_task_projects = task_projects
        .iter()
        .filter(|project| !collapsed_task_projects.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let selected_session = visible_sessions
        .first()
        .map(|session| session.id)
        .unwrap_or_default();
    let session_run_phases = visible_sessions
        .iter()
        .map(|session| (session.id, SessionRunPhase::Idle))
        .collect::<BTreeMap<_, _>>();

    Ok(WorkspaceSnapshot {
        task_projects,
        archived_task_projects,
        sessions: visible_sessions,
        archived_sessions,
        session_messages: transcript_map,
        archived_session_messages: archived_transcript_map,
        session_compactions: compaction_map,
        archived_session_compactions: archived_compaction_map,
        session_runtime_state: runtime_state_map,
        archived_session_runtime_state: archived_runtime_state_map,
        session_run_phases,
        expanded_task_projects,
        selected_session,
        hydrated_session_ids: BTreeSet::new(),
    })
}

/// Clears process-scoped runtime latches that must never survive an application restart.
/// 清理绝不能跨应用重启保留的进程级运行态闩锁。
///
/// # Parameters
/// 参数
/// `runtime_state` is the freshly loaded persisted session runtime state.
/// `runtime_state` 是刚从持久化内容加载出的会话运行态。
///
/// # Returns
/// 返回值
/// Whether any volatile field was cleared.
/// 是否清理了任何临时字段。
pub(super) fn sanitize_loaded_runtime_state_for_fresh_process(
    runtime_state: &mut SessionRuntimeStateEntry,
) -> bool {
    runtime_state.active_precheck_turn_id.take().is_some()
}

/// Builds the default empty workspace snapshot used before any real project has been added.
/// 构建在尚未添加任何真实项目之前使用的默认空工作台快照。
pub(super) fn build_empty_workspace_snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: Vec::new(),
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

/// Ensures a project summary exists and owns the provided session id exactly once.
/// 确保项目摘要存在，并且只拥有一次给定会话标识。
pub(super) fn ensure_project_summary_for_session(
    projects: &mut Vec<TaskProjectSummary>,
    project_id: &str,
    project_name: &str,
    session_id: SessionId,
) {
    if let Some(project) = projects.iter_mut().find(|project| project.id == project_id) {
        if !project.session_ids.contains(&session_id) {
            project.session_ids.push(session_id);
        }
        return;
    }
    projects.push(TaskProjectSummary::new(
        project_id.to_string(),
        project_name.to_string(),
        String::new(),
        false,
        projects.len(),
        None,
        None,
        vec![session_id],
    ));
}

/// Clears all workspace business tables so the application can rebuild from a clean empty state.
/// 清空全部工作台业务表，以便应用从干净的空状态重新开始。
async fn clear_workspace_storage(controller: &ControllerRuntimeHandle) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
DELETE FROM projects;
DELETE FROM workspace_session_index;
DELETE FROM app_metadata WHERE key = 'workspace.collapsed_task_projects';
"#,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("清理旧原型工作台数据失败：{error}"))?;
    clear_all_segmented_session_roots(&controller.data_root)?;

    Ok(())
}

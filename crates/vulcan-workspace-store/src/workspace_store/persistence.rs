//! Workspace persistence write-path helpers extracted from the main workspace store module.
//! 从主工作台存储模块中抽取的写入持久化路径辅助实现。

#![allow(dead_code)]

use super::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, append_segmented_session_message_update,
    archive_segmented_session_by_created_date, archive_segmented_session_by_id,
    create_segmented_session_in_location, delete_segmented_session_by_created_date,
    delete_segmented_session_by_id, find_segmented_session_directory,
    find_segmented_session_directory_by_id, patch_segmented_session_metadata,
    restore_segmented_session_by_created_date, segmented_session_directory,
    state_document_from_runtime_state, sync_runtime_log_from_conversation_state, write_state,
};
use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;

/// Persists the current visible workspace snapshot back into the controller-backed SQLite database.
/// 将当前可见工作台快照回写到控制器承载的 SQLite 数据库。
pub async fn persist_workspace_snapshot(
    controller: ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<WorkspaceSnapshotPersistOutcome, String> {
    let mut warnings = Vec::new();
    let existing_active_project_ids = query_active_project_ids(&controller).await?;
    let visible_project_ids = snapshot
        .task_projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let all_projects = snapshot
        .task_projects
        .iter()
        .cloned()
        .map(|project| (project, false))
        .chain(
            snapshot
                .archived_task_projects
                .iter()
                .cloned()
                .map(|project| (project, true)),
        )
        .collect::<Vec<_>>();
    upsert_projects(&controller, &all_projects).await?;
    persist_session_index_snapshot(&controller, snapshot).await?;
    persist_segmented_snapshot_session_stores(
        &controller,
        &snapshot.sessions,
        &snapshot.session_messages,
        &snapshot.session_runtime_state,
        &snapshot.archived_sessions,
        &snapshot.archived_session_messages,
        &snapshot.archived_session_runtime_state,
        &snapshot.hydrated_session_ids,
        &mut warnings,
    )
    .await;
    archive_missing_projects(
        &controller,
        &existing_active_project_ids,
        &visible_project_ids,
    )
    .await?;

    Ok(WorkspaceSnapshotPersistOutcome { warnings })
}

/// Persists the lightweight deterministic session locator index for one workspace snapshot.
/// 为一个工作台快照持久化轻量确定性会话定位索引。
async fn persist_session_index_snapshot(
    controller: &ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    let active_owner_map = session_owner_map_from_projects(&snapshot.task_projects);
    let archived_owner_map = session_owner_map_from_projects(&snapshot.archived_task_projects);
    for session in &snapshot.sessions {
        let (project_id, sort_order) = active_owner_map
            .get(&session.id)
            .cloned()
            .or_else(|| session.project_id.clone().map(|project_id| (project_id, 0)))
            .unwrap_or_else(|| (String::from("untagged"), 0));
        upsert_session_index_row(controller, session, &project_id, false, sort_order, None).await?;
    }
    for session in &snapshot.archived_sessions {
        let (project_id, sort_order) = archived_owner_map
            .get(&session.id)
            .cloned()
            .or_else(|| session.project_id.clone().map(|project_id| (project_id, 0)))
            .unwrap_or_else(|| (String::from("untagged"), 0));
        upsert_session_index_row(controller, session, &project_id, true, sort_order, None).await?;
    }
    Ok(())
}

/// Mirrors all loaded snapshot sessions into segmented append-only directories.
/// 将所有已加载的快照会话镜像到分段追加式目录。
async fn persist_segmented_snapshot_session_stores(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    hydrated_session_ids: &BTreeSet<SessionId>,
    warnings: &mut Vec<String>,
) {
    persist_segmented_snapshot_session_group(
        controller,
        sessions,
        false,
        session_messages,
        session_runtime_state,
        hydrated_session_ids,
        warnings,
    )
    .await;
    persist_segmented_snapshot_session_group(
        controller,
        archived_sessions,
        true,
        archived_session_messages,
        archived_session_runtime_state,
        hydrated_session_ids,
        warnings,
    )
    .await;
}

/// Mirrors one active or archived snapshot session group into segmented storage.
/// 将一组活动或归档快照会话镜像到分段存储。
async fn persist_segmented_snapshot_session_group(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    archived: bool,
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    hydrated_session_ids: &BTreeSet<SessionId>,
    warnings: &mut Vec<String>,
) {
    let empty_messages = Vec::<SessionMessageEntry>::new();
    for session in sessions {
        let runtime_state = session_runtime_state.get(&session.id);
        if !snapshot_session_transcript_is_loaded(
            session.id,
            session_messages,
            hydrated_session_ids,
        ) {
            continue;
        }
        let rebuilt_messages;
        let stored_messages = session_messages.get(&session.id).unwrap_or(&empty_messages);
        let effective_messages = if let Some(runtime_state) = runtime_state {
            rebuilt_messages =
                conversation_state_to_session_messages(&runtime_state.conversation_state);
            if rebuilt_messages.is_empty() && !stored_messages.is_empty() {
                stored_messages.as_slice()
            } else {
                rebuilt_messages.as_slice()
            }
        } else {
            stored_messages.as_slice()
        };
        if let Err(error) = persist_segmented_session_tail_update(
            controller,
            session,
            session.project_id.as_deref().unwrap_or("untagged"),
            archived,
            effective_messages,
            runtime_state,
        )
        .await
        {
            warnings.push(segmented_session_persist_warning(
                "同步快照分段会话失败",
                1,
                &error,
            ));
        }
    }
}

/// Returns whether a snapshot session has a loaded transcript window that is safe to mirror into JSONL.
/// 返回快照会话是否已有可安全镜像到 JSONL 的已加载转录窗口。
fn snapshot_session_transcript_is_loaded(
    session_id: SessionId,
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    hydrated_session_ids: &BTreeSet<SessionId>,
) -> bool {
    session_messages.contains_key(&session_id) || hydrated_session_ids.contains(&session_id)
}

/// Persists one session and its runtime artifacts without rewriting unrelated workspace sessions.
/// 仅持久化单个会话及其运行时工件，不重写无关工作区会话。
pub async fn persist_single_session_update(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    session: &SessionSummary,
    archived: bool,
    messages: &[SessionMessageEntry],
    compactions: Option<&[SessionCompactionEntry]>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<WorkspaceSnapshotPersistOutcome, String> {
    let mut warnings = Vec::new();
    let session_owner_map = session_owner_map_from_projects(task_projects);
    let Some((project_id, _sort_order)) = session_owner_map.get(&session.id) else {
        return Err(format!(
            "单会话保存失败：未找到会话所属项目：{}",
            session.id
        ));
    };

    let sessions = std::slice::from_ref(session);
    let archived_sessions = Vec::<SessionSummary>::new();
    let rebuilt_messages;
    let effective_messages = if let Some(runtime_state) = runtime_state {
        rebuilt_messages =
            conversation_state_to_session_messages(&runtime_state.conversation_state);
        if rebuilt_messages.is_empty() && !messages.is_empty() {
            messages
        } else {
            rebuilt_messages.as_slice()
        }
    } else {
        messages
    };
    let _ = (sessions, archived_sessions, compactions);
    upsert_session_index_row(
        &controller,
        session,
        project_id,
        archived,
        0,
        runtime_state.map(|state| state.updated_at.as_str()),
    )
    .await?;

    if let Err(error) = persist_segmented_session_tail_update(
        &controller,
        session,
        project_id,
        archived,
        effective_messages,
        runtime_state,
    )
    .await
    {
        warnings.push(segmented_session_persist_warning(
            "同步分段会话追加流失败",
            1,
            &error,
        ));
    }

    Ok(WorkspaceSnapshotPersistOutcome { warnings })
}

/// Persists only one session runtime-state document without rewriting transcript or project rows.
/// 仅持久化单个会话运行时状态文档，不重写转录或项目行。
pub async fn persist_single_session_runtime_state_update(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    persist_segmented_session_runtime_state_checkpoint(&controller, session_id, runtime_state)
        .await?;

    Ok(())
}

/// Renames one visible session by exact locator metadata without scanning segmented directories.
/// 通过精确定位元数据重命名一个可见会话，不扫描分段目录。
pub async fn rename_single_session_metadata(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    let mut renamed_session = session.clone();
    renamed_session.title = title.to_string();
    renamed_session.title_source = SessionTitleSource::Manual;
    renamed_session.updated_at = updated_at.to_string();
    let created_at = normalized_session_created_at(&renamed_session);
    let created_date = segmented_session_date_from_unix_millis_text(&created_at);
    let session_dir = segmented_session_directory(
        &controller.data_root,
        false,
        project_id,
        &created_date,
        &renamed_session.id.to_string(),
    );
    if session_dir.exists() {
        patch_segmented_session_metadata(
            &session_dir,
            SegmentedSessionMetadataPatch {
                updated_at: Some(updated_at.to_string()),
                title: Some(Some(title.to_string())),
                title_source: Some(Some(
                    SessionTitleSource::Manual.as_persisted_key().to_string(),
                )),
                ..Default::default()
            },
        )?;
    }
    upsert_session_index_row(
        &controller,
        &renamed_session,
        project_id,
        false,
        0,
        Some(updated_at),
    )
    .await?;
    Ok(())
}

/// Marks one visible session as archived without rewriting transcript or runtime-state documents.
/// 将单个可见会话标记为已归档，不重写转录或运行时状态文档。
pub async fn archive_single_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    archive_segmented_session_directory_if_present(&controller, session_id).await?;
    Ok(())
}

/// Archives one visible session when its owning project is already known.
/// 在已知所属项目时归档单个可见会话。
pub async fn archive_single_session_for_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
) -> Result<(), String> {
    let archived_at = archive_segmented_session_directory_for_project_and_summary_if_present(
        &controller,
        project_id,
        session,
    )?;
    upsert_session_index_row(
        &controller,
        session,
        project_id,
        true,
        0,
        Some(&archived_at),
    )
    .await?;
    Ok(())
}

/// Restores one archived session summary into the visible project binding.
/// 将单个已归档会话摘要恢复到可见项目绑定中。
pub async fn restore_single_session(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    session: &SessionSummary,
) -> Result<(), String> {
    let project_id = session
        .project_id
        .as_deref()
        .or_else(|| {
            task_projects
                .iter()
                .find(|project| project.session_ids.contains(&session.id))
                .map(|project| project.id.as_str())
        })
        .unwrap_or("untagged");
    restore_segmented_session_directory_for_project_if_present(&controller, project_id, session)?;
    upsert_session_index_row(&controller, session, project_id, false, 0, None).await?;
    let _ = task_projects;
    Ok(())
}

/// Archives all visible sessions under one project while keeping the project active.
/// 归档单个项目下的全部可见会话，同时保留项目为活动状态。
pub async fn archive_project_sessions(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    sessions: &[SessionSummary],
) -> Result<(), String> {
    for (index, session) in sessions.iter().enumerate() {
        let archived_at = archive_segmented_session_directory_for_project_and_summary_if_present(
            &controller,
            project_id,
            session,
        )?;
        upsert_session_index_row(
            &controller,
            session,
            project_id,
            true,
            index,
            Some(&archived_at),
        )
        .await?;
    }
    Ok(())
}

/// Marks one visible project and all of its child sessions as archived.
/// 将单个可见项目及其全部子会话标记为已归档。
pub async fn archive_project_with_sessions(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session_ids: &[SessionId],
) -> Result<(), String> {
    archive_project_session_ids(controller.clone(), project_id, session_ids).await?;
    update_project_archive_flag(&controller, project_id, true).await?;
    Ok(())
}

/// Archives visible project sessions by id for legacy callers that do not retain creation timestamps.
/// 为尚未保留创建时间的旧调用方按标识归档可见项目会话。
async fn archive_project_session_ids(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session_ids: &[SessionId],
) -> Result<(), String> {
    for session_id in session_ids {
        archive_segmented_session_directory_for_project_if_present(
            &controller,
            project_id,
            *session_id,
        )?;
    }
    Ok(())
}

/// Archives project sessions and removes the active project row without deleting archived data.
/// 归档项目会话并移除活动项目记录，但不删除归档数据。
pub async fn remove_project_after_archiving_sessions(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    sessions: &[SessionSummary],
) -> Result<(), String> {
    if !sessions.is_empty() {
        archive_project_sessions(controller.clone(), project_id, sessions).await?;
    }
    delete_active_project_row(&controller, project_id).await?;
    Ok(())
}

/// Restores one archived project and its child session summaries into the visible tree.
/// 将单个已归档项目及其子会话摘要恢复到可见树中。
pub async fn restore_project_with_sessions(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    project: &TaskProjectSummary,
    sessions: &[SessionSummary],
) -> Result<(), String> {
    for (index, session) in sessions.iter().enumerate() {
        restore_segmented_session_directory_for_project_if_present(
            &controller,
            &project.id,
            session,
        )?;
        upsert_session_index_row(&controller, session, &project.id, false, index, None).await?;
    }
    upsert_projects(&controller, &[(project.clone(), false)]).await?;
    let _ = task_projects;
    Ok(())
}

/// Archives a segmented session directory using the persisted session locator when present.
/// 存在持久化会话定位信息时归档对应分段会话目录。
async fn archive_segmented_session_directory_if_present(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let Some(locator) = query_session_index_locator(controller, session_id, false).await? else {
        return Ok(());
    };
    let archived_at = current_unix_millis_text();
    let project_tag = if locator.project_tag.trim().is_empty() {
        locator.project_id.as_str()
    } else {
        locator.project_tag.as_str()
    };
    let created_date = segmented_session_date_from_unix_millis_text(&locator.created_at);
    let _ = archive_segmented_session_by_created_date(
        &controller.data_root,
        project_tag,
        &created_date,
        &session_id.to_string(),
        &archived_at,
    )?;
    update_session_index_archive_state(controller, session_id, true, &archived_at).await
}

/// Archives a segmented session directory for a known project id when present.
/// 在已知项目标识时归档对应分段会话目录。
fn archive_segmented_session_directory_for_project_if_present(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
) -> Result<(), String> {
    let archived_at = current_unix_millis_text();
    let _ = archive_segmented_session_by_id(
        &controller.data_root,
        project_id,
        &session_id.to_string(),
        &archived_at,
    )?;
    Ok(())
}

/// Archives a known project session by deterministic metadata instead of scanning directories.
/// 通过确定性元数据归档已知项目会话，而不是扫描目录。
fn archive_segmented_session_directory_for_project_and_summary_if_present(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
) -> Result<String, String> {
    let created_at = session.created_at.trim();
    if created_at.is_empty() {
        return Err(format!(
            "会话缺少创建时间，无法无扫描定位目录：{}",
            session.id
        ));
    }
    let archived_at = current_unix_millis_text();
    let created_date = segmented_session_date_from_unix_millis_text(created_at);
    let _ = archive_segmented_session_by_created_date(
        &controller.data_root,
        project_id,
        &created_date,
        &session.id.to_string(),
        &archived_at,
    )?;
    Ok(archived_at)
}

/// Restores a segmented session directory for a known project id when present.
/// 在已知项目标识时恢复对应分段会话目录。
fn restore_segmented_session_directory_for_project_if_present(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
) -> Result<(), String> {
    let restored_at = current_unix_millis_text();
    let created_at = normalized_session_created_at(session);
    let created_date = segmented_session_date_from_unix_millis_text(&created_at);
    let _ = restore_segmented_session_by_created_date(
        &controller.data_root,
        project_id,
        &created_date,
        &session.id.to_string(),
        &restored_at,
    )?;
    Ok(())
}

/// Deletes active and archived segmented session directories for one known project when present.
/// 在已知项目标识时删除活动与归档树中存在的分段会话目录。
fn delete_segmented_session_directories_for_project_if_present(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let _ =
        delete_segmented_session_by_id(&controller.data_root, true, project_id, &session_id_text)?;
    let _ =
        delete_segmented_session_by_id(&controller.data_root, false, project_id, &session_id_text)?;
    Ok(())
}

/// Deletes one segmented session directory from a persisted locator.
/// 按持久化定位信息删除一个分段会话目录。
fn delete_segmented_session_directory_for_locator_if_present(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    locator: &PersistedSessionIndexLocator,
) -> Result<(), String> {
    let project_tag = if locator.project_tag.trim().is_empty() {
        locator.project_id.as_str()
    } else {
        locator.project_tag.as_str()
    };
    let created_date = segmented_session_date_from_unix_millis_text(&locator.created_at);
    let _ = delete_segmented_session_by_created_date(
        &controller.data_root,
        locator.archived,
        project_tag,
        &created_date,
        &session_id.to_string(),
    )?;
    Ok(())
}

/// Persists one newly visible project row without rewriting unrelated workspace rows.
/// 持久化单个新出现的可见项目行，不重写无关工作区行。
pub async fn persist_new_project(
    controller: ControllerRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    upsert_projects(&controller, &[(project.clone(), false)]).await
}

/// Updates the archived flag for one project row without changing its metadata.
/// 只更新单个项目行的归档标记，不改变其元数据。
async fn update_project_archive_flag(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    archived: bool,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE projects SET archived = ?1, updated_at = ?2 WHERE id = ?3;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("更新项目归档状态失败：{error}"))?;

    Ok(())
}

/// Deletes one active project row while preserving archived session directories.
/// 删除单个活动项目记录，同时保留归档会话目录。
async fn delete_active_project_row(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "DELETE FROM projects WHERE id = ?1 AND archived = 0;",
            vec![vec![ControllerSqliteValue::String(project_id.to_string())]],
        )
        .await
        .map_err(|error| format!("移除项目记录失败：{error}"))?;

    Ok(())
}

/// Toggles one active project's pinned flag without rewriting unrelated workspace rows.
/// 切换单个活动项目的置顶标记，不重写无关工作区行。
pub async fn toggle_project_pinned(
    controller: ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;

    Ok(())
}

/// Persists one project's pinned flag and both project-section orders in a deterministic sequence.
/// 按确定顺序持久化单个项目的置顶标记以及两个项目分区的顺序。
pub async fn persist_project_pinned_and_order_update(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    pinned: bool,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET pinned = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(pinned)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("写入项目置顶状态失败：{error}"))?;

    persist_project_order_update(controller.clone(), pinned_project_ids).await?;
    persist_project_order_update(controller, regular_project_ids).await?;

    Ok(())
}

/// Persists one active project's display name without touching its sessions.
/// 持久化单个活动项目的显示名称，不触碰其会话数据。
pub async fn rename_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<(), String> {
    let normalized_name = resolve_project_rename_target(&controller, project_id, name).await?;

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_name.to_string()),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("重命名项目失败：{error}"))?;

    Ok(())
}

/// Resolves an empty project rename into the owning folder name.
/// 将空项目重命名解析为所属文件夹名称。
async fn resolve_project_rename_target(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<String, String> {
    let normalized_name = name.trim();
    if !normalized_name.is_empty() && !is_path_separator_only_project_name(normalized_name) {
        return Ok(normalized_name.to_string());
    }

    /// Raw project path row decoded for empty-name reset.
    /// 为项目空名还原解码的原始项目路径行。
    #[derive(Debug, Clone, Deserialize)]
    struct ProjectPathRow {
        /// Absolute project root path stored in SQLite.
        /// SQLite 中存储的项目绝对根路径。
        root_path: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT root_path FROM projects WHERE id = ?1 AND archived = 0 LIMIT 1;",
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询项目路径失败：{error}"))?;
    let row = decode_query_rows::<ProjectPathRow>(&result.json_data)?
        .into_iter()
        .next()
        .ok_or_else(|| String::from("没有找到要重命名的项目。"))?;
    let fallback_name = project_folder_name_from_path_text(&row.root_path)
        .ok_or_else(|| String::from("项目路径无法生成默认名称。"))?;
    Ok(fallback_name)
}

/// Detects invalid slash-only project display names and treats them as reset requests.
/// 检测仅由斜杠组成的无效项目显示名称，并将其视为还原请求。
fn is_path_separator_only_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| matches!(character, '\\' | '/'))
}

/// Extracts the final folder segment from a Windows or Unix path string.
/// 从 Windows 或 Unix 路径字符串中提取最后一级文件夹名称。
fn project_folder_name_from_path_text(path: &str) -> Option<String> {
    path.split(['\\', '/'])
        .filter(|segment| !segment.trim().is_empty())
        .next_back()
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
}

/// Persists the project order for one pinned or regular section.
/// 持久化单个置顶或普通分区内的项目顺序。
pub async fn persist_project_order_update(
    controller: ControllerRuntimeHandle,
    project_ids: &[String],
) -> Result<(), String> {
    if project_ids.is_empty() {
        return Ok(());
    }

    let updated_at = current_unix_millis_text();
    let rows = project_ids
        .iter()
        .enumerate()
        .map(|(index, project_id)| {
            vec![
                ControllerSqliteValue::Int64(index as i64),
                ControllerSqliteValue::String(updated_at.clone()),
                ControllerSqliteValue::String(project_id.clone()),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            rows,
        )
        .await
        .map_err(|error| format!("重排项目失败：{error}"))?;

    Ok(())
}

/// Persists the collapsed task-project identifiers as workspace-level UI state.
/// 将已折叠任务项目标识作为工作台级 UI 状态持久化。
pub async fn persist_collapsed_task_project_ids(
    controller: ControllerRuntimeHandle,
    collapsed_project_ids: &[String],
) -> Result<(), String> {
    let value = serde_json::to_string(collapsed_project_ids)
        .map_err(|error| format!("序列化工作台项目展开状态失败：{error}"))?;
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
INSERT INTO app_metadata (key, value)
VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
            vec![vec![
                ControllerSqliteValue::String(COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string()),
                ControllerSqliteValue::String(value),
            ]],
        )
        .await
        .map_err(|error| format!("写入工作台项目展开状态失败：{error}"))?;

    Ok(())
}

/// Stores runtime checkpoints only in segmented state files.
/// 仅在分段 state 文件中存储运行态检查点。
/// Mirrors a runtime-state checkpoint into the segmented session state file without touching append-only JSONL segments.
/// 将运行时状态检查点镜像到分段会话状态文件，且不触碰追加式 JSONL 分段。
async fn persist_segmented_session_runtime_state_checkpoint(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id_text)?
    else {
        return Ok(());
    };
    let state_document = state_document_from_runtime_state(&session_id_text, runtime_state)?;
    write_state(&session_dir, &state_document)?;
    sync_runtime_log_from_conversation_state(
        &session_dir,
        &runtime_state.conversation_state,
        &runtime_state.updated_at,
    )?;
    Ok(())
}

/// Mirrors one full session update into the segmented append-only stream and overwrite state file.
/// 将一次完整会话更新镜像到分段追加流与覆盖式状态文件。
async fn persist_segmented_session_tail_update(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    let session_id_text = session.id.to_string();
    let session_dir =
        ensure_segmented_session_directory_for_locator(controller, &session_id_text, &locator)?;
    append_segmented_session_message_update(&session_dir, messages)?;
    if let Some(runtime_state) = runtime_state {
        let state_document = state_document_from_runtime_state(&session_id_text, runtime_state)?;
        write_state(&session_dir, &state_document)?;
        sync_runtime_log_from_conversation_state(
            &session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
        )?;
    }
    Ok(())
}

/// Builds a segmented-session locator directly from the in-memory session summary.
/// 直接从内存会话摘要构建分段会话定位信息。
fn segmented_session_locator_from_summary(
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
) -> SegmentedSessionLocator {
    SegmentedSessionLocator {
        project_id: project_id.to_string(),
        title: session.title.clone(),
        title_source: session.title_source.as_persisted_key().to_string(),
        archived,
        created_at: if session.updated_at.trim().is_empty() {
            current_unix_millis_text()
        } else {
            session.updated_at.clone()
        },
    }
}

/// Resolves or creates the segmented directory described by one persisted session locator.
/// 解析或创建一条持久化会话定位信息描述的分段目录。
fn ensure_segmented_session_directory_for_locator(
    controller: &ControllerRuntimeHandle,
    session_id_text: &str,
    locator: &SegmentedSessionLocator,
) -> Result<std::path::PathBuf, String> {
    match find_segmented_session_directory(
        &controller.data_root,
        locator.archived,
        &locator.project_id,
        session_id_text,
    )? {
        Some(session_dir) => Ok(session_dir),
        None => create_segmented_session_in_location(
            &controller.data_root,
            locator.archived,
            SegmentedSessionCreateRequest {
                session_id: session_id_text.to_string(),
                project_tag: locator.project_id.clone(),
                created_date: segmented_session_date_from_unix_millis_text(&locator.created_at),
                created_at: locator.created_at.clone(),
                project_id: Some(locator.project_id.clone()),
                title: Some(locator.title.clone()),
                title_source: Some(locator.title_source.clone()),
                segment_policy: SegmentPolicy::default(),
            },
        ),
    }
}

/// Stores normalized locator data after SQLite scalar decoding.
/// 保存 SQLite 标量解码后的规范化定位数据。
struct SegmentedSessionLocator {
    /// Stable owning project identifier used as the project tag.
    /// 作为项目标签使用的稳定所属项目标识。
    project_id: String,
    /// Visible session title mirrored into metadata when the segmented directory is first created.
    /// 首次创建分段目录时镜像到 metadata 的可见会话标题。
    title: String,
    /// Persisted title-source key mirrored into metadata.
    /// 镜像到 metadata 的持久化标题来源键。
    title_source: String,
    /// Whether the segmented directory belongs under archived_sessions.
    /// 分段目录是否属于 archived_sessions。
    archived: bool,
    /// Creation timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本存储的创建时间。
    created_at: String,
}

/// Upserts all visible project rows while clearing the archive flag.
/// 在清理归档标记的同时上写全部可见项目行。
async fn upsert_projects(
    controller: &ControllerRuntimeHandle,
    task_projects: &[(TaskProjectSummary, bool)],
) -> Result<(), String> {
    if task_projects.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = task_projects
        .iter()
        .map(|(project, archived)| {
            vec![
                ControllerSqliteValue::String(project.id.clone()),
                ControllerSqliteValue::String(project.title.clone()),
                ControllerSqliteValue::String(project.path.clone()),
                ControllerSqliteValue::Int64(i64::from(project.pinned)),
                ControllerSqliteValue::Int64(i64::from(*archived)),
                ControllerSqliteValue::Int64(project.order as i64),
                optional_sqlite_string(&project.icon_pack),
                optional_sqlite_string(&project.icon_key),
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(now.clone()),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO projects (
    id,
    name,
    root_path,
    pinned,
    archived,
    sort_order,
    icon_pack,
    icon_key,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    root_path = excluded.root_path,
    pinned = excluded.pinned,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            params,
        )
        .await
        .map_err(|error| format!("写入项目列表失败：{error}"))?;

    Ok(())
}

/// Upserts one lightweight session locator row without storing transcript or runtime payload.
/// 写入或更新一条轻量会话定位行，不存储转录或运行态载荷。
async fn upsert_session_index_row(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    sort_order: usize,
    updated_at_override: Option<&str>,
) -> Result<(), String> {
    let created_at = normalized_session_created_at(session);
    let updated_at = updated_at_override
        .map(str::to_string)
        .or_else(|| {
            let trimmed = session.updated_at.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| created_at.clone());
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO workspace_session_index (
    id,
    project_id,
    project_tag,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(id) DO UPDATE SET
    project_id = excluded.project_id,
    project_tag = excluded.project_tag,
    title = excluded.title,
    title_source = excluded.title_source,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            vec![vec![
                ControllerSqliteValue::String(session.id.to_string()),
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::String(session.title.clone()),
                ControllerSqliteValue::String(session.title_source.as_persisted_key().to_string()),
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::Int64(sort_order as i64),
                ControllerSqliteValue::String(created_at),
                ControllerSqliteValue::String(updated_at),
            ]],
        )
        .await
        .map_err(|error| format!("写入会话定位索引失败：{error}"))?;
    Ok(())
}

/// Deletes one lightweight session locator row after the backing session is permanently removed.
/// 在底层会话被永久删除后删除一条轻量会话定位行。
async fn delete_session_index_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "DELETE FROM workspace_session_index WHERE id = ?1;",
            vec![vec![ControllerSqliteValue::String(session_id.to_string())]],
        )
        .await
        .map_err(|error| format!("删除会话定位索引失败：{error}"))?;
    Ok(())
}

/// Updates one lightweight session locator archive state without touching body files.
/// 只更新一条轻量会话定位行的归档状态，不触碰正文文件。
async fn update_session_index_archive_state(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    archived: bool,
    updated_at: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE workspace_session_index SET archived = ?1, updated_at = ?2 WHERE id = ?3;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(updated_at.to_string()),
                ControllerSqliteValue::String(session_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("更新会话定位索引归档状态失败：{error}"))?;
    Ok(())
}

/// Queries one persisted session locator row by session id and expected archive state.
/// 按会话标识和期望归档状态查询一条持久化会话定位行。
async fn query_session_index_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    archived: bool,
) -> Result<Option<PersistedSessionIndexLocator>, String> {
    /// Raw session locator row decoded for deterministic filesystem operations.
    /// 为确定性文件系统操作解码的原始会话定位行。
    #[derive(Debug, Clone, Deserialize)]
    struct SessionLocatorRow {
        /// Owning project identifier.
        /// 所属项目标识。
        project_id: String,
        /// Directory project tag.
        /// 目录项目标签。
        project_tag: String,
        /// Session creation timestamp.
        /// 会话创建时间戳。
        created_at: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT project_id, project_tag, created_at
FROM workspace_session_index
WHERE id = ?1 AND archived = ?2
LIMIT 1;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(i64::from(archived)),
            ],
        )
        .await
        .map_err(|error| format!("查询会话定位索引失败：{error}"))?;
    Ok(decode_query_rows::<SessionLocatorRow>(&result.json_data)?
        .into_iter()
        .next()
        .map(|row| PersistedSessionIndexLocator {
            project_id: row.project_id,
            project_tag: row.project_tag,
            created_at: row.created_at,
            archived,
        }))
}

/// Lightweight persisted session locator used for exact filesystem operations.
/// 用于精确文件系统操作的轻量持久化会话定位信息。
struct PersistedSessionIndexLocator {
    /// Owning project identifier.
    /// 所属项目标识。
    project_id: String,
    /// Directory project tag.
    /// 目录项目标签。
    project_tag: String,
    /// Session creation timestamp.
    /// 会话创建时间戳。
    created_at: String,
    /// Whether the locator points to the archived tree.
    /// 该定位信息是否指向归档树。
    archived: bool,
}

/// Returns a stable creation timestamp for a session locator row.
/// 返回会话定位行使用的稳定创建时间戳。
fn normalized_session_created_at(session: &SessionSummary) -> String {
    let created_at = session.created_at.trim();
    if !created_at.is_empty() {
        return created_at.to_string();
    }
    let updated_at = session.updated_at.trim();
    if !updated_at.is_empty() {
        return updated_at.to_string();
    }
    current_unix_millis_text()
}

/// Soft-archives active projects missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活项目软归档。
async fn archive_missing_projects(
    controller: &ControllerRuntimeHandle,
    existing_active_ids: &BTreeSet<String>,
    visible_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let missing_ids = existing_active_ids
        .iter()
        .filter(|project_id| !visible_ids.contains(*project_id))
        .cloned()
        .collect::<Vec<_>>();

    if missing_ids.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = missing_ids
        .into_iter()
        .map(|project_id| {
            vec![
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE projects SET archived = 1, updated_at = ?1 WHERE id = ?2;",
            params,
        )
        .await
        .map_err(|error| format!("软归档项目失败：{error}"))?;

    Ok(())
}

/// Builds a map from session id to owning project id and project-local sort order.
/// 构建从会话标识到所属项目标识及项目内排序位置的映射。
fn session_owner_map_from_projects(
    task_projects: &[TaskProjectSummary],
) -> BTreeMap<SessionId, (String, usize)> {
    task_projects
        .iter()
        .flat_map(|project| {
            project
                .session_ids
                .iter()
                .enumerate()
                .map(move |(index, session_id)| (*session_id, (project.id.clone(), index)))
        })
        .collect()
}

/// Builds one non-fatal warning describing that segmented session persistence lagged behind a successful snapshot save.
/// 构建一条非致命告警，用于描述分段会话持久化落后于一次成功的快照保存。
fn segmented_session_persist_warning(operation: &str, session_count: usize, error: &str) -> String {
    format!(
        "{operation}：涉及 {session_count} 个会话。核心工作台状态已继续保存，但分段会话文件需要稍后重新同步。原始错误：{error}"
    )
}

/// Converts Unix-milliseconds text into the UTC date segment used by segmented session directories.
/// 将 Unix 毫秒文本转换为分段会话目录使用的 UTC 日期片段。
fn segmented_session_date_from_unix_millis_text(value: &str) -> String {
    let millis = value.trim().parse::<i64>().unwrap_or_default();
    let seconds = millis.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Converts days since the Unix epoch into one proleptic Gregorian UTC date.
/// 将 Unix 纪元以来的天数转换为一个前推格里高利 UTC 日期。
fn civil_date_from_unix_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_unix_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Permanently deletes one archived session together with all persisted transcript and runtime artifacts.
/// 彻底删除一个已归档会话及其全部已持久化的转录与运行时工件。
pub async fn permanently_delete_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(locator) = query_session_index_locator(&controller, session_id, true).await? {
        delete_segmented_session_directory_for_locator_if_present(
            &controller,
            session_id,
            &locator,
        )?;
    }
    delete_session_index_row(&controller, session_id).await?;
    Ok(())
}

/// Permanently deletes one archived project together with all child archived sessions and persisted artifacts.
/// 彻底删除一个已归档项目及其全部子归档会话与已持久化工件。
pub async fn permanently_delete_archived_project(
    controller: ControllerRuntimeHandle,
    project_id: String,
    session_ids: Vec<SessionId>,
) -> Result<(), String> {
    for session_id in &session_ids {
        if let Some(locator) = query_session_index_locator(&controller, *session_id, true).await? {
            delete_segmented_session_directory_for_locator_if_present(
                &controller,
                *session_id,
                &locator,
            )?;
        }
        delete_session_index_row(&controller, *session_id).await?;
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "DELETE FROM projects WHERE id = ?1 AND archived != 0;",
            vec![vec![ControllerSqliteValue::String(project_id.clone())]],
        )
        .await
        .map_err(|error| format!("删除归档项目记录失败：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one compact session summary used by persistence helper tests.
    /// 为持久化辅助测试构建一条紧凑会话摘要。
    fn test_session_summary(id: SessionId, project_id: &str, title: &str) -> SessionSummary {
        SessionSummary::new(
            id,
            Some(project_id.to_string()),
            title.to_string(),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        )
    }

    /// Verifies segmented session directory dates are derived from stable Unix-milliseconds text.
    /// 验证分段会话目录日期由稳定的 Unix 毫秒文本推导。
    #[test]
    fn segmented_session_date_uses_unix_millis_text() {
        assert_eq!(
            segmented_session_date_from_unix_millis_text("0"),
            "1970-01-01"
        );
        assert_eq!(
            segmented_session_date_from_unix_millis_text("86400000"),
            "1970-01-02"
        );
        assert_eq!(
            segmented_session_date_from_unix_millis_text(" 1722470400000 "),
            "2024-08-01"
        );
    }

    /// Verifies bulk snapshot mirroring only touches sessions with a loaded transcript window.
    /// 验证批量快照镜像只会处理已有已加载转录窗口的会话。
    #[test]
    fn snapshot_session_transcript_is_loaded_requires_messages_or_hydration() {
        let loaded_by_messages = SessionId::from_u128(401);
        let loaded_by_hydration = SessionId::from_u128(402);
        let not_loaded = SessionId::from_u128(403);
        let mut session_messages = BTreeMap::new();
        session_messages.insert(loaded_by_messages, Vec::new());
        let hydrated_session_ids = BTreeSet::from([loaded_by_hydration]);

        assert!(snapshot_session_transcript_is_loaded(
            loaded_by_messages,
            &session_messages,
            &hydrated_session_ids
        ));
        assert!(snapshot_session_transcript_is_loaded(
            loaded_by_hydration,
            &session_messages,
            &hydrated_session_ids
        ));
        assert!(!snapshot_session_transcript_is_loaded(
            not_loaded,
            &session_messages,
            &hydrated_session_ids
        ));
    }
}

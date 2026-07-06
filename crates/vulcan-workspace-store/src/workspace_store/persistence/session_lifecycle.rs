use super::*;

/// Persists one session addon document without rewriting transcript or runtime-state documents.
/// 持久化单个会话附属状态文档，且不重写转录或运行态文档。
///
/// # Parameters
/// 参数
/// `controller` is the workspace data-plane controller handle.
/// `controller` 是工作区数据平面控制器句柄。
/// `task_projects` are the in-memory project summaries used to resolve session ownership.
/// `task_projects` 是用于解析会话归属的内存项目摘要。
/// `session` is the session summary that owns the addon state.
/// `session` 是拥有附属状态的会话摘要。
/// `archived` describes whether the session lives in the archived tree.
/// `archived` 描述该会话是否位于归档树。
/// `addon_state` is the full namespace-split addon document to write.
/// `addon_state` 是要写入的完整命名空间拆分附属状态文档。
///
/// # Returns
/// 返回值
/// The merged addon document that was written successfully.
/// 成功写入后的合并附属状态文档。
pub async fn persist_single_session_addon_state_update(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    session: &SessionSummary,
    archived: bool,
    addon_state: &SessionAddonState,
) -> Result<SessionAddonState, String> {
    if addon_state.session_id != session.id {
        return Err(format!(
            "单会话附属状态保存失败：附属状态会话 `{}` 与目标会话 `{}` 不一致",
            addon_state.session_id, session.id
        ));
    }
    let session_owner_map = session_owner_map_from_projects(task_projects);
    let Some((project_id, _sort_order)) = session_owner_map.get(&session.id) else {
        return Err(format!(
            "单会话附属状态保存失败：未找到会话所属项目：{}",
            session.id
        ));
    };

    let session_id_text = session.id.to_string();
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    let session_dir =
        ensure_segmented_session_directory_for_locator(&controller, &session_id_text, &locator)?;
    let mut merged_state = read_session_addon_state(&session_dir)?
        .unwrap_or_else(|| SessionAddonState::new(session.id));
    for (namespace, value) in &addon_state.namespaces {
        let _ = merged_state.set_json_namespace(
            *namespace,
            value.value.clone(),
            value.updated_at.clone(),
        );
    }
    write_session_addon_state(&session_dir, &merged_state)?;
    Ok(merged_state)
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
    let created_date = persistence_segmented_session_date_from_unix_millis_text(&created_at);
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
pub(super) async fn archive_project_session_ids(
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
    Ok(())
}

/// Archives a segmented session directory using the persisted session locator when present.
/// 存在持久化会话定位信息时归档对应分段会话目录。
pub(super) async fn archive_segmented_session_directory_if_present(
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
    let created_date =
        persistence_segmented_session_date_from_unix_millis_text(&locator.created_at);
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
pub(super) fn archive_segmented_session_directory_for_project_if_present(
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
pub(super) fn archive_segmented_session_directory_for_project_and_summary_if_present(
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
    let created_date = persistence_segmented_session_date_from_unix_millis_text(created_at);
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
pub(super) fn restore_segmented_session_directory_for_project_if_present(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session: &SessionSummary,
) -> Result<(), String> {
    let restored_at = current_unix_millis_text();
    let created_at = normalized_session_created_at(session);
    let created_date = persistence_segmented_session_date_from_unix_millis_text(&created_at);
    let _ = restore_segmented_session_by_created_date(
        &controller.data_root,
        project_id,
        &created_date,
        &session.id.to_string(),
        &restored_at,
    )?;
    Ok(())
}

/// Deletes one segmented session directory from a persisted locator.
/// 按持久化定位信息删除一个分段会话目录。
pub(super) fn delete_segmented_session_directory_for_locator_if_present(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    locator: &PersistedSessionIndexLocator,
) -> Result<(), String> {
    let project_tag = if locator.project_tag.trim().is_empty() {
        locator.project_id.as_str()
    } else {
        locator.project_tag.as_str()
    };
    let created_date =
        persistence_segmented_session_date_from_unix_millis_text(&locator.created_at);
    let _ = delete_segmented_session_by_created_date(
        &controller.data_root,
        locator.archived,
        project_tag,
        &created_date,
        &session_id.to_string(),
    )?;
    Ok(())
}

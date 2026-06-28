use super::*;

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
    let active_owner_map = session_owner_map_from_projects(&snapshot.task_projects);
    let archived_owner_map = session_owner_map_from_projects(&snapshot.archived_task_projects);
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
        &active_owner_map,
        &snapshot.session_messages,
        &snapshot.session_runtime_state,
        &snapshot.archived_sessions,
        &archived_owner_map,
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
pub(super) async fn persist_session_index_snapshot(
    controller: &ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    let active_owner_map = session_owner_map_from_projects(&snapshot.task_projects);
    let archived_owner_map = session_owner_map_from_projects(&snapshot.archived_task_projects);
    for session in &snapshot.sessions {
        let (project_id, sort_order) =
            required_session_project_owner(&active_owner_map, session, "活动会话索引持久化")?;
        upsert_session_index_row(controller, session, &project_id, false, sort_order, None).await?;
    }
    for session in &snapshot.archived_sessions {
        let (project_id, sort_order) =
            required_session_project_owner(&archived_owner_map, session, "归档会话索引持久化")?;
        upsert_session_index_row(controller, session, &project_id, true, sort_order, None).await?;
    }
    Ok(())
}

/// Mirrors all loaded snapshot sessions into segmented append-only directories.
/// 将所有已加载的快照会话镜像到分段追加式目录。
pub(super) async fn persist_segmented_snapshot_session_stores(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    active_owner_map: &BTreeMap<SessionId, (String, usize)>,
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_owner_map: &BTreeMap<SessionId, (String, usize)>,
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    hydrated_session_ids: &BTreeSet<SessionId>,
    warnings: &mut Vec<String>,
) {
    persist_segmented_snapshot_session_group(
        controller,
        sessions,
        false,
        active_owner_map,
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
        archived_owner_map,
        archived_session_messages,
        archived_session_runtime_state,
        hydrated_session_ids,
        warnings,
    )
    .await;
}

/// Mirrors one active or archived snapshot session group into segmented storage.
/// 将一组活动或归档快照会话镜像到分段存储。
pub(super) async fn persist_segmented_snapshot_session_group(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    archived: bool,
    session_owner_map: &BTreeMap<SessionId, (String, usize)>,
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
        let stored_messages = session_messages.get(&session.id).unwrap_or(&empty_messages);
        let rebuilt_messages = runtime_state.map(|runtime_state| {
            effective_session_messages_for_persistence(stored_messages, runtime_state)
        });
        let effective_messages = rebuilt_messages
            .as_deref()
            .unwrap_or_else(|| stored_messages.as_slice());
        let Some((project_id, _sort_order)) = session_owner_map.get(&session.id) else {
            warnings.push(format!(
                "跳过缺少明确项目归属的分段会话同步：session={}",
                session.id
            ));
            continue;
        };
        if let Err(error) = persist_segmented_session_tail_update(
            controller,
            session,
            project_id,
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
pub(super) fn snapshot_session_transcript_is_loaded(
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
    let rebuilt_messages = runtime_state
        .map(|runtime_state| effective_session_messages_for_persistence(messages, runtime_state));
    let effective_messages = rebuilt_messages.as_deref().unwrap_or(messages);
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

/// Appends new visible transcript suffix rows for one existing segmented session.
/// 为一个既有分段会话追加新的可见转录尾部行。
pub async fn append_single_session_transcript_suffix(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }
    let Some(session_dir) =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id.to_string())?
    else {
        return Ok(());
    };
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    append_segmented_session_message_suffix(&session_dir, messages)?;
    Ok(())
}

/// Persists the complete visible transcript window without touching runtime state.
/// 持久化完整可见转录窗口，且不触碰运行态。
///
/// # Parameters
/// # 参数
/// `controller` is the workspace data-plane controller handle.
/// `controller` 是工作区数据平面控制器句柄。
/// `session_id` identifies the segmented session receiving the transcript update.
/// `session_id` 标识接收转录更新的分段会话。
/// `messages` is the complete visible transcript window retained by the WebUI.
/// `messages` 是 WebUI 当前保留的完整可见转录窗口。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after the transcript tail or replacement boundary is persisted.
/// 写入转录尾部或替换边界后返回 `Ok(())`。
pub async fn persist_single_session_transcript_messages(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }
    let Some(session_dir) =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id.to_string())?
    else {
        return Ok(());
    };
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    append_segmented_session_message_update(&session_dir, messages)?;
    Ok(())
}

/// Builds the visible transcript slice persisted together with runtime checkpoints.
/// 构建与运行态检查点一起持久化的可见转录切片。
///
/// # Parameters
/// # 参数
/// `stored_messages` is the WebUI display window already retained by the caller.
/// `stored_messages` 是调用方已保留的 WebUI 显示窗口。
/// `runtime_state` is the latest canonical runtime state whose display records should be upserted.
/// `runtime_state` 是需要增量合并显示记录的最新规范运行态。
///
/// # Returns
/// # 返回值
/// Returns display messages that preserve existing visible history while refreshing runtime-owned rows.
/// 返回既保留既有可见历史又刷新运行态显示行的消息列表。
pub(super) fn effective_session_messages_for_persistence(
    stored_messages: &[SessionMessageEntry],
    runtime_state: &SessionRuntimeStateEntry,
) -> Vec<SessionMessageEntry> {
    merge_session_messages_with_conversation_state_records(
        stored_messages,
        &runtime_state.conversation_state,
    )
}

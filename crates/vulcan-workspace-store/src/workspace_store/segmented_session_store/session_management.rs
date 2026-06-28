//! Archive, restore, delete, and task-session management.
//! 归档、恢复、删除与子任务会话管理。

use super::*;

/// Moves one active session directory into the archived tree and marks metadata archived.
/// 将一个活动会话目录移动到归档树，并在元数据中标记归档。
pub(in crate::workspace_store) fn archive_segmented_session(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<PathBuf, String> {
    // Resolve both sides with the same leaf layout so segment paths remain unchanged.
    // 使用相同叶子布局解析两侧路径，确保分段路径保持不变。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    // Resolve the archived destination before moving the directory.
    // 移动目录前解析归档目标路径。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    if let Some(parent) = archived_dir.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(&active_dir, &archived_dir).map_err(to_store_error)?;
    patch_segmented_session_metadata(
        &archived_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(archived_at.to_string()),
            archived_at: Some(Some(archived_at.to_string())),
            ..Default::default()
        },
    )?;
    Ok(archived_dir)
}

/// Finds and moves one active segmented session directory into the archived tree.
/// 查找并将一个活动分段会话目录移动到归档树。
pub(in crate::workspace_store) fn archive_segmented_session_by_id(
    data_root: &Path,
    project_tag: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(active_dir) =
        find_segmented_session_directory(data_root, false, project_tag, session_id)?
    else {
        return Ok(None);
    };
    let Some(leaf_name) = active_dir.file_name().map(|value| value.to_owned()) else {
        return Err(format!(
            "Segmented session directory has no leaf name: {}",
            active_dir.display()
        ));
    };
    let archived_dir = data_root
        .join(SEGMENTED_ARCHIVED_SESSIONS_DIR)
        .join(sanitize_file_component(project_tag))
        .join(leaf_name);
    move_segmented_session_directory(&active_dir, &archived_dir)?;
    patch_segmented_session_metadata(
        &archived_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(archived_at.to_string()),
            archived_at: Some(Some(archived_at.to_string())),
            ..Default::default()
        },
    )?;
    Ok(Some(archived_dir))
}

/// Moves one active segmented session directory into the archived tree by its exact storage date.
/// 根据精确存储日期将一个活动分段会话目录移动到归档树。
pub(in crate::workspace_store) fn archive_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<Option<PathBuf>, String> {
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    if !active_dir.exists() {
        return Ok(None);
    }
    archive_segmented_session(
        data_root,
        project_tag,
        created_date,
        session_id,
        archived_at,
    )
    .map(Some)
}

/// Moves one archived session directory back into the active tree and clears archived metadata.
/// 将一个归档会话目录移回活动树，并清除元数据中的归档标记。
pub(in crate::workspace_store) fn restore_segmented_session(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<PathBuf, String> {
    // Resolve both sides with the same leaf layout so segment paths remain unchanged.
    // 使用相同叶子布局解析两侧路径，确保分段路径保持不变。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    // Resolve the active destination before moving the directory.
    // 移动目录前解析活动目标路径。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    if let Some(parent) = active_dir.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(&archived_dir, &active_dir).map_err(to_store_error)?;
    patch_segmented_session_metadata(
        &active_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(restored_at.to_string()),
            archived_at: Some(None),
            ..Default::default()
        },
    )?;
    Ok(active_dir)
}

/// Finds and moves one archived segmented session directory back into the active tree.
/// 查找并将一个归档分段会话目录移回活动树。
pub(in crate::workspace_store) fn restore_segmented_session_by_id(
    data_root: &Path,
    project_tag: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(archived_dir) =
        find_segmented_session_directory(data_root, true, project_tag, session_id)?
    else {
        return Ok(None);
    };
    let Some(leaf_name) = archived_dir.file_name().map(|value| value.to_owned()) else {
        return Err(format!(
            "Segmented archived session directory has no leaf name: {}",
            archived_dir.display()
        ));
    };
    let active_dir = data_root
        .join(SEGMENTED_SESSIONS_DIR)
        .join(sanitize_file_component(project_tag))
        .join(leaf_name);
    move_segmented_session_directory(&archived_dir, &active_dir)?;
    patch_segmented_session_metadata(
        &active_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(restored_at.to_string()),
            archived_at: Some(None),
            ..Default::default()
        },
    )?;
    Ok(Some(active_dir))
}

/// Moves one archived segmented session directory back by its exact storage date.
/// 根据精确存储日期将一个归档分段会话目录移回活动树。
pub(in crate::workspace_store) fn restore_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<Option<PathBuf>, String> {
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    if !archived_dir.exists() {
        return Ok(None);
    }
    restore_segmented_session(
        data_root,
        project_tag,
        created_date,
        session_id,
        restored_at,
    )
    .map(Some)
}

/// Deletes one active or archived segmented session directory by session identifier.
/// 按会话标识删除一个活动或归档分段会话目录。
pub(in crate::workspace_store) fn delete_segmented_session_by_id(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    session_id: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory(data_root, archived, project_tag, session_id)?
    else {
        return Ok(false);
    };
    fs::remove_dir_all(&session_dir).map_err(to_store_error)?;
    Ok(true)
}

/// Deletes one segmented session directory by its exact storage date.
/// 根据精确存储日期删除一个分段会话目录。
pub(in crate::workspace_store) fn delete_segmented_session_by_created_date(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
) -> Result<bool, String> {
    let session_dir =
        segmented_session_directory(data_root, archived, project_tag, created_date, session_id);
    if !session_dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&session_dir).map_err(to_store_error)?;
    Ok(true)
}

/// Removes all active and archived segmented session roots below the Vulcan code data root.
/// 删除 Vulcan Code 数据根目录下全部活动与归档分段会话根目录。
pub(in crate::workspace_store) fn clear_all_segmented_session_roots(
    data_root: &Path,
) -> Result<(), String> {
    for root_name in [SEGMENTED_SESSIONS_DIR, SEGMENTED_ARCHIVED_SESSIONS_DIR] {
        let root = data_root.join(root_name);
        if root.exists() {
            fs::remove_dir_all(&root).map_err(to_store_error)?;
        }
        create_dir_all(&root)?;
    }
    Ok(())
}

/// Creates one child task session below a parent segmented session directory.
/// 在父分段会话目录下创建一个子任务会话。
pub(in crate::workspace_store) fn create_segmented_task_session(
    parent_session_dir: &Path,
    request: SegmentedTaskSessionCreateRequest,
) -> Result<PathBuf, String> {
    // Task creation appends a parent link and writes child metadata, so serialize it with session appends.
    // 子任务创建会追加父会话关联并写入子元数据，因此与会话追加串行化。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load parent metadata so task mirrors stay colocated with the parent directory index.
    // 加载父元数据，确保任务镜像与父目录索引保持一致。
    let mut parent_metadata = read_session_metadata(parent_session_dir)?;
    // Resolve the child task directory under the singular task directory required by the target layout.
    // 按目标布局要求，在单数 task 目录下解析子任务目录。
    let task_dir = parent_session_dir
        .join(SEGMENTED_TASK_DIR)
        .join(sanitize_file_component(&request.task_session_id));
    create_dir_all(&task_dir)?;

    // Build the first child segment before the canonical SubagentTaskMeta event is written.
    // 在写入规范 SubagentTaskMeta 事件前构建首个子任务分段。
    let first_segment = SegmentMetadata::empty(segment_file_name(1), 0);
    // Store child metadata with task-local sequence tracking.
    // 保存带有子任务局部序号跟踪的子元数据。
    let mut task_metadata = SegmentedTaskSessionMetadata {
        schema_version: 1,
        task_session_id: request.task_session_id.clone(),
        parent_session_id: parent_metadata.session_id.clone(),
        parent_project_tag: parent_metadata.project_tag.clone(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at.clone(),
        status: request.status.clone(),
        agent_kind: request.agent_kind.clone(),
        agent_profile: request.agent_profile.clone(),
        spawned_by: request.spawned_by.clone(),
        active_segment: first_segment.file.clone(),
        next_local_seq: 0,
        segment_policy: request.segment_policy,
        segments: vec![first_segment],
    };
    // The first child line is the required canonical SubagentTaskMeta event.
    // 子任务首行是要求的规范 SubagentTaskMeta 事件。
    let meta_payload = SubagentTaskMetadataPayload {
        task_session_id: request.task_session_id.clone(),
        parent_session_id: parent_metadata.session_id.clone(),
        parent_project_tag: parent_metadata.project_tag.clone(),
        agent_kind: request.agent_kind.clone(),
        agent_profile: request.agent_profile.clone(),
        spawned_by: request.spawned_by.clone(),
        created_at: request.created_at.clone(),
    };
    let meta_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SubagentTaskMeta,
        payload: serde_json::to_value(meta_payload).map_err(to_store_error)?,
        turn_index: None,
    };
    append_task_events_with_metadata(&task_dir, &mut task_metadata, &[meta_event])?;
    write_task_metadata(&task_dir, &task_metadata)?;
    write_state(
        &task_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: request.task_session_id.clone(),
            updated_at: request.created_at.clone(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        },
    )?;

    // Mirror lightweight task data into the parent metadata for routing and list views.
    // 将轻量任务数据镜像到父元数据中，供路由与列表视图使用。
    let task_relative_dir = format!(
        "{SEGMENTED_TASK_DIR}/{}",
        sanitize_file_component(&request.task_session_id)
    );
    // Parent JSONL receives a link event so transcript views can navigate to task details.
    // 父 JSONL 接收关联事件，让转录视图能够跳转到子任务详情。
    let parent_link_payload = json!({
        "task_session_id": request.task_session_id,
        "task_dir": task_relative_dir,
        "task_active_segment": format!("{}/{}", task_relative_dir, task_metadata.active_segment),
        "task_metadata_file": format!("{}/{}", task_relative_dir, SEGMENTED_METADATA_FILE),
        "agent_kind": request.agent_kind,
        "agent_profile": request.agent_profile,
        "parent_event_seq": request.parent_start_seq,
        "status": request.status
    });
    let parent_link_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SubagentTaskStarted,
        payload: parent_link_payload,
        turn_index: None,
    };
    let parent_link_lines = append_events_with_metadata(
        parent_session_dir,
        &mut parent_metadata,
        &[parent_link_event],
    )?;
    let parent_link_seq = parent_link_lines.first().map(|line| line.seq);
    let task_mirror = SubagentTaskMirror {
        session_id: request.task_session_id.clone(),
        dir: task_relative_dir.clone(),
        active_segment: format!("{}/{}", task_relative_dir, task_metadata.active_segment),
        metadata_file: format!("{}/{}", task_relative_dir, SEGMENTED_METADATA_FILE),
        agent_kind: request.agent_kind,
        agent_profile: request.agent_profile,
        parent_start_seq: parent_link_seq.or(request.parent_start_seq),
        parent_end_seq: None,
        status: request.status,
        line_count: task_metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at,
    };
    parent_metadata.tasks.push(task_mirror);
    parent_metadata.updated_at = task_metadata.updated_at.clone();
    write_metadata(parent_session_dir, &parent_metadata)?;
    Ok(task_dir)
}

/// Appends events to one child task session.
/// 向一个子任务会话追加事件。
pub(in crate::workspace_store) fn append_segmented_task_events(
    task_dir: &Path,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Child task JSONL appends share the process-local append lock with parent sessions.
    // 子任务 JSONL 追加与父会话共用进程内追加锁。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load task metadata first so task-local sequence numbers stay authoritative.
    // 先加载子任务元数据，确保子任务局部序号保持权威。
    let mut metadata = read_task_session_metadata(task_dir)?;
    // Append the requested events and then atomically overwrite task metadata.
    // 追加请求事件，然后原子覆盖子任务元数据。
    let lines = append_task_events_with_metadata(task_dir, &mut metadata, events)?;
    write_task_metadata(task_dir, &metadata)?;
    Ok(lines)
}

/// Records a parent-session link update for one child task and refreshes task metadata.
/// 为一个子任务记录父会话关联更新，并刷新任务元数据。
pub(in crate::workspace_store) fn record_segmented_task_link_update(
    parent_session_dir: &Path,
    update: SegmentedTaskLinkUpdate,
) -> Result<SegmentedSessionEventLine, String> {
    // Parent task-link updates append to the parent JSONL segment and mutate metadata together.
    // 父任务关联更新会同时追加父 JSONL 分段并修改元数据。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load parent metadata so the task mirror remains the routing source for this update.
    // 加载父元数据，确保任务镜像仍是本次更新的路由来源。
    let mut parent_metadata = read_session_metadata(parent_session_dir)?;
    let task_index = parent_metadata
        .tasks
        .iter()
        .position(|task| task.session_id == update.task_session_id)
        .ok_or_else(|| {
            format!(
                "Task `{}` is not mirrored in parent metadata.",
                update.task_session_id
            )
        })?;
    // Clone stable routing fields before mutating parent metadata through append.
    // 在通过 append 修改父元数据前克隆稳定路由字段。
    let task_dir_relative = parent_metadata.tasks[task_index].dir.clone();
    let task_metadata_file = parent_metadata.tasks[task_index].metadata_file.clone();
    let task_dir = parent_session_dir.join(&task_dir_relative);

    // Update child task metadata first; parent link event then points at the latest task status.
    // 先更新子任务元数据；随后父关联事件指向最新任务状态。
    let mut task_metadata = read_task_session_metadata(&task_dir)?;
    task_metadata.status = update.status.clone();
    task_metadata.updated_at = update.timestamp.clone();
    write_task_metadata(&task_dir, &task_metadata)?;
    let task_active_segment = format!("{}/{}", task_dir_relative, task_metadata.active_segment);

    // Terminal updates get the completed event kind; non-terminal updates remain progress links.
    // 终态更新使用 completed 事件类型；非终态更新保持进度关联事件。
    let event_kind = if update.terminal {
        SegmentedSessionEventKind::SubagentTaskCompleted
    } else {
        SegmentedSessionEventKind::SubagentTaskEventLinked
    };
    let parent_payload = json!({
        "task_session_id": update.task_session_id,
        "task_dir": task_dir_relative,
        "task_active_segment": task_active_segment,
        "task_metadata_file": task_metadata_file,
        "agent_kind": task_metadata.agent_kind.clone(),
        "agent_profile": task_metadata.agent_profile.clone(),
        "status": update.status,
        "terminal": update.terminal,
        "detail": update.payload
    });
    let event_input = SegmentedSessionEventInput {
        timestamp: update.timestamp.clone(),
        kind: event_kind,
        payload: parent_payload,
        turn_index: update.turn_index,
    };
    let appended_lines =
        append_events_with_metadata(parent_session_dir, &mut parent_metadata, &[event_input])?;
    let appended_line = appended_lines
        .into_iter()
        .next()
        .ok_or_else(|| "Task link update did not append any parent event.".to_string())?;

    // Update the parent task mirror after the event sequence is known.
    // 在事件序号确定后更新父任务镜像。
    if let Some(task_mirror) = parent_metadata.tasks.get_mut(task_index) {
        task_mirror.status = task_metadata.status.clone();
        task_mirror.updated_at = task_metadata.updated_at.clone();
        task_mirror.active_segment = task_active_segment.clone();
        task_mirror.line_count = task_metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum();
        if update.terminal {
            task_mirror.parent_end_seq = Some(appended_line.seq);
        }
    }
    parent_metadata.updated_at = task_metadata.updated_at.clone();
    write_metadata(parent_session_dir, &parent_metadata)?;
    Ok(appended_line)
}

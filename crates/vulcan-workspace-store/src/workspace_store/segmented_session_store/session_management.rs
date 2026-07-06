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

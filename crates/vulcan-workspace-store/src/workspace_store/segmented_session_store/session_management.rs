//! Archive, restore, delete, and task-session management.
//! 归档、恢复、删除与子任务会话管理。

use super::*;
use uuid::Uuid;

/// Moves one session directory, patches its metadata, and rolls both changes back on commit failure.
/// 移动一个会话目录并修补其元数据，在提交失败时回滚两项变更。
///
/// # Parameters
/// 参数
/// `source` is the current authoritative session directory.
/// `source` 是当前权威会话目录。
/// `destination` is the absent target directory in the other lifecycle tree.
/// `destination` 是另一生命周期树中必须不存在的目标目录。
/// `patch` is the lifecycle metadata update committed after the directory move.
/// `patch` 是目录移动后提交的生命周期元数据更新。
///
/// # Returns
/// 返回值
/// Returns success after move, metadata patch, and parent synchronization; otherwise restores source state.
/// 移动、元数据修补与父目录同步完成后返回成功；否则恢复源状态。
fn move_and_patch_segmented_session_directory(
    source: &Path,
    destination: &Path,
    patch: SegmentedSessionMetadataPatch,
) -> Result<(), String> {
    // original_metadata is the authoritative rollback document captured before any rename.
    // original_metadata 是任何重命名发生前捕获的权威回滚文档。
    let original_metadata = read_session_metadata(source)?;
    move_segmented_session_directory(source, destination)?;
    // commit_result covers every fallible operation after the directory becomes visible at destination.
    // commit_result 覆盖目录在目标位置可见后的全部可能失败操作。
    let commit_result = (|| -> Result<(), String> {
        patch_segmented_session_metadata(destination, patch)?;
        synchronize_segmented_session_directory_move(source, destination)
    })();
    let Err(commit_error) = commit_result else {
        return Ok(());
    };

    if let Err(rollback_move_error) = move_segmented_session_directory(destination, source) {
        return Err(format!(
            "Segmented session lifecycle commit failed: {commit_error}; directory rollback also failed: {rollback_move_error}"
        ));
    }
    if let Err(rollback_metadata_error) = write_metadata(source, &original_metadata) {
        return Err(format!(
            "Segmented session lifecycle commit failed: {commit_error}; directory was restored but metadata rollback failed: {rollback_metadata_error}"
        ));
    }
    if let Err(rollback_sync_error) =
        synchronize_segmented_session_directory_move(destination, source)
    {
        return Err(format!(
            "Segmented session lifecycle commit failed: {commit_error}; content was restored but rollback directory synchronization failed: {rollback_sync_error}"
        ));
    }
    Err(format!(
        "Segmented session lifecycle commit failed and was rolled back: {commit_error}"
    ))
}

/// Renames one active segmented session by its exact persisted directory identity.
/// 根据精确持久化目录身份重命名一个活动分段会话。
///
/// # Parameters
/// 参数
/// `data_root` is the workspace persistence root containing the active session tree.
/// `data_root` 是包含活动会话树的工作区持久化根。
/// `project_tag` is the exact persisted directory project tag.
/// `project_tag` 是精确持久化的目录项目标签。
/// `created_date` is the exact persisted session date component.
/// `created_date` 是精确持久化的会话日期片段。
/// `session_id` is the stable session directory identity.
/// `session_id` 是稳定会话目录身份。
/// `title` is the non-empty manual display title to publish.
/// `title` 是需要发布的非空手动展示标题。
/// `updated_at` is the non-empty timestamp committed with the title.
/// `updated_at` 是随标题提交的非空时间戳。
///
/// # Returns
/// 返回值
/// Returns the exact active directory after its metadata commit succeeds, or an explicit locator-state error.
/// 元数据提交成功后返回精确活动目录；定位状态异常时返回显式错误。
pub(in crate::workspace_store) fn rename_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    title: &str,
    updated_at: &str,
) -> Result<PathBuf, String> {
    if title.trim().is_empty() {
        return Err(String::from(
            "Segmented session rename title cannot be empty.",
        ));
    }
    if updated_at.trim().is_empty() {
        return Err(String::from(
            "Segmented session rename timestamp cannot be empty.",
        ));
    }

    // session_dir is the only active path derived from the authoritative persisted locator.
    // session_dir 是从权威持久化定位信息派生出的唯一活动路径。
    let session_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    if !segmented_session_directory_exists(&session_dir)? {
        return Err(format!(
            "Segmented session rename directory does not exist: {}",
            session_dir.display()
        ));
    }
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
    Ok(session_dir)
}

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
    move_and_patch_segmented_session_directory(
        &active_dir,
        &archived_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(archived_at.to_string()),
            archived_at: Some(Some(archived_at.to_string())),
            ..Default::default()
        },
    )?;
    Ok(archived_dir)
}

/// Moves one active segmented session directory into the archived tree by its exact storage date.
/// 根据精确存储日期将一个活动分段会话目录移动到归档树。
///
/// # Parameters
/// 参数
/// `data_root` is the workspace persistence root containing both lifecycle trees.
/// `data_root` 是包含两个生命周期树的工作区持久化根。
/// `project_tag` identifies the owning project directory.
/// `project_tag` 标识所属项目目录。
/// `created_date` is the exact persisted session date component.
/// `created_date` 是精确持久化会话日期片段。
/// `session_id` is the stable session directory identity.
/// `session_id` 是稳定会话目录身份。
/// `archived_at` is the timestamp committed to lifecycle metadata.
/// `archived_at` 是提交到生命周期元数据的时间戳。
///
/// # Returns
/// 返回值
/// Returns the archived directory after a normal move or interrupted-move recovery; rejects missing or ambiguous states.
/// 正常移动或中断移动恢复后返回归档目录；缺失或歧义状态会被拒绝。
pub(in crate::workspace_store) fn archive_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<PathBuf, String> {
    // active_dir is the authoritative source identity before archival.
    // active_dir 是归档前的权威源身份。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    // archived_dir is both the normal destination and interrupted-move recovery identity.
    // archived_dir 既是正常目标，也是中断移动的恢复身份。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    // active_exists and archived_exists establish the complete lifecycle state before mutation.
    // active_exists 与 archived_exists 在变更前建立完整生命周期状态。
    let active_exists = segmented_session_directory_exists(&active_dir)?;
    let archived_exists = segmented_session_directory_exists(&archived_dir)?;
    match (active_exists, archived_exists) {
        (false, false) => Err(format!(
            "Segmented session archive has neither active nor archived directory: active=`{}`, archived=`{}`",
            active_dir.display(),
            archived_dir.display()
        )),
        (true, true) => Err(format!(
            "Segmented session archive has both active and archived directories: active=`{}`, archived=`{}`",
            active_dir.display(),
            archived_dir.display()
        )),
        (true, false) => archive_segmented_session(
            data_root,
            project_tag,
            created_date,
            session_id,
            archived_at,
        ),
        (false, true) => {
            // Recommit lifecycle metadata because a crash may have happened after rename but before metadata publication.
            // 重新提交生命周期元数据，因为崩溃可能发生在重命名之后、元数据发布之前。
            patch_segmented_session_metadata(
                &archived_dir,
                SegmentedSessionMetadataPatch {
                    updated_at: Some(archived_at.to_string()),
                    archived_at: Some(Some(archived_at.to_string())),
                    ..Default::default()
                },
            )?;
            synchronize_segmented_session_directory_move(&active_dir, &archived_dir)?;
            Ok(archived_dir)
        }
    }
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
    move_and_patch_segmented_session_directory(
        &archived_dir,
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
///
/// # Parameters
/// 参数
/// `data_root` is the workspace persistence root containing both lifecycle trees.
/// `data_root` 是包含两个生命周期树的工作区持久化根。
/// `project_tag` identifies the owning project directory.
/// `project_tag` 标识所属项目目录。
/// `created_date` is the exact persisted session date component.
/// `created_date` 是精确持久化会话日期片段。
/// `session_id` is the stable session directory identity.
/// `session_id` 是稳定会话目录身份。
/// `restored_at` is the timestamp committed to lifecycle metadata.
/// `restored_at` 是提交到生命周期元数据的时间戳。
///
/// # Returns
/// 返回值
/// Returns the active directory after a normal move or interrupted-move recovery; rejects missing or ambiguous states.
/// 正常移动或中断移动恢复后返回活动目录；缺失或歧义状态会被拒绝。
pub(in crate::workspace_store) fn restore_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<PathBuf, String> {
    // archived_dir is the authoritative source identity before restoration.
    // archived_dir 是恢复前的权威源身份。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    // active_dir is both the normal destination and interrupted-move recovery identity.
    // active_dir 既是正常目标，也是中断移动的恢复身份。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    // archived_exists and active_exists establish the complete lifecycle state before mutation.
    // archived_exists 与 active_exists 在变更前建立完整生命周期状态。
    let archived_exists = segmented_session_directory_exists(&archived_dir)?;
    let active_exists = segmented_session_directory_exists(&active_dir)?;
    match (archived_exists, active_exists) {
        (false, false) => Err(format!(
            "Segmented session restore has neither archived nor active directory: archived=`{}`, active=`{}`",
            archived_dir.display(),
            active_dir.display()
        )),
        (true, true) => Err(format!(
            "Segmented session restore has both archived and active directories: archived=`{}`, active=`{}`",
            archived_dir.display(),
            active_dir.display()
        )),
        (true, false) => restore_segmented_session(
            data_root,
            project_tag,
            created_date,
            session_id,
            restored_at,
        ),
        (false, true) => {
            // Recommit lifecycle metadata because a crash may have happened after rename but before metadata publication.
            // 重新提交生命周期元数据，因为崩溃可能发生在重命名之后、元数据发布之前。
            patch_segmented_session_metadata(
                &active_dir,
                SegmentedSessionMetadataPatch {
                    updated_at: Some(restored_at.to_string()),
                    archived_at: Some(None),
                    ..Default::default()
                },
            )?;
            synchronize_segmented_session_directory_move(&archived_dir, &active_dir)?;
            Ok(active_dir)
        }
    }
}

/// Moves one archived session directory into a same-parent deletion staging path.
/// 将一个归档会话目录移动到同父级删除暂存路径。
///
/// # Parameters
/// 参数
/// `data_root` is the workspace persistence root containing the archived session tree.
/// `data_root` 是包含归档会话树的工作区持久化根目录。
/// `project_tag` is the exact persisted project directory tag.
/// `project_tag` 是精确持久化的项目目录标签。
/// `created_date` is the exact persisted session date component.
/// `created_date` 是精确持久化的会话日期片段。
/// `session_id` is the stable archived session directory identity.
/// `session_id` 是稳定的归档会话目录身份。
///
/// # Returns
/// 返回值
/// Returns the staged directory path after validating that only the archived lifecycle directory exists.
/// 仅在验证只有归档生命周期目录存在后返回删除暂存目录路径。
pub(in crate::workspace_store) fn stage_archived_segmented_session_deletion(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
) -> Result<PathBuf, String> {
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    let archived_exists = segmented_session_directory_exists(&archived_dir)?;
    let active_exists = segmented_session_directory_exists(&active_dir)?;
    if !archived_exists || active_exists {
        return Err(format!(
            "Segmented session permanent deletion requires only an archived directory: archived=`{}`, active=`{}`",
            archived_dir.display(),
            active_dir.display()
        ));
    }
    let staged_dir =
        archived_dir.with_file_name(format!(".{session_id}.deleting-{}", Uuid::new_v4()));
    move_segmented_session_directory(&archived_dir, &staged_dir)?;
    synchronize_segmented_session_directory_move(&archived_dir, &staged_dir)?;
    Ok(staged_dir)
}

/// Restores one staged archived session directory after an index deletion failure.
/// 在索引删除失败后恢复一个已暂存的归档会话目录。
///
/// # Parameters
/// 参数
/// `staged_dir` is the exact temporary directory returned by staging.
/// `staged_dir` 是暂存阶段返回的精确临时目录。
/// `archived_dir` is the original authoritative archived directory.
/// `archived_dir` 是原始权威归档目录。
///
/// # Returns
/// 返回值
/// Returns success after moving the directory back and synchronizing its parent directory.
/// 目录移回并同步父目录后返回成功。
pub(in crate::workspace_store) fn rollback_staged_archived_session_deletion(
    staged_dir: &Path,
    archived_dir: &Path,
) -> Result<(), String> {
    move_segmented_session_directory(staged_dir, archived_dir)?;
    synchronize_segmented_session_directory_move(staged_dir, archived_dir)
}

/// Permanently removes one staged archived session directory after its index row commits deletion.
/// 在索引行提交删除后永久移除一个归档会话删除暂存目录。
///
/// # Parameters
/// 参数
/// `staged_dir` is the exact temporary directory whose contents are no longer indexed.
/// `staged_dir` 是已不再被索引引用的精确临时目录。
///
/// # Returns
/// 返回值
/// Returns success only after the complete staged directory tree is removed.
/// 仅在完整删除暂存目录树后返回成功。
pub(in crate::workspace_store) fn finalize_staged_archived_session_deletion(
    staged_dir: &Path,
) -> Result<(), String> {
    if !segmented_session_directory_exists(staged_dir)? {
        return Err(format!(
            "Segmented session deletion staging directory does not exist: {}",
            staged_dir.display()
        ));
    }
    fs::remove_dir_all(staged_dir).map_err(to_store_error)?;
    synchronize_segmented_session_directory_move(staged_dir, staged_dir)
}

/// Returns whether one lifecycle participant is a real directory, rejecting links and non-directories.
/// 返回一个生命周期参与路径是否为真实目录，并拒绝链接与非目录。
///
/// # Parameters
/// 参数
/// `path` is the active or archived lifecycle directory path to inspect.
/// `path` 是要检查的活动或归档生命周期目录路径。
///
/// # Returns
/// 返回值
/// `true` for an existing real directory, `false` when absent, or an explicit error for another entry type.
/// 现有真实目录返回 `true`，缺失返回 `false`，其他目录项类型返回显式错误。
pub(in crate::workspace_store) fn segmented_session_directory_exists(
    path: &Path,
) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(format!(
            "Segmented session lifecycle path is not a real directory: {}",
            path.display()
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(to_store_error(error)),
    }
}

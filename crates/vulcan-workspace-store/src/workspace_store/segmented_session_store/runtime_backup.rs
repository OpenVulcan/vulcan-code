//! Runtime log compaction backups.
//! 运行时日志压缩备份。

use super::*;

pub(in crate::workspace_store) fn backup_runtime_log_before_compaction(
    session_dir: &Path,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    let metadata_path = runtime_metadata_path(session_dir);
    let index_path = runtime_index_path(session_dir);
    let runtime_dir = runtime_dir_path(session_dir);
    if !metadata_path.exists() && !index_path.exists() && !runtime_dir.exists() {
        return Ok(Vec::new());
    }

    let backup_root = runtime_backup_dir_path(session_dir);
    create_dir_all(&backup_root)?;
    let base_name = runtime_compaction_backup_dir_name(session_dir, compaction_id, timestamp);
    let mut backup_dir = backup_root.join(&base_name);
    if backup_dir.exists() {
        backup_dir = backup_root.join(format!("{base_name}-{}", uuid::Uuid::new_v4()));
    }
    create_dir_all(&backup_dir)?;

    let mut copied_files = Vec::new();
    copy_runtime_file_into_backup(session_dir, &metadata_path, &backup_dir, &mut copied_files)?;
    copy_runtime_file_into_backup(session_dir, &index_path, &backup_dir, &mut copied_files)?;
    if runtime_dir.exists() {
        copy_runtime_dir_into_backup(
            session_dir,
            &runtime_dir,
            &backup_dir.join(SEGMENTED_RUNTIME_DIR),
            &mut copied_files,
        )?;
    }
    prune_runtime_log_backups(session_dir, RUNTIME_COMPACTION_BACKUP_LIMIT)?;
    append_runtime_log_event_if_present(
        session_dir,
        timestamp,
        RuntimeLogEvent::ContextCompactionApplied(RuntimeContextCompactionAppliedEvent {
            compaction_id: compaction_id.to_string(),
            summary_message_id: None,
            backup_files: copied_files.clone(),
            compaction_fingerprint: Some(runtime_compaction_fingerprint(
                compaction_id,
                timestamp,
                &copied_files,
            )),
            merged_injection_fingerprint: None,
        }),
    )?;
    Ok(copied_files)
}

/// Returns the runtime backup directory path for one session directory.
/// 返回某个会话目录的运行时备份目录路径。
fn runtime_backup_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR)
}

/// Builds a compaction backup directory name with enough runtime scope to audit source coverage.
/// 构建包含足够运行时范围信息的压缩备份目录名，便于审计源文件覆盖情况。
fn runtime_compaction_backup_dir_name(
    session_dir: &Path,
    compaction_id: &str,
    timestamp: &str,
) -> String {
    let (segment_id, event_range) = runtime_compaction_backup_scope(session_dir)
        .unwrap_or_else(|| ("segment-unknown".to_string(), "seq-unknown".to_string()));
    format!(
        "compact-{}-{}-{}-{}",
        sanitize_file_component(timestamp),
        sanitize_file_component(compaction_id),
        sanitize_file_component(&segment_id),
        sanitize_file_component(&event_range)
    )
}

/// Reads the active runtime segment and sequence range used in one compaction backup name.
/// 读取用于单个压缩备份目录名的活动运行时分段与序号范围。
fn runtime_compaction_backup_scope(session_dir: &Path) -> Option<(String, String)> {
    let metadata = read_runtime_metadata(session_dir).ok()?;
    let active_segment = metadata
        .segments
        .iter()
        .find(|segment| segment.file == metadata.active_segment)
        .or_else(|| metadata.segments.last())?;
    let raw_segment_file = metadata
        .active_segment
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(metadata.active_segment.as_str());
    let segment_stem = raw_segment_file
        .strip_suffix(".jsonl")
        .unwrap_or(raw_segment_file);
    let end_seq = active_segment
        .end_seq
        .unwrap_or_else(|| metadata.next_seq.saturating_sub(1));
    let event_range = if active_segment.line_count == 0 || end_seq < active_segment.start_seq {
        "seq-empty".to_string()
    } else {
        format!("seq{}-{}", active_segment.start_seq, end_seq)
    };
    Some((format!("segment-{segment_stem}"), event_range))
}

/// Builds a stable fingerprint for one compaction audit boundary and copied runtime files.
/// 为单个压缩审计边界及已复制 runtime 文件构建稳定指纹。
fn runtime_compaction_fingerprint(
    compaction_id: &str,
    timestamp: &str,
    copied_files: &[String],
) -> String {
    let mut hasher = DefaultHasher::new();
    compaction_id.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    copied_files.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Copies one runtime file into a compaction backup directory when the source exists.
/// 当源文件存在时，将单个运行时文件复制到压缩备份目录。
fn copy_runtime_file_into_backup(
    session_dir: &Path,
    source: &Path,
    backup_dir: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    let file_name = source.file_name().ok_or_else(|| {
        format!(
            "Runtime backup source has no file name: {}",
            source.display()
        )
    })?;
    let destination = backup_dir.join(file_name);
    fs::copy(source, &destination).map_err(to_store_error)?;
    copied_files.push(relative_session_path(session_dir, &destination));
    Ok(())
}

/// Recursively copies the runtime segment directory into a compaction backup.
/// 将运行时分段目录递归复制到压缩备份中。
fn copy_runtime_dir_into_backup(
    session_dir: &Path,
    source_dir: &Path,
    destination_dir: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    create_dir_all(destination_dir)?;
    for entry in fs::read_dir(source_dir).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());
        let file_type = entry.file_type().map_err(to_store_error)?;
        if file_type.is_dir() {
            copy_runtime_dir_into_backup(
                session_dir,
                &source_path,
                &destination_path,
                copied_files,
            )?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(to_store_error)?;
            copied_files.push(relative_session_path(session_dir, &destination_path));
        }
    }
    Ok(())
}

/// Removes old runtime-log backups while keeping the newest requested count.
/// 删除旧的运行时日志备份，并保留指定数量的最新备份。
fn prune_runtime_log_backups(session_dir: &Path, keep: usize) -> Result<(), String> {
    let backup_root = runtime_backup_dir_path(session_dir);
    if !backup_root.exists() {
        return Ok(());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(&backup_root).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let file_type = entry.file_type().map_err(to_store_error)?;
        if !file_type.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        backups.push((modified, entry.path()));
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let prune_count = backups.len().saturating_sub(keep);
    for (_, path) in backups.into_iter().take(prune_count) {
        fs::remove_dir_all(path).map_err(to_store_error)?;
    }
    Ok(())
}

/// Formats a path relative to the session directory for compact audit messages.
/// 将路径格式化为相对会话目录的紧凑审计文本。
fn relative_session_path(session_dir: &Path, path: &Path) -> String {
    path.strip_prefix(session_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

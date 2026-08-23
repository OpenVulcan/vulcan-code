//! Runtime log compaction backups.
//! 运行时日志压缩备份。

use super::*;

/// File name of the exact ordered runtime-backup directory index.
/// 精确有序运行时备份目录索引的文件名。
const RUNTIME_BACKUP_INDEX_FILE: &str = "index.json";

/// Ordered runtime-backup directory names owned by one session.
/// 单个会话拥有的有序运行时备份目录名。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct RuntimeBackupDirectoryIndex {
    /// Directory names in creation order, oldest first.
    /// 按创建顺序排列的目录名，最旧项在前。
    directories: Vec<String>,
}

/// Backs up one validated runtime-log snapshot before context compaction.
/// 在上下文压缩前备份一份已验证的运行时日志快照。
///
/// # Parameters
/// 参数
/// - `session_dir`: Session directory that owns the runtime log and backup root.
/// - `session_dir`：拥有运行时日志与备份根目录的会话目录。
/// - `compaction_id`: Stable identifier of the pending context compaction.
/// - `compaction_id`：待执行上下文压缩的稳定标识符。
/// - `timestamp`: Timestamp shared by the backup name and audit event.
/// - `timestamp`：备份名称与审计事件共用的时间戳。
///
/// # Returns
/// 返回值
/// Returns copied session-relative paths, an empty list for consistent runtime absence, or a
/// strict error. A completed backup is intentionally retained if later audit or pruning fails.
/// 返回已复制的会话相对路径；运行时一致缺失时返回空列表；其他情况返回严格错误。若后续审计或
/// 清理失败，已完成的备份会被有意保留。
pub(in crate::workspace_store) fn backup_runtime_log_before_compaction(
    session_dir: &Path,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    // append_guard keeps validation, copying, and the audit append on one immutable source state.
    // append_guard 让验证、复制与审计追加共享同一个不可变源状态。
    let append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(|error| format!("Failed to lock runtime log for compaction backup: {error}"))?;
    // snapshot is the only authoritative source for backup naming and audit sequencing.
    // snapshot 是备份命名与审计序号的唯一权威来源。
    let Some(snapshot) = read_runtime_log_snapshot_with_owned_lock(session_dir, &append_guard)?
    else {
        return Ok(Vec::new());
    };

    // backup_root groups the bounded compaction history for this session.
    // backup_root 汇集当前会话有界保留的压缩历史。
    let backup_root = runtime_backup_dir_path(session_dir);
    create_dir_all(&backup_root)?;
    // base_name is derived only from metadata already validated against the segment bytes.
    // base_name 仅从已经与分段字节互证的元数据派生。
    let base_name = runtime_compaction_backup_dir_name(&snapshot, compaction_id, timestamp)?;
    // backup_dir is created atomically, avoiding a separate existence check race.
    // backup_dir 通过原子创建得到，避免单独存在性检查产生竞态。
    let backup_dir = create_unique_runtime_backup_dir(&backup_root, &base_name)?;

    // copied_files records only artifacts that were durably copied into the completed backup.
    // copied_files 仅记录已经持久复制到完整备份中的工件。
    let copied_files = match copy_validated_runtime_snapshot(session_dir, &backup_dir) {
        Ok(copied_files) => copied_files,
        Err(copy_error) => {
            return Err(cleanup_incomplete_runtime_backup(&backup_dir, copy_error));
        }
    };
    // audit_event binds the exact copied artifact set to the same validated append transaction.
    // audit_event 将精确复制的工件集合绑定到同一个已验证追加事务。
    let audit_event =
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
        });
    if let Err(error) = append_runtime_log_event_to_snapshot_with_owned_lock(
        session_dir,
        snapshot,
        timestamp,
        audit_event,
        &append_guard,
    ) {
        return Err(format!(
            "Runtime backup completed at {}, but audit append failed: {error}",
            backup_dir.display()
        ));
    }
    record_runtime_backup_directory(session_dir, &backup_dir)?;
    if let Err(error) = prune_runtime_log_backups(session_dir, RUNTIME_COMPACTION_BACKUP_LIMIT) {
        return Err(format!(
            "Runtime backup completed at {}, but pruning failed: {error}",
            backup_dir.display()
        ));
    }
    Ok(copied_files)
}

/// Returns the runtime backup directory path for one session directory.
/// 返回某个会话目录的运行时备份目录路径。
///
/// # Parameters
/// 参数
/// - `session_dir`: Owning session directory.
/// - `session_dir`：所属会话目录。
///
/// # Returns
/// 返回值
/// The session-local runtime backup root path.
/// 返回会话内的运行时备份根目录路径。
fn runtime_backup_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR)
}

/// Builds a backup directory name from one already validated runtime snapshot.
/// 从一份已经验证的运行时快照构建备份目录名。
///
/// # Parameters
/// 参数
/// - `snapshot`: Runtime state validated while the append lock is held.
/// - `snapshot`：持有追加锁期间验证通过的运行时状态。
/// - `compaction_id`: Stable identifier of the pending compaction.
/// - `compaction_id`：待执行压缩的稳定标识符。
/// - `timestamp`: Timestamp included in the directory name.
/// - `timestamp`：写入目录名的时间戳。
///
/// # Returns
/// 返回值
/// A filesystem-safe directory name describing the exact segment and sequence range, or a
/// strict path-encoding error.
/// 返回描述精确分段与序号范围的文件系统安全目录名，或严格的路径编码错误。
fn runtime_compaction_backup_dir_name(
    snapshot: &RuntimeLogSnapshot<'_>,
    compaction_id: &str,
    timestamp: &str,
) -> Result<String, String> {
    // segment is unique because snapshot validation enforces the single-segment contract.
    // segment 是唯一分段，因为快照验证会强制执行单分段契约。
    let segment = &snapshot.metadata.segments[0];
    // segment_stem is fixed by the validated contract and kept explicit for audit readability.
    // segment_stem 由已验证契约固定，并显式保留以提高审计可读性。
    let segment_stem = Path::new(&segment.file)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            format!(
                "Validated runtime segment path has no UTF-8 file stem: {}",
                segment.file
            )
        })?;
    // event_range is exact because metadata and physical lines have already been cross-checked.
    // event_range 是精确范围，因为元数据与物理日志行已经交叉校验。
    let event_range = match segment.end_seq {
        Some(end_seq) => format!("seq{}-{end_seq}", segment.start_seq),
        None => "seq-empty".to_string(),
    };
    Ok(format!(
        "compact-{}-{}-segment-{}-{}",
        sanitize_file_component(timestamp),
        sanitize_file_component(compaction_id),
        sanitize_file_component(segment_stem),
        sanitize_file_component(&event_range)
    ))
}

/// Creates one unique backup directory without a check-then-create race.
/// 在没有先检查后创建竞态的情况下创建唯一备份目录。
///
/// # Parameters
/// 参数
/// - `backup_root`: Existing session backup root.
/// - `backup_root`：已经存在的会话备份根目录。
/// - `base_name`: Preferred deterministic backup directory name.
/// - `base_name`：首选的确定性备份目录名。
///
/// # Returns
/// 返回值
/// The atomically created backup directory path.
/// 返回原子创建成功的备份目录路径。
fn create_unique_runtime_backup_dir(
    backup_root: &Path,
    base_name: &str,
) -> Result<PathBuf, String> {
    // preferred_path preserves deterministic naming when no prior backup owns the name.
    // preferred_path 在没有既有备份占用名称时保留确定性命名。
    let preferred_path = backup_root.join(base_name);
    match fs::create_dir(&preferred_path) {
        Ok(()) => Ok(preferred_path),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // unique_path resolves a real name collision without weakening any other error.
            // unique_path 仅解决真实名称冲突，不弱化其他错误。
            let unique_path = backup_root.join(format!("{base_name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&unique_path).map_err(|unique_error| {
                format!(
                    "Failed to create unique runtime backup directory {}: {unique_error}",
                    unique_path.display()
                )
            })?;
            Ok(unique_path)
        }
        Err(error) => Err(format!(
            "Failed to create runtime backup directory {}: {error}",
            preferred_path.display()
        )),
    }
}

/// Copies the exact persisted artifacts represented by one validated runtime snapshot.
/// 复制一份已验证运行时快照所表示的精确持久化工件。
///
/// # Parameters
/// 参数
/// - `session_dir`: Source session directory.
/// - `session_dir`：源会话目录。
/// - `backup_dir`: Newly created destination backup directory.
/// - `backup_dir`：新创建的目标备份目录。
///
/// # Returns
/// 返回值
/// Session-relative paths of copied required artifacts and the optional rebuildable index.
/// 返回已复制必需工件及可选可重建索引的会话相对路径。
fn copy_validated_runtime_snapshot(
    session_dir: &Path,
    backup_dir: &Path,
) -> Result<Vec<String>, String> {
    // copied_files becomes the canonical ordered artifact list stored in the audit event.
    // copied_files 将成为审计事件中保存的规范有序工件列表。
    let mut copied_files = Vec::new();
    copy_required_runtime_file(
        session_dir,
        &runtime_metadata_path(session_dir),
        &backup_dir.join(SEGMENTED_RUNTIME_METADATA_FILE),
        &mut copied_files,
    )?;
    copy_optional_runtime_file(
        session_dir,
        &runtime_index_path(session_dir),
        &backup_dir.join(SEGMENTED_RUNTIME_INDEX_FILE),
        &mut copied_files,
    )?;
    // backup_runtime_dir contains only the implemented fixed runtime segment, never unrelated files.
    // backup_runtime_dir 只包含已实现的固定运行时分段，绝不递归带入无关文件。
    let backup_runtime_dir = backup_dir.join(SEGMENTED_RUNTIME_DIR);
    fs::create_dir(&backup_runtime_dir).map_err(|error| {
        format!(
            "Failed to create runtime backup segment directory {}: {error}",
            backup_runtime_dir.display()
        )
    })?;
    // segment_relative_path is fixed by the validated single-segment runtime contract.
    // segment_relative_path 由已验证的单分段运行时契约固定。
    let segment_relative_path = runtime_segment_file_name();
    copy_required_runtime_file(
        session_dir,
        &session_dir.join(&segment_relative_path),
        &backup_dir.join(&segment_relative_path),
        &mut copied_files,
    )?;
    Ok(copied_files)
}

/// Copies one required runtime artifact through exclusive destination creation and data sync.
/// 通过目标独占创建与数据同步复制一个必需运行时工件。
///
/// # Parameters
/// 参数
/// - `session_dir`: Session root used to format the copied path.
/// - `session_dir`：用于格式化复制路径的会话根目录。
/// - `source`: Required source artifact path.
/// - `source`：必需源工件路径。
/// - `destination`: New destination artifact path.
/// - `destination`：新的目标工件路径。
/// - `copied_files`: Ordered output list updated after durable copy completion.
/// - `copied_files`：持久复制完成后更新的有序输出列表。
///
/// # Returns
/// 返回值
/// Success after the destination data is synchronized, otherwise a path-aware error.
/// 目标数据同步后返回成功，否则返回包含路径的错误。
fn copy_required_runtime_file(
    session_dir: &Path,
    source: &Path,
    destination: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    // source_file opening is the authoritative existence and readability decision.
    // source_file 的打开结果是存在性与可读性的权威判定。
    let source_file = File::open(source).map_err(|error| {
        format!(
            "Failed to open required runtime backup source {}: {error}",
            source.display()
        )
    })?;
    copy_open_runtime_file(session_dir, source_file, destination, copied_files)
}

/// Copies the rebuildable runtime index when it exists and rejects every other open failure.
/// 在可重建运行时索引存在时复制它，并拒绝其他所有打开失败。
///
/// # Parameters
/// 参数
/// - `session_dir`: Session root used to format the copied path.
/// - `session_dir`：用于格式化复制路径的会话根目录。
/// - `source`: Optional rebuildable index source path.
/// - `source`：可选的可重建索引源路径。
/// - `destination`: New destination index path.
/// - `destination`：新的目标索引路径。
/// - `copied_files`: Ordered output list updated only when the index is copied.
/// - `copied_files`：仅在索引被复制时更新的有序输出列表。
///
/// # Returns
/// 返回值
/// Success for source absence or a synchronized copy, otherwise a path-aware error.
/// 源文件缺失或同步复制完成时返回成功，否则返回包含路径的错误。
fn copy_optional_runtime_file(
    session_dir: &Path,
    source: &Path,
    destination: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    // source_file distinguishes only the explicitly allowed missing-index state.
    // source_file 只区分契约明确允许的索引缺失状态。
    let source_file = match File::open(source) {
        Ok(source_file) => source_file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to open optional runtime backup source {}: {error}",
                source.display()
            ));
        }
    };
    copy_open_runtime_file(session_dir, source_file, destination, copied_files)
}

/// Streams one already-open source into a newly created destination and synchronizes its data.
/// 将一个已经打开的源文件流式写入新建目标并同步其数据。
///
/// # Parameters
/// 参数
/// - `session_dir`: Session root used to format the copied path.
/// - `session_dir`：用于格式化复制路径的会话根目录。
/// - `source_file`: Already-open authoritative source handle.
/// - `source_file`：已经打开的权威源文件句柄。
/// - `destination`: Destination path that must not already exist.
/// - `destination`：不得预先存在的目标路径。
/// - `copied_files`: Ordered output list updated after synchronization.
/// - `copied_files`：同步后更新的有序输出列表。
///
/// # Returns
/// 返回值
/// Success after data synchronization and path recording, otherwise a strict error.
/// 数据同步并记录路径后返回成功，否则返回严格错误。
fn copy_open_runtime_file(
    session_dir: &Path,
    mut source_file: File,
    destination: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    // destination_file uses create_new so backup code can never overwrite an existing artifact.
    // destination_file 使用 create_new，确保备份代码绝不覆盖既有工件。
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            format!(
                "Failed to create runtime backup destination {}: {error}",
                destination.display()
            )
        })?;
    std::io::copy(&mut source_file, &mut destination_file).map_err(|error| {
        format!(
            "Failed to copy runtime backup destination {}: {error}",
            destination.display()
        )
    })?;
    destination_file.sync_all().map_err(|error| {
        format!(
            "Failed to synchronize runtime backup destination {}: {error}",
            destination.display()
        )
    })?;
    copied_files.push(relative_session_path(session_dir, destination)?);
    Ok(())
}

/// Removes an incomplete backup and preserves both the primary and cleanup failures.
/// 删除不完整备份，并同时保留主失败与清理失败信息。
///
/// # Parameters
/// 参数
/// - `backup_dir`: Incomplete backup directory to remove.
/// - `backup_dir`：需要删除的不完整备份目录。
/// - `copy_error`: Primary copy failure that triggered cleanup.
/// - `copy_error`：触发清理的主要复制失败。
///
/// # Returns
/// 返回值
/// The primary error enriched with cleanup status.
/// 返回附带清理状态的主要错误。
fn cleanup_incomplete_runtime_backup(backup_dir: &Path, copy_error: String) -> String {
    match fs::remove_dir_all(backup_dir) {
        Ok(()) => format!(
            "Runtime backup failed and incomplete directory {} was removed: {copy_error}",
            backup_dir.display()
        ),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => format!(
            "Runtime backup failed and incomplete directory {} was already absent: {copy_error}",
            backup_dir.display()
        ),
        Err(cleanup_error) => format!(
            "Runtime backup failed: {copy_error}; additionally failed to remove incomplete directory {}: {cleanup_error}",
            backup_dir.display()
        ),
    }
}

/// Builds a stable fingerprint for one compaction audit boundary and copied runtime files.
/// 为单个压缩审计边界及已复制运行时文件构建稳定指纹。
///
/// # Parameters
/// 参数
/// - `compaction_id`: Stable compaction identifier.
/// - `compaction_id`：稳定压缩标识符。
/// - `timestamp`: Timestamp shared by backup and audit.
/// - `timestamp`：备份与审计共用的时间戳。
/// - `copied_files`: Ordered copied artifact paths.
/// - `copied_files`：有序的已复制工件路径。
///
/// # Returns
/// 返回值
/// A deterministic lowercase hexadecimal fingerprint.
/// 返回确定性的小写十六进制指纹。
fn runtime_compaction_fingerprint(
    compaction_id: &str,
    timestamp: &str,
    copied_files: &[String],
) -> String {
    // hasher covers the complete logical audit boundary in canonical field order.
    // hasher 按规范字段顺序覆盖完整逻辑审计边界。
    let mut hasher = DefaultHasher::new();
    compaction_id.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    copied_files.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Removes old runtime-log backups while keeping the newest requested count.
/// 删除旧的运行时日志备份，并保留指定数量的最新备份。
///
/// # Parameters
/// 参数
/// - `session_dir`: Session directory that owns the backup root.
/// - `session_dir`：拥有备份根目录的会话目录。
/// - `keep`: Maximum number of newest backup directories to retain.
/// - `keep`：需要保留的最新备份目录最大数量。
///
/// # Returns
/// 返回值
/// Success for an absent root or completed pruning, otherwise a strict filesystem error.
/// 备份根目录缺失或清理完成时返回成功，否则返回严格文件系统错误。
fn prune_runtime_log_backups(session_dir: &Path, keep: usize) -> Result<(), String> {
    let backup_root = runtime_backup_dir_path(session_dir);
    let index_path = backup_root.join(RUNTIME_BACKUP_INDEX_FILE);
    let mut index = read_runtime_backup_directory_index(&index_path)?;
    let prune_count = index.directories.len().saturating_sub(keep);
    let expired = index.directories.drain(..prune_count).collect::<Vec<_>>();
    for directory_name in expired {
        validate_runtime_backup_directory_name(&directory_name)?;
        let path = backup_root.join(directory_name);
        fs::remove_dir_all(&path).map_err(|error| {
            format!(
                "Failed to remove expired runtime backup {}: {error}",
                path.display()
            )
        })?;
    }
    write_runtime_backup_directory_index(&index_path, &index)
}

/// Records one completed backup directory in the exact ordered index.
/// 在精确有序索引中记录一个已完成备份目录。
fn record_runtime_backup_directory(session_dir: &Path, backup_dir: &Path) -> Result<(), String> {
    let backup_root = runtime_backup_dir_path(session_dir);
    let directory_name = backup_dir
        .strip_prefix(&backup_root)
        .ok()
        .and_then(|relative| relative.to_str())
        .ok_or_else(|| String::from("Runtime backup directory is not a direct UTF-8 child."))?;
    validate_runtime_backup_directory_name(directory_name)?;
    let index_path = backup_root.join(RUNTIME_BACKUP_INDEX_FILE);
    let mut index = read_runtime_backup_directory_index(&index_path)?;
    if !index
        .directories
        .iter()
        .any(|current| current == directory_name)
    {
        index.directories.push(directory_name.to_string());
    }
    write_runtime_backup_directory_index(&index_path, &index)
}

/// Reads the exact runtime-backup index or returns an empty first-generation index.
/// 读取精确运行时备份索引，索引尚未创建时返回空的第一代索引。
fn read_runtime_backup_directory_index(
    index_path: &Path,
) -> Result<RuntimeBackupDirectoryIndex, String> {
    match fs::read(index_path) {
        Ok(encoded) => serde_json::from_slice(&encoded).map_err(|error| {
            format!(
                "Failed to decode runtime backup index {}: {error}",
                index_path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(RuntimeBackupDirectoryIndex::default())
        }
        Err(error) => Err(format!(
            "Failed to read runtime backup index {}: {error}",
            index_path.display()
        )),
    }
}

/// Atomically writes the exact runtime-backup index.
/// 原子写入精确运行时备份索引。
fn write_runtime_backup_directory_index(
    index_path: &Path,
    index: &RuntimeBackupDirectoryIndex,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(index)
        .map_err(|error| format!("Failed to serialize runtime backup index: {error}"))?;
    write_file_bytes_atomic(index_path, &encoded, "runtime backup index")
}

/// Rejects any indexed backup name that could escape the direct backup root.
/// 拒绝任何可能逃逸直接备份根目录的索引备份名称。
fn validate_runtime_backup_directory_name(directory_name: &str) -> Result<(), String> {
    if directory_name.is_empty()
        || directory_name.contains('/')
        || directory_name.contains('\\')
        || directory_name == "."
        || directory_name == ".."
    {
        return Err(String::from(
            "Runtime backup index contains an invalid directory name.",
        ));
    }
    Ok(())
}

/// Formats a strict session-relative path for compact audit messages.
/// 为紧凑审计消息格式化严格的会话相对路径。
///
/// # Parameters
/// 参数
/// - `session_dir`: Required path prefix and session root.
/// - `session_dir`：必需路径前缀与会话根目录。
/// - `path`: Descendant artifact path to format.
/// - `path`：需要格式化的后代工件路径。
///
/// # Returns
/// 返回值
/// A forward-slash session-relative path or an explicit ownership error.
/// 返回使用正斜杠的会话相对路径，或明确的归属错误。
fn relative_session_path(session_dir: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(session_dir)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|error| {
            format!(
                "Runtime backup path {} is not owned by session directory {}: {error}",
                path.display(),
                session_dir.display()
            )
        })
}

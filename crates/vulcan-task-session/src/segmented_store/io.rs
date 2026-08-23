use super::*;
#[cfg(unix)]
use crate::atomic_file::synchronize_parent_directory;
use crate::atomic_file::write_file_atomically;
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};

/// Retains weak per-document locks for JSON read-modify-write transactions.
/// 为 JSON 读改写事务保留按文档划分的弱引用锁。
static JSON_FILE_TRANSACTION_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Allocates process-local suffixes for complete task-directory staging layouts.
/// 为完整子任务目录暂存布局分配进程内后缀。
static TASK_DIRECTORY_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Resolves the shared transaction lock for one exact JSON document path.
/// 解析单个精确 JSON 文档路径的共享事务锁。
///
/// # Parameters
/// 参数
/// `path` is the final JSON document path used as transaction identity.
/// `path` 是作为事务身份的最终 JSON 文档路径。
///
/// # Returns
/// 返回值
/// A shared mutex that callers retain across the complete read-modify-write cycle.
/// 返回一个由调用方在完整读改写周期内保留的共享互斥锁。
pub(super) fn json_file_transaction_lock(path: &Path) -> Result<Arc<Mutex<()>>, SessionStoreError> {
    // registry holds only weak references so inactive document paths do not accumulate forever.
    // registry 仅保存弱引用，使非活跃文档路径不会永久积累。
    let mut registry = JSON_FILE_TRANSACTION_LOCKS.lock().map_err(to_store_error)?;
    registry.retain(|_, transaction_lock| transaction_lock.strong_count() > 0);
    if let Some(transaction_lock) = registry.get(path).and_then(Weak::upgrade) {
        return Ok(transaction_lock);
    }

    // transaction_lock is published while the registry is still locked, preventing duplicate mutexes.
    // transaction_lock 在注册表仍被锁定时发布，防止创建重复互斥锁。
    let transaction_lock = Arc::new(Mutex::new(()));
    registry.insert(path.to_path_buf(), Arc::downgrade(&transaction_lock));
    Ok(transaction_lock)
}

/// Mutable segment append target owned by one metadata document.
/// 由单个元数据文档持有的可变分段追加目标。
pub(super) struct SegmentEventAppendTarget<'a> {
    /// Directory containing segment JSONL files.
    /// 包含分段 JSONL 文件的目录。
    pub(super) dir: &'a Path,
    /// Mutable segment metadata list.
    /// 可变分段元数据列表。
    pub(super) segments: &'a mut Vec<SegmentMetadata>,
    /// Active segment file name.
    /// 当前活跃分段文件名。
    pub(super) active_segment: &'a mut String,
    /// Next event sequence number.
    /// 下一条事件序号。
    pub(super) next_seq: &'a mut u64,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub(super) segment_policy: &'a SegmentPolicy,
}

/// Event payload appended into one segment.
/// 追加到单个分段中的事件载荷。
pub(super) struct SegmentEventAppendRequest {
    /// Stable event kind.
    /// 稳定事件类型。
    pub(super) kind: String,
    /// Event timestamp text.
    /// 事件时间戳文本。
    pub(super) timestamp: String,
    /// Event JSON payload.
    /// 事件 JSON 载荷。
    pub(super) payload: Value,
    /// Optional turn index associated with the event.
    /// 与事件关联的可选回合索引。
    pub(super) turn_index: Option<i64>,
}

pub(super) fn append_segment_event(
    target: SegmentEventAppendTarget<'_>,
    request: SegmentEventAppendRequest,
) -> Result<SegmentedEventLine, SessionStoreError> {
    let SegmentEventAppendTarget {
        dir,
        segments,
        active_segment,
        next_seq,
        segment_policy,
    } = target;
    let SegmentEventAppendRequest {
        kind,
        timestamp,
        payload,
        turn_index,
    } = request;
    if segments.is_empty() {
        segments.push(SegmentMetadata::empty("s001.jsonl", *next_seq));
        *active_segment = "s001.jsonl".to_string();
    }
    let line = SegmentedEventLine {
        seq: *next_seq,
        timestamp,
        kind,
        payload,
        turn_index,
    };
    let encoded = serde_json::to_string(&line).map_err(to_store_error)?;
    let encoded_len = encoded.len() as u64 + 1;
    if should_roll_segment(
        segments.last().expect("segment exists"),
        segment_policy,
        encoded_len,
    ) {
        let next_file = segment_file_name(segments.len() + 1);
        segments.push(SegmentMetadata::empty(next_file.clone(), *next_seq));
        *active_segment = next_file;
    }
    let active_path = dir.join(active_segment.as_str());
    let byte_len = append_encoded_json_line(&active_path, &encoded)?;
    if let Some(segment) = segments.last_mut() {
        segment.line_count = segment.line_count.saturating_add(1);
        segment.byte_len = segment.byte_len.saturating_add(byte_len);
        if segment.first_timestamp.is_none() {
            segment.first_timestamp = Some(line.timestamp.clone());
        }
        segment.last_timestamp = Some(line.timestamp.clone());
        if let Some(turn_index) = turn_index {
            if segment.first_turn_index.is_none() {
                segment.first_turn_index = Some(turn_index);
            }
            segment.last_turn_index = Some(turn_index);
        }
        if line.kind == "compacted" {
            segment.last_compaction_seq = Some(line.seq);
        }
    }
    *next_seq = next_seq.saturating_add(1);
    Ok(line)
}

/// Validates one exact segmented append boundary without replaying any JSONL history.
/// 在不重放任何 JSONL 历史的情况下校验一个精确分段追加边界。
///
/// # Parameters
/// 参数
/// `dir` is the exact session directory, `active_segment` and `segments` are the retained
/// metadata locator, and `next_seq` is the next sequence declared by that metadata.
/// `dir` 是精确会话目录，`active_segment` 与 `segments` 是保留的元数据定位信息，
/// `next_seq` 是该元数据声明的下一序号。
///
/// # Returns
/// 返回值
/// Success when the active file length, newline boundary, and constant-time sequence arithmetic agree.
/// 当活动文件长度、换行边界与常数时间序号计算一致时返回成功。
pub(super) fn validate_segmented_append_boundary_locked(
    dir: &Path,
    active_segment: &str,
    segments: &[SegmentMetadata],
    next_seq: u64,
) -> Result<(), SessionStoreError> {
    // activeMetadata is the only physical segment that an append may touch.
    // activeMetadata 是一次追加可能接触的唯一物理分段。
    let active_metadata = segments.last().ok_or_else(|| {
        SessionStoreError::new(
            "Segmented append metadata has no active segment; load-time repair is required.",
        )
    })?;
    if active_segment != active_metadata.file {
        return Err(SessionStoreError::new(format!(
            "Segmented append metadata points to `{active_segment}` but the exact final segment is `{}`; load-time repair is required.",
            active_metadata.file
        )));
    }
    // activePath is resolved directly from retained metadata; no directory enumeration is allowed.
    // activePath 直接由保留元数据解析；不允许目录枚举。
    let active_path = dir.join(&active_metadata.file);
    // actualByteLen is authoritative for the exact append target.
    // actualByteLen 是精确追加目标的权威长度。
    let actual_byte_len = fs::metadata(&active_path).map_err(to_store_error)?.len();
    if actual_byte_len != active_metadata.byte_len {
        return Err(SessionStoreError::new(format!(
            "Exact active segment `{}` has byte length {actual_byte_len}, expected {}; load-time repair is required.",
            active_path.display(),
            active_metadata.byte_len
        )));
    }
    // minimumNextSeq proves the declared sequence cannot overlap any retained row without replaying history.
    // minimumNextSeq 在不重放历史的情况下证明声明序号不会与任何保留行重叠。
    let minimum_next_seq = active_metadata
        .start_seq
        .saturating_add(active_metadata.line_count);
    if next_seq < minimum_next_seq {
        return Err(SessionStoreError::new(format!(
            "Segmented append metadata declares next sequence {next_seq}, below the safe minimum {minimum_next_seq}; load-time repair is required."
        )));
    }
    if actual_byte_len == 0 {
        if active_metadata.line_count != 0 {
            return Err(SessionStoreError::new(format!(
                "Exact active segment `{}` is empty but metadata retains {} lines; load-time repair is required.",
                active_path.display(),
                active_metadata.line_count
            )));
        }
        return Ok(());
    }
    // activeFile reads only the final byte needed to prove a complete JSONL boundary.
    // activeFile 只读取证明 JSONL 边界完整所需的最后一个字节。
    let mut active_file = File::open(&active_path).map_err(to_store_error)?;
    active_file
        .seek(SeekFrom::End(-1))
        .map_err(to_store_error)?;
    // finalByte holds the exact append-boundary byte and never scales with transcript size.
    // finalByte 保存精确追加边界字节，其开销不随转录大小增长。
    let mut final_byte = [0_u8; 1];
    active_file
        .read_exact(&mut final_byte)
        .map_err(to_store_error)?;
    if final_byte[0] != b'\n' {
        return Err(SessionStoreError::new(format!(
            "Exact active segment `{}` has an incomplete JSONL tail; load-time repair is required.",
            active_path.display()
        )));
    }
    Ok(())
}

/// Returns whether a segment should roll before appending a new line.
/// 返回追加新行前是否应滚动分段。
pub(super) fn should_roll_segment(
    segment: &SegmentMetadata,
    segment_policy: &SegmentPolicy,
    next_line_bytes: u64,
) -> bool {
    segment.line_count >= segment_policy.max_lines
        || segment.byte_len.saturating_add(next_line_bytes) > segment_policy.max_bytes
}

/// Appends a task-link event to the main session JSONL stream when metadata exists.
/// 当主会话元数据存在时向主会话 JSONL 流追加一条子任务关联事件。
pub(super) fn segment_file_name(index: usize) -> String {
    format!("s{index:03}.jsonl")
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Sanitizes one filesystem path component.
/// 清理单个文件系统路径片段。
pub(super) fn sanitize_file_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

/// Creates a directory tree.
/// 创建目录树。
pub(super) fn create_dir_all(path: &Path) -> Result<(), SessionStoreError> {
    fs::create_dir_all(path).map_err(to_store_error)
}

/// Opens one optional file without a separate existence preflight.
/// 在不执行独立存在性预检的情况下打开一个可选文件。
///
/// # Parameters
/// 参数
/// `path` is the exact file path whose open result is authoritative.
/// `path` 是以打开结果为权威事实的精确文件路径。
///
/// # Returns
/// 返回值
/// The opened file, `None` only for `NotFound`, or a path-aware open failure.
/// 返回已打开文件；仅在 `NotFound` 时返回 `None`，否则返回包含路径的打开错误。
fn open_optional_file(path: &Path) -> Result<Option<File>, SessionStoreError> {
    match File::open(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(SessionStoreError::new(format!(
            "Failed to open optional file `{}`: {error}",
            path.display()
        ))),
    }
}

/// Removes one optional file without a separate existence preflight.
/// 在不执行独立存在性预检的情况下删除一个可选文件。
///
/// # Parameters
/// 参数
/// `path` is the exact file path whose removal result is authoritative.
/// `path` 是以删除结果为权威事实的精确文件路径。
///
/// # Returns
/// 返回值
/// `true` when a file was removed, `false` only for `NotFound`, or a path-aware removal failure.
/// 删除文件时返回 `true`；仅在 `NotFound` 时返回 `false`，否则返回包含路径的删除错误。
fn remove_optional_file(path: &Path) -> Result<bool, SessionStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(SessionStoreError::new(format!(
            "Failed to remove optional file `{}`: {error}",
            path.display()
        ))),
    }
}

/// Appends one JSON object as a JSONL line and returns the bytes written.
/// 将一个 JSON 对象作为 JSONL 行追加并返回写入字节数。
pub(super) fn append_json_line<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<u64, SessionStoreError> {
    let encoded = serde_json::to_string(value).map_err(to_store_error)?;
    append_encoded_json_line(path, &encoded)
}

/// Appends and synchronizes one pre-encoded JSONL line before metadata publication.
/// 在元数据发布前追加并同步一条已编码 JSONL 行。
///
/// # Parameters
/// 参数
/// `path` is the active JSONL segment receiving the row.
/// `path` 是接收该记录的活动 JSONL 分段。
/// `encoded` is one complete JSON value without the trailing newline.
/// `encoded` 是不含尾随换行符的一条完整 JSON 值。
///
/// # Returns
/// 返回值
/// The durable byte count added by a repaired boundary and the new row.
/// 返回修复边界与新记录新增的耐久字节数。
pub(super) fn append_encoded_json_line(
    path: &Path,
    encoded: &str,
) -> Result<u64, SessionStoreError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let mut encoded_line = String::with_capacity(encoded.len().saturating_add(1));
    encoded_line.push_str(encoded);
    encoded_line.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    let boundary_bytes = ensure_jsonl_append_boundary(&mut file)?;
    file.write_all(encoded_line.as_bytes())
        .map_err(to_store_error)?;
    // sync_data orders durable JSONL content before callers publish synchronized segment metadata.
    // sync_data 使 JSONL 内容先于调用方发布并同步分段元数据完成耐久化。
    file.sync_data().map_err(to_store_error)?;
    Ok(boundary_bytes + encoded_line.len() as u64)
}

/// Ensures the existing JSONL file tail ends with a newline before appending another row.
/// 确保已有 JSONL 文件尾部在追加新行前以换行结束。
///
/// # Parameters
/// 参数
/// `file` is the append handle opened for the target JSONL file.
/// `file` 是为目标 JSONL 文件打开的追加句柄。
///
/// # Returns
/// 返回值
/// The number of boundary bytes written before the next JSONL record.
/// 返回下一条 JSONL 记录前额外写入的边界字节数。
fn ensure_jsonl_append_boundary(file: &mut File) -> Result<u64, SessionStoreError> {
    let len = file.metadata().map_err(to_store_error)?.len();
    if len == 0 {
        return Ok(0);
    }
    file.seek(SeekFrom::Start(len.saturating_sub(1)))
        .map_err(to_store_error)?;
    let mut tail = [0_u8; 1];
    file.read_exact(&mut tail).map_err(to_store_error)?;
    if tail[0] != b'\n' {
        file.write_all(b"\n").map_err(to_store_error)?;
        return Ok(1);
    }
    Ok(0)
}

/// Reads one optional JSON document.
/// 读取一个可选 JSON 文档。
pub(super) fn read_json_file<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, SessionStoreError> {
    // file comes from the authoritative open result so inspection failures cannot look absent.
    // file 来自权威打开结果，因此检查失败不会伪装成缺失。
    let Some(file) = open_optional_file(path)? else {
        return Ok(None);
    };
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .map(Some)
        .map_err(to_store_error)
}

/// Reads one required JSON document.
/// 读取一个必需 JSON 文档。
pub(super) fn read_required_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<T, SessionStoreError> {
    read_json_file(path)?.ok_or_else(|| {
        SessionStoreError::new(format!(
            "Required JSON file does not exist: {}",
            path.display()
        ))
    })
}

/// Writes one complete JSON document through a synchronized unique sibling staging file.
/// 通过同步且唯一的同级暂存文件写入一份完整 JSON 文档。
pub(super) fn write_json_file<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SessionStoreError> {
    // encoded is complete before any filesystem path is created or replaced.
    // encoded 在创建或替换任何文件系统路径前已经完整生成。
    let encoded = serde_json::to_vec_pretty(value).map_err(to_store_error)?;
    write_file_atomically(path, &encoded, "JSON document").map_err(SessionStoreError::new)
}

/// Creates one unique sibling staging directory for a complete task layout.
/// 为完整子任务布局创建一个唯一的同级暂存目录。
///
/// # Parameters
/// 参数
/// `final_task_dir` is the final directory published after every task artifact is complete.
/// `final_task_dir` 是全部子任务产物完整后发布的最终目录。
///
/// # Returns
/// 返回值
/// The exclusively created sibling directory owned by the current creation attempt.
/// 返回由当前创建尝试独占的同级目录。
pub(super) fn create_task_directory_staging(
    final_task_dir: &Path,
) -> Result<PathBuf, SessionStoreError> {
    // parent keeps staging and publication on the same filesystem.
    // parent 使暂存与发布保持在同一文件系统。
    let parent = final_task_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // directory_name preserves the exact filesystem identity of the final task directory.
    // directory_name 保留最终子任务目录的精确文件系统身份。
    let directory_name = final_task_dir.file_name().ok_or_else(|| {
        SessionStoreError::new(format!(
            "Task directory path has no final component: {}",
            final_task_dir.display()
        ))
    })?;

    loop {
        // sequence distinguishes process-local attempts while create_dir rejects real collisions.
        // sequence 区分进程内尝试，同时由 create_dir 拒绝真实碰撞。
        let sequence = TASK_DIRECTORY_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // staged_name retains the target name for diagnostics without lossy Unicode conversion.
        // staged_name 保留目标名称用于诊断，且不进行有损 Unicode 转换。
        let mut staged_name = OsString::from(".");
        staged_name.push(directory_name);
        staged_name.push(format!(
            ".vulcan-creating-{}-{sequence}",
            std::process::id()
        ));
        // staged_task_dir is always a sibling of the final task directory.
        // staged_task_dir 始终是最终子任务目录的同级目录。
        let staged_task_dir = parent.join(staged_name);
        match fs::create_dir(&staged_task_dir) {
            Ok(()) => return Ok(staged_task_dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(to_store_error(error)),
        }
    }
}

/// Publishes one complete staged task layout through a same-filesystem directory rename.
/// 通过同文件系统目录重命名发布一个完整暂存子任务布局。
///
/// # Parameters
/// 参数
/// `staged_task_dir` is the exclusively owned complete staging directory.
/// `staged_task_dir` 是当前尝试独占的完整暂存目录。
/// `final_task_dir` is the authoritative task directory that must not already exist.
/// `final_task_dir` 是不得预先存在的权威子任务目录。
///
/// # Returns
/// 返回值
/// `Ok(())` after the complete directory entry is atomically published and synchronized.
/// 完整目录项完成原子发布与同步后返回 `Ok(())`。
pub(super) fn publish_task_directory(
    staged_task_dir: &Path,
    final_task_dir: &Path,
) -> Result<(), SessionStoreError> {
    fs::rename(staged_task_dir, final_task_dir).map_err(|error| {
        SessionStoreError::new(format!(
            "Failed to publish task directory `{}` from `{}`: {error}",
            final_task_dir.display(),
            staged_task_dir.display()
        ))
    })?;
    // POSIX durability requires synchronizing the parent directory entry after rename.
    // POSIX 持久性要求在重命名后同步父目录项。
    #[cfg(unix)]
    synchronize_parent_directory(final_task_dir).map_err(SessionStoreError::new)?;
    Ok(())
}

/// Removes one unpublished task staging directory after a failed creation attempt.
/// 在创建尝试失败后删除一个尚未发布的子任务暂存目录。
///
/// # Parameters
/// 参数
/// `staged_task_dir` is the exclusively owned unpublished directory.
/// `staged_task_dir` 是当前尝试独占且尚未发布的目录。
///
/// # Returns
/// 返回值
/// Best-effort cleanup has no return value because the primary creation error remains authoritative.
/// 尽力清理不返回值，因为首要创建错误保持权威性。
pub(super) fn remove_task_directory_staging(staged_task_dir: &Path) {
    let _ = fs::remove_dir_all(staged_task_dir);
}

/// Reads JSONL records from one path.
/// 从指定路径读取 JSONL 记录。
#[cfg(test)]
pub(super) fn read_json_lines<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Vec<T>, SessionStoreError> {
    // file distinguishes a genuinely absent fixture from every other open failure.
    // file 区分真正缺失的测试文件与其他所有打开失败。
    let Some(file) = open_optional_file(path)? else {
        return Ok(Vec::new());
    };
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(to_store_error)?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(to_store_error)?);
    }
    Ok(values)
}

/// Repairs malformed TaskSession segment files while the caller owns the shared append lock.
/// 在调用方持有共享追加锁时修复格式损坏的 TaskSession 分段文件。
///
/// # Parameters
/// 参数
/// `dir` is the task-session directory that owns the segment files.
/// `dir` 是拥有分段文件的子代理会话目录。
/// `metadata` is the mutable main-compatible metadata document to rebuild when repair changes files.
/// `metadata` 是修复文件后需要重建的可变主会话兼容元数据。
///
/// # Returns
/// 返回值
/// `true` when at least one segment file or metadata field was repaired.
/// 当至少一个分段文件或元数据字段被修复时返回 `true`。
pub(super) fn repair_task_segment_jsonl_files_if_needed_locked(
    dir: &Path,
    metadata: &mut SegmentedTaskSessionMainMetadataDocument,
) -> Result<bool, SessionStoreError> {
    let original_segments = metadata.segments.clone();
    let mut retained_segment_lines: Vec<(String, Vec<SegmentedEventLine>)> = Vec::new();
    let mut changed = original_segments.is_empty();
    let mut discard_remaining_segments = false;

    for segment in &original_segments {
        let segment_path = dir.join(&segment.file);
        if discard_remaining_segments {
            if remove_optional_file(&segment_path)? {
                changed = true;
            }
            continue;
        }

        let repair_read = read_segment_event_lines_for_repair(&segment_path)?;
        changed |= repair_read.changed;
        if repair_read.truncated_at_corrupt_line {
            discard_remaining_segments = true;
        }
        if repair_read.lines.is_empty() {
            if remove_optional_file(&segment_path)? {
                changed = true;
            }
            continue;
        }
        if repair_read.changed {
            write_json_lines_file_atomic(&segment_path, &repair_read.lines)?;
        }
        retained_segment_lines.push((segment.file.clone(), repair_read.lines));
    }

    let mut retained_segments = Vec::new();
    for (file, lines) in &retained_segment_lines {
        // durableByteLen comes from the exact retained file after any repair rewrite.
        // durableByteLen 来自任何修复重写之后的精确保留文件。
        let durable_byte_len = fs::metadata(dir.join(file)).map_err(to_store_error)?.len();
        retained_segments.push(rebuild_task_segment_metadata(
            file,
            lines,
            durable_byte_len,
        )?);
    }
    if retained_segments.is_empty() {
        let empty_segment = SegmentMetadata::empty(segment_file_name(1), 0);
        write_json_lines_file_atomic::<SegmentedEventLine>(&dir.join(&empty_segment.file), &[])?;
        retained_segments.push(empty_segment);
    }
    let next_global_seq = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter().map(|line| line.seq))
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(0);
    changed |= metadata.segments.len() != retained_segments.len();
    changed |= metadata
        .segments
        .iter()
        .zip(retained_segments.iter())
        .any(|(left, right)| task_segment_metadata_changed(left, right));
    changed |= metadata.next_global_seq != next_global_seq;
    changed |= metadata.active_segment
        != retained_segments
            .last()
            .map(|segment| segment.file.clone())
            .unwrap_or_else(|| segment_file_name(1));

    if !changed {
        return Ok(false);
    }

    metadata.segments = retained_segments;
    metadata.active_segment = metadata
        .segments
        .last()
        .map(|segment| segment.file.clone())
        .unwrap_or_else(|| segment_file_name(1));
    metadata.next_global_seq = next_global_seq;
    metadata.updated_at = current_unix_millis_u64().to_string();
    Ok(true)
}

/// Reads TaskSession segment files using the same corruption boundary as repair.
/// 按修复逻辑相同的损坏边界读取 TaskSession 分段文件。
///
/// # Parameters
/// 参数
/// `dir` is the task-session directory containing segment files.
/// `dir` 是包含分段文件的子代理会话目录。
/// `segments` are the metadata-indexed segment files to read.
/// `segments` 是元数据索引的待读取分段文件。
///
/// # Returns
/// 返回值
/// Valid segment lines before the first malformed row.
/// 返回首条格式错误行之前的有效分段行。
pub(super) fn read_segment_lines_repairing_locked(
    dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<Vec<SegmentedEventLine>, SessionStoreError> {
    let mut lines = Vec::new();
    for segment in segments {
        let segment_path = dir.join(&segment.file);
        let repair_read = read_segment_event_lines_for_repair(&segment_path)?;
        if repair_read.changed && repair_read.source_file_was_opened {
            write_json_lines_file_atomic(&segment_path, &repair_read.lines)?;
        }
        lines.extend(repair_read.lines);
        if repair_read.truncated_at_corrupt_line {
            break;
        }
    }
    lines.sort_by_key(|line| line.seq);
    Ok(lines)
}

/// Parsed JSONL content and repair status for one TaskSession segment file.
/// 单个 TaskSession 分段文件的 JSONL 解析内容与修复状态。
struct TaskSegmentJsonlRepairRead {
    /// Retained valid event lines.
    /// 保留下来的有效事件行。
    lines: Vec<SegmentedEventLine>,
    /// Whether the physical file or metadata should be rewritten.
    /// 是否需要重写物理文件或元数据。
    changed: bool,
    /// Whether a malformed line forced truncating this segment suffix.
    /// 是否存在格式错误行导致当前分段后缀被截断。
    truncated_at_corrupt_line: bool,
    /// Whether the authoritative read opened a physical source file at its observation point.
    /// 权威读取是否在观察时点打开了物理源文件。
    source_file_was_opened: bool,
}

/// Reads one TaskSession segment file and marks the first malformed line as the cutoff.
/// 读取单个 TaskSession 分段文件，并把首条格式错误行标记为截断点。
///
/// # Parameters
/// 参数
/// `segment_path` is the physical JSONL segment path.
/// `segment_path` 是物理 JSONL 分段路径。
///
/// # Returns
/// 返回值
/// Retained valid lines and repair metadata.
/// 返回保留的有效行与修复元数据。
fn read_segment_event_lines_for_repair(
    segment_path: &Path,
) -> Result<TaskSegmentJsonlRepairRead, SessionStoreError> {
    // file makes the open operation the only source of physical segment existence.
    // file 让打开操作成为物理分段存在性的唯一事实来源。
    let Some(file) = open_optional_file(segment_path)? else {
        return Ok(TaskSegmentJsonlRepairRead {
            lines: Vec::new(),
            changed: true,
            truncated_at_corrupt_line: true,
            source_file_was_opened: false,
        });
    };
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut changed = false;
    let mut truncated_at_corrupt_line = false;
    for line in reader.lines() {
        let line = line.map_err(to_store_error)?;
        if line.trim().is_empty() {
            changed = true;
            continue;
        }
        match serde_json::from_str::<SegmentedEventLine>(&line) {
            Ok(event_line) => lines.push(event_line),
            Err(_) => {
                changed = true;
                truncated_at_corrupt_line = true;
                break;
            }
        }
    }
    Ok(TaskSegmentJsonlRepairRead {
        lines,
        changed,
        truncated_at_corrupt_line,
        source_file_was_opened: true,
    })
}

/// Rebuilds one TaskSession segment metadata record from retained lines.
/// 根据保留行重建单个 TaskSession 分段元数据记录。
///
/// # Parameters
/// 参数
/// `file` is the segment file name stored in metadata.
/// `file` 是元数据中保存的分段文件名。
/// `lines` are the retained valid lines for this segment.
/// `lines` 是该分段保留的有效行。
/// `durable_byte_len` is the exact file length observed after optional repair publication.
/// `durable_byte_len` 是可选修复发布后观察到的精确文件长度。
///
/// # Returns
/// 返回值
/// A segment metadata record matching the retained physical file.
/// 返回与保留物理文件匹配的分段元数据。
fn rebuild_task_segment_metadata(
    file: &str,
    lines: &[SegmentedEventLine],
    durable_byte_len: u64,
) -> Result<SegmentMetadata, SessionStoreError> {
    let mut metadata = SegmentMetadata::empty(
        file.to_string(),
        lines.first().map(|line| line.seq).unwrap_or(0),
    );
    metadata.line_count = lines.len() as u64;
    metadata.byte_len = durable_byte_len;
    metadata.first_timestamp = lines.first().map(|line| line.timestamp.clone());
    metadata.last_timestamp = lines.last().map(|line| line.timestamp.clone());
    metadata.first_turn_index = lines.iter().find_map(|line| line.turn_index);
    metadata.last_turn_index = lines.iter().rev().find_map(|line| line.turn_index);
    metadata.last_compaction_seq = lines
        .iter()
        .rev()
        .find(|line| line.kind == "compacted")
        .map(|line| line.seq);
    Ok(metadata)
}

/// Returns whether two TaskSession segment metadata records differ.
/// 返回两条 TaskSession 分段元数据是否存在差异。
///
/// # Parameters
/// 参数
/// `left` is the existing metadata record.
/// `left` 是已有元数据记录。
/// `right` is the rebuilt metadata record.
/// `right` 是重建后的元数据记录。
///
/// # Returns
/// 返回值
/// `true` when any persisted metadata field differs.
/// 当任意持久化元数据字段不同时返回 `true`。
fn task_segment_metadata_changed(left: &SegmentMetadata, right: &SegmentMetadata) -> bool {
    left.file != right.file
        || left.start_seq != right.start_seq
        || left.line_count != right.line_count
        || left.byte_len != right.byte_len
        || left.first_timestamp != right.first_timestamp
        || left.last_timestamp != right.last_timestamp
        || left.first_turn_index != right.first_turn_index
        || left.last_turn_index != right.last_turn_index
        || left.last_compaction_seq != right.last_compaction_seq
}

/// Writes one complete JSONL segment through the shared atomic file publisher.
/// 通过共享原子文件发布器写入一个完整 JSONL 分段。
///
/// # Parameters
/// 参数
/// `path` is the final JSONL segment path.
/// `path` 是最终 JSONL 分段路径。
/// `values` are the complete ordered rows retained by repair.
/// `values` 是 repair 保留的完整有序记录。
///
/// # Returns
/// 返回值
/// Returns success after the complete JSONL bytes are atomically published.
/// 完整 JSONL 字节完成原子发布后返回成功。
pub(super) fn write_json_lines_file_atomic<T: Serialize>(
    path: &Path,
    values: &[T],
) -> Result<(), SessionStoreError> {
    // encoded is complete before the repair writer creates or replaces any filesystem entry.
    // encoded 在 repair writer 创建或替换任何文件系统目录项前已经完整生成。
    let mut encoded = Vec::new();
    for value in values {
        serde_json::to_writer(&mut encoded, value).map_err(to_store_error)?;
        encoded.write_all(b"\n").map_err(to_store_error)?;
    }
    write_file_atomically(path, &encoded, "JSONL segment").map_err(SessionStoreError::new)
}

/// Converts any displayable error into a session store error.
/// 将任意可显示错误转换为会话存储错误。
pub(super) fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}

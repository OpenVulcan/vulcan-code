use super::*;

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

/// Appends one JSON object as a JSONL line and returns the bytes written.
/// 将一个 JSON 对象作为 JSONL 行追加并返回写入字节数。
pub(super) fn append_json_line<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<u64, SessionStoreError> {
    let encoded = serde_json::to_string(value).map_err(to_store_error)?;
    append_encoded_json_line(path, &encoded)
}

/// Appends one pre-encoded JSONL line and returns the bytes written.
/// 追加一条已编码 JSONL 行并返回写入字节数。
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
    file.flush().map_err(to_store_error)?;
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
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path).map_err(to_store_error)?;
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

/// Writes one JSON document atomically enough for local file updates.
/// 以足够本地文件更新使用的原子方式写入一个 JSON 文档。
pub(super) fn write_json_file<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SessionStoreError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("json.tmp");
    {
        let file = File::create(&temp_path).map_err(to_store_error)?;
        serde_json::to_writer_pretty(file, value).map_err(to_store_error)?;
    }
    fs::rename(&temp_path, path).map_err(to_store_error)
}

/// Reads JSONL records from one path.
/// 从指定路径读取 JSONL 记录。
#[cfg(test)]
pub(super) fn read_json_lines<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Vec<T>, SessionStoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(to_store_error)?;
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
            if segment_path.exists() {
                fs::remove_file(&segment_path).map_err(to_store_error)?;
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
            if segment_path.exists() {
                fs::remove_file(&segment_path).map_err(to_store_error)?;
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
        retained_segments.push(rebuild_task_segment_metadata(file, lines)?);
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
        if repair_read.changed && segment_path.exists() {
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
    if !segment_path.exists() {
        return Ok(TaskSegmentJsonlRepairRead {
            lines: Vec::new(),
            changed: true,
            truncated_at_corrupt_line: true,
        });
    }
    let file = File::open(segment_path).map_err(to_store_error)?;
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
///
/// # Returns
/// 返回值
/// A segment metadata record matching the retained physical file.
/// 返回与保留物理文件匹配的分段元数据。
fn rebuild_task_segment_metadata(
    file: &str,
    lines: &[SegmentedEventLine],
) -> Result<SegmentMetadata, SessionStoreError> {
    let mut metadata = SegmentMetadata::empty(
        file.to_string(),
        lines.first().map(|line| line.seq).unwrap_or(0),
    );
    metadata.line_count = lines.len() as u64;
    metadata.byte_len = task_segment_lines_byte_len(lines)?;
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

/// Calculates the byte length of retained TaskSession JSONL lines.
/// 计算保留的 TaskSession JSONL 行字节长度。
///
/// # Parameters
/// 参数
/// `lines` are the retained valid segment lines.
/// `lines` 是保留的有效分段行。
///
/// # Returns
/// 返回值
/// The serialized JSONL byte length including newline bytes.
/// 返回包含换行字节的序列化 JSONL 字节长度。
fn task_segment_lines_byte_len(lines: &[SegmentedEventLine]) -> Result<u64, SessionStoreError> {
    let mut byte_len = 0_u64;
    for line in lines {
        byte_len = byte_len
            .saturating_add(serde_json::to_string(line).map_err(to_store_error)?.len() as u64 + 1);
    }
    Ok(byte_len)
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

/// Writes JSONL values through a temporary file and rename.
/// 通过临时文件与重命名写入 JSONL 值集合。
fn write_json_lines_file_atomic<T: Serialize>(
    path: &Path,
    values: &[T],
) -> Result<(), SessionStoreError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = path.with_extension(format!(
        "{}.tmp.{}",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("jsonl"),
        current_unix_millis_u64()
    ));
    {
        let mut file = File::create(&temp_path).map_err(to_store_error)?;
        for value in values {
            serde_json::to_writer(&mut file, value).map_err(to_store_error)?;
            file.write_all(b"\n").map_err(to_store_error)?;
        }
        file.flush().map_err(to_store_error)?;
    }
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Converts any displayable error into a session store error.
/// 将任意可显示错误转换为会话存储错误。
pub(super) fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}

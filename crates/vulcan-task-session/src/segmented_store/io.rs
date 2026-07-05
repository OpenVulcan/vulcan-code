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
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded.as_bytes()).map_err(to_store_error)?;
    file.write_all(b"\n").map_err(to_store_error)?;
    Ok(encoded.len() as u64 + 1)
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

/// Converts any displayable error into a session store error.
/// 将任意可显示错误转换为会话存储错误。
pub(super) fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}

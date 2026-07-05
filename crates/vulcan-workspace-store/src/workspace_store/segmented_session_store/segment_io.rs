//! Low-level segmented JSONL and metadata I/O helpers.
//! 底层分段 JSONL 与元数据 I/O 辅助。

use super::*;

/// 当内存消息项尚未分配标识时构建一个确定性消息标识。
pub(in crate::workspace_store) fn read_segment_event_lines(
    segment_path: &Path,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    read_segment_event_lines_tolerant(segment_path)
}

/// Reads segmented event lines while isolating unknown historical event types from the active workspace.
/// 读取分段事件行时隔离未知历史事件类型，避免单条旧事件拖垮当前工作区。
pub(in crate::workspace_store) fn read_segment_event_lines_tolerant(
    segment_path: &Path,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if !segment_path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(segment_path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                segment_path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            format!(
                "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                segment_path.display()
            )
        })?;
        if !segmented_event_value_has_known_kind(&value) {
            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("<missing>");
            eprintln!(
                "[workspace-store] skipped unknown segmented session event type `{event_type}` at {}:{line_number}",
                segment_path.display()
            );
            continue;
        }
        lines.push(
            serde_json::from_value::<SegmentedSessionEventLine>(value).map_err(|error| {
                format!(
                    "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                    segment_path.display()
                )
            })?,
        );
    }
    Ok(lines)
}

/// Returns whether one raw segment JSON value declares an event type accepted by the current schema.
/// 返回原始分段 JSON 值是否声明了当前 schema 接受的事件类型。
pub(in crate::workspace_store) fn segmented_event_value_has_known_kind(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .and_then(|kind| {
            serde_json::from_value::<SegmentedSessionEventKind>(Value::String(kind.to_string()))
                .ok()
        })
        .is_some()
}

/// Reverse-scans ordered segment metadata for the latest replacement-history anchor.
/// 按倒序扫描有序分段元数据，寻找最新的 replacement-history 锚点。
pub(in crate::workspace_store) fn find_latest_compaction_anchor(
    session_dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<SegmentedCompactionScanResult, String> {
    // Keep newer segment lines while scanning from newest to oldest.
    // 从新到旧扫描时暂存较新分段的事件行。
    let mut newer_segment_lines: Vec<Vec<SegmentedSessionEventLine>> = Vec::new();
    // Count segment reads so tests and diagnostics can verify lazy behavior later.
    // 记录分段读取次数，方便测试与诊断后续验证懒加载行为。
    let mut scanned_segment_count = 0;
    // Capture the newest turn context encountered before the scan can stop at a compaction anchor.
    // 捕获扫描在压缩锚点停止前遇到的最新轮次上下文。
    let mut latest_turn_context: Option<SegmentedTurnContextAnchor> = None;
    // Capture rollback markers in reverse discovery order for reducer follow-up.
    // 以倒序发现顺序捕获回滚标记，供后续 reducer 使用。
    let mut rollback_markers: Vec<SegmentedRollbackMarker> = Vec::new();

    for segment in segments.iter().rev() {
        scanned_segment_count += 1;
        let segment_path = session_dir.join(&segment.file);
        let lines = read_segment_event_lines(&segment_path)?;
        for line_index in (0..lines.len()).rev() {
            let line = &lines[line_index];
            if line.kind == SegmentedSessionEventKind::TurnContext && latest_turn_context.is_none()
            {
                latest_turn_context = Some(SegmentedTurnContextAnchor {
                    seq: line.seq,
                    segment_file: segment.file.clone(),
                    line_index,
                    payload: line.payload.clone(),
                });
            }
            if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                rollback_markers.push(SegmentedRollbackMarker {
                    seq: line.seq,
                    segment_file: segment.file.clone(),
                    line_index,
                    payload: line.payload.clone(),
                });
            }
            let is_replacement_anchor = matches!(
                line.kind,
                SegmentedSessionEventKind::Compacted | SegmentedSessionEventKind::ThreadRolledBack
            ) && line.payload.get("replacement_history").is_some();
            if !is_replacement_anchor {
                continue;
            }
            let Some(replacement_history) = line.payload.get("replacement_history").cloned() else {
                continue;
            };
            // The suffix begins after the replacement anchor line in the current segment.
            // suffix 从当前分段中 replacement 锚点行之后开始。
            let mut suffix = lines[line_index + 1..].to_vec();
            // Newer segments were collected newest-first, so replay order needs the reverse.
            // 较新分段按最新优先收集，因此重放顺序需要反转。
            for newer_lines in newer_segment_lines.iter().rev() {
                suffix.extend(newer_lines.clone());
            }
            let suffix = suffix_after_replacement_anchor(line, &suffix);
            return Ok(SegmentedCompactionScanResult {
                anchor: Some(SegmentedCompactionAnchor {
                    anchor_seq: line.seq,
                    anchor_kind: line.kind,
                    segment_file: segment.file.clone(),
                    line_index,
                    replacement_history,
                }),
                suffix,
                latest_turn_context,
                rollback_markers,
                scanned_segment_count,
            });
        }
        newer_segment_lines.push(lines);
    }

    // Without an anchor, return all scanned events in forward segment order.
    // 没有锚点时，按正向分段顺序返回所有已扫描事件。
    let mut suffix = Vec::new();
    for lines in newer_segment_lines.iter().rev() {
        suffix.extend(lines.clone());
    }
    Ok(SegmentedCompactionScanResult {
        anchor: None,
        suffix,
        latest_turn_context,
        rollback_markers,
        scanned_segment_count,
    })
}

/// Appends events using a main session metadata document already loaded by the caller.
/// 使用调用方已加载的主会话元数据文档追加事件。
pub(in crate::workspace_store) fn append_events_with_metadata(
    session_dir: &Path,
    metadata: &mut SegmentedSessionMetadata,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep appended lines for tests and projection callers.
    // 为测试与投影调用方保留已追加行。
    let mut appended_lines = Vec::new();
    for event in events {
        // Allocate the next global sequence only when this event is about to be written.
        // 仅在事件即将写入时分配下一个全局序号。
        let line = SegmentedSessionEventLine {
            seq: metadata.next_global_seq,
            timestamp: event.timestamp.clone(),
            kind: event.kind,
            payload: event.payload.clone(),
        };
        append_line_to_active_segment(
            session_dir,
            &metadata.segment_policy,
            &mut metadata.active_segment,
            &mut metadata.segments,
            &line,
            event.turn_index,
        )?;
        metadata.next_global_seq += 1;
        metadata.updated_at = event.timestamp.clone();
        if event.kind == SegmentedSessionEventKind::ThreadRolledBack {
            metadata.latest_rollback_seq = Some(line.seq);
        }
        appended_lines.push(line);
    }
    Ok(appended_lines)
}

/// Builds the durable payload for one session boundary event with store-assigned sequence.
/// 使用存储层分配的序号构建单个 会话边界事件的持久化 payload。
pub(in crate::workspace_store) fn session_boundary_payload(
    event: &SessionBoundaryEvent,
    assigned_sequence: u64,
) -> Value {
    json!({
        "event_id": event.event_id,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "kind": event.kind,
        "scope": event.scope,
        "visibility": event.visibility,
        "sequence": assigned_sequence,
        "payload": event.payload,
    })
}

/// Appends events using a task session metadata document already loaded by the caller.
/// 使用调用方已加载的子任务会话元数据文档追加事件。
pub(in crate::workspace_store) fn append_task_events_with_metadata(
    task_dir: &Path,
    metadata: &mut SegmentedTaskSessionMetadata,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep appended lines for tests and projection callers.
    // 为测试与投影调用方保留已追加行。
    let mut appended_lines = Vec::new();
    for event in events {
        // Allocate task-local sequence numbers independently from the parent session.
        // 独立于父会话分配子任务局部序号。
        let line = SegmentedSessionEventLine {
            seq: metadata.next_local_seq,
            timestamp: event.timestamp.clone(),
            kind: event.kind,
            payload: event.payload.clone(),
        };
        append_line_to_active_segment(
            task_dir,
            &metadata.segment_policy,
            &mut metadata.active_segment,
            &mut metadata.segments,
            &line,
            event.turn_index,
        )?;
        metadata.next_local_seq += 1;
        metadata.updated_at = event.timestamp.clone();
        appended_lines.push(line);
    }
    Ok(appended_lines)
}

/// Appends one line to the active segment, rolling over before the write when needed.
/// 向活动分段追加一行，并在需要时于写入前滚动到新分段。
pub(in crate::workspace_store) fn append_line_to_active_segment(
    session_dir: &Path,
    segment_policy: &SegmentPolicy,
    active_segment: &mut String,
    segments: &mut Vec<SegmentMetadata>,
    line: &SegmentedSessionEventLine,
    turn_index: Option<i64>,
) -> Result<(), String> {
    // Serialize before rollover so the byte budget includes the final newline.
    // 先序列化再判断滚动，使字节预算包含最终换行。
    let line_text = serde_json::to_string(line).map_err(to_store_error)?;
    // Account for the newline byte appended by JSONL.
    // 计入 JSONL 追加的换行字节。
    let line_byte_len = line_text.len() as u64 + 1;
    if active_segment_needs_rollover(segments, segment_policy, line_byte_len) {
        // Create the next segment with a start sequence matching the line about to be written.
        // 创建下一个分段，其起始序号匹配即将写入的事件行。
        let next_file = segment_file_name(segments.len() + 1);
        segments.push(SegmentMetadata::empty(next_file.clone(), line.seq));
        *active_segment = next_file;
    }
    // The active segment must exist after initialization or rollover.
    // 初始化或滚动后活动分段必须存在。
    let Some(segment) = segments.last_mut() else {
        return Err("Segment metadata is empty.".to_string());
    };
    let segment_path = session_dir.join(&segment.file);
    append_json_line(&segment_path, line)?;
    update_segment_metadata(segment, line, line_byte_len, turn_index);
    Ok(())
}

/// Returns whether the active segment should roll over before appending a new line.
/// 返回追加新行前活动分段是否应该滚动。
pub(in crate::workspace_store) fn active_segment_needs_rollover(
    segments: &[SegmentMetadata],
    segment_policy: &SegmentPolicy,
    next_line_byte_len: u64,
) -> bool {
    // An empty metadata list is repaired by the caller through initialization, so no rollover is possible.
    // 空元数据列表由调用方通过初始化修复，因此无法滚动。
    let Some(active_segment) = segments.last() else {
        return false;
    };
    if active_segment.line_count == 0 {
        return false;
    }
    active_segment.line_count >= segment_policy.max_lines
        || active_segment.byte_len + next_line_byte_len > segment_policy.max_bytes
}

/// Updates segment metadata after one line has been appended successfully.
/// 在一行成功追加后更新分段元数据。
pub(in crate::workspace_store) fn update_segment_metadata(
    segment: &mut SegmentMetadata,
    line: &SegmentedSessionEventLine,
    line_byte_len: u64,
    turn_index: Option<i64>,
) {
    if segment.line_count == 0 {
        segment.first_timestamp = Some(line.timestamp.clone());
        segment.first_turn_index = turn_index;
    }
    segment.line_count += 1;
    segment.byte_len += line_byte_len;
    segment.last_timestamp = Some(line.timestamp.clone());
    if let Some(turn_index) = turn_index {
        segment.last_turn_index = Some(turn_index);
    }
    if line.kind == SegmentedSessionEventKind::Compacted {
        segment.last_compaction_seq = Some(line.seq);
    }
}

/// Rebuilds one segment metadata record from the retained physical event lines.
/// 根据保留的物理事件行重建一条分段元数据记录。
pub(in crate::workspace_store) fn rebuild_segment_metadata(
    file: &str,
    lines: &[SegmentedSessionEventLine],
) -> Result<SegmentMetadata, String> {
    let start_seq = lines.first().map(|line| line.seq).unwrap_or(1);
    let mut segment = SegmentMetadata::empty(file.to_string(), start_seq);
    for line in lines {
        let line_text = serde_json::to_string(line).map_err(to_store_error)?;
        let line_byte_len = line_text.len() as u64 + 1;
        update_segment_metadata(
            &mut segment,
            line,
            line_byte_len,
            segmented_line_turn_index(line),
        );
    }
    Ok(segment)
}

/// Reads the canonical message identifier carried by one segmented event line.
/// 读取单条分段事件行携带的规范消息标识。
pub(in crate::workspace_store) fn segmented_line_message_id(
    line: &SegmentedSessionEventLine,
) -> Option<&str> {
    line.payload.get("message_id").and_then(Value::as_str)
}

/// Reads one main segmented session metadata document.
/// 读取一份主分段会话元数据文档。
pub(in crate::workspace_store) fn read_session_metadata(
    session_dir: &Path,
) -> Result<SegmentedSessionMetadata, String> {
    read_required_json::<SegmentedSessionMetadata>(&metadata_path(session_dir))
}

/// Reads one child task metadata document.
/// 读取一份子任务元数据文档。
pub(in crate::workspace_store) fn read_task_session_metadata(
    task_dir: &Path,
) -> Result<SegmentedTaskSessionMetadata, String> {
    read_required_json::<SegmentedTaskSessionMetadata>(&metadata_path(task_dir))
}

/// Lists one active or archived segmented-session root from text metadata.
/// 从文本元数据列出一个活动或归档分段会话根。
pub(in crate::workspace_store) fn list_segmented_session_catalog_for_root(
    data_root: &Path,
    archived: bool,
) -> Result<Vec<SegmentedSessionCatalogEntry>, String> {
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    let root = data_root.join(root_name);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for project_entry in fs::read_dir(&root).map_err(to_store_error)? {
        let project_entry = project_entry.map_err(to_store_error)?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        for session_entry in fs::read_dir(&project_path).map_err(to_store_error)? {
            let session_entry = session_entry.map_err(to_store_error)?;
            let session_dir = session_entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let metadata_file = metadata_path(&session_dir);
            if !metadata_file.exists() {
                continue;
            }
            let metadata = read_session_metadata(&session_dir)?;
            entries.push(SegmentedSessionCatalogEntry {
                session_dir,
                archived,
                metadata,
            });
        }
    }
    Ok(entries)
}

/// Writes a main session metadata document atomically.
/// 原子写入主会话元数据文档。
pub(in crate::workspace_store) fn write_metadata(
    session_dir: &Path,
    metadata: &SegmentedSessionMetadata,
) -> Result<(), String> {
    write_json_file_atomic(&metadata_path(session_dir), metadata)
}

/// Writes a task session metadata document atomically.
/// 原子写入子任务会话元数据文档。
pub(in crate::workspace_store) fn write_task_metadata(
    task_dir: &Path,
    metadata: &SegmentedTaskSessionMetadata,
) -> Result<(), String> {
    write_json_file_atomic(&metadata_path(task_dir), metadata)
}

/// Returns a metadata path for a session-like directory.
/// 返回会话类目录的元数据路径。
pub(in crate::workspace_store) fn metadata_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_METADATA_FILE)
}

/// Returns a state path for a session-like directory.
/// 返回会话类目录的状态路径。
pub(in crate::workspace_store) fn state_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_STATE_FILE)
}

/// Returns the shared process-local mutex used for state document read/modify/write operations.
/// 返回状态文档读改写操作共用的进程内互斥锁。
pub(in crate::workspace_store) fn segmented_state_file_io_lock() -> &'static Mutex<()> {
    SEGMENTED_STATE_FILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}

/// Returns the shared process-local mutex used for segmented JSONL append operations.
/// 返回分段 JSONL 追加操作共用的进程内互斥锁。
pub(in crate::workspace_store) fn segmented_append_file_io_lock() -> &'static Mutex<()> {
    SEGMENTED_APPEND_FILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}

/// Builds a stable segment file name from its one-based index.
/// 根据从一开始的索引构建稳定分段文件名。
pub(in crate::workspace_store) fn segment_file_name(index: usize) -> String {
    format!("s{index:03}.jsonl")
}

/// Creates a directory and all missing parents.
/// 创建目录及所有缺失的父目录。
pub(in crate::workspace_store) fn create_dir_all(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(to_store_error)
}

/// Appends one JSON value as a JSONL line.
/// 将一个 JSON 值作为 JSONL 行追加写入。
pub(in crate::workspace_store) fn append_json_line<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // Encode one complete JSONL row before touching the file handle.
    // 在接触文件句柄前先编码完整 JSONL 行。
    let mut encoded_line = serde_json::to_string(value).map_err(to_store_error)?;
    encoded_line.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded_line.as_bytes())
        .map_err(to_store_error)?;
    file.flush().map_err(to_store_error)
}

/// Reads a required JSON document from disk.
/// 从磁盘读取一个必需的 JSON 文档。
pub(in crate::workspace_store) fn read_required_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)
        .map_err(|error| format!("打开 JSON 文件失败 `{}`：{error}", path.display()))?;
    serde_json::from_reader(file)
        .map_err(|error| format!("解析 JSON 文件失败 `{}`：{error}", path.display()))
}

/// Writes one JSON document through a temporary file and rename.
/// 通过临时文件与重命名写入一个 JSON 文档。
pub(in crate::workspace_store) fn write_json_file_atomic<T>(
    path: &Path,
    value: &T,
) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // The temporary file lives next to the target so rename stays on the same filesystem.
    // 临时文件放在目标旁边，确保重命名位于同一文件系统。
    let temp_path = unique_json_temp_path(path);
    let mut file = File::create(&temp_path).map_err(to_store_error)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(to_store_error)?;
    file.write_all(b"\n").map_err(to_store_error)?;
    file.flush().map_err(to_store_error)?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Writes JSONL values through a temporary file and rename.
/// 通过临时文件与重命名写入 JSONL 值集合。
pub(in crate::workspace_store) fn write_json_lines_file_atomic<T>(
    path: &Path,
    values: &[T],
) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = unique_json_temp_path(path);
    let mut file = File::create(&temp_path).map_err(to_store_error)?;
    for value in values {
        serde_json::to_writer(&mut file, value).map_err(to_store_error)?;
        file.write_all(b"\n").map_err(to_store_error)?;
    }
    file.flush().map_err(to_store_error)?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Appends JSONL values to an existing file, creating its parent directory when needed.
/// 将 JSONL 值追加到已有文件，并在需要时创建父目录。
pub(in crate::workspace_store) fn append_json_lines_file<T>(
    path: &Path,
    values: &[T],
) -> Result<(), String>
where
    T: Serialize,
{
    if values.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // Build the full batch first so each append call emits one contiguous byte buffer.
    // 先构建完整批次，使每次追加调用输出一个连续字节缓冲。
    let mut encoded_batch = String::new();
    for value in values {
        encoded_batch.push_str(&serde_json::to_string(value).map_err(to_store_error)?);
        encoded_batch.push('\n');
    }
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded_batch.as_bytes())
        .map_err(to_store_error)?;
    file.flush().map_err(to_store_error)
}

/// Builds a collision-free temporary JSON path next to the target file.
/// 在目标文件旁构建不会互相冲突的 JSON 临时路径。
pub(in crate::workspace_store) fn unique_json_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.json");
    let temp_name = format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4());
    path.with_file_name(temp_name)
}

/// Moves one segmented session directory without overwriting an existing destination.
/// 移动一个分段会话目录，且不覆盖已存在的目标目录。
pub(in crate::workspace_store) fn move_segmented_session_directory(
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    if destination.exists() {
        return Err(format!(
            "Segmented session destination already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(source, destination).map_err(to_store_error)
}

/// Reads JSONL values from a file, returning an empty list when the file is absent.
/// 从文件读取 JSONL 值，文件不存在时返回空列表。
pub(in crate::workspace_store) fn read_json_lines<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(|error| {
            format!(
                "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?);
    }
    Ok(values)
}

/// Sanitizes one path component while preserving enough human-readable text for debugging.
/// 清理单个路径片段，同时保留足够的人类可读文本便于调试。
pub(in crate::workspace_store) fn sanitize_file_component(value: &str) -> String {
    // Replace path separators and control-like punctuation with underscores.
    // 将路径分隔符与控制类标点替换为下划线。
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "untagged".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Converts any displayable error into a storage error string.
/// 将任意可展示错误转换为存储错误字符串。
pub(in crate::workspace_store) fn to_store_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

use super::*;

/// Maximum bytes inspected from the exact active segment for one latest-event lookup.
/// 单次最新事件查询从精确活动分段检查的最大字节数。
const LATEST_EVENT_ACTIVE_SEGMENT_TAIL_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn read_task_invocations_from_main_segments(
    root_session_dir: &Path,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    // metadata is absent only when the authoritative file open reports `NotFound`.
    // metadata 仅在权威文件打开报告 `NotFound` 时视为缺失。
    let Some(metadata) = read_json_file::<Value>(&metadata_path)? else {
        return Ok(Vec::new());
    };
    let segments = serde_json::from_value::<Vec<SegmentMetadata>>(
        metadata
            .get("segments")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(to_store_error)?;
    read_task_invocations_from_lines(read_segment_lines(root_session_dir, &segments)?, parent)
}

/// Reads task invocations from a task segmented stream.
/// 从子任务分段流读取子任务调用事件。
pub(super) fn read_task_invocations_from_task_segments(
    task_dir: &Path,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let (_, lines) = read_repaired_task_segment_lines(task_dir)?;
    read_task_invocations_from_lines(lines, parent)
}

/// Extracts task invocation events from segment lines.
/// 从分段事件行中提取子任务调用事件。
pub(super) fn read_task_invocations_from_lines(
    lines: Vec<SegmentedEventLine>,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let mut events = Vec::new();
    for line in lines {
        let Some(value) = line.payload.get("task_invocation_event").cloned() else {
            continue;
        };
        let mut event =
            serde_json::from_value::<TaskInvocationEvent>(value).map_err(to_store_error)?;
        event.sequence = line.seq;
        if &event.parent == parent {
            events.push(event);
        }
    }
    events.sort_by_key(|event| event.sequence);
    Ok(events)
}

/// Reads every JSONL segment line described by metadata.
/// 读取元数据描述的全部 JSONL 分段行。
pub(super) fn read_segment_lines(
    dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<Vec<SegmentedEventLine>, SessionStoreError> {
    let _guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    read_segment_lines_with_owned_append_lock(dir, segments)
}

/// Reads task-session segment lines after repairing damaged JSONL and synchronizing metadata.
/// 修复损坏 JSONL 并同步元数据后读取子任务会话分段行。
pub(super) fn read_repaired_task_segment_lines(
    task_dir: &Path,
) -> Result<
    (
        SegmentedTaskSessionMainMetadataDocument,
        Vec<SegmentedEventLine>,
    ),
    SessionStoreError,
> {
    let _guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let main_metadata_path = task_main_metadata_path(task_dir);
    let mut main_metadata = read_required_task_main_metadata_document(&main_metadata_path)?;
    let repaired = repair_task_segment_jsonl_files_if_needed_locked(task_dir, &mut main_metadata)?;
    let lines = read_segment_lines_with_owned_append_lock(task_dir, &main_metadata.segments)?;
    if repaired {
        write_json_file(&main_metadata_path, &main_metadata)?;
        if let Some(mut task_metadata) = read_task_metadata_document(&task_metadata_path(task_dir))?
        {
            sync_task_metadata_segments_from_main(&mut task_metadata, &main_metadata);
            if let Ok(updated_at) = main_metadata.updated_at.parse::<u64>() {
                task_metadata.updated_at = updated_at;
            }
            write_json_file(&task_metadata_path(task_dir), &task_metadata)?;
        }
    }
    Ok((main_metadata, lines))
}

/// Reads the newest task-local session-event payload from only the indexed active segment tail.
/// 仅从索引声明的活动分段尾部读取最新子任务本地会话事件载荷。
///
/// # Parameters
/// 参数
/// `task_dir` is the exact task directory selected by task-session id.
/// `task_dir` 是按子任务会话标识选中的精确任务目录。
/// `metadata` is the already-read task-local metadata index.
/// `metadata` 是已读取的任务本地元数据索引。
///
/// # Returns
/// 返回值
/// The newest retained `session_event` payload, or `None` when the bounded active tail has none.
/// 返回最新保留的 `session_event` 载荷；有界活动尾部不存在时返回 `None`。
pub(super) fn read_latest_session_event_payload_from_active_segment(
    task_dir: &Path,
    metadata: &SegmentedTaskSessionMainMetadataDocument,
) -> Result<Option<Value>, SessionStoreError> {
    let Some(active_segment) = metadata
        .segments
        .iter()
        .find(|segment| segment.file == metadata.active_segment)
    else {
        return Err(SessionStoreError::new(format!(
            "TaskSession active segment `{}` is not present in metadata.",
            metadata.active_segment
        )));
    };
    if active_segment.line_count == 0 {
        return Ok(None);
    }
    let active_segment_component = Path::new(&active_segment.file);
    let mut active_segment_components = active_segment_component.components();
    if !matches!(
        (
            active_segment_components.next(),
            active_segment_components.next()
        ),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(SessionStoreError::new(format!(
            "TaskSession active segment path `{}` is not a single file name.",
            active_segment.file
        )));
    }
    let active_segment_path = task_dir.join(&active_segment.file);
    let mut active_segment_file = match File::open(&active_segment_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(to_store_error(error)),
    };
    let active_segment_len = active_segment_file
        .metadata()
        .map_err(to_store_error)?
        .len();
    let tail_len = active_segment_len.min(LATEST_EVENT_ACTIVE_SEGMENT_TAIL_MAX_BYTES);
    let tail_start = active_segment_len.saturating_sub(tail_len);
    active_segment_file
        .seek(SeekFrom::Start(tail_start))
        .map_err(to_store_error)?;
    let mut tail_bytes = Vec::with_capacity(tail_len as usize);
    active_segment_file
        .take(tail_len)
        .read_to_end(&mut tail_bytes)
        .map_err(to_store_error)?;
    let complete_tail = if tail_start == 0 {
        tail_bytes.as_slice()
    } else if let Some(first_newline) = tail_bytes.iter().position(|byte| *byte == b'\n') {
        &tail_bytes[first_newline.saturating_add(1)..]
    } else {
        &[]
    };
    for encoded_line in complete_tail.rsplit(|byte| *byte == b'\n') {
        if encoded_line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(event_line) = serde_json::from_slice::<SegmentedEventLine>(encoded_line) else {
            continue;
        };
        if let Some(session_event) = event_line.payload.get("session_event").cloned()
            && !session_event.is_null()
        {
            return Ok(Some(session_event));
        }
    }
    Ok(None)
}

/// Reads segment lines when the caller already owns the shared append lock.
/// 在调用方已经持有共享追加锁时读取分段行。
///
/// # Parameters
/// 参数
/// `dir` is the session directory containing segment files.
/// `dir` 是包含分段文件的会话目录。
/// `segments` are the metadata-indexed segment files to read.
/// `segments` 是元数据索引的待读取分段文件。
///
/// # Returns
/// 返回值
/// Sorted segment lines retained before the first corrupt row.
/// 返回首条损坏行之前保留并排序后的分段行。
pub(super) fn read_segment_lines_with_owned_append_lock(
    dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<Vec<SegmentedEventLine>, SessionStoreError> {
    read_segment_lines_repairing_locked(dir, segments)
}

use super::*;

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

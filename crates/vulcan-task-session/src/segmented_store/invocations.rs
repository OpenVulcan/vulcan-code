use super::*;

pub(super) fn read_task_invocations_from_main_segments(
    root_session_dir: &Path,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(Vec::new());
    }
    let metadata = read_required_json::<Value>(&metadata_path)?;
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
    let metadata = read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
    read_task_invocations_from_lines(read_segment_lines(task_dir, &metadata.segments)?, parent)
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
        let event = serde_json::from_value::<TaskInvocationEvent>(value).map_err(to_store_error)?;
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
    let mut lines = Vec::new();
    for segment in segments {
        let path = dir.join(&segment.file);
        if !path.exists() {
            continue;
        }
        lines.extend(read_json_lines::<SegmentedEventLine>(&path)?);
    }
    lines.sort_by_key(|line| line.seq);
    Ok(lines)
}

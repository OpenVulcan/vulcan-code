//! Runtime log segment I/O helpers.
//! 运行时日志分段 I/O 辅助。

use super::*;

pub(in crate::workspace_store) fn read_runtime_segment_lines_tolerant(
    path: &Path,
) -> Result<RuntimeLogRepairRead, String> {
    if !path.exists() {
        return Ok(RuntimeLogRepairRead::default());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut read = RuntimeLogRepairRead::default();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 runtime JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RuntimeLogEventLine>(&line) {
            Ok(value) => read.lines.push(value),
            Err(_) => {
                read.corrupt_line_numbers.push(line_number);
            }
        }
    }
    Ok(read)
}

/// Compacts one-based line numbers into a short comma-separated repair summary.
/// 将 1 基行号压缩为简短的逗号分隔修复摘要。
pub(in crate::workspace_store) fn compact_runtime_line_number_summary(
    line_numbers: &[usize],
) -> String {
    const MAX_VISIBLE_LINES: usize = 6;
    if line_numbers.is_empty() {
        return "unknown".to_string();
    }
    let mut visible = line_numbers
        .iter()
        .take(MAX_VISIBLE_LINES)
        .map(|line_number| line_number.to_string())
        .collect::<Vec<_>>();
    if line_numbers.len() > MAX_VISIBLE_LINES {
        visible.push(format!("+{} more", line_numbers.len() - MAX_VISIBLE_LINES));
    }
    visible.join(",")
}

/// Preserves a damaged runtime segment before rewriting the cleaned JSONL file.
/// 在重写清洁 JSONL 文件前保留损坏的运行时分段。
pub(in crate::workspace_store) fn backup_corrupt_runtime_segment(
    path: &Path,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("runtime.jsonl");
    let backup_name = format!(
        "{file_name}.corrupt-{}-{}.bak",
        current_unix_millis_text(),
        uuid::Uuid::new_v4()
    );
    fs::copy(path, path.with_file_name(backup_name))
        .map(|_| ())
        .map_err(to_store_error)
}

/// Refreshes runtime metadata after a tolerant read removed corrupt JSONL rows.
/// 在容错读取移除损坏 JSONL 行后刷新运行时元数据。
pub(in crate::workspace_store) fn refresh_runtime_metadata_from_lines(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
    lines: &[RuntimeLogEventLine],
) -> Result<(), String> {
    let mut next_metadata = metadata.clone();
    next_metadata.next_seq = lines
        .last()
        .map(|line| line.seq.saturating_add(1))
        .unwrap_or(1);
    next_metadata.updated_at = lines
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
    for segment in &mut next_metadata.segments {
        let segment_path = session_dir.join(&segment.file);
        let segment_lines = read_json_lines::<RuntimeLogEventLine>(&segment_path)?;
        segment.line_count = segment_lines.len() as u64;
        segment.byte_len = fs::metadata(&segment_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        segment.end_seq = segment_lines.last().map(|line| line.seq);
        segment.updated_at = segment_lines.last().map(|line| line.created_at.clone());
    }
    write_runtime_metadata(session_dir, &next_metadata)
}

/// Appends one already-sequenced runtime line to the active segment without refreshing metadata.
/// 将一条已分配序号的运行时日志行追加到活动分段，但不刷新元数据。
pub(in crate::workspace_store) fn append_runtime_line_to_active_segment(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
    line: &RuntimeLogEventLine,
) -> Result<(), String> {
    // Runtime single-line appends use the same lock to avoid JSONL byte interleaving.
    // 运行时单行追加使用同一把锁，避免 JSONL 字节交错。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let active_segment_path = session_dir.join(&metadata.active_segment);
    if !active_segment_path.exists() {
        return Err(format!(
            "Runtime active segment is missing: {}",
            active_segment_path.display()
        ));
    }
    append_json_lines_file(&active_segment_path, std::slice::from_ref(line))
}

/// Appends new runtime lines to the active segment and refreshes metadata plus index.
/// 将新的运行时行追加到活动分段，并刷新元数据与索引。
pub(in crate::workspace_store) fn append_runtime_log_tail(
    session_dir: &Path,
    tail: &[RuntimeLogEventLine],
) -> Result<RuntimeLogMetadata, String> {
    // Runtime tail append also refreshes metadata, so keep the batch serialized.
    // 运行时尾部追加也会刷新元数据，因此保持批量串行。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_runtime_metadata(session_dir)?;
    let active_segment_path = session_dir.join(&metadata.active_segment);
    if !active_segment_path.exists() {
        return Err(format!(
            "Runtime active segment is missing: {}",
            active_segment_path.display()
        ));
    }
    append_json_lines_file(&active_segment_path, tail)?;
    let all_lines = read_runtime_log_lines(session_dir)?.unwrap_or_default();
    metadata.next_seq = all_lines
        .last()
        .map(|line| line.seq.saturating_add(1))
        .unwrap_or(1);
    metadata.updated_at = tail
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
    for segment in &mut metadata.segments {
        if segment.file != metadata.active_segment {
            continue;
        }
        let segment_lines = read_json_lines::<RuntimeLogEventLine>(&active_segment_path)?;
        segment.line_count = segment_lines.len() as u64;
        segment.byte_len = fs::metadata(&active_segment_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        segment.end_seq = segment_lines.last().map(|line| line.seq);
        segment.updated_at = segment_lines.last().map(|line| line.created_at.clone());
        break;
    }
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&all_lines))?;
    Ok(metadata)
}

/// Returns whether the existing runtime log is a provider-equivalent prefix of the desired log.
/// 返回现有运行时日志是否为目标日志的 Provider 等价前缀。
pub(in crate::workspace_store) fn runtime_log_prefix_matches(
    existing_lines: &[RuntimeLogEventLine],
    desired_lines: &[RuntimeLogEventLine],
) -> bool {
    existing_lines.len() <= desired_lines.len()
        && existing_lines
            .iter()
            .zip(desired_lines)
            .all(|(existing, desired)| runtime_log_line_matches(existing, desired))
}

/// Compares two runtime lines while ignoring write timestamps that can differ across saves.
/// 比较两条运行时日志行，并忽略不同保存时可能变化的写入时间戳。
pub(in crate::workspace_store) fn runtime_log_line_matches(
    existing: &RuntimeLogEventLine,
    desired: &RuntimeLogEventLine,
) -> bool {
    existing.seq == desired.seq
        && existing.schema_version == desired.schema_version
        && serde_json::to_value(&existing.event).ok() == serde_json::to_value(&desired.event).ok()
}

/// Reads runtime metadata for one segmented session directory.
/// 读取某个分段会话目录的运行时元数据。
pub(in crate::workspace_store) fn read_runtime_metadata(
    session_dir: &Path,
) -> Result<RuntimeLogMetadata, String> {
    read_required_json::<RuntimeLogMetadata>(&runtime_metadata_path(session_dir))
}

/// Writes runtime metadata for one segmented session directory.
/// 写入某个分段会话目录的运行时元数据。
pub(in crate::workspace_store) fn write_runtime_metadata(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
) -> Result<(), String> {
    write_json_file_atomic(&runtime_metadata_path(session_dir), metadata)
}

/// Writes the rebuildable runtime index for one segmented session directory.
/// 写入某个分段会话目录的可重建运行时索引。
pub(in crate::workspace_store) fn write_runtime_index(
    session_dir: &Path,
    index: &RuntimeLogIndex,
) -> Result<(), String> {
    write_json_file_atomic(&runtime_index_path(session_dir), index)
}

/// Returns the runtime metadata path for one session directory.
/// 返回某个会话目录的运行时元数据路径。
pub(in crate::workspace_store) fn runtime_metadata_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE)
}

/// Returns the runtime index path for one session directory.
/// 返回某个会话目录的运行时索引路径。
pub(in crate::workspace_store) fn runtime_index_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE)
}

/// Returns the runtime segment directory path for one session directory.
/// 返回某个会话目录的运行时分段目录路径。
pub(in crate::workspace_store) fn runtime_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_DIR)
}

/// Builds a runtime segment file name relative to the session directory.
/// 构建相对会话目录的运行时分段文件名。
pub(in crate::workspace_store) fn runtime_segment_file_name(index: usize) -> String {
    format!("{SEGMENTED_RUNTIME_DIR}/r{index:03}.jsonl")
}

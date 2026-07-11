//! Low-level segmented JSONL and metadata I/O helpers.
//! 底层分段 JSONL 与元数据 I/O 辅助。

use super::*;

/// Reads one segment event file without mutating on-disk data.
/// 读取单个分段事件文件，且不改动磁盘数据。
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

/// Repairs malformed segmented JSONL files when the caller already owns the append lock.
/// 当调用方已经持有追加锁时修复格式损坏的分段 JSONL 文件。
pub(in crate::workspace_store) fn repair_segmented_session_jsonl_files_if_needed_locked(
    session_dir: &Path,
) -> Result<bool, String> {
    let mut metadata = read_session_metadata(session_dir)?;
    let original_segments = metadata.segments.clone();
    let mut retained_segment_lines: Vec<(String, Vec<SegmentedSessionEventLine>)> = Vec::new();
    let mut changed = original_segments.is_empty();
    let mut discard_remaining_segments = false;

    for segment in &original_segments {
        let segment_path = session_dir.join(&segment.file);
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

    if repair_duplicate_segmented_message_indexes(&mut retained_segment_lines) {
        for (file, lines) in &retained_segment_lines {
            let segment_path = session_dir.join(file);
            if lines.is_empty() {
                if segment_path.exists() {
                    fs::remove_file(&segment_path).map_err(to_store_error)?;
                }
            } else {
                write_json_lines_file_atomic(&segment_path, lines)?;
            }
        }
        changed = true;
    }

    let retained_segments = retained_segment_lines
        .iter()
        .map(|(file, lines)| rebuild_segment_metadata(file, lines))
        .collect::<Result<Vec<_>, _>>()?;
    let active_segment = retained_segments
        .last()
        .map(|segment| segment.file.clone())
        .unwrap_or_else(|| segment_file_name(1));
    let next_global_seq = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter().map(|line| line.seq))
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(0);
    let latest_rollback_seq = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter())
        .filter(|line| line.kind == SegmentedSessionEventKind::ThreadRolledBack)
        .map(|line| line.seq)
        .max();
    changed |= metadata.segments != retained_segments;
    changed |= metadata.active_segment != active_segment;
    changed |= metadata.next_global_seq != next_global_seq;
    changed |= metadata.latest_rollback_seq != latest_rollback_seq;

    if !changed {
        return Ok(false);
    }

    let retained_segments = if retained_segments.is_empty() {
        let empty_segment = SegmentMetadata::empty(segment_file_name(1), 0);
        write_json_lines_file_atomic::<SegmentedSessionEventLine>(
            &session_dir.join(&empty_segment.file),
            &[],
        )?;
        vec![empty_segment]
    } else {
        retained_segments
    };
    metadata.active_segment = retained_segments
        .last()
        .map(|segment| segment.file.clone())
        .unwrap_or_else(|| segment_file_name(1));
    metadata.next_global_seq = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter().map(|line| line.seq))
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(0);
    metadata.segments = retained_segments;
    metadata.latest_rollback_seq = latest_rollback_seq;
    metadata.updated_at = current_unix_millis_text();

    let effective_lines = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter())
        .filter(|line| {
            metadata
                .latest_rollback_seq
                .is_none_or(|seq| line.seq > seq)
        })
        .cloned()
        .collect::<Vec<_>>();
    let messages = segmented_lines_to_session_messages(&effective_lines)?;
    update_segmented_message_checkpoint(&mut metadata, &messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(true)
}

/// Removes older duplicate persisted transcript rows before metadata is rebuilt.
/// 在重建元数据之前移除较旧的重复持久化转录行。
///
/// # Parameters
/// 参数
/// `retained_segment_lines` contains repaired segment lines in persisted segment order.
/// `retained_segment_lines` 包含按持久化分段顺序排列的已修复分段行。
///
/// # Returns
/// 返回值
/// `true` when at least one duplicate message row was removed.
/// 当至少移除一条重复消息行时返回 `true`。
fn repair_duplicate_segmented_message_indexes(
    retained_segment_lines: &mut [(String, Vec<SegmentedSessionEventLine>)],
) -> bool {
    let mut latest_line_by_message_index = BTreeMap::new();
    for (_, lines) in retained_segment_lines.iter() {
        for line in lines {
            if let Some(message_index) = repair_message_index_from_segment_line(line) {
                latest_line_by_message_index.insert(message_index, line.clone());
            }
        }
    }

    let mut changed = false;
    let mut emitted_message_indexes = BTreeSet::new();
    for (_, lines) in retained_segment_lines.iter_mut() {
        let previous_lines = std::mem::take(lines);
        let mut repaired_lines = Vec::with_capacity(previous_lines.len());
        for line in previous_lines {
            let Some(message_index) = repair_message_index_from_segment_line(&line) else {
                repaired_lines.push(line);
                continue;
            };
            if !emitted_message_indexes.insert(message_index) {
                changed = true;
                continue;
            }
            let replacement = latest_line_by_message_index
                .get(&message_index)
                .map(|latest_line| {
                    repair_message_line_with_latest_payload(line.clone(), latest_line)
                })
                .unwrap_or_else(|| line.clone());
            if replacement != line {
                changed = true;
            }
            repaired_lines.push(replacement);
        }
        *lines = repaired_lines;
    }
    changed
}

/// Keeps one message row position while applying the newest duplicate payload.
/// 保留消息行位置，同时套用最新重复行的载荷内容。
fn repair_message_line_with_latest_payload(
    mut original: SegmentedSessionEventLine,
    latest: &SegmentedSessionEventLine,
) -> SegmentedSessionEventLine {
    original.timestamp = latest.timestamp.clone();
    original.kind = latest.kind;
    original.payload = latest.payload.clone();
    original
}

/// Reads the transcript message index from one repair candidate line.
/// 从单条修复候选行中读取转录消息索引。
fn repair_message_index_from_segment_line(line: &SegmentedSessionEventLine) -> Option<usize> {
    line.payload.get("message")?;
    line.payload
        .get("message_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

/// Parsed JSONL content and repair status for one segment file.
/// 单个分段文件的 JSONL 解析内容与修复状态。
struct SegmentJsonlRepairRead {
    /// Retained valid event lines.
    /// 保留下来的有效事件行。
    lines: Vec<SegmentedSessionEventLine>,
    /// Whether the physical file or metadata should be rewritten.
    /// 是否需要重写物理文件或元数据。
    changed: bool,
    /// Whether a malformed line forced truncating this segment suffix.
    /// 是否存在格式错误行导致当前分段后缀被截断。
    truncated_at_corrupt_line: bool,
}

/// Reads one segment file into valid lines and marks the first malformed line as the cutoff.
/// 将单个分段文件读取为有效事件行，并把首条格式错误行标记为截断点。
fn read_segment_event_lines_for_repair(
    segment_path: &Path,
) -> Result<SegmentJsonlRepairRead, String> {
    if !segment_path.exists() {
        return Ok(SegmentJsonlRepairRead {
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
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            changed = true;
            truncated_at_corrupt_line = true;
            break;
        };
        if !segmented_event_value_has_known_kind(&value) {
            changed = true;
            continue;
        }
        match serde_json::from_value::<SegmentedSessionEventLine>(value) {
            Ok(event_line) => lines.push(event_line),
            Err(_) => {
                changed = true;
                truncated_at_corrupt_line = true;
                break;
            }
        }
    }
    Ok(SegmentJsonlRepairRead {
        lines,
        changed,
        truncated_at_corrupt_line,
    })
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
    let actual_line_byte_len = append_json_line(&segment_path, line)?;
    update_segment_metadata(segment, line, actual_line_byte_len, turn_index);
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

/// Writes display event lines once and returns metadata collected during the same serialization pass.
/// 单次写入展示事件行，并返回同一轮序列化过程中收集的元数据。
pub(in crate::workspace_store) fn write_segment_event_lines_atomic_with_metadata(
    path: &Path,
    file: &str,
    lines: &[SegmentedSessionEventLine],
) -> Result<SegmentMetadata, String> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let start_seq = lines.first().map(|line| line.seq).unwrap_or(1);
    let mut segment = SegmentMetadata::empty(file.to_string(), start_seq);
    let mut encoded_lines = Vec::new();
    for line in lines {
        let line_text = serde_json::to_string(line).map_err(to_store_error)?;
        let line_byte_len = line_text.len() as u64 + 1;
        encoded_lines.extend_from_slice(line_text.as_bytes());
        encoded_lines.push(b'\n');
        update_segment_metadata(
            &mut segment,
            line,
            line_byte_len,
            segmented_line_turn_index(line),
        );
    }
    write_file_bytes_atomic(path, &encoded_lines, "segmented event JSONL")?;
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
#[cfg(test)]
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
    vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
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
pub(in crate::workspace_store) fn append_json_line<T>(path: &Path, value: &T) -> Result<u64, String>
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
        .read(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    let boundary_bytes = ensure_jsonl_append_boundary(&mut file)?;
    file.write_all(encoded_line.as_bytes())
        .map_err(to_store_error)?;
    // Synchronize durable event bytes before callers publish the corresponding segment metadata.
    // 在调用方发布对应分段元数据前同步耐久事件字节。
    file.sync_data().map_err(to_store_error)?;
    Ok(boundary_bytes + encoded_line.len() as u64)
}

/// Reads an optional JSON document through one authoritative file-open operation.
/// 通过一次权威文件打开操作读取一个可选 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the exact JSON document path to open and parse.
/// `path` 是需要打开并解析的精确 JSON 文档路径。
///
/// # Returns
/// 返回值
/// The parsed document, `None` only for `NotFound`, or a path-aware open or parse failure.
/// 返回已解析文档；仅在 `NotFound` 时返回 `None`，否则返回包含路径的打开或解析错误。
pub(in crate::workspace_store) fn read_optional_json<T>(path: &Path) -> Result<Option<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    // file comes from the authoritative open result so inspection failures cannot look absent.
    // file 来自权威打开结果，因此检查失败不会伪装成缺失。
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("打开 JSON 文件失败 `{}`：{error}", path.display())),
    };
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .map(Some)
        .map_err(|error| format!("解析 JSON 文件失败 `{}`：{error}", path.display()))
}

/// Reads a required JSON document from disk.
/// 从磁盘读取一个必需的 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the exact JSON document path that must exist and parse successfully.
/// `path` 是必须存在且必须成功解析的精确 JSON 文档路径。
///
/// # Returns
/// 返回值
/// The parsed document, or a path-aware error when the file is absent, unreadable, or invalid.
/// 返回已解析文档；文件缺失、不可读或无效时返回包含路径的错误。
pub(in crate::workspace_store) fn read_required_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    read_optional_json(path)?.ok_or_else(|| format!("必需 JSON 文件不存在：{}", path.display()))
}

/// Writes one complete JSON document through the shared atomic byte publisher.
/// 通过共享原子字节发布器写入一个完整 JSON 文档。
pub(in crate::workspace_store) fn write_json_file_atomic<T>(
    path: &Path,
    value: &T,
) -> Result<(), String>
where
    T: Serialize,
{
    // encoded is complete before any directory or staging file is created.
    // encoded 在创建任何目录或暂存文件前已经完整生成。
    let mut encoded = serde_json::to_vec_pretty(value).map_err(to_store_error)?;
    encoded.push(b'\n');
    write_file_bytes_atomic(path, &encoded, "JSON document")
}

/// Writes complete JSONL values through the shared atomic byte publisher.
/// 通过共享原子字节发布器写入完整 JSONL 值集合。
pub(in crate::workspace_store) fn write_json_lines_file_atomic<T>(
    path: &Path,
    values: &[T],
) -> Result<(), String>
where
    T: Serialize,
{
    // encoded is complete before any directory or staging file is created.
    // encoded 在创建任何目录或暂存文件前已经完整生成。
    let encoded = encode_json_lines(values)?;
    write_file_bytes_atomic(path, &encoded, "JSONL document")
}

/// Encodes complete newline-delimited JSON values without touching the filesystem.
/// 在不接触文件系统的情况下编码完整的换行分隔 JSON 值。
///
/// # Parameters
/// 参数
/// `values` contains ordered serializable records for one complete JSONL document.
/// `values` 包含一份完整 JSONL 文档的有序可序列化记录。
///
/// # Returns
/// 返回值
/// One complete byte buffer ending every record with a newline, or a serialization failure.
/// 返回每条记录都以换行结尾的完整字节缓冲，序列化失败时返回错误。
pub(in crate::workspace_store) fn encode_json_lines<T>(values: &[T]) -> Result<Vec<u8>, String>
where
    T: Serialize,
{
    // encoded accumulates the whole candidate before any persistence mutation is authorized.
    // encoded 在允许任何持久化修改前累积完整候选内容。
    let mut encoded = Vec::new();
    for value in values {
        serde_json::to_writer(&mut encoded, value).map_err(to_store_error)?;
        encoded.push(b'\n');
    }
    Ok(encoded)
}

/// Publishes complete file bytes through an exclusively created sibling staging file.
/// 通过独占创建的同级暂存文件发布完整文件字节。
///
/// # Parameters
/// 参数
/// `path` is the final file path replaced after staging succeeds.
/// `path` 是暂存成功后被替换的最终文件路径。
/// `encoded` is the fully materialized file content.
/// `encoded` 是已经完整生成的文件内容。
/// `artifact_kind` is the stable English artifact label used in diagnostics.
/// `artifact_kind` 是诊断信息使用的稳定英文产物标签。
///
/// # Returns
/// 返回值
/// Returns success after the staged bytes and renamed parent entry are synchronized where supported.
/// 暂存字节与受支持平台上的重命名父目录项完成同步后返回成功。
pub(in crate::workspace_store) fn write_file_bytes_atomic(
    path: &Path,
    encoded: &[u8],
    artifact_kind: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // existing_permissions preserves the current access policy across replacement.
    // existing_permissions 在替换期间保留当前访问策略。
    let existing_permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Refusing to replace symbolic-link {artifact_kind}: {}",
                path.display()
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(to_store_error(error)),
    };
    // staged_path and staged_file are exclusively owned by the current publication attempt.
    // staged_path 与 staged_file 由当前发布尝试独占。
    let (staged_path, mut staged_file) = create_unique_staging_file(path)?;
    // staging_result makes complete bytes and inherited permissions durable before rename.
    // staging_result 在重命名前使完整字节与继承权限耐久化。
    let staging_result = (|| -> std::io::Result<()> {
        staged_file.write_all(encoded)?;
        if let Some(permissions) = existing_permissions {
            staged_file.set_permissions(permissions)?;
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        remove_staging_file(&staged_path);
        return Err(format!(
            "Failed to stage {artifact_kind} `{}` for `{}`: {error}",
            staged_path.display(),
            path.display()
        ));
    }
    drop(staged_file);

    if let Err(error) = fs::rename(&staged_path, path) {
        remove_staging_file(&staged_path);
        return Err(format!(
            "Failed to replace {artifact_kind} `{}` from `{}`: {error}",
            path.display(),
            staged_path.display()
        ));
    }
    // POSIX durability requires synchronizing the renamed parent directory entry.
    // POSIX 持久性要求同步重命名后的父目录项。
    #[cfg(unix)]
    synchronize_parent_directory(path)?;
    Ok(())
}

/// Creates one unique sibling staging file with authoritative create-new collision detection.
/// 通过权威 create-new 碰撞检测创建一个唯一同级暂存文件。
///
/// # Parameters
/// 参数
/// `path` is the final file path whose exact name is retained in the staging name.
/// `path` 是暂存名称中保留精确名称的最终文件路径。
///
/// # Returns
/// 返回值
/// The unique staging path and its exclusively created writable file handle.
/// 返回唯一暂存路径及其独占创建的可写文件句柄。
fn create_unique_staging_file(path: &Path) -> Result<(PathBuf, File), String> {
    // parent keeps staging and target on the same filesystem.
    // parent 使暂存文件与目标文件保持在同一文件系统。
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // file_name preserves non-Unicode target identity without lossy conversion.
    // file_name 无需有损转换即可保留非 Unicode 目标身份。
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("Atomic file path has no file name: {}", path.display()))?;
    loop {
        // staged_name combines the exact target name with a diagnostic random suffix.
        // staged_name 组合精确目标名称与可诊断随机后缀。
        let mut staged_name = OsString::from(".");
        staged_name.push(file_name);
        staged_name.push(format!(".vulcan-stage-{}", Uuid::new_v4()));
        // staged_path is always a sibling of the final target.
        // staged_path 始终是最终目标的同级文件。
        let staged_path = parent.join(staged_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
        {
            Ok(file) => return Ok((staged_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(to_store_error(error)),
        }
    }
}

/// Removes one exclusively owned staging file after a failed publication.
/// 在发布失败后删除一个独占暂存文件。
///
/// # Parameters
/// 参数
/// `staged_path` is the unpublished file owned by the failed attempt.
/// `staged_path` 是失败尝试独占且尚未发布的文件。
///
/// # Returns
/// 返回值
/// Best-effort cleanup has no return value because the primary publication error remains authoritative.
/// 尽力清理不返回值，因为首要发布错误保持权威性。
fn remove_staging_file(staged_path: &Path) {
    let _ = fs::remove_file(staged_path);
}

/// Synchronizes the parent directory containing a newly renamed entry on POSIX.
/// 在 POSIX 上同步包含新重命名目录项的父目录。
///
/// # Parameters
/// 参数
/// `path` is the final file or directory whose parent directory entry changed.
/// `path` 是父目录项发生变化的最终文件或目录。
///
/// # Returns
/// 返回值
/// Returns success after the parent directory metadata reaches durable storage.
/// 父目录元数据到达持久存储后返回成功。
#[cfg(unix)]
fn synchronize_parent_directory(path: &Path) -> Result<(), String> {
    // parent is the directory containing both staging and final paths.
    // parent 是同时包含暂存路径与最终路径的目录。
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // directory exists only for the post-rename synchronization call.
    // directory 仅在重命名后的同步调用期间存在。
    let directory = File::open(parent).map_err(to_store_error)?;
    directory.sync_all().map_err(to_store_error)
}

/// Appends JSONL values to an existing file without creating a missing persistence artifact.
/// 将 JSONL 值追加到已有文件，且不会创建缺失的持久化产物。
///
/// # Parameters
/// 参数
/// `path` is the exact existing JSONL file that receives the serialized values.
/// `path` 是接收序列化值的精确已有 JSONL 文件。
/// `values` contains the ordered records appended as newline-delimited JSON.
/// `values` 包含按顺序以换行分隔 JSON 形式追加的记录。
///
/// # Returns
/// 返回值
/// Success after all bytes are flushed and synchronized, or a path-aware persistence error.
/// 全部字节刷新并同步后返回成功，否则返回包含路径的持久化错误。
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
    // Build the full batch first so each append call emits one contiguous byte buffer.
    // 先构建完整批次，使每次追加调用输出一个连续字节缓冲。
    let mut encoded_batch = String::new();
    for value in values {
        encoded_batch.push_str(&serde_json::to_string(value).map_err(to_store_error)?);
        encoded_batch.push('\n');
    }
    let mut file = OpenOptions::new()
        .append(true)
        .read(true)
        .open(path)
        .map_err(|error| {
            format!(
                "Failed to open existing JSONL file `{}` for append: {error}",
                path.display()
            )
        })?;
    ensure_jsonl_append_boundary(&mut file)?;
    file.write_all(encoded_batch.as_bytes())
        .map_err(to_store_error)?;
    // Synchronize the whole batch before runtime metadata/index files can reference it.
    // 在运行时元数据与索引文件引用该批次前同步完整批次。
    file.sync_data().map_err(to_store_error)
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
fn ensure_jsonl_append_boundary(file: &mut File) -> Result<u64, String> {
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

/// Creates every missing destination-parent directory and durably publishes each new path component.
/// 创建所有缺失的目标父目录，并耐久发布每个新路径片段。
///
/// # Parameters
/// 参数
/// `destination_parent` is the complete parent directory required by a later session-directory move.
/// `destination_parent` 是后续会话目录移动所需的完整父目录。
///
/// # Returns
/// 返回值
/// Returns success only after every required component is a real directory and each new entry is synchronized.
/// 仅当所有必需片段都是真实目录且每个新目录项均已同步后返回成功。
fn create_segmented_session_move_parent_durably(destination_parent: &Path) -> Result<(), String> {
    // missing_directories records absent components from leaf to the nearest existing ancestor.
    // missing_directories 按叶子到最近现有祖先的顺序记录缺失目录片段。
    let mut missing_directories = Vec::new();
    // current_directory walks upward without following a symbolic-link parent.
    // current_directory 向上遍历且不跟随符号链接父目录。
    let mut current_directory = destination_parent;
    loop {
        match fs::symlink_metadata(current_directory) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "Segmented session move parent is not a real directory: {}",
                    current_directory.display()
                ));
            }
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing_directories.push(current_directory.to_path_buf());
                current_directory = current_directory.parent().ok_or_else(|| {
                    format!(
                        "Segmented session move parent has no existing ancestor: {}",
                        destination_parent.display()
                    )
                })?;
            }
            Err(error) => return Err(to_store_error(error)),
        }
    }

    // Create ancestors before descendants so every parent entry can be synchronized immediately.
    // 先创建祖先再创建后代，使每个父目录项都能立即同步。
    for directory in missing_directories.into_iter().rev() {
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // competing_metadata validates a directory concurrently published after preflight.
                // competing_metadata 验证预检后由并发方发布的目录。
                let competing_metadata =
                    fs::symlink_metadata(&directory).map_err(to_store_error)?;
                if competing_metadata.file_type().is_symlink() || !competing_metadata.is_dir() {
                    return Err(format!(
                        "Segmented session move parent is not a real directory: {}",
                        directory.display()
                    ));
                }
            }
            Err(error) => return Err(to_store_error(error)),
        }
        // Synchronizing the parent after every creation makes the full destination path durable before rename.
        // 每次创建后同步父目录，使完整目标路径在重命名前已经耐久化。
        #[cfg(unix)]
        synchronize_parent_directory(&directory)?;
    }
    Ok(())
}

/// Moves one real segmented session directory without overwriting any destination entry.
/// 移动一个真实分段会话目录，且不覆盖任何已有目标目录项。
///
/// # Parameters
/// 参数
/// `source` is the existing real directory to move.
/// `source` 是要移动的现有真实目录。
/// `destination` is the absent final directory path.
/// `destination` 是必须不存在的最终目录路径。
///
/// # Returns
/// 返回值
/// Returns success after rename; callers own metadata updates and parent-directory synchronization.
/// 重命名后返回成功；元数据更新与父目录同步由调用方负责。
pub(in crate::workspace_store) fn move_segmented_session_directory(
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    // source_metadata rejects missing paths, non-directories, and symbolic-link indirection.
    // source_metadata 拒绝缺失路径、非目录与符号链接间接跳转。
    let source_metadata = fs::symlink_metadata(source).map_err(to_store_error)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err(format!(
            "Segmented session source is not a real directory: {}",
            source.display()
        ));
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(format!(
                "Segmented session destination already exists: {}",
                destination.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(to_store_error(error)),
    }
    if let Some(parent) = destination.parent() {
        create_segmented_session_move_parent_durably(parent)?;
    }
    fs::rename(source, destination).map_err(to_store_error)
}

/// Synchronizes both parent directories affected by one completed session-directory move.
/// 同步一次已完成会话目录移动所影响的两个父目录。
///
/// # Parameters
/// 参数
/// `source` is the removed source directory path.
/// `source` 是已经移除的源目录路径。
/// `destination` is the newly published destination directory path.
/// `destination` 是新发布的目标目录路径。
///
/// # Returns
/// 返回值
/// Returns success after the destination publication is durable before the source removal on supported platforms.
/// 在受支持平台上目标发布先于源删除完成耐久化后返回成功。
pub(in crate::workspace_store) fn synchronize_segmented_session_directory_move(
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        // Persist the destination first so a crash cannot durably remove the source before publishing its replacement.
        // 先持久化目标，避免崩溃造成源已耐久删除而替代目标尚未发布。
        synchronize_parent_directory(destination)?;
        if source.parent() != destination.parent() {
            synchronize_parent_directory(source)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (source, destination);
    }
    Ok(())
}

/// Reads JSONL values from one required file.
/// 从一个必需文件读取 JSONL 值。
///
/// # Parameters
/// 参数
/// `path` is the exact required JSONL file opened and decoded line by line.
/// `path` 是需要打开并逐行解码的精确必需 JSONL 文件。
///
/// # Returns
/// 返回值
/// Ordered decoded values, or a path-and-line-aware open, read, or parse failure.
/// 返回有序解码值，否则返回包含路径与行号的打开、读取或解析失败。
pub(in crate::workspace_store) fn read_json_lines<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)
        .map_err(|error| format!("打开 JSONL 文件失败 `{}`：{error}", path.display()))?;
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

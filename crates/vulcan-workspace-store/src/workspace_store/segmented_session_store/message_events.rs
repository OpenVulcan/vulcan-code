//! Message event append and mapping helpers.
//! 消息事件追加与映射辅助。

use super::*;

pub(in crate::workspace_store) fn append_segmented_session_events(
    session_dir: &Path,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Hold one process-local append lock across metadata and segment updates.
    // 在元数据与分段更新期间持有同一个进程内追加锁。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    // Load metadata first so sequence numbers and active segment remain authoritative.
    // 先加载元数据，确保序号与活动分段以元数据为准。
    let mut metadata = read_session_metadata(session_dir)?;
    // Append all events and update the in-memory metadata only after file writes succeed.
    // 追加全部事件，并且只在文件写入成功后更新内存元数据。
    let lines = append_events_with_metadata(session_dir, &mut metadata, events)?;
    write_metadata(session_dir, &metadata)?;
    Ok(lines)
}

/// Appends session boundary events while making persisted payload sequence match JSONL sequence.
/// 追加 会话边界事件，并确保持久化 payload 序号与 JSONL 行序号一致。
pub(in crate::workspace_store) fn append_segmented_boundary_events(
    session_dir: &Path,
    events: &[SessionBoundaryEvent],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    // Serialize boundary and display transcript appends sharing the same active segment.
    // 串行化共享同一活动分段的边界事件与展示转录追加。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    // Load metadata before every batch so the store remains the only sequence authority.
    // 批量追加前先加载元数据，确保存储层始终是唯一序号来源。
    let mut metadata = read_session_metadata(session_dir)?;
    let mut lines = Vec::new();
    for event in events {
        let assigned_sequence = metadata.next_global_seq;
        let event_input = SegmentedSessionEventInput {
            timestamp: event.timestamp_ms.to_string(),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: session_boundary_payload(event, assigned_sequence),
            turn_index: None,
        };
        lines.extend(append_events_with_metadata(
            session_dir,
            &mut metadata,
            std::slice::from_ref(&event_input),
        )?);
    }
    write_metadata(session_dir, &metadata)?;
    Ok(lines)
}

/// Appends only the not-yet-persisted tail of a transcript message slice to the main segmented session.
/// 只将转录消息切片中尚未持久化的尾部追加到主分段会话。
pub(in crate::workspace_store) fn append_segmented_session_message_tail(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep transcript tail detection, sequence assignment, and file append atomic in-process.
    // 让转录尾部检测、序号分配与文件追加在进程内保持原子。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let first_new_index = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if messages.len() < first_new_index {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted message count {} exceeds incoming message count {}.",
            first_new_index,
            messages.len()
        ));
    }
    if messages.len() == first_new_index {
        validate_segmented_message_tail_boundary(&metadata, messages)?;
        return Ok(Vec::new());
    }
    validate_segmented_message_tail_boundary(&metadata, messages)?;
    let mut new_message_ids = BTreeSet::new();
    for (index, message) in messages.iter().enumerate().skip(first_new_index) {
        let message_id = stable_segmented_message_id(index, message);
        if !new_message_ids.insert(message_id.clone()) {
            return Err(format!(
                "Segmented session append rejected duplicate tail message `{message_id}` after new message index {first_new_index}."
            ));
        }
    }
    let turn_indexes =
        assign_segmented_tail_message_turn_indexes(&metadata, &messages[first_new_index..]);
    let mut events = messages
        .iter()
        .enumerate()
        .skip(first_new_index)
        .zip(turn_indexes)
        .map(|((index, message), turn_index)| {
            segmented_event_input_from_message(index, message, turn_index)
        })
        .collect::<Result<Vec<_>, _>>()?;
    backfill_tool_detail_ref_sequences(&mut events, metadata.next_global_seq);
    let appended_lines = append_events_with_metadata(session_dir, &mut metadata, &events)?;
    update_segmented_message_checkpoint(&mut metadata, messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(appended_lines)
}

/// Appends or physically rewrites one transcript update without creating rollback bloat.
/// 追加或物理重写一次转录更新，避免创建回滚膨胀。
pub(in crate::workspace_store) fn append_segmented_session_message_update(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    match append_segmented_session_message_tail(session_dir, messages) {
        Ok(lines) => Ok(lines),
        Err(error) if error.contains("non-tail transcript change") => {
            rewrite_segmented_session_message_records(session_dir, messages)
        }
        Err(error) => Err(error),
    }
}

/// Appends caller-verified new display messages after the persisted transcript tail.
/// 在已持久化转录尾部之后追加调用方已确认的新显示消息。
pub(in crate::workspace_store) fn append_segmented_session_message_suffix(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let first_new_index = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    let mut new_message_ids = BTreeSet::new();
    for (offset, message) in messages.iter().enumerate() {
        let message_id = stable_segmented_message_id(first_new_index + offset, message);
        if !new_message_ids.insert(message_id.clone()) {
            return Err(format!(
                "Segmented session suffix append rejected duplicate message `{message_id}` after message index {first_new_index}."
            ));
        }
    }
    let turn_indexes = assign_segmented_tail_message_turn_indexes(&metadata, messages);
    let mut events = messages
        .iter()
        .enumerate()
        .zip(turn_indexes)
        .map(|((offset, message), turn_index)| {
            segmented_event_input_from_message(first_new_index + offset, message, turn_index)
        })
        .collect::<Result<Vec<_>, _>>()?;
    backfill_tool_detail_ref_sequences(&mut events, metadata.next_global_seq);
    let appended_lines = append_events_with_metadata(session_dir, &mut metadata, &events)?;
    advance_segmented_message_checkpoint_with_suffix(&mut metadata, first_new_index, messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(appended_lines)
}

/// Physically rewrites persisted message rows to match the supplied transcript.
/// 将已持久化的消息行物理重写为给定转录。
pub(in crate::workspace_store) fn rewrite_segmented_session_message_records(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Message-row rewrites update segment files and metadata under the same append lock.
    // 消息行重写会在同一把追加锁下更新分段文件与元数据。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let turn_indexes = assign_segmented_message_turn_indexes(messages);
    let original_segments = metadata.segments.clone();
    let mut rewritten_segments = Vec::new();
    let mut changed_lines = Vec::new();
    let mut seen_message_indexes = BTreeSet::new();
    let mut latest_rollback_seq = None;
    let mut highest_retained_seq = None;
    for segment in &original_segments {
        let segment_path = session_dir.join(&segment.file);
        let original_lines = read_segment_event_lines(&segment_path)?;
        let mut rewritten_lines = Vec::new();
        for line in original_lines.iter().cloned() {
            if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                latest_rollback_seq = Some(line.seq);
            }
            if line.payload.get("message").is_none() {
                rewritten_lines.push(line);
                continue;
            }
            let message_index = segmented_line_message_index(&line)?;
            if !seen_message_indexes.insert(message_index) {
                return Err(format!(
                    "Segmented session message rewrite rejected duplicate persisted message index {message_index}."
                ));
            }
            let Some(message) = messages.get(message_index) else {
                continue;
            };
            let rewritten_line = segmented_line_from_message(
                line.seq,
                message_index,
                message,
                turn_indexes[message_index],
            )?;
            if rewritten_line != line {
                changed_lines.push(rewritten_line.clone());
            }
            rewritten_lines.push(rewritten_line);
        }
        if rewritten_lines != original_lines {
            write_json_lines_file_atomic(&segment_path, &rewritten_lines)?;
        }
        if !rewritten_lines.is_empty() {
            if let Some(segment_highest_seq) = rewritten_lines.iter().map(|line| line.seq).max() {
                highest_retained_seq = Some(
                    highest_retained_seq
                        .map(|highest_seq: u64| highest_seq.max(segment_highest_seq))
                        .unwrap_or(segment_highest_seq),
                );
            }
            rewritten_segments.push(rebuild_segment_metadata(&segment.file, &rewritten_lines)?);
        } else if segment_path.exists() {
            fs::remove_file(&segment_path).map_err(to_store_error)?;
        }
    }
    if rewritten_segments.is_empty() {
        let empty_segment = SegmentMetadata::empty(segment_file_name(1), 0);
        write_json_lines_file_atomic::<SegmentedSessionEventLine>(
            &session_dir.join(&empty_segment.file),
            &[],
        )?;
        metadata.active_segment = empty_segment.file.clone();
        metadata.next_global_seq = empty_segment.start_seq;
        metadata.segments = vec![empty_segment];
    } else {
        metadata.active_segment = rewritten_segments
            .last()
            .map(|segment| segment.file.clone())
            .unwrap_or_else(|| segment_file_name(1));
        metadata.next_global_seq = highest_retained_seq
            .map(|seq| seq.saturating_add(1))
            .unwrap_or(1);
        metadata.segments = rewritten_segments;
    }
    metadata.latest_rollback_seq = latest_rollback_seq;
    let mut missing_events = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if seen_message_indexes.contains(&index) {
            continue;
        }
        missing_events.push(segmented_event_input_from_message(
            index,
            message,
            turn_indexes[index],
        )?);
    }
    backfill_tool_detail_ref_sequences(&mut missing_events, metadata.next_global_seq);
    let appended_lines = append_events_with_metadata(session_dir, &mut metadata, &missing_events)?;
    changed_lines.extend(appended_lines);
    update_segmented_message_checkpoint(&mut metadata, messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(changed_lines)
}

/// Builds one event line for an existing sequence from a transcript message.
/// 使用既有序号从转录消息构建一条事件行。
fn segmented_line_from_message(
    seq: u64,
    index: usize,
    message: &SessionMessageEntry,
    turn_index: i64,
) -> Result<SegmentedSessionEventLine, String> {
    let event = segmented_event_input_from_message(index, message, turn_index)?;
    Ok(SegmentedSessionEventLine {
        seq,
        timestamp: event.timestamp,
        kind: event.kind,
        payload: event.payload,
    })
}

/// Reads the persisted transcript message index from one message-bearing line.
/// 从一条携带消息的事件行读取已持久化的转录消息索引。
fn segmented_line_message_index(line: &SegmentedSessionEventLine) -> Result<usize, String> {
    let index = line
        .payload
        .get("message_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            format!(
                "Segmented session message rewrite rejected message line without message_index at seq {}.",
                line.seq
            )
        })?;
    usize::try_from(index).map_err(to_store_error)
}

/// Physically truncates a segmented session before one persisted message without adding display events.
/// 在不追加展示事件的情况下，将分段会话物理截断到某条已持久化消息之前。
pub(in crate::workspace_store) fn truncate_segmented_session_before_message(
    session_dir: &Path,
    message_id: &str,
    retained_messages: &[SessionMessageEntry],
) -> Result<(), String> {
    // Editing a prompt is semantically identical to rewinding the transcript, so no rollback marker is appended.
    // 编辑提示词语义上等同回退转录，因此这里不追加任何回滚标记。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let mut retained_segment_lines: Vec<(String, Vec<SegmentedSessionEventLine>)> = Vec::new();
    let mut found_cutoff = false;
    let mut latest_rollback_seq = None;
    let original_segments = metadata.segments.clone();
    for segment in &original_segments {
        let segment_path = session_dir.join(&segment.file);
        let lines = read_segment_event_lines(&segment_path)?;
        let mut retained_lines = Vec::new();
        if !found_cutoff {
            for line in lines {
                if segmented_line_message_id(&line) == Some(message_id) {
                    found_cutoff = true;
                    break;
                }
                if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                    latest_rollback_seq = Some(line.seq);
                }
                retained_lines.push(line);
            }
        }
        if !retained_lines.is_empty() {
            retained_segment_lines.push((segment.file.clone(), retained_lines));
        }
        if found_cutoff {
            break;
        }
    }
    if !found_cutoff {
        return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"));
    }
    let mut retained_segments = Vec::new();
    for (file, retained_lines) in &retained_segment_lines {
        let segment_path = session_dir.join(file);
        write_json_lines_file_atomic(&segment_path, retained_lines)?;
        retained_segments.push(rebuild_segment_metadata(file, retained_lines)?);
    }
    for segment in original_segments.iter().filter(|segment| {
        !retained_segment_lines
            .iter()
            .any(|(file, _)| file == &segment.file)
    }) {
        let segment_path = session_dir.join(&segment.file);
        if segment_path.exists() {
            let _ = fs::remove_file(segment_path);
        }
    }
    if retained_segments.is_empty() {
        let empty_segment = SegmentMetadata::empty(segment_file_name(1), 0);
        write_json_lines_file_atomic::<SegmentedSessionEventLine>(
            &session_dir.join(&empty_segment.file),
            &[],
        )?;
        metadata.active_segment = empty_segment.file.clone();
        metadata.next_global_seq = empty_segment.start_seq;
        metadata.segments = vec![empty_segment];
        metadata.latest_rollback_seq = None;
    } else {
        metadata.active_segment = retained_segments
            .last()
            .map(|segment| segment.file.clone())
            .unwrap_or_else(|| segment_file_name(1));
        metadata.next_global_seq = retained_segment_lines
            .iter()
            .flat_map(|(_, lines)| lines.iter().map(|line| line.seq))
            .max()
            .map(|seq| seq.saturating_add(1))
            .unwrap_or(1);
        metadata.segments = retained_segments;
        metadata.latest_rollback_seq = latest_rollback_seq;
    }
    metadata.updated_at = current_unix_millis_text();
    if let Some(anchor_message_id) = metadata
        .fork_source
        .as_ref()
        .and_then(|source| source.anchor_message_id.as_deref())
        && !retained_messages
            .iter()
            .any(|message| message.id == anchor_message_id)
    {
        metadata.fork_source = None;
    }
    update_segmented_message_checkpoint(&mut metadata, retained_messages)?;
    write_metadata(session_dir, &metadata)
}

/// Physically truncates a segmented session after one persisted message without adding display events.
/// 在不追加展示事件的情况下，将分段会话物理截断到某条已持久化消息之后。
#[cfg(test)]
pub(in crate::workspace_store) fn truncate_segmented_session_after_message(
    session_dir: &Path,
    message_id: &str,
    retained_messages: &[SessionMessageEntry],
) -> Result<(), String> {
    // Forking to a visible anchor should preserve that anchor and drop only the future tail.
    // 分叉到可见锚点时应保留该锚点，只丢弃其后的未来尾部。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let mut retained_segment_lines: Vec<(String, Vec<SegmentedSessionEventLine>)> = Vec::new();
    let mut found_cutoff = false;
    let mut latest_rollback_seq = None;
    let original_segments = metadata.segments.clone();
    for segment in &original_segments {
        let segment_path = session_dir.join(&segment.file);
        let lines = read_segment_event_lines(&segment_path)?;
        let mut retained_lines = Vec::new();
        if !found_cutoff {
            for line in lines {
                let is_cutoff = segmented_line_message_id(&line) == Some(message_id);
                if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                    latest_rollback_seq = Some(line.seq);
                }
                retained_lines.push(line);
                if is_cutoff {
                    found_cutoff = true;
                    break;
                }
            }
        }
        if !retained_lines.is_empty() {
            retained_segment_lines.push((segment.file.clone(), retained_lines));
        }
        if found_cutoff {
            break;
        }
    }
    if !found_cutoff {
        return Err(String::from(
            "ERR_SEGMENTED_SESSION_TRUNCATE_ANCHOR_NOT_FOUND",
        ));
    }
    let mut retained_segments = Vec::new();
    for (file, retained_lines) in &retained_segment_lines {
        let segment_path = session_dir.join(file);
        write_json_lines_file_atomic(&segment_path, retained_lines)?;
        retained_segments.push(rebuild_segment_metadata(file, retained_lines)?);
    }
    for segment in original_segments.iter().filter(|segment| {
        !retained_segment_lines
            .iter()
            .any(|(file, _)| file == &segment.file)
    }) {
        let segment_path = session_dir.join(&segment.file);
        if segment_path.exists() {
            let _ = fs::remove_file(segment_path);
        }
    }
    metadata.active_segment = retained_segments
        .last()
        .map(|segment| segment.file.clone())
        .unwrap_or_else(|| segment_file_name(1));
    metadata.next_global_seq = retained_segment_lines
        .iter()
        .flat_map(|(_, lines)| lines.iter().map(|line| line.seq))
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(1);
    metadata.segments = retained_segments;
    metadata.latest_rollback_seq = latest_rollback_seq;
    metadata.updated_at = current_unix_millis_text();
    if let Some(anchor_message_id) = metadata
        .fork_source
        .as_ref()
        .and_then(|source| source.anchor_message_id.as_deref())
        && !retained_messages
            .iter()
            .any(|message| message.id == anchor_message_id)
    {
        metadata.fork_source = None;
    }
    update_segmented_message_checkpoint(&mut metadata, retained_messages)?;
    write_metadata(session_dir, &metadata)
}

/// Validates the persisted transcript tail boundary without scanning historical segment files.
/// 不扫描历史分段文件，仅校验已持久化转录尾部边界。
pub(in crate::workspace_store) fn validate_segmented_message_tail_boundary(
    metadata: &SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    let message_count = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if message_count == 0 {
        return Ok(());
    }
    let Some(previous_message) = messages.get(message_count.saturating_sub(1)) else {
        return Ok(());
    };
    let previous_message_id =
        stable_segmented_message_id(message_count.saturating_sub(1), previous_message);
    if metadata
        .last_message_id
        .as_deref()
        .is_some_and(|stored_id| stored_id != previous_message_id)
    {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted tail message id changed from `{}` to `{previous_message_id}`.",
            metadata.last_message_id.as_deref().unwrap_or_default()
        ));
    }
    let previous_message_fingerprint = segmented_message_fingerprint(previous_message)?;
    if metadata
        .last_message_fingerprint
        .as_deref()
        .is_some_and(|stored_fingerprint| stored_fingerprint != previous_message_fingerprint)
    {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted tail message `{previous_message_id}` changed."
        ));
    }
    Ok(())
}

/// Stores the current persisted transcript tail checkpoint in metadata.
/// 将当前已持久化转录尾部检查点写入元数据。
pub(in crate::workspace_store) fn update_segmented_message_checkpoint(
    metadata: &mut SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    metadata.message_count = messages.len() as u64;
    if let Some((index, message)) = messages.iter().enumerate().next_back() {
        metadata.last_message_id = Some(stable_segmented_message_id(index, message));
        metadata.last_message_fingerprint = Some(segmented_message_fingerprint(message)?);
    } else {
        metadata.last_message_id = None;
        metadata.last_message_fingerprint = None;
    }
    Ok(())
}

/// Advances the persisted transcript checkpoint after suffix-only append.
/// 在仅追加尾部消息后推进已持久化转录检查点。
pub(in crate::workspace_store) fn advance_segmented_message_checkpoint_with_suffix(
    metadata: &mut SegmentedSessionMetadata,
    first_new_index: usize,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    metadata.message_count = first_new_index.saturating_add(messages.len()) as u64;
    if let Some((offset, message)) = messages.iter().enumerate().next_back() {
        let index = first_new_index + offset;
        metadata.last_message_id = Some(stable_segmented_message_id(index, message));
        metadata.last_message_fingerprint = Some(segmented_message_fingerprint(message)?);
    }
    Ok(())
}

/// Builds the lightweight persisted fingerprint for one transcript message.
/// 为单条转录消息构建轻量持久化指纹。
pub(in crate::workspace_store) fn segmented_message_fingerprint(
    message: &SessionMessageEntry,
) -> Result<String, String> {
    serde_json::to_string(message).map_err(to_store_error)
}

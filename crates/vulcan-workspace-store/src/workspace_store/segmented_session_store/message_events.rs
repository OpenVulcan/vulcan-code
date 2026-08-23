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
    // Load metadata first so sequence numbers and active segment remain authoritative.
    // 先加载元数据，确保序号与活动分段以元数据为准。
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
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
    // Load metadata before every batch so the store remains the only sequence authority.
    // 批量追加前先加载元数据，确保存储层始终是唯一序号来源。
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
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

/// Persists one complete transcript snapshot by appending a verified tail or rewriting divergent rows.
/// 通过追加已校验尾部或重写分歧行来持久化一份完整转录快照。
///
/// # Parameters
/// # 参数
/// `session_dir` is the existing segmented session directory that owns the transcript.
/// `session_dir` 是拥有该转录的既有分段会话目录。
/// `messages` is the complete authoritative transcript snapshot supplied by the caller.
/// `messages` 是调用方提供的完整权威转录快照。
///
/// # Returns
/// # 返回值
/// Returns the appended or changed event lines after one in-process atomic update decision.
/// 在一次进程内原子更新决策后返回新增或变更的事件行。
pub(in crate::workspace_store) fn append_segmented_session_message_update(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // append_guard keeps boundary detection and the selected append/rewrite commit indivisible in-process.
    // append_guard 让边界检测与选定的追加/重写提交在进程内不可分割。
    let _append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // metadata is the single checkpoint snapshot used by both the decision and the selected write path.
    // metadata 是决策与选定写入路径共同使用的唯一检查点快照。
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    if segmented_message_tail_matches(&metadata, messages)? {
        append_segmented_session_message_tail_locked(session_dir, messages, &mut metadata)
    } else {
        rewrite_segmented_session_message_records_locked(session_dir, messages, &mut metadata)
    }
}

/// Appends only one exact stable transcript suffix after validating its persisted prefix checkpoint.
/// 校验已持久化前缀检查点后，仅追加一个精确的稳定转录后缀。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_message_count` is the caller-observed persisted prefix length.
/// `expected_message_count` 是调用方观察到的已持久化前缀长度。
/// `previous_message` is the exact final row of that prefix when the prefix is non-empty.
/// `previous_message` 是非空前缀的精确末行。
/// `tail_messages` contains only new immutable rows beginning at `expected_message_count`.
/// `tail_messages` 仅包含从 `expected_message_count` 开始的新不可变行。
///
/// # Returns
/// 返回值
/// The exact appended event lines, or an explicit stale/corrupt-boundary error without historical scanning.
/// 返回精确追加的事件行；若边界过期或损坏，则在不扫描历史的情况下返回显式错误。
pub(in crate::workspace_store) fn append_segmented_session_message_tail_update(
    session_dir: &Path,
    expected_message_count: usize,
    previous_message: Option<&SessionMessageEntry>,
    tail_messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    let _append_guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    let persisted_message_count =
        usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if persisted_message_count != expected_message_count {
        return Err(format!(
            "Segmented transcript checkpoint advanced from {expected_message_count} to {persisted_message_count}; retry from the exact latest checkpoint."
        ));
    }
    if expected_message_count == 0 {
        if metadata.last_message_id.is_some() || metadata.last_message_fingerprint.is_some() {
            return Err(String::from(
                "Empty segmented transcript checkpoint retains a last-message identity; load-time repair is required.",
            ));
        }
    } else {
        let previous_message = previous_message.ok_or_else(|| {
            format!(
                "Segmented transcript checkpoint {expected_message_count} requires its exact previous message."
            )
        })?;
        let previous_index = expected_message_count - 1;
        let previous_message_id = stable_segmented_message_id(previous_index, previous_message);
        let previous_message_fingerprint = segmented_message_fingerprint(previous_message)?;
        if metadata.last_message_id.as_deref() != Some(previous_message_id.as_str())
            || metadata.last_message_fingerprint.as_deref()
                != Some(previous_message_fingerprint.as_str())
        {
            return Err(format!(
                "Segmented transcript checkpoint {expected_message_count} no longer matches the exact retained prefix boundary."
            ));
        }
    }
    if tail_messages.is_empty() {
        return Ok(Vec::new());
    }
    let next_message_count = expected_message_count
        .checked_add(tail_messages.len())
        .ok_or_else(|| String::from("Segmented transcript message count overflowed."))?;
    let mut new_message_ids = BTreeSet::new();
    for (offset, message) in tail_messages.iter().enumerate() {
        let message_index = expected_message_count
            .checked_add(offset)
            .ok_or_else(|| String::from("Segmented transcript message index overflowed."))?;
        let message_id = stable_segmented_message_id(message_index, message);
        if !new_message_ids.insert(message_id.clone()) {
            return Err(format!(
                "Segmented transcript tail rejected duplicate message `{message_id}` at index {message_index}."
            ));
        }
    }
    let turn_indexes = assign_segmented_tail_message_turn_indexes(&metadata, tail_messages);
    let mut events = tail_messages
        .iter()
        .enumerate()
        .zip(turn_indexes)
        .map(|((offset, message), turn_index)| {
            segmented_event_input_from_message(expected_message_count + offset, message, turn_index)
        })
        .collect::<Result<Vec<_>, _>>()?;
    backfill_tool_detail_ref_sequences(&mut events, metadata.next_global_seq);
    let appended_lines = append_events_with_metadata(session_dir, &mut metadata, &events)?;
    let final_index = next_message_count - 1;
    let final_message = tail_messages
        .last()
        .expect("non-empty tail guarantees a final message");
    metadata.message_count = next_message_count as u64;
    metadata.last_message_id = Some(stable_segmented_message_id(final_index, final_message));
    metadata.last_message_fingerprint = Some(segmented_message_fingerprint(final_message)?);
    write_metadata(session_dir, &metadata)?;
    Ok(appended_lines)
}

/// Appends the new portion of a transcript whose persisted boundary was already verified under the append lock.
/// 在追加锁内已校验持久化边界后，追加转录中的新增部分。
///
/// # Parameters
/// # 参数
/// `session_dir` is the segmented session directory currently protected by the append lock.
/// `session_dir` 是当前受追加锁保护的分段会话目录。
/// `messages` is the complete transcript whose prefix matches the persisted checkpoint.
/// `messages` 是前缀与已持久化检查点匹配的完整转录。
/// `metadata` is the same metadata snapshot used for boundary verification.
/// `metadata` 是用于边界校验的同一份元数据快照。
///
/// # Returns
/// # 返回值
/// Returns only the newly appended event lines.
/// 仅返回新追加的事件行。
fn append_segmented_session_message_tail_locked(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
    metadata: &mut SegmentedSessionMetadata,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // first_new_index is the verified persisted prefix length.
    // first_new_index 是已经校验的持久化前缀长度。
    let first_new_index = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if messages.len() == first_new_index {
        return Ok(Vec::new());
    }
    // new_message_ids rejects duplicate identities inside the newly appended slice.
    // new_message_ids 用于拒绝新增切片内部的重复身份。
    let mut new_message_ids = BTreeSet::new();
    for (index, message) in messages.iter().enumerate().skip(first_new_index) {
        let message_id = stable_segmented_message_id(index, message);
        if !new_message_ids.insert(message_id.clone()) {
            return Err(format!(
                "Segmented session append rejected duplicate tail message `{message_id}` after new message index {first_new_index}."
            ));
        }
    }
    // turn_indexes preserves turn ownership for only the new suffix.
    // turn_indexes 仅为新增尾部保留轮次归属。
    let turn_indexes =
        assign_segmented_tail_message_turn_indexes(metadata, &messages[first_new_index..]);
    // events is the exact append batch derived from the verified suffix.
    // events 是从已校验尾部派生出的精确追加批次。
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
    let appended_lines = append_events_with_metadata(session_dir, metadata, &events)?;
    update_segmented_message_checkpoint(metadata, messages)?;
    write_metadata(session_dir, metadata)?;
    Ok(appended_lines)
}

/// Rewrites persisted message rows while the caller holds the append lock and owns the verified metadata snapshot.
/// 在调用方持有追加锁并拥有已校验元数据快照时重写持久化消息行。
///
/// # Parameters
/// # 参数
/// `session_dir` is the segmented session directory currently protected by the append lock.
/// `session_dir` 是当前受追加锁保护的分段会话目录。
/// `messages` is the complete authoritative transcript replacing divergent persisted rows.
/// `messages` 是替换分歧持久化行的完整权威转录。
/// `metadata` is the metadata snapshot read before the append/rewrite decision.
/// `metadata` 是追加/重写决策前读取的元数据快照。
///
/// # Returns
/// # 返回值
/// Returns all rewritten or newly appended event lines.
/// 返回全部被重写或新追加的事件行。
fn rewrite_segmented_session_message_records_locked(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
    metadata: &mut SegmentedSessionMetadata,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
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
    let appended_lines = append_events_with_metadata(session_dir, metadata, &missing_events)?;
    changed_lines.extend(appended_lines);
    update_segmented_message_checkpoint(metadata, messages)?;
    write_metadata(session_dir, metadata)?;
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
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    // cutoffMessageIndex is exact because retainedMessages contains every row strictly before the selected prompt.
    // cutoffMessageIndex 是精确索引，因为 retainedMessages 包含所选提示词之前的全部消息行。
    let cutoff_message_index = u64::try_from(retained_messages.len()).map_err(to_store_error)?;
    if cutoff_message_index >= metadata.message_count {
        return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"));
    }
    let original_segments = metadata.segments.clone();
    // targetSegmentPosition comes from metadata populated during normal hydration; no historical segment probe is allowed here.
    // targetSegmentPosition 来自正常水合期间填充的元数据；此处禁止探测历史分段。
    let target_segment_position = original_segments
        .iter()
        .position(|segment| {
            segment
                .first_message_index
                .zip(segment.last_message_index)
                .is_some_and(|(first, last)| {
                    first <= cutoff_message_index && cutoff_message_index <= last
                })
        })
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_DISPLAY_LOCATOR_MISSING"))?;
    let target_segment = &original_segments[target_segment_position];
    let target_segment_path = session_dir.join(&target_segment.file);
    let target_lines = read_segment_event_lines(&target_segment_path)?;
    let mut retained_target_lines = Vec::new();
    let mut found_cutoff = false;
    for line in target_lines {
        if line.payload.get("message_index").and_then(Value::as_u64) == Some(cutoff_message_index) {
            if segmented_line_message_id(&line) != Some(message_id) {
                return Err(String::from(
                    "ERR_USER_PROMPT_EDIT_DISPLAY_LOCATOR_MISMATCH",
                ));
            }
            found_cutoff = true;
            break;
        }
        retained_target_lines.push(line);
    }
    if !found_cutoff {
        return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"));
    }
    let mut retained_segments = original_segments[..target_segment_position].to_vec();
    if retained_target_lines.is_empty() {
        if target_segment_path.exists() {
            fs::remove_file(&target_segment_path).map_err(to_store_error)?;
        }
    } else {
        write_json_lines_file_atomic(&target_segment_path, &retained_target_lines)?;
        retained_segments.push(rebuild_segment_metadata(
            &target_segment.file,
            &retained_target_lines,
        )?);
    }
    // Every removed tail file comes from the exact retained metadata list; directory enumeration is forbidden.
    // 每个被移除的尾部文件都来自精确保留元数据列表；禁止枚举目录。
    for segment in original_segments.iter().skip(target_segment_position + 1) {
        let segment_path = session_dir.join(&segment.file);
        if segment_path.exists() {
            fs::remove_file(segment_path).map_err(to_store_error)?;
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
        metadata.next_global_seq = retained_segments
            .last()
            .and_then(|segment| segment.last_seq)
            .map(|seq| seq.saturating_add(1))
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_DISPLAY_LOCATOR_MISSING"))?;
        metadata.segments = retained_segments;
        metadata.latest_rollback_seq = metadata
            .latest_rollback_seq
            .filter(|rollback_seq| *rollback_seq < metadata.next_global_seq);
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

/// Determines whether a complete incoming transcript preserves the persisted tail checkpoint.
/// 判断完整传入转录是否保留已持久化尾部检查点。
///
/// # Parameters
/// # 参数
/// `metadata` is the persisted transcript checkpoint read under the append lock.
/// `metadata` 是在追加锁内读取的已持久化转录检查点。
/// `messages` is the complete authoritative transcript proposed for persistence.
/// `messages` 是拟持久化的完整权威转录。
///
/// # Returns
/// # 返回值
/// Returns `true` only when count, tail identity, and tail content fingerprint all match exactly.
/// 仅当数量、尾部身份与尾部内容指纹全部精确匹配时返回 `true`。
fn segmented_message_tail_matches(
    metadata: &SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
) -> Result<bool, String> {
    // message_count is the exact persisted prefix length represented by the checkpoint.
    // message_count 是检查点表示的精确持久化前缀长度。
    let message_count = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if message_count == 0 {
        return Ok(
            metadata.last_message_id.is_none() && metadata.last_message_fingerprint.is_none()
        );
    }
    let Some(previous_message) = messages.get(message_count - 1) else {
        return Ok(false);
    };
    // previous_message_id binds the incoming boundary row to its persisted position and identity.
    // previous_message_id 将传入边界行绑定到其持久化位置与身份。
    let previous_message_id = stable_segmented_message_id(message_count - 1, previous_message);
    if metadata.last_message_id.as_deref() != Some(previous_message_id.as_str()) {
        return Ok(false);
    }
    // previous_message_fingerprint proves the boundary row content is unchanged.
    // previous_message_fingerprint 证明边界行内容未发生变化。
    let previous_message_fingerprint = segmented_message_fingerprint(previous_message)?;
    Ok(metadata.last_message_fingerprint.as_deref() == Some(previous_message_fingerprint.as_str()))
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

/// Builds the lightweight persisted fingerprint for one transcript message.
/// 为单条转录消息构建轻量持久化指纹。
pub(in crate::workspace_store) fn segmented_message_fingerprint(
    message: &SessionMessageEntry,
) -> Result<String, String> {
    serde_json::to_string(message).map_err(to_store_error)
}

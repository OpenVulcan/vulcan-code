//! Message event append and mapping helpers.
//! 消息事件追加与映射辅助。

use super::*;
use crate::workspace_store::PersistedSessionTranscriptCheckpoint;

pub(in crate::workspace_store) fn append_segmented_session_events(
    session_dir: &Path,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Hold one process-local append lock across metadata and segment updates.
    // 在元数据与分段更新期间持有同一个进程内追加锁。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.lock().map_err(to_store_error)?;
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
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.lock().map_err(to_store_error)?;
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

/// Persists one complete transcript snapshot only by appending a verified immutable tail.
/// 仅通过追加已校验的不可变尾部来持久化一份完整转录快照。
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
/// Returns appended event lines, or rejects any divergent, shortened, or rewritten history.
/// 返回新增事件行；任何分歧、缩短或改写历史都会被拒绝。
pub(in crate::workspace_store) fn append_segmented_session_message_update(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // append_guard keeps boundary detection and the append commit indivisible in-process.
    // append_guard 让边界检测与追加提交在进程内不可分割。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    // metadata is the single checkpoint snapshot used by both validation and append.
    // metadata 是校验与追加共同使用的唯一检查点快照。
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    if segmented_message_tail_matches(&metadata, messages)? {
        append_segmented_session_message_tail_locked(session_dir, messages, &mut metadata)
    } else {
        Err(format!(
            "Segmented transcript update rejected a non-append display mutation: persisted_message_count={} supplied_message_count={}. Ordinary persistence cannot rewrite or shorten display history; use an explicit truncate or maintenance repair operation.",
            metadata.message_count,
            messages.len(),
        ))
    }
}

/// Replaces every persisted display-message row during an explicit maintenance repair.
/// 在显式维护修复期间替换全部持久化展示消息行。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact deterministic session directory selected for repair.
/// `session_dir` 是为修复选定的精确确定性会话目录。
/// `messages` is the complete verified display history that must replace damaged rows.
/// `messages` 是必须替换受损记录的完整已验证展示历史。
///
/// # Returns
/// 返回值
/// Returns the rewritten event lines after the exclusive repair commit succeeds.
/// 在独占修复提交成功后返回重写的事件行。
pub(in crate::workspace_store) fn replace_segmented_session_message_records_for_repair(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // session_storage_lock separates explicit destructive maintenance from every ordinary append writer.
    // session_storage_lock 将显式破坏性维护与所有普通追加写入器隔离。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _repair_guard = session_storage_lock.lock().map_err(to_store_error)?;
    // metadata is validated before the repair consumes the caller's complete replacement history.
    // metadata 在修复使用调用方提供的完整替换历史前完成校验。
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    rewrite_segmented_session_message_records_locked(session_dir, messages, &mut metadata)
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
    let (expected_last_message_id, expected_last_message_fingerprint) = if expected_message_count
        == 0
    {
        (None, None)
    } else {
        let previous_message = previous_message.ok_or_else(|| {
                format!(
                    "Segmented transcript checkpoint {expected_message_count} requires its exact previous message."
                )
            })?;
        let previous_index = expected_message_count - 1;
        (
            Some(stable_segmented_message_id(
                previous_index,
                previous_message,
            )),
            Some(segmented_message_fingerprint(previous_message)?),
        )
    };
    append_segmented_session_message_tail_from_checkpoint(
        session_dir,
        expected_message_count,
        expected_last_message_id.as_deref(),
        expected_last_message_fingerprint.as_deref(),
        tail_messages,
    )
}

/// Appends one immutable suffix after revalidating a metadata checkpoint captured from the same session.
/// 重新校验从同一会话捕获的元数据检查点后追加一段不可变后缀。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_message_count` is the captured durable prefix length.
/// `expected_message_count` 是已捕获的可靠前缀长度。
/// `expected_last_message_id` is the captured final-message identity.
/// `expected_last_message_id` 是已捕获的末消息身份。
/// `expected_last_message_fingerprint` is the captured final-message content fingerprint.
/// `expected_last_message_fingerprint` 是已捕获的末消息内容指纹。
/// `tail_messages` contains only the immutable suffix to append.
/// `tail_messages` 仅包含需要追加的不可变后缀。
///
/// # Returns
/// 返回值
/// The exact appended event lines, or a stale/corrupt checkpoint error without historical scanning.
/// 返回精确追加的事件行；若检查点过期或损坏，则在不扫描历史的情况下返回错误。
pub(in crate::workspace_store) fn append_segmented_session_message_tail_from_checkpoint(
    session_dir: &Path,
    expected_message_count: usize,
    expected_last_message_id: Option<&str>,
    expected_last_message_fingerprint: Option<&str>,
    tail_messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    let mut metadata = read_session_metadata(session_dir)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata)?;
    append_segmented_session_message_tail_from_metadata_locked(
        session_dir,
        &mut metadata,
        expected_message_count,
        expected_last_message_id,
        expected_last_message_fingerprint,
        tail_messages,
        None,
    )
}

/// Opens one resident exact-session transcript cursor after a single metadata read.
/// 在单次读取元数据后打开一个常驻的精确会话转录游标。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact deterministic session directory.
/// `session_dir` 是精确的确定性会话目录。
/// `session_id` is the stable owner identity that must match metadata.
/// `session_id` 是必须与元数据匹配的稳定所属身份。
///
/// # Returns
/// 返回值
/// An opaque resident cursor and its exact initial durable transcript checkpoint.
/// 返回不透明常驻游标及其精确初始可靠转录检查点。
pub(in crate::workspace_store) fn open_segmented_session_transcript_append_cursor(
    session_dir: &Path,
    session_id: Uuid,
) -> Result<
    (
        SessionTranscriptAppendCursor,
        PersistedSessionTranscriptCheckpoint,
    ),
    String,
> {
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    let metadata_cursor = acquire_session_metadata_cursor_with_owned_lock(session_dir)?;
    let mut metadata_guard = metadata_cursor.lock().map_err(to_store_error)?;
    if metadata_guard.session_id != session_id.to_string() {
        return Err(format!(
            "Segmented transcript cursor owner mismatch: expected={session_id} actual={}.",
            metadata_guard.session_id
        ));
    }
    recover_resident_transcript_active_tail_locked(session_dir, &mut metadata_guard)?;
    validate_segmented_session_append_boundary_locked(session_dir, &metadata_guard)?;
    let checkpoint = PersistedSessionTranscriptCheckpoint {
        message_count: usize::try_from(metadata_guard.message_count).map_err(to_store_error)?,
        last_message_id: metadata_guard.last_message_id.clone(),
        last_message_fingerprint: metadata_guard.last_message_fingerprint.clone(),
    };
    drop(metadata_guard);
    Ok((
        SessionTranscriptAppendCursor {
            session_directory: session_dir.to_path_buf(),
            session_id,
            metadata: metadata_cursor,
            active_file: None,
        },
        checkpoint,
    ))
}

/// Recovers only complete cursor-owned rows beyond the active-segment metadata boundary.
/// 仅恢复活动分段元数据边界之后由游标拥有的完整记录。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory already protected by its root write lock.
/// `session_dir` 是已经由根会话写锁保护的精确会话目录。
/// `metadata` is the resident authoritative frontier advanced only after strict tail validation.
/// `metadata` 是仅在严格尾部校验后推进的常驻权威前沿。
///
/// # Returns
/// 返回值
/// `true` when complete rows were adopted, `false` for an unchanged or partial-only tail,
/// or a strict error that preserves any unrecognized complete row for explicit maintenance.
/// 接纳完整记录时返回 `true`；边界未变或仅有半行尾部时返回 `false`；
/// 遇到无法识别的完整记录时返回严格错误，并为显式维护保留该记录。
fn recover_resident_transcript_active_tail_locked(
    session_dir: &Path,
    metadata: &mut SegmentedSessionMetadata,
) -> Result<bool, String> {
    // active_segment comes only from the final authoritative metadata row.
    // active_segment 仅来自最后一条权威元数据记录。
    let active_segment = metadata.segments.last().ok_or_else(|| {
        String::from("Transcript cursor metadata has no active segment for bounded recovery.")
    })?;
    if metadata.active_segment != active_segment.file {
        return Err(format!(
            "Transcript cursor metadata points to `{}` but its final segment is `{}`.",
            metadata.active_segment, active_segment.file
        ));
    }
    // segment_path is resolved directly without directory enumeration.
    // segment_path 直接解析，不进行目录枚举。
    let segment_path = session_dir.join(&active_segment.file);
    // file_metadata rejects links and obtains the exact physical tail boundary.
    // file_metadata 拒绝链接并取得精确物理尾部边界。
    let file_metadata = fs::symlink_metadata(&segment_path).map_err(to_store_error)?;
    if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
        return Err(format!(
            "Transcript cursor active segment is not a regular file: {}",
            segment_path.display()
        ));
    }
    let expected_byte_len = active_segment.byte_len;
    let actual_byte_len = file_metadata.len();
    if actual_byte_len == expected_byte_len {
        return Ok(false);
    }
    if actual_byte_len < expected_byte_len {
        return Err(format!(
            "Transcript cursor active segment `{}` is shorter than metadata: actual={actual_byte_len} expected={expected_byte_len}.",
            segment_path.display()
        ));
    }
    // file is the sole exact active-tail handle used for read, optional truncation, and sync.
    // file 是用于读取、可选截断和同步精确活动尾部的唯一句柄。
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&segment_path)
        .map_err(to_store_error)?;
    file.seek(SeekFrom::Start(expected_byte_len))
        .map_err(to_store_error)?;
    // tail_bytes contains only bytes after the metadata watermark, never historical prefix bytes.
    // tail_bytes 仅包含元数据水位之后的字节，绝不包含历史前缀字节。
    let mut tail_bytes = Vec::new();
    file.read_to_end(&mut tail_bytes).map_err(to_store_error)?;
    // complete_byte_len excludes only the final unterminated physical row.
    // complete_byte_len 仅排除最后一条未终止的物理记录。
    let complete_byte_len = tail_bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index.saturating_add(1))
        .unwrap_or_default();
    // candidate isolates metadata mutation until every complete row has been validated.
    // candidate 隔离元数据修改，直到每条完整记录均完成校验。
    let mut candidate = metadata.clone();
    // next_message_index is the only accepted absolute transcript coordinate.
    // next_message_index 是唯一允许的绝对转录坐标。
    let mut next_message_index =
        usize::try_from(candidate.message_count).map_err(to_store_error)?;
    // recovered_line_count counts only complete current-schema transcript rows.
    // recovered_line_count 仅统计完整且符合当前 schema 的转录记录。
    let mut recovered_line_count = 0_usize;
    for raw_line_with_newline in
        tail_bytes[..complete_byte_len].split_inclusive(|byte| *byte == b'\n')
    {
        // raw_line_without_lf removes the required physical record terminator.
        // raw_line_without_lf 移除必需的物理记录终止符。
        let raw_line_without_lf = raw_line_with_newline
            .strip_suffix(b"\n")
            .unwrap_or(raw_line_with_newline);
        // raw_line removes an optional CR from a CRLF record before typed decoding.
        // raw_line 在强类型解码前移除 CRLF 记录中可选的 CR。
        let raw_line = raw_line_without_lf
            .strip_suffix(b"\r")
            .unwrap_or(raw_line_without_lf);
        if jsonl_line_is_blank(raw_line) {
            return Err(format!(
                "Transcript cursor recovery found a blank complete row in `{}`.",
                segment_path.display()
            ));
        }
        // line is decoded exactly once into the current canonical event schema.
        // line 仅按当前规范事件 schema 强类型解码一次。
        let line =
            serde_json::from_slice::<SegmentedSessionEventLine>(raw_line).map_err(|error| {
                format!(
                    "Transcript cursor recovery found an invalid complete row in `{}`: {error}",
                    segment_path.display()
                )
            })?;
        if line.seq != candidate.next_global_seq {
            return Err(format!(
                "Transcript cursor recovery sequence mismatch in `{}`: actual={} expected={}.",
                segment_path.display(),
                line.seq,
                candidate.next_global_seq
            ));
        }
        // message_index proves the recovered row is exactly the next immutable transcript row.
        // message_index 证明恢复记录正是下一条不可变转录记录。
        let message_index = line
            .payload
            .get("message_index")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                format!(
                    "Transcript cursor recovery row {} has no valid message index.",
                    line.seq
                )
            })?;
        if message_index != next_message_index {
            return Err(format!(
                "Transcript cursor recovery message index mismatch: actual={message_index} expected={next_message_index}."
            ));
        }
        // message is required to rebuild the exact checkpoint identity without a history scan.
        // message 用于在不扫描历史的情况下重建精确检查点身份。
        let message = segmented_line_to_session_message(&line)?.ok_or_else(|| {
            format!(
                "Transcript cursor recovery row {} is not a transcript message.",
                line.seq
            )
        })?;
        // segment receives the same incremental metadata projection used by ordinary append.
        // segment 接收与普通追加相同的增量元数据投影。
        let segment = candidate
            .segments
            .last_mut()
            .expect("candidate retains the validated active segment");
        let line_byte_len = u64::try_from(raw_line_with_newline.len()).map_err(to_store_error)?;
        update_segment_metadata(
            segment,
            &line,
            line_byte_len,
            segmented_line_turn_index(&line),
        );
        candidate.next_global_seq = candidate
            .next_global_seq
            .checked_add(1)
            .ok_or_else(|| String::from("Recovered transcript sequence overflowed."))?;
        candidate.message_count = candidate
            .message_count
            .checked_add(1)
            .ok_or_else(|| String::from("Recovered transcript message count overflowed."))?;
        candidate.updated_at = line.timestamp.clone();
        candidate.last_message_id = Some(stable_segmented_message_id(message_index, &message));
        candidate.last_message_fingerprint = Some(segmented_message_fingerprint(&message)?);
        if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
            candidate.latest_rollback_seq = Some(line.seq);
        }
        next_message_index = next_message_index.saturating_add(1);
        recovered_line_count = recovered_line_count.saturating_add(1);
    }
    // recovered_byte_len retains complete rows and discards only one unterminated final row.
    // recovered_byte_len 保留完整记录，并且只丢弃最后一条未终止记录。
    let recovered_byte_len = expected_byte_len
        .checked_add(u64::try_from(complete_byte_len).map_err(to_store_error)?)
        .ok_or_else(|| String::from("Recovered transcript byte boundary overflowed."))?;
    if actual_byte_len != recovered_byte_len {
        file.set_len(recovered_byte_len).map_err(to_store_error)?;
    }
    file.sync_data().map_err(to_store_error)?;
    if recovered_line_count == 0 {
        return Ok(false);
    }
    candidate.storage_generation = candidate.next_global_seq;
    candidate.integrity_version = 1;
    write_metadata(session_dir, &candidate)?;
    *metadata = candidate;
    eprintln!(
        "[workspace-store][transcript-tail-recovery] source=active_tail recovered_lines={recovered_line_count} discarded_partial_bytes={} new_byte_len={recovered_byte_len}",
        tail_bytes.len().saturating_sub(complete_byte_len)
    );
    Ok(true)
}

/// Appends one immutable transcript suffix through an already initialized resident cursor.
/// 通过已经初始化的常驻游标追加一个不可变转录后缀。
///
/// # Parameters
/// 参数
/// `cursor` owns the exact resident metadata frontier.
/// `cursor` 持有精确的常驻元数据前沿。
/// `expected_checkpoint` is the caller's immediate durable predecessor.
/// `expected_checkpoint` 是调用方的直接可靠前驱。
/// `tail_messages` is the immutable canonical suffix.
/// `tail_messages` 是不可变规范后缀。
///
/// # Returns
/// 返回值
/// The advanced durable checkpoint after data and metadata publication complete.
/// 在数据与元数据发布完成后返回推进后的可靠检查点。
pub(in crate::workspace_store) fn append_segmented_session_message_tail_with_cursor(
    cursor: &mut SessionTranscriptAppendCursor,
    expected_checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    append_segmented_session_message_tail_with_cursor_mode(
        cursor,
        expected_checkpoint,
        tail_messages,
        false,
    )
}

/// Retries one immutable transcript suffix after reconciling only the exact active physical tail.
/// 仅在协调精确活动物理尾部后重试一个不可变转录后缀。
///
/// # Parameters
/// 参数
/// `cursor` owns the resident metadata frontier and exact active file handle.
/// `cursor` 持有常驻元数据前沿与精确活动文件句柄。
/// `expected_checkpoint` and `tail_messages` are the originally retained immutable command.
/// `expected_checkpoint` 与 `tail_messages` 是最初保留的不可变命令。
///
/// # Returns
/// 返回值
/// The durable checkpoint after bounded tail recovery and idempotent append completion.
/// 在有界尾部恢复与幂等追加完成后返回可靠检查点。
pub(in crate::workspace_store) fn retry_segmented_session_message_tail_with_cursor(
    cursor: &mut SessionTranscriptAppendCursor,
    expected_checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    append_segmented_session_message_tail_with_cursor_mode(
        cursor,
        expected_checkpoint,
        tail_messages,
        true,
    )
}

/// Appends or retries one resident suffix with an explicit bounded-recovery mode.
/// 使用显式有界恢复模式追加或重试一个常驻后缀。
///
/// # Parameters
/// 参数
/// The cursor, checkpoint, and suffix identify the exact immutable writer command.
/// 游标、检查点与后缀标识精确的不可变写入器命令。
/// `recover_active_tail` allows only retry callers to inspect bytes beyond active metadata.
/// `recover_active_tail` 仅允许重试调用方检查活动元数据之后的字节。
///
/// # Returns
/// 返回值
/// The exact durable checkpoint after one locked append decision.
/// 在一次加锁追加决策后返回精确可靠检查点。
fn append_segmented_session_message_tail_with_cursor_mode(
    cursor: &mut SessionTranscriptAppendCursor,
    expected_checkpoint: &PersistedSessionTranscriptCheckpoint,
    tail_messages: &[SessionMessageEntry],
    recover_active_tail: bool,
) -> Result<PersistedSessionTranscriptCheckpoint, String> {
    let SessionTranscriptAppendCursor {
        session_directory,
        session_id,
        metadata: metadata_cursor,
        active_file,
    } = cursor;
    let session_storage_lock = segmented_append_file_io_lock(session_directory);
    let _append_guard = session_storage_lock.lock().map_err(to_store_error)?;
    let mut metadata = metadata_cursor.lock().map_err(to_store_error)?;
    if metadata.session_id != session_id.to_string() {
        return Err(format!(
            "Resident transcript cursor owner changed: expected={} actual={}.",
            session_id, metadata.session_id
        ));
    }
    if recover_active_tail {
        recover_resident_transcript_active_tail_locked(session_directory, &mut metadata)?;
    }
    validate_segmented_session_append_boundary_locked(session_directory, &metadata)?;
    append_segmented_session_message_tail_from_metadata_locked(
        session_directory,
        &mut metadata,
        expected_checkpoint.message_count,
        expected_checkpoint.last_message_id.as_deref(),
        expected_checkpoint.last_message_fingerprint.as_deref(),
        tail_messages,
        Some(active_file),
    )?;
    Ok(PersistedSessionTranscriptCheckpoint {
        message_count: usize::try_from(metadata.message_count).map_err(to_store_error)?,
        last_message_id: metadata.last_message_id.clone(),
        last_message_fingerprint: metadata.last_message_fingerprint.clone(),
    })
}

/// Appends a checkpointed immutable suffix using one caller-owned metadata frontier and root lock.
/// 使用调用方持有的元数据前沿与根会话锁追加一个带检查点的不可变后缀。
///
/// # Parameters
/// 参数
/// `session_dir` and `metadata` identify the exact locked session frontier.
/// `session_dir` 与 `metadata` 标识精确且已经加锁的会话前沿。
/// The remaining parameters describe the expected prefix and immutable suffix.
/// 其余参数描述预期前缀与不可变后缀。
///
/// # Returns
/// 返回值
/// The exact appended event lines, or an explicit stale/corrupt checkpoint error.
/// 返回精确追加的事件行，或明确的过期/损坏检查点错误。
fn append_segmented_session_message_tail_from_metadata_locked(
    session_dir: &Path,
    metadata: &mut SegmentedSessionMetadata,
    expected_message_count: usize,
    expected_last_message_id: Option<&str>,
    expected_last_message_fingerprint: Option<&str>,
    tail_messages: &[SessionMessageEntry],
    append_file: Option<&mut Option<(PathBuf, File)>>,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    let persisted_message_count =
        usize::try_from(metadata.message_count).map_err(to_store_error)?;
    // next_message_count is the only complete committed boundary accepted for idempotent retry.
    // next_message_count 是幂等重试唯一接受的完整已提交边界。
    let next_message_count = expected_message_count
        .checked_add(tail_messages.len())
        .ok_or_else(|| String::from("Segmented transcript message count overflowed."))?;
    // new_message_ids and tail_checkpoints validate the entire immutable command before storage changes.
    // new_message_ids 与 tail_checkpoints 在修改存储前校验完整不可变命令。
    let mut new_message_ids = BTreeSet::new();
    let mut tail_checkpoints = Vec::with_capacity(tail_messages.len());
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
        tail_checkpoints.push((message_id, segmented_message_fingerprint(message)?));
    }
    if !tail_checkpoints.is_empty() && persisted_message_count == next_message_count {
        let final_checkpoint = &tail_checkpoints[tail_checkpoints.len() - 1];
        if metadata.last_message_id.as_ref() != Some(&final_checkpoint.0)
            || metadata.last_message_fingerprint.as_ref() != Some(&final_checkpoint.1)
        {
            return Err(format!(
                "Segmented transcript completed retry boundary {next_message_count} does not match the retained immutable command."
            ));
        }
        // Republish metadata because the original data flush may have succeeded before metadata publication failed.
        // 重新发布元数据，因为首次数据刷新可能成功，而元数据发布随后失败。
        write_metadata(session_dir, metadata)?;
        return Ok(Vec::new());
    }
    if persisted_message_count != expected_message_count {
        return Err(format!(
            "Segmented transcript checkpoint advanced from {expected_message_count} to {persisted_message_count}; retry from the exact latest checkpoint."
        ));
    }
    if expected_message_count == 0 {
        if expected_last_message_id.is_some()
            || expected_last_message_fingerprint.is_some()
            || metadata.last_message_id.is_some()
            || metadata.last_message_fingerprint.is_some()
        {
            return Err(String::from(
                "Empty segmented transcript checkpoint retains a last-message identity; explicit maintenance repair is required.",
            ));
        }
    } else {
        let expected_last_message_id = expected_last_message_id.ok_or_else(|| {
            format!(
                "Segmented transcript checkpoint {expected_message_count} requires its last-message identity."
            )
        })?;
        let expected_last_message_fingerprint = expected_last_message_fingerprint.ok_or_else(|| {
            format!(
                "Segmented transcript checkpoint {expected_message_count} requires its last-message fingerprint."
            )
        })?;
        if metadata.last_message_id.as_deref() != Some(expected_last_message_id)
            || metadata.last_message_fingerprint.as_deref()
                != Some(expected_last_message_fingerprint)
        {
            return Err(format!(
                "Segmented transcript checkpoint {expected_message_count} no longer matches the exact metadata boundary."
            ));
        }
    }
    if tail_messages.is_empty() {
        return Ok(Vec::new());
    }
    let turn_indexes = assign_segmented_tail_message_turn_indexes(metadata, tail_messages);
    let mut events = tail_messages
        .iter()
        .enumerate()
        .zip(turn_indexes)
        .map(|((offset, message), turn_index)| {
            segmented_event_input_from_message(expected_message_count + offset, message, turn_index)
        })
        .collect::<Result<Vec<_>, _>>()?;
    backfill_tool_detail_ref_sequences(&mut events, metadata.next_global_seq);
    let appended_lines = match append_file {
        Some(append_file) => {
            let mut appended_lines = Vec::with_capacity(events.len());
            for (offset, (event, (message_id, message_fingerprint))) in
                events.iter().zip(tail_checkpoints.iter()).enumerate()
            {
                let line =
                    append_event_with_metadata_cursor(session_dir, metadata, event, append_file)?;
                // Advance the resident transcript checkpoint immediately after this line is durable.
                // 当前行可靠落盘后立即推进常驻转录检查点。
                metadata.message_count =
                    u64::try_from(expected_message_count + offset + 1).map_err(to_store_error)?;
                metadata.last_message_id = Some(message_id.clone());
                metadata.last_message_fingerprint = Some(message_fingerprint.clone());
                appended_lines.push(line);
            }
            appended_lines
        }
        None => append_events_with_metadata(session_dir, metadata, &events)?,
    };
    metadata.message_count = next_message_count as u64;
    metadata.last_message_id = Some(tail_checkpoints[tail_checkpoints.len() - 1].0.clone());
    metadata.last_message_fingerprint =
        Some(tail_checkpoints[tail_checkpoints.len() - 1].1.clone());
    write_metadata(session_dir, metadata)?;
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
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.lock().map_err(to_store_error)?;
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
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.lock().map_err(to_store_error)?;
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
/// Returns `true` for an exact boundary or for the same tool card advanced only through its projection lifecycle.
/// 当边界完全一致，或同一工具卡仅推进其投影生命周期时返回 `true`。
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
    if metadata.last_message_fingerprint.as_deref() == Some(previous_message_fingerprint.as_str()) {
        return Ok(true);
    }
    let Some(persisted_fingerprint) = metadata.last_message_fingerprint.as_deref() else {
        return Ok(false);
    };
    // persistedMessage is restored directly from the serialized checkpoint already validated under the append lock.
    // persistedMessage 直接从已在追加锁内校验过的序列化检查点恢复。
    let Ok(persisted_message) = serde_json::from_str::<SessionMessageEntry>(persisted_fingerprint)
    else {
        return Ok(false);
    };
    Ok(segmented_tool_call_lifecycle_boundary_matches(
        &persisted_message,
        previous_message,
    ))
}

/// Checks whether one supplied tool-call boundary only advances mutable display lifecycle metadata.
/// 检查传入的工具调用边界是否仅推进可变展示生命周期元数据。
///
/// # Parameters
/// # 参数
/// `persisted` is the exact immutable physical tool card at the append boundary.
/// `persisted` 是追加边界上的精确不可变物理工具卡。
/// `supplied` is the current in-memory projection for the same stable card.
/// `supplied` 是同一稳定工具卡的当前内存投影。
///
/// # Returns
/// # 返回值
/// Returns `true` only when immutable fields and every persisted metadata entry remain unchanged.
/// 仅当不可变字段以及每个既有元数据项均保持不变时返回 `true`。
fn segmented_tool_call_lifecycle_boundary_matches(
    persisted: &SessionMessageEntry,
    supplied: &SessionMessageEntry,
) -> bool {
    if persisted.id != supplied.id
        || persisted.kind != supplied.kind
        || persisted.record_kind != SessionMessageRecordKind::ToolCallOutput
        || supplied.record_kind != SessionMessageRecordKind::ToolCallOutput
        || persisted.visible != supplied.visible
        || persisted.title != supplied.title
        || persisted.body != supplied.body
        || persisted.created_at != supplied.created_at
    {
        return false;
    }
    let (Some(persisted_metadata), Some(supplied_metadata)) = (
        persisted.metadata.as_ref().and_then(Value::as_object),
        supplied.metadata.as_ref().and_then(Value::as_object),
    ) else {
        return false;
    };
    let persisted_tool_call_id = persisted_metadata
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if persisted_tool_call_id.is_none()
        || supplied_metadata
            .get("tool_call_id")
            .and_then(Value::as_str)
            .map(str::trim)
            != persisted_tool_call_id
    {
        return false;
    }
    let persisted_state = persisted_metadata
        .get("tool_display_state")
        .and_then(Value::as_str);
    let supplied_state = supplied_metadata
        .get("tool_display_state")
        .and_then(Value::as_str);
    if !matches!(
        (persisted_state, supplied_state),
        (
            Some("ready"),
            Some("ready" | "running" | "completed" | "failed")
        ) | (Some("running"), Some("running" | "completed" | "failed"))
    ) {
        return false;
    }
    persisted_metadata.iter().all(|(key, value)| {
        key == "tool_display_state" || supplied_metadata.get(key) == Some(value)
    })
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

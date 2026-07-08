//! Segmented session reading, replay, and display maintenance.
//! 分段会话读取、重放与显示维护。

use super::*;

/// Reads a backward transcript window ending before the optional sequence cursor.
/// 读取一个结束于可选序号游标之前的反向转录窗口。
pub(in crate::workspace_store) fn read_segmented_session_event_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let metadata = read_session_metadata(session_dir)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    let mut window = Vec::with_capacity(limit);
    for segment in metadata.segments.iter().rev() {
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines.into_iter().rev() {
            if line.seq >= cursor {
                continue;
            }
            window.push(line);
            if window.len() == limit {
                window.reverse();
                return Ok(window);
            }
        }
    }
    window.reverse();
    Ok(window)
}

/// Reads a backward transcript window by complete display turns instead of raw event count.
/// 按完整展示轮次而不是原始事件数量读取反向转录窗口。
pub(in crate::workspace_store) fn read_segmented_session_turn_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    turn_limit: usize,
) -> Result<SegmentedSessionTurnWindow, String> {
    if turn_limit == 0 {
        return Ok(SegmentedSessionTurnWindow {
            lines: Vec::new(),
            has_more_history: false,
        });
    }
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let metadata = read_session_metadata(session_dir)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    // Rollback replacement lines define the durable transcript boundary for display reads.
    // 回滚替换行定义展示读取时持久转录的有效边界。
    let rollback_floor_seq = metadata.latest_rollback_seq;
    let mut window = Vec::new();
    let mut seen_turn_indexes = BTreeSet::new();
    for segment in metadata.segments.iter().rev() {
        if segment.start_seq >= cursor {
            continue;
        }
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines.into_iter().rev() {
            if line.seq >= cursor {
                continue;
            }
            if rollback_floor_seq.is_some_and(|seq| line.seq <= seq) {
                continue;
            }
            let Some(turn_index) = segmented_line_turn_index(&line) else {
                continue;
            };
            if seen_turn_indexes.len() >= turn_limit && !seen_turn_indexes.contains(&turn_index) {
                window.reverse();
                return Ok(SegmentedSessionTurnWindow {
                    lines: window,
                    has_more_history: true,
                });
            }
            seen_turn_indexes.insert(turn_index);
            window.push(line);
        }
    }
    window.reverse();
    Ok(SegmentedSessionTurnWindow {
        lines: window,
        has_more_history: false,
    })
}

/// Projects segmented JSONL events back into displayable transcript messages.
/// 将分段 JSONL 事件投影回可展示的转录消息。
pub(in crate::workspace_store) fn segmented_lines_to_session_messages(
    lines: &[SegmentedSessionEventLine],
) -> Result<Vec<SessionMessageEntry>, String> {
    let mut messages = Vec::new();
    for line in lines {
        if let Some(message) = segmented_line_to_session_message(line)? {
            messages.push(message);
        }
    }
    Ok(messages)
}

/// Rebuilds the current conversation state from segmented compaction history and suffix events.
/// 根据分段压缩历史与后缀事件重建当前会话状态。
pub(in crate::workspace_store) fn replay_segmented_session_context(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionReplayResult>, String> {
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    replay_segmented_session_context_without_repair(session_dir)
}

/// Rebuilds conversation state after the caller has already repaired segmented JSONL files.
/// 在调用方已经修复分段 JSONL 文件后重建会话状态。
fn replay_segmented_session_context_without_repair(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionReplayResult>, String> {
    let metadata = read_session_metadata(session_dir)?;
    let scan = find_latest_compaction_anchor(session_dir, &metadata.segments)?;
    let latest_rollback_seq = scan.rollback_markers.iter().map(|marker| marker.seq).max();
    let mut messages = match scan.anchor.as_ref() {
        Some(anchor) if compaction_anchor_is_replay_baseline(anchor, latest_rollback_seq) => {
            replacement_history_to_session_messages(&anchor.replacement_history)?
        }
        _ => Vec::new(),
    };
    let suffix = suffix_after_latest_rollback(&scan);
    messages.extend(segmented_lines_to_session_messages(&suffix)?);
    if messages.is_empty() {
        return Ok(None);
    }
    let conversation_state = conversation_state_from_session_messages(&messages);
    let updated_at = suffix
        .last()
        .map(|line| line.timestamp.clone())
        .or_else(|| {
            scan.anchor
                .as_ref()
                .map(|anchor| anchor.anchor_seq.to_string())
        });
    let last_compaction_id = scan
        .anchor
        .as_ref()
        .filter(|anchor| anchor.anchor_kind == SegmentedSessionEventKind::Compacted)
        .map(|anchor| format!("segmented-compaction-{}", anchor.anchor_seq));
    let context_start_message_id = messages
        .first()
        .and_then(|message| (!message.id.trim().is_empty()).then(|| message.id.clone()));
    Ok(Some(SegmentedSessionReplayResult {
        conversation_state,
        last_compaction_id,
        context_start_message_id,
        updated_at,
    }))
}

/// Returns whether one replacement-history anchor remains the replay baseline after rollback markers.
/// 返回单个 replacement-history 锚点在回滚标记之后是否仍可作为重放基线。
fn compaction_anchor_is_replay_baseline(
    anchor: &SegmentedCompactionAnchor,
    latest_rollback_seq: Option<u64>,
) -> bool {
    match anchor.anchor_kind {
        SegmentedSessionEventKind::Compacted => true,
        SegmentedSessionEventKind::ThreadRolledBack => {
            latest_rollback_seq.is_none_or(|rollback_seq| rollback_seq <= anchor.anchor_seq)
        }
        _ => false,
    }
}

/// Rebuilds one bloated display transcript from its effective replacement-anchor view when needed.
/// 在需要时根据有效 replacement 锚点视图重建一个膨胀的显示转录。
pub(in crate::workspace_store) fn repair_segmented_session_display_if_needed(
    session_dir: &Path,
) -> Result<Option<SegmentedDisplayMaintenanceReport>, String> {
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let metadata = read_session_metadata(session_dir)?;
    if !segmented_display_needs_maintenance(&metadata) {
        return Ok(None);
    }
    let Some(replay) = replay_segmented_session_context_without_repair(session_dir)? else {
        return Ok(None);
    };
    let messages = conversation_state_to_session_messages(&replay.conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let retained_non_message_events =
        retained_non_message_events_after_latest_anchor(session_dir, &metadata)?;
    let original_segment_count = metadata.segments.len();
    let reason = String::from("replacement_history_rollback_bloat");
    let repaired_build = build_repaired_display_metadata_in_temp_dir(
        session_dir,
        &metadata,
        &messages,
        &retained_non_message_events,
    )?;
    let backup_dir = backup_segmented_session_before_display_maintenance(session_dir, &reason)?;
    let report = SegmentedDisplayMaintenanceReport {
        reason,
        backup_dir: backup_dir.to_string_lossy().to_string(),
        original_segment_count,
        rebuilt_segment_count: repaired_build.metadata.segments.len(),
        effective_message_count: messages.len(),
    };
    write_json_file_atomic(&backup_dir.join("maintenance_report.json"), &report)?;
    replace_display_segments_with_repaired_metadata(
        session_dir,
        &metadata,
        &repaired_build,
        &backup_dir.join("original"),
    )?;
    Ok(Some(report))
}

/// Returns whether one metadata snapshot shows display rollback bloat that should be normalized.
/// 返回一个元数据快照是否显示需要规整的显示层回滚膨胀。
pub(in crate::workspace_store) fn segmented_display_needs_maintenance(
    metadata: &SegmentedSessionMetadata,
) -> bool {
    if metadata.latest_rollback_seq.is_none() {
        return false;
    }
    let total_segment_bytes = metadata
        .segments
        .iter()
        .map(|segment| segment.byte_len)
        .sum::<u64>();
    let maintenance_threshold = metadata.segment_policy.max_bytes.saturating_mul(2);
    total_segment_bytes > maintenance_threshold
}

/// Builds repaired display segment files in an isolated temporary directory.
/// 在隔离的临时目录中构建修复后的显示层分段文件。
pub(in crate::workspace_store) fn build_repaired_display_metadata_in_temp_dir(
    session_dir: &Path,
    metadata: &SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
    retained_non_message_events: &[SegmentedSessionEventInput],
) -> Result<RepairedDisplayBuild, String> {
    let temp_dir = display_maintenance_temp_dir(session_dir);
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(to_store_error)?;
    }
    create_dir_all(&temp_dir)?;
    let mut repaired_metadata = metadata.clone();
    repaired_metadata.active_segment = segment_file_name(1);
    repaired_metadata.next_global_seq = 0;
    repaired_metadata.message_count = 0;
    repaired_metadata.last_message_id = None;
    repaired_metadata.last_message_fingerprint = None;
    repaired_metadata.latest_rollback_seq = None;
    repaired_metadata.segments = vec![SegmentMetadata::empty(segment_file_name(1), 0)];
    repaired_metadata.updated_at = current_unix_millis_text();
    let session_meta_event = SegmentedSessionEventInput {
        timestamp: repaired_metadata.updated_at.clone(),
        kind: SegmentedSessionEventKind::SessionMeta,
        payload: json!({
            "session_id": repaired_metadata.session_id.clone(),
            "project_tag": repaired_metadata.project_tag.clone(),
            "canonical_source": repaired_metadata.canonical_source.clone(),
            "maintenance": "display_rebuild"
        }),
        turn_index: None,
    };
    append_events_with_metadata(&temp_dir, &mut repaired_metadata, &[session_meta_event])?;
    let turn_indexes = assign_segmented_message_turn_indexes(messages);
    let events = messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            segmented_event_input_from_message(index, message, turn_indexes[index])
        })
        .collect::<Result<Vec<_>, _>>()?;
    append_events_with_metadata(&temp_dir, &mut repaired_metadata, &events)?;
    append_events_with_metadata(
        &temp_dir,
        &mut repaired_metadata,
        retained_non_message_events,
    )?;
    update_segmented_message_checkpoint(&mut repaired_metadata, messages)?;
    write_metadata(&temp_dir, &repaired_metadata)?;
    Ok(RepairedDisplayBuild {
        metadata: repaired_metadata,
        temp_dir,
    })
}

/// Extracts effective non-message events that should survive display transcript maintenance.
/// 提取显示转录维护后仍应保留的有效非消息事件。
pub(in crate::workspace_store) fn retained_non_message_events_after_latest_anchor(
    session_dir: &Path,
    metadata: &SegmentedSessionMetadata,
) -> Result<Vec<SegmentedSessionEventInput>, String> {
    let scan = find_latest_compaction_anchor(session_dir, &metadata.segments)?;
    let suffix = suffix_after_latest_rollback(&scan);
    Ok(suffix
        .into_iter()
        .filter(|line| line.payload.get("message").is_none())
        .filter(|line| line.kind != SegmentedSessionEventKind::SessionMeta)
        .map(|line| SegmentedSessionEventInput {
            timestamp: line.timestamp,
            kind: line.kind,
            payload: line.payload,
            turn_index: None,
        })
        .collect())
}

/// Copies the original session directory into an audit backup before display maintenance.
/// 在显示层维护前将原始会话目录复制到审计备份中。
pub(in crate::workspace_store) fn backup_segmented_session_before_display_maintenance(
    session_dir: &Path,
    reason: &str,
) -> Result<PathBuf, String> {
    let parent = session_dir
        .parent()
        .ok_or_else(|| format!("Session directory has no parent: {}", session_dir.display()))?;
    let session_name = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    let backup_root = parent
        .join(SEGMENTED_DISPLAY_MAINTENANCE_BACKUP_DIR)
        .join(session_name);
    create_dir_all(&backup_root)?;
    let backup_dir = backup_root.join(format!(
        "{}-{}-{}",
        current_unix_millis_text(),
        sanitize_file_component(reason),
        Uuid::new_v4()
    ));
    create_dir_all(&backup_dir)?;
    copy_segmented_directory_tree(session_dir, &backup_dir.join("original"))?;
    Ok(backup_dir)
}

/// Replaces physical display segment files after repaired files were built and the original was backed up.
/// 在修复文件构建完成且原始文件已备份后替换物理显示分段文件。
pub(in crate::workspace_store) fn replace_display_segments_with_repaired_metadata(
    session_dir: &Path,
    original_metadata: &SegmentedSessionMetadata,
    repaired_build: &RepairedDisplayBuild,
    original_backup_dir: &Path,
) -> Result<(), String> {
    let replace_result = (|| {
        let temp_token = Uuid::new_v4().to_string();
        let mut staged_files = Vec::new();
        for segment in &repaired_build.metadata.segments {
            let source = repaired_build.temp_dir.join(&segment.file);
            let staged = session_dir.join(format!(".{}.{}.tmp", segment.file, temp_token));
            fs::copy(&source, &staged).map_err(to_store_error)?;
            staged_files.push((staged, session_dir.join(&segment.file)));
        }
        for segment in &original_metadata.segments {
            let path = session_dir.join(&segment.file);
            if path.exists() {
                fs::remove_file(path).map_err(to_store_error)?;
            }
        }
        for (staged, final_path) in staged_files {
            fs::rename(&staged, &final_path).map_err(to_store_error)?;
        }
        write_metadata(session_dir, &repaired_build.metadata)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = replace_result {
        let restore_result = restore_display_segments_from_backup(session_dir, original_backup_dir);
        return match restore_result {
            Ok(()) => Err(format!(
                "display maintenance replacement failed and original files were restored: {error}"
            )),
            Err(restore_error) => Err(format!(
                "display maintenance replacement failed: {error}; restore also failed: {restore_error}"
            )),
        };
    }
    if repaired_build.temp_dir.exists() {
        let _ = fs::remove_dir_all(&repaired_build.temp_dir);
    }
    Ok(())
}

/// Restores root display segment files and metadata from one maintenance backup.
/// 从一个维护备份恢复根显示分段文件与元数据。
pub(in crate::workspace_store) fn restore_display_segments_from_backup(
    session_dir: &Path,
    original_backup_dir: &Path,
) -> Result<(), String> {
    let backup_metadata = read_required_json::<SegmentedSessionMetadata>(
        &original_backup_dir.join(SEGMENTED_METADATA_FILE),
    )?;
    remove_root_display_segment_files(session_dir)?;
    for segment in &backup_metadata.segments {
        fs::copy(
            original_backup_dir.join(&segment.file),
            session_dir.join(&segment.file),
        )
        .map_err(to_store_error)?;
    }
    write_metadata(session_dir, &backup_metadata)
}

/// Removes root display segment files without touching runtime, task, blob, or backup directories.
/// 删除根显示分段文件，但不触碰 runtime、task、blob 或备份目录。
pub(in crate::workspace_store) fn remove_root_display_segment_files(
    session_dir: &Path,
) -> Result<(), String> {
    for entry in fs::read_dir(session_dir).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.starts_with('s') && file_name.ends_with(".jsonl") {
            fs::remove_file(path).map_err(to_store_error)?;
        }
    }
    Ok(())
}

/// Builds a unique temporary directory path for display maintenance output.
/// 构建显示层维护输出使用的唯一临时目录路径。
pub(in crate::workspace_store) fn display_maintenance_temp_dir(session_dir: &Path) -> PathBuf {
    let session_name = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    session_dir.with_file_name(format!(
        ".{}.display_rebuild_tmp_{}",
        sanitize_file_component(session_name),
        Uuid::new_v4()
    ))
}

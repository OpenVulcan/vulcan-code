//! Runtime log persistence for segmented sessions.
//! 分段会话的运行时日志持久化。

use super::*;

pub(in crate::workspace_store) fn rebuild_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    rebuild_runtime_log_from_conversation_state_with_extra_events(
        session_dir,
        conversation_state,
        updated_at,
        Vec::new(),
    )
}

/// Rebuilds provider-facing runtime JSONL while preserving audit-only runtime events.
/// 在保留仅审计运行时事件的同时重建 Provider 可见运行时 JSONL。
fn rebuild_runtime_log_from_conversation_state_with_extra_events(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    extra_events: Vec<RuntimeLogEventLine>,
) -> Result<RuntimeLogMetadata, String> {
    let runtime_dir = runtime_dir_path(session_dir);
    if runtime_dir.exists() {
        fs::remove_dir_all(&runtime_dir).map_err(to_store_error)?;
    }
    create_dir_all(&runtime_dir)?;
    let active_segment = runtime_segment_file_name(1);
    let active_segment_path = session_dir.join(&active_segment);
    let mut lines = runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1);
    append_runtime_extra_events_with_fresh_sequences(&mut lines, extra_events);
    write_json_lines_file_atomic(&active_segment_path, &lines)?;
    let byte_len = fs::metadata(&active_segment_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut segment =
        RuntimeSegmentMetadata::empty(active_segment.clone(), 1, updated_at.to_string());
    segment.line_count = lines.len() as u64;
    segment.byte_len = byte_len;
    segment.end_seq = lines.last().map(|line| line.seq);
    segment.updated_at = lines.last().map(|line| line.created_at.clone());
    let metadata = RuntimeLogMetadata {
        schema_version: 1,
        active_segment,
        next_seq: lines
            .last()
            .map(|line| line.seq.saturating_add(1))
            .unwrap_or(1),
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
        segments: vec![segment],
    };
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    Ok(metadata)
}

/// Appends runtime JSONL tail lines when possible and rebuilds when history no longer matches.
/// 在可能时追加 runtime JSONL 尾部，并在历史不再匹配时重建。
pub(in crate::workspace_store) fn sync_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    if !runtime_metadata_path(session_dir).exists() {
        return rebuild_runtime_log_from_conversation_state(
            session_dir,
            conversation_state,
            updated_at,
        );
    }
    let desired_lines =
        runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1);
    let existing_lines = match read_runtime_log_lines(session_dir) {
        Ok(Some(lines)) => lines,
        Ok(None) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
        Err(_) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
    };
    let extra_events = runtime_extra_event_lines(&existing_lines);
    let existing_conversation_lines = runtime_conversation_event_lines(&existing_lines);
    if !runtime_log_prefix_matches(&existing_conversation_lines, &desired_lines) {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    if existing_conversation_lines.len() == desired_lines.len() {
        return read_runtime_metadata(session_dir);
    }
    if !extra_events.is_empty() {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    let tail = &desired_lines[existing_conversation_lines.len()..];
    append_runtime_log_tail(session_dir, tail)
}

/// Syncs provider-facing runtime JSONL after compaction while dropping pre-compaction audit tails.
/// 在压缩后同步 Provider 可见运行时 JSONL，并丢弃压缩前审计尾巴。
///
/// # Parameters
/// # 参数
/// `session_dir` is the segmented session directory that owns the runtime log.
/// `session_dir` 是拥有运行时日志的分段会话目录。
/// `conversation_state` is the compacted canonical provider conversation.
/// `conversation_state` 是已压缩的规范 Provider 会话。
/// `updated_at` is written onto rebuilt runtime lines.
/// `updated_at` 会写入重建后的运行时行。
/// `compaction_id` identifies the compaction boundary that must remain in the main runtime log.
/// `compaction_id` 标识必须保留在主运行时日志中的压缩边界。
///
/// # Returns
/// # 返回值
/// Returns updated runtime-log metadata after append or rebuild.
/// 返回追加或重建后的运行时日志元数据。
pub(in crate::workspace_store) fn sync_runtime_log_from_compacted_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    compaction_id: &str,
) -> Result<RuntimeLogMetadata, String> {
    if !runtime_metadata_path(session_dir).exists() {
        return rebuild_runtime_log_from_conversation_state(
            session_dir,
            conversation_state,
            updated_at,
        );
    }
    let desired_lines =
        runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1);
    let existing_lines = match read_runtime_log_lines(session_dir) {
        Ok(Some(lines)) => lines,
        Ok(None) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
        Err(_) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
    };
    let existing_extra_events = runtime_extra_event_lines(&existing_lines);
    let extra_events =
        runtime_extra_event_lines_from_compaction_boundary(&existing_lines, compaction_id);
    let existing_conversation_lines = runtime_conversation_event_lines(&existing_lines);
    if !runtime_log_prefix_matches(&existing_conversation_lines, &desired_lines) {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    if existing_conversation_lines.len() == desired_lines.len() {
        if !runtime_extra_event_lines_match(&existing_extra_events, &extra_events) {
            return rebuild_runtime_log_from_conversation_state_with_extra_events(
                session_dir,
                conversation_state,
                updated_at,
                extra_events,
            );
        }
        return read_runtime_metadata(session_dir);
    }
    if !extra_events.is_empty() {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    let tail = &desired_lines[existing_conversation_lines.len()..];
    append_runtime_log_tail(session_dir, tail)
}

/// Ensures a provider-facing runtime log exists, rebuilding it from display segments when needed.
/// 确保存在 Provider 可见运行时日志，缺失时从展示分段重建。
pub(in crate::workspace_store) fn ensure_runtime_log_from_segmented_context(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    if runtime_metadata_path(session_dir).exists() {
        match read_runtime_conversation_state(session_dir) {
            Ok(Some(conversation_state)) if !conversation_state.messages.is_empty() => {
                return Ok(Some(conversation_state));
            }
            Ok(Some(conversation_state)) => {
                if replay_segmented_session_context(session_dir)?
                    .as_ref()
                    .map(|replay| replay.conversation_state.messages.is_empty())
                    .unwrap_or(true)
                {
                    return Ok(Some(conversation_state));
                }
            }
            Ok(None) => return Ok(None),
            Err(error) => {
                let Some(replay) = replay_segmented_session_context(session_dir)? else {
                    return Err(error);
                };
                let updated_at = replay
                    .updated_at
                    .clone()
                    .unwrap_or_else(current_unix_millis_text);
                rebuild_runtime_log_from_conversation_state(
                    session_dir,
                    &replay.conversation_state,
                    &updated_at,
                )?;
                return Ok(Some(replay.conversation_state));
            }
        }
    }
    let Some(replay) = replay_segmented_session_context(session_dir)? else {
        return Ok(None);
    };
    let updated_at = replay
        .updated_at
        .clone()
        .unwrap_or_else(current_unix_millis_text);
    rebuild_runtime_log_from_conversation_state(
        session_dir,
        &replay.conversation_state,
        &updated_at,
    )?;
    Ok(Some(replay.conversation_state))
}

/// Reads provider-facing runtime log lines in metadata segment order.
/// 按元数据分段顺序读取 Provider 可见运行时日志行。
pub(in crate::workspace_store) fn read_runtime_log_lines(
    session_dir: &Path,
) -> Result<Option<Vec<RuntimeLogEventLine>>, String> {
    let metadata_path = runtime_metadata_path(session_dir);
    if !metadata_path.exists() {
        return Ok(None);
    }
    let metadata = read_runtime_metadata(session_dir)?;
    let mut segments = metadata.segments.clone();
    segments.sort_by_key(|segment| segment.start_seq);
    let mut lines = Vec::new();
    let mut repair_summaries = Vec::new();
    for segment in segments {
        let segment_path = session_dir.join(&segment.file);
        let repair_read = read_runtime_segment_lines_tolerant(&segment_path)?;
        if !repair_read.corrupt_line_numbers.is_empty() {
            backup_corrupt_runtime_segment(&segment_path)?;
            write_json_lines_file_atomic(&segment_path, &repair_read.lines)?;
            repair_summaries.push(format!(
                "{} lines {}",
                segment.file,
                compact_runtime_line_number_summary(&repair_read.corrupt_line_numbers)
            ));
        }
        lines.extend(repair_read.lines);
    }
    if !repair_summaries.is_empty() {
        if lines.is_empty() {
            return Err(format!(
                "Runtime log is fully damaged after repairing segments: {}",
                repair_summaries.join(", ")
            ));
        }
        let repair_detail = format!(
            "Removed corrupt runtime JSONL rows from segments: {}",
            repair_summaries.join(", ")
        );
        lines.push(RuntimeLogEventLine::new(
            next_runtime_line_seq(&lines),
            current_unix_millis_text(),
            RuntimeLogEvent::RuntimeRepair(RuntimeRepairEvent {
                repair_id: format!(
                    "runtime-repair-{}-{}",
                    current_unix_millis_text(),
                    uuid::Uuid::new_v4()
                ),
                source: String::from("runtime_jsonl_tolerant_read"),
                detail: repair_detail,
                downgraded_tool_segments: 0,
            }),
        ));
        append_runtime_line_to_active_segment(
            session_dir,
            &metadata,
            lines.last().expect("repair line"),
        )?;
        refresh_runtime_metadata_from_lines(session_dir, &metadata, &lines)?;
        write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    }
    Ok(Some(lines))
}

/// Reads the provider-facing runtime log as a canonical conversation state.
/// 将 Provider 可见运行时日志读取为规范会话状态。
pub(in crate::workspace_store) fn read_runtime_conversation_state(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    let Some(lines) = read_runtime_log_lines(session_dir)? else {
        return Ok(None);
    };
    Ok(Some(conversation_state_from_runtime_log_lines(&lines)))
}

/// Copies active runtime-log files before context compaction mutates the conversation window.
/// 在上下文压缩修改会话窗口前复制当前运行时日志文件。
pub(in crate::workspace_store) fn record_runtime_injection_applied(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeInjectionAppliedEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(
        session_dir,
        timestamp,
        RuntimeLogEvent::InjectionApplied(event),
    )
}

/// Records one runtime-only audit event without projecting it into the visible conversation.
/// 记录单个仅运行时审计事件，不将其投影进可见会话。
pub(in crate::workspace_store) fn record_runtime_audit_event(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(session_dir, timestamp, event)
}

/// Appends one runtime event when the session already owns a runtime log.
/// 当会话已经拥有 runtime 日志时追加一条运行时事件。
pub(in crate::workspace_store) fn append_runtime_log_event_if_present(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    if !runtime_metadata_path(session_dir).exists() {
        let _ = ensure_runtime_log_from_segmented_context(session_dir)?;
    }
    if !runtime_metadata_path(session_dir).exists() {
        return Ok(false);
    }
    let metadata = read_runtime_metadata(session_dir)?;
    let line = RuntimeLogEventLine::new(metadata.next_seq, timestamp.to_string(), event);
    append_runtime_log_tail(session_dir, &[line])?;
    Ok(true)
}

/// Rebuilds the lightweight runtime index from runtime JSONL lines.
/// 从运行时 JSONL 行重建轻量运行时索引。
pub(in crate::workspace_store) fn rebuild_runtime_log_index(
    lines: &[RuntimeLogEventLine],
) -> RuntimeLogIndex {
    let mut index = RuntimeLogIndex {
        schema_version: 1,
        last_seq: lines.last().map(|line| line.seq),
        line_count: lines.len() as u64,
        latest_compaction_seq: None,
        message_offsets: Default::default(),
        tool_call_offsets: Default::default(),
        provider_checkpoint_offsets: Default::default(),
        repair_offsets: Default::default(),
        injection_offsets: Default::default(),
        system_update_offsets: Default::default(),
        injection_cancelled_offsets: Default::default(),
        injection_merged_offsets: Default::default(),
        provider_response_checkpoint_offsets: Default::default(),
        provider_lowering_repair_offsets: Default::default(),
    };
    for line in lines {
        match &line.event {
            vulcan_context::RuntimeLogEvent::UserMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::AssistantMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolCall(event) => {
                index
                    .tool_call_offsets
                    .insert(event.runtime_tool_call_id.clone(), line.seq);
                if let Some(provider_tool_call_id) = &event.provider_tool_call_id {
                    index
                        .tool_call_offsets
                        .insert(provider_tool_call_id.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::ToolResult(event) => {
                index
                    .message_offsets
                    .insert(event.tool_result_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ContextCompactionApplied(_) => {
                index.latest_compaction_seq = Some(line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderTurnCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}",
                        event.provider_id, event.model_id, event.converter_version
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::RuntimeRepair(event) => {
                index
                    .repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionApplied(event) => {
                if let Some(fingerprint) = event
                    .transient_fingerprint
                    .as_ref()
                    .or(event.persistent_fingerprint.as_ref())
                {
                    index
                        .injection_offsets
                        .insert(fingerprint.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::SystemUpdate(event) => {
                index
                    .system_update_offsets
                    .insert(event.update_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionCancelled(event) => {
                index
                    .injection_cancelled_offsets
                    .insert(event.cancellation_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionMerged(event) => {
                index
                    .injection_merged_offsets
                    .insert(event.merge_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderResponseCheckpoint(event) => {
                index.provider_response_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event) => {
                index
                    .provider_lowering_repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolBatchStarted(_)
            | vulcan_context::RuntimeLogEvent::ToolBatchCompleted(_) => {}
        }
    }
    index
}

/// Returns runtime lines that rebuild the canonical provider conversation.
/// 返回用于重建规范 Provider 会话的运行时日志行。
fn runtime_conversation_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns runtime-only audit lines that must survive conversation projection rebuilds.
/// 返回必须在会话投影重建后继续保留的仅审计运行时日志行。
fn runtime_extra_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| !runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns non-conversation audit lines at or after one compaction boundary.
/// 返回某个压缩边界及其之后的非会话审计行。
fn runtime_extra_event_lines_from_compaction_boundary(
    lines: &[RuntimeLogEventLine],
    compaction_id: &str,
) -> Vec<RuntimeLogEventLine> {
    let Some(boundary_index) = lines.iter().rposition(|line| {
        matches!(
            &line.event,
            RuntimeLogEvent::ContextCompactionApplied(event)
                if event.compaction_id == compaction_id
        )
    }) else {
        return Vec::new();
    };
    lines[boundary_index..]
        .iter()
        .filter(|line| !runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns whether two runtime extra-event slices describe the same retained audit facts.
/// 返回两组运行时额外事件切片是否描述相同的保留审计事实。
fn runtime_extra_event_lines_match(
    left: &[RuntimeLogEventLine],
    right: &[RuntimeLogEventLine],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.created_at == right.created_at
                && serde_json::to_value(&left.event).ok() == serde_json::to_value(&right.event).ok()
        })
}

/// Reports whether one runtime event contributes to canonical conversation replay.
/// 判断单条运行时事件是否参与规范会话回放。
fn runtime_event_projects_to_conversation(event: &RuntimeLogEvent) -> bool {
    matches!(
        event,
        RuntimeLogEvent::UserMessage(_)
            | RuntimeLogEvent::AssistantMessage(_)
            | RuntimeLogEvent::ToolBatchStarted(_)
            | RuntimeLogEvent::ToolCall(_)
            | RuntimeLogEvent::ToolResult(_)
            | RuntimeLogEvent::ToolBatchCompleted(_)
    )
}

/// Appends audit events after conversation lines while assigning fresh sequence numbers.
/// 将审计事件追加到会话日志之后，并分配新的连续序号。
fn append_runtime_extra_events_with_fresh_sequences(
    lines: &mut Vec<RuntimeLogEventLine>,
    extra_events: Vec<RuntimeLogEventLine>,
) {
    let mut next_seq = next_runtime_line_seq(lines);
    for event_line in extra_events {
        lines.push(RuntimeLogEventLine::new(
            next_seq,
            event_line.created_at,
            event_line.event,
        ));
        next_seq = next_seq.saturating_add(1);
    }
}

/// Computes the next runtime sequence number after the provided lines.
/// 根据已提供日志行计算下一个运行时序号。
pub(in crate::workspace_store) fn next_runtime_line_seq(lines: &[RuntimeLogEventLine]) -> u64 {
    lines
        .iter()
        .map(|line| line.seq)
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(1)
}

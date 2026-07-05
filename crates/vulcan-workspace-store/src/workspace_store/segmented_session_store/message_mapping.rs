//! Message-to-segment mapping helpers.
//! 消息到分段事件的映射辅助。

use super::*;

pub(in crate::workspace_store) fn stable_segmented_message_id(
    index: usize,
    message: &SessionMessageEntry,
) -> String {
    let message_id = message.id.trim();
    if message_id.is_empty() {
        format!("message-{index:06}")
    } else {
        message_id.to_string()
    }
}

/// Converts one transcript message into the segmented append event input shape.
/// 将一条转录消息转换为分段追加事件输入形态。
pub(in crate::workspace_store) fn segmented_event_input_from_message(
    index: usize,
    message: &SessionMessageEntry,
    turn_index: i64,
) -> Result<SegmentedSessionEventInput, String> {
    let message_id = stable_segmented_message_id(index, message);
    let timestamp = non_empty_timestamp_text(&message.created_at);
    let kind = segmented_event_kind_from_message(message);
    let message_value = serde_json::to_value(message).map_err(to_store_error)?;
    Ok(SegmentedSessionEventInput {
        timestamp,
        kind,
        payload: json!({
            "message_id": message_id,
            "message_index": index,
            "turn_index": turn_index,
            "message": message_value,
            "message_kind": message.kind.label(),
            "record_kind": message.record_kind.as_persisted_key(),
            "visible": message.visible,
        }),
        turn_index: Some(turn_index),
    })
}

/// Backfills JSONL sequence numbers into visible tool-call detail references before append.
/// 追加前将 JSONL 序号回填到可见工具调用的详情引用中。
pub(in crate::workspace_store) fn backfill_tool_detail_ref_sequences(
    events: &mut [SegmentedSessionEventInput],
    first_event_seq: u64,
) {
    let mut detail_seq_by_index = BTreeMap::new();
    let mut detail_seq_by_id = BTreeMap::new();
    for (offset, event) in events.iter().enumerate() {
        if event.kind != SegmentedSessionEventKind::DisplayEvent {
            continue;
        }
        if event.payload.get("record_kind").and_then(Value::as_str)
            != Some(SessionMessageRecordKind::ToolDetailOutput.as_persisted_key())
        {
            continue;
        }
        let seq = first_event_seq.saturating_add(offset as u64);
        if let Some(message_index) = event.payload.get("message_index").and_then(Value::as_u64) {
            detail_seq_by_index.insert(message_index, seq);
        }
        if let Some(message_id) = event
            .payload
            .get("message_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            detail_seq_by_id.insert(message_id.to_string(), seq);
        }
    }
    if detail_seq_by_index.is_empty() && detail_seq_by_id.is_empty() {
        return;
    }
    for event in events {
        let Some(detail_ref) = event
            .payload
            .get_mut("message")
            .and_then(Value::as_object_mut)
            .and_then(|message| message.get_mut("metadata"))
            .and_then(Value::as_object_mut)
            .and_then(|metadata| metadata.get_mut("result"))
            .and_then(Value::as_object_mut)
            .and_then(|result| result.get_mut("detail_ref"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let seq = detail_ref
            .get("record_id")
            .and_then(Value::as_str)
            .and_then(|record_id| detail_seq_by_id.get(record_id).copied())
            .or_else(|| {
                detail_ref
                    .get("message_index")
                    .and_then(Value::as_u64)
                    .and_then(|message_index| detail_seq_by_index.get(&message_index).copied())
            });
        if let Some(seq) = seq {
            detail_ref.insert(
                String::from("seq"),
                Value::Number(serde_json::Number::from(seq)),
            );
        }
    }
}

/// Maps one transcript message onto the coarse segmented event kind used by reducers.
/// 将一条转录消息映射到 reducer 使用的粗粒度分段事件类型。
pub(in crate::workspace_store) fn segmented_event_kind_from_message(
    message: &SessionMessageEntry,
) -> SegmentedSessionEventKind {
    match (message.kind, message.record_kind, message.visible) {
        (SessionMessageKind::User, SessionMessageRecordKind::Transcript, true) => {
            SegmentedSessionEventKind::UserPrompt
        }
        (SessionMessageKind::Assistant, SessionMessageRecordKind::AssistantOutput, true) => {
            SegmentedSessionEventKind::AssistantOutput
        }
        _ if message.visible => SegmentedSessionEventKind::DisplayEvent,
        _ if message.contributes_to_context() => SegmentedSessionEventKind::ResponseItem,
        _ => SegmentedSessionEventKind::DisplayEvent,
    }
}

/// Converts one segmented event line into a transcript message when the line carries a message payload.
/// 当分段事件行携带消息载荷时，将其转换为转录消息。
pub(in crate::workspace_store) fn segmented_line_to_session_message(
    line: &SegmentedSessionEventLine,
) -> Result<Option<SessionMessageEntry>, String> {
    let Some(message_value) = line.payload.get("message").cloned() else {
        return Ok(None);
    };
    let mut message =
        serde_json::from_value::<SessionMessageEntry>(message_value).map_err(to_store_error)?;
    if message.id.trim().is_empty()
        && let Some(message_id) = line
            .payload
            .get("message_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        message.id = message_id.to_string();
    }
    if message.created_at.trim().is_empty() {
        message.created_at = line.timestamp.clone();
    }
    Ok(Some(message))
}

/// Returns whether one segmented event line is a tool-detail row for the requested turn.
/// 返回单条分段事件行是否是指定轮次的工具详情行。
pub(in crate::workspace_store) fn segmented_line_matches_tool_turn(
    line: &SegmentedSessionEventLine,
    turn_index: i64,
) -> Result<bool, String> {
    if line
        .payload
        .get("turn_index")
        .and_then(Value::as_i64)
        .is_none_or(|value| value != turn_index)
    {
        return Ok(false);
    }
    let Some(message) = segmented_line_to_session_message(line)? else {
        return Ok(false);
    };
    Ok(matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    ))
}

/// Extracts the display turn index carried by one segmented event line.
/// 提取单条分段事件行携带的展示轮次索引。
pub(in crate::workspace_store) fn segmented_line_turn_index(
    line: &SegmentedSessionEventLine,
) -> Option<i64> {
    line.payload.get("message")?;
    line.payload.get("turn_index").and_then(Value::as_i64)
}

/// Returns suffix lines that remain valid after the newest rollback marker.
/// 返回最新回滚标记之后仍然有效的后缀事件行。
pub(in crate::workspace_store) fn suffix_after_latest_rollback(
    scan: &SegmentedCompactionScanResult,
) -> Vec<SegmentedSessionEventLine> {
    let rollback_seq = scan.rollback_markers.iter().map(|marker| marker.seq).max();
    scan.suffix
        .iter()
        .filter(|line| rollback_seq.is_none_or(|seq| line.seq > seq))
        .cloned()
        .collect()
}

/// Returns suffix lines after one replacement anchor without replaying its duplicated replacement rows.
/// 返回 replacement 锚点后的后缀行，并跳过其重复写入的 replacement 行。
pub(in crate::workspace_store) fn suffix_after_replacement_anchor(
    anchor_line: &SegmentedSessionEventLine,
    following_lines: &[SegmentedSessionEventLine],
) -> Vec<SegmentedSessionEventLine> {
    if anchor_line.kind != SegmentedSessionEventKind::ThreadRolledBack {
        return following_lines.to_vec();
    }
    let Some(mut remaining_replacement_rows) = anchor_line
        .payload
        .get("replacement_message_count")
        .and_then(Value::as_u64)
    else {
        return following_lines.to_vec();
    };
    let mut suffix = Vec::new();
    for line in following_lines {
        if remaining_replacement_rows > 0 && line.payload.get("message").is_some() {
            remaining_replacement_rows = remaining_replacement_rows.saturating_sub(1);
            continue;
        }
        suffix.push(line.clone());
    }
    suffix
}

/// Converts a compaction replacement-history value into transcript messages.
/// 将压缩 replacement-history 值转换为转录消息。
pub(in crate::workspace_store) fn replacement_history_to_session_messages(
    value: &Value,
) -> Result<Vec<SessionMessageEntry>, String> {
    if let Ok(state) = serde_json::from_value::<SessionConversationState>(value.clone()) {
        return Ok(conversation_state_to_session_messages(&state));
    }
    if let Some(messages_value) = value.get("messages") {
        return serde_json::from_value::<Vec<SessionMessageEntry>>(messages_value.clone())
            .map_err(|error| format!("invalid replacement_history.messages: {error}"));
    }
    serde_json::from_value::<Vec<SessionMessageEntry>>(value.clone()).map_err(|error| {
        format!(
            "replacement_history must be a conversation state or session message array: {error}"
        )
    })
}

/// Assigns stable turn indexes to one ordered segmented transcript slice.
/// 为一段有序分段转录消息分配稳定轮次索引。
pub(in crate::workspace_store) fn assign_segmented_message_turn_indexes(
    messages: &[SessionMessageEntry],
) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(messages.len());
    let mut current_turn_index = 0_i64;
    let mut has_seen_user_turn = false;

    for message in messages {
        if segmented_message_starts_new_turn(message) {
            if has_seen_user_turn {
                current_turn_index = current_turn_index.saturating_add(1);
            } else {
                has_seen_user_turn = true;
            }
        }
        turn_indexes.push(current_turn_index);
    }

    turn_indexes
}

/// Assigns turn indexes only to newly appended transcript tail messages.
/// 仅为新追加的转录尾部消息分配轮次索引。
pub(in crate::workspace_store) fn assign_segmented_tail_message_turn_indexes(
    metadata: &SegmentedSessionMetadata,
    tail_messages: &[SessionMessageEntry],
) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(tail_messages.len());
    let mut current_turn_index = metadata
        .segments
        .iter()
        .rev()
        .find_map(|segment| segment.last_turn_index)
        .unwrap_or(0);
    let mut has_seen_user_turn = metadata.message_count > 0;

    for message in tail_messages {
        if segmented_message_starts_new_turn(message) {
            if has_seen_user_turn {
                current_turn_index = current_turn_index.saturating_add(1);
            } else {
                has_seen_user_turn = true;
            }
        }
        turn_indexes.push(current_turn_index);
    }

    turn_indexes
}

/// Returns whether one segmented transcript message opens a user-driven turn.
/// 返回一条分段转录消息是否开启用户驱动轮次。
pub(in crate::workspace_store) fn segmented_message_starts_new_turn(
    message: &SessionMessageEntry,
) -> bool {
    matches!(message.kind, SessionMessageKind::User)
        && message.visible
        && message.contributes_to_context()
}

/// Returns a non-empty timestamp for append-only events.
/// 为追加式事件返回非空时间戳。
pub(in crate::workspace_store) fn non_empty_timestamp_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

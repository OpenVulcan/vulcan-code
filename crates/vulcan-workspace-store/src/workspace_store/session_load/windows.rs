//! Segmented transcript window loading and boundary projection.
//! 分段转录窗口加载与会话边界投影逻辑。

use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;
use vulcan_session_boundary::boundary_events_to_vulcan_conversation_state;

/// Loads one segmented transcript window when the new session directory exists.
/// 当新会话目录存在时加载一个分段转录窗口。
pub(super) fn load_segmented_window_messages(
    controller: &ControllerRuntimeHandle,
    locator: Option<&StoredSessionLocatorRow>,
    session_id: SessionId,
    before_ordinal: Option<i64>,
    requested_limit: usize,
    source_preference: SegmentedWindowSourcePreference,
) -> Result<Option<SegmentedWindowMessages>, String> {
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    let before_seq = before_ordinal.and_then(|value| u64::try_from(value).ok());
    if matches!(
        source_preference,
        SegmentedWindowSourcePreference::SessionBoundaryFirst
    ) && let Some(mut window) =
        load_segmented_boundary_window_messages(&session_dir, before_seq, requested_limit)?
    {
        if let Some(raw_window) =
            load_segmented_raw_window_messages(&session_dir, before_seq, requested_limit)?
        {
            window.tool_detail_messages = raw_window.tool_detail_messages;
            window.has_more_history = window.has_more_history || raw_window.has_more_history;
        }
        return Ok(Some(window));
    }
    load_segmented_raw_window_messages(&session_dir, before_seq, requested_limit)
}

/// Loads one raw segmented display window from message-bearing event lines.
/// 从携带消息的分段事件行加载一个原始展示窗口。
fn load_segmented_raw_window_messages(
    session_dir: &std::path::Path,
    before_seq: Option<u64>,
    requested_limit: usize,
) -> Result<Option<SegmentedWindowMessages>, String> {
    let turn_window = read_segmented_session_turn_window(session_dir, before_seq, requested_limit)?;
    if turn_window.lines.is_empty() {
        return Ok(None);
    }
    let earliest_ordinal = turn_window
        .lines
        .first()
        .and_then(|line| i64::try_from(line.seq).ok());
    let messages = segmented_lines_to_session_messages(&turn_window.lines)?;
    if messages.is_empty() {
        return Ok(None);
    }
    let tool_detail_messages = messages
        .iter()
        .filter(|message| {
            matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(SegmentedWindowMessages {
        messages: messages.clone(),
        tool_detail_messages,
        earliest_ordinal,
        has_more_history: turn_window.has_more_history,
        full_messages: messages,
        boundary_conversation_state: None,
    }))
}

/// Loads a recent display window from session boundary events when the session has been migrated.
/// 当会话已经迁移时，从 会话边界事件加载最近展示窗口。
fn load_segmented_boundary_window_messages(
    session_dir: &std::path::Path,
    before_seq: Option<u64>,
    requested_limit: usize,
) -> Result<Option<SegmentedWindowMessages>, String> {
    let (boundary_lines, has_more_history) =
        read_segmented_session_boundary_turn_window(session_dir, before_seq, requested_limit)?;
    session_boundary_lines_to_window_messages(&boundary_lines, has_more_history)
}

/// Reads session-boundary lines by complete boundary turns instead of raw row count.
/// 按完整会话边界轮次而不是原始行数读取会话边界行。
pub(super) fn read_segmented_session_boundary_turn_window(
    session_dir: &std::path::Path,
    before_seq: Option<u64>,
    requested_limit: usize,
) -> Result<
    (
        Vec<self::segmented_session_store::SegmentedSessionEventLine>,
        bool,
    ),
    String,
> {
    let turn_limit = requested_limit.max(1);
    let raw_batch_limit = requested_limit
        .saturating_mul(SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT)
        .max(SESSION_TRANSCRIPT_MIN_ROW_LIMIT);
    let mut cursor = before_seq;
    let mut boundary_lines_reversed = Vec::new();
    let mut seen_turn_ids = std::collections::BTreeSet::new();
    let mut has_more_history = false;

    'read_batches: loop {
        let raw_lines = read_segmented_session_event_window(session_dir, cursor, raw_batch_limit)?;
        if raw_lines.is_empty() {
            break;
        }
        let first_seq = raw_lines.first().map(|line| line.seq);
        for line in raw_lines.into_iter().rev() {
            if line.kind != SegmentedSessionEventKind::SessionBoundary {
                continue;
            }
            let turn_id = line
                .payload
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if let Some(turn_id) = turn_id.as_ref() {
                if seen_turn_ids.len() >= turn_limit && !seen_turn_ids.contains(turn_id) {
                    has_more_history = true;
                    break 'read_batches;
                }
                seen_turn_ids.insert(turn_id.clone());
            }
            boundary_lines_reversed.push(line);
        }
        match first_seq {
            Some(0) | None => break,
            Some(seq) => cursor = Some(seq),
        }
    }

    boundary_lines_reversed.reverse();
    Ok((boundary_lines_reversed, has_more_history))
}

/// Projects session boundary JSONL lines into the existing segmented transcript window shape.
/// 将 会话边界 JSONL 行投影为现有分段转录窗口结构。
pub(super) fn session_boundary_lines_to_window_messages(
    lines: &[self::segmented_session_store::SegmentedSessionEventLine],
    has_more_history: bool,
) -> Result<Option<SegmentedWindowMessages>, String> {
    if lines.is_empty() {
        return Ok(None);
    }
    let events = lines
        .iter()
        .map(super::session_boundary_event_from_segment_line)
        .collect::<Result<Vec<_>, _>>()?;
    if events.is_empty() {
        return Ok(None);
    }
    let projection = boundary_events_to_vulcan_conversation_state(&events);
    let messages = conversation_state_to_session_messages(&projection.conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let tool_detail_messages = messages
        .iter()
        .filter(|message| {
            matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(SegmentedWindowMessages {
        messages: messages.clone(),
        tool_detail_messages,
        earliest_ordinal: lines.first().and_then(|line| i64::try_from(line.seq).ok()),
        has_more_history,
        full_messages: messages,
        boundary_conversation_state: Some(projection.conversation_state),
    }))
}

/// Applies session-boundary transcript records while preserving non-transcript runtime continuation state.
/// 应用 会话边界投影出的转录记录，同时保留非转录运行时 continuation 状态。
pub(super) fn apply_boundary_conversation_projection_to_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    boundary_conversation_state: SessionConversationState,
) {
    runtime_state.conversation_state.messages = boundary_conversation_state.messages;
    runtime_state.conversation_state.records = boundary_conversation_state.records;
}

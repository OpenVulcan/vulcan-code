//! Segmented transcript window loading and boundary projection.
//! 分段转录窗口加载与会话边界投影逻辑。

use super::tool_detail_hydration::augment_window_tool_details_from_runtime_logs;
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
    if let Some(mut window) =
        load_segmented_boundary_window_messages(&session_dir, before_seq, requested_limit)?
    {
        if let Some(raw_window) =
            load_segmented_raw_window_messages(&session_dir, before_seq, requested_limit)?
        {
            merge_raw_timeline_window_into_boundary_window(&mut window, raw_window);
        }
        augment_window_tool_details_from_runtime_logs(&session_dir, &mut window)?;
        return Ok(Some(window));
    }
    let mut window = load_segmented_raw_window_messages(&session_dir, before_seq, requested_limit)?;
    if let Some(window) = window.as_mut() {
        augment_window_tool_details_from_runtime_logs(&session_dir, window)?;
    }
    Ok(window)
}

/// Merges a raw segmented window into a boundary-projected window for visible timeline hydration.
/// 将 raw 分段窗口合并到 boundary 投影窗口，用于可见时间线水合。
/// Parameters: `boundary_window` is mutated in place, and `raw_window` is the raw display window read from segmented event rows.
/// 参数：`boundary_window` 会被就地更新，`raw_window` 是从分段事件行读取的 raw 展示窗口。
/// Returns nothing.
/// 返回值：无。
pub(super) fn merge_raw_timeline_window_into_boundary_window(
    boundary_window: &mut SegmentedWindowMessages,
    raw_window: SegmentedWindowMessages,
) {
    let raw_has_timeline_process_rows = raw_window
        .messages
        .iter()
        .any(session_message_is_timeline_process_row);
    let raw_has_more_history = raw_window.has_more_history;
    let raw_earliest_ordinal = raw_window.earliest_ordinal;
    let raw_message_start_index = raw_window.message_start_index;
    let boundary_has_timeline_process_rows = boundary_window
        .messages
        .iter()
        .any(session_message_is_timeline_process_row);
    boundary_window.tool_detail_messages = raw_window.tool_detail_messages;
    boundary_window.has_more_history = boundary_window.has_more_history || raw_has_more_history;

    // Boundary projection owns order whenever it already contains process rows; raw rows are only a legacy gap filler.
    // 当边界投影已包含过程行时由其负责顺序；原始行仅用于填补旧版边界记录缺口。
    if raw_has_timeline_process_rows && !boundary_has_timeline_process_rows {
        boundary_window.messages = raw_window.messages;
        boundary_window.full_messages = raw_window.full_messages;
        boundary_window.earliest_ordinal =
            raw_earliest_ordinal.or(boundary_window.earliest_ordinal);
        boundary_window.message_start_index = raw_message_start_index;
    } else if boundary_window.messages.len() == raw_window.messages.len() {
        // Equal row counts allow the canonical projection to reuse only the raw absolute coordinate, never its order.
        // 行数一致时规范投影仅复用原始绝对坐标，绝不复用其顺序。
        boundary_window.message_start_index = raw_message_start_index;
    }
}

/// Returns whether a session message is a timeline-only process row that boundary projection may omit.
/// 返回会话消息是否为 boundary 投影可能省略的时间线专用过程行。
/// Parameters: `message` is the persisted session message entry being classified.
/// 参数：`message` 是正在分类的已持久化会话消息项。
/// Return value: `true` means the row must stay visible in the main transcript timeline.
/// 返回值：`true` 表示该行必须保留在主转录时间线中可见。
fn session_message_is_timeline_process_row(message: &SessionMessageEntry) -> bool {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput
            | SessionMessageRecordKind::ToolResultOutput
            | SessionMessageRecordKind::TurnStats
            | SessionMessageRecordKind::CompactionStatus
    )
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
    let message_start_index = turn_window.lines.iter().find_map(|line| {
        line.payload
            .get("message_index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
    });
    let messages = segmented_lines_to_session_messages(&turn_window.lines)?;
    if messages.is_empty() {
        return Ok(None);
    }
    let tool_detail_messages = messages
        .iter()
        .filter(|message| message.supports_tool_detail_lookup())
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(SegmentedWindowMessages {
        messages: messages.clone(),
        tool_detail_messages,
        message_start_index,
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
    let projection = boundary_events_to_vulcan_conversation_state(&events)?;
    let messages = conversation_state_to_session_messages(&projection.conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let tool_detail_messages = messages
        .iter()
        .filter(|message| message.supports_tool_detail_lookup())
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(SegmentedWindowMessages {
        messages: messages.clone(),
        tool_detail_messages,
        message_start_index: None,
        earliest_ordinal: lines.first().and_then(|line| i64::try_from(line.seq).ok()),
        has_more_history,
        full_messages: messages,
        boundary_conversation_state: Some(projection.conversation_state),
    }))
}

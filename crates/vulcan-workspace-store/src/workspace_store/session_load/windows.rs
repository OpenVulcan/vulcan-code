//! Segmented transcript window loading and boundary projection.
//! 分段转录窗口加载与会话边界投影逻辑。

use super::tool_detail_hydration::augment_window_tool_details_from_runtime_logs;
use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;
use crate::workspace_store::segmented_session_store::restore_tool_card_terminal_projection;
use vulcan_context::SessionMessageKind;
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
    // total_started_at measures the complete segmented display-window path.
    // total_started_at 测量完整的分段展示窗口路径。
    let total_started_at = std::time::Instant::now();
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let before_seq = before_ordinal.and_then(|value| u64::try_from(value).ok());
    // scan_started_at measures the one shared Boundary and Raw prepared read.
    // scan_started_at 测量 Boundary 与 Raw 共享的唯一一次预备读取。
    let scan_started_at = std::time::Instant::now();
    // combined_window guarantees both projections observe one storage generation.
    // combined_window 保证两种投影观察同一个存储代次。
    let combined_window =
        read_segmented_session_combined_turn_window(&session_dir, before_seq, requested_limit)?;
    trace_session_load_if_slow(
        "combined_window_scan",
        session_id,
        scan_started_at,
        format!(
            "requested_turns={requested_limit} storage_generation={} boundary_lines={} raw_lines={}",
            combined_window.storage_generation,
            combined_window.boundary_lines.len(),
            combined_window.raw_lines.len()
        ),
    );
    // boundary_window and raw_window are projected from the already-decoded same-generation rows.
    // boundary_window 与 raw_window 均从已经解码的同代次行投影。
    let boundary_window = session_boundary_lines_to_window_messages(
        &combined_window.boundary_lines,
        combined_window.boundary_has_more_history,
    )?;
    let raw_window = segmented_raw_lines_to_window_messages(
        &combined_window.raw_lines,
        combined_window.raw_has_more_history,
    )?;
    let mut window = match (boundary_window, raw_window) {
        (Some(mut boundary_window), Some(raw_window)) => {
            merge_raw_timeline_window_into_boundary_window(&mut boundary_window, raw_window);
            Some(boundary_window)
        }
        (Some(boundary_window), None) => Some(boundary_window),
        (None, raw_window) => raw_window,
    };
    if let Some(window) = window.as_mut() {
        // tool_detail_started_at isolates runtime-log augmentation from transcript reads.
        // tool_detail_started_at 将运行时日志详情补全与转录读取分开测量。
        let tool_detail_started_at = std::time::Instant::now();
        augment_window_tool_details_from_runtime_logs(&session_dir, window)?;
        restore_tool_card_terminal_projection(
            &mut window.messages,
            window.tool_detail_messages.as_slice(),
        );
        trace_session_load_if_slow(
            "tool_detail_augmentation",
            session_id,
            tool_detail_started_at,
            format!("messages={}", window.tool_detail_messages.len()),
        );
    }
    trace_session_load_if_slow(
        "segmented_window_total",
        session_id,
        total_started_at,
        format!("requested_turns={requested_limit} source=combined"),
    );
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
    // raw_matches_boundary_transcript proves both projections describe the same visible transcript window.
    // raw_matches_boundary_transcript 证明两种投影描述的是同一个可见转录窗口。
    let raw_matches_boundary_transcript =
        raw_window_matches_boundary_transcript(&boundary_window.messages, &raw_window.messages);
    let raw_has_timeline_process_rows = raw_window
        .messages
        .iter()
        .any(session_message_is_timeline_process_row);
    let raw_message_start_index = raw_window.message_start_index;
    let boundary_has_timeline_process_rows = boundary_window
        .messages
        .iter()
        .any(session_message_is_timeline_process_row);

    // Boundary owns pagination coordinates, while raw hidden details are merged by stable record id.
    // Boundary 负责分页坐标，而 Raw 隐藏详情按稳定记录标识合并。
    merge_raw_tool_detail_messages(
        &mut boundary_window.tool_detail_messages,
        &raw_window.tool_detail_messages,
    );

    // Boundary projection owns order whenever it already contains process rows; raw rows are only a legacy gap filler.
    // 当边界投影已包含过程行时由其负责顺序；原始行仅用于填补旧版边界记录缺口。
    if raw_matches_boundary_transcript
        && raw_has_timeline_process_rows
        && !boundary_has_timeline_process_rows
    {
        boundary_window.messages = raw_window.messages;
        boundary_window.message_start_index = raw_message_start_index;
        return;
    }

    // Even a damaged or compacted raw window can own newer display-only process rows. Those rows
    // must remain visible without allowing its incomplete user/final anchors to replace Boundary.
    // 即使 Raw 窗口已受损或压缩，它仍可能拥有更新的仅展示过程行；这些记录必须保持可见，
    // 同时不允许其不完整的用户/最终答复锚点替换 Boundary。
    if !raw_matches_boundary_transcript {
        merge_raw_process_messages_by_created_at(
            &mut boundary_window.messages,
            &raw_window.messages,
        );
    }

    if raw_matches_boundary_transcript
        && boundary_window.messages.len() == raw_window.messages.len()
    {
        // Equal row counts allow the canonical projection to reuse only the raw absolute coordinate, never its order.
        // 行数一致时规范投影仅复用原始绝对坐标，绝不复用其顺序。
        boundary_window.message_start_index = raw_message_start_index;
    }
}

/// Merges missing raw process rows into the Boundary timeline by durable creation time.
/// 按可靠创建时间将缺失的 Raw 过程行合并进 Boundary 时间线。
///
/// # Parameters
/// 参数
/// `boundary_messages` is the canonical visible timeline mutated in place.
/// `boundary_messages` 是会被就地修改的规范可见时间线。
/// `raw_messages` contains raw rows from the same exact segmented read generation.
/// `raw_messages` 包含来自同一精确分段读取代次的 Raw 记录。
///
/// # Returns
/// 返回值
/// Returns nothing after inserting only stable-id-unique display process rows.
/// 仅插入稳定标识唯一的展示过程行后无返回值。
fn merge_raw_process_messages_by_created_at(
    boundary_messages: &mut Vec<SessionMessageEntry>,
    raw_messages: &[SessionMessageEntry],
) {
    // retained_ids prevents raw replay from duplicating any record already projected by Boundary.
    // retained_ids 防止 Raw 重放重复 Boundary 已投影的任何记录。
    let mut retained_ids = boundary_messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for raw_message in raw_messages
        .iter()
        .filter(|message| session_message_is_timeline_process_row(message))
    {
        if !retained_ids.insert(raw_message.id.clone()) {
            continue;
        }
        // insert_at keeps equal timestamps stable by placing later raw rows after existing peers.
        // insert_at 通过把后续 Raw 记录放在同时间戳既有记录之后来维持稳定顺序。
        let insert_at = raw_message
            .created_at
            .parse::<u128>()
            .ok()
            .and_then(|raw_created_at| {
                boundary_messages.iter().position(|boundary_message| {
                    boundary_message
                        .created_at
                        .parse::<u128>()
                        .ok()
                        .is_some_and(|boundary_created_at| boundary_created_at > raw_created_at)
                })
            })
            .unwrap_or(boundary_messages.len());
        boundary_messages.insert(insert_at, raw_message.clone());
    }
}

/// Merges hidden tool-detail rows by stable identity without changing Boundary pagination.
/// 按稳定身份合并隐藏工具详情行，且不改变 Boundary 分页。
///
/// # Parameters
/// 参数
/// `boundary_details` is the retained detail collection mutated in place.
/// `boundary_details` 是会被就地修改的既有详情集合。
/// `raw_details` contains exact raw detail rows from the same storage generation.
/// `raw_details` 包含来自同一存储代次的精确 Raw 详情行。
///
/// # Returns
/// 返回值
/// Returns nothing after appending only missing detail identities.
/// 仅追加缺失详情身份后无返回值。
fn merge_raw_tool_detail_messages(
    boundary_details: &mut Vec<SessionMessageEntry>,
    raw_details: &[SessionMessageEntry],
) {
    // retained_ids is the exact identity set already available for on-demand expansion.
    // retained_ids 是已经可供按需展开使用的精确身份集合。
    let mut retained_ids = boundary_details
        .iter()
        .map(|message| message.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    boundary_details.extend(
        raw_details
            .iter()
            .filter(|message| retained_ids.insert(message.id.clone()))
            .cloned(),
    );
}

/// Returns whether Boundary and Raw contain the same visible turn anchors in the same order.
/// 返回 Boundary 与 Raw 是否按相同顺序包含完全一致的可见轮次锚点。
///
/// # Parameters
/// 参数
/// `boundary_messages` contains the canonical Boundary projection for one history window.
/// `boundary_messages` 包含一个历史窗口的规范 Boundary 投影。
/// `raw_messages` contains the resident Raw projection considered only as a legacy process-row supplement.
/// `raw_messages` 包含仅作为旧版过程行补充来源的常驻 Raw 投影。
///
/// # Returns
/// 返回值
/// `true` only when both projections have equal user-prompt and final-assistant anchors.
/// 仅当两种投影具有完全一致的用户提示词与最终助手输出锚点时返回 `true`。
fn raw_window_matches_boundary_transcript(
    boundary_messages: &[SessionMessageEntry],
    raw_messages: &[SessionMessageEntry],
) -> bool {
    // boundary_transcript keeps only canonical user-prompt and final-assistant anchors.
    // boundary_transcript 仅保留规范的用户提示词与最终助手输出锚点。
    let mut boundary_transcript = boundary_messages
        .iter()
        .filter(|message| session_message_is_timeline_anchor_row(message));
    // raw_transcript excludes thinking, tools, notices, statistics, and hidden detail records.
    // raw_transcript 排除思考、工具、提示、统计与隐藏详情记录。
    let mut raw_transcript = raw_messages
        .iter()
        .filter(|message| session_message_is_timeline_anchor_row(message));
    // matched_anchor prevents two unrelated process-only windows from being treated as aligned.
    // matched_anchor 防止两个无关的纯过程窗口被视为已经对齐。
    let mut matched_anchor = false;

    loop {
        match (boundary_transcript.next(), raw_transcript.next()) {
            (Some(boundary_message), Some(raw_message)) => {
                if boundary_message.kind != raw_message.kind
                    || boundary_message.record_kind != raw_message.record_kind
                    || boundary_message.visible != raw_message.visible
                    || boundary_message.body != raw_message.body
                {
                    return false;
                }
                matched_anchor = true;
            }
            (None, None) => return matched_anchor,
            (Some(_), None) | (None, Some(_)) => return false,
        }
    }
}

/// Returns whether a session message is a visible turn anchor shared by Boundary and Raw projections.
/// 返回会话消息是否为 Boundary 与 Raw 投影共享的可见轮次锚点。
/// Parameters: `message` is the persisted session message entry being classified.
/// 参数：`message` 是正在分类的已持久化会话消息项。
/// Return value: `true` means the row is a user prompt or final assistant output.
/// 返回值：`true` 表示该行是用户提示词或最终助手输出。
fn session_message_is_timeline_anchor_row(message: &SessionMessageEntry) -> bool {
    message.visible
        && matches!(
            (message.kind, message.record_kind),
            (
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript
            ) | (
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput
            )
        )
}

/// Returns whether a session message is a timeline-only process row that boundary projection may omit.
/// 返回会话消息是否为 boundary 投影可能省略的时间线专用过程行。
/// Parameters: `message` is the persisted session message entry being classified.
/// 参数：`message` 是正在分类的已持久化会话消息项。
/// Return value: `true` means the row must stay visible in the main transcript timeline.
/// 返回值：`true` 表示该行必须保留在主转录时间线中可见。
fn session_message_is_timeline_process_row(message: &SessionMessageEntry) -> bool {
    message.visible && !session_message_is_timeline_anchor_row(message)
}

/// Loads one raw segmented display window from message-bearing event lines.
/// 从携带消息的分段事件行加载一个原始展示窗口。
fn segmented_raw_lines_to_window_messages(
    lines: &[self::segmented_session_store::SegmentedSessionEventLine],
    has_more_history: bool,
) -> Result<Option<SegmentedWindowMessages>, String> {
    if lines.is_empty() {
        return Ok(None);
    }
    let earliest_ordinal = lines.first().and_then(|line| i64::try_from(line.seq).ok());
    let message_start_index = lines.iter().find_map(|line| {
        line.payload
            .get("message_index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
    });
    let messages = segmented_lines_to_session_messages(lines)?;
    if messages.is_empty() {
        return Ok(None);
    }
    let tool_detail_messages = messages
        .iter()
        .filter(|message| message.supports_tool_detail_lookup())
        .cloned()
        .collect::<Vec<_>>();
    Ok(Some(SegmentedWindowMessages {
        messages,
        tool_detail_messages,
        message_start_index,
        earliest_ordinal,
        has_more_history,
        boundary_conversation_state: None,
    }))
}

/// Reads session-boundary lines by complete boundary turns instead of raw row count.
/// 按完整会话边界轮次而不是原始行数读取会话边界行。
#[cfg(test)]
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
    let combined =
        read_segmented_session_combined_turn_window(session_dir, before_seq, requested_limit)?;
    Ok((combined.boundary_lines, combined.boundary_has_more_history))
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
        messages,
        tool_detail_messages,
        message_start_index: None,
        earliest_ordinal: lines.first().and_then(|line| i64::try_from(line.seq).ok()),
        has_more_history,
        boundary_conversation_state: Some(projection.conversation_state),
    }))
}

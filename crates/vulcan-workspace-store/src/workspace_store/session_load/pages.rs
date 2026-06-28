//! Transcript history and tool-detail paging helpers.
//! 转录历史与工具详情分页辅助逻辑。

use super::*;
/// Lazily loads one older transcript page before the provided ordinal cursor.
/// 在给定序号游标之前懒加载一页更早的转录历史。
pub async fn load_session_history_page(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: i64,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    let requested_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_locator(&controller, session_id).await?;
    if let Some(window) = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        Some(before_ordinal),
        requested_limit,
        SegmentedWindowSourcePreference::RawTurnOnly,
    )? {
        return Ok(SessionHistoryPage {
            session_id,
            messages: window.messages,
            tool_detail_messages: window.tool_detail_messages,
            earliest_ordinal: window.earliest_ordinal,
            has_more_history: window.has_more_history,
        });
    }
    Ok(SessionHistoryPage {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        earliest_ordinal: None,
        has_more_history: false,
    })
}

/// Lazily loads the latest transcript window without hydrating runtime state.
/// 懒加载最近转录窗口，但不水合运行态。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `session_id` selects the session to read.
/// `session_id` 选择要读取的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A recent transcript page suitable for display-only WebUI hydration.
/// 返回适合 WebUI 仅展示水合使用的最近转录页。
pub async fn load_session_transcript_window(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    let requested_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_locator(&controller, session_id).await?;
    if let Some(window) = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        None,
        requested_limit,
        SegmentedWindowSourcePreference::RawTurnOnly,
    )? {
        return Ok(SessionHistoryPage {
            session_id,
            messages: window.messages,
            tool_detail_messages: window.tool_detail_messages,
            earliest_ordinal: window.earliest_ordinal,
            has_more_history: window.has_more_history,
        });
    }
    Ok(SessionHistoryPage {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        earliest_ordinal: None,
        has_more_history: false,
    })
}

/// Lazily loads one tool-detail page for a specific conversation turn.
/// 为指定对话轮次懒加载一页工具明细。
pub async fn load_session_tool_detail_page(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<SessionToolDetailPage, String> {
    let requested_limit = requested_limit.max(1).min(SESSION_TOOL_DETAIL_PAGE_LIMIT);
    let locator = query_session_locator(&controller, session_id).await?;
    if let Some(page) = load_segmented_tool_detail_page(
        &controller,
        locator.as_ref(),
        session_id,
        turn_index,
        before_ordinal,
        requested_limit,
    )? {
        return Ok(page);
    }
    Ok(SessionToolDetailPage {
        messages: Vec::new(),
        earliest_ordinal: None,
        has_more_detail: false,
    })
}

/// Loads the complete persisted tool-detail stream for one conversation turn.
/// 加载单个对话轮次的完整持久化工具明细流。
pub async fn load_complete_session_tool_detail_turn(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
) -> Result<SessionToolDetailPage, String> {
    let locator = query_session_locator(&controller, session_id).await?;
    if let Some(page) = load_segmented_complete_tool_detail_turn(
        &controller,
        locator.as_ref(),
        session_id,
        turn_index,
    )? {
        return Ok(page);
    }
    Ok(SessionToolDetailPage {
        messages: Vec::new(),
        earliest_ordinal: None,
        has_more_detail: false,
    })
}

/// Loads one segmented tool-detail page when the new session directory exists.
/// 当新会话目录存在时加载一页分段工具详情。
fn load_segmented_tool_detail_page(
    controller: &ControllerRuntimeHandle,
    locator: Option<&StoredSessionLocatorRow>,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<Option<SessionToolDetailPage>, String> {
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let before_seq = before_ordinal.and_then(|value| u64::try_from(value).ok());
    let window = read_segmented_session_tool_detail_window(
        &session_dir,
        turn_index,
        before_seq,
        requested_limit,
    )?;
    let earliest_ordinal = window
        .lines
        .first()
        .and_then(|line| i64::try_from(line.seq).ok());
    let messages = segmented_lines_to_session_messages(&window.lines)?;
    Ok(Some(SessionToolDetailPage {
        messages,
        earliest_ordinal,
        has_more_detail: window.has_more_detail,
    }))
}

/// Loads the complete segmented tool-detail stream for one turn when available.
/// 可用时加载单个轮次的完整分段工具详情流。
fn load_segmented_complete_tool_detail_turn(
    controller: &ControllerRuntimeHandle,
    locator: Option<&StoredSessionLocatorRow>,
    session_id: SessionId,
    turn_index: i64,
) -> Result<Option<SessionToolDetailPage>, String> {
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let lines = read_segmented_session_complete_tool_detail_turn(&session_dir, turn_index)?;
    let earliest_ordinal = lines.first().and_then(|line| i64::try_from(line.seq).ok());
    let messages = segmented_lines_to_session_messages(&lines)?;
    Ok(Some(SessionToolDetailPage {
        messages,
        earliest_ordinal,
        has_more_detail: false,
    }))
}

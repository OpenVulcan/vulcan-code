//! Transcript history and tool-detail paging helpers.
//! 转录历史与工具详情分页辅助逻辑。

use super::*;
/// Lazily loads one older transcript page using deterministic project and creation metadata.
/// 使用确定性的项目与创建时间元数据懒加载一页更早转录历史。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known segmented project tag.
/// `project_id` 是已知的分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to read.
/// `session_id` 选择要读取的会话。
/// `before_ordinal` is the exclusive upper ordinal cursor.
/// `before_ordinal` 是不包含自身的序号上界游标。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A transcript history page for the target session.
/// 返回目标会话的一页转录历史。
pub async fn load_session_history_page_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    before_ordinal: i64,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    let requested_limit = requested_limit.clamp(1, SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    if let Some(window) = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        Some(before_ordinal),
        requested_limit,
    )? {
        return Ok(SessionHistoryPage {
            session_id,
            messages: window.messages,
            tool_detail_messages: window.tool_detail_messages,
            message_start_index: window.message_start_index,
            earliest_ordinal: window.earliest_ordinal,
            has_more_history: window.has_more_history,
        });
    }
    Ok(SessionHistoryPage {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        message_start_index: None,
        earliest_ordinal: None,
        has_more_history: false,
    })
}

/// Lazily loads the latest transcript window using deterministic project and creation metadata.
/// 使用确定性的项目与创建时间元数据懒加载最近转录窗口。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known segmented project tag.
/// `project_id` 是已知的分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to read.
/// `session_id` 选择要读取的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A recent transcript page suitable for display-only WebUI hydration.
/// 返回适合 WebUI 仅展示水合使用的最近转录页。
pub async fn load_session_transcript_window_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    let requested_limit = requested_limit.clamp(1, SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    if let Some(window) = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        None,
        requested_limit,
    )? {
        return Ok(SessionHistoryPage {
            session_id,
            messages: window.messages,
            tool_detail_messages: window.tool_detail_messages,
            message_start_index: window.message_start_index,
            earliest_ordinal: window.earliest_ordinal,
            has_more_history: window.has_more_history,
        });
    }
    Ok(SessionHistoryPage {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        message_start_index: None,
        earliest_ordinal: None,
        has_more_history: false,
    })
}

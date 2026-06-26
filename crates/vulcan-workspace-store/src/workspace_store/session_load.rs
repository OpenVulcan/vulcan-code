//! Session-loading helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的会话加载辅助逻辑。

use super::segmented_session_store::{
    SegmentedSessionReplayResult, ensure_runtime_log_from_segmented_context,
    find_segmented_session_directory, find_segmented_session_directory_by_id,
    read_segmented_session_complete_tool_detail_turn, read_segmented_session_event_window,
    read_segmented_session_tool_detail_window, read_segmented_session_turn_window, read_state,
    replay_segmented_session_context, segmented_lines_to_session_messages,
};
use super::session_addon_store::read_session_addon_state;
use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;
use std::collections::BTreeSet;
use std::path::PathBuf;
use vulcan_session_boundary::boundary_events_to_vulcan_conversation_state;

/// Lightweight persisted session locator row used to resolve segmented session directories.
/// 用于解析分段会话目录的轻量持久化会话定位行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionLocatorRow {
    /// Absolute segmented session directory.
    /// 绝对分段会话目录。
    session_dir: PathBuf,
    /// Latest persisted canonical turn index mirrored from the event store.
    /// 从事件存储镜像回数据库的最近规范轮次索引。
    latest_turn_index: i64,
}

/// Segmented transcript window ready for hydration or history paging.
/// 可直接用于水合或历史分页的分段转录窗口。
struct SegmentedWindowMessages {
    /// Visible transcript messages projected from segmented JSONL lines.
    /// 从分段 JSONL 行投影出的可见转录消息。
    messages: Vec<SessionMessageEntry>,
    /// Tool-detail messages split out for the existing WebUI side channel.
    /// 为现有 WebUI 带外通道拆出的工具明细消息。
    tool_detail_messages: Vec<SessionMessageEntry>,
    /// Earliest event sequence included in this window.
    /// 当前窗口包含的最早事件序号。
    earliest_ordinal: Option<i64>,
    /// Whether older segmented events exist before this window.
    /// 当前窗口之前是否还存在更早的分段事件。
    has_more_history: bool,
    /// Full projected message window used by runtime-state hydration.
    /// 供运行态水合使用的完整投影消息窗口。
    full_messages: Vec<SessionMessageEntry>,
    /// Canonical session-boundary-derived conversation state when the window came from session boundary events.
    /// 当窗口来自 会话边界事件时对应的规范 会话边界投影会话状态。
    boundary_conversation_state: Option<SessionConversationState>,
}

/// Selects which segmented source should feed the visible transcript window.
/// 选择用于构建可见转录窗口的分段数据来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentedWindowSourcePreference {
    /// Prefer compact session-boundary projection when it exists.
    /// 当存在会话边界投影时优先使用紧凑投影。
    SessionBoundaryFirst,
    /// Read the raw turn window so tool, thinking, and stats rows stay available.
    /// 读取原始轮次窗口，确保工具、思考与统计行仍可展示。
    RawTurnOnly,
}

/// Returns the display-preserving source preference for ordinary session hydration.
/// 返回普通会话水合使用的保留展示行来源策略。
fn default_session_hydration_window_source_preference() -> SegmentedWindowSourcePreference {
    SegmentedWindowSourcePreference::RawTurnOnly
}

/// Lazily loads the recent transcript window and lightweight runtime state for one session.
/// 为单个会话懒加载最近转录窗口与轻量运行时状态。
pub async fn load_session_hydration_snapshot(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator(&controller, session_id).await?;
    load_session_hydration_snapshot_from_locator(
        controller,
        session_id,
        requested_limit,
        locator,
        default_session_hydration_window_source_preference(),
    )
    .await
}

/// Lazily loads a display-complete transcript window for one session.
/// 为单个会话懒加载展示完整的转录窗口。
pub async fn load_session_display_hydration_snapshot(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator(&controller, session_id).await?;
    load_session_hydration_snapshot_from_locator(
        controller,
        session_id,
        requested_limit,
        locator,
        default_session_hydration_window_source_preference(),
    )
    .await
}

/// Lazily loads one session by constraining segmented lookup to its owning project.
/// 按所属项目约束分段目录查找，并懒加载单个会话的最近转录窗口与轻量运行态。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A hydration snapshot for the target session, or an empty snapshot if no segmented directory exists.
/// 返回目标会话的水合快照；若分段目录不存在则返回空快照。
pub async fn load_session_hydration_snapshot_for_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project(&controller, project_id, session_id).await?;
    load_session_hydration_snapshot_from_locator(
        controller,
        session_id,
        requested_limit,
        locator,
        default_session_hydration_window_source_preference(),
    )
    .await
}

/// Lazily loads a display-complete transcript window for one project-scoped session.
/// 为单个项目内会话懒加载展示完整的转录窗口。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A display-complete hydration snapshot for the target session.
/// 返回目标会话的展示完整水合快照。
pub async fn load_session_display_hydration_snapshot_for_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project(&controller, project_id, session_id).await?;
    load_session_hydration_snapshot_from_locator(
        controller,
        session_id,
        requested_limit,
        locator,
        SegmentedWindowSourcePreference::RawTurnOnly,
    )
    .await
}

/// Builds a session hydration snapshot from an already resolved segmented locator.
/// 根据已经解析出的分段定位信息构建会话水合快照。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
/// `locator` points at the segmented session directory when one was found.
/// `locator` 在找到分段会话目录时指向该目录。
///
/// # Returns
/// 返回值
/// A hydration snapshot assembled from the locator and segmented state.
/// 返回基于定位信息与分段状态组装出的水合快照。
async fn load_session_hydration_snapshot_from_locator(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
    locator: Option<StoredSessionLocatorRow>,
    source_preference: SegmentedWindowSourcePreference,
) -> Result<SessionHydrationSnapshot, String> {
    let requested_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let compactions = Vec::new();
    let runtime_state_row = None;
    let target_turn_count = resolve_initial_hydration_turn_count(
        requested_limit,
        &compactions,
        runtime_state_row.as_ref(),
        locator.as_ref(),
    );
    let segmented_window = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        None,
        target_turn_count,
        source_preference,
    )?;
    let (
        has_more_history,
        mut messages,
        mut tool_detail_messages,
        earliest_ordinal,
        full_messages,
        boundary_conversation_state,
    ) = segmented_window
        .map(|window| {
            (
                window.has_more_history,
                window.messages,
                window.tool_detail_messages,
                window.earliest_ordinal,
                window.full_messages,
                window.boundary_conversation_state,
            )
        })
        .unwrap_or_else(|| (false, Vec::new(), Vec::new(), None, Vec::new(), None));
    let runtime_state =
        match load_segmented_runtime_state(&controller, locator.as_ref(), session_id)? {
            Some(runtime_state) => Some(runtime_state),
            None => {
                hydrate_session_runtime_state(session_id, runtime_state_row, &full_messages).await?
            }
        }
        .map(|mut runtime_state| {
            if let Some(boundary_conversation_state) = boundary_conversation_state {
                apply_boundary_conversation_projection_to_runtime_state(
                    &mut runtime_state,
                    boundary_conversation_state,
                );
            }
            runtime_state
        });
    restore_hydration_window_from_runtime_state_if_empty(
        &mut messages,
        &mut tool_detail_messages,
        runtime_state.as_ref(),
    );

    Ok(SessionHydrationSnapshot {
        session_id,
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        earliest_ordinal,
        has_more_history,
    })
}

/// Repairs duplicate canonical record ids inside one loaded conversation state so older broken sessions can recover during hydration.
/// 修复单份已加载会话状态中的重复规范记录标识，确保旧的损坏会话能在水合阶段自动恢复。
pub(super) fn repair_duplicate_conversation_record_ids(
    session_id: SessionId,
    conversation_state: &mut SessionConversationState,
) -> bool {
    let mut seen_ids = BTreeSet::new();
    let mut repaired = false;

    for (record_index, record) in conversation_state.records.iter_mut().enumerate() {
        if seen_ids.insert(record.id.clone()) {
            continue;
        }

        let created_at = non_empty_timestamp_text(&record.created_at);
        let mut candidate_id = format!("session-{session_id}-message-{created_at}-{record_index}");
        let mut collision_suffix = 1usize;
        while !seen_ids.insert(candidate_id.clone()) {
            candidate_id = format!(
                "session-{session_id}-message-{created_at}-{record_index}-{collision_suffix}"
            );
            collision_suffix = collision_suffix.saturating_add(1);
        }
        record.id = candidate_id;
        repaired = true;
    }

    repaired
}

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

/// Finds the lightweight session locator needed to open one segmented session directory.
/// 查找打开单个分段会话目录所需的轻量会话定位信息。
async fn query_session_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredSessionLocatorRow>, String> {
    Ok(
        find_segmented_session_directory_by_id(&controller.data_root, &session_id.to_string())?
            .map(|session_dir| StoredSessionLocatorRow {
                session_dir,
                latest_turn_index: -1,
            }),
    )
}

/// Resolves one segmented session locator by checking only the known owning project.
/// 仅检查已知所属项目来解析单个分段会话定位信息。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known segmented project tag for this session.
/// `project_id` 是该会话已知的分段项目标签。
/// `session_id` selects the session directory suffix.
/// `session_id` 选择会话目录后缀。
///
/// # Returns
/// 返回值
/// The locator when a matching active or archived segmented session directory exists.
/// 当活动或归档分段会话目录存在匹配项时返回定位信息。
async fn query_session_locator_for_project(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<StoredSessionLocatorRow>, String> {
    let session_id_text = session_id.to_string();
    let active_dir = find_segmented_session_directory(
        &controller.data_root,
        false,
        project_id,
        &session_id_text,
    )?;
    let session_dir = match active_dir {
        Some(session_dir) => Some(session_dir),
        None => find_segmented_session_directory(
            &controller.data_root,
            true,
            project_id,
            &session_id_text,
        )?,
    };
    Ok(session_dir.map(|session_dir| StoredSessionLocatorRow {
        session_dir,
        latest_turn_index: -1,
    }))
}

/// Loads one segmented transcript window when the new session directory exists.
/// 当新会话目录存在时加载一个分段转录窗口。
fn load_segmented_window_messages(
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
    let before_seq = before_ordinal.and_then(|value| u64::try_from(value).ok());
    if matches!(
        source_preference,
        SegmentedWindowSourcePreference::SessionBoundaryFirst
    ) {
        if let Some(window) =
            load_segmented_boundary_window_messages(&session_dir, before_seq, requested_limit)?
        {
            return Ok(Some(window));
        }
    }
    let turn_window =
        read_segmented_session_turn_window(&session_dir, before_seq, requested_limit)?;
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
    let raw_limit = requested_limit
        .saturating_mul(SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT)
        .max(SESSION_TRANSCRIPT_MIN_ROW_LIMIT);
    let raw_lines = read_segmented_session_event_window(session_dir, before_seq, raw_limit)?;
    let has_more_history = raw_lines.len() >= raw_limit
        && raw_lines
            .first()
            .is_some_and(|line| line.seq > 0 || before_seq.is_some());
    let boundary_lines = raw_lines
        .into_iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .collect::<Vec<_>>();
    session_boundary_lines_to_window_messages(&boundary_lines, has_more_history)
}

/// Projects session boundary JSONL lines into the existing segmented transcript window shape.
/// 将 会话边界 JSONL 行投影为现有分段转录窗口结构。
fn session_boundary_lines_to_window_messages(
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
fn apply_boundary_conversation_projection_to_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    boundary_conversation_state: SessionConversationState,
) {
    runtime_state.conversation_state.messages = boundary_conversation_state.messages;
    runtime_state.conversation_state.records = boundary_conversation_state.records;
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

/// Loads the overwrite-only runtime state from the segmented session state file when available.
/// 可用时从分段会话状态文件加载覆盖式运行态。
fn load_segmented_runtime_state(
    controller: &ControllerRuntimeHandle,
    locator: Option<&StoredSessionLocatorRow>,
    session_id: SessionId,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(locator) = locator else {
        return Ok(None);
    };
    let Some(session_dir) =
        find_segmented_session_dir_for_locator(controller, locator, session_id)?
    else {
        return Ok(None);
    };
    let runtime_conversation_state = ensure_runtime_log_from_segmented_context(&session_dir)?;
    let Some(state_document) = read_state(&session_dir)? else {
        let mut runtime_state = runtime_state_from_runtime_or_segmented_replay(
            session_id,
            runtime_conversation_state,
            &session_dir,
        )?;
        let _ =
            apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
        return Ok(runtime_state);
    };
    let has_runtime_state = !state_document.runtime_state.is_null();
    if !has_runtime_state && state_document.current_context.is_null() {
        let mut runtime_state = runtime_state_from_runtime_or_segmented_replay(
            session_id,
            runtime_conversation_state,
            &session_dir,
        )?;
        let _ =
            apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
        return Ok(runtime_state);
    }
    let mut runtime_state = if has_runtime_state {
        serde_json::from_value::<SessionRuntimeStateEntry>(state_document.runtime_state)
            .map_err(|error| format!("解析分段会话状态文件失败：{error}"))?
    } else {
        SessionRuntimeStateEntry::default()
    };
    let applied_context =
        apply_segmented_current_context(&mut runtime_state, &state_document.current_context)?;
    if runtime_state.updated_at.trim().is_empty() && !state_document.updated_at.trim().is_empty() {
        runtime_state.updated_at = state_document.updated_at;
    }
    if let Some(conversation_state) = runtime_conversation_state {
        runtime_state.conversation_state = conversation_state;
    }
    let mut runtime_state = Some(runtime_state);
    let addon_applied =
        apply_session_addon_state_to_loaded_runtime_state(&session_dir, &mut runtime_state)?;
    let Some(mut runtime_state) = runtime_state else {
        return Ok(None);
    };
    if !has_runtime_state
        && !applied_context
        && !addon_applied
        && runtime_state.conversation_state.messages.is_empty()
        && runtime_state.conversation_state.records.is_empty()
    {
        return Ok(None);
    }
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);
    let _ = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);
    Ok(Some(runtime_state))
}

/// Applies addon state to an optional loaded runtime state.
/// 将附属状态应用到一个可选的已加载运行态。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
/// `runtime_state` is the optional runtime state that receives addon-owned fields.
/// `runtime_state` 是接收附属状态字段的可选运行态。
///
/// # Returns
/// 返回值
/// `true` when an addon document existed and was applied.
/// 当附属状态文档存在且已应用时返回 `true`。
fn apply_session_addon_state_to_loaded_runtime_state(
    session_dir: &std::path::Path,
    runtime_state: &mut Option<SessionRuntimeStateEntry>,
) -> Result<bool, String> {
    let Some(addon_state) = read_session_addon_state(session_dir)? else {
        return Ok(false);
    };
    addon_state.apply_to_runtime_state(
        runtime_state.get_or_insert_with(SessionRuntimeStateEntry::default),
    );
    Ok(true)
}

/// Rebuilds runtime state from runtime JSONL when available, otherwise falls back to segmented replay.
/// 优先从 runtime JSONL 重建运行态，不可用时回退到分段展示重放。
fn runtime_state_from_runtime_or_segmented_replay(
    session_id: SessionId,
    runtime_conversation_state: Option<SessionConversationState>,
    session_dir: &std::path::Path,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    if let Some(conversation_state) = runtime_conversation_state {
        let mut runtime_state = SessionRuntimeStateEntry {
            conversation_state,
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        };
        let _ = repair_duplicate_conversation_record_ids(
            session_id,
            &mut runtime_state.conversation_state,
        );
        return Ok(Some(runtime_state));
    }
    runtime_state_from_segmented_replay(session_id, session_dir)
}

/// Rebuilds runtime state from segmented JSONL replay when state.json has no usable context.
/// 当 state.json 没有可用上下文时，通过分段 JSONL 重放重建运行态。
fn runtime_state_from_segmented_replay(
    session_id: SessionId,
    session_dir: &std::path::Path,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(replay) = replay_segmented_session_context(session_dir)? else {
        return Ok(None);
    };
    let mut runtime_state = runtime_state_entry_from_segmented_replay(replay);
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);
    Ok(Some(runtime_state))
}

/// Converts a segmented replay result into the existing runtime-state model.
/// 将分段重放结果转换为现有运行态模型。
fn runtime_state_entry_from_segmented_replay(
    replay: SegmentedSessionReplayResult,
) -> SessionRuntimeStateEntry {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state = replay.conversation_state;
    runtime_state.last_compaction_id = replay.last_compaction_id;
    runtime_state.context_start_message_id = replay.context_start_message_id;
    if let Some(updated_at) = replay.updated_at {
        runtime_state.updated_at = updated_at;
    }
    runtime_state
}

/// Applies a segmented current-context document onto one runtime-state entry.
/// 将分段当前上下文文档应用到一条运行态记录上。
fn apply_segmented_current_context(
    runtime_state: &mut SessionRuntimeStateEntry,
    current_context: &serde_json::Value,
) -> Result<bool, String> {
    let Some(context_object) = current_context.as_object() else {
        return Ok(false);
    };
    let mut applied = false;
    if let Some(conversation_state_value) = context_object
        .get("conversation_state")
        .filter(|value| !value.is_null())
    {
        runtime_state.conversation_state =
            serde_json::from_value::<SessionConversationState>(conversation_state_value.clone())
                .map_err(|error| format!("解析分段当前上下文会话状态失败：{error}"))?;
        applied = true;
    }
    if let Some(value) =
        segmented_current_context_optional_string(current_context, "last_compaction_id")
    {
        runtime_state.last_compaction_id = value;
        applied = true;
    }
    if let Some(value) =
        segmented_current_context_optional_string(current_context, "context_start_message_id")
    {
        runtime_state.context_start_message_id = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "live_context_budget_fingerprint",
    ) {
        runtime_state.live_context_budget_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "live_context_assembly_fingerprint",
    ) {
        runtime_state.live_context_assembly_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "anchor_context_budget_fingerprint",
    ) {
        runtime_state.anchor_context_budget_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "anchor_context_assembly_fingerprint",
    ) {
        runtime_state.anchor_context_assembly_fingerprint = value;
        applied = true;
    }
    if let Some(value) = segmented_current_context_optional_string(
        current_context,
        "last_provider_request_preview_markdown",
    ) {
        runtime_state.last_provider_request_preview_markdown = value;
        applied = true;
    }
    if let Some(updated_at) = segmented_current_context_string(current_context, "updated_at") {
        runtime_state.updated_at = updated_at;
        applied = true;
    }
    Ok(applied)
}

/// Extracts one nullable string field from a segmented current-context value.
/// 从分段当前上下文值中提取一个可空字符串字段。
fn segmented_current_context_optional_string(
    current_context: &serde_json::Value,
    key: &str,
) -> Option<Option<String>> {
    let value = current_context.get(key)?;
    if value.is_null() {
        return Some(None);
    }
    Some(segmented_json_string(value))
}

/// Extracts one non-empty string field from a segmented current-context value.
/// 从分段当前上下文值中提取一个非空字符串字段。
fn segmented_current_context_string(
    current_context: &serde_json::Value,
    key: &str,
) -> Option<String> {
    current_context.get(key).and_then(segmented_json_string)
}

/// Converts one JSON value into a trimmed string when possible.
/// 尽可能将一个 JSON 值转换为去除首尾空白后的字符串。
fn segmented_json_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Resolves the segmented session directory for an active or archived session locator.
/// 根据活动或归档会话定位信息解析分段会话目录。
fn find_segmented_session_dir_for_locator(
    controller: &ControllerRuntimeHandle,
    locator: &StoredSessionLocatorRow,
    session_id: SessionId,
) -> Result<Option<std::path::PathBuf>, String> {
    let _ = (controller, session_id);
    Ok(Some(locator.session_dir.clone()))
}

/// Returns no controller runtime document because segmented state files own hydration state.
/// 不返回控制器运行态文档，因为分段 state 文件拥有水合状态。
async fn query_session_runtime_state_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredRuntimeStateRow>, String> {
    let _ = (controller, session_id);
    Ok(None)
}

/// Restores the hydrated runtime state for one session from the segmented checkpoint plus the canonical event stream.
/// 通过分段检查点和规范事件流为单个会话恢复水合后的运行时状态。
async fn hydrate_session_runtime_state(
    session_id: SessionId,
    runtime_state_row: Option<StoredRuntimeStateRow>,
    full_messages: &[SessionMessageEntry],
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(mut runtime_state) = runtime_state_row.map(stored_runtime_state_row_to_entry) else {
        if full_messages.is_empty() {
            return Ok(None);
        }
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state = conversation_state_from_session_messages(full_messages);
        runtime_state.updated_at = current_unix_millis_text();
        let _ = repair_duplicate_conversation_record_ids(
            session_id,
            &mut runtime_state.conversation_state,
        );
        return Ok(Some(runtime_state));
    };
    if runtime_state.updated_at.trim().is_empty() {
        runtime_state.updated_at = current_unix_millis_text();
    }
    if runtime_state.conversation_state.records.is_empty() && !full_messages.is_empty() {
        runtime_state.conversation_state = conversation_state_from_session_messages(full_messages);
    }
    let _ =
        repair_duplicate_conversation_record_ids(session_id, &mut runtime_state.conversation_state);

    Ok(Some(runtime_state))
}

/// Rebuilds the visible hydration transcript from runtime state when the persisted turn window is unexpectedly empty.
/// 当持久化 turn 窗口意外为空时，根据运行时状态回建可见的水合转录窗口。
fn restore_hydration_window_from_runtime_state_if_empty(
    messages: &mut Vec<SessionMessageEntry>,
    tool_detail_messages: &mut Vec<SessionMessageEntry>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) {
    if !messages.is_empty() {
        return;
    }
    let Some(runtime_state) = runtime_state else {
        return;
    };
    let rebuilt_messages =
        conversation_state_to_session_messages(&runtime_state.conversation_state);
    if rebuilt_messages.is_empty() {
        return;
    }
    *tool_detail_messages = rebuilt_messages
        .iter()
        .filter(|message| {
            matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .cloned()
        .collect();
    *messages = rebuilt_messages;
}

/// Resolves the startup hydration turn target by taking the larger value between the configured page size and the overlap required to expose five turns before the active compaction boundary.
/// 通过取“配置分页大小”和“为暴露活动压缩边界前五轮所需的重叠范围”中的较大值，解析启动水合轮次目标。
fn resolve_initial_hydration_turn_count(
    requested_turn_count: usize,
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
    locator: Option<&StoredSessionLocatorRow>,
) -> usize {
    let mut effective_turn_count = requested_turn_count.max(1);
    let Some(boundary_turn_index) =
        active_compaction_overlap_boundary_turn_index(compactions, runtime_state_row)
    else {
        return effective_turn_count;
    };
    let Some(latest_turn_index) = locator
        .map(|row| row.latest_turn_index)
        .filter(|value| *value >= 0)
    else {
        return effective_turn_count;
    };
    let overlap_turn_count = SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT as i64;
    let earliest_required_turn_index = boundary_turn_index
        .saturating_sub(overlap_turn_count)
        .max(0);
    let turns_needed = latest_turn_index
        .saturating_sub(earliest_required_turn_index)
        .saturating_add(1) as usize;
    effective_turn_count = effective_turn_count.max(turns_needed);
    effective_turn_count
}

/// Resolves the active compaction overlap boundary from persisted runtime-state metadata and checkpoint rows.
/// 根据持久化运行时状态元数据与检查点行解析当前活动压缩重叠边界。
fn active_compaction_overlap_boundary_turn_index(
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
) -> Option<i64> {
    let active_compaction = runtime_state_row
        .and_then(|row| row.last_compaction_id.as_deref())
        .and_then(|compaction_id| compactions.iter().find(|entry| entry.id == compaction_id))
        .or_else(|| compactions.last())?;

    active_compaction
        .retained_tail_start_turn_index
        .or_else(|| {
            active_compaction
                .source_until_turn_index
                .map(|turn_index| turn_index + 1)
        })
}

#[cfg(test)]
mod tests {
    //! Session-load tests covering transcript hydration behavior for segmented sessions.
    //! 面向分段会话的转录水合行为测试。

    use super::*;
    use crate::conversation_protocol::SessionConversationRecord;
    use serde_json::json;
    use vulcan_context::SessionMessageKind;
    use vulcan_session_boundary::{
        SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    };
    use vulcan_session_core::SessionNodeId;

    /// Builds one transcript message entry for session-load regression tests.
    /// 为会话加载回归测试构建一条转录消息项。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: id.to_string(),
            body: body.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }
    }

    /// Builds one persisted session boundary line for session-load regression tests.
    /// 为会话加载回归测试构建一条已持久化 会话边界行。
    fn session_boundary_line(
        seq: u64,
        thread_id: SessionNodeId,
        kind: SessionBoundaryEventKind,
        visibility: SessionEventVisibility,
        text: &str,
    ) -> super::super::segmented_session_store::SegmentedSessionEventLine {
        let display_payload = if matches!(kind, SessionBoundaryEventKind::UserInput) {
            json!({ "prompt": text })
        } else {
            json!({ "text": text })
        };
        super::super::segmented_session_store::SegmentedSessionEventLine {
            seq,
            timestamp: (1000 + seq).to_string(),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: json!({
                "event_id": format!("event-{seq}"),
                "thread_id": thread_id,
                "turn_id": "turn-1",
                "item_id": serde_json::Value::Null,
                "kind": kind,
                "scope": SessionEventScope::Turn,
                "visibility": visibility,
                "sequence": seq,
                "payload": display_payload,
            }),
        }
    }

    /// Verifies session boundary events become the primary hydrated transcript source.
    /// 验证 会话边界事件会成为水合转录正文的首要来源。
    #[test]
    fn session_boundary_window_projects_visible_transcript() {
        let thread_id = SessionNodeId::new_v4();
        let window = session_boundary_lines_to_window_messages(
            &[
                session_boundary_line(
                    1,
                    thread_id,
                    SessionBoundaryEventKind::UserInput,
                    SessionEventVisibility::Visible,
                    "用户输入",
                ),
                session_boundary_line(
                    2,
                    thread_id,
                    SessionBoundaryEventKind::AgentMessage,
                    SessionEventVisibility::Visible,
                    "助手输出",
                ),
            ],
            false,
        )
        .expect("project session boundary")
        .expect("visible transcript");

        assert_eq!(window.messages.len(), 2);
        assert_eq!(window.messages[0].body, "用户输入");
        assert_eq!(window.messages[1].body, "助手输出");
        assert_eq!(window.earliest_ordinal, Some(1));
        assert!(window.boundary_conversation_state.is_some());
    }

    /// Verifies ordinary startup hydration preserves display events instead of preferring boundary projection.
    /// 验证普通启动水合会保留展示事件，而不是优先使用边界投影。
    #[test]
    fn default_session_hydration_prefers_raw_display_turns() {
        assert_eq!(
            default_session_hydration_window_source_preference(),
            SegmentedWindowSourcePreference::RawTurnOnly
        );
    }

    /// Verifies archived and restored session directories still hydrate display text from session boundary events.
    /// 验证归档与恢复后的会话目录仍然从 会话边界事件水合显示正文。
    #[test]
    fn session_boundary_archive_restore_keeps_display_projection() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-session-boundary-archive-restore-test-{}",
            uuid::Uuid::new_v4()
        ));
        let session_id = SessionId::from_u128(7005);
        let thread_id = SessionNodeId::from(session_id);
        let project_tag = "project-context";
        let request = super::super::segmented_session_store::SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: project_tag.to_string(),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(project_tag.to_string()),
            title: Some(String::from("归档恢复")),
            title_source: Some(String::from("test")),
            segment_policy: super::super::segmented_session_store::SegmentPolicy::default(),
        };
        let session_dir =
            super::super::segmented_session_store::create_segmented_session(&root, request.clone())
                .expect("create segmented session");
        let boundary_events = vec![
            SessionBoundaryEvent::new(
                "turn-1-user",
                thread_id,
                0,
                SessionBoundaryEventKind::UserInput,
                SessionEventScope::Turn,
                SessionEventVisibility::Visible,
                101,
            )
            .with_turn_id("turn-1")
            .with_payload(json!({ "prompt": "用户归档前输入" })),
            SessionBoundaryEvent::new(
                "turn-1-agent",
                thread_id,
                0,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventScope::Turn,
                SessionEventVisibility::Visible,
                102,
            )
            .with_turn_id("turn-1")
            .with_payload(json!({ "text": "助手归档后输出" })),
        ];
        super::super::segmented_session_store::append_segmented_boundary_events(
            &session_dir,
            &boundary_events,
        )
        .expect("append session boundary events");

        let archived_dir = super::super::segmented_session_store::archive_segmented_session_by_id(
            &root,
            project_tag,
            &request.session_id,
            "200",
        )
        .expect("archive session")
        .expect("archived directory");
        let archived_lines =
            read_segmented_session_event_window(&archived_dir, None, 8).expect("read archived");
        let archived_boundary_lines = archived_lines
            .into_iter()
            .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
            .collect::<Vec<_>>();
        let archived_window =
            session_boundary_lines_to_window_messages(&archived_boundary_lines, false)
                .expect("project archived boundary")
                .expect("archived projection");
        assert_eq!(archived_window.messages[0].body, "用户归档前输入");
        assert_eq!(archived_window.messages[1].body, "助手归档后输出");

        let restored_dir = super::super::segmented_session_store::restore_segmented_session_by_id(
            &root,
            project_tag,
            &request.session_id,
            "300",
        )
        .expect("restore session")
        .expect("restored directory");
        let restored_lines =
            read_segmented_session_event_window(&restored_dir, None, 8).expect("read restored");
        let restored_boundary_lines = restored_lines
            .into_iter()
            .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
            .collect::<Vec<_>>();
        let restored_window =
            session_boundary_lines_to_window_messages(&restored_boundary_lines, false)
                .expect("project restored boundary")
                .expect("restored projection");
        assert_eq!(restored_window.messages[0].body, "用户归档前输入");
        assert_eq!(restored_window.messages[1].body, "助手归档后输出");

        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies session boundary projection overwrites transcript records while preserving runtime metadata.
    /// 验证 会话边界投影会覆盖转录正文，同时保留运行时元数据。
    #[test]
    fn boundary_projection_replaces_runtime_transcript_only() {
        let mut runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("provider-a")),
            ..SessionRuntimeStateEntry::default()
        };
        runtime_state.conversation_state =
            conversation_state_from_session_messages(&[test_message(
                "old-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "旧正文",
            )]);
        let boundary_state = conversation_state_from_session_messages(&[test_message(
            "session-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "会话边界正文",
        )]);

        apply_boundary_conversation_projection_to_runtime_state(&mut runtime_state, boundary_state);

        assert_eq!(
            runtime_state.active_provider_id.as_deref(),
            Some("provider-a")
        );
        assert_eq!(runtime_state.conversation_state.records.len(), 1);
        assert_eq!(
            runtime_state.conversation_state.records[0].body,
            "会话边界正文"
        );
    }

    /// Verifies hydration can rebuild runtime state from the segmented transcript window when no overwrite state exists.
    /// 验证没有覆盖式状态时，水合能从分段转录窗口重建运行态。
    #[test]
    fn hydrate_session_runtime_state_rebuilds_from_segmented_window() {
        let session_id = SessionId::from_u128(99);
        let transcript_messages = vec![test_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "仅转录消息",
        )];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread tokio runtime");
        let hydrated = runtime
            .block_on(hydrate_session_runtime_state(
                session_id,
                None,
                &transcript_messages,
            ))
            .expect("hydrate runtime state")
            .expect("expected hydrated runtime state");

        assert_eq!(hydrated.conversation_state.messages.len(), 1);
        assert_eq!(hydrated.conversation_state.records.len(), 1);
    }

    /// Verifies hydration no longer falls back to legacy context snapshots when segmented state is absent.
    /// 验证分段状态缺失时水合不再回退旧上下文快照。
    #[test]
    fn hydrate_session_runtime_state_without_segmented_inputs_returns_empty() {
        let session_id = SessionId::from_u128(119);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread tokio runtime");
        let hydrated = runtime
            .block_on(hydrate_session_runtime_state(session_id, None, &[]))
            .expect("hydrate empty runtime state");

        assert!(hydrated.is_none());
    }

    /// Verifies startup hydration rebuilds the visible transcript from runtime context when turn-window rows are temporarily absent.
    /// 验证当 turn 窗口行暂时缺失时，启动水合会从运行时上下文回建可见转录。
    #[test]
    fn restore_hydration_window_from_runtime_state_if_empty_rebuilds_visible_messages() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state = conversation_state_from_session_messages(&[
            test_message(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "启动后要能看到我",
            ),
            test_message(
                "assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "当然可以",
            ),
        ]);

        let mut messages = Vec::new();
        let mut tool_detail_messages = Vec::new();
        restore_hydration_window_from_runtime_state_if_empty(
            &mut messages,
            &mut tool_detail_messages,
            Some(&runtime_state),
        );

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].body, "启动后要能看到我");
        assert_eq!(messages[1].body, "当然可以");
        assert!(tool_detail_messages.is_empty());
    }

    /// Verifies segmented current-context fallback can restore runtime conversation state without a full runtime-state blob.
    /// 验证分段当前上下文兜底能在缺少完整运行态 blob 时恢复运行时会话状态。
    #[test]
    fn segmented_current_context_restores_runtime_state_without_full_blob() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let conversation_state = conversation_state_from_session_messages(&[test_message(
            "assistant-context",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "分段上下文正文",
        )]);
        let current_context = serde_json::json!({
            "conversation_state": conversation_state,
            "last_compaction_id": "compaction-segment-1",
            "context_start_message_id": "context-start-1",
            "anchor_context_budget_fingerprint": "budget-fp",
            "anchor_context_assembly_fingerprint": "assembly-fp",
            "last_provider_request_preview_markdown": "# Segmented Preview",
            "updated_at": "9001"
        });

        let applied = apply_segmented_current_context(&mut runtime_state, &current_context)
            .expect("apply segmented current context");

        assert!(applied);
        assert_eq!(runtime_state.conversation_state.messages.len(), 1);
        assert_eq!(
            runtime_state.last_compaction_id.as_deref(),
            Some("compaction-segment-1")
        );
        assert_eq!(
            runtime_state.context_start_message_id.as_deref(),
            Some("context-start-1")
        );
        assert_eq!(
            runtime_state.anchor_context_budget_fingerprint.as_deref(),
            Some("budget-fp")
        );
        assert_eq!(
            runtime_state.anchor_context_assembly_fingerprint.as_deref(),
            Some("assembly-fp")
        );
        assert_eq!(
            runtime_state
                .last_provider_request_preview_markdown
                .as_deref(),
            Some("# Segmented Preview")
        );
        assert_eq!(runtime_state.updated_at, "9001");
    }

    /// Verifies segmented replay results can hydrate the existing runtime-state structure.
    /// 验证分段重放结果可以水合到现有运行态结构中。
    #[test]
    fn segmented_replay_result_maps_to_runtime_state() {
        let conversation_state = conversation_state_from_session_messages(&[test_message(
            "assistant-replay",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "重放正文",
        )]);
        let runtime_state =
            runtime_state_entry_from_segmented_replay(SegmentedSessionReplayResult {
                conversation_state,
                last_compaction_id: Some("segmented-compaction-9".to_string()),
                context_start_message_id: Some("assistant-replay".to_string()),
                updated_at: Some("9900".to_string()),
            });

        assert_eq!(runtime_state.conversation_state.messages.len(), 1);
        assert_eq!(
            runtime_state.last_compaction_id.as_deref(),
            Some("segmented-compaction-9")
        );
        assert_eq!(
            runtime_state.context_start_message_id.as_deref(),
            Some("assistant-replay")
        );
        assert_eq!(runtime_state.updated_at, "9900");
    }

    /// Verifies hydration repairs duplicate canonical record ids that were previously persisted by the broken runtime id generator.
    /// 验证水合过程会修复旧版错误运行时标识生成器持久化下来的重复规范记录标识。
    #[test]
    fn repair_duplicate_conversation_record_ids_rewrites_later_duplicates() {
        let session_id = SessionId::from_u128(0x30cf28c9);
        let mut conversation_state = SessionConversationState {
            messages: Vec::new(),
            records: vec![
                SessionConversationRecord {
                    id: String::from("session-old-message-100-1"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::SummaryOutput,
                    visible: false,
                    title: String::from("Summary"),
                    body: String::from("done"),
                    metadata: None,
                    created_at: String::from("100"),
                },
                SessionConversationRecord {
                    id: String::from("session-old-message-100-1"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::TurnStats,
                    visible: true,
                    title: String::from("Turn Stats"),
                    body: String::from("stats"),
                    metadata: None,
                    created_at: String::from("100"),
                },
            ],
            continuation: Default::default(),
        };

        let repaired =
            repair_duplicate_conversation_record_ids(session_id, &mut conversation_state);

        assert!(repaired);
        assert_eq!(
            conversation_state.records[0].id,
            "session-old-message-100-1"
        );
        assert_eq!(
            conversation_state.records[1].id,
            format!("session-{session_id}-message-100-1")
        );
    }
}

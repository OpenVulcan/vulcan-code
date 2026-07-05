//! Shared session-load data structures.
//! 会话加载共享数据结构。

use super::*;
use std::path::PathBuf;

/// Lightweight persisted session locator row used to resolve segmented session directories.
/// 用于解析分段会话目录的轻量持久化会话定位行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredSessionLocatorRow {
    /// Absolute segmented session directory.
    /// 绝对分段会话目录。
    pub(super) session_dir: PathBuf,
    /// Latest persisted canonical turn index mirrored from the event store.
    /// 从事件存储镜像回数据库的最近规范轮次索引。
    pub(super) latest_turn_index: i64,
}

/// Segmented transcript window ready for hydration or history paging.
/// 可直接用于水合或历史分页的分段转录窗口。
pub(super) struct SegmentedWindowMessages {
    /// Visible transcript messages projected from segmented JSONL lines.
    /// 从分段 JSONL 行投影出的可见转录消息。
    pub(super) messages: Vec<SessionMessageEntry>,
    /// Tool-detail messages split out for the existing WebUI side channel.
    /// 为现有 WebUI 带外通道拆出的工具明细消息。
    pub(super) tool_detail_messages: Vec<SessionMessageEntry>,
    /// Earliest event sequence included in this window.
    /// 当前窗口包含的最早事件序号。
    pub(super) earliest_ordinal: Option<i64>,
    /// Whether older segmented events exist before this window.
    /// 当前窗口之前是否还存在更早的分段事件。
    pub(super) has_more_history: bool,
    /// Full projected message window used by runtime-state hydration.
    /// 供运行态水合使用的完整投影消息窗口。
    pub(super) full_messages: Vec<SessionMessageEntry>,
    /// Canonical session-boundary-derived conversation state when the window came from session boundary events.
    /// 当窗口来自 会话边界事件时对应的规范 会话边界投影会话状态。
    pub(super) boundary_conversation_state: Option<SessionConversationState>,
}

/// Selects which segmented source should feed the visible transcript window.
/// 选择用于构建可见转录窗口的分段数据来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SegmentedWindowSourcePreference {
    /// Prefer compact session-boundary projection when it exists.
    /// 当存在会话边界投影时优先使用紧凑投影。
    SessionBoundaryFirst,
    /// Read the raw turn window so tool, thinking, and stats rows stay available.
    /// 读取原始轮次窗口，确保工具、思考与统计行仍可展示。
    RawTurnOnly,
}

/// Returns the display-preserving source preference for ordinary session hydration.
/// 返回普通会话水合使用的保留展示行来源策略。
pub(super) fn default_session_hydration_window_source_preference() -> SegmentedWindowSourcePreference
{
    SegmentedWindowSourcePreference::SessionBoundaryFirst
}

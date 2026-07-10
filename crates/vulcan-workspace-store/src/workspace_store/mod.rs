//! Controller-backed persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的控制器持久化辅助模块。

use crate::conversation_protocol::SessionConversationState;
#[cfg(test)]
use crate::conversation_protocol::conversation_state_from_session_messages;
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::{
    SessionAddonState, SessionCompactionEntry, SessionId, SessionMessageEntry,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, TaskProjectSummary,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionBoundaryStateStore,
    SessionBoundaryStorageWrite, SessionBoundaryStore, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;

mod goal_store;
mod persistence;
mod segmented_session_store;
mod session_addon_store;
mod session_load;

pub use self::segmented_session_store::find_segmented_session_directory_for_task_store_created_at;
use self::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventInput,
    SegmentedSessionEventKind, append_segmented_boundary_events, append_segmented_session_events,
    backup_runtime_log_before_compaction, create_segmented_session_in_location,
    find_segmented_session_directory, find_segmented_session_directory_by_id,
    load_session_boundary_state, read_runtime_log_lines, read_segmented_session_event_window,
    read_state, record_runtime_audit_event, record_runtime_injection_applied,
    replace_session_boundary_state, segmented_session_directory,
    segmented_session_directory_exists, state_document_from_runtime_state, write_state,
};
use self::session_addon_store::read_session_addon_state;
pub use goal_store::{
    GoalAccountingMode, GoalAccountingOutcome, GoalUpdate, ThreadGoal, ThreadGoalStatus,
    VulcanGoalStore,
};
pub use persistence::{
    PersistedSessionForkResult, PersistedUserPromptEditDraft, PersistedUserPromptRollbackResult,
    archive_project_sessions, archive_single_session, fork_persisted_session,
    load_persisted_user_prompt_edit_draft, persist_collapsed_task_project_ids, persist_new_project,
    persist_project_icon_update, persist_project_order_update,
    persist_project_pinned_and_order_update, persist_single_session_addon_state_update,
    persist_single_session_runtime_state_update, persist_single_session_transcript_messages,
    persist_single_session_update, remove_project_after_archiving_sessions, rename_project,
    rename_single_session, restore_project, rollback_persisted_session_before_user_prompt,
};
pub use session_load::{
    load_session_display_hydration_snapshot_for_project_created_at,
    load_session_history_page_for_project_created_at,
    load_session_hydration_snapshot_for_project_created_at,
    load_session_transcript_window_for_project_created_at,
};

/// Maximum transcript turn-page size supported by storage-side lazy transcript paging queries.
/// 存储侧懒加载转录分页查询支持的最大轮次分页大小。
pub const SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT: usize = 50;
/// Number of complete turns kept visible before the latest compaction boundary when the startup transcript page would otherwise stop too late.
/// 当启动转录分页本应停得过晚时，在最近压缩边界之前仍需额外保留的完整轮次数量。
pub const SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT: usize = 5;
/// Maximum message rows fetched for each requested turn before an inner-turn page becomes necessary.
/// 在需要轮内分页前，每个请求轮次允许拉取的最大消息行数。
const SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT: usize = 32;
/// Minimum message-row floor used when one segmented turn window contains dense rows.
/// 当单个分段轮次窗口包含密集行时使用的最小消息行下限。
const SESSION_TRANSCRIPT_MIN_ROW_LIMIT: usize = 100;
/// App metadata key used to persist collapsed task-project identifiers.
/// 用于持久化已折叠任务项目标识的应用元数据键。
const COLLAPSED_TASK_PROJECTS_METADATA_KEY: &str = "workspace.collapsed_task_projects";

mod boundary_store;
mod loading;
mod models;
mod queries;
mod runtime_logs;
mod serialization;

pub use boundary_store::*;
pub use loading::*;
pub use models::*;
pub use runtime_logs::*;

use boundary_store::{
    find_existing_segmented_context_session_dir, session_boundary_event_from_segment_line,
};
use loading::sanitize_loaded_runtime_state_for_fresh_process;
use queries::*;
use serialization::*;

#[cfg(test)]
mod tests;

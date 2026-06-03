//! Controller-backed persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的控制器持久化辅助模块。

#![allow(dead_code)]

use crate::conversation_protocol::{
    SessionConversationState, SessionProviderContinuationState,
    conversation_state_from_session_messages,
};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::{
    SessionCompactionEntry, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, SessionTranscriptUiState, TaskProjectSummary,
    VmmProfileSyncState, VmmWritebackOutboxEntry,
};
use crate::provider_usage::{parse_provider_usage_from_json, serialize_provider_usage_to_json};
use crate::tooling::change_set::compact_change_set_host_result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vldb_controller_client::{
    ControllerSqliteSearchFtsHit, ControllerSqliteTokenizerMode, ControllerSqliteValue,
};
use vulcan_context_injection::ContextInjectionState;
use vulcan_vmm::VmmConfigLayer;

mod event_store;
mod history_search;
mod persistence;
mod session_load;

use self::event_store::{
    SessionEventStorePersistResult, clear_all_session_event_stores,
    load_latest_session_event_store_context_snapshot, load_session_event_store,
    load_session_event_store_compactions, load_session_event_store_context_current,
    load_session_event_store_runtime_state, persist_session_event_store,
    remove_session_event_store,
};
pub use self::event_store::{
    persist_session_event_store_context_current as persist_session_context_current_document,
    snapshot_session_event_store_context_before_user_prompt as snapshot_session_context_before_user_prompt,
};
pub use history_search::search_workspace_history;
pub use persistence::{
    archive_project_with_sessions, archive_single_session, maintain_legacy_session_storage,
    permanently_delete_archived_project, permanently_delete_archived_session,
    persist_collapsed_task_project_ids, persist_new_project, persist_project_order_update,
    persist_project_pinned_and_order_update, persist_single_session_runtime_state_update,
    persist_single_session_update, persist_workspace_snapshot, rename_project,
    restore_project_with_sessions, restore_single_session, toggle_project_pinned,
};
pub use session_load::{load_session_history_page, load_session_hydration_snapshot};

/// Describes the completed workspace-snapshot persistence pass, including any non-fatal auxiliary warnings.
/// 描述一次已完成的工作台快照持久化过程，并携带所有非致命的辅助告警。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshotPersistOutcome {
    /// Non-fatal warnings collected while synchronizing auxiliary projections such as workspace-history search.
    /// 在同步工作台历史搜索等辅助投影期间收集到的非致命告警。
    pub warnings: Vec<String>,
}

impl WorkspaceSnapshotPersistOutcome {
    /// Returns whether the persistence pass completed without any auxiliary warnings.
    /// 返回本次持久化是否在没有任何辅助告警的情况下完成。
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

#[cfg(test)]
use self::persistence::{
    assign_session_message_turn_indexes, build_session_runtime_state_batch_row,
    legacy_inline_message_chunk_count, legacy_inline_message_chunk_migration_script,
    legacy_oversized_message_candidate_query, prepare_session_message_body_for_storage,
    push_session_message_rows_for_session, push_session_runtime_state_batch_rows,
    session_message_chunk_upsert_statement, session_message_is_workspace_history_search_indexable,
    session_message_upsert_statement,
};

/// Target request frame budget used before protobuf/gRPC overhead approaches tonic's default limit.
/// 在 protobuf/gRPC 结构开销接近 tonic 默认限制前使用的请求帧目标预算。
const SQLITE_BATCH_SAFE_FRAME_TARGET_BYTES: usize = 1024 * 1024;
/// Maximum row count kept in one SQLite batch regardless of byte-size estimates.
/// 单个 SQLite 批处理在字节估算之外仍要遵守的最大行数。
const SQLITE_BATCH_SAFE_ROW_LIMIT: usize = 128;
/// Conservative per-batch structure overhead for typed SQLite requests.
/// 类型化 SQLite 请求按批次估算的保守结构开销。
const SQLITE_BATCH_FRAME_OVERHEAD_BYTES: usize = 4096;
/// Conservative per-row structure overhead for typed SQLite request parameters.
/// 类型化 SQLite 请求参数按行估算的保守结构开销。
const SQLITE_ROW_FRAME_OVERHEAD_BYTES: usize = 256;
/// Conservative per-value structure overhead for typed SQLite scalar values.
/// 类型化 SQLite 标量值按字段估算的保守结构开销。
const SQLITE_VALUE_FRAME_OVERHEAD_BYTES: usize = 48;
/// Maximum transcript turn-page size supported by storage-side lazy transcript paging queries.
/// 存储侧懒加载转录分页查询支持的最大轮次分页大小。
pub const SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT: usize = 50;
/// Number of complete turns kept visible before the latest compaction boundary when the startup transcript page would otherwise stop too late.
/// 当启动转录分页本应停得过晚时，在最近压缩边界之前仍需额外保留的完整轮次数量。
pub const SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT: usize = 5;
/// Tool-detail page size used when a specific turn asks for its tool rows.
/// 指定轮次请求工具明细行时使用的分页大小。
pub const SESSION_TOOL_DETAIL_PAGE_LIMIT: usize = 100;
/// Search-result page size used by the lightweight per-session history search.
/// 单会话轻量历史搜索使用的搜索结果页大小。
pub const SESSION_HISTORY_SEARCH_RESULT_LIMIT: usize = 20;
/// Maximum characters retained from one user-provided history-search query.
/// 单条用户提供的历史搜索词允许保留的最大字符数。
pub const SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS: usize = 512;
/// Stable global index name used by controller-managed workspace-history FTS search.
/// 控制器托管工作台历史 FTS 搜索使用的稳定全局索引名。
const WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME: &str = "workspace_history_search_all";
/// Stable project-index prefix used by controller-managed workspace-history FTS search.
/// 控制器托管工作台历史 FTS 搜索使用的稳定项目索引前缀。
const WORKSPACE_HISTORY_SEARCH_PROJECT_INDEX_PREFIX: &str = "workspace_history_search_project_";
/// SQLite name filter used to enumerate controller-managed workspace-history FTS virtual tables.
/// 用于枚举控制器托管工作台历史 FTS 虚表的 SQLite 名称过滤模式。
const WORKSPACE_HISTORY_SEARCH_INDEX_NAME_LIKE_PATTERN: &str = "workspace_history_search_%";
/// Maximum message rows fetched for each requested turn before an inner-turn page becomes necessary.
/// 在需要轮内分页前，每个请求轮次允许拉取的最大消息行数。
const SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT: usize = 32;
/// Minimum message-row cap retained for legacy data whose turn index has not been rebuilt yet.
/// 为尚未重建轮次索引的旧数据保留的最小消息行上限。
const SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT: usize = 100;
/// Maximum persisted metadata JSON size before large host payload fields are stripped.
/// 触发剥离大型宿主载荷字段前允许持久化的最大 metadata JSON 大小。
const SESSION_MESSAGE_METADATA_BUDGET_BYTES: usize = 64 * 1024;
/// Maximum compact metadata preview length retained for oversized argument-like fields.
/// 超大参数类字段被压缩后保留的最大预览长度。
const SESSION_MESSAGE_METADATA_PREVIEW_CHARS: usize = 1024;
/// Maximum body size kept inline in `session_messages.content` before chunk storage is used.
/// 在启用分片存储前允许内联保留在 `session_messages.content` 中的最大正文大小。
const SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES: usize = 48 * 1024;
/// Maximum raw text bytes stored in one session-message chunk row.
/// 单条会话消息分片行允许保存的最大原始文本字节数。
const SESSION_MESSAGE_CHUNK_CONTENT_BYTES: usize = 128 * 1024;
/// Maximum legacy oversized inline messages migrated during one maintenance pass.
/// 单次维护最多迁移的旧版超大内联消息数量。
const SESSION_LEGACY_INLINE_MIGRATION_MAX_MESSAGES_PER_PASS: usize = 32;
/// Number of legacy inline-message candidates fetched per lightweight detection query.
/// 单次轻量检测查询最多获取的旧版内联消息候选数量。
const SESSION_LEGACY_INLINE_MIGRATION_CANDIDATE_LIMIT: usize = 4;
/// Character chunk size used by SQLite-side legacy migration to stay safe for UTF-8 text.
/// SQLite 侧旧数据迁移使用的字符分片大小，用于兼容 UTF-8 文本安全边界。
const SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS: usize = SESSION_MESSAGE_CHUNK_CONTENT_BYTES / 4;
/// Maximum chunk rows generated for one legacy message by the fixed-width SQLite number CTE.
/// 固定宽度 SQLite 数字 CTE 为单条旧消息最多生成的分片行数。
const SESSION_LEGACY_INLINE_MIGRATION_MAX_CHUNKS_PER_MESSAGE: usize = 100_000;
/// Maximum chunk rows fetched in one query response while rebuilding one oversized message.
/// 重建单条超大消息时单次查询响应最多拉取的分片行数。
const SESSION_MESSAGE_CHUNK_QUERY_LIMIT: usize = 8;
/// Maximum preview characters retained in the inline message row for a chunked body.
/// 分片正文在内联消息行中保留的最大预览字符数。
const SESSION_MESSAGE_CHUNK_PREVIEW_CHARS: usize = 2048;
/// Maximum characters returned for one lightweight history-search preview.
/// 单条轻量历史搜索预览允许返回的最大字符数。
const SESSION_HISTORY_SEARCH_PREVIEW_CHARS: usize = 240;
/// App metadata key used to persist collapsed task-project identifiers.
/// 用于持久化已折叠任务项目标识的应用元数据键。
const COLLAPSED_TASK_PROJECTS_METADATA_KEY: &str = "workspace.collapsed_task_projects";

/// Stable in-memory snapshot hydrated from the controller-backed workspace database.
/// 从控制器承载的工作台数据库中还原出的稳定内存快照。
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    /// Visible project nodes shown in the task tree.
    /// 展示在任务树中的可见项目节点集合。
    pub task_projects: Vec<TaskProjectSummary>,
    /// Archived project nodes retained outside the visible tree for later restoration.
    /// 保留在可见树之外、供后续恢复使用的归档项目节点集合。
    pub archived_task_projects: Vec<TaskProjectSummary>,
    /// Visible sessions shown by the workbench.
    /// 展示在工作台中的可见会话集合。
    pub sessions: Vec<SessionSummary>,
    /// Archived sessions retained outside the visible workbench for later restoration.
    /// 保留在可见工作台之外、供后续恢复使用的归档会话集合。
    pub archived_sessions: Vec<SessionSummary>,
    /// Session-scoped transcript history kept fully in memory for the current runtime.
    /// 当前运行时完整保留在内存中的按会话归属转录历史。
    pub session_messages: BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    /// Archived session-scoped transcript history retained for restoration.
    /// 为后续恢复保留的按归档会话归属转录历史集合。
    pub archived_session_messages: BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    /// Session-scoped hidden compaction checkpoints retained for future context reconstruction.
    /// 为未来上下文重建保留的按会话归属隐藏压缩检查点集合。
    pub session_compactions: BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    /// Archived hidden compaction checkpoints retained for restoration.
    /// 为后续恢复保留的按归档会话归属隐藏压缩检查点集合。
    pub archived_session_compactions: BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    /// Session-scoped runtime state used to rebuild context start points and active identities.
    /// 用于重建上下文起点与激活身份的按会话归属运行时状态集合。
    pub session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Archived session-scoped runtime state retained for restoration.
    /// 为后续恢复保留的按归档会话归属运行时状态集合。
    pub archived_session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Fresh execution-phase defaults reconstructed from the loaded sessions.
    /// 根据已加载会话重建出的全新执行阶段默认值集合。
    pub session_run_phases: BTreeMap<SessionId, SessionRunPhase>,
    /// Expanded project identifiers restored for the task tree.
    /// 为任务树恢复出的项目展开标识集合。
    pub expanded_task_projects: BTreeSet<String>,
    /// Selected session identifier restored after startup.
    /// 启动完成后恢复出的当前选中会话标识。
    pub selected_session: SessionId,
    /// Session identifiers whose transcript/runtime payloads were actually loaded into memory.
    /// 已经真实加载到内存中的转录与运行时载荷会话标识集合。
    pub hydrated_session_ids: BTreeSet<SessionId>,
}

/// Lazily loaded transcript and runtime payload for one opened session.
/// 为单个已打开会话懒加载出的转录与运行时载荷。
#[derive(Debug, Clone)]
pub struct SessionHydrationSnapshot {
    /// Session identifier owning the loaded payload.
    /// 拥有本次加载载荷的会话标识。
    pub session_id: SessionId,
    /// Recent transcript messages restored in ascending display order.
    /// 按展示正序恢复出的最近转录消息。
    pub messages: Vec<SessionMessageEntry>,
    /// Complete tool-detail rows belonging to the loaded transcript turns, used to keep transcript rows and tool payloads hydrated together.
    /// 归属于本次已加载转录轮次的完整工具明细行，用于让转录正文与工具载荷同步完成水合。
    pub tool_detail_messages: Vec<SessionMessageEntry>,
    /// Hidden compaction checkpoints scoped to this session.
    /// 归属于该会话的隐藏压缩检查点。
    pub compactions: Vec<SessionCompactionEntry>,
    /// Runtime metadata restored without forcing global startup to read every session body.
    /// 在不迫使全局启动读取所有会话正文的前提下恢复出的运行时元数据。
    pub runtime_state: Option<SessionRuntimeStateEntry>,
    /// Earliest loaded ordinal, used as the cursor for older history pagination.
    /// 已加载的最早序号，用作继续分页加载更早历史的游标。
    pub earliest_ordinal: Option<i64>,
    /// Whether older transcript rows probably still exist before the current window.
    /// 当前窗口之前是否大概率仍存在更早的转录行。
    pub has_more_history: bool,
}

/// One older transcript page loaded before an already hydrated session window.
/// 在已水合会话窗口之前加载的一页更早转录历史。
#[derive(Debug, Clone)]
pub struct SessionHistoryPage {
    /// Session identifier owning this page.
    /// 拥有该页历史的会话标识。
    pub session_id: SessionId,
    /// Older transcript messages restored in ascending display order.
    /// 按展示正序恢复出的更早转录消息。
    pub messages: Vec<SessionMessageEntry>,
    /// Complete tool-detail rows belonging to this older transcript page, loaded together with the visible turns.
    /// 归属于当前更早转录页的完整工具明细行，并与可见轮次一同加载完成。
    pub tool_detail_messages: Vec<SessionMessageEntry>,
    /// Earliest loaded ordinal after this page is merged.
    /// 合并该页后已加载的最早序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether more older transcript rows probably still exist.
    /// 是否大概率仍存在更多更早的转录行。
    pub has_more_history: bool,
}

/// Workspace-scoped BM25 search results returned for one left-panel history query.
/// 左侧面板历史查询返回的工作区级 BM25 搜索结果。
#[derive(Debug, Clone)]
pub struct WorkspaceHistorySearchPage {
    /// Normalized user query used to produce the result page.
    /// 生成该结果页时使用的规范化用户查询。
    pub query: String,
    /// Search hits ordered by BM25 score and recency.
    /// 按 BM25 分数与时间新旧排序的搜索命中集合。
    pub hits: Vec<WorkspaceHistorySearchHit>,
    /// Whether the result count reached the requested limit.
    /// 结果数量是否已经达到请求上限。
    pub limit_reached: bool,
    /// Total match count reported by SQLite FTS for the current query and scope.
    /// SQLite FTS 针对当前查询与范围返回的总命中数量。
    pub total_hits: usize,
    /// Next result offset that should be requested when the user asks to load more.
    /// 用户请求继续加载时应使用的下一页偏移量。
    pub next_offset: usize,
    /// Whether more results still remain after the currently returned page.
    /// 当前结果页之后是否仍然存在更多命中。
    pub has_more: bool,
}

/// One workspace-scoped BM25 history-search hit.
/// 单条工作区级 BM25 历史搜索命中。
#[derive(Debug, Clone)]
pub struct WorkspaceHistorySearchHit {
    /// Human-readable project name rendered in the left search panel.
    /// 渲染在左侧搜索面板中的项目名称。
    pub project_name: String,
    /// Stable session identifier owning the matched message row.
    /// 拥有该命中消息行的稳定会话标识。
    pub session_id: SessionId,
    /// Human-readable session title restored from the persisted session row.
    /// 从持久化会话行恢复出的会话标题。
    pub session_title: String,
    /// Stable identifier of the matched message row.
    /// 命中消息行的稳定标识。
    pub message_id: String,
    /// Stable ordinal inside the session transcript.
    /// 会话转录内部的稳定序号。
    pub ordinal: i64,
    /// Stable turn index inside the session transcript.
    /// 会话转录内部的稳定轮次索引。
    pub turn_index: i64,
    /// High-level message kind reconstructed from the persisted role.
    /// 根据持久化角色恢复出的高层消息类型。
    pub message_kind: SessionMessageKind,
    /// Compact preview fragment extracted by the FTS snippet helper.
    /// 由 FTS snippet 辅助函数提取出的紧凑预览片段。
    pub preview: String,
    /// Raw preview markup returned by the FTS snippet helper, including optional `<mark>` tags.
    /// FTS snippet 辅助函数返回的原始预览标记片段，可能包含 `<mark>` 标签。
    pub preview_markup: String,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    pub created_at: String,
    /// Raw BM25 score returned by SQLite for tie-break visibility.
    /// SQLite 返回的原始 BM25 分值，用于并列诊断。
    pub score: f64,
}

/// One tool-detail page loaded on demand for a single conversation turn.
/// 为单个对话轮次按需加载的一页工具明细。
#[derive(Debug, Clone)]
pub struct SessionToolDetailPage {
    /// Tool-call and tool-result messages restored in ascending display order.
    /// 按展示正序恢复出的工具调用与工具结果消息。
    pub messages: Vec<SessionMessageEntry>,
    /// Earliest loaded ordinal after this page is merged.
    /// 合并该页后已加载的最早序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether more older tool-detail rows probably still exist in this turn.
    /// 当前轮次内是否大概率仍存在更多更早的工具明细行。
    pub has_more_detail: bool,
}

/// Result summary produced after one hydrated session receives legacy-storage maintenance.
/// 单个已水合会话完成旧存储维护后产生的结果摘要。
#[derive(Debug, Clone)]
pub struct SessionMaintenanceReport {
    /// Session identifier that was maintained.
    /// 已执行维护的会话标识。
    pub session_id: SessionId,
    /// Number of currently loaded transcript messages upserted through the new storage path.
    /// 通过新存储路径重新 upsert 的当前已加载转录消息数量。
    pub upserted_message_count: usize,
    /// Whether runtime-state persistence stayed internally consistent during the maintenance pass.
    /// 本次维护过程中运行时状态持久化是否保持了内部一致性。
    pub runtime_state_persistence_ok: bool,
    /// Legacy oversized inline-message migration summary for this maintenance pass.
    /// 本次维护中旧版超大内联消息迁移的结果摘要。
    pub legacy_chunk_migration: SessionLegacyChunkMigrationReport,
}

/// Result summary produced by one legacy oversized inline-message migration pass.
/// 单次旧版超大内联消息迁移产生的结果摘要。
#[derive(Debug, Clone, Default)]
pub struct SessionLegacyChunkMigrationReport {
    /// Session identifier whose legacy messages were inspected.
    /// 被检查旧消息的会话标识。
    pub session_id: SessionId,
    /// Number of oversized inline messages migrated into chunk rows.
    /// 已迁移到分片行的超大内联消息数量。
    pub migrated_message_count: usize,
    /// Number of chunk rows estimated from character lengths for migrated messages.
    /// 根据字符长度估算出的已迁移分片行数量。
    pub inserted_chunk_count: usize,
}

/// Raw project row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始项目行。
#[derive(Debug, Clone, Deserialize)]
struct StoredProjectRow {
    /// Stable project identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定项目标识。
    id: String,
    /// Project display name stored in SQLite.
    /// 存储在 SQLite 中的项目展示名称。
    name: String,
    /// Absolute root path stored in SQLite.
    /// 存储在 SQLite 中的绝对根目录路径。
    root_path: String,
    /// SQLite numeric pinned flag.
    /// SQLite 数值型置顶标记。
    pinned: i64,
    /// Stable section order value.
    /// 稳定的分区顺序值。
    sort_order: i64,
    /// Optional project icon pack stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标包。
    icon_pack: Option<String>,
    /// Optional project icon key stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标键。
    icon_key: Option<String>,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
}

/// Raw application metadata row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始应用元数据行。
#[derive(Debug, Clone, Deserialize)]
struct StoredAppMetadataRow {
    /// Persisted metadata value.
    /// 持久化元数据值。
    value: String,
}

/// Raw session row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始会话行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionRow {
    /// Stable text session identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定文本会话标识。
    id: String,
    /// Owning project identifier stored in SQLite.
    /// 存储在 SQLite 中的所属项目标识。
    project_id: String,
    /// Visible session title stored in SQLite.
    /// 存储在 SQLite 中的可见会话标题。
    title: String,
    /// Persisted title source key.
    /// 持久化标题来源键。
    title_source: String,
    /// Stable order value inside the owning project.
    /// 所属项目内部的稳定顺序值。
    sort_order: i64,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
    /// Last-updated timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的最近更新时间戳。
    updated_at: String,
}

/// Raw session message row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始会话消息行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionMessageRow {
    /// Stable message identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定消息标识。
    id: String,
    /// Stable ordinal inside one transcript stream.
    /// 单个转录流内部的稳定序号。
    ordinal: i64,
    /// Persisted message role used to reconstruct the message kind.
    /// 用于重建消息类型的持久化消息角色。
    role: String,
    /// Persisted record kind key.
    /// 持久化记录类型键。
    kind: String,
    /// SQLite numeric visible flag.
    /// SQLite 数值型可见标记。
    visible: i64,
    /// Stable conversation-turn index derived from user-authored prompts.
    /// 根据用户提示词派生出的稳定对话轮次索引。
    #[serde(default)]
    turn_index: i64,
    /// Persisted primary text body.
    /// 持久化后的主要正文文本。
    content: String,
    /// Optional structured JSON metadata.
    /// 可选的结构化 JSON 元数据。
    meta_json: Option<String>,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    created_at: String,
}

/// Lightweight session turn-index row used by tool-detail hydration lookups.
/// 工具明细水合查询使用的轻量会话轮次索引行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionTurnIndexRow {
    /// Stable conversation-turn index already stored in SQLite.
    /// 已持久化到 SQLite 中的稳定对话轮次索引。
    turn_index: i64,
}

/// Raw session-message chunk row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始会话消息分片行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionMessageChunkRow {
    /// Stable message identifier owning this chunk.
    /// 拥有该分片的稳定消息标识。
    message_id: String,
    /// Stable chunk order inside one message body.
    /// 单条消息正文内部的稳定分片顺序。
    chunk_index: i64,
    /// Persisted chunk body.
    /// 持久化后的分片正文。
    content: String,
}

/// Lightweight legacy oversized inline-message candidate row.
/// 轻量旧版超大内联消息候选行。
#[derive(Debug, Clone, Deserialize)]
struct StoredLegacyOversizedMessageCandidateRow {
    /// Stable message identifier selected for migration.
    /// 被选中迁移的稳定消息标识。
    id: String,
    /// Inline body size measured by SQLite as UTF-8 bytes.
    /// SQLite 以 UTF-8 字节计量出的内联正文大小。
    content_bytes: i64,
    /// Inline body length measured by SQLite as text characters.
    /// SQLite 以文本字符计量出的内联正文长度。
    content_chars: i64,
}

/// Lightweight persisted message position used to preserve partial-window upsert order.
/// 用于保留局部窗口上写顺序的轻量持久化消息位置。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionMessagePositionRow {
    /// Stable message identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定消息标识。
    id: String,
    /// Stable ordinal already assigned inside one session transcript.
    /// 已经在单个会话转录中分配的稳定序号。
    ordinal: i64,
    /// Stable turn index already assigned inside one session transcript.
    /// 已经在单个会话转录中分配的稳定轮次索引。
    turn_index: i64,
}

/// Maximum persisted message identifiers looked up in one position query.
/// 单次位置查询最多查找的持久化消息标识数量。
const SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT: usize = 128;

/// Raw workspace-history search metadata row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始工作台历史搜索元数据行。
#[derive(Debug, Clone, Deserialize)]
struct StoredWorkspaceHistorySearchMetadataRow {
    /// Human-readable project name stored on the project row.
    /// 存储在项目行中的项目名称。
    project_name: String,
    /// Stable session identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定会话标识。
    session_id: String,
    /// Human-readable session title stored on the session row.
    /// 存储在会话行中的会话标题。
    session_title: String,
    /// Stable message identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定消息标识。
    message_id: String,
    /// Stable ordinal inside one transcript stream.
    /// 单个转录流内部的稳定序号。
    ordinal: i64,
    /// Stable turn index inside one transcript stream.
    /// 单个转录流内部的稳定轮次索引。
    turn_index: i64,
    /// Persisted message role used to reconstruct the user-facing label.
    /// 用于重建用户界面标签的持久化消息角色。
    role: String,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    created_at: String,
}

/// Raw workspace-history search ownership row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始工作台历史搜索归属行。
#[derive(Debug, Clone, Deserialize)]
struct StoredWorkspaceHistorySearchDocumentStateRow {
    /// Stable message identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定消息标识。
    message_id: String,
    /// Stable session identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定会话标识。
    #[serde(rename = "session_id")]
    _session_id: String,
    /// Stable project identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定项目标识。
    project_id: String,
}

/// One prepared workspace-history search document awaiting controller-managed FTS synchronization.
/// 一条等待同步到控制器托管 FTS 的预处理工作台历史搜索文档。
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceHistorySearchDocumentUpsert {
    /// Stable message identifier used as the document id.
    /// 作为文档标识使用的稳定消息 ID。
    message_id: String,
    /// Stable session identifier owning the document.
    /// 拥有该文档的稳定会话标识。
    session_id: String,
    /// Stable project identifier used to route the document into its project index.
    /// 用于将文档路由到所属项目索引的稳定项目标识。
    project_id: String,
    /// Stable canonical event identifier backing the indexed document.
    /// 作为索引文档后端来源的稳定规范事件标识。
    event_id: String,
    /// Stable turn index used by transcript paging and search-hit reconstruction.
    /// 被转录分页与搜索命中重建使用的稳定轮次索引。
    turn_index: i64,
    /// Stable event sequence used as the persisted ordinal-like anchor.
    /// 作为持久化类 ordinal 锚点使用的稳定事件序号。
    ordinal: i64,
    /// Persisted role label used to rebuild the high-level message kind.
    /// 用于重建高层消息类型的持久化角色标签。
    role: String,
    /// Created-at timestamp stored beside the search projection.
    /// 与搜索投影一同保存的创建时间戳。
    created_at: String,
    /// Final transcript body indexed by controller-managed FTS search.
    /// 由控制器托管 FTS 搜索建立索引的最终转录正文。
    content: String,
}

/// Structured transcript-message metadata payload persisted inside the SQLite `meta_json` column.
/// 持久化在 SQLite `meta_json` 列中的结构化转录消息元数据载荷。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredSessionMessageMeta {
    /// Optional user-visible transcript card title.
    /// 可选的用户可见转录卡片标题。
    #[serde(default)]
    title: Option<String>,
    /// Optional structured transcript metadata used only by host-side UI surfaces.
    /// 仅供宿主侧界面表面使用的可选结构化转录元数据。
    #[serde(default)]
    metadata: Option<Value>,
}

/// Raw compaction checkpoint row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始压缩检查点行。
#[derive(Debug, Clone, Deserialize)]
struct StoredCompactionRow {
    /// Stable compaction identifier.
    /// 稳定压缩标识。
    id: String,
    /// Optional source tail message identifier.
    /// 可选的源历史尾消息标识。
    source_until_message_id: Option<String>,
    /// Optional absolute source tail turn index.
    /// 可选的绝对源历史尾轮次索引。
    #[serde(default)]
    source_until_turn_index: Option<i64>,
    /// Optional rebuilt context start message identifier.
    /// 可选的新上下文起始消息标识。
    context_start_message_id: Option<String>,
    /// Optional absolute retained-tail start turn index.
    /// 可选的绝对保留尾部起始轮次索引。
    #[serde(default)]
    retained_tail_start_turn_index: Option<i64>,
    /// Optional hidden summary message identifier.
    /// 可选的隐藏摘要消息标识。
    summary_message_id: Option<String>,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    created_at: String,
}

/// Raw runtime-state row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始运行时状态行。
#[derive(Debug, Clone, Deserialize)]
struct StoredRuntimeStateRow {
    /// Stable session identifier.
    /// 稳定会话标识。
    session_id: String,
    /// Optional active agent identifier.
    /// 可选的激活代理标识。
    active_agent_id: Option<String>,
    /// Optional active provider identifier.
    /// 可选的激活供应商标识。
    active_provider_id: Option<String>,
    /// Optional active model identifier.
    /// 可选的激活模型标识。
    active_model_id: Option<String>,
    /// Optional latest compaction identifier.
    /// 可选的最近一次压缩标识。
    last_compaction_id: Option<String>,
    /// Optional current context start message identifier.
    /// 可选的当前上下文起始消息标识。
    context_start_message_id: Option<String>,
    /// Optional persisted transcript UI state payload.
    /// 可选的持久化转录界面状态载荷。
    ui_state_json: Option<String>,
    /// Optional persisted provider usage payload.
    /// 可选的持久化 Provider 用量载荷。
    #[serde(default)]
    provider_usage_json: Option<String>,
    /// Optional persisted full canonical conversation-state payload.
    /// 可选的持久化完整规范会话状态载荷。
    #[serde(default)]
    conversation_state_json: Option<String>,
    /// Optional persisted lightweight provider continuation payload.
    /// 可选的持久化轻量 Provider continuation 载荷。
    #[serde(default)]
    continuation_json: Option<String>,
    /// Optional persisted hidden context-injection runtime payload.
    /// 可选的隐藏上下文注入运行态载荷。
    #[serde(default)]
    context_injection_json: Option<String>,
    /// Optional persisted session-level VMM override payload.
    /// 可选的持久化会话级 VMM 覆盖载荷。
    #[serde(default)]
    vmm_json: Option<String>,
    /// Optional persisted VMM writeback outbox payload.
    /// 可选的持久化 VMM 写回 outbox 载荷。
    #[serde(default)]
    vmm_writeback_outbox_json: Option<String>,
    /// Optional persisted VMM profile sync state payload.
    /// 可选的持久化 VMM 画像同步状态载荷。
    #[serde(default)]
    vmm_profile_sync_json: Option<String>,
    /// Updated-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的更新时间。
    updated_at: String,
}

/// Loads the persisted workspace snapshot or seeds the controller database when it is still empty.
/// 加载持久化工作台快照，若控制器数据库仍为空则先写入种子数据。
pub async fn load_or_seed_workspace(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    let existing_project_count = query_project_count(&controller).await?;

    if existing_project_count == 0 {
        return Ok(build_empty_workspace_snapshot());
    }

    let snapshot = load_workspace_snapshot(controller.clone()).await?;
    if is_legacy_seed_workspace(&snapshot) {
        clear_workspace_storage(&controller).await?;
        return Ok(build_empty_workspace_snapshot());
    }

    Ok(snapshot)
}

/// Loads the current persisted workspace snapshot from the controller-backed SQLite database.
/// 从控制器承载的 SQLite 数据库中加载当前持久化工作台快照。
pub async fn load_workspace_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    let project_rows = query_project_rows(&controller).await?;
    let mut session_rows = query_session_rows(&controller).await?;
    if migrate_legacy_session_ids_if_needed(&controller, &session_rows).await? {
        session_rows = query_session_rows(&controller).await?;
    }
    let runtime_state_rows = query_session_runtime_state_shell_rows(&controller).await?;

    let mut task_projects = Vec::new();
    let mut archived_task_projects = Vec::new();
    for row in &project_rows {
        let project = TaskProjectSummary::new(
            row.id.clone(),
            row.name.clone(),
            row.root_path.clone(),
            row.pinned != 0,
            row.sort_order.max(0) as usize,
            row.icon_pack.clone(),
            row.icon_key.clone(),
            Vec::new(),
        );

        if row.archived != 0 {
            archived_task_projects.push(project);
        } else {
            task_projects.push(project);
        }
    }

    let archived_session_ids = session_rows
        .iter()
        .filter(|row| row.archived != 0)
        .filter_map(|row| parse_session_id(&row.id).ok())
        .collect::<BTreeSet<_>>();

    let transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let archived_transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();
    let archived_compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();

    let mut runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut archived_runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> =
        BTreeMap::new();
    for row in runtime_state_rows {
        let session_id = parse_session_id(&row.session_id)?;
        let provider_usage = row
            .provider_usage_json
            .as_deref()
            .and_then(parse_provider_usage_from_json);
        let entry = SessionRuntimeStateEntry {
            active_agent_id: row.active_agent_id,
            active_provider_id: row.active_provider_id,
            active_model_id: row.active_model_id,
            active_openrouter_endpoint_route: None,
            active_reasoning_effort: None,
            active_temperature: None,
            prompt_skill_overrides: Default::default(),
            context_injection_state: row
                .context_injection_json
                .as_deref()
                .and_then(parse_context_injection_state_from_json)
                .unwrap_or_default(),
            vmm: row
                .vmm_json
                .as_deref()
                .and_then(parse_vmm_config_layer_from_json)
                .unwrap_or_default(),
            vmm_writeback_outbox: row
                .vmm_writeback_outbox_json
                .as_deref()
                .and_then(parse_vmm_writeback_outbox_from_json)
                .unwrap_or_default(),
            vmm_profile_sync: row
                .vmm_profile_sync_json
                .as_deref()
                .and_then(parse_vmm_profile_sync_from_json)
                .unwrap_or_default(),
            last_compaction_id: row.last_compaction_id,
            context_start_message_id: row.context_start_message_id,
            live_context_budget_fingerprint: None,
            live_context_assembly_fingerprint: None,
            anchor_context_budget_fingerprint: None,
            anchor_context_assembly_fingerprint: None,
            last_provider_request_preview_markdown: None,
            ui_state: row
                .ui_state_json
                .as_deref()
                .and_then(parse_session_ui_state_from_json)
                .unwrap_or_default(),
            conversation_state: persisted_conversation_state_or_continuation(
                row.conversation_state_json,
                row.continuation_json,
            ),
            last_provider_usage: provider_usage.clone(),
            last_successful_provider_usage: provider_usage,
            last_safe_provider_request_count: None,
            updated_at: row.updated_at,
        };

        if archived_session_ids.contains(&session_id) {
            archived_runtime_state_map.insert(session_id, entry);
        } else {
            runtime_state_map.insert(session_id, entry);
        }
    }

    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    for row in &session_rows {
        let session_id = parse_session_id(&row.id)?;
        let focus = session_focus_preview(&[]);
        let session_summary = SessionSummary::new(
            session_id,
            Some(row.project_id.clone()),
            row.title.clone(),
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(&row.title_source))
        .with_updated_at(row.updated_at.clone());

        if row.archived != 0 {
            if let Some(project) = archived_task_projects
                .iter_mut()
                .find(|project| project.id == row.project_id)
            {
                project.session_ids.push(session_id.clone());
            }
            archived_sessions.push(session_summary);
        } else {
            if let Some(project) = task_projects
                .iter_mut()
                .find(|project| project.id == row.project_id)
            {
                project.session_ids.push(session_id.clone());
            }
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            session_rows
                .iter()
                .find(|row| row.id == session_id.to_string())
                .map(|row| row.sort_order)
                .unwrap_or(i64::MAX)
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            session_rows
                .iter()
                .find(|row| row.id == session_id.to_string())
                .map(|row| row.sort_order)
                .unwrap_or(i64::MAX)
        });
    }

    visible_sessions.sort_by_key(|session| {
        session_rows
            .iter()
            .find(|row| row.id == session.id.to_string())
            .and_then(|row| row.updated_at.parse::<i64>().ok())
            .map(|timestamp| -timestamp)
            .unwrap_or(i64::MIN)
    });

    archived_sessions.sort_by_key(|session| {
        session_rows
            .iter()
            .find(|row| row.id == session.id.to_string())
            .and_then(|row| row.updated_at.parse::<i64>().ok())
            .map(|timestamp| -timestamp)
            .unwrap_or(i64::MIN)
    });

    let collapsed_task_projects = query_collapsed_task_project_ids(&controller)
        .await?
        .unwrap_or_default();
    let expanded_task_projects = task_projects
        .iter()
        .filter(|project| !collapsed_task_projects.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let selected_session = visible_sessions
        .first()
        .map(|session| session.id.clone())
        .unwrap_or_default();
    let session_run_phases = visible_sessions
        .iter()
        .map(|session| (session.id.clone(), SessionRunPhase::Idle))
        .collect::<BTreeMap<_, _>>();

    Ok(WorkspaceSnapshot {
        task_projects,
        archived_task_projects,
        sessions: visible_sessions,
        archived_sessions,
        session_messages: transcript_map,
        archived_session_messages: archived_transcript_map,
        session_compactions: compaction_map,
        archived_session_compactions: archived_compaction_map,
        session_runtime_state: runtime_state_map,
        archived_session_runtime_state: archived_runtime_state_map,
        session_run_phases,
        expanded_task_projects,
        selected_session,
        hydrated_session_ids: BTreeSet::new(),
    })
}

/// Builds the default empty workspace snapshot used before any real project has been added.
/// 构建在尚未添加任何真实项目之前使用的默认空工作台快照。
fn build_empty_workspace_snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    }
}

/// Returns whether one restored snapshot still exactly matches the old prototype seed workspace.
/// 返回一份已恢复快照是否仍然与旧原型种子工作台完全一致。
fn is_legacy_seed_workspace(snapshot: &WorkspaceSnapshot) -> bool {
    if !snapshot.archived_task_projects.is_empty()
        || !snapshot.archived_sessions.is_empty()
        || !snapshot.archived_session_messages.is_empty()
        || !snapshot.archived_session_compactions.is_empty()
    {
        return false;
    }

    let seeded_projects = TaskProjectSummary::seeded();
    let seeded_sessions = SessionSummary::seeded();

    snapshot.task_projects.len() == seeded_projects.len()
        && snapshot.sessions.len() == seeded_sessions.len()
        && snapshot
            .task_projects
            .iter()
            .zip(seeded_projects.iter())
            .all(|(current, seeded)| {
                current.id == seeded.id
                    && current.title == seeded.title
                    && current.path == seeded.path
                    && current.session_ids == seeded.session_ids
            })
        && snapshot
            .sessions
            .iter()
            .zip(seeded_sessions.iter())
            .all(|(current, seeded)| {
                current.id == seeded.id
                    && current.project_id == seeded.project_id
                    && current.title == seeded.title
                    && current.title_source == seeded.title_source
            })
}

/// Clears all workspace business tables so the application can rebuild from a clean empty state.
/// 清空全部工作台业务表，以便应用从干净的空状态重新开始。
async fn clear_workspace_storage(controller: &ControllerRuntimeHandle) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
DELETE FROM workspace_history_search_docs;
DELETE FROM session_turn_index;
DELETE FROM session_runtime_state;
DELETE FROM session_compactions;
DELETE FROM session_message_chunks;
DELETE FROM session_messages;
DELETE FROM sessions;
DELETE FROM projects;
DELETE FROM app_metadata WHERE key = 'workspace.collapsed_task_projects';
"#,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("清理旧原型工作台数据失败：{error}"))?;
    clear_all_session_event_stores()?;

    let index_names = query_workspace_history_search_index_names(controller).await?;
    drop_workspace_history_search_indexes(controller, &index_names, "清理工作台历史搜索索引失败")
        .await?;

    Ok(())
}

/// Returns the current controller-scoped SQLite binding identifier used by the application workspace tables.
/// 返回应用工作台表当前使用的控制器作用域 SQLite 绑定标识符。
fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
}

/// Queries all controller-managed workspace-history FTS virtual table names currently stored in SQLite.
/// 查询当前 SQLite 中存储的全部控制器托管工作台历史 FTS 虚表名称。
async fn query_workspace_history_search_index_names(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<String>, String> {
    #[derive(Debug, Deserialize)]
    struct IndexNameRow {
        /// Virtual-table name.
        /// 虚表名称。
        name: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE ?1 AND sql LIKE 'CREATE VIRTUAL TABLE%' ORDER BY name ASC;",
            vec![ControllerSqliteValue::String(
                WORKSPACE_HISTORY_SEARCH_INDEX_NAME_LIKE_PATTERN.to_string(),
            )],
        )
        .await
        .map_err(|error| format!("查询工作台历史搜索索引列表失败：{error}"))?;

    Ok(decode_query_rows::<IndexNameRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

/// Drops the provided controller-managed workspace-history FTS virtual tables.
/// 删除给定的控制器托管工作台历史 FTS 虚表。
async fn drop_workspace_history_search_indexes(
    controller: &ControllerRuntimeHandle,
    index_names: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if index_names.is_empty() {
        return Ok(());
    }

    let script = index_names
        .iter()
        .map(|index_name| {
            format!(
                "DROP TABLE IF EXISTS {};",
                quote_sqlite_identifier(index_name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            script,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("{error_prefix}：{error}"))?;

    Ok(())
}

/// Ensures one controller-managed workspace-history FTS index exists with Jieba tokenization enabled.
/// 确保某个控制器托管工作台历史 FTS 索引已经启用 Jieba 分词并存在。
async fn ensure_workspace_history_search_index(
    controller: &ControllerRuntimeHandle,
    index_name: &str,
) -> Result<(), String> {
    controller
        .client
        .ensure_sqlite_fts_index(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            index_name.to_string(),
            ControllerSqliteTokenizerMode::Jieba,
        )
        .await
        .map_err(|error| format!("确保工作台历史搜索索引 `{index_name}` 失败：{error}"))?;

    Ok(())
}

/// Queries the number of visible project rows currently stored in the application database.
/// 查询当前应用数据库中已存储的可见项目行数量。
async fn query_project_count(controller: &ControllerRuntimeHandle) -> Result<u64, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT COUNT(*) AS total FROM projects WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目数量失败：{error}"))?;

    #[derive(Deserialize)]
    struct CountRow {
        /// Numeric total returned by SQLite.
        /// SQLite 返回的数值总量。
        total: u64,
    }

    let rows = decode_query_rows::<CountRow>(&result.json_data)?;
    Ok(rows.first().map(|row| row.total).unwrap_or(0))
}

/// Queries visible project rows ordered by pinned section and stable order.
/// 查询按置顶分区与稳定顺序排序后的可见项目行。
async fn query_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, name, root_path, pinned, sort_order, icon_pack, icon_key, archived
FROM projects
ORDER BY pinned DESC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries the persisted collapsed project identifiers, if a workspace UI state row exists.
/// 查询已持久化的折叠项目标识集合；若工作台 UI 状态行不存在则返回空。
async fn query_collapsed_task_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<Option<BTreeSet<String>>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT value FROM app_metadata WHERE key = ?1;",
            vec![ControllerSqliteValue::String(
                COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string(),
            )],
        )
        .await
        .map_err(|error| format!("查询工作台项目展开状态失败：{error}"))?;

    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&row.value)
        .map_err(|error| format!("解析工作台项目展开状态失败：{error}"))?;
    Ok(Some(parsed.into_iter().collect()))
}

/// Queries active project identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活项目标识集合。
async fn query_active_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<BTreeSet<String>, String> {
    #[derive(Deserialize)]
    struct IdRow {
        /// Persisted identifier.
        /// 持久化标识。
        id: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT id FROM projects WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询激活项目标识失败：{error}"))?;

    Ok(decode_query_rows::<IdRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// Queries visible session rows ordered by project and stable order.
/// 查询按项目与稳定顺序排序后的可见会话行。
async fn query_session_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, project_id, title, title_source, sort_order, archived, updated_at
FROM sessions
ORDER BY project_id ASC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台会话列表失败：{error}"))?;

    decode_query_rows::<StoredSessionRow>(&result.json_data)
}

/// Queries active session identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活会话标识集合。
async fn query_active_session_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<BTreeSet<String>, String> {
    #[derive(Deserialize)]
    struct IdRow {
        /// Persisted identifier.
        /// 持久化标识。
        id: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT id FROM sessions WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询激活会话标识失败：{error}"))?;

    Ok(decode_query_rows::<IdRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// Returns the maximum message rows fetched for one turn-level transcript window.
/// 返回单个轮次级转录窗口允许拉取的最大消息行数。
fn session_transcript_row_limit_for_turns(turn_limit: usize) -> usize {
    SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT
        .max(turn_limit.saturating_mul(SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT))
}

/// Returns whether one persisted record kind should be part of the default transcript window.
/// 返回某个持久化记录类型是否应进入默认转录窗口。
#[cfg(test)]
fn persisted_kind_in_default_transcript_window(kind: &str) -> bool {
    !matches!(kind, "tool-call-output" | "tool-result-output")
}

/// Returns whether one persisted record kind belongs to the on-demand tool-detail stream.
/// 返回某个持久化记录类型是否属于按需工具明细流。
#[cfg(test)]
fn persisted_kind_in_tool_detail_stream(kind: &str) -> bool {
    matches!(kind, "tool-call-output" | "tool-result-output")
}

/// Queries lightweight runtime-state rows for startup without the large canonical conversation JSON.
/// 启动时查询不含大型规范会话 JSON 的轻量运行时状态行。
async fn query_session_runtime_state_shell_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredRuntimeStateRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.provider_usage_json, NULL AS conversation_state_json, r.continuation_json, r.context_injection_json, r.vmm_json, r.vmm_writeback_outbox_json, r.vmm_profile_sync_json, r.updated_at
FROM session_runtime_state AS r
INNER JOIN sessions AS s ON s.id = r.session_id
ORDER BY r.session_id ASC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询会话运行时状态失败：{error}"))?;

    decode_query_rows::<StoredRuntimeStateRow>(&result.json_data)
}

/// Merges generated-media tool-result rows into one visible transcript window while keeping row ordering stable.
/// 将生成媒体工具结果行合并进一个可见转录窗口，同时保持行顺序稳定。
fn merge_generated_media_rows_into_transcript_window(
    transcript_rows: &mut Vec<StoredSessionMessageRow>,
    generated_media_rows: Vec<StoredSessionMessageRow>,
) -> Option<i64> {
    let existing_ids = transcript_rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    transcript_rows.extend(
        generated_media_rows
            .into_iter()
            .filter(|row| !existing_ids.contains(&row.id)),
    );
    transcript_rows.sort_by_key(|row| row.ordinal);
    transcript_rows.first().map(|row| row.ordinal)
}

/// Returns whether one controller error looks like tonic's protobuf message-size guard.
/// 判断一条控制器错误是否类似 tonic 的 protobuf 消息大小保护。
fn is_grpc_message_frame_size_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("decoded message length too large")
        || normalized.contains("message length too large")
        || normalized.contains("limit is: 4194304")
}

/// Decodes one controller JSON row-set payload into typed Rust rows.
/// 将一条控制器 JSON 行集载荷解码为带类型的 Rust 行集合。
fn decode_query_rows<T>(json_data: &str) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<Vec<T>>(json_data)
        .map_err(|error| format!("解析控制器 SQLite 查询结果失败：{error}"))
}

/// Converts one persisted message row into the in-memory transcript entry shape.
/// 将一条持久化消息行转换为内存中的转录消息结构。
fn stored_session_message_row_to_entry(
    row: StoredSessionMessageRow,
    message_body_chunks: &BTreeMap<String, String>,
) -> SessionMessageEntry {
    let persisted_meta = row
        .meta_json
        .as_deref()
        .and_then(parse_session_message_meta_from_json)
        .unwrap_or_default();
    let title = persisted_meta
        .title
        .unwrap_or_else(|| default_message_title(&row.role).to_string());
    let body = message_body_chunks
        .get(&row.id)
        .cloned()
        .unwrap_or_else(|| row.content.clone());

    SessionMessageEntry {
        id: row.id,
        kind: session_kind_from_role(&row.role),
        record_kind: SessionMessageRecordKind::from_persisted_key(&row.kind),
        visible: row.visible != 0,
        title,
        body,
        metadata: persisted_meta.metadata,
        created_at: row.created_at,
    }
}

/// Rebuilds one oversized message body from ordered chunk rows and validates ownership.
/// 从有序分片行重建单条超大消息正文，并校验归属关系。
fn rebuild_session_message_body_from_chunks(
    message_id: &str,
    mut chunk_rows: Vec<StoredSessionMessageChunkRow>,
) -> Result<String, String> {
    chunk_rows.sort_by_key(|row| row.chunk_index);

    let mut rebuilt_body = String::new();
    for (expected_index, row) in chunk_rows.into_iter().enumerate() {
        if row.message_id != message_id {
            return Err(format!(
                "会话消息分片归属不一致：期望 `{message_id}`，实际 `{}`。",
                row.message_id
            ));
        }
        if row.chunk_index != expected_index as i64 {
            return Err(format!(
                "会话消息 `{message_id}` 分片序号不连续：期望 {expected_index}，实际 {}。",
                row.chunk_index
            ));
        }
        rebuilt_body.push_str(&row.content);
    }

    Ok(rebuilt_body)
}

/// Converts one persisted compaction row into the in-memory checkpoint shape.
/// 将一条持久化压缩行转换为内存中的检查点结构。
fn stored_compaction_row_to_entry(row: StoredCompactionRow) -> SessionCompactionEntry {
    SessionCompactionEntry {
        id: row.id,
        source_until_message_id: row.source_until_message_id,
        source_until_turn_index: row.source_until_turn_index,
        context_start_message_id: row.context_start_message_id,
        context_budget_fingerprint: None,
        context_assembly_fingerprint: None,
        retained_tail_start_turn_index: row.retained_tail_start_turn_index,
        summary_message_id: row.summary_message_id,
        created_at: row.created_at,
    }
}

/// Converts one persisted runtime row into the in-memory runtime-state shape.
/// 将一条持久化运行时行转换为内存中的运行时状态结构。
fn stored_runtime_state_row_to_entry(row: StoredRuntimeStateRow) -> SessionRuntimeStateEntry {
    let provider_usage = row
        .provider_usage_json
        .as_deref()
        .and_then(parse_provider_usage_from_json);
    SessionRuntimeStateEntry {
        active_agent_id: row.active_agent_id,
        active_provider_id: row.active_provider_id,
        active_model_id: row.active_model_id,
        active_openrouter_endpoint_route: None,
        active_reasoning_effort: None,
        active_temperature: None,
        prompt_skill_overrides: Default::default(),
        context_injection_state: row
            .context_injection_json
            .as_deref()
            .and_then(parse_context_injection_state_from_json)
            .unwrap_or_default(),
        vmm: row
            .vmm_json
            .as_deref()
            .and_then(parse_vmm_config_layer_from_json)
            .unwrap_or_default(),
        vmm_writeback_outbox: row
            .vmm_writeback_outbox_json
            .as_deref()
            .and_then(parse_vmm_writeback_outbox_from_json)
            .unwrap_or_default(),
        vmm_profile_sync: row
            .vmm_profile_sync_json
            .as_deref()
            .and_then(parse_vmm_profile_sync_from_json)
            .unwrap_or_default(),
        last_compaction_id: row.last_compaction_id,
        context_start_message_id: row.context_start_message_id,
        live_context_budget_fingerprint: None,
        live_context_assembly_fingerprint: None,
        anchor_context_budget_fingerprint: None,
        anchor_context_assembly_fingerprint: None,
        last_provider_request_preview_markdown: None,
        ui_state: row
            .ui_state_json
            .as_deref()
            .and_then(parse_session_ui_state_from_json)
            .unwrap_or_default(),
        conversation_state: persisted_conversation_state_or_continuation(
            row.conversation_state_json,
            row.continuation_json,
        ),
        last_provider_usage: provider_usage.clone(),
        last_successful_provider_usage: provider_usage,
        last_safe_provider_request_count: None,
        updated_at: row.updated_at,
    }
}

/// Builds a minimal conversation state from one persisted continuation payload.
/// 根据一条持久化 continuation 载荷构建最小会话状态。
fn conversation_state_with_continuation(
    continuation_json: Option<String>,
) -> SessionConversationState {
    SessionConversationState {
        continuation: continuation_json
            .as_deref()
            .and_then(parse_session_continuation_from_json)
            .unwrap_or_default(),
        ..SessionConversationState::default()
    }
}

/// Restores one persisted canonical conversation state when available and otherwise falls back to the lightweight continuation-only payload.
/// 在可用时恢复一条持久化规范会话状态，否则回退到仅包含轻量 continuation 的载荷。
fn persisted_conversation_state_or_continuation(
    conversation_state_json: Option<String>,
    continuation_json: Option<String>,
) -> SessionConversationState {
    parse_session_conversation_state_from_json(conversation_state_json.as_deref())
        .unwrap_or_else(|| conversation_state_with_continuation(continuation_json))
}

/// Parses one persisted session identifier back into the in-memory UUID session key.
/// 将一条持久化会话标识解析回内存中的 UUID 会话键。
fn parse_session_id(value: &str) -> Result<SessionId, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(String::from("解析持久化会话标识失败：会话标识不能为空。"));
    }

    if let Ok(uuid) = Uuid::parse_str(normalized) {
        return Ok(uuid);
    }

    Ok(legacy_session_id_fallback(normalized))
}

/// Migrates legacy non-UUID persisted session identifiers to canonical UUID text so scoped deletes keep working.
/// 将旧版非 UUID 持久化会话标识迁移为规范 UUID 文本，确保后续按会话范围删除旧行时能够正确命中。
async fn migrate_legacy_session_ids_if_needed(
    controller: &ControllerRuntimeHandle,
    session_rows: &[StoredSessionRow],
) -> Result<bool, String> {
    let legacy_mappings = session_rows
        .iter()
        .filter(|row| !is_uuid_session_id_text(&row.id))
        .map(|row| {
            (
                row.id.clone(),
                legacy_session_id_fallback(&row.id).to_string(),
            )
        })
        .collect::<Vec<_>>();

    if legacy_mappings.is_empty() {
        return Ok(false);
    }

    let mut migration_script =
        String::from("PRAGMA foreign_keys = OFF;\nBEGIN IMMEDIATE TRANSACTION;\n");
    for (legacy_id, canonical_id) in &legacy_mappings {
        let escaped_legacy_id = sqlite_escape_text(legacy_id);
        let escaped_canonical_id = sqlite_escape_text(canonical_id);
        migration_script.push_str(&format!(
            "UPDATE session_messages SET session_id = '{escaped_canonical_id}' WHERE session_id = '{escaped_legacy_id}';\n"
        ));
        migration_script.push_str(&format!(
            "UPDATE session_message_chunks SET session_id = '{escaped_canonical_id}' WHERE session_id = '{escaped_legacy_id}';\n"
        ));
        migration_script.push_str(&format!(
            "UPDATE session_compactions SET session_id = '{escaped_canonical_id}' WHERE session_id = '{escaped_legacy_id}';\n"
        ));
        migration_script.push_str(&format!(
            "UPDATE session_runtime_state SET session_id = '{escaped_canonical_id}' WHERE session_id = '{escaped_legacy_id}';\n"
        ));
        migration_script.push_str(&format!(
            "UPDATE sessions SET id = '{escaped_canonical_id}' WHERE id = '{escaped_legacy_id}';\n"
        ));
    }
    migration_script.push_str("COMMIT;\nPRAGMA foreign_keys = ON;\n");

    controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            migration_script,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("迁移旧版会话 UUID 标识失败：{error}"))?;

    Ok(true)
}

/// Returns whether one persisted session identifier already matches UUID text.
/// 返回一条持久化会话标识是否已经符合 UUID 文本格式。
fn is_uuid_session_id_text(value: &str) -> bool {
    Uuid::parse_str(value.trim()).is_ok()
}

/// Builds one deterministic UUID fallback from one legacy non-UUID session identifier.
/// 根据旧版非 UUID 会话标识构建一条确定性的 UUID 兼容回退值。
fn legacy_session_id_fallback(value: &str) -> SessionId {
    let mut high_hasher = DefaultHasher::new();
    "vulcan-legacy-session-id-high".hash(&mut high_hasher);
    value.hash(&mut high_hasher);
    let high = high_hasher.finish() as u128;

    let mut low_hasher = DefaultHasher::new();
    "vulcan-legacy-session-id-low".hash(&mut low_hasher);
    value.hash(&mut low_hasher);
    let low = low_hasher.finish() as u128;

    Uuid::from_u128((high << 64) | low)
}

/// Escapes one SQLite text literal used inside a migration script.
/// 转义迁移脚本中使用的一条 SQLite 文本字面量。
fn sqlite_escape_text(value: &str) -> String {
    value.replace('\'', "''")
}

/// Quotes one SQLite identifier after the caller has already constrained its allowed character set.
/// 在调用方已约束允许字符集后，为一条 SQLite 标识符添加引号。
fn quote_sqlite_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Builds the stable project-scoped controller FTS index name used for workspace-history search.
/// 构建工作台历史搜索使用的稳定项目级控制器 FTS 索引名。
fn workspace_history_search_project_index_name(project_id: &str) -> String {
    format!(
        "{WORKSPACE_HISTORY_SEARCH_PROJECT_INDEX_PREFIX}{:016x}",
        stable_workspace_history_search_hash(project_id)
    )
}

/// Builds the logical controller FTS file-path field used only for deterministic tie-breaking.
/// 构建仅用于稳定并列排序的逻辑控制器 FTS file_path 字段。
fn workspace_history_search_document_file_path(project_id: &str, session_id: &str) -> String {
    format!("project/{project_id}/session/{session_id}")
}

/// Returns one stable hash used to compress long workspace-history search routing identifiers.
/// 返回一个用于压缩长工作台历史搜索路由标识的稳定哈希值。
fn stable_workspace_history_search_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    "workspace-history-search".hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

/// Converts one persisted message-role string back into the session message kind.
/// 将一条持久化消息角色字符串转换回会话消息类型。
fn session_kind_from_role(role: &str) -> SessionMessageKind {
    match role {
        "user" => SessionMessageKind::User,
        "assistant" => SessionMessageKind::Assistant,
        "error" => SessionMessageKind::Error,
        _ => SessionMessageKind::Assistant,
    }
}

/// Returns the persisted text role used for one session message kind.
/// 返回某个会话消息类型使用的持久化文本角色。
fn role_from_session_kind(kind: SessionMessageKind) -> &'static str {
    match kind {
        SessionMessageKind::User => "user",
        SessionMessageKind::Assistant => "assistant",
        SessionMessageKind::Error => "error",
    }
}

/// Parses one persisted transcript-message metadata JSON payload into the structured helper shape.
/// 将一条持久化转录消息元数据 JSON 载荷解析为结构化辅助形状。
fn parse_session_message_meta_from_json(meta_json: &str) -> Option<StoredSessionMessageMeta> {
    serde_json::from_str(meta_json).ok()
}

/// Returns whether one persisted message row still carries the generated-media host-result metadata needed for transcript recovery.
/// 返回单条持久化消息行是否仍携带转录恢复所需的生成媒体宿主结果元数据。
fn stored_session_message_row_contains_generated_attachment_host_result(
    row: &StoredSessionMessageRow,
) -> bool {
    row.meta_json
        .as_deref()
        .and_then(parse_session_message_meta_from_json)
        .and_then(|meta| meta.metadata)
        .as_ref()
        .and_then(resolve_generated_attachment_host_result_from_metadata)
        .is_some()
}

/// Resolves one generated-media host-result payload from persisted transcript metadata, accepting nested and flattened shapes.
/// 从持久化转录元数据中解析一份生成媒体宿主结果载荷，同时兼容嵌套与扁平两种形状。
fn resolve_generated_attachment_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    let nested_host_result = metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("generated_attachments"));
    nested_host_result.or_else(|| {
        (metadata.get("kind").and_then(Value::as_str) == Some("generated_attachments")
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Serializes one transcript-message entry into the optional SQLite metadata JSON payload.
/// 将单条转录消息项序列化为可选的 SQLite 元数据 JSON 载荷。
fn serialize_session_message_meta_to_json(message: &SessionMessageEntry) -> Option<String> {
    let metadata = StoredSessionMessageMeta {
        title: (!message.title.trim().is_empty()).then(|| message.title.clone()),
        metadata: sanitize_session_message_metadata_for_persistence(message.metadata.clone()),
    };
    if metadata.title.is_none() && metadata.metadata.is_none() {
        None
    } else {
        serde_json::to_string(&metadata).ok()
    }
}

/// Normalizes a user-provided session-history search query before it reaches SQLite.
/// 在用户提供的会话历史搜索词进入 SQLite 前进行规范化。
fn normalize_session_history_search_query(query: &str) -> String {
    normalize_sqlite_fts_literal_terms(query).join(" ")
}

/// Collects literal SQLite FTS terms while discarding parser punctuation and operators.
/// 收集字面量 SQLite FTS 词项，同时丢弃解析器标点与操作符。
fn normalize_sqlite_fts_literal_terms(query: &str) -> Vec<String> {
    // Keep only tokenizer-friendly literal characters so pasted text like `50 TOPS (NPU)` cannot trip FTS syntax parsing.
    // 仅保留分词器友好的字面字符，这样 `50 TOPS (NPU)` 一类粘贴文本就不会触发 FTS 语法解析。
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in query.chars().take(SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS) {
        if sqlite_fts_query_char_is_literal(ch) {
            current.push(ch);
        } else {
            push_normalized_sqlite_fts_literal_term(&mut terms, &mut current);
        }
    }
    push_normalized_sqlite_fts_literal_term(&mut terms, &mut current);
    terms
}

/// Pushes one completed literal SQLite FTS term after downgrading reserved operators to plain text.
/// 在将保留操作符降级为普通文本后推入一条完整的字面量 SQLite FTS 词项。
fn push_normalized_sqlite_fts_literal_term(terms: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    let normalized_term = normalize_sqlite_fts_reserved_term(current);
    terms.push(normalized_term);
    current.clear();
}

/// Rewrites reserved SQLite FTS operator keywords into lowercase literals so user text stays searchable.
/// 将 SQLite FTS 保留操作符关键字改写为小写字面量，确保用户文本仍可被检索。
fn normalize_sqlite_fts_reserved_term(term: &str) -> String {
    match term {
        "AND" | "OR" | "NOT" | "NEAR" => term.to_ascii_lowercase(),
        _ => term.to_string(),
    }
}

/// Returns whether one character may stay inside a literal SQLite FTS token.
/// 返回某个字符是否可以保留在字面量 SQLite FTS 词元内部。
fn sqlite_fts_query_char_is_literal(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Builds an escaped SQLite LIKE pattern for a normalized session-history query.
/// 为规范化后的会话历史查询构建已转义的 SQLite LIKE 模式。
#[cfg(test)]
fn session_history_search_like_pattern(query: &str) -> String {
    format!("%{}%", escape_sqlite_like_pattern(query))
}

/// Escapes SQLite LIKE wildcard characters while preserving literal user text.
/// 转义 SQLite LIKE 通配符，同时保留用户文本的字面含义。
#[cfg(test)]
fn escape_sqlite_like_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Produces a compact search preview from one SQL-selected content fragment.
/// 从 SQL 选出的正文字段片段生成紧凑搜索预览。
#[cfg(test)]
fn session_history_search_preview(value: &str) -> String {
    let preview = truncate_preview_lines(value, 3, SESSION_HISTORY_SEARCH_PREVIEW_CHARS);
    if preview.trim().is_empty() {
        String::from("（该命中位于分片正文或元数据中，当前仅显示轻量定位信息。）")
    } else {
        preview
    }
}

/// Produces a compact workspace-history preview from one FTS snippet fragment.
/// 从一段 FTS snippet 片段生成紧凑的工作台历史预览。
fn workspace_history_search_preview(value: &str) -> String {
    let preview = truncate_preview_lines(
        &value.replace("<mark>", "").replace("</mark>", ""),
        3,
        SESSION_HISTORY_SEARCH_PREVIEW_CHARS,
    );
    if preview.trim().is_empty() {
        String::from("（该命中缺少可展示预览，但仍可定位到对应会话位置。）")
    } else {
        preview
    }
}

/// Preserves the original FTS preview markup so the UI can render highlighted spans without losing the fallback plain preview.
/// 保留原始 FTS 预览标记片段，便于 UI 渲染高亮片段，同时不丢失纯文本回退预览。
fn workspace_history_search_preview_markup(value: &str) -> String {
    if value.trim().is_empty() {
        String::from("（该命中缺少可展示预览，但仍可定位到对应会话位置。）")
    } else {
        value.to_string()
    }
}

/// Returns the inclusive turn-index bounds used for one search-result context load.
/// 返回单次搜索结果上下文加载使用的闭区间轮次边界。
#[cfg(test)]
fn session_history_context_turn_bounds(anchor_turn_index: i64, radius: i64) -> (i64, i64) {
    let safe_anchor = anchor_turn_index.max(0);
    let safe_radius = radius.max(0);
    (
        safe_anchor.saturating_sub(safe_radius).max(0),
        safe_anchor.saturating_add(safe_radius),
    )
}

/// Sanitizes transcript metadata before persistence so oversized host payloads do not dominate gRPC frames.
/// 持久化前清洗转录元数据，避免超大宿主载荷主导 gRPC 帧大小。
fn sanitize_session_message_metadata_for_persistence(metadata: Option<Value>) -> Option<Value> {
    let metadata = metadata?;
    let sanitized = strip_large_metadata_fields(metadata);
    if estimate_json_value_bytes(&sanitized) <= SESSION_MESSAGE_METADATA_BUDGET_BYTES {
        return Some(sanitized);
    }

    compact_metadata_to_essential_fields(&sanitized)
}

/// Removes known large host-result and argument payload fields while preserving display metadata.
/// 移除已知的大型宿主结果与参数载荷字段，同时保留展示所需元数据。
fn strip_large_metadata_fields(value: Value) -> Value {
    if let Some(preserved) = preserve_compactable_host_result(&value) {
        return preserved;
    }

    match value {
        Value::Object(mut object) => {
            for key in [
                "host_result",
                "payload",
                "raw_payload",
                "raw_result",
                "result_payload",
                "conversation_state",
                "provider_message_state",
            ] {
                if let Some(value) = object.get_mut(key) {
                    *value = preserve_or_compact_large_metadata_field(key, value.clone());
                }
            }
            for key in ["arguments", "input", "request", "response"] {
                if let Some(value) = object.get_mut(key) {
                    if estimate_json_value_bytes(value) > SESSION_MESSAGE_METADATA_BUDGET_BYTES / 4
                    {
                        *value = preserve_or_compact_large_metadata_field(key, value.clone());
                    }
                }
            }
            Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, strip_large_metadata_fields(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(strip_large_metadata_fields)
                .collect(),
        ),
        other => other,
    }
}

/// Preserves compact generated-media host payloads whenever they are needed for later transcript reconstruction.
/// 在后续转录重建确有需要时保留紧凑生成媒体宿主载荷。
fn preserve_or_compact_large_metadata_field(key: &str, value: Value) -> Value {
    preserve_compactable_host_result(&value)
        .filter(|_| key == "host_result")
        .unwrap_or_else(|| compact_large_metadata_field(key, value))
}

/// Preserves compact host-result payloads whose semantics must survive restart and transcript reconstruction.
/// 保留那些在重启与转录重建后仍必须保留语义的紧凑宿主结果载荷。
fn preserve_compactable_host_result(value: &Value) -> Option<Value> {
    preserve_generated_attachment_host_result(value)
        .or_else(|| preserve_change_set_host_result(value))
}

/// Preserves the minimal generated-attachments host-result shape required to rebuild inline media cards after restart.
/// 保留重启后重建内联媒体卡片所需的最小 generated-attachments 宿主结果结构。
fn preserve_generated_attachment_host_result(value: &Value) -> Option<Value> {
    let host_result = value.as_object()?;
    if host_result.get("kind").and_then(Value::as_str) != Some("generated_attachments") {
        return None;
    }

    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let compact_outputs = outputs
        .iter()
        .filter_map(compact_generated_attachment_output)
        .collect::<Vec<_>>();
    if compact_outputs.is_empty() {
        return None;
    }

    Some(serde_json::json!({
        "kind": "generated_attachments",
        "payload": {
            "outputs": compact_outputs,
        },
    }))
}

/// Preserves the minimal `change_set` host-result shape required to rebuild change statistics and file lists after restart without reviving stale filesystem verification warnings.
/// 保留重启后重建变更统计与文件清单所需的最小 `change_set` 宿主结果形态，并避免复活过期的文件系统校验警告。
fn preserve_change_set_host_result(value: &Value) -> Option<Value> {
    compact_change_set_host_result(value)
}

/// Compacts one generated attachment output down to the fields required by transcript rendering and direct host actions.
/// 将单条生成附件输出压缩到转录渲染与直接宿主动作所需的字段集合。
fn compact_generated_attachment_output(output: &Value) -> Option<Value> {
    let output_object = output.as_object()?;
    let kind = output_object
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let file_name = output_object
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let local_path = output_object
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let mut compact = serde_json::Map::new();
    compact.insert(String::from("kind"), Value::String(kind.to_string()));
    compact.insert(
        String::from("file_name"),
        Value::String(file_name.to_string()),
    );
    compact.insert(
        String::from("local_path"),
        Value::String(local_path.to_string()),
    );

    if let Some(mime_type) = output_object
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(
            String::from("mime_type"),
            Value::String(mime_type.to_string()),
        );
    }
    if let Some(byte_len) = output_object.get("byte_len").cloned() {
        compact.insert(String::from("byte_len"), byte_len);
    }
    if let Some(source_path) = output_object
        .get("source_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(
            String::from("source_path"),
            Value::String(source_path.to_string()),
        );
    }
    if let Some(preview_local_path) = output_object
        .get("preview_local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(
            String::from("preview_local_path"),
            Value::String(preview_local_path.to_string()),
        );
    }
    if let Some(asset_id) = output_object
        .get("asset_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(
            String::from("asset_id"),
            Value::String(asset_id.to_string()),
        );
    }

    Some(Value::Object(compact))
}

/// Compacts one oversized metadata field into a small audit-friendly placeholder.
/// 将一个超大元数据字段压缩为便于审计的小型占位结构。
fn compact_large_metadata_field(key: &str, value: Value) -> Value {
    let estimated_bytes = estimate_json_value_bytes(&value);
    let preview = match &value {
        Value::String(text) => {
            truncate_preview_lines(text, 4, SESSION_MESSAGE_METADATA_PREVIEW_CHARS)
        }
        _ => serde_json::to_string(&value)
            .ok()
            .map(|text| truncate_preview_lines(&text, 4, SESSION_MESSAGE_METADATA_PREVIEW_CHARS))
            .unwrap_or_default(),
    };

    serde_json::json!({
        "omitted": true,
        "field": key,
        "estimated_bytes": estimated_bytes,
        "preview": preview,
    })
}

/// Falls back to a small allow-list when metadata remains too large after structural pruning.
/// 当结构化裁剪后元数据仍过大时，回退到一个小型白名单。
fn compact_metadata_to_essential_fields(metadata: &Value) -> Option<Value> {
    let object = metadata.as_object()?;
    let mut compact = serde_json::Map::new();
    for key in [
        "tool_call_id",
        "tool_result_id",
        "tool_name",
        "name",
        "title",
        "display_title",
        "status",
        "execution_status",
        "run_state",
        "task_id",
        "session_id",
        "parent_session_id",
        "public_id",
    ] {
        if let Some(value) = object.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(host_result) = object
        .get("host_result")
        .and_then(preserve_compactable_host_result)
    {
        compact.insert(String::from("host_result"), host_result);
    }
    if compact.is_empty() {
        None
    } else {
        compact.insert(
            "metadata_omitted".to_string(),
            Value::String(String::from("oversized")),
        );
        Some(Value::Object(compact))
    }
}

/// Estimates a JSON value's serialized byte size for persistence-budget decisions.
/// 为持久化预算判断估算 JSON 值序列化后的字节大小。
fn estimate_json_value_bytes(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.len())
        .unwrap_or_default()
}

/// Parses one persisted transcript UI-state JSON payload into the typed runtime-state structure.
/// 将一条持久化转录界面状态 JSON 载荷解析为带类型的运行时状态结构。
fn parse_session_ui_state_from_json(value: &str) -> Option<SessionTranscriptUiState> {
    serde_json::from_str(value).ok()
}

/// Parses one persisted provider continuation JSON payload into the typed cursor structure.
/// 将一条持久化 Provider continuation JSON 载荷解析为带类型的游标结构。
fn parse_session_continuation_from_json(value: &str) -> Option<SessionProviderContinuationState> {
    serde_json::from_str(value).ok()
}

/// Parses one persisted canonical conversation-state JSON payload into the typed runtime structure.
/// 将一条持久化规范会话状态 JSON 载荷解析为带类型的运行时结构。
fn parse_session_conversation_state_from_json(
    value: Option<&str>,
) -> Option<SessionConversationState> {
    value.and_then(|payload| serde_json::from_str(payload).ok())
}

/// Parses one persisted hidden context-injection JSON payload into runtime state.
/// 将一条持久化隐藏上下文注入 JSON 载荷解析为运行态。
fn parse_context_injection_state_from_json(value: &str) -> Option<ContextInjectionState> {
    serde_json::from_str(value).ok()
}

/// Serializes one typed transcript UI-state structure into the persisted JSON payload when needed.
/// 在需要时将一条带类型的转录界面状态结构序列化为持久化 JSON 载荷。
fn serialize_session_ui_state_to_json(state: &SessionTranscriptUiState) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Serializes one typed provider continuation cursor into the persisted JSON payload when needed.
/// 在需要时将一条带类型的 Provider continuation 游标序列化为持久化 JSON 载荷。
fn serialize_session_continuation_to_json(
    continuation: &SessionProviderContinuationState,
) -> Option<String> {
    if continuation.provider_id.is_none()
        && continuation.model_id.is_none()
        && continuation.last_provider_message_id.is_none()
        && continuation.updated_at.is_none()
    {
        None
    } else {
        serde_json::to_string(continuation).ok()
    }
}

/// Serializes hidden context-injection runtime state into the persisted JSON payload when needed.
/// 在需要时将隐藏上下文注入运行态序列化为持久化 JSON 载荷。
fn serialize_context_injection_state_to_json(state: &ContextInjectionState) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Serializes session-level VMM overrides into the persisted JSON payload when needed.
/// 在需要时将会话级 VMM 覆盖序列化为持久化 JSON 载荷。
fn serialize_vmm_config_layer_to_json(layer: &VmmConfigLayer) -> Option<String> {
    if layer.is_empty() {
        None
    } else {
        serde_json::to_string(layer).ok()
    }
}

/// Parses persisted session-level VMM overrides from a JSON payload.
/// 从 JSON 载荷解析持久化的会话级 VMM 覆盖。
fn parse_vmm_config_layer_from_json(value: &str) -> Option<VmmConfigLayer> {
    serde_json::from_str(value).ok()
}

/// Serializes the VMM writeback outbox into the persisted JSON payload when needed.
/// 在需要时将 VMM 写回 outbox 序列化为持久化 JSON 载荷。
fn serialize_vmm_writeback_outbox_to_json(outbox: &[VmmWritebackOutboxEntry]) -> Option<String> {
    if outbox.is_empty() {
        None
    } else {
        serde_json::to_string(outbox).ok()
    }
}

/// Parses a persisted VMM writeback outbox JSON payload.
/// 解析持久化的 VMM 写回 outbox JSON 载荷。
fn parse_vmm_writeback_outbox_from_json(value: &str) -> Option<Vec<VmmWritebackOutboxEntry>> {
    serde_json::from_str(value).ok()
}

/// Serializes the VMM profile sync state into the persisted JSON payload when needed.
/// 在需要时将 VMM 画像同步状态序列化为持久化 JSON 载荷。
fn serialize_vmm_profile_sync_to_json(state: &VmmProfileSyncState) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Parses a persisted VMM profile sync JSON payload.
/// 解析持久化的 VMM 画像同步 JSON 载荷。
fn parse_vmm_profile_sync_from_json(value: &str) -> Option<VmmProfileSyncState> {
    serde_json::from_str(value).ok()
}

/// Serializes one typed canonical conversation state into the persisted JSON payload when it still carries messages or records.
/// 当规范会话状态仍携带消息或记录时，将其序列化为持久化 JSON 载荷。
fn serialize_session_conversation_state_to_json(
    conversation_state: &SessionConversationState,
) -> Option<String> {
    if conversation_state.messages.is_empty()
        && conversation_state.records.is_empty()
        && conversation_state.continuation.provider_id.is_none()
        && conversation_state.continuation.model_id.is_none()
        && conversation_state
            .continuation
            .last_provider_message_id
            .is_none()
        && conversation_state.continuation.updated_at.is_none()
    {
        None
    } else {
        serde_json::to_string(conversation_state).ok()
    }
}

/// Returns one default message title when no explicit title is stored in the metadata payload.
/// 当元数据载荷中未存储显式标题时返回默认消息标题。
fn default_message_title(role: &str) -> &'static str {
    match role {
        "user" => "用户指令",
        "assistant" => "AI 输出",
        "error" => "执行失败",
        _ => "运行消息",
    }
}

/// Builds one short focus preview from the latest visible transcript row.
/// 根据最近一条可见转录行构建一段简短聚焦预览。
fn session_focus_preview(messages: &[SessionMessageEntry]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.visible)
        .map(|message| truncate_preview_lines(&message.body, 1, 36))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("等待新的上下文与执行结果"))
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as i64
}

/// Returns the current Unix timestamp in milliseconds as one text value.
/// 以文本值形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    current_unix_millis().to_string()
}

/// Returns the provided timestamp text or falls back to the current time when it is empty.
/// 返回给定时间戳文本，若为空则回退到当前时间。
fn non_empty_timestamp_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        current_unix_millis_text()
    } else {
        trimmed.to_string()
    }
}

/// Converts one optional string into the nullable SQLite value expected by the controller SDK.
/// 将一个可选字符串转换为控制器 SDK 期望的可空 SQLite 值。
fn optional_sqlite_string(value: &Option<String>) -> ControllerSqliteValue {
    value
        .as_ref()
        .map(|value| ControllerSqliteValue::String(value.clone()))
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Converts one optional 64-bit integer into the nullable SQLite value expected by the controller SDK.
/// 将一个可选 64 位整数转换为控制器 SDK 期望的可空 SQLite 值。
fn optional_sqlite_i64(value: Option<i64>) -> ControllerSqliteValue {
    value
        .map(ControllerSqliteValue::Int64)
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Builds one relative-time label from one persisted Unix-milliseconds text value.
/// 根据一条持久化 Unix 毫秒文本值构建相对时间标签。
fn relative_time_label_from_unix_millis_text(value: &str) -> String {
    let parsed = value
        .parse::<i64>()
        .unwrap_or_else(|_| current_unix_millis());
    relative_time_label_from_unix_millis(parsed)
}

/// Builds one relative-time label from one Unix-milliseconds timestamp.
/// 根据一条 Unix 毫秒时间戳构建相对时间标签。
fn relative_time_label_from_unix_millis(timestamp_millis: i64) -> String {
    let now = current_unix_millis();
    let delta_millis = now.saturating_sub(timestamp_millis).max(0);
    let delta_secs = delta_millis / 1_000;

    if delta_secs < 60 {
        return String::from("刚刚");
    }

    let delta_minutes = delta_secs / 60;
    if delta_minutes < 60 {
        return format!("{delta_minutes}分钟前");
    }

    let delta_hours = delta_minutes / 60;
    if delta_hours < 24 {
        return format!("{delta_hours}小时前");
    }

    let delta_days = delta_hours / 24;
    if delta_days < 7 {
        return format!("{delta_days}天前");
    }

    if delta_days < 30 {
        return format!("{}周前", (delta_days / 7).max(1));
    }

    format!("{}月前", (delta_days / 30).max(1))
}

/// Approximates one Unix-milliseconds timestamp from the compact relative label stored by the UI.
/// 根据界面存储的紧凑相对标签近似还原一条 Unix 毫秒时间戳。
fn approximate_unix_millis_from_relative_label(label: &str) -> Option<i64> {
    if label.trim().is_empty() {
        return None;
    }

    let now = current_unix_millis();
    if label == "刚刚" {
        return Some(now);
    }

    let number_text = label
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let amount = number_text.parse::<i64>().ok()?;

    let millis = if label.contains("分钟前") {
        amount * 60 * 1_000
    } else if label.contains("小时前") {
        amount * 60 * 60 * 1_000
    } else if label.contains("天前") {
        amount * 24 * 60 * 60 * 1_000
    } else if label.contains("周前") {
        amount * 7 * 24 * 60 * 60 * 1_000
    } else if label.contains("月前") {
        amount * 30 * 24 * 60 * 60 * 1_000
    } else {
        0
    };

    Some(now.saturating_sub(millis))
}

/// Truncates one multi-line preview into a single compact snippet used by persistence-derived summaries.
/// 将一段多行预览截断为单条紧凑片段，供基于持久化的摘要使用。
fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
    let joined = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(max_lines)
        .collect::<Vec<_>>()
        .join(" / ");
    let normalized = if joined.is_empty() {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        joined
    };

    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let truncated = normalized
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests;

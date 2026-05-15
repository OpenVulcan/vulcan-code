//! Controller-backed persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的控制器持久化辅助模块。

use crate::conversation_protocol::{
    SessionConversationState, SessionProviderContinuationState,
    conversation_state_from_session_messages,
};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::{
    SessionCompactionEntry, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, SessionTranscriptUiState, TaskProjectSummary,
};
use crate::provider_usage::{parse_provider_usage_from_json, serialize_provider_usage_to_json};
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
/// First transcript turn-window size used when opening a session lazily.
/// 懒加载打开会话时使用的首屏转录轮次数。
pub const SESSION_TRANSCRIPT_WINDOW_LIMIT: usize = 30;
/// Tool-detail page size used when a specific turn asks for its tool rows.
/// 指定轮次请求工具明细行时使用的分页大小。
pub const SESSION_TOOL_DETAIL_PAGE_LIMIT: usize = 100;
/// Search-result page size used by the lightweight per-session history search.
/// 单会话轻量历史搜索使用的搜索结果页大小。
pub const SESSION_HISTORY_SEARCH_RESULT_LIMIT: usize = 20;
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
    /// Latest loaded turn index used to lazily hydrate recent tool details.
    /// 用于懒加载最近工具明细的已加载最新轮次索引。
    pub latest_turn_index: Option<i64>,
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
    /// Session identifier owning this tool-detail page.
    /// 拥有该工具明细页的会话标识。
    pub session_id: SessionId,
    /// Conversation turn whose tool details were loaded.
    /// 本次加载工具明细所属的对话轮次。
    pub turn_index: i64,
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
    /// Whether the legacy full conversation-state column was cleared successfully.
    /// 旧版完整会话状态列是否已成功清理。
    pub cleared_runtime_state_json: bool,
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
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
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

/// Lightweight chunk-presence row used before loading chunk content.
/// 加载分片内容前用于判断分片是否存在的轻量行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionMessageChunkPresenceRow {
    /// Stable message identifier that has at least one chunk.
    /// 至少拥有一个分片的稳定消息标识。
    message_id: String,
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
    /// Optional rebuilt context start message identifier.
    /// 可选的新上下文起始消息标识。
    context_start_message_id: Option<String>,
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
    /// Optional persisted lightweight provider continuation payload.
    /// 可选的持久化轻量 Provider continuation 载荷。
    #[serde(default)]
    continuation_json: Option<String>,
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
        let entry = SessionRuntimeStateEntry {
            active_agent_id: row.active_agent_id,
            active_provider_id: row.active_provider_id,
            active_model_id: row.active_model_id,
            last_compaction_id: row.last_compaction_id,
            context_start_message_id: row.context_start_message_id,
            ui_state: row
                .ui_state_json
                .as_deref()
                .and_then(parse_session_ui_state_from_json)
                .unwrap_or_default(),
            conversation_state: conversation_state_with_continuation(row.continuation_json),
            last_provider_usage: row
                .provider_usage_json
                .as_deref()
                .and_then(parse_provider_usage_from_json),
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
        .with_title_source(SessionTitleSource::from_persisted_key(&row.title_source));

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

    let expanded_task_projects = task_projects
        .iter()
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

/// Lazily loads the recent transcript window and lightweight runtime state for one session.
/// 为单个会话懒加载最近转录窗口与轻量运行时状态。
pub async fn load_session_hydration_snapshot(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    migrate_legacy_oversized_session_messages(&controller, session_id).await?;

    let requested_limit = requested_limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_LIMIT);
    let (message_rows, effective_limit) = query_session_message_window_rows_with_retry(
        &controller,
        session_id,
        None,
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let earliest_ordinal = message_rows.first().map(|row| row.ordinal);
    let latest_turn_index = message_rows.last().map(|row| row.turn_index);
    let has_more_history = loaded_message_turn_count(&message_rows) >= effective_limit
        && earliest_ordinal.is_some_and(|ordinal| ordinal > 0);
    let message_body_chunks =
        query_session_message_body_chunks_for_rows(&controller, session_id, &message_rows).await?;
    let messages = message_rows
        .into_iter()
        .map(|row| stored_session_message_row_to_entry(row, &message_body_chunks))
        .collect::<Vec<_>>();
    let compactions = query_session_compaction_rows_for_session(&controller, session_id)
        .await?
        .into_iter()
        .map(stored_compaction_row_to_entry)
        .collect::<Vec<_>>();
    let runtime_state = query_session_runtime_state_shell_row(&controller, session_id)
        .await?
        .map(stored_runtime_state_row_to_entry)
        .map(|runtime_state| runtime_state_with_recent_records(runtime_state, &messages))
        .or_else(|| {
            (!messages.is_empty()).then(|| SessionRuntimeStateEntry {
                conversation_state: conversation_state_from_session_messages(&messages),
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            })
        });

    Ok(SessionHydrationSnapshot {
        session_id,
        messages,
        compactions,
        runtime_state,
        earliest_ordinal,
        has_more_history,
        latest_turn_index,
    })
}

/// Lazily loads one older transcript page before the provided ordinal cursor.
/// 在给定序号游标之前懒加载一页更早的转录历史。
pub async fn load_session_history_page(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: i64,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    migrate_legacy_oversized_session_messages(&controller, session_id).await?;

    let requested_limit = requested_limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_LIMIT);
    let (message_rows, effective_limit) = query_session_message_window_rows_with_retry(
        &controller,
        session_id,
        Some(before_ordinal),
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let earliest_ordinal = message_rows.first().map(|row| row.ordinal);
    let has_more_history = loaded_message_turn_count(&message_rows) >= effective_limit
        && earliest_ordinal.is_some_and(|ordinal| ordinal > 0);
    let message_body_chunks =
        query_session_message_body_chunks_for_rows(&controller, session_id, &message_rows).await?;
    let messages = message_rows
        .into_iter()
        .map(|row| stored_session_message_row_to_entry(row, &message_body_chunks))
        .collect::<Vec<_>>();

    Ok(SessionHistoryPage {
        session_id,
        messages,
        earliest_ordinal,
        has_more_history,
    })
}

/// Searches visible workspace history with SQLite FTS5 and BM25 scoring.
/// 使用 SQLite FTS5 与 BM25 评分搜索可见工作台历史。
pub async fn search_workspace_history(
    controller: ControllerRuntimeHandle,
    project_id: Option<String>,
    query: String,
    requested_limit: usize,
    offset: usize,
) -> Result<WorkspaceHistorySearchPage, String> {
    let query = normalize_session_history_search_query(&query);
    if query.is_empty() {
        return Ok(WorkspaceHistorySearchPage {
            query,
            hits: Vec::new(),
            limit_reached: false,
            total_hits: 0,
            next_offset: 0,
            has_more: false,
        });
    }

    let limit = requested_limit
        .max(1)
        .min(SESSION_HISTORY_SEARCH_RESULT_LIMIT);
    let scoped_project_id = project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let index_name = scoped_project_id
        .map(workspace_history_search_project_index_name)
        .unwrap_or_else(|| WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME.to_string());
    ensure_workspace_history_search_index(&controller, &index_name).await?;
    let search_result = controller
        .client
        .search_sqlite_fts(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            index_name.clone(),
            ControllerSqliteTokenizerMode::Jieba,
            query.clone(),
            limit as u32,
            offset as u32,
        )
        .await
        .map_err(|error| format!("搜索工作台历史失败：{error}"))?;
    let message_ids = search_result
        .hits
        .iter()
        .map(|hit| hit.id.clone())
        .collect::<Vec<_>>();
    let metadata_rows =
        query_workspace_history_search_metadata_rows(&controller, &message_ids).await?;
    let metadata_by_message_id = metadata_rows
        .into_iter()
        .map(|row| (row.message_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let hits = search_result
        .hits
        .into_iter()
        .filter_map(|hit| {
            metadata_by_message_id
                .get(&hit.id)
                .map(|row| stored_workspace_history_search_hit(row, &hit))
        })
        .collect::<Vec<_>>();

    Ok(WorkspaceHistorySearchPage {
        query,
        total_hits: search_result.total as usize,
        next_offset: offset.saturating_add(hits.len()),
        has_more: offset.saturating_add(hits.len()) < search_result.total as usize,
        hits,
        limit_reached: search_result.total >= limit as u64,
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
    migrate_legacy_oversized_session_messages(&controller, session_id).await?;

    let requested_limit = requested_limit.max(1).min(SESSION_TOOL_DETAIL_PAGE_LIMIT);
    let (message_rows, effective_limit) = query_session_tool_detail_rows_with_retry(
        &controller,
        session_id,
        turn_index,
        before_ordinal,
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let earliest_ordinal = message_rows.first().map(|row| row.ordinal);
    let has_more_detail = message_rows.len() >= effective_limit
        && earliest_ordinal.is_some_and(|ordinal| ordinal > 0);
    let message_body_chunks =
        query_session_message_body_chunks_for_rows(&controller, session_id, &message_rows).await?;
    let messages = message_rows
        .into_iter()
        .map(|row| stored_session_message_row_to_entry(row, &message_body_chunks))
        .collect::<Vec<_>>();

    Ok(SessionToolDetailPage {
        session_id,
        turn_index,
        messages,
        earliest_ordinal,
        has_more_detail,
    })
}

/// Maintains the storage shape for one already hydrated session without scanning unrelated history.
/// 在不扫描无关历史的前提下维护单个已水合会话的存储形态。
pub async fn maintain_hydrated_session_storage(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    messages: Vec<SessionMessageEntry>,
) -> Result<SessionMaintenanceReport, String> {
    let legacy_chunk_migration =
        migrate_legacy_oversized_session_messages(&controller, session_id).await?;
    clear_session_runtime_conversation_state_json(&controller, session_id).await?;

    let upserted_message_count = messages.len();
    let mut message_batch_params = Vec::new();
    let mut chunk_batch_params = Vec::new();
    let mut search_batch_params = Vec::new();
    let Some((project_id, session_title)) =
        query_session_workspace_history_search_routing(&controller, session_id).await?
    else {
        return Err(format!(
            "维护已水合会话存储失败：未找到会话 `{session_id}` 的项目归属。"
        ));
    };
    let persisted_positions =
        query_session_message_positions_for_entries(&controller, session_id, &messages).await?;
    let (max_ordinal, max_turn_index) =
        query_session_message_max_position(&controller, session_id).await?;
    push_session_message_rows_for_session(
        &persisted_positions,
        max_ordinal,
        max_turn_index,
        &mut message_batch_params,
        &mut chunk_batch_params,
        &mut search_batch_params,
        project_id.as_str(),
        true,
        session_id,
        session_title.as_str(),
        &messages,
    );
    upsert_session_message_batches(
        &controller,
        message_batch_params,
        chunk_batch_params,
        search_batch_params,
    )
    .await?;

    Ok(SessionMaintenanceReport {
        session_id,
        upserted_message_count,
        cleared_runtime_state_json: true,
        legacy_chunk_migration,
    })
}

/// Maintains legacy storage shape for one session without hydrating transcript messages.
/// 在不水合转录消息的前提下维护单个会话的旧存储形态。
pub async fn maintain_legacy_session_storage(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<SessionMaintenanceReport, String> {
    let legacy_chunk_migration =
        migrate_legacy_oversized_session_messages(&controller, session_id).await?;
    clear_session_runtime_conversation_state_json(&controller, session_id).await?;

    Ok(SessionMaintenanceReport {
        session_id,
        upserted_message_count: 0,
        cleared_runtime_state_json: true,
        legacy_chunk_migration,
    })
}

/// Migrates legacy oversized inline message bodies into chunk rows without returning full bodies to Rust.
/// 在不向 Rust 返回完整正文的前提下，将旧版超大内联消息正文迁移为分片行。
async fn migrate_legacy_oversized_session_messages(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<SessionLegacyChunkMigrationReport, String> {
    let mut report = SessionLegacyChunkMigrationReport {
        session_id,
        ..SessionLegacyChunkMigrationReport::default()
    };

    while report.migrated_message_count < SESSION_LEGACY_INLINE_MIGRATION_MAX_MESSAGES_PER_PASS {
        let remaining_limit = SESSION_LEGACY_INLINE_MIGRATION_MAX_MESSAGES_PER_PASS
            .saturating_sub(report.migrated_message_count)
            .min(SESSION_LEGACY_INLINE_MIGRATION_CANDIDATE_LIMIT);
        let candidates =
            query_legacy_oversized_message_candidates(controller, session_id, remaining_limit)
                .await?;
        if candidates.is_empty() {
            break;
        }

        let candidate_count = candidates.len();
        for candidate in candidates {
            let chunk_count = legacy_inline_message_chunk_count(candidate.content_chars)?;
            if chunk_count > SESSION_LEGACY_INLINE_MIGRATION_MAX_CHUNKS_PER_MESSAGE {
                return Err(format!(
                    "旧版超大消息分片数量超出安全上限：session_id={session_id}, message_id={}, content_bytes={}, chunks={}, limit={}",
                    candidate.id,
                    candidate.content_bytes,
                    chunk_count,
                    SESSION_LEGACY_INLINE_MIGRATION_MAX_CHUNKS_PER_MESSAGE
                ));
            }
            let script = legacy_inline_message_chunk_migration_script(session_id, &candidate.id);
            controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    app_sqlite_binding_id(controller),
                    script,
                    Vec::<ControllerSqliteValue>::new(),
                )
                .await
                .map_err(|error| {
                    format!(
                        "迁移旧版超大内联消息失败：session_id={session_id}, message_id={}, error={error}",
                        candidate.id
                    )
                })?;
            report.migrated_message_count += 1;
            report.inserted_chunk_count = report.inserted_chunk_count.saturating_add(chunk_count);
        }

        if candidate_count < remaining_limit {
            break;
        }
    }

    Ok(report)
}

/// Queries legacy oversized inline-message candidates without selecting their full content.
/// 在不选择完整正文的前提下查询旧版超大内联消息候选。
async fn query_legacy_oversized_message_candidates(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    limit: usize,
) -> Result<Vec<StoredLegacyOversizedMessageCandidateRow>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            legacy_oversized_message_candidate_query(),
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES as i64),
                ControllerSqliteValue::Int64(
                    limit.min(SESSION_LEGACY_INLINE_MIGRATION_CANDIDATE_LIMIT) as i64,
                ),
            ],
        )
        .await
        .map_err(|error| format!("查询旧版超大内联消息候选失败：{error}"))?;

    decode_query_rows::<StoredLegacyOversizedMessageCandidateRow>(&result.json_data)
}

/// Returns the lightweight candidate query used by legacy inline-message migration.
/// 返回旧版内联消息迁移使用的轻量候选查询。
fn legacy_oversized_message_candidate_query() -> &'static str {
    r#"
SELECT
    m.id,
    length(CAST(m.content AS BLOB)) AS content_bytes,
    length(m.content) AS content_chars
FROM session_messages AS m
WHERE m.session_id = ?1
  AND length(CAST(m.content AS BLOB)) > ?2
  AND NOT EXISTS (
      SELECT 1
      FROM session_message_chunks AS c
      WHERE c.message_id = m.id
      LIMIT 1
  )
ORDER BY m.ordinal DESC
LIMIT ?3;
"#
}

/// Builds one SQLite-side migration script for a legacy oversized inline message.
/// 为单条旧版超大内联消息构建 SQLite 侧迁移脚本。
fn legacy_inline_message_chunk_migration_script(session_id: SessionId, message_id: &str) -> String {
    let escaped_session_id = sqlite_escape_text(&session_id.to_string());
    let escaped_message_id = sqlite_escape_text(message_id);
    format!(
        r#"
BEGIN IMMEDIATE TRANSACTION;

WITH
digits(n) AS (
    VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
),
numbers(n) AS (
    SELECT d0.n + d1.n * 10 + d2.n * 100 + d3.n * 1000 + d4.n * 10000
    FROM digits AS d0
    CROSS JOIN digits AS d1
    CROSS JOIN digits AS d2
    CROSS JOIN digits AS d3
    CROSS JOIN digits AS d4
),
source(content_text, content_chars) AS (
    SELECT content, length(content)
    FROM session_messages
    WHERE session_id = '{session_id}'
      AND id = '{message_id}'
      AND length(CAST(content AS BLOB)) > {inline_budget_bytes}
      AND NOT EXISTS (
          SELECT 1
          FROM session_message_chunks
          WHERE message_id = '{message_id}'
          LIMIT 1
      )
)
INSERT OR REPLACE INTO session_message_chunks (
    session_id,
    message_id,
    chunk_index,
    content
)
SELECT
    '{session_id}',
    '{message_id}',
    numbers.n,
    substr(source.content_text, numbers.n * {chunk_chars} + 1, {chunk_chars})
FROM source
JOIN numbers ON numbers.n * {chunk_chars} < source.content_chars
ORDER BY numbers.n;

UPDATE session_messages
SET content = substr(content, 1, {preview_chars})
WHERE session_id = '{session_id}'
  AND id = '{message_id}'
  AND length(CAST(content AS BLOB)) > {inline_budget_bytes}
  AND EXISTS (
      SELECT 1
      FROM session_message_chunks
      WHERE message_id = '{message_id}'
      LIMIT 1
  );

COMMIT;
"#,
        session_id = escaped_session_id,
        message_id = escaped_message_id,
        chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS,
        inline_budget_bytes = SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES,
        preview_chars = SESSION_MESSAGE_CHUNK_PREVIEW_CHARS
    )
}

/// Estimates how many chunk rows a legacy inline message will produce from its character length.
/// 根据旧版内联消息的字符长度估算其会生成多少分片行。
fn legacy_inline_message_chunk_count(content_chars: i64) -> Result<usize, String> {
    if content_chars <= 0 {
        return Ok(0);
    }

    let content_chars = content_chars as u128;
    let chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS.max(1) as u128;
    let chunk_count = content_chars.div_ceil(chunk_chars);
    if chunk_count > usize::MAX as u128 {
        return Err(format!("旧版超大消息字符长度无法安全转换：{content_chars}"));
    }

    Ok(chunk_count as usize)
}

/// Persists the current visible workspace snapshot back into the controller-backed SQLite database.
/// 将当前可见工作台快照回写到控制器承载的 SQLite 数据库。
pub async fn persist_workspace_snapshot(
    controller: ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    let existing_active_project_ids = query_active_project_ids(&controller).await?;
    let existing_active_session_ids = query_active_session_ids(&controller).await?;
    let visible_project_ids = snapshot
        .task_projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let visible_session_ids = snapshot
        .sessions
        .iter()
        .map(|session| session.id.to_string())
        .collect::<BTreeSet<_>>();
    let all_projects = snapshot
        .task_projects
        .iter()
        .cloned()
        .map(|project| (project, false))
        .chain(
            snapshot
                .archived_task_projects
                .iter()
                .cloned()
                .map(|project| (project, true)),
        )
        .collect::<Vec<_>>();
    let all_sessions = snapshot
        .sessions
        .iter()
        .cloned()
        .map(|session| (session, false))
        .chain(
            snapshot
                .archived_sessions
                .iter()
                .cloned()
                .map(|session| (session, true)),
        )
        .collect::<Vec<_>>();
    let all_task_projects = snapshot
        .task_projects
        .iter()
        .cloned()
        .chain(snapshot.archived_task_projects.iter().cloned())
        .collect::<Vec<_>>();

    upsert_projects(&controller, &all_projects).await?;
    upsert_sessions(&controller, &all_task_projects, &all_sessions).await?;
    archive_missing_projects(
        &controller,
        &existing_active_project_ids,
        &visible_project_ids,
    )
    .await?;
    archive_missing_sessions(
        &controller,
        &existing_active_session_ids,
        &visible_session_ids,
    )
    .await?;
    replace_session_messages(
        &controller,
        &snapshot.sessions,
        &snapshot.session_messages,
        &snapshot.archived_sessions,
        &snapshot.archived_session_messages,
    )
    .await?;
    replace_session_compactions(
        &controller,
        &snapshot.sessions,
        &snapshot.session_compactions,
        &snapshot.archived_sessions,
        &snapshot.archived_session_compactions,
    )
    .await?;
    replace_session_runtime_state(
        &controller,
        &snapshot.sessions,
        &snapshot.session_runtime_state,
        &snapshot.archived_sessions,
        &snapshot.archived_session_runtime_state,
        &snapshot.hydrated_session_ids,
    )
    .await
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
DELETE FROM session_runtime_state;
DELETE FROM session_compactions;
DELETE FROM session_message_chunks;
DELETE FROM session_messages;
DELETE FROM sessions;
DELETE FROM projects;
"#,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("清理旧原型工作台数据失败：{error}"))?;

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
SELECT id, name, root_path, pinned, sort_order, archived
FROM projects
ORDER BY pinned DESC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
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

/// Queries one bounded transcript window for a single session, ordered newest-first at SQL level.
/// 查询单个会话的一段有界转录窗口，SQL 层按新到旧排序。
async fn query_session_message_window_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let session_id_text = session_id.to_string();
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_LIMIT) as i64;
    let (statement, params) = if let Some(before_ordinal) = before_ordinal {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
INNER JOIN (
    SELECT DISTINCT turn_index
    FROM session_messages
    WHERE session_id = ?1 AND ordinal < ?2
    ORDER BY turn_index DESC
    LIMIT ?3
) AS selected_turns ON selected_turns.turn_index = m.turn_index
WHERE m.session_id = ?1 AND m.ordinal < ?2 AND m.kind NOT IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(before_ordinal),
                ControllerSqliteValue::Int64(limit),
                ControllerSqliteValue::Int64(
                    session_transcript_row_limit_for_turns(limit as usize) as i64,
                ),
            ],
        )
    } else {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
INNER JOIN (
    SELECT DISTINCT turn_index
    FROM session_messages
    WHERE session_id = ?1
    ORDER BY turn_index DESC
    LIMIT ?2
) AS selected_turns ON selected_turns.turn_index = m.turn_index
WHERE m.session_id = ?1 AND m.kind NOT IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(limit),
                ControllerSqliteValue::Int64(
                    session_transcript_row_limit_for_turns(limit as usize) as i64,
                ),
            ],
        )
    };

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话转录窗口失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
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

/// Counts distinct persisted turn indexes in one loaded message window.
/// 统计一个已加载消息窗口中的不同持久化轮次索引数量。
fn loaded_message_turn_count(message_rows: &[StoredSessionMessageRow]) -> usize {
    message_rows
        .iter()
        .map(|row| row.turn_index)
        .collect::<BTreeSet<_>>()
        .len()
}

/// Queries workspace-history metadata rows for a bounded list of matched message identifiers.
/// 为一组有界命中的消息标识查询工作台历史元数据行。
async fn query_workspace_history_search_metadata_rows(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchMetadataRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (1..=message_ids.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT
    p.name AS project_name,
    s.id AS session_id,
    s.title AS session_title,
    m.id AS message_id,
    m.ordinal,
    m.turn_index,
    m.role,
    m.created_at
FROM session_messages AS m
INNER JOIN sessions AS s ON s.id = m.session_id
INNER JOIN projects AS p ON p.id = s.project_id
WHERE m.id IN ({placeholders})
  AND s.archived = 0
  AND p.archived = 0;
"#
    );
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            message_ids
                .iter()
                .cloned()
                .map(ControllerSqliteValue::String)
                .collect(),
        )
        .await
        .map_err(|error| format!("查询工作台历史搜索元数据失败：{error}"))?;

    decode_query_rows::<StoredWorkspaceHistorySearchMetadataRow>(&result.json_data)
}

/// Queries and rebuilds chunked message bodies for one already-bounded transcript window.
/// 查询并重建一个已限界转录窗口中的分片消息正文。
async fn query_session_message_body_chunks_for_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_rows: &[StoredSessionMessageRow],
) -> Result<BTreeMap<String, String>, String> {
    if message_rows.is_empty() {
        return Ok(BTreeMap::new());
    }

    let chunked_message_ids =
        query_chunked_message_ids_for_window(controller, session_id, message_rows).await?;
    let mut rebuilt_bodies = BTreeMap::new();
    for message_id in chunked_message_ids {
        let chunk_rows =
            query_session_message_chunk_rows_for_message(controller, session_id, &message_id)
                .await?;
        let rebuilt_body = rebuild_session_message_body_from_chunks(&message_id, chunk_rows)?;
        rebuilt_bodies.insert(message_id, rebuilt_body);
    }

    Ok(rebuilt_bodies)
}

/// Queries the subset of one transcript window that has external body chunks.
/// 查询一个转录窗口中拥有外部分片正文的消息子集。
async fn query_chunked_message_ids_for_window(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_rows: &[StoredSessionMessageRow],
) -> Result<Vec<String>, String> {
    let message_ids = message_rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<Vec<_>>();
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (2..message_ids.len() + 2)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT DISTINCT message_id
FROM session_message_chunks
WHERE session_id = ?1 AND message_id IN ({placeholders})
ORDER BY message_id ASC;
"#
    );
    let mut params = Vec::with_capacity(message_ids.len() + 1);
    params.push(ControllerSqliteValue::String(session_id.to_string()));
    params.extend(
        message_ids
            .into_iter()
            .map(|message_id| ControllerSqliteValue::String(message_id.to_string())),
    );

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话消息分片索引失败：{error}"))?;

    Ok(
        decode_query_rows::<StoredSessionMessageChunkPresenceRow>(&result.json_data)?
            .into_iter()
            .map(|row| row.message_id)
            .collect(),
    )
}

/// Queries chunk rows for one oversized message in small pages to stay below gRPC response limits.
/// 以小分页查询单条超大消息的分片行，避免超过 gRPC 响应限制。
async fn query_session_message_chunk_rows_for_message(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
) -> Result<Vec<StoredSessionMessageChunkRow>, String> {
    let mut rows = Vec::new();
    let mut last_chunk_index = -1_i64;
    let mut effective_limit = SESSION_MESSAGE_CHUNK_QUERY_LIMIT.max(1);

    loop {
        match query_session_message_chunk_rows_after_index(
            controller,
            session_id,
            message_id,
            last_chunk_index,
            effective_limit,
        )
        .await
        {
            Ok(page) => {
                if page.is_empty() {
                    break;
                }
                last_chunk_index = page
                    .last()
                    .map(|row| row.chunk_index)
                    .unwrap_or(last_chunk_index);
                let page_len = page.len();
                rows.extend(page);
                if page_len < effective_limit {
                    break;
                }
            }
            Err(error) if effective_limit > 1 && is_grpc_message_frame_size_error(&error) => {
                effective_limit = (effective_limit / 2).max(1);
            }
            Err(error) => return Err(error),
        }
    }

    Ok(rows)
}

/// Queries one bounded chunk page after the provided chunk-index cursor.
/// 查询给定分片序号游标之后的一段有限分片页。
async fn query_session_message_chunk_rows_after_index(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
    after_chunk_index: i64,
    limit: usize,
) -> Result<Vec<StoredSessionMessageChunkRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT message_id, chunk_index, content
FROM session_message_chunks
WHERE session_id = ?1 AND message_id = ?2 AND chunk_index > ?3
ORDER BY chunk_index ASC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::String(message_id.to_string()),
                ControllerSqliteValue::Int64(after_chunk_index),
                ControllerSqliteValue::Int64(limit.max(1) as i64),
            ],
        )
        .await
        .map_err(|error| format!("查询会话消息分片内容失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageChunkRow>(&result.json_data)
}

/// Queries hidden compaction checkpoint rows for one session.
/// 查询单个会话对应的隐藏压缩检查点行。
async fn query_session_compaction_rows_for_session(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Vec<StoredCompactionRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT c.id, c.source_until_message_id, c.context_start_message_id, c.summary_message_id, c.created_at
FROM session_compactions AS c
WHERE c.session_id = ?1
ORDER BY c.created_at ASC;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话压缩节点失败：{error}"))?;

    decode_query_rows::<StoredCompactionRow>(&result.json_data)
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
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.provider_usage_json, r.continuation_json, r.updated_at
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

/// Queries lightweight runtime-state metadata for one session without the large canonical conversation JSON.
/// 查询单个会话不含大型规范会话 JSON 的轻量运行时状态元数据。
async fn query_session_runtime_state_shell_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredRuntimeStateRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.provider_usage_json, r.continuation_json, r.updated_at
FROM session_runtime_state AS r
WHERE r.session_id = ?1
LIMIT 1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话运行时状态失败：{error}"))?;

    Ok(
        decode_query_rows::<StoredRuntimeStateRow>(&result.json_data)?
            .into_iter()
            .next(),
    )
}

/// Queries the current project ownership and title needed to route one session into workspace-history search indexes.
/// 查询将某个会话路由到工作台历史搜索索引所需的当前项目归属与标题。
async fn query_session_workspace_history_search_routing(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<(String, String)>, String> {
    #[derive(Debug, Deserialize)]
    struct SessionRoutingRow {
        /// Stable project identifier owning the session.
        /// 拥有该会话的稳定项目标识。
        project_id: String,
        /// Human-readable session title.
        /// 人类可读的会话标题。
        title: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT project_id, title FROM sessions WHERE id = ?1 LIMIT 1;",
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话搜索索引归属失败：{error}"))?;

    Ok(decode_query_rows::<SessionRoutingRow>(&result.json_data)?
        .into_iter()
        .next()
        .map(|row| (row.project_id, row.title)))
}

/// Queries one transcript window with progressive limit reduction when gRPC frame limits are hit.
/// 查询单段转录窗口，并在触发 gRPC 帧限制时逐步缩小 limit 重试。
async fn query_session_message_window_rows_with_retry(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<(Vec<StoredSessionMessageRow>, usize), String> {
    let mut effective_limit = requested_limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_LIMIT);

    loop {
        match query_session_message_window_rows(
            controller,
            session_id,
            before_ordinal,
            effective_limit,
        )
        .await
        {
            Ok(rows) => return Ok((rows, effective_limit)),
            Err(error) if effective_limit > 1 && is_grpc_message_frame_size_error(&error) => {
                effective_limit = (effective_limit / 2).max(1);
            }
            Err(error) => return Err(error),
        }
    }
}

/// Queries one tool-detail window with progressive limit reduction when gRPC frame limits are hit.
/// 查询单段工具明细窗口，并在触发 gRPC 帧限制时逐步缩小 limit 重试。
async fn query_session_tool_detail_rows_with_retry(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<(Vec<StoredSessionMessageRow>, usize), String> {
    let mut effective_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT);

    loop {
        match query_session_tool_detail_rows(
            controller,
            session_id,
            turn_index,
            before_ordinal,
            effective_limit,
        )
        .await
        {
            Ok(rows) => return Ok((rows, effective_limit)),
            Err(error) if effective_limit > 1 && is_grpc_message_frame_size_error(&error) => {
                effective_limit = (effective_limit / 2).max(1);
            }
            Err(error) => return Err(error),
        }
    }
}

/// Queries one bounded tool-detail window for a single turn, ordered newest-first at SQL level.
/// 查询单个轮次的一段有界工具明细窗口，SQL 层按新到旧排序。
async fn query_session_tool_detail_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let session_id_text = session_id.to_string();
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT) as i64;
    let (statement, params) = if let Some(before_ordinal) = before_ordinal {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index = ?2
  AND m.ordinal < ?3
  AND m.kind IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(turn_index),
                ControllerSqliteValue::Int64(before_ordinal),
                ControllerSqliteValue::Int64(limit),
            ],
        )
    } else {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index = ?2
  AND m.kind IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(turn_index),
                ControllerSqliteValue::Int64(limit),
            ],
        )
    };

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话工具明细窗口失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
}

/// Returns whether one controller error looks like tonic's protobuf message-size guard.
/// 判断一条控制器错误是否类似 tonic 的 protobuf 消息大小保护。
fn is_grpc_message_frame_size_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("decoded message length too large")
        || normalized.contains("message length too large")
        || normalized.contains("limit is: 4194304")
}

/// Upserts all visible project rows while clearing the archive flag.
/// 在清理归档标记的同时上写全部可见项目行。
async fn upsert_projects(
    controller: &ControllerRuntimeHandle,
    task_projects: &[(TaskProjectSummary, bool)],
) -> Result<(), String> {
    if task_projects.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = task_projects
        .iter()
        .map(|(project, archived)| {
            vec![
                ControllerSqliteValue::String(project.id.clone()),
                ControllerSqliteValue::String(project.title.clone()),
                ControllerSqliteValue::String(project.path.clone()),
                ControllerSqliteValue::Int64(i64::from(project.pinned)),
                ControllerSqliteValue::Int64(i64::from(*archived)),
                ControllerSqliteValue::Int64(project.order as i64),
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(now.clone()),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO projects (
    id,
    name,
    root_path,
    pinned,
    archived,
    sort_order,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    root_path = excluded.root_path,
    pinned = excluded.pinned,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            params,
        )
        .await
        .map_err(|error| format!("写入项目列表失败：{error}"))?;

    Ok(())
}

/// Soft-archives active projects missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活项目软归档。
async fn archive_missing_projects(
    controller: &ControllerRuntimeHandle,
    existing_active_ids: &BTreeSet<String>,
    visible_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let missing_ids = existing_active_ids
        .iter()
        .filter(|project_id| !visible_ids.contains(*project_id))
        .cloned()
        .collect::<Vec<_>>();

    if missing_ids.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = missing_ids
        .into_iter()
        .map(|project_id| {
            vec![
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE projects SET archived = 1, updated_at = ?1 WHERE id = ?2;",
            params,
        )
        .await
        .map_err(|error| format!("软归档项目失败：{error}"))?;

    Ok(())
}

/// Upserts all visible session rows using the current project-to-session bindings.
/// 使用当前项目到会话的绑定关系上写全部可见会话行。
async fn upsert_sessions(
    controller: &ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    sessions: &[(SessionSummary, bool)],
) -> Result<(), String> {
    if sessions.is_empty() {
        return Ok(());
    }

    let session_owner_map = task_projects
        .iter()
        .flat_map(|project| {
            project
                .session_ids
                .iter()
                .enumerate()
                .map(move |(index, session_id)| (*session_id, (project.id.clone(), index)))
        })
        .collect::<BTreeMap<SessionId, (String, usize)>>();
    let now = current_unix_millis();
    let mut batch_params = Vec::new();

    for (session, archived) in sessions {
        if let Some((project_id, sort_order)) = session_owner_map.get(&session.id) {
            let updated_at =
                approximate_unix_millis_from_relative_label(&session.last_updated_label)
                    .unwrap_or(now)
                    .to_string();
            batch_params.push(vec![
                ControllerSqliteValue::String(session.id.to_string()),
                ControllerSqliteValue::String(project_id.clone()),
                ControllerSqliteValue::String(session.title.clone()),
                ControllerSqliteValue::String(session.title_source.as_persisted_key().to_string()),
                ControllerSqliteValue::Int64(i64::from(*archived)),
                ControllerSqliteValue::Int64(*sort_order as i64),
                ControllerSqliteValue::String(now.to_string()),
                ControllerSqliteValue::String(updated_at),
            ]);
        }
    }

    if batch_params.is_empty() {
        return Ok(());
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO sessions (
    id,
    project_id,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(id) DO UPDATE SET
    project_id = excluded.project_id,
    title = excluded.title,
    title_source = excluded.title_source,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            batch_params,
        )
        .await
        .map_err(|error| format!("写入会话列表失败：{error}"))?;

    Ok(())
}

/// Soft-archives active sessions missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活会话软归档。
async fn archive_missing_sessions(
    controller: &ControllerRuntimeHandle,
    existing_active_ids: &BTreeSet<String>,
    visible_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let missing_ids = existing_active_ids
        .iter()
        .filter(|session_id| !visible_ids.contains(*session_id))
        .cloned()
        .collect::<Vec<_>>();

    if missing_ids.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = missing_ids
        .iter()
        .cloned()
        .map(|session_id| {
            vec![
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(session_id),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE sessions SET archived = 1, updated_at = ?1 WHERE id = ?2;",
            params,
        )
        .await
        .map_err(|error| format!("软归档会话失败：{error}"))?;
    delete_workspace_history_search_documents_for_session_ids(
        controller,
        &missing_ids,
        "删除已软归档会话的搜索索引失败",
    )
    .await?;

    Ok(())
}

/// Permanently deletes one archived session together with all persisted transcript and runtime artifacts.
/// 彻底删除一个已归档会话及其全部已持久化的转录与运行时工件。
pub async fn permanently_delete_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let session_ids = vec![session_id_text.clone()];

    delete_session_scoped_rows(
        &controller,
        "session_message_chunks",
        &session_ids,
        "删除归档会话转录消息分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_session_ids(
        &controller,
        &session_ids,
        "删除归档会话搜索索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_messages",
        &session_ids,
        "删除归档会话转录消息失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_compactions",
        &session_ids,
        "删除归档会话压缩检查点失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_runtime_state",
        &session_ids,
        "删除归档会话运行时状态失败",
    )
    .await?;

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "DELETE FROM sessions WHERE id = ?1 AND archived != 0;",
            vec![vec![ControllerSqliteValue::String(session_id_text)]],
        )
        .await
        .map_err(|error| format!("删除归档会话记录失败：{error}"))?;

    Ok(())
}

/// Permanently deletes one archived project together with all child archived sessions and persisted artifacts.
/// 彻底删除一个已归档项目及其全部子归档会话与已持久化工件。
pub async fn permanently_delete_archived_project(
    controller: ControllerRuntimeHandle,
    project_id: String,
    session_ids: Vec<SessionId>,
) -> Result<(), String> {
    let project_index_name = workspace_history_search_project_index_name(&project_id);
    let session_id_texts = session_ids
        .into_iter()
        .map(|session_id| session_id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        &controller,
        "session_message_chunks",
        &session_id_texts,
        "删除归档项目下的转录消息分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_session_ids(
        &controller,
        &session_id_texts,
        "删除归档项目下的搜索索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_messages",
        &session_id_texts,
        "删除归档项目下的转录消息失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_compactions",
        &session_id_texts,
        "删除归档项目下的压缩检查点失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_runtime_state",
        &session_id_texts,
        "删除归档项目下的运行时状态失败",
    )
    .await?;

    if !session_id_texts.is_empty() {
        let session_delete_params = session_id_texts
            .into_iter()
            .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
            .collect::<Vec<_>>();
        controller
            .client
            .execute_sqlite_batch_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(&controller),
                "DELETE FROM sessions WHERE id = ?1 AND archived != 0;",
                session_delete_params,
            )
            .await
            .map_err(|error| format!("删除归档项目下的会话记录失败：{error}"))?;
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "DELETE FROM projects WHERE id = ?1 AND archived != 0;",
            vec![vec![ControllerSqliteValue::String(project_id)]],
        )
        .await
        .map_err(|error| format!("删除归档项目记录失败：{error}"))?;
    drop_workspace_history_search_indexes(
        &controller,
        &[project_index_name],
        "删除归档项目对应的搜索索引失败",
    )
    .await?;

    Ok(())
}

/// Upserts loaded transcript message rows while leaving not-yet-loaded history intact.
/// 在保留尚未加载历史的前提下上写已加载的转录消息行。
async fn replace_session_messages(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
) -> Result<(), String> {
    let mut message_batch_params = Vec::new();
    let mut chunk_batch_params = Vec::new();
    let mut search_batch_params = Vec::new();
    push_session_message_batch_rows(
        controller,
        &mut message_batch_params,
        &mut chunk_batch_params,
        &mut search_batch_params,
        true,
        sessions,
        session_messages,
    )
    .await?;
    push_session_message_batch_rows(
        controller,
        &mut message_batch_params,
        &mut chunk_batch_params,
        &mut search_batch_params,
        false,
        archived_sessions,
        archived_session_messages,
    )
    .await?;

    upsert_session_message_batches(
        controller,
        message_batch_params,
        chunk_batch_params,
        search_batch_params,
    )
    .await
}

/// Upserts prepared transcript rows and rewrites only their own chunk rows.
/// 上写已预处理的转录行，并且只重写这些消息自身的分片行。
async fn upsert_session_message_batches(
    controller: &ControllerRuntimeHandle,
    message_batch_params: Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: Vec<WorkspaceHistorySearchDocumentUpsert>,
) -> Result<(), String> {
    if message_batch_params.is_empty() {
        return Ok(());
    }

    let message_ids = message_batch_params
        .iter()
        .filter_map(|row| match row.first() {
            Some(ControllerSqliteValue::String(message_id)) => Some(message_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    delete_session_message_chunks_for_message_ids(
        controller,
        &message_ids,
        "删除当前窗口消息的旧正文分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_message_ids(
        controller,
        &message_ids,
        "删除当前窗口消息的旧搜索索引失败",
    )
    .await?;

    execute_sqlite_batch_safely(
        controller,
        "session_messages",
        session_message_upsert_statement(),
        message_batch_params,
        "写入会话转录消息失败",
    )
    .await?;

    if !chunk_batch_params.is_empty() {
        execute_sqlite_batch_safely(
            controller,
            "session_message_chunks",
            session_message_chunk_upsert_statement(),
            chunk_batch_params,
            "写入会话转录消息分片失败",
        )
        .await?;
    }

    upsert_workspace_history_search_documents(controller, search_batch_params).await?;

    Ok(())
}

/// Returns the upsert SQL used to persist chunk rows safely across overlapping maintenance tasks.
/// 返回用于在重叠维护任务间安全持久化分片行的 upsert SQL。
fn session_message_chunk_upsert_statement() -> &'static str {
    r#"
INSERT INTO session_message_chunks (
    session_id,
    message_id,
    chunk_index,
    content
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(message_id, chunk_index) DO UPDATE SET
    session_id = excluded.session_id,
    content = excluded.content;
"#
}

/// Replaces all compaction checkpoint rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部压缩检查点行。
async fn replace_session_compactions(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
        .filter(|session| {
            session_compactions.contains_key(&session.id)
                || archived_session_compactions.contains_key(&session.id)
        })
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        controller,
        "session_compactions",
        &active_session_ids,
        "删除当前可见会话的旧压缩节点失败",
    )
    .await?;

    let mut batch_params = Vec::new();
    push_session_compaction_batch_rows(&mut batch_params, sessions, session_compactions);
    push_session_compaction_batch_rows(
        &mut batch_params,
        archived_sessions,
        archived_session_compactions,
    );

    if batch_params.is_empty() {
        return Ok(());
    }

    execute_sqlite_batch_safely(
        controller,
        "session_compactions",
        r#"
INSERT INTO session_compactions (
    id,
    session_id,
    source_until_message_id,
    context_start_message_id,
    summary_message_id,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
"#,
        batch_params,
        "写入会话压缩节点失败",
    )
    .await?;

    Ok(())
}

/// Replaces all runtime-state rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部运行时状态行。
async fn replace_session_runtime_state(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    hydrated_session_ids: &BTreeSet<SessionId>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
        .filter(|session| hydrated_session_ids.contains(&session.id))
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        controller,
        "session_runtime_state",
        &active_session_ids,
        "删除当前可见会话的旧运行时状态失败",
    )
    .await?;

    let mut batch_params = Vec::new();
    push_session_runtime_state_batch_rows(&mut batch_params, sessions, session_runtime_state);
    push_session_runtime_state_batch_rows(
        &mut batch_params,
        archived_sessions,
        archived_session_runtime_state,
    );

    if batch_params.is_empty() {
        return Ok(());
    }

    execute_sqlite_batch_safely(
        controller,
        "session_runtime_state",
        r#"
INSERT INTO session_runtime_state (
    session_id,
    active_agent_id,
    active_provider_id,
    active_model_id,
    last_compaction_id,
    context_start_message_id,
    ui_state_json,
    provider_usage_json,
    conversation_state_json,
    continuation_json,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11);
"#,
        batch_params,
        "写入会话运行时状态失败",
    )
    .await?;

    Ok(())
}

/// Queries existing ordinals and turn indexes for loaded entries so partial upserts keep their positions.
/// 查询已加载消息已有的序号与轮次索引，确保局部上写保留原位置。
async fn query_session_message_positions_for_entries(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<BTreeMap<String, StoredSessionMessagePositionRow>, String> {
    let message_ids = messages
        .iter()
        .filter_map(|message| {
            let message_id = message.id.trim();
            (!message_id.is_empty()).then(|| message_id.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut positions = BTreeMap::new();
    for chunk in message_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let chunk_positions =
            query_session_message_positions_for_ids(controller, session_id, chunk).await?;
        positions.extend(
            chunk_positions
                .into_iter()
                .map(|position| (position.id.clone(), position)),
        );
    }

    Ok(positions)
}

/// Queries existing persisted positions for a bounded set of message identifiers.
/// 查询一组有界消息标识对应的已持久化位置。
async fn query_session_message_positions_for_ids(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_ids: &[String],
) -> Result<Vec<StoredSessionMessagePositionRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (2..message_ids.len() + 2)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT id, ordinal, turn_index
FROM session_messages
WHERE session_id = ?1 AND id IN ({placeholders})
ORDER BY ordinal ASC;
"#
    );
    let mut params = Vec::with_capacity(message_ids.len() + 1);
    params.push(ControllerSqliteValue::String(session_id.to_string()));
    params.extend(
        message_ids
            .iter()
            .cloned()
            .map(ControllerSqliteValue::String),
    );

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话消息持久化位置失败：{error}"))?;

    decode_query_rows::<StoredSessionMessagePositionRow>(&result.json_data)
}

/// Queries the maximum persisted ordinal and turn index for one session without loading bodies.
/// 在不加载正文的前提下查询单个会话已持久化的最大序号与轮次索引。
async fn query_session_message_max_position(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(i64, i64), String> {
    #[derive(Deserialize)]
    struct MaxPositionRow {
        /// Maximum persisted ordinal or -1 when the session has no rows.
        /// 最大已持久化序号；会话没有行时为 -1。
        max_ordinal: i64,
        /// Maximum persisted turn index or -1 when the session has no rows.
        /// 最大已持久化轮次索引；会话没有行时为 -1。
        max_turn_index: i64,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT
    COALESCE(MAX(ordinal), -1) AS max_ordinal,
    COALESCE(MAX(turn_index), -1) AS max_turn_index
FROM session_messages
WHERE session_id = ?1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话消息最大位置失败：{error}"))?;

    let rows = decode_query_rows::<MaxPositionRow>(&result.json_data)?;
    let row = rows.into_iter().next().unwrap_or(MaxPositionRow {
        max_ordinal: -1,
        max_turn_index: -1,
    });
    Ok((row.max_ordinal, row.max_turn_index))
}

/// Pushes transcript-message batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的转录消息批量行推入共享 SQLite 批处理载荷。
async fn push_session_message_batch_rows(
    controller: &ControllerRuntimeHandle,
    message_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: &mut Vec<WorkspaceHistorySearchDocumentUpsert>,
    index_search_documents: bool,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
) -> Result<(), String> {
    for session in sessions {
        if let Some(messages) = session_messages.get(&session.id) {
            let persisted_positions =
                query_session_message_positions_for_entries(controller, session.id, messages)
                    .await?;
            let (max_ordinal, max_turn_index) =
                query_session_message_max_position(controller, session.id).await?;
            let project_id = session.project_id.as_deref().unwrap_or_default();
            push_session_message_rows_for_session(
                &persisted_positions,
                max_ordinal,
                max_turn_index,
                message_batch_params,
                chunk_batch_params,
                search_batch_params,
                project_id,
                index_search_documents && !project_id.trim().is_empty(),
                session.id,
                session.title.as_str(),
                messages,
            );
        }
    }

    Ok(())
}

/// Pushes transcript-message batch rows for one concrete session identifier.
/// 为一个明确的会话标识推入转录消息批处理行。
fn push_session_message_rows_for_session(
    persisted_positions: &BTreeMap<String, StoredSessionMessagePositionRow>,
    max_persisted_ordinal: i64,
    max_persisted_turn_index: i64,
    message_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: &mut Vec<WorkspaceHistorySearchDocumentUpsert>,
    project_id: &str,
    index_search_documents: bool,
    session_id: SessionId,
    _session_title: &str,
    messages: &[SessionMessageEntry],
) {
    let mut next_new_ordinal = max_persisted_ordinal.saturating_add(1).max(0);
    let mut current_turn_index = max_persisted_turn_index.max(0);
    let mut has_prior_turn_context = max_persisted_turn_index >= 0;
    for (local_index, message) in messages.iter().enumerate() {
        let meta_json = serialize_session_message_meta_to_json(message);
        let message_id = if message.id.trim().is_empty() {
            format!("session-{}-message-{}", session_id, local_index)
        } else {
            message.id.clone()
        };
        let (ordinal, turn_index) = if let Some(position) = persisted_positions.get(&message_id) {
            current_turn_index = position.turn_index.max(0);
            has_prior_turn_context = true;
            (position.ordinal, position.turn_index)
        } else {
            if session_message_starts_new_turn(message) && has_prior_turn_context {
                current_turn_index = current_turn_index.saturating_add(1);
            }
            has_prior_turn_context = true;
            let ordinal = next_new_ordinal;
            next_new_ordinal = next_new_ordinal.saturating_add(1);
            (ordinal, current_turn_index)
        };
        let prepared_body = prepare_session_message_body_for_storage(&message.body);

        message_batch_params.push(vec![
            ControllerSqliteValue::String(message_id.clone()),
            ControllerSqliteValue::String(session_id.to_string()),
            ControllerSqliteValue::Int64(ordinal),
            ControllerSqliteValue::String(role_from_session_kind(message.kind).to_string()),
            ControllerSqliteValue::String(message.record_kind.as_persisted_key().to_string()),
            ControllerSqliteValue::Int64(i64::from(message.visible)),
            ControllerSqliteValue::Int64(turn_index),
            ControllerSqliteValue::String(prepared_body.inline_content),
            optional_sqlite_string(&meta_json),
            ControllerSqliteValue::String(non_empty_timestamp_text(&message.created_at)),
        ]);

        chunk_batch_params.extend(prepared_body.chunks.into_iter().enumerate().map(
            |(chunk_index, chunk)| {
                vec![
                    ControllerSqliteValue::String(session_id.to_string()),
                    ControllerSqliteValue::String(message_id.clone()),
                    ControllerSqliteValue::Int64(chunk_index as i64),
                    ControllerSqliteValue::String(chunk),
                ]
            },
        ));

        if index_search_documents
            && session_message_is_workspace_history_search_indexable(message)
            && !message.body.trim().is_empty()
        {
            search_batch_params.push(WorkspaceHistorySearchDocumentUpsert {
                message_id,
                session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                content: message.body.clone(),
            });
        }
    }
}

/// Returns the upsert SQL used to persist loaded transcript rows without deleting unloaded history.
/// 返回用于持久化已加载转录行且不删除未加载历史的 upsert SQL。
fn session_message_upsert_statement() -> &'static str {
    r#"
INSERT INTO session_messages (
    id,
    session_id,
    ordinal,
    role,
    kind,
    visible,
    turn_index,
    content,
    meta_json,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(id) DO UPDATE SET
    session_id = excluded.session_id,
    ordinal = excluded.ordinal,
    role = excluded.role,
    kind = excluded.kind,
    visible = excluded.visible,
    turn_index = excluded.turn_index,
    content = excluded.content,
    meta_json = excluded.meta_json,
    created_at = excluded.created_at;
"#
}

/// Returns the upsert SQL used to persist workspace-history document ownership rows.
/// 返回用于持久化工作台历史文档归属状态行的 upsert SQL。
fn workspace_history_search_state_upsert_statement() -> &'static str {
    r#"
INSERT INTO workspace_history_search_docs (
    message_id,
    session_id,
    project_id
) VALUES (?1, ?2, ?3)
ON CONFLICT(message_id) DO UPDATE SET
    session_id = excluded.session_id,
    project_id = excluded.project_id;
"#
}

/// Returns the insert SQL used to persist controller-managed workspace-history FTS documents.
/// 返回用于持久化控制器托管工作台历史 FTS 文档的插入 SQL。
fn workspace_history_search_index_insert_statement(index_name: &str) -> String {
    format!(
        r#"
INSERT INTO {} (
    id,
    file_path,
    title,
    content
) VALUES (?1, ?2, ?3, ?4);
"#,
        quote_sqlite_identifier(index_name)
    )
}

/// Returns the delete SQL used to remove controller-managed workspace-history FTS documents by id.
/// 返回用于按文档 ID 删除控制器托管工作台历史 FTS 文档的 SQL。
fn workspace_history_search_index_delete_statement(index_name: &str) -> String {
    format!(
        "DELETE FROM {} WHERE id = ?1;",
        quote_sqlite_identifier(index_name)
    )
}

/// Assigns stable turn indexes to one ordered transcript slice.
/// 为一段有序转录消息分配稳定轮次索引。
#[cfg(test)]
fn assign_session_message_turn_indexes(messages: &[SessionMessageEntry]) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(messages.len());
    let mut current_turn_index = 0_i64;
    let mut has_seen_user_turn = false;

    for message in messages {
        if session_message_starts_new_turn(message) {
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

/// Returns whether one transcript message should open a new user-driven dialogue turn.
/// 返回一条转录消息是否应开启新的用户驱动对话轮次。
fn session_message_starts_new_turn(message: &SessionMessageEntry) -> bool {
    matches!(message.kind, SessionMessageKind::User)
        && message.visible
        && message.contributes_to_context()
}

/// Returns whether one visible transcript message should participate in workspace BM25 search indexing.
/// 返回某条可见转录消息是否应参与工作台 BM25 搜索索引。
fn session_message_is_workspace_history_search_indexable(message: &SessionMessageEntry) -> bool {
    matches!(
        (message.kind, message.record_kind, message.visible),
        (
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            true
        ) | (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            true,
        )
    )
}

/// Prepared persistence payload for one transcript message body.
/// 单条转录消息正文的预处理持久化载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedSessionMessageBody {
    /// Inline content stored directly on the lightweight `session_messages` row.
    /// 直接存储在轻量 `session_messages` 行上的内联内容。
    inline_content: String,
    /// External body chunks stored in `session_message_chunks` when the body is oversized.
    /// 正文过大时存储在 `session_message_chunks` 中的外部分片。
    chunks: Vec<String>,
}

/// Prepares one message body for inline storage or chunked external storage.
/// 将单条消息正文预处理为内联存储或外部分片存储。
fn prepare_session_message_body_for_storage(body: &str) -> PreparedSessionMessageBody {
    if body.len() <= SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES {
        return PreparedSessionMessageBody {
            inline_content: body.to_string(),
            chunks: Vec::new(),
        };
    }

    PreparedSessionMessageBody {
        inline_content: truncate_preview_lines(body, 4, SESSION_MESSAGE_CHUNK_PREVIEW_CHARS),
        chunks: split_text_into_byte_chunks(body, SESSION_MESSAGE_CHUNK_CONTENT_BYTES),
    }
}

/// Splits UTF-8 text into chunks that do not exceed the requested byte limit.
/// 将 UTF-8 文本拆分为不超过指定字节上限的分片。
fn split_text_into_byte_chunks(value: &str, max_bytes: usize) -> Vec<String> {
    let max_bytes = max_bytes.max(4);
    let mut chunks = Vec::new();
    let mut chunk_start = 0usize;

    for (char_start, ch) in value.char_indices() {
        let char_end = char_start + ch.len_utf8();
        if char_end.saturating_sub(chunk_start) > max_bytes && char_start > chunk_start {
            chunks.push(value[chunk_start..char_start].to_string());
            chunk_start = char_start;
        }
    }

    if chunk_start < value.len() {
        chunks.push(value[chunk_start..].to_string());
    }

    chunks
}

/// Pushes compaction-checkpoint batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的压缩检查点批量行推入共享 SQLite 批处理载荷。
fn push_session_compaction_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
) {
    for session in sessions {
        if let Some(compactions) = session_compactions.get(&session.id) {
            for entry in compactions {
                batch_params.push(vec![
                    ControllerSqliteValue::String(entry.id.clone()),
                    ControllerSqliteValue::String(session.id.to_string()),
                    optional_sqlite_string(&entry.source_until_message_id),
                    optional_sqlite_string(&entry.context_start_message_id),
                    optional_sqlite_string(&entry.summary_message_id),
                    ControllerSqliteValue::String(non_empty_timestamp_text(&entry.created_at)),
                ]);
            }
        }
    }
}

/// Pushes runtime-state batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的运行时状态批量行推入共享 SQLite 批处理载荷。
fn push_session_runtime_state_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
) {
    for session in sessions {
        if let Some(entry) = session_runtime_state.get(&session.id) {
            batch_params.push(vec![
                ControllerSqliteValue::String(session.id.to_string()),
                optional_sqlite_string(&entry.active_agent_id),
                optional_sqlite_string(&entry.active_provider_id),
                optional_sqlite_string(&entry.active_model_id),
                optional_sqlite_string(&entry.last_compaction_id),
                optional_sqlite_string(&entry.context_start_message_id),
                optional_sqlite_string(&serialize_session_ui_state_to_json(&entry.ui_state)),
                optional_sqlite_string(&serialize_provider_usage_to_json(
                    entry.last_provider_usage.as_ref(),
                )),
                ControllerSqliteValue::Null,
                optional_sqlite_string(&serialize_session_continuation_to_json(
                    &entry.conversation_state.continuation,
                )),
                ControllerSqliteValue::String(non_empty_timestamp_text(&entry.updated_at)),
            ]);
        }
    }
}

/// Deletes all rows from one session-scoped table for the provided active session identifiers.
/// 删除某个会话作用域表中指定激活会话标识对应的全部行。
async fn delete_session_scoped_rows(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if session_ids.is_empty() {
        return Ok(());
    }

    let statement = format!("DELETE FROM {table_name} WHERE session_id = ?1;");
    let params = session_ids
        .iter()
        .cloned()
        .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
        .collect::<Vec<_>>();

    execute_sqlite_batch_safely(controller, table_name, &statement, params, error_prefix).await?;

    Ok(())
}

/// Clears the legacy full conversation-state JSON for one hydrated session without reading it first.
/// 在不预读旧大字段的前提下清理单个已水合会话的旧完整会话状态 JSON。
async fn clear_session_runtime_conversation_state_json(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    execute_sqlite_batch_safely(
        controller,
        "session_runtime_state",
        session_runtime_conversation_state_cleanup_statement(),
        vec![vec![ControllerSqliteValue::String(session_id.to_string())]],
        "清理旧会话运行时完整状态失败",
    )
    .await
}

/// Returns the SQL used to null out legacy full conversation-state payloads after hydration.
/// 返回会话水合后用于清空旧完整会话状态载荷的 SQL。
fn session_runtime_conversation_state_cleanup_statement() -> &'static str {
    "UPDATE session_runtime_state SET conversation_state_json = NULL WHERE session_id = ?1;"
}

/// Deletes chunk rows for the provided message identifiers before those messages are upserted.
/// 在对应消息被上写前，删除这些消息标识关联的正文分片行。
async fn delete_session_message_chunks_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if message_ids.is_empty() {
        return Ok(());
    }

    let params = message_ids
        .iter()
        .cloned()
        .map(|message_id| vec![ControllerSqliteValue::String(message_id)])
        .collect::<Vec<_>>();

    execute_sqlite_batch_safely(
        controller,
        "session_message_chunks",
        "DELETE FROM session_message_chunks WHERE message_id = ?1;",
        params,
        error_prefix,
    )
    .await?;

    Ok(())
}

/// Upserts controller-managed workspace-history FTS documents together with their ownership state rows.
/// 上写控制器托管工作台历史 FTS 文档，并同步更新对应的归属状态行。
async fn upsert_workspace_history_search_documents(
    controller: &ControllerRuntimeHandle,
    documents: Vec<WorkspaceHistorySearchDocumentUpsert>,
) -> Result<(), String> {
    if documents.is_empty() {
        return Ok(());
    }

    ensure_workspace_history_search_index(controller, WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME)
        .await?;
    let mut project_index_rows = BTreeMap::<String, Vec<Vec<ControllerSqliteValue>>>::new();
    let mut global_index_rows = Vec::with_capacity(documents.len());
    let mut state_rows = Vec::with_capacity(documents.len());
    let mut ensured_project_indexes = BTreeSet::new();

    for document in documents {
        let project_index_name = workspace_history_search_project_index_name(&document.project_id);
        if ensured_project_indexes.insert(project_index_name.clone()) {
            ensure_workspace_history_search_index(controller, &project_index_name).await?;
        }

        let file_path =
            workspace_history_search_document_file_path(&document.project_id, &document.session_id);
        let index_row = vec![
            ControllerSqliteValue::String(document.message_id.clone()),
            ControllerSqliteValue::String(file_path),
            ControllerSqliteValue::String(String::new()),
            ControllerSqliteValue::String(document.content.clone()),
        ];
        global_index_rows.push(index_row.clone());
        project_index_rows
            .entry(project_index_name)
            .or_default()
            .push(index_row);
        state_rows.push(vec![
            ControllerSqliteValue::String(document.message_id),
            ControllerSqliteValue::String(document.session_id),
            ControllerSqliteValue::String(document.project_id),
        ]);
    }

    execute_sqlite_batch_safely(
        controller,
        WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        &workspace_history_search_index_insert_statement(
            WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        ),
        global_index_rows,
        "写入全局工作台历史搜索索引失败",
    )
    .await?;
    for (index_name, params) in project_index_rows {
        execute_sqlite_batch_safely(
            controller,
            &index_name,
            &workspace_history_search_index_insert_statement(&index_name),
            params,
            "写入项目工作台历史搜索索引失败",
        )
        .await?;
    }
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        workspace_history_search_state_upsert_statement(),
        state_rows,
        "写入工作台历史搜索归属状态失败",
    )
    .await?;

    Ok(())
}

/// Deletes controller-managed workspace-history FTS documents for the provided message identifiers.
/// 删除这些消息标识关联的控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    let state_rows =
        query_workspace_history_search_document_state_for_message_ids(controller, message_ids)
            .await?;
    delete_workspace_history_search_documents_from_rows(controller, &state_rows, error_prefix)
        .await?;
    delete_workspace_history_search_document_state_for_message_ids(
        controller,
        message_ids,
        error_prefix,
    )
    .await
}

/// Deletes controller-managed workspace-history FTS documents for the provided session identifiers.
/// 删除这些会话标识关联的控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    let state_rows =
        query_workspace_history_search_document_state_for_session_ids(controller, session_ids)
            .await?;
    delete_workspace_history_search_documents_from_rows(controller, &state_rows, error_prefix)
        .await?;
    delete_workspace_history_search_document_state_for_session_ids(
        controller,
        session_ids,
        error_prefix,
    )
    .await
}

/// Queries workspace-history document ownership rows for the provided message identifiers.
/// 查询给定消息标识对应的工作台历史文档归属状态行。
async fn query_workspace_history_search_document_state_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchDocumentStateRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    for chunk in message_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let placeholders = (1..=chunk.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            "SELECT message_id, session_id, project_id FROM workspace_history_search_docs WHERE message_id IN ({placeholders});"
        );
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                statement,
                chunk
                    .iter()
                    .cloned()
                    .map(ControllerSqliteValue::String)
                    .collect(),
            )
            .await
            .map_err(|error| format!("查询工作台历史搜索文档归属失败：{error}"))?;
        rows.extend(decode_query_rows::<
            StoredWorkspaceHistorySearchDocumentStateRow,
        >(&result.json_data)?);
    }

    Ok(rows)
}

/// Queries workspace-history document ownership rows for the provided session identifiers.
/// 查询给定会话标识对应的工作台历史文档归属状态行。
async fn query_workspace_history_search_document_state_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchDocumentStateRow>, String> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    for chunk in session_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let placeholders = (1..=chunk.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            "SELECT message_id, session_id, project_id FROM workspace_history_search_docs WHERE session_id IN ({placeholders});"
        );
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                statement,
                chunk
                    .iter()
                    .cloned()
                    .map(ControllerSqliteValue::String)
                    .collect(),
            )
            .await
            .map_err(|error| format!("查询工作台历史搜索会话归属失败：{error}"))?;
        rows.extend(decode_query_rows::<
            StoredWorkspaceHistorySearchDocumentStateRow,
        >(&result.json_data)?);
    }

    Ok(rows)
}

/// Deletes controller-managed workspace-history FTS documents using preloaded ownership rows.
/// 基于预加载归属状态行删除控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_from_rows(
    controller: &ControllerRuntimeHandle,
    state_rows: &[StoredWorkspaceHistorySearchDocumentStateRow],
    error_prefix: &str,
) -> Result<(), String> {
    if state_rows.is_empty() {
        return Ok(());
    }

    let global_params = state_rows
        .iter()
        .map(|row| vec![ControllerSqliteValue::String(row.message_id.clone())])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        &workspace_history_search_index_delete_statement(
            WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        ),
        global_params,
        error_prefix,
    )
    .await?;

    let mut project_params = BTreeMap::<String, Vec<Vec<ControllerSqliteValue>>>::new();
    for row in state_rows {
        project_params
            .entry(workspace_history_search_project_index_name(&row.project_id))
            .or_default()
            .push(vec![ControllerSqliteValue::String(row.message_id.clone())]);
    }
    for (index_name, params) in project_params {
        execute_sqlite_batch_safely(
            controller,
            &index_name,
            &workspace_history_search_index_delete_statement(&index_name),
            params,
            error_prefix,
        )
        .await?;
    }

    Ok(())
}

/// Deletes workspace-history document ownership rows for the provided message identifiers.
/// 删除给定消息标识对应的工作台历史文档归属状态行。
async fn delete_workspace_history_search_document_state_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if message_ids.is_empty() {
        return Ok(());
    }

    let params = message_ids
        .iter()
        .cloned()
        .map(|message_id| vec![ControllerSqliteValue::String(message_id)])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        "DELETE FROM workspace_history_search_docs WHERE message_id = ?1;",
        params,
        error_prefix,
    )
    .await
}

/// Deletes workspace-history document ownership rows for the provided session identifiers.
/// 删除给定会话标识对应的工作台历史文档归属状态行。
async fn delete_workspace_history_search_document_state_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if session_ids.is_empty() {
        return Ok(());
    }

    let params = session_ids
        .iter()
        .cloned()
        .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        "DELETE FROM workspace_history_search_docs WHERE session_id = ?1;",
        params,
        error_prefix,
    )
    .await
}

/// Executes a typed SQLite batch in safe chunks estimated against the full gRPC protobuf frame.
/// 按完整 gRPC protobuf 帧估算将类型化 SQLite 批处理拆成安全小批执行。
async fn execute_sqlite_batch_safely(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    statement: &str,
    params: Vec<Vec<ControllerSqliteValue>>,
    error_prefix: &str,
) -> Result<(), String> {
    if params.is_empty() {
        return Ok(());
    }

    let statement_bytes = estimate_text_frame_bytes(statement);
    let mut batch_index = 0usize;
    let mut current_rows: Vec<Vec<ControllerSqliteValue>> = Vec::new();
    let mut current_estimated_bytes =
        SQLITE_BATCH_FRAME_OVERHEAD_BYTES.saturating_add(statement_bytes);

    for row in params {
        let row_estimated_bytes = estimate_sqlite_row_frame_bytes(&row);
        let row_would_overflow = !current_rows.is_empty()
            && (current_rows.len() >= SQLITE_BATCH_SAFE_ROW_LIMIT
                || current_estimated_bytes.saturating_add(row_estimated_bytes)
                    > SQLITE_BATCH_SAFE_FRAME_TARGET_BYTES);

        if row_would_overflow {
            batch_index = batch_index.saturating_add(1);
            execute_sqlite_batch_chunk(
                controller,
                table_name,
                statement,
                std::mem::take(&mut current_rows),
                batch_index,
                current_estimated_bytes,
                error_prefix,
            )
            .await?;
            current_estimated_bytes =
                SQLITE_BATCH_FRAME_OVERHEAD_BYTES.saturating_add(statement_bytes);
        }

        current_estimated_bytes = current_estimated_bytes.saturating_add(row_estimated_bytes);
        current_rows.push(row);
    }

    if !current_rows.is_empty() {
        batch_index = batch_index.saturating_add(1);
        execute_sqlite_batch_chunk(
            controller,
            table_name,
            statement,
            current_rows,
            batch_index,
            current_estimated_bytes,
            error_prefix,
        )
        .await?;
    }

    Ok(())
}

/// Executes one already-sized SQLite batch chunk and reports frame-budget context on failure.
/// 执行一段已经切好大小的 SQLite 批处理，并在失败时报告帧预算上下文。
async fn execute_sqlite_batch_chunk(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    statement: &str,
    params: Vec<Vec<ControllerSqliteValue>>,
    batch_index: usize,
    estimated_bytes: usize,
    error_prefix: &str,
) -> Result<(), String> {
    let row_count = params.len();
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| {
            format!(
                "{error_prefix}：表 {table_name} 第 {batch_index} 批失败，行数 {row_count}，估算完整帧 {estimated_bytes} bytes：{error}"
            )
        })?;

    Ok(())
}

/// Estimates one SQLite row's contribution to the encoded request frame.
/// 估算一行 SQLite 参数对编码后请求帧的贡献。
fn estimate_sqlite_row_frame_bytes(row: &[ControllerSqliteValue]) -> usize {
    SQLITE_ROW_FRAME_OVERHEAD_BYTES
        + row
            .iter()
            .map(estimate_sqlite_value_frame_bytes)
            .sum::<usize>()
}

/// Estimates one SQLite scalar value's encoded request-frame contribution.
/// 估算一个 SQLite 标量值对编码后请求帧的贡献。
fn estimate_sqlite_value_frame_bytes(value: &ControllerSqliteValue) -> usize {
    SQLITE_VALUE_FRAME_OVERHEAD_BYTES
        + match value {
            ControllerSqliteValue::Int64(_) => 16,
            ControllerSqliteValue::Float64(_) => 16,
            ControllerSqliteValue::String(value) => estimate_text_frame_bytes(value),
            ControllerSqliteValue::Bytes(value) => value.len(),
            ControllerSqliteValue::Bool(_) => 8,
            ControllerSqliteValue::Null => 4,
        }
}

/// Estimates a text field after JSON/protobuf structural overhead and escaping slack are considered.
/// 在考虑 JSON/protobuf 结构开销与转义余量后估算文本字段大小。
fn estimate_text_frame_bytes(value: &str) -> usize {
    value.len().saturating_mul(2).saturating_add(32)
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

/// Converts one metadata row plus one controller FTS hit into the public workspace-history search-hit shape.
/// 将一条元数据行与一条控制器 FTS 命中转换为公开的工作台历史搜索命中结构。
fn stored_workspace_history_search_hit(
    row: &StoredWorkspaceHistorySearchMetadataRow,
    search_hit: &ControllerSqliteSearchFtsHit,
) -> WorkspaceHistorySearchHit {
    let preview_source = if !search_hit.content_snippet.trim().is_empty() {
        search_hit.content_snippet.as_str()
    } else if !search_hit.title_highlight.trim().is_empty() {
        search_hit.title_highlight.as_str()
    } else {
        search_hit.title.as_str()
    };
    WorkspaceHistorySearchHit {
        project_name: row.project_name.clone(),
        session_id: parse_session_id(&row.session_id).unwrap_or_default(),
        session_title: row.session_title.clone(),
        message_id: row.message_id.clone(),
        ordinal: row.ordinal,
        turn_index: row.turn_index,
        message_kind: session_kind_from_role(&row.role),
        preview: workspace_history_search_preview(preview_source),
        preview_markup: workspace_history_search_preview_markup(preview_source),
        created_at: row.created_at.clone(),
        score: search_hit.score,
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
        context_start_message_id: row.context_start_message_id,
        summary_message_id: row.summary_message_id,
        created_at: row.created_at,
    }
}

/// Converts one persisted runtime row into the in-memory runtime-state shape.
/// 将一条持久化运行时行转换为内存中的运行时状态结构。
fn stored_runtime_state_row_to_entry(row: StoredRuntimeStateRow) -> SessionRuntimeStateEntry {
    SessionRuntimeStateEntry {
        active_agent_id: row.active_agent_id,
        active_provider_id: row.active_provider_id,
        active_model_id: row.active_model_id,
        last_compaction_id: row.last_compaction_id,
        context_start_message_id: row.context_start_message_id,
        ui_state: row
            .ui_state_json
            .as_deref()
            .and_then(parse_session_ui_state_from_json)
            .unwrap_or_default(),
        conversation_state: conversation_state_with_continuation(row.continuation_json),
        last_provider_usage: row
            .provider_usage_json
            .as_deref()
            .and_then(parse_provider_usage_from_json),
        updated_at: row.updated_at,
    }
}

/// Seeds runtime conversation records from the recent transcript window when no full state was loaded.
/// 当未加载完整状态时，基于最近转录窗口为运行时会话补齐记录。
fn runtime_state_with_recent_records(
    mut runtime_state: SessionRuntimeStateEntry,
    messages: &[SessionMessageEntry],
) -> SessionRuntimeStateEntry {
    if runtime_state.conversation_state.records.is_empty() && !messages.is_empty() {
        let continuation = runtime_state.conversation_state.continuation.clone();
        runtime_state.conversation_state = conversation_state_from_session_messages(messages);
        runtime_state.conversation_state.continuation = continuation;
    }
    runtime_state
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
    query.split_whitespace().collect::<Vec<_>>().join(" ")
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
                    *value = compact_large_metadata_field(key, value.clone());
                }
            }
            for key in ["arguments", "input", "request", "response"] {
                if let Some(value) = object.get_mut(key) {
                    if estimate_json_value_bytes(value) > SESSION_MESSAGE_METADATA_BUDGET_BYTES / 4
                    {
                        *value = compact_large_metadata_field(key, value.clone());
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

/// Serializes one typed transcript UI-state structure into the persisted JSON payload when needed.
/// 在需要时将一条带类型的转录界面状态结构序列化为持久化 JSON 载荷。
fn serialize_session_ui_state_to_json(state: &SessionTranscriptUiState) -> Option<String> {
    if state.collapsed_message_ids.is_empty() && state.pending_prompt_queue.is_empty() {
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
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies that transcript metadata keeps display fields while sanitizing oversized host payloads.
    /// 验证转录元数据会保留展示字段，同时清洗超大的宿主载荷。
    #[test]
    fn session_message_meta_json_sanitizes_large_host_payloads() {
        let message = SessionMessageEntry {
            id: String::from("message-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回"),
            body: String::from("已写入文件。"),
            metadata: Some(json!({
                "tool_name": "write_file",
                "host_result": {
                    "kind": "change_set",
                    "payload": {
                        "path": "src/main.rs",
                    },
                },
            })),
            created_at: String::from("1"),
        };

        let json_payload = serialize_session_message_meta_to_json(&message)
            .expect("expected metadata payload to be serialized");
        let parsed = parse_session_message_meta_from_json(&json_payload)
            .expect("expected metadata payload to be parsed");

        assert_eq!(parsed.title.as_deref(), Some("工具返回"));
        let metadata = parsed.metadata.expect("expected sanitized metadata");
        assert_eq!(
            metadata.get("tool_name").and_then(|value| value.as_str()),
            Some("write_file")
        );

        let host_result = metadata
            .get("host_result")
            .and_then(|value| value.as_object())
            .expect("expected host result to be compacted");
        assert_eq!(
            host_result.get("omitted").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            host_result.get("field").and_then(|value| value.as_str()),
            Some("host_result")
        );
        let preview = host_result
            .get("preview")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assert!(preview.contains("change_set"));
        assert!(preview.contains("src/main.rs"));
    }

    /// Verifies that oversized transcript bodies are split without losing UTF-8 content.
    /// 验证超大转录正文会在不破坏 UTF-8 内容的前提下被拆分。
    #[test]
    fn oversized_session_message_body_is_split_and_rebuilt() {
        let body = "工具输出包含中文与 ASCII content\n".repeat(8_000);
        let prepared = prepare_session_message_body_for_storage(&body);

        assert_ne!(prepared.inline_content, body);
        assert!(!prepared.chunks.is_empty());
        assert!(
            prepared
                .chunks
                .iter()
                .all(|chunk| chunk.len() <= SESSION_MESSAGE_CHUNK_CONTENT_BYTES)
        );
        assert_eq!(prepared.chunks.concat(), body);

        let chunk_rows = prepared
            .chunks
            .into_iter()
            .enumerate()
            .map(|(chunk_index, content)| StoredSessionMessageChunkRow {
                message_id: String::from("message-1"),
                chunk_index: chunk_index as i64,
                content,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            rebuild_session_message_body_from_chunks("message-1", chunk_rows)
                .expect("expected chunks to rebuild"),
            body
        );
    }

    /// Verifies that SQL row preparation keeps large bodies out of the lightweight message row.
    /// 验证 SQL 行预处理会避免将大型正文写入轻量消息行。
    #[test]
    fn session_message_batch_rows_store_oversized_body_as_chunks() {
        let session_id = SessionId::from_u128(42);
        let body = "大型工具结果 payload ".repeat(10_000);
        let message = SessionMessageEntry {
            id: String::from("message-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回"),
            body: body.clone(),
            metadata: None,
            created_at: String::from("1"),
        };
        let mut message_rows = Vec::new();
        let mut chunk_rows = Vec::new();
        let mut search_rows = Vec::new();

        push_session_message_rows_for_session(
            &BTreeMap::new(),
            -1,
            -1,
            &mut message_rows,
            &mut chunk_rows,
            &mut search_rows,
            "project-1",
            true,
            session_id,
            "会话标题",
            &[message],
        );

        assert_eq!(message_rows.len(), 1);
        assert!(!chunk_rows.is_empty());
        assert!(search_rows.is_empty());
        match &message_rows[0][7] {
            ControllerSqliteValue::String(inline_content) => {
                assert_ne!(inline_content, &body);
                assert!(inline_content.len() < body.len());
            }
            other => panic!("expected inline content string, got {other:?}"),
        }

        let rebuilt = chunk_rows
            .iter()
            .map(|row| match &row[3] {
                ControllerSqliteValue::String(content) => content.as_str(),
                other => panic!("expected chunk content string, got {other:?}"),
            })
            .collect::<String>();
        assert_eq!(rebuilt, body);
    }

    /// Verifies that user messages start stable dialogue turns while following outputs stay grouped.
    /// 验证用户消息会开启稳定对话轮次，后续输出会保持归组。
    #[test]
    fn session_message_turn_indexes_group_outputs_under_user_turns() {
        let messages = vec![
            SessionMessageEntry::hidden(
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ExploreOutput,
                "探索",
                "项目概览",
            ),
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第一轮回答"),
            SessionMessageEntry {
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                ..SessionMessageEntry::new(SessionMessageKind::Assistant, "工具返回", "结果")
            },
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第二轮回答"),
        ];

        assert_eq!(
            assign_session_message_turn_indexes(&messages),
            vec![0, 0, 0, 0, 1, 1]
        );
    }

    /// Verifies that message batch rows persist the computed turn index beside ordinal order.
    /// 验证消息批处理行会在 ordinal 顺序之外持久化计算后的轮次索引。
    #[test]
    fn session_message_batch_rows_persist_turn_indexes() {
        let session_id = SessionId::from_u128(44);
        let messages = vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第一轮回答"),
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
        ];
        let mut message_rows = Vec::new();
        let mut chunk_rows = Vec::new();
        let mut search_rows = Vec::new();

        push_session_message_rows_for_session(
            &BTreeMap::new(),
            -1,
            -1,
            &mut message_rows,
            &mut chunk_rows,
            &mut search_rows,
            "project-1",
            true,
            session_id,
            "会话标题",
            &messages,
        );

        let turn_indexes = message_rows
            .iter()
            .map(|row| match row[6] {
                ControllerSqliteValue::Int64(value) => value,
                ref other => panic!("expected turn index integer, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(turn_indexes, vec![0, 0, 1]);
        assert!(chunk_rows.is_empty());
        assert_eq!(search_rows.len(), 2);
    }

    /// Verifies partial-window persistence keeps existing ordinals instead of renumbering from zero.
    /// 验证局部窗口持久化会保留已有序号，而不是从零重新编号。
    #[test]
    fn session_message_rows_preserve_existing_positions_for_partial_windows() {
        let session_id = SessionId::from_u128(46);
        let existing_message = SessionMessageEntry {
            id: String::from("existing-message"),
            ..SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "历史回答")
        };
        let new_message = SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "新问题");
        let messages = vec![existing_message, new_message];
        let mut persisted_positions = BTreeMap::new();
        persisted_positions.insert(
            String::from("existing-message"),
            StoredSessionMessagePositionRow {
                id: String::from("existing-message"),
                ordinal: 42,
                turn_index: 7,
            },
        );
        let mut message_rows = Vec::new();
        let mut chunk_rows = Vec::new();
        let mut search_rows = Vec::new();

        push_session_message_rows_for_session(
            &persisted_positions,
            42,
            7,
            &mut message_rows,
            &mut chunk_rows,
            &mut search_rows,
            "project-1",
            true,
            session_id,
            "会话标题",
            &messages,
        );

        let ordinals = message_rows
            .iter()
            .map(|row| match row[2] {
                ControllerSqliteValue::Int64(value) => value,
                ref other => panic!("expected ordinal integer, got {other:?}"),
            })
            .collect::<Vec<_>>();
        let turn_indexes = message_rows
            .iter()
            .map(|row| match row[6] {
                ControllerSqliteValue::Int64(value) => value,
                ref other => panic!("expected turn index integer, got {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(ordinals, vec![42, 43]);
        assert_eq!(turn_indexes, vec![7, 8]);
        assert!(chunk_rows.is_empty());
        assert_eq!(search_rows.len(), 1);
    }

    /// Verifies that one-session maintenance row building does not require a full session list.
    /// 验证单会话维护行构建不需要依赖完整会话列表。
    #[test]
    fn single_session_message_rows_can_be_built_for_maintenance() {
        let session_id = SessionId::from_u128(45);
        let messages = vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
            SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "AI 输出",
                &"维护路径正文 ".repeat(10_000),
            ),
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
        ];
        let mut message_rows = Vec::new();
        let mut chunk_rows = Vec::new();
        let mut search_rows = Vec::new();

        push_session_message_rows_for_session(
            &BTreeMap::new(),
            -1,
            -1,
            &mut message_rows,
            &mut chunk_rows,
            &mut search_rows,
            "project-1",
            true,
            session_id,
            "会话标题",
            &messages,
        );

        assert_eq!(message_rows.len(), 3);
        assert!(!chunk_rows.is_empty());
        assert!(message_rows.iter().all(|row| {
            matches!(&row[1], ControllerSqliteValue::String(value) if value == &session_id.to_string())
        }));
        let turn_indexes = message_rows
            .iter()
            .map(|row| match row[6] {
                ControllerSqliteValue::Int64(value) => value,
                ref other => panic!("expected turn index integer, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(turn_indexes, vec![0, 0, 1]);
        assert_eq!(search_rows.len(), 2);
    }

    /// Verifies that default transcript windows keep tool details out of the mainline query profile.
    /// 验证默认转录窗口会将工具明细排除在主线查询画像之外。
    #[test]
    fn default_transcript_window_filters_tool_detail_kinds() {
        assert!(persisted_kind_in_default_transcript_window(
            "assistant-output"
        ));
        assert!(persisted_kind_in_default_transcript_window(
            "summary-output"
        ));
        assert!(!persisted_kind_in_default_transcript_window(
            "tool-call-output"
        ));
        assert!(!persisted_kind_in_default_transcript_window(
            "tool-result-output"
        ));
    }

    /// Verifies that the on-demand tool-detail stream accepts only tool call and result rows.
    /// 验证按需工具明细流只接收工具调用与工具结果行。
    #[test]
    fn tool_detail_stream_filters_only_tool_detail_kinds() {
        assert!(persisted_kind_in_tool_detail_stream("tool-call-output"));
        assert!(persisted_kind_in_tool_detail_stream("tool-result-output"));
        assert!(!persisted_kind_in_tool_detail_stream("assistant-output"));
        assert!(!persisted_kind_in_tool_detail_stream("summary-output"));
    }

    /// Verifies that transcript persistence uses upsert semantics instead of full-session deletion.
    /// 验证转录持久化使用 upsert 语义，而不是整会话删除。
    #[test]
    fn session_message_persistence_statement_uses_upsert() {
        let statement = session_message_upsert_statement();

        assert!(statement.contains("ON CONFLICT(id) DO UPDATE"));
        assert!(!statement.contains("DELETE FROM session_messages"));
    }

    /// Verifies that chunk persistence tolerates overlapping maintenance tasks.
    /// 验证分片持久化可以容忍重叠维护任务。
    #[test]
    fn session_message_chunk_persistence_statement_uses_upsert() {
        let statement = session_message_chunk_upsert_statement();

        assert!(statement.contains("ON CONFLICT(message_id, chunk_index) DO UPDATE"));
        assert!(statement.contains("content = excluded.content"));
    }

    /// Verifies that user search text is treated literally inside SQLite LIKE patterns.
    /// 验证用户搜索文本会在 SQLite LIKE 模式中按字面量处理。
    #[test]
    fn session_history_search_like_pattern_escapes_wildcards() {
        assert_eq!(
            session_history_search_like_pattern(r"src\foo_%"),
            r"%src\\foo\_\%%"
        );
    }

    /// Verifies that lightweight search previews stay compact and non-empty.
    /// 验证轻量搜索预览会保持紧凑且非空。
    #[test]
    fn session_history_search_preview_compacts_text() {
        let preview = session_history_search_preview(
            "第一行命中内容\n\n第二行包含 src/main.rs\n第三行还有更多上下文\n第四行应被截断",
        );

        assert!(preview.contains("第一行命中内容"));
        assert!(preview.contains("src/main.rs"));
        assert!(!preview.contains("第四行应被截断"));
    }

    /// Verifies that project-scoped controller FTS index names stay deterministic and use the dedicated prefix.
    /// 验证项目级控制器 FTS 索引名保持确定性并使用专用前缀。
    #[test]
    fn workspace_history_search_project_index_name_is_stable() {
        let first = workspace_history_search_project_index_name("project-alpha");
        let second = workspace_history_search_project_index_name("project-alpha");
        let third = workspace_history_search_project_index_name("project-beta");

        assert_eq!(first, second);
        assert_ne!(first, third);
        assert!(first.starts_with(WORKSPACE_HISTORY_SEARCH_PROJECT_INDEX_PREFIX));
    }

    /// Verifies that controller FTS preview fragments strip highlight tags before compact rendering.
    /// 验证控制器 FTS 预览片段会在紧凑渲染前移除高亮标签。
    #[test]
    fn workspace_history_search_preview_strips_mark_tags() {
        let preview = workspace_history_search_preview(
            "第一行 <mark>命中</mark>\n第二行继续\n第三行还有内容\n第四行应被截断",
        );

        assert!(preview.contains("第一行 命中"));
        assert!(!preview.contains("<mark>"));
        assert!(!preview.contains("第四行应被截断"));
    }

    /// Verifies that raw preview markup keeps the original `<mark>` tags for UI-side highlight rendering.
    /// 验证原始预览标记会保留 `<mark>` 标签，供 UI 侧渲染命中高亮。
    #[test]
    fn workspace_history_search_preview_markup_preserves_mark_tags() {
        let preview = workspace_history_search_preview_markup("第一行 <mark>命中</mark>");

        assert!(preview.contains("<mark>命中</mark>"));
    }

    /// Verifies that only visible user prompts and visible assistant final outputs enter the workspace BM25 index.
    /// 验证只有可见用户提问与可见助手最终回复会进入工作台 BM25 索引。
    #[test]
    fn workspace_history_search_indexable_messages_filter_intermediate_records() {
        assert!(session_message_is_workspace_history_search_indexable(
            &SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "继续")
        ));
        assert!(session_message_is_workspace_history_search_indexable(
            &SessionMessageEntry {
                record_kind: SessionMessageRecordKind::AssistantOutput,
                ..SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "最终答案")
            }
        ));
        assert!(!session_message_is_workspace_history_search_indexable(
            &SessionMessageEntry::hidden(
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "工具返回",
                "中间结果"
            )
        ));
        assert!(!session_message_is_workspace_history_search_indexable(
            &SessionMessageEntry::hidden(
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ExploreOutput,
                "探索",
                "分析过程"
            )
        ));
    }

    /// Verifies that context windows never request negative turn indexes.
    /// 验证上下文窗口不会请求负数轮次索引。
    #[test]
    fn session_history_context_turn_bounds_clamp_lower_edge() {
        assert_eq!(session_history_context_turn_bounds(1, 2), (0, 3));
        assert_eq!(session_history_context_turn_bounds(-5, 2), (0, 2));
    }

    /// Verifies that context windows keep a stable inclusive turn span around the anchor.
    /// 验证上下文窗口会围绕锚点保留稳定的闭区间轮次范围。
    #[test]
    fn session_history_context_turn_bounds_expand_around_anchor() {
        assert_eq!(session_history_context_turn_bounds(12, 2), (10, 14));
        assert_eq!(session_history_context_turn_bounds(12, -1), (12, 12));
    }

    /// Verifies that legacy migration candidates do not project full message content.
    /// 验证旧数据迁移候选查询不会投影完整消息正文。
    #[test]
    fn legacy_oversized_message_candidate_query_avoids_full_content_projection() {
        let statement = legacy_oversized_message_candidate_query();
        let normalized = statement
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        assert!(normalized.contains("length(cast(m.content as blob)) as content_bytes"));
        assert!(normalized.contains("length(m.content) as content_chars"));
        assert!(normalized.contains("not exists"));
        assert!(normalized.contains("from session_message_chunks"));
        assert!(!normalized.contains("select m.id, m.content"));
        assert!(!normalized.contains("substr(m.content"));
    }

    /// Verifies that legacy migration scripts split content inside SQLite before shrinking inline text.
    /// 验证旧数据迁移脚本会在 SQLite 内部分片后再收缩内联文本。
    #[test]
    fn legacy_inline_message_chunk_migration_script_uses_sqlite_substr() {
        let session_id = SessionId::from_u128(55);
        let script = legacy_inline_message_chunk_migration_script(session_id, "message-'quoted");

        assert!(script.contains("BEGIN IMMEDIATE TRANSACTION"));
        assert!(script.contains("INSERT OR REPLACE INTO session_message_chunks"));
        assert!(script.contains("substr(source.content_text"));
        assert!(script.contains("NOT EXISTS"));
        assert!(script.contains("UPDATE session_messages"));
        assert!(script.contains("SET content = substr(content"));
        assert!(script.contains("length(CAST(content AS BLOB))"));
        assert!(script.contains("message-''quoted"));
        assert!(script.contains("CROSS JOIN digits"));
        assert!(!script.contains("DELETE FROM session_message_chunks"));
        assert!(script.contains("COMMIT"));
    }

    /// Verifies that legacy chunk-count estimates use ceiling division over text characters.
    /// 验证旧数据分片数量估算会基于文本字符数做向上取整。
    #[test]
    fn legacy_inline_message_chunk_count_rounds_up() {
        let chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS as i64;

        assert_eq!(legacy_inline_message_chunk_count(0).unwrap(), 0);
        assert_eq!(legacy_inline_message_chunk_count(chunk_chars).unwrap(), 1);
        assert_eq!(
            legacy_inline_message_chunk_count(chunk_chars + 1).unwrap(),
            2
        );
    }

    /// Verifies that legacy runtime cleanup nulls the large column without selecting it.
    /// 验证旧运行态清理会直接置空大字段而不会查询该字段。
    #[test]
    fn runtime_state_cleanup_statement_nulls_legacy_conversation_json() {
        let statement = session_runtime_conversation_state_cleanup_statement();

        assert!(statement.contains("conversation_state_json = NULL"));
        assert!(statement.contains("WHERE session_id = ?1"));
        assert!(!statement.to_ascii_uppercase().contains("SELECT"));
    }

    /// Verifies that lightweight runtime persistence keeps continuation separate from large records.
    /// 验证轻量运行时持久化会将 continuation 与大型记录分离保存。
    #[test]
    fn runtime_state_rows_store_continuation_without_full_conversation_state() {
        let session_id = SessionId::from_u128(43);
        let session = SessionSummary::new(
            session_id,
            None,
            String::from("测试会话"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let runtime_state = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: Vec::new(),
                records: vec![crate::conversation_protocol::SessionConversationRecord {
                    id: String::from("record-1"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::ToolResultOutput,
                    visible: true,
                    title: String::from("工具返回"),
                    body: "大型正文 ".repeat(10_000),
                    metadata: None,
                    created_at: String::from("1"),
                }],
                continuation: SessionProviderContinuationState {
                    provider_id: Some(String::from("openai")),
                    model_id: Some(String::from("gpt-test")),
                    last_provider_message_id: Some(String::from("resp-1")),
                    updated_at: Some(String::from("123")),
                },
            },
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        };
        let mut runtime_states = BTreeMap::new();
        runtime_states.insert(session_id, runtime_state);
        let mut rows = Vec::new();

        push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0][7], ControllerSqliteValue::Null));
        let continuation_json = match &rows[0][8] {
            ControllerSqliteValue::String(value) => value,
            other => panic!("expected continuation JSON string, got {other:?}"),
        };
        assert!(continuation_json.contains("resp-1"));
        assert!(!continuation_json.contains("大型正文"));
    }

    /// Verifies that rebuilding runtime state from recent messages preserves provider continuation.
    /// 验证基于最近消息重建运行时状态时会保留 Provider continuation。
    #[test]
    fn runtime_state_rebuild_from_recent_messages_preserves_continuation() {
        let runtime_state = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                continuation: SessionProviderContinuationState {
                    provider_id: Some(String::from("openai")),
                    model_id: Some(String::from("gpt-test")),
                    last_provider_message_id: Some(String::from("resp-1")),
                    updated_at: Some(String::from("123")),
                },
                ..SessionConversationState::default()
            },
            ..SessionRuntimeStateEntry::default()
        };
        let messages = vec![SessionMessageEntry {
            id: String::from("message-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: String::from("继续。"),
            metadata: None,
            created_at: String::from("1"),
        }];

        let rebuilt = runtime_state_with_recent_records(runtime_state, &messages);

        assert_eq!(rebuilt.conversation_state.records.len(), 1);
        assert_eq!(
            rebuilt
                .conversation_state
                .continuation
                .last_provider_message_id
                .as_deref(),
            Some("resp-1")
        );
    }
}

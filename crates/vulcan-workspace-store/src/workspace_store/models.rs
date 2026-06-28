use super::*;

/// Describes the completed workspace-snapshot persistence pass, including any non-fatal auxiliary warnings.
/// 描述一次已完成的工作台快照持久化过程，并携带所有非致命的辅助告警。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshotPersistOutcome {
    /// Non-fatal warnings collected while synchronizing auxiliary workspace projections.
    /// 在同步工作台辅助投影期间收集到的非致命告警。
    pub warnings: Vec<String>,
}

impl WorkspaceSnapshotPersistOutcome {
    /// Returns whether the persistence pass completed without any auxiliary warnings.
    /// 返回本次持久化是否在没有任何辅助告警的情况下完成。
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

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
/// Maximum message rows fetched for each requested turn before an inner-turn page becomes necessary.
/// 在需要轮内分页前，每个请求轮次允许拉取的最大消息行数。
const SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT: usize = 32;
/// Minimum message-row floor used when one segmented turn window contains dense rows.
/// 当单个分段轮次窗口包含密集行时使用的最小消息行下限。
const SESSION_TRANSCRIPT_MIN_ROW_LIMIT: usize = 100;
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

/// Lightweight child-task mirror restored from a parent session metadata document.
/// 从父会话元数据文档恢复出的轻量子任务镜像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTaskMirrorSnapshot {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: SessionId,
    /// Current child task status label such as running, completed, stopped, or failed.
    /// 当前子任务状态标签，例如 running、completed、stopped 或 failed。
    pub status: String,
    /// Mirrored child segment line count.
    /// 镜像的子任务分段行数。
    pub line_count: u64,
    /// Last update timestamp stored in the parent metadata mirror.
    /// 父元数据镜像中保存的最后更新时间戳。
    pub updated_at: String,
}

/// Display-only fork source data projected from segmented session metadata.
/// 从分段会话元数据投影出的纯展示分叉来源数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionForkSourceSnapshot {
    /// Source project tag used to locate the original conversation.
    /// 用于定位原始会话的源项目标签。
    pub source_project_tag: String,
    /// Source session identifier for navigation.
    /// 用于导航的源会话标识。
    pub source_session_id: String,
    /// Source conversation title captured at fork time.
    /// 分叉时捕获的源会话标题。
    pub source_title: String,
    /// Last retained message id at the fork boundary.
    /// 分叉边界处最后保留的消息标识。
    pub anchor_message_id: Option<String>,
    /// Fork creation timestamp.
    /// 分叉创建时间戳。
    pub created_at: String,
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

/// Raw project row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始项目行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredProjectRow {
    /// Stable project identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定项目标识。
    pub(super) id: String,
    /// Project display name stored in SQLite.
    /// 存储在 SQLite 中的项目展示名称。
    pub(super) name: String,
    /// Absolute root path stored in SQLite.
    /// 存储在 SQLite 中的绝对根目录路径。
    pub(super) root_path: String,
    /// SQLite numeric pinned flag.
    /// SQLite 数值型置顶标记。
    pub(super) pinned: i64,
    /// Stable section order value.
    /// 稳定的分区顺序值。
    pub(super) sort_order: i64,
    /// Optional project icon pack stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标包。
    pub(super) icon_pack: Option<String>,
    /// Optional project icon key stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标键。
    pub(super) icon_key: Option<String>,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    pub(super) archived: i64,
}

/// Raw lightweight session locator row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的轻量会话定位行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredSessionIndexRow {
    /// Stable session identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定会话标识。
    pub(super) id: String,
    /// Owning project identifier persisted in SQLite.
    /// 持久化到 SQLite 的所属项目标识。
    pub(super) project_id: String,
    /// Project tag used by the deterministic segmented directory layout.
    /// 确定性分段目录布局使用的项目标签。
    pub(super) project_tag: String,
    /// Optional session display title.
    /// 可选的会话展示标题。
    pub(super) title: Option<String>,
    /// Optional persisted title-source key.
    /// 可选的持久化标题来源键。
    pub(super) title_source: Option<String>,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    pub(super) archived: i64,
    /// Project-local stable sort order.
    /// 项目内稳定排序值。
    pub(super) sort_order: i64,
    /// Raw creation timestamp as Unix milliseconds text.
    /// Unix 毫秒文本格式的原始创建时间。
    pub(super) created_at: String,
    /// Raw update timestamp as Unix milliseconds text.
    /// Unix 毫秒文本格式的原始更新时间。
    pub(super) updated_at: String,
}

/// Raw application metadata row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始应用元数据行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredAppMetadataRow {
    /// Persisted metadata value.
    /// 持久化元数据值。
    pub(super) value: String,
}

/// Lightweight session turn-index row used by tool-detail hydration lookups.
/// 工具明细水合查询使用的轻量会话轮次索引行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredSessionTurnIndexRow {
    /// Stable conversation-turn index already stored in SQLite.
    /// 已持久化到 SQLite 中的稳定对话轮次索引。
    pub(super) turn_index: i64,
}

/// Disabled legacy runtime-state shape retained for migration-free loader boundaries.
/// 为无旧兼容加载边界保留的已禁用旧运行态形状。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredRuntimeStateRow {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub(super) session_id: String,
    /// Optional active agent identifier.
    /// 可选的激活代理标识。
    pub(super) active_agent_id: Option<String>,
    /// Optional active provider identifier.
    /// 可选的激活供应商标识。
    pub(super) active_provider_id: Option<String>,
    /// Optional active model identifier.
    /// 可选的激活模型标识。
    pub(super) active_model_id: Option<String>,
    /// Optional latest compaction identifier.
    /// 可选的最近一次压缩标识。
    pub(super) last_compaction_id: Option<String>,
    /// Optional current context start message identifier.
    /// 可选的当前上下文起始消息标识。
    pub(super) context_start_message_id: Option<String>,
    /// Optional persisted transcript UI state payload.
    /// 可选的持久化转录界面状态载荷。
    pub(super) ui_state_json: Option<String>,
    /// Optional persisted provider usage payload.
    /// 可选的持久化 Provider 用量载荷。
    #[serde(default)]
    pub(super) provider_usage_json: Option<String>,
    /// Optional persisted full canonical conversation-state payload.
    /// 可选的持久化完整规范会话状态载荷。
    #[serde(default)]
    pub(super) conversation_state_json: Option<String>,
    /// Optional persisted lightweight provider continuation payload.
    /// 可选的持久化轻量 Provider continuation 载荷。
    #[serde(default)]
    pub(super) continuation_json: Option<String>,
    /// Optional persisted hidden context-injection runtime payload.
    /// 可选的隐藏上下文注入运行态载荷。
    #[serde(default)]
    pub(super) context_injection_json: Option<String>,
    /// Optional persisted session-level VMM override payload.
    /// 可选的持久化会话级 VMM 覆盖载荷。
    #[serde(default)]
    pub(super) vmm_json: Option<String>,
    /// Optional persisted VMM writeback outbox payload.
    /// 可选的持久化 VMM 写回 outbox 载荷。
    #[serde(default)]
    pub(super) vmm_writeback_outbox_json: Option<String>,
    /// Optional persisted VMM profile sync state payload.
    /// 可选的持久化 VMM 画像同步状态载荷。
    #[serde(default)]
    pub(super) vmm_profile_sync_json: Option<String>,
    /// Updated-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的更新时间。
    pub(super) updated_at: String,
}

//! Controller-backed persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的控制器持久化辅助模块。

#![allow(dead_code)]

use crate::conversation_protocol::{
    SessionConversationState, SessionProviderContinuationState,
    conversation_state_from_session_messages,
};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::{
    SessionCompactionEntry, SessionId, SessionMessageEntry, SessionMessageRecordKind,
    SessionRunPhase, SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTitleSource,
    SessionTranscriptUiState, TaskProjectSummary, VmmProfileSyncState, VmmWritebackOutboxEntry,
};
use crate::provider_usage::parse_provider_usage_from_json;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_context_injection::ContextInjectionState;
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionBoundaryStateStore,
    SessionBoundaryStorageWrite, SessionBoundaryStore, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;
use vulcan_vmm::VmmConfigLayer;

mod persistence;
mod segmented_session_store;
mod session_load;

pub use self::segmented_session_store::find_segmented_session_directory_for_task_store;
use self::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventInput,
    SegmentedSessionEventKind, SegmentedSessionMetadataPatch, append_segmented_boundary_events,
    append_segmented_session_events, backup_runtime_log_before_compaction,
    clear_all_segmented_session_roots, create_segmented_session_in_location,
    find_segmented_session_directory, find_segmented_session_directory_by_id,
    load_session_boundary_state, patch_segmented_session_metadata, read_runtime_log_lines,
    read_segmented_session_event_window, read_state, record_runtime_audit_event,
    record_runtime_injection_applied, replace_session_boundary_state,
    replay_segmented_session_context, segmented_session_directory,
    state_document_from_runtime_state, write_state,
};
pub use persistence::{
    archive_project_sessions, archive_project_with_sessions, archive_single_session,
    archive_single_session_for_project, permanently_delete_archived_project,
    permanently_delete_archived_session, persist_collapsed_task_project_ids, persist_new_project,
    persist_project_order_update, persist_project_pinned_and_order_update,
    persist_single_session_runtime_state_update, persist_single_session_update,
    persist_workspace_snapshot, remove_project_after_archiving_sessions, rename_project,
    rename_single_session_metadata, restore_project_with_sessions, restore_single_session,
    toggle_project_pinned,
};
pub use session_load::{
    load_session_history_page, load_session_hydration_snapshot,
    load_session_hydration_snapshot_for_project, load_session_transcript_window,
};

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

/// Raw lightweight session locator row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的轻量会话定位行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionIndexRow {
    /// Stable session identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定会话标识。
    id: String,
    /// Owning project identifier persisted in SQLite.
    /// 持久化到 SQLite 的所属项目标识。
    project_id: String,
    /// Project tag used by the deterministic segmented directory layout.
    /// 确定性分段目录布局使用的项目标签。
    project_tag: String,
    /// Optional session display title.
    /// 可选的会话展示标题。
    title: Option<String>,
    /// Optional persisted title-source key.
    /// 可选的持久化标题来源键。
    title_source: Option<String>,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
    /// Project-local stable sort order.
    /// 项目内稳定排序值。
    sort_order: i64,
    /// Raw creation timestamp as Unix milliseconds text.
    /// Unix 毫秒文本格式的原始创建时间。
    created_at: String,
    /// Raw update timestamp as Unix milliseconds text.
    /// Unix 毫秒文本格式的原始更新时间。
    updated_at: String,
}

/// Raw application metadata row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始应用元数据行。
#[derive(Debug, Clone, Deserialize)]
struct StoredAppMetadataRow {
    /// Persisted metadata value.
    /// 持久化元数据值。
    value: String,
}

/// Lightweight session turn-index row used by tool-detail hydration lookups.
/// 工具明细水合查询使用的轻量会话轮次索引行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionTurnIndexRow {
    /// Stable conversation-turn index already stored in SQLite.
    /// 已持久化到 SQLite 中的稳定对话轮次索引。
    turn_index: i64,
}

/// Disabled legacy runtime-state shape retained for migration-free loader boundaries.
/// 为无旧兼容加载边界保留的已禁用旧运行态形状。
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
    load_workspace_snapshot(controller).await
}

/// Loads the current persisted workspace snapshot from project config and segmented text metadata.
/// 从项目配置与分段文本元数据加载当前持久化工作台快照。
pub async fn load_workspace_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    let project_rows = query_project_rows(&controller).await?;
    let session_rows = query_session_index_rows(&controller).await?;

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

    let transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let archived_transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();
    let archived_compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();

    let mut runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let archived_runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    let mut session_updated_at = BTreeMap::new();
    for row in &session_rows {
        let session_id = parse_session_id(&row.id)?;
        let project_id = row
            .project_id
            .trim()
            .is_empty()
            .then(|| row.project_tag.clone())
            .unwrap_or_else(|| row.project_id.clone());
        let project_tag = row
            .project_tag
            .trim()
            .is_empty()
            .then(|| project_id.clone())
            .unwrap_or_else(|| row.project_tag.clone());
        let archived = row.archived != 0;
        let session_dir = segmented_session_directory(
            &controller.data_root,
            archived,
            &project_tag,
            &segmented_session_date_from_unix_millis_text(&row.created_at),
            &row.id,
        );
        if !session_dir.exists() {
            continue;
        }
        let project_name = project_rows
            .iter()
            .find(|row| row.id == project_id)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| project_id.clone());
        let project_row_exists = project_rows.iter().any(|row| row.id == project_id);
        if archived && project_row_exists {
            ensure_project_summary_for_session(
                &mut archived_task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        } else if !archived {
            ensure_project_summary_for_session(
                &mut task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        }
        if !archived && let Some(state_document) = read_state(&session_dir)? {
            if !state_document.runtime_state.is_null() {
                let mut runtime_state = serde_json::from_value::<SessionRuntimeStateEntry>(
                    state_document.runtime_state,
                )
                .map_err(|error| {
                    format!("解析分段会话 `{}` 的 state.json 失败：{error}", row.id)
                })?;
                if runtime_state.updated_at.trim().is_empty()
                    && !state_document.updated_at.trim().is_empty()
                {
                    runtime_state.updated_at = state_document.updated_at;
                }
                hydrate_runtime_state_from_segmented_replay_if_missing(
                    session_id,
                    &session_dir,
                    &mut runtime_state,
                )?;
                let _ = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);
                runtime_state_map.insert(session_id, runtime_state);
            }
        }
        let focus = session_focus_preview(&[]);
        let title = row
            .title
            .clone()
            .unwrap_or_else(|| String::from("Untitled"));
        let session_summary = SessionSummary::new(
            session_id,
            Some(project_id),
            title,
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(
            row.title_source.as_deref().unwrap_or("manual"),
        ))
        .with_created_at(row.created_at.clone())
        .with_updated_at(row.updated_at.clone());
        session_updated_at.insert(
            session_id,
            row.updated_at.parse::<i64>().unwrap_or_default(),
        );

        if archived {
            archived_sessions.push(session_summary);
        } else {
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    visible_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
    });

    archived_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
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

/// Clears process-scoped runtime latches that must never survive an application restart.
/// 清理绝不能跨应用重启保留的进程级运行态闩锁。
///
/// # Parameters
/// 参数
/// `runtime_state` is the freshly loaded persisted session runtime state.
/// `runtime_state` 是刚从持久化内容加载出的会话运行态。
///
/// # Returns
/// 返回值
/// Whether any volatile field was cleared.
/// 是否清理了任何临时字段。
fn sanitize_loaded_runtime_state_for_fresh_process(
    runtime_state: &mut SessionRuntimeStateEntry,
) -> bool {
    runtime_state.active_precheck_turn_id.take().is_some()
}

/// Rehydrates the canonical conversation state from segmented JSONL when state.json is intentionally lightweight.
/// 当 state.json 按设计只保存轻量运行态时，从分段 JSONL 回填规范会话状态。
fn hydrate_runtime_state_from_segmented_replay_if_missing(
    session_id: SessionId,
    session_dir: &std::path::Path,
    runtime_state: &mut SessionRuntimeStateEntry,
) -> Result<(), String> {
    if runtime_state_has_concrete_conversation_context(runtime_state) {
        return Ok(());
    }
    let Some(replay) = replay_segmented_session_context(session_dir)? else {
        return Ok(());
    };
    runtime_state.conversation_state = replay.conversation_state;
    if runtime_state.last_compaction_id.is_none() {
        runtime_state.last_compaction_id = replay.last_compaction_id;
    }
    if runtime_state.context_start_message_id.is_none() {
        runtime_state.context_start_message_id = replay.context_start_message_id;
    }
    if runtime_state.updated_at.trim().is_empty() {
        if let Some(updated_at) = replay.updated_at {
            runtime_state.updated_at = updated_at;
        }
    }
    let _ = session_load::repair_duplicate_conversation_record_ids(
        session_id,
        &mut runtime_state.conversation_state,
    );
    Ok(())
}

/// Returns whether a runtime state already carries concrete transcript context.
/// 返回运行态是否已经携带具体转录上下文。
fn runtime_state_has_concrete_conversation_context(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.conversation_state.messages.is_empty()
        || !runtime_state.conversation_state.records.is_empty()
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

/// Ensures a project summary exists and owns the provided session id exactly once.
/// 确保项目摘要存在，并且只拥有一次给定会话标识。
fn ensure_project_summary_for_session(
    projects: &mut Vec<TaskProjectSummary>,
    project_id: &str,
    project_name: &str,
    session_id: SessionId,
) {
    if let Some(project) = projects.iter_mut().find(|project| project.id == project_id) {
        if !project.session_ids.contains(&session_id) {
            project.session_ids.push(session_id);
        }
        return;
    }
    projects.push(TaskProjectSummary::new(
        project_id.to_string(),
        project_name.to_string(),
        String::new(),
        false,
        projects.len(),
        None,
        None,
        vec![session_id],
    ));
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
DELETE FROM projects;
DELETE FROM workspace_session_index;
DELETE FROM app_metadata WHERE key = 'workspace.collapsed_task_projects';
"#,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("清理旧原型工作台数据失败：{error}"))?;
    clear_all_segmented_session_roots(&controller.data_root)?;

    Ok(())
}

/// Persists the latest current context into the segmented session state file when the directory exists.
/// 当分段会话目录存在时，将最新当前上下文写入分段会话状态文件。
pub fn persist_segmented_session_context_current_document(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let Some(session_dir) =
        find_existing_segmented_context_session_dir(data_root, project_id, session_id)?
    else {
        return Ok(());
    };
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    write_state(&session_dir, &state_document)
}

/// Backs up provider-facing runtime JSONL files before context compaction changes a session.
/// 在上下文压缩修改会话前备份 Provider 可见 runtime JSONL 文件。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should be copied.
/// `session_id` 标识需要复制运行时日志的会话。
/// `compaction_id` is the logical compaction identifier used in the backup directory name.
/// `compaction_id` 是用于备份目录命名的逻辑压缩标识。
/// `timestamp` is the compaction creation timestamp used in the backup directory name.
/// `timestamp` 是用于备份目录命名的压缩创建时间戳。
///
/// # Returns
/// 返回值
/// Relative paths copied into the runtime backup directory; missing sessions return an empty list.
/// 已复制到运行时备份目录的相对路径；会话缺失时返回空列表。
pub fn backup_session_runtime_log_before_compaction(
    data_root: &std::path::Path,
    session_id: SessionId,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    backup_runtime_log_before_compaction(&session_dir, compaction_id, timestamp)
}

/// Records the provider-facing injection view applied to one session turn.
/// 记录单个会话回合已应用的 Provider 可见注入视图。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the audit event.
/// `session_id` 标识需要写入审计事件的会话。
/// `turn_id` is the logical prompt turn identifier when available.
/// `turn_id` 是可用时的逻辑提示词回合标识。
/// `combined_fingerprint` identifies the complete provider-facing injection overlay.
/// `combined_fingerprint` 标识完整的 Provider 可见注入覆盖层。
/// `persistent_blocks` are the persistent system-layer injection blocks.
/// `persistent_blocks` 是长期系统层注入块。
/// `transient_prefix` is the transient user-overlay injection text.
/// `transient_prefix` 是动态用户覆盖注入文本。
/// `merged_baseline_fingerprint` identifies the folded baseline after compaction when known.
/// `merged_baseline_fingerprint` 标识已知的压缩后折叠基线。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_applied(
    data_root: &std::path::Path,
    session_id: SessionId,
    turn_id: Option<String>,
    combined_fingerprint: Option<String>,
    persistent_blocks: &[String],
    transient_prefix: Option<&str>,
    merged_baseline_fingerprint: Option<String>,
    timestamp: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(false);
    };
    let persistent_fingerprint = injection_text_parts_fingerprint(persistent_blocks);
    let transient_fingerprint =
        transient_prefix.and_then(|prefix| injection_text_fingerprint(prefix));
    let source_node_ids = injection_applied_source_node_ids(persistent_blocks, transient_prefix);
    record_runtime_injection_applied(
        &session_dir,
        timestamp,
        vulcan_context::RuntimeInjectionAppliedEvent {
            turn_id,
            combined_fingerprint,
            persistent_fingerprint,
            transient_fingerprint,
            has_transient: transient_prefix.is_some_and(|prefix| !prefix.trim().is_empty()),
            merged_baseline_fingerprint,
            source_node_ids,
        },
    )
}

/// Builds a stable fingerprint for one non-empty injection text block.
/// 为单个非空注入文本块构建稳定指纹。
fn injection_text_fingerprint(text: &str) -> Option<String> {
    let normalized = text.trim();
    if normalized.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Builds a stable fingerprint for an ordered list of provider-facing injection blocks.
/// 为一组有序 Provider 可见注入块构建稳定指纹。
fn injection_text_parts_fingerprint(parts: &[String]) -> Option<String> {
    let normalized_parts = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if normalized_parts.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    normalized_parts.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Builds stable source identifiers for applied injection blocks without storing injection text.
/// 为已应用注入块构建稳定来源标识，同时不存储注入正文。
fn injection_applied_source_node_ids(
    persistent_blocks: &[String],
    transient_prefix: Option<&str>,
) -> Vec<String> {
    let mut source_node_ids = persistent_blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            injection_text_fingerprint(block)
                .map(|fingerprint| format!("persistent:{index}:{fingerprint}"))
        })
        .collect::<Vec<_>>();
    if let Some(fingerprint) = transient_prefix.and_then(injection_text_fingerprint) {
        source_node_ids.push(format!("transient:0:{fingerprint}"));
    }
    source_node_ids
}

/// Records that an injection source was cancelled and must not enter future provider requests.
/// 记录某个注入来源已取消，后续不得进入 Provider 请求。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the cancellation event.
/// `session_id` 标识需要写入取消事件的会话。
/// `event` describes the cancelled source nodes and previous fingerprints.
/// `event` 描述被取消的来源节点和旧指纹。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_cancelled(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionCancelledEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        data_root,
        session_id,
        vulcan_context::RuntimeLogEvent::InjectionCancelled(event),
        timestamp,
    )
}

/// Records that persistent injection content was merged into a stable baseline.
/// 记录长期注入内容已合并进稳定基线。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the merge event.
/// `session_id` 标识需要写入合并事件的会话。
/// `event` describes the merged baseline and included source nodes.
/// `event` 描述合并后的基线和纳入的来源节点。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_merged(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        data_root,
        session_id,
        vulcan_context::RuntimeLogEvent::InjectionMerged(event),
        timestamp,
    )
}

/// Records one runtime audit event for a segmented session when the session can be found.
/// 当能够找到分段会话时，为该会话记录单个运行时审计事件。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the audit event.
/// `session_id` 标识需要写入审计事件的会话。
/// `event` is the runtime-only event to append to `runtime.jsonl`.
/// `event` 是需要追加到 `runtime.jsonl` 的仅运行时事件。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when the runtime event was written; missing sessions or unavailable runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志不可用时返回 `false`。
pub fn record_session_runtime_audit_event(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeLogEvent,
    timestamp: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(false);
    };
    record_runtime_audit_event(&session_dir, timestamp, event)
}

/// Loads chronological runtime system updates for one segmented session.
/// 加载单个分段会话的时间顺序 runtime system update。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should be scanned.
/// `session_id` 标识需要扫描 runtime 日志的会话。
///
/// # Returns
/// 返回值
/// Runtime system updates in log order; missing sessions or missing runtime logs return an empty list.
/// 按日志顺序返回 runtime system update；会话或 runtime 日志缺失时返回空列表。
pub fn load_session_runtime_system_updates(
    data_root: &std::path::Path,
    session_id: SessionId,
) -> Result<Vec<vulcan_context::RuntimeSystemUpdateEvent>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(lines) = read_runtime_log_lines(&session_dir)? else {
        return Ok(Vec::new());
    };
    Ok(lines
        .into_iter()
        .filter_map(|line| match line.event {
            vulcan_context::RuntimeLogEvent::SystemUpdate(event) => Some(event),
            _ => None,
        })
        .collect())
}

/// Loads lightweight child-task mirrors from the parent session metadata without reading child transcripts.
/// 从父会话元数据加载轻量子任务镜像，不读取子任务转录正文。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the parent session whose child task mirrors should be read.
/// `session_id` 标识需要读取子任务镜像的父会话。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`; missing sessions return an empty list.
/// 来自父级 `metadata.json` 的轻量任务镜像；会话缺失时返回空列表。
pub fn load_segmented_session_task_mirrors(
    data_root: &std::path::Path,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    load_segmented_session_task_mirrors_from_dir(&session_dir)
}

/// Loads lightweight child-task mirrors by checking only one known owning project.
/// 仅检查一个已知所属项目来加载轻量子任务镜像。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `project_id` is the known segmented project tag for the parent session.
/// `project_id` 是父会话已知的分段项目标签。
/// `session_id` identifies the parent session whose child task mirrors should be read.
/// `session_id` 标识需要读取子任务镜像的父会话。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`; missing sessions return an empty list.
/// 来自父级 `metadata.json` 的轻量任务镜像；会话缺失时返回空列表。
pub fn load_segmented_session_task_mirrors_for_project(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let session_id_text = session_id.to_string();
    let active_dir =
        find_segmented_session_directory(data_root, false, project_id, &session_id_text)?;
    let session_dir = match active_dir {
        Some(session_dir) => Some(session_dir),
        None => find_segmented_session_directory(data_root, true, project_id, &session_id_text)?,
    };
    let Some(session_dir) = session_dir else {
        return Ok(Vec::new());
    };
    load_segmented_session_task_mirrors_from_dir(&session_dir)
}

/// Loads lightweight child-task mirrors from one already resolved session directory.
/// 从一个已解析的会话目录加载轻量子任务镜像。
///
/// # Parameters
/// 参数
/// `session_dir` points at the resolved segmented parent session directory.
/// `session_dir` 指向已解析的分段父会话目录。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`.
/// 来自父级 `metadata.json` 的轻量任务镜像行。
fn load_segmented_session_task_mirrors_from_dir(
    session_dir: &std::path::Path,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let metadata = segmented_session_store::read_session_metadata(session_dir)?;
    Ok(metadata
        .tasks
        .into_iter()
        .filter_map(|task| {
            Some(SessionTaskMirrorSnapshot {
                task_session_id: SessionId::parse_str(&task.session_id).ok()?,
                status: task.status,
                line_count: task.line_count,
                updated_at: task.updated_at,
            })
        })
        .collect())
}

/// Appends a pre-prompt turn-context anchor to the segmented session stream when available.
/// 可用时在分段会话流中追加用户提示词前的轮次上下文锚点。
pub fn snapshot_segmented_session_context_before_user_prompt(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    if state_document.current_context.is_null() {
        return Ok(());
    }
    let Some(session_dir) =
        find_existing_segmented_context_session_dir(data_root, project_id, session_id)?
    else {
        return Ok(());
    };
    append_segmented_session_events(
        &session_dir,
        &[SegmentedSessionEventInput {
            timestamp: current_unix_millis_text(),
            kind: SegmentedSessionEventKind::TurnContext,
            payload: serde_json::json!({
                "reason": "before_user_prompt",
                "current_context": state_document.current_context,
                "runtime_state": state_document.runtime_state,
                "source_updated_at": state_document.updated_at,
            }),
            turn_index: None,
        }],
    )?;
    Ok(())
}

/// session boundary store adapter backed by one workspace segmented session directory.
/// 由单个工作区分段会话目录支撑的 会话边界存储适配器。
#[derive(Debug, Clone)]
pub struct WorkspaceSegmentedSessionBoundaryStore {
    /// Vulcan code data root.
    /// Vulcan Code 数据根目录。
    data_root: std::path::PathBuf,
    /// Owning project identifier and project tag.
    /// 所属项目标识与项目标签。
    project_id: String,
    /// Owning session identifier.
    /// 所属会话标识。
    session_id: SessionId,
    /// Human-readable session title used when the directory must be created.
    /// 需要创建目录时使用的人类可读会话标题。
    session_title: String,
}

impl WorkspaceSegmentedSessionBoundaryStore {
    /// Creates one session boundary store adapter for a workspace segmented session.
    /// 为工作区分段会话创建一个 会话边界存储适配器。
    pub fn new(
        data_root: impl Into<std::path::PathBuf>,
        project_id: impl Into<String>,
        session_id: SessionId,
        session_title: impl Into<String>,
    ) -> Self {
        Self {
            data_root: data_root.into(),
            project_id: project_id.into(),
            session_id,
            session_title: session_title.into(),
        }
    }

    /// Resolves or creates the backing segmented session directory.
    /// 解析或创建背后的分段会话目录。
    fn ensure_session_dir(&self) -> Result<std::path::PathBuf, String> {
        ensure_session_boundary_dir(
            &self.data_root,
            &self.project_id,
            self.session_id,
            &self.session_title,
        )
    }
}

impl SessionBoundaryStore for WorkspaceSegmentedSessionBoundaryStore {
    /// Appends one session boundary event into the segmented session stream.
    /// 向分段会话流追加一条 会话边界事件。
    fn append_event(&self, write: SessionBoundaryStorageWrite) -> Result<(), String> {
        if write.thread_id != SessionNodeId::from(self.session_id) {
            return Err(format!(
                "Session boundary write thread `{}` does not match workspace session `{}`.",
                write.thread_id, self.session_id
            ));
        }
        let session_dir = self.ensure_session_dir()?;
        append_segmented_boundary_events(&session_dir, std::slice::from_ref(&write.event))?;
        Ok(())
    }

    /// Reads session boundary events from the segmented session stream.
    /// 从分段会话流读取 会话边界事件。
    fn read_events(&self, thread_id: SessionNodeId) -> Result<Vec<SessionBoundaryEvent>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(Vec::new());
        }
        load_segmented_session_boundary_events(
            &self.data_root,
            &self.project_id,
            self.session_id,
            None,
            usize::MAX,
        )
    }
}

impl SessionBoundaryStateStore for WorkspaceSegmentedSessionBoundaryStore {
    /// Replaces only the session lifecycle runner state inside state.json.
    /// 仅覆盖 state.json 内的 session lifecycle runner 状态。
    fn replace_state(&self, thread_id: SessionNodeId, state: Value) -> Result<(), String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Err(format!(
                "Session boundary state thread `{}` does not match workspace session `{}`.",
                thread_id, self.session_id
            ));
        }
        let session_dir = self.ensure_session_dir()?;
        replace_session_boundary_state(&session_dir, state)
    }

    /// Loads the session lifecycle runner state from state.json.
    /// 从 state.json 读取 session lifecycle runner 状态。
    fn load_state(&self, thread_id: SessionNodeId) -> Result<Option<Value>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(None);
        }
        let session_id_text = self.session_id.to_string();
        let Some(session_dir) = find_segmented_session_directory(
            &self.data_root,
            false,
            &self.project_id,
            &session_id_text,
        )?
        else {
            return Ok(None);
        };
        load_session_boundary_state(&session_dir)
    }
}

/// Appends accepted session boundary events into the segmented session stream.
/// 将已接受的 会话边界事件追加到分段会话流。
pub fn append_segmented_session_boundary_events(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
    events: &[SessionBoundaryEvent],
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let session_dir =
        ensure_session_boundary_dir(data_root, project_id, session_id, session_title)?;
    append_segmented_boundary_events(&session_dir, events)?;
    Ok(())
}

/// Resolves or creates the segmented session directory used by session boundary storage.
/// 解析或创建 会话边界存储使用的分段会话目录。
fn ensure_session_boundary_dir(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
) -> Result<std::path::PathBuf, String> {
    let session_id_text = session_id.to_string();
    let now = current_unix_millis_text();
    match find_segmented_session_directory(data_root, false, project_id, &session_id_text)? {
        Some(session_dir) => Ok(session_dir),
        None => create_segmented_session_in_location(
            data_root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id_text,
                project_tag: project_id.to_string(),
                created_date: segmented_session_date_from_unix_millis_text(&now),
                created_at: now,
                project_id: Some(project_id.to_string()),
                title: Some(session_title.to_string()),
                title_source: Some(String::from("session_boundary")),
                segment_policy: SegmentPolicy::default(),
            },
        ),
    }
}

/// Loads session boundary events from one segmented session window.
/// 从一个分段会话窗口读取 会话边界事件。
pub fn load_segmented_session_boundary_events(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SessionBoundaryEvent>, String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) =
        find_segmented_session_directory(data_root, false, project_id, &session_id_text)?
    else {
        return Ok(Vec::new());
    };
    read_segmented_session_event_window(&session_dir, before_seq, limit)?
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .map(session_boundary_event_from_segment_line)
        .collect()
}

/// Updates display metadata for one segmented session without rewriting transcript segments.
/// 仅更新单个分段会话的展示元数据，不重写转录分段。
pub fn rename_segmented_session_metadata(
    data_root: &std::path::Path,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Err(format!("未找到分段会话目录：{session_id_text}"));
    };
    patch_segmented_session_metadata(
        &session_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(updated_at.to_string()),
            title: Some(Some(title.to_string())),
            title_source: Some(Some("manual".to_string())),
            archived_at: None,
        },
    )?;
    Ok(())
}

/// Rebuilds one session boundary event from a segmented JSONL line.
/// 从分段 JSONL 行重建一条 会话边界事件。
fn session_boundary_event_from_segment_line(
    line: &self::segmented_session_store::SegmentedSessionEventLine,
) -> Result<SessionBoundaryEvent, String> {
    let payload = &line.payload;
    let event_id = required_string(payload, "event_id")?;
    let thread_id = serde_json::from_value::<SessionNodeId>(required_value(payload, "thread_id")?)
        .map_err(|error| format!("invalid session boundary thread_id: {error}"))?;
    let kind = serde_json::from_value::<SessionBoundaryEventKind>(required_value(payload, "kind")?)
        .map_err(|error| format!("invalid session boundary kind: {error}"))?;
    let scope = serde_json::from_value::<SessionEventScope>(required_value(payload, "scope")?)
        .map_err(|error| format!("invalid session boundary scope: {error}"))?;
    let visibility =
        serde_json::from_value::<SessionEventVisibility>(required_value(payload, "visibility")?)
            .map_err(|error| format!("invalid session boundary visibility: {error}"))?;
    let timestamp_ms = line.timestamp.parse::<u64>().unwrap_or_default();
    Ok(SessionBoundaryEvent {
        event_id,
        thread_id,
        turn_id: optional_string(payload, "turn_id"),
        item_id: optional_string(payload, "item_id"),
        sequence: payload
            .get("sequence")
            .and_then(Value::as_u64)
            .unwrap_or(line.seq),
        kind,
        scope,
        visibility,
        timestamp_ms,
        payload: payload.get("payload").cloned().unwrap_or(Value::Null),
    })
}

/// Finds the active or archived segmented session directory used by context persistence.
/// 查找上下文持久化使用的活动或归档分段会话目录。
fn find_existing_segmented_context_session_dir(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<std::path::PathBuf>, String> {
    match find_segmented_session_directory(data_root, false, project_id, &session_id.to_string())? {
        Some(session_dir) => Ok(Some(session_dir)),
        None => {
            find_segmented_session_directory(data_root, true, project_id, &session_id.to_string())
        }
    }
}

/// Converts Unix-milliseconds text into the UTC date segment used by segmented session directories.
/// 将 Unix 毫秒文本转换为分段会话目录使用的 UTC 日期片段。
fn segmented_session_date_from_unix_millis_text(value: &str) -> String {
    let millis = value.trim().parse::<i64>().unwrap_or_default();
    let seconds = millis.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Converts days since the Unix epoch into one proleptic Gregorian UTC date.
/// 将 Unix 纪元以来的天数转换为一个前推格里高利 UTC 日期。
fn civil_date_from_unix_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_unix_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Returns the current controller-scoped SQLite binding identifier used by the application workspace tables.
/// 返回应用工作台表当前使用的控制器作用域 SQLite 绑定标识符。
fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
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

/// Queries lightweight session locator rows ordered without scanning segmented directories.
/// 查询轻量会话定位行，不扫描分段目录。
async fn query_session_index_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionIndexRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT
    id,
    project_id,
    project_tag,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
FROM workspace_session_index
ORDER BY archived ASC, project_id ASC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台会话定位索引失败：{error}"))?;

    decode_query_rows::<StoredSessionIndexRow>(&result.json_data)
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
/// Returns the maximum message rows fetched for one turn-level transcript window.
/// 返回单个轮次级转录窗口允许拉取的最大消息行数。
fn session_transcript_row_limit_for_turns(turn_limit: usize) -> usize {
    SESSION_TRANSCRIPT_MIN_ROW_LIMIT
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

/// Returns no controller runtime documents because segmented state files are now the startup source.
/// 不再返回控制器运行态文档，因为分段 state 文件已经是启动事实源。
async fn query_session_runtime_state_shell_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredRuntimeStateRow>, String> {
    let _ = controller;
    Ok(Vec::new())
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

/// Converts one legacy runtime document into the in-memory runtime-state shape.
/// 将一个旧运行态文档转换为内存中的运行时状态结构。
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
        tool_call_record_ids: Default::default(),
        last_provider_usage: provider_usage.clone(),
        last_successful_provider_usage: provider_usage,
        last_safe_provider_request_count: None,
        active_precheck_turn_id: None,
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

    Uuid::parse_str(normalized)
        .map_err(|error| format!("解析持久化会话标识失败：{normalized}，{error}"))
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

/// Returns a required cloned JSON value from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个必需的 JSON 值副本。
fn required_value(payload: &Value, key: &str) -> Result<Value, String> {
    payload
        .get(key)
        .cloned()
        .ok_or_else(|| format!("session boundary payload missing `{key}`"))
}

/// Returns a required string from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个必需字符串。
fn required_string(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("session boundary payload missing string `{key}`"))
}

/// Returns an optional string from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个可选字符串。
fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key)?.as_str().map(str::to_string)
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

//! File-backed session event-store helpers used as the canonical transcript source.
//! 作为规范转录真相源使用的文件化会话事件存储辅助实现。

#![allow(dead_code)]

use super::*;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use vulcan_context_injection::ContextInjectionState;
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_workbench_model::VmmProfileSyncState;

/// Maximum body size kept inline inside one persisted event row before the full text is moved into a blob file.
/// 单条持久化事件行允许内联保留正文的最大大小，超过后会将完整文本移动到 blob 文件中。
const SESSION_EVENT_INLINE_BODY_MAX_BYTES: usize = 64 * 1024;
/// Maximum preview characters retained inside one persisted event row when the full body is stored in a blob.
/// 当完整正文被写入 blob 时，单条持久化事件行内保留的最大预览字符数。
const SESSION_EVENT_INLINE_PREVIEW_CHARS: usize = 4096;
/// Stable relative path of the single-segment event stream used by the current implementation.
/// 当前实现使用的单段事件流稳定相对路径。
const SESSION_EVENT_SEGMENT_RELATIVE_PATH: &str = "events/000001.jsonl";
/// Stable runtime-state cache file name stored beside the session manifest.
/// 与会话 manifest 并列保存的运行时状态缓存文件名。
const SESSION_EVENT_RUNTIME_STATE_FILE_NAME: &str = "runtime_state.json";
/// Stable checkpoint directory name stored beside the session manifest.
/// 与会话 manifest 并列保存的稳定检查点目录名称。
const SESSION_EVENT_CHECKPOINT_DIRECTORY_NAME: &str = "checkpoints";
/// Stable context directory name stored beside the session manifest.
/// 与会话 manifest 并列保存的稳定上下文目录名称。
const SESSION_EVENT_CONTEXT_DIRECTORY_NAME: &str = "context";
/// Stable current-context file name stored inside the context directory.
/// 保存于上下文目录内部的稳定当前上下文文件名。
const SESSION_EVENT_CONTEXT_CURRENT_FILE_NAME: &str = "current.json";
/// Stable snapshot directory name stored inside the context directory.
/// 保存于上下文目录内部的稳定快照目录名称。
const SESSION_EVENT_CONTEXT_SNAPSHOT_DIRECTORY_NAME: &str = "snapshots";
/// Maximum number of historical context snapshots retained for one session.
/// 单个会话最多保留的历史上下文快照数量。
const SESSION_EVENT_CONTEXT_SNAPSHOT_RETAIN_LIMIT: usize = 5;

/// Compact manifest written beside one persisted session event stream.
/// 与单个持久化会话事件流一同写入的紧凑 manifest。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SessionEventStoreManifest {
    /// Stable session identifier used by the store directory.
    /// 供存储目录使用的稳定会话标识。
    pub session_id: String,
    /// Stable project identifier owning the session store.
    /// 拥有该会话存储的稳定项目标识。
    pub project_id: String,
    /// Human-readable persisted session title.
    /// 持久化的人类可读会话标题。
    pub title: String,
    /// Persisted title-source key.
    /// 持久化标题来源键。
    pub title_source: String,
    /// Latest persisted turn index derived from the event stream.
    /// 从事件流派生出的最近持久化轮次索引。
    pub latest_turn_index: i64,
    /// Latest persisted event sequence derived from the event stream.
    /// 从事件流派生出的最近持久化事件序号。
    pub latest_event_seq: i64,
    /// Optional active compaction identifier referenced by the runtime-state cache.
    /// 由运行时状态缓存引用的可选活动压缩标识。
    pub active_checkpoint_id: Option<String>,
    /// Relative event-segment paths retained by the manifest.
    /// 由 manifest 保留的事件分段相对路径集合。
    pub event_segments: Vec<String>,
    /// Relative runtime-state cache file path.
    /// 运行时状态缓存文件的相对路径。
    pub runtime_state_file: String,
    /// Relative checkpoint document paths retained by the manifest.
    /// 由 manifest 保留的检查点文档相对路径集合。
    #[serde(default)]
    pub checkpoint_files: Vec<String>,
    /// Relative current-context file path retained by the manifest.
    /// 由 manifest 保留的当前上下文文件相对路径。
    #[serde(default = "default_context_current_relative_path")]
    pub context_current_file: String,
    /// Relative context-snapshot directory retained by the manifest.
    /// 由 manifest 保留的上下文快照目录相对路径。
    #[serde(default = "default_context_snapshot_directory_relative_path")]
    pub context_snapshot_dir: String,
    /// Creation timestamp captured for the manifest.
    /// 为 manifest 捕获的创建时间戳。
    pub created_at: String,
    /// Update timestamp captured for the manifest.
    /// 为 manifest 捕获的更新时间戳。
    pub updated_at: String,
}

/// Small runtime-state cache persisted beside the canonical event stream.
/// 与规范事件流并列持久化的小型运行时状态缓存。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SessionEventStoreRuntimeState {
    /// Last active agent identifier used by the session.
    /// 最近一次被该会话使用的激活代理标识。
    pub active_agent_id: Option<String>,
    /// Last active provider identifier used by the session.
    /// 最近一次被该会话使用的激活供应商标识。
    pub active_provider_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    pub active_model_id: Option<String>,
    /// Last active OpenRouter endpoint route locked by the session.
    /// 最近一次被该会话锁定使用的 OpenRouter 端点路由。
    pub active_openrouter_endpoint_route:
        Option<crate::provider_config::OpenRouterEndpointRouteSelection>,
    /// Last reasoning-effort preset selected while this session was active.
    /// 最近一次该会话激活时选择的推理档位预设。
    #[serde(default)]
    pub active_reasoning_effort: Option<crate::provider_config::ReasoningEffortLevel>,
    /// Last optional temperature override selected while this session was active.
    /// 最近一次该会话激活时选择的可选温度覆盖。
    #[serde(default)]
    pub active_temperature: Option<f32>,
    /// Session-level prompt skill state overrides.
    /// 会话级提示词技能状态覆盖。
    #[serde(default)]
    pub prompt_skill_overrides: BTreeMap<String, crate::provider_config::PersistedPromptSkillState>,
    /// Hidden context-injection runtime state kept outside transcript records.
    /// 保留在转录记录之外的隐藏上下文注入运行态。
    #[serde(default)]
    pub context_injection_state: ContextInjectionState,
    /// Session-level VMM memory integration overrides.
    /// 会话级 VMM 记忆集成覆盖配置。
    #[serde(default, skip_serializing_if = "VmmConfigLayer::is_empty")]
    pub vmm: VmmConfigLayer,
    /// Pending VMM writebacks retained for FIFO replay.
    /// 为 FIFO 重放保留的待处理 VMM 写回项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vmm_writeback_outbox: Vec<VmmWritebackOutboxEntry>,
    /// Session-scoped VMM profile synchronization state.
    /// 会话级 VMM 画像同步状态。
    #[serde(default, skip_serializing_if = "VmmProfileSyncState::is_empty")]
    pub vmm_profile_sync: VmmProfileSyncState,
    /// Latest active compaction identifier retained by the session runtime.
    /// 被会话运行时保留的最近活动压缩标识。
    pub last_compaction_id: Option<String>,
    /// Current context-start message identifier.
    /// 当前上下文起始消息标识。
    pub context_start_message_id: Option<String>,
    /// Live budget fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时预算指纹。
    #[serde(default)]
    pub live_context_budget_fingerprint: Option<String>,
    /// Live layered-context fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时分层上下文指纹。
    #[serde(default)]
    pub live_context_assembly_fingerprint: Option<String>,
    /// Anchor budget fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点预算指纹。
    #[serde(default, alias = "last_context_budget_fingerprint")]
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Anchor layered-context fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点分层上下文指纹。
    #[serde(default, alias = "last_context_assembly_fingerprint")]
    pub anchor_context_assembly_fingerprint: Option<String>,
    /// Last persisted provider-request preview Markdown used by the request-preview transcript tab after hydration.
    /// 供水合后请求预览转录页签使用的最近一次持久化 Provider 请求预览 Markdown。
    #[serde(default)]
    pub last_provider_request_preview_markdown: Option<String>,
    /// Persisted transcript UI state.
    /// 持久化转录界面状态。
    pub ui_state: SessionTranscriptUiState,
    /// Persisted provider continuation payload.
    /// 持久化 Provider continuation 载荷。
    pub continuation: SessionProviderContinuationState,
    /// Persisted latest provider usage snapshot.
    /// 持久化最近一次 Provider 用量快照。
    pub last_provider_usage: Option<ProviderUsageSnapshot>,
    /// Persisted latest successful provider usage snapshot.
    /// 持久化最近一次成功 Provider 用量快照。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Update timestamp captured for the runtime-state cache.
    /// 为运行时状态缓存捕获的更新时间戳。
    pub updated_at: String,
}

/// Canonical current-context document persisted beside one session event stream.
/// 与单个会话事件流并列持久化保存的规范当前上下文文档。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SessionEventStoreContextDocument {
    /// Stable session identifier owning the current context.
    /// 拥有当前上下文的稳定会话标识。
    pub session_id: String,
    /// Stable project identifier owning the current context.
    /// 拥有当前上下文的稳定项目标识。
    pub project_id: String,
    /// Optional latest active compaction identifier mirrored into the current context.
    /// 镜像进入当前上下文的可选最近活动压缩标识。
    pub last_compaction_id: Option<String>,
    /// Optional context-start message identifier mirrored into the current context.
    /// 镜像进入当前上下文的可选上下文起始消息标识。
    pub context_start_message_id: Option<String>,
    /// Optional live budget fingerprint mirrored into the current context.
    /// 镜像进入当前上下文的可选实时预算指纹。
    #[serde(default)]
    pub live_context_budget_fingerprint: Option<String>,
    /// Optional live layered-context fingerprint mirrored into the current context.
    /// 镜像进入当前上下文的可选实时分层上下文指纹。
    #[serde(default)]
    pub live_context_assembly_fingerprint: Option<String>,
    /// Optional anchor budget fingerprint mirrored into the current context.
    /// 镜像进入当前上下文的可选锚点预算指纹。
    #[serde(default, alias = "last_context_budget_fingerprint")]
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Optional anchor layered-context fingerprint mirrored into the current context.
    /// 镜像进入当前上下文的可选锚点分层上下文指纹。
    #[serde(default, alias = "last_context_assembly_fingerprint")]
    pub anchor_context_assembly_fingerprint: Option<String>,
    /// Last persisted provider-request preview Markdown mirrored into the current context.
    /// 镜像进入当前上下文的最近一次持久化 Provider 请求预览 Markdown。
    #[serde(default)]
    pub last_provider_request_preview_markdown: Option<String>,
    /// Unix-millis text describing when the current context was last refreshed.
    /// 描述当前上下文最近刷新时间的 Unix 毫秒文本。
    pub updated_at: String,
    /// Canonical runtime conversation state used by future model requests.
    /// 被未来模型请求使用的规范运行时会话状态。
    pub conversation_state: SessionConversationState,
}

/// Historical context snapshot captured immediately before one new user prompt enters the current context.
/// 在一条新的用户提示进入当前上下文之前立即捕获的历史上下文快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SessionEventStoreContextSnapshotDocument {
    /// Stable snapshot identifier derived from capture time and randomness.
    /// 根据捕获时间与随机因子派生出的稳定快照标识。
    pub snapshot_id: String,
    /// Stable session identifier owning the snapshot.
    /// 拥有该快照的稳定会话标识。
    pub session_id: String,
    /// Stable project identifier owning the snapshot.
    /// 拥有该快照的稳定项目标识。
    pub project_id: String,
    /// Stable capture reason used by future context-management workflows.
    /// 被未来上下文管理工作流使用的稳定捕获原因。
    pub reason: String,
    /// Unix-millis text describing when the snapshot was captured.
    /// 描述该快照何时被捕获的 Unix 毫秒文本。
    pub created_at: String,
    /// Unix-millis text copied from the source current-context document.
    /// 从源当前上下文文档复制而来的 Unix 毫秒文本。
    pub source_updated_at: String,
    /// Frozen context document copied from the previous current-context state.
    /// 从上一份当前上下文状态复制出的冻结上下文文档。
    pub context: SessionEventStoreContextDocument,
}

/// File-backed compaction checkpoint document persisted beside the canonical event stream.
/// 与规范事件流并列持久化的文件化压缩检查点文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ContextCompactionCheckpoint {
    /// Stable compaction identifier.
    /// 稳定压缩标识。
    pub id: String,
    /// Last source message included by the compaction input window.
    /// 被压缩输入窗口覆盖的最后一条源消息标识。
    pub source_until_message_id: Option<String>,
    /// Last source user-turn index included by the compaction input window.
    /// 被压缩输入窗口覆盖的最后一个源用户轮次索引。
    pub source_until_turn_index: Option<i64>,
    /// First message identifier that should seed the next active context window.
    /// 应作为下一段活跃上下文窗口起点的第一条消息标识。
    pub context_start_message_id: Option<String>,
    /// Stable budget fingerprint captured when this checkpoint was produced.
    /// 产出该检查点时捕获的稳定预算指纹。
    #[serde(default)]
    pub context_budget_fingerprint: Option<String>,
    /// Stable layered-context fingerprint captured when this checkpoint was produced.
    /// 产出该检查点时捕获的稳定分层上下文指纹。
    #[serde(default)]
    pub context_assembly_fingerprint: Option<String>,
    /// First retained user-turn index that remained verbatim after compaction.
    /// 压缩后仍按原样保留的第一个用户轮次索引。
    pub retained_tail_start_turn_index: Option<i64>,
    /// Hidden summary message emitted by the compaction pass.
    /// 压缩步骤输出的隐藏摘要消息标识。
    pub summary_message_id: Option<String>,
    /// Creation timestamp captured for the checkpoint.
    /// 为检查点捕获的创建时间戳。
    pub created_at: String,
}

/// One persisted canonical event row stored inside the session JSONL stream.
/// 存储在会话 JSONL 流中的单条规范事件行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedSessionEventRow {
    /// Stable message identifier shared with transcript projection surfaces.
    /// 与转录投影表面共享的稳定消息标识。
    pub event_id: String,
    /// Stable turn identifier derived from the current turn state machine.
    /// 从当前轮次状态机派生出的稳定轮次标识。
    pub turn_id: String,
    /// Stable turn index derived from user prompts, interventions, and terminal markers.
    /// 根据用户提示、插入消息与终止标记派生出的稳定轮次索引。
    pub turn_index: i64,
    /// Stable event sequence used by pagination and scroll anchoring.
    /// 被分页与滚动锚定使用的稳定事件序号。
    pub seq: i64,
    /// Stable event sequence relative to the owning turn.
    /// 相对于所属轮次的稳定事件序号。
    pub seq_in_turn: i64,
    /// High-level persisted message kind.
    /// 持久化的高层消息类型。
    pub kind: SessionMessageKind,
    /// Durable record kind preserved for transcript and context reconstruction.
    /// 为转录与上下文重建保留的持久化记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Whether the event should appear in the visible transcript projection.
    /// 该事件是否应出现在可见转录投影中。
    pub visible: bool,
    /// Optional transcript-card title.
    /// 可选的转录卡片标题。
    pub title: String,
    /// Inline body text or blob preview retained on the event row.
    /// 保留在事件行上的内联正文文本或 blob 预览文本。
    pub body: String,
    /// Optional relative blob path holding the full body.
    /// 保存完整正文的可选相对 blob 路径。
    pub body_blob_path: Option<String>,
    /// Optional structured host metadata.
    /// 可选的结构化宿主元数据。
    pub metadata: Option<Value>,
    /// Event creation timestamp captured as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式捕获的事件创建时间戳。
    pub created_at: String,
}

/// One persisted turn-index row derived from the canonical event stream.
/// 从规范事件流派生出的单条持久化轮次索引行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionTurnIndexPersistenceRow {
    /// Stable turn identifier.
    /// 稳定轮次标识。
    pub turn_id: String,
    /// Stable turn index.
    /// 稳定轮次索引。
    pub turn_index: i64,
    /// First event sequence belonging to the turn.
    /// 归属于该轮次的首个事件序号。
    pub start_seq: i64,
    /// Last event sequence belonging to the turn.
    /// 归属于该轮次的最后一个事件序号。
    pub end_seq: i64,
    /// Stable terminal state label for the turn.
    /// 该轮次的稳定终态标签。
    pub turn_state: String,
    /// Optional summary message identifier that closed the turn.
    /// 闭合该轮次的可选摘要消息标识。
    pub summary_message_id: Option<String>,
}

/// One prepared workspace-history search document derived from the event stream.
/// 从事件流派生出的单条预处理工作台历史搜索文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionEventSearchDocument {
    /// Stable document identifier used by controller-managed FTS.
    /// 供控制器托管 FTS 使用的稳定文档标识。
    pub document_id: String,
    /// Stable session identifier owning the document.
    /// 拥有该文档的稳定会话标识。
    pub session_id: String,
    /// Stable project identifier owning the document.
    /// 拥有该文档的稳定项目标识。
    pub project_id: String,
    /// Stable event identifier backing the document.
    /// 作为该文档后端来源的稳定事件标识。
    pub event_id: String,
    /// Stable turn index owning the event.
    /// 拥有该事件的稳定轮次索引。
    pub turn_index: i64,
    /// Stable event sequence used as the ordinal-like search anchor.
    /// 作为类 ordinal 搜索锚点使用的稳定事件序号。
    pub ordinal: i64,
    /// Persisted role label used by search-hit projection.
    /// 供搜索命中投影使用的持久化角色标签。
    pub role: String,
    /// Event creation timestamp used by result metadata.
    /// 供结果元数据使用的事件创建时间戳。
    pub created_at: String,
    /// Final searchable content pushed into FTS.
    /// 被推入 FTS 的最终可搜索文本内容。
    pub content: String,
}

/// Persist result returned after one session event store has been rewritten.
/// 单个会话事件存储被重写后返回的持久化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionEventStorePersistResult {
    /// Latest persisted turn index.
    /// 最近持久化轮次索引。
    pub latest_turn_index: i64,
    /// Latest persisted event sequence.
    /// 最近持久化事件序号。
    pub latest_event_seq: i64,
    /// Rebuilt turn-index rows that should be mirrored into SQLite.
    /// 需要镜像到 SQLite 的重建轮次索引行集合。
    pub turn_rows: Vec<SessionTurnIndexPersistenceRow>,
    /// Rebuilt search documents that should be mirrored into controller-managed FTS.
    /// 需要镜像到控制器托管 FTS 的重建搜索文档集合。
    pub search_documents: Vec<SessionEventSearchDocument>,
}

/// Loaded session event rows plus the manifest metadata required by transcript hydration.
/// 已加载的会话事件行以及转录水合所需的 manifest 元数据。
#[derive(Debug, Clone)]
pub(super) struct SessionEventStoreLoadResult {
    /// Persisted manifest decoded from disk.
    /// 从磁盘解码出的持久化 manifest。
    pub manifest: SessionEventStoreManifest,
    /// Fully materialized event rows with blob-backed bodies restored.
    /// 已恢复 blob 正文的完整事件行集合。
    pub rows: Vec<PersistedSessionEventRow>,
}

/// Persists one session event store from the in-memory transcript snapshot, appending when the existing stream is a valid prefix.
/// 根据内存中的转录快照持久化单个会话事件存储；当已有事件流是有效前缀时采用追加。
pub(super) fn persist_session_event_store(
    session: &SessionSummary,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
    compactions: &[SessionCompactionEntry],
) -> Result<SessionEventStorePersistResult, String> {
    let project_id = session
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "持久化会话事件存储失败：会话 `{}` 缺少项目归属。",
                session.id
            )
        })?;
    let store_dir = session_event_store_dir(project_id, session.id);
    let events_path = session_event_segment_path(&store_dir);
    let runtime_state_path = session_event_runtime_state_path(&store_dir);
    let blob_dir = session_event_blob_dir(&store_dir);
    let checkpoint_dir = session_event_checkpoint_dir(&store_dir);
    let context_current_path = session_event_context_current_path(&store_dir);
    let (rows, turn_rows, search_documents) =
        build_persisted_session_event_rows(project_id, session.id, messages);

    let append_result = try_append_session_event_store_if_prefix_matches(
        &store_dir,
        &events_path,
        &blob_dir,
        &rows,
    )?;
    if !append_result.appended_or_unchanged {
        recreate_session_store_directory(&store_dir)?;
    }
    fs::create_dir_all(&blob_dir).map_err(|error| {
        format!(
            "创建会话事件 blob 目录失败（{}）：{error}",
            blob_dir.display()
        )
    })?;
    fs::create_dir_all(&checkpoint_dir).map_err(|error| {
        format!(
            "创建会话事件 checkpoint 目录失败（{}）：{error}",
            checkpoint_dir.display()
        )
    })?;
    if !append_result.appended_or_unchanged {
        write_session_event_rows(&events_path, &blob_dir, &rows)?;
    }

    let runtime_state_document = runtime_state
        .map(runtime_state_to_store_document)
        .unwrap_or_default();
    write_runtime_state_document(&runtime_state_path, &runtime_state_document)?;
    let context_document = runtime_state
        .map(|runtime_state| {
            runtime_state_to_context_document(project_id, session.id, runtime_state)
        })
        .unwrap_or_else(|| session_messages_to_context_document(project_id, session.id, messages));
    write_context_document(&context_current_path, &context_document)?;
    let checkpoint_files = write_compaction_checkpoint_documents(&checkpoint_dir, compactions)?;

    let latest_turn_index = turn_rows.last().map(|row| row.turn_index).unwrap_or(-1);
    let latest_event_seq = rows.last().map(|row| row.seq).unwrap_or(-1);
    let manifest = SessionEventStoreManifest {
        session_id: session.id.to_string(),
        project_id: project_id.to_string(),
        title: session.title.clone(),
        title_source: session.title_source.as_persisted_key().to_string(),
        latest_turn_index,
        latest_event_seq,
        active_checkpoint_id: runtime_state.and_then(|entry| entry.last_compaction_id.clone()),
        event_segments: vec![String::from(SESSION_EVENT_SEGMENT_RELATIVE_PATH)],
        runtime_state_file: String::from(SESSION_EVENT_RUNTIME_STATE_FILE_NAME),
        checkpoint_files,
        context_current_file: default_context_current_relative_path(),
        context_snapshot_dir: default_context_snapshot_directory_relative_path(),
        created_at: append_result
            .existing_manifest
            .as_ref()
            .map(|manifest| manifest.created_at.clone())
            .unwrap_or_else(current_unix_millis_text),
        updated_at: current_unix_millis_text(),
    };
    write_session_manifest(&session_event_manifest_path(&store_dir), &manifest)?;

    Ok(SessionEventStorePersistResult {
        latest_turn_index,
        latest_event_seq,
        turn_rows,
        search_documents,
    })
}

/// Loads one whole session event store from disk and restores blob-backed bodies eagerly.
/// 从磁盘加载单个完整会话事件存储并急切恢复 blob 正文。
pub(super) fn load_session_event_store(
    project_id: &str,
    session_id: SessionId,
) -> Result<SessionEventStoreLoadResult, String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    ensure_session_event_store_manifest_if_recoverable(project_id, session_id)?;
    let manifest_path = session_event_manifest_path(&store_dir);
    let manifest = read_json_file::<SessionEventStoreManifest>(&manifest_path)?;
    let events_path = session_event_segment_path(&store_dir);
    let rows = read_session_event_rows(&events_path, &session_event_blob_dir(&store_dir))?;
    Ok(SessionEventStoreLoadResult { manifest, rows })
}

/// Loads the small runtime-state cache stored beside one session event stream.
/// 加载保存在单个会话事件流旁边的小型运行时状态缓存。
pub(super) fn load_session_event_store_runtime_state(
    project_id: &str,
    session_id: SessionId,
) -> Result<SessionEventStoreRuntimeState, String> {
    let path = session_event_runtime_state_path(&session_event_store_dir(project_id, session_id));
    if !path.exists() {
        return Ok(SessionEventStoreRuntimeState::default());
    }
    read_json_file::<SessionEventStoreRuntimeState>(&path)
}

/// Loads the canonical current-context document stored beside one session event stream.
/// 加载保存在单个会话事件流旁边的规范当前上下文文档。
pub(super) fn load_session_event_store_context_current(
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<SessionEventStoreContextDocument>, String> {
    let path = session_event_context_current_path(&session_event_store_dir(project_id, session_id));
    if !path.exists() {
        return Ok(None);
    }
    read_json_file::<SessionEventStoreContextDocument>(&path).map(Some)
}

/// Loads the newest available context snapshot document for one session.
/// 加载单个会话当前可用的最新上下文快照文档。
pub(super) fn load_latest_session_event_store_context_snapshot(
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<SessionEventStoreContextSnapshotDocument>, String> {
    let snapshot_dir =
        session_event_context_snapshot_dir(&session_event_store_dir(project_id, session_id));
    if !snapshot_dir.exists() {
        return Ok(None);
    }
    let mut entries = fs::read_dir(&snapshot_dir)
        .map_err(|error| {
            format!(
                "读取会话上下文快照目录失败（{}）：{error}",
                snapshot_dir.display()
            )
        })?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    entries.reverse();
    for entry in entries {
        if let Ok(snapshot) =
            read_json_file::<SessionEventStoreContextSnapshotDocument>(&entry.path())
        {
            return Ok(Some(snapshot));
        }
    }
    Ok(None)
}

/// Persists the latest canonical current-context document for one session immediately.
/// 立即持久化单个会话的最近规范当前上下文文档。
pub fn persist_session_event_store_context_current(
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    fs::create_dir_all(&store_dir).map_err(|error| {
        format!(
            "创建会话事件存储目录失败（{}）：{error}",
            store_dir.display()
        )
    })?;
    ensure_minimal_session_event_store_manifest(project_id, session_id)?;
    let path = session_event_context_current_path(&store_dir);
    let document = runtime_state_to_context_document(project_id, session_id, runtime_state);
    write_context_document(&path, &document)
}

/// Captures a historical context snapshot immediately before one new user prompt mutates the current context.
/// 在一条新的用户提示词改变当前上下文之前立即捕获一份历史上下文快照。
pub fn snapshot_session_event_store_context_before_user_prompt(
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let current_document = runtime_state_to_context_document(project_id, session_id, runtime_state);
    if context_document_is_empty(&current_document) {
        return Ok(());
    }

    let store_dir = session_event_store_dir(project_id, session_id);
    ensure_minimal_session_event_store_manifest(project_id, session_id)?;
    let snapshot_dir = session_event_context_snapshot_dir(&store_dir);
    fs::create_dir_all(&snapshot_dir).map_err(|error| {
        format!(
            "创建会话上下文快照目录失败（{}）：{error}",
            snapshot_dir.display()
        )
    })?;
    let created_at = current_unix_millis_text();
    let snapshot_id = format!(
        "snapshot-{created_at}-{}",
        sanitize_file_component(Uuid::new_v4().to_string().as_str())
    );
    let snapshot = SessionEventStoreContextSnapshotDocument {
        snapshot_id: snapshot_id.clone(),
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        reason: String::from("before_user_prompt"),
        created_at: created_at.clone(),
        source_updated_at: current_document.updated_at.clone(),
        context: current_document,
    };
    write_json_file(
        &session_event_context_snapshot_path(&snapshot_dir, &snapshot_id),
        &snapshot,
    )?;
    prune_session_event_store_context_snapshots(&snapshot_dir)?;
    Ok(())
}

/// Loads the compaction checkpoints persisted beside one session event stream.
/// 加载保存在单个会话事件流旁边的压缩检查点集合。
pub(super) fn load_session_event_store_compactions(
    project_id: &str,
    session_id: SessionId,
) -> Result<Vec<SessionCompactionEntry>, String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    ensure_session_event_store_manifest_if_recoverable(project_id, session_id)?;
    let manifest_path = session_event_manifest_path(&store_dir);
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }
    let manifest = read_json_file::<SessionEventStoreManifest>(&manifest_path)?;
    let checkpoint_dir = session_event_checkpoint_dir(&store_dir);
    let mut entries = manifest
        .checkpoint_files
        .iter()
        .map(|relative_path| checkpoint_dir.join(relative_path))
        .map(|path| read_json_file::<ContextCompactionCheckpoint>(&path))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(checkpoint_document_to_entry)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(entries)
}

/// Permanently removes one whole session event store directory from disk.
/// 从磁盘永久移除单个完整会话事件存储目录。
pub(super) fn remove_session_event_store(
    project_id: &str,
    session_id: SessionId,
) -> Result<(), String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    if store_dir.exists() {
        fs::remove_dir_all(&store_dir).map_err(|error| {
            format!(
                "删除会话事件存储目录失败（{}）：{error}",
                store_dir.display()
            )
        })?;
    }
    Ok(())
}

/// Removes the whole canonical session-store root so bootstrap resets can rebuild from a clean state.
/// 移除完整的规范会话存储根目录，以便引导重置后从干净状态重建。
pub(super) fn clear_all_session_event_stores() -> Result<(), String> {
    let root = session_event_store_root();
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| {
            format!("删除会话事件存储根目录失败（{}）：{error}", root.display())
        })?;
    }
    Ok(())
}

/// Converts one runtime-state entry into the persisted event-store runtime-state cache shape.
/// 将单条运行时状态项转换为持久化事件存储运行时状态缓存结构。
fn runtime_state_to_store_document(
    runtime_state: &SessionRuntimeStateEntry,
) -> SessionEventStoreRuntimeState {
    SessionEventStoreRuntimeState {
        active_agent_id: runtime_state.active_agent_id.clone(),
        active_provider_id: runtime_state.active_provider_id.clone(),
        active_model_id: runtime_state.active_model_id.clone(),
        active_openrouter_endpoint_route: runtime_state.active_openrouter_endpoint_route.clone(),
        active_reasoning_effort: runtime_state.active_reasoning_effort,
        active_temperature: runtime_state.active_temperature,
        prompt_skill_overrides: runtime_state.prompt_skill_overrides.clone(),
        context_injection_state: runtime_state.context_injection_state.clone(),
        vmm: runtime_state.vmm.clone(),
        vmm_writeback_outbox: runtime_state.vmm_writeback_outbox.clone(),
        vmm_profile_sync: runtime_state.vmm_profile_sync.clone(),
        last_compaction_id: runtime_state.last_compaction_id.clone(),
        context_start_message_id: runtime_state.context_start_message_id.clone(),
        live_context_budget_fingerprint: runtime_state.live_context_budget_fingerprint.clone(),
        live_context_assembly_fingerprint: runtime_state.live_context_assembly_fingerprint.clone(),
        anchor_context_budget_fingerprint: runtime_state.anchor_context_budget_fingerprint.clone(),
        anchor_context_assembly_fingerprint: runtime_state
            .anchor_context_assembly_fingerprint
            .clone(),
        last_provider_request_preview_markdown: runtime_state
            .last_provider_request_preview_markdown
            .clone(),
        ui_state: runtime_state.ui_state.clone(),
        continuation: runtime_state.conversation_state.continuation.clone(),
        last_provider_usage: runtime_state.last_provider_usage.clone(),
        last_successful_provider_usage: runtime_state.last_successful_provider_usage.clone(),
        updated_at: runtime_state.updated_at.clone(),
    }
}

/// Converts one runtime-state entry into the persisted current-context document shape.
/// 将单条运行时状态项转换为持久化当前上下文文档结构。
fn runtime_state_to_context_document(
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> SessionEventStoreContextDocument {
    SessionEventStoreContextDocument {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        last_compaction_id: runtime_state.last_compaction_id.clone(),
        context_start_message_id: runtime_state.context_start_message_id.clone(),
        live_context_budget_fingerprint: runtime_state.live_context_budget_fingerprint.clone(),
        live_context_assembly_fingerprint: runtime_state.live_context_assembly_fingerprint.clone(),
        anchor_context_budget_fingerprint: runtime_state.anchor_context_budget_fingerprint.clone(),
        anchor_context_assembly_fingerprint: runtime_state
            .anchor_context_assembly_fingerprint
            .clone(),
        last_provider_request_preview_markdown: runtime_state
            .last_provider_request_preview_markdown
            .clone(),
        updated_at: non_empty_timestamp_text(&runtime_state.updated_at),
        conversation_state: runtime_state.conversation_state.clone(),
    }
}

/// Converts one transcript message slice into the fallback current-context document shape.
/// 将一段转录消息切片转换为兜底当前上下文文档结构。
fn session_messages_to_context_document(
    project_id: &str,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> SessionEventStoreContextDocument {
    SessionEventStoreContextDocument {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        last_compaction_id: None,
        context_start_message_id: None,
        live_context_budget_fingerprint: None,
        live_context_assembly_fingerprint: None,
        anchor_context_budget_fingerprint: None,
        anchor_context_assembly_fingerprint: None,
        last_provider_request_preview_markdown: None,
        updated_at: current_unix_millis_text(),
        conversation_state: conversation_state_from_session_messages(messages),
    }
}

/// Returns whether one current-context document still carries no meaningful runtime context.
/// 返回一份当前上下文文档是否仍然不携带任何有意义的运行时上下文。
fn context_document_is_empty(document: &SessionEventStoreContextDocument) -> bool {
    document.conversation_state.messages.is_empty()
        && document.conversation_state.records.is_empty()
        && document
            .conversation_state
            .continuation
            .provider_id
            .is_none()
        && document.conversation_state.continuation.model_id.is_none()
        && document
            .conversation_state
            .continuation
            .last_provider_message_id
            .is_none()
        && document
            .conversation_state
            .continuation
            .updated_at
            .is_none()
        && document.last_compaction_id.is_none()
        && document.context_start_message_id.is_none()
        && document.live_context_budget_fingerprint.is_none()
        && document.live_context_assembly_fingerprint.is_none()
        && document.anchor_context_budget_fingerprint.is_none()
        && document.anchor_context_assembly_fingerprint.is_none()
        && document.last_provider_request_preview_markdown.is_none()
}

/// Writes all compaction checkpoint documents for one session store rewrite and returns the relative file list captured by the manifest.
/// 为一次会话存储重写写入全部压缩检查点文档，并返回供 manifest 记录的相对文件列表。
fn write_compaction_checkpoint_documents(
    checkpoint_dir: &Path,
    compactions: &[SessionCompactionEntry],
) -> Result<Vec<String>, String> {
    let mut relative_paths = Vec::with_capacity(compactions.len());
    for entry in compactions {
        let file_name = checkpoint_file_name(entry);
        let path = checkpoint_dir.join(&file_name);
        let document = checkpoint_entry_to_document(entry);
        write_json_file(&path, &document)?;
        relative_paths.push(file_name);
    }
    Ok(relative_paths)
}

/// Converts one in-memory compaction entry into the file-backed checkpoint document shape.
/// 将单条内存压缩检查点转换为文件化检查点文档结构。
fn checkpoint_entry_to_document(entry: &SessionCompactionEntry) -> ContextCompactionCheckpoint {
    ContextCompactionCheckpoint {
        id: entry.id.clone(),
        source_until_message_id: entry.source_until_message_id.clone(),
        source_until_turn_index: entry.source_until_turn_index,
        context_start_message_id: entry.context_start_message_id.clone(),
        context_budget_fingerprint: entry.context_budget_fingerprint.clone(),
        context_assembly_fingerprint: entry.context_assembly_fingerprint.clone(),
        retained_tail_start_turn_index: entry.retained_tail_start_turn_index,
        summary_message_id: entry.summary_message_id.clone(),
        created_at: entry.created_at.clone(),
    }
}

/// Converts one file-backed checkpoint document into the in-memory compaction entry shape.
/// 将单条文件化检查点文档转换为内存压缩检查点结构。
fn checkpoint_document_to_entry(document: ContextCompactionCheckpoint) -> SessionCompactionEntry {
    SessionCompactionEntry {
        id: document.id,
        source_until_message_id: document.source_until_message_id,
        source_until_turn_index: document.source_until_turn_index,
        context_start_message_id: document.context_start_message_id,
        context_budget_fingerprint: document.context_budget_fingerprint,
        context_assembly_fingerprint: document.context_assembly_fingerprint,
        retained_tail_start_turn_index: document.retained_tail_start_turn_index,
        summary_message_id: document.summary_message_id,
        created_at: document.created_at,
    }
}

/// Builds persisted event rows, turn-index rows, and search documents from one in-memory transcript slice.
/// 根据一段内存转录切片构建持久化事件行、轮次索引行与搜索文档。
fn build_persisted_session_event_rows(
    project_id: &str,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> (
    Vec<PersistedSessionEventRow>,
    Vec<SessionTurnIndexPersistenceRow>,
    Vec<SessionEventSearchDocument>,
) {
    let mut rows = Vec::with_capacity(messages.len());
    let mut turn_rows = Vec::new();
    let mut search_documents = Vec::new();
    let mut active_turn_index = -1_i64;
    let mut active_turn_open = false;
    let mut active_turn_state = String::from("open");
    let mut active_turn_start_seq = 0_i64;
    let mut active_turn_summary_message_id = None::<String>;
    let mut seq_in_turn = 0_i64;
    let mut turn_closed_waiting_for_stats = false;

    for (position, message) in messages.iter().enumerate() {
        let next_is_turn_stats = messages.get(position + 1).is_some_and(|next_message| {
            next_message.record_kind == SessionMessageRecordKind::TurnStats
        });
        let opens_turn = session_message_starts_new_event_turn(message);
        if opens_turn && !active_turn_open {
            active_turn_index = active_turn_index.saturating_add(1);
            active_turn_open = true;
            active_turn_state = String::from("open");
            active_turn_start_seq = position as i64;
            active_turn_summary_message_id = None;
            seq_in_turn = 0;
        } else if !active_turn_open {
            active_turn_index = active_turn_index.max(0);
            active_turn_open = true;
            active_turn_state = String::from("open");
            active_turn_start_seq = position as i64;
            active_turn_summary_message_id = None;
            seq_in_turn = 0;
        }

        let seq = position as i64;
        let turn_id = format!("turn-{active_turn_index:06}");
        let row = PersistedSessionEventRow {
            event_id: non_empty_event_id(session_id, position, &message.id),
            turn_id: turn_id.clone(),
            turn_index: active_turn_index,
            seq,
            seq_in_turn,
            kind: message.kind,
            record_kind: message.record_kind,
            visible: message.visible,
            title: message.title.clone(),
            body: message.body.clone(),
            body_blob_path: None,
            metadata: message.metadata.clone(),
            created_at: non_empty_timestamp_text(&message.created_at),
        };
        rows.push(row.clone());

        if event_row_is_workspace_history_search_indexable(message)
            && !message.body.trim().is_empty()
        {
            search_documents.push(SessionEventSearchDocument {
                document_id: row.event_id.clone(),
                session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                event_id: row.event_id.clone(),
                turn_index: active_turn_index,
                ordinal: seq,
                role: role_from_session_kind(message.kind).to_string(),
                created_at: row.created_at.clone(),
                content: message.body.clone(),
            });
        }

        if message.record_kind == SessionMessageRecordKind::SummaryOutput {
            active_turn_state = String::from("summarized");
            active_turn_summary_message_id = Some(row.event_id.clone());
        } else if message.record_kind == SessionMessageRecordKind::TurnTerminated {
            active_turn_state = String::from("terminated");
        }

        seq_in_turn = seq_in_turn.saturating_add(1);

        if active_turn_open && turn_should_close_after_message(message) {
            if next_is_turn_stats {
                turn_closed_waiting_for_stats = true;
                continue;
            }
            turn_rows.push(SessionTurnIndexPersistenceRow {
                turn_id,
                turn_index: active_turn_index,
                start_seq: active_turn_start_seq,
                end_seq: seq,
                turn_state: active_turn_state.clone(),
                summary_message_id: active_turn_summary_message_id.clone(),
            });
            active_turn_open = false;
            active_turn_state = String::from("open");
            active_turn_summary_message_id = None;
            turn_closed_waiting_for_stats = false;
            continue;
        }

        if turn_closed_waiting_for_stats && !next_is_turn_stats {
            turn_rows.push(SessionTurnIndexPersistenceRow {
                turn_id,
                turn_index: active_turn_index,
                start_seq: active_turn_start_seq,
                end_seq: seq,
                turn_state: active_turn_state.clone(),
                summary_message_id: active_turn_summary_message_id.clone(),
            });
            active_turn_open = false;
            active_turn_state = String::from("open");
            active_turn_summary_message_id = None;
            turn_closed_waiting_for_stats = false;
            continue;
        }

        if position + 1 == messages.len() {
            if !active_turn_open && !turn_closed_waiting_for_stats {
                continue;
            }
            let persisted_end_seq = if turn_closed_waiting_for_stats {
                seq
            } else {
                seq
            };
            turn_rows.push(SessionTurnIndexPersistenceRow {
                turn_id,
                turn_index: active_turn_index,
                start_seq: active_turn_start_seq,
                end_seq: persisted_end_seq,
                turn_state: active_turn_state.clone(),
                summary_message_id: active_turn_summary_message_id.clone(),
            });
            active_turn_open = false;
            turn_closed_waiting_for_stats = false;
        }
    }

    turn_rows.dedup_by(|left, right| left.turn_index == right.turn_index);
    (rows, turn_rows, search_documents)
}

/// Writes one session manifest as formatted JSON.
/// 以格式化 JSON 写入单个会话 manifest。
fn write_session_manifest(path: &Path, manifest: &SessionEventStoreManifest) -> Result<(), String> {
    write_json_file(path, manifest)
}

/// Ensures a recoverable session store always owns a minimal manifest before lazy loaders require it.
/// 确保可恢复的会话存储在懒加载器依赖之前始终拥有最小 manifest。
fn ensure_session_event_store_manifest_if_recoverable(
    project_id: &str,
    session_id: SessionId,
) -> Result<(), String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    let manifest_path = session_event_manifest_path(&store_dir);
    if manifest_path.exists() || !session_event_store_has_recoverable_payload(&store_dir) {
        return Ok(());
    }
    ensure_minimal_session_event_store_manifest(project_id, session_id)
}

/// Ensures one concrete session store has a minimal manifest even before the first full transcript rewrite lands.
/// 确保单个具体会话存储即使在首次完整转录重写落地前也具备最小 manifest。
fn ensure_minimal_session_event_store_manifest(
    project_id: &str,
    session_id: SessionId,
) -> Result<(), String> {
    let store_dir = session_event_store_dir(project_id, session_id);
    let manifest_path = session_event_manifest_path(&store_dir);
    if manifest_path.exists() {
        return Ok(());
    }
    fs::create_dir_all(&store_dir).map_err(|error| {
        format!(
            "创建最小会话事件存储目录失败（{}）：{error}",
            store_dir.display()
        )
    })?;
    let now = current_unix_millis_text();
    let manifest = SessionEventStoreManifest {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        title: String::new(),
        title_source: String::from("generated"),
        latest_turn_index: -1,
        latest_event_seq: -1,
        active_checkpoint_id: None,
        event_segments: vec![String::from(SESSION_EVENT_SEGMENT_RELATIVE_PATH)],
        runtime_state_file: String::from(SESSION_EVENT_RUNTIME_STATE_FILE_NAME),
        checkpoint_files: Vec::new(),
        context_current_file: default_context_current_relative_path(),
        context_snapshot_dir: default_context_snapshot_directory_relative_path(),
        created_at: now.clone(),
        updated_at: now,
    };
    write_session_manifest(&manifest_path, &manifest)
}

/// Writes one runtime-state cache document as formatted JSON.
/// 以格式化 JSON 写入单个运行时状态缓存文档。
fn write_runtime_state_document(
    path: &Path,
    runtime_state: &SessionEventStoreRuntimeState,
) -> Result<(), String> {
    write_json_file(path, runtime_state)
}

/// Writes one current-context document as formatted JSON.
/// 以格式化 JSON 写入单个当前上下文文档。
fn write_context_document(
    path: &Path,
    context: &SessionEventStoreContextDocument,
) -> Result<(), String> {
    write_json_file(path, context)
}

/// Result of trying to persist a session event stream through the append fast path.
/// 尝试通过追加快路径持久化会话事件流后的结果。
struct SessionEventStoreAppendAttempt {
    /// Existing manifest preserved when the append path can keep the original creation timestamp.
    /// 追加路径可保留原创建时间时使用的既有 manifest。
    existing_manifest: Option<SessionEventStoreManifest>,
    /// Whether the event stream was appended or already matched the requested rows.
    /// 事件流是否已经追加完成或本来就匹配请求行。
    appended_or_unchanged: bool,
}

/// Appends only the new event rows when the stored event stream is a semantic prefix of the requested snapshot.
/// 当已存事件流是请求快照的语义前缀时，仅追加新的事件行。
fn try_append_session_event_store_if_prefix_matches(
    store_dir: &Path,
    events_path: &Path,
    blob_dir: &Path,
    rows: &[PersistedSessionEventRow],
) -> Result<SessionEventStoreAppendAttempt, String> {
    let manifest_path = session_event_manifest_path(store_dir);
    if !manifest_path.exists() || !events_path.exists() {
        return Ok(SessionEventStoreAppendAttempt {
            existing_manifest: None,
            appended_or_unchanged: false,
        });
    }
    let existing_manifest = read_json_file::<SessionEventStoreManifest>(&manifest_path).ok();
    let existing_rows = match read_session_event_rows(events_path, blob_dir) {
        Ok(rows) => rows,
        Err(_) => {
            return Ok(SessionEventStoreAppendAttempt {
                existing_manifest,
                appended_or_unchanged: false,
            });
        }
    };
    if existing_rows.len() > rows.len() {
        return Ok(SessionEventStoreAppendAttempt {
            existing_manifest,
            appended_or_unchanged: false,
        });
    }
    let prefix_matches = existing_rows
        .iter()
        .zip(rows.iter())
        .all(|(left, right)| persisted_event_rows_match_semantically(left, right));
    if !prefix_matches {
        return Ok(SessionEventStoreAppendAttempt {
            existing_manifest,
            appended_or_unchanged: false,
        });
    }
    append_session_event_rows(events_path, blob_dir, &rows[existing_rows.len()..])?;
    Ok(SessionEventStoreAppendAttempt {
        existing_manifest,
        appended_or_unchanged: true,
    })
}

/// Writes one whole JSONL event stream and externalizes oversized bodies into blob files.
/// 写入完整 JSONL 事件流，并将超大正文外置到 blob 文件。
fn write_session_event_rows(
    events_path: &Path,
    blob_dir: &Path,
    rows: &[PersistedSessionEventRow],
) -> Result<(), String> {
    if let Some(parent) = events_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("创建会话事件流父目录失败（{}）：{error}", parent.display())
        })?;
    }
    let file = fs::File::create(events_path).map_err(|error| {
        format!(
            "创建会话事件流文件失败（{}）：{error}",
            events_path.display()
        )
    })?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        let stored_row = externalize_event_body_if_needed(blob_dir, row)?;
        let line = serde_json::to_string(&stored_row)
            .map_err(|error| format!("序列化会话事件行失败：{error}"))?;
        writer
            .write_all(line.as_bytes())
            .map_err(|error| format!("写入会话事件行失败（{}）：{error}", events_path.display()))?;
        writer.write_all(b"\n").map_err(|error| {
            format!("写入会话事件换行失败（{}）：{error}", events_path.display())
        })?;
    }
    writer.flush().map_err(|error| {
        format!(
            "刷新会话事件流文件失败（{}）：{error}",
            events_path.display()
        )
    })
}

/// Appends new JSONL event rows and externalizes oversized bodies into blob files.
/// 追加新的 JSONL 事件行，并将超大正文外置到 blob 文件。
fn append_session_event_rows(
    events_path: &Path,
    blob_dir: &Path,
    rows: &[PersistedSessionEventRow],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    if let Some(parent) = events_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("创建会话事件流父目录失败（{}）：{error}", parent.display())
        })?;
    }
    fs::create_dir_all(blob_dir).map_err(|error| {
        format!(
            "创建会话事件 blob 目录失败（{}）：{error}",
            blob_dir.display()
        )
    })?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(events_path)
        .map_err(|error| {
            format!(
                "打开会话事件流追加失败（{}）：{error}",
                events_path.display()
            )
        })?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        let stored_row = externalize_event_body_if_needed(blob_dir, row)?;
        let line = serde_json::to_string(&stored_row)
            .map_err(|error| format!("序列化会话事件行失败：{error}"))?;
        writer
            .write_all(line.as_bytes())
            .map_err(|error| format!("追加会话事件行失败（{}）：{error}", events_path.display()))?;
        writer.write_all(b"\n").map_err(|error| {
            format!("追加会话事件换行失败（{}）：{error}", events_path.display())
        })?;
    }
    writer.flush().map_err(|error| {
        format!(
            "刷新追加会话事件流失败（{}）：{error}",
            events_path.display()
        )
    })
}

/// Reads the full JSONL event stream and restores blob-backed bodies eagerly.
/// 读取完整 JSONL 事件流并急切恢复 blob 正文。
fn read_session_event_rows(
    events_path: &Path,
    blob_dir: &Path,
) -> Result<Vec<PersistedSessionEventRow>, String> {
    if !events_path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(events_path).map_err(|error| {
        format!(
            "打开会话事件流文件失败（{}）：{error}",
            events_path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| {
            format!(
                "读取会话事件流文本行失败（{}）：{error}",
                events_path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let mut row = serde_json::from_str::<PersistedSessionEventRow>(&line)
            .map_err(|error| format!("解析会话事件行失败（{}）：{error}", events_path.display()))?;
        if let Some(blob_path) = row.body_blob_path.as_deref() {
            let full_path = blob_dir.join(blob_path);
            row.body = fs::read_to_string(&full_path).map_err(|error| {
                format!(
                    "读取会话事件正文 blob 失败（{}）：{error}",
                    full_path.display()
                )
            })?;
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Returns whether two event rows represent the same canonical transcript event, ignoring storage-only blob placement.
/// 返回两条事件行是否表示同一个规范转录事件，并忽略仅用于存储的 blob 放置差异。
fn persisted_event_rows_match_semantically(
    left: &PersistedSessionEventRow,
    right: &PersistedSessionEventRow,
) -> bool {
    left.event_id == right.event_id
        && left.turn_id == right.turn_id
        && left.turn_index == right.turn_index
        && left.seq == right.seq
        && left.seq_in_turn == right.seq_in_turn
        && left.kind == right.kind
        && left.record_kind == right.record_kind
        && left.visible == right.visible
        && left.title == right.title
        && left.body == right.body
        && left.metadata == right.metadata
        && left.created_at == right.created_at
}

/// Externalizes one oversized event body into a blob file while keeping a short inline preview on the event row.
/// 将单条超大事件正文外置到 blob 文件，并在事件行上保留简短内联预览。
fn externalize_event_body_if_needed(
    blob_dir: &Path,
    row: &PersistedSessionEventRow,
) -> Result<PersistedSessionEventRow, String> {
    if row.body.len() <= SESSION_EVENT_INLINE_BODY_MAX_BYTES {
        return Ok(row.clone());
    }
    let mut stored_row = row.clone();
    let blob_name = session_event_body_blob_name(row);
    let blob_path = blob_dir.join(&blob_name);
    fs::write(&blob_path, row.body.as_bytes()).map_err(|error| {
        format!(
            "写入会话事件正文 blob 失败（{}）：{error}",
            blob_path.display()
        )
    })?;
    stored_row.body =
        truncate_preview_lines(row.body.as_str(), 12, SESSION_EVENT_INLINE_PREVIEW_CHARS);
    stored_row.body_blob_path = Some(blob_name);
    Ok(stored_row)
}

/// Returns whether one transcript message should open a new persisted event turn.
/// 返回某条转录消息是否应开启新的持久化事件轮次。
fn session_message_starts_new_event_turn(message: &SessionMessageEntry) -> bool {
    matches!(message.kind, SessionMessageKind::User)
        && message.visible
        && message.contributes_to_context()
}

/// Returns whether the currently processed turn should be considered closed after the message.
/// 返回当前处理中的轮次在该消息之后是否应被视为闭合。
fn turn_should_close_after_message(message: &SessionMessageEntry) -> bool {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::SummaryOutput | SessionMessageRecordKind::TurnTerminated
    )
}

/// Returns whether one canonical event row should enter the lightweight workspace-history BM25 projection.
/// 返回某条规范事件行是否应进入轻量工作台历史 BM25 投影。
fn event_row_is_workspace_history_search_indexable(message: &SessionMessageEntry) -> bool {
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

/// Recreates one concrete session store directory before a full rewrite.
/// 在完整重写前重建单个具体会话存储目录。
fn recreate_session_store_directory(store_dir: &Path) -> Result<(), String> {
    if store_dir.exists() {
        fs::remove_dir_all(store_dir).map_err(|error| {
            format!(
                "删除旧会话事件存储目录失败（{}）：{error}",
                store_dir.display()
            )
        })?;
    }
    fs::create_dir_all(store_dir).map_err(|error| {
        format!(
            "创建会话事件存储目录失败（{}）：{error}",
            store_dir.display()
        )
    })
}

/// Returns whether one session store directory still contains durable payload worthy of manifest self-healing.
/// 返回单个会话存储目录是否仍包含值得进行 manifest 自愈的持久化载荷。
fn session_event_store_has_recoverable_payload(store_dir: &Path) -> bool {
    session_event_segment_path(store_dir).exists()
        || session_event_runtime_state_path(store_dir).exists()
        || session_event_context_current_path(store_dir).exists()
        || directory_has_json_files(&session_event_context_snapshot_dir(store_dir))
        || directory_has_json_files(&session_event_checkpoint_dir(store_dir))
}

/// Returns whether one directory exists and currently contains at least one JSON file.
/// 返回某个目录是否存在且当前至少包含一个 JSON 文件。
fn directory_has_json_files(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
}

/// Returns the stable root directory used by all session event stores.
/// 返回全部会话事件存储共用的稳定根目录。
fn session_event_store_root() -> PathBuf {
    crate::data_plane::application_data_root().join("sessions")
}

/// Returns the stable directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定目录。
fn session_event_store_dir(project_id: &str, session_id: SessionId) -> PathBuf {
    session_event_store_root()
        .join(sanitize_file_component(project_id))
        .join(session_id.to_string())
}

/// Returns the public stable directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的公开稳定目录。
pub fn session_event_store_directory(project_id: &str, session_id: SessionId) -> PathBuf {
    session_event_store_dir(project_id, session_id)
}

/// Returns the stable manifest path used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定 manifest 路径。
fn session_event_manifest_path(store_dir: &Path) -> PathBuf {
    store_dir.join("manifest.json")
}

/// Returns the stable event-segment path used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定事件分段路径。
fn session_event_segment_path(store_dir: &Path) -> PathBuf {
    store_dir.join("events").join("000001.jsonl")
}

/// Returns the stable blob directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定 blob 目录。
fn session_event_blob_dir(store_dir: &Path) -> PathBuf {
    store_dir.join("blobs")
}

/// Returns the stable checkpoint directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定 checkpoint 目录。
fn session_event_checkpoint_dir(store_dir: &Path) -> PathBuf {
    store_dir.join(SESSION_EVENT_CHECKPOINT_DIRECTORY_NAME)
}

/// Returns the stable runtime-state cache path used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定运行时状态缓存路径。
fn session_event_runtime_state_path(store_dir: &Path) -> PathBuf {
    store_dir.join(SESSION_EVENT_RUNTIME_STATE_FILE_NAME)
}

/// Returns the stable context directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定上下文目录。
fn session_event_context_dir(store_dir: &Path) -> PathBuf {
    store_dir.join(SESSION_EVENT_CONTEXT_DIRECTORY_NAME)
}

/// Returns the stable current-context document path used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定当前上下文文档路径。
fn session_event_context_current_path(store_dir: &Path) -> PathBuf {
    session_event_context_dir(store_dir).join(SESSION_EVENT_CONTEXT_CURRENT_FILE_NAME)
}

/// Returns the stable snapshot directory used by one concrete session event store.
/// 返回单个具体会话事件存储使用的稳定快照目录。
fn session_event_context_snapshot_dir(store_dir: &Path) -> PathBuf {
    session_event_context_dir(store_dir).join(SESSION_EVENT_CONTEXT_SNAPSHOT_DIRECTORY_NAME)
}

/// Returns the stable path used by one concrete context snapshot document.
/// 返回单个具体上下文快照文档使用的稳定路径。
fn session_event_context_snapshot_path(snapshot_dir: &Path, snapshot_id: &str) -> PathBuf {
    snapshot_dir.join(format!("{snapshot_id}.json"))
}

/// Removes old snapshot documents so each session keeps only the latest bounded context history.
/// 移除旧的快照文档，以便每个会话只保留最近的有界上下文历史。
fn prune_session_event_store_context_snapshots(snapshot_dir: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(snapshot_dir)
        .map_err(|error| {
            format!(
                "读取会话上下文快照目录失败（{}）：{error}",
                snapshot_dir.display()
            )
        })?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    entries.reverse();
    for stale_entry in entries
        .into_iter()
        .skip(SESSION_EVENT_CONTEXT_SNAPSHOT_RETAIN_LIMIT)
    {
        fs::remove_file(stale_entry.path()).map_err(|error| {
            format!(
                "删除旧会话上下文快照失败（{}）：{error}",
                stale_entry.path().display()
            )
        })?;
    }
    Ok(())
}

/// Returns one deterministic checkpoint file name derived from the checkpoint creation timestamp and identifier.
/// 返回一个根据检查点创建时间与标识派生出的确定性 checkpoint 文件名。
fn checkpoint_file_name(entry: &SessionCompactionEntry) -> String {
    format!(
        "{}-{}.json",
        sanitize_file_component(entry.created_at.as_str()),
        sanitize_file_component(entry.id.as_str())
    )
}

/// Returns one deterministic fallback event identifier when the source transcript row still lacks a stable id.
/// 当来源转录行仍然缺少稳定标识时，返回一个确定性的兜底事件标识。
fn non_empty_event_id(session_id: SessionId, index: usize, event_id: &str) -> String {
    let trimmed = event_id.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("session-{session_id}-event-{index}")
}

/// Returns one deterministic blob file name derived from the event identifier and body hash.
/// 返回一个根据事件标识与正文哈希派生出的确定性 blob 文件名。
fn session_event_body_blob_name(row: &PersistedSessionEventRow) -> String {
    format!(
        "{:016x}-{}.txt",
        row.seq,
        sanitize_file_component(row.event_id.as_str())
    )
}

/// Serializes one JSON document with pretty formatting.
/// 以格式化方式序列化单个 JSON 文档。
fn write_json_file<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("创建 JSON 文档父目录失败（{}）：{error}", parent.display())
        })?;
    }
    let payload = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 JSON 文档失败：{error}"))?;
    fs::write(path, payload.as_bytes())
        .map_err(|error| format!("写入 JSON 文档失败（{}）：{error}", path.display()))
}

/// Reads one JSON document from disk.
/// 从磁盘读取单个 JSON 文档。
fn read_json_file<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let payload = fs::read_to_string(path)
        .map_err(|error| format!("读取 JSON 文档失败（{}）：{error}", path.display()))?;
    serde_json::from_str::<T>(&payload)
        .map_err(|error| format!("解析 JSON 文档失败（{}）：{error}", path.display()))
}

/// Returns one sanitized file-system component that stays stable across restarts.
/// 返回一个跨重启保持稳定的已清洗文件系统路径片段。
fn sanitize_file_component(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::from("_");
    }
    trimmed
        .chars()
        .map(|ch| match ch {
            ':' | '\\' | '/' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

/// Returns the default relative current-context path persisted in the session manifest.
/// 返回持久化在会话 manifest 中的默认当前上下文相对路径。
fn default_context_current_relative_path() -> String {
    format!(
        "{}/{}",
        SESSION_EVENT_CONTEXT_DIRECTORY_NAME, SESSION_EVENT_CONTEXT_CURRENT_FILE_NAME
    )
}

/// Returns the default relative context-snapshot directory persisted in the session manifest.
/// 返回持久化在会话 manifest 中的默认上下文快照目录相对路径。
fn default_context_snapshot_directory_relative_path() -> String {
    format!(
        "{}/{}",
        SESSION_EVENT_CONTEXT_DIRECTORY_NAME, SESSION_EVENT_CONTEXT_SNAPSHOT_DIRECTORY_NAME
    )
}

#[cfg(test)]
mod tests {
    //! Event-store tests covering turn-boundary semantics and blob externalization.
    //! 事件存储测试，覆盖 turn 边界语义与 blob 外置行为。

    use super::*;
    use crate::conversation_protocol::{append_assistant_text_message, append_user_text_message};

    /// Builds one transcript message entry for event-store turn-building tests.
    /// 为事件存储 turn 构建测试创建一条转录消息项。
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

    /// Verifies semantic event-row matching ignores blob placement while still comparing transcript content.
    /// 验证事件行语义匹配会忽略 blob 放置差异，但仍比较转录正文内容。
    #[test]
    fn event_row_semantic_match_ignores_blob_path_only() {
        let mut stored = test_persisted_event_row("event-1", "hello");
        stored.body_blob_path = Some(String::from("blobs/event-1.txt"));
        let requested = test_persisted_event_row("event-1", "hello");
        let changed_body = test_persisted_event_row("event-1", "changed");

        assert!(persisted_event_rows_match_semantically(&stored, &requested));
        assert!(!persisted_event_rows_match_semantically(
            &stored,
            &changed_body,
        ));
    }

    /// Builds one persisted event row for append-prefix tests.
    /// 为追加前缀测试构建一条持久化事件行。
    fn test_persisted_event_row(event_id: &str, body: &str) -> PersistedSessionEventRow {
        PersistedSessionEventRow {
            event_id: event_id.to_string(),
            turn_id: String::from("turn-000000"),
            turn_index: 0,
            seq: 0,
            seq_in_turn: 0,
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("title"),
            body: body.to_string(),
            body_blob_path: None,
            metadata: None,
            created_at: String::from("1"),
        }
    }

    /// Verifies that user intervention during an active execution stays inside the same turn until the summary closes it.
    /// 验证运行中的用户插入会一直留在同一轮内，直到摘要节点将其闭合。
    #[test]
    fn event_store_turn_builder_keeps_user_intervention_inside_open_turn() {
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "开始执行",
            ),
            test_message(
                "a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "正在处理",
            ),
            test_message(
                "u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "补充一点要求",
            ),
            SessionMessageEntry {
                visible: false,
                ..test_message(
                    "summary",
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::SummaryOutput,
                    "已完成摘要",
                )
            },
        ];

        let (_, turn_rows, _) =
            build_persisted_session_event_rows("project-1", SessionId::from_u128(1), &messages);

        assert_eq!(turn_rows.len(), 1);
        assert_eq!(turn_rows[0].turn_index, 0);
        assert_eq!(turn_rows[0].turn_state, "summarized");
        assert_eq!(turn_rows[0].summary_message_id.as_deref(), Some("summary"));
    }

    /// Verifies that a terminated turn closes immediately and the next user prompt opens a fresh turn.
    /// 验证终止轮次会立即闭合，而后续新的用户提示词会开启全新轮次。
    #[test]
    fn event_store_turn_builder_starts_new_turn_after_termination() {
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "开始执行",
            ),
            test_message(
                "a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "处理中",
            ),
            SessionMessageEntry {
                visible: false,
                ..test_message(
                    "terminated",
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::TurnTerminated,
                    "执行被中止",
                )
            },
            test_message(
                "u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "重新开始新一轮",
            ),
            SessionMessageEntry {
                visible: false,
                ..test_message(
                    "summary2",
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::SummaryOutput,
                    "第二轮完成",
                )
            },
        ];

        let (_, turn_rows, _) =
            build_persisted_session_event_rows("project-1", SessionId::from_u128(2), &messages);

        assert_eq!(turn_rows.len(), 2);
        assert_eq!(turn_rows[0].turn_index, 0);
        assert_eq!(turn_rows[0].turn_state, "terminated");
        assert_eq!(turn_rows[1].turn_index, 1);
        assert_eq!(turn_rows[1].turn_state, "summarized");
        assert_eq!(turn_rows[1].summary_message_id.as_deref(), Some("summary2"));
    }

    /// Verifies that one visible turn-stats row appended after a hidden summary still belongs to the just-closed turn instead of opening a fresh turn.
    /// 验证隐藏摘要后追加的一条可见 turn 统计行仍然归属于刚闭合的轮次，而不会错误开启新轮次。
    #[test]
    fn event_store_turn_builder_keeps_turn_stats_inside_summarized_turn() {
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "执行任务",
            ),
            test_message(
                "a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "处理中",
            ),
            SessionMessageEntry {
                visible: false,
                ..test_message(
                    "summary",
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::SummaryOutput,
                    "摘要完成",
                )
            },
            test_message(
                "stats",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::TurnStats,
                "2 次调用 · 120 tokens",
            ),
        ];

        let (rows, turn_rows, _) =
            build_persisted_session_event_rows("project-1", SessionId::from_u128(3), &messages);

        assert_eq!(turn_rows.len(), 1);
        assert_eq!(turn_rows[0].turn_index, 0);
        assert_eq!(turn_rows[0].end_seq, 3);
        assert_eq!(rows[3].turn_index, 0);
        assert_eq!(rows[3].turn_id, "turn-000000");
    }

    /// Verifies compaction checkpoints round-trip through the file-backed event store without losing ordering or anchors.
    /// 验证压缩检查点经过文件化事件存储往返后不会丢失顺序或锚点信息。
    #[test]
    fn compaction_checkpoints_round_trip_through_event_store_files() {
        let project_id = "project-9";
        let session_id = SessionId::from_u128(9);
        remove_session_event_store(project_id, session_id).expect("cleanup test event-store");
        let session = SessionSummary::new(
            session_id,
            Some(String::from(project_id)),
            String::from("测试会话"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let messages = vec![test_message(
            "u1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "hello",
        )];
        let compactions = vec![
            SessionCompactionEntry {
                id: String::from("cp-1"),
                source_until_message_id: Some(String::from("u1")),
                source_until_turn_index: Some(0),
                context_start_message_id: Some(String::from("summary-1")),
                context_budget_fingerprint: Some(String::from("budget-fp-1")),
                context_assembly_fingerprint: Some(String::from("assembly-fp-1")),
                retained_tail_start_turn_index: Some(1),
                summary_message_id: Some(String::from("summary-1")),
                created_at: String::from("100"),
            },
            SessionCompactionEntry {
                id: String::from("cp-2"),
                source_until_message_id: Some(String::from("u2")),
                source_until_turn_index: Some(1),
                context_start_message_id: Some(String::from("summary-2")),
                context_budget_fingerprint: Some(String::from("budget-fp-2")),
                context_assembly_fingerprint: Some(String::from("assembly-fp-2")),
                retained_tail_start_turn_index: Some(2),
                summary_message_id: Some(String::from("summary-2")),
                created_at: String::from("200"),
            },
        ];

        let result =
            persist_session_event_store(&session, &messages, None, &compactions).expect("persist");
        assert_eq!(result.latest_turn_index, 0);

        let loaded = load_session_event_store_compactions(project_id, session.id).expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "cp-1");
        assert_eq!(loaded[1].id, "cp-2");
        assert_eq!(
            loaded[1].context_start_message_id.as_deref(),
            Some("summary-2")
        );
        assert_eq!(
            loaded[1].context_budget_fingerprint.as_deref(),
            Some("budget-fp-2")
        );
        assert_eq!(
            loaded[1].context_assembly_fingerprint.as_deref(),
            Some("assembly-fp-2")
        );
        remove_session_event_store(project_id, session.id).expect("cleanup persisted event-store");
    }

    /// Verifies that the current-context document round-trips through the event store and prefers runtime canonical state over transcript reconstruction.
    /// 验证当前上下文文档可以在事件存储中往返，并优先保留运行时规范状态而不是转录重建结果。
    #[test]
    fn current_context_round_trips_through_event_store_files() {
        let project_id = "project-context";
        let session_id = SessionId::from_u128(77);
        remove_session_event_store(project_id, session_id).expect("cleanup context store");
        let session = SessionSummary::new(
            session_id,
            Some(String::from(project_id)),
            String::from("上下文测试"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let mut runtime_state = SessionRuntimeStateEntry {
            last_compaction_id: Some(String::from("cp-1")),
            context_start_message_id: Some(String::from("summary-1")),
            live_context_budget_fingerprint: Some(String::from("budget-live-1")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live-1")),
            anchor_context_budget_fingerprint: Some(String::from("budget-fp-1")),
            anchor_context_assembly_fingerprint: Some(String::from("assembly-fp-1")),
            last_provider_request_preview_markdown: Some(String::from("# Provider Preview")),
            updated_at: String::from("123"),
            ..SessionRuntimeStateEntry::default()
        };
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            String::from("assistant-1"),
            String::from("规范上下文正文"),
        );

        persist_session_event_store(&session, &[], Some(&runtime_state), &[])
            .expect("persist context event store");
        let loaded = load_session_event_store_context_current(project_id, session_id)
            .expect("load current context")
            .expect("expected current context document");

        assert_eq!(loaded.last_compaction_id.as_deref(), Some("cp-1"));
        assert_eq!(
            loaded.live_context_budget_fingerprint.as_deref(),
            Some("budget-live-1")
        );
        assert_eq!(
            loaded.live_context_assembly_fingerprint.as_deref(),
            Some("assembly-live-1")
        );
        assert_eq!(
            loaded.anchor_context_budget_fingerprint.as_deref(),
            Some("budget-fp-1")
        );
        assert_eq!(
            loaded.anchor_context_assembly_fingerprint.as_deref(),
            Some("assembly-fp-1")
        );
        assert_eq!(
            loaded.last_provider_request_preview_markdown.as_deref(),
            Some("# Provider Preview")
        );
        assert_eq!(
            loaded.context_start_message_id.as_deref(),
            Some("summary-1")
        );
        assert_eq!(loaded.conversation_state.messages.len(), 1);
        remove_session_event_store(project_id, session_id).expect("cleanup context event-store");
    }

    /// Verifies that session-level VMM overrides survive the event-store runtime cache.
    /// 验证会话级 VMM 覆盖可以通过事件存储运行时缓存往返保留。
    #[test]
    fn runtime_state_round_trips_vmm_overrides_through_event_store_files() {
        let project_id = "project-vmm-runtime";
        let session_id = SessionId::from_u128(78);
        remove_session_event_store(project_id, session_id).expect("cleanup vmm runtime store");
        let session = SessionSummary::new(
            session_id,
            Some(String::from(project_id)),
            String::from("VMM 运行态"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let runtime_state = SessionRuntimeStateEntry {
            vmm: VmmConfigLayer {
                enabled: Some(false),
                ai_tools_enabled: Some(false),
                grpc_target: Some(String::from("127.0.0.1:50051")),
                ..VmmConfigLayer::default()
            },
            updated_at: String::from("456"),
            ..SessionRuntimeStateEntry::default()
        };

        persist_session_event_store(&session, &[], Some(&runtime_state), &[])
            .expect("persist vmm runtime state");
        let loaded = load_session_event_store_runtime_state(project_id, session_id)
            .expect("load vmm runtime state");

        assert_eq!(loaded.vmm.enabled, Some(false));
        assert_eq!(loaded.vmm.ai_tools_enabled, Some(false));
        assert_eq!(loaded.vmm.grpc_target.as_deref(), Some("127.0.0.1:50051"));
        remove_session_event_store(project_id, session_id).expect("cleanup vmm runtime store");
    }

    /// Verifies that persisting the standalone current-context document also materializes a minimal manifest for future hydration.
    /// 验证持久化独立当前上下文文档时也会生成最小 manifest，供后续水合使用。
    #[test]
    fn persisting_current_context_materializes_minimal_manifest() {
        let project_id = "project-minimal-manifest";
        let session_id = SessionId::from_u128(79);
        remove_session_event_store(project_id, session_id).expect("cleanup minimal manifest store");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            String::from("assistant-ctx"),
            String::from("上下文正文"),
        );

        persist_session_event_store_context_current(project_id, session_id, &runtime_state)
            .expect("persist standalone current context");
        let manifest_path =
            session_event_manifest_path(&session_event_store_dir(project_id, session_id));
        assert!(manifest_path.exists());
        let loaded = load_session_event_store(project_id, session_id)
            .expect("load event store after current-only persist");
        assert!(loaded.rows.is_empty());
        remove_session_event_store(project_id, session_id).expect("cleanup minimal manifest store");
    }

    /// Verifies that context snapshots are retained only for the latest five user-turn boundaries.
    /// 验证上下文快照只会为最近五个用户轮次边界保留历史副本。
    #[test]
    fn context_snapshot_capture_prunes_to_latest_five_files() {
        let project_id = "project-snapshot";
        let session_id = SessionId::from_u128(88);
        remove_session_event_store(project_id, session_id).expect("cleanup snapshot store");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_text_message(
            &mut runtime_state.conversation_state,
            String::from("user-1"),
            String::from("历史上下文"),
            Vec::new(),
        );

        for _ in 0..7 {
            snapshot_session_event_store_context_before_user_prompt(
                project_id,
                session_id,
                &runtime_state,
            )
            .expect("snapshot before prompt");
        }

        let snapshot_dir =
            session_event_context_snapshot_dir(&session_event_store_dir(project_id, session_id));
        let snapshot_count = fs::read_dir(&snapshot_dir)
            .expect("read snapshot dir")
            .filter_map(Result::ok)
            .count();
        assert_eq!(snapshot_count, SESSION_EVENT_CONTEXT_SNAPSHOT_RETAIN_LIMIT);
        remove_session_event_store(project_id, session_id).expect("cleanup snapshot event-store");
    }

    /// Verifies that the latest readable context snapshot can be recovered even when older snapshots are still present.
    /// 验证即使仍保留更早快照，也可以恢复最新且可读的上下文快照。
    #[test]
    fn load_latest_context_snapshot_returns_most_recent_snapshot() {
        let project_id = "project-latest-snapshot";
        let session_id = SessionId::from_u128(109);
        remove_session_event_store(project_id, session_id).expect("cleanup latest snapshot store");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_text_message(
            &mut runtime_state.conversation_state,
            String::from("user-1"),
            String::from("历史上下文"),
            Vec::new(),
        );

        snapshot_session_event_store_context_before_user_prompt(
            project_id,
            session_id,
            &runtime_state,
        )
        .expect("capture first snapshot");
        runtime_state.updated_at = String::from("999");
        snapshot_session_event_store_context_before_user_prompt(
            project_id,
            session_id,
            &runtime_state,
        )
        .expect("capture second snapshot");

        let snapshot = load_latest_session_event_store_context_snapshot(project_id, session_id)
            .expect("load latest snapshot")
            .expect("expected latest snapshot document");

        assert_eq!(snapshot.source_updated_at, "999");
        remove_session_event_store(project_id, session_id).expect("cleanup latest snapshot store");
    }
}

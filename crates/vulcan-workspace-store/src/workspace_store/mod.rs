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
    restore_guided_pending_prompts_for_fresh_process,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionBoundaryStateStore,
    SessionBoundaryStorageWrite, SessionBoundaryStore, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;

mod ai_credentials;
mod git_preferences;
mod goal_store;
mod persistence;
mod provider_usage;
mod runtime_state_codec;
mod segmented_session_store;
mod session_addon_store;
mod session_load;

use self::runtime_state_codec::{
    deserialize_session_runtime_state_entry, serialize_session_runtime_state_entry,
};
use self::segmented_session_store::{
    SegmentedSessionEventInput, SegmentedSessionEventKind, SegmentedSessionStateDocument,
    append_segmented_boundary_events, append_segmented_session_events,
    backup_runtime_log_before_compaction, core_vmm_rebuild_reason, load_core_vmm_controller_state,
    load_session_boundary_state, read_runtime_log_lines, read_segmented_session_event_window,
    read_session_metadata, read_state, record_runtime_audit_event,
    record_runtime_injection_applied, replace_session_boundary_state,
    reset_vmm_state_for_history_rewrite, segmented_session_directory,
    segmented_session_directory_exists, state_document_from_runtime_state,
    write_core_vmm_controller_state, write_file_bytes_atomic, write_state,
};
pub use self::segmented_session_store::{
    SessionTranscriptAppendCursor, find_segmented_session_directory_for_task_store_created_at,
};
use self::session_addon_store::read_session_addon_state;
pub use ai_credentials::{
    AiCredentialSecretRecord, create_ai_credential_secret_record,
    delete_ai_credential_secret_record, load_ai_credential_secret_record,
    replace_ai_credential_secret_record, restore_ai_credential_secret_record,
};
pub use git_preferences::{
    load_git_repository_monitoring_preferences, persist_git_repository_monitoring_preference,
};
pub use goal_store::{
    GoalAccountingOutcome, GoalResume, GoalUpdate, MAX_THREAD_GOAL_OBJECTIVE_CHARS, ThreadGoal,
    ThreadGoalObjectiveError, ThreadGoalProviderUsage, ThreadGoalStatus,
    ThreadGoalTokenBudgetError, VulcanGoalStore, normalize_thread_goal_objective,
    validate_thread_goal_token_budget,
};
pub use persistence::{
    PersistedSessionForkResult, PersistedUserPromptRollbackResult,
    advance_single_session_transcript_checkpoint, archive_project_sessions, archive_single_session,
    cleanup_missing_session_index, delete_archived_session, fork_persisted_session,
    load_single_session_transcript_checkpoint_in_directory,
    open_single_session_transcript_append_cursor_in_directory, persist_collapsed_task_project_ids,
    persist_existing_session_index_update, persist_new_project,
    persist_project_codebase_memory_enabled, persist_project_icon_update,
    persist_project_order_update, persist_project_pinned_and_order_update,
    persist_single_session_addon_state_update,
    persist_single_session_addon_state_update_in_directory,
    persist_single_session_runtime_state_update,
    persist_single_session_transcript_messages_in_directory,
    persist_single_session_transcript_tail_from_checkpoint_in_directory,
    persist_single_session_transcript_tail_in_directory,
    persist_single_session_transcript_tail_with_cursor, persist_single_session_update,
    remove_project_after_archiving_sessions, rename_project, rename_single_session,
    restore_archived_session, restore_project, retry_single_session_transcript_tail_with_cursor,
    rollback_persisted_session_before_user_prompt,
};

/// Lightweight pre-serialized metadata for one exact `state.json` update.
/// 用于一次精确 `state.json` 更新的轻量预序列化元数据。
#[derive(Debug, Clone)]
pub struct SessionRuntimeStatePersistenceProjection {
    /// Canonical string identity of the owning session.
    /// 所属会话的规范字符串身份。
    session_id: String,
    /// Runtime update timestamp copied from the projected state.
    /// 从投影运行态复制的更新时间戳。
    updated_at: String,
    /// Lightweight runtime JSON that never contains canonical conversation history.
    /// 永不包含规范会话历史的轻量运行态 JSON。
    runtime_state: Value,
    /// Explicit current-context cursor metadata stored beside the runtime JSON.
    /// 与运行态 JSON 一同保存的显式当前上下文游标元数据。
    current_context: Value,
}
pub use provider_usage::{
    PROVIDER_USAGE_SCHEMA_REVISION, ProviderAccountUsageSnapshotRecord,
    ProviderAccountUsageSummary, ProviderRateLimitSnapshotRecord, ProviderUsageAttemptRecord,
    ProviderUsageAttemptSummary, ProviderUsageDailySummary, ProviderUsageDashboard,
    ensure_provider_usage_schema, load_provider_usage_dashboard,
    load_provider_usage_migration_completed, load_provider_usage_migration_cursor,
    lock_provider_usage_writes, migrate_application_provider_usage,
    persist_provider_account_usage_snapshot, persist_provider_rate_limit_snapshot,
    persist_provider_usage_attempt, persist_provider_usage_attempt_and_rebuild_daily,
    persist_provider_usage_attempt_batch_and_rebuild_daily,
    persist_provider_usage_migration_completed, persist_provider_usage_migration_cursor,
    rebuild_provider_usage_daily,
    remove_duplicate_legacy_provider_usage_attempts_and_rebuild_daily,
};
pub use session_load::{
    load_session_display_hydration_snapshot_for_project_created_at,
    load_session_execution_hydration_snapshot_for_project_created_at,
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
/// App metadata key used to persist collapsed task-project identifiers.
/// 用于持久化已折叠任务项目标识的应用元数据键。
const COLLAPSED_TASK_PROJECTS_METADATA_KEY: &str = "workspace.collapsed_task_projects";
/// Removed hidden-user injection group that represented the historical VMM profile owner.
/// 表示历史 VMM 画像所有者的已移除隐藏用户注入分组。
const HISTORICAL_VMM_PROFILE_CONTEXT_INJECTION_TAG: &str = "vmm_profile";
/// Session-local directory that owns complete opt-in provider request traces.
/// 保存显式启用的完整 Provider 请求追踪文件的会话本地目录。
const PROVIDER_REQUEST_TRACE_DIRECTORY: &str = "provider-request-trace";
/// Session-local grouped Provider API trace directory used by native diagnostics.
/// 原生诊断使用的会话本地分组 Provider API 追踪目录。
const PROVIDER_API_TRACE_DIRECTORY: &str = "provider-api-trace";
/// Application-level directory that owns configured-inference request traces without a conversation session.
/// 保存无对话会话归属的已配置推理请求追踪的应用级目录。
const CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY: &str =
    "configured-inference-provider-request-trace";
/// Number of exact rotating files retained for each Provider request-trace owner.
/// 每个 Provider 请求追踪所有者保留的精确轮转文件数量。
pub const PROVIDER_REQUEST_TRACE_SLOT_COUNT: u64 = 4;
/// Maximum encoded bytes retained in one complete Provider request-trace slot.
/// 单个完整 Provider 请求追踪槽位保留的最大编码字节数。
pub const PROVIDER_REQUEST_TRACE_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum retained bytes across one grouped physical Provider API attempt.
/// 单个分组 Provider API 真实尝试允许保留的最大字节数。
pub const PROVIDER_API_TRACE_ATTEMPT_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum physical attempts retained inside one logical Provider API trace group.
/// 单个逻辑 Provider API 追踪组允许保留的最大真实尝试次数。
pub const PROVIDER_API_TRACE_MAX_ATTEMPTS_PER_GROUP: u32 = 32;
/// Serializes exact trace-slot replacement without directory discovery or cross-write races.
/// 串行化精确追踪槽位替换，避免目录发现与并发写入竞争。
static PROVIDER_REQUEST_TRACE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Borrowed artifacts required to persist one physical Provider API attempt.
/// 持久化一次真实 Provider API 尝试所需的借用产物。
pub struct ProviderApiTraceAttemptArtifacts<'a> {
    /// Monotonic process-local group generation used for stale-writer rejection.
    /// 用于拒绝陈旧写入者的进程内单调组代际。
    pub group_sequence: u64,
    /// Stable logical request group identifier shared by retries.
    /// 由重试共享的稳定逻辑请求组标识。
    pub group_id: &'a str,
    /// Zero-based physical attempt index within the logical group.
    /// 逻辑组内从零开始的真实尝试索引。
    pub attempt_index: u32,
    /// Group-level route and ownership metadata.
    /// 组级路由与归属元数据。
    pub group: &'a Value,
    /// Secret-safe exact dispatched request.
    /// 不含秘密的精确已分派请求。
    pub request: &'a Value,
    /// Response metadata captured independently from the raw body.
    /// 与原始正文独立捕获的响应元数据。
    pub response_meta: &'a Value,
    /// Raw retained response bytes, including partial failed responses.
    /// 保留的原始响应字节，包括失败时的部分响应。
    pub response_body: &'a [u8],
    /// Ordered transport chunk boundary records.
    /// 有序传输 chunk 边界记录。
    pub chunks: &'a [Value],
    /// Ordered WebSocket frame boundary records.
    /// 有序 WebSocket 帧边界记录。
    pub frames: &'a [Value],
    /// Attempt terminal classification and completeness facts.
    /// 尝试终态分类与完整性事实。
    pub outcome: &'a Value,
}

/// Validates that one caller-supplied segmented directory belongs to the exact requested session.
/// 校验调用方提供的分段目录属于精确请求的会话。
///
/// # Parameters
/// 参数
/// `session_directory` is the already-resolved absolute storage target and `session_id` is its expected owner.
/// `session_directory` 是已经解析的绝对存储目标，`session_id` 是其预期所有者。
///
/// # Returns
/// 返回值
/// Success only when the target metadata exists and carries the exact canonical session identity.
/// 仅在目标元数据存在且携带精确规范会话身份时成功。
fn validate_exact_segmented_session_directory_owner(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<(), String> {
    // metadata is read from the one specified directory; no sibling or catalog enumeration is permitted.
    // metadata 仅从指定目录读取；不允许枚举兄弟目录或会话目录。
    let metadata = read_session_metadata(session_directory)?;
    if metadata.session_id != session_id.to_string() {
        return Err(format!(
            "Session `{session_id}` exact segmented directory owner is `{}`.",
            metadata.session_id
        ));
    }
    Ok(())
}

/// Loads the strict Core-owned VMM state for one persisted session.
/// 加载单个持久化会话中由 Core 所有的严格 VMM 状态。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact session owner.
/// `session_id` 是精确的会话所有者。
///
/// # Returns
/// 返回值
/// A validated state, clean absence, or an explicit storage and format error.
/// 返回已校验状态、干净缺失，或明确的存储与格式错误。
pub fn load_core_session_vmm_state(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<Option<vulcan_vmm::VmmSessionMemoryState>, String> {
    // session_id_text is the canonical persisted owner spelling.
    // session_id_text 是规范的持久化所有者文本形式。
    let session_id_text = session_id.to_string();
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    load_core_vmm_controller_state(session_directory, &session_id_text)
}

/// Projects one runtime entry into lightweight `state.json` metadata without cloning history.
/// 在不克隆历史的情况下把一个运行态条目投影为轻量 `state.json` 元数据。
///
/// # Parameters
/// 参数
/// `session_id` is the exact owning session.
/// `session_id` 是精确所属会话。
/// `runtime_state` is the authoritative in-memory entry borrowed under the caller's state lock.
/// `runtime_state` 是在调用方状态锁下借用的权威内存条目。
///
/// # Returns
/// 返回值
/// A detached metadata projection that never visits or retains canonical conversation history.
/// 返回永不访问或保留规范会话历史的分离元数据投影。
pub fn project_single_session_runtime_state_update(
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<SessionRuntimeStatePersistenceProjection, String> {
    // stateDocument uses exhaustive borrowed-field serialization and excludes canonical history before allocation.
    // stateDocument 使用穷尽借用字段序列化，并在分配前排除规范历史。
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    Ok(SessionRuntimeStatePersistenceProjection {
        session_id: state_document.session_id,
        updated_at: state_document.updated_at,
        runtime_state: state_document.runtime_state,
        current_context: state_document.current_context,
    })
}

/// Persists one previously projected runtime metadata update to an exact session directory.
/// 把一份预先投影的运行态元数据更新持久化到精确会话目录。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact deterministic main-session directory.
/// `session_directory` 是精确确定的主会话目录。
/// `session_id` is the expected owner identity.
/// `session_id` 是预期所属身份。
/// `projection` is the detached history-free metadata produced under the authoritative state lock.
/// `projection` 是在权威状态锁下生成的分离无历史元数据。
///
/// # Returns
/// 返回值
/// Success after exact-owner validation and the overwrite-only `state.json` write.
/// 在完成精确归属校验与覆盖式 `state.json` 写入后返回成功。
pub async fn persist_projected_single_session_runtime_state_update(
    session_directory: PathBuf,
    session_id: SessionId,
    projection: &SessionRuntimeStatePersistenceProjection,
) -> Result<(), String> {
    if projection.session_id != session_id.to_string() {
        return Err(String::from(
            "Projected runtime state owner does not match the requested session.",
        ));
    }
    validate_exact_segmented_session_directory_owner(&session_directory, session_id)?;
    // stateDocument reconstructs only the storage envelope; writeState preserves the independent session-boundary field.
    // stateDocument 只重建存储信封；writeState 会保留独立的会话边界字段。
    let state_document = SegmentedSessionStateDocument {
        schema_version: 1,
        session_id: projection.session_id.clone(),
        updated_at: projection.updated_at.clone(),
        runtime_state: projection.runtime_state.clone(),
        current_context: projection.current_context.clone(),
        session_boundary_state: Value::Null,
    };
    write_state(&session_directory, &state_document)
}

/// Synchronizes one borrowed canonical runtime conversation into the exact session runtime log.
/// 将一个借用的规范运行时会话同步到精确会话运行时日志。
///
/// # Parameters
/// 参数
/// `session_directory` is the deterministic owning session directory, `session_id` is its exact owner,
/// and `runtime_state` is borrowed from the authoritative in-memory entry without cloning history.
/// `session_directory` 是确定性的所属会话目录，`session_id` 是其精确所有者，
/// `runtime_state` 从权威内存条目借用且不克隆历史。
///
/// # Returns
/// 返回值
/// Success after bounded runtime-log synchronization, or a strict locator, projection, or persistence error.
/// 在有界运行时日志同步后返回成功，或返回严格定位、投影或持久化错误。
pub fn sync_single_session_runtime_log_update(
    session_directory: &Path,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    persistence::sync_segmented_runtime_log_for_state(session_directory, runtime_state)
}

/// Projects one live in-memory retry snapshot into a lightweight runtime-state reference.
/// 将一份实时内存重试快照投影为轻量运行态引用。
///
/// # Parameters
/// 参数
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `snapshot` is the complete retry state retained by the live service process.
/// `snapshot` 是由实时服务进程保留的完整重试状态。
///
/// # Returns
/// 返回值
/// A digest-bound lightweight reference without any filesystem mutation.
/// 返回不产生任何文件系统变更的摘要绑定轻量引用。
pub fn project_execution_retry_snapshot(
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshotRef, String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    ExecutionRetrySnapshotStore::project(snapshot)
}

/// Loads the exact fixed retry-file reference solely for unreferenced interruption reconciliation.
/// 仅为协调未被引用的中断正文而加载精确固定重试文件引用。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact owning segmented session directory.
/// `session_directory` 是精确的所属分段会话目录。
/// `session_id` is the exact main-session owner required in the decoded body.
/// `session_id` 是解码正文必须匹配的精确主会话所有者。
///
/// # Returns
/// 返回值
/// A validated reference when the fixed file exists, `None` when absent, or an explicit owner/integrity error.
/// 固定文件存在时返回经过校验的引用；缺失时返回 `None`；所有者或完整性异常时返回明确错误。
pub fn load_unreferenced_execution_retry_snapshot_if_present(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<Option<vulcan_workbench_model::ExecutionRetrySnapshotRef>, String> {
    let loaded = ExecutionRetrySnapshotStore::new(session_directory)
        .load_interrupted_reference_if_present()?;
    let Some((snapshot, reference)) = loaded else {
        return Ok(None);
    };
    // Directory ownership is read only after the exact retry file exists, keeping the ordinary
    // no-sidecar submission path to one direct missing-file lookup.
    // 仅在精确重试文件存在后读取目录所有权，使普通无 sidecar 提交路径只执行一次固定缺失文件查询。
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    Ok(Some(reference))
}

/// Persists one real interrupted retry boundary to the exact session-root `retry.json` file.
/// 将一个真实中断重试边界持久化到精确的会话根目录 `retry.json` 文件。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact owning segmented session directory.
/// `session_directory` 是精确的所属分段会话目录。
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `snapshot` is the final retry state after the terminal boundary is classified.
/// `snapshot` 是终止边界完成分类后的最终重试状态。
///
/// # Returns
/// 返回值
/// A digest-bound reference only after the single fixed file completes `sync_all`.
/// 仅在唯一固定文件完成 `sync_all` 后返回摘要绑定引用。
pub fn persist_interrupted_execution_retry_snapshot(
    session_directory: &Path,
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshotRef, String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    ExecutionRetrySnapshotStore::new(session_directory).write_interrupted(snapshot)
}

/// Persists one real terminal interruption when no current/newer retry generation is already published.
/// 仅当尚未发布当前或更新重试代际时持久化一个真实终止中断边界。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact owning segmented session directory.
/// `session_directory` 是精确的所属分段会话目录。
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `snapshot` is the final memory-owned retry body from the unique terminal transaction.
/// `snapshot` 是唯一终止事务提供的最终内存重试正文。
///
/// # Returns
/// 返回值
/// A durable reference after unique-file creation or older-generation replacement, otherwise an explicit stale-writer/storage error.
/// 创建唯一文件或替换更旧代际后返回可靠引用，否则返回明确的陈旧写入者或存储错误。
pub fn persist_interrupted_execution_retry_snapshot_if_absent(
    session_directory: &Path,
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshotRef, String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    ExecutionRetrySnapshotStore::new(session_directory).write_interrupted_if_absent(snapshot)
}

/// Persists one bounded provider request trace into an exact rotating session slot.
/// 将一份有界 Provider 请求追踪持久化到精确的会话轮转槽位。
///
/// # Parameters
/// 参数
/// `session_directory` is the already-resolved exact owning session directory.
/// `session_directory` 是已经解析的精确所属会话目录。
/// `session_id` is the exact session that owns the traced provider request.
/// `session_id` 是拥有被追踪 Provider 请求的精确会话。
/// `slot_index` is the deterministic bounded rotation slot selected by the caller.
/// `slot_index` 是调用方选择的确定性有界轮转槽位。
/// `trace` is the complete secret-free diagnostic document.
/// `trace` 是不含秘钥的完整诊断文档。
///
/// # Returns
/// 返回值
/// The final trace path after atomic publication, or an explicit validation and storage error.
/// 原子发布完成后返回最终追踪路径，或返回明确的校验与存储错误。
pub fn persist_provider_request_trace(
    session_directory: &Path,
    session_id: SessionId,
    slot_index: u64,
    trace: &Value,
) -> Result<PathBuf, String> {
    if slot_index >= PROVIDER_REQUEST_TRACE_SLOT_COUNT {
        return Err(format!(
            "Provider request trace slot {slot_index} is outside the 0..{} range.",
            PROVIDER_REQUEST_TRACE_SLOT_COUNT - 1
        ));
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let encoded = encode_bounded_provider_request_trace(trace)?;
    let trace_path = session_directory
        .join("temp")
        .join(PROVIDER_REQUEST_TRACE_DIRECTORY)
        .join(format!("slot-{slot_index:02}.json"));
    replace_exact_provider_request_trace_slot(&trace_path, &encoded, "provider request trace")?;
    Ok(trace_path)
}

/// Persists one physical Provider API attempt inside a rotating logical-request group.
/// 将一次真实 Provider API 尝试持久化到轮转逻辑请求组中。
///
/// # Parameters
/// 参数
/// `session_directory` and `session_id` identify the exact owner; `slot_index` selects one
/// bounded group slot; `artifacts` contains the exact request, raw response, boundaries, and outcome.
/// `session_directory` 与 `session_id` 标识精确归属；`slot_index` 选择一个有界组槽位；
/// `artifacts` 包含精确请求、原始响应、边界和终态。
///
/// # Returns
/// 返回值
/// The final attempt directory after every artifact has been atomically replaced.
/// 全部产物完成原子替换后返回最终尝试目录。
pub fn persist_provider_api_trace_attempt(
    session_directory: &Path,
    session_id: SessionId,
    slot_index: u64,
    artifacts: ProviderApiTraceAttemptArtifacts<'_>,
) -> Result<PathBuf, String> {
    if slot_index >= PROVIDER_REQUEST_TRACE_SLOT_COUNT {
        return Err(format!(
            "Provider API trace slot {slot_index} is outside the 0..{} range.",
            PROVIDER_REQUEST_TRACE_SLOT_COUNT - 1
        ));
    }
    if artifacts.group_id.is_empty()
        || !artifacts
            .group_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(String::from(
            "Provider API trace group id must contain only ASCII letters, digits, or hyphens.",
        ));
    }
    if artifacts.attempt_index >= PROVIDER_API_TRACE_MAX_ATTEMPTS_PER_GROUP {
        return Err(format!(
            "Provider API trace attempt {} is outside the 0..{} range.",
            artifacts.attempt_index,
            PROVIDER_API_TRACE_MAX_ATTEMPTS_PER_GROUP - 1
        ));
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let retained_bytes = u64::try_from(artifacts.response_body.len()).unwrap_or(u64::MAX);
    if retained_bytes > PROVIDER_API_TRACE_ATTEMPT_MAX_BYTES {
        return Err(format!(
            "Provider API trace body exceeds {PROVIDER_API_TRACE_ATTEMPT_MAX_BYTES} bytes."
        ));
    }
    let _write_guard = PROVIDER_REQUEST_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("Provider request trace write lock is poisoned."))?;
    let trace_directory = ensure_owned_provider_api_trace_directory(session_directory)?;
    let slot_directory = trace_directory.join(format!("slot-{slot_index:02}"));
    prepare_provider_api_trace_group_slot(
        &slot_directory,
        artifacts.group_sequence,
        artifacts.group_id,
    )?;
    let mut group = artifacts.group.clone();
    let group_object = group
        .as_object_mut()
        .ok_or_else(|| String::from("Provider API trace group metadata must be a JSON object."))?;
    group_object.insert(
        String::from("group_sequence"),
        Value::from(artifacts.group_sequence),
    );
    group_object.insert(
        String::from("group_id"),
        Value::String(artifacts.group_id.to_string()),
    );
    write_provider_api_trace_json(
        &slot_directory.join("group.json"),
        &group,
        "Provider API trace group metadata",
    )?;
    let attempt_directory = slot_directory.join(format!("attempt-{:02}", artifacts.attempt_index));
    prepare_exact_trace_attempt_directory(&attempt_directory)?;
    write_provider_api_trace_json(
        &attempt_directory.join("request.json"),
        artifacts.request,
        "Provider API trace request",
    )?;
    write_provider_api_trace_json(
        &attempt_directory.join("response.meta.json"),
        artifacts.response_meta,
        "Provider API trace response metadata",
    )?;
    write_file_bytes_atomic(
        &attempt_directory.join("response.body.part"),
        artifacts.response_body,
        "Provider API trace response body",
    )?;
    write_provider_api_trace_ndjson(
        &attempt_directory.join("response.chunks.ndjson"),
        artifacts.chunks,
        "Provider API trace chunk index",
    )?;
    write_provider_api_trace_ndjson(
        &attempt_directory.join("response.frames.ndjson"),
        artifacts.frames,
        "Provider API trace frame index",
    )?;
    write_provider_api_trace_json(
        &attempt_directory.join("outcome.json"),
        artifacts.outcome,
        "Provider API trace outcome",
    )?;
    Ok(attempt_directory)
}

/// Creates and returns the exact Provider API trace directory owned by one retained session.
/// 创建并返回一个保留会话拥有的精确 Provider API 追踪目录。
///
/// # Parameters
/// 参数
/// `session_directory` and `session_id` identify the exact segmented-session owner.
/// `session_directory` 与 `session_id` 标识精确的分段会话归属。
///
/// # Returns
/// 返回值
/// The canonical trace directory after ownership and symlink containment checks succeed.
/// 归属与符号链接包含检查成功后返回 canonical 追踪目录。
pub fn prepare_provider_api_trace_directory(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<PathBuf, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let _write_guard = PROVIDER_REQUEST_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("Provider request trace write lock is poisoned."))?;
    ensure_owned_provider_api_trace_directory(session_directory)
}

/// Loads the greatest persisted Provider API trace group generation for one exact session.
/// 加载一个精确会话已经持久化的最大 Provider API 追踪组代际。
///
/// # Parameters
/// 参数
/// `session_directory` and `session_id` identify the exact segmented-session owner.
/// `session_directory` 与 `session_id` 标识精确的分段会话归属。
///
/// # Returns
/// 返回值
/// The greatest valid generation across all rotating slots, or zero when no group exists.
/// 返回全部轮转槽位中的最大有效代际；尚无任何组时返回零。
pub fn load_provider_api_trace_group_sequence_floor(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<u64, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    // writeGuard gives the floor scan one process-local snapshot relative to trace replacement.
    // writeGuard 使代际下限扫描相对于追踪替换获得同一进程内的一致快照。
    let _write_guard = PROVIDER_REQUEST_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("Provider request trace write lock is poisoned."))?;
    // traceDirectory is the exact bounded trace root owned by this retained session.
    // traceDirectory 是这个保留会话精确拥有的有界追踪根目录。
    let trace_directory = session_directory
        .join("temp")
        .join(PROVIDER_API_TRACE_DIRECTORY);
    match fs::symlink_metadata(&trace_directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(format!(
                "Refusing to read non-directory or symbolic-link Provider API trace root: {}",
                trace_directory.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "Failed to inspect Provider API trace directory `{}`: {error}",
                trace_directory.display()
            ));
        }
    }
    // canonicalTraceDirectory proves every subsequent fixed slot read remains under the owner.
    // canonicalTraceDirectory 证明后续每个固定槽位读取都保持在所属会话之内。
    let trace_directory =
        canonical_provider_api_trace_directory(session_directory, &trace_directory)?;
    // sequenceFloor advances monotonically across the four bounded rotating slots.
    // sequenceFloor 在四个有界轮转槽位之间单调提升。
    let mut sequence_floor = 0_u64;
    for slot_index in 0..PROVIDER_REQUEST_TRACE_SLOT_COUNT {
        // slotDirectory is derived only from the fixed validated slot index.
        // slotDirectory 仅由经过固定边界验证的槽位索引派生。
        let slot_directory = trace_directory.join(format!("slot-{slot_index:02}"));
        match fs::symlink_metadata(&slot_directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Refusing to read non-directory or symbolic-link Provider API trace slot: {}",
                    slot_directory.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect Provider API trace slot `{}`: {error}",
                    slot_directory.display()
                ));
            }
        }
        // groupPath is the authoritative identity document for this exact slot.
        // groupPath 是这个精确槽位的权威身份文档。
        let group_path = slot_directory.join("group.json");
        match fs::symlink_metadata(&group_path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Refusing to read non-file or symbolic-link Provider API trace group metadata: {}",
                    group_path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect Provider API trace group metadata `{}`: {error}",
                    group_path.display()
                ));
            }
        }
        // group is decoded only after both its root and file shape pass containment checks.
        // group 仅在根目录与文件形态都通过包含性检查后解码。
        let group = serde_json::from_slice::<Value>(&fs::read(&group_path).map_err(|error| {
            format!(
                "Provider API trace group metadata `{}` could not be read: {error}",
                group_path.display()
            )
        })?)
        .map_err(|error| {
            format!(
                "Provider API trace group metadata `{}` is invalid: {error}",
                group_path.display()
            )
        })?;
        // sequence is the persisted ordering authority used to reject stale detached writers.
        // sequence 是用于拒绝陈旧独立写入者的持久化顺序权威。
        let sequence = group
            .get("group_sequence")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "Provider API trace group metadata `{}` is missing group_sequence.",
                    group_path.display()
                )
            })?;
        sequence_floor = sequence_floor.max(sequence);
    }
    Ok(sequence_floor)
}

/// Removes only the exact Provider API trace directory owned by one retained session.
/// 仅移除一个保留会话拥有的精确 Provider API 追踪目录。
///
/// # Parameters
/// 参数
/// `session_directory` and `session_id` identify the exact segmented-session owner.
/// `session_directory` 与 `session_id` 标识精确的分段会话归属。
///
/// # Returns
/// 返回值
/// `true` when an existing trace directory was removed, otherwise `false`.
/// 已有追踪目录被移除时返回 `true`，否则返回 `false`。
pub fn clear_provider_api_trace_directory(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<bool, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let _write_guard = PROVIDER_REQUEST_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("Provider request trace write lock is poisoned."))?;
    let trace_directory = session_directory
        .join("temp")
        .join(PROVIDER_API_TRACE_DIRECTORY);
    match fs::symlink_metadata(&trace_directory) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Failed to inspect Provider API trace directory `{}`: {error}",
                trace_directory.display()
            ));
        }
    }
    let trace_directory =
        canonical_provider_api_trace_directory(session_directory, &trace_directory)?;
    fs::remove_dir_all(&trace_directory).map_err(|error| {
        format!(
            "Failed to clear Provider API trace directory `{}`: {error}",
            trace_directory.display()
        )
    })?;
    Ok(true)
}

/// Creates the owned trace root one directory at a time without traversing symbolic links.
/// 逐级创建所属追踪根目录，且不穿越符号链接。
fn ensure_owned_provider_api_trace_directory(session_directory: &Path) -> Result<PathBuf, String> {
    let canonical_session = fs::canonicalize(session_directory).map_err(|error| {
        format!(
            "Failed to canonicalize Provider API trace session directory `{}`: {error}",
            session_directory.display()
        )
    })?;
    let temp_directory = session_directory.join("temp");
    ensure_owned_trace_directory_component(
        &canonical_session,
        &temp_directory,
        "Provider API trace temp directory",
    )?;
    let trace_directory = temp_directory.join(PROVIDER_API_TRACE_DIRECTORY);
    ensure_owned_trace_directory_component(
        &canonical_session,
        &trace_directory,
        "Provider API trace directory",
    )?;
    canonical_provider_api_trace_directory(session_directory, &trace_directory)
}

/// Ensures one trace path component is a real directory contained by the canonical session.
/// 确保一个追踪路径组件是 canonical 会话内的真实目录。
fn ensure_owned_trace_directory_component(
    canonical_session: &Path,
    path: &Path,
    artifact_kind: &str,
) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(format!(
                "Refusing to use non-directory or symbolic-link {artifact_kind}: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|create_error| {
                format!(
                    "Failed to create {artifact_kind} `{}`: {create_error}",
                    path.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "Failed to inspect {artifact_kind} `{}`: {error}",
                path.display()
            ));
        }
    }
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to canonicalize {artifact_kind} `{}`: {error}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(canonical_session) {
        return Err(format!(
            "{artifact_kind} `{}` escapes session directory `{}`.",
            canonical_path.display(),
            canonical_session.display()
        ));
    }
    Ok(())
}

/// Canonicalizes one existing trace directory and proves it remains inside its session owner.
/// canonical 化一个已有追踪目录，并证明它仍位于所属会话目录内。
fn canonical_provider_api_trace_directory(
    session_directory: &Path,
    trace_directory: &Path,
) -> Result<PathBuf, String> {
    let canonical_session = fs::canonicalize(session_directory).map_err(|error| {
        format!(
            "Failed to canonicalize Provider API trace session directory `{}`: {error}",
            session_directory.display()
        )
    })?;
    let canonical_trace = fs::canonicalize(trace_directory).map_err(|error| {
        format!(
            "Failed to canonicalize Provider API trace directory `{}`: {error}",
            trace_directory.display()
        )
    })?;
    if !canonical_trace.starts_with(&canonical_session) {
        return Err(format!(
            "Provider API trace directory `{}` escapes session directory `{}`.",
            canonical_trace.display(),
            canonical_session.display()
        ));
    }
    Ok(canonical_trace)
}

/// Prepares one rotating group slot while rejecting an older out-of-order writer.
/// 准备一个轮转组槽位，同时拒绝较旧的乱序写入者。
fn prepare_provider_api_trace_group_slot(
    slot_directory: &Path,
    group_sequence: u64,
    group_id: &str,
) -> Result<(), String> {
    let group_path = slot_directory.join("group.json");
    let existing_group =
        match std::fs::read(&group_path) {
            Ok(bytes) => Some(serde_json::from_slice::<Value>(&bytes).map_err(|error| {
                format!("Provider API trace group metadata is invalid: {error}")
            })?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "Provider API trace group metadata `{}` could not be read: {error}",
                    group_path.display()
                ));
            }
        };
    if let Some(existing_group) = existing_group {
        let existing_sequence = existing_group
            .get("group_sequence")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                String::from("Provider API trace group metadata is missing group_sequence.")
            })?;
        let existing_id = existing_group
            .get("group_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                String::from("Provider API trace group metadata is missing group_id.")
            })?;
        if existing_sequence > group_sequence {
            return Err(String::from(
                "A newer Provider API trace group already owns this rotating slot.",
            ));
        }
        if existing_sequence == group_sequence && existing_id != group_id {
            return Err(String::from(
                "Provider API trace group identity changed within one generation.",
            ));
        }
        if existing_sequence < group_sequence {
            remove_exact_trace_directory(slot_directory, "Provider API trace group slot")?;
        }
    } else if let Ok(metadata) = std::fs::symlink_metadata(slot_directory) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to use non-directory Provider API trace slot: {}",
                slot_directory.display()
            ));
        }
        // A slot without authoritative group metadata cannot safely retain any earlier attempts.
        // 缺少权威组元数据的槽位不能安全保留任何更早尝试。
        remove_exact_trace_directory(slot_directory, "unowned Provider API trace group slot")?;
    }
    std::fs::create_dir_all(slot_directory).map_err(|error| {
        format!(
            "Provider API trace slot `{}` could not be created: {error}",
            slot_directory.display()
        )
    })
}

/// Replaces one exact attempt directory owned by an already validated group slot.
/// 替换一个由已校验组槽位拥有的精确尝试目录。
fn prepare_exact_trace_attempt_directory(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok() {
        remove_exact_trace_directory(path, "Provider API trace attempt")?;
    }
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "Provider API trace attempt `{}` could not be created: {error}",
            path.display()
        )
    })
}

/// Removes one exact bounded trace directory after rejecting links and non-directories.
/// 在拒绝链接和非目录后删除一个精确有界追踪目录。
fn remove_exact_trace_directory(path: &Path, artifact_kind: &str) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            std::fs::remove_dir_all(path).map_err(|error| {
                format!(
                    "Existing {artifact_kind} `{}` could not be removed: {error}",
                    path.display()
                )
            })
        }
        Ok(_) => Err(format!(
            "Refusing to remove non-directory {artifact_kind}: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Existing {artifact_kind} `{}` could not be inspected: {error}",
            path.display()
        )),
    }
}

/// Atomically serializes one JSON artifact in a Provider API trace attempt.
/// 原子序列化 Provider API 追踪尝试中的一个 JSON 产物。
fn write_provider_api_trace_json(
    path: &Path,
    value: &Value,
    artifact_kind: &str,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("{artifact_kind} serialization failed: {error}"))?;
    write_file_bytes_atomic(path, &encoded, artifact_kind)
}

/// Atomically serializes ordered JSON records as newline-delimited JSON.
/// 将有序 JSON 记录原子序列化为换行分隔 JSON。
fn write_provider_api_trace_ndjson(
    path: &Path,
    records: &[Value],
    artifact_kind: &str,
) -> Result<(), String> {
    let mut encoded = Vec::new();
    for record in records {
        serde_json::to_writer(&mut encoded, record)
            .map_err(|error| format!("{artifact_kind} serialization failed: {error}"))?;
        encoded.push(b'\n');
    }
    write_file_bytes_atomic(path, &encoded, artifact_kind)
}

/// Persists one bounded configured-inference provider trace into an exact rotating slot.
/// 将一份有界的已配置推理 Provider 追踪持久化到精确轮转槽位。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root; `consumer_profile_id` is the exact
/// authenticated internal consumer; `slot_index` is the deterministic bounded slot; and
/// `trace` is the complete secret-free diagnostic document.
/// `data_root` 是控制器管理的 Vulcan 数据根目录；`consumer_profile_id` 是精确的
/// 已认证内部消费者；`slot_index` 是确定性有界槽位；`trace` 是完整的
/// 不含秘密诊断文档。
///
/// # Returns
/// 返回值
/// The final trace path after atomic publication, or an explicit validation and storage error.
/// 原子发布完成后返回最终追踪路径，或返回明确的校验与存储错误。
pub fn persist_configured_inference_provider_request_trace(
    data_root: &Path,
    consumer_profile_id: &str,
    slot_index: u64,
    trace: &Value,
) -> Result<PathBuf, String> {
    let safe_component = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    };
    if !safe_component(consumer_profile_id) {
        return Err(String::from(
            "Configured-inference trace consumer profile must contain only ASCII letters, digits, hyphens, or underscores.",
        ));
    }
    if slot_index >= PROVIDER_REQUEST_TRACE_SLOT_COUNT {
        return Err(format!(
            "Configured-inference trace slot {slot_index} is outside the 0..{} range.",
            PROVIDER_REQUEST_TRACE_SLOT_COUNT - 1
        ));
    }
    let encoded = encode_bounded_provider_request_trace(trace)?;
    let trace_path = data_root
        .join("diagnostics")
        .join(CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY)
        .join(consumer_profile_id)
        .join(format!("slot-{slot_index:02}.json"));
    replace_exact_provider_request_trace_slot(
        &trace_path,
        &encoded,
        "configured-inference provider request trace",
    )?;
    Ok(trace_path)
}

/// Encodes one compact trace and omits only the oversized request body before response evidence.
/// 编码单份紧凑追踪，并在响应证据之前仅省略超大的请求正文。
///
/// # Parameters
/// 参数
/// `trace` is the transport-boundary document whose credential-shaped request headers are redacted.
/// `trace` 是在传输边界捕获的文档，其中凭据形态的请求头已经脱敏。
///
/// # Returns
/// 返回值
/// Compact bytes bounded by the trace-slot capacity, or an explicit serialization error.
/// 返回受追踪槽位容量约束的紧凑字节，或明确的序列化错误。
fn encode_bounded_provider_request_trace(trace: &Value) -> Result<Vec<u8>, String> {
    let encoded = serde_json::to_vec(trace)
        .map_err(|error| format!("Provider request trace JSON serialization failed: {error}"))?;
    let encoded_len = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
    if encoded_len <= PROVIDER_REQUEST_TRACE_MAX_BYTES {
        return Ok(encoded);
    }
    let api_request = trace.get("api_request");
    // apiResponse remains intact because failed raw Provider bytes are the primary diagnostic evidence.
    // apiResponse 保持完整，因为失败时的 Provider 原始字节是首要诊断证据。
    let api_response = trace.get("api_response");
    let summary = serde_json::json!({
        "schema_version": trace.get("schema_version"),
        "captured_at_unix_ms": trace.get("captured_at_unix_ms"),
        "process_sequence": trace.get("process_sequence"),
        "session_id": trace.get("session_id"),
        "request": trace.get("request"),
        "execution_id": trace.get("execution_id"),
        "model_id": trace.get("model_id"),
        "route": trace.get("route"),
        "trace_omission": {
            "reason": "encoded_trace_exceeded_bounded_slot_capacity",
            "encoded_bytes": encoded_len,
            "maximum_bytes": PROVIDER_REQUEST_TRACE_MAX_BYTES,
        },
        "api_request": {
            "capture_point": api_request.and_then(|value| value.get("capture_point")),
            "transport": api_request.and_then(|value| value.get("transport")),
            "method": api_request.and_then(|value| value.get("method")),
            "path": api_request.and_then(|value| value.get("path")),
            "resolved_url": api_request.and_then(|value| value.get("resolved_url")),
            "headers": api_request.and_then(|value| value.get("headers")),
            "credential_values_redacted": true,
            "body_omitted": true,
        },
        "api_response": api_response,
    });
    let summary_bytes = serde_json::to_vec(&summary).map_err(|error| {
        format!("Bounded Provider request trace summary serialization failed: {error}")
    })?;
    if u64::try_from(summary_bytes.len()).unwrap_or(u64::MAX) > PROVIDER_REQUEST_TRACE_MAX_BYTES {
        return Err(String::from(
            "Bounded Provider request trace metadata exceeds its slot capacity.",
        ));
    }
    Ok(summary_bytes)
}

/// Replaces one exact trace slot without enumerating its directory.
/// 在不枚举目录的情况下替换一个精确追踪槽位。
///
/// # Parameters
/// 参数
/// `path` is the deterministic slot path, `encoded` contains bounded compact JSON, and
/// `artifact_kind` labels storage diagnostics.
/// `path` 是确定性槽位路径，`encoded` 包含有界紧凑 JSON，`artifact_kind` 用于标记存储诊断。
///
/// # Returns
/// 返回值
/// Success after exact replacement, or an explicit file-type or publication error.
/// 精确替换后返回成功，或返回明确的文件类型与发布错误。
fn replace_exact_provider_request_trace_slot(
    path: &Path,
    encoded: &[u8],
    artifact_kind: &str,
) -> Result<(), String> {
    let _write_guard = PROVIDER_REQUEST_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("Provider request trace write lock is poisoned."))?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            std::fs::remove_file(path).map_err(|error| {
                format!(
                    "Existing {artifact_kind} slot `{}` could not be removed: {error}",
                    path.display()
                )
            })?;
        }
        Ok(_) => {
            return Err(format!(
                "Refusing to replace non-file {artifact_kind} slot: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Existing {artifact_kind} slot `{}` could not be inspected: {error}",
                path.display()
            ));
        }
    }
    write_file_bytes_atomic(path, encoded, artifact_kind)
}

/// Loads one exact retry snapshot through its digest-bound runtime-state reference.
/// 通过运行态中的摘要绑定引用加载一份精确重试快照。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `reference` is the lightweight reference committed in session runtime state.
/// `reference` 是提交在会话运行态中的轻量引用。
///
/// # Returns
/// 返回值
/// The complete sidecar only when every strict identity and digest check succeeds.
/// 仅当全部严格身份与摘要校验成功时返回完整 sidecar。
pub fn load_execution_retry_snapshot(
    session_directory: &Path,
    session_id: SessionId,
    reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshot, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let snapshot = ExecutionRetrySnapshotStore::new(session_directory).read(reference)?;
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    Ok(snapshot)
}

/// Deletes one exact retry sidecar after its consumed or replacement reference is durable.
/// 在消费状态或替代引用可靠写入后删除一个精确重试 sidecar。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `snapshot_id` is the exact UUID sidecar identity.
/// `snapshot_id` 是精确的 UUID sidecar 身份。
///
/// # Returns
/// 返回值
/// Success when the session and sidecar are absent or the sidecar is removed.
/// 会话或 sidecar 不存在，或 sidecar 已删除时返回成功。
pub fn delete_execution_retry_snapshot(
    session_directory: &Path,
    session_id: SessionId,
    reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
) -> Result<(), String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    ExecutionRetrySnapshotStore::new(session_directory).delete(reference)
}

/// Returns the exact rebuild reason when one session contains a rejected historical or damaged VMM attachment.
/// 当单个会话包含已拒绝的历史或损坏 VMM 附件时返回精确重建原因。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact session inspected before controller creation.
/// `session_id` 是控制器创建前接受检查的精确会话。
///
/// # Returns
/// 返回值
/// A user-recoverable reason only for classified historical or damaged VMM artifacts; clean or current state returns `None`.
/// 仅对已分类的历史或损坏 VMM 工件返回用户可恢复原因；干净或当前状态返回 `None`。
pub fn core_session_vmm_rebuild_reason(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    // session_id_text is the canonical owner used by both lookup and diagnostics.
    // session_id_text 是查找与诊断共同使用的规范所有者。
    let session_id_text = session_id.to_string();
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    core_vmm_rebuild_reason(session_directory, &session_id_text)
}

/// Persists one candidate state through the strict Core VMM storage adapter.
/// 通过严格的 Core VMM 存储适配器持久化一份候选状态。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact owning session.
/// `session_id` 是精确的所属会话。
/// `state` is the complete controller-serialized candidate.
/// `state` 是控制器串行化后的完整候选状态。
///
/// # Returns
/// 返回值
/// Success only after the candidate is durably published or confirmed idempotent.
/// 仅在候选状态持久发布或确认幂等后返回成功。
pub fn persist_core_session_vmm_state(
    session_directory: &Path,
    session_id: SessionId,
    state: &vulcan_vmm::VmmSessionMemoryState,
) -> Result<(), String> {
    // session_id_text is shared by directory lookup, ownership validation, and the document.
    // session_id_text 由目录查找、所有权校验与文档共同使用。
    let session_id_text = session_id.to_string();
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    write_core_vmm_controller_state(session_directory, &session_id_text, state)
}

/// Deletes every VMM-owned root artifact for one exact persisted session.
/// 删除单个精确持久化会话的全部 VMM 根工件。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact session whose VMM ownership is being deleted.
/// `session_id` 是正在删除 VMM 所有权的精确会话。
///
/// # Returns
/// 返回值
/// `true` when the session directory existed and its VMM artifacts are now absent.
/// 会话目录存在且其 VMM 工件现已全部清除时返回 `true`。
pub fn delete_core_session_vmm_state(
    session_directory: &Path,
    session_id: SessionId,
) -> Result<bool, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    reset_vmm_state_for_history_rewrite(session_directory)?;
    Ok(true)
}

/// Replaces one classified damaged VMM attachment with an empty current-format document.
/// 使用空的当前格式文档替换一个已分类的损坏 VMM 附件。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact session whose derived VMM memory is being rebuilt.
/// `session_id` 是正在重建派生 VMM 记忆的精确会话。
/// `updated_at` is the timestamp assigned to the new empty memory state.
/// `updated_at` 是分配给新空记忆状态的时间戳。
///
/// # Returns
/// 返回值
/// `true` only after a classified damaged attachment is replaced without changing the session transcript.
/// 仅在已分类损坏附件被替换且会话转录未改变后返回 `true`。
pub fn rebuild_damaged_core_session_vmm_state(
    session_directory: &Path,
    session_id: SessionId,
    updated_at: &str,
) -> Result<bool, String> {
    // session_id_text is the canonical key shared by classification, reset, and the new document.
    // session_id_text 是分类、重置与新文档共同使用的规范键。
    let session_id_text = session_id.to_string();
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    // rebuild_reason gates destructive VMM reset to a storage-classified damaged attachment.
    // rebuild_reason 将破坏性 VMM 重置限制为存储层已分类的损坏附件。
    let Some(_rebuild_reason) = core_vmm_rebuild_reason(session_directory, &session_id_text)?
    else {
        return Err(format!(
            "Session `{session_id}` VMM attachment is healthy or absent and cannot be rebuilt."
        ));
    };
    // replacement_state contains no recalled memory and uses only the current strict document schema.
    // replacement_state 不包含任何召回记忆，并且仅使用当前严格文档结构。
    let replacement_state =
        vulcan_vmm::VmmSessionMemoryState::new(session_id_text.clone(), updated_at)?;
    reset_vmm_state_for_history_rewrite(session_directory)?;
    write_core_vmm_controller_state(session_directory, &session_id_text, &replacement_state)?;
    Ok(true)
}

/// Rejects ordinary runtime-state JSON that still embeds the removed mutable VMM owner.
/// 拒绝仍嵌入已移除可变 VMM 所有者的普通运行态 JSON。
///
/// # Parameters
/// 参数
/// `runtime_state` is the raw ordinary runtime-state object loaded from `state.json`.
/// `runtime_state` 是从 `state.json` 加载的原始普通运行态对象。
/// `session_id` is the exact owner included in the unsupported-history diagnostic.
/// `session_id` 是不支持历史格式诊断中包含的精确所有者。
///
/// # Returns
/// 返回值
/// Success only when the removed `vmm_session_memory` field is absent.
/// 仅在已移除的 `vmm_session_memory` 字段不存在时返回成功。
fn reject_embedded_historical_vmm_runtime_state(
    runtime_state: &serde_json::Value,
    session_id: &str,
) -> Result<(), String> {
    if runtime_state
        .as_object()
        .is_some_and(|object| object.contains_key("vmm_session_memory"))
    {
        return Err(format!(
            "Session `{session_id}` contains unsupported historical embedded VMM runtime state; automatic migration and fallback are disabled."
        ));
    }
    Ok(())
}

/// Rejects decoded ordinary runtime state containing the removed hidden-user VMM profile group.
/// 拒绝包含已移除隐藏用户 VMM 画像分组的已解码普通运行态。
///
/// # Parameters
/// 参数
/// `runtime_state` is the decoded ordinary runtime state being loaded.
/// `runtime_state` 是正在加载的已解码普通运行态。
/// `session_id` is the exact owner included in the unsupported-history diagnostic.
/// `session_id` 是不支持历史格式诊断中包含的精确所有者。
///
/// # Returns
/// 返回值
/// Success only when no removed VMM profile injection group is present.
/// 仅在不存在已移除 VMM 画像注入分组时返回成功。
fn reject_historical_vmm_profile_context_injection(
    runtime_state: &SessionRuntimeStateEntry,
    session_id: &str,
) -> Result<(), String> {
    if runtime_state
        .context_injection_state
        .groups
        .values()
        .any(|group| group.tag == HISTORICAL_VMM_PROFILE_CONTEXT_INJECTION_TAG)
    {
        return Err(format!(
            "Session `{session_id}` contains unsupported historical VMM profile context injection state; automatic cleanup and fallback are disabled."
        ));
    }
    Ok(())
}

mod boundary_store;
mod execution_retry_store;
mod loading;
mod models;
mod queries;
mod runtime_logs;
mod serialization;

pub use boundary_store::*;
pub use execution_retry_store::*;
pub use loading::*;
pub use models::*;
pub use runtime_logs::*;

use boundary_store::session_boundary_event_from_segment_line;
use loading::sanitize_loaded_runtime_state_for_fresh_process;
use queries::*;
use serialization::*;

#[cfg(test)]
mod tests;

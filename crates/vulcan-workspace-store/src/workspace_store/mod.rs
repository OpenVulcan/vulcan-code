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
pub use self::segmented_session_store::find_segmented_session_directory_for_task_store_created_at;
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
    PersistedSessionForkResult, PersistedUserPromptRollbackResult, archive_project_sessions,
    archive_single_session, cleanup_missing_session_index, delete_archived_session,
    fork_persisted_session, load_single_session_transcript_message_count_in_directory,
    persist_collapsed_task_project_ids, persist_new_project,
    persist_project_codebase_memory_enabled, persist_project_icon_update,
    persist_project_order_update, persist_project_pinned_and_order_update,
    persist_single_session_addon_state_update,
    persist_single_session_addon_state_update_in_directory,
    persist_single_session_runtime_state_update,
    persist_single_session_transcript_messages_in_directory,
    persist_single_session_transcript_tail_in_directory, persist_single_session_update,
    remove_project_after_archiving_sessions, rename_project, rename_single_session,
    restore_archived_session, restore_project, rollback_persisted_session_before_user_prompt,
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
/// Removed hidden-user injection group that represented the historical VMM profile owner.
/// 表示历史 VMM 画像所有者的已移除隐藏用户注入分组。
const HISTORICAL_VMM_PROFILE_CONTEXT_INJECTION_TAG: &str = "vmm_profile";
/// Session-local directory that owns complete opt-in provider request traces.
/// 保存显式启用的完整 Provider 请求追踪文件的会话本地目录。
const PROVIDER_REQUEST_TRACE_DIRECTORY: &str = "provider-request-trace";
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
/// Serializes exact trace-slot replacement without directory discovery or cross-write races.
/// 串行化精确追踪槽位替换，避免目录发现与并发写入竞争。
static PROVIDER_REQUEST_TRACE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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

/// Encodes one compact trace once and replaces oversized bodies with a bounded diagnostic summary.
/// 对单份紧凑追踪只编码一次，并将超大正文替换为有界诊断摘要。
///
/// # Parameters
/// 参数
/// `trace` is the already redacted request document captured at the transport boundary.
/// `trace` 是已经在传输边界捕获并脱敏的请求文档。
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

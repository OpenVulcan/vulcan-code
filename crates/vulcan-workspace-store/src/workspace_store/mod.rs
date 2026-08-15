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
use std::path::{Path, PathBuf};
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
mod provider_billing;
mod runtime_state_codec;
mod segmented_session_store;
mod session_addon_store;
mod session_load;

use self::runtime_state_codec::{
    deserialize_session_runtime_state_entry, serialize_session_runtime_state_entry,
};
pub use self::segmented_session_store::find_segmented_session_directory_for_task_store_created_at;
use self::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventInput,
    SegmentedSessionEventKind, append_segmented_boundary_events, append_segmented_session_events,
    backup_runtime_log_before_compaction, core_vmm_rebuild_reason,
    create_segmented_session_in_location, find_segmented_session_directory,
    find_segmented_session_directory_by_id, load_core_vmm_controller_state,
    load_session_boundary_state, read_runtime_log_lines, read_segmented_session_event_window,
    read_state, record_runtime_audit_event, record_runtime_injection_applied,
    replace_session_boundary_state, reset_vmm_state_for_history_rewrite,
    segmented_session_directory, segmented_session_directory_exists,
    state_document_from_runtime_state, write_core_vmm_controller_state, write_file_bytes_atomic,
    write_state,
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
    PersistedSessionForkResult, PersistedUserPromptEditDraft, PersistedUserPromptRollbackResult,
    archive_project_sessions, archive_single_session, fork_persisted_session,
    load_persisted_user_prompt_edit_draft, persist_collapsed_task_project_ids, persist_new_project,
    persist_project_codebase_memory_enabled, persist_project_icon_update,
    persist_project_order_update, persist_project_pinned_and_order_update,
    persist_single_session_addon_state_update, persist_single_session_runtime_state_update,
    persist_single_session_transcript_messages, persist_single_session_update,
    remove_project_after_archiving_sessions, rename_project, rename_single_session,
    restore_project, rollback_persisted_session_before_user_prompt,
};
pub use provider_billing::{
    PROVIDER_USAGE_SCHEMA_REVISION, ProviderAccountUsageSnapshotRecord,
    ProviderAccountUsageSummary, ProviderPricingProfileRecord, ProviderRateLimitSnapshotRecord,
    ProviderUsageAttemptRecord, ProviderUsageAttemptSummary, ProviderUsageDailySummary,
    ProviderUsageDashboard, ensure_provider_usage_schema, load_provider_usage_dashboard,
    load_provider_usage_migration_completed, load_provider_usage_migration_cursor,
    lock_provider_usage_writes, migrate_application_provider_usage,
    persist_provider_account_usage_snapshot, persist_provider_pricing_profile,
    persist_provider_rate_limit_snapshot, persist_provider_usage_attempt,
    persist_provider_usage_attempt_and_rebuild_daily,
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
    data_root: &Path,
    session_id: SessionId,
) -> Result<Option<vulcan_vmm::VmmSessionMemoryState>, String> {
    // session_id_text is the canonical persisted owner spelling.
    // session_id_text 是规范的持久化所有者文本形式。
    let session_id_text = session_id.to_string();
    // session_dir resolves the one active or archived directory indexed by the session id.
    // session_dir 解析由会话标识索引的唯一活动或归档目录。
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Ok(None);
    };
    load_core_vmm_controller_state(&session_dir, &session_id_text)
}

/// Atomically persists one exact retry snapshot below its owning segmented session directory.
/// 在所属分段会话目录下原子持久化一份精确重试快照。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact main-session owner.
/// `session_id` 是精确的主会话所有者。
/// `snapshot` is the complete retry sidecar candidate.
/// `snapshot` 是完整的重试 sidecar 候选。
///
/// # Returns
/// 返回值
/// A digest-bound lightweight reference only after atomic sidecar publication succeeds.
/// 仅在 sidecar 原子发布成功后返回摘要绑定的轻量引用。
pub fn persist_execution_retry_snapshot(
    data_root: &Path,
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshotRef, String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "Execution retry snapshot owner does not match the requested session.",
        ));
    }
    let session_directory = find_segmented_session_directory_by_id(
        data_root,
        &session_id.to_string(),
    )?
    .ok_or_else(|| {
        format!(
            "Session `{session_id}` has no segmented directory for execution retry persistence."
        )
    })?;
    ExecutionRetrySnapshotStore::new(session_directory).write(snapshot)
}

/// Atomically persists one complete provider request trace below its owning session temp directory.
/// 在所属会话的临时目录下原子持久化一份完整 Provider 请求追踪。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root.
/// `data_root` 是控制器管理的 Vulcan 数据根目录。
/// `session_id` is the exact session that owns the traced provider request.
/// `session_id` 是拥有被追踪 Provider 请求的精确会话。
/// `file_stem` is an internally generated filesystem-safe trace identity without an extension.
/// `file_stem` 是内部生成且不含扩展名的文件系统安全追踪标识。
/// `trace` is the complete secret-free diagnostic document.
/// `trace` 是不含秘钥的完整诊断文档。
///
/// # Returns
/// 返回值
/// The final trace path after atomic publication, or an explicit validation and storage error.
/// 原子发布完成后返回最终追踪路径，或返回明确的校验与存储错误。
pub fn persist_provider_request_trace(
    data_root: &Path,
    session_id: SessionId,
    file_stem: &str,
    trace: &Value,
) -> Result<PathBuf, String> {
    if file_stem.is_empty()
        || !file_stem
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(String::from(
            "Provider request trace file stem must contain only ASCII letters, digits, hyphens, or underscores.",
        ));
    }
    let session_directory = find_segmented_session_directory_by_id(
        data_root,
        &session_id.to_string(),
    )?
    .ok_or_else(|| {
        format!("Session `{session_id}` has no segmented directory for provider request tracing.")
    })?;
    // encoded is completed before filesystem publication so a serialization failure cannot leave a partial trace.
    // encoded 在文件系统发布前已经完整生成，因此序列化失败不会留下不完整追踪文件。
    let encoded = serde_json::to_vec_pretty(trace)
        .map_err(|error| format!("Provider request trace JSON serialization failed: {error}"))?;
    let trace_path = session_directory
        .join("temp")
        .join(PROVIDER_REQUEST_TRACE_DIRECTORY)
        .join(format!("{file_stem}.json"));
    write_file_bytes_atomic(&trace_path, &encoded, "provider request trace")?;
    Ok(trace_path)
}

/// Atomically persists one configured-inference provider trace below the application diagnostics root.
/// 在应用诊断根目录下原子持久化一份已配置推理 Provider 追踪。
///
/// # Parameters
/// 参数
/// `data_root` is the controller-managed Vulcan data root; `consumer_profile_id` is the exact
/// authenticated internal consumer; `file_stem` is an internally generated safe identity; and
/// `trace` is the complete secret-free diagnostic document.
/// `data_root` 是控制器管理的 Vulcan 数据根目录；`consumer_profile_id` 是精确的
/// 已认证内部消费者；`file_stem` 是内部生成的安全标识；`trace` 是完整的
/// 不含秘密诊断文档。
///
/// # Returns
/// 返回值
/// The final trace path after atomic publication, or an explicit validation and storage error.
/// 原子发布完成后返回最终追踪路径，或返回明确的校验与存储错误。
pub fn persist_configured_inference_provider_request_trace(
    data_root: &Path,
    consumer_profile_id: &str,
    file_stem: &str,
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
    if !safe_component(file_stem) {
        return Err(String::from(
            "Configured-inference trace file stem must contain only ASCII letters, digits, hyphens, or underscores.",
        ));
    }
    // encoded is complete before publication so a serialization failure cannot leave a partial diagnostic.
    // encoded 在发布前已完整生成，因此序列化失败不会留下局部诊断文件。
    let encoded = serde_json::to_vec_pretty(trace).map_err(|error| {
        format!("Configured-inference provider request trace JSON serialization failed: {error}")
    })?;
    let trace_path = data_root
        .join("diagnostics")
        .join(CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY)
        .join(consumer_profile_id)
        .join(format!("{file_stem}.json"));
    write_file_bytes_atomic(
        &trace_path,
        &encoded,
        "configured-inference provider request trace",
    )?;
    Ok(trace_path)
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
    data_root: &Path,
    session_id: SessionId,
    reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
) -> Result<vulcan_workbench_model::ExecutionRetrySnapshot, String> {
    let session_directory = find_segmented_session_directory_by_id(
        data_root,
        &session_id.to_string(),
    )?
    .ok_or_else(|| {
        format!("Session `{session_id}` has no segmented directory for execution retry loading.")
    })?;
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
    data_root: &Path,
    session_id: SessionId,
    snapshot_id: &str,
) -> Result<(), String> {
    let Some(session_directory) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(());
    };
    ExecutionRetrySnapshotStore::new(session_directory).delete(snapshot_id)
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
    data_root: &Path,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    // session_id_text is the canonical owner used by both lookup and diagnostics.
    // session_id_text 是查找与诊断共同使用的规范所有者。
    let session_id_text = session_id.to_string();
    // session_dir resolves only an already indexed active or archived session.
    // session_dir 仅解析已建立索引的活动或归档会话。
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Ok(None);
    };
    core_vmm_rebuild_reason(&session_dir, &session_id_text)
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
    data_root: &Path,
    session_id: SessionId,
    state: &vulcan_vmm::VmmSessionMemoryState,
) -> Result<(), String> {
    // session_id_text is shared by directory lookup, ownership validation, and the document.
    // session_id_text 由目录查找、所有权校验与文档共同使用。
    let session_id_text = session_id.to_string();
    // session_dir must already exist because the controller never owns general session creation.
    // session_dir 必须已经存在，因为控制器不拥有普通会话创建职责。
    let session_dir = find_segmented_session_directory_by_id(data_root, &session_id_text)?
        .ok_or_else(|| {
            format!("Session `{session_id}` has no segmented directory for Core VMM persistence.")
        })?;
    write_core_vmm_controller_state(&session_dir, &session_id_text, state)
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
    data_root: &Path,
    session_id: SessionId,
) -> Result<bool, String> {
    // session_id_text is the canonical key for exact active-or-archived directory lookup.
    // session_id_text 是精确查找活动或归档目录的规范键。
    let session_id_text = session_id.to_string();
    // session_dir is never created by deletion because absence is an idempotent outcome.
    // session_dir 不会由删除操作创建，因为缺失属于幂等结果。
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Ok(false);
    };
    reset_vmm_state_for_history_rewrite(&session_dir)?;
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
    data_root: &Path,
    session_id: SessionId,
    updated_at: &str,
) -> Result<bool, String> {
    // session_id_text is the canonical key shared by classification, reset, and the new document.
    // session_id_text 是分类、重置与新文档共同使用的规范键。
    let session_id_text = session_id.to_string();
    // session_dir resolves only an existing active or archived conversation.
    // session_dir 仅解析一个已存在的活动或归档会话。
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Ok(false);
    };
    // rebuild_reason gates destructive VMM reset to a storage-classified damaged attachment.
    // rebuild_reason 将破坏性 VMM 重置限制为存储层已分类的损坏附件。
    let Some(_rebuild_reason) = core_vmm_rebuild_reason(&session_dir, &session_id_text)? else {
        return Err(format!(
            "Session `{session_id}` VMM attachment is healthy or absent and cannot be rebuilt."
        ));
    };
    // replacement_state contains no recalled memory and uses only the current strict document schema.
    // replacement_state 不包含任何召回记忆，并且仅使用当前严格文档结构。
    let replacement_state =
        vulcan_vmm::VmmSessionMemoryState::new(session_id_text.clone(), updated_at)?;
    reset_vmm_state_for_history_rewrite(&session_dir)?;
    write_core_vmm_controller_state(&session_dir, &session_id_text, &replacement_state)?;
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

use boundary_store::{
    find_existing_segmented_context_session_dir, session_boundary_event_from_segment_line,
};
use loading::sanitize_loaded_runtime_state_for_fresh_process;
use queries::*;
use serialization::*;

#[cfg(test)]
mod tests;

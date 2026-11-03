//! In-process persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的进程内持久化辅助模块。

use crate::data_plane::HostRuntimeHandle;
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use crate::model::{
    SessionId, SessionRunPhase, SessionRuntimeStateEntry, SessionState, SessionSummary,
    SessionTitleSource, TaskProjectSummary,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use vulcan_sqlite_engine::sql_exec::SqliteValue;

mod ai_credentials;
mod environment_variables;
mod git_preferences;
mod persistence;
mod project_groups;
mod provider_api_trace;
mod provider_usage;
mod resource_catalog;
mod session_index_v1;
mod session_sidecars;
mod session_usage;
mod session_v1;
mod session_v1_store;
mod session_vmm_v1;

use self::session_v1::atomic_io::write_file_atomic;
pub use ai_credentials::{
    AiCredentialSecretRecord, create_ai_credential_secret_record,
    delete_ai_credential_secret_record, load_ai_credential_secret_record,
    replace_ai_credential_secret_record, restore_ai_credential_secret_record,
};
pub use environment_variables::{
    ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS, ENVIRONMENT_VARIABLE_NAME_MAX_CHARS,
    ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES, EffectiveEnvironmentVariable, EnvironmentVariableDraft,
    EnvironmentVariableKind, EnvironmentVariableLayer, EnvironmentVariableRecord,
    EnvironmentVariableScope, RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX,
    RESERVED_ENVIRONMENT_VARIABLE_NAMES, SessionEnvironmentVariable, create_environment_variable,
    delete_environment_variable, environment_variable_name_key, list_environment_variables,
    load_session_environment, normalized_environment_variable_description,
    resolve_effective_environment, save_session_environment, store_environment_variable_value,
    update_environment_variable, validate_environment_variable_name,
    validate_environment_variable_value,
};
pub use git_preferences::{
    load_git_repository_monitoring_preferences, persist_git_repository_monitoring_preference,
};
pub use persistence::{
    delete_active_project_row_v1, persist_collapsed_task_project_ids, persist_new_project,
    persist_project_icon_update, persist_project_order_update,
    persist_project_pinned_and_order_update, persist_project_session_sort_mode, rename_project,
    restore_archived_project_row_v1,
};
pub use project_groups::{
    DEFAULT_PROJECT_GROUP_ID, PROJECT_GROUP_NAME_MAX_CHARS, PROJECT_GROUPS_FORMAT_VERSION,
    ProjectGroup, ProjectGroupSlot, ProjectTree, ProjectTreePin, ProjectTreePinKind,
    load_project_tree, normalize_project_group_name, persist_project_tree, validate_project_tree,
};
pub use provider_api_trace::{
    PROVIDER_API_TRACE_LIMIT, ProviderApiTraceAttemptReservation, ProviderApiTraceDiagnostic,
    ProviderApiTraceDiagnosticCode, ProviderApiTracePayload, ProviderApiTraceReservationOutcome,
    ProviderApiTraceStore, ProviderApiTraceWriteOutcome, REDACTED_PROVIDER_SECRET,
    redact_provider_secrets,
};
pub use resource_catalog::{
    MISSING_RESOURCE_PLACEHOLDER, ProjectResourceCatalog, ProjectResourceCatalogError,
    ProjectResourceCatalogRegistry, ProjectResourceRegistration, ProjectResourceResolution,
    ProjectResourceSource, ProjectResourceUploadReceipt, ProviderBinding, ResourceCatalogEntry,
    add_project_resource, load_project_resource_catalog, parse_project_resource_locator,
    project_resource_catalog_path, project_resource_entity_path, project_resource_locator,
    resolve_project_resource, resolve_project_resource_source,
};
pub use session_index_v1::SESSION_ASSISTANT_NAME_MAX_CHARS;
#[cfg(test)]
pub(crate) use session_index_v1::SESSION_INDEX_V1_SCHEMA_SQL;
pub use session_index_v1::{record_session_request_v1, set_project_session_order_v1};
pub use session_sidecars::{
    load_session_outside_file_access, load_session_review_prompt, load_session_subagent_models,
    load_session_subagent_models_with_upgrade, save_session_outside_file_access,
    save_session_review_prompt, save_session_subagent_models,
};
pub use session_usage::{load_session_lifetime_usage, save_session_lifetime_usage};
#[cfg(feature = "test-support")]
pub use session_v1_store::create_session_v1_storage_fixture;
pub use session_v1_store::{
    CreateEmbeddedTaskSessionV1Request, CreateSessionV1Request, CreatedSessionV1,
    DeleteSessionV1Outcome, LoadedSessionV1, SESSION_V1_STORAGE_PARTITION,
    SessionIndexV1ConflictField, SessionIndexV1Error, SessionV1CollaborationRecoverySnapshot,
    SessionV1EventRange, SessionV1EventWindowState, SessionV1FlushResult, SessionV1ListEntry,
    SessionV1LoadReport, SessionV1StorageError, SessionV1StorageErrorCode, SessionV1StorageLocator,
    SessionV1StoragePreflightOutcome, SessionV1StoreError, SessionV1TurnEventWindow,
    create_embedded_task_session_v1, create_indexed_session_v1, delete_session_v1,
    ensure_session_v1_storage_schema, flush_session_v1_candidate, list_active_sessions_v1,
    list_archived_sessions_v1, load_session_v1_for_runtime, load_session_v1_full,
    load_session_v1_tail, locate_embedded_task_session_v1, locate_optional_session_v1_storage,
    locate_session_v1_storage, preflight_session_v1_storage, project_resource_directory_v1,
    project_session_lifecycle_v1, publish_edit_session_v1, session_v1_format_is_unsupported,
    set_session_assistant_v1, set_session_unread_v1,
};
pub use session_vmm_v1::{load_session_vmm_state, persist_session_vmm_state};

pub use provider_usage::{
    PROVIDER_USAGE_SCHEMA_REVISION, ProviderAccountUsageSnapshotRecord,
    ProviderAccountUsageSummary, ProviderRateLimitSnapshotRecord, ProviderUsageAttemptRecord,
    ProviderUsageAttemptSummary, ProviderUsageDailySummary, ProviderUsageDashboard,
    ensure_provider_usage_schema, load_provider_usage_dashboard, lock_provider_usage_writes,
    persist_provider_account_usage_snapshot, persist_provider_rate_limit_snapshot,
    persist_provider_usage_attempt, persist_provider_usage_attempt_and_rebuild_daily,
    persist_provider_usage_attempt_batch_and_rebuild_daily, rebuild_provider_usage_daily,
};
/// App metadata key used to persist collapsed task-project identifiers.
/// 用于持久化已折叠任务项目标识的应用元数据键。
const COLLAPSED_TASK_PROJECTS_METADATA_KEY: &str = "workspace.collapsed_task_projects";
/// Application-level directory that owns configured-inference request traces without a conversation session.
/// 保存无对话会话归属的已配置推理请求追踪的应用级目录。
const CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY: &str =
    "configured-inference-provider-request-trace";
/// Number of exact rotating files retained for configured inference diagnostics.
/// 已配置推理诊断保留的精确轮转文件数量。
pub const CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SLOT_COUNT: u64 = 4;
/// Maximum encoded bytes retained in one configured inference trace slot.
/// 单个已配置推理追踪槽位保留的最大编码字节数。
pub const CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Application-level directory that owns rejected native VMM structured outputs.
/// 保存被拒绝的原生 VMM 结构化输出的应用级目录。
const VMM_STRUCTURED_OUTPUT_FAILURE_DIRECTORY: &str = "vmm-structured-output-failures";
/// Number of exact rotating files retained for rejected VMM structured outputs.
/// 被拒绝的 VMM 结构化输出保留的精确轮转文件数量。
pub const VMM_STRUCTURED_OUTPUT_FAILURE_SLOT_COUNT: u64 = 8;
/// Maximum encoded bytes retained in one rejected VMM structured-output slot.
/// 单个被拒绝 VMM 结构化输出槽位保留的最大编码字节数。
pub const VMM_STRUCTURED_OUTPUT_FAILURE_MAX_BYTES: u64 = 1024 * 1024;
/// Serializes configured inference trace-slot replacement.
/// 串行化已配置推理追踪槽位替换。
static CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_WRITE_LOCK: OnceLock<Mutex<()>> =
    OnceLock::new();
/// Persists one bounded configured-inference provider trace into an exact rotating slot.
/// 将一份有界的已配置推理 Provider 追踪持久化到精确轮转槽位。
///
/// # Parameters
/// 参数
/// `data_root` is the host_runtime-managed Vulcan data root; `consumer_profile_id` is the exact
/// authenticated internal consumer; `slot_index` is the deterministic bounded slot; and
/// `trace` is the complete secret-free diagnostic document.
/// `data_root` 是宿主运行时管理的 Vulcan 数据根目录；`consumer_profile_id` 是精确的
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
    if slot_index >= CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SLOT_COUNT {
        return Err(format!(
            "Configured-inference trace slot {slot_index} is outside the 0..{} range.",
            CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SLOT_COUNT - 1
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

/// Persists one rejected native VMM structured output into an exact rotating slot.
/// 将一份被拒绝的原生 VMM 结构化输出持久化到精确轮转槽位。
///
/// # Parameters
/// 参数
/// `data_root` is the host_runtime-managed Vulcan data root; `slot_index` is the deterministic
/// bounded slot; and `record` is the complete secret-free diagnostic document whose text fields
/// the caller has already truncated.
/// `data_root` 是宿主运行时管理的 Vulcan 数据根目录；`slot_index` 是确定性有界槽位；
/// `record` 是调用方已截断文本字段的完整不含秘密诊断文档。
///
/// # Returns
/// 返回值
/// The final record path after atomic publication, or an explicit validation and storage error.
/// 原子发布完成后返回最终记录路径，或返回明确的校验与存储错误。
pub fn persist_vmm_structured_output_failure(
    data_root: &Path,
    slot_index: u64,
    record: &Value,
) -> Result<PathBuf, String> {
    if slot_index >= VMM_STRUCTURED_OUTPUT_FAILURE_SLOT_COUNT {
        return Err(format!(
            "VMM structured-output failure slot {slot_index} is outside the 0..{} range.",
            VMM_STRUCTURED_OUTPUT_FAILURE_SLOT_COUNT - 1
        ));
    }
    let encoded = serde_json::to_vec(record).map_err(|error| {
        format!("VMM structured-output failure JSON serialization failed: {error}")
    })?;
    if u64::try_from(encoded.len()).unwrap_or(u64::MAX) > VMM_STRUCTURED_OUTPUT_FAILURE_MAX_BYTES {
        return Err(format!(
            "VMM structured-output failure record exceeds its {VMM_STRUCTURED_OUTPUT_FAILURE_MAX_BYTES}-byte slot capacity."
        ));
    }
    let record_path = data_root
        .join("diagnostics")
        .join(VMM_STRUCTURED_OUTPUT_FAILURE_DIRECTORY)
        .join(format!("slot-{slot_index:02}.json"));
    replace_exact_provider_request_trace_slot(
        &record_path,
        &encoded,
        "VMM structured-output failure record",
    )?;
    Ok(record_path)
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
    if encoded_len <= CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_MAX_BYTES {
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
            "maximum_bytes": CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_MAX_BYTES,
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
    if u64::try_from(summary_bytes.len()).unwrap_or(u64::MAX)
        > CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_MAX_BYTES
    {
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
    let _write_guard = CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_WRITE_LOCK
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
    write_file_atomic(path, encoded, artifact_kind).map_err(|error| error.to_string())
}

mod index_read;
mod load_report;
mod loading;
pub use load_report::{WorkspaceIndexLoadIssue, WorkspaceIndexLoadReport, WorkspaceIndexLoadStage};
mod models;
mod queries;
mod serialization;

pub use loading::*;
pub use models::*;
use queries::*;
use serialization::*;

#[cfg(test)]
mod tests;

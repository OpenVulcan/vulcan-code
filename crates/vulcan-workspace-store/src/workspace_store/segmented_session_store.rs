//! Segmented append-only session storage used by the new Vulcan session fact source.
//! Vulcan 新会话事实源使用的分段追加式会话存储。

use super::current_unix_millis_text;
use crate::conversation_protocol::{
    SessionConversationState, conversation_state_from_session_messages,
    conversation_state_to_session_messages,
};
use crate::model::{
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRuntimeStateEntry,
    SessionTitleSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
use vulcan_context::{
    RUNTIME_LOG_SCHEMA_VERSION, RuntimeContextCompactionAppliedEvent, RuntimeInjectionAppliedEvent,
    RuntimeLogEvent, RuntimeLogEventLine, RuntimeRepairEvent,
    conversation_state_from_runtime_log_lines, decode_runtime_log_event_line_json,
    runtime_log_lines_from_conversation_state,
};
#[cfg(test)]
use vulcan_context::{
    RuntimeInjectionCancelledEvent, RuntimeInjectionMergedEvent,
    RuntimeProviderLoweringRepairEvent, RuntimeProviderNativeStateEnvelope,
    RuntimeProviderRequestCheckpointEvent, RuntimeProviderResponseCheckpointEvent,
    RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
};
use vulcan_session_boundary::SessionBoundaryEvent;

/// Directory name used for active sessions below the Vulcan code data root.
/// Vulcan Code 数据根目录下用于活动会话的目录名称。
pub(super) const SEGMENTED_SESSIONS_DIR: &str = "sessions";
/// Directory name used for archived sessions below the Vulcan code data root.
/// Vulcan Code 数据根目录下用于归档会话的目录名称。
pub(super) const SEGMENTED_ARCHIVED_SESSIONS_DIR: &str = "archived_sessions";
/// Metadata file name stored in every main session and task session directory.
/// 每个主会话与子任务会话目录内保存的元数据文件名。
pub(super) const SEGMENTED_METADATA_FILE: &str = "metadata.json";
/// Runtime state file name stored in every main session and task session directory.
/// 每个主会话与子任务会话目录内保存的运行状态文件名。
pub(super) const SEGMENTED_STATE_FILE: &str = "state.json";
/// Runtime metadata file name stored beside display transcript segments.
/// 保存在展示转录分段旁边的运行时元数据文件名。
pub(super) const SEGMENTED_RUNTIME_METADATA_FILE: &str = "runtime_metadata.json";
/// Runtime index file name rebuilt from runtime JSONL when needed.
/// 需要时可从 runtime JSONL 重建的运行时索引文件名。
pub(super) const SEGMENTED_RUNTIME_INDEX_FILE: &str = "runtime.index.json";
/// Runtime segment directory name used for provider-facing fact logs.
/// 用于 Provider 可见事实日志的运行时分段目录名。
pub(super) const SEGMENTED_RUNTIME_DIR: &str = "runtime";
/// Runtime backup directory name used before context compaction.
/// 上下文压缩前使用的运行时备份目录名。
pub(super) const SEGMENTED_RUNTIME_BACKUP_DIR: &str = "runtime_backup";
/// Display maintenance backup directory name stored beside one session directory.
/// 保存在单个会话目录旁边的显示层维护备份目录名。
const SEGMENTED_DISPLAY_MAINTENANCE_BACKUP_DIR: &str = ".display_maintenance_backup";
/// Child task directory name used by the new session layout.
/// 新会话布局中用于保存子任务的目录名称。
pub(super) const SEGMENTED_TASK_DIR: &str = "task";
/// Blob directory name used by the new session layout.
/// 新会话布局中用于保存外置大对象的目录名称。
pub(super) const SEGMENTED_BLOB_DIR: &str = "blobs";
/// Default maximum line count allowed in one segment before rollover.
/// 单个分段在滚动前默认允许的最大行数。
pub(super) const DEFAULT_SEGMENT_MAX_LINES: u64 = 3000;
/// Default maximum byte count allowed in one segment before rollover.
/// 单个分段在滚动前默认允许的最大字节数。
pub(super) const DEFAULT_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum runtime-log backups retained after context compaction.
/// 上下文压缩后保留的运行时日志备份最大数量。
const RUNTIME_COMPACTION_BACKUP_LIMIT: usize = 5;
/// Process-local mutex protecting overwrite-only state read/modify/write operations.
/// 保护覆盖式状态读改写操作的进程内互斥锁。
static SEGMENTED_STATE_FILE_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

mod message_events;
mod message_mapping;
mod record_types;
mod runtime_backup;
mod runtime_io;
mod runtime_log;
mod segment_io;
mod segment_types;
mod session_fork;
mod session_management;
mod session_reading;
mod state_documents;
mod task_types;

pub(super) use message_events::*;
pub(super) use message_mapping::*;
pub(super) use record_types::*;
pub(super) use runtime_backup::*;
pub(super) use runtime_io::*;
pub(super) use runtime_log::*;
pub(super) use segment_io::*;
pub(super) use segment_types::*;
pub(super) use session_fork::*;
pub(super) use session_management::*;
pub(super) use session_reading::*;
pub(super) use state_documents::*;
pub(super) use task_types::*;

/// Builds an active or archived session directory path.
/// 构建活动或归档会话目录路径。
pub(super) fn segmented_session_directory(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
) -> PathBuf {
    // Keep project and session path components filesystem-safe before joining them.
    // 在拼接路径前确保项目与会话路径片段对文件系统安全。
    let safe_project_tag = sanitize_file_component(project_tag);
    // Keep the date visible while retaining the stable session id in the leaf name.
    // 保留日期可读性，同时在叶子目录名中保留稳定会话标识。
    let session_dir_name = format!(
        "{}-{}",
        sanitize_file_component(created_date),
        sanitize_file_component(session_id)
    );
    // Select the active or archived root without changing the leaf layout.
    // 选择活动或归档根目录，同时保持叶子布局不变。
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    data_root
        .join(root_name)
        .join(safe_project_tag)
        .join(session_dir_name)
}

/// Creates the initial main session directory in the active or archived tree.
/// 在活动树或归档树中创建初始主会话目录。
pub(super) fn create_segmented_session_in_location(
    data_root: &Path,
    archived: bool,
    request: SegmentedSessionCreateRequest,
) -> Result<PathBuf, String> {
    // Resolve the canonical active session directory for this request.
    // 为当前请求解析规范会话目录。
    let session_dir = segmented_session_directory(
        data_root,
        archived,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    create_dir_all(&session_dir)?;
    create_dir_all(&session_dir.join(SEGMENTED_TASK_DIR))?;
    create_dir_all(&session_dir.join(SEGMENTED_BLOB_DIR))?;

    // Build the first segment and metadata document before writing any JSONL row.
    // 在写入任何 JSONL 行前构建首个分段与元数据文档。
    let first_segment = SegmentMetadata::empty(segment_file_name(1), 0);
    // Store metadata as the overwrite-only directory index.
    // 将元数据作为覆盖式目录索引保存。
    let mut metadata = SegmentedSessionMetadata {
        schema_version: 1,
        session_id: request.session_id.clone(),
        project_tag: request.project_tag.clone(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at.clone(),
        project_id: request.project_id,
        title: request.title,
        title_source: request.title_source,
        canonical_source: "segmented_rollout".to_string(),
        archived_at: archived.then(|| request.created_at.clone()),
        active_segment: first_segment.file.clone(),
        task_dir: SEGMENTED_TASK_DIR.to_string(),
        next_global_seq: 0,
        message_count: 0,
        last_message_id: None,
        last_message_fingerprint: None,
        latest_rollback_seq: None,
        segment_policy: request.segment_policy,
        segments: vec![first_segment],
        tasks: Vec::new(),
        fork_source: None,
    };
    // The first JSONL line stores the canonical SessionMeta audit record.
    // 首条 JSONL 行存储规范 SessionMeta 审计记录。
    let session_meta_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SessionMeta,
        payload: json!({
            "session_id": request.session_id,
            "project_tag": request.project_tag,
            "canonical_source": "segmented_rollout"
        }),
        turn_index: None,
    };
    append_events_with_metadata(&session_dir, &mut metadata, &[session_meta_event])?;
    write_metadata(&session_dir, &metadata)?;
    write_state(
        &session_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: metadata.session_id.clone(),
            updated_at: metadata.updated_at.clone(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        },
    )?;
    Ok(session_dir)
}

/// Finds an existing segmented session directory by project tag and session id.
/// 按项目标签与会话标识查找既有分段会话目录。
pub(super) fn find_segmented_session_directory(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    session_id: &str,
) -> Result<Option<PathBuf>, String> {
    // Resolve the project root using the same sanitization as path construction.
    // 使用与路径构建相同的清理规则解析项目根目录。
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    let project_root = data_root
        .join(root_name)
        .join(sanitize_file_component(project_tag));
    if !project_root.exists() {
        return Ok(None);
    }
    let expected_suffix = format!("-{}", sanitize_file_component(session_id));
    for entry in fs::read_dir(&project_root).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.ends_with(&expected_suffix) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Finds an existing segmented session directory by its deterministic project/created-at/session locator.
/// 通过确定性的项目、创建时间与会话标识定位既有分段会话目录。
///
/// # Parameters
/// 参数
/// `data_root` is the application data root that owns active and archived session roots.
/// `data_root` 是拥有活动与归档会话根目录的应用数据根。
/// `project_tag` is the persisted project scope.
/// `project_tag` 是持久化项目作用域。
/// `created_at` is the raw Unix-milliseconds creation timestamp used to derive the directory date.
/// `created_at` 是用于推导目录日期的原始 Unix 毫秒创建时间戳。
/// `session_id` is the stable session identifier.
/// `session_id` 是稳定会话标识。
///
/// # Returns
/// 返回值
/// The active or archived deterministic path when it exists; otherwise `None`.
/// 若活动或归档确定路径存在则返回该路径，否则返回 `None`。
pub fn find_segmented_session_directory_for_task_store_created_at(
    data_root: &Path,
    project_tag: &str,
    created_at: &str,
    session_id: &str,
) -> Option<PathBuf> {
    if created_at.trim().is_empty() {
        return None;
    }
    // created_date is absent when the optional task-store locator receives invalid timestamp text.
    // created_date 在可选任务存储定位器收到无效时间戳文本时保持缺失。
    let created_date = super::session_directory_date_from_unix_millis_text(created_at).ok()?;
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, &created_date, session_id);
    if active_dir.is_dir() {
        return Some(active_dir);
    }
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, &created_date, session_id);
    archived_dir.is_dir().then_some(archived_dir)
}

/// Lists active and archived segmented sessions from text metadata.
/// 从文本元数据列出活动与归档分段会话。
pub(super) fn list_segmented_session_catalog(
    data_root: &Path,
) -> Result<Vec<SegmentedSessionCatalogEntry>, String> {
    let mut entries = Vec::new();
    entries.extend(list_segmented_session_catalog_for_root(data_root, false)?);
    entries.extend(list_segmented_session_catalog_for_root(data_root, true)?);
    Ok(entries)
}

/// Finds one segmented session directory by id without consulting the database.
/// 不查询数据库，仅按会话标识查找分段会话目录。
pub(super) fn find_segmented_session_directory_by_id(
    data_root: &Path,
    session_id: &str,
) -> Result<Option<PathBuf>, String> {
    for entry in list_segmented_session_catalog(data_root)? {
        if entry.metadata.session_id == session_id {
            return Ok(Some(entry.session_dir));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;

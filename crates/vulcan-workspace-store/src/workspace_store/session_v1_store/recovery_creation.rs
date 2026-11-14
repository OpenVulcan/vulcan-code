//! Resumes the index publication for one explicitly retried recovery creation.
//! 续接一次显式重试的恢复会话索引发布。

use super::*;
use crate::workspace_store::{
    session_index_v1::insert_recovery_session_index_v1,
    session_usage::ensure_recovery_session_lifetime_usage,
};
use uuid::Uuid;

/// The versioned directory holds only pending recovery index rows, addressed by operation UUID.
/// 版本化目录仅存放待发布的恢复索引行，按操作 UUID 寻址。
const RECOVERY_INDEX_DIRECTORY: &str = ".recovery-index-v1";

/// Creates the recovery tree and its index while the caller holds session and project admission.
/// 在调用方持有会话及项目准入期间创建恢复目录树和索引。
/// The host owns persistence; the request contains the draft and locator, and session_id binds the operation.
/// 宿主持有持久化状态；请求包含草稿和定位信息，session_id 绑定该操作。
/// Returns the indexed session, or leaves the prepared row available when publication fails.
/// 返回已索引会话；发布失败时保留待发布索引行。
pub async fn create_indexed_recovery_session_v1(
    host_runtime: &HostRuntimeHandle,
    request: &CreateSessionV1Request<'_>,
    session_id: Uuid,
) -> Result<CreatedSessionV1, SessionV1StoreError> {
    let row = create_index_row(request)?;
    let pending_path = recovery_index_path(host_runtime, session_id);
    validate_recovery_index_row(&row, session_id, &pending_path)?;
    validate_new_session_index_v1(host_runtime, &row).await?;
    // Publish the locator before the writer can make the session tree visible on disk.
    // 在写入器使会话目录树在磁盘上可见之前发布定位记录。
    super::super::session_v1::documents::write_json_document(
        &pending_path,
        &row,
        "recovery index preparation",
    )?;
    let created = create_session_v1_tree(&host_runtime.data_root, request, &row)?;
    publish_recovery_index(host_runtime, &row, &created.locator, &pending_path).await?;
    Ok(created)
}

/// The verified index and tree for a recovery operation that failed before SQLite publication.
/// SQLite 发布失败后，恢复操作已校验的索引和目录树。
pub struct PreparedRecoverySessionIndexV1 {
    /// The exact index row from the operation's durable preparation.
    /// 恢复操作待发布记录中的精确索引行。
    row: SessionIndexV1Row,
    /// Validated locator for the first published tree.
    /// 首次发布目录树的已校验定位器。
    locator: SessionV1StorageLocator,
    /// The retry record to retire only after index commit.
    /// 仅在索引提交后清除的重试记录。
    pending_path: PathBuf,
}

impl PreparedRecoverySessionIndexV1 {
    /// Returns the exact owning project ID for the caller's publication admission.
    /// 返回精确所属项目标识，供调用方取得发布准入。
    pub fn project_id(&self) -> &str {
        &self.row.project_id
    }

    /// Publishes the verified row to the same host while the caller holds session and project admission.
    /// 在调用方持有会话及项目准入期间，将已校验的索引行发布到同一个宿主。
    /// Returns success once usage and index are durable, or leaves the preparation available for retry.
    /// 用量和索引可靠落盘后返回成功，失败则保留待发布记录以供重试。
    pub async fn publish(
        self,
        host_runtime: &HostRuntimeHandle,
    ) -> Result<(), SessionV1StoreError> {
        publish_recovery_index(host_runtime, &self.row, &self.locator, &self.pending_path).await
    }
}

/// Reads a previously prepared recovery index and verifies the first published tree.
/// 读取待发布的恢复索引并校验首次发布的目录树。
/// The host owns the database and files; session_id is derived from the complete recovery operation.
/// 宿主持有数据库与文件；session_id 从完整恢复操作派生。
/// Returns the verified preparation, or None when no tree was published.
/// 返回已校验的待发布记录，未发布目录树时返回 None。
pub fn prepare_recovery_session_index_retry_v1(
    host_runtime: &HostRuntimeHandle,
    session_id: Uuid,
) -> Result<Option<PreparedRecoverySessionIndexV1>, SessionV1StoreError> {
    let pending_path = recovery_index_path(host_runtime, session_id);
    let bytes = match fs::read(&pending_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SessionV1StorageError::io(&pending_path, error.to_string()).into());
        }
    };
    let row: SessionIndexV1Row = serde_json::from_slice(&bytes).map_err(|error| {
        SessionV1StorageError::corrupt(
            SessionV1StorageErrorCode::MetadataCorrupt,
            &pending_path,
            format!("Invalid recovery index preparation: {error}"),
        )
    })?;
    validate_recovery_index_row(&row, session_id, &pending_path)?;
    let locator = storage_locator_from_row(&host_runtime.data_root, row.clone())?;
    let session_directory = locator.session_directory();
    match fs::symlink_metadata(&session_directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        // No complete tree was published. The caller may still prepare the first draft.
        // 完整目录树尚未发布，调用方仍可准备首次草稿。
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(_) => {
            return Err(SessionV1StorageError::io(
                &session_directory,
                "Recovery session tree is not a regular directory.",
            )
            .into());
        }
        Err(error) => {
            return Err(SessionV1StorageError::io(&session_directory, error.to_string()).into());
        }
    }
    let loaded = load_session(&locator.layout)?;
    // Only the unchanged initial recovery draft can be published through this prepared row.
    // 只有未变更的初始恢复草稿能通过此待发布索引行发布。
    let verified_row = create_index_row(&CreateSessionV1Request {
        cleaned_history: None,
        project_id: &row.project_id,
        storage_partition: SESSION_V1_STORAGE_PARTITION,
        date_bucket: &locator.layout.date_bucket,
        metadata: &loaded.metadata,
        runtime_lock: &loaded.document.runtime_lock,
        compactions: &loaded.document.compactions,
        events: loaded.document.events(),
    })?;
    if row != verified_row
        || !loaded.document.events().is_empty()
        || loaded.metadata.cleaned_history.is_some()
        || loaded.metadata.dedicated.is_some()
        || loaded.metadata.fork_origin.is_some()
        || !loaded.document.compactions.is_empty()
    {
        return Err(SessionV1StorageError::corrupt(
            SessionV1StorageErrorCode::MetadataCorrupt,
            locator.layout.metadata_path(),
            "The published tree does not match the prepared recovery index.",
        )
        .into());
    }
    Ok(Some(PreparedRecoverySessionIndexV1 {
        row,
        locator,
        pending_path,
    }))
}

/// Completes usage and index publication for the verified initial recovery tree.
/// 为已校验的初始恢复目录树完成用量和索引发布。
/// The host owns SQLite, row and locator identify the tree, and pending_path is its retry record.
/// 宿主持有 SQLite，索引行和定位器标识目录树，pending_path 是其重试记录。
/// Returns success only after usage and the project-checked index are durable.
/// 仅在用量和通过项目校验的索引均可靠落盘后返回成功。
async fn publish_recovery_index(
    host_runtime: &HostRuntimeHandle,
    row: &SessionIndexV1Row,
    locator: &SessionV1StorageLocator,
    pending_path: &Path,
) -> Result<(), SessionV1StoreError> {
    // An index observer can cold-load immediately after commit, so usage must be ready first.
    // 索引观察者可在提交后立即冷加载，因此必须先准备好用量。
    ensure_recovery_session_lifetime_usage(locator, row.created_at_ms)
        .map_err(|error| SessionV1StorageError::io(locator.session_directory(), error))?;
    insert_recovery_session_index_v1(host_runtime, row).await?;
    retire_recovery_index_preparation(pending_path);
    Ok(())
}

/// Returns the pending row path for session_id under the host's session root.
/// 返回宿主会话根目录下 session_id 对应的待发布索引行路径。
fn recovery_index_path(host_runtime: &HostRuntimeHandle, session_id: Uuid) -> PathBuf {
    host_runtime
        .data_root
        .join(SESSION_V1_STORAGE_PARTITION)
        .join(RECOVERY_INDEX_DIRECTORY)
        .join(format!("{session_id}.json"))
}

/// Validates the prepared row's operation, project ownership, and active initial state.
/// 校验待发布索引行的操作、项目归属与初始活动状态。
/// The path is diagnostic context; mismatching rows are rejected before any tree or index access.
/// 路径用于诊断；不匹配的索引行在访问目录树或索引前被拒绝。
fn validate_recovery_index_row(
    row: &SessionIndexV1Row,
    session_id: Uuid,
    path: &Path,
) -> Result<(), SessionV1StorageError> {
    if row.session_id != session_id.to_string()
        || row.project_id != row.project_tag
        || row.storage_partition != SESSION_V1_STORAGE_PARTITION
        || row.archived_at_ms.is_some()
        || row.unread != 0
    {
        return Err(SessionV1StorageError::corrupt(
            SessionV1StorageErrorCode::MetadataCorrupt,
            path,
            "Invalid ownership or state in the prepared recovery index.",
        ));
    }
    Ok(())
}

/// Removes the pending row at path only after SQLite has committed the recovery result.
/// 仅在 SQLite 提交恢复结果后移除 path 处的待发布索引行。
/// A cleanup failure leaves the committed SQLite row authoritative.
/// 清理失败时，已提交的 SQLite 索引行仍是权威。
fn retire_recovery_index_preparation(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), %error, "Failed to retire a committed recovery index preparation");
    }
}

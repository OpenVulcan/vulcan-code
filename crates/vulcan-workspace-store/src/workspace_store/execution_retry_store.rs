//! Atomic sidecar storage for exact execution-retry snapshots.
//! 精确执行重试快照的原子 sidecar 存储。

use super::segmented_session_store::write_file_bytes_atomic;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction, ExecutionRetrySnapshot,
    ExecutionRetrySnapshotRef,
};

/// Directory name containing retry sidecars below one segmented session.
/// 单个分段会话下保存重试 sidecar 的目录名。
const EXECUTION_RETRY_DIRECTORY: &str = "execution_retry";

/// Owns strict read, write, validation, and deletion of retry sidecars for one session directory.
/// 负责单个会话目录中重试 sidecar 的严格读写、校验与删除。
#[derive(Debug, Clone)]
pub struct ExecutionRetrySnapshotStore {
    /// Exact segmented main-session directory.
    /// 精确的分段主会话目录。
    session_directory: PathBuf,
}

impl ExecutionRetrySnapshotStore {
    /// Creates a retry snapshot store rooted at one exact segmented session directory.
    /// 创建一个以精确分段会话目录为根的重试快照存储。
    ///
    /// # Parameters
    /// 参数
    /// `session_directory` is the validated main-session directory selected by workspace storage.
    /// `session_directory` 是工作区存储选定的已校验主会话目录。
    ///
    /// # Returns
    /// 返回值
    /// A path-bound store that performs no I/O until an operation is invoked.
    /// 返回路径绑定的存储；在调用具体操作前不执行 I/O。
    pub fn new(session_directory: impl Into<PathBuf>) -> Self {
        Self {
            session_directory: session_directory.into(),
        }
    }

    /// Atomically publishes one complete retry snapshot and returns its lightweight reference.
    /// 原子发布一份完整重试快照并返回其轻量引用。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the complete exact execution payload serialized before any final path changes.
    /// `snapshot` 是在最终路径变化前完整序列化的精确执行载荷。
    ///
    /// # Returns
    /// 返回值
    /// A digest-bound reference suitable for synchronous publication in session runtime state.
    /// 返回适合同步发布到会话运行态的摘要绑定引用。
    pub fn write(
        &self,
        snapshot: &ExecutionRetrySnapshot,
    ) -> Result<ExecutionRetrySnapshotRef, String> {
        validate_snapshot_identity(snapshot)?;
        // encoded is complete before the atomic writer is allowed to mutate the filesystem.
        // encoded 在允许原子写入器修改文件系统前已经完整生成。
        let encoded = serde_json::to_vec(snapshot)
            .map_err(|error| format!("Failed to serialize execution retry snapshot: {error}"))?;
        // contentSha256 binds the lightweight main-state reference to the exact sidecar bytes.
        // contentSha256 将轻量主状态引用绑定到精确 sidecar 字节。
        let content_sha256 = sha256_hex(&encoded);
        let content_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        let path = self.snapshot_path(snapshot.snapshot_id.as_str())?;
        write_file_bytes_atomic(&path, &encoded, "execution retry snapshot")?;
        Ok(ExecutionRetrySnapshotRef {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: snapshot.snapshot_id.clone(),
            generation: snapshot.generation,
            state: snapshot.state,
            content_sha256,
            content_bytes,
            logical_execution_id: snapshot.logical_execution_id.clone(),
            pending_action_label: pending_action_label(&snapshot.pending_action).to_string(),
            claimed_attempt_id: snapshot.claimed_attempt_id.clone(),
            error_summary: snapshot.error.as_ref().map(|error| error.message.clone()),
            updated_at: snapshot.updated_at.clone(),
        })
    }

    /// Reads and strictly validates one exact digest-bound retry snapshot.
    /// 读取并严格校验一份精确摘要绑定的重试快照。
    ///
    /// # Parameters
    /// 参数
    /// `reference` is the lightweight state reference loaded from the owning session.
    /// `reference` 是从所属会话加载的轻量状态引用。
    ///
    /// # Returns
    /// 返回值
    /// The exact decoded snapshot only when schema, identity, size, digest, generation, and state all match.
    /// 仅当 Schema、身份、大小、摘要、代际与状态全部一致时返回精确解码快照。
    pub fn read(
        &self,
        reference: &ExecutionRetrySnapshotRef,
    ) -> Result<ExecutionRetrySnapshot, String> {
        if reference.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported execution retry reference schema version `{}`; expected `{}`.",
                reference.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
            ));
        }
        let path = self.snapshot_path(reference.snapshot_id.as_str())?;
        let encoded = fs::read(&path).map_err(|error| {
            format!(
                "Failed to read execution retry snapshot `{}`: {error}",
                path.display()
            )
        })?;
        let actual_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        if actual_bytes != reference.content_bytes {
            return Err(format!(
                "Execution retry snapshot byte length mismatch: expected `{}`, actual `{actual_bytes}`.",
                reference.content_bytes
            ));
        }
        let actual_sha256 = sha256_hex(&encoded);
        if actual_sha256 != reference.content_sha256 {
            return Err(String::from(
                "Execution retry snapshot SHA-256 digest mismatch.",
            ));
        }
        let snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(&encoded)
            .map_err(|error| format!("Failed to decode execution retry snapshot: {error}"))?;
        validate_snapshot_identity(&snapshot)?;
        if snapshot.snapshot_id != reference.snapshot_id
            || snapshot.generation != reference.generation
            || snapshot.state != reference.state
            || snapshot.logical_execution_id != reference.logical_execution_id
            || snapshot.claimed_attempt_id != reference.claimed_attempt_id
        {
            return Err(String::from(
                "Execution retry snapshot identity or lifecycle state does not match its reference.",
            ));
        }
        Ok(snapshot)
    }

    /// Deletes one exact sidecar after its replacement or successful consumption is durably referenced.
    /// 在替代快照或成功消费状态被可靠引用后删除一个精确 sidecar。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot_id` is the validated content-independent identity selected for deletion.
    /// `snapshot_id` 是为删除选定的已校验内容无关身份。
    ///
    /// # Returns
    /// 返回值
    /// Success when the file is absent or removed, otherwise an exact filesystem error.
    /// 文件不存在或已删除时返回成功，否则返回精确文件系统错误。
    pub fn delete(&self, snapshot_id: &str) -> Result<(), String> {
        let path = self.snapshot_path(snapshot_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Failed to delete execution retry snapshot `{}`: {error}",
                path.display()
            )),
        }
    }

    /// Resolves one validated snapshot identity below the session-owned retry directory.
    /// 在会话所属重试目录下解析一个已校验快照身份。
    fn snapshot_path(&self, snapshot_id: &str) -> Result<PathBuf, String> {
        validate_snapshot_id(snapshot_id)?;
        Ok(self
            .session_directory
            .join(EXECUTION_RETRY_DIRECTORY)
            .join(format!("{snapshot_id}.v1.json")))
    }
}

/// Validates strict snapshot schema and content-independent identity fields.
/// 校验严格快照 Schema 与内容无关身份字段。
fn validate_snapshot_identity(snapshot: &ExecutionRetrySnapshot) -> Result<(), String> {
    if snapshot.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported execution retry snapshot schema version `{}`; expected `{}`.",
            snapshot.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
        ));
    }
    validate_snapshot_id(snapshot.snapshot_id.as_str())?;
    if snapshot.session_id.trim().is_empty()
        || snapshot.logical_execution_id.trim().is_empty()
        || snapshot.physical_attempt_id.trim().is_empty()
        || snapshot.canonical_digest.trim().is_empty()
    {
        return Err(String::from(
            "Execution retry snapshot is missing a required stable identity or digest.",
        ));
    }
    Ok(())
}

/// Rejects path separators and non-UUID snapshot identities before filesystem resolution.
/// 在文件系统解析前拒绝路径分隔符与非 UUID 快照身份。
fn validate_snapshot_id(snapshot_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(snapshot_id)
        .map(|_| ())
        .map_err(|_| String::from("Execution retry snapshot id must be an exact UUID."))
}

/// Returns the stable lightweight label for one exact pending action.
/// 返回一个精确待执行动作的稳定轻量标签。
fn pending_action_label(action: &ExecutionRetryPendingAction) -> &'static str {
    match action {
        ExecutionRetryPendingAction::ResumePreparation { .. } => "resume_preparation",
        ExecutionRetryPendingAction::DispatchProviderRequest { .. } => "dispatch_provider_request",
        ExecutionRetryPendingAction::ExecuteToolBatch { .. } => "execute_tool_batch",
        ExecutionRetryPendingAction::PrepareNextProviderRequest => "prepare_next_provider_request",
        ExecutionRetryPendingAction::FinalizeAssistant => "finalize_assistant",
    }
}

/// Computes a lowercase SHA-256 digest for exact serialized bytes.
/// 为精确序列化字节计算小写 SHA-256 摘要。
fn sha256_hex(encoded: &[u8]) -> String {
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    //! Tests for strict atomic execution-retry sidecar storage.
    //! 严格原子执行重试 sidecar 存储测试。

    use super::*;
    use vulcan_workbench_model::{ExecutionRetrySnapshotState, SessionProviderRouteLeaseIdentity};

    /// Builds one complete minimal retry snapshot for storage contract tests.
    /// 为存储契约测试构建一份完整的最小重试快照。
    fn test_snapshot(snapshot_id: String) -> ExecutionRetrySnapshot {
        ExecutionRetrySnapshot {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id,
            session_id: uuid::Uuid::from_u128(42).to_string(),
            generation: 3,
            state: ExecutionRetrySnapshotState::Frozen,
            logical_execution_id: String::from("logical-execution-1"),
            logical_turn_id: Some(String::from("turn-1")),
            physical_attempt_id: String::from("attempt-1"),
            claimed_attempt_id: None,
            provider_route: SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("provider-a"),
                connection_id: String::from("connection-a"),
                model_id: String::from("model-a"),
                protocol_profile_id: String::from("openai-responses.v1"),
                manifest_revision: 1,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-a"),
                route_revision: String::from("route-a"),
            },
            provider_service_entry_id: Some(String::from("entry-a")),
            credential_profile_id: Some(String::from("credential-a")),
            controller_config: Some(vulcan_core::types::ControllerConfig::default()),
            controller_snapshot: vulcan_core::types::ConversationSnapshot::default(),
            pending_action: ExecutionRetryPendingAction::PrepareNextProviderRequest,
            error: None,
            canonical_digest: String::from("canonical-digest-a"),
            context_generation_id: Some(String::from("context-generation-a")),
            created_at: String::from("1000"),
            updated_at: String::from("1000"),
        }
    }

    /// Verifies atomic publication round-trips through a digest-bound lightweight reference.
    /// 验证原子发布能够通过摘要绑定轻量引用完整往返。
    #[test]
    fn retry_snapshot_round_trips_through_digest_reference() {
        // tempRoot owns the exact isolated session directory used by this test.
        // tempRoot 持有当前测试使用的精确隔离会话目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store binds every sidecar operation to the isolated session directory.
        // store 将每个 sidecar 操作绑定到隔离会话目录。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());

        let reference = store.write(&snapshot).expect("snapshot should persist");
        let restored = store.read(&reference).expect("snapshot should restore");

        assert_eq!(restored.snapshot_id, snapshot.snapshot_id);
        assert_eq!(restored.generation, snapshot.generation);
        assert_eq!(restored.canonical_digest, snapshot.canonical_digest);
        assert_eq!(reference.content_sha256.len(), 64);
        assert!(reference.content_bytes > 0);
    }

    /// Verifies changed sidecar bytes fail closed instead of being decoded as a retry candidate.
    /// 验证被修改的 sidecar 字节会失败关闭，而不是被解码为重试候选。
    #[test]
    fn retry_snapshot_rejects_digest_mismatch() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store.write(&snapshot).expect("snapshot should persist");
        // path is resolved by the same validated UUID rule used by normal reads.
        // path 使用与正常读取相同的已校验 UUID 规则解析。
        let path = store
            .snapshot_path(reference.snapshot_id.as_str())
            .expect("snapshot path should resolve");
        let mut bytes = fs::read(&path).expect("snapshot bytes should exist");
        bytes.push(b' ');
        fs::write(&path, bytes).expect("test corruption should be written");

        let error = store
            .read(&reference)
            .expect_err("changed bytes must fail closed");

        assert!(error.contains("byte length mismatch") || error.contains("digest mismatch"));
    }

    /// Verifies unknown schemas and path-like identities are rejected before publication.
    /// 验证未知 Schema 与路径式身份会在发布前被拒绝。
    #[test]
    fn retry_snapshot_rejects_unknown_schema_and_invalid_identity() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION + 1;
        assert!(store.write(&snapshot).is_err());

        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION;
        snapshot.snapshot_id = String::from("../escape");
        assert!(store.write(&snapshot).is_err());
    }
}

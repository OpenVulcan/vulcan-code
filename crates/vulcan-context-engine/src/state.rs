//! Versioned, storage-neutral snapshots for VCE process restart recovery.
//! VCE 进程重启恢复使用的版本化、存储中立快照。

use crate::embedding::EmbeddingSpace;
use crate::engine::{VceQueryStatistics, VceRefreshSummary, VceRetrievalSummary};
use crate::workspace::{WorkspaceSnapshot, WorkspaceState};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Current persisted-state Schema version understood by this crate.
/// 当前 crate 可理解的持久化状态 Schema 版本。
pub const VCE_STATE_SCHEMA_VERSION: u32 = 1;

/// Immutable namespace identity that prevents a snapshot from crossing index boundaries.
/// 防止快照跨越索引边界的不可变命名空间身份。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceStateIdentity {
    /// Host-owned code-semantic namespace identity.
    /// 宿主持有的代码语义命名空间身份。
    pub index_namespace_id: String,
    /// Checkpoint chain identity bound to the namespace.
    /// 绑定到命名空间的检查点链身份。
    pub checkpoint_chain_id: String,
    /// Host index Schema version required by the engine.
    /// 引擎要求的宿主索引 Schema 版本。
    pub index_schema_version: u32,
    /// Exact vector compatibility contract for persisted embeddings.
    /// 持久化向量使用的精确兼容性契约。
    pub embedding_space: EmbeddingSpace,
}

impl VceStateIdentity {
    /// Construct one persisted namespace identity from explicit host-neutral values.
    /// 根据显式且与宿主实现无关的值构造持久化命名空间身份。
    pub fn new(
        index_namespace_id: impl Into<String>,
        checkpoint_chain_id: impl Into<String>,
        index_schema_version: u32,
        embedding_space: EmbeddingSpace,
    ) -> Self {
        Self {
            index_namespace_id: index_namespace_id.into(),
            checkpoint_chain_id: checkpoint_chain_id.into(),
            index_schema_version,
            embedding_space,
        }
    }

    /// Validate the persisted identity without consulting a provider or storage implementation.
    /// 在不访问 Provider 或存储实现的情况下校验持久化身份。
    pub fn validate(&self) -> Result<(), VceStateError> {
        if self.index_namespace_id.trim().is_empty() {
            return Err(VceStateError::InvalidField("index_namespace_id"));
        }
        if self.checkpoint_chain_id.trim().is_empty() {
            return Err(VceStateError::InvalidField("checkpoint_chain_id"));
        }
        if self.index_schema_version == 0 {
            return Err(VceStateError::InvalidField("index_schema_version"));
        }
        if !self.embedding_space.is_well_formed() {
            return Err(VceStateError::InvalidField("embedding_space"));
        }
        Ok(())
    }
}

/// Complete VCE-owned state that an upper host may persist as JSON.
/// 上层宿主可按 JSON 持久化的完整 VCE 自有状态。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VceStateSnapshot {
    /// Version of this persisted state envelope.
    /// 此持久化状态封装的版本。
    pub schema_version: u32,
    /// Namespace and vector compatibility identity.
    /// 命名空间与向量兼容身份。
    pub identity: VceStateIdentity,
    /// Provider-independent workspace and checkpoint state.
    /// 与 Provider 无关的工作区和检查点状态。
    pub workspace: WorkspaceSnapshot,
    /// Engine-local operation counters.
    /// 引擎本地操作计数器。
    pub statistics: VceQueryStatistics,
    /// Last refresh summary retained for diagnostics.
    /// 为诊断保留的最近一次刷新摘要。
    pub last_refresh: Option<VceRefreshSummary>,
    /// Last retrieval summary retained for diagnostics.
    /// 为诊断保留的最近一次检索摘要。
    pub last_retrieval: Option<VceRetrievalSummary>,
    /// Last functional error retained for diagnostics.
    /// 为诊断保留的最近一次功能错误。
    pub last_error: Option<String>,
}

impl VceStateSnapshot {
    /// Construct a snapshot using the current persisted-state Schema version.
    /// 使用当前持久化状态 Schema 版本构造快照。
    pub fn new(
        identity: VceStateIdentity,
        workspace: WorkspaceSnapshot,
        statistics: VceQueryStatistics,
        last_refresh: Option<VceRefreshSummary>,
        last_retrieval: Option<VceRetrievalSummary>,
        last_error: Option<String>,
    ) -> Self {
        Self {
            schema_version: VCE_STATE_SCHEMA_VERSION,
            identity,
            workspace,
            statistics,
            last_refresh,
            last_retrieval,
            last_error,
        }
    }

    /// Validate the envelope before restoring its workspace state.
    /// 在恢复工作区状态前校验快照封装。
    pub fn validate(&self) -> Result<(), VceStateError> {
        if self.schema_version != VCE_STATE_SCHEMA_VERSION {
            return Err(VceStateError::UnsupportedSchemaVersion {
                expected: VCE_STATE_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        self.identity.validate()?;
        if let Some(summary) = &self.last_refresh {
            validate_refresh_summary(summary)?;
        }
        if let Some(summary) = &self.last_retrieval {
            validate_retrieval_summary(summary)?;
        }
        if self
            .last_error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty())
        {
            return Err(VceStateError::InvalidField("last_error"));
        }
        if self.statistics.refresh_successes > self.statistics.refresh_attempts
            || self.statistics.retrieval_successes > self.statistics.retrieval_attempts
            || self.last_refresh.is_some() != (self.statistics.refresh_successes > 0)
            || self.last_retrieval.is_some() != (self.statistics.retrieval_successes > 0)
            || (self.statistics.retrieval_successes == 0
                && (self.statistics.dense_hits > 0
                    || self.statistics.path_hits > 0
                    || self.statistics.exact_hits > 0
                    || self.statistics.selected_hits > 0))
            || (self.last_error.is_some()
                && self.statistics.refresh_attempts == self.statistics.refresh_successes
                && self.statistics.retrieval_attempts == self.statistics.retrieval_successes)
        {
            return Err(VceStateError::InvalidField("statistics"));
        }
        let workspace = WorkspaceState::from_snapshot(self.workspace.clone())
            .map_err(|error| VceStateError::Workspace(error.to_string()))?;
        let checkpoint = workspace.checkpoint.as_ref();
        if checkpoint.is_some_and(|value| value.chain_id != self.identity.checkpoint_chain_id) {
            return Err(VceStateError::InvalidField("identity"));
        }
        if let Some(summary) = &self.last_refresh
            && (summary.generation > workspace.generation
                || summary.checkpoint_version != checkpoint.map(|value| value.version))
        {
            return Err(VceStateError::InvalidField("last_refresh"));
        }
        if let Some(summary) = &self.last_retrieval
            && (self.statistics.dense_hits < summary.dense_hit_count as u64
                || self.statistics.path_hits < summary.path_hit_count as u64
                || self.statistics.exact_hits < summary.exact_hit_count as u64
                || self.statistics.selected_hits < summary.selected_hit_count as u64)
        {
            return Err(VceStateError::InvalidField("statistics"));
        }
        Ok(())
    }

    /// Serialize this versioned snapshot to human-readable JSON.
    /// 将此版本化快照序列化为便于阅读的 JSON。
    pub fn to_json(&self) -> Result<String, VceStateError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| VceStateError::Json(error.to_string()))
    }

    /// Deserialize and validate one versioned JSON snapshot.
    /// 反序列化并校验一份版本化 JSON 快照。
    pub fn from_json(json: &str) -> Result<Self, VceStateError> {
        let snapshot = serde_json::from_str::<Self>(json)
            .map_err(|error| VceStateError::Json(error.to_string()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }
}

/// Errors raised while decoding or validating persisted VCE state.
/// 解码或校验 VCE 持久化状态时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VceStateError {
    /// JSON encoding or decoding failed.
    /// JSON 编码或解码失败。
    Json(String),
    /// The envelope uses an unsupported Schema version.
    /// 快照封装使用了不受支持的 Schema 版本。
    UnsupportedSchemaVersion {
        /// Version supported by this crate.
        /// 此 crate 支持的版本。
        expected: u32,
        /// Version found in persisted state.
        /// 持久化状态中的版本。
        actual: u32,
    },
    /// One required persisted field is invalid.
    /// 一个必需的持久化字段无效。
    InvalidField(&'static str),
    /// The embedded workspace snapshot violates its state-machine invariants.
    /// 内嵌工作区快照违反其状态机不变量。
    Workspace(String),
}

impl Display for VceStateError {
    /// Format the state error for host diagnostics.
    /// 为宿主诊断格式化状态错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(reason) => write!(formatter, "state JSON failed: {reason}"),
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "state Schema version mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidField(field) => write!(formatter, "state field `{field}` is invalid"),
            Self::Workspace(reason) => write!(formatter, "workspace state is invalid: {reason}"),
        }
    }
}

impl Error for VceStateError {}

/// Validate a persisted refresh summary before exposing it through restored diagnostics.
/// 在通过恢复后的诊断暴露刷新摘要前校验它。
fn validate_refresh_summary(summary: &VceRefreshSummary) -> Result<(), VceStateError> {
    if summary.request_id.trim().is_empty()
        || summary.generation == 0
        || summary
            .checkpoint_version
            .is_none_or(|version| version == 0)
        || summary
            .source_revision
            .as_ref()
            .is_some_and(|revision| revision.trim().is_empty())
        || summary.indexed_blob_count > summary.added_blob_count
    {
        return Err(VceStateError::InvalidField("last_refresh"));
    }
    Ok(())
}

/// Validate persisted retrieval counts and degradation diagnostics.
/// 校验持久化检索计数与降级诊断。
fn validate_retrieval_summary(summary: &VceRetrievalSummary) -> Result<(), VceStateError> {
    let unique_features = summary
        .degraded_features
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let degradation_features = summary
        .degradations
        .iter()
        .map(|degradation| degradation.feature)
        .collect::<BTreeSet<_>>();
    if summary.request_id.trim().is_empty()
        || summary.query_count == 0
        || summary.selected_hit_count > summary.candidate_count
        || unique_features.len() != summary.degraded_features.len()
        || unique_features != degradation_features
        || summary.degradations.iter().any(|degradation| {
            degradation.operation_id.trim().is_empty()
                || degradation.reason.trim().is_empty()
                || !unique_features.contains(&degradation.feature)
        })
    {
        return Err(VceStateError::InvalidField("last_retrieval"));
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Build a valid empty persisted snapshot for serialization tests.
    /// 为序列化测试构造有效的空持久化快照。
    fn snapshot() -> VceStateSnapshot {
        VceStateSnapshot::new(
            VceStateIdentity::new(
                "vce:test",
                "test",
                1,
                EmbeddingSpace {
                    vector_space_id: "test-space".to_owned(),
                    physical_fingerprint: Some("model-v1".to_owned()),
                    dimension: 3,
                    preprocessing_version: "pre-v1".to_owned(),
                    chunker_version: "chunk-v1".to_owned(),
                },
            ),
            WorkspaceSnapshot {
                generation: 0,
                checkpoint: None,
                records: Vec::new(),
            },
            VceQueryStatistics::default(),
            None,
            None,
            None,
        )
    }

    #[test]
    fn versioned_state_round_trips_through_json() {
        let expected = snapshot();
        let json = expected.to_json().unwrap();
        let actual = VceStateSnapshot::from_json(&json).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn unsupported_state_version_is_rejected() {
        let mut state = snapshot();
        state.schema_version = VCE_STATE_SCHEMA_VERSION + 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::UnsupportedSchemaVersion {
                expected: VCE_STATE_SCHEMA_VERSION,
                actual: VCE_STATE_SCHEMA_VERSION + 1,
            })
        );
    }

    /// Verify persisted compatibility identities reject a present but empty host fingerprint.
    /// 验证持久化兼容身份会拒绝已提供但为空的宿主指纹。
    #[test]
    fn state_identity_rejects_blank_physical_fingerprint() {
        let mut state = snapshot();
        state.identity.embedding_space.physical_fingerprint = Some("   ".to_owned());

        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("embedding_space"))
        );
    }

    /// Verify JSON-level validation rejects an invalid embedded workspace before engine restore.
    /// 验证 JSON 层校验会在引擎恢复前拒绝非法内嵌工作区。
    #[test]
    fn state_validation_rejects_invalid_embedded_workspace() {
        let mut state = snapshot();
        let mut workspace = WorkspaceState::default();
        workspace.observe("src/lib.rs", "fn pending() {}").unwrap();
        state.workspace = workspace.snapshot();
        state.workspace.records[0].blob_name = None;

        assert!(matches!(state.validate(), Err(VceStateError::Workspace(_))));
    }

    /// Verify restored counters and diagnostics cannot contradict their public summaries.
    /// 验证恢复后的计数与诊断不能违背其公开摘要。
    #[test]
    fn state_validation_rejects_inconsistent_diagnostics() {
        let mut state = snapshot();
        state.statistics.refresh_successes = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );

        let mut state = snapshot();
        state.last_error = Some("   ".to_owned());
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("last_error"))
        );

        let mut state = snapshot();
        state.statistics.refresh_attempts = 1;
        state.statistics.refresh_successes = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );

        let mut state = snapshot();
        state.statistics.retrieval_attempts = 1;
        state.statistics.retrieval_successes = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );

        let mut state = snapshot();
        state.statistics.selected_hits = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );

        let mut state = snapshot();
        state.last_error = Some("stale failure".to_owned());
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );
    }

    /// Verify successful operation summaries cannot be restored without their engine invariants.
    /// 验证成功操作摘要不能在缺少引擎不变量时被恢复。
    #[test]
    fn state_validation_rejects_impossible_operation_summaries() {
        let mut state = snapshot();
        state.last_refresh = Some(VceRefreshSummary {
            request_id: "refresh-1".to_owned(),
            mode: crate::ports::RefreshMode::Full,
            reason: crate::ports::RefreshReason::Initial,
            generation: 0,
            added_blob_count: 0,
            deleted_blob_count: 0,
            indexed_blob_count: 0,
            skipped_path_count: 0,
            checkpoint_version: None,
            source_revision: None,
        });
        state.statistics.refresh_attempts = 1;
        state.statistics.refresh_successes = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("last_refresh"))
        );

        let mut state = snapshot();
        state.last_retrieval = Some(VceRetrievalSummary {
            request_id: "retrieve-1".to_owned(),
            intent: crate::query::QueryIntent::Feature,
            query_count: 1,
            dense_hit_count: 0,
            path_hit_count: 0,
            exact_hit_count: 0,
            candidate_count: 0,
            selected_hit_count: 0,
            degraded_features: vec![crate::engine::VceDegradedFeature::Rerank],
            degradations: Vec::new(),
        });
        state.statistics.retrieval_attempts = 1;
        state.statistics.retrieval_successes = 1;
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("last_retrieval"))
        );

        let mut state = snapshot();
        let mut workspace = WorkspaceState::default();
        workspace.reconcile(Vec::new()).unwrap();
        workspace
            .commit_checkpoint(crate::workspace::Checkpoint::new("test", 1, Vec::new()))
            .unwrap();
        state.workspace = workspace.snapshot();
        state.last_refresh = Some(VceRefreshSummary {
            request_id: "refresh-2".to_owned(),
            mode: crate::ports::RefreshMode::Paths,
            reason: crate::ports::RefreshReason::Manual,
            generation: 1,
            added_blob_count: 0,
            deleted_blob_count: 0,
            indexed_blob_count: 0,
            skipped_path_count: 0,
            checkpoint_version: Some(1),
            source_revision: None,
        });
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("statistics"))
        );

        let mut state = snapshot();
        let mut workspace = WorkspaceState::default();
        workspace.reconcile(Vec::new()).unwrap();
        workspace
            .commit_checkpoint(crate::workspace::Checkpoint::new(
                "another-chain",
                1,
                Vec::new(),
            ))
            .unwrap();
        state.workspace = workspace.snapshot();
        assert_eq!(
            state.validate(),
            Err(VceStateError::InvalidField("identity"))
        );
    }
}

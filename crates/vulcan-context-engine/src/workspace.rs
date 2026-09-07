//! Pure workspace snapshot, overlay, checkpoint, and sync-plan state.
//! 纯工作区快照、覆盖层、检查点和同步计划状态。

use crate::identity::{PathError, blob_name, is_sha256_identity, normalize_relative_path};
use crate::model::{FileRecord, FileSource, FileStatus};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A single observed workspace path supplied by a filesystem or explicit overlay.
/// 由文件系统或显式覆盖层提供的一次工作区路径观察结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileObservation {
    /// Workspace-relative path being observed.
    /// 被观察的工作区相对路径。
    pub path: String,
    /// Verbatim text when the observation is a present text file.
    /// 当观察对象是存在的文本文件时保存其原始文本。
    pub content: Option<String>,
    /// Optional precomputed Blob identity for metadata-only observations.
    /// 元数据观察结果可选的预计算 Blob 身份。
    pub blob_name: Option<String>,
    /// File size in bytes at observation time.
    /// 观察时的文件字节大小。
    pub size: u64,
    /// File modification timestamp in nanoseconds, when available.
    /// 文件修改时间纳秒值（如果可用）。
    pub mtime_ns: Option<u128>,
    /// Source that produced this observation.
    /// 产生该观察结果的来源。
    pub source: FileSource,
    /// Admission state represented by this observation.
    /// 该观察结果表示的准入状态。
    pub status: FileStatus,
    /// Explicit skip reason when status is Skipped.
    /// 状态为跳过时记录明确的跳过原因。
    pub skip_reason: Option<String>,
}

impl FileObservation {
    /// Build a present text-file observation from its exact content.
    /// 根据文件原始内容构建存在的文本文件观察结果。
    pub fn present(
        path: impl Into<String>,
        content: impl Into<String>,
        mtime_ns: Option<u128>,
        source: FileSource,
    ) -> Self {
        let content = content.into();
        Self {
            path: path.into(),
            size: content.len() as u64,
            content: Some(content),
            blob_name: None,
            mtime_ns,
            source,
            status: FileStatus::Present,
            skip_reason: None,
        }
    }

    /// Build a skipped-file observation without reading or indexing its content.
    /// 构建跳过文件观察结果，不读取或索引其内容。
    pub fn skipped(
        path: impl Into<String>,
        size: u64,
        mtime_ns: Option<u128>,
        reason: impl Into<String>,
        source: FileSource,
    ) -> Self {
        Self {
            path: path.into(),
            content: None,
            blob_name: None,
            size,
            mtime_ns,
            source,
            status: FileStatus::Skipped,
            skip_reason: Some(reason.into()),
        }
    }

    /// Build a deletion observation for a path no longer present in the workspace.
    /// 为工作区中已不存在的路径构建删除观察结果。
    pub fn deleted(path: impl Into<String>, source: FileSource) -> Self {
        Self {
            path: path.into(),
            content: None,
            blob_name: None,
            size: 0,
            mtime_ns: None,
            source,
            status: FileStatus::Deleted,
            skip_reason: None,
        }
    }

    /// Build a metadata-only present observation with a host-precomputed Blob identity.
    /// 使用宿主预计算的 Blob 身份构建仅包含元数据的存在观察结果。
    pub fn identified(
        path: impl Into<String>,
        blob_name: impl Into<String>,
        size: u64,
        mtime_ns: Option<u128>,
        source: FileSource,
    ) -> Self {
        Self {
            path: path.into(),
            content: None,
            blob_name: Some(blob_name.into()),
            size,
            mtime_ns,
            source,
            status: FileStatus::Present,
            skip_reason: None,
        }
    }
}

/// A versioned set of Blob identities that has been committed to the index.
/// 已提交到索引的 Blob 身份版本集合。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    /// Stable chain identity.
    /// 稳定的链身份。
    pub chain_id: String,
    /// Monotonically increasing chain version.
    /// 单调递增的链版本。
    pub version: u64,
    /// Blob members visible at this checkpoint.
    /// 该检查点可见的 Blob 成员。
    pub members: BTreeSet<String>,
}

impl Checkpoint {
    /// Build a checkpoint with a deterministic member set.
    /// 使用确定性的成员集合构建检查点。
    pub fn new(
        chain_id: impl Into<String>,
        version: u64,
        members: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            version,
            members: members.into_iter().collect(),
        }
    }
}

/// A pure synchronization diff between the current snapshot and its checkpoint.
/// 当前快照与检查点之间的纯同步差异。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    /// Snapshot generation that produced this plan.
    /// 生成该计划的快照代数。
    pub generation: u64,
    /// Current Blob identities not present in the checkpoint.
    /// 当前存在但不在检查点中的 Blob 身份。
    pub added_blobs: BTreeSet<String>,
    /// Checkpoint Blob identities no longer present in the current snapshot.
    /// 检查点中存在但当前快照中已不存在的 Blob 身份。
    pub deleted_blobs: BTreeSet<String>,
    /// Paths skipped by admission rules.
    /// 被准入规则跳过的路径。
    pub skipped_paths: BTreeSet<String>,
}

/// Resolve a safe retrieval scope without silently widening it to the whole repository.
/// 解析安全的检索范围，不静默扩大为整个仓库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeResolution {
    /// No checkpoint or added Blob declaration exists.
    /// 没有检查点且没有新增 Blob 声明。
    MissingCheckpoint,
    /// A valid scope was declared but contains no searchable Blob.
    /// 已声明有效范围，但其中没有可搜索 Blob。
    Empty,
    /// The exact Blob identities allowed for retrieval.
    /// 允许检索的精确 Blob 身份集合。
    Members(BTreeSet<String>),
}

/// A serializable view of the current pure workspace state.
/// 当前纯工作区状态的可复制视图。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceSnapshot {
    /// Monotonically increasing local snapshot generation.
    /// 单调递增的本地快照代数。
    pub generation: u64,
    /// Current committed checkpoint, if one exists.
    /// 当前已提交的检查点（如果存在）。
    pub checkpoint: Option<Checkpoint>,
    /// File records in canonical path order.
    /// 按规范路径顺序排列的文件记录。
    pub records: Vec<FileRecord>,
}

/// Errors raised by the pure workspace state machine.
/// 纯工作区状态机产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// The supplied path is not a valid workspace-relative path.
    /// 提供的路径不是有效的工作区相对路径。
    InvalidPath(PathError),
    /// A present observation did not provide source text.
    /// 存在的文件观察结果没有提供源码文本。
    MissingContent {
        /// Canonical path whose present observation omitted source text.
        /// 存在状态观察中缺少源码文本的规范路径。
        path: String,
    },
    /// A partial refresh mentioned the same path more than once.
    /// 部分刷新在同一批次中重复提及了同一个路径。
    DuplicatePath {
        /// Canonical path repeated in one partial refresh.
        /// 在单次部分刷新中重复出现的规范路径。
        path: String,
    },
    /// The restored workspace snapshot violates a persisted-state invariant.
    /// 恢复的工作区快照违反了持久化状态不变量。
    InvalidSnapshot {
        /// Explicit persisted-state invariant violation.
        /// 明确的持久化状态不变量违反原因。
        reason: String,
    },
    /// A checkpoint commit does not match the current workspace or checkpoint chain.
    /// 检查点提交与当前工作区或检查点链不匹配。
    InvalidCheckpoint {
        /// Explicit checkpoint transition invariant violation.
        /// 明确的检查点转换不变量违反原因。
        reason: String,
    },
    /// The workspace generation cannot advance without wrapping.
    /// 工作区代数无法在不回绕的情况下继续推进。
    GenerationExhausted,
}

impl Display for WorkspaceError {
    /// Formats a workspace state error for deterministic diagnostics.
    /// 将工作区状态错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid workspace path: {error}"),
            Self::MissingContent { path } => {
                write!(
                    formatter,
                    "present workspace observation has no content: {path}"
                )
            }
            Self::DuplicatePath { path } => {
                write!(
                    formatter,
                    "workspace refresh mentions path more than once: {path}"
                )
            }
            Self::InvalidSnapshot { reason } => {
                write!(formatter, "invalid workspace snapshot: {reason}")
            }
            Self::InvalidCheckpoint { reason } => {
                write!(formatter, "invalid workspace checkpoint: {reason}")
            }
            Self::GenerationExhausted => formatter.write_str("workspace generation is exhausted"),
        }
    }
}

impl Error for WorkspaceError {}

/// Own the provider-independent workspace records and checkpoint state.
/// 拥有不依赖供应商的工作区记录和检查点状态。
#[derive(Debug, Clone, Default)]
pub struct WorkspaceState {
    /// Current local snapshot generation.
    /// 当前本地快照代数。
    pub(crate) generation: u64,
    /// Canonical path-to-record map.
    /// 规范路径到文件记录的映射。
    pub(crate) records: BTreeMap<String, FileRecord>,
    /// Last committed index checkpoint.
    /// 最近一次提交的索引检查点。
    pub(crate) checkpoint: Option<Checkpoint>,
}

impl WorkspaceState {
    /// Return the current local workspace generation.
    /// 返回当前本地工作区代次。
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Return the current records keyed by canonical workspace-relative path.
    /// 返回按规范工作区相对路径索引的当前记录。
    pub fn records(&self) -> &BTreeMap<String, FileRecord> {
        &self.records
    }

    /// Return the last committed checkpoint when one exists.
    /// 返回最近一次已提交检查点（如果存在）。
    pub fn checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoint.as_ref()
    }

    /// Reconcile a complete set of observations into one new snapshot generation.
    /// 将完整观察结果集合协调为一个新的快照代数。
    pub fn reconcile(
        &mut self,
        observations: impl IntoIterator<Item = FileObservation>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let next_generation = self.next_generation()?;
        let observations = observations.into_iter().collect::<Vec<_>>();
        let mut seen_paths = BTreeSet::new();
        let mut next_records = BTreeMap::new();

        for observation in observations {
            let path =
                normalize_relative_path(&observation.path).map_err(WorkspaceError::InvalidPath)?;
            if !seen_paths.insert(path.clone()) {
                return Err(WorkspaceError::DuplicatePath { path });
            }
            let previous = self.records.get(&path);
            let record = if let Some(previous) = previous.filter(|record| {
                record.source == FileSource::Explicit
                    && record.status == FileStatus::Present
                    && observation.source == FileSource::Filesystem
            }) {
                if filesystem_observation_matches_explicit(&path, &observation, previous)? {
                    Self::record_from_observation(
                        path.clone(),
                        observation,
                        Some(previous),
                        next_generation,
                    )?
                } else {
                    let mut record = previous.clone();
                    record.generation = next_generation;
                    record
                }
            } else {
                Self::record_from_observation(path.clone(), observation, previous, next_generation)?
            };
            next_records.insert(path, record);
        }

        for (path, previous) in &self.records {
            if !seen_paths.contains(path) {
                // A full filesystem scan cannot observe a new unsaved editor file, so only an
                // explicit host removal may discard this authoritative overlay.
                // 全量文件系统扫描无法观察新建的未保存编辑器文件，因此只有宿主显式删除才能丢弃该权威覆盖层。
                if previous.source == FileSource::Explicit && previous.status == FileStatus::Present
                {
                    let mut retained = previous.clone();
                    retained.generation = next_generation;
                    next_records.insert(path.clone(), retained);
                    continue;
                }
                // Use the same deletion constructor as explicit observations to clear size and mtime.
                // 使用与明确观察相同的删除构造入口，清除大小和修改时间。
                // Preserve committed identity until publication so the old vectors are still retired exactly.
                // 在发布前保留已提交身份，确保仍能精确退役旧向量。
                let deleted = Self::record_from_observation(
                    path.clone(),
                    FileObservation::deleted(path.clone(), FileSource::Filesystem),
                    Some(previous),
                    next_generation,
                )?;
                next_records.insert(path.clone(), deleted);
            }
        }

        validate_blob_ownership(&next_records)?;

        self.generation = next_generation;
        self.records = next_records;
        Ok(self.snapshot())
    }

    /// Reconcile only changed paths while retaining untouched workspace records.
    /// 仅协调发生变化的路径，同时保留未触碰的工作区记录。
    ///
    /// This is the state transition used by a host-owned file or Git watcher after it has
    /// rescanned the affected paths. A filesystem observation replaces a present explicit overlay
    /// only after the host proves that both represent the same Blob and byte size.
    /// 这是宿主文件或 Git 监听器重新扫描受影响路径后的状态转换。只有宿主证明文件系统观察与显式覆盖层
    /// 表示同一 Blob 且字节大小一致时，文件系统观察才会替换该覆盖层。
    pub fn reconcile_paths(
        &mut self,
        observations: impl IntoIterator<Item = FileObservation>,
        removed_paths: impl IntoIterator<Item = String>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let next_generation = self.next_generation()?;
        let mut touched_paths = BTreeSet::new();
        let mut next_records = self.records.clone();

        for observation in observations {
            let path =
                normalize_relative_path(&observation.path).map_err(WorkspaceError::InvalidPath)?;
            if !touched_paths.insert(path.clone()) {
                return Err(WorkspaceError::DuplicatePath { path });
            }
            let previous = self.records.get(&path);
            let record = if let Some(previous) = previous.filter(|record| {
                record.source == FileSource::Explicit
                    && record.status == FileStatus::Present
                    && observation.source == FileSource::Filesystem
            }) {
                if filesystem_observation_matches_explicit(&path, &observation, previous)? {
                    Self::record_from_observation(
                        path.clone(),
                        observation,
                        Some(previous),
                        next_generation,
                    )?
                } else {
                    let mut record = previous.clone();
                    record.generation = next_generation;
                    record
                }
            } else {
                Self::record_from_observation(path.clone(), observation, previous, next_generation)?
            };
            next_records.insert(path, record);
        }

        for removed_path in removed_paths {
            let path =
                normalize_relative_path(&removed_path).map_err(WorkspaceError::InvalidPath)?;
            if !touched_paths.insert(path.clone()) {
                return Err(WorkspaceError::DuplicatePath { path });
            }
            let previous = self.records.get(&path);
            if let Some(previous) = previous.filter(|record| {
                record.source == FileSource::Explicit && record.status == FileStatus::Present
            }) {
                let mut record = previous.clone();
                record.generation = next_generation;
                next_records.insert(path, record);
                continue;
            }

            let committed_blob_name =
                previous.and_then(|record| record.committed_blob_name.clone());
            next_records.insert(
                path.clone(),
                FileRecord {
                    path,
                    blob_name: None,
                    committed_blob_name,
                    status: FileStatus::Deleted,
                    size: 0,
                    mtime_ns: None,
                    source: FileSource::Filesystem,
                    content: None,
                    generation: next_generation,
                    skip_reason: None,
                },
            );
        }

        validate_blob_ownership(&next_records)?;

        self.generation = next_generation;
        self.records = next_records;
        Ok(self.snapshot())
    }

    /// Restore the pure workspace state from a previously captured snapshot.
    /// 从之前捕获的快照恢复纯工作区状态。
    pub fn from_snapshot(snapshot: WorkspaceSnapshot) -> Result<Self, WorkspaceError> {
        let mut records = BTreeMap::new();
        for record in snapshot.records {
            let normalized = normalize_relative_path(&record.path).map_err(|error| {
                WorkspaceError::InvalidSnapshot {
                    reason: format!("record path `{}` is invalid: {error}", record.path),
                }
            })?;
            if normalized != record.path {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("record path `{}` is not canonical", record.path),
                });
            }
            if record.generation > snapshot.generation {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "record `{}` generation {} exceeds snapshot generation {}",
                        record.path, record.generation, snapshot.generation
                    ),
                });
            }
            validate_snapshot_record(&record)?;
            if records.insert(record.path.clone(), record).is_some() {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("record path `{normalized}` appears more than once"),
                });
            }
        }

        validate_blob_ownership(&records)?;
        validate_snapshot_checkpoint(snapshot.checkpoint.as_ref(), &records, snapshot.generation)?;
        Ok(Self {
            generation: snapshot.generation,
            records,
            checkpoint: snapshot.checkpoint,
        })
    }

    /// Add or replace an explicit unsaved text overlay without scanning other paths.
    /// 添加或替换显式未保存文本覆盖层，不扫描其他路径。
    pub fn observe(
        &mut self,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<(), WorkspaceError> {
        let observation = FileObservation::present(path, content, None, FileSource::Explicit);
        let normalized_path =
            normalize_relative_path(&observation.path).map_err(WorkspaceError::InvalidPath)?;
        let next_generation = self.next_generation()?;
        let previous = self.records.get(&normalized_path);
        let record = Self::record_from_observation(
            normalized_path.clone(),
            observation,
            previous,
            next_generation,
        )?;
        self.generation = next_generation;
        self.records.insert(normalized_path, record);
        Ok(())
    }

    /// Mark one workspace-relative path as explicitly removed.
    /// 将一个工作区相对路径标记为显式删除。
    pub fn remove(&mut self, path: impl Into<String>) -> Result<(), WorkspaceError> {
        let normalized_path =
            normalize_relative_path(&path.into()).map_err(WorkspaceError::InvalidPath)?;
        let next_generation = self.next_generation()?;
        let previous = self.records.get(&normalized_path);
        self.generation = next_generation;
        self.records.insert(
            normalized_path.clone(),
            FileRecord {
                path: normalized_path,
                blob_name: None,
                committed_blob_name: previous.and_then(|record| record.committed_blob_name.clone()),
                status: FileStatus::Deleted,
                size: 0,
                mtime_ns: None,
                source: FileSource::Explicit,
                content: None,
                generation: next_generation,
                skip_reason: None,
            },
        );
        Ok(())
    }

    /// Commit a checkpoint and update each present record's committed Blob identity.
    /// 提交检查点并更新每个存在文件记录的已提交 Blob 身份。
    pub fn commit_checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), WorkspaceError> {
        validate_checkpoint_transition(
            self.checkpoint.as_ref(),
            &checkpoint,
            &self.records,
            self.generation,
        )?;
        for record in self.records.values_mut() {
            record.committed_blob_name = record
                .blob_name
                .as_ref()
                .filter(|blob| checkpoint.members.contains(*blob))
                .cloned();
        }
        self.checkpoint = Some(checkpoint);
        Ok(())
    }

    /// Release committed filesystem text while retaining explicit unsaved overlays.
    /// 释放已提交的文件系统文本，同时保留显式未保存覆盖内容。
    pub fn release_committed_filesystem_content(&mut self) {
        for record in self.records.values_mut() {
            if record.source == FileSource::Filesystem
                && record.committed_blob_name.is_some()
                && record.committed_blob_name == record.blob_name
            {
                record.content = None;
            }
        }
    }

    /// Create a deterministic added/deleted synchronization plan.
    /// 创建确定性的新增与删除同步计划。
    pub fn sync_plan(&self) -> SyncPlan {
        let current = self
            .records
            .values()
            .filter_map(|record| record.blob_name.clone().filter(|_| record.is_present()))
            .collect::<BTreeSet<_>>();
        let committed = self
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.members.clone())
            .unwrap_or_default();
        let added_blobs = current.difference(&committed).cloned().collect();
        let deleted_blobs = committed.difference(&current).cloned().collect();
        let skipped_paths = self
            .records
            .values()
            .filter(|record| record.status == FileStatus::Skipped)
            .map(|record| record.path.clone())
            .collect();
        SyncPlan {
            generation: self.generation,
            added_blobs,
            deleted_blobs,
            skipped_paths,
        }
    }

    /// Resolve a retrieval scope from checkpoint and explicit delta declarations.
    /// 根据检查点和显式增量声明解析检索范围。
    pub fn resolve_scope(
        &self,
        added_blobs: impl IntoIterator<Item = String>,
        deleted_blobs: impl IntoIterator<Item = String>,
    ) -> ScopeResolution {
        let added = added_blobs.into_iter().collect::<BTreeSet<_>>();
        let deleted = deleted_blobs.into_iter().collect::<BTreeSet<_>>();
        let Some(checkpoint) = &self.checkpoint else {
            return if added.is_empty() {
                ScopeResolution::MissingCheckpoint
            } else {
                let members = added.difference(&deleted).cloned().collect::<BTreeSet<_>>();
                if members.is_empty() {
                    ScopeResolution::Empty
                } else {
                    ScopeResolution::Members(members)
                }
            };
        };

        let mut members = checkpoint.members.clone();
        members.extend(added);
        for blob in deleted {
            members.remove(&blob);
        }
        if members.is_empty() {
            ScopeResolution::Empty
        } else {
            ScopeResolution::Members(members)
        }
    }

    /// Return a stable snapshot copy for callers and tests.
    /// 返回供调用方和测试使用的稳定快照副本。
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            generation: self.generation,
            checkpoint: self.checkpoint.clone(),
            records: self.records.values().cloned().collect(),
        }
    }

    /// Convert one normalized observation into a retained FileRecord.
    /// 将一个规范化观察结果转换为保留的文件记录。
    fn record_from_observation(
        path: String,
        observation: FileObservation,
        previous: Option<&FileRecord>,
        generation: u64,
    ) -> Result<FileRecord, WorkspaceError> {
        validate_observation(&path, &observation)?;
        let blob_name = match observation.status {
            FileStatus::Present => match observation.content.as_deref() {
                Some(content) => {
                    let actual = blob_name(&path, content).map_err(WorkspaceError::InvalidPath)?;
                    if observation
                        .blob_name
                        .as_ref()
                        .is_some_and(|expected| expected != &actual)
                    {
                        return Err(WorkspaceError::InvalidSnapshot {
                            reason: format!(
                                "observation `{path}` content does not match its Blob identity"
                            ),
                        });
                    }
                    Some(actual)
                }
                None => observation
                    .blob_name
                    .clone()
                    .or_else(|| reusable_blob(previous, &observation))
                    .ok_or_else(|| WorkspaceError::MissingContent { path: path.clone() })
                    .map(Some)?,
            },
            FileStatus::Skipped | FileStatus::Deleted => None,
        };
        if blob_name
            .as_deref()
            .is_some_and(|identity| !is_sha256_identity(identity))
        {
            return Err(WorkspaceError::InvalidSnapshot {
                reason: format!("observation `{path}` has a noncanonical Blob identity"),
            });
        }
        Ok(FileRecord {
            path,
            blob_name,
            committed_blob_name: previous.and_then(|record| record.committed_blob_name.clone()),
            status: observation.status,
            size: observation.size,
            mtime_ns: observation.mtime_ns,
            source: observation.source,
            content: observation.content,
            generation,
            skip_reason: observation.skip_reason,
        })
    }

    /// Compute the next generation without allowing silent numeric saturation.
    /// 计算下一代状态，并禁止数值静默饱和。
    fn next_generation(&self) -> Result<u64, WorkspaceError> {
        self.generation
            .checked_add(1)
            .ok_or(WorkspaceError::GenerationExhausted)
    }
}

/// Validate one publicly constructible observation before it changes workspace state.
/// 在公开可构造的观察结果改变工作区状态前校验它。
fn validate_observation(path: &str, observation: &FileObservation) -> Result<(), WorkspaceError> {
    match observation.status {
        FileStatus::Present => {
            if observation.skip_reason.is_some()
                || (observation.source == FileSource::Explicit && observation.content.is_none())
                || observation
                    .content
                    .as_ref()
                    .is_some_and(|content| content.len() as u64 != observation.size)
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("present observation `{path}` has inconsistent metadata"),
                });
            }
        }
        FileStatus::Skipped => {
            if observation.content.is_some()
                || observation.blob_name.is_some()
                || observation
                    .skip_reason
                    .as_ref()
                    .is_none_or(|reason| reason.trim().is_empty())
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("skipped observation `{path}` violates skip invariants"),
                });
            }
        }
        FileStatus::Deleted => {
            if observation.content.is_some()
                || observation.blob_name.is_some()
                || observation.skip_reason.is_some()
                || observation.size != 0
                || observation.mtime_ns.is_some()
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("deleted observation `{path}` retains source metadata"),
                });
            }
        }
    }
    Ok(())
}

/// Reuse a previous Blob identity only when all available filesystem metadata is unchanged.
/// 仅在全部可用文件系统元数据未变化时复用先前 Blob 身份。
fn reusable_blob(previous: Option<&FileRecord>, observation: &FileObservation) -> Option<String> {
    let previous = previous?;
    if previous.status != FileStatus::Present
        || previous.source != observation.source
        || previous.size != observation.size
        || observation.mtime_ns.is_none()
        || previous.mtime_ns != observation.mtime_ns
    {
        return None;
    }
    previous.blob_name.clone()
}

/// Decide whether a saved filesystem version exactly replaces one explicit editor overlay.
/// 判断已保存的文件系统版本是否精确替代一个显式编辑器覆盖层。
fn filesystem_observation_matches_explicit(
    path: &str,
    observation: &FileObservation,
    explicit: &FileRecord,
) -> Result<bool, WorkspaceError> {
    validate_observation(path, observation)?;
    if observation.status != FileStatus::Present {
        return Ok(false);
    }
    let observed_blob = match observation.content.as_deref() {
        Some(content) => {
            let actual = blob_name(path, content).map_err(WorkspaceError::InvalidPath)?;
            if observation
                .blob_name
                .as_ref()
                .is_some_and(|expected| expected != &actual)
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "filesystem observation `{path}` content does not match its Blob identity"
                    ),
                });
            }
            Some(actual)
        }
        None => observation.blob_name.clone(),
    };
    if observed_blob
        .as_deref()
        .is_some_and(|identity| !is_sha256_identity(identity))
    {
        return Err(WorkspaceError::InvalidSnapshot {
            reason: format!("filesystem observation `{path}` has a noncanonical Blob identity"),
        });
    }
    if observed_blob.is_some() && observed_blob == explicit.blob_name {
        if observation.size != explicit.size {
            return Err(WorkspaceError::InvalidSnapshot {
                reason: format!(
                    "filesystem observation `{path}` size does not match the saved explicit overlay"
                ),
            });
        }
        return Ok(true);
    }
    Ok(false)
}

/// Validate one restored file record before it enters the live state machine.
/// 在恢复的文件记录进入实时状态机前校验其不变量。
fn validate_snapshot_record(record: &FileRecord) -> Result<(), WorkspaceError> {
    if record.generation == 0 {
        return Err(WorkspaceError::InvalidSnapshot {
            reason: format!("record `{}` has generation zero", record.path),
        });
    }
    if record
        .committed_blob_name
        .as_deref()
        .is_some_and(|identity| !is_sha256_identity(identity))
    {
        return Err(WorkspaceError::InvalidSnapshot {
            reason: format!(
                "record `{}` has a noncanonical committed Blob identity",
                record.path
            ),
        });
    }
    match record.status {
        FileStatus::Present => {
            let blob =
                record
                    .blob_name
                    .as_ref()
                    .ok_or_else(|| WorkspaceError::InvalidSnapshot {
                        reason: format!("present record `{}` has no Blob identity", record.path),
                    })?;
            if !is_sha256_identity(blob) {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "present record `{}` has a noncanonical Blob identity",
                        record.path
                    ),
                });
            }
            if let Some(content) = record.content.as_deref() {
                if content.len() as u64 != record.size {
                    return Err(WorkspaceError::InvalidSnapshot {
                        reason: format!("record `{}` has inconsistent source size", record.path),
                    });
                }
                let actual = blob_name(&record.path, content).map_err(|error| {
                    WorkspaceError::InvalidSnapshot {
                        reason: format!("record `{}` cannot be hashed: {error}", record.path),
                    }
                })?;
                if &actual != blob {
                    return Err(WorkspaceError::InvalidSnapshot {
                        reason: format!("record `{}` content does not match its Blob", record.path),
                    });
                }
            }
            if record.source == FileSource::Explicit && record.content.is_none() {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "explicit present record `{}` has no overlay content",
                        record.path
                    ),
                });
            }
            if record.skip_reason.is_some() {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("present record `{}` retains a skip reason", record.path),
                });
            }
        }
        FileStatus::Skipped => {
            if record.blob_name.is_some() || record.content.is_some() {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "non-present record `{}` retains current Blob or content",
                        record.path
                    ),
                });
            }
            if record
                .skip_reason
                .as_ref()
                .is_none_or(|reason| reason.trim().is_empty())
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("skipped record `{}` has no skip reason", record.path),
                });
            }
        }
        FileStatus::Deleted => {
            if record.blob_name.is_some()
                || record.content.is_some()
                || record.skip_reason.is_some()
                || record.size != 0
                || record.mtime_ns.is_some()
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!("deleted record `{}` retains active data", record.path),
                });
            }
        }
    }
    Ok(())
}

/// Validate that one content-addressed Blob identity belongs to exactly one workspace path.
/// 校验一个内容寻址 Blob 身份只属于一个工作区路径。
fn validate_blob_ownership(records: &BTreeMap<String, FileRecord>) -> Result<(), WorkspaceError> {
    let mut owners = BTreeMap::<&str, &str>::new();
    for record in records.values() {
        for identity in [
            record.blob_name.as_deref(),
            record.committed_blob_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(owner) = owners.insert(identity, record.path.as_str())
                && owner != record.path
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: format!(
                        "Blob identity `{identity}` is claimed by both `{owner}` and `{}`",
                        record.path
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Validate checkpoint identity and committed membership in one restored snapshot.
/// 校验恢复快照中的检查点身份和已提交成员关系。
fn validate_snapshot_checkpoint(
    checkpoint: Option<&Checkpoint>,
    records: &BTreeMap<String, FileRecord>,
    generation: u64,
) -> Result<(), WorkspaceError> {
    if checkpoint.is_some_and(|checkpoint| {
        checkpoint
            .members
            .iter()
            .any(|member| !is_sha256_identity(member))
    }) {
        return Err(WorkspaceError::InvalidSnapshot {
            reason: "checkpoint contains a noncanonical Blob identity".to_owned(),
        });
    }
    let committed = records
        .values()
        .filter_map(|record| record.committed_blob_name.clone())
        .collect::<BTreeSet<_>>();
    match checkpoint {
        Some(checkpoint) => {
            if generation == 0
                || checkpoint.chain_id.trim().is_empty()
                || checkpoint.version == 0
                || checkpoint.version > generation
            {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: "checkpoint identity, version, or generation is invalid".to_owned(),
                });
            }
            if checkpoint.members != committed {
                return Err(WorkspaceError::InvalidSnapshot {
                    reason: "checkpoint members do not match committed file records".to_owned(),
                });
            }
        }
        None if !committed.is_empty() => {
            return Err(WorkspaceError::InvalidSnapshot {
                reason: "committed file records exist without a checkpoint".to_owned(),
            });
        }
        None => {}
    }
    Ok(())
}

/// Validate a new checkpoint before mutating the live workspace state.
/// 在修改实时工作区状态前校验新检查点。
fn validate_checkpoint_transition(
    previous: Option<&Checkpoint>,
    checkpoint: &Checkpoint,
    records: &BTreeMap<String, FileRecord>,
    generation: u64,
) -> Result<(), WorkspaceError> {
    if generation == 0
        || checkpoint.chain_id.trim().is_empty()
        || checkpoint.version == 0
        || checkpoint.version > generation
        || checkpoint
            .members
            .iter()
            .any(|member| !is_sha256_identity(member))
    {
        return Err(WorkspaceError::InvalidCheckpoint {
            reason: "checkpoint identity, version, generation, or members are invalid".to_owned(),
        });
    }
    let expected_version = previous
        .map_or(Some(1), |current| current.version.checked_add(1))
        .ok_or_else(|| WorkspaceError::InvalidCheckpoint {
            reason: "checkpoint version is exhausted".to_owned(),
        })?;
    if checkpoint.version != expected_version
        || previous.is_some_and(|current| current.chain_id != checkpoint.chain_id)
    {
        return Err(WorkspaceError::InvalidCheckpoint {
            reason: "checkpoint chain or version is not the next valid transition".to_owned(),
        });
    }
    let current_members = records
        .values()
        .filter(|record| record.is_present())
        .filter_map(|record| record.blob_name.clone())
        .collect::<BTreeSet<_>>();
    if checkpoint.members != current_members {
        return Err(WorkspaceError::InvalidCheckpoint {
            reason: "checkpoint members do not match the current present Blob set".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Verify full reconcile preserves explicit overlays over filesystem observations.
    /// 验证全量协调会保留覆盖文件系统观察结果的显式覆盖层。
    #[test]
    fn explicit_overlay_wins_during_reconcile() {
        let mut state = WorkspaceState::default();
        state.observe("src/lib.rs", "unsaved version").unwrap();
        state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "disk version",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        assert_eq!(
            state.records["src/lib.rs"].content.as_deref(),
            Some("unsaved version")
        );
        assert_eq!(state.records["src/lib.rs"].source, FileSource::Explicit);
    }

    /// Verify saving the exact overlay content returns ownership to the filesystem watcher.
    /// 验证保存与覆盖层完全相同的内容后所有权会归还文件系统监听器。
    #[test]
    fn saved_overlay_returns_to_filesystem_ownership() {
        let mut state = WorkspaceState::default();
        state.observe("src/lib.rs", "saved version").unwrap();

        state
            .reconcile(vec![FileObservation::present(
                "src/lib.rs",
                "saved version",
                Some(7),
                FileSource::Filesystem,
            )])
            .unwrap();

        let record = &state.records["src/lib.rs"];
        assert_eq!(record.source, FileSource::Filesystem);
        assert_eq!(record.content.as_deref(), Some("saved version"));
        assert_eq!(record.mtime_ns, Some(7));
    }

    /// Verify an incremental watcher refresh also clears an overlay after an exact save.
    /// 验证增量监听刷新也会在精确保存后清除覆盖层。
    #[test]
    fn saved_overlay_returns_to_filesystem_during_path_refresh() {
        let mut state = WorkspaceState::default();
        state.observe("src/lib.rs", "saved version").unwrap();
        let identity = blob_name("src/lib.rs", "saved version").unwrap();

        state
            .reconcile_paths(
                vec![FileObservation::identified(
                    "src/lib.rs",
                    identity,
                    "saved version".len() as u64,
                    Some(8),
                    FileSource::Filesystem,
                )],
                Vec::new(),
            )
            .unwrap();

        let record = &state.records["src/lib.rs"];
        assert_eq!(record.source, FileSource::Filesystem);
        assert!(record.content.is_none());
        assert_eq!(record.mtime_ns, Some(8));
    }

    /// Verify matching Blob metadata cannot clear an overlay while reporting a forged byte size.
    /// 验证 Blob 相同但字节大小伪造的元数据不能清除显式覆盖层。
    #[test]
    fn saved_overlay_rejects_inconsistent_filesystem_size() {
        let mut state = WorkspaceState::default();
        state.observe("src/lib.rs", "saved version").unwrap();
        let identity = blob_name("src/lib.rs", "saved version").unwrap();

        let result = state.reconcile_paths(
            vec![FileObservation::identified(
                "src/lib.rs",
                identity,
                "saved version".len() as u64 + 1,
                Some(9),
                FileSource::Filesystem,
            )],
            Vec::new(),
        );

        assert!(matches!(
            result,
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
        assert_eq!(state.generation, 1);
        assert_eq!(state.records["src/lib.rs"].source, FileSource::Explicit);
    }

    /// Verify a full filesystem scan retains an unsaved file absent from disk observations.
    /// 验证全量文件系统扫描会保留磁盘观察中不存在的未保存文件。
    #[test]
    fn full_reconcile_retains_explicit_file_absent_from_disk() {
        let mut state = WorkspaceState::default();
        state
            .observe("src/new_file.rs", "pub fn unsaved() {}")
            .unwrap();

        let snapshot = state.reconcile(Vec::new()).unwrap();

        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].path, "src/new_file.rs");
        assert_eq!(snapshot.records[0].status, FileStatus::Present);
        assert_eq!(snapshot.records[0].source, FileSource::Explicit);
        assert_eq!(
            snapshot.records[0].content.as_deref(),
            Some("pub fn unsaved() {}")
        );
    }

    /// Verify synchronization computes added and deleted Blob identities.
    /// 验证同步计划会计算新增和删除的 Blob 身份。
    #[test]
    fn computes_sync_plan_from_checkpoint() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([
                FileObservation::present("a.rs", "a", Some(1), FileSource::Filesystem),
                FileObservation::present("old.rs", "old", Some(1), FileSource::Filesystem),
            ])
            .unwrap();
        let a = state.records["a.rs"].blob_name.clone().unwrap();
        let old = state.records["old.rs"].blob_name.clone().unwrap();
        state
            .commit_checkpoint(Checkpoint::new("chain", 1, [a.clone(), old.clone()]))
            .unwrap();
        state
            .reconcile([
                FileObservation::present("a.rs", "a", Some(1), FileSource::Filesystem),
                FileObservation::present("b.rs", "b", Some(1), FileSource::Filesystem),
            ])
            .unwrap();
        let plan = state.sync_plan();
        assert!(
            plan.added_blobs
                .contains(&state.records["b.rs"].blob_name.clone().unwrap())
        );
        assert!(plan.deleted_blobs.contains(&old));
    }

    /// Verify an absent checkpoint never widens an empty scope to all records.
    /// 验证缺少检查点时空范围不会被扩大为全部记录。
    #[test]
    fn empty_scope_requires_checkpoint_or_explicit_additions() {
        let state = WorkspaceState::default();
        assert_eq!(
            state.resolve_scope(Vec::<String>::new(), Vec::<String>::new()),
            ScopeResolution::MissingCheckpoint
        );
    }

    /// Verify a checkpoint can produce an explicitly empty scope after deletion.
    /// 验证检查点在删除全部成员后可以产生明确的空范围。
    #[test]
    fn deleted_only_delta_produces_empty_scope() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([FileObservation::present(
                "deleted.rs",
                "deleted",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let deleted = state.records["deleted.rs"].blob_name.clone().unwrap();
        state
            .commit_checkpoint(Checkpoint::new("chain", 1, [deleted.clone()]))
            .unwrap();
        assert_eq!(
            state.resolve_scope(Vec::<String>::new(), [deleted]),
            ScopeResolution::Empty
        );
    }

    /// Verify a partial reconciliation updates only changed paths and keeps Blob deletion state.
    /// 验证部分协调只更新变化路径，并保留 Blob 删除状态。
    #[test]
    fn partial_reconcile_preserves_untouched_records() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([
                FileObservation::present("a.rs", "a", Some(1), FileSource::Filesystem),
                FileObservation::present("b.rs", "b", Some(1), FileSource::Filesystem),
                FileObservation::present("c.rs", "c", Some(1), FileSource::Filesystem),
            ])
            .unwrap();
        let b_blob = state.records["b.rs"].blob_name.clone().unwrap();
        state
            .commit_checkpoint(Checkpoint::new(
                "chain",
                1,
                state
                    .records
                    .values()
                    .filter_map(|record| record.blob_name.clone())
                    .collect::<Vec<_>>(),
            ))
            .unwrap();

        state
            .reconcile_paths(
                [FileObservation::present(
                    "a.rs",
                    "a changed",
                    Some(2),
                    FileSource::Filesystem,
                )],
                ["b.rs".to_owned()],
            )
            .unwrap();

        assert_eq!(state.records["c.rs"].content.as_deref(), Some("c"));
        assert_eq!(state.records["b.rs"].status, FileStatus::Deleted);
        assert_eq!(state.records["b.rs"].committed_blob_name, Some(b_blob));
        assert_eq!(state.records["a.rs"].content.as_deref(), Some("a changed"));
    }

    /// Verify a late invalid path cannot leave a partial incremental mutation behind.
    /// 验证后置非法路径不会留下部分增量修改。
    #[test]
    fn partial_reconcile_rolls_back_on_late_error() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "old",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let before = state.snapshot();
        let error = state
            .reconcile_paths(
                [
                    FileObservation::present("src/lib.rs", "new", Some(2), FileSource::Filesystem),
                    FileObservation::present(
                        "../escape.rs",
                        "invalid",
                        Some(2),
                        FileSource::Filesystem,
                    ),
                ],
                Vec::<String>::new(),
            )
            .unwrap_err();
        assert!(matches!(error, WorkspaceError::InvalidPath(_)));
        assert_eq!(state.snapshot(), before);
    }

    /// Verify full reconciliation rejects paths that normalize to the same identity.
    /// 验证全量协调会拒绝规范化后身份相同的路径。
    #[test]
    fn full_reconcile_rejects_normalized_duplicates() {
        let mut state = WorkspaceState::default();
        let error = state
            .reconcile([
                FileObservation::present("src/lib.rs", "one", Some(1), FileSource::Filesystem),
                FileObservation::present("src/./lib.rs", "two", Some(1), FileSource::Filesystem),
            ])
            .unwrap_err();
        assert!(matches!(error, WorkspaceError::DuplicatePath { .. }));
        assert_eq!(state.generation, 0);
        assert!(state.records.is_empty());
    }

    /// Verify a full metadata refresh cannot assign one Blob identity to two paths.
    /// 验证完整元数据刷新不能把同一 Blob 身份分配给两个路径。
    #[test]
    fn full_reconcile_rejects_duplicate_blob_ownership() {
        let mut state = WorkspaceState::default();
        let shared = crate::identity::content_hash("forged shared Blob");
        let error = state
            .reconcile([
                FileObservation::identified(
                    "src/a.rs",
                    shared.clone(),
                    1,
                    Some(1),
                    FileSource::Filesystem,
                ),
                FileObservation::identified("src/b.rs", shared, 1, Some(1), FileSource::Filesystem),
            ])
            .unwrap_err();

        assert!(matches!(error, WorkspaceError::InvalidSnapshot { .. }));
        assert_eq!(state.generation(), 0);
        assert!(state.records().is_empty());
    }

    /// Verify an incremental metadata refresh rejects duplicate Blob ownership transactionally.
    /// 验证增量元数据刷新会事务式拒绝重复 Blob 所有权。
    #[test]
    fn partial_reconcile_rejects_duplicate_blob_ownership() {
        let mut state = WorkspaceState::default();
        let shared = crate::identity::content_hash("forged shared Blob");
        state
            .reconcile([FileObservation::identified(
                "src/a.rs",
                shared.clone(),
                1,
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let before = state.snapshot();

        let error = state
            .reconcile_paths(
                [FileObservation::identified(
                    "src/b.rs",
                    shared,
                    1,
                    Some(1),
                    FileSource::Filesystem,
                )],
                Vec::<String>::new(),
            )
            .unwrap_err();

        assert!(matches!(error, WorkspaceError::InvalidSnapshot { .. }));
        assert_eq!(state.snapshot(), before);
    }

    /// Verify failed explicit operations do not consume a workspace generation.
    /// 验证失败的显式操作不会消耗工作区代数。
    #[test]
    fn invalid_explicit_path_does_not_advance_generation() {
        let mut state = WorkspaceState::default();
        assert!(state.observe("../escape.rs", "bad").is_err());
        assert_eq!(state.generation, 0);
        assert!(state.remove("../escape.rs").is_err());
        assert_eq!(state.generation, 0);
    }

    /// Verify metadata-only observations never reuse a Blob without a reliable timestamp.
    /// 验证缺少可靠时间戳的纯元数据观察绝不会复用旧 Blob。
    #[test]
    fn metadata_only_observation_without_mtime_requires_identity() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "fn old() {}",
                None,
                FileSource::Filesystem,
            )])
            .unwrap();

        let result = state.reconcile_paths(
            [FileObservation {
                path: "src/lib.rs".to_owned(),
                content: None,
                blob_name: None,
                size: "fn new() {}".len() as u64,
                mtime_ns: None,
                source: FileSource::Filesystem,
                status: FileStatus::Present,
                skip_reason: None,
            }],
            Vec::new(),
        );

        assert!(matches!(
            result,
            Err(WorkspaceError::MissingContent { path }) if path == "src/lib.rs"
        ));
        assert_eq!(state.generation, 1);
    }

    /// Verify restored snapshots reject duplicate canonical records.
    /// 验证恢复快照会拒绝重复的规范记录。
    #[test]
    fn restored_snapshot_rejects_duplicate_records() {
        let mut state = WorkspaceState::default();
        let snapshot = state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "content",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let mut invalid = snapshot;
        invalid.records.push(invalid.records[0].clone());
        assert!(matches!(
            WorkspaceState::from_snapshot(invalid),
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
    }

    /// Verify restored metadata cannot alias one Blob identity across multiple paths.
    /// 验证恢复的元数据不能让同一 Blob 身份跨多个路径产生别名。
    #[test]
    fn restored_snapshot_rejects_duplicate_blob_ownership() {
        let shared = crate::identity::content_hash("forged shared Blob");
        let mut state = WorkspaceState::default();
        let mut snapshot = state
            .reconcile([FileObservation::identified(
                "src/a.rs",
                shared.clone(),
                1,
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let mut duplicate = snapshot.records[0].clone();
        duplicate.path = "src/b.rs".to_owned();
        duplicate.blob_name = Some(shared);
        snapshot.records.push(duplicate);

        assert!(matches!(
            WorkspaceState::from_snapshot(snapshot),
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
    }

    /// Verify restored explicit overlays always retain their authoritative source text.
    /// 验证恢复的显式覆盖层始终保留其权威源码正文。
    #[test]
    fn restored_snapshot_rejects_explicit_overlay_without_content() {
        let mut state = WorkspaceState::default();
        state.observe("src/lib.rs", "fn pending() {}").unwrap();
        let mut snapshot = state.snapshot();
        snapshot.records[0].content = None;

        assert!(matches!(
            WorkspaceState::from_snapshot(snapshot),
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
    }

    /// Verify restored deletion records cannot retain stale filesystem metadata.
    /// 验证恢复的删除记录不能保留陈旧文件系统元数据。
    #[test]
    fn restored_snapshot_rejects_deleted_file_metadata() {
        let mut state = WorkspaceState::default();
        state.remove("src/lib.rs").unwrap();
        let mut snapshot = state.snapshot();
        snapshot.records[0].size = 1;
        snapshot.records[0].mtime_ns = Some(1);

        assert!(matches!(
            WorkspaceState::from_snapshot(snapshot),
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
    }

    /// Full-scan deletion must produce a restorable empty publication after retaining pending removals.
    /// 全量扫描删除在保留待处理移除之后必须生成可恢复的空发布。
    ///
    /// Takes no parameters and returns nothing; validates pending and committed snapshot boundaries.
    /// 无参数、无返回值；验证待处理与已提交快照边界。
    #[test]
    fn full_scan_removal_clears_live_metadata_and_preserves_pending_identity() {
        // Cover both vector-bearing files and previously skipped files with nonzero metadata.
        // 同时覆盖含向量文件与之前跳过且带非零元数据的文件。
        let mut state = WorkspaceState::default();
        state
            .reconcile([
                FileObservation::present(
                    "src/lib.rs",
                    "fn entry() {}",
                    Some(12),
                    FileSource::Filesystem,
                ),
                FileObservation::skipped(
                    "large.bin",
                    8192,
                    Some(24),
                    "binary",
                    FileSource::Filesystem,
                ),
            ])
            .expect("observe real and skipped metadata");
        let committed_blob = state.records["src/lib.rs"]
            .blob_name
            .clone()
            .expect("present blob");
        state
            .commit_checkpoint(Checkpoint::new("scan-delete", 1, [committed_blob.clone()]))
            .expect("commit original");
        let pending = state.reconcile(Vec::new()).expect("full scan now empty");
        assert_eq!(
            state.sync_plan().deleted_blobs,
            BTreeSet::from([committed_blob])
        );
        for record in &pending.records {
            assert_eq!(record.status, FileStatus::Deleted);
            assert_eq!(record.size, 0);
            assert_eq!(record.mtime_ns, None);
            assert!(record.content.is_none() && record.skip_reason.is_none());
        }
        WorkspaceState::from_snapshot(pending).expect("pending deletion remains restorable");
        state
            .commit_checkpoint(Checkpoint::new("scan-delete", 2, Vec::new()))
            .expect("commit empty");
        WorkspaceState::from_snapshot(state.snapshot())
            .expect("empty publication remains restorable");
    }

    /// Verify an invalid public checkpoint cannot corrupt an otherwise valid live state.
    /// 验证无效的公开检查点无法破坏原本有效的实时状态。
    #[test]
    fn checkpoint_commit_is_transactionally_validated() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([FileObservation::present(
                "a.rs",
                "a",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let before = state.snapshot();

        let error = state
            .commit_checkpoint(Checkpoint::new("chain", 2, ["not-a-blob".to_owned()]))
            .unwrap_err();
        assert!(matches!(error, WorkspaceError::InvalidCheckpoint { .. }));
        assert_eq!(state.snapshot(), before);
    }

    /// Verify an empty generation cannot publish the first checkpoint directly.
    /// 验证空代次不能直接发布第一个检查点。
    #[test]
    fn checkpoint_commit_requires_a_positive_workspace_generation() {
        let mut state = WorkspaceState::default();
        let before = state.snapshot();

        let error = state
            .commit_checkpoint(Checkpoint::new("chain", 1, Vec::<String>::new()))
            .unwrap_err();

        assert!(matches!(error, WorkspaceError::InvalidCheckpoint { .. }));
        assert_eq!(state.snapshot(), before);
    }

    /// Verify a restored checkpoint version cannot exceed its workspace generation.
    /// 验证恢复的检查点版本不能超过其工作区代次。
    #[test]
    fn restored_checkpoint_version_cannot_exceed_generation() {
        let snapshot = WorkspaceSnapshot {
            generation: 1,
            checkpoint: Some(Checkpoint::new("chain", 2, Vec::<String>::new())),
            records: Vec::new(),
        };

        assert!(matches!(
            WorkspaceState::from_snapshot(snapshot),
            Err(WorkspaceError::InvalidSnapshot { .. })
        ));
    }

    /// Verify a changed filesystem Blob keeps its source text until that exact Blob is committed.
    /// 验证变化后的文件系统 Blob 在其精确身份提交前会保留源码正文。
    #[test]
    fn content_release_preserves_changed_uncommitted_filesystem_blob() {
        let mut state = WorkspaceState::default();
        state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "fn first() {}",
                Some(1),
                FileSource::Filesystem,
            )])
            .unwrap();
        let committed = state.records["src/lib.rs"].blob_name.clone().unwrap();
        state
            .commit_checkpoint(Checkpoint::new("chain", 1, [committed]))
            .unwrap();
        state.release_committed_filesystem_content();
        assert!(state.records["src/lib.rs"].content.is_none());

        state
            .reconcile([FileObservation::present(
                "src/lib.rs",
                "fn second() {}",
                Some(2),
                FileSource::Filesystem,
            )])
            .unwrap();
        state.release_committed_filesystem_content();

        assert_eq!(
            state.records["src/lib.rs"].content.as_deref(),
            Some("fn second() {}")
        );
    }
}

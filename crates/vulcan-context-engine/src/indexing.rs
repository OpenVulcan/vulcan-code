//! Provider- and storage-neutral indexing state transitions.
//! 不依赖供应商和存储的索引状态转换。

use crate::chunking::{ChunkerError, RecursiveChunker, RecursiveChunkerConfig};
use crate::embedding::{EmbeddingError, EmbeddingInput, EmbeddingPlan, EmbeddingVector};
use crate::identity::{PathError, blob_name, content_hash, normalize_relative_path};
use crate::model::Chunk;
use crate::path_index::{PathDocument, PathDocumentError};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Explicit states used by the content indexing pipeline.
/// 内容索引管线使用的明确状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    /// Blob metadata and staging content are waiting for preparation.
    /// Blob 元数据和暂存内容等待准备。
    Pending,
    /// All required content processing completed successfully.
    /// 所有必需的内容处理已成功完成。
    Ready,
    /// Processing failed and the error must remain observable.
    /// 处理失败，错误必须保持可观测。
    Error,
}

/// Configuration for the provider-neutral indexing state machine.
/// 不依赖供应商的索引状态机配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexingConfig {
    /// Maximum accepted UTF-8 content size in bytes.
    /// 接受的 UTF-8 内容最大字节大小。
    pub max_file_bytes: u64,
    /// Recursive source Chunker limits.
    /// 递归源码切块器限制。
    pub chunker: RecursiveChunkerConfig,
    /// Maximum provider items per Embedding batch.
    /// 每个向量批次允许的最大供应商输入条数。
    pub embedding_max_inputs: usize,
    /// Maximum Unicode scalar values per Embedding batch.
    /// 每个向量批次允许的最大 Unicode 标量值数量。
    pub embedding_max_chars: usize,
    /// Maximum Unicode scalar values in one provider segment.
    /// 一个供应商片段允许的最大 Unicode 标量值数量。
    pub embedding_max_segment_chars: usize,
    /// Character overlap between long-input segments.
    /// 长输入片段之间的字符重叠数量。
    pub embedding_overlap_chars: usize,
    /// Maximum worker retries before terminal error.
    /// 进入终态错误前允许的最大工作重试次数。
    pub max_retries: u32,
}

impl Default for IndexingConfig {
    /// Return lightweight defaults derived from the extracted OCE behavior.
    /// 返回根据提取 OCE 行为得到的轻量默认配置。
    fn default() -> Self {
        Self {
            max_file_bytes: 1_048_576,
            chunker: RecursiveChunkerConfig::default(),
            embedding_max_inputs: 32,
            embedding_max_chars: 32_000,
            embedding_max_segment_chars: 8_000,
            embedding_overlap_chars: 400,
            max_retries: 3,
        }
    }
}

/// A newly accepted Blob waiting for chunking and Embedding.
/// 等待切块和向量化的新接收 Blob。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingIndex {
    /// Stable path-plus-content Blob identity.
    /// 稳定的路径加内容 Blob 身份。
    pub blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Staged verbatim source content.
    /// 暂存的原始源码内容。
    pub content: String,
    /// Snapshot generation that accepted this Blob.
    /// 接收该 Blob 的快照代数。
    pub generation: u64,
    /// Whether staging content is still available.
    /// 暂存内容是否仍然可用。
    pub staging_present: bool,
    /// Number of prior failed processing attempts.
    /// 之前失败的处理尝试次数。
    pub retry_count: u32,
}

/// A Blob after deterministic Chunking and before provider execution.
/// 完成确定性切块但尚未执行供应商调用的 Blob。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedIndex {
    /// Pending Blob metadata and staging state.
    /// 待处理 Blob 元数据和暂存状态。
    pub pending: PendingIndex,
    /// Source-preserving Chunks in deterministic order.
    /// 按确定性顺序排列且保留源码的代码块。
    pub chunks: Vec<Chunk>,
    /// Embedding request plan for the Chunks.
    /// 代码块对应的向量请求计划。
    pub embedding_plan: Option<EmbeddingPlan>,
    /// Auxiliary path document to be indexed after the Blob is ready.
    /// Blob 就绪后写入辅助索引的路径文档。
    pub path_document: Option<PathDocument>,
}

/// A successfully prepared result ready for upper-layer persistence.
/// 可交给上层持久化的成功准备结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyIndex {
    /// Stable Blob identity.
    /// 稳定的 Blob 身份。
    pub blob_name: String,
    /// Canonical source path.
    /// 规范化的源码路径。
    pub path: String,
    /// Snapshot generation that produced the result.
    /// 生成该结果的快照代数。
    pub generation: u64,
    /// Source-preserving Chunks to persist.
    /// 待持久化的保留源码代码块。
    pub chunks: Vec<Chunk>,
    /// One normalized vector for each logical Chunk input.
    /// 每个逻辑代码块输入对应的一个归一化向量。
    pub embedding_vectors: Vec<EmbeddingVector>,
    /// Path document to persist independently of the content index.
    /// 独立于内容索引持久化的路径文档。
    pub path_document: Option<PathDocument>,
    /// Terminal state of this result.
    /// 该结果的终态。
    pub status: IndexStatus,
}

/// An ingest result that keeps fast acceptance separate from background work.
/// 将快速接收与后台处理分离的接收结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestResult {
    /// A text Blob waiting for asynchronous preparation.
    /// 等待异步准备的文本 Blob。
    Pending(PendingIndex),
    /// A binary or over-limit Blob that is ready without semantic indexing.
    /// 无需语义索引即可就绪的二进制或超限 Blob。
    ReadyWithoutEmbedding {
        /// Stable Blob identity.
        /// 稳定的 Blob 身份。
        blob_name: String,
        /// Canonical source path.
        /// 规范化的源码路径。
        path: String,
        /// Reason semantic indexing was skipped.
        /// 跳过语义索引的原因。
        reason: String,
        /// Snapshot generation that accepted the Blob.
        /// 接收该 Blob 的快照代数。
        generation: u64,
    },
}

/// The result of a failed attempt and its next safe state transition.
/// 失败尝试的结果及其下一步安全状态转换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureDecision {
    /// Updated retry count.
    /// 更新后的重试次数。
    pub retry_count: u32,
    /// Whether the caller may requeue the Blob.
    /// 调用方是否可以重新排队该 Blob。
    pub should_retry: bool,
    /// Whether staging must be cleaned after terminal failure.
    /// 进入终态失败后是否必须清理暂存内容。
    pub cleanup_staging: bool,
    /// Resulting status after the failed attempt.
    /// 失败尝试后的结果状态。
    pub status: IndexStatus,
}

/// Errors raised by indexing preparation and completion.
/// 索引准备和完成阶段产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexingError {
    /// The source path is invalid.
    /// 源码路径无效。
    InvalidPath(PathError),
    /// The source content exceeds the configured limit.
    /// 源码内容超过配置限制。
    FileTooLarge {
        /// Exact source size rejected by admission control.
        /// 被准入控制拒绝的精确源码大小。
        bytes: u64,
        /// Maximum source size allowed by the indexing configuration.
        /// 索引配置允许的最大源码大小。
        max_bytes: u64,
    },
    /// The Blob was accepted as present but staging text is unavailable.
    /// Blob 已接收为存在状态，但暂存文本不可用。
    MissingStaging {
        /// Blob identity whose source text is unavailable for preparation.
        /// 准备阶段缺少源码文本的 Blob 身份。
        blob_name: String,
    },
    /// Chunking failed before any provider call was made.
    /// 在供应商调用前切块失败。
    Chunking(ChunkerError),
    /// Embedding planning or response validation failed.
    /// 向量计划或响应校验失败。
    Embedding(EmbeddingError),
    /// Path document construction failed.
    /// 路径文档构建失败。
    PathDocument(PathDocumentError),
    /// The expected physical vector dimension is invalid.
    /// 期望的物理向量维度无效。
    InvalidDimension,
    /// One or more indexing admission or Embedding budgets are invalid.
    /// 一个或多个索引准入或向量预算无效。
    InvalidConfig,
    /// The workspace generation is zero and cannot identify indexed source state.
    /// 工作区代次为零，无法标识已索引源码状态。
    InvalidGeneration,
    /// A public pending record reports more prior failures than the configured retry budget.
    /// 公开待处理记录报告的既往失败次数超过配置的重试预算。
    RetryLimitExceeded {
        /// Number of prior failures carried by the pending record.
        /// 待处理记录携带的既往失败次数。
        retry_count: u32,
        /// Maximum prior failures allowed before the final retry attempt.
        /// 最后一次重试前允许的最大既往失败次数。
        max_retries: u32,
    },
    /// An injected structural analyzer returned chunks outside the accepted source Blob.
    /// 注入的结构分析器返回了超出已接收源码 Blob 的代码块。
    InvalidChunkSet {
        /// Explicit structural-analysis contract violation.
        /// 明确的结构分析契约违反原因。
        reason: String,
    },
}

impl Display for IndexingError {
    /// Formats an indexing error for deterministic diagnostics.
    /// 将索引错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid indexing path: {error}"),
            Self::FileTooLarge { bytes, max_bytes } => {
                write!(
                    formatter,
                    "file size {bytes} exceeds configured limit {max_bytes}"
                )
            }
            Self::MissingStaging { blob_name } => {
                write!(formatter, "staging content is missing for Blob {blob_name}")
            }
            Self::Chunking(error) => write!(formatter, "index Chunking failed: {error}"),
            Self::Embedding(error) => write!(formatter, "index Embedding failed: {error}"),
            Self::PathDocument(error) => write!(formatter, "path document failed: {error}"),
            Self::InvalidDimension => {
                formatter.write_str("expected Embedding dimension must be positive")
            }
            Self::InvalidConfig => formatter.write_str("indexing budgets must be positive"),
            Self::InvalidGeneration => {
                formatter.write_str("indexed source generation must be positive")
            }
            Self::RetryLimitExceeded {
                retry_count,
                max_retries,
            } => write!(
                formatter,
                "pending retry count {retry_count} exceeds configured limit {max_retries}"
            ),
            Self::InvalidChunkSet { reason } => {
                write!(formatter, "structural Chunk set is invalid: {reason}")
            }
        }
    }
}

impl Error for IndexingError {}

/// Orchestrate ingest, Chunking, and vector-result validation without owning storage.
/// 编排接收、切块和向量结果校验，但不拥有存储实现。
#[derive(Debug, Clone)]
pub struct IndexingEngine {
    /// Explicit indexing configuration.
    /// 明确的索引配置。
    pub config: IndexingConfig,
    /// Source-preserving fallback Chunker.
    /// 保留源码的回退切块器。
    pub chunker: RecursiveChunker,
}

impl IndexingEngine {
    /// Construct an indexing engine from explicit configuration.
    /// 根据明确配置创建索引引擎。
    pub fn new(config: IndexingConfig) -> Result<Self, IndexingError> {
        if config.max_file_bytes == 0
            || config.embedding_max_inputs == 0
            || config.embedding_max_chars == 0
            || config.embedding_max_segment_chars == 0
            || config.embedding_overlap_chars >= config.embedding_max_segment_chars
        {
            return Err(IndexingError::InvalidConfig);
        }
        let chunker = RecursiveChunker::new(config.chunker).map_err(IndexingError::Chunking)?;
        Ok(Self { config, chunker })
    }

    /// Accept a path and source quickly without Chunking or provider calls.
    /// 快速接收路径和源码，不在接收阶段切块或调用供应商。
    pub fn ingest(
        &self,
        path: &str,
        content: &str,
        generation: u64,
    ) -> Result<IngestResult, IndexingError> {
        if generation == 0 {
            return Err(IndexingError::InvalidGeneration);
        }
        let path = normalize_relative_path(path).map_err(IndexingError::InvalidPath)?;
        let bytes = content.len() as u64;
        let blob_name = blob_name(&path, content).map_err(IndexingError::InvalidPath)?;
        if bytes > self.config.max_file_bytes {
            return Ok(IngestResult::ReadyWithoutEmbedding {
                blob_name,
                path,
                reason: "file exceeds configured admission size".to_owned(),
                generation,
            });
        }
        if content.contains('\0') {
            return Ok(IngestResult::ReadyWithoutEmbedding {
                blob_name,
                path,
                reason: "content contains a NUL byte and is treated as binary".to_owned(),
                generation,
            });
        }
        Ok(IngestResult::Pending(PendingIndex {
            blob_name,
            path,
            content: content.to_owned(),
            generation,
            staging_present: true,
            retry_count: 0,
        }))
    }

    /// Prepare one pending Blob into source Chunks and provider-sized Embedding batches.
    /// 将一个待处理 Blob 准备为源码代码块和供应商批次。
    pub fn prepare(&self, pending: PendingIndex) -> Result<PreparedIndex, IndexingError> {
        self.validate_pending(&pending)?;
        let chunks = self
            .chunker
            .chunk(pending.blob_name.clone(), &pending.path, &pending.content)
            .map_err(IndexingError::Chunking)?;
        self.prepare_with_chunks(pending, chunks)
    }

    /// Prepare one pending Blob from host-independent structural chunks.
    /// 使用不依赖宿主的结构化代码块准备一个待处理 Blob。
    pub fn prepare_with_chunks(
        &self,
        pending: PendingIndex,
        chunks: Vec<Chunk>,
    ) -> Result<PreparedIndex, IndexingError> {
        self.validate_pending(&pending)?;
        validate_chunks(&pending, &chunks)?;
        let embedding_plan = self.build_embedding_plan(&chunks)?;
        let path_document = PathDocument::new(pending.blob_name.clone(), &pending.path)
            .map(Some)
            .map_err(IndexingError::PathDocument)?;
        Ok(PreparedIndex {
            pending,
            chunks,
            embedding_plan,
            path_document,
        })
    }

    /// Validate provider segment vectors and transition a prepared Blob to Ready.
    /// 校验供应商片段向量并将准备好的 Blob 转换为就绪状态。
    pub fn complete(
        &self,
        prepared: PreparedIndex,
        vectors: Vec<crate::embedding::SegmentVector>,
        expected_dimension: usize,
    ) -> Result<ReadyIndex, IndexingError> {
        if expected_dimension == 0 {
            return Err(IndexingError::InvalidDimension);
        }
        self.validate_pending(&prepared.pending)?;
        validate_chunks(&prepared.pending, &prepared.chunks)?;
        let expected_embedding_plan = self.build_embedding_plan(&prepared.chunks)?;
        let expected_path_document =
            PathDocument::new(prepared.pending.blob_name.clone(), &prepared.pending.path)
                .map(Some)
                .map_err(IndexingError::PathDocument)?;
        if prepared.embedding_plan != expected_embedding_plan
            || prepared.path_document != expected_path_document
        {
            return Err(IndexingError::InvalidChunkSet {
                reason: "prepared indexing artifacts do not match the staged source".to_owned(),
            });
        }
        let embedding_vectors = match prepared.embedding_plan {
            Some(plan) => plan.aggregate(vectors).map_err(IndexingError::Embedding)?,
            None => {
                if !vectors.is_empty() {
                    return Err(IndexingError::Embedding(
                        EmbeddingError::ResultCountMismatch {
                            expected: 0,
                            actual: vectors.len(),
                        },
                    ));
                }
                Vec::new()
            }
        };
        if let Some(vector) = embedding_vectors
            .iter()
            .find(|vector| vector.values.len() != expected_dimension)
        {
            return Err(IndexingError::Embedding(
                EmbeddingError::DimensionMismatch {
                    expected: expected_dimension,
                    actual: vector.values.len(),
                },
            ));
        }
        Ok(ReadyIndex {
            blob_name: prepared.pending.blob_name,
            path: prepared.pending.path,
            generation: prepared.pending.generation,
            chunks: prepared.chunks,
            embedding_vectors,
            path_document: prepared.path_document,
            status: IndexStatus::Ready,
        })
    }

    /// Build the deterministic Embedding plan represented by an exact Chunk set.
    /// 根据精确代码块集合构建确定性的向量计划。
    fn build_embedding_plan(
        &self,
        chunks: &[Chunk],
    ) -> Result<Option<EmbeddingPlan>, IndexingError> {
        if chunks.is_empty() {
            return Ok(None);
        }
        let inputs = chunks
            .iter()
            .map(|chunk| EmbeddingInput::new(chunk.chunk_id.clone(), chunk.embedding_text()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(IndexingError::Embedding)?;
        EmbeddingPlan::new(
            inputs,
            self.config.embedding_max_inputs,
            self.config.embedding_max_chars,
            self.config.embedding_max_segment_chars,
            self.config.embedding_overlap_chars,
        )
        .map(Some)
        .map_err(IndexingError::Embedding)
    }

    /// Revalidate a publicly constructible pending record before any expensive processing.
    /// 在任何高成本处理前重新校验可公开构造的待处理记录。
    fn validate_pending(&self, pending: &PendingIndex) -> Result<(), IndexingError> {
        if !pending.staging_present {
            return Err(IndexingError::MissingStaging {
                blob_name: pending.blob_name.clone(),
            });
        }
        if pending.generation == 0 {
            return Err(IndexingError::InvalidGeneration);
        }
        if pending.retry_count > self.config.max_retries {
            return Err(IndexingError::RetryLimitExceeded {
                retry_count: pending.retry_count,
                max_retries: self.config.max_retries,
            });
        }
        let normalized =
            normalize_relative_path(&pending.path).map_err(IndexingError::InvalidPath)?;
        if normalized != pending.path {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!("pending path `{}` is not canonical", pending.path),
            });
        }
        let bytes = pending.content.len() as u64;
        if bytes > self.config.max_file_bytes {
            return Err(IndexingError::FileTooLarge {
                bytes,
                max_bytes: self.config.max_file_bytes,
            });
        }
        if pending.content.contains('\0') {
            return Err(IndexingError::InvalidChunkSet {
                reason: "pending source contains a NUL byte and cannot enter text indexing"
                    .to_owned(),
            });
        }
        let expected_blob =
            blob_name(&pending.path, &pending.content).map_err(IndexingError::InvalidPath)?;
        if pending.blob_name != expected_blob {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "pending Blob `{}` does not match its path and source content",
                    pending.blob_name
                ),
            });
        }
        Ok(())
    }

    /// Compute retry and staging cleanup behavior after a failed processing attempt.
    /// 计算处理失败后的重试和暂存清理行为。
    pub fn failure_decision(&self, retry_count: u32) -> FailureDecision {
        let next_retry_count = retry_count.saturating_add(1);
        let should_retry = retry_count
            .checked_add(1)
            .is_some_and(|next| next <= self.config.max_retries);
        FailureDecision {
            retry_count: next_retry_count,
            should_retry,
            cleanup_staging: !should_retry,
            status: if should_retry {
                IndexStatus::Pending
            } else {
                IndexStatus::Error
            },
        }
    }
}

/// Validate injected chunks against the staged source identity and exact source text.
/// 根据暂存源码身份和原始文本校验注入的代码块。
fn validate_chunks(pending: &PendingIndex, chunks: &[Chunk]) -> Result<(), IndexingError> {
    if pending.generation == 0 {
        return Err(IndexingError::InvalidGeneration);
    }
    let normalized_path =
        normalize_relative_path(&pending.path).map_err(IndexingError::InvalidPath)?;
    if pending.path != normalized_path {
        return Err(IndexingError::InvalidChunkSet {
            reason: format!("pending path `{}` is not canonical", pending.path),
        });
    }
    let expected_blob =
        blob_name(&pending.path, &pending.content).map_err(IndexingError::InvalidPath)?;
    if pending.blob_name != expected_blob {
        return Err(IndexingError::InvalidChunkSet {
            reason: format!(
                "pending Blob `{}` does not match its path and source content",
                pending.blob_name
            ),
        });
    }
    if chunks.is_empty() && pending.content.chars().any(char::is_alphanumeric) {
        return Err(IndexingError::InvalidChunkSet {
            reason: "semantic source produced no Chunks".to_owned(),
        });
    }
    let mut identities = BTreeSet::new();
    let mut previous_order = None::<(usize, usize, usize, String)>;
    let source_lines = pending.content.split_inclusive('\n').collect::<Vec<_>>();
    for chunk in chunks {
        let order = (
            chunk.range.start_line,
            chunk.start_char_offset,
            chunk.range.end_line,
            chunk.chunk_id.clone(),
        );
        if previous_order
            .as_ref()
            .is_some_and(|previous| previous >= &order)
        {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "Chunk `{}` is not in canonical source order",
                    chunk.chunk_id
                ),
            });
        }
        previous_order = Some(order);
        if chunk.kind == crate::model::ChunkKind::Path {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "Chunk `{}` is a synthetic path document, not source content",
                    chunk.chunk_id
                ),
            });
        }
        if crate::model::source_line_count(&chunk.content) != chunk.range.line_count() {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "Chunk `{}` source text does not match its declared line range",
                    chunk.chunk_id
                ),
            });
        }
        if chunk.blob_name != pending.blob_name || chunk.path != pending.path {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!("Chunk `{}` belongs to another Blob or path", chunk.chunk_id),
            });
        }
        if !identities.insert(chunk.chunk_id.clone()) {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!("Chunk identity `{}` is duplicated", chunk.chunk_id),
            });
        }
        let start = chunk.range.start_line.saturating_sub(1);
        let end = chunk.range.end_line;
        if start >= source_lines.len() || end > source_lines.len() || start >= end {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!("Chunk `{}` has an out-of-range line span", chunk.chunk_id),
            });
        }
        let range_text = source_lines[start..end].concat();
        let first_line_char_count = source_lines[start].chars().count();
        let content_start = (chunk.start_char_offset < first_line_char_count)
            .then(|| range_text.char_indices().nth(chunk.start_char_offset))
            .flatten()
            .map(|(byte, _)| byte);
        if content_start
            .and_then(|byte| range_text.get(byte..))
            .is_none_or(|tail| !tail.starts_with(&chunk.content))
        {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "Chunk `{}` content does not start at its declared line offset",
                    chunk.chunk_id
                ),
            });
        }
        let expected_content_hash = content_hash(&chunk.content);
        let expected_chunk_id = crate::identity::chunk_id_with_offset(
            &chunk.blob_name,
            &expected_content_hash,
            chunk.range.start_line,
            chunk.range.end_line,
            chunk.start_char_offset,
        );
        if chunk.content_hash != expected_content_hash || chunk.chunk_id != expected_chunk_id {
            return Err(IndexingError::InvalidChunkSet {
                reason: format!(
                    "Chunk `{}` has inconsistent content identity",
                    chunk.chunk_id
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::embedding::SegmentVector;

    /// Verify ingest is lightweight and defers Chunking to preparation.
    /// 验证接收阶段轻量化，并把切块推迟到准备阶段。
    #[test]
    fn ingest_defers_processing() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let result = engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap();
        let IngestResult::Pending(pending) = result else {
            panic!("text source should be pending");
        };
        assert!(pending.staging_present);
        assert_eq!(pending.retry_count, 0);
    }

    /// Verify no public indexing path accepts the reserved empty generation.
    /// 验证任何公开索引路径都不会接受保留的空代次。
    #[test]
    fn rejects_zero_generation() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        assert_eq!(
            engine.ingest("src/lib.rs", "fn main() {}", 0),
            Err(IndexingError::InvalidGeneration)
        );
    }

    /// Verify standalone construction enforces the same budgets as the top-level engine.
    /// 验证独立构造会执行与顶层引擎相同的预算约束。
    #[test]
    fn constructor_rejects_invalid_embedding_budgets() {
        let config = IndexingConfig {
            embedding_max_inputs: 0,
            ..IndexingConfig::default()
        };

        assert!(matches!(
            IndexingEngine::new(config),
            Err(IndexingError::InvalidConfig)
        ));
    }

    /// Verify callers cannot forge a pending record that bypasses admission control.
    /// 验证调用方不能伪造待处理记录绕过准入控制。
    #[test]
    fn prepare_revalidates_public_pending_admission() {
        let config = IndexingConfig {
            max_file_bytes: 4,
            ..IndexingConfig::default()
        };
        let engine = IndexingEngine::new(config).unwrap();
        let pending = PendingIndex {
            blob_name: blob_name("src/lib.rs", "abcde").unwrap(),
            path: "src/lib.rs".to_owned(),
            content: "abcde".to_owned(),
            generation: 1,
            staging_present: true,
            retry_count: 0,
        };

        assert_eq!(
            engine.prepare(pending),
            Err(IndexingError::FileTooLarge {
                bytes: 5,
                max_bytes: 4,
            })
        );
    }

    /// Verify preparation creates stable Chunk inputs and an independent path document.
    /// 验证准备阶段创建稳定代码块输入和独立路径文档。
    #[test]
    fn prepares_chunks_and_embedding_plan() {
        let engine = IndexingEngine::new(IndexingConfig {
            chunker: RecursiveChunkerConfig {
                max_chars: 100,
                min_chars: 1,
            },
            ..IndexingConfig::default()
        })
        .unwrap();
        let IngestResult::Pending(pending) =
            engine.ingest("src/lib.rs", "fn main() {}", 2).unwrap()
        else {
            panic!("text source should be pending");
        };
        let prepared = engine.prepare(pending).unwrap();
        assert_eq!(prepared.chunks.len(), 1);
        assert!(prepared.embedding_plan.is_some());
        assert!(prepared.path_document.is_some());
    }

    /// Verify repeated text fragments on one physical line retain distinct source occurrences.
    /// 验证同一物理行上的重复文本片段会保留不同的源码出现位置。
    #[test]
    fn prepares_repeated_long_line_fragments_without_identity_collision() {
        let engine = IndexingEngine::new(IndexingConfig {
            chunker: RecursiveChunkerConfig {
                max_chars: 4,
                min_chars: 1,
            },
            ..IndexingConfig::default()
        })
        .unwrap();
        let IngestResult::Pending(pending) = engine.ingest("src/lib.rs", "abc1abc1", 1).unwrap()
        else {
            panic!("text source should be pending");
        };

        let prepared = engine.prepare(pending).unwrap();

        assert_eq!(prepared.chunks.len(), 2);
        assert_ne!(prepared.chunks[0].chunk_id, prepared.chunks[1].chunk_id);
        assert_eq!(prepared.chunks[0].start_char_offset, 0);
        assert_eq!(prepared.chunks[1].start_char_offset, 4);
    }

    /// Verify an injected Chunk cannot claim a character offset where its text does not begin.
    /// 验证注入的代码块不能声明与正文起点不符的字符偏移。
    #[test]
    fn rejects_forged_structural_chunk_offset() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let IngestResult::Pending(pending) = engine.ingest("src/lib.rs", "abc1abc1", 1).unwrap()
        else {
            panic!("text source should be pending");
        };
        let forged = Chunk::new_with_start_char_offset(
            pending.blob_name.clone(),
            2,
            &pending.path,
            "abc1",
            1,
            1,
            crate::model::ChunkKind::Ast,
        )
        .unwrap();

        assert!(matches!(
            engine.prepare_with_chunks(pending, vec![forged]),
            Err(IndexingError::InvalidChunkSet { .. })
        ));
    }

    /// Verify injected structural chunks cannot forge their content-addressed identities.
    /// 验证注入的结构化代码块不能伪造其内容寻址身份。
    #[test]
    fn rejects_forged_structural_chunk_identity() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let IngestResult::Pending(pending) =
            engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap()
        else {
            panic!("text source should be pending");
        };
        let mut chunks = engine
            .chunker
            .chunk(pending.blob_name.clone(), &pending.path, &pending.content)
            .unwrap();
        chunks[0].content_hash = content_hash("forged");

        assert!(matches!(
            engine.prepare_with_chunks(pending, chunks),
            Err(IndexingError::InvalidChunkSet { .. })
        ));
    }

    /// Verify injected structural chunks must retain canonical source order.
    /// 验证注入的结构化代码块必须保持规范源码顺序。
    #[test]
    fn rejects_out_of_order_structural_chunks() {
        let engine = IndexingEngine::new(IndexingConfig {
            chunker: RecursiveChunkerConfig {
                max_chars: 4,
                min_chars: 1,
            },
            ..IndexingConfig::default()
        })
        .unwrap();
        let IngestResult::Pending(pending) = engine.ingest("src/lib.rs", "abc1abc2", 1).unwrap()
        else {
            panic!("text source should be pending");
        };
        let mut chunks = engine
            .chunker
            .chunk(pending.blob_name.clone(), &pending.path, &pending.content)
            .unwrap();
        chunks.reverse();

        assert!(matches!(
            engine.prepare_with_chunks(pending, chunks),
            Err(IndexingError::InvalidChunkSet { .. })
        ));
    }

    /// Verify direct structural preparation cannot drop all retrievable source content.
    /// 验证直接结构准备不能丢弃全部可检索源码内容。
    #[test]
    fn rejects_empty_chunks_for_semantic_source() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let IngestResult::Pending(pending) =
            engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap()
        else {
            panic!("text source should be pending");
        };

        assert!(matches!(
            engine.prepare_with_chunks(pending, Vec::new()),
            Err(IndexingError::InvalidChunkSet { .. })
        ));
    }

    /// Verify completion rejects publicly forged plans that detach vectors from prepared Chunks.
    /// 验证完成阶段会拒绝把向量与已准备代码块脱离的公开伪造计划。
    #[test]
    fn completion_revalidates_prepared_artifacts() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let pending = match engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap() {
            IngestResult::Pending(pending) => pending,
            IngestResult::ReadyWithoutEmbedding { .. } => panic!("expected pending"),
        };
        let mut prepared = engine.prepare(pending).unwrap();
        assert!(!prepared.chunks.is_empty());
        prepared.embedding_plan = None;

        assert!(matches!(
            engine.complete(prepared, Vec::new(), 3),
            Err(IndexingError::InvalidChunkSet { .. })
        ));
    }

    /// Verify completion rejects partial provider results instead of writing partial success.
    /// 验证完成阶段拒绝部分供应商结果而不是写入部分成功。
    #[test]
    fn completion_requires_all_segment_results() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let IngestResult::Pending(pending) =
            engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap()
        else {
            panic!("text source should be pending");
        };
        let prepared = engine.prepare(pending).unwrap();
        let error = engine.complete(prepared, Vec::new(), 2).unwrap_err();
        assert!(matches!(
            error,
            IndexingError::Embedding(EmbeddingError::ResultCountMismatch { .. })
        ));
    }

    /// Verify successful completion returns normalized vectors and ready state.
    /// 验证成功完成会返回归一化向量和就绪状态。
    #[test]
    fn completion_transitions_to_ready() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let IngestResult::Pending(pending) =
            engine.ingest("src/lib.rs", "fn main() {}", 1).unwrap()
        else {
            panic!("text source should be pending");
        };
        let prepared = engine.prepare(pending).unwrap();
        let segment_id =
            prepared.embedding_plan.as_ref().unwrap().batches[0].segments[0].request_input_id();
        let ready = engine
            .complete(
                prepared,
                vec![SegmentVector {
                    input_id: segment_id,
                    values: vec![3.0, 4.0],
                }],
                2,
            )
            .unwrap();
        assert_eq!(ready.status, IndexStatus::Ready);
        assert_eq!(ready.embedding_vectors.len(), 1);
    }

    /// Verify retry exhaustion becomes an observable terminal error and cleans staging.
    /// 验证重试耗尽后进入可观测终态错误并清理暂存。
    #[test]
    fn retry_exhaustion_is_explicit() {
        let engine = IndexingEngine::new(IndexingConfig {
            max_retries: 1,
            ..IndexingConfig::default()
        })
        .unwrap();
        assert!(engine.failure_decision(0).should_retry);
        let terminal = engine.failure_decision(1);
        assert!(!terminal.should_retry);
        assert!(terminal.cleanup_staging);
        assert_eq!(terminal.status, IndexStatus::Error);
    }

    /// Verify an exhausted integer counter cannot become an infinite retry state.
    /// 验证耗尽的整数计数器不会变成无限重试状态。
    #[test]
    fn retry_counter_exhaustion_is_terminal() {
        let config = IndexingConfig {
            max_retries: u32::MAX,
            ..IndexingConfig::default()
        };
        let engine = IndexingEngine::new(config).unwrap();
        let decision = engine.failure_decision(u32::MAX);

        assert_eq!(decision.retry_count, u32::MAX);
        assert!(!decision.should_retry);
        assert!(decision.cleanup_staging);
        assert_eq!(decision.status, IndexStatus::Error);
    }

    /// Verify a forged pending record cannot re-enter processing after retry exhaustion.
    /// 验证伪造的待处理记录不能在重试耗尽后重新进入处理流程。
    #[test]
    fn prepare_rejects_retry_count_above_budget() {
        let engine = IndexingEngine::new(IndexingConfig {
            max_retries: 1,
            ..IndexingConfig::default()
        })
        .unwrap();
        let pending = PendingIndex {
            blob_name: blob_name("src/lib.rs", "fn main() {}").unwrap(),
            path: "src/lib.rs".to_owned(),
            content: "fn main() {}".to_owned(),
            generation: 1,
            staging_present: true,
            retry_count: 2,
        };

        assert_eq!(
            engine.prepare(pending),
            Err(IndexingError::RetryLimitExceeded {
                retry_count: 2,
                max_retries: 1,
            })
        );
    }

    /// Verify missing staging fails even when the retained source is empty.
    /// 验证即使保留内容为空，缺失暂存也会显式失败。
    #[test]
    fn missing_staging_is_always_observable() {
        let engine = IndexingEngine::new(IndexingConfig::default()).unwrap();
        let error = engine
            .prepare(PendingIndex {
                blob_name: String::from("blob"),
                path: String::from("src/empty.rs"),
                content: String::new(),
                generation: 1,
                staging_present: false,
                retry_count: 0,
            })
            .unwrap_err();
        assert!(matches!(error, IndexingError::MissingStaging { .. }));
    }
}

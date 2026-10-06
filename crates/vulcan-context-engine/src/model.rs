//! Domain records shared by synchronization, indexing, and retrieval.
//! 在同步、索引和检索之间共享的领域记录。

use crate::identity::{PathError, chunk_id_with_offset, content_hash, normalize_relative_path};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Identify the source strategy used to create a code Chunk.
/// 标识代码块使用的切块策略。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum ChunkKind {
    /// A semantic AST-derived code window.
    /// 基于语义 AST 生成的代码窗口。
    Ast,
    /// A deterministic recursive fallback window.
    /// 基于递归切分生成的确定性回退窗口。
    Recursive,
    /// A path-document window used by the auxiliary path index.
    /// 用于辅助路径索引的路径文档窗口。
    Path,
}

/// Identify how a workspace file entered the local snapshot.
/// 标识工作区文件进入本地快照的来源。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum FileSource {
    /// A file read from the filesystem.
    /// 从文件系统读取的文件。
    Filesystem,
    /// An unsaved editor or caller-provided overlay.
    /// 未保存编辑器内容或调用方提供的覆盖层。
    Explicit,
}

/// Identify the current state of a workspace file.
/// 标识工作区文件的当前状态。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum FileStatus {
    /// The file is eligible for indexing and retrieval.
    /// 文件可进入索引和检索。
    Present,
    /// The file was intentionally skipped by admission rules.
    /// 文件因准入规则被主动跳过。
    Skipped,
    /// The file is absent or explicitly removed from the snapshot.
    /// 文件已不存在或被显式从快照中移除。
    Deleted,
}

/// A source range using one-based inclusive line numbers.
/// 使用从一开始且闭区间行号的源码范围。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct SourceRange {
    /// The first source line included in the range.
    /// 范围包含的第一行源码行号。
    pub start_line: usize,
    /// The last source line included in the range.
    /// 范围包含的最后一行源码行号。
    pub end_line: usize,
}

impl SourceRange {
    /// Create a validated one-based inclusive source range.
    /// 创建经过校验的从一开始闭区间源码范围。
    pub fn new(start_line: usize, end_line: usize) -> Result<Self, ChunkError> {
        if start_line == 0 || end_line < start_line {
            return Err(ChunkError::InvalidRange {
                start_line,
                end_line,
            });
        }
        Ok(Self {
            start_line,
            end_line,
        })
    }

    /// Return the fraction of this range overlapped by another range.
    /// 返回当前范围被另一个范围覆盖的比例。
    pub fn overlap_ratio(self, other: Self) -> f32 {
        let overlap_start = self.start_line.max(other.start_line);
        let overlap_end = self.end_line.min(other.end_line);
        if overlap_end < overlap_start {
            return 0.0;
        }
        let overlap = overlap_end.saturating_sub(overlap_start).saturating_add(1) as f32;
        let shorter = self.line_count().min(other.line_count()) as f32;
        if shorter == 0.0 {
            0.0
        } else {
            overlap / shorter
        }
    }

    /// Return the number of inclusive source lines in the range.
    /// 返回闭区间范围包含的源码行数。
    pub fn line_count(self) -> usize {
        self.end_line
            .saturating_sub(self.start_line)
            .saturating_add(1)
    }
}

/// A content-addressed and source-locatable code Chunk.
/// 一个具有内容地址和源码定位信息的代码块。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    /// Stable occurrence identity of this Chunk in its Blob.
    /// 代码块在 Blob 内的稳定出现位置身份。
    pub chunk_id: String,
    /// Content identity shared by equal Chunk text.
    /// 相同代码块文本共享的内容身份。
    pub content_hash: String,
    /// Stable identity of the source file content and path.
    /// 源文件路径和内容的稳定身份。
    pub blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Verbatim source text used for both embedding and rendering.
    /// 同时用于向量化和渲染的原始源码文本。
    pub content: String,
    /// One-based inclusive source range.
    /// 从一开始的闭区间源码范围。
    pub range: SourceRange,
    /// Unicode-scalar offset within the first source line, used only for line-internal fragments.
    /// 首行内的 Unicode 标量偏移，仅用于行内切分片段。
    pub start_char_offset: usize,
    /// Chunking strategy that produced this record.
    /// 生成该记录的切块策略。
    pub kind: ChunkKind,
}

impl Chunk {
    /// Build a stable Chunk record from a Blob, path, and verbatim source range.
    /// 根据 Blob、路径和原始源码范围构建稳定的代码块记录。
    pub fn new(
        blob_name: impl Into<String>,
        path: &str,
        content: impl Into<String>,
        start_line: usize,
        end_line: usize,
        kind: ChunkKind,
    ) -> Result<Self, ChunkError> {
        Self::new_with_start_char_offset(blob_name, 0, path, content, start_line, end_line, kind)
    }

    /// Build a stable Chunk with an explicit Unicode-scalar offset in its first source line.
    /// 使用首行内显式 Unicode 标量偏移构建稳定的代码块。
    pub fn new_with_start_char_offset(
        blob_name: impl Into<String>,
        start_char_offset: usize,
        path: &str,
        content: impl Into<String>,
        start_line: usize,
        end_line: usize,
        kind: ChunkKind,
    ) -> Result<Self, ChunkError> {
        let normalized_path = normalize_relative_path(path).map_err(ChunkError::InvalidPath)?;
        let content = content.into();
        if !content.chars().any(char::is_alphanumeric) {
            return Err(ChunkError::NonSemanticContent);
        }
        let range = SourceRange::new(start_line, end_line)?;
        let source_lines = source_line_count(&content);
        if kind != ChunkKind::Path && source_lines != range.line_count() {
            return Err(ChunkError::LineCountMismatch {
                source_lines,
                range_lines: range.line_count(),
            });
        }
        let blob_name = blob_name.into();
        let content_hash = content_hash(&content);
        let chunk_id = chunk_id_with_offset(
            &blob_name,
            &content_hash,
            range.start_line,
            range.end_line,
            start_char_offset,
        );
        Ok(Self {
            chunk_id,
            content_hash,
            blob_name,
            path: normalized_path,
            content,
            range,
            start_char_offset,
            kind,
        })
    }

    /// Return the exact text sent to the document Embedding operation.
    /// 返回提交给文档向量化操作的精确文本。
    pub fn embedding_text(&self) -> String {
        format!("File: {}\n\n{}", self.path, self.content)
    }

    /// Return a stable key for deduplicating the same source occurrence.
    /// 返回用于去重同一源码出现位置的稳定键。
    pub fn occurrence_key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.blob_name,
            self.path,
            self.range.start_line,
            self.range.end_line,
            self.start_char_offset,
            self.content_hash
        )
    }
}

/// Errors raised while constructing a source-locatable Chunk.
/// 构建带源码定位的代码块时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkError {
    /// The source range is not a valid one-based inclusive range.
    /// 源码范围不是有效的从一开始闭区间范围。
    InvalidRange {
        /// Requested one-based inclusive start line.
        /// 请求的从一开始闭区间起始行。
        start_line: usize,
        /// Requested one-based inclusive end line.
        /// 请求的从一开始闭区间结束行。
        end_line: usize,
    },
    /// The source path is not a valid workspace-relative path.
    /// 源码路径不是有效的工作区相对路径。
    InvalidPath(PathError),
    /// The source fragment contains no letter or number and is not retrievable code content.
    /// 源码片段不包含任何字母或数字，不能作为可检索代码内容。
    NonSemanticContent,
    /// The source text line count disagrees with its declared source range.
    /// 源码文本行数与声明的源码范围不一致。
    LineCountMismatch {
        /// Number of physical source lines represented by the text.
        /// 文本表示的物理源码行数。
        source_lines: usize,
        /// Number of lines represented by the declared range.
        /// 声明范围表示的行数。
        range_lines: usize,
    },
}

impl Display for ChunkError {
    /// Formats the Chunk construction error for diagnostics.
    /// 将代码块构建错误格式化为诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange {
                start_line,
                end_line,
            } => write!(
                formatter,
                "invalid source range {start_line}:{end_line}; expected one-based inclusive range"
            ),
            Self::InvalidPath(error) => write!(formatter, "invalid Chunk path: {error}"),
            Self::NonSemanticContent => {
                formatter.write_str("Chunk content must contain at least one letter or number")
            }
            Self::LineCountMismatch {
                source_lines,
                range_lines,
            } => write!(
                formatter,
                "Chunk contains {source_lines} source lines but declares a {range_lines}-line range"
            ),
        }
    }
}

impl Error for ChunkError {}

/// Count physical source lines while treating a trailing terminator as part of its line.
/// 统计物理源码行，并将尾部终止符视为其所在行的一部分。
pub(crate) fn source_line_count(content: &str) -> usize {
    content.split_inclusive('\n').count()
}

/// Identify the retrieval channel that produced a SearchHit.
/// 标识检索命中的来源通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchSource {
    /// A dense vector similarity index.
    /// 稠密向量相似度索引。
    Dense,
    /// An exact identifier or symbol index.
    /// 精确标识符或符号索引。
    Exact,
    /// An auxiliary path semantic index.
    /// 辅助路径语义索引。
    Path,
    /// A result produced after merging multiple channels.
    /// 多个通道融合后的结果。
    Fused,
}

/// Identify the semantic kind of a retrieval hit.
/// 标识检索命中的语义类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitKind {
    /// A general content chunk.
    /// 普通内容代码块。
    Content,
    /// A normal symbol definition.
    /// 普通符号定义。
    Definition,
    /// A command or route endpoint definition.
    /// 命令或路由入口定义。
    Endpoint,
    /// A symbol reference or usage occurrence.
    /// 符号引用或使用位置。
    Reference,
}

/// A structured retrieval result before final context rendering.
/// 在最终上下文渲染前保留的结构化检索结果。
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Stable occurrence identity used for fusion and deduplication.
    /// 用于融合和去重的稳定出现位置身份。
    pub id: String,
    /// Blob identity containing this occurrence.
    /// 包含该出现位置的 Blob 身份。
    pub blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// One-based inclusive source range.
    /// 从一开始的闭区间源码范围。
    pub range: SourceRange,
    /// Unicode-scalar offset within the first source line for line-internal fragments.
    /// 行内切分片段在首行内的 Unicode 标量偏移。
    pub start_char_offset: usize,
    /// Verbatim source text for the hit.
    /// 命中的原始源码文本。
    pub content: String,
    /// Content identity of the hit text.
    /// 命中文本的内容身份。
    pub content_hash: String,
    /// Current ranking score.
    /// 当前排序分数。
    pub score: f32,
    /// Retrieval channel that produced the current score.
    /// 生成当前分数的检索通道。
    pub source: SearchSource,
    /// Semantic kind used by endpoint promotion and rendering policies.
    /// 用于入口提升和渲染策略的语义类型。
    pub kind: HitKind,
}

impl SearchHit {
    /// Create a SearchHit from a previously indexed Chunk.
    /// 根据已索引的代码块创建结构化检索命中。
    pub fn from_chunk(chunk: &Chunk, score: f32, source: SearchSource, kind: HitKind) -> Self {
        Self {
            id: chunk.chunk_id.clone(),
            blob_name: chunk.blob_name.clone(),
            path: chunk.path.clone(),
            range: chunk.range,
            start_char_offset: chunk.start_char_offset,
            content: chunk.content.clone(),
            content_hash: chunk.content_hash.clone(),
            score,
            source,
            kind,
        }
    }

    /// Return the source-occurrence key used by RRF and deduplication.
    /// 返回 RRF 和去重使用的源码出现位置键。
    pub fn occurrence_key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.blob_name,
            self.path,
            self.range.start_line,
            self.range.end_line,
            self.start_char_offset,
            self.content_hash
        )
    }

    /// Return the number of Unicode scalar values in the hit content.
    /// 返回命中文本包含的 Unicode 标量值数量。
    pub fn char_len(&self) -> usize {
        self.content.chars().count()
    }
}

/// A local file record used by the pure synchronization state machine.
/// 纯同步状态机使用的本地文件记录。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileRecord {
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Current content-addressed Blob identity, when present.
    /// 当前内容地址化 Blob 身份（如果存在）。
    pub blob_name: Option<String>,
    /// Last Blob identity committed to a checkpoint.
    /// 最近一次提交到 checkpoint 的 Blob 身份。
    pub committed_blob_name: Option<String>,
    /// Current admission status.
    /// 当前准入状态。
    pub status: FileStatus,
    /// File size in bytes at observation time.
    /// 观察时的文件字节大小。
    pub size: u64,
    /// File modification timestamp in nanoseconds, when available.
    /// 文件修改时间纳秒值（如果可用）。
    pub mtime_ns: Option<u128>,
    /// Source of the current record.
    /// 当前记录的来源。
    pub source: FileSource,
    /// Verbatim text retained for indexing or explicit overlays.
    /// 为索引或显式覆盖层保留的原始文本。
    pub content: Option<String>,
    /// Local snapshot generation that produced this record.
    /// 生成该记录的本地快照代数。
    pub generation: u64,
    /// Why the record was skipped, when status is Skipped.
    /// 当状态为跳过时记录跳过原因。
    pub skip_reason: Option<String>,
}

impl FileRecord {
    /// Return true when this record can participate in indexing and retrieval.
    /// 判断该记录是否可以参与索引和检索。
    pub fn is_present(&self) -> bool {
        self.status == FileStatus::Present && self.blob_name.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that Chunk text is used verbatim for Embedding input.
    /// 验证向量化输入使用代码块原始文本。
    #[test]
    fn chunk_embedding_text_keeps_path_context() {
        let chunk = Chunk::new(
            "blob",
            "src/lib.rs",
            "fn main() {}",
            2,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        assert_eq!(chunk.embedding_text(), "File: src/lib.rs\n\nfn main() {}");
    }

    /// Verify inclusive range overlap is bounded by the shorter range.
    /// 验证闭区间范围重叠比例以较短范围为分母。
    #[test]
    fn source_range_overlap_is_inclusive() {
        let first = SourceRange::new(1, 4).unwrap();
        let second = SourceRange::new(3, 6).unwrap();
        assert!((first.overlap_ratio(second) - 0.5).abs() < f32::EPSILON);
    }

    /// Verify externally supplied maximum line numbers cannot overflow range calculations.
    /// 验证外部提供的最大行号不会使区间计算溢出。
    #[test]
    fn source_range_extremes_remain_saturating() {
        let range = SourceRange::new(1, usize::MAX).unwrap();
        assert_eq!(range.line_count(), usize::MAX);
        assert_eq!(range.overlap_ratio(range), 1.0);
    }

    /// Verify standalone Chunk construction enforces the semantic admission rule.
    /// 验证独立代码块构造会强制执行语义准入规则。
    #[test]
    fn chunk_rejects_non_semantic_content() {
        assert_eq!(
            Chunk::new("blob", "src/lib.rs", "{}[]", 1, 1, ChunkKind::Recursive),
            Err(ChunkError::NonSemanticContent)
        );
    }

    /// Verify source chunks cannot claim a line range inconsistent with their exact text.
    /// 验证源码块不能声明与其精确文本不一致的行范围。
    #[test]
    fn source_chunk_rejects_inconsistent_line_count() {
        assert_eq!(
            Chunk::new(
                "blob",
                "src/lib.rs",
                "fn one() {}\nfn two() {}",
                1,
                1,
                ChunkKind::Recursive,
            ),
            Err(ChunkError::LineCountMismatch {
                source_lines: 2,
                range_lines: 1,
            })
        );
    }
}

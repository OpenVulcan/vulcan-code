//! Structured V1 session storage errors.
//! V1 会话存储的结构化错误。
//!
//! Every error carries the session id, the exact file path, the segment or
//! generation context, a line or byte position when available, expected and
//! actual values, and a stable machine-readable code. The loader never
//! truncates, reorders, backfills, rewrites, or degrades.
//! 每个错误都携带会话 id、精确文件路径、分片或 generation 上下文、可得时的
//! 行号或字节位置、期望值与实际值，以及稳定机器可读错误码。加载器绝不截断、
//! 重排、补写、重写或降级。

use std::fmt;
use std::path::PathBuf;

/// Stable machine-readable code of one storage error.
/// 一个存储错误的稳定机器可读错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// session missing
    /// 会话缺失
    SessionMissing,
    /// unsupported development format
    /// 不受支持的开发格式
    UnsupportedDevelopmentFormat,
    /// metadata corrupt
    /// 元数据损坏
    MetadataCorrupt,
    /// segment missing
    /// 分片缺失
    SegmentMissing,
    /// segment corrupt
    /// 分片损坏
    SegmentCorrupt,
    /// A replay checkpoint digest matches no supported fingerprint format.
    /// 重放检查点摘要不匹配任何受支持的指纹格式。
    ProviderSourceFingerprintMismatch,
    /// sequence discontinuity
    /// 序号不连续
    SequenceDiscontinuity,
    /// reference violation
    /// 引用违反
    ReferenceViolation,
    /// runtime corrupt
    /// 运行时锁定损坏
    RuntimeCorrupt,
    /// compaction corrupt
    /// 压缩节点损坏
    CompactionCorrupt,
    /// atomic write failed
    /// 原子写失败
    AtomicWriteFailed,
    /// io failure
    /// IO 失败
    IoFailure,
}

impl ErrorCode {
    /// Returns the stable lowercase code string.
    /// 返回稳定的小写错误码字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionMissing => "session_missing",
            Self::UnsupportedDevelopmentFormat => "unsupported_development_format",
            Self::MetadataCorrupt => "metadata_corrupt",
            Self::SegmentMissing => "segment_missing",
            Self::SegmentCorrupt => "segment_corrupt",
            Self::ProviderSourceFingerprintMismatch => "provider_source_fingerprint_mismatch",
            Self::SequenceDiscontinuity => "sequence_discontinuity",
            Self::ReferenceViolation => "reference_violation",
            Self::RuntimeCorrupt => "runtime_corrupt",
            Self::CompactionCorrupt => "compaction_corrupt",
            Self::AtomicWriteFailed => "atomic_write_failed",
            Self::IoFailure => "io_failure",
        }
    }
}

/// Heap-owned details of one structured V1 storage error.
/// 一条结构化 V1 存储错误的堆上详情。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1ErrorDetails {
    /// Owning session id when known.
    /// 已知时的所属会话 id。
    pub session_id: Option<String>,
    /// Exact file path involved.
    /// 涉及文件的精确路径。
    pub path: Option<PathBuf>,
    /// Segment/generation context when relevant.
    /// 相关时的分片/generation 上下文。
    pub context: Option<String>,
    /// 1-based line number when relevant.
    /// 相关时的 1-based 行号。
    pub line: Option<u64>,
    /// 0-based byte offset of the failing position when known.
    /// 已知时的失败位置 0-based 字节偏移。
    pub byte_offset: Option<u64>,
    /// Expected value description.
    /// 期望值描述。
    pub expected: Option<String>,
    /// Actual value description.
    /// 实际值描述。
    pub actual: Option<String>,
    /// Human-readable detail.
    /// 人类可读详情。
    pub detail: String,
}

/// One compact structured V1 storage error handle.
/// 一条紧凑的结构化 V1 存储错误句柄。
///
/// The diagnostic payload is boxed once so every `Result<T, SessionV1Error>`
/// keeps a small error variant without boxing individual call sites.
/// 诊断载荷统一装箱一次，使每个 `Result<T, SessionV1Error>` 都保持较小的错误分支，
/// 无需让各调用点分别装箱。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1Error {
    /// Stabilized error code kept inline for cheap classification and matching.
    /// 内联保留的稳定错误码，用于低成本分类与模式匹配。
    pub code: ErrorCode,
    /// Complete structured diagnostic payload.
    /// 完整结构化诊断载荷。
    pub(crate) details: Box<SessionV1ErrorDetails>,
}

impl std::ops::Deref for SessionV1Error {
    type Target = SessionV1ErrorDetails;

    /// Exposes immutable structured fields without expanding the outer error value.
    /// 暴露不可变结构化字段，同时不扩大外层错误值。
    fn deref(&self) -> &Self::Target {
        &self.details
    }
}

impl std::ops::DerefMut for SessionV1Error {
    /// Exposes mutable structured fields to context-enrichment helpers.
    /// 向上下文补充辅助方法暴露可变结构化字段。
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.details
    }
}

impl SessionV1Error {
    /// Wraps one complete structured diagnostic payload.
    /// 包装一份完整结构化诊断载荷。
    pub fn from_details(code: ErrorCode, details: SessionV1ErrorDetails) -> Self {
        Self {
            code,
            details: Box::new(details),
        }
    }

    /// One missing session.
    /// 一条会话缺失错误。
    pub fn session_missing(session_id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::from_details(
            ErrorCode::SessionMissing,
            SessionV1ErrorDetails {
                session_id: Some(session_id.into()),
                path: Some(path.into()),
                context: None,
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: "session directory or metadata.json does not exist".to_string(),
            },
        )
    }

    /// One unsupported development format error.
    /// 一条不受支持的开发格式错误。
    pub fn unsupported(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::from_details(
            ErrorCode::UnsupportedDevelopmentFormat,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: None,
                line: None,
                byte_offset: None,
                expected: Some("v1 contract".to_string()),
                actual: None,
                detail: detail.into(),
            },
        )
    }

    /// One corrupt document error (metadata/runtime/compactions).
    /// 一条文档损坏错误(metadata/runtime/compactions)。
    pub fn corrupt(code: ErrorCode, path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::from_details(
            code,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: None,
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: detail.into(),
            },
        )
    }

    /// One segment parse error with line and byte position.
    /// 一条带行号的分片解析错误。
    pub fn segment_corrupt(
        path: impl Into<PathBuf>,
        segment_number: u64,
        line: u64,
        byte_offset: Option<u64>,
        detail: impl Into<String>,
    ) -> Self {
        Self::from_details(
            ErrorCode::SegmentCorrupt,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: Some(format!("segment s{segment_number:07}")),
                line: Some(line),
                byte_offset,
                expected: None,
                actual: None,
                detail: detail.into(),
            },
        )
    }

    /// One missing segment error.
    /// 一条分片缺失错误。
    pub fn segment_missing(path: impl Into<PathBuf>, segment_number: u64) -> Self {
        Self::from_details(
            ErrorCode::SegmentMissing,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: Some(format!("segment s{segment_number:07}")),
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: "split_list references a segment file that does not exist".to_string(),
            },
        )
    }

    /// One sequence discontinuity error.
    /// 一条序号不连续错误。
    pub fn discontinuity(
        path: impl Into<PathBuf>,
        context: impl Into<String>,
        expected: impl fmt::Display,
        actual: impl fmt::Display,
    ) -> Self {
        Self::from_details(
            ErrorCode::SequenceDiscontinuity,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: Some(context.into()),
                line: None,
                byte_offset: None,
                expected: Some(expected.to_string()),
                actual: Some(actual.to_string()),
                detail: "expected and actual seq do not match".to_string(),
            },
        )
    }

    /// One atomic write failure at a named stage.
    /// 一条命名阶段的原子写失败。
    pub fn atomic_write(path: impl Into<PathBuf>, stage: &str, detail: impl Into<String>) -> Self {
        Self::from_details(
            ErrorCode::AtomicWriteFailed,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: Some(format!("stage {stage}")),
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: detail.into(),
            },
        )
    }

    /// One generic IO failure.
    /// 一条通用 IO 失败。
    pub fn io(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::from_details(
            ErrorCode::IoFailure,
            SessionV1ErrorDetails {
                session_id: None,
                path: Some(path.into()),
                context: None,
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: detail.into(),
            },
        )
    }

    /// Attaches one session id and path to an error missing them.
    /// 为缺失它们的错误附加会话 id 与路径。
    pub fn with_context(mut self, session_id: &str, path: impl Into<PathBuf>) -> Self {
        if self.session_id.is_none() {
            self.session_id = Some(session_id.to_string());
        }
        if self.path.is_none() {
            self.path = Some(path.into());
        }
        self
    }
}

impl fmt::Display for SessionV1Error {
    /// Formats the structured storage error for diagnostics.
    /// 为诊断格式化结构化存储错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.detail)?;
        if let Some(session_id) = &self.session_id {
            write!(formatter, " (session {session_id})")?;
        }
        if let Some(path) = &self.path {
            write!(formatter, " at {}", path.display())?;
        }
        if let Some(context) = &self.context {
            write!(formatter, " [{context}]")?;
        }
        if let Some(line) = self.line {
            write!(formatter, " line {line}")?;
        }
        if let Some(byte_offset) = self.byte_offset {
            write!(formatter, " byte {byte_offset}")?;
        }
        if let (Some(expected), Some(actual)) = (&self.expected, &self.actual) {
            write!(formatter, " expected {expected}, actual {actual}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SessionV1Error {}

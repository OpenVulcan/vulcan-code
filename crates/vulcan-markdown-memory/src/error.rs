//! Explicit failures of the Markdown memory store.
//! Markdown 记忆存储的明确失败类型。

/// One failure; every message is English and safe to return to the model.
/// 单个失败；每条消息均为英文，可直接返回给模型。
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// The caller supplied an argument that the store cannot accept.
    /// 调用方提供了存储无法接受的参数。
    #[error("{0}")]
    InvalidArgument(String),
    /// The referenced node does not exist in the requested scope.
    /// 引用的节点不存在于请求的层中。
    #[error(
        "Memory node `{0}` was not found; run memory_outline or memory_search to get current ids."
    )]
    NodeNotFound(String),
    /// The node changed since the caller read it.
    /// 节点在调用方读取后已发生变化。
    #[error(
        "Memory node `{id}` changed (expected revision {expected}, current {current}); read it again before writing."
    )]
    RevisionMismatch {
        /// Node identifier.
        /// 节点标识。
        id: String,
        /// Revision supplied by the caller.
        /// 调用方提供的修订。
        expected: String,
        /// Revision currently stored.
        /// 当前存储的修订。
        current: String,
    },
    /// The expected revision differs from the batch state, which differs from the committed snapshot.
    /// 预期修订与批内状态不符，且批内状态与已提交快照不同。
    #[error(
        "Memory node `{id}` has staged revision {staged} in this batch (expected {expected}). Committed revision before the batch: {}. Submit dependent edits in separate calls and read the node after each successful write.",
        .committed.as_deref().unwrap_or("none for this node ID")
    )]
    StagedRevisionMismatch {
        /// Node identifier used by the operation.
        /// 当前操作使用的节点标识。
        id: String,
        /// Revision supplied by the caller.
        /// 调用方提供的修订。
        expected: String,
        /// Uncommitted revision used for the operation's check.
        /// 当前操作用于校验的未提交修订。
        staged: String,
        /// Revision of this node ID in the original files, if present.
        /// 此节点标识在原始文件中的修订，可能不存在。
        committed: Option<String>,
    },
    /// The project scope is unavailable for this request.
    /// 当前请求无法使用项目层。
    #[error("{0}")]
    ProjectUnavailable(String),
    /// A persisted file uses a format this build does not read.
    /// 持久文件使用了当前版本不读取的格式。
    #[error("{0}")]
    UnsupportedFormat(String),
    /// A memory file was edited outside Vulcan while a write was being prepared.
    /// 准备写入期间记忆文件在 Vulcan 之外被修改。
    #[error("Memory file `{0}` changed on disk while the write was prepared; retry the write.")]
    ConcurrentModification(String),
    /// A failed commit could not restore every original; recovery copies are preserved on disk.
    /// 提交失败后无法还原所有原文；恢复副本保留在磁盘上。
    #[error(
        "Memory commit failed: {source}. Rollback is incomplete for {inconsistent_files:?}. Original recovery files: {recovery_files:?}. Restore these files before retrying."
    )]
    RollbackIncomplete {
        /// Original failure that stopped publication.
        /// 阻止发布的原始错误。
        source: Box<MemoryError>,
        /// Destinations that could not be restored without losing another writer's changes.
        /// 无法在保留其他写入方修改的前提下还原的目标文件。
        inconsistent_files: Vec<std::path::PathBuf>,
        /// Exact original-byte copies retained for manual recovery; absent originals need no copy.
        /// 为人工恢复保留的精确原文字节副本；原先不存在的文件无需副本。
        recovery_files: Vec<std::path::PathBuf>,
    },
    /// Reading or writing a memory file failed.
    /// 读取或写入记忆文件失败。
    #[error("Memory file `{path}` could not be {action}: {source}")]
    Io {
        /// File path.
        /// 文件路径。
        path: String,
        /// Attempted action in past participle form.
        /// 以过去分词形式描述的尝试动作。
        action: &'static str,
        /// Underlying I/O error.
        /// 底层 I/O 错误。
        source: std::io::Error,
    },
    /// One operation of a multi-operation write failed; nothing was written.
    /// 多操作写入中的某个操作失败；未写入任何内容。
    #[error("Operation #{index} (counting from 1) failed, so no change was written: {source}")]
    Operation {
        /// One-based operation index.
        /// 从一开始的操作序号。
        index: usize,
        /// Failure of that operation.
        /// 该操作的失败。
        source: Box<MemoryError>,
    },
    /// A JSON file could not be parsed or produced.
    /// JSON 文件无法解析或生成。
    #[error("Memory file `{path}` is not valid JSON: {message}")]
    Json {
        /// File path.
        /// 文件路径。
        path: String,
        /// Parser message.
        /// 解析器消息。
        message: String,
    },
}

impl MemoryError {
    /// Builds an invalid-argument error from any message.
    /// 由任意消息构建参数无效错误。
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }

    /// Builds an I/O error for one path and action.
    /// 为一个路径与动作构建 I/O 错误。
    pub(crate) fn io(path: &std::path::Path, action: &'static str, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            action,
            source,
        }
    }
}

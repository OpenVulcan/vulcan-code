//! Structured LanceDB engine errors.
//! LanceDB 引擎结构化错误。
//!
//! The engine reports local, typed failures rather than transport status codes. Callers must be
//! able to distinguish database open failure, path conflict, lock contention, schema violation,
//! table-not-found, and compaction-state failures without string matching.
//!
//! 引擎返回本地类型化失败，而不是传输状态码。调用方必须能够在不做字符串匹配的前提下区分
//! 数据库打开失败、路径冲突、锁占用、Schema 违规、表不存在与整理状态失败。

use std::path::PathBuf;

/// Result alias for the LanceDB engine.
/// LanceDB 引擎的结果别名。
pub type EngineResult<T> = Result<T, LanceDbEngineError>;

/// Structured failure returned by the in-process LanceDB engine.
/// 进程内 LanceDB 引擎返回的结构化失败。
#[derive(Debug, thiserror::Error)]
pub enum LanceDbEngineError {
    /// The caller supplied an invalid argument.
    /// 调用方提供了无效参数。
    #[error("invalid LanceDB argument: {detail}")]
    InvalidArgument {
        /// Human-readable validation failure description.
        /// 校验失败的可读描述。
        detail: String,
    },

    /// The database could not be opened, or its directory could not be prepared.
    /// 数据库无法打开，或其目录无法准备。
    #[error("failed to open LanceDB database at {path}: {detail}")]
    Open {
        /// Normalized database path that failed to open.
        /// 打开失败的规范化数据库路径。
        path: PathBuf,
        /// Human-readable open failure description.
        /// 打开失败的可读描述。
        detail: String,
    },

    /// A path escaped its authorized root.
    /// 路径逃逸出授权根。
    #[error("LanceDB path is not authorized: {detail}")]
    PathNotAuthorized {
        /// Human-readable refusal description.
        /// 拒绝的可读描述。
        detail: String,
    },

    /// Another process or handle currently owns the database root lock.
    /// 另一个进程或句柄当前持有该数据库根锁。
    #[error("LanceDB database root lock is held at {lock_path} ({owner})")]
    LockHeld {
        /// Lock file path that is currently held.
        /// 当前被持有的锁文件路径。
        lock_path: PathBuf,
        /// Recorded owner identity from the lock payload.
        /// 来自锁载荷的持有者身份。
        owner: String,
    },

    /// Lock acquisition failed for a real I/O reason rather than contention.
    /// 因真实 I/O 原因而非竞争导致加锁失败。
    #[error("failed to acquire LanceDB database root lock at {lock_path}: {source}")]
    LockIo {
        /// Lock file path involved in the failure.
        /// 失败涉及的锁文件路径。
        lock_path: PathBuf,
        /// Underlying I/O failure.
        /// 底层 I/O 失败。
        source: std::io::Error,
    },

    /// The requested table does not exist.
    /// 请求的表不存在。
    #[error("LanceDB table does not exist: {table_name}")]
    TableNotFound {
        /// Table name that could not be resolved.
        /// 无法解析的表名。
        table_name: String,
    },

    /// The supplied schema violates a v1 schema invariant.
    /// 提供的 Schema 违反 v1 Schema 不变量。
    #[error("LanceDB schema is not allowed: {detail}")]
    SchemaNotAllowed {
        /// Human-readable schema refusal description.
        /// 拒绝 Schema 的可读描述。
        detail: String,
    },

    /// A compaction request or observed transition was inconsistent.
    /// 整理请求或观测到的状态转移不一致。
    #[error("LanceDB compaction failed: {detail}")]
    Compaction {
        /// Human-readable compaction failure description.
        /// 整理失败的可读描述。
        detail: String,
    },

    /// An internal invariant failed.
    /// 内部不变量失败。
    #[error("internal LanceDB engine failure: {detail}")]
    Internal {
        /// Human-readable internal failure description.
        /// 内部失败的可读描述。
        detail: String,
    },
}

impl LanceDbEngineError {
    /// Builds one invalid-argument failure from a message.
    /// 由消息构造一次无效参数失败。
    pub fn invalid_argument(detail: impl Into<String>) -> Self {
        Self::InvalidArgument {
            detail: detail.into(),
        }
    }

    /// Builds one database-open failure, keeping the failing path for diagnostics.
    /// 由消息构造一次数据库打开失败，并保留失败路径以供诊断。
    pub fn open(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::Open {
            path: path.into(),
            detail: detail.into(),
        }
    }

    /// Builds one path-authorization refusal from a message.
    /// 由消息构造一次路径授权拒绝。
    pub fn path_not_authorized(detail: impl Into<String>) -> Self {
        Self::PathNotAuthorized {
            detail: detail.into(),
        }
    }

    /// Builds one table-not-found failure for a table name.
    /// 由表名构造一次表不存在失败。
    pub fn table_not_found(table_name: impl Into<String>) -> Self {
        Self::TableNotFound {
            table_name: table_name.into(),
        }
    }

    /// Builds one compaction failure from a message.
    /// 由消息构造一次整理失败。
    pub fn compaction(detail: impl Into<String>) -> Self {
        Self::Compaction {
            detail: detail.into(),
        }
    }

    /// Builds one internal failure from a message.
    /// 由消息构造一次内部失败。
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal {
            detail: detail.into(),
        }
    }

    /// Builds one schema refusal from a message.
    /// 由消息构造一次 Schema 拒绝。
    pub fn schema_not_allowed(detail: impl Into<String>) -> Self {
        Self::SchemaNotAllowed {
            detail: detail.into(),
        }
    }
}

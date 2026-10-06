//! Structured SQLite engine errors.
//! SQLite 引擎结构化错误。
//!
//! The engine reports local, typed failures rather than transport status codes. Callers must be
//! able to distinguish database open failure, path conflict, lock contention, SQL failure,
//! serialization failure and cancel/close state without string matching.
//!
//! 引擎返回本地类型化失败，而不是传输状态码。调用方必须能够在不做字符串匹配的前提下区分
//! 数据库打开失败、路径冲突、锁占用、SQL 失败、序列化失败与取消／关闭状态。

use std::path::PathBuf;

/// Result alias for the SQLite engine.
/// SQLite 引擎的结果别名。
pub type EngineResult<T> = Result<T, SqliteEngineError>;

/// Structured failure returned by the in-process SQLite engine.
/// 进程内 SQLite 引擎返回的结构化失败。
#[derive(Debug, thiserror::Error)]
pub enum SqliteEngineError {
    /// The caller supplied an invalid argument, such as an empty SQL string, a malformed index
    /// name or a non-scalar JSON parameter.
    /// 调用方提供了无效参数，例如空 SQL 字符串、非法索引名或非标量 JSON 参数。
    #[error("invalid SQLite engine argument: {detail}")]
    InvalidArgument {
        /// Human-readable validation failure description.
        /// 校验失败的可读描述。
        detail: String,
    },

    /// The database could not be opened, or its directory could not be prepared.
    /// 数据库无法打开，或其目录无法准备。
    #[error("failed to open SQLite database at {path}: {source}")]
    Open {
        /// Normalized database path that failed to open.
        /// 打开失败的规范化数据库路径。
        path: PathBuf,
        /// Underlying I/O or SQLite failure.
        /// 底层 I/O 或 SQLite 失败。
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },

    /// The same logical resource was bound to two different physical paths, or one physical path
    /// was opened twice with conflicting options.
    /// 同一逻辑资源绑定到两个不同物理路径，或同一物理路径以冲突参数重复打开。
    #[error("SQLite resource conflict for {path}: {detail}")]
    ResourceConflict {
        /// Normalized database path involved in the conflict.
        /// 冲突涉及的规范化数据库路径。
        path: PathBuf,
        /// Human-readable conflict description.
        /// 冲突的可读描述。
        detail: String,
    },

    /// A path escaped its authorized root, or a URI form was supplied where a local path is
    /// required.
    /// 路径逃逸出授权根，或在要求本地路径处提供了 URI 形式。
    #[error("SQLite path is not authorized: {detail}")]
    PathNotAuthorized {
        /// Human-readable refusal description.
        /// 拒绝的可读描述。
        detail: String,
    },

    /// Another process or handle currently owns the database lock.
    /// 另一个进程或句柄当前持有该数据库锁。
    #[error("SQLite database lock is held at {lock_path} ({owner})")]
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
    #[error("failed to acquire SQLite database lock at {lock_path}: {source}")]
    LockIo {
        /// Lock file path involved in the failure.
        /// 失败涉及的锁文件路径。
        lock_path: PathBuf,
        /// Underlying I/O failure.
        /// 底层 I/O 失败。
        source: std::io::Error,
    },

    /// SQL compilation or execution failed.
    /// SQL 编译或执行失败。
    #[error("SQLite statement failed: {source}")]
    Sql {
        /// Underlying rusqlite failure.
        /// 底层 rusqlite 失败。
        source: rusqlite::Error,
    },

    /// A batch item failed, so the whole batch was rolled back.
    /// 批量中的某项失败，因此整个批次已回滚。
    #[error("SQLite batch item {index} failed and the batch was rolled back: {source}")]
    BatchItem {
        /// Zero-based index of the failing batch item.
        /// 失败批次的零基下标。
        index: usize,
        /// Underlying rusqlite failure for that item.
        /// 该项的底层 rusqlite 失败。
        source: rusqlite::Error,
    },

    /// Parameter or result serialization failed.
    /// 参数或结果序列化失败。
    #[error("SQLite value serialization failed: {detail}")]
    Serialization {
        /// Human-readable serialization failure description.
        /// 序列化失败的可读描述。
        detail: String,
    },

    /// A query stream was already closed, cancelled, or exceeded its resource limit.
    /// 查询流已关闭、已取消或超出资源上限。
    #[error("SQLite query stream {stream_id} is not available: {state}")]
    QueryStreamState {
        /// Identifier of the affected query stream.
        /// 受影响查询流的标识。
        stream_id: u64,
        /// Terminal state description such as closed, cancelled or limit reached.
        /// 终态描述，例如已关闭、已取消或已达上限。
        state: String,
    },

    /// A query result crossed a caller-supplied row or byte limit and was abandoned mid-read.
    /// 查询结果越过调用方提供的行数或字节上限，已在读取途中放弃。
    #[error("SQLite query result exceeded its limit: {detail}")]
    ResultLimitExceeded {
        /// Human-readable description naming the crossed limit.
        /// 指明被越过上限的可读描述。
        detail: String,
    },

    /// A requested tokenizer mode or dictionary entry was invalid.
    /// 请求的分词模式或词典项无效。
    #[error("SQLite tokenizer request is invalid: {detail}")]
    Tokenizer {
        /// Human-readable validation failure description.
        /// 校验失败的可读描述。
        detail: String,
    },
}

impl SqliteEngineError {
    /// Wraps one rusqlite failure as a SQL error.
    /// 将一次 rusqlite 失败包装为 SQL 错误。
    pub fn sql(source: rusqlite::Error) -> Self {
        Self::Sql { source }
    }

    /// Builds one invalid-argument failure from a message.
    /// 由消息构造一次无效参数失败。
    pub fn invalid_argument(detail: impl Into<String>) -> Self {
        Self::InvalidArgument {
            detail: detail.into(),
        }
    }

    /// Builds one serialization failure from a message.
    /// 由消息构造一次序列化失败。
    pub fn serialization(detail: impl Into<String>) -> Self {
        Self::Serialization {
            detail: detail.into(),
        }
    }

    /// Builds one result-limit failure from a message.
    /// 由消息构造一次结果上限失败。
    pub fn result_limit_exceeded(detail: impl Into<String>) -> Self {
        Self::ResultLimitExceeded {
            detail: detail.into(),
        }
    }

    /// Builds one tokenizer validation failure from a message.
    /// 由消息构造一次分词器校验失败。
    pub fn tokenizer(detail: impl Into<String>) -> Self {
        Self::Tokenizer {
            detail: detail.into(),
        }
    }

    /// Reports whether SQLite itself answered busy or locked, so the same work may succeed later.
    /// 报告 SQLite 本身是否回复忙或已锁定，即同一工作稍后可能成功。
    ///
    /// The decision reads SQLite's result code and never the rendered message.
    /// 判定读取 SQLite 结果码，绝不读取渲染后的消息。
    pub fn is_busy(&self) -> bool {
        match self {
            Self::Sql { source } | Self::BatchItem { source, .. } => rusqlite_is_busy(source),
            Self::Open { source, .. } => source
                .downcast_ref::<rusqlite::Error>()
                .is_some_and(rusqlite_is_busy),
            _ => false,
        }
    }
}

/// Matches the two SQLite result codes that mean another connection holds the database.
/// 匹配表示另一连接持有数据库的两个 SQLite 结果码。
fn rusqlite_is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second connection blocked by an exclusive writer is busy; a malformed statement is not.
    /// 被独占写入者阻塞的第二个连接属于忙；格式错误的语句则不属于。
    #[test]
    fn busy_is_decided_by_the_sqlite_result_code() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("busy.sqlite");
        let holder = rusqlite::Connection::open(&path).expect("open holder connection");
        holder
            .execute_batch("CREATE TABLE t(v INTEGER); BEGIN EXCLUSIVE;")
            .expect("hold the database exclusively");
        let contender = rusqlite::Connection::open(&path).expect("open contending connection");
        contender
            .busy_timeout(std::time::Duration::ZERO)
            .expect("fail immediately on contention");

        let blocked = contender
            .execute("INSERT INTO t(v) VALUES (1)", [])
            .expect_err("the exclusive holder must block the write");
        assert!(SqliteEngineError::sql(blocked).is_busy());

        let malformed = contender
            .execute("NOT A STATEMENT", [])
            .expect_err("malformed SQL must fail");
        assert!(!SqliteEngineError::sql(malformed).is_busy());
        assert!(!SqliteEngineError::invalid_argument("empty").is_busy());
    }
}

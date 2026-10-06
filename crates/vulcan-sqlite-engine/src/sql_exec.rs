//! Local SQL value types, script/batch/query execution and Arrow IPC chunk streaming.
//! 本地 SQL 值类型、脚本／批量／查询执行与 Arrow IPC 分块流式输出。
//!
//! This module is where the migrated source's protocol coupling is removed. The upstream
//! implementation carried `ProtoSqliteValue` and `ExecuteBatchItem` — `tonic`/`prost` generated
//! types — through its parameter and result path, so every call performed a
//! `Value → Protobuf → Value` round trip. Here the boundary carries [`SqliteValue`], a local
//! scalar type, and no Protobuf type exists anywhere in the crate.
//!
//! 本模块是拆除迁入源码协议耦合的地方。上游实现的参数与结果路径上携带 `ProtoSqliteValue` 与
//! `ExecuteBatchItem`（`tonic`/`prost` 生成类型），因此每次调用都要做 `Value → Protobuf → Value`
//! 往返。此处边界携带本地标量类型 [`SqliteValue`]，且本 crate 中不存在任何 Protobuf 类型。
//!
//! JSON handling / JSON 处理:
//!
//! JSON remains because the LuaSkills boundary and knowledge-base callers speak JSON. It is now a
//! **boundary convenience** rather than an internal transport: [`SqliteValue::from_json`] converts
//! an incoming scalar and [`SqliteValue::to_json`] converts a returned value, with no intermediate
//! encoding step.
//!
//! JSON 仍然保留，因为 LuaSkills 边界与知识库调用方使用 JSON。它现在是**边界便利措施**而不是内部
//! 传输格式：[`SqliteValue::from_json`] 转换传入标量，[`SqliteValue::to_json`] 转换返回值，
//! 中间没有额外编码步骤。

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::{ArrayRef, BinaryBuilder, Float64Builder, Int64Builder, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use rusqlite::types::{ToSql, ToSqlOutput, Value as RusqliteValue, ValueRef};
use rusqlite::{Connection, Error as RusqliteError, TransactionBehavior};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::error::{EngineResult, SqliteEngineError};

/// Default Arrow IPC chunk size, balancing memory against downstream consumption.
/// 默认 Arrow IPC 分块大小，在内存与下游消费之间折中。
pub const DEFAULT_IPC_CHUNK_BYTES: usize = 1024 * 1024;

/// Minimum chunk size accepted by a chunk writer.
/// 分块写入器接受的最小分块大小。
const MIN_CHUNK_BYTES: usize = 64 * 1024;

/// Maximum rows materialized into one Arrow record batch.
/// 单个 Arrow record batch 物化的最大行数。
pub const STREAMING_BATCH_ROWS: usize = 1000;

/// One local SQL scalar value.
/// 一个本地 SQL 标量值。
///
/// This is the crate's own value type, deliberately not `rusqlite::types::Value`. Keeping the
/// host boundary independent of a database driver type means a driver upgrade cannot silently
/// change the host's contract, and the JSON mapping stays under this crate's control.
///
/// 这是本 crate 自己的值类型，刻意不使用 `rusqlite::types::Value`。让宿主边界独立于数据库驱动类型
/// 意味着驱动升级不会静默改变宿主契约，且 JSON 映射始终由本 crate 掌控。
#[derive(Debug, Clone, PartialEq)]
pub enum SqliteValue {
    /// SQL NULL.
    /// SQL 空值。
    Null,
    /// Signed 64-bit integer.
    /// 有符号 64 位整数。
    Integer(i64),
    /// 64-bit floating point.
    /// 64 位浮点数。
    Real(f64),
    /// UTF-8 text.
    /// UTF-8 文本。
    Text(String),
    /// Raw bytes.
    /// 原始字节。
    Blob(Vec<u8>),
}

impl SqliteValue {
    /// Converts one JSON scalar into a SQL value.
    /// 将一个 JSON 标量转换为 SQL 值。
    ///
    /// Only scalars are accepted. Arrays and objects are refused rather than stringified, because
    /// silently turning a structure into text would store a value the caller did not intend.
    ///
    /// 只接受标量。数组与对象会被拒绝而不是转成字符串，因为把结构静默变成文本会存入调用方并未预期的值。
    pub fn from_json(value: JsonValue) -> EngineResult<Self> {
        match value {
            JsonValue::Null => Ok(Self::Null),
            JsonValue::Bool(value) => Ok(Self::Integer(i64::from(value))),
            JsonValue::Number(value) => {
                if let Some(value) = value.as_i64() {
                    Ok(Self::Integer(value))
                } else if let Some(value) = value.as_u64() {
                    i64::try_from(value).map(Self::Integer).map_err(|_| {
                        SqliteEngineError::invalid_argument(
                            "JSON parameter is an unsigned integer larger than the SQLite signed 64-bit range",
                        )
                    })
                } else if let Some(value) = value.as_f64() {
                    Ok(Self::Real(value))
                } else {
                    Err(SqliteEngineError::invalid_argument(
                        "JSON parameter contains an unsupported numeric value",
                    ))
                }
            }
            JsonValue::String(value) => Ok(Self::Text(value)),
            JsonValue::Array(_) | JsonValue::Object(_) => Err(SqliteEngineError::invalid_argument(
                "JSON parameters only support scalars (null, bool, number, string)",
            )),
        }
    }

    /// Parses a JSON array of scalars into a bound-parameter list.
    /// 把标量 JSON 数组解析为绑定参数列表。
    ///
    /// An empty or whitespace-only input yields an empty list, which binds no parameters.
    ///
    /// 空或仅含空白的输入返回空列表，即不绑定任何参数。
    pub fn list_from_json(params_json: &str) -> EngineResult<Vec<Self>> {
        if params_json.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed = serde_json::from_str::<JsonValue>(params_json).map_err(|error| {
            SqliteEngineError::invalid_argument(format!(
                "JSON parameters must be an array of scalar values: {error}"
            ))
        })?;

        let items = parsed.as_array().ok_or_else(|| {
            SqliteEngineError::invalid_argument("JSON parameters must be an array of scalar values")
        })?;

        items
            .iter()
            .cloned()
            .map(Self::from_json)
            .collect::<EngineResult<Vec<_>>>()
    }

    /// Converts this value into a JSON value.
    /// 将本值转换为 JSON 值。
    ///
    /// Mapping notes / 映射说明:
    ///
    /// - `Null`, `Integer` and `Text` map directly.
    /// - `Real` maps to a JSON number, except that NaN and infinity become strings because JSON
    ///   has no representation for them.
    /// - `Blob` maps to an array of byte values. This direction is intentionally **not**
    ///   round-trippable with [`Self::from_json`], which refuses arrays — the same asymmetry the
    ///   migrated source had. It is pinned by a test rather than left as an accident.
    ///
    /// - `Null`、`Integer`、`Text` 直接映射。
    /// - `Real` 映射为 JSON 数字，但 NaN 与无穷会变成字符串，因为 JSON 无法表示它们。
    /// - `Blob` 映射为字节值数组。此方向刻意**不**与 [`Self::from_json`] 往返（后者拒绝数组），
    ///   与迁入源码一致。该行为由测试固定，而不是留作偶然。
    pub fn to_json(&self) -> JsonValue {
        match self {
            Self::Null => JsonValue::Null,
            Self::Integer(value) => JsonValue::from(*value),
            Self::Real(value) => json_float(*value),
            Self::Text(value) => JsonValue::String(value.clone()),
            Self::Blob(value) => JsonValue::Array(
                value
                    .iter()
                    .map(|byte| JsonValue::from(u64::from(*byte)))
                    .collect(),
            ),
        }
    }

    /// Renders this value as text, returning `None` for NULL.
    /// 将本值渲染为文本，NULL 返回 `None`。
    ///
    /// A blob renders as a SQLite blob literal (`x'…'`) so its identity survives a text column.
    ///
    /// 二进制渲染为 SQLite blob 字面量（`x'…'`），使其身份在文本列中得以保留。
    fn to_text(&self) -> Option<String> {
        match self {
            Self::Null => None,
            Self::Integer(value) => Some(value.to_string()),
            Self::Real(value) => Some(format_float(*value)),
            Self::Text(value) => Some(value.clone()),
            Self::Blob(value) => Some(format!("x'{}'", blob_to_hex(value))),
        }
    }
}

impl ToSql for SqliteValue {
    /// Binds this value as a borrowed SQLite parameter, avoiding a copy per parameter.
    /// 以借用方式把本值绑定为 SQLite 参数，避免每个参数一次拷贝。
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Self::Null => ToSqlOutput::Borrowed(ValueRef::Null),
            Self::Integer(value) => ToSqlOutput::Borrowed(ValueRef::Integer(*value)),
            Self::Real(value) => {
                // SQLite has no NaN: a bound NaN is stored as NULL, so accepting it would silently change the value
                // the caller asked to write, and an infinity has no exact representation either. Refusing at the
                // boundary matches the refusal `from_json` already applies to integers it cannot represent, and it
                // is the last point before the driver where the caller can still be told.
                // SQLite 没有 NaN：绑定的 NaN 会被存为 NULL，因此接受它会静默改变调用方要求写入的值，而无穷也没有精确
                // 表示。在边界处拒绝，与 `from_json` 对无法表示的整数已有的拒绝一致，且这是驱动之前最后一个仍能告知
                // 调用方的位置。
                if !value.is_finite() {
                    return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("a non-finite real ({value}) cannot be stored in SQLite"),
                        ),
                    )));
                }
                ToSqlOutput::Borrowed(ValueRef::Real(*value))
            }
            Self::Text(value) => ToSqlOutput::Borrowed(ValueRef::Text(value.as_bytes())),
            Self::Blob(value) => ToSqlOutput::Borrowed(ValueRef::Blob(value)),
        })
    }
}

impl From<RusqliteValue> for SqliteValue {
    /// Converts a driver value into a local value.
    /// 将驱动值转换为本地值。
    fn from(value: RusqliteValue) -> Self {
        match value {
            RusqliteValue::Null => Self::Null,
            RusqliteValue::Integer(value) => Self::Integer(value),
            RusqliteValue::Real(value) => Self::Real(value),
            RusqliteValue::Text(value) => Self::Text(value),
            RusqliteValue::Blob(value) => Self::Blob(value),
        }
    }
}

impl From<SqliteValue> for RusqliteValue {
    /// Converts a local value into a driver value.
    /// 将本地值转换为驱动值。
    fn from(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Null => Self::Null,
            SqliteValue::Integer(value) => Self::Integer(value),
            SqliteValue::Real(value) => Self::Real(value),
            SqliteValue::Text(value) => Self::Text(value),
            SqliteValue::Blob(value) => Self::Blob(value),
        }
    }
}

/// Result of executing a script or a single statement.
/// 执行脚本或单条语句的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteScriptResult {
    /// Whether the execution succeeded.
    /// 执行是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Number of rows changed.
    /// 受影响行数。
    pub rows_changed: i64,
    /// Last inserted row id.
    /// 最近一次插入行 ID。
    pub last_insert_rowid: i64,
}

/// Result of a batch execution.
/// 批量执行的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteBatchResult {
    /// Whether the execution succeeded.
    /// 执行是否成功。
    pub success: bool,
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Total number of rows changed across the batch.
    /// 批次累计受影响行数。
    pub rows_changed: i64,
    /// Last inserted row id.
    /// 最近一次插入行 ID。
    pub last_insert_rowid: i64,
    /// Number of statements executed.
    /// 实际执行的语句次数。
    pub statements_executed: i64,
}

/// Result of a JSON row-set query.
/// JSON 行集查询的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryJsonResult {
    /// JSON array-of-objects string.
    /// JSON 对象数组字符串。
    pub json_data: String,
    /// Number of rows returned.
    /// 返回行数。
    pub row_count: u64,
}

/// Arrow IPC chunk metrics for one query stream.
/// 一次查询流的 Arrow IPC 分块统计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryStreamMetrics {
    /// Number of rows returned.
    /// 返回行数。
    pub row_count: u64,
    /// Number of emitted chunks.
    /// 已发出分块数。
    pub chunk_count: u64,
    /// Total emitted byte size.
    /// 已发出总字节数。
    pub total_bytes: u64,
}

/// Location of one chunk inside the spool file.
/// 一个分块在暂存文件中的位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QueryStreamChunkDescriptor {
    /// Starting offset inside the spool file.
    /// 暂存文件中的起始偏移。
    offset: u64,
    /// Chunk byte length.
    /// 分块字节长度。
    len: u64,
}

/// Spool backend behind a query stream result.
/// 查询流结果背后的暂存后端。
#[derive(Debug)]
struct QueryStreamStorage {
    /// Spool file path, removed when the result is dropped.
    /// 暂存文件路径，结果被丢弃时删除。
    file_path: PathBuf,
    /// Chunk offset index.
    /// 分块偏移索引。
    chunks: Vec<QueryStreamChunkDescriptor>,
}

/// Result of a query stream, backed by a spool file.
/// 查询流结果，由暂存文件支撑。
///
/// The spool file is deleted when this value is dropped. That is the only cleanup path, which is
/// why the type is not `Clone`: duplicating it would create two owners of one file.
///
/// 本值被丢弃时暂存文件即被删除。这是唯一的清理路径，因此该类型不实现 `Clone`：复制它会产生一个
/// 文件的两个所有者。
#[derive(Debug)]
pub struct QueryStreamResult {
    /// Spool backend and chunk index.
    /// 暂存后端与分块索引。
    storage: QueryStreamStorage,
    /// Number of rows returned.
    /// 返回行数。
    pub row_count: u64,
    /// Number of emitted chunks.
    /// 已发出分块数。
    pub chunk_count: u64,
    /// Total emitted byte size.
    /// 已发出总字节数。
    pub total_bytes: u64,
}

impl QueryStreamResult {
    /// Reads the chunk at `index`.
    /// 读取下标 `index` 处的分块。
    pub fn read_chunk(&self, index: usize) -> EngineResult<Vec<u8>> {
        let descriptor = self.chunk_descriptor(index)?;

        let mut file = File::open(&self.storage.file_path).map_err(|error| {
            SqliteEngineError::serialization(format!(
                "failed to open query stream spool file: {error}"
            ))
        })?;
        file.seek(SeekFrom::Start(descriptor.offset))
            .map_err(|error| {
                SqliteEngineError::serialization(format!(
                    "failed to seek query stream spool file: {error}"
                ))
            })?;

        let chunk_len = usize::try_from(descriptor.len).map_err(|_| {
            SqliteEngineError::serialization(
                "query stream chunk length exceeds the platform usize range",
            )
        })?;

        let mut chunk = vec![0_u8; chunk_len];
        file.read_exact(&mut chunk).map_err(|error| {
            SqliteEngineError::serialization(format!(
                "failed to read query stream spool chunk: {error}"
            ))
        })?;

        Ok(chunk)
    }

    /// Returns the descriptor for `index`.
    /// 返回下标 `index` 的描述信息。
    fn chunk_descriptor(&self, index: usize) -> EngineResult<QueryStreamChunkDescriptor> {
        self.storage
            .chunks
            .get(index)
            .copied()
            .ok_or_else(|| SqliteEngineError::QueryStreamState {
                stream_id: index as u64,
                state: "chunk index out of bounds".to_string(),
            })
    }

    /// Returns the spool file path, for diagnostics.
    /// 返回暂存文件路径，用于诊断。
    pub fn spool_path(&self) -> &std::path::Path {
        &self.storage.file_path
    }
}

impl Drop for QueryStreamResult {
    /// Removes the spool file. Failure is ignored because there is no recovery action available at
    /// drop time and a leaked temp file must not abort the process.
    /// 删除暂存文件。失败被忽略，因为在 drop 时没有可用的补救动作，而泄漏一个临时文件不应终止进程。
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.storage.file_path);
    }
}

/// Executes a script or a single statement.
/// 执行脚本或单条语句。
///
/// With no bound parameters the whole string runs through `execute_batch`, so multi-statement
/// scripts behave exactly as SQLite scripts do. With bound parameters exactly one statement is
/// allowed, because a parameter list cannot be mapped onto an unknown number of statements.
///
/// 无绑定参数时整串通过 `execute_batch` 执行，使多语句脚本行为与 SQLite 脚本一致。有绑定参数时
/// 只允许单条语句，因为参数列表无法映射到数量未知的语句上。
pub fn execute_script(
    connection: &mut Connection,
    sql: &str,
    bound_values: &[SqliteValue],
) -> EngineResult<ExecuteScriptResult> {
    if sql.trim().is_empty() {
        return Err(SqliteEngineError::invalid_argument("sql must not be empty"));
    }

    if bound_values.is_empty() {
        connection
            .execute_batch(sql)
            .map_err(SqliteEngineError::sql)?;

        return Ok(ExecuteScriptResult {
            success: true,
            message: "script executed successfully".to_string(),
            rows_changed: i64::try_from(connection.changes()).unwrap_or(i64::MAX),
            last_insert_rowid: connection.last_insert_rowid(),
        });
    }

    if has_multiple_sql_statements(sql) {
        return Err(SqliteEngineError::invalid_argument(
            "bound parameters are only supported for a single SQL statement",
        ));
    }

    let mut statement = connection.prepare(sql).map_err(SqliteEngineError::sql)?;
    let params = bind_values_as_params(bound_values);
    let rows_changed = statement
        .execute(params.as_slice())
        .map_err(SqliteEngineError::sql)?;

    Ok(ExecuteScriptResult {
        success: true,
        message: format!("statement executed successfully (rows_changed={rows_changed})"),
        rows_changed: i64::try_from(rows_changed).unwrap_or(i64::MAX),
        last_insert_rowid: connection.last_insert_rowid(),
    })
}

/// Executes one statement repeatedly with multiple parameter groups inside a single transaction.
/// 在单个事务内以多组参数重复执行同一条语句。
///
/// The batch is all-or-nothing: any failing item rolls the whole batch back, and the failure
/// identifies the offending item index. This is why the statements are not run in autocommit.
///
/// 批次是全有或全无：任一项失败都会回滚整个批次，且失败会指出出错项的下标。这正是这些语句不在
/// 自动提交模式下执行的原因。
pub fn execute_batch(
    connection: &mut Connection,
    sql: &str,
    batch_params: &[Vec<SqliteValue>],
) -> EngineResult<ExecuteBatchResult> {
    if sql.trim().is_empty() {
        return Err(SqliteEngineError::invalid_argument("sql must not be empty"));
    }
    if batch_params.is_empty() {
        return Err(SqliteEngineError::invalid_argument(
            "batch parameters must not be empty",
        ));
    }
    if has_multiple_sql_statements(sql) {
        return Err(SqliteEngineError::invalid_argument(
            "execute_batch only supports a single SQL statement",
        ));
    }

    connection
        .execute_batch("BEGIN TRANSACTION")
        .map_err(SqliteEngineError::sql)?;

    let batch_result = (|| -> EngineResult<ExecuteBatchResult> {
        let mut statement = connection.prepare(sql).map_err(SqliteEngineError::sql)?;

        let mut rows_changed = 0_i64;
        for (index, params) in batch_params.iter().enumerate() {
            let bound = bind_values_as_params(params);
            let changed = statement
                .execute(bound.as_slice())
                .map_err(|source| SqliteEngineError::BatchItem { index, source })?;
            rows_changed = rows_changed.saturating_add(i64::try_from(changed).unwrap_or(i64::MAX));
        }

        drop(statement);
        connection
            .execute_batch("COMMIT")
            .map_err(SqliteEngineError::sql)?;

        Ok(ExecuteBatchResult {
            success: true,
            message: format!(
                "batch executed successfully (statements_executed={} rows_changed={rows_changed})",
                batch_params.len()
            ),
            rows_changed,
            last_insert_rowid: connection.last_insert_rowid(),
            statements_executed: i64::try_from(batch_params.len()).unwrap_or(i64::MAX),
        })
    })();

    if batch_result.is_err() {
        // The original failure is the actionable one; a rollback error cannot be repaired here.
        // 原始失败才是可处置的那个；回滚失败无法在此修复。
        let _ = connection.execute_batch("ROLLBACK");
    }

    batch_result
}

/// Executes ordered SQL scripts and one parameterized terminal statement in a single transaction.
/// 在单个事务中执行有序 SQL 脚本与一条参数化终结语句。
///
/// `scripts` contains host-validated schema scripts, while `terminal_sql` and `terminal_params`
/// durably record the operation only after every script succeeds. The complete sequence commits or
/// rolls back as one unit.
/// `scripts` 包含经宿主校验的 Schema 脚本；`terminal_sql` 与 `terminal_params` 仅在全部脚本成功后
/// 持久记录该操作。完整序列作为一个整体提交或回滚。
///
/// # Parameters / 参数
/// `connection` is the exclusively admitted database connection; `scripts` are executed in order;
/// `terminal_sql` must be one statement and `terminal_params` are its bound values.
/// `connection` 是已获独占准入的数据库连接；`scripts` 按顺序执行；`terminal_sql` 必须为单条语句，
/// `terminal_params` 是其绑定值。
///
/// # Returns / 返回值
/// Returns the aggregate committed change count and final row id, or the first error after rollback.
/// 返回已提交的聚合变更数量及最终行号，或回滚后的首个错误。
pub fn execute_scripts_transaction(
    connection: &mut Connection,
    scripts: &[String],
    terminal_sql: &str,
    terminal_params: &[SqliteValue],
) -> EngineResult<ExecuteScriptResult> {
    if terminal_sql.trim().is_empty() {
        return Err(SqliteEngineError::invalid_argument(
            "terminal sql must not be empty",
        ));
    }
    if has_multiple_sql_statements(terminal_sql) {
        return Err(SqliteEngineError::invalid_argument(
            "terminal sql must contain exactly one statement",
        ));
    }
    if scripts.iter().any(|script| script.trim().is_empty()) {
        return Err(SqliteEngineError::invalid_argument(
            "transaction scripts must not contain an empty script",
        ));
    }

    let changes_before = connection.total_changes();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(SqliteEngineError::sql)?;
    for script in scripts {
        transaction
            .execute_batch(script)
            .map_err(SqliteEngineError::sql)?;
    }
    let bound = bind_values_as_params(terminal_params);
    transaction
        .execute(terminal_sql, bound.as_slice())
        .map_err(SqliteEngineError::sql)?;
    let last_insert_rowid = transaction.last_insert_rowid();
    transaction.commit().map_err(SqliteEngineError::sql)?;
    let rows_changed = connection.total_changes().saturating_sub(changes_before);

    Ok(ExecuteScriptResult {
        success: true,
        message: format!(
            "transaction executed successfully (scripts={} rows_changed={rows_changed})",
            scripts.len()
        ),
        rows_changed: i64::try_from(rows_changed).unwrap_or(i64::MAX),
        last_insert_rowid,
    })
}

/// Executes a query and returns a JSON array of row objects.
/// 执行查询并返回行对象的 JSON 数组。
///
/// Column order follows the statement's column order, and duplicate column names collapse to the
/// last occurrence because the row object is keyed by name.
///
/// 列顺序遵循语句的列顺序；由于行对象以名称为键，重复列名会收敛为最后一次出现。
pub fn query_json(
    connection: &Connection,
    sql: &str,
    bound_values: &[SqliteValue],
) -> EngineResult<QueryJsonResult> {
    query_json_bounded(connection, sql, bound_values, QueryJsonLimits::UNBOUNDED)
}

/// Row and encoded-byte ceilings enforced while a JSON query result is being read.
/// 读取 JSON 查询结果期间强制执行的行数与编码字节上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryJsonLimits {
    /// Maximum number of rows the result may contain.
    /// 结果允许包含的最大行数。
    pub max_rows: u64,
    /// Maximum length of the encoded JSON array in bytes.
    /// 编码后 JSON 数组的最大字节长度。
    pub max_bytes: usize,
}

impl QueryJsonLimits {
    /// Limits that never stop a query early.
    /// 永不提前停止查询的上限。
    pub const UNBOUNDED: Self = Self {
        max_rows: u64::MAX,
        max_bytes: usize::MAX,
    };
}

/// Executes a query and returns a JSON array of row objects, stopping as soon as a limit is crossed.
/// 执行查询并返回行对象的 JSON 数组；一旦越过上限立即停止。
///
/// Rows are encoded one at a time into the output buffer, so an oversized result is rejected after
/// reading at most one row past the limit instead of being fully materialized first.
/// 行被逐条编码进输出缓冲，因此超限结果最多多读一行即被拒绝，而不是先完整物化。
///
/// # Returns / 返回值
/// Returns the encoded rows, or [`SqliteEngineError::ResultLimitExceeded`] naming the crossed limit.
/// 返回编码后的行，或指明被越过上限的 [`SqliteEngineError::ResultLimitExceeded`]。
pub fn query_json_bounded(
    connection: &Connection,
    sql: &str,
    bound_values: &[SqliteValue],
    limits: QueryJsonLimits,
) -> EngineResult<QueryJsonResult> {
    if sql.trim().is_empty() {
        return Err(SqliteEngineError::invalid_argument("sql must not be empty"));
    }
    if has_multiple_sql_statements(sql) {
        return Err(SqliteEngineError::invalid_argument(
            "query_json only supports a single SQL statement",
        ));
    }

    let mut statement = connection.prepare(sql).map_err(SqliteEngineError::sql)?;
    let column_names = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let params = bind_values_as_params(bound_values);
    let mut rows = statement
        .query(params.as_slice())
        .map_err(SqliteEngineError::sql)?;

    // json_data is the compact JSON array built row by row; its bytes equal serializing the whole row list.
    // json_data 是逐行构建的紧凑 JSON 数组；其字节与一次性序列化整个行列表完全一致。
    let mut json_data = vec![b'['];
    let mut row_count = 0_u64;
    while let Some(row) = rows.next().map_err(SqliteEngineError::sql)? {
        row_count += 1;
        if row_count > limits.max_rows {
            return Err(SqliteEngineError::result_limit_exceeded(format!(
                "more than {} rows",
                limits.max_rows
            )));
        }
        let mut object = JsonMap::new();
        for (index, column_name) in column_names.iter().enumerate() {
            let value = row.get_ref(index).map_err(SqliteEngineError::sql)?;
            object.insert(column_name.clone(), sqlite_value_ref_to_json(value));
        }
        if row_count > 1 {
            json_data.push(b',');
        }
        serde_json::to_writer(&mut json_data, &JsonValue::Object(object)).map_err(|error| {
            SqliteEngineError::serialization(format!("failed to serialize JSON result: {error}"))
        })?;
        // The closing bracket is counted so the check matches the final encoded length.
        // 计入结尾方括号，使检查与最终编码长度一致。
        if json_data.len() + 1 > limits.max_bytes {
            return Err(SqliteEngineError::result_limit_exceeded(format!(
                "more than {} encoded JSON bytes",
                limits.max_bytes
            )));
        }
    }
    json_data.push(b']');
    let json_data = String::from_utf8(json_data).map_err(|error| {
        SqliteEngineError::serialization(format!("JSON result is not valid UTF-8: {error}"))
    })?;

    Ok(QueryJsonResult {
        json_data,
        row_count,
    })
}

/// Executes a query and spools its Arrow IPC chunks to a temporary file.
/// 执行查询并把其 Arrow IPC 分块暂存到临时文件。
pub fn query_stream(
    connection: &mut Connection,
    sql: &str,
    bound_values: &[SqliteValue],
    target_chunk_size: usize,
) -> EngineResult<QueryStreamResult> {
    let writer = TempFileChunkWriter::new(target_chunk_size)?;
    let (writer, metrics) = query_stream_with_writer(connection, sql, bound_values, writer)?;

    Ok(QueryStreamResult {
        storage: writer.into_storage(),
        row_count: metrics.row_count,
        chunk_count: metrics.chunk_count,
        total_bytes: metrics.total_bytes,
    })
}

/// Executes a query and writes its Arrow IPC chunks into any [`Write`] sink.
/// 执行查询并把其 Arrow IPC 分块写入任意 [`Write`] 接收端。
///
/// A host that must keep query-stream artefacts inside its own authorized data area supplies its
/// own writer here; [`query_stream`] is the convenience wrapper that spools to the system
/// temporary directory. No path is accepted for the spool location, so this crate never derives
/// storage locations itself.
///
/// 必须把查询流产物保留在自己授权数据区内的宿主在此提供自己的写入器；[`query_stream`] 是暂存到系统
/// 临时目录的便利封装。本函数不接受暂存位置的路径，因此本 crate 从不自行派生存储位置。
pub fn query_stream_with_writer<W: QueryStreamChunkWriter>(
    connection: &mut Connection,
    sql: &str,
    bound_values: &[SqliteValue],
    writer: W,
) -> EngineResult<(W, QueryStreamMetrics)> {
    if sql.trim().is_empty() {
        return Err(SqliteEngineError::invalid_argument("sql must not be empty"));
    }
    if has_multiple_sql_statements(sql) {
        return Err(SqliteEngineError::invalid_argument(
            "query_stream only supports a single SQL statement",
        ));
    }

    let mut statement = connection.prepare(sql).map_err(SqliteEngineError::sql)?;

    let columns = statement.columns();
    let column_names = columns
        .iter()
        .map(|column| column.name().to_string())
        .collect::<Vec<_>>();
    let declared_types = columns
        .iter()
        .map(|column| column.decl_type().map(str::to_string))
        .collect::<Vec<_>>();

    let params = bind_values_as_params(bound_values);
    let mut rows = statement
        .query(params.as_slice())
        .map_err(SqliteEngineError::sql)?;

    let mut chunk_writer = Some(writer);
    let mut ipc_writer: Option<StreamWriter<W>> = None;
    let mut schema: Option<Arc<Schema>> = None;
    let mut column_kinds: Option<Vec<ArrowColumnKind>> = None;
    let mut total_rows: usize = 0;

    loop {
        let mut batch_rows = Vec::<Vec<SqliteValue>>::new();
        while batch_rows.len() < STREAMING_BATCH_ROWS {
            let Some(row) = rows.next().map_err(SqliteEngineError::sql)? else {
                break;
            };

            let mut values = Vec::with_capacity(column_names.len());
            for index in 0..column_names.len() {
                let value = row.get_ref(index).map_err(SqliteEngineError::sql)?;
                values.push(SqliteValue::try_from_value_ref(value, index)?);
            }
            batch_rows.push(values);
        }

        if batch_rows.is_empty() {
            break;
        }

        total_rows += batch_rows.len();

        if ipc_writer.is_none() {
            let kinds = infer_column_kinds(&declared_types, &batch_rows);
            let built_schema = Arc::new(Schema::new(
                column_names
                    .iter()
                    .zip(kinds.iter())
                    .map(|(name, kind)| Field::new(name, kind.data_type(), true))
                    .collect::<Vec<_>>(),
            ));

            let sink = chunk_writer.take().ok_or_else(|| {
                SqliteEngineError::serialization(
                    "query stream writer was already consumed before the header was written",
                )
            })?;
            let stream_writer =
                StreamWriter::try_new(sink, built_schema.as_ref()).map_err(|error| {
                    SqliteEngineError::serialization(format!(
                        "failed to write the Arrow stream header: {error}"
                    ))
                })?;

            column_kinds = Some(kinds);
            schema = Some(built_schema);
            ipc_writer = Some(stream_writer);
        }

        let Some(current_schema) = schema.as_ref() else {
            return Err(SqliteEngineError::serialization(
                "query stream schema was not initialized",
            ));
        };
        let Some(current_kinds) = column_kinds.as_ref() else {
            return Err(SqliteEngineError::serialization(
                "query stream column kinds were not initialized",
            ));
        };

        let arrays = build_arrow_arrays(current_kinds, &batch_rows)?;
        let batch = RecordBatch::try_new(Arc::clone(current_schema), arrays).map_err(|error| {
            SqliteEngineError::serialization(format!(
                "failed to build the Arrow record batch: {error}"
            ))
        })?;

        let Some(stream_writer) = ipc_writer.as_mut() else {
            return Err(SqliteEngineError::serialization(
                "query stream writer was not initialized",
            ));
        };
        stream_writer.write(&batch).map_err(|error| {
            SqliteEngineError::serialization(format!("failed to write an Arrow batch: {error}"))
        })?;
        stream_writer.flush().map_err(|error| {
            SqliteEngineError::serialization(format!("failed to flush an Arrow batch: {error}"))
        })?;
    }

    let (writer, chunk_count, total_bytes) = match ipc_writer {
        Some(mut stream_writer) => {
            stream_writer.finish().map_err(|error| {
                SqliteEngineError::serialization(format!(
                    "failed to finish the Arrow stream: {error}"
                ))
            })?;
            stream_writer.flush().map_err(|error| {
                SqliteEngineError::serialization(format!(
                    "failed to flush the Arrow stream: {error}"
                ))
            })?;

            let writer = stream_writer.into_inner().map_err(|error| {
                SqliteEngineError::serialization(format!(
                    "failed to finalize the Arrow stream: {error}"
                ))
            })?;
            let chunk_count = writer.emitted_chunk_count();
            let total_bytes = writer.emitted_total_bytes();
            (writer, chunk_count, total_bytes)
        }
        None => {
            let writer = chunk_writer.ok_or_else(|| {
                SqliteEngineError::serialization(
                    "query stream writer was consumed without producing a stream",
                )
            })?;
            (writer, 0, 0)
        }
    };

    Ok((
        writer,
        QueryStreamMetrics {
            row_count: u64::try_from(total_rows).unwrap_or(u64::MAX),
            chunk_count,
            total_bytes,
        },
    ))
}

/// Reports whether the SQL string contains more than one statement.
/// 判断 SQL 字符串是否包含多于一条语句。
pub fn has_multiple_sql_statements(sql: &str) -> bool {
    count_sql_statements(sql) > 1
}

/// Counts the effective SQL statements in a string.
/// 统计字符串中的有效 SQL 语句数。
///
/// The scanner tracks string literals, quoted identifiers, line comments and block comments so
/// that a semicolon inside them is not counted as a separator. An unterminated literal or block
/// comment is tolerated rather than rejected here; SQLite reports the syntax failure when the
/// statement is prepared, which is the authoritative error.
///
/// **All four of SQLite's identifier-quoting forms are tracked** — `"..."`, `'...'`, `[...]` and `` `...` `` — because
/// missing any of them made the scanner count a semicolon inside such an identifier as a separator and so refuse a
/// valid single statement. The escapers differ and are mirrored rather than unified: `"` and `'` double the quote
/// character, while SQLite's tokenizer ends `[...]` and `` `...` `` at the first closing character with no escape.
///
/// 扫描器跟踪字符串字面量、引用标识符、行注释与块注释，使它们内部的分号不被计为分隔符。未终止的
/// 字面量或块注释在此被容忍而非拒绝；SQLite 在准备语句时会报告语法失败，那才是权威错误。
///
/// **SQLite 的四种标识符引用形式全部被跟踪**——`"..."`、`'...'`、`[...]` 与 `` `...` ``——因为缺少任何一种都会让
/// 扫描器把该标识符内部的分号计为分隔符，从而拒绝一条合法语句。各形式的转义规则不同，此处是模仿而非统一：`"` 与 `'`
/// 以字符翻倍转义，而 SQLite 的词法分析器对 `[...]` 与 `` `...` `` 在第一个闭合字符处结束、没有转义。
pub fn count_sql_statements(sql: &str) -> usize {
    let chars: Vec<char> = sql.chars().collect();
    let len = chars.len();
    let mut index = 0;
    let mut statement_count = 0;
    let mut has_content = false;

    while index < len {
        match chars[index] {
            '\'' => {
                has_content = true;
                index += 1;
                while index < len {
                    if chars[index] == '\'' {
                        if index + 1 < len && chars[index + 1] == '\'' {
                            index += 2;
                        } else {
                            index += 1;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
            }
            '"' => {
                has_content = true;
                index += 1;
                while index < len {
                    if chars[index] == '"' {
                        if index + 1 < len && chars[index + 1] == '"' {
                            index += 2;
                        } else {
                            index += 1;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
            }
            '-' if index + 1 < len && chars[index + 1] == '-' => {
                index += 2;
                while index < len && chars[index] != '\n' {
                    index += 1;
                }
            }
            // SQLite accepts two further identifier-quoting forms, and both were missing here. Measured, not inferred:
            // `SELECT * FROM [a;b]` and ``SELECT * FROM `a;b` `` were counted as **two** statements even though SQLite
            // creates and queries such a table happily, so the engine refused a valid single statement with "only
            // supports a single SQL statement" at all four call sites. SQLite's tokenizer ends these forms at the first
            // closing character and has **no** escape inside them, which is what these arms mirror — deliberately not
            // the doubled-character rule used by the two quoting forms above.
            // SQLite 还接受另外两种标识符引用形式，而它们在本处缺失。此为实测而非推断：`SELECT * FROM [a;b]` 与
            // ``SELECT * FROM `a;b` `` 被计为**两条**语句，而 SQLite 能正常创建并查询这样的表，因此引擎会在全部四个
            // 调用点以"仅支持单条 SQL 语句"拒绝一条合法语句。SQLite 的词法分析器在这两种形式遇到第一个闭合字符即结束，
            // 且**没有**转义，这正是这两个分支所模仿的——刻意不采用上方两种引用形式的字符翻倍规则。
            '[' => {
                has_content = true;
                index += 1;
                while index < len {
                    if chars[index] == ']' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            '`' => {
                has_content = true;
                index += 1;
                while index < len {
                    if chars[index] == '`' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            '/' if index + 1 < len && chars[index + 1] == '*' => {
                index += 2;
                while index + 1 < len {
                    if chars[index] == '*' && chars[index + 1] == '/' {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            ';' => {
                if has_content {
                    statement_count += 1;
                }
                has_content = false;
                index += 1;
            }
            character if !character.is_whitespace() => {
                has_content = true;
                index += 1;
            }
            _ => index += 1,
        }
    }

    if has_content {
        statement_count += 1;
    }

    statement_count
}

/// Converts a driver value reference into a JSON value.
/// 将驱动值引用转换为 JSON 值。
fn sqlite_value_ref_to_json(value: ValueRef<'_>) -> JsonValue {
    match SqliteValue::try_from_value_ref(value, 0) {
        Ok(value) => value.to_json(),
        Err(_) => JsonValue::Null,
    }
}

/// Collects bound parameters as trait objects for rusqlite.
/// 把绑定参数收集为供 rusqlite 使用的 trait 对象。
fn bind_values_as_params(values: &[SqliteValue]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

impl SqliteValue {
    /// Converts a borrowed driver value reference into a local value.
    /// 将借用的驱动值引用转换为本地值。
    ///
    /// `column_index` is used only to build an informative conversion error, because rusqlite's
    /// own conversion failure type wants the column position.
    ///
    /// `column_index` 仅用于构造可读的转换错误，因为 rusqlite 自身的转换失败类型需要列位置。
    fn try_from_value_ref(value: ValueRef<'_>, column_index: usize) -> EngineResult<Self> {
        match value {
            ValueRef::Null => Ok(Self::Null),
            ValueRef::Integer(value) => Ok(Self::Integer(value)),
            ValueRef::Real(value) => Ok(Self::Real(value)),
            ValueRef::Text(bytes) => std::str::from_utf8(bytes)
                .map(|text| Self::Text(text.to_string()))
                .map_err(|error| SqliteEngineError::Sql {
                    source: RusqliteError::FromSqlConversionFailure(
                        column_index,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    ),
                }),
            ValueRef::Blob(bytes) => Ok(Self::Blob(bytes.to_vec())),
        }
    }
}

/// Arrow column kind inferred for one result column.
/// 为一个结果列推断出的 Arrow 列类型。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ArrowColumnKind {
    /// 64-bit signed integer column.
    /// 64 位有符号整数列。
    Int64,
    /// 64-bit floating point column.
    /// 64 位浮点列。
    Float64,
    /// UTF-8 text column.
    /// UTF-8 文本列。
    Utf8,
    /// Binary column.
    /// 二进制列。
    Binary,
}

impl ArrowColumnKind {
    /// Returns the Arrow data type for this column kind.
    /// 返回本列类型对应的 Arrow 数据类型。
    fn data_type(self) -> DataType {
        match self {
            Self::Int64 => DataType::Int64,
            Self::Float64 => DataType::Float64,
            Self::Utf8 => DataType::Utf8,
            Self::Binary => DataType::Binary,
        }
    }
}

/// Infers one Arrow column kind per result column.
/// 为每个结果列推断一个 Arrow 列类型。
///
/// A declared SQLite type provides a starting hint, and observed values refine it. Because the
/// schema must be fixed before the first batch is written, inference runs on the first batch only.
/// A later incompatible value is rejected by [`build_arrow_arrays`] rather than silently converted
/// to NULL or truncated under a schema that can no longer be widened.
///
/// 声明的 SQLite 类型提供初始提示，观测到的值对其进行细化。由于 schema 必须在对第一批写入前确定，
/// 推断只对第一批执行；后续批次出现不兼容值时由 [`build_arrow_arrays`] 拒绝，而不是在已无法放宽的
/// schema 下静默写成 NULL 或截断。
fn infer_column_kinds(
    declared_types: &[Option<String>],
    rows: &[Vec<SqliteValue>],
) -> Vec<ArrowColumnKind> {
    let column_count = declared_types.len();
    let mut kinds = Vec::with_capacity(column_count);

    for (index, declared_type) in declared_types.iter().enumerate() {
        let mut current = declared_type_hint(declared_type.as_deref());
        for row in rows {
            // A row shorter than the declared column count cannot happen for a live statement, but
            // skipping a missing cell defensively keeps a driver change from panicking.
            // 对存活语句而言行不会短于声明列数，但跳过缺失单元格可避免驱动变化导致 panic。
            if let Some(value) = row.get(index) {
                current = merge_column_kind(current, value);
            }
        }
        kinds.push(current.unwrap_or(ArrowColumnKind::Utf8));
    }

    kinds
}

/// Maps a declared SQLite type name onto an initial Arrow column kind.
/// 将声明的 SQLite 类型名映射为初始 Arrow 列类型。
fn declared_type_hint(value: Option<&str>) -> Option<ArrowColumnKind> {
    let normalized = value?.trim().to_ascii_uppercase();

    if normalized.contains("INT") || normalized.contains("BOOL") {
        Some(ArrowColumnKind::Int64)
    } else if normalized.contains("REAL")
        || normalized.contains("FLOA")
        || normalized.contains("DOUB")
        || normalized.contains("NUMERIC")
        || normalized.contains("DEC")
    {
        Some(ArrowColumnKind::Float64)
    } else if normalized.contains("BLOB") {
        Some(ArrowColumnKind::Binary)
    } else if normalized.contains("CHAR")
        || normalized.contains("CLOB")
        || normalized.contains("TEXT")
        || normalized.contains("JSON")
        || normalized.contains("DATE")
        || normalized.contains("TIME")
    {
        Some(ArrowColumnKind::Utf8)
    } else {
        None
    }
}

/// Merges one observed value into the running column kind.
/// 将一个观测值合并进当前列类型。
///
/// Mixed input widens to text, because text can represent every SQLite value without loss of
/// identity. NULL never changes the running kind, so an all-NULL column keeps its declared hint.
///
/// 混合输入会放宽为文本，因为文本能在不丢失身份的前提下表示任何 SQLite 值。NULL 不改变当前类型，
/// 因此全 NULL 列保留其声明提示。
fn merge_column_kind(
    current: Option<ArrowColumnKind>,
    value: &SqliteValue,
) -> Option<ArrowColumnKind> {
    use ArrowColumnKind::{Binary, Float64, Int64, Utf8};

    match value {
        SqliteValue::Null => current,
        SqliteValue::Integer(_) => Some(match current {
            None | Some(Int64) => Int64,
            Some(Float64) => Float64,
            Some(Utf8 | Binary) => Utf8,
        }),
        SqliteValue::Real(_) => Some(match current {
            None | Some(Float64) => Float64,
            Some(Int64) => Float64,
            Some(Utf8 | Binary) => Utf8,
        }),
        SqliteValue::Text(_) => Some(Utf8),
        SqliteValue::Blob(_) => Some(match current {
            None | Some(Binary) => Binary,
            _ => Utf8,
        }),
    }
}

/// Builds one Arrow array per column for a batch of rows.
/// 为一批行的每个列构建一个 Arrow 数组。
fn build_arrow_arrays(
    kinds: &[ArrowColumnKind],
    rows: &[Vec<SqliteValue>],
) -> EngineResult<Vec<ArrayRef>> {
    let mut arrays = Vec::<ArrayRef>::with_capacity(kinds.len());

    for (index, kind) in kinds.iter().enumerate() {
        match kind {
            ArrowColumnKind::Int64 => {
                let mut builder = Int64Builder::with_capacity(rows.len());
                for row in rows {
                    match row.get(index) {
                        Some(SqliteValue::Integer(value)) => builder.append_value(*value),
                        Some(SqliteValue::Null) => builder.append_null(),
                        Some(_) => return Err(query_stream_type_changed(index, "Int64")),
                        None => return Err(query_stream_row_is_short(index)),
                    }
                }
                arrays.push(Arc::new(builder.finish()));
            }
            ArrowColumnKind::Float64 => {
                let mut builder = Float64Builder::with_capacity(rows.len());
                for row in rows {
                    match row.get(index) {
                        Some(SqliteValue::Integer(value)) => {
                            let converted = *value as f64;
                            if converted as i64 != *value {
                                return Err(SqliteEngineError::serialization(format!(
                                    "query stream column {index} cannot represent integer {value} exactly as Float64"
                                )));
                            }
                            builder.append_value(converted);
                        }
                        Some(SqliteValue::Real(value)) => builder.append_value(*value),
                        Some(SqliteValue::Null) => builder.append_null(),
                        Some(_) => return Err(query_stream_type_changed(index, "Float64")),
                        None => return Err(query_stream_row_is_short(index)),
                    }
                }
                arrays.push(Arc::new(builder.finish()));
            }
            ArrowColumnKind::Utf8 => {
                let mut builder = StringBuilder::new();
                for row in rows {
                    match row.get(index) {
                        Some(SqliteValue::Null) => builder.append_null(),
                        Some(value) => builder.append_value(
                            value
                                .to_text()
                                .ok_or_else(|| query_stream_type_changed(index, "Utf8"))?,
                        ),
                        None => return Err(query_stream_row_is_short(index)),
                    }
                }
                arrays.push(Arc::new(builder.finish()));
            }
            ArrowColumnKind::Binary => {
                let mut builder = BinaryBuilder::new();
                for row in rows {
                    match row.get(index) {
                        Some(SqliteValue::Blob(value)) => builder.append_value(value),
                        Some(SqliteValue::Null) => builder.append_null(),
                        Some(_) => return Err(query_stream_type_changed(index, "Binary")),
                        None => return Err(query_stream_row_is_short(index)),
                    }
                }
                arrays.push(Arc::new(builder.finish()));
            }
        }
    }

    Ok(arrays)
}

/// Builds the explicit error used when a later batch no longer fits the fixed Arrow schema.
/// 构造后续批次不再匹配固定 Arrow schema 时使用的明确错误。
fn query_stream_type_changed(column_index: usize, expected: &str) -> SqliteEngineError {
    SqliteEngineError::serialization(format!(
        "query stream column {column_index} changed SQLite value type after the Arrow schema was fixed as {expected}"
    ))
}

/// Builds the invariant error for a row shorter than the prepared statement's column list.
/// 构造行长度短于预编译语句列清单时的不变量错误。
fn query_stream_row_is_short(column_index: usize) -> SqliteEngineError {
    SqliteEngineError::serialization(format!("query stream row is missing column {column_index}"))
}

/// Renders bytes as an uppercase hexadecimal string.
/// 将字节渲染为大写十六进制字符串。
fn blob_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[(byte >> 4) as usize]));
        output.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    output
}

/// Converts a float into a JSON value, degrading non-finite values to strings.
/// 将浮点数转换为 JSON 值，非有限值降级为字符串。
fn json_float(value: f64) -> JsonValue {
    if value.is_nan() || value.is_infinite() {
        return JsonValue::String(format_float(value));
    }
    JsonNumber::from_f64(value)
        .map(JsonValue::Number)
        .unwrap_or_else(|| JsonValue::String(format_float(value)))
}

/// Formats a float, keeping a decimal point on integral values so type identity survives text.
/// 格式化浮点数；整数值保留小数点，使类型身份在文本中得以保留。
fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

/// A chunk sink that reports how much it has emitted.
/// 可报告已发出量的分块接收端。
pub trait QueryStreamChunkWriter: Write {
    /// Returns the number of chunks emitted so far.
    /// 返回已发出的分块数。
    fn emitted_chunk_count(&self) -> u64;

    /// Returns the number of bytes emitted so far.
    /// 返回已发出的字节数。
    fn emitted_total_bytes(&self) -> u64;
}

/// In-memory chunk collector.
/// 内存分块收集器。
///
/// Intended for tests and small queries; [`TempFileChunkWriter`] is the spooling counterpart.
/// 适用于测试与小型查询；[`TempFileChunkWriter`] 是暂存到文件的对应用例。
#[derive(Debug, Default)]
pub struct ChunkCollector {
    /// Completed chunks in emission order.
    /// 按发出顺序排列的已完成分块。
    chunks: Vec<Vec<u8>>,
    /// Bytes accumulated but not yet emitted as a chunk.
    /// 已累积但尚未作为分块发出的字节。
    pending: Vec<u8>,
    /// Threshold at which `pending` is emitted as a chunk.
    /// `pending` 被作为分块发出的阈值。
    target_chunk_size: usize,
    /// Number of chunks emitted.
    /// 已发出分块数。
    emitted_chunks: usize,
    /// Number of bytes emitted.
    /// 已发出字节数。
    emitted_bytes: usize,
}

impl ChunkCollector {
    /// Creates a collector with an enforced minimum chunk size.
    /// 创建一个带最小分块大小下限的收集器。
    pub fn new(target_chunk_size: usize) -> Self {
        let chunk_size = target_chunk_size.max(MIN_CHUNK_BYTES);
        Self {
            chunks: Vec::new(),
            pending: Vec::with_capacity(chunk_size),
            target_chunk_size: chunk_size,
            emitted_chunks: 0,
            emitted_bytes: 0,
        }
    }

    /// Returns the chunks emitted so far.
    /// 返回已发出的分块。
    pub fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }

    /// Emits every full-size chunk currently buffered.
    /// 发出当前已缓冲的每个满尺寸分块。
    fn emit_full_chunks(&mut self) {
        while self.pending.len() >= self.target_chunk_size {
            let remainder = self.pending.split_off(self.target_chunk_size);
            let chunk = std::mem::replace(&mut self.pending, remainder);
            self.send_chunk(chunk);
        }
    }

    /// Emits whatever remains buffered, if anything.
    /// 发出仍被缓冲的内容（若有）。
    fn emit_remaining(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let chunk = std::mem::take(&mut self.pending);
        self.send_chunk(chunk);
    }

    /// Records one emitted chunk.
    /// 记录一个已发出的分块。
    fn send_chunk(&mut self, chunk: Vec<u8>) {
        self.emitted_chunks += 1;
        self.emitted_bytes += chunk.len();
        self.chunks.push(chunk);
    }
}

impl QueryStreamChunkWriter for ChunkCollector {
    /// Returns the number of chunks emitted so far.
    /// 返回已发出的分块数。
    fn emitted_chunk_count(&self) -> u64 {
        u64::try_from(self.emitted_chunks).unwrap_or(u64::MAX)
    }

    /// Returns the number of bytes emitted so far.
    /// 返回已发出的字节数。
    fn emitted_total_bytes(&self) -> u64 {
        u64::try_from(self.emitted_bytes).unwrap_or(u64::MAX)
    }
}

impl Write for ChunkCollector {
    /// Buffers bytes and emits any completed chunks.
    /// 缓冲字节并发出已完成的分块。
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.pending.extend_from_slice(buf);
        self.emit_full_chunks();
        Ok(buf.len())
    }

    /// Emits remaining buffered bytes.
    /// 发出仍被缓冲的字节。
    fn flush(&mut self) -> io::Result<()> {
        self.emit_remaining();
        Ok(())
    }
}

/// Temporary-file-backed chunk writer.
/// 由临时文件支撑的分块写入器。
///
/// Chunks are appended to a spool file and indexed by offset, so reading a chunk later does not
/// require holding the whole result in memory.
///
/// 分块被追加到暂存文件并按偏移建立索引，因此后续读取某个分块不需要把整个结果保留在内存中。
#[derive(Debug)]
pub struct TempFileChunkWriter {
    /// Spool file handle.
    /// 暂存文件句柄。
    ///
    /// It is an `Option` because cleanup must close the handle before removing the file: on Windows deleting a file
    /// that is still open fails, which would leave exactly the file the cleanup exists to remove. The write paths
    /// report a closed spool instead of panicking, and only `Drop` ever clears it.
    /// 它是 `Option`，因为清理必须在删除文件之前关闭句柄：在 Windows 上删除仍打开的文件会失败，从而恰好留下本次
    /// 清理要移除的文件。写入路径会报告"暂存已关闭"而不是 panic，且只有 `Drop` 会清空它。
    file: Option<File>,
    /// Spool file path, owned until handed to [`QueryStreamResult`].
    /// 暂存文件路径，在交给 [`QueryStreamResult`] 之前由其拥有。
    file_path: PathBuf,
    /// Whether the spool file was handed over, after which [`QueryStreamResult`] owns its removal.
    /// 暂存文件是否已交接；交接后由 [`QueryStreamResult`] 负责其删除。
    handed_over: bool,
    /// Bytes accumulated but not yet emitted.
    /// 已累积但尚未发出的字节。
    pending: Vec<u8>,
    /// Threshold at which `pending` is emitted.
    /// `pending` 被发出的阈值。
    target_chunk_size: usize,
    /// Number of chunks emitted.
    /// 已发出分块数。
    emitted_chunks: usize,
    /// Number of bytes emitted.
    /// 已发出字节数。
    emitted_bytes: usize,
    /// Offset where the next chunk starts.
    /// 下一个分块的起始偏移。
    current_offset: u64,
    /// Offset index of emitted chunks.
    /// 已发出分块的偏移索引。
    chunk_descriptors: Vec<QueryStreamChunkDescriptor>,
}

/// Monotonic counter making spool file names unique within one process.
/// 使暂存文件名在单进程内唯一的单调计数器。
static NEXT_QUERY_STREAM_SPOOL_ID: AtomicU64 = AtomicU64::new(1);

impl Drop for TempFileChunkWriter {
    /// Removes an un-handed-over spool file, closing its handle first.
    /// 删除尚未交接的暂存文件，并先关闭其句柄。
    ///
    /// The order is the whole point. On Windows a file that is still open cannot be deleted, so removing before
    /// closing would fail and leave behind exactly the file this exists to remove. Failure is ignored after that
    /// because there is no recovery action at drop time and a leaked temporary file must not abort the process.
    /// 顺序就是全部要点。在 Windows 上仍打开的文件无法删除，因此先删除后关闭会失败，并恰好留下本实现要移除的文件。
    /// 此后失败被忽略，因为在 drop 时没有可用的补救动作，而泄漏一个临时文件不应终止进程。
    fn drop(&mut self) {
        if self.handed_over {
            return;
        }
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.file_path);
    }
}

impl TempFileChunkWriter {
    /// Creates a spool file writer with an enforced minimum chunk size.
    /// 创建一个带最小分块大小下限的暂存文件写入器。
    pub fn new(target_chunk_size: usize) -> EngineResult<Self> {
        let chunk_size = target_chunk_size.max(MIN_CHUNK_BYTES);
        let file_path = make_query_stream_spool_path();
        let file = File::create(&file_path).map_err(|error| {
            SqliteEngineError::serialization(format!(
                "failed to create the query stream spool file: {error}"
            ))
        })?;

        Ok(Self {
            file: Some(file),
            file_path,
            handed_over: false,
            pending: Vec::with_capacity(chunk_size),
            target_chunk_size: chunk_size,
            emitted_chunks: 0,
            emitted_bytes: 0,
            current_offset: 0,
            chunk_descriptors: Vec::new(),
        })
    }

    /// Consumes the writer, handing its spool file to a query stream result.
    /// 消费写入器，把其暂存文件交给查询流结果。
    ///
    /// The hand-over disarms this writer's cleanup, because removal now belongs to the result. The fields are taken
    /// rather than moved, which a `Drop` implementation requires.
    /// 交接会解除本写入器的清理职责，因为删除现已属于结果。字段以取出而非移动的方式处理，这是 `Drop` 实现所要求的。
    fn into_storage(mut self) -> QueryStreamStorage {
        self.handed_over = true;
        let file_path = std::mem::take(&mut self.file_path);
        let chunks = std::mem::take(&mut self.chunk_descriptors);
        QueryStreamStorage { file_path, chunks }
    }

    /// Emits every full-size chunk currently buffered.
    /// 发出当前已缓冲的每个满尺寸分块。
    fn emit_full_chunks(&mut self) -> io::Result<()> {
        while self.pending.len() >= self.target_chunk_size {
            let remainder = self.pending.split_off(self.target_chunk_size);
            let chunk = std::mem::replace(&mut self.pending, remainder);
            self.write_chunk(chunk)?;
        }
        Ok(())
    }

    /// Emits whatever remains buffered, if anything.
    /// 发出仍被缓冲的内容（若有）。
    fn emit_remaining(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::take(&mut self.pending);
        self.write_chunk(chunk)
    }

    /// Appends one chunk to the spool file and records its offset.
    /// 把一个分块追加到暂存文件并记录其偏移。
    fn write_chunk(&mut self, chunk: Vec<u8>) -> io::Result<()> {
        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other(
                "the query stream spool file was already closed",
            ));
        };
        file.write_all(&chunk)?;
        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        self.chunk_descriptors.push(QueryStreamChunkDescriptor {
            offset: self.current_offset,
            len: chunk_len,
        });
        self.current_offset = self.current_offset.saturating_add(chunk_len);
        self.emitted_chunks += 1;
        self.emitted_bytes += chunk.len();
        Ok(())
    }
}

impl QueryStreamChunkWriter for TempFileChunkWriter {
    /// Returns the number of chunks emitted so far.
    /// 返回已发出的分块数。
    fn emitted_chunk_count(&self) -> u64 {
        u64::try_from(self.emitted_chunks).unwrap_or(u64::MAX)
    }

    /// Returns the number of bytes emitted so far.
    /// 返回已发出的字节数。
    fn emitted_total_bytes(&self) -> u64 {
        u64::try_from(self.emitted_bytes).unwrap_or(u64::MAX)
    }
}

impl Write for TempFileChunkWriter {
    /// Buffers bytes and emits any completed chunks.
    /// 缓冲字节并发出已完成的分块。
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.pending.extend_from_slice(buf);
        self.emit_full_chunks()?;
        Ok(buf.len())
    }

    /// Emits remaining buffered bytes and flushes the spool file.
    /// 发出仍被缓冲的字节并刷新暂存文件。
    fn flush(&mut self) -> io::Result<()> {
        self.emit_remaining()?;
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Err(io::Error::other(
                "the query stream spool file was already closed",
            )),
        }
    }
}

/// Builds a unique spool file path in the system temporary directory.
/// 在系统临时目录中构造唯一的暂存文件路径。
///
/// Uniqueness comes from the process id, a monotonic counter and the wall-clock nanosecond value,
/// so two writers in one process and two processes cannot collide. A monotonic clock reading is
/// unavailable before the UNIX epoch, in which case only the counter and process id distinguish
/// names — still sufficient within a process.
///
/// 唯一性来自进程 id、单调计数器与墙上时钟纳秒值，因此同进程的两个写入器以及两个进程都不会碰撞。
/// UNIX 纪元之前无法读取该时钟值，此时仅由计数器与进程 id 区分名称，在单进程内仍然充分。
fn make_query_stream_spool_path() -> PathBuf {
    let unique = NEXT_QUERY_STREAM_SPOOL_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();

    std::env::temp_dir().join(format!(
        "vulcan-sqlite-query-stream-{}-{unique}-{timestamp_nanos}.bin",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        ChunkCollector, DEFAULT_IPC_CHUNK_BYTES, QueryJsonLimits, QueryStreamChunkWriter,
        SqliteValue, count_sql_statements, execute_batch, execute_script,
        execute_scripts_transaction, has_multiple_sql_statements, query_json, query_json_bounded,
        query_stream, query_stream_with_writer,
    };
    use crate::error::SqliteEngineError;
    use arrow::array::Int64Array;
    use arrow::datatypes::DataType;
    use arrow::ipc::reader::StreamReader;
    use arrow::record_batch::RecordBatch;
    use rusqlite::Connection;
    use std::io::Cursor;

    /// Verifies an ordered script failure rolls back both earlier schema changes and the terminal record.
    /// 验证有序脚本失败会同时回滚更早的 Schema 变更与终结记录。
    #[test]
    fn ordered_scripts_and_terminal_record_are_atomic() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, label TEXT NOT NULL);",
            )
            .expect("create migration ledger");
        let failing_scripts = vec![
            String::from("CREATE TABLE created_before_failure(id INTEGER PRIMARY KEY);"),
            String::from("INSERT INTO missing_table(id) VALUES (1);"),
        ];

        let error = execute_scripts_transaction(
            &mut connection,
            &failing_scripts,
            "INSERT INTO schema_migrations(version, label) VALUES (?1, ?2)",
            &[
                SqliteValue::Integer(1),
                SqliteValue::Text(String::from("failed")),
            ],
        )
        .expect_err("failing script must roll the transaction back");
        assert!(matches!(error, SqliteEngineError::Sql { .. }));
        let created_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'created_before_failure'",
                [],
                |row| row.get(0),
            )
            .expect("inspect rolled-back schema");
        let ledger_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("inspect migration ledger");
        assert_eq!(created_count, 0);
        assert_eq!(ledger_count, 0);

        let successful_scripts = vec![String::from(
            "CREATE TABLE committed_schema(id INTEGER PRIMARY KEY);",
        )];
        execute_scripts_transaction(
            &mut connection,
            &successful_scripts,
            "INSERT INTO schema_migrations(version, label) VALUES (?1, ?2)",
            &[
                SqliteValue::Integer(2),
                SqliteValue::Text(String::from("committed")),
            ],
        )
        .expect("commit migration transaction");
        let committed_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM committed_schema", [], |row| {
                row.get(0)
            })
            .expect("query committed schema");
        let committed_label: String = connection
            .query_row(
                "SELECT label FROM schema_migrations WHERE version = 2",
                [],
                |row| row.get(0),
            )
            .expect("query committed ledger row");
        assert_eq!(committed_count, 0);
        assert_eq!(committed_label, "committed");
    }

    /// Measures batch-write and query-stream latency and reports p50/p95 as plan §10 evidence.
    /// 测量批量写入与查询流的延迟，并报告中位数与 p95，作为方案 §10 的证据。
    ///
    /// Ignored by default because it is evidence rather than a correctness check. It asserts nothing about
    /// timing — a wall-clock threshold would be flaky on shared CI — and prints the measurements instead, so
    /// the evidence is the recorded number, exactly as plan §10 asks for a record rather than a bound.
    /// 默认忽略，因为它是证据而非正确性检查。它不对耗时作任何断言——墙上时钟阈值在共享 CI 上会抖动——而是
    /// 打印测量值，故证据就是被记录下来的数字，正如方案 §10 要求的是记录而非界限。
    #[test]
    #[ignore = "explicit §10 SQLite engine performance evidence"]
    fn performance_batch_write_and_query_stream_report_p50_p95() {
        const SAMPLES: usize = 20;
        const ROWS_PER_BATCH: usize = 10_000;

        let mut connection = open_test_connection();
        crate::allocation_evidence::reset_high_water();
        let mut streamed_chunks = 0u64;
        let mut streamed_bytes = 0u64;
        let insert_sql = "INSERT INTO items (id, name, weight) VALUES (?, ?, ?)";
        let mut write_micros = Vec::with_capacity(SAMPLES);
        let mut read_micros = Vec::with_capacity(SAMPLES);

        for sample in 0..SAMPLES {
            // Every sample uses its own id range so the primary key never collides across samples.
            // 每个样本使用自己的 id 区间，使主键不会跨样本冲突。
            let base = (sample * ROWS_PER_BATCH) as i64;
            let batch_params = (0..ROWS_PER_BATCH)
                .map(|offset| {
                    let id = base + offset as i64;
                    vec![
                        SqliteValue::Integer(id),
                        SqliteValue::Text(format!("row-{id}")),
                        SqliteValue::Real(id as f64 * 0.5),
                    ]
                })
                .collect::<Vec<_>>();

            let write_started = std::time::Instant::now();
            execute_batch(&mut connection, insert_sql, &batch_params).expect("batch insert");
            write_micros.push(write_started.elapsed().as_micros());

            let read_started = std::time::Instant::now();
            let stream = query_stream(
                &mut connection,
                "SELECT id, name, weight FROM items ORDER BY id",
                &[] as &[SqliteValue],
                64 * 1024,
            )
            .expect("query stream");
            read_micros.push(read_started.elapsed().as_micros());
            // The stream is dropped here; its spool file is removed by the writer's own cleanup.
            // 查询流在此被丢弃；其暂存文件由写入器自身的清理逻辑移除。
            streamed_chunks = stream.chunk_count;
            streamed_bytes = stream.total_bytes;
            drop(stream);
        }

        // The first iteration is a warmup: its work runs and warms the path, its sample is discarded.
        // 首次迭代为预热：其工作照常执行并预热路径，其样本被丢弃。
        write_micros.remove(0);
        read_micros.remove(0);
        let (write_p50, write_p95) = percentile_micros(&mut write_micros);
        let (read_p50, read_p95) = percentile_micros(&mut read_micros);
        let peak_live_bytes = crate::allocation_evidence::high_water_bytes();
        let env_os = std::env::consts::OS;
        let env_arch = std::env::consts::ARCH;
        eprintln!(
            "[perf-sqlite] samples={SAMPLES} rows_per_batch={ROWS_PER_BATCH} \
         batch_write_p50_us={write_p50} batch_write_p95_us={write_p95} \
         query_stream_p50_us={read_p50} query_stream_p95_us={read_p95} \
         streamed_chunks={streamed_chunks} streamed_bytes={streamed_bytes} \
         peak_live_bytes={peak_live_bytes} env_os={env_os} env_arch={env_arch}"
        );
    }

    /// Returns the median and the ninety-fifth percentile of a sample list, in microseconds.
    /// 返回样本列表的中位数与第九十五百分位，单位为微秒。
    ///
    /// The input is sorted in place. Percentile selection uses the nearest-rank method, which is what
    /// plan §10's "median and ninety-fifth percentile" asks for on a twenty-sample list.
    /// 输入会被就地排序。百分位采用最近秩法，这正是方案 §10 的"中位数与第九十五百分位"在二十个样本上的取法。
    fn percentile_micros(samples: &mut [u128]) -> (u128, u128) {
        samples.sort_unstable();
        let last = samples.len().saturating_sub(1);
        let p50 = samples[last / 2];
        // Nearest rank: ceil(0.95 * n) - 1, clamped into range.
        // 最近秩：ceil(0.95 * n) - 1，并夹在有效范围内。
        let rank = ((95 * samples.len()).div_ceil(100)).saturating_sub(1);
        let p95 = samples[rank.min(last)];
        (p50, p95)
    }
    /// Records concurrent-read latency for plan §10: several connections reading one file database at once.
    /// 记录并发读取延迟，供方案 §10 使用：多个连接同时读取同一个文件数据库。
    #[test]
    #[ignore = "explicit §10 SQLite concurrent-read evidence"]
    fn performance_concurrent_reads_report_p50_p95() {
        const SAMPLES: usize = 20;
        const READERS: usize = 4;
        const ROWS: i64 = 2_000;

        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("concurrent.db");
        // Opened through the production path so the recorded numbers describe the configuration the application
        // actually runs — WAL, the v1 PRAGMA set and the real `busy_timeout`. Raw `Connection::open` would measure a
        // configuration nothing ships, which is what this test did before.
        // 经生产路径打开，使被记录的数字描述应用真正运行的配置——WAL、v1 PRAGMA 集合与真实的 `busy_timeout`。
        // 裸 `Connection::open` 会测量一个无人发布的配置，而本测试此前正是如此。
        let options = crate::runtime::SqliteOpenOptions::default();

        // Prepare the shared file database once, with enough rows for a read to be measurable.
        // 先备好该共享文件数据库，行数足以让一次读取可被测量。
        {
            let setup = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("open setup connection");
            setup
                .execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .expect("create table");
            let mut statement = setup
                .prepare("INSERT INTO items (id, name) VALUES (?1, ?2)")
                .expect("prepare insert");
            for index in 0..ROWS {
                statement
                    .execute(rusqlite::params![index, format!("row-{index}")])
                    .expect("insert row");
            }
        }

        // One more iteration than samples, so the first can be discarded as a warmup.
        // 比样本数多一次迭代，以便把首次作为预热丢弃。
        let mut slowest_micros = Vec::with_capacity(SAMPLES + 1);
        for _ in 0..=SAMPLES {
            let started = std::time::Instant::now();
            let handles = (0..READERS)
                .map(|_| {
                    let path = db_path.clone();
                    let options = options.clone();
                    std::thread::spawn(move || {
                        let connection = crate::runtime::open_sqlite_connection(&path, &options)
                            .expect("open reader connection");
                        let mut statement = connection
                            .prepare("SELECT COUNT(*) FROM items")
                            .expect("prepare read");
                        statement
                            .query_row([], |row| row.get::<_, i64>(0))
                            .expect("read rows")
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                let count = handle.join().expect("reader thread");
                assert_eq!(
                    count, ROWS,
                    "every concurrent reader must see the prepared rows"
                );
            }
            slowest_micros.push(started.elapsed().as_micros());
        }

        // The first iteration is a warmup: its work runs and warms the file, its sample is discarded.
        // 首次迭代为预热：其工作照常执行并预热该文件，其样本被丢弃。
        slowest_micros.remove(0);

        slowest_micros.sort_unstable();
        let last_index = slowest_micros.len().saturating_sub(1);
        let read_p50 = slowest_micros[last_index / 2];
        let rank = ((95 * slowest_micros.len()).div_ceil(100)).saturating_sub(1);
        let read_p95 = slowest_micros[rank.min(last_index)];
        let env_os = std::env::consts::OS;
        let env_arch = std::env::consts::ARCH;
        eprintln!(
            "[perf-sqlite-concurrent] samples={SAMPLES} readers={READERS} rows={ROWS} \
             read_p50_us={read_p50} read_p95_us={read_p95} peak_live_bytes={} env_os={env_os} env_arch={env_arch}",
            crate::allocation_evidence::high_water_bytes()
        );
    }

    /// Verifies concurrent autocommit writers serialise under the production configuration.
    /// 验证并发自动提交写入者在生产配置下会串行化。
    ///
    /// **This test previously opened raw `Connection::open` handles, which bypassed the runtime's whole PRAGMA set** —
    /// no WAL, no `busy_timeout`, no `synchronous` — so it passed even when `busy_timeout` was set to zero and the
    /// production configuration was broken. Verified by mutation, not argued. It now opens through
    /// `open_sqlite_connection`, so its claim and its subject finally match. Its distinct value is that it issues
    /// single autocommit statements rather than batches, which is the other write shape the application uses.
    /// **本测试此前打开裸 `Connection::open` 句柄，绕过了运行时的整套 PRAGMA**——没有 WAL、没有 `busy_timeout`、
    /// 没有 `synchronous`——因此在 `busy_timeout` 被设为 0、生产配置已被破坏时它仍然通过。这一点由变异验证，而非
    /// 论证。现在它经 `open_sqlite_connection` 打开，其主张与对象终于一致。它的独立价值在于发出的是单条自动提交
    /// 语句而非批次，那是应用使用的另一种写入形态。
    #[test]
    fn concurrent_writes_do_not_surface_database_is_locked() {
        const WRITERS: usize = 4;
        const ROWS_PER_WRITER: i64 = 50;

        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("concurrent-writes.db");
        let options = crate::runtime::SqliteOpenOptions::default();
        {
            let setup = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("open setup connection");
            setup
                .execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .expect("create table");
        }

        let handles = (0..WRITERS)
            .map(|writer| {
                let path = db_path.clone();
                let options = options.clone();
                std::thread::spawn(move || -> Result<(), String> {
                    let connection = crate::runtime::open_sqlite_connection(&path, &options)
                        .map_err(|error| error.to_string())?;
                    for index in 0..ROWS_PER_WRITER {
                        let id = (writer as i64) * ROWS_PER_WRITER + index;
                        connection
                            .execute(
                                "INSERT INTO items (id, name) VALUES (?1, ?2)",
                                rusqlite::params![id, format!("row-{id}")],
                            )
                            .map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>();

        let mut failures = Vec::new();
        for handle in handles {
            match handle.join().expect("writer thread") {
                Ok(()) => {}
                Err(detail) => failures.push(detail),
            }
        }

        assert!(
            failures.is_empty(),
            "no writer may fail, and none may report a host-side lock: {failures:?}"
        );

        let connection = crate::runtime::open_sqlite_connection(&db_path, &options)
            .expect("open verification connection");
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM items", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(
            rows,
            (WRITERS as i64) * ROWS_PER_WRITER,
            "every concurrent write must be durable"
        );
    }
    /// Verifies the production configuration survives concurrent batches, not only autocommit inserts.
    /// 验证生产配置能扛住并发批次，而不只是自动提交的单条插入。
    ///
    /// The neighbouring test opens raw `Connection::open` handles and issues single autocommit inserts, so it exercises
    /// neither the runtime's PRAGMA set (WAL, `busy_timeout`, `synchronous`) nor `execute_batch`'s explicit transaction.
    /// This test uses exactly the production path: `open_sqlite_connection` with the default options, then concurrent
    /// `execute_batch` calls, which is the combination the application actually runs and the only one whose contention
    /// behaviour matters.
    /// 相邻的那个测试打开裸 `Connection::open` 句柄并发出单条自动提交插入，因此它既不经过运行时的 PRAGMA 集合
    /// （WAL、`busy_timeout`、`synchronous`），也不经过 `execute_batch` 的显式事务。本测试恰好使用生产路径：
    /// 以默认选项经 `open_sqlite_connection` 打开，再并发执行 `execute_batch`——这才是应用真正运行的组合，也是
    /// 唯一其争用行为有意义的组合。
    #[test]
    fn concurrent_batches_survive_the_production_configuration() {
        const WRITERS: usize = 4;
        const BATCHES_PER_WRITER: usize = 25;
        const ROWS_PER_BATCH: usize = 20;

        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("concurrent-batches.db");
        let options = crate::runtime::SqliteOpenOptions::default();
        {
            let setup = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("open setup connection");
            setup
                .execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .expect("create table");
        }

        let handles = (0..WRITERS)
            .map(|writer| {
                let path = db_path.clone();
                let options = options.clone();
                std::thread::spawn(move || -> Result<(), String> {
                    let mut connection = crate::runtime::open_sqlite_connection(&path, &options)
                        .map_err(|error| error.to_string())?;
                    for batch in 0..BATCHES_PER_WRITER {
                        let base = ((writer * BATCHES_PER_WRITER + batch) * ROWS_PER_BATCH) as i64;
                        let params = (0..ROWS_PER_BATCH)
                            .map(|offset| {
                                let id = base + offset as i64;
                                vec![
                                    SqliteValue::Integer(id),
                                    SqliteValue::Text(format!("row-{id}")),
                                ]
                            })
                            .collect::<Vec<_>>();
                        execute_batch(
                            &mut connection,
                            "INSERT INTO items (id, name) VALUES (?, ?)",
                            &params,
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>();

        let mut failures = Vec::new();
        for handle in handles {
            match handle.join().expect("writer thread") {
                Ok(()) => {}
                Err(detail) => failures.push(detail),
            }
        }

        assert!(
            failures.is_empty(),
            "no concurrent batch may fail under the production configuration: {failures:?}"
        );

        let connection = crate::runtime::open_sqlite_connection(&db_path, &options)
            .expect("open verification connection");
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM items", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(
            rows,
            (WRITERS * BATCHES_PER_WRITER * ROWS_PER_BATCH) as i64,
            "every batch row must be durable"
        );
    }

    /// Reports where the production connection cost goes, as evidence rather than inference.
    /// 报告生产连接开销的构成，作为证据而非推断。
    ///
    /// Ignored by default: it asserts nothing about timing and prints its measurements. It exists because the
    /// concurrent-read numbers differ by an order of magnitude between a bare `Connection::open` and the production
    /// open path, and the fix depends on knowing which part dominates.
    /// 默认忽略：它不对耗时作断言，只打印测量值。它存在的原因是并发读的数字在裸 `Connection::open` 与生产打开
    /// 路径之间相差一个数量级，而修复取决于知道哪一部分占主导。
    #[test]
    #[ignore = "explicit connection-cost evidence, not a correctness check"]
    fn performance_connection_open_cost_breakdown() {
        const ROUNDS: usize = 50;

        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("open-cost.db");
        let options = crate::runtime::SqliteOpenOptions::default();
        {
            let setup = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("open setup connection");
            setup
                .execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .expect("create table");
        }

        let mut bare_micros = Vec::with_capacity(ROUNDS);
        let mut production_micros = Vec::with_capacity(ROUNDS);
        let mut dict_ddl_micros = Vec::with_capacity(ROUNDS);
        let mut tokenizer_micros = Vec::with_capacity(ROUNDS);
        let mut first_statement_micros = Vec::with_capacity(ROUNDS);
        let mut schema_select_micros = Vec::with_capacity(ROUNDS);
        let mut journal_mode_micros = Vec::with_capacity(ROUNDS);
        let mut peak_live_after = 0_usize;
        for _ in 0..ROUNDS {
            let started = std::time::Instant::now();
            let bare = Connection::open(&db_path).expect("bare open");
            bare_micros.push(started.elapsed().as_micros());
            drop(bare);

            let started = std::time::Instant::now();
            let production = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("production open");
            production_micros.push(started.elapsed().as_micros());
            peak_live_after = peak_live_after.max(crate::allocation_evidence::high_water_bytes());
            drop(production);

            // Component isolation on a bare connection, assembled from public entry points only, so this probe
            // needs no test-only back door inside production code.
            // 在裸连接上做组件隔离，全部由公开入口拼装，因此本探针不需要在生产代码里开仅测试用的后门。
            let staged = Connection::open(&db_path).expect("staged open");
            let started = std::time::Instant::now();
            crate::tokenizer::ensure_vulcan_dict_table(&staged).expect("dict table");
            dict_ddl_micros.push(started.elapsed().as_micros());

            let started = std::time::Instant::now();
            crate::tokenizer::ensure_jieba_tokenizer_registered(&staged).expect("tokenizer");
            tokenizer_micros.push(started.elapsed().as_micros());
            drop(staged);

            // Is the 651 µs the DDL, or the cost of the *first statement* on a fresh connection? Measured rather
            // than assumed, because the two answers demand completely different fixes: one targets the DDL, the
            // other targets issuing any statement at all during open.
            // 那 651 µs 是 DDL 本身，还是"新连接上第一条语句"的固有成本？此处实测而非假定，因为两个答案要求
            // 完全不同的修法：一个针对该 DDL，另一个针对"在打开阶段发出任何语句"这一行为。
            let probe = Connection::open(&db_path).expect("probe open");
            let started = std::time::Instant::now();
            let _: i64 = probe
                .query_row("SELECT 1", [], |row| row.get(0))
                .expect("trivial select");
            first_statement_micros.push(started.elapsed().as_micros());
            drop(probe);

            let probe = Connection::open(&db_path).expect("probe open");
            let started = std::time::Instant::now();
            let _: i64 = probe
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_vulcan_dict'",
                    [],
                    |row| row.get(0),
                )
                .expect("schema select");
            schema_select_micros.push(started.elapsed().as_micros());
            drop(probe);

            // Per-pragma cost. `journal_mode` is a persistent *database* property rather than a per-connection
            // setting, so re-applying it on every open is the prime suspect for the concurrency regression, and
            // this measures it rather than assuming it.
            // 逐条 PRAGMA 成本。`journal_mode` 是持久化的*数据库*属性而非按连接的设置，因此在每次打开时重新应用它
            // 是并发退化的首要嫌疑；此处实测而非假定。
            let probe = Connection::open(&db_path).expect("probe open");
            let _: i64 = probe
                .query_row("SELECT 1", [], |row| row.get(0))
                .expect("warm");
            let started = std::time::Instant::now();
            let _: String = probe
                .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
                .expect("journal mode");
            journal_mode_micros.push(started.elapsed().as_micros());
            drop(probe);
        }

        let (bare_p50, bare_p95) = percentile_micros(&mut bare_micros);
        let (production_p50, production_p95) = percentile_micros(&mut production_micros);
        let (dict_p50, dict_p95) = percentile_micros(&mut dict_ddl_micros);
        let (tokenizer_p50, tokenizer_p95) = percentile_micros(&mut tokenizer_micros);
        let (first_p50, first_p95) = percentile_micros(&mut first_statement_micros);
        let (schema_p50, schema_p95) = percentile_micros(&mut schema_select_micros);
        let (journal_p50, journal_p95) = percentile_micros(&mut journal_mode_micros);
        let env_os = std::env::consts::OS;
        eprintln!(
            "[perf-open] rounds={ROUNDS} bare_open_p50_us={bare_p50} bare_open_p95_us={bare_p95} \
             first_stmt_select1_p50_us={first_p50} first_stmt_select1_p95_us={first_p95} \
             schema_select_p50_us={schema_p50} schema_select_p95_us={schema_p95} \
             journal_mode_reapply_p50_us={journal_p50} journal_mode_reapply_p95_us={journal_p95} \
             dict_ddl_only_p50_us={dict_p50} dict_ddl_only_p95_us={dict_p95} \
             tokenizer_path_only_p50_us={tokenizer_p50} tokenizer_path_only_p95_us={tokenizer_p95} \
             production_open_p50_us={production_p50} production_open_p95_us={production_p95} \
             peak_live_bytes={peak_live_after} \
             cache_size_kib={} mmap_size_bytes={} wal_autocheckpoint_pages={} env_os={env_os}",
            options.pragmas.cache_size_kib,
            options.pragmas.mmap_size_bytes,
            options.pragmas.wal_autocheckpoint_pages
        );
    }

    /// Shows whether opening a connection needs the write lock, which decides whether readers run concurrently at all.
    /// 显示打开连接是否需要写锁，这决定了读取者究竟能否并发运行。
    ///
    /// The engine opens one connection per operation and never pools them, so whatever an open does happens once per
    /// statement — including on a pure read. If an open needs the write lock, one active writer blocks every other
    /// operation in the process for up to `busy_timeout`, and "concurrent reads" are not concurrent at all.
    /// 引擎每次操作打开一个连接且从不池化，因此打开时做的事在每个语句上都会发生一次——纯读取也不例外。若打开需要写锁，
    /// 那么一个活跃写入者会把进程中其他所有操作阻塞至多 `busy_timeout`，而"并发读"根本并不并发。
    #[test]
    fn opening_a_connection_does_not_wait_for_an_active_writer() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("open-lock.db");
        let options = crate::runtime::SqliteOpenOptions::default();
        {
            let setup = crate::runtime::open_sqlite_connection(&db_path, &options)
                .expect("open setup connection");
            setup
                .execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .expect("create table");
        }

        let writer = crate::runtime::open_sqlite_connection(&db_path, &options)
            .expect("open writer connection");
        writer
            .execute_batch("BEGIN IMMEDIATE TRANSACTION;")
            .expect("take the write lock");
        writer
            .execute("INSERT INTO items (id, name) VALUES (1, 'held')", [])
            .expect("write inside the held transaction");

        // The writer still holds the lock here, so this open either completes promptly or is waiting on it.
        // 此刻写入者仍持有锁，因此本次打开要么立即完成，要么正在等待该锁。
        let started = std::time::Instant::now();
        let reader = crate::runtime::open_sqlite_connection(&db_path, &options);
        let elapsed = started.elapsed();

        writer
            .execute_batch("ROLLBACK;")
            .expect("release the write lock");

        let Ok(reader) = reader else {
            panic!(
                "opening a connection must not require the write lock, but it failed while a writer held it"
            );
        };

        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "opening a connection must not wait for an active writer, but it took {elapsed:?}; the production \
             open path issues DDL, which needs the write lock, so every reader queues behind any writer"
        );
        drop(reader);
    }

    /// Opens an in-memory connection with one table ready for use.
    /// 打开一个内存连接并准备好一张表。
    fn open_test_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE items (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    weight REAL,
                    payload BLOB,
                    optional TEXT
                );",
            )
            .expect("create test table");
        connection
    }

    /// Verifies every identifier-quoting form SQLite accepts is counted as part of one statement.
    /// 验证 SQLite 接受的每一种标识符引用形式都被计为同一条语句的一部分。
    ///
    /// This exists because two of the four forms were missing, and the miss was not academic: SQLite creates and
    /// queries a table named `[a;b]` without complaint, so the scanner's miscount made the engine refuse a valid
    /// statement. The test drives the **public entry points**, not the scanner alone, because the harm is the refusal
    /// the caller sees rather than the number the scanner returns.
    /// 本测试存在的原因是四种形式中缺了两种，而这次遗漏并非纸上谈兵：SQLite 能毫无怨言地创建并查询名为 `[a;b]` 的表，
    /// 因此扫描器的误计使引擎拒绝了一条合法语句。本测试驱动的是**公开入口**而非仅扫描器，因为危害是调用方看到的拒绝，
    /// 而不是扫描器返回的那个数字。
    #[test]
    fn every_sqlite_identifier_quoting_form_counts_as_one_statement() {
        // All four forms, each holding a semicolon that must not be read as a separator.
        // 四种形式，每一种都含有一个不得被读作分隔符的分号。
        for sql in [
            "SELECT * FROM [a;b]",
            "SELECT * FROM \"a;b\"",
            "SELECT * FROM 'a;b'",
            "SELECT * FROM `a;b`",
        ] {
            assert_eq!(
                count_sql_statements(sql),
                1,
                "a semicolon inside a quoted identifier must not split the statement: {sql}"
            );
            assert!(
                !has_multiple_sql_statements(sql),
                "a single statement using a quoted identifier must not be refused: {sql}"
            );
        }

        // Genuine separation still has to be detected, or the barrier would be worthless.
        // 真正的分隔仍必须被检出，否则这道屏障毫无价值。
        for sql in [
            "SELECT 1; SELECT 2;",
            "SELECT * FROM [a;b]; SELECT 2;",
            "SELECT * FROM `a;b`; SELECT 2;",
        ] {
            assert!(
                has_multiple_sql_statements(sql),
                "two statements must still be refused: {sql}"
            );
        }

        // SQLite's own rule for these two forms, mirrored rather than assumed: the first closing character ends the
        // identifier and there is no escape inside it.
        // 这两种形式下 SQLite 自身的规则，此处是模仿而非假定：第一个闭合字符即结束标识符，其内部没有转义。
        assert_eq!(count_sql_statements("SELECT * FROM [a] ; SELECT 2;"), 2);
        assert_eq!(count_sql_statements("SELECT * FROM `a` ; SELECT 2;"), 2);

        // End to end through the public entries: the table really exists and the statement really runs.
        // 经公开入口端到端验证：表确实存在，语句确实执行。
        let mut connection = open_test_connection();
        connection
            .execute_batch("CREATE TABLE [odd;name] (value INTEGER);")
            .expect("sqlite must accept a bracket-quoted identifier containing a semicolon");
        connection
            .execute_batch("INSERT INTO [odd;name] (value) VALUES (7);")
            .expect("insert through the bracket-quoted identifier");

        execute_script(
            &mut connection,
            "UPDATE [odd;name] SET value = ?1 WHERE value = 7;",
            &[SqliteValue::Integer(9)],
        )
        .expect("execute_script must accept the bracket-quoted identifier");

        let result = query_json(&connection, "SELECT value FROM [odd;name]", &[])
            .expect("query_json must accept the bracket-quoted identifier");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&result.json_data).expect("decode query_json payload");
        assert_eq!(result.row_count, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("value").and_then(serde_json::Value::as_i64),
            Some(9)
        );
    }

    /// Verifies scalar JSON values map onto the SQL value type.
    /// 验证 JSON 标量映射到 SQL 值类型。
    #[test]
    fn json_scalars_map_to_sql_values() {
        assert_eq!(
            SqliteValue::from_json(serde_json::json!(null)).expect("null"),
            SqliteValue::Null
        );
        assert_eq!(
            SqliteValue::from_json(serde_json::json!(true)).expect("bool"),
            SqliteValue::Integer(1)
        );
        assert_eq!(
            SqliteValue::from_json(serde_json::json!(-7)).expect("int"),
            SqliteValue::Integer(-7)
        );
        assert_eq!(
            SqliteValue::from_json(serde_json::json!(1.5)).expect("real"),
            SqliteValue::Real(1.5)
        );
        assert_eq!(
            SqliteValue::from_json(serde_json::json!("text")).expect("text"),
            SqliteValue::Text("text".to_string())
        );
    }

    /// Verifies non-scalar JSON parameters are refused rather than stringified.
    /// 验证非标量 JSON 参数被拒绝而不是转成字符串。
    #[test]
    fn json_containers_are_refused_as_parameters() {
        assert!(SqliteValue::from_json(serde_json::json!([1, 2])).is_err());
        assert!(SqliteValue::from_json(serde_json::json!({ "a": 1 })).is_err());
    }

    /// Verifies an unsigned integer beyond the signed range is refused.
    /// 验证超出有符号范围的无符号整数被拒绝。
    #[test]
    fn oversized_unsigned_integer_is_refused() {
        let value = serde_json::json!(u64::MAX);
        assert!(SqliteValue::from_json(value).is_err());
    }

    /// Verifies a non-finite real is refused instead of being silently stored as NULL.
    /// 验证非有限实数被拒绝，而不是被静默存为 NULL。
    ///
    /// SQLite stores a bound NaN as NULL, so without this refusal a caller that computed a NaN would write a value
    /// it never asked for and never be told. The second assertion proves the connection is still usable afterwards.
    /// SQLite 会把绑定的 NaN 存为 NULL，因此缺少此拒绝时，算出 NaN 的调用方会写入一个它从未要求的值且永远不会被告知。
    /// 第二条断言证明之后连接仍然可用。
    #[test]
    fn a_non_finite_real_is_refused() {
        let mut connection = open_test_connection();
        let error = execute_script(
            &mut connection,
            "INSERT INTO items (id, name) VALUES (?1, ?2)",
            &[
                SqliteValue::Real(f64::NAN),
                SqliteValue::Text("nan".to_string()),
            ],
        )
        .expect_err("a NaN parameter must be refused");
        assert!(
            error.to_string().contains("non-finite"),
            "the refusal must say what was wrong: {error}"
        );

        query_json(&connection, "SELECT id FROM items", &[])
            .expect("the connection must remain usable after a refused parameter");
    }

    /// Verifies the JSON parameter list parser handles empty, array and non-array input.
    /// 验证 JSON 参数列表解析器处理空、数组与非数组输入。
    #[test]
    fn json_parameter_list_parsing() {
        assert_eq!(
            SqliteValue::list_from_json("   ").expect("blank"),
            Vec::<SqliteValue>::new()
        );

        let parsed = SqliteValue::list_from_json(r#"[1, "two", null]"#).expect("array");
        assert_eq!(
            parsed,
            vec![
                SqliteValue::Integer(1),
                SqliteValue::Text("two".to_string()),
                SqliteValue::Null
            ]
        );

        assert!(SqliteValue::list_from_json(r#"{"a":1}"#).is_err());
        assert!(SqliteValue::list_from_json("not json").is_err());
    }

    /// Verifies the documented JSON mapping, including the deliberate blob asymmetry.
    /// 验证已记录的 JSON 映射，包括刻意的 Blob 非对称性。
    #[test]
    fn json_mapping_is_documented_and_stable() {
        assert_eq!(SqliteValue::Null.to_json(), serde_json::json!(null));
        assert_eq!(SqliteValue::Integer(3).to_json(), serde_json::json!(3));
        assert_eq!(
            SqliteValue::Text("x".to_string()).to_json(),
            serde_json::json!("x")
        );
        assert_eq!(
            SqliteValue::Blob(vec![1, 255]).to_json(),
            serde_json::json!([1, 255])
        );

        // Non-finite floats degrade to strings because JSON cannot represent them.
        // 非有限浮点降级为字符串，因为 JSON 无法表示它们。
        assert_eq!(
            SqliteValue::Real(f64::NAN).to_json(),
            serde_json::json!("NaN")
        );

        // Blob → array is not accepted back as a parameter; this asymmetry is intentional and
        // matches the migrated source.
        // Blob → 数组不可作为参数反向接受；该非对称性是有意保留的，与迁入源码一致。
        let as_json = SqliteValue::Blob(vec![1, 2]).to_json();
        assert!(SqliteValue::from_json(as_json).is_err());
    }

    /// Verifies a parameterized statement binds and returns its change count.
    /// 验证参数化语句完成绑定并返回影响行数。
    #[test]
    fn execute_script_binds_parameters() {
        let mut connection = open_test_connection();

        let result = execute_script(
            &mut connection,
            "INSERT INTO items (name, weight) VALUES (?1, ?2)",
            &[
                SqliteValue::Text("alpha".to_string()),
                SqliteValue::Real(2.5),
            ],
        )
        .expect("execute parameterized insert");

        assert!(result.success);
        assert_eq!(result.rows_changed, 1);
        assert!(result.last_insert_rowid > 0);
    }

    /// Verifies an unparameterized multi-statement script runs through `execute_batch`.
    /// 验证无参数的多语句脚本通过 `execute_batch` 执行。
    #[test]
    fn execute_script_runs_multi_statement_scripts() {
        let mut connection = open_test_connection();

        execute_script(
            &mut connection,
            "INSERT INTO items (name) VALUES ('one'); INSERT INTO items (name) VALUES ('two');",
            &[],
        )
        .expect("execute script");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 2);
    }

    /// Verifies bound parameters are refused for multi-statement input.
    /// 验证多语句输入时绑定参数被拒绝。
    #[test]
    fn execute_script_refuses_parameters_for_multiple_statements() {
        let mut connection = open_test_connection();

        let error = execute_script(
            &mut connection,
            "INSERT INTO items (name) VALUES (?1); INSERT INTO items (name) VALUES (?1);",
            &[SqliteValue::Text("x".to_string())],
        )
        .expect_err("multi-statement input with parameters must be refused");

        assert!(matches!(error, SqliteEngineError::InvalidArgument { .. }));
    }

    /// Verifies an empty SQL string is refused.
    /// 验证空 SQL 字符串被拒绝。
    #[test]
    fn empty_sql_is_refused() {
        let mut connection = open_test_connection();
        assert!(execute_script(&mut connection, "   ", &[]).is_err());
    }

    /// Verifies a batch commits all items and reports totals.
    /// 验证批次提交全部项并报告合计。
    #[test]
    fn execute_batch_commits_all_items() {
        let mut connection = open_test_connection();

        let result = execute_batch(
            &mut connection,
            "INSERT INTO items (name, weight) VALUES (?1, ?2)",
            &[
                vec![SqliteValue::Text("a".to_string()), SqliteValue::Integer(1)],
                vec![SqliteValue::Text("b".to_string()), SqliteValue::Integer(2)],
                vec![SqliteValue::Text("c".to_string()), SqliteValue::Integer(3)],
            ],
        )
        .expect("execute batch");

        assert!(result.success);
        assert_eq!(result.statements_executed, 3);
        assert_eq!(result.rows_changed, 3);

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 3);
    }

    /// Verifies a failing batch item rolls the entire batch back and names the failing index.
    /// 验证批次中某项失败会回滚整个批次并指出失败下标。
    #[test]
    fn execute_batch_rolls_back_on_failure_and_names_index() {
        let mut connection = open_test_connection();

        let error = execute_batch(
            &mut connection,
            "INSERT INTO items (id, name) VALUES (?1, ?2)",
            &[
                vec![
                    SqliteValue::Integer(1),
                    SqliteValue::Text("first".to_string()),
                ],
                // Duplicate primary key: this item must fail.
                // 主键重复：该项必然失败。
                vec![
                    SqliteValue::Integer(1),
                    SqliteValue::Text("duplicate".to_string()),
                ],
            ],
        )
        .expect_err("duplicate primary key must fail the batch");

        match error {
            SqliteEngineError::BatchItem { index, .. } => assert_eq!(index, 1),
            other => panic!("expected BatchItem, got {other:?}"),
        }

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 0, "a failed batch must leave no rows behind");
    }

    /// Verifies an empty batch is refused.
    /// 验证空批次被拒绝。
    #[test]
    fn empty_batch_is_refused() {
        let mut connection = open_test_connection();
        assert!(
            execute_batch(&mut connection, "INSERT INTO items (name) VALUES (?1)", &[]).is_err()
        );
    }

    /// Verifies JSON query output covers nulls, integers, floats, text and blobs.
    /// 验证 JSON 查询输出覆盖空值、整数、浮点、文本与二进制。
    #[test]
    fn query_json_covers_all_scalar_kinds() {
        let mut connection = open_test_connection();

        execute_script(
            &mut connection,
            "INSERT INTO items (id, name, weight, payload, optional) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[
                SqliteValue::Integer(1),
                SqliteValue::Text("alpha".to_string()),
                SqliteValue::Real(1.25),
                SqliteValue::Blob(vec![0, 255]),
                SqliteValue::Null,
            ],
        )
        .expect("insert row");

        let result = query_json(
            &connection,
            "SELECT id, name, weight, payload, optional FROM items",
            &[],
        )
        .expect("query json");

        assert_eq!(result.row_count, 1);

        let parsed: serde_json::Value =
            serde_json::from_str(&result.json_data).expect("parse json result");
        let row = &parsed[0];
        assert_eq!(row["id"], serde_json::json!(1));
        assert_eq!(row["name"], serde_json::json!("alpha"));
        assert_eq!(row["weight"], serde_json::json!(1.25));
        assert_eq!(row["payload"], serde_json::json!([0, 255]));
        assert_eq!(row["optional"], serde_json::json!(null));
    }

    /// Verifies a JSON query binds parameters and refuses multi-statement input.
    /// 验证 JSON 查询可绑定参数并拒绝多语句输入。
    #[test]
    fn query_json_binds_parameters_and_refuses_multiple_statements() {
        let mut connection = open_test_connection();

        execute_script(
            &mut connection,
            "INSERT INTO items (name) VALUES (?1)",
            &[SqliteValue::Text("needle".to_string())],
        )
        .expect("insert");

        let result = query_json(
            &connection,
            "SELECT name FROM items WHERE name = ?1",
            &[SqliteValue::Text("needle".to_string())],
        )
        .expect("query");
        assert_eq!(result.row_count, 1);

        assert!(
            query_json(&connection, "SELECT 1; SELECT 2;", &[]).is_err(),
            "multi-statement JSON queries must be refused"
        );
    }

    /// Verifies bounded queries stop at the first crossed limit and match unbounded encoding otherwise.
    /// 验证有界查询在首次越过上限时停止，未越限时与无界编码完全一致。
    #[test]
    fn query_json_bounded_enforces_row_and_byte_limits_while_reading() {
        let mut connection = open_test_connection();
        for name in ["a", "b", "c"] {
            execute_script(
                &mut connection,
                "INSERT INTO items (name) VALUES (?1)",
                &[SqliteValue::Text(name.to_string())],
            )
            .expect("insert");
        }
        let sql = "SELECT name FROM items ORDER BY name";
        let unbounded = query_json(&connection, sql, &[]).expect("unbounded query");
        let expected = serde_json::to_string(&serde_json::json!([
            {"name": "a"}, {"name": "b"}, {"name": "c"}
        ]))
        .expect("encode expected rows");
        assert_eq!(unbounded.json_data, expected);
        assert_eq!(unbounded.row_count, 3);

        let exact = query_json_bounded(
            &connection,
            sql,
            &[],
            QueryJsonLimits {
                max_rows: 3,
                max_bytes: expected.len(),
            },
        )
        .expect("limits equal to the result size must pass");
        assert_eq!(exact.json_data, expected);

        let rows_error = query_json_bounded(
            &connection,
            sql,
            &[],
            QueryJsonLimits {
                max_rows: 2,
                max_bytes: usize::MAX,
            },
        )
        .expect_err("row limit must be enforced");
        assert!(matches!(
            rows_error,
            SqliteEngineError::ResultLimitExceeded { .. }
        ));

        let bytes_error = query_json_bounded(
            &connection,
            sql,
            &[],
            QueryJsonLimits {
                max_rows: u64::MAX,
                max_bytes: expected.len() - 1,
            },
        )
        .expect_err("byte limit must be enforced");
        assert!(matches!(
            bytes_error,
            SqliteEngineError::ResultLimitExceeded { .. }
        ));

        let empty = query_json_bounded(
            &connection,
            "SELECT name FROM items WHERE name = 'missing'",
            &[],
            QueryJsonLimits {
                max_rows: 0,
                max_bytes: 2,
            },
        )
        .expect("empty result fits the smallest limits");
        assert_eq!(empty.json_data, "[]");
        assert_eq!(empty.row_count, 0);
    }

    /// Verifies statement counting ignores separators inside literals and comments.
    /// 验证语句统计忽略字面量与注释内部的分隔符。
    #[test]
    fn statement_counting_ignores_literals_and_comments() {
        assert_eq!(count_sql_statements("SELECT 1"), 1);
        assert_eq!(count_sql_statements("SELECT 1;"), 1);
        assert_eq!(count_sql_statements("SELECT 1; SELECT 2;"), 2);
        assert_eq!(count_sql_statements("SELECT ';'"), 1);
        assert_eq!(count_sql_statements("SELECT \";\""), 1);
        assert_eq!(count_sql_statements("SELECT 1 -- ; ignored\n"), 1);
        assert_eq!(count_sql_statements("SELECT 1 /* ; ignored */"), 1);
        assert_eq!(count_sql_statements("   ;  ;  "), 0);
        assert_eq!(count_sql_statements(""), 0);

        assert!(!has_multiple_sql_statements("SELECT ';'"));
        assert!(has_multiple_sql_statements("SELECT 1; SELECT 2;"));
    }

    /// Verifies an escaped quote inside a literal does not end the literal early.
    /// 验证字面量内的转义引号不会提前结束字面量。
    #[test]
    fn statement_counting_handles_escaped_quotes() {
        assert_eq!(count_sql_statements("SELECT 'it''s ; here'"), 1);
        assert_eq!(count_sql_statements("SELECT \"a\"\"b ; c\""), 1);
    }

    /// Lists this process's query-stream spool files, which are what a leak leaves behind.
    /// 列出本进程的查询流暂存文件，泄漏会留下它们。
    fn spool_files() -> Vec<std::path::PathBuf> {
        let prefix = format!("vulcan-sqlite-query-stream-{}-", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .expect("the temporary directory must be readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
            })
            .collect()
    }

    /// Verifies a failed query stream leaves no spool file behind.
    /// 验证失败的查询流不留下暂存文件。
    ///
    /// The writer is created before the query runs, so every error path — a missing table, a conversion failure, a
    /// failing sink — would otherwise leave one temporary file per attempt. Handing the file to a
    /// [`QueryStreamResult`] is the only cleanup path the success case uses, and an error never reaches it.
    /// 写入器在查询执行之前创建，因此每个错误路径——表不存在、转换失败、接收端失败——否则都会每次尝试留下一个
    /// 临时文件。把文件交给 [`QueryStreamResult`] 是成功路径使用的唯一清理途径，而错误永远不会到达它。
    #[test]
    fn a_failed_query_stream_leaves_no_spool_file() {
        let mut connection = open_test_connection();
        let before = spool_files().len();

        query_stream(
            &mut connection,
            "SELECT * FROM missing_table",
            &[],
            DEFAULT_IPC_CHUNK_BYTES,
        )
        .expect_err("a missing table must fail the stream");

        assert_eq!(
            spool_files().len(),
            before,
            "a failed query stream must not leave its spool file behind"
        );
    }

    /// Verifies a type change after the first row batch fails instead of emitting NULL data.
    /// 验证首个行批次后的类型变化会失败，而不是发出 NULL 数据。
    #[test]
    fn later_query_stream_type_change_is_rejected_without_a_spool_leak() {
        let mut connection = open_test_connection();
        let before = spool_files().len();
        let error = query_stream(
            &mut connection,
            "WITH RECURSIVE sequence(value) AS (
                 SELECT 1
                 UNION ALL
                 SELECT value + 1 FROM sequence WHERE value < 1000
             )
             SELECT value AS mixed_value FROM sequence
             UNION ALL
             SELECT 'late-text'",
            &[],
            DEFAULT_IPC_CHUNK_BYTES,
        )
        .expect_err("a later incompatible SQLite value must fail the stream");

        assert!(
            error.to_string().contains("changed SQLite value type"),
            "unexpected error: {error}"
        );
        assert_eq!(
            spool_files().len(),
            before,
            "a type-change failure must not leave its partial spool file behind"
        );
    }

    /// Verifies a spooled query stream decodes back into the expected Arrow data, and that the
    /// spool file is removed when the result is dropped.
    /// 验证暂存的查询流可解码回预期的 Arrow 数据，且结果被丢弃时暂存文件被删除。
    #[test]
    fn query_stream_spools_decodable_arrow_ipc_and_cleans_up() {
        let mut connection = open_test_connection();

        execute_batch(
            &mut connection,
            "INSERT INTO items (id, name) VALUES (?1, ?2)",
            &(0..5)
                .map(|index| {
                    vec![
                        SqliteValue::Integer(index),
                        SqliteValue::Text(format!("row-{index}")),
                    ]
                })
                .collect::<Vec<_>>(),
        )
        .expect("insert rows");

        let stream = query_stream(
            &mut connection,
            "SELECT id, name FROM items ORDER BY id",
            &[],
            DEFAULT_IPC_CHUNK_BYTES,
        )
        .expect("query stream");

        assert_eq!(stream.row_count, 5);
        assert!(
            stream.chunk_count > 0,
            "a stream must emit at least one chunk"
        );

        let spool_path = stream.spool_path().to_path_buf();
        assert!(
            spool_path.exists(),
            "spool file must exist while the result lives"
        );

        // Concatenating the chunks is exactly what a downstream consumer does, so decoding the
        // result is the real verification that the emitted bytes are valid Arrow IPC.
        // 拼接分块正是下游消费方所做的事，因此解码结果是"已发出字节确为合法 Arrow IPC"的真正验证。
        let mut buffer = Vec::new();
        for index in 0..stream.chunk_count as usize {
            buffer.extend_from_slice(&stream.read_chunk(index).expect("read chunk"));
        }
        assert_eq!(
            u64::try_from(buffer.len()).expect("buffer length fits u64"),
            stream.total_bytes,
            "reported total_bytes must equal the bytes actually stored"
        );

        let reader =
            StreamReader::try_new(Cursor::new(buffer), None).expect("open the Arrow IPC stream");
        let schema = reader.schema();
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(1).name(), "name");
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);

        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .expect("decode the record batches");
        let decoded_rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(decoded_rows, 5, "every row must survive the round trip");

        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("id column is Int64");
        let names = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .expect("name column is Utf8");
        assert_eq!(ids.value(0), 0);
        assert_eq!(ids.value(4), 4);
        assert_eq!(names.value(0), "row-0");
        assert_eq!(names.value(4), "row-4");

        assert!(
            matches!(
                stream.read_chunk(stream.chunk_count as usize),
                Err(SqliteEngineError::QueryStreamState { .. })
            ),
            "reading past the last chunk must report query stream state"
        );

        drop(stream);
        assert!(
            !spool_path.exists(),
            "dropping the stream must remove its spool file"
        );
    }

    /// Verifies a stream with no rows reports zero chunks and no rows.
    /// 验证无行的流报告零分块与零行。
    #[test]
    fn query_stream_with_no_rows_reports_zero() {
        let mut connection = open_test_connection();

        let stream = query_stream(
            &mut connection,
            "SELECT id, name FROM items",
            &[],
            DEFAULT_IPC_CHUNK_BYTES,
        )
        .expect("query stream");

        assert_eq!(stream.row_count, 0);
        assert_eq!(stream.chunk_count, 0);
        assert_eq!(stream.total_bytes, 0);
    }

    /// Verifies the host can supply its own chunk sink instead of the temporary directory.
    /// 验证宿主可以提供自己的分块接收端，而不是使用临时目录。
    #[test]
    fn query_stream_with_writer_uses_the_supplied_sink() {
        let mut connection = open_test_connection();

        execute_script(
            &mut connection,
            "INSERT INTO items (id, name) VALUES (?1, ?2)",
            &[
                SqliteValue::Integer(1),
                SqliteValue::Text("sink".to_string()),
            ],
        )
        .expect("insert row");

        let (collector, metrics) = query_stream_with_writer(
            &mut connection,
            "SELECT id, name FROM items",
            &[],
            ChunkCollector::new(DEFAULT_IPC_CHUNK_BYTES),
        )
        .expect("stream into collector");

        assert_eq!(metrics.row_count, 1);
        assert!(metrics.chunk_count > 0);
        assert_eq!(metrics.total_bytes, collector.emitted_total_bytes());

        let buffer = collector.chunks().concat();
        assert_eq!(
            u64::try_from(buffer.len()).expect("buffer length fits u64"),
            metrics.total_bytes
        );

        let reader =
            StreamReader::try_new(Cursor::new(buffer), None).expect("open the Arrow IPC stream");
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .expect("decode the record batches");
        let decoded_rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(
            decoded_rows, 1,
            "the supplied sink must receive real Arrow data"
        );

        let names = batches[0]
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .expect("name column is Utf8");
        assert_eq!(names.value(0), "sink");
    }

    /// Verifies a query stream refuses multi-statement input.
    /// 验证查询流拒绝多语句输入。
    #[test]
    fn query_stream_refuses_multiple_statements() {
        let mut connection = open_test_connection();
        assert!(
            query_stream(
                &mut connection,
                "SELECT 1; SELECT 2;",
                &[],
                DEFAULT_IPC_CHUNK_BYTES
            )
            .is_err()
        );
    }
}

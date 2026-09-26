//! In-process SQL helpers that replace controller round trips.
//! 取代控制器往返的进程内 SQL 辅助函数。
//!
//! Every helper here takes the same arguments the controller client took — a logical database, a
//! SQL string and a parameter list — so migrating a call site is a change of callee rather than a
//! rewrite of its arguments. That is deliberate: the alternative is 253 hand-written rewrites, and
//! hand rewrites are where argument-order and dropped-`?` mistakes hide.
//!
//! 此处每个辅助函数接受与控制器客户端相同的参数——一个逻辑数据库、一段 SQL 与一个参数列表——因此迁移
//! 调用点只是更换被调用者，而不是重写其参数。这是刻意的：另一种做法是 253 次手工重写，而参数顺序与漏掉
//! `?` 的错误正是藏匿在手工重写之中。
//!
//! The first parameter is the runtime, not the whole host handle: a subsystem that owns a
//! [`VmmDataPlaneHandle`](crate::vmm_data_plane::VmmDataPlaneHandle)-style handle holds the runtime
//! but must not hold the retiring controller client, so requiring the runtime keeps that boundary
//! enforced by the type checker instead of by convention.
//!
//! 第一个参数是运行时而不是整个宿主句柄：持有 [`VmmDataPlaneHandle`](crate::vmm_data_plane::VmmDataPlaneHandle)
//! 式句柄的子系统持有运行时，但不得持有正在退役的控制器客户端，因此要求传入运行时使该边界由类型检查
//! 器而非约定来强制。
//!
//! Connection lifetime / 连接生命周期:
//!
//! Each helper owns an isolated connection; related reads share [`with_read_snapshot_async`].
//! 每个辅助函数持有独立连接；相关读取通过 [`with_read_snapshot_async`] 共享快照。
//! Mixed read/write sequences use [`with_connection_async`] with an explicit transaction.
//! 混合读写序列使用 [`with_connection_async`] 与显式事务。
//! Connections are not pooled across callers, keeping temporary state and interrupts isolated.
//! 连接不跨调用方复用，避免临时状态及中断互相影响。
//!
//! [`with_connection`] is the one entry point that must never be called from asynchronous code directly:
//! it runs a whole sequence synchronously, so it belongs inside a blocking task started through
//! [`LocalDatabaseRuntime::boundary`], which is also what holds that database's gate for the sequence.
//! [`with_connection`] 是唯一绝不可从异步代码直接调用的入口：它同步运行整个序列，因此属于通过
//! [`LocalDatabaseRuntime::boundary`] 启动的阻塞任务内部，而那也正是为整个序列持有该数据库门的机制。
//!
//! Error type / 错误类型:
//!
//! Helpers return `Result<_, String>` because the call sites they replace already produced
//! user-facing Chinese messages with `format!` and `map_err`. Returning the engine error here would
//! force every call site to re-render it, which is churn without benefit; the engine error is
//! rendered into the message with `Display`.
//!
//! 辅助函数返回 `Result<_, String>`，因为它们所取代的调用点本就用 `format!` 与 `map_err` 产出面向
//! 用户的中文消息。在此返回引擎错误会迫使每个调用点重新渲染，是无收益的改动；引擎错误经 `Display`
//! 渲染进消息。

use std::path::PathBuf;
use std::sync::Arc;

use vulcan_sqlite_engine::fts::FtsMutationResult;
use vulcan_sqlite_engine::fts::{
    self, EnsureFtsIndexResult, RebuildFtsIndexResult, SearchFtsResult,
};
use vulcan_sqlite_engine::sql_exec::{
    self, ChunkCollector, ExecuteBatchResult, ExecuteScriptResult, QueryJsonResult, SqliteValue,
};
pub use vulcan_sqlite_engine::sql_exec::{
    DEFAULT_IPC_CHUNK_BYTES, QueryJsonLimits, QueryStreamResult,
};
pub use vulcan_sqlite_engine::tokenizer::TokenizerMode;
use vulcan_sqlite_engine::tokenizer::{
    self, DictionaryMutationResult, ListCustomWordsResult, TokenizeOutput,
};

use crate::local_database_runtime::{
    BlockingBoundaryError, DatabaseKey, LocalDatabaseError, LocalDatabaseRuntime, LogicalDatabase,
    SqliteFileBinding,
};

/// Renders a local-database failure into a call-site message.
/// 将本地数据库失败渲染为调用点消息。
fn render(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Selects which database one helper operates on.
/// 选择某个辅助函数作用于哪个数据库。
///
/// The two cases are not interchangeable and are therefore not unified behind a name. A
/// [`LogicalDatabase`] is one of the host's **singular** databases, whose path the host bootstrap
/// derived; a [`SqliteFileBinding`] is a file an instance-scoped subsystem derived for itself, such
/// as one knowledge library's database. Keeping them distinct means a call site states which kind of
/// resource it addresses rather than passing a value that could be either.
///
/// 两种情况不可互换，因此不用一个名字统一。[`LogicalDatabase`] 是宿主的**单例**数据库之一，其路径由
/// 宿主引导派生；[`SqliteFileBinding`] 是实例作用域子系统为自身派生的文件，例如某个知识库的资料库数据库。
/// 保持区分意味着调用点会表明自己在寻址哪一类资源，而不是传一个两者皆可的值。
#[derive(Clone, Copy)]
pub enum DatabaseTarget<'a> {
    /// One of the host's singular logical databases.
    /// 宿主单例逻辑数据库之一。
    Logical(LogicalDatabase),
    /// One declared database file owned by an instance-scoped subsystem.
    /// 实例作用域子系统持有的一个已声明数据库文件。
    File(&'a SqliteFileBinding),
}

impl From<LogicalDatabase> for DatabaseTarget<'_> {
    /// Addresses a singular logical database.
    /// 寻址一个单例逻辑数据库。
    fn from(logical: LogicalDatabase) -> Self {
        Self::Logical(logical)
    }
}

impl<'a> From<&'a SqliteFileBinding> for DatabaseTarget<'a> {
    /// Addresses one declared database file.
    /// 寻址一个已声明数据库文件。
    fn from(binding: &'a SqliteFileBinding) -> Self {
        Self::File(binding)
    }
}

/// One database addressed by an owned value, so it can cross into a blocking task.
/// 以所有权值寻址的一个数据库，因此可以跨入阻塞任务。
///
/// [`DatabaseTarget`] borrows a file binding, and a blocking task must own everything it touches, so the
/// asynchronous entry points take this form instead. Constructing it is the only extra step those entry
/// points require, and it keeps the borrowed form for callers that never leave their own stack.
/// [`DatabaseTarget`] 借用文件绑定，而阻塞任务必须拥有它所触及的一切，因此异步入口改用本形态。构造它正是
/// 那些入口唯一多出的一步，同时为不离栈的调用方保留借用形态。
#[derive(Clone, Debug)]
pub enum OwnedDatabaseTarget {
    /// One of the host's singular logical databases.
    /// 宿主单例逻辑数据库之一。
    Logical(LogicalDatabase),
    /// One declared database file owned by the caller.
    /// 调用方持有的一个已声明数据库文件。
    File(SqliteFileBinding),
}

impl OwnedDatabaseTarget {
    /// Returns the normalized physical database this target names, which is what the boundary gates on.
    /// 返回本目标命名的规范化物理数据库，也就是边界取门所依据的对象。
    ///
    /// # Returns / 返回值
    /// Returns the normalized file path, or a binding/path-validation error.
    /// 返回规范化文件路径，或绑定／路径校验错误。
    pub fn key(&self, runtime: &LocalDatabaseRuntime) -> Result<DatabaseKey, String> {
        let path = match self {
            Self::Logical(logical) => runtime.path_for(*logical).map_err(render)?,
            Self::File(binding) => binding.path.clone(),
        };
        vulcan_sqlite_engine::runtime::normalize_db_path(&path)
            .map(DatabaseKey::File)
            .map_err(render)
    }

    /// Returns the borrowed target this owned target stands for.
    /// 返回本所有权目标所代表的借用目标。
    fn as_target(&self) -> DatabaseTarget<'_> {
        match self {
            Self::Logical(logical) => DatabaseTarget::Logical(*logical),
            Self::File(binding) => DatabaseTarget::File(binding),
        }
    }
}

impl From<LogicalDatabase> for OwnedDatabaseTarget {
    /// Addresses a singular logical database.
    /// 寻址一个单例逻辑数据库。
    fn from(logical: LogicalDatabase) -> Self {
        Self::Logical(logical)
    }
}

impl From<SqliteFileBinding> for OwnedDatabaseTarget {
    /// Addresses one declared database file by ownership.
    /// 以所有权寻址一个已声明数据库文件。
    fn from(binding: SqliteFileBinding) -> Self {
        Self::File(binding)
    }
}

impl From<&SqliteFileBinding> for OwnedDatabaseTarget {
    /// Addresses one declared database file by cloning it across the blocking boundary.
    /// 通过克隆跨过阻塞边界来寻址一个已声明数据库文件。
    fn from(binding: &SqliteFileBinding) -> Self {
        Self::File(binding.clone())
    }
}

/// Renders a blocking-boundary failure into the string error the synchronous entry points also use.
/// 把阻塞边界失败渲染为同步入口同样使用的字符串错误。
fn render_boundary_failure(error: BlockingBoundaryError<String>) -> String {
    match error {
        BlockingBoundaryError::Operation(inner) => inner,
        BlockingBoundaryError::Task { detail } => {
            format!("blocking database operation did not run to completion: {detail}")
        }
    }
}

/// Runs one script on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上运行一条脚本，离开 Tokio 核心线程。
///
/// This is the entry point every asynchronous caller should use. The synchronous [`execute_script`] remains
/// for code that already runs inside the boundary and must not take the gate twice.
/// 这是每个异步调用方都应使用的入口。同步的 [`execute_script`] 保留给已在边界内运行、且不得重复取门的代码。
///
/// # Parameters / 参数
/// `runtime` is shared so the blocking task can own a handle to it; `target` names the database; `sql` and
/// `params` are moved into the task.
/// `runtime` 以共享方式传入，使阻塞任务能拥有它的句柄；`target` 命名数据库；`sql` 与 `params` 被移入任务。
///
/// # Returns / 返回值
/// Returns the engine's script result, or the operation's or boundary's error.
/// 返回引擎的脚本结果，或操作本身或边界的错误。
pub async fn execute_script_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: impl Into<String>,
    params: Vec<SqliteValue>,
) -> Result<ExecuteScriptResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let sql = sql.into();
    boundary
        .run(key, move || {
            execute_script(shared.as_ref(), target.as_target(), &sql, &params)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Runs one parameter batch on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上运行一组参数批次，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared so the blocking task can own a handle to it; `target` names the database; `sql` and
/// `batch_params` are moved into the task.
/// `runtime` 以共享方式传入，使阻塞任务能拥有它的句柄；`target` 命名数据库；`sql` 与 `batch_params` 被移入任务。
///
/// # Returns / 返回值
/// Returns the engine's batch result, or the operation's or boundary's error.
/// 返回引擎的批次结果，或操作本身或边界的错误。
pub async fn execute_batch_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: impl Into<String>,
    batch_params: Vec<Vec<SqliteValue>>,
) -> Result<ExecuteBatchResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let sql = sql.into();
    boundary
        .run(key, move || {
            execute_batch(shared.as_ref(), target.as_target(), &sql, &batch_params)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Run a bounded multi-statement operation with one connection and one physical database gate.
/// 使用一个连接和一道物理数据库闸门执行有界多语句操作。
/// `runtime` and `target` identify the database; `body` owns its inputs and may start a transaction.
/// `runtime` 和 `target` 标识数据库；`body` 持有输入且可开启事务。返回操作结果或明确错误。
pub async fn with_connection_async<T: Send + 'static>(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    body: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    // Resolve the same canonical gate used by ordinary reads, writes and shutdown.
    // 解析普通读取、写入与关闭共同使用的规范闸门。
    let target = target.into();
    // Preserve the physical identity before moving the owned target into the blocking call.
    // 在将目标所有权移入阻塞调用之前保留物理身份。
    let key = target.key(runtime.as_ref())?;
    // Retain the runtime while the blocking operation owns its connection.
    // 阻塞操作持有连接期间保留运行时。
    let shared = Arc::clone(runtime);
    runtime
        .boundary()
        .run(key, move || {
            with_connection(shared.as_ref(), target.as_target(), body)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Reads a related projection using one connection and one SQLite snapshot.
/// 使用一个连接及一个 SQLite 快照读取相关投影。
///
/// `body` runs on the existing physical-database blocking boundary. Its read transaction
/// and connection are dropped before admission is released, including on error or cancellation.
/// Connections are never pooled across callers: temporary state and deadline interrupts stay isolated.
/// `body` 在现有物理数据库阻塞边界内运行；读取事务及连接在释放准入之前清理，包括失败和取消。
/// 连接不跨调用方进入池，避免临时状态及截止时间中断串扰。
///
/// Returns the complete projection or a connection, query, or transaction failure.
/// 返回完整投影，或连接、查询及事务失败。
pub async fn with_read_snapshot_async<T: Send + 'static>(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    body: impl FnOnce(&rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    with_connection_async(runtime, target, move |connection| {
        connection
            .pragma_update(None, "query_only", true)
            .map_err(render)?;
        let transaction = connection.transaction().map_err(render)?;
        let result = body(&transaction)?;
        transaction.commit().map_err(render)?;
        Ok(result)
    })
    .await
}

/// Runs ordered scripts and one parameterized terminal statement atomically on the blocking boundary.
/// 在阻塞边界上原子执行有序脚本与一条参数化终结语句。
///
/// # Parameters / 参数
/// `runtime` and `target` select one admitted database; `scripts` are executed in order;
/// `terminal_sql` and `terminal_params` record completion inside the same transaction.
/// `runtime` 与 `target` 选择一个已准入数据库；`scripts` 按顺序执行；`terminal_sql` 与
/// `terminal_params` 在同一事务中记录完成状态。
///
/// # Returns / 返回值
/// Returns the committed execution result, or the operation or blocking-boundary error.
/// 返回已提交的执行结果，或操作/阻塞边界错误。
pub async fn execute_scripts_transaction_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    scripts: Vec<String>,
    terminal_sql: impl Into<String>,
    terminal_params: Vec<SqliteValue>,
) -> Result<ExecuteScriptResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let terminal_sql = terminal_sql.into();
    boundary
        .run(key, move || {
            execute_scripts_transaction(
                shared.as_ref(),
                target.as_target(),
                &scripts,
                &terminal_sql,
                &terminal_params,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Runs one query on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上运行一次查询，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared so the blocking task can own a handle to it; `target` names the database; `sql` and
/// `params` are moved into the task.
/// `runtime` 以共享方式传入，使阻塞任务能拥有它的句柄；`target` 命名数据库；`sql` 与 `params` 被移入任务。
///
/// # Returns / 返回值
/// Returns the engine's JSON row result, or the operation's or boundary's error.
/// 返回引擎的 JSON 行结果，或操作本身或边界的错误。
pub async fn query_json_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: impl Into<String>,
    params: Vec<SqliteValue>,
) -> Result<QueryJsonResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let sql = sql.into();
    boundary
        .run(key, move || {
            query_json(shared.as_ref(), target.as_target(), &sql, &params)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Queries within one caller deadline, interrupting SQLite and awaiting real connection cleanup.
/// 在单个调用方截止时间内查询，中断 SQLite 并等待真实连接清理。
/// `deadline` bounds queueing, busy waits and SQL execution; returns rows or the actual error.
/// `deadline` 限制排队、繁忙等待和 SQL 执行；返回记录或实际错误。
pub async fn query_json_until_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
    deadline: std::time::Instant,
) -> Result<QueryJsonResult, String> {
    query_json_bounded_until_async(
        runtime,
        target,
        sql,
        params,
        QueryJsonLimits::UNBOUNDED,
        deadline,
    )
    .await
}

/// Queries within one caller deadline while enforcing result limits during reading.
/// 在单个调用方截止时间内查询，并在读取期间强制执行结果上限。
/// `limits` rejects an oversized result after at most one extra row instead of materializing it first.
/// `limits` 最多多读一行即拒绝超限结果，而不是先完整物化。
pub async fn query_json_bounded_until_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
    limits: QueryJsonLimits,
    deadline: std::time::Instant,
) -> Result<QueryJsonResult, String> {
    with_connection_until_async(runtime, target, Some(deadline), move |connection| {
        let result =
            sql_exec::query_json_bounded(connection, &sql, &params, limits).map_err(render);
        if std::time::Instant::now() >= deadline {
            return Err(
                "SQLite query deadline exceeded; execution and connection cleanup completed."
                    .into(),
            );
        }
        result
    })
    .await
}

/// Runs one operation within its original deadline and owns interruption through connection cleanup.
/// 在原始截止时间内执行单次操作，并持有中断与连接清理过程。
/// The optional deadline applies to queues and SQLite; committed writes retain their real result.
/// 可选截止时间作用于排队与 SQLite；已提交写入保留实际结果。
pub async fn with_connection_until_async<T: Send + 'static>(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    deadline: Option<std::time::Instant>,
    body: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let shared = Arc::clone(runtime);
    let scheduler = tokio::runtime::Handle::current();
    runtime
        .boundary()
        .run_until(key, deadline, move || {
            with_connection(shared.as_ref(), target.as_target(), |connection| {
                let Some(deadline) = deadline else {
                    return body(connection);
                };
                let remaining = deadline
                    .checked_duration_since(std::time::Instant::now())
                    .ok_or_else(|| "SQLite query deadline expired before execution.".to_owned())?;
                connection.busy_timeout(remaining).map_err(render)?;
                let interrupt = connection.get_interrupt_handle();
                // Interrupt is connection-specific and never targets a subsequent operation's connection.
                // 中断专属于当前连接，绝不指向后续操作的连接。
                let _timer = SqlInterruptTimer(scheduler.spawn(async move {
                    tokio::time::sleep_until(deadline.into()).await;
                    interrupt.interrupt();
                }));
                body(connection)
            })
        })
        .await
        .map_err(render_boundary_failure)
}

/// Cancels the connection-specific interrupt task on both ordinary return and panic.
/// 正常返回及异常展开时都取消当前连接专属的中断任务。
struct SqlInterruptTimer(tokio::task::JoinHandle<()>);
impl Drop for SqlInterruptTimer {
    /// Stops only the timer; the database operation retains ownership of its connection.
    /// 仅停止计时器；数据库操作仍拥有其连接。
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Executes one script with a caller-owned deadline and returns the actual commit outcome.
/// 使用调用方截止时间执行脚本，并返回实际提交结果。
pub async fn execute_script_until_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
    deadline: Option<std::time::Instant>,
) -> Result<ExecuteScriptResult, String> {
    with_connection_until_async(runtime, target, deadline, move |connection| {
        sql_exec::execute_script(connection, &sql, &params).map_err(render)
    })
    .await
}

/// Executes an atomic parameter batch within the remaining caller budget.
/// 在调用方剩余预算内执行原子参数批次。
pub async fn execute_batch_until_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<Vec<SqliteValue>>,
    deadline: Option<std::time::Instant>,
) -> Result<ExecuteBatchResult, String> {
    with_connection_until_async(runtime, target, deadline, move |connection| {
        sql_exec::execute_batch(connection, &sql, &params).map_err(render)
    })
    .await
}

/// Spools one cursor-style query stream on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上暂存一条游标式查询流，离开 Tokio 核心线程。
///
/// The whole statement runs to completion before this returns, because the engine spools it: a cursor is a
/// handle over a finished result rather than a live statement, which is why the boundary can bound it like any
/// other operation.
/// 整条语句在返回前运行完毕，因为引擎会暂存它：游标是覆盖一个已完成结果的句柄而非活动语句，这正是边界能像其他
/// 操作一样约束它的原因。
///
/// # Parameters / 参数
/// The arguments are the synchronous entry point's, owned so they can move into the blocking task.
/// 参数与同步入口相同，但为所有权形式以便移入阻塞任务。
///
/// # Returns / 返回值
/// Returns the spooled stream, or the operation''s or boundary''s error.
/// 返回暂存后的流，或操作本身或边界的错误。
pub async fn query_stream_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: impl Into<String>,
    params: Vec<SqliteValue>,
    target_chunk_size: usize,
) -> Result<QueryStreamResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let sql = sql.into();
    boundary
        .run(key, move || {
            query_stream(
                shared.as_ref(),
                target.as_target(),
                &sql,
                &params,
                target_chunk_size,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Ensures one FTS index on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上确保一个 FTS 索引，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; the remaining arguments are the
/// synchronous entry point's, owned so they can move into the task.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；其余参数与同步入口相同，但为所有权形式以便移入任务。
///
/// # Returns / 返回值
/// Returns the engine's index result, or the operation's or boundary's error.
/// 返回引擎的索引结果，或操作本身或边界的错误。
pub async fn ensure_fts_index_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    index_name: impl Into<String>,
    tokenizer_mode: TokenizerMode,
) -> Result<EnsureFtsIndexResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let index_name = index_name.into();
    boundary
        .run(key, move || {
            ensure_fts_index(
                shared.as_ref(),
                target.as_target(),
                &index_name,
                tokenizer_mode,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Rebuilds one FTS index on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上重建一个 FTS 索引，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; `index_name` and
/// `tokenizer_mode` are the synchronous entry point's.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；`index_name` 与 `tokenizer_mode` 与同步入口相同。
///
/// # Returns / 返回值
/// Returns the engine's rebuild result, or the operation's or boundary's error.
/// 返回引擎的重建结果，或操作本身或边界的错误。
pub async fn rebuild_fts_index_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    index_name: impl Into<String>,
    tokenizer_mode: TokenizerMode,
) -> Result<RebuildFtsIndexResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let index_name = index_name.into();
    boundary
        .run(key, move || {
            rebuild_fts_index(
                shared.as_ref(),
                target.as_target(),
                &index_name,
                tokenizer_mode,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Tokenizes one text on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上对一段文本分词，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database that carries the dictionary;
/// `text` is owned so it can move into the task.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名携带词典的数据库；`text` 为所有权形式以便移入任务。
///
/// # Returns / 返回值
/// Returns the engine's tokenization output, or the operation's or boundary's error.
/// 返回引擎的分词输出，或操作本身或边界的错误。
pub async fn tokenize_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    tokenizer_mode: TokenizerMode,
    text: impl Into<String>,
    search_mode: bool,
) -> Result<TokenizeOutput, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let text = text.into();
    boundary
        .run(key, move || {
            tokenize(
                shared.as_ref(),
                target.as_target(),
                tokenizer_mode,
                &text,
                search_mode,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Upserts one FTS document on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上写入一条 FTS 文档，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; the document fields are owned so
/// they can move into the task.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；文档字段为所有权形式以便移入任务。
///
/// # Returns / 返回值
/// Returns the engine's mutation result, or the operation's or boundary's error.
/// 返回引擎的变更结果，或操作本身或边界的错误。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_fts_document_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    index_name: impl Into<String>,
    tokenizer_mode: TokenizerMode,
    id: impl Into<String>,
    file_path: impl Into<String>,
    title: impl Into<String>,
    content: impl Into<String>,
) -> Result<FtsMutationResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let index_name = index_name.into();
    let id = id.into();
    let file_path = file_path.into();
    let title = title.into();
    let content = content.into();
    boundary
        .run(key, move || {
            upsert_fts_document(
                shared.as_ref(),
                target.as_target(),
                &index_name,
                tokenizer_mode,
                &id,
                &file_path,
                &title,
                &content,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Upserts one custom dictionary word on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上写入一个自定义词典词，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; `word` is owned.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；`word` 为所有权形式。
///
/// # Returns / 返回值
/// Returns the engine's dictionary mutation result, or the operation's or boundary's error.
/// 返回引擎的词典变更结果，或操作本身或边界的错误。
pub async fn upsert_custom_word_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    word: impl Into<String>,
    weight: usize,
) -> Result<DictionaryMutationResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let word = word.into();
    boundary
        .run(key, move || {
            upsert_custom_word(shared.as_ref(), target.as_target(), &word, weight)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Removes one custom dictionary word on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上删除一个自定义词典词，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; `word` is owned.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；`word` 为所有权形式。
///
/// # Returns / 返回值
/// Returns the engine's dictionary mutation result, or the operation's or boundary's error.
/// 返回引擎的词典变更结果，或操作本身或边界的错误。
pub async fn remove_custom_word_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    word: impl Into<String>,
) -> Result<DictionaryMutationResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let word = word.into();
    boundary
        .run(key, move || {
            remove_custom_word(shared.as_ref(), target.as_target(), &word)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Lists the custom dictionary on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上列出自定义词典，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task and `target` names the database.
/// `runtime` 以共享方式传入阻塞任务，`target` 命名数据库。
///
/// # Returns / 返回值
/// Returns the engine's dictionary listing, or the operation's or boundary's error.
/// 返回引擎的词典列表，或操作本身或边界的错误。
pub async fn list_custom_words_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
) -> Result<ListCustomWordsResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    boundary
        .run(key, move || {
            list_custom_words(shared.as_ref(), target.as_target())
        })
        .await
        .map_err(render_boundary_failure)
}

/// Closes one database file's engine handle on the blocking boundary.
/// 在阻塞边界上关闭某个数据库文件的引擎句柄。
///
/// Closing is access to the database like any other, so it takes the same gate. A close that ran outside
/// the gate could take the file's lock away between two statements of one operation, which is exactly what
/// a VCE layout change must not be able to do.
/// 关闭与任何其他访问一样属于对数据库的访问，因此它取同一道门。在门外运行的关闭可能在一个操作的两条语句之间
/// 把该文件的锁拿走，而这正是 VCE 布局变更绝不能做到的事。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task and `binding` names the file to close.
/// `runtime` 以共享方式传入阻塞任务，`binding` 命名要关闭的文件。
///
/// # Returns / 返回值
/// Returns whether a handle was open and is now closed, or the boundary's error.
/// 返回是否曾有句柄打开并现已关闭，或边界的错误。
pub async fn close_database_file_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    binding: &SqliteFileBinding,
) -> Result<bool, String> {
    let key = DatabaseKey::File(binding.path.clone());
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let binding = binding.clone();
    boundary
        .run(key, move || {
            Ok::<bool, String>(shared.close_sqlite_file(&binding))
        })
        .await
        .map_err(render_boundary_failure)
}

/// Deletes one FTS document on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上删除一条 FTS 文档，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; `index_name` and `id` are owned.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；`index_name` 与 `id` 为所有权形式。
///
/// # Returns / 返回值
/// Returns the engine's mutation result, or the operation's or boundary's error.
/// 返回引擎的变更结果，或操作本身或边界的错误。
pub async fn delete_fts_document_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    index_name: impl Into<String>,
    id: impl Into<String>,
) -> Result<FtsMutationResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let index_name = index_name.into();
    let id = id.into();
    boundary
        .run(key, move || {
            delete_fts_document(shared.as_ref(), target.as_target(), &index_name, &id)
        })
        .await
        .map_err(render_boundary_failure)
}

/// Searches one FTS index on the blocking boundary, off the Tokio core threads.
/// 在阻塞边界上检索一个 FTS 索引，离开 Tokio 核心线程。
///
/// # Parameters / 参数
/// `runtime` is shared for the blocking task; `target` names the database; the index, query and page
/// arguments are the synchronous entry point's, with the text owned so it can move into the task.
/// `runtime` 以共享方式传入阻塞任务；`target` 命名数据库；索引、查询与分页参数与同步入口相同，其中文本为
/// 所有权形式以便移入任务。
///
/// # Returns / 返回值
/// Returns the engine's search result, or the operation's or boundary's error.
/// 返回引擎的检索结果，或操作本身或边界的错误。
pub async fn search_fts_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    index_name: impl Into<String>,
    tokenizer_mode: TokenizerMode,
    query: impl Into<String>,
    limit: u32,
    offset: u32,
) -> Result<SearchFtsResult, String> {
    let target = target.into();
    let key = target.key(runtime.as_ref())?;
    let boundary = runtime.boundary().clone();
    let shared = Arc::clone(runtime);
    let index_name = index_name.into();
    let query = query.into();
    boundary
        .run(key, move || {
            search_fts(
                shared.as_ref(),
                target.as_target(),
                &index_name,
                tokenizer_mode,
                &query,
                limit,
                offset,
            )
        })
        .await
        .map_err(render_boundary_failure)
}

/// Opens one connection to the selected database.
/// 打开到所选数据库的一个连接。
pub fn connect<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
) -> Result<rusqlite::Connection, String> {
    match target.into() {
        DatabaseTarget::Logical(logical) => runtime.connect(logical).map_err(render),
        DatabaseTarget::File(binding) => runtime.connect_sqlite_file(binding).map_err(render),
    }
}

/// Runs `body` with one connection to the selected database, releasing it afterwards.
/// 在到所选数据库的一个连接上运行 `body`，随后释放该连接。
///
/// Use this when one logical operation issues more than one statement, so the sequence shares a
/// single connection instead of opening one per statement.
///
/// 当一个逻辑操作发出多于一条语句时使用本函数，使该序列共用一个连接，而不是每条语句打开一个。
pub fn with_connection<'a, T>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    body: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut connection = connect(runtime, target)?;
    body(&mut connection)
}

/// Runs one SQL script against a logical database.
/// 对某个逻辑数据库运行一段 SQL 脚本。
pub fn execute_script<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    sql: impl AsRef<str>,
    params: &[SqliteValue],
) -> Result<ExecuteScriptResult, String> {
    let mut connection = connect(runtime, target)?;
    sql_exec::execute_script(&mut connection, sql.as_ref(), params).map_err(render)
}

/// Runs one statement and spools its Arrow IPC chunks for cursor-style reads.
/// 运行一条语句并把其 Arrow IPC 分块暂存，供游标式读取。
///
/// The result owns a spool file and deletes it when dropped, so the caller decides how long a cursor lives by
/// deciding how long it holds this value. The chunks are one Arrow IPC stream split at about
/// `target_chunk_size` bytes; concatenating every chunk yields a stream an `arrow::ipc::reader::StreamReader`
/// decodes, which is what the retired library produced and therefore what a skill already expects.
/// 结果拥有一个暂存文件并在被丢弃时删除它，因此游标存活多久由调用方决定其持有本值多久。分块是一条 Arrow IPC 流按
/// 约 `target_chunk_size` 字节切开；把所有分块拼接起来得到一条可被 `arrow::ipc::reader::StreamReader` 解码的
/// 流，这正是已退役库所产出的内容，因而也是技能所预期的内容。
///
/// # Parameters / 参数
/// `runtime` and `target` name the database; `sql` must be a single statement; `params` are the bound values;
/// `target_chunk_size` is a soft target that the engine raises to its own minimum.
/// `runtime` 与 `target` 命名数据库；`sql` 必须是单条语句；`params` 是绑定值；`target_chunk_size` 是软目标，
/// 引擎会把它提升到自身的最小值。
///
/// # Returns / 返回值
/// Returns the spooled stream, or the engine's or the connection's error.
/// 返回暂存后的流，或引擎或连接的错误。
pub fn query_stream<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    sql: impl AsRef<str>,
    params: &[SqliteValue],
    target_chunk_size: usize,
) -> Result<QueryStreamResult, String> {
    let mut connection = connect(runtime, target)?;
    sql_exec::query_stream(&mut connection, sql.as_ref(), params, target_chunk_size).map_err(render)
}

/// Runs one statement with several parameter groups inside a single transaction.
/// 在单个事务内以多组参数运行一条语句。
pub fn execute_batch<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    sql: impl AsRef<str>,
    batch_params: &[Vec<SqliteValue>],
) -> Result<ExecuteBatchResult, String> {
    let mut connection = connect(runtime, target)?;
    sql_exec::execute_batch(&mut connection, sql.as_ref(), batch_params).map_err(render)
}

/// Runs ordered scripts and one parameterized terminal statement in one transaction.
/// 在一个事务中运行有序脚本与一条参数化终结语句。
///
/// # Parameters / 参数
/// `runtime` and `target` select the database; `scripts` run in order; the terminal statement records
/// success only when all scripts complete.
/// `runtime` 与 `target` 选择数据库；`scripts` 按顺序运行；仅当全部脚本完成时终结语句才记录成功。
///
/// # Returns / 返回值
/// Returns the committed result or an explicit database error after rollback.
/// 返回已提交结果，或回滚后的明确数据库错误。
pub fn execute_scripts_transaction<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    scripts: &[String],
    terminal_sql: &str,
    terminal_params: &[SqliteValue],
) -> Result<ExecuteScriptResult, String> {
    let mut connection = connect(runtime, target)?;
    sql_exec::execute_scripts_transaction(&mut connection, scripts, terminal_sql, terminal_params)
        .map_err(render)
}

/// Runs one query and returns its rows as JSON.
/// 运行一次查询并以 JSON 返回其行。
pub fn query_json<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    sql: impl AsRef<str>,
    params: &[SqliteValue],
) -> Result<QueryJsonResult, String> {
    let mut connection = connect(runtime, target)?;
    sql_exec::query_json(&mut connection, sql.as_ref(), params).map_err(render)
}

/// Runs one query and returns its rows as Arrow IPC bytes.
/// 运行一次查询并以 Arrow IPC 字节返回其行。
///
/// This replaces the controller's query-stream round trip. The controller had to open a stream,
/// wait for metrics and read chunks back over RPC because the result lived in another process; here
/// the same bytes are produced in process and returned directly, so there is no stream identifier,
/// no polling and no 4 MiB message limit.
///
/// 本函数取代控制器的查询流往返。控制器必须打开流、等待指标并经 RPC 读回分块，是因为结果位于另一个
/// 进程；此处同样的字节在进程内产生并直接返回，因此没有流标识、没有轮询，也没有 4 MiB 消息上限。
pub fn query_arrow_ipc<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    sql: impl AsRef<str>,
    params: &[SqliteValue],
) -> Result<Vec<u8>, String> {
    let mut connection = connect(runtime, target)?;

    let mut collector = ChunkCollector::new(sql_exec::DEFAULT_IPC_CHUNK_BYTES);
    let (writer, _metrics) = sql_exec::query_stream_with_writer(
        &mut connection,
        sql.as_ref(),
        params,
        std::mem::replace(
            &mut collector,
            ChunkCollector::new(sql_exec::DEFAULT_IPC_CHUNK_BYTES),
        ),
    )
    .map_err(render)?;

    // `ChunkCollector` emits on flush, and `query_stream_with_writer` finishes the Arrow stream
    // before returning, so the writer already holds every byte.
    // `ChunkCollector` 在 flush 时发出内容，而 `query_stream_with_writer` 在返回前已结束 Arrow 流，
    // 因此该写入器已持有全部字节。
    Ok(writer.chunks().concat())
}

/// Ensures one FTS5 index exists in a logical database.
/// 在某个逻辑数据库中确保一个 FTS5 索引存在。
pub fn ensure_fts_index<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
) -> Result<EnsureFtsIndexResult, String> {
    let connection = connect(runtime, target)?;
    fts::ensure_fts_index(&connection, index_name, tokenizer_mode).map_err(render)
}

/// Rebuilds one FTS5 index in a logical database.
/// 在某个逻辑数据库中重建一个 FTS5 索引。
pub fn rebuild_fts_index<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
) -> Result<RebuildFtsIndexResult, String> {
    let connection = connect(runtime, target)?;
    fts::rebuild_fts_index(&connection, index_name, tokenizer_mode).map_err(render)
}

/// Tokenizes one query with the tokenizer a logical database has registered.
/// 使用某个逻辑数据库已注册的分词器对一段查询分词。
///
/// The connection is what supplies that database's custom dictionary, which is why this is a
/// per-database helper rather than a free function over the text.
///
/// 连接提供了该数据库的自定义词典，因此本函数是按数据库的辅助函数，而不是仅针对文本的自由函数。
pub fn tokenize<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    tokenizer_mode: TokenizerMode,
    text: &str,
    search_mode: bool,
) -> Result<TokenizeOutput, String> {
    let connection = connect(runtime, target)?;
    tokenizer::tokenize_text(Some(&connection), tokenizer_mode, text, search_mode).map_err(render)
}

/// Writes one document into an FTS5 index of a logical database.
/// 把一篇文档写入某个逻辑数据库的 FTS5 索引。
///
/// The index is ensured first, so a caller that has just created a generation-scoped index can upsert
/// into it without a separate ensure step.
/// 索引会先被确保存在，因此刚创建了代次范围索引的调用方可以直接写入，无需单独的确保步骤。
// The parameter list mirrors the engine call it forwards to. Introducing a parallel document type here
// would add a second vocabulary for one call, and the alternative — a struct that exists only to satisfy
// an arity lint — is the kind of scaffolding this migration is removing elsewhere. The repository already
// allows this lint at its other wide engine boundaries.
// 参数列表与它所转发的引擎调用保持一致。在此引入一个并行的文档类型会为一次调用增加第二套词汇，而另一
// 种做法——只为满足元数 lint 而存在的结构体——正是本迁移在其他地方正在移除的那类脚手架。仓库在其其他
// 宽引擎边界处已经允许该 lint。
#[allow(clippy::too_many_arguments)]
pub fn upsert_fts_document<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
    id: &str,
    file_path: &str,
    title: &str,
    content: &str,
) -> Result<FtsMutationResult, String> {
    let connection = connect(runtime, target)?;
    fts::upsert_fts_document(
        &connection,
        index_name,
        tokenizer_mode,
        id,
        file_path,
        title,
        content,
    )
    .map_err(render)
}
/// Deletes one FTS document from one database on the engine's behalf.
/// 代表引擎从一个数据库中删除一条 FTS 文档。
///
/// # Parameters / 参数
/// `runtime` selects the engine, `target` the database, and `index_name` and `id` the document.
/// `runtime` 选择引擎，`target` 选择数据库，`index_name` 与 `id` 选择文档。
///
/// # Returns / 返回值
/// Returns the engine's mutation result, or the rendered engine error.
/// 返回引擎的变更结果，或渲染后的引擎错误。
///
/// # Panics / panic
/// This function does not panic.
/// 本函数不会 panic。
pub fn delete_fts_document<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    index_name: &str,
    id: &str,
) -> Result<FtsMutationResult, String> {
    let connection = connect(runtime, target)?;
    fts::delete_fts_document(&connection, index_name, id).map_err(render)
}

/// Searches one FTS index in one database on the engine's behalf.
/// 代表引擎在一个数据库中检索一个 FTS 索引。
///
/// # Parameters / 参数
/// `runtime` selects the engine, `target` the database, `index_name` and `tokenizer_mode` the index, and
/// `query`, `limit` and `offset` the page of hits.
/// `runtime` 选择引擎，`target` 选择数据库，`index_name` 与 `tokenizer_mode` 选择索引，`query`、`limit` 与
/// `offset` 选择命中页。
///
/// # Returns / 返回值
/// Returns the engine's search result, or the rendered engine error.
/// 返回引擎的检索结果，或渲染后的引擎错误。
///
/// # Panics / panic
/// This function does not panic.
/// 本函数不会 panic。
pub fn search_fts<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    index_name: &str,
    tokenizer_mode: TokenizerMode,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<SearchFtsResult, String> {
    let connection = connect(runtime, target)?;
    fts::search_fts(
        &connection,
        index_name,
        tokenizer_mode,
        query,
        limit,
        offset,
    )
    .map_err(render)
}

/// Adds or updates one custom dictionary word for a logical database.
/// 为某个逻辑数据库新增或更新一个自定义词典词。
pub fn upsert_custom_word<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    word: &str,
    weight: usize,
) -> Result<DictionaryMutationResult, String> {
    let connection = connect(runtime, target)?;
    tokenizer::upsert_custom_word(&connection, word, weight).map_err(render)
}

/// Removes one custom dictionary word from a logical database.
/// 从某个逻辑数据库移除一个自定义词典词。
pub fn remove_custom_word<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
    word: &str,
) -> Result<DictionaryMutationResult, String> {
    let connection = connect(runtime, target)?;
    tokenizer::remove_custom_word(&connection, word).map_err(render)
}

/// Lists the enabled custom dictionary words of a logical database.
/// 列出某个逻辑数据库已启用的自定义词典词。
pub fn list_custom_words<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
) -> Result<ListCustomWordsResult, String> {
    let connection = connect(runtime, target)?;
    tokenizer::list_custom_words(&connection).map_err(render)
}
/// Returns the SQLite file path bound to a logical database, for diagnostics.
/// 返回绑定到某个逻辑数据库的 SQLite 文件路径，用于诊断。
pub fn database_path(
    runtime: &LocalDatabaseRuntime,
    logical: LogicalDatabase,
) -> Result<PathBuf, String> {
    runtime.path_for(logical).map_err(render)
}

/// Converts a JSON scalar into an engine parameter value.
/// 将 JSON 标量转换为引擎参数值。
pub fn json_param(value: serde_json::Value) -> Result<SqliteValue, String> {
    SqliteValue::from_json(value).map_err(render)
}

/// Converts a list of JSON scalars into engine parameter values.
/// 将一组 JSON 标量转换为引擎参数值。
pub fn json_params(values: Vec<serde_json::Value>) -> Result<Vec<SqliteValue>, String> {
    values.into_iter().map(json_param).collect()
}

/// Reports whether a local-database failure means the database could not be reached.
/// 判断某个本地数据库失败是否表示数据库无法访问。
///
/// Call sites that used to branch on controller error strings need a typed equivalent; this is it.
///
/// 此前依据控制器错误字符串分支的调用点需要类型化等价物，本函数即为此。
///
/// "Unavailable" means *retrying later might succeed* — the file, its directory or its lock was not
/// reachable right now. It deliberately does **not** mean "something went wrong": a refused argument,
/// a path conflict, a tokenizer request and a failed statement are all failures a retry cannot fix,
/// and reporting them as unavailable would tell a caller to loop on a condition that will never
/// clear. That distinction has to be made from the engine error, not from the wrapper variant,
/// because one wrapper carries both kinds — [`LocalDatabaseError::SqliteFile`] wraps both "the file
/// could not be opened" and "this file is already open with different options".
///
/// “不可用”意味着*稍后重试可能成功*——文件、其目录或其锁在当时无法访问。它刻意**不**意味着"出了错"：
/// 被拒绝的参数、路径冲突、分词器请求与失败的语句都是重试无法修复的失败，把它们报告为不可用会告诉调用方
/// 在一个永远不会消失的条件上循环。该区分必须由引擎错误作出，而不是由包装变体作出，因为同一个包装同时
/// 承载两类： [`LocalDatabaseError::SqliteFile`] 既包装"文件无法打开"，也包装"该文件已以不同选项打开"。
pub fn is_unavailable(error: &LocalDatabaseError) -> bool {
    match error {
        LocalDatabaseError::Unbound { .. }
        | LocalDatabaseError::HealthCheck { .. }
        | LocalDatabaseError::ShutDown => true,
        LocalDatabaseError::Sqlite { source, .. }
        | LocalDatabaseError::SqliteFile { source, .. } => sqlite_failure_is_unavailability(source),
        LocalDatabaseError::LanceDb(source) => lance_failure_is_unavailability(source),
        // Recorded state that the caller cannot retry away: a conflicting binding, a conflicting
        // engine option set, a database whose schema version this build does not accept, or an
        // identity no part of the host registered.
        // 调用方无法靠重试消除的已记录状态：冲突的绑定、冲突的引擎选项集合、结构版本不被本构建接受的
        // 数据库，或宿主任何部分都未登记的身份。
        LocalDatabaseError::BindingConflict { .. }
        | LocalDatabaseError::LanceDbLimitsConflict { .. }
        | LocalDatabaseError::NamedBinding { .. }
        | LocalDatabaseError::SchemaVersionMismatch { .. } => false,
    }
}

/// Reports whether a SQLite engine failure means the database was not reachable.
/// 判断某个 SQLite 引擎失败是否表示数据库当时无法访问。
///
/// The match is exhaustive on purpose: a new engine failure must be classified rather than defaulting
/// into either answer.
///
/// 该匹配刻意穷尽：新增的引擎失败必须被分类，而不是默认落入任一答案。
fn sqlite_failure_is_unavailability(
    error: &vulcan_sqlite_engine::error::SqliteEngineError,
) -> bool {
    use vulcan_sqlite_engine::error::SqliteEngineError as Failure;

    match error {
        Failure::Open { .. } | Failure::LockHeld { .. } | Failure::LockIo { .. } => true,
        Failure::InvalidArgument { .. }
        | Failure::ResourceConflict { .. }
        | Failure::PathNotAuthorized { .. }
        | Failure::Sql { .. }
        | Failure::BatchItem { .. }
        | Failure::Serialization { .. }
        | Failure::QueryStreamState { .. }
        | Failure::ResultLimitExceeded { .. }
        | Failure::Tokenizer { .. } => false,
    }
}

/// Reports whether a LanceDB engine failure means the database was not reachable.
/// 判断某个 LanceDB 引擎失败是否表示数据库当时无法访问。
fn lance_failure_is_unavailability(
    error: &vulcan_lancedb_engine::error::LanceDbEngineError,
) -> bool {
    use vulcan_lancedb_engine::error::LanceDbEngineError as Failure;

    match error {
        Failure::Open { .. } | Failure::LockHeld { .. } | Failure::LockIo { .. } => true,
        Failure::InvalidArgument { .. }
        | Failure::PathNotAuthorized { .. }
        | Failure::TableNotFound { .. }
        | Failure::SchemaNotAllowed { .. }
        | Failure::Compaction { .. }
        | Failure::Internal { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    /// Deadline interruption completes before a subsequent query reuses the database boundary.
    /// 截止中断完成后，后续查询才复用数据库边界。
    #[test]
    fn query_deadline_interrupts_sql_and_releases_the_actual_connection() {
        let (_directory, handle) = fixture();
        let scheduler = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        scheduler.block_on(async {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(40);
            let error = super::query_json_until_async(&handle.local, LogicalDatabase::SystemStatistics,
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n".into(), vec![], deadline).await.unwrap_err();
            assert!(error.contains("deadline") || error.contains("interrupt"), "unexpected query failure: {error}");
            let rows = super::query_json_until_async(&handle.local, LogicalDatabase::SystemStatistics,
                "SELECT 7 AS value".into(), vec![], std::time::Instant::now() + std::time::Duration::from_secs(2)).await.unwrap();
            assert_eq!(rows.row_count, 1);
        });
    }
    use super::*;
    use std::collections::HashMap;

    use crate::data_plane::{HostRuntimeHandle, HostRuntimeMetadata, SYSTEM_STATISTICS_SPACE_ID};
    use crate::local_database_runtime::LocalDatabaseRuntime;

    /// Verifies lexical aliases share one physical boundary key.
    /// 验证字面路径别名共享同一个物理边界键。
    #[test]
    fn file_target_key_normalizes_lexical_aliases() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = LocalDatabaseRuntime::open(HashMap::new()).expect("local runtime");
        let direct = OwnedDatabaseTarget::from(SqliteFileBinding::new(
            directory.path().join("storage.sqlite3"),
        ));
        let alias = OwnedDatabaseTarget::from(SqliteFileBinding::new(
            directory
                .path()
                .join("unused")
                .join("..")
                .join("storage.sqlite3"),
        ));

        assert_eq!(
            direct.key(&runtime).expect("direct key"),
            alias.key(&runtime).expect("alias key"),
            "aliases must serialize on one database boundary"
        );
    }

    /// Verifies two logical identities bound to one physical file share one boundary key.
    /// 验证绑定到同一物理文件的两个逻辑身份共享同一个边界键。
    #[test]
    fn logical_targets_gate_by_physical_path() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let direct = directory.path().join("shared.sqlite3");
        let alias = directory
            .path()
            .join("unused")
            .join("..")
            .join("shared.sqlite3");
        let runtime = LocalDatabaseRuntime::open(HashMap::from([
            (LogicalDatabase::AppState, direct),
            (LogicalDatabase::SystemStatistics, alias),
        ]))
        .expect("local runtime");

        assert_eq!(
            OwnedDatabaseTarget::from(LogicalDatabase::AppState)
                .key(&runtime)
                .expect("app key"),
            OwnedDatabaseTarget::from(LogicalDatabase::SystemStatistics)
                .key(&runtime)
                .expect("statistics key"),
            "logical aliases must serialize on one physical database boundary"
        );
    }

    /// Builds one handle whose local runtime is bound to a temporary database.
    /// 构造一个句柄，其本地运行时绑定到临时数据库。
    fn fixture() -> (tempfile::TempDir, HostRuntimeHandle) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let sqlite_path = directory.path().join("statistics.sqlite3");

        let mut bindings = HashMap::new();
        bindings.insert(LogicalDatabase::SystemStatistics, sqlite_path.clone());
        let local = LocalDatabaseRuntime::open(bindings).expect("local runtime");

        let handle = HostRuntimeHandle::new(
            std::sync::Arc::new(local),
            HostRuntimeMetadata {
                data_root: directory.path().to_path_buf(),
                sqlite_path: sqlite_path.clone(),
                sqlite_binding_id: SYSTEM_STATISTICS_SPACE_ID.to_string(),
                system_statistics_sqlite_path: sqlite_path,
                system_statistics_sqlite_binding_id: SYSTEM_STATISTICS_SPACE_ID.to_string(),
            },
        );

        (directory, handle)
    }

    /// Verifies bounded queries report crossed row limits as failures.
    /// 验证有界查询将越过的行数上限报告为失败。
    #[tokio::test]
    async fn bounded_query_rejects_rows_past_the_limit() {
        let (_directory, handle) = fixture();
        let outcome = query_json_bounded_until_async(
            &handle.local,
            LogicalDatabase::SystemStatistics,
            "SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3".into(),
            Vec::new(),
            QueryJsonLimits {
                max_rows: 2,
                max_bytes: usize::MAX,
            },
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .await;
        match outcome {
            Err(message) => {
                assert!(message.contains("more than 2 rows"), "{message}")
            }
            other => panic!("expected a row-limit failure, got {other:?}"),
        }
    }

    #[test]
    fn script_query_and_batch_reach_the_bound_database() {
        let (_directory, handle) = fixture();

        execute_script(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "CREATE TABLE samples (id INTEGER PRIMARY KEY, label TEXT NOT NULL);",
            &[],
        )
        .expect("create table");

        execute_batch(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "INSERT INTO samples (id, label) VALUES (?1, ?2)",
            &[
                vec![SqliteValue::Integer(1), SqliteValue::Text("a".into())],
                vec![SqliteValue::Integer(2), SqliteValue::Text("b".into())],
            ],
        )
        .expect("batch insert");

        let result = query_json(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "SELECT id, label FROM samples ORDER BY id",
            &[],
        )
        .expect("query");

        assert_eq!(result.row_count, 2);
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&result.json_data).expect("decode json");
        assert_eq!(rows[0]["label"], serde_json::json!("a"));
        assert_eq!(rows[1]["label"], serde_json::json!("b"));
    }

    #[test]
    fn parameters_are_bound_rather_than_interpolated() {
        let (_directory, handle) = fixture();

        execute_script(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "CREATE TABLE notes (body TEXT NOT NULL);",
            &[],
        )
        .expect("create table");

        // A value that would terminate a string literal and inject a statement must be stored as
        // data, which is only true if binding is real rather than string interpolation.
        // 一个本可终止字符串字面量并注入语句的值必须被当作数据存储，这只有在参数确实被绑定而非字符串
        // 拼接时才成立。
        let hostile = "x'); DROP TABLE notes; --";
        execute_script(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "INSERT INTO notes (body) VALUES (?1)",
            &[SqliteValue::Text(hostile.to_string())],
        )
        .expect("insert hostile text");

        let result = query_json(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "SELECT body FROM notes",
            &[],
        )
        .expect("query");

        assert_eq!(result.row_count, 1, "the table must still exist");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&result.json_data).expect("decode json");
        assert_eq!(rows[0]["body"], serde_json::json!(hostile));
    }

    #[test]
    fn arrow_ipc_output_decodes_back_to_rows() {
        use arrow_ipc::reader::StreamReader;
        use std::io::Cursor;

        let (_directory, handle) = fixture();

        execute_script(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "CREATE TABLE metrics (id INTEGER, score REAL); \
             INSERT INTO metrics VALUES (1, 1.5), (2, 2.5);",
            &[],
        )
        .expect("seed metrics");

        let bytes = query_arrow_ipc(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "SELECT id, score FROM metrics ORDER BY id",
            &[],
        )
        .expect("arrow query");

        let reader = StreamReader::try_new(Cursor::new(bytes), None).expect("open arrow stream");
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .expect("decode arrow batches");
        let rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();
        assert_eq!(rows, 2);
    }

    /// Related queries cannot observe different database generations during one projection.
    /// 同一投影中的相关查询不能观察到不同数据库代次。
    #[tokio::test]
    async fn read_snapshot_reuses_one_transaction_and_isolates_the_next_caller() {
        let (_directory, handle) = fixture();
        execute_script(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "CREATE TABLE snapshot_value (value INTEGER); INSERT INTO snapshot_value VALUES (1);",
            &[],
        )
        .expect("seed snapshot");
        let database =
            database_path(handle.local.as_ref(), LogicalDatabase::SystemStatistics).expect("path");
        let observed = super::with_read_snapshot_async(
            &handle.local,
            LogicalDatabase::SystemStatistics,
            move |reader| {
                assert!(
                    !reader.is_autocommit(),
                    "all projection queries must share one read transaction"
                );
                let first: i64 = reader
                    .query_row("SELECT value FROM snapshot_value", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                // An independent WAL connection simulates a writer between this projection's reads.
                // 独立 WAL 连接模拟当前投影两次读取之间的写入。
                let writer =
                    rusqlite::Connection::open(database).map_err(|error| error.to_string())?;
                writer
                    .execute("UPDATE snapshot_value SET value = 2", [])
                    .map_err(|error| error.to_string())?;
                let second: i64 = reader
                    .query_row("SELECT value FROM snapshot_value", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                assert!(
                    reader
                        .execute("UPDATE snapshot_value SET value = 3", [])
                        .is_err(),
                    "projection is read-only"
                );
                Ok((first, second))
            },
        )
        .await
        .expect("complete snapshot");
        assert_eq!(observed, (1, 1));
        let next = super::with_connection_async(
            &handle.local,
            LogicalDatabase::SystemStatistics,
            |connection| {
                assert!(
                    connection.is_autocommit(),
                    "a prior read transaction must not leak"
                );
                connection
                    .execute("UPDATE snapshot_value SET value = 3", [])
                    .map_err(|error| error.to_string())?;
                connection
                    .query_row("SELECT value FROM snapshot_value", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(|error| error.to_string())
            },
        )
        .await
        .expect("next caller is writable and current");
        assert_eq!(next, 3);
    }

    /// A failed projection drops its read transaction without poisoning future operations.
    /// 失败投影释放读取事务，不污染后续操作。
    #[tokio::test]
    async fn failed_read_snapshot_releases_its_connection() {
        let (_directory, handle) = fixture();
        let failed = super::with_read_snapshot_async(
            &handle.local,
            LogicalDatabase::SystemStatistics,
            |_| Err::<(), _>("projection failed".to_string()),
        )
        .await;
        assert_eq!(failed.expect_err("explicit failure"), "projection failed");
        super::execute_script_async(
            &handle.local,
            LogicalDatabase::SystemStatistics,
            "CREATE TABLE after_failure (id INTEGER);",
            Vec::new(),
        )
        .await
        .expect("boundary remains usable");
    }

    /// Cancellation releases the waiter, but the running read retains its database gate until cleanup.
    /// 取消会释放等待方，但正在执行的读取保留数据库闸门直至清理完成。
    #[tokio::test]
    async fn cancelled_read_snapshot_retains_gate_until_actual_cleanup() {
        // Isolate the database and coordinate real blocking work without relying on scheduling order.
        // 隔离数据库并协调真实阻塞工作，不依赖任务调度顺序。
        let (_directory, handle) = fixture();
        let runtime = Arc::clone(&handle.local);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let read = tokio::spawn(async move {
            super::with_read_snapshot_async(
                &runtime,
                LogicalDatabase::SystemStatistics,
                move |connection| {
                    assert!(!connection.is_autocommit());
                    started_tx.send(()).expect("announce active transaction");
                    release_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| error.to_string())?;
                    Ok(())
                },
            )
            .await
        });
        started_rx.await.expect("read transaction started");
        read.abort();
        assert!(read.await.expect_err("waiter cancelled").is_cancelled());
        // The second operation must remain queued while the detached blocking read still owns its gate.
        // 脱离等待方的阻塞读取仍持有闸门时，第二次操作必须保持排队。
        let runtime = Arc::clone(&handle.local);
        let mut next = tokio::spawn(async move {
            super::with_connection_async(
                &runtime,
                LogicalDatabase::SystemStatistics,
                |connection| {
                    assert!(connection.is_autocommit());
                    connection
                        .execute_batch("CREATE TABLE after_cancel (id INTEGER);")
                        .map_err(render)
                },
            )
            .await
        });
        let remained_queued = tokio::time::timeout(std::time::Duration::from_millis(50), &mut next)
            .await
            .is_err();
        release_tx
            .send(())
            .expect("release real read before asserting");
        assert!(
            remained_queued,
            "cancellation must not release an active database gate"
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), next)
            .await
            .expect("operation completes after actual cleanup")
            .expect("join queued caller")
            .expect("connection is writable and isolated");
    }

    /// Real dashboard and memory projections share read scopes without leaking across hosts.
    /// 真实用量面板与记忆投影复用读取作用域，且不会跨宿主泄漏数据。
    #[tokio::test]
    async fn read_snapshot_business_projections_keep_hosts_isolated() {
        // Both fixtures own independent data roots and database bindings.
        // 两个夹具分别持有独立的数据根目录与数据库绑定。
        let (_first_root, first) = fixture();
        let (_second_root, second) = fixture();
        let dashboard = crate::workspace_store::load_provider_usage_dashboard(&first)
            .await
            .expect("read all dashboard panels through one transaction");
        assert!(
            dashboard.daily.is_empty()
                && dashboard.recent_attempts.is_empty()
                && dashboard.accounts.is_empty()
        );
        let first_vmm = crate::vmm_data_plane::bootstrap_vmm_data_plane(&first)
            .await
            .expect("first memory store");
        let second_vmm = crate::vmm_data_plane::bootstrap_vmm_data_plane(&second)
            .await
            .expect("second memory store");
        let project = first_vmm
            .create_project("Snapshot fixture", "snapshot-fixture")
            .await
            .expect("project");
        first_vmm
            .bind_vulcan_project("workspace-one", project.id)
            .await
            .expect("binding");
        let usage = first_vmm
            .project_usage(project.id)
            .await
            .expect("consistent usage projection");
        assert_eq!(usage.owner_vulcan_project_ids, vec!["workspace-one"]);
        let bindings = first_vmm
            .list_project_bindings()
            .await
            .expect("consistent binding projection");
        assert!(
            bindings
                .iter()
                .any(|binding| binding.vulcan_project_id == "workspace-one"
                    && binding.owner_project_id == project.id)
        );
        assert!(
            second_vmm
                .list_project_bindings()
                .await
                .expect("isolated binding projection")
                .is_empty()
        );
    }

    #[test]
    fn with_connection_shares_one_connection_across_statements() {
        let (_directory, handle) = fixture();

        with_connection(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            |connection| {
                connection
                    .execute_batch("CREATE TABLE shared (value INTEGER);")
                    .map_err(|error| error.to_string())?;
                connection
                    .execute_batch("INSERT INTO shared VALUES (42);")
                    .map_err(|error| error.to_string())?;
                Ok(())
            },
        )
        .expect("both statements");

        let result = query_json(
            handle.local.as_ref(),
            LogicalDatabase::SystemStatistics,
            "SELECT value FROM shared",
            &[],
        )
        .expect("query");
        assert_eq!(result.row_count, 1);
    }

    #[test]
    fn an_unbound_logical_database_is_refused() {
        let (_directory, handle) = fixture();

        let result = query_json(
            handle.local.as_ref(),
            LogicalDatabase::AppState,
            "SELECT 1",
            &[],
        );
        assert!(
            result.is_err(),
            "a logical database without a binding must be refused"
        );
    }

    #[test]
    fn json_parameters_convert_and_refuse_containers() {
        assert_eq!(
            json_param(serde_json::json!(7)).expect("integer"),
            SqliteValue::Integer(7)
        );
        assert!(json_param(serde_json::json!([1, 2])).is_err());

        let list = json_params(vec![serde_json::json!(null), serde_json::json!("text")])
            .expect("scalar list");
        assert_eq!(
            list,
            vec![SqliteValue::Null, SqliteValue::Text("text".into())]
        );
    }

    #[test]
    fn database_path_reports_the_bound_file() {
        let (directory, handle) = fixture();

        let path = database_path(handle.local.as_ref(), LogicalDatabase::SystemStatistics)
            .expect("bound path");
        assert_eq!(path, directory.path().join("statistics.sqlite3"));
        assert!(database_path(handle.local.as_ref(), LogicalDatabase::KnowledgeBase).is_err());
    }

    #[test]
    fn unavailability_classification_covers_every_local_failure() {
        use vulcan_lancedb_engine::error::LanceDbEngineError;
        use vulcan_sqlite_engine::error::SqliteEngineError;

        assert!(is_unavailable(&LocalDatabaseError::Unbound {
            logical: "app-state"
        }));
        assert!(is_unavailable(&LocalDatabaseError::HealthCheck {
            logical: "app-state",
            detail: String::new()
        }));
        assert!(is_unavailable(&LocalDatabaseError::ShutDown));
        // A schema-version mismatch is a correctness failure rather than an availability failure,
        // so it must not be classified as unavailable.
        // 结构版本不匹配是正确性失败而非可用性失败，因此不得归类为不可用。
        assert!(!is_unavailable(
            &LocalDatabaseError::SchemaVersionMismatch {
                expected: 1,
                observed: 0
            }
        ));

        // The decisive case: one wrapper variant carries both kinds, so the classification has to
        // come from the engine error underneath it.
        // 决定性用例：同一个包装变体同时承载两类，因此分类必须来自其下的引擎错误。
        let open_failure = LocalDatabaseError::SqliteFile {
            path: std::path::PathBuf::from("library.sqlite3"),
            source: SqliteEngineError::Open {
                path: std::path::PathBuf::from("library.sqlite3"),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "denied",
                )),
            },
        };
        assert!(
            is_unavailable(&open_failure),
            "a file that could not be opened is unavailable, so a retry is worth attempting"
        );

        let conflict = LocalDatabaseError::SqliteFile {
            path: std::path::PathBuf::from("library.sqlite3"),
            source: SqliteEngineError::ResourceConflict {
                path: std::path::PathBuf::from("library.sqlite3"),
                detail: "already open with different options".to_string(),
            },
        };
        assert!(
            !is_unavailable(&conflict),
            "an options conflict is a programming error, so retrying cannot clear it"
        );

        // The same distinction must hold for the engine wrapped by the logical variant, and for the
        // LanceDB engine.
        // 同一区分必须也适用于逻辑变体所包装的引擎，以及 LanceDB 引擎。
        assert!(!is_unavailable(&LocalDatabaseError::Sqlite {
            logical: "app-state",
            source: SqliteEngineError::invalid_argument("empty SQL"),
        }));
        assert!(is_unavailable(&LocalDatabaseError::LanceDb(
            LanceDbEngineError::LockHeld {
                lock_path: std::path::PathBuf::from("vmm.lock"),
                owner: "other-process".to_string(),
            }
        )));
        assert!(!is_unavailable(&LocalDatabaseError::LanceDb(
            LanceDbEngineError::TableNotFound {
                table_name: "vmm_vec_missing".to_string(),
            }
        )));
    }
}

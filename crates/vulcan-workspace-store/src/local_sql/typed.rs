//! Typed SQL execution before legacy display adaptation.
//! 旧展示适配之前的类型化 SQL 执行。

use super::{DatabaseTarget, OwnedDatabaseTarget, SqlInterruptTimer};
use crate::local_database_runtime::{
    BlockingBoundaryError, DatabaseKey, LocalDatabaseError, LocalDatabaseRuntime,
};
use std::sync::Arc;
use vulcan_sqlite_engine::error::SqliteEngineError;
use vulcan_sqlite_engine::sql_exec::{self, QueryJsonLimits, QueryJsonResult, SqliteValue};

/// Failures retain their source through admission, connection ownership and execution.
/// 失败在准入、连接持有和执行期间保留原始原因。
#[derive(Debug, thiserror::Error)]
pub enum LocalSqlError {
    /// Runtime binding or connection failure.
    /// 运行时绑定或连接失败。
    #[error(transparent)]
    Database(#[from] LocalDatabaseError),
    /// SQL engine failure, retaining SQLite's native error code where available.
    /// SQL 引擎失败，在可用时保留 SQLite 原生错误码。
    #[error(transparent)]
    Engine(#[from] SqliteEngineError),
    /// The original deadline expired, including time spent queued.
    /// 原始截止时间已到，包括排队耗时。
    #[error("SQLite query deadline exceeded; execution and connection cleanup completed.")]
    DeadlineExceeded,
    /// The blocking task failed independently of SQL execution.
    /// 阻塞任务发生独立于 SQL 执行的失败。
    #[error("blocking database operation did not run to completion: {0}")]
    Task(String),
    /// Legacy business callbacks explicitly retain their display text without invented classification.
    /// 旧业务回调显式保留展示文本，不猜测错误类别。
    #[error("{0}")]
    LegacyOperation(String),
}

impl LocalSqlError {
    /// Reports whether SQLite answered busy or locked, whether during open or execution.
    /// 报告 SQLite 在打开或执行期间是否回复忙或已锁定。
    pub fn is_busy(&self) -> bool {
        match self {
            Self::Engine(error)
            | Self::Database(LocalDatabaseError::Sqlite { source: error, .. }) => error.is_busy(),
            _ => false,
        }
    }

    /// Returns a stable machine code independent of the diagnostic message; takes only this error.
    /// 仅接收当前错误，返回不依赖诊断文案的稳定机器码。
    pub fn code(&self) -> &'static str {
        match self {
            Self::DeadlineExceeded => "database_deadline_exceeded",
            Self::Task(_) => "database_task_failed",
            Self::LegacyOperation(_) => "database_operation_failed",
            Self::Database(LocalDatabaseError::Unbound { .. }) => "database_unbound",
            Self::Database(LocalDatabaseError::ShutDown) => "database_shutdown",
            Self::Database(_) => "database_runtime_error",
            Self::Engine(error) => match error {
                SqliteEngineError::InvalidArgument { .. } => "sqlite_invalid_argument",
                SqliteEngineError::Open { .. } => "sqlite_open_failed",
                SqliteEngineError::ResourceConflict { .. } => "sqlite_resource_conflict",
                SqliteEngineError::PathNotAuthorized { .. } => "sqlite_path_not_authorized",
                SqliteEngineError::LockHeld { .. } => "sqlite_lock_held",
                SqliteEngineError::LockIo { .. } => "sqlite_lock_io",
                SqliteEngineError::Sql { .. } => "sqlite_statement_failed",
                SqliteEngineError::BatchItem { .. } => "sqlite_batch_failed",
                SqliteEngineError::Serialization { .. } => "sqlite_serialization_failed",
                SqliteEngineError::QueryStreamState { .. } => "sqlite_stream_unavailable",
                SqliteEngineError::ResultLimitExceeded { .. } => "sqlite_result_limit",
                SqliteEngineError::Tokenizer { .. } => "sqlite_tokenizer_failed",
            },
        }
    }
}

/// Converts a boundary failure without inspecting display text; returns the original operation error.
/// 转换边界失败而不检查展示文本；返回原始操作错误。
fn boundary_error(error: BlockingBoundaryError<LocalSqlError>) -> LocalSqlError {
    match error {
        BlockingBoundaryError::Operation(error) => error,
        BlockingBoundaryError::DeadlineExceeded => LocalSqlError::DeadlineExceeded,
        BlockingBoundaryError::Task { detail } => LocalSqlError::Task(detail),
    }
}

/// Resolves target against runtime; returns the physical gate identity or a typed source failure.
/// 根据运行时解析目标；返回物理闸门身份或类型化原始失败。
pub(super) fn key(
    target: &OwnedDatabaseTarget,
    runtime: &LocalDatabaseRuntime,
) -> Result<DatabaseKey, LocalSqlError> {
    // The binding remains authoritative; filesystem normalization supplies one physical gate.
    // 绑定保持权威；文件系统规范化提供唯一物理闸门。
    let path = match target {
        OwnedDatabaseTarget::Logical(logical) => runtime.path_for(*logical)?,
        OwnedDatabaseTarget::File(binding) => binding.path.clone(),
    };
    Ok(DatabaseKey::File(
        vulcan_sqlite_engine::runtime::normalize_db_path(&path)?,
    ))
}

/// Opens target within runtime; returns its owned connection without rendering the failure.
/// 在运行时打开目标；返回其独占连接，不渲染失败。
pub fn connect<'a>(
    runtime: &LocalDatabaseRuntime,
    target: impl Into<DatabaseTarget<'a>>,
) -> Result<rusqlite::Connection, LocalSqlError> {
    match target.into() {
        DatabaseTarget::Logical(logical) => runtime.connect(logical).map_err(Into::into),
        DatabaseTarget::File(binding) => runtime.connect_sqlite_file(binding).map_err(Into::into),
    }
}

/// Executes body using runtime and target within deadline; returns its typed result after cleanup.
/// 使用运行时和目标，在截止时间内执行回调；清理完成后返回类型化结果。
/// Cancellation of the waiter never releases the worker's permit or connection early.
/// 等待方取消绝不会提前释放工作线程的许可或连接。
pub async fn with_connection_until_async<T: Send + 'static>(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    deadline: Option<std::time::Instant>,
    body: impl FnOnce(&mut rusqlite::Connection) -> Result<T, LocalSqlError> + Send + 'static,
) -> Result<T, LocalSqlError> {
    // Preparation owns its own copy until the physical gate is known.
    // 准备阶段持有自身副本，直到确定物理闸门。
    let target = target.into();
    let preparation_target = target.clone();
    let preparation_runtime = Arc::clone(runtime);
    let key = runtime
        .boundary()
        .prepare_until(deadline, move || {
            key(&preparation_target, &preparation_runtime)
        })
        .await
        .map_err(boundary_error)?;
    // The worker and interrupt timer share only this call's connection.
    // 工作线程和中断计时器只共享本次调用的连接。
    let shared = Arc::clone(runtime);
    let scheduler = tokio::runtime::Handle::current();
    runtime
        .boundary()
        .run_until(key, deadline, move || {
            let mut connection = connect(shared.as_ref(), target.as_target())?;
            let Some(deadline) = deadline else {
                return body(&mut connection);
            };
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .ok_or(LocalSqlError::DeadlineExceeded)?;
            connection
                .busy_timeout(remaining)
                .map_err(SqliteEngineError::sql)?;
            let interrupt = connection.get_interrupt_handle();
            let _timer = SqlInterruptTimer(scheduler.spawn(async move {
                tokio::time::sleep_until(deadline.into()).await;
                interrupt.interrupt();
            }));
            body(&mut connection)
        })
        .await
        .map_err(boundary_error)
}

/// Queries sql and params on target using limits and the original deadline; returns rows or typed failure.
/// 在目标上使用上限和原始截止时间查询 SQL 与参数；返回结果行或类型化失败。
pub async fn query_json_bounded_until_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
    limits: QueryJsonLimits,
    deadline: std::time::Instant,
) -> Result<QueryJsonResult, LocalSqlError> {
    with_connection_until_async(runtime, target, Some(deadline), move |connection| {
        let result = sql_exec::query_json_bounded(connection, &sql, &params, limits);
        if std::time::Instant::now() >= deadline {
            return Err(LocalSqlError::DeadlineExceeded);
        }
        result.map_err(Into::into)
    })
    .await
}

/// Executes sql with params on target without a deadline; returns the engine result or typed failure.
/// 在目标上无截止时间地执行 SQL 与参数；返回引擎结果或类型化失败。
pub async fn execute_script_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
) -> Result<sql_exec::ExecuteScriptResult, LocalSqlError> {
    with_connection_until_async(runtime, target, None, move |connection| {
        sql_exec::execute_script(connection, &sql, &params).map_err(Into::into)
    })
    .await
}

/// Queries sql with params on target without a deadline; returns JSON rows or typed failure.
/// 在目标上无截止时间地查询 SQL 与参数；返回 JSON 行或类型化失败。
pub async fn query_json_async(
    runtime: &Arc<LocalDatabaseRuntime>,
    target: impl Into<OwnedDatabaseTarget>,
    sql: String,
    params: Vec<SqliteValue>,
) -> Result<QueryJsonResult, LocalSqlError> {
    with_connection_until_async(runtime, target, None, move |connection| {
        sql_exec::query_json(connection, &sql, &params).map_err(Into::into)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_database_runtime::LogicalDatabase;

    /// Runs real SQL with caller limits; returns typed errors without comparing their message.
    /// 使用调用方上限执行真实 SQL；返回类型化错误，不比较文案。
    async fn query(
        runtime: &Arc<LocalDatabaseRuntime>,
        sql: &str,
        max_rows: u64,
    ) -> Result<QueryJsonResult, LocalSqlError> {
        query_json_bounded_until_async(
            runtime,
            LogicalDatabase::SystemStatistics,
            sql.into(),
            Vec::new(),
            QueryJsonLimits {
                max_rows,
                max_bytes: usize::MAX,
            },
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        )
        .await
    }

    /// Checks runtime, engine, task and deadline categories using actual boundary failures.
    /// 使用真实边界失败检查运行时、引擎、任务和截止时间类别。
    #[tokio::test]
    async fn classifications_preserve_sources_and_release_failed_work() {
        // Each test owns its isolated database and runtime.
        // 每个测试持有独立数据库和运行时。
        let directory = tempfile::tempdir().unwrap();
        let runtime = Arc::new(
            LocalDatabaseRuntime::open(std::collections::HashMap::from([(
                LogicalDatabase::SystemStatistics,
                directory.path().join("typed.sqlite3"),
            )]))
            .unwrap(),
        );
        let unbound = connect(runtime.as_ref(), LogicalDatabase::AppState).unwrap_err();
        assert_eq!(unbound.code(), "database_unbound");
        let syntax = query(&runtime, "SELECT FROM", 10).await.unwrap_err();
        assert_eq!(syntax.code(), "sqlite_statement_failed");
        assert!(matches!(
            syntax,
            LocalSqlError::Engine(SqliteEngineError::Sql { .. })
        ));
        let limit = query(&runtime, "SELECT 1 UNION ALL SELECT 2", 1)
            .await
            .unwrap_err();
        assert_eq!(limit.code(), "sqlite_result_limit");
        assert!(matches!(
            limit,
            LocalSqlError::Engine(SqliteEngineError::ResultLimitExceeded { .. })
        ));
        let deadline = with_connection_until_async(
            &runtime,
            LogicalDatabase::SystemStatistics,
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            |_| -> Result<(), LocalSqlError> { panic!("expired work must not execute") },
        )
        .await
        .unwrap_err();
        assert!(matches!(deadline, LocalSqlError::DeadlineExceeded));
        let panic = with_connection_until_async(
            &runtime,
            LogicalDatabase::SystemStatistics,
            None,
            |_| -> Result<(), LocalSqlError> { panic!("injected worker failure") },
        )
        .await
        .unwrap_err();
        assert_eq!(panic.code(), "database_task_failed");
        assert_eq!(query(&runtime, "SELECT 1", 1).await.unwrap().row_count, 1);
        runtime.shutdown();
        let shutdown = query(&runtime, "SELECT 1", 1).await.unwrap_err();
        assert_eq!(shutdown.code(), "database_shutdown");
    }

    /// Confirms changed or misleading diagnostics cannot alter machine classification.
    /// 确认变化或误导性的诊断文案不会改变机器分类。
    #[test]
    fn classification_never_matches_display_text() {
        for text in ["deadline exceeded", "database shutdown", "任意诊断"] {
            let error = LocalSqlError::Engine(SqliteEngineError::result_limit_exceeded(text));
            assert_eq!(error.code(), "sqlite_result_limit");
            assert!(matches!(
                error,
                LocalSqlError::Engine(SqliteEngineError::ResultLimitExceeded { .. })
            ));
        }
    }
}

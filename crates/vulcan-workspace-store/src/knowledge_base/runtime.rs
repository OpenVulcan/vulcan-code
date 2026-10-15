//! Shared per-library concurrency and exact-version storage runtime.
//! 共享每库并发与精确版本存储运行时。

mod authorization;
mod content;
mod fts;
mod library;
mod storage;

use super::models::*;
use super::schema::{KNOWLEDGE_FTS_INDEX, KNOWLEDGE_SCHEMA_VERSION};
use crate::data_plane::HostRuntimeHandle;
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use storage::{KnowledgeManifest, validate_library_id};
use tokio::sync::Mutex;
use uuid::Uuid;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Cloneable runtime; wrappers over one host share locks, bindings and workers.
/// 可克隆运行时；同一宿主上的包装共享锁、绑定与工作器。
#[derive(Clone)]
pub struct KnowledgeBaseRuntime {
    /// Established host data plane.
    /// 既有宿主数据面。
    host_runtime: HostRuntimeHandle,
    /// Shared process-local coordination.
    /// 共享进程内协调。
    shared: Arc<KnowledgeBaseRuntimeSharedState>,
}

/// State held by the host rather than by a short-lived API wrapper.
/// 由宿主持有而非短生命周期接口包装持有的状态。
#[derive(Default)]
pub(crate) struct KnowledgeBaseRuntimeSharedState {
    /// Per-library exclusive mutation locks.
    /// 每库独占变更锁。
    mutation_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    /// Serializes binding activation and closure.
    /// 串行化绑定激活和关闭。
    binding_activation_lock: Mutex<()>,
    /// Active SQLite bindings.
    /// 活动 SQLite 绑定。
    active_bindings: Mutex<BTreeSet<String>>,
    /// Active LanceDB bindings.
    /// 活动 LanceDB 绑定。
    active_vector_bindings: Mutex<BTreeSet<String>>,
}

impl KnowledgeBaseRuntime {
    /// Wraps the established host; returns a runtime sharing its exact mutation domain.
    /// 包装既有宿主；返回共享其精确变更域的运行时。
    pub fn new(host_runtime: HostRuntimeHandle) -> Self {
        // The host is the only owner so independently rebuilt service/tool wrappers coordinate.
        // 宿主是唯一所有者，确保独立重建的服务和工具包装协调一致。
        let shared = host_runtime.knowledge_runtime_state.clone();
        Self {
            host_runtime,
            shared,
        }
    }

    /// Acquires the exclusive lock for a canonical library UUID; returns an owned guard.
    /// 获取规范知识库 UUID 的独占锁；返回自有守卫。
    async fn library_mutation_guard(
        &self,
        library_id: &str,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        validate_library_id(library_id)?;
        // Each UUID always resolves to the same host-owned lock.
        // 每个 UUID 始终解析到同一宿主锁。
        let lock = self
            .shared
            .mutation_locks
            .lock()
            .await
            .entry(library_id.into())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        Ok(lock.lock_owned().await)
    }
}

/// Encodes a bound SQL string; returns the typed value.
/// 编码绑定 SQL 字符串；返回类型化值。
fn text(value: impl Into<String>) -> SqliteValue {
    SqliteValue::Text(value.into())
}
/// Encodes a bound SQL integer; returns the typed value.
/// 编码绑定 SQL 整数；返回类型化值。
fn int(value: i64) -> SqliteValue {
    SqliteValue::Integer(value)
}
/// Converts a public nonnegative count to SQLite's signed integer range; rejects overflow.
/// 将公共非负计数转换到 SQLite 有符号整数范围；拒绝溢出。
fn sql_integer<T: TryInto<i64>>(value: T) -> Result<i64, String> {
    value
        .try_into()
        .map_err(|_| "ERR_KNOWLEDGE_INTEGER_RANGE".into())
}
/// Reads a checked nonnegative SQLite count; rejects negative and out-of-range stored values.
/// 读取已检查的非负 SQLite 计数；拒绝负数及超范围存储值。
fn unsigned_column<T: TryFrom<i64>>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T> {
    // Stored counts must never be wrapped or silently clamped.
    // 存储计数绝不回绕或静默钳制。
    let value = row.get::<_, i64>(index)?;
    T::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ERR_KNOWLEDGE_INTEGER_RANGE",
            )),
        )
    })
}
/// Reads the wall clock; returns checked Unix milliseconds or an explicit range error.
/// 读取墙钟；返回已检查的 Unix 毫秒或明确范围错误。
fn unix_epoch_millis() -> Result<i64, String> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;

//! Host-registered VLDB bindings exposed to trusted Vulcan plugins.
//! 向受信 Vulcan 插件暴露的宿主注册 VLDB 绑定。

use crate::plugin_storage::{
    PLUGIN_SQLITE_MAX_QUERY_ROWS, PLUGIN_SQLITE_QUERY_TIMEOUT, SqliteValuePayload,
    decode_sqlite_params,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tokio::runtime::Handle;
use vulcan_plugin_runtime::{
    VulcanPluginVldbOperation, VulcanPluginVldbRequest, VulcanPluginVldbService,
};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::local_database_runtime::LogicalDatabase;
use vulcan_workspace_store::local_sql::{self, BoundedQueryError, QueryJsonLimits};
use vulcan_workspace_store::workspace_store::lock_provider_usage_writes;

/// Stable logical binding exposed to trusted statistics plugins.
/// 向受信统计插件暴露的稳定逻辑绑定。
pub(crate) const SYSTEM_STATISTICS_LOGICAL_BINDING: &str = "system.statistics";

/// Maximum encoded JSON bytes returned by one host VLDB query.
/// 单次宿主 VLDB 查询返回的最大 JSON 编码字节数。
///
/// Sized independently from the plugin-owned SQLite byte budget; rows and timeout are shared.
/// 与插件自有 SQLite 字节预算独立设定；行数与超时共用同一定义。
const HOST_VLDB_SQLITE_MAX_QUERY_BYTES: usize = 2 * 1024 * 1024;

/// Host coordinates for one registered logical SQLite binding.
/// 一个宿主注册逻辑 SQLite 绑定的坐标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisteredSqliteBindingKind {
    /// Host-owned Provider usage facts with the shared statistics mutation gate.
    /// 使用共享统计变更门禁的宿主 Provider 用量事实。
    SystemStatistics,
}

/// Host coordinates and consistency policy for one registered logical SQLite binding.
/// 一个已注册逻辑 SQLite 绑定的宿主坐标与一致性策略。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredSqliteBinding {
    /// Binding kind selecting the exact mutation-consistency boundary.
    /// 选择精确变更一致性边界的绑定种类。
    kind: RegisteredSqliteBindingKind,
    /// Host logical database this entry resolves to.
    /// 本条目解析到的宿主逻辑数据库。
    ///
    /// A logical name rather than a physical path or a controller coordinate, so a plugin can never name a
    /// file, and the host never has to hand one out.
    /// 逻辑名而非物理路径或引擎坐标，因此插件绝不可能命名文件，宿主也无需交出文件。
    database: LogicalDatabase,
}

/// Builds the exact logical SQLite binding table for one host bootstrap.
/// 为一次宿主引导构建精确的逻辑 SQLite 绑定表。
///
/// # Parameters
/// 参数
/// `system_statistics_binding_id` is the bootstrap-scoped host binding identity.
/// `system_statistics_binding_id` 是单次引导作用域的宿主绑定身份。
///
/// # Returns
/// 返回值
/// Returns a logical-name registry that never contains a physical database path.
/// 返回一个不包含物理数据库路径的逻辑名注册表。
fn build_registered_sqlite_bindings() -> BTreeMap<String, RegisteredSqliteBinding> {
    BTreeMap::from([(
        String::from(SYSTEM_STATISTICS_LOGICAL_BINDING),
        RegisteredSqliteBinding {
            kind: RegisteredSqliteBindingKind::SystemStatistics,
            database: LogicalDatabase::SystemStatistics,
        },
    )])
}

/// In-process host VLDB service for trusted plugin requests.
/// 面向受信插件请求的进程内宿主 VLDB 服务。
pub(crate) struct PluginVldbService {
    /// Active host runtime handle containing registered binding identities.
    /// 包含已注册绑定身份的活动宿主运行时句柄。
    host_runtime: HostRuntimeHandle,
    /// Service runtime used to bridge the synchronous LuaSkills callback.
    /// 用于桥接同步 LuaSkills 回调的服务运行时。
    runtime: Handle,
    /// Exact logical SQLite bindings registered by the current host bootstrap.
    /// 当前宿主引导注册的精确逻辑 SQLite 绑定。
    sqlite_bindings: BTreeMap<String, RegisteredSqliteBinding>,
}

impl PluginVldbService {
    /// Builds one host VLDB service from the active host runtime handle.
    /// 使用活动宿主运行时句柄构建一个宿主 VLDB 服务。
    ///
    /// # Parameters
    /// 参数
    /// `host_runtime` owns logical binding coordinates and `runtime` executes asynchronous engine calls.
    /// `host_runtime` 持有逻辑绑定坐标，`runtime` 执行异步引擎调用。
    ///
    /// # Returns
    /// 返回值
    /// Returns a ready service adapter.
    /// 返回一个就绪的服务适配器。
    pub(crate) fn new(host_runtime: HostRuntimeHandle, runtime: Handle) -> Self {
        // sqliteBindings is the host-owned registration table; plugins can name only these logical entries.
        // sqliteBindings 是宿主持有的注册表；插件只能命名其中的逻辑条目。
        let sqlite_bindings = build_registered_sqlite_bindings();
        Self {
            host_runtime,
            runtime,
            sqlite_bindings,
        }
    }

    /// Executes one authenticated host VLDB operation asynchronously.
    /// 异步执行一项已认证宿主 VLDB 操作。
    async fn execute_async(&self, request: VulcanPluginVldbRequest) -> Result<Value, String> {
        if request.plugin_id.trim().is_empty() {
            return Err(String::from(
                "Vulcan plugin VLDB request requires plugin identity.",
            ));
        }
        match request.operation {
            VulcanPluginVldbOperation::SqliteQuery => self.sqlite_query(request.payload).await,
            VulcanPluginVldbOperation::SqliteExecute => self.sqlite_execute(request.payload).await,
            VulcanPluginVldbOperation::SqliteExecuteBatch => {
                self.sqlite_execute_batch(request.payload).await
            }
            VulcanPluginVldbOperation::SqliteFlush => self.sqlite_flush(request.payload).await,
        }
    }

    /// Resolves one exact logical binding without accepting physical engine coordinates.
    /// 解析一个精确逻辑绑定，且不接受物理引擎坐标。
    fn resolve_sqlite_binding(
        &self,
        logical_binding: &str,
    ) -> Result<&RegisteredSqliteBinding, String> {
        self.sqlite_bindings.get(logical_binding).ok_or_else(|| {
            format!("Vulcan plugin VLDB binding `{logical_binding}` is not registered.")
        })
    }

    /// Queries one registered host SQLite binding.
    /// 查询一个已注册宿主 SQLite 绑定。
    async fn sqlite_query(&self, payload: Value) -> Result<Value, String> {
        let request = serde_json::from_value::<HostVldbSqliteQueryPayload>(payload)
            .map_err(|error| format!("Invalid Vulcan plugin VLDB query payload: {error}"))?;
        let binding = self.resolve_sqlite_binding(request.binding.as_str())?;
        let params = decode_sqlite_params(request.params)?;
        let result = local_sql::query_json_bounded_async(
            &self.host_runtime.local,
            binding.database,
            request.sql,
            params,
            QueryJsonLimits {
                max_rows: PLUGIN_SQLITE_MAX_QUERY_ROWS,
                max_bytes: HOST_VLDB_SQLITE_MAX_QUERY_BYTES,
            },
            PLUGIN_SQLITE_QUERY_TIMEOUT,
        )
        .await
        .map_err(|error| match error {
            BoundedQueryError::TimedOut => format!(
                "Vulcan plugin VLDB query exceeded the {} second timeout.",
                PLUGIN_SQLITE_QUERY_TIMEOUT.as_secs()
            ),
            BoundedQueryError::Failed(error) => {
                format!("Vulcan plugin VLDB query failed: {error}")
            }
        })?;
        let rows = serde_json::from_str::<Value>(&result.json_data)
            .map_err(|error| format!("SQLite engine returned invalid VLDB rows: {error}"))?;
        Ok(json!({ "rows": rows, "row_count": result.row_count }))
    }

    /// Executes one parameterized write against a registered host SQLite binding.
    /// 对一个已注册宿主 SQLite 绑定执行一条参数化写入。
    async fn sqlite_execute(&self, payload: Value) -> Result<Value, String> {
        let request = serde_json::from_value::<HostVldbSqliteExecutePayload>(payload)
            .map_err(|error| format!("Invalid Vulcan plugin VLDB execute payload: {error}"))?;
        let binding = self.resolve_sqlite_binding(request.binding.as_str())?;
        let params = decode_sqlite_params(request.params)?;
        // guard prevents direct plugin maintenance from splitting the core attempt-write and aggregate-rebuild sequence.
        // guard 防止插件直接维护操作切断内核的调用写入与聚合重建序列。
        let _guard = match binding.kind {
            RegisteredSqliteBindingKind::SystemStatistics => {
                Some(lock_provider_usage_writes().await)
            }
        };
        let result = local_sql::execute_script_async(
            &self.host_runtime.local,
            binding.database,
            request.sql,
            params,
        )
        .await
        .map_err(|error| format!("Vulcan plugin VLDB execute failed: {error}"))?;
        // The host adapter builds the plugin-visible JSON, because the engine deliberately does not
        // implement serialization: JSON may only be produced at a host adapter.
        // 宿主适配器构造插件可见的 JSON，因为引擎刻意不实现序列化：JSON 只能在宿主适配器处产生。
        Ok(json!({
            "success": result.success,
            "message": result.message,
            "rows_changed": result.rows_changed,
            "last_insert_rowid": result.last_insert_rowid,
        }))
    }

    /// Executes one homogeneous parameter batch atomically against a registered binding.
    /// 对一个已注册绑定原子执行一组同构参数批次。
    async fn sqlite_execute_batch(&self, payload: Value) -> Result<Value, String> {
        let request = serde_json::from_value::<HostVldbSqliteExecuteBatchPayload>(payload)
            .map_err(|error| format!("Invalid Vulcan plugin VLDB batch payload: {error}"))?;
        let binding = self.resolve_sqlite_binding(request.binding.as_str())?;
        let items = request
            .items
            .into_iter()
            .map(decode_sqlite_params)
            .collect::<Result<Vec<_>, _>>()?;
        // guard gives one logical binding a shared mutation boundary without adding a plugin-private storage lane.
        // guard 为同一逻辑绑定提供共享变更边界，且不增加插件私有存储队列。
        let _guard = match binding.kind {
            RegisteredSqliteBindingKind::SystemStatistics => {
                Some(lock_provider_usage_writes().await)
            }
        };
        let result = local_sql::execute_batch_async(
            &self.host_runtime.local,
            binding.database,
            request.sql,
            items,
        )
        .await
        .map_err(|error| format!("Vulcan plugin VLDB batch execute failed: {error}"))?;
        // Same host-adapter boundary as the single-statement result, plus the executed-group count.
        // 与单语句结果相同的宿主适配器边界，另加实际执行的参数组数量。
        Ok(json!({
            "success": result.success,
            "message": result.message,
            "rows_changed": result.rows_changed,
            "last_insert_rowid": result.last_insert_rowid,
            "statements_executed": result.statements_executed,
        }))
    }

    /// Checkpoints one registered host SQLite binding without exposing its physical path.
    /// 在不暴露物理路径的情况下检查点化一个已注册宿主 SQLite 绑定。
    async fn sqlite_flush(&self, payload: Value) -> Result<Value, String> {
        let request = serde_json::from_value::<HostVldbSqliteFlushPayload>(payload)
            .map_err(|error| format!("Invalid Vulcan plugin VLDB flush payload: {error}"))?;
        let binding = self.resolve_sqlite_binding(request.binding.as_str())?;
        let result = local_sql::query_json_async(
            &self.host_runtime.local,
            binding.database,
            "PRAGMA wal_checkpoint(PASSIVE)",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("Vulcan plugin VLDB flush failed: {error}"))?;
        Ok(json!({ "flushed": true, "row_count": result.row_count }))
    }
}

impl VulcanPluginVldbService for PluginVldbService {
    /// Executes one authenticated request on the service runtime.
    /// 在服务运行时上执行一条已认证请求。
    fn execute(&self, request: VulcanPluginVldbRequest) -> Result<Value, String> {
        self.runtime.block_on(self.execute_async(request))
    }
}

/// Strict payload for one registered host SQLite query.
/// 单次已注册宿主 SQLite 查询的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostVldbSqliteQueryPayload {
    /// Stable logical binding name registered by the host.
    /// 宿主注册的稳定逻辑绑定名称。
    binding: String,
    /// Parameterized SQLite query.
    /// 参数化 SQLite 查询。
    sql: String,
    /// Ordered tagged scalar parameters.
    /// 有序标记标量参数。
    #[serde(default)]
    params: Vec<SqliteValuePayload>,
}

/// Strict payload for one registered host SQLite write.
/// 单次已注册宿主 SQLite 写入的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostVldbSqliteExecutePayload {
    /// Stable logical binding name registered by the host.
    /// 宿主注册的稳定逻辑绑定名称。
    binding: String,
    /// Parameterized SQLite statement.
    /// 参数化 SQLite 语句。
    sql: String,
    /// Ordered tagged scalar parameters.
    /// 有序标记标量参数。
    #[serde(default)]
    params: Vec<SqliteValuePayload>,
}

/// Strict payload for one registered host SQLite batch.
/// 单次已注册宿主 SQLite 批次的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostVldbSqliteExecuteBatchPayload {
    /// Stable logical binding name registered by the host.
    /// 宿主注册的稳定逻辑绑定名称。
    binding: String,
    /// Parameterized SQLite statement reused for every item.
    /// 为每个条目复用的参数化 SQLite 语句。
    sql: String,
    /// Ordered parameter groups executed in one in-process SQLite transaction.
    /// 在单个进程内 SQLite 事务中执行的有序参数组。
    items: Vec<Vec<SqliteValuePayload>>,
}

/// Strict payload for one registered host SQLite checkpoint.
/// 单次已注册宿主 SQLite 检查点的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostVldbSqliteFlushPayload {
    /// Stable logical binding name registered by the host.
    /// 宿主注册的稳定逻辑绑定名称。
    binding: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the host registry exposes only the stable statistics logical name.
    /// 验证宿主注册表仅暴露稳定统计逻辑名。
    #[test]
    fn system_statistics_binding_registration_is_exact() {
        let bindings = build_registered_sqlite_bindings();

        assert_eq!(bindings.len(), 1);
        let binding = bindings
            .get(SYSTEM_STATISTICS_LOGICAL_BINDING)
            .expect("resolve system statistics logical binding");
        assert_eq!(binding.kind, RegisteredSqliteBindingKind::SystemStatistics);
        // The registry resolves to a host logical database, never to a path or a controller coordinate.
        // 注册表解析到宿主逻辑数据库，绝不解析到路径或引擎坐标。
        assert_eq!(binding.database, LogicalDatabase::SystemStatistics);
        assert!(!bindings.contains_key("provider-usage@test"));
    }
}

//! In-process isolated storage service for authorized Vulcan Plugins.
//! 面向已授权 Vulcan 插件的进程内隔离存储服务。

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use vulcan_lancedb_engine::engine::LanceDbEngine;
use vulcan_lancedb_engine::types::{
    LanceDbColumnDef, LanceDbColumnType, LanceDbCreateTableInput, LanceDbDeleteInput,
    LanceDbDropTableInput, LanceDbInputFormat, LanceDbOutputFormat, LanceDbSearchInput,
    LanceDbUpsertInput,
};
use vulcan_plugin_runtime::{
    VulcanPluginStorageOperation, VulcanPluginStorageRequest, VulcanPluginStorageService,
};
use vulcan_sqlite_engine::sql_exec::SqliteValue;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::local_database_runtime::{
    LanceDbDirectoryBinding, LocalDatabaseRuntime, SqliteFileBinding, SqliteHardeningOptions,
    SqliteOpenOptions, SqlitePragmaOptions,
};
use vulcan_workspace_store::local_sql::{self, BoundedQueryError, QueryJsonLimits};

/// Maximum SQLite rows returned by one plugin query, shared by plugin-owned and host VLDB bindings.
/// 单次插件 SQLite 查询允许返回的最大行数，插件自有库与宿主 VLDB 绑定共用。
pub(crate) const PLUGIN_SQLITE_MAX_QUERY_ROWS: u64 = 5_000;

/// Maximum encoded JSON bytes returned by one plugin query.
/// 单次插件查询允许返回的最大编码 JSON 字节数。
const PLUGIN_SQLITE_MAX_QUERY_BYTES: usize = 4 * 1024 * 1024;

/// Maximum wall-clock time allowed for one plugin SQLite query, shared by plugin-owned and host VLDB bindings.
/// 单次插件 SQLite 查询允许占用的最长墙钟时间，插件自有库与宿主 VLDB 绑定共用。
pub(crate) const PLUGIN_SQLITE_QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum LanceDB search result count exposed to one plugin request.
/// 单次插件请求允许返回的最大 LanceDB 检索结果数。
const PLUGIN_LANCEDB_MAX_SEARCH_LIMIT: u32 = 5_000;

/// Maximum active and queued storage operations accepted for one plugin.
/// 单个插件允许接收的活动与排队存储操作上限。
const PLUGIN_STORAGE_MAX_PENDING_OPERATIONS: usize = 64;

/// Per-plugin bounded serial lane that preserves migration and write ordering.
/// 按插件隔离的有界串行通道，用于保持迁移与写入顺序。
struct PluginStorageOperationLane {
    /// Serial execution gate shared by every backend operation for this plugin.
    /// 当前插件全部后端操作共享的串行执行门。
    serial_gate: AsyncMutex<()>,
    /// Capacity semaphore that rejects unbounded callback accumulation.
    /// 拒绝无界回调积压的容量信号量。
    capacity: Arc<Semaphore>,
}

impl PluginStorageOperationLane {
    /// Creates one empty bounded operation lane.
    /// 创建一个空的有界操作通道。
    ///
    /// # Returns
    /// 返回值
    /// Returns a serial lane with the host-defined pending-operation ceiling.
    /// 返回带有宿主定义待处理操作上限的串行通道。
    fn new() -> Self {
        Self {
            serial_gate: AsyncMutex::new(()),
            capacity: Arc::new(Semaphore::new(PLUGIN_STORAGE_MAX_PENDING_OPERATIONS)),
        }
    }
}

/// In-process storage adapter that derives every path and binding from plugin identity.
/// 根据插件身份派生全部路径与绑定的进程内存储适配器。
pub(crate) struct PluginStorageService {
    /// In-process database runtime that serves plugin storage.
    /// 服务插件存储的进程内数据库运行时。
    local: Arc<LocalDatabaseRuntime>,
    /// Runtime handle used to bridge synchronous Lua host callbacks into asynchronous engine calls.
    /// 用于把同步 Lua 宿主回调桥接到异步引擎调用的运行时句柄。
    runtime: Handle,
    /// User-owned plugin state root.
    /// 用户持有的插件状态根目录。
    plugin_state_root: PathBuf,
    /// Plugin identities whose SQLite binding has been enabled in this process.
    /// 当前进程中已启用 SQLite 绑定的插件身份集合。
    initialized_sqlite_plugins: Mutex<BTreeSet<String>>,
    /// Plugin identities whose LanceDB binding has been enabled in this process.
    /// 当前进程中已启用 LanceDB 绑定的插件身份集合。
    initialized_lancedb_plugins: Mutex<BTreeSet<String>>,
    /// Bounded serial storage lanes keyed by authenticated plugin identity.
    /// 以已认证插件身份为键的有界串行存储通道。
    operation_lanes: Mutex<BTreeMap<String, Arc<PluginStorageOperationLane>>>,
}

impl PluginStorageService {
    /// Creates one adapter over the host-owned in-process database runtime.
    /// 基于宿主持有的进程内数据库运行时创建一个适配器。
    ///
    /// # Parameters
    /// 参数
    /// `client` is the shared proxy, `runtime` owns async execution, and `plugin_state_root` contains isolated plugin data.
    /// `client` 是共享代理，`runtime` 承载异步执行，`plugin_state_root` 包含隔离插件数据。
    ///
    /// # Returns
    /// 返回值
    /// Returns an uninitialized lazy storage adapter.
    /// 返回一个尚未初始化且按需启用的存储适配器。
    pub(crate) fn new(
        host_runtime: &HostRuntimeHandle,
        runtime: Handle,
        plugin_state_root: PathBuf,
    ) -> Self {
        Self {
            local: host_runtime.local.clone(),
            runtime,
            plugin_state_root,
            initialized_sqlite_plugins: Mutex::new(BTreeSet::new()),
            initialized_lancedb_plugins: Mutex::new(BTreeSet::new()),
            operation_lanes: Mutex::new(BTreeMap::new()),
        }
    }

    /// Declares the LanceDB limits one plugin's vector store runs with.
    /// 声明某个插件向量存储使用的 LanceDB 限制。
    ///
    /// Every knob the controller's enable request stated is carried over, because the engine has an exact
    /// equivalent for each and a plugin's limits are part of its isolation contract.
    /// 旧边界声明的每一项限制都被沿用，因为引擎对每一项都有精确等价物，而插件限制属于其隔离契约。
    fn plugin_lancedb_binding(&self, plugin_id: &str) -> LanceDbDirectoryBinding {
        LanceDbDirectoryBinding {
            directory: plugin_data_root(&self.plugin_state_root, plugin_id).join("lancedb"),
            read_consistency_interval_ms: Some(250),
            max_upsert_payload: 16 * 1024 * 1024,
            max_search_limit: PLUGIN_LANCEDB_MAX_SEARCH_LIMIT as usize,
            max_concurrent_requests: 32,
        }
    }

    /// Returns the in-process LanceDB engine holding one plugin's vector tables.
    /// 返回保存某个插件向量表的进程内 LanceDB 引擎。
    ///
    /// # Errors
    /// 错误
    /// Returns an explicit error when the plugin's vector directory cannot be opened.
    /// 插件向量目录无法打开时返回显式错误。
    async fn vector_engine(&self, plugin_id: &str) -> Result<LanceDbEngine, String> {
        self.local
            .lance_engine(self.plugin_lancedb_binding(plugin_id))
            .await
            .map_err(|error| format!("Failed to open plugin LanceDB engine: {error}"))
    }

    /// Resolves the stable bounded operation lane for one authenticated plugin.
    /// 解析单个已认证插件的稳定有界操作通道。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the validated single-segment plugin identity.
    /// `plugin_id` 是已校验的单段插件身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns one shared plugin-local serial lane.
    /// 返回一个共享的插件本地串行通道。
    fn operation_lane(&self, plugin_id: &str) -> Result<Arc<PluginStorageOperationLane>, String> {
        // lanes is held only while resolving or inserting one small in-memory record.
        // lanes 仅在解析或插入一条小型内存记录时持有。
        let mut lanes = self
            .operation_lanes
            .lock()
            .map_err(|_| "Plugin storage operation-lane lock is poisoned.".to_string())?;
        Ok(Arc::clone(
            lanes
                .entry(plugin_id.to_string())
                .or_insert_with(|| Arc::new(PluginStorageOperationLane::new())),
        ))
    }

    /// Executes one request on the in-process engines after lazy backend initialization.
    /// 在按需初始化后端后，通过进程内引擎执行一条请求。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains the authenticated plugin identity and operation payload.
    /// `request` 包含已认证插件身份与操作载荷。
    ///
    /// # Returns
    /// 返回值
    /// Returns the JSON-safe storage result.
    /// 返回 JSON 安全的存储结果。
    async fn execute_async(&self, request: VulcanPluginStorageRequest) -> Result<Value, String> {
        validate_plugin_storage_identity(&request.plugin_id)?;
        // operationLane provides both bounded backpressure and exact per-plugin ordering.
        // operationLane 同时提供有界背压与精确的插件级操作顺序。
        let operation_lane = self.operation_lane(&request.plugin_id)?;
        // capacityPermit rejects new work instead of allowing unbounded synchronous callbacks to accumulate.
        // capacityPermit 拒绝新工作，避免同步回调无界积压。
        let _capacity_permit = Arc::clone(&operation_lane.capacity)
            .try_acquire_owned()
            .map_err(|_| {
                format!(
                    "Plugin `{}` storage queue is saturated at {} pending operations.",
                    request.plugin_id, PLUGIN_STORAGE_MAX_PENDING_OPERATIONS
                )
            })?;
        // serialGuard remains held through lazy initialization and the complete engine operation.
        // serialGuard 在按需初始化与完整引擎操作期间保持持有。
        let _serial_guard = operation_lane.serial_gate.lock().await;
        if request.operation.is_sqlite() {
            self.ensure_sqlite(&request.plugin_id).await?;
        } else {
            self.ensure_lancedb(&request.plugin_id).await?;
        }
        match request.operation {
            VulcanPluginStorageOperation::SqliteMigrate => {
                self.sqlite_migrate(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::SqliteExecute => {
                self.sqlite_execute(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::SqliteExecuteBatch => {
                self.sqlite_execute_batch(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::SqliteQuery => {
                self.sqlite_query(&request.plugin_id, request.payload).await
            }
            VulcanPluginStorageOperation::SqliteFlush => {
                self.sqlite_flush(&request.plugin_id).await
            }
            VulcanPluginStorageOperation::LanceDbCreateTable => {
                self.lancedb_create_table(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::LanceDbUpsert => {
                self.lancedb_upsert(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::LanceDbSearch => {
                self.lancedb_search(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::LanceDbDelete => {
                self.lancedb_delete(&request.plugin_id, request.payload)
                    .await
            }
            VulcanPluginStorageOperation::LanceDbDropTable => {
                self.lancedb_drop_table(&request.plugin_id, request.payload)
                    .await
            }
        }
    }

    /// Ensures the plugin SQLite space and binding exist exactly once per process.
    /// 确保插件 SQLite 空间与绑定在每个进程中只初始化一次。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the host-derived isolated data root.
    /// `plugin_id` 选择由宿主派生的隔离数据根。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after the binding is ready.
    /// 绑定就绪后返回空值。
    async fn ensure_sqlite(&self, plugin_id: &str) -> Result<(), String> {
        if self
            .initialized_sqlite_plugins
            .lock()
            .map_err(|_| "Plugin SQLite initialization lock is poisoned.".to_string())?
            .contains(plugin_id)
        {
            return Ok(());
        }
        // pluginDataRoot is the only physical root plugin SQLite storage can reach.
        // pluginDataRoot 是插件 SQLite 存储可触达的唯一物理根目录。
        let plugin_data_root = plugin_data_root(&self.plugin_state_root, plugin_id);

        fs::create_dir_all(&plugin_data_root).map_err(|error| {
            format!(
                "Failed to create plugin data directory `{}`: {error}",
                plugin_data_root.display()
            )
        })?;
        // The engine opens the file on the first statement and keeps it open for the process, because a
        // plugin's binding has no teardown in this adapter.
        // 引擎在第一条语句时打开该文件并为进程保持打开，因为本适配器中插件的绑定没有拆除路径。
        self.initialized_sqlite_plugins
            .lock()
            .map_err(|_| "Plugin SQLite initialization lock is poisoned.".to_string())?
            .insert(plugin_id.to_string());
        Ok(())
    }

    /// Builds one plugin's exact in-process SQLite binding.
    /// 构建某个插件精确的进程内 SQLite 绑定。
    ///
    /// The binding declares only options enforced by the in-process engine. Host-side runtime
    /// serialization provides the concurrency boundary instead of exposing a fictitious pool size.
    /// 此绑定只声明进程内引擎实际执行的选项。并发边界由宿主运行时串行化提供，不再暴露虚假的连接池大小。
    fn plugin_sqlite_binding(&self, plugin_id: &str) -> SqliteFileBinding {
        SqliteFileBinding {
            path: plugin_data_root(&self.plugin_state_root, plugin_id).join("plugin.sqlite3"),
            options: SqliteOpenOptions {
                busy_timeout_ms: 10_000,
                pragmas: SqlitePragmaOptions {
                    journal_mode: String::from("WAL"),
                    synchronous: String::from("NORMAL"),
                    foreign_keys: true,
                    temp_store: String::from("MEMORY"),
                    wal_autocheckpoint_pages: 1_000,
                    cache_size_kib: 16_384,
                    mmap_size_bytes: 64 * 1024 * 1024,
                },
                hardening: SqliteHardeningOptions {
                    enforce_db_file_lock: true,
                    read_only: false,
                    trusted_schema: false,
                    defensive: true,
                },
            },
        }
    }

    /// Ensures the plugin LanceDB space and binding exist exactly once per process.
    /// 确保插件 LanceDB 空间与绑定在每个进程中只初始化一次。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the host-derived isolated data root.
    /// `plugin_id` 选择由宿主派生的隔离数据根。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after the binding is ready.
    /// 绑定就绪后返回空值。
    async fn ensure_lancedb(&self, plugin_id: &str) -> Result<(), String> {
        if self
            .initialized_lancedb_plugins
            .lock()
            .map_err(|_| "Plugin LanceDB initialization lock is poisoned.".to_string())?
            .contains(plugin_id)
        {
            return Ok(());
        }
        // pluginDataRoot is the only physical root this plugin's vector storage can reach.
        // pluginDataRoot 是当前插件向量存储可触达的唯一物理根目录。
        let plugin_data_root = plugin_data_root(&self.plugin_state_root, plugin_id);
        // lancedbRoot is host-derived and cannot be influenced by the plugin payload.
        // lancedbRoot 由宿主派生，插件载荷无法影响。
        let lancedb_root = plugin_data_root.join("lancedb");
        fs::create_dir_all(&lancedb_root).map_err(|error| {
            format!(
                "Failed to create plugin LanceDB directory `{}`: {error}",
                lancedb_root.display()
            )
        })?;
        self.initialized_lancedb_plugins
            .lock()
            .map_err(|_| "Plugin LanceDB initialization lock is poisoned.".to_string())?
            .insert(plugin_id.to_string());
        Ok(())
    }

    /// Applies one idempotent plugin schema migration.
    /// 应用一次幂等插件 Schema 迁移。
    async fn sqlite_migrate(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // migration is strictly decoded so unknown protocol fields fail explicitly.
        // migration 使用严格解码，未知协议字段会显式失败。
        let migration = serde_json::from_value::<SqliteMigrationPayload>(payload)
            .map_err(|error| format!("Invalid plugin SQLite migration payload: {error}"))?;
        for statement in &migration.statements {
            validate_sqlite_statement(statement, SqliteStatementAccess::Write)?;
        }
        // migrationTableSql is host-owned metadata and cannot be replaced by plugin SQL.
        // migrationTableSql 是宿主持有的元数据，不能被插件 SQL 替换。
        let migration_table_sql = "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL, plugin_version TEXT NOT NULL)";
        local_sql::execute_script_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            migration_table_sql,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("Failed to initialize plugin migration table: {error}"))?;
        // existingVersionQuery prevents reapplying an already committed migration.
        // existingVersionQuery 防止重复应用已提交迁移。
        let existing_version_query = local_sql::query_json_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            "SELECT version FROM schema_migrations WHERE version = ?1 LIMIT 1",
            vec![SqliteValue::Integer(migration.version)],
        )
        .await
        .map_err(|error| format!("Failed to inspect plugin migration version: {error}"))?;
        if existing_version_query.row_count > 0 {
            return Ok(json!({ "applied": false, "version": migration.version }));
        }
        local_sql::execute_scripts_transaction_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            migration.statements,
            "INSERT INTO schema_migrations(version, applied_at_ms, plugin_version) VALUES (?1, ?2, ?3)",
            vec![
                SqliteValue::Integer(migration.version),
                SqliteValue::Integer(migration.applied_at_ms),
                SqliteValue::Text(migration.plugin_version),
            ],
        )
        .await
        .map_err(|error| format!("Failed to apply plugin SQLite migration atomically: {error}"))?;
        Ok(json!({ "applied": true, "version": migration.version }))
    }

    /// Executes one parameterized SQLite write statement.
    /// 执行一条参数化 SQLite 写语句。
    async fn sqlite_execute(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded so file or binding fields cannot be smuggled through.
        // request 使用严格解码，不能夹带文件或绑定字段。
        let request = serde_json::from_value::<SqliteExecutePayload>(payload)
            .map_err(|error| format!("Invalid plugin SQLite execute payload: {error}"))?;
        validate_sqlite_statement(&request.sql, SqliteStatementAccess::Write)?;
        // params uses the explicit tagged scalar protocol.
        // params 使用显式标记的标量协议。
        let params = decode_sqlite_params(request.params)?;
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = local_sql::execute_script_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            request.sql,
            params,
        )
        .await
        .map_err(|error| format!("Plugin SQLite execute failed: {error}"))?;
        // The plugin-visible JSON is built here rather than derived from the engine's result type, because
        // the engine deliberately knows nothing about JSON: a host adapter is the only place allowed to
        // cross that boundary. The field names and types are the ones plugins already receive.
        // 插件可见的 JSON 在此构造，而不是从引擎结果类型派生，因为引擎刻意对 JSON 一无所知：宿主适配器是唯一
        // 允许跨越该边界的地方。字段名与类型正是插件已经收到的那些。
        Ok(json!({
            "success": result.success,
            "message": result.message,
            "rows_changed": result.rows_changed,
            "last_insert_rowid": result.last_insert_rowid,
        }))
    }

    /// Executes one homogeneous parameter batch through the in-process transaction API.
    /// 通过进程内事务 API 执行一组同构参数批次。
    async fn sqlite_execute_batch(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded so file or binding fields cannot be smuggled through.
        // request 使用严格解码，不能夹带文件或绑定字段。
        let request = serde_json::from_value::<SqliteExecuteBatchPayload>(payload)
            .map_err(|error| format!("Invalid plugin SQLite batch payload: {error}"))?;
        validate_sqlite_statement(&request.sql, SqliteStatementAccess::Write)?;
        // items preserves the declared parameter group ordering.
        // items 保留声明的参数组顺序。
        let items = request
            .items
            .into_iter()
            .map(decode_sqlite_params)
            .collect::<Result<Vec<_>, _>>()?;
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = local_sql::execute_batch_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            request.sql,
            items,
        )
        .await
        .map_err(|error| format!("Plugin SQLite batch execute failed: {error}"))?;
        // Same boundary rule as the single-statement result, plus the batch's executed-group count.
        // 与单语句结果相同的边界规则，另外加上批次实际执行的参数组数量。
        Ok(json!({
            "success": result.success,
            "message": result.message,
            "rows_changed": result.rows_changed,
            "last_insert_rowid": result.last_insert_rowid,
            "statements_executed": result.statements_executed,
        }))
    }

    /// Executes one bounded parameterized SQLite query.
    /// 执行一次有界参数化 SQLite 查询。
    async fn sqlite_query(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded so file or binding fields cannot be smuggled through.
        // request 使用严格解码，不能夹带文件或绑定字段。
        let request = serde_json::from_value::<SqliteQueryPayload>(payload)
            .map_err(|error| format!("Invalid plugin SQLite query payload: {error}"))?;
        validate_sqlite_statement(&request.sql, SqliteStatementAccess::Read)?;
        // params uses the explicit tagged scalar protocol.
        // params 使用显式标记的标量协议。
        let params = decode_sqlite_params(request.params)?;
        // result is bounded while reading, and a timeout interrupts the running statement.
        // result 在读取期间即受约束，超时会中断正在运行的语句。
        let result = local_sql::query_json_bounded_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            request.sql,
            params,
            QueryJsonLimits {
                max_rows: PLUGIN_SQLITE_MAX_QUERY_ROWS,
                max_bytes: PLUGIN_SQLITE_MAX_QUERY_BYTES,
            },
            PLUGIN_SQLITE_QUERY_TIMEOUT,
        )
        .await
        .map_err(|error| match error {
            BoundedQueryError::TimedOut => format!(
                "Plugin SQLite query exceeded the {} second timeout.",
                PLUGIN_SQLITE_QUERY_TIMEOUT.as_secs()
            ),
            BoundedQueryError::Failed(error) => format!("Plugin SQLite query failed: {error}"),
        })?;
        // rows is decoded once at the host boundary so Lua receives ordinary JSON-safe values.
        // rows 在宿主边界只解码一次，使 Lua 接收普通 JSON 安全值。
        let rows = serde_json::from_str::<Value>(&result.json_data)
            .map_err(|error| format!("SQLite engine returned invalid JSON rows: {error}"))?;
        Ok(json!({ "rows": rows, "row_count": result.row_count }))
    }

    /// Checkpoints the plugin WAL after all earlier serialized calls have completed.
    /// 在此前串行调用全部完成后检查点化插件 WAL。
    async fn sqlite_flush(&self, plugin_id: &str) -> Result<Value, String> {
        // result is returned for diagnostics while the plugin sees no physical path.
        // result 用于诊断，同时插件不会看到物理路径。
        let result = local_sql::query_json_async(
            &self.local,
            self.plugin_sqlite_binding(plugin_id),
            "PRAGMA wal_checkpoint(PASSIVE)",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("Plugin SQLite flush failed: {error}"))?;
        Ok(json!({ "flushed": true, "row_count": result.row_count }))
    }

    /// Creates one typed table in the isolated plugin LanceDB binding.
    /// 在隔离插件 LanceDB 绑定中创建一张类型化表。
    async fn lancedb_create_table(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains no database path.
        // request 使用严格解码且不包含数据库路径。
        let request = serde_json::from_value::<LanceDbCreateTablePayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB create-table payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        // columns maps the public JSON protocol to the typed in-process engine contract.
        // columns 将公开 JSON 协议映射到类型化进程内引擎契约。
        let columns = request
            .columns
            .into_iter()
            .map(decode_lancedb_column)
            .collect::<Result<Vec<_>, _>>()?;
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = self
            .vector_engine(plugin_id)
            .await?
            .create_table(LanceDbCreateTableInput {
                table_name: request.table_name,
                columns,
                overwrite_if_exists: request.overwrite_if_exists,
            })
            .await
            .map_err(|error| format!("Plugin LanceDB create table failed: {error}"))?;
        Ok(json!({ "message": result.message }))
    }

    /// Upserts JSON rows into one isolated plugin LanceDB table.
    /// 向一张隔离插件 LanceDB 表写入或更新 JSON 行。
    async fn lancedb_upsert(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains JSON rows instead of arbitrary binary formats.
        // request 使用严格解码并包含 JSON 行，而不是任意二进制格式。
        let request = serde_json::from_value::<LanceDbUpsertPayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB upsert payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        for key_column in &request.key_columns {
            validate_lancedb_identifier(key_column, "key_columns")?;
        }
        // data is encoded at the host boundary using the engine JSON-rows contract.
        // data 在宿主边界按引擎 JSON 行契约编码。
        let data = serde_json::to_vec(&request.rows)
            .map_err(|error| format!("Failed to encode LanceDB JSON rows: {error}"))?;
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = self
            .vector_engine(plugin_id)
            .await?
            .vector_upsert(LanceDbUpsertInput {
                table_name: request.table_name,
                input_format: LanceDbInputFormat::JsonRows,
                data,
                key_columns: request.key_columns,
            })
            .await
            .map_err(|error| format!("Plugin LanceDB upsert failed: {error}"))?;
        Ok(json!({
            "message": result.message,
            "version": result.version,
            "input_rows": result.input_rows,
            "inserted_rows": result.inserted_rows,
            "updated_rows": result.updated_rows,
            "deleted_rows": result.deleted_rows,
        }))
    }

    /// Searches one isolated plugin LanceDB table with a bounded result count.
    /// 以有界结果数量检索一张隔离插件 LanceDB 表。
    async fn lancedb_search(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains no database path.
        // request 使用严格解码且不包含数据库路径。
        let request = serde_json::from_value::<LanceDbSearchPayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB search payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        validate_lancedb_identifier(&request.vector_column, "vector_column")?;
        if request.limit == 0 || request.limit > PLUGIN_LANCEDB_MAX_SEARCH_LIMIT {
            return Err(format!(
                "Plugin LanceDB search limit must be between 1 and {}.",
                PLUGIN_LANCEDB_MAX_SEARCH_LIMIT
            ));
        }
        // result uses JSON rows so no opaque Arrow payload crosses into Lua.
        // result 使用 JSON 行，避免不透明 Arrow 载荷进入 Lua。
        let result = self
            .vector_engine(plugin_id)
            .await?
            .vector_search(LanceDbSearchInput {
                table_name: request.table_name,
                vector: request.vector,
                limit: request.limit,
                filter: request.filter,
                vector_column: request.vector_column,
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .map_err(|error| format!("Plugin LanceDB search failed: {error}"))?;
        // rows is decoded at the host boundary to keep the plugin protocol JSON-safe.
        // rows 在宿主边界解码，以保持插件协议 JSON 安全。
        let rows = serde_json::from_slice::<Value>(&result.data)
            .map_err(|error| format!("LanceDB returned invalid JSON rows: {error}"))?;
        Ok(
            json!({ "message": result.message, "format": result.format.as_str(), "row_count": result.rows, "rows": rows }),
        )
    }

    /// Deletes rows from one isolated plugin LanceDB table.
    /// 从一张隔离插件 LanceDB 表删除行。
    async fn lancedb_delete(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains no database path.
        // request 使用严格解码且不包含数据库路径。
        let request = serde_json::from_value::<LanceDbDeletePayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB delete payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        if request.condition.trim().is_empty() {
            return Err("Plugin LanceDB delete requires a non-empty condition.".to_string());
        }
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = self
            .vector_engine(plugin_id)
            .await?
            .delete(LanceDbDeleteInput {
                table_name: request.table_name,
                condition: request.condition,
            })
            .await
            .map_err(|error| format!("Plugin LanceDB delete failed: {error}"))?;
        Ok(json!({
            "message": result.message,
            "version": result.version,
            "deleted_rows": result.deleted_rows,
        }))
    }

    /// Drops one isolated plugin LanceDB table.
    /// 删除一张隔离插件 LanceDB 表。
    async fn lancedb_drop_table(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains no database path.
        // request 使用严格解码且不包含数据库路径。
        let request = serde_json::from_value::<LanceDbDropTablePayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB drop-table payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        // result is projected from the typed in-process engine response.
        // result 从类型化进程内引擎响应投影。
        let result = self
            .vector_engine(plugin_id)
            .await?
            .drop_table(LanceDbDropTableInput {
                table_name: request.table_name,
            })
            .await
            .map_err(|error| format!("Plugin LanceDB drop table failed: {error}"))?;
        Ok(json!({ "message": result.message }))
    }
}

impl VulcanPluginStorageService for PluginStorageService {
    /// Executes one authenticated request on the retained service runtime.
    /// 在保留的服务运行时上执行一条已认证请求。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains only authenticated plugin identity and plugin-owned operation data.
    /// `request` 仅包含已认证插件身份与插件持有的操作数据。
    ///
    /// # Returns
    /// 返回值
    /// Returns the JSON-safe result or an explicit storage diagnostic.
    /// 返回 JSON 安全结果或明确的存储诊断。
    fn execute(&self, request: VulcanPluginStorageRequest) -> Result<Value, String> {
        self.runtime.block_on(self.execute_async(request))
    }
}

/// Access class used by SQLite statement policy validation.
/// SQLite 语句策略校验使用的访问类别。
#[derive(Debug, Clone, Copy)]
enum SqliteStatementAccess {
    /// Read-only query operation.
    /// 只读查询操作。
    Read,
    /// Schema or data mutation operation.
    /// Schema 或数据变更操作。
    Write,
}

/// Strict payload for one schema migration.
/// 单次 Schema 迁移的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqliteMigrationPayload {
    /// Positive monotonic migration version.
    /// 正数单调迁移版本。
    version: i64,
    /// Plugin package version applying the migration.
    /// 应用迁移的插件包版本。
    plugin_version: String,
    /// Host timestamp recorded for the migration.
    /// 为迁移记录的宿主时间戳。
    applied_at_ms: i64,
    /// Ordered idempotent SQL scripts without parameters.
    /// 不带参数的有序幂等 SQL 脚本。
    statements: Vec<String>,
}

/// Strict payload for one SQLite statement.
/// 单条 SQLite 语句的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqliteExecutePayload {
    /// Parameterized SQL statement.
    /// 参数化 SQL 语句。
    sql: String,
    /// Ordered tagged scalar parameters.
    /// 有序标记标量参数。
    #[serde(default)]
    params: Vec<SqliteValuePayload>,
}

/// Strict payload for one homogeneous SQLite batch.
/// 单次同构 SQLite 批次的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqliteExecuteBatchPayload {
    /// Parameterized SQL statement reused for every item.
    /// 为每个条目复用的参数化 SQL 语句。
    sql: String,
    /// Ordered parameter groups executed in one in-process SQLite transaction.
    /// 在单个进程内 SQLite 事务中执行的有序参数组。
    items: Vec<Vec<SqliteValuePayload>>,
}

/// Strict payload for one SQLite query.
/// 单次 SQLite 查询的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqliteQueryPayload {
    /// Parameterized read-only SQL statement.
    /// 参数化只读 SQL 语句。
    sql: String,
    /// Ordered tagged scalar parameters.
    /// 有序标记标量参数。
    #[serde(default)]
    params: Vec<SqliteValuePayload>,
}

/// One explicit JSON representation of a SQLite scalar value.
/// SQLite 标量值的一种显式 JSON 表示。
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum SqliteValuePayload {
    /// Signed 64-bit integer value.
    /// 有符号 64 位整数值。
    Int64(i64),
    /// IEEE-754 64-bit floating-point value.
    /// IEEE-754 64 位浮点值。
    Float64(f64),
    /// UTF-8 string value.
    /// UTF-8 字符串值。
    String(String),
    /// Base64-encoded byte sequence.
    /// Base64 编码字节序列。
    Bytes(String),
    /// Boolean value.
    /// 布尔值。
    Bool(bool),
    /// Explicit SQLite NULL value.
    /// 显式 SQLite NULL 值。
    Null,
}

/// Strict payload for one LanceDB table column.
/// 单个 LanceDB 表列的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbColumnPayload {
    /// Stable column identifier.
    /// 稳定列标识。
    name: String,
    /// Stable snake-case column type.
    /// 稳定蛇形列类型。
    column_type: String,
    /// Vector dimension for vector columns.
    /// 向量列的向量维度。
    #[serde(default)]
    vector_dim: u32,
    /// Whether NULL values are accepted.
    /// 是否接受 NULL 值。
    #[serde(default)]
    nullable: bool,
}

/// Strict payload for LanceDB table creation.
/// LanceDB 建表的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbCreateTablePayload {
    /// Stable table identifier.
    /// 稳定表标识。
    table_name: String,
    /// Typed table columns.
    /// 类型化表列。
    columns: Vec<LanceDbColumnPayload>,
    /// Whether an existing table may be replaced.
    /// 是否允许替换现有表。
    #[serde(default)]
    overwrite_if_exists: bool,
}

/// Strict payload for LanceDB JSON-row upsert.
/// LanceDB JSON 行写入更新的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbUpsertPayload {
    /// Stable table identifier.
    /// 稳定表标识。
    table_name: String,
    /// JSON row array.
    /// JSON 行数组。
    rows: Vec<Value>,
    /// Stable key columns used for merge semantics.
    /// 合并语义使用的稳定键列。
    #[serde(default)]
    key_columns: Vec<String>,
}

/// Strict payload for one LanceDB search.
/// 单次 LanceDB 检索的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbSearchPayload {
    /// Stable table identifier.
    /// 稳定表标识。
    table_name: String,
    /// Query vector.
    /// 查询向量。
    vector: Vec<f32>,
    /// Bounded result count.
    /// 有界结果数量。
    limit: u32,
    /// LanceDB-supported predicate expression.
    /// LanceDB 支持的谓词表达式。
    #[serde(default)]
    filter: String,
    /// Stable vector column identifier.
    /// 稳定向量列标识。
    vector_column: String,
}

/// Strict payload for LanceDB row deletion.
/// LanceDB 行删除的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbDeletePayload {
    /// Stable table identifier.
    /// 稳定表标识。
    table_name: String,
    /// Required LanceDB-supported predicate expression.
    /// 必填的 LanceDB 支持谓词表达式。
    condition: String,
}

/// Strict payload for LanceDB table deletion.
/// LanceDB 表删除的严格载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanceDbDropTablePayload {
    /// Stable table identifier.
    /// 稳定表标识。
    table_name: String,
}

/// Converts tagged plugin parameters into the typed in-process SQLite contract.
/// 将标记插件参数转换为类型化进程内 SQLite 契约。
///
/// # Parameters
/// 参数
/// `params` is the ordered tagged scalar list.
/// `params` 是有序标记标量列表。
///
/// # Returns
/// 返回值
/// Returns typed SQLite values or an explicit Base64 diagnostic.
/// 返回类型化 SQLite 值或明确的 Base64 诊断。
pub(crate) fn decode_sqlite_params(
    params: Vec<SqliteValuePayload>,
) -> Result<Vec<SqliteValue>, String> {
    params
        .into_iter()
        .map(|value| match value {
            SqliteValuePayload::Int64(value) => Ok(SqliteValue::Integer(value)),
            SqliteValuePayload::Float64(value) => Ok(SqliteValue::Real(value)),
            SqliteValuePayload::String(value) => Ok(SqliteValue::Text(value)),
            SqliteValuePayload::Bytes(value) => BASE64_STANDARD
                .decode(value)
                .map(SqliteValue::Blob)
                .map_err(|error| format!("Invalid Base64 SQLite byte parameter: {error}")),
            SqliteValuePayload::Bool(value) => Ok(SqliteValue::Integer(i64::from(value))),
            SqliteValuePayload::Null => Ok(SqliteValue::Null),
        })
        .collect()
}

/// Converts one public LanceDB column payload into the typed engine contract.
/// 将一个公开 LanceDB 列载荷转换为类型化引擎契约。
///
/// # Parameters
/// 参数
/// `column` is the strict public column descriptor.
/// `column` 是严格公开列描述符。
///
/// # Returns
/// 返回值
/// Returns a validated typed column definition.
/// 返回一份已校验的类型化列定义。
fn decode_lancedb_column(column: LanceDbColumnPayload) -> Result<LanceDbColumnDef, String> {
    validate_lancedb_identifier(&column.name, "column.name")?;
    // columnType accepts only the exact public protocol keys.
    // columnType 仅接受公开协议中的精确键。
    let column_type = match column.column_type.as_str() {
        "string" => LanceDbColumnType::String,
        "int64" => LanceDbColumnType::Int64,
        "float64" => LanceDbColumnType::Float64,
        "bool" => LanceDbColumnType::Bool,
        "vector_float32" => LanceDbColumnType::VectorFloat32,
        "float32" => LanceDbColumnType::Float32,
        "uint64" => LanceDbColumnType::Uint64,
        "int32" => LanceDbColumnType::Int32,
        "uint32" => LanceDbColumnType::Uint32,
        unsupported => return Err(format!("Unsupported LanceDB column type `{unsupported}`.")),
    };
    if column_type == LanceDbColumnType::VectorFloat32 && column.vector_dim == 0 {
        return Err("LanceDB vector_float32 columns require positive vector_dim.".to_string());
    }
    Ok(LanceDbColumnDef {
        name: column.name,
        column_type,
        vector_dim: column.vector_dim,
        nullable: column.nullable,
    })
}

/// Validates one SQLite statement against plugin isolation policy.
/// 根据插件隔离策略校验一条 SQLite 语句。
///
/// # Parameters
/// 参数
/// `sql` is the exact statement text and `access` selects the allowed statement class.
/// `sql` 是精确语句文本，`access` 选择允许的语句类别。
///
/// # Returns
/// 返回值
/// Returns unit for a non-empty statement without isolation-breaking keywords.
/// 对不含破坏隔离关键字的非空语句返回空值。
fn validate_sqlite_statement(sql: &str, access: SqliteStatementAccess) -> Result<(), String> {
    // tokens retain statement separators while excluding quoted text and comments.
    // tokens 保留语句分隔符，同时排除引号文本与注释。
    let tokens = sqlite_policy_tokens(sql);
    if tokens.is_empty() || tokens.first().is_some_and(|token| token == ";") {
        return Err("Plugin SQLite statement must not be empty.".to_string());
    }
    // case_depth distinguishes expression END from transaction END, including nested CASE.
    // case_depth 区分表达式 END 与事务 END，并支持嵌套 CASE。
    let mut case_depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "CASE" => case_depth += 1,
            "END" if case_depth > 0 => case_depth -= 1,
            "END" => {
                return Err("Plugin SQLite statement cannot use transaction `END`.".to_string());
            }
            ";" if index + 1 != tokens.len() || case_depth != 0 => {
                return Err(
                    "Plugin SQLite operation requires exactly one complete statement.".to_string(),
                );
            }
            "ATTACH" | "DETACH" | "VACUUM" | "LOAD_EXTENSION" | "BEGIN" | "COMMIT" | "ROLLBACK"
            | "SAVEPOINT" | "RELEASE" => {
                return Err(format!("Plugin SQLite statement cannot use `{token}`."));
            }
            _ => {}
        }
    }
    if case_depth != 0 {
        return Err("Plugin SQLite CASE expression must end within its statement.".to_string());
    }
    if matches!(access, SqliteStatementAccess::Read)
        && !matches!(tokens.first().map(String::as_str), Some("SELECT" | "WITH"))
    {
        return Err("Plugin SQLite query must begin with SELECT or WITH.".to_string());
    }
    if matches!(access, SqliteStatementAccess::Read)
        && tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "CREATE" | "DROP" | "ALTER"
            )
        })
    {
        return Err("Plugin SQLite query cannot contain mutation statements.".to_string());
    }
    Ok(())
}

/// Tokenizes policy-relevant SQL identifiers while skipping strings, comments and bind names.
/// 在跳过字符串、注释与绑定参数名的同时提取策略相关 SQL 标识符。
///
/// # Parameters
/// 参数
/// `sql` is the exact statement text.
/// `sql` 是精确语句文本。
///
/// # Returns
/// 返回值
/// Returns uppercase identifier tokens outside quoted regions.
/// 返回引号区域之外的大写标识符 Token。
fn sqlite_policy_tokens(sql: &str) -> Vec<String> {
    // tokens accumulates completed identifiers.
    // tokens 累积已完成标识符。
    let mut tokens = Vec::new();
    // current accumulates the current ASCII identifier.
    // current 累积当前 ASCII 标识符。
    let mut current = String::new();
    // characters provides one-character lookahead for SQL comments.
    // characters 为 SQL 注释提供单字符前瞻。
    let mut characters = sql.chars().peekable();
    // quote tracks the active quoted region.
    // quote 跟踪当前活动引号区域。
    let mut quote: Option<char> = None;
    // lineComment tracks a double-hyphen comment.
    // lineComment 跟踪双连字符行注释。
    let mut line_comment = false;
    // blockComment tracks a slash-star comment.
    // blockComment 跟踪斜杠星号块注释。
    let mut block_comment = false;
    while let Some(character) = characters.next() {
        if line_comment {
            if character == '\n' {
                line_comment = false;
            }
            continue;
        }
        if block_comment {
            if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                block_comment = false;
            }
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                if characters.peek() == Some(&active_quote) {
                    characters.next();
                } else {
                    quote = None;
                }
            }
            continue;
        }
        if character == '-' && characters.peek() == Some(&'-') {
            characters.next();
            line_comment = true;
            push_sqlite_policy_token(&mut tokens, &mut current);
            continue;
        }
        if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            block_comment = true;
            push_sqlite_policy_token(&mut tokens, &mut current);
            continue;
        }
        if matches!(character, '\'' | '"' | '`' | '[') {
            push_sqlite_policy_token(&mut tokens, &mut current);
            quote = Some(if character == '[' { ']' } else { character });
            continue;
        }
        // SQLite treats named bind variables as one token, even when their name is CASE or END.
        // SQLite 将命名绑定变量视为一个 Token，即使变量名为 CASE 或 END。
        // A dollar sign inside an identifier remains part of that identifier, not a new parameter.
        // 标识符内部的美元符号仍属于该标识符，而不是新参数。
        if matches!(character, ':' | '@' | '#') || (character == '$' && current.is_empty()) {
            push_sqlite_policy_token(&mut tokens, &mut current);
            skip_sqlite_bind_name(&mut characters);
            continue;
        }
        if character.is_ascii_alphanumeric()
            || matches!(character, '_' | '$')
            || !character.is_ascii()
        {
            current.push(character.to_ascii_uppercase());
        } else {
            push_sqlite_policy_token(&mut tokens, &mut current);
            // Preserve separators so CASE cannot authorize a following transaction statement.
            // 保留分隔符，防止 CASE 授权后续事务语句。
            if character == ';' {
                tokens.push(";".to_string());
            }
        }
    }
    push_sqlite_policy_token(&mut tokens, &mut current);
    tokens
}

/// Consumes the name of one SQLite bind variable after its prefix has been read.
/// 在读取前缀后消费一个 SQLite 绑定变量名。
///
/// `characters` points immediately after a colon, at sign, hash or leading dollar sign.
/// `characters` 位于冒号、艾特符号、井号或前导美元符号之后。
/// Returns no value; advances only across SQLite identifier bytes, namespace pairs and a suffix.
/// 不返回值；仅越过 SQLite 标识符字符、命名空间冒号对与可选后缀。
fn skip_sqlite_bind_name(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    // has_identifier matches the bundled SQLite tokenizer's requirement for a parenthesized suffix.
    // has_identifier 对齐内置 SQLite 分词器对括号后缀的标识符要求。
    let mut has_identifier = false;
    while let Some(&character) = characters.peek() {
        if character.is_ascii_alphanumeric()
            || matches!(character, '_' | '$')
            || !character.is_ascii()
        {
            has_identifier = true;
            characters.next();
        } else if character == ':' && characters.clone().nth(1) == Some(':') {
            characters.next();
            characters.next();
        } else if character == '(' && has_identifier {
            characters.next();
            // Tcl-style suffixes are a single bind name, not SQL; whitespace ends an invalid token.
            // Tcl 风格后缀属于同一个绑定名称，不是 SQL；空白会终止无效 Token。
            // This mirrors libsqlite3-sys 0.38.2 sqlite3GetToken; SQLite still validates the full SQL.
            // 此处对齐 libsqlite3-sys 0.38.2 的 sqlite3GetToken；完整 SQL 仍由 SQLite 校验。
            while let Some(&suffix_character) = characters.peek() {
                if suffix_character == '\0' || suffix_character.is_ascii_whitespace() {
                    break;
                }
                characters.next();
                if suffix_character == ')' {
                    break;
                }
            }
            break;
        } else {
            break;
        }
    }
}

/// Moves one completed SQL policy token into the output list.
/// 将一个已完成 SQL 策略 Token 移入输出列表。
///
/// # Parameters
/// 参数
/// `tokens` receives completed values and `current` is the mutable token buffer.
/// `tokens` 接收已完成值，`current` 是可变 Token 缓冲区。
///
/// # Returns
/// 返回值
/// Returns no value.
/// 不返回值。
fn push_sqlite_policy_token(tokens: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        tokens.push(std::mem::take(current));
    }
}

/// Validates one LanceDB identifier accepted by the plugin storage protocol.
/// 校验插件存储协议接受的一个 LanceDB 标识符。
///
/// # Parameters
/// 参数
/// `value` is the candidate identifier and `field` labels diagnostics.
/// `value` 是候选标识符，`field` 标记诊断。
///
/// # Returns
/// 返回值
/// Returns unit for one portable identifier.
/// 对可移植标识符返回空值。
fn validate_lancedb_identifier(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().enumerate().all(|(index, character)| {
            character.is_ascii_alphanumeric() || character == '_' || (character == '-' && index > 0)
        })
    {
        return Err(format!(
            "Plugin LanceDB field `{field}` must be a portable identifier of 1 to 128 bytes."
        ));
    }
    Ok(())
}

/// Validates the authenticated plugin identity before deriving filesystem paths.
/// 在派生文件系统路径前校验已认证插件身份。
///
/// # Parameters
/// 参数
/// `plugin_id` is the manifest-validated identity repeated at the storage boundary.
/// `plugin_id` 是在存储边界再次校验的清单身份。
///
/// # Returns
/// 返回值
/// Returns unit only for one portable single-segment identifier.
/// 仅对可移植单段标识符返回空值。
fn validate_plugin_storage_identity(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty()
        || plugin_id.len() > 128
        || !plugin_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Plugin storage identity is not a portable identifier.".to_string());
    }
    Ok(())
}

/// Returns the isolated data root for one authenticated plugin identity.
/// 返回一个已认证插件身份的隔离数据根。
fn plugin_data_root(plugin_state_root: &Path, plugin_id: &str) -> PathBuf {
    plugin_state_root.join(plugin_id).join("data")
}

#[cfg(test)]
mod tests {
    use super::{
        SqliteStatementAccess, sqlite_policy_tokens, validate_lancedb_identifier,
        validate_plugin_storage_identity, validate_sqlite_statement,
    };

    /// Verifies isolation-breaking SQLite statements are rejected outside quoted text.
    /// 验证破坏隔离的 SQLite 语句在引号文本之外会被拒绝。
    #[test]
    fn sqlite_policy_rejects_database_escape_statements() {
        assert!(
            validate_sqlite_statement(
                "ATTACH DATABASE 'outside.sqlite3' AS outside",
                SqliteStatementAccess::Write,
            )
            .is_err()
        );
        assert!(
            validate_sqlite_statement(
                "SELECT 'ATTACH DATABASE harmless' AS value",
                SqliteStatementAccess::Read,
            )
            .is_ok()
        );
        assert_eq!(
            sqlite_policy_tokens("SELECT 1 -- ATTACH DATABASE x\n"),
            vec!["SELECT", "1"]
        );
        assert!(
            validate_sqlite_statement("PRAGMA writable_schema = ON", SqliteStatementAccess::Read)
                .is_err()
        );
        assert!(
            validate_sqlite_statement("BEGIN IMMEDIATE", SqliteStatementAccess::Write).is_err()
        );
        assert!(validate_sqlite_statement("ROLLBACK", SqliteStatementAccess::Write).is_err());
    }

    /// Accepts nested CASE aggregates while preserving all statement and write boundaries.
    /// 接受嵌套 CASE 聚合，同时保留全部语句与写入边界。
    /// No parameters; assertions verify expression tokens cannot authorize transactions.
    /// 无参数；断言验证表达式 Token 不能授权事务。
    #[test]
    fn sqlite_policy_accepts_case_without_opening_transaction_boundaries() {
        for sql in [
            "SELECT CASE WHEN ?1 > 0 THEN CASE ?2 WHEN 1 THEN 2 ELSE 0 END ELSE 0 END;",
            "WITH rows(value) AS (SELECT ?1) SELECT SUM(CASE WHEN value = 1 THEN 1 ELSE 0 END) FROM rows",
            "SELECT 'CASE; END; COMMIT' AS label /* END; */; -- END",
            "SELECT \"END\", [CASE], `COMMIT` FROM \"quoted table\"",
        ] {
            assert!(
                validate_sqlite_statement(sql, SqliteStatementAccess::Read).is_ok(),
                "{sql}"
            );
        }
        for sql in [
            "END",
            "END TRANSACTION",
            "COMMIT",
            "SELECT CASE WHEN 1 THEN 1 END; END",
            "SELECT CASE WHEN 1 THEN 1; END",
            "SELECT CASE WHEN 1 THEN 1",
            "SELECT 1; SELECT 2",
            "SELECT 1;;",
            "WITH rows AS (SELECT 1) DELETE FROM usage",
            "SELECT (INSERT INTO usage VALUES (1))",
            "SELECT CASE WHEN 1 THEN load_extension('x') ELSE 0 END",
            "SELECT CASE WHEN 1 THEN 1 END; ATTACH 'x' AS escaped",
            "BEGIN; SELECT CASE WHEN 1 THEN 1 END; COMMIT",
            "SAVEPOINT nested",
            "RELEASE nested",
        ] {
            assert!(
                validate_sqlite_statement(sql, SqliteStatementAccess::Read).is_err(),
                "{sql}"
            );
        }
        for sql in [";", "END", "COMMIT", "BEGIN", "ROLLBACK", "SELECT 1; END"] {
            assert!(
                validate_sqlite_statement(sql, SqliteStatementAccess::Write).is_err(),
                "{sql}"
            );
        }
    }

    /// Keeps legal bind names out of policy keyword matching without hiding real SQL statements.
    /// 将合法绑定名称排除在策略关键字匹配外，同时保留真实 SQL 语句检查。
    /// No parameters; assertions cover all named prefixes, suffixes and adjacent transaction tokens.
    /// 无参数；断言覆盖命名前缀、后缀与相邻事务 Token。
    #[test]
    fn sqlite_policy_distinguishes_named_parameters_from_keywords() {
        for sql in [
            "SELECT :case AS value",
            "SELECT @case AS value",
            "SELECT $case AS value",
            "SELECT CASE WHEN :case > 0 THEN @end ELSE $rollback END AS value",
            "SELECT $scope::case(END;COMMIT) AS value",
            "SELECT :删除CASE AS value, price$case FROM data",
        ] {
            assert!(
                validate_sqlite_statement(sql, SqliteStatementAccess::Read).is_ok(),
                "{sql}"
            );
        }
        for sql in [
            "SELECT :case; END",
            "SELECT @end; COMMIT",
            "SELECT $scope::case(END); ATTACH 'x' AS escaped",
            "SELECT CASE WHEN :case > 0 THEN @end",
            "SELECT :value + load_extension('x')",
            "SELECT $value; DELETE FROM usage",
        ] {
            assert!(
                validate_sqlite_statement(sql, SqliteStatementAccess::Read).is_err(),
                "{sql}"
            );
        }
    }

    /// Verifies plugin and LanceDB identifiers cannot escape their host-derived namespace.
    /// 验证插件与 LanceDB 标识符不能逃逸宿主派生命名空间。
    #[test]
    fn storage_identifiers_are_portable_single_segments() {
        assert!(validate_plugin_storage_identity("vulcan-statistics").is_ok());
        assert!(validate_plugin_storage_identity("../outside").is_err());
        assert!(validate_lancedb_identifier("usage_vectors", "table").is_ok());
        assert!(validate_lancedb_identifier("usage/vectors", "table").is_err());
    }
}

#[cfg(test)]
#[path = "plugin_storage_runtime_tests.rs"]
mod runtime_tests;

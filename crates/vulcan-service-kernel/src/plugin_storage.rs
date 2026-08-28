//! Controller-backed isolated storage service for authorized Vulcan Plugins.
//! 面向已授权 Vulcan 插件的 Controller 后端隔离存储服务。

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::runtime::Handle;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use vldb_controller_client::{
    ControllerClient, ControllerLanceDbColumnDef, ControllerLanceDbColumnType,
    ControllerLanceDbEnableRequest, ControllerLanceDbInputFormat, ControllerLanceDbOutputFormat,
    ControllerSqliteEnableRequest, ControllerSqliteValue, SpaceKind, SpaceRegistration,
};
use vulcan_plugin_runtime::{
    VulcanPluginStorageOperation, VulcanPluginStorageRequest, VulcanPluginStorageService,
};

/// Maximum SQLite rows returned by one plugin query.
/// 单次插件 SQLite 查询允许返回的最大行数。
const PLUGIN_SQLITE_MAX_QUERY_ROWS: u64 = 5_000;

/// Maximum encoded JSON bytes returned by one plugin query.
/// 单次插件查询允许返回的最大编码 JSON 字节数。
const PLUGIN_SQLITE_MAX_QUERY_BYTES: usize = 4 * 1024 * 1024;

/// Maximum wall-clock time allowed for one plugin SQLite query.
/// 单次插件 SQLite 查询允许占用的最长墙钟时间。
const PLUGIN_SQLITE_QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum LanceDB search result count exposed to one plugin request.
/// 单次插件请求允许返回的最大 LanceDB 检索结果数。
const PLUGIN_LANCEDB_MAX_SEARCH_LIMIT: u32 = 5_000;

/// Maximum active and queued storage operations accepted for one plugin.
/// 单个插件允许接收的活动与排队存储操作上限。
const PLUGIN_STORAGE_MAX_PENDING_OPERATIONS: usize = 64;

/// Monotonic in-process sequence preventing same-millisecond binding collisions.
/// 防止同一毫秒内绑定冲突的进程内单调序列。
static PLUGIN_STORAGE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

/// Controller-backed storage adapter that derives every path and binding from plugin identity.
/// 根据插件身份派生全部路径与绑定的 Controller 后端存储适配器。
pub(crate) struct ControllerPluginStorageService {
    /// Shared Controller client bootstrapped by the application data plane.
    /// 由应用数据平面引导的共享 Controller 客户端。
    client: ControllerClient,
    /// Runtime handle used to bridge synchronous Lua host callbacks into async Controller calls.
    /// 用于把同步 Lua 宿主回调桥接到异步 Controller 调用的运行时句柄。
    runtime: Handle,
    /// User-owned plugin state root.
    /// 用户持有的插件状态根目录。
    plugin_state_root: PathBuf,
    /// Bootstrap-scoped suffix ensuring Controller bindings belong only to this client.
    /// 确保 Controller 绑定仅属于当前客户端的引导作用域后缀。
    binding_scope: String,
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

impl ControllerPluginStorageService {
    /// Creates one adapter over the already-connected application Controller client.
    /// 基于已连接的应用 Controller 客户端创建一个适配器。
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
        client: ControllerClient,
        runtime: Handle,
        plugin_state_root: PathBuf,
    ) -> Self {
        Self {
            client,
            runtime,
            plugin_state_root,
            binding_scope: build_plugin_storage_binding_scope(),
            initialized_sqlite_plugins: Mutex::new(BTreeSet::new()),
            initialized_lancedb_plugins: Mutex::new(BTreeSet::new()),
            operation_lanes: Mutex::new(BTreeMap::new()),
        }
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

    /// Executes one request on the async Controller client after lazy backend initialization.
    /// 在按需初始化后端后，通过异步 Controller 客户端执行一条请求。
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
        // serialGuard remains held through lazy initialization and the complete Controller operation.
        // serialGuard 在按需初始化与完整 Controller 操作期间保持持有。
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
        // pluginDataRoot is the only physical root exposed to the Controller for this plugin.
        // pluginDataRoot 是为当前插件暴露给 Controller 的唯一物理根目录。
        let plugin_data_root = plugin_data_root(&self.plugin_state_root, plugin_id);
        // sqlitePath is host-derived and cannot be influenced by the plugin payload.
        // sqlitePath 由宿主派生，插件载荷无法影响。
        let sqlite_path = plugin_data_root.join("plugin.sqlite3");
        fs::create_dir_all(&plugin_data_root).map_err(|error| {
            format!(
                "Failed to create plugin data directory `{}`: {error}",
                plugin_data_root.display()
            )
        })?;
        self.attach_plugin_space(plugin_id, &plugin_data_root)
            .await?;
        self.client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: plugin_space_id(plugin_id),
                binding_id: plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                db_path: vulcan_runtime_assets::native_host_path_text_from_path(&sqlite_path),
                connection_pool_size: 2,
                busy_timeout_ms: 10_000,
                journal_mode: "WAL".to_string(),
                synchronous: "NORMAL".to_string(),
                foreign_keys: true,
                temp_store: "MEMORY".to_string(),
                wal_autocheckpoint_pages: 1_000,
                cache_size_kib: 16_384,
                mmap_size_bytes: 64 * 1024 * 1024,
                enforce_db_file_lock: true,
                read_only: false,
                allow_uri_filenames: false,
                trusted_schema: false,
                defensive: true,
            })
            .await
            .map_err(|error| format!("Failed to enable plugin SQLite binding: {error}"))?;
        self.initialized_sqlite_plugins
            .lock()
            .map_err(|_| "Plugin SQLite initialization lock is poisoned.".to_string())?
            .insert(plugin_id.to_string());
        Ok(())
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
        // pluginDataRoot is the only physical root exposed to the Controller for this plugin.
        // pluginDataRoot 是为当前插件暴露给 Controller 的唯一物理根目录。
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
        self.attach_plugin_space(plugin_id, &plugin_data_root)
            .await?;
        self.client
            .enable_lancedb(ControllerLanceDbEnableRequest {
                space_id: plugin_space_id(plugin_id),
                binding_id: plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                default_db_path: vulcan_runtime_assets::native_host_path_text_from_path(
                    &lancedb_root,
                ),
                db_root: Some(vulcan_runtime_assets::native_host_path_text_from_path(
                    &lancedb_root,
                )),
                read_consistency_interval_ms: Some(250),
                max_upsert_payload: 16 * 1024 * 1024,
                max_search_limit: PLUGIN_LANCEDB_MAX_SEARCH_LIMIT as usize,
                max_concurrent_requests: 32,
            })
            .await
            .map_err(|error| format!("Failed to enable plugin LanceDB binding: {error}"))?;
        self.initialized_lancedb_plugins
            .lock()
            .map_err(|_| "Plugin LanceDB initialization lock is poisoned.".to_string())?
            .insert(plugin_id.to_string());
        Ok(())
    }

    /// Attaches one host-derived plugin data space to the shared Controller client.
    /// 将一个宿主派生的插件数据空间附着到共享 Controller 客户端。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the stable space and `plugin_data_root` is its physical root.
    /// `plugin_id` 选择稳定空间，`plugin_data_root` 是其物理根。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after attachment.
    /// 附着完成后返回空值。
    async fn attach_plugin_space(
        &self,
        plugin_id: &str,
        plugin_data_root: &Path,
    ) -> Result<(), String> {
        self.client
            .attach_space(SpaceRegistration {
                space_id: plugin_space_id(plugin_id),
                space_label: format!("PLUGIN {plugin_id}"),
                space_kind: SpaceKind::User,
                space_root: vulcan_runtime_assets::native_host_path_text_from_path(
                    plugin_data_root,
                ),
            })
            .await
            .map_err(|error| format!("Failed to attach plugin data space: {error}"))?;
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
        self.client
            .execute_sqlite_script_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                migration_table_sql,
                Vec::new(),
            )
            .await
            .map_err(|error| format!("Failed to initialize plugin migration table: {error}"))?;
        // existingVersionQuery prevents reapplying an already committed migration.
        // existingVersionQuery 防止重复应用已提交迁移。
        let existing_version_query = self
            .client
            .query_sqlite_json_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                "SELECT version FROM schema_migrations WHERE version = ?1 LIMIT 1",
                vec![ControllerSqliteValue::Int64(migration.version)],
            )
            .await
            .map_err(|error| format!("Failed to inspect plugin migration version: {error}"))?;
        if existing_version_query.row_count > 0 {
            return Ok(json!({ "applied": false, "version": migration.version }));
        }
        for statement in migration.statements {
            self.client
                .execute_sqlite_script_typed(
                    plugin_space_id(plugin_id),
                    plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                    statement,
                    Vec::new(),
                )
                .await
                .map_err(|error| format!("Failed to apply plugin SQLite migration: {error}"))?;
        }
        self.client
            .execute_sqlite_script_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                "INSERT INTO schema_migrations(version, applied_at_ms, plugin_version) VALUES (?1, ?2, ?3)",
                vec![
                    ControllerSqliteValue::Int64(migration.version),
                    ControllerSqliteValue::Int64(migration.applied_at_ms),
                    ControllerSqliteValue::String(migration.plugin_version),
                ],
            )
            .await
            .map_err(|error| format!("Failed to record plugin SQLite migration: {error}"))?;
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
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .execute_sqlite_script_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                request.sql,
                params,
            )
            .await
            .map_err(|error| format!("Plugin SQLite execute failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode plugin SQLite execute result: {error}"))
    }

    /// Executes one homogeneous parameter batch through the Controller transaction API.
    /// 通过 Controller 事务 API 执行一组同构参数批次。
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
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .execute_sqlite_batch_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                request.sql,
                items,
            )
            .await
            .map_err(|error| format!("Plugin SQLite batch execute failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode plugin SQLite batch result: {error}"))
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
        // result carries Controller-reported row count and encoded JSON data.
        // result 携带 Controller 报告的行数与编码 JSON 数据。
        let result = tokio::time::timeout(
            PLUGIN_SQLITE_QUERY_TIMEOUT,
            self.client.query_sqlite_json_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
                request.sql,
                params,
            ),
        )
        .await
        .map_err(|_| {
            format!(
                "Plugin SQLite query exceeded the {} second timeout.",
                PLUGIN_SQLITE_QUERY_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| format!("Plugin SQLite query failed: {error}"))?;
        if result.row_count > PLUGIN_SQLITE_MAX_QUERY_ROWS {
            return Err(format!(
                "Plugin SQLite query returned {} rows; maximum is {}.",
                result.row_count, PLUGIN_SQLITE_MAX_QUERY_ROWS
            ));
        }
        if result.json_data.len() > PLUGIN_SQLITE_MAX_QUERY_BYTES {
            return Err(format!(
                "Plugin SQLite query returned {} bytes; maximum is {}.",
                result.json_data.len(),
                PLUGIN_SQLITE_MAX_QUERY_BYTES
            ));
        }
        // rows is decoded once at the host boundary so Lua receives ordinary JSON-safe values.
        // rows 在宿主边界只解码一次，使 Lua 接收普通 JSON 安全值。
        let rows = serde_json::from_str::<Value>(&result.json_data)
            .map_err(|error| format!("Controller returned invalid SQLite JSON rows: {error}"))?;
        Ok(json!({ "rows": rows, "row_count": result.row_count }))
    }

    /// Checkpoints the plugin WAL after all earlier serialized calls have completed.
    /// 在此前串行调用全部完成后检查点化插件 WAL。
    async fn sqlite_flush(&self, plugin_id: &str) -> Result<Value, String> {
        // result is returned for diagnostics while the plugin sees no physical path.
        // result 用于诊断，同时插件不会看到物理路径。
        let result = self
            .client
            .query_sqlite_json_typed(
                plugin_space_id(plugin_id),
                plugin_sqlite_binding_id(plugin_id, &self.binding_scope),
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
        // columns maps the public JSON protocol to the typed Controller contract.
        // columns 将公开 JSON 协议映射到类型化 Controller 契约。
        let columns = request
            .columns
            .into_iter()
            .map(decode_lancedb_column)
            .collect::<Result<Vec<_>, _>>()?;
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .create_lancedb_table_typed(
                plugin_space_id(plugin_id),
                plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                request.table_name,
                columns,
                request.overwrite_if_exists,
            )
            .await
            .map_err(|error| format!("Plugin LanceDB create table failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode LanceDB create result: {error}"))
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
        // data is encoded at the host boundary using the Controller JSON-rows contract.
        // data 在宿主边界按 Controller JSON 行契约编码。
        let data = serde_json::to_vec(&request.rows)
            .map_err(|error| format!("Failed to encode LanceDB JSON rows: {error}"))?;
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .upsert_lancedb_typed(
                plugin_space_id(plugin_id),
                plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                request.table_name,
                ControllerLanceDbInputFormat::JsonRows,
                data,
                request.key_columns,
            )
            .await
            .map_err(|error| format!("Plugin LanceDB upsert failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode LanceDB upsert result: {error}"))
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
            .client
            .search_lancedb_typed(
                plugin_space_id(plugin_id),
                plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                request.table_name,
                request.vector,
                request.limit,
                request.filter,
                request.vector_column,
                ControllerLanceDbOutputFormat::JsonRows,
            )
            .await
            .map_err(|error| format!("Plugin LanceDB search failed: {error}"))?;
        // rows is decoded at the host boundary to keep the plugin protocol JSON-safe.
        // rows 在宿主边界解码，以保持插件协议 JSON 安全。
        let rows = serde_json::from_slice::<Value>(&result.data)
            .map_err(|error| format!("Controller returned invalid LanceDB JSON rows: {error}"))?;
        Ok(
            json!({ "message": result.message, "format": result.format, "row_count": result.rows, "rows": rows }),
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
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .delete_lancedb_typed(
                plugin_space_id(plugin_id),
                plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                request.table_name,
                request.condition,
            )
            .await
            .map_err(|error| format!("Plugin LanceDB delete failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode LanceDB delete result: {error}"))
    }

    /// Drops one isolated plugin LanceDB table.
    /// 删除一张隔离插件 LanceDB 表。
    async fn lancedb_drop_table(&self, plugin_id: &str, payload: Value) -> Result<Value, String> {
        // request is strictly decoded and contains no database path.
        // request 使用严格解码且不包含数据库路径。
        let request = serde_json::from_value::<LanceDbDropTablePayload>(payload)
            .map_err(|error| format!("Invalid plugin LanceDB drop-table payload: {error}"))?;
        validate_lancedb_identifier(&request.table_name, "table_name")?;
        // result is serialized directly from the typed Controller response.
        // result 直接从类型化 Controller 响应序列化。
        let result = self
            .client
            .drop_lancedb_table(
                plugin_space_id(plugin_id),
                plugin_lancedb_binding_id(plugin_id, &self.binding_scope),
                request.table_name,
            )
            .await
            .map_err(|error| format!("Plugin LanceDB drop table failed: {error}"))?;
        serde_json::to_value(result)
            .map_err(|error| format!("Failed to encode LanceDB drop result: {error}"))
    }
}

impl VulcanPluginStorageService for ControllerPluginStorageService {
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
    /// Ordered parameter groups executed in one Controller transaction.
    /// 在单个 Controller 事务中执行的有序参数组。
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
    /// Controller-supported predicate expression.
    /// Controller 支持的谓词表达式。
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
    /// Required Controller-supported predicate expression.
    /// 必填的 Controller 支持谓词表达式。
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

/// Converts tagged plugin parameters into the typed Controller SQLite contract.
/// 将标记插件参数转换为类型化 Controller SQLite 契约。
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
) -> Result<Vec<ControllerSqliteValue>, String> {
    params
        .into_iter()
        .map(|value| match value {
            SqliteValuePayload::Int64(value) => Ok(ControllerSqliteValue::Int64(value)),
            SqliteValuePayload::Float64(value) => Ok(ControllerSqliteValue::Float64(value)),
            SqliteValuePayload::String(value) => Ok(ControllerSqliteValue::String(value)),
            SqliteValuePayload::Bytes(value) => BASE64_STANDARD
                .decode(value)
                .map(ControllerSqliteValue::Bytes)
                .map_err(|error| format!("Invalid Base64 SQLite byte parameter: {error}")),
            SqliteValuePayload::Bool(value) => Ok(ControllerSqliteValue::Bool(value)),
            SqliteValuePayload::Null => Ok(ControllerSqliteValue::Null),
        })
        .collect()
}

/// Converts one public LanceDB column payload into the typed Controller contract.
/// 将一个公开 LanceDB 列载荷转换为类型化 Controller 契约。
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
fn decode_lancedb_column(
    column: LanceDbColumnPayload,
) -> Result<ControllerLanceDbColumnDef, String> {
    validate_lancedb_identifier(&column.name, "column.name")?;
    // columnType accepts only the exact public protocol keys.
    // columnType 仅接受公开协议中的精确键。
    let column_type = match column.column_type.as_str() {
        "string" => ControllerLanceDbColumnType::String,
        "int64" => ControllerLanceDbColumnType::Int64,
        "float64" => ControllerLanceDbColumnType::Float64,
        "bool" => ControllerLanceDbColumnType::Bool,
        "vector_float32" => ControllerLanceDbColumnType::VectorFloat32,
        "float32" => ControllerLanceDbColumnType::Float32,
        "uint64" => ControllerLanceDbColumnType::Uint64,
        "int32" => ControllerLanceDbColumnType::Int32,
        "uint32" => ControllerLanceDbColumnType::Uint32,
        unsupported => return Err(format!("Unsupported LanceDB column type `{unsupported}`.")),
    };
    if column_type == ControllerLanceDbColumnType::VectorFloat32 && column.vector_dim == 0 {
        return Err("LanceDB vector_float32 columns require positive vector_dim.".to_string());
    }
    Ok(ControllerLanceDbColumnDef {
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
    // tokens contains SQL identifiers outside quoted strings and comments.
    // tokens 包含引号字符串与注释之外的 SQL 标识符。
    let tokens = sqlite_policy_tokens(sql);
    if tokens.is_empty() {
        return Err("Plugin SQLite statement must not be empty.".to_string());
    }
    for forbidden in ["ATTACH", "DETACH", "VACUUM", "LOAD_EXTENSION"] {
        if tokens.iter().any(|token| token == forbidden) {
            return Err(format!("Plugin SQLite statement cannot use `{forbidden}`."));
        }
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

/// Tokenizes policy-relevant SQL identifiers while skipping strings and comments.
/// 在跳过字符串与注释的同时提取策略相关 SQL 标识符。
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
        if character.is_ascii_alphanumeric() || character == '_' {
            current.push(character.to_ascii_uppercase());
        } else {
            push_sqlite_policy_token(&mut tokens, &mut current);
        }
    }
    push_sqlite_policy_token(&mut tokens, &mut current);
    tokens
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

/// Returns the stable Controller space identifier for one plugin.
/// 返回单个插件的稳定 Controller 空间标识。
fn plugin_space_id(plugin_id: &str) -> String {
    format!("plugin-{plugin_id}")
}

/// Builds one bootstrap-scoped Controller binding suffix.
/// 构建一个引导作用域内唯一的 Controller 绑定后缀。
///
/// # Returns
/// 返回值
/// Returns a process, timestamp, and sequence tuple matching Controller client ownership lifetime.
/// 返回与 Controller 客户端所有权生命周期一致的进程、时间戳与序列元组。
fn build_plugin_storage_binding_scope() -> String {
    // epochMillis is only an identity component and never a persisted business timestamp.
    // epochMillis 仅作为身份组成部分，绝不是持久化业务时间戳。
    let epoch_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // sequence closes the collision window for multiple adapters created in one millisecond.
    // sequence 关闭同一毫秒内创建多个适配器时的冲突窗口。
    let sequence = PLUGIN_STORAGE_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{epoch_millis}-{sequence}", std::process::id())
}

/// Returns the bootstrap-scoped Controller SQLite binding identifier for one plugin.
/// 返回单个插件的引导作用域 Controller SQLite 绑定标识。
fn plugin_sqlite_binding_id(plugin_id: &str, binding_scope: &str) -> String {
    format!("plugin-{plugin_id}-sqlite@{binding_scope}")
}

/// Returns the bootstrap-scoped Controller LanceDB binding identifier for one plugin.
/// 返回单个插件的引导作用域 Controller LanceDB 绑定标识。
fn plugin_lancedb_binding_id(plugin_id: &str, binding_scope: &str) -> String {
    format!("plugin-{plugin_id}-lancedb@{binding_scope}")
}

#[cfg(test)]
mod tests {
    use super::{
        SqliteStatementAccess, build_plugin_storage_binding_scope, plugin_lancedb_binding_id,
        plugin_sqlite_binding_id, sqlite_policy_tokens, validate_lancedb_identifier,
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

    /// Verifies every adapter bootstrap receives client-owned SQLite and LanceDB bindings.
    /// 验证每次适配器引导都会获得客户端自有的 SQLite 与 LanceDB 绑定。
    #[test]
    fn plugin_bindings_are_bootstrap_scoped() {
        // firstScope and secondScope model two adapters created by separate bootstrap attempts.
        // firstScope 与 secondScope 模拟由两次独立引导创建的适配器。
        let first_scope = build_plugin_storage_binding_scope();
        let second_scope = build_plugin_storage_binding_scope();
        assert_ne!(first_scope, second_scope);
        assert_ne!(
            plugin_sqlite_binding_id("vulcan-statistics", &first_scope),
            plugin_sqlite_binding_id("vulcan-statistics", &second_scope)
        );
        assert_ne!(
            plugin_lancedb_binding_id("vulcan-statistics", &first_scope),
            plugin_lancedb_binding_id("vulcan-statistics", &second_scope)
        );
    }
}

//! In-process application data-plane bootstrap helpers.
//! 进程内应用数据平面引导辅助模块。

use crate::knowledge_base::KnowledgeBaseRuntimeSharedState;
use crate::local_database_runtime::{LocalDatabaseRuntime, LogicalDatabase};
#[cfg(not(test))]
use crate::runtime_assets;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable application-level logical space identifier.
/// 稳定的应用级逻辑空间标识符。
pub const APP_SPACE_ID: &str = "app-workspace";

/// Stable application-level SQLite binding tag.
/// 稳定的应用级 SQLite 绑定标签。
pub const APP_SQLITE_BINDING_TAG: &str = "app-state";

/// Stable logical space identifier for host-owned system statistics.
/// 宿主持有的系统统计稳定逻辑空间标识符。
pub const SYSTEM_STATISTICS_SPACE_ID: &str = "system-statistics";

/// Stable SQLite binding tag for host-owned system statistics.
/// 宿主持有的系统统计稳定 SQLite 绑定标签。
pub const SYSTEM_STATISTICS_SQLITE_BINDING_TAG: &str = "provider-usage";

/// Process-local sequence that guarantees each bootstrap gets one distinct SQLite binding suffix.
/// 进程内序号，确保每次引导都能拿到不同的 SQLite 绑定后缀。
static APP_SQLITE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Process-local sequence that gives every system-statistics bootstrap a distinct binding suffix.
/// 进程内序号，为每次系统统计引导提供不同的绑定后缀。
static SYSTEM_STATISTICS_SQLITE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Pre-release application schema version, including atomic OpenDesign credential pairs.
/// 预发布应用结构版本，包含原子 OpenDesign 凭据对。
/// Local Debug structure changes use the standalone maintenance tool while this version stays at v1.
/// 此版本保持 v1，本地 Debug 结构变化通过独立维护工具同步。
pub const APP_SCHEMA_VERSION: i64 = 1;

/// Credential table shared by initialization and every persistence operation.
/// 初始化与全部持久化操作共用的凭据表。
pub const AI_CREDENTIAL_SECRETS_TABLE: &str = "ai_credential_secrets";

/// Long-lived host runtime handle kept by the host runtime after startup succeeds.
/// 启动成功后由宿主运行时持有的长生命周期宿主运行时句柄。
#[derive(Clone)]
pub struct HostRuntimeHandle {
    /// In-process database runtime that replaces the controller's process, endpoint and client.
    /// 取代控制器进程、端点与客户端的进程内数据库运行时。
    pub local: Arc<LocalDatabaseRuntime>,
    /// Local data root reserved for the application data plane.
    /// 为应用数据平面保留的本地数据根目录。
    pub data_root: PathBuf,
    /// SQLite file path used by the application state space.
    /// 应用状态空间使用的 SQLite 文件路径。
    pub sqlite_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier used by the current host runtime handle.
    /// 当前宿主运行时句柄使用的单次引导作用域 SQLite 绑定标识符。
    pub sqlite_binding_id: String,
    /// SQLite file path used by the independent system-statistics binding.
    /// 独立系统统计绑定使用的 SQLite 文件路径。
    pub system_statistics_sqlite_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier for the system-statistics database.
    /// 系统统计数据库使用的单次引导作用域 SQLite 绑定标识符。
    pub system_statistics_sqlite_binding_id: String,
    /// Process-local knowledge state shared by every runtime reconstructed from this handle.
    /// 由基于当前句柄重建的所有运行时共享的进程内知识库状态。
    pub(crate) knowledge_runtime_state: Arc<KnowledgeBaseRuntimeSharedState>,
}

/// Immutable location and binding metadata retained by one host runtime.
/// 单个宿主运行时保留的不可变位置与绑定元数据。
#[derive(Debug, Clone)]
pub struct HostRuntimeMetadata {
    /// Local data root reserved for the application data plane.
    /// 为应用数据平面保留的本地数据根目录。
    pub data_root: PathBuf,
    /// SQLite file path used by the application state space.
    /// 应用状态空间使用的 SQLite 文件路径。
    pub sqlite_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier for application state.
    /// 应用状态使用的单次引导作用域 SQLite 绑定标识符。
    pub sqlite_binding_id: String,
    /// SQLite file path used by the independent system-statistics binding.
    /// 独立系统统计绑定使用的 SQLite 文件路径。
    pub system_statistics_sqlite_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier for system statistics.
    /// 系统统计使用的单次引导作用域 SQLite 绑定标识符。
    pub system_statistics_sqlite_binding_id: String,
}

impl fmt::Debug for HostRuntimeHandle {
    /// Formats one host runtime handle without dumping the in-process runtime internals.
    /// 在不输出进程内运行时内部状态的前提下格式化宿主运行时句柄。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostRuntimeHandle")
            .field("data_root", &self.data_root)
            .field("sqlite_path", &self.sqlite_path)
            .field("sqlite_binding_id", &self.sqlite_binding_id)
            .field(
                "system_statistics_sqlite_path",
                &self.system_statistics_sqlite_path,
            )
            .field(
                "system_statistics_sqlite_binding_id",
                &self.system_statistics_sqlite_binding_id,
            )
            .finish()
    }
}

impl HostRuntimeHandle {
    /// Builds one long-lived host runtime handle and initializes its shared knowledge runtime state.
    /// 构建一个长生命周期宿主运行时句柄，并初始化其共享知识库运行时状态。
    ///
    /// `local` is the in-process database runtime that supersedes the controller's process,
    /// endpoint and client duties.
    ///
    /// `local` 是取代控制器进程、端点与客户端职责的进程内数据库运行时。
    pub fn new(local: Arc<LocalDatabaseRuntime>, metadata: HostRuntimeMetadata) -> Self {
        Self {
            local,
            data_root: metadata.data_root,
            sqlite_path: metadata.sqlite_path,
            sqlite_binding_id: metadata.sqlite_binding_id,
            system_statistics_sqlite_path: metadata.system_statistics_sqlite_path,
            system_statistics_sqlite_binding_id: metadata.system_statistics_sqlite_binding_id,
            knowledge_runtime_state: Arc::new(KnowledgeBaseRuntimeSharedState::default()),
        }
    }

    /// Fixture constructor that derives an in-process runtime from the metadata's own paths.
    /// 夹具构造函数：由元数据自身的路径派生进程内运行时。
    ///
    /// Production code must not use this: the bootstrap entry point is the single authoritative
    /// place allowed to derive storage paths, and it opens the runtime once for the whole process.
    /// This exists so the numerous storage fixtures that only need a handle carrying paths do not
    /// each have to rebuild a binding set.
    ///
    /// 生产代码不得使用本函数：引导入口是唯一被允许派生存储路径的权威位置，且它为整个进程只打开一次
    /// 运行时。本函数的存在只是为了让众多仅需一个携带路径的句柄的存储夹具不必各自重建绑定集合。
    ///
    /// Every call opens its own runtime, deliberately: a downstream fixture that isolates itself with
    /// a unique temporary data root must not collide with another fixture in the same test process.
    ///
    /// 每次调用都打开自己的运行时，这是刻意的：用唯一临时数据根隔离自身的下游夹具不得与同进程内另一个
    /// 夹具冲突。
    #[cfg(any(test, feature = "test-support"))]
    pub fn new_for_test(metadata: HostRuntimeMetadata) -> Self {
        let mut bindings = std::collections::HashMap::new();
        bindings.insert(LogicalDatabase::AppState, metadata.sqlite_path.clone());
        bindings.insert(
            LogicalDatabase::SystemStatistics,
            metadata.system_statistics_sqlite_path.clone(),
        );

        let local = LocalDatabaseRuntime::open(bindings)
            .expect("fixture local database runtime should open");

        Self::new(Arc::new(local), metadata)
    }
}

/// Summary returned after one application data plane bootstrap completes.
/// 单次应用数据平面引导完成后返回的摘要信息。
#[derive(Debug, Clone)]
pub struct DataPlaneBootstrapReport {
    /// Long-lived host runtime handle retained by the application.
    /// 应用保留的长生命周期宿主运行时句柄。
    pub handle: HostRuntimeHandle,
}

impl DataPlaneBootstrapReport {
    /// Builds one concise human-readable summary row for status bars and logs.
    /// 为状态栏与日志构建一条简洁的人类可读摘要。
    pub fn summary_text(&self) -> String {
        format!(
            "vldb data plane ready: SQLite {}",
            self.handle.sqlite_path.display()
        )
    }
}

/// Bootstraps the in-process application data plane and the primary SQLite schema.
/// 引导进程内应用数据平面以及主 SQLite 结构。
pub async fn bootstrap_application_data_plane() -> Result<DataPlaneBootstrapReport, String> {
    let data_root = application_data_root();
    // sqlite_path shares the authoritative derivation with the independent Debug maintenance command.
    // sqlite_path 与独立 Debug 维护命令共用权威派生入口。
    let sqlite_path = application_sqlite_path();
    let system_statistics_root = data_root.join("statistics");
    let system_statistics_sqlite_path = system_statistics_root.join("provider-usage.sqlite3");

    ensure_parent_directory(&sqlite_path)?;
    ensure_parent_directory(&system_statistics_sqlite_path)?;

    // The authoritative path derivation stays exactly here, in the single host entry point. The
    // bindings below hand the already-derived paths to the engines, which never derive a storage
    // location themselves.
    // 权威路径派生就留在此处，即唯一宿主入口。下方绑定把已派生的路径交给引擎，引擎自身绝不派生存储位置。
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(LogicalDatabase::AppState, sqlite_path.clone());
    bindings.insert(
        LogicalDatabase::SystemStatistics,
        system_statistics_sqlite_path.clone(),
    );
    let local = Arc::new(
        LocalDatabaseRuntime::open(bindings)
            .map_err(|error| format!("Failed to open the local database runtime: {error}"))?,
    );

    let bootstrap_result = async {
        let sqlite_binding_id = build_application_sqlite_binding_id();
        let system_statistics_sqlite_binding_id = build_system_statistics_sqlite_binding_id();

        // The binding identifiers stay in the runtime metadata because plugin and VCE LanceDB paths use
        // them to address the host's own in-process databases.
        // 绑定标识符保留在运行期元数据中，因为插件与 VCE 的 LanceDB 路径用它们寻址宿主自身的进程内数据库。
        // Initialization checks the declared version; existing Debug structures are synchronized separately.
        // 初始化检查声明版本；已有 Debug 结构由独立入口同步。
        {
            let connection = local
                .connect(LogicalDatabase::AppState)
                .map_err(|error| format!("Failed to open the application database: {error}"))?;
            validate_application_schema_for_initialization(&connection)?;
            connection
                .execute_batch(&build_application_schema_script())
                .map_err(|error| {
                    format!("Failed to initialize the application data schema: {error}")
                })?;
        }
        local
            .verify_app_schema_version()
            .map_err(|error| format!("Application data schema version check failed: {error}"))?;

        let runtime_handle = HostRuntimeHandle::new(
            Arc::clone(&local),
            HostRuntimeMetadata {
                data_root: data_root.clone(),
                sqlite_path: sqlite_path.clone(),
                sqlite_binding_id,
                system_statistics_sqlite_path: system_statistics_sqlite_path.clone(),
                system_statistics_sqlite_binding_id,
            },
        );
        // The v1 provider-usage schema is created here and nowhere else. There is deliberately no call
        // that looks for usage facts in an older application database, copies them across or marks a
        // cutover: pre-v1 storage is not scanned, not identified and not migrated at runtime.
        // v1 供应商用量结构只在此处创建，别无他处。此处刻意不存在任何调用去在旧应用数据库中查找用量事实、把它们
        // 复制过来或标记切换：v1 之前的存储在运行时既不被扫描、也不被识别、更不被迁移。
        crate::workspace_store::ensure_provider_usage_schema(&runtime_handle).await?;

        Ok::<_, String>(runtime_handle)
    }
    .await;

    bootstrap_result.map(|handle| DataPlaneBootstrapReport { handle })
}

/// Prepare the production application schema on an already open caller-owned runtime handle.
/// 在已经打开且由调用方持有的运行时句柄上准备生产应用数据结构。
/// The handle supplies the exact application database. Returns initialization errors without creating a
/// runtime or resolving the user's application directory; available only to explicitly enabled test
/// fixtures.
/// 句柄提供精确的应用数据库。返回初始化错误，不创建运行时或解析用户应用目录；仅供明确启用的测试夹具使用。
#[cfg(feature = "test-support")]
pub async fn prepare_application_schema_for_test(
    host_runtime: &HostRuntimeHandle,
) -> Result<(), String> {
    // The path is fully qualified because this function only exists under the `test-support`
    // feature, so importing `local_sql` at module scope would leave it unused in every other build.
    // 使用全限定路径是因为本函数仅在 `test-support` 特性下存在，在模块作用域导入 `local_sql` 会让它在
    // 其他所有构建中无人使用。
    crate::local_sql::execute_script_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        &build_application_schema_script(),
        vec![],
    )
    .await
    .map_err(|error| format!("Initialize private application schema: {error}"))?;
    Ok(())
}

/// Checks the existing schema version before initialization can overwrite its version marker.
/// 在初始化覆盖版本标记之前检查既有结构版本。
///
/// `connection` is the application database; accepts new or current-version databases without migrating DDL.
/// `connection` 为应用数据库；接受新库或当前版本数据库，不迁移 DDL。
fn validate_application_schema_for_initialization(
    connection: &rusqlite::Connection,
) -> Result<(), String> {
    // observed is read before CREATE IF NOT EXISTS, which cannot update old CHECK constraints.
    // observed 在 CREATE IF NOT EXISTS 之前读取；后者无法更新旧 CHECK 约束。
    let observed: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| String::from("Failed to inspect the application schema version."))?;
    if observed != 0 && observed != APP_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported application schema version {observed}; expected {APP_SCHEMA_VERSION}."
        ));
    }
    Ok(())
}

/// Builds one bootstrap-scoped application SQLite binding identifier while preserving the stable app-level tag for diagnostics.
/// 在保留稳定应用级标签用于诊断的同时，构建单次引导作用域的应用 SQLite 绑定标识符。
fn build_application_sqlite_binding_id() -> String {
    let bootstrap_scope_ms = current_unix_epoch_millis();
    let bootstrap_sequence = APP_SQLITE_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}@{}-{}-{}",
        APP_SQLITE_BINDING_TAG,
        std::process::id(),
        bootstrap_scope_ms,
        bootstrap_sequence
    )
}

/// Builds one process-scoped system-statistics SQLite binding identifier.
/// 构建一个进程作用域的系统统计 SQLite 绑定标识符。
///
/// # Returns
/// 返回值
/// Returns a binding identifier unique to the current bootstrap.
/// 返回当前引导过程唯一的绑定标识符。
fn build_system_statistics_sqlite_binding_id() -> String {
    let bootstrap_scope_ms = current_unix_epoch_millis();
    let bootstrap_sequence =
        SYSTEM_STATISTICS_SQLITE_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}@{}-{}-{}",
        SYSTEM_STATISTICS_SQLITE_BINDING_TAG,
        std::process::id(),
        bootstrap_scope_ms,
        bootstrap_sequence
    )
}

/// Returns one best-effort Unix epoch timestamp in milliseconds for bootstrap-scoped binding naming.
/// 返回一个尽力获取的 Unix 纪元毫秒时间戳，用于单次引导作用域绑定命名。
fn current_unix_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

/// Returns the stable application data root used for host-owned state files.
/// 返回宿主持有状态文件使用的稳定应用数据根目录。
pub fn application_data_root() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir()
            .join("vulcan-code-tests")
            .join(std::process::id().to_string())
    }

    #[cfg(not(test))]
    {
        let home = vulcan_runtime_assets::user_home_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        runtime_assets::user_vulcan_code_root_for_home(&home)
    }
}

/// Derive the application SQLite path shared by bootstrap and explicit maintenance.
/// 派生引导与显式维护共用的应用 SQLite 路径。
/// No arguments; returns the path under the build-profile-specific application data root.
/// 无参数；返回当前构建配置对应的应用数据目录内路径。
pub fn application_sqlite_path() -> PathBuf {
    application_data_root()
        .join("sqlite")
        .join("app-workspace.sqlite3")
}

/// Resolves the fixed absolute host-owned root used by LuaSkills package configuration stores.
/// 解析 LuaSkills 技能包配置存储使用的固定绝对宿主持有根目录。
///
/// Returns an explicit error when the platform home directory is unavailable or not absolute.
/// 平台主目录不可用或不是绝对路径时返回显式错误。
pub fn application_luaskills_config_root() -> Result<PathBuf, String> {
    #[cfg(test)]
    // root uses the process-isolated absolute temporary data plane during unit tests.
    // root 在单元测试中使用进程隔离的绝对临时数据平面。
    let root = application_data_root();

    #[cfg(not(test))]
    // root requires the platform home directory and deliberately has no current-directory fallback.
    // root 要求平台主目录存在，并刻意不提供当前目录回退。
    let home = vulcan_runtime_assets::user_home_dir()
        .ok_or_else(|| String::from("Cannot resolve the LuaSkills package configuration root: the home directory is unavailable."))?;
    #[cfg(not(test))]
    let root = runtime_assets::user_vulcan_code_root_for_home(&home);

    if !root.is_absolute() {
        return Err(format!(
            "The LuaSkills package configuration root must be an absolute path: {}",
            root.display()
        ));
    }
    Ok(root.join("luaskills-config"))
}

/// Ensures the parent directory of one target file exists.
/// 确保某个目标文件的父目录已经存在。
fn ensure_parent_directory(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent).map_err(|error| {
        format!(
            "Failed to create the application data directory ({}): {error}",
            parent.display()
        )
    })
}

/// Builds the initial SQLite schema used by the Vulcan application data plane.
/// 构建 Vulcan 应用数据平面使用的初始 SQLite 结构。
/// No parameters; returns the authoritative DDL shared with the standalone Debug maintenance command.
/// 无参数；返回与独立 Debug 维护命令共用的权威 DDL。
pub fn build_application_schema_script() -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO app_metadata (key, value)
VALUES ('schema_version', '{schema_version}')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    icon_pack TEXT,
    icon_key TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_sort
ON projects(pinned DESC, sort_order ASC, updated_at DESC);

CREATE TABLE IF NOT EXISTS project_vce_settings (
    project_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    max_file_bytes INTEGER NOT NULL DEFAULT 1048576 CHECK(max_file_bytes > 0),
    extra_ignore_patterns_json TEXT NOT NULL DEFAULT '[]',
    allow_query_rewrite INTEGER NOT NULL DEFAULT 0 CHECK(allow_query_rewrite IN (0, 1)),
    allow_rerank INTEGER NOT NULL DEFAULT 1 CHECK(allow_rerank IN (0, 1)),
    embedding_max_concurrency INTEGER NOT NULL DEFAULT 4 CHECK(embedding_max_concurrency > 0),
    query_max_concurrency INTEGER NOT NULL DEFAULT 3 CHECK(query_max_concurrency > 0),
    max_results INTEGER NOT NULL DEFAULT 10 CHECK(max_results > 0),
    max_context_chars INTEGER NOT NULL DEFAULT 32000 CHECK(max_context_chars > 0),
    update_mode TEXT NOT NULL CHECK(update_mode IN ('manual', 'automatic')),
    auto_update_min_interval_seconds INTEGER NOT NULL CHECK(auto_update_min_interval_seconds BETWEEN 60 AND 86400),
    updated_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS workspace_session_index (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    project_tag TEXT NOT NULL,
    title TEXT,
    title_source TEXT,
    archived INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_session_index_project
ON workspace_session_index(project_id, archived, sort_order ASC, updated_at DESC);

CREATE TABLE IF NOT EXISTS thread_goals (
    thread_id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    objective TEXT NOT NULL,
    status TEXT NOT NULL,
    token_budget INTEGER,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    uncached_input_tokens INTEGER NOT NULL DEFAULT 0,
    cached_input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    time_used_seconds INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_goals_status
ON thread_goals(status, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS {credential_table} (
    secret_reference TEXT PRIMARY KEY,
    format_version INTEGER NOT NULL CHECK(format_version > 0),
    revision INTEGER NOT NULL CHECK(revision > 0),
    material_kind TEXT NOT NULL CHECK(material_kind IN (
        'api_key',
        'static_bearer',
        'open_design_auth',
        'openvulcan_auth',
        'open_ai_codex_oauth',
        'claude_code_oauth',
        'google_oauth',
        'mini_max_oauth',
        'kimi_coding_plan_oauth'
    )),
    secret_value TEXT,
    access_token TEXT,
    refresh_token TEXT,
    issued_at_unix_ms INTEGER,
    expires_at_unix_ms INTEGER,
    scope_text TEXT NOT NULL DEFAULT '',
    account_subject TEXT,
    authorization_kind TEXT CHECK(authorization_kind IN ('browser_callback', 'rotation_pending')),
    provider_username TEXT,
    credential_label TEXT,
    email TEXT,
    id_token TEXT,
    account_id TEXT,
    oauth_client_id TEXT,
    oauth_client_secret TEXT,
    project_id TEXT,
    quota_project_id TEXT,
    provider_region TEXT,
    resource_url TEXT,
    provider_device_id TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    CHECK(
        (material_kind = 'api_key' AND secret_value IS NOT NULL AND length(trim(secret_value)) > 0)
        OR (material_kind = 'static_bearer' AND access_token IS NOT NULL AND length(trim(access_token)) > 0)
        OR (
            material_kind = 'open_design_auth'
            AND access_token IS NOT NULL AND length(trim(access_token)) > 0
            AND secret_value IS NOT NULL AND length(trim(secret_value)) > 0
            AND account_subject IS NOT NULL AND length(trim(account_subject)) > 0
            AND email IS NOT NULL AND length(trim(email)) > 0
            AND refresh_token IS NULL AND expires_at_unix_ms IS NULL
        )
        OR (
            material_kind = 'openvulcan_auth'
            AND access_token IS NOT NULL
            AND length(trim(access_token)) > 0
            AND refresh_token IS NULL
            AND issued_at_unix_ms IS NULL
            AND expires_at_unix_ms IS NOT NULL
            AND expires_at_unix_ms > 0
            AND provider_device_id IS NOT NULL
            AND length(trim(provider_device_id)) > 0
            AND provider_region IS NOT NULL
            AND provider_region IN ('china', 'global')
        )
        OR (
            material_kind IN (
                'open_ai_codex_oauth',
                'claude_code_oauth',
                'google_oauth',
                'mini_max_oauth',
                'kimi_coding_plan_oauth'
            )
            AND access_token IS NOT NULL
            AND length(trim(access_token)) > 0
            AND refresh_token IS NOT NULL
            AND length(trim(refresh_token)) > 0
            AND issued_at_unix_ms IS NOT NULL
            AND issued_at_unix_ms >= 0
            AND expires_at_unix_ms IS NOT NULL
            AND expires_at_unix_ms > issued_at_unix_ms
        )
    ),
    CHECK(
        material_kind != 'google_oauth'
        OR (oauth_client_id IS NOT NULL AND length(trim(oauth_client_id)) > 0)
    ),
    CHECK(
        material_kind != 'mini_max_oauth'
        OR (
            provider_region IS NOT NULL
            AND length(trim(provider_region)) > 0
            AND resource_url IS NOT NULL
            AND length(trim(resource_url)) > 0
        )
    ),
    CHECK(
        material_kind != 'kimi_coding_plan_oauth'
        OR (provider_device_id IS NOT NULL AND length(trim(provider_device_id)) > 0)
    ),
    CHECK(
        authorization_kind IS NULL
        OR (material_kind = 'openvulcan_auth' AND authorization_kind = 'rotation_pending')
        OR (
            material_kind IN ('api_key', 'static_bearer')
            AND authorization_kind = 'browser_callback'
            AND account_subject IS NOT NULL
            AND length(trim(account_subject)) > 0
            AND provider_username IS NOT NULL
            AND length(trim(provider_username)) > 0
            AND credential_label IS NOT NULL
            AND length(trim(credential_label)) > 0
        )
    )
);

CREATE TABLE IF NOT EXISTS project_git_review_preferences (
    project_id TEXT NOT NULL,
    repository_key TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, repository_key),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_git_review_preferences_project
ON project_git_review_preferences(project_id, repository_key ASC);

CREATE TABLE IF NOT EXISTS knowledge_libraries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    lifecycle_state TEXT NOT NULL DEFAULT 'active'
        CHECK(lifecycle_state IN ('active', 'deleting', 'error')),
    document_count INTEGER NOT NULL DEFAULT 0,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    index_status TEXT NOT NULL DEFAULT 'empty',
    last_completed_import_at_ms INTEGER,
    settings_json TEXT NOT NULL DEFAULT '{{}}',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_library_project_bindings (
    library_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    default_enabled INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (library_id, project_id),
    FOREIGN KEY (library_id) REFERENCES knowledge_libraries(id) ON DELETE CASCADE,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_knowledge_library_project_bindings_project
ON knowledge_library_project_bindings(project_id, default_enabled DESC, library_id);

CREATE TABLE IF NOT EXISTS knowledge_import_jobs (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL,
    source_root TEXT NOT NULL,
    operation_kind TEXT NOT NULL DEFAULT 'import',
    state TEXT NOT NULL,
    phase TEXT NOT NULL,
    scanned_files INTEGER NOT NULL DEFAULT 0,
    total_files INTEGER NOT NULL DEFAULT 0,
    processed_files INTEGER NOT NULL DEFAULT 0,
    total_index_items INTEGER NOT NULL DEFAULT 0,
    processed_index_items INTEGER NOT NULL DEFAULT 0,
    current_file TEXT,
    changed_files INTEGER NOT NULL DEFAULT 0,
    unchanged_files INTEGER NOT NULL DEFAULT 0,
    deleted_files INTEGER NOT NULL DEFAULT 0,
    skipped_files INTEGER NOT NULL DEFAULT 0,
    processed_bytes INTEGER NOT NULL DEFAULT 0,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    observability_json TEXT NOT NULL DEFAULT '{{}}',
    error_message TEXT,
    created_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    finished_at_ms INTEGER,
    FOREIGN KEY (library_id) REFERENCES knowledge_libraries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_knowledge_import_jobs_library
ON knowledge_import_jobs(library_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS knowledge_operations (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES knowledge_libraries(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    phase TEXT NOT NULL,
    total_items INTEGER NOT NULL DEFAULT 0,
    processed_items INTEGER NOT NULL DEFAULT 0,
    current_item TEXT,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    observability_json TEXT NOT NULL DEFAULT '{{}}',
    error_message TEXT,
    created_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    finished_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_knowledge_operations_library
ON knowledge_operations(library_id, created_at_ms DESC);

PRAGMA user_version = {schema_version};
"#,
        schema_version = APP_SCHEMA_VERSION,
        credential_table = AI_CREDENTIAL_SECRETS_TABLE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies initialization refuses unknown versions before modifying the version or stored data.
    /// 验证初始化在修改版本或已存数据之前拒绝未知版本。
    #[test]
    fn initialization_rejects_unknown_version_without_mutation() {
        // connection models a pre-existing database independently of the production data directory.
        // connection 模拟既有数据库，与生产数据目录隔离。
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        // unsupported_version deliberately differs from the version declared by the application.
        // unsupported_version 刻意使用与应用声明不同的版本。
        let unsupported_version = APP_SCHEMA_VERSION + 1;
        connection.execute_batch(&format!("PRAGMA user_version = {unsupported_version}; CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('retained');")).unwrap();
        assert!(validate_application_schema_for_initialization(&connection).is_err());
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            unsupported_version
        );
        assert_eq!(
            connection
                .query_row("SELECT value FROM sentinel", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "retained"
        );
    }

    /// Verifies that the base schema no longer creates legacy SQLite transcript tables.
    /// 验证基础结构不再创建旧版 SQLite 转录表。
    #[test]
    fn base_schema_excludes_legacy_session_message_tables() {
        let schema = build_application_schema_script();

        assert!(!schema.contains("CREATE TABLE IF NOT EXISTS session_messages"));
        assert!(!schema.contains("CREATE TABLE IF NOT EXISTS session_message_chunks"));
        assert!(!schema.contains("idx_session_messages_session_ordinal"));
        assert!(!schema.contains("idx_session_messages_session_turn_ordinal"));
    }

    /// Verifies that the base schema no longer includes the legacy global workspace-history search table.
    /// 验证基础结构不再包含旧全局工作台历史搜索表。
    #[test]
    fn base_schema_excludes_legacy_workspace_history_search_state_table() {
        let schema = build_application_schema_script();

        assert!(!schema.contains("CREATE TABLE IF NOT EXISTS workspace_history_search_docs"));
        assert!(!schema.contains("idx_workspace_history_search_docs_session"));
    }

    /// Verifies that heavyweight session body tables are not duplicated in SQLite.
    /// 验证重量级会话正文表不再重复写入 SQLite。
    #[test]
    fn base_schema_excludes_heavy_session_body_tables() {
        let schema = build_application_schema_script();

        assert!(!schema.contains("CREATE TABLE IF NOT EXISTS session_compactions"));
        assert!(!schema.contains("idx_session_compactions_session"));
    }

    /// Verifies that the base schema includes the lightweight deterministic session locator index.
    /// 验证基础结构包含轻量确定性会话定位索引。
    #[test]
    fn base_schema_includes_lightweight_session_locator_index() {
        let schema = build_application_schema_script();

        assert!(schema.contains("CREATE TABLE IF NOT EXISTS workspace_session_index"));
        assert!(schema.contains("idx_workspace_session_index_project"));
    }

    /// Verifies that runtime checkpoints are no longer duplicated into the application SQLite schema.
    /// 验证运行态检查点不再被重复写入应用 SQLite 结构。
    #[test]
    fn base_schema_excludes_session_runtime_state_table() {
        let schema = build_application_schema_script();

        assert!(!schema.contains("CREATE TABLE IF NOT EXISTS session_runtime_state"));
        assert!(!schema.contains("conversation_state_json"));
        assert!(!schema.contains("vmm_writeback_outbox_json"));
    }

    /// Verifies new databases persist cumulative per-request goal usage counters.
    /// 验证新数据库会持久化逐请求累计目标用量计数。
    #[test]
    fn base_schema_includes_goal_provider_usage_counters() {
        let schema = build_application_schema_script();

        for column in [
            "uncached_input_tokens INTEGER NOT NULL DEFAULT 0",
            "cached_input_tokens INTEGER NOT NULL DEFAULT 0",
            "output_tokens INTEGER NOT NULL DEFAULT 0",
            "reasoning_tokens INTEGER NOT NULL DEFAULT 0",
            "total_tokens INTEGER NOT NULL DEFAULT 0",
        ] {
            assert!(schema.contains(column));
        }
    }

    /// With no inputs, verifies relational credential columns through SQLite; returns on success.
    /// 无需参数，通过 SQLite 验证关系化凭据字段，成功时正常返回。
    #[test]
    fn base_schema_includes_relational_ai_credential_storage() {
        // connection executes the real schema instead of asserting source-text fragments.
        // connection 执行真实结构，而不是断言源码文本片段。
        let connection = rusqlite::Connection::open_in_memory().expect("private database");
        connection
            .execute_batch(&build_application_schema_script())
            .expect("initialize schema");
        // statement inspects the actual relational columns exposed to persistence.
        // statement 检查持久层实际可用的关系列。
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({AI_CREDENTIAL_SECRETS_TABLE})"))
            .expect("credential columns");
        // columns are matched by identity so added fields do not change assertion positions.
        // columns 按名称定位，新增字段不会改变断言位置。
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query columns")
            .collect::<Result<std::collections::BTreeSet<_>, _>>()
            .expect("collect columns");
        for column in [
            "material_kind",
            "provider_device_id",
            "authorization_kind",
            "provider_username",
            "credential_label",
            "scope_text",
        ] {
            assert!(
                columns.contains(column),
                "Missing credential column {column}; verify the current persistence contract"
            );
        }
        assert!(!columns.contains("credential_json"));
    }

    /// Verifies initialization creates every current object and writes the current schema version.
    /// 验证初始化创建全部当前对象并写入当前结构版本号。
    ///
    /// This is the test that makes "one initialization replaces the migration chain" a verified
    /// claim rather than an assertion: the object set below is the union of what the previous base
    /// script and the deleted additive migrations produced, so a missing table or index fails here.
    ///
    /// 本测试使"一次初始化取代迁移链"成为被验证的结论而非断言：下方对象集是此前基础脚本与被删除的追加式
    /// 迁移共同产出的并集，因此任何缺失的表或索引都会在此失败。
    #[test]
    fn initialization_creates_every_object_and_writes_current_version() {
        let connection = rusqlite::Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(&build_application_schema_script())
            .expect("the single v1 initialization must apply cleanly to an empty database");

        let mut statement = connection
            .prepare(
                "SELECT type, name FROM sqlite_master
                 WHERE name NOT LIKE 'sqlite_%' AND name NOT LIKE '_vulcan_%'
                 ORDER BY type, name",
            )
            .expect("prepare schema query");

        let observed = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query schema")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect schema");

        let mut expected: Vec<(String, String)> = [
            ("table", AI_CREDENTIAL_SECRETS_TABLE),
            ("table", "app_metadata"),
            ("table", "knowledge_import_jobs"),
            ("table", "knowledge_libraries"),
            ("table", "knowledge_library_project_bindings"),
            ("table", "knowledge_operations"),
            ("table", "project_git_review_preferences"),
            ("table", "project_vce_settings"),
            ("table", "projects"),
            ("table", "thread_goals"),
            ("table", "workspace_session_index"),
            ("index", "idx_knowledge_import_jobs_library"),
            ("index", "idx_knowledge_library_project_bindings_project"),
            ("index", "idx_knowledge_operations_library"),
            ("index", "idx_project_git_review_preferences_project"),
            ("index", "idx_projects_sort"),
            ("index", "idx_thread_goals_status"),
            ("index", "idx_workspace_session_index_project"),
        ]
        .into_iter()
        .map(|(kind, name)| (kind.to_string(), name.to_string()))
        .collect();
        expected.sort();

        // `session_messages`, `session_message_chunks` and `workspace_history_search_docs` are
        // deliberately absent: they belong to a separate schema, and sibling tests in this module
        // assert this script does not create them. An earlier draft of this list wrongly included
        // them, and the exact-set comparison is what caught it.
        // `session_messages`、`session_message_chunks` 与 `workspace_history_search_docs` 刻意不在其中：
        // 它们属于另一套结构，且本模块的同类测试断言本脚本不创建它们。本清单的早期草稿错误地包含了它们，
        // 正是精确集合比较捕捉到了这一点。

        let observed_sorted = {
            let mut sorted = observed;
            sorted.sort();
            sorted
        };
        assert_eq!(
            observed_sorted, expected,
            "the v1 initialization must create exactly the expected object set"
        );

        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(
            user_version, APP_SCHEMA_VERSION,
            "initialization must write the current schema version"
        );

        let recorded_version: String = connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("read recorded schema version");
        assert_eq!(recorded_version, APP_SCHEMA_VERSION.to_string());

        // Applying the initialization twice must be a no-op, because the script is the only entry
        // point and a host may re-run it against an existing database.
        // 重复应用初始化必须是无操作，因为该脚本是唯一入口，且宿主可能对既有数据库再次运行它。
        connection
            .execute_batch(&build_application_schema_script())
            .expect("re-applying the v1 initialization must be idempotent");
    }

    /// Verifies new databases own typed VCE settings in their own table.
    /// 验证新数据库在独立表中持有类型化 VCE 设置。
    #[test]
    fn project_schema_includes_independent_vce_settings() {
        let schema = build_application_schema_script();

        assert!(schema.contains("CREATE TABLE IF NOT EXISTS project_vce_settings"));
        assert!(schema.contains("extra_ignore_patterns_json TEXT NOT NULL DEFAULT '[]'"));
        assert!(
            schema.contains("FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE")
        );
    }

    /// Verifies that each bootstrap receives one distinct SQLite binding id while preserving the stable diagnostic tag prefix.
    /// 验证每次引导都会获得不同的 SQLite 绑定标识，并保留稳定诊断标签前缀。
    #[test]
    fn application_sqlite_binding_id_is_bootstrap_scoped() {
        let first = build_application_sqlite_binding_id();
        let second = build_application_sqlite_binding_id();

        assert!(first.starts_with("app-state@"));
        assert!(second.starts_with("app-state@"));
        assert_ne!(first, second);
    }

    /// Verifies that system statistics use their own bootstrap-scoped binding namespace.
    /// 验证系统统计使用自己的单次引导作用域绑定命名空间。
    #[test]
    fn system_statistics_sqlite_binding_id_is_bootstrap_scoped() {
        let first = build_system_statistics_sqlite_binding_id();
        let second = build_system_statistics_sqlite_binding_id();

        assert!(first.starts_with("provider-usage@"));
        assert!(second.starts_with("provider-usage@"));
        assert_ne!(first, second);
        assert!(!first.starts_with(APP_SQLITE_BINDING_TAG));
    }

    /// Verifies that LuaSkills package configuration uses one stable root below the application data plane.
    /// 验证 LuaSkills 技能包配置使用应用数据平面下的唯一稳定根目录。
    #[test]
    fn luaskills_config_root_is_stable_below_application_data_root() {
        assert_eq!(
            application_luaskills_config_root().expect("resolve LuaSkills config root"),
            application_data_root().join("luaskills-config")
        );
    }
}

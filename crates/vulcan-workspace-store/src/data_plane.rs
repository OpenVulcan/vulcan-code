//! Controller-backed application data-plane bootstrap helpers.
//! 基于控制器的应用数据平面引导辅助模块。

use crate::knowledge_base::KnowledgeBaseRuntimeSharedState;
use crate::provider_config::{PersistedControllerConfiguration, PersistedControllerProcessMode};
use crate::runtime_assets;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use vldb_controller_client::{
    ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    ControllerSqliteEnableRequest, ControllerStatusSnapshot, SpaceKind, SpaceRegistration,
};

/// Stable application-level controller space identifier.
/// 稳定的应用级控制器空间标识符。
pub const APP_SPACE_ID: &str = "app-workspace";

/// Stable application-level SQLite binding tag.
/// 稳定的应用级 SQLite 绑定标签。
pub const APP_SQLITE_BINDING_TAG: &str = "app-state";

/// Process-local sequence that guarantees each bootstrap gets one distinct SQLite binding suffix.
/// 进程内序号，确保每次引导都能拿到不同的 SQLite 绑定后缀。
static APP_SQLITE_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Shared schema version stored in the application metadata table.
/// 存储在应用元数据表中的共享结构版本号。
pub const APP_SCHEMA_VERSION: i64 = 13;

/// Long-lived controller handle kept by the host runtime after startup succeeds.
/// 启动成功后由宿主运行时持有的长生命周期控制器句柄。
#[derive(Clone)]
pub struct ControllerRuntimeHandle {
    /// Shared controller client proxy reused by later data operations.
    /// 供后续数据操作复用的共享控制器客户端代理。
    pub client: ControllerClient,
    /// Normalized controller endpoint URL used by the SDK.
    /// SDK 使用的标准化控制器端点 URL。
    pub endpoint: String,
    /// Resolved controller executable path or command name.
    /// 已解析的控制器可执行文件路径或命令名。
    pub executable: String,
    /// Local data root reserved for the application data plane.
    /// 为应用数据平面保留的本地数据根目录。
    pub data_root: PathBuf,
    /// SQLite file path used by the application state space.
    /// 应用状态空间使用的 SQLite 文件路径。
    pub sqlite_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier used by the current controller runtime handle.
    /// 当前控制器运行时句柄使用的单次引导作用域 SQLite 绑定标识符。
    pub sqlite_binding_id: String,
    /// Process-local knowledge state shared by every runtime reconstructed from this handle.
    /// 由基于当前句柄重建的所有运行时共享的进程内资料库状态。
    pub(crate) knowledge_runtime_state: Arc<KnowledgeBaseRuntimeSharedState>,
}

impl fmt::Debug for ControllerRuntimeHandle {
    /// Formats one controller runtime handle without dumping the opaque client internals.
    /// 在不输出不透明客户端内部状态的前提下格式化控制器运行时句柄。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControllerRuntimeHandle")
            .field("endpoint", &self.endpoint)
            .field("executable", &self.executable)
            .field("data_root", &self.data_root)
            .field("sqlite_path", &self.sqlite_path)
            .field("sqlite_binding_id", &self.sqlite_binding_id)
            .finish()
    }
}

impl ControllerRuntimeHandle {
    /// Builds one long-lived controller handle and initializes its shared knowledge runtime state.
    /// 构建一个长生命周期控制器句柄，并初始化其共享资料库运行时状态。
    ///
    /// `client` is the connected controller proxy, `endpoint` is its normalized URL,
    /// `executable` identifies the controller process, `data_root` owns application data,
    /// `sqlite_path` points to the application database, and `sqlite_binding_id` identifies
    /// the bootstrap-scoped application binding. The returned handle shares one knowledge
    /// activation and mutation state across every clone.
    /// `client` 是已连接的控制器代理，`endpoint` 是其标准化 URL，`executable` 标识控制器进程，
    /// `data_root` 保存应用数据，`sqlite_path` 指向应用数据库，`sqlite_binding_id` 标识单次引导作用域的
    /// 应用绑定。返回的句柄会在所有克隆之间共享同一份资料库激活与变更状态。
    pub fn new(
        client: ControllerClient,
        endpoint: String,
        executable: String,
        data_root: PathBuf,
        sqlite_path: PathBuf,
        sqlite_binding_id: String,
    ) -> Self {
        Self {
            client,
            endpoint,
            executable,
            data_root,
            sqlite_path,
            sqlite_binding_id,
            knowledge_runtime_state: Arc::new(KnowledgeBaseRuntimeSharedState::default()),
        }
    }
}

/// Summary returned after one controller bootstrap completes.
/// 单次控制器引导完成后返回的摘要信息。
#[derive(Debug, Clone)]
pub struct ControllerBootstrapReport {
    /// Long-lived controller handle retained by the application.
    /// 应用保留的长生命周期控制器句柄。
    pub handle: ControllerRuntimeHandle,
    /// Optional controller status snapshot fetched after connect.
    /// 在连接后拉取到的可选控制器状态快照。
    pub status: Option<ControllerStatusSnapshot>,
}

impl ControllerBootstrapReport {
    /// Builds one concise human-readable summary row for status bars and logs.
    /// 为状态栏与日志构建一条简洁的人类可读摘要。
    pub fn summary_text(&self) -> String {
        let process_mode = self
            .status
            .as_ref()
            .map(|status| match status.process_mode {
                ControllerProcessMode::Service => "Service",
                ControllerProcessMode::Managed => "Managed",
            })
            .unwrap_or("Unknown");

        format!(
            "vldb 数据平面已就绪：{} · 模式 {} · SQLite {}",
            self.handle.endpoint,
            process_mode,
            self.handle.sqlite_path.display()
        )
    }
}

/// Bootstraps the shared controller, the application space, and the primary SQLite schema.
/// 引导共享控制器、应用空间以及主 SQLite 结构。
pub async fn bootstrap_application_data_plane(
    controller: PersistedControllerConfiguration,
) -> Result<ControllerBootstrapReport, String> {
    let config = build_controller_client_config(&controller)?;
    let endpoint = config
        .endpoint_url()
        .map_err(|error| format!("解析 vldb 控制器端点失败：{error}"))?;
    let executable = config.spawn_executable().to_string();
    let data_root = application_data_root();
    let sqlite_path = data_root.join("sqlite").join("app-workspace.sqlite3");
    let started_at_ms = current_unix_epoch_millis();

    ensure_parent_directory(&sqlite_path)?;

    let registration = ClientRegistration {
        client_name: format!("vulcan-code-{}-{}", std::process::id(), started_at_ms),
        host_kind: String::from("vulcan-code"),
        process_id: std::process::id(),
        process_name: String::from("vulcan-code"),
        lease_ttl_secs: Some(controller.default_lease_ttl_secs),
    };
    let client = ControllerClient::new(config, registration);

    let bootstrap_result = async {
        client
            .connect()
            .await
            .map_err(|error| format!("连接或唤起 vldb-controller 失败：{error}"))?;

        let sqlite_binding_id = build_application_sqlite_binding_id();

        client
            .attach_space(SpaceRegistration {
                space_id: String::from(APP_SPACE_ID),
                space_label: String::from("APP"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(&data_root),
            })
            .await
            .map_err(|error| format!("附着应用数据空间失败：{error}"))?;

        client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: String::from(APP_SPACE_ID),
                binding_id: sqlite_binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(&sqlite_path),
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .map_err(|error| format!("启用应用 SQLite 数据库失败：{error}"))?;

        client
            .execute_sqlite_script_typed(
                APP_SPACE_ID,
                sqlite_binding_id.as_str(),
                build_application_schema_script(),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("初始化应用数据结构失败：{error}"))?;

        ensure_application_schema_migrations(&client, &sqlite_binding_id).await?;

        Ok::<_, String>((client.get_status().await.ok(), sqlite_binding_id))
    }
    .await;

    match bootstrap_result {
        Ok((status, sqlite_binding_id)) => Ok(ControllerBootstrapReport {
            handle: ControllerRuntimeHandle::new(
                client,
                endpoint,
                executable,
                data_root,
                sqlite_path,
                sqlite_binding_id,
            ),
            status,
        }),
        Err(error) => {
            cleanup_failed_controller_bootstrap(&client, &error).await?;
            Err(error)
        }
    }
}

/// Builds one controller client config from the persisted application configuration.
/// 根据持久化应用配置构建控制器客户端配置。
fn build_controller_client_config(
    controller: &PersistedControllerConfiguration,
) -> Result<ControllerClientConfig, String> {
    let mut config = ControllerClientConfig::default();

    if let Some(endpoint) = controller.endpoint.as_deref() {
        let trimmed = endpoint.trim();
        if !trimmed.is_empty() {
            config.endpoint = trimmed.to_string();
        }
    }

    config.auto_spawn = controller.auto_spawn;
    config.spawn_executable = controller
        .executable_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(runtime_assets::native_host_path_text)
        .or_else(|| {
            runtime_assets::resolve_bundled_controller_executable_path()
                .map(|path| runtime_assets::native_host_path_text_from_path(&path))
        });
    config.spawn_process_mode = match controller.process_mode {
        PersistedControllerProcessMode::Service => ControllerProcessMode::Service,
        PersistedControllerProcessMode::Managed => ControllerProcessMode::Managed,
    };
    config.minimum_uptime_secs = controller.minimum_uptime_secs;
    config.idle_timeout_secs = controller.idle_timeout_secs;
    config.default_lease_ttl_secs = controller.default_lease_ttl_secs;
    config.connect_timeout_secs = controller.connect_timeout_secs;
    config.startup_timeout_secs = controller.startup_timeout_secs;
    config.startup_retry_interval_ms = controller.startup_retry_interval_ms;
    config.lease_renew_interval_secs = controller.lease_renew_interval_secs;

    config
        .endpoint_url()
        .map_err(|error| format!("控制器端点格式不合法：{error}"))?;

    Ok(config)
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

/// Shuts down one partially initialized controller client after bootstrap failure so the next retry starts cleanly.
/// 在引导失败后关闭部分初始化的控制器客户端，确保下一次重试从干净状态开始。
async fn cleanup_failed_controller_bootstrap(
    client: &ControllerClient,
    bootstrap_error: &str,
) -> Result<(), String> {
    client.shutdown().await.map_err(|cleanup_error| {
        format!("{bootstrap_error}；另外清理失败的 vldb 会话也失败：{cleanup_error}")
    })
}

/// Applies additive schema migrations required by newer application builds.
/// 应用较新应用版本所需的追加式结构迁移。
pub(crate) async fn ensure_application_schema_migrations(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    ensure_optional_project_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE projects ADD COLUMN icon_pack TEXT;",
    )
    .await?;
    ensure_optional_project_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE projects ADD COLUMN icon_key TEXT;",
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
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
"#,
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
CREATE INDEX IF NOT EXISTS idx_workspace_session_index_project
ON workspace_session_index(project_id, archived, sort_order ASC, updated_at DESC);
"#,
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
CREATE TABLE IF NOT EXISTS thread_goals (
    thread_id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    objective TEXT NOT NULL,
    status TEXT NOT NULL,
    token_budget INTEGER,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    time_used_seconds INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
"#,
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
CREATE INDEX IF NOT EXISTS idx_thread_goals_status
ON thread_goals(status, updated_at_ms DESC);

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
    error_message TEXT,
    created_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    finished_at_ms INTEGER,
    FOREIGN KEY (library_id) REFERENCES knowledge_libraries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_knowledge_import_jobs_library
ON knowledge_import_jobs(library_id, created_at_ms DESC);
"#,
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN operation_kind TEXT NOT NULL DEFAULT 'import';",
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN total_files INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN processed_files INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN current_file TEXT;",
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN total_index_items INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN processed_index_items INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    Ok(())
}

/// Applies one idempotent project column migration and ignores duplicate-column failures.
/// 执行一条幂等项目列迁移，并忽略重复列失败。
async fn ensure_optional_project_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_schema_column(client, sqlite_binding_id, statement).await
}

/// Applies one idempotent optional column migration and ignores duplicate-column failures.
/// 执行一条幂等可选列迁移，并忽略重复列失败。
async fn ensure_optional_schema_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_schema_statement(client, sqlite_binding_id, statement).await
}

/// Applies one idempotent optional schema statement and ignores already-applied failures.
/// 执行一条幂等可选结构语句，并忽略已经应用过的失败。
async fn ensure_optional_schema_statement(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    let migration_result = client
        .execute_sqlite_script_typed(APP_SPACE_ID, sqlite_binding_id, statement, Vec::new())
        .await;

    match migration_result {
        Ok(_) => Ok(()),
        Err(error) => {
            let detail = error.to_string();
            if detail.contains("duplicate column name")
                || detail.contains("already exists")
                || detail.contains("duplicate column")
            {
                Ok(())
            } else {
                Err(format!("执行应用结构迁移失败：{detail}"))
            }
        }
    }
}

/// Returns one best-effort Unix epoch timestamp in milliseconds for controller client naming.
/// 返回一个尽力获取的 Unix 纪元毫秒时间戳，用于控制器客户端命名。
fn current_unix_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

/// Returns the stable application data root used for controller-backed state files.
/// 返回控制器承载状态文件使用的稳定应用数据根目录。
pub fn application_data_root() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir()
            .join("vulcan-code-tests")
            .join(std::process::id().to_string())
    }

    #[cfg(not(test))]
    {
        let home = dirs::home_dir().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .to_path_buf()
        });

        home.join(".vulcan").join("code")
    }
}

/// Ensures the parent directory of one target file exists.
/// 确保某个目标文件的父目录已经存在。
fn ensure_parent_directory(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)
        .map_err(|error| format!("创建应用数据目录失败（{}）：{error}", parent.display()))
}

/// Builds the initial SQLite schema used by the Vulcan application data plane.
/// 构建 Vulcan 应用数据平面使用的初始 SQLite 结构。
pub(crate) fn build_application_schema_script() -> String {
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
    time_used_seconds INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_goals_status
ON thread_goals(status, updated_at_ms DESC);
"#,
        schema_version = APP_SCHEMA_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

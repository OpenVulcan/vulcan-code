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

/// Stable controller space identifier for host-owned system statistics.
/// 宿主持有的系统统计稳定控制器空间标识符。
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

/// Shared schema version stored in the application metadata table.
/// 存储在应用元数据表中的共享结构版本号。
pub const APP_SCHEMA_VERSION: i64 = 16;

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

impl ControllerRuntimeHandle {
    /// Builds one long-lived controller handle and initializes its shared knowledge runtime state.
    /// 构建一个长生命周期控制器句柄，并初始化其共享知识库运行时状态。
    ///
    /// `client` is the connected controller proxy, `endpoint` is its normalized URL,
    /// `executable` identifies the controller process, `data_root` owns application data,
    /// `sqlite_path` points to the application database, `sqlite_binding_id` identifies
    /// the bootstrap-scoped application binding, and the system-statistics arguments identify
    /// the independent Provider usage database. The returned handle shares one knowledge
    /// activation and mutation state across every clone.
    /// `client` 是已连接的控制器代理，`endpoint` 是其标准化 URL，`executable` 标识控制器进程，
    /// `data_root` 保存应用数据，`sqlite_path` 指向应用数据库，`sqlite_binding_id` 标识单次引导作用域的
    /// 应用绑定，系统统计参数标识独立的 Provider 用量数据库。返回的句柄会在所有克隆之间共享同一份
    /// 知识库激活与变更状态。
    pub fn new(
        client: ControllerClient,
        endpoint: String,
        executable: String,
        data_root: PathBuf,
        sqlite_path: PathBuf,
        sqlite_binding_id: String,
        system_statistics_sqlite_path: PathBuf,
        system_statistics_sqlite_binding_id: String,
    ) -> Self {
        Self {
            client,
            endpoint,
            executable,
            data_root,
            sqlite_path,
            sqlite_binding_id,
            system_statistics_sqlite_path,
            system_statistics_sqlite_binding_id,
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
    let system_statistics_root = data_root.join("statistics");
    let system_statistics_sqlite_path = system_statistics_root.join("provider-usage.sqlite3");
    let started_at_ms = current_unix_epoch_millis();

    ensure_parent_directory(&sqlite_path)?;
    ensure_parent_directory(&system_statistics_sqlite_path)?;

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
        let system_statistics_sqlite_binding_id = build_system_statistics_sqlite_binding_id();

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
            .attach_space(SpaceRegistration {
                space_id: String::from(SYSTEM_STATISTICS_SPACE_ID),
                space_label: String::from("SYSTEM STATISTICS"),
                space_kind: SpaceKind::User,
                space_root: runtime_assets::native_host_path_text_from_path(
                    &system_statistics_root,
                ),
            })
            .await
            .map_err(|error| format!("附着系统统计数据空间失败：{error}"))?;

        client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: String::from(SYSTEM_STATISTICS_SPACE_ID),
                binding_id: system_statistics_sqlite_binding_id.clone(),
                db_path: runtime_assets::native_host_path_text_from_path(
                    &system_statistics_sqlite_path,
                ),
                ..ControllerSqliteEnableRequest::default()
            })
            .await
            .map_err(|error| format!("启用系统统计 SQLite 数据库失败：{error}"))?;

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

        let runtime_handle = ControllerRuntimeHandle::new(
            client.clone(),
            endpoint.clone(),
            executable.clone(),
            data_root.clone(),
            sqlite_path.clone(),
            sqlite_binding_id,
            system_statistics_sqlite_path.clone(),
            system_statistics_sqlite_binding_id,
        );
        crate::workspace_store::ensure_provider_usage_schema(&runtime_handle).await?;
        crate::workspace_store::migrate_application_provider_usage(&runtime_handle).await?;

        Ok::<_, String>((client.get_status().await.ok(), runtime_handle))
    }
    .await;

    match bootstrap_result {
        Ok((status, handle)) => Ok(ControllerBootstrapReport { handle, status }),
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

/// Additive migration that keeps every existing project disabled by default.
/// 使所有既有项目保持默认关闭的追加式迁移。
const ADD_CODEBASE_MEMORY_ENABLED_COLUMN_SQL: &str =
    "ALTER TABLE projects ADD COLUMN codebase_memory_enabled INTEGER NOT NULL DEFAULT 0;";

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
    ensure_optional_project_column(
        client,
        sqlite_binding_id,
        ADD_CODEBASE_MEMORY_ENABLED_COLUMN_SQL,
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
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
"#,
    )
    .await?;
    ensure_optional_schema_statement(
        client,
        sqlite_binding_id,
        r#"
CREATE TABLE IF NOT EXISTS ai_credential_secrets (
    secret_reference TEXT PRIMARY KEY,
    format_version INTEGER NOT NULL CHECK(format_version > 0),
    revision INTEGER NOT NULL CHECK(revision > 0),
    material_kind TEXT NOT NULL CHECK(material_kind IN (
        'api_key',
        'static_bearer',
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
    )
);
"#,
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
    uncached_input_tokens INTEGER NOT NULL DEFAULT 0,
    cached_input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    time_used_seconds INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
"#,
    )
    .await?;
    // goalUsageColumns are additive cumulative counters introduced for per-request goal accounting.
    // goalUsageColumns 是为逐请求目标记账引入的可累加累计计数列。
    for statement in [
        "ALTER TABLE thread_goals ADD COLUMN uncached_input_tokens INTEGER NOT NULL DEFAULT 0;",
        "ALTER TABLE thread_goals ADD COLUMN cached_input_tokens INTEGER NOT NULL DEFAULT 0;",
        "ALTER TABLE thread_goals ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;",
        "ALTER TABLE thread_goals ADD COLUMN reasoning_tokens INTEGER NOT NULL DEFAULT 0;",
        "ALTER TABLE thread_goals ADD COLUMN total_tokens INTEGER NOT NULL DEFAULT 0;",
    ] {
        ensure_optional_schema_column(client, sqlite_binding_id, statement).await?;
    }
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
    settings_json TEXT NOT NULL DEFAULT '{}',
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
    observability_json TEXT NOT NULL DEFAULT '{}',
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
        "ALTER TABLE knowledge_libraries ADD COLUMN settings_json TEXT NOT NULL DEFAULT '{}';",
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
    ensure_optional_schema_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE knowledge_import_jobs ADD COLUMN observability_json TEXT NOT NULL DEFAULT '{}';",
    )
    .await?;
    Ok(())
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
    let root = dirs::home_dir()
        .ok_or_else(|| String::from("无法解析 LuaSkills 技能包配置根：系统主目录不可用。"))?
        .join(".vulcan")
        .join("code");

    if !root.is_absolute() {
        return Err(format!(
            "LuaSkills 技能包配置根必须是绝对路径：{}",
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
    codebase_memory_enabled INTEGER NOT NULL DEFAULT 0,
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

CREATE TABLE IF NOT EXISTS ai_credential_secrets (
    secret_reference TEXT PRIMARY KEY,
    format_version INTEGER NOT NULL CHECK(format_version > 0),
    revision INTEGER NOT NULL CHECK(revision > 0),
    material_kind TEXT NOT NULL CHECK(material_kind IN (
        'api_key',
        'static_bearer',
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
    )
);
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

    /// Verifies that the base schema stores AI credentials as relational columns without JSON documents.
    /// 验证基础结构使用关系字段存储 AI 凭据且不保存 JSON 文档。
    #[test]
    fn base_schema_includes_relational_ai_credential_storage() {
        let schema = build_application_schema_script();

        assert_eq!(APP_SCHEMA_VERSION, 16);
        assert!(schema.contains("CREATE TABLE IF NOT EXISTS ai_credential_secrets"));
        assert!(schema.contains("provider_device_id TEXT"));
        assert!(schema.contains("scope_text TEXT NOT NULL DEFAULT ''"));
        assert!(!schema.contains("credential_json"));
    }

    /// Verifies both new and upgraded databases default Codebase Memory to disabled.
    /// 验证新建与升级数据库都会把 Codebase Memory 默认设为关闭。
    #[test]
    fn project_schema_and_migration_default_codebase_memory_to_disabled() {
        let schema = build_application_schema_script();

        assert!(schema.contains("codebase_memory_enabled INTEGER NOT NULL DEFAULT 0"));
        assert_eq!(
            ADD_CODEBASE_MEMORY_ENABLED_COLUMN_SQL,
            "ALTER TABLE projects ADD COLUMN codebase_memory_enabled INTEGER NOT NULL DEFAULT 0;"
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

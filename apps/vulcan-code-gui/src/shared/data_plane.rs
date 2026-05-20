//! Controller-backed application data-plane bootstrap helpers.
//! 基于控制器的应用数据平面引导辅助模块。

use crate::provider_config::{PersistedControllerConfiguration, PersistedControllerProcessMode};
use crate::runtime_assets;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
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
pub const APP_SCHEMA_VERSION: i64 = 7;

/// One-shot metadata key that marks the non-compatible session reset required by the protocol refactor.
/// 标记协议重构所需一次性不兼容会话清理已经完成的元数据键。
const SESSION_PROTOCOL_RESET_MARKER_KEY: &str = "session-protocol-v4-reset";

/// Long-lived controller handle kept by the GUI after startup succeeds.
/// 启动成功后由图形界面持有的长生命周期控制器句柄。
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
        process_name: String::from("vulcan-code-gui"),
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
                space_root: data_root.display().to_string(),
            })
            .await
            .map_err(|error| format!("附着应用数据空间失败：{error}"))?;

        client
            .enable_sqlite(ControllerSqliteEnableRequest {
                space_id: String::from(APP_SPACE_ID),
                binding_id: sqlite_binding_id.clone(),
                db_path: sqlite_path.display().to_string(),
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
            handle: ControllerRuntimeHandle {
                client,
                endpoint,
                executable,
                data_root,
                sqlite_path,
                sqlite_binding_id,
            },
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
        .map(str::to_string)
        .or_else(|| {
            runtime_assets::resolve_bundled_controller_executable_path()
                .map(|path| path.display().to_string())
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
async fn ensure_application_schema_migrations(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    ensure_optional_runtime_state_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_runtime_state ADD COLUMN ui_state_json TEXT;",
    )
    .await?;
    ensure_optional_runtime_state_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_runtime_state ADD COLUMN provider_usage_json TEXT;",
    )
    .await?;
    ensure_optional_runtime_state_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_runtime_state ADD COLUMN conversation_state_json TEXT;",
    )
    .await?;
    ensure_optional_runtime_state_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_runtime_state ADD COLUMN continuation_json TEXT;",
    )
    .await?;
    ensure_optional_session_compaction_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_compactions ADD COLUMN source_until_turn_index INTEGER;",
    )
    .await?;
    ensure_optional_session_compaction_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_compactions ADD COLUMN retained_tail_start_turn_index INTEGER;",
    )
    .await?;
    ensure_optional_session_message_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE session_messages ADD COLUMN turn_index INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_session_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE sessions ADD COLUMN latest_turn_index INTEGER NOT NULL DEFAULT -1;",
    )
    .await?;
    ensure_optional_session_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE sessions ADD COLUMN latest_event_seq INTEGER NOT NULL DEFAULT -1;",
    )
    .await?;
    ensure_session_message_chunk_table(client, sqlite_binding_id).await?;
    ensure_workspace_history_search_state_table(client, sqlite_binding_id).await?;
    ensure_session_message_turn_index(client, sqlite_binding_id).await?;
    ensure_session_event_turn_index_table(client, sqlite_binding_id).await?;
    ensure_optional_workspace_history_search_doc_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE workspace_history_search_docs ADD COLUMN event_id TEXT;",
    )
    .await?;
    ensure_optional_workspace_history_search_doc_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE workspace_history_search_docs ADD COLUMN turn_index INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_workspace_history_search_doc_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE workspace_history_search_docs ADD COLUMN ordinal INTEGER NOT NULL DEFAULT 0;",
    )
    .await?;
    ensure_optional_workspace_history_search_doc_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE workspace_history_search_docs ADD COLUMN role TEXT NOT NULL DEFAULT 'assistant';",
    )
    .await?;
    ensure_optional_workspace_history_search_doc_column(
        client,
        sqlite_binding_id,
        "ALTER TABLE workspace_history_search_docs ADD COLUMN created_at TEXT NOT NULL DEFAULT '';",
    )
    .await?;
    ensure_protocol_reset_applied(client, sqlite_binding_id).await
}

/// Applies one idempotent session-message column migration and ignores duplicate-column failures.
/// 执行一条幂等会话消息列迁移，并忽略重复列失败。
async fn ensure_optional_session_message_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_runtime_state_column(client, sqlite_binding_id, statement).await
}

/// Applies one idempotent session column migration and ignores duplicate-column failures.
/// 执行一条幂等会话列迁移，并忽略重复列失败。
async fn ensure_optional_session_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_runtime_state_column(client, sqlite_binding_id, statement).await
}

/// Executes one idempotent session-compaction column migration and ignores duplicate-column failures.
/// 执行一条幂等会话压缩列迁移，并忽略重复列失败。
async fn ensure_optional_session_compaction_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_runtime_state_column(client, sqlite_binding_id, statement).await
}

/// Applies one idempotent workspace-history search document column migration and ignores duplicate-column failures.
/// 执行一条幂等工作台历史搜索文档列迁移，并忽略重复列失败。
async fn ensure_optional_workspace_history_search_doc_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    ensure_optional_runtime_state_column(client, sqlite_binding_id, statement).await
}

/// Ensures the additive session-message chunk table exists for large transcript bodies.
/// 确保用于大型转录正文的追加式会话消息分片表已经存在。
async fn ensure_session_message_chunk_table(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            session_message_chunk_schema_script(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("执行会话消息分片结构迁移失败：{error}"))?;

    Ok(())
}

/// Ensures the additive workspace-history search state table exists for controller-managed FTS ownership tracking.
/// 确保控制器托管 FTS 归属跟踪使用的工作台历史搜索状态表已经存在。
async fn ensure_workspace_history_search_state_table(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            workspace_history_search_state_schema_script(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("执行工作台历史搜索状态结构迁移失败：{error}"))?;

    Ok(())
}

/// Ensures the turn-index lookup index exists for turn-level transcript pagination.
/// 确保用于轮次级转录分页的 turn-index 查询索引已经存在。
async fn ensure_session_message_turn_index(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            String::from(session_message_turn_index_statement()),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("执行会话消息轮次索引迁移失败：{error}"))?;

    Ok(())
}

/// Ensures the canonical session turn-index table exists for event-store hydration, paging, and context reconstruction.
/// 确保规范会话 turn 索引表已经存在，以支持事件流水合、分页与上下文重建。
async fn ensure_session_event_turn_index_table(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            session_event_turn_index_schema_script(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("执行会话事件 turn 索引结构迁移失败：{error}"))?;

    Ok(())
}

/// Returns the additive turn-index lookup statement run after the column is guaranteed to exist.
/// 返回在确认列存在后执行的追加式轮次索引语句。
fn session_message_turn_index_statement() -> &'static str {
    r#"
CREATE INDEX IF NOT EXISTS idx_session_messages_session_turn_ordinal
ON session_messages(session_id, turn_index, ordinal);
"#
}

/// Applies one idempotent runtime-state column migration and ignores duplicate-column failures.
/// 执行一条幂等运行时状态列迁移，并忽略重复列失败。
async fn ensure_optional_runtime_state_column(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    statement: &str,
) -> Result<(), String> {
    let migration_result = client
        .execute_sqlite_batch_typed(APP_SPACE_ID, sqlite_binding_id, statement, vec![Vec::new()])
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

/// Applies the one-shot non-compatible session reset required by the protocol refactor while preserving projects.
/// 在保留项目数据的前提下，执行协议重构所需的一次性不兼容会话清理。
async fn ensure_protocol_reset_applied(
    client: &ControllerClient,
    sqlite_binding_id: &str,
) -> Result<(), String> {
    if query_metadata_flag(client, sqlite_binding_id, SESSION_PROTOCOL_RESET_MARKER_KEY).await? {
        return Ok(());
    }

    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            format!(
                r#"
DELETE FROM session_runtime_state;
DELETE FROM session_compactions;
DELETE FROM session_message_chunks;
DELETE FROM session_messages;
DELETE FROM sessions;
INSERT INTO app_metadata (key, value)
VALUES ('{marker_key}', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
                marker_key = SESSION_PROTOCOL_RESET_MARKER_KEY
            ),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("执行协议升级会话清理失败：{error}"))?;

    Ok(())
}

/// Queries one metadata-flag key and returns whether it has already been written to the app metadata table.
/// 查询单个元数据标记键，并返回它是否已经写入应用元数据表。
async fn query_metadata_flag(
    client: &ControllerClient,
    sqlite_binding_id: &str,
    key: &str,
) -> Result<bool, String> {
    #[derive(serde::Deserialize)]
    /// Typed metadata row used by the reset-marker lookup query.
    /// 供重置标记查询使用的带类型元数据行。
    struct MetadataRow {
        /// Persisted metadata value.
        /// 持久化元数据值。
        value: String,
    }

    let result = client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            sqlite_binding_id,
            "SELECT value FROM app_metadata WHERE key = ?1 LIMIT 1;",
            vec![vldb_controller_client::ControllerSqliteValue::String(
                key.to_string(),
            )],
        )
        .await
        .map_err(|error| format!("查询协议升级元数据标记失败：{error}"))?;
    let rows = serde_json::from_str::<Vec<MetadataRow>>(&result.json_data)
        .map_err(|error| format!("解析协议升级元数据标记失败：{error}"))?;

    Ok(rows
        .first()
        .map(|row| row.value.trim() == "1")
        .unwrap_or(false))
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
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });

    home.join(".vulcan").join("code")
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
fn build_application_schema_script() -> String {
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
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    title_source TEXT NOT NULL DEFAULT 'generated',
    archived INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    latest_turn_index INTEGER NOT NULL DEFAULT -1,
    latest_event_seq INTEGER NOT NULL DEFAULT -1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    visible INTEGER NOT NULL DEFAULT 1,
    turn_index INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    meta_json TEXT,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_session_messages_session_ordinal
ON session_messages(session_id, ordinal);

{session_message_chunk_schema}
{session_event_turn_index_schema}

{workspace_history_search_state_schema}

CREATE TABLE IF NOT EXISTS session_compactions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    source_until_message_id TEXT,
    source_until_turn_index INTEGER,
    context_start_message_id TEXT,
    retained_tail_start_turn_index INTEGER,
    summary_message_id TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_runtime_state (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    active_agent_id TEXT,
    active_provider_id TEXT,
    active_model_id TEXT,
    last_compaction_id TEXT,
    context_start_message_id TEXT,
    ui_state_json TEXT,
    provider_usage_json TEXT,
    conversation_state_json TEXT,
    continuation_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_sort
ON projects(pinned DESC, sort_order ASC, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_sessions_project_sort
ON sessions(project_id, archived ASC, sort_order ASC, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_session_compactions_session
ON session_compactions(session_id, created_at DESC);
"#,
        schema_version = APP_SCHEMA_VERSION,
        session_message_chunk_schema = session_message_chunk_schema_script(),
        session_event_turn_index_schema = session_event_turn_index_schema_script(),
        workspace_history_search_state_schema = workspace_history_search_state_schema_script()
    )
}

/// Builds the additive schema objects used to split large transcript message bodies.
/// 构建用于拆分大型转录消息正文的追加式结构对象。
fn session_message_chunk_schema_script() -> String {
    r#"
CREATE TABLE IF NOT EXISTS session_message_chunks (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES session_messages(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    PRIMARY KEY(message_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_session_message_chunks_session_message
ON session_message_chunks(session_id, message_id, chunk_index);
"#
    .to_string()
}

/// Builds the additive state table used to track controller-managed workspace-history FTS ownership.
/// 构建用于跟踪控制器托管工作台历史 FTS 归属关系的追加式状态表。
fn workspace_history_search_state_schema_script() -> String {
    r#"
CREATE TABLE IF NOT EXISTS workspace_history_search_docs (
    message_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    event_id TEXT,
    turn_index INTEGER NOT NULL DEFAULT 0,
    ordinal INTEGER NOT NULL DEFAULT 0,
    role TEXT NOT NULL DEFAULT 'assistant',
    created_at TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_workspace_history_search_docs_session
ON workspace_history_search_docs(session_id, project_id);
"#
    .to_string()
}

/// Builds the canonical session turn-index schema mirrored from the file-backed event store.
/// 构建与文件化事件存储镜像同步的规范会话 turn 索引结构。
fn session_event_turn_index_schema_script() -> String {
    r#"
CREATE TABLE IF NOT EXISTS session_turn_index (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    start_seq INTEGER NOT NULL,
    end_seq INTEGER NOT NULL,
    turn_state TEXT NOT NULL,
    summary_message_id TEXT,
    PRIMARY KEY(session_id, turn_index)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_session_turn_index_session_turn_id
ON session_turn_index(session_id, turn_id);

CREATE INDEX IF NOT EXISTS idx_session_turn_index_session_seq
ON session_turn_index(session_id, start_seq, end_seq);
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the base schema does not create turn-index objects before additive migrations run.
    /// 验证基础结构不会在追加迁移执行前创建依赖 turn_index 的对象。
    #[test]
    fn base_schema_defers_turn_index_lookup_index_to_migration() {
        let schema = build_application_schema_script();

        assert!(schema.contains("turn_index INTEGER NOT NULL DEFAULT 0"));
        assert!(!schema.contains("idx_session_messages_session_turn_ordinal"));
    }

    /// Verifies that the turn-index migration creates the lookup index after the column migration.
    /// 验证轮次索引迁移会在列迁移之后创建查询索引。
    #[test]
    fn turn_index_migration_defines_lookup_index() {
        let statement = session_message_turn_index_statement();

        assert!(statement.contains("idx_session_messages_session_turn_ordinal"));
        assert!(statement.contains("turn_index"));
    }

    /// Verifies that the base schema includes the workspace-history search ownership state table.
    /// 验证基础结构已经包含工作台历史搜索归属状态表定义。
    #[test]
    fn base_schema_includes_workspace_history_search_state_table() {
        let schema = build_application_schema_script();

        assert!(schema.contains("CREATE TABLE IF NOT EXISTS workspace_history_search_docs"));
        assert!(schema.contains("idx_workspace_history_search_docs_session"));
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

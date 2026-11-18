//! Persistent projects and sessions owned by authenticated external MCP clients.
//! 由已认证外部 MCP 客户端持有的持久项目和会话。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use vulcan_sqlite_engine::runtime::{SqliteOpenOptions, open_sqlite_connection};

/// Host-only session binding resolved from the client's short identifier.
/// 根据客户端短标识解析出的宿主专用会话绑定。
pub(super) struct SessionBinding {
    /// Public project identifier.
    /// 对外项目标识。
    pub project_id: String,
    /// Absolute directory attached to this project.
    /// 绑定到此项目的绝对目录。
    pub cwd: PathBuf,
    /// Stable private identifier for tool runtimes.
    /// 工具运行时使用的稳定私有标识。
    pub uuid: Uuid,
}

/// Management output and any private identities retired by the transaction.
/// 管理工具结果及事务注销的私有身份。
pub(super) struct ManagementResult {
    /// Value returned to the client.
    /// 返回给客户端的值。
    pub output: Value,
    /// Sessions whose live resources must be stopped.
    /// 需要关闭实时资源的会话。
    pub removed: Vec<Uuid>,
}

/// Project commands use explicit fields for each operation.
/// 项目命令按操作声明精确字段。
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ProjectCommand {
    New {
        name: String,
        path: PathBuf,
        #[serde(default)]
        note: String,
    },
    List,
    Update {
        project_id: String,
        note: String,
    },
    Remove {
        project_id: String,
    },
}

/// Session commands reference their project during creation and optional listing.
/// 会话命令只在创建和可选的列表筛选中引用项目。
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SessionCommand {
    New {
        project_id: String,
        #[serde(default)]
        note: String,
    },
    List {
        project_id: Option<String>,
    },
    Update {
        sid: String,
        note: String,
    },
    Remove {
        sid: String,
    },
}

/// Opens a process-owned database on a blocking worker and runs one operation.
/// 在阻塞工作线程打开进程持有的数据库并运行一次操作。
pub(super) async fn database<T: Send + 'static>(
    path: PathBuf,
    schema: &'static str,
    run: impl FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || {
        let parent = path
            .parent()
            .ok_or("The MCP database directory is unavailable.")?;
        std::fs::create_dir_all(parent).map_err(db_error)?;
        let mut connection =
            open_sqlite_connection(&path, &SqliteOpenOptions::default()).map_err(db_error)?;
        connection.execute_batch(schema).map_err(db_error)?;
        run(&mut connection)
    })
    .await
    .map_err(db_error)?
}

/// Formats a database operation error without exposing SQL parameter values.
/// 格式化数据库操作错误，不输出 SQL 参数值。
pub(super) fn db_error(error: impl std::fmt::Display) -> String {
    format!("MCP storage: {error}")
}

/// Returns milliseconds in SQLite's signed integer range.
/// 返回 SQLite 有符号整数范围内的毫秒时间戳。
pub(super) fn sqlite_time() -> i64 {
    i64::try_from(super::now_unix_millis()).unwrap_or(i64::MAX)
}

/// Returns this profile's dedicated storage root.
/// 返回当前配置环境专用的 MCP 存储目录。
pub(super) fn storage_root() -> Result<PathBuf, String> {
    vulcan_runtime_assets::resolve_user_vulcan_root()
        .map(|root| root.join("code").join("external-mcp"))
        .ok_or_else(|| "The MCP storage directory is unavailable.".into())
}

/// The independent context schema preserves tombstones and allocates identifiers in SQLite.
/// 独立上下文数据结构保留删除标记并由 SQLite 分配标识。
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  client_id TEXT NOT NULL, name TEXT NOT NULL, path TEXT NOT NULL, note TEXT NOT NULL,
  removed_at INTEGER
);
CREATE TABLE IF NOT EXISTS sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects(id),
  uuid TEXT NOT NULL UNIQUE, note TEXT NOT NULL, removed_at INTEGER
);
CREATE INDEX IF NOT EXISTS mcp_projects_owner ON projects(client_id, removed_at, id);
CREATE INDEX IF NOT EXISTS mcp_sessions_project ON sessions(project_id, removed_at, id);
";

/// Parses one canonical short identifier, with no implicit ID namespace conversion.
/// 解析一个规范短标识，不隐式转换标识命名空间。
pub(super) fn short_id(text: &str, prefix: &str) -> Result<i64, String> {
    let number = text
        .strip_prefix(prefix)
        .and_then(|input| input.parse::<i64>().ok())
        .filter(|number| *number > 0 && text == format!("{prefix}{number}"));
    number.ok_or_else(|| format!("Expected a {prefix} identifier."))
}

/// Checks an absolute directory at registration without modifying any of its files.
/// 登记时检查绝对目录，不修改其中的任何文件。
fn project_path(path: &Path) -> Result<String, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("An existing absolute project directory is required.".into());
    }
    let canonical = path.canonicalize().map_err(db_error)?;
    canonical
        .to_str()
        .map(vulcan_runtime_assets::native_host_path_text)
        .ok_or_else(|| "The project directory must be valid UTF-8.".into())
}

/// Validates a bounded display label or note before storage.
/// 写入前校验展示名称或备注的长度。
fn text_field(input: &str, max_chars: usize, required: bool) -> Result<&str, String> {
    let value = input.trim();
    if (required && value.is_empty()) || value.chars().count() > max_chars {
        return Err(format!(
            "Invalid project name or note; the limit is {max_chars} characters."
        ));
    }
    Ok(value)
}

/// Runs one project or session management operation for its authenticated owner.
/// 为认证所有者执行一次项目或会话管理操作。
pub(super) async fn manage(
    root: &Path,
    client: &str,
    tool: &str,
    arguments: Value,
) -> Result<ManagementResult, String> {
    let owner = client.to_owned();
    let project_command = tool == "vc_mcp_project";
    database(root.join("contexts.sqlite3"), SCHEMA, move |connection| {
        if project_command {
            manage_projects(
                connection,
                &owner,
                serde_json::from_value(arguments).map_err(db_error)?,
            )
        } else {
            manage_sessions(
                connection,
                &owner,
                serde_json::from_value(arguments).map_err(db_error)?,
            )
        }
    })
    .await
}

/// Applies a project command in a transaction and returns public fields.
/// 在事务中执行项目命令并返回公共字段。
fn manage_projects(
    connection: &mut Connection,
    owner: &str,
    command: ProjectCommand,
) -> Result<ManagementResult, String> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let mut removed = Vec::new();
    let output = match command {
        ProjectCommand::New { name, path, note } => {
            let name = text_field(&name, 80, true)?;
            let path = project_path(&path)?;
            let note = text_field(&note, 300, false)?;
            transaction
                .execute(
                    "INSERT INTO projects(client_id,name,path,note) VALUES (?1,?2,?3,?4)",
                    params![owner, name, path, note],
                )
                .map_err(db_error)?;
            json!({ "project_id": format!("p_{}", transaction.last_insert_rowid()), "name": name, "path": path, "note": note })
        }
        ProjectCommand::List => {
            let mut statement = transaction.prepare(
                "SELECT id,name,path,note FROM projects WHERE client_id=?1 AND removed_at IS NULL ORDER BY id"
            ).map_err(db_error)?;
            let entries = statement.query_map([owner], |row| {
                Ok(json!({ "project_id":format!("p_{}",row.get::<_, i64>(0)?), "name":row.get::<_,String>(1)?,
                    "path":row.get::<_,String>(2)?, "note":row.get::<_,String>(3)? }))
            }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            json!({ "projects": entries })
        }
        ProjectCommand::Update { project_id, note } => {
            let id = short_id(&project_id, "p_")?;
            let note = text_field(&note, 300, false)?;
            if transaction.execute("UPDATE projects SET note=?3 WHERE id=?1 AND client_id=?2 AND removed_at IS NULL",
                params![id,owner,note]).map_err(db_error)? == 0 {
                return Err("MCP project not found.".into());
            }
            json!({ "project_id":project_id, "note":note })
        }
        ProjectCommand::Remove { project_id } => {
            let id = short_id(&project_id, "p_")?;
            let now = sqlite_time();
            if transaction.execute("UPDATE projects SET removed_at=?3 WHERE id=?1 AND client_id=?2 AND removed_at IS NULL",
                params![id,owner,now]).map_err(db_error)? == 0 {
                return Err("MCP project not found.".into());
            }
            {
                let mut statement = transaction
                    .prepare("SELECT uuid FROM sessions WHERE project_id=?1 AND removed_at IS NULL")
                    .map_err(db_error)?;
                for row in statement
                    .query_map([id], |row| row.get::<_, String>(0))
                    .map_err(db_error)?
                {
                    removed.push(Uuid::parse_str(&row.map_err(db_error)?).map_err(db_error)?);
                }
            }
            transaction
                .execute(
                    "UPDATE sessions SET removed_at=?2 WHERE project_id=?1 AND removed_at IS NULL",
                    params![id, now],
                )
                .map_err(db_error)?;
            json!({ "project_id":project_id, "removed":true })
        }
    };
    transaction.commit().map_err(db_error)?;
    Ok(ManagementResult { output, removed })
}

/// Applies a session command atomically under its owning project.
/// 在所属项目内原子执行会话命令。
fn manage_sessions(
    connection: &mut Connection,
    owner: &str,
    command: SessionCommand,
) -> Result<ManagementResult, String> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let mut removed = Vec::new();
    let output = match command {
        SessionCommand::New { project_id, note } => {
            let id = short_id(&project_id, "p_")?;
            let note = text_field(&note, 300, false)?;
            let uuid = Uuid::new_v4().to_string();
            if transaction.execute(
                "INSERT INTO sessions(project_id,uuid,note) SELECT id,?3,?4 FROM projects WHERE id=?1 AND client_id=?2 AND removed_at IS NULL",
                params![id,owner,uuid,note]).map_err(db_error)? == 0 {
                return Err("MCP project not found.".into());
            }
            json!({ "sid":format!("s_{}", transaction.last_insert_rowid()), "project_id":project_id, "note":note })
        }
        SessionCommand::List { project_id } => {
            let id = project_id
                .as_deref()
                .map(|value| short_id(value, "p_"))
                .transpose()?;
            let mut statement = transaction.prepare(
                "SELECT s.id,p.id,p.name,s.note FROM sessions s JOIN projects p ON p.id=s.project_id
                 WHERE p.client_id=?1 AND p.removed_at IS NULL AND s.removed_at IS NULL
                 AND (?2 IS NULL OR p.id=?2) ORDER BY s.id"
            ).map_err(db_error)?;
            let entries = statement.query_map(params![owner,id], |row| {
                Ok(json!({ "sid":format!("s_{}",row.get::<_, i64>(0)?), "project_id":format!("p_{}",row.get::<_,i64>(1)?),
                    "project_name":row.get::<_,String>(2)?, "note":row.get::<_,String>(3)? }))
            }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            json!({ "sessions":entries })
        }
        SessionCommand::Update { sid, note } => {
            let id = short_id(&sid, "s_")?;
            let note = text_field(&note, 300, false)?;
            if transaction.execute("UPDATE sessions SET note=?3 WHERE id=?1 AND removed_at IS NULL AND project_id IN
                (SELECT id FROM projects WHERE client_id=?2 AND removed_at IS NULL)", params![id,owner,note]).map_err(db_error)? == 0 {
                return Err("MCP session not found.".into());
            }
            json!({ "sid":sid, "note":note })
        }
        SessionCommand::Remove { sid } => {
            let id = short_id(&sid, "s_")?;
            let uuid = transaction.query_row(
                "UPDATE sessions SET removed_at=?3 WHERE id=?1 AND removed_at IS NULL AND project_id IN
                 (SELECT id FROM projects WHERE client_id=?2 AND removed_at IS NULL) RETURNING uuid",
                params![id,owner,sqlite_time()], |row| row.get::<_,String>(0)
            ).optional().map_err(db_error)?.ok_or("MCP session not found.")?;
            removed.push(Uuid::parse_str(&uuid).map_err(db_error)?);
            json!({ "sid":sid, "removed":true })
        }
    };
    transaction.commit().map_err(db_error)?;
    Ok(ManagementResult { output, removed })
}

/// Resolves a live session for tool execution, or a tombstone for an audit lookup.
/// 为工具执行解析活跃会话，或为审计查询解析已移除会话。
pub(super) async fn binding(
    root: &Path,
    client: &str,
    sid: &str,
    history: bool,
) -> Result<Option<SessionBinding>, String> {
    let id = short_id(sid, "s_")?;
    let owner = client.to_owned();
    database(root.join("contexts.sqlite3"), SCHEMA, move |connection| {
        let entry = connection.query_row(
            "SELECT p.id,p.path,s.uuid FROM sessions s JOIN projects p ON p.id=s.project_id WHERE s.id=?1 AND p.client_id=?2
             AND (?3 OR (s.removed_at IS NULL AND p.removed_at IS NULL))",
            params![id,owner,history], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)),
        ).optional().map_err(db_error)?;
        entry.map(|(id,path,uuid)| Ok(SessionBinding {
            project_id: format!("p_{id}"), cwd: PathBuf::from(path), uuid: Uuid::parse_str(&uuid).map_err(db_error)?,
        })).transpose()
    }).await
}

/// Tests project ownership for audit attribution, including retained tombstones.
/// 检查项目是否归该客户端所有以归档审计记录，包含已注销的项目。
pub(super) async fn owns_project(root: &Path, client: &str, id: i64) -> Result<bool, String> {
    let owner = client.to_owned();
    database(root.join("contexts.sqlite3"), SCHEMA, move |connection| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND client_id=?2)",
                params![id, owner],
                |row| row.get(0),
            )
            .map_err(db_error)
    })
    .await
}

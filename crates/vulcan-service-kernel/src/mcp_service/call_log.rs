//! Opt-in audit storage for authenticated MCP tool calls.
//! 已认证 MCP 工具调用的按客户端启用审计存储。

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{path::Path, sync::OnceLock};
use uuid::Uuid;
use vulcan_service_api::McpCallLogQuery;

use super::contexts::{binding, database, db_error, owns_project, short_id, sqlite_time};

/// Redacts registered secrets from a JSON payload without changing its structure.
/// 从载荷的 JSON 字符串中遮蔽已登记的密钥，并保留结构。
pub(super) fn redact_payload(
    value: &mut Value,
    redact: &impl Fn(&str) -> Result<String, String>,
) -> Result<(), String> {
    match value {
        Value::String(text) => *text = redact(text)?,
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|value| redact_payload(value, redact))?,
        Value::Object(fields) => fields
            .values_mut()
            .try_for_each(|value| redact_payload(value, redact))?,
        _ => {}
    }
    Ok(())
}

/// This database is independent of application data and workspace metadata.
/// 此数据库独立于应用数据和工作区元数据。
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  client_id TEXT NOT NULL, client_name TEXT NOT NULL,
  project_id TEXT, sid TEXT, tool TEXT NOT NULL,
  started_at INTEGER NOT NULL, finished_at INTEGER,
  status TEXT NOT NULL, arguments TEXT NOT NULL, result TEXT, process_id TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS mcp_calls_client ON calls(client_id, id);
CREATE INDEX IF NOT EXISTS mcp_calls_project ON calls(project_id, id);
CREATE INDEX IF NOT EXISTS mcp_calls_session ON calls(sid, id);
";

/// Identifies rows that can still receive a response from this Core process.
/// 标识仍可从当前 Core 进程收到响应的记录。
fn process_id() -> &'static str {
    static PROCESS_ID: OnceLock<String> = OnceLock::new();
    PROCESS_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Inserts the call before dispatch; only the tool parameters are retained.
/// 调度前写入调用；只保留工具参数。
pub(super) async fn begin(
    root: &Path,
    client_id: &str,
    client_name: &str,
    parameters: &Value,
    mut logged_parameters: Value,
) -> Result<i64, String> {
    let tool = parameters
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("(invalid)");
    let arguments = &parameters["arguments"];
    let sid = arguments
        .get("sid")
        .and_then(Value::as_str)
        .filter(|sid| short_id(sid, "s_").is_ok());
    let (sid, project_id) = if let Some(sid) = sid {
        match binding(root, client_id, sid, true).await? {
            Some(entry) => (Some(sid.to_owned()), Some(entry.project_id)),
            None => (None, None),
        }
    } else if let Some(project) = arguments
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|_| matches!(tool, "vc_mcp_session" | "vc_mcp_project"))
    {
        let project_id = match short_id(project, "p_") {
            Ok(id) if owns_project(root, client_id, id).await? => Some(project.to_owned()),
            _ => None,
        };
        (None, project_id)
    } else {
        (None, None)
    };
    let tool = logged_parameters["name"]
        .as_str()
        .unwrap_or("(invalid)")
        .to_owned();
    let arguments = logged_parameters
        .get_mut("arguments")
        .map(Value::take)
        .unwrap_or_else(|| json!({}));
    let client_id = client_id.to_owned();
    let client_name = client_name.to_owned();
    let started = sqlite_time();
    database(root.join("calls.sqlite3"), SCHEMA, move |connection| {
        connection.execute(
            "INSERT INTO calls(client_id,client_name,project_id,sid,tool,started_at,status,arguments,process_id)
             VALUES (?1,?2,?3,?4,?5,?6,'running',?7,?8)",
            params![client_id,client_name,project_id,sid,tool,started,arguments.to_string(),process_id()],
        ).map_err(db_error)?;
        Ok(connection.last_insert_rowid())
    }).await
}

/// Adds the final response and attaches IDs returned by creation operations.
/// 补写最终响应，并关联创建操作返回的标识。
pub(super) async fn finish(
    root: &Path,
    id: i64,
    status: &str,
    result: Value,
) -> Result<(), String> {
    let status = status.to_owned();
    let finished = sqlite_time();
    database(root.join("calls.sqlite3"), SCHEMA, move |connection| {
        let data = &result["structuredContent"];
        let project = data["project_id"].as_str().filter(|value| super::contexts::short_id(value,"p_").is_ok());
        let sid = data["sid"].as_str().filter(|value| super::contexts::short_id(value,"s_").is_ok());
        connection.execute(
            "UPDATE calls SET finished_at=?2,status=?3,result=?4,
                project_id=COALESCE(project_id,CASE WHEN tool IN ('vc_mcp_project','vc_mcp_session') THEN ?5 END),
                sid=COALESCE(sid,CASE WHEN tool='vc_mcp_session' THEN ?6 END)
             WHERE id=?1 AND status='running'",
            params![id,finished,status,result.to_string(),project,sid],
        ).map_err(db_error)?;
        Ok(())
    }).await
}

/// Returns one page of summaries, with a stable cursor in the requested order.
/// 按指定排序返回一页摘要及稳定游标。
pub(super) async fn list(root: &Path, query: McpCallLogQuery) -> Result<Value, String> {
    if !root.join("calls.sqlite3").exists() {
        return Ok(json!({"rows":[],"nextCursor":null}));
    }
    const PAGE_SIZE: u32 = 50;
    database(root.join("calls.sqlite3"), SCHEMA, move |connection| {
        let sql = if query.ascending {
            "SELECT id,client_id,client_name,project_id,sid,tool,started_at,finished_at,
                CASE WHEN status='running' AND process_id<>?6 THEN 'interrupted' ELSE status END FROM calls
             WHERE (?1 IS NULL OR client_id=?1) AND (?2 IS NULL OR project_id=?2) AND (?3 IS NULL OR sid=?3)
             AND (?4 IS NULL OR id>?4) ORDER BY id ASC LIMIT ?5"
        } else {
            "SELECT id,client_id,client_name,project_id,sid,tool,started_at,finished_at,
                CASE WHEN status='running' AND process_id<>?6 THEN 'interrupted' ELSE status END FROM calls
             WHERE (?1 IS NULL OR client_id=?1) AND (?2 IS NULL OR project_id=?2) AND (?3 IS NULL OR sid=?3)
             AND (?4 IS NULL OR id<?4) ORDER BY id DESC LIMIT ?5"
        };
        let mut statement = connection.prepare(sql).map_err(db_error)?;
        let mut rows = statement.query_map(params![query.client_id,query.project_id,query.sid,query.cursor,PAGE_SIZE+1,process_id()], |row| {
            Ok(json!({"id":row.get::<_,i64>(0)?,"clientId":row.get::<_,String>(1)?,"clientName":row.get::<_,String>(2)?,
                "projectId":row.get::<_,Option<String>>(3)?,"sid":row.get::<_,Option<String>>(4)?,"tool":row.get::<_,String>(5)?,
                "startedAt":row.get::<_,i64>(6)?,"finishedAt":row.get::<_,Option<i64>>(7)?,"status":row.get::<_,String>(8)?}))
        }).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        let cursor = if rows.len() > PAGE_SIZE as usize {
            rows.truncate(PAGE_SIZE as usize);
            rows.last().and_then(|row|row["id"].as_i64())
        } else { None };
        Ok(json!({"rows":rows,"nextCursor":cursor}))
    }).await
}

/// Downloads a single exact payload only when the local settings UI opens the call.
/// 仅在本地设置界面打开调用详情时读取单条完整载荷。
pub(super) async fn detail(root: &Path, id: i64) -> Result<Value, String> {
    if !root.join("calls.sqlite3").exists() {
        return Err("MCP call not found.".into());
    }
    database(root.join("calls.sqlite3"), SCHEMA, move |connection| {
        let (arguments, result) = connection
            .query_row(
                "SELECT arguments,result FROM calls WHERE id=?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or("MCP call not found.")?;
        let arguments: Value = serde_json::from_str(&arguments).map_err(db_error)?;
        let result: Option<Value> = result
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(db_error)?;
        Ok(json!({"arguments":arguments,"result":result}))
    })
    .await
}

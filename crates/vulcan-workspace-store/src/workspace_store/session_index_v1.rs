//! V1 SQLite session directory index.
//! V1 SQLite 会话目录索引。
//!
//! `session_index_v1` stores only precise directory locators and list-required
//! fields. The unique path function is `configured_storage_root + relative_path`;
//! nothing scans directories or guesses paths. Archive/restore only flip the
//! archived state in SQLite and never move the session directory.
//! `session_index_v1` 只保存精确目录定位与列表必需字段。唯一路径函数是
//! `configured_storage_root + relative_path`；任何代码都不扫描目录或猜测路径。
//! 归档/恢复只翻转 SQLite 中的归档状态，绝不移动会话目录。

use serde::{Deserialize, Serialize};
use vldb_controller_client::ControllerSqliteValue;

use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};

use super::{app_sqlite_binding_id, decode_query_rows};

/// SQL used by the strict V1 session-index insertion.
/// V1 会话索引严格插入使用的 SQL。
const STRICT_INSERT_SESSION_INDEX_V1_SQL: &str = r#"
INSERT INTO session_index_v1 (
    session_id, project_id, project_tag, storage_partition, relative_path,
    title, created_at_ms, updated_at_ms, archived_at_ms, unread
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
"#;

/// The unique index field that rejected one V1 session insertion.
/// 拒绝某次 V1 会话插入的唯一索引字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIndexV1ConflictField {
    /// The globally unique session id already exists.
    /// 全局唯一的会话 ID 已存在。
    SessionId,
    /// The exact relative storage path already belongs to another row.
    /// 精确相对存储路径已属于另一条记录。
    RelativePath,
}

impl SessionIndexV1ConflictField {
    /// Returns the stable machine-readable field name.
    /// 返回稳定的机器可读字段名。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionId => "session_id",
            Self::RelativePath => "relative_path",
        }
    }
}

/// One explicit failure produced by the V1 SQLite locator index.
/// V1 SQLite 定位索引产生的一条显式失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionIndexV1Error {
    /// The schema could not be prepared.
    /// 无法准备索引结构。
    Schema {
        /// Controller or SQLite failure detail.
        /// Controller 或 SQLite 失败详情。
        detail: String,
    },
    /// One stored or proposed locator violates the exact path contract.
    /// 某个已存储或待写入的定位信息违反精确路径契约。
    InvalidLocator {
        /// Session id associated with the locator when known.
        /// 已知时与定位信息关联的会话 ID。
        session_id: Option<String>,
        /// Exact validation failure detail.
        /// 精确的校验失败详情。
        detail: String,
    },
    /// The exact locator row could not be read.
    /// 无法读取精确定位记录。
    Read {
        /// Session id being located when known.
        /// 已知时正在定位的会话 ID。
        session_id: Option<String>,
        /// Controller or SQLite failure detail.
        /// Controller 或 SQLite 失败详情。
        detail: String,
    },
    /// The requested session has no locator row.
    /// 请求的会话没有定位记录。
    Missing {
        /// Missing session id.
        /// 缺失的会话 ID。
        session_id: String,
    },
    /// A strict unique index rejected the insertion.
    /// 严格唯一索引拒绝了插入。
    Conflict {
        /// Unique field that conflicts.
        /// 发生冲突的唯一字段。
        field: SessionIndexV1ConflictField,
        /// Proposed unique value.
        /// 待插入的唯一值。
        value: String,
        /// Existing row that owns the conflicting value.
        /// 已占用冲突值的现有会话 ID。
        existing_session_id: String,
    },
    /// The strict insertion failed without publishing a row.
    /// 严格插入失败且没有发布记录。
    Write {
        /// Session id being inserted.
        /// 正在插入的会话 ID。
        session_id: String,
        /// Controller or SQLite failure detail.
        /// Controller 或 SQLite 失败详情。
        detail: String,
    },
}

impl std::fmt::Display for SessionIndexV1Error {
    /// Formats one stable V1 index diagnostic.
    /// 格式化一条稳定的 V1 索引诊断信息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schema { detail } => {
                write!(formatter, "session_index_v1 schema failed: {detail}")
            }
            Self::InvalidLocator { session_id, detail } => write!(
                formatter,
                "invalid session_index_v1 locator for {}: {detail}",
                session_id.as_deref().unwrap_or("unknown session")
            ),
            Self::Read { session_id, detail } => write!(
                formatter,
                "session_index_v1 read failed for {}: {detail}",
                session_id.as_deref().unwrap_or("unknown session")
            ),
            Self::Missing { session_id } => {
                write!(
                    formatter,
                    "session_index_v1 row is missing for {session_id}"
                )
            }
            Self::Conflict {
                field,
                value,
                existing_session_id,
            } => write!(
                formatter,
                "session_index_v1 {} conflict for {value}; owned by {existing_session_id}",
                field.as_str()
            ),
            Self::Write { session_id, detail } => {
                write!(
                    formatter,
                    "session_index_v1 write failed for {session_id}: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for SessionIndexV1Error {}

/// One V1 session index row.
/// 一条 V1 会话索引行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIndexV1Row {
    /// Cross-system session uuid.
    /// 跨系统会话 uuid。
    pub session_id: String,
    /// Owning project id.
    /// 所属项目 id。
    pub project_id: String,
    /// Human-readable project tag.
    /// 人类可读项目标识。
    pub project_tag: String,
    /// Physical storage partition of the session home.
    /// 会话所在位置的物理存储分区。
    pub storage_partition: String,
    /// Canonical relative path from the configured storage root.
    /// 从配置存储根开始的规范相对路径。
    pub relative_path: String,
    /// Session display title.
    /// 会话显示标题。
    pub title: String,
    /// Creation time in Unix milliseconds.
    /// 创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间(Unix 毫秒)。
    pub updated_at_ms: i64,
    /// Archive time in Unix milliseconds, if archived.
    /// 归档时间(Unix 毫秒)；未归档时为 None。
    pub archived_at_ms: Option<i64>,
    /// Lightweight unread marker for list UI.
    /// 轻量未读标记，仅服务列表界面。
    pub unread: i64,
}

/// Validates one canonical relative path for directory escape and shape.
/// 校验一个规范相对路径是否逃逸目录或形状非法。
///
/// Rejects absolute paths, drive letters, `..` components, backslashes, and
/// other separators so the unique path function can never leave the storage
/// root.
/// 拒绝绝对路径、盘符、`..` 组件、反斜杠与其他分隔符，使唯一路径函数绝不
/// 可能离开存储根。
pub fn validate_relative_path(relative_path: &str) -> Result<(), String> {
    if relative_path.is_empty() {
        return Err("relative_path must not be empty".to_string());
    }
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err("relative_path must be relative".to_string());
    }
    if relative_path.contains('\\') {
        return Err("relative_path must use forward slashes only".to_string());
    }
    let bytes = relative_path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return Err("relative_path must not contain a drive letter".to_string());
    }
    for component in relative_path.split('/') {
        if component.is_empty() {
            return Err("relative_path must not contain empty components".to_string());
        }
        if component == "." || component == ".." {
            return Err("relative_path must not contain dot components".to_string());
        }
        if component.contains(':') {
            return Err("relative_path must not contain colons".to_string());
        }
    }
    Ok(())
}

/// Validates one standalone project partition used below a storage root.
/// 校验一个在存储根下使用的独立项目分区。
///
/// # Parameters / 参数
///
/// - `project_partition`: Exact project directory component.
///   `project_partition`：精确项目目录组件。
///
/// # Returns / 返回值
///
/// Success only for one non-empty, non-escaping path component.
/// 仅当输入是一个非空且不会逃逸的路径组件时返回成功。
pub fn validate_project_partition(project_partition: &str) -> Result<(), String> {
    validate_locator_component("project_partition", project_partition)?;
    validate_relative_path(project_partition)
}

/// Validates one locator component before composing a relative path.
/// 在组合相对路径前校验一个定位器组件。
///
/// # Parameters / 参数
///
/// - `label`: Stable diagnostic field name.
///   `label`：稳定诊断字段名。
/// - `value`: Exact component text.
///   `value`：精确组件文本。
///
/// # Returns / 返回值
///
/// Success when the component is non-empty and separator-free.
/// 当组件非空且不含分隔符时返回成功。
fn validate_locator_component(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value.contains('/') || value.contains('\\') || value.contains(':') {
        return Err(format!(
            "{label} must not contain path separators or colons"
        ));
    }
    Ok(())
}

/// Composes the canonical relative path of one session and validates it.
/// 组合一个会话的规范相对路径并校验它。
pub fn compose_relative_path(
    project_tag: &str,
    date_bucket: &str,
    session_id: &str,
) -> Result<String, String> {
    for (label, value) in [
        ("project_tag", project_tag),
        ("date_bucket", date_bucket),
        ("session_id", session_id),
    ] {
        validate_locator_component(label, value)?;
    }
    let relative_path = format!("{project_tag}/{date_bucket}-{session_id}");
    validate_relative_path(&relative_path)?;
    Ok(relative_path)
}

/// Creates the `session_index_v1` table when missing.
/// 在缺失时创建 `session_index_v1` 表。
pub async fn ensure_session_index_v1_schema(
    controller: &ControllerRuntimeHandle,
) -> Result<(), SessionIndexV1Error> {
    let result = controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
CREATE TABLE IF NOT EXISTS session_index_v1 (
    session_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    project_tag TEXT NOT NULL,
    storage_partition TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    archived_at_ms INTEGER,
    unread INTEGER NOT NULL DEFAULT 0
);
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| SessionIndexV1Error::Schema {
            detail: error.to_string(),
        })?;
    if result.success {
        Ok(())
    } else {
        Err(SessionIndexV1Error::Schema {
            detail: format!("controller returned failure: {}", result.message),
        })
    }
}

/// Inserts one V1 session index row. The session becomes visible only after
/// this insert succeeds (files are published first).
/// 插入一条 V1 会话索引行。只有该插入成功(文件已先发布)后会话才可见。
pub async fn insert_session_index_v1(
    controller: &ControllerRuntimeHandle,
    row: &SessionIndexV1Row,
) -> Result<(), SessionIndexV1Error> {
    let execution = controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            STRICT_INSERT_SESSION_INDEX_V1_SQL,
            vec![vec![
                ControllerSqliteValue::String(row.session_id.clone()),
                ControllerSqliteValue::String(row.project_id.clone()),
                ControllerSqliteValue::String(row.project_tag.clone()),
                ControllerSqliteValue::String(row.storage_partition.clone()),
                ControllerSqliteValue::String(row.relative_path.clone()),
                ControllerSqliteValue::String(row.title.clone()),
                ControllerSqliteValue::Int64(row.created_at_ms),
                ControllerSqliteValue::Int64(row.updated_at_ms),
                row.archived_at_ms
                    .map(ControllerSqliteValue::Int64)
                    .unwrap_or(ControllerSqliteValue::Null),
                ControllerSqliteValue::Int64(row.unread),
            ]],
        )
        .await;
    match execution {
        Ok(result) if result.success && result.rows_changed == 1 => Ok(()),
        Ok(result) => {
            let detail = format!(
                "controller returned success={}, rows_changed={}, message={}",
                result.success, result.rows_changed, result.message
            );
            Err(classify_failed_session_index_v1_insert(controller, row, detail).await)
        }
        Err(error) => {
            Err(classify_failed_session_index_v1_insert(controller, row, error.to_string()).await)
        }
    }
}

/// Validates one proposed row and rejects every existing unique-key owner.
/// 校验一条待写入记录并拒绝所有已存在的唯一键占用者。
pub async fn validate_new_session_index_v1(
    controller: &ControllerRuntimeHandle,
    row: &SessionIndexV1Row,
) -> Result<(), SessionIndexV1Error> {
    validate_relative_path(&row.storage_partition).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: format!("invalid storage_partition: {detail}"),
        }
    })?;
    validate_relative_path(&row.relative_path).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: format!("invalid relative_path: {detail}"),
        }
    })?;
    if let Some(conflict) = find_session_index_v1_conflict(controller, row).await? {
        return Err(conflict);
    }
    Ok(())
}

/// Locates one V1 session row by session id (the exact address function).
/// 按 session id 定位一条 V1 会话行(精确地址函数)。
pub async fn locate_session_index_v1(
    controller: &ControllerRuntimeHandle,
    session_id: &str,
) -> Result<Option<SessionIndexV1Row>, SessionIndexV1Error> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT * FROM session_index_v1 WHERE session_id = ?1;",
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| SessionIndexV1Error::Read {
            session_id: Some(session_id.to_string()),
            detail: error.to_string(),
        })?;
    let rows = decode_query_rows::<SessionIndexV1Row>(&result.json_data).map_err(|detail| {
        SessionIndexV1Error::Read {
            session_id: Some(session_id.to_string()),
            detail,
        }
    })?;
    if result.row_count != rows.len() as u64 || rows.len() > 1 {
        return Err(SessionIndexV1Error::Read {
            session_id: Some(session_id.to_string()),
            detail: format!(
                "locator cardinality mismatch: controller row_count={}, decoded_rows={}",
                result.row_count,
                rows.len()
            ),
        });
    }
    Ok(rows.into_iter().next())
}

/// Finds a strict unique-key conflict for one proposed index row.
/// 查找一条待插入索引记录的严格唯一键冲突。
async fn find_session_index_v1_conflict(
    controller: &ControllerRuntimeHandle,
    row: &SessionIndexV1Row,
) -> Result<Option<SessionIndexV1Error>, SessionIndexV1Error> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT * FROM session_index_v1
WHERE session_id = ?1 OR relative_path = ?2;
"#,
            vec![
                ControllerSqliteValue::String(row.session_id.clone()),
                ControllerSqliteValue::String(row.relative_path.clone()),
            ],
        )
        .await
        .map_err(|error| SessionIndexV1Error::Read {
            session_id: Some(row.session_id.clone()),
            detail: error.to_string(),
        })?;
    let existing_rows =
        decode_query_rows::<SessionIndexV1Row>(&result.json_data).map_err(|detail| {
            SessionIndexV1Error::Read {
                session_id: Some(row.session_id.clone()),
                detail,
            }
        })?;
    Ok(classify_session_index_v1_conflict(row, &existing_rows))
}

/// Classifies exact unique-key conflicts without relying on SQLite error text.
/// 在不依赖 SQLite 错误文本的情况下分类精确唯一键冲突。
fn classify_session_index_v1_conflict(
    proposed: &SessionIndexV1Row,
    existing_rows: &[SessionIndexV1Row],
) -> Option<SessionIndexV1Error> {
    if let Some(existing) = existing_rows
        .iter()
        .find(|existing| existing.session_id == proposed.session_id)
    {
        return Some(SessionIndexV1Error::Conflict {
            field: SessionIndexV1ConflictField::SessionId,
            value: proposed.session_id.clone(),
            existing_session_id: existing.session_id.clone(),
        });
    }
    existing_rows
        .iter()
        .find(|existing| existing.relative_path == proposed.relative_path)
        .map(|existing| SessionIndexV1Error::Conflict {
            field: SessionIndexV1ConflictField::RelativePath,
            value: proposed.relative_path.clone(),
            existing_session_id: existing.session_id.clone(),
        })
}

/// Preserves the primary insert failure while probing for a raced conflict.
/// 在探测并发冲突时保留主要插入失败。
async fn classify_failed_session_index_v1_insert(
    controller: &ControllerRuntimeHandle,
    row: &SessionIndexV1Row,
    write_detail: String,
) -> SessionIndexV1Error {
    match find_session_index_v1_conflict(controller, row).await {
        Ok(Some(conflict)) => conflict,
        Ok(None) => SessionIndexV1Error::Write {
            session_id: row.session_id.clone(),
            detail: write_detail,
        },
        Err(probe_error) => SessionIndexV1Error::Write {
            session_id: row.session_id.clone(),
            detail: format!("{write_detail}; post-failure conflict probe failed: {probe_error}"),
        },
    }
}

/// Lists active V1 sessions without reading any session directory.
/// 列出活动 V1 会话，不读取任何会话目录。
pub async fn list_active_session_index_v1(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<Vec<SessionIndexV1Row>, SessionIndexV1Error> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT * FROM session_index_v1
WHERE archived_at_ms IS NULL AND project_id = ?1
ORDER BY updated_at_ms DESC;
"#,
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| SessionIndexV1Error::Read {
            session_id: None,
            detail: format!("active list query failed for project {project_id}: {error}"),
        })?;
    decode_session_index_v1_list(project_id, "active", result.row_count, &result.json_data)
}

/// Lists archived V1 sessions reading only SQLite rows.
/// 列出归档 V1 会话，只读取 SQLite 行。
pub async fn list_archived_session_index_v1(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<Vec<SessionIndexV1Row>, SessionIndexV1Error> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT * FROM session_index_v1
WHERE archived_at_ms IS NOT NULL AND project_id = ?1
ORDER BY archived_at_ms DESC;
"#,
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| SessionIndexV1Error::Read {
            session_id: None,
            detail: format!("archived list query failed for project {project_id}: {error}"),
        })?;
    decode_session_index_v1_list(project_id, "archived", result.row_count, &result.json_data)
}

/// Deletes one V1 session index row (the directory is deleted first by the
/// caller; a failed row deletion keeps the row for explicit cleanup).
/// 删除一条 V1 会话索引行(目录由调用方先删除；row 删除失败时保留该行供
/// 显式清理)。
pub async fn delete_session_index_v1(
    controller: &ControllerRuntimeHandle,
    session_id: &str,
) -> Result<(), SessionIndexV1Error> {
    execute_session_index_v1_write(
        controller,
        "delete",
        session_id,
        "DELETE FROM session_index_v1 WHERE session_id = ?1;",
        vec![ControllerSqliteValue::String(session_id.to_string())],
        true,
    )
    .await
}

/// Updates the lightweight unread projection for one exact V1 session row.
/// 更新一条精确 V1 会话记录的轻量未读投影。
///
/// # Parameters / 参数
///
/// - `controller`: Controller runtime owning the application SQLite binding.
///   `controller`：持有应用 SQLite 绑定的 Controller 运行时。
/// - `session_id`: Stable session identity selected by the user interface.
///   `session_id`：用户界面选中的稳定会话身份。
/// - `unread`: Exact list-projection value to persist.
///   `unread`：需要持久化的精确列表投影值。
///
/// # Returns / 返回值
///
/// Success only when the exact row accepted the projection; a missing row remains explicit.
/// 仅在精确记录接受投影后返回成功；记录缺失仍会显式报错。
pub async fn set_session_index_unread_v1(
    controller: &ControllerRuntimeHandle,
    session_id: &str,
    unread: bool,
) -> Result<(), SessionIndexV1Error> {
    execute_session_index_v1_write(
        controller,
        "set_unread",
        session_id,
        "UPDATE session_index_v1 SET unread = ?1 WHERE session_id = ?2;",
        vec![
            ControllerSqliteValue::Int64(i64::from(unread)),
            ControllerSqliteValue::String(session_id.to_string()),
        ],
        false,
    )
    .await
}

/// Projects authoritative lifecycle metadata into one exact SQLite locator row.
/// 把权威生命周期 metadata 投影到一条精确 SQLite 定位记录。
///
/// # Parameters / 参数
///
/// - `controller`: Controller runtime owning the application SQLite binding.
///   `controller`：持有应用 SQLite 绑定的 Controller 运行时。
/// - `session_id`: Stable session identity already verified by the V1 owner.
///   `session_id`：已经由 V1 owner 校验的稳定会话身份。
/// - `relative_path`: Immutable exact locator retained by the same owner.
///   `relative_path`：同一 owner 保留的不可变精确定位路径。
/// - `title`, `updated_at_ms`, and `archived_at_ms`: Values read from the durable metadata
///   revision, never from a service request or legacy snapshot.
///   `title`、`updated_at_ms` 与 `archived_at_ms`：从已可靠落盘的 metadata 修订读取，绝不
///   来自服务请求或旧快照。
///
/// # Returns / 返回值
///
/// Success only when the exact session-and-path row accepted the projection.
/// 仅当精确 session 与 path 记录接受该投影时返回成功。
pub async fn project_session_lifecycle_index_v1(
    controller: &ControllerRuntimeHandle,
    session_id: &str,
    relative_path: &str,
    title: &str,
    updated_at_ms: i64,
    archived_at_ms: Option<i64>,
) -> Result<(), SessionIndexV1Error> {
    validate_relative_path(relative_path).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(session_id.to_string()),
            detail,
        }
    })?;
    execute_session_index_v1_write(
        controller,
        "lifecycle_projection",
        session_id,
        r#"
UPDATE session_index_v1
SET title = ?1,
    updated_at_ms = CASE WHEN updated_at_ms > ?2 THEN updated_at_ms ELSE ?2 END,
    archived_at_ms = ?3
WHERE session_id = ?4 AND relative_path = ?5;
"#,
        vec![
            ControllerSqliteValue::String(title.to_string()),
            ControllerSqliteValue::Int64(updated_at_ms),
            archived_at_ms
                .map(ControllerSqliteValue::Int64)
                .unwrap_or(ControllerSqliteValue::Null),
            ControllerSqliteValue::String(session_id.to_string()),
            ControllerSqliteValue::String(relative_path.to_string()),
        ],
        false,
    )
    .await
}

/// Decodes one row-only list and verifies Controller cardinality and project ownership.
/// 解码一个仅含记录的列表，并校验 Controller 基数与项目归属。
fn decode_session_index_v1_list(
    project_id: &str,
    list_kind: &str,
    reported_row_count: u64,
    json_data: &str,
) -> Result<Vec<SessionIndexV1Row>, SessionIndexV1Error> {
    let rows = decode_query_rows::<SessionIndexV1Row>(json_data).map_err(|detail| {
        SessionIndexV1Error::Read {
            session_id: None,
            detail: format!("decode {list_kind} list for project {project_id} failed: {detail}"),
        }
    })?;
    if reported_row_count != rows.len() as u64 {
        return Err(SessionIndexV1Error::Read {
            session_id: None,
            detail: format!(
                "{list_kind} list cardinality mismatch for project {project_id}: controller row_count={reported_row_count}, decoded_rows={}",
                rows.len()
            ),
        });
    }
    if let Some(row) = rows.iter().find(|row| row.project_id != project_id) {
        return Err(SessionIndexV1Error::Read {
            session_id: Some(row.session_id.clone()),
            detail: format!(
                "{list_kind} list returned project_id {}, expected {project_id}",
                row.project_id
            ),
        });
    }
    Ok(rows)
}

/// Executes one checked single-row write; delete alone treats a missing row as success.
/// 执行一次受检单行写入；只有删除把缺失记录视为成功。
async fn execute_session_index_v1_write(
    controller: &ControllerRuntimeHandle,
    operation: &'static str,
    session_id: &str,
    sql: &str,
    params: Vec<ControllerSqliteValue>,
    missing_is_success: bool,
) -> Result<(), SessionIndexV1Error> {
    let result = controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            sql,
            vec![params],
        )
        .await
        .map_err(|error| SessionIndexV1Error::Write {
            session_id: session_id.to_string(),
            detail: format!("{operation} request failed: {error}"),
        })?;
    validate_session_index_v1_write_outcome(
        operation,
        session_id,
        result.success,
        &result.message,
        result.rows_changed,
        missing_is_success,
    )
}

/// Validates one Controller write result without trusting transport success alone.
/// 校验一次 Controller 写入结果，不只信任传输层成功。
fn validate_session_index_v1_write_outcome(
    operation: &'static str,
    session_id: &str,
    success: bool,
    message: &str,
    rows_changed: i64,
    missing_is_success: bool,
) -> Result<(), SessionIndexV1Error> {
    if !success {
        return Err(SessionIndexV1Error::Write {
            session_id: session_id.to_string(),
            detail: format!("{operation} returned failure: {message}"),
        });
    }
    match rows_changed {
        1 => Ok(()),
        0 if missing_is_success => Ok(()),
        0 => Err(SessionIndexV1Error::Missing {
            session_id: session_id.to_string(),
        }),
        rows_changed => Err(SessionIndexV1Error::Write {
            session_id: session_id.to_string(),
            detail: format!(
                "{operation} changed {rows_changed} rows; expected {}",
                if missing_is_success {
                    "zero or one"
                } else {
                    "one"
                }
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one canonical V1 index row for strict-index tests.
    /// 为严格索引测试构建一条规范 V1 索引记录。
    fn index_row(session_id: &str, relative_path: &str) -> SessionIndexV1Row {
        SessionIndexV1Row {
            session_id: session_id.to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-1".to_string(),
            storage_partition: "sessions".to_string(),
            relative_path: relative_path.to_string(),
            title: "title".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            archived_at_ms: None,
            unread: 0,
        }
    }

    /// Relative-path composition round-trips exactly and rejects escapes.
    /// 相对路径组合精确往返，并拒绝逃逸。
    #[test]
    fn relative_path_compose_and_validate() {
        let path = compose_relative_path("project-1", "2026-08-26", "abc-123").unwrap();
        assert_eq!(path, "project-1/2026-08-26-abc-123");
        validate_relative_path(&path).unwrap();
        assert!(compose_relative_path("p", "d", "a/../b").is_err());
        assert!(compose_relative_path("p", "d", "a\\b").is_err());
        assert!(compose_relative_path("p", "d", "a:b").is_err());
        assert!(compose_relative_path("p", "d", "").is_err());
        // A session id of `..` composes into the single component `d-..`,
        // which is a safe (if odd) file name, not a traversal.
        // 纯 `..` 会话 id 组合成单个组件 `d-..`，这是安全(虽怪异)的文件名，
        // 不是目录遍历。
        assert!(compose_relative_path("p", "d", "..").is_ok());
    }

    /// Absolute, drive-letter, and dot-component paths are rejected.
    /// 绝对路径、盘符与点组件路径被拒绝。
    #[test]
    fn relative_path_rejects_escapes() {
        assert!(validate_relative_path("/abs").is_err());
        assert!(validate_relative_path("\\abs").is_err());
        assert!(validate_relative_path("C:/x").is_err());
        assert!(validate_relative_path("a/../b").is_err());
        assert!(validate_relative_path("a/./b").is_err());
        assert!(validate_relative_path("a//b").is_err());
        assert!(validate_relative_path("a\\b").is_err());
    }

    /// Row fields decode exactly from the controller JSON payload shape.
    /// 行字段从控制器 JSON 载荷形态精确解码。
    #[test]
    fn row_decode_round_trip() {
        let rows = vec![SessionIndexV1Row {
            session_id: "s-1".to_string(),
            project_id: "p-1".to_string(),
            project_tag: "project-1".to_string(),
            storage_partition: "sessions".to_string(),
            relative_path: "project-1/2026-08-26-s-1".to_string(),
            title: "title".to_string(),
            created_at_ms: 1,
            updated_at_ms: 2,
            archived_at_ms: None,
            unread: 0,
        }];
        let payload = serde_json::to_string(&rows).unwrap();
        let decoded = decode_query_rows::<SessionIndexV1Row>(&payload).unwrap();
        assert_eq!(decoded, rows);
    }

    /// Location round-trip through locate() semantics returns the same row.
    /// 通过 locate() 语义往返定位返回同一行。
    #[test]
    fn locate_round_trip_preserves_fields() {
        let row = SessionIndexV1Row {
            session_id: "s-2".to_string(),
            project_id: "p-1".to_string(),
            project_tag: "project-1".to_string(),
            storage_partition: "sessions".to_string(),
            relative_path: "project-1/2026-08-26-s-2".to_string(),
            title: "hello".to_string(),
            created_at_ms: 10,
            updated_at_ms: 11,
            archived_at_ms: None,
            unread: 0,
        };
        let payload = serde_json::to_string(&vec![row.clone()]).unwrap();
        let decoded: Vec<SessionIndexV1Row> = decode_query_rows(&payload).unwrap();
        assert_eq!(decoded[0].relative_path, row.relative_path);
        assert_eq!(decoded[0].archived_at_ms, None);
    }

    /// The insert statement is strict and never contains an overwrite branch.
    /// 插入语句是严格插入且绝不包含覆盖分支。
    #[test]
    fn strict_insert_sql_never_overwrites() {
        assert!(STRICT_INSERT_SESSION_INDEX_V1_SQL.contains("INSERT INTO session_index_v1"));
        assert!(!STRICT_INSERT_SESSION_INDEX_V1_SQL.contains("ON CONFLICT"));
        assert!(!STRICT_INSERT_SESSION_INDEX_V1_SQL.contains("UPDATE SET"));
    }

    /// A duplicate session id is reported independently of its proposed path.
    /// 重复会话 ID 会独立于待写入路径被显式报告。
    #[test]
    fn duplicate_session_id_is_explicit() {
        let proposed = index_row("session-1", "project-1/2026-08-27-session-1");
        let existing = index_row("session-1", "project-1/2026-08-26-session-1");

        let conflict = classify_session_index_v1_conflict(&proposed, &[existing])
            .expect("duplicate session id");

        assert_eq!(
            conflict,
            SessionIndexV1Error::Conflict {
                field: SessionIndexV1ConflictField::SessionId,
                value: "session-1".to_string(),
                existing_session_id: "session-1".to_string(),
            }
        );
    }

    /// A relative path owned by another session is reported explicitly.
    /// 已由其他会话占用的相对路径会被显式报告。
    #[test]
    fn duplicate_relative_path_is_explicit() {
        let relative_path = "project-1/2026-08-27-shared";
        let proposed = index_row("session-new", relative_path);
        let existing = index_row("session-existing", relative_path);

        let conflict = classify_session_index_v1_conflict(&proposed, &[existing])
            .expect("duplicate relative path");

        assert_eq!(
            conflict,
            SessionIndexV1Error::Conflict {
                field: SessionIndexV1ConflictField::RelativePath,
                value: relative_path.to_string(),
                existing_session_id: "session-existing".to_string(),
            }
        );
    }

    /// Transport success with zero updated rows is still a missing-row failure.
    /// 传输成功但更新零行仍然是记录缺失失败。
    #[test]
    fn update_requires_one_changed_row() {
        assert_eq!(
            validate_session_index_v1_write_outcome("archive", "session-1", true, "ok", 0, false,),
            Err(SessionIndexV1Error::Missing {
                session_id: "session-1".to_string(),
            })
        );
    }

    /// Deletion alone accepts zero rows while rejecting multi-row corruption.
    /// 只有删除接受零行，同时拒绝多行损坏状态。
    #[test]
    fn delete_is_missing_idempotent_but_never_multi_row() {
        assert_eq!(
            validate_session_index_v1_write_outcome("delete", "session-1", true, "ok", 0, true,),
            Ok(())
        );
        assert!(matches!(
            validate_session_index_v1_write_outcome("delete", "session-1", true, "ok", 2, true,),
            Err(SessionIndexV1Error::Write { .. })
        ));
    }

    /// Controller-declared failure is never accepted as a successful write.
    /// Controller 声明的失败绝不会被接受为成功写入。
    #[test]
    fn controller_failure_is_an_explicit_write_error() {
        assert!(matches!(
            validate_session_index_v1_write_outcome(
                "restore",
                "session-1",
                false,
                "injected transaction failure",
                0,
                false,
            ),
            Err(SessionIndexV1Error::Write { detail, .. })
                if detail.contains("injected transaction failure")
        ));
    }
}

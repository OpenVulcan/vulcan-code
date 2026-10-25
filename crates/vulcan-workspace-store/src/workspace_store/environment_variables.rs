//! Vulcan environment variables: user-defined values injected into the child processes sessions
//! start. Global and project variables live in SQLite; session variables live in a file beside
//! the session document. Stored values are obfuscated so they are not readable at a glance; this
//! is deliberately not encryption, since anything this machine can decrypt, other programs on it
//! can decrypt too.
//! Vulcan 环境变量：注入会话所启动子进程的用户自定义值。全局与项目变量存于 SQLite；会话变量存于
//! 会话文档旁的文件。存储值只做混淆，避免一眼可读；这刻意不是加密：本机能解密的，同机其他程序
//! 同样能解密。

use super::session_sidecars::{load_sidecar, save_sidecar};
use super::*;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rusqlite::OptionalExtension;
use serde::Serialize;

/// Longest accepted variable name, in characters.
/// 可接受的最长变量名（字符数）。
pub const ENVIRONMENT_VARIABLE_NAME_MAX_CHARS: usize = 128;

/// Longest accepted variable value, in bytes.
/// 可接受的最长变量值（字节数）。
pub const ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES: usize = 32 * 1024;

/// Longest accepted variable description, in characters.
/// 可接受的最长变量说明（字符数）。
pub const ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS: usize = 500;

/// Name prefix recommended for Vulcan variables; names without it are still accepted.
/// 推荐的 Vulcan 变量名前缀；不带该前缀的名称同样接受。
pub const RECOMMENDED_ENVIRONMENT_VARIABLE_PREFIX: &str = "VULCAN_ENV_";

/// Variables the host itself writes into child-process environments. No stored variable may use
/// these names. Add a name here whenever the host starts writing another variable into children.
/// 宿主自身写入子进程环境的变量。任何存储变量都不得使用这些名称。宿主每新增一个写入子进程的
/// 变量，都必须同步加入此处。
pub const RESERVED_ENVIRONMENT_VARIABLE_NAMES: &[&str] = &["VULCAN_HOME"];

/// Prefix that marks the current obfuscation format of one stored value.
/// 标记单个存储值当前混淆格式的前缀。
const STORED_VALUE_PREFIX: &str = "v1:";

/// Fixed mask applied before encoding; it only keeps values from being read at a glance.
/// 编码前应用的固定掩码；它只防止值被一眼读出。
const STORED_VALUE_MASK: &[u8] = b"vulcan-code/environment-variables";

/// Version of the session environment file; other versions are refused.
/// 会话环境变量文件的版本；其他版本一律拒绝。
const SESSION_ENVIRONMENT_VERSION: u32 = 1;

/// File name of the session environment inside the session directory.
/// 会话目录中会话环境变量的文件名。
const SESSION_ENVIRONMENT_FILE: &str = "environment.json";

/// Whether one value is shown to the AI.
/// 一个值是否向 AI 展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentVariableKind {
    /// Ordinary value; the AI may read it.
    /// 普通值；AI 可以读取。
    Plain,
    /// Secret value; the AI sees only the name, and outputs replace the value.
    /// 密钥值；AI 只看到名称，输出中的该值会被替换。
    Secret,
}

impl EnvironmentVariableKind {
    /// Stable storage key.
    /// 稳定存储键。
    pub fn key(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Secret => "secret",
        }
    }

    /// Parses one stable storage key.
    /// 解析一个稳定存储键。
    pub fn from_key(key: &str) -> Result<Self, String> {
        match key {
            "plain" => Ok(Self::Plain),
            "secret" => Ok(Self::Secret),
            other => Err(format!("Unknown environment variable kind `{other}`.")),
        }
    }
}

/// Where one stored variable applies.
/// 一个存储变量的生效范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentVariableScope {
    /// Every project and session.
    /// 所有项目与会话。
    Global,
    /// The listed projects and their sessions; never empty.
    /// 所列项目及其会话；绝不为空。
    Projects(Vec<String>),
}

/// One global or project variable as stored.
/// 一条已存储的全局或项目变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentVariableRecord {
    /// Stable identifier.
    /// 稳定标识。
    pub id: String,
    /// Variable name as the user wrote it.
    /// 用户书写的变量名。
    pub name: String,
    /// Revealed value.
    /// 已还原的值。
    pub value: String,
    /// Plain or secret.
    /// 普通或密钥。
    pub kind: EnvironmentVariableKind,
    /// User-facing description; may be empty.
    /// 面向用户的说明；可为空。
    pub description: String,
    /// Where the variable applies.
    /// 变量生效范围。
    pub scope: EnvironmentVariableScope,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最近更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Fields of one variable to create or replace.
/// 待创建或替换的一条变量字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentVariableDraft {
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// Value to store.
    /// 待存储的值。
    pub value: String,
    /// Plain or secret.
    /// 普通或密钥。
    pub kind: EnvironmentVariableKind,
    /// Description; may be empty.
    /// 说明；可为空。
    pub description: String,
    /// Where the variable applies.
    /// 变量生效范围。
    pub scope: EnvironmentVariableScope,
}

/// One session variable as stored beside the session document.
/// 一条保存在会话文档旁的会话变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnvironmentVariable {
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// Revealed value.
    /// 已还原的值。
    pub value: String,
    /// Plain or secret.
    /// 普通或密钥。
    pub kind: EnvironmentVariableKind,
    /// Description; may be empty.
    /// 说明；可为空。
    pub description: String,
    /// Last update time in Unix milliseconds.
    /// 最近更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Which stored layer supplied one effective variable.
/// 一个生效变量来自哪一存储层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentVariableLayer {
    /// The session's own variable.
    /// 会话自有变量。
    Session,
    /// A variable bound to the session's project.
    /// 绑定到会话所属项目的变量。
    Project,
    /// A global variable.
    /// 全局变量。
    Global,
}

impl EnvironmentVariableLayer {
    /// Stable key.
    /// 稳定键。
    pub fn key(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Project => "project",
            Self::Global => "global",
        }
    }
}

/// One variable that applies to a session after scope precedence.
/// 经作用域优先级处理后对一个会话生效的变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveEnvironmentVariable {
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// Revealed value.
    /// 已还原的值。
    pub value: String,
    /// Plain or secret.
    /// 普通或密钥。
    pub kind: EnvironmentVariableKind,
    /// Description; may be empty.
    /// 说明；可为空。
    pub description: String,
    /// Layer that supplied the value.
    /// 提供该值的层。
    pub layer: EnvironmentVariableLayer,
}

/// Session environment file stored beside the session document.
/// 保存在会话文档旁的会话环境变量文件。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionEnvironmentDocument {
    /// File format version.
    /// 文件格式版本。
    version: u32,
    /// Session variables in name order.
    /// 按名称排序的会话变量。
    variables: Vec<SessionEnvironmentEntry>,
}

/// One variable inside the session environment file.
/// 会话环境变量文件中的一条变量。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionEnvironmentEntry {
    /// Variable name.
    /// 变量名。
    name: String,
    /// Obfuscated value.
    /// 已混淆的值。
    stored_value: String,
    /// Plain or secret.
    /// 普通或密钥。
    kind: EnvironmentVariableKind,
    /// Description; may be empty.
    /// 说明；可为空。
    description: String,
    /// Last update time in Unix milliseconds.
    /// 最近更新时间（Unix 毫秒）。
    updated_at_ms: i64,
}

/// Comparison key of one name; environment names are case-insensitive on Windows, so every
/// platform compares them that way to keep one meaning.
/// 一个名称的比较键；Windows 上环境变量名不区分大小写，因此所有平台都按此比较以保持同一语义。
pub fn environment_variable_name_key(name: &str) -> String {
    name.to_ascii_uppercase()
}

/// Validates one variable name: a portable shell identifier that no host-written variable uses.
/// 校验一个变量名：可移植的 shell 标识符，且不与任何宿主写入变量重名。
pub fn validate_environment_variable_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(String::from(
            "The environment variable name must not be empty.",
        ));
    }
    if name.chars().count() > ENVIRONMENT_VARIABLE_NAME_MAX_CHARS {
        return Err(format!(
            "The environment variable name is longer than {ENVIRONMENT_VARIABLE_NAME_MAX_CHARS} characters."
        ));
    }
    let mut characters = name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    if !valid_start || !characters.all(|next| next.is_ascii_alphanumeric() || next == '_') {
        return Err(format!(
            "The environment variable name `{name}` must start with a letter or `_` and contain only letters, digits and `_`."
        ));
    }
    let key = environment_variable_name_key(name);
    if RESERVED_ENVIRONMENT_VARIABLE_NAMES
        .iter()
        .any(|reserved| environment_variable_name_key(reserved) == key)
    {
        return Err(format!(
            "`{name}` is set by Vulcan Code itself and cannot be overridden."
        ));
    }
    Ok(name.to_string())
}

/// Validates one value's size.
/// 校验一个值的大小。
pub fn validate_environment_variable_value(value: &str) -> Result<(), String> {
    if value.len() > ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES {
        return Err(format!(
            "The environment variable value is larger than {ENVIRONMENT_VARIABLE_VALUE_MAX_BYTES} bytes."
        ));
    }
    if value.contains('\0') {
        return Err(String::from(
            "The environment variable value must not contain NUL characters.",
        ));
    }
    Ok(())
}

/// Trims and bounds one description.
/// 去除首尾空白并限制一个说明的长度。
pub fn normalized_environment_variable_description(description: &str) -> Result<String, String> {
    let description = description.trim();
    if description.chars().count() > ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS {
        return Err(format!(
            "The environment variable description is longer than {ENVIRONMENT_VARIABLE_DESCRIPTION_MAX_CHARS} characters."
        ));
    }
    Ok(description.to_string())
}

/// Obfuscates one value for storage.
/// 混淆一个值以便存储。
pub fn obfuscate_environment_value(value: &str) -> String {
    format!(
        "{STORED_VALUE_PREFIX}{}",
        BASE64.encode(masked(value.as_bytes()))
    )
}

/// Reveals one stored value; an unknown format is an error.
/// 还原一个存储值；未知格式返回错误。
pub fn reveal_environment_value(stored: &str) -> Result<String, String> {
    let encoded = stored
        .strip_prefix(STORED_VALUE_PREFIX)
        .ok_or_else(|| String::from("The stored environment value uses an unknown format."))?;
    let bytes = BASE64
        .decode(encoded)
        .map_err(|error| format!("The stored environment value is damaged: {error}"))?;
    String::from_utf8(masked(&bytes))
        .map_err(|error| format!("The stored environment value is damaged: {error}"))
}

/// Applies the fixed mask; applying it twice returns the input.
/// 应用固定掩码；应用两次即还原输入。
fn masked(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .zip(STORED_VALUE_MASK.iter().cycle())
        .map(|(byte, mask)| byte ^ mask)
        .collect()
}

/// Validated draft ready for SQL.
/// 已校验、可写入 SQL 的草稿。
struct ValidatedDraft {
    name: String,
    name_key: String,
    stored_value: String,
    kind: EnvironmentVariableKind,
    description: String,
    project_ids: Option<Vec<String>>,
}

/// Validates one draft.
/// 校验一份草稿。
fn validate_draft(draft: EnvironmentVariableDraft) -> Result<ValidatedDraft, String> {
    let name = validate_environment_variable_name(&draft.name)?;
    validate_environment_variable_value(&draft.value)?;
    let description = normalized_environment_variable_description(&draft.description)?;
    let project_ids = match draft.scope {
        EnvironmentVariableScope::Global => None,
        EnvironmentVariableScope::Projects(project_ids) => {
            let project_ids = project_ids
                .into_iter()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect::<BTreeSet<_>>();
            if project_ids.is_empty() {
                return Err(String::from(
                    "A project environment variable needs at least one project.",
                ));
            }
            Some(project_ids.into_iter().collect())
        }
    };
    Ok(ValidatedDraft {
        name_key: environment_variable_name_key(&name),
        name,
        stored_value: obfuscate_environment_value(&draft.value),
        kind: draft.kind,
        description,
        project_ids,
    })
}

/// Lists every global and project variable in name order.
/// 按名称顺序列出全部全局与项目变量。
pub async fn list_environment_variables(
    host_runtime: &HostRuntimeHandle,
) -> Result<Vec<EnvironmentVariableRecord>, String> {
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        |connection| read_all_records(connection),
    )
    .await
}

/// Creates one variable; a name already used in the same scope is refused.
/// 创建一条变量；同一范围内已使用的名称会被拒绝。
pub async fn create_environment_variable(
    host_runtime: &HostRuntimeHandle,
    draft: EnvironmentVariableDraft,
) -> Result<EnvironmentVariableRecord, String> {
    let draft = validate_draft(draft)?;
    let id = uuid::Uuid::new_v4().to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let transaction = connection.transaction().map_err(sql_error)?;
            ensure_name_free(&transaction, &draft, None)?;
            let now = current_unix_millis();
            transaction
                .execute(
                    "INSERT INTO environment_variables (id, name, name_key, stored_value, kind, scope, description, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    rusqlite::params![
                        id,
                        draft.name,
                        draft.name_key,
                        draft.stored_value,
                        draft.kind.key(),
                        scope_key(&draft),
                        draft.description,
                        now
                    ],
                )
                .map_err(sql_error)?;
            write_bindings(&transaction, &id, draft.project_ids.as_deref())?;
            let record = read_record(&transaction, &id)?
                .ok_or_else(|| String::from("The created environment variable disappeared."))?;
            transaction.commit().map_err(sql_error)?;
            Ok(record)
        },
    )
    .await
}

/// Replaces every field of one variable; returns `None` when it does not exist.
/// 替换一条变量的全部字段；变量不存在时返回 `None`。
pub async fn update_environment_variable(
    host_runtime: &HostRuntimeHandle,
    id: &str,
    draft: EnvironmentVariableDraft,
) -> Result<Option<EnvironmentVariableRecord>, String> {
    let draft = validate_draft(draft)?;
    let id = id.to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let transaction = connection.transaction().map_err(sql_error)?;
            ensure_name_free(&transaction, &draft, Some(&id))?;
            let changed = transaction
                .execute(
                    "UPDATE environment_variables SET name = ?2, name_key = ?3, stored_value = ?4, kind = ?5, scope = ?6, description = ?7, updated_at_ms = ?8 WHERE id = ?1",
                    rusqlite::params![
                        id,
                        draft.name,
                        draft.name_key,
                        draft.stored_value,
                        draft.kind.key(),
                        scope_key(&draft),
                        draft.description,
                        current_unix_millis()
                    ],
                )
                .map_err(sql_error)?;
            if changed == 0 {
                return Ok(None);
            }
            transaction
                .execute(
                    "DELETE FROM environment_variable_project_bindings WHERE variable_id = ?1",
                    [&id],
                )
                .map_err(sql_error)?;
            write_bindings(&transaction, &id, draft.project_ids.as_deref())?;
            let record = read_record(&transaction, &id)?;
            transaction.commit().map_err(sql_error)?;
            Ok(record)
        },
    )
    .await
}

/// Deletes one variable; returns whether it existed.
/// 删除一条变量；返回其是否存在。
pub async fn delete_environment_variable(
    host_runtime: &HostRuntimeHandle,
    id: &str,
) -> Result<bool, String> {
    let id = id.to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            connection
                .execute("DELETE FROM environment_variables WHERE id = ?1", [&id])
                .map(|changed| changed > 0)
                .map_err(sql_error)
        },
    )
    .await
}

/// Stores one value under a name in the global layer or for one project: the variable that
/// already holds the name there gets the new value and keeps its other fields; otherwise a new
/// variable is created with `kind` and `description`.
/// 将一个值以某名称存入全局层或某一项目：该处已持有此名称的变量获得新值并保留其他字段；否则以
/// `kind` 与 `description` 新建变量。
pub async fn store_environment_variable_value(
    host_runtime: &HostRuntimeHandle,
    project_id: Option<&str>,
    name: &str,
    value: &str,
    kind: EnvironmentVariableKind,
    description: &str,
) -> Result<EnvironmentVariableRecord, String> {
    let draft = validate_draft(EnvironmentVariableDraft {
        name: name.to_string(),
        value: value.to_string(),
        kind,
        description: description.to_string(),
        scope: match project_id {
            Some(project_id) => EnvironmentVariableScope::Projects(vec![project_id.to_string()]),
            None => EnvironmentVariableScope::Global,
        },
    })?;
    let new_id = uuid::Uuid::new_v4().to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let transaction = connection.transaction().map_err(sql_error)?;
            let existing = holder_of_name(&transaction, &draft)?;
            let now = current_unix_millis();
            let id = match existing {
                Some(id) => {
                    transaction
                        .execute(
                            "UPDATE environment_variables SET stored_value = ?2, updated_at_ms = ?3 WHERE id = ?1",
                            rusqlite::params![id, draft.stored_value, now],
                        )
                        .map_err(sql_error)?;
                    id
                }
                None => {
                    transaction
                        .execute(
                            "INSERT INTO environment_variables (id, name, name_key, stored_value, kind, scope, description, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                            rusqlite::params![
                                new_id,
                                draft.name,
                                draft.name_key,
                                draft.stored_value,
                                draft.kind.key(),
                                scope_key(&draft),
                                draft.description,
                                now
                            ],
                        )
                        .map_err(sql_error)?;
                    write_bindings(&transaction, &new_id, draft.project_ids.as_deref())?;
                    new_id
                }
            };
            let record = read_record(&transaction, &id)?
                .ok_or_else(|| String::from("The stored environment variable disappeared."))?;
            transaction.commit().map_err(sql_error)?;
            Ok(record)
        },
    )
    .await
}

/// Storage key of the draft's scope.
/// 草稿作用域的存储键。
fn scope_key(draft: &ValidatedDraft) -> &'static str {
    if draft.project_ids.is_some() {
        "project"
    } else {
        "global"
    }
}

/// Refuses a name another variable already uses in the same scope: among global variables, or
/// among the variables of any one of the draft's projects.
/// 拒绝同一范围内已被其他变量使用的名称：全局变量之间，或草稿任一项目的变量之间。
fn ensure_name_free(
    connection: &rusqlite::Connection,
    draft: &ValidatedDraft,
    own_id: Option<&str>,
) -> Result<(), String> {
    let own_id = own_id.unwrap_or("");
    match &draft.project_ids {
        None => {
            let taken = connection
                .query_row(
                    "SELECT name FROM environment_variables WHERE scope = 'global' AND name_key = ?1 AND id != ?2",
                    rusqlite::params![draft.name_key, own_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sql_error)?;
            if let Some(taken) = taken {
                return Err(format!(
                    "A global environment variable named `{taken}` already exists."
                ));
            }
        }
        Some(project_ids) => {
            for project_id in project_ids {
                let taken = connection
                    .query_row(
                        "SELECT variable.name FROM environment_variables variable JOIN environment_variable_project_bindings binding ON binding.variable_id = variable.id WHERE binding.project_id = ?1 AND variable.name_key = ?2 AND variable.id != ?3",
                        rusqlite::params![project_id, draft.name_key, own_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                if let Some(taken) = taken {
                    return Err(format!(
                        "Project `{project_id}` already has an environment variable named `{taken}`."
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Variable that holds the draft's name in its single target scope.
/// 在草稿唯一目标范围内持有该名称的变量。
fn holder_of_name(
    connection: &rusqlite::Connection,
    draft: &ValidatedDraft,
) -> Result<Option<String>, String> {
    match draft.project_ids.as_deref() {
        None => connection
            .query_row(
                "SELECT id FROM environment_variables WHERE scope = 'global' AND name_key = ?1",
                [&draft.name_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error),
        Some([project_id]) => connection
            .query_row(
                "SELECT variable.id FROM environment_variables variable JOIN environment_variable_project_bindings binding ON binding.variable_id = variable.id WHERE binding.project_id = ?1 AND variable.name_key = ?2",
                rusqlite::params![project_id, draft.name_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error),
        Some(_) => Err(String::from(
            "A stored value targets exactly one project.",
        )),
    }
}

/// Writes the project bindings of one variable.
/// 写入一条变量的项目绑定。
fn write_bindings(
    connection: &rusqlite::Connection,
    id: &str,
    project_ids: Option<&[String]>,
) -> Result<(), String> {
    for project_id in project_ids.unwrap_or_default() {
        connection
            .execute(
                "INSERT INTO environment_variable_project_bindings (variable_id, project_id) VALUES (?1, ?2)",
                rusqlite::params![id, project_id],
            )
            .map_err(|error| {
                format!("Failed to bind the environment variable to project `{project_id}`: {error}")
            })?;
    }
    Ok(())
}

/// Columns of one variable row in select order.
/// 一条变量行按查询顺序排列的列。
const RECORD_COLUMNS: &str =
    "id, name, stored_value, kind, scope, description, created_at_ms, updated_at_ms";

/// Reads every variable with its bindings.
/// 读取全部变量及其绑定。
fn read_all_records(
    connection: &rusqlite::Connection,
) -> Result<Vec<EnvironmentVariableRecord>, String> {
    let mut bindings = BTreeMap::<String, Vec<String>>::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT variable_id, project_id FROM environment_variable_project_bindings ORDER BY project_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (variable_id, project_id) = row.map_err(sql_error)?;
            bindings.entry(variable_id).or_default().push(project_id);
        }
    }
    let mut statement = connection
        .prepare(&format!(
            "SELECT {RECORD_COLUMNS} FROM environment_variables ORDER BY name_key, scope, id"
        ))
        .map_err(sql_error)?;
    let rows = statement.query_map([], raw_row).map_err(sql_error)?;
    let mut records = Vec::new();
    for row in rows {
        let row = row.map_err(sql_error)?;
        let projects = bindings.remove(&row.id).unwrap_or_default();
        records.push(record_from_raw(row, projects)?);
    }
    Ok(records)
}

/// Reads one variable with its bindings.
/// 读取一条变量及其绑定。
fn read_record(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<Option<EnvironmentVariableRecord>, String> {
    let Some(row) = connection
        .query_row(
            &format!("SELECT {RECORD_COLUMNS} FROM environment_variables WHERE id = ?1"),
            [id],
            raw_row,
        )
        .optional()
        .map_err(sql_error)?
    else {
        return Ok(None);
    };
    let mut statement = connection
        .prepare(
            "SELECT project_id FROM environment_variable_project_bindings WHERE variable_id = ?1 ORDER BY project_id",
        )
        .map_err(sql_error)?;
    let projects = statement
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    record_from_raw(row, projects).map(Some)
}

/// One variable row before decoding.
/// 解码前的一条变量行。
struct RawRecord {
    id: String,
    name: String,
    stored_value: String,
    kind: String,
    scope: String,
    description: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

/// Maps one SQL row in [`RECORD_COLUMNS`] order.
/// 按 [`RECORD_COLUMNS`] 顺序映射一条 SQL 行。
fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRecord> {
    Ok(RawRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        stored_value: row.get(2)?,
        kind: row.get(3)?,
        scope: row.get(4)?,
        description: row.get(5)?,
        created_at_ms: row.get(6)?,
        updated_at_ms: row.get(7)?,
    })
}

/// Decodes one raw row; a project variable without projects is damaged data.
/// 解码一条原始行；没有项目的项目变量属于损坏数据。
fn record_from_raw(
    row: RawRecord,
    projects: Vec<String>,
) -> Result<EnvironmentVariableRecord, String> {
    let scope = match row.scope.as_str() {
        "global" => EnvironmentVariableScope::Global,
        "project" if !projects.is_empty() => EnvironmentVariableScope::Projects(projects),
        "project" => {
            return Err(format!(
                "The project environment variable `{}` has no projects.",
                row.name
            ));
        }
        other => return Err(format!("Unknown environment variable scope `{other}`.")),
    };
    Ok(EnvironmentVariableRecord {
        value: reveal_environment_value(&row.stored_value)
            .map_err(|error| format!("Environment variable `{}`: {error}", row.name))?,
        kind: EnvironmentVariableKind::from_key(&row.kind)?,
        id: row.id,
        name: row.name,
        description: row.description,
        scope,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

/// Renders one SQL failure.
/// 渲染一次 SQL 失败。
fn sql_error(error: rusqlite::Error) -> String {
    format!("Environment variable storage failed: {error}")
}

/// Loads the session's own variables; an absent file means none.
/// 加载会话自有变量；文件缺失表示没有。
pub fn load_session_environment(
    locator: &SessionV1StorageLocator,
) -> Result<Vec<SessionEnvironmentVariable>, String> {
    let Some(document) =
        load_sidecar::<SessionEnvironmentDocument>(locator, SESSION_ENVIRONMENT_FILE)?
    else {
        return Ok(Vec::new());
    };
    if document.version != SESSION_ENVIRONMENT_VERSION {
        return Err(format!(
            "Session environment version {} is not supported.",
            document.version
        ));
    }
    document
        .variables
        .into_iter()
        .map(|entry| {
            Ok(SessionEnvironmentVariable {
                value: reveal_environment_value(&entry.stored_value)
                    .map_err(|error| format!("Session variable `{}`: {error}", entry.name))?,
                name: entry.name,
                kind: entry.kind,
                description: entry.description,
                updated_at_ms: entry.updated_at_ms,
            })
        })
        .collect()
}

/// Saves the session's own variables; an empty list removes the file. Names are validated and
/// must be distinct.
/// 保存会话自有变量；空列表删除该文件。名称经校验且必须互不相同。
pub fn save_session_environment(
    locator: &SessionV1StorageLocator,
    variables: &[SessionEnvironmentVariable],
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(variables.len());
    for variable in variables {
        let name = validate_environment_variable_name(&variable.name)?;
        validate_environment_variable_value(&variable.value)?;
        if !seen.insert(environment_variable_name_key(&name)) {
            return Err(format!(
                "The session already has an environment variable named `{name}`."
            ));
        }
        entries.push(SessionEnvironmentEntry {
            name,
            stored_value: obfuscate_environment_value(&variable.value),
            kind: variable.kind,
            description: normalized_environment_variable_description(&variable.description)?,
            updated_at_ms: variable.updated_at_ms,
        });
    }
    entries.sort_by_key(|entry| environment_variable_name_key(&entry.name));
    let document = (!entries.is_empty()).then_some(SessionEnvironmentDocument {
        version: SESSION_ENVIRONMENT_VERSION,
        variables: entries,
    });
    save_sidecar(
        locator,
        SESSION_ENVIRONMENT_FILE,
        "session environment",
        document.as_ref(),
    )
}

/// Merges the stored layers for one session: session over project over global. Names compare
/// case-insensitively; the result is in name order.
/// 合并一个会话的各存储层：会话优先于项目，项目优先于全局。名称不区分大小写比较；结果按名称排序。
pub fn resolve_effective_environment(
    records: &[EnvironmentVariableRecord],
    project_id: Option<&str>,
    session: &[SessionEnvironmentVariable],
) -> Vec<EffectiveEnvironmentVariable> {
    let mut effective = BTreeMap::<String, EffectiveEnvironmentVariable>::new();
    let mut place = |name: &str,
                     value: &str,
                     kind: EnvironmentVariableKind,
                     description: &str,
                     layer: EnvironmentVariableLayer| {
        effective.insert(
            environment_variable_name_key(name),
            EffectiveEnvironmentVariable {
                name: name.to_string(),
                value: value.to_string(),
                kind,
                description: description.to_string(),
                layer,
            },
        );
    };
    for record in records {
        if record.scope == EnvironmentVariableScope::Global {
            place(
                &record.name,
                &record.value,
                record.kind,
                &record.description,
                EnvironmentVariableLayer::Global,
            );
        }
    }
    for record in records {
        if let EnvironmentVariableScope::Projects(projects) = &record.scope
            && project_id.is_some_and(|project_id| projects.iter().any(|id| id == project_id))
        {
            place(
                &record.name,
                &record.value,
                record.kind,
                &record.description,
                EnvironmentVariableLayer::Project,
            );
        }
    }
    for variable in session {
        place(
            &variable.name,
            &variable.value,
            variable.kind,
            &variable.description,
            EnvironmentVariableLayer::Session,
        );
    }
    effective.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// Opens an in-memory database with the production schema and two projects.
    /// 打开带生产结构与两个项目的内存数据库。
    fn fixture() -> rusqlite::Connection {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&crate::data_plane::build_application_schema_script())
            .unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 INSERT INTO projects (id, name, root_path, created_at, updated_at) VALUES ('one', 'one', '/one', '0', '0'), ('two', 'two', '/two', '0', '0');",
            )
            .unwrap();
        connection
    }

    /// Inserts one validated draft the way the async entry does.
    /// 以异步入口相同的方式插入一份已校验草稿。
    fn insert(
        connection: &mut rusqlite::Connection,
        draft: EnvironmentVariableDraft,
    ) -> Result<EnvironmentVariableRecord, String> {
        let draft = validate_draft(draft)?;
        let id = uuid::Uuid::new_v4().to_string();
        let transaction = connection.transaction().map_err(sql_error)?;
        ensure_name_free(&transaction, &draft, None)?;
        transaction
            .execute(
                "INSERT INTO environment_variables (id, name, name_key, stored_value, kind, scope, description, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 1)",
                rusqlite::params![
                    id,
                    draft.name,
                    draft.name_key,
                    draft.stored_value,
                    draft.kind.key(),
                    scope_key(&draft),
                    draft.description
                ],
            )
            .map_err(sql_error)?;
        write_bindings(&transaction, &id, draft.project_ids.as_deref())?;
        let record = read_record(&transaction, &id)?.unwrap();
        transaction.commit().map_err(sql_error)?;
        Ok(record)
    }

    fn draft(name: &str, value: &str, scope: EnvironmentVariableScope) -> EnvironmentVariableDraft {
        EnvironmentVariableDraft {
            name: name.to_string(),
            value: value.to_string(),
            kind: EnvironmentVariableKind::Plain,
            description: String::new(),
            scope,
        }
    }

    fn projects(ids: &[&str]) -> EnvironmentVariableScope {
        EnvironmentVariableScope::Projects(ids.iter().map(|id| id.to_string()).collect())
    }

    /// Stored values round-trip and are not stored as plain text.
    /// 存储值可往返还原，且不以明文存储。
    #[test]
    fn stored_values_round_trip_without_plain_text() {
        for value in ["", "token-123", "多字节 value", "a\nb"] {
            let stored = obfuscate_environment_value(value);
            assert!(value.is_empty() || !stored.contains(value));
            assert_eq!(reveal_environment_value(&stored).as_deref(), Ok(value));
        }
        assert!(reveal_environment_value("token-123").is_err());
        assert!(reveal_environment_value("v1:%%%").is_err());
    }

    /// Names are portable identifiers, and host-written names are refused in any case.
    /// 名称必须是可移植标识符，宿主写入的名称不论大小写一律拒绝。
    #[test]
    fn names_are_portable_and_reserved_names_are_refused() {
        assert_eq!(
            validate_environment_variable_name(" VULCAN_ENV_TOKEN ").as_deref(),
            Ok("VULCAN_ENV_TOKEN")
        );
        assert!(validate_environment_variable_name("_x9").is_ok());
        for invalid in ["", "9X", "A-B", "A B", "名"] {
            assert!(
                validate_environment_variable_name(invalid).is_err(),
                "{invalid}"
            );
        }
        for reserved in RESERVED_ENVIRONMENT_VARIABLE_NAMES {
            assert!(validate_environment_variable_name(reserved).is_err());
            assert!(validate_environment_variable_name(&reserved.to_ascii_lowercase()).is_err());
        }
    }

    /// A name is unique among global variables and within each project, case-insensitively,
    /// while a project variable may share a global variable's name.
    /// 名称在全局变量之间以及每个项目之内唯一（不区分大小写）；项目变量可以与全局变量同名。
    #[test]
    fn names_are_unique_per_scope() {
        let mut connection = fixture();
        insert(
            &mut connection,
            draft("TOKEN", "g", EnvironmentVariableScope::Global),
        )
        .unwrap();
        assert!(
            insert(
                &mut connection,
                draft("token", "g2", EnvironmentVariableScope::Global)
            )
            .is_err()
        );
        insert(&mut connection, draft("TOKEN", "p", projects(&["one"]))).unwrap();
        assert!(
            insert(
                &mut connection,
                draft("Token", "p2", projects(&["two", "one"]))
            )
            .is_err()
        );
        insert(&mut connection, draft("TOKEN", "p3", projects(&["two"]))).unwrap();
        assert!(insert(&mut connection, draft("EMPTY", "x", projects(&[]))).is_err());
        assert!(
            insert(
                &mut connection,
                draft("MISSING", "x", projects(&["absent"]))
            )
            .is_err()
        );
    }

    /// Session overrides project, which overrides global; other projects' variables never apply.
    /// 会话覆盖项目，项目覆盖全局；其他项目的变量绝不生效。
    #[test]
    fn effective_environment_follows_scope_precedence() {
        let mut connection = fixture();
        insert(
            &mut connection,
            draft("A", "global", EnvironmentVariableScope::Global),
        )
        .unwrap();
        insert(
            &mut connection,
            draft("B", "global", EnvironmentVariableScope::Global),
        )
        .unwrap();
        insert(
            &mut connection,
            draft("C", "global", EnvironmentVariableScope::Global),
        )
        .unwrap();
        insert(
            &mut connection,
            draft("b", "project", projects(&["one", "two"])),
        )
        .unwrap();
        insert(&mut connection, draft("C", "other", projects(&["two"]))).unwrap();
        let records = read_all_records(&connection).unwrap();
        let session = [SessionEnvironmentVariable {
            name: String::from("a"),
            value: String::from("session"),
            kind: EnvironmentVariableKind::Secret,
            description: String::new(),
            updated_at_ms: 1,
        }];
        let effective = resolve_effective_environment(&records, Some("one"), &session)
            .into_iter()
            .map(|variable| (variable.name, variable.value, variable.layer))
            .collect::<Vec<_>>();
        assert_eq!(
            effective,
            vec![
                (
                    String::from("a"),
                    String::from("session"),
                    EnvironmentVariableLayer::Session
                ),
                (
                    String::from("b"),
                    String::from("project"),
                    EnvironmentVariableLayer::Project
                ),
                (
                    String::from("C"),
                    String::from("global"),
                    EnvironmentVariableLayer::Global
                ),
            ]
        );
        assert_eq!(resolve_effective_environment(&records, None, &[]).len(), 3);
    }

    /// Deleting a project removes its bindings; the variable reads back as damaged rather than
    /// silently becoming global.
    /// 删除项目会移除其绑定；变量读回时报告损坏，而不是静默变为全局。
    #[test]
    fn project_deletion_never_turns_a_variable_global() {
        let mut connection = fixture();
        insert(&mut connection, draft("A", "x", projects(&["one", "two"]))).unwrap();
        connection
            .execute("DELETE FROM projects WHERE id = 'one'", [])
            .unwrap();
        let records = read_all_records(&connection).unwrap();
        assert_eq!(records[0].scope, projects(&["two"]));
        connection
            .execute("DELETE FROM projects WHERE id = 'two'", [])
            .unwrap();
        assert!(read_all_records(&connection).is_err());
    }

    /// The session file round-trips, stores no plain values, refuses duplicates, and an empty
    /// list removes it.
    /// 会话文件可往返，不存明文值，拒绝重名，空列表删除文件。
    #[test]
    fn session_environment_round_trips() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "session-environment", 1).unwrap();
        assert!(load_session_environment(&locator).unwrap().is_empty());
        let variable = SessionEnvironmentVariable {
            name: String::from("VULCAN_ENV_TOKEN"),
            value: String::from("secret-value"),
            kind: EnvironmentVariableKind::Secret,
            description: String::from("token"),
            updated_at_ms: 7,
        };
        save_session_environment(&locator, std::slice::from_ref(&variable)).unwrap();
        let path = locator.session_directory().join(SESSION_ENVIRONMENT_FILE);
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("secret-value")
        );
        assert_eq!(
            load_session_environment(&locator).unwrap(),
            vec![variable.clone()]
        );
        let mut duplicate = variable.clone();
        duplicate.name = String::from("vulcan_env_token");
        assert!(save_session_environment(&locator, &[variable, duplicate]).is_err());
        save_session_environment(&locator, &[]).unwrap();
        assert!(!path.exists());
    }
}

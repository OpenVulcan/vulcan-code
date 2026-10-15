//! Offline schema synchronization, deliberately excluded from the application startup path.
//! 离线结构同步，刻意不进入应用启动路径。

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use vulcan_workspace_store::data_plane::{
    APP_SCHEMA_VERSION, application_sqlite_path, build_application_schema_script,
};

/// Maximum wait for an existing writer or reader; no busy retry can wait indefinitely.
/// 等待既有写入者或读取者的最长时间；锁重试不会无限等待。
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// One actual SQLite object; SQL is obtained from an initialized authoritative schema.
/// 一个实际 SQLite 对象；SQL 来自已初始化的权威结构。
#[derive(Clone, PartialEq, Eq)]
struct SchemaObject {
    /// SQLite object category, such as table or index.
    /// SQLite 对象类别，例如表或索引。
    kind: String,
    /// Owning table reported by SQLite.
    /// SQLite 报告的所属表。
    table: String,
    /// SQLite's stored creation statement, never reconstructed from source text.
    /// SQLite 保存的创建语句，绝不从源码文本重新解析。
    sql: String,
}

/// Planned table rebuilds and index updates with exact retained column mappings.
/// 计划重建的表、更新的索引及精确保留的列映射。
#[derive(Default)]
struct Changes {
    /// Explicitly retired tables whose original rows remain available only in the verified backup.
    /// 明确退役的表，其原始记录仅保留在已经验证的备份中。
    retired_tables: Vec<String>,
    /// Existing or new tables whose authoritative definition differs.
    /// 与权威定义不同的既有表或新表。
    tables: Vec<String>,
    /// Authoritative indexes that require creation or replacement.
    /// 需要创建或替换的权威索引。
    indexes: Vec<String>,
    /// Target columns for every managed table, in declared order.
    /// 全部受管表的目标列，按声明顺序排列。
    columns: BTreeMap<String, Vec<String>>,
}

/// Read CLI mode, derive the sole Debug target, and report only paths and object names.
/// 读取命令模式、派生唯一 Debug 目标，并仅报告路径及对象名。
/// No parameters; returns an error for invalid arguments, absent files, drift, or failed synchronization.
/// 无参数；参数非法、文件缺失、存在差异或同步失败时返回错误。
pub(super) fn run() -> Result<(), String> {
    // arguments never accept an alternate path that could select a Release database.
    // arguments 不接受可能选中 Release 数据库的替代路径。
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    // apply requires the explicit mutation flag; check remains read-only.
    // apply 需要显式修改标志；check 保持只读。
    let apply = match arguments.as_slice() {
        [argument] if argument == "--apply" => true,
        [argument] if argument == "--check" => false,
        _ => return Err("Usage: vulcan-debug-db --check | --apply".into()),
    };
    // path is resolved through the same authority as the application bootstrap.
    // path 通过与应用引导相同的权威入口解析。
    let path = application_sqlite_path();
    reject_redirected_path(&path)?;
    println!("Debug database: {}", path.display());
    match synchronize(&path, apply)? {
        Some(backup) => println!(
            "Synchronized schema v{APP_SCHEMA_VERSION}; all retained values verified. Backup: {}",
            backup.display()
        ),
        None => println!("Schema v{APP_SCHEMA_VERSION} is current; no changes."),
    }
    Ok(())
}

/// Reject links and directory junctions along the derived Debug path before opening it.
/// 打开前拒绝派生 Debug 路径沿途的符号链接及目录联接。
/// `path` must already exist; returns an error rather than following a redirected user directory.
/// `path` 必须已存在；遇到重定向用户目录时返回错误，而非继续跟随。
fn reject_redirected_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        // metadata inspects the link itself, without following its destination.
        // metadata 检查链接本身，不跟随其目标。
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("The Debug database path must not contain symbolic links.".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("The Debug database path must not contain reparse points.".into());
            }
        }
    }
    if !path.is_file() {
        return Err("The existing Debug application database is required.".into());
    }
    Ok(())
}

/// Synchronize an existing database against freshly initialized production DDL.
/// 将既有数据库同步到刚刚初始化的生产 DDL。
/// `path` is the validated target, `apply` enables mutation; returns a retained backup only after success.
/// `path` 为已验证目标，`apply` 启用修改；仅在成功修改后返回保留的备份路径。
fn synchronize(path: &Path, apply: bool) -> Result<Option<PathBuf>, String> {
    // expected executes the production generator, keeping DDL as a single source of truth.
    // expected 执行生产生成器，保持 DDL 的唯一权威来源。
    let expected = sql(Connection::open_in_memory())?;
    sql(expected.execute_batch(&build_application_schema_script()))?;
    // flags never create a missing user database and keep inspection read-only.
    // flags 绝不创建缺失的用户数据库，并让检查模式保持只读。
    let flags = if apply {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    // connection holds the write reservation throughout planning, backup and verification.
    // connection 在规划、备份和验证全过程持有写入保留锁。
    let mut connection = sql(Connection::open_with_flags(path, flags))?;
    sql(connection.busy_timeout(LOCK_WAIT))?;
    sql(connection.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA synchronous = FULL;"))?;
    // transaction prevents concurrent writers before the backup snapshot starts.
    // transaction 在备份快照开始前阻止并发写入者。
    let transaction = sql(connection.transaction_with_behavior(if apply {
        TransactionBehavior::Immediate
    } else {
        TransactionBehavior::Deferred
    }))?;
    verify_version(&transaction)?;
    verify_integrity(&transaction)?;
    // current and desired are actual SQLite catalogs, including explicit index definitions.
    // current 与 desired 是实际 SQLite 目录，包含显式索引定义。
    let current = schema(&transaction)?;
    let desired = schema(&expected)?;
    // changes reject unsupported mappings before any data or schema writes.
    // changes 在任何数据或结构写入之前拒绝不受支持的映射。
    let changes = plan(&transaction, &expected, &current, &desired)?;
    if changes.tables.is_empty() && changes.indexes.is_empty() && changes.retired_tables.is_empty()
    {
        return Ok(None);
    }
    println!("Tables to synchronize: {}", changes.tables.join(", "));
    println!("Tables to retire: {}", changes.retired_tables.join(", "));
    println!("Indexes to synchronize: {}", changes.indexes.join(", "));
    if !apply {
        return Err("Schema drift detected; run the independent command with --apply.".into());
    }
    // backup is a consistent SQLite snapshot created while the source write reservation is held.
    // backup 是在源库写入保留锁持有期间创建的一致 SQLite 快照。
    let backup = backup_database(path)?;
    sql(transaction.execute("ATTACH DATABASE ?1 AS before_sync", [path_text(&backup)?]))?;
    apply_changes(&transaction, &current, &desired, &changes)?;
    verify_retained_rows(&transaction, &current, &changes)?;
    verify_version(&transaction)?;
    verify_integrity(&transaction)?;
    // remaining confirms every managed object matches the authoritative DDL before commit.
    // remaining 在提交前确认每个受管对象都与权威 DDL 一致。
    let remaining = plan(&transaction, &expected, &schema(&transaction)?, &desired)?;
    if !remaining.tables.is_empty()
        || !remaining.indexes.is_empty()
        || !remaining.retired_tables.is_empty()
    {
        return Err("Schema synchronization did not reach the declared structure.".into());
    }
    sql(transaction.commit())?;
    Ok(Some(backup))
}

/// Convert SQLite failures to codes without exposing row values or trigger-generated messages.
/// 将 SQLite 失败转换为错误码，不泄漏行值或触发器生成的消息。
/// `result` is a SQLite operation; returns its value or a sanitized error.
/// `result` 为 SQLite 操作；返回操作值或脱敏错误。
fn sql<T>(result: rusqlite::Result<T>) -> Result<T, String> {
    result.map_err(|error| format!("SQLite operation failed ({:?}).", error.sqlite_error_code()))
}

/// Quote one known SQLite identifier; returns a safely delimited identifier.
/// 引用一个已知 SQLite 标识符；返回安全包围的标识符。
fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Convert an existing filesystem path to SQLite text, rejecting non-Unicode paths explicitly.
/// 将既有文件系统路径转换为 SQLite 文本，明确拒绝非 Unicode 路径。
fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "The database path must be valid Unicode.".into())
}

/// Inspect `connection` and return non-internal objects keyed by their unique names.
/// 检查 `connection`，返回以唯一名称索引的非内部对象。
fn schema(connection: &Connection) -> Result<BTreeMap<String, SchemaObject>, String> {
    // statement excludes automatic indexes because table DDL owns their constraints.
    // statement 排除自动索引，因为其约束归属于表 DDL。
    let mut statement = sql(connection.prepare("SELECT name, type, tbl_name, sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT GLOB 'sqlite_*'"))?;
    sql(sql(statement.query_map([], |row| {
        Ok((
            row.get(0)?,
            SchemaObject {
                kind: row.get(1)?,
                table: row.get(2)?,
                sql: row.get(3)?,
            },
        ))
    }))?
    .collect())
}

/// Read ordered column names and declared types for `table` from `connection`.
/// 从 `connection` 读取 `table` 按声明排序的列名和类型。
/// Returns an error for generated columns, whose mapping requires a separately reviewed change.
/// 生成列需要单独审核映射，因此遇到生成列时返回错误。
fn columns(connection: &Connection, table: &str) -> Result<Vec<(String, String)>, String> {
    // statement uses bound table names so schema identifiers cannot become executable input.
    // statement 绑定表名，避免结构标识符成为可执行输入。
    let mut statement =
        sql(connection
            .prepare("SELECT name, type, hidden FROM pragma_table_xinfo(?1) ORDER BY cid"))?;
    // rows carry only structural metadata, never credential values.
    // rows 仅包含结构元数据，不包含凭据值。
    let rows: Vec<(String, String, i64)> = sql(sql(
        statement.query_map([table], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    )?
    .collect())?;
    if rows.iter().any(|(_, _, hidden)| *hidden != 0) {
        return Err(format!(
            "Generated columns require an explicit mapping for {table}."
        ));
    }
    Ok(rows
        .into_iter()
        .map(|(name, kind, _)| (name, kind))
        .collect())
}

/// Compare actual catalogs, validating every retained column and explicitly retired marker.
/// 比较实际目录，验证每个保留列和明确废弃的标记。
/// Inputs are the source and reference connections/catalogs; returns safe rebuild operations.
/// 输入为源库、参照库连接及目录；返回安全重建操作。
fn plan(
    connection: &Connection,
    expected: &Connection,
    current: &BTreeMap<String, SchemaObject>,
    desired: &BTreeMap<String, SchemaObject>,
) -> Result<Changes, String> {
    // changes accumulate only supported differences; unrelated tables stay untouched.
    // changes 仅收集受支持的差异；不触碰无关表。
    let mut changes = Changes::default();
    if let Some(retired) = current.get("project_vce_settings") {
        if retired.kind != "table" {
            return Err("The retired project settings object must be a table.".into());
        }
        // This exact mapping was verified against the removed source DDL and the existing Debug catalog.
        // 此精确映射已经通过被删除的源 DDL 以及现有 Debug 目录核实。
        for (column, kind) in columns(connection, "project_vce_settings")? {
            // Unknown columns may belong to newer work; never erase them through the retirement mapping.
            // 未知列可能属于更新的工作，不能通过退役映射将其抹除。
            let supported = match column.as_str() {
                "project_id" | "extra_ignore_patterns_json" | "update_mode" | "updated_at" => {
                    kind == "TEXT"
                }
                "enabled"
                | "revision"
                | "max_file_bytes"
                | "allow_query_rewrite"
                | "allow_rerank"
                | "embedding_max_concurrency"
                | "query_max_concurrency"
                | "max_results"
                | "max_context_chars"
                | "auto_update_min_interval_seconds"
                | "migrated_from_codebase_memory" => kind == "INTEGER",
                _ => false,
            };
            if !supported {
                return Err(format!(
                    "Unmapped retired column project_vce_settings.{column}; no data was removed."
                ));
            }
        }
        changes.retired_tables.push("project_vce_settings".into());
    }
    if desired
        .values()
        .any(|object| !matches!(object.kind.as_str(), "table" | "index"))
    {
        return Err("The authoritative schema contains an unsupported object kind.".into());
    }
    for (name, object) in desired.iter().filter(|(_, object)| object.kind == "table") {
        // target_columns come from the executed authoritative DDL, not a copied field list.
        // target_columns 来自已执行的权威 DDL，不复制字段列表。
        let target_columns = columns(expected, name)?;
        if let Some(existing) = current.get(name) {
            if existing.kind != "table" {
                return Err(format!("Unexpected object kind for {name}."));
            }
            // source_columns are the only allowed source of retained values.
            // source_columns 是保留值唯一允许的来源。
            let source_columns = columns(connection, name)?;
            for column in &target_columns {
                if !source_columns.contains(column) {
                    return Err(format!(
                        "An explicit column mapping is required for {name}.{}.",
                        column.0
                    ));
                }
            }
            for (column, _) in source_columns
                .iter()
                .filter(|column| !target_columns.contains(column))
            {
                // This retired project flag was verified in local Debug storage and has no current readers.
                // 此废弃项目标志已经在本地 Debug 存储中查证，当前源码已无读取者。
                if !matches!(
                    (name.as_str(), column.as_str()),
                    ("projects", "codebase_memory_enabled")
                ) {
                    return Err(format!(
                        "Unmapped existing column {name}.{column}; no data was removed."
                    ));
                }
                // nonzero rejects meaningful legacy settings rather than inventing a replacement policy.
                // nonzero 拒绝仍有含义的旧设置，避免凭空推断替代策略。
                let nonzero: bool = sql(connection.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM {} WHERE {} IS NOT 0)",
                        identifier(name),
                        identifier(column)
                    ),
                    [],
                    |row| row.get(0),
                ))?;
                if nonzero {
                    return Err(format!(
                        "Retired marker {name}.{column} contains a nonzero value; explicit mapping required."
                    ));
                }
            }
        }
        changes.columns.insert(
            name.clone(),
            target_columns.into_iter().map(|(name, _)| name).collect(),
        );
        if current.get(name) != Some(object) {
            changes.tables.push(name.clone());
        }
    }
    for (name, object) in desired.iter().filter(|(_, object)| object.kind == "index") {
        if current
            .get(name)
            .is_some_and(|existing| existing.kind != "index")
        {
            return Err(format!("Unexpected object kind for {name}."));
        }
        if current.get(name) != Some(object) || changes.tables.contains(&object.table) {
            changes.indexes.push(name.clone());
        }
    }
    for (name, object) in current {
        if !changes.retired_tables.is_empty()
            && object.kind == "table"
            && !changes.retired_tables.contains(name)
        {
            // Inspect SQLite's own foreign-key metadata before retiring a table referenced by newer work.
            // 在退役可能被更新工作引用的表之前，检查 SQLite 自身的外键元数据。
            let mut statement =
                sql(connection.prepare("SELECT \"table\" FROM pragma_foreign_key_list(?1)"))?;
            // Referenced names contain structural metadata only; no row values enter diagnostics.
            // 引用名称仅包含结构元数据，不将记录值写入诊断。
            let references = sql(
                sql(statement.query_map([name], |row| row.get::<_, String>(0)))?
                    .collect::<Result<Vec<_>, _>>(),
            )?;
            if references
                .iter()
                .any(|table| changes.retired_tables.contains(table))
            {
                return Err(format!(
                    "Dependent table {name} requires an explicit mapping before retiring tables."
                ));
            }
        }
        // Views and triggers can reference another table; their dependencies require explicit mapping.
        // 视图和触发器可能引用其他表；其依赖关系需要显式映射。
        if (!changes.tables.is_empty() || !changes.retired_tables.is_empty())
            && matches!(object.kind.as_str(), "view" | "trigger")
        {
            return Err(format!(
                "Dependent object {name} requires an explicit mapping before rebuilding tables."
            ));
        }
        if (changes.tables.contains(&object.table)
            || changes.retired_tables.contains(&object.table))
            && object.kind != "table"
            && !desired.contains_key(name)
        {
            return Err(format!(
                "Unmanaged dependent object {name} requires an explicit mapping."
            ));
        }
    }
    Ok(changes)
}

/// Require matching current application versions in both SQLite and metadata.
/// 要求 SQLite 和元数据同时声明当前应用版本。
/// `connection` is inspected only; unsupported versions are never rewritten.
/// 仅检查 `connection`；绝不改写不受支持的版本。
fn verify_version(connection: &Connection) -> Result<(), String> {
    // version and recorded must agree with the sole application version constant.
    // version 与 recorded 必须和唯一应用版本常量一致。
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
    let recorded: String = sql(connection.query_row(
        "SELECT value FROM app_metadata WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    ))?;
    if version != APP_SCHEMA_VERSION || recorded != APP_SCHEMA_VERSION.to_string() {
        return Err("Unsupported application database version; no changes made.".into());
    }
    Ok(())
}

/// Verify physical, CHECK, and foreign-key integrity without reporting stored row contents.
/// 验证物理结构、CHECK 及外键完整性，不报告已存储行内容。
/// `connection` is inspected only; returns failure for any violated invariant.
/// 仅检查 `connection`；任何约束被破坏时返回失败。
fn verify_integrity(connection: &Connection) -> Result<(), String> {
    // valid and foreign_keys_valid aggregate diagnostics without logging corrupt records.
    // valid 与 foreign_keys_valid 汇总诊断，不记录损坏的行。
    let valid: bool = sql(connection.query_row(
        "SELECT COUNT(*) = 1 AND MIN(integrity_check) = 'ok' FROM pragma_integrity_check",
        [],
        |row| row.get(0),
    ))?;
    let foreign_keys_valid: bool = sql(connection.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |row| row.get(0),
    ))?;
    if !valid || !foreign_keys_valid {
        return Err("Database integrity verification failed.".into());
    }
    Ok(())
}

/// Create a persistent, unique snapshot while the caller owns the source write reservation.
/// 在调用者持有源库写入保留锁时创建持久、唯一的快照。
/// `path` identifies the source; returns a verified backup below its existing SQLite directory.
/// `path` 标识源库；返回位于其既有 SQLite 目录下的已验证备份。
fn backup_database(path: &Path) -> Result<PathBuf, String> {
    // parent keeps credential backups within the existing private application storage directory.
    // parent 将凭据备份保留在既有私有应用存储目录内。
    let parent = path
        .parent()
        .ok_or("The database parent directory is required.")?;
    // temporary reserves a unique owner-only file on Unix and inherits the application ACL on Windows.
    // temporary 在 Unix 上预留仅所有者可访问的唯一文件，在 Windows 上继承应用目录 ACL。
    let temporary = tempfile::Builder::new()
        .prefix("app-workspace-before-sync-")
        .suffix(".sqlite3")
        .tempfile_in(parent)
        .map_err(|error| error.to_string())?;
    // reader shares the committed snapshot; VACUUM INTO does not modify the locked source database.
    // reader 读取已提交快照；VACUUM INTO 不修改已锁定的源数据库。
    let reader = sql(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ))?;
    sql(reader.busy_timeout(LOCK_WAIT))?;
    sql(reader.execute_batch("PRAGMA synchronous = FULL;"))?;
    sql(reader.execute("VACUUM main INTO ?1", [path_text(temporary.path())?]))?;
    // verification opens the completed snapshot read-only before it can become a restore source.
    // verification 在快照成为恢复来源前以只读方式打开并验证。
    let verification = sql(Connection::open_with_flags(
        temporary.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ))?;
    verify_integrity(&verification)?;
    drop(verification);
    // persisted transfers deletion responsibility to the operator, even if a later transaction rolls back.
    // persisted 将删除责任交给操作者，即使后续事务回滚也保留备份。
    let (_, persisted) = temporary.keep().map_err(|error| error.to_string())?;
    println!("Verified backup: {}", persisted.display());
    Ok(persisted)
}

/// Recreate changed tables and indexes from authoritative SQL within the caller's transaction.
/// 在调用者事务内使用权威 SQL 重建发生变化的表和索引。
/// Catalogs define existing/desired objects; mappings copy retained columns exactly from the backup.
/// 目录定义既有与目标对象；映射从备份精确保留列值。
fn apply_changes(
    connection: &Connection,
    current: &BTreeMap<String, SchemaObject>,
    desired: &BTreeMap<String, SchemaObject>,
    changes: &Changes,
) -> Result<(), String> {
    for name in &changes.retired_tables {
        sql(connection.execute_batch(&format!("DROP TABLE main.{};", identifier(name))))?;
    }
    for name in &changes.tables {
        if current.contains_key(name) {
            sql(connection.execute_batch(&format!("DROP TABLE main.{};", identifier(name))))?;
        }
        sql(connection.execute_batch(&desired[name].sql))?;
        if current.contains_key(name) {
            // projection follows target columns exactly; removed zero-valued markers remain in the backup.
            // projection 精确遵循目标列；被移除的零值标记保留在备份中。
            let projection = changes.columns[name]
                .iter()
                .map(|name| identifier(name))
                .collect::<Vec<_>>()
                .join(", ");
            sql(connection.execute_batch(&format!("INSERT INTO main.{table} ({projection}) SELECT {projection} FROM before_sync.{table};", table = identifier(name))))?;
        }
    }
    for name in &changes.indexes {
        sql(connection.execute_batch(&format!("DROP INDEX IF EXISTS main.{};", identifier(name))))?;
        sql(connection.execute_batch(&desired[name].sql))?;
    }
    Ok(())
}

/// Compare every existing table with the backup, including unmanaged tables and SQLite value types.
/// 比较每张既有表与备份，包括非受管表及 SQLite 值类型。
/// `changes` specifies retained columns; counts and grouped multiplicities must match before commit.
/// `changes` 指定保留列；提交前行数及分组重复次数必须完全一致。
fn verify_retained_rows(
    connection: &Connection,
    current: &BTreeMap<String, SchemaObject>,
    changes: &Changes,
) -> Result<(), String> {
    // updated holds the final catalog once, shared by every unchanged-table comparison.
    // updated 保存一份最终目录，供全部未变表比较共用。
    let updated = schema(connection)?;
    for (name, object) in current.iter().filter(|(_, object)| object.kind == "table") {
        if changes.retired_tables.contains(name) {
            continue;
        }
        // retained uses explicit target mappings only for managed tables; all other columns are preserved.
        // retained 对受管表使用显式目标映射；其他表保留全部列。
        let retained = match changes.columns.get(name) {
            Some(columns) => columns.clone(),
            None => columns(connection, name)?
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
        };
        // projection includes typeof so affinity conversions cannot pass as unchanged values.
        // projection 包含 typeof，避免类型亲和性转换被误判为值未变化。
        let projection = retained
            .iter()
            .map(|name| format!("{0}, typeof({0})", identifier(name)))
            .collect::<Vec<_>>()
            .join(", ");
        // grouped compares duplicate multiplicities as well as values without materializing secrets in Rust.
        // grouped 同时比较重复次数及值，不在 Rust 中物化秘密。
        let grouped = |schema: &str| {
            format!(
                "SELECT {projection}, COUNT(*) FROM {schema}.{} GROUP BY {projection}",
                identifier(name)
            )
        };
        // matches verifies both set differences; diagnostics intentionally contain no row values.
        // matches 检查双向集合差；诊断刻意不包含行值。
        let matches: bool = sql(connection.query_row(
            &format!(
                "SELECT NOT EXISTS({} EXCEPT {}) AND NOT EXISTS({} EXCEPT {})",
                grouped("main"),
                grouped("before_sync"),
                grouped("before_sync"),
                grouped("main")
            ),
            [],
            |row| row.get(0),
        ))?;
        if !matches {
            return Err(format!(
                "Retained values changed in table {name}; synchronization rolled back."
            ));
        }
        if !changes.tables.contains(name) && updated.get(name) != Some(object) {
            return Err(format!("Unmanaged table structure changed: {name}."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

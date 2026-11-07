//! Runtime upgrade of the application SQLite database through an ordered chain of versioned steps.
//! 通过有序、带版本的步骤链在运行时升级应用 SQLite 数据库。
//!
//! Every step upgrades exactly one version, owns its SQL as frozen history, and never reads the current
//! authoritative DDL: a later version changing a table must add a new step instead of editing an old one.
//! Startup reads `PRAGMA user_version` before the initialization script runs and then:
//!
//! - `0`: a new database; the initialization script creates the current schema.
//! - the current version: nothing to upgrade.
//! - from the baseline up to the current version: takes a verified `VACUUM INTO` backup, applies every
//!   remaining step in order inside one `BEGIN IMMEDIATE` transaction, checks integrity and foreign keys,
//!   and commits; any failure rolls the database back and keeps the backup.
//! - anything else (older than the baseline or newer than this build): rejected without any change.
//!
//! 每个步骤恰好升级一个版本，其 SQL 作为冻结历史由步骤自身持有，绝不读取当前权威 DDL：后续版本修改表时
//! 必须新增步骤，不得改写旧步骤。启动时在初始化脚本运行前读取 `PRAGMA user_version`，然后：
//!
//! - `0`：新数据库，由初始化脚本创建当前结构。
//! - 当前版本：无需升级。
//! - 从基线到当前版本之间：先创建并校验 `VACUUM INTO` 备份，再在一个 `BEGIN IMMEDIATE` 事务内按顺序执行
//!   剩余全部步骤，检查完整性与外键后提交；任一步失败都回滚数据库并保留备份。
//! - 其他取值（早于基线或新于当前构建）：明确拒绝，不做任何修改。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Oldest application schema version that can still be upgraded at runtime; older versions are rejected.
/// 仍可在运行时升级的最旧应用结构版本；更旧的版本会被拒绝。
pub const APP_SCHEMA_BASELINE_VERSION: i64 = 1;

/// One upgrade step from `from_version` to `from_version + 1`.
/// 从 `from_version` 升级到 `from_version + 1` 的一个步骤。
pub(crate) struct AppSchemaMigration {
    /// Version this step upgrades from.
    /// 该步骤升级的起始版本。
    pub(crate) from_version: i64,
    /// Short English description used in diagnostics.
    /// 用于诊断的简短英文说明。
    pub(crate) description: &'static str,
    /// Applies the step inside the caller's transaction; it must not change the version markers.
    /// 在调用方事务内执行该步骤；不得修改版本标记。
    pub(crate) apply: fn(&Transaction<'_>) -> rusqlite::Result<()>,
}

/// Ordered upgrade chain; entry `i` upgrades from `APP_SCHEMA_BASELINE_VERSION + i`.
/// 有序升级链；第 `i` 项从 `APP_SCHEMA_BASELINE_VERSION + i` 开始升级。
pub(crate) const APP_SCHEMA_MIGRATIONS: &[AppSchemaMigration] = &[
    AppSchemaMigration {
        from_version: 1,
        description: "add project session sort preferences and session list state",
        apply: migrate_v1_to_v2,
    },
    AppSchemaMigration {
        from_version: 2,
        description: "add session activity clear markers",
        apply: migrate_v2_to_v3,
    },
    AppSchemaMigration {
        from_version: 3,
        description: "add local announcement marks",
        apply: migrate_v3_to_v4,
    },
];

/// Current application schema version, derived from the baseline and the length of the upgrade chain.
/// 当前应用结构版本，由基线与升级链长度派生。
pub const APP_SCHEMA_VERSION: i64 =
    APP_SCHEMA_BASELINE_VERSION + APP_SCHEMA_MIGRATIONS.len() as i64;

/// Version 2: per-project session sort mode and per-session list ordering facts.
/// 版本 2：按项目保存的会话排序方式，以及按会话保存的列表排序事实。
///
/// The session locator tables are created lazily, so the list state is created and backfilled only when
/// `session_index_v1` already exists; otherwise its later creation brings the list state with it. A
/// historical session's latest request time is its existing `updated_at_ms`, the fact the sidebar sorted
/// by before this version, and it has no manual position yet.
/// 会话定位表是惰性创建的，因此只有 `session_index_v1` 已存在时才创建并回填列表状态；否则其后续创建会一并
/// 创建列表状态。历史会话的最近请求时间取既有的 `updated_at_ms`，即本版本之前侧栏排序所用的同一事实，且尚无
/// 手动位置。
fn migrate_v1_to_v2(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS project_session_preferences (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    session_sort_mode TEXT NOT NULL CHECK(session_sort_mode IN ('last_request', 'manual'))
);
"#,
    )?;
    let has_session_index: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'session_index_v1')",
        [],
        |row| row.get(0),
    )?;
    if has_session_index {
        transaction.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS session_list_state_v1 (
    session_id TEXT PRIMARY KEY
        REFERENCES session_index_v1(session_id) ON DELETE CASCADE,
    last_request_at_ms INTEGER NOT NULL,
    manual_position INTEGER
);

INSERT INTO session_list_state_v1 (session_id, last_request_at_ms, manual_position)
SELECT session_id, updated_at_ms, NULL FROM session_index_v1;
"#,
        )?;
    }
    Ok(())
}

/// Version 3: per-session clear markers of the activity panel.
/// 版本 3：活动面板按会话保存的清理标记。
///
/// A new table without rows; sessions never cleared have no marker.
/// 新表不含任何行；从未清理过的会话没有标记。
fn migrate_v2_to_v3(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS session_activity_marks_v1 (
    session_id TEXT PRIMARY KEY,
    subagents_cleared_at_ms INTEGER,
    commands_cleared_at_ms INTEGER
);
"#,
    )
}

/// Version 4: local marks of OpenVulcan announcements (seen in the message center, banner closed).
/// 版本 4：OpenVulcan 公告的本地标记（已在消息中心看到、横幅已关闭）。
///
/// A new table without rows; announcements never seen or closed have no marks.
/// 新表不含任何行；从未看过或关闭过的公告没有标记。
fn migrate_v3_to_v4(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS announcement_states_v1 (
    region TEXT NOT NULL,
    announcement_id TEXT NOT NULL,
    seen_at_ms INTEGER,
    dismissed_at_ms INTEGER,
    PRIMARY KEY (region, announcement_id)
);
"#,
    )
}

/// Brings an existing application database to `APP_SCHEMA_VERSION` before the initialization script runs.
/// 在初始化脚本运行之前，将既有应用数据库升级到 `APP_SCHEMA_VERSION`。
///
/// `connection` is the application database; `sqlite_path` is its file, used only to place the backup.
/// Returns the verified backup path when an upgrade ran, `None` for new or current databases, or an error
/// that leaves the database unchanged.
/// `connection` 为应用数据库；`sqlite_path` 为其文件，仅用于放置备份。执行了升级时返回已校验的备份路径；
/// 新库或当前版本返回 `None`；出错时数据库保持不变。
pub(crate) fn upgrade_application_schema(
    connection: &mut Connection,
    sqlite_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let observed = read_user_version(connection)?;
    let steps = pending_steps(observed)?;
    if steps.is_empty() {
        return Ok(None);
    }
    verify_integrity(connection)?;
    let backup = backup_database(connection, sqlite_path, observed)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("Failed to start the application schema upgrade: {error}"))?;
    for step in steps {
        (step.apply)(&transaction).map_err(|error| {
            format!(
                "Application schema upgrade from v{} ({}) failed: {error}",
                step.from_version, step.description
            )
        })?;
    }
    write_version_markers(&transaction)?;
    verify_integrity(&transaction)?;
    transaction
        .commit()
        .map_err(|error| format!("Failed to commit the application schema upgrade: {error}"))?;
    Ok(Some(backup))
}

/// Selects the steps that bring `observed` to the current version; rejects unsupported versions.
/// 选取将 `observed` 升级到当前版本所需的步骤；拒绝不受支持的版本。
fn pending_steps(observed: i64) -> Result<&'static [AppSchemaMigration], String> {
    if observed == 0 || observed == APP_SCHEMA_VERSION {
        return Ok(&[]);
    }
    if !(APP_SCHEMA_BASELINE_VERSION..APP_SCHEMA_VERSION).contains(&observed) {
        return Err(format!(
            "Unsupported application schema version {observed}; this build upgrades v{APP_SCHEMA_BASELINE_VERSION} through v{APP_SCHEMA_VERSION}."
        ));
    }
    // The range check above keeps the offset within the chain.
    // 上方的范围检查保证偏移量位于升级链之内。
    let offset = usize::try_from(observed - APP_SCHEMA_BASELINE_VERSION)
        .map_err(|_| format!("Invalid application schema version {observed}."))?;
    Ok(&APP_SCHEMA_MIGRATIONS[offset..])
}

/// Reads `PRAGMA user_version` from the application database.
/// 读取应用数据库的 `PRAGMA user_version`。
fn read_user_version(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("Failed to inspect the application schema version: {error}"))
}

/// Writes the current version into both version markers inside the upgrade transaction.
/// 在升级事务内把当前版本写入两个版本标记。
fn write_version_markers(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(&format!(
            "INSERT INTO app_metadata (key, value) VALUES ('schema_version', '{APP_SCHEMA_VERSION}')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;
             PRAGMA user_version = {APP_SCHEMA_VERSION};"
        ))
        .map_err(|error| {
            format!("Failed to record the upgraded application schema version: {error}")
        })
}

/// Checks page integrity, CHECK constraints and foreign keys without reporting stored values.
/// 检查页完整性、CHECK 约束与外键，不报告已存储的值。
fn verify_integrity(connection: &Connection) -> Result<(), String> {
    let valid: bool = connection
        .query_row(
            "SELECT COUNT(*) = 1 AND MIN(integrity_check) = 'ok' FROM pragma_integrity_check",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Failed to check application database integrity: {error}"))?;
    let foreign_keys_valid: bool = connection
        .query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Failed to check application database foreign keys: {error}"))?;
    if valid && foreign_keys_valid {
        Ok(())
    } else {
        Err(String::from(
            "The application database failed its integrity check; no upgrade was applied.",
        ))
    }
}

/// Writes a consistent snapshot next to the database and verifies it before the upgrade starts.
/// 在升级开始前，于数据库旁写入一致快照并完成校验。
///
/// The backup inherits the private application directory permissions and is never deleted automatically.
/// 备份沿用应用私有目录的权限，且不会被自动删除。
fn backup_database(
    connection: &Connection,
    sqlite_path: &Path,
    observed: i64,
) -> Result<PathBuf, String> {
    let parent = sqlite_path
        .parent()
        .ok_or_else(|| String::from("The application database has no parent directory."))?;
    let reserved = tempfile::Builder::new()
        .prefix(&format!(
            "app-workspace-before-v{APP_SCHEMA_VERSION}-from-v{observed}-"
        ))
        .suffix(".sqlite3")
        .tempfile_in(parent)
        .map_err(|error| format!("Failed to reserve the upgrade backup file: {error}"))?;
    let target = reserved
        .path()
        .to_str()
        .ok_or_else(|| String::from("The upgrade backup path must be valid Unicode."))?
        .to_string();
    connection
        .execute("VACUUM main INTO ?1", [target.as_str()])
        .map_err(|error| format!("Failed to back up the application database: {error}"))?;
    let verification =
        Connection::open_with_flags(reserved.path(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| format!("Failed to open the upgrade backup: {error}"))?;
    verify_integrity(&verification)?;
    drop(verification);
    let (_, kept) = reserved
        .keep()
        .map_err(|error| format!("Failed to keep the upgrade backup: {error}"))?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_plane::build_application_schema_script;
    use crate::workspace_store::SESSION_INDEX_V1_SCHEMA_SQL;

    /// Opens one file database in a private temporary directory, as the bootstrap does.
    /// 与引导过程一样，在私有临时目录中打开一个文件数据库。
    fn database() -> (tempfile::TempDir, PathBuf, Connection) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("app-workspace.sqlite3");
        let connection = Connection::open(&path).expect("open database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        (directory, path, connection)
    }

    /// Creates the current schema, including the lazily created session locator tables.
    /// 创建当前结构，包括惰性创建的会话定位表。
    fn create_current(connection: &Connection, with_session_index: bool) {
        connection
            .execute_batch(&build_application_schema_script())
            .expect("current schema");
        if with_session_index {
            connection
                .execute_batch(SESSION_INDEX_V1_SCHEMA_SQL)
                .expect("session index schema");
        }
    }

    /// Turns a current database back into version 1 by removing what versions 2 to 4 added.
    /// 移除版本 2 至版本 4 新增的对象，把当前数据库还原为版本 1。
    fn downgrade_to_v1(connection: &Connection) {
        connection
            .execute_batch(
                "DROP TABLE IF EXISTS session_list_state_v1;
                 DROP TABLE project_session_preferences;
                 DROP TABLE session_activity_marks_v1;
                 DROP TABLE announcement_states_v1;
                 UPDATE app_metadata SET value = '1' WHERE key = 'schema_version';
                 PRAGMA user_version = 1;",
            )
            .expect("v1 fixture");
    }

    /// Returns the stored definition of every non-internal object, keyed by name.
    /// 返回每个非内部对象的已存储定义，以名称为键。
    fn catalog(connection: &Connection) -> Vec<(String, String)> {
        let mut statement = connection
            .prepare(
                "SELECT name, sql FROM sqlite_schema
                 WHERE sql IS NOT NULL AND name NOT GLOB 'sqlite_*' ORDER BY name",
            )
            .expect("catalog query");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("catalog rows")
            .collect::<Result<_, _>>()
            .expect("collect catalog")
    }

    /// The chain is contiguous from the baseline and its length defines the current version.
    /// 升级链自基线起连续，其长度决定当前版本。
    #[test]
    fn chain_is_contiguous_from_the_baseline() {
        for (index, step) in APP_SCHEMA_MIGRATIONS.iter().enumerate() {
            assert_eq!(
                step.from_version,
                APP_SCHEMA_BASELINE_VERSION + index as i64,
                "step {index} must upgrade the version right after the previous step"
            );
        }
        assert_eq!(
            crate::data_plane::APP_SCHEMA_VERSION,
            APP_SCHEMA_VERSION,
            "the data plane must expose the derived version"
        );
    }

    /// Upgrading the oldest supported database yields exactly the schema a new database gets.
    /// 升级最旧受支持数据库所得的结构与新数据库完全一致。
    #[test]
    fn upgraded_schema_equals_a_new_database() {
        for with_session_index in [true, false] {
            let (_directory, path, mut connection) = database();
            create_current(&connection, with_session_index);
            downgrade_to_v1(&connection);
            let backup = upgrade_application_schema(&mut connection, &path)
                .expect("upgrade")
                .expect("an upgrade must report its backup");
            assert!(backup.is_file());
            create_current(&connection, with_session_index);

            let (_fresh_directory, _fresh_path, fresh) = database();
            create_current(&fresh, with_session_index);
            assert_eq!(
                catalog(&connection),
                catalog(&fresh),
                "an upgraded database must match a new one (session index: {with_session_index})"
            );
            assert_eq!(read_user_version(&connection).unwrap(), APP_SCHEMA_VERSION);
        }
    }

    /// Existing sessions keep their recency as the latest request and receive no manual position.
    /// 既有会话以原最近程度作为最近请求时间，且没有手动位置。
    #[test]
    fn v2_backfills_session_list_state_from_updated_at() {
        let (_directory, path, mut connection) = database();
        create_current(&connection, true);
        downgrade_to_v1(&connection);
        connection
            .execute_batch(
                "INSERT INTO session_index_v1 VALUES
                    ('s1','p','tag','sessions','tag/2026-01-01-s1','one',10,40,NULL,0),
                    ('s2','p','tag','sessions','tag/2026-01-01-s2','two',20,30,5,1);",
            )
            .expect("v1 rows");
        upgrade_application_schema(&mut connection, &path).expect("upgrade");
        let mut statement = connection
            .prepare("SELECT session_id, last_request_at_ms, manual_position FROM session_list_state_v1 ORDER BY session_id")
            .unwrap();
        let rows: Vec<(String, i64, Option<i64>)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![("s1".into(), 40, None), ("s2".into(), 30, None)]);
    }

    /// New and current databases are left alone and produce no backup.
    /// 新库与当前版本数据库保持不变，也不产生备份。
    #[test]
    fn new_and_current_databases_need_no_upgrade() {
        let (directory, path, mut connection) = database();
        assert_eq!(
            upgrade_application_schema(&mut connection, &path).unwrap(),
            None
        );
        create_current(&connection, true);
        assert_eq!(
            upgrade_application_schema(&mut connection, &path).unwrap(),
            None
        );
        let files = std::fs::read_dir(directory.path()).unwrap().count();
        assert_eq!(files, 1, "no backup may be written without an upgrade");
    }

    /// Versions outside the supported range are rejected without touching the database.
    /// 受支持范围之外的版本被拒绝，且不触碰数据库。
    #[test]
    fn unsupported_versions_are_rejected_unchanged() {
        for version in [APP_SCHEMA_BASELINE_VERSION - 1, APP_SCHEMA_VERSION + 1] {
            if version == 0 {
                continue;
            }
            let (directory, path, mut connection) = database();
            connection
                .execute_batch(&format!(
                    "CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('kept'); PRAGMA user_version = {version};"
                ))
                .unwrap();
            assert!(upgrade_application_schema(&mut connection, &path).is_err());
            assert_eq!(read_user_version(&connection).unwrap(), version);
            let kept: String = connection
                .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
                .unwrap();
            assert_eq!(kept, "kept");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    /// A failing step rolls the whole upgrade back and keeps the verified backup.
    /// 失败的步骤会回滚整个升级，并保留已校验的备份。
    #[test]
    fn failed_upgrade_rolls_back_and_keeps_the_backup() {
        let (directory, path, mut connection) = database();
        create_current(&connection, true);
        downgrade_to_v1(&connection);
        // A pre-existing table of the same name with an incompatible shape makes the backfill fail.
        // 预先存在、结构不兼容的同名表会让回填失败。
        connection
            .execute_batch(
                "CREATE TABLE session_list_state_v1 (other TEXT);
                 INSERT INTO session_index_v1 VALUES ('s1','p','tag','sessions','tag/x-s1','one',1,2,NULL,0);",
            )
            .unwrap();
        assert!(upgrade_application_schema(&mut connection, &path).is_err());
        assert_eq!(read_user_version(&connection).unwrap(), 1);
        let preferences: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'project_session_preferences')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            !preferences,
            "a failed step must roll back earlier statements"
        );
        let backups = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("app-workspace-before-")
            })
            .count();
        assert_eq!(backups, 1);
    }
}

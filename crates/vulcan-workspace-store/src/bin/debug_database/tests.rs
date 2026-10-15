//! Behavioral tests use disposable SQLite files and never resolve a user's application directory.
//! 行为测试仅使用可丢弃的 SQLite 文件，绝不解析用户应用目录。

use super::*;

/// Historical source schema used only to exercise explicit retirement and backup preservation.
/// 仅用于验证显式退役与备份保留的历史源结构。
const RETIRED_PROJECT_SETTINGS_FIXTURE_SQL: &str = r#"
CREATE TABLE project_vce_settings (
    project_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1,
    max_file_bytes INTEGER NOT NULL DEFAULT 1048576,
    extra_ignore_patterns_json TEXT NOT NULL DEFAULT '[]',
    allow_query_rewrite INTEGER NOT NULL DEFAULT 0,
    allow_rerank INTEGER NOT NULL DEFAULT 1,
    embedding_max_concurrency INTEGER NOT NULL DEFAULT 4,
    query_max_concurrency INTEGER NOT NULL DEFAULT 3,
    max_results INTEGER NOT NULL DEFAULT 10,
    max_context_chars INTEGER NOT NULL DEFAULT 32000,
    update_mode TEXT NOT NULL,
    auto_update_min_interval_seconds INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    migrated_from_codebase_memory INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
"#;

/// Create an isolated historical Debug shape containing credentials, projects and a dependent table.
/// 创建隔离的历史 Debug 结构，包含凭据、项目及依赖表。
/// `directory` owns all files; returns the database path with synthetic records only.
/// `directory` 持有全部文件；返回仅含合成记录的数据库路径。
fn fixture(directory: &Path) -> PathBuf {
    // path is private to this fixture and deliberately does not use application_sqlite_path.
    // path 专属于当前夹具，刻意不使用 application_sqlite_path。
    let path = directory.join("fixture.sqlite3");
    // connection initializes the current DDL before introducing explicit historical differences.
    // connection 先初始化当前 DDL，再引入明确的历史差异。
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(&build_application_schema_script())
        .unwrap();
    // credential_sql derives all columns from SQLite; the extra CHECK models a formerly unsupported kind.
    // credential_sql 从 SQLite 派生全部列；额外 CHECK 模拟此前不支持的类型。
    let mut credential_sql = schema(&connection).unwrap()["ai_credential_secrets"]
        .sql
        .clone();
    // closing identifies the final table-definition delimiter in the generated SQL.
    // closing 标识生成 SQL 中表定义的最后一个结束分隔符。
    let closing = credential_sql.rfind(')').unwrap();
    credential_sql.insert_str(closing, ", CHECK(material_kind != 'open_design_auth')");
    connection
        .execute_batch("DROP TABLE ai_credential_secrets;")
        .unwrap();
    connection.execute_batch(&credential_sql).unwrap();
    connection
        .execute_batch(RETIRED_PROJECT_SETTINGS_FIXTURE_SQL)
        .unwrap();
    connection.execute_batch("ALTER TABLE projects ADD COLUMN codebase_memory_enabled INTEGER NOT NULL DEFAULT 0;
        INSERT INTO projects (id, name, root_path, pinned, created_at, updated_at) VALUES ('project', 'Synthetic', '/synthetic', 1, 'created', 'updated');
        INSERT INTO project_vce_settings (project_id, enabled, update_mode, auto_update_min_interval_seconds, updated_at) VALUES ('project', 1, 'automatic', 600, 'updated');
        INSERT INTO ai_credential_secrets (secret_reference, format_version, revision, material_kind, secret_value, created_at_unix_ms, updated_at_unix_ms) VALUES ('key', 1, 2, 'api_key', 'synthetic-secret', 10, 20);
        CREATE TABLE independent_records (id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id) ON DELETE CASCADE, contents BLOB);
        INSERT INTO independent_records VALUES ('record', 'project', X'0001FF');").unwrap();
    path
}

/// Insert a synthetic paired credential into `connection`; returns the SQLite insertion result.
/// 向 `connection` 插入合成的成对凭据；返回 SQLite 插入结果。
fn insert_pair(connection: &Connection) -> rusqlite::Result<usize> {
    connection.execute("INSERT INTO ai_credential_secrets (secret_reference, format_version, revision, material_kind, secret_value, access_token, account_subject, email, created_at_unix_ms, updated_at_unix_ms) VALUES ('pair', 1, 1, 'open_design_auth', 'synthetic-control', 'synthetic-runtime', 'synthetic-user', 'synthetic@example.test', 1, 2)", [])
}

/// Verify v1 upgrades, original values, dependent rows, usable backups and idempotence.
/// 验证 v1 升级、原值、依赖行、可用备份及幂等性。
/// No parameters or return value; all assertions operate on a disposable fixture.
/// 无参数及返回值；全部断言操作可丢弃夹具。
#[test]
fn synchronization_preserves_rows_and_enables_paired_credentials() {
    // directory and path isolate both the source and retained backup.
    // directory 与 path 隔离源库及保留的备份。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    // before proves the synthetic old constraint rejects the new material kind.
    // before 证明合成旧约束拒绝新凭据类型。
    let before = Connection::open(&path).unwrap();
    assert!(insert_pair(&before).is_err());
    drop(before);
    // backup records the successful update's independently readable restore source.
    // backup 记录成功更新后可独立读取的恢复来源。
    let backup = synchronize(&path, true).unwrap().unwrap();
    let connection = Connection::open(&path).unwrap();
    verify_version(&connection).unwrap();
    verify_integrity(&connection).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT secret_value FROM ai_credential_secrets WHERE secret_reference='key'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "synthetic-secret"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT contents FROM independent_records WHERE id='record'",
                [],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        vec![0, 1, 255]
    );
    assert!(
        !schema(&connection)
            .unwrap()
            .contains_key("project_vce_settings")
    );
    assert!(
        !columns(&connection, "projects")
            .unwrap()
            .iter()
            .any(|(name, _)| name == "codebase_memory_enabled")
    );
    insert_pair(&connection).unwrap();
    assert!(
        connection
            .execute(
                "UPDATE ai_credential_secrets SET secret_value=NULL WHERE secret_reference='pair'",
                []
            )
            .is_err()
    );
    // original keeps the historical schema and all original records unchanged.
    // original 保留历史结构及全部原始记录。
    let original = Connection::open(&backup).unwrap();
    assert_eq!(original.query_row("SELECT auto_update_min_interval_seconds FROM project_vce_settings WHERE project_id='project'", [], |row| row.get::<_, i64>(0)).unwrap(), 600);
    verify_version(&original).unwrap();
    assert!(insert_pair(&original).is_err());
    assert!(
        columns(&original, "projects")
            .unwrap()
            .iter()
            .any(|(name, _)| name == "codebase_memory_enabled")
    );
    assert!(synchronize(&path, true).unwrap().is_none());
    assert!(synchronize(&path, false).unwrap().is_none());
}

/// Verify check-only mode leaves the existing database bytes and directory contents unchanged.
/// 验证只读检查保持既有数据库字节及目录内容不变。
/// No parameters or return value; no real user paths are accessed.
/// 无参数及返回值；不访问真实用户路径。
#[test]
fn check_only_reports_drift_without_writing() {
    // directory and path hold the synthetic old structure.
    // directory 与 path 保存合成旧结构。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    // original is a byte-level guard against writes by check mode.
    // original 用于按字节约束检查模式不得写入。
    let original = std::fs::read(&path).unwrap();
    assert!(synchronize(&path, false).is_err());
    assert!(std::fs::read(&path).unwrap() == original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

/// Reject unknown columns and meaningful retired flags before creating a backup or changing data.
/// 在创建备份或修改数据前拒绝未知列及有含义的废弃标志。
/// No parameters or return value; each case receives its own disposable database.
/// 无参数及返回值；每种情况均使用独立可丢弃数据库。
#[test]
fn unsupported_mappings_do_not_drop_data() {
    for modification in [
        "ALTER TABLE projects ADD COLUMN future_setting TEXT",
        "ALTER TABLE project_vce_settings ADD COLUMN future_setting TEXT",
        "ALTER TABLE project_vce_settings DROP COLUMN max_results",
        "CREATE INDEX future_vce_index ON project_vce_settings(enabled)",
        "CREATE TABLE future_dependency (id TEXT REFERENCES project_vce_settings(project_id))",
        "CREATE TABLE future_dependency (id TEXT REFERENCES PROJECT_VCE_SETTINGS(project_id))",
        "UPDATE projects SET codebase_memory_enabled=1",
        "CREATE VIEW old_projection AS SELECT codebase_memory_enabled FROM projects",
        "CREATE TRIGGER old_projection AFTER INSERT ON independent_records BEGIN UPDATE projects SET codebase_memory_enabled=1; END",
    ] {
        // directory, path and connection belong only to the current mapping case.
        // directory、path 及 connection 仅属于当前映射用例。
        let directory = tempfile::tempdir().unwrap();
        let path = fixture(directory.path());
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(modification).unwrap();
        drop(connection);
        // original detects even partial destructive updates after refusal.
        // original 可检测拒绝后的任何局部破坏性更新。
        let original = std::fs::read(&path).unwrap();
        assert!(synchronize(&path, true).is_err());
        assert!(std::fs::read(&path).unwrap() == original);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

/// Retires the same SQLite table identity under a different ASCII spelling and preserves its backup.
/// 退役使用不同 ASCII 大小写拼写的同一 SQLite 表身份，并保留其备份。
/// No parameters or return value; all database files belong to this temporary fixture.
/// 无参数及返回值；全部数据库文件均归当前临时夹具所有。
#[test]
fn retired_table_identity_uses_sqlite_case_rules() {
    // The isolated fixture contains one retired settings row and unrelated retained rows.
    // 隔离夹具包含一条退役设置记录及其他需要保留的记录。
    let directory = tempfile::tempdir().unwrap();
    // The target path remains private to the fixture throughout synchronization.
    // 同步全过程中目标路径始终归夹具私有。
    let path = fixture(directory.path());
    // SQLite requires an intermediate identity for a rename that changes only letter case.
    // SQLite 对仅改变大小写的重命名需要一个中间身份。
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("ALTER TABLE project_vce_settings RENAME TO retired_case_intermediate; ALTER TABLE retired_case_intermediate RENAME TO PROJECT_VCE_SETTINGS;").unwrap();
    drop(connection);
    // The production mapping must recognize the identity without guessing a different table shape.
    // 生产映射必须识别该身份，且不能推测其他表结构。
    let backup = synchronize(&path, true).unwrap().unwrap();
    // The target no longer contains the retired table under any ASCII spelling.
    // 目标库不再包含任意 ASCII 大小写拼写的退役表。
    let current = Connection::open(&path).unwrap();
    assert!(
        !schema(&current)
            .unwrap()
            .keys()
            .any(|name| name.eq_ignore_ascii_case("project_vce_settings"))
    );
    // The backup still resolves the original row through SQLite's identifier rules.
    // 备份仍可按 SQLite 标识符规则读取原始记录。
    let original = Connection::open(&backup).unwrap();
    assert_eq!(
        original
            .query_row("SELECT COUNT(*) FROM project_vce_settings", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
}

/// Verify a target NOT NULL failure rolls back earlier table rebuilds and retains the backup.
/// 验证目标 NOT NULL 失败会回滚此前重建的表并保留备份。
/// No parameters or return value; a nullable historical column supplies the invalid fixture.
/// 无参数及返回值；可空的历史列提供不符合新约束的夹具。
#[test]
fn failed_copy_rolls_back_all_prior_changes() {
    // The fixture owns every original and backup byte.
    // 夹具持有全部源库与备份字节。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    // Keep the original constraint generation observable after a later copy fails.
    // 保持原约束代际可观察，以验证后续复制失败时发生回滚。
    let connection = Connection::open(&path).unwrap();
    // Relax only a known source NOT NULL constraint so the authoritative target rejects its null.
    // 仅放宽已确认源列的非空约束，让权威目标拒绝其中的空值。
    let nullable_sql = schema(&connection).unwrap()["projects"]
        .sql
        .replace("name TEXT NOT NULL", "name TEXT");
    connection
        .execute_batch("PRAGMA foreign_keys=OFF; DROP TABLE projects;")
        .unwrap();
    connection.execute_batch(&nullable_sql).unwrap();
    connection.execute_batch("INSERT INTO projects (id, name, root_path, created_at, updated_at) VALUES ('project', NULL, '/synthetic', 'created', 'updated');").unwrap();
    // Compare the complete original catalog after the failed transaction.
    // 在失败事务之后比较完整原目录。
    let original = schema(&connection).unwrap();
    assert!(synchronize(&path, true).is_err());
    assert!(schema(&connection).unwrap() == original);
    assert!(insert_pair(&connection).is_err());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM independent_records", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        schema(&connection)
            .unwrap()
            .contains_key("project_vce_settings")
    );
    assert!(std::fs::read_dir(directory.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("app-workspace-before-sync-")
    }));
}

/// Verify source write locks bound contention and WAL snapshots include committed records.
/// 验证源库写锁对争用有时间界限，WAL 快照包含已提交记录。
/// No parameters or return value; only the fixture's connections compete.
/// 无参数及返回值；仅夹具连接参与争用。
#[test]
fn wal_backup_preserves_records_and_writer_contention_is_explicit() {
    // directory, path and writer hold an isolated WAL database.
    // directory、path 及 writer 持有隔离 WAL 数据库。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; BEGIN IMMEDIATE;")
        .unwrap();
    assert!(synchronize(&path, true).is_err());
    writer
        .execute_batch("UPDATE projects SET sort_order=17 WHERE id='project'; COMMIT;")
        .unwrap();
    // backup must contain the committed WAL value even while the original connection stays open.
    // 即使原连接仍打开，backup 也必须包含已提交的 WAL 值。
    let backup = synchronize(&path, true).unwrap().unwrap();
    let original = Connection::open(backup).unwrap();
    assert_eq!(
        original
            .query_row(
                "SELECT sort_order FROM projects WHERE id='project'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        17
    );
}

/// Reject unknown version declarations without rewriting either version or original rows.
/// 拒绝未知版本声明，不改写版本或原始行。
/// No parameters or return value; version drift is synthesized in a temporary file.
/// 无参数及返回值；版本差异仅在临时文件中合成。
#[test]
fn unsupported_versions_are_not_relabelled() {
    // directory and path isolate the unsupported-version fixture.
    // directory 与 path 隔离不受支持版本夹具。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", APP_SCHEMA_VERSION + 1)
        .unwrap();
    // original proves rejection does not simply relabel an incompatible database.
    // original 证明拒绝不会简单重标不兼容数据库。
    let original = std::fs::read(&path).unwrap();
    assert!(synchronize(&path, true).is_err());
    assert!(std::fs::read(&path).unwrap() == original);
}

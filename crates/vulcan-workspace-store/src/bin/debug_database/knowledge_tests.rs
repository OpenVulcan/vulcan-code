//! Offline knowledge-schema removal tests over synthetic temporary databases.
//! 对合成临时数据库执行的离线知识结构移除测试。

use super::*;

/// Creates the verified retired import shape in `directory`; returns a disposable database with preserved unrelated data.
/// 在 `directory` 创建已查证废弃导入形状；返回含应保留无关数据的可丢弃数据库。
fn fixture(directory: &Path) -> PathBuf {
    // The fixture never resolves the real Debug path.
    // 夹具绝不解析真实 Debug 路径。
    let path = directory.join("retirement.sqlite3");
    // Start from the current generator and introduce only the documented removed objects.
    // 从当前生成器开始，仅引入文档中已移除对象。
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(&build_application_schema_script())
        .unwrap();
    connection.execute_batch("ALTER TABLE knowledge_libraries ADD COLUMN last_completed_import_at_ms INTEGER;
        CREATE TABLE knowledge_import_jobs (
            id TEXT PRIMARY KEY,library_id TEXT NOT NULL,source_root TEXT NOT NULL,
            operation_kind TEXT NOT NULL DEFAULT 'import',state TEXT NOT NULL,phase TEXT NOT NULL,
            scanned_files INTEGER NOT NULL DEFAULT 0,total_files INTEGER NOT NULL DEFAULT 0,
            processed_files INTEGER NOT NULL DEFAULT 0,total_index_items INTEGER NOT NULL DEFAULT 0,
            processed_index_items INTEGER NOT NULL DEFAULT 0,current_file TEXT,changed_files INTEGER NOT NULL DEFAULT 0,
            unchanged_files INTEGER NOT NULL DEFAULT 0,deleted_files INTEGER NOT NULL DEFAULT 0,
            skipped_files INTEGER NOT NULL DEFAULT 0,processed_bytes INTEGER NOT NULL DEFAULT 0,
            warnings_json TEXT NOT NULL DEFAULT '[]',observability_json TEXT NOT NULL DEFAULT '{}',error_message TEXT,
            created_at_ms INTEGER NOT NULL,started_at_ms INTEGER,finished_at_ms INTEGER,
            FOREIGN KEY (library_id) REFERENCES knowledge_libraries(id) ON DELETE CASCADE);
        CREATE INDEX idx_knowledge_import_jobs_library ON knowledge_import_jobs(library_id,created_at_ms DESC);
        INSERT INTO knowledge_libraries(id,name,name_key,description,settings_json,created_at_ms,updated_at_ms,last_completed_import_at_ms)
            VALUES('library','Library','library','purpose','{}',1,2,12345);
        INSERT INTO projects(id,name,root_path,created_at,updated_at) VALUES('project','Project','synthetic','','');
        INSERT INTO knowledge_library_project_bindings VALUES('library','project',1,1,2);
        INSERT INTO knowledge_operations(id,library_id,kind,state,phase,created_at_ms) VALUES('current','library','rebuild','completed','completed',3);
        INSERT INTO knowledge_import_jobs(id,library_id,source_root,state,phase,created_at_ms,finished_at_ms)
            VALUES('retired','library','synthetic','completed','completed',4,5);").unwrap();
    path
}

/// Verifies timestamp/table removal preserves all retained values and their original backup; no inputs or return value.
/// 验证时间戳及整表移除保留全部应留值及原始备份；无输入及返回值。
#[test]
fn knowledge_retirement_preserves_values_backup_and_idempotence() {
    // Every source/backup file is isolated from the user's application directory.
    // 每个源及备份文件均与用户应用目录隔离。
    let directory = tempfile::tempdir().unwrap();
    // The synthetic old import has a nonzero timestamp that must remain recoverable.
    // 合成旧导入具有非零时间戳，必须保持可恢复。
    let path = fixture(directory.path());
    assert!(synchronize(&path, false).is_err());
    // A verified complete backup is mandatory even for explicitly retired rows.
    // 即使明确废弃的行也必须拥有已验证完整备份。
    let backup = synchronize(&path, true).unwrap().unwrap();
    // The current catalog contains only the new operation model.
    // 当前目录仅包含新操作模型。
    let current = Connection::open(&path).unwrap();
    assert!(
        !schema(&current)
            .unwrap()
            .contains_key("knowledge_import_jobs")
    );
    assert!(
        !columns(&current, "knowledge_libraries")
            .unwrap()
            .iter()
            .any(|(name, _)| name == "last_completed_import_at_ms")
    );
    assert_eq!(
        current
            .query_row(
                "SELECT description FROM knowledge_libraries WHERE id='library'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "purpose"
    );
    assert_eq!(
        current
            .query_row(
                "SELECT COUNT(*) FROM knowledge_library_project_bindings",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        current
            .query_row(
                "SELECT COUNT(*) FROM knowledge_operations WHERE id='current'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    // Old counters are not invented as extra new operations.
    // 不将旧计数虚构为额外新操作。
    assert_eq!(
        current
            .query_row("SELECT COUNT(*) FROM knowledge_operations", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    // Both removed objects retain their exact original values in the backup.
    // 两个移除对象在备份中均保留精确原值。
    let before = Connection::open(&backup).unwrap();
    assert_eq!(
        before
            .query_row(
                "SELECT last_completed_import_at_ms FROM knowledge_libraries",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        12345
    );
    assert_eq!(before.query_row("SELECT COUNT(*) FROM knowledge_import_jobs WHERE id='retired' AND finished_at_ms=5",[],|row|row.get::<_,i64>(0)).unwrap(),1);
    assert!(synchronize(&path, false).unwrap().is_none());
    assert!(synchronize(&path, true).unwrap().is_none());
}

/// Rejects active old jobs before writes/backups; takes no parameters and returns no value.
/// 在写入及备份之前拒绝旧活动任务；无参数及返回值。
#[test]
fn knowledge_retirement_refuses_queued_and_running_jobs() {
    for state in ["queued", "running"] {
        // Each state receives a fresh fully populated retired shape.
        // 每种状态使用全新且完整填充的废弃形状。
        let directory = tempfile::tempdir().unwrap();
        // The path is never the application-derived production path.
        // 路径绝非应用派生生产路径。
        let path = fixture(directory.path());
        // Update the exact job rather than fabricating a new schema branch.
        // 更新精确任务，不虚构新结构分支。
        let connection = Connection::open(&path).unwrap();
        connection
            .execute("UPDATE knowledge_import_jobs SET state=?", [state])
            .unwrap();
        assert!(
            synchronize(&path, true)
                .unwrap_err()
                .contains("queued or running")
        );
        assert!(
            schema(&connection)
                .unwrap()
                .contains_key("knowledge_import_jobs")
        );
        assert!(
            columns(&connection, "knowledge_libraries")
                .unwrap()
                .iter()
                .any(|(name, _)| name == "last_completed_import_at_ms")
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

/// Rejects unmapped columns, indexes and foreign-key dependencies without removing data.
/// 拒绝未映射列、索引及外键依赖，不移除数据。
/// Takes no parameters and returns no value; all cases must fail before a backup or mutation.
/// 无参数及返回值；全部情况必须在备份或变更前失败。
#[test]
fn knowledge_retirement_refuses_unknown_shape_and_dependencies() {
    for modification in [
        "ALTER TABLE knowledge_import_jobs ADD COLUMN future_metadata TEXT;",
        "CREATE INDEX unmanaged_job_index ON knowledge_import_jobs(state);",
        "CREATE TABLE retained_references(id TEXT REFERENCES knowledge_import_jobs(id));",
        "CREATE TABLE retained_references(id TEXT REFERENCES KNOWLEDGE_IMPORT_JOBS(id));",
    ] {
        // Unknown dependencies are independent synthetic cases.
        // 未知依赖为相互独立合成情况。
        let directory = tempfile::tempdir().unwrap();
        // The original source remains present on every refusal.
        // 每次拒绝后原始源均保持存在。
        let path = fixture(directory.path());
        // Execute only fixed synthetic DDL, never caller-generated SQL.
        // 仅执行固定合成 DDL，绝非调用方生成 SQL。
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(modification).unwrap();
        assert!(synchronize(&path, true).is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM knowledge_import_jobs", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

/// Retires a differently cased import table and its index, because SQLite table identity ignores ASCII case.
/// 退役大小写不同的导入表及其索引，因为 SQLite 表身份不区分 ASCII 大小写。
/// Takes no parameters and returns no value; the retired table must stay recoverable from the backup.
/// 无参数及返回值；退役表必须仍可从备份恢复。
#[test]
fn knowledge_retirement_matches_table_identity_case_insensitively() {
    // The case variant is isolated from every other fixture.
    // 大小写变体与其他夹具相互隔离。
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path());
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "ALTER TABLE knowledge_import_jobs RENAME TO retired_jobs_stage;
             ALTER TABLE retired_jobs_stage RENAME TO Knowledge_Import_Jobs;",
        )
        .unwrap();
    // backup keeps the renamed source table with its single historical row.
    // backup 保留已改名的源表及其唯一历史行。
    let backup = synchronize(&path, true).unwrap().unwrap();
    let current = Connection::open(&path).unwrap();
    assert!(
        !schema(&current)
            .unwrap()
            .keys()
            .any(|name| name.eq_ignore_ascii_case("knowledge_import_jobs"))
    );
    assert_eq!(
        Connection::open(&backup)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM knowledge_import_jobs", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    assert!(synchronize(&path, false).unwrap().is_none());
}

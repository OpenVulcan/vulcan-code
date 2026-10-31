//! One statement snapshot for all workspace locators and revision-checked stale-index pruning.
//! 单语句加载工作区全部定位器快照，并按已观察修订清理陈旧索引。

use super::{
    load_report::{WorkspaceIndexLoadIssue, WorkspaceIndexLoadStage},
    session_index_v1::SessionIndexV1Row,
};
use crate::{data_plane::HostRuntimeHandle, local_database_runtime::LogicalDatabase, local_sql};
use std::collections::{BTreeMap, BTreeSet};

/// One statement's decoded rows and explicit row-local failures; query failures remain errors.
/// 同一查询中成功解码的记录和明确的行级失败；查询级失败仍返回错误。
#[derive(Debug)]
pub(super) struct WorkspaceIndexRows {
    /// Valid records admitted for independent locator inspection.
    /// 已准入、可独立检查定位器的有效记录。
    pub rows: Vec<SessionIndexV1Row>,
    /// Admitted rows whose session carries a creation-frozen dedicated profile.
    /// 已准入记录中带有创建时冻结专职配置的会话。
    pub dedicated_session_ids: BTreeSet<String>,
    /// Display names of admitted dedicated sessions converted into floating assistants.
    /// 已准入、已转换为浮动助手的专职会话显示名称。
    pub assistant_names: BTreeMap<String, String>,
    /// Sidebar ordering facts of admitted sessions: latest request time and optional manual position.
    /// 已准入会话的侧栏排序事实：最近请求时间与可选手动位置。
    pub list_states: BTreeMap<String, SessionListState>,
    /// Corrupt records excluded from rows but retained for incomplete-load reporting.
    /// 未放入有效记录、但保留用于不完整加载报告的损坏记录。
    pub issues: Vec<WorkspaceIndexLoadIssue>,
}

/// Sidebar ordering facts stored beside one session locator.
/// 与一条会话定位记录一同保存的侧栏排序事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionListState {
    /// Time of the latest request in Unix milliseconds.
    /// 最近一次请求的时间（Unix 毫秒）。
    pub last_request_at_ms: i64,
    /// Manual-order position, absent until the user arranges the project.
    /// 手动排列位置；用户排列该项目之前不存在。
    pub manual_position: Option<i64>,
}

/// Read all lightweight rows with one connection and statement, never one query per project/session.
/// 用一个连接和语句读取全部轻量记录，绝不按项目或会话逐条查询。
pub(super) async fn list_workspace_rows(
    host: &HostRuntimeHandle,
) -> Result<WorkspaceIndexRows, String> {
    local_sql::with_connection_async(&host.local, LogicalDatabase::AppState, |connection| {
        read_rows(connection)
            .map_err(|error| format!("workspace session index query failed: {error}"))
    })
    .await
}

/// Reads one statement snapshot from connection; returns row diagnostics or a query-level failure.
/// 从 connection 读取单语句快照；返回行级诊断或查询级失败。
fn read_rows(connection: &rusqlite::Connection) -> rusqlite::Result<WorkspaceIndexRows> {
    // Retain a single SQL snapshot instead of retrying each project after a decode error.
    // 保留单次 SQL 快照，避免解码失败后逐项目重试。
    let mut statement = connection.prepare(
        "SELECT session_index_v1.session_id AS session_id, project_id, project_tag, storage_partition, relative_path, title, created_at_ms, updated_at_ms, archived_at_ms, unread, EXISTS (SELECT 1 FROM session_dedicated_index_v1 AS marker WHERE marker.session_id = session_index_v1.session_id) AS dedicated, (SELECT assistant.name FROM session_assistant_index_v1 AS assistant WHERE assistant.session_id = session_index_v1.session_id) AS assistant_name, list_state.last_request_at_ms AS last_request_at_ms, list_state.manual_position AS manual_position FROM session_index_v1 JOIN session_list_state_v1 AS list_state ON list_state.session_id = session_index_v1.session_id WHERE project_id IN (SELECT id FROM projects) ORDER BY session_index_v1.session_id"
    )?;
    // Cursor failures abort the snapshot; only typed decoding failures isolate one row.
    // 游标读取失败终止快照；仅强类型解码失败隔离单条记录。
    let mut cursor = statement.query([])?;
    // Accumulate healthy rows and diagnostics separately without losing either.
    // 分别累计正常记录和诊断，两者均不得丢弃。
    let mut report = WorkspaceIndexRows {
        rows: Vec::new(),
        dedicated_session_ids: BTreeSet::new(),
        assistant_names: BTreeMap::new(),
        list_states: BTreeMap::new(),
        issues: Vec::new(),
    };
    while let Some(row) = cursor.next()? {
        match decode_row(row).and_then(|decoded| {
            Ok((
                decoded,
                row.get::<_, bool>("dedicated")?,
                row.get::<_, Option<String>>("assistant_name")?,
                SessionListState {
                    last_request_at_ms: row.get("last_request_at_ms")?,
                    manual_position: row.get("manual_position")?,
                },
            ))
        }) {
            Ok((decoded, dedicated, assistant_name, list_state)) => {
                if dedicated {
                    report
                        .dedicated_session_ids
                        .insert(decoded.session_id.clone());
                }
                if let Some(name) = assistant_name {
                    report
                        .assistant_names
                        .insert(decoded.session_id.clone(), name);
                }
                report
                    .list_states
                    .insert(decoded.session_id.clone(), list_state);
                report.rows.push(decoded);
            }
            Err(error) => report.issues.push(WorkspaceIndexLoadIssue {
                stage: WorkspaceIndexLoadStage::Index,
                // Corruption can affect identity columns too; missing diagnostic identities
                // are represented explicitly and never reconstructed from unrelated fields.
                // 损坏也可能影响身份列；缺失的诊断身份显式留空，不从其他字段重建。
                project_id: row.get::<_, String>("project_id").ok(),
                session_id: row.get::<_, String>("session_id").ok(),
                detail: format!("workspace session index row decode failed: {error}"),
            }),
        }
    }
    Ok(report)
}

/// Decodes the declared index columns from row, preserving SQLite's exact type error.
/// 从 row 解码已声明的索引列，保留 SQLite 精确的类型错误。
fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionIndexV1Row> {
    Ok(SessionIndexV1Row {
        session_id: row.get("session_id")?,
        project_id: row.get("project_id")?,
        project_tag: row.get("project_tag")?,
        storage_partition: row.get("storage_partition")?,
        relative_path: row.get("relative_path")?,
        title: row.get("title")?,
        created_at_ms: row.get("created_at_ms")?,
        updated_at_ms: row.get("updated_at_ms")?,
        archived_at_ms: row.get("archived_at_ms")?,
        unread: row.get("unread")?,
    })
}

/// Remove only rows that still equal the exact missing-file observation; never delete a newer locator.
/// 只删除仍与缺失文件观察完全一致的索引行，绝不删除较新的定位器。
pub(super) async fn prune_observed_missing_rows(
    host: &HostRuntimeHandle,
    rows: Vec<SessionIndexV1Row>,
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    local_sql::with_connection_async(&host.local, LogicalDatabase::AppState, move |connection| {
        prune_rows(connection, &rows)
            .map_err(|error| format!("workspace stale index cleanup failed: {error}"))
    })
    .await
}

fn prune_rows(
    connection: &mut rusqlite::Connection,
    rows: &[SessionIndexV1Row],
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    {
        let mut statement = transaction.prepare(
            "DELETE FROM session_index_v1 WHERE session_id=?1 AND project_id=?2 AND project_tag=?3 AND storage_partition=?4 AND relative_path=?5 AND title=?6 AND created_at_ms=?7 AND updated_at_ms=?8 AND archived_at_ms IS ?9 AND unread=?10"
        )?;
        for row in rows {
            if statement.execute(rusqlite::params![
                row.session_id,
                row.project_id,
                row.project_tag,
                row.storage_partition,
                row.relative_path,
                row.title,
                row.created_at_ms,
                row.updated_at_ms,
                row.archived_at_ms,
                row.unread
            ])? != 1
            {
                // The transaction rolls back; a later refresh must observe the changed revision.
                // 事务将回滚；后续刷新必须观察已经变化的修订。
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::super::session_index_v1::{
        SESSION_INDEX_V1_SCHEMA_SQL, write_session_assistant_v1_rows,
    };
    use super::*;

    fn fixture() -> rusqlite::Connection {
        let connection = rusqlite::Connection::open_in_memory().expect("in-memory index");
        connection
            .execute_batch(SESSION_INDEX_V1_SCHEMA_SQL)
            .expect("production index schema");
        connection.execute_batch("CREATE TABLE projects (id TEXT PRIMARY KEY); INSERT INTO projects VALUES ('one'), ('two'); INSERT INTO session_index_v1 VALUES
            ('a','one','tag','sessions','sessions/tag/session-a','active',1,2,NULL,0),
            ('b','two','tag','sessions','sessions/tag/session-b','archived',1,3,3,1);
            INSERT INTO session_list_state_v1 VALUES ('a', 2, NULL), ('b', 3, 0);").expect("rows");
        connection
    }

    /// Active and archived rows from different projects belong to the same statement result.
    /// 不同项目的活动与归档记录属于同一次语句结果。
    #[test]
    fn one_query_retains_cross_project_rows_and_archive_state() {
        let connection = fixture();
        let report = read_rows(&connection).expect("typed rows");
        assert!(report.issues.is_empty());
        let rows = report.rows;
        assert_eq!(rows.len(), 2);
        let archived = rows
            .iter()
            .find(|row| row.session_id == "b")
            .expect("archived identity");
        assert_eq!(archived.project_id, "two");
        assert_eq!(archived.archived_at_ms, Some(3));
        assert_eq!(archived.unread, 1);
        assert_eq!(
            report.list_states.get("b"),
            Some(&SessionListState {
                last_request_at_ms: 3,
                manual_position: Some(0)
            })
        );
    }

    /// Only marked sessions are reported as dedicated, and pruning a row removes its marker.
    /// 只有带标记的会话被报告为专职会话，清理记录时同时移除其标记。
    #[test]
    fn dedicated_marker_follows_its_locator_row() {
        let mut connection = fixture();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; INSERT INTO session_dedicated_index_v1 VALUES ('a')",
            )
            .expect("mark dedicated session");
        let report = read_rows(&connection).expect("typed rows");
        assert!(report.issues.is_empty());
        assert_eq!(
            report.dedicated_session_ids,
            BTreeSet::from([String::from("a")])
        );
        prune_rows(&mut connection, &report.rows).expect("prune observed rows");
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM session_dedicated_index_v1",
                [],
                |row| row.get(0),
            )
            .expect("count markers");
        assert_eq!(
            remaining, 0,
            "a pruned locator must not leave its dedicated marker"
        );
    }

    /// Converting projects the dedicated kind with the name, restoring clears only the name,
    /// an unindexed session is refused, and pruning the locator removes the assistant row.
    /// 转换时同时投影专职种类与名称，还原只清除名称，未索引会话被拒绝，清理定位记录时移除助手记录。
    #[test]
    fn assistant_row_follows_its_dedicated_marker() {
        let mut connection = fixture();
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .expect("enforce foreign keys");
        write_session_assistant_v1_rows(&mut connection, "a", Some("Helper"))
            .expect("convert indexed session");
        write_session_assistant_v1_rows(&mut connection, "a", Some("Renamed"))
            .expect("rename assistant");
        let report = read_rows(&connection).expect("typed rows");
        assert_eq!(
            report.dedicated_session_ids,
            BTreeSet::from([String::from("a")])
        );
        assert_eq!(
            report.assistant_names,
            BTreeMap::from([(String::from("a"), String::from("Renamed"))])
        );
        assert!(
            write_session_assistant_v1_rows(&mut connection, "missing", Some("Ghost")).is_err(),
            "an unindexed session cannot become an assistant"
        );
        write_session_assistant_v1_rows(&mut connection, "a", None).expect("restore session");
        let restored = read_rows(&connection).expect("restored rows");
        assert!(restored.assistant_names.is_empty());
        assert_eq!(restored.dedicated_session_ids.len(), 1);
        write_session_assistant_v1_rows(&mut connection, "a", Some("Again"))
            .expect("convert again");
        prune_rows(&mut connection, &restored.rows).expect("prune observed rows");
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM session_assistant_index_v1",
                [],
                |row| row.get(0),
            )
            .expect("count assistants");
        assert_eq!(
            remaining, 0,
            "a pruned locator must not leave its assistant row"
        );
    }

    /// A newer locator or lifecycle prevents stale deletion and rolls back earlier batch removals.
    /// 较新的定位器或生命周期阻止陈旧删除，并回滚本批次此前的删除。
    #[test]
    fn stale_prune_never_deletes_a_newer_row_or_partially_commits() {
        let mut connection = fixture();
        let observed = read_rows(&connection).expect("observed rows").rows;
        connection
            .execute(
                "UPDATE session_index_v1 SET title='new', updated_at_ms=9 WHERE session_id='b'",
                [],
            )
            .expect("concurrent update");
        assert!(prune_rows(&mut connection, &observed).is_err());
        assert_eq!(read_rows(&connection).expect("retained rows").rows.len(), 2);
        let latest = read_rows(&connection).expect("latest observation").rows;
        prune_rows(&mut connection, &latest).expect("matching observations may be removed");
        assert!(read_rows(&connection).expect("empty index").rows.is_empty());
    }

    /// Tombstoned owners are excluded before decoding their obsolete rows.
    /// 在解码旧记录前排除已经墓碑化的所属项目。
    #[test]
    fn removed_project_rows_cannot_poison_a_retained_project_list() {
        let connection = fixture();
        connection
            .execute("DELETE FROM projects WHERE id='one'", [])
            .expect("remove project");
        connection
            .execute(
                "UPDATE session_index_v1 SET updated_at_ms='broken' WHERE session_id='a'",
                [],
            )
            .expect("obsolete row");
        let report = read_rows(&connection).expect("only retained project rows");
        assert!(report.issues.is_empty());
        let rows = report.rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.first().expect("remaining identity").session_id, "b");
    }

    /// Corrupt rows retain diagnostics while healthy rows from other projects remain loadable.
    /// 损坏记录保留诊断，同时其他项目的正常记录仍可加载。
    #[test]
    fn malformed_current_row_preserves_healthy_rows_and_reports_failure() {
        let connection = fixture();
        connection
            .execute(
                "UPDATE session_index_v1 SET updated_at_ms='broken' WHERE session_id='a'",
                [],
            )
            .expect("corrupt fixture");
        let report = read_rows(&connection).expect("row corruption is not a query failure");
        assert_eq!(report.rows.len(), 1);
        assert!(report.rows.iter().any(|row| row.session_id == "b"));
        assert_eq!(report.issues.len(), 1);
        let issue = report
            .issues
            .iter()
            .find(|issue| issue.session_id.as_deref() == Some("a"))
            .expect("corrupt session must have an explicit diagnostic");
        assert_eq!(issue.project_id.as_deref(), Some("one"));
        assert_eq!(issue.stage, WorkspaceIndexLoadStage::Index);
        assert!(issue.detail.contains("updated_at_ms"), "{}", issue.detail);
    }

    /// Query failures cannot masquerade as a successful empty index.
    /// 查询级失败不能伪装成成功读取的空索引。
    #[test]
    fn missing_table_remains_a_query_failure() {
        // Remove the authoritative table in an isolated database fixture.
        // 在隔离数据库夹具中移除权威索引表。
        let connection = fixture();
        connection
            .execute_batch("DROP TABLE session_index_v1")
            .expect("remove index fixture");
        assert!(read_rows(&connection).is_err());
    }
}

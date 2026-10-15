//! Explicit removal mapping for the retired import pipeline, excluded from application startup.
//! 已废弃导入流水线的明确移除映射，不进入应用启动。

use super::*;

/// Verifies `connection`'s retired job shape and absence of active work; returns an explicit refusal for unknown data.
/// 验证 `connection` 的废弃任务形状及无活动工作；对未知数据返回明确拒绝。
/// Historical rows are never converted; the caller preserves the complete table in a verified backup before removal.
/// 绝不转换历史行；调用方在移除前将整表保存在已验证备份中。
pub(super) fn validate_retired_knowledge_jobs(connection: &Connection) -> Result<(), String> {
    // This is a removal mapping for the verified old DDL, not an alternate authoritative application schema.
    // 这是已查证旧 DDL 的移除映射，并非另一份应用权威结构。
    let expected = [
        ("id", "TEXT"),
        ("library_id", "TEXT"),
        ("source_root", "TEXT"),
        ("operation_kind", "TEXT"),
        ("state", "TEXT"),
        ("phase", "TEXT"),
        ("scanned_files", "INTEGER"),
        ("total_files", "INTEGER"),
        ("processed_files", "INTEGER"),
        ("total_index_items", "INTEGER"),
        ("processed_index_items", "INTEGER"),
        ("current_file", "TEXT"),
        ("changed_files", "INTEGER"),
        ("unchanged_files", "INTEGER"),
        ("deleted_files", "INTEGER"),
        ("skipped_files", "INTEGER"),
        ("processed_bytes", "INTEGER"),
        ("warnings_json", "TEXT"),
        ("observability_json", "TEXT"),
        ("error_message", "TEXT"),
        ("created_at_ms", "INTEGER"),
        ("started_at_ms", "INTEGER"),
        ("finished_at_ms", "INTEGER"),
    ]
    .into_iter()
    .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
    .collect::<Vec<_>>();
    if columns(connection, "knowledge_import_jobs")? != expected {
        return Err("Unmapped retired knowledge import job columns; no data was removed.".into());
    }
    // Active jobs require their old owner to stop or settle them; counts are never guessed into the new model.
    // 活动任务需要旧所有者停止或结算；绝不将计数推测映射到新模型。
    let active: bool = sql(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_import_jobs WHERE state IN ('queued','running'));",
        [],
        |row| row.get(0),
    ))?;
    if active {
        return Err(
            "Retired knowledge import jobs are queued or running; synchronization was refused."
                .into(),
        );
    }
    Ok(())
}

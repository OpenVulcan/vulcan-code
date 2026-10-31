use super::*;

/// Queries every active and archived project row ordered by pinned section and stable order.
/// 查询按置顶分区与稳定顺序排序的全部活动及归档项目行。
pub(super) async fn query_project_rows(
    host_runtime: &HostRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = local_sql::query_json(
        host_runtime.local.as_ref(),
        LogicalDatabase::AppState,
        r#"
SELECT p.id, p.name, p.root_path, p.pinned, p.sort_order, p.icon_pack, p.icon_key,
       p.archived, preference.session_sort_mode
FROM projects p
LEFT JOIN project_session_preferences preference ON preference.project_id = p.id
ORDER BY p.pinned DESC, p.sort_order ASC, p.updated_at DESC;
"#,
        &[],
    )
    .map_err(|error| format!("Failed to query the workbench project list: {error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries the persisted collapsed project identifiers, if a workspace UI state row exists.
/// 查询已持久化的折叠项目标识集合；若工作台 UI 状态行不存在则返回空。
pub(super) async fn query_collapsed_task_project_ids(
    host_runtime: &HostRuntimeHandle,
) -> Result<Option<BTreeSet<String>>, String> {
    let result = local_sql::query_json(
        host_runtime.local.as_ref(),
        LogicalDatabase::AppState,
        "SELECT value FROM app_metadata WHERE key = ?1;",
        &[SqliteValue::Text(
            COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string(),
        )],
    )
    .map_err(|error| format!("Failed to query the workbench project expansion state: {error}"))?;

    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&row.value).map_err(|error| {
        format!("Failed to parse the workbench project expansion state: {error}")
    })?;
    Ok(Some(parsed.into_iter().collect()))
}

/// Decodes one engine JSON row-set payload into typed Rust rows.
/// 将一条引擎 JSON 行集载荷解码为带类型的 Rust 行集合。
pub(super) fn decode_query_rows<T>(json_data: &str) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<Vec<T>>(json_data)
        .map_err(|error| format!("Failed to parse the host runtime SQLite query result: {error}"))
}

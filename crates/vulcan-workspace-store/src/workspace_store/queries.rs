use super::*;

pub(super) fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
}

/// Queries every active and archived project row ordered by pinned section and stable order.
/// 查询按置顶分区与稳定顺序排序的全部活动及归档项目行。
pub(super) async fn query_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT p.id, p.name, p.root_path, p.pinned, p.sort_order, p.icon_pack, p.icon_key,
       p.codebase_memory_enabled, p.archived,
       v.enabled AS vce_enabled,
       v.revision AS vce_revision,
       v.migrated_from_codebase_memory AS vce_migrated_from_codebase_memory,
       v.max_file_bytes AS vce_max_file_bytes,
       v.extra_ignore_patterns_json AS vce_extra_ignore_patterns_json,
       v.allow_query_rewrite AS vce_allow_query_rewrite,
       v.allow_rerank AS vce_allow_rerank,
       v.embedding_max_concurrency AS vce_embedding_max_concurrency,
       v.query_max_concurrency AS vce_query_max_concurrency,
       v.max_results AS vce_max_results,
       v.max_context_chars AS vce_max_context_chars
FROM projects p
JOIN project_vce_settings v ON v.project_id = p.id
ORDER BY p.pinned DESC, p.sort_order ASC, p.updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries the persisted collapsed project identifiers, if a workspace UI state row exists.
/// 查询已持久化的折叠项目标识集合；若工作台 UI 状态行不存在则返回空。
pub(super) async fn query_collapsed_task_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<Option<BTreeSet<String>>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT value FROM app_metadata WHERE key = ?1;",
            vec![ControllerSqliteValue::String(
                COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string(),
            )],
        )
        .await
        .map_err(|error| format!("查询工作台项目展开状态失败：{error}"))?;

    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&row.value)
        .map_err(|error| format!("解析工作台项目展开状态失败：{error}"))?;
    Ok(Some(parsed.into_iter().collect()))
}

/// Decodes one controller JSON row-set payload into typed Rust rows.
/// 将一条控制器 JSON 行集载荷解码为带类型的 Rust 行集合。
pub(super) fn decode_query_rows<T>(json_data: &str) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<Vec<T>>(json_data)
        .map_err(|error| format!("解析控制器 SQLite 查询结果失败：{error}"))
}

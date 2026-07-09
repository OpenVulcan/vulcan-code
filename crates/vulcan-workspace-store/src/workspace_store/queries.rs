use super::*;

pub(super) fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
}

/// Queries visible project rows ordered by pinned section and stable order.
/// 查询按置顶分区与稳定顺序排序后的可见项目行。
pub(super) async fn query_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, name, root_path, pinned, sort_order, icon_pack, icon_key, archived
FROM projects
ORDER BY pinned DESC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries lightweight session locator rows ordered without scanning segmented directories.
/// 查询轻量会话定位行，不扫描分段目录。
pub(super) async fn query_session_index_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionIndexRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT
    id,
    project_id,
    project_tag,
    title,
    title_source,
    archived,
    created_at,
    updated_at
FROM workspace_session_index
ORDER BY archived ASC, project_id ASC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台会话定位索引失败：{error}"))?;

    decode_query_rows::<StoredSessionIndexRow>(&result.json_data)
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

/// Queries active project identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活项目标识集合。
pub(super) async fn query_active_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<BTreeSet<String>, String> {
    #[derive(Deserialize)]
    struct IdRow {
        /// Persisted identifier.
        /// 持久化标识。
        id: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT id FROM projects WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询激活项目标识失败：{error}"))?;

    Ok(decode_query_rows::<IdRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// Returns whether one persisted record kind should be part of the default transcript window.
/// 返回某个持久化记录类型是否应进入默认转录窗口。
#[cfg(test)]
pub(super) fn persisted_kind_in_default_transcript_window(kind: &str) -> bool {
    !matches!(
        kind,
        "tool-call-output" | "tool-result-output" | "tool-detail-output"
    )
}

/// Returns whether one persisted record kind belongs to the on-demand tool-detail stream.
/// 返回某个持久化记录类型是否属于按需工具明细流。
#[cfg(test)]
pub(super) fn persisted_kind_in_tool_detail_stream(kind: &str) -> bool {
    matches!(
        kind,
        "tool-call-output" | "tool-result-output" | "tool-detail-output"
    )
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

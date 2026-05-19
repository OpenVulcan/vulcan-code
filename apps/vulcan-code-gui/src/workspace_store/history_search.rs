//! Workspace-history search helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的工作台历史搜索辅助逻辑。

use super::*;

/// Searches visible workspace history with SQLite FTS5 and BM25 scoring.
/// 使用 SQLite FTS5 与 BM25 评分搜索可见工作台历史。
pub async fn search_workspace_history(
    controller: ControllerRuntimeHandle,
    project_id: Option<String>,
    query: String,
    requested_limit: usize,
    offset: usize,
) -> Result<WorkspaceHistorySearchPage, String> {
    let query = normalize_session_history_search_query(&query);
    if query.is_empty() {
        return Ok(WorkspaceHistorySearchPage {
            query,
            hits: Vec::new(),
            limit_reached: false,
            total_hits: 0,
            next_offset: 0,
            has_more: false,
        });
    }

    let limit = requested_limit
        .max(1)
        .min(SESSION_HISTORY_SEARCH_RESULT_LIMIT);
    let scoped_project_id = project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let index_name = scoped_project_id
        .map(workspace_history_search_project_index_name)
        .unwrap_or_else(|| WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME.to_string());
    ensure_workspace_history_search_index(&controller, &index_name).await?;
    let search_result = controller
        .client
        .search_sqlite_fts(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            index_name.clone(),
            ControllerSqliteTokenizerMode::Jieba,
            query.clone(),
            limit as u32,
            offset as u32,
        )
        .await
        .map_err(|error| format!("搜索工作台历史失败：{error}"))?;
    let message_ids = search_result
        .hits
        .iter()
        .map(|hit| hit.id.clone())
        .collect::<Vec<_>>();
    let metadata_rows =
        query_workspace_history_search_metadata_rows(&controller, &message_ids).await?;
    let metadata_by_message_id = metadata_rows
        .into_iter()
        .map(|row| (row.message_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let hits = search_result
        .hits
        .into_iter()
        .filter_map(|hit| {
            metadata_by_message_id
                .get(&hit.id)
                .map(|row| stored_workspace_history_search_hit(row, &hit))
        })
        .collect::<Vec<_>>();

    Ok(WorkspaceHistorySearchPage {
        query,
        total_hits: search_result.total as usize,
        next_offset: offset.saturating_add(hits.len()),
        has_more: offset.saturating_add(hits.len()) < search_result.total as usize,
        hits,
        limit_reached: search_result.total >= limit as u64,
    })
}

/// Queries workspace-history metadata rows for a bounded list of matched message identifiers.
/// 为一组有界命中的消息标识查询工作台历史元数据行。
async fn query_workspace_history_search_metadata_rows(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchMetadataRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (1..=message_ids.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT
    p.name AS project_name,
    s.id AS session_id,
    s.title AS session_title,
    m.id AS message_id,
    m.ordinal,
    m.turn_index,
    m.role,
    m.created_at
FROM session_messages AS m
INNER JOIN sessions AS s ON s.id = m.session_id
INNER JOIN projects AS p ON p.id = s.project_id
WHERE m.id IN ({placeholders})
  AND s.archived = 0
  AND p.archived = 0;
"#
    );
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            message_ids
                .iter()
                .cloned()
                .map(ControllerSqliteValue::String)
                .collect(),
        )
        .await
        .map_err(|error| format!("查询工作台历史搜索元数据失败：{error}"))?;

    decode_query_rows::<StoredWorkspaceHistorySearchMetadataRow>(&result.json_data)
}

/// Converts one metadata row plus one controller FTS hit into the public workspace-history search-hit shape.
/// 将一条元数据行与一条控制器 FTS 命中转换为公开的工作台历史搜索命中结构。
fn stored_workspace_history_search_hit(
    row: &StoredWorkspaceHistorySearchMetadataRow,
    search_hit: &ControllerSqliteSearchFtsHit,
) -> WorkspaceHistorySearchHit {
    let preview_source = if !search_hit.content_snippet.trim().is_empty() {
        search_hit.content_snippet.as_str()
    } else if !search_hit.title_highlight.trim().is_empty() {
        search_hit.title_highlight.as_str()
    } else {
        search_hit.title.as_str()
    };
    WorkspaceHistorySearchHit {
        project_name: row.project_name.clone(),
        session_id: parse_session_id(&row.session_id).unwrap_or_default(),
        session_title: row.session_title.clone(),
        message_id: row.message_id.clone(),
        ordinal: row.ordinal,
        turn_index: row.turn_index,
        message_kind: session_kind_from_role(&row.role),
        preview: workspace_history_search_preview(preview_source),
        preview_markup: workspace_history_search_preview_markup(preview_source),
        created_at: row.created_at.clone(),
        score: search_hit.score,
    }
}

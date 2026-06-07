//! Workspace-history search is intentionally disabled while the segmented session kernel stabilizes.
//! 在分段会话内核稳定前，工作台历史搜索会被有意下线。

use super::*;

/// Returns an empty history-search page without touching persistence or auxiliary indexes.
/// 返回空的历史搜索页，且不触碰持久化内容或辅助索引。
pub async fn search_workspace_history(
    _controller: ControllerRuntimeHandle,
    _project_id: Option<String>,
    query: String,
    _requested_limit: usize,
    _offset: usize,
) -> Result<WorkspaceHistorySearchPage, String> {
    let query = query
        .trim()
        .chars()
        .take(SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS)
        .collect::<String>();
    Ok(WorkspaceHistorySearchPage {
        query,
        hits: Vec::new(),
        limit_reached: false,
        total_hits: 0,
        next_offset: 0,
        has_more: false,
    })
}

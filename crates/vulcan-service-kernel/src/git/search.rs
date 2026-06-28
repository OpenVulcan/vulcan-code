//! Backend Git diff search logic for WebUI review surfaces.
//! WebUI 审查界面的后端 Git diff 搜索逻辑。

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use super::diff::{
    load_webui_branch_git_diff, load_webui_commit_git_diff, load_webui_current_git_branch,
    load_webui_default_git_branch, load_webui_recent_commits, load_webui_staged_git_diff,
    load_webui_unstaged_git_diff, load_webui_untracked_git_diff, resolve_webui_git_repository_root,
};
use super::diff_text::{
    for_each_webui_unified_diff_line, truncate_string_to_char_boundary,
    webui_unified_diff_file_summaries, webui_unified_diff_line_count,
};
use super::model::{
    WEBUI_DIFF_SEARCH_MAX_RESULTS, WebuiWorkspaceDiffFileSummary, WebuiWorkspaceDiffMode,
    WebuiWorkspaceDiffRequest, WebuiWorkspaceDiffSearchFileMatch, WebuiWorkspaceDiffSearchRequest,
    WebuiWorkspaceDiffSearchResponse, default_webui_workspace_diff_request,
};
use crate::{current_unix_millis_value, normalized_workspace_path_text_from_path, workspace_root};

/// Maximum bytes kept for one compact diff search preview line.
/// 单条紧凑 diff 搜索预览行保留的最大字节数。
const WEBUI_DIFF_SEARCH_PREVIEW_MAX_BYTES: usize = 180;

/// Maximum compact matching previews returned for one file-level backend diff search hit.
/// 单个后端 diff 文件级命中返回的紧凑预览最大数量。
const WEBUI_DIFF_SEARCH_MATCH_PREVIEW_LIMIT: usize = 4;

/// Character width used by the compact diff search n-gram inverted index.
/// 紧凑 diff 搜索 n-gram 倒排索引使用的字符宽度。
const WEBUI_DIFF_SEARCH_NGRAM_WIDTH: usize = 3;

/// Time-to-live for compact WebUI diff search responses.
/// 紧凑 WebUI diff 搜索响应的缓存存活时间。
const WEBUI_DIFF_SEARCH_CACHE_TTL_MS: i128 = 2_500;

/// Time-to-live for reusable WebUI diff search inputs.
/// 可复用 WebUI diff 搜索输入的缓存存活时间。
const WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS: i128 = 2_500;

/// Maximum compact WebUI diff search responses retained in memory.
/// 内存中保留的紧凑 WebUI diff 搜索响应最大数量。
const WEBUI_DIFF_SEARCH_CACHE_LIMIT: usize = 24;

/// Maximum reusable WebUI diff search inputs retained in memory.
/// 内存中保留的可复用 WebUI diff 搜索输入最大数量。
const WEBUI_DIFF_SEARCH_INPUT_CACHE_LIMIT: usize = 8;

mod cache;
mod index;
mod input;

pub(crate) use cache::*;
pub(crate) use index::*;
pub(crate) use input::*;

#[cfg(test)]
mod tests;

/// Searches a complete backend diff without sending the full text to the WebUI.
/// 在不把完整文本发送给 WebUI 的情况下搜索完整后端 diff。
pub(crate) fn load_webui_workspace_diff_search(
    workspace_path: &str,
    request: Option<WebuiWorkspaceDiffRequest>,
    search: WebuiWorkspaceDiffSearchRequest,
) -> Result<WebuiWorkspaceDiffSearchResponse, String> {
    let requested_path = PathBuf::from(workspace_path.trim());
    let workspace_path = if requested_path.as_os_str().is_empty() {
        workspace_root()
    } else {
        requested_path
    };
    let workspace_path = fs::canonicalize(&workspace_path).map_err(|error| {
        format!("failed to resolve workspace path `{workspace_path:?}`: {error}")
    })?;
    let repository_root_text = normalized_workspace_path_text_from_path(&workspace_path);
    let query = search.query.trim().to_string();
    if query.is_empty() {
        return Ok(empty_webui_diff_search_response(
            repository_root_text,
            query,
            String::from("搜索词为空，未执行 Git diff 搜索。"),
        ));
    }
    let Some(repository_root) = resolve_webui_git_repository_root(&workspace_path)? else {
        return Ok(empty_webui_diff_search_response(
            repository_root_text,
            query,
            String::from("当前工作区不是 Git 仓库，无法搜索代码审查 diff。"),
        ));
    };

    let request = request.unwrap_or_else(default_webui_workspace_diff_request);
    let limit = normalized_webui_diff_search_limit(search.limit);
    let repository_root_text = normalized_workspace_path_text_from_path(&repository_root);
    let cache_key = webui_diff_search_cache_key(&repository_root_text, &request, &query, limit);
    let cache_now_ms = current_unix_millis_value();
    if let Some(response) = cached_webui_diff_search_response(&cache_key, cache_now_ms) {
        return Ok(response);
    }
    let current_branch = load_webui_current_git_branch(&repository_root)?;
    let default_branch = load_webui_default_git_branch(&repository_root)?;
    let recent_commits = load_webui_recent_commits(&repository_root)?;
    let selected_base_branch = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_branch.clone());
    let selected_compare_branch = request
        .compare_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| default_branch.clone());
    let selected_commit_hash = request
        .commit_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| recent_commits.first().map(|commit| commit.hash.clone()));
    let input_cache_key = webui_diff_search_input_cache_key(
        &repository_root_text,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
    );
    let input = if let Some(input) = cached_webui_diff_search_input(&input_cache_key, cache_now_ms)
    {
        input
    } else {
        let input = load_webui_workspace_diff_search_input(
            &repository_root,
            request.mode,
            selected_base_branch.as_deref(),
            selected_compare_branch.as_deref(),
            selected_commit_hash.as_deref(),
        )?;
        store_webui_diff_search_input(input_cache_key, cache_now_ms, input.clone());
        input
    };
    let result = webui_diff_search_index_file_matches(
        &input.indexed_files,
        &query,
        limit,
        input.raw_line_count,
    );
    let notice = if input.unified_diff.trim().is_empty() {
        Some(String::from("当前工作区没有可搜索的 Git 变更。"))
    } else if input.notices.is_empty() {
        None
    } else {
        Some(input.notices.join(" "))
    };

    let response = WebuiWorkspaceDiffSearchResponse {
        repository_root: repository_root_text,
        query,
        matches: result.matches,
        truncated: result.truncated,
        scanned_files: result.scanned_files,
        scanned_lines: result.scanned_lines,
        notice,
    };
    store_webui_diff_search_response(cache_key, cache_now_ms, response.clone());
    Ok(response)
}

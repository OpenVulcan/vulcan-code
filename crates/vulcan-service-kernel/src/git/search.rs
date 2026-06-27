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

/// Process-local compact diff search response cache.
/// 进程内紧凑 diff 搜索响应缓存。
static WEBUI_DIFF_SEARCH_CACHE: OnceLock<Mutex<VecDeque<WebuiDiffSearchCacheEntry>>> =
    OnceLock::new();

/// Process-local reusable diff search input cache.
/// 进程内可复用 diff 搜索输入缓存。
static WEBUI_DIFF_SEARCH_INPUT_CACHE: OnceLock<Mutex<VecDeque<WebuiDiffSearchInputCacheEntry>>> =
    OnceLock::new();

/// One compact diff search response cache entry.
/// 单条紧凑 diff 搜索响应缓存项。
struct WebuiDiffSearchCacheEntry {
    /// Stable search cache key.
    /// 稳定搜索缓存键。
    key: String,
    /// Cache creation timestamp in milliseconds.
    /// 缓存创建时间戳，单位毫秒。
    created_at_ms: i128,
    /// Cached compact search response.
    /// 已缓存的紧凑搜索响应。
    response: WebuiWorkspaceDiffSearchResponse,
}

/// Reusable complete diff input for backend summary search.
/// 后端摘要搜索可复用的完整 diff 输入。
#[derive(Clone)]
struct WebuiDiffSearchInput {
    /// Complete unified diff text used by the backend scanner.
    /// 后端扫描器使用的完整 unified diff 文本。
    unified_diff: String,
    /// File-level candidate index precomputed from the complete diff text.
    /// 从完整 diff 文本预先计算出的文件级候选索引。
    indexed_files: Vec<WebuiDiffSearchIndexedFile>,
    /// Number of raw diff lines in the complete diff text.
    /// 完整 diff 文本中的原始 diff 行数。
    raw_line_count: usize,
    /// User-facing notices produced while loading the diff input.
    /// 加载 diff 输入时产生的面向用户提示。
    notices: Vec<String>,
}

/// Precomputed search candidates for one diff file.
/// 单个 diff 文件的预计算搜索候选项。
#[derive(Clone)]
struct WebuiDiffSearchIndexedFile {
    /// File summary already computed by the summary scanner.
    /// 摘要扫描器已经计算出的文件摘要。
    summary: WebuiWorkspaceDiffFileSummary,
    /// Candidate previews matched by later query scans.
    /// 后续 query 扫描使用的候选预览。
    candidates: Vec<WebuiDiffSearchIndexedCandidate>,
    /// Per-file n-gram inverted index from normalized text to candidate indexes.
    /// 从归一化文本到候选下标的文件级 n-gram 倒排索引。
    ngram_index: BTreeMap<String, Vec<usize>>,
}

/// Precomputed search candidate for one diff row or file path.
/// 单条 diff 行或文件路径的预计算搜索候选项。
#[derive(Clone)]
struct WebuiDiffSearchIndexedCandidate {
    /// Original compact preview sent back when this candidate is first matched.
    /// 该候选首次命中时返回的原始紧凑预览。
    preview: String,
    /// Lowercase compact preview used for repeated case-insensitive matching.
    /// 用于重复大小写不敏感匹配的小写紧凑预览。
    normalized_preview: String,
    /// Optional diff row position for the candidate.
    /// 候选项可选的 diff 行位置。
    position: Option<WebuiDiffSearchLinePosition>,
}

/// Mutable index builder used while scanning one file patch.
/// 扫描单个文件 patch 时使用的可变索引构建器。
struct WebuiDiffSearchIndexedFileBuilder {
    /// File summary already computed by the summary scanner.
    /// 摘要扫描器已经计算出的文件摘要。
    summary: WebuiWorkspaceDiffFileSummary,
    /// Candidate previews collected for this file.
    /// 当前文件已收集的候选预览。
    candidates: Vec<WebuiDiffSearchIndexedCandidate>,
    /// Current hunk index while scanning the file.
    /// 扫描文件时当前所在的 hunk 索引。
    current_hunk_index: Option<usize>,
    /// Current old-file line number while scanning a hunk.
    /// 扫描 hunk 时当前旧文件行号。
    current_old_line: usize,
    /// Current new-file line number while scanning a hunk.
    /// 扫描 hunk 时当前新文件行号。
    current_new_line: usize,
    /// Number of hunks already seen in this file.
    /// 当前文件中已经遇到的 hunk 数量。
    seen_hunk_count: usize,
}

/// One reusable diff search input cache entry.
/// 单条可复用 diff 搜索输入缓存项。
struct WebuiDiffSearchInputCacheEntry {
    /// Stable input cache key.
    /// 稳定输入缓存键。
    key: String,
    /// Cache creation timestamp in milliseconds.
    /// 缓存创建时间戳，单位毫秒。
    created_at_ms: i128,
    /// Cached complete diff search input.
    /// 已缓存的完整 diff 搜索输入。
    input: WebuiDiffSearchInput,
}

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

/// Loads the complete diff input used by backend summary search.
/// 加载后端摘要搜索使用的完整 diff 输入。
fn load_webui_workspace_diff_search_input(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
) -> Result<WebuiDiffSearchInput, String> {
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            let unstaged_diff = load_webui_unstaged_git_diff(repository_root)?;
            if !unstaged_diff.trim().is_empty() {
                sections.push(unstaged_diff);
            }
            let untracked_diff = load_webui_untracked_git_diff(repository_root, &mut notices)?;
            if !untracked_diff.trim().is_empty() {
                sections.push(untracked_diff);
            }
        }
        WebuiWorkspaceDiffMode::Staged => {
            let staged_diff = load_webui_staged_git_diff(repository_root)?;
            if !staged_diff.trim().is_empty() {
                sections.push(staged_diff);
            }
        }
        WebuiWorkspaceDiffMode::Commit => {
            if let Some(commit_hash) = selected_commit_hash {
                let commit_diff = load_webui_commit_git_diff(repository_root, commit_hash)?;
                if !commit_diff.trim().is_empty() {
                    sections.push(commit_diff);
                }
            } else {
                notices.push(String::from("当前仓库没有可搜索的提交记录。"));
            }
        }
        WebuiWorkspaceDiffMode::Branch => match (selected_base_branch, selected_compare_branch) {
            (Some(base_branch), Some(compare_branch)) => {
                let branch_diff =
                    load_webui_branch_git_diff(repository_root, compare_branch, base_branch)?;
                if !branch_diff.trim().is_empty() {
                    sections.push(branch_diff);
                }
            }
            _ => notices.push(String::from("当前仓库无法解析用于分支对比的分支。")),
        },
    }

    let unified_diff = sections.join("\n");
    let summaries = webui_unified_diff_file_summaries(&unified_diff);
    let indexed_files = webui_unified_diff_search_index(&unified_diff, &summaries);
    let raw_line_count = webui_unified_diff_line_count(&unified_diff);
    Ok(WebuiDiffSearchInput {
        unified_diff,
        indexed_files,
        raw_line_count,
        notices,
    })
}

/// Builds an empty WebUI diff search response with one user-facing notice.
/// 构建带有单条用户提示的空 WebUI diff 搜索响应。
fn empty_webui_diff_search_response(
    repository_root: String,
    query: String,
    notice: String,
) -> WebuiWorkspaceDiffSearchResponse {
    WebuiWorkspaceDiffSearchResponse {
        repository_root,
        query,
        matches: Vec::new(),
        truncated: false,
        scanned_files: 0,
        scanned_lines: 0,
        notice: Some(notice),
    }
}

/// Returns the process-local compact diff search cache.
/// 返回进程内紧凑 diff 搜索缓存。
fn webui_diff_search_cache() -> &'static Mutex<VecDeque<WebuiDiffSearchCacheEntry>> {
    WEBUI_DIFF_SEARCH_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Returns the process-local reusable diff search input cache.
/// 返回进程内可复用 diff 搜索输入缓存。
fn webui_diff_search_input_cache() -> &'static Mutex<VecDeque<WebuiDiffSearchInputCacheEntry>> {
    WEBUI_DIFF_SEARCH_INPUT_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Builds a stable compact diff search cache key.
/// 构建稳定的紧凑 diff 搜索缓存键。
fn webui_diff_search_cache_key(
    repository_root: &str,
    request: &WebuiWorkspaceDiffRequest,
    query: &str,
    limit: usize,
) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        repository_root,
        webui_diff_mode_cache_key(request.mode),
        request.base_branch.as_deref().unwrap_or_default(),
        request.compare_branch.as_deref().unwrap_or_default(),
        request.commit_hash.as_deref().unwrap_or_default(),
        query,
        limit,
    )
}

/// Builds a stable reusable diff search input cache key.
/// 构建稳定的可复用 diff 搜索输入缓存键。
fn webui_diff_search_input_cache_key(
    repository_root: &str,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        repository_root,
        webui_diff_mode_cache_key(mode),
        selected_base_branch.unwrap_or_default(),
        selected_compare_branch.unwrap_or_default(),
        selected_commit_hash.unwrap_or_default(),
    )
}

/// Returns a stable compact key for one WebUI diff mode.
/// 返回单个 WebUI diff 模式的稳定紧凑键。
fn webui_diff_mode_cache_key(mode: WebuiWorkspaceDiffMode) -> &'static str {
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => "unstaged",
        WebuiWorkspaceDiffMode::Staged => "staged",
        WebuiWorkspaceDiffMode::Commit => "commit",
        WebuiWorkspaceDiffMode::Branch => "branch",
    }
}

/// Reads one cached compact diff search response when it is still fresh.
/// 在仍然新鲜时读取一条已缓存的紧凑 diff 搜索响应。
fn cached_webui_diff_search_response(
    key: &str,
    now_ms: i128,
) -> Option<WebuiWorkspaceDiffSearchResponse> {
    let Ok(mut cache) = webui_diff_search_cache().lock() else {
        return None;
    };
    prune_webui_diff_search_cache(&mut cache, now_ms);
    cache
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.response.clone())
}

/// Reads one cached reusable diff search input when it is still fresh.
/// 在仍然新鲜时读取一条已缓存的可复用 diff 搜索输入。
fn cached_webui_diff_search_input(key: &str, now_ms: i128) -> Option<WebuiDiffSearchInput> {
    let Ok(mut cache) = webui_diff_search_input_cache().lock() else {
        return None;
    };
    prune_webui_diff_search_input_cache(&mut cache, now_ms);
    cache
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.input.clone())
}

/// Stores one compact diff search response in the short-lived cache.
/// 将单条紧凑 diff 搜索响应写入短生命周期缓存。
fn store_webui_diff_search_response(
    key: String,
    now_ms: i128,
    response: WebuiWorkspaceDiffSearchResponse,
) {
    let Ok(mut cache) = webui_diff_search_cache().lock() else {
        return;
    };
    prune_webui_diff_search_cache(&mut cache, now_ms);
    cache.retain(|entry| entry.key != key);
    cache.push_back(WebuiDiffSearchCacheEntry {
        key,
        created_at_ms: now_ms,
        response,
    });
    while cache.len() > WEBUI_DIFF_SEARCH_CACHE_LIMIT {
        cache.pop_front();
    }
}

/// Stores one reusable diff search input in the short-lived cache.
/// 将单条可复用 diff 搜索输入写入短生命周期缓存。
fn store_webui_diff_search_input(key: String, now_ms: i128, input: WebuiDiffSearchInput) {
    let Ok(mut cache) = webui_diff_search_input_cache().lock() else {
        return;
    };
    prune_webui_diff_search_input_cache(&mut cache, now_ms);
    cache.retain(|entry| entry.key != key);
    cache.push_back(WebuiDiffSearchInputCacheEntry {
        key,
        created_at_ms: now_ms,
        input,
    });
    while cache.len() > WEBUI_DIFF_SEARCH_INPUT_CACHE_LIMIT {
        cache.pop_front();
    }
}

/// Removes stale compact diff search cache entries.
/// 移除过期的紧凑 diff 搜索缓存项。
fn prune_webui_diff_search_cache(cache: &mut VecDeque<WebuiDiffSearchCacheEntry>, now_ms: i128) {
    cache.retain(|entry| {
        now_ms.saturating_sub(entry.created_at_ms) <= WEBUI_DIFF_SEARCH_CACHE_TTL_MS
    });
}

/// Removes stale reusable diff search input cache entries.
/// 移除过期的可复用 diff 搜索输入缓存项。
fn prune_webui_diff_search_input_cache(
    cache: &mut VecDeque<WebuiDiffSearchInputCacheEntry>,
    now_ms: i128,
) {
    cache.retain(|entry| {
        now_ms.saturating_sub(entry.created_at_ms) <= WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS
    });
}

/// Normalizes one WebUI diff search limit into the supported range.
/// 将单个 WebUI diff 搜索上限规范化到受支持范围内。
fn normalized_webui_diff_search_limit(limit: usize) -> usize {
    limit.clamp(1, WEBUI_DIFF_SEARCH_MAX_RESULTS)
}

/// Clears short-lived diff search caches after mutating Git state.
/// 在修改 Git 状态后清理短生命周期 diff 搜索缓存。
pub(crate) fn clear_webui_diff_search_caches() {
    if let Ok(mut cache) = webui_diff_search_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = webui_diff_search_input_cache().lock() {
        cache.clear();
    }
}

/// Compact result returned by the backend diff search scanner.
/// 后端 diff 搜索扫描器返回的紧凑结果。
struct WebuiWorkspaceDiffSearchScanResult {
    /// File-level matches retained for the WebUI.
    /// 保留给 WebUI 的文件级命中。
    matches: Vec<WebuiWorkspaceDiffSearchFileMatch>,
    /// Whether matching files exceeded the retained result limit.
    /// 命中文件是否超过保留结果上限。
    truncated: bool,
    /// Number of files scanned from the complete diff.
    /// 从完整 diff 扫描到的文件数量。
    scanned_files: usize,
    /// Number of diff lines scanned from the complete diff.
    /// 从完整 diff 扫描到的行数量。
    scanned_lines: usize,
}

/// Position metadata for one backend diff search candidate line.
/// 单条后端 diff 搜索候选行的位置元数据。
#[derive(Clone)]
struct WebuiDiffSearchLinePosition {
    /// Hunk index containing the candidate line.
    /// 包含候选行的 hunk 索引。
    hunk_index: usize,
    /// Old-file line number when the candidate maps to the old side.
    /// 候选行映射到旧侧时的旧文件行号。
    old_line: Option<usize>,
    /// New-file line number when the candidate maps to the new side.
    /// 候选行映射到新侧时的新文件行号。
    new_line: Option<usize>,
    /// Candidate diff line kind.
    /// 候选 diff 行类型。
    line_kind: String,
}

/// Builds a reusable file-level search candidate index from one complete unified diff.
/// 从完整 unified diff 构建可复用的文件级搜索候选索引。
fn webui_unified_diff_search_index(
    unified_diff: &str,
    summaries: &[WebuiWorkspaceDiffFileSummary],
) -> Vec<WebuiDiffSearchIndexedFile> {
    let mut indexed_files = Vec::new();
    let mut current: Option<WebuiDiffSearchIndexedFileBuilder> = None;
    let mut file_index = 0usize;
    for_each_webui_unified_diff_line(unified_diff, |line| {
        if line.starts_with("diff --git ") {
            flush_webui_diff_search_indexed_file(&mut indexed_files, current.take());
            current = summaries.get(file_index).cloned().map(|summary| {
                let path_preview =
                    webui_diff_search_path_preview(&summary.old_path, &summary.new_path);
                let mut builder = WebuiDiffSearchIndexedFileBuilder {
                    summary,
                    candidates: Vec::new(),
                    current_hunk_index: None,
                    current_old_line: 0,
                    current_new_line: 0,
                    seen_hunk_count: 0,
                };
                push_webui_diff_search_index_candidate(&mut builder, path_preview, None);
                builder
            });
            file_index += 1;
            return;
        }
        let Some(builder) = current.as_mut() else {
            return;
        };
        if line.starts_with("@@ ") {
            if let Some((old_start, new_start)) = parse_webui_diff_search_hunk_header(line) {
                builder.current_hunk_index = Some(builder.seen_hunk_count);
                builder.current_old_line = old_start;
                builder.current_new_line = new_start;
                builder.seen_hunk_count += 1;
            }
            let position =
                builder
                    .current_hunk_index
                    .map(|hunk_index| WebuiDiffSearchLinePosition {
                        hunk_index,
                        old_line: None,
                        new_line: None,
                        line_kind: String::from("hunk"),
                    });
            push_webui_diff_search_index_candidate(
                builder,
                webui_diff_search_line_preview(line),
                position,
            );
            return;
        }
        let position = webui_diff_search_index_line_position(builder, line);
        push_webui_diff_search_index_candidate(
            builder,
            webui_diff_search_line_preview(line),
            position,
        );
        advance_webui_diff_search_index_line_position(builder, line);
    });
    flush_webui_diff_search_indexed_file(&mut indexed_files, current);
    indexed_files
}

/// Appends one completed indexed file into the reusable search index.
/// 将一个已完成的索引文件追加到可复用搜索索引。
fn flush_webui_diff_search_indexed_file(
    indexed_files: &mut Vec<WebuiDiffSearchIndexedFile>,
    builder: Option<WebuiDiffSearchIndexedFileBuilder>,
) {
    let Some(builder) = builder else {
        return;
    };
    let ngram_index = webui_diff_search_candidate_ngram_index(&builder.candidates);
    indexed_files.push(WebuiDiffSearchIndexedFile {
        summary: builder.summary,
        candidates: builder.candidates,
        ngram_index,
    });
}

/// Appends one pre-normalized candidate to the current indexed file.
/// 将单条预归一化候选项追加到当前索引文件。
fn push_webui_diff_search_index_candidate(
    builder: &mut WebuiDiffSearchIndexedFileBuilder,
    preview: String,
    position: Option<WebuiDiffSearchLinePosition>,
) {
    builder.candidates.push(WebuiDiffSearchIndexedCandidate {
        normalized_preview: preview.to_lowercase(),
        preview,
        position,
    });
}

/// Builds a compact per-file n-gram inverted index for backend diff search candidates.
/// 为后端 diff 搜索候选项构建紧凑的文件级 n-gram 倒排索引。
fn webui_diff_search_candidate_ngram_index(
    candidates: &[WebuiDiffSearchIndexedCandidate],
) -> BTreeMap<String, Vec<usize>> {
    let mut index = BTreeMap::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        for ngram in webui_diff_search_unique_ngrams(&candidate.normalized_preview) {
            index
                .entry(ngram)
                .or_insert_with(Vec::new)
                .push(candidate_index);
        }
    }
    index
}

/// Returns unique n-grams for one normalized search string.
/// 返回单段归一化搜索文本中的唯一 n-gram。
fn webui_diff_search_unique_ngrams(value: &str) -> BTreeSet<String> {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() < WEBUI_DIFF_SEARCH_NGRAM_WIDTH {
        return BTreeSet::new();
    }
    let mut ngrams = BTreeSet::new();
    for window in chars.windows(WEBUI_DIFF_SEARCH_NGRAM_WIDTH) {
        ngrams.insert(window.iter().collect());
    }
    ngrams
}

/// Selects candidate indexes that may contain the normalized query.
/// 选择可能包含归一化 query 的候选下标。
fn webui_diff_search_candidate_indexes(
    indexed_file: &WebuiDiffSearchIndexedFile,
    normalized_query: &str,
) -> Vec<usize> {
    let query_ngrams = webui_diff_search_unique_ngrams(normalized_query);
    if query_ngrams.is_empty() {
        return (0..indexed_file.candidates.len()).collect();
    }
    let mut posting_lists = Vec::new();
    for ngram in query_ngrams {
        let Some(posting_list) = indexed_file.ngram_index.get(&ngram) else {
            return Vec::new();
        };
        posting_lists.push(posting_list);
    }
    posting_lists.sort_by_key(|posting_list| posting_list.len());
    let Some((first_posting_list, remaining_posting_lists)) = posting_lists.split_first() else {
        return Vec::new();
    };
    let mut candidate_indexes = (*first_posting_list).clone();
    for posting_list in remaining_posting_lists {
        candidate_indexes
            .retain(|candidate_index| posting_list.binary_search(candidate_index).is_ok());
        if candidate_indexes.is_empty() {
            break;
        }
    }
    candidate_indexes
}

/// Computes compact file-level search hits from a reusable candidate index.
/// 从可复用候选索引计算紧凑的文件级搜索命中。
fn webui_diff_search_index_file_matches(
    indexed_files: &[WebuiDiffSearchIndexedFile],
    query: &str,
    limit: usize,
    raw_line_count: usize,
) -> WebuiWorkspaceDiffSearchScanResult {
    let normalized_query = query.to_lowercase();
    let mut matches = Vec::new();
    let mut truncated = false;
    for indexed_file in indexed_files {
        let mut match_count = 0usize;
        let mut first_preview = None;
        let mut first_position = None;
        let mut previews = Vec::new();
        for candidate_index in webui_diff_search_candidate_indexes(indexed_file, &normalized_query)
        {
            let Some(candidate) = indexed_file.candidates.get(candidate_index) else {
                continue;
            };
            if candidate.normalized_preview.contains(&normalized_query) {
                match_count += 1;
                if first_preview.is_none() {
                    first_preview = Some(candidate.preview.clone());
                    first_position = candidate.position.clone();
                }
                push_webui_diff_search_match_preview(&mut previews, &candidate.preview);
            }
        }
        if match_count == 0 {
            continue;
        }
        if matches.len() >= limit {
            truncated = true;
            break;
        }
        let first_position = first_position;
        matches.push(WebuiWorkspaceDiffSearchFileMatch {
            key: indexed_file.summary.key.clone(),
            old_path: indexed_file.summary.old_path.clone(),
            new_path: indexed_file.summary.new_path.clone(),
            status: indexed_file.summary.status.clone(),
            match_count,
            first_preview: first_preview.unwrap_or_default(),
            previews,
            first_hunk_index: first_position.as_ref().map(|position| position.hunk_index),
            first_old_line: first_position
                .as_ref()
                .and_then(|position| position.old_line),
            first_new_line: first_position
                .as_ref()
                .and_then(|position| position.new_line),
            first_line_kind: first_position.map(|position| position.line_kind),
        });
    }
    WebuiWorkspaceDiffSearchScanResult {
        matches,
        truncated,
        scanned_files: indexed_files.len(),
        scanned_lines: raw_line_count,
    }
}

/// Appends one bounded unique preview for a file-level backend diff search hit.
/// 为单个后端 diff 文件级命中追加一条受限去重预览。
fn push_webui_diff_search_match_preview(previews: &mut Vec<String>, preview: &str) {
    if previews.len() >= WEBUI_DIFF_SEARCH_MATCH_PREVIEW_LIMIT || preview.trim().is_empty() {
        return;
    }
    if previews.iter().any(|item| item == preview) {
        return;
    }
    previews.push(preview.to_string());
}

/// Parses hunk start line numbers from one unified diff hunk header.
/// 从单个 unified diff hunk 头部解析起始行号。
fn parse_webui_diff_search_hunk_header(header: &str) -> Option<(usize, usize)> {
    let body = header.strip_prefix("@@ ")?;
    let mut parts = body.split_whitespace();
    let old_range = parts.next()?.strip_prefix('-')?;
    let new_range = parts.next()?.strip_prefix('+')?;
    let old_start = old_range.split(',').next()?.parse::<usize>().ok()?;
    let new_start = new_range.split(',').next()?.parse::<usize>().ok()?;
    Some((old_start, new_start))
}

/// Builds position metadata for one raw diff line using the current search cursor.
/// 使用当前搜索游标为单条原始 diff 行构建位置元数据。
fn webui_diff_search_index_line_position(
    builder: &WebuiDiffSearchIndexedFileBuilder,
    line: &str,
) -> Option<WebuiDiffSearchLinePosition> {
    let hunk_index = builder.current_hunk_index?;
    if line.starts_with('+') && !line.starts_with("+++") {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: None,
            new_line: Some(builder.current_new_line),
            line_kind: String::from("added"),
        });
    }
    if line.starts_with('-') && !line.starts_with("---") {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: Some(builder.current_old_line),
            new_line: None,
            line_kind: String::from("removed"),
        });
    }
    if line.starts_with(' ') {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: Some(builder.current_old_line),
            new_line: Some(builder.current_new_line),
            line_kind: String::from("context"),
        });
    }
    Some(WebuiDiffSearchLinePosition {
        hunk_index,
        old_line: None,
        new_line: None,
        line_kind: String::from("metadata"),
    })
}

/// Advances the backend diff search line cursor after scanning one raw diff line.
/// 扫描单条原始 diff 行后推进后端 diff 搜索行游标。
fn advance_webui_diff_search_index_line_position(
    builder: &mut WebuiDiffSearchIndexedFileBuilder,
    line: &str,
) {
    if builder.current_hunk_index.is_none() {
        return;
    }
    if line.starts_with('+') && !line.starts_with("+++") {
        builder.current_new_line += 1;
    } else if line.starts_with('-') && !line.starts_with("---") {
        builder.current_old_line += 1;
    } else if line.starts_with(' ') {
        builder.current_old_line += 1;
        builder.current_new_line += 1;
    }
}

/// Builds a compact preview for one file path search candidate.
/// 为单个文件路径搜索候选项构建紧凑预览。
fn webui_diff_search_path_preview(old_path: &str, new_path: &str) -> String {
    if old_path == new_path {
        format!("path: {new_path}")
    } else {
        format!("path: {old_path} -> {new_path}")
    }
}

/// Builds a compact preview for one raw diff line search candidate.
/// 为单条原始 diff 行搜索候选项构建紧凑预览。
fn webui_diff_search_line_preview(line: &str) -> String {
    let body = if line.starts_with('+') && !line.starts_with("+++") {
        &line[1..]
    } else if line.starts_with('-') && !line.starts_with("---") {
        &line[1..]
    } else if let Some(body) = line.strip_prefix(' ') {
        body
    } else {
        line
    };
    truncate_string_to_char_boundary(body.trim(), WEBUI_DIFF_SEARCH_PREVIEW_MAX_BYTES).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{webui_unified_diff_file_summaries, webui_unified_diff_line_count};

    /// Verifies reusable diff search input cache keys include the resolved source.
    /// 验证可复用 diff 搜索输入缓存键包含解析后的来源信息。
    #[test]
    fn webui_diff_search_input_cache_key_distinguishes_resolved_sources() {
        let branch_key = webui_diff_search_input_cache_key(
            "repo-a",
            WebuiWorkspaceDiffMode::Branch,
            Some("main"),
            Some("feature"),
            None,
        );
        let other_branch_key = webui_diff_search_input_cache_key(
            "repo-a",
            WebuiWorkspaceDiffMode::Branch,
            Some("release"),
            Some("feature"),
            None,
        );
        let commit_key = webui_diff_search_input_cache_key(
            "repo-a",
            WebuiWorkspaceDiffMode::Commit,
            None,
            None,
            Some("abc123"),
        );

        assert_ne!(branch_key, other_branch_key);
        assert_ne!(branch_key, commit_key);
    }

    /// Verifies reusable diff search input cache pruning removes stale entries.
    /// 验证可复用 diff 搜索输入缓存清理会移除过期条目。
    #[test]
    fn webui_diff_search_input_cache_prunes_stale_entries() {
        let mut cache = std::collections::VecDeque::new();
        cache.push_back(WebuiDiffSearchInputCacheEntry {
            key: String::from("old"),
            created_at_ms: 0,
            input: WebuiDiffSearchInput {
                unified_diff: String::from("old diff"),
                indexed_files: Vec::new(),
                raw_line_count: 1,
                notices: Vec::new(),
            },
        });
        cache.push_back(WebuiDiffSearchInputCacheEntry {
            key: String::from("fresh"),
            created_at_ms: WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS,
            input: WebuiDiffSearchInput {
                unified_diff: String::from("fresh diff"),
                indexed_files: Vec::new(),
                raw_line_count: 1,
                notices: Vec::new(),
            },
        });

        prune_webui_diff_search_input_cache(&mut cache, WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS + 1);

        assert_eq!(cache.len(), 1);
        assert_eq!(cache.front().map(|entry| entry.key.as_str()), Some("fresh"));
    }

    /// Verifies indexed diff search keeps first-match line metadata.
    /// 验证索引式 diff 搜索会保留首条命中的行元数据。
    #[test]
    fn webui_diff_search_index_keeps_first_match_position() {
        let unified_diff = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -1,2 +1,2 @@\n",
            "-old value\n",
            "+new value\n",
            " context value\n"
        );
        let summaries = webui_unified_diff_file_summaries(unified_diff);
        let index = webui_unified_diff_search_index(unified_diff, &summaries);
        let result = webui_diff_search_index_file_matches(
            &index,
            "new value",
            10,
            webui_unified_diff_line_count(unified_diff),
        );

        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.scanned_files, 1);
        assert_eq!(
            result.scanned_lines,
            webui_unified_diff_line_count(unified_diff)
        );
        assert_eq!(result.matches[0].first_new_line, Some(1));
        assert_eq!(result.matches[0].first_line_kind.as_deref(), Some("added"));
        assert_eq!(result.matches[0].previews.len(), 1);
        assert!(
            result.matches[0]
                .previews
                .contains(&String::from("new value"))
        );
    }
}

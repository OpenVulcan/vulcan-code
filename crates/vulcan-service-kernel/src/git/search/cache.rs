//! Git diff search cache helpers.
//! Git diff 搜索 cache 辅助逻辑。

use super::*;

static WEBUI_DIFF_SEARCH_CACHE: OnceLock<Mutex<VecDeque<WebuiDiffSearchCacheEntry>>> =
    OnceLock::new();

/// Process-local reusable diff search input cache.
/// 进程内可复用 diff 搜索输入缓存。
static WEBUI_DIFF_SEARCH_INPUT_CACHE: OnceLock<Mutex<VecDeque<WebuiDiffSearchInputCacheEntry>>> =
    OnceLock::new();

/// One compact diff search response cache entry.
/// 单条紧凑 diff 搜索响应缓存项。
pub(crate) struct WebuiDiffSearchCacheEntry {
    /// Stable search cache key.
    /// 稳定搜索缓存键。
    pub(crate) key: String,
    /// Cache creation timestamp in milliseconds.
    /// 缓存创建时间戳，单位毫秒。
    pub(crate) created_at_ms: i128,
    /// Cached compact search response.
    /// 已缓存的紧凑搜索响应。
    response: WebuiWorkspaceDiffSearchResponse,
}

pub(crate) struct WebuiDiffSearchInputCacheEntry {
    /// Stable input cache key.
    /// 稳定输入缓存键。
    pub(crate) key: String,
    /// Cache creation timestamp in milliseconds.
    /// 缓存创建时间戳，单位毫秒。
    pub(crate) created_at_ms: i128,
    /// Cached complete diff search input.
    /// 已缓存的完整 diff 搜索输入。
    pub(crate) input: WebuiDiffSearchInput,
}

pub(crate) fn empty_webui_diff_search_response(
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
pub(crate) fn webui_diff_search_cache() -> &'static Mutex<VecDeque<WebuiDiffSearchCacheEntry>> {
    WEBUI_DIFF_SEARCH_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Returns the process-local reusable diff search input cache.
/// 返回进程内可复用 diff 搜索输入缓存。
pub(crate) fn webui_diff_search_input_cache()
-> &'static Mutex<VecDeque<WebuiDiffSearchInputCacheEntry>> {
    WEBUI_DIFF_SEARCH_INPUT_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Builds a stable compact diff search cache key.
/// 构建稳定的紧凑 diff 搜索缓存键。
pub(crate) fn webui_diff_search_cache_key(
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
pub(crate) fn webui_diff_search_input_cache_key(
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
pub(crate) fn webui_diff_mode_cache_key(mode: WebuiWorkspaceDiffMode) -> &'static str {
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => "unstaged",
        WebuiWorkspaceDiffMode::Staged => "staged",
        WebuiWorkspaceDiffMode::Commit => "commit",
        WebuiWorkspaceDiffMode::Branch => "branch",
    }
}

/// Reads one cached compact diff search response when it is still fresh.
/// 在仍然新鲜时读取一条已缓存的紧凑 diff 搜索响应。
pub(crate) fn cached_webui_diff_search_response(
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
pub(crate) fn cached_webui_diff_search_input(
    key: &str,
    now_ms: i128,
) -> Option<WebuiDiffSearchInput> {
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
pub(crate) fn store_webui_diff_search_response(
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
pub(crate) fn store_webui_diff_search_input(
    key: String,
    now_ms: i128,
    input: WebuiDiffSearchInput,
) {
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
pub(crate) fn prune_webui_diff_search_cache(
    cache: &mut VecDeque<WebuiDiffSearchCacheEntry>,
    now_ms: i128,
) {
    cache.retain(|entry| {
        now_ms.saturating_sub(entry.created_at_ms) <= WEBUI_DIFF_SEARCH_CACHE_TTL_MS
    });
}

/// Removes stale reusable diff search input cache entries.
/// 移除过期的可复用 diff 搜索输入缓存项。
pub(crate) fn prune_webui_diff_search_input_cache(
    cache: &mut VecDeque<WebuiDiffSearchInputCacheEntry>,
    now_ms: i128,
) {
    cache.retain(|entry| {
        now_ms.saturating_sub(entry.created_at_ms) <= WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS
    });
}

/// Normalizes one WebUI diff search limit into the supported range.
/// 将单个 WebUI diff 搜索上限规范化到受支持范围内。
pub(crate) fn normalized_webui_diff_search_limit(limit: usize) -> usize {
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

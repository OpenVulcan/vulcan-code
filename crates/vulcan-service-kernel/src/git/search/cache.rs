//! Git diff search cache helpers.
//! Git diff 搜索 cache 辅助逻辑。

use super::*;
use crate::git::selection::WebuiWorkspaceDiffSelection;

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

/// Builds a stable reusable diff search input cache key from a resolved selection.
/// 基于已解析选择项构建稳定的可复用 diff 搜索输入缓存键。
pub(crate) fn webui_diff_search_input_cache_key_for_selection(
    repository_root: &str,
    selection: &WebuiWorkspaceDiffSelection,
) -> String {
    webui_diff_search_input_cache_key(
        repository_root,
        selection.request.mode,
        selection.selected_base_branch.as_deref(),
        selection.selected_compare_branch.as_deref(),
        selection.selected_commit_hash.as_deref(),
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
    cached_webui_diff_cache_value(
        &mut cache,
        key,
        now_ms,
        WEBUI_DIFF_SEARCH_CACHE_TTL_MS,
        |entry| entry.key.as_str(),
        |entry| entry.created_at_ms,
        |entry| &entry.response,
    )
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
    cached_webui_diff_cache_value(
        &mut cache,
        key,
        now_ms,
        WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS,
        |entry| entry.key.as_str(),
        |entry| entry.created_at_ms,
        |entry| &entry.input,
    )
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
    store_webui_diff_cache_entry(
        &mut cache,
        key,
        now_ms,
        WEBUI_DIFF_SEARCH_CACHE_TTL_MS,
        WEBUI_DIFF_SEARCH_CACHE_LIMIT,
        |entry| entry.key.as_str(),
        |entry| entry.created_at_ms,
        |key| WebuiDiffSearchCacheEntry {
            key,
            created_at_ms: now_ms,
            response,
        },
    );
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
    store_webui_diff_cache_entry(
        &mut cache,
        key,
        now_ms,
        WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS,
        WEBUI_DIFF_SEARCH_INPUT_CACHE_LIMIT,
        |entry| entry.key.as_str(),
        |entry| entry.created_at_ms,
        |key| WebuiDiffSearchInputCacheEntry {
            key,
            created_at_ms: now_ms,
            input,
        },
    );
}

/// Reads one fresh value from a short-lived WebUI diff cache.
/// 从短生命周期 WebUI diff 缓存中读取一条新鲜值。
///
/// # Parameters
/// 参数
/// `cache` stores entries ordered from oldest to newest.
/// `cache` 按从旧到新顺序保存缓存条目。
/// `key` selects the cached value.
/// `key` 选择缓存值。
/// `now_ms` is the current timestamp used for stale entry pruning.
/// `now_ms` 是用于清理过期条目的当前时间戳。
/// `ttl_ms` is the cache freshness window in milliseconds.
/// `ttl_ms` 是缓存新鲜窗口，单位毫秒。
/// `entry_key` reads an entry key.
/// `entry_key` 读取条目键。
/// `entry_created_at_ms` reads an entry creation timestamp.
/// `entry_created_at_ms` 读取条目创建时间戳。
/// `entry_value` reads the cached payload.
/// `entry_value` 读取缓存载荷。
///
/// # Returns
/// 返回值
/// The cloned cached value when the key is present after pruning.
/// 当清理后仍命中键时返回克隆出的缓存值。
fn cached_webui_diff_cache_value<Entry, Value: Clone>(
    cache: &mut VecDeque<Entry>,
    key: &str,
    now_ms: i128,
    ttl_ms: i128,
    entry_key: impl Fn(&Entry) -> &str,
    entry_created_at_ms: impl Fn(&Entry) -> i128,
    entry_value: impl Fn(&Entry) -> &Value,
) -> Option<Value> {
    prune_webui_diff_cache_entries(cache, now_ms, ttl_ms, entry_created_at_ms);
    cache
        .iter()
        .find(|entry| entry_key(entry) == key)
        .map(entry_value)
        .cloned()
}

/// Stores one value in a short-lived WebUI diff cache.
/// 向短生命周期 WebUI diff 缓存写入一条值。
///
/// # Parameters
/// 参数
/// `cache` stores entries ordered from oldest to newest.
/// `cache` 按从旧到新顺序保存缓存条目。
/// `key` is the stable cache key for the entry being inserted.
/// `key` 是待插入条目的稳定缓存键。
/// `now_ms` is the current timestamp used for stale entry pruning.
/// `now_ms` 是用于清理过期条目的当前时间戳。
/// `ttl_ms` is the cache freshness window in milliseconds.
/// `ttl_ms` 是缓存新鲜窗口，单位毫秒。
/// `limit` is the maximum number of entries retained after insertion.
/// `limit` 是插入后保留的最大条目数。
/// `entry_key` reads an entry key for replacement.
/// `entry_key` 读取条目键以执行替换。
/// `entry_created_at_ms` reads an entry creation timestamp.
/// `entry_created_at_ms` 读取条目创建时间戳。
/// `make_entry` builds the stored entry from the key.
/// `make_entry` 使用键构建要保存的条目。
// The generic cache borrows storage and independent key, age and construction callbacks.
// 通用缓存借用存储及独立的键、年龄与构造回调。
#[expect(
    clippy::too_many_arguments,
    reason = "The generic cache borrows storage and independent key, age and construction callbacks."
)]
fn store_webui_diff_cache_entry<Entry>(
    cache: &mut VecDeque<Entry>,
    key: String,
    now_ms: i128,
    ttl_ms: i128,
    limit: usize,
    entry_key: impl Fn(&Entry) -> &str,
    entry_created_at_ms: impl Fn(&Entry) -> i128,
    make_entry: impl FnOnce(String) -> Entry,
) {
    prune_webui_diff_cache_entries(cache, now_ms, ttl_ms, entry_created_at_ms);
    cache.retain(|entry| entry_key(entry) != key);
    cache.push_back(make_entry(key));
    while cache.len() > limit {
        cache.pop_front();
    }
}

/// Removes stale entries from one short-lived WebUI diff cache.
/// 从短生命周期 WebUI diff 缓存中移除过期条目。
///
/// # Parameters
/// 参数
/// `cache` stores entries ordered from oldest to newest.
/// `cache` 按从旧到新顺序保存缓存条目。
/// `now_ms` is the current timestamp used for stale entry pruning.
/// `now_ms` 是用于清理过期条目的当前时间戳。
/// `ttl_ms` is the cache freshness window in milliseconds.
/// `ttl_ms` 是缓存新鲜窗口，单位毫秒。
/// `entry_created_at_ms` reads an entry creation timestamp.
/// `entry_created_at_ms` 读取条目创建时间戳。
fn prune_webui_diff_cache_entries<Entry>(
    cache: &mut VecDeque<Entry>,
    now_ms: i128,
    ttl_ms: i128,
    entry_created_at_ms: impl Fn(&Entry) -> i128,
) {
    cache.retain(|entry| now_ms.saturating_sub(entry_created_at_ms(entry)) <= ttl_ms);
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

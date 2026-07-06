//! Git diff search tests.
//! Git diff 搜索测试。

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

/// Verifies input cache stores prune stale entries before inserting.
/// 验证输入缓存写入会在插入前清理过期条目。
#[test]
fn webui_diff_search_input_cache_store_prunes_stale_entries() {
    clear_webui_diff_search_caches();

    let old_key = String::from("input-cache-store-prune::old");
    let fresh_key = String::from("input-cache-store-prune::fresh");
    let new_key = String::from("input-cache-store-prune::new");

    store_webui_diff_search_input(old_key, 0, webui_diff_search_test_input("old diff"));
    store_webui_diff_search_input(
        fresh_key,
        WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS,
        webui_diff_search_test_input("fresh diff"),
    );
    store_webui_diff_search_input(
        new_key,
        WEBUI_DIFF_SEARCH_INPUT_CACHE_TTL_MS + 1,
        webui_diff_search_test_input("new diff"),
    );

    let cache = webui_diff_search_input_cache()
        .lock()
        .expect("diff search input cache lock should not be poisoned");
    let cached_keys: Vec<&str> = cache
        .iter()
        .filter_map(|entry| entry.key.strip_prefix("input-cache-store-prune::"))
        .collect();

    assert!(!cached_keys.contains(&"old"));
    assert!(cached_keys.contains(&"fresh"));
    assert!(cached_keys.contains(&"new"));
}

/// Builds compact reusable diff search input test data.
/// 构建紧凑的可复用 diff 搜索输入测试数据。
fn webui_diff_search_test_input(unified_diff: &str) -> WebuiDiffSearchInput {
    WebuiDiffSearchInput {
        unified_diff: String::from(unified_diff),
        indexed_files: Vec::new(),
        raw_line_count: 1,
        notices: Vec::new(),
    }
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

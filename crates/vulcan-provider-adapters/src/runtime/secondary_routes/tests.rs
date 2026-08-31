//! Tests for exact cache scope, freshness, forced refresh, and bounded eviction.
//! 精确缓存作用域、新鲜度、强制刷新及有界淘汰测试。

use super::super::test_support::two_account_runtime;
use super::*;

/// Verifies cached secondary routes are account-scoped, expire after six hours, and may be bypassed explicitly.
/// 验证二级路由缓存按账号隔离、六小时后过期，并可被显式绕过。
#[tokio::test]
async fn secondary_route_cache_honors_scope_ttl_and_forced_refresh() {
    let (runtime, configuration) = two_account_runtime();
    // cache_key targets only the second credential and its exact model.
    // cache_key 仅指向第二个凭据及其精确模型。
    let (cache_key, _, _, _, _) = runtime
        .secondary_route_cache_identity(&configuration, "two", "openai/gpt-5.6-sol")
        .expect("cache identity should resolve");
    // cached_route is deterministic and requires no network access.
    // cached_route 是确定性的，不需要网络访问。
    let cached_route = SecondaryRouteOption {
        route_id: String::from("cached"),
        provider_display_name: String::from("Cached Provider"),
        endpoint_display_name: String::from("Cached Endpoint"),
        prompt_price_per_token: None,
        completion_price_per_token: None,
        cache_read_price_per_token: None,
        latency_milliseconds: None,
        uptime_percent: None,
        throughput_tokens_per_second: None,
    };
    runtime
        .secondary_route_cache
        .write()
        .expect("cache should be writable")
        .insert(
            cache_key.clone(),
            SecondaryRouteCacheEntry {
                connection_id: String::from("two"),
                routes: vec![cached_route.clone()],
                fetched_at: Instant::now(),
            },
        );

    let cached = runtime
        .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", false)
        .await
        .expect("fresh cache should avoid credential access");
    assert_eq!(cached, vec![cached_route.clone()]);
    assert_eq!(
        runtime
            .cached_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol")
            .expect("selection validation should use the populated cache"),
        vec![cached_route.clone()]
    );
    assert!(
        runtime
            .cached_secondary_routes(&configuration, "one", "openai/gpt-5.6-sol")
            .is_err(),
        "selection validation must not reuse another credential's cache partition"
    );
    assert!(
        runtime
            .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", true)
            .await
            .is_err(),
        "forced refresh should bypass the cache and reach the unavailable secret boundary"
    );

    runtime
        .secondary_route_cache
        .write()
        .expect("cache should be writable")
        .get_mut(cache_key.as_str())
        .expect("cached route should remain present")
        .fetched_at = Instant::now()
        .checked_sub(SECONDARY_ROUTE_CACHE_TTL)
        .expect("six hours should fit in the monotonic clock");
    assert!(
        runtime
            .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", false)
            .await
            .is_err(),
        "expired cache should reach the unavailable secret boundary"
    );
    assert!(
        runtime
            .cached_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol")
            .is_err(),
        "selection validation must reject an expired route identity without issuing a request"
    );
}

/// Verifies secondary-route discovery cannot grow its process cache without bound.
/// 验证二级路由发现无法让进程缓存无界增长。
#[test]
fn secondary_route_cache_evicts_the_oldest_partition_at_capacity() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // baseInstant keeps every inserted entry fresh while giving the first key a stable oldest time.
    // baseInstant 让全部插入项保持新鲜，同时为首个键提供稳定的最旧时间。
    let base_instant = Instant::now();
    // index identifies one deterministic cache partition across the capacity boundary.
    // index 标识一个跨越容量边界的确定性缓存分区。
    for index in 0..=SECONDARY_ROUTE_CACHE_MAX_ENTRIES {
        // ageSeconds gives lower indexes an older but still-live observation time.
        // ageSeconds 为较小索引提供更旧但仍有效的观测时间。
        let age_seconds = (SECONDARY_ROUTE_CACHE_MAX_ENTRIES
            - index.min(SECONDARY_ROUTE_CACHE_MAX_ENTRIES)) as u64;
        // fetchedAt is the exact monotonic timestamp stored for deterministic eviction.
        // fetchedAt 是用于确定性淘汰的精确单调时钟时间戳。
        let fetched_at = base_instant
            .checked_sub(Duration::from_secs(age_seconds))
            .expect("bounded cache age should fit the monotonic clock");
        runtime
            .insert_secondary_route_cache_entry(
                format!("cache-key-{index:04}"),
                format!("connection-{index:04}").as_str(),
                Vec::new(),
                fetched_at,
            )
            .expect("bounded cache insertion should succeed");
    }
    let cache = runtime
        .secondary_route_cache
        .read()
        .expect("secondary-route cache should remain readable");
    assert_eq!(cache.len(), SECONDARY_ROUTE_CACHE_MAX_ENTRIES);
    assert!(
        !cache.contains_key("cache-key-0000"),
        "the deterministically oldest partition must be evicted"
    );
    assert!(cache.contains_key("cache-key-0512"));
}

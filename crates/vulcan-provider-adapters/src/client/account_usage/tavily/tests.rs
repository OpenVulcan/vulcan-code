//! Regression tests for account_usage/tavily.
//! account_usage/tavily 的回归测试。

use super::*;

/// Verifies a missing Tavily limit remains unknown rather than becoming zero.
/// 验证缺失的 Tavily 上限保持未知而不会被转换为零。
#[test]
fn tavily_missing_limit_is_not_zero() {
    let window = tavily_usage_window("search", "Search", 12, None, false)
        .expect("unbounded provider counter");
    assert_eq!(window.used.as_deref(), Some("12"));
    assert!(window.total.is_none());
    assert!(window.remaining.is_none());
    assert!(!window.unlimited);
}

/// Verifies only the documented Tavily PAYGO null limit is represented as unlimited.
/// 验证仅文档明确的 Tavily PAYGO 空上限会表示为无限。
#[test]
fn tavily_paygo_null_limit_is_unlimited() {
    let window =
        tavily_usage_window("paygo", "PAYGO", 3, None, true).expect("unlimited paygo counter");
    assert!(window.unlimited);
    assert!(window.total.is_none());
    assert!(window.remaining.is_none());
}

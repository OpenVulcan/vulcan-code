//! Regression tests for account_usage/minimax.
//! account_usage/minimax 的回归测试。

use super::*;

/// Verifies MiniMax's zero-allocation status-three bucket is unavailable, not unlimited.
/// 验证 MiniMax 零分配且状态为三的额度桶表示不可用而不是无限。
#[test]
fn minimax_zero_allocation_bucket_is_unavailable() {
    let windows = minimax_quota_windows(vec![MiniMaxQuotaModelRemain {
        model_name: String::from("video"),
        start_time: 1,
        end_time: 2,
        current_interval_total_count: 0,
        current_interval_usage_count: 0,
        current_interval_status: Some(3),
        current_interval_remaining_percent: Some(100),
        current_weekly_total_count: 0,
        current_weekly_usage_count: 0,
        current_weekly_status: Some(3),
        current_weekly_remaining_percent: Some(100),
        weekly_start_time: 1,
        weekly_end_time: 2,
    }])
    .expect("quota windows should normalize");
    assert_eq!(windows.len(), 2);
    assert!(windows.iter().all(|window| !window.available));
    assert!(windows.iter().all(|window| !window.unlimited));
}

/// Verifies a non-empty weekly status-three bucket remains explicitly unlimited.
/// 验证非空且周状态为三的额度桶仍明确表示无限。
#[test]
fn minimax_weekly_status_three_remains_unlimited() {
    let windows = minimax_quota_windows(vec![MiniMaxQuotaModelRemain {
        model_name: String::from("general"),
        start_time: 1,
        end_time: 2,
        current_interval_total_count: 100,
        current_interval_usage_count: 90,
        current_interval_status: Some(1),
        current_interval_remaining_percent: Some(90),
        current_weekly_total_count: 100,
        current_weekly_usage_count: 90,
        current_weekly_status: Some(3),
        current_weekly_remaining_percent: Some(90),
        weekly_start_time: 1,
        weekly_end_time: 2,
    }])
    .expect("quota windows should normalize");
    assert!(windows.iter().all(|window| window.available));
    assert!(!windows[0].unlimited);
    assert!(windows[1].unlimited);
}

//! Tests for retaining confirmed access evidence after transient usage-query failures.
//! 暂时用量查询失败后保留已确认访问证据的测试。

use super::super::test_support::account_usage;
use super::*;

/// Verifies transient usage failures retain the last confirmed plan and quota access evidence.
/// 验证暂时用量失败会保留最后一次确认的套餐与额度访问证据。
#[test]
fn transient_usage_failure_preserves_account_access_evidence() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let quota = vulcan_ai_contract::AccountUsageWindow {
        id: String::from("purchased-credits"),
        display_name: String::from("Purchased credits"),
        bucket_name: None,
        used: Some(String::from("0")),
        remaining: Some(String::from("1.5")),
        total: Some(String::from("1.5")),
        remaining_percent: None,
        unit: String::from("USD"),
        starts_at: None,
        resets_at: None,
        unlimited: false,
        available: true,
    };
    let confirmed = account_usage(
        "credential-access-account",
        Some("Go"),
        Some("individual-go"),
        vec![quota.clone()],
    );
    runtime
        .cache_usage_snapshot("access-account", confirmed, true)
        .expect("confirmed usage should cache");

    let mut transient = account_usage("credential-access-account", None, None, Vec::new());
    transient.availability = vulcan_ai_contract::AccountUsageAvailability::TemporarilyUnavailable;
    let retained = runtime
        .cache_usage_snapshot("access-account", transient, true)
        .expect("transient usage should preserve access evidence");

    assert_eq!(retained.plan_name.as_deref(), Some("Go"));
    assert_eq!(retained.plan_id.as_deref(), Some("individual-go"));
    assert_eq!(retained.quota_windows, vec![quota]);
    assert!(matches!(
        retained.availability,
        vulcan_ai_contract::AccountUsageAvailability::TemporarilyUnavailable
    ));
}

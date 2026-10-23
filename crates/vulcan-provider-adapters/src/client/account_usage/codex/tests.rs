//! Regression tests for account_usage/codex.
//! account_usage/codex 的回归测试。

use super::*;

/// Finds one normalized window by its stable identity.
/// 按稳定身份查找一个归一化窗口。
fn window<'a>(snapshot: &'a AccountUsageSnapshot, id: &str) -> &'a AccountUsageWindow {
    snapshot
        .quota_windows
        .iter()
        .find(|window| window.id == id)
        .unwrap_or_else(|| panic!("window `{id}` missing; the fixture may be out of date"))
}

/// Verifies the main and additional buckets become remaining-percentage windows with reset times.
/// 验证主限额桶与附加限额桶转换为带重置时间的剩余百分比窗口。
#[test]
fn codex_usage_maps_main_and_additional_buckets() {
    let snapshot = decode_openai_codex_usage(
        r#"{"plan_type":"plus","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":42,"limit_window_seconds":18000,"reset_after_seconds":600,"reset_at":1790000000},"secondary_window":{"used_percent":7,"limit_window_seconds":604800,"reset_after_seconds":86400,"reset_at":1790500000}},"additional_rate_limits":[{"limit_name":"GPT-6-Astra","metered_feature":"codex_astra","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":100,"limit_window_seconds":604800,"reset_after_seconds":3600,"reset_at":1790100000},"secondary_window":null}}],"credits":{"has_credits":true,"unlimited":false,"balance":"12.5"}}"#,
    )
    .expect("valid Codex usage");
    assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
    assert_eq!(snapshot.plan_id.as_deref(), Some("plus"));
    assert_eq!(snapshot.plan_name.as_deref(), Some("Plus"));
    assert_eq!(snapshot.balance_amount.as_deref(), Some("12.5"));
    assert_eq!(snapshot.currency.as_deref(), Some("credits"));
    assert_eq!(snapshot.quota_windows.len(), 3);

    let primary = window(&snapshot, "codex:primary");
    assert_eq!(primary.remaining_percent.as_deref(), Some("58"));
    assert_eq!(primary.duration_seconds, Some(18_000));
    assert_eq!(primary.resets_at.as_deref(), Some("1790000000"));
    assert_eq!(primary.bucket_name, None);

    let secondary = window(&snapshot, "codex:secondary");
    assert_eq!(secondary.remaining_percent.as_deref(), Some("93"));
    assert_eq!(secondary.duration_seconds, Some(604_800));

    let additional = window(&snapshot, "codex_astra:primary");
    assert_eq!(additional.remaining_percent.as_deref(), Some("0"));
    assert_eq!(additional.bucket_name.as_deref(), Some("GPT-6-Astra"));
}

/// Verifies absent limits and unlimited credits produce no invented windows or balances.
/// 验证缺失的限额与不限量 Credits 不会臆造窗口或余额。
#[test]
fn codex_usage_without_limits_or_finite_credits_stays_empty() {
    let snapshot = decode_openai_codex_usage(
        r#"{"plan_type":"pro","rate_limit":null,"credits":{"has_credits":true,"unlimited":true,"balance":"0"}}"#,
    )
    .expect("valid Codex usage");
    assert!(snapshot.quota_windows.is_empty());
    assert_eq!(snapshot.balance_amount, None);
    assert_eq!(snapshot.currency, None);
}

/// Verifies malformed payloads fail instead of producing a partial snapshot.
/// 验证格式错误的响应直接失败，而不是生成部分快照。
#[test]
fn codex_usage_rejects_malformed_window() {
    assert!(
        decode_openai_codex_usage(
            r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":"high"}}}"#
        )
        .is_err()
    );
}

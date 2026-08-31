//! Regression tests for account_usage/kimi.
//! account_usage/kimi 的回归测试。

use super::*;

/// Verifies Kimi preserves membership, nested counters, exact decimals, and reset evidence.
/// 验证 Kimi 保留会员、嵌套计数、精确十进制及重置证据。
#[test]
fn kimi_usage_preserves_membership_and_nested_windows() {
    let snapshot = decode_kimi_coding_plan_usage(
        r#"{"usage":{"used":"1.25","limit":"10"},"limits":[{"name":"5 hour usage","detail":{"used":2,"limit":8,"remaining":6,"resetAt":"2026-07-29T10:00:00Z"},"window":{"duration":5,"timeUnit":"HOURS"}}],"user":{"membership":{"level":"LEVEL_BASIC"}}}"#,
    )
    .expect("valid Kimi usage");
    assert_eq!(snapshot.plan_name.as_deref(), Some("Moderato"));
    assert_eq!(snapshot.quota_windows.len(), 2);
    assert_eq!(snapshot.quota_windows[0].id, "weekly_usage");
    assert_eq!(snapshot.quota_windows[0].display_name, "weekly_usage");
    assert_eq!(snapshot.quota_windows[0].remaining.as_deref(), Some("8.75"));
    assert_eq!(snapshot.quota_windows[1].id, "5_hour_usage");
    assert_eq!(snapshot.quota_windows[1].used.as_deref(), Some("2"));
    assert_eq!(snapshot.quota_windows[1].remaining.as_deref(), Some("6"));
    assert_eq!(
        snapshot.quota_windows[1].resets_at.as_deref(),
        Some("2026-07-29T10:00:00Z")
    );
    assert_eq!(
        snapshot.quota_windows[1].bucket_name.as_deref(),
        Some("5 HOURS")
    );
}

/// Verifies Kimi membership names follow ModelCore's confirmed provider-level matrix.
/// 验证 Kimi 套餐名称遵循 ModelCore 已确认的供应商等级矩阵。
#[test]
fn kimi_usage_maps_confirmed_membership_names() {
    for (provider_level, plan_name) in [
        ("LEVEL_TRIAL", "Andante"),
        ("LEVEL_BASIC", "Moderato"),
        ("LEVEL_INTERMEDIATE", "Allegretto"),
        ("LEVEL_ADVANCED", "Allegro"),
    ] {
        let body = format!(r#"{{"user":{{"membership":{{"level":"{provider_level}"}}}}}}"#);
        let snapshot = decode_kimi_coding_plan_usage(body.as_str()).expect("known Kimi membership");
        assert_eq!(snapshot.plan_name.as_deref(), Some(plan_name));
    }

    let error =
        decode_kimi_coding_plan_usage(r#"{"user":{"membership":{"level":"LEVEL_FUTURE"}}}"#)
            .expect_err("unknown Kimi membership must be rejected");
    assert!(error.contains("unknown membership level `LEVEL_FUTURE`"));
}

/// Verifies an unnamed five-hour Kimi window receives ModelCore's stable metric identifier.
/// 验证未命名的五小时 Kimi 窗口获得 ModelCore 的稳定指标标识。
#[test]
fn kimi_usage_identifies_unnamed_five_hour_window() {
    let snapshot = decode_kimi_coding_plan_usage(
        r#"{"limits":[{"duration":300,"timeUnit":"TIME_UNIT_MINUTE","detail":{"used":2,"limit":8,"remaining":6}}],"user":{"membership":{"level":"LEVEL_INTERMEDIATE"}}}"#,
    )
    .expect("valid unnamed Kimi window");

    assert_eq!(snapshot.plan_name.as_deref(), Some("Allegretto"));
    assert_eq!(snapshot.quota_windows.len(), 1);
    assert_eq!(snapshot.quota_windows[0].id, "five_hour_usage");
    assert_eq!(snapshot.quota_windows[0].display_name, "five_hour_usage");
    assert_eq!(
        snapshot.quota_windows[0].bucket_name.as_deref(),
        Some("300 TIME_UNIT_MINUTE")
    );
}

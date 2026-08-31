//! Regression tests for account_usage/command_code.
//! account_usage/command_code 的回归测试。

use super::*;

/// Verifies CommandCode credit pools and rolling limits retain exact provider semantics.
/// 验证 CommandCode 额度池与滚动限制保留精确供应商语义。
#[test]
fn command_code_usage_normalization_is_exact() {
    let subscription: CommandCodeSubscriptionResponse = serde_json::from_str(
        r#"{
                "success": true,
                "data": {
                    "status": "active",
                    "planId": "individual-goat",
                    "currentPeriodStart": "2026-08-17T08:28:50.000Z",
                    "currentPeriodEnd": "2026-09-17T08:28:50.000Z"
                }
            }"#,
    )
    .expect("CommandCode subscription fixture");
    let snapshot = decode_command_code_usage(
        r#"{
                "credits": {
                    "belowThreshold": false,
                    "creditThreshold": 0,
                    "monthlyCredits": 70,
                    "purchasedCredits": 2.5,
                    "freeCredits": 0.25
                },
                "windowLimits": {
                    "limited": true,
                    "exceeded": null,
                    "fiveHour": {"used": 0.0127491583, "cap": 14, "exceeded": false, "resetAt": 123},
                    "weekly": {"used": 4, "cap": 35, "exceeded": false, "resetAt": 456}
                }
            }"#,
        subscription,
        r#"{
                "totalCost": 7,
                "totalMonthlyCredits": 5,
                "totalPurchasedCredits": 1.25,
                "totalFreeCredits": 0.75
            }"#,
    )
    .expect("CommandCode usage fixture must normalize");
    assert_eq!(snapshot.plan_name.as_deref(), Some("GOAT"));
    assert_eq!(snapshot.plan_id.as_deref(), Some("individual-goat"));
    assert_eq!(snapshot.used_amount.as_deref(), Some("7"));
    assert_eq!(snapshot.remaining_amount.as_deref(), Some("72.75"));
    assert_eq!(snapshot.balance_amount.as_deref(), Some("72.75"));
    assert_eq!(snapshot.quota_windows.len(), 5);
    assert_eq!(snapshot.quota_windows[0].total.as_deref(), Some("75"));
    assert_eq!(snapshot.quota_windows[1].total.as_deref(), Some("3.75"));
    assert_eq!(snapshot.quota_windows[2].total.as_deref(), Some("1"));
    assert_eq!(
        snapshot.quota_windows[3].used.as_deref(),
        Some("0.0127491583")
    );
    assert_eq!(
        snapshot.quota_windows[3].remaining.as_deref(),
        Some("13.9872508417")
    );
    assert_eq!(snapshot.quota_windows[3].resets_at.as_deref(), Some("123"));
    assert!(snapshot.refresh_message.is_none());
}

/// Verifies unrestricted CommandCode plans may omit both rolling-window objects.
/// 验证不受滚动限制的 CommandCode 套餐可以省略两个窗口对象。
#[test]
fn command_code_unlimited_plan_accepts_absent_request_windows() {
    let subscription: CommandCodeSubscriptionResponse =
        serde_json::from_str(r#"{"success":true,"data":null}"#)
            .expect("CommandCode subscription fixture");
    let snapshot = decode_command_code_usage(
        r#"{
                "credits": {
                    "belowThreshold": false,
                    "monthlyCredits": 0,
                    "purchasedCredits": 10,
                    "freeCredits": 0
                },
                "windowLimits": {"limited": false}
            }"#,
        subscription,
        r#"{
                "totalCost": 1,
                "totalMonthlyCredits": 0,
                "totalPurchasedCredits": 1,
                "totalFreeCredits": 0
            }"#,
    )
    .expect("unrestricted CommandCode usage fixture must normalize");
    assert_eq!(snapshot.plan_name.as_deref(), Some("CommandCode"));
    assert!(snapshot.plan_id.is_none());
    assert_eq!(snapshot.quota_windows.len(), 3);
    assert!(
        snapshot
            .quota_windows
            .iter()
            .all(|window| window.unit == "USD")
    );
}

/// Verifies inactive subscriptions do not project a stale CommandCode plan identity.
/// 验证非活动订阅不会投影过期的 CommandCode 套餐身份。
#[test]
fn command_code_inactive_subscription_omits_plan_identity() {
    let subscription: CommandCodeSubscriptionResponse = serde_json::from_str(
        r#"{
                "success": true,
                "data": {
                    "status": "canceled",
                    "planId": "individual-goat",
                    "currentPeriodStart": "2026-08-17T08:28:50.000Z",
                    "currentPeriodEnd": "2026-09-17T08:28:50.000Z"
                }
            }"#,
    )
    .expect("CommandCode subscription fixture");
    let snapshot = decode_command_code_usage(
        r#"{
                "credits": {
                    "belowThreshold": false,
                    "monthlyCredits": 0,
                    "purchasedCredits": 0,
                    "freeCredits": 0
                },
                "windowLimits": {"limited": false}
            }"#,
        subscription,
        r#"{
                "totalCost": 0,
                "totalMonthlyCredits": 0,
                "totalPurchasedCredits": 0,
                "totalFreeCredits": 0
            }"#,
    )
    .expect("inactive CommandCode usage fixture must normalize");
    assert_eq!(snapshot.plan_name.as_deref(), Some("CommandCode"));
    assert!(snapshot.plan_id.is_none());
    assert!(snapshot.refresh_message.is_some());
}

/// Verifies CommandCode metadata queries remain on the registered origin and encode values.
/// 验证 CommandCode 元数据查询保持在已注册 Origin，并正确编码查询值。
#[test]
fn command_code_usage_url_is_fixed_and_encoded() {
    let url = command_code_usage_url(
        "/alpha/usage/summary",
        Some("org/a b"),
        Some("2026-08-17T08:28:50.000Z"),
    )
    .expect("registered CommandCode URL");
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("api.commandcode.ai"));
    assert_eq!(url.path(), "/alpha/usage/summary");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        vec![
            ("orgId".into(), "org/a b".into()),
            ("since".into(), "2026-08-17T08:28:50.000Z".into()),
        ]
    );
}

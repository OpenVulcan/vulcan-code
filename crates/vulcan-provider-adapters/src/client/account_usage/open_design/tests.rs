//! Plan-window precision, scope binding, credential roles, and complete-observation failures.
//! 套餐窗口精度、作用域绑定、凭据角色及完整观测失败测试。

use super::*;
use crate::{CredentialSecretEnvelope, CredentialSecretMaterial, open_design::test_server};
use serde_json::{Value, json};
use std::time::Duration;

/// Builds a synthetic production-shaped preflight; no parameters, returns JSON without real account data.
/// 构建符合生产结构的合成预检；无参数，返回不含真实账户数据的 JSON。
fn preflight() -> Value {
    json!({
        "schemaVersion": 1, "workspaceId": "synthetic-workspace",
        "workspaceMemberId": "synthetic-member", "modelId": null, "balanceUsd": "1.5000",
        "codingPlan": {"workspaceId": "synthetic-workspace", "eligible": true, "tier": "go",
            "windows": [
                {"policyId": "weekly-policy", "durationSeconds": 604800, "resetMode": "anchored_recurring",
                    "usedCredits": "20", "limitCredits": "100", "remainingCredits": "80",
                    "windowStart": "2026-10-15T00:00:00Z", "resetsAt": "2026-10-22T00:00:00Z"},
                {"policyId": "five-hour-policy", "durationSeconds": 18000, "resetMode": "anchored_recurring",
                    "usedCredits": "2500000000000000000000000000000000000000",
                    "limitCredits": "10000000000000000000000000000000000000000",
                    "remainingCredits": "7500000000000000000000000000000000000000",
                    "windowStart": "2026-10-15T00:00:00Z", "resetsAt": "2026-10-15T05:00:00Z"}
            ]}
    })
}

/// Decodes one synthetic preflight against its exact wallet identity; returns normalization or a closed error.
/// 根据精确钱包身份解码一份合成预检；返回规范化结果或封闭错误。
fn decode(response: Value) -> Result<AccountUsageSnapshot, String> {
    // Scope is independently established by the preceding authenticated wallet request.
    // 作用域由前置已鉴权钱包请求独立建立。
    let scope = WalletScope {
        workspace_id: String::from("synthetic-workspace"),
        workspace_member_id: String::from("synthetic-member"),
    };
    normalize_usage(
        &scope,
        serde_json::from_value(response).map_err(|_| String::from("Invalid test wire shape."))?,
    )
}

/// Selects the synthetic short window by stable policy identity; returns its mutable JSON object.
/// 按稳定策略身份选择合成短周期窗口；返回其可变 JSON 对象。
fn short_window(response: &mut Value) -> &mut Value {
    response["codingPlan"]["windows"].as_array_mut().unwrap().iter_mut()
        .find(|window| window["policyId"] == "five-hour-policy")
        .expect("The synthetic short-window policy must exist; update its identity when the fixture changes.")
}

/// Verifies exact quotas, independent periods and reset timestamps; no parameters or return value.
/// 验证精确额度、独立周期及重置时间戳；无参数或返回值。
#[test]
fn open_design_plan_windows_remain_exact_and_separate() {
    // Find by identity so fixture ordering does not become an accidental content constraint.
    // 按身份查找，避免将夹具顺序变成意外的内容约束。
    let snapshot = decode(preflight()).unwrap();
    let short = snapshot
        .quota_windows
        .iter()
        .find(|window| window.id == "five-hour-policy")
        .expect("Expected the synthetic short-window identity; the fixture may have changed.");
    assert_eq!(snapshot.plan_id.as_deref(), Some("go"));
    assert_eq!(short.duration_seconds, Some(18000));
    assert_eq!(
        short.remaining.as_deref(),
        Some("7500000000000000000000000000000000000000")
    );
    assert_eq!(short.resets_at.as_deref(), Some("2026-10-15T05:00:00Z"));
    assert!(
        snapshot
            .quota_windows
            .iter()
            .all(|window| window.available && !window.unlimited)
    );
    assert!(
        snapshot
            .quota_windows
            .windows(2)
            .all(|pair| pair[0].duration_seconds <= pair[1].duration_seconds)
    );
}

/// Rejects foreign identities, unknown contracts and malformed credit windows; no parameters or return value.
/// 拒绝外部身份、未知契约及非法积分窗口；无参数或返回值。
#[test]
fn open_design_plan_rejects_invalid_scope_and_windows() {
    for (field, value) in [
        ("schemaVersion", json!(2)),
        ("workspaceId", json!("foreign")),
        ("workspaceMemberId", json!("foreign")),
        ("modelId", json!("unexpected-model")),
    ] {
        // Each mutation isolates one required account-binding failure.
        // 每次变更隔离一个必需账户绑定错误。
        let mut response = preflight();
        response[field] = value;
        assert!(decode(response).is_err());
    }
    for (field, value) in [
        ("workspaceId", json!("foreign")),
        ("tier", json!("unknown-tier")),
    ] {
        let mut response = preflight();
        response["codingPlan"][field] = value;
        assert!(decode(response).is_err());
    }
    for (field, value) in [
        ("policyId", json!("")),
        ("policyId", json!("weekly-policy")),
        ("durationSeconds", json!(0)),
        ("durationSeconds", json!(9007199254740992_u64)),
        ("limitCredits", json!("0")),
        ("limitCredits", json!("1")),
        ("usedCredits", json!("1.5")),
        ("remainingCredits", json!("-1")),
        ("remainingCredits", json!(12)),
        ("resetMode", json!("unknown")),
        ("resetsAt", json!("invalid-date")),
    ] {
        // Malformed quantities and timestamps fail instead of fabricating zero or unlimited quota.
        // 非法数量及时间戳必须失败，而不是伪造零额度或无限额度。
        let mut response = preflight();
        short_window(&mut response)[field] = value;
        assert!(decode(response).is_err());
    }
}

/// Preserves explicitly nullable reset times and unsupported membership; no parameters or return value.
/// 保留明确可空的重置时间及不适用会员状态；无参数或返回值。
#[test]
fn open_design_plan_does_not_invent_resets_or_entitlements() {
    // Activity-triggered windows may have no start until the first eligible request.
    // 活动触发窗口在首次符合条件的请求前可以没有开始时间。
    let mut response = preflight();
    let window = short_window(&mut response);
    window["resetMode"] = json!("activity_triggered");
    window["windowStart"] = Value::Null;
    window["resetsAt"] = Value::Null;
    let snapshot = decode(response).unwrap();
    let short = snapshot
        .quota_windows
        .iter()
        .find(|window| window.id == "five-hour-policy")
        .unwrap();
    assert!(short.starts_at.is_none() && short.resets_at.is_none() && !short.unlimited);
    // A wallet can exist without a qualifying plan, including a zero wallet balance.
    // 钱包可以在没有适用套餐时存在，包括钱包余额为零的情况。
    let mut response = preflight();
    response["balanceUsd"] = json!("0");
    response["codingPlan"]["eligible"] = json!(false);
    response["codingPlan"]["tier"] = Value::Null;
    response["codingPlan"]["windows"] = json!([]);
    let snapshot = decode(response).unwrap();
    assert!(snapshot.plan_id.is_none() && snapshot.quota_windows.is_empty());
    assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
}

/// Verifies exact signed preflight amounts without conflating wallet money and plan credit.
/// 验证精确的带符号预检金额，不混淆钱包金额与套餐积分。
#[test]
fn open_design_wallet_preserves_exact_decimal() {
    // The wallet retains fractional precision and never becomes plan remaining credit.
    // 钱包保留小数精度，绝不变成套餐剩余积分。
    let mut response = preflight();
    response["balanceUsd"] = json!("12345678901234567890.12345678901234567890");
    let snapshot = decode(response).unwrap();
    assert_eq!(
        snapshot.balance_amount.as_deref(),
        Some("12345678901234567890.12345678901234567890")
    );
    assert_eq!(snapshot.currency.as_deref(), Some("USD"));
    assert!(snapshot.remaining_amount.is_none() && snapshot.used_amount.is_none());
    for amount in ["0.0000", "-0.0001"] {
        let mut response = preflight();
        response["balanceUsd"] = json!(amount);
        assert_eq!(
            decode(response).unwrap().balance_amount.as_deref(),
            Some(amount)
        );
    }
    for amount in [
        json!(1),
        json!("1e3"),
        json!(""),
        json!("1.2.3"),
        json!(".5"),
        json!("1."),
        json!("--1"),
    ] {
        let mut response = preflight();
        response["balanceUsd"] = amount;
        assert!(decode(response).is_err());
    }
}

/// Verifies the wire uses only the account key and rejected HTTP bodies never leak credentials.
/// 验证线协议仅使用账户密钥，被拒绝的 HTTP 正文绝不泄露凭据。
#[tokio::test]
async fn open_design_wallet_uses_control_key_and_reports_auth_expiry() {
    // The synthetic pair makes credential-role mistakes observable on both requests.
    // 合成凭据对使两次请求中的角色错误均可被观测。
    let envelope = CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenDesignAuth {
        runtime_key: String::from("runtime-only"),
        control_key: String::from("control-only"),
        user_id: String::from("synthetic-user"),
        email: String::from("test@example.invalid"),
    })
    .unwrap();
    let secret = envelope.execution_secret().unwrap();
    for (wallet_status, plan_status) in [
        (200, 200),
        (401, 200),
        (403, 200),
        (500, 200),
        (302, 200),
        (200, 401),
        (200, 403),
        (200, 500),
        (200, 302),
    ] {
        // Failure bodies contain secrets to verify that controlled errors never echo them.
        // 失败正文包含秘密，以验证受控错误绝不回显这些值。
        let failure = String::from(r#"{"error":"control-only runtime-only"}"#);
        let mut responses = vec![(
            wallet_status,
            if wallet_status == 200 {
                json!({"workspaceId":"synthetic-workspace","workspaceMemberId":"synthetic-member"})
                    .to_string()
            } else {
                failure.clone()
            },
            Duration::ZERO,
        )];
        if wallet_status == 200 {
            responses.push((
                plan_status,
                if plan_status == 200 {
                    preflight().to_string()
                } else {
                    failure
                },
                Duration::ZERO,
            ));
        }
        let (origin, server) = test_server(responses).await;
        let result = query_at(
            &crate::open_design::client().unwrap(),
            &origin,
            secret.open_design_control_key().unwrap(),
        )
        .await;
        if wallet_status == 200 && plan_status == 200 {
            assert_eq!(result.unwrap().plan_id.as_deref(), Some("go"));
        } else {
            let error = result.unwrap_err();
            assert!(!error.contains("control-only") && !error.contains("runtime-only"));
        }
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), if wallet_status == 200 { 2 } else { 1 });
        assert!(
            requests
                .iter()
                .any(|request| request.starts_with("GET /api/v1/wallet/balance HTTP/1.1"))
        );
        if wallet_status == 200 {
            assert!(requests.iter().any(|request| request.starts_with(
                "GET /api/v1/workspaces/synthetic-workspace/billing/preflight HTTP/1.1"
            )));
        }
        for request in requests {
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer control-only\r\n")
            );
            assert!(!request.contains("runtime-only"));
        }
    }
    assert!(
        CredentialSecretEnvelope::new_api_key(String::from("api-key"))
            .unwrap()
            .execution_secret()
            .unwrap()
            .open_design_control_key()
            .is_err()
    );
}

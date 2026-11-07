//! Behavior of the TokenRhythm wallet query against synthetic local servers only.
//! 仅针对本地合成服务验证 TokenRhythm 钱包查询行为。

use super::*;
use crate::open_design::test_server;
use std::time::Duration;

/// Builds one wallet envelope for `account` with the given debt and frozen amounts.
/// 为 `account` 构建一个带指定欠款与冻结金额的钱包封装。
fn wallet_body(account: &str, debt: &str, frozen: &str) -> String {
    serde_json::json!({
        "code": 0,
        "data": {
            "account_id": account,
            "currency": "CNY",
            "balances": {"available": "12.50", "recharge": "10.00", "bonus": "2.50",
                         "debt": debt, "frozen": frozen},
            "as_of": "2026-11-07T00:00:00Z"
        }
    })
    .to_string()
}

#[tokio::test]
async fn wallet_reports_available_balance_and_its_parts() {
    let (origin, server) = test_server(vec![(
        200,
        wallet_body("acct_demo", "0", "0.00"),
        Duration::ZERO,
    )])
    .await;
    let http = crate::tokenrhythm::client().unwrap();
    let snapshot = query_at(&http, &origin, "synthetic-access", "acct_demo")
        .await
        .unwrap();
    assert_eq!(snapshot.balance_amount.as_deref(), Some("12.50"));
    assert_eq!(snapshot.currency.as_deref(), Some("CNY"));
    assert!(snapshot.refresh_message.is_none());
    let requests = server.await.unwrap();
    assert!(requests[0].starts_with(&format!(
        "GET /api/v1/integrations/{}/wallet ",
        crate::tokenrhythm::INTEGRATION_APP
    )));
}

#[tokio::test]
async fn wallet_reports_outstanding_debt() {
    let (origin, _server) = test_server(vec![(
        200,
        wallet_body("acct_demo", "1.20", "0.30"),
        Duration::ZERO,
    )])
    .await;
    let http = crate::tokenrhythm::client().unwrap();
    let snapshot = query_at(&http, &origin, "synthetic-access", "acct_demo")
        .await
        .unwrap();
    assert!(snapshot.refresh_message.is_some());
}

#[tokio::test]
async fn wallet_of_another_account_is_rejected() {
    let (origin, _server) = test_server(vec![(
        200,
        wallet_body("acct_other", "0", "0"),
        Duration::ZERO,
    )])
    .await;
    let http = crate::tokenrhythm::client().unwrap();
    assert!(
        query_at(&http, &origin, "synthetic-access", "acct_demo")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wallet_rejects_a_failed_envelope_and_expired_session() {
    for (status, body) in [
        (200, String::from(r#"{"code":1001,"data":null}"#)),
        (401, String::from(r#"{"error":"ACCOUNT_TOKEN_EXPIRED"}"#)),
    ] {
        let (origin, _server) = test_server(vec![(status, body, Duration::ZERO)]).await;
        let http = crate::tokenrhythm::client().unwrap();
        assert!(
            query_at(&http, &origin, "synthetic-access", "acct_demo")
                .await
                .is_err()
        );
    }
}

#[test]
fn decimals_follow_the_wallet_contract() {
    for value in ["0", "-1.5", "12.0000000000000001"] {
        assert!(is_signed_decimal(value), "{value}");
    }
    for value in ["", "1.", ".5", "1e3", "12.00000000000000001", "--1"] {
        assert!(!is_signed_decimal(value), "{value}");
    }
}

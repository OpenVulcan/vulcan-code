//! Wallet precision, credential-role, and HTTP failure tests.
//! 钱包精度、凭据角色及 HTTP 失败测试。

use super::*;
use crate::{CredentialSecretEnvelope, CredentialSecretMaterial, open_design::test_server};
use std::time::Duration;

/// Verifies exact decimal preservation and rejection of numbers, negative balances, and exponents.
/// 验证精确十进制保留，拒绝数字类型、负余额及指数形式。
#[test]
fn open_design_wallet_preserves_exact_decimal() {
    // snapshot contains more precision than a binary float can preserve.
    // snapshot 包含超过二进制浮点数可保留的精度。
    let snapshot = decode_balance(r#"{"balanceUsd":"12345678901234567890.12345678901234567890","updatedAt":"2026-10-14T00:00:00Z"}"#).unwrap();
    assert_eq!(
        snapshot.balance_amount.as_deref(),
        Some("12345678901234567890.12345678901234567890")
    );
    assert_eq!(snapshot.currency.as_deref(), Some("USD"));
    assert!(snapshot.quota_windows.is_empty() && snapshot.used_amount.is_none());
    assert_eq!(
        decode_balance(r#"{"balanceUsd":"0.0000"}"#)
            .unwrap()
            .balance_amount
            .as_deref(),
        Some("0.0000")
    );
    for body in [
        r#"{"balanceUsd":1}"#,
        r#"{"balanceUsd":"-1"}"#,
        r#"{"balanceUsd":"1e3"}"#,
        r#"{"balanceUsd":""}"#,
        r#"{"balanceUsd":"1.2.3"}"#,
        r#"{"balanceUsd":".5"}"#,
        r#"{"balanceUsd":"1."}"#,
        "{}",
    ] {
        assert!(decode_balance(body).is_err());
    }
}

/// Verifies the wire uses only the account key and rejected HTTP bodies never leak credentials.
/// 验证线协议仅使用账户密钥，被拒绝的 HTTP 正文绝不泄露凭据。
#[tokio::test]
async fn open_design_wallet_uses_control_key_and_reports_auth_expiry() {
    // envelope reproduces the paired material produced by a successful device transaction.
    // envelope 复现成功设备事务产生的成对材料。
    let envelope = CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenDesignAuth {
        runtime_key: String::from("runtime-only"),
        control_key: String::from("control-only"),
        user_id: String::from("synthetic-user"),
        email: String::from("test@example.invalid"),
    })
    .unwrap();
    let secret = envelope.execution_secret().unwrap();
    for status in [200, 401, 403, 500, 302] {
        // Response text intentionally includes a secret on failure to exercise diagnostic redaction.
        // 失败响应故意含有秘密，以验证诊断不泄露正文。
        let body = if status == 200 {
            r#"{"balanceUsd":"1.234500"}"#
        } else {
            r#"{"error":"control-only runtime-only"}"#
        };
        let (origin, server) = test_server(vec![(status, body.to_string(), Duration::ZERO)]).await;
        let result = query_at(
            &crate::open_design::client().unwrap(),
            &format!("{origin}/api/v1/wallet/balance"),
            secret.open_design_control_key().unwrap(),
        )
        .await;
        if status == 200 {
            assert_eq!(result.unwrap().balance_amount.as_deref(), Some("1.234500"));
        } else {
            let error = result.unwrap_err();
            assert!(!error.contains("control-only") && !error.contains("runtime-only"));
        }
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /api/v1/wallet/balance HTTP/1.1"));
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer control-only\r\n")
        );
        assert!(!requests[0].contains("runtime-only"));
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

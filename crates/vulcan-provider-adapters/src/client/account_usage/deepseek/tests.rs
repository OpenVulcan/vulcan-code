//! Regression tests for account_usage/deepseek.
//! account_usage/deepseek 的回归测试。

use super::*;

/// Verifies DeepSeek balances preserve every currency and exact decimal string.
/// 验证 DeepSeek 余额会保留每种币种及精确十进制字符串。
#[test]
fn deepseek_balance_preserves_multiple_currencies() {
    let snapshot = decode_deepseek_balance(
        r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"},{"currency":"USD","total_balance":"2.50","granted_balance":"0","topped_up_balance":"2.50"}]}"#,
    )
    .expect("valid DeepSeek balance");
    assert_eq!(snapshot.quota_windows.len(), 2);
    assert_eq!(
        snapshot.quota_windows[0].remaining.as_deref(),
        Some("110.00")
    );
    assert_eq!(snapshot.quota_windows[1].unit, "USD");
    assert!(snapshot.balance_amount.is_none());
    assert!(snapshot.currency.is_none());
}

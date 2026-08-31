//! Regression tests for account_usage/decimal.
//! account_usage/decimal 的回归测试。

use super::*;

/// Verifies OpenRouter credit balances retain decimal precision and exponent semantics.
/// 验证 OpenRouter 余额保留十进制精度及指数语义。
#[test]
fn openrouter_credit_subtraction_is_exact() {
    assert_eq!(
        subtract_non_negative_decimal("100.10", "0.09").expect("decimal subtraction"),
        "100.01"
    );
    assert_eq!(
        subtract_non_negative_decimal("1e2", "2.5").expect("exponent subtraction"),
        "97.5"
    );
    assert!(subtract_non_negative_decimal("1", "2").is_err());
}

/// Verifies exact decimal addition handles fractional and exponent representations.
/// 验证精确十进制加法可处理小数与指数表示。
#[test]
fn command_code_credit_addition_is_exact() {
    assert_eq!(
        add_non_negative_decimals(&["70", "2.5", "0.25"]).expect("credit addition"),
        "72.75"
    );
    assert_eq!(
        add_non_negative_decimals(&["1e2", "0.01"]).expect("exponent addition"),
        "100.01"
    );
}

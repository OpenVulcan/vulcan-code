//! Regression tests for minimax_registration.
//! minimax_registration 的回归测试。

use super::*;

/// Verifies MiniMax billing profiles are selected only from reviewed literal prefixes.
/// 验证 MiniMax 计费配置档仅从已审核的字面前缀中选择。
#[test]
fn minimax_key_prefixes_select_exact_profiles() {
    assert_eq!(
        minimax_key_profile_id("sk-api-example").expect("API key should classify"),
        "payg-api-key"
    );
    assert_eq!(
        minimax_key_profile_id("sk-cp-example").expect("Plan key should classify"),
        "token-plan-key"
    );
    assert!(minimax_key_profile_id("sk-example").is_err());
    assert!(minimax_key_profile_id("SK-CP-example").is_err());
}

/// Verifies regional registration rejects ambiguous and fully rejected probe outcomes.
/// 验证区域注册会拒绝歧义结果与全部拒绝结果。
#[test]
fn minimax_region_requires_exactly_one_successful_probe() {
    assert_eq!(
        resolve_minimax_region(Ok(()), Err(String::from("rejected"))).expect("global region"),
        "global"
    );
    assert_eq!(
        resolve_minimax_region(Err(String::from("rejected")), Ok(())).expect("China region"),
        "cn"
    );
    assert!(resolve_minimax_region(Ok(()), Ok(())).is_err());
    assert!(
        resolve_minimax_region(
            Err(String::from("global rejected")),
            Err(String::from("China rejected"))
        )
        .is_err()
    );
}

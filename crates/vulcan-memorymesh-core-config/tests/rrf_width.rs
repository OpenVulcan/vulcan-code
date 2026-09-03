//! Verifies exact wide RRF values and arithmetic identities in compiled configuration snapshots.
//! 本文件属于配置契约测试层，验证编译快照中的精确宽RRF参数及算术身份。

use std::collections::BTreeSet;
use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry};
use vulcan_memorymesh_core_contracts::{ErrorCode, RrfMathProfile};

/// Registers both arithmetic profiles under one name and checks each exact value has an independent immutable policy identity.
/// 在同一名称下注册两种算术，验证每个精确参数都有独立不可变策略身份；无输入，返回配置错误。
#[test]
fn wide_rrf_configuration_is_exact_and_digest_bound() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = ConfigRegistry::new()?;
    let mut identities = BTreeSet::new();
    let mut digests = BTreeSet::new();
    for parameter in [
        1,
        4_294_967_296,
        9_007_199_254_740_992,
        9_007_199_254_740_993,
        i64::MAX.unsigned_abs(),
    ] {
        for profile in [
            RrfMathProfile::Signed64WrappingThenFloat64V1,
            RrfMathProfile::Float64AddV1,
        ] {
            let mut bundle = ConfigBundle::default();
            bundle.memory_policy.retrieval_policy.rrf_k = parameter;
            bundle.memory_policy.retrieval_policy.rrf_math_profile = profile;
            let snapshot = registry.register(bundle)?;
            assert_eq!(snapshot.memory_policy.retrieval_policy.rrf_k, parameter);
            assert_eq!(
                snapshot.memory_policy.retrieval_policy.rrf_math_profile,
                profile
            );
            assert!(identities.insert(snapshot.id.clone()));
            assert!(digests.insert(snapshot.memory_policy_digest.clone()));
        }
    }
    assert_eq!(identities.len(), 10);
    Ok(())
}

/// Rejects invalid source ranges and the former narrow policy schema instead of silently converting them.
/// 拒绝非法来源值域和旧窄策略版本，不静默转换；无输入，返回验证错误。
#[test]
fn wide_rrf_configuration_rejects_invalid_or_retired_values()
-> Result<(), Box<dyn std::error::Error>> {
    for parameter in [0, i64::MAX.unsigned_abs() + 1, u64::MAX] {
        let mut bundle = ConfigBundle::default();
        bundle.memory_policy.retrieval_policy.rrf_k = parameter;
        let error = ConfigRegistry::new()?
            .register(bundle)
            .err()
            .ok_or("invalid RRF parameter accepted")?;
        assert_eq!(error.code, ErrorCode::InvalidConfiguration);
    }
    let mut bundle = ConfigBundle::default();
    bundle.memory_policy.retrieval_policy.schema_version = 6;
    assert_eq!(
        ConfigRegistry::new()?
            .register(bundle)
            .err()
            .ok_or("old retrieval schema accepted")?
            .code,
        ErrorCode::InvalidConfiguration
    );
    Ok(())
}

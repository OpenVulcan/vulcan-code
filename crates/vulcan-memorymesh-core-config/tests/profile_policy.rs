//! Profile formula parity, validation, role isolation and frozen snapshot coverage.
//! 画像公式对齐、校验、角色隔离及冻结快照覆盖。

use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigRegistry, ProfileLifetimePolicy, ProfilePolicy,
};
use vulcan_memorymesh_core_contracts::{ProfileLevel, SectionRole};

/// Verifies external defaults, capped refresh extensions and the non-expiring persistent level.
/// 验证外置默认值、有上限的刷新延长及不过期的持久等级。
#[test]
fn default_formulas_match_external_levels_and_caps() {
    let policy = ProfileLifetimePolicy::default();
    const DAY: u64 = 86_400_000;
    for (level, base, step, cap) in [
        (ProfileLevel::Transient, 3, 2, 14),
        (ProfileLevel::Situational, 14, 7, 90),
        (ProfileLevel::Stable, 90, 30, 365),
    ] {
        assert_eq!(policy.lifetime_ms(level, 0), Some(base * DAY));
        assert_eq!(policy.lifetime_ms(level, 1), Some((base + step) * DAY));
        assert_eq!(policy.lifetime_ms(level, u32::MAX), Some(cap * DAY));
    }
    assert_eq!(policy.lifetime_ms(ProfileLevel::Persistent, u32::MAX), None);
    let mut separate = ProfilePolicy::default();
    separate.user.transient.base_ms = 1;
    separate.user.transient.step_ms = 0;
    assert_eq!(
        separate
            .for_role(SectionRole::Subject)
            .lifetime_ms(ProfileLevel::Transient, 99),
        Some(1)
    );
    assert_eq!(separate.for_role(SectionRole::Environment), &policy);
}

/// Every one of the eighteen role/level/formula fields changes snapshot identity without mutating old policy.
/// 十八个角色/等级/公式字段均改变快照身份，且不修改旧策略。
#[test]
fn all_profile_fields_are_frozen_and_hashed() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let original = registry.register(ConfigBundle::default()).expect("default");
    for role in 0..2 {
        for level in 0..3 {
            for field in 0..3 {
                let mut bundle = ConfigBundle::default();
                let policy = if role == 0 {
                    &mut bundle.memory_policy.profiles.user
                } else {
                    &mut bundle.memory_policy.profiles.project
                };
                let rule = match level {
                    0 => &mut policy.transient,
                    1 => &mut policy.situational,
                    _ => &mut policy.stable,
                };
                match field {
                    0 => rule.base_ms += 1,
                    1 => rule.step_ms += 1,
                    _ => rule.max_ms += 1,
                }
                let changed = registry.register(bundle).expect("valid change");
                assert_ne!(
                    changed.id, original.id,
                    "role {role}, level {level}, field {field}"
                );
                assert_ne!(changed.memory_policy_digest, original.memory_policy_digest);
                assert_eq!(original.memory_policy.profiles, ProfilePolicy::default());
            }
        }
    }
}

/// Invalid finite rules are rejected during admission, including rules for the other role.
/// 非法有限规则在准入时被拒绝，包括另一角色的规则。
#[test]
fn invalid_profile_rules_never_register() {
    let mut registry = ConfigRegistry::new().expect("registry");
    for role in 0..2 {
        for values in [(0, 0, 10), (11, 0, 10), (1, u64::MAX, 10), (1, 0, u64::MAX)] {
            let mut bundle = ConfigBundle::default();
            let policy = if role == 0 {
                &mut bundle.memory_policy.profiles.user
            } else {
                &mut bundle.memory_policy.profiles.project
            };
            policy.stable.base_ms = values.0;
            policy.stable.step_ms = values.1;
            policy.stable.max_ms = values.2;
            assert!(registry.register(bundle).is_err());
        }
    }
}

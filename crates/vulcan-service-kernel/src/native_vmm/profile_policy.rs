//! Exact mapping between native profile settings and immutable Core lifetime rules.
//! 原生画像设置与不可变 Core 生命周期规则之间的准确映射。

use vulcan_memorymesh_core::{ProfileLifetimePolicy, ProfileLifetimeRule, ProfilePolicy};
use vulcan_service_api::{
    VmmNativeProfileLifetimeRule, VmmNativeProfileLifetimes, VmmNativeProfilePolicySummary,
};

/// Returns Core-owned defaults for both roles without creating a user file.
/// 返回 Core 持有的两种角色默认值，不创建用户文件。
pub(super) fn defaults() -> VmmNativeProfilePolicySummary {
    let policy = ProfilePolicy::default();
    VmmNativeProfilePolicySummary {
        user: public_lifetimes(policy.user),
        project: public_lifetimes(policy.project),
    }
}

/// Maps exact user/project groups to the immutable Core policy; Core validates numeric relationships.
/// 将准确用户/项目组映射到不可变 Core 策略；数值关系由 Core 校验。
pub(super) fn resolve(policy: &VmmNativeProfilePolicySummary) -> ProfilePolicy {
    ProfilePolicy {
        user: core_lifetimes(&policy.user),
        project: core_lifetimes(&policy.project),
    }
}

/// Projects one Core lifetime group using the public millisecond units without rounding.
/// 使用公开毫秒单位投影一组 Core 生命周期，不进行舍入。
fn public_lifetimes(policy: ProfileLifetimePolicy) -> VmmNativeProfileLifetimes {
    VmmNativeProfileLifetimes {
        transient: public_rule(policy.transient),
        situational: public_rule(policy.situational),
        stable: public_rule(policy.stable),
    }
}

/// Copies the exact three duration fields for display and explicit reset.
/// 复制准确的三个期限字段，用于显示与显式重置。
fn public_rule(rule: ProfileLifetimeRule) -> VmmNativeProfileLifetimeRule {
    VmmNativeProfileLifetimeRule {
        base_ms: rule.base_ms,
        step_ms: rule.step_ms,
        max_ms: rule.max_ms,
    }
}

/// Resolves one independently owned role with no fallback to another role's rules.
/// 解析一种独立归属角色，不回退到另一角色的规则。
pub(super) fn core_lifetimes(policy: &VmmNativeProfileLifetimes) -> ProfileLifetimePolicy {
    ProfileLifetimePolicy {
        transient: core_rule(&policy.transient),
        situational: core_rule(&policy.situational),
        stable: core_rule(&policy.stable),
    }
}

/// Preserves all three integer durations for the Core formula.
/// 为 Core 公式保留三个整数期限。
fn core_rule(rule: &VmmNativeProfileLifetimeRule) -> ProfileLifetimeRule {
    ProfileLifetimeRule {
        base_ms: rule.base_ms,
        step_ms: rule.step_ms,
        max_ms: rule.max_ms,
    }
}

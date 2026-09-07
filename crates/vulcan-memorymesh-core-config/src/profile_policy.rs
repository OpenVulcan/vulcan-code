//! Immutable user/project profile lifetime rules for newly materialized nodes.
//! 用于新物化节点的不可变用户/项目画像生命周期规则。

use vulcan_memorymesh_core_contracts::{ProfileLevel, SectionRole};

/// Configures one finite profile level; all values are milliseconds.
/// 配置一个有限画像等级；全部值使用毫秒。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileLifetimeRule {
    /// Lifetime before any reaffirmation or replacement.
    /// 尚无重申或替代时的生命周期。
    pub base_ms: u64,
    /// Additional lifetime for each existing refresh-weight increment; zero disables extension.
    /// 既有刷新权重每次增加对应的额外期限；零表示不延长。
    pub step_ms: u64,
    /// Upper bound on base plus refresh extensions.
    /// 基础期限加刷新延长期限的上限。
    pub max_ms: u64,
}

/// Owns finite-level rules for exactly one profile role; Persistent always remains non-expiring.
/// 持有恰好一种画像角色的有限等级规则；持久级始终不过期。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileLifetimePolicy {
    /// Rules for short-lived profile facts.
    /// 短期画像事实的规则。
    pub transient: ProfileLifetimeRule,
    /// Rules for situational profile facts.
    /// 情境画像事实的规则。
    pub situational: ProfileLifetimeRule,
    /// Rules for stable profile facts.
    /// 稳定画像事实的规则。
    pub stable: ProfileLifetimeRule,
}

impl Default for ProfileLifetimePolicy {
    /// Returns the exact external VMM lifetime formula constants.
    /// 返回外置 VMM 生命周期公式的准确常量。
    fn default() -> Self {
        const DAY_MS: u64 = 86_400_000;
        Self {
            transient: ProfileLifetimeRule {
                base_ms: 3 * DAY_MS,
                step_ms: 2 * DAY_MS,
                max_ms: 14 * DAY_MS,
            },
            situational: ProfileLifetimeRule {
                base_ms: 14 * DAY_MS,
                step_ms: 7 * DAY_MS,
                max_ms: 90 * DAY_MS,
            },
            stable: ProfileLifetimeRule {
                base_ms: 90 * DAY_MS,
                step_ms: 30 * DAY_MS,
                max_ms: 365 * DAY_MS,
            },
        }
    }
}

impl ProfileLifetimePolicy {
    /// Validates finite positive lifetimes and bounded extensions without clamping user configuration.
    /// 校验有限正生命周期与有界延长量，不截断用户配置。
    /// Returns an error for an invalid rule; zero refresh extension remains valid.
    /// 对非法规则返回错误；零刷新延长量仍合法。
    pub fn validate(&self) -> Result<(), String> {
        const MAX_DURATION_MS: u64 = 10 * 365 * 86_400_000;
        for rule in [&self.transient, &self.situational, &self.stable] {
            if rule.base_ms == 0
                || rule.base_ms > rule.max_ms
                || rule.max_ms > MAX_DURATION_MS
                || rule.step_ms > MAX_DURATION_MS
            {
                return Err("Profile lifetime must satisfy 1 <= base <= maximum <= ten years, with extension between zero and ten years".to_owned());
            }
        }
        Ok(())
    }

    /// Evaluates the admitted formula for the selected level and existing refresh weight.
    /// 按选定等级及既有刷新权重计算已准入公式。
    /// Returns a capped finite duration, or None only for the explicit Persistent level.
    /// 返回受限的有限期限，仅对显式持久级返回 None。
    pub fn lifetime_ms(&self, level: ProfileLevel, refresh_weight: u32) -> Option<u64> {
        let rule = match level {
            ProfileLevel::Transient => &self.transient,
            ProfileLevel::Situational => &self.situational,
            ProfileLevel::Stable => &self.stable,
            ProfileLevel::Persistent => return None,
        };
        Some(
            rule.base_ms
                .saturating_add(rule.step_ms.saturating_mul(u64::from(refresh_weight)))
                .min(rule.max_ms),
        )
    }
}

/// Separates global user rules from project defaults in the same immutable command policy.
/// 在同一不可变命令策略中分离全局用户规则与项目默认规则。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfilePolicy {
    /// Global fixed-user rules; project-specific overrides must never replace them.
    /// 全局固定用户规则；项目级覆盖绝不能替换它们。
    pub user: ProfileLifetimePolicy,
    /// Default project rules, independently resolved for a project's admitted command.
    /// 项目默认规则，为项目的准入命令独立解析。
    pub project: ProfileLifetimePolicy,
}

impl ProfilePolicy {
    /// Selects by the command's explicit typed role, never by a guessed scope name or node identity.
    /// 按命令显式强类型角色选择，不猜测作用域名称或节点身份。
    pub fn for_role(&self, role: SectionRole) -> &ProfileLifetimePolicy {
        match role {
            SectionRole::Subject => &self.user,
            SectionRole::Environment => &self.project,
        }
    }
}

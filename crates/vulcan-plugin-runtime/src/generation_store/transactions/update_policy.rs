//! Original plugin policy is durable before package preparation and survives manifest default changes.
//! 原插件策略在插件包准备前持久保存，并跨清单默认值变化保留。

use serde::{Deserialize, Serialize};

/// Exact plugin-specific override membership and predecessor baseline captured by the host update coordinator.
/// 宿主更新协调器捕获的精确插件专属覆盖成员关系及前驱基线。
/// This contains no other plugin settings and cannot authorize restoring an entire configuration snapshot.
/// 此结构不含其他插件设置，不能授权恢复整个配置快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginUpdatePolicy {
    /// Original membership in the explicit enabled set; this takes precedence in the provider contract.
    /// 原显式启用集合成员关系；供应商契约规定其优先。
    pub explicitly_enabled: bool,
    /// Original membership in the explicit disabled set, retained exactly even before normalization.
    /// 原显式停用集合成员关系，即使尚未归一化也精确保留。
    pub explicitly_disabled: bool,
    /// Default from the predecessor manifest, never from the candidate or a later filesystem scan.
    /// 来自前驱清单的默认值，绝不来自候选或后续文件系统扫描。
    pub predecessor_default_enabled: bool,
}

impl VulcanPluginUpdatePolicy {
    /// Resolve the frozen effective state using enabled, then disabled, then predecessor-default precedence.
    /// 按启用、停用、前驱默认值的优先顺序解析固定有效状态。
    /// Returns the user's update target independent of future candidate defaults.
    /// 返回独立于未来候选默认值的用户更新目标。
    pub fn desired_enabled(&self) -> bool {
        self.explicitly_enabled || (!self.explicitly_disabled && self.predecessor_default_enabled)
    }

    /// Compare current `enabled` and `disabled` membership with the exact original policy; unrelated settings are excluded.
    /// 比较当前 `enabled` 与 `disabled` 成员关系和精确原策略；不包含无关设置。
    pub fn matches_overrides(&self, enabled: bool, disabled: bool) -> bool {
        self.explicitly_enabled == enabled && self.explicitly_disabled == disabled
    }
}

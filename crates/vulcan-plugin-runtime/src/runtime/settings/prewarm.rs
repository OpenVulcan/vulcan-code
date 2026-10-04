//! Explicit automatic targets describe host intent, never current VM availability or a permanent reservation.
//! 明确自动目标描述宿主意图，绝不代表当前 VM 可用量或永久预留。

use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// A configured target selects one exact declared reusable tool when the host admits a fresh lifecycle trigger.
/// 配置目标在宿主接纳新生命周期触发时选择一个精确声明的复用工具。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginAutomaticPrewarmRule {
    /// Plugin identity is independent from the frozen generation of a later execution receipt.
    /// 插件身份独立于后续执行回执冻结的代次。
    pub plugin_id: String,
    /// The globally unique declared tool must still exist and declare reuse at actual admission.
    /// 实际入场时，全局唯一声明工具必须仍存在且声明复用。
    pub tool_id: String,
    /// Positive desired confirmed idle count; this is not a permanent minimum or an unbounded refill loop.
    /// 正的期望已确认空闲数量；不是永久最小值或无界补齐循环。
    pub target_idle: usize,
}

impl VulcanPluginRuntimeSettings {
    /// Validate rule uniqueness and configured capacity limits without guessing a package or allocating VMs.
    /// 校验规则唯一性及已配置容量上限，不猜测包或分配 VM。
    /// Return an error before publication; manifest and workspace checks belong to actual trigger admission.
    /// 发布前返回错误；清单及工作区检查属于实际触发入场。
    pub(super) fn validate_automatic_prewarm(&self) -> Result<(), String> {
        // Existing pool limits bound explicit rule metadata; runtime owners still bound actual domains.
        // 既有池上限约束明确规则元数据；运行时所有者仍约束实际域。
        if self.automatic_prewarm.len() > self.parent.max_registered_pools {
            return Err("Automatic prewarm rules exceed parent pool registration limits.".into());
        }
        // Global tool IDs are unique in the catalog, so conflicting plugin ownership is rejected too.
        // 工具 ID 在目录中全局唯一，因此同时拒绝冲突的插件归属。
        let mut tools = BTreeSet::new();
        let mut counts = BTreeMap::<&str, usize>::new();
        for rule in &self.automatic_prewarm {
            crate::catalog::validate_identifier(&rule.plugin_id, "automatic prewarm plugin")?;
            crate::catalog::validate_identifier(&rule.tool_id, "automatic prewarm tool")?;
            if !tools.insert(&rule.tool_id) {
                return Err(format!(
                    "Duplicate automatic prewarm tool `{}`.",
                    rule.tool_id
                ));
            }
            // Targets share the same parent/plugin/purpose resolver as ordinary tool registration.
            // 目标与普通工具注册共用父级／插件／用途解析器。
            let (capacity, _) =
                self.capacity_budget(&rule.plugin_id, VulcanPluginCapacityPurpose::ToolCall);
            if rule.target_idle == 0 || rule.target_idle > capacity.resources.max_resident_vms {
                return Err(format!(
                    "Automatic prewarm target for `{}` must be positive and within tool-call capacity.",
                    rule.tool_id
                ));
            }
            let count = counts.entry(&rule.plugin_id).or_default();
            *count += 1;
            if *count > self.plugin_budget(&rule.plugin_id).0.max_registered_pools {
                return Err(format!(
                    "Automatic prewarm rules exceed plugin `{}` pool registration limits.",
                    rule.plugin_id
                ));
            }
        }
        if counts.len() > self.parent.max_registered_plugins {
            return Err(
                "Automatic prewarm owners exceed parent plugin registration limits.".into(),
            );
        }
        Ok(())
    }
}

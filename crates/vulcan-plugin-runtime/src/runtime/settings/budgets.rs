//! Resolve one immutable aggregate budget and preserve exact ownership of configured overrides.
//! 解析单个不可变聚合预算，并保留配置覆盖的精确归属。

use super::*;
use serde::de::{Error, MapAccess, Visitor};

/// The authoritative source of every field in one effective aggregate budget.
/// 单个有效聚合预算全部字段的权威来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginBudgetSource {
    /// No plugin override exists; values are derived from the frozen parent configuration.
    /// 不存在插件覆盖；值从冻结父级配置派生。
    Parent,
    /// One complete explicit host override supplies every aggregate field.
    /// 单个完整显式宿主覆盖提供全部聚合字段。
    HostOverride,
}

impl VulcanPluginRuntimeSettings {
    /// Resolve the exact plugin budget from already validated immutable settings.
    /// 从已校验不可变设置解析精确插件预算。
    /// The plugin ID selects one whole override or explicit parent inheritance; no values are clamped.
    /// 插件标识选择单个整体覆盖或明确父级继承；不裁剪任何值。
    /// Returns the effective upstream policy and its single source without changing either configuration.
    /// 返回有效上游策略及其唯一来源，不改变任一配置。
    pub(in crate::runtime) fn plugin_budget(
        &self,
        plugin_id: &str,
    ) -> (EmbeddedPluginConfig, VulcanPluginBudgetSource) {
        match self.plugin_budgets.get(plugin_id) {
            Some(budget) => (budget.clone(), VulcanPluginBudgetSource::HostOverride),
            None => (
                EmbeddedPluginConfig {
                    max_registered_pools: self.parent.max_registered_pools,
                    max_sessions: self.parent.max_sessions,
                    max_resident_vms: self.parent.max_resident_vms,
                    max_running_calls: self.parent.max_running_calls,
                    max_queued_calls: self.parent.max_queued_calls,
                    max_queued_bytes: self.parent.max_queued_bytes,
                    max_operations: self.parent.max_operations,
                },
                VulcanPluginBudgetSource::Parent,
            ),
        }
    }
}

/// Deserialize exact plugin IDs once; repeated keys are errors rather than last-write-wins policy changes.
/// 一次性反序列化精确插件标识；重复键为错误，不采用最后写入覆盖策略。
/// The deserializer provides one map; returns unique upstream policies or a format diagnostic.
/// 反序列化器提供单个映射；返回唯一上游策略或格式诊断。
pub(super) fn deserialize_unique_budgets<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, EmbeddedPluginConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    /// Visit one configured policy map while preserving duplicate-key evidence.
    /// 遍历单个已配置策略映射，同时保留重复键证据。
    struct Budgets;
    impl<'de> Visitor<'de> for Budgets {
        /// Exact map returned to the settings owner.
        /// 返回给设置所有者的精确映射。
        type Value = BTreeMap<String, EmbeddedPluginConfig>;

        /// Describe the expected map to the supplied error formatter; return the formatting outcome.
        /// 向给定错误格式化器说明预期映射；返回格式化结果。
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a map of unique plugin IDs to complete aggregate budgets")
        }

        /// Read the supplied entries without normalization; return an error on duplicate ownership.
        /// 读取给定条目且不进行规范化；归属重复时返回错误。
        fn visit_map<M>(self, mut entries: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            // Each exact ID is authoritative at most once in this file.
            // 本文件中每个精确标识最多一次作为权威。
            let mut budgets = BTreeMap::new();
            while let Some((plugin_id, budget)) =
                entries.next_entry::<String, EmbeddedPluginConfig>()?
            {
                if budgets.insert(plugin_id.clone(), budget).is_some() {
                    return Err(M::Error::custom(format!(
                        "Duplicate plugin budget owner {plugin_id}."
                    )));
                }
            }
            Ok(budgets)
        }
    }
    deserializer.deserialize_map(Budgets)
}

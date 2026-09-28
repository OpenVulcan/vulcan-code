//! Resolve immutable purpose budgets without duplicating upstream resource contracts.
//! 解析不可变用途预算，不复制上游资源契约。

use super::*;
use luaskills::runtime::embedded::{EmbeddedCapacityConfig, PoolKind, VmCapacityConfig};
use serde::de::{Error, MapAccess, Visitor};
use std::fmt::Debug;
use std::marker::PhantomData;

/// Host-selected execution purpose; plugin business arguments cannot select capacity ownership.
/// 宿主选择的执行用途；插件业务参数不能选择容量归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginCapacityPurpose {
    /// Ordinary tool invocations keep their declared single-call semantics.
    /// 普通工具调用保留声明的单次调用语义。
    ToolCall,
    /// Controller-owned tool sessions retain one exact VM for their lifetime.
    /// 控制器拥有的工具会话在整个寿命内保留一个精确 VM。
    ToolSession,
    /// Persistent Hook instances preserve their ordering and original shutdown declaration.
    /// 持久 Hook 实例保留顺序及原关闭声明。
    Hook,
    /// Transient and persistent UI Actions share limits without sharing Lua state.
    /// 瞬时及持久 UI Action 共享上限，不共享 Lua 状态。
    UiAction,
    /// Host lifecycle callbacks retain their own capacity across catalog generations.
    /// 宿主生命周期回调跨目录代次保留自身容量。
    Lifecycle,
    /// Activation probes consume real initialization resources without a business export.
    /// 启用探测消费真实初始化资源，不具有业务导出。
    ActivationProbe,
}

/// Source of a complete effective purpose budget, separate from plugin aggregate provenance.
/// 完整有效用途预算的来源，独立于插件聚合来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginCapacityBudgetSource {
    /// Shared unreserved capacity derives every maximum from the resolved plugin aggregate.
    /// 无预留公共容量从已解析插件聚合派生全部上限。
    PluginBudget,
    /// A complete host override supplies every capacity field without per-field merging.
    /// 完整宿主覆盖提供全部容量字段，不逐字段合并。
    HostOverride,
}

impl VulcanPluginRuntimeSettings {
    /// Resolve plugin_id and purpose from frozen settings; return full upstream config and its source.
    /// 从冻结设置解析 plugin_id 及 purpose；返回完整上游配置及其来源。
    /// Missing purposes explicitly inherit shared aggregate maxima; no declaration is clipped or persisted.
    /// 缺省用途明确继承公共聚合上限；不裁剪或持久化任何声明。
    pub(in crate::runtime) fn capacity_budget(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> (EmbeddedCapacityConfig, VulcanPluginCapacityBudgetSource) {
        if let Some(config) = self
            .capacity_budgets
            .get(plugin_id)
            .and_then(|purposes| purposes.get(&purpose))
        {
            return (
                config.clone(),
                VulcanPluginCapacityBudgetSource::HostOverride,
            );
        }
        // Inheritance is defined by the settings contract and never selects an alternative runtime identity.
        // 继承由设置契约定义，绝不选择备用运行时身份。
        let (plugin, _) = self.plugin_budget(plugin_id);
        (
            EmbeddedCapacityConfig {
                resources: VmCapacityConfig {
                    kind: PoolKind::Shared,
                    min_resident_vms: 0,
                    max_resident_vms: plugin.max_resident_vms,
                    max_running_calls: plugin.max_running_calls,
                },
                max_queued_calls: plugin.max_queued_calls,
                max_queued_bytes: plugin.max_queued_bytes,
            },
            VulcanPluginCapacityBudgetSource::PluginBudget,
        )
    }

    /// Validate every configured owner before workers start; return precise conflicts without changing limits.
    /// 在工作线程启动前校验全部配置所有者；返回精确冲突，不改变上限。
    /// Configured minima must fit simultaneously, including currently uninstalled plugins.
    /// 配置最小值必须可同时满足，包括当前未安装的插件。
    pub(super) fn validate_capacity_budgets(&self) -> Result<(), String> {
        // Count configured commitments without allocating or claiming that VMs have been initialized.
        // 统计配置承诺，不分配或宣称 VM 已初始化。
        let mut parent_minimum = 0usize;
        // The core's existing registration limit bounds the explicit capacity inventory too.
        // 核心既有注册上限也约束显式容量清单。
        let mut configured_capacities = 0usize;
        if self
            .capacity_budgets
            .values()
            .filter(|purposes| !purposes.is_empty())
            .count()
            > self.parent.max_registered_plugins
        {
            return Err(
                "Configured capacity owners exceed parent plugin registration limits.".into(),
            );
        }
        for (plugin_id, purposes) in &self.capacity_budgets {
            crate::catalog::validate_identifier(plugin_id, "capacity owner")?;
            // The same aggregate resolver governs both startup validation and actual registration.
            // 同一聚合解析器同时治理启动校验及实际注册。
            let (plugin, _) = self.plugin_budget(plugin_id);
            if purposes.len() > plugin.max_registered_pools {
                return Err(format!(
                    "Configured capacities exceed plugin {plugin_id} registration limits."
                ));
            }
            // Each purpose owns one minimum, independent of the number of module generations.
            // 每个用途拥有一份最小预留，独立于模块代次数量。
            let mut plugin_minimum = 0usize;
            for (purpose, config) in purposes {
                config.validate(&self.parent, &plugin).map_err(|error| {
                    format!("Invalid capacity for {plugin_id}/{purpose:?}: {error}")
                })?;
                plugin_minimum = plugin_minimum
                    .checked_add(config.resources.min_resident_vms)
                    .ok_or_else(|| "Configured plugin capacity guarantees overflow.".to_owned())?;
            }
            if plugin_minimum > plugin.max_resident_vms {
                return Err(format!(
                    "Configured capacity guarantees exceed plugin {plugin_id} resident limits."
                ));
            }
            parent_minimum = parent_minimum
                .checked_add(plugin_minimum)
                .ok_or_else(|| "Configured parent capacity guarantees overflow.".to_owned())?;
            configured_capacities = configured_capacities
                .checked_add(purposes.len())
                .ok_or_else(|| "Configured capacity count overflows.".to_owned())?;
        }
        if parent_minimum > self.parent.max_resident_vms
            || configured_capacities > self.parent.max_registered_pools
        {
            return Err(
                "Configured capacities exceed parent guarantees or registration limits.".into(),
            );
        }
        Ok(())
    }
}

/// A decoding-only map preserves duplicate-key evidence at every configured ownership level.
/// 仅用于解码的映射在每个配置归属层保留重复键证据。
struct UniqueMap<K, V>(BTreeMap<K, V>);

impl<'de, K, V> Deserialize<'de> for UniqueMap<K, V>
where
    K: Ord + Debug + Deserialize<'de>,
    V: Deserialize<'de>,
{
    /// Decode supplied map bytes once; return unique keys or an explicit duplicate diagnostic.
    /// 一次解码给定映射字节；返回唯一键或明确重复诊断。
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Retain the exact key and value types while visiting unnormalized JSON entries.
        /// 遍历未经规范化的 JSON 条目时保留精确键及值类型。
        struct UniqueVisitor<K, V>(PhantomData<(K, V)>);
        impl<'de, K, V> Visitor<'de> for UniqueVisitor<K, V>
        where
            K: Ord + Debug + Deserialize<'de>,
            V: Deserialize<'de>,
        {
            /// Parsed map retains the same typed ownership as the caller.
            /// 解析后的映射保留与调用方相同的类型化归属。
            type Value = UniqueMap<K, V>;
            /// Write the expected format into formatter; return the formatting result.
            /// 向 formatter 写入预期格式；返回格式化结果。
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a capacity map with unique ownership keys")
            }
            /// Consume entries without collapsing duplicates; return the exact typed map.
            /// 消费 entries 而不消除重复；返回精确类型化映射。
            fn visit_map<M: MapAccess<'de>>(self, mut entries: M) -> Result<Self::Value, M::Error> {
                // Reject duplicates before replacing the first authoritative value.
                // 替换第一个权威值之前拒绝重复。
                let mut values = BTreeMap::new();
                while let Some((key, value)) = entries.next_entry::<K, V>()? {
                    if values.contains_key(&key) {
                        return Err(M::Error::custom(format!(
                            "Duplicate capacity ownership key {key:?}."
                        )));
                    }
                    values.insert(key, value);
                }
                Ok(UniqueMap(values))
            }
        }
        deserializer.deserialize_map(UniqueVisitor(PhantomData))
    }
}

/// Decode the exact plugin/purpose map from deserializer; reject duplicate owners, purposes or unknown purposes.
/// 从 deserializer 解码精确插件／用途映射；拒绝重复所有者、用途或未知用途。
/// Return complete upstream configurations without inserting defaults into explicit overrides.
/// 返回完整上游配置，不向显式覆盖插入默认值。
pub(super) fn deserialize_unique_capacities<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, BTreeMap<VulcanPluginCapacityPurpose, EmbeddedCapacityConfig>>, D::Error>
{
    // Both map layers must be checked before converting to the public settings shape.
    // 转换为公开设置形状前，必须检查两层映射。
    let values = UniqueMap::<String, UniqueMap<VulcanPluginCapacityPurpose, EmbeddedCapacityConfig>>::deserialize(deserializer)?;
    Ok(values
        .0
        .into_iter()
        .map(|(plugin_id, purposes)| (plugin_id, purposes.0))
        .collect())
}

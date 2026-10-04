//! One host-resolved aggregate budget applies to every production route and generation of a plugin.
//! 单个宿主解析聚合预算应用于插件全部生产路由及代次。

use super::*;

#[cfg(test)]
mod tests;
use luaskills::runtime::embedded::{
    EmbeddedErrorCode, EmbeddedPluginConfig, EmbeddedPluginSnapshot,
};

/// An actual core plugin snapshot with the authority that selected its immutable budget.
/// 实际核心插件快照及选择其不可变预算的权威。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalPluginStatus {
    /// The original parent identity also identifies this immutable configuration lifetime.
    /// 原父级身份同时标识此不可变配置寿命。
    pub runtime_id: String,
    /// Every aggregate field comes from this one source; no hidden per-field fallback is applied.
    /// 全部聚合字段来自此唯一来源；不应用隐藏逐字段回退。
    pub budget_source: VulcanPluginBudgetSource,
    /// Exact upstream usage includes draining pools, queued work and retained results.
    /// 精确上游用量包含排空中的池、排队工作及保留结果。
    pub snapshot: EmbeddedPluginSnapshot,
}

impl ManagedFormalRuntime {
    /// Register or verify the plugin's one resolved budget while the host holds its publication gate.
    /// 宿主持有发布门时，注册或验证插件唯一已解析预算。
    /// The exact plugin ID and caller-held gate select admission; return the unchanged effective core budget.
    /// 精确插件标识及调用方持有的门选择入场；返回未改变的有效核心预算。
    /// Existing mismatched or closed ownership is rejected, never silently replaced or reopened.
    /// 拒绝既有不匹配或已关闭所有权，绝不静默替换或重新开启。
    pub(super) fn ensure_plugin_budget(
        &self,
        plugin_id: &str,
        _publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<EmbeddedPluginConfig, VulcanPluginRuntimeError> {
        // Every route resolves from the same settings frozen before runtime creation.
        // 每条路由从运行时创建前冻结的同一设置解析。
        let (budget, _) = self.settings.plugin_budget(plugin_id);
        match self.host.runtime().plugin(plugin_id) {
            Ok(existing) => {
                if existing.config != budget {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Plugin {plugin_id} core budget differs from its frozen host configuration."
                    )));
                }
                if existing.closing {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Plugin {plugin_id} core admission is permanently closed."
                    )));
                }
            }
            Err(error) if error.code == EmbeddedErrorCode::NotFound => self
                .host
                .runtime()
                .register_plugin(plugin_id.to_owned(), budget.clone())
                .map_err(calls::core_error)?,
            Err(error) => return Err(calls::core_error(error)),
        }
        Ok(budget)
    }
}

impl VulcanPluginRuntimeManager {
    /// Observe a registered plugin's actual aggregate resources and frozen policy source.
    /// 观测已注册插件的实际聚合资源及冻结策略来源。
    /// The exact ID selects one core owner; return None before parent attachment or plugin registration.
    /// 精确标识选择单个核心所有者；父级未附加或插件未注册时返回空值。
    /// This read never registers a plugin, rewrites configuration or reopens a closed owner.
    /// 此读取绝不注册插件、回写配置或重新开启已关闭所有者。
    pub fn formal_plugin_status(
        &self,
        plugin_id: &str,
    ) -> Result<Option<VulcanPluginFormalPluginStatus>, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "budget owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // An absent runtime is an actual state rather than a trigger for lazy construction.
        // 缺少运行时是真实状态，不作为延迟构造的触发器。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // Query the same retained core identity, including closing generations and physical work.
        // 查询同一保留核心身份，包含关闭中的代次及物理工作。
        let snapshot = match owner.host.runtime().plugin(plugin_id) {
            Ok(snapshot) => snapshot,
            Err(error) if error.code == EmbeddedErrorCode::NotFound => return Ok(None),
            Err(error) => return Err(calls::core_error(error)),
        };
        // Frozen host settings are the sole authority for policy provenance.
        // 冻结宿主设置是策略来源的唯一权威。
        let (budget, budget_source) = owner.settings.plugin_budget(plugin_id);
        if snapshot.config != budget {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Plugin {plugin_id} core budget differs from its frozen host configuration."
            )));
        }
        Ok(Some(VulcanPluginFormalPluginStatus {
            runtime_id: owner.host.runtime().id().to_owned(),
            budget_source,
            snapshot,
        }))
    }
}

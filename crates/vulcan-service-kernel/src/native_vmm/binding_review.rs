//! Commit-time admission for reviewed native project binding plans.
//! 已审查原生项目绑定计划的提交期准入。

use super::*;

impl NativeVmmMaintenance {
    /// Freezes project association changes after checking every currently unbound reviewed host has a decision.
    /// 检查每个当前未绑定的已审查宿主都有决定后，冻结项目关联变化。
    ///
    /// # Parameters
    /// 参数
    /// `host_project_ids` is the exact active host scope captured by enable-time preflight.
    /// `host_project_ids` 是启用预检捕获的准确活动宿主范围。
    /// `planned_project_ids` contains hosts with already validated explicit binding operations.
    /// `planned_project_ids` 包含已有经校验明确绑定操作的宿主。
    ///
    /// # Returns
    /// 返回值
    /// The existing project mutation guard, held through global persistence, or a missing-decision error.
    /// 返回须持有至全局持久化完成的既有项目修改守卫，或缺少决定的错误。
    /// Drop the guard before executing binding methods that acquire the same gate.
    /// 在执行会获取同一门禁的绑定方法之前释放守卫。
    pub(crate) async fn freeze_project_binding_coverage(
        &self,
        host_project_ids: &BTreeSet<String>,
        planned_project_ids: &BTreeSet<String>,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        // The same gate already serializes bind, unbind, deletion and the catalog projection.
        // 同一门禁已串行化绑定、解绑、删除与目录投影。
        let guard = self.project_mutation_gate.clone().lock_owned().await;
        // Query the real bindings without calling project_catalog, which would acquire this gate again.
        // 直接查询真实绑定，不调用会再次获取此门禁的 project_catalog。
        let bound_ids = self
            .data_plane
            .list_project_bindings()
            .await?
            .into_iter()
            .map(|binding| binding.vulcan_project_id)
            .collect::<BTreeSet<_>>();
        // A concurrently chosen binding is already covered and must remain protected by if_unbound execution.
        // 并发选择的绑定已经覆盖，仍须由 if_unbound 执行保护。
        // Only newly uncovered hosts need a fresh user decision; do not silently invent one.
        // 只有新出现的未覆盖宿主需要用户重新决定，不静默生成决定。
        if let Some(host_id) = host_project_ids.iter().find(|host_id| {
            !bound_ids.contains(*host_id) && !planned_project_ids.contains(*host_id)
        }) {
            return Err(format!(
                "原生 VMM 项目 `{host_id}` 在预检后变为未绑定，请重新加载项目列表并确认绑定计划后保存。"
            ));
        }
        Ok(guard)
    }
}

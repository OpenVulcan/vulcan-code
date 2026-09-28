//! Deterministic formal plugin drain ownership and cleanup diagnostics.
//! 确定性正式插件排空所有权与清理诊断。

use super::{
    VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
};
use crate::VulcanPluginRuntimeState;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Reports completed plugin cleanup and retained failures after drain attempts.
/// 报告排空尝试中完成的插件清理与保留的失败。
#[derive(Debug, Clone, Default)]
pub struct VulcanPluginDrainReport {
    /// Number of tracked formal calls whose actual pool ownership has retired.
    /// 真实池所有权已退役的受跟踪正式调用数量。
    pub closed_formal_calls: usize,
    /// Number of sessions closed and removed successfully.
    /// 成功关闭并移除的会话数量。
    pub closed_sessions: usize,
    /// Number of persistent plugin hook runtimes closed successfully.
    /// 成功关闭的持久插件 hook 运行时数量。
    pub closed_hook_runtimes: usize,
    /// Number of persistent plugin UI action runtimes closed successfully.
    /// 成功关闭的持久插件界面 Action 运行时数量。
    pub closed_ui_action_runtimes: usize,
    /// Deterministic cleanup failures retained for diagnostics and retry.
    /// 为诊断与重试保留的确定性清理失败集合。
    pub failures: Vec<String>,
}

impl VulcanPluginDrainReport {
    /// Merges another drain phase into this ordered lifecycle report.
    /// 将另一个排空阶段合并到当前有序生命周期报告。
    /// other contributes ordered counts and diagnostics to self; returns no value.
    /// other向self贡献有序计数与诊断；不返回值。
    fn merge(&mut self, other: Self) {
        self.closed_formal_calls = self
            .closed_formal_calls
            .saturating_add(other.closed_formal_calls);
        self.closed_sessions = self.closed_sessions.saturating_add(other.closed_sessions);
        self.closed_hook_runtimes = self
            .closed_hook_runtimes
            .saturating_add(other.closed_hook_runtimes);
        self.closed_ui_action_runtimes = self
            .closed_ui_action_runtimes
            .saturating_add(other.closed_ui_action_runtimes);
        self.failures.extend(other.failures);
    }
}

impl VulcanPluginRuntimeManager {
    /// Closes every retained session owned by one draining plugin.
    /// 关闭一个正在排空的插件所拥有的全部保留会话。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin whose runtime resources must be drained.
    /// `plugin_id` 是需要排空运行时资源的精确插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns counts and non-fatal cleanup failures for the drain attempt.
    /// 返回本次排空尝试的计数与非致命清理失败。
    pub async fn drain_plugin_sessions(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let manager = self.clone();
        let plugin_id = plugin_id.to_owned();
        tokio::spawn(async move { manager.drain_plugin_sessions_owned(&plugin_id).await })
            .await
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!("Plugin drain supervisor failed: {error}"))
            })?
    }

    /// Finishes plugin drain despite cancellation of the caller awaiting its report.
    /// 即使等待报告的调用方取消，也继续完成插件排空。
    /// The plugin identity selects retained resources; returns the exact cleanup report.
    /// 插件身份选择保留资源；返回精确清理报告。
    async fn drain_plugin_sessions_owned(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let mut report = VulcanPluginDrainReport::default();
        report.merge(self.drain_formal_calls(Some(plugin_id), None).await?);
        report.merge(self.drain_pinned(Some(plugin_id), None).await?);
        for kind in [
            VulcanPluginPersistentRuntimeKind::Hook,
            VulcanPluginPersistentRuntimeKind::UiAction,
        ] {
            report.merge(self.drain_formal_instance(plugin_id, kind).await?);
        }
        Ok(report)
    }

    /// Closes every retained plugin session owned by one archived conversation.
    /// 关闭一个已归档会话所拥有的全部保留插件会话。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the stable owner identifier attached at session creation.
    /// `owner_session_id` 是创建会话时绑定的稳定所有者标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns counts and non-fatal cleanup failures for all matching resources.
    /// 返回所有匹配资源的计数与非致命清理失败。
    pub async fn drain_owner_sessions(
        &self,
        owner_session_id: &str,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let mut affected_plugin_ids = BTreeSet::new();
        // Include formal-only owners so a completed retry clears the original drain warning.
        // 包含仅持有正式调用的所有者，使重试完成后清除原排空警告。
        affected_plugin_ids.extend(
            self.formal_calls(owner_session_id)?
                .into_iter()
                .map(|call| call.plugin_id),
        );
        affected_plugin_ids.extend(
            self.formal_sessions(owner_session_id)?
                .into_iter()
                .map(|session| session.plugin_id),
        );
        let mut report = VulcanPluginDrainReport::default();
        report.merge(
            self.drain_formal_calls(None, Some(owner_session_id))
                .await?,
        );
        report.merge(self.drain_pinned(None, Some(owner_session_id)).await?);
        for plugin_id in affected_plugin_ids {
            self.clear_cleanup_diagnostics_if_fully_drained(&plugin_id)?;
        }
        Ok(report)
    }

    /// Retains one cleanup diagnostic so UI and later refreshes cannot hide leaked resources.
    /// 保留一条清理诊断，确保界面与后续刷新不会隐藏泄漏资源。
    /// plugin_id selects the owner and diagnostic is retained without duplication; returns publication status.
    /// plugin_id选择所有者，不重复保留diagnostic；返回发布状态。
    pub(super) fn record_cleanup_failure(
        &self,
        plugin_id: &str,
        diagnostic: String,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Serialize diagnostics with lifecycle capacity retirement and its final catalog publication.
        // 将诊断与生命周期容量回收及最终目录发布排序。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.record_cleanup_failure_locked(plugin_id, diagnostic, &registration)
    }

    /// Retain diagnostic for plugin_id under an existing publication guard; return after its live projection is updated.
    /// 在既有发布守卫下为 plugin_id 保留 diagnostic；更新实时投影后返回。
    pub(super) fn record_cleanup_failure_locked(
        &self,
        plugin_id: &str,
        diagnostic: String,
        registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let plugin_diagnostics = diagnostics.entry(plugin_id.to_owned()).or_default();
        if !plugin_diagnostics.contains(&diagnostic) {
            plugin_diagnostics.push(diagnostic);
        }
        drop(diagnostics);
        self.publish_cleanup_diagnostics(plugin_id, registration)
    }

    /// Publishes newly recorded cleanup failures without waiting for a later catalog refresh.
    /// 无需等待后续目录刷新即可发布新记录的清理失败。
    /// plugin_id selects the immutable catalog projection to update; returns unit or a lock diagnostic.
    /// plugin_id选择要更新的不可变目录投影；返回空值或锁诊断。
    fn publish_cleanup_diagnostics(
        &self,
        plugin_id: &str,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let Some(plugin_diagnostics) = cleanup_diagnostics.get(plugin_id) else {
            return Ok(());
        };
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        if plugin.runtime_state == VulcanPluginRuntimeState::Disabled {
            plugin.runtime_state = VulcanPluginRuntimeState::DisabledWithCleanupWarning;
        }
        for diagnostic in plugin_diagnostics {
            if !plugin.diagnostics.contains(diagnostic) {
                plugin.diagnostics.push(diagnostic.clone());
            }
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Clears stale cleanup diagnostics only after every resource owned by one plugin is gone.
    /// 仅在单个插件拥有的全部资源均已消失后清除陈旧清理诊断。
    /// plugin_id selects all registries and capacity counters; removes diagnostics only if none remain and returns status.
    /// plugin_id选择全部注册表与容量计数；仅在没有残留时移除诊断并返回状态。
    pub(super) fn clear_cleanup_diagnostics_if_fully_drained(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.clear_cleanup_diagnostics_locked(plugin_id, &_registration)
    }

    /// Clear resolved diagnostics for plugin_id while the caller holds publication; retain explicit unfinished capacity retirement.
    /// 调用方持有发布门时清除 plugin_id 已解决诊断；保留明确未完成容量回收。
    /// Return success after projection, or a precise ownership error without claiming cleanup completion.
    /// 投影后返回成功；或返回精确归属错误而不宣称清理完成。
    pub(super) fn clear_cleanup_diagnostics_locked(
        &self,
        plugin_id: &str,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if let Some(owner) = self.formal_runtime_owner()?
            && owner.has_retiring_capacities(plugin_id)?
        {
            return Ok(());
        }
        // Probe preparation has no legacy session quota, but still retains real cleanup ownership.
        // 探测准备不占旧会话配额，但仍保留真实清理所有权。
        let has_formal_calls = self.has_unretired_formal_calls(plugin_id)?;
        let has_hook_runtime =
            self.has_formal_instance(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        let has_ui_action_runtime =
            self.has_formal_instance(plugin_id, VulcanPluginPersistentRuntimeKind::UiAction)?;
        let has_active_count = self
            .state
            .active_session_counts
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
            })?
            .get(plugin_id)
            .copied()
            .unwrap_or_default()
            > 0;
        if has_formal_calls || has_hook_runtime || has_ui_action_runtime || has_active_count {
            return Ok(());
        }
        let removed_diagnostics = self
            .state
            .cleanup_diagnostics
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
            })?
            .remove(plugin_id)
            .unwrap_or_default();
        if removed_diagnostics.is_empty() {
            return Ok(());
        }
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        plugin
            .diagnostics
            .retain(|diagnostic| !removed_diagnostics.contains(diagnostic));
        if plugin.runtime_state == VulcanPluginRuntimeState::DisabledWithCleanupWarning {
            plugin.runtime_state = VulcanPluginRuntimeState::Disabled;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

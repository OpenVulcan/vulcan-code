//! Deterministic plugin drain ownership, retained-lease retries, and cleanup diagnostics.
//! 确定性插件排空所有权、保留租约重试与清理诊断。

use super::{
    RetainedVulcanPluginLease, VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager, lock_hook_runtimes, lock_retained_leases,
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
    /// Number of previously retained raw leases closed successfully.
    /// 成功关闭的此前保留原始租约数量。
    pub closed_retained_leases: usize,
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
        self.closed_retained_leases = self
            .closed_retained_leases
            .saturating_add(other.closed_retained_leases);
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
        self.drain_persistent_runtime(
            plugin_id,
            VulcanPluginPersistentRuntimeKind::Hook,
            &mut report,
        )
        .await?;
        self.drain_persistent_runtime(
            plugin_id,
            VulcanPluginPersistentRuntimeKind::UiAction,
            &mut report,
        )
        .await?;
        let retained = self.take_retained_leases(|lease| lease.plugin_id == plugin_id)?;
        report.merge(self.drain_retained_leases(retained).await?);
        Ok(report)
    }

    /// Drains one category-isolated persistent runtime into the shared lifecycle report.
    /// 把一个按类别隔离的持久运行时排空到共享生命周期报告中。
    /// plugin_id and runtime_kind select one registry; appends cleanup results to report and returns unit or a lock error.
    /// plugin_id与runtime_kind选择一个注册表；向report追加清理结果，返回空值或锁错误。
    async fn drain_persistent_runtime(
        &self,
        plugin_id: &str,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
        report: &mut VulcanPluginDrainReport,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let gate = self.persistent_runtime_gate(plugin_id, runtime_kind)?;
        let _lifecycle = gate.lock().await;
        let runtimes = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => &self.state.hook_runtimes,
            VulcanPluginPersistentRuntimeKind::UiAction => &self.state.ui_action_runtimes,
        };
        let runtime = lock_hook_runtimes(runtimes)?.get(plugin_id).cloned();
        let Some(runtime) = runtime else {
            return Ok(());
        };
        match self
            .close_persistent_runtime(Arc::clone(&runtime), runtime_kind)
            .await
        {
            Ok(shutdown_diagnostic) => {
                lock_hook_runtimes(runtimes)?
                    .retain(|id, current| id != plugin_id || !Arc::ptr_eq(current, &runtime));
                match runtime_kind {
                    VulcanPluginPersistentRuntimeKind::Hook => {
                        report.closed_hook_runtimes += 1;
                    }
                    VulcanPluginPersistentRuntimeKind::UiAction => {
                        report.closed_ui_action_runtimes += 1;
                    }
                }
                if let Some(diagnostic) = shutdown_diagnostic {
                    report.failures.push(diagnostic);
                }
            }
            Err(error) => {
                // A failed close remains quarantined; subsequent drains may retry cleanup.
                // 关闭失败的实例保持隔离；后续排空可以重试清理。
                let diagnostic = format!(
                    "Plugin `{plugin_id}` persistent {} runtime cleanup failed and remains retained: {error}",
                    runtime_kind.label()
                );
                self.record_cleanup_failure(plugin_id, diagnostic.clone())?;
                report.failures.push(diagnostic);
            }
        }
        Ok(())
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
        let retained = self.take_retained_leases(|lease| {
            lease.owner_session_id.as_deref() == Some(owner_session_id)
        })?;
        affected_plugin_ids.extend(retained.iter().map(|lease| lease.plugin_id.clone()));
        report.merge(self.drain_retained_leases(retained).await?);
        for plugin_id in affected_plugin_ids {
            self.clear_cleanup_diagnostics_if_fully_drained(&plugin_id)?;
        }
        Ok(report)
    }

    /// Retries exact closure for raw leases retained by earlier failed cleanup paths.
    /// 对此前清理失败路径保留的原始租约重试精确关闭。
    /// leases contains atomically claimed retry identities; returns cleanup counts while restoring failed entries.
    /// leases包含原子领取的重试身份；返回清理计数并恢复失败条目。
    async fn drain_retained_leases(
        &self,
        leases: Vec<RetainedVulcanPluginLease>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let mut report = VulcanPluginDrainReport::default();
        for lease in leases {
            match self
                .close_raw_lease(&lease.lease_id, &lease.sid, lease.generation)
                .await
            {
                Ok(()) => {
                    if lease.reserved_session_slot {
                        self.release_session_slot(&lease.plugin_id);
                    }
                    report.closed_retained_leases += 1;
                }
                Err(error) => {
                    self.restore_retained_lease(lease.clone())?;
                    let diagnostic = format!(
                        "Retained lease `{}` cleanup retry failed: {error}",
                        lease.lease_id
                    );
                    self.record_cleanup_failure(&lease.plugin_id, diagnostic.clone())?;
                    report.failures.push(diagnostic);
                }
            }
        }
        Ok(report)
    }

    /// Atomically claims retained raw leases that match one exact lifecycle predicate.
    /// 原子领取匹配一个精确生命周期谓词的保留原始租约。
    /// predicate selects identities under one lock; returns and removes only the matching leases.
    /// predicate在同一个锁内选择身份；返回并移除匹配租约。
    fn take_retained_leases(
        &self,
        predicate: impl Fn(&RetainedVulcanPluginLease) -> bool,
    ) -> Result<Vec<RetainedVulcanPluginLease>, VulcanPluginRuntimeError> {
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        let lease_ids = retained
            .values()
            .filter(|lease| predicate(lease))
            .map(|lease| lease.lease_id.clone())
            .collect::<Vec<_>>();
        Ok(lease_ids
            .into_iter()
            .filter_map(|lease_id| retained.remove(&lease_id))
            .collect())
    }

    /// Retains one raw lease after immediate cleanup fails and records a visible diagnostic.
    /// 在立即清理失败后保留一个原始租约，并记录可见诊断。
    /// lease preserves the retry identity and diagnostic is published; returns unit or a duplicate/lock error.
    /// lease保留重试身份并发布diagnostic；返回空值或重复/锁错误。
    pub(super) fn retain_failed_lease(
        &self,
        lease: RetainedVulcanPluginLease,
        diagnostic: String,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let plugin_id = lease.plugin_id.clone();
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Retained Vulcan Plugin lease id `{lease_id}` already exists."
            )));
        }
        retained.insert(lease_id, lease);
        drop(retained);
        self.record_cleanup_failure(&plugin_id, diagnostic)
    }

    /// Restores one atomically claimed lease after its cleanup retry fails.
    /// 在清理重试失败后恢复一个已被原子领取的租约。
    /// lease is one failed claimed retry; returns unit after reinsertion or an explicit concurrent-restore error.
    /// lease是一次失败的已领取重试；重新插入后返回空值，否则显式报告并发恢复错误。
    fn restore_retained_lease(
        &self,
        lease: RetainedVulcanPluginLease,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Retained Vulcan Plugin lease id `{lease_id}` was restored concurrently."
            )));
        }
        retained.insert(lease_id, lease);
        Ok(())
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
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let plugin_diagnostics = diagnostics.entry(plugin_id.to_owned()).or_default();
        if !plugin_diagnostics.contains(&diagnostic) {
            plugin_diagnostics.push(diagnostic);
        }
        drop(diagnostics);
        self.publish_cleanup_diagnostics(plugin_id)
    }

    /// Publishes newly recorded cleanup failures without waiting for a later catalog refresh.
    /// 无需等待后续目录刷新即可发布新记录的清理失败。
    /// plugin_id selects the immutable catalog projection to update; returns unit or a lock diagnostic.
    /// plugin_id选择要更新的不可变目录投影；返回空值或锁诊断。
    fn publish_cleanup_diagnostics(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
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
        let has_retained_leases = lock_retained_leases(&self.state.retained_leases)?
            .values()
            .any(|lease| lease.plugin_id == plugin_id);
        let has_hook_runtime =
            lock_hook_runtimes(&self.state.hook_runtimes)?.contains_key(plugin_id);
        let has_ui_action_runtime =
            lock_hook_runtimes(&self.state.ui_action_runtimes)?.contains_key(plugin_id);
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
        if has_retained_leases || has_hook_runtime || has_ui_action_runtime || has_active_count {
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

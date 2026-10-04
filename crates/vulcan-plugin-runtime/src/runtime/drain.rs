//! Deterministic formal plugin drain ownership and cleanup diagnostics.
//! 确定性正式插件排空所有权与清理诊断。

use super::{
    VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
};
use std::collections::BTreeSet;

/// Preserve the exact cleanup error with its proven retirement origin; unknown errors remain failures.
/// 保留精确清理错误及其已证回收来源；未知错误仍为失败。
pub(in crate::runtime) enum CleanupOriginError {
    /// Only unfinished preparation or retained native members may produce this finite observation.
    /// 只有未完成准备或保留的原生成员可产生此有限观测。
    Pending(VulcanPluginRuntimeError),
    /// Native, policy, lock and activation errors require explicit failure acknowledgement.
    /// 原生、策略、锁及启用错误需要显式失败确认。
    Failure(VulcanPluginRuntimeError),
}

impl CleanupOriginError {
    /// Return whether this error originates from a proven finite physical owner.
    /// 返回此错误是否来自已证有限物理所有者。
    pub(in crate::runtime) fn is_pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }

    /// Consume the classification and return the unchanged original public error.
    /// 消耗分类并返回未变的原公开错误。
    pub(in crate::runtime) fn into_runtime_error(self) -> VulcanPluginRuntimeError {
        match self {
            Self::Pending(error) | Self::Failure(error) => error,
        }
    }
}

impl From<VulcanPluginRuntimeError> for CleanupOriginError {
    /// Classify an otherwise unknown error as Failure without changing its original value.
    /// 将其他未知错误归类为 Failure，不改变原值。
    fn from(error: VulcanPluginRuntimeError) -> Self {
        Self::Failure(error)
    }
}

/// Proven failure ownership controls explicit acknowledgement; unknown owners remain retained.
/// 已证失败归属控制显式确认；未知所有者继续保留。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CleanupFailureSource {
    /// No specific resource has acknowledged this terminal or unknown failure.
    /// 没有特定资源确认此终态或未知失败。
    Unknown,
    /// The exact reusable domain owns the failure and its explicit acknowledgement.
    /// 精确可复用域拥有此失败及其显式确认。
    ReusableDomain(String),
}

/// One unique diagnostic retains its public message and all actual failure owners.
/// 一条唯一诊断保留公开消息及全部实际失败所有者。
pub(super) struct CleanupDiagnostic {
    /// Original ordered public diagnostic bytes remain unchanged.
    /// 原有序公开诊断字节保持不变。
    pub(super) message: String,
    /// Empty means proven finite Pending; any source means Failure and dominates Pending.
    /// 空集合表示已证有限 Pending；任一来源表示 Failure，优先于 Pending。
    failure_sources: BTreeSet<CleanupFailureSource>,
}

/// Reports completed plugin cleanup and retained failures after drain attempts.
/// 报告排空尝试中完成的插件清理与保留的失败。
#[derive(Debug, Clone, Default)]
pub struct VulcanPluginDrainReport {
    /// Number of tracked formal calls whose request ownership, including any reusable borrow, has retired.
    /// 请求归属已退役的受跟踪正式调用数量，包含其可复用借用。
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

/// Internal cleanup evidence separates a finite pending observation from an actual terminal failure.
/// 内部清理证据区分有限未完成观测与实际终态失败。
#[derive(Default)]
pub(in crate::runtime) struct DrainObservation {
    /// The unchanged public report preserves every count and diagnostic for existing callers.
    /// 未变公开报告为既有调用方保留全部计数与诊断。
    pub(in crate::runtime) report: VulcanPluginDrainReport,
    /// A real failure must stay sticky even after its physical record leaves the live registry.
    /// 即使物理记录离开活动注册表，真实失败也必须持续保留。
    pub(in crate::runtime) terminal_failure: bool,
}

impl DrainObservation {
    /// Append diagnostic from a proven budget-expiration branch; preserve pending authority and return no value.
    /// 追加已证预算到期分支的 diagnostic；保留待完成权威，不返回值。
    pub(in crate::runtime) fn pending(&mut self, diagnostic: String) {
        self.report.failures.push(diagnostic);
    }

    /// Append an actual terminal or unknown cleanup diagnostic and retain its failure classification; return no value.
    /// 追加实际终态或未知清理 diagnostic 并保留失败分类；不返回值。
    pub(in crate::runtime) fn failed(&mut self, diagnostic: String) {
        self.terminal_failure = true;
        self.report.failures.push(diagnostic);
    }

    /// Merge other's exact report and sticky classification into this observation; return no value.
    /// 将 other 的精确报告及持续失败分类合并至当前观测；不返回值。
    fn merge(&mut self, other: Self) {
        self.terminal_failure |= other.terminal_failure;
        self.report.merge(other.report);
    }
}

/// Only a known finite deadline may yield Pending; synchronization and real cleanup errors remain Err.
/// 仅已知有限截止时间可产生 Pending；同步及真实清理错误保持 Err。
pub(in crate::runtime) enum RetirementObservation {
    /// Original native owners have physically retired.
    /// 原生原所有者已经物理退役。
    Retired,
    /// The budget expired while the same original owner remains retained.
    /// 预算到期时相同原所有者仍被保留。
    Pending(VulcanPluginRuntimeError),
}

impl RetirementObservation {
    /// Project this typed observation to the original waiting API; return its unchanged error or successful retirement.
    /// 将此类型化观测投影至原等待接口；返回未变错误或成功退役。
    pub(in crate::runtime) fn into_result(self) -> Result<(), VulcanPluginRuntimeError> {
        match self {
            Self::Retired => Ok(()),
            Self::Pending(error) => Err(error),
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Test all tracked physical retirement owners while registration is excluded.
    /// 排除注册时检查全部受跟踪物理退役所有者。
    /// plugin_id selects one namespace; registration proves publication exclusion; true means work remains.
    /// plugin_id 选择命名空间；registration 证明发布互斥；true 表示仍有工作。
    pub(super) fn has_unretired_plugin_resources_locked(
        &self,
        plugin_id: &str,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        if let Some(owner) = self.formal_runtime_owner()?
            && owner.has_retiring_capacities(plugin_id)?
        {
            return Ok(true);
        }
        // Probe preparation has no legacy session quota, but still retains real cleanup ownership.
        // 探测准备不占旧会话配额，但仍保留真实清理所有权。
        let has_formal_calls = self.has_unretired_formal_calls(plugin_id)?;
        let has_hook_runtime =
            self.has_formal_instance(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        let has_ui_action_runtime =
            self.has_formal_instance(plugin_id, VulcanPluginPersistentRuntimeKind::UiAction)?;
        let has_active_count = self.active_session_count(plugin_id)? > 0;
        if self.has_reusable_domains(plugin_id)?
            || has_formal_calls
            || has_hook_runtime
            || has_ui_action_runtime
            || has_active_count
        {
            return Ok(true);
        }
        Ok(false)
    }

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
    pub(super) async fn drain_plugin_sessions_owned(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        // Keep the exact report even if retirement races its final pending reconciliation.
        // 即使退役与最终待完成对账竞态，也保留精确报告。
        let observation = self.observe_plugin_sessions_owned(plugin_id).await?;
        self.clear_cleanup_diagnostics_if_fully_drained(plugin_id)?;
        Ok(observation.report)
    }

    /// Observe plugin_id's original cleanup with internal failure classification; return exact evidence without replay.
    /// 以内部失败分类观测 plugin_id 的原清理；返回精确证据，不重放。
    pub(super) async fn observe_plugin_sessions_owned(
        &self,
        plugin_id: &str,
    ) -> Result<DrainObservation, VulcanPluginRuntimeError> {
        let mut observation = DrainObservation::default();
        observation.merge(self.observe_formal_calls(Some(plugin_id), None).await?);
        observation.merge(self.observe_reusable_domains(Some(plugin_id), None).await?);
        observation.merge(self.observe_pinned(Some(plugin_id), None).await?);
        for kind in [
            VulcanPluginPersistentRuntimeKind::Hook,
            VulcanPluginPersistentRuntimeKind::UiAction,
        ] {
            observation.merge(self.observe_formal_instance(plugin_id, kind).await?);
        }
        Ok(observation)
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
        affected_plugin_ids.extend(
            self.reusable_tool_domains(owner_session_id)?
                .into_iter()
                .map(|domain| domain.plugin_id),
        );
        let mut report = VulcanPluginDrainReport::default();
        report.merge(
            self.drain_formal_calls(None, Some(owner_session_id))
                .await?,
        );
        report.merge(
            self.drain_reusable_domains(None, Some(owner_session_id))
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
    /// plugin_id selects the owner and diagnostic is retained without duplication; returns accounting status.
    /// plugin_id选择所有者，不重复保留diagnostic；返回记账状态。
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

    /// Retain diagnostic for plugin_id under an existing publication guard; return after its unique live map is updated.
    /// 在既有发布守卫下为 plugin_id 保留 diagnostic；更新唯一实时映射后返回。
    pub(super) fn record_cleanup_failure_locked(
        &self,
        plugin_id: &str,
        diagnostic: String,
        registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.record_cleanup_diagnostic_locked(
            plugin_id,
            diagnostic,
            Some(CleanupFailureSource::Unknown),
            registration,
        )
    }

    /// Record a proven finite diagnostic for plugin_id; diagnostic preserves its message; return accounting status.
    /// 为 plugin_id 登记已证有限诊断；diagnostic 保留消息；返回记账状态。
    pub(super) fn record_cleanup_pending(
        &self,
        plugin_id: &str,
        diagnostic: String,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Publication excludes reconciliation until this observation is recorded.
        // 发布门在此观测记账前排除对账。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.record_cleanup_pending_locked(plugin_id, diagnostic, &registration)
    }

    /// Record proven finite diagnostic for plugin_id using the caller-held registration gate; return accounting status.
    /// 使用调用方持有的 registration 门为 plugin_id 登记已证有限 diagnostic；返回记账状态。
    pub(super) fn record_cleanup_pending_locked(
        &self,
        plugin_id: &str,
        diagnostic: String,
        registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.record_cleanup_diagnostic_locked(plugin_id, diagnostic, None, registration)
    }

    /// Record diagnostic for its exact domain under registration; failure selects actual Failure versus finite Pending.
    /// 在 registration 下为精确 domain 登记 diagnostic；failure 选择实际 Failure 或有限 Pending。
    /// Returns accounting status without changing public strings or other failure ownership.
    /// 返回记账状态，不改变公开字符串或其他失败归属。
    pub(super) fn record_domain_cleanup_locked(
        &self,
        plugin_id: &str,
        diagnostic: String,
        domain_id: &str,
        failure: bool,
        registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.record_cleanup_diagnostic_locked(
            plugin_id,
            diagnostic,
            failure.then(|| CleanupFailureSource::ReusableDomain(domain_id.to_owned())),
            registration,
        )
    }

    /// Merge message and source into plugin_id's sole map under registration; return explicit accounting status.
    /// 在 registration 下将 message 与 source 合并到 plugin_id 唯一映射；返回明确记账状态。
    /// None is accepted only from a proven finite branch; unknown failures always pass Unknown.
    /// None 仅接受已证有限分支；未知失败始终传入 Unknown。
    fn record_cleanup_diagnostic_locked(
        &self,
        plugin_id: &str,
        message: String,
        source: Option<CleanupFailureSource>,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Diagnostics remain the only ledger, ordered by first occurrence.
        // diagnostics 仍是唯一账本，按首次出现排序。
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        // Entries merge exact message duplicates without losing independent failure owners.
        // entries 合并精确消息重复项，不丢失独立失败所有者。
        let entries = diagnostics.entry(plugin_id.to_owned()).or_default();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.message == message) {
            if let Some(source) = source {
                entry.failure_sources.insert(source);
            }
        } else {
            entries.push(CleanupDiagnostic {
                message,
                failure_sources: source.into_iter().collect(),
            });
        }
        Ok(())
    }

    /// Observe unresolved cleanup without acquiring the registration gate again.
    /// 观测未解决清理，不再次获取登记门。
    /// `plugin_id` selects the live namespace; returns whether diagnostics remain or an explicit lock error.
    /// `plugin_id` 选择实时命名空间；返回是否残留诊断或明确锁错误。
    pub fn has_cleanup_diagnostics(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        self.state.has_cleanup_diagnostics(plugin_id)
    }

    /// Reconcile only finite Pending after every resource owned by plugin_id retires; return accounting status.
    /// 仅在 plugin_id 全部资源退役后对账有限 Pending；返回记账状态。
    /// Ordinary drain is not acknowledgement of terminal or unknown failures.
    /// 普通排空不是对终态或未知失败的确认。
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
    /// Return success after the live map update, or a precise ownership error without claiming cleanup completion.
    /// 实时映射更新后返回成功；或返回精确归属错误而不宣称清理完成。
    pub(super) fn clear_cleanup_diagnostics_locked(
        &self,
        plugin_id: &str,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if self.has_unretired_plugin_resources_locked(plugin_id, _registration)? {
            return Ok(());
        }
        // Preserve every Failure while removing only observations whose physical owners are gone.
        // 保留全部 Failure，仅移除物理所有者已消失的观测。
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        if let Some(entries) = diagnostics.get_mut(plugin_id) {
            entries.retain(|entry| !entry.failure_sources.is_empty());
            if entries.is_empty() {
                diagnostics.remove(plugin_id);
            }
        }
        Ok(())
    }

    /// Acknowledge only domain_id's proven failures in plugin_id under registration; retain all other sources.
    /// 在 registration 下仅确认 plugin_id 中 domain_id 的已证失败；保留全部其他来源。
    /// Returns status after exact domain removal has proved acknowledgement, without inferring from messages.
    /// 精确域移除已证明确认后返回状态，不从消息推断。
    pub(super) fn acknowledge_domain_cleanup_locked(
        &self,
        plugin_id: &str,
        domain_id: &str,
        registration: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Source selects one confirmed owner, including duplicates shared with unknown failures.
        // source 选择一个已确认所有者，包含与未知失败共用的重复项。
        let source = CleanupFailureSource::ReusableDomain(domain_id.to_owned());
        {
            // The sole map must not turn an acknowledged Failure into a surviving unowned Pending.
            // 唯一映射不得把已确认 Failure 变成残留无归属 Pending。
            let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
            })?;
            if let Some(entries) = diagnostics.get_mut(plugin_id) {
                entries.retain_mut(|entry| {
                    !entry.failure_sources.remove(&source) || !entry.failure_sources.is_empty()
                });
                if entries.is_empty() {
                    diagnostics.remove(plugin_id);
                }
            }
        }
        self.clear_cleanup_diagnostics_locked(plugin_id, registration)
    }
}

impl super::VulcanPluginManagerState {
    /// Inspect the unique cleanup map without acquiring publication or changing routing identity.
    /// 检查唯一清理映射，不获取发布门或改变路由身份。
    /// `plugin_id` selects the namespace; returns true for retained diagnostics or a poisoned-lock error.
    /// `plugin_id` 选择命名空间；保留诊断返回真，锁污染返回错误。
    pub(super) fn has_cleanup_diagnostics(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        self.cleanup_diagnostics
            .lock()
            .map(|diagnostics| {
                diagnostics
                    .get(plugin_id)
                    .is_some_and(|entries| !entries.is_empty())
            })
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
            })
    }
}

#[cfg(test)]
mod tests;

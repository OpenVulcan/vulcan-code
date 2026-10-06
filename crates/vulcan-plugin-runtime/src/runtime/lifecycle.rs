//! Plugin catalog projection, exact generation guards, and host lifecycle transitions.
//! 插件目录投影、精确代次守卫与宿主生命周期转换。

use super::authorization::plugin_is_authorized;
use super::{
    VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS, VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
    VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
};
use crate::{
    VulcanPluginAvailability, VulcanPluginCatalogSnapshot, VulcanPluginHookDescriptor,
    VulcanPluginPackageOrigin, VulcanPluginRuntimeState, VulcanPluginToolDescriptor,
    load_vulcan_plugin_catalog,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod predecessor_publication;
mod predecessor_restore;
pub use predecessor_publication::VulcanPluginActivationPredecessorPublication;
mod update_publication;
pub use update_publication::VulcanPluginActivationTargetPublication;

/// Host-owned plugin lifecycle event that may invoke one manifest callback.
/// 可调用清单回调的宿主自有插件生命周期事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginLifecycleEvent {
    /// Package installation completed and its private environment is ready.
    /// 插件包安装完成且私有环境已就绪。
    Install,
    /// Plugin is about to become enabled.
    /// 插件即将进入启用状态。
    Enable,
    /// Compensate a failed enable transition through the declared disable callback.
    /// 通过声明的停用回调补偿失败的启用转换。
    EnableRollback,
    /// Plugin has stopped accepting new work and is being disabled.
    /// 插件已停止接收新任务并正在停用。
    Disable,
    /// User package is about to be removed.
    /// 用户插件包即将被移除。
    Uninstall,
}

impl VulcanPluginRuntimeManager {
    /// Share the existing publication gate and transition rules with a concrete original update owner.
    /// 与具体原更新所有者共用既有发布门及转换规则。
    /// Optional update carries the frozen catalog and real candidate ownership; returns retained authority only for that path.
    /// 可选 update 携带固定目录及真实候选所有权；仅该路径返回保留权威。
    pub(super) fn begin_lifecycle_transition_with_update(
        &self,
        plugin_id: &str,
        runtime_state: VulcanPluginRuntimeState,
        allowed_source_states: &[VulcanPluginRuntimeState],
        update: Option<(
            &crate::VulcanPluginActivationCandidate<'_>,
            &VulcanPluginCatalogSnapshot,
        )>,
    ) -> Result<Option<Arc<super::update_drain::UpdateDrainAuthority>>, VulcanPluginRuntimeError>
    {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(
            &_registration,
            update.map(|(candidate, _)| {
                super::routing::ActivationRoutingOriginal::Candidate(candidate)
            }),
        )?;
        // A disabled recovery publication is not permission to start ordinary lifecycle effects.
        // 停用恢复发布不表示允许开始普通生命周期副作用。
        let (recovery_error, recovery_diagnostics) = self.activation_recovery_diagnostics(None)?;
        if let Some(diagnostic) = recovery_error
            .as_ref()
            .or_else(|| recovery_diagnostics.get(plugin_id))
        {
            return Err(VulcanPluginRuntimeError::new(diagnostic.clone()));
        }
        // New unfinished requests created after startup also exclude ordinary enablement and lifecycle side effects.
        // 启动后新建的未完成请求同样排除普通启用及生命周期副作用。
        if let Some(store) =
            crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
        {
            // A concrete owned original is the only exception; every other pending request still excludes transition.
            // 具体自有原请求是唯一例外；其他任何待完成请求仍排除转换。
            let pending = store
                .activation_recovery_records(plugin_id)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
            let allowed = match (update, pending.as_slice()) {
                (None, []) => true,
                (
                    Some((candidate, _)),
                    [crate::VulcanPluginActivationRequestRecord::Activation(original)],
                ) => original == candidate.transaction_record(),
                _ => false,
            };
            if !allowed {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin has an unfinished original activation request.",
                ));
            }
        }
        let mut snapshot = (*self.snapshot()).clone();
        let plugin = snapshot.plugins.get_mut(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if let Some((candidate, expected)) = update {
            super::update_drain::validate_predecessor(&self.state, candidate, expected, plugin)?;
            if runtime_state != VulcanPluginRuntimeState::Draining {
                return Err(VulcanPluginRuntimeError::new(
                    "Update authority only permits predecessor draining.",
                ));
            }
        }
        // Actual retained cleanup permits the existing explicit retry without changing the frozen routing source.
        // 实际保留清理允许既有明确重试，不改变冻结路由来源。
        let observed_source_state = observed_runtime_state(
            plugin.runtime_state,
            self.has_cleanup_diagnostics(plugin_id)?,
        );
        if !allowed_source_states.contains(&observed_source_state) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter state `{runtime_state:?}` from state `{:?}`.",
                observed_source_state
            )));
        }
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter a lifecycle transition while unavailable."
            )));
        }
        // Check requirements before changing generations, allocating guarantees, or publishing Enabling.
        // 在变更代次、分配保证或发布启用中状态前检查必需条件。
        if runtime_state == VulcanPluginRuntimeState::Enabling {
            self.validate_plugin_requirements(plugin)?;
        }
        // Re-establish all configured guarantees before callbacks or the Enabling catalog can observe activation.
        // 在回调或启用中目录可观测启用之前，重新建立全部配置保证。
        if runtime_state == VulcanPluginRuntimeState::Enabling
            && let Some(owner) = self.formal_runtime_owner()?
            && let Err(error) =
                owner.prepare_plugin_capacities_classified(plugin_id, &_registration)
        {
            // Preserve the originating preflight classification before returning the unchanged public error.
            // 返回未变公开错误前保留前置检查的来源分类。
            let pending = error.is_pending();
            // The original error retains its public message and rejection semantics.
            // 原错误保留公开消息及拒绝语义。
            let error = error.into_runtime_error();
            if owner.has_retiring_capacities(plugin_id)? {
                if pending {
                    self.record_cleanup_pending_locked(
                        plugin_id,
                        error.to_string(),
                        &_registration,
                    )?;
                } else {
                    self.record_cleanup_failure_locked(
                        plugin_id,
                        error.to_string(),
                        &_registration,
                    )?;
                }
            }
            return Err(error);
        }
        let (generation, update_authority) = {
            let mut generations = self.state.plugin_generations.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
            })?;
            let generation = generations.entry(plugin_id.to_owned()).or_insert(1);
            let successor = generation.checked_add(1).ok_or_else(|| {
                VulcanPluginRuntimeError::new("Vulcan Plugin generation is exhausted.")
            })?;
            // Persist update intent before issuing the new generation or granting any old finalizer.
            // 签发新代次或授权任何旧关闭函数之前，持久保存更新意图。
            let update_authority = match update {
                Some((candidate, _)) => Some(super::update_drain::UpdateDrainAuthority::begin(
                    &self.state,
                    candidate,
                    plugin,
                    successor,
                )?),
                None => None,
            };
            *generation = successor;
            (successor, update_authority)
        };
        // Grant only existing formal domains while publication is excluded; the grant needs the new snapshot to work.
        // 在排除发布竞争时仅授权既有正式域；授权必须匹配新快照才生效。
        if runtime_state == VulcanPluginRuntimeState::Draining
            && let Some(owner) = self.formal_runtime_owner()?
        {
            match update_authority.as_ref() {
                Some(authority) => {
                    owner
                        .host
                        .authorize_draining_with_update(plugin, generation, Some(authority))
                }
                None => owner.host.authorize_draining(plugin, generation),
            }
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        }
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
            })?
            .insert(plugin_id.to_owned(), runtime_state);
        plugin.runtime_state = runtime_state;
        plugin.generation = generation;
        for tool in &mut plugin.tools {
            tool.plugin_generation = generation;
        }
        for hook in &mut plugin.hooks {
            hook.plugin_generation = generation;
        }
        for server in &mut plugin.mcp_servers {
            server.plugin_generation = generation;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        rebuild_enabled_surface(&mut snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(update_authority)
    }

    /// Atomically replaces the catalog snapshot after a full package scan.
    /// 在完整扫描插件包后原子替换目录快照。
    ///
    /// # Parameters
    /// 参数
    /// The two sets contain explicit disabled and enabled plugin identifiers.
    /// 两个集合分别包含显式停用与显式启用的插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns the newly published immutable catalog snapshot.
    /// 返回新发布的不可变目录快照。
    pub fn refresh_catalog(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Scan and publish under the same gate used by lifecycle capacity ownership.
        // 在生命周期容量归属使用的同一门下扫描并发布。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&registration, None)?;
        // The declared policy selects this one scan; no alternate catalog is consulted.
        // 声明策略选择此次唯一扫描；不查询备用目录。
        let snapshot =
            self.scan_runtime_catalog(disabled_plugin_ids, enabled_plugin_ids, &registration);
        self.publish_runtime_catalog(snapshot, &registration, None)
    }

    /// Refresh the declared catalog through one retained original Legacy namespace owner.
    /// 通过单个保留原旧格式命名空间所有者刷新声明目录。
    /// Policy sets select the projection; plugin_id and mutation must identify this manager's same original guard.
    /// 策略集合选择投影；plugin_id 及 mutation 必须标识此管理器同一原屏障。
    /// Returns the actual published manager snapshot without reacquiring the guarded plugin namespace.
    /// 返回实际已发布管理器快照，不重新获取已屏障插件命名空间。
    pub fn refresh_catalog_with_legacy_mutation(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        plugin_id: &str,
        mutation: &crate::VulcanPluginLegacyMutationGuard,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Reuse the existing publication gate and projection, with only the exact guarded scan changed.
        // 复用既有发布门及投影，仅改变精确已屏障扫描。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog gate is poisoned.")
        })?;
        self.require_routing_namespace_retired_locked(&registration, mutation)?;
        // Validate literal root ownership before asking the catalog reader to borrow its original pin.
        // 要求目录读取器借用原固定之前，校验字面根所有权。
        mutation
            .validate_user_root(&self.state.user_plugin_root, plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // The revision is issued by the same sole publisher after runtime projection.
        // 修订在运行时投影之后由相同唯一发布器签发。
        let revision = self.state.next_revision.load(Ordering::Relaxed);
        // Guarded catalog loading remains the existing authoritative package-selection implementation.
        // 已屏障目录加载保持使用既有权威插件包选择实现。
        let snapshot = crate::catalog::load_vulcan_plugin_catalog_with_legacy_mutation(
            &self.state.vulcan_plugin_root,
            &self.state.user_plugin_root,
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
            plugin_id,
            mutation,
        )
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        self.publish_runtime_catalog(
            snapshot,
            &registration,
            Some(super::routing::ActivationRoutingOriginal::LegacyMutation(
                mutation,
            )),
        )
    }

    /// Refresh this manager's catalog through the same original removal File after exact tombstone publication.
    /// 在精确停用记录发布之后，通过同一原移除文件刷新此管理器目录。
    /// Policy sets select the projection; original must belong to the actual user root and remain retained.
    /// 策略集合选择投影；original 必须属于实际用户根并保持保留。
    /// Returns the manager snapshot; service Full publication, credentials, package GC and terminal receipts remain separate obligations.
    /// 返回管理器快照；服务完整发布、凭据、包回收及终态回执仍为独立义务。
    pub fn refresh_catalog_with_deactivation(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        original: &crate::VulcanPluginDeactivationReservation,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Reuse the same sole manager publisher while the original namespace remains held throughout actual readback.
        // 复用同一唯一管理器发布器，同时原命名空间跨实际回读全程保持持有。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog gate is poisoned.")
        })?;
        self.require_routing_publication_locked(
            &registration,
            Some(super::routing::ActivationRoutingOriginal::Deactivation(
                original,
            )),
        )?;
        original
            .require_no_lifecycle_borrowers()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Candidate revision is issued only by the common publisher after successful runtime projection.
        // 候选修订仅在运行时投影成功后由公共发布器签发。
        let revision = self.state.next_revision.load(Ordering::Relaxed);
        // The explicit original source accepts only its exact admitted tombstone; there is no ordinary scan fallback.
        // 明确原来源仅接受精确已准入停用记录；不存在普通扫描备用路径。
        let snapshot = crate::catalog::load_vulcan_plugin_catalog_with_deactivation(
            &self.state.vulcan_plugin_root,
            &self.state.user_plugin_root,
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
            original,
        )
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        self.publish_runtime_catalog(
            snapshot,
            &registration,
            Some(super::routing::ActivationRoutingOriginal::Deactivation(
                original,
            )),
        )
    }

    /// Scan the declared policy sets while publication is excluded; return the unprojected catalog.
    /// 排除发布竞争时扫描声明策略集合；返回尚未投影的目录。
    fn scan_runtime_catalog(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        _registration: &std::sync::MutexGuard<'_, ()>,
    ) -> VulcanPluginCatalogSnapshot {
        // This candidate routing revision is not issued until successful publication.
        // 此候选路由修订仅在成功发布时签发。
        let revision = self.state.next_revision.load(Ordering::Relaxed);
        load_vulcan_plugin_catalog(
            &self.state.vulcan_plugin_root,
            Some(&self.state.user_plugin_root),
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
        )
    }

    /// Apply live ownership to snapshot and publish it under the caller-held gate; return the immutable projection.
    /// 在调用方持有的门下向 snapshot 应用实时归属并发布；返回不可变投影。
    /// `original` is absent for ordinary publication or borrows the actual validated original owner.
    /// 普通发布的 `original` 缺省，否则借用实际已校验原所有者。
    fn publish_runtime_catalog(
        &self,
        mut snapshot: VulcanPluginCatalogSnapshot,
        _registration: &std::sync::MutexGuard<'_, ()>,
        original: Option<super::routing::ActivationRoutingOriginal<'_>>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Only an actual transaction can supply the recovery projection's full original record.
        // 仅实际事务能够提供恢复投影的完整原记录。
        let recovery_permit = match &original {
            Some(super::routing::ActivationRoutingOriginal::Transaction(transaction)) => {
                Some((*transaction).record())
            }
            _ => None,
        };
        self.require_routing_publication_locked(_registration, original)?;
        self.project_runtime_state(&mut snapshot, recovery_permit)?;
        // Issue the routing revision only after the authoritative routing projection succeeds.
        // 仅在权威路由投影成功后签发路由修订。
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        // Readers retain this exact published value independently of future changes.
        // 读取方独立于未来变化保留此精确发布值。
        let snapshot = Arc::new(snapshot);
        // Publication remains a single replacement of the authoritative catalog pointer.
        // 发布继续作为权威目录指针的单次替换。
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::clone(&snapshot);
        Ok(snapshot)
    }

    /// Marks one valid plugin as enabling and immediately removes any stale provider surface.
    /// 将一个有效插件标记为正在启用，并立即移除任何陈旧 Provider 工具面。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after publishing the transient lifecycle state.
    /// 在发布临时生命周期状态后返回成功。
    pub fn begin_enabling(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[
                VulcanPluginRuntimeState::Disabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
    }

    /// Marks one enabled plugin as draining and immediately rejects every new invocation.
    /// 将一个已启用插件标记为正在排空，并立即拒绝所有新调用。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after tools and hooks leave the enabled surface.
    /// 在工具与 Hook 离开启用面后返回成功。
    pub fn begin_draining(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Draining,
            &[
                VulcanPluginRuntimeState::Enabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
    }

    /// Re-enters enabling from an uncommitted draining transition during service rollback.
    /// 在服务回滚期间从尚未提交的排空转换重新进入启用中状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin whose disable transaction must be compensated.
    /// `plugin_id` 是必须补偿其停用事务的精确插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after atomically replacing `Draining` with `Enabling`.
    /// 仅在原子地以 `Enabling` 替换 `Draining` 后返回成功。
    pub fn rollback_draining_to_enabling(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[VulcanPluginRuntimeState::Draining],
        )
    }

    /// Clears a transient lifecycle transition and rebuilds the catalog from persisted policy.
    /// 清除临时生命周期转换，并根据持久化策略重建目录。
    ///
    /// # Parameters
    /// 参数
    /// The plugin id selects the transition, policy sets are persisted overrides, and failures are unresolved cleanup diagnostics.
    /// 插件标识选择转换，策略集合是持久化覆盖，失败集合是未解决清理诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns the final immutable catalog snapshot.
    /// 返回最终不可变目录快照。
    pub fn finish_lifecycle_transition(
        &self,
        plugin_id: &str,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        mut cleanup_failures: Vec<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Capacity retirement and final catalog publication must exclude new member preparation together.
        // 容量回收及最终目录发布必须共同排除新成员准备。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&registration, None)?;
        // Ordinary completion cannot remove an update-owned admission barrier.
        // 普通完成不得移除更新所有的入场屏障。
        if let Some(store) =
            crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            && !store
                .activation_recovery_records(plugin_id)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
                .is_empty()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin has an unfinished original activation request.",
            ));
        }
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
            })?
            .remove(plugin_id);
        // Resolve actual persisted policy before deciding whether this transition ends the capacity lifetime.
        // 决定此次转换是否结束容量寿命前，解析实际持久策略。
        let snapshot =
            self.scan_runtime_catalog(disabled_plugin_ids, enabled_plugin_ids, &registration);
        if snapshot
            .plugins
            .get(plugin_id)
            .is_some_and(|plugin| !plugin.desired_enabled)
            && let Some(owner) = self.formal_runtime_owner()?
            && let Err(error) = owner.retire_plugin_capacities_classified(plugin_id, &registration)
        {
            // Finite physical owners remain Pending; no text or current-map lookup determines their source.
            // 有限物理所有者保持 Pending；来源不由文本或当前映射查询决定。
            let pending = error.is_pending();
            // Keep the original projected capacity error wording for both classifications.
            // 为两种分类保留原容量错误投影文案。
            let diagnostic = format!(
                "Plugin capacity retirement failed: {}",
                error.into_runtime_error()
            );
            if pending {
                self.record_cleanup_pending_locked(plugin_id, diagnostic, &registration)?;
            } else {
                cleanup_failures.push(diagnostic);
            }
        }
        // Only additional failures enter this unchanged public argument; recorded drain evidence stays typed.
        // 此未变公开参数仅接收额外失败；已登记排空证据保持类型。
        for diagnostic in cleanup_failures {
            self.record_cleanup_failure_locked(plugin_id, diagnostic, &registration)?;
        }
        self.clear_cleanup_diagnostics_locked(plugin_id, &registration)?;
        self.publish_runtime_catalog(snapshot, &registration, None)
    }

    /// Probes dependency resolution and Lua module loading without opening a business session.
    /// 在不打开业务会话的前提下探测依赖解析与 Lua 模块加载。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin to probe.
    /// `plugin_id` 是需要探测的精确插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after the original probe initialization and actual core retirement complete.
    /// 仅在原探测初始化及实际核心退役完成后返回成功。
    pub async fn probe_plugin(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        // Probe preparation and initialization share the existing finite probe budget from API entry.
        // 探测准备及初始化从接口入场起共用既有有限探测预算。
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(
                VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
            ))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin probe deadline is out of range.")
            })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` is unavailable: {}",
                plugin.diagnostics.join("; ")
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        if !manifest.has_lua_runtime() {
            return Ok(());
        }
        self.invoke_formal_probe(plugin.clone(), deadline).await
    }

    /// Returns one immutable current catalog snapshot.
    /// 返回一份不可变的当前目录快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns a reference-counted snapshot that remains stable for its consumer.
    /// 返回供消费者稳定持有的引用计数快照。
    pub fn snapshot(&self) -> Arc<VulcanPluginCatalogSnapshot> {
        self.state
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_else(|poisoned| Arc::clone(&poisoned.into_inner()))
    }

    /// Project live counts and cleanup onto the explicitly supplied routing catalog without publishing it.
    /// 将实时计数及清理投影到明确传入的路由目录，不发布该目录。
    /// `catalog` is the actual current or service-published routing value; returns a display-only owned value or a lock error.
    /// `catalog` 为实际当前或服务已发布路由值；返回仅用于展示的自有值或锁错误。
    /// The routing revision is preserved; this value must never replace a Full, Source, or prepared routing Arc.
    /// 保留路由修订；此值绝不能替换 Full、Source 或已准备路由 Arc。
    pub fn observe_catalog(
        &self,
        catalog: &VulcanPluginCatalogSnapshot,
    ) -> Result<VulcanPluginCatalogSnapshot, VulcanPluginRuntimeError> {
        // One gate makes both actual maps coherent while leaving immutable routing ownership untouched.
        // 同一登记门使两个实际映射保持一致，同时不触碰不可变路由归属。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        // Read the sole counters; no projection cache becomes another accounting authority.
        // 读取唯一计数器；不让投影缓存成为另一记账权威。
        let counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        // Retain cleanup evidence from its unique live map, including failures after catalog publication.
        // 从唯一实时映射保留清理证据，包含目录发布后的失败。
        let diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        // Failure history is plain data, so a poisoned ledger still projects consistently.
        // 失败历史是普通数据，因此中毒的账本仍可一致投影。
        let runtime_errors = self
            .state
            .runtime_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // This plain value has no publication identity and retains the supplied routing revision.
        // 此普通值没有发布身份，并保留传入路由修订。
        let mut observed = catalog.clone();
        for plugin in observed.plugins.values_mut() {
            plugin.active_session_count = counts.get(&plugin.id).copied().unwrap_or_default();
            plugin.runtime_errors = runtime_errors.entries(&plugin.id);
            if let Some(entries) = diagnostics.get(&plugin.id) {
                plugin.runtime_state =
                    observed_runtime_state(plugin.runtime_state, !entries.is_empty());
                for diagnostic in entries {
                    if !plugin.diagnostics.contains(&diagnostic.message) {
                        plugin.diagnostics.push(diagnostic.message.clone());
                    }
                }
            }
        }
        Ok(observed)
    }

    /// Invokes one optional manifest-declared lifecycle callback through the owned formal runtime.
    /// 通过拥有明确归属的正式运行时调用一个可选的清单声明生命周期回调。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact package and `event` selects its declared callback.
    /// `plugin_id` 选择精确插件包，`event` 选择其声明的回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when no callback is declared or after the callback and actual core retirement succeed.
    /// 未声明回调时直接成功；否则在回调及实际核心退役均成功后返回成功。
    pub async fn invoke_lifecycle(
        &self,
        plugin_id: &str,
        event: VulcanPluginLifecycleEvent,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.invoke_lifecycle_classified(plugin_id, event)
            .await
            .map_err(crate::runtime::drain::CleanupOriginError::into_runtime_error)
    }

    /// Execute plugin_id's event with its exact internal cleanup origin; return unchanged runtime evidence.
    /// 使用精确内部清理来源执行 plugin_id 的 event；返回未变运行时证据。
    /// Unknown preparation and actual callback errors remain Failure; only the proven capacity owners may be Pending.
    /// 未知准备及实际回调错误仍为 Failure；只有已证容量所有者可为 Pending。
    async fn invoke_lifecycle_classified(
        &self,
        plugin_id: &str,
        event: VulcanPluginLifecycleEvent,
    ) -> Result<(), crate::runtime::drain::CleanupOriginError> {
        // The original deadline includes environment preparation, core initialization and business work.
        // 原截止时间涵盖环境准备、核心初始化及业务工作。
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(
                VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
            ))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin lifecycle deadline is out of range.")
            })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let Some(lua_runtime) = manifest.lua_runtime.as_ref() else {
            return Ok(());
        };
        let (function, event_key) = match event {
            VulcanPluginLifecycleEvent::Install => {
                (lua_runtime.lifecycle.install.clone(), "install")
            }
            VulcanPluginLifecycleEvent::Enable => (lua_runtime.lifecycle.enable.clone(), "enable"),
            VulcanPluginLifecycleEvent::EnableRollback => {
                (lua_runtime.lifecycle.disable.clone(), "enable_rollback")
            }
            VulcanPluginLifecycleEvent::Disable => {
                (lua_runtime.lifecycle.disable.clone(), "disable")
            }
            VulcanPluginLifecycleEvent::Uninstall => {
                (lua_runtime.lifecycle.uninstall.clone(), "uninstall")
            }
        };
        let Some(function) = function else {
            return Ok(());
        };
        // The registered blocking worker prepares the original writable environment before Lua admission.
        // 已登记阻塞工作者在 Lua 入场前准备原可写环境。
        self.invoke_formal_lifecycle(
            plugin.clone(),
            event,
            function,
            json!({
                "event": event_key,
                "plugin_id": plugin_id,
            }),
            deadline,
        )
        .await
        .map(|_| ())
    }

    /// Execute and record one cleanup callback without losing its actual diagnostic origin.
    /// 执行并登记单个清理回调，不丢失实际诊断来源。
    /// plugin_id and event select the original callback; diagnostic_context supplies its existing host message prefix.
    /// plugin_id 与 event 选择原回调；diagnostic_context 提供既有宿主消息前缀。
    /// None means success or no declaration; Some is already recorded; Err reports accounting failure only.
    /// None 表示成功或无声明；Some 已登记；Err 仅报告记账失败。
    pub async fn invoke_lifecycle_for_cleanup(
        &self,
        plugin_id: &str,
        event: VulcanPluginLifecycleEvent,
        diagnostic_context: &str,
    ) -> Result<Option<String>, VulcanPluginRuntimeError> {
        match self.invoke_lifecycle_classified(plugin_id, event).await {
            Ok(()) => Ok(None),
            Err(error) => {
                // Preserve the actual producer classification before projecting its public message.
                // 投影公开消息前保留实际生产者分类。
                let pending = error.is_pending();
                // Use the unchanged host prefix and original error, never a current-map or text classification.
                // 使用未变宿主前缀及原错误，绝不通过当前映射或文本分类。
                let diagnostic = format!("{diagnostic_context}：{}", error.into_runtime_error());
                if pending {
                    self.record_cleanup_pending(plugin_id, diagnostic.clone())?;
                } else {
                    self.record_cleanup_failure(plugin_id, diagnostic.clone())?;
                }
                Ok(Some(diagnostic))
            }
        }
    }

    /// Requires one tool definition to match the current enabled plugin generation.
    /// 要求一个工具定义匹配当前已启用插件代次。
    /// `tool` is the frozen request descriptor; returns unit or an explicit stale-generation error.
    /// `tool` 是冻结的请求描述；返回空值或明确的陈旧代次错误。
    pub(super) fn ensure_current_tool_generation(
        &self,
        tool: &VulcanPluginToolDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Published identity alone cannot admit business before the original target handoff completes.
        // 仅发布身份不能在原目标交接完成前接纳业务。
        crate::runtime::update_drain::validate_ordinary_lifecycle_admission(
            &self.state,
            &tool.plugin_id,
            crate::VulcanPluginBridgeLeaseScope::Enabled,
        )?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                tool.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != tool.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry with a new provider request.",
                tool.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one hook declaration to match the current enabled plugin generation.
    /// 要求一个 hook 声明匹配当前已启用插件代次。
    /// `hook` identifies the registered callback; returns unit only while both catalog generations match.
    /// `hook` 标识注册回调；仅在两处目录代次匹配时返回空值。
    pub(super) fn ensure_current_hook_generation(
        &self,
        hook: &VulcanPluginHookDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Published identity alone cannot admit business before the original target handoff completes.
        // 仅发布身份不能在原目标交接完成前接纳业务。
        crate::runtime::update_drain::validate_ordinary_lifecycle_admission(
            &self.state,
            &hook.plugin_id,
            crate::VulcanPluginBridgeLeaseScope::Enabled,
        )?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                hook.plugin_id
            ))
        })?;
        let current_hook = snapshot
            .enabled_hooks
            .get(&hook.registration_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin hook `{}` is not enabled in the current catalog.",
                    hook.registration_id
                ))
            })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != hook.plugin_generation
            || current_hook.plugin_generation != hook.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry the hook on the current runtime.",
                hook.plugin_id
            )));
        }
        Ok(())
    }

    /// Publish the exact disabled target owned by `transaction` using the supplied explicit policy sets.
    /// 按提供的明确策略集合发布 `transaction` 持有的精确停用目标。
    /// Return its catalog only after validating original ownership; retain recovery obligations until route completion.
    /// 仅在校验原所有权后返回目录；路由完成前保留恢复义务。
    pub fn refresh_catalog_for_activation_recovery(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // One publication gate excludes ordinary lifecycle admission throughout validation and projection.
        // 单个发布门在校验及投影全程排除普通生命周期入场。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(
            &registration,
            Some(super::routing::ActivationRoutingOriginal::Transaction(
                transaction,
            )),
        )?;
        // Borrow the original target; neither a plugin ID nor a UUID alone grants publication.
        // 借用原始目标；仅插件标识或 UUID 均不授予发布权。
        let target = transaction.record().target();
        if !disabled_plugin_ids.contains(&target.plugin_id)
            || enabled_plugin_ids.contains(&target.plugin_id)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Recovery publication requires explicit disabled policy.",
            ));
        }
        // Read the existing store, preserving absence as a failure after an observed original request.
        // 读取既有存储，已观察原请求后将缺失保留为失败。
        let store = crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original activation store is missing.")
            })?;
        store
            .validate_published_activation_owner(transaction)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // A disabled descriptor pins the complete target authority without executing plugin callbacks.
        // 停用描述符固定完整目标权威，不执行插件回调。
        let snapshot =
            self.scan_runtime_catalog(disabled_plugin_ids, enabled_plugin_ids, &registration);
        if !snapshot
            .plugins
            .get(&target.plugin_id)
            .is_some_and(|plugin| {
                !plugin.desired_enabled
                    && plugin.availability == crate::VulcanPluginAvailability::Disabled
                    && plugin
                        .managed_package()
                        .is_some_and(|package| package.matches_activation(target))
            })
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original activation target is not a valid disabled catalog entry.",
            ));
        }
        self.publish_runtime_catalog(
            snapshot,
            &registration,
            Some(super::routing::ActivationRoutingOriginal::Transaction(
                transaction,
            )),
        )
    }

    /// Apply generations and transient lifecycle overrides without live observation overlays.
    /// 应用代次及临时生命周期覆盖，不混入实时观测。
    /// Mutates the scanned `snapshot` using this manager's state; returns unit or a lock diagnostic.
    /// 使用本管理器状态修改扫描所得的 `snapshot`；返回空值或锁诊断。
    /// `recovery_permit` exempts only its exact original from startup gating for this disabled publication.
    /// `recovery_permit` 仅使精确原请求在此次停用发布中豁免启动门控。
    fn project_runtime_state(
        &self,
        snapshot: &mut VulcanPluginCatalogSnapshot,
        recovery_permit: Option<&crate::VulcanPluginActivationTransactionRecord>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Resolve durable startup obligations before taking unrelated runtime state locks.
        // 获取无关运行时状态锁之前解析持久启动义务。
        let (recovery_error, recovery_diagnostics) =
            self.activation_recovery_diagnostics(recovery_permit)?;
        let previous = self.snapshot();
        snapshot.service_epoch = self.state.manager_id.clone();
        let overrides = self.state.lifecycle_overrides.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
        })?;
        let mut generations = self.state.plugin_generations.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
        })?;
        let runtime_engine_available = self
            .state
            .engine
            .read()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin runtime engine lock is poisoned.")
            })?
            .is_some();
        for plugin in snapshot.plugins.values_mut() {
            let requires_runtime = plugin
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.has_lua_runtime());
            plugin.runtime_execution_available = !requires_runtime || runtime_engine_available;
            if requires_runtime && !runtime_engine_available {
                plugin.diagnostics.push(String::from(
                    "The plugin package is valid, but its Lua execution runtime is unavailable; read-only skills and agents remain active.",
                ));
            }
            // Remember prior incarnations even when a tombstone removed the preceding catalog row.
            // 即使停用记录移除了前一目录条目，也必须记住先前实例。
            let previously_seen = generations.contains_key(&plugin.id);
            // Routing identity remains process-local and must never be reused after disappearance.
            // 路由身份保持进程内归属，插件消失后也绝不能复用。
            let generation = generations.entry(plugin.id.clone()).or_insert(1);
            // Managed activation identity is independent from semantic version and content authorization.
            // 受管激活身份独立于语义版本及内容授权。
            let changed = previous
                .plugins
                .get(&plugin.id)
                .map_or(previously_seen, |prior| {
                    prior.version != plugin.version
                        || prior.content_fingerprint != plugin.content_fingerprint
                        || prior.desired_enabled != plugin.desired_enabled
                        || prior.availability != plugin.availability
                        || prior.managed_package().map(|managed| {
                            (
                                managed.activation_revision(),
                                managed.generation().identity(),
                            )
                        }) != plugin.managed_package().map(|managed| {
                            (
                                managed.activation_revision(),
                                managed.generation().identity(),
                            )
                        })
                });
            if changed {
                *generation = generation.checked_add(1).ok_or_else(|| {
                    VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin `{}` routing generation is exhausted.",
                        plugin.id
                    ))
                })?;
            }
            plugin.generation = *generation;
            for tool in &mut plugin.tools {
                tool.plugin_generation = *generation;
            }
            for hook in &mut plugin.hooks {
                hook.plugin_generation = *generation;
            }
            for server in &mut plugin.mcp_servers {
                server.plugin_generation = *generation;
            }
            if let Some(runtime_state) = overrides.get(&plugin.id).copied() {
                plugin.runtime_state = runtime_state;
            }
            if plugin.origin == VulcanPluginPackageOrigin::UserInstalled {
                match plugin_is_authorized(&self.state.plugin_state_root, plugin) {
                    Ok(true) => {}
                    Ok(false) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(String::from(
                            "User-installed plugin requires authorization for its current version, content fingerprint, and complete capability set.",
                        ));
                    }
                    Err(error) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(error);
                    }
                }
            }
            // Resource requirements gate the complete plugin surface before any activation is published.
            // 在任何激活发布前，资源必需条件门控插件的完整暴露面。
            if let Err(error) = self.validate_plugin_requirements(plugin) {
                plugin.runtime_execution_available = false;
                plugin.diagnostics.push(error.to_string());
            }
            // Recovery gates every surface while preserving the user's declared enabled policy.
            // 恢复门控全部暴露面，同时保留用户声明的启用策略。
            if let Some(diagnostic) = recovery_error
                .as_ref()
                .or_else(|| recovery_diagnostics.get(&plugin.id))
            {
                plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                plugin.runtime_execution_available = false;
                plugin.diagnostics.push(diagnostic.clone());
            }
        }
        rebuild_enabled_surface(snapshot);
        Ok(())
    }

    /// Begins one transient lifecycle state and publishes a generation-bumped snapshot.
    /// 开始一个临时生命周期状态，并发布代次递增的快照。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package, `runtime_state` is the target, and source states are exact.
    /// `plugin_id` 选择插件包，`runtime_state` 是目标状态，源状态集合必须精确匹配。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after atomically validating and publishing the transition.
    /// 原子校验并发布转换后返回成功。
    fn begin_lifecycle_transition(
        &self,
        plugin_id: &str,
        runtime_state: VulcanPluginRuntimeState,
        allowed_source_states: &[VulcanPluginRuntimeState],
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition_with_update(
            plugin_id,
            runtime_state,
            allowed_source_states,
            None,
        )
        .map(|_| ())
    }
}

/// Observe cleanup warning state independently from the immutable routing descriptor.
/// 独立于不可变路由描述符观测清理告警状态。
/// `routing_state` is the actual lifecycle state and `has_cleanup` comes from its sole live map.
/// `routing_state` 为实际生命周期状态，`has_cleanup` 来自其唯一实时映射。
/// Returns a warning only for Disabled with real retained cleanup; all other lifecycle states remain exact.
/// 仅对存在真实保留清理的停用状态返回告警；其他全部生命周期状态保持精确。
fn observed_runtime_state(
    routing_state: VulcanPluginRuntimeState,
    has_cleanup: bool,
) -> VulcanPluginRuntimeState {
    if routing_state == VulcanPluginRuntimeState::Disabled && has_cleanup {
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    } else {
        routing_state
    }
}

/// Rebuilds provider-visible tools and prompt contributions from enabled plugin descriptors.
/// 根据已启用插件描述重新构建 Provider 可见工具与提示词贡献。
/// Replaces the derived surfaces of `snapshot` in place and returns no value.
/// 原位替换 `snapshot` 的派生暴露面，不返回值。
fn rebuild_enabled_surface(snapshot: &mut VulcanPluginCatalogSnapshot) {
    snapshot.enabled_tools.clear();
    snapshot.enabled_hooks.clear();
    snapshot.enabled_mcp_servers.clear();
    snapshot.prompt_contributions.clear();
    snapshot.enabled_skills.clear();
    snapshot.enabled_agents.clear();
    for plugin in snapshot.plugins.values() {
        if plugin.availability != VulcanPluginAvailability::Enabled
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || (!plugin.runtime_execution_available
                && plugin
                    .manifest
                    .as_ref()
                    .is_some_and(|manifest| manifest.runtime_requirements.is_some()))
        {
            continue;
        }
        for skill in &plugin.skills {
            snapshot
                .enabled_skills
                .insert(skill.name.clone(), skill.clone());
        }
        for agent in &plugin.agents {
            snapshot
                .enabled_agents
                .insert(agent.id.clone(), agent.clone());
        }
        for server in &plugin.mcp_servers {
            snapshot.enabled_mcp_servers.insert(
                format!("{}:{}", server.plugin_id, server.id),
                server.clone(),
            );
        }
        if !plugin.runtime_execution_available {
            continue;
        }
        for tool in &plugin.tools {
            snapshot.enabled_tools.insert(tool.id.clone(), tool.clone());
        }
        for hook in &plugin.hooks {
            snapshot
                .enabled_hooks
                .insert(hook.registration_id.clone(), hook.clone());
        }
        snapshot
            .prompt_contributions
            .extend(plugin.prompt_contributions.iter().cloned());
    }
}

#[cfg(test)]
mod tests;

//! Managed removal retains its local environment cleanup without adding a namespace borrower or an immutable package pin.
//! 受管移除保留其本地环境清理，不添加命名空间借用者或不可变包固定。

use super::*;
use crate::{
    VulcanPluginActivation, VulcanPluginDeactivationObservation,
    VulcanPluginDeactivationReservation,
};

#[cfg(test)]
mod tests;

/// Local-state observations are separate from selection publication, package/credential collection and the host Full receipt.
/// 本地状态观测独立于选择发布、包／凭据回收及宿主完整回执。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginDeactivationStateObservation {
    /// The local owner is registered before callbacks and no cleanup was admitted.
    /// 本地所有者在回调之前登记，未准入清理。
    Reserved,
    /// Callbacks have retired and cleanup is admitted for selection publication; no environment deletion has started.
    /// 回调已退役，清理已为选择发布准入；环境删除尚未启动。
    Frozen,
    /// Local irreversible cleanup was admitted and the same original may retain partial physical deletion.
    /// 本地不可逆清理已准入，相同原实例可能保留部分物理删除。
    CleanupPending,
    /// The original environment child is actually absent under retained ownership.
    /// 保留所有权内，原环境子级实际缺失。
    Collected,
}

/// A non-cloneable local original must be strongly registered beside its actual selection original before callbacks or policy effects.
/// 不可克隆本地原实例必须在回调或策略效果之前，与实际选择原实例共同强登记。
/// Every effect also borrows that same selection original; UUIDs, records and paths cannot reconstruct its physical File authority.
/// 每个效果还借用相同选择原实例；UUID、记录及路径不能重建其物理文件权威。
pub struct VulcanPluginDeactivationStateReservation {
    /// Actual manager allocation fixes all local state and environment roots.
    /// 实际管理器分配固定全部本地状态及环境根。
    manager: Weak<super::super::super::VulcanPluginManagerState>,
    /// Weak allocation identity binds the same File without keeping an extra callback borrower alive.
    /// 弱分配身份绑定相同文件，不额外保持回调借用者存活。
    namespace: Weak<crate::VulcanPluginLegacyMutationGuard>,
    /// Original request identity is metadata checked together with actual allocation identity.
    /// 原请求身份为与实际分配身份共同检查的元数据。
    request_id: Uuid,
    /// The complete source selection includes approval, credentials and configuration, without retaining a package pin.
    /// 完整来源选择包含批准、凭据及配置，不保留包固定。
    selection: VulcanPluginActivation,
    /// Actual original routing generation excludes a refreshed or replacement callback source before freezing.
    /// 实际原路由代次在固定之前排除刷新或替代回调来源。
    generation: u64,
    /// Original immutable package path is evidence for exact current-source comparison, never independent deletion authority.
    /// 原不可变包路径为精确当前来源比较证据，绝非独立删除权威。
    package_root: PathBuf,
    /// Freeze admission is retained once every callback borrower has retired.
    /// 每个回调借用者退役之后保留固定准入。
    cleanup_frozen: bool,
    /// Irreversible cleanup intent precedes the first environment deletion.
    /// 不可逆清理意图先于首次环境删除。
    cleanup_started: bool,
    /// The same actual exclusive environment File and canonical root survive errors and panic.
    /// 相同实际排他环境文件及规范根跨错误及 panic 存续。
    environment_cleanup: Option<(File, PathBuf)>,
    /// Local terminal confirmation is stored adjacent to true absence and never implies whole uninstallation success.
    /// 本地终态确认与真实缺失相邻存储，绝不暗示整个卸载成功。
    collected: bool,
}

impl std::fmt::Debug for VulcanPluginDeactivationStateReservation {
    /// Format public identities only; formatter returns no state bytes, paths or resource handles.
    /// 仅格式化公共身份；formatter 不返回状态字节、路径或资源句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginDeactivationStateReservation")
            .field("request_id", &self.request_id)
            .field("plugin_id", &self.selection.plugin_id)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginDeactivationStateReservation {
    /// Borrow original request metadata; no parameters and no mutation or completion authority is returned.
    /// 借用原请求元数据；无参数，不返回变更或完成权威。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Borrow the sole source identity; no parameters and no catalog lookup or replacement owner is created.
    /// 借用唯一来源身份；无参数，不执行目录查找或创建替代所有者。
    pub fn plugin_id(&self) -> &str {
        &self.selection.plugin_id
    }

    /// Validate the actual manager and same original allocation before reading its actual existing store selection.
    /// 在读取实际既有存储选择之前，校验实际管理器及相同原分配。
    /// manager/original must be the reserving instances; returns the existing store without reacquiring a namespace or creating directories.
    /// manager／original 必须为预留实例；返回既有存储，不重新取得命名空间或创建目录。
    fn validate_binding(
        &self,
        manager: &VulcanPluginRuntimeManager,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<crate::VulcanPluginGenerationStore, VulcanPluginRuntimeError> {
        if !Weak::ptr_eq(&self.manager, &Arc::downgrade(&manager.state))
            || !original.matches_namespace_identity(&self.namespace)
            || original.request_id() != self.request_id
            || original.original() != &self.selection
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original removal state belongs to another actual manager or selection owner.",
            ));
        }
        if self.environment_cleanup.is_some() {
            validate_environment_cleanup(manager, &self.environment_cleanup)?;
        }
        // Existing canonical layout is checked without allocating any replacement physical ownership.
        // 检查既有规范布局，不分配任何替代物理所有权。
        let store =
            crate::VulcanPluginGenerationStore::open_existing(&manager.state.user_plugin_root)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Original removal store is absent.")
                })?;
        store
            .reconcile_deactivation_retained(original)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        Ok(store)
    }

    /// Require the exact actual disabled zero-session managed source before the post-callback freeze.
    /// 回调后固定之前要求精确实际停用零会话受管来源。
    /// manager is protected by its actual registration gate; returns unit or a conflict without adopting newer routing.
    /// manager 由实际登记门保护；返回空值或冲突，不接纳较新路由。
    fn validate_current_disabled(
        &self,
        manager: &VulcanPluginRuntimeManager,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Strict data-bearing snapshot poison remains an explicit error.
        // 严格携带数据快照污染保持为明确错误。
        let snapshot = current_uninstallation_snapshot(manager)?;
        // The actual descriptor must retain the complete selected authority and exact process-local source generation.
        // 实际描述符必须保留完整已选权威及精确进程内来源代次。
        let current = snapshot.plugins.get(self.plugin_id()).ok_or_else(|| {
            VulcanPluginRuntimeError::new(
                "Original managed source is no longer selected for state freezing.",
            )
        })?;
        if current.origin != VulcanPluginPackageOrigin::UserInstalled
            || current.generation != self.generation
            || current.package_root != self.package_root
            || current.runtime_state != VulcanPluginRuntimeState::Disabled
            || manager.active_session_count(self.plugin_id())? != 0
            || manager.has_cleanup_diagnostics(self.plugin_id())?
            || !current
                .managed_package()
                .is_some_and(|package| package.matches_activation(&self.selection))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original managed source is no longer the same disabled zero-session selection.",
            ));
        }
        Ok(())
    }

    /// Observe only explicit local phases under the same selection original; returns no Full, package or credential receipt.
    /// 仅在相同选择原实例内观测明确本地阶段；不返回完整、包或凭据回执。
    /// manager/original are actual retained originals and store is their verified existing layout.
    /// manager／original 为实际保留原实例，store 为其已验证既有布局。
    fn observe(
        &self,
        manager: &VulcanPluginRuntimeManager,
        original: &VulcanPluginDeactivationReservation,
        store: &crate::VulcanPluginGenerationStore,
    ) -> Result<VulcanPluginDeactivationStateObservation, VulcanPluginRuntimeError> {
        // Only the exact original or admitted successor can be observed; partial local cleanup requires the latter.
        // 仅能观测精确原记录或已准入后继；部分本地清理要求后者。
        let selection = store
            .reconcile_deactivation_retained(original)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if self.cleanup_started {
            if selection != VulcanPluginDeactivationObservation::Published {
                return Err(VulcanPluginRuntimeError::new(
                    "Original local cleanup requires its exact published removal selection.",
                ));
            }
            validate_environment_cleanup(manager, &self.environment_cleanup)?;
            if self.collected {
                confirm_environment_absence(self.plugin_id(), &self.environment_cleanup)?;
                return Ok(VulcanPluginDeactivationStateObservation::Collected);
            }
            return Ok(VulcanPluginDeactivationStateObservation::CleanupPending);
        }
        if self.cleanup_frozen {
            Ok(VulcanPluginDeactivationStateObservation::Frozen)
        } else {
            if selection != VulcanPluginDeactivationObservation::Original {
                return Err(VulcanPluginRuntimeError::new(
                    "Original local state was not frozen before selection publication.",
                ));
            }
            Ok(VulcanPluginDeactivationStateObservation::Reserved)
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Register the managed local cleanup owner before callbacks while borrowing the same selection original without adding a strong owner.
    /// 回调之前登记受管本地清理所有者，借用相同选择原实例，不添加强所有者。
    /// original must remain strongly retained beside the returned non-cloneable state owner; no policy, publication or cleanup occurs.
    /// original 必须与返回不可克隆状态所有者共同强保留；不发生策略、发布或清理。
    pub fn reserve_deactivation_state(
        &self,
        original: &mut VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationStateReservation, VulcanPluginRuntimeError> {
        original
            .require_no_lifecycle_borrowers()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Initial admission never clears a poisoned gate on behalf of a new local owner.
        // 初始准入绝不代表新本地所有者清除污染门。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(
            &_registration,
            Some(crate::runtime::routing::ActivationRoutingOriginal::Deactivation(original)),
        )?;
        // The actual current descriptor is inspected without retaining its managed package/credential pins in the new owner.
        // 检查实际当前描述符，不在新所有者内保留其受管包／凭据固定。
        let snapshot = current_uninstallation_snapshot(self)?;
        // Absence is a conflict rather than a default empty source.
        // 缺失为冲突，而非默认空来源。
        let current = snapshot
            .plugins
            .get(&original.original().plugin_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original managed removal source is absent.")
            })?;
        // The inert owner is complete before it can be handed to a host registry.
        // 未启动所有者在能够交给宿主注册表之前已完整。
        let state = VulcanPluginDeactivationStateReservation {
            manager: Arc::downgrade(&self.state),
            namespace: original.namespace_identity(),
            request_id: original.request_id(),
            selection: original.original().clone(),
            generation: current.generation,
            package_root: current.package_root.clone(),
            cleanup_frozen: false,
            cleanup_started: false,
            environment_cleanup: None,
            collected: false,
        };
        // Validate actual store/root/allocation and current selection before returning any cleanup admission.
        // 返回任何清理准入之前，校验实际存储／根／分配及当前选择。
        let store = state.validate_binding(self, original)?;
        if store
            .reconcile_deactivation_retained(original)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            != VulcanPluginDeactivationObservation::Original
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original managed state reservation must precede selection publication.",
            ));
        }
        state.validate_current_disabled(self)?;
        plain_directory_presence(&self.state.plugin_runtime_root)?;
        // Register the sole local owner only after all fallible inert capture and validation, without replacing prior admission.
        // 仅在全部可失败未启动捕获及校验之后登记唯一本地所有者，不替代先前准入。
        original
            .reserve_local_state_owner()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        Ok(state)
    }

    /// Admit cleanup once before selection publication, after every callback borrower has retired.
    /// 每个回调借用者退役之后，于选择发布之前一次性准入清理。
    /// state/original must be the same retained pair and actual callback borrowers must retire; returns a local Frozen observation.
    /// state／original 必须为相同保留配对，实际回调借用者必须退役；返回本地已固定观测。
    pub fn freeze_deactivation_state(
        &self,
        state: &mut VulcanPluginDeactivationStateReservation,
        original: &mut VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationStateObservation, VulcanPluginRuntimeError> {
        // The verified store carries no replacement File or physical package pin.
        // 已验证存储不携带替代文件或物理包固定。
        let store = state.validate_binding(self, original)?;
        original
            .require_no_lifecycle_borrowers()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Same-original recovery alone may repair an otherwise intact sequencing-only gate.
        // 仅同原实例恢复可以修复其他方面完整的仅排序门。
        let _registration = lock_uninstallation_gate(self, || {
            state.validate_binding(self, original)?;
            state.observe(self, original, &store)?;
            original
                .require_no_lifecycle_borrowers()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
        })?;
        if state.cleanup_frozen || state.cleanup_started {
            return Err(VulcanPluginRuntimeError::new(
                "Original managed cleanup state was already frozen; observe the same request.",
            ));
        }
        if store
            .reconcile_deactivation_retained(original)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            != VulcanPluginDeactivationObservation::Original
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original cleanup state must be frozen before selection publication.",
            ));
        }
        state.validate_current_disabled(self)?;
        original
            .validate_local_state_freeze()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        state.cleanup_frozen = true;
        original.record_local_state_freeze();
        state.observe(self, original, &store)
    }

    /// Read exact local phase without any effects; state/original must be the same actual retained pair.
    /// 读取精确本地阶段，不产生效果；state／original 必须为相同实际保留配对。
    /// Returns local observation only, without treating a published tombstone as host/native or physical package completion.
    /// 仅返回本地观测，不把已发布停用记录当成宿主／原生或物理包完成。
    pub fn observe_deactivation_state(
        &self,
        state: &VulcanPluginDeactivationStateReservation,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationStateObservation, VulcanPluginRuntimeError> {
        // The actual allocation binding is checked even for terminal queries.
        // 即使终态查询仍检查实际分配绑定。
        let store = state.validate_binding(self, original)?;
        state.observe(self, original, &store)
    }

    /// Collect only the local environment after the caller independently proves Full removal and actual native/MCP union retirement.
    /// 仅在调用方独立证明完整移除及实际原生／MCP 集合退役之后回收本地环境。
    /// state/original remain borrowed across errors and panic; returns local physical confirmation without package/credential GC or a host receipt.
    /// state／original 跨错误及 panic 保持借用；返回本地物理确认，不执行包／凭据回收或提供宿主回执。
    pub fn collect_deactivation_state(
        &self,
        state: &mut VulcanPluginDeactivationStateReservation,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationStateObservation, VulcanPluginRuntimeError> {
        // Every operation requires the same real original, including terminal absence recovery.
        // 每个操作要求相同真实原实例，包括终态缺失恢复。
        let store = state.validate_binding(self, original)?;
        original
            .require_no_lifecycle_borrowers()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // The actual unit gate excludes new sessions across capacity retirement and the first environment ownership admission.
        // 实际空元组门跨容量退役及首次环境所有权准入排除新会话。
        let registration = lock_uninstallation_gate(self, || {
            state.validate_binding(self, original)?;
            state.observe(self, original, &store)?;
            original
                .require_no_lifecycle_borrowers()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
        })?;
        original
            .validate_published_for_catalog(&store)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if state.collected {
            return state.observe(self, original, &store);
        }
        if !state.cleanup_frozen {
            return Err(VulcanPluginRuntimeError::new(
                "Original local cleanup state was not frozen before publication.",
            ));
        }
        if let Some(owner) = self.formal_runtime_owner()? {
            owner.retire_plugin_capacities(state.plugin_id(), &registration)?;
        }
        retain_environment_cleanup(
            self,
            &state.selection.plugin_id,
            &mut state.environment_cleanup,
        )?;
        state.cleanup_started = true;
        collect_original_environment(self, state.plugin_id(), &state.environment_cleanup, || {
            original
                .validate_published_for_catalog(&store)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
        })?;
        original
            .validate_published_for_catalog(&store)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        confirm_environment_absence(state.plugin_id(), &state.environment_cleanup)?;
        state.collected = true;
        // Store the true local terminal confirmation before a test-only acknowledgement can fail or panic.
        // 在仅测试确认能够失败或 panic 之前，存储真实本地终态确认。
        #[cfg(test)]
        observation::checkpoint_exact(&state.package_root, state.plugin_id(), state.request_id())?;
        Ok(VulcanPluginDeactivationStateObservation::Collected)
    }
}

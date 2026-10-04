//! A published original target initializes one private persistent Hook before its exact domain is transferred.
//! 已发布原目标在精确域转交前初始化单个私有持久 Hook。

use super::*;

/// Frozen target authority owns the original request until confirmed transfer or actual pool retirement.
/// 固定目标权威拥有原请求，直至已确认转交或实际池退役。
pub(in crate::runtime) struct ActivationTargetHookAuthority {
    /// Full selected target fixes package, configuration, credentials and approved permissions together.
    /// 完整已选目标共同固定包、配置、凭据及已批准权限。
    pub(in crate::runtime) target: crate::VulcanPluginActivation,
    /// The exact staged descriptor retains actual managed generation and credential pins.
    /// 精确暂存描述符保留实际受管代次及凭据固定句柄。
    pub(in crate::runtime) plugin: VulcanPluginDescriptor,
    /// The exact original UUID cannot be replaced by a later namespace request.
    /// 精确原 UUID 不能被后续命名空间请求替换。
    pub(in crate::runtime) request_id: Uuid,
    /// The original closed catalog is restored on unconfirmed completion without inventing a route generation.
    /// 未确认完成时恢复原关闭目录，不虚构路由代次。
    pub(in crate::runtime) staged: Arc<crate::VulcanPluginCatalogSnapshot>,
    /// Only the creating manager owns this native initialization grant.
    /// 仅创建管理器拥有此原生初始化授权。
    manager_id: String,
    /// Same Arc retains the actual exclusive original lock without opening another file.
    /// 相同 Arc 保留实际原排他锁，不打开另一个文件。
    _request: Arc<std::fs::File>,
    /// Preparation owns this stable shared lease until it is moved into the original core domain.
    /// 准备拥有此稳定共享租约，直至其移入原核心域。
    environment: Mutex<Option<std::fs::File>>,
    /// Durable initialization ownership is derived only from this same original request and route.
    /// 持久初始化所有权仅派生自此相同原请求及路由。
    pub(in crate::runtime) initialization_owner:
        crate::generation_store::TargetHookInitializationOwner,
}

impl ActivationTargetHookAuthority {
    /// Bind transaction and staged to state after full disk and exact current-Arc validation; return one private owner.
    /// 完整磁盘及精确当前 Arc 校验后，将 transaction 与 staged 绑定至 state；返回单个私有所有者。
    pub(in crate::runtime) fn new(
        state: &VulcanPluginManagerState,
        transaction: &crate::VulcanPluginActivationTransaction,
        staged: &Arc<crate::VulcanPluginCatalogSnapshot>,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // The actual store validates original ownership before retaining its same lock Arc.
        // 实际存储在保留相同锁 Arc 前校验原所有权。
        let store = crate::VulcanPluginGenerationStore::open_existing(&state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original activation store is missing.")
            })?;
        store
            .validate_published_target_handoff_owner(transaction)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if !transaction
            .record()
            .update_policy()
            .is_some_and(|policy| policy.desired_enabled())
        {
            return Err(VulcanPluginRuntimeError::new(
                "Private target Hook requires the original enabled policy.",
            ));
        }
        // Retain the exact current catalog observation for this checked transition.
        // 为此已校验转换保留精确当前目录观察。
        let current = state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        if !Arc::ptr_eq(&current, staged) {
            return Err(VulcanPluginRuntimeError::new(
                "Private target Hook requires the exact staged catalog.",
            ));
        }
        // Freeze the complete original selected target.
        // 固定完整原已选目标。
        let target = transaction.record().target().clone();
        // Retain the descriptor selected by the exact current catalog.
        // 保留精确当前目录选择的描述符。
        let plugin = staged
            .plugins
            .get(&target.plugin_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Staged target Hook package is missing.")
            })?;
        if plugin.generation == 0 || plugin.runtime_state != VulcanPluginRuntimeState::Enabling {
            return Err(VulcanPluginRuntimeError::new(
                "Private target Hook requires a closed real routing generation.",
            ));
        }
        // Retain the original request and full selected target as one private authority.
        // 将原请求及完整已选目标保留为单个私有权威。
        let authority = Arc::new(Self {
            initialization_owner: transaction
                .target_hook_initialization_owner(plugin.generation)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
            target,
            plugin,
            request_id: transaction.record().request_id(),
            staged: Arc::clone(staged),
            manager_id: state.manager_id.clone(),
            _request: transaction.retain_published_target_owner(),
            environment: Mutex::new(None),
        });
        drop(current);
        authority.validate(state, &authority.plugin)?;
        Ok(authority)
    }

    /// Validate manager, full managed target and immutable route identity without authorizing business exports.
    /// 校验管理器、完整受管目标及不可变路由身份，不授权业务导出。
    /// plugin is the retained original descriptor; current may only be its closed or published exact target.
    /// plugin 为保留原描述符；当前目录仅可为其关闭或已发布精确目标。
    pub(in crate::runtime) fn validate(
        &self,
        state: &VulcanPluginManagerState,
        plugin: &VulcanPluginDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Observe original core or catalog evidence without replaying work.
        // 观测原核心或目录证据，不重放工作。
        let snapshot = state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        // Retain the exact current catalog observation for this checked transition.
        // 为此已校验转换保留精确当前目录观察。
        let current = snapshot
            .plugins
            .get(&self.target.plugin_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original target Hook is no longer installed.")
            })?;
        if self.manager_id != state.manager_id
            || plugin.id != self.plugin.id
            || plugin.generation != self.plugin.generation
            || plugin.package_root != self.plugin.package_root
            || plugin.content_fingerprint != self.plugin.content_fingerprint
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(&self.target))
            || current.generation != self.plugin.generation
            || current.origin != VulcanPluginPackageOrigin::UserInstalled
            || current.availability != crate::VulcanPluginAvailability::Enabled
            || !current.desired_enabled
            || !current.runtime_execution_available
            || !current
                .managed_package()
                .is_some_and(|package| package.matches_activation(&self.target))
            || !matches!(
                current.runtime_state,
                VulcanPluginRuntimeState::Enabling | VulcanPluginRuntimeState::Enabled
            )
        {
            return Err(VulcanPluginRuntimeError::new(
                "Private target Hook authority differs from the original selected route.",
            ));
        }
        Ok(())
    }

    /// Retain the permanent environment lock before preparation; return without replacing existing ownership.
    /// 准备前保留永久环境锁；返回时不替换既有所有权。
    pub(in crate::runtime) fn retain_environment(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate(state, &self.plugin)?;
        // Serialize ownership of the same permanent environment lease.
        // 串行化相同永久环境租约的所有权。
        let mut environment = self.environment.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Target Hook environment lock is poisoned.")
        })?;
        if environment.is_none() {
            *environment = Some(
                super::super::environment::retain_plugin_execution_environment(
                    state,
                    &self.plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            );
        }
        Ok(())
    }

    /// Move the same prepared environment file into the actual core domain; missing ownership is an error.
    /// 将相同已准备环境文件移入实际核心域；缺失所有权即为错误。
    pub(in crate::runtime) fn take_environment(
        &self,
    ) -> Result<std::fs::File, VulcanPluginRuntimeError> {
        self.environment
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Target Hook environment lock is poisoned.")
            })?
            .take()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Target Hook environment ownership is missing.")
            })
    }
}

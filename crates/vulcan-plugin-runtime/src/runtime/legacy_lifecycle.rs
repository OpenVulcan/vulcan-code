//! Original namespace ownership for explicitly reserved managed-removal execution.
//! 明确预留受管移除执行的原命名空间所有权。

use super::*;
use crate::{
    VulcanPluginActivation, VulcanPluginDeactivationReservation, VulcanPluginDescriptor,
    VulcanPluginLegacyMutationGuard, VulcanPluginPackageOrigin,
};
use std::sync::Weak;

/// One explicit authority supplies both formal admission and actual pool admission.
/// 单个显式权威同时提供正式入场及实际池入场。
#[derive(Clone)]
pub(in crate::runtime) enum CallExecutionSource {
    /// Ordinary execution validates the live published catalog.
    /// 普通执行校验实时已发布目录。
    Published,
    /// Activation execution retains the original prepared request.
    /// 激活执行保留原准备请求。
    Activation(Arc<super::candidates::ActivationExecution>),
    /// Guarded lifecycle execution retains its explicitly reserved managed-removal namespace.
    /// 受保护生命周期执行保留其明确预留受管移除命名空间。
    LegacyLifecycle(Arc<LegacyLifecycleExecution>),
}

impl CallExecutionSource {
    /// Borrow activation authority without retaining an additional independent selection.
    /// 借用激活权威，不保留额外独立选择。
    /// self is the sole source; returns an original borrowed activation Arc only for Activation.
    /// self 为唯一来源；仅对激活来源返回原激活 Arc 的借用。
    pub(in crate::runtime) fn activation(
        &self,
    ) -> Option<&Arc<super::candidates::ActivationExecution>> {
        match self {
            Self::Activation(authority) => Some(authority),
            Self::Published | Self::LegacyLifecycle(_) => None,
        }
    }

    /// Return whether execution avoids a published catalog session reservation.
    /// 返回执行是否免于已发布目录会话预留。
    /// self is the sole source; returns true for original private activation or lifecycle ownership.
    /// self 为唯一来源；原私有激活或生命周期所有权返回 true。
    pub(in crate::runtime) fn is_private(&self) -> bool {
        !matches!(self, Self::Published)
    }
}

/// Sealed original authority; actual calls and pools keep the same guard alive.
/// 封闭原权威；实际调用及池使同一护栏持续存活。
pub(in crate::runtime) struct LegacyLifecycleExecution {
    /// Weak identity prevents a registry-authority-manager ownership cycle.
    /// 弱身份避免注册表、权威及管理器形成所有权环。
    manager: Weak<VulcanPluginManagerState>,
    /// The exact namespace owner is never reacquired during callback admission.
    /// 回调入场期间绝不重新获取精确命名空间所有者。
    guard: Arc<VulcanPluginLegacyMutationGuard>,
    /// The actual user descriptor fixes package bytes and the original manifest.
    /// 实际用户描述符固定包字节及原清单。
    plugin: VulcanPluginDescriptor,
    /// Complete original selection evidence accompanies the actual guard; evidence alone grants no authority.
    /// 完整原选择证据伴随实际守卫；仅证据不授予权威。
    original: Box<VulcanPluginActivation>,
    /// Optional declaration is resolved once from the actual manifest.
    /// 可选声明从实际清单一次性解析。
    export: Option<String>,
    /// Lazy environment ownership survives original execution and native retirement.
    /// 延迟环境所有权跨原执行及原生退役存续。
    environment: Mutex<Option<std::fs::File>>,
}

impl LegacyLifecycleExecution {
    /// Bind the same original removal reservation to its exact disabled managed package and Uninstall scope.
    /// 将同一原移除预留绑定至其精确停用受管包及卸载作用域。
    /// state/plugin are the actual manager/catalog; original lends its real File; returns inert checked ownership.
    /// state／plugin 为实际管理器／目录；original 借出真实文件；返回未启动已校验所有权。
    pub(in crate::runtime) fn for_deactivation(
        state: &Arc<VulcanPluginManagerState>,
        plugin: VulcanPluginDescriptor,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // Full original evidence accompanies the same actual guard; evidence alone cannot manufacture this authority.
        // 完整原证据伴随同一实际守卫；仅证据不能制造此权威。
        let guard = original
            .lifecycle_guard()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Read the actual manifest before selecting the removal declaration.
        // 选择卸载声明前读取实际清单。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Legacy lifecycle package has no valid manifest.")
        })?;
        let export = manifest
            .lua_runtime
            .as_ref()
            .and_then(|runtime| runtime.lifecycle.uninstall.clone());
        // Retain the same original checked execution authority.
        // 保留同一原已校验执行权威。
        let authority = Arc::new(Self {
            manager: Arc::downgrade(state),
            guard,
            plugin,
            original: Box::new(original.original().clone()),
            export,
            environment: Mutex::new(None),
        });
        authority.validate(state, &authority.plugin, authority.scope())?;
        Ok(authority)
    }

    /// Return the sole native lease scope of a managed removal callback.
    /// 返回受管卸载回调唯一的原生租约作用域。
    pub(in crate::runtime) fn scope(&self) -> VulcanPluginBridgeLeaseScope {
        VulcanPluginBridgeLeaseScope::LifecycleUninstall
    }

    /// Validate original manager/root, full frozen identity and the removal scope before effects.
    /// 副作用前校验原管理器及根、完整冻结身份以及卸载作用域。
    /// No namespace is reopened; mismatched current selection returns an explicit error.
    /// 不重新打开命名空间；当前选择不匹配时返回明确错误。
    /// state and plugin must match original ownership and scope must be the removal scope; returns unit or exact authority rejection.
    /// state 及 plugin 必须匹配原所有权，scope 必须为卸载作用域；返回空值或精确权威拒绝。
    pub(in crate::runtime) fn validate(
        &self,
        state: &Arc<VulcanPluginManagerState>,
        plugin: &VulcanPluginDescriptor,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Weak::ptr_eq(&self.manager, &Arc::downgrade(state)) {
            return Err(VulcanPluginRuntimeError::new(
                "Legacy lifecycle belongs to another manager.",
            ));
        }
        self.validate_owner(state)?;
        // Inspect the current authoritative catalog without reopening the namespace.
        // 检查当前权威目录，不重新打开命名空间。
        let snapshot = state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        // Select the actual current descriptor by the original plugin identity.
        // 按原插件身份选择实际当前描述符。
        let current = snapshot.plugins.get(&self.plugin.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Legacy lifecycle package is no longer selected.")
        })?;
        if scope != self.scope()
            || !Self::same_descriptor(&self.plugin, plugin)
            || !Self::same_descriptor(&self.plugin, current)
            || current.origin != VulcanPluginPackageOrigin::UserInstalled
            || !self.matches_selection(plugin)
            || !self.matches_selection(current)
            || current.runtime_state != VulcanPluginRuntimeState::Disabled
            || state.active_session_count(&self.plugin.id)? != 0
            || state.has_cleanup_diagnostics(&self.plugin.id)?
        {
            return Err(VulcanPluginRuntimeError::new(
                "Legacy lifecycle authority does not match the exact disabled user package.",
            ));
        }
        Ok(())
    }

    /// Match descriptor against the complete managed original; no managed record is inferred from a package ID.
    /// 将 descriptor 与完整受管原记录匹配；不从包标识推断受管记录。
    /// descriptor is supplied or current routing; returns true only for the complete managed original.
    /// descriptor 为传入或当前路由；仅完整受管原记录返回真。
    fn matches_selection(&self, descriptor: &VulcanPluginDescriptor) -> bool {
        descriptor
            .managed_package()
            .is_some_and(|managed| managed.matches_activation(&self.original))
    }

    /// Verify only retained owner identity for cleanup, without requiring still-published routing.
    /// 仅核验清理的保留所有者身份，不要求路由仍处于已发布状态。
    /// state is the original manager; returns unit after original guard validation, or an ownership error.
    /// state 为原管理器；原护栏校验后返回空值，否则返回所有权错误。
    pub(in crate::runtime) fn validate_owner(
        &self,
        state: &Arc<VulcanPluginManagerState>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Weak::ptr_eq(&self.manager, &Arc::downgrade(state))
            || !self
                .manager
                .upgrade()
                .is_some_and(|original| Arc::ptr_eq(&original, state))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Legacy lifecycle belongs to another manager.",
            ));
        }
        self.guard
            .validate_user_root(&state.user_plugin_root, &self.plugin.id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
    }

    /// Compare actual catalog identity and original manifest ownership, without pointer-based package leases.
    /// 比较实际目录身份及原清单所有权，不使用包租约指针。
    /// original is the sealed descriptor and other is supplied/current routing; returns true only for their complete execution identity.
    /// original 为封闭描述符，other 为传入或当前路由；仅完整执行身份一致时返回 true。
    fn same_descriptor(original: &VulcanPluginDescriptor, other: &VulcanPluginDescriptor) -> bool {
        original.id == other.id
            && original.version == other.version
            && original.package_root == other.package_root
            && original.content_fingerprint == other.content_fingerprint
            && original.origin == other.origin
            && original.generation == other.generation
            && original.runtime_state == other.runtime_state
            && original.availability == other.availability
            && original.desired_enabled == other.desired_enabled
            && original.runtime_execution_available == other.runtime_execution_available
            && original
                .manifest
                .as_ref()
                .zip(other.manifest.as_ref())
                .is_some_and(|(left, right)| Arc::ptr_eq(left, right))
    }

    /// Borrow the frozen actual descriptor for pool construction.
    /// 借用冻结实际描述符用于池构建。
    /// self retains the original package; returns its immutable descriptor reference.
    /// self 保留原插件包；返回其不可变描述符引用。
    pub(in crate::runtime) fn plugin(&self) -> &VulcanPluginDescriptor {
        &self.plugin
    }

    /// Borrow the same typed mutation authority for actual configuration effects.
    /// 借用同一类型化变更权威用于实际配置副作用。
    /// self owns the guard Arc; returns its typed borrow without revealing an OS file.
    /// self 持有护栏 Arc；返回类型化借用，不暴露系统文件。
    pub(in crate::runtime) fn mutation_guard(&self) -> &VulcanPluginLegacyMutationGuard {
        &self.guard
    }

    /// Borrow the original declared export; absence creates no formal parent.
    /// 借用原声明导出；缺失时不创建正式父级。
    /// self retains the inspected manifest; returns its exact optional lifecycle export.
    /// self 保留已检查清单；返回其精确可选生命周期导出。
    pub(in crate::runtime) fn export(&self) -> Option<&str> {
        self.export.as_deref()
    }

    /// Compare original guard ownership when preventing overlapping effects.
    /// 阻止副作用重叠时比较原护栏所有权。
    /// other is another retained authority; returns true only when both retain the same original guard Arc.
    /// other 为另一个保留权威；仅两者保留同一原护栏 Arc 时返回 true。
    pub(in crate::runtime) fn same_guard(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.guard, &other.guard)
    }

    /// Transfer the prepared environment lease exactly once into the actual pool owner.
    /// 将准备环境租约恰好一次转移至实际池所有者。
    /// self must retain a prepared lease; returns the actual file or an explicit missing/poisoned error.
    /// self 必须保留准备租约；返回实际文件或明确缺失、污染错误。
    pub(in crate::runtime) fn take_environment(
        &self,
    ) -> Result<std::fs::File, VulcanPluginRuntimeError> {
        self.environment
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Legacy environment ownership lock is poisoned.")
            })?
            .take()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Legacy lifecycle environment was not prepared.")
            })
    }

    /// Release only untransferred preparation ownership after actual call cleanup completes.
    /// 实际调用清理完成后仅释放未转移准备所有权。
    /// self is the original execution authority; returns unit after release or a poisoned-lock error.
    /// self 为原执行权威；释放后返回空值，否则返回锁污染错误。
    pub(in crate::runtime) fn retire_prepared_environment(
        &self,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.environment
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Legacy environment ownership lock is poisoned.")
            })?
            .take();
        Ok(())
    }

    /// Retain the existing environment lease under the same original authority.
    /// 在同一原权威下保留既有环境租约。
    /// state must be the actual original manager; returns unit after monotonic lease retention or an error.
    /// state 必须为实际原管理器；单向保留租约后返回空值，否则返回错误。
    pub(in crate::runtime) fn retain_execution_environment(
        &self,
        state: &Arc<VulcanPluginManagerState>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate(state, &self.plugin, self.scope())?;
        // Serialize ownership of the actual prepared environment lease.
        // 串行化实际准备环境租约所有权。
        let mut environment = self.environment.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Legacy environment ownership lock is poisoned.")
        })?;
        if environment.is_none() {
            *environment = Some(
                super::environment::retain_plugin_execution_environment(state, &self.plugin.id)
                    .map_err(VulcanPluginRuntimeError::new)?,
            );
        }
        Ok(())
    }
}

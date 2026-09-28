//! Single manager ownership and control for the formal plugin parent runtime.
//! 正式插件父运行时的单个管理器所有权及控制。

use super::*;
use luaskills::runtime::embedded::{EmbeddedRuntimeUsage, PoolUsage};
use serde::Serialize;

mod budgets;
pub use budgets::VulcanPluginFormalPluginStatus;
mod capacities;
pub use capacities::{
    VulcanPluginCapacityRegistrationRequest, VulcanPluginFormalCapacityCatalog,
    VulcanPluginFormalCapacityStatus,
};

mod calls;
pub use calls::VulcanPluginFormalCallStatus;
pub use calls::reuse::VulcanPluginReusableDomainStatus;
mod pinned;
pub use pinned::VulcanPluginFormalSessionStatus;
pub(super) use pinned::deadline as operation_deadline;
mod instances;
#[cfg(test)]
pub(super) use instances::PersistentRuntime;

/// Manager-owned formal runtime and the exact settings frozen at construction.
/// 管理器持有的正式运行时及构造时冻结的精确设置。
pub(super) struct ManagedFormalRuntime {
    /// Native adapter owns the core parent and all runtime-local capability registrations.
    /// 原生适配器拥有核心父级及全部运行时内能力注册。
    pub host: VulcanPluginEmbeddedHost,
    /// Construction baseline freezes parent/plugin limits and timeouts; live purpose policy belongs to capacities.
    /// 构造基线冻结父级／插件上限及超时；活动用途策略属于 capacities。
    pub settings: VulcanPluginRuntimeSettings,
    /// Exact core capacity identities and preparing-member claims survive module generation changes.
    /// 精确核心容量身份及准备中成员声明在模块代次变化后保持存活。
    capacities: Mutex<capacities::CapacityRegistry>,
    /// Bounded host ownership covers preparation through physical cleanup and unacknowledged results.
    /// 有界宿主所有权覆盖准备、物理清理及未确认结果。
    calls: Mutex<BTreeMap<String, Arc<calls::FormalCall>>>,
    /// Bounded reusable domain metadata; physical VM accounting remains exclusively in LuaSkills.
    /// 有界可复用域元数据；物理 VM 记账仍完全属于 LuaSkills。
    reusable: Mutex<calls::reuse::Registry>,
    /// One weakly owned observer collects independent idle-instance finalizations.
    /// 单个弱持有观测者收集独立空闲实例关闭结果。
    reuse_observer: AtomicBool,
    /// Host session identities exist before preparation and survive interrupted observers.
    /// 宿主会话身份在准备前存在，并在观测者中断后存活。
    pinned: Mutex<BTreeMap<String, Arc<pinned::PinnedSession>>>,
    /// Persistent Hook ownership includes preparation and physical retirement.
    /// 持久 Hook 所有权包含准备及物理退役。
    hooks: Mutex<BTreeMap<String, Arc<instances::PersistentRuntime>>>,
    /// Persistent UI instances share ownership mechanics but never Hook state or finalizers.
    /// 持久 UI 实例共享所有权机制，但绝不共享 Hook 状态或关闭器。
    ui_actions: Mutex<BTreeMap<String, Arc<instances::PersistentRuntime>>>,
}

/// Read-only observation of this manager's formal runtime, separate from legacy lease accounting.
/// 此管理器正式运行时的只读观测，与旧租约记账分开。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalRuntimeStatus {
    /// Core-issued identity shared by every clone of this manager.
    /// 此管理器全部克隆共享的核心签发身份。
    pub runtime_id: String,
    /// Exact validated settings, including the effective parent limits.
    /// 精确已校验设置，包含生效父级上限。
    pub settings: VulcanPluginRuntimeSettings,
    /// Current retained administrative receipt is captured atomically with its effective settings.
    /// 当前保留管理回执与其生效设置原子捕获。
    pub policy_update: Option<VulcanPluginCapacityPolicyRecord>,
    /// Current queue and operation accounting from the sole core owner.
    /// 来自唯一核心所有者的当前队列及操作记账。
    pub operations: EmbeddedRuntimeUsage,
    /// Actual VM accounting, including creation and unfinished retirement.
    /// 真实 VM 记账，包含创建中及尚未完成退役。
    pub resources: PoolUsage,
}

impl VulcanPluginRuntimeManager {
    /// Attach exactly one formal runtime using `settings` and the already attached plugin engine.
    /// 使用 `settings` 及已附加插件引擎，恰好附加一个正式运行时。
    /// Returns an error without publishing a partial runtime; repeated attachment never replaces an owner.
    /// 错误时不发布局部运行时；重复附加绝不替换所有者。
    pub fn initialize_formal_runtime(
        &self,
        settings: VulcanPluginRuntimeSettings,
    ) -> Result<(), VulcanPluginRuntimeError> {
        settings.validate().map_err(VulcanPluginRuntimeError::new)?;
        // Hold only the attachment lock; construction registers metadata and starts bounded core workers, never Lua.
        // 仅持有附加锁；构造注册元数据并启动有界核心工作线程，绝不运行 Lua。
        let mut slot = self
            .state
            .formal_runtime
            .lock()
            .map_err(|_| formal_lock_error())?;
        if slot.is_some() {
            return Err(VulcanPluginRuntimeError::new(
                "Formal plugin runtime is already attached.",
            ));
        }
        // Preserve disk transaction evidence before creating any native owner; attachment never rewrites it.
        // 创建任何原生所有者前保留磁盘事务证据；附加绝不回写它。
        let policy =
            super::settings::policy::PolicyStore::load(self.plugin_state_root(), &settings)
                .map_err(VulcanPluginRuntimeError::new)?;
        // The adapter validates native registration capacity before starting its core owner.
        // 适配器在启动核心所有者前校验原生注册容量。
        let host = VulcanPluginEmbeddedHost::new(
            self,
            settings.parent.clone(),
            settings.max_capability_call_ms,
        )
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Explicit guarantees exist before any shared production work can consume the parent budget.
        // 在任何公共生产工作消费父级预算前，显式保证已经存在。
        let capacities = capacities::register_configured_capacities(&host, &settings, policy)?;
        *slot = Some(Arc::new(ManagedFormalRuntime {
            host,
            settings,
            capacities: Mutex::new(capacities),
            calls: Mutex::new(BTreeMap::new()),
            reusable: Mutex::new(calls::reuse::Registry::default()),
            reuse_observer: AtomicBool::new(false),
            pinned: Mutex::new(BTreeMap::new()),
            hooks: Mutex::new(BTreeMap::new()),
            ui_actions: Mutex::new(BTreeMap::new()),
        }));
        Ok(())
    }

    /// Return the actual attached capability registry for trusted host registration and queued dispatch.
    /// 返回真实附加的能力注册表，供可信宿主注册及分发排队调用。
    /// Uses only this manager; absence is an error and never creates a parent or another registry.
    /// 仅使用此管理器；未附加时返回错误，绝不创建父级或另一个注册表。
    /// The returned handle retains its original identity after shutdown; core closure still rejects new registration.
    /// 返回句柄在关闭后保留原身份；核心关闭仍拒绝新注册。
    /// Callers explicitly refresh the catalog after changing registrations and retain responsibility for handler authority.
    /// 调用方在变更注册后显式刷新目录，并继续负责处理器授权。
    pub fn formal_capabilities(
        &self,
    ) -> Result<
        Arc<luaskills::runtime::embedded::capabilities::CapabilityRegistry>,
        VulcanPluginRuntimeError,
    > {
        // Keep the exact attached parent alive until its existing registry handle has been cloned.
        // 保持精确附加父级存活，直到其已有注册表句柄完成克隆。
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal plugin runtime is not attached.")
        })?;
        Ok(owner.host.runtime().capabilities())
    }

    /// Return this manager's shared formal owner, or `None` for a metadata-only manager.
    /// 返回此管理器共享的正式所有者；纯元数据管理器返回 `None`。
    /// The attachment lock is released before callers enter core control or execution.
    /// 调用方进入核心控制或执行前释放附加锁。
    pub(super) fn formal_runtime_owner(
        &self,
    ) -> Result<Option<Arc<ManagedFormalRuntime>>, VulcanPluginRuntimeError> {
        Ok(self
            .state
            .formal_runtime
            .lock()
            .map_err(|_| formal_lock_error())?
            .clone())
    }

    /// Observe effective settings and current accounting without creating a runtime or changing configuration.
    /// 观测生效设置及当前记账，不创建运行时或修改配置。
    /// Returns `None` before explicit attachment; core observation failures remain explicit errors.
    /// 显式附加前返回 `None`；核心观测失败保持显式错误。
    pub fn formal_runtime_status(
        &self,
    ) -> Result<Option<VulcanPluginFormalRuntimeStatus>, VulcanPluginRuntimeError> {
        // Absence is a real attachment state, never a reason to create an implicit runtime.
        // 缺失是真实附加状态，绝不据此隐式创建运行时。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // The cloned owner keeps this exact runtime alive throughout both observations.
        // 克隆所有者在两次观测期间保持此精确运行时存活。
        let runtime = owner.host.runtime();
        // Persisted intent and current settings must never be sampled from different policy transactions.
        // 持久意图及当前设置绝不能采样自不同策略事务。
        let (settings, policy_update) = owner.policy_status()?;
        Ok(Some(VulcanPluginFormalRuntimeStatus {
            runtime_id: runtime.id().to_string(),
            settings,
            policy_update,
            operations: runtime
                .usage()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
            resources: runtime
                .resources()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
        }))
    }

    /// Permanently close formal admission and request cancellation without claiming that resources have drained.
    /// 永久关闭正式入场并请求取消，不宣称资源已经排空。
    /// No runtime means no work; attached owners remain available for subsequent observations.
    /// 没有运行时即没有工作；已附加所有者继续可供后续观测。
    pub fn request_formal_runtime_close(&self) -> Result<(), VulcanPluginRuntimeError> {
        if let Some(owner) = self.formal_runtime_owner()? {
            self.close_reusable_domains(None, None, true)?;
            owner.cancel_calls()?;
            owner.cancel_pinned()?;
            owner.cancel_instances()?;
            owner
                .host
                .runtime()
                .request_close()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        }
        Ok(())
    }

    /// Check actual core worker and VM termination after a close request.
    /// 在关闭请求后检查核心工作线程与 VM 的真实终止。
    /// Returns true only without a runtime, or after host ownership drains and the core confirms complete shutdown.
    /// 仅在无运行时，或宿主所有权排空且核心确认完整关闭后返回真值。
    pub fn poll_formal_runtime_closed(&self) -> Result<bool, VulcanPluginRuntimeError> {
        if let Some(owner) = self.formal_runtime_owner()? {
            self.maintain_reusable_domains(&owner)?;
            if !owner
                .reusable
                .lock()
                .map_err(|_| formal_lock_error())?
                .is_empty()
            {
                return Ok(false);
            }
        }
        match self.formal_runtime_owner()? {
            Some(owner) if !owner.calls_drained()? => Ok(false),
            Some(owner) if !owner.pinned_drained()? => Ok(false),
            Some(owner) if !owner.instances_drained()? => Ok(false),
            Some(owner) => owner
                .host
                .runtime()
                .poll_closed()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string())),
            None => Ok(true),
        }
    }
}

/// Construct the stable attachment-lock error without hiding poisoned ownership.
/// 构造稳定附加锁错误，不隐藏已中毒的所有权。
fn formal_lock_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Formal plugin runtime attachment lock is poisoned.")
}

#[cfg(test)]
mod tests;

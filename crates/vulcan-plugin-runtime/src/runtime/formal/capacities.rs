//! Stable purpose ownership links host preparation to the sole native capacity authority.
//! 稳定用途归属将宿主准备连接至唯一原生容量权威。

use super::*;
use crate::runtime::settings::policy::{PolicyDocument, PolicyStore};
use luaskills::runtime::embedded::{
    EmbeddedCapacityConfig, EmbeddedCapacityPolicySnapshot, EmbeddedCapacitySnapshot,
};
use std::collections::BTreeSet;

mod lifecycle;
mod management;
mod policy;

pub use management::{VulcanPluginCapacityRegistrationRequest, VulcanPluginFormalCapacityCatalog};

/// Keep exact capacity identities and explicit lifecycle retirement intent under one ownership lock.
/// 在同一归属锁下保留精确容量身份及明确生命周期回收意图。
pub(super) struct CapacityRegistry {
    /// Opaque mapping predecessor changes on insertion or removal, including compensated registrations.
    /// 不透明映射前驱在插入或移除时变化，包含被补偿的注册。
    revision: String,
    /// Current effective purpose policy and retained durable transaction share the ownership lock.
    /// 当前生效用途策略及保留持久事务共用归属锁。
    policy: PolicyStore,
    /// Each semantic owner retains the core-issued identity until successful explicit removal.
    /// 每个语义所有者保留核心签发身份，直到显式移除成功。
    registrations: BTreeMap<(String, VulcanPluginCapacityPurpose), CapacityRegistration>,
    /// Failed lifecycle retirement fences fresh admission and preserves cleanup diagnostics.
    /// 失败的生命周期回收阻止新入场，并保留清理诊断。
    retiring_plugins: BTreeSet<String>,
}

/// Register every explicit settings policy in host before publishing the parent; return exact retained identities.
/// 发布父级前，在 host 中注册全部显式 settings 策略；返回精确保留身份。
/// No Lua executes; failure drops the unpublished native owner instead of exposing partial guarantees.
/// 不执行 Lua；失败时释放未发布原生所有者，不暴露局部保证。
pub(super) fn register_configured_capacities(
    host: &VulcanPluginEmbeddedHost,
    settings: &VulcanPluginRuntimeSettings,
    policy: PolicyStore,
) -> Result<CapacityRegistry, VulcanPluginRuntimeError> {
    // Only actual native success may become a host identity; the owner is not public during this transaction.
    // 只有实际原生成功才能成为宿主身份；此事务期间所有者尚未公开。
    let mut registrations = BTreeMap::new();
    for (plugin_id, purposes) in &settings.capacity_budgets {
        if purposes.is_empty() {
            continue;
        }
        host.runtime()
            .register_plugin(plugin_id.clone(), settings.plugin_budget(plugin_id).0)
            .map_err(calls::core_error)?;
        for (purpose, config) in purposes {
            // The validated configured minima are committed once, even before the first module is admitted.
            // 已校验配置最小值仅承诺一次，即使第一个模块尚未入场。
            let identity = host
                .runtime()
                .register_capacity(plugin_id, config.clone())
                .map_err(calls::core_error)?;
            registrations.insert(
                (plugin_id.clone(), *purpose),
                CapacityRegistration {
                    identity,
                    preparing_members: 0,
                    management_request: None,
                },
            );
        }
    }
    Ok(CapacityRegistry {
        revision: uuid::Uuid::new_v4().to_string(),
        policy,
        registrations,
        retiring_plugins: BTreeSet::new(),
    })
}

/// One native identity per exact plugin purpose; this is not a second physical resource ledger.
/// 每个精确插件用途对应单个原生身份；这不是第二份物理资源账本。
pub(super) struct CapacityRegistration {
    /// Successful administrative request remains attached to this original identity until explicit forgetting.
    /// 成功管理请求附着于此原身份，直至显式遗忘。
    management_request: Option<VulcanPluginCapacityRegistrationRequest>,
    /// Core-issued identity never changes while retained, including after closure.
    /// 核心签发身份在保留期间绝不变化，包括关闭后。
    identity: String,
    /// Real host preparations retain the identity until their registration attempt returns.
    /// 真实宿主准备在注册尝试返回前保留身份。
    preparing_members: usize,
}

/// A preparation claim prevents explicit forgetting before its actual native registration returns.
/// 准备声明阻止在实际原生注册返回前显式遗忘容量。
pub(super) struct CapacityAdmission<'a> {
    /// Borrow the original owner whose enclosing preparation task retains its runtime lifetime.
    /// 借用原所有者；外层准备任务保留其运行时寿命。
    owner: &'a ManagedFormalRuntime,
    /// Immutable semantic key identifies the retained registration, not a fallback lookup.
    /// 不可变语义键标识保留注册，不用于回退查找。
    key: (String, VulcanPluginCapacityPurpose),
    /// Exact native identity frozen before bundle preparation.
    /// bundle 准备前冻结的精确原生身份。
    pub(super) capacity_id: String,
    /// The complete validated policy supplies member kind and queue limits.
    /// 完整已校验策略提供成员类别及队列上限。
    pub(super) config: EmbeddedCapacityConfig,
    /// Exact current native policy revision partitions newly registered execution bindings.
    /// 精确当前原生策略修订划分新注册执行绑定。
    pub(super) policy_revision: String,
}

impl Drop for CapacityAdmission<'_> {
    /// Release only the completed preparation claim; native members and reservations retain their own lifetime.
    /// 仅释放已完成准备声明；原生成员及预留保留自身寿命。
    fn drop(&mut self) {
        // Poisoned ownership stays retained and every control API reports the lock failure; never claim release.
        // 中毒归属保持保留，全部控制接口报告锁失败；绝不宣称释放。
        let Ok(mut registrations) = self.owner.capacities.lock() else {
            return;
        };
        // Explicit forgetting is forbidden while this guard exists, so the original entry must remain.
        // 此守卫存在期间禁止显式遗忘，因此原条目必须保留。
        let registration = registrations
            .registrations
            .get_mut(&self.key)
            .expect("capacity admission retains its registration");
        assert_eq!(
            registration.identity, self.capacity_id,
            "capacity identity changed during preparation"
        );
        registration.preparing_members = registration
            .preparing_members
            .checked_sub(1)
            .expect("one capacity preparation claim");
    }
}

/// Read-only native ownership plus the host claims not yet represented by a core member pool.
/// 只读原生归属，以及尚未由核心成员池表示的宿主声明。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalCapacityStatus {
    /// Retained management identity allows observers to recover the original registration without resubmission.
    /// 保留管理身份使观测者无需重新提交即可恢复原登记。
    pub management_request: Option<VulcanPluginCapacityRegistrationRequest>,
    /// The exact parent identity distinguishes this native ownership lifetime.
    /// 精确父级身份区分此原生归属寿命。
    pub runtime_id: String,
    /// Opaque predecessor belongs to the same atomic native observation as snapshot.
    /// 不透明前驱与 snapshot 属于同一次原子原生观测。
    pub policy_revision: String,
    /// Actual retained usage still exceeds the published desired policy.
    /// 实际保留用量仍超过已发布目标策略。
    pub pending_convergence: bool,
    /// Host routing chose this purpose independently of plugin business arguments.
    /// 宿主路由独立于插件业务参数选择此用途。
    pub purpose: VulcanPluginCapacityPurpose,
    /// Complete policy provenance comes from the same current settings that govern member admission.
    /// 完整策略来源来自治理成员入场的同一当前设置。
    pub budget_source: VulcanPluginCapacityBudgetSource,
    /// Actual unfinished registration preparations prevent metadata forgetting.
    /// 实际未结束注册准备阻止元数据遗忘。
    pub preparing_members: usize,
    /// True while this plugin's explicit lifecycle retirement still owns unfinished cleanup.
    /// 此插件明确生命周期回收仍持有未完成清理时为真。
    pub retirement_requested: bool,
    /// Physical use and core work come only from the original native capacity owner.
    /// 物理用量及核心工作仅来自原原生容量所有者。
    pub snapshot: EmbeddedCapacitySnapshot,
}

impl ManagedFormalRuntime {
    /// Claim plugin_id/purpose under the caller-held publication gate; return one exact capacity binding.
    /// 在调用方持有的发布门下声明 plugin_id／purpose；返回单个精确容量绑定。
    /// First registration uses current effective settings; retained, missing or closed identities are never silently replaced.
    /// 首次注册使用当前生效设置；绝不静默替换保留、缺失或关闭的身份。
    pub(super) fn admit_capacity_member(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
        publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<CapacityAdmission<'_>, VulcanPluginRuntimeError> {
        self.ensure_plugin_budget(plugin_id, publication)?;
        // Publication orders native policy changes, new claims and explicit metadata removal.
        // 发布门排序原生策略变化、新声明及显式元数据移除。
        let mut registry = self.capacities.lock().map_err(|_| capacity_lock_error())?;
        registry.require_policy_admission(plugin_id, purpose)?;
        if registry.retiring_plugins.contains(plugin_id) {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin capacity lifecycle retirement is unfinished.",
            ));
        }
        // Semantic ownership remains independent of package generation and mutable module state.
        // 语义归属保持独立于包代次及可变模块状态。
        let key = (plugin_id.to_owned(), purpose);
        if !registry.registrations.contains_key(&key) {
            // Resolve the complete live policy from its sole host authority.
            // 从唯一宿主权威解析完整活动策略。
            let config = registry
                .policy
                .settings
                .capacity_budget(plugin_id, purpose)
                .0;
            // Retain successful native ownership before performing another fallible observation.
            // 进行另一个可能失败的观测前保留成功原生归属。
            let identity = self
                .host
                .runtime()
                .register_capacity(plugin_id, config)
                .map_err(calls::core_error)?;
            registry.insert_registration(
                key.clone(),
                CapacityRegistration {
                    identity,
                    preparing_members: 0,
                    management_request: None,
                },
            );
        }
        // Read revision and configuration together before publishing the actual preparation claim.
        // 发布实际准备声明前，同时读取修订及配置。
        let policy = self.verify_capacity(
            plugin_id,
            purpose,
            registry.registrations.get(&key).expect("retained capacity"),
            &registry.policy.settings,
        )?;
        if policy.capacity.closing {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin purpose capacity admission is permanently closed.",
            ));
        }
        // The original registration remains retained even when validation fails, without a fabricated active claim.
        // 即使校验失败，原注册仍保持保留，不虚构活动声明。
        let registration = registry
            .registrations
            .get_mut(&key)
            .expect("retained capacity");
        registration.preparing_members = registration
            .preparing_members
            .checked_add(1)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Capacity preparation count overflow."))?;
        Ok(CapacityAdmission {
            owner: self,
            key,
            capacity_id: registration.identity.clone(),
            config: policy.capacity.config,
            policy_revision: policy.revision,
        })
    }

    /// Validate registration against exact plugin_id/purpose and supplied effective settings; return one atomic native policy.
    /// 针对精确 plugin_id／purpose 及给定生效 settings 校验 registration；返回单个原子原生策略。
    fn verify_capacity(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
        registration: &CapacityRegistration,
        settings: &VulcanPluginRuntimeSettings,
    ) -> Result<EmbeddedCapacityPolicySnapshot, VulcanPluginRuntimeError> {
        // Missing native identity is an ownership failure, never permission to register a replacement.
        // 原生身份缺失是归属失败，绝不据此注册替代身份。
        let policy = self
            .host
            .runtime()
            .capacity_policy(&registration.identity)
            .map_err(calls::core_error)?;
        if policy.capacity.plugin_id != plugin_id
            || policy.capacity.config != settings.capacity_budget(plugin_id, purpose).0
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin capacity differs from its current host ownership or configuration.",
            ));
        }
        Ok(policy)
    }
}

impl VulcanPluginRuntimeManager {
    /// Observe exact plugin_id/purpose without allocation; return None before registration or parent attachment.
    /// 不分配资源地观测精确 plugin_id／purpose；注册或父级附加前返回 None。
    /// Core counters and preparation claims are observed under the same metadata lock.
    /// 核心计数及准备声明在同一元数据锁下观测。
    pub fn formal_capacity_status(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> Result<Option<VulcanPluginFormalCapacityStatus>, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "capacity owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // A read never allocates a missing runtime or capacity.
        // 读取绝不分配缺失运行时或容量。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // One ownership lock orders the host policy with the same native revision and resource snapshot.
        // 同一归属锁将宿主策略与相同原生修订及资源快照排序。
        let registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // Exact semantic lookup cannot switch to another purpose or generation.
        // 精确语义查找不能切换至另一用途或代次。
        let Some(registration) = registry.registrations.get(&(plugin_id.to_owned(), purpose))
        else {
            return Ok(None);
        };
        owner
            .capacity_status_locked(&registry, plugin_id, purpose, registration)
            .map(Some)
    }

    /// Close the exact plugin_id/purpose and its current members; return whether a registration existed.
    /// 关闭精确 plugin_id／purpose 及当前成员；返回注册是否存在。
    /// Preparation may still finish with a closed error; acknowledgement never proves physical drainage.
    /// 准备仍可能以关闭错误结束；确认绝不证明物理排空。
    pub fn request_formal_capacity_close(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "capacity owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // Order explicit lifecycle control with every production registration preparation.
        // 将显式生命周期控制与全部生产注册准备排序。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        // A metadata-only manager has nothing to close.
        // 纯元数据管理器没有需要关闭的资源。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        // Retain the original mapping even after permanent native admission closure.
        // 原生入场永久关闭后仍保留原映射。
        let registrations = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // Explicit absence does not create a capacity.
        // 明确缺失不创建容量。
        let Some(registration) = registrations
            .registrations
            .get(&(plugin_id.to_owned(), purpose))
        else {
            return Ok(false);
        };
        owner.verify_capacity(
            plugin_id,
            purpose,
            registration,
            &registrations.policy.settings,
        )?;
        owner
            .host
            .runtime()
            .close_capacity(&registration.identity)
            .map_err(calls::core_error)?;
        Ok(true)
    }

    /// Forget exact plugin_id/purpose only after preparation and native members drain; return whether removed.
    /// 仅在准备及原生成员排空后遗忘精确 plugin_id／purpose；返回是否移除。
    /// Failed removal retains the original identity; later admission may register anew only after explicit success.
    /// 移除失败保留原身份；仅在显式成功之后，后续入场才可重新注册。
    pub fn forget_formal_capacity(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "capacity owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // Prevent publication from inserting a new preparation between checking and removing the original owner.
        // 防止发布在检查及移除原所有者之间插入新准备。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        // An unattached manager has no native capacity metadata to release.
        // 未附加管理器没有需要释放的原生容量元数据。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        // No callbacks or physical destructors run during this metadata transaction.
        // 此元数据事务期间不运行回调或物理析构器。
        let mut registrations = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // Keep exactly this semantic owner until both layers have acknowledged release.
        // 在两层均确认释放前保留此精确语义所有者。
        let key = (plugin_id.to_owned(), purpose);
        // Absence is reported rather than inventing a successful release of another object.
        // 报告缺失，不虚构另一对象成功释放。
        let Some(registration) = registrations.registrations.get(&key) else {
            return Ok(false);
        };
        registrations.require_policy_admission(plugin_id, purpose)?;
        owner.verify_capacity(
            plugin_id,
            purpose,
            registration,
            &registrations.policy.settings,
        )?;
        if registration.preparing_members != 0 {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin capacity still owns unfinished member preparation.",
            ));
        }
        owner
            .host
            .runtime()
            .forget_capacity(&registration.identity)
            .map_err(calls::core_error)?;
        registrations.remove_registration(&key);
        Ok(true)
    }
}

/// Return a stable ownership diagnostic for a poisoned capacity registry without recovering uncertain state.
/// 为中毒容量注册表返回稳定归属诊断，不恢复不确定状态。
fn capacity_lock_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Formal plugin capacity lock is poisoned.")
}

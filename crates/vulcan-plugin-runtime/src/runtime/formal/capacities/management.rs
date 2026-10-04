//! Explicit metadata registration uses mapping predecessors without starting plugin business.
//! 明确元数据登记使用映射前驱，不启动插件业务。

use super::*;
use serde::Deserialize;

/// A caller-owned request creates exactly one previously absent purpose in an observed native parent.
/// 调用方拥有的请求在已观测原生父级内创建一个此前缺失的用途。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginCapacityRegistrationRequest {
    /// Caller-generated UUID identifies the original request while its capacity remains retained.
    /// 调用方生成的 UUID 在容量保留期间标识原请求。
    pub registration_id: String,
    /// Exact parent identity prevents replay after runtime replacement or process restart.
    /// 精确父级身份阻止运行时替换或进程重启后的重放。
    pub runtime_id: String,
    /// Opaque catalog predecessor must match; callers must never automatically refresh and retry it.
    /// 不透明目录前驱必须匹配；调用方绝不能自动刷新并重试。
    pub expected_registry_revision: String,
    /// Exact installed Lua plugin selected by trusted management, never by plugin business arguments.
    /// 可信管理选择的精确已安装 Lua 插件，绝非插件业务参数选择。
    pub plugin_id: String,
    /// Host purpose chooses its existing effective policy, without inventing a new configuration.
    /// 宿主用途选择既有生效策略，不虚构新配置。
    pub purpose: VulcanPluginCapacityPurpose,
}

/// Read-only mapping snapshot supports management discovery without admitting business or allocating capacities.
/// 只读映射快照支持管理发现，不接纳业务或分配容量。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalCapacityCatalog {
    /// Exact parent owns every listed capacity and the mapping revision.
    /// 精确父级拥有全部列出容量及映射修订。
    pub runtime_id: String,
    /// One mapping revision is observed under the same ownership lock as all registrations.
    /// 单个映射修订与全部注册在同一归属锁下观测。
    pub registry_revision: String,
    /// Each native capacity has its own atomic observation; physical counters are not globally simultaneous.
    /// 各原生容量分别原子观测；物理计数并非全局同时采样。
    pub capacities: Vec<VulcanPluginFormalCapacityStatus>,
}

impl CapacityRegistry {
    /// Retain registration under absent key after native success; invalidate every earlier mapping predecessor.
    /// 原生成功后在缺失 key 下保留 registration；使全部先前映射前驱失效。
    /// Return no value; callers hold the sole ownership lock and must prove absence before native registration.
    /// 无返回值；调用方持有唯一归属锁，必须在原生登记前证明缺失。
    pub(super) fn insert_registration(
        &mut self,
        key: (String, VulcanPluginCapacityPurpose),
        registration: CapacityRegistration,
    ) {
        assert!(
            !self.registrations.contains_key(&key),
            "capacity owner already retained"
        );
        self.registrations.insert(key, registration);
        self.revision = uuid::Uuid::new_v4().to_string();
    }

    /// Remove original key only after native forgetting succeeds; invalidate predecessors even during compensation.
    /// 仅在原生遗忘成功后移除原 key；即使补偿期间也使前驱失效。
    /// Return no value; removal cannot silently acknowledge another or missing owner.
    /// 无返回值；移除不能静默确认另一或缺失所有者。
    pub(super) fn remove_registration(&mut self, key: &(String, VulcanPluginCapacityPurpose)) {
        self.registrations
            .remove(key)
            .expect("original capacity retained until native forgetting");
        self.revision = uuid::Uuid::new_v4().to_string();
    }
}

impl ManagedFormalRuntime {
    /// Project registration's actual policy for plugin_id/purpose under registry's current mapping lock.
    /// 在 registry 当前映射锁下，为 plugin_id／purpose 投影 registration 的实际策略。
    /// Return one native observation with host provenance; never create or repair missing native ownership.
    /// 返回附带宿主来源的单个原生观测；绝不创建或修复缺失原生归属。
    pub(super) fn capacity_status_locked(
        &self,
        registry: &CapacityRegistry,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
        registration: &CapacityRegistration,
    ) -> Result<VulcanPluginFormalCapacityStatus, VulcanPluginRuntimeError> {
        // A single core query supplies revision, limits and actual convergence evidence.
        // 单次核心查询提供修订、上限及真实收敛证据。
        let policy =
            self.verify_capacity(plugin_id, purpose, registration, &registry.policy.settings)?;
        Ok(VulcanPluginFormalCapacityStatus {
            management_request: registration.management_request.clone(),
            runtime_id: self.host.runtime().id().to_owned(),
            policy_revision: policy.revision,
            pending_convergence: policy.pending_convergence,
            purpose,
            budget_source: registry
                .policy
                .settings
                .capacity_budget(plugin_id, purpose)
                .1,
            preparing_members: registration.preparing_members,
            retirement_requested: registry.retiring_plugins.contains(plugin_id),
            snapshot: policy.capacity,
        })
    }
}

impl VulcanPluginRuntimeManager {
    /// Observe every retained capacity and its mapping predecessor; return None before parent attachment.
    /// 观测全部保留容量及映射前驱；父级附加前返回 None。
    /// No policy file, plugin budget, capacity or Lua instance is created by this read.
    /// 此读取不创建策略文件、插件预算、容量或 Lua 实例。
    pub fn formal_capacities(
        &self,
    ) -> Result<Option<VulcanPluginFormalCapacityCatalog>, VulcanPluginRuntimeError> {
        // Preserve the actual owner across every separately atomic native query.
        // 在全部分别原子的原生查询期间保留实际所有者。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // One metadata lock freezes mapping membership without freezing live native execution.
        // 单个元数据锁冻结映射成员，不冻结活动原生执行。
        let registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // Each row derives from the same projection used by exact-purpose queries.
        // 每行来自精确用途查询使用的同一投影。
        let capacities = registry
            .registrations
            .iter()
            .map(|((plugin, purpose), registration)| {
                owner.capacity_status_locked(&registry, plugin, *purpose, registration)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(VulcanPluginFormalCapacityCatalog {
            runtime_id: owner.host.runtime().id().to_owned(),
            registry_revision: registry.revision.clone(),
            capacities,
        }))
    }

    /// Find registration_id in runtime_id without writes; return None after its original capacity was forgotten.
    /// 在 runtime_id 中无写入查找 registration_id；原容量遗忘后返回 None。
    /// Foreign parents are rejected rather than searching another lifetime for a matching caller identity.
    /// 拒绝外来父级，不在另一寿命内寻找匹配的调用方身份。
    pub fn capacity_registration_status(
        &self,
        runtime_id: &str,
        registration_id: &str,
    ) -> Result<Option<VulcanPluginFormalCapacityStatus>, VulcanPluginRuntimeError> {
        // Missing parent has no retained registration receipt.
        // 缺失父级没有保留登记回执。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        if owner.host.runtime().id() != runtime_id {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity registration belongs to another runtime.",
            ));
        }
        // Receipt count is bounded by actual retained capacity metadata, never by unbounded request history.
        // 回执数量受真实保留容量元数据约束，绝不受无界请求历史支配。
        let registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        registry
            .registrations
            .iter()
            .find(|(_, registration)| {
                registration
                    .management_request
                    .as_ref()
                    .is_some_and(|request| request.registration_id == registration_id)
            })
            .map(|((plugin, purpose), registration)| {
                owner.capacity_status_locked(&registry, plugin, *purpose, registration)
            })
            .transpose()
    }

    /// Register request's previously absent purpose without executing Lua; return the original live capacity receipt.
    /// 不执行 Lua 地登记 request 此前缺失的用途；返回原活动容量回执。
    /// Exact repeats observe retained ownership; changed identities or stale mapping predecessors fail without replacement.
    /// 精确重复仅观测保留归属；改变身份或陈旧映射前驱会失败，不进行替换。
    pub fn register_formal_capacity(
        &self,
        request: VulcanPluginCapacityRegistrationRequest,
    ) -> Result<VulcanPluginFormalCapacityStatus, VulcanPluginRuntimeError> {
        uuid::Uuid::parse_str(&request.registration_id).map_err(|_| {
            VulcanPluginRuntimeError::new("Capacity registration_id must be a UUID.")
        })?;
        crate::catalog::validate_identifier(&request.plugin_id, "capacity registration owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // Publication serializes catalog ownership, policy transactions and every metadata mutation.
        // 发布门串行化目录归属、策略事务及每个元数据变更。
        let publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        // Administrative control never constructs an implicit parent.
        // 管理控制绝不构造隐式父级。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        if request.runtime_id != owner.host.runtime().id() {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity registration belongs to another runtime.",
            ));
        }
        // Keep exact native success and its caller receipt in the existing capacity map.
        // 在既有容量映射中保留精确原生成功及调用方回执。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        if let Some(((plugin, purpose), registration)) =
            registry.registrations.iter().find(|(_, registration)| {
                registration
                    .management_request
                    .as_ref()
                    .is_some_and(|original| original.registration_id == request.registration_id)
            })
        {
            if registration.management_request.as_ref() != Some(&request) {
                return Err(VulcanPluginRuntimeError::new(
                    "Capacity registration identity was reused for a different request.",
                ));
            }
            return owner.capacity_status_locked(&registry, plugin, *purpose, registration);
        }
        if request.expected_registry_revision != registry.revision {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity registry predecessor is stale; observe current ownership before a new explicit request.",
            ));
        }
        // The observed absence is required even when the installed plugin has never executed business.
        // 即使已安装插件从未执行业务，也必须满足所观测缺失。
        let key = (request.plugin_id.clone(), request.purpose);
        if registry.registrations.contains_key(&key) {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity purpose already has an original owner.",
            ));
        }
        registry.require_policy_admission(&request.plugin_id, request.purpose)?;
        if registry.retiring_plugins.contains(&request.plugin_id) {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin capacity lifecycle retirement is unfinished.",
            ));
        }
        // Structural ownership is mandatory; current business requirements may be the reason for this repair.
        // 结构归属为必需；当前业务要求可能正是此次修复的原因。
        let catalog = self.snapshot();
        // Resolve one installed owner from the same publication-protected catalog.
        // 从同一受发布门保护的目录解析单个已安装所有者。
        let plugin = catalog.plugins.get(&request.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Capacity registration requires an installed plugin.")
        })?;
        // Reject malformed packages before allocating native metadata.
        // 分配原生元数据前拒绝畸形插件包。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Capacity registration requires a valid plugin manifest.")
        })?;
        crate::runtime::lease::required_lua_runtime(manifest)?;
        owner.ensure_plugin_budget(&request.plugin_id, &publication)?;
        // The existing validated resolver owns all registration limits; management does not invent defaults.
        // 既有已校验解析器拥有全部登记上限；管理不虚构默认值。
        let config = registry
            .policy
            .settings
            .capacity_budget(&request.plugin_id, request.purpose)
            .0;
        // Only the actual core return value may become a managed capacity identity.
        // 仅实际核心返回值可成为受管容量身份。
        let identity = owner
            .host
            .runtime()
            .register_capacity(&request.plugin_id, config)
            .map_err(calls::core_error)?;
        registry.insert_registration(
            key.clone(),
            CapacityRegistration {
                identity,
                preparing_members: 0,
                management_request: Some(request),
            },
        );
        // Retain actual success before any fallible observation so a lost reply remains queryable.
        // 在任何可能失败的观测前保留真实成功，使丢失应答仍可查询。
        owner.capacity_status_locked(
            &registry,
            &key.0,
            key.1,
            registry
                .registrations
                .get(&key)
                .expect("native registration retained"),
        )
    }
}

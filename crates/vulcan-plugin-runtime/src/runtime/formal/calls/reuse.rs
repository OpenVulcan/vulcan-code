//! Bounded ownership of reusable tool domains; LuaSkills remains the sole VM scheduler.
//! 可复用工具域的有界所有权；LuaSkills 仍为唯一 VM 调度器。

use super::*;
use crate::VulcanPluginReusableToolManifest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod maintenance;
#[cfg(test)]
mod tests;

/// One exact cache partition; configuration fingerprints are private and never projected in status.
/// 一个精确缓存分区；配置指纹为私有数据，绝不投影到状态。
#[derive(Clone, PartialEq, Eq)]
struct DomainKey {
    /// Catalog generation owns package and permission identity.
    /// 目录代次拥有包及权限身份。
    generation: u64,
    /// Controller and workspace cannot share mutable Lua state with another owner.
    /// 控制器及工作区不能与其他所有者共享可变 Lua 状态。
    controller: String,
    /// The manifest entry selects private hygiene and scheduling policy.
    /// 清单入口选择私有清理及调度策略。
    tool: String,
    /// Canonical workspace spelling is part of the original native execution identity.
    /// 规范化工作区拼写属于原原生执行身份。
    workspace: PathBuf,
    /// Hash of validated configuration, or None for packages without configuration.
    /// 已校验配置的哈希；无配置插件包为 None。
    configuration: Option<[u8; 32]>,
    /// Core-owned capability registration revision captured before pool registration.
    /// 池注册前捕获的核心能力注册修订号。
    capabilities: String,
}

/// Metadata counts actual host borrows, never physical VM or scheduler permits.
/// 元数据计数真实宿主借用，绝不计数物理 VM 或调度许可。
struct Domain {
    /// Retained immutable descriptor keeps the original package alive during idle and closing.
    /// 保留的不可变描述符在空闲及关闭期间保持原包存活。
    plugin: VulcanPluginDescriptor,
    /// Original exact partition remains unchanged throughout the domain lifetime.
    /// 原精确分区在域寿命期间不变。
    key: DomainKey,
    /// Registration publishes exactly one core identity; None also represents completed pool retirement.
    /// 注册恰好发布一个核心身份；None 同时表示池退役已完成。
    pool: Option<String>,
    /// Physical pool forgetting is separate from acknowledgement of its finalization records.
    /// 物理池遗忘与其关闭记录确认分开。
    pool_retired: bool,
    /// Successful independent shutdown records remain owned until core acknowledgement succeeds.
    /// 独立成功关闭记录在核心确认成功前仍具有归属。
    pending_finalizations: bool,
    /// Actual registration must exit before physical cleanup can inspect the pool.
    /// 实际注册必须退出，物理清理才可检查池。
    preparing: bool,
    /// Every attached call retains its borrow until terminal request cleanup.
    /// 每个附加调用保留借用，直到终态请求清理。
    borrowers: usize,
    /// Closing rejects all new borrows, including while original registration is still running.
    /// 关闭拒绝全部新借用，包括原注册仍在运行时。
    closing: bool,
    /// Last returned borrow starts idle metadata retirement.
    /// 最后归还的借用启动空闲元数据退役。
    idle_since: Instant,
    /// Explicit manifest idle duration, shared with the original core pool policy.
    /// 显式清单空闲时长，与原核心池策略共用。
    idle_ttl: Duration,
    /// Independent failed finalizations remain in the core until explicit owner acknowledgement.
    /// 独立失败关闭保留在核心中，直到所有者明确确认。
    failures: BTreeSet<String>,
}

/// One manager-local registry bounds pending, active, draining and unacknowledged domains together.
/// 单个管理器局部注册表共同约束准备、活跃、排空及未确认域。
#[derive(Default)]
pub(in crate::runtime::formal) struct Registry {
    /// Stable host identities own original pool registration and cleanup evidence.
    /// 稳定宿主身份拥有原池注册及清理证据。
    domains: BTreeMap<String, Domain>,
    /// Permanent parent closure fences future domain preparation.
    /// 永久父级关闭阻止后续域准备。
    closing: bool,
    /// A failed observer keeps its exact recovery error visible until a successful maintenance pass.
    /// 失败观测者保留精确恢复错误，直到维护成功。
    observer_error: Option<String>,
}

impl Registry {
    /// Return whether no domain preparation, physical resource, or unacknowledged failure remains.
    /// 返回是否不再残留域准备、物理资源或未确认失败。
    pub(in crate::runtime::formal) fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

/// Public observation uses original core identities without exposing configuration fingerprints.
/// 公开观测使用原核心身份，不暴露配置指纹。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginReusableDomainStatus {
    /// Stable host domain identifier used for explicit cleanup acknowledgement.
    /// 用于明确清理确认的稳定宿主域标识。
    pub domain_id: String,
    /// Exact package owner.
    /// 精确包所有者。
    pub plugin_id: String,
    /// Immutable catalog generation retained by the domain.
    /// 域保留的不可变目录代次。
    pub plugin_generation: u64,
    /// Original tool entry, distinct from its shared bundle dispatcher.
    /// 原工具入口，与其共享 bundle 分发器区分。
    pub tool_id: String,
    /// Original core pool identity, absent only before registration or after physical forgetting.
    /// 原核心池身份，仅在注册前或物理遗忘后不存在。
    pub pool_id: Option<String>,
    /// True only after the core confirms physical pool forgetting.
    /// 仅在核心确认物理池遗忘后为真。
    pub pool_retired: bool,
    /// Independent finalizations or their physical acknowledgement are still pending.
    /// 独立关闭或其物理确认仍未完成。
    pub pending_finalizations: bool,
    /// Registration is still physically preparing.
    /// 注册仍在实际准备中。
    pub preparing: bool,
    /// Actual in-flight host borrows, including request cleanup.
    /// 真实进行中的宿主借用，包含请求清理。
    pub borrowers: usize,
    /// No new borrow may enter a closing domain.
    /// 新借用不能进入关闭域。
    pub closing: bool,
    /// Exact independent closing failures from the core, retained until acknowledgement.
    /// 来自核心的精确独立关闭失败，保留至确认。
    pub finalization_failures: Vec<OperationSnapshot>,
    /// Exact maintenance error remains observable without hiding owned domain identities.
    /// 精确维护错误保持可观测，不隐藏所拥有域身份。
    pub observer_error: Option<String>,
}

/// Registration guard clears preparation on every normal or panic exit.
/// 注册守卫在每个正常或 panic 退出路径清除准备状态。
struct RegistrationGuard<'a> {
    /// Original registry owner remains live while the blocking preparation runs.
    /// 阻塞准备运行时原注册表所有者保持存活。
    owner: &'a ManagedFormalRuntime,
    /// The already-published call ID also identifies its newly created domain.
    /// 已发布调用标识同时标识其新建域。
    id: String,
}
impl Drop for RegistrationGuard<'_> {
    /// Clear actual registration ownership; failed unpublished pools remain explicitly closing.
    /// 清除实际注册所有权；失败且未发布的池保持明确关闭。
    fn drop(&mut self) {
        if let Ok(mut registry) = self.owner.reusable.lock()
            && let Some(domain) = registry.domains.get_mut(&self.id)
        {
            domain.preparing = false;
            if domain.pool.is_none() {
                domain.closing = true;
            }
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Capture the current private configuration identity for plugin, with no persistent writes or public digest.
    /// 捕获 plugin 当前私有配置身份，不持久写入，不公开摘要。
    /// Returns None only for an explicitly absent manifest configuration declaration.
    /// 仅在清单明确无配置声明时返回 None。
    fn reusable_configuration(
        &self,
        plugin: &VulcanPluginDescriptor,
    ) -> Result<Option<[u8; 32]>, VulcanPluginRuntimeError> {
        // A validated descriptor is required even for a package without configuration.
        // 即使包没有配置，也要求已校验描述符。
        let manifest = plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Reusable plugin has no manifest."))?;
        if manifest.config.is_none() {
            return Ok(None);
        }
        // Hash only validated current state; native callbacks continue using their authoritative live read path.
        // 仅哈希已校验当前状态；原生回调继续使用其权威实时读取路径。
        let value = self.plugin_config_for_snapshot(plugin)?;
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        Ok(Some(Sha256::digest(bytes).into()))
    }

    /// Borrow or create one exact tool domain for call; return its original pool without running Lua.
    /// 为 call 借用或创建一个精确工具域；返回原池，不执行 Lua。
    /// Preparation remains attached to call across cancellation; duplicate preparations wait within the same deadline.
    /// 取消期间准备仍附加于 call；重复准备在同一截止时间内等待。
    pub(super) fn prepare_reusable_call(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        input: &mut Preparation,
        tool_id: &str,
        reuse: &VulcanPluginReusableToolManifest,
    ) -> Result<String, VulcanPluginRuntimeError> {
        // All production tool work supplies an explicit workspace and controller.
        // 全部生产工具工作提供明确工作区及控制器。
        let CallWorkspace::Provided(workspace) = &input.workspace else {
            return Err(VulcanPluginRuntimeError::new(
                "Reusable tool requires its exact workspace.",
            ));
        };
        let workspace = workspace.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve reusable workspace: {error}"))
        })?;
        let controller = call.ownership.controller_id().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable tool requires a controller owner.")
        })?;
        loop {
            if call.cancelled.load(Ordering::Acquire) || Instant::now() >= input.deadline {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable tool preparation cancelled or expired.",
                ));
            }
            // Lock order is publication, registry, call; Lua and package registration run after release.
            // 锁顺序为发布、注册表、调用；Lua 及包注册在释放后运行。
            let publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| calls_error())?;
            self.validate_formal_submission(call, input.scope)?;
            let key = DomainKey {
                generation: call.plugin.generation,
                controller: controller.to_owned(),
                tool: tool_id.to_owned(),
                workspace: workspace.clone(),
                configuration: self.reusable_configuration(&call.plugin)?,
                capabilities: owner
                    .host
                    .runtime()
                    .capabilities()
                    .snapshot()
                    .map_err(core_error)?
                    .revision(),
            };
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            if registry.closing {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain admission is closed.",
                ));
            }
            // An observed closing failure cannot be bypassed by minting a replacement for the same partition.
            // 不能通过为同一分区创建替代身份来绕过已观测关闭失败。
            if registry.domains.values().any(|domain| {
                domain.plugin.id == call.plugin.id
                    && domain.key == key
                    && !domain.failures.is_empty()
            }) {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain shutdown failure requires explicit acknowledgement.",
                ));
            }
            // Changed configuration, generation or capabilities never select old cached state.
            // 配置、代次或能力变化绝不选择旧缓存状态。
            for domain in registry
                .domains
                .values_mut()
                .filter(|domain| domain.plugin.id == call.plugin.id)
            {
                if domain.key.generation != key.generation
                    || domain.key.configuration != key.configuration
                    || domain.key.capabilities != key.capabilities
                {
                    domain.closing = true;
                }
            }
            if let Some((id, domain)) = registry.domains.iter_mut().find(|(_, domain)| {
                domain.plugin.id == call.plugin.id && domain.key == key && !domain.closing
            }) {
                if domain.preparing {
                    drop(registry);
                    drop(publication);
                    // The already-bounded registered blocking preparation owns this wait, never an untracked task.
                    // 已有界的已登记阻塞准备拥有此次等待，绝非未跟踪任务。
                    std::thread::sleep(
                        OBSERVATION_INTERVAL
                            .min(input.deadline.saturating_duration_since(Instant::now())),
                    );
                    continue;
                }
                let pool = domain.pool.clone().ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Reusable domain has no live pool.")
                })?;
                let mut state = call.lock()?;
                domain.borrowers = domain.borrowers.checked_add(1).ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Reusable domain borrow count is exhausted.")
                })?;
                state.reusable_domain = Some(id.clone());
                state.pool = Some(pool.clone());
                return Ok(pool);
            }
            // Pending registrations and failed evidence consume the same finite host metadata capacity.
            // 准备中的注册及失败证据消费同一有限宿主元数据容量。
            let plugin_limit = owner
                .settings
                .plugin_budget(&call.plugin.id)
                .0
                .max_registered_pools;
            if registry.domains.len() >= owner.settings.parent.max_registered_pools
                || registry
                    .domains
                    .values()
                    .filter(|domain| domain.plugin.id == call.plugin.id)
                    .count()
                    >= plugin_limit
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain metadata capacity is exhausted.",
                ));
            }
            let capacity = owner.admit_capacity_member(
                &call.plugin.id,
                VulcanPluginCapacityPurpose::ToolCall,
                &publication,
            )?;
            call.lock()?.reusable_domain = Some(call.id.clone());
            registry.domains.insert(
                call.id.clone(),
                Domain {
                    plugin: call.plugin.clone(),
                    key,
                    pool: None,
                    pool_retired: false,
                    pending_finalizations: false,
                    preparing: true,
                    borrowers: 1,
                    closing: false,
                    idle_since: Instant::now(),
                    idle_ttl: Duration::from_millis(reuse.idle_ttl_ms),
                    failures: BTreeSet::new(),
                },
            );
            drop(registry);
            drop(publication);
            // Guard preserves cleanup ownership if registration errors or its worker unwinds.
            // 注册错误或工作者展开时，守卫保留清理归属。
            let _registration = RegistrationGuard {
                owner,
                id: call.id.clone(),
            };
            let pool = owner
                .host
                .register_reusable_bundle_pool_in_capacity(
                    &capacity.capacity_id,
                    VulcanPluginBundlePoolRequest {
                        plugin_id: call.plugin.id.clone(),
                        plugin_generation: call.plugin.generation,
                        workspace_root: workspace,
                        security_partition: input.security_partition.clone(),
                        scope: input.scope,
                        finalizer: input.finalizer.take(),
                        policy: PluginPoolConfig {
                            kind: capacity.config.resources.kind,
                            min_resident_vms: 0,
                            max_resident_vms: reuse.max_resident_vms,
                            max_running_calls: reuse.max_running_calls,
                            max_queued_calls: capacity.config.max_queued_calls,
                            reuse: InstanceReuse::Reusable,
                            serial: reuse.ordering == crate::VulcanPluginToolOrdering::KeySerial,
                            backend: VULCAN_PLUGIN_EXECUTION_BACKEND,
                            idle_ttl_ms: Some(reuse.idle_ttl_ms),
                            max_uses: Some(reuse.max_uses),
                        },
                    },
                    tool_id,
                )
                .map_err(core_error)?;
            // Publish native ownership even if a concurrent drain has already requested closure.
            // 即使并发排空已请求关闭，也发布原生归属。
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            let domain = registry.domains.get_mut(&call.id).ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Reusable domain disappeared before pool publication.",
                )
            })?;
            domain.pool = Some(pool.clone());
            call.lock()?.pool = Some(pool.clone());
            return Ok(pool);
        }
    }

    /// Recheck a retained borrow at the final core submission boundary under caller-held publication.
    /// 在调用方持有发布门的最终核心提交边界复核保留借用。
    /// Changes close the original domain and fail this admission without substituting another pool.
    /// 变化关闭原域并使此次入场失败，不替换为其他池。
    pub(super) fn validate_reusable_submission(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let id = call.lock()?.reusable_domain.clone();
        let Some(id) = id else {
            return Ok(());
        };
        let configuration = self.reusable_configuration(&call.plugin)?;
        let capabilities = owner
            .host
            .runtime()
            .capabilities()
            .snapshot()
            .map_err(core_error)?
            .revision();
        let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
        let domain = registry.domains.get_mut(&id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable domain disappeared before submission.")
        })?;
        if domain.closing
            || domain.key.configuration != configuration
            || domain.key.capabilities != capabilities
        {
            domain.closing = true;
            return Err(VulcanPluginRuntimeError::new(
                "Reusable domain authority changed before submission.",
            ));
        }
        Ok(())
    }

    /// Release call's exact borrow after terminal cleanup; return whether it owned reusable pool metadata.
    /// 终态清理后释放 call 的精确借用；返回其是否拥有可复用池元数据。
    pub(super) fn release_reusable_call(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let id = call.lock()?.reusable_domain.clone();
        let Some(id) = id else {
            return Ok(false);
        };
        let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
        let domain = registry.domains.get_mut(&id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable domain disappeared before borrow cleanup.")
        })?;
        domain.borrowers = domain.borrowers.checked_sub(1).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable domain borrow ownership is inconsistent.")
        })?;
        domain.idle_since = Instant::now();
        let mut state = call.lock()?;
        state.reusable_domain = None;
        state.pool = None;
        Ok(true)
    }
}

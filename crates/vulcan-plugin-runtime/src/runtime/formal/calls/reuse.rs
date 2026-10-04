//! Bounded ownership of reusable tool domains; LuaSkills remains the sole VM scheduler.
//! 可复用工具域的有界所有权；LuaSkills 仍为唯一 VM 调度器。

use super::*;
use crate::VulcanPluginReusableToolManifest;
use sha2::{Digest, Sha256};

pub(in crate::runtime::formal) mod diagnostics;
mod maintenance;
#[cfg(test)]
mod tests;

/// One exact cache partition; configuration fingerprints are private and never projected in status.
/// 一个精确缓存分区；配置指纹为私有数据，绝不投影到状态。
#[derive(Clone, PartialEq, Eq, Serialize)]
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
    /// Native capacity policy revision selects fresh bindings without rewriting original module state.
    /// 原生容量策略修订选择新绑定，不重写原模块状态。
    capacity_policy: String,
}

/// Metadata counts actual host borrows, never physical VM or scheduler permits.
/// 元数据计数真实宿主借用，绝不计数物理 VM 或调度许可。
struct Domain {
    /// Exact plugin identity remains available after its catalog descriptor is replaced.
    /// 目录描述符替换后精确插件身份仍然可用。
    plugin_id: String,
    /// The real immutable package lease survives idle time and unacknowledged cleanup evidence.
    /// 真实不可变包租约在空闲和未确认清理证据期间保持存活。
    _managed_package: Option<crate::VulcanPluginManagedPackage>,
    /// Serialized retained routing input shares the parent budget with calls and pinned sessions.
    /// 序列化保留路由输入与调用及固定会话共享父级预算。
    input_bytes: usize,
    /// Original exact partition remains unchanged throughout the domain lifetime.
    /// 原精确分区在域寿命期间不变。
    key: DomainKey,
    /// Registration publishes one core identity retained even after physical pool retirement.
    /// 注册发布一个核心身份，在物理池退役后仍保留。
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
    /// At most one explicit target may initialize this exact domain at a time.
    /// 同时至多一个明确目标可以初始化此精确域。
    prewarm_owner: Option<String>,
    /// Closing rejects all new borrows, including while original registration is still running.
    /// 关闭拒绝全部新借用，包括原注册仍在运行时。
    closing: bool,
    /// Last returned borrow starts idle metadata retirement.
    /// 最后归还的借用启动空闲元数据退役。
    idle_since: Instant,
    /// Explicit manifest idle duration, shared with the original core pool policy.
    /// 显式清单空闲时长，与原核心池策略共用。
    idle_ttl: Duration,
    /// Core failure IDs map to successful host diagnostic publication; false remains pending across retries.
    /// 核心失败标识映射至宿主诊断发布成功状态；false 在重试期间保持待发布。
    failures: BTreeMap<String, bool>,
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
    /// Sum every pending, live and retiring domain reservation, returning an error on overflow.
    /// 汇总所有准备、存活和退役域预留；溢出返回错误。
    pub(in crate::runtime::formal) fn retained_input_bytes(
        &self,
    ) -> Result<usize, VulcanPluginRuntimeError> {
        self.domains
            .values()
            .map(|domain| domain.input_bytes)
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Reusable routing input size overflow."))
    }

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
    /// Serialized retained routing bytes remain reserved until this exact domain is removed.
    /// 序列化保留路由字节在此精确域移除前持续预留。
    pub retained_input_bytes: usize,
    /// Original core pool identity, absent only when registration has not published a pool.
    /// 原核心池身份，仅在注册尚未发布池时不存在。
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
    /// Claim the existing borrowed domain for call; reject a different target without changing its ownership.
    /// 为 call 声明既有借用域；拒绝不同目标，不改变其归属。
    /// Owner supplies the sole domain registry; ordinary business remains admitted by the native scheduler.
    /// owner 提供唯一域注册表；普通业务继续由原生调度器接纳。
    pub(super) fn claim_prewarm_domain(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Capture the exact borrowed domain ID before locking its registry.
        // 在锁定注册表前捕获精确借用域标识。
        let id = call.lock()?.reusable_domain.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Prewarm target has no reusable domain.")
        })?;
        // Serialize the target claim with other borrowers and domain cleanup.
        // 将目标声明与其他借用者及域清理排序。
        let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
        // The existing domain retains the sole target claim until borrow cleanup.
        // 既有域保留唯一目标声明，直到借用清理。
        let domain = registry
            .domains
            .get_mut(&id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Prewarm target domain was not found."))?;
        if domain
            .prewarm_owner
            .as_deref()
            .is_some_and(|id| id != call.id)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Another prewarm target owns this domain.",
            ));
        }
        domain.prewarm_owner = Some(call.id.clone());
        Ok(())
    }

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
            // Lock order is publication, calls, pinned sessions, reusable domains, then call state.
            // 锁顺序为发布、调用表、固定会话、复用域，最后为调用状态。
            let publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| calls_error())?;
            self.validate_formal_submission(call, input.scope)?;
            // One immutable snapshot supplies both the partition key and the registered native binding.
            // 单个不可变快照同时提供分区键和所注册原生绑定。
            let capability_snapshot = owner
                .host
                .runtime()
                .capabilities()
                .snapshot()
                .map_err(core_error)?;
            // A policy claim spans both cached selection and any real registration; updates wait for it to return.
            // 策略声明覆盖缓存选择及实际注册；更新等待其归还。
            let capacity = owner.admit_capacity_member(
                &call.plugin.id,
                VulcanPluginCapacityPurpose::ToolCall,
                &publication,
            )?;
            let key = DomainKey {
                generation: call.plugin.generation,
                controller: controller.to_owned(),
                tool: tool_id.to_owned(),
                workspace: workspace.clone(),
                configuration: self.reusable_configuration(&call.plugin)?,
                capabilities: capability_snapshot.revision(),
                capacity_policy: capacity.policy_revision.clone(),
            };
            // Call registry ownership serializes all increases to the shared retained-input budget.
            // 调用表所有权串行化共享保留输入预算的全部增加。
            let calls = owner.calls.lock().map_err(|_| calls_error())?;
            // Pinned admission also owns the call registry; removal can only decrease this sum.
            // 固定会话入场同样持有调用表；移除只会减小此总和。
            let other_input_bytes = calls
                .values()
                .map(|call| call.input_bytes.load(Ordering::Acquire))
                .chain(
                    owner
                        .pinned
                        .lock()
                        .map_err(|_| calls_error())?
                        .values()
                        .map(|session| session.input_bytes),
                )
                .try_fold(0usize, usize::checked_add)
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Formal retained input size overflow.")
                })?;
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            if registry.closing {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain admission is closed.",
                ));
            }
            // Failed maintenance fences fresh borrows until a complete pass verifies the original ownership.
            // 维护失败阻止新借用，直到完整轮次验证原归属。
            if let Some(error) = &registry.observer_error {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Reusable domain maintenance has not recovered: {error}"
                )));
            }
            // An observed closing failure cannot be bypassed by minting a replacement for the same partition.
            // 不能通过为同一分区创建替代身份来绕过已观测关闭失败。
            if registry.domains.values().any(|domain| {
                domain.plugin_id == call.plugin.id
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
                .filter(|domain| domain.plugin_id == call.plugin.id)
            {
                if domain.key.generation != key.generation
                    || domain.key.configuration != key.configuration
                    || domain.key.capabilities != key.capabilities
                    || domain.key.capacity_policy != key.capacity_policy
                {
                    domain.closing = true;
                }
            }
            if let Some((id, domain)) = registry.domains.iter_mut().find(|(_, domain)| {
                domain.plugin_id == call.plugin.id && domain.key == key && !domain.closing
            }) {
                if domain.preparing {
                    drop(registry);
                    drop(calls);
                    drop(capacity);
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
                    .filter(|domain| domain.plugin_id == call.plugin.id)
                    .count()
                    >= plugin_limit
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain metadata capacity is exhausted.",
                ));
            }
            // Reserve the exact privately serialized routing selection before native registration.
            // 原生注册前预留精确的私有序列化路由选择。
            let input_bytes = luaskills::runtime::embedded::json_size(
                &(&call.id, &call.plugin.id, &key),
                owner.settings.parent.max_queued_bytes,
            )
            .map_err(core_error)?;
            // Preserve one aggregate bound across every independent retained input owner.
            // 为每个独立保留输入所有者维护同一聚合边界。
            let total_bytes = registry
                .retained_input_bytes()?
                .checked_add(other_input_bytes)
                .and_then(|bytes| bytes.checked_add(input_bytes));
            if total_bytes.is_none_or(|bytes| bytes > owner.settings.parent.max_queued_bytes) {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable routing input byte capacity is exhausted.",
                ));
            }
            call.lock()?.reusable_domain = Some(call.id.clone());
            registry.domains.insert(
                call.id.clone(),
                Domain {
                    plugin_id: call.plugin.id.clone(),
                    _managed_package: call.plugin.managed_package.clone(),
                    input_bytes,
                    key,
                    pool: None,
                    pool_retired: false,
                    pending_finalizations: false,
                    preparing: true,
                    borrowers: 1,
                    prewarm_owner: None,
                    closing: false,
                    idle_since: Instant::now(),
                    idle_ttl: Duration::from_millis(reuse.idle_ttl_ms),
                    failures: BTreeMap::new(),
                },
            );
            drop(registry);
            drop(calls);
            drop(publication);
            // Guard preserves cleanup ownership if registration errors or its worker unwinds.
            // 注册错误或工作者展开时，守卫保留清理归属。
            let _registration = RegistrationGuard {
                owner,
                id: call.id.clone(),
            };
            let pool = owner
                .host
                .register_reusable_bundle_pool_with_snapshot_in_capacity(
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
                    capability_snapshot,
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
        // Final submission uses the same live policy authority as preparation, including unfinished-transaction fencing.
        // 最终提交使用与准备相同的活动策略权威，包含未完成事务屏障。
        let capacity_policy =
            owner.capacity_revision(&call.plugin.id, VulcanPluginCapacityPurpose::ToolCall)?;
        let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
        // A health failure discovered after borrowing also prevents final submission.
        // 借用后发现的健康失败同样阻止最终提交。
        if let Some(error) = &registry.observer_error {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Reusable domain maintenance has not recovered: {error}"
            )));
        }
        let domain = registry.domains.get_mut(&id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable domain disappeared before submission.")
        })?;
        if domain.closing
            || domain.key.configuration != configuration
            || domain.key.capabilities != capabilities
            || domain.key.capacity_policy != capacity_policy
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
        if domain.prewarm_owner.as_deref() == Some(call.id.as_str()) {
            domain.prewarm_owner = None;
        }
        let mut state = call.lock()?;
        state.reusable_domain = None;
        state.pool = None;
        Ok(true)
    }
}

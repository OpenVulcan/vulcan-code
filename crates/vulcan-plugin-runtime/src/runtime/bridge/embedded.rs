//! Runtime-owned native capabilities bound to exact plugin execution generations.
//! 绑定精确插件执行代次、由运行时拥有的原生能力。

use super::*;
use crate::VulcanPluginDescriptor;
use crate::runtime::candidates::{ActivationExecution, ActivationTargetHookAuthority};
use luaskills::runtime::embedded::capabilities::{
    CapabilityDescriptor, CapabilityEffects, CapabilityExecution, CapabilityIdempotency,
    CapabilityInvocation, CapabilityOutcome, CapabilityPermissions, CapabilityRegistrationRequest,
    CapabilityScope, CapabilitySnapshot, ModuleCapabilities,
};
use luaskills::runtime::embedded::{
    EffectState, EmbeddedError, EmbeddedErrorCode, EmbeddedResult, EmbeddedRuntime,
    EmbeddedRuntimeConfig, ModuleDefinition, ModuleResourceOwner, PluginPoolConfig,
};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

mod bundle;
mod dispatch;
mod requirements;
mod workspaces;
pub use bundle::VulcanPluginBundlePoolRequest;
use workspaces::WorkspaceRequests;

/// Exact native names installed atomically in each formal runtime.
/// 在每个正式运行时中原子安装的精确原生名称。
const CAPABILITIES: &[&str] = &[
    VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL,
    VULCAN_PLUGIN_CONFIG_GET_TOOL,
    VULCAN_PLUGIN_CONFIG_UPDATE_TOOL,
    VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL,
    VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL,
    VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL,
    VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL,
    VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL,
    VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL,
    VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL,
    VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL,
    VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL,
    VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL,
    VULCAN_VLDB_SQLITE_QUERY_V1_TOOL,
    VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL,
    VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL,
    VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL,
    VULCAN_PLUGIN_UI_INVALIDATE_TOOL,
];

/// Owns a formal runtime and installs this manager's native services without global dispatch.
/// 拥有正式运行时并安装此管理器的原生服务，不经过全局分发。
/// Hosts supply trusted module source and explicit budgets; this adapter does not sandbox source.
/// 宿主提供可信模块源码和显式预算；此适配器不对源码建立沙箱。
pub struct VulcanPluginEmbeddedHost {
    /// Core owns all execution, queue, session, capability and shutdown lifetimes.
    /// 核心拥有全部执行、队列、会话、能力及关闭寿命。
    runtime: EmbeddedRuntime,
    /// Native callbacks share identity lookup but never strongly own this runtime.
    /// 原生回调共享身份查询，但绝不强持有此运行时。
    authority: Arc<HostAuthority>,
}

/// Small runtime-local authority registry; weak entries cannot pin retired packages.
/// 运行时内的小型权威注册表；弱条目不能固定已退役包。
struct HostAuthority {
    /// Manager services remain externally owned to avoid a manager/runtime cycle.
    /// 管理器服务仍由外部持有，避免管理器与运行时循环引用。
    manager: Weak<VulcanPluginManagerState>,
    /// Opaque core namespace checked against every trusted invocation.
    /// 对每次可信调用检查的核心不透明命名空间。
    runtime_id: String,
    /// Capacity derives from the same core pool-registration budget.
    /// 容量派生自同一核心池注册预算。
    max_domains: usize,
    /// Exact execution revisions map to resources retained by actual core pools.
    /// 精确执行修订映射到真实核心池保留的资源。
    domains: Mutex<BTreeMap<String, Weak<ExecutionAuthority>>>,
    /// Request scopes retain exact operation observers, never the runtime or package owner.
    /// 请求作用域保留精确操作观察者，绝不保留运行时或包所有者。
    workspaces: Mutex<WorkspaceRequests>,
}

/// Immutable business authority retained by the core until real VM cleanup completes.
/// 核心保留至真实 VM 清理完成的不可变业务权威。
struct ExecutionAuthority {
    /// Cloning this descriptor retains its managed physical generation pin.
    /// 克隆此描述符会保留其受管物理代次固定句柄。
    plugin: VulcanPluginDescriptor,
    /// Exact core generation spelling used by this registration.
    /// 此注册使用的精确核心代次拼写。
    generation: String,
    /// Lifecycle transition that authorizes the callback.
    /// 授权回调的生命周期转换。
    scope: VulcanPluginBridgeLeaseScope,
    /// Canonical host workspace; business JSON cannot replace it.
    /// 规范宿主工作区；业务 JSON 不能替换它。
    workspace: PathBuf,
    /// Original trusted workspace spelling copied into the core caller context.
    /// 复制到核心调用上下文的原始可信工作区拼写。
    workspace_declaration: String,
    /// Host-assigned isolation partition, independently checked against the caller.
    /// 宿主分配的隔离分区，与调用方独立核对。
    security_partition: String,
    /// Exact draining generation granted by the manager; zero means no transition grant.
    /// 管理器授予的精确排空代次；零表示没有转换授权。
    closing_generation: AtomicU64,
    /// Actual core ownership retains the original update after observers or the manager disappear.
    /// 观察者或管理器消失后，实际核心所有权仍保留原更新。
    update_drain: Mutex<Option<Arc<crate::runtime::update_drain::UpdateDrainAuthority>>>,
    /// Only explicitly admitted unpublished operations carry this real transaction readiness owner.
    /// 仅明确接纳的未发布操作携带此实际事务就绪所有者。
    source: Mutex<ExecutionSource>,
    /// Physical environment retention is independent from transferable transaction authorization.
    /// 物理环境保留独立于可转交的事务授权。
    _environment: Option<std::fs::File>,
    /// Filled once from actual core registration before any session can initialize.
    /// 在任何会话初始化前，仅从实际核心注册填写一次。
    pool_id: std::sync::OnceLock<String>,
}

/// One captured source stays authoritative through both native admission and the effect boundary.
/// 单个捕获来源在原生入场及副作用边界两处均保持权威。
#[derive(Clone)]
enum ExecutionSource {
    /// Current published lifecycle and live manifest withdrawal authorize ordinary execution.
    /// 当前已发布生命周期及实时清单撤回授权普通执行。
    Published,
    /// Exact existing candidate or predecessor operations retain their original authority.
    /// 精确既有候选或前驱操作保留原权威。
    ActivationOperation(Arc<ActivationExecution>),
    /// Only original target initialization and its declared Hook finalizer may invoke native services.
    /// 仅原目标初始化及其声明 Hook 关闭器可以调用原生服务。
    PrivateTargetHook(Arc<ActivationTargetHookAuthority>),
}

impl ExecutionSource {
    /// Return the existing operation authority only for its explicit variant; all other sources remain distinct.
    /// 仅对显式变体返回既有操作权威；其他来源保持独立。
    fn activation(&self) -> Option<&ActivationExecution> {
        match self {
            Self::ActivationOperation(authority) => Some(authority),
            _ => None,
        }
    }
}

/// Restricted weak handle addresses the same original core pool and domain without extending physical ownership.
/// 受限弱句柄指向相同原核心池及域，不延长物理所有权。
pub(in crate::runtime) struct PrivateTargetHookHandoff {
    /// Core-issued pool identity is immutable throughout transfer.
    /// 核心签发池身份在转交全程保持不变。
    pub(in crate::runtime) pool_id: String,
    /// Only the actual core resource owner retains this domain strongly.
    /// 仅实际核心资源所有者强持有此域。
    domain: Weak<ExecutionAuthority>,
}

/// Prepared transfer retains the source guard until durable completion; applying it performs no I/O.
/// 已准备转交保留来源守卫至持久完成；应用时不执行 I/O。
pub(in crate::runtime) struct PrivateTargetHookTransfer<'a> {
    /// Caller retains the exact original domain while this short transfer guard exists.
    /// 此短转交守卫存在期间，调用方保留精确原域。
    source: std::sync::MutexGuard<'a, ExecutionSource>,
}

impl PrivateTargetHookTransfer<'_> {
    /// Replace only the authorization source after confirmed completion, releasing its original request owner.
    /// 已确认完成后仅替换授权来源，释放其原请求所有者。
    pub(in crate::runtime) fn publish(mut self) {
        *self.source = ExecutionSource::Published;
    }
}

impl PrivateTargetHookHandoff {
    /// Upgrade the exact physical domain; returns an error rather than creating another pool.
    /// 升级精确物理域；返回错误，绝不创建另一个池。
    pub(in crate::runtime) fn retain_domain(&self) -> EmbeddedResult<PrivateTargetHookDomain> {
        Ok(PrivateTargetHookDomain(self.domain.upgrade().ok_or_else(
            || {
                error(
                    EmbeddedErrorCode::Closed,
                    "Original target Hook domain has retired.",
                )
            },
        )?))
    }
}

/// Temporary exact domain retention permits prevalidated source transfer without exposing native registration.
/// 临时精确域保留允许预校验来源转交，不暴露原生注册。
pub(in crate::runtime) struct PrivateTargetHookDomain(Arc<ExecutionAuthority>);

impl PrivateTargetHookDomain {
    /// Lock only this original source after matching authority; return a guard for a later infallible publish.
    /// 匹配权威后仅锁定此原来源；返回供后续不可失败发布使用的守卫。
    pub(in crate::runtime) fn prepare_transfer(
        &self,
        authority: &Arc<ActivationTargetHookAuthority>,
    ) -> EmbeddedResult<PrivateTargetHookTransfer<'_>> {
        let source = self
            .0
            .source
            .lock()
            .map_err(|_| internal("Plugin execution source lock is poisoned."))?;
        if !matches!(&*source, ExecutionSource::PrivateTargetHook(original) if Arc::ptr_eq(original, authority))
        {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Target Hook source differs from its original authority.",
            ));
        }
        Ok(PrivateTargetHookTransfer { source })
    }
}

/// Internal registration result retains the actual domain only until the host records its weak handoff.
/// 内部注册结果仅在宿主记录弱交接前保留实际域。
struct RegisteredPool {
    /// Actual core-issued identity, never a host-generated replacement.
    /// 实际核心签发身份，绝非宿主生成替代值。
    id: String,
    /// Exact resource owner installed in that core pool.
    /// 安装于该核心池的精确资源所有者。
    domain: Arc<ExecutionAuthority>,
}

/// Host-selected lifecycle scope and optional private activation authority form one pool admission.
/// 宿主选择的生命周期作用域与可选原请求权威构成单个池入场。
struct PoolAdmission {
    /// Published routes require current lifecycle state; activation calls require their exact operation scope.
    /// 已发布路由要求当前生命周期状态；激活调用要求精确操作作用域。
    scope: VulcanPluginBridgeLeaseScope,
    /// Absence explicitly chooses ordinary published authority.
    /// 缺失明确选择普通已发布权威。
    source: ExecutionSource,
}

impl PoolAdmission {
    /// Construct ordinary published admission for scope without creating any activation authorization.
    /// 为 scope 构造普通已发布入场，不创建任何激活授权。
    fn published(scope: VulcanPluginBridgeLeaseScope) -> Self {
        Self {
            scope,
            source: ExecutionSource::Published,
        }
    }
}

/// Resolve plugin_id under admission's exact authority; activation resolution never changes the public catalog.
/// 按 admission 精确权威解析 plugin_id；激活解析绝不改变公开目录。
/// Returns a descriptor with real leases or an explicit ownership error before any executable source is read.
/// 在读取任何可执行源码前返回具有实际租约的描述符，或明确归属错误。
fn admitted_plugin(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    admission: &PoolAdmission,
) -> EmbeddedResult<VulcanPluginDescriptor> {
    if let ExecutionSource::PrivateTargetHook(authority) = &admission.source {
        if authority.plugin.id != plugin_id
            || admission.scope != VulcanPluginBridgeLeaseScope::Enabled
        {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Private target Hook pool identity differs from its authority.",
            ));
        }
        authority
            .validate(state, &authority.plugin)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        return Ok(authority.plugin.clone());
    }
    if let ExecutionSource::ActivationOperation(activation) = &admission.source {
        if activation.plugin.id != plugin_id {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Activation plugin identity differs from pool admission.",
            ));
        }
        activation
            .validate(state, &activation.plugin, admission.scope)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        return Ok(activation.plugin.clone());
    }
    state
        .snapshot
        .read()
        .map_err(|_| internal("Plugin catalog lock is poisoned."))?
        .plugins
        .get(plugin_id)
        .cloned()
        .ok_or_else(|| {
            error(
                EmbeddedErrorCode::StaleGeneration,
                "Plugin is no longer installed.",
            )
        })
}

impl VulcanPluginEmbeddedHost {
    /// Observe the actual domain pointer for pool_id in tests without extending its physical resource lifetime.
    /// 测试中观测 pool_id 实际域指针，不延长其物理资源寿命。
    #[cfg(test)]
    pub(in crate::runtime) fn target_test_domain_identity(&self, pool_id: &str) -> usize {
        let domains = self.authority.domains.lock().expect("test domains");
        let domain = domains
            .values()
            .filter_map(Weak::upgrade)
            .find(|domain| domain.pool_id.get().is_some_and(|id| id == pool_id))
            .expect("original test pool domain");
        Arc::as_ptr(&domain) as usize
    }

    /// Inspect actual weak domains for request_id; return true while any original activation execution survives.
    /// 检查 request_id 的实际弱域；任何原激活执行仍存续时返回 true。
    /// Logical call removal cannot substitute for physical native retirement.
    /// 逻辑调用移除不能替代物理原生退役。
    pub(in crate::runtime) fn retains_activation_request(
        &self,
        request_id: uuid::Uuid,
    ) -> EmbeddedResult<bool> {
        // Upgrade only during observation, without introducing another retained-owner registry.
        // 仅在观测期间升级引用，不引入另一份保留所有者注册表。
        let domains = self
            .authority
            .domains
            .lock()
            .map_err(|_| internal("Plugin authority lock is poisoned."))?;
        for domain in domains.values().filter_map(Weak::upgrade) {
            let source = domain
                .source
                .lock()
                .map_err(|_| internal("Plugin execution source lock is poisoned."))?;
            if match &*source {
                ExecutionSource::ActivationOperation(authority) => {
                    authority.request_id == request_id
                }
                ExecutionSource::PrivateTargetHook(authority) => authority.request_id == request_id,
                ExecutionSource::Published => false,
            } {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Check actual core owners retaining update; true means physical execution has not retired.
    /// 检查保留 update 的实际核心所有者；true 表示物理执行尚未退役。
    /// The caller excludes registration; this method neither closes pools nor upgrades manager ownership.
    /// 调用方排除注册；此方法不关闭池，也不升级管理器所有权。
    pub(in crate::runtime) fn retains_predecessor_drain(
        &self,
        update: &Arc<crate::runtime::update_drain::UpdateDrainAuthority>,
    ) -> EmbeddedResult<bool> {
        // Weak domain entries outlive logical session receipts without pinning completed workers.
        // 弱域条目寿命超过逻辑会话回执，但不会固定已完成工作者。
        let domains = self
            .authority
            .domains
            .lock()
            .map_err(|_| internal("Plugin authority lock is poisoned."))?;
        for entry in domains.values() {
            // Only the original actual core domain can keep this request busy.
            // 仅原实际核心域可以使此请求持续忙碌。
            let Some(domain) = entry.upgrade() else {
                continue;
            };
            if domain
                .update_drain
                .lock()
                .map_err(|_| internal("Predecessor drain ownership lock is poisoned."))?
                .as_ref()
                .is_some_and(|retained| Arc::ptr_eq(retained, update))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Grant finalization and retain an optional concrete update owner in each matching original core domain.
    /// 为每个匹配原核心域授予关闭权限，并保留可选具体更新所有者。
    /// The caller holds publication exclusion; return only after all live original domains receive the same authority.
    /// 调用方持有发布互斥；仅全部活动原域取得相同权威后返回。
    pub(in crate::runtime) fn authorize_draining_with_update(
        &self,
        plugin: &VulcanPluginDescriptor,
        transition_generation: u64,
        update: Option<&Arc<crate::runtime::update_drain::UpdateDrainAuthority>>,
    ) -> EmbeddedResult<()> {
        // A strictly newer generation prevents an old grant from becoming a future transition credential.
        // 严格更新的代次防止旧授权成为未来转换的凭据。
        if transition_generation <= plugin.generation {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Draining generation must advance.",
            ));
        }
        let domains = self
            .authority
            .domains
            .lock()
            .map_err(|_| internal("Plugin authority lock is poisoned."))?;
        for entry in domains.values() {
            // Upgrading an existing weak owner cannot create a domain or retain it beyond this publication.
            // 升级既有弱所有者不会创建执行域，也不会在本次发布之后额外保留它。
            let Some(domain) = entry.upgrade() else {
                continue;
            };
            if domain.scope == VulcanPluginBridgeLeaseScope::Enabled
                && domain.plugin.id == plugin.id
                && domain.plugin.generation == plugin.generation
                && domain.plugin.content_fingerprint == plugin.content_fingerprint
                && domain.plugin.package_root == plugin.package_root
            {
                if let Some(update) = update {
                    // Attach to actual core domain ownership before changing its finalizer grant.
                    // 改变关闭授权之前附加至实际核心域所有权。
                    let mut retained = domain
                        .update_drain
                        .lock()
                        .map_err(|_| internal("Predecessor drain ownership lock is poisoned."))?;
                    if retained
                        .as_ref()
                        .is_some_and(|original| !Arc::ptr_eq(original, update))
                    {
                        return Err(error(
                            EmbeddedErrorCode::StaleGeneration,
                            "Another update owns predecessor execution.",
                        ));
                    }
                    *retained = Some(Arc::clone(update));
                }
                domain
                    .closing_generation
                    .store(transition_generation, Ordering::Release);
            }
        }
        Ok(())
    }
    /// Grant only existing enabled domains from `plugin` access during `transition_generation` finalization.
    /// 仅授权 `plugin` 的既有启用域在 `transition_generation` 的关闭阶段访问能力。
    /// The manager must hold its publication gate; returns an error if the authority registry is unavailable.
    /// 管理器必须持有发布门；权威注册表不可用时返回错误。
    pub(crate) fn authorize_draining(
        &self,
        plugin: &VulcanPluginDescriptor,
        transition_generation: u64,
    ) -> EmbeddedResult<()> {
        self.authorize_draining_with_update(plugin, transition_generation, None)
    }

    /// Construct a runtime for `manager` with explicit parent and per-capability time budgets.
    /// 使用显式父级及单能力时间预算，为 `manager` 构造运行时。
    /// Return the owner after atomic capability registration, or a construction error.
    /// 在原子注册能力后返回所有者，失败则返回构造错误。
    pub fn new(
        manager: &VulcanPluginRuntimeManager,
        config: EmbeddedRuntimeConfig,
        max_capability_call_ms: u64,
    ) -> EmbeddedResult<Self> {
        // Validate before allocating any runtime-owned worker.
        // 在分配任何运行时工作线程前校验。
        config.validate()?;
        if max_capability_call_ms == 0 || config.max_registered_capabilities < CAPABILITIES.len() {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Native capability budgets are insufficient.",
            ));
        }
        // Use the already isolated immutable plugin engine.
        // 使用已经隔离的不可变插件引擎。
        let engine = manager
            .runtime_engine()
            .map_err(|failure| error(EmbeddedErrorCode::Closed, failure.message))?;
        // This core instance is the single owner of every plugin pool in this adapter.
        // 此核心实例是本适配器中全部插件池的唯一所有者。
        let runtime = EmbeddedRuntime::new(engine, config.clone())?;
        // Callback closures hold only this acyclic authority object.
        // 回调闭包仅持有此无环权威对象。
        let authority = Arc::new(HostAuthority {
            manager: Arc::downgrade(&manager.state),
            runtime_id: runtime.id().to_string(),
            max_domains: config.max_registered_pools,
            domains: Mutex::new(BTreeMap::new()),
            workspaces: Mutex::new(WorkspaceRequests::new(&config)),
        });
        // Publish the entire fixed native surface as one registry transaction.
        // 将完整固定原生接口作为一个注册表事务发布。
        let registrations = CAPABILITIES
            .iter()
            .map(|&name| {
                // Each callback captures only its fixed name and runtime-local authority.
                // 每个回调仅捕获固定名称及运行时内权威。
                let captured = Arc::clone(&authority);
                CapabilityRegistrationRequest {
                    descriptor: CapabilityDescriptor {
                        name: name.to_string(),
                        version: "1.0.0".to_string(),
                        description: format!(
                            "Execute {name} for the authenticated plugin generation."
                        ),
                        input_schema: json!({"type":"object"}),
                        output_schema: json!(true),
                        execution: CapabilityExecution::Native,
                        permissions: [name.to_string()].into(),
                        scope: CapabilityScope::Invocation,
                        max_concurrent: config.max_running_calls.min(config.max_host_requests),
                        max_call_ms: max_capability_call_ms,
                        max_input_bytes: config.max_value_bytes,
                        max_output_bytes: config.max_value_bytes,
                        effects: CapabilityEffects::Mutating,
                        idempotency: CapabilityIdempotency::None,
                    },
                    native: Some(Arc::new(move |invocation| {
                        captured.invoke(name, invocation)
                    })),
                }
            })
            .collect();
        runtime.capabilities().register(registrations)?;
        Ok(Self { runtime, authority })
    }

    /// Borrow the sole core owner for plugin budgets, calls, sessions, revocation and shutdown.
    /// 借用唯一核心所有者，管理插件预算、调用、会话、撤权及关闭。
    /// Pool source remains trusted host input; use this adapter's registration for native services.
    /// 池源码仍属于可信宿主输入；使用本适配器的注册入口获得原生服务。
    pub fn runtime(&self) -> &EmbeddedRuntime {
        &self.runtime
    }

    /// Register trusted definition and policy in scope with independent capacity; return the native pool identity.
    /// 在 scope 中以独立容量注册可信 definition 及 policy；返回原生池身份。
    /// The plugin budget must exist; catalog and capability checks are shared with capacity-bound registration.
    /// 插件预算必须已存在；目录及能力检查与绑定容量的注册共用。
    pub fn register_pool(
        &self,
        definition: ModuleDefinition,
        policy: PluginPoolConfig,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> EmbeddedResult<String> {
        self.register_pool_with_capacity(
            None,
            definition,
            policy,
            PoolAdmission::published(scope),
            None,
            None,
        )
        .map(|pool| pool.id)
    }

    /// Register definition and policy in scope as a member of the exact capacity_id; return the native pool identity.
    /// 在 scope 中将 definition 及 policy 注册为精确 capacity_id 的成员；返回原生池身份。
    /// Missing, closed or foreign ownership is rejected by the core without independent-placement fallback.
    /// 核心拒绝缺失、关闭或外来归属，不回退独立归属。
    pub fn register_pool_in_capacity(
        &self,
        capacity_id: &str,
        definition: ModuleDefinition,
        policy: PluginPoolConfig,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> EmbeddedResult<String> {
        self.register_pool_with_capacity(
            Some(capacity_id),
            definition,
            policy,
            PoolAdmission::published(scope),
            None,
            None,
        )
        .map(|pool| pool.id)
    }

    /// Register trusted `definition` and `policy` under the exact published or activation `admission`.
    /// 在精确已发布或激活 `admission` 下注册可信 `definition` 及 `policy`。
    /// Identity, generation, dependencies and package root must match the retained catalog entry.
    /// 身份、代次、依赖及包根必须匹配保留的目录条目。
    /// Returns a core pool ID; the host must first register the plugin's aggregate budget.
    /// 返回核心池 ID；宿主必须先注册插件聚合预算。
    /// capacity_id explicitly selects exact membership; None preserves independent placement.
    /// capacity_id 显式选择精确成员关系；None 保留独立归属。
    /// initialization_capabilities freezes a manifest-derived narrowing, independent of ordinary business grants.
    /// initialization_capabilities 冻结从清单派生的收窄集合，独立于普通业务授权。
    fn register_pool_with_capacity(
        &self,
        capacity_id: Option<&str>,
        definition: ModuleDefinition,
        policy: PluginPoolConfig,
        admission: PoolAdmission,
        capability_snapshot: Option<CapabilitySnapshot>,
        initialization_capabilities: Option<BTreeSet<String>>,
    ) -> EmbeddedResult<RegisteredPool> {
        // Scope and activation ownership were selected together by the trusted host route.
        // 作用域及激活所有权由可信宿主路由共同选择。
        let scope = admission.scope;
        // Upgrading the manager protects the short registration operation only.
        // 升级管理器仅保护此短注册操作。
        let state = self
            .authority
            .manager
            .upgrade()
            .ok_or_else(|| error(EmbeddedErrorCode::Closed, "Plugin manager is closed."))?;
        // Resolve the complete descriptor from the route's explicit authority before deriving permissions.
        // 在派生权限前，从路由显式权威解析完整描述符。
        let plugin = admitted_plugin(&state, &definition.plugin_id, &admission)?;
        // Exact dependencies come from the validated manifest, never a caller fallback.
        // 精确依赖来自已校验清单，绝不使用调用方回退值。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin has no valid manifest.",
            )
        })?;
        if matches!(admission.source, ExecutionSource::Published)
            && plugin.runtime_state != scope.required_runtime_state()
        {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Plugin lifecycle state does not authorize pool registration.",
            ));
        }
        if definition.generation != plugin.generation.to_string()
            || manifest.execution.dependencies.manifest.as_deref()
                != Some(definition.dependencies_file.as_str())
        {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Module generation or dependencies do not match the catalog.",
            ));
        }
        // Package materialization retains the existing host environment resolution contract.
        // 包物化保留现有宿主环境解析契约。
        let package = super::super::environment::plugin_execution_package_root(&state, &plugin)
            .map_err(|message| error(EmbeddedErrorCode::InvalidArgument, message))?;
        if definition.package_root != package.to_string_lossy() {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Module declaration does not match the exact catalog generation and package.",
            ));
        }
        // Workspaces are explicit and canonicalized once, outside plugin business arguments.
        // 工作区为显式值，在插件业务参数之外一次性规范化。
        let workspace = definition.workspace_root.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin host capabilities require an explicit workspace.",
            )
        })?;
        // Preserve the exact core declaration separately from its canonical process path.
        // 将精确核心声明与其规范进程路径分开保留。
        let workspace_declaration = workspace.clone();
        // Reject a missing or non-directory workspace before exposing host services.
        // 在暴露宿主服务前拒绝缺失或非目录工作区。
        let workspace = PathBuf::from(workspace)
            .canonicalize()
            .map_err(|failure| error(EmbeddedErrorCode::InvalidArgument, failure.to_string()))?;
        if !workspace.is_dir() {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin workspace is not a directory.",
            ));
        }
        // Each fixed capability requires its own revocable grant, derived from the manifest.
        // 每个固定能力要求其自身可撤销授权，该授权从清单派生。
        let permissions = requirements::permissions_for(manifest)?;
        // A supplied snapshot is the caller's exact partition authority; omission selects current membership.
        // 已提供快照是调用方精确分区权威；省略时选择当前成员。
        let snapshot = match capability_snapshot {
            Some(snapshot) => snapshot,
            None => self.runtime.capabilities().snapshot()?,
        };
        requirements::validate(manifest, scope, &snapshot, &permissions)?;
        // Revision is opaque and cannot be selected by Lua or reused by another registration.
        // 修订为不透明值，Lua 不能选择，其他注册不能复用。
        let revision = uuid::Uuid::new_v4().to_string();
        // Bind exactly what compatibility validation observed; the core must never reread a newer snapshot.
        // 精确绑定兼容性检查所观测内容；核心绝不能重新读取更新快照。
        let binding = ModuleCapabilities::new(snapshot, permissions, revision.clone())?;
        // Initialization uses the same immutable registration authority, narrowed before any VM exists.
        // 初始化使用相同不可变注册权威，在任何 VM 存在前完成收窄。
        let binding = match initialization_capabilities {
            Some(names) => binding.with_initialization_capabilities(names)?,
            None => binding,
        };
        // Actual core pool ownership pins the descriptor, not historical host handles.
        // 真实核心池所有权固定描述符，历史宿主句柄不固定它。
        let environment = match &admission.source {
            ExecutionSource::PrivateTargetHook(authority) => Some(
                authority
                    .take_environment()
                    .map_err(|failure| internal(&failure.message))?,
            ),
            _ => None,
        };
        let domain = Arc::new(ExecutionAuthority {
            plugin,
            generation: definition.generation.clone(),
            scope,
            workspace,
            workspace_declaration,
            security_partition: definition.security_partition.clone(),
            closing_generation: AtomicU64::new(0),
            update_drain: Mutex::new(None),
            source: Mutex::new(admission.source),
            _environment: environment,
            pool_id: std::sync::OnceLock::new(),
        });
        // Catalog publication cannot race the final authority check and pool registration.
        // 目录发布不能与最终权威检查及池注册竞争。
        let _publication = state
            .session_registration_gate
            .lock()
            .map_err(|_| internal("Plugin publication gate is poisoned."))?;
        // Direct native registration must not create an unowned lifecycle callback during an original update.
        // 直接原生注册不得在原更新期间创建无原请求归属的生命周期回调。
        let source = domain
            .source
            .lock()
            .map_err(|_| internal("Plugin execution source lock is poisoned."))?
            .clone();
        if matches!(source, ExecutionSource::Published) {
            crate::runtime::update_drain::validate_ordinary_lifecycle_admission(
                &state,
                &domain.plugin.id,
                scope,
            )
            .map_err(|failure| error(EmbeddedErrorCode::PermissionDenied, failure.message))?;
        }
        validate_current_source(&state, &domain, &source, false, None)?;
        {
            // Prune dead entries and reserve one bounded identity before core publication.
            // 在核心发布前清除死亡条目并预留一个有界身份。
            let mut domains = self
                .authority
                .domains
                .lock()
                .map_err(|_| internal("Plugin authority lock is poisoned."))?;
            domains.retain(|_, entry| entry.strong_count() != 0);
            if domains.len() >= self.authority.max_domains {
                return Err(error(
                    EmbeddedErrorCode::CapacityExceeded,
                    "Plugin authority capacity is exhausted.",
                ));
            }
            domains.insert(revision.clone(), Arc::downgrade(&domain));
        }
        // No callback can run during registration; initialization starts on later core admission.
        // 注册期间不会运行回调；初始化在后续核心入场时开始。
        let registered = self.runtime.register_pool_with_binding(
            definition,
            policy,
            binding,
            Some(ModuleResourceOwner::new(Arc::clone(&domain))),
            capacity_id,
        );
        if registered.is_err() {
            // A failed registration has no pool owner and must release its identity reservation.
            // 失败注册没有池所有者，必须释放其身份预留。
            self.authority
                .domains
                .lock()
                .map_err(|_| internal("Plugin authority lock is poisoned."))?
                .remove(&revision);
        }
        registered.map(|id| {
            // Initialization cannot begin until the caller receives this already-bound pool.
            // 调用方收到此已绑定池前，初始化不能开始。
            domain
                .pool_id
                .set(id.clone())
                .expect("new original domain has no pool identity");
            RegisteredPool { id, domain }
        })
    }
}

impl HostAuthority {
    /// Resolve `invocation` against this runtime and dispatch the exact fixed `name`.
    /// 针对此运行时解析 `invocation`，并分发精确固定的 `name`。
    /// Rejected admission reports no effect; opaque service execution conservatively stays unknown.
    /// 被拒入场报告尚无副作用；不透明服务执行保守保留未知状态。
    fn invoke(&self, name: &str, invocation: &CapabilityInvocation) -> CapabilityOutcome {
        // Admission errors never enter a host service.
        // 入场错误绝不进入宿主服务。
        let admitted = (|| {
            invocation.authorize()?;
            // Upgrade only the owner named by the authenticated core execution revision.
            // 仅升级由核心认证执行修订指向的所有者。
            let domain = self
                .domains
                .lock()
                .map_err(|_| internal("Plugin authority lock is poisoned."))?
                .get(&invocation.caller.execution_revision)
                .and_then(Weak::upgrade)
                .ok_or_else(|| {
                    error(
                        EmbeddedErrorCode::StaleGeneration,
                        "Plugin execution authority has retired.",
                    )
                })?;
            if invocation.caller.runtime_id != self.runtime_id
                || invocation.caller.plugin_id != domain.plugin.id
                || invocation.caller.package_generation != domain.generation
                || invocation.caller.security_partition != domain.security_partition
                || invocation.caller.workspace_root.as_deref()
                    != Some(domain.workspace_declaration.as_str())
            {
                return Err(error(
                    EmbeddedErrorCode::PermissionDenied,
                    "Plugin invocation authority does not match its registered domain.",
                ));
            }
            // The service owner is retained only for this admitted invocation.
            // 服务所有者仅为此已接纳调用保留。
            let state = self
                .manager
                .upgrade()
                .ok_or_else(|| error(EmbeddedErrorCode::Closed, "Plugin manager is closed."))?;
            // Capture provenance once; a concurrent handoff cannot change this invocation's second check.
            // 仅捕获一次来源；并发交接不能改变此调用的第二次校验。
            let source = domain
                .source
                .lock()
                .map_err(|_| internal("Plugin execution source lock is poisoned."))?
                .clone();
            validate_invocation_source(&state, &domain, &source, name, invocation)?;
            // Correlated calls require their exact admitted operation; absent correlation uses the module declaration.
            // 带关联调用要求精确已入场操作；无关联调用使用模块声明。
            let scope = self.workspace(&domain, invocation)?;
            // Capture the clock before sampling remaining budget to avoid extending the deadline.
            // 在采样剩余预算前读取时钟，避免延长截止时间。
            let now = Instant::now();
            // Millisecond flooring may shorten the budget but never grants additional time.
            // 毫秒向下取整可能缩短预算，但绝不授予额外时间。
            let deadline = now
                .checked_add(Duration::from_millis(invocation.budget.remaining_ms()))
                .ok_or_else(|| {
                    error(
                        EmbeddedErrorCode::InvalidArgument,
                        "Plugin host deadline is out of range.",
                    )
                })?;
            invocation.authorize()?;
            Ok((state, domain, source, scope, deadline))
        })();
        match admitted {
            Ok((state, domain, source, scope, deadline)) => {
                // Correlation was authenticated before attributing native execution time.
                // 原生执行计时归属前已认证关联。
                let _timer = crate::runtime::action_status::HostCallTimer::for_record(scope.action);
                dispatch::execute(
                    &state,
                    &domain,
                    &source,
                    &scope.workspace,
                    name,
                    invocation,
                    deadline,
                )
            }
            Err(failure) => CapabilityOutcome {
                result: Err(failure),
                effects: EffectState::NotStarted,
            },
        }
    }
}

/// Require `domain` to retain the exact installed bytes and authorized lifecycle state.
/// 要求 `domain` 保留精确已安装字节及已授权生命周期状态。
/// `finalizing` is core-owned evidence; return success only for current authority or its exact draining grant.
/// `finalizing` 为核心自有证据；仅当前权威或其精确排空授权返回成功。
/// capability is absent for registration; calls require the exact capability's current permission.
/// capability 在注册时缺省；调用要求精确能力的当前权限。
/// The catalog read lock ends before any external service or Lua code executes.
/// 目录读锁在任何外部服务或 Lua 代码执行前结束。
/// Validate the captured source against state; source remains fixed across one native invocation.
/// 针对 state 校验已捕获来源；来源在单次原生调用中保持固定。
fn validate_current_source(
    state: &VulcanPluginManagerState,
    domain: &ExecutionAuthority,
    source: &ExecutionSource,
    finalizing: bool,
    capability: Option<&str>,
) -> EmbeddedResult<()> {
    if let ExecutionSource::PrivateTargetHook(authority) = source {
        authority
            .validate(state, &domain.plugin)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        let manifest = domain
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| internal("Private target Hook manifest is unavailable."))?;
        if capability.is_some_and(|name| !permitted(name, &manifest.permissions)) {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Private target Hook manifest does not authorize this capability.",
            ));
        }
        return Ok(());
    }
    if let ExecutionSource::ActivationOperation(activation) = source {
        activation
            .validate(state, &domain.plugin, domain.scope)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        // The frozen operation manifest owns native permissions while original request ownership excludes withdrawal.
        // 原请求所有权排除事务撤回期间，固定操作清单拥有原生权限。
        let manifest = domain.plugin.manifest.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::PermissionDenied,
                "Activation operation manifest is unavailable.",
            )
        })?;
        if capability.is_some_and(|name| !permitted(name, &manifest.permissions)) {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Activation operation manifest does not authorize this native capability.",
            ));
        }
        return Ok(());
    }
    // Ordinary native admission follows the same selected-target handoff fence as formal calls.
    // 普通原生入场遵循与正式调用相同的已选目标交接屏障。
    crate::runtime::update_drain::validate_ordinary_lifecycle_admission(
        state,
        &domain.plugin.id,
        domain.scope,
    )
    .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
    // One immutable snapshot binds the package and lifecycle transition to the same observation.
    // 单个不可变快照将包与生命周期转换绑定至同一次观测。
    let snapshot = state
        .snapshot
        .read()
        .map_err(|_| internal("Plugin catalog lock is poisoned."))?;
    let current = snapshot.plugins.get(&domain.plugin.id).ok_or_else(|| {
        error(
            EmbeddedErrorCode::StaleGeneration,
            "Plugin is no longer installed.",
        )
    })?;
    if current.content_fingerprint != domain.plugin.content_fingerprint
        || current.package_root != domain.plugin.package_root
    {
        return Err(error(
            EmbeddedErrorCode::StaleGeneration,
            "Plugin content changed before host admission.",
        ));
    }
    // The frozen core grant is an upper bound; current manifest withdrawal still removes access.
    // 冻结核心授权只是上界；当前清单撤回仍会移除访问权。
    if let Some(name) = capability {
        // Check current authority under the same catalog observation as identity and lifecycle.
        // 在与身份及生命周期相同的目录观测下检查当前权威。
        let manifest = current.manifest.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::PermissionDenied,
                "Plugin current manifest is unavailable.",
            )
        })?;
        if !permitted(name, &manifest.permissions) {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                format!("Plugin current manifest no longer authorizes native capability: {name}."),
            ));
        }
    }
    // Only trusted core finalization may use the manager's exact, still-current draining grant.
    // 仅可信核心关闭阶段可使用管理器精确且仍为当前代次的排空授权。
    if finalizing
        && current.runtime_state == crate::VulcanPluginRuntimeState::Draining
        && current.generation == domain.closing_generation.load(Ordering::Acquire)
    {
        return Ok(());
    }
    if current.generation != domain.plugin.generation {
        return Err(error(
            EmbeddedErrorCode::StaleGeneration,
            "Plugin generation changed before host admission.",
        ));
    }
    if current.runtime_state != domain.scope.required_runtime_state() {
        return Err(error(
            EmbeddedErrorCode::PermissionDenied,
            "Plugin lifecycle state does not authorize this execution scope.",
        ));
    }
    Ok(())
}

/// Validate `invocation` against `domain` using only core-owned stage evidence and current host state.
/// 仅使用核心自有阶段证据及当前宿主状态，校验 `invocation` 与 `domain`。
/// name selects the fixed native capability whose current authorization must remain valid.
/// name 选择当前授权必须保持有效的固定原生能力。
/// Returns the original identity failure; this never substitutes for `invocation.authorize()`.
/// 返回原始身份失败；这绝不替代 `invocation.authorize()`。
/// Require the captured private source to match actual core initialization or declared finalization evidence.
/// 要求已捕获私有来源匹配实际核心初始化或声明关闭证据。
/// state and domain identify one original pool; name and invocation retain all live permission and budget checks.
/// state 与 domain 标识单个原池；name 及 invocation 保留全部实时权限与预算校验。
fn validate_invocation_source(
    state: &Arc<VulcanPluginManagerState>,
    domain: &ExecutionAuthority,
    source: &ExecutionSource,
    name: &str,
    invocation: &CapabilityInvocation,
) -> EmbeddedResult<()> {
    if let ExecutionSource::PrivateTargetHook(_) = source {
        let owner = VulcanPluginRuntimeManager {
            state: Arc::clone(state),
        }
        .formal_runtime_owner()
        .map_err(|failure| internal(&failure.message))?
        .ok_or_else(|| internal("Formal plugin runtime is missing."))?;
        let session_id = invocation.caller.session_id.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::PermissionDenied,
                "Private target Hook requires its actual fixed session.",
            )
        })?;
        let session = owner.host.runtime().session(session_id)?;
        let finalizing = invocation.budget.is_finalization()?;
        if domain.pool_id.get() != Some(&session.pool_id)
            || session.active_operation.as_ref() != Some(&invocation.caller.operation_id)
            || (finalizing
                && (session.phase != luaskills::runtime::embedded::EmbeddedSessionPhase::Closing
                    || session.finalization_operation.as_ref()
                        != Some(&invocation.caller.operation_id)))
            || (!finalizing
                && session.phase != luaskills::runtime::embedded::EmbeddedSessionPhase::Opening)
        {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Private target Hook permits only actual initialization and declared finalization.",
            ));
        }
    }
    validate_current_source(
        state,
        domain,
        source,
        invocation.budget.is_finalization()?,
        Some(name),
    )
}

/// Check whether `permissions` authorize the fixed `name`; unknown names are never granted.
/// 检查 `permissions` 是否授权固定 `name`；未知名称绝不获授权。
/// Process resolution needs one execution permission, then validates the selected entry itself.
/// 进程解析需要一种执行权限，随后自行校验所选入口。
fn permitted(name: &str, permissions: &[VulcanPluginPermission]) -> bool {
    match name {
        VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL => [
            VulcanPluginPermission::ProcessBinary,
            VulcanPluginPermission::ProcessNode,
            VulcanPluginPermission::ProcessPython,
        ]
        .iter()
        .any(|permission| permissions.contains(permission)),
        VULCAN_PLUGIN_CONFIG_GET_TOOL | VULCAN_PLUGIN_CONFIG_UPDATE_TOOL => {
            permissions.contains(&VulcanPluginPermission::Config)
        }
        VULCAN_PLUGIN_UI_INVALIDATE_TOOL => {
            permissions.contains(&VulcanPluginPermission::UiHostContext)
        }
        _ => {
            if let Some(operation) = plugin_storage_operation(name) {
                return permissions.contains(&if operation.is_sqlite() {
                    VulcanPluginPermission::StorageSqlite
                } else {
                    VulcanPluginPermission::StorageLancedb
                });
            }
            plugin_vldb_operation(name).is_some()
                && permissions.contains(&VulcanPluginPermission::SystemVldbAccess)
        }
    }
}

/// Build a structured `code` and English `message` without interpreting business payloads.
/// 使用结构化 `code` 和英文 `message` 构造错误，不解释业务载荷。
fn error(code: EmbeddedErrorCode, message: impl Into<String>) -> EmbeddedError {
    EmbeddedError::new(code, message)
}

/// Report an internal host invariant failure using the exact diagnostic `message`.
/// 使用精确诊断 `message` 报告宿主内部不变量失败。
fn internal(message: &str) -> EmbeddedError {
    error(EmbeddedErrorCode::Internal, message)
}

#[cfg(test)]
mod tests;

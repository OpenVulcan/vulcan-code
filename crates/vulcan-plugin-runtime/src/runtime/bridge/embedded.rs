//! Runtime-owned native capabilities bound to exact plugin execution generations.
//! 绑定精确插件执行代次、由运行时拥有的原生能力。

use super::*;
use crate::VulcanPluginDescriptor;
use crate::runtime::candidates::CandidateExecution;
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
    /// Only explicitly admitted unpublished probes carry this real transaction readiness owner.
    /// 仅明确接纳的未发布探测携带此实际事务就绪所有者。
    candidate: Option<Arc<CandidateExecution>>,
}

/// Host-selected lifecycle scope and optional private candidate authority form one pool admission.
/// 宿主选择的生命周期作用域与可选私有候选权威构成单个池入场。
struct PoolAdmission {
    /// Published routes require their current lifecycle state; candidates accept only probe scope.
    /// 已发布路由要求其当前生命周期状态；候选仅接受探测作用域。
    scope: VulcanPluginBridgeLeaseScope,
    /// Absence explicitly chooses ordinary published authority.
    /// 缺失明确选择普通已发布权威。
    candidate: Option<Arc<CandidateExecution>>,
}

impl PoolAdmission {
    /// Construct ordinary published admission for scope without creating any candidate authorization.
    /// 为 scope 构造普通已发布入场，不创建任何候选授权。
    fn published(scope: VulcanPluginBridgeLeaseScope) -> Self {
        Self {
            scope,
            candidate: None,
        }
    }
}

/// Resolve plugin_id under admission's exact authority; candidate resolution never changes the public catalog.
/// 按 admission 精确权威解析 plugin_id；候选解析绝不改变公开目录。
/// Returns a descriptor with real leases or an explicit ownership error before any executable source is read.
/// 在读取任何可执行源码前返回具有实际租约的描述符，或明确归属错误。
fn admitted_plugin(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    admission: &PoolAdmission,
) -> EmbeddedResult<VulcanPluginDescriptor> {
    if let Some(candidate) = &admission.candidate {
        if candidate.plugin.id != plugin_id {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Candidate plugin identity differs from pool admission.",
            ));
        }
        candidate
            .validate(state, &candidate.plugin, admission.scope)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        return Ok(candidate.plugin.clone());
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
    /// Grant only existing enabled domains from `plugin` access during `transition_generation` finalization.
    /// 仅授权 `plugin` 的既有启用域在 `transition_generation` 的关闭阶段访问能力。
    /// The manager must hold its publication gate; returns an error if the authority registry is unavailable.
    /// 管理器必须持有发布门；权威注册表不可用时返回错误。
    pub(crate) fn authorize_draining(
        &self,
        plugin: &VulcanPluginDescriptor,
        transition_generation: u64,
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
                domain
                    .closing_generation
                    .store(transition_generation, Ordering::Release);
            }
        }
        Ok(())
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
    }

    /// Register trusted `definition` and `policy` under the exact published or candidate `admission`.
    /// 在精确已发布或候选 `admission` 下注册可信 `definition` 及 `policy`。
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
    ) -> EmbeddedResult<String> {
        // Scope and candidate ownership were selected together by the trusted host route.
        // 作用域及候选所有权由可信宿主路由共同选择。
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
        if admission.candidate.is_none() && plugin.runtime_state != scope.required_runtime_state() {
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
        let domain = Arc::new(ExecutionAuthority {
            plugin,
            generation: definition.generation.clone(),
            scope,
            workspace,
            workspace_declaration,
            security_partition: definition.security_partition.clone(),
            closing_generation: AtomicU64::new(0),
            candidate: admission.candidate,
        });
        // Catalog publication cannot race the final authority check and pool registration.
        // 目录发布不能与最终权威检查及池注册竞争。
        let _publication = state
            .session_registration_gate
            .lock()
            .map_err(|_| internal("Plugin publication gate is poisoned."))?;
        validate_current(&state, &domain, false, None)?;
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
            Some(ModuleResourceOwner::new(domain)),
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
        registered
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
            validate_invocation_current(&state, &domain, name, invocation)?;
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
            Ok((state, domain, scope, deadline))
        })();
        match admitted {
            Ok((state, domain, scope, deadline)) => {
                // Correlation was authenticated before attributing native execution time.
                // 原生执行计时归属前已认证关联。
                let _timer = crate::runtime::action_status::HostCallTimer::for_record(scope.action);
                dispatch::execute(
                    &state,
                    &domain,
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
fn validate_current(
    state: &VulcanPluginManagerState,
    domain: &ExecutionAuthority,
    finalizing: bool,
    capability: Option<&str>,
) -> EmbeddedResult<()> {
    if let Some(candidate) = &domain.candidate {
        candidate
            .validate(state, &domain.plugin, domain.scope)
            .map_err(|failure| error(EmbeddedErrorCode::StaleGeneration, failure.message))?;
        // The exact prepared manifest owns candidate permissions while original request ownership excludes withdrawal.
        // 原请求所有权排除事务撤回期间，精确准备清单拥有候选权限。
        let manifest = domain.plugin.manifest.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::PermissionDenied,
                "Candidate manifest is unavailable.",
            )
        })?;
        if capability.is_some_and(|name| !permitted(name, &manifest.permissions)) {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Candidate manifest does not authorize this native capability.",
            ));
        }
        return Ok(());
    }
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
fn validate_invocation_current(
    state: &VulcanPluginManagerState,
    domain: &ExecutionAuthority,
    name: &str,
    invocation: &CapabilityInvocation,
) -> EmbeddedResult<()> {
    validate_current(
        state,
        domain,
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

use super::*;

#[cfg(test)]
mod plugin_drain_tests;

mod candidate;
mod catalog_refresh;
mod cleanup_receipts;
#[cfg(test)]
mod launch_input_tests;
mod lifecycle_guard;
mod retained_replacement;
pub use retained_replacement::RetainedPluginMcpReplacement;
mod published;
pub use candidate::{CandidatePluginMcpObservation, CandidatePluginMcpRuntime};
pub use candidate::{PredecessorPluginMcpHandoffGuard, PredecessorPluginMcpRuntime};
pub use candidate::{TargetPluginMcpHandoffGuard, TargetPluginMcpRuntime};
use cleanup_receipts::{McpActualCleanupObservation, McpCleanupAdmission};
pub use published::{
    PreparedPublishedPluginMcpPublication, PublishedPluginMcpObservation,
    PublishedPluginMcpPublicationGuard, PublishedPluginMcpRuntime,
};

/// Explicit runtime intent controls retries without inferring authority from generation numbers.
/// 显式运行时用途控制重试，不从代次数值推断权威。
#[derive(Clone, Copy, PartialEq, Eq)]
enum McpRuntimePurpose {
    /// Published managed connections retain the ordinary bounded restart policy.
    /// 已发布托管连接保留普通有限重启策略。
    Managed,
    /// Unpublished readiness runs once and never automatically replays initialization.
    /// 未发布就绪仅运行一次，绝不自动重放初始化。
    CandidateReadiness,
    /// Actual selected target prepares once with business admission closed until confirmed handoff.
    /// 实际已选目标仅准备一次，业务入场在确认交接前保持关闭。
    TargetPreparation,
    /// Aborted old restoration starts once and stays closed until a distinct authorized transfer.
    /// 中止旧恢复仅启动一次，保持关闭至独立获授权转交。
    PredecessorPreparation,
    /// Proven ordinary preparation runs once with business access closed until full publication.
    /// 已证明普通准备仅运行一次，业务访问在完整发布前保持关闭。
    PublishedPreparation,
    /// Raw retained replacement remains single-shot and closed until full service publication.
    /// 原始保留替换保持单次尝试及关闭，直至完整服务发布。
    RetainedPluginReplacement,
}

/// Business admission for one actual target instance; a closed instance can never reopen.
/// 单个实际目标实例的业务入场；关闭实例绝不能重新开放。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum McpBusinessGate {
    /// Internal readiness may run while business access remains unavailable.
    /// 内部就绪可运行，但业务访问保持不可用。
    Preparing,
    /// Confirmed handoff grants access to the same original transport.
    /// 确认交接授权访问同一个原传输。
    Open,
    /// Cancellation, failed publication or drain permanently closes this original instance.
    /// 取消、发布失败或排空永久关闭此原实例。
    Closed,
}

impl McpRuntimePurpose {
    /// Returns the sole attempt bound for this explicitly declared runtime purpose.
    /// 返回此明确声明运行时用途的唯一尝试边界。
    /// No inputs; managed attempts retain the existing bound and candidates are single-shot.
    /// 无输入；托管尝试保留原上限，候选仅单次尝试。
    fn maximum_attempts(self) -> u8 {
        match self {
            Self::Managed => 5,
            Self::CandidateReadiness
            | Self::TargetPreparation
            | Self::PredecessorPreparation
            | Self::PublishedPreparation
            | Self::RetainedPluginReplacement => 1,
        }
    }
}

/// Creates one source-neutral runtime whose actual lifecycle tasks retain the supplied launch scope.
/// 创建实际生命周期任务保留给定启动范围的中立运行时。
/// Servers are immutable inputs, resources retains their host authority, and purpose fixes retry behavior.
/// servers 为不可变输入，resources 保留其宿主权威，purpose 固定重试行为。
/// Returns control state without retaining resources in historical snapshots; errors precede any task spawn.
/// 返回不在历史快照中保留资源的控制状态；错误发生在任何任务启动前。
fn create_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
    purpose: McpRuntimePurpose,
) -> Result<McpRuntimeHandle, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        purpose,
        crate::mcp::registry::McpPluginLaunchSource::Unproven,
    )
    .map(PreparedMcpRuntime::dispatch)
}

/// Undispatched work retains actual resources while original controls can be registered before any task exists.
/// 未派发工作保留实际资源，使原控制可在任何任务存在前登记。
pub(super) struct PreparedMcpRuntime {
    /// Source, diagnostic and shutdown state is allocated once without launching native work.
    /// 一次分配来源、诊断及关闭状态，不启动原生工作。
    runtime: McpRuntimeHandle,
    /// Only the preparation or actual tasks retain physical resource authority.
    /// 仅准备对象或实际任务保留物理资源权威。
    resources: vulcan_mcp_runtime::McpTransportResources,
}

impl PreparedMcpRuntime {
    /// Clone original control for registration; no inputs and no execution resources escape.
    /// 克隆原控制以供登记；无输入，不泄出执行资源。
    pub(super) fn handle(&self) -> McpRuntimeHandle {
        self.runtime.clone()
    }

    /// Dispatch the prepared tasks exactly once and return their already allocated original control.
    /// 恰好派发一次已准备任务，并返回其已分配原控制。
    /// No inputs; original registration must precede this consumption for sealed published and target starts.
    /// 无输入；对密封已发布及目标启动，原登记必须先于此消费。
    pub(super) fn dispatch(self) -> McpRuntimeHandle {
        for server in self.runtime.inner.registry.enabled_servers() {
            spawn_server_runtime_task(
                Arc::clone(&self.runtime.inner),
                server.clone(),
                self.resources.clone(),
            );
        }
        self.runtime
    }
}

/// Allocate immutable inputs and original control before work dispatch with the explicit purpose and source.
/// 使用显式 purpose 及 source，在工作派发前分配不可变输入及原控制。
/// servers define one registry and resources own its actual leases; return undispatched work or a pre-effect error.
/// servers 定义单个注册表，resources 拥有其实际租约；返回未派发工作或副作用前错误。
fn prepare_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
    purpose: McpRuntimePurpose,
    source: crate::mcp::registry::McpPluginLaunchSource,
) -> Result<PreparedMcpRuntime, String> {
    // Reject construction without an active runtime before spawning any lifecycle task.
    // 在启动任何生命周期任务前拒绝没有活动执行器的构造。
    tokio::runtime::Handle::try_current()
        .map_err(|_| String::from("MCP runtime creation requires an active Tokio runtime."))?;
    // A server identifier has exactly one lifecycle writer and one current launch receipt.
    // 服务器标识仅有一个生命周期写入者及一个当前启动回执。
    let mut server_ids = BTreeSet::new();
    for server in &servers {
        if !server_ids.insert(&server.id) {
            return Err(format!("Duplicate MCP server identifier `{}`.", server.id));
        }
    }
    // Keep a single immutable registry for every task in this runtime.
    // 为此运行时中的每个任务保留唯一不可变注册表。
    let registry = McpServerRegistry::new(servers);
    // Aggregate original lifecycle and transport ownership without retaining it in control handles.
    // 聚合原生命周期及传输所有权，不在控制句柄内保留它。
    let (resources, partition_release) = vulcan_mcp_runtime::McpTransportResources::new(resources);
    // Every task observes the same latched shutdown request.
    // 每个任务观察相同锁存关闭请求。
    let (shutdown_tx, _) = watch::channel(false);
    // Initialize diagnostic state without storing candidate authority in history.
    // 初始化诊断状态，不在历史中存储候选权威。
    let snapshot = McpRuntimeSnapshot {
        servers: registry
            .servers()
            .iter()
            .map(|server| (server.id.clone(), initial_server_snapshot(server)))
            .collect(),
    };
    // Shared control contains independent receipts rather than owning aggregate resources.
    // 共享控制包含独立回执，不拥有聚合资源。
    let inner = Arc::new(McpRuntimeInner {
        instance_id: uuid::Uuid::new_v4(),
        registry: registry.clone(),
        launch_source: source,
        purpose,
        business_gate: RwLock::new(match purpose {
            McpRuntimePurpose::TargetPreparation
            | McpRuntimePurpose::PredecessorPreparation
            | McpRuntimePurpose::PublishedPreparation
            | McpRuntimePurpose::RetainedPluginReplacement => McpBusinessGate::Preparing,
            McpRuntimePurpose::Managed | McpRuntimePurpose::CandidateReadiness => {
                McpBusinessGate::Open
            }
        }),
        transports: RwLock::new(BTreeMap::new()),
        launch_inputs: RwLock::new(BTreeMap::new()),
        cleanup_observations: RwLock::new(BTreeMap::new()),
        resource_releases: RwLock::new(BTreeMap::new()),
        partition_release,
        auth_sessions: AsyncRwLock::new(BTreeMap::new()),
        snapshot: RwLock::new(snapshot),
        retry_counts: RwLock::new(
            registry
                .servers()
                .iter()
                .map(|server| (server.id.clone(), 0))
                .collect(),
        ),
        shutdown_tx,
    });

    Ok(PreparedMcpRuntime {
        runtime: McpRuntimeHandle { inner },
        resources,
    })
}

/// Allocate configured managed work without starting a native lifecycle task.
/// 分配已配置受管工作，不启动原生生命周期任务。
/// servers contain the declared definitions and resources their original ownership scope.
/// servers 包含已声明定义，resources 包含其原所有权范围。
/// Return undispatched work so the shared admission can register it before effects.
/// 返回未派发工作，使共享入场可在副作用之前登记它。
pub(super) fn prepare_managed_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
) -> Result<PreparedMcpRuntime, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        McpRuntimePurpose::Managed,
        crate::mcp::registry::McpPluginLaunchSource::Unproven,
    )
}

/// Allocate closed target work with the original pure profile before native dispatch.
/// 在原生派发前使用原纯 profile 分配关闭目标工作。
/// servers, resources and profile come from one sealed target; return undispatched work or a pre-effect error.
/// servers、resources 及 profile 来自单个密封目标；返回未派发工作或副作用前错误。
pub(super) fn prepare_target_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
    profile: vulcan_plugin_runtime::VulcanPluginTargetMcpProfile,
) -> Result<PreparedMcpRuntime, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        McpRuntimePurpose::TargetPreparation,
        crate::mcp::registry::McpPluginLaunchSource::Target(profile),
    )
}

/// Allocate old restored work with its original profile before any native dispatch.
/// 在任何原生派发前，使用原画像分配旧恢复工作。
/// servers and resources are sealed old inputs; return one closed undispatched runtime or pre-effect error.
/// servers 及 resources 为密封旧输入；返回单个关闭未派发运行时或副作用前错误。
pub(super) fn prepare_predecessor_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
    profile: vulcan_plugin_runtime::VulcanPluginPredecessorMcpProfile,
) -> Result<PreparedMcpRuntime, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        McpRuntimePurpose::PredecessorPreparation,
        crate::mcp::registry::McpPluginLaunchSource::Predecessor(profile),
    )
}

/// Allocate proven ordinary work with its original profile before any native dispatch.
/// 在任何原生派发前，使用原 profile 分配已证明普通工作。
/// servers and resources are sealed materialization inputs; return one undispatched closed original.
/// servers 及 resources 为密封物化输入；返回单个未派发且关闭的原实例。
pub(super) fn prepare_published_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
    profile: vulcan_plugin_runtime::VulcanPluginPublishedMcpProfile,
) -> Result<PreparedMcpRuntime, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        McpRuntimePurpose::PublishedPreparation,
        crate::mcp::registry::McpPluginLaunchSource::Published(profile),
    )
}

#[cfg(test)]
mod release_barrier_tests;
#[cfg(test)]
mod remote_release_tests;

/// Host-owned timeout for optional MCP prompt and resource discovery requests.
/// MCP 可选 prompt 与 resource 发现请求使用的宿主持有超时。
const MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Host-owned per-partition readiness timeout for one plugin MCP partition.
/// 单个插件 MCP 分区使用的宿主持有就绪超时。
///
/// Every plugin partition waits with its own identical deadline, and the waits run concurrently,
/// so a slow or offline server can never add its deadline to the next partition.
/// 每个插件分区使用各自相同的期限等待，且这些等待并发执行，
/// 因此慢或离线服务器无法把自己的等待时间叠加到下一个分区。
pub(super) const MCP_PARTITION_READINESS_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum simultaneous transport starts across user, plugin, and browser MCP partitions.
/// 用户、插件与浏览器 MCP 分区同时启动传输的最大数量。
const MCP_MAX_CONCURRENT_TRANSPORT_STARTS: usize = 4;

/// Returns the process-wide fair start gate without limiting already-connected servers.
/// 返回进程级公平启动门，不限制已经连接的服务器数量。
fn mcp_transport_startup_semaphore() -> &'static tokio::sync::Semaphore {
    static STARTUP_SEMAPHORE: std::sync::OnceLock<tokio::sync::Semaphore> =
        std::sync::OnceLock::new();
    STARTUP_SEMAPHORE
        .get_or_init(|| tokio::sync::Semaphore::new(MCP_MAX_CONCURRENT_TRANSPORT_STARTS))
}

/// Process-local monotonic catalog identity; it contains no configuration or credential material.
/// 进程内单调目录身份，不包含配置或凭据内容。
static MCP_INSPECTION_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// Allocates an identity for a newly initialized or replaced catalog without persisting user state.
/// 为新初始化或替换的目录分配身份，不持久化用户状态。
fn next_inspection_generation() -> u64 {
    MCP_INSPECTION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone)]
pub(super) struct McpRuntimeHandle {
    /// Shared mutable inner state.
    /// 共享可变内部状态。
    inner: Arc<McpRuntimeInner>,
}

impl std::fmt::Debug for McpRuntimeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpRuntimeHandle").finish_non_exhaustive()
    }
}

/// Shared mutable state behind one long-lived MCP runtime handle.
/// 单个长期 MCP 运行时句柄背后的共享可变状态。
struct McpRuntimeInner {
    /// Opaque identity is allocated once for this actual resource owner, independently of the original request UUID.
    /// 不透明身份仅为此实际资源所有者分配一次，独立于原请求 UUID。
    instance_id: uuid::Uuid,
    /// Same-instance business admission is independent from irreversible shutdown and lease revocation.
    /// 同实例业务入场独立于不可逆关闭及租约撤销。
    business_gate: RwLock<McpBusinessGate>,
    /// Explicit admission intent fixes whether this exact instance may restart.
    /// 显式入场用途固定此精确实例是否允许重启。
    purpose: McpRuntimePurpose,
    /// Runtime registry containing the configured MCP server set.
    /// 包含已配置 MCP 服务器集合的运行时注册表。
    registry: McpServerRegistry,
    /// Source was fixed before dispatch and is copied directly into every actual native input Arc.
    /// 来源在派发前固定，并直接复制至每个实际原生输入 Arc。
    launch_source: crate::mcp::registry::McpPluginLaunchSource,
    /// Mutable transport map keyed by server identifier.
    /// 以服务器标识为键保存的可变传输映射。
    transports: RwLock<BTreeMap<String, Arc<dyn McpTransport>>>,
    /// Exact launch evidence is bound to a weak original transport, independently of configuration and retry counters.
    /// 精确启动证据绑定原传输弱引用，独立于配置及重试计数。
    launch_inputs: RwLock<BTreeMap<String, crate::mcp::registry::McpActualLaunchInputLease>>,
    /// Sealed original cleanup results distinguish actual retirement from startup and historical diagnostics.
    /// 密封原清理结果区分实际退役、启动及历史诊断。
    cleanup_observations: RwLock<BTreeMap<String, McpActualCleanupObservation>>,
    /// Original non-owning release receipts survive initialization failure and observer cancellation.
    /// 原非拥有释放回执在初始化失败及观察取消后继续存在。
    resource_releases: RwLock<BTreeMap<String, vulcan_mcp_runtime::McpResourceRelease>>,
    /// Original partition-wide evidence also covers unpublished initialization and lifecycle disposal.
    /// 原分区级证据同时覆盖尚未发布的初始化及生命周期销毁。
    partition_release: vulcan_mcp_runtime::McpResourceRelease,
    /// Mutable OAuth-session cache keyed by server identifier for host-driven remote authorization flows.
    /// 以服务器标识为键保存的可变 OAuth 会话缓存，供宿主驱动的远端授权流程复用。
    auth_sessions: AsyncRwLock<BTreeMap<String, Arc<McpOAuthSession>>>,
    /// Mutable snapshot published to the host runtime.
    /// 发布给宿主运行时的可变快照。
    snapshot: RwLock<McpRuntimeSnapshot>,
    /// Consecutive automatic connection failures keyed by exact server identifier.
    /// 以精确服务器标识为键保存的连续自动连接失败次数。
    retry_counts: RwLock<BTreeMap<String, u8>>,
    /// Shared shutdown signal observed by all background MCP runtime tasks.
    /// 被所有后台 MCP 运行时任务共同观察的关闭信号。
    shutdown_tx: watch::Sender<bool>,
}

impl McpRuntimeHandle {
    /// Captures exact protected inputs from a currently connected original transport under its live locks.
    /// 在实时锁内，从当前已连接原传输捕获精确受保护输入。
    /// Server_id selects the actual live entry; read is a trusted synchronous host closure, never plugin code.
    /// server_id 选择实际实时条目；read 为可信同步宿主闭包，绝不是插件代码。
    /// Returns the closure result only for the same recorded original owner, otherwise a value-free explicit rejection.
    /// 仅对同一有记录原所有者返回闭包结果，否则返回不含值的显式拒绝。
    #[cfg(test)]
    pub(crate) fn with_connected_launch_inputs<R>(
        &self,
        server_id: &str,
        read: impl FnOnce(
            &vulcan_mcp_runtime::McpTransportLaunchSpec,
            Option<&Arc<McpOAuthSession>>,
        ) -> R,
    ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError> {
        self.with_connected_input_lease(server_id, |inputs, transport| {
            inputs.with_current_transport(transport, read)
        })
    }

    /// Borrow protected inputs for the exact original published profile under the same live admission locks.
    /// 在相同实时入场锁下，为精确原已发布 profile 借用受保护输入。
    /// server_id selects the real entry and read is a trusted host closure; return its result or explicit rejection.
    /// server_id 选择真实条目，read 为可信宿主闭包；返回其结果或显式拒绝。
    pub(crate) fn with_connected_published_launch_inputs<R>(
        &self,
        server_id: &str,
        profile: &vulcan_plugin_runtime::VulcanPluginPublishedMcpProfile,
        read: impl FnOnce(
            &vulcan_mcp_runtime::McpTransportLaunchSpec,
            Option<&Arc<McpOAuthSession>>,
        ) -> R,
    ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError> {
        self.with_connected_input_lease(server_id, |inputs, transport| {
            inputs.with_published_source(transport, profile, read)
        })
    }

    /// Borrow original target inputs under the same actual admission, transport, input and status locks.
    /// 在同一实际入场、传输、输入及状态锁下借用原目标输入。
    /// server_id selects the actual entry, profile fixes its original source and read receives protected inputs.
    /// server_id 选择实际条目，profile 固定其原来源，read 接收受保护输入。
    /// Return read's value or a value-free rejection; no captured lease escapes the synchronous borrow.
    /// 返回 read 值或不含值的拒绝；捕获租约不逃逸同步借用。
    pub(crate) fn with_connected_target_launch_inputs<R>(
        &self,
        server_id: &str,
        profile: &vulcan_plugin_runtime::VulcanPluginTargetMcpProfile,
        read: impl FnOnce(
            &vulcan_mcp_runtime::McpTransportLaunchSpec,
            Option<&Arc<McpOAuthSession>>,
        ) -> R,
    ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError> {
        self.with_connected_input_lease(server_id, |inputs, transport| {
            inputs.with_target_source(transport, profile, read)
        })
    }

    /// Inspect old actual inputs under the same admission, transport, input and readiness locks.
    /// 在相同入场、传输、输入及就绪锁下检查旧实际输入。
    /// server_id selects the actual entry and profile fixes its original source; return read's value or rejection.
    /// server_id 选择实际条目，profile 固定原来源；返回 read 值或拒绝。
    pub(crate) fn with_connected_predecessor_launch_inputs<R>(
        &self,
        server_id: &str,
        profile: &vulcan_plugin_runtime::VulcanPluginPredecessorMcpProfile,
        read: impl FnOnce(
            &vulcan_mcp_runtime::McpTransportLaunchSpec,
            Option<&Arc<McpOAuthSession>>,
        ) -> R,
    ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError> {
        self.with_connected_input_lease(server_id, |inputs, transport| {
            inputs.with_predecessor_source(transport, profile, read)
        })
    }

    /// Freeze one actual connected input lease and transport for a synchronous trusted inspection.
    /// 为同步可信检查固定单个实际已连接输入租约及传输。
    /// server_id selects the original live entry and read validates its lease; no raw lease escapes this borrow.
    /// server_id 选择原实时条目，read 校验其租约；此借用不泄出原始租约。
    fn with_connected_input_lease<R>(
        &self,
        server_id: &str,
        read: impl FnOnce(
            &crate::mcp::registry::McpActualLaunchInputLease,
            &Arc<dyn McpTransport>,
        ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError>,
    ) -> Result<R, crate::mcp::registry::McpLaunchInputCaptureError> {
        use crate::mcp::registry::McpLaunchInputCaptureError;
        // Admission, transport, evidence and status remain frozen throughout the synchronous borrow.
        // 同步借用期间，入场、传输、证据及状态保持冻结。
        let business = self
            .inner
            .business_gate
            .read()
            .expect("mcp business gate poisoned");
        if *business == McpBusinessGate::Closed || self.shutdown_requested() {
            return Err(McpLaunchInputCaptureError::Unavailable);
        }
        let transports = self
            .inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned");
        let inputs = self
            .inner
            .launch_inputs
            .read()
            .expect("MCP actual launch input map poisoned");
        let snapshot = self
            .inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned");
        if snapshot
            .servers
            .get(server_id)
            .is_none_or(|server| server.status != McpRuntimeServerStatus::Connected)
        {
            return Err(McpLaunchInputCaptureError::Unavailable);
        }
        let transport = transports
            .get(server_id)
            .ok_or(McpLaunchInputCaptureError::Unavailable)?;
        let inputs = inputs
            .get(server_id)
            .ok_or(McpLaunchInputCaptureError::Unavailable)?;
        read(inputs, transport)
    }

    /// Returns whether this runtime partition owns the exact server identifier.
    /// 返回当前运行时分区是否持有精确服务器标识。
    pub(super) fn contains_server(&self, server_id: &str) -> bool {
        self.inner.registry.server_by_id(server_id).is_some()
    }

    /// Returns the latest immutable runtime snapshot.
    /// 返回最新的不可变运行时快照。
    fn snapshot(&self) -> McpRuntimeSnapshot {
        self.inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned")
            .clone()
    }

    /// Returns one stable MCP runtime overview for upper runtime surfaces.
    /// 返回供上层运行时表面消费的单份稳定 MCP 运行时概览。
    pub(super) fn overview(&self) -> McpRuntimeOverview {
        self.snapshot().overview()
    }

    /// Return plugin namespaces declared by this actual runtime's immutable server registry.
    /// 返回此实际运行时不可变服务器注册表声明的插件命名空间。
    /// No inputs; the owned set identifies observation routing only, never native package authority.
    /// 无输入；自有集合仅标识观察路由，绝不证明原生插件包权威。
    pub(super) fn plugin_source_ids(&self) -> BTreeSet<String> {
        self.inner
            .registry
            .servers()
            .iter()
            .filter_map(|server| match &server.source {
                crate::mcp::config::McpServerSource::UserConfigured => None,
                crate::mcp::config::McpServerSource::Plugin { plugin_id, .. } => {
                    Some(plugin_id.clone())
                }
            })
            .collect()
    }

    /// Returns structured runtime states for every server owned by this partition.
    /// 返回当前分区持有的每个服务器的结构化运行时状态。
    pub(super) fn server_states(&self) -> Vec<McpRuntimeServerState> {
        let snapshot = self.snapshot();
        self.states_from_snapshot(&snapshot)
    }

    /// Projects states from the caller-owned snapshot so schemas and status share one capture.
    /// 从调用者持有的快照投影状态，使 Schema 与状态共享同次采集。
    fn states_from_snapshot(&self, snapshot: &McpRuntimeSnapshot) -> Vec<McpRuntimeServerState> {
        let retry_counts = self
            .inner
            .retry_counts
            .read()
            .expect("mcp runtime retry count map poisoned");
        // transports is cloned once so diagnostic reads do not repeatedly acquire the shared map lock.
        // transports 只克隆一次，避免诊断读取反复获取共享映射锁。
        let transports = self
            .inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned")
            .clone();
        snapshot
            .servers
            .iter()
            .filter_map(|(server_id, server)| {
                let config = self.inner.registry.server_by_id(server_id)?;
                let (source, plugin_id, generation) = match &config.source {
                    crate::mcp::config::McpServerSource::UserConfigured => {
                        (String::from("user"), None, None)
                    }
                    crate::mcp::config::McpServerSource::Plugin {
                        plugin_id,
                        generation,
                    } => (
                        String::from("plugin"),
                        Some(plugin_id.clone()),
                        Some(*generation),
                    ),
                };
                // identity is read from the live transport so reconnects cannot retain stale initialize metadata.
                // identity 从实时传输读取，避免重连后继续保留过期的 initialize 元数据。
                let identity = transports
                    .get(server_id)
                    .and_then(|transport| transport.server_identity());
                Some(McpRuntimeServerState {
                    id: server_id.clone(),
                    name: server.name.clone(),
                    source,
                    plugin_id,
                    generation,
                    transport: server.transport_label.clone(),
                    endpoint: server.endpoint_summary.clone(),
                    status: server.status,
                    retry_count: retry_counts.get(server_id).copied().unwrap_or(0),
                    protocol_version: identity
                        .as_ref()
                        .map(|value| value.protocol_version.clone()),
                    server_product: identity.as_ref().map(|value| value.product.clone()),
                    server_version: identity.as_ref().map(|value| value.version.clone()),
                    tool_count: server.visible_tool_descriptors().len(),
                    prompt_count: server.prompt_count,
                    resource_count: server.resource_count,
                    last_error: server.last_error.clone(),
                    recent_logs: transports
                        .get(server_id)
                        .map(|transport| transport.recent_logs())
                        .unwrap_or_else(|| server.recent_logs.clone()),
                })
            })
            .collect()
    }

    /// Returns full schemas and states from the same immutable snapshot.
    /// 从同一不可变快照返回完整 Schema 与状态。
    pub(super) fn server_inspections(&self) -> Vec<McpRuntimeServerInspection> {
        let snapshot = self.snapshot();
        self.states_from_snapshot(&snapshot)
            .into_iter()
            .map(|state| {
                let tools = snapshot.servers[&state.id]
                    .visible_tool_descriptors()
                    .iter()
                    .map(|tool| tool.inspection())
                    .collect();
                McpRuntimeServerInspection {
                    inspection_generation: snapshot.servers[&state.id].inspection_generation,
                    state,
                    tools,
                }
            })
            .collect()
    }

    /// Queries metadata on an already connected transport and rejects a replaced connection.
    /// 在已连接传输查询元数据，并拒绝连接已替换时的过期结果。
    pub(super) async fn server_capabilities(
        &self,
        server_id: &str,
    ) -> Result<serde_json::Value, String> {
        let server = self
            .server_config(server_id)
            .ok_or_else(|| String::from("未找到 MCP 服务器。"))?;
        if self
            .snapshot()
            .servers
            .get(server_id)
            .is_none_or(|entry| entry.status != McpRuntimeServerStatus::Connected)
        {
            return Err(String::from("MCP 服务器尚未连接；详情读取不会自动连接。"));
        }
        let transport = self
            .transport_for_server(server_id)
            .ok_or_else(|| String::from("MCP 连接不可用。"))?;
        let (prompts, resources) = tokio::join!(
            run_bounded_optional_capability_discovery(
                &server,
                "prompt",
                MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
                transport.list_prompts()
            ),
            run_bounded_optional_capability_discovery(
                &server,
                "resource",
                MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
                transport.list_resources()
            )
        );
        if self
            .transport_for_server(server_id)
            .is_none_or(|current| !Arc::ptr_eq(&current, &transport))
            || self
                .snapshot()
                .servers
                .get(server_id)
                .is_none_or(|entry| entry.status != McpRuntimeServerStatus::Connected)
        {
            return Err(String::from("MCP 连接已经变化，请重新读取详情。"));
        }
        Ok(serde_json::json!({
            "prompts": prompts.as_ref().ok(), "promptError": prompts.as_ref().err().map(|error| server.sanitize_diagnostic(error)),
            "resources": resources.as_ref().ok(), "resourceError": resources.as_ref().err().map(|error| server.sanitize_diagnostic(error))
        }))
    }

    /// Resolves the currently active transport for one connected server.
    /// 解析某个已连接服务器当前活动的传输实例。
    fn transport_for_server(&self, server_id: &str) -> Option<Arc<dyn McpTransport>> {
        // Retain admission through the actual clone, so closure cannot race transport acquisition.
        // 实际克隆期间保留入场锁，避免关闭与传输获取竞争。
        let business = self
            .inner
            .business_gate
            .read()
            .expect("mcp business gate poisoned");
        if *business != McpBusinessGate::Open {
            return None;
        }
        // Keep transport registration locked until this call retains its actual resource.
        // 保持传输登记锁，直至此调用保留其实际资源。
        let transports = self
            .inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned");
        if self.shutdown_requested() {
            return None;
        }
        transports.get(server_id).cloned()
    }

    /// Resolves one configured server entry by identifier from the immutable registry snapshot.
    /// 从不可变注册表快照中根据标识解析单个已配置服务器条目。
    fn server_config(&self, server_id: &str) -> Option<McpServerConfig> {
        self.inner.registry.server_by_id(server_id).cloned()
    }

    /// Resolves or lazily creates one reusable OAuth session for the target configured remote MCP server.
    /// 为目标已配置远端 MCP 服务器解析或延迟创建一个可复用 OAuth 会话。
    async fn auth_session_for_server(
        &self,
        server_id: &str,
    ) -> Result<Option<Arc<McpOAuthSession>>, String> {
        if let Some(session) = self
            .inner
            .auth_sessions
            .read()
            .await
            .get(server_id)
            .cloned()
        {
            return Ok(Some(session));
        }

        let Some(server) = self.server_config(server_id) else {
            return Err(format!("未找到 MCP 服务器 `{server_id}`。"));
        };
        let Some(session) = build_server_auth_session(&server).await? else {
            return Ok(None);
        };
        self.inner
            .auth_sessions
            .write()
            .await
            .insert(server_id.to_string(), session.clone());
        Ok(Some(session))
    }

    /// Returns currently available MCP tool views filtered by one allowlist.
    /// 返回按单份白名单过滤后的当前可用 MCP 工具视图集合。
    pub(super) fn available_tool_views_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolView> {
        // A prepared catalog is diagnostic only until this original instance is admitted.
        // 此原实例准入前，准备目录仅供诊断。
        let business = self
            .inner
            .business_gate
            .read()
            .expect("mcp business gate poisoned");
        if *business != McpBusinessGate::Open || self.shutdown_requested() {
            return Vec::new();
        }
        self.snapshot().filtered_tool_views(allowlist)
    }

    /// Calls one MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用单个 MCP 工具。
    ///
    /// Every rejection carries one stable machine-readable code followed by concise English text:
    /// the model must classify the failure by code, and any localized UI copy is derived from the
    /// code instead of the transport wording.
    /// 每个拒绝都携带一个稳定机器可读编码，后接简明英文说明：模型必须按编码分类失败，
    /// 任何本地化界面文案都由编码派生，而不是传输层措辞。
    ///
    /// # Parameters
    /// 参数
    /// `tool_id` is the exact qualified host-facing identifier and `args` is the complete argument object.
    /// `tool_id` 是精确的限定宿主侧标识；`args` 是完整参数对象。
    ///
    /// # Returns
    /// 返回值
    /// The remote tool result, or one coded failure for an unknown tool, a disconnected server,
    /// or a rejected remote invocation.
    /// 返回远端工具结果；或针对未知工具、已断开服务器、被拒绝的远端调用返回一条带编码失败。
    pub async fn call_tool_by_id(
        &self,
        tool_id: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.call_tool_by_id_for_audience(
            tool_id,
            args,
            vulcan_mcp_runtime::McpToolResultAudience::Machine,
        )
        .await
    }

    /// Resolves the same authorized tool and selects only its final result audience.
    /// 解析同一个授权工具，仅选择其最终结果消费者。
    /// Inputs preserve tool identity and arguments; returns the selected projection without retries.
    /// 输入保留工具身份与参数；返回选定投影，不重试调用。
    pub(super) async fn call_tool_by_id_for_audience(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        audience: vulcan_mcp_runtime::McpToolResultAudience,
    ) -> Result<serde_json::Value, String> {
        let Some(descriptor) = self
            .snapshot()
            .tool_descriptors()
            .into_iter()
            .find(|tool| tool.qualified_name() == tool_id)
        else {
            return Err(format!(
                "mcp_tool_not_found: no MCP tool matches `{tool_id}` in the current runtime catalog"
            ));
        };
        let Some(transport) = self.transport_for_server(descriptor.server_id()) else {
            return Err(format!(
                "mcp_server_disconnected: MCP server `{}` is not connected; tool `{}` was not invoked",
                descriptor.server_id(),
                descriptor.tool_name()
            ));
        };
        transport
            .call_tool_with_audience(descriptor.tool_name(), args, audience)
            .await
            .map_err(|error| {
                format!(
                    "mcp_tool_invocation_failed: MCP tool `{}` failed: {error}",
                    descriptor.tool_name(),
                )
            })
    }

    /// Returns the current OAuth status label for one configured MCP server.
    /// 返回单个已配置 MCP 服务器的当前 OAuth 状态标签。
    pub(super) async fn server_auth_status_label(&self, server_id: &str) -> Result<String, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Ok(String::from("未配置 OAuth"));
        };
        Ok(session.auth_status().await.label().to_string())
    }

    /// Starts one host-driven OAuth authorization flow for the target MCP server.
    /// 为目标 MCP 服务器启动一条宿主驱动的 OAuth 授权流程。
    pub(super) async fn start_server_auth(
        &self,
        server_id: &str,
        client_name: &str,
    ) -> Result<McpAuthChallenge, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session
            .start_authorization(client_name)
            .await
            .map_err(|error| error.to_string())
    }

    /// Finishes one host-driven OAuth authorization flow for the target MCP server.
    /// 为目标 MCP 服务器完成一条宿主驱动的 OAuth 授权流程。
    pub(super) async fn finish_server_auth(
        &self,
        server_id: &str,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session
            .finish_authorization(authorization_code, oauth_state)
            .await
            .map_err(|error| error.to_string())
    }

    /// Removes every stored OAuth credential retained for the target MCP server.
    /// 移除目标 MCP 服务器当前保留的全部 OAuth 凭据。
    pub(super) async fn remove_server_auth(&self, server_id: &str) -> Result<(), String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session.remove_authorization().await;
        Ok(())
    }

    /// Requests all background MCP runtime tasks to stop and tear down their transports.
    /// 请求所有后台 MCP 运行时任务停止并拆除各自传输。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; the shutdown state is latched for current and future server-task receivers.
    /// 不返回值；关闭状态会为当前及未来服务器任务接收端锁存。
    pub(super) fn shutdown(&self) {
        // Closing admission precedes the latched stop signal for this same actual transport.
        // 此相同实际传输先关闭入场，再锁存停止信号。
        let mut business = self
            .inner
            .business_gate
            .write()
            .expect("mcp business gate poisoned");
        *business = McpBusinessGate::Closed;
        request_mcp_runtime_shutdown(&self.inner.shutdown_tx);
    }

    /// Returns the latched shutdown decision, including before terminal resource cleanup.
    /// 返回锁存的关闭决定，包括终态资源清理之前的阶段。
    pub(super) fn shutdown_requested(&self) -> bool {
        *self.inner.shutdown_tx.borrow()
    }

    /// Compares original control identity without relying on server names or mutable catalog state.
    /// 比较原控制身份，不依赖服务器名称或可变目录状态。
    /// Receives another handle and returns whether both observe the same original instance.
    /// 接收另一个句柄，返回两者是否观察同一原实例。
    pub(super) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Returns whether latched shutdown has positive original cleanup and physical retirement evidence.
    /// 返回锁存关闭是否具有正向原清理及物理退役证据。
    pub(super) fn is_fully_stopped(&self) -> bool {
        matches!(self.cleanup_admission(), McpCleanupAdmission::Complete)
    }

    /// Waits for protocol teardown and original physical ownership within the caller's total deadline.
    /// 在调用者总期限内等待协议拆除及原物理所有权释放。
    /// Errors preserve original state and receipts; observing this future never cancels cleanup.
    /// 错误保留原状态及回执；观察此 future 绝不取消清理。
    pub(super) async fn wait_until_drained(
        &self,
        deadline: tokio::time::Instant,
    ) -> Result<(), String> {
        // The common admission predicate already includes the original aggregate physical release.
        // 共同准入判定已经包含原聚合物理释放。
        self.wait_until_stopped(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await
    }

    /// Waits until every enabled server in this partition has completed initialize and atomic tool discovery.
    /// 等待当前分区内每个已启用服务器完成 initialize 与原子工具发现。
    ///
    /// # Parameters
    /// 参数
    /// `maximum_wait` is the host-owned readiness deadline and cannot be expanded by plugin input.
    /// `maximum_wait` 是宿主持有的就绪期限，插件输入不能放大该期限。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only for a fully connected partition; terminal failure or deadline expiry returns a diagnostic error.
    /// 仅对完全连接的分区返回成功；终止失败或期限到期会返回诊断错误。
    pub(super) async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + maximum_wait;
        loop {
            if self.shutdown_requested() {
                return Err(String::from(
                    "MCP partition was shut down during readiness.",
                ));
            }
            let snapshot = self.snapshot();
            let enabled_servers = snapshot
                .servers
                .values()
                .filter(|server| server.enabled)
                .collect::<Vec<_>>();
            if enabled_servers
                .iter()
                .all(|server| server.status == McpRuntimeServerStatus::Connected)
            {
                return Ok(());
            }
            if let Some(server) = enabled_servers
                .iter()
                .find(|server| server.status == McpRuntimeServerStatus::Failed)
            {
                return Err(server.last_error.clone().unwrap_or_else(|| {
                    format!("MCP server `{}` entered Failed state.", server.name)
                }));
            }
            if tokio::time::Instant::now() >= deadline {
                let pending_servers = enabled_servers
                    .iter()
                    .filter(|server| server.status != McpRuntimeServerStatus::Connected)
                    .map(|server| format!("{}={}", server.name, server.status.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "MCP partition readiness timed out after {} seconds: {pending_servers}",
                    maximum_wait.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Waits until every server task has completed transport teardown after shutdown.
    /// 等待全部服务器任务在收到关闭请求后完成传输拆除。
    ///
    /// # Parameters
    /// 参数
    /// `maximum_wait` is the host-owned teardown deadline.
    /// `maximum_wait` 是宿主持有的拆除期限。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only for positive original cleanup and physical release; startup diagnostics remain unchanged.
    /// 仅对正向原清理及物理释放返回成功；启动诊断保持不变。
    async fn wait_until_stopped(&self, maximum_wait: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + maximum_wait;
        loop {
            match self.cleanup_admission() {
                McpCleanupAdmission::Complete => return Ok(()),
                McpCleanupAdmission::Failed(error) => return Err(error),
                McpCleanupAdmission::Pending => {}
            }
            if tokio::time::Instant::now() >= deadline {
                // pending_servers identifies every process whose shutdown could not be confirmed.
                // pending_servers 标识每个无法确认已关闭的进程。
                let snapshot = self.snapshot();
                let pending_servers = snapshot
                    .servers
                    .values()
                    .filter(|server| server.status != McpRuntimeServerStatus::Stopped)
                    .map(|server| format!("{}={}", server.name, server.status.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "MCP partition shutdown timed out after {} seconds: {pending_servers}",
                    maximum_wait.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// Latches one MCP runtime shutdown request for current and future watch receivers.
/// 为当前及未来 watch 接收端锁存一次 MCP 运行时关闭请求。
///
/// # Parameters
/// 参数
/// `shutdown_tx` owns the single monotonic shutdown state shared by every server task.
/// `shutdown_tx` 持有全部服务器任务共享的单调关闭状态。
///
/// # Returns
/// 返回值
/// Returns no value; repeated requests keep the channel in the requested state.
/// 不返回值；重复请求会使通道继续保持已请求状态。
fn request_mcp_runtime_shutdown(shutdown_tx: &watch::Sender<bool>) {
    shutdown_tx.send_replace(true);
}

/// Boots one long-lived MCP runtime handle from source-neutral server definitions.
/// 根据中立服务器定义引导一个长期 MCP 运行时句柄。
pub(super) async fn bootstrap_mcp_runtime(
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeHandle, String> {
    // Ordinary managed partitions use the same ownership path without a candidate host bundle.
    // 普通托管分区使用相同所有权路径，不附带候选宿主资源包。
    let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(());
    create_mcp_runtime(servers, resources, McpRuntimePurpose::Managed)
}

/// Prepare raw target work without source certification or native dispatch.
/// 准备原始目标工作，不进行来源认证或原生派发。
/// servers and resources retain the exact caller inputs; return undispatched work with closed business admission.
/// servers 及 resources 保留精确调用者输入；返回关闭业务入场的未派发工作。
pub(super) fn prepare_unproven_target_mcp_runtime(
    servers: Vec<McpServerConfig>,
    resources: vulcan_mcp_runtime::McpTransportResources,
) -> Result<PreparedMcpRuntime, String> {
    prepare_mcp_runtime(
        servers,
        resources,
        McpRuntimePurpose::TargetPreparation,
        crate::mcp::registry::McpPluginLaunchSource::Unproven,
    )
}

/// Owns unpublished MCP runtimes until their access generation is published.
/// 在访问代次发布前持有尚未发布的 MCP 运行时。
pub(super) struct McpRuntimeStartupGuard {
    /// Candidate handles that must stop if startup fails or its future is cancelled.
    /// 启动失败或 Future 被取消时必须停止的候选句柄。
    runtimes: Vec<McpRuntimeHandle>,
    /// Whether publication transferred lifetime ownership to a runtime access generation.
    /// 发布是否已把生命周期所有权转交给运行时访问代次。
    published: bool,
}

impl McpRuntimeStartupGuard {
    /// Creates an empty unpublished-runtime owner.
    /// 创建一个空的未发布运行时所有者。
    pub(super) fn new() -> Self {
        Self {
            runtimes: Vec::new(),
            published: false,
        }
    }

    /// Tracks one started runtime before the next cancellation point.
    /// 在下一个取消点之前跟踪一个已启动运行时。
    pub(super) fn track(&mut self, runtime: &McpRuntimeHandle) {
        self.runtimes.push(runtime.clone());
    }

    /// Transfers lifecycle ownership to the published access generation.
    /// 将生命周期所有权转交给已发布的访问代次。
    pub(super) fn mark_published(&mut self) {
        self.published = true;
    }
}

impl Drop for McpRuntimeStartupGuard {
    /// Stops every unpublished candidate even when its startup future is cancelled.
    /// 即使启动 Future 被取消，也会停止每个未发布的候选运行时。
    fn drop(&mut self) {
        if !self.published {
            for runtime in &self.runtimes {
                runtime.shutdown();
            }
        }
    }
}

/// Boots one stable MCP runtime access surface from source-neutral server definitions.
/// 根据中立服务器定义引导一个稳定 MCP 运行时访问面。
///
/// The user partition never blocks startup, and every plugin partition is booted first and then
/// waited on concurrently with its own host-owned deadline. Failures are reported for the first
/// failing partition in stable key order after every started partition has been shut down.
/// 用户分区从不阻塞启动；每个插件分区先完成引导，再以各自宿主持有期限并发等待。
/// 在全部已启动分区被关闭后，按稳定键顺序报告第一个失败分区。
pub async fn bootstrap_mcp_runtime_access(
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    bootstrap_mcp_runtime_access_with_originals(servers, |_| Ok(Vec::new()))
        .await
        .map(|(access, _)| access)
}

/// Bootstrap raw unproven declarations and sealed published inputs as one concurrently observed batch.
/// 将原始未证明声明及密封已发布输入作为单个并发观察批次引导。
/// servers contains no sealed owner; manager and authority are the original host pair; published is consumed once.
/// servers 不包含密封所有者；manager 及 authority 为原宿主配对；published 仅消费一次。
/// Return original access and ready closed controls; errors or cancellation close every original without replay.
/// 返回原访问及就绪关闭控制；错误或取消关闭每个原实例，不重播。
pub async fn bootstrap_mcp_runtime_access_with_published(
    servers: Vec<McpServerConfig>,
    manager: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    authority: &vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority,
    published: Vec<crate::mcp::PreparedPublishedPluginMcpConfigs>,
) -> Result<(McpRuntimeAccess, Vec<PublishedPluginMcpRuntime>), String> {
    if !authority.matches_manager(manager) {
        return Err(String::from(
            "Published MCP bootstrap requires its original manager and host.",
        ));
    }
    // Exactly one declared producer owns each plugin before any actual raw or sealed task starts.
    // 在任何实际原始或密封任务启动之前，每个插件恰由单个已声明生产者拥有。
    let mut sealed_owners = BTreeSet::new();
    for prepared in &published {
        prepared.validate_original_host(authority)?;
        if !sealed_owners.insert(prepared.identity().0.to_owned()) {
            return Err(String::from(
                "Published MCP bootstrap contains a duplicate sealed owner.",
            ));
        }
    }
    if servers.iter().any(|server| {
        matches!(&server.source,
        crate::mcp::McpServerSource::Plugin { plugin_id, .. } if sealed_owners.contains(plugin_id))
    }) {
        return Err(String::from(
            "MCP bootstrap declares both raw and sealed inputs for one plugin.",
        ));
    }
    bootstrap_mcp_runtime_access_with_originals(servers, |access| {
        published
            .into_iter()
            .map(|prepared| PublishedPluginMcpRuntime::start(access, manager, prepared, authority))
            .collect()
    })
    .await
}

/// Register initializer's original closed controls after raw construction and before any partition readiness wait.
/// 在原始构造之后及任何分区就绪等待之前，登记 initializer 原关闭控制。
/// servers supplies explicit raw declarations; initializer is a trusted internal synchronous factory, never plugin business.
/// servers 提供明确原始声明；initializer 为可信内部同步工厂，绝不是插件业务。
/// Return the one access and same ready controls; all actual startup owners remain armed on errors and cancellation.
/// 返回单个访问及同一就绪控制；错误及取消时全部实际启动所有者保持启用。
async fn bootstrap_mcp_runtime_access_with_originals(
    servers: Vec<McpServerConfig>,
    initializer: impl FnOnce(&McpRuntimeAccess) -> Result<Vec<PublishedPluginMcpRuntime>, String>,
) -> Result<(McpRuntimeAccess, Vec<PublishedPluginMcpRuntime>), String> {
    let overview = configured_mcp_registry_overview(servers.clone());
    let mut grouped = BTreeMap::<String, Vec<McpServerConfig>>::new();
    for server in servers {
        grouped
            .entry(server.source.partition_key())
            .or_default()
            .push(server);
    }
    let mut partitions = BTreeMap::<String, McpRuntimeHandle>::new();
    // startup_guard closes already-started partitions when the bootstrap future is cancelled.
    // startup_guard 在引导 Future 被取消时关闭已经启动的分区。
    let mut startup_guard = McpRuntimeStartupGuard::new();
    for (partition_key, partition_servers) in grouped {
        match bootstrap_mcp_runtime(partition_servers).await {
            Ok(runtime) => {
                startup_guard.track(&runtime);
                partitions.insert(partition_key, runtime);
            }
            Err(error) => return Err(error),
        }
    }
    // plugin_keys lists every plugin partition in stable key order for deterministic reporting.
    // plugin_keys 按稳定键顺序列出全部插件分区，使失败报告顺序确定。
    let mut plugin_keys = partitions
        .keys()
        .filter(|partition_key| partition_key.starts_with("plugin:"))
        .cloned()
        .collect::<Vec<_>>();
    // Original raw control clones do not extend execution resources; access owns their sole partition leases.
    // 原原始控制克隆不延长执行资源；访问拥有其唯一分区租约。
    let access = McpRuntimeAccess::new(overview, partitions.clone());
    // Sealed originals enter that same actual registry lineage before any readiness await.
    // 密封原实例在任何就绪 await 之前进入该同一实际注册表谱系。
    let originals = initializer(&access)?;
    for native in &originals {
        // Route metadata only determines stable reporting order; the factory validated actual resource authority.
        // 路由元数据仅决定稳定报告顺序；工厂已经校验实际资源权威。
        let observer = native.observer();
        plugin_keys.push(
            crate::mcp::McpServerSource::Plugin {
                plugin_id: observer.identity().0.to_owned(),
                generation: observer.identity().1,
            }
            .partition_key(),
        );
    }
    if plugin_keys.is_empty() {
        startup_guard.mark_published();
        return Ok((access, originals));
    }
    // readiness_waits runs every plugin partition readiness check concurrently so one slow or
    // offline server cannot add its own deadline to the next partition.
    // readiness_waits 并发执行每个插件分区就绪检查，使单个慢或离线服务器
    // 无法把自己的等待时间叠加到下一个分区。
    let mut readiness_waits = tokio::task::JoinSet::new();
    for (partition_key, runtime) in partitions
        .iter()
        .filter(|(key, _)| key.starts_with("plugin:"))
    {
        // The same raw control is observed without borrowing another execution partition.
        // 观察同一原始控制，不借用另一个执行分区。
        let runtime = runtime.clone();
        let owned_partition_key = partition_key.clone();
        readiness_waits.spawn(async move {
            let readiness = runtime
                .wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT)
                .await;
            (owned_partition_key, readiness)
        });
    }
    for native in &originals {
        // This exact non-owning observer belongs to the already registered actual sealed launch.
        // 此精确非拥有观察属于已登记实际密封启动。
        let observer = native.observer();
        // Canonical source formatting is shared with every actual partition producer.
        // 规范来源格式与每个实际分区生产者共享。
        let partition_key = crate::mcp::McpServerSource::Plugin {
            plugin_id: observer.identity().0.to_owned(),
            generation: observer.identity().1,
        }
        .partition_key();
        readiness_waits.spawn(async move {
            let readiness = observer.wait_until_bootstrap_ready().await;
            (partition_key, readiness)
        });
    }
    // failures keeps every rejection keyed by partition so the first stable key is reported.
    // failures 以分区为键保留全部拒绝结果，从而报告第一个稳定键。
    let mut failures = BTreeMap::<String, String>::new();
    while let Some(joined_readiness) = readiness_waits.join_next().await {
        match joined_readiness {
            Ok((partition_key, Ok(()))) => {
                debug_assert!(plugin_keys.contains(&partition_key));
            }
            Ok((partition_key, Err(error))) => {
                failures.insert(partition_key, error);
            }
            Err(join_error) => {
                failures.insert(
                    format!("plugin-partition-readiness-task:{join_error}"),
                    format!("MCP plugin partition readiness task failed: {join_error}"),
                );
            }
        }
    }
    if let Some((_, error)) = failures.into_iter().next() {
        return Err(error);
    }
    startup_guard.mark_published();
    Ok((access, originals))
}

/// Replaces only the user-configured MCP runtime partition while retaining plugin partitions.
/// 仅替换用户配置 MCP 运行时分区，同时保留插件分区。
///
/// # Parameters
/// 参数
/// `current` owns retained plugin partitions and `servers` must contain only user definitions.
/// `current` 持有保留的插件分区，`servers` 必须只包含用户定义。
///
/// # Returns
/// 返回值
/// Returns a new access generation whose unrelated plugin processes were not restarted.
/// 返回一个未重启无关插件进程的新访问代次。
pub async fn replace_user_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    if servers
        .iter()
        .any(|server| server.source != crate::mcp::config::McpServerSource::UserConfigured)
    {
        return Err(String::from(
            "The user MCP partition accepts only user-configured server definitions.",
        ));
    }
    // The declared overview remains separate from the original sole registered execution lease.
    // 已声明概览与原唯一已登记执行租约保持分离。
    let overview = configured_mcp_registry_overview(servers.clone());
    // Shared admission rejects closure and registers this actual original before dispatch.
    // 共享入场拒绝已关闭状态，并在派发前登记此实际原实例。
    let lease = current.start_user_partition(servers)?;
    Ok(current.with_replaced_user_partition(overview, lease))
}

/// Prepares and atomically projects one plugin MCP partition while retaining user and unrelated plugin partitions.
/// 准备并原子投影单个插件 MCP 分区，同时保留用户与无关插件分区。
///
/// # Parameters
/// 参数
/// `current` owns retained partitions, `plugin_id` and `generation` define the exact source identity, and `servers` must belong only to that identity.
/// `current` 持有保留分区，`plugin_id` 与 `generation` 定义精确来源身份，`servers` 必须仅属于该身份。
///
/// # Returns
/// 返回值
/// Returns a new access generation only after every candidate server passes initialize and complete tool discovery.
/// 仅在每个候选服务器通过 initialize 与完整工具发现后返回新的访问代次。
pub async fn replace_plugin_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    plugin_id: &str,
    generation: u64,
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    if servers.iter().any(|server| {
        !matches!(
            &server.source,
            crate::mcp::config::McpServerSource::Plugin {
                plugin_id: source_plugin_id,
                generation: source_generation,
            } if source_plugin_id == plugin_id && *source_generation == generation
        )
    }) {
        return Err(format!(
            "Plugin MCP partition `{plugin_id}:{generation}` received a server from another source."
        ));
    }
    let overview = configured_mcp_registry_overview(servers.clone());
    let runtime = current.start_preparing_plugin_partition(plugin_id, servers)?;
    let mut startup_guard = McpRuntimeStartupGuard::new();
    startup_guard.track(&runtime);
    runtime
        .wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT)
        .await?;
    current.finish_preparing_plugin_partition(plugin_id, &runtime)?;
    let access = current.with_replaced_plugin_partition(plugin_id, generation, overview, runtime);
    startup_guard.mark_published();
    Ok(access)
}

/// Starts, validates, and fully stops one isolated plugin MCP partition without publishing it.
/// 启动、校验并完全停止一个隔离的插件 MCP 分区，且不发布该分区。
///
/// # Parameters
/// 参数
/// `plugin_id` and `generation` define the exact candidate source identity, while `servers` contains only local installation candidates.
/// `plugin_id` 与 `generation` 定义精确候选来源身份，`servers` 只包含本地安装候选服务器。
///
/// # Returns
/// 返回值
/// Returns success only after initialize, complete tool discovery, schema validation, and confirmed process teardown all succeed.
/// 仅在初始化、完整工具发现、Schema 校验与进程拆除确认全部成功后返回成功。
pub async fn validate_plugin_mcp_runtime_partition(
    plugin_id: &str,
    generation: u64,
    servers: Vec<McpServerConfig>,
) -> Result<(), String> {
    if servers.iter().any(|server| {
        !matches!(
            &server.source,
            crate::mcp::config::McpServerSource::Plugin {
                plugin_id: source_plugin_id,
                generation: source_generation,
            } if source_plugin_id == plugin_id && *source_generation == generation
        )
    }) {
        return Err(format!(
            "Plugin MCP validation partition `{plugin_id}:{generation}` received a server from another source."
        ));
    }
    // runtime remains isolated from every published MCP access generation.
    // runtime 与所有已发布 MCP 访问代次保持隔离。
    let runtime = bootstrap_mcp_runtime(servers).await?;
    let mut startup_guard = McpRuntimeStartupGuard::new();
    startup_guard.track(&runtime);
    // validation_result preserves readiness failure while teardown still runs unconditionally.
    // validation_result 保留就绪失败，同时无条件执行拆除。
    let validation_result = runtime
        .wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT)
        .await;
    runtime.shutdown();
    // shutdown_result proves local processes released staging files before installation continues.
    // shutdown_result 证明本地进程已在安装继续前释放暂存文件。
    let shutdown_result = runtime.wait_until_stopped(Duration::from_secs(7)).await;
    match (validation_result, shutdown_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(validation_error), Ok(())) => Err(validation_error),
        (Ok(()), Err(shutdown_error)) => Err(shutdown_error),
        (Err(validation_error), Err(shutdown_error)) => Err(format!(
            "{validation_error}; candidate teardown also failed: {shutdown_error}"
        )),
    }
}

/// Removes one plugin MCP partition from the next access generation without stopping old in-flight leases immediately.
/// 从下一访问代次移除单个插件 MCP 分区，同时不立即停止旧执行中的租约。
///
/// # Parameters
/// 参数
/// `current` owns all current partitions and `plugin_id` selects the exact plugin namespace to withdraw.
/// `current` 持有全部当前分区，`plugin_id` 选择需要撤销的精确插件命名空间。
///
/// # Returns
/// 返回值
/// Returns a new access generation that retains every unrelated partition.
/// 返回保留全部无关分区的新访问代次。
pub fn remove_plugin_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    plugin_id: &str,
) -> McpRuntimeAccess {
    current.without_plugin_partition(plugin_id)
}

/// Builds one reusable host-owned OAuth session for a configured remote MCP server when the server exposes OAuth metadata.
/// 当服务器暴露 OAuth 元数据时，为已配置的远端 MCP 服务器构建一个可复用的宿主自有 OAuth 会话。
async fn build_server_auth_session(
    server: &McpServerConfig,
) -> Result<Option<Arc<McpOAuthSession>>, String> {
    let Some(oauth) = server.oauth().cloned() else {
        return Ok(None);
    };
    let Some(uri) = server.remote_url().map(str::to_owned) else {
        return Err(format!(
            "MCP 服务器 `{}` 配置了 OAuth，但缺少远端 URL。",
            server.name
        ));
    };

    let session = McpOAuthSession::new(uri, oauth.into())
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(Arc::new(session)))
}

/// Spawns one background lifecycle task for a single enabled MCP server.
/// 为单个已启用 MCP 服务器启动一个后台生命周期任务。
///
/// # Parameters
/// 参数
/// `inner` owns the shared registry, snapshots, transports, auth cache, and shutdown latch.
/// `inner` 持有共享注册表、快照、传输、认证缓存与关闭锁存状态。
/// `server` is the exact enabled server whose connection lifecycle this task owns.
/// `server` 是本任务持有连接生命周期的精确已启用服务器。
/// `launch_resources` retains the original authority until this task and all its actual work end.
/// `launch_resources` 保留原权威，直至此任务及其全部实际工作结束。
///
/// # Returns
/// 返回值
/// Returns no handle; the long-lived task exits through the shared latched shutdown state or terminal setup failure.
/// 不返回句柄；长期任务通过共享锁存关闭状态或终止性初始化失败退出。
fn spawn_server_runtime_task(
    inner: Arc<McpRuntimeInner>,
    server: McpServerConfig,
    launch_resources: vulcan_mcp_runtime::McpTransportResources,
) {
    // Construct before spawn so even an unpolled cancelled task owns admission disposal.
    // 在启动前构造，使未轮询的已取消任务也拥有入场销毁责任。
    let admission = lifecycle_guard::retain_lifecycle_admission(
        inner.clone(),
        &server,
        launch_resources.clone(),
    );
    tokio::spawn(async move {
        // Keep disposal protection until every local future and transport reference is gone.
        // 保留销毁保护，直至全部本地 future 及传输引用消失。
        let _admission = admission;
        let mut shutdown_rx = inner.shutdown_tx.subscribe();
        // consecutiveFailures gates automatic restart so a permanently invalid server cannot spin forever.
        // consecutiveFailures 限制自动重启，避免永久无效的服务器无限循环。
        let mut consecutive_failures = 0_u8;
        // authSession is initialized inside the bounded retry loop because metadata discovery can fail transiently.
        // authSession 在有限重试循环内初始化，因为元数据发现可能发生瞬时失败。
        let mut auth_session: Option<Option<Arc<McpOAuthSession>>> = None;
        loop {
            // A new actual attempt cannot reuse a previous operation's positive cleanup evidence.
            // 新实际尝试不能复用前次操作的正向清理证据。
            // Clear before OAuth session construction or any other first external launch effect is allowed.
            // 在允许 OAuth 会话构造或其他首次外部启动效果之前清除。
            inner
                .cleanup_observations
                .write()
                .expect("MCP actual cleanup observation map poisoned")
                .remove(&server.id);

            // This same actual attempt scope covers prelaunch work and is consumed by native launch on success.
            // 此同一实际尝试范围覆盖启动前工作，并在成功时由原生启动消费。
            // Registration precedes the initial shutdown check so an unstarted task still proves its original release.
            // 登记先于初始关闭检查，使未开始工作的任务仍可证明其原释放。
            let (resources, release) =
                vulcan_mcp_runtime::McpTransportResources::new(launch_resources.clone());
            inner
                .resource_releases
                .write()
                .expect("MCP launch release map poisoned")
                .insert(server.id.clone(), release);
            if *shutdown_rx.borrow() {
                drop(resources);
                shutdown_server_runtime_state(&inner, &server.id).await;
                break;
            }

            if auth_session.is_none() {
                // Current OAuth initialization constructs owned clients and validates the URL; it starts no detached requests.
                // 当前 OAuth 初始化构造自有客户端并校验 URL；不会启动分离请求。
                // Leave the select before cleanup so cancellation has destroyed the complete original session future.
                // 清理前离开选择表达式，使取消已销毁完整原会话 future。
                let auth_session_result = tokio::select! {
                    _ = shutdown_rx.changed() => None,
                    result = build_server_auth_session(&server) => Some(result),
                };
                let Some(auth_session_result) = auth_session_result else {
                    drop(resources);
                    shutdown_server_runtime_state(&inner, &server.id).await;
                    return;
                };
                match auth_session_result {
                    Ok(session) => {
                        update_server_auth_session_cache(&inner, &server.id, session.clone()).await;
                        auth_session = Some(session);
                    }
                    Err(error) => {
                        // The failed session future already ended; release this exact prelaunch owner before observing cleanup.
                        // 失败会话 future 已结束；观察清理前释放此精确启动前所有者。
                        drop(resources);
                        if record_server_failure_and_should_stop(
                            &inner,
                            &server,
                            &mut consecutive_failures,
                            None,
                            error,
                        )
                        .await
                        {
                            return;
                        }
                        if wait_for_retry_or_shutdown(
                            &mut shutdown_rx,
                            retry_delay(&server.id, consecutive_failures),
                        )
                        .await
                        {
                            shutdown_server_runtime_state(&inner, &server.id).await;
                            return;
                        }
                        continue;
                    }
                }
            }

            update_server_status(
                &inner,
                &server.id,
                McpRuntimeServerStatus::Connecting,
                None,
                Vec::new(),
            );

            // The permit covers initialize and the first complete tools/list validation;
            // healthy long-lived connections release it before optional capability refresh.
            // 许可覆盖初始化与首次完整 tools/list 校验；健康长连接在可选能力刷新前释放它。
            let startup_permit = tokio::select! {
                _ = shutdown_rx.changed() => None,
                permit = mcp_transport_startup_semaphore().acquire() => {
                    Some(permit.expect("the process-wide MCP startup semaphore is never closed"))
                }
            };
            let Some(startup_permit) = startup_permit else {
                drop(resources);
                shutdown_server_runtime_state(&inner, &server.id).await;
                break;
            };
            // Leave the select before cleanup so the cancelled launch future drops all of its references.
            // 清理前先离开选择表达式，使已取消启动 future 释放其全部引用。
            let transport_result = tokio::select! {
                _ = shutdown_rx.changed() => None,
                result = async {
                    tokio::time::timeout(
                        Duration::from_secs(15),
                        inner.registry.spawn_transport(
                            &server.id,
                            auth_session.as_ref().cloned().flatten(),
                            resources,
                            inner.launch_source.clone(),
                        ),
                    )
                    .await
                    .map_err(|_| {
                        format!(
                            "MCP server `{}` initialize timed out after 15 seconds.",
                            server.name
                        )
                    })?
                } => Some(result),
            };
            let transport = match transport_result {
                None => {
                    shutdown_server_runtime_state(&inner, &server.id).await;
                    break;
                }
                Some(Ok(launched)) => {
                    // This matching pair comes from the exact successful native launch, before discovery admission.
                    // 此匹配对来自精确成功原生启动，先于发现入场。
                    let (transport, inputs) = launched.into_parts();
                    inner
                        .launch_inputs
                        .write()
                        .expect("MCP actual launch input map poisoned")
                        .insert(server.id.clone(), inputs);
                    transport
                }
                Some(Err(error)) => {
                    let should_stop = record_server_failure_and_should_stop(
                        &inner,
                        &server,
                        &mut consecutive_failures,
                        None,
                        error,
                    )
                    .await;
                    drop(startup_permit);
                    if should_stop {
                        return;
                    }
                    if wait_for_retry_or_shutdown(
                        &mut shutdown_rx,
                        retry_delay(&server.id, consecutive_failures),
                    )
                    .await
                    {
                        shutdown_server_runtime_state(&inner, &server.id).await;
                        break;
                    }
                    continue;
                }
            };

            // Discovery still owns an unpublished transport, so shutdown must interrupt
            // tools/list and explicitly close that exact candidate before returning.
            // 工具发现仍持有尚未发布的传输，因此关闭必须中断 tools/list 并在返回前明确关闭该候选者。
            let discovery_result = tokio::select! {
                _ = shutdown_rx.changed() => None,
                result = discover_server_tools_with_events(&inner, &server, transport.clone()) => Some(result),
            };
            let mut event_rx = match discovery_result {
                None => {
                    shutdown_server_runtime_state_with_transport(
                        &inner,
                        &server.id,
                        Some(transport),
                    )
                    .await;
                    return;
                }
                Some(Ok(receiver)) => receiver,
                Some(Err(error)) => {
                    let should_stop = record_server_failure_and_should_stop(
                        &inner,
                        &server,
                        &mut consecutive_failures,
                        Some(transport),
                        error,
                    )
                    .await;
                    drop(startup_permit);
                    if should_stop {
                        return;
                    }
                    if wait_for_retry_or_shutdown(
                        &mut shutdown_rx,
                        retry_delay(&server.id, consecutive_failures),
                    )
                    .await
                    {
                        shutdown_server_runtime_state(&inner, &server.id).await;
                        break;
                    }
                    continue;
                }
            };
            drop(startup_permit);

            let mut optional_state = catalog_refresh::OptionalCapabilityState::default();
            let stopping = tokio::select! {
                _ = shutdown_rx.changed() => true,
                _ = catalog_refresh::refresh_optional(
                    &inner, &server, transport.as_ref(), &mut optional_state,
                    catalog_refresh::CatalogRefreshDemand::OPTIONAL,
                ) => false,
            };
            if stopping {
                shutdown_server_runtime_state_with_transport(&inner, &server.id, Some(transport))
                    .await;
                return;
            }

            // Each connection owns its observation state; reconnect starts a fresh capability generation.
            // 每个连接持有自己的观察状态；重连开启新的能力代次。
            let connected_at = tokio::time::Instant::now();
            loop {
                let demand = tokio::select! {
                    _ = shutdown_rx.changed() => None,
                    result = catalog_refresh::receive_demand(&mut event_rx, &server.name) => Some(result),
                };
                let demand = match demand {
                    None => {
                        shutdown_server_runtime_state_with_transport(
                            &inner,
                            &server.id,
                            Some(transport),
                        )
                        .await;
                        return;
                    }
                    Some(Ok(demand)) => demand,
                    Some(Err(error)) => {
                        if !clear_server_runtime_state(
                            &inner,
                            &server.id,
                            Some(transport),
                            Some(error),
                        )
                        .await
                        {
                            return;
                        }
                        break;
                    }
                };
                let result = tokio::select! {
                    _ = shutdown_rx.changed() => None,
                    result = catalog_refresh::refresh_catalog(
                        &inner, &server, transport.clone(), &mut optional_state, demand,
                    ) => Some(result),
                };
                let Some(result) = result else {
                    shutdown_server_runtime_state_with_transport(
                        &inner,
                        &server.id,
                        Some(transport),
                    )
                    .await;
                    return;
                };
                if let Err(error) = result {
                    if !clear_server_runtime_state(&inner, &server.id, Some(transport), Some(error))
                        .await
                    {
                        return;
                    }
                    break;
                }
            }

            if inner.purpose == McpRuntimePurpose::Managed
                && connected_at.elapsed() >= Duration::from_secs(60)
            {
                consecutive_failures = 0;
            }
            consecutive_failures = consecutive_failures.saturating_add(1);
            update_server_retry_count(&inner, &server.id, consecutive_failures);
            if consecutive_failures >= inner.purpose.maximum_attempts() {
                mark_server_failed(
                    &inner,
                    &server.id,
                    None,
                    format!(
                        "MCP server `{}` exhausted {} permitted connection attempt(s).",
                        server.name,
                        inner.purpose.maximum_attempts()
                    ),
                )
                .await;
                return;
            }
            if wait_for_retry_or_shutdown(
                &mut shutdown_rx,
                retry_delay(&server.id, consecutive_failures),
            )
            .await
            {
                shutdown_server_runtime_state(&inner, &server.id).await;
                break;
            }
        }
    });
}

/// Records startup or discovery failure and stops at the explicitly declared runtime attempt bound.
/// 记录启动或发现失败，并在明确声明的运行时尝试上限处停止。
///
/// # Parameters
/// 参数
/// `inner` owns runtime state, `server` identifies the failure, `consecutive_failures` stores the bounded counter,
/// `unpublished_transport` retains a failed setup candidate, and `error` is the sanitized failure text.
/// `inner` 持有运行时状态，`server` 标识失败，`consecutive_failures` 保存有限计数，
/// `unpublished_transport` 保留失败的初始化候选传输，`error` 是已脱敏失败文本。
///
/// # Returns
/// 返回值
/// Returns true when automatic retry must stop because the failure limit was reached.
/// 达到失败上限、必须停止自动重试时返回 true。
async fn record_server_failure_and_should_stop(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    consecutive_failures: &mut u8,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: String,
) -> bool {
    *consecutive_failures = consecutive_failures.saturating_add(1);
    update_server_retry_count(inner, &server.id, *consecutive_failures);
    if *consecutive_failures >= inner.purpose.maximum_attempts() {
        mark_server_failed(
            inner,
            &server.id,
            unpublished_transport,
            format!(
                "MCP server `{}` exhausted {} permitted connection attempt(s). Last error: {error}",
                server.name,
                inner.purpose.maximum_attempts()
            ),
        )
        .await;
        true
    } else {
        !clear_server_runtime_state(inner, &server.id, unpublished_transport, Some(error)).await
    }
}

/// Calculates bounded exponential reconnect delay with deterministic per-server jitter.
/// 计算带确定性单服务器抖动的有界指数重连延迟。
///
/// # Parameters
/// 参数
/// `server_id` provides a stable jitter seed and `consecutive_failures` selects the 1/2/4/8/16/30-second backoff step.
/// `server_id` 提供稳定抖动种子，`consecutive_failures` 选择 1/2/4/8/16/30 秒退避步骤。
///
/// # Returns
/// 返回值
/// Returns a delay capped at 30 seconds plus no more than 250 milliseconds of jitter.
/// 返回上限为 30 秒且抖动不超过 250 毫秒的延迟。
fn retry_delay(server_id: &str, consecutive_failures: u8) -> Duration {
    const BACKOFF_SECONDS: [u64; 6] = [1, 2, 4, 8, 16, 30];
    let backoff_index = usize::from(consecutive_failures.saturating_sub(1))
        .min(BACKOFF_SECONDS.len().saturating_sub(1));
    let jitter_millis = server_id
        .bytes()
        .fold(u64::from(consecutive_failures), |accumulator, byte| {
            accumulator.wrapping_mul(33).wrapping_add(u64::from(byte))
        })
        % 251;
    Duration::from_secs(BACKOFF_SECONDS[backoff_index])
        .saturating_add(Duration::from_millis(jitter_millis))
}

/// Updates the shared OAuth-session cache for one server after startup or teardown decisions have been made.
/// 在完成启动或拆除决策后，更新单个服务器对应的共享 OAuth 会话缓存。
async fn update_server_auth_session_cache(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    auth_session: Option<Arc<McpOAuthSession>>,
) {
    let mut sessions = inner.auth_sessions.write().await;
    match auth_session {
        Some(session) => {
            sessions.insert(server_id.to_string(), session);
        }
        None => {
            sessions.remove(server_id);
        }
    }
}

/// Waits for the selected bounded reconnect backoff unless the runtime has already been asked to shut down.
/// 在运行时已经收到关闭请求前提下提前退出，否则等待选定的有界重连退避窗口。
///
/// # Parameters
/// 参数
/// `shutdown_rx` observes the latched partition shutdown signal and `delay` is the precomputed exponential backoff with jitter.
/// `shutdown_rx` 观察锁存的分区关闭信号，`delay` 是预先计算并带抖动的指数退避时长。
///
/// # Returns
/// 返回值
/// Returns true when shutdown won the race and false when the retry delay elapsed.
/// 关闭信号先到达时返回 true，重试等待到期时返回 false。
async fn wait_for_retry_or_shutdown(
    shutdown_rx: &mut watch::Receiver<bool>,
    delay: Duration,
) -> bool {
    tokio::select! {
        _ = shutdown_rx.changed() => true,
        _ = tokio::time::sleep(delay) => false,
    }
}

/// Converts event loss into an explicit lifecycle failure instead of trusting a stale catalog.
/// 将事件丢失转为明确生命周期失败，不再信任过期目录。
async fn receive_server_runtime_event(
    receiver: &mut tokio::sync::broadcast::Receiver<McpTransportEvent>,
    server_name: &str,
) -> Result<McpTransportEvent, String> {
    receiver
        .recv()
        .await
        .map_err(|error| event_channel_error(error, server_name))
}

/// Keeps blocking and nonblocking event reads on the same explicit gap/reconnection contract.
/// 让阻塞与非阻塞事件读取遵循同一明确缺口及重连契约。
fn event_channel_error(
    error: tokio::sync::broadcast::error::RecvError,
    server_name: &str,
) -> String {
    match error {
        tokio::sync::broadcast::error::RecvError::Lagged(skipped) => format!(
            "MCP server `{server_name}` event stream lagged by {skipped} events; reconnecting to restore catalog consistency."
        ),
        tokio::sync::broadcast::error::RecvError::Closed => {
            format!("MCP server `{server_name}` event stream closed.")
        }
    }
}

/// Subscribes before initial discovery so notifications during the read remain observable.
/// 在首次发现前订阅，使读取期间的通知仍可被观察。
async fn discover_server_tools_with_events(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<tokio::sync::broadcast::Receiver<McpTransportEvent>, String> {
    let receiver = transport.event_receiver();
    refresh_server_tools(inner, server, transport).await?;
    Ok(receiver)
}

/// Refreshes one server tool snapshot after the connection is established or a tool-list change arrives.
/// 在连接建立后或收到工具列表变化通知时刷新单个服务器的工具快照。
async fn refresh_server_tools(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<(), String> {
    let schemas = tokio::time::timeout(Duration::from_secs(10), transport.list_tools())
        .await
        .map_err(|_| {
            format!(
                "MCP server `{}` tools/list timed out after 10 seconds.",
                server.name
            )
        })?
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` 工具列表失败：{}。",
                server.name, error
            )
        })?;
    validate_mcp_tool_schema_list(&server.name, &schemas)?;
    let tools = schemas
        .into_iter()
        .map(|schema| {
            McpRuntimeToolDescriptor::new(
                server.id.clone(),
                schema.name,
                schema.description,
                schema.args_schema,
            )
            .with_result_schema(schema.result_schema)
        })
        .collect::<Vec<_>>();

    inner
        .transports
        .write()
        .expect("mcp runtime transport map poisoned")
        .insert(server.id.clone(), transport);
    update_server_status(
        inner,
        &server.id,
        McpRuntimeServerStatus::Connected,
        None,
        tools,
    );
    Ok(())
}

/// Validates one complete MCP tool list before any part of it replaces the last successful snapshot.
/// 在完整 MCP 工具列表的任何部分替换上次成功快照前对其进行校验。
///
/// # Parameters
/// 参数
/// `server_name` identifies the diagnostic owner and `schemas` is the exact complete list returned by one `tools/list` response.
/// `server_name` 标识诊断归属，`schemas` 是单次 `tools/list` 响应返回的精确完整列表。
///
/// # Returns
/// 返回值
/// Returns success only when names are non-empty and unique and every declared input/output JSON Schema is structurally valid.
/// 仅当名称非空且唯一，并且所有已声明输入/输出 JSON Schema 结构有效时返回成功。
fn validate_mcp_tool_schema_list(
    server_name: &str,
    schemas: &[vulcan_mcp_runtime::McpToolSchema],
) -> Result<(), String> {
    // occupiedNames prevents ambiguous invocation ownership within one server snapshot.
    // occupiedNames 防止单个服务器快照内出现含糊的调用归属。
    let mut occupied_names = BTreeSet::new();
    for schema in schemas {
        let tool_name = schema.name.trim();
        if tool_name.is_empty() {
            return Err(format!(
                "MCP server `{server_name}` returned a tool with an empty name."
            ));
        }
        if !occupied_names.insert(tool_name) {
            return Err(format!(
                "MCP server `{server_name}` returned duplicate tool name `{tool_name}`."
            ));
        }
        if !schema.args_schema.is_object() {
            return Err(format!(
                "MCP tool `{server_name}/{tool_name}` input schema must be a JSON object schema."
            ));
        }
        validate_mcp_schema_document(server_name, tool_name, "input", &schema.args_schema)?;
        if !schema.result_schema.is_null() {
            validate_mcp_schema_document(server_name, tool_name, "output", &schema.result_schema)?;
        }
    }
    Ok(())
}

/// Validates and compiles one untrusted MCP tool schema as fixed JSON Schema Draft 2020-12.
/// 按固定 JSON Schema Draft 2020-12 校验并编译单个不受信任的 MCP 工具 Schema。
///
/// # Parameters
/// 参数
/// `server_name` and `tool_name` identify diagnostics, `schema_role` identifies input or output, and `schema` is the complete schema document.
/// `server_name` 与 `tool_name` 标识诊断，`schema_role` 标识输入或输出，`schema` 是完整 Schema 文档。
///
/// # Returns
/// 返回值
/// Returns success only after fixed-draft meta-validation, controlled-reference validation, and compilation all succeed.
/// 仅在固定方言元校验、受控引用校验与编译全部成功后返回成功。
fn validate_mcp_schema_document(
    server_name: &str,
    tool_name: &str,
    schema_role: &str,
    schema: &serde_json::Value,
) -> Result<(), String> {
    jsonschema::draft202012::meta::validate(schema).map_err(|error| {
        format!("MCP tool `{server_name}/{tool_name}` {schema_role} schema is invalid: {error}")
    })?;
    validate_mcp_schema_references(server_name, tool_name, schema_role, schema)?;
    jsonschema::draft202012::new(schema).map_err(|_| {
        format!(
            "MCP tool `{server_name}/{tool_name}` {schema_role} schema could not be compiled as JSON Schema Draft 2020-12."
        )
    })?;
    Ok(())
}

/// Rejects schema references that would require resolution outside the received MCP schema document.
/// 拒绝需要在收到的 MCP Schema 文档之外解析的 Schema 引用。
///
/// # Parameters
/// 参数
/// `server_name` and `tool_name` identify the diagnostic owner, `schema_role` identifies input or output, and `schema` is the exact untrusted schema document.
/// `server_name` 与 `tool_name` 标识诊断归属，`schema_role` 标识输入或输出，`schema` 是精确的未受信任 Schema 文档。
///
/// # Returns
/// 返回值
/// Returns success only when every reference-bearing keyword contains a local fragment reference beginning with `#`.
/// 仅当每个带引用语义的关键字都包含以 `#` 开头的本地片段引用时返回成功。
fn validate_mcp_schema_references(
    server_name: &str,
    tool_name: &str,
    schema_role: &str,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let Some(schema_object) = schema.as_object() else {
        return Ok(());
    };
    for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
        if let Some(value) = schema_object.get(keyword) {
            let is_local_fragment = value
                .as_str()
                .is_some_and(|reference| reference.starts_with('#'));
            if !is_local_fragment {
                // Never include the reference value because an untrusted URI may embed credentials.
                // 绝不包含引用值，因为不受信任的 URI 可能嵌入凭据。
                return Err(format!(
                    "MCP tool `{server_name}/{tool_name}` {schema_role} schema contains an external or invalid `{keyword}`; only local fragment references are allowed."
                ));
            }
        }
    }

    // directSchemaKeywords contains keywords whose value is exactly one child schema.
    // directSchemaKeywords 包含值恰好为单个子 Schema 的关键字。
    for keyword in [
        "additionalProperties",
        "contains",
        "contentSchema",
        "else",
        "if",
        "items",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(child_schema) = schema_object.get(keyword) {
            validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
        }
    }

    // schemaMapKeywords contains keywords whose object values are independently evaluated child schemas.
    // schemaMapKeywords 包含对象值会作为独立子 Schema 求值的关键字。
    for keyword in [
        "$defs",
        "dependentSchemas",
        "patternProperties",
        "properties",
    ] {
        if let Some(children) = schema_object
            .get(keyword)
            .and_then(serde_json::Value::as_object)
        {
            for child_schema in children.values() {
                validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
            }
        }
    }

    // schemaArrayKeywords contains applicators whose array items are child schemas.
    // schemaArrayKeywords 包含数组元素均为子 Schema 的应用器关键字。
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(children) = schema_object
            .get(keyword)
            .and_then(serde_json::Value::as_array)
        {
            for child_schema in children {
                validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
            }
        }
    }

    Ok(())
}

/// Combined prompt/resource refresh result ready to apply to one connected server snapshot.
/// 可应用到单个已连接服务器快照的 prompt/resource 组合刷新结果。
#[derive(Debug, PartialEq, Eq)]
struct McpOptionalCapabilityRefreshOutcome {
    /// Latest prompt count when prompt discovery succeeded; failure preserves the prior snapshot count.
    /// prompt 发现成功时的最新计数；失败时保留快照中的既有计数。
    prompt_count: Option<usize>,
    /// Latest resource count when resource discovery succeeded; failure preserves the prior snapshot count.
    /// resource 发现成功时的最新计数；失败时保留快照中的既有计数。
    resource_count: Option<usize>,
    /// Combined current refresh warning, or `None` when both capabilities refreshed successfully.
    /// 当前组合刷新警告；两项能力均刷新成功时为 `None`。
    warning: Option<String>,
}

/// Builds one optional-capability outcome without converting failed discovery into zero counts.
/// 构建一份可选能力刷新结果，且不把发现失败转换成零计数。
///
/// # Parameters
/// 参数
/// `prompt_result` contains the current prompt count or its exact refresh failure.
/// `prompt_result` 包含当前 prompt 计数或其精确刷新失败。
/// `resource_result` contains the current resource count or its exact refresh failure.
/// `resource_result` 包含当前 resource 计数或其精确刷新失败。
///
/// # Returns
/// 返回值
/// Successful counts plus every current failure combined into one optional warning.
/// 返回成功计数以及合并全部当前失败后的可选警告。
fn build_server_optional_capability_refresh_outcome(
    prompt_result: Result<usize, String>,
    resource_result: Result<usize, String>,
) -> McpOptionalCapabilityRefreshOutcome {
    // errors retains every independent capability failure for one observable snapshot warning.
    // errors 保留每项独立能力失败，以生成单条可观察快照警告。
    let mut errors = Vec::new();
    // promptCount updates the snapshot only when prompt discovery returned authoritative data.
    // promptCount 仅在 prompt 发现返回权威数据时更新快照。
    let prompt_count = match prompt_result {
        Ok(count) => Some(count),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    // resourceCount updates the snapshot only when resource discovery returned authoritative data.
    // resourceCount 仅在 resource 发现返回权威数据时更新快照。
    let resource_count = match resource_result {
        Ok(count) => Some(count),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    // warning is absent only when both current capability refreshes succeeded.
    // warning 仅在当前两项能力刷新均成功时缺席。
    let warning = if errors.is_empty() {
        None
    } else {
        Some(errors.join(" "))
    };
    McpOptionalCapabilityRefreshOutcome {
        prompt_count,
        resource_count,
        warning,
    }
}

/// Loads the current prompt count for one server or returns its exact discovery failure.
/// 加载单个服务器的当前 prompt 计数，失败时返回精确发现错误。
///
/// # Parameters
/// 参数
/// `server` supplies the stable human-readable server identity for diagnostics.
/// `server` 提供诊断使用的稳定人类可读服务器身份。
/// `transport` is the active transport queried for prompt discovery.
/// `transport` 是用于 prompt 发现的活动传输。
///
/// # Returns
/// 返回值
/// The authoritative prompt count, or a contextualized transport/protocol error.
/// 返回权威 prompt 计数，或带上下文的传输/协议错误。
async fn load_server_prompt_count(
    server: &McpServerConfig,
    transport: &dyn McpTransport,
) -> Result<usize, String> {
    run_bounded_optional_capability_discovery(
        server,
        "prompt",
        MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
        transport.list_prompts(),
    )
    .await
    .map(|prompts| prompts.len())
}

/// Loads the current resource count for one server or returns its exact discovery failure.
/// 加载单个服务器的当前 resource 计数，失败时返回精确发现错误。
///
/// # Parameters
/// 参数
/// `server` supplies the stable human-readable server identity for diagnostics.
/// `server` 提供诊断使用的稳定人类可读服务器身份。
/// `transport` is the active transport queried for resource discovery.
/// `transport` 是用于 resource 发现的活动传输。
///
/// # Returns
/// 返回值
/// The authoritative resource count, or a contextualized transport/protocol error.
/// 返回权威 resource 计数，或带上下文的传输/协议错误。
async fn load_server_resource_count(
    server: &McpServerConfig,
    transport: &dyn McpTransport,
) -> Result<usize, String> {
    run_bounded_optional_capability_discovery(
        server,
        "resource",
        MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
        transport.list_resources(),
    )
    .await
    .map(|resources| resources.len())
}

/// Runs one optional MCP capability discovery request under an immutable host-owned deadline.
/// 在不可变的宿主持有期限内运行单个 MCP 可选能力发现请求。
///
/// # Parameters
/// 参数
/// `server` provides stable diagnostics, `capability` names the protocol surface, `maximum_wait` is the host deadline,
/// and `request` is the exact transport future being bounded.
/// `server` 提供稳定诊断，`capability` 命名协议表面，`maximum_wait` 是宿主期限，
/// `request` 是被限制的精确传输 Future。
///
/// # Returns
/// 返回值
/// Returns the authoritative response or a contextual transport/timeout diagnostic without blocking lifecycle shutdown indefinitely.
/// 返回权威响应或带上下文的传输/超时诊断，且不会无限阻塞生命周期关闭。
async fn run_bounded_optional_capability_discovery<T, F>(
    server: &McpServerConfig,
    capability: &str,
    maximum_wait: Duration,
    request: F,
) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, vulcan_mcp_runtime::McpRuntimeError>>,
{
    tokio::time::timeout(maximum_wait, request)
        .await
        .map_err(|_| {
            format!(
                "MCP server `{}` {capability} discovery timed out after {} seconds.",
                server.name,
                maximum_wait.as_secs()
            )
        })?
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` {capability} 列表失败：{error}。",
                server.name
            )
        })
}

/// Removes and explicitly shuts down one server transport before publishing its disconnected state.
/// 移除并显式关闭单个服务器传输，然后发布其断开状态。
///
/// # Parameters
/// 参数
/// `inner` owns registered transports and snapshots, `server_id` selects one server,
/// `unpublished_transport` retains a just-connected transport that failed before registration, and `error` is the primary diagnostic.
/// `inner` 持有已注册传输与快照，`server_id` 选择单个服务器，
/// `unpublished_transport` 保留注册前失败的刚连接传输，`error` 是主要诊断。
///
/// Returns whether cleanup succeeded and another connection attempt is permitted.
/// 返回清理是否成功及是否允许再次尝试连接。
async fn clear_server_runtime_state(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: Option<String>,
) -> bool {
    // Cleanup must finish before another instance may be started for this server.
    // 必须完成清理后，才允许为此服务器启动另一实例。
    let (recent_logs, shutdown_error) = remove_and_shutdown_server_transport(
        inner,
        server_id,
        unpublished_transport,
        error.clone(),
    )
    .await;
    let cleanup_succeeded = shutdown_error.is_none();
    update_server_status(
        inner,
        server_id,
        if cleanup_succeeded {
            McpRuntimeServerStatus::Disconnected
        } else {
            McpRuntimeServerStatus::Failed
        },
        combine_mcp_runtime_errors(error, shutdown_error),
        Vec::new(),
    );
    update_server_recent_logs(inner, server_id, recent_logs);
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
    cleanup_succeeded
}

/// Gracefully shuts down one registered transport before publishing the terminal stopped state.
/// 在发布终止的已停止状态前优雅关闭单个已注册传输。
///
/// # Parameters
/// 参数
/// `inner` owns the transport and snapshot while `server_id` selects the exact server task.
/// `inner` 持有传输与快照，`server_id` 选择精确服务器任务。
async fn shutdown_server_runtime_state(inner: &Arc<McpRuntimeInner>, server_id: &str) {
    shutdown_server_runtime_state_with_transport(inner, server_id, None).await;
}

/// Stops a server and also owns a transport that may not yet be in the published map.
/// 停止服务器，同时持有可能尚未进入已发布映射的传输。
///
/// # Parameters
/// 参数
/// `inner` owns runtime state, `server_id` identifies the server, and `unpublished_transport`
/// retains the exact setup candidate when shutdown wins during discovery.
/// `inner` 持有运行时状态，`server_id` 标识服务器，`unpublished_transport`
/// 在发现阶段关闭先到达时保留精确的设置候选者。
///
/// # Returns
/// 返回值
/// Returns only after final transport teardown and stopped-state publication; callers bound their own observation.
/// 仅在最终传输拆除及已停止状态发布后返回；调用方限制自身观测期限。
async fn shutdown_server_runtime_state_with_transport(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
) {
    let (recent_logs, shutdown_error) =
        remove_and_shutdown_server_transport(inner, server_id, unpublished_transport, None).await;
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Stopped,
        shutdown_error,
        Vec::new(),
    );
    update_server_recent_logs(inner, server_id, recent_logs);
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
}

/// Clears one server transport and publishes a terminal bounded-retry failure.
/// 清除单个服务器传输并发布终止性的有限重试失败状态。
///
/// # Parameters
/// 参数
/// `inner` owns transport state, `server_id` identifies the server, `unpublished_transport` retains a failed setup candidate,
/// and `error` is the final sanitized diagnostic.
/// `inner` 持有传输状态，`server_id` 标识服务器，`unpublished_transport` 保留失败的初始化候选传输，
/// `error` 是最终已脱敏诊断。
///
/// # Returns
/// 返回值
/// Returns no value; the server stays failed until a new runtime partition is explicitly published.
/// 不返回值；服务器会保持失败，直到显式发布新的运行时分区。
async fn mark_server_failed(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: String,
) {
    let (recent_logs, shutdown_error) = remove_and_shutdown_server_transport(
        inner,
        server_id,
        unpublished_transport,
        Some(error.clone()),
    )
    .await;
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Failed,
        combine_mcp_runtime_errors(Some(error), shutdown_error),
        Vec::new(),
    );
    if !recent_logs.is_empty() {
        update_server_recent_logs(inner, server_id, recent_logs);
    }
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
}

/// Removes one registered or unpublished transport and waits for its original final shutdown result.
/// 移除单个已注册或尚未发布的传输，等待其原最终关闭结果。
///
/// # Parameters
/// 参数
/// `inner` owns the transport registry, `server_id` selects one registered transport, and
/// `unpublished_transport` supplies the exact setup candidate when registration never completed.
/// `inner` 持有传输注册表，`server_id` 选择单个已注册传输，
/// `unpublished_transport` 在注册未完成时提供精确的初始化候选传输。
/// `primary_error` preserves the initiating failure while cleanup remains pending.
/// `primary_error` 在清理尚未完成期间保留触发清理的失败。
///
/// # Returns
/// 返回值
/// Returns the bounded sanitized log snapshot and an optional shutdown diagnostic.
/// 返回有限脱敏日志快照与可选关闭诊断。
async fn remove_and_shutdown_server_transport(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    primary_error: Option<String>,
) -> (Vec<String>, Option<String>) {
    /// Actual ownership is either the sole original producer or an observer of that same result.
    /// 实际所有权为唯一原产出者，或相同结果的观察者。
    enum CleanupOwnership {
        /// Observe the same original operation without taking another native resource owner.
        /// 观察相同原操作，不取得另一个原生资源所有者。
        Observer(McpActualCleanupObservation),
        /// Produce one actual protocol and native-resource cleanup result.
        /// 产出单个实际协议及原生资源清理结果。
        Original {
            /// Original registered or unpublished transport is consumed by this producer.
            /// 此产出者消费原已注册或未发布传输。
            transport: Option<Arc<dyn McpTransport>>,
            /// The independent receipt exists even when initialization returned no transport.
            /// 即使初始化未返回传输，独立回执仍存在。
            release: Option<vulcan_mcp_runtime::McpResourceRelease>,
            /// Only this exact producer may publish the original operation's actual final result.
            /// 只有此精确产出者可以发布原操作实际最终结果。
            completion: cleanup_receipts::McpCleanupCompletionWriter,
        },
    }
    // Observers must dispose any duplicate Arc before waiting, especially when SSE owns a client in Self.
    // 观察者必须在等待前销毁重复 Arc，尤其是 SSE 在自身拥有客户端时。
    let mut unpublished_transport = unpublished_transport;
    // Register the original operation under the same live lock order before removing its transport.
    // 移除传输前，按照相同实时锁顺序登记原操作。
    let ownership = {
        let mut transports = inner
            .transports
            .write()
            .expect("mcp runtime transport map poisoned");
        let mut inputs = inner
            .launch_inputs
            .write()
            .expect("MCP actual launch input map poisoned");
        let releases = inner
            .resource_releases
            .read()
            .expect("MCP launch release map poisoned");
        let mut observations = inner
            .cleanup_observations
            .write()
            .expect("MCP actual cleanup observation map poisoned");
        if let Some(observation) = observations.get(server_id) {
            CleanupOwnership::Observer(observation.clone())
        } else {
            let release = releases.get(server_id).cloned();
            let original_transport = transports
                .get(server_id)
                .or(unpublished_transport.as_ref())
                .map(Arc::downgrade);
            let (observation, completion) =
                McpActualCleanupObservation::begin(original_transport, release.clone());
            observations.insert(server_id.to_owned(), observation);
            // Removing admission also revokes capture; the sealed original observation is already retained.
            // 移除入场同时撤销捕获；密封原观察已经保留。
            let transport = transports
                .remove(server_id)
                .or(unpublished_transport.take());
            inputs.remove(server_id);
            CleanupOwnership::Original {
                transport,
                release,
                completion,
            }
        }
    };
    drop(unpublished_transport);
    let (transport, release, completion) = match ownership {
        CleanupOwnership::Original {
            transport,
            release,
            completion,
        } => (transport, release, completion),
        CleanupOwnership::Observer(observation) => {
            return match observation.wait().await {
                Ok(logs) => (logs, None),
                Err((logs, error)) => (logs, Some(error)),
            };
        }
    };
    // One result covers protocol shutdown and the same original final native release.
    // 单个结果覆盖协议关闭及相同原最终原生释放。
    let actual_result: cleanup_receipts::McpActualCleanupResult = async {
        if transport.is_none() && release.is_none() {
            return Err((
                Vec::new(),
                String::from("MCP cleanup has no original transport or release receipt."),
            ));
        }
        update_server_status(
            inner,
            server_id,
            McpRuntimeServerStatus::Stopping,
            primary_error,
            Vec::new(),
        );
        update_server_prompt_count(inner, server_id, 0);
        update_server_resource_count(inner, server_id, 0);
        // Close errors stay errors even if native disposal later finishes independently.
        // 即使原生销毁之后独立完成，关闭错误仍保持错误。
        let recent_logs = if let Some(transport) = transport {
            let shutdown_result = transport.shutdown_and_wait().await;
            let recent_logs = transport.recent_logs();
            // SSE retains its client in Self; release this original Arc before waiting on native disposal.
            // SSE 在自身保留客户端；等待原生销毁前释放此原 Arc。
            drop(transport);
            if let Err(error) = shutdown_result {
                return Err((recent_logs, error.to_string()));
            }
            recent_logs
        } else {
            Vec::new()
        };
        if let Some(release) = release {
            if let Err(error) = release.wait().await {
                return Err((recent_logs, error.to_string()));
            }
            // Only the actual positive original release permits removal of its active receipt.
            // 仅实际正向原释放允许移除其活动回执。
            inner
                .resource_releases
                .write()
                .expect("MCP launch release map poisoned")
                .remove(server_id);
        }
        Ok(recent_logs)
    }
    .await;
    // This is the sole positive observation writer; no snapshot or missing map entry constructs success.
    // 此处是唯一正向观察写入点；任何快照或缺失映射条目都不构造成功。
    completion.complete(actual_result.clone());
    match actual_result {
        Ok(logs) => (logs, None),
        Err((logs, error)) => (logs, Some(error)),
    }
}

/// Combines one primary lifecycle error with a later transport shutdown failure without losing either cause.
/// 合并一个主要生命周期错误与后续传输关闭失败，且不丢失任一原因。
///
/// # Parameters
/// 参数
/// `primary_error` is the operation failure and `shutdown_error` is the bounded cleanup failure.
/// `primary_error` 是操作失败，`shutdown_error` 是有限清理失败。
///
/// # Returns
/// 返回值
/// Returns no diagnostic when both inputs are absent, otherwise one complete ordered diagnostic.
/// 两项输入均不存在时不返回诊断，否则返回一条完整有序诊断。
fn combine_mcp_runtime_errors(
    primary_error: Option<String>,
    shutdown_error: Option<String>,
) -> Option<String> {
    match (primary_error, shutdown_error) {
        (Some(primary), Some(shutdown)) => Some(format!(
            "{primary}; MCP transport shutdown also failed: {shutdown}"
        )),
        (Some(primary), None) => Some(primary),
        (None, Some(shutdown)) => Some(shutdown),
        (None, None) => None,
    }
}

/// Replaces the retained sanitized transport diagnostics for one server snapshot.
/// 替换单个服务器快照中保留的脱敏传输诊断。
///
/// # Parameters
/// 参数
/// `inner` owns the shared snapshot, `server_id` selects the server, and `recent_logs` is already bounded and sanitized by the transport.
/// `inner` 持有共享快照，`server_id` 选择服务器，`recent_logs` 已由传输执行有界与脱敏处理。
fn update_server_recent_logs(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    recent_logs: Vec<String>,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.recent_logs = recent_logs;
    }
}

/// Updates one server snapshot in-place with the latest runtime status, warning, and tool set.
/// 使用最新运行时状态、警告与工具集合原地更新单个服务器快照。
fn update_server_status(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    status: McpRuntimeServerStatus,
    error: Option<String>,
    tools: Vec<McpRuntimeToolDescriptor>,
) {
    // error is sanitized against the exact server configuration before entering shared observable state.
    // error 在进入共享可观察状态前，依据精确服务器配置完成脱敏。
    let error = error.map(|error| {
        inner
            .registry
            .server_by_id(server_id)
            .map_or(error.clone(), |server| server.sanitize_diagnostic(&error))
    });
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.status = status;
        server.last_error = error;
        if status == McpRuntimeServerStatus::Connected {
            server.inspection_generation = next_inspection_generation();
            server.cached_tools = tools.clone();
            server.tools = tools;
        } else {
            server.tools = tools;
        }
    }
}

/// Applies one complete optional-capability refresh outcome atomically to a connected server snapshot.
/// 将一份完整可选能力刷新结果原子应用到已连接服务器快照。
///
/// # Parameters
/// 参数
/// `inner` owns the shared runtime snapshot.
/// `inner` 持有共享运行时快照。
/// `server_id` identifies the exact server snapshot to update.
/// `server_id` 标识需要更新的精确服务器快照。
/// `outcome` contains every successful current count and the combined current warning.
/// `outcome` 包含全部成功的当前计数与组合当前警告。
///
/// # Returns
/// 返回值
/// Returns no value; a missing server entry remains unchanged.
/// 不返回值；服务器条目缺失时保持不变。
fn apply_server_optional_capability_refresh_outcome(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    outcome: McpOptionalCapabilityRefreshOutcome,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        if let Some(prompt_count) = outcome.prompt_count {
            server.prompt_count = prompt_count;
        }
        if let Some(resource_count) = outcome.resource_count {
            server.resource_count = resource_count;
        }
        server.last_error = outcome.warning;
    }
}

/// Updates one server snapshot in-place with the latest prompt count.
/// 使用最新 prompt 数量原地更新单个服务器快照。
fn update_server_prompt_count(inner: &Arc<McpRuntimeInner>, server_id: &str, prompt_count: usize) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.prompt_count = prompt_count;
    }
}

/// Updates one server snapshot in-place with the latest resource count.
/// 使用最新资源数量原地更新单个服务器快照。
fn update_server_resource_count(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    resource_count: usize,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.resource_count = resource_count;
    }
}

/// Updates the observable consecutive retry count for one exact server.
/// 更新单个精确服务器的可观察连续重试次数。
///
/// # Parameters
/// 参数
/// `inner` owns the retry map, `server_id` selects one configured server, and `retry_count` is the authoritative current value.
/// `inner` 持有重试映射，`server_id` 选择单个已配置服务器，`retry_count` 是权威当前值。
///
/// # Returns
/// 返回值
/// Returns no value; missing identifiers are inserted only for runtime-owned configured servers.
/// 不返回值；缺失标识仅会为运行时持有的已配置服务器插入。
fn update_server_retry_count(inner: &Arc<McpRuntimeInner>, server_id: &str, retry_count: u8) {
    inner
        .retry_counts
        .write()
        .expect("mcp runtime retry count map poisoned")
        .insert(server_id.to_string(), retry_count);
}

/// Builds the initial snapshot for one configured MCP server before any connection attempt begins.
/// 在任何连接尝试开始之前，为单个已配置 MCP 服务器构建初始快照。
fn initial_server_snapshot(server: &McpServerConfig) -> McpRuntimeServerSnapshot {
    McpRuntimeServerSnapshot {
        inspection_generation: next_inspection_generation(),
        name: server.name.clone(),
        enabled: server.enabled,
        transport_label: server.transport_kind().label().to_string(),
        endpoint_summary: server.command_summary(),
        status: if server.enabled {
            McpRuntimeServerStatus::Connecting
        } else {
            McpRuntimeServerStatus::Disabled
        },
        last_error: None,
        recent_logs: Vec::new(),
        tools: Vec::new(),
        cached_tools: Vec::new(),
        prompt_count: 0,
        resource_count: 0,
    }
}

#[cfg(test)]
mod shutdown_tests;

#[cfg(test)]
mod optional_capability_tests {
    //! MCP optional-capability refresh outcome tests.
    //! MCP 可选能力刷新结果测试。

    use super::*;
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;
    use tokio::sync::broadcast;
    use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema};

    /// A transport may notify during tools/list; subscribing afterwards loses this last notification.
    /// 传输可能在 tools/list 期间通知；之后才订阅会丢失最后一次通知。
    #[tokio::test]
    async fn initial_discovery_retains_notifications_emitted_during_the_read() {
        struct NotifyingDiscoveryTransport {
            events: broadcast::Sender<McpTransportEvent>,
        }
        #[async_trait]
        impl McpTransport for NotifyingDiscoveryTransport {
            fn endpoint(&self) -> &str {
                "test://discovery-order"
            }
            fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
                self.events.subscribe()
            }
            async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
                self.events
                    .send(McpTransportEvent::ToolsChanged)
                    .expect("the lifecycle must subscribe before initial discovery");
                Ok(Vec::new())
            }
            async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
                panic!("discovery must not execute tools")
            }
        }
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: "discovery-order".to_string(),
            name: "Discovery order".to_string(),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: "unused".to_string(),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let inner = empty_runtime_inner();
        let (events, _) = broadcast::channel(4);
        let transport = Arc::new(NotifyingDiscoveryTransport { events });
        let mut receiver = discover_server_tools_with_events(&inner, &server, transport)
            .await
            .expect("valid discovery");
        assert_eq!(
            receiver
                .try_recv()
                .expect("retained discovery notification"),
            McpTransportEvent::ToolsChanged
        );
    }

    /// Losing the final catalog notification must request reconnection without waiting for another event.
    /// 丢失最后一次目录通知后必须要求重连，无需等待下一个事件。
    #[tokio::test]
    async fn event_loss_is_an_explicit_reconnect_failure() {
        let (sender, mut receiver) = broadcast::channel(1);
        sender
            .send(McpTransportEvent::ToolsChanged)
            .expect("first notification");
        sender
            .send(McpTransportEvent::ResourcesChanged)
            .expect("overwrite old notification");
        let error = receive_server_runtime_event(&mut receiver, "gap-fixture")
            .await
            .expect_err("a gap must not remain connected");
        assert!(error.contains("lagged by 1") && error.contains("reconnecting"));
        assert_eq!(
            receive_server_runtime_event(&mut receiver, "gap-fixture")
                .await
                .expect("retained event"),
            McpTransportEvent::ResourcesChanged
        );
        drop(sender);
        assert!(
            receive_server_runtime_event(&mut receiver, "gap-fixture")
                .await
                .expect_err("closed stream")
                .contains("closed")
        );
    }

    /// Verifies aborting an unpublished startup future requests shutdown for every tracked runtime.
    /// 验证中止未发布的启动 Future 会请求关闭所有已跟踪的运行时。
    #[tokio::test]
    async fn cancelled_startup_shuts_down_unpublished_runtime() {
        let runtime = McpRuntimeHandle {
            inner: empty_runtime_inner(),
        };
        let shutdown = runtime.inner.shutdown_tx.subscribe();
        let mut startup_guard = McpRuntimeStartupGuard::new();
        startup_guard.track(&runtime);
        let pending_startup = tokio::spawn(async move {
            let _startup_guard = startup_guard;
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        pending_startup.abort();
        pending_startup
            .await
            .expect_err("startup should be cancelled");
        assert!(
            *shutdown.borrow(),
            "unpublished runtime must receive shutdown"
        );
    }

    /// Waits for one exact Windows test child to disappear from the process table.
    /// 等待一个精确的 Windows 测试子进程从进程表消失。
    ///
    /// # Parameters
    /// 参数
    /// `pid` is the process identity written by the isolated MCP fixture.
    /// `pid` 是隔离 MCP 夹具写出的进程身份。
    #[cfg(windows)]
    async fn wait_for_windows_mcp_child_exit(pid: u32, phase: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                // Process-table inspection is blocking and must not starve the current-thread
                // Tokio runtime responsible for the cancellation and child cleanup tasks.
                // 进程表检查会阻塞，不能饿死负责取消与子进程清理的单线程 Tokio 运行时。
                let output = tokio::task::spawn_blocking(move || {
                    std::process::Command::new("tasklist")
                        .args(["/FO", "CSV", "/NH", "/FI", &format!("PID eq {pid}")])
                        .output()
                        .expect("Windows tasklist should inspect the exact child pid")
                })
                .await
                .expect("Windows process inspection task should finish");
                assert!(output.status.success(), "tasklist should complete");
                if !String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("real MCP child must exit after shutdown during {phase}"));
    }

    /// One real stdio configuration starts one child and stops it with its runtime generation.
    /// 一份真实 stdio 配置只启动一个子进程，并随运行时代次关闭该进程。
    #[cfg(windows)]
    #[tokio::test]
    async fn user_stdio_bootstrap_starts_one_real_child_and_clears_it() {
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .expect("Node.js is required by this workspace test environment");
        let fixture = tempfile::tempdir().expect("isolated MCP process marker");
        let marker = fixture.path().join("starts.txt");
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("owner-mcp.mjs");
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("real-stdio-count"),
            name: String::from("Real stdio count"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("node"),
                args: vec![script.to_string_lossy().into_owned()],
                cwd: None,
                env: BTreeMap::from([(
                    String::from("VULCAN_MCP_PROCESS_MARKER"),
                    marker.to_string_lossy().into_owned(),
                )]),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let access = bootstrap_mcp_runtime_access(vec![server])
            .await
            .expect("one user server should bootstrap");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let inspections = access.server_inspections();
                if inspections.iter().any(|inspection| {
                    inspection.state.id == "real-stdio-count"
                        && inspection.state.status == McpRuntimeServerStatus::Connected
                        && inspection.tools.iter().any(|tool| tool.name == "read_page")
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("real MCP child should connect and discover its tool");
        let process_ids =
            std::fs::read_to_string(&marker).expect("real child should publish its startup marker");
        let process_ids = process_ids.lines().collect::<Vec<_>>();
        assert_eq!(
            process_ids.len(),
            1,
            "one configuration must launch one child"
        );
        let pid = process_ids[0]
            .parse::<u32>()
            .expect("startup marker should be a process id");
        access.shutdown();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if access.server_states().iter().any(|state| {
                    state.id == "real-stdio-count"
                        && state.status == McpRuntimeServerStatus::Stopped
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("real MCP runtime should report stopped");
        wait_for_windows_mcp_child_exit(pid, "normal shutdown").await;
    }

    /// Cancelling either initialize or discovery after child launch tears down the process.
    /// 子进程启动后无论在初始化还是目录发现阶段取消，都会清理该进程。
    #[cfg(windows)]
    #[tokio::test]
    async fn cancelled_plugin_bootstrap_clears_real_child() {
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .expect("Node.js is required by this workspace test environment");
        let fixture = tempfile::tempdir().expect("isolated MCP process marker");
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("owner-mcp.mjs");
        for (phase, stall_variable) in [
            ("initialize", "VULCAN_MCP_STALL_INITIALIZE"),
            ("discovery", "VULCAN_MCP_STALL_DISCOVERY"),
        ] {
            let marker = fixture.path().join(format!("cancelled-{phase}-starts.txt"));
            let discovery_marker = fixture
                .path()
                .join(format!("cancelled-{phase}-discovery.txt"));
            let server = McpServerConfig {
                source: crate::mcp::config::McpServerSource::Plugin {
                    plugin_id: format!("cancelled-real-child-{phase}"),
                    generation: 1,
                },
                id: format!("cancelled-real-child-server-{phase}"),
                name: format!("Cancelled real child {phase}"),
                enabled: true,
                transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                    command: String::from("node"),
                    args: vec![script.to_string_lossy().into_owned()],
                    cwd: None,
                    env: BTreeMap::from([
                        (
                            String::from("VULCAN_MCP_PROCESS_MARKER"),
                            marker.to_string_lossy().into_owned(),
                        ),
                        (
                            String::from("VULCAN_MCP_DISCOVERY_MARKER"),
                            discovery_marker.to_string_lossy().into_owned(),
                        ),
                        (String::from(stall_variable), String::from("1")),
                    ]),
                    env_passthrough: Vec::new(),
                    inherit_environment: true,
                },
            };
            let pending = tokio::spawn(bootstrap_mcp_runtime_access(vec![server]));
            let pid = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Ok(marker_text) = std::fs::read_to_string(&marker)
                        && let Some(pid_text) = marker_text.lines().next()
                    {
                        break pid_text.parse::<u32>().expect("real child pid marker");
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("real child should launch before cancellation");
            if phase == "discovery" {
                tokio::time::timeout(Duration::from_secs(10), async {
                    while !discovery_marker.is_file() {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .expect("real child must enter tools/list before cancellation");
                assert_eq!(
                    mcp_transport_startup_semaphore().available_permits(),
                    MCP_MAX_CONCURRENT_TRANSPORT_STARTS - 1,
                    "first tool discovery must retain its startup permit"
                );
            }
            pending.abort();
            pending
                .await
                .expect_err("plugin bootstrap should be cancelled");
            wait_for_windows_mcp_child_exit(pid, phase).await;
            let starts =
                std::fs::read_to_string(&marker).expect("startup marker should remain readable");
            assert_eq!(
                starts.lines().count(),
                1,
                "cancelled partition must not restart"
            );
        }
    }

    /// Test transport that records every explicit shutdown request.
    /// 记录每次显式关闭请求的测试传输。
    struct ShutdownTrackingTransport {
        /// Shared exact shutdown invocation count.
        /// 共享的精确关闭调用次数。
        shutdown_count: Arc<AtomicUsize>,
        /// Event channel required by the transport contract.
        /// 传输契约要求的事件通道。
        event_tx: broadcast::Sender<McpTransportEvent>,
    }

    /// Test transport whose remote tool invocation always fails.
    /// 远端工具调用始终失败的测试传输。
    struct FailingToolTransport {
        /// Event channel required by the transport contract.
        /// 传输契约要求的事件通道。
        event_tx: broadcast::Sender<McpTransportEvent>,
        /// Deterministic transport failure surfaced to the caller.
        /// 呈现给调用方的确定性传输失败。
        failure: String,
    }

    #[async_trait]
    impl McpTransport for ShutdownTrackingTransport {
        /// Returns the deterministic test endpoint identifier.
        /// 返回确定性的测试端点标识。
        fn endpoint(&self) -> &str {
            "test://shutdown-tracking"
        }

        /// Returns an empty valid tool list for this lifecycle-only fixture.
        /// 为仅测试生命周期的夹具返回空的有效工具列表。
        async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
            Ok(Vec::new())
        }

        /// Subscribes to the fixture event channel.
        /// 订阅夹具事件通道。
        fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
            self.event_tx.subscribe()
        }

        /// Records one explicit host-owned resource teardown.
        /// 记录一次宿主持有的显式资源拆除。
        async fn shutdown(&self) -> Result<(), McpRuntimeError> {
            self.shutdown_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        /// Fails if metadata inspection accidentally crosses the execution boundary.
        /// 元数据检查意外跨越执行边界时使测试失败。
        async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
            panic!("metadata inspection must never execute tools")
        }
    }

    #[async_trait]
    impl McpTransport for FailingToolTransport {
        /// Returns the deterministic test endpoint identifier.
        /// 返回确定性的测试端点标识。
        fn endpoint(&self) -> &str {
            "test://failing-tool"
        }

        /// Returns an empty valid tool list for this failure fixture.
        /// 为该失败夹具返回空的有效工具列表。
        async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
            Ok(Vec::new())
        }

        /// Subscribes to the fixture event channel.
        /// 订阅夹具事件通道。
        fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
            self.event_tx.subscribe()
        }

        /// Accepts one explicit host-owned teardown.
        /// 接纳一次宿主持有的显式拆除。
        async fn shutdown(&self) -> Result<(), McpRuntimeError> {
            Ok(())
        }

        /// Fails with the deterministic transport failure under test.
        /// 以测试中的确定性传输失败返回错误。
        async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
            Err(McpRuntimeError::transport(self.failure.clone()))
        }
    }

    /// Test transport whose tool invocation answers after one deterministic delay.
    /// 工具调用在确定性延迟后返回的测试传输。
    struct SlowToolTransport {
        /// Event channel required by the transport contract.
        /// 传输契约要求的事件通道。
        event_tx: broadcast::Sender<McpTransportEvent>,
        /// Deterministic per-invocation delay.
        /// 确定性的单次调用延迟。
        delay: Duration,
        /// Number of calls currently inside the remote transport.
        /// 当前进入远端传输的调用数量。
        active_calls: Arc<AtomicUsize>,
        /// Highest observed number of concurrent remote calls.
        /// 已观察到的远端并发调用峰值。
        peak_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl McpTransport for SlowToolTransport {
        /// Returns the deterministic test endpoint identifier.
        /// 返回确定性的测试端点标识。
        fn endpoint(&self) -> &str {
            "test://slow-tool"
        }

        /// Returns an empty valid tool list for this fixture.
        /// 为该夹具返回空的有效工具列表。
        async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
            Ok(Vec::new())
        }

        /// Subscribes to the fixture event channel.
        /// 订阅夹具事件通道。
        fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
            self.event_tx.subscribe()
        }

        /// Accepts one explicit host-owned teardown.
        /// 接纳一次宿主持有的显式拆除。
        async fn shutdown(&self) -> Result<(), McpRuntimeError> {
            Ok(())
        }

        /// Answers after the deterministic delay.
        /// 在确定性延迟后返回结果。
        async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
            let active = self.active_calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak_calls.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.active_calls.fetch_sub(1, Ordering::SeqCst);
            Ok(json!({"ok": true}))
        }
    }

    /// Verifies concurrent owner-scoped tools are not serialized by the revocation gate.
    /// 验证并发所有者范围工具不会被撤销闸门串行化。
    ///
    /// The controller executes one tool batch in parallel, so the owner gate must cover only the
    /// revocation check: four concurrent 40 ms calls must cost about one call, not four.
    /// 控制器并行执行同一批工具，因此所有者闸门只能覆盖撤销检查：
    /// 四次并发 40 ms 调用的耗时必须约等于一次，而不是四次。
    #[tokio::test]
    async fn concurrent_owner_scoped_mcp_tools_run_in_parallel() {
        // server identifies one plugin partition so the owner lease can be registered.
        // server 标识一个插件分区，以便注册所有者租约。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::Plugin {
                plugin_id: String::from("parallel"),
                generation: 5,
            },
            id: String::from("parallel:browser"),
            name: String::from("Parallel Browser"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner)
            .expect("unique parallel owner fixture")
            .registry = McpServerRegistry::new(vec![server.clone()]);
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(McpRuntimeToolDescriptor::new(
            "parallel:browser",
            "read_page",
            "Read page",
            json!({"type":"object"}),
        ));
        *inner
            .snapshot
            .write()
            .expect("parallel owner snapshot lock") = McpRuntimeSnapshot {
            servers: BTreeMap::from([(server.id.clone(), snapshot)]),
        };
        let (events, _) = broadcast::channel(1);
        inner
            .transports
            .write()
            .expect("parallel owner transport lock")
            .insert(
                server.id.clone(),
                Arc::new(SlowToolTransport {
                    event_tx: events,
                    delay: Duration::from_millis(40),
                    active_calls: Arc::new(AtomicUsize::new(0)),
                    peak_calls: Arc::new(AtomicUsize::new(0)),
                }),
            );

        let runtime = McpRuntimeHandle { inner };
        let access = McpRuntimeAccess::new(runtime.overview(), BTreeMap::new());
        let owner = crate::model::SessionId::from_u128(77);
        let key = McpRuntimeOwnedPartitionKey::try_new("parallel", 5, owner)
            .expect("valid owner partition key");
        let partition = access.register_owned_stdio_partition_for_test(key, runtime);
        let bindings = partition
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("owner tools should be queryable");

        // single measures one isolated owner-scoped invocation.
        // single 测量一次孤立的所有者范围调用。
        let single_started = Instant::now();
        bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect("single owner call");
        let single_elapsed = single_started.elapsed();
        // concurrent issues four owner-scoped calls at once through the same binding.
        // concurrent 通过同一绑定同时发起四次所有者范围调用。
        let concurrent_started = Instant::now();
        let joined = futures_util::future::join_all([
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
        ])
        .await;
        let concurrent_elapsed = concurrent_started.elapsed();
        assert!(joined.iter().all(|result| result.is_ok()));
        assert!(
            concurrent_elapsed < single_elapsed * 2,
            "concurrent owner-scoped calls must not be serialized by the revocation gate"
        );
    }

    /// Verifies an ordered owner partition never overlaps remote page operations.
    /// 验证有序所有者分区不会使远端页面操作重叠。
    #[tokio::test]
    async fn ordered_owner_scoped_mcp_tools_keep_one_remote_call_at_a_time() {
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::Plugin {
                plugin_id: String::from("ordered"),
                generation: 5,
            },
            id: String::from("ordered:browser"),
            name: String::from("Ordered Browser"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner)
            .expect("unique ordered owner fixture")
            .registry = McpServerRegistry::new(vec![server.clone()]);
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(McpRuntimeToolDescriptor::new(
            "ordered:browser",
            "navigate",
            "Navigate page",
            json!({"type":"object"}),
        ));
        *inner.snapshot.write().expect("ordered owner snapshot lock") = McpRuntimeSnapshot {
            servers: BTreeMap::from([(server.id.clone(), snapshot)]),
        };
        let (events, _) = broadcast::channel(1);
        let active_calls = Arc::new(AtomicUsize::new(0));
        let peak_calls = Arc::new(AtomicUsize::new(0));
        inner
            .transports
            .write()
            .expect("ordered owner transport lock")
            .insert(
                server.id.clone(),
                Arc::new(SlowToolTransport {
                    event_tx: events,
                    delay: Duration::from_millis(15),
                    active_calls: Arc::clone(&active_calls),
                    peak_calls: Arc::clone(&peak_calls),
                }),
            );
        let runtime = McpRuntimeHandle { inner };
        let access = McpRuntimeAccess::new(runtime.overview(), BTreeMap::new());
        let owner = crate::model::SessionId::from_u128(78);
        let key = McpRuntimeOwnedPartitionKey::try_new("ordered", 5, owner)
            .expect("valid ordered owner key");
        let partition =
            access.register_owned_stdio_partition_with_ordering_for_test(key, runtime, true);
        let bindings = partition
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("ordered owner tool should be queryable");
        let joined = futures_util::future::join_all([
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
            bindings[0].invoke_for_owner(owner, json!({})),
        ])
        .await;
        assert!(joined.iter().all(|result| result.is_ok()));
        assert_eq!(active_calls.load(Ordering::SeqCst), 0);
        assert_eq!(peak_calls.load(Ordering::SeqCst), 1);
    }

    /// Verifies full schemas, real connectivity, and metadata-only reads on an isolated transport.
    /// 在隔离传输上验证完整 Schema、真实连接状态与仅元数据读取。
    #[tokio::test]
    async fn mcp_inspection_preserves_schemas_and_never_executes_tools() {
        // server and inner exist only in this test and never start a process or network connection.
        // server 与 inner 仅存在于本测试中，从不启动进程或网络连接。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("inspection"),
            name: String::from("Inspection"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner).expect("unique fixture").registry =
            McpServerRegistry::new(vec![server.clone()]);
        // input and output exercise unknown extension keywords and nested schemas without normalization.
        // input 与 output 使用未知扩展关键字和嵌套 Schema 验证不发生归一化。
        let input = json!({"type":"object", "properties":{"nested":{"type":"array","items":{"$ref":"#/$defs/item"}}}, "$defs":{"item":{"enum":["x","y"]}}, "x-extension":{"preserve":true}});
        let output =
            json!({"oneOf":[{"type":"string"},{"type":"null"}], "description":"original output"});
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(
            McpRuntimeToolDescriptor::new("inspection", "inspect", "original", input.clone())
                .with_result_schema(output.clone()),
        );
        inner
            .snapshot
            .write()
            .expect("snapshot lock")
            .servers
            .insert(server.id.clone(), snapshot);
        let (event_tx, _) = broadcast::channel(1);
        inner.transports.write().expect("transport lock").insert(
            server.id.clone(),
            Arc::new(ShutdownTrackingTransport {
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                event_tx,
            }),
        );
        let handle = McpRuntimeHandle { inner };
        let inspection = handle.server_inspections();
        assert_eq!(
            inspection[0].state.status,
            McpRuntimeServerStatus::Connected
        );
        assert_eq!(inspection[0].tools[0].input_schema, input);
        assert_eq!(inspection[0].tools[0].output_schema, output);
        // A replacement catalog gets a new identity even when the server ID and connectivity remain unchanged.
        // 即使服务器标识与连接状态不变，替换后的目录仍获得新身份。
        let retained_tools = handle.snapshot().servers[&server.id].tools.clone();
        update_server_status(
            &handle.inner,
            &server.id,
            McpRuntimeServerStatus::Connected,
            None,
            retained_tools,
        );
        assert_ne!(
            handle.server_inspections()[0].inspection_generation,
            inspection[0].inspection_generation
        );
        let metadata = handle
            .server_capabilities(&server.id)
            .await
            .expect("list metadata on existing transport");
        assert_eq!(metadata["prompts"], json!([]));
        assert_eq!(metadata["resources"], json!([]));
        handle
            .inner
            .snapshot
            .write()
            .expect("snapshot lock")
            .servers
            .get_mut(&server.id)
            .expect("server")
            .status = McpRuntimeServerStatus::Disconnected;
        assert!(
            handle
                .server_capabilities(&server.id)
                .await
                .expect_err("disconnected reads must not connect")
                .contains("尚未连接")
        );
    }

    /// Verifies every AI-visible MCP call failure carries one stable machine code and English text.
    /// 验证每条 AI 可见 MCP 调用失败都携带稳定机器编码与英文说明。
    #[tokio::test]
    async fn ai_visible_mcp_call_failures_use_stable_codes_and_english_text() {
        // server and inner stay process-free so the three rejection branches are deterministic.
        // server 与 inner 保持无进程状态，使三个拒绝分支保持确定性。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("coded"),
            name: String::from("Coded"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner).expect("unique fixture").registry =
            McpServerRegistry::new(vec![server.clone()]);
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(McpRuntimeToolDescriptor::new(
            "coded",
            "run",
            "Run",
            json!({"type":"object"}),
        ));
        inner
            .snapshot
            .write()
            .expect("snapshot lock")
            .servers
            .insert(server.id.clone(), snapshot);
        let handle = McpRuntimeHandle { inner };

        let unknown_tool_error = handle
            .call_tool_by_id("coded/missing", json!({}))
            .await
            .expect_err("an unknown tool must fail closed");
        assert!(unknown_tool_error.starts_with("mcp_tool_not_found:"));
        assert!(unknown_tool_error.contains("coded/missing"));

        // A connected server without any live transport keeps its own distinct rejection code.
        // 已连接但没有任何活动传输的服务器保持各自独立的拒绝编码。
        let disconnected_error = handle
            .call_tool_by_id("coded/run", json!({}))
            .await
            .expect_err("a missing transport must fail closed");
        assert!(disconnected_error.starts_with("mcp_server_disconnected:"));
        assert!(disconnected_error.contains("coded"));

        // A rejected remote invocation preserves the transport reason behind its own code.
        // 被拒绝的远端调用在自己的编码之后保留传输层原因。
        let (event_tx, _) = broadcast::channel(1);
        handle
            .inner
            .transports
            .write()
            .expect("transport lock")
            .insert(
                server.id.clone(),
                Arc::new(FailingToolTransport {
                    event_tx,
                    failure: String::from("tools/call(stdio): remote rejected"),
                }),
            );
        let invocation_error = handle
            .call_tool_by_id("coded/run", json!({}))
            .await
            .expect_err("a rejected remote invocation must fail closed");
        assert!(invocation_error.starts_with("mcp_tool_invocation_failed:"));
        assert!(invocation_error.contains("tools/call(stdio): remote rejected"));
    }

    /// Builds empty shared MCP runtime state for deterministic teardown tests.
    /// 为确定性拆除测试构建空的共享 MCP 运行时状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns independently owned state with no background server tasks.
    /// 返回不含后台服务器任务的独立状态。
    fn empty_runtime_inner() -> Arc<McpRuntimeInner> {
        let (shutdown_tx, _) = watch::channel(false);
        Arc::new(McpRuntimeInner {
            instance_id: uuid::Uuid::new_v4(),
            registry: McpServerRegistry::new(Vec::new()),
            // This fixture has no actual producer or full published source.
            // 此夹具没有实际生产者或完整已发布来源。
            launch_source: crate::mcp::registry::McpPluginLaunchSource::Unproven,
            purpose: McpRuntimePurpose::Managed,
            business_gate: RwLock::new(McpBusinessGate::Open),
            transports: RwLock::new(BTreeMap::new()),
            launch_inputs: RwLock::new(BTreeMap::new()),
            cleanup_observations: RwLock::new(BTreeMap::new()),
            resource_releases: RwLock::new(BTreeMap::new()),
            // This synthetic registry starts no native lifecycle and therefore has an empty released scope.
            // 此合成注册表不启动原生生命周期，因此拥有已释放的空范围。
            partition_release: {
                let (resources, released) = vulcan_mcp_runtime::McpTransportResources::new(());
                drop(resources);
                released
            },
            auth_sessions: AsyncRwLock::new(BTreeMap::new()),
            snapshot: RwLock::new(McpRuntimeSnapshot {
                servers: BTreeMap::new(),
            }),
            retry_counts: RwLock::new(BTreeMap::new()),
            shutdown_tx,
        })
    }

    /// Verifies registered and pre-publication transports both receive explicit shutdown before removal completes.
    /// 验证已注册与发布前传输都会在移除完成前收到显式关闭。
    #[tokio::test]
    async fn failed_transport_teardown_is_explicit_for_registered_and_unpublished_instances() {
        // inner owns the exact registered transport map under test.
        // inner 持有测试中的精确已注册传输映射。
        let inner = empty_runtime_inner();
        // registered_count observes the transport that already entered the runtime map.
        // registered_count 观察已经进入运行时映射的传输。
        let registered_count = Arc::new(AtomicUsize::new(0));
        // registered_events keeps the transport event contract alive for the fixture lifetime.
        // registered_events 在夹具生命周期内保持传输事件契约有效。
        let (registered_events, _) = broadcast::channel(1);
        inner
            .transports
            .write()
            .expect("lock registered transport map")
            .insert(
                String::from("registered"),
                Arc::new(ShutdownTrackingTransport {
                    shutdown_count: registered_count.clone(),
                    event_tx: registered_events,
                }),
            );
        remove_and_shutdown_server_transport(&inner, "registered", None, None).await;
        assert_eq!(registered_count.load(Ordering::SeqCst), 1);
        assert!(
            !inner
                .transports
                .read()
                .expect("read registered transport map")
                .contains_key("registered")
        );

        // unpublished_count observes a connection that failed before successful tool publication.
        // unpublished_count 观察在工具成功发布前失败的连接。
        let unpublished_count = Arc::new(AtomicUsize::new(0));
        // unpublished_events keeps the fallback transport event contract valid.
        // unpublished_events 保持回退传输的事件契约有效。
        let (unpublished_events, _) = broadcast::channel(1);
        // unpublished_transport is supplied directly because it never entered the runtime map.
        // unpublished_transport 被直接提供，因为它从未进入运行时映射。
        let unpublished_transport: Arc<dyn McpTransport> = Arc::new(ShutdownTrackingTransport {
            shutdown_count: unpublished_count.clone(),
            event_tx: unpublished_events,
        });
        remove_and_shutdown_server_transport(
            &inner,
            "unpublished",
            Some(unpublished_transport),
            None,
        )
        .await;
        assert_eq!(unpublished_count.load(Ordering::SeqCst), 1);
    }

    /// Verifies an unresponsive optional discovery request cannot hold the server lifecycle task indefinitely.
    /// 验证无响应的可选能力发现请求不能无限占用服务器生命周期任务。
    #[tokio::test]
    async fn optional_capability_discovery_has_a_host_owned_deadline() {
        // server supplies deterministic identity for the timeout diagnostic.
        // server 为超时诊断提供确定性身份。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("hanging-optional"),
            name: String::from("Hanging Optional"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        // hanging_request models a protocol peer that never answers prompt or resource discovery.
        // hanging_request 模拟永不响应 prompt 或 resource 发现的协议对端。
        let hanging_request =
            std::future::pending::<Result<(), vulcan_mcp_runtime::McpRuntimeError>>();
        // error proves the deadline is owned by the host rather than the transport implementation.
        // error 证明期限由宿主持有，而不是依赖传输实现。
        let error = run_bounded_optional_capability_discovery(
            &server,
            "prompt",
            Duration::from_millis(1),
            hanging_request,
        )
        .await
        .expect_err("time out unresponsive optional discovery");
        assert!(error.contains("prompt discovery timed out"));
    }

    /// Builds one tool schema fixture with caller-selected input and output schemas.
    /// 构建一份由调用方选择输入与输出 Schema 的工具 Schema 夹具。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the exact tool name, while `args_schema` and `result_schema` are validated as one list entry.
    /// `name` 是精确工具名，`args_schema` 与 `result_schema` 作为单个列表项接受校验。
    ///
    /// # Returns
    /// 返回值
    /// Returns one host-owned MCP tool schema fixture.
    /// 返回一份宿主持有的 MCP 工具 Schema 夹具。
    fn tool_schema(
        name: &str,
        args_schema: serde_json::Value,
        result_schema: serde_json::Value,
    ) -> McpToolSchema {
        McpToolSchema {
            name: name.to_string(),
            description: String::from("test tool"),
            args_schema,
            result_schema,
        }
    }

    /// Verifies shutdown remains observable when it is requested before any server task subscribes.
    /// 验证关闭请求发生在任何服务器任务订阅前时，未来仍可观察该请求。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if a zero-receiver request is lost or repetition reopens the channel.
    /// 此测试无返回值；若零接收端请求丢失或重复请求重新打开通道则测试失败。
    #[test]
    fn mcp_shutdown_request_is_latched_without_receivers() {
        // shutdownTx owns the monotonic value while initialReceiver proves the zero-receiver setup after drop.
        // shutdownTx 持有单调值，initialReceiver 被丢弃后形成零接收端测试场景。
        let (shutdown_tx, initial_receiver) = watch::channel(false);
        drop(initial_receiver);

        request_mcp_runtime_shutdown(&shutdown_tx);
        // futureReceiver subscribes only after the shutdown request has already been published.
        // futureReceiver 仅在关闭请求已经发布后才创建订阅。
        let future_receiver = shutdown_tx.subscribe();
        assert!(*future_receiver.borrow());

        request_mcp_runtime_shutdown(&shutdown_tx);
        assert!(*future_receiver.borrow());
    }

    /// Verifies latched shutdown cannot replace actual original cleanup and physical resource release.
    /// 验证锁存关闭不能替代实际原清理及物理资源释放。
    #[tokio::test]
    async fn mcp_partition_requires_original_cleanup_and_physical_release() {
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("pending"),
            name: String::from("Pending"),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let runtime = bootstrap_mcp_runtime(vec![server])
            .await
            .expect("build disabled test partition");
        // A real exclusive file lease supplies original physical ownership without replaying any startup.
        // 真实排他文件租约提供原物理所有权，不重放任何启动。
        let witness = tempfile::NamedTempFile::new().expect("original physical witness");
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(witness.path())
            .expect("original exclusive file owner");
        lease.lock().expect("original file lock");
        let (actual_owner, release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
        runtime
            .inner
            .resource_releases
            .write()
            .expect("original release map")
            .insert("pending".into(), release);
        runtime.shutdown();
        assert!(
            !runtime.is_fully_stopped(),
            "a shutdown request must not prove server teardown"
        );
        assert!(
            runtime
                .inner
                .cleanup_observations
                .read()
                .expect("cleanup observations")
                .is_empty()
        );
        let inner = runtime.inner.clone();
        let cleanup =
            tokio::spawn(async move { shutdown_server_runtime_state(&inner, "pending").await });
        runtime
            .wait_until_stopped(Duration::from_millis(10))
            .await
            .expect_err("real original file ownership still prevents completion");
        assert!(matches!(
            witness.as_file().try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(!runtime.is_fully_stopped());
        drop(actual_owner);
        tokio::time::timeout(Duration::from_secs(5), cleanup)
            .await
            .expect("original physical release deadline")
            .expect("original cleanup joined");
        runtime
            .wait_until_stopped(Duration::from_millis(10))
            .await
            .expect("actual positive original cleanup proof");
        assert!(runtime.is_fully_stopped());
        witness
            .as_file()
            .try_lock()
            .expect("actual file released before completion");
        assert_eq!(runtime.snapshot().servers["pending"].last_error, None);
        assert_eq!(
            runtime
                .inner
                .retry_counts
                .read()
                .expect("original retry state")["pending"],
            0,
            "cleanup must not replay disabled startup"
        );
    }

    /// Verifies owner bindings reject foreign callers and remain fail-closed after revocation.
    /// 验证所有者绑定拒绝外部调用者，并在撤销后保持安全失败。
    #[tokio::test]
    async fn owned_stdio_binding_requires_owner_and_stops_after_revoke() {
        // server identifies the exact plugin generation while keeping the test transport disabled for bootstrap.
        // server 标识精确插件代次，同时让测试传输保持禁用以避免引导子进程。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::Plugin {
                plugin_id: String::from("fixture"),
                generation: 11,
            },
            id: String::from("fixture:browser"),
            name: String::from("Fixture Browser"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner)
            .expect("unique owner binding fixture")
            .registry = McpServerRegistry::new(vec![server.clone()]);
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(McpRuntimeToolDescriptor::new(
            "fixture:browser",
            "read_page",
            "Read page",
            json!({"type":"object"}),
        ));
        *inner.snapshot.write().expect("owner binding snapshot lock") = McpRuntimeSnapshot {
            servers: BTreeMap::from([(server.id.clone(), snapshot)]),
        };
        let (events, _) = broadcast::channel(1);
        inner
            .transports
            .write()
            .expect("owner binding transport lock")
            .insert(
                server.id.clone(),
                Arc::new(ShutdownTrackingTransport {
                    shutdown_count: Arc::new(AtomicUsize::new(0)),
                    event_tx: events,
                }),
            );

        let runtime = McpRuntimeHandle { inner };
        // Capture the latched shutdown signal before moving the handle into the owner lease.
        // 在把运行时句柄移动到所有者租约前捕获锁存的关闭信号。
        let shutdown_receiver = runtime.inner.shutdown_tx.subscribe();
        let access = McpRuntimeAccess::new(runtime.overview(), BTreeMap::new());
        let owner = crate::model::SessionId::from_u128(11);
        let foreign_owner = crate::model::SessionId::from_u128(12);
        let key = McpRuntimeOwnedPartitionKey::try_new("fixture", 11, owner)
            .expect("valid owner partition key");
        let partition = access.register_owned_stdio_partition_for_test(key, runtime);
        let bindings = partition
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("owner tools should be queryable");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].id(), "mcp_fixture_browser_read_page");
        assert_eq!(bindings[0].owner_session_id(), Some(owner));
        let following_binding = access
            .owned_stdio_tool_bindings_following_replacement(
                "fixture",
                11,
                owner,
                &McpRuntimeToolAllowlist::default(),
            )
            .expect("browser-style owner binding")
            .remove(0);
        assert!(bindings[0].invoke(json!({})).await.is_err());
        let foreign_error = bindings[0]
            .invoke_for_owner(foreign_owner, json!({}))
            .await
            .expect_err("foreign owner must be rejected");
        assert!(foreign_error.contains("does not match"));
        let direct_foreign_error = access
            .invoke_owned_stdio_tool(
                "fixture",
                11,
                foreign_owner,
                "fixture:browser/read_page",
                json!({}),
            )
            .await
            .expect_err("direct invocation must not fall back to another owner");
        assert!(direct_foreign_error.contains("unavailable"));

        // A browser grant replacement keeps the same owner and plugin lifecycle, so only the
        // explicitly following binding reaches the replacement catalog.
        // 浏览器授权替换保持同一所有者与插件生命周期，因此只有显式跟随的绑定会到达新目录。
        access
            .replace_owned_stdio_partition(
                "fixture",
                11,
                owner,
                vec![McpServerConfig {
                    enabled: false,
                    ..server.clone()
                }],
            )
            .await
            .expect("replace owner grant within one plugin lifecycle");
        let following_replacement_error = following_binding
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("replacement catalog intentionally has no tools");
        assert!(following_replacement_error.contains("mcp_tool_not_found"));
        let strict_replacement_error = bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("ordinary bindings must remain pinned to their original lease");
        assert!(strict_replacement_error.contains("stale"));

        // Plugin drain revokes the captured binding and closes admission for every owner.
        // 插件排空会撤销已捕获绑定，并关闭所有所有者的准入。
        access
            .begin_plugin_owner_drain("fixture")
            .expect("begin plugin owner MCP drain");
        assert!(
            *shutdown_receiver.borrow(),
            "plugin drain must request shutdown before removing the owner lease"
        );
        let drained_binding_error = bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("drained plugin binding must fail closed");
        assert!(drained_binding_error.contains("unavailable"));
        let drained_open_error = access
            .open_owned_stdio_partition("fixture", 11, owner, vec![server.clone()])
            .await
            .expect_err("drained plugin must reject new owner partition");
        assert!(drained_open_error.contains("draining"));
        access
            .finish_plugin_owner_drain("fixture")
            .expect("finish plugin owner MCP drain");
        let reopened_binding_error = bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("a revoked binding must stay stale after admission reopens");
        assert!(reopened_binding_error.contains("unavailable"));
        let reopened_following_error = following_binding
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("a browser-style binding must stay revoked after plugin drain");
        assert!(reopened_following_error.contains("plugin drain"));

        // replacement_server is disabled so the replacement path can prove atomic lease turnover without a child process.
        // replacement_server 保持禁用，使替换路径可以在不启动子进程的情况下验证租约原子切换。
        let replacement_server = McpServerConfig {
            enabled: false,
            ..server.clone()
        };
        let replacement = access
            .replace_owned_stdio_partition("fixture", 11, owner, vec![replacement_server])
            .await
            .expect("replacement owner partition should become ready");
        let stale_binding_error = bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("replacement binding must resolve the current lease");
        assert!(stale_binding_error.contains("stale"));
        let replacement_following_error = following_binding
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("a drained browser-style binding must not follow a new lifecycle");
        assert!(replacement_following_error.contains("plugin drain"));
        let current_lease_error = access
            .invoke_owned_stdio_tool("fixture", 11, owner, "fixture:browser/read_page", json!({}))
            .await
            .expect_err("current replacement lease must resolve its own catalog");
        assert!(current_lease_error.contains("mcp_tool_not_found"));
        replacement
            .shutdown()
            .expect("shutdown replacement owner partition");

        partition.revoke().expect("owner partition revoke");
        let revoked_error = bindings[0]
            .invoke_for_owner(owner, json!({}))
            .await
            .expect_err("binding must fail closed when the current lease is gone");
        assert!(revoked_error.contains("unavailable"));
        assert!(
            partition
                .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                .is_err()
        );
        let direct_revoked_error = access
            .invoke_owned_stdio_tool("fixture", 11, owner, "fixture:browser/read_page", json!({}))
            .await
            .expect_err("direct invocation must not use a revoked lease");
        assert!(direct_revoked_error.contains("unavailable"));
    }

    /// Verifies a replacement generation shares retired-lease safety and blocks uninstall until the old partition is terminally stopped.
    /// 验证替换代次共享退休租约安全状态，并在旧分区最终停止前持续阻止卸载。
    #[tokio::test]
    async fn retired_plugin_partition_remains_usable_until_terminal_stop() {
        // server models one disabled bootstrap partition that is manually advanced to an in-flight connecting state.
        // server 模拟一个停用引导分区，并由测试手动推进到执行中的连接状态。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::Plugin {
                plugin_id: String::from("fixture"),
                generation: 1,
            },
            id: String::from("fixture:server"),
            name: String::from("Fixture"),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        // runtime owns the old generation partition whose terminal state controls package-removal safety.
        // runtime 持有旧代次分区，其最终状态控制插件包删除安全性。
        let runtime = bootstrap_mcp_runtime(vec![server])
            .await
            .expect("build retired plugin partition fixture");
        update_server_status(
            &runtime.inner,
            "fixture:server",
            McpRuntimeServerStatus::Connecting,
            None,
            Vec::new(),
        );
        // previousGeneration owns the execution lease and shares independent cleanup observations.
        // previousGeneration 拥有执行租约并共享独立清理观测。
        let previous_generation = McpRuntimeAccess::new(
            runtime.overview(),
            BTreeMap::from([(String::from("plugin:fixture:1"), runtime.clone())]),
        );
        // replacementGeneration removes the live partition while retaining the same retirement registry.
        // replacementGeneration 移除实时分区，同时保留同一退休注册表。
        let replacement_generation = previous_generation.without_plugin_partition("fixture");
        previous_generation.retire_plugin_partitions_after("fixture", Duration::from_secs(3_600));

        assert!(replacement_generation.has_usable_retired_plugin_partitions("fixture"));
        drop(previous_generation);
        assert!(
            *runtime.inner.shutdown_tx.borrow(),
            "dropping the last execution lease must still request shutdown immediately"
        );
        assert!(
            replacement_generation.has_usable_retired_plugin_partitions("fixture"),
            "losing execution leases must not erase the pending cleanup state"
        );
        runtime.shutdown();
        assert!(
            replacement_generation.has_usable_retired_plugin_partitions("fixture"),
            "a shutdown request alone must not make package removal safe"
        );

        update_server_status(
            &runtime.inner,
            "fixture:server",
            McpRuntimeServerStatus::Stopped,
            Some(String::from("actual transport cleanup remains unconfirmed")),
            Vec::new(),
        );
        assert!(
            replacement_generation.has_usable_retired_plugin_partitions("fixture"),
            "terminal status with a cleanup error must retain the retired partition"
        );
        assert_eq!(
            runtime
                .wait_until_stopped(Duration::from_millis(10))
                .await
                .expect_err("unconfirmed cleanup must fail the stop barrier"),
            "actual transport cleanup remains unconfirmed"
        );

        // A hand-written Stopped snapshot never proves cleanup; this declared-disabled server was never
        // launched, so returning to Disabled is its only legitimate completed state.
        // 手写 Stopped 快照不能证明清理；该声明停用的服务器从未启动，回到 Disabled 是其唯一合法完成状态。
        update_server_status(
            &runtime.inner,
            "fixture:server",
            McpRuntimeServerStatus::Disabled,
            None,
            Vec::new(),
        );
        assert!(runtime.is_fully_stopped());
        // Retain only a weak observation to prove that later retirement releases completed history.
        // 仅保留弱观测，证明后续退役会释放已经完成的历史。
        let completed_runtime = Arc::downgrade(&runtime.inner);
        drop(runtime);
        assert!(completed_runtime.upgrade().is_some());
        // An empty next generation is valid and must still prune the predecessor's completed state.
        // 下一代次可以没有 MCP 服务器，此时仍须清理前驱的已完成状态。
        let next_runtime = bootstrap_mcp_runtime(Vec::new())
            .await
            .expect("empty next plugin partition");
        let next_generation = replacement_generation.with_replaced_plugin_partition(
            "fixture",
            2,
            next_runtime.overview(),
            next_runtime,
        );
        next_generation.retire_plugin_partitions_after("fixture", Duration::from_secs(3_600));
        assert!(
            completed_runtime.upgrade().is_none(),
            "repeated updates must release successfully completed runtime history"
        );
        // The just-retired empty partition is safe only after its last lease requests shutdown.
        // 刚退役的空分区只有在最后租约请求关闭后才安全。
        assert!(replacement_generation.has_usable_retired_plugin_partitions("fixture"));
        drop(next_generation);
        assert!(!replacement_generation.has_usable_retired_plugin_partitions("fixture"));
    }

    /// Verifies successful counts survive beside one failure and dual failures remain combined.
    /// 验证单项失败时成功计数仍被保留，双项失败时两条诊断都会合并。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if errors become zero counts or overwrite each other.
    /// 此测试无返回值；若错误被转换为零计数或相互覆盖则测试失败。
    #[test]
    fn optional_capability_refresh_outcome_preserves_counts_and_all_errors() {
        // partialOutcome preserves the successful resource count while prompt discovery fails.
        // partialOutcome 在 prompt 发现失败时保留成功的 resource 计数。
        let partial_outcome = build_server_optional_capability_refresh_outcome(
            Err(String::from("prompt refresh failed")),
            Ok(4),
        );
        assert_eq!(partial_outcome.prompt_count, None);
        assert_eq!(partial_outcome.resource_count, Some(4));
        assert_eq!(
            partial_outcome.warning.as_deref(),
            Some("prompt refresh failed")
        );

        // failedOutcome combines two independent current capability failures.
        // failedOutcome 合并两项相互独立的当前能力失败。
        let failed_outcome = build_server_optional_capability_refresh_outcome(
            Err(String::from("prompt refresh failed")),
            Err(String::from("resource refresh failed")),
        );
        // warning is the exact combined diagnostic published to the runtime snapshot.
        // warning 是发布到运行时快照的精确组合诊断。
        let warning = failed_outcome
            .warning
            .expect("dual failures should publish one warning");
        assert!(warning.contains("prompt refresh failed"));
        assert!(warning.contains("resource refresh failed"));

        // recoveredOutcome clears the warning only after both current refreshes succeed.
        // recoveredOutcome 仅在当前两项刷新均成功后清除警告。
        let recovered_outcome = build_server_optional_capability_refresh_outcome(Ok(2), Ok(3));
        assert_eq!(recovered_outcome.prompt_count, Some(2));
        assert_eq!(recovered_outcome.resource_count, Some(3));
        assert_eq!(recovered_outcome.warning, None);
    }

    /// Verifies exponential reconnect delay follows the exact bounded sequence with deterministic sub-second jitter.
    /// 验证指数重连延迟遵循精确有界序列，并附带确定性的亚秒抖动。
    #[test]
    fn retry_delay_is_exponential_deterministic_and_capped() {
        let bases = [1_u64, 2, 4, 8, 16, 30, 30];
        for (attempt, base) in (1_u8..=7).zip(bases) {
            let first = retry_delay("plugin-a:server", attempt);
            let second = retry_delay("plugin-a:server", attempt);
            assert_eq!(first, second);
            assert!(first >= Duration::from_secs(base));
            assert!(first <= Duration::from_secs(base) + Duration::from_millis(250));
        }
    }

    /// Verifies a complete tool list accepts local references and rejects duplicates, external references, and structurally invalid schemas.
    /// 验证完整工具列表接受本地引用，并拒绝重复名称、外部引用和结构无效的 Schema。
    #[test]
    fn tool_schema_list_validation_rejects_every_atomicity_hazard() {
        let valid = tool_schema(
            "lookup",
            json!({
                "type": "object",
                "properties": {"query": {"$ref": "#/$defs/query"}},
                "$defs": {"query": {"type": "string"}}
            }),
            json!({"type": "object"}),
        );
        validate_mcp_tool_schema_list("fixture", std::slice::from_ref(&valid))
            .expect("valid complete list");

        let annotation_data = tool_schema(
            "annotation-data",
            json!({
                "type": "object",
                "default": {"$ref": "https://example.test/instance-value"},
                "examples": [{"$dynamicRef": "relative-instance-value"}]
            }),
            serde_json::Value::Null,
        );
        validate_mcp_tool_schema_list("fixture", &[annotation_data])
            .expect("reference-shaped annotation data is not a schema reference");

        let duplicate_error = validate_mcp_tool_schema_list("fixture", &[valid.clone(), valid])
            .expect_err("duplicate names must fail the whole list");
        assert!(duplicate_error.contains("duplicate tool name"));

        let non_object_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema("scalar", json!(true), serde_json::Value::Null)],
        )
        .expect_err("input schema must be an object schema document");
        assert!(non_object_error.contains("must be a JSON object schema"));

        // referenceKeywords covers every JSON Schema keyword whose value can escape the received document.
        // referenceKeywords 覆盖值可能逸出当前接收文档的每个 JSON Schema 引用关键字。
        for reference_keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let external_reference_error = validate_mcp_tool_schema_list(
                "fixture",
                &[tool_schema(
                    "external-reference",
                    json!({
                        "type": "object",
                        "properties": {
                            "secret": {(reference_keyword): "https://user:secret@example.test/schema.json"}
                        }
                    }),
                    serde_json::Value::Null,
                )],
            )
            .expect_err("external input references must fail the whole list");
            assert!(external_reference_error.contains("only local fragment references"));
            assert!(external_reference_error.contains(reference_keyword));
            assert!(!external_reference_error.contains("user:secret"));
            assert!(!external_reference_error.contains("example.test"));
        }

        let external_output_reference_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "external-output-reference",
                json!({"type": "object"}),
                json!({"$ref": "relative/schema.json"}),
            )],
        )
        .expect_err("external output references must fail the whole list");
        assert!(external_output_reference_error.contains("output schema"));
        assert!(external_output_reference_error.contains("only local fragment references"));

        let unresolved_local_reference_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "unresolved-local-reference",
                json!({"type": "object", "properties": {"missing": {"$ref": "#/$defs/missing"}}}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("unresolved local references must fail schema compilation");
        assert!(unresolved_local_reference_error.contains("could not be compiled"));
        assert!(unresolved_local_reference_error.contains("Draft 2020-12"));

        let legacy_items_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "legacy-items-array",
                json!({"type": "object", "items": [{"type": "string"}]}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("legacy draft syntax must not weaken the fixed Draft 2020-12 contract");
        assert!(legacy_items_error.contains("input schema is invalid"));

        let invalid_input_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "invalid-input",
                json!({"type": "not-a-json-schema-type"}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("invalid input meta-schema must fail");
        assert!(invalid_input_error.contains("input schema is invalid"));

        let invalid_output_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "invalid-output",
                json!({"type": "object"}),
                json!({"type": "not-a-json-schema-type"}),
            )],
        )
        .expect_err("invalid output meta-schema must fail");
        assert!(invalid_output_error.contains("output schema is invalid"));
    }

    /// Verifies plugin partitions are waited on concurrently so offline servers cannot stack deadlines.
    /// 验证插件分区并发等待，使离线服务器无法叠加等待期限。
    ///
    /// Three offline plugin partitions each own one launch deadline; a serial implementation would
    /// spend three deadlines, while the bounded-concurrent implementation spends only one.
    /// 三个离线插件分区各自持有一个启动期限；串行实现会消耗三个期限，
    /// 而有界并发实现只消耗一个期限。
    #[tokio::test(start_paused = true)]
    async fn plugin_partition_readiness_waits_are_concurrent_without_stacked_deadlines() {
        // offline_servers uses three distinct plugin partitions so every source key is independent.
        // offline_servers 使用三个不同插件分区，使每个来源键相互独立。
        let offline_servers = (0..3)
            .map(|index| McpServerConfig {
                source: crate::mcp::config::McpServerSource::Plugin {
                    plugin_id: format!("concurrent-{index}"),
                    generation: 1,
                },
                id: format!("concurrent-server-{index}"),
                name: format!("Concurrent {index}"),
                enabled: true,
                transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                    command: String::from("must-not-launch"),
                    args: Vec::new(),
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                    inherit_environment: false,
                },
            })
            .collect::<Vec<_>>();
        let started = tokio::time::Instant::now();
        let error = bootstrap_mcp_runtime_access(offline_servers)
            .await
            .expect_err("offline plugin partitions must fail readiness");
        assert!(error.contains("readiness timed out"));
        assert!(
            started.elapsed() < MCP_PARTITION_READINESS_TIMEOUT * 2,
            "three concurrent plugin partitions must spend one deadline, not three"
        );
    }

    /// Verifies isolated plugin validation accepts an empty exact partition and rejects foreign sources before launch.
    /// 验证隔离插件校验接受空的精确分区，并在启动前拒绝外部来源。
    #[tokio::test]
    async fn isolated_plugin_validation_enforces_exact_source_identity() {
        validate_plugin_mcp_runtime_partition("fixture", 7, Vec::new())
            .await
            .expect("empty exact plugin partition should validate and stop");

        // foreign_server must be rejected before its intentionally unusable process command is observed.
        // foreign_server 必须在其刻意不可用的进程命令被使用前遭到拒绝。
        let foreign_server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("foreign"),
            name: String::from("Foreign"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("must-not-launch"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let error = validate_plugin_mcp_runtime_partition("fixture", 7, vec![foreign_server])
            .await
            .expect_err("foreign source must fail before launch");
        assert!(error.contains("another source"));
    }
}

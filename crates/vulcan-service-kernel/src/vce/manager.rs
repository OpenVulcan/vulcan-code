//! Project lifecycle and functional entry manager for native VCE integration.
//! 原生 VCE 集成的项目生命周期与功能入口管理器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock, RwLockReadGuard};
use uuid::Uuid;
use vulcan_ai_contract::HostAiPort;
use vulcan_context_engine::{
    RefreshReason, RefreshRequest, RefreshResult, RetrieveRequest, RetrieveResult,
    VceCancelOutcome, VceEngine, VceEngineSnapshot, VceError, VceIndexBinding,
    VceManagementQueryRequest, VceManagementQueryResponse, VceManagementRequest,
    VceManagementSnapshot, VceRefreshControl, VceRetrievalLimits, VceStateIdentity,
    VceWorkspaceStatistics,
};
use vulcan_workbench_model::{ProjectVceSettings, TaskProjectSummary};
use vulcan_workspace_store::vce_data_plane::{
    VceDataPlaneFactory, VceProjectIndex, VceProjectIndexConfig,
};

use crate::ServiceEventEmitter;

use super::{
    VceHostAiAdapter, VceHostAiRegistrationConfig, VceProjectSourceAdapter, VceSourceLimits,
};

#[path = "jobs.rs"]
mod jobs;
pub(crate) use jobs::{VceJobAdmission, VceRebuildProjectInput};

#[path = "runtime_configuration.rs"]
mod runtime_configuration;

#[path = "publication_index.rs"]
mod publication_index;
pub(crate) use publication_index::{
    VcePublicationAuthority, VcePublicationContract, VcePublicationError,
};

/// System inference profile configured through the shared public AI settings.
/// 通过共享公共 AI 设置配置的系统推理 Profile。
pub(crate) const VCE_CONSUMER_PROFILE_ID: &str = "vce";

/// Stable preprocessing contract used before every VCE embedding call.
/// 每次 VCE 向量调用前使用的稳定预处理契约。
pub(super) const VCE_PREPROCESSING_VERSION: &str = "vce-preprocess-v1";

/// Stable recursive semantic chunker contract persisted with VCE vectors.
/// 与 VCE 向量共同持久化的稳定递归语义切块契约。
pub(super) const VCE_CHUNKER_VERSION: &str = "vce-recursive-v1";

/// Stable cancellation diagnostic shared by RPC and watcher retry classification.
/// RPC 与监听重试分类共同使用的稳定取消诊断。
pub(crate) const VCE_REFRESH_CANCELLED_ERROR: &str = "VCE refresh was cancelled by the host";

/// Project lifecycle states owned by the VulcanCode host.
/// 由 VulcanCode 宿主持有的项目生命周期状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VceProjectLifecycle {
    /// No committed checkpoint has been initialized for this project.
    /// 此项目尚未初始化已提交检查点。
    Uninitialized,
    /// The project has a usable committed checkpoint.
    /// 项目已有可用的已提交检查点。
    Ready,
    /// A host-owned refresh is running against a private working engine.
    /// 宿主持有的刷新正在私有工作引擎上执行。
    Refreshing,
    /// Required HostAI or project configuration is unavailable.
    /// 必需的 HostAI 或项目配置不可用。
    BlockedConfiguration,
    /// Persisted state is incompatible and requires one complete rebuild.
    /// 持久化状态不兼容且需要一次完整重建。
    RebuildRequired,
    /// The last host or VCE operation failed.
    /// 最近一次宿主或 VCE 操作失败。
    Failed,
    /// The runtime is cancelling work and releasing its HostAI registration.
    /// 运行时正在取消工作并释放其 HostAI 注册。
    Stopping,
}

/// Management-safe project status returned by the VCE runtime manager.
/// VCE 运行管理器返回的管理安全项目状态。
#[derive(Clone, Debug)]
pub(crate) struct VceRuntimeProjectStatus {
    /// Stable workbench project identity.
    /// 稳定的工作台项目身份。
    pub(crate) project_id: String,
    /// Host lifecycle state.
    /// 宿主生命周期状态。
    pub(crate) lifecycle: VceProjectLifecycle,
    /// Project configuration revision represented by the runtime.
    /// 运行时表示的项目配置修订号。
    pub(crate) config_revision: u64,
    /// Physical embedding contract represented by the active project index.
    /// 活动项目索引所表示的物理向量契约。
    pub(crate) embedding_physical_fingerprint: Option<String>,
    /// Current functional engine snapshot when activation succeeded.
    /// 激活成功时的当前功能引擎快照。
    pub(crate) engine: Option<VceEngineSnapshot>,
    /// Current workspace counters when activation succeeded.
    /// 激活成功时的当前工作区计数。
    pub(crate) workspace: Option<VceWorkspaceStatistics>,
    /// Latest host-boundary error, if any.
    /// 最近一条宿主边界错误（如果有）。
    pub(crate) last_error: Option<String>,
}

/// Counts retained VCE job controls without awaiting async runtime locks.
/// 在不等待异步运行时锁的情况下统计保留的 VCE 作业控制数。
///
/// # Parameters
/// 参数
/// `manager` is the optional VCE runtime owned by shared service state.
/// `manager` 是共享服务状态持有的可选 VCE 运行时。
///
/// # Returns
/// 返回值
/// Best-effort job-control gauge; an absent manager reports zero.
/// 尽力而为的作业控制度量；管理器缺失时报告零。
pub(crate) fn vce_retained_job_gauge(manager: Option<&VceRuntimeManager>) -> u64 {
    manager
        .map(|manager| manager.retained_job_control_gauge())
        .unwrap_or(0)
}

/// Immutable runtime envelope returned by one project-scoped semantic retrieval.
/// 单次项目作用域语义检索返回的不可变运行时信封。
#[derive(Clone, Debug)]
pub(crate) struct VceRuntimeRetrieveResult {
    /// Settings revision represented by the selected runtime.
    /// 所选运行时表示的设置修订号。
    pub(crate) settings_revision: u64,
    /// Exact committed checkpoint searched by the selected engine snapshot.
    /// 所选引擎快照实际检索的已提交检查点。
    pub(crate) checkpoint_version: u64,
    /// Functional VCE retrieval result.
    /// VCE 功能检索结果。
    pub(crate) result: RetrieveResult,
}

/// Model-independent stored target returned with one read-only maintenance plan.
/// 与一份只读维护计划一同返回的模型无关存储目标。
#[derive(Clone, Debug)]
pub(crate) struct VceStoredMaintenanceTarget {
    /// Deterministic plan produced against the exact stored index.
    /// 针对精确已存索引生成的确定性计划。
    pub(crate) plan: vulcan_context_engine::VceMaintenancePlan,
    /// Stored physical embedding identity, independent from current provider readiness.
    /// 已存物理向量身份，独立于当前供应商就绪状态。
    pub(crate) embedding_fingerprint: String,
    /// Stored explicit vector dimension.
    /// 已存明确向量维度。
    pub(crate) embedding_dimension: u32,
    /// Exact published checkpoint observed while the plan was generated.
    /// 生成计划时观察到的精确已发布检查点。
    pub(crate) checkpoint_version: Option<u64>,
}

/// Shared process-wide manager for all enabled project VCE runtimes.
/// 所有已启用项目 VCE 运行时使用的进程级共享管理器。
#[derive(Clone)]
pub(crate) struct VceRuntimeManager {
    /// Shared HostAI port used by all project-specific registrations.
    /// 所有项目专属注册共同使用的共享 HostAI 端口。
    ai_port: Arc<dyn HostAiPort>,
    /// Live host authorization checked at the same transaction boundary as model configuration saves.
    /// 在与模型配置保存相同事务边界检查的实时宿主授权。
    publication_authority: VcePublicationAuthority,
    /// Host-owned lazy factory for physically isolated project storage.
    /// 宿主持有的项目物理隔离存储惰性工厂。
    data_plane: VceDataPlaneFactory,
    /// Service event boundary used to publish lifecycle transitions as they occur.
    /// 用于在生命周期转换发生时发布事件的服务事件边界。
    event_emitter: ServiceEventEmitter,
    /// Successfully activated project runtimes keyed by workbench project ID.
    /// 按工作台项目 ID 索引的已成功激活项目运行时。
    projects: Arc<RwLock<BTreeMap<String, Arc<VceProjectRuntime>>>>,
    /// Project-owned job control survives model runtime replacement without holding a query engine.
    /// 项目自有作业控制跨模型运行时替换保留，不持有查询引擎。
    project_job_states: Arc<Mutex<BTreeMap<String, Arc<jobs::VceProjectJobState>>>>,
    /// Short-lived synchronous projection of committed retrieval admission, never an engine lock.
    /// 已提交检索准入的短时同步投影，绝不持有引擎锁。
    query_admission: Arc<std::sync::RwLock<BTreeMap<String, VceQueryAdmission>>>,
    /// Failed activation records retained for management and retry.
    /// 为管理与重试保留的激活失败记录。
    activation_failures: Arc<RwLock<BTreeMap<String, VceActivationFailure>>>,
    /// Manager-wide lifecycle gate that lets shutdown exclude project activation atomically.
    /// 使关闭流程能够原子排斥项目激活的管理器级生命周期闸门。
    lifecycle_gate: Arc<RwLock<()>>,
    /// Stable per-project gates serializing runtime replacement and removal.
    /// 串行化运行时替换与移除的稳定项目级闸门。
    project_lifecycle_gates: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// Sticky shutdown state preventing project activation after teardown begins.
    /// 防止拆卸开始后再次激活项目的粘性关闭状态。
    shutting_down: Arc<AtomicBool>,
    /// Whether runtime teardown has removed project handles and requires reconciliation on resume.
    /// 运行时拆卸是否已移除项目句柄并要求恢复时重新协调。
    teardown_started: Arc<AtomicBool>,
    /// Exact formerly-active projects retained until the application window actually closes.
    /// 在应用窗口实际关闭前保留的精确原活动项目。
    shutdown_recovery_projects: Arc<Mutex<BTreeMap<String, VceShutdownRecoveryProject>>>,
    /// Exact process owner of newly admitted durable jobs.
    /// 新准入持久作业的精确进程所有者。
    job_owner_run_id: String,
    /// Conservative process-wide writer budget, independent from chat concurrency.
    /// 保守的进程级写入预算，独立于聊天并发。
    job_writer_budget: Arc<tokio::sync::Semaphore>,
    /// Weak host watcher registration avoids an ownership cycle with watcher tasks.
    /// 弱宿主监听注册避免与监听任务形成所有权环。
    watcher: Arc<std::sync::Mutex<Option<super::watcher::VceWeakWatcherService>>>,
}

/// Immutable activation inputs needed only when a drained application window fails to close.
/// 仅在已排空应用窗口关闭失败时需要的不可变激活输入。
#[derive(Clone)]
struct VceShutdownRecoveryProject {
    /// Stable workbench project identity.
    /// 稳定工作台项目身份。
    project_id: String,
    /// Canonical source root owned by the stopped runtime.
    /// 已停止运行时持有的规范源码根目录。
    project_root: PathBuf,
    /// Latest complete admitted project policy.
    /// 最近一次完整准入的项目策略。
    settings: ProjectVceSettings,
    /// Exact physical embedding dimension.
    /// 精确物理向量维度。
    embedding_dimension: u32,
    /// Exact physical embedding fingerprint.
    /// 精确物理向量指纹。
    embedding_physical_fingerprint: String,
}

/// Immutable admission facts derived only from a restored or successfully published checkpoint.
/// 仅从恢复或成功发布的检查点派生的不可变准入事实。
#[derive(Clone, Debug)]
struct VceQueryAdmission {
    /// Host settings revision admitted for this runtime.
    /// 为此运行时准入的宿主设置修订。
    settings_revision: u64,
    /// Exact physical vector contract admitted for the committed checkpoint.
    /// 为已提交检查点准入的精确物理向量契约。
    physical_fingerprint: String,
    /// Published checkpoint identity; zero remains a valid explicit identity if supplied by core.
    /// 已发布检查点身份；内核明确提供零时，零仍是有效身份。
    checkpoint_version: u64,
    /// Frozen current origins rechecked against live host approvals without database reads on tool filtering.
    /// 冻结当前来源，用于根据实时宿主批准复查；工具过滤不读取数据库。
    publication_contract: VcePublicationContract,
}

impl VceQueryAdmission {
    /// Return the committed checkpoint only for the exact admitted host contract.
    /// 仅对精确准入的宿主契约返回已提交检查点。
    ///
    /// `revision` and `fingerprint` are current authoritative facts. Returns no checkpoint
    /// for a mismatch; this pure check does not inspect job state or acquire engine locks.
    /// `revision` 与 `fingerprint` 是当前权威事实。不匹配时不返回检查点；
    /// 此纯检查不读取作业状态，也不获取引擎锁。
    fn checkpoint_for(&self, revision: u64, fingerprint: &str) -> Option<u64> {
        (self.settings_revision == revision && self.physical_fingerprint == fingerprint)
            .then_some(self.checkpoint_version)
    }

    /// Publish this checkpoint while holding the same admission lock used by policy changes.
    /// 持有策略变更使用的同一准入锁时发布此检查点。
    /// `admissions` proves exclusive access; `settings_revision` is the runtime's live policy revision.
    /// `admissions` 证明独占访问；`settings_revision` 为运行时的实时策略修订。
    /// Returns nothing; only the exact contract-owned project's admission is replaced.
    /// 无返回值；仅替换精确契约所属项目的准入。
    fn publish(
        mut self,
        admissions: &mut std::sync::RwLockWriteGuard<'_, BTreeMap<String, Self>>,
        settings_revision: &AtomicU64,
    ) {
        // Sample only after acquiring the policy writer's lock; a pre-lock snapshot can roll admission back.
        // 仅在取得策略写入方使用的锁后采样；加锁前快照可能回滚准入。
        self.settings_revision = settings_revision.load(Ordering::Acquire);
        admissions.insert(self.publication_contract.project_id.clone(), self);
    }
}

/// One activated project runtime with atomic committed-engine replacement.
/// 一个通过原子替换已提交引擎工作的已激活项目运行时。
struct VceProjectRuntime {
    /// Stable workbench project identity.
    /// 稳定的工作台项目身份。
    project_id: String,
    /// Immutable engine configuration used to fork refresh workers.
    /// 用于派生刷新工作引擎的不可变引擎配置。
    config: vulcan_context_engine::VceConfig,
    /// Latest admitted policy revision; independent from immutable physical-index provenance.
    /// 最近准入的策略修订；独立于不可变物理索引来源。
    config_revision: AtomicU64,
    /// Validated project-owned behavior and resource budgets.
    /// 经过校验的项目自有行为与资源预算。
    settings: ProjectVceSettings,
    /// Latest complete saved policy; readers freeze only their query fields without holding this lock across awaits.
    /// 最新完整已保存策略；读取方仅固定查询字段，不跨异步等待持有此锁。
    latest_settings: std::sync::RwLock<ProjectVceSettings>,
    /// Project-scoped HostAI registration.
    /// 项目作用域 HostAI 注册。
    ai: Arc<VceHostAiAdapter>,
    /// Project-scoped code index.
    /// 项目作用域代码索引。
    index: Arc<VceProjectIndex>,
    /// Filesystem-only immutable source boundary.
    /// 纯文件系统不可变源码边界。
    source: Arc<VceProjectSourceAdapter>,
    /// Current committed engine handle; refreshes replace it only after persistence succeeds.
    /// 当前已提交引擎句柄；刷新仅在持久化成功后替换它。
    current_engine: RwLock<Arc<VcePublishedEngine>>,
    /// Committed metadata copied at publication so status reads never wait for a provider call.
    /// 发布时复制的已提交元数据，使状态读取绝不等待供应商调用。
    committed_projection: RwLock<VceCommittedProjection>,
    /// Single-project refresh serialization gate.
    /// 单项目刷新串行化闸门。
    refresh_gate: Arc<Mutex<()>>,
    /// Share project-owned admission and cancellation independently from this model runtime.
    /// 独立于本模型运行时，共享项目自有准入与取消控制。
    job_state: Arc<jobs::VceProjectJobState>,
    /// Live project update policy, separate from immutable physical index settings.
    /// 实时项目更新策略，独立于不可变物理索引设置。
    job_schedule: std::sync::Mutex<jobs::VceJobSchedule>,
    /// Host lifecycle and latest boundary diagnostic.
    /// 宿主生命周期与最近边界诊断。
    host_status: RwLock<VceHostStatus>,
    /// Currently running refresh cancellation handle, if any.
    /// 当前正在运行的刷新取消句柄（如果有）。
    active_refresh: Mutex<Option<VceActiveRefresh>>,
    /// Monotonic control transitions sampled under the active-refresh lock, including idle-to-idle cycles.
    /// 在活动刷新锁内采样的单调控制变更，包含从空闲回到空闲的完整周期。
    refresh_observation_revision: AtomicU64,
    /// Admission and drain boundary for every runtime operation.
    /// 每项运行时操作的接纳与排空边界。
    operation_admission: VceOperationAdmission,
}

/// Immutable engine and its exact storage lease, retained together by every admitted reader.
/// 不可变引擎及其精确存储租约，由每个已准入读取方共同保留。
struct VcePublishedEngine {
    /// Core state belonging to this publication, never mutated by retrieval.
    /// 属于本次发布的核心状态，检索绝不修改它。
    engine: VceEngine,
    /// Matching physical read view; absent only when no checkpoint has been committed.
    /// 匹配的物理读取视图；仅在没有已提交检查点时缺失。
    read_index: Option<VceProjectIndex>,
}

impl VcePublishedEngine {
    /// Identify this exact physical publication without retaining a lease across browser requests.
    /// 标识此精确物理发布，不跨浏览器请求保留租约。
    /// No parameters; returns absence only for an uninitialized engine.
    /// 无参数；仅未初始化引擎返回空值。
    fn management_read_identity(&self) -> Option<String> {
        self.read_index.as_ref()?.management_read_identity()
    }

    /// Pin storage for `engine` using the host's exact `owner_run_id` and writable `index`.
    /// 使用宿主精确的 `owner_run_id` 和可写 `index` 为 `engine` 固定存储。
    ///
    /// Returns an immutable publication or an explicit checkpoint admission failure.
    /// 返回不可变发布对象，或明确的检查点准入失败。
    async fn pin(
        engine: VceEngine,
        index: &VceProjectIndex,
        owner_run_id: &str,
    ) -> Result<Self, String> {
        // A missing checkpoint is a legitimate uninitialized state, never an inferred empty index.
        // 缺少检查点是合法的未初始化状态，绝不推断成已建好的空索引。
        let read_index = match engine.workspace_snapshot().checkpoint {
            Some(checkpoint) => Some(index.pin_checkpoint(checkpoint, owner_run_id).await?),
            None => None,
        };
        Ok(Self { engine, read_index })
    }
}

/// Lightweight metadata derived from one committed engine, independent from running queries.
/// 从一个已提交引擎派生的轻量元数据，与运行中的查询独立。
#[derive(Clone)]
struct VceCommittedProjection {
    /// Core checkpoint and last-operation summary belonging to this publication.
    /// 属于本次发布的内核检查点与最近操作摘要。
    engine: VceEngineSnapshot,
    /// Exact workspace counts belonging to the same publication.
    /// 属于同一次发布的精确工作区计数。
    workspace: VceWorkspaceStatistics,
}

/// One host-cancellable refresh identity retained only while its future is running.
/// 一项仅在刷新 Future 运行期间保留的宿主可取消刷新身份。
#[derive(Clone)]
struct VceActiveRefresh {
    /// Stable logical refresh identity.
    /// 稳定逻辑刷新身份。
    request_id: String,
    /// Embedding concurrency frozen at batch execution admission, before any source scan.
    /// 在批次执行准入时、任何源码扫描之前冻结的向量并发数。
    embedding_max_concurrency: usize,
    /// Shared scan/core cancellation; admitted commits cannot be dropped by an outer selector.
    /// 共享扫描与核心取消；已准入提交不能被外层选择器丢弃。
    cancellation: VceRefreshControl,
}

/// Race-safe admission boundary that drains active operations before runtime teardown.
/// 在运行时拆卸前排空活动操作的竞态安全接纳边界。
#[derive(Default)]
struct VceOperationAdmission {
    /// Sticky state rejecting every operation after stop begins.
    /// 在停止开始后拒绝全部操作的粘性状态。
    accepting: AtomicBool,
    /// Read leases held by operations and acquired exclusively by teardown.
    /// 由操作持有读租约、由拆卸流程独占获取的闸门。
    gate: Arc<RwLock<()>>,
}

impl VceOperationAdmission {
    /// Creates an admission boundary that initially accepts operations.
    /// 创建一开始接纳操作的边界。
    ///
    /// # Returns
    /// 返回值
    /// A fresh accepting boundary.
    /// 返回全新的接纳边界。
    fn accepting() -> Self {
        Self {
            accepting: AtomicBool::new(true),
            gate: Arc::new(RwLock::new(())),
        }
    }

    /// Stops new admissions before teardown waits for existing read leases.
    /// 在拆卸等待现有读租约前停止新的操作接纳。
    fn stop(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    /// Admits one operation while closing the check-versus-lock race with teardown.
    /// 接纳一项操作，并消除检查与加锁之间的拆卸竞态。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` identifies the runtime in rejection diagnostics.
    /// `project_id` 在拒绝诊断中标识运行时。
    ///
    /// # Returns
    /// 返回值
    /// A read lease held until completion, or an explicit stopping error.
    /// 返回持有到完成的读租约，或明确的停止错误。
    async fn begin(&self, project_id: &str) -> Result<RwLockReadGuard<'_, ()>, String> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(format!("VCE project `{project_id}` is stopping"));
        }
        let lease = self.gate.read().await;
        if !self.accepting.load(Ordering::Acquire) {
            return Err(format!("VCE project `{project_id}` is stopping"));
        }
        Ok(lease)
    }
}

/// Mutable host lifecycle data independent from the VCE functional engine.
/// 独立于 VCE 功能引擎的可变宿主生命周期数据。
struct VceHostStatus {
    /// Current host lifecycle state.
    /// 当前宿主生命周期状态。
    lifecycle: VceProjectLifecycle,
    /// Latest host-boundary error.
    /// 最近一条宿主边界错误。
    last_error: Option<String>,
}

/// Failed activation retained even when no functional runtime could be constructed.
/// 即使无法构造功能运行时也会保留的激活失败记录。
#[derive(Clone)]
struct VceActivationFailure {
    /// Project configuration revision that failed.
    /// 失败的项目配置修订号。
    config_revision: u64,
    /// Explicit classification retained even without an executable project runtime.
    /// 即使没有可执行项目运行时也保留的明确分类。
    lifecycle: VceProjectLifecycle,
    /// Exact bounded activation diagnostic.
    /// 精确且有界的激活诊断。
    error: String,
}

/// Typed activation error separating host AI configuration, rebuild requirements and runtime failures.
/// 将宿主 AI 配置、重建要求及运行时失败分离的类型化激活错误。
#[derive(Debug)]
pub(crate) struct VceActivationError {
    /// Exact blocked, rebuild-required or failed state; never inferred from error wording.
    /// 精确的配置阻塞、需要重建或失败状态；绝不从错误措辞推断。
    lifecycle: VceProjectLifecycle,
    /// Exact bounded-ready diagnostic retained by the manager.
    /// 由管理器保留且可限制长度的精确诊断。
    error: String,
}

impl VceActivationError {
    /// Builds one deterministic configuration-blocked activation error.
    /// 构建一项确定性的配置阻塞激活错误。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the exact configuration diagnostic.
    /// `error` 是精确配置诊断。
    ///
    /// # Returns
    /// 返回值
    /// A typed blocked-configuration error.
    /// 返回类型化配置阻塞错误。
    fn configuration(error: impl ToString) -> Self {
        Self {
            lifecycle: VceProjectLifecycle::BlockedConfiguration,
            error: error.to_string(),
        }
    }

    /// Builds one runtime activation failure that does not imply AI reconfiguration.
    /// 构建一项不表示需要重新配置 AI 的运行时激活失败。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the exact runtime diagnostic.
    /// `error` 是精确运行时诊断。
    ///
    /// # Returns
    /// 返回值
    /// A typed non-configuration activation error.
    /// 返回类型化非配置激活错误。
    fn runtime(error: impl ToString) -> Self {
        Self {
            lifecycle: VceProjectLifecycle::Failed,
            error: error.to_string(),
        }
    }

    // refreshing represents a transient replacement owner that status reads must expose without restoring the predecessor.
    // refreshing 表示暂时持有替换权的作业；状态读取应暴露该状态，而不是恢复前驱运行时。
    fn refreshing(error: impl ToString) -> Self {
        Self {
            lifecycle: VceProjectLifecycle::Refreshing,
            error: error.to_string(),
        }
    }
}

impl From<String> for VceActivationError {
    /// Preserve ordinary activation failures independently from typed compatibility decisions.
    /// 保留普通激活失败，使其独立于类型化兼容决定。
    /// Input is the exact operational error; returns a failed lifecycle.
    /// 输入为精确操作错误；返回失败生命周期。
    fn from(error: String) -> Self {
        Self::runtime(error)
    }
}

impl From<VcePublicationError> for VceActivationError {
    /// Carry the authority's exact classification into the first project-status response.
    /// 将权威校验的精确分类传递到首次项目状态响应。
    /// Input is the typed denial; returns rebuild-required only for explicit compatibility rejection.
    /// 输入为类型化拒绝；仅明确兼容拒绝返回需要重建。
    fn from(error: VcePublicationError) -> Self {
        match error {
            VcePublicationError::Rejected(message) => Self::runtime(message),
            VcePublicationError::RebuildRequired(message) => Self {
                lifecycle: VceProjectLifecycle::RebuildRequired,
                error: message,
            },
        }
    }
}

impl From<VceActivationError> for String {
    /// Retain the exact activation diagnostic at existing non-status service boundaries.
    /// 在既有非状态服务边界保留精确激活诊断。
    /// Input is a typed activation failure; returns its unchanged message.
    /// 输入为类型化激活失败；返回未改变的消息。
    fn from(error: VceActivationError) -> Self {
        error.error
    }
}

impl VceRuntimeManager {
    /// Inspect persisted vector provenance independently from global readiness or active AI handles.
    /// 独立于全局就绪或活动 AI 句柄，检查持久化向量来源。
    ///
    /// `project_id` is exact host ownership. Returns read-only metadata; no source scan or AI call.
    /// `project_id` 是精确宿主归属。返回只读元数据；不扫描源码、不调用 AI。
    pub(crate) async fn embedding_provenance(
        &self,
        project_id: &str,
    ) -> Result<Option<vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance>, String>
    {
        self.data_plane.embedding_provenance(project_id).await
    }

    /// Create one manager over the already bootstrapped host AI and data-plane services.
    /// 在已完成引导的宿主 AI 与数据面服务上创建一个管理器。
    ///
    /// # Parameters
    /// 参数
    /// `ai_port` is the process-wide unified HostAI boundary.
    /// `ai_port` 是进程级统一 HostAI 边界。
    /// `data_plane` is the host-owned VCE storage handle.
    /// `data_plane` 是宿主持有的 VCE 存储句柄。
    /// `event_emitter` publishes project lifecycle transitions to connected surfaces.
    /// `event_emitter` 向已连接界面发布项目生命周期转换。
    ///
    /// # Returns
    /// 返回值
    /// An empty manager ready for lazy project activation.
    /// 返回可延迟激活项目的空管理器。
    pub(crate) fn new(
        ai_port: Arc<dyn HostAiPort>,
        data_plane: VceDataPlaneFactory,
        event_emitter: ServiceEventEmitter,
        publication_authority: VcePublicationAuthority,
    ) -> Self {
        Self {
            ai_port,
            publication_authority,
            data_plane,
            event_emitter,
            projects: Arc::new(RwLock::new(BTreeMap::new())),
            project_job_states: Arc::new(Mutex::new(BTreeMap::new())),
            query_admission: Arc::new(std::sync::RwLock::new(BTreeMap::new())),
            activation_failures: Arc::new(RwLock::new(BTreeMap::new())),
            lifecycle_gate: Arc::new(RwLock::new(())),
            project_lifecycle_gates: Arc::new(Mutex::new(BTreeMap::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
            teardown_started: Arc::new(AtomicBool::new(false)),
            shutdown_recovery_projects: Arc::new(Mutex::new(BTreeMap::new())),
            job_owner_run_id: Uuid::new_v4().to_string(),
            job_writer_budget: Arc::new(tokio::sync::Semaphore::new(1)),
            watcher: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Attach the existing host watcher without retaining its background tasks.
    /// 挂接既有宿主监听器，不持有其后台任务。
    /// `watcher` is a weak link created by host assembly. Returns nothing and schedules no work.
    /// `watcher` 为宿主装配建立的弱链接。无返回值，不安排工作。
    pub(super) fn bind_watcher(&self, watcher: super::watcher::VceWeakWatcherService) {
        *self
            .watcher
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(watcher);
    }

    /// Resolve the existing watcher for read-only source evidence; never construct or restart observation.
    /// 获取既有监听器以读取源码证据；绝不构造或重启观察。
    /// No parameters; returns None when the host link is absent, released or poisoned.
    /// 无参数；宿主链接缺失、释放或损坏时返回 None。
    pub(super) fn source_watcher(&self) -> Option<super::VceWatcherService> {
        self.watcher.lock().ok()?.as_ref()?.upgrade()
    }

    /// Read source freshness for the currently activated canonical root without activating a project.
    /// 读取当前已激活规范根的源码新鲜度，不激活项目。
    /// project_id is authoritative; missing runtime or watcher returns unknown evidence.
    /// project_id 为权威身份；运行时或监听器缺失时返回未知证据。
    pub(crate) async fn source_freshness(
        &self,
        project_id: &str,
    ) -> super::watcher::VceSourceFreshness {
        let runtime = self.projects.read().await.get(project_id).cloned();
        match (runtime, self.source_watcher()) {
            (Some(runtime), Some(watcher)) => {
                watcher.source_freshness(project_id, runtime.source.project_root())
            }
            _ => super::watcher::VceSourceFreshness::default(),
        }
    }

    /// Attach automatic observation after lazy restoration or the first committed publication.
    /// 在惰性恢复或首次提交发布后挂接自动观察。
    /// `runtime` owns the current root and checkpoint. Records observer failures without revoking queries.
    /// `runtime` 持有当前根与检查点。记录观察器失败，不撤销查询。
    async fn reconcile_runtime_watcher(&self, runtime: &VceProjectRuntime) {
        if self.shutting_down.load(Ordering::Acquire)
            || !runtime
                .operation_admission
                .accepting
                .load(Ordering::Acquire)
            || runtime.index.is_read_only()
            || runtime
                .committed_projection
                .read()
                .await
                .engine
                .checkpoint_version
                .is_none()
        {
            return;
        }
        // Keep the policy lock through synchronous registration so an older automatic snapshot cannot win.
        // 将策略锁持有到同步注册完成，防止较旧自动策略快照覆盖新值。
        let result = (|| {
            let schedule = runtime
                .job_schedule
                .lock()
                .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?;
            if !schedule.automatic() {
                return Ok(());
            }
            let watcher = self
                .watcher
                .lock()
                .map_err(|_| String::from("VCE watcher registration lock is poisoned"))?
                .as_ref()
                .and_then(super::watcher::VceWeakWatcherService::upgrade)
                .ok_or_else(|| String::from("VCE automatic file observation is unavailable"))?;
            watcher.watch_project(&runtime.project_id, runtime.source.project_root())
        })();
        if let Err(error) = result {
            runtime.host_status.write().await.last_error = Some(bounded_error(error));
        }
    }

    /// Closes admission synchronously before the host cancels active agent tasks.
    /// 在宿主取消活动智能体任务前同步关闭接纳。
    pub(crate) fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
    }

    /// Reopens admission after a failed application shutdown attempt.
    /// 在应用关闭尝试失败后重新开放接纳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when teardown had started and persisted projects must be reconciled again.
    /// 当拆卸已经开始且必须重新协调持久化项目时返回 `true`。
    pub(crate) fn resume_after_failed_shutdown(&self) -> bool {
        self.shutting_down.store(false, Ordering::Release);
        self.teardown_started.load(Ordering::Acquire)
    }

    /// Restores exactly the project runtimes removed by a completed shutdown teardown.
    /// 精确恢复已完成关闭拆卸所移除的项目运行时。
    ///
    /// Takes no parameters. Returns only after every retained runtime is active again; failed
    /// restoration keeps the complete descriptor set for another explicit recovery attempt.
    /// 无参数。仅在全部保留运行时重新激活后返回；恢复失败时保留完整描述集合，供再次明确恢复。
    pub(crate) async fn restore_after_failed_shutdown(&self) -> Result<(), String> {
        if !self.teardown_started.load(Ordering::Acquire) {
            return Ok(());
        }
        let recoveries = self
            .shutdown_recovery_projects
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for recovery in recoveries {
            self.enable_project(
                &recovery.project_id,
                &recovery.project_root,
                recovery.settings,
                recovery.embedding_dimension,
                &recovery.embedding_physical_fingerprint,
                false,
            )
            .await
            .map_err(String::from)?;
        }
        self.shutdown_recovery_projects.lock().await.clear();
        self.teardown_started.store(false, Ordering::Release);
        Ok(())
    }

    /// Activate one project and restore compatible metadata without scanning or refreshing.
    /// 激活一个项目并恢复兼容元数据，不扫描或刷新。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the authoritative workbench identity.
    /// `project_id` 是权威工作台身份。
    /// `project_root` is the authoritative absolute project path.
    /// `project_root` 是权威绝对项目路径。
    /// `settings` is the complete validated persisted project settings revision.
    /// `settings` 是完整且经过校验的持久化项目设置修订版。
    /// `embedding_dimension` is the saved consumer-profile value already checked by readiness.
    /// `embedding_dimension` 是已由就绪检查验证的消费者配置档保存值。
    ///
    /// # Returns
    /// 返回值
    /// Success after activation only; this method never scans or embeds project source.
    /// 仅激活完成后返回成功；本方法绝不扫描或向量化项目源码。
    pub(crate) async fn enable_project(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: ProjectVceSettings,
        embedding_dimension: u32,
        // expected_fingerprint freezes the readiness result against concurrent route changes.
        // expected_fingerprint 冻结就绪结果，防止并发路由变更。
        expected_fingerprint: &str,
        // Creation authority is supplied only by an explicit human initialization job.
        // 创建授权仅由明确的人为初始化作业提供。
        create_missing_index: bool,
    ) -> Result<(), VceActivationError> {
        if !settings.enabled {
            return Err(VceActivationError::configuration(
                "VCE project settings are disabled",
            ));
        }
        settings.validate()?;
        let _manager_lifecycle = self.lifecycle_gate.read().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(VceActivationError::runtime(
                "VCE runtime manager is shutting down",
            ));
        }
        let project_lifecycle = self.project_lifecycle_gate(project_id).await;
        let _project_lifecycle = project_lifecycle.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(VceActivationError::runtime(
                "VCE runtime manager is shutting down",
            ));
        }
        // existing may have been restored by another cold-state read while this caller waited.
        // existing 可能在此调用方等待期间被另一个冷态读取恢复。
        let existing = self.projects.read().await.get(project_id).cloned();
        // A delayed page read must not tear down the newer runtime installed by settings reconciliation or handoff.
        // 延迟页面读取不得拆卸由设置协调或交接安装的较新运行时。
        if existing.as_ref().is_some_and(|runtime| {
            settings.revision < runtime.config_revision.load(Ordering::Acquire)
        }) {
            return Err(VceActivationError::runtime(
                "VCE runtime activation settings snapshot is stale",
            ));
        }
        // Replacement workers restore the published target themselves; cold reads cannot resurrect its predecessor.
        // 替换工作任务自行恢复已发布目标；冷态读取不能复活其前驱。
        if self.project_has_replacement_job(project_id).await {
            return Err(VceActivationError::refreshing(
                "VCE explicit replacement is in progress; observe its durable job",
            ));
        }
        if let Some(existing) = existing
            && runtime_policy_matches(&existing.settings, &settings)
            && existing.ai.embedding_space().dimension == embedding_dimension as usize
            && existing
                .ai
                .embedding_space()
                .physical_fingerprint
                .as_deref()
                == Some(expected_fingerprint)
            && std::fs::canonicalize(project_root)
                .map_err(|error| format!("Resolve VCE project root: {error}"))?
                == existing.source.project_root()
        {
            self.adopt_runtime_policy_revision(&existing, &settings)?;
            self.reconcile_runtime_watcher(&existing).await;
            return Ok(());
        }
        // Fence and drain the project's durable job owner before replacing its runtime or leases.
        // 在替换运行时或租约前封锁并排空项目的持久作业所有者。
        let jobs = self
            .project_job_states
            .lock()
            .await
            .get(project_id)
            .cloned();
        if let Some(jobs) = &jobs {
            jobs.request_stop().await;
        }
        // Release the map lock before draining a replaced runtime's in-flight operations.
        // 在排空被替换运行时的执行中操作前释放映射锁。
        let previous = self.projects.write().await.remove(project_id);
        if let Some(jobs) = jobs {
            jobs.cancel_and_drain()
                .await
                .map_err(VceActivationError::runtime)?;
        }
        if let Some(previous) = previous {
            self.stop_runtime(&previous).await?;
        }
        let config_revision = settings.revision;
        match self
            .activate_project(
                project_id,
                project_root,
                settings,
                embedding_dimension,
                expected_fingerprint,
                create_missing_index,
            )
            .await
        {
            Ok(runtime) => {
                self.activation_failures.write().await.remove(project_id);
                self.projects
                    .write()
                    .await
                    .insert(project_id.to_owned(), Arc::clone(&runtime));
                // Restoring a committed engine, not lifecycle labels, establishes retrieval eligibility.
                // 恢复已提交引擎才建立检索资格，不能依据生命周期标签。
                let checkpoint_version = runtime
                    .status()
                    .await
                    .engine
                    .and_then(|engine| engine.checkpoint_version);
                self.publish_query_admission(&runtime, checkpoint_version)
                    .await?;
                self.reconcile_runtime_watcher(&runtime).await;
                Ok(())
            }
            Err(error) => {
                self.projects.write().await.remove(project_id);
                self.activation_failures.write().await.insert(
                    project_id.to_owned(),
                    VceActivationFailure {
                        config_revision,
                        lifecycle: error.lifecycle,
                        error: bounded_error(error.error.clone()),
                    },
                );
                Err(error)
            }
        }
    }

    /// Restore an already-created project without scanning source, creating storage or embedding.
    /// 恢复已创建项目，不扫描源码、不创建存储、不执行向量化。
    ///
    /// `project_id`, `project_root`, `settings` and `embedding_dimension` are authoritative
    /// host inputs. Returns false for absent storage, true for restored or rebuild-required state,
    /// and an explicit error for other activation failures. This never grants query admission by itself.
    /// `project_id`、`project_root`、`settings` 与 `embedding_dimension` 是权威宿主输入。
    /// 无存储时返回假，已恢复或明确需要重建时返回真，其他激活失败返回明确错误；此结果本身不授予查询权限。
    pub(crate) async fn restore_existing_project(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: ProjectVceSettings,
        embedding_dimension: u32,
        // expected_fingerprint is the exact current readiness contract, never a display name.
        // expected_fingerprint 是精确当前就绪契约，绝非展示名。
        expected_fingerprint: &str,
    ) -> Result<bool, String> {
        if self
            .data_plane
            .load_active_project_config(project_id)
            .await?
            .is_none()
        {
            let recovery = self
                .data_plane
                .load_uninitialized_recovery_config(project_id)
                .await?;
            let Some(recovery) = recovery else {
                return Ok(false);
            };
            if self
                .data_plane
                .open_uninitialized_recovery_project_index(&recovery)
                .await?
                .is_none()
            {
                return Err(String::from(
                    "VCE uninitialized recovery binding disappeared before restoration",
                ));
            }
            // Preserve an explicit management state so the UI can offer rebuild after a cold cancellation or failed handoff.
            // 保留明确的管理状态，使界面能在冷取消或交接失败后提供重建入口。
            self.activation_failures.write().await.insert(
                project_id.to_owned(),
                VceActivationFailure {
                    config_revision: settings.revision,
                    lifecycle: VceProjectLifecycle::RebuildRequired,
                    error: String::from("VCE uninitialized recovery requires an explicit rebuild"),
                },
            );
            return Ok(true);
        }
        // Restoration is a status read: an explicitly classified compatibility denial is a readable state.
        // 恢复属于状态读取：明确分类的兼容拒绝属于可读取状态。
        let config_revision = settings.revision;
        let restored = self
            .enable_project(
                project_id,
                project_root,
                settings,
                embedding_dimension,
                expected_fingerprint,
                false,
            )
            .await;
        if let Err(error) = restored {
            // Classify this call's own result, never another activation's concurrently retained status.
            // 分类本次调用自身结果，绝不使用其他激活并发保留的状态。
            match error.lifecycle {
                VceProjectLifecycle::RebuildRequired => {}
                VceProjectLifecycle::Refreshing => {
                    // Preserve a readable transient status while the replacement worker owns the target.
                    // 替换工作任务持有目标期间保留可读取的暂时状态。
                    self.activation_failures.write().await.insert(
                        project_id.to_owned(),
                        VceActivationFailure {
                            config_revision,
                            lifecycle: VceProjectLifecycle::Refreshing,
                            error: error.error,
                        },
                    );
                }
                _ => return Err(error.into()),
            }
        }
        Ok(true)
    }

    /// Adopt scheduling and execution policy changes without cancelling queries or active indexing.
    /// 接纳调度及执行策略变化，不取消查询或活动索引。
    ///
    /// `project_id`, `project_root`, and `settings` are authoritative host facts; `fingerprint`
    /// is the admitted embedding contract. Returns false when actual runtime behavior differs.
    /// `project_id`、`project_root` 与 `settings` 是权威宿主事实；`fingerprint` 是准入向量契约。
    /// 实际运行时行为不同时返回假。
    #[cfg(test)]
    pub(crate) async fn reconcile_policy_revision(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: &ProjectVceSettings,
        fingerprint: &str,
    ) -> Result<bool, String> {
        settings.validate()?;
        let _manager_lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(project_id).await;
        let _project_gate = project_gate.lock().await;
        self.reconcile_policy_revision_locked(project_id, project_root, settings, fingerprint)
            .await
    }

    /// Adopt `settings` for `project_id`/`project_root` and `fingerprint` under the caller's lifecycle locks.
    /// 在调用方生命周期锁下为 `project_id`／`project_root` 与 `fingerprint` 接纳 `settings`。
    /// Returns whether runtime behavior matches; stale revisions and invalid roots remain explicit errors.
    /// 返回运行时行为是否匹配；过期修订与非法根目录保持明确错误。
    async fn reconcile_policy_revision_locked(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: &ProjectVceSettings,
        fingerprint: &str,
    ) -> Result<bool, String> {
        // A replacement has no query runtime yet; reconcile its handoff under the same job admission lock.
        // 替换尚无查询运行时；在相同作业准入锁下协调其交接。
        if let Some(matches) = self
            .reconcile_replacement_policy(project_id, project_root, settings, fingerprint)
            .await?
        {
            return Ok(matches);
        }
        let runtime = self.projects.read().await.get(project_id).cloned();
        let Some(runtime) = runtime else {
            return Ok(false);
        };
        // Check revision before behavior: an older conflicting callback cannot revoke newer query admission.
        // 先检查修订再比较行为：较旧冲突回调不能撤销较新的查询准入。
        if settings.revision < runtime.config_revision.load(Ordering::Acquire) {
            return Err(String::from("VCE policy reconciliation snapshot is stale"));
        }
        if !runtime_policy_matches(&runtime.settings, settings)
            || runtime.ai.embedding_space().physical_fingerprint.as_deref() != Some(fingerprint)
            || std::fs::canonicalize(project_root)
                .map_err(|error| format!("Resolve VCE policy project root: {error}"))?
                != runtime.source.project_root()
        {
            return Ok(false);
        }
        self.adopt_runtime_policy_revision(&runtime, settings)?;
        Ok(true)
    }

    /// Atomically advance only policy admission metadata while preserving checkpoint provenance.
    /// 原子推进策略准入元数据，同时保留检查点来源。
    ///
    /// `runtime` is protected by its lifecycle gate; `settings` is current persisted policy.
    /// Returns a stale-policy error instead of revoking a newer runtime.
    /// `runtime` 由其生命周期闸门保护；`settings` 为当前持久策略。
    /// 返回过期策略错误，而不是撤销较新运行时。
    fn adopt_runtime_policy_revision(
        &self,
        runtime: &VceProjectRuntime,
        settings: &ProjectVceSettings,
    ) -> Result<(), String> {
        let revision = settings.revision;
        let mut admissions = self
            .query_admission
            .write()
            .map_err(|_| String::from("VCE query admission lock is poisoned"))?;
        if revision < runtime.config_revision.load(Ordering::Acquire) {
            return Err(String::from("VCE policy reconciliation snapshot is stale"));
        }
        // Acquire every fallible lock before changing any policy projection.
        // 在修改任何策略投影之前取得全部可能失败的锁。
        let mut latest = runtime
            .latest_settings
            .write()
            .map_err(|_| String::from("VCE project policy lock is poisoned"))?;
        if settings.revision == latest.revision && settings != &*latest {
            return Err(String::from(
                "VCE policy reconciliation revision has conflicting settings",
            ));
        }
        let mut schedule = runtime
            .job_schedule
            .lock()
            .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?;
        *latest = settings.clone();
        schedule.update(settings);
        runtime.config_revision.store(revision, Ordering::Release);
        if let Some(admission) = admissions.get_mut(&runtime.project_id) {
            admission.settings_revision = revision;
        }
        Ok(())
    }

    /// Disable one project and unregister its HostAI capability.
    /// 禁用一个项目并注销其 HostAI 能力。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the authoritative workbench project identity.
    /// `project_id` 是权威工作台项目身份。
    ///
    /// # Returns
    /// 返回值
    /// Success after the runtime and failed activation record are removed.
    /// 移除运行时与激活失败记录后返回成功。
    #[cfg(test)]
    pub(crate) async fn disable_project(&self, project_id: &str) -> Result<(), String> {
        validate_project_identity(project_id, 1)?;
        let _manager_lifecycle = self.lifecycle_gate.read().await;
        let project_lifecycle = self.project_lifecycle_gate(project_id).await;
        let _project_lifecycle = project_lifecycle.lock().await;
        self.disable_project_locked(project_id).await
    }

    /// Disable `project_id` under the caller's manager/project lifecycle locks and return drain failures.
    /// 在调用方管理器／项目生命周期锁下关闭 `project_id` 并返回排空失败。
    async fn disable_project_locked(&self, project_id: &str) -> Result<(), String> {
        // Fence handoff before removing the map entry, including when no runtime exists yet.
        // 在移除映射条目之前封锁交接，包括运行时尚不存在的情况。
        let jobs = self
            .project_job_states
            .lock()
            .await
            .get(project_id)
            .cloned();
        if let Some(jobs) = &jobs {
            jobs.request_stop().await;
        }
        // Revoke the query/runtime admission before awaiting a worker at the publication boundary.
        // 在发布边界等待工作任务之前撤销查询／运行时准入。
        let runtime = self.projects.write().await.remove(project_id);
        self.activation_failures.write().await.remove(project_id);
        // Project-owned writers must stop even when no compatible query runtime was activated.
        // 即使没有激活兼容查询运行时，也必须停止项目自有写入任务。
        if let Some(jobs) = jobs {
            jobs.cancel_and_drain().await?;
        }
        // Drain workers before runtime teardown so a terminal receipt cannot hold the operation
        // gate while shutdown is waiting for the same worker's SQLite write to settle.
        // 在运行时拆卸前排空工作任务，避免终态回执持有操作闸门时关闭流程反过来等待同一任务的 SQLite 写入。
        if let Some(runtime) = runtime {
            self.stop_runtime(&runtime).await?;
        }
        Ok(())
    }

    /// Compare `other` with this manager's shared lifecycle identity; clones return true, replacements false.
    /// 比较 `other` 与此管理器的共享生命周期身份；克隆返回真，替换实例返回假。
    pub(super) fn shares_lifecycle_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.lifecycle_gate, &other.lifecycle_gate)
    }

    /// Verify validation for `project`/`readiness` waits for both actual lifecycle gates on this fixture manager.
    /// 在此夹具管理器上验证 `project`／`readiness` 的校验等待两个实际生命周期闸门。
    /// Returns only after exact one-poll assertions and one rejected validation; no timers or runtime mutations.
    /// 仅在精确单次轮询断言与一次拒绝校验后返回；不使用计时器或修改运行时。
    #[cfg(test)]
    pub(super) fn verify_reconciliation_validation_waits_for_lifecycle<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
        readiness: &'a super::VceConfigurationReadiness,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use futures_util::FutureExt;
            // These are the production gate instances, not a simulated ordering primitive.
            // 这些是生产闸门实例，不是模拟顺序原语。
            let project_gate = self.project_lifecycle_gate(&project.id).await;
            // Hold the project gate across the manager-gate release to exercise both suspension points.
            // 跨管理器闸门释放持有项目闸门，以执行两个挂起点。
            let project_guard = project_gate.lock().await;
            // Block the first production await without starting a separate timed task.
            // 不启动独立计时任务，阻止第一个生产等待点。
            let manager_guard = self.lifecycle_gate.write().await;
            // Count validation itself, so an entry-only check cannot pass this regression.
            // 计数校验自身，使仅入口检查无法通过此回归。
            let validation_calls = AtomicU64::new(0);
            // The rejection prevents any fixture runtime, watcher, or stored checkpoint mutation.
            // 此拒绝防止修改夹具运行时、监听器或存储检查点。
            let mut reconciliation =
                self.reconcile_project_configuration(project, readiness, None, || {
                    validation_calls.fetch_add(1, Ordering::SeqCst);
                    Err(String::from(
                        "fixture reconciliation authority retired while waiting",
                    ))
                });
            assert!(reconciliation.as_mut().now_or_never().is_none());
            assert_eq!(validation_calls.load(Ordering::SeqCst), 0);
            drop(manager_guard);
            assert!(reconciliation.as_mut().now_or_never().is_none());
            assert_eq!(validation_calls.load(Ordering::SeqCst), 0);
            drop(project_guard);
            assert_eq!(
                reconciliation.await.expect_err("retired authority"),
                "fixture reconciliation authority retired while waiting"
            );
            assert_eq!(validation_calls.load(Ordering::SeqCst), 1);
        })
    }

    /// Reconcile `project`, `readiness`, and its optional `watcher` in one project lifecycle critical section.
    /// 在同一个项目生命周期临界区协调 `project`、`readiness` 及其可选 `watcher`。
    /// `validate_current` rechecks live authority after lock acquisition; stale callbacks return before mutation.
    /// `validate_current` 在获得锁后复核实时权威；过期回调在任何变更前返回。
    /// Returns explicit validation, policy, watcher, or drain errors; it never initializes a cold index.
    /// 返回明确的校验、策略、监听或排空错误；绝不初始化冷态索引。
    pub(super) fn reconcile_project_configuration<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
        readiness: &'a super::VceConfigurationReadiness,
        watcher: Option<&'a super::VceWatcherService>,
        validate_current: impl FnOnce() -> Result<(), String> + Send + 'a,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            validate_project_identity(&project.id, project.vce.revision)?;
            project.vce.validate()?;
            // Serialize with activation and replacement without retaining the global AI transaction while draining.
            // 与激活和替换串行，排空期间不持有全局 AI 事务。
            let _manager_lifecycle = self.lifecycle_gate.read().await;
            // Reuse the exact project gate held by activation and disable, not a separate configuration mutex.
            // 复用激活和关闭持有的精确项目闸门，不引入独立配置互斥锁。
            let project_gate = self.project_lifecycle_gate(&project.id).await;
            // Keep the guard through watcher changes so a later callback cannot finish before an older decision.
            // 将守卫保持到监听变更结束，使后续回调不能先于较旧决策完成。
            let _project_gate = project_gate.lock().await;
            if self.shutting_down.load(Ordering::Acquire) {
                return Err(String::from("VCE runtime manager is shutting down"));
            }
            validate_current()?;
            if !project.vce.enabled || !readiness.status.ready {
                if let Some(watcher) = watcher {
                    watcher.unwatch_project(&project.id).await;
                }
                return self.disable_project_locked(&project.id).await;
            }
            // A ready snapshot must carry the exact physical contract used by the runtime.
            // 就绪快照必须携带运行时使用的精确物理契约。
            let expected_fingerprint = readiness
                .embedding_physical_fingerprint
                .as_deref()
                .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?;
            // The persisted root and policy are the sole configuration inputs.
            // 持久化根目录及策略是唯一配置输入。
            let project_root = Path::new(&project.path);
            // Policy-only revisions retain the active runtime and its committed query generation.
            // 仅策略修订保留活动运行时及其已提交查询代际。
            let policy_matches = self
                .reconcile_policy_revision_locked(
                    &project.id,
                    project_root,
                    &project.vce,
                    expected_fingerprint,
                )
                .await?;
            // Read status under the same lifecycle fence used by the policy decision.
            // 在策略决策使用的同一生命周期围栏内读取状态。
            let current_status = self.project_status(&project.id).await;
            let Some(current_status) = current_status else {
                // Cold projects remain metadata-only, including unfinished replacement handoffs.
                // 冷态项目保持仅元数据状态，包括未完成的替换交接。
                if let Some(watcher) = watcher {
                    watcher.unwatch_project(&project.id).await;
                }
                return Ok(());
            };
            if !policy_matches
                || current_status.config_revision != project.vce.revision
                || current_status.embedding_physical_fingerprint.as_deref()
                    != Some(expected_fingerprint)
            {
                if let Some(watcher) = watcher {
                    watcher.unwatch_project(&project.id).await;
                }
                return self.disable_project_locked(&project.id).await;
            }
            // Model readiness alone cannot grant observation before the first committed checkpoint.
            // 仅模型就绪不能在首个检查点提交前授予观察能力。
            let initialized = current_status
                .engine
                .as_ref()
                .is_some_and(|engine| engine.checkpoint_version.is_some());
            if project.vce.update_mode == vulcan_workbench_model::ProjectVceUpdateMode::Automatic
                && initialized
            {
                // An automatic initialized project requires the host-owned shared watcher.
                // 自动且已初始化的项目必须使用宿主持有的共享监听器。
                let watcher = watcher.ok_or_else(|| String::from("VCE 文件监听服务不可用。"))?;
                watcher.watch_project(&project.id, project_root)?;
            } else if let Some(watcher) = watcher {
                watcher.unwatch_project(&project.id).await;
            }
            Ok(())
        })
    }

    /// Retry cleanup of unpublished generations for one project without touching its checkpoint.
    /// 为单个项目重试未发布代次清理，且不触碰其检查点。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    ///
    /// # Returns
    /// 返回值
    /// Success after the storage-owned cleanup queue converges.
    /// 返回存储持有的清理队列收敛后的成功结果。
    pub(crate) async fn cleanup_staging(&self, project_id: &str) -> Result<(), String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        // Cleanup is a writer, but must not wait behind another host or claim its unfinished work.
        // 清理属于写入，但不得等待另一个宿主或接管其未完成工作。
        let _writer = self
            .data_plane
            .try_acquire_project_writer(project_id)?
            .ok_or_else(|| String::from("VCE project already has an active host writer"))?;
        runtime.index.retry_pending_cleanup().await?;
        if runtime.index.has_unresolved_staging().await? {
            return Err(String::from(
                "VCE unpublished staging remains owned or unconfirmed; writer recovery is required before it can be removed",
            ));
        }
        Ok(())
    }

    /// Retrieve semantic code context from one project's last committed checkpoint.
    /// 从一个项目最近已提交检查点检索语义代码上下文。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is taken from the authoritative execution context.
    /// `project_id` 来自权威执行上下文。
    /// `information_request` is the model-provided natural-language need.
    /// `information_request` 是模型提供的自然语言需求。
    /// # Returns
    /// 返回值
    /// A bounded VCE retrieval result from the committed index.
    /// 返回来自已提交索引的有界 VCE 检索结果。
    pub(crate) async fn retrieve(
        &self,
        project_id: &str,
        information_request: &str,
    ) -> Result<VceRuntimeRetrieveResult, String> {
        if information_request.trim().is_empty() {
            return Err(String::from("VCE information_request must not be empty"));
        }
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        // Freeze revision, switches and limits together; concurrent policy changes affect only later queries.
        // 一起固定修订、开关及限制；并发策略变化仅影响后续查询。
        let (settings_revision, allow_query_rewrite, allow_rerank, limits) = {
            // settings is a synchronous snapshot and is released before any provider or index await.
            // settings 为同步快照，在任何供应商或索引等待之前释放。
            let settings = runtime
                .latest_settings
                .read()
                .map_err(|_| String::from("VCE project policy lock is poisoned"))?;
            (
                settings.revision,
                settings.allow_query_rewrite,
                settings.allow_rerank,
                VceRetrievalLimits {
                    query_max_concurrency: settings.query_max_concurrency,
                    max_results: settings.max_results,
                    max_context_chars: settings.max_context_chars,
                },
            )
        };
        let engine_handle = runtime.current_engine.read().await.clone();
        // The shared publication retains old metadata and vectors while updates publish independently.
        // 共享发布对象在更新独立发布期间保留旧元数据和向量。
        let engine = &engine_handle.engine;
        let read_index = engine_handle.read_index.as_ref().ok_or_else(|| {
            String::from("VCE project has no committed checkpoint available for retrieval")
        })?;
        let checkpoint_version = engine.status().checkpoint_version.ok_or_else(|| {
            String::from("VCE project has no committed checkpoint available for retrieval")
        })?;
        let mut request = RetrieveRequest::current_checkpoint(
            request_id(project_id, "retrieve"),
            information_request,
        );
        request.allow_query_rewrite = allow_query_rewrite;
        request.allow_rerank = allow_rerank;
        let result = engine
            .retrieve_read_view_with_limits(request, read_index, runtime.ai.as_ref(), limits)
            .await
            .map_err(|error| error.to_string())?;
        {
            // A query on an older checkpoint must not overwrite a newer publication's metadata.
            // 旧检查点上的查询不得覆盖更新发布的元数据。
            let mut committed = runtime.committed_projection.write().await;
            if committed.engine.checkpoint_version == Some(checkpoint_version) {
                committed.engine.last_retrieval = Some(result.summary.clone());
            }
        }
        Ok(VceRuntimeRetrieveResult {
            settings_revision,
            checkpoint_version,
            result,
        })
    }

    /// Build one storage-backed management snapshot for a project.
    /// 为一个项目构建一份存储支持的管理快照。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    ///
    /// # Returns
    /// 返回值
    /// Optional publication identity paired with combined engine, AI, workspace, and index status.
    /// 返回可选发布身份及配套的引擎、AI、工作区与索引组合状态。
    pub(crate) async fn management_snapshot(
        &self,
        project_id: &str,
    ) -> Result<(Option<String>, VceManagementSnapshot), String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let engine_handle = runtime.current_engine.read().await.clone();
        let engine = &engine_handle.engine;
        // Freeze the host-observed writer identity without holding its lock during storage reads.
        // 固定宿主已观察写入身份，不在存储读取期间持有其锁。
        let (refresh_revision, active_refresh) = runtime.active_refresh_observation().await;
        let index = engine_handle
            .read_index
            .as_ref()
            .unwrap_or(runtime.index.as_ref())
            .management_read_for_refresh(active_refresh.as_deref());
        // A cold publication has no lease; reject a first-commit race instead of mixing its counts.
        // 冷发布尚无租约；拒绝首次提交竞争，不能混入新检查点数量。
        let snapshot = engine
            .management_snapshot(
                VceManagementRequest {
                    request_id: request_id(project_id, "management"),
                },
                &index,
                runtime.ai.as_ref(),
            )
            .await
            .map_err(|error| error.to_string())?;
        if runtime.active_refresh_observation().await.0 != refresh_revision {
            return Err(String::from("VCE 索引任务已变化，请重新读取管理状态。"));
        }
        if snapshot
            .index
            .as_ref()
            .and_then(|index| index.checkpoint_version)
            != snapshot.engine.checkpoint_version
        {
            return Err(String::from("VCE 管理读取期间检查点已变化，请重新读取。"));
        }
        Ok((engine_handle.management_read_identity(), snapshot))
    }

    /// Execute one typed read-only VCE management query.
    /// 执行一项类型化只读 VCE 管理查询。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    /// `request` contains the already typed and bounded query.
    /// `request` 包含已类型化且有界的查询。
    /// `expected_read_identity` must match the overview for content reads; live history and plans omit it.
    /// 内容读取的 `expected_read_identity` 必须匹配概览；实时历史与计划不携带此值。
    ///
    /// # Returns
    /// 返回值
    /// The validated VCE management response.
    /// 返回经过校验的 VCE 管理响应。
    pub(crate) async fn management_query(
        &self,
        project_id: &str,
        request: VceManagementQueryRequest,
        expected_read_identity: Option<&str>,
    ) -> Result<VceManagementQueryResponse, String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let engine_handle = runtime.current_engine.read().await.clone();
        let engine = &engine_handle.engine;
        // Only integrity queries classify staging; file pages, details and live jobs need no writer fence.
        // 仅完整性查询分类暂存；文件分页、详情与实时作业无需写入身份保护。
        let observes_staging = matches!(
            &request.query,
            vulcan_context_engine::VceManagementQuery::IntegrityIssues(_)
        );
        let (refresh_revision, active_refresh) = if observes_staging {
            runtime.active_refresh_observation().await
        } else {
            (0, None)
        };
        // Content pages must match the displayed publication; live jobs and maintenance are separate.
        // 内容分页必须匹配所显示发布；实时作业和维护属于独立语义。
        if matches!(
            &request.query,
            vulcan_context_engine::VceManagementQuery::Files(_)
                | vulcan_context_engine::VceManagementQuery::BlobDetail { .. }
                | vulcan_context_engine::VceManagementQuery::IntegrityIssues(_)
        ) && (expected_read_identity.is_none()
            || expected_read_identity != engine_handle.management_read_identity().as_deref())
        {
            return Err(String::from(
                "语义索引已变化或缺少读取身份，请刷新索引详情后重试。",
            ));
        }
        // Maintenance targets current writable storage; inventory and detail target the publication lease.
        // 维护以当前可写存储为目标；清单与详情以发布租约为目标。
        let index = match &request.query {
            vulcan_context_engine::VceManagementQuery::MaintenancePlan { .. } => {
                runtime.index.as_ref()
            }
            _ => engine_handle
                .read_index
                .as_ref()
                .unwrap_or(runtime.index.as_ref()),
        }
        .management_read_for_refresh(active_refresh.as_deref());
        let response = engine
            .management_query(request, &index)
            .await
            .map_err(|error| error.to_string())?;
        if observes_staging && runtime.active_refresh_observation().await.0 != refresh_revision {
            return Err(String::from("VCE 索引任务已变化，请重新读取完整性状态。"));
        }
        Ok(response)
    }

    /// Read a maintenance plan from the exact persisted index independently of model/runtime activation.
    /// 独立于模型／运行时激活，从精确持久索引读取维护计划。
    /// `project_id` selects existing storage; `mode` selects the read-only plan. Returns a core-validated plan.
    /// `project_id` 选择已有存储；`mode` 选择只读计划。返回经核心校验的计划。
    pub(crate) async fn stored_maintenance_target(
        &self,
        project_id: &str,
        mode: vulcan_context_engine::VceMaintenanceMode,
    ) -> Result<VceStoredMaintenanceTarget, String> {
        // lifecycle excludes shutdown and project removal, without acquiring any inference or refresh lock.
        // lifecycle 排斥关闭与项目移除，不获取推理或刷新锁。
        let _lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(project_id).await;
        let _project_gate = project_gate.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from(
                "VCE maintenance planning is unavailable during shutdown",
            ));
        }
        // Prefer the active binding, then read the deterministic orphaned recovery binding without activating or scanning it.
        // 优先使用活动绑定；否则读取确定性的孤立恢复绑定，不激活也不扫描。
        let (original, index, active) = if let Some(config) = self
            .data_plane
            .load_active_project_config(project_id)
            .await?
        {
            let index = self
                .data_plane
                .open_existing_project_index(&config)
                .await?
                .ok_or_else(|| String::from("VCE maintenance index disappeared during planning"))?;
            (config, index, true)
        } else {
            if mode == vulcan_context_engine::VceMaintenanceMode::Compact {
                return Err(String::from(
                    "VCE physical compaction planning requires an active published index",
                ));
            }
            let config = self
                .data_plane
                .load_uninitialized_recovery_config(project_id)
                .await?
                .ok_or_else(|| {
                    String::from("VCE maintenance planning requires an active or recovery index")
                })?;
            let index = self
                .data_plane
                .open_uninitialized_recovery_project_index(&config)
                .await?
                .ok_or_else(|| String::from("VCE recovery index disappeared during planning"))?;
            (config, index, false)
        };
        let provenance = if active {
            index.embedding_provenance().await?
        } else {
            index.uninitialized_recovery_provenance().await?
        };
        if provenance.original != original {
            return Err(String::from(
                "VCE maintenance target changed while its stored identity was read",
            ));
        }
        // config creates only the core's validation context; no source scan or provider registration occurs.
        // config 仅创建核心校验上下文；不扫描源码，也不注册供应商。
        let mut config = vulcan_context_engine::VceConfig::new(
            original.binding.checkpoint_chain_id.clone(),
            original.binding.embedding_space.clone(),
        )
        .map_err(|error| error.to_string())?;
        config.index_namespace_id = original.binding.namespace_id.clone();
        config.index_schema_version = original.binding.schema_version;
        // engine validates request/response identity and the exact bound namespace without restoring a query runtime.
        // engine 校验请求／响应身份与精确绑定命名空间，不恢复查询运行时。
        let engine = VceEngine::new(config).map_err(|error| error.to_string())?;
        let response = engine
            .management_query(
                VceManagementQueryRequest {
                    operation_id: request_id(project_id, "stored-maintenance-plan"),
                    query: vulcan_context_engine::VceManagementQuery::MaintenancePlan { mode },
                },
                &index,
            )
            .await
            .map_err(|error| error.to_string())?;
        let plan = match response.result {
            vulcan_context_engine::VceManagementQueryResult::MaintenancePlan(plan) => plan,
            _ => Err(String::from(
                "VCE stored maintenance query returned an unexpected result",
            ))?,
        };
        Ok(VceStoredMaintenanceTarget {
            plan,
            embedding_fingerprint: original.physical_fingerprint,
            embedding_dimension: original.dimension,
            checkpoint_version: provenance.checkpoint_version,
        })
    }

    /// Build one read-only maintenance plan from stored identity without requiring a live model.
    /// 使用已存身份构建一份只读维护计划，不要求模型实时可用。
    ///
    /// `project_id` selects the exact project and `mode` selects the maintenance semantics.
    /// Returns only the deterministic core plan for existing callers.
    /// `project_id` 选择精确项目，`mode` 选择维护语义。仅为现有调用方返回确定性内核计划。
    pub(crate) async fn stored_maintenance_plan(
        &self,
        project_id: &str,
        mode: vulcan_context_engine::VceMaintenanceMode,
    ) -> Result<vulcan_context_engine::VceMaintenancePlan, String> {
        self.stored_maintenance_target(project_id, mode)
            .await
            .map(|target| target.plan)
    }

    /// Return one management-safe status even when activation is blocked.
    /// 即使激活受阻也返回一份管理安全状态。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact workbench project.
    /// `project_id` 选择精确工作台项目。
    ///
    /// # Returns
    /// 返回值
    /// An activated or failed-activation status; disabled projects return `None`.
    /// 返回已激活或激活失败状态；禁用项目返回 `None`。
    pub(crate) async fn project_status(&self, project_id: &str) -> Option<VceRuntimeProjectStatus> {
        if let Some(runtime) = self.projects.read().await.get(project_id).cloned() {
            return Some(runtime.status().await);
        }
        self.activation_failures
            .read()
            .await
            .get(project_id)
            .map(|failure| VceRuntimeProjectStatus {
                project_id: project_id.to_owned(),
                lifecycle: failure.lifecycle,
                config_revision: failure.config_revision,
                embedding_physical_fingerprint: None,
                engine: None,
                workspace: None,
                last_error: Some(failure.error.clone()),
            })
    }

    /// Reports whether one project can expose retrieval without awaiting runtime locks.
    /// 判断一个项目是否可在不等待运行时锁的前提下暴露检索能力。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact workbench project.
    /// `project_id` 选择精确工作台项目。
    /// `config_revision` is the currently persisted project settings revision.
    /// `config_revision` 是当前持久化的项目设置修订号。
    /// `embedding_physical_fingerprint` is the current global embedding identity.
    /// `embedding_physical_fingerprint` 是当前全局向量身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` for a matching committed projection; busy query or refresh locks cannot hide tools.
    /// 已提交投影匹配时返回 `true`；查询锁或刷新锁忙碌不能隐藏工具。
    pub(crate) fn project_can_serve_now(
        &self,
        project_id: &str,
        config_revision: u64,
        embedding_physical_fingerprint: &str,
    ) -> bool {
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        // This lock protects only tiny copied metadata and is never held across an await or I/O.
        // 此锁只保护少量复制元数据，绝不跨等待或输入输出持有。
        let admission = match self.query_admission.read() {
            Ok(admissions) => admissions.get(project_id).cloned(),
            Err(_) => return false,
        };
        admission.is_some_and(|admission| {
            admission
                .checkpoint_for(config_revision, embedding_physical_fingerprint)
                .is_some()
                && (self.publication_authority)(&admission.publication_contract).is_ok()
        })
    }

    /// Unregister all active project capabilities during ordered application shutdown.
    /// 在应用有序关闭期间注销全部活动项目能力。
    ///
    /// # Returns
    /// 返回值
    /// Success when every registered project was removed.
    /// 所有已注册项目均移除后返回成功。
    pub(crate) async fn shutdown(&self) -> Result<(), String> {
        self.begin_shutdown();
        let _manager_lifecycle = self.lifecycle_gate.write().await;
        // Capture complete restart inputs before removing or stopping any runtime. The desktop shell
        // may keep this process alive when the final native window close fails.
        // 在移除或停止任何运行时之前捕获完整重启输入。最终原生窗口关闭失败时，桌面壳可能继续保留本进程。
        let runtime_snapshot = self
            .projects
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut recovery_snapshot = Vec::with_capacity(runtime_snapshot.len());
        for runtime in &runtime_snapshot {
            let settings = runtime
                .latest_settings
                .read()
                .map_err(|_| String::from("VCE project policy lock is poisoned"))?
                .clone();
            let embedding_space = runtime.ai.embedding_space();
            let embedding_dimension = u32::try_from(embedding_space.dimension)
                .map_err(|_| String::from("VCE embedding dimension exceeds the host limit"))?;
            let embedding_physical_fingerprint = embedding_space
                .physical_fingerprint
                .clone()
                .ok_or_else(|| String::from("VCE runtime is missing its physical fingerprint"))?;
            recovery_snapshot.push(VceShutdownRecoveryProject {
                project_id: runtime.project_id.clone(),
                project_root: runtime.source.project_root().to_path_buf(),
                settings,
                embedding_dimension,
                embedding_physical_fingerprint,
            });
        }
        let runtimes = std::mem::take(&mut *self.projects.write().await)
            .into_values()
            .collect::<Vec<_>>();
        {
            let mut recoveries = self.shutdown_recovery_projects.lock().await;
            for recovery in recovery_snapshot {
                recoveries.insert(recovery.project_id.clone(), recovery);
            }
        }
        drop(runtime_snapshot);
        self.teardown_started.store(true, Ordering::Release);
        self.activation_failures.write().await.clear();
        let mut failures = Vec::new();
        // Snapshot project controls without holding the registry lock across worker completion.
        // 获取项目控制快照，不跨工作任务完成过程持有注册表锁。
        let job_states = self
            .project_job_states
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        // Broadcast stop before draining any one project; otherwise a later worker can keep a
        // runtime operation read lease while the first project is still persisting its receipt.
        // 先广播停止再逐个排空项目，避免第一个项目持久化回执时后续工作任务仍持有运行时读租约。
        for jobs in &job_states {
            jobs.request_stop().await;
        }
        for jobs in job_states {
            if let Err(error) = jobs.cancel_and_drain().await {
                failures.push(error);
            }
        }
        // Worker leases are gone before runtime teardown tries to acquire its operation write gate.
        // 工作任务租约已释放后再拆卸运行时，避免运行时写闸门与终态回执互相等待。
        for runtime in &runtimes {
            if let Err(error) = self.stop_runtime(runtime).await {
                failures.push(error);
            }
        }
        // Drop the shutdown snapshot before closing cached bindings; each runtime index retains one project lease.
        // 在关闭缓存绑定前释放退出快照；每个 runtime index 都会保留一个项目租约。
        drop(runtimes);
        // Keep the lightweight catalog alive for failed app-shutdown recovery; close content leases.
        // 为应用关闭失败恢复保留轻量目录，只关闭内容租约。
        if let Err(error) = self.data_plane.close_project_bindings().await {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    /// Construct one project runtime from exact host identities and compatible persisted state.
    /// 根据精确宿主身份与兼容持久化状态构造一个项目运行时。
    /// Opens one project runtime with the caller's validated settings and explicit dimensions.
    /// 使用调用方校验过的设置与明确维度打开一个项目运行时。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` and `project_root` identify the project; `settings` owns its index budgets.
    /// `project_id` 与 `project_root` 标识项目；`settings` 持有其索引预算。
    /// `embedding_dimension` is the exact dimension frozen by host configuration admission.
    /// `embedding_dimension` 为宿主配置准入冻结的精确维度。
    ///
    /// # Returns
    /// 返回值
    /// A project runtime or a classified configuration/runtime error.
    /// 返回项目运行时，或分类后的配置／运行时错误。
    async fn activate_project(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: ProjectVceSettings,
        embedding_dimension: u32,
        expected_fingerprint: &str,
        // Only an admitted initialization job may create a missing index binding.
        // 仅已准入的初始化作业可以创建缺失索引绑定。
        create_missing_index: bool,
    ) -> Result<Arc<VceProjectRuntime>, VceActivationError> {
        validate_project_identity(project_id, settings.revision)
            .map_err(VceActivationError::runtime)?;
        settings
            .validate()
            .map_err(VceActivationError::configuration)?;
        // prepared freezes source/index facts without allocating storage or registering models.
        // prepared 冻结源码／索引事实，不分配存储或注册模型。
        let prepared = runtime_configuration::prepare_runtime_configuration(
            project_id,
            project_root,
            &settings,
            embedding_dimension,
            expected_fingerprint,
            None,
        )
        .map_err(VceActivationError::runtime)?;
        // provenance supplies the persisted logical ID, including a previously published replacement.
        // provenance 提供持久逻辑编号，包括先前已发布的替换库。
        let provenance = self
            .data_plane
            .embedding_provenance(project_id)
            .await
            .map_err(VceActivationError::runtime)?;
        // execution_space retains current producer facts; compatibility still requires explicit admission.
        // execution_space 保留当前生产者事实；兼容性仍需明确准入。
        let execution_space = prepared
            .execution_space(provenance.as_ref().map(|origin| &origin.original))
            .map_err(VceActivationError::runtime)?;
        let ai = Arc::new(
            VceHostAiAdapter::activate(
                Arc::clone(&self.ai_port),
                VceHostAiRegistrationConfig {
                    runtime_instance_id: Uuid::new_v4(),
                    project_id: project_id.to_owned(),
                    consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                    vector_space_id: execution_space.vector_space_id.clone(),
                    dimension: Some(embedding_dimension),
                    preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                    chunker_version: String::from(VCE_CHUNKER_VERSION),
                },
            )
            .map_err(VceActivationError::configuration)?,
        );
        let activation = async {
            if ai.embedding_space() != &execution_space {
                return Err(VceActivationError::runtime(
                    "VCE embedding route changed after readiness admission; reload configuration",
                ));
            }
            // config/source/index_config consume the already validated preparation, without changing stored origins.
            // config/source/index_config 消费已验证准备结果，不改写存储来源。
            let mut config = prepared.engine;
            let source = prepared.source;
            let index_config = prepared.index;
            let admission = VcePublicationContract {
                replacement_admission: None,
                project_id: project_id.to_owned(),
                canonical_root: index_config.root_path.clone(),
                embedding_fingerprint: expected_fingerprint.to_owned(),
                dimension: embedding_dimension,
                max_file_bytes: settings.max_file_bytes,
                extra_ignore_patterns: settings.extra_ignore_patterns.clone(),
                provenance: provenance.clone(),
            };
            {
                let _transaction = crate::begin_public_ai_configuration_transaction().await;
                (self.publication_authority)(&admission)?;
            }
            // Explicit host approval selects the unchanged original binding, not a rewritten model label.
            // 明确宿主批准选择不变的原始绑定，不改写模型标签。
            let index_config = match provenance {
                Some(provenance) => {
                    let original = provenance.original;
                    let grant = vulcan_context_engine::VceEmbeddingExecutionContract::new(
                        original.binding.clone(),
                        ai.embedding_space().clone(),
                        format!("vce-host-admission:{}", Uuid::new_v4()),
                    )
                    .map_err(|error| error.to_string())?;
                    config.index_namespace_id = original.binding.namespace_id.clone();
                    config.checkpoint_chain_id = original.binding.checkpoint_chain_id.clone();
                    config.index_schema_version = original.binding.schema_version;
                    config.embedding_space = original.binding.embedding_space.clone();
                    config.embedding_execution = Some(grant);
                    config.validate().map_err(|error| error.to_string())?;
                    original
                }
                None => index_config,
            };
            // Restore uses a read-only path; model mismatch is not implicit rebuild authority.
            // 恢复使用只读路径；模型不匹配不代表获准隐式重建。
            let index = match self
                .data_plane
                .open_existing_project_index(&index_config)
                .await?
            {
                Some(index) => index,
                None if create_missing_index => {
                    self.data_plane.create_project_index(index_config).await?
                }
                None => {
                    return Err(VceActivationError::runtime(
                        "VCE project index has not been initialized",
                    ));
                }
            };
            let index = Arc::new(match &config.embedding_execution {
                Some(grant) => index.with_embedding_execution(grant.clone())?,
                None => index,
            });
            let expected_identity = VceStateIdentity::new(
                config.index_namespace_id.clone(),
                config.checkpoint_chain_id.clone(),
                config.index_schema_version,
                config.embedding_space.clone(),
            );
            let (engine, initial_lifecycle) = match index.load_state_snapshot().await? {
                Some(snapshot) if snapshot.identity == expected_identity => {
                    match VceEngine::from_state_snapshot(config.clone(), snapshot) {
                        Ok(engine) => {
                            // Restoring metadata completes immediately; checkpoint absence stays explicit.
                            // 元数据恢复立即完成；没有检查点时保持明确的未初始化状态。
                            let lifecycle = if engine.status().checkpoint_version.is_some() {
                                VceProjectLifecycle::Ready
                            } else {
                                VceProjectLifecycle::Uninitialized
                            };
                            (engine, lifecycle)
                        }
                        Err(_) => (
                            VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                            VceProjectLifecycle::RebuildRequired,
                        ),
                    }
                }
                Some(_) => (
                    VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                    VceProjectLifecycle::RebuildRequired,
                ),
                None => (
                    VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                    VceProjectLifecycle::Uninitialized,
                ),
            };
            // committed_projection belongs to the immutable engine admitted below.
            // committed_projection 属于下方准入的不可变引擎。
            let committed_projection = VceCommittedProjection {
                engine: engine.status(),
                workspace: engine.workspace_statistics(),
            };
            // Restore only the admission clock; neither unfinished jobs nor dirty paths are resumed.
            // 仅恢复准入时钟；不恢复执行未完成作业或脏路径。
            let mut job_schedule = jobs::VceJobSchedule::new(&settings);
            if let Some(receipt) = index.job_store().latest().await? {
                job_schedule.restore_admission(
                    receipt.created_at_unix_ms,
                    jobs::job_time()?,
                    tokio::time::Instant::now(),
                );
            }
            // Retain the exact durable checkpoint before exposing this runtime to queries.
            // 在向查询暴露本运行时之前保留精确持久检查点。
            let publication =
                VcePublishedEngine::pin(engine, &index, &self.job_owner_run_id).await?;
            Ok::<_, VceActivationError>(Arc::new(VceProjectRuntime {
                project_id: project_id.to_owned(),
                config,
                config_revision: AtomicU64::new(settings.revision),
                job_schedule: std::sync::Mutex::new(job_schedule),
                latest_settings: std::sync::RwLock::new(settings.clone()),
                settings,
                ai: Arc::clone(&ai),
                index,
                source,
                current_engine: RwLock::new(Arc::new(publication)),
                committed_projection: RwLock::new(committed_projection),
                refresh_gate: Arc::new(Mutex::new(())),
                job_state: self.project_job_state(project_id).await,
                host_status: RwLock::new(VceHostStatus {
                    lifecycle: initial_lifecycle,
                    last_error: None,
                }),
                active_refresh: Mutex::new(None),
                refresh_observation_revision: AtomicU64::new(0),
                operation_admission: VceOperationAdmission::accepting(),
            }))
        }
        .await;
        match activation {
            Ok(runtime) => Ok(runtime),
            Err(error) => match ai.deactivate() {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(VceActivationError::runtime(format!(
                    "{}; cleanup failed VCE HostAI activation: {cleanup_error}",
                    error.error
                ))),
            },
        }
    }

    /// Execute one refresh on a private engine and atomically publish it after persistence.
    /// 在私有引擎上执行一次刷新，并在持久化后原子发布。
    async fn refresh_runtime_locked(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        request: RefreshRequest,
        // active was admitted before source scanning and remains owned until publication settles.
        // active 在源码扫描前准入，并持续持有到发布确定。
        active: VceActiveRefresh,
    ) -> Result<RefreshResult, String> {
        let current_handle = runtime.current_engine.read().await.clone();
        let base_snapshot = current_handle.engine.state_snapshot();
        let base_has_checkpoint = base_snapshot.workspace.checkpoint.is_some();
        // working_config keeps immutable source identity and the budget frozen before this batch scanned files.
        // working_config 保留不可变源码身份及本批次扫描文件前冻结的预算。
        let mut working_config = runtime.config.clone();
        working_config.embedding_max_concurrency = active.embedding_max_concurrency;
        let mut working = match VceEngine::from_state_snapshot(working_config, base_snapshot) {
            Ok(working) => working,
            Err(error) => {
                return Err(self
                    .fail_active_refresh(runtime, &active, error.to_string())
                    .await);
            }
        };
        // The publication view shares the public configuration transaction, not a new AI registry.
        // 发布视图共享公共配置事务，不建立新 AI 注册表。
        let publication = publication_index::VcePublicationIndex::new(
            Arc::clone(runtime),
            Arc::clone(&self.publication_authority),
            &self.job_owner_run_id,
        );
        // The core cooperatively aborts staging and awaits a commit already in flight.
        // 核心协作中止暂存，并等待已经进入执行的提交。
        let result = working
            .refresh_observed(
                request,
                &publication,
                runtime.ai.as_ref(),
                runtime.source.as_ref(),
                &active.cancellation,
            )
            .await;
        self.end_active_refresh(runtime, &active).await;
        if matches!(&result, Err(VceError::RefreshCancelled)) {
            runtime.set_cancelled(base_has_checkpoint).await;
            super::services::emit_vce_runtime_changed(
                &self.event_emitter,
                self,
                &runtime.project_id,
            )
            .await;
            return Err(String::from(VCE_REFRESH_CANCELLED_ERROR));
        }
        match result {
            Ok(result) => {
                // A checkpoint-changing refresh persisted its state atomically inside commit.
                // A no-op refresh has no commit transaction, so persist its counters and summary here.
                // 改变检查点的刷新已在提交事务内原子持久化状态；无提交的刷新则在此持久化计数与摘要。
                if result.commit_receipt.is_none() {
                    // Even metadata-only no-op publication must not race a revoked model contract.
                    // 即使仅更新元数据的无操作发布，也不得与已撤销模型契约竞态。
                    let _transaction = crate::begin_public_ai_configuration_transaction().await;
                    if let Err(error) = publication.validate().await {
                        return Err(self.fail_runtime(runtime, error).await);
                    }
                    let snapshot = working.state_snapshot();
                    if let Err(error) = runtime.index.persist_state_snapshot(&snapshot).await {
                        return Err(self.fail_runtime(runtime, error).await);
                    }
                }
                // checkpoint_version is taken from the completed private engine before publication.
                // checkpoint_version 在发布前取自已完成的私有引擎。
                let checkpoint_version = working.status().checkpoint_version;
                // committed_projection and the engine describe the exact same completed generation.
                // committed_projection 与引擎描述完全相同的已完成代次。
                let committed_projection = VceCommittedProjection {
                    engine: working.status(),
                    workspace: working.workspace_statistics(),
                };
                // Commit already retained the new lease atomically; no-op refreshes reuse the old one.
                // 提交已原子保留新租约；无变化刷新复用原租约。
                let read_index = if result.commit_receipt.is_some() {
                    match publication.take_committed_read_index().await {
                        Ok(read_index) => Some(read_index),
                        Err(error) => return Err(self.fail_runtime(runtime, error).await),
                    }
                } else {
                    current_handle.read_index.clone()
                };
                // Old readers retain their own engine and lease after this in-memory publication swap.
                // 此次内存发布替换后，旧读取方仍保留自己的引擎及租约。
                let publication = VcePublishedEngine {
                    engine: working,
                    read_index,
                };
                *runtime.current_engine.write().await = Arc::new(publication);
                *runtime.committed_projection.write().await = committed_projection;
                // A private replacement writer must not expose tools before ordinary-runtime handoff.
                // 私有替换写入器在普通运行时交接之前不得暴露工具。
                let registered = self
                    .projects
                    .read()
                    .await
                    .get(&runtime.project_id)
                    .is_some_and(|current| Arc::ptr_eq(current, runtime));
                if registered {
                    self.publish_query_admission(runtime, checkpoint_version)
                        .await?;
                }
                runtime.set_ready().await;
                super::services::emit_vce_runtime_changed(
                    &self.event_emitter,
                    self,
                    &runtime.project_id,
                )
                .await;
                Ok(result)
            }
            Err(error) => Err(self.fail_runtime(runtime, error.to_string()).await),
        }
    }

    /// Admit the exact host job control before scanning, sharing cancellation and progress.
    /// 扫描前准入精确宿主作业控制句柄，共享取消与进度。
    ///
    /// `runtime` owns the index, `request_id` identifies the job, and `cancellation` is its control.
    /// Returns the admitted refresh or a stopping error without starting source work.
    /// `runtime` 持有索引，`request_id` 标识作业，`cancellation` 为其控制句柄。
    /// 返回已准入刷新或停止错误，不启动源码处理。
    async fn begin_observed_refresh(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        request_id: String,
        cancellation: VceRefreshControl,
    ) -> Result<VceActiveRefresh, String> {
        // embedding_max_concurrency is sampled once, so later policy changes cannot reshape in-flight batches.
        // embedding_max_concurrency 仅采样一次，后续策略变化不能改变执行中批次。
        let embedding_max_concurrency = runtime
            .latest_settings
            .read()
            .map_err(|_| String::from("VCE project policy lock is poisoned"))?
            .embedding_max_concurrency;
        // active remains queryable during scanning and never shares a chat cancellation token.
        // active 在扫描期间保持可查询，且绝不共享聊天取消令牌。
        let active = VceActiveRefresh {
            request_id,
            embedding_max_concurrency,
            cancellation,
        };
        {
            // current serializes admission with teardown's cancellation lookup.
            // current 将准入与拆卸的取消查找串行化。
            let mut current = runtime.active_refresh.lock().await;
            if !runtime
                .operation_admission
                .accepting
                .load(Ordering::Acquire)
            {
                return Err(String::from("VCE project stopped before refresh admission"));
            }
            *current = Some(active.clone());
            runtime
                .refresh_observation_revision
                .fetch_add(1, Ordering::Relaxed);
        }
        runtime.set_refreshing().await;
        super::services::emit_vce_runtime_changed(&self.event_emitter, self, &runtime.project_id)
            .await;
        Ok(active)
    }

    /// Finish only the exact active refresh identity after its worker has settled.
    /// 工作线程确定后，仅结束精确活动刷新身份。
    ///
    /// `runtime` owns the project and `active` identifies the admitted job. Returns no value.
    /// `runtime` 持有项目，`active` 标识已准入作业。无返回值。
    async fn end_active_refresh(&self, runtime: &VceProjectRuntime, active: &VceActiveRefresh) {
        active.cancellation.finish();
        // current cannot clear a newer job because the exact logical identity must match.
        // current 必须匹配精确逻辑身份，因此不能清除较新作业。
        let mut current = runtime.active_refresh.lock().await;
        if current
            .as_ref()
            .is_some_and(|current| current.request_id == active.request_id)
        {
            *current = None;
            runtime
                .refresh_observation_revision
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Classify scan/setup failure and clear the exact active job without touching its old checkpoint.
    /// 分类扫描或准备失败，清除精确活动作业，不触碰其旧检查点。
    ///
    /// `runtime` and `active` select ownership; `error` is the worker diagnostic. Returns the retained error.
    /// `runtime` 与 `active` 选择归属；`error` 是工作线程诊断。返回保留的错误。
    async fn fail_active_refresh(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        active: &VceActiveRefresh,
        error: String,
    ) -> String {
        // cancelled is checked before terminalizing the shared control.
        // cancelled 在将共享控制句柄转为终态之前检查。
        let cancelled = matches!(active.cancellation.check(), Err(VceError::RefreshCancelled));
        self.end_active_refresh(runtime, active).await;
        if cancelled {
            runtime
                .set_cancelled(
                    runtime
                        .committed_projection
                        .read()
                        .await
                        .engine
                        .checkpoint_version
                        .is_some(),
                )
                .await;
            super::services::emit_vce_runtime_changed(
                &self.event_emitter,
                self,
                &runtime.project_id,
            )
            .await;
            String::from(VCE_REFRESH_CANCELLED_ERROR)
        } else {
            self.fail_runtime(runtime, error).await
        }
    }

    /// Retain and publish one bounded project failure before returning it to the caller.
    /// 在向调用方返回前保留并发布一条有界项目失败。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the exact project whose operation failed.
    /// `runtime` 是操作失败的精确项目。
    /// `error` is the unbounded source diagnostic.
    /// `error` 是未限制长度的来源诊断。
    ///
    /// # Returns
    /// 返回值
    /// The same bounded diagnostic retained in runtime state.
    /// 返回与运行时状态中保留内容相同的有界诊断。
    async fn fail_runtime(&self, runtime: &Arc<VceProjectRuntime>, error: String) -> String {
        let error = bounded_error(error);
        runtime.set_failure(error.clone()).await;
        super::services::emit_vce_runtime_changed(&self.event_emitter, self, &runtime.project_id)
            .await;
        error
    }

    /// Stop one runtime after cancelling and joining its serialized refresh boundary.
    /// 在取消并汇合其串行刷新边界后停止一个运行时。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the exact runtime removed from the manager map.
    /// `runtime` 是从管理器映射中移除的精确运行时。
    ///
    /// # Returns
    /// 返回值
    /// Success after no refresh can use the deactivated HostAI registration.
    /// 在没有刷新可再使用已注销 HostAI 注册后返回成功。
    async fn stop_runtime(&self, runtime: &Arc<VceProjectRuntime>) -> Result<(), String> {
        runtime.operation_admission.stop();
        {
            // A coalesced successor replaces its one-shot control under the same admission lock.
            // 合并后续批次在相同准入锁下替换一次性控制句柄。
            // Cancel the installed owner after that handoff, then release admission before draining workers.
            // 在交接后取消已安装所有者，然后先释放准入锁再排空工作线程。
            let _admission = runtime.job_state.admission.lock().await;
            if let Some(job) = runtime.job_state.active.lock().await.as_ref() {
                job.cancel_for_shutdown();
            }
        }
        self.query_admission
            .write()
            .map_err(|_| String::from("VCE query admission lock is poisoned"))?
            .remove(&runtime.project_id);
        runtime.set_stopping().await;
        if let Some(active) = runtime.active_refresh.lock().await.clone() {
            active.cancellation.cancel();
        }
        let _operation_drain = runtime.operation_admission.gate.write().await;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        // Draining first prevents a completing publication from reattaching a disabled project's watch.
        // 先排空，防止正在完成的发布重新挂接已关闭项目的监听。
        let watcher = self
            .watcher
            .lock()
            .map_err(|_| String::from("VCE watcher registration lock is poisoned"))?
            .as_ref()
            .and_then(super::watcher::VceWeakWatcherService::upgrade);
        if let Some(watcher) = watcher {
            watcher.unwatch_project(&runtime.project_id).await;
        }
        // Shutdown may retry authorized cleanup, but cannot claim another host's unfinished staging.
        // 退出可重试已授权清理，但不能接管其他宿主的未完成暂存。
        let cleanup = match self
            .data_plane
            .try_acquire_project_writer(&runtime.project_id)
        {
            Ok(Some(_writer)) => runtime.index.retry_pending_cleanup().await,
            // A replacement may already own the project; teardown leaves persisted cleanup for its owner.
            // 替换可能已经持有项目；拆卸将持久清理保留给其所有者。
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        };
        // Always release the exact AI registration, even when storage cleanup failed.
        // 即使存储清理失败，也始终释放精确的 AI 注册。
        let deactivate = runtime.ai.deactivate();
        match (cleanup, deactivate) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(cleanup), Err(deactivate)) => Err(format!("{cleanup}; {deactivate}")),
        }
    }

    /// Publish committed retrieval eligibility without consulting asynchronous engine/job locks.
    /// 发布已提交检索资格，不读取异步引擎锁或作业锁。
    ///
    /// `runtime` owns the admitted physical contract; `checkpoint_version` comes from a
    /// restored/published engine. Returns an explicit error if the projection lock is poisoned.
    /// `runtime` 持有准入物理契约；`checkpoint_version` 来自已恢复或发布的引擎。
    /// 投影锁中毒时返回明确错误。
    async fn publish_query_admission(
        &self,
        runtime: &VceProjectRuntime,
        checkpoint_version: Option<u64>,
    ) -> Result<(), String> {
        // Published leases already contain exact provenance; admission performs no post-commit I/O.
        // 已发布租约已包含精确来源；准入不执行提交后输入输出。
        let admission = match checkpoint_version {
            Some(checkpoint_version) => {
                // publication retains matching engine, member producers and space grants together.
                // publication 共同保留匹配的引擎、成员生产来源及空间授权事实。
                let publication = runtime.current_engine.read().await.clone();
                let read_index = publication.read_index.as_ref().ok_or_else(|| {
                    String::from("VCE initialized publication has no retained read view")
                })?;
                let provenance = read_index.embedding_provenance().await?;
                if provenance.checkpoint_version != Some(checkpoint_version) {
                    return Err(String::from(
                        "VCE committed provenance does not match query publication",
                    ));
                }
                let publication_contract =
                    VcePublicationContract::for_runtime(runtime, provenance)?;
                Some(VceQueryAdmission {
                    settings_revision: runtime.config_revision.load(Ordering::Acquire),
                    physical_fingerprint: publication_contract.embedding_fingerprint.clone(),
                    checkpoint_version,
                    publication_contract,
                })
            }
            None => None,
        };
        // admissions serializes revocation and publication using only in-memory facts.
        // admissions 仅使用内存事实串行化撤销与发布。
        let mut admissions = self
            .query_admission
            .write()
            .map_err(|_| String::from("VCE query admission lock is poisoned"))?;
        if !runtime
            .operation_admission
            .accepting
            .load(Ordering::Acquire)
        {
            return Ok(());
        }
        match admission {
            Some(admission) => {
                admission.publish(&mut admissions, &runtime.config_revision);
            }
            None => {
                admissions.remove(&runtime.project_id);
            }
        }
        Ok(())
    }

    /// Return the stable serialization gate for one project lifecycle.
    /// 返回单个项目生命周期的稳定串行化闸门。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the authoritative project identity.
    /// `project_id` 是权威项目身份。
    ///
    /// # Returns
    /// 返回值
    /// A process-lifetime gate shared by every enable and disable command for the project.
    /// 返回该项目全部启用与禁用命令共享的进程生命周期闸门。
    async fn project_lifecycle_gate(&self, project_id: &str) -> Arc<Mutex<()>> {
        let mut gates = self.project_lifecycle_gates.lock().await;
        Arc::clone(
            gates
                .entry(project_id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// Resolve one active project or preserve its blocked activation diagnostic.
    /// 解析一个活动项目，或保留其受阻激活诊断。
    async fn require_project(&self, project_id: &str) -> Result<Arc<VceProjectRuntime>, String> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE runtime manager is shutting down"));
        }
        if let Some(runtime) = self.projects.read().await.get(project_id).cloned() {
            return Ok(runtime);
        }
        if let Some(failure) = self.activation_failures.read().await.get(project_id) {
            return Err(format!(
                "VCE project `{project_id}` is {}: {}",
                if failure.lifecycle == VceProjectLifecycle::BlockedConfiguration {
                    "blocked by configuration"
                } else {
                    "not active"
                },
                failure.error
            ));
        }
        Err(format!("VCE project `{project_id}` is disabled"))
    }
}

impl VceProjectRuntime {
    /// Sample the exact active refresh identity while releasing the control lock before any I/O.
    /// 采样精确活动刷新身份，在任何输入输出前释放控制锁。
    /// No parameters; returns the control revision and optional currently admitted worker identity.
    /// 无参数；返回控制修订及可选的当前已准入工作线程身份。
    async fn active_refresh_observation(&self) -> (u64, Option<String>) {
        // Sample both facts under one lock so a full idle-to-active-to-idle cycle is detectable.
        // 在同一锁内采样两个事实，确保可以检测空闲到活动再到空闲的完整周期。
        let active = self.active_refresh.lock().await;
        (
            self.refresh_observation_revision.load(Ordering::Relaxed),
            active.as_ref().map(|active| active.request_id.clone()),
        )
    }

    /// Admit one runtime operation while rejecting stale references after stop begins.
    /// 在停止开始后拒绝陈旧引用，并接纳一项运行时操作。
    ///
    /// # Returns
    /// 返回值
    /// A read lease held until the operation ends, or an explicit stopping diagnostic.
    /// 返回持有到操作结束的读租约，或明确的停止诊断。
    async fn begin_operation(&self) -> Result<RwLockReadGuard<'_, ()>, String> {
        self.operation_admission.begin(&self.project_id).await
    }

    /// Build one current management-safe project status.
    /// 构建一份当前管理安全项目状态。
    async fn status(&self) -> VceRuntimeProjectStatus {
        let host_status = self.host_status.read().await;
        // committed is a small metadata read, never the engine mutex held by LLM/embedding work.
        // committed 是少量元数据读取，绝不是 LLM／向量任务持有的引擎互斥锁。
        let committed = self.committed_projection.read().await;
        VceRuntimeProjectStatus {
            project_id: self.project_id.clone(),
            lifecycle: host_status.lifecycle,
            config_revision: self.config_revision.load(Ordering::Acquire),
            embedding_physical_fingerprint: self.ai.embedding_space().physical_fingerprint.clone(),
            engine: Some(committed.engine.clone()),
            workspace: Some(committed.workspace),
            last_error: host_status
                .last_error
                .clone()
                .or_else(|| committed.engine.last_error.clone()),
        }
    }

    /// Mark the host runtime as executing a private refresh.
    /// 将宿主运行时标记为正在执行私有刷新。
    async fn set_refreshing(&self) {
        let mut status = self.host_status.write().await;
        status.lifecycle = VceProjectLifecycle::Refreshing;
        status.last_error = None;
    }

    /// Mark the host runtime ready after atomic publication.
    /// 在原子发布后将宿主运行时标记为就绪。
    async fn set_ready(&self) {
        let mut status = self.host_status.write().await;
        status.lifecycle = self.ready_lifecycle();
        status.last_error = None;
    }

    /// Mark a cancelled refresh without hiding an already committed checkpoint.
    /// 标记一次已取消刷新，同时不隐藏已经提交的检查点。
    ///
    /// # Parameters
    /// 参数
    /// `has_checkpoint` states whether retrieval can continue from the previous publication.
    /// `has_checkpoint` 表示检索是否可继续读取之前的发布结果。
    async fn set_cancelled(&self, has_checkpoint: bool) {
        let mut status = self.host_status.write().await;
        status.lifecycle = if has_checkpoint {
            self.ready_lifecycle()
        } else {
            VceProjectLifecycle::Failed
        };
        status.last_error = Some(String::from(VCE_REFRESH_CANCELLED_ERROR));
    }

    /// Mark the runtime as stopping before its refresh gate is joined.
    /// 在汇合刷新闸门前将运行时标记为正在停止。
    async fn set_stopping(&self) {
        let mut status = self.host_status.write().await;
        status.lifecycle = VceProjectLifecycle::Stopping;
    }

    /// Select the usable lifecycle after every required HostAI route has passed activation.
    /// 在每条必需 HostAI 路由均通过激活后选择可用生命周期。
    ///
    /// # Returns
    /// 返回值
    /// The ready lifecycle; capability loss is handled by global fail-closed reconciliation.
    /// 返回就绪生命周期；能力丢失由全局失败关闭协调处理。
    fn ready_lifecycle(&self) -> VceProjectLifecycle {
        VceProjectLifecycle::Ready
    }

    /// Retain one bounded host or functional failure.
    /// 保留一条有界宿主或功能失败。
    async fn set_failure(&self, error: String) {
        let mut status = self.host_status.write().await;
        status.lifecycle = VceProjectLifecycle::Failed;
        status.last_error = Some(error);
    }
}

/// Validate one authoritative project identity and settings revision.
/// 校验一个权威项目身份与设置修订号。
///
/// # Parameters
/// 参数
/// `project_id` is the workbench-owned project key.
/// `project_id` 是工作台持有的项目键。
/// `config_revision` is the persisted optimistic revision.
/// `config_revision` 是持久化的乐观修订号。
///
/// # Returns
/// 返回值
/// Success for a non-empty identity and positive revision.
/// 对非空身份与正修订号返回成功。
fn validate_project_identity(project_id: &str, config_revision: u64) -> Result<(), String> {
    if project_id.trim().is_empty() {
        return Err(String::from("VCE project_id must not be empty"));
    }
    if config_revision == 0 {
        return Err(String::from(
            "VCE config_revision must be greater than zero",
        ));
    }
    Ok(())
}

/// Compare runtime-affecting settings without treating scheduling policy as index provenance.
/// 比较影响运行时的设置，不将调度策略当作索引来源。
///
/// `admitted` is the runtime's frozen behavior; `current` is persisted policy. Returns true when
/// only revision, scheduling, query behavior, execution budgets, or historical migration metadata changed.
/// `admitted` 为运行时冻结行为；`current` 为持久策略。仅修订、调度模式、间隔或历史迁移元数据
/// 以及查询行为／执行预算变化时返回真。
fn runtime_policy_matches(admitted: &ProjectVceSettings, current: &ProjectVceSettings) -> bool {
    // normalized changes only the explicitly policy-only fields; future fields remain compared.
    // normalized 仅调整明确的纯策略字段；未来新增字段仍参与比较。
    let mut normalized = current.clone();
    normalized.revision = admitted.revision;
    normalized.update_mode = admitted.update_mode;
    normalized.auto_update_min_interval_seconds = admitted.auto_update_min_interval_seconds;
    normalized.allow_query_rewrite = admitted.allow_query_rewrite;
    normalized.allow_rerank = admitted.allow_rerank;
    normalized.query_max_concurrency = admitted.query_max_concurrency;
    normalized.embedding_max_concurrency = admitted.embedding_max_concurrency;
    normalized.max_results = admitted.max_results;
    normalized.max_context_chars = admitted.max_context_chars;
    &normalized == admitted
}

/// Build one globally unique operation identity while retaining project diagnostics.
/// 构建一个全局唯一操作身份，同时保留项目诊断信息。
///
/// # Parameters
/// 参数
/// `project_id` identifies the owning project.
/// `project_id` 标识所属项目。
/// `operation` identifies the functional entry.
/// `operation` 标识功能入口。
///
/// # Returns
/// 返回值
/// A stable-shape unique operation ID.
/// 返回形状稳定的唯一操作 ID。
fn request_id(project_id: &str, operation: &str) -> String {
    format!("vce:{project_id}:{operation}:{}", Uuid::new_v4())
}

/// Bound one externally sourced diagnostic before retaining it in process state.
/// 在进程状态中保留外部诊断前对其进行长度限制。
///
/// # Parameters
/// 参数
/// `error` is the full source diagnostic.
/// `error` 是完整来源诊断。
///
/// # Returns
/// 返回值
/// At most 2048 Unicode scalar values.
/// 返回最多 2048 个 Unicode 标量值。
fn bounded_error(error: String) -> String {
    error.chars().take(2_048).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify committed admission has exact identities and never uses checkpoint zero as absence.
    /// 验证已提交准入使用精确身份，且不把零检查点当成缺失。
    ///
    /// Takes no parameters and returns nothing; mismatched contracts must not expose a checkpoint.
    /// 无参数、无返回值；不匹配契约不得暴露检查点。
    #[test]
    fn committed_query_admission_is_independent_from_engine_and_job_locks() {
        // admission contains committed facts only; running/failed update labels are not inputs.
        // admission 仅包含已提交事实；更新运行中或失败标签不是其输入。
        let admission = VceQueryAdmission {
            settings_revision: 3,
            physical_fingerprint: String::from("physical-a"),
            checkpoint_version: 0,
            publication_contract: VcePublicationContract {
                replacement_admission: None,
                project_id: String::from("project-a"),
                canonical_root: String::from("test-root"),
                embedding_fingerprint: String::from("physical-a"),
                dimension: 3,
                max_file_bytes: 1024,
                extra_ignore_patterns: Vec::new(),
                provenance: None,
            },
        };
        assert_eq!(admission.checkpoint_for(3, "physical-a"), Some(0));
        assert_eq!(admission.checkpoint_for(4, "physical-a"), None);
        assert_eq!(admission.checkpoint_for(3, "physical-b"), None);
    }

    /// Stable host identities preserve the project and root hash without truncation.
    /// 稳定宿主身份完整保留项目与根哈希且不截断。
    #[test]
    fn project_identity_contract_is_deterministic() {
        let project_id = "project-a";
        let root = r"D:\workspace\project-a";
        let root_hash = hex::encode(Sha256::digest(root.as_bytes()));

        assert_eq!(
            format!("project:{project_id}:root:{root_hash}"),
            format!("project:project-a:root:{root_hash}")
        );
        assert_eq!(
            format!("code-semantic:{project_id}:{root_hash}"),
            format!("code-semantic:project-a:{root_hash}")
        );
    }

    /// A checkpoint prepared before a policy save must publish with the revision protected by the lock.
    /// 策略保存之前准备的检查点必须使用锁保护下的修订进行发布。
    /// No parameters or return value; replays both orderings without timers, providers or storage.
    /// 无参数及返回值；不使用计时器、供应商或存储，重放两种顺序。
    #[test]
    fn delayed_checkpoint_publication_preserves_current_policy_revision() {
        // revision and admissions model the same atomic and write lock held by policy adoption.
        // revision 与 admissions 模拟策略接纳持有的相同原子值和写锁。
        let revision = AtomicU64::new(3);
        let admissions = std::sync::RwLock::new(BTreeMap::new());
        // prepared is the exact immutable checkpoint envelope captured before the policy update.
        // prepared 为策略更新前捕获的精确不可变检查点信封。
        let prepared = VceQueryAdmission {
            settings_revision: revision.load(Ordering::Acquire),
            physical_fingerprint: String::from("physical-a"),
            checkpoint_version: 7,
            publication_contract: VcePublicationContract {
                replacement_admission: None,
                project_id: String::from("project-a"),
                canonical_root: String::from("test-root"),
                embedding_fingerprint: String::from("physical-a"),
                dimension: 3,
                max_file_bytes: 1024,
                extra_ignore_patterns: Vec::new(),
                provenance: None,
            },
        };
        {
            // Publish first, then adopt a policy while preserving the committed checkpoint.
            // 先发布，再接纳策略，同时保留已提交检查点。
            let mut locked = admissions.write().expect("admission write lock");
            prepared.clone().publish(&mut locked, &revision);
            revision.store(4, Ordering::Release);
            locked
                .get_mut("project-a")
                .expect("published admission")
                .settings_revision = 4;
        }
        {
            // Replay a publication delayed behind that completed save, using the production insertion boundary.
            // 使用生产插入边界重放延迟到该保存完成后的发布。
            let mut locked = admissions.write().expect("delayed publication lock");
            prepared.publish(&mut locked, &revision);
            // published must admit the latest policy without changing the checkpoint or physical contract.
            // published 必须准入最新策略，同时不改变检查点或物理契约。
            let published = locked.get("project-a").expect("current publication");
            assert_eq!(published.checkpoint_for(4, "physical-a"), Some(7));
            assert_eq!(published.checkpoint_for(3, "physical-a"), None);
            assert_eq!(published.publication_contract.dimension, 3);
            assert_eq!(published.publication_contract.canonical_root, "test-root");
        }
    }

    /// Typed activation failures never infer configuration ownership from message text.
    /// 类型化激活失败绝不根据消息文本推断配置归属。
    #[test]
    fn activation_failure_classification_is_explicit() {
        let configuration = VceActivationError::configuration("route unavailable");
        let runtime = VceActivationError::runtime("HostAI text inside filesystem failure");

        assert_eq!(
            configuration.lifecycle,
            VceProjectLifecycle::BlockedConfiguration
        );
        assert_eq!(runtime.lifecycle, VceProjectLifecycle::Failed);
        // Only the typed authority decision can become a readable rebuild-required state.
        // 只有类型化权威决定可以成为可读取的需要重建状态。
        let rebuild = VceActivationError::from(VcePublicationError::RebuildRequired(
            "explicit decision".into(),
        ));
        let ordinary = VceActivationError::from(VcePublicationError::Rejected(
            "rebuild required in unrelated error text".into(),
        ));
        assert_eq!(rebuild.lifecycle, VceProjectLifecycle::RebuildRequired);
        assert_eq!(ordinary.lifecycle, VceProjectLifecycle::Failed);
        let refreshing = VceActivationError::refreshing("replacement is active");
        assert_eq!(refreshing.lifecycle, VceProjectLifecycle::Refreshing);
    }

    /// Verifies teardown rejects new work and cannot drain until every active lease ends.
    /// 验证拆卸会拒绝新工作，并且只有全部活动租约结束后才能完成排空。
    #[tokio::test]
    async fn operation_admission_rejects_stale_work_and_drains_existing_work() {
        let admission = VceOperationAdmission::accepting();
        let active_lease = admission
            .begin("project-a")
            .await
            .expect("active runtime should admit work");

        admission.stop();

        assert_eq!(
            admission
                .begin("project-a")
                .await
                .expect_err("stopping runtime should reject new work"),
            "VCE project `project-a` is stopping"
        );
        assert!(admission.gate.try_write().is_err());
        drop(active_lease);
        assert!(admission.gate.try_write().is_ok());
    }

    /// Verifies a cancellation requested before awaiting remains observable.
    /// 验证等待前发出的取消请求仍然可观察。
    #[tokio::test]
    async fn refresh_cancellation_is_sticky() {
        let cancellation = VceRefreshControl::default();
        cancellation.cancel();
        assert!(matches!(
            cancellation.check(),
            Err(VceError::RefreshCancelled)
        ));
    }

    /// Verify scheduling, query and execution policies preserve runtime identity while source changes do not.
    /// 验证调度、查询及执行策略保持运行时身份，源码变化则不能保持。
    ///
    /// Takes no parameters and returns nothing; every non-policy field remains compared by equality.
    /// 无参数、无返回值；所有非策略字段仍通过相等比较参与判断。
    #[test]
    fn policy_revision_changes_do_not_recreate_the_runtime() {
        // admitted represents the immutable behavior frozen for a running project.
        // admitted 表示为运行项目冻结的不可变行为。
        let admitted = ProjectVceSettings {
            enabled: true,
            ..ProjectVceSettings::default()
        };
        // current changes only scheduling and optimistic metadata.
        // current 仅改变调度与乐观元数据。
        let mut current = admitted.clone();
        current.revision = 2;
        current.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
        current.auto_update_min_interval_seconds = 600;
        assert!(runtime_policy_matches(&admitted, &current));
        current
            .extra_ignore_patterns
            .push(String::from("generated/**"));
        assert!(!runtime_policy_matches(&admitted, &current));
        current.extra_ignore_patterns.clear();
        current.max_results += 1;
        current.max_context_chars += 1;
        current.query_max_concurrency = 1;
        current.embedding_max_concurrency = 3;
        current.allow_query_rewrite = true;
        current.allow_rerank = false;
        assert!(runtime_policy_matches(&admitted, &current));
        current.enabled = false;
        assert!(!runtime_policy_matches(&admitted, &current));
    }
}

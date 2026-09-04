//! Project lifecycle and functional entry manager for native VCE integration.
//! 原生 VCE 集成的项目生命周期与功能入口管理器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify, RwLock, RwLockReadGuard};
use uuid::Uuid;
use vulcan_ai_contract::HostAiPort;
use vulcan_context_engine::{
    IndexAbortRequest, RefreshReason, RefreshRequest, RefreshResult, RetrieveRequest,
    RetrieveResult, VceEngine, VceEngineSnapshot, VceIndexBinding, VceIndexPort,
    VceManagementQueryRequest, VceManagementQueryResponse, VceManagementRequest,
    VceManagementSnapshot, VceStateIdentity, VceWorkspaceStatistics,
};
use vulcan_workbench_model::ProjectVceSettings;
use vulcan_workspace_store::vce_data_plane::{
    VceDataPlaneHandle, VceProjectIndex, VceProjectIndexConfig,
};

use crate::ServiceEventEmitter;

use super::{
    VceHostAiAdapter, VceHostAiRegistrationConfig, VceProjectSourceAdapter, VceSourceLimits,
};

/// System inference profile configured through the shared public AI settings.
/// 通过共享公共 AI 设置配置的系统推理 Profile。
pub(crate) const VCE_CONSUMER_PROFILE_ID: &str = "vce";

/// Stable preprocessing contract used before every VCE embedding call.
/// 每次 VCE 向量调用前使用的稳定预处理契约。
const VCE_PREPROCESSING_VERSION: &str = "vce-preprocess-v1";

/// Stable recursive semantic chunker contract persisted with VCE vectors.
/// 与 VCE 向量共同持久化的稳定递归语义切块契约。
const VCE_CHUNKER_VERSION: &str = "vce-recursive-v1";

/// Stable cancellation diagnostic shared by RPC and watcher retry classification.
/// RPC 与监听重试分类共同使用的稳定取消诊断。
pub(crate) const VCE_REFRESH_CANCELLED_ERROR: &str = "VCE refresh was cancelled by the host";

/// Project lifecycle states owned by the VulcanCode host.
/// 由 VulcanCode 宿主持有的项目生命周期状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VceProjectLifecycle {
    /// The project is being constructed and validated.
    /// 项目正在构建与校验。
    Activating,
    /// A compatible persisted engine state is being restored.
    /// 正在恢复一份兼容的持久化引擎状态。
    Restoring,
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

/// Shared process-wide manager for all enabled project VCE runtimes.
/// 所有已启用项目 VCE 运行时使用的进程级共享管理器。
#[derive(Clone)]
pub(crate) struct VceRuntimeManager {
    /// Shared HostAI port used by all project-specific registrations.
    /// 所有项目专属注册共同使用的共享 HostAI 端口。
    ai_port: Arc<dyn HostAiPort>,
    /// Host-owned shared VCE SQLite and LanceDB data plane.
    /// 宿主持有的共享 VCE SQLite 与 LanceDB 数据面。
    data_plane: VceDataPlaneHandle,
    /// Service event boundary used to publish lifecycle transitions as they occur.
    /// 用于在生命周期转换发生时发布事件的服务事件边界。
    event_emitter: ServiceEventEmitter,
    /// Successfully activated project runtimes keyed by workbench project ID.
    /// 按工作台项目 ID 索引的已成功激活项目运行时。
    projects: Arc<RwLock<BTreeMap<String, Arc<VceProjectRuntime>>>>,
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
    /// Project configuration revision.
    /// 项目配置修订号。
    config_revision: u64,
    /// Validated project-owned behavior and resource budgets.
    /// 经过校验的项目自有行为与资源预算。
    settings: ProjectVceSettings,
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
    current_engine: RwLock<Arc<Mutex<VceEngine>>>,
    /// Single-project refresh serialization gate.
    /// 单项目刷新串行化闸门。
    refresh_gate: Mutex<()>,
    /// Host lifecycle and latest boundary diagnostic.
    /// 宿主生命周期与最近边界诊断。
    host_status: RwLock<VceHostStatus>,
    /// Currently running refresh cancellation handle, if any.
    /// 当前正在运行的刷新取消句柄（如果有）。
    active_refresh: Mutex<Option<VceActiveRefresh>>,
    /// Admission and drain boundary for every runtime operation.
    /// 每项运行时操作的接纳与排空边界。
    operation_admission: VceOperationAdmission,
}

/// One host-cancellable refresh identity retained only while its future is running.
/// 一项仅在刷新 Future 运行期间保留的宿主可取消刷新身份。
#[derive(Clone)]
struct VceActiveRefresh {
    /// Stable logical refresh identity.
    /// 稳定逻辑刷新身份。
    request_id: String,
    /// Cooperative cancellation signal that drops the active VCE future.
    /// 用于丢弃活动 VCE Future 的协作式取消信号。
    cancellation: VceRefreshCancellation,
}

/// Race-safe one-shot cancellation signal for a project refresh.
/// 项目刷新的竞态安全一次性取消信号。
#[derive(Clone, Default)]
struct VceRefreshCancellation {
    /// Sticky flag prevents a notification from being lost before the waiter registers.
    /// 粘性标志防止等待器登记前丢失通知。
    cancelled: Arc<AtomicBool>,
    /// Async notification wakes the active refresh future.
    /// 异步通知唤醒活动刷新 Future。
    notify: Arc<Notify>,
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
    gate: RwLock<()>,
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
            gate: RwLock::new(()),
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

impl VceRefreshCancellation {
    /// Request cancellation exactly once.
    /// 精确请求一次取消。
    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.notify.notify_waiters();
        }
    }

    /// Wait until cancellation has been requested without losing an early signal.
    /// 等待取消请求且不丢失提前到达的信号。
    async fn cancelled(&self) {
        let notified = self.notify.notified();
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        notified.await;
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
    /// Whether the failure is caused by required configuration.
    /// 失败是否由必需配置导致。
    blocked_configuration: bool,
    /// Exact bounded activation diagnostic.
    /// 精确且有界的激活诊断。
    error: String,
}

/// Typed activation error separating host AI configuration from runtime failures.
/// 将宿主 AI 配置问题与运行时失败分离的类型化激活错误。
struct VceActivationError {
    /// Whether user or system AI configuration must change before retry.
    /// 重试前是否必须修改用户或系统 AI 配置。
    blocked_configuration: bool,
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
            blocked_configuration: true,
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
            blocked_configuration: false,
            error: error.to_string(),
        }
    }
}

impl VceRuntimeManager {
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
        data_plane: VceDataPlaneHandle,
        event_emitter: ServiceEventEmitter,
    ) -> Self {
        Self {
            ai_port,
            data_plane,
            event_emitter,
            projects: Arc::new(RwLock::new(BTreeMap::new())),
            activation_failures: Arc::new(RwLock::new(BTreeMap::new())),
            lifecycle_gate: Arc::new(RwLock::new(())),
            project_lifecycle_gates: Arc::new(Mutex::new(BTreeMap::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
            teardown_started: Arc::new(AtomicBool::new(false)),
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
        self.teardown_started.swap(false, Ordering::AcqRel)
    }

    /// Enable one project, restore compatible state, and perform an authoritative full refresh.
    /// 启用一个项目、恢复兼容状态并执行一次权威全量刷新。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the authoritative workbench identity.
    /// `project_id` 是权威工作台身份。
    /// `project_root` is the authoritative absolute project path.
    /// `project_root` 是权威绝对项目路径。
    /// `settings` is the complete validated persisted project settings revision.
    /// `settings` 是完整且经过校验的持久化项目设置修订版。
    ///
    /// # Returns
    /// 返回值
    /// The first completed refresh or an explicit activation/refresh error.
    /// 返回首次完成的刷新或明确的激活/刷新错误。
    pub(crate) async fn enable_project(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: ProjectVceSettings,
    ) -> Result<RefreshResult, String> {
        if !settings.enabled {
            return Err(String::from("VCE project settings are disabled"));
        }
        let _manager_lifecycle = self.lifecycle_gate.read().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE runtime manager is shutting down"));
        }
        let project_lifecycle = self.project_lifecycle_gate(project_id).await;
        let _project_lifecycle = project_lifecycle.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE runtime manager is shutting down"));
        }
        if let Some(previous) = self.projects.write().await.remove(project_id) {
            self.stop_runtime(&previous).await?;
        }
        let config_revision = settings.revision;
        match self
            .activate_project(project_id, project_root, settings)
            .await
        {
            Ok(runtime) => {
                self.activation_failures.write().await.remove(project_id);
                self.projects
                    .write()
                    .await
                    .insert(project_id.to_owned(), Arc::clone(&runtime));
                // Initial indexing is cancellable by a later disable or replacement command, so
                // lifecycle serialization ends immediately after the new runtime is published.
                // 初始索引可被后续禁用或替换命令取消，因此新运行时发布后立即结束生命周期串行化。
                drop(_project_lifecycle);
                drop(_manager_lifecycle);
                self.refresh_full_runtime(&runtime, RefreshReason::Initial)
                    .await
            }
            Err(error) => {
                self.projects.write().await.remove(project_id);
                self.activation_failures.write().await.insert(
                    project_id.to_owned(),
                    VceActivationFailure {
                        config_revision,
                        blocked_configuration: error.blocked_configuration,
                        error: bounded_error(error.error.clone()),
                    },
                );
                Err(error.error)
            }
        }
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
    pub(crate) async fn disable_project(&self, project_id: &str) -> Result<(), String> {
        validate_project_identity(project_id, 1)?;
        let _manager_lifecycle = self.lifecycle_gate.read().await;
        let project_lifecycle = self.project_lifecycle_gate(project_id).await;
        let _project_lifecycle = project_lifecycle.lock().await;
        let runtime = self.projects.write().await.remove(project_id);
        self.activation_failures.write().await.remove(project_id);
        if let Some(runtime) = runtime {
            self.stop_runtime(&runtime).await?;
        }
        Ok(())
    }

    /// Perform a manual or recovery full refresh for one enabled project.
    /// 为一个已启用项目执行手动或恢复全量刷新。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    /// `reason` records why the refresh was requested.
    /// `reason` 记录请求刷新的原因。
    ///
    /// # Returns
    /// 返回值
    /// The committed refresh result.
    /// 返回已提交的刷新结果。
    pub(crate) async fn refresh_full(
        &self,
        project_id: &str,
        reason: RefreshReason,
    ) -> Result<RefreshResult, String> {
        let runtime = self.require_project(project_id).await?;
        self.refresh_full_runtime(&runtime, reason).await
    }

    /// Rebuild every present Blob while keeping the previous checkpoint visible until commit.
    /// 重建全部现存 Blob，并在提交前保持旧检查点可见。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    /// `reason` records the host reason for the rebuild.
    /// `reason` 记录宿主发起重建的原因。
    ///
    /// # Returns
    /// 返回值
    /// The newly committed full-rebuild result.
    /// 返回新提交的完整重建结果。
    pub(crate) async fn rebuild_full(
        &self,
        project_id: &str,
        reason: RefreshReason,
    ) -> Result<RefreshResult, String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        self.begin_refresh(&runtime).await;
        let manifest = match runtime.source.scan_full().await {
            Ok(manifest) => manifest,
            Err(error) => return Err(self.fail_runtime(&runtime, error).await),
        };
        let request = RefreshRequest::full_rebuild(
            request_id(project_id, "rebuild"),
            reason,
            manifest.observations,
            None,
        );
        self.refresh_runtime_locked(&runtime, request).await
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
        runtime.index.cleanup_abandoned_staging().await
    }

    /// Cancel the currently running project refresh under an optional visible operation identity.
    /// 在可选可见操作身份约束下取消当前正在运行的项目刷新。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    /// `operation_id` optionally guards against cancelling a newer unrelated refresh.
    /// `operation_id` 可选用于防止取消较新的无关刷新。
    ///
    /// # Returns
    /// 返回值
    /// The stable logical refresh identity whose cancellation was requested.
    /// 返回已请求取消的稳定逻辑刷新身份。
    pub(crate) async fn cancel_refresh(
        &self,
        project_id: &str,
        operation_id: Option<&str>,
    ) -> Result<String, String> {
        let runtime = self.require_project(project_id).await?;
        let active = runtime
            .active_refresh
            .lock()
            .await
            .clone()
            .ok_or_else(|| String::from("VCE project has no active refresh to cancel"))?;
        if let Some(operation_id) = operation_id {
            let operation_id = operation_id.trim();
            if operation_id.is_empty()
                || (operation_id != active.request_id
                    && !operation_id.starts_with(format!("{}:", active.request_id).as_str()))
            {
                return Err(String::from(
                    "VCE cancel operation identity does not match the active refresh",
                ));
            }
        }
        active.cancellation.cancel();
        Ok(active.request_id)
    }

    /// Perform one watcher-driven path refresh, escalating ignore/directory changes to full.
    /// 执行一次监听驱动的路径刷新，并将 ignore/目录变化升级为全量刷新。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the exact project runtime.
    /// `project_id` 选择精确项目运行时。
    /// `relative_paths` is the normalized dirty path set.
    /// `relative_paths` 是规范化的脏路径集合。
    /// `reason` identifies the watcher source.
    /// `reason` 标识监听来源。
    ///
    /// # Returns
    /// 返回值
    /// A committed refresh, or `None` when the set produced no semantic change.
    /// 返回已提交刷新；集合未产生语义变化时返回 `None`。
    pub(crate) async fn refresh_paths(
        &self,
        project_id: &str,
        relative_paths: BTreeSet<String>,
        reason: RefreshReason,
    ) -> Result<Option<RefreshResult>, String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        self.begin_refresh(&runtime).await;
        let current_handle = runtime.current_engine.read().await.clone();
        let current_snapshot = current_handle.lock().await.workspace_snapshot();
        let removed_directory_requires_full = relative_paths.iter().any(|path| {
            !runtime.source.project_root().join(path).exists()
                && current_snapshot
                    .records
                    .iter()
                    .any(|record| record.path.starts_with(format!("{path}/").as_str()))
        });
        let delta = match runtime.source.scan_paths(relative_paths).await {
            Ok(delta) => delta,
            Err(error) => return Err(self.fail_runtime(&runtime, error).await),
        };
        if delta.requires_full_refresh || removed_directory_requires_full {
            let manifest = match runtime.source.scan_full().await {
                Ok(manifest) => manifest,
                Err(error) => return Err(self.fail_runtime(&runtime, error).await),
            };
            let request = RefreshRequest::full(
                request_id(project_id, "full"),
                reason,
                manifest.observations,
                None,
            );
            return self
                .refresh_runtime_locked(&runtime, request)
                .await
                .map(Some);
        }
        if delta.observations.is_empty() && delta.removed_paths.is_empty() {
            runtime.set_ready().await;
            super::services::emit_vce_runtime_changed(
                &self.event_emitter,
                self,
                &runtime.project_id,
            )
            .await;
            return Ok(None);
        }
        let request = RefreshRequest::paths(
            request_id(project_id, "paths"),
            reason,
            delta.observations,
            delta.removed_paths,
            None,
        );
        self.refresh_runtime_locked(&runtime, request)
            .await
            .map(Some)
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
        let engine_handle = runtime.current_engine.read().await.clone();
        let mut engine = engine_handle.lock().await;
        let checkpoint_version = engine.status().checkpoint_version.ok_or_else(|| {
            String::from("VCE project has no committed checkpoint available for retrieval")
        })?;
        let mut request = RetrieveRequest::current_checkpoint(
            request_id(project_id, "retrieve"),
            information_request,
        );
        request.allow_query_rewrite = runtime.settings.allow_query_rewrite;
        request.allow_rerank = runtime.settings.allow_rerank;
        let result = engine
            .retrieve(request, runtime.index.as_ref(), runtime.ai.as_ref())
            .await
            .map_err(|error| error.to_string())?;
        Ok(VceRuntimeRetrieveResult {
            settings_revision: runtime.config_revision,
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
    /// Combined engine, AI, workspace, and index status.
    /// 返回组合后的引擎、AI、工作区与索引状态。
    pub(crate) async fn management_snapshot(
        &self,
        project_id: &str,
    ) -> Result<VceManagementSnapshot, String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let engine_handle = runtime.current_engine.read().await.clone();
        let engine = engine_handle.lock().await;
        engine
            .management_snapshot(
                VceManagementRequest {
                    request_id: request_id(project_id, "management"),
                },
                runtime.index.as_ref(),
                runtime.ai.as_ref(),
            )
            .await
            .map_err(|error| error.to_string())
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
    ///
    /// # Returns
    /// 返回值
    /// The validated VCE management response.
    /// 返回经过校验的 VCE 管理响应。
    pub(crate) async fn management_query(
        &self,
        project_id: &str,
        request: VceManagementQueryRequest,
    ) -> Result<VceManagementQueryResponse, String> {
        let runtime = self.require_project(project_id).await?;
        let _operation = runtime.begin_operation().await?;
        let engine_handle = runtime.current_engine.read().await.clone();
        let engine = engine_handle.lock().await;
        engine
            .management_query(request, runtime.index.as_ref())
            .await
            .map_err(|error| error.to_string())
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
                lifecycle: if failure.blocked_configuration {
                    VceProjectLifecycle::BlockedConfiguration
                } else {
                    VceProjectLifecycle::Failed
                },
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
    /// `true` only for a current usable runtime with a committed checkpoint; contention fails closed.
    /// 仅对拥有已提交检查点的当前可用运行时返回 `true`；锁竞争按失败关闭处理。
    pub(crate) fn project_can_serve_now(
        &self,
        project_id: &str,
        config_revision: u64,
        embedding_physical_fingerprint: &str,
    ) -> bool {
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        let Ok(projects) = self.projects.try_read() else {
            return false;
        };
        let Some(runtime) = projects.get(project_id) else {
            return false;
        };
        if runtime.config_revision != config_revision
            || runtime.ai.embedding_space().physical_fingerprint.as_deref()
                != Some(embedding_physical_fingerprint)
        {
            return false;
        }
        let Ok(host_status) = runtime.host_status.try_read() else {
            return false;
        };
        if !matches!(
            host_status.lifecycle,
            VceProjectLifecycle::Ready | VceProjectLifecycle::Refreshing
        ) {
            return false;
        }
        let Ok(engine_handle) = runtime.current_engine.try_read() else {
            return false;
        };
        let Ok(engine) = engine_handle.try_lock() else {
            return false;
        };
        engine.status().checkpoint_version.is_some()
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
        self.teardown_started.store(true, Ordering::Release);
        let runtimes = std::mem::take(&mut *self.projects.write().await)
            .into_values()
            .collect::<Vec<_>>();
        self.activation_failures.write().await.clear();
        let mut failures = Vec::new();
        for runtime in runtimes {
            if let Err(error) = self.stop_runtime(&runtime).await {
                failures.push(error);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    /// Construct one project runtime from exact host identities and compatible persisted state.
    /// 根据精确宿主身份与兼容持久化状态构造一个项目运行时。
    async fn activate_project(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: ProjectVceSettings,
    ) -> Result<Arc<VceProjectRuntime>, VceActivationError> {
        validate_project_identity(project_id, settings.revision)
            .map_err(VceActivationError::runtime)?;
        settings
            .validate()
            .map_err(VceActivationError::configuration)?;
        let preliminary_stage_bytes = (4 * 1_048_576_u64).max(settings.max_file_bytes);
        let preliminary_limits =
            VceSourceLimits::new(settings.max_file_bytes, 8, preliminary_stage_bytes)
                .map_err(VceActivationError::runtime)?;
        let source = Arc::new(
            VceProjectSourceAdapter::activate(
                project_root,
                preliminary_limits,
                &settings.extra_ignore_patterns,
            )
            .map_err(VceActivationError::runtime)?,
        );
        let project_root = crate::normalized_workspace_path_text_from_path(source.project_root());
        let root_hash = hex::encode(Sha256::digest(project_root.as_bytes()));
        let checkpoint_chain_id = format!("project:{project_id}:root:{root_hash}");
        let vector_space_id = format!("code-semantic:{project_id}:{root_hash}");
        let ai = Arc::new(
            VceHostAiAdapter::activate(
                Arc::clone(&self.ai_port),
                VceHostAiRegistrationConfig {
                    project_id: project_id.to_owned(),
                    consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                    vector_space_id: vector_space_id.clone(),
                    dimension: None,
                    preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                    chunker_version: String::from(VCE_CHUNKER_VERSION),
                },
            )
            .map_err(VceActivationError::configuration)?,
        );
        let activation = async {
            let mut config = vulcan_context_engine::VceConfig::new(
                checkpoint_chain_id.clone(),
                ai.embedding_space().clone(),
            )
            .map_err(|error| error.to_string())?;
            config.indexing.max_file_bytes = settings.max_file_bytes;
            config.index_stage_max_bytes =
                config.index_stage_max_bytes.max(settings.max_file_bytes);
            config.embedding_max_concurrency = settings.embedding_max_concurrency;
            config.query_max_concurrency = settings.query_max_concurrency;
            config.retrieval.coverage.max_results = settings.max_results;
            config.retrieval.coverage.max_chars = settings.max_context_chars;
            config.validate().map_err(|error| error.to_string())?;
            let source_limits = VceSourceLimits::new(
                config.indexing.max_file_bytes,
                config.index_stage_blob_limit,
                config.index_stage_max_bytes,
            )?;
            let source = Arc::new(VceProjectSourceAdapter::activate(
                source.project_root(),
                source_limits,
                &settings.extra_ignore_patterns,
            )?);
            let binding = VceIndexBinding::new(
                config.index_namespace_id.clone(),
                checkpoint_chain_id,
                config.index_schema_version,
                ai.embedding_space().clone(),
            )
            .map_err(|error| error.to_string())?;
            let dimension = u32::try_from(ai.embedding_space().dimension)
                .map_err(|error| format!("VCE embedding dimension exceeds u32: {error}"))?;
            let physical_fingerprint = ai
                .embedding_space()
                .physical_fingerprint
                .clone()
                .ok_or_else(|| String::from("VCE physical embedding fingerprint is missing"))?;
            let index = Arc::new(
                self.data_plane
                    .open_project_index(VceProjectIndexConfig {
                        project_id: project_id.to_owned(),
                        root_path: project_root.clone(),
                        root_hash,
                        binding,
                        vector_space_id,
                        physical_fingerprint,
                        preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                        chunker_version: String::from(VCE_CHUNKER_VERSION),
                        dimension,
                        config_revision: settings.revision,
                    })
                    .await?,
            );
            let expected_identity = VceStateIdentity::new(
                config.index_namespace_id.clone(),
                config.checkpoint_chain_id.clone(),
                config.index_schema_version,
                config.embedding_space.clone(),
            );
            let (engine, initial_lifecycle) = match self
                .data_plane
                .load_state_snapshot(&config.index_namespace_id)
                .await?
            {
                Some(snapshot) if snapshot.identity == expected_identity => {
                    match VceEngine::from_state_snapshot(config.clone(), snapshot) {
                        Ok(engine) => (engine, VceProjectLifecycle::Restoring),
                        Err(_) => {
                            index.reset_incompatible_state().await?;
                            (
                                VceEngine::new(config.clone())
                                    .map_err(|error| error.to_string())?,
                                VceProjectLifecycle::RebuildRequired,
                            )
                        }
                    }
                }
                Some(_) => {
                    index.reset_incompatible_state().await?;
                    (
                        VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                        VceProjectLifecycle::RebuildRequired,
                    )
                }
                None => (
                    VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                    VceProjectLifecycle::Activating,
                ),
            };
            Ok::<_, String>(Arc::new(VceProjectRuntime {
                project_id: project_id.to_owned(),
                config,
                config_revision: settings.revision,
                settings,
                ai: Arc::clone(&ai),
                index,
                source,
                current_engine: RwLock::new(Arc::new(Mutex::new(engine))),
                refresh_gate: Mutex::new(()),
                host_status: RwLock::new(VceHostStatus {
                    lifecycle: initial_lifecycle,
                    last_error: None,
                }),
                active_refresh: Mutex::new(None),
                operation_admission: VceOperationAdmission::accepting(),
            }))
        }
        .await;
        match activation {
            Ok(runtime) => Ok(runtime),
            Err(error) => match ai.deactivate() {
                Ok(()) => Err(VceActivationError::runtime(error)),
                Err(cleanup_error) => Err(VceActivationError::runtime(format!(
                    "{error}; cleanup failed VCE HostAI activation: {cleanup_error}"
                ))),
            },
        }
    }

    /// Refresh one project from a complete filesystem manifest under its serial gate.
    /// 在项目串行闸门内使用完整文件系统清单刷新一个项目。
    async fn refresh_full_runtime(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        reason: RefreshReason,
    ) -> Result<RefreshResult, String> {
        let _operation = runtime.begin_operation().await?;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        self.begin_refresh(runtime).await;
        let manifest = match runtime.source.scan_full().await {
            Ok(manifest) => manifest,
            Err(error) => return Err(self.fail_runtime(runtime, error).await),
        };
        let requires_rebuild =
            runtime.host_status.read().await.lifecycle == VceProjectLifecycle::RebuildRequired;
        let refresh_id = request_id(
            &runtime.project_id,
            if requires_rebuild { "rebuild" } else { "full" },
        );
        let request = if requires_rebuild {
            RefreshRequest::full_rebuild(refresh_id, reason, manifest.observations, None)
        } else {
            RefreshRequest::full(refresh_id, reason, manifest.observations, None)
        };
        self.refresh_runtime_locked(runtime, request).await
    }

    /// Execute one refresh on a private engine and atomically publish it after persistence.
    /// 在私有引擎上执行一次刷新，并在持久化后原子发布。
    async fn refresh_runtime_locked(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        request: RefreshRequest,
    ) -> Result<RefreshResult, String> {
        let current_handle = runtime.current_engine.read().await.clone();
        let base_snapshot = current_handle.lock().await.state_snapshot();
        let base_generation = base_snapshot.workspace.generation;
        let base_has_checkpoint = base_snapshot.workspace.checkpoint.is_some();
        let mut working =
            match VceEngine::from_state_snapshot(runtime.config.clone(), base_snapshot) {
                Ok(working) => working,
                Err(error) => return Err(self.fail_runtime(runtime, error.to_string()).await),
            };
        let request_id = request.request_id.clone();
        let cancellation = VceRefreshCancellation::default();
        {
            let mut active = runtime.active_refresh.lock().await;
            *active = Some(VceActiveRefresh {
                request_id: request_id.clone(),
                cancellation: cancellation.clone(),
            });
        }
        // refresh_result owns no borrow after this scope, allowing cancellation cleanup to inspect
        // the private working engine before deciding whether an unpublished generation exists.
        // refresh_result 在此作用域后不再持有借用，使取消清理可检查私有工作引擎并判断是否
        // 存在未发布代次。
        let refresh_result = {
            let refresh = working.refresh(
                request,
                runtime.index.as_ref(),
                runtime.ai.as_ref(),
                runtime.source.as_ref(),
            );
            tokio::pin!(refresh);
            tokio::select! {
                biased;
                () = cancellation.cancelled() => None,
                result = &mut refresh => Some(result),
            }
        };
        {
            let mut active = runtime.active_refresh.lock().await;
            if active
                .as_ref()
                .is_some_and(|active| active.request_id == request_id)
            {
                *active = None;
            }
        }
        let Some(result) = refresh_result else {
            let cancelled_generation = working.workspace_snapshot().generation;
            if cancelled_generation > base_generation {
                let abort_result = runtime
                    .index
                    .abort_stage(IndexAbortRequest {
                        operation_id: format!("{request_id}:cancel"),
                        refresh_id: request_id,
                        generation: cancelled_generation,
                    })
                    .await;
                if let Err(error) = abort_result {
                    return Err(self
                        .fail_runtime(
                            runtime,
                            format!(
                                "{VCE_REFRESH_CANCELLED_ERROR}; staging cleanup failed: {error}"
                            ),
                        )
                        .await);
                }
            }
            runtime.set_cancelled(base_has_checkpoint).await;
            super::services::emit_vce_runtime_changed(
                &self.event_emitter,
                self,
                &runtime.project_id,
            )
            .await;
            return Err(String::from(VCE_REFRESH_CANCELLED_ERROR));
        };
        match result {
            Ok(result) => {
                // A checkpoint-changing refresh persisted its state atomically inside commit.
                // A no-op refresh has no commit transaction, so persist its counters and summary here.
                // 改变检查点的刷新已在提交事务内原子持久化状态；无提交的刷新则在此持久化计数与摘要。
                if result.commit_receipt.is_none() {
                    let snapshot = working.state_snapshot();
                    if let Err(error) = self
                        .data_plane
                        .persist_state_snapshot(&runtime.config.index_namespace_id, &snapshot)
                        .await
                    {
                        return Err(self.fail_runtime(runtime, error).await);
                    }
                }
                *runtime.current_engine.write().await = Arc::new(Mutex::new(working));
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

    /// Publish the beginning of one serialized refresh before source scanning starts.
    /// 在源码扫描开始前发布一次串行刷新已启动状态。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the exact admitted project runtime.
    /// `runtime` 是已接纳的精确项目运行时。
    async fn begin_refresh(&self, runtime: &Arc<VceProjectRuntime>) {
        runtime.set_refreshing().await;
        super::services::emit_vce_runtime_changed(&self.event_emitter, self, &runtime.project_id)
            .await;
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
        runtime.set_stopping().await;
        if let Some(active) = runtime.active_refresh.lock().await.clone() {
            active.cancellation.cancel();
        }
        let _operation_drain = runtime.operation_admission.gate.write().await;
        let _refresh_guard = runtime.refresh_gate.lock().await;
        runtime.index.cleanup_abandoned_staging().await?;
        runtime.ai.deactivate()
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
                if failure.blocked_configuration {
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
        let engine_handle = self.current_engine.read().await.clone();
        let engine = engine_handle.lock().await;
        VceRuntimeProjectStatus {
            project_id: self.project_id.clone(),
            lifecycle: host_status.lifecycle,
            config_revision: self.config_revision,
            embedding_physical_fingerprint: self.ai.embedding_space().physical_fingerprint.clone(),
            engine: Some(engine.status()),
            workspace: Some(engine.workspace_statistics()),
            last_error: host_status
                .last_error
                .clone()
                .or_else(|| engine.status().last_error),
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

    /// Typed activation failures never infer configuration ownership from message text.
    /// 类型化激活失败绝不根据消息文本推断配置归属。
    #[test]
    fn activation_failure_classification_is_explicit() {
        let configuration = VceActivationError::configuration("route unavailable");
        let runtime = VceActivationError::runtime("HostAI text inside filesystem failure");

        assert!(configuration.blocked_configuration);
        assert!(!runtime.blocked_configuration);
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
        let cancellation = VceRefreshCancellation::default();
        cancellation.cancel();

        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            cancellation.cancelled(),
        )
        .await
        .expect("an early cancellation should resolve immediately");
    }
}

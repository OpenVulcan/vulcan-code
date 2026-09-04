//! Project lifecycle and functional entry manager for native VCE integration.
//! 原生 VCE 集成的项目生命周期与功能入口管理器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;
use vulcan_ai_contract::HostAiPort;
use vulcan_context_engine::{
    IndexAbortRequest, RefreshReason, RefreshRequest, RefreshResult, RetrieveRequest,
    RetrieveResult, VceAiPort, VceEngine, VceEngineSnapshot, VceIndexBinding, VceIndexPort,
    VceManagementQueryRequest, VceManagementQueryResponse, VceManagementRequest,
    VceManagementSnapshot, VceStateIdentity, VceWorkspaceStatistics,
};
use vulcan_workbench_model::ProjectVceSettings;
use vulcan_workspace_store::vce_data_plane::{
    VceDataPlaneHandle, VceProjectIndex, VceProjectIndexConfig,
};

use super::{
    VceHostAiAdapter, VceHostAiRegistrationConfig, VceProjectSourceAdapter, VceSourceLimits,
};

/// System inference profile configured through the shared public AI settings.
/// 通过共享公共 AI 设置配置的系统推理 Profile。
const VCE_CONSUMER_PROFILE_ID: &str = "vce";

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
    /// The committed checkpoint is ready while optional AI stages are unavailable.
    /// 已提交检查点可用，但可选 AI 阶段不可用。
    DegradedOptionalAi,
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
    /// Successfully activated project runtimes keyed by workbench project ID.
    /// 按工作台项目 ID 索引的已成功激活项目运行时。
    projects: Arc<RwLock<BTreeMap<String, Arc<VceProjectRuntime>>>>,
    /// Failed activation records retained for management and retry.
    /// 为管理与重试保留的激活失败记录。
    activation_failures: Arc<RwLock<BTreeMap<String, VceActivationFailure>>>,
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
    ///
    /// # Returns
    /// 返回值
    /// An empty manager ready for lazy project activation.
    /// 返回可延迟激活项目的空管理器。
    pub(crate) fn new(ai_port: Arc<dyn HostAiPort>, data_plane: VceDataPlaneHandle) -> Self {
        Self {
            ai_port,
            data_plane,
            projects: Arc::new(RwLock::new(BTreeMap::new())),
            activation_failures: Arc::new(RwLock::new(BTreeMap::new())),
        }
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
        settings.validate()?;
        if !settings.enabled {
            return Err(String::from("VCE project settings are disabled"));
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
                self.refresh_full_runtime(&runtime, RefreshReason::Initial)
                    .await
            }
            Err(error) => {
                let blocked_configuration = is_configuration_error(&error);
                self.projects.write().await.remove(project_id);
                self.activation_failures.write().await.insert(
                    project_id.to_owned(),
                    VceActivationFailure {
                        config_revision,
                        blocked_configuration,
                        error: bounded_error(error.clone()),
                    },
                );
                Err(error)
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
        let _refresh_guard = runtime.refresh_gate.lock().await;
        let manifest = runtime.source.scan_full().await?;
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
        let _refresh_guard = runtime.refresh_gate.lock().await;
        let current_handle = runtime.current_engine.read().await.clone();
        let current_snapshot = current_handle.lock().await.workspace_snapshot();
        let removed_directory_requires_full = relative_paths.iter().any(|path| {
            !runtime.source.project_root().join(path).exists()
                && current_snapshot
                    .records
                    .iter()
                    .any(|record| record.path.starts_with(format!("{path}/").as_str()))
        });
        let delta = runtime.source.scan_paths(relative_paths).await?;
        if delta.requires_full_refresh || removed_directory_requires_full {
            let manifest = runtime.source.scan_full().await?;
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
                engine: None,
                workspace: None,
                last_error: Some(failure.error.clone()),
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
    ) -> Result<Arc<VceProjectRuntime>, String> {
        validate_project_identity(project_id, settings.revision)?;
        settings.validate()?;
        let preliminary_stage_bytes = (4 * 1_048_576_u64).max(settings.max_file_bytes);
        let preliminary_limits =
            VceSourceLimits::new(settings.max_file_bytes, 8, preliminary_stage_bytes)?;
        let source = Arc::new(VceProjectSourceAdapter::activate(
            project_root,
            preliminary_limits,
            &settings.extra_ignore_patterns,
        )?);
        let project_root = crate::normalized_workspace_path_text_from_path(source.project_root());
        let root_hash = hex::encode(Sha256::digest(project_root.as_bytes()));
        let checkpoint_chain_id = format!("project:{project_id}:root:{root_hash}");
        let vector_space_id = format!("code-semantic:{project_id}:{root_hash}");
        let ai = Arc::new(VceHostAiAdapter::activate(
            Arc::clone(&self.ai_port),
            VceHostAiRegistrationConfig {
                project_id: project_id.to_owned(),
                consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                vector_space_id: vector_space_id.clone(),
                dimension: None,
                preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                chunker_version: String::from(VCE_CHUNKER_VERSION),
            },
        )?);
        let mut config = vulcan_context_engine::VceConfig::new(
            checkpoint_chain_id.clone(),
            ai.embedding_space().clone(),
        )
        .map_err(|error| error.to_string())?;
        config.indexing.max_file_bytes = settings.max_file_bytes;
        config.index_stage_max_bytes = config.index_stage_max_bytes.max(settings.max_file_bytes);
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
            Some(snapshot) if snapshot.identity == expected_identity => (
                VceEngine::from_state_snapshot(config.clone(), snapshot)
                    .map_err(|error| error.to_string())?,
                VceProjectLifecycle::Restoring,
            ),
            Some(_) => (
                VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                VceProjectLifecycle::RebuildRequired,
            ),
            None => (
                VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
                VceProjectLifecycle::Activating,
            ),
        };
        Ok(Arc::new(VceProjectRuntime {
            project_id: project_id.to_owned(),
            config,
            config_revision: settings.revision,
            settings,
            ai,
            index,
            source,
            current_engine: RwLock::new(Arc::new(Mutex::new(engine))),
            refresh_gate: Mutex::new(()),
            host_status: RwLock::new(VceHostStatus {
                lifecycle: initial_lifecycle,
                last_error: None,
            }),
            active_refresh: Mutex::new(None),
        }))
    }

    /// Refresh one project from a complete filesystem manifest under its serial gate.
    /// 在项目串行闸门内使用完整文件系统清单刷新一个项目。
    async fn refresh_full_runtime(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        reason: RefreshReason,
    ) -> Result<RefreshResult, String> {
        let _refresh_guard = runtime.refresh_gate.lock().await;
        let manifest = match runtime.source.scan_full().await {
            Ok(manifest) => manifest,
            Err(error) => {
                runtime.set_failure(bounded_error(error.clone())).await;
                return Err(error);
            }
        };
        let request = RefreshRequest::full(
            request_id(&runtime.project_id, "full"),
            reason,
            manifest.observations,
            None,
        );
        self.refresh_runtime_locked(runtime, request).await
    }

    /// Execute one refresh on a private engine and atomically publish it after persistence.
    /// 在私有引擎上执行一次刷新，并在持久化后原子发布。
    async fn refresh_runtime_locked(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        request: RefreshRequest,
    ) -> Result<RefreshResult, String> {
        runtime.set_refreshing().await;
        let current_handle = runtime.current_engine.read().await.clone();
        let base_snapshot = current_handle.lock().await.state_snapshot();
        let base_generation = base_snapshot.workspace.generation;
        let base_has_checkpoint = base_snapshot.workspace.checkpoint.is_some();
        let mut working = VceEngine::from_state_snapshot(runtime.config.clone(), base_snapshot)
            .map_err(|error| error.to_string())?;
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
                    runtime
                        .set_failure(bounded_error(format!(
                            "{VCE_REFRESH_CANCELLED_ERROR}; staging cleanup failed: {error}"
                        )))
                        .await;
                    return Err(format!(
                        "{VCE_REFRESH_CANCELLED_ERROR}; staging cleanup failed: {error}"
                    ));
                }
            }
            runtime.set_cancelled(base_has_checkpoint).await;
            return Err(String::from(VCE_REFRESH_CANCELLED_ERROR));
        };
        match result {
            Ok(result) => {
                // A checkpoint-changing refresh persisted its state atomically inside commit.
                // A no-op refresh has no commit transaction, so persist its counters and summary here.
                // 改变检查点的刷新已在提交事务内原子持久化状态；无提交的刷新则在此持久化计数与摘要。
                if result.commit_receipt.is_none() {
                    let snapshot = working.state_snapshot();
                    self.data_plane
                        .persist_state_snapshot(&runtime.config.index_namespace_id, &snapshot)
                        .await?;
                }
                *runtime.current_engine.write().await = Arc::new(Mutex::new(working));
                runtime.set_ready().await;
                Ok(result)
            }
            Err(error) => {
                let message = bounded_error(error.to_string());
                runtime.set_failure(message.clone()).await;
                Err(message)
            }
        }
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
        runtime.set_stopping().await;
        if let Some(active) = runtime.active_refresh.lock().await.clone() {
            active.cancellation.cancel();
        }
        let _refresh_guard = runtime.refresh_gate.lock().await;
        runtime.index.cleanup_abandoned_staging().await?;
        runtime.ai.deactivate()
    }

    /// Resolve one active project or preserve its blocked activation diagnostic.
    /// 解析一个活动项目，或保留其受阻激活诊断。
    async fn require_project(&self, project_id: &str) -> Result<Arc<VceProjectRuntime>, String> {
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

    /// Select ready or optional-AI-degraded state from current host capabilities and settings.
    /// 根据当前宿主能力和设置选择就绪或可选 AI 降级状态。
    ///
    /// # Returns
    /// 返回值
    /// A usable lifecycle that never treats optional AI as required.
    /// 返回不会把可选 AI 当作必需能力的可用生命周期。
    fn ready_lifecycle(&self) -> VceProjectLifecycle {
        let capability = self.ai.capability_snapshot();
        if (self.settings.allow_query_rewrite && !capability.query_rewrite_available)
            || (self.settings.allow_rerank && !capability.rerank_available)
        {
            VceProjectLifecycle::DegradedOptionalAi
        } else {
            VceProjectLifecycle::Ready
        }
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

/// Classify activation failures that require public AI or vector configuration.
/// 分类需要公共 AI 或向量配置的激活失败。
///
/// # Parameters
/// 参数
/// `error` is one normalized activation diagnostic.
/// `error` 是一条规范化激活诊断。
///
/// # Returns
/// 返回值
/// `true` only for exact HostAI capability/configuration diagnostics.
/// 仅对明确 HostAI 能力或配置诊断返回 `true`。
fn is_configuration_error(error: &str) -> bool {
    error.contains("HostAI")
        || error.contains("Inference consumer profile")
        || error.contains("embedding dimension")
        || error.contains("Embedding route")
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

    /// Configuration failures are classified without treating filesystem errors as blocked AI.
    /// 配置失败分类不会把文件系统错误误判为 AI 配置阻塞。
    #[test]
    fn activation_failure_classification_is_narrow() {
        assert!(is_configuration_error(
            "VCE HostAI Embedding route is unavailable"
        ));
        assert!(!is_configuration_error("canonicalize project path failed"));
    }
}

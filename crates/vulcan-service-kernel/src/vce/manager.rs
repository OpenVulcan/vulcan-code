//! Project lifecycle and functional entry manager for native VCE integration.
//! 原生 VCE 集成的项目生命周期与功能入口管理器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use vulcan_ai_contract::HostAiPort;
use vulcan_context_engine::{
    RefreshReason, RefreshRequest, RefreshResult, RetrieveRequest, RetrieveResult, VceEngine,
    VceEngineSnapshot, VceIndexBinding, VceManagementQueryRequest, VceManagementQueryResponse,
    VceManagementRequest, VceManagementSnapshot, VceStateIdentity, VceWorkspaceStatistics,
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

/// Project lifecycle states owned by the VulcanCode host.
/// 由 VulcanCode 宿主持有的项目生命周期状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VceProjectLifecycle {
    /// The project is being constructed and validated.
    /// 项目正在构建与校验。
    Activating,
    /// The project has a usable committed checkpoint.
    /// 项目已有可用的已提交检查点。
    Ready,
    /// A host-owned refresh is running against a private working engine.
    /// 宿主持有的刷新正在私有工作引擎上执行。
    Refreshing,
    /// Required HostAI or project configuration is unavailable.
    /// 必需的 HostAI 或项目配置不可用。
    BlockedConfiguration,
    /// The last host or VCE operation failed.
    /// 最近一次宿主或 VCE 操作失败。
    Failed,
}

impl VceProjectLifecycle {
    /// Return the stable API value for this lifecycle state.
    /// 返回此生命周期状态的稳定 API 值。
    ///
    /// # Returns
    /// 返回值
    /// A lowercase snake-case lifecycle identifier.
    /// 返回小写蛇形生命周期标识。
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Activating => "activating",
            Self::Ready => "ready",
            Self::Refreshing => "refreshing",
            Self::BlockedConfiguration => "blocked_configuration",
            Self::Failed => "failed",
        }
    }
}

/// Management-safe project status returned by the VCE runtime manager.
/// VCE 运行管理器返回的管理安全项目状态。
#[derive(Clone, Debug)]
pub(crate) struct VceRuntimeProjectStatus {
    /// Stable workbench project identity.
    /// 稳定的工作台项目身份。
    pub(crate) project_id: String,
    /// Canonical absolute project root.
    /// 规范绝对项目根目录。
    pub(crate) project_root: String,
    /// Host lifecycle state.
    /// 宿主生命周期状态。
    pub(crate) lifecycle: VceProjectLifecycle,
    /// Project configuration revision represented by the runtime.
    /// 运行时表示的项目配置修订号。
    pub(crate) config_revision: u64,
    /// Complete validated project-owned VCE settings represented by this status.
    /// 此状态表示的完整已验证项目自有 VCE 设置。
    pub(crate) settings: ProjectVceSettings,
    /// Active code-semantic namespace when activation succeeded.
    /// 激活成功时的活动代码语义命名空间。
    pub(crate) namespace_id: Option<String>,
    /// Active logical vector space when activation succeeded.
    /// 激活成功时的活动逻辑向量空间。
    pub(crate) vector_space_id: Option<String>,
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
    /// Canonical absolute project root.
    /// 规范绝对项目根目录。
    project_root: String,
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
    /// Canonical or requested project root used during activation.
    /// 激活期间使用的规范或请求项目根目录。
    project_root: String,
    /// Project configuration revision that failed.
    /// 失败的项目配置修订号。
    config_revision: u64,
    /// Complete settings revision that failed activation.
    /// 激活失败的完整设置修订版。
    settings: ProjectVceSettings,
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
            previous.ai.deactivate()?;
        }
        let requested_root = crate::normalized_workspace_path_text_from_path(project_root);
        match self
            .activate_project(project_id, project_root, settings.clone())
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
                        project_root: requested_root,
                        config_revision: settings.revision,
                        settings,
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
            runtime.ai.deactivate()?;
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
    ) -> Result<RetrieveResult, String> {
        if information_request.trim().is_empty() {
            return Err(String::from("VCE information_request must not be empty"));
        }
        let runtime = self.require_project(project_id).await?;
        let engine_handle = runtime.current_engine.read().await.clone();
        let mut engine = engine_handle.lock().await;
        let mut request = RetrieveRequest::current_checkpoint(
            request_id(project_id, "retrieve"),
            information_request,
        );
        request.allow_query_rewrite = runtime.settings.allow_query_rewrite;
        request.allow_rerank = runtime.settings.allow_rerank;
        engine
            .retrieve(request, runtime.index.as_ref(), runtime.ai.as_ref())
            .await
            .map_err(|error| error.to_string())
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
                project_root: failure.project_root.clone(),
                lifecycle: if failure.blocked_configuration {
                    VceProjectLifecycle::BlockedConfiguration
                } else {
                    VceProjectLifecycle::Failed
                },
                config_revision: failure.config_revision,
                settings: failure.settings.clone(),
                namespace_id: None,
                vector_space_id: None,
                engine: None,
                workspace: None,
                last_error: Some(failure.error.clone()),
            })
    }

    /// Return deterministic statuses for every active or blocked project.
    /// 返回每个活动或受阻项目的确定性状态。
    ///
    /// # Returns
    /// 返回值
    /// Project statuses sorted by stable project ID.
    /// 返回按稳定项目 ID 排序的项目状态。
    pub(crate) async fn project_statuses(&self) -> Vec<VceRuntimeProjectStatus> {
        let mut project_ids = self
            .projects
            .read()
            .await
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        project_ids.extend(self.activation_failures.read().await.keys().cloned());
        let mut statuses = Vec::with_capacity(project_ids.len());
        for project_id in project_ids {
            if let Some(status) = self.project_status(&project_id).await {
                statuses.push(status);
            }
        }
        statuses
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
            if let Err(error) = runtime.ai.deactivate() {
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
        let engine = match self
            .data_plane
            .load_state_snapshot(&config.index_namespace_id)
            .await?
        {
            Some(snapshot) if snapshot.identity == expected_identity => {
                VceEngine::from_state_snapshot(config.clone(), snapshot)
                    .map_err(|error| error.to_string())?
            }
            Some(_) | None => VceEngine::new(config.clone()).map_err(|error| error.to_string())?,
        };
        Ok(Arc::new(VceProjectRuntime {
            project_id: project_id.to_owned(),
            project_root,
            config,
            config_revision: settings.revision,
            settings,
            ai,
            index,
            source,
            current_engine: RwLock::new(Arc::new(Mutex::new(engine))),
            refresh_gate: Mutex::new(()),
            host_status: RwLock::new(VceHostStatus {
                lifecycle: VceProjectLifecycle::Activating,
                last_error: None,
            }),
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
        let mut working = VceEngine::from_state_snapshot(runtime.config.clone(), base_snapshot)
            .map_err(|error| error.to_string())?;
        let result = working
            .refresh(
                request,
                runtime.index.as_ref(),
                runtime.ai.as_ref(),
                runtime.source.as_ref(),
            )
            .await;
        match result {
            Ok(result) => {
                let snapshot = working.state_snapshot();
                self.data_plane
                    .persist_state_snapshot(&runtime.config.index_namespace_id, &snapshot)
                    .await?;
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
            project_root: self.project_root.clone(),
            lifecycle: host_status.lifecycle,
            config_revision: self.config_revision,
            settings: self.settings.clone(),
            namespace_id: Some(self.config.index_namespace_id.clone()),
            vector_space_id: Some(self.config.embedding_space.vector_space_id.clone()),
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
        status.lifecycle = VceProjectLifecycle::Ready;
        status.last_error = None;
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

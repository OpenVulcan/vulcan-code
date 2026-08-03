use super::*;

/// Summarizes one VMM memory bundle or node group.
/// 汇总单个 VMM 记忆集合或节点组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    /// Stable memory identifier.
    /// 稳定记忆标识。
    pub id: String,
    /// Memory display title.
    /// 记忆显示标题。
    pub title: String,
    /// Memory category.
    /// 记忆分类。
    pub category: String,
    /// Memory state summary.
    /// 记忆状态摘要。
    pub state: String,
}

/// Describes how Vulcan Code reaches the VMM runtime.
/// 描述 Vulcan Code 连接 VMM 运行时的方式。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmDeploymentMode {
    /// Long-term memory is not configured.
    /// 长期记忆尚未配置。
    #[default]
    Disabled,
    /// Vulcan Code owns the local VMM process.
    /// Vulcan Code 托管本地 VMM 进程。
    Managed,
    /// Vulcan Code connects to an external VMM service.
    /// Vulcan Code 连接外部 VMM 服务。
    External,
}

/// Describes whether the user-facing VMM setup is complete.
/// 描述面向用户的 VMM 设置是否完整。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmSetupState {
    /// No usable VMM configuration exists.
    /// 尚不存在可用的 VMM 配置。
    #[default]
    NotConfigured,
    /// Required setup conditions are being evaluated.
    /// 正在检查必需的设置条件。
    Checking,
    /// All required setup conditions are satisfied.
    /// 所有必需设置条件均已满足。
    Ready,
    /// One or more required settings are missing.
    /// 缺少一项或多项必需设置。
    Incomplete,
    /// Existing data or another safety condition blocks startup.
    /// 历史数据或其他安全条件阻止启动。
    Blocked,
}

/// Describes the VMM process lifecycle independently from setup and health.
/// 独立于配置与健康状态描述 VMM 进程生命周期。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmLifecycleState {
    /// No VMM process is running.
    /// 当前没有 VMM 进程运行。
    #[default]
    Stopped,
    /// A VMM process is starting.
    /// VMM 进程正在启动。
    Starting,
    /// A VMM process is running.
    /// VMM 进程正在运行。
    Running,
    /// A VMM process is stopping.
    /// VMM 进程正在停止。
    Stopping,
    /// A VMM process is restarting.
    /// VMM 进程正在重启。
    Restarting,
    /// A managed VMM process exited unexpectedly.
    /// 托管 VMM 进程意外退出。
    Crashed,
    /// The host is waiting before a managed restart.
    /// 宿主正在等待下一次托管重启。
    WaitingRetry,
    /// The most recent VMM start attempt failed.
    /// 最近一次 VMM 启动尝试失败。
    Failed,
}

/// Describes the currently available VMM experience.
/// 描述当前可用的 VMM 使用体验。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmHealthState {
    /// Health has not been established.
    /// 尚未确定健康状态。
    #[default]
    Unknown,
    /// All enabled capabilities are healthy.
    /// 所有已启用能力均正常。
    Healthy,
    /// Core memory works but one or more optional capabilities are unavailable.
    /// 核心记忆可用，但一项或多项可选能力不可用。
    Degraded,
    /// Core memory is unavailable.
    /// 核心记忆不可用。
    Unavailable,
}

/// Combines the independent product-facing VMM state dimensions.
/// 汇总彼此独立的面向产品的 VMM 状态维度。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmExperienceStateSummary {
    /// Configured deployment mode.
    /// 已配置的部署方式。
    pub deployment: VmmDeploymentMode,
    /// Setup readiness state.
    /// 设置就绪状态。
    pub setup: VmmSetupState,
    /// Runtime lifecycle state.
    /// 运行时生命周期状态。
    pub lifecycle: VmmLifecycleState,
    /// User-visible health state.
    /// 用户可见健康状态。
    pub health: VmmHealthState,
}

/// Describes one setup or runtime prerequisite.
/// 描述单个设置或运行前置条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmPrerequisiteSummary {
    /// Stable prerequisite identifier.
    /// 稳定前置条件标识。
    pub id: String,
    /// Stable prerequisite state.
    /// 稳定前置条件状态。
    pub state: String,
    /// Stable severity key.
    /// 稳定严重程度键。
    pub severity: String,
    /// Translation key for the short title.
    /// 简短标题的翻译键。
    pub title_key: String,
    /// Translation key for the actionable description.
    /// 可操作说明的翻译键。
    pub description_key: String,
    /// Controlled technical detail shown only in diagnostics.
    /// 仅在诊断中显示的受控技术详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical_detail: Option<String>,
    /// Stable action kind for the primary recovery action.
    /// 主要恢复操作的稳定动作类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_kind: Option<String>,
    /// Optional route or section targeted by the recovery action.
    /// 恢复操作指向的可选路由或分区。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_target: Option<String>,
}

/// Describes one independently available VMM capability.
/// 描述单项独立可用的 VMM 能力。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmCapabilitySummary {
    /// Stable capability identifier.
    /// 稳定能力标识。
    pub id: String,
    /// Whether the capability can currently be used.
    /// 当前是否可以使用该能力。
    pub available: bool,
    /// Whether the user configuration enables the capability.
    /// 用户配置是否启用该能力。
    pub enabled: bool,
    /// Stable capability state.
    /// 稳定能力状态。
    pub state: String,
    /// Optional user-facing reason key.
    /// 可选的用户可见原因键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Optional destination used to configure the capability.
    /// 用于配置该能力的可选目标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configure_target: Option<String>,
}

/// Describes the managed VMM data-space safety state.
/// 描述托管 VMM 数据空间的安全状态。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmDataSpaceState {
    /// Data-space state is not applicable or not yet known.
    /// 数据空间状态不适用或尚未确定。
    #[default]
    Unknown,
    /// The managed data directory contains no durable data.
    /// 托管数据目录不包含长期数据。
    Empty,
    /// The managed data directory has a valid identity marker.
    /// 托管数据目录具有有效身份标记。
    Initialized,
    /// Historical data exists without a trusted vector identity.
    /// 存在没有可信向量身份的历史数据。
    UnmarkedHistory,
    /// The configured vector space differs from the stored identity.
    /// 配置的向量空间与已存储身份不同。
    FingerprintChanged,
    /// The vector identity marker is malformed.
    /// 向量身份标记格式损坏。
    MarkerCorrupt,
    /// The vector identity marker uses an unsupported version.
    /// 向量身份标记使用了不受支持的版本。
    MarkerVersionUnsupported,
    /// A formal data schema migration is required.
    /// 需要执行正式数据架构迁移。
    SchemaMigrationRequired,
    /// A vector rebuild is currently running.
    /// 当前正在重建向量。
    RebuildRunning,
    /// The data space is ready for the configured vector contract.
    /// 数据空间已适配当前向量契约。
    Ready,
}

/// Summarizes data-space state without exposing internal marker files.
/// 在不暴露内部标记文件的前提下汇总数据空间状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmDataSpaceSummary {
    /// Structured data-space state.
    /// 结构化数据空间状态。
    pub state: VmmDataSpaceState,
    /// Translation key for the state title.
    /// 状态标题翻译键。
    pub title_key: String,
    /// Translation key for the state description.
    /// 状态说明翻译键。
    pub description_key: String,
    /// Optional primary maintenance action.
    /// 可选的主要维护操作。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_kind: Option<String>,
}

/// Stable phase of one host-owned VMM data maintenance operation.
/// 一次宿主持有 VMM 数据维护操作的稳定阶段。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmMaintenancePhase {
    /// No maintenance operation has been started.
    /// 尚未启动维护操作。
    #[default]
    Idle,
    /// Historical data is being copied into a separate backup.
    /// 正在把历史数据复制到独立备份。
    BackingUp,
    /// The official VMM tool is rebuilding the vector index.
    /// VMM 正式工具正在重建向量索引。
    Rebuilding,
    /// A cancellation request is being delivered.
    /// 正在传递取消请求。
    Cancelling,
    /// Backup and rebuild completed successfully.
    /// 备份与重建已经成功完成。
    Completed,
    /// Maintenance stopped after a controlled cancellation.
    /// 维护已在受控取消后停止。
    Cancelled,
    /// Maintenance failed and retained its completed backup when available.
    /// 维护失败，并在可用时保留已完成备份。
    Failed,
}

/// Product-facing status of one host-owned VMM maintenance operation.
/// 一次宿主持有 VMM 维护操作的产品状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMaintenanceSummary {
    /// Stable maintenance phase.
    /// 稳定维护阶段。
    pub phase: VmmMaintenancePhase,
    /// Approximate completed percentage across backup and rebuild stages.
    /// 备份与重建阶段的近似完成百分比。
    pub progress_percent: u8,
    /// User-facing current activity or terminal outcome.
    /// 面向用户的当前活动或终态结果。
    pub message: String,
    /// Whether the current operation can still be cancelled safely.
    /// 当前操作是否仍可安全取消。
    pub can_cancel: bool,
    /// Whether a complete backup was created before rebuild began.
    /// 重建开始前是否已创建完整备份。
    pub backup_created: bool,
    /// Optional current rebuild sub-stage reported by the official maintenance process.
    /// 正式维护进程报告的可选当前重建子阶段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_stage: Option<String>,
    /// Optional number of records completed in the current rebuild sub-stage.
    /// 当前重建子阶段已完成的可选记录数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processed_items: Option<u64>,
    /// Optional total number of records in the current rebuild sub-stage.
    /// 当前重建子阶段的可选记录总数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_items: Option<u64>,
    /// Optional maintenance start time in Unix milliseconds.
    /// 可选维护开始时间，单位为 Unix 毫秒。
    pub started_at_unix_ms: Option<u64>,
    /// Optional latest update time in Unix milliseconds.
    /// 可选最近更新时间，单位为 Unix 毫秒。
    pub updated_at_unix_ms: Option<u64>,
    /// Controlled technical detail shown only in advanced diagnostics.
    /// 仅在高级诊断中展示的受控技术详情。
    pub technical_detail: Option<String>,
}

/// User-facing summary of one durable VMM user.
/// 单个长期 VMM 用户的用户可见摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmUserDisplaySummary {
    /// Stable internal key used for follow-up requests.
    /// 后续请求使用的稳定内部键。
    pub stable_key: String,
    /// Human-readable user name.
    /// 人类可读的用户名称。
    pub display_name: String,
    /// Number of associated durable projects.
    /// 关联的长期项目数量。
    pub linked_project_count: Option<usize>,
    /// Compact memory state key.
    /// 紧凑记忆状态键。
    pub memory_status: String,
    /// Whether this user is protected from deletion.
    /// 该用户是否受删除保护。
    pub protected: bool,
}

/// User-facing summary of one durable VMM project.
/// 单个长期 VMM 项目的用户可见摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmProjectDisplaySummary {
    /// Stable internal key used for follow-up requests.
    /// 后续请求使用的稳定内部键。
    pub stable_key: String,
    /// Human-readable project name.
    /// 人类可读的项目名称。
    pub display_name: String,
    /// Human-readable canonical project path.
    /// 人类可读的标准项目路径。
    pub display_path: String,
    /// Opaque VMM project reference used only for management requests.
    /// 仅供管理请求使用的不透明 VMM 项目引用。
    pub action_path: String,
    /// Optional Vulcan Code project key bound to this VMM project.
    /// 与该 VMM 项目绑定的可选 Vulcan Code 项目键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_project_key: Option<String>,
    /// Whether this project has an explicit memory-rule override.
    /// 该项目是否具有独立记忆规则覆盖。
    pub has_override: Option<bool>,
    /// Compact project state key.
    /// 紧凑项目状态键。
    pub state: String,
    /// Whether this project is protected from deletion.
    /// 该项目是否受删除保护。
    pub protected: bool,
}

/// Cached user and project catalog used by product-facing memory pages.
/// 面向产品的记忆页面使用的用户与项目缓存目录。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmManagementCatalogSummary {
    /// Whether the catalog is currently loading.
    /// 目录当前是否正在加载。
    pub loading: bool,
    /// Durable user summaries.
    /// 长期用户摘要。
    pub users: Vec<VmmUserDisplaySummary>,
    /// Durable project summaries.
    /// 长期项目摘要。
    pub projects: Vec<VmmProjectDisplaySummary>,
    /// Last successful catalog refresh timestamp.
    /// 最近一次目录刷新成功时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refreshed_at: Option<String>,
    /// Controlled catalog refresh error.
    /// 受控的目录刷新错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Summarizes recent VMM activity without exposing internal identifiers.
/// 在不暴露内部标识的前提下汇总近期 VMM 活动。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmRecentActivitySummary {
    /// Number of pending writeback items.
    /// 待写回项目数量。
    pub pending_writebacks: usize,
    /// Number of completed turns observed by profile synchronization.
    /// 画像同步已观察的完成回合数量。
    pub completed_turns: u32,
    /// Latest durable profile update timestamp.
    /// 最近一次长期画像更新时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_profile_update_at: Option<String>,
}

/// User-facing names for the globally configured default VMM identity.
/// 全局配置的默认 VMM 身份对应的用户可见名称。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmDisplayIdentitySummary {
    /// Default VMM project display name resolved from the management catalog.
    /// 从管理目录解析得到的默认 VMM 项目显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// Canonical Team/Space/Project path of the default VMM project.
    /// 默认 VMM 项目的规范 Team/Space/Project 路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Default VMM user display name resolved from the management catalog.
    /// 从管理目录解析得到的默认 VMM 用户显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
}

/// Full VMM memory page snapshot used by the WebUI.
/// WebUI 使用的完整 VMM 记忆页面快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemorySnapshot {
    /// Product-facing VMM state dimensions.
    /// 面向产品的 VMM 状态维度。
    pub experience_state: VmmExperienceStateSummary,
    /// Structured setup and runtime prerequisites.
    /// 结构化设置与运行前置条件。
    pub prerequisites: Vec<VmmPrerequisiteSummary>,
    /// Independently projected memory capabilities.
    /// 独立投影的记忆能力。
    pub capabilities: Vec<VmmCapabilitySummary>,
    /// Managed data-space safety state.
    /// 托管数据空间安全状态。
    pub data_space_state: VmmDataSpaceSummary,
    /// Host-owned backup and rebuild status.
    /// 宿主持有的备份与重建状态。
    pub maintenance: VmmMaintenanceSummary,
    /// Cached VMM user and project directory.
    /// 缓存的 VMM 用户与项目目录。
    pub management_catalog: VmmManagementCatalogSummary,
    /// Recent memory activity.
    /// 近期记忆活动。
    pub recent_activity: VmmRecentActivitySummary,
    /// User-facing globally configured default VMM identity.
    /// 用户可见的全局默认 VMM 身份。
    pub display_identity: VmmDisplayIdentitySummary,
    /// Effective VMM configuration for the selected session.
    /// 当前选中会话的最终 VMM 配置。
    pub effective: VmmEffectiveSummary,
    /// Global VMM editable draft.
    /// 全局 VMM 可编辑草稿。
    pub global_draft: VmmConfigDraftSummary,
    /// Project VMM editable draft for the selected project.
    /// 当前选中项目的 VMM 可编辑草稿。
    pub project_draft: VmmConfigDraftSummary,
    /// Available project scopes for project-level VMM settings.
    /// 可用于项目级 VMM 设置的项目作用域。
    pub project_scopes: Vec<VmmProjectScopeSummary>,
    /// Current selected project scope id when available.
    /// 当前可用的选中项目作用域标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_project_id: Option<String>,
    /// Current selected conversation id when available.
    /// 当前可用的选中会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_conversation_id: Option<String>,
    /// Runtime side-effect summary for VMM hooks.
    /// VMM hook 运行时副作用摘要。
    pub runtime: VmmRuntimeSummary,
    /// Management sections available in the VMM page.
    /// VMM 页面可用的管理分区。
    pub sections: Vec<VmmManagementSectionSummary>,
    /// Human-readable notice for the current VMM page.
    /// 当前 VMM 页面的可读提示。
    pub notice: String,
}

/// Effective VMM configuration summary.
/// VMM 最终配置摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmEffectiveSummary {
    /// Whether memory flow is enabled by configuration.
    /// 配置是否启用记忆链路。
    pub enabled: bool,
    /// Configured gRPC target.
    /// 已配置的 gRPC 目标。
    pub grpc_target: String,
    /// Whether gRPC target is present.
    /// 是否存在 gRPC 目标。
    pub grpc_configured: bool,
    /// Effective user id.
    /// 最终用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<u64>,
    /// Effective project id.
    /// 最终项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<u64>,
    /// Effective memory flow diagnostic key.
    /// 最终记忆链路诊断键。
    pub diagnostic: String,
    /// Effective automatic precheck diagnostic key.
    /// 最终自动前置召回诊断键。
    pub precheck_diagnostic: String,
    /// Effective completed-turn memory writeback diagnostic key.
    /// 最终已完成回合记忆写回诊断键。
    pub writeback_diagnostic: String,
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub ai_tools_enabled: bool,
    /// Whether automatic precheck recall is enabled.
    /// 是否启用自动前置召回。
    pub precheck_enabled: bool,
    /// Whether completed-turn memory writeback is enabled.
    /// 是否启用已完成回合记忆写回。
    pub memory_writeback_enabled: bool,
    /// Transient recall TTL in turns.
    /// 临时召回持续回合数。
    pub implicit_memory_turns: u32,
    /// Profile refresh interval in turns.
    /// 画像刷新间隔回合数。
    pub profile_refresh_turns: u32,
    /// Per-target profile node limit.
    /// 每目标画像节点上限。
    pub profile_node_limit: u32,
    /// Context injection tag.
    /// 上下文注入标签。
    pub injection_tag: String,
    /// Context injection description.
    /// 上下文注入说明。
    pub injection_description: String,
}

/// Editable host-authoritative managed VMM behavior draft.
/// 可编辑的宿主权威托管 VMM 行为草稿。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmManagedRuntimeSettingsDraft {
    /// PII rule language.
    /// PII 规则语言。
    pub pii_default_language: String,
    /// Whether noise filtering is enabled.
    /// 是否启用噪声过滤。
    pub noise_enabled: bool,
    /// Noise-rule language.
    /// 噪声规则语言。
    pub noise_default_language: String,
    /// Whether semantic noise filtering is enabled.
    /// 是否启用语义噪声过滤。
    pub noise_semantic_enabled: bool,
    /// Semantic noise threshold text.
    /// 语义噪声阈值文本。
    pub noise_semantic_threshold: String,
    /// Prompt bundle language.
    /// 提示词包语言。
    pub prompt_language: String,
    /// PreCheck intent timeout.
    /// PreCheck 意图解析超时。
    pub precheck_intent_timeout: String,
    /// PreCheck recall count text.
    /// PreCheck 召回数量文本。
    pub precheck_top_k: String,
    /// PreCheck search scope.
    /// PreCheck 搜索范围。
    pub precheck_search_scope: String,
    /// PreCheck similarity threshold text.
    /// PreCheck 相似度阈值文本。
    pub precheck_similarity_threshold: String,
    /// PostAction input mode.
    /// PostAction 输入模式。
    pub postaction_input_mode: String,
    /// PostAction turn threshold text.
    /// PostAction 轮次阈值文本。
    pub postaction_turn_threshold: String,
    /// PostAction token threshold text.
    /// PostAction Token 阈值文本。
    pub postaction_token_threshold: String,
    /// PostAction idle timeout.
    /// PostAction 空闲超时。
    pub postaction_idle_timeout: String,
    /// PostAction history turn count text.
    /// PostAction 历史轮数文本。
    pub postaction_history_turns: String,
    /// PostAction maximum input tokens text.
    /// PostAction 最大输入 Token 文本。
    pub postaction_max_input_tokens: String,
    /// PostAction queue worker count text.
    /// PostAction 队列工作数文本。
    pub postaction_max_queue_workers: String,
    /// Maximum recall keyword count text.
    /// 最大召回关键词数量文本。
    pub pipeline_max_search_keywords: String,
    /// Minimum recall similarity text.
    /// 最小召回相似度文本。
    pub pipeline_min_similarity_score: String,
    /// Minimum replacement similarity text.
    /// 最小替换相似度文本。
    pub pipeline_replace_min_similarity_score: String,
    /// Hard deduplication threshold text.
    /// 硬去重阈值文本。
    pub pipeline_hard_dedupe_threshold: String,
    /// Hard deduplication pool size text.
    /// 硬去重候选数量文本。
    pub pipeline_hard_dedupe_pool_top_k: String,
    /// Whether hybrid retrieval is enabled.
    /// 是否启用混合检索。
    pub pipeline_hybrid_enabled: bool,
    /// Lexical recall count text.
    /// 词法召回数量文本。
    pub pipeline_lexical_top_k: String,
    /// Reciprocal-rank-fusion constant text.
    /// 倒数排名融合常量文本。
    pub pipeline_rrf_k: String,
    /// Whether diversity filtering is enabled.
    /// 是否启用多样性过滤。
    pub pipeline_mmr_enabled: bool,
    /// Diversity balance text.
    /// 多样性平衡系数文本。
    pub pipeline_mmr_lambda: String,
    /// Whether Weibull decay is enabled.
    /// 是否启用 Weibull 衰减。
    pub pipeline_weibull_enabled: bool,
    /// Weibull shape text.
    /// Weibull 形状参数文本。
    pub pipeline_weibull_shape: String,
    /// Weibull scale text.
    /// Weibull 尺度文本。
    pub pipeline_weibull_scale_hours: String,
    /// Minimum decay multiplier text.
    /// 最小衰减倍率文本。
    pub pipeline_weibull_min_multiplier: String,
    /// Reinforcement weight text.
    /// 强化权重文本。
    pub pipeline_weibull_reinforce_weight: String,
    /// Cross-session boost text.
    /// 跨会话增强系数文本。
    pub pipeline_weibull_cross_session_boost: String,
    /// Whether retention scanning is enabled.
    /// 是否启用保留扫描。
    pub retention_enabled: bool,
    /// Retention scan interval.
    /// 保留扫描间隔。
    pub retention_scan_interval: String,
    /// Extra hot-window turns text.
    /// 热窗口额外轮数文本。
    pub retention_turn_keep_extra_turns: String,
    /// Session idle recycle duration.
    /// 会话空闲回收时长。
    pub retention_session_idle_after: String,
    /// Trash retention duration.
    /// 回收站保留时长。
    pub retention_trash_duration: String,
    /// Protected priority floor.
    /// 受保护优先级下限。
    pub retention_priority_floor: String,
    /// Protected memory-level floor.
    /// 受保护记忆级别下限。
    pub retention_memory_level_floor: String,
    /// Whether protected shared memories are skipped.
    /// 是否跳过受保护共享记忆。
    pub retention_skip_protected_shared: bool,
}

impl Default for VmmManagedRuntimeSettingsDraft {
    /// Returns the editable projection of VMM's audited managed-runtime defaults.
    /// 返回 VMM 已审计托管运行时默认值的可编辑投影。
    fn default() -> Self {
        Self {
            pii_default_language: String::from("zh-CN"),
            noise_enabled: true,
            noise_default_language: String::from("zh-CN"),
            noise_semantic_enabled: true,
            noise_semantic_threshold: String::from("0.88"),
            prompt_language: String::from("default_en"),
            precheck_intent_timeout: String::from("5s"),
            precheck_top_k: String::from("5"),
            precheck_search_scope: String::from("space"),
            precheck_similarity_threshold: String::from("0"),
            postaction_input_mode: String::from("compat"),
            postaction_turn_threshold: String::from("2"),
            postaction_token_threshold: String::from("12000"),
            postaction_idle_timeout: String::from("15m"),
            postaction_history_turns: String::from("3"),
            postaction_max_input_tokens: String::from("6000"),
            postaction_max_queue_workers: String::from("4"),
            pipeline_max_search_keywords: String::from("5"),
            pipeline_min_similarity_score: String::from("0.75"),
            pipeline_replace_min_similarity_score: String::from("0.8"),
            pipeline_hard_dedupe_threshold: String::from("0.99"),
            pipeline_hard_dedupe_pool_top_k: String::from("16"),
            pipeline_hybrid_enabled: true,
            pipeline_lexical_top_k: String::from("8"),
            pipeline_rrf_k: String::from("60"),
            pipeline_mmr_enabled: true,
            pipeline_mmr_lambda: String::from("0.75"),
            pipeline_weibull_enabled: true,
            pipeline_weibull_shape: String::from("1.35"),
            pipeline_weibull_scale_hours: String::from("2160"),
            pipeline_weibull_min_multiplier: String::from("0.4"),
            pipeline_weibull_reinforce_weight: String::from("0.18"),
            pipeline_weibull_cross_session_boost: String::from("0.12"),
            retention_enabled: true,
            retention_scan_interval: String::from("30m"),
            retention_turn_keep_extra_turns: String::from("5"),
            retention_session_idle_after: String::from("360h"),
            retention_trash_duration: String::from("720h"),
            retention_priority_floor: String::from("P1"),
            retention_memory_level_floor: String::from("stable"),
            retention_skip_protected_shared: true,
        }
    }
}

/// Editable VMM configuration draft summary.
/// 可编辑 VMM 配置草稿摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmConfigDraftSummary {
    /// Whether this layer has explicit overrides.
    /// 当前层是否存在显式覆盖。
    pub has_override: bool,
    /// Whether memory is enabled on this layer.
    /// 当前层是否启用记忆。
    pub enabled: bool,
    /// Editable gRPC target.
    /// 可编辑 gRPC 目标。
    pub grpc_target: String,
    /// Editable gRPC API key.
    /// 可编辑 gRPC API key。
    pub grpc_api_key: String,
    /// Editable external management API base URL.
    /// 可编辑外部管理 API 基础地址。
    pub management_base_url: String,
    /// Editable external management bearer token or redacted placeholder.
    /// 可编辑外部管理 Bearer Token 或脱敏占位符。
    pub management_bearer_token: String,
    /// Editable external management timeout text.
    /// 可编辑外部管理超时文本。
    pub management_timeout_ms: String,
    /// Whether Vulcan Code owns a local VMM child process.
    /// Vulcan Code 是否持有本地 VMM 子进程。
    pub managed_install_enabled: bool,
    /// Optional explicit managed VMM executable path.
    /// 可选的显式托管 VMM 可执行文件路径。
    pub managed_binary_path: String,
    /// Managed child auto-start policy.
    /// 托管子进程自动启动策略。
    pub auto_start_policy: String,
    /// Host-authoritative managed VMM behavior.
    /// 宿主权威托管 VMM 行为配置。
    pub managed_runtime_settings: VmmManagedRuntimeSettingsDraft,
    /// Editable user id text.
    /// 可编辑用户标识文本。
    pub user_id: String,
    /// Editable project id text.
    /// 可编辑项目标识文本。
    pub project_id: String,
    /// Editable receive timeout text.
    /// 可编辑请求超时文本。
    pub receive_timeout_ms: String,
    /// Editable handshake timeout text.
    /// 可编辑握手超时文本。
    pub handshake_timeout_ms: String,
    /// Editable max receive bytes text.
    /// 可编辑最大接收字节数文本。
    pub max_receive_message_bytes: String,
    /// Editable gRPC keepalive interval text.
    /// 可编辑 gRPC keepalive 间隔文本。
    pub grpc_keepalive_time_ms: String,
    /// Editable gRPC keepalive timeout text.
    /// 可编辑 gRPC keepalive 超时文本。
    pub grpc_keepalive_timeout_ms: String,
    /// Whether idle keepalive is enabled.
    /// 是否启用空闲 keepalive。
    pub grpc_keepalive_permit_without_calls: bool,
    /// Editable transient recall TTL text.
    /// 可编辑临时召回 TTL 文本。
    pub implicit_memory_turns: String,
    /// Editable profile refresh interval text.
    /// 可编辑画像刷新间隔文本。
    pub profile_refresh_turns: String,
    /// Editable per-target profile node limit text.
    /// 可编辑每目标画像节点上限文本。
    pub profile_node_limit: String,
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub ai_tools_enabled: bool,
    /// Whether automatic precheck recall is enabled.
    /// 是否启用自动前置召回。
    pub precheck_enabled: bool,
    /// Precheck policy mode for this layer.
    /// 当前层的前置召回策略模式。
    pub precheck_mode: String,
    /// Whether completed-turn memory writeback is enabled.
    /// 是否启用已完成回合记忆写回。
    pub memory_writeback_enabled: bool,
    /// Completed-turn writeback policy mode for this layer.
    /// 当前层的已完成回合写回策略模式。
    pub memory_writeback_mode: String,
    /// Editable injection tag.
    /// 可编辑注入标签。
    pub injection_tag: String,
    /// Editable injection description.
    /// 可编辑注入说明。
    pub injection_description: String,
}

/// Project scope row available to VMM project settings.
/// VMM 项目设置可用的项目作用域行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmProjectScopeSummary {
    /// Stable task project id.
    /// 稳定任务项目标识。
    pub project_id: String,
    /// Project display name.
    /// 项目显示名称。
    pub name: String,
    /// Absolute workspace path.
    /// 绝对工作区路径。
    pub path: String,
    /// Whether this project has explicit VMM overrides.
    /// 该项目是否存在显式 VMM 覆盖。
    pub has_override: bool,
}

/// Runtime state summary for VMM hooks.
/// VMM hook 运行态摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmRuntimeSummary {
    /// Structured managed data-space safety state.
    /// 结构化托管数据空间安全状态。
    pub data_space_state: VmmDataSpaceState,
    /// Whether the active VMM process or externally probed endpoint is ready.
    /// 当前 VMM 进程或已显式探测的外部端点是否已就绪。
    pub process_ready: bool,
    /// Host-managed child lifecycle phase.
    /// 宿主管理子进程生命周期阶段。
    pub managed_phase: String,
    /// Actual managed gRPC target without authorization data.
    /// 不含鉴权数据的实际托管 gRPC 目标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_grpc_target: Option<String>,
    /// Shared Controller endpoint used by the managed child.
    /// 托管子进程使用的共享 Controller 端点。
    pub managed_controller_endpoint: String,
    /// Isolated Controller space used by VMM.
    /// VMM 使用的隔离 Controller Space。
    pub managed_controller_space: String,
    /// Stable managed VMM data root.
    /// 稳定托管 VMM 数据根。
    pub managed_data_root: String,
    /// Stable managed VMM log path.
    /// 稳定托管 VMM 日志路径。
    pub managed_log_path: String,
    /// Latest controlled managed-runtime error.
    /// 最近一次受控托管运行时错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_last_error: Option<String>,
    /// Whether the configured numeric user/project scope was reconciled.
    /// 配置的数字用户/项目作用域是否已完成对账。
    pub scope_ready: bool,
    /// Controlled scope reconciliation diagnostic.
    /// 受控的作用域对账诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_error: Option<String>,
    /// Whether PreCheck inference is available.
    /// PreCheck 推理是否可用。
    pub precheck_ready: bool,
    /// Controlled PreCheck degradation reason.
    /// 受控的 PreCheck 降级原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precheck_error: Option<String>,
    /// Whether PostAction inference is available.
    /// PostAction 推理是否可用。
    pub postaction_ready: bool,
    /// Controlled PostAction degradation reason.
    /// 受控的 PostAction 降级原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postaction_error: Option<String>,
    /// Whether manual profile instruction inference is available.
    /// 手工画像指令推理是否可用。
    pub profile_instruction_ready: bool,
    /// Controlled profile-instruction degradation reason.
    /// 受控的画像指令降级原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_instruction_error: Option<String>,
    /// Whether rerank inference is available.
    /// 重排推理是否可用。
    pub rerank_ready: bool,
    /// Controlled rerank degradation reason.
    /// 受控的重排降级原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_error: Option<String>,
    /// Pending VMM writeback outbox count.
    /// 待处理 VMM 写回 outbox 数量。
    pub outbox_count: usize,
    /// Profile sync node count.
    /// 画像同步节点数量。
    pub profile_sync_count: usize,
    /// Completed turn count recorded by profile sync.
    /// 画像同步记录的已完成回合数。
    pub completed_turns: u32,
    /// Latest profile update timestamp.
    /// 最近画像更新时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Profile base content preview.
    /// 画像基点内容预览。
    pub profile_preview: String,
}

/// VMM management section descriptor.
/// VMM 管理分区描述。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmManagementSectionSummary {
    /// Stable section id.
    /// 稳定分区标识。
    pub id: String,
    /// OpenCode-style section key.
    /// OpenCode 风格分区键。
    pub key: String,
    /// User-facing section label.
    /// 面向用户的分区标签。
    pub label: String,
    /// Compact section description.
    /// 紧凑分区说明。
    pub description: String,
}

/// Request body for one WebUI VMM management action.
/// WebUI 单次 VMM 管理动作的请求体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VmmMemoryActionRequest {
    /// Stable action id.
    /// 稳定动作标识。
    pub action: String,
    /// Stable UUID identifying this exact user operation across transport retries.
    /// 跨传输重试标识本次精确用户操作的稳定 UUID。
    pub command_id: String,
    /// Scope key, either global or project.
    /// 作用域键，可为 global 或 project。
    pub scope: String,
    /// Optional task project id used by project scope.
    /// 项目作用域使用的可选任务项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Exact persisted conversation id required by session-scoped actions.
    /// 会话级动作必须提供的准确已持久化会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Action-specific JSON payload.
    /// 动作专用 JSON 载荷。
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Typed payload returned by one VMM management action.
/// VMM 单次管理动作返回的强类型载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VmmMemoryActionResponse {
    /// No typed payload is available for one diagnostic-only action.
    /// 诊断专用动作没有可用的强类型载荷。
    Empty,
    /// Managed-runtime control details are diagnostic-only.
    /// 托管运行时控制详情仅用于诊断。
    ManagedRuntime(serde_json::Value),
    /// Context-injection suppression details are diagnostic-only.
    /// 上下文注入抑制详情仅用于诊断。
    ContextInjection(serde_json::Value),
    /// VMM health status.
    /// VMM 健康状态。
    Healthz {
        /// Human-readable service status.
        /// 人类可读服务状态。
        status: String,
    },
    /// Exact durable user and project lists.
    /// 准确的长期用户与项目列表。
    UsersProjects {
        /// Durable VMM users.
        /// 长期 VMM 用户。
        users: Vec<vulcan_vmm::UserEntry>,
        /// Durable VMM projects.
        /// 长期 VMM 项目。
        projects: Vec<vulcan_vmm::ProjectEntry>,
    },
    /// Current user and project resolution result.
    /// 当前用户与项目解析结果。
    Resolved {
        /// User resolution response.
        /// 用户解析响应。
        user: vulcan_vmm::ResolveUserResponse,
        /// Project resolution response.
        /// 项目解析响应。
        project: vulcan_vmm::ResolveProjectResponse,
    },
    /// Project create or resolve result.
    /// 项目创建或解析结果。
    EnsureProject(vulcan_vmm::EnsureProjectResponse),
    /// Guarded user deletion result.
    /// 受保护用户删除结果。
    DeleteUser(vulcan_vmm::DeleteUserResponse),
    /// Guarded project deletion result.
    /// 受保护项目删除结果。
    DeleteProject(vulcan_vmm::DeleteProjectResponse),
    /// Guarded project migration result.
    /// 受保护项目迁移结果。
    MigrateProject(vulcan_vmm::MigrateProjectResponse),
    /// Profile node list.
    /// 画像节点列表。
    ProfileNodes(vulcan_vmm::GetProfileNodesResponse),
    /// Final profile bundle.
    /// 最终画像组合。
    ProfileBundle(vulcan_vmm::GetProfileBundleResponse),
    /// Applied profile instruction result.
    /// 已应用画像指令结果。
    ProfileInstruction(vulcan_vmm::ApplyProfileInstructionResponse),
    /// User resolve or create result.
    /// 用户解析或创建结果。
    ResolveUser(vulcan_vmm::ResolveUserResponse),
    /// Natural-language memory search result.
    /// 自然语言记忆检索结果。
    SearchMemoryEvents(vulcan_vmm::SearchMemoryEventsResponse),
    /// Source turn details.
    /// 来源回合详情。
    GetTurnDetails(vulcan_vmm::GetTurnDetailsResponse),
    /// Direct memory write result.
    /// 主动记忆写入结果。
    WriteMemories(vulcan_vmm::WriteMemoriesResponse),
    /// Memory deletion result.
    /// 记忆删除结果。
    DeleteMemories(vulcan_vmm::DeleteMemoriesResponse),
    /// Managed backup and vector-rebuild status.
    /// 托管备份与向量重建状态。
    Maintenance(VmmMaintenanceSummary),
    /// Structured response returned by the isolated VMM management HTTP API.
    /// 独立 VMM 管理 HTTP API 返回的结构化响应。
    Management(serde_json::Value),
}

impl Default for VmmMemoryActionResponse {
    /// Returns the empty typed response used before an action completes.
    /// 返回动作完成前使用的空强类型响应。
    fn default() -> Self {
        Self::Empty
    }
}

/// Result body returned by one WebUI VMM management action.
/// WebUI 单次 VMM 管理动作返回的结果体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemoryActionResult {
    /// Stable action id.
    /// 稳定动作标识。
    pub action: String,
    /// Compact user-facing summary.
    /// 紧凑用户可见摘要。
    pub summary: String,
    /// Structured response kind.
    /// 结构化响应类型。
    pub kind: String,
    /// Raw request preview JSON.
    /// 原始请求预览 JSON。
    pub request: serde_json::Value,
    /// Raw response preview JSON.
    /// 原始响应预览 JSON。
    pub response: serde_json::Value,
    /// Typed response consumed by normal product pages.
    /// 普通产品页面消费的强类型响应。
    pub data: VmmMemoryActionResponse,
}

/// Request body for saving a WebUI VMM configuration draft.
/// 保存 WebUI VMM 配置草稿的请求体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmConfigSaveRequest {
    /// Scope key, either global or project.
    /// 作用域键，可为 global 或 project。
    pub scope: String,
    /// Optional task project id used by project scope.
    /// 项目作用域使用的可选任务项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Draft data to persist.
    /// 需要持久化的草稿数据。
    pub draft: VmmConfigDraftSummary,
}

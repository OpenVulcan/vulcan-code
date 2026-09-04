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
    /// Vulcan Code owns the Rust-native VMM Core in-process.
    /// VulcanCode 在进程内负责 Rust 原生 VMM Core。
    Native,
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

/// Describes the selected VMM backend lifecycle independently from setup and health.
/// 独立于配置与健康状态描述所选 VMM 后端生命周期。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VmmLifecycleState {
    /// The selected VMM backend is stopped or disabled.
    /// 所选 VMM 后端已停止或禁用。
    #[default]
    Stopped,
    /// The selected VMM backend is running.
    /// 所选 VMM 后端正在运行。
    Running,
    /// The selected VMM backend is unavailable because its latest validation failed.
    /// 所选 VMM 后端因最近一次校验失败而不可用。
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
    /// One transient result attached only to the project-add response snapshot.
    /// 仅附加在项目添加响应快照上的一次性结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_binding_notice: Option<VmmProjectBindingNotice>,
}

/// Reports the native or external VMM binding attempted after adding one VulcanCode project.
/// 报告添加一个 VulcanCode 项目后尝试的原生或外置 VMM 绑定。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmProjectBindingNotice {
    /// Stable VulcanCode project tag.
    /// 稳定的 VulcanCode 项目标签。
    pub vulcan_project_id: String,
    /// Stable outcome token: `bound` or `failed`.
    /// 稳定结果标记：`bound` 或 `failed`。
    pub status: String,
    /// Bound VMM project display name when successful.
    /// 成功时绑定的 VMM 项目显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmm_project_name: Option<String>,
    /// Bound native VMM project id when available.
    /// 可用时绑定的原生 VMM 项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_vmm_project_id: Option<u64>,
    /// Bound external VMM project id when available.
    /// 可用时绑定的外置 VMM 项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_vmm_project_id: Option<u64>,
    /// Explicit success or failure message suitable for a toast.
    /// 适合通知提示的明确成功或失败消息。
    pub message: String,
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

/// Editable VMM configuration draft summary.
/// 可编辑 VMM 配置草稿摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmConfigDraftSummary {
    /// Whether this layer has explicit overrides.
    /// 当前层是否存在显式覆盖。
    pub has_override: bool,
    /// Editable global backend selector using `native` or `external_grpc`.
    /// 可编辑的全局后端选择器，取值为 `native` 或 `external_grpc`。
    pub backend_mode: String,
    /// Native new-project binding policy, either `fixed` or `per_project`.
    /// 原生新增项目绑定策略，取值为 `fixed` 或 `per_project`。
    pub native_project_binding_mode: String,
    /// External new-project binding policy, either `fixed` or `per_project`.
    /// 外置新增项目绑定策略，取值为 `fixed` 或 `per_project`。
    pub external_project_binding_mode: String,
    /// Stable native VMM project id used by the fixed binding policy.
    /// 固定绑定策略使用的稳定原生 VMM 项目标识。
    pub native_fixed_project_id: String,
    /// External Team/Space prefix used only for automatic project creation.
    /// 仅用于外置自动创建项目的 Team/Space 前缀。
    pub external_project_prefix: String,
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
    /// External VMM project id explicitly persisted for this VulcanCode project.
    /// 为该 VulcanCode 项目显式持久化的外置 VMM 项目标识。
    pub bound_project_id: Option<u64>,
}

/// Runtime state summary for VMM hooks.
/// VMM hook 运行态摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmRuntimeSummary {
    /// Whether the active VMM process or externally probed endpoint is ready.
    /// 当前 VMM 进程或已显式探测的外部端点是否已就绪。
    pub process_ready: bool,
    /// Active backend state, either `native`, `external`, or `disabled`.
    /// 当前后端状态，取值为 `native`、`external` 或 `disabled`。
    pub backend_state: String,
    /// Latest controlled backend error.
    /// 最近一次受控后端错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_error: Option<String>,
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

/// One one-level native VMM project exposed to the product UI.
/// 向产品界面暴露的单个一级原生 VMM 项目。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProjectSummary {
    /// Stable numeric native VMM project identity.
    /// 稳定的原生 VMM 数字项目标识。
    pub project_id: u64,
    /// Editable display name.
    /// 可编辑显示名称。
    pub name: String,
    /// Immutable project tag used for deterministic rebinding.
    /// 用于确定性重新绑定的不可变项目标签。
    pub tag: String,
    /// Whether deletion is prohibited by the data model.
    /// 数据模型是否禁止删除该项目。
    pub protected: bool,
    /// VulcanCode projects that write to this native VMM project.
    /// 写入该原生 VMM 项目的 VulcanCode 项目。
    pub owner_vulcan_project_ids: Vec<String>,
    /// VulcanCode projects that additionally query this native VMM project.
    /// 额外查询该原生 VMM 项目的 VulcanCode 项目。
    pub linked_vulcan_project_ids: Vec<String>,
}

/// One stable VulcanCode-to-native-VMM project binding.
/// 单个稳定的 VulcanCode 到原生 VMM 项目绑定。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProjectBindingSummary {
    /// Stable VulcanCode project tag.
    /// 稳定的 VulcanCode 项目标签。
    pub vulcan_project_id: String,
    /// Unique native VMM owner project id used for writes.
    /// 写入使用的唯一原生 VMM 所有者项目标识。
    pub owner_project_id: u64,
    /// Ordered additional native VMM project ids used only for recall.
    /// 仅用于召回的有序附加原生 VMM 项目标识。
    pub linked_project_ids: Vec<u64>,
}

/// Complete native project and binding catalog loaded from the host-owned data plane.
/// 从宿主持有的数据平面加载的完整原生项目及绑定目录。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProjectCatalogSummary {
    /// Every one-level native VMM project.
    /// 全部一级原生 VMM 项目。
    pub projects: Vec<VmmNativeProjectSummary>,
    /// Every current VulcanCode project binding.
    /// 全部当前 VulcanCode 项目绑定。
    pub bindings: Vec<VmmNativeProjectBindingSummary>,
}

/// Preview of deleting one native VMM project and detaching its consumers.
/// 删除一个原生 VMM 项目并解除其使用方之前的预检结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProjectDeletePreview {
    /// Core storage version that must still match when deletion is confirmed.
    /// 确认删除时必须仍然匹配的 Core 存储版本。
    pub storage_version: u64,
    /// Project selected for deletion.
    /// 选择删除的项目。
    pub project: VmmNativeProjectSummary,
    /// Project-owned content that will be permanently removed.
    /// 将被永久删除的项目归属内容。
    pub content: VmmNativeProjectContentCounts,
}

/// Counts native VMM records affected by one project maintenance operation.
/// 统计一次原生 VMM 项目维护操作影响的记录。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProjectContentCounts {
    /// Number of distinct host sessions.
    /// 不同宿主会话数量。
    pub sessions: u64,
    /// Number of canonical turns.
    /// 规范回合数量。
    pub turns: u64,
    /// Number of durable memories.
    /// 持久记忆数量。
    pub memories: u64,
    /// Number of project profile nodes.
    /// 项目画像节点数量。
    pub profile_nodes: u64,
    /// Number of context-evidence rows.
    /// 情境证据数量。
    pub context_evidence: u64,
    /// Number of compact boundaries.
    /// 压缩边界数量。
    pub compact_boundaries: u64,
    /// Number of affected vector rows.
    /// 受影响的向量行数量。
    pub vector_rows: u64,
}

/// Counts native memories by their four authoritative lifecycle levels.
/// 按四个权威生命周期等级统计原生记忆。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMemoryLevelCounts {
    /// Number of session-level memories.
    /// 会话级记忆数量。
    pub session: u64,
    /// Number of phase-level memories.
    /// 阶段级记忆数量。
    pub phase: u64,
    /// Number of stable-level memories.
    /// 稳定级记忆数量。
    pub stable: u64,
    /// Number of persistent-level memories.
    /// 持久级记忆数量。
    pub persistent: u64,
}

/// Counts native turns by their authoritative extraction-processing state.
/// 按权威提取处理状态统计原生回合。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeTurnStatusCounts {
    /// Number of pending turns.
    /// 待分析回合数量。
    pub pending: u64,
    /// Number of atomically completed turns.
    /// 已原子完成分析的回合数量。
    pub completed: u64,
    /// Number of historical turns with unknown successful-analysis metadata.
    /// 成功分析元数据未知的历史回合数量。
    pub processed_unknown: u64,
    /// Number of explicitly bypassed turns.
    /// 被明确跳过分析的回合数量。
    pub analysis_bypassed: u64,
    /// Number of corrupted and isolated turns.
    /// 已损坏并隔离的回合数量。
    pub corrupted: u64,
}

/// Summarizes native memory activity attributable to one host session.
/// 汇总可归属于一个宿主会话的原生记忆活动。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMemorySessionSummary {
    /// Stable host session identity persisted by Core.
    /// Core 持久化的稳定宿主会话标识。
    pub session_source_id: String,
    /// Current VulcanCode session title when the host still retains that session.
    /// 宿主仍保留该会话时的当前 VulcanCode 会话标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    /// Number of canonical turns in the session.
    /// 会话中的规范回合数量。
    pub turn_count: u64,
    /// Turn counts split by authoritative processing state.
    /// 按权威处理状态拆分的回合统计。
    pub turn_statuses: VmmNativeTurnStatusCounts,
    /// Number of memories attributed to the session.
    /// 归属于会话的记忆数量。
    pub memory_count: u64,
    /// Number of user or project profile nodes attributed to turns in the session.
    /// 可归属于该会话回合的用户或项目画像节点数量。
    pub profile_node_count: u64,
    /// Number of explicit direct writes without one source turn.
    /// 不含来源回合的显式主动写入数量。
    pub direct_memory_count: u64,
    /// Memory counts split by authoritative lifecycle level.
    /// 按权威生命周期等级拆分的记忆统计。
    pub memory_levels: VmmNativeMemoryLevelCounts,
    /// Earliest attributable event time in Unix milliseconds.
    /// 最早可归属事件的 Unix 毫秒时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_activity_at_unix_ms: Option<i64>,
    /// Latest attributable event time in Unix milliseconds.
    /// 最近可归属事件的 Unix 毫秒时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at_unix_ms: Option<i64>,
}

/// One complete persisted timeline item within a native canonical turn.
/// 原生规范回合中的一条完整持久化时间线项目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeTimelineItemSummary {
    /// Stable author-role token.
    /// 稳定的作者角色标记。
    pub role: String,
    /// Full normalized timeline content.
    /// 完整的规范化时间线内容。
    pub content: String,
    /// Event time in Unix milliseconds.
    /// 事件时间的 Unix 毫秒值。
    pub occurred_at_unix_ms: i64,
}

/// Projects one canonical native turn with extraction and generated-memory status.
/// 投影一个规范原生回合的提取及生成记忆状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeTurnRecordSummary {
    /// Stable native numeric turn identity.
    /// 稳定的原生数字回合标识。
    pub turn_id: u64,
    /// Stable host session identity owning the turn.
    /// 拥有该回合的稳定宿主会话标识。
    pub session_source_id: String,
    /// Canonical creation time in Unix milliseconds.
    /// 规范创建时间的 Unix 毫秒值。
    pub created_at_unix_ms: i64,
    /// Stable authoritative turn-status token.
    /// 权威回合状态的稳定标记。
    pub status: String,
    /// Whether extraction is known complete; null preserves historical uncertainty.
    /// 是否已知完成提取；空值用于保留历史不确定性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted: Option<bool>,
    /// Compact first user message preview.
    /// 首条用户消息的紧凑预览。
    pub user_preview: String,
    /// Compact last assistant message preview.
    /// 最后一条助手消息的紧凑预览。
    pub assistant_preview: String,
    /// Complete canonical timeline in persisted order.
    /// 按持久化顺序排列的完整规范时间线。
    pub timeline: Vec<VmmNativeTimelineItemSummary>,
    /// Durable compact analysis detail when available.
    /// 可用时的持久紧凑分析详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_details: Option<String>,
    /// Stable identities of memories extracted from this exact turn.
    /// 从该准确回合提取出的记忆稳定标识集合。
    pub generated_memory_ids: Vec<u64>,
    /// Stable identities of user or project profile nodes extracted from this exact turn.
    /// 从该准确回合提取出的用户或项目画像节点稳定标识集合。
    pub generated_profile_node_ids: Vec<u64>,
    /// Generated memories split by authoritative lifecycle level.
    /// 生成记忆按权威生命周期等级拆分的统计。
    pub memory_levels: VmmNativeMemoryLevelCounts,
    /// Active host queue state while analysis remains pending.
    /// 分析仍待处理时的活动宿主队列状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_job_state: Option<String>,
    /// Number of failed background analysis attempts.
    /// 后台分析失败尝试次数。
    pub analysis_attempt_count: u32,
    /// Latest bounded background analysis error.
    /// 最近一次有界后台分析错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_last_error: Option<String>,
}

/// Projects one durable native memory with exact provenance and lifecycle metadata.
/// 投影一条持久原生记忆的准确来源与生命周期元数据。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMemoryRecordSummary {
    /// Stable native numeric memory identity.
    /// 稳定的原生数字记忆标识。
    pub memory_id: u64,
    /// Originating turn identity for extracted memories.
    /// 提取型记忆的来源回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<u64>,
    /// Originating host session when present.
    /// 存在时的来源宿主会话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_source_id: Option<String>,
    /// Compact caller-facing summary.
    /// 面向调用方的紧凑摘要。
    pub summary: String,
    /// Full normalized memory statement.
    /// 完整的规范化记忆陈述。
    pub content: String,
    /// Stable authoritative category token.
    /// 权威分类的稳定标记。
    pub category: String,
    /// Stable authoritative source-kind token.
    /// 权威来源类型的稳定标记。
    pub source_kind: String,
    /// Stable authoritative lifecycle-state token.
    /// 权威生命周期状态的稳定标记。
    pub state: String,
    /// Stable authoritative memory-level token.
    /// 权威记忆等级的稳定标记。
    pub memory_level: String,
    /// Host-normalized priority from zero through one hundred.
    /// 宿主规范化的零到一百优先级。
    pub priority: u32,
    /// Searchable labels persisted with the memory.
    /// 与记忆一起持久化的可搜索标签。
    pub tags: Vec<String>,
    /// Creation time in Unix milliseconds.
    /// 创建时间的 Unix 毫秒值。
    pub created_at_unix_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间的 Unix 毫秒值。
    pub updated_at_unix_ms: i64,
    /// Optional absolute expiration time in Unix milliseconds.
    /// 可选的绝对过期 Unix 毫秒时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<i64>,
    /// Stable lifecycle reason when present.
    /// 存在时的稳定生命周期原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

/// Complete native project/session/turn/memory provenance response.
/// 完整的原生项目/会话/回合/记忆来源响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMemoryRecordsSummary {
    /// Core storage version used to build this coherent response.
    /// 构建该一致响应所使用的 Core 存储版本。
    pub storage_version: u64,
    /// Selected one-level native VMM project identity.
    /// 已选择的一级原生 VMM 项目标识。
    pub project_id: u64,
    /// Every session attributable to the selected project.
    /// 可归属于所选项目的全部会话。
    pub sessions: Vec<VmmNativeMemorySessionSummary>,
    /// Exact selected session when detailed records were requested.
    /// 请求详细记录时的准确所选会话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_session_source_id: Option<String>,
    /// Canonical turns for the selected session, or every project turn when no session is selected.
    /// 所选会话的规范回合集合；未选择会话时为项目全部回合。
    pub turns: Vec<VmmNativeTurnRecordSummary>,
    /// Durable memories for the selected session, or every project memory when no session is selected.
    /// 所选会话的持久记忆；未选择会话时为项目全部记忆。
    pub memories: Vec<VmmNativeMemoryRecordSummary>,
    /// User and project profile nodes attributable to the selected turns.
    /// 可归属于所选回合的用户与项目画像节点。
    pub profile_nodes: Vec<VmmNativeProfileRecordSummary>,
}

/// Counts native profile nodes by effective read-time lifecycle state.
/// 按读取时的有效生命周期状态统计原生画像节点。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProfileStateCounts {
    /// Nodes currently eligible for profile rendering and injection.
    /// 当前可参与画像渲染与注入的节点。
    pub active: u64,
    /// Nodes retained after deactivation or replacement.
    /// 停用或替换后保留的节点。
    pub inactive: u64,
    /// Extracted nodes awaiting an authoritative review result.
    /// 等待权威评审结果的已提取节点。
    pub pending: u64,
    /// Nodes past their lifecycle deadline.
    /// 已超过生命周期截止时间的节点。
    pub expired: u64,
    /// Active nodes whose effective date has not arrived yet.
    /// 生效日期尚未到达的活动节点。
    pub scheduled: u64,
}

/// Projects one native profile node with complete lifecycle and provenance metadata.
/// 投影一条包含完整生命周期与来源元数据的原生画像节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProfileRecordSummary {
    /// Stable native numeric profile-node identity.
    /// 稳定的原生数字画像节点标识。
    pub profile_node_id: u64,
    /// Stable profile target token: user or project.
    /// 稳定的画像目标标记：user 或 project。
    pub target: String,
    /// Persisted authoritative lifecycle state.
    /// 已持久化的权威生命周期状态。
    pub stored_state: String,
    /// Effective lifecycle state after current date boundaries are applied.
    /// 应用当前日期边界后的有效生命周期状态。
    pub effective_state: String,
    /// Whether this node is currently eligible for prompt injection.
    /// 该节点当前是否可参与提示词注入。
    pub active_for_injection: bool,
    /// Atomic profile statement.
    /// 原子化画像陈述。
    pub content: String,
    /// Host-normalized priority from zero through one hundred.
    /// 宿主规范化后的零到一百优先级。
    pub priority: u32,
    /// Stable profile lifecycle level token.
    /// 稳定的画像生命周期等级标记。
    pub level: String,
    /// Stable durable source-kind token.
    /// 稳定的持久来源类别标记。
    pub source_kind: String,
    /// Exact durable source identity when present.
    /// 存在时的准确持久来源标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Supporting canonical turn identities.
    /// 提供支撑的规范回合标识。
    pub source_turn_ids: Vec<u64>,
    /// Optional effective date in Unix milliseconds.
    /// 可选的生效日期 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date_at_unix_ms: Option<i64>,
    /// Optional absolute expiration time in Unix milliseconds.
    /// 可选的绝对过期 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<i64>,
    /// Stable lifecycle reason when present.
    /// 存在时的稳定生命周期原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间的 Unix 毫秒值。
    pub updated_at_unix_ms: i64,
}

/// Complete native user or project profile audit response.
/// 完整的原生用户或项目画像审计响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeProfileRecordsSummary {
    /// Core storage version used to build this coherent response.
    /// 构建该一致响应所使用的 Core 存储版本。
    pub storage_version: u64,
    /// Selected project used by project scope or user-profile display context.
    /// 项目作用域或用户画像显示上下文使用的所选项目。
    pub project_id: u64,
    /// Stable selected target token: user or project.
    /// 稳定的所选目标标记：user 或 project。
    pub target: String,
    /// Effective state counts covering every returned node.
    /// 覆盖全部返回节点的有效状态统计。
    pub counts: VmmNativeProfileStateCounts,
    /// Every durable node for the selected target, including audit-only states.
    /// 所选目标的全部持久节点，包括仅供审计的状态。
    pub nodes: Vec<VmmNativeProfileRecordSummary>,
}

/// Describes one mandatory automatic session rename in a native migration.
/// 描述原生迁移中的一次必须自动会话改名。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeSessionRename {
    /// Original source session identity.
    /// 原始来源会话身份。
    pub from: String,
    /// Collision-free migrated session identity.
    /// 无冲突的迁移后会话身份。
    pub to: String,
}

/// User-visible primary record included by a native migration preview.
/// 原生迁移预检包含的一条用户可见主记录。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMigrationRecordSummary {
    /// Stable record class: session, turn, memory, or project_profile.
    /// 稳定记录类别：会话、回合、记忆或项目画像。
    pub record_kind: String,
    /// Stable Core identity, or the host session identity for a session row.
    /// 稳定 Core 身份；会话行则使用宿主会话身份。
    pub record_id: String,
    /// Originating host session when the record belongs to one.
    /// 当记录属于某个宿主会话时携带其来源会话。
    pub session_source_id: Option<String>,
    /// Compact non-secret description available for human review.
    /// 供人工核对的紧凑非敏感描述。
    pub summary: Option<String>,
}

/// Side-effect-free preview returned before a native project migration.
/// 原生项目迁移前返回的无副作用预检。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMigrationPreview {
    /// Storage version required by the confirmation request.
    /// 确认请求必须携带的存储版本。
    pub storage_version: u64,
    /// Source native project identity.
    /// 来源原生项目身份。
    pub source_project_id: u64,
    /// Target native project identity.
    /// 目标原生项目身份。
    pub target_project_id: u64,
    /// Source content to move.
    /// 将迁移的来源内容。
    pub source: VmmNativeProjectContentCounts,
    /// Target content before migration.
    /// 迁移前的目标内容。
    pub target_before: VmmNativeProjectContentCounts,
    /// Projected target content after migration.
    /// 迁移后的预计目标内容。
    pub target_after: VmmNativeProjectContentCounts,
    /// Conflicting sessions and their deterministic new identities.
    /// 冲突会话及其确定性新身份。
    pub session_renames: Vec<VmmNativeSessionRename>,
    /// User-visible primary records whose project ownership will change.
    /// 将改变项目归属的用户可见主记录。
    pub records: Vec<VmmNativeMigrationRecordSummary>,
    /// SHA-256 identity of the exact displayed preview required by confirmation.
    /// 确认操作必须携带的用户所见准确预检 SHA-256 身份。
    pub preview_fingerprint: String,
}

/// Result returned after a native project migration commits.
/// 原生项目迁移提交后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeMigrationResult {
    /// New committed storage version.
    /// 新提交的存储版本。
    pub storage_version: u64,
    /// Content moved from the source project.
    /// 从来源项目迁出的内容。
    pub moved: VmmNativeProjectContentCounts,
    /// Session renames applied by the migration.
    /// 迁移实际应用的会话改名。
    pub session_renames: Vec<VmmNativeSessionRename>,
}

/// Side-effect-free preview returned before clearing a native project.
/// 清空原生项目前返回的无副作用预检。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeClearPreview {
    /// Storage version required by the confirmation request.
    /// 确认请求必须携带的存储版本。
    pub storage_version: u64,
    /// Native project whose content will be cleared.
    /// 将被清空内容的原生项目。
    pub project_id: u64,
    /// Content that will be removed.
    /// 将被删除的内容。
    pub removed: VmmNativeProjectContentCounts,
}

/// Result returned after clearing a native project commits.
/// 清空原生项目提交后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeClearResult {
    /// New committed storage version.
    /// 新提交的存储版本。
    pub storage_version: u64,
    /// Content removed by the clear.
    /// 本次清空删除的内容。
    pub removed: VmmNativeProjectContentCounts,
}

/// Native Core resource selections persisted under the host-owned VMM configuration directory.
/// 持久化在宿主持有 VMM 配置目录下的原生 Core 资源选择。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeResourceSettingsSummary {
    /// Selected complete prompt bundle.
    /// 当前选择的完整提示词 Bundle。
    pub prompt_bundle: String,
    /// Selected PII locale combined after common rules.
    /// 在公共规则后组合的 PII 区域。
    pub pii_language: String,
    /// Selected Noise locale combined after common rules.
    /// 在公共规则后组合的噪音区域。
    pub noise_language: String,
    /// Whether Noise filtering is active.
    /// 是否启用噪音过滤。
    pub noise_enabled: bool,
    /// Whether semantic Noise phrase matching is active.
    /// 是否启用语义噪音短语匹配。
    pub noise_semantic_enabled: bool,
    /// Default semantic Noise threshold.
    /// 默认语义噪音阈值。
    pub noise_semantic_threshold: f64,
}

/// Describes one independently selectable native Core resource file.
/// 描述一份可独立选择的原生 Core 资源文件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeResourceFileSummary {
    /// Exact slash-separated path used by Core layering.
    /// Core 分层使用的准确斜杠路径。
    pub path: String,
    /// Resource group: prompt, pii, or noise.
    /// 资源分组：prompt、pii 或 noise。
    pub group: String,
    /// Bundle, locale, or common selector owning the file.
    /// 拥有该文件的 Bundle、区域或 common 选择器。
    pub selector: String,
    /// File name within the selected group.
    /// 所选分组中的文件名。
    pub file_name: String,
    /// Whether editing this file requires the two-step high-risk confirmation.
    /// 编辑该文件是否需要两步高风险确认。
    pub high_risk: bool,
    /// Whether an effective user-layer override currently exists.
    /// 当前是否存在有效用户层覆盖。
    pub has_user_override: bool,
}

/// Catalog of native Core resource files and active composition settings.
/// 原生 Core 资源文件与当前组合设置的目录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeResourceCatalogSummary {
    /// Absolute user-owned override directory used without modifying embedded resources.
    /// 不修改内置资源时使用的用户自有覆盖目录绝对路径。
    pub override_root: String,
    /// Current persisted or default resource selections.
    /// 当前持久化或默认资源选择。
    pub settings: VmmNativeResourceSettingsSummary,
    /// Available complete prompt bundles.
    /// 可用的完整提示词 Bundle。
    pub prompt_bundles: Vec<String>,
    /// Required prompt scene file names.
    /// 必需的提示词场景文件名。
    pub prompt_scenes: Vec<String>,
    /// Available PII locale selectors excluding common.
    /// 可用的 PII 区域选择，不含 common。
    pub pii_languages: Vec<String>,
    /// Available Noise locale selectors excluding common.
    /// 可用的噪音区域选择，不含 common。
    pub noise_languages: Vec<String>,
    /// Every independently selectable resource file.
    /// 每份可独立选择的资源文件。
    pub files: Vec<VmmNativeResourceFileSummary>,
}

/// Exact built-in and effective text for one native Core resource file.
/// 一份原生 Core 资源文件的准确内置与生效文本。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeResourceFileContent {
    /// Exact catalog path.
    /// 准确目录路径。
    pub path: String,
    /// Immutable source compiled into the application.
    /// 编译进应用程序的不可变原文。
    pub built_in_text: String,
    /// Effective text selected by user-over-built-in precedence.
    /// 按用户层优先于内置层规则选择的生效文本。
    pub effective_text: String,
    /// User override text when a physical override exists.
    /// 存在物理覆盖文件时的用户覆盖文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_override_text: Option<String>,
    /// Whether the effective text comes from the user layer.
    /// 生效文本是否来自用户层。
    pub has_user_override: bool,
    /// Whether editing requires the two-step high-risk confirmation.
    /// 编辑是否需要两步高风险确认。
    pub high_risk: bool,
}

/// Result of applying active common-plus-locale PII rules to sample text.
/// 使用当前 common 加区域 PII 规则处理示例文本的结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativePiiTestResult {
    /// Scrubbed output text.
    /// 脱敏后的输出文本。
    pub output_text: String,
    /// Applied rule names and replacement counts in execution order.
    /// 按执行顺序排列的命中规则名与替换数量。
    pub redactions: Vec<VmmNativePiiRedactionSummary>,
}

/// One PII tester rule match summary.
/// 一条 PII 测试规则命中摘要。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativePiiRedactionSummary {
    /// Stable rule name.
    /// 稳定规则名。
    pub rule_name: String,
    /// Number of replacements made by the rule.
    /// 该规则执行的替换数量。
    pub match_count: u32,
}

/// Result of compiling and executing one ad-hoc PII regex plus condition formula.
/// 编译并执行一条临时 PII 正则及条件公式的结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativePiiFormulaTestResult {
    /// Text after accepted candidates have been replaced.
    /// 接受的候选被替换后的文本。
    pub output_text: String,
    /// Full local trace for every regex candidate, including rejected candidates.
    /// 每个正则候选的完整本地轨迹，包括被拒绝的候选。
    pub matches: Vec<VmmNativePiiFormulaMatchSummary>,
}

/// One candidate trace produced by the authoritative PII condition evaluator.
/// 权威 PII 条件求值器产生的一条候选轨迹。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativePiiFormulaMatchSummary {
    /// Byte offset at which the complete match begins.
    /// 完整匹配开始处的字节偏移。
    pub start_byte: usize,
    /// Exclusive byte offset at which the complete match ends.
    /// 完整匹配结束处的排他字节偏移。
    pub end_byte: usize,
    /// Complete `$0` match text shown in this explicit local tester.
    /// 此显式本地测试器展示的完整 `$0` 匹配文本。
    pub matched_text: String,
    /// Ordered `$0..$n` capture values.
    /// 有序的 `$0..$n` 捕获值。
    pub captures: Vec<Option<String>>,
    /// Whether the condition formula accepted this regex candidate.
    /// 条件公式是否接受该正则候选。
    pub condition_accepted: bool,
}

/// Deterministic result of testing active common-plus-locale Noise rules.
/// 测试当前 common 加区域噪音规则的确定性结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeNoiseTestResult {
    /// Whether a deterministic regex rule classified the text as noise.
    /// 是否有确定性正则规则将文本判定为噪音。
    pub is_noise: bool,
    /// Matched category when present.
    /// 存在时的命中类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Deterministic confidence reported by Core.
    /// Core 报告的确定性置信度。
    pub confidence: f64,
    /// Explicit tester mode; semantic matching remains part of live intake.
    /// 明确的测试器模式；语义匹配仍属于实时录入链路。
    pub mode: String,
}

/// Frozen native retention policy exposed for maintenance diagnostics.
/// 为维护诊断暴露的冻结原生保留策略。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRetentionPolicySummary {
    /// Closed policy schema version.
    /// 封闭策略 Schema 版本。
    pub schema_version: u32,
    /// Effective number of newest turns protected per session.
    /// 每个会话受保护的最新回合有效数量。
    pub turn_hot_window_size: u32,
    /// Session idle duration before cleanup evaluation, in milliseconds.
    /// 进入清洗评估前的会话空闲毫秒数。
    pub session_idle_ms: u64,
    /// Recycle-bin retention duration before automatic purge, in milliseconds.
    /// 回收站自动清除前的保留毫秒数。
    pub trash_retention_ms: u64,
    /// Lowest protected priority on the zero-to-one-hundred scale.
    /// 零到一百尺度上的最低受保护优先级。
    pub protected_priority_floor: u32,
    /// Lowest protected memory-level discriminant.
    /// 最低受保护记忆等级判别值。
    pub protected_memory_level_floor: u32,
    /// Whether either protection floor preserves shared terminal memories.
    /// 是否由任一保护下限保留共享终态记忆。
    pub skip_protected_shared_memories: bool,
    /// Maximum due active profile nodes expired by one project pass.
    /// 单个项目轮次最多收敛的到期活跃画像节点数。
    pub profile_expiry_limit: u32,
    /// Maximum terminal memories recycled by one project pass.
    /// 单个项目轮次最多回收的终态记忆数。
    pub memory_recycle_limit: u32,
    /// Maximum sessions scanned for cold-turn work by one project pass.
    /// 单个项目轮次最多扫描的冷回合会话数。
    pub cold_turn_scan_limit: u32,
    /// Maximum due cold-turn jobs executed by one project pass.
    /// 单个项目轮次最多执行的到期冷回合作业数。
    pub cold_turn_job_limit: u32,
    /// Maximum idle sessions recycled by one project pass.
    /// 单个项目轮次最多回收的闲置会话数。
    pub idle_session_limit: u32,
    /// External-compatible vector-GC batch limit retained in the Core policy.
    /// Core 策略中保留的外置兼容向量 GC 批次上限。
    pub vector_gc_limit: u32,
    /// Maximum expired recycle batches purged by one project pass.
    /// 单个项目轮次最多清除的过期回收批次数。
    pub trash_purge_limit: u32,
}

/// Aggregate counters produced by one complete native retention run.
/// 一次完整原生保留运行产生的汇总计数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRetentionApplyResult {
    /// Due active profile nodes materialized as expired.
    /// 被真实收敛为过期状态的到期活跃画像节点数量。
    pub expired_profile_nodes: u64,
    /// Terminal or idle memories moved to the recycle bin.
    /// 移入回收站的终态或闲置记忆数量。
    pub recycled_memories: u64,
    /// Cold or idle turns moved to the recycle bin.
    /// 移入回收站的冷回合或闲置回合数量。
    pub recycled_turns: u64,
    /// Durable cold-turn jobs created by the run.
    /// 本次运行创建的耐久冷回合作业数量。
    pub enqueued_cold_jobs: u64,
    /// Durable cold-turn jobs completed by the run.
    /// 本次运行完成的耐久冷回合作业数量。
    pub completed_cold_jobs: u64,
    /// Expired recycle batches permanently purged by the run.
    /// 本次运行永久清除的过期回收批次数量。
    pub purged_trash_batches: u64,
    /// Project-local failures collected while unrelated projects continued.
    /// 在其他无关项目继续运行时收集的项目局部失败。
    pub project_failures: Vec<VmmNativeRetentionProjectFailure>,
}

/// One project-local failure from a complete native retention run.
/// 一次完整原生保留运行中的项目局部失败。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRetentionProjectFailure {
    /// One-level native project whose maintenance failed.
    /// 维护失败的一级原生项目。
    pub project_id: u64,
    /// Exact planning or persistence failure.
    /// 准确的规划或持久化失败。
    pub error: String,
}

/// Current schedule and last-run state of native periodic retention.
/// 原生周期保留的当前调度与最近运行状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRetentionStatusSummary {
    /// Whether the globally selected native backend enables the worker.
    /// 全局选择的原生后端是否启用该工作器。
    pub enabled: bool,
    /// Whether one scheduled or manual pass is running.
    /// 是否有一次定时或手动运行正在执行。
    pub running: bool,
    /// Whether the independently scheduled profile-expiry convergence is running.
    /// 独立调度的画像过期收敛是否正在运行。
    pub profile_expiry_running: bool,
    /// Fixed schedule interval in minutes.
    /// 固定调度周期的分钟数。
    pub interval_minutes: u64,
    /// Fixed profile-expiry convergence interval in seconds.
    /// 固定画像过期收敛周期的秒数。
    pub profile_expiry_interval_seconds: u64,
    /// Frozen retention policy applied by Core.
    /// Core 应用的冻结保留策略。
    pub policy: VmmNativeRetentionPolicySummary,
    /// Start time of the latest attempted run in Unix milliseconds.
    /// 最近一次尝试运行开始时间的 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_started_at_unix_ms: Option<i64>,
    /// Finish time of the latest attempted run in Unix milliseconds.
    /// 最近一次尝试运行结束时间的 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_finished_at_unix_ms: Option<i64>,
    /// Latest exact failure, absent after a successful run.
    /// 最近一次准确失败；成功运行后为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Aggregate result of the latest completed run, including project-local failures.
    /// 最近一次已完成运行的汇总结果，包括项目局部失败。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<VmmNativeRetentionApplyResult>,
    /// Start time of the latest profile-expiry convergence in Unix milliseconds.
    /// 最近一次画像过期收敛开始时间的 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_expiry_last_started_at_unix_ms: Option<i64>,
    /// Finish time of the latest profile-expiry convergence in Unix milliseconds.
    /// 最近一次画像过期收敛结束时间的 Unix 毫秒值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_expiry_last_finished_at_unix_ms: Option<i64>,
    /// Latest exact profile-expiry convergence failure.
    /// 最近一次准确的画像过期收敛失败。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_expiry_last_error: Option<String>,
    /// Aggregate result of the latest profile-expiry convergence.
    /// 最近一次画像过期收敛的汇总结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_expiry_last_result: Option<VmmNativeRetentionApplyResult>,
    /// Current durable cold-turn job count.
    /// 当前耐久冷回合作业数量。
    pub pending_cold_jobs: u64,
    /// Current durable recycle-batch count.
    /// 当前耐久回收批次数量。
    pub recycle_batch_count: u64,
}

/// Current compatibility between native active/recoverable memories and the embedding generation.
/// 原生活动/可恢复记忆与已配置 Embedding 代次之间的当前兼容状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeVectorStatusSummary {
    /// Whether the configured route can produce an exact vector generation.
    /// 已配置路由是否能够产生准确向量代次。
    pub configured: bool,
    /// Current route-derived logical vector-space identity when configured.
    /// 已配置时当前由路由派生的逻辑向量空间标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_vector_space_id: Option<String>,
    /// Current dense embedding width when configured.
    /// 已配置时当前稠密向量维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_dimension: Option<u32>,
    /// Number of all active native memories.
    /// 全部原生活动记忆数量。
    pub active_memory_count: u64,
    /// Number of memories retained in recoverable recycle batches.
    /// 可恢复回收批次中的记忆数量。
    pub recycle_bin_memory_count: u64,
    /// Number of active memories already using the current generation.
    /// 已使用当前代次的活动记忆数量。
    pub current_memory_count: u64,
    /// Number of recoverable memories already using the current generation.
    /// 已使用当前代次的可恢复记忆数量。
    pub current_recycle_bin_memory_count: u64,
    /// Total number of active and recoverable memories requiring an explicit rebuild.
    /// 需要显式重建的活动与可恢复记忆总数。
    pub rebuild_required_memory_count: u64,
    /// Number of incompatible active memories.
    /// 不兼容的活动记忆数量。
    pub active_rebuild_required_memory_count: u64,
    /// Number of incompatible recoverable memories.
    /// 不兼容的可恢复记忆数量。
    pub recycle_bin_rebuild_required_memory_count: u64,
    /// Number of distinct older logical spaces still referenced by active memories.
    /// 活动记忆仍引用的不同旧逻辑空间数量。
    pub legacy_vector_space_count: u64,
    /// Number of durable turns waiting for PostAction analysis.
    /// 等待 PostAction 分析的耐久回合数量。
    pub pending_analysis_job_count: u64,
    /// Exact route-resolution failure when configuration is unavailable.
    /// 配置不可用时的准确路由解析失败。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Side-effect-free confirmation payload for one native vector-generation rebuild.
/// 一次原生向量代次重建的无副作用确认载荷。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeVectorRebuildPreview {
    /// Core storage version required by execution.
    /// 执行所需的 Core 存储版本。
    pub storage_version: u64,
    /// Exact target logical vector generation.
    /// 准确目标逻辑向量代次。
    pub target_vector_space_id: String,
    /// Exact target embedding width.
    /// 准确目标向量维度。
    pub embedding_dimension: u32,
    /// Total active-memory count inspected by the preview.
    /// 预检检查的活动记忆总数。
    pub active_memory_count: u64,
    /// Total recycle-bin memory count inspected by the preview.
    /// 预检检查的回收站记忆总数。
    pub recycle_bin_memory_count: u64,
    /// Active and recoverable memories requiring a new embedding.
    /// 需要生成新向量的活动与可恢复记忆数量。
    pub rebuild_memory_count: u64,
    /// Active memories requiring a new embedding.
    /// 需要生成新向量的活动记忆数量。
    pub active_rebuild_memory_count: u64,
    /// Recoverable memories requiring a new embedding.
    /// 需要生成新向量的可恢复记忆数量。
    pub recycle_bin_rebuild_memory_count: u64,
    /// Project scopes affected by the rebuild.
    /// 重建影响的项目作用域数量。
    pub affected_project_count: u64,
    /// Exact number of bounded durable batches across active and recoverable storage.
    /// 跨活动与可恢复存储的有界耐久批次数量。
    pub batch_count: u64,
    /// SHA-256 identity of the exact displayed target set.
    /// 所显示准确目标集合的 SHA-256 身份。
    pub preview_fingerprint: String,
}

/// Result of one confirmed native vector-generation rebuild.
/// 一次已确认原生向量代次重建的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeVectorRebuildResult {
    /// Final committed Core storage version.
    /// 最终提交的 Core 存储版本。
    pub storage_version: u64,
    /// Exact target logical vector generation.
    /// 准确目标逻辑向量代次。
    pub target_vector_space_id: String,
    /// Number of memories rebuilt by this execution.
    /// 本次执行重建的记忆数量。
    pub rebuilt_memory_count: u64,
    /// Number of durable batches committed by this execution.
    /// 本次执行提交的耐久批次数量。
    pub batch_count: u64,
}

/// One native recycle-bin batch available for restore or permanent purge.
/// 一份可恢复或永久清除的原生回收站批次。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRecycleBatchSummary {
    /// Stable batch identity used by mutation commands.
    /// 变更命令使用的稳定批次身份。
    pub batch_id: String,
    /// One-level native project that owns the batch.
    /// 拥有该批次的一级原生项目。
    pub project_id: u64,
    /// Stable retention reason token.
    /// 稳定保留原因标记。
    pub reason: String,
    /// Recycle timestamp in Unix milliseconds.
    /// 回收时间的 Unix 毫秒值。
    pub recycled_at_unix_ms: i64,
    /// Automatic permanent-purge deadline in Unix milliseconds.
    /// 自动永久清除截止时间的 Unix 毫秒值。
    pub expires_at_unix_ms: i64,
    /// Recycled canonical turn count.
    /// 已回收规范回合数量。
    pub turn_count: u64,
    /// Recycled durable memory count.
    /// 已回收持久记忆数量。
    pub memory_count: u64,
    /// Recycled profile-node count.
    /// 已回收画像节点数量。
    pub profile_node_count: u64,
    /// Recycled context-evidence row count.
    /// 已回收情境证据行数量。
    pub context_evidence_count: u64,
}

/// Coherent native recycle-bin projection guarded by one storage version.
/// 由一个存储版本保护的一致原生回收站投影。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmNativeRecycleBinSummary {
    /// Core storage version required by restore and purge confirmations.
    /// 恢复与清除确认所需的 Core 存储版本。
    pub storage_version: u64,
    /// Recycle batches ordered newest first.
    /// 按最新优先排列的回收批次。
    pub batches: Vec<VmmNativeRecycleBatchSummary>,
}

/// Typed payload returned by one VMM management action.
/// VMM 单次管理动作返回的强类型载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VmmMemoryActionResponse {
    /// No typed payload is available for one diagnostic-only action.
    /// 诊断专用动作没有可用的强类型载荷。
    Empty,
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
    /// Native one-level project and binding catalog.
    /// 原生一级项目与绑定目录。
    NativeProjectCatalog(VmmNativeProjectCatalogSummary),
    /// Native project/session/turn/memory provenance records.
    /// 原生项目/会话/回合/记忆来源记录。
    NativeMemoryRecords(VmmNativeMemoryRecordsSummary),
    /// Native all-state user or project profile audit records.
    /// 原生全状态用户或项目画像审计记录。
    NativeProfileRecords(VmmNativeProfileRecordsSummary),
    /// Native project deletion impact preview.
    /// 原生项目删除影响预检。
    NativeProjectDeletePreview(VmmNativeProjectDeletePreview),
    /// Native project migration preview.
    /// 原生项目迁移预检。
    NativeMigrationPreview(VmmNativeMigrationPreview),
    /// Committed native project migration result.
    /// 已提交的原生项目迁移结果。
    NativeMigration(VmmNativeMigrationResult),
    /// Native project clear preview.
    /// 原生项目清空预检。
    NativeClearPreview(VmmNativeClearPreview),
    /// Committed native project clear result.
    /// 已提交的原生项目清空结果。
    NativeClear(VmmNativeClearResult),
    /// Native Core resource catalog and current selections.
    /// 原生 Core 资源目录与当前选择。
    NativeResourceCatalog(VmmNativeResourceCatalogSummary),
    /// One native Core resource file's built-in and effective text.
    /// 一份原生 Core 资源文件的内置与生效文本。
    NativeResourceFile(VmmNativeResourceFileContent),
    /// Active PII-rule tester result.
    /// 当前 PII 规则测试结果。
    NativePiiTest(VmmNativePiiTestResult),
    /// Ad-hoc PII regex and condition-formula tester result.
    /// 临时 PII 正则与条件公式测试结果。
    NativePiiFormulaTest(VmmNativePiiFormulaTestResult),
    /// Active deterministic Noise-rule tester result.
    /// 当前确定性噪音规则测试结果。
    NativeNoiseTest(VmmNativeNoiseTestResult),
    /// Current native periodic-retention status and frozen policy.
    /// 当前原生周期保留状态与冻结策略。
    NativeRetentionStatus(VmmNativeRetentionStatusSummary),
    /// Result counters returned by a completed manual retention run.
    /// 完成人工保留运行后返回的结果计数。
    NativeRetentionRun(VmmNativeRetentionApplyResult),
    /// Current native embedding-generation compatibility.
    /// 当前原生 Embedding 代次兼容状态。
    NativeVectorStatus(VmmNativeVectorStatusSummary),
    /// Side-effect-free native vector-rebuild preview.
    /// 无副作用的原生向量重建预检。
    NativeVectorRebuildPreview(VmmNativeVectorRebuildPreview),
    /// Completed native vector-generation rebuild.
    /// 已完成的原生向量代次重建。
    NativeVectorRebuild(VmmNativeVectorRebuildResult),
    /// Current native recycle-bin batches.
    /// 当前原生回收站批次。
    NativeRecycleBin(VmmNativeRecycleBinSummary),
    /// One restored or permanently purged native recycle batch.
    /// 一份已恢复或已永久清除的原生回收批次。
    NativeRecycleBatch(VmmNativeRecycleBatchSummary),
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
    /// Explicit decisions for every currently unbound project under the selected backend.
    /// 针对所选后端的每个当前未绑定项目作出的明确决定。
    #[serde(default)]
    pub binding_plan: Vec<VmmProjectBindingPlanEntry>,
}

/// One explicit binding decision submitted while the selected VMM backend is being enabled.
/// 启用所选 VMM 后端时提交的单个明确绑定决定。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmProjectBindingPlanEntry {
    /// Stable VulcanCode project tag being configured.
    /// 正在配置的稳定 VulcanCode 项目标签。
    pub vulcan_project_id: String,
    /// Action token: `create_and_bind` or `bind_existing`.
    /// 动作标记：`create_and_bind` 或 `bind_existing`。
    pub action: String,
    /// Editable native VMM project name used by `create_and_bind`.
    /// `create_and_bind` 使用的可编辑原生 VMM 项目名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// Existing native VMM project id used by `bind_existing`.
    /// `bind_existing` 使用的已有原生 VMM 项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_project_id: Option<u64>,
}

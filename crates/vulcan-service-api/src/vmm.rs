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

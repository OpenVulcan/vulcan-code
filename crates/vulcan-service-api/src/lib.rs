//! Shared service API types for Vulcan clients.
//! Vulcan 客户端共享的服务 API 类型。

use std::{
    fs,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD;
use http_range::HttpRange;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Maximum bytes returned for one bounded media range response.
/// 单个有界媒体 Range 响应返回的最大字节数。
pub const SERVICE_MEDIA_RANGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Redacted secret placeholder returned through browser-facing HTTP snapshots.
/// 通过浏览器侧 HTTP 快照返回的敏感信息脱敏占位符。
pub const WEBUI_REDACTED_SECRET_PLACEHOLDER: &str = "********";

/// Default local port used by the browser-facing WebUI service.
/// 浏览器侧 WebUI 服务使用的默认本地端口。
pub const WEBUI_SERVICE_HTTP_PORT: u16 = 17891;

/// Last local port tried by automatic browser-facing service discovery.
/// 浏览器侧服务自动发现尝试的最后一个本地端口。
pub const WEBUI_SERVICE_HTTP_PORT_END: u16 = 17920;

/// Environment variable name used to override the browser-facing service port.
/// 用于覆盖浏览器侧服务端口的环境变量名。
pub const WEBUI_SERVICE_PORT_ENV: &str = "VULCAN_WEBUI_SERVICE_PORT";

/// Loopback host used by the browser-facing WebUI service.
/// 浏览器侧 WebUI 服务使用的回环主机。
pub const WEBUI_SERVICE_LOOPBACK_HOST: &str = "127.0.0.1";

/// Environment variable name used to override the browser-facing WebUI static directory.
/// 用于覆盖浏览器侧 WebUI 静态目录的环境变量名。
pub const WEBUI_STATIC_DIR_ENV: &str = "VULCAN_WEBUI_STATIC_DIR";

/// Describes one startup step state exposed by the Core Service.
/// 描述 Core Service 对外暴露的单个启动步骤状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupStepSnapshot {
    /// Stable startup step identifier used by clients to merge updates.
    /// 客户端用于合并更新的稳定启动步骤标识。
    pub id: String,
    /// User-facing startup step label.
    /// 面向用户显示的启动步骤名称。
    pub label: String,
    /// Current startup step status.
    /// 当前启动步骤状态。
    pub status: StartupStepStatus,
    /// Optional detailed startup step message.
    /// 可选的启动步骤详细消息。
    pub message: Option<String>,
    /// Optional timestamp for the most recent status update.
    /// 最近一次状态更新的可选时间戳。
    pub updated_at: Option<OffsetDateTime>,
}

/// Represents the lifecycle status of one startup step.
/// 表示单个启动步骤的生命周期状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum StartupStepStatus {
    /// The startup step has not started yet.
    /// 启动步骤尚未开始。
    Pending,
    /// The startup step is currently running.
    /// 启动步骤正在运行。
    Running,
    /// The startup step completed successfully.
    /// 启动步骤已成功完成。
    Ready,
    /// The startup step completed with a warning.
    /// 启动步骤已完成但存在警告。
    Warning,
    /// The startup step failed.
    /// 启动步骤执行失败。
    Error,
}

/// Provides a complete startup snapshot for the WebUI.
/// 为 WebUI 提供完整启动快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupSnapshot {
    /// Overall service readiness flag.
    /// 服务整体是否就绪。
    pub ready: bool,
    /// Current service startup phase label.
    /// 当前服务启动阶段标签。
    pub phase: String,
    /// Ordered startup steps displayed by clients.
    /// 客户端展示的有序启动步骤。
    pub steps: Vec<StartupStepSnapshot>,
    /// Recent startup log lines.
    /// 最近启动日志行。
    pub recent_logs: Vec<String>,
}

/// Summarizes one runtime module shown in the startup and status panels.
/// 汇总启动与状态面板中展示的单个运行时模块。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModuleSnapshot {
    /// Stable module identifier.
    /// 稳定模块标识。
    pub id: String,
    /// User-facing module label.
    /// 面向用户显示的模块名称。
    pub label: String,
    /// Current module status.
    /// 当前模块状态。
    pub status: StartupStepStatus,
    /// Short module status message.
    /// 简短模块状态消息。
    pub message: String,
}

/// Provides the first WebUI workbench snapshot.
/// 提供第一版 WebUI 工作台快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchSnapshot {
    /// Active application version.
    /// 当前应用版本。
    pub app_version: String,
    /// Runtime modules shown by the dashboard.
    /// 仪表盘展示的运行时模块。
    pub modules: Vec<RuntimeModuleSnapshot>,
    /// Project summaries shown in the sidebar.
    /// 侧栏展示的项目摘要。
    pub projects: Vec<ProjectSummary>,
    /// Expanded project identifiers shown in the sidebar tree.
    /// 侧栏树中当前展开的项目标识集合。
    #[serde(default)]
    pub expanded_project_ids: Vec<String>,
    /// Conversation summaries shown in the sidebar.
    /// 侧栏展示的会话摘要。
    pub conversations: Vec<ConversationSummary>,
    /// Archived project summaries available for restore surfaces.
    /// 可用于恢复界面的归档项目摘要。
    #[serde(default)]
    pub archived_projects: Vec<ProjectSummary>,
    /// Archived conversation summaries available for restore surfaces.
    /// 可用于恢复界面的归档会话摘要。
    #[serde(default)]
    pub archived_conversations: Vec<ConversationSummary>,
    /// Currently selected conversation identifier.
    /// 当前选中的会话标识。
    pub selected_conversation_id: Option<String>,
    /// Conversation messages shown in the main timeline.
    /// 主时间线展示的会话消息。
    pub messages: Vec<ConversationMessage>,
    /// Hydrated conversation body windows retained by the service.
    /// 服务端已保留的会话正文窗口。
    #[serde(default)]
    pub hydrated_conversation_windows: Vec<HydratedConversationWindowSummary>,
    /// Current lazy conversation body hydration progress.
    /// 当前懒加载会话正文水合进度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_hydration: Option<ConversationHydrationProgress>,
    /// Subagent navigation projected for the currently selected parent conversation.
    /// 为当前选中父会话投影出的子代理导航状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_navigation: Option<SubagentNavigationSnapshot>,
    /// Tool calls shown in the tool panel.
    /// 工具面板展示的工具调用。
    pub tool_calls: Vec<ToolCallSummary>,
    /// Feature areas shown by the main navigation.
    /// 主导航展示的功能区域。
    pub feature_areas: Vec<FeatureAreaSummary>,
    /// Plugin summaries shown in the plugin center.
    /// 插件中心展示的插件摘要。
    pub plugins: Vec<PluginSummary>,
    /// Lua Skills catalog shown in the skill management area.
    /// 技能管理区域展示的 Lua Skills 目录。
    #[serde(default)]
    pub lua_skills: LuaSkillsCatalogSummary,
    /// Agent Skills catalog shown in the skill management area.
    /// 技能管理区域展示的 Agent Skills 目录。
    #[serde(default)]
    pub agent_skills: AgentSkillsCatalogSummary,
    /// Setting sections shown in the settings area.
    /// 设置区域展示的设置分组。
    pub settings: Vec<SettingSectionSummary>,
    /// Provider summaries shown in the provider area.
    /// Provider 区域展示的供应商摘要。
    pub providers: Vec<ProviderSummary>,
    /// MCP server summaries shown in the settings area.
    /// 设置区域展示的 MCP 服务器摘要。
    #[serde(default)]
    pub mcp_servers: Vec<McpServerSummary>,
    /// Reasoning selector view resolved from the active provider and model.
    /// 根据当前激活 Provider 与模型解析出的思考选择器视图。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_selection: Option<ReasoningSelectionSummary>,
    /// Git commit dialog settings resolved from the shared configuration file.
    /// 从共享配置文件解析出的 Git 提交弹窗设置。
    #[serde(default)]
    pub git_commit: GitCommitSettingsSummary,
    /// AI capability route summaries shown by the composer quick controls.
    /// 输入区快捷控制展示的 AI 能力路由摘要。
    #[serde(default)]
    pub ai_capabilities: Vec<AiCapabilitySummary>,
    /// Default AI capability route summaries used to reset one session draft.
    /// 用于重置单个会话草稿的默认 AI 能力路由摘要。
    #[serde(default)]
    pub default_ai_capabilities: Vec<AiCapabilitySummary>,
    /// Shared AI capability vendor summaries shown in settings.
    /// 设置中展示的共享 AI 能力供应商摘要。
    #[serde(default)]
    pub ai_capability_vendors: Vec<AiCapabilityVendorSummary>,
    /// Shared AI capability vendor catalog choices shown by the add-vendor picker.
    /// 添加供应商选择器展示的共享 AI 能力供应商目录选项。
    #[serde(default)]
    pub ai_capability_vendor_catalog: Vec<AiCapabilityVendorCatalogSummary>,
    /// Composer attachment capability gates resolved by the shared runtime.
    /// 由共享运行时解析出的输入区附件能力门控。
    #[serde(default)]
    pub composer_attachment_capabilities: Vec<ComposerAttachmentCapabilitySummary>,
    /// Composer slash commands projected from the shared command catalog.
    /// 从共享指令目录投影出的输入区 slash 指令。
    #[serde(default)]
    pub composer_slash_commands: Vec<ComposerSlashCommandSummary>,
    /// Visible pending prompts retained for the selected conversation.
    /// 当前选中会话保留的可见待发送提示词。
    #[serde(default)]
    pub pending_prompts: Vec<PendingPromptSummary>,
    /// Oldest pending ask-tool question waiting for user input.
    /// 当前等待用户输入的最早 ask 工具问题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<PendingQuestionSnapshot>,
    /// Current session todolist shown in the conversation tool panel.
    /// 当前会话工具面板展示的会话 todolist。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_todolist: Option<SessionTodolistSummary>,
    /// Current session goal shown above the composer when present.
    /// 当前会话存在时显示在输入区上方的会话目标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_goal: Option<SessionGoalSummary>,
    /// Current session Plan mode state shown in the composer controls.
    /// 当前会话输入区控制项展示的计划模式状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_plan_mode: Option<SessionPlanModeSummary>,
    /// Memory summaries shown in the VMM area.
    /// VMM 区域展示的记忆摘要。
    pub memories: Vec<MemorySummary>,
    /// Detailed VMM memory management snapshot shown in the VMM area.
    /// VMM 区域展示的详细记忆管理快照。
    #[serde(default)]
    pub vmm_memory: VmmMemorySnapshot,
    /// Command sessions shown in the command area.
    /// 命令区域展示的命令会话。
    pub command_sessions: Vec<CommandSessionSummary>,
    /// Log entries shown in the diagnostics area.
    /// 诊断区域展示的日志条目。
    pub logs: Vec<LogEntrySummary>,
    /// Context injection cards projected from the selected session runtime state.
    /// 从当前选中会话运行态投影出的上下文注入卡片。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_injection: Option<ContextInjectionSnapshot>,
    /// Context token usage snapshot projected from the latest execution preparation.
    /// 从最近一次执行准备投影出的上下文 Token 占用快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsageSnapshot>,
}

/// Describes Git commit dialog settings projected to WebUI clients.
/// 描述投影给 WebUI 客户端的 Git 提交弹窗设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitSettingsSummary {
    /// Default namespace prefix used when the current branch has no namespace.
    /// 当前分支没有命名空间时使用的默认命名空间前缀。
    pub new_branch_prefix: String,
    /// Model selection used when commit messages are generated automatically.
    /// 自动生成提交信息时使用的模型选择。
    #[serde(default)]
    pub summary_model: GitCommitSummaryModelSelectionSummary,
    /// User-authored extra prompt appended to commit-message generation.
    /// 追加到提交信息生成中的用户额外提示词。
    #[serde(default)]
    pub summary_extra_prompt: String,
}

impl Default for GitCommitSettingsSummary {
    /// Builds the WebUI default Git commit settings summary.
    /// 构建 WebUI 默认 Git 提交设置摘要。
    fn default() -> Self {
        Self {
            new_branch_prefix: String::from("vulcan/"),
            summary_model: GitCommitSummaryModelSelectionSummary::default(),
            summary_extra_prompt: String::new(),
        }
    }
}

/// Describes the model selection used for automatic Git commit summaries.
/// 描述自动 Git 提交总结使用的模型选择。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GitCommitSummaryModelSelectionSummary {
    /// Follow the model selected by the session that owns the commit action.
    /// 跟随发起提交动作的会话当前模型。
    Session,
    /// Always use one fixed provider/model pair.
    /// 始终使用一组固定 Provider/模型。
    Fixed {
        /// Stable provider identifier.
        /// 稳定 Provider 标识。
        provider_id: String,
        /// Stable model identifier.
        /// 稳定模型标识。
        model_id: String,
    },
}

impl Default for GitCommitSummaryModelSelectionSummary {
    /// Builds the default session-following summary model selection.
    /// 构建默认的跟随会话总结模型选择。
    fn default() -> Self {
        Self::Session
    }
}

/// Describes one retained lazy conversation body window.
/// 描述一个已保留的懒加载会话正文窗口。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HydratedConversationWindowSummary {
    /// Conversation identifier owning this retained body window.
    /// 拥有该保留正文窗口的会话标识。
    pub session_id: String,
    /// Earliest loaded transcript ordinal used to request older history.
    /// 用于继续请求更早历史的已加载最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether older transcript turns still exist before this window.
    /// 当前窗口之前是否仍存在更早的转录轮次。
    pub has_more_history: bool,
}

/// Describes lazy loading progress for one conversation transcript body.
/// 描述单个会话正文懒加载进度。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHydrationProgress {
    /// Conversation identifier being loaded.
    /// 正在加载的会话标识。
    pub session_id: String,
    /// Current loading status key.
    /// 当前加载状态键。
    pub status: ConversationHydrationStatus,
    /// Current phase key used by the WebUI i18n layer.
    /// WebUI 多语言层使用的当前阶段键。
    pub phase: String,
    /// Human-readable fallback message.
    /// 面向用户的回退消息。
    pub message: String,
    /// Completed progress percentage from 0 to 100.
    /// 0 到 100 的已完成百分比。
    pub percent: u8,
}

/// Stable lazy conversation hydration status values.
/// 稳定的懒加载会话正文水合状态值。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConversationHydrationStatus {
    /// The conversation body is being loaded.
    /// 会话正文正在加载。
    Loading,
    /// The conversation body has been loaded.
    /// 会话正文已经载入。
    Ready,
    /// The conversation body failed to load.
    /// 会话正文载入失败。
    Error,
}

/// Response returned by a lazy conversation body load request.
/// 懒加载会话正文请求返回的响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessagesLoadResponse {
    /// Complete workbench snapshot rebuilt with the hydrated selected conversation.
    /// 使用已水合选中会话重建出的完整工作台快照。
    pub workbench: Box<WorkbenchSnapshot>,
    /// Loaded conversation messages.
    /// 已加载的会话消息。
    pub messages: Vec<ConversationMessage>,
    /// Earliest loaded transcript ordinal used to request older history.
    /// 用于继续请求更早历史的已加载最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether older transcript turns still exist before this window.
    /// 当前窗口之前是否仍存在更早的转录轮次。
    pub has_more_history: bool,
    /// Final loading progress.
    /// 最终加载进度。
    pub progress: ConversationHydrationProgress,
}

/// Response returned by one older conversation history page request.
/// 单次更早会话历史页请求返回的响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHistoryPageResponse {
    /// Conversation identifier owning this page.
    /// 拥有该页历史的会话标识。
    pub session_id: String,
    /// Complete workbench snapshot after merging this older page.
    /// 合并该页更早历史后的完整工作台快照。
    pub workbench: Box<WorkbenchSnapshot>,
    /// Older conversation messages in ascending display order.
    /// 按展示正序排列的更早会话消息。
    pub messages: Vec<ConversationMessage>,
    /// Earliest loaded transcript ordinal after this page.
    /// 加载该页后已加载的最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether more older transcript turns still exist.
    /// 是否仍存在更多更早的转录轮次。
    pub has_more_history: bool,
}

/// Full WebUI summary for the embedded Lua Skills catalog.
/// 嵌入式 Lua Skills 目录的完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCatalogSummary {
    /// Runtime root used by the embedded Lua Skills host.
    /// 嵌入式 Lua Skills 宿主使用的运行时根目录。
    pub runtime_root: String,
    /// System Lua library directory used by Lua Skills.
    /// Lua Skills 使用的系统 Lua 库目录。
    pub system_lua_lib: String,
    /// Total visible runtime entry count.
    /// 可见运行时入口总数。
    pub entry_count: usize,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Skill package rows visible to the host.
    /// 宿主当前可见的技能包行。
    pub skills: Vec<LuaSkillSummary>,
}

/// One Lua Skill package row projected for the WebUI.
/// 投影给 WebUI 的单个 Lua Skill 包行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillSummary {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Skill package version.
    /// 技能包版本。
    pub version: String,
    /// Runtime root layer name that owns this effective skill.
    /// 拥有该生效技能的运行时根层名称。
    pub root_name: String,
    /// Filesystem directory that contains this skill.
    /// 包含该技能的文件系统目录。
    pub skill_dir: String,
    /// Whether this skill is currently enabled.
    /// 该技能当前是否已启用。
    pub enabled: bool,
    /// Optional disabled reason from the lifecycle record.
    /// 生命周期记录中的可选停用原因。
    pub disabled_reason: Option<String>,
    /// Whether this skill has a managed install record.
    /// 该技能是否拥有受管安装记录。
    pub managed: bool,
    /// Optional install source type label.
    /// 可选安装来源类型标签。
    pub source_type: Option<String>,
    /// Optional install source locator.
    /// 可选安装来源定位值。
    pub source_locator: Option<String>,
    /// Number of visible runtime entries owned by this skill.
    /// 该技能拥有的可见运行时入口数量。
    pub entry_count: usize,
    /// Whether this skill belongs to the host-owned ROOT layer.
    /// 该技能是否属于宿主自有 ROOT 层。
    pub system_owned: bool,
    /// Tool rows exposed by this skill.
    /// 该技能暴露的工具行。
    pub tools: Vec<LuaSkillToolSummary>,
}

/// One Lua Skill runtime tool row with registration state.
/// 带注册状态的单个 Lua Skill 运行时工具行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillToolSummary {
    /// Canonical host-visible tool name.
    /// 宿主可见的规范工具名称。
    pub canonical_name: String,
    /// Local tool name inside the skill package.
    /// 技能包内部的本地工具名称。
    pub local_name: String,
    /// Tool description from the runtime entry descriptor.
    /// 来自运行时入口描述符的工具说明。
    pub description: String,
    /// Persisted registration state key.
    /// 持久化注册状态键。
    pub registration_state: String,
    /// Human-readable registration state label.
    /// 人类可读的注册状态标签。
    pub registration_label: String,
    /// Short help text for the current registration state.
    /// 当前注册状态的简短说明。
    pub registration_help: String,
}

/// Full WebUI summary for prompt-based Agent Skills.
/// 基于提示词的 Agent Skills 完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillsCatalogSummary {
    /// Whether Agent Skills are globally enabled.
    /// Agent Skills 是否已全局启用。
    pub global_enabled: bool,
    /// System skill root used by host-managed Agent Skills.
    /// 宿主管理 Agent Skills 使用的系统技能根目录。
    pub system_root: String,
    /// User skill root used by user-installed Agent Skills.
    /// 用户安装 Agent Skills 使用的用户技能根目录。
    pub user_root: String,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Parsed Agent Skill rows.
    /// 已解析的 Agent Skill 行。
    pub skills: Vec<AgentSkillSummary>,
    /// Non-fatal loading issues.
    /// 非致命加载问题。
    pub errors: Vec<AgentSkillLoadIssueSummary>,
}

/// One prompt-based Agent Skill row projected for the WebUI.
/// 投影给 WebUI 的单个提示词 Agent Skill 行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillSummary {
    /// Stable skill name from frontmatter.
    /// 来自 frontmatter 的稳定技能名称。
    pub name: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Human-facing short description.
    /// 面向用户的简短说明。
    pub description: String,
    /// Source scope label.
    /// 来源作用域标签。
    pub scope: String,
    /// Whether this Agent Skill is enabled after global configuration is applied.
    /// 应用全局配置后该 Agent Skill 是否启用。
    pub enabled: bool,
    /// Filesystem path to the SKILL.md file.
    /// 指向 SKILL.md 文件的文件系统路径。
    pub skill_file: String,
    /// Directory that owns this Agent Skill and bundled resources.
    /// 拥有该 Agent Skill 与附带资源的目录。
    pub base_dir: String,
    /// Optional resolved icon path that the WebUI can render.
    /// WebUI 可渲染的可选解析图标路径。
    pub icon_path: Option<String>,
    /// Markdown body without YAML frontmatter.
    /// 去除 YAML frontmatter 后的 Markdown 正文。
    pub body: String,
}

/// One Agent Skill loading issue projected for diagnostics.
/// 投影给诊断界面的单个 Agent Skill 加载问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillLoadIssueSummary {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: String,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
}

/// Describes one pending ask-tool request mirrored into the WebUI.
/// 描述镜像到 WebUI 的单个待处理 ask 工具请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionSnapshot {
    /// Stable request identifier used by reply and reject commands.
    /// 回复与拒绝命令使用的稳定请求标识。
    pub id: String,
    /// Controller session that owns the blocked ask tool call.
    /// 拥有阻塞 ask 工具调用的控制器会话。
    pub session_id: String,
    /// Monotonic runtime sequence used to order concurrent asks.
    /// 用于排序并发 ask 请求的运行时单调序号。
    pub sequence: u64,
    /// Ordered prompts shown in the modal.
    /// 在弹窗中按顺序展示的问题提示。
    pub questions: Vec<PendingQuestionPromptSummary>,
}

/// Describes one prompt inside a pending ask-tool request.
/// 描述待处理 ask 工具请求中的单条问题提示。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionPromptSummary {
    /// Full question text shown to the user.
    /// 展示给用户的完整问题文本。
    pub question: String,
    /// Short field header shown above the question.
    /// 展示在问题上方的简短字段标题。
    pub header: String,
    /// Selectable choices offered by the prompt.
    /// 该问题提供的可选答案。
    #[serde(default)]
    pub options: Vec<PendingQuestionOptionSummary>,
    /// Whether multiple structured choices may be selected together.
    /// 是否允许同时选择多个结构化选项。
    pub multiple: bool,
}

/// Describes one selectable choice inside an ask-tool prompt.
/// 描述 ask 工具问题中的单个可选答案。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionOptionSummary {
    /// Choice label returned to the model when selected.
    /// 被选中时回传给模型的选项标签。
    pub label: String,
    /// Helper text explaining the choice.
    /// 解释该选项的辅助说明。
    pub description: String,
}

/// Describes the current model-authored todolist for one selected session.
/// 描述当前选中会话的模型编写 todolist。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistSummary {
    /// Optional explanation attached to the latest todolist update.
    /// 最近一次 todolist 更新附带的可选说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// Ordered todolist items shown in the session tool panel.
    /// 会话工具面板中展示的有序 todolist 条目。
    #[serde(default)]
    pub items: Vec<SessionTodolistItemSummary>,
    /// Completed item count.
    /// 已完成条目数量。
    pub completed_count: usize,
    /// Total item count.
    /// 条目总数量。
    pub total_count: usize,
    /// Unix-millis update timestamp text.
    /// Unix 毫秒格式的更新时间文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Describes one todolist item for WebUI rendering.
/// 描述供 WebUI 渲染的单个 todolist条目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistItemSummary {
    /// Human-readable step text supplied by the model.
    /// 模型提供的人类可读步骤文本。
    pub step: String,
    /// Stable status key: pending, in_progress, or completed.
    /// 稳定状态键：pending、in_progress 或 completed。
    pub status: String,
}

/// Describes the current session Plan mode state for WebUI rendering.
/// 描述供 WebUI 渲染的当前会话计划模式状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanModeSummary {
    /// Stable execution mode key such as `build` or `plan`.
    /// 稳定执行模式键，例如 `build` 或 `plan`。
    pub execution_mode: String,
    /// Whether Plan mode steering has already been injected for this activation.
    /// 当前计划模式激活期间是否已经注入过计划模式引导。
    pub prompt_injected: bool,
    /// Latest generated plan artifact, if one exists.
    /// 最近生成的计划产物，如果存在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<SessionPlanArtifactSummary>,
}

/// Describes one generated Plan mode artifact for WebUI rendering.
/// 描述供 WebUI 渲染的单个计划模式产物。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanArtifactSummary {
    /// Human-readable plan name supplied by the model.
    /// 模型提供的人类可读计划名称。
    pub name: String,
    /// Absolute filesystem path of the generated Markdown plan file.
    /// 生成的 Markdown 计划文件绝对路径。
    pub path: String,
    /// Full Markdown plan content captured at submit time.
    /// 提交时捕获的完整 Markdown 计划内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Unix-millis text capturing when the artifact was written.
    /// 捕获产物写入时间的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Describes the current long-running goal for one selected session.
/// 描述当前选中会话的长生命周期目标。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGoalSummary {
    /// Stable session identifier that owns this goal.
    /// 拥有该目标的稳定会话标识。
    pub session_id: String,
    /// Stable goal version identifier used for stale update protection.
    /// 用于过期更新保护的稳定目标版本标识。
    pub goal_id: String,
    /// Human-readable goal objective.
    /// 人类可读的目标内容。
    pub objective: String,
    /// Stable status key: active, paused, blocked, usage_limited, budget_limited, or complete.
    /// 稳定状态键：active、paused、blocked、usage_limited、budget_limited 或 complete。
    pub status: String,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    /// Accounted token usage.
    /// 已记账 token 用量。
    pub tokens_used: i64,
    /// Live usage total displayed by the composer strip.
    /// 输入区目标条展示的实时用量总数。
    pub usage_tokens: i64,
    /// Provider API calls observed for the current goal turn.
    /// 当前目标轮次观测到的 Provider API 调用次数。
    pub api_call_count: usize,
    /// Tool calls observed for the current goal turn.
    /// 当前目标轮次观测到的工具调用次数。
    pub tool_call_count: usize,
    /// Input tokens observed for the current goal turn.
    /// 当前目标轮次观测到的输入 Token 数。
    pub input_tokens: i64,
    /// Cache-hit input tokens observed for the current goal turn.
    /// 当前目标轮次观测到的缓存命中输入 Token 数。
    pub cached_input_tokens: i64,
    /// Output tokens observed for the current goal turn.
    /// 当前目标轮次观测到的输出 Token 数。
    pub output_tokens: i64,
    /// Total provider tokens observed for the current goal turn.
    /// 当前目标轮次观测到的 Provider 总 Token 数。
    pub total_tokens: i64,
    /// Accounted wall-clock usage in seconds.
    /// 已记账墙钟用量秒数。
    pub time_used_seconds: i64,
    /// Creation timestamp in Unix epoch milliseconds.
    /// Unix 纪元毫秒格式的创建时间。
    pub created_at_ms: i64,
    /// Last update timestamp in Unix epoch milliseconds.
    /// Unix 纪元毫秒格式的最近更新时间。
    pub updated_at_ms: i64,
}

/// Describes the subagent navigation surface for one selected parent conversation.
/// 描述单个已选父会话的子代理导航界面状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentNavigationSnapshot {
    /// Parent conversation identifier that owns the subagent task cards.
    /// 拥有子代理任务卡片的父会话标识。
    pub parent_conversation_id: String,
    /// Conversation identifier currently rendered in the transcript surface.
    /// 当前渲染在转录界面中的会话标识。
    pub active_conversation_id: String,
    /// Active child task identifier when the subagent surface is open.
    /// 子代理界面打开时的活动子任务标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<String>,
    /// Ordered subagent task entries shown in the navigation control.
    /// 导航控件中展示的有序子代理任务条目。
    #[serde(default)]
    pub tasks: Vec<SubagentTaskSummary>,
}

/// Summarizes one subagent task entry for WebUI rendering.
/// 汇总一条供 WebUI 渲染的子代理任务条目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTaskSummary {
    /// Stable child task conversation identifier.
    /// 稳定的子任务会话标识。
    pub task_id: String,
    /// Caller-facing task identifier preserved in task-session metadata.
    /// 保留在 task-session 元数据中的调用方面向任务标识。
    pub public_task_id: String,
    /// Human-facing label built by the shared subagent projection.
    /// 由共享子代理投影构建的人类可读标签。
    pub label: String,
    /// Optional task description supplied to the subagent.
    /// 传给子代理的可选任务描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Canonical lowercase run state such as running, completed, stopped, or failed.
    /// 规范小写运行状态，例如 running、completed、stopped 或 failed。
    pub run_state: String,
    /// Whether this task is actively running according to the shared task runtime.
    /// 根据共享任务运行时判断该任务是否正在活动运行。
    pub is_running: bool,
    /// Whether this task is the currently opened subagent surface.
    /// 该任务是否为当前打开的子代理界面。
    pub is_active: bool,
}

/// Describes one visible pending prompt retained above the composer.
/// 描述输入区上方保留的单条可见待发送提示词。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPromptSummary {
    /// Stable entry identifier used for editing, removal and drag sorting.
    /// 用于编辑、移除与拖拽排序的稳定条目标识。
    pub id: String,
    /// Full prompt text restored when the entry is edited.
    /// 编辑条目时恢复的完整提示词文本。
    pub prompt: String,
    /// Compact preview rendered in the queue strip.
    /// 渲染在队列条中的紧凑预览。
    pub preview: String,
    /// Attachments that will be replayed with the queued prompt.
    /// 将随排队提示词一起回放的附件。
    #[serde(default)]
    pub attachments: Vec<ComposerAttachmentSummary>,
    /// Stable queue state key.
    /// 稳定队列状态键。
    pub state: String,
    /// Unix-millis creation timestamp text.
    /// Unix 毫秒格式的创建时间文本。
    pub created_at: String,
}

/// Describes one slash command visible to composer clients.
/// 描述输入区客户端可见的单个 slash 指令。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSlashCommandSummary {
    /// Stable command identifier used by routers.
    /// 路由使用的稳定指令标识。
    pub id: String,
    /// Canonical English slash trigger shown to users.
    /// 展示给用户的规范英文 slash 触发词。
    pub command: String,
    /// Chinese command description.
    /// 中文指令说明。
    pub description_zh: String,
    /// English command description.
    /// 英文指令说明。
    pub description_en: String,
}

/// Returns every composer slash command in the same stable order as the native workbench.
/// 按原生工作台相同的稳定顺序返回全部输入区 slash 指令。
pub fn composer_slash_command_catalog() -> Vec<ComposerSlashCommandSummary> {
    vec![
        ComposerSlashCommandSummary {
            id: String::from("model"),
            command: String::from("/model"),
            description_zh: String::from("选择当前模型"),
            description_en: String::from("Choose the current model"),
        },
        ComposerSlashCommandSummary {
            id: String::from("compact"),
            command: String::from("/compact"),
            description_zh: String::from("立即压缩当前会话上下文"),
            description_en: String::from("Compact the current conversation context"),
        },
        ComposerSlashCommandSummary {
            id: String::from("reasoning"),
            command: String::from("/reasoning"),
            description_zh: String::from("选择当前思考级别"),
            description_en: String::from("Choose the current reasoning effort"),
        },
        ComposerSlashCommandSummary {
            id: String::from("plan"),
            command: String::from("/plan"),
            description_zh: String::from("切换当前会话计划模式"),
            description_en: String::from("Toggle Plan mode for the current conversation"),
        },
        ComposerSlashCommandSummary {
            id: String::from("skill"),
            command: String::from("/skill"),
            description_zh: String::from("插入提示词技能加载指令"),
            description_en: String::from("Insert a prompt-skill loading instruction"),
        },
        ComposerSlashCommandSummary {
            id: String::from("skills"),
            command: String::from("/skills"),
            description_zh: String::from("打开提示词技能面板"),
            description_en: String::from("Open the prompt skills panel"),
        },
    ]
}

/// Summarizes the dynamic and static context injected into the selected session.
/// 汇总当前选中会话中的动态与静态上下文注入信息。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionSnapshot {
    /// Dynamic per-turn injection cards.
    /// 按轮次生效的动态注入卡片。
    pub dynamic_cards: Vec<ContextInjectionCardSummary>,
    /// Structured active VMM profile snapshot for static memory display.
    /// 用于静态记忆展示的结构化 active VMM 画像快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_profile: Option<VmmStaticProfileSummary>,
    /// Static persistent injection cards.
    /// 持续生效的静态注入卡片。
    pub static_cards: Vec<ContextInjectionCardSummary>,
}

/// Summarizes the current active VMM profile nodes.
/// 汇总当前 active VMM 画像节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileSummary {
    /// Unix-millis text captured when the profile snapshot was updated.
    /// 画像快照更新时捕获的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Profile sections grouped by VMM target scope.
    /// 按 VMM 目标作用域分组的画像段落。
    pub sections: Vec<VmmStaticProfileSectionSummary>,
}

/// Summarizes one VMM profile target section.
/// 汇总一个 VMM 画像目标段落。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileSectionSummary {
    /// Stable target key such as team, space, project, or user.
    /// 稳定目标键，例如 team、space、project 或 user。
    pub target: String,
    /// Uppercase target label displayed by the panel.
    /// 面板展示的大写目标标签。
    pub label: String,
    /// Date groups under this target.
    /// 当前目标下的日期分组。
    pub dates: Vec<VmmStaticProfileDateGroupSummary>,
}

/// Summarizes one date group inside a VMM profile section.
/// 汇总 VMM 画像段落内的单个日期分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileDateGroupSummary {
    /// Human-readable profile date.
    /// 人类可读画像日期。
    pub date: String,
    /// Active profile nodes for this date.
    /// 当前日期下的 active 画像节点。
    pub nodes: Vec<VmmStaticProfileNodeSummary>,
}

/// Summarizes one active VMM profile node for UI display.
/// 汇总单个用于 UI 展示的 active VMM 画像节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileNodeSummary {
    /// Durable VMM profile node id.
    /// 长期 VMM 画像节点标识。
    pub id: u64,
    /// Priority marker such as P1.
    /// 优先级标记，例如 P1。
    pub priority: String,
    /// Lifetime marker such as L2.
    /// 生命周期标记，例如 L2。
    pub level: String,
    /// Refresh weight marker such as W3.
    /// 刷新权重标记，例如 W3。
    pub refresh_weight: String,
    /// Atomic profile content.
    /// 原子化画像内容。
    pub content: String,
    /// Source kind key returned by VMM.
    /// VMM 返回的来源类型键。
    pub source_kind: String,
    /// Source row id such as the originating turn id.
    /// 来源行标识，例如来源回合标识。
    pub source_id: u64,
    /// Bound user/project/team/space id.
    /// 绑定的 user/project/team/space 标识。
    pub bind_id: u64,
}

/// Summarizes one context injection source group.
/// 汇总单个上下文注入来源分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionCardSummary {
    /// Stable context injection tag.
    /// 稳定的上下文注入标记。
    pub tag: String,
    /// Human-readable card description.
    /// 面向用户的卡片说明。
    pub description: String,
    /// Items emitted by this injection source.
    /// 当前注入来源产生的条目。
    pub items: Vec<ContextInjectionItemSummary>,
}

/// Summarizes one visible context injection item.
/// 汇总单条可见上下文注入内容。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionItemSummary {
    /// Compact content preview displayed in the side panel.
    /// 侧面板展示的紧凑内容预览。
    pub content: String,
    /// Full detail text shown on hover or expansion.
    /// 悬停或展开时展示的完整详情文本。
    pub detail: String,
    /// Optional turn label that owns this injection.
    /// 拥有该注入的可选轮次标签。
    pub turn_label: Option<String>,
    /// TTL or persistence label.
    /// TTL 或持久化状态标签。
    pub ttl_label: String,
    /// Optional source display label.
    /// 可选来源显示标签。
    pub source_display: Option<String>,
    /// Optional stable injection dedupe key for current-session suppression.
    /// 用于当前会话抑制的可选稳定注入去重键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    /// Optional durable VMM memory id when this item comes from a memory row.
    /// 当条目来自长期 VMM 记忆行时对应的可选记忆标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<u64>,
    /// Optional source turn id when the backing dialogue can be queried.
    /// 当背后对话可查询时对应的可选来源回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<u64>,
    /// Optional relevance score label.
    /// 可选相关性评分标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_label: Option<String>,
    /// Whether the item can query full turn detail.
    /// 条目是否可以查询完整回合详情。
    #[serde(default)]
    pub has_dialogue: bool,
    /// Optional VMM creation datetime.
    /// 可选 VMM 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_datetime: Option<String>,
    /// Optional injection update timestamp.
    /// 可选注入更新时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Summarizes context token usage for the selected session composer.
/// 汇总当前选中会话输入区的上下文 Token 占用。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageSnapshot {
    /// Rounded usage percent shown inside the orb.
    /// 圆盘中显示的四舍五入占用百分比。
    pub percent: u8,
    /// Severity key used by WebUI styling.
    /// WebUI 样式使用的严重级别键。
    pub severity: String,
    /// Whether the active runtime still matches the accepted baseline.
    /// 当前运行时是否仍匹配已接受的基线。
    pub baseline_matches_active_runtime: bool,
    /// Tooltip title.
    /// 提示浮层标题。
    pub tooltip_title: String,
    /// Tooltip diagnostic rows.
    /// 提示浮层诊断行。
    pub tooltip_rows: Vec<String>,
}

/// Summarizes one semantic AI capability for the WebUI composer.
/// 汇总 WebUI 输入区中的单个语义 AI 能力。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilitySummary {
    /// Stable semantic capability key.
    /// 稳定语义能力键。
    pub kind: String,
    /// User-facing semantic capability label.
    /// 面向用户显示的语义能力名称。
    pub label: String,
    /// Whether the capability has at least one usable route.
    /// 当前能力是否至少拥有一条可用路由。
    pub available: bool,
    /// Whether the capability is globally enabled in the workbench configuration.
    /// 当前能力是否在工作台配置中全局启用。
    pub enabled: bool,
    /// Whether the active provider/model can handle this semantic capability natively.
    /// 当前 Provider/模型是否能原生处理该语义能力。
    pub native: bool,
    /// Whether host-side AI tools can bridge this semantic capability.
    /// 宿主侧 AI 工具是否能桥接该语义能力。
    pub bridged: bool,
    /// Configured route targets under this semantic capability.
    /// 当前语义能力下已配置的路由目标。
    pub targets: Vec<AiCapabilityTargetSummary>,
}

/// Summarizes one configured AI capability route target.
/// 汇总单个已配置的 AI 能力路由目标。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityTargetSummary {
    /// Stable `vendor/target` selector used by runtime prompt overrides.
    /// 运行时提示词覆盖使用的稳定 `vendor/target` 选择器。
    pub target_selector: String,
    /// Provider or capability vendor identifier.
    /// Provider 或能力供应商标识。
    pub vendor_id: String,
    /// User-facing vendor name.
    /// 面向用户显示的供应商名称。
    pub vendor_name: String,
    /// Route target alias exposed to the runtime.
    /// 暴露给运行时的路由目标别名。
    pub target_alias: String,
    /// User-facing route target name.
    /// 面向用户显示的路由目标名称。
    pub target_name: String,
    /// Provider model identifier used by this target.
    /// 当前目标使用的 Provider 模型标识。
    pub model_id: String,
    /// Whether this target is globally enabled.
    /// 当前目标是否全局启用。
    pub enabled: bool,
}

/// Summarizes one shared AI capability vendor configuration.
/// 汇总单个共享 AI 能力供应商配置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityVendorSummary {
    /// Stable vendor instance identifier.
    /// 稳定供应商实例标识。
    pub id: String,
    /// Stable catalog vendor identifier.
    /// 稳定目录供应商标识。
    pub vendor_id: String,
    /// User-facing vendor name.
    /// 面向用户显示的供应商名称。
    pub name: String,
    /// Whether the vendor participates in runtime routing.
    /// 该供应商是否参与运行时路由。
    pub enabled: bool,
    /// Stored API key or environment placeholder.
    /// 已保存的 API Key 或环境变量占位符。
    pub api_key: String,
    /// Stored vendor base URL override.
    /// 已保存的供应商 Base URL 覆盖值。
    pub base_url: String,
    /// Stored vendor region override.
    /// 已保存的供应商区域覆盖值。
    pub region: String,
    /// Stored connection mode key.
    /// 已保存的连接模式键。
    pub connection_mode: String,
    /// Number of configured route targets under the vendor.
    /// 该供应商下已配置路由目标数量。
    pub target_count: usize,
    /// Catalog target choices exposed by this vendor.
    /// 该供应商暴露的目录目标选项。
    #[serde(default)]
    pub target_choices: Vec<AiCapabilityVendorTargetChoiceSummary>,
}

/// Summarizes one selectable AI capability target under a configured vendor.
/// 汇总已配置供应商下单个可选择的 AI 能力目标。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityVendorTargetChoiceSummary {
    /// Stable semantic capability kind owned by this target.
    /// 当前目标所属的稳定语义能力类型。
    pub capability_kind: String,
    /// Stable target alias exposed below the vendor id.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// Default provider request model attached to this target.
    /// 当前目标默认绑定的 Provider 请求模型。
    pub default_model_id: String,
    /// User-facing compact label.
    /// 面向用户的紧凑标签。
    pub label: String,
    /// Whether the target supports editing the request model.
    /// 当前目标是否支持编辑请求模型。
    pub supports_model_override: bool,
    /// Optional finite model choices.
    /// 可选的有限模型选项。
    pub model_options: Vec<String>,
}

/// Summarizes one supported AI capability vendor catalog choice.
/// 汇总单个受支持 AI 能力供应商目录选项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityVendorCatalogSummary {
    /// Stable vendor identifier.
    /// 稳定供应商标识。
    pub vendor_id: String,
    /// User-facing vendor name.
    /// 面向用户显示的供应商名称。
    pub name: String,
    /// Capability labels supported by the vendor.
    /// 该供应商支持的能力标签集合。
    pub capabilities: Vec<String>,
}

/// Summarizes one primary workbench feature area.
/// 汇总工作台中的单个主功能区域。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAreaSummary {
    /// Stable feature area identifier.
    /// 稳定功能区域标识。
    pub id: String,
    /// User-facing feature area label.
    /// 面向用户显示的功能区域名称。
    pub label: String,
    /// Short feature area description.
    /// 简短功能区域描述。
    pub description: String,
    /// Feature area status text.
    /// 功能区域状态文本。
    pub status: String,
}

/// Summarizes a project in the WebUI sidebar.
/// 汇总 WebUI 侧栏中的项目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// Stable project identifier.
    /// 稳定项目标识。
    pub id: String,
    /// Project display name.
    /// 项目显示名称。
    pub name: String,
    /// Project path shown to users.
    /// 展示给用户的项目路径。
    pub path: String,
    /// Whether the project is currently pinned.
    /// 项目当前是否处于置顶状态。
    #[serde(default)]
    pub pinned: bool,
    /// Optional icon pack used to render the project.
    /// 用于渲染项目的可选图标包。
    #[serde(default)]
    pub icon_pack: Option<String>,
    /// Optional icon key used to render the project.
    /// 用于渲染项目的可选图标键。
    #[serde(default)]
    pub icon_key: Option<String>,
}

/// Summarizes one filesystem directory row returned to WebUI folder pickers.
/// 汇总一条返回给 WebUI 文件夹选择器的文件系统目录行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryEntrySummary {
    /// Directory display name.
    /// 目录显示名称。
    pub name: String,
    /// Absolute directory path on the service host.
    /// 服务宿主上的绝对目录路径。
    pub path: String,
    /// Whether the directory name looks hidden to the current platform.
    /// 当前目录名称在平台语义下是否看起来为隐藏目录。
    pub hidden: bool,
}

/// Summarizes one selectable filesystem root returned to WebUI folder pickers.
/// 汇总一条返回给 WebUI 文件夹选择器的可选文件系统根。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryRootSummary {
    /// User-facing root label such as C: or /.
    /// 面向用户显示的根标签，例如 C: 或 /。
    pub label: String,
    /// Absolute root path on the service host.
    /// 服务宿主上的绝对根路径。
    pub path: String,
}

/// Describes one browsed filesystem directory for WebUI project selection.
/// 描述一次 WebUI 项目选择浏览到的文件系统目录。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryListing {
    /// Selectable filesystem roots available on the service host.
    /// 服务宿主上可选择的文件系统根列表。
    #[serde(default)]
    pub roots: Vec<ProjectDirectoryRootSummary>,
    /// Absolute current directory path on the service host.
    /// 服务宿主上的当前绝对目录路径。
    pub path: String,
    /// Optional parent directory path.
    /// 可选父目录路径。
    pub parent_path: Option<String>,
    /// Whether the current directory may be selected as a project.
    /// 当前目录是否可以作为项目选择。
    pub can_select: bool,
    /// Child directory rows sorted for display.
    /// 已按显示顺序排序的子目录行。
    pub entries: Vec<ProjectDirectoryEntrySummary>,
    /// Optional human-readable notice for partial listing failures.
    /// 局部读取失败时使用的可选人类可读提示。
    pub notice: Option<String>,
}

/// Describes one workspace file entry category returned to the WebUI file panel.
/// 描述返回给 WebUI 文件面板的单个工作区文件条目类别。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceFileEntryKind {
    /// Directory entry that can be expanded lazily.
    /// 可懒加载展开的目录条目。
    Directory,
    /// Text or source-code file that can be previewed as UTF-8 content.
    /// 可作为 UTF-8 内容预览的文本或源代码文件。
    Code,
    /// Image file that can be displayed through the local media route.
    /// 可通过本地媒体路由显示的图片文件。
    Image,
    /// Video file that can be displayed through the local media route.
    /// 可通过本地媒体路由显示的视频文件。
    Video,
    /// Other file type that is not previewed by the first workspace file panel.
    /// 首版工作区文件面板不预览的其他文件类型。
    Other,
}

/// Stores the active workspace file tree filtering options.
/// 保存当前工作区文件树过滤选项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileFilterOptions {
    /// Whether platform-hidden files and folders are filtered out.
    /// 是否过滤平台隐藏文件与文件夹。
    pub ignore_hidden: bool,
    /// Whether dot-prefixed file and folder names are filtered out.
    /// 是否过滤以点开头的文件与文件夹名称。
    pub ignore_dot_names: bool,
    /// Whether `.ignore` files are applied while listing the tree.
    /// 列出文件树时是否应用 `.ignore` 文件。
    pub use_ignore_files: bool,
    /// Whether Git ignore sources are applied while listing the tree.
    /// 列出文件树时是否应用 Git ignore 来源。
    pub use_gitignore: bool,
    /// Whether image files may be previewed after selection.
    /// 选中后是否允许预览图片文件。
    pub allow_images: bool,
    /// Whether video files may be previewed after selection.
    /// 选中后是否允许预览视频文件。
    pub allow_videos: bool,
}

impl Default for WorkspaceFileFilterOptions {
    /// Builds the default safe workspace file tree filtering options.
    /// 构建默认安全的工作区文件树过滤选项。
    fn default() -> Self {
        Self {
            ignore_hidden: true,
            ignore_dot_names: true,
            use_ignore_files: true,
            use_gitignore: true,
            allow_images: false,
            allow_videos: false,
        }
    }
}

/// Summarizes one file or directory row returned to the WebUI workspace file panel.
/// 汇总一条返回给 WebUI 工作区文件面板的文件或目录行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileEntrySummary {
    /// Display name of the file or directory.
    /// 文件或目录显示名称。
    pub name: String,
    /// Absolute path on the service host.
    /// 服务宿主上的绝对路径。
    pub path: String,
    /// Workspace-root-relative path using forward slashes.
    /// 使用正斜杠的工作区根相对路径。
    pub relative_path: String,
    /// Entry category used by the WebUI to pick icons and preview behavior.
    /// WebUI 用于选择图标与预览行为的条目类别。
    pub kind: WorkspaceFileEntryKind,
    /// Whether the row is a directory.
    /// 当前行是否为目录。
    pub is_directory: bool,
    /// Lowercase file extension when present.
    /// 存在时的小写文件扩展名。
    pub extension: Option<String>,
    /// Best-effort MIME type used by media previews.
    /// 媒体预览使用的尽力推断 MIME 类型。
    pub mime_type: Option<String>,
    /// File size in bytes when available.
    /// 可用时的文件字节大小。
    pub size_bytes: Option<u64>,
    /// Last modified Unix timestamp in milliseconds when available.
    /// 可用时的 Unix 毫秒级最后修改时间戳。
    pub modified_at: Option<String>,
    /// Whether the entry matched a platform-hidden or dot-name rule.
    /// 条目是否命中平台隐藏或点名称规则。
    pub hidden: bool,
    /// Whether a directory may contain children after filtering.
    /// 目录在过滤后是否可能包含子项。
    pub has_children: bool,
}

/// Describes one lazily loaded workspace file directory listing.
/// 描述一次懒加载的工作区文件目录列表。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileDirectoryListing {
    /// Absolute workspace root path used for the listing.
    /// 本次列表使用的工作区根绝对路径。
    pub workspace_path: String,
    /// Absolute current directory path.
    /// 当前目录绝对路径。
    pub path: String,
    /// Workspace-root-relative current directory path.
    /// 当前目录相对工作区根的路径。
    pub relative_path: String,
    /// Optional absolute parent directory path inside the same workspace.
    /// 同一工作区内的可选父目录绝对路径。
    pub parent_path: Option<String>,
    /// Child rows returned for the current directory.
    /// 当前目录返回的子行。
    pub entries: Vec<WorkspaceFileEntrySummary>,
    /// Filtering options applied by the backend.
    /// 后端实际应用的过滤选项。
    pub filters: WorkspaceFileFilterOptions,
    /// Whether the result was truncated by the backend listing limit.
    /// 结果是否因后端列表上限被截断。
    pub truncated: bool,
    /// Number of entries returned after filtering and truncation.
    /// 过滤与截断后返回的条目数量。
    pub entry_count: usize,
    /// Optional human-readable notice for partial or bounded results.
    /// 局部或受限结果的人类可读提示。
    pub notice: Option<String>,
}

/// Describes one workspace file preview response.
/// 描述单个工作区文件预览响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileReadResponse {
    /// Absolute workspace root path used for the read.
    /// 本次读取使用的工作区根绝对路径。
    pub workspace_path: String,
    /// Absolute file path on the service host.
    /// 服务宿主上的文件绝对路径。
    pub path: String,
    /// Workspace-root-relative file path using forward slashes.
    /// 使用正斜杠的工作区根相对文件路径。
    pub relative_path: String,
    /// File category used by the WebUI preview renderer.
    /// WebUI 预览渲染器使用的文件类别。
    pub kind: WorkspaceFileEntryKind,
    /// Best-effort MIME type for the file.
    /// 文件的尽力推断 MIME 类型。
    pub mime_type: Option<String>,
    /// File size in bytes.
    /// 文件字节大小。
    pub size_bytes: u64,
    /// UTF-8 code content for code files.
    /// 代码文件的 UTF-8 内容。
    pub content: Option<String>,
    /// Encoding label for textual content.
    /// 文本内容的编码标签。
    pub encoding: Option<String>,
    /// Number of lines in the returned content when textual content is present.
    /// 返回文本内容存在时的行数。
    pub line_count: Option<usize>,
    /// Whether textual content was truncated by the backend read limit.
    /// 文本内容是否因后端读取上限被截断。
    pub truncated: bool,
}

/// Summarizes one conversation in the WebUI sidebar.
/// 汇总 WebUI 侧栏中的单个会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    /// Stable conversation identifier.
    /// 稳定会话标识。
    pub id: String,
    /// Conversation title.
    /// 会话标题。
    pub title: String,
    /// Conversation status text.
    /// 会话状态文本。
    pub status: String,
    /// Current per-session runtime phase.
    /// 当前会话级运行阶段。
    #[serde(default)]
    pub run_phase: String,
    /// Relative last-updated label shown in the sidebar.
    /// 侧栏中展示的相对最后更新时间。
    #[serde(default)]
    pub last_updated_label: String,
    /// Raw last-updated timestamp as Unix milliseconds text.
    /// 以 Unix 毫秒文本表示的原始最近更新时间。
    #[serde(default)]
    pub last_updated_at: String,
    /// Whether the conversation has unread completed activity.
    /// 会话是否存在未读的已完成活动。
    #[serde(default)]
    pub unread: bool,
    /// Optional owning project identifier.
    /// 可选所属项目标识。
    #[serde(default)]
    pub project_id: Option<String>,
    /// Display-only fork source marker.
    /// 纯展示用分叉来源标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_source: Option<ConversationForkSourceSummary>,
}

/// Describes the display-only source marker for a forked conversation.
/// 描述分叉会话的纯展示来源标记。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationForkSourceSummary {
    /// Source project tag used to locate the original conversation.
    /// 用于定位原始会话的源项目标签。
    pub source_project_tag: String,
    /// Source conversation identifier for navigation.
    /// 用于导航的源会话标识。
    pub source_conversation_id: String,
    /// Source conversation title captured at fork time.
    /// 分叉时捕获的源会话标题。
    pub source_title: String,
    /// Last retained message id at the fork boundary.
    /// 分叉边界处最后保留的消息标识。
    pub anchor_message_id: Option<String>,
    /// Fork creation timestamp.
    /// 分叉创建时间戳。
    pub created_at: String,
}

/// Represents one rendered conversation message.
/// 表示单条已渲染的会话消息。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    /// Stable message identifier.
    /// 稳定消息标识。
    pub id: String,
    /// Stable conversation identifier that owns this message.
    /// 拥有该消息的稳定会话标识。
    pub conversation_id: String,
    /// Unix-millisecond creation timestamp for turn duration projection.
    /// 用于轮次耗时投影的 Unix 毫秒创建时间戳。
    #[serde(default)]
    pub created_at: String,
    /// Message author role.
    /// 消息作者角色。
    pub role: MessageRole,
    /// Optional transcript title projected from the persisted session entry.
    /// 从持久化会话项投影出的可选转录标题。
    pub title: Option<String>,
    /// Durable transcript record kind such as transcript, tool-call-output, or turn-stats.
    /// 持久化转录记录类型，例如 transcript、tool-call-output 或 turn-stats。
    pub record_kind: Option<String>,
    /// Markdown or plain text body.
    /// Markdown 或纯文本正文。
    pub body: String,
    /// Optional structured metadata for WebUI-specific transcript renderers.
    /// 供 WebUI 专用转录渲染器使用的可选结构化元数据。
    pub metadata: Option<serde_json::Value>,
    /// Structured user attachments projected from transcript metadata.
    /// 从转录元数据投影出的结构化用户附件。
    #[serde(default)]
    pub attachments: Vec<ComposerAttachmentSummary>,
    /// Whether this user prompt can be safely edited and replayed by the backend.
    /// 后端判定该用户提示词是否可以安全编辑并重新执行。
    #[serde(default)]
    pub editable_user_prompt: bool,
}

/// Carries one ordered partial workbench update that avoids resending the full snapshot.
/// 携带一条有序的局部工作台更新，避免重复发送完整快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchDeltaEnvelope {
    /// Monotonic backend workbench generation used by clients for diagnostics and ordering.
    /// 客户端用于诊断与排序的后端工作台单调版本号。
    pub generation: u64,
    /// Partial workbench update payload.
    /// 局部工作台更新载荷。
    pub delta: WorkbenchDelta,
}

/// Describes one partial workbench update emitted on hot paths.
/// 描述热路径上发出的单条局部工作台更新。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "payload")]
pub enum WorkbenchDelta {
    /// Append-only body delta for a visible conversation message.
    /// 针对可见会话消息的追加式正文增量。
    AppendMessageBody(AppendMessageBodyDelta),
}

/// Appends streamed text to one visible conversation message.
/// 向单条可见会话消息追加流式文本。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppendMessageBodyDelta {
    /// Stable conversation identifier that owns the message.
    /// 拥有该消息的稳定会话标识。
    pub conversation_id: String,
    /// Stable message identifier that should receive the appended body text.
    /// 接收正文追加片段的稳定消息标识。
    pub message_id: String,
    /// Text appended by this event only.
    /// 本事件单独追加的文本片段。
    pub body_delta: String,
    /// Full message body after applying this delta, used by WebUI as the authoritative streaming text.
    /// 应用本次增量后的完整消息正文，供 WebUI 作为权威流式文本使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_body: Option<String>,
    /// Initial full message metadata when this delta creates the row on the client.
    /// 当本次增量在客户端创建行时携带的初始完整消息元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_message: Option<ConversationMessage>,
    /// Current per-conversation run phase after applying this delta.
    /// 应用本次增量后的当前会话运行阶段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_phase: Option<String>,
}

/// Summarizes one composer attachment kind exposed to WebUI controls.
/// 汇总 WebUI 输入区控件可使用的单种附件类型。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAttachmentCapabilitySummary {
    /// Stable attachment kind key such as image, audio, video, or pdf.
    /// 稳定附件类型键，例如 image、audio、video 或 pdf。
    pub kind: String,
    /// User-facing attachment kind label.
    /// 面向用户显示的附件类型名称。
    pub label: String,
    /// Whether this attachment kind can be submitted for the active model/tool setup.
    /// 当前模型与工具配置下是否允许提交该附件类型。
    pub enabled: bool,
    /// Whether the active provider/model can consume this attachment natively.
    /// 当前 Provider/模型是否能原生消费该附件类型。
    pub native: bool,
    /// Whether host-side AI tools can bridge this attachment kind when native support is missing.
    /// 原生能力缺失时宿主侧 AI 工具是否能桥接该附件类型。
    pub bridged: bool,
    /// Browser file picker accept string for this attachment kind.
    /// 该附件类型对应的浏览器文件选择 accept 字符串。
    pub accept: String,
}

/// Describes one staged composer attachment shared by the service and WebUI.
/// 描述一条由 service 与 WebUI 共享的已暂存输入区附件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAttachmentSummary {
    /// Stable attachment kind key such as image, audio, video, or pdf.
    /// 稳定附件类型键，例如 image、audio、video 或 pdf。
    pub kind: String,
    /// Human-readable file name shown in composer and transcript previews.
    /// 展示在输入区与转录预览中的人类可读文件名。
    pub file_name: String,
    /// Canonical managed local file path used for provider replay.
    /// Provider 回放使用的规范托管本地文件路径。
    pub local_path: String,
    /// Optional original source path retained for diagnostics.
    /// 为诊断保留的可选原始来源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Optional stable managed asset identifier.
    /// 可选稳定托管资产标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Optional source byte length.
    /// 可选源字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// Optional local preview image path.
    /// 可选本地预览图路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_local_path: Option<String>,
    /// Resolved MIME type attached to the staged file.
    /// 绑定到已暂存文件的解析后 MIME 类型。
    pub mime_type: String,
}

/// Represents the author role for a conversation message.
/// 表示会话消息的作者角色。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MessageRole {
    /// Message created by the user.
    /// 用户创建的消息。
    User,
    /// Message created by the assistant.
    /// 助手创建的消息。
    Assistant,
    /// Message created by the system.
    /// 系统创建的消息。
    System,
}

/// Summarizes one tool call in the WebUI.
/// 汇总 WebUI 中的单个工具调用。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallSummary {
    /// Stable tool call identifier.
    /// 稳定工具调用标识。
    pub id: String,
    /// Tool display name.
    /// 工具显示名称。
    pub name: String,
    /// Tool call status.
    /// 工具调用状态。
    pub status: String,
    /// Tool call detail text.
    /// 工具调用详情文本。
    pub detail: String,
}

/// Summarizes one plugin or LuaSkill package.
/// 汇总单个插件或 LuaSkill 包。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    /// Stable plugin identifier.
    /// 稳定插件标识。
    pub id: String,
    /// Plugin display name.
    /// 插件显示名称。
    pub name: String,
    /// Plugin kind such as LuaSkill or native Rust.
    /// 插件类型，例如 LuaSkill 或原生 Rust。
    pub kind: String,
    /// Plugin lifecycle status.
    /// 插件生命周期状态。
    pub status: String,
    /// Plugin capability summary.
    /// 插件能力摘要。
    pub capability: String,
}

/// Summarizes one settings section.
/// 汇总单个设置分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingSectionSummary {
    /// Stable settings section identifier.
    /// 稳定设置分组标识。
    pub id: String,
    /// Settings section display label.
    /// 设置分组显示名称。
    pub label: String,
    /// Settings section description.
    /// 设置分组描述。
    pub description: String,
    /// Representative setting items.
    /// 代表性设置项。
    pub items: Vec<String>,
}

/// Summarizes one model provider.
/// 汇总单个模型供应商。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub id: String,
    /// Provider display name.
    /// 供应商显示名称。
    pub name: String,
    /// Provider status text.
    /// 供应商状态文本。
    pub status: String,
    /// Whether this provider has a saved local configuration.
    /// 该供应商是否已经保存本地配置。
    #[serde(default)]
    pub configured: bool,
    /// Enabled model count text.
    /// 已启用模型数量文本。
    pub enabled_models: String,
    /// Whether this provider owns the currently active workbench model.
    /// 该供应商是否拥有当前激活的工作台模型。
    #[serde(default)]
    pub active: bool,
    /// Enabled model summaries exposed to the WebUI model picker.
    /// 暴露给 WebUI 模型选择器的已启用模型摘要。
    #[serde(default)]
    pub models: Vec<ProviderModelSummary>,
    /// Full catalog model summaries before enabled filtering.
    /// 启用过滤前的完整目录模型摘要。
    #[serde(default)]
    pub catalog_models: Vec<ProviderModelSummary>,
    /// Editable provider form fields projected from the provider catalog definition.
    /// 从供应商目录定义投影出的可编辑 Provider 表单字段。
    #[serde(default)]
    pub fields: Vec<ProviderFieldSummary>,
}

/// Summarizes one enabled model under a provider.
/// 汇总某个供应商下的单个已启用模型。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSummary {
    /// Stable model identifier persisted with the provider id.
    /// 与供应商标识一起持久化的稳定模型标识。
    pub id: String,
    /// Human-readable model display name.
    /// 人类可读的模型显示名称。
    pub name: String,
    /// Whether this model is the current workbench model.
    /// 该模型是否为当前工作台模型。
    #[serde(default)]
    pub active: bool,
    /// Whether this model is enabled for the provider.
    /// 该模型是否已在该供应商下启用。
    #[serde(default)]
    pub enabled: bool,
    /// Optional lifecycle status from the provider catalog.
    /// 来自供应商目录的可选生命周期状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional model family from the provider catalog.
    /// 来自供应商目录的可选模型家族。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional context window limit.
    /// 可选上下文窗口上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input token limit.
    /// 可选输入 Token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output token limit.
    /// 可选输出 Token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning capability flag.
    /// 可选推理能力标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional attachment capability flag.
    /// 可选附件能力标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Normalized input modality list.
    /// 归一化输入模态列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    /// Normalized output modality list.
    /// 归一化输出模态列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_modalities: Vec<String>,
    /// Optional reasoning profile JSON text.
    /// 可选推理档位配置 JSON 文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_profile_json: Option<String>,
    /// Whether this provider/model pair exposes backend-managed secondary routes.
    /// 该 Provider/模型组合是否暴露由后端管理的二级路由。
    #[serde(default)]
    pub supports_secondary_routes: bool,
    /// Active backend-managed secondary route, when one is selected.
    /// 当前已选中的后端二级路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_route: Option<ProviderModelSecondaryRouteSummary>,
}

/// Summarizes one backend-managed secondary route for a concrete provider/model pair.
/// 汇总某个具体 Provider/模型组合下的一条后端二级路由。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSecondaryRouteSummary {
    /// Stable owning provider identifier.
    /// 稳定所属 Provider 标识。
    pub provider_id: String,
    /// Stable owning model identifier.
    /// 稳定所属模型标识。
    pub model_id: String,
    /// Stable route identifier forwarded to the provider request.
    /// 转发到 Provider 请求中的稳定路由标识。
    pub route_id: String,
    /// Compatibility alias for providers that call the route id a tag.
    /// 兼容将路由标识称为 tag 的 Provider。
    pub tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    pub provider_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    pub endpoint_name: String,
    /// Compact label rendered in picker triggers.
    /// 渲染在选择器触发器中的紧凑标签。
    pub label: String,
    /// Secondary detail line rendered in route lists.
    /// 渲染在路由列表中的二级详情文本。
    pub subtitle: String,
    /// Whether this route is the active persisted selection.
    /// 该路由是否为当前已持久化选择。
    #[serde(default)]
    pub active: bool,
}

/// Selects one backend-managed secondary route for a concrete provider/model pair.
/// 为某个具体 Provider/模型组合选择一条后端二级路由。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSecondaryRouteSelection {
    /// Stable route identifier forwarded to the provider request.
    /// 转发到 Provider 请求中的稳定路由标识。
    pub route_id: String,
    /// Compatibility alias for providers that call the route id a tag.
    /// 兼容将路由标识称为 tag 的 Provider。
    #[serde(default)]
    pub tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    #[serde(default)]
    pub provider_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    #[serde(default)]
    pub endpoint_name: String,
}

/// Draft data used by the WebUI user-prompt edit dialog.
/// WebUI 用户提示词编辑弹窗使用的草稿数据。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptEditDraftSummary {
    /// Stable conversation identifier.
    /// 稳定会话标识。
    pub conversation_id: String,
    /// Stable user prompt message identifier.
    /// 稳定用户提示词消息标识。
    pub message_id: String,
    /// Editable prompt text.
    /// 可编辑提示词文本。
    pub prompt: String,
    /// Attachments that will be replayed with the edited prompt.
    /// 将随编辑后提示词重新执行的附件。
    #[serde(default)]
    pub attachments: Vec<ComposerAttachmentSummary>,
    /// Whether the owning conversation is currently running.
    /// 所属会话当前是否正在运行。
    #[serde(default)]
    pub running: bool,
}

/// Summarizes one editable provider configuration field.
/// 汇总单个可编辑 Provider 配置字段。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFieldSummary {
    /// Stable field key used by save requests.
    /// 保存请求使用的稳定字段键。
    pub key: String,
    /// User-facing field label.
    /// 面向用户显示的字段标签。
    pub label: String,
    /// Field placeholder text.
    /// 字段占位提示文本。
    pub placeholder: String,
    /// Stored field value.
    /// 已保存的字段值。
    pub value: String,
    /// Whether the field should be edited as a secret.
    /// 该字段是否应按密文字段编辑。
    pub secret: bool,
    /// Whether the field must be non-empty before saving.
    /// 保存前该字段是否必须非空。
    pub required: bool,
}

/// Summarizes one persisted MCP server configuration.
/// 汇总单个持久化 MCP 服务器配置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSummary {
    /// Stable MCP server identifier.
    /// 稳定 MCP 服务器标识。
    pub id: String,
    /// User-facing server name.
    /// 面向用户显示的服务器名称。
    pub name: String,
    /// Whether the server is enabled.
    /// 该服务器是否启用。
    pub enabled: bool,
    /// Transport key such as `stdio`, `http`, or `sse`.
    /// 传输键，例如 `stdio`、`http` 或 `sse`。
    pub transport: String,
    /// Command used for stdio servers.
    /// stdio 服务器使用的启动命令。
    pub command: String,
    /// Command arguments used for stdio servers.
    /// stdio 服务器使用的启动参数。
    pub args: Vec<String>,
    /// URL used for remote servers.
    /// 远端服务器使用的 URL。
    pub url: String,
    /// Bearer token used for remote servers.
    /// 远端服务器使用的 Bearer Token。
    pub bearer_token: String,
    /// Explicit request headers.
    /// 显式请求头。
    pub headers: Vec<KeyValueSummary>,
    /// Request headers resolved from environment variables.
    /// 从环境变量解析的请求头。
    pub headers_from_env: Vec<KeyValueSummary>,
    /// OAuth client identifier.
    /// OAuth 客户端标识。
    pub oauth_client_id: String,
    /// OAuth client secret.
    /// OAuth 客户端密钥。
    pub oauth_client_secret: String,
    /// OAuth scope list.
    /// OAuth scope 列表。
    pub oauth_scopes: Vec<String>,
    /// OAuth redirect URI.
    /// OAuth 回调 URI。
    pub oauth_redirect_uri: String,
    /// Working directory used by stdio servers.
    /// stdio 服务器使用的工作目录。
    pub cwd: String,
    /// Environment variables injected into stdio servers.
    /// 注入 stdio 服务器的环境变量。
    pub env: Vec<KeyValueSummary>,
    /// Environment variable names passed through to stdio servers.
    /// 透传给 stdio 服务器的环境变量名。
    pub env_passthrough: Vec<String>,
}

/// Summarizes one string key/value pair.
/// 汇总单个字符串键值对。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueSummary {
    /// Stable key.
    /// 稳定键。
    pub key: String,
    /// Stored value.
    /// 已保存值。
    pub value: String,
}

/// Summarizes the workbench reasoning selector resolved by the backend.
/// 汇总由后端解析出的工作台思考选择器。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSelectionSummary {
    /// Dynamic options supported by the active provider and model.
    /// 当前激活 Provider 与模型支持的动态选项。
    pub options: Vec<ReasoningSelectionOptionSummary>,
    /// Effective selected reasoning effort value.
    /// 当前生效的已选思考档位值。
    pub selected_effort: String,
    /// Optional note explaining model-specific reconciliation.
    /// 可选说明，用于解释模型特定的档位协调结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Summarizes one reasoning selector option.
/// 汇总单个思考选择器选项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSelectionOptionSummary {
    /// Stable reasoning effort value sent back through the service router.
    /// 通过服务路由回传的稳定思考档位值。
    pub value: String,
    /// User-facing option label resolved from provider configuration.
    /// 从 Provider 配置解析出的面向用户选项标签。
    pub label: String,
}

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

/// Full VMM memory page snapshot used by the WebUI.
/// WebUI 使用的完整 VMM 记忆页面快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemorySnapshot {
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
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub ai_tools_enabled: bool,
    /// Whether automatic precheck recall is enabled.
    /// 是否启用自动前置召回。
    pub precheck_enabled: bool,
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
    /// Whether memory is enabled on this layer.
    /// 当前层是否启用记忆。
    pub enabled: bool,
    /// Editable gRPC target.
    /// 可编辑 gRPC 目标。
    pub grpc_target: String,
    /// Editable gRPC API key.
    /// 可编辑 gRPC API key。
    pub grpc_api_key: String,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemoryActionRequest {
    /// Stable action id.
    /// 稳定动作标识。
    pub action: String,
    /// Scope key, either global or project.
    /// 作用域键，可为 global 或 project。
    pub scope: String,
    /// Optional task project id used by project scope.
    /// 项目作用域使用的可选任务项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Action-specific JSON payload.
    /// 动作专用 JSON 载荷。
    #[serde(default)]
    pub payload: serde_json::Value,
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

/// Summarizes one long-running command session.
/// 汇总单个长生命周期命令会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSessionSummary {
    /// Stable command session identifier.
    /// 稳定命令会话标识。
    pub id: String,
    /// Optional user-facing terminal session name.
    /// 可选用户可见终端会话名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Command display name.
    /// 命令显示名称。
    pub command: String,
    /// Live command status.
    /// 实时命令状态。
    pub status: String,
    /// Recent command output preview.
    /// 最近命令输出预览。
    pub preview: String,
    /// Optional owner session id when the runtime reports one.
    /// 运行时报告时的可选归属会话标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    /// Effective working directory used by the command session.
    /// 命令会话使用的实际工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Stable shell label selected by the host.
    /// 宿主选择的稳定 shell 标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_label: Option<String>,
    /// Concrete backend label reported by the host runtime.
    /// 宿主运行时报告的具体后端标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Latest screen frame sequence reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新屏幕帧序号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_frame_seq: Option<u64>,
    /// Latest terminal row count reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新终端行数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_rows: Option<u16>,
    /// Latest terminal column count reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新终端列数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_cols: Option<u16>,
    /// Latest renderable screen text reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新可渲染屏幕内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_text: Option<String>,
    /// Optional process exit code captured by the host.
    /// 宿主捕获到的可选进程退出码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Optional failure reason synthesized by the host runtime.
    /// 宿主运行时综合生成的可选失败原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Whether this summary represents a historical session no longer present in live runtime state.
    /// 该摘要是否代表实时运行时中已不存在的历史会话。
    #[serde(default, skip_serializing_if = "is_false")]
    pub lost: bool,
}

/// Summarizes one diagnostic log entry.
/// 汇总单条诊断日志。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntrySummary {
    /// Stable log entry identifier.
    /// 稳定日志条目标识。
    pub id: String,
    /// Log entry level.
    /// 日志级别。
    pub level: String,
    /// Log entry source.
    /// 日志来源。
    pub source: String,
    /// Log entry message.
    /// 日志消息。
    pub message: String,
}

/// Describes one session addon-state namespace change emitted to connected UI clients.
/// 描述一次发送给已连接 UI 客户端的会话附属状态命名空间变更。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAddonChangedEvent {
    /// Session identifier whose addon state changed.
    /// 附属状态发生变化的会话标识。
    pub session_id: String,
    /// Changed addon namespace keys.
    /// 已变化的附属状态命名空间键。
    pub namespaces: Vec<String>,
    /// Latest addon document revision after the change.
    /// 变化后的最新附属状态文档版本。
    pub revision: u64,
    /// Unix-millis text representing the latest addon update time.
    /// 表示最近附属状态更新时间的 Unix 毫秒文本。
    pub updated_at: String,
    /// Optional local instance id that originated the change.
    /// 发起该变化的可选本地实例标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_instance_id: Option<String>,
}

/// Represents one event emitted from the local service bridge.
/// 表示本地服务桥接层发出的单个事件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "payload")]
pub enum ServiceEvent {
    /// Startup snapshot event.
    /// 启动快照事件。
    Startup(Box<StartupSnapshot>),
    /// Workbench snapshot event.
    /// 工作台快照事件。
    Workbench(Box<WorkbenchSnapshot>),
    /// Partial workbench update event.
    /// 局部工作台更新事件。
    WorkbenchDelta(Box<WorkbenchDeltaEnvelope>),
    /// Session addon namespace change event.
    /// 会话附属状态命名空间变更事件。
    SessionAddonChanged(Box<SessionAddonChangedEvent>),
}

/// Receives one service event emitted by the Core Service boundary.
/// 接收 Core Service 边界发出的单个服务事件。
pub trait ServiceEventSink: Send + Sync {
    /// Delivers one service event to the concrete sink implementation.
    /// 将单个服务事件投递给具体 sink 实现。
    fn emit_service_event(&self, event: ServiceEvent);
}

/// Multiplexes service events to all registered client-facing sinks.
/// 将服务事件复用分发到所有已注册的客户端 sink。
#[derive(Clone)]
pub struct ServiceEventEmitter {
    /// Ordered event sinks used by the emitter.
    /// 事件发送器使用的有序 sink 列表。
    sinks: Arc<Vec<Arc<dyn ServiceEventSink>>>,
}

impl ServiceEventEmitter {
    /// Builds an event emitter from an ordered sink list.
    /// 使用有序 sink 列表构建事件发送器。
    pub fn new(sinks: Vec<Arc<dyn ServiceEventSink>>) -> Self {
        Self {
            sinks: Arc::new(sinks),
        }
    }

    /// Emits one event to every registered sink while minimizing event clones.
    /// 向所有已注册 sink 发送单个事件，并尽量减少事件克隆。
    pub fn emit(&self, event: ServiceEvent) {
        let Some((last, leading)) = self.sinks.split_last() else {
            return;
        };
        for sink in leading {
            sink.emit_service_event(event.clone());
        }
        last.emit_service_event(event);
    }
}

/// Browser-facing service discovery snapshot shared by health checks and clients.
/// 健康检查与客户端共享的浏览器侧服务发现快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceDiscoverySnapshot {
    /// Human-readable service status.
    /// 人类可读的服务状态。
    pub status: String,
    /// Browser-facing base URL.
    /// 面向浏览器的基础 URL。
    pub base_url: String,
    /// Loopback host used by the service bridge.
    /// 服务桥接层使用的回环主机。
    pub host: String,
    /// TCP port used by the service bridge.
    /// 服务桥接层使用的 TCP 端口。
    pub port: u16,
    /// Owning process id.
    /// 拥有该服务的进程标识。
    pub pid: u32,
    /// UTC startup timestamp in Unix milliseconds.
    /// UTC 启动时间戳，单位为 Unix 毫秒。
    pub started_at: String,
    /// Instance access token for loopback browser clients.
    /// 提供给回环浏览器客户端的实例访问令牌。
    pub service_token: String,
}

/// Service discovery status used while the HTTP bridge is bound but startup is still running.
/// HTTP 桥接已绑定但启动流程仍在运行时使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_STARTING: &str = "starting";

/// Service discovery status used after the full service bootstrap has completed.
/// 完整服务引导完成后使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_READY: &str = "ready";

/// Service discovery status used after service bootstrap fails.
/// 服务引导失败后使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_FAILED: &str = "failed";

/// Builds one browser-facing WebUI service discovery snapshot with an explicit status.
/// 使用显式状态构建一个浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `status` is the service readiness marker, `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `status` 是服务就绪标记，`port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot suitable for health responses and discovery files.
/// 返回适用于健康响应和发现文件的发现快照。
pub fn webui_service_discovery_snapshot_with_status(
    status: impl Into<String>,
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    let base_url = format!("http://{WEBUI_SERVICE_LOOPBACK_HOST}:{port}");
    WebuiServiceDiscoverySnapshot {
        status: status.into(),
        base_url,
        host: String::from(WEBUI_SERVICE_LOOPBACK_HOST),
        port,
        pid,
        started_at: started_at.into(),
        service_token: service_token.into(),
    }
}

/// Builds one starting browser-facing WebUI service discovery snapshot.
/// 构建一个 starting 状态的浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot that advertises a reachable but not-yet-ready service.
/// 返回表示服务可访问但尚未就绪的发现快照。
pub fn webui_service_starting_discovery_snapshot(
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    webui_service_discovery_snapshot_with_status(
        WEBUI_SERVICE_DISCOVERY_STATUS_STARTING,
        port,
        pid,
        started_at,
        service_token,
    )
}

/// Builds one ready browser-facing WebUI service discovery snapshot.
/// 构建一个 ready 状态的浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot suitable for health responses and discovery files.
/// 返回适用于健康响应和发现文件的发现快照。
pub fn webui_service_ready_discovery_snapshot(
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    webui_service_discovery_snapshot_with_status(
        WEBUI_SERVICE_DISCOVERY_STATUS_READY,
        port,
        pid,
        started_at,
        service_token,
    )
}

/// Browser-facing RPC request body shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 请求体。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpRequest {
    /// Logical service method name.
    /// 逻辑服务方法名。
    pub method: String,
    /// Optional JSON parameter object.
    /// 可选 JSON 参数对象。
    pub params: Option<serde_json::Value>,
}

/// Browser-facing RPC response body shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 响应体。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpResponse {
    /// Whether the RPC call succeeded.
    /// RPC 调用是否成功。
    pub ok: bool,
    /// Successful response payload.
    /// 成功响应载荷。
    pub result: Option<serde_json::Value>,
    /// Normalized error payload.
    /// 标准化错误载荷。
    pub error: Option<WebuiServiceRpcHttpError>,
}

/// Browser-facing RPC error payload shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 错误载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpError {
    /// Stable machine-readable error code.
    /// 稳定的机器可读错误码。
    pub code: String,
    /// Human-readable error message.
    /// 人类可读错误消息。
    pub message: String,
}

/// Builds one successful browser-facing RPC envelope and redacts secrets in place.
/// 构建单个成功的浏览器侧 RPC 信封，并原地脱敏敏感信息。
///
/// # Parameters
/// 参数
/// `result` is the JSON payload produced by the service method.
/// `result` 是服务方法生成的 JSON 载荷。
///
/// # Returns
/// 返回值
/// A normalized successful RPC envelope safe to send to browser clients.
/// 返回可安全发送给浏览器客户端的标准化成功 RPC 信封。
pub fn webui_service_rpc_success_envelope(
    mut result: serde_json::Value,
) -> WebuiServiceRpcHttpResponse {
    redact_webui_service_rpc_secrets(&mut result);
    WebuiServiceRpcHttpResponse {
        ok: true,
        result: Some(result),
        error: None,
    }
}

/// Builds one failed browser-facing RPC envelope.
/// 构建单个失败的浏览器侧 RPC 信封。
///
/// # Parameters
/// 参数
/// `code` is the stable machine-readable error code, and `message` is the human-readable error.
/// `code` 是稳定的机器可读错误码，`message` 是人类可读错误消息。
///
/// # Returns
/// 返回值
/// A normalized failed RPC envelope.
/// 返回标准化失败 RPC 信封。
pub fn webui_service_rpc_error_envelope(
    code: impl Into<String>,
    message: impl Into<String>,
) -> WebuiServiceRpcHttpResponse {
    WebuiServiceRpcHttpResponse {
        ok: false,
        result: None,
        error: Some(WebuiServiceRpcHttpError {
            code: code.into(),
            message: message.into(),
        }),
    }
}

/// Redacts sensitive fields before a value crosses the browser-facing HTTP boundary.
/// 在值跨越浏览器侧 HTTP 边界前脱敏敏感字段。
///
/// # Parameters
/// 参数
/// `value` is the JSON RPC result payload to sanitize in place.
/// `value` 是需要原地脱敏的 JSON RPC 结果载荷。
pub fn redact_webui_service_rpc_secrets(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map.iter_mut() {
                if webui_service_rpc_secret_key_should_be_redacted(key) {
                    if nested
                        .as_str()
                        .is_some_and(|secret| !secret.trim().is_empty())
                    {
                        *nested = serde_json::Value::String(String::from(
                            WEBUI_REDACTED_SECRET_PLACEHOLDER,
                        ));
                    }
                    continue;
                }
                redact_webui_service_rpc_secrets(nested);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_webui_service_rpc_secrets(item);
            }
        }
        _ => {}
    }
}

/// Returns whether one JSON key carries a browser-visible secret.
/// 返回一个 JSON 字段名是否携带浏览器可见敏感信息。
///
/// # Parameters
/// 参数
/// `key` is the JSON object key being inspected.
/// `key` 是正在检查的 JSON 对象字段名。
///
/// # Returns
/// 返回值
/// `true` when the key should be redacted for HTTP clients.
/// 当该字段应对 HTTP 客户端脱敏时返回 `true`。
pub fn webui_service_rpc_secret_key_should_be_redacted(key: &str) -> bool {
    matches!(
        key,
        "apiKey"
            | "api_key"
            | "grpcApiKey"
            | "grpc_api_key"
            | "accessToken"
            | "access_token"
            | "refreshToken"
            | "refresh_token"
            | "clientSecret"
            | "client_secret"
            | "secret"
    )
}

/// Converts a browser WebUI request path into a traversal-safe relative static path.
/// 将浏览器 WebUI 请求路径转换为防穿越的相对静态路径。
///
/// # Parameters
/// 参数
/// `request_path` is the URL path received by the browser-facing static route.
/// `request_path` 是浏览器侧静态路由收到的 URL 路径。
///
/// # Returns
/// 返回值
/// A relative path rooted under the WebUI static bundle directory.
/// 返回位于 WebUI 静态包目录下的相对路径。
pub fn webui_static_relative_path(request_path: &str) -> Result<PathBuf, String> {
    let trimmed = request_path.trim_start_matches('/');
    let normalized = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    let mut relative_path = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." || segment.contains('\\') || segment.contains(':') {
            return Err(String::from("invalid WebUI static path segment"));
        }
        relative_path.push(segment);
    }
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(String::from("invalid WebUI static path traversal"));
    }
    Ok(relative_path)
}

/// Infers a conservative MIME type for WebUI static assets.
/// 为 WebUI 静态资源推断保守 MIME 类型。
///
/// # Parameters
/// 参数
/// `path` is the resolved static file path or relative static asset path.
/// `path` 是已解析的静态文件路径或相对静态资源路径。
///
/// # Returns
/// 返回值
/// A static MIME type suitable for HTTP response headers.
/// 返回适用于 HTTP 响应头的静态 MIME 类型。
pub fn webui_static_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Framework-neutral static file response returned by the WebUI static pipeline.
/// WebUI 静态文件管线返回的框架无关静态响应。
pub struct ServiceStaticResponse {
    /// Numeric HTTP status code.
    /// 数字 HTTP 状态码。
    pub status: u16,
    /// Response headers preserved in insertion order.
    /// 按插入顺序保留的响应头。
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    /// 响应体字节。
    pub body: Vec<u8>,
}

/// Resolves the WebUI static root from an optional override and fallback directory.
/// 根据可选覆盖目录和回退目录解析 WebUI 静态根目录。
///
/// # Parameters
/// 参数
/// `override_path` is the raw configured static directory and `fallback_path` is the bundled dist directory.
/// `override_path` 是原始配置静态目录，`fallback_path` 是随包发布的 dist 目录。
///
/// # Returns
/// 返回值
/// The first existing directory, preferring the override path when valid.
/// 返回第一个存在的目录，并在覆盖目录有效时优先使用覆盖目录。
pub fn webui_static_root_from_candidates(
    override_path: Option<&str>,
    fallback_path: Option<PathBuf>,
) -> Option<PathBuf> {
    let override_path = override_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir());
    if override_path.is_some() {
        return override_path;
    }
    fallback_path.filter(|path| path.is_dir())
}

/// Builds one WebUI static response from a configured static root.
/// 从已配置的静态根目录构建单个 WebUI 静态响应。
///
/// # Parameters
/// 参数
/// `static_root` is the optional WebUI bundle directory, `request_path` is the browser path, `cache_control` is the response cache policy, and `missing_message` is the 404 text.
/// `static_root` 是可选 WebUI 包目录，`request_path` 是浏览器路径，`cache_control` 是响应缓存策略，`missing_message` 是 404 文本。
///
/// # Returns
/// 返回值
/// A framework-neutral static response ready for HTTP adapter conversion.
/// 返回可供 HTTP 适配器转换的框架无关静态响应。
pub fn build_webui_static_response(
    static_root: Option<&Path>,
    request_path: &str,
    cache_control: &str,
    missing_message: &str,
) -> ServiceStaticResponse {
    let Some(static_root) = static_root else {
        return service_static_text_response(404, missing_message);
    };
    match webui_static_file_path_for_request(static_root, request_path) {
        Ok(Some(path)) => build_webui_static_file_response(path, cache_control),
        Ok(None) => service_static_text_response(404, missing_message),
        Err(error) => service_static_text_response(400, &error),
    }
}

/// Resolves one request path into a safe WebUI static file path.
/// 将单个请求路径解析为安全的 WebUI 静态文件路径。
///
/// # Parameters
/// 参数
/// `static_root` is the WebUI bundle directory and `request_path` is the browser route path.
/// `static_root` 是 WebUI 包目录，`request_path` 是浏览器路由路径。
///
/// # Returns
/// 返回值
/// `Some(path)` for an existing static file or SPA fallback, `None` when no file should be served.
/// 对存在的静态文件或 SPA 回退返回 `Some(path)`，无可服务文件时返回 `None`。
pub fn webui_static_file_path_for_request(
    static_root: &Path,
    request_path: &str,
) -> Result<Option<PathBuf>, String> {
    let relative_path = webui_static_relative_path(request_path)?;
    let file_path = static_root.join(&relative_path);
    if file_path.is_file() {
        return Ok(Some(file_path));
    }
    if relative_path.extension().is_some() {
        return Ok(None);
    }
    let index_path = static_root.join("index.html");
    if index_path.is_file() {
        return Ok(Some(index_path));
    }
    Ok(None)
}

/// Builds one static WebUI file response with a conservative content type.
/// 使用保守的内容类型构建单个 WebUI 静态文件响应。
fn build_webui_static_file_response(path: PathBuf, cache_control: &str) -> ServiceStaticResponse {
    match fs::read(&path) {
        Ok(bytes) => ServiceStaticResponse {
            status: 200,
            headers: vec![
                (
                    String::from("content-type"),
                    String::from(webui_static_mime_type(&path)),
                ),
                (String::from("cache-control"), String::from(cache_control)),
            ],
            body: bytes,
        },
        Err(error) => {
            service_static_text_response(500, &format!("failed to read WebUI static file: {error}"))
        }
    }
}

/// Builds one text response for browser-service static file failures.
/// 为浏览器服务静态文件失败场景构建单个文本响应。
fn service_static_text_response(status: u16, message: &str) -> ServiceStaticResponse {
    ServiceStaticResponse {
        status,
        headers: vec![(
            String::from("content-type"),
            String::from("text/plain; charset=utf-8"),
        )],
        body: message.as_bytes().to_vec(),
    }
}

/// Returns whether one browser Origin string may access the loopback WebUI service.
/// 返回单个浏览器 Origin 字符串是否允许访问回环 WebUI 服务。
///
/// # Parameters
/// 参数
/// `origin` is the raw Origin header value.
/// `origin` 是原始 Origin 请求头值。
///
/// # Returns
/// 返回值
/// `true` for trusted Tauri origins, the Windows Tauri HTTP origin, and loopback HTTP origins.
/// 对受信任的 Tauri 来源、Windows Tauri HTTP 来源和回环 HTTP 来源返回 `true`。
pub fn webui_service_origin_text_is_allowed(origin: &str) -> bool {
    let Some((scheme, remainder)) = origin.split_once("://") else {
        return false;
    };
    let host_port = remainder.split('/').next().unwrap_or_default();
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    if scheme == "tauri" {
        return host.eq_ignore_ascii_case("localhost");
    }
    if scheme != "http" && scheme != "https" {
        return false;
    }
    if host.eq_ignore_ascii_case("tauri.localhost") {
        return true;
    }
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// Returns whether a required browser Origin value is present and allowed.
/// 返回必需的浏览器 Origin 值是否存在且被允许。
///
/// # Parameters
/// 参数
/// `origin` is the optional raw Origin header value extracted by the transport adapter.
/// `origin` 是传输适配层提取出的可选原始 Origin 请求头值。
///
/// # Returns
/// 返回值
/// `true` only when the Origin header exists and passes loopback origin validation.
/// 仅当 Origin 请求头存在且通过回环来源校验时返回 `true`。
pub fn webui_service_required_origin_text_is_allowed(origin: Option<&str>) -> bool {
    origin.is_some_and(webui_service_origin_text_is_allowed)
}

/// Returns whether browser service token locations contain the current instance token.
/// 返回浏览器服务令牌位置是否包含当前实例令牌。
///
/// # Parameters
/// 参数
/// `header_token` is the optional token header, `query` is the raw query string, and `expected_token` is the generated instance token.
/// `header_token` 是可选令牌请求头，`query` 是原始查询字符串，`expected_token` 是生成的实例令牌。
///
/// # Returns
/// 返回值
/// `true` when either header or query token matches the expected token.
/// 当 header 或 query 中的令牌与期望令牌匹配时返回 `true`。
pub fn webui_service_request_token_values_are_valid(
    header_token: Option<&str>,
    query: Option<&str>,
    expected_token: &str,
) -> bool {
    if expected_token.trim().is_empty() {
        return false;
    }
    if header_token.is_some_and(|token| webui_service_token_matches(token, expected_token)) {
        return true;
    }
    query
        .and_then(webui_service_token_from_query)
        .is_some_and(|token| webui_service_token_matches(&token, expected_token))
}

/// Extracts the service token value from a URL query string.
/// 从 URL 查询字符串中提取服务令牌值。
///
/// # Parameters
/// 参数
/// `query` is the raw query string without the leading question mark.
/// `query` 是不带问号前缀的原始查询字符串。
///
/// # Returns
/// 返回值
/// The first decoded `token` or `serviceToken` query parameter when present.
/// 存在时返回首个解码后的 `token` 或 `serviceToken` 查询参数。
pub fn webui_service_token_from_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "token" && key != "serviceToken" {
            continue;
        }
        return percent_decode_query_component(value);
    }
    None
}

/// Compares two browser service tokens.
/// 比较两个浏览器服务令牌。
///
/// # Parameters
/// 参数
/// `actual` is the token supplied by the request, and `expected` is the generated token.
/// `actual` 是请求提供的令牌，`expected` 是生成的令牌。
///
/// # Returns
/// 返回值
/// `true` when both token strings are exactly equal after trimming request whitespace.
/// 当两个令牌字符串在裁剪请求空白后完全一致时返回 `true`。
pub fn webui_service_token_matches(actual: &str, expected: &str) -> bool {
    actual.trim() == expected
}

/// Resolves candidate browser-facing local service ports from an optional override value.
/// 根据可选覆盖值解析浏览器侧本地服务候选端口。
///
/// # Parameters
/// 参数
/// `port_override` is the raw configured port value, usually read from `WEBUI_SERVICE_PORT_ENV`.
/// `port_override` 是原始配置端口值，通常来自 `WEBUI_SERVICE_PORT_ENV`。
///
/// # Returns
/// 返回值
/// A single override port when valid, otherwise the default discovery range.
/// 覆盖值有效时返回单个端口，否则返回默认发现端口范围。
pub fn webui_service_http_bind_ports_from_override(port_override: Option<&str>) -> Vec<u16> {
    if let Some(port) = port_override
        .map(str::trim)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
    {
        return vec![port];
    }
    webui_service_http_default_bind_ports()
}

/// Returns the default browser-facing local service discovery port range.
/// 返回浏览器侧本地服务默认发现端口范围。
pub fn webui_service_http_default_bind_ports() -> Vec<u16> {
    (WEBUI_SERVICE_HTTP_PORT..=WEBUI_SERVICE_HTTP_PORT_END).collect()
}

/// Resolves the shared browser service discovery file path.
/// 解析共享浏览器服务发现文件路径。
///
/// # Parameters
/// 参数
/// `home_dir` is the optional user home directory and `temp_dir` is the fallback temporary directory.
/// `home_dir` 是可选用户主目录，`temp_dir` 是回退临时目录。
///
/// # Returns
/// 返回值
/// The path where browser hosts can discover the current service endpoint.
/// 返回浏览器宿主可发现当前服务端点的路径。
pub fn webui_service_discovery_snapshot_path(
    home_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> PathBuf {
    home_dir
        .unwrap_or(temp_dir)
        .join(".vulcan")
        .join("code")
        .join("webui-service.json")
}

/// Decodes one URL query component enough for opaque service tokens.
/// 对单个 URL 查询组件进行足以支持不透明服务令牌的解码。
fn percent_decode_query_component(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut index = 0usize;
    let raw = value.as_bytes();
    while index < raw.len() {
        match raw[index] {
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < raw.len() => {
                let high = hex_digit_value(raw[index + 1])?;
                let low = hex_digit_value(raw[index + 2])?;
                bytes.push((high << 4) | low);
                index += 3;
            }
            byte => {
                bytes.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(bytes).ok()
}

/// Converts one ASCII hex digit into its numeric value.
/// 将单个 ASCII 十六进制字符转换为数值。
fn hex_digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Framework-neutral media response returned by the Core Service media pipeline.
/// Core Service 媒体管线返回的框架无关媒体响应。
pub struct ServiceMediaResponse {
    /// Numeric HTTP status code.
    /// 数字 HTTP 状态码。
    pub status: u16,
    /// Response headers preserved in insertion order.
    /// 按插入顺序保留的响应头。
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    /// 响应体字节。
    pub body: Vec<u8>,
}

/// Decodes one URL-safe base64 media route segment into a local filesystem path.
/// 将单个 URL 安全 base64 媒体路由片段解码为本地文件系统路径。
pub fn service_media_path_from_encoded_route_segment(
    encoded_path: &str,
) -> Result<PathBuf, String> {
    let path_bytes = BASE64_URL_SAFE_NO_PAD
        .decode(encoded_path)
        .map_err(|error| format!("媒体路径解码失败：{error}"))?;
    let path_text = String::from_utf8(path_bytes)
        .map_err(|error| format!("媒体路径不是有效 UTF-8：{error}"))?;
    Ok(PathBuf::from(path_text))
}

/// Builds one shared media file response with optional HTTP range support.
/// 构建单个带可选 HTTP Range 支持的共享媒体文件响应。
pub fn build_service_media_file_response(
    media_path: PathBuf,
    is_head: bool,
    range_header: Option<&str>,
) -> Result<ServiceMediaResponse, String> {
    if !is_allowed_service_media_asset_path(&media_path) {
        return Ok(service_media_error_response(
            403,
            "媒体路径不是允许的媒体资源类型。",
        ));
    }

    let mut file = match File::open(&media_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(service_media_error_response(404, "媒体文件不存在。"));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(service_media_error_response(403, "没有权限读取媒体文件。"));
        }
        Err(error) => return Err(format!("无法打开媒体文件：{error}")),
    };
    let file_len = file
        .metadata()
        .map_err(|error| format!("无法读取媒体文件元数据：{error}"))?
        .len();
    let mime_type = infer_service_media_asset_mime_type(&media_path);
    let mut headers = service_media_base_headers(mime_type);

    if is_head {
        headers.push((String::from("content-length"), file_len.to_string()));
        return Ok(ServiceMediaResponse {
            status: 200,
            headers,
            body: Vec::new(),
        });
    }

    if let Some(range_header) = range_header {
        let range = single_service_media_http_range(range_header, file_len)?;
        let range_start = range.start;
        let range_end = range.start + range.length - 1;
        let mut body = Vec::with_capacity(range.length as usize);
        file.seek(SeekFrom::Start(range_start))
            .map_err(|error| format!("无法定位媒体范围：{error}"))?;
        file.take(range.length)
            .read_to_end(&mut body)
            .map_err(|error| format!("无法读取媒体范围：{error}"))?;
        headers.push((
            String::from("content-range"),
            format!("bytes {range_start}-{range_end}/{file_len}"),
        ));
        headers.push((String::from("content-length"), body.len().to_string()));
        return Ok(ServiceMediaResponse {
            status: 206,
            headers,
            body,
        });
    }

    let mut body = Vec::with_capacity(file_len.min(SERVICE_MEDIA_RANGE_MAX_BYTES) as usize);
    file.read_to_end(&mut body)
        .map_err(|error| format!("无法读取媒体文件：{error}"))?;
    headers.push((String::from("content-length"), file_len.to_string()));
    Ok(ServiceMediaResponse {
        status: 200,
        headers,
        body,
    })
}

/// Returns whether a media path belongs to the generated attachment storage tree.
/// 返回媒体路径是否属于生成附件存储树。
fn is_allowed_generated_service_media_path(path: &Path) -> bool {
    let normalized_components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|text| text.to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    normalized_components
        .windows(3)
        .any(|window| window == [".vulcan", "code", "attachments"])
}

/// Returns whether a media path can be served by the local media route.
/// 返回媒体路径是否允许通过本地媒体路由提供。
fn is_allowed_service_media_asset_path(path: &Path) -> bool {
    if is_allowed_generated_service_media_path(path) {
        return true;
    }
    has_supported_service_media_asset_extension(path)
}

/// Returns whether a local path extension is supported by the media route.
/// 返回本地路径扩展名是否受媒体路由支持。
fn has_supported_service_media_asset_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("webp")
            | Some("gif")
            | Some("bmp")
            | Some("svg")
            | Some("mp4")
            | Some("webm")
            | Some("mov")
            | Some("mkv")
            | Some("mp3")
            | Some("wav")
            | Some("m4a")
            | Some("aac")
            | Some("flac")
            | Some("ogg")
            | Some("pdf")
    )
}

/// Parses one bounded HTTP range for media playback.
/// 解析一个有上限的媒体播放 HTTP 范围。
fn single_service_media_http_range(range_header: &str, file_len: u64) -> Result<HttpRange, String> {
    let mut ranges = HttpRange::parse(range_header, file_len)
        .map_err(|error| format!("媒体 Range 请求无效：{error:?}"))?;
    let Some(mut range) = ranges.drain(..).next() else {
        return Err(String::from("媒体 Range 请求为空。"));
    };
    if range.start >= file_len || range.length == 0 {
        return Err(String::from("媒体 Range 请求越界。"));
    }
    range.length = range.length.min(SERVICE_MEDIA_RANGE_MAX_BYTES);
    Ok(range)
}

/// Builds one plain text error response for the generated media route.
/// 为生成媒体路由构建一个纯文本错误响应。
pub fn service_media_error_response(status: u16, message: &str) -> ServiceMediaResponse {
    ServiceMediaResponse {
        status,
        headers: vec![
            (
                String::from("access-control-allow-origin"),
                String::from("*"),
            ),
            (
                String::from("content-type"),
                String::from("text/plain; charset=utf-8"),
            ),
        ],
        body: message.as_bytes().to_vec(),
    }
}

/// Builds common headers shared by media success responses.
/// 构建媒体成功响应共享的公共响应头。
fn service_media_base_headers(mime_type: String) -> Vec<(String, String)> {
    vec![
        (
            String::from("access-control-allow-origin"),
            String::from("*"),
        ),
        (
            String::from("access-control-expose-headers"),
            String::from("content-range"),
        ),
        (String::from("accept-ranges"), String::from("bytes")),
        (String::from("cache-control"), String::from("no-store")),
        (String::from("content-type"), mime_type),
    ]
}

/// Infers a conservative MIME type from a generated media path extension.
/// 根据生成媒体路径扩展名推断保守 MIME 类型。
fn infer_service_media_asset_mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => String::from("image/png"),
        Some("jpg") | Some("jpeg") => String::from("image/jpeg"),
        Some("webp") => String::from("image/webp"),
        Some("gif") => String::from("image/gif"),
        Some("bmp") => String::from("image/bmp"),
        Some("svg") => String::from("image/svg+xml"),
        Some("mp4") => String::from("video/mp4"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("mkv") => String::from("video/x-matroska"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("wav") => String::from("audio/wav"),
        Some("m4a") => String::from("audio/mp4"),
        Some("aac") => String::from("audio/aac"),
        Some("flac") => String::from("audio/flac"),
        Some("ogg") => String::from("audio/ogg"),
        Some("pdf") => String::from("application/pdf"),
        _ => String::from("application/octet-stream"),
    }
}

/// Returns true when a boolean is false for serde omission helpers.
/// 当布尔值为 false 时返回 true，用于 serde 省略辅助。
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::{
        WEBUI_REDACTED_SECRET_PLACEHOLDER, WEBUI_SERVICE_HTTP_PORT, WEBUI_SERVICE_HTTP_PORT_END,
        WEBUI_SERVICE_LOOPBACK_HOST, WEBUI_STATIC_DIR_ENV, build_webui_static_response,
        redact_webui_service_rpc_secrets, webui_service_discovery_snapshot_path,
        webui_service_http_bind_ports_from_override, webui_service_http_default_bind_ports,
        webui_service_origin_text_is_allowed, webui_service_ready_discovery_snapshot,
        webui_service_request_token_values_are_valid,
        webui_service_required_origin_text_is_allowed,
        webui_service_rpc_secret_key_should_be_redacted, webui_service_token_from_query,
        webui_static_file_path_for_request, webui_static_mime_type, webui_static_relative_path,
        webui_static_root_from_candidates,
    };
    use serde_json::{Value, json};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Verifies browser-facing RPC payloads redact nested secret fields.
    /// 验证浏览器侧 RPC 载荷会递归脱敏嵌套敏感字段。
    #[test]
    fn browser_rpc_redacts_nested_secret_fields() {
        let mut payload = json!({
            "aiCapabilityVendors": [
                {
                    "vendorId": "minimax",
                    "apiKey": "real-secret",
                    "targetChoices": [
                        {
                            "label": "safe"
                        }
                    ]
                }
            ],
            "vmm": {
                "grpcApiKey": "grpc-secret",
                "grpcTarget": "http://127.0.0.1"
            },
            "empty": {
                "api_key": ""
            }
        });

        redact_webui_service_rpc_secrets(&mut payload);

        assert_eq!(
            payload
                .pointer("/aiCapabilityVendors/0/apiKey")
                .and_then(Value::as_str),
            Some(WEBUI_REDACTED_SECRET_PLACEHOLDER)
        );
        assert_eq!(
            payload.pointer("/vmm/grpcApiKey").and_then(Value::as_str),
            Some(WEBUI_REDACTED_SECRET_PLACEHOLDER)
        );
        assert_eq!(
            payload.pointer("/empty/api_key").and_then(Value::as_str),
            Some("")
        );
    }

    /// Verifies the browser-facing secret key matcher covers current settings fields.
    /// 验证浏览器侧敏感字段匹配器覆盖当前设置字段。
    #[test]
    fn browser_rpc_secret_key_matcher_covers_settings_fields() {
        for key in ["apiKey", "api_key", "grpcApiKey", "grpc_api_key"] {
            assert!(webui_service_rpc_secret_key_should_be_redacted(key));
        }
        assert!(!webui_service_rpc_secret_key_should_be_redacted("baseUrl"));
        assert!(!webui_service_rpc_secret_key_should_be_redacted(
            "targetChoices"
        ));
    }

    /// Verifies WebUI static route paths cannot traverse outside the bundle directory.
    /// 验证 WebUI 静态路由路径不能穿越到包目录外。
    #[test]
    fn webui_static_relative_path_rejects_traversal() {
        assert!(webui_static_relative_path("/../secret.txt").is_err());
        assert!(webui_static_relative_path("/assets/..\\secret.txt").is_err());
        assert!(webui_static_relative_path("/C:/Windows/system.ini").is_err());
    }

    /// Verifies WebUI static route paths normalize common safe asset paths.
    /// 验证 WebUI 静态路由路径会规范化常见安全资源路径。
    #[test]
    fn webui_static_relative_path_accepts_safe_assets() {
        assert_eq!(
            webui_static_relative_path("/").unwrap(),
            PathBuf::from("index.html")
        );
        assert_eq!(
            webui_static_relative_path("/assets/app.js").unwrap(),
            PathBuf::from("assets").join("app.js")
        );
    }

    /// Verifies WebUI static assets get browser-safe MIME types.
    /// 验证 WebUI 静态资源会得到浏览器安全 MIME 类型。
    #[test]
    fn webui_static_mime_type_covers_vite_assets() {
        assert_eq!(
            webui_static_mime_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            webui_static_mime_type(Path::new("assets/app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            webui_static_mime_type(Path::new("assets/app.css")),
            "text/css; charset=utf-8"
        );
    }

    /// Verifies WebUI static file resolution serves direct files and SPA fallback.
    /// 验证 WebUI 静态文件解析会提供直接文件和 SPA 回退。
    #[test]
    fn webui_static_file_path_resolves_direct_files_and_spa_fallback() {
        let root = temporary_service_api_test_directory("webui-static-path");
        fs::create_dir_all(root.join("assets")).expect("create assets directory");
        fs::write(root.join("index.html"), "<html></html>").expect("write index");
        fs::write(root.join("assets").join("app.js"), "console.log(1);").expect("write js");

        assert_eq!(
            webui_static_file_path_for_request(&root, "/assets/app.js").unwrap(),
            Some(root.join("assets").join("app.js"))
        );
        assert_eq!(
            webui_static_file_path_for_request(&root, "/conversation/abc").unwrap(),
            Some(root.join("index.html"))
        );
        assert_eq!(
            webui_static_file_path_for_request(&root, "/assets/missing.js").unwrap(),
            None
        );

        fs::remove_dir_all(root).expect("remove temp directory");
    }

    /// Verifies WebUI static response builder returns framework-neutral HTTP data.
    /// 验证 WebUI 静态响应构建器会返回框架无关 HTTP 数据。
    #[test]
    fn webui_static_response_builder_returns_neutral_http_data() {
        let root = temporary_service_api_test_directory("webui-static-response");
        fs::create_dir_all(&root).expect("create static root");
        fs::write(root.join("index.html"), "<html>ok</html>").expect("write index");

        let response = build_webui_static_response(
            Some(&root),
            "/",
            "no-cache",
            "Vulcan WebUI static bundle is not available.",
        );
        assert_eq!(response.status, 200);
        assert!(response.body.starts_with(b"<html>"));
        assert!(
            response
                .headers
                .iter()
                .any(|(key, value)| key == "cache-control" && value == "no-cache")
        );

        let missing = build_webui_static_response(None, "/", "no-cache", "missing");
        assert_eq!(missing.status, 404);
        assert_eq!(String::from_utf8(missing.body).unwrap(), "missing");

        fs::remove_dir_all(root).expect("remove temp directory");
    }

    /// Verifies WebUI static root resolution prefers a valid override directory.
    /// 验证 WebUI 静态根目录解析会优先使用有效覆盖目录。
    #[test]
    fn webui_static_root_prefers_valid_override_directory() {
        let override_root = temporary_service_api_test_directory("webui-static-override");
        let fallback_root = temporary_service_api_test_directory("webui-static-fallback");
        fs::create_dir_all(&override_root).expect("create override root");
        fs::create_dir_all(&fallback_root).expect("create fallback root");

        assert_eq!(
            webui_static_root_from_candidates(override_root.to_str(), Some(fallback_root.clone())),
            Some(override_root.clone())
        );
        assert_eq!(WEBUI_STATIC_DIR_ENV, "VULCAN_WEBUI_STATIC_DIR");

        fs::remove_dir_all(override_root).expect("remove override root");
        fs::remove_dir_all(fallback_root).expect("remove fallback root");
    }

    /// Verifies WebUI static root resolution falls back when override is missing.
    /// 验证 WebUI 静态根目录解析会在覆盖目录缺失时回退。
    #[test]
    fn webui_static_root_falls_back_when_override_is_missing() {
        let fallback_root = temporary_service_api_test_directory("webui-static-fallback-only");
        fs::create_dir_all(&fallback_root).expect("create fallback root");

        assert_eq!(
            webui_static_root_from_candidates(Some("  "), Some(fallback_root.clone())),
            Some(fallback_root.clone())
        );
        assert_eq!(
            webui_static_root_from_candidates(Some("Z:/definitely/not/here"), None),
            None
        );

        fs::remove_dir_all(fallback_root).expect("remove fallback root");
    }

    /// Verifies browser-facing CORS only allows loopback HTTP and trusted Tauri origins.
    /// 验证浏览器侧 CORS 仅允许回环 HTTP 与受信任 Tauri 来源。
    #[test]
    fn webui_service_origin_allows_only_loopback_http_and_tauri_origins() {
        for origin in [
            "http://localhost:5173",
            "https://localhost:443",
            "http://127.0.0.1:5173",
            "http://[::1]:5173",
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
        ] {
            assert!(webui_service_origin_text_is_allowed(origin));
        }

        for origin in [
            "http://example.com",
            "https://evil.localhost.example",
            "file://localhost",
            "tauri://example.com",
            "http://evil.tauri.localhost",
            "http://not-tauri.localhost",
        ] {
            assert!(!webui_service_origin_text_is_allowed(origin));
        }
    }

    /// Verifies browser WebSocket origins must be present before they are accepted.
    /// 验证浏览器 WebSocket 来源必须存在才会被接受。
    #[test]
    fn webui_service_required_origin_rejects_missing_origin() {
        assert!(webui_service_required_origin_text_is_allowed(Some(
            "http://127.0.0.1:5173"
        )));
        assert!(!webui_service_required_origin_text_is_allowed(None));
        assert!(!webui_service_required_origin_text_is_allowed(Some(
            "http://example.com"
        )));
    }

    /// Verifies browser service token validation accepts header and query tokens.
    /// 验证浏览器服务令牌校验接受请求头与查询参数中的令牌。
    #[test]
    fn webui_service_token_validation_accepts_header_and_query() {
        let expected = "abc 123";

        assert!(webui_service_request_token_values_are_valid(
            Some("abc 123"),
            None,
            expected
        ));
        assert!(webui_service_request_token_values_are_valid(
            None,
            Some("token=abc%20123"),
            expected
        ));
        assert!(webui_service_request_token_values_are_valid(
            None,
            Some("serviceToken=abc+123"),
            expected
        ));
        assert!(!webui_service_request_token_values_are_valid(
            Some("wrong"),
            Some("token=wrong"),
            expected
        ));
    }

    /// Verifies browser service token query parser decodes supported parameter names.
    /// 验证浏览器服务令牌查询解析器会解码受支持的参数名。
    #[test]
    fn webui_service_token_query_parser_decodes_values() {
        assert_eq!(
            webui_service_token_from_query("token=abc%20123&x=1"),
            Some(String::from("abc 123"))
        );
        assert_eq!(
            webui_service_token_from_query("x=1&serviceToken=abc%2B123"),
            Some(String::from("abc+123"))
        );
        assert_eq!(webui_service_token_from_query("x=1"), None);
    }

    /// Verifies default browser service port discovery keeps the documented range.
    /// 验证默认浏览器服务端口发现保持文档约定范围。
    #[test]
    fn webui_service_default_bind_ports_cover_discovery_range() {
        let ports = webui_service_http_default_bind_ports();

        assert_eq!(ports.first(), Some(&WEBUI_SERVICE_HTTP_PORT));
        assert_eq!(ports.last(), Some(&WEBUI_SERVICE_HTTP_PORT_END));
        assert_eq!(
            ports.len(),
            usize::from(WEBUI_SERVICE_HTTP_PORT_END - WEBUI_SERVICE_HTTP_PORT + 1)
        );
    }

    /// Verifies browser service port override accepts only valid nonzero ports.
    /// 验证浏览器服务端口覆盖只接受有效的非零端口。
    #[test]
    fn webui_service_bind_ports_accept_valid_override_only() {
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some(" 19000 ")),
            vec![19000]
        );
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some("0")),
            webui_service_http_default_bind_ports()
        );
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some("not-a-port")),
            webui_service_http_default_bind_ports()
        );
    }

    /// Verifies browser service discovery path prefers home and falls back to temp.
    /// 验证浏览器服务发现路径优先使用主目录并在缺失时回退临时目录。
    #[test]
    fn webui_service_discovery_path_uses_home_or_temp_root() {
        let home = PathBuf::from("C:/Users/example");
        let temp = PathBuf::from("C:/Temp");

        assert_eq!(
            webui_service_discovery_snapshot_path(Some(home.clone()), temp.clone()),
            home.join(".vulcan").join("code").join("webui-service.json")
        );
        assert_eq!(
            webui_service_discovery_snapshot_path(None, temp.clone()),
            temp.join(".vulcan").join("code").join("webui-service.json")
        );
    }

    /// Verifies browser service discovery snapshots keep URL and host fields consistent.
    /// 验证浏览器服务发现快照保持 URL 与主机字段一致。
    #[test]
    fn webui_service_ready_discovery_snapshot_builds_consistent_url() {
        let snapshot =
            webui_service_ready_discovery_snapshot(17891, 42, "123456789", "token-value");

        assert_eq!(snapshot.status, "ready");
        assert_eq!(snapshot.host, WEBUI_SERVICE_LOOPBACK_HOST);
        assert_eq!(snapshot.base_url, "http://127.0.0.1:17891");
        assert_eq!(snapshot.port, 17891);
        assert_eq!(snapshot.pid, 42);
        assert_eq!(snapshot.started_at, "123456789");
        assert_eq!(snapshot.service_token, "token-value");
    }

    /// Creates a unique temporary directory path for service-api tests.
    /// 为 service-api 测试创建唯一临时目录路径。
    fn temporary_service_api_test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-service-api-{label}-{unique}"))
    }
}

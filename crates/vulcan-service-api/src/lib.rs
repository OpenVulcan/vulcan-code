//! Shared service API types for Vulcan clients.
//! Vulcan 客户端共享的服务 API 类型。

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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
    /// Currently selected conversation identifier.
    /// 当前选中的会话标识。
    pub selected_conversation_id: Option<String>,
    /// Conversation messages shown in the main timeline.
    /// 主时间线展示的会话消息。
    pub messages: Vec<ConversationMessage>,
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
    /// AI capability route summaries shown by the composer quick controls.
    /// 输入区快捷控制展示的 AI 能力路由摘要。
    #[serde(default)]
    pub ai_capabilities: Vec<AiCapabilitySummary>,
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
    /// Static persistent injection cards.
    /// 持续生效的静态注入卡片。
    pub static_cards: Vec<ContextInjectionCardSummary>,
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
    /// Command display name.
    /// 命令显示名称。
    pub command: String,
    /// Live command status.
    /// 实时命令状态。
    pub status: String,
    /// Recent command output preview.
    /// 最近命令输出预览。
    pub preview: String,
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
}

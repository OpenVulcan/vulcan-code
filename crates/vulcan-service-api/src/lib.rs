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
    /// Setting sections shown in the settings area.
    /// 设置区域展示的设置分组。
    pub settings: Vec<SettingSectionSummary>,
    /// Provider summaries shown in the provider area.
    /// Provider 区域展示的供应商摘要。
    pub providers: Vec<ProviderSummary>,
    /// Reasoning selector view resolved from the active provider and model.
    /// 根据当前激活 Provider 与模型解析出的思考选择器视图。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_selection: Option<ReasoningSelectionSummary>,
    /// AI capability route summaries shown by the composer quick controls.
    /// 输入区快捷控制展示的 AI 能力路由摘要。
    #[serde(default)]
    pub ai_capabilities: Vec<AiCapabilitySummary>,
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
    /// Memory summaries shown in the VMM area.
    /// VMM 区域展示的记忆摘要。
    pub memories: Vec<MemorySummary>,
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

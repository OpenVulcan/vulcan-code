use super::*;

/// Provides the first WebUI workbench snapshot.
/// 提供第一版 WebUI 工作台快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchSnapshot {
    /// Monotonic workbench revision shared by full snapshots and delta events.
    /// 全量快照与增量事件共享的工作台单调版本号。
    #[serde(default)]
    pub revision: u64,
    /// Active application version.
    /// 当前应用版本。
    pub app_version: String,
    /// WebUI locale resolved from the shared configuration file.
    /// 从共享配置文件解析出的 WebUI 语言。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_locale: Option<String>,
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
    /// Thinking-output display mode resolved from the shared configuration file.
    /// 从共享配置文件解析出的思考输出展示模式。
    #[serde(default)]
    pub thinking_display_mode: String,
    /// Git commit dialog settings resolved from the shared configuration file.
    /// 从共享配置文件解析出的 Git 提交弹窗设置。
    #[serde(default)]
    pub git_commit: GitCommitSettingsSummary,
    /// Host-owned tool settings resolved from the shared configuration file.
    /// 从共享配置文件解析出的宿主自有工具设置。
    #[serde(default)]
    pub host_tools: HostToolsSettingsSummary,
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
    /// Pending prompt queues retained for every conversation that has one.
    /// 为每个存在待发送队列的会话保留的待发送提示词队列。
    #[serde(default)]
    pub pending_prompts_by_conversation: Vec<PendingPromptsChangedDelta>,
    /// Oldest pending ask-tool question waiting for user input.
    /// 当前等待用户输入的最早 ask 工具问题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<PendingQuestionSnapshot>,
    /// Pending ask-tool questions grouped by their owning conversation.
    /// 按所属会话保留的 ask 工具待回答问题。
    #[serde(default)]
    pub pending_questions: Vec<PendingQuestionSnapshot>,
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
    /// Current session VMM memory policy shown in the composer controls.
    /// 当前会话输入区控制项展示的 VMM 记忆策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_vmm_policy: Option<SessionVmmPolicySummary>,
    /// Session addon projections retained for every conversation that has addon state.
    /// 为每个存在附属状态的会话保留的会话附属投影。
    #[serde(default)]
    pub session_addons_by_conversation: Vec<SessionAddonsChangedDelta>,
    /// VMM policy projections retained for every visible conversation.
    /// 为每个可见会话保留的 VMM 策略投影。
    #[serde(default)]
    pub session_vmm_policies_by_conversation: Vec<SessionVmmPolicyByConversation>,
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
    /// Context injection projections retained by conversation id.
    /// 按会话标识保留的上下文注入投影。
    #[serde(default)]
    pub context_injection_by_conversation: Vec<ContextInjectionChangedDelta>,
    /// Context token usage snapshot projected from the latest execution preparation.
    /// 从最近一次执行准备投影出的上下文 Token 占用快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsageSnapshot>,
}

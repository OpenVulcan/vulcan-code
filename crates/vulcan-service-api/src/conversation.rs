use super::*;

/// Identifies the terminal outcome of one fully settled conversation run.
/// 标识一次完全结束的会话运行终态结果。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConversationRunOutcome {
    /// The conversation run completed successfully.
    /// 会话运行已成功完成。
    Completed,
    /// The conversation run ended with an execution failure.
    /// 会话运行因执行失败而结束。
    Failed,
}

/// Describes one platform-neutral conversation run settlement event.
/// 描述一个与平台无关的会话运行最终结束事件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRunSettledEvent {
    /// Stable event identifier used by clients for idempotent consumption.
    /// 客户端用于幂等消费的稳定事件标识。
    pub event_id: String,
    /// Conversation identifier owning the settled run.
    /// 拥有本次终态运行的会话标识。
    pub conversation_id: String,
    /// Optional project identifier resolved from the authoritative workspace projection.
    /// 从权威工作区投影解析出的可选项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Exact stream task identifier that owned the terminal report.
    /// 持有终态报告的精确流任务标识。
    pub run_id: String,
    /// Strict terminal result of the settled run.
    /// 本次终态运行的严格结果。
    pub outcome: ConversationRunOutcome,
    /// Unix-millisecond timestamp text captured when settlement was emitted.
    /// 发出终态事件时捕获的 Unix 毫秒时间戳文本。
    pub occurred_at: String,
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
    /// Highest-priority action marker that belongs to this conversation.
    /// 属于该会话的最高优先级待处理动作标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<ConversationAttentionSummary>,
    /// Optional owning project identifier.
    /// 可选所属项目标识。
    #[serde(default)]
    pub project_id: Option<String>,
}

/// Describes one conversation-level action marker for cross-session navigation.
/// 描述用于跨会话导航的单个会话级动作标记。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAttentionSummary {
    /// Stable marker kind such as `ask`, `plan_confirm`, or `unread`.
    /// 稳定标记类型，例如 `ask`、`plan_confirm` 或 `unread`。
    pub kind: String,
    /// Numeric priority used by clients when multiple markers exist.
    /// 多个标记同时存在时客户端使用的数字优先级。
    pub priority: u8,
    /// Optional request or artifact identifier that owns this marker.
    /// 拥有该标记的可选请求或产物标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Optional human-readable label for tooltips and assistive text.
    /// 用于提示与辅助文本的可选人类可读标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Unix-millisecond timestamp text for ordering diagnostics.
    /// 用于排序诊断的 Unix 毫秒时间戳文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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

/// Identifies one phase in the pre-dispatch media transfer lifecycle.
/// 标识分派前媒体传输生命周期中的一个阶段。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MediaUploadPhase {
    /// The host is validating and opening the local source.
    /// 宿主正在校验并打开本地源。
    Preparing,
    /// The source is being uploaded or read for an inline provider payload.
    /// 源文件正在上传，或正在为内联供应商载荷读取。
    Uploading,
    /// Source bytes were sent and the provider is confirming a reusable reference.
    /// 源字节已经发送，供应商正在确认可复用引用。
    AwaitingProviderConfirmation,
    /// The source is ready and the actual model request may start.
    /// 源文件已就绪，可以开始实际模型请求。
    Completed,
    /// The user cancelled before the model request was sent.
    /// 用户在模型请求发送前取消了本次操作。
    Cancelled,
    /// The source could not be prepared.
    /// 源文件准备失败。
    Failed,
}

/// Carries one ephemeral media-transfer progress update for submission preparation or a running media tool.
/// 携带提交准备阶段或运行中媒体工具的一条临时媒体传输进度更新。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaUploadProgressChangedDelta {
    /// Unique transfer identity; cancellable pre-dispatch uploads reuse the client submission identity.
    /// 唯一传输标识；可取消的分派前上传复用客户端提交标识。
    pub client_submission_id: String,
    /// Existing conversation identity, or null while a new draft is being prepared.
    /// 已有会话标识；新草稿准备期间为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Stable managed attachment identity when the transfer belongs to a composer attachment.
    /// 当传输属于输入区附件时使用的稳定托管附件身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    /// Exact provider selected for the pending request.
    /// 待处理请求选定的精确供应商。
    pub provider_id: String,
    /// Exact service entry selected for the pending request.
    /// 待处理请求选定的精确服务入口。
    pub service_entry_id: String,
    /// Exact credential connection selected without exposing credential material.
    /// 不暴露凭据材料的精确凭据连接。
    pub connection_id: String,
    /// Exact model selected for the pending request.
    /// 待处理请求选定的精确模型。
    pub model_id: String,
    /// Exact provider-neutral protocol profile frozen for media preparation.
    /// 媒体准备冻结的精确供应商无关协议 Profile。
    pub protocol_profile_id: String,
    /// Manifest revision frozen for media preparation.
    /// 媒体准备冻结的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Runtime manifest generation frozen for media preparation.
    /// 媒体准备冻结的运行时 Manifest 代际。
    pub manifest_generation: u64,
    /// Content-addressed revision of the complete frozen route.
    /// 完整冻结路由的内容寻址修订号。
    pub route_revision: String,
    /// Stable provider-neutral operation owning the transfer.
    /// 拥有本次传输的稳定供应商无关 operation。
    pub operation_id: String,
    /// Exact manifest upload channel or the closed host inline channel identity.
    /// 精确 Manifest 上传通道或封闭宿主内联通道身份。
    pub upload_channel_id: String,
    /// User-visible source file name.
    /// 面向用户展示的源文件名。
    pub file_name: String,
    /// Stable media kind such as image, audio, or video.
    /// 稳定媒体类型，例如 image、audio 或 video。
    pub media_kind: String,
    /// Zero-based attachment index within the current submission.
    /// 当前提交中的从零开始附件索引。
    pub file_index: usize,
    /// Total attachment count participating in preparation.
    /// 参与准备的附件总数。
    pub file_count: usize,
    /// Current transfer lifecycle phase.
    /// 当前传输生命周期阶段。
    pub phase: MediaUploadPhase,
    /// Bytes transferred or pre-read so far.
    /// 当前已传输或预读取的字节数。
    pub bytes_sent: u64,
    /// Optional source byte length.
    /// 可选的源文件字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// Controlled failure detail for the failed phase.
    /// 失败阶段携带的受控错误详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Whether the active transfer accepts the existing cancellation RPC.
    /// 当前活动传输是否接受现有取消 RPC。
    #[serde(default)]
    pub cancellable: bool,
}

/// Classifies one ephemeral tool-request formation snapshot.
/// 对单条瞬态工具请求构造快照进行分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRequestProgressState {
    /// The provider is still streaming the tool argument document.
    /// Provider 仍在流式发送工具参数文档。
    Receiving,
    /// One complete JSON root has been assembled even though the surrounding Provider response remains open.
    /// 已组装出一个完整 JSON 根值，但外围 Provider 响应仍未结束。
    ArgumentsAssembled,
    /// The provider completed the tool argument document and execution has not started yet.
    /// Provider 已完成工具参数文档，但工具执行尚未开始。
    Ready,
}

/// Carries the latest safe argument-formation snapshot for one tool request.
/// 携带单个工具请求最新的安全参数构造快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequestProgressSnapshot {
    /// Stable host-side request-stream identity.
    /// 稳定的宿主侧请求流标识。
    pub request_stream_id: String,
    /// Canonical tool-call identity known at this revision.
    /// 当前修订已知的规范工具调用标识。
    pub tool_call_id: String,
    /// Exact public tool name frozen for this request.
    /// 当前请求冻结的精确公开工具名称。
    pub tool_name: String,
    /// Current closed request-formation state.
    /// 当前封闭的请求构造状态。
    pub state: ToolRequestProgressState,
    /// Exact UTF-8 byte count received so far.
    /// 当前已接收的精确 UTF-8 字节数。
    pub received_bytes: u64,
    /// Exact Unicode character count received so far.
    /// 当前已接收的精确 Unicode 字符数。
    pub received_characters: u64,
    /// Monotonic request-local revision.
    /// 请求局部的单调修订号。
    pub request_revision: u64,
    /// Optional parser-sanitized preview owned by the streaming contract.
    /// 流式契约持有的可选解析器净化预览。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<serde_json::Value>,
}

/// Replaces or clears one ephemeral tool-request formation snapshot.
/// 替换或清除单个瞬态工具请求构造快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequestProgressChangedDelta {
    /// Conversation identity owning the request.
    /// 持有当前请求的会话标识。
    pub conversation_id: String,
    /// Stable request-stream identity used for replacement and clearing.
    /// 用于替换与清除的稳定请求流标识。
    pub request_stream_id: String,
    /// Latest safe snapshot, or null when validation fails or the request is aborted.
    /// 最新安全快照；校验失败或请求中止后为空。
    #[serde(default)]
    pub progress: Option<ToolRequestProgressSnapshot>,
}

/// Describes one partial workbench update emitted on hot paths.
/// 描述热路径上发出的单条局部工作台更新。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "payload")]
pub enum WorkbenchDelta {
    /// Phase-only update for a running conversation.
    /// 针对运行中会话的纯阶段更新。
    UpdateConversationRunPhase(UpdateConversationRunPhaseDelta),
    /// Pending prompt queue replacement for one conversation.
    /// 单个会话的待发送提示词队列替换增量。
    PendingPromptsChanged(PendingPromptsChangedDelta),
    /// Conversation summary upsert for sidebar and selection state.
    /// 用于侧栏与选择状态的会话摘要写入增量。
    ConversationSummaryChanged(ConversationSummaryChangedDelta),
    /// Committed conversation archive transition for sidebar lifecycle state.
    /// 用于侧栏生命周期状态的已提交会话归档转换增量。
    ConversationArchived(ConversationArchivedDelta),
    /// Committed multi-conversation archive transition for one project lifecycle action.
    /// 单次项目生命周期操作提交的多会话归档转换增量。
    ConversationsArchived(ConversationsArchivedDelta),
    /// Committed project removal transition with every owned conversation archive.
    /// 包含全部所属会话归档结果的已提交项目移除转换增量。
    ProjectRemoved(ProjectRemovedDelta),
    /// Pending ask-question replacement for the global ask overlay.
    /// 全局 ask 浮层的待回答问题替换增量。
    PendingQuestionChanged(PendingQuestionChangedDelta),
    /// Command-session list replacement for terminal side panels.
    /// 终端侧面板命令会话列表替换增量。
    CommandSessionsChanged(CommandSessionsChangedDelta),
    /// Subagent navigation replacement for floating and side panels.
    /// 用于浮动面板与右侧页签的子代理导航替换增量。
    SubagentNavigationChanged(SubagentNavigationChangedDelta),
    /// Context-usage replacement for the composer usage indicator.
    /// 用于输入区上下文用量指示器的上下文用量替换增量。
    ContextUsageChanged(ContextUsageChangedDelta),
    /// Context-injection replacement for the selected-session memory panel.
    /// 用于当前选中会话记忆面板的上下文注入替换增量。
    ContextInjectionChanged(ContextInjectionChangedDelta),
    /// Session addon-state replacement for goal, todolist, and Plan mode surfaces.
    /// 面向目标、待办与计划模式界面的会话附属状态替换增量。
    SessionAddonsChanged(Box<SessionAddonsChangedDelta>),
    /// Complete Agent Skills catalog replacement after one effective filesystem change.
    /// Agent Skills 文件系统发生一次有效变化后的完整目录替换增量。
    AgentSkillsCatalogChanged(AgentSkillsCatalogChangedDelta),
    /// Complete LuaSkills catalog and plugin replacement after one committed mutation.
    /// LuaSkills 完成一次已提交变更后的完整目录与插件替换增量。
    LuaSkillsCatalogChanged(Box<LuaSkillsCatalogChangedDelta>),
    /// Complete subagent catalog replacement after one committed mutation.
    /// 智能体目录完成一次已提交变更后的完整替换增量。
    SubagentCatalogChanged(Box<SubagentCatalogChangedDelta>),
    /// Atomic plugin, Agent Skills, and subagent catalog replacement after one plugin lifecycle mutation.
    /// 插件生命周期完成一次变更后，原子替换插件、Agent Skills 与智能体目录。
    PluginCatalogChanged(Box<PluginCatalogChangedDelta>),
    /// Project-scoped settings invalidation for dialogs that load data outside the workbench snapshot.
    /// 项目级设置失效通知，用于重新加载工作台快照之外的弹窗数据。
    ProjectSettingsChanged(ProjectSettingsChangedDelta),
    /// Native LuaSkills managed-runtime progress replacement.
    /// LuaSkills 原生受管运行时进度替换增量。
    LuaSkillsManagedRuntimeProgressChanged(LuaSkillsManagedRuntimeProgressChangedDelta),
    /// Ephemeral media upload progress for a submission that has not been materialized yet.
    /// 尚未物化提交的临时媒体上传进度增量。
    MediaUploadProgressChanged(MediaUploadProgressChangedDelta),
    /// Ephemeral replace-in-place argument progress for one active tool request.
    /// 单个活动工具请求使用的瞬态原地替换式参数进度。
    ToolRequestProgressChanged(ToolRequestProgressChangedDelta),
    /// Ephemeral replace-in-place reasoning status for one active conversation.
    /// 单个活动会话使用的瞬态原地替换式推理状态。
    TransientReasoningStatusChanged(TransientReasoningStatusChangedDelta),
}

/// Classifies the visible form of one transient reasoning status.
/// 对单条瞬态推理状态的可见形式进行分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TransientReasoningStatusKind {
    /// Readable provider-generated reasoning summary.
    /// 供应商生成的可读推理摘要。
    Summary,
    /// Opaque encrypted reasoning represented by progress only.
    /// 仅通过进度表示的不透明加密推理。
    Encrypted,
}

/// Carries one readable transient reasoning summary node.
/// 携带一个可读瞬态推理摘要节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransientReasoningSummaryItemSnapshot {
    /// Stable summary identity used for in-place updates.
    /// 用于原地更新的稳定摘要标识。
    pub item_id: String,
    /// Latest complete text for this summary node.
    /// 当前摘要节点的最新完整文本。
    pub text: String,
}

/// Carries one transient reasoning status value displayed outside the transcript.
/// 携带一条显示在转录之外的瞬态推理状态值。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransientReasoningStatusSnapshot {
    /// Presentation kind selected by the response profile.
    /// 由响应 Profile 选择的展示类型。
    pub kind: TransientReasoningStatusKind,
    /// Ordered readable summary nodes when available.
    /// 可用时的有序可读摘要节点。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summaries: Vec<TransientReasoningSummaryItemSnapshot>,
    /// Latest cumulative reasoning-token count when available.
    /// 可用时的最新累计推理 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

/// Replaces or clears one conversation's non-persistent reasoning status.
/// 替换或清除单个会话的非持久化推理状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransientReasoningStatusChangedDelta {
    /// Stable conversation identifier owning the active request.
    /// 拥有当前活动请求的稳定会话标识。
    pub conversation_id: String,
    /// Latest transient status, or `None` to clear it immediately.
    /// 最新瞬态状态；为 `None` 时立即清除。
    pub status: Option<TransientReasoningStatusSnapshot>,
}

/// Replaces the Agent Skills domain without resending the full workbench snapshot.
/// 在不重发完整工作台快照的情况下替换 Agent Skills 领域。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillsCatalogChangedDelta {
    /// Complete Agent Skills catalog committed at the envelope generation.
    /// 在信封版本提交的完整 Agent Skills 目录。
    pub agent_skills: AgentSkillsCatalogSummary,
}

/// Replaces the LuaSkills and plugin domains from one atomic catalog projection.
/// 使用同一份原子目录投影替换 LuaSkills 与插件领域。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCatalogChangedDelta {
    /// Complete LuaSkills catalog committed at the envelope generation.
    /// 在信封版本提交的完整 LuaSkills 目录。
    pub lua_skills: LuaSkillsCatalogSummary,
    /// Plugin rows derived from the same committed catalog.
    /// 从同一份已提交目录派生的插件行。
    pub plugins: Vec<PluginSummary>,
}

/// Replaces the subagent catalog without resending unrelated workbench domains.
/// 在不重发无关工作台领域的情况下替换智能体目录。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogChangedDelta {
    /// Complete subagent catalog committed at the envelope generation.
    /// 在信封版本提交的完整智能体目录。
    pub subagent_catalog: SubagentCatalogSummary,
}

/// Replaces every plugin-owned workbench domain from one immutable plugin catalog snapshot.
/// 使用同一份不可变插件目录快照替换全部插件所有的工作台领域。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogChangedDelta {
    /// Complete LuaSkills catalog captured during the same plugin transaction.
    /// 在同一插件事务中捕获的完整 LuaSkills 目录。
    pub lua_skills: LuaSkillsCatalogSummary,
    /// Plugin rows projected from the committed plugin catalog snapshot.
    /// 从已提交插件目录快照投影出的插件行。
    pub plugins: Vec<PluginSummary>,
    /// Complete Agent Skills catalog containing the same snapshot's plugin skills.
    /// 包含同一快照插件 Skill 的完整 Agent Skills 目录。
    pub agent_skills: AgentSkillsCatalogSummary,
    /// Complete subagent catalog containing the same snapshot's plugin agents.
    /// 包含同一快照插件 Agent 的完整智能体目录。
    pub subagent_catalog: SubagentCatalogSummary,
}

/// Identifies one independently loaded project-settings domain.
/// 标识一个独立加载的项目设置领域。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ProjectSettingsDomain {
    /// Project-scoped VMM configuration.
    /// 项目级 VMM 配置。
    Vmm,
    /// Project-scoped native code-semantic index settings.
    /// 项目级原生代码语义索引设置。
    Vce,
    /// Project-scoped Agent Skill overrides.
    /// 项目级 Agent Skill 覆盖。
    AgentSkills,
}

/// Invalidates one independently loaded settings domain for one exact project.
/// 使单个准确项目的一个独立加载设置领域失效。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsChangedDelta {
    /// Stable project identifier whose settings changed, or `None` when a global setting invalidates every project.
    /// 设置发生变化的稳定项目标识；全局设置使所有项目失效时为 `None`。
    pub project_id: Option<String>,
    /// Exact independently loaded settings domain that changed.
    /// 发生变化的准确独立加载设置领域。
    pub domain: ProjectSettingsDomain,
}

/// Replaces only the active LuaSkills managed-runtime installation progress.
/// 仅替换当前 LuaSkills 受管运行时安装进度。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsManagedRuntimeProgressChangedDelta {
    /// Current progress, or `None` after the operation reaches a terminal state.
    /// 当前进度；操作到达终态后为 `None`。
    pub progress: Option<LuaSkillsManagedRuntimeProgressSummary>,
}

/// Updates one conversation lifecycle phase without resending transcript rows.
/// 更新单个会话的生命周期阶段，避免重新发送转录行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationRunPhaseDelta {
    /// Stable conversation identifier receiving the phase update.
    /// 接收阶段更新的稳定会话标识。
    pub conversation_id: String,
    /// Current backend run phase for the conversation.
    /// 会话当前的后端运行阶段。
    pub run_phase: String,
    /// Optional backend status label paired with the phase.
    /// 与阶段配套的可选后端状态标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional relative update label for the conversation row.
    /// 会话行使用的可选相对更新时间标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated_label: Option<String>,
}

/// Replaces the visible pending prompt queue for one conversation.
/// 替换单个会话可见的待发送提示词队列。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPromptsChangedDelta {
    /// Stable conversation identifier receiving the queue projection.
    /// 接收队列投影的稳定会话标识。
    pub conversation_id: String,
    /// Current visible pending prompt summaries for the conversation.
    /// 当前会话可见的待发送提示词摘要。
    #[serde(default)]
    pub pending_prompts: Vec<PendingPromptSummary>,
}

/// Upserts one conversation summary and optionally selects it.
/// 写入单个会话摘要，并可选中该会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummaryChangedDelta {
    /// Current conversation summary projected for the sidebar.
    /// 当前投影到侧栏的会话摘要。
    pub conversation: ConversationSummary,
    /// Selected conversation id after applying this summary update.
    /// 应用该摘要更新后的选中会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_conversation_id: Option<String>,
}

/// Applies one committed conversation archive transition without replacing unrelated workbench domains.
/// 应用单个已提交会话归档转换，且不替换无关工作台领域。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationArchivedDelta {
    /// Stable identifier removed from the active conversation collection.
    /// 从活动会话集合移除的稳定会话标识。
    pub conversation_id: String,
    /// Authoritative archived summary when the session remains restorable in the archived collection.
    /// 当会话在归档集合中仍可恢复时对应的权威归档摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_conversation: Option<ConversationSummary>,
    /// Selected active conversation identifier after the archive transition.
    /// 归档转换完成后的选中活动会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_conversation_id: Option<String>,
}

/// Carries every conversation archive committed by one project-level operation.
/// 携带单次项目级操作提交的全部会话归档结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationsArchivedDelta {
    /// Exact conversation lifecycle transitions committed in the shared generation.
    /// 在同一 generation 中提交的精确会话生命周期转换。
    pub archived_conversations: Vec<ConversationArchivedDelta>,
}

/// Carries one committed project removal and every child conversation archive.
/// 携带单次已提交项目移除及其全部子会话归档结果。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemovedDelta {
    /// Exact active project identifier removed by the lifecycle transaction.
    /// 生命周期事务移除的精确活动项目标识。
    pub project_id: String,
    /// Authoritative active projects after removal and default-project repair.
    /// 移除及默认项目修复后的权威活动项目集合。
    pub projects: Vec<ProjectSummary>,
    /// Authoritative expanded project identifiers after removal.
    /// 移除后的权威展开项目标识集合。
    pub expanded_project_ids: Vec<String>,
    /// Exact child conversation archive transitions committed with the project removal.
    /// 与项目移除一同提交的精确子会话归档转换。
    pub archived_conversations: Vec<ConversationArchivedDelta>,
}

/// Replaces the active pending ask-question projection.
/// 替换当前活动的待回答 ask 问题投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionChangedDelta {
    /// Stable conversation identifier receiving the ask projection.
    /// 接收 ask 投影的稳定会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Current pending question, or null when no ask question is waiting.
    /// 当前待回答问题；没有等待中的 ask 问题时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<PendingQuestionSnapshot>,
    /// All pending ask-tool questions grouped by their own session ids.
    /// 按各自会话标识保留的全部 ask 工具待回答问题。
    #[serde(default)]
    pub pending_questions: Vec<PendingQuestionSnapshot>,
}

/// Replaces the command-session summaries shown by the workbench.
/// 替换工作台展示的命令会话摘要。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSessionsChangedDelta {
    /// Current command-session summaries projected from the live runtime.
    /// 从实时运行时投影出的当前命令会话摘要。
    #[serde(default)]
    pub command_sessions: Vec<CommandSessionSummary>,
}

/// Replaces the current subagent navigation snapshot.
/// 替换当前子代理导航快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentNavigationChangedDelta {
    /// Parent conversation identifier receiving this navigation projection.
    /// 接收该导航投影的父会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_conversation_id: Option<String>,
    /// Current subagent navigation, or null when no subagent panel should be shown.
    /// 当前子代理导航；不应展示子代理面板时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_navigation: Option<SubagentNavigationSnapshot>,
}

/// Carries the focused state needed after opening or closing a subagent panel.
/// 携带打开或关闭子代理面板后所需的聚焦状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPanelRouteResponse {
    /// Parent conversation id that owns this route response.
    /// 拥有本次路由响应的父会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_conversation_id: Option<String>,
    /// Conversation id that owns the returned message window.
    /// 拥有返回消息窗口的会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_conversation_id: Option<String>,
    /// Current subagent navigation, or null when no subagent panel is active.
    /// 当前子代理导航；没有活动子代理面板时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_navigation: Option<SubagentNavigationSnapshot>,
    /// Latest workbench revision after applying the route update.
    /// 应用路由更新后的最新工作台版本。
    pub revision: u64,
}

/// Replaces the current context-usage projection.
/// 替换当前上下文用量投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageChangedDelta {
    /// Current context usage, or null when no active usage is available.
    /// 当前上下文用量；没有活动用量时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsageSnapshot>,
}

/// Replaces the selected-session context injection memory-panel projection.
/// 替换当前选中会话的上下文注入记忆面板投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionChangedDelta {
    /// Stable conversation identifier that owns the memory-panel projection.
    /// 拥有记忆面板投影的稳定会话标识。
    pub conversation_id: String,
    /// Current context injection projection, or null when no runtime state is available.
    /// 当前上下文注入投影；运行态不可用时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_injection: Option<ContextInjectionSnapshot>,
}

/// One token-usage snapshot projected for the live Provider request panel.
/// 实时 Provider 请求面板使用的一份 Token 用量快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionProviderUsageTokenSummary {
    /// Provider-reported input tokens when known.
    /// Provider 报告的输入 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<usize>,
    /// Normalized uncached input tokens when known.
    /// 规范化的非缓存输入 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncached_input_tokens: Option<usize>,
    /// Provider-reported cached input tokens when known.
    /// Provider 报告的缓存命中输入 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<usize>,
    /// Provider-reported cache-creation input tokens when known.
    /// Provider 报告的缓存写入输入 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<usize>,
    /// Provider-reported output tokens when known.
    /// Provider 报告的输出 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<usize>,
    /// Provider-reported reasoning tokens when known.
    /// Provider 报告的推理 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<usize>,
    /// Provider-reported total tokens when known.
    /// Provider 报告的总 Token，未知时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<usize>,
}

/// Describes the latest physical Provider request retained for live diagnostics.
/// 描述实时诊断保留的最近一次物理 Provider 请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionProviderUsageCallSummary {
    /// Stable physical Provider request identifier.
    /// 稳定的物理 Provider 请求标识。
    pub provider_call_id: String,
    /// Stable execution owner category.
    /// 稳定的执行归属类别。
    pub owner_kind: String,
    /// Request lifecycle status: in_flight, succeeded, or failed.
    /// 请求生命周期状态：进行中、成功或失败。
    pub status: String,
    /// Usage facts for this request when any were known.
    /// 此请求已知的用量事实。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<SessionProviderUsageTokenSummary>,
    /// Provider dispatch timestamp in Unix milliseconds.
    /// Provider 分发时间的 Unix 毫秒时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// Provider terminal timestamp in Unix milliseconds.
    /// Provider 终态时间的 Unix 毫秒时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    /// Stable redacted error category for failed requests.
    /// 失败请求的稳定脱敏错误类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
    /// Stable redacted error code for failed requests.
    /// 失败请求的稳定脱敏错误代码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Projects per-session Provider request counts and token usage at every physical request boundary.
/// 在每个物理请求边界投影会话级 Provider 请求计数与 Token 用量。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionProviderUsageSummary {
    /// Number of physical Provider requests dispatched for this session.
    /// 此会话已分发的物理 Provider 请求数量。
    pub provider_call_count: usize,
    /// Number of dispatched requests still awaiting terminal events.
    /// 仍在等待终态事件的已分发请求数量。
    pub in_flight_call_count: usize,
    /// Number of terminal requests without usable usage facts.
    /// 未提供可用用量事实的终态请求数量。
    pub unknown_usage_call_count: usize,
    /// Latest usable usage snapshot.
    /// 最近一份可用用量快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<SessionProviderUsageTokenSummary>,
    /// Cumulative usable usage across terminal requests.
    /// 全部终态请求的可用用量累计值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cumulative_usage: Option<SessionProviderUsageTokenSummary>,
    /// Latest physical request lifecycle projection.
    /// 最近一次物理请求的生命周期投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_call: Option<SessionProviderUsageCallSummary>,
    /// Monotonic usage revision used to reject stale UI projections.
    /// 用于拒绝过期 UI 投影的单调用量版本号。
    pub usage_revision: u64,
}

/// Exposes one fixed system context-compaction window node to the WebUI.
/// 向 WebUI 公开一个固定系统上下文压缩窗口节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompactionWindowNodeSummary {
    /// Stable backend-owned node identifier persisted by sessions.
    /// 由后台持有且被会话持久化的稳定节点标识。
    pub id: String,
    /// Current backend-authoritative token value for this node.
    /// 当前节点由后台权威持有的 Token 值。
    pub tokens: u64,
}

/// Replaces selected-session addon panels without rebuilding the full workbench.
/// 在不重建完整工作台的情况下替换当前会话附属面板状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAddonsChangedDelta {
    /// Stable conversation identifier receiving the addon projection.
    /// 接收附属状态投影的稳定会话标识。
    pub conversation_id: String,
    /// Current session goal, or null when no goal is active.
    /// 当前会话目标；没有活动目标时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_goal: Option<SessionGoalSummary>,
    /// Current session todolist, or null when no todolist is visible.
    /// 当前会话待办；没有可见待办时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_todolist: Option<SessionTodolistSummary>,
    /// Current Plan mode state, or null when Plan mode is inactive and no artifact exists.
    /// 当前计划模式状态；计划模式未启用且无产物时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_plan_mode: Option<SessionPlanModeSummary>,
    /// Current session VMM memory policy overrides.
    /// 当前会话 VMM 记忆策略覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_vmm_policy: Option<SessionVmmPolicySummary>,
    /// Current output-style override and the last safely applied snapshot.
    /// 当前输出风格覆盖与最近一次安全应用的快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_output_style: Option<SessionOutputStyleSummary>,
    /// Stable context-compaction window node selected by this session.
    /// 当前会话选择的稳定上下文压缩窗口节点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_context_compaction_window_node_id: Option<String>,
    /// Live physical Provider request counts and token usage for this session.
    /// 当前会话实时物理 Provider 请求计数与 Token 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_provider_usage: Option<Box<SessionProviderUsageSummary>>,
}

/// Describes one session's output-style selection and safe-boundary activation state.
/// 描述单个会话的输出风格选择与安全边界激活状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputStyleSummary {
    /// Explicit session override; null means inherit the global style.
    /// 会话显式覆盖；为空表示继承全局风格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_style_id: Option<String>,
    /// Desired style after applying session-over-global precedence.
    /// 按会话优先于全局规则解析出的目标风格。
    pub desired_style_id: String,
    /// Style identifier in the last prompt snapshot admitted at a safe boundary.
    /// 最近一次在安全边界接纳的提示词快照中的风格标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_style_id: Option<String>,
    /// Whether the desired style waits for a new session or compaction boundary.
    /// 目标风格是否等待新会话或上下文压缩边界。
    pub pending_activation: bool,
    /// Stable fingerprint of the applied prompt text.
    /// 已应用提示词正文的稳定指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_fingerprint: Option<u64>,
    /// Unix-millis text recording when the applied snapshot became effective.
    /// 已应用快照生效时间的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    /// User-facing explanation of when a pending selection will activate.
    /// 面向用户说明待激活选择何时生效。
    pub next_activation_boundary: String,
}

/// Requests a session-scoped output-style override change.
/// 请求修改会话级输出风格覆盖。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputStyleUpdateRequest {
    /// Target conversation identifier.
    /// 目标会话标识。
    pub conversation_id: String,
    /// Style identifier; null or empty clears the override and inherits global settings.
    /// 风格标识；为空或空字符串清除覆盖并继承全局设置。
    #[serde(default)]
    pub style_id: Option<String>,
}

/// Requests one session-scoped context-compaction window node selection.
/// 请求一次会话级上下文压缩窗口节点选择。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextCompactionWindowUpdateRequest {
    /// Target conversation identifier.
    /// 目标会话标识。
    pub conversation_id: String,
    /// Stable fixed node identifier; null clears the selection and restores the model default.
    /// 稳定固定节点标识；为空时清除选择并恢复模型默认值。
    #[serde(default)]
    pub node_id: Option<String>,
}

/// Describes the selected session's VMM memory policy override and effective state.
/// 描述当前会话的 VMM 记忆策略覆盖与最终状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionVmmPolicySummary {
    /// Final memory enablement for this exact session, independent of another client's selection.
    /// 当前准确会话的最终记忆启用状态，独立于其他客户端的选择。
    pub memory_enabled: bool,
    /// Precheck memory injection policy mode: inherit, enabled, or disabled.
    /// 前置记忆注入策略模式：继承、开启或关闭。
    pub precheck_mode: String,
    /// Final precheck memory injection enablement after all layers resolve.
    /// 所有层级解析后的最终前置记忆注入启用状态。
    pub precheck_enabled: bool,
    /// Completed-turn memory writeback policy mode: inherit, enabled, or disabled.
    /// 完成回合记忆写回策略模式：继承、开启或关闭。
    pub memory_writeback_mode: String,
    /// Final completed-turn memory writeback enablement after all layers resolve.
    /// 所有层级解析后的最终完成回合记忆写回启用状态。
    pub memory_writeback_enabled: bool,
}

/// Stores one conversation's VMM memory policy projection for stable UI lookup.
/// 为稳定 UI 查找保存单个会话的 VMM 记忆策略投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionVmmPolicyByConversation {
    /// Stable conversation identifier owning this VMM policy projection.
    /// 拥有该 VMM 策略投影的稳定会话标识。
    pub conversation_id: String,
    /// VMM policy override and effective state for this conversation.
    /// 该会话的 VMM 策略覆盖与最终状态。
    pub session_vmm_policy: SessionVmmPolicySummary,
}

/// Carries the applied session addon projection for direct RPC responses.
/// 携带直接 RPC 响应使用的已应用会话附属状态投影。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAddonsChangedResponse {
    /// Stable conversation identifier receiving the addon projection.
    /// 接收附属状态投影的稳定会话标识。
    pub conversation_id: String,
    /// Current session goal, or null when no goal is active.
    /// 当前会话目标；没有活动目标时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_goal: Option<SessionGoalSummary>,
    /// Current session todolist, or null when no todolist is visible.
    /// 当前会话待办；没有可见待办时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_todolist: Option<SessionTodolistSummary>,
    /// Current Plan mode state, or null when Plan mode is inactive and no artifact exists.
    /// 当前计划模式状态；计划模式未启用且无产物时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_plan_mode: Option<SessionPlanModeSummary>,
    /// Current session VMM memory policy overrides.
    /// 当前会话 VMM 记忆策略覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_vmm_policy: Option<SessionVmmPolicySummary>,
    /// Current output-style override and the last safely applied snapshot.
    /// 当前输出风格覆盖与最近一次安全应用的快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_output_style: Option<SessionOutputStyleSummary>,
    /// Stable context-compaction window node selected by this session.
    /// 当前会话选择的稳定上下文压缩窗口节点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_context_compaction_window_node_id: Option<String>,
    /// Live physical Provider request counts and token usage for this session.
    /// 当前会话实时物理 Provider 请求计数与 Token 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_provider_usage: Option<SessionProviderUsageSummary>,
    /// Latest workbench revision after applying this addon update.
    /// 应用该附属状态更新后的最新工作台版本。
    pub revision: u64,
}

/// Acknowledges that a submitted composer prompt was accepted by the service.
/// 确认输入区提交的提示词已被服务接收。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitConversationPromptAck {
    /// Stable client-generated identifier that correlates optimistic UI state with this acknowledgement.
    /// 将乐观界面状态与本次确认关联的客户端稳定生成标识。
    pub client_submission_id: String,
    /// Stable conversation identifier that owns the accepted prompt.
    /// 拥有已接收提示词的稳定会话标识。
    pub conversation_id: String,
    /// Visible user-message record id when one was materialized immediately.
    /// 立即物化可见用户消息时对应的记录标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_message_id: Option<String>,
    /// Runtime turn identifier used by the started kernel turn when available.
    /// 已启动内核轮次可用时对应的运行轮次标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Current run phase after accepting the prompt.
    /// 接收提示词后的当前运行阶段。
    pub run_phase: String,
    /// Whether the prompt was queued behind existing work.
    /// 该提示词是否已排队等待既有工作结束。
    pub queued: bool,
    /// Latest accepted workbench revision after the submit-side update.
    /// 提交侧更新后的最新已接收工作台版本。
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies transient reasoning status uses the exact camel-case Web transport contract.
    /// 验证瞬态推理状态使用精确的 Web 驼峰传输契约。
    #[test]
    fn transient_reasoning_status_serializes_for_the_live_store() {
        // envelope mirrors one backend event consumed by the external Web status Store.
        // envelope 镜像一条由 Web 外部状态 Store 消费的后端事件。
        let envelope = WorkbenchDeltaEnvelope {
            generation: 9,
            delta: WorkbenchDelta::TransientReasoningStatusChanged(
                TransientReasoningStatusChangedDelta {
                    conversation_id: String::from("conversation"),
                    status: Some(TransientReasoningStatusSnapshot {
                        kind: TransientReasoningStatusKind::Summary,
                        summaries: vec![TransientReasoningSummaryItemSnapshot {
                            item_id: String::from("summary-0"),
                            text: String::from("正在核对清单"),
                        }],
                        reasoning_tokens: Some(321),
                    }),
                },
            ),
        };

        assert_eq!(
            serde_json::to_value(envelope).expect("transient status should serialize"),
            serde_json::json!({
                "generation": 9,
                "delta": {
                    "kind": "transientReasoningStatusChanged",
                    "payload": {
                        "conversationId": "conversation",
                        "status": {
                            "kind": "summary",
                            "summaries": [{
                                "itemId": "summary-0",
                                "text": "正在核对清单"
                            }],
                            "reasoningTokens": 321
                        }
                    }
                }
            })
        );
    }

    /// Verifies tool request progress uses one closed replace-and-clear Web contract.
    /// 验证工具请求进度使用单个封闭的替换与清除 Web 契约。
    #[test]
    fn tool_request_progress_serializes_for_the_live_store() {
        // envelope mirrors one preview revision consumed only by the leaf-level external Store.
        // envelope 镜像一条仅由叶子级外部 Store 消费的预览修订。
        let envelope = WorkbenchDeltaEnvelope {
            generation: 10,
            delta: WorkbenchDelta::ToolRequestProgressChanged(ToolRequestProgressChangedDelta {
                conversation_id: String::from("conversation"),
                request_stream_id: String::from("request-1"),
                progress: Some(ToolRequestProgressSnapshot {
                    request_stream_id: String::from("request-1"),
                    tool_call_id: String::from("call-1"),
                    tool_name: String::from("terminal_session"),
                    state: ToolRequestProgressState::Receiving,
                    received_bytes: 512,
                    received_characters: 500,
                    request_revision: 3,
                    preview: Some(serde_json::json!({ "kind": "command" })),
                }),
            }),
        };

        assert_eq!(
            serde_json::to_value(envelope).expect("tool request progress should serialize"),
            serde_json::json!({
                "generation": 10,
                "delta": {
                    "kind": "toolRequestProgressChanged",
                    "payload": {
                        "conversationId": "conversation",
                        "requestStreamId": "request-1",
                        "progress": {
                            "requestStreamId": "request-1",
                            "toolCallId": "call-1",
                            "toolName": "terminal_session",
                            "state": "receiving",
                            "receivedBytes": 512,
                            "receivedCharacters": 500,
                            "requestRevision": 3,
                            "preview": { "kind": "command" }
                        }
                    }
                }
            })
        );

        // clear proves the terminal event carries explicit null instead of an omitted ambiguous field.
        // clear 证明终态事件携带显式空值，而不是省略产生歧义的字段。
        let clear = WorkbenchDeltaEnvelope {
            generation: 11,
            delta: WorkbenchDelta::ToolRequestProgressChanged(ToolRequestProgressChangedDelta {
                conversation_id: String::from("conversation"),
                request_stream_id: String::from("request-1"),
                progress: None,
            }),
        };
        assert_eq!(
            serde_json::to_value(clear).expect("tool request clear should serialize")["delta"]["payload"]
                ["progress"],
            serde_json::Value::Null,
        );
    }
}

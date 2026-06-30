use super::*;

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
    /// Display-only fork source marker.
    /// 纯展示用分叉来源标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_source: Option<ConversationForkSourceSummary>,
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
    /// Phase-only update for a running conversation.
    /// 针对运行中会话的纯阶段更新。
    UpdateConversationRunPhase(UpdateConversationRunPhaseDelta),
    /// Pending prompt queue replacement for one conversation.
    /// 单个会话的待发送提示词队列替换增量。
    PendingPromptsChanged(PendingPromptsChangedDelta),
    /// Conversation summary upsert for sidebar and selection state.
    /// 用于侧栏与选择状态的会话摘要写入增量。
    ConversationSummaryChanged(ConversationSummaryChangedDelta),
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
    /// Message-window replacement for one focused conversation surface.
    /// 单个聚焦会话界面的消息窗口替换增量。
    ConversationMessagesChanged(ConversationMessagesChangedDelta),
    /// Message-node patch for one focused conversation surface.
    /// 单个聚焦会话界面的消息节点补丁增量。
    ConversationMessagesPatched(ConversationMessagesPatchedDelta),
    /// Session addon-state replacement for goal, todolist, and Plan mode surfaces.
    /// 面向目标、待办与计划模式界面的会话附属状态替换增量。
    SessionAddonsChanged(SessionAddonsChangedDelta),
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
    /// Conversation id that owns the returned message window.
    /// 拥有返回消息窗口的会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_conversation_id: Option<String>,
    /// Current subagent navigation, or null when no subagent panel is active.
    /// 当前子代理导航；没有活动子代理面板时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_navigation: Option<SubagentNavigationSnapshot>,
    /// Messages for the active surface only.
    /// 仅当前活动界面的消息列表。
    #[serde(default)]
    pub messages: Vec<ConversationMessage>,
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

/// Replaces the visible message window for one conversation.
/// 替换单个会话的可见消息窗口。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessagesChangedDelta {
    /// Stable conversation identifier receiving the message window.
    /// 接收消息窗口的稳定会话标识。
    pub conversation_id: String,
    /// Current visible messages for the conversation.
    /// 当前会话的可见消息列表。
    #[serde(default)]
    pub messages: Vec<ConversationMessage>,
}

/// Patches message nodes for one conversation without replacing the full visible window.
/// 在不替换完整可见窗口的情况下修补单个会话的消息节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessagesPatchedDelta {
    /// Stable conversation identifier receiving the message patch.
    /// 接收消息补丁的稳定会话标识。
    pub conversation_id: String,
    /// Full message nodes to insert or replace by id.
    /// 需要按标识插入或替换的完整消息节点。
    #[serde(default)]
    pub upsert_messages: Vec<ConversationMessage>,
    /// Message ids that should be removed from the visible window.
    /// 需要从可见窗口移除的消息标识。
    #[serde(default)]
    pub remove_message_ids: Vec<String>,
    /// Optional authoritative order for visible nodes in this conversation.
    /// 当前会话可见节点的可选权威顺序。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_order: Option<Vec<String>>,
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
    /// Latest workbench revision after applying this addon update.
    /// 应用该附属状态更新后的最新工作台版本。
    pub revision: u64,
}

/// Acknowledges that a submitted composer prompt was accepted by the service.
/// 确认输入区提交的提示词已被服务接收。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitConversationPromptAck {
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

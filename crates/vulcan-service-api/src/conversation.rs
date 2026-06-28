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

use super::*;

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
    /// Messages waiting for steering or subsequent task execution.
    /// 等待插队投递或后续任务执行的消息。
    #[serde(default)]
    pub pending_messages: Vec<SubagentPendingMessage>,
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
    /// Optional catalog agent identifier for agent-mode child agents.
    /// 智能体模式子代理对应的可选目录智能体标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_id: Option<String>,
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

/// One queued child message; delivery mode distinguishes steering from follow-up work.
/// 一条待处理子代理消息；投递模式区分插队与后续任务。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPendingMessage {
    /// Stable delivery identity.
    /// 稳定投递身份。
    pub delivery_id: String,
    /// Complete message body.
    /// 完整消息正文。
    pub message: String,
    /// True when the next API request should consume this message.
    /// 下一次 API 请求应消费此消息时为 true。
    pub steering: bool,
}

/// Returns a committed pending-prompt edit even when a later queue-panel projection needs refresh.
/// 即使后续队列面板投影需要刷新，也返回已经提交的待发送提示词编辑。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPromptEditOutcome {
    /// Exact prompt now owned by the durable draft.
    /// 当前由持久草稿持有的精确提示词。
    pub entry: PendingPromptSummary,
    /// Controlled post-commit projection warning; absence means every local projection was refreshed.
    /// 受控的提交后投影警告；缺失表示全部本地投影均已刷新。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_warning: Option<String>,
}

/// Describes one slash command visible to composer clients.
/// 描述输入区客户端可见的单个 slash 指令。
///
/// Clients render the description from their language catalog by `id`; the service carries no
/// display text.
/// 客户端按 `id` 从语言目录渲染说明；服务不携带展示文案。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSlashCommandSummary {
    /// Stable command identifier used by routers and catalog lookups.
    /// 路由与目录查找使用的稳定指令标识。
    pub id: String,
    /// Canonical English slash trigger shown to users.
    /// 展示给用户的规范英文 slash 触发词。
    pub command: String,
}

/// Stable `(id, trigger)` pairs in composer display order.
/// 按输入区展示顺序排列的稳定 `(id, 触发词)` 对。
const COMPOSER_SLASH_COMMANDS: &[(&str, &str)] = &[
    ("model", "/model"),
    ("reasoning", "/reasoning"),
    ("compact", "/compact"),
    ("clean-history", "/clean-history"),
    ("plan", "/plan"),
    ("goal", "/goal"),
    ("settings", "/settings"),
    ("style", "/style"),
    ("answer", "/answer"),
    ("knowledge", "/knowledge"),
    ("collapse", "/collapse"),
    ("thinking", "/thinking"),
    ("memory", "/memory"),
    ("skill", "/skill"),
    ("skills", "/skills"),
];

/// Returns every composer slash command in its stable display order.
/// 按稳定展示顺序返回全部输入区 slash 指令。
pub fn composer_slash_command_catalog() -> Vec<ComposerSlashCommandSummary> {
    COMPOSER_SLASH_COMMANDS
        .iter()
        .map(|(id, command)| ComposerSlashCommandSummary {
            id: (*id).to_string(),
            command: (*command).to_string(),
        })
        .collect()
}

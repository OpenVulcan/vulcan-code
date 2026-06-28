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

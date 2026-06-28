use super::*;

/// One stable status value used by the session todolist.
/// 会话 todolist使用的单个稳定状态值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTodolistItemStatus {
    /// The step has not started yet.
    /// 该步骤尚未开始。
    Pending,
    /// The step is currently being worked on.
    /// 该步骤正在进行中。
    InProgress,
    /// The step has been completed.
    /// 该步骤已经完成。
    Completed,
}

/// One todolist step retained for the current session.
/// 为当前会话保留的单个 todolist步骤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistItem {
    /// Human-readable step text supplied by the model.
    /// 模型提供的人类可读步骤文本。
    pub step: String,
    /// Current lifecycle status for the step.
    /// 当前步骤的生命周期状态。
    pub status: SessionTodolistItemStatus,
}

/// Session-scoped todolist persisted outside the visible transcript.
/// 在可见转录之外持久化的会话级 todolist。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistState {
    /// Optional model-authored explanation for the latest todolist update.
    /// 最近一次 todolist 更新的可选模型说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// Ordered todolist items supplied by the model.
    /// 模型提供的有序 todolist 条目。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<SessionTodolistItem>,
    /// Unix-millis text capturing when the todolist was last updated.
    /// 捕获 todolist 最近更新时间的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl SessionTodolistState {
    /// Returns whether the todolist has no user-facing payload.
    /// 返回该 todolist 是否没有任何面向用户的有效载荷。
    ///
    /// # Returns
    /// 返回值
    /// `true` when no explanation, steps, or update time is stored.
    /// 当没有说明、步骤或更新时间时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.explanation.is_none() && self.items.is_empty() && self.updated_at.is_none()
    }
}

/// Session-scoped Plan mode state retained outside the visible transcript.
/// 保留在可见转录之外的会话级计划模式状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanModeState {
    /// Whether the Plan mode steering prompt has been injected for this activation.
    /// 当前计划模式激活期间是否已经注入过计划模式引导提示。
    #[serde(default, skip_serializing_if = "is_false")]
    pub prompt_injected: bool,
    /// Whether the next Build request should receive one Plan-exit notice.
    /// 下一次 Build 请求是否需要接收一次计划模式退出提示。
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending_execution_notice: bool,
    /// Latest tool-produced plan artifact, if one exists.
    /// 最近一次由工具生成的计划产物，如果存在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<SessionPlanArtifactState>,
}

impl SessionPlanModeState {
    /// Returns whether the Plan mode state has no user-facing payload.
    /// 返回计划模式状态是否没有任何面向用户的有效载荷。
    ///
    /// # Returns
    /// 返回值
    /// `true` when no prompt marker, exit notice, or plan artifact is stored.
    /// 未保存提示标记、退出提示或计划产物时返回 `true`。
    pub fn is_empty(&self) -> bool {
        !self.prompt_injected && !self.pending_execution_notice && self.artifact.is_none()
    }
}

/// One Plan mode artifact produced by the model-facing submit_plan tool.
/// 由模型可见 submit_plan 工具生成的单个计划模式产物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanArtifactState {
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

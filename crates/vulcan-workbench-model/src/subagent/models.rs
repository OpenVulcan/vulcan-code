use super::*;

/// Canonical user-facing stop summary reused by subagent cards, panels, and WebUI projections.
/// 供子代理卡片、面板与 WebUI 投影共同复用的规范用户可见停止摘要。
pub const SUBAGENT_TASK_STOPPED_SUMMARY_TEXT: &str = "子代理已按用户请求主动停止，当前没有完整返回结果。请基于现有上下文决定是否重试、改写任务，或直接继续主流程。";

/// Lifecycle states used by one cached subagent task panel.
/// 单个已缓存子代理任务面板使用的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTaskRunState {
    /// The subagent is still running and only a lightweight placeholder panel should be shown.
    /// 子代理仍在运行中，此时只应显示轻量占位面板。
    Running,
    /// The subagent was stopped before natural completion and the panel should preserve the partial transcript plus the stop summary.
    /// 子代理在自然完成前已被停止，此时面板应保留部分转录与停止摘要。
    Stopped,
    /// The subagent finished successfully and its final transcript snapshot can be reviewed.
    /// 子代理已成功完成，此时可以回看其最终转录快照。
    Completed,
    /// The subagent stopped in a failed state and the panel should surface the latest failure detail.
    /// 子代理以失败状态结束，此时面板应展示最近一次失败说明。
    Failed,
}

impl SubagentTaskRunState {
    /// Returns whether the panel state still represents one active running subagent.
    /// 返回当前面板状态是否仍表示一个处于活动运行中的子代理。
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Active subagent panel selection shown in place of the main parent transcript.
/// 显示在主父会话转录位置上的活动子代理面板选择状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedSubagentTaskPanel {
    /// Parent session identifier that owns the subagent task card and badge entry.
    /// 拥有该子代理任务卡片与徽标入口的父会话标识。
    pub parent_session_id: SessionId,
    /// Stable child task identifier whose transcript panel is currently opened.
    /// 当前已打开其转录面板的稳定子任务标识。
    pub task_id: SessionId,
}

/// Cached subagent task panel state reconstructed from runtime delegate events and persisted spawn-agent results.
/// 由运行时委派事件与持久化 spawn_agent 结果重建出的子代理任务面板缓存状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTaskPanelState {
    /// Stable child task identifier used by badges, cards, and panel navigation.
    /// 被徽标、卡片与面板导航共同使用的稳定子任务标识。
    pub task_id: SessionId,
    /// Caller-facing task identifier preserved for parent-agent reasoning and human-visible summaries.
    /// 为父代理推理与人类可见摘要保留的调用方面向任务标识。
    pub public_task_id: String,
    /// Parent session identifier that owns the cached subagent task panel.
    /// 拥有该缓存子代理任务面板的父会话标识。
    pub parent_session_id: SessionId,
    /// Best-effort subagent label shown in badges and panel headers.
    /// 展示在徽标与面板标题中的最佳努力子代理标签。
    pub subagent_label: String,
    /// Optional user-facing task description attached to the task invocation.
    /// 附着在任务调用上的可选面向用户任务描述。
    pub description: Option<String>,
    /// Current lifecycle state of the cached subagent panel.
    /// 当前缓存子代理面板所处的生命周期状态。
    pub run_state: SubagentTaskRunState,
    /// Best-effort final or failure text shown in the child panel summary area.
    /// 展示在子面板摘要区中的最佳努力最终结果或失败文本。
    pub result_text: String,
    /// Optional canonical child conversation snapshot projected into the dedicated subagent panel.
    /// 可选的规范子会话快照，会被投影到独立子代理面板中。
    pub conversation_state: Option<SessionConversationState>,
}

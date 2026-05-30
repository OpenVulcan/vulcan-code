//! Session and subagent state extracted from the main workbench module.
//! 从主工作台模块中抽取的会话与子代理状态。

use super::*;

/// Lifecycle states used by one cached subagent task panel.
/// 单个已缓存子代理任务面板使用的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubagentTaskRunState {
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
    pub(super) fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Active subagent panel selection shown in place of the main parent transcript.
/// 显示在主父会话转录位置上的活动子代理面板选择状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SelectedSubagentTaskPanel {
    /// Parent session identifier that owns the subagent task card and badge entry.
    /// 拥有该子代理任务卡片与徽标入口的父会话标识。
    pub(super) parent_session_id: SessionId,
    /// Stable child task identifier whose transcript panel is currently opened.
    /// 当前已打开其转录面板的稳定子任务标识。
    pub(super) task_id: SessionId,
}

/// Cached subagent task panel state reconstructed from runtime delegate events and persisted task tool results.
/// 由运行时委派事件与持久化 task 工具结果重建出的子代理任务面板缓存状态。
#[derive(Debug, Clone)]
pub(super) struct SubagentTaskPanelState {
    /// Stable child task identifier used by badges, cards, and panel navigation.
    /// 被徽标、卡片与面板导航共同使用的稳定子任务标识。
    pub(super) task_id: SessionId,
    /// Caller-facing task identifier preserved for parent-agent reasoning and human-visible summaries.
    /// 为父代理推理与人类可见摘要保留的调用方面向任务标识。
    pub(super) public_task_id: String,
    /// Parent session identifier that owns the cached subagent task panel.
    /// 拥有该缓存子代理任务面板的父会话标识。
    pub(super) parent_session_id: SessionId,
    /// Best-effort subagent label shown in badges and panel headers.
    /// 展示在徽标与面板标题中的最佳努力子代理标签。
    pub(super) subagent_label: String,
    /// Optional user-facing task description attached to the task invocation.
    /// 附着在任务调用上的可选面向用户任务描述。
    pub(super) description: Option<String>,
    /// Current lifecycle state of the cached subagent panel.
    /// 当前缓存子代理面板所处的生命周期状态。
    pub(super) run_state: SubagentTaskRunState,
    /// Best-effort final or failure text shown in the child panel summary area.
    /// 展示在子面板摘要区中的最佳努力最终结果或失败文本。
    pub(super) result_text: String,
    /// Optional canonical child conversation snapshot projected into the dedicated subagent panel.
    /// 可选的规范子会话快照，会被投影到独立子代理面板中。
    pub(super) conversation_state: Option<SessionConversationState>,
}

/// Active drag auto-scroll snapshot retained while one transcript-like surface keeps selecting beyond its visible viewport.
/// 当某个转录类表面在可见视口之外继续拖拽选择时保留的活动自动滚动快照。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ActiveTranscriptDragAutoScroll {
    /// Session identifier that owns the scrollable surface currently being auto-scrolled.
    /// 当前正在被自动滚动的滚动表面所属会话标识。
    pub(super) session_id: SessionId,
    /// Latest overshoot request reported by the selectable transcript widget.
    /// 可选择转录组件最近一次回传的越界滚动请求。
    pub(super) request: SelectableTranscriptDragAutoScroll,
}

/// Active real-AI run state tracked independently for one session.
/// 为单个会话独立跟踪的活动真实 AI 运行状态。
#[derive(Debug)]
pub(super) struct ActiveSessionRunState {
    /// Monotonic run identifier used to reject stale stream events for this session.
    /// 用于拒绝该会话过期流事件的单调递增运行标识。
    pub(super) run_id: u64,
    /// Abort handle used to stop the session-specific in-flight AI call.
    /// 用于停止该会话专属进行中 AI 调用的中止句柄。
    pub(super) handle: task::Handle,
}

/// Session-scoped precheck progress hints declared by currently active hook handlers.
/// 当前活动 hook 处理器声明的会话级 precheck 进度提示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionPrecheckProgressState {
    /// Ordered labels rendered while request precheck handlers are still running.
    /// precheck 处理器仍在运行时按顺序展示的提示标签。
    labels: Vec<String>,
}

impl SessionPrecheckProgressState {
    /// Builds a precheck progress state from hook-declared labels, dropping empty entries.
    /// 根据 hook 声明的标签构建 precheck 进度状态，并丢弃空白条目。
    pub(super) fn new(labels: impl IntoIterator<Item = String>) -> Option<Self> {
        let labels = labels
            .into_iter()
            .map(|label| label.trim().to_string())
            .filter(|label| !label.is_empty())
            .collect::<Vec<_>>();
        if labels.is_empty() {
            None
        } else {
            Some(Self { labels })
        }
    }

    /// Returns the primary label rendered in compact transcript progress hints.
    /// 返回紧凑转录进度提示中展示的主标签。
    pub(super) fn primary_label(&self) -> &str {
        self.labels
            .first()
            .map(String::as_str)
            .unwrap_or("正在执行前置处理")
    }
}

/// Active prompt-compaction task state retained while one internal summary request is still running.
/// 在单次内部总结请求仍在运行时保留的活动提示词压缩任务状态。
#[derive(Debug)]
pub(super) struct ActiveSessionCompactionState {
    /// Monotonic task identifier used to reject stale compaction completions for this session.
    /// 用于拒绝该会话过期压缩完成回调的单调任务标识。
    pub(super) run_id: u64,
    /// Abort handle used to stop the session-specific in-flight compaction request.
    /// 用于停止该会话专属进行中压缩请求的中止句柄。
    pub(super) handle: task::Handle,
    /// Session phase that should be restored after the compaction task settles normally.
    /// 压缩任务正常收敛后应恢复的会话阶段。
    pub(super) previous_phase: SessionRunPhase,
    /// Original trigger source used to label status updates consistently.
    /// 用于一致标记状态更新的原始触发来源。
    pub(super) trigger: SessionPromptCompactionTrigger,
    /// Visible context-compaction start boundary message that owns the section collapse state.
    /// 拥有分段折叠状态的可见上下文压缩开始边界消息。
    pub(super) status_message_id: Option<String>,
    /// Visible context-compaction body message that receives model-produced summary deltas.
    /// 接收模型生成摘要增量的可见上下文压缩正文消息。
    pub(super) output_message_id: Option<String>,
}

/// Trigger source used to label one prompt-based session compaction run.
/// 用于标记单次提示词式会话压缩运行来源的触发器类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionPromptCompactionTrigger {
    /// User explicitly invoked `/compact` from the composer slash panel.
    /// 用户通过输入框 slash 面板显式调用了 `/compact`。
    ManualSlash,
    /// Context budget policy automatically triggered compaction after a normal run finished.
    /// 上下文预算策略在普通运行结束后自动触发了压缩。
    BudgetAuto,
}

/// Visible lifecycle state rendered inside one context-compaction transcript panel.
/// 渲染在单个上下文压缩转录面板中的可见生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextCompactionStatusPanelState {
    /// Prompt-based compaction completed with a model-produced summary.
    /// 提示词式压缩已通过模型生成摘要完成。
    Completed,
    /// Prompt-based compaction completed by falling back to the host-side legacy summary.
    /// 提示词式压缩已通过回退到宿主侧旧摘要完成。
    CompletedWithFallback,
    /// Prompt-based compaction was stopped before it could rewrite the context anchor.
    /// 提示词式压缩在改写上下文锚点前被停止。
    Stopped,
}

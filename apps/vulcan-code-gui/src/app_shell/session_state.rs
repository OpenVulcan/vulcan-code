//! Session and subagent state extracted from the main workbench module.
//! 从主工作台模块中抽取的会话与子代理状态。

use super::*;

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

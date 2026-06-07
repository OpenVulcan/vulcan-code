use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::SessionNodeId;

/// Stable thread identifier used by the Codex-compatible session contract.
/// Codex 兼容会话契约使用的稳定线程标识符。
pub type CodexThreadId = SessionNodeId;

/// Stable turn identifier used inside one thread.
/// 单个线程内使用的稳定轮次标识符。
pub type CodexTurnId = String;

/// Event visibility used to separate model/runtime facts from user-facing display.
/// 用于区分模型/运行时事实与面向用户展示内容的事件可见性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexEventVisibility {
    /// Visible transcript content shown in normal conversation history.
    /// 展示在普通会话历史中的可见转录内容。
    Visible,
    /// Hidden runtime content available to reducers but not rendered as normal text.
    /// 可供 reducer 使用但不作为普通文本渲染的隐藏运行时内容。
    Hidden,
    /// Diagnostic content used for tracing or debugging only.
    /// 仅用于追踪或调试的诊断内容。
    Diagnostic,
}

/// Logical scope of one Codex-compatible event.
/// 单条 Codex 兼容事件的逻辑范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexEventScope {
    /// Event belongs to the whole thread.
    /// 事件属于整个线程。
    Thread,
    /// Event belongs to one model turn.
    /// 事件属于一次模型轮次。
    Turn,
    /// Event belongs to a tool or item lifecycle.
    /// 事件属于工具或条目生命周期。
    Item,
    /// Event belongs to a child agent or delegated task.
    /// 事件属于子代理或委派任务。
    Agent,
}

/// User-facing role for visible display projection.
/// 可见展示投影使用的面向用户角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexVisibleRole {
    /// User-authored input.
    /// 用户输入。
    User,
    /// Assistant-authored final or incremental visible output.
    /// 助手输出的最终或增量可见内容。
    Assistant,
    /// Tool lifecycle entry visible in the transcript.
    /// 转录中可见的工具生命周期条目。
    Tool,
    /// System notice visible to the user.
    /// 面向用户可见的系统提示。
    Notice,
}

/// Codex-compatible event kind preserved across storage, runtime, and display reducers.
/// 在存储、运行时和展示 reducer 之间保持一致的 Codex 兼容事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionEventKind {
    /// Thread was created or resumed.
    /// 线程已创建或恢复。
    ThreadStarted,
    /// Turn was accepted by the runtime.
    /// 轮次已被运行时接受。
    TurnStarted,
    /// User input item was appended.
    /// 用户输入条目已追加。
    UserInput,
    /// Runtime item started.
    /// 运行时条目已开始。
    ItemStarted,
    /// Runtime item completed.
    /// 运行时条目已完成。
    ItemCompleted,
    /// Raw provider item completed.
    /// 原始供应商条目已完成。
    RawItemCompleted,
    /// Turn completed with a terminal status.
    /// 轮次以终态完成。
    TurnCompleted,
    /// Child agent was spawned.
    /// 子代理已创建。
    AgentStarted,
    /// Child agent produced a message or result.
    /// 子代理产出消息或结果。
    AgentMessage,
    /// Child agent completed.
    /// 子代理已完成。
    AgentCompleted,
    /// Context compaction marker was recorded.
    /// 上下文压缩标记已记录。
    Compaction,
    /// Thread state changed without adding visible transcript content.
    /// 线程状态变化但未添加可见转录内容。
    StateChanged,
}

/// Append-only Codex-compatible event row.
/// 追加式 Codex 兼容事件行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexSessionEvent {
    /// Stable event identifier.
    /// 稳定事件标识符。
    pub event_id: String,
    /// Owning thread identifier.
    /// 所属线程标识符。
    pub thread_id: CodexThreadId,
    /// Optional turn identifier.
    /// 可选轮次标识符。
    pub turn_id: Option<CodexTurnId>,
    /// Optional item identifier.
    /// 可选条目标识符。
    pub item_id: Option<String>,
    /// Monotonic event sequence in the owning thread.
    /// 所属线程内的单调事件序号。
    pub sequence: u64,
    /// Event kind.
    /// 事件类型。
    pub kind: CodexSessionEventKind,
    /// Event logical scope.
    /// 事件逻辑范围。
    pub scope: CodexEventScope,
    /// Event visibility.
    /// 事件可见性。
    pub visibility: CodexEventVisibility,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Structured payload following the matching Codex event schema.
    /// 遵循对应 Codex 事件 schema 的结构化载荷。
    pub payload: Value,
}

impl CodexSessionEvent {
    /// Creates one append-only Codex-compatible event row.
    /// 创建一条追加式 Codex 兼容事件行。
    pub fn new(
        event_id: impl Into<String>,
        thread_id: CodexThreadId,
        sequence: u64,
        kind: CodexSessionEventKind,
        scope: CodexEventScope,
        visibility: CodexEventVisibility,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            thread_id,
            turn_id: None,
            item_id: None,
            sequence,
            kind,
            scope,
            visibility,
            timestamp_ms,
            payload: Value::Null,
        }
    }

    /// Attaches a turn identifier to this event.
    /// 为该事件附加轮次标识符。
    pub fn with_turn_id(mut self, turn_id: impl Into<CodexTurnId>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }

    /// Attaches an item identifier to this event.
    /// 为该事件附加条目标识符。
    pub fn with_item_id(mut self, item_id: impl Into<String>) -> Self {
        self.item_id = Some(item_id.into());
        self
    }

    /// Attaches a structured payload to this event.
    /// 为该事件附加结构化载荷。
    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }
}

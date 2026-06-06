use crate::model::SessionNodeId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared event kind for main sessions and task sessions.
/// 主会话与子代理会话共享的事件类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    /// User prompt event.
    /// 用户提示词事件。
    UserPrompt,
    /// Assistant reasoning or thinking event.
    /// 助手推理或思考事件。
    AssistantThinking,
    /// Tool call event.
    /// 工具调用事件。
    ToolCall,
    /// Tool result event.
    /// 工具结果事件。
    ToolResult,
    /// Assistant output event.
    /// 助手输出事件。
    AssistantOutput,
    /// Turn statistics event.
    /// 轮次统计事件。
    TurnStats,
    /// Context compaction marker event.
    /// 上下文压缩标记事件。
    CompactionMarker,
    /// Task invocation index event.
    /// 子代理调用索引事件。
    TaskInvocation,
    /// Task feedback event.
    /// 子代理反馈事件。
    TaskFeedback,
    /// Runtime status marker event.
    /// 运行时状态标记事件。
    RuntimeMarker,
}

/// Append-only event record stored by a session node.
/// 会话节点保存的追加式事件记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEventRecord {
    /// Stable event identifier.
    /// 稳定事件标识符。
    pub event_id: String,
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Monotonic sequence inside the owning node.
    /// 所属节点内的单调递增序号。
    pub sequence: u64,
    /// Event kind.
    /// 事件类型。
    pub kind: SessionEventKind,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Event payload owned by the specific session implementation.
    /// 由具体会话实现拥有的事件载荷。
    pub payload: Value,
}

impl SessionEventRecord {
    /// Builds one shared event record.
    /// 构建一条共享事件记录。
    pub fn new(
        event_id: impl Into<String>,
        node_id: SessionNodeId,
        sequence: u64,
        kind: SessionEventKind,
        timestamp_ms: u64,
        payload: Value,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            node_id,
            sequence,
            kind,
            timestamp_ms,
            payload,
        }
    }
}

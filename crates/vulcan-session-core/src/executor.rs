use crate::config::{SessionEffectiveConfig, SessionRequestedConfig};
use crate::event::SessionEventRecord;
use crate::model::SessionNodeId;
use crate::policy::{ExecutionRuntimePolicy, ExecutionScope};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt;
use std::pin::Pin;

/// Request passed from a session router to the runtime executor.
/// 从会话路由传递给运行时执行器的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionExecutionRequest {
    /// Execution scope.
    /// 执行范围。
    pub scope: ExecutionScope,
    /// Target session node identifier.
    /// 目标会话节点标识符。
    pub node_id: SessionNodeId,
    /// User prompt or follow-up prompt.
    /// 用户提示词或后续提示词。
    pub prompt: String,
    /// Requested configuration before resolution.
    /// 解析前的请求配置。
    pub requested_config: SessionRequestedConfig,
    /// Effective configuration used by execution.
    /// 执行使用的有效配置。
    pub effective_config: SessionEffectiveConfig,
    /// Runtime feature policy.
    /// 运行时能力策略。
    pub runtime_policy: ExecutionRuntimePolicy,
    /// Caller-owned execution metadata.
    /// 调用方拥有的执行元数据。
    pub metadata: Value,
}

/// Streaming execution event kind.
/// 流式执行事件类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionEventKind {
    /// Execution started.
    /// 执行已开始。
    Started,
    /// Incremental event was produced.
    /// 已产生增量事件。
    Delta,
    /// Execution completed.
    /// 执行已完成。
    Completed,
    /// Execution failed.
    /// 执行已失败。
    Failed,
    /// Execution was stopped.
    /// 执行已停止。
    Stopped,
}

/// Streaming execution event emitted by an executor.
/// 执行器发出的流式执行事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionExecutionEvent {
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Execution event kind.
    /// 执行事件类型。
    pub kind: SessionExecutionEventKind,
    /// Optional session event projection.
    /// 可选的会话事件投影。
    pub event: Option<SessionEventRecord>,
    /// Caller-owned event payload.
    /// 调用方拥有的事件载荷。
    pub payload: Value,
}

/// Terminal execution status.
/// 终止执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionStatus {
    /// Execution completed successfully.
    /// 执行成功完成。
    Completed,
    /// Execution failed.
    /// 执行失败。
    Failed,
    /// Execution was stopped by user or runtime.
    /// 执行被用户或运行时停止。
    Stopped,
}

/// Terminal execution outcome.
/// 终止执行结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionExecutionOutcome {
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Terminal status.
    /// 终止状态。
    pub status: SessionExecutionStatus,
    /// Final assistant text or summary.
    /// 最终助手文本或摘要。
    pub result_text: Option<String>,
    /// Number of provider turns executed.
    /// 已执行的供应商轮次数量。
    pub provider_turns_executed: usize,
    /// Number of tool calls processed.
    /// 已处理的工具调用数量。
    pub tool_calls_processed: usize,
    /// Error message when status is failed.
    /// 状态为失败时的错误信息。
    pub error: Option<String>,
}

/// Error returned by a session node executor.
/// 会话节点执行器返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionExecutionError {
    /// Human-readable error message.
    /// 面向人的错误信息。
    pub message: String,
}

impl SessionExecutionError {
    /// Creates an executor error from a message.
    /// 使用消息创建执行器错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionExecutionError {
    /// Formats the error message.
    /// 格式化错误信息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SessionExecutionError {}

/// Boxed stream of execution events emitted by the runtime adapter.
/// 运行时适配器发出的盒装执行事件流。
pub type SessionExecutionStream =
    Pin<Box<dyn Stream<Item = Result<SessionExecutionEvent, SessionExecutionError>> + Send>>;

/// Runtime execution adapter shared by main and task session routers.
/// 主会话与子代理会话路由共享的运行时执行适配器。
pub trait SessionNodeExecutor: Send + Sync {
    /// Executes one prepared session request.
    /// 执行一个已准备好的会话请求。
    fn execute(
        &self,
        request: SessionExecutionRequest,
    ) -> Result<SessionExecutionOutcome, SessionExecutionError>;

    /// Streams one prepared session request.
    /// 流式执行一个已准备好的会话请求。
    fn stream(
        &self,
        request: SessionExecutionRequest,
    ) -> Result<SessionExecutionStream, SessionExecutionError>;
}

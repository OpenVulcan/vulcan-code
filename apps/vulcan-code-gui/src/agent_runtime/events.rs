#![allow(dead_code)]

//! Runtime event definitions used by future agent execution surfaces.
//! 供后续代理执行界面使用的运行时事件定义。

/// High-level runtime event kinds emitted by the compose foundation.
/// compose 基础架构发出的高层运行时事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEventKind {
    /// Token-level model streaming delta.
    /// 模型流式输出的 Token 增量事件。
    TokenDelta,
    /// Native or MCP tool invocation started.
    /// 原生工具或 MCP 工具调用开始事件。
    ToolStarted,
    /// Native or MCP tool invocation finished.
    /// 原生工具或 MCP 工具调用结束事件。
    ToolFinished,
    /// Delegate or sub-agent invocation started.
    /// 委派代理或子代理调用开始事件。
    DelegateStarted,
    /// Delegate or sub-agent invocation finished.
    /// 委派代理或子代理调用结束事件。
    DelegateFinished,
    /// Execution pipeline failed.
    /// 执行链路失败事件。
    Failed,
}

/// Structured runtime event record reserved for future telemetry surfaces.
/// 为后续遥测界面预留的结构化运行时事件记录。
#[derive(Debug, Clone)]
pub struct RuntimeEvent {
    /// Event kind.
    /// 事件类型。
    pub kind: RuntimeEventKind,
    /// Short event headline.
    /// 事件简短标题。
    pub title: String,
    /// Additional event detail text.
    /// 事件补充说明文本。
    pub detail: String,
}

impl RuntimeEvent {
    /// Creates a new runtime event from strongly typed parts.
    /// 使用强类型片段创建新的运行时事件。
    pub fn new(
        kind: RuntimeEventKind,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            detail: detail.into(),
        }
    }
}

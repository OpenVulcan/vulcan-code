//! Runtime event definitions used by future agent execution surfaces.
//! 供后续代理执行界面使用的运行时事件定义。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionRunReport};
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use serde_json::Value;
use vulcan_context::RuntimeProviderLoweringRepairEvent;
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

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
    #[allow(dead_code)]
    DelegateStarted,
    /// Delegate or sub-agent invocation finished.
    /// 委派代理或子代理调用结束事件。
    #[allow(dead_code)]
    DelegateFinished,
    /// Execution pipeline failed.
    /// 执行链路失败事件。
    Failed,
    /// Provider execution is waiting before an automatic retry.
    /// Provider 执行正在等待一次自动重试。
    Retrying,
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
    /// Optional structured metadata reserved for transcript projection and host-side UI enrichments.
    /// 为转录投影与宿主侧界面增强保留的可选结构化元数据。
    pub metadata: Option<Value>,
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
            metadata: None,
        }
    }

    /// Attaches optional structured metadata without changing the visible runtime headline or detail.
    /// 在不改变可见运行时标题与正文的前提下附着可选结构化元数据。
    pub fn with_metadata(mut self, metadata: Option<Value>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Incremental execution events streamed from the real provider runtime into the GUI workbench.
/// 从真实 Provider 运行时流式发送到图形工作台的增量执行事件。
#[derive(Debug, Clone)]
pub enum ExecutionStreamEvent {
    /// Execution preparation completed and validated.
    /// 执行准备已完成且通过校验。
    Prepared(ExecutionPreparation),
    /// Provider-neutral request snapshot has been built before transport-specific serialization.
    /// Provider 中立请求快照已在传输层专用序列化前构建完成。
    RequestReady {
        /// Session identifier owning the provider request.
        /// 拥有该 Provider 请求的会话标识。
        session_id: SessionId,
    },
    /// Provider request snapshot checkpoint emitted for runtime audit persistence.
    /// 为运行时审计持久化发出的 Provider 请求快照检查点。
    ProviderRequestCheckpoint {
        /// Session identifier owning the provider request checkpoint.
        /// 拥有该 Provider 请求检查点的会话标识。
        session_id: SessionId,
        /// Provider-neutral request snapshot captured before wire lowering.
        /// 线协议转换前捕获的 Provider 中立请求快照。
        snapshot: CoreProviderRequestSnapshot,
    },
    /// Provider-specific wire request payload has been built immediately before the HTTP/API call.
    /// Provider 专用线缆请求载荷已在 HTTP/API 调用前构建完成。
    WireRequestReady {
        /// Session identifier owning the provider wire request.
        /// 拥有该 Provider 线缆请求的会话标识。
        session_id: SessionId,
    },
    /// One real provider request snapshot has just been sent and the GUI should show a lightweight waiting hint until visible output arrives.
    /// 一次真实 Provider 请求快照刚刚发出，图形界面应显示轻量等待提示，直到出现可见输出为止。
    RequestStarted {
        /// Session identifier owning the provider request.
        /// 拥有该 Provider 请求的会话标识。
        session_id: SessionId,
    },
    /// Visible assistant thinking delta that should stay outside future context.
    /// 应保持在未来上下文之外的可见思考增量。
    ThinkingDelta {
        /// Session identifier receiving the thinking delta.
        /// 接收该思考增量的会话标识。
        session_id: SessionId,
        /// Incremental thinking text emitted by the provider.
        /// Provider 发出的增量思考文本。
        delta: String,
    },
    /// Visible assistant thinking phase completed and should dock back into the main transcript immediately.
    /// 可见助手思考阶段已完成，并应立即收回主转录正文中。
    ThinkingCompleted {
        /// Session identifier whose thinking phase completed.
        /// 思考阶段已经完成的会话标识。
        session_id: SessionId,
    },
    /// Visible assistant final-output delta that should stay inside future context.
    /// 应保留在未来上下文之内的可见正式输出增量。
    OutputDelta {
        /// Session identifier receiving the output delta.
        /// 接收该输出增量的会话标识。
        session_id: SessionId,
        /// Incremental assistant output text emitted by the provider.
        /// Provider 发出的助手正式输出增量文本。
        delta: String,
    },
    /// Runtime event emitted while the run is still in progress.
    /// 在运行仍进行中时发出的运行时事件。
    RuntimeEvent {
        /// Session identifier owning the runtime event.
        /// 拥有该运行时事件的会话标识。
        session_id: SessionId,
        /// Structured runtime event payload to project into host UI.
        /// 需要投影到宿主界面的结构化运行时事件载荷。
        event: RuntimeEvent,
    },
    /// Provider lowering repair audit event emitted by a protocol adapter.
    /// 协议适配器发出的 Provider 转换修复审计事件。
    ProviderLoweringRepair {
        /// Session identifier owning the lowering repair event.
        /// 拥有该转换修复事件的会话标识。
        session_id: SessionId,
        /// Structured lowering repair payload to persist into runtime audit.
        /// 需要持久化到运行时审计中的结构化转换修复载荷。
        event: RuntimeProviderLoweringRepairEvent,
    },
    /// Safe canonical context checkpoint emitted after one provider/tool cycle becomes replayable.
    /// 在一次 Provider/工具循环已经可回放后发出的安全规范上下文检查点。
    ConversationCheckpoint {
        /// Session identifier owning the checkpoint.
        /// 拥有该检查点的会话标识。
        session_id: SessionId,
        /// Canonical conversation state that can be used by the next request.
        /// 可供下一次请求使用的规范会话状态。
        conversation_state: SessionConversationState,
        /// Latest successful provider usage captured at this checkpoint.
        /// 该检查点捕获到的最近一次成功 Provider 用量。
        last_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
        /// Number of provider requests observed by this run when the checkpoint was emitted.
        /// 发出该检查点时本轮已观测到的 Provider 请求次数。
        provider_request_count: usize,
    },
    /// Terminal execution report emitted exactly once at the end of the run.
    /// 在运行结束时恰好发出一次的最终执行报告。
    Finished(ExecutionRunReport),
}

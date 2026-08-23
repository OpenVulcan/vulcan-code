//! Shared provider-stream channel types reused by the executor, transport adapters, and protocol bridge helpers.
//! 供执行器、传输适配器与协议桥接辅助逻辑复用的共享 Provider 流式通道类型。

use crate::agent_runtime::events::{
    ProviderRequestDurabilityCompletion, RuntimeEvent, TransientReasoningStatus,
};
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use crate::conversation_protocol::SessionConversationState;
use futures_util::Stream;
use serde_json::Value;
use std::collections::BTreeMap;
use std::pin::Pin;
use vulcan_ai_contract::{AiExecutionError, AiExecutionRequest, AiTransportKind};
use vulcan_context::RuntimeProviderLoweringRepairEvent;
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Lightweight provider-stream delta emitted while one real run is still in progress.
/// 单次真实运行仍在进行中时发出的轻量 Provider 流式增量。
// Synthetic runtime tests still inject the full neutral delta contract, while the public adapter
// routes text through the Core sink and emits only host-side fragments in production.
// 合成运行时测试仍会注入完整中立增量契约；公共适配器在生产环境中通过 Core sink 路由文本，并仅发出宿主侧碎片。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ProviderStreamDelta {
    /// Structured request snapshot captured right before the next provider request is dispatched.
    /// 在下一次 Provider 请求真正发出前捕获的结构化请求快照。
    RequestSnapshot(CoreProviderRequestSnapshot),
    /// Structured request snapshot whose durable start record must be acknowledged before dispatch.
    /// 结构化请求快照；分派前必须确认其开始记录已可靠持久化。
    RequestPrepared {
        /// Exact Provider-neutral request snapshot.
        /// 精确 Provider 中立请求快照。
        snapshot: CoreProviderRequestSnapshot,
        /// Exact provider-neutral request that will be dispatched after the durability barrier.
        /// 可靠持久化屏障后将被分派的精确 Provider 中立请求。
        exact_request: AiExecutionRequest,
        /// Exact controller configuration frozen for subsequent tool-result turns.
        /// 为后续工具结果回合冻结的精确控制器配置。
        controller_config: vulcan_core::types::ControllerConfig,
        /// Exact canonical Core snapshot used to build this request.
        /// 用于构建当前请求的精确规范 Core 快照。
        controller_snapshot: vulcan_core::types::ConversationSnapshot,
        /// Context-budget decision made from this exact request before dispatch.
        /// 根据当前精确请求在分派前作出的上下文预算决策。
        context_budget: Option<crate::context_budget::ContextBudgetSnapshot>,
        /// Host completion released after the session state write succeeds.
        /// 会话状态写入成功后由宿主释放的完成句柄。
        completion: ProviderRequestDurabilityCompletion,
    },
    /// Confirms the host durably retained Started and released the pre-dispatch gate.
    /// 确认宿主已经可靠保留 Started，并释放分派前闸门。
    RequestStartDurabilityConfirmed,
    /// Final provider-specific HTTP request JSON captured immediately before network dispatch.
    /// 在网络发送前捕获到的最终 Provider 专用 HTTP 请求 JSON。
    WireRequest(Value),
    /// Exact upstream transport facts observed by the physical dispatcher.
    /// 真实分派器观察到的精确上游传输事实。
    TransportObserved {
        /// Exact selected transport.
        /// 精确选中的传输。
        transport: AiTransportKind,
        /// Host timestamp immediately before dispatch.
        /// 分派前的宿主时间戳。
        dispatched_at_ms: i64,
        /// Host timestamp when HTTP headers became available.
        /// HTTP 响应头可用时的宿主时间戳。
        headers_at_ms: Option<i64>,
        /// Exact HTTP status when applicable.
        /// 适用时的精确 HTTP 状态码。
        http_status: Option<u16>,
        /// Allowlisted upstream request identifier.
        /// 允许列表内的上游请求标识。
        provider_request_id: Option<String>,
        /// Fixed-allowlist rate-limit response headers.
        /// 固定允许列表内的限流响应头。
        rate_limit_headers: BTreeMap<String, String>,
        /// Host completion released after the dispatched state is durably persisted.
        /// dispatched 状态可靠持久化后由宿主释放的完成句柄。
        completion: ProviderRequestDurabilityCompletion,
    },
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
    /// Replace-in-place reasoning status that must never become transcript content.
    /// 必须绝不成为转录正文的原地替换式推理状态。
    TransientReasoningStatusChanged(Option<TransientReasoningStatus>),
    /// Assistant final-output delta returned by the provider stream.
    /// Provider 流返回的助手正式输出增量。
    Output(String),
    /// Normalized model-side tool request fragment emitted before host execution starts.
    /// 宿主执行开始前发出的规范化模型侧工具请求碎片。
    ToolRequestFragment(ToolRequestFragment),
    /// Runtime event emitted from one streamed provider-side tool interaction.
    /// 来自某次 Provider 侧流式工具交互的运行时事件。
    RuntimeEvent(RuntimeEvent),
    /// Latest accumulated normalized usage for the active physical Provider request.
    /// 当前活动真实 Provider 请求的最新累计规范化用量。
    UsageUpdated(CoreProviderUsageSnapshot),
    /// Successful physical request boundary that must be durable before the controller continues.
    /// 成功物理请求边界；控制器继续前必须可靠持久化。
    RequestSucceeded {
        /// Final normalized Provider usage when supplied.
        /// Provider 提供时的最终规范化用量。
        usage: Option<CoreProviderUsageSnapshot>,
        /// Host completion released after terminal session-state persistence.
        /// 终态会话状态持久化后由宿主释放的完成句柄。
        completion: ProviderRequestDurabilityCompletion,
    },
    /// Terminal failure metadata for the currently active physical Provider request.
    /// 当前活动真实 Provider 请求的终态失败元数据。
    RequestFailed {
        /// Whether the transport will start another attempt.
        /// 传输层是否会开始另一次尝试。
        will_retry: bool,
        /// One-based retry number, or zero when no retry follows.
        /// 从一开始的重试序号；不继续重试时为零。
        retry_number: u32,
        /// Effective finite retry ceiling, or zero when no retry follows.
        /// 有效有限重试上限；不继续重试时为零。
        max_retries: u32,
        /// Retry delay in milliseconds, or zero when no retry follows.
        /// 重试延迟毫秒数；不继续重试时为零。
        delay_ms: u64,
        /// Stable redacted failure category.
        /// 稳定脱敏失败类别。
        error_category: String,
        /// Structured provider failure facts retained without parsing display text.
        /// 无需解析展示文本即可保留的结构化供应商失败事实。
        error: Option<Box<AiExecutionError>>,
    },
    /// Failed physical request boundary that must be durable before retry or teardown.
    /// 失败物理请求边界；重试或清理前必须可靠持久化。
    RequestFailedDurable {
        /// Whether the transport will start another attempt.
        /// 传输层是否会开始另一次尝试。
        will_retry: bool,
        /// One-based retry number, or zero when no retry follows.
        /// 从一开始的重试序号；不继续重试时为零。
        retry_number: u32,
        /// Effective finite retry ceiling, or zero when no retry follows.
        /// 有效有限重试上限；不继续重试时为零。
        max_retries: u32,
        /// Retry delay in milliseconds, or zero when no retry follows.
        /// 重试延迟毫秒数；不继续重试时为零。
        delay_ms: u64,
        /// Stable redacted failure category.
        /// 稳定脱敏失败类别。
        error_category: String,
        /// Structured provider failure facts retained without parsing display text.
        /// 无需解析展示文本即可保留的结构化供应商失败事实。
        error: Option<Box<AiExecutionError>>,
        /// Host completion released after terminal session-state persistence.
        /// 终态会话状态持久化后由宿主释放的完成句柄。
        completion: ProviderRequestDurabilityCompletion,
    },
    /// Provider lowering repair, downgrade, or rejection emitted for runtime audit.
    /// 为运行时审计发出的 Provider 转换修复、降级或拒绝事件。
    LoweringRepair(RuntimeProviderLoweringRepairEvent),
    /// Final canonical conversation state plus the latest successful provider usage from the current run.
    /// 当前运行结束后的最终规范会话状态，以及本次运行中最近一次成功 Provider 用量。
    FinalConversationState(SessionConversationState, Option<CoreProviderUsageSnapshot>),
    /// Requests host-owned context compaction before the next provider turn can continue.
    /// 请求宿主拥有的上下文压缩，以便继续下一次 Provider 回合。
    ContextCompactionRequired {
        /// Replay-safe canonical conversation state at the tool boundary.
        /// 工具边界处可安全回放的规范会话状态。
        conversation_state: SessionConversationState,
        /// Verified context-budget diagnostic produced before dispatch.
        /// 分派前生成的已验证上下文预算诊断。
        reason: String,
    },
    /// Requests a host-owned restart so a pending exact route governs the next Provider request.
    /// 请求宿主重启当前循环，使待生效精确路由管辖下一次 Provider 请求。
    ProviderRouteSwitchRequired {
        /// Replay-safe canonical conversation state at the completed tool boundary.
        /// 已完成工具边界处可安全回放的规范会话状态。
        conversation_state: SessionConversationState,
        /// Internal diagnostic describing why the frozen transport must be replaced.
        /// 描述为何必须替换冻结传输的内部诊断。
        reason: String,
    },
}

/// Boxed provider stream used by the runtime to forward thinking and output deltas incrementally.
/// 供运行时逐步转发思考与输出增量的装箱 Provider 流类型。
pub(crate) type ProviderDeltaStream =
    Pin<Box<dyn Stream<Item = Result<ProviderStreamDelta, String>> + Send>>;

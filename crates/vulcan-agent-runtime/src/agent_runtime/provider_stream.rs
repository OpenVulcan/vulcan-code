//! Shared provider-stream channel types reused by the executor, transport adapters, and protocol bridge helpers.
//! 供执行器、传输适配器与协议桥接辅助逻辑复用的共享 Provider 流式通道类型。

use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use crate::conversation_protocol::SessionConversationState;
use futures_util::Stream;
use serde_json::Value;
use std::collections::BTreeMap;
use std::pin::Pin;
use vulcan_ai_contract::{AiExecutionError, AiTransportKind};
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
    },
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
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
}

/// Boxed provider stream used by the runtime to forward thinking and output deltas incrementally.
/// 供运行时逐步转发思考与输出增量的装箱 Provider 流类型。
pub(crate) type ProviderDeltaStream =
    Pin<Box<dyn Stream<Item = Result<ProviderStreamDelta, String>> + Send>>;

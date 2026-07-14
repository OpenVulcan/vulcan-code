//! Shared provider-stream channel types reused by the executor, transport adapters, and protocol bridge helpers.
//! 供执行器、传输适配器与协议桥接辅助逻辑复用的共享 Provider 流式通道类型。

use crate::agent_runtime::events::RuntimeEvent;
use crate::conversation_protocol::SessionConversationState;
use futures_util::Stream;
use serde_json::Value;
use std::pin::Pin;
use vulcan_context::RuntimeProviderLoweringRepairEvent;
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Lightweight provider-stream delta emitted while one real run is still in progress.
/// 单次真实运行仍在进行中时发出的轻量 Provider 流式增量。
pub(crate) enum ProviderStreamDelta {
    /// Structured request snapshot captured right before the next provider request is dispatched.
    /// 在下一次 Provider 请求真正发出前捕获的结构化请求快照。
    RequestSnapshot(CoreProviderRequestSnapshot),
    /// Final provider-specific HTTP request JSON captured immediately before network dispatch.
    /// 在网络发送前捕获到的最终 Provider 专用 HTTP 请求 JSON。
    WireRequest(Value),
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
    /// Assistant final-output delta returned by the provider stream.
    /// Provider 流返回的助手正式输出增量。
    Output(String),
    /// Runtime event emitted from one streamed provider-side tool interaction.
    /// 来自某次 Provider 侧流式工具交互的运行时事件。
    RuntimeEvent(RuntimeEvent),
    /// Terminal failure metadata for the currently active physical Provider request.
    /// 当前活动真实 Provider 请求的终态失败元数据。
    RequestFailed {
        /// Whether the transport will start another attempt.
        /// 传输层是否会开始另一次尝试。
        will_retry: bool,
        /// One-based retry number, or zero when no retry follows.
        /// 从一开始的重试序号；不继续重试时为零。
        retry_number: u32,
        /// Configured retry ceiling.
        /// 配置的重试上限。
        max_retries: u32,
        /// Retry delay in milliseconds, or zero when no retry follows.
        /// 重试延迟毫秒数；不继续重试时为零。
        delay_ms: u64,
        /// Stable redacted failure category.
        /// 稳定脱敏失败类别。
        error_category: String,
    },
    /// Provider lowering repair, downgrade, or rejection emitted for runtime audit.
    /// 为运行时审计发出的 Provider 转换修复、降级或拒绝事件。
    LoweringRepair(RuntimeProviderLoweringRepairEvent),
    /// Final canonical conversation state plus the latest successful provider usage from the current run.
    /// 当前运行结束后的最终规范会话状态，以及本次运行中最近一次成功 Provider 用量。
    FinalConversationState(SessionConversationState, Option<CoreProviderUsageSnapshot>),
}

/// Boxed provider stream used by the runtime to forward thinking and output deltas incrementally.
/// 供运行时逐步转发思考与输出增量的装箱 Provider 流类型。
pub(crate) type ProviderDeltaStream =
    Pin<Box<dyn Stream<Item = Result<ProviderStreamDelta, String>> + Send>>;

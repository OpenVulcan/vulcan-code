//! Shared provider-stream channel types reused by the executor, transport adapters, and protocol bridge helpers.
//! 供执行器、传输适配器与协议桥接辅助逻辑复用的共享 Provider 流式通道类型。

use crate::agent_runtime::events::RuntimeEvent;
use crate::conversation_protocol::SessionConversationState;
use futures_util::Stream;
use std::pin::Pin;
use vulcan_core::types::ProviderRequestSnapshot as CoreProviderRequestSnapshot;

/// Lightweight provider-stream delta emitted while one real run is still in progress.
/// 单次真实运行仍在进行中时发出的轻量 Provider 流式增量。
pub(crate) enum ProviderStreamDelta {
    /// Structured request snapshot captured right before the next provider request is dispatched.
    /// 在下一次 Provider 请求真正发出前捕获的结构化请求快照。
    RequestSnapshot(CoreProviderRequestSnapshot),
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
    /// Assistant final-output delta returned by the provider stream.
    /// Provider 流返回的助手正式输出增量。
    Output(String),
    /// Runtime event emitted from one streamed provider-side tool interaction.
    /// 来自某次 Provider 侧流式工具交互的运行时事件。
    RuntimeEvent(RuntimeEvent),
    /// Final canonical conversation state captured after one host-owned controller run finishes.
    /// 在一次宿主自有控制器运行结束后捕获的最终规范会话状态。
    FinalConversationState(SessionConversationState),
}

/// Boxed provider stream used by the runtime to forward thinking and output deltas incrementally.
/// 供运行时逐步转发思考与输出增量的装箱 Provider 流类型。
pub(crate) type ProviderDeltaStream =
    Pin<Box<dyn Stream<Item = Result<ProviderStreamDelta, String>> + Send>>;

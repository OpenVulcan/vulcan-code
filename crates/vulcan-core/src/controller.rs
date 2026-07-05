//! Core controller state machine owned by `vulcan-core`.
//! `vulcan-core` 自有的控制器状态机。

use crate::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use crate::tool::{VulcanToolExecutor, VulcanToolPolicy};
use crate::types::{
    ControllerConfig, ControllerRunResult, ConversationAttachment, ConversationMessage,
    ConversationPayload, ConversationRecord, ConversationSnapshot, CoreError, CoreEvent,
    CoreEventKind, ProviderTurnDelta, ProviderTurnResponse, ProviderUsageSnapshot,
    ToolInvocationRequest, ToolInvocationResult, TranscriptRecordKind, TranscriptRole,
};
use futures_util::future::join_all;
use serde_json::{Map as JsonMap, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// Describes the current coarse-grained execution phase of the controller.
/// 描述控制器当前的粗粒度执行阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerPhase {
    /// No execution is running at the moment.
    /// 当前没有执行在运行。
    Idle,
    /// The controller is preparing the next provider request.
    /// 控制器正在准备下一次 Provider 请求。
    PreparingRequest,
    /// The controller has sent a request and is waiting for the response.
    /// 控制器已发出请求并正在等待响应。
    WaitingForProvider,
    /// The controller finished one turn successfully.
    /// 控制器成功完成了一次回合。
    Completed,
    /// The controller failed during the current turn.
    /// 控制器在当前回合中失败。
    Failed,
}

/// Owns the controller configuration, canonical snapshot, request trace, and emitted events.
/// 持有控制器配置、规范快照、请求轨迹与已发出事件。
#[derive(Debug, Clone)]
pub struct VulcanCoreController {
    /// Stable controller configuration for the current session.
    /// 当前会话使用的稳定控制器配置。
    config: ControllerConfig,
    /// Canonical conversation snapshot owned by the controller.
    /// 由控制器持有的规范会话快照。
    snapshot: ConversationSnapshot,
    /// Latest serialized request snapshot captured before provider execution.
    /// 在 Provider 执行前捕获的最新序列化请求快照。
    last_request_snapshot: Option<Value>,
    /// Accumulated controller events emitted during the current lifecycle.
    /// 当前生命周期内累计的控制器事件。
    events: Vec<CoreEvent>,
    /// Current coarse-grained controller phase.
    /// 当前粗粒度控制器阶段。
    phase: ControllerPhase,
    /// Local identifier counter used for synthesized message and record identifiers.
    /// 用于合成消息与记录标识的本地递增计数器。
    local_id_counter: u64,
    /// Last successful provider usage captured during the current controller run.
    /// 当前控制器运行期间捕获到的最近一次成功 Provider 用量。
    last_successful_provider_usage: Option<ProviderUsageSnapshot>,
}

/// Collects provider-stream deltas during one request so the controller can apply them in order after the turn finishes.
/// 在单次请求期间收集 Provider 流式增量，以便控制器在回合结束后按顺序应用它们。
#[derive(Debug, Default)]
struct BufferedProviderDeltaSink {
    /// Buffered provider deltas captured during the current request lifecycle.
    /// 在当前请求生命周期内捕获到的 Provider 增量缓冲集合。
    deltas: Vec<ProviderTurnDelta>,
}

impl VulcanProviderDeltaSink for BufferedProviderDeltaSink {
    /// Pushes one provider-stream delta into the in-memory buffer.
    /// 将一条 Provider 流式增量推入内存缓冲区。
    fn push_delta(&mut self, delta: ProviderTurnDelta) {
        self.deltas.push(delta);
    }
}
mod completion;
mod lifecycle;
mod metadata;
mod provider_turns;
mod run_loop;
mod tool_execution;
mod transcript;

pub use transcript::AgentMessageAppendRequest;

use metadata::*;

#[cfg(test)]
mod tests;

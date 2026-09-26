//! Shared normalized-stream lifecycle validation and semantic-output observation.
//! 共用规范化流生命周期校验与语义输出观察。

use super::failure::{ProviderExecutionFailure, stream_protocol_failure};
use vulcan_ai_contract::{AiStreamEvent, AiStreamStateMachine};
use vulcan_llm_core::ProtocolConversionError;

/// Returns whether one normalized event makes transparent replay unsafe.
/// 返回一条归一化事件是否会使透明重放变得不安全。
///
/// # Parameters
/// 参数
/// `event` is the exact normalized event emitted by one physical provider attempt.
/// `event` 是一次真实供应商尝试发出的精确归一化事件。
///
/// # Returns
/// 返回值
/// `true` for user-visible, tool-visible, or resource-side-effect semantic output.
/// 对用户可见、工具可见或资源副作用语义输出返回 `true`。
fn stream_event_has_semantic_output(event: &AiStreamEvent) -> bool {
    matches!(
        event,
        AiStreamEvent::ResponseTextDelta { .. }
            | AiStreamEvent::ReasoningDelta { .. }
            | AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::UsageUpdated { .. }
    )
}

/// Tracks semantic output and annotates terminal public failures with the observed state.
/// 跟踪语义输出并用已观测状态标注公共终态失败。
pub(super) fn update_semantic_output_observation(observed: &mut bool, event: &mut AiStreamEvent) {
    if stream_event_has_semantic_output(event) {
        *observed = true;
    }
    if let AiStreamEvent::ResponseFailed { error } = event {
        error.semantic_output_observed = *observed;
    }
}

/// Applies the production stream state machine and preserves the physical request state on failure.
/// 应用生产流状态机，并在失败时保留物理请求状态。
///
/// # Parameters
/// 参数
/// `state` owns lifecycle validation, `event` is the next normalized event, and `observed`
/// records whether semantic output has already escaped the transport boundary.
/// `state` 持有生命周期校验，`event` 是下一条规范化事件，`observed`
/// 记录语义输出是否已经越过传输边界。
///
/// # Returns
/// 返回值
/// `Ok(())` for a valid transition or a structured dispatched protocol failure.
/// 有效转换返回 `Ok(())`，否则返回结构化的已分派协议失败。
pub(super) fn validate_stream_event(
    state: &mut AiStreamStateMachine,
    event: &AiStreamEvent,
    observed: bool,
) -> Result<(), ProviderExecutionFailure> {
    state
        .apply(event)
        .map_err(|message| stream_protocol_failure(ProtocolConversionError::new(message), observed))
}

#[cfg(test)]
mod tests;

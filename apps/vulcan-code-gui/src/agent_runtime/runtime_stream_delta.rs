//! Runtime streaming delta processing helpers and mutable loop state.
//! 运行时流式增量处理辅助逻辑与可变循环状态模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_stream_projection::{
    is_provider_tool_boundary_runtime_event, normalize_streamed_thinking_output,
    reset_current_thinking_phase_tracking, runtime_event_to_core_event,
    take_current_thinking_phase_completed_event,
};
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use std::mem;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Owns the mutable runtime-stream loop state that changes while provider deltas are consumed.
/// 持有在消费 Provider 增量期间持续变化的运行时流式循环状态。
pub(crate) struct RuntimeStreamDeltaState {
    /// Host-owned controller used to persist streamed reasoning, outputs, and runtime events.
    /// 负责持久化流式思考、输出与运行时事件的宿主自有控制器。
    core_controller: VulcanCoreController,
    /// Raw provider reasoning buffer used before synthetic preamble normalization.
    /// 在合成前导语规范化之前累计的 Provider 原始思考缓冲区。
    thinking_raw_output: String,
    /// Visible reasoning buffer after normalization and preamble stripping.
    /// 完成规范化与前导语剥离后的可见思考缓冲区。
    thinking_visible_output: String,
    /// Visible assistant output buffer accumulated across output deltas.
    /// 跨输出增量累计得到的可见助手输出缓冲区。
    assistant_output: String,
    /// Marks whether the current thinking phase has already emitted its completion event.
    /// 标记当前思考阶段是否已经发出过完成事件。
    thinking_completed_emitted: bool,
    /// Latest structured request snapshot captured before the most recent provider request.
    /// 最近一次 Provider 请求发出前捕获到的最新结构化请求快照。
    latest_request_snapshot: Option<CoreProviderRequestSnapshot>,
    /// Number of real provider requests observed during the current runtime stream.
    /// 当前运行时流期间观测到的真实 Provider 请求次数。
    provider_request_count: usize,
    /// Final canonical conversation state returned by the provider loop, if it has arrived.
    /// 如果已经收到，则保存 Provider 循环返回的最终规范会话状态。
    final_conversation_state: Option<SessionConversationState>,
    /// Latest successful provider usage emitted by the current provider loop.
    /// 当前 Provider 循环发出的最近一次成功 Provider 用量。
    last_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    /// Terminal provider error captured from the stream, if one has occurred.
    /// 如果已经发生，则保存从流中捕获到的终态 Provider 错误。
    terminal_error: Option<String>,
}

impl RuntimeStreamDeltaState {
    /// Creates a new mutable runtime-stream state seeded with one host-owned controller.
    /// 使用一个宿主自有控制器创建新的可变运行时流式状态。
    pub(crate) fn new(core_controller: VulcanCoreController) -> Self {
        Self {
            core_controller,
            thinking_raw_output: String::new(),
            thinking_visible_output: String::new(),
            assistant_output: String::new(),
            thinking_completed_emitted: false,
            latest_request_snapshot: None,
            provider_request_count: 0,
            final_conversation_state: None,
            last_successful_provider_usage: None,
            terminal_error: None,
        }
    }

    /// Returns the immutable host-owned controller captured by the loop state.
    /// 返回循环状态持有的不可变宿主控制器。
    pub(crate) fn controller(&self) -> &VulcanCoreController {
        &self.core_controller
    }

    /// Returns the mutable host-owned controller captured by the loop state.
    /// 返回循环状态持有的可变宿主控制器。
    pub(crate) fn controller_mut(&mut self) -> &mut VulcanCoreController {
        &mut self.core_controller
    }

    /// Returns the latest structured request snapshot captured during the stream, if one exists.
    /// 返回流式执行期间最近一次捕获到的结构化请求快照（如果存在）。
    pub(crate) fn latest_request_snapshot(&self) -> Option<&CoreProviderRequestSnapshot> {
        self.latest_request_snapshot.as_ref()
    }

    /// Returns the number of real provider requests captured during the stream so far.
    /// 返回截至当前流式执行期间已经捕获到的真实 Provider 请求次数。
    pub(crate) const fn provider_request_count(&self) -> usize {
        self.provider_request_count
    }

    /// Returns the normalized visible thinking output accumulated so far.
    /// 返回截至当前累计得到的规范化可见思考输出。
    pub(crate) fn thinking_visible_output(&self) -> &str {
        &self.thinking_visible_output
    }

    /// Drains and returns the accumulated assistant output buffer.
    /// 提取并返回累计的助手输出缓冲区。
    pub(crate) fn take_assistant_output(&mut self) -> String {
        mem::take(&mut self.assistant_output)
    }

    /// Drains and returns the final canonical conversation state captured from the stream.
    /// 提取并返回从流中捕获到的最终规范会话状态。
    pub(crate) fn take_final_conversation_state(&mut self) -> Option<SessionConversationState> {
        self.final_conversation_state.take()
    }

    /// Drains and returns the latest successful provider usage captured during this stream.
    /// 提取并返回本次流式执行期间捕获到的最近一次成功 Provider 用量。
    pub(crate) fn take_last_successful_provider_usage(
        &mut self,
    ) -> Option<CoreProviderUsageSnapshot> {
        self.last_successful_provider_usage.take()
    }

    /// Drains and returns the terminal stream error, if one has been captured.
    /// 提取并返回已经捕获到的终态流错误（如果存在）。
    pub(crate) fn take_terminal_error(&mut self) -> Option<String> {
        self.terminal_error.take()
    }

    /// Returns whether one terminal stream error has already been captured and is waiting for finalization.
    /// 返回是否已经捕获到一条等待收尾处理的终态流错误。
    pub(crate) fn has_terminal_error(&self) -> bool {
        self.terminal_error.is_some()
    }

    /// Completes the current floating thinking phase exactly once when visible reasoning content exists.
    /// 当存在可见思考内容时，将当前浮动思考阶段恰好一次收口为完成事件。
    pub(crate) fn complete_pending_thinking(
        &mut self,
        session_id: SessionId,
    ) -> Option<ExecutionStreamEvent> {
        take_current_thinking_phase_completed_event(
            session_id,
            &self.thinking_visible_output,
            &mut self.thinking_completed_emitted,
        )
    }

    /// Resets all per-phase thinking trackers after one real tool boundary.
    /// 在一次真实工具边界后重置所有按阶段维护的思考跟踪器。
    pub(crate) fn reset_thinking_phase_tracking(&mut self) {
        reset_current_thinking_phase_tracking(
            &mut self.thinking_raw_output,
            &mut self.thinking_visible_output,
            &mut self.thinking_completed_emitted,
        );
    }

    /// Applies one provider thinking delta to controller state and returns one visible event when new normalized text appears.
    /// 将一条 Provider 思考增量应用到控制器状态，并在出现新的规范化可见文本时返回一个可见事件。
    fn process_thinking_delta(
        &mut self,
        session_id: SessionId,
        delta: String,
    ) -> Option<ExecutionStreamEvent> {
        self.core_controller.record_thinking(delta.clone());
        self.thinking_raw_output.push_str(&delta);
        let normalized_visible_output =
            normalize_streamed_thinking_output(&self.thinking_raw_output);
        let visible_delta = normalized_visible_output
            .strip_prefix(&self.thinking_visible_output)
            .unwrap_or(normalized_visible_output.as_str())
            .to_string();
        self.thinking_visible_output = normalized_visible_output;

        (!visible_delta.is_empty()).then_some(ExecutionStreamEvent::ThinkingDelta {
            session_id,
            delta: visible_delta,
        })
    }

    /// Applies one provider output delta and emits the pending thinking-completed event before the visible output delta when needed.
    /// 应用一条 Provider 输出增量，并在需要时先发出待收口的思考完成事件，再发出可见输出增量。
    fn process_output_delta(
        &mut self,
        session_id: SessionId,
        delta: String,
    ) -> Vec<ExecutionStreamEvent> {
        let mut events = Vec::new();
        self.core_controller.record_output(delta.clone());
        if let Some(thinking_completed_event) = self.complete_pending_thinking(session_id) {
            events.push(thinking_completed_event);
        }
        self.assistant_output.push_str(&delta);
        events.push(ExecutionStreamEvent::OutputDelta { session_id, delta });
        events
    }

    /// Applies one runtime event, sealing the current thinking phase on real tool boundaries before forwarding the event.
    /// 应用一条运行时事件，并在真实工具边界上先收口当前思考阶段，再继续转发该事件。
    fn process_runtime_event(
        &mut self,
        session_id: SessionId,
        event: RuntimeEvent,
    ) -> Vec<ExecutionStreamEvent> {
        let mut events = Vec::new();
        if is_provider_tool_boundary_runtime_event(&event) {
            if let Some(thinking_completed_event) = self.complete_pending_thinking(session_id) {
                events.push(thinking_completed_event);
            }
            self.reset_thinking_phase_tracking();
        }
        if let Some(core_event) = runtime_event_to_core_event(&event) {
            self.core_controller.push_event(core_event);
        }
        events.push(ExecutionStreamEvent::RuntimeEvent { session_id, event });
        events
    }
}

/// Processes one provider-stream item, mutates the loop state in place, and returns the execution events that should be emitted immediately.
/// 处理一条 Provider 流项目，就地更新循环状态，并返回应立即发出的执行事件集合。
pub(crate) fn process_provider_stream_delta(
    session_id: SessionId,
    state: &mut RuntimeStreamDeltaState,
    item: Result<ProviderStreamDelta, String>,
) -> Vec<ExecutionStreamEvent> {
    match item {
        Ok(ProviderStreamDelta::RequestSnapshot(snapshot)) => {
            state.latest_request_snapshot = Some(snapshot);
            state.provider_request_count = state.provider_request_count.saturating_add(1);
            vec![ExecutionStreamEvent::RequestStarted { session_id }]
        }
        Ok(ProviderStreamDelta::Thinking(delta)) => state
            .process_thinking_delta(session_id, delta)
            .into_iter()
            .collect(),
        Ok(ProviderStreamDelta::Output(delta)) => state.process_output_delta(session_id, delta),
        Ok(ProviderStreamDelta::RuntimeEvent(event)) => {
            state.process_runtime_event(session_id, event)
        }
        Ok(ProviderStreamDelta::FinalConversationState(
            conversation_state,
            last_successful_provider_usage,
        )) => {
            state.final_conversation_state = Some(conversation_state);
            if let Some(usage) = last_successful_provider_usage {
                state.last_successful_provider_usage = Some(usage);
            }
            Vec::new()
        }
        Err(error) => {
            state.terminal_error = Some(error);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    //! Runtime-stream delta tests for host-owned provider loop projection behavior.
    //! 面向宿主自有 Provider 循环投影行为的运行时流式增量测试。

    use super::*;
    use crate::agent_runtime::events::RuntimeEventKind;
    use serde_json::json;
    use uuid::Uuid;
    use vulcan_core::types::{ControllerConfig, ConversationSnapshot};

    /// Builds one minimal controller-backed delta state for unit tests.
    /// 为单元测试构建一份最小控制器支撑的增量状态。
    fn build_delta_state() -> RuntimeStreamDeltaState {
        RuntimeStreamDeltaState::new(VulcanCoreController::new(
            ControllerConfig {
                provider_id: String::from("mock-provider"),
                model_id: String::from("mock-model"),
                system_prompt: String::from("system"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system").clone(),
                ),
                temperature: Some(0.1),
                max_turns: 4,
                additional_params: None,
            },
            ConversationSnapshot::default(),
        ))
    }

    /// Emits the pending thinking-completed event before the first visible output delta once reasoning has already become visible.
    /// 一旦思考内容已经变为可见，就会在首个正式输出增量之前先发出待收口的思考完成事件。
    #[test]
    fn output_delta_seals_visible_thinking_before_streaming_output() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();

        let thinking_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::Thinking(String::from("实际思考内容"))),
        );
        assert!(matches!(
            thinking_events.as_slice(),
            [ExecutionStreamEvent::ThinkingDelta { delta, .. }] if delta == "实际思考内容"
        ));

        let output_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::Output(String::from("正式输出"))),
        );
        assert!(matches!(
            output_events.as_slice(),
            [
                ExecutionStreamEvent::ThinkingCompleted { .. },
                ExecutionStreamEvent::OutputDelta { delta, .. }
            ] if delta == "正式输出"
        ));
    }

    /// Seals and resets the floating thinking phase when one real tool-boundary runtime event arrives.
    /// 当收到真实工具边界运行时事件时，会收口并重置当前浮动思考阶段。
    #[test]
    fn tool_boundary_runtime_event_seals_and_resets_thinking_phase() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();

        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::Thinking(String::from("先准备参数"))),
        );

        let runtime_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RuntimeEvent(
                RuntimeEvent::new(
                    RuntimeEventKind::ToolStarted,
                    "工具调用 · read_file",
                    "开始调用工具",
                )
                .with_metadata(Some(json!({"tool_call_id":"call-1"}))),
            )),
        );
        assert!(matches!(
            runtime_events.as_slice(),
            [
                ExecutionStreamEvent::ThinkingCompleted { .. },
                ExecutionStreamEvent::RuntimeEvent { event, .. }
            ] if event.title == "工具调用 · read_file"
        ));
        assert!(state.thinking_visible_output().is_empty());

        let follow_up_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::Thinking(String::from("继续分析"))),
        );
        assert!(matches!(
            follow_up_events.as_slice(),
            [ExecutionStreamEvent::ThinkingDelta { delta, .. }] if delta == "继续分析"
        ));
    }

    /// Captures terminal errors and final conversation state without emitting immediate GUI events from the delta processor itself.
    /// 捕获终态错误与最终会话状态时，增量处理器自身不会立刻额外发出 GUI 事件。
    #[test]
    fn terminal_error_and_final_state_are_buffered_for_finalization() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();

        let final_state_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::FinalConversationState(
                SessionConversationState::default(),
                None,
            )),
        );
        assert!(final_state_events.is_empty());
        assert!(state.take_final_conversation_state().is_some());

        let error_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Err(String::from("provider failed")),
        );
        assert!(error_events.is_empty());
        assert_eq!(
            state.take_terminal_error().as_deref(),
            Some("provider failed")
        );
    }
}

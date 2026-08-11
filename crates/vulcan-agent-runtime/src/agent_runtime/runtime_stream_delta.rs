//! Runtime streaming delta processing helpers and mutable loop state.
//! 运行时流式增量处理辅助逻辑与可变循环状态模块。

use crate::agent_runtime::events::{
    ExecutionStreamEvent, ProviderRequestDurabilityCompletion, RuntimeEvent,
};
use crate::agent_runtime::provider_request_telemetry::{
    ProviderRequestTelemetryContext, ProviderRequestTelemetryEvent,
    ProviderRequestTelemetryEventKind, ProviderRequestTelemetryTracker,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_stream_projection::{
    is_provider_tool_boundary_runtime_event, normalize_streamed_thinking_output,
    reset_current_thinking_phase_tracking, runtime_event_to_core_event,
    take_current_thinking_phase_completed_event,
};
use crate::agent_runtime::provider_transport_request_budget::ProviderUsageBudgetBaselineHandle;
use crate::agent_runtime::tool_request_stream::ToolRequestStreamCoordinator;
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use serde_json::Value;
use std::mem;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Owns the mutable runtime-stream loop state that changes while provider deltas are consumed.
/// 持有在消费 Provider 增量期间持续变化的运行时流式循环状态。
pub(crate) struct RuntimeStreamDeltaState {
    /// Host-owned controller used to persist streamed reasoning, outputs, and runtime events.
    /// 负责持久化流式思考、输出与运行时事件的宿主自有控制器。
    core_controller: VulcanCoreController,
    /// Canonical provider API family resolved before the runtime stream starts.
    /// 运行时流启动前解析出的规范 Provider API 协议族。
    protocol_profile_id: String,
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
    /// Latest final wire-level provider request captured before network dispatch.
    /// 网络发送前捕获到的最近一次最终 wire-level Provider 请求。
    latest_wire_request_json: Option<Value>,
    /// Number of real provider requests observed during the current runtime stream.
    /// 当前运行时流期间观测到的真实 Provider 请求次数。
    provider_request_count: usize,
    /// Final canonical conversation state returned by the provider loop, if it has arrived.
    /// 如果已经收到，则保存 Provider 循环返回的最终规范会话状态。
    final_conversation_state: Option<SessionConversationState>,
    /// Latest successful provider usage emitted by the current provider loop.
    /// 当前 Provider 循环发出的最近一次成功 Provider 用量。
    last_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    /// Cumulative successful provider usage for the current runtime stream.
    /// 当前运行时流中成功 Provider 调用的累计用量。
    cumulative_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    /// Latest accumulated usage for the currently active physical request, including a failed partial response.
    /// 当前活动真实请求的最新累计用量，包括失败响应已产生的部分用量。
    active_provider_usage: Option<CoreProviderUsageSnapshot>,
    /// Number of successful assistant provider turns already reflected in the cumulative usage.
    /// 已经计入累计用量的成功助手 Provider 回合数量。
    cumulative_usage_accounted_assistant_turns: usize,
    /// Terminal provider error captured from the stream, if one has occurred.
    /// 如果已经发生，则保存从流中捕获到的终态 Provider 错误。
    terminal_error: Option<String>,
    /// Host-owned context compaction request captured at a replay-safe boundary.
    /// 在可回放安全边界捕获的宿主上下文压缩请求。
    context_compaction_required: Option<(SessionConversationState, String)>,
    /// Low-frequency physical Provider request telemetry state machine.
    /// 低频真实 Provider 请求遥测状态机。
    provider_request_telemetry: ProviderRequestTelemetryTracker,
    /// Model-side tool request stream coordinator kept separate from real tool execution.
    /// 与真实工具执行分离保存的模型侧工具请求流协调器。
    tool_request_stream: ToolRequestStreamCoordinator,
    /// Whether the provider emitted any model-side tool request during this run.
    /// 当前运行期间 Provider 是否发出过任何模型侧工具请求。
    observed_tool_request: bool,
}

impl RuntimeStreamDeltaState {
    /// Creates a new mutable runtime-stream state from exact controller and provider facts.
    /// 使用精确的控制器与 Provider 事实创建新的可变运行时流式状态。
    ///
    /// # Parameters
    /// 参数
    /// `core_controller` owns canonical conversation projection for the stream.
    /// `core_controller` 持有当前流的规范会话投影。
    /// `provider_api_family` is the canonical typed API family resolved during execution preparation.
    /// `provider_api_family` 是执行准备期间解析出的规范强类型 API 协议族。
    ///
    /// # Returns
    /// 返回值
    /// Returns a fresh stream state that preserves the immutable provider family.
    /// 返回保留不可变 Provider 协议族的新流状态。
    pub(crate) fn new(core_controller: VulcanCoreController, protocol_profile_id: String) -> Self {
        Self {
            core_controller,
            protocol_profile_id,
            thinking_raw_output: String::new(),
            thinking_visible_output: String::new(),
            assistant_output: String::new(),
            thinking_completed_emitted: false,
            latest_request_snapshot: None,
            latest_wire_request_json: None,
            provider_request_count: 0,
            final_conversation_state: None,
            last_successful_provider_usage: None,
            cumulative_successful_provider_usage: None,
            active_provider_usage: None,
            cumulative_usage_accounted_assistant_turns: 0,
            terminal_error: None,
            context_compaction_required: None,
            provider_request_telemetry: ProviderRequestTelemetryTracker::default(),
            tool_request_stream: ToolRequestStreamCoordinator::default(),
            observed_tool_request: false,
        }
    }

    /// Installs the frozen tool snapshot used by streamed request progress.
    /// 安装流式请求进度使用的冻结工具快照。
    ///
    /// # Parameters
    /// 参数
    /// `resolved_tools` is the frozen tool snapshot used for exact request parser routing.
    /// `resolved_tools` 是用于精确请求解析器路由的冻结工具快照。
    ///
    /// # Returns
    /// 返回值
    /// The updated runtime stream state.
    /// 返回更新后的运行时流状态。
    pub(crate) fn with_tool_request_stream(
        mut self,
        resolved_tools: &[ResolvedToolDefinition],
    ) -> Self {
        self.tool_request_stream = ToolRequestStreamCoordinator::new(resolved_tools);
        self
    }

    /// Installs immutable request telemetry context derived from execution preparation.
    /// 安装从执行准备派生的不可变请求遥测上下文。
    ///
    /// # Parameters
    /// 参数
    /// `context` contains the exact model and owner facts for this stream.
    /// `context` 包含当前流的精确模型与归属事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated state for launch assembly.
    /// 返回用于启动装配的更新状态。
    pub(crate) fn with_provider_request_telemetry_context(
        mut self,
        context: ProviderRequestTelemetryContext,
    ) -> Self {
        self.provider_request_telemetry.install_context(context);
        self
    }

    /// Returns the durable Usage baseline handle shared by telemetry and request budgeting.
    /// 返回遥测与请求预算共享的持久化用量基线句柄。
    pub(crate) fn provider_usage_budget_baseline_handle(
        &self,
    ) -> Option<ProviderUsageBudgetBaselineHandle> {
        self.provider_request_telemetry.budget_baseline_handle()
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

    /// Returns the latest final wire-level provider request captured during the stream.
    /// 返回流式执行期间捕获到的最近一次最终 wire-level Provider 请求。
    pub(crate) fn latest_wire_request_json(&self) -> Option<&Value> {
        self.latest_wire_request_json.as_ref()
    }

    /// Returns the number of real provider requests captured during the stream so far.
    /// 返回截至当前流式执行期间已经捕获到的真实 Provider 请求次数。
    pub(crate) const fn provider_request_count(&self) -> usize {
        self.provider_request_count
    }

    /// Reports whether this run produced at least one provider tool request.
    /// 返回当前运行是否至少产生过一次 Provider 工具请求。
    pub(crate) const fn observed_tool_request(&self) -> bool {
        self.observed_tool_request
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

    /// Drains and returns cumulative successful provider usage captured during this stream.
    /// 提取并返回本次流式执行期间捕获到的成功 Provider 累计用量。
    pub(crate) fn take_cumulative_successful_provider_usage(
        &mut self,
    ) -> Option<CoreProviderUsageSnapshot> {
        self.cumulative_successful_provider_usage.take()
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

    /// Drains a pending host-owned context compaction request.
    /// 提取等待中的宿主上下文压缩请求。
    pub(crate) fn take_context_compaction_required(
        &mut self,
    ) -> Option<(SessionConversationState, String)> {
        self.context_compaction_required.take()
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
    ) -> Vec<ExecutionStreamEvent> {
        self.core_controller.record_thinking(delta.clone());
        self.thinking_raw_output.push_str(&delta);
        let normalized_visible_output =
            normalize_streamed_thinking_output(&self.thinking_raw_output);
        let visible_delta = normalized_visible_output
            .strip_prefix(&self.thinking_visible_output)
            .unwrap_or(normalized_visible_output.as_str())
            .to_string();
        self.thinking_visible_output = normalized_visible_output;

        if visible_delta.is_empty() {
            self.provider_request_telemetry.observe_first_event();
            return Vec::new();
        }
        // events preserves the first-token Hook before the visible delta projection.
        // events 保留首 Token Hook 先于可见增量投影的顺序。
        let mut events = Vec::new();
        if let Some(telemetry_event) = self.provider_request_telemetry.observe_first_token() {
            events.push(ExecutionStreamEvent::ProviderRequestTelemetry(
                telemetry_event,
            ));
        }
        events.push(ExecutionStreamEvent::ThinkingDelta {
            session_id,
            delta: visible_delta,
        });
        events
    }

    /// Applies one provider output delta and emits the pending thinking-completed event before the visible output delta when needed.
    /// 应用一条 Provider 输出增量，并在需要时先发出待收口的思考完成事件，再发出可见输出增量。
    fn process_output_delta(
        &mut self,
        session_id: SessionId,
        delta: String,
    ) -> Vec<ExecutionStreamEvent> {
        // events preserves the first-token Hook before visible output projection.
        // events 保留首 Token Hook 先于可见输出投影的顺序。
        let mut events = Vec::new();
        if !delta.is_empty()
            && let Some(telemetry_event) = self.provider_request_telemetry.observe_first_token()
        {
            events.push(ExecutionStreamEvent::ProviderRequestTelemetry(
                telemetry_event,
            ));
        }
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
        self.provider_request_telemetry.observe_first_event();
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

    /// Records one newly completed successful provider turn usage exactly once.
    /// 将一个新完成的成功 Provider 回合用量恰好记录一次。
    ///
    /// # Parameters
    /// 参数
    /// `conversation_state` is the checkpoint state containing successful assistant turns.
    /// `conversation_state` 是包含成功助手回合的检查点状态。
    /// `usage` is the latest successful provider usage reported with that checkpoint.
    /// `usage` 是该检查点携带的最近一次成功 Provider 用量。
    fn record_successful_provider_usage_checkpoint(
        &mut self,
        conversation_state: &SessionConversationState,
        usage: &CoreProviderUsageSnapshot,
    ) {
        let successful_assistant_turns =
            successful_assistant_provider_turn_count(conversation_state);
        if successful_assistant_turns <= self.cumulative_usage_accounted_assistant_turns {
            return;
        }
        let cumulative = self
            .cumulative_successful_provider_usage
            .get_or_insert_with(CoreProviderUsageSnapshot::default);
        cumulative.accumulate_from(usage);
        self.cumulative_usage_accounted_assistant_turns = successful_assistant_turns;
    }
}

/// Counts assistant messages that represent completed provider turns in one checkpoint.
/// 统计一个检查点中代表已完成 Provider 回合的助手消息数量。
///
/// # Parameters
/// 参数
/// `conversation_state` is the canonical conversation checkpoint to inspect.
/// `conversation_state` 是需要检查的规范会话检查点。
///
/// # Returns
/// 返回值
/// The number of assistant messages that carry visible output, reasoning, tools, or usage.
/// 返回携带可见输出、思考、工具调用或用量的助手消息数量。
fn successful_assistant_provider_turn_count(
    conversation_state: &SessionConversationState,
) -> usize {
    conversation_state
        .messages
        .iter()
        .filter(|message| match &message.payload {
            crate::conversation_protocol::SessionConversationPayload::Assistant {
                text,
                reasoning,
                usage,
                tool_calls,
                ..
            } => {
                !text.trim().is_empty()
                    || !reasoning.trim().is_empty()
                    || usage.is_some()
                    || !tool_calls.is_empty()
            }
            crate::conversation_protocol::SessionConversationPayload::UserText { .. }
            | crate::conversation_protocol::SessionConversationPayload::ToolResult { .. }
            | crate::conversation_protocol::SessionConversationPayload::AgentMessage { .. } => {
                false
            }
        })
        .count()
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
            state.active_provider_usage = None;
            state.latest_request_snapshot = Some(snapshot.clone());
            state.provider_request_count = state.provider_request_count.saturating_add(1);
            // events begins with telemetry so an effect Hook can observe the request before UI timing markers.
            // events 以遥测开始，使 Effect Hook 能在界面计时标记前观测请求。
            let telemetry_events = match state
                .provider_request_telemetry
                .start_request(&snapshot, None)
            {
                Ok(events) => events,
                Err(error) => {
                    state.terminal_error = Some(format!(
                        "Provider request evidence validation failed before dispatch: {error}"
                    ));
                    return Vec::new();
                }
            };
            let mut events = telemetry_events
                .into_iter()
                .map(ExecutionStreamEvent::ProviderRequestTelemetry)
                .collect::<Vec<_>>();
            events.extend([
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    provider_family: state.protocol_profile_id.clone(),
                    snapshot,
                },
                ExecutionStreamEvent::RequestReady { session_id },
                ExecutionStreamEvent::RequestStarted { session_id },
            ]);
            events
        }
        Ok(ProviderStreamDelta::RequestPrepared {
            snapshot,
            context_budget,
            completion,
        }) => {
            state.active_provider_usage = None;
            state.latest_request_snapshot = Some(snapshot.clone());
            state.provider_request_count = state.provider_request_count.saturating_add(1);
            let telemetry_events = match state
                .provider_request_telemetry
                .start_request(&snapshot, context_budget.as_ref())
            {
                Ok(events) => events,
                Err(error) => {
                    let error = format!(
                        "Provider request evidence validation failed before dispatch: {error}"
                    );
                    let _ = completion.complete(Err(error.clone()));
                    state.terminal_error = Some(error);
                    return Vec::new();
                }
            };
            let mut events =
                provider_request_events_with_durability_boundary(telemetry_events, completion);
            events.extend([
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    provider_family: state.protocol_profile_id.clone(),
                    snapshot,
                },
                ExecutionStreamEvent::RequestReady { session_id },
                ExecutionStreamEvent::RequestStarted { session_id },
            ]);
            events
        }
        Ok(ProviderStreamDelta::TransportObserved {
            transport,
            dispatched_at_ms,
            headers_at_ms,
            http_status,
            provider_request_id,
            rate_limit_headers,
            completion,
        }) => {
            let telemetry_events = state
                .provider_request_telemetry
                .observe_transport(
                    transport,
                    dispatched_at_ms,
                    headers_at_ms,
                    http_status,
                    provider_request_id,
                    rate_limit_headers,
                )
                .into_iter()
                .collect();
            provider_request_events_with_durability_boundary(telemetry_events, completion)
        }
        Ok(ProviderStreamDelta::WireRequest(request_json)) => {
            state.latest_wire_request_json = Some(request_json);
            vec![ExecutionStreamEvent::WireRequestReady { session_id }]
        }
        Ok(ProviderStreamDelta::Thinking(delta)) => state.process_thinking_delta(session_id, delta),
        Ok(ProviderStreamDelta::Output(delta)) => state.process_output_delta(session_id, delta),
        Ok(ProviderStreamDelta::ToolRequestFragment(fragment)) => {
            state.observed_tool_request = true;
            state.provider_request_telemetry.observe_semantic_output();
            // runtimeEvents are safe low-frequency projections produced without raw arguments.
            // runtimeEvents 是在不包含原始参数的前提下生成的安全低频投影。
            let runtime_events = state.tool_request_stream.apply(fragment);
            runtime_events
                .into_iter()
                .flat_map(|event| state.process_runtime_event(session_id, event))
                .collect()
        }
        Ok(ProviderStreamDelta::RuntimeEvent(event)) => {
            state.process_runtime_event(session_id, event)
        }
        Ok(ProviderStreamDelta::UsageUpdated(usage)) => {
            state.active_provider_usage = Some(usage);
            Vec::new()
        }
        Ok(ProviderStreamDelta::RequestSucceeded { usage, completion }) => {
            state.active_provider_usage = usage;
            let events = state
                .provider_request_telemetry
                .complete_success(state.active_provider_usage.as_ref())
                .into_iter()
                .collect::<Vec<_>>();
            provider_request_events_with_durability_boundary(events, completion)
        }
        Ok(ProviderStreamDelta::RequestFailed {
            will_retry,
            retry_number,
            max_retries,
            delay_ms,
            error_category,
            error,
        }) => state
            .provider_request_telemetry
            .complete_failure(
                will_retry,
                retry_number,
                max_retries,
                delay_ms,
                &error_category,
                error.as_deref(),
                state.active_provider_usage.as_ref(),
            )
            .into_iter()
            .map(ExecutionStreamEvent::ProviderRequestTelemetry)
            .collect(),
        Ok(ProviderStreamDelta::RequestFailedDurable {
            will_retry,
            retry_number,
            max_retries,
            delay_ms,
            error_category,
            error,
            completion,
        }) => {
            let events = state.provider_request_telemetry.complete_failure(
                will_retry,
                retry_number,
                max_retries,
                delay_ms,
                &error_category,
                error.as_deref(),
                state.active_provider_usage.as_ref(),
            );
            provider_request_events_with_durability_boundary(events, completion)
        }
        Ok(ProviderStreamDelta::LoweringRepair(event)) => {
            vec![ExecutionStreamEvent::ProviderLoweringRepair { session_id, event }]
        }
        Ok(ProviderStreamDelta::ContextCompactionRequired {
            conversation_state,
            reason,
        }) => {
            state.context_compaction_required = Some((conversation_state.clone(), reason.clone()));
            Vec::new()
        }
        Ok(ProviderStreamDelta::FinalConversationState(
            conversation_state,
            last_successful_provider_usage,
        )) => {
            state.final_conversation_state = Some(conversation_state.clone());
            if let Some(ref usage) = last_successful_provider_usage {
                state.last_successful_provider_usage = Some(usage.clone());
                state.record_successful_provider_usage_checkpoint(&conversation_state, usage);
            }
            // events closes the active physical request before publishing its replayable checkpoint.
            // events 在发布可回放检查点前关闭活动真实请求。
            let mut events = state
                .provider_request_telemetry
                .complete_success(last_successful_provider_usage.as_ref())
                .map(ExecutionStreamEvent::ProviderRequestTelemetry)
                .into_iter()
                .collect::<Vec<_>>();
            state.active_provider_usage = None;
            events.push(ExecutionStreamEvent::ConversationCheckpoint {
                session_id,
                conversation_state,
                last_successful_provider_usage: state.last_successful_provider_usage.clone(),
                provider_request_count: state.provider_request_count(),
            });
            events
        }
        Err(error) => {
            // telemetryEvents closes a request when an adapter error bypassed the typed failure delta.
            // telemetryEvents 在适配器错误绕过类型化失败增量时关闭请求。
            let telemetry_events = state
                .provider_request_telemetry
                .complete_failure(
                    false,
                    0,
                    0,
                    0,
                    "provider_stream_error",
                    None,
                    state.active_provider_usage.as_ref(),
                )
                .into_iter()
                .map(ExecutionStreamEvent::ProviderRequestTelemetry)
                .collect();
            state.terminal_error = Some(error);
            telemetry_events
        }
    }
}

/// Converts one started or terminal telemetry event into a durable execution barrier.
/// 将一条开始或终态遥测事件转换为可靠执行屏障。
///
/// # Parameters
/// 参数
/// `telemetry_events` are the ordered events produced by the physical-call tracker.
/// `telemetry_events` 是物理调用跟踪器产生的有序事件。
/// `completion` is released by the service after session-state persistence.
/// `completion` 在服务完成会话状态持久化后释放。
///
/// # Returns
/// 返回值
/// Returns ordered execution events with exactly one durable started or terminal boundary.
/// 返回包含恰好一个可靠开始或终态边界的有序执行事件。
fn provider_request_events_with_durability_boundary(
    telemetry_events: Vec<ProviderRequestTelemetryEvent>,
    completion: ProviderRequestDurabilityCompletion,
) -> Vec<ExecutionStreamEvent> {
    let boundary_index = telemetry_events.iter().rposition(|event| {
        matches!(
            event.kind,
            ProviderRequestTelemetryEventKind::Started
                | ProviderRequestTelemetryEventKind::Dispatched
                | ProviderRequestTelemetryEventKind::Completed
        )
    });
    let Some(boundary_index) = boundary_index else {
        let _ = completion.complete(Err(String::from(
            "Provider durability boundary did not contain a prepared, dispatched, or terminal telemetry event.",
        )));
        return telemetry_events
            .into_iter()
            .map(ExecutionStreamEvent::ProviderRequestTelemetry)
            .collect();
    };
    telemetry_events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            if index == boundary_index {
                ExecutionStreamEvent::ProviderRequestDurabilityBoundary {
                    event,
                    completion: completion.clone(),
                }
            } else {
                ExecutionStreamEvent::ProviderRequestTelemetry(event)
            }
        })
        .collect()
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
        RuntimeStreamDeltaState::new(
            VulcanCoreController::new(
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
                    context_injection: None,
                },
                ConversationSnapshot::default(),
            )
            .expect("valid core conversation snapshot"),
            String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
        )
    }

    /// Builds one delta state with exact request telemetry identities installed.
    /// 构建一份已安装精确请求遥测身份的增量状态。
    fn build_delta_state_with_telemetry() -> RuntimeStreamDeltaState {
        build_delta_state()
            .with_provider_request_telemetry_context(ProviderRequestTelemetryContext::for_test())
    }

    /// Preserves the prepared protocol profile when emitting one provider request checkpoint.
    /// 发出 Provider 请求检查点时保留执行准备阶段解析出的协议画像。
    #[test]
    fn request_checkpoint_uses_prepared_protocol_profile() {
        // session_id identifies the stream whose first physical request is observed.
        // session_id 标识观测到首个真实请求的运行时流。
        let session_id = Uuid::nil();
        // checkpoint_events captures the ordered telemetry, checkpoint, and phase markers.
        // checkpoint_events 捕获有序的遥测、检查点与阶段标记。
        let checkpoint_events = process_provider_stream_delta(
            session_id,
            &mut build_delta_state(),
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );

        assert!(checkpoint_events.iter().any(|event| matches!(
            event,
            ExecutionStreamEvent::ProviderRequestCheckpoint {
                provider_family,
                snapshot,
                ..
            } if provider_family == vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
                && snapshot.provider_id == "mock-provider"
        )));
    }

    /// Verifies an accumulated usage update survives a later physical request failure.
    /// 验证累计用量更新会在后续真实请求失败时保留下来。
    #[test]
    fn failed_stream_request_emits_partial_usage_telemetry() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state_with_telemetry();
        process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::UsageUpdated(
                CoreProviderUsageSnapshot {
                    input_tokens: Some(21),
                    output_tokens: Some(4),
                    total_tokens: Some(25),
                    ..CoreProviderUsageSnapshot::default()
                },
            )),
        );

        let events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RequestFailed {
                will_retry: false,
                retry_number: 0,
                max_retries: 0,
                delay_ms: 0,
                error_category: String::from("provider_transport_error"),
                error: None,
            }),
        );

        let completed_usage = events.into_iter().find_map(|event| match event {
            ExecutionStreamEvent::ProviderRequestTelemetry(event) => event.completed_usage,
            _ => None,
        });
        assert_eq!(
            completed_usage.and_then(|usage| usage.total_tokens),
            Some(25)
        );
    }

    /// Builds one minimal request snapshot so provider-request counting uses the real event shape.
    /// 构建一份最小请求快照，让 Provider 请求计数使用真实事件形态。
    fn build_request_snapshot() -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("mock-provider"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system"),
            ),
            prompt: String::from("prompt"),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: Some(0.1),
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: vulcan_core::types::ProviderContinuation::default(),
        }
    }

    /// Builds one provider usage snapshot for cumulative accounting tests.
    /// 为累计统计测试构建一份 Provider 用量快照。
    fn build_usage_snapshot(
        input_tokens: usize,
        output_tokens: usize,
        reasoning_tokens: usize,
        total_tokens: usize,
        cached_input_tokens: usize,
    ) -> CoreProviderUsageSnapshot {
        CoreProviderUsageSnapshot {
            input_tokens: Some(input_tokens),
            uncached_input_tokens: Some(input_tokens.saturating_sub(cached_input_tokens)),
            output_tokens: Some(output_tokens),
            reasoning_tokens: Some(reasoning_tokens),
            total_tokens: Some(total_tokens),
            cached_input_tokens: Some(cached_input_tokens),
            cache_creation_input_tokens: None,
            ..CoreProviderUsageSnapshot::default()
        }
    }

    /// Builds one canonical conversation checkpoint with visible successful assistant turns.
    /// 构建一份包含可见成功助手回合的规范会话检查点。
    fn build_successful_conversation_state(assistant_texts: &[&str]) -> SessionConversationState {
        SessionConversationState {
            messages: assistant_texts
                .iter()
                .enumerate()
                .map(
                    |(index, text)| crate::conversation_protocol::SessionConversationMessage {
                        id: format!("assistant-{index}"),
                        payload:
                            crate::conversation_protocol::SessionConversationPayload::Assistant {
                                text: (*text).to_string(),
                                reasoning: String::new(),
                                provider_message_id: None,
                                provider_message_state: None,
                                usage: None,
                                tool_calls: Vec::new(),
                            },
                    },
                )
                .collect(),
            records: Vec::new(),
            continuation: Default::default(),
        }
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

    /// Verifies request start seals thinking once while later request progress does not create another boundary.
    /// 验证请求开始只收口一次思考，而后续请求进度不会再次创建边界。
    #[test]
    fn tool_request_start_seals_thinking_only_once() {
        // state first receives visible reasoning that must end when the tool identity becomes known.
        // state 首先接收可见思考，并在工具身份明确时结束该思考。
        let session_id = Uuid::nil();
        let mut state = build_delta_state();
        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::Thinking(String::from(
                "正在组织长参数",
            ))),
        );
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolRequestStarted,
            "工具请求 · pending-1 · submit_plan",
            String::new(),
        );
        let started_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RuntimeEvent(started)),
        );
        assert!(matches!(
            started_events.as_slice(),
            [
                ExecutionStreamEvent::ThinkingCompleted { .. },
                ExecutionStreamEvent::RuntimeEvent { event, .. }
            ] if event.kind == RuntimeEventKind::ToolRequestStarted
        ));
        let progress = RuntimeEvent::new(
            RuntimeEventKind::ToolRequestProgress,
            "工具请求 · pending-1 · submit_plan",
            String::new(),
        );
        let progress_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RuntimeEvent(progress)),
        );
        assert!(matches!(
            progress_events.as_slice(),
            [ExecutionStreamEvent::RuntimeEvent { event, .. }]
                if event.kind == RuntimeEventKind::ToolRequestProgress
        ));
    }

    /// Forwards provider lowering repair deltas as audit-only execution events.
    /// 将 Provider 转换修复增量转发为仅用于审计的执行事件。
    #[test]
    fn lowering_repair_delta_becomes_audit_execution_event() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();

        let events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::LoweringRepair(
                vulcan_context::RuntimeProviderLoweringRepairEvent {
                    turn_id: None,
                    repair_id: String::from("repair-1"),
                    provider_id: String::from("anthropic"),
                    model_id: String::from("claude-test"),
                    provider_family: String::from("anthropic"),
                    action: String::from("downgrade_orphan_tool_result_to_text"),
                    reason: String::from("test"),
                    affected_items: 2,
                },
            )),
        );

        assert!(matches!(
            events.as_slice(),
            [ExecutionStreamEvent::ProviderLoweringRepair { event, .. }]
                if event.repair_id == "repair-1" && event.affected_items == 2
        ));
    }

    /// Accumulates successful provider usage across provider turns without double-counting duplicate checkpoints.
    /// 跨 Provider 回合累计成功用量，同时避免重复检查点造成重复计数。
    #[test]
    fn final_state_accumulates_provider_usage_once_per_successful_assistant_turn() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();
        let usage_one = build_usage_snapshot(20_000, 120, 30, 20_150, 19_500);
        let usage_two = build_usage_snapshot(22_000, 160, 40, 22_200, 21_200);

        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        let first_checkpoint = build_successful_conversation_state(&["第一次响应"]);
        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::FinalConversationState(
                first_checkpoint.clone(),
                Some(usage_one.clone()),
            )),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::FinalConversationState(
                first_checkpoint,
                Some(usage_one),
            )),
        );

        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        let final_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::FinalConversationState(
                build_successful_conversation_state(&["第一次响应", "第二次响应"]),
                Some(usage_two),
            )),
        );
        assert!(matches!(
            final_events.as_slice(),
            [ExecutionStreamEvent::ConversationCheckpoint {
                provider_request_count,
                ..
            }] if *provider_request_count == 2
        ));

        let cumulative_usage = state
            .take_cumulative_successful_provider_usage()
            .expect("successful provider usage should be accumulated");
        assert_eq!(cumulative_usage.input_tokens, Some(42_000));
        assert_eq!(cumulative_usage.output_tokens, Some(280));
        assert_eq!(cumulative_usage.reasoning_tokens, Some(70));
        assert_eq!(cumulative_usage.total_tokens, Some(42_350));
        assert_eq!(cumulative_usage.cached_input_tokens, Some(40_700));
    }

    /// Captures terminal errors while final conversation states also emit a safe workbench checkpoint.
    /// 捕获终态错误，同时让最终会话状态额外发出安全工作台检查点。
    #[test]
    fn terminal_error_is_buffered_and_final_state_emits_checkpoint() {
        let session_id = Uuid::nil();
        let mut state = build_delta_state();

        let _ = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        let final_state_events = process_provider_stream_delta(
            session_id,
            &mut state,
            Ok(ProviderStreamDelta::FinalConversationState(
                SessionConversationState::default(),
                None,
            )),
        );
        assert!(matches!(
            final_state_events.as_slice(),
            [ExecutionStreamEvent::ConversationCheckpoint {
                provider_request_count,
                ..
            }] if *provider_request_count == 1
        ));
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

//! Core controller state machine owned by `vulcan-core`.
//! `vulcan-core` 自有的控制器状态机。

use crate::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use crate::tool::{VulcanToolExecutor, VulcanToolPolicy};
use crate::types::{
    ControllerConfig, ControllerRunResult, ConversationMessage, ConversationPayload,
    ConversationRecord, ConversationSnapshot, CoreError, CoreEvent, CoreEventKind,
    ProviderTurnDelta, ProviderTurnResponse, ProviderUsageSnapshot, ToolInvocationRequest,
    ToolInvocationResult, TranscriptRecordKind, TranscriptRole,
};
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

impl VulcanCoreController {
    /// Creates a new controller with one configuration and one initial conversation snapshot.
    /// 使用一份配置与一份初始会话快照创建新的控制器。
    pub fn new(config: ControllerConfig, snapshot: ConversationSnapshot) -> Self {
        Self {
            config,
            local_id_counter: derive_initial_local_id_counter(&snapshot),
            snapshot,
            last_request_snapshot: None,
            events: Vec::new(),
            phase: ControllerPhase::Idle,
            last_successful_provider_usage: None,
        }
    }

    /// Returns the immutable controller configuration.
    /// 返回不可变的控制器配置。
    pub fn config(&self) -> &ControllerConfig {
        &self.config
    }

    /// Returns the immutable canonical snapshot currently owned by the controller.
    /// 返回控制器当前持有的不可变规范快照。
    pub fn snapshot(&self) -> &ConversationSnapshot {
        &self.snapshot
    }

    /// Replaces the canonical snapshot with a newer one and re-seeds the local identifier counter.
    /// 用一份更新后的规范快照替换当前内容，并重新播种本地标识计数器。
    pub fn replace_snapshot(&mut self, snapshot: ConversationSnapshot) {
        self.local_id_counter = derive_initial_local_id_counter(&snapshot);
        self.snapshot = snapshot;
        self.last_successful_provider_usage = None;
    }

    /// Returns the latest captured request snapshot if one exists.
    /// 返回最近一次捕获到的请求快照（如果存在）。
    pub fn last_request_snapshot(&self) -> Option<&Value> {
        self.last_request_snapshot.as_ref()
    }

    /// Returns the most recent successful provider usage observed during the current controller run.
    /// 返回当前控制器运行期间观测到的最近一次成功 Provider 用量。
    pub fn last_successful_provider_usage(&self) -> Option<&ProviderUsageSnapshot> {
        self.last_successful_provider_usage.as_ref()
    }

    /// Returns the current coarse-grained controller phase.
    /// 返回当前粗粒度控制器阶段。
    pub fn phase(&self) -> ControllerPhase {
        self.phase
    }

    /// Returns all controller events accumulated so far.
    /// 返回当前累计的全部控制器事件。
    pub fn events(&self) -> &[CoreEvent] {
        &self.events
    }

    /// Clears the accumulated controller events.
    /// 清空当前累计的控制器事件。
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Appends one controller event to the in-memory event buffer.
    /// 向内存事件缓冲区追加一条控制器事件。
    pub fn push_event(&mut self, event: CoreEvent) {
        self.events.push(event);
    }

    /// Records one provider-emitted text event for later GUI and audit projection.
    /// 记录一条 Provider 产出的文本事件，供后续 GUI 与审计投影使用。
    pub fn record_output(&mut self, body: impl Into<String>) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Output,
            title: String::from("模型输出"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Records one provider-emitted reasoning event for later GUI and audit projection.
    /// 记录一条 Provider 产出的思考事件，供后续 GUI 与审计投影使用。
    pub fn record_thinking(&mut self, body: impl Into<String>) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Thinking,
            title: String::from("模型思考"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Records one tool-start event with structured tool-call metadata.
    /// 记录一条带结构化工具调用元数据的工具开始事件。
    pub fn record_tool_started(&mut self, request: &ToolInvocationRequest) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::ToolStarted,
            title: format!("调用 {} 工具", request.tool_name),
            body: render_json_pretty(&request.arguments),
            metadata: Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            })),
        });
    }

    /// Records one tool-finished event with structured tool-result metadata.
    /// 记录一条带结构化工具结果元数据的工具完成事件。
    pub fn record_tool_finished(&mut self, result: &ToolInvocationResult) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::ToolFinished,
            title: format!("调用 {} 工具", result.tool_name),
            body: result.model_output.clone(),
            metadata: Some(build_tool_result_metadata(result)),
        });
    }

    /// Marks the controller as failed and captures the failure body as one error event.
    /// 将控制器标记为失败，并把失败正文作为一条错误事件捕获下来。
    pub fn mark_failed(&mut self, body: impl Into<String>) {
        self.phase = ControllerPhase::Failed;
        self.push_event(CoreEvent {
            kind: CoreEventKind::Error,
            title: String::from("控制器失败"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Captures the provider request built by the adapter and keeps a serialized snapshot for debugging.
    /// 捕获适配器构建出的 Provider 请求，并保留一份序列化快照用于调试。
    pub async fn build_request<A>(&mut self, adapter: &A) -> Result<A::Request, CoreError>
    where
        A: VulcanProviderAdapter,
    {
        self.phase = ControllerPhase::PreparingRequest;
        let request = adapter
            .build_request(&self.snapshot, &self.config)
            .await
            .map_err(|error| CoreError::ProviderFailure {
                provider_id: adapter.provider_id().to_string(),
                message: error.to_string(),
            })?;
        self.last_request_snapshot = serde_json::to_value(&request).ok();
        Ok(request)
    }

    /// Executes one host-owned tool request through the provided tool executor and applies the current tool policy.
    /// 通过给定的工具执行器执行一次宿主自有工具请求，并应用当前工具策略。
    pub async fn execute_tool<E, P>(
        &mut self,
        executor: &E,
        policy: &P,
        request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError>
    where
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        if !policy.should_execute(request, &self.snapshot) {
            let skipped_result = ToolInvocationResult {
                tool_call_id: request.tool_call_id.clone(),
                tool_name: request.tool_name.clone(),
                model_output: String::from("Tool execution was skipped by the host policy."),
                host_result: Some(json!({
                    "tool_call_id": request.tool_call_id,
                    "tool_name": request.tool_name,
                    "status": "skipped_by_policy",
                    "arguments": request.arguments,
                })),
                is_error: false,
            };
            self.record_tool_started(request);
            self.record_tool_finished(&skipped_result);
            return Ok(skipped_result);
        }

        self.record_tool_started(request);
        let result =
            executor
                .execute_tool(request)
                .await
                .map_err(|error| CoreError::ToolFailure {
                    tool_name: request.tool_name.clone(),
                    message: error.to_string(),
                })?;
        self.record_tool_finished(&result);
        Ok(result)
    }

    /// Executes one provider turn and updates the controller phase according to the outcome.
    /// 执行一次 Provider 回合，并根据结果更新控制器阶段。
    pub async fn execute_once<A>(&mut self, adapter: &A) -> Result<A::Response, CoreError>
    where
        A: VulcanProviderAdapter,
    {
        let request = self.build_request(adapter).await?;
        self.phase = ControllerPhase::WaitingForProvider;
        match adapter.execute_request(request).await {
            Ok(response) => {
                self.phase = ControllerPhase::Completed;
                self.push_event(CoreEvent {
                    kind: CoreEventKind::Final,
                    title: String::from("控制器完成"),
                    body: String::from("一次 Provider 回合已成功结束。"),
                    metadata: None,
                });
                Ok(response)
            }
            Err(error) => {
                self.phase = ControllerPhase::Failed;
                self.push_event(CoreEvent {
                    kind: CoreEventKind::Error,
                    title: String::from("控制器失败"),
                    body: error.to_string(),
                    metadata: Some(Value::String(adapter.provider_id().to_string())),
                });
                Err(error)
            }
        }
    }

    /// Runs one complete user-authored turn through the host-owned provider and tool loop until the assistant settles.
    /// 运行一次完整的用户输入回合，并由宿主自有 Provider 与工具循环持续推进直到助手稳定结束。
    pub async fn run_user_turn<A, E, P>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        let user_prompt = user_prompt.into();
        let trimmed_prompt = user_prompt.trim();
        if trimmed_prompt.is_empty() {
            let error = CoreError::InvalidState {
                message: String::from("控制器无法执行空白用户输入。"),
            };
            self.mark_failed(error.to_string());
            return Err(error);
        }

        if self.should_append_user_turn(trimmed_prompt) {
            self.append_user_turn(trimmed_prompt);
        }
        let mut provider_turns_executed = 0usize;
        let mut tool_calls_processed = 0usize;

        loop {
            if provider_turns_executed >= self.config.max_turns {
                let error = CoreError::InvalidState {
                    message: format!(
                        "控制器已达到最大 Provider 回合限制 `{}`，仍存在未收敛的工具循环。",
                        self.config.max_turns
                    ),
                };
                self.mark_failed(error.to_string());
                return Err(error);
            }

            let response = self.execute_provider_turn(adapter).await?;
            provider_turns_executed += 1;

            if !response.assistant_reasoning.trim().is_empty() {
                self.record_thinking(response.assistant_reasoning.clone());
            }
            if !response.assistant_text.trim().is_empty() {
                self.record_output(response.assistant_text.clone());
            }

            if let Some(usage) = response.usage.clone() {
                self.last_successful_provider_usage = Some(usage);
            }

            let assistant_message_id = self.append_assistant_turn(&response);
            self.refresh_continuation(&response);

            if response.tool_calls.is_empty() {
                self.phase = ControllerPhase::Completed;
                self.push_final_event(
                    provider_turns_executed,
                    tool_calls_processed,
                    Some(assistant_message_id.as_str()),
                );
                return Ok(ControllerRunResult {
                    provider_turns_executed,
                    tool_calls_processed,
                    final_assistant_message_id: Some(assistant_message_id),
                });
            }

            for tool_call in &response.tool_calls {
                let request = ToolInvocationRequest {
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                };
                let result = match self.execute_tool(executor, policy, &request).await {
                    Ok(result) => result,
                    Err(error) => {
                        self.mark_failed(error.to_string());
                        return Err(error);
                    }
                };
                tool_calls_processed += 1;
                self.append_tool_result_message(&result);
            }
        }
    }

    /// Runs one complete user-authored turn through one streaming-capable provider adapter while preserving streamed thinking and output deltas.
    /// 通过一个支持流式增量的 Provider 适配器运行一次完整用户回合，并保留流式思考与输出增量。
    pub async fn run_user_turn_streaming<A, E, P>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        let user_prompt = user_prompt.into();
        let trimmed_prompt = user_prompt.trim();
        if trimmed_prompt.is_empty() {
            let error = CoreError::InvalidState {
                message: String::from("控制器无法执行空白用户输入。"),
            };
            self.mark_failed(error.to_string());
            return Err(error);
        }

        if self.should_append_user_turn(trimmed_prompt) {
            self.append_user_turn(trimmed_prompt);
        }
        let mut provider_turns_executed = 0usize;
        let mut tool_calls_processed = 0usize;

        loop {
            if provider_turns_executed >= self.config.max_turns {
                let error = CoreError::InvalidState {
                    message: format!(
                        "控制器已达到最大 Provider 回合限制 `{}`，仍存在未收敛的工具循环。",
                        self.config.max_turns
                    ),
                };
                self.mark_failed(error.to_string());
                return Err(error);
            }

            let (response, deltas) = self.execute_provider_turn_streaming(adapter).await?;
            provider_turns_executed += 1;
            self.apply_provider_turn_deltas(&deltas);

            if let Some(usage) = response.usage.clone() {
                self.last_successful_provider_usage = Some(usage);
            }

            let assistant_message_id = self.append_assistant_turn(&response);
            self.refresh_continuation(&response);

            if response.tool_calls.is_empty() {
                self.phase = ControllerPhase::Completed;
                self.push_final_event(
                    provider_turns_executed,
                    tool_calls_processed,
                    Some(assistant_message_id.as_str()),
                );
                return Ok(ControllerRunResult {
                    provider_turns_executed,
                    tool_calls_processed,
                    final_assistant_message_id: Some(assistant_message_id),
                });
            }

            for tool_call in &response.tool_calls {
                let request = ToolInvocationRequest {
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                };
                let result = match self.execute_tool(executor, policy, &request).await {
                    Ok(result) => result,
                    Err(error) => {
                        self.mark_failed(error.to_string());
                        return Err(error);
                    }
                };
                tool_calls_processed += 1;
                self.append_tool_result_message(&result);
            }
        }
    }

    /// Executes one normalized provider turn without emitting an artificial final event.
    /// 执行一次规范化 Provider 回合，但不额外发出人为合成的完成事件。
    async fn execute_provider_turn<A>(
        &mut self,
        adapter: &A,
    ) -> Result<ProviderTurnResponse, CoreError>
    where
        A: VulcanProviderAdapter<Response = ProviderTurnResponse>,
    {
        let request = match self.build_request(adapter).await {
            Ok(request) => request,
            Err(error) => {
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        self.phase = ControllerPhase::WaitingForProvider;
        match adapter.execute_request(request).await {
            Ok(response) => {
                self.phase = ControllerPhase::Idle;
                Ok(response)
            }
            Err(error) => {
                self.mark_failed(error.to_string());
                Err(error)
            }
        }
    }

    /// Executes one normalized provider turn through the streaming adapter and returns both the final turn response and buffered deltas.
    /// 通过流式适配器执行一次规范化 Provider 回合，并返回最终回合响应与缓冲增量。
    async fn execute_provider_turn_streaming<A>(
        &mut self,
        adapter: &A,
    ) -> Result<(ProviderTurnResponse, Vec<ProviderTurnDelta>), CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
    {
        let request = match self.build_request(adapter).await {
            Ok(request) => request,
            Err(error) => {
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        self.phase = ControllerPhase::WaitingForProvider;
        let mut sink = BufferedProviderDeltaSink::default();
        match adapter.execute_request_streaming(request, &mut sink).await {
            Ok(response) => {
                self.phase = ControllerPhase::Idle;
                Ok((response, sink.deltas))
            }
            Err(error) => {
                self.mark_failed(error.to_string());
                Err(error)
            }
        }
    }

    /// Applies buffered provider-stream deltas to the controller-owned event trace in the original emission order.
    /// 按原始发出顺序将缓冲的 Provider 流式增量应用到控制器自有事件轨迹。
    fn apply_provider_turn_deltas(&mut self, deltas: &[ProviderTurnDelta]) {
        for delta in deltas {
            match delta {
                ProviderTurnDelta::Thinking { delta } => self.record_thinking(delta.clone()),
                ProviderTurnDelta::Output { delta } => self.record_output(delta.clone()),
            }
        }
    }

    /// Appends one user-authored prompt into the canonical message window and durable record stream.
    /// 将一条用户输入追加到规范消息窗口与持久化记录流中。
    fn append_user_turn(&mut self, prompt: &str) -> String {
        let message_id = self.next_local_id("user-message");
        let record_id = self.next_local_id("user-record");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::UserText {
                text: prompt.to_string(),
                attachments: Vec::new(),
            },
        });
        self.snapshot.records.push(ConversationRecord {
            id: record_id,
            role: TranscriptRole::User,
            record_kind: TranscriptRecordKind::Visible,
            visible: true,
            title: String::from("用户输入"),
            body: prompt.to_string(),
            metadata: None,
            created_at: timestamp,
        });
        message_id
    }

    /// Returns whether the controller still needs to append one new user turn instead of reusing the host-preseeded latest user prompt.
    /// 返回控制器当前是否仍需追加一条新的用户回合，而不是复用宿主已预播种的最新用户提示词。
    fn should_append_user_turn(&self, prompt: &str) -> bool {
        !matches!(
            self.snapshot.messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, .. },
                ..
            }) if text.trim() == prompt
        )
    }

    /// Appends one provider assistant turn into the canonical message window and the durable record stream.
    /// 将一条 Provider 助手回合追加到规范消息窗口与持久化记录流中。
    fn append_assistant_turn(&mut self, response: &ProviderTurnResponse) -> String {
        let message_id = self.next_local_id("assistant-message");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::Assistant {
                text: response.assistant_text.clone(),
                reasoning: response.assistant_reasoning.clone(),
                provider_message_id: response.provider_message_id.clone(),
                provider_message_state: response.provider_message_state.clone(),
                usage: response.usage.clone(),
                tool_calls: response.tool_calls.clone(),
            },
        });

        if !response.assistant_text.trim().is_empty() {
            let record_id = self.next_local_id("assistant-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Assistant,
                record_kind: TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("模型输出"),
                body: response.assistant_text.clone(),
                metadata: response.usage.as_ref().map(build_provider_usage_metadata),
                created_at: timestamp.clone(),
            });
        }

        if !response.assistant_reasoning.trim().is_empty() {
            let record_id = self.next_local_id("thinking-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Thinking,
                record_kind: TranscriptRecordKind::Hidden,
                visible: false,
                title: String::from("模型思考"),
                body: response.assistant_reasoning.clone(),
                metadata: None,
                created_at: timestamp.clone(),
            });
        }

        for tool_call in &response.tool_calls {
            let record_id = self.next_local_id("tool-call-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Tool,
                record_kind: TranscriptRecordKind::Visible,
                visible: true,
                title: format!("调用 {} 工具", tool_call.tool_name),
                body: render_json_pretty(&tool_call.arguments),
                metadata: Some(json!({
                    "tool_call_id": resolved_tool_call_id(tool_call),
                    "tool_name": tool_call.tool_name,
                    "arguments": tool_call.arguments,
                })),
                created_at: timestamp.clone(),
            });
        }

        message_id
    }

    /// Appends one tool result back into the canonical message window and durable record stream.
    /// 将一条工具结果回写到规范消息窗口与持久化记录流中。
    fn append_tool_result_message(&mut self, result: &ToolInvocationResult) -> String {
        let message_id = self.next_local_id("tool-result-message");
        let tool_result_id = self.next_local_id("tool-result");
        let record_id = self.next_local_id("tool-result-record");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::ToolResult {
                tool_result_id,
                tool_call_id: Some(result.tool_call_id.clone()),
                text: result.model_output.clone(),
            },
        });
        self.snapshot.records.push(ConversationRecord {
            id: record_id,
            role: TranscriptRole::Tool,
            record_kind: TranscriptRecordKind::Visible,
            visible: true,
            title: format!("调用 {} 工具", result.tool_name),
            body: result.model_output.clone(),
            metadata: Some(build_tool_result_metadata(result)),
            created_at: timestamp,
        });
        message_id
    }

    /// Refreshes the provider continuation snapshot from the latest assistant response.
    /// 根据最近一次助手响应刷新 Provider continuation 快照。
    fn refresh_continuation(&mut self, response: &ProviderTurnResponse) {
        let mut next = response.continuation.clone().unwrap_or_default();
        next.provider_id
            .get_or_insert_with(|| self.config.provider_id.clone());
        next.model_id
            .get_or_insert_with(|| self.config.model_id.clone());
        next.last_provider_message_id
            .get_or_insert_with(|| response.provider_message_id.clone().unwrap_or_default());
        if next
            .last_provider_message_id
            .as_ref()
            .is_some_and(|message_id| !message_id.is_empty())
        {
            next.updated_at.get_or_insert_with(current_unix_millis_text);
            self.snapshot.continuation = next;
        }
    }

    /// Appends one final completion event that summarizes the host-owned loop outcome.
    /// 追加一条概括宿主自有循环结果的最终完成事件。
    fn push_final_event(
        &mut self,
        provider_turns_executed: usize,
        tool_calls_processed: usize,
        final_assistant_message_id: Option<&str>,
    ) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Final,
            title: String::from("控制器完成"),
            body: format!(
                "宿主控制器已完成一次完整回合，Provider 回合数为 {}，工具调用数为 {}。",
                provider_turns_executed, tool_calls_processed
            ),
            metadata: Some(json!({
                "provider_turns_executed": provider_turns_executed,
                "tool_calls_processed": tool_calls_processed,
                "final_assistant_message_id": final_assistant_message_id,
            })),
        });
    }

    /// Generates one new local identifier for synthesized protocol entities.
    /// 为合成协议实体生成一个新的本地标识。
    fn next_local_id(&mut self, prefix: &str) -> String {
        self.local_id_counter += 1;
        format!(
            "{prefix}-{}-{}",
            current_unix_millis_text(),
            self.local_id_counter
        )
    }
}

/// Derives the starting local identifier counter from the current snapshot sizes.
/// 根据当前快照大小推导本地标识计数器的起始值。
fn derive_initial_local_id_counter(snapshot: &ConversationSnapshot) -> u64 {
    (snapshot.messages.len() + snapshot.records.len()) as u64
}

/// Builds one stable JSON metadata object for a tool result record or event.
/// 为工具结果记录或事件构建一份稳定 JSON 元数据对象。
fn build_tool_result_metadata(result: &ToolInvocationResult) -> Value {
    let mut base = json!({
        "tool_call_id": result.tool_call_id,
        "tool_name": result.tool_name,
        "is_error": result.is_error,
    });
    if let Some(host_result) = &result.host_result {
        if let (Some(base_object), Value::Object(host_object)) = (base.as_object_mut(), host_result)
        {
            merge_metadata_objects(base_object, host_object);
        } else {
            base["host_result"] = host_result.clone();
        }
    }
    base
}

/// Builds one stable JSON metadata object for a provider usage snapshot.
/// 为一份 Provider 用量快照构建稳定 JSON 元数据对象。
fn build_provider_usage_metadata(usage: &crate::types::ProviderUsageSnapshot) -> Value {
    json!({
        "provider_usage": usage,
    })
}

/// Merges one host metadata object into the base metadata object without dropping existing controller fields.
/// 将一份宿主元数据对象合并到基础元数据对象中，同时不丢失控制器原有字段。
fn merge_metadata_objects(base: &mut JsonMap<String, Value>, host: &JsonMap<String, Value>) {
    for (key, value) in host {
        base.insert(key.clone(), value.clone());
    }
}

/// Resolves one stable tool-call identifier from the provider-emitted tool-call payload.
/// 从 Provider 产出的工具调用载荷中解析一份稳定工具调用标识。
fn resolved_tool_call_id(tool_call: &crate::types::ConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| tool_call.id.clone())
}

/// Renders a JSON value into a pretty string and falls back to the compact representation on serialization failure.
/// 将 JSON 值渲染为美观字符串，并在序列化失败时回退到紧凑表示。
fn render_json_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Returns the current Unix-millis timestamp formatted as text.
/// 返回以文本形式表示的当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    //! Controller-loop tests for the product-owned host execution flow.
    //! 面向产品自有宿主执行流的控制器循环测试。

    use super::*;
    use crate::provider::{VulcanProviderDeltaSink, VulcanStreamingProviderAdapter};
    use crate::tool::{AllowAllToolPolicy, ExactToolSetPolicy};
    use crate::types::{
        ConversationPayload, ConversationToolCall, ProviderContinuation, ProviderPromptLayers,
        ProviderRequestSnapshot, ProviderRequestToolSchema, ProviderTurnDelta,
    };
    use async_trait::async_trait;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};

    /// Captures pre-planned provider turn responses and records the built request snapshots.
    /// 持有预先规划的 Provider 回合响应，并记录构建出的请求快照。
    #[derive(Debug, Clone)]
    struct MockProviderAdapter {
        /// Stable provider identifier exposed by the mock adapter.
        /// 由模拟适配器暴露的稳定 Provider 标识。
        provider_id: String,
        /// Stable model identifier exposed by the mock adapter.
        /// 由模拟适配器暴露的稳定模型标识。
        model_id: String,
        /// Planned normalized provider responses returned sequentially.
        /// 按顺序返回的规范化 Provider 响应计划队列。
        planned_responses: Arc<Mutex<VecDeque<ProviderTurnResponse>>>,
        /// Built request snapshots captured before execution.
        /// 执行前捕获到的请求快照集合。
        built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
    }

    impl MockProviderAdapter {
        /// Creates one mock provider adapter from the planned turn-response sequence.
        /// 根据计划中的回合响应序列创建一个模拟 Provider 适配器。
        fn new(planned_responses: Vec<ProviderTurnResponse>) -> Self {
            Self {
                provider_id: String::from("mock-openai"),
                model_id: String::from("mock-model"),
                planned_responses: Arc::new(Mutex::new(VecDeque::from(planned_responses))),
                built_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Returns the built request snapshots captured by the adapter so far.
        /// 返回适配器截至当前捕获到的请求快照集合。
        fn built_requests(&self) -> Vec<ProviderRequestSnapshot> {
            self.built_requests
                .lock()
                .expect("lock built requests")
                .clone()
        }
    }

    /// Captures pre-planned streaming provider responses and emits buffered turn deltas before returning the final normalized response.
    /// 持有预先规划的流式 Provider 响应，并在返回最终规范化响应前发出缓冲回合增量。
    #[derive(Debug, Clone)]
    struct MockStreamingProviderAdapter {
        /// Stable provider identifier exposed by the mock streaming adapter.
        /// 由模拟流式适配器暴露的稳定 Provider 标识。
        provider_id: String,
        /// Stable model identifier exposed by the mock streaming adapter.
        /// 由模拟流式适配器暴露的稳定模型标识。
        model_id: String,
        /// Planned normalized provider responses paired with the deltas that should be emitted before each final response.
        /// 与每次最终响应之前应发出的增量配对保存的规范化 Provider 响应计划队列。
        planned_turns: Arc<Mutex<VecDeque<(Vec<ProviderTurnDelta>, ProviderTurnResponse)>>>,
        /// Built request snapshots captured before execution.
        /// 执行前捕获到的请求快照集合。
        built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
    }

    impl MockStreamingProviderAdapter {
        /// Creates one mock streaming provider adapter from the planned turn sequence.
        /// 根据计划中的回合序列创建一个模拟流式 Provider 适配器。
        fn new(planned_turns: Vec<(Vec<ProviderTurnDelta>, ProviderTurnResponse)>) -> Self {
            Self {
                provider_id: String::from("mock-openai-streaming"),
                model_id: String::from("mock-model"),
                planned_turns: Arc::new(Mutex::new(VecDeque::from(planned_turns))),
                built_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl VulcanProviderAdapter for MockProviderAdapter {
        type Request = ProviderRequestSnapshot;
        type Response = ProviderTurnResponse;

        /// Returns the stable provider identifier owned by the mock adapter.
        /// 返回由模拟适配器持有的稳定 Provider 标识。
        fn provider_id(&self) -> &str {
            &self.provider_id
        }

        /// Returns the stable model identifier owned by the mock adapter.
        /// 返回由模拟适配器持有的稳定模型标识。
        fn model_id(&self) -> &str {
            &self.model_id
        }

        /// Builds one request snapshot directly from the controller snapshot for later assertions.
        /// 直接根据控制器快照构建一份请求快照，供后续断言使用。
        async fn build_request(
            &self,
            snapshot: &ConversationSnapshot,
            config: &ControllerConfig,
        ) -> Result<Self::Request, CoreError> {
            let request = ProviderRequestSnapshot {
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                system_prompt: config.system_prompt.clone(),
                prompt_layers: config.prompt_layers.clone(),
                prompt: String::from("mock-user-prompt"),
                history: snapshot.messages.clone(),
                temperature: config.temperature,
                max_turns: config.max_turns,
                tool_schemas: vec![ProviderRequestToolSchema {
                    id: String::from("mock-tool"),
                    name: String::from("mock-tool"),
                    description: String::from("mock tool"),
                    source_label: String::from("test"),
                    parameters_schema: json!({"type":"object"}),
                }],
                additional_params: config.additional_params.clone(),
                request_headers: None,
                continuation: snapshot.continuation.clone(),
            };
            self.built_requests
                .lock()
                .expect("lock built requests")
                .push(request.clone());
            Ok(request)
        }

        /// Returns the next pre-planned turn response or fails when the queue has been exhausted.
        /// 返回下一个预先规划的回合响应，或在队列耗尽时失败。
        async fn execute_request(
            &self,
            _request: Self::Request,
        ) -> Result<Self::Response, CoreError> {
            self.planned_responses
                .lock()
                .expect("lock planned responses")
                .pop_front()
                .ok_or_else(|| CoreError::InvalidState {
                    message: String::from("模拟 Provider 响应队列已耗尽。"),
                })
        }
    }

    #[async_trait]
    impl VulcanProviderAdapter for MockStreamingProviderAdapter {
        type Request = ProviderRequestSnapshot;
        type Response = ProviderTurnResponse;

        /// Returns the stable provider identifier owned by the mock streaming adapter.
        /// 返回由模拟流式适配器持有的稳定 Provider 标识。
        fn provider_id(&self) -> &str {
            &self.provider_id
        }

        /// Returns the stable model identifier owned by the mock streaming adapter.
        /// 返回由模拟流式适配器持有的稳定模型标识。
        fn model_id(&self) -> &str {
            &self.model_id
        }

        /// Builds one request snapshot directly from the controller snapshot for later assertions.
        /// 直接根据控制器快照构建一份请求快照，供后续断言使用。
        async fn build_request(
            &self,
            snapshot: &ConversationSnapshot,
            config: &ControllerConfig,
        ) -> Result<Self::Request, CoreError> {
            let request = ProviderRequestSnapshot {
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                system_prompt: config.system_prompt.clone(),
                prompt_layers: config.prompt_layers.clone(),
                prompt: String::from("mock-streaming-user-prompt"),
                history: snapshot.messages.clone(),
                temperature: config.temperature,
                max_turns: config.max_turns,
                tool_schemas: vec![ProviderRequestToolSchema {
                    id: String::from("mock-tool"),
                    name: String::from("mock-tool"),
                    description: String::from("mock tool"),
                    source_label: String::from("test"),
                    parameters_schema: json!({"type":"object"}),
                }],
                additional_params: config.additional_params.clone(),
                request_headers: None,
                continuation: snapshot.continuation.clone(),
            };
            self.built_requests
                .lock()
                .expect("lock built requests")
                .push(request.clone());
            Ok(request)
        }

        /// Mock streaming adapters are expected to use `execute_request_streaming` instead of the one-shot request path.
        /// 模拟流式适配器应当使用 `execute_request_streaming`，而不是一次性请求路径。
        async fn execute_request(
            &self,
            _request: Self::Request,
        ) -> Result<Self::Response, CoreError> {
            Err(CoreError::InvalidState {
                message: String::from("模拟流式适配器不支持一次性 execute_request 调用。"),
            })
        }
    }

    #[async_trait]
    impl VulcanStreamingProviderAdapter for MockStreamingProviderAdapter {
        /// Emits the pre-planned delta sequence into the provided sink and then returns the next final turn response.
        /// 将预先规划的增量序列写入给定 sink，并在随后返回下一条最终回合响应。
        async fn execute_request_streaming<S>(
            &self,
            _request: Self::Request,
            sink: &mut S,
        ) -> Result<Self::Response, CoreError>
        where
            S: VulcanProviderDeltaSink,
        {
            let (deltas, response) = self
                .planned_turns
                .lock()
                .expect("lock planned streaming turns")
                .pop_front()
                .ok_or_else(|| CoreError::InvalidState {
                    message: String::from("模拟流式 Provider 响应队列已耗尽。"),
                })?;
            for delta in deltas {
                sink.push_delta(delta);
            }
            Ok(response)
        }
    }

    /// Executes pre-planned tool outcomes and records each incoming tool request.
    /// 执行预先规划的工具结果，并记录每一次收到的工具请求。
    #[derive(Debug, Clone, Default)]
    struct MockToolExecutor {
        /// Pre-planned tool outcomes keyed by tool name.
        /// 按工具名称索引的预先规划工具结果。
        outcomes_by_tool: BTreeMap<String, (String, Option<Value>, bool)>,
        /// Recorded tool invocation requests received by the executor.
        /// 被执行器收到并记录下来的工具调用请求集合。
        executed_requests: Arc<Mutex<Vec<ToolInvocationRequest>>>,
    }

    impl MockToolExecutor {
        /// Creates one empty mock tool executor.
        /// 创建一个空白的模拟工具执行器。
        fn new() -> Self {
            Self::default()
        }

        /// Registers one pre-planned tool outcome for a specific tool name.
        /// 为指定工具名称注册一条预先规划的工具结果。
        fn with_outcome(
            mut self,
            tool_name: &str,
            model_output: &str,
            host_result: Option<Value>,
            is_error: bool,
        ) -> Self {
            self.outcomes_by_tool.insert(
                tool_name.to_string(),
                (model_output.to_string(), host_result, is_error),
            );
            self
        }

        /// Returns the recorded tool invocation requests captured by the executor so far.
        /// 返回执行器截至当前捕获到的工具调用请求集合。
        fn executed_requests(&self) -> Vec<ToolInvocationRequest> {
            self.executed_requests
                .lock()
                .expect("lock executed requests")
                .clone()
        }
    }

    #[async_trait]
    impl VulcanToolExecutor for MockToolExecutor {
        /// Executes one tool request by looking up the pre-planned result for the requested tool.
        /// 通过查找被请求工具的预设结果来执行一次工具请求。
        async fn execute_tool(
            &self,
            request: &ToolInvocationRequest,
        ) -> Result<ToolInvocationResult, CoreError> {
            self.executed_requests
                .lock()
                .expect("lock executed requests")
                .push(request.clone());
            let (model_output, host_result, is_error) = self
                .outcomes_by_tool
                .get(&request.tool_name)
                .cloned()
                .ok_or_else(|| CoreError::InvalidState {
                    message: format!("未找到工具 `{}` 的模拟结果。", request.tool_name),
                })?;
            Ok(ToolInvocationResult {
                tool_call_id: request.tool_call_id.clone(),
                tool_name: request.tool_name.clone(),
                model_output,
                host_result,
                is_error,
            })
        }
    }

    /// Builds one baseline controller configuration for loop tests.
    /// 为循环测试构建一份基线控制器配置。
    fn build_test_config(max_turns: usize) -> ControllerConfig {
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: ProviderPromptLayers::from_system_prompt("system"),
            temperature: Some(0.1),
            max_turns,
            additional_params: Some(json!({"reasoning_effort":"medium"})),
        }
    }

    /// Executes a full host-owned multi-turn loop and persists assistant, tool, and continuation state.
    /// 执行一条完整的宿主自有多轮循环，并持久化助手、工具与 continuation 状态。
    #[test]
    fn run_user_turn_executes_multi_turn_loop_and_refreshes_continuation() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let adapter = MockProviderAdapter::new(vec![
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::from("先读取文件，再整理结论。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({"path":"src/main.rs"}),
                    }],
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-1")),
                        updated_at: Some(String::from("1000")),
                    }),
                    usage: None,
                },
                ProviderTurnResponse {
                    assistant_text: String::from("文件检查完成，已经拿到关键上下文。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-2")),
                        updated_at: Some(String::from("2000")),
                    }),
                    usage: None,
                },
            ]);
            let executor = MockToolExecutor::new().with_outcome(
                "read_file",
                "已读取 src/main.rs 的摘要内容。",
                Some(json!({"kind":"file_read","payload":{"path":"src/main.rs"}})),
                false,
            );
            let mut controller =
                VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default());

            let result = controller
                .run_user_turn(&adapter, &executor, &AllowAllToolPolicy, "请检查主入口文件")
                .await
                .expect("run user turn");

            assert_eq!(result.provider_turns_executed, 2);
            assert_eq!(result.tool_calls_processed, 1);
            assert_eq!(controller.phase(), ControllerPhase::Completed);
            assert_eq!(controller.snapshot().messages.len(), 4);
            assert_eq!(
                controller
                    .snapshot()
                    .continuation
                    .last_provider_message_id
                    .as_deref(),
                Some("resp-2")
            );
            assert_eq!(executor.executed_requests().len(), 1);

            let built_requests = adapter.built_requests();
            assert_eq!(built_requests.len(), 2);
            assert_eq!(built_requests[0].history.len(), 1);
            assert_eq!(built_requests[1].history.len(), 3);
            assert_eq!(
                built_requests[1]
                    .continuation
                    .last_provider_message_id
                    .as_deref(),
                Some("resp-1")
            );

            assert!(matches!(
                &controller.snapshot().messages[1].payload,
                ConversationPayload::Assistant { tool_calls, .. } if tool_calls.len() == 1
            ));
            assert!(matches!(
                &controller.snapshot().messages[2].payload,
                ConversationPayload::ToolResult { text, .. } if text == "已读取 src/main.rs 的摘要内容。"
            ));
            assert!(controller.snapshot().records.iter().any(|record| {
                record.title == "调用 read_file 工具"
                    && record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("kind"))
                        .and_then(Value::as_str)
                        == Some("file_read")
            }));
            assert!(controller.events().iter().any(|event| {
                matches!(event.kind, CoreEventKind::Final)
                    && event
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("provider_turns_executed"))
                        .and_then(Value::as_u64)
                        == Some(2)
            }));
        });
    }

    /// Executes a full host-owned streaming loop and preserves provider deltas in the controller event trace before finalizing the assistant turn.
    /// 执行一条完整的宿主自有流式循环，并在完成最终助手回合前把 Provider 增量保留到控制器事件轨迹中。
    #[test]
    fn run_user_turn_streaming_records_provider_deltas_before_finalizing() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let adapter = MockStreamingProviderAdapter::new(vec![(
                vec![
                    ProviderTurnDelta::Thinking {
                        delta: String::from("先读取文件。"),
                    },
                    ProviderTurnDelta::Output {
                        delta: String::from("已完成分析。"),
                    },
                ],
                ProviderTurnResponse {
                    assistant_text: String::from("已完成分析。"),
                    assistant_reasoning: String::from("先读取文件。"),
                    provider_message_id: Some(String::from("resp-stream-1")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai-streaming")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-stream-1")),
                        updated_at: Some(String::from("2001")),
                    }),
                    usage: None,
                },
            )]);
            let executor = MockToolExecutor::new();
            let mut controller =
                VulcanCoreController::new(build_test_config(2), ConversationSnapshot::default());

            let result = controller
                .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "请先分析文件")
                .await
                .expect("run streaming user turn");

            assert_eq!(result.provider_turns_executed, 1);
            assert_eq!(result.tool_calls_processed, 0);
            assert_eq!(controller.phase(), ControllerPhase::Completed);

            let thinking_events = controller
                .events()
                .iter()
                .filter(|event| matches!(event.kind, CoreEventKind::Thinking))
                .collect::<Vec<_>>();
            let output_events = controller
                .events()
                .iter()
                .filter(|event| matches!(event.kind, CoreEventKind::Output))
                .collect::<Vec<_>>();

            assert_eq!(thinking_events.len(), 1);
            assert_eq!(thinking_events[0].body, "先读取文件。");
            assert_eq!(output_events.len(), 1);
            assert_eq!(output_events[0].body, "已完成分析。");
            assert!(controller.events().iter().any(|event| {
                matches!(event.kind, CoreEventKind::Final)
                    && event.body.contains("Provider 回合数为 1")
            }));
            assert!(matches!(
                &controller.snapshot().messages[1].payload,
                ConversationPayload::Assistant {
                    text,
                    reasoning,
                    provider_message_id,
                    provider_message_state: _,
                    tool_calls,
                    usage: _,
                } if text == "已完成分析。"
                    && reasoning == "先读取文件。"
                    && provider_message_id.as_deref() == Some("resp-stream-1")
                    && tool_calls.is_empty()
            ));
        });
    }

    /// Reuses the host-preseeded latest user prompt during streaming execution so the canonical snapshot does not duplicate one turn boundary.
    /// 在流式执行期间复用宿主预播种的最新用户提示词，从而避免规范快照重复记录同一轮次边界。
    #[test]
    fn run_user_turn_streaming_reuses_host_preseeded_user_prompt() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let adapter = MockStreamingProviderAdapter::new(vec![(
                vec![ProviderTurnDelta::Output {
                    delta: String::from("已完成分析。"),
                }],
                ProviderTurnResponse {
                    assistant_text: String::from("已完成分析。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-stream-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            )]);
            let executor = MockToolExecutor::new();
            let mut controller = VulcanCoreController::new(
                build_test_config(2),
                ConversationSnapshot {
                    messages: vec![ConversationMessage {
                        id: String::from("host-user-1"),
                        payload: ConversationPayload::UserText {
                            text: String::from("请先分析文件"),
                            attachments: Vec::new(),
                        },
                    }],
                    records: vec![ConversationRecord {
                        id: String::from("host-user-record-1"),
                        role: TranscriptRole::User,
                        record_kind: TranscriptRecordKind::Visible,
                        visible: true,
                        title: String::from("用户输入"),
                        body: String::from("请先分析文件"),
                        metadata: None,
                        created_at: String::from("1000"),
                    }],
                    continuation: ProviderContinuation::default(),
                },
            );

            let result = controller
                .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "请先分析文件")
                .await
                .expect("run streaming user turn");

            assert_eq!(result.provider_turns_executed, 1);
            assert_eq!(controller.snapshot().messages.len(), 2);
            assert_eq!(
                controller
                    .snapshot()
                    .messages
                    .iter()
                    .filter(|message| matches!(
                        message.payload,
                        ConversationPayload::UserText { .. }
                    ))
                    .count(),
                1
            );
        });
    }

    /// Generates a synthetic skipped tool result when the host policy refuses execution and continues the loop.
    /// 当宿主策略拒绝执行时生成一条合成跳过工具结果，并继续推进后续循环。
    #[test]
    fn run_user_turn_synthesizes_skipped_tool_result_when_policy_blocks_tool() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let adapter = MockProviderAdapter::new(vec![
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-skip-1")),
                    provider_message_state: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-skip-1"),
                        tool_call_id: Some(String::from("call-skip-1")),
                        tool_name: String::from("blocked_tool"),
                        arguments: json!({"id":"42"}),
                    }],
                    continuation: None,
                    usage: None,
                },
                ProviderTurnResponse {
                    assistant_text: String::from("已收到宿主返回的跳过结果。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-skip-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ]);
            let executor = MockToolExecutor::new();
            let policy = ExactToolSetPolicy::new(["allowed_tool"]);
            let mut controller =
                VulcanCoreController::new(build_test_config(3), ConversationSnapshot::default());

            let result = controller
                .run_user_turn(&adapter, &executor, &policy, "请调用被限制的工具")
                .await
                .expect("run blocked tool turn");

            assert_eq!(result.provider_turns_executed, 2);
            assert_eq!(result.tool_calls_processed, 1);
            assert!(executor.executed_requests().is_empty());
            assert!(matches!(
                &controller.snapshot().messages[2].payload,
                ConversationPayload::ToolResult { text, .. }
                    if text == "Tool execution was skipped by the host policy."
            ));
            assert!(controller.snapshot().records.iter().any(|record| {
                record.title == "调用 blocked_tool 工具"
                    && record.body == "Tool execution was skipped by the host policy."
            }));
        });
    }

    /// Stops the host-owned loop with a controller error once the configured provider-turn budget has been exhausted.
    /// 一旦达到已配置的 Provider 回合预算上限，就以控制器错误终止宿主自有循环。
    #[test]
    fn run_user_turn_fails_when_provider_turn_budget_is_exhausted() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let adapter = MockProviderAdapter::new(vec![ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-limit-1")),
                provider_message_state: None,
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-item-limit-1"),
                    tool_call_id: Some(String::from("call-limit-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({"path":"src/lib.rs"}),
                }],
                continuation: None,
                usage: None,
            }]);
            let executor = MockToolExecutor::new().with_outcome(
                "read_file",
                "已读取 src/lib.rs。",
                None,
                false,
            );
            let mut controller =
                VulcanCoreController::new(build_test_config(1), ConversationSnapshot::default());

            let error = controller
                .run_user_turn(&adapter, &executor, &AllowAllToolPolicy, "请继续检查")
                .await
                .expect_err("expect provider turn budget failure");

            assert!(matches!(error, CoreError::InvalidState { .. }));
            assert_eq!(controller.phase(), ControllerPhase::Failed);
            assert!(controller.events().iter().any(|event| {
                matches!(event.kind, CoreEventKind::Error)
                    && event.body.contains("最大 Provider 回合限制")
            }));
            assert_eq!(controller.snapshot().messages.len(), 3);
        });
    }

    /// Verifies the controller falls back to the local tool-call identifier when the provider-visible id is empty.
    /// 验证当 Provider 可见 id 为空时，控制器会回退到本地工具调用标识。
    #[test]
    fn resolved_tool_call_id_falls_back_when_provider_visible_id_is_empty() {
        let tool_call = crate::types::ConversationToolCall {
            id: String::from("local-call-1"),
            tool_call_id: Some(String::from("   ")),
            tool_name: String::from("read_file"),
            arguments: json!({ "path": "src/main.rs" }),
        };

        assert_eq!(resolved_tool_call_id(&tool_call), "local-call-1");
    }
}

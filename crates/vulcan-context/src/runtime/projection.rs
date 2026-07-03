//! Projection between canonical conversation state and runtime event logs.
//! 规范会话状态与运行时事件日志之间的投影。

use super::events::*;
use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall, normalize_replayed_protocol_messages,
};
use std::collections::BTreeMap;
use vulcan_core::types::ConversationPayload;

/// Converts one canonical conversation state into runtime log lines.
/// 将一个规范会话状态转换为运行时日志行。
pub fn runtime_log_lines_from_conversation_state(
    state: &SessionConversationState,
    created_at: &str,
    starting_seq: u64,
) -> Vec<RuntimeLogEventLine> {
    let mut builder = RuntimeLogProjectionBuilder::new(created_at, starting_seq);
    let mut open_batches = BTreeMap::<String, RuntimeOpenToolBatch>::new();
    for message in &state.messages {
        builder.push_message(message, &mut open_batches);
    }
    builder.close_remaining_batches(open_batches);
    builder.lines
}

/// Rebuilds a canonical conversation state from runtime log lines.
/// 从运行时日志行重建规范会话状态。
pub fn conversation_state_from_runtime_log_lines(
    lines: &[RuntimeLogEventLine],
) -> SessionConversationState {
    let mut projection = RuntimeConversationProjection::default();
    for line in lines {
        projection.apply_event(&line.event);
    }
    projection.finish()
}

/// Internal builder that projects conversation messages into runtime events.
/// 将会话消息投影为运行时事件的内部构建器。
struct RuntimeLogProjectionBuilder<'a> {
    /// Timestamp attached to generated runtime lines.
    /// 附加到生成运行时行上的时间戳。
    created_at: &'a str,
    /// Next sequence number assigned to generated runtime lines.
    /// 分配给生成运行时行的下一个序号。
    next_seq: u64,
    /// Generated runtime event lines.
    /// 已生成的运行时事件行。
    lines: Vec<RuntimeLogEventLine>,
}

impl<'a> RuntimeLogProjectionBuilder<'a> {
    /// Builds one projection builder.
    /// 构建一个投影构建器。
    fn new(created_at: &'a str, starting_seq: u64) -> Self {
        Self {
            created_at,
            next_seq: starting_seq,
            lines: Vec::new(),
        }
    }

    /// Appends a runtime event and advances the local sequence.
    /// 追加一条运行时事件并推进本地序号。
    fn append(&mut self, event: RuntimeLogEvent) {
        self.lines.push(RuntimeLogEventLine::new(
            self.next_seq,
            self.created_at,
            event,
        ));
        self.next_seq = self.next_seq.saturating_add(1);
    }

    /// Projects one canonical message into runtime events.
    /// 将一条规范消息投影为运行时事件。
    fn push_message(
        &mut self,
        message: &SessionConversationMessage,
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) {
        match &message.payload {
            SessionConversationPayload::UserText { text, attachments } => {
                self.append(RuntimeLogEvent::UserMessage(RuntimeUserMessageEvent {
                    turn_id: Some(message.id.clone()),
                    message_id: message.id.clone(),
                    text: text.clone(),
                    attachments: attachments.clone(),
                }));
            }
            SessionConversationPayload::Assistant {
                text,
                reasoning,
                provider_message_id,
                provider_message_state,
                usage,
                tool_calls,
            } => {
                self.append(RuntimeLogEvent::AssistantMessage(
                    RuntimeAssistantMessageEvent {
                        turn_id: Some(message.id.clone()),
                        message_id: message.id.clone(),
                        text: text.clone(),
                        reasoning: reasoning.clone(),
                        provider_id: None,
                        model_id: None,
                        provider_message_id: provider_message_id.clone(),
                        provider_message_state: provider_message_state.clone(),
                        usage: usage.clone(),
                    },
                ));
                if !tool_calls.is_empty() {
                    self.open_tool_batch(message, tool_calls, open_batches);
                }
            }
            SessionConversationPayload::ToolResult {
                tool_result_id,
                tool_call_id,
                text,
            } => {
                let runtime_tool_call_id = tool_call_id.clone();
                let tool_batch_id = runtime_tool_call_id
                    .as_deref()
                    .and_then(|id| open_batches.get(id))
                    .map(|batch| batch.tool_batch_id.clone());
                let status = if runtime_tool_call_id.is_some() && tool_batch_id.is_some() {
                    RuntimeToolResultStatus::Completed
                } else {
                    RuntimeToolResultStatus::Orphan
                };
                if let Some(runtime_tool_call_id) = runtime_tool_call_id.as_deref()
                    && let Some(batch) = open_batches.get_mut(runtime_tool_call_id)
                {
                    batch.tool_result_count = batch.tool_result_count.saturating_add(1);
                }
                self.append(RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                    turn_id: Some(message.id.clone()),
                    tool_batch_id,
                    runtime_tool_call_id,
                    provider_tool_call_id: tool_call_id.clone(),
                    tool_result_id: tool_result_id.clone(),
                    text: text.clone(),
                    status,
                    error: None,
                }));
            }
            SessionConversationPayload::AgentMessage {
                message_type,
                task_name,
                sender,
                payload,
                ..
            } => {
                let text = ConversationPayload::render_agent_message_text(
                    message_type,
                    task_name,
                    sender,
                    payload,
                );
                self.append(RuntimeLogEvent::UserMessage(RuntimeUserMessageEvent {
                    turn_id: Some(message.id.clone()),
                    message_id: message.id.clone(),
                    text,
                    attachments: Vec::new(),
                }));
            }
        }
    }

    /// Opens and records one assistant tool-call batch.
    /// 打开并记录一个助手工具调用批次。
    fn open_tool_batch(
        &mut self,
        message: &SessionConversationMessage,
        tool_calls: &[SessionConversationToolCall],
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) {
        let tool_batch_id = format!("tool-batch-{}", message.id);
        self.append(RuntimeLogEvent::ToolBatchStarted(
            RuntimeToolBatchStartedEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
            },
        ));
        for tool_call in tool_calls {
            let runtime_tool_call_id = runtime_tool_call_id(tool_call);
            open_batches.insert(
                runtime_tool_call_id.clone(),
                RuntimeOpenToolBatch {
                    tool_batch_id: tool_batch_id.clone(),
                    tool_call_count: tool_calls.len(),
                    tool_result_count: 0,
                },
            );
            self.append(RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
                runtime_tool_call_id,
                provider_tool_call_id: tool_call.tool_call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                arguments: tool_call.arguments.clone(),
            }));
        }
    }

    /// Closes every open batch that survived projection.
    /// 关闭投影后仍保持打开的每个批次。
    fn close_remaining_batches(&mut self, open_batches: BTreeMap<String, RuntimeOpenToolBatch>) {
        let mut closed_batches = BTreeMap::<String, RuntimeOpenToolBatch>::new();
        for batch in open_batches.into_values() {
            closed_batches
                .entry(batch.tool_batch_id.clone())
                .and_modify(|existing| {
                    existing.tool_result_count = existing
                        .tool_result_count
                        .saturating_add(batch.tool_result_count);
                    existing.tool_call_count = existing.tool_call_count.max(batch.tool_call_count);
                })
                .or_insert(batch);
        }
        for batch in closed_batches.into_values() {
            self.append(RuntimeLogEvent::ToolBatchCompleted(
                RuntimeToolBatchCompletedEvent {
                    turn_id: None,
                    tool_batch_id: batch.tool_batch_id,
                    tool_call_count: batch.tool_call_count,
                    tool_result_count: batch.tool_result_count,
                    complete: batch.tool_call_count == batch.tool_result_count,
                },
            ));
        }
    }
}

/// Tracks one open tool batch during conversation-to-runtime projection.
/// 在会话到运行时投影期间跟踪一个打开的工具批次。
#[derive(Debug, Clone)]
struct RuntimeOpenToolBatch {
    /// Runtime batch identifier.
    /// 运行时批次标识。
    tool_batch_id: String,
    /// Number of tool calls inside the batch.
    /// 批次中的工具调用数量。
    tool_call_count: usize,
    /// Number of matched tool results seen so far.
    /// 当前已经看到的匹配工具结果数量。
    tool_result_count: usize,
}

/// Internal projection state used to rebuild conversation messages from runtime events.
/// 用于从运行时事件重建会话消息的内部投影状态。
#[derive(Debug, Default)]
struct RuntimeConversationProjection {
    /// Rebuilt canonical messages.
    /// 已重建的规范消息。
    messages: Vec<SessionConversationMessage>,
    /// Assistant messages keyed by local message id.
    /// 按本地消息标识索引的助手消息。
    assistant_indexes: BTreeMap<String, usize>,
    /// Tool calls keyed by runtime-owned id.
    /// 按运行时持有标识索引的工具调用。
    tool_calls: BTreeMap<String, RuntimeToolCallEvent>,
}

impl RuntimeConversationProjection {
    /// Applies one runtime event to the projection.
    /// 将一条运行时事件应用到投影。
    fn apply_event(&mut self, event: &RuntimeLogEvent) {
        match event {
            RuntimeLogEvent::UserMessage(event) => self.push_user_message(event),
            RuntimeLogEvent::AssistantMessage(event) => self.push_assistant_message(event),
            RuntimeLogEvent::ToolCall(event) => self.attach_tool_call(event),
            RuntimeLogEvent::ToolResult(event) => self.push_tool_result(event),
            RuntimeLogEvent::ToolBatchStarted(_)
            | RuntimeLogEvent::ToolBatchCompleted(_)
            | RuntimeLogEvent::ProviderTurnCheckpoint(_)
            | RuntimeLogEvent::ContextCompactionApplied(_)
            | RuntimeLogEvent::RuntimeRepair(_)
            | RuntimeLogEvent::InjectionApplied(_)
            | RuntimeLogEvent::SystemUpdate(_)
            | RuntimeLogEvent::InjectionCancelled(_)
            | RuntimeLogEvent::InjectionMerged(_)
            | RuntimeLogEvent::ProviderRequestCheckpoint(_)
            | RuntimeLogEvent::ProviderResponseCheckpoint(_)
            | RuntimeLogEvent::ProviderLoweringRepair(_) => {}
        }
    }

    /// Finalizes the rebuilt conversation state.
    /// 完成已重建会话状态。
    fn finish(self) -> SessionConversationState {
        SessionConversationState {
            messages: normalize_replayed_protocol_messages(self.messages),
            records: Vec::new(),
            continuation: Default::default(),
        }
    }

    /// Adds one user message to the projection.
    /// 向投影添加一条用户消息。
    fn push_user_message(&mut self, event: &RuntimeUserMessageEvent) {
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::UserText {
                text: event.text.clone(),
                attachments: event.attachments.clone(),
            },
        });
    }

    /// Adds one assistant message to the projection.
    /// 向投影添加一条助手消息。
    fn push_assistant_message(&mut self, event: &RuntimeAssistantMessageEvent) {
        let index = self.messages.len();
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::Assistant {
                text: event.text.clone(),
                reasoning: event.reasoning.clone(),
                provider_message_id: event.provider_message_id.clone(),
                provider_message_state: event.provider_message_state.clone(),
                usage: event.usage.clone(),
                tool_calls: Vec::new(),
            },
        });
        self.assistant_indexes
            .insert(event.message_id.clone(), index);
    }

    /// Attaches one tool call to its owning assistant message.
    /// 将一条工具调用附加到其所属助手消息。
    fn attach_tool_call(&mut self, event: &RuntimeToolCallEvent) {
        self.tool_calls
            .insert(event.runtime_tool_call_id.clone(), event.clone());
        let Some(index) = self
            .assistant_indexes
            .get(&event.assistant_message_id)
            .copied()
        else {
            return;
        };
        let Some(message) = self.messages.get_mut(index) else {
            return;
        };
        let SessionConversationPayload::Assistant { tool_calls, .. } = &mut message.payload else {
            return;
        };
        tool_calls.push(SessionConversationToolCall {
            id: event.runtime_tool_call_id.clone(),
            tool_call_id: event
                .provider_tool_call_id
                .clone()
                .or_else(|| Some(event.runtime_tool_call_id.clone())),
            tool_name: event.tool_name.clone(),
            arguments: event.arguments.clone(),
        });
    }

    /// Adds one matched or downgraded tool result to the projection.
    /// 向投影添加一条已匹配或已降级的工具结果。
    fn push_tool_result(&mut self, event: &RuntimeToolResultEvent) {
        self.messages.push(SessionConversationMessage {
            id: event.tool_result_id.clone(),
            payload: SessionConversationPayload::ToolResult {
                tool_result_id: event.tool_result_id.clone(),
                tool_call_id: event
                    .provider_tool_call_id
                    .clone()
                    .or_else(|| event.runtime_tool_call_id.clone()),
                text: event.text.clone(),
            },
        });
    }
}

/// Resolves the stable runtime id for one tool call.
/// 解析单条工具调用的稳定运行时标识。
fn runtime_tool_call_id(tool_call: &SessionConversationToolCall) -> String {
    tool_call
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| tool_call.id.clone())
}

//! Projection from canonical conversation state into runtime event logs.
//! 从规范会话状态到运行时事件日志的投影。

use super::{
    events::*,
    identity::{
        CanonicalMessageIdentifierError, normalized_optional_identifier,
        register_canonical_message_identifier,
    },
    projection_error::RuntimeLogProjectionError,
};
use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall,
};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_core::types::ConversationPayload;

/// Converts one canonical conversation state into runtime log lines.
/// 将一个规范会话状态转换为运行时日志行。
///
/// # Parameters
/// 参数
/// - `state`: Canonical provider-facing conversation state to project.
/// - `state`：需要投影的规范 Provider 可见会话状态。
/// - `created_at`: Timestamp attached to every generated runtime event.
/// - `created_at`：附加到每条生成运行时事件的时间戳。
/// - `starting_seq`: First sequence available to the projection.
/// - `starting_seq`：投影可使用的首个序号。
///
/// # Returns
/// 返回值
/// Complete ordered runtime lines, or a strict sequence/tool-batch projection error.
/// 返回完整有序运行时日志行，或严格的序号/工具批次投影错误。
pub fn runtime_log_lines_from_conversation_state(
    state: &SessionConversationState,
    created_at: &str,
    starting_seq: u64,
) -> Result<Vec<RuntimeLogEventLine>, RuntimeLogProjectionError> {
    // builder owns every mutable projection invariant for this one all-or-nothing result.
    // builder 持有本次全有或全无结果的全部可变投影不变量。
    let mut builder = RuntimeLogProjectionBuilder::new(created_at, starting_seq);
    // open_batches retains every call id so duplicates and repeated results remain observable.
    // open_batches 保留每个调用标识，使重复调用与重复结果保持可观察。
    let mut open_batches = BTreeMap::<String, RuntimeOpenToolBatch>::new();
    for message in &state.messages {
        builder.push_message(message, &mut open_batches)?;
    }
    builder.close_remaining_batches(open_batches)?;
    Ok(builder.lines)
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
    /// Every canonical message identifier already projected into runtime events.
    /// 已经投影到运行时事件中的每个规范消息标识。
    message_ids: BTreeSet<String>,
}

impl<'a> RuntimeLogProjectionBuilder<'a> {
    /// Builds one projection builder.
    /// 构建一个投影构建器。
    ///
    /// # Parameters
    /// 参数
    /// - `created_at`: Timestamp borrowed by every generated event line.
    /// - `created_at`：每条生成事件行借用的时间戳。
    /// - `starting_seq`: First sequence available for assignment.
    /// - `starting_seq`：首个可分配序号。
    ///
    /// # Returns
    /// 返回值
    /// An empty builder initialized with the requested timestamp and sequence boundary.
    /// 返回使用指定时间戳与序号边界初始化的空构建器。
    fn new(created_at: &'a str, starting_seq: u64) -> Self {
        Self {
            created_at,
            next_seq: starting_seq,
            lines: Vec::new(),
            message_ids: BTreeSet::new(),
        }
    }

    /// Appends a runtime event and advances the local sequence.
    /// 追加一条运行时事件并推进本地序号。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Fully materialized runtime event to append.
    /// - `event`：需要追加的完整运行时事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after assigning one sequence, or a sequence-exhaustion error before mutation.
    /// 分配一个序号后返回成功；序号耗尽时在修改前返回错误。
    fn append(&mut self, event: RuntimeLogEvent) -> Result<(), RuntimeLogProjectionError> {
        // following_seq is proven before the current event becomes externally returnable.
        // following_seq 会在当前事件能够被对外返回前完成证明。
        let following_seq =
            self.next_seq
                .checked_add(1)
                .ok_or(RuntimeLogProjectionError::SequenceExhausted {
                    sequence: self.next_seq,
                })?;
        self.lines.push(RuntimeLogEventLine::new(
            self.next_seq,
            self.created_at,
            event,
        ));
        self.next_seq = following_seq;
        Ok(())
    }

    /// Projects one canonical message into runtime events.
    /// 将一条规范消息投影为运行时事件。
    ///
    /// # Parameters
    /// 参数
    /// - `message`: Canonical message to project.
    /// - `message`：需要投影的规范消息。
    /// - `open_batches`: Call-id keyed batch states retained across subsequent result messages.
    /// - `open_batches`：跨后续结果消息保留、按调用标识索引的批次状态。
    ///
    /// # Returns
    /// 返回值
    /// Success after projecting the complete message, or the first strict projection failure.
    /// 完整投影消息后返回成功，或返回首个严格投影失败。
    fn push_message(
        &mut self,
        message: &SessionConversationMessage,
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) -> Result<(), RuntimeLogProjectionError> {
        // message_kind makes identity failures actionable without deriving behavior from display text.
        // message_kind 使身份失败可直接诊断，且不从显示文本推导行为。
        let message_kind = match &message.payload {
            SessionConversationPayload::UserText { .. } => "user",
            SessionConversationPayload::Assistant { .. } => "assistant",
            SessionConversationPayload::ToolResult { .. } => "tool_result",
            SessionConversationPayload::AgentMessage { .. } => "agent",
        };
        register_canonical_message_identifier(&mut self.message_ids, &message.id).map_err(
            |error| match error {
                CanonicalMessageIdentifierError::Missing => {
                    RuntimeLogProjectionError::MissingMessageIdentifier { message_kind }
                }
                CanonicalMessageIdentifierError::NonCanonical { identifier } => {
                    RuntimeLogProjectionError::NonCanonicalMessageIdentifier {
                        message_kind,
                        message_id: identifier,
                    }
                }
                CanonicalMessageIdentifierError::Duplicate { identifier } => {
                    RuntimeLogProjectionError::DuplicateMessageIdentifier {
                        message_id: identifier,
                    }
                }
            },
        )?;
        match &message.payload {
            SessionConversationPayload::UserText { text, attachments } => {
                self.append(RuntimeLogEvent::UserMessage(RuntimeUserMessageEvent {
                    turn_id: Some(message.id.clone()),
                    message_id: message.id.clone(),
                    text: text.clone(),
                    attachments: attachments.clone(),
                }))?;
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
                        usage: usage.as_deref().cloned(),
                    },
                ))?;
                if !tool_calls.is_empty() {
                    self.open_tool_batch(message, tool_calls, open_batches)?;
                }
            }
            SessionConversationPayload::ToolResult {
                tool_call_id,
                is_error,
                text,
            } => {
                // provider_tool_call_id is normalized once and reused as the runtime lookup id.
                // provider_tool_call_id 只规范化一次，并复用为运行时查找标识。
                let provider_tool_call_id = normalized_optional_identifier(tool_call_id.as_deref());
                // runtime_tool_call_id has no local fallback because result messages own no call item id.
                // runtime_tool_call_id 不使用本地回退，因为结果消息不拥有调用条目标识。
                let runtime_tool_call_id = provider_tool_call_id.clone();
                // tool_batch_id is assigned only once per pending call; repeated results are orphans.
                // tool_batch_id 仅为每个待处理调用分配一次；重复结果属于孤儿结果。
                let tool_batch_id = runtime_tool_call_id
                    .as_deref()
                    .and_then(|id| open_batches.get_mut(id))
                    .and_then(|batch| {
                        if batch.matched {
                            None
                        } else {
                            batch.matched = true;
                            Some(batch.tool_batch_id.clone())
                        }
                    });
                // status reflects an actual first match rather than mere identifier presence.
                // status 反映真实的首次匹配，而不是仅凭标识存在性判断。
                let status = if tool_batch_id.is_some() && *is_error {
                    RuntimeToolResultStatus::Failed
                } else if tool_batch_id.is_some() {
                    RuntimeToolResultStatus::Completed
                } else {
                    RuntimeToolResultStatus::Orphan
                };
                self.append(RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                    turn_id: Some(message.id.clone()),
                    message_id: message.id.clone(),
                    tool_batch_id,
                    runtime_tool_call_id,
                    provider_tool_call_id,
                    text: text.clone(),
                    status,
                    error: is_error.then(|| text.clone()),
                }))?;
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
                }))?;
            }
        }
        Ok(())
    }

    /// Opens and records one assistant tool-call batch.
    /// 打开并记录一个助手工具调用批次。
    ///
    /// # Parameters
    /// 参数
    /// - `message`: Assistant message that owns the new batch.
    /// - `message`：拥有新批次的助手消息。
    /// - `tool_calls`: Non-empty canonical tool calls to validate and emit.
    /// - `tool_calls`：需要验证并发出的非空规范工具调用。
    /// - `open_batches`: Shared call-id keyed state for later result matching.
    /// - `open_batches`：供后续结果匹配使用的共享调用标识索引状态。
    ///
    /// # Returns
    /// 返回值
    /// Success after emitting the complete batch, or a strict identity/sequence failure.
    /// 完整发出批次后返回成功，或返回严格身份/序号失败。
    fn open_tool_batch(
        &mut self,
        message: &SessionConversationMessage,
        tool_calls: &[SessionConversationToolCall],
        open_batches: &mut BTreeMap<String, RuntimeOpenToolBatch>,
    ) -> Result<(), RuntimeLogProjectionError> {
        // tool_batch_id is stable because assistant message identity was validated before entry.
        // tool_batch_id 保持稳定，因为助手消息身份已在进入本函数前验证。
        let tool_batch_id = format!("tool-batch-{}", message.id);
        // resolved_calls validates the complete batch before emitting its first runtime event.
        // resolved_calls 会在发出首条运行时事件前验证完整批次。
        let resolved_calls = tool_calls
            .iter()
            .map(|tool_call| {
                runtime_tool_call_id(tool_call)
                    .map(|runtime_tool_call_id| (tool_call, runtime_tool_call_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // batch_call_ids detects duplicates inside the new batch before shared state changes.
        // batch_call_ids 会在共享状态修改前检测新批次内部的重复标识。
        let mut batch_call_ids = BTreeSet::new();
        for (_, runtime_tool_call_id) in &resolved_calls {
            if open_batches.contains_key(runtime_tool_call_id)
                || !batch_call_ids.insert(runtime_tool_call_id.clone())
            {
                return Err(RuntimeLogProjectionError::DuplicateToolCallIdentifier {
                    tool_call_id: runtime_tool_call_id.clone(),
                });
            }
        }
        self.append(RuntimeLogEvent::ToolBatchStarted(
            RuntimeToolBatchStartedEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
            },
        ))?;
        for (tool_call, runtime_tool_call_id) in resolved_calls {
            open_batches.insert(
                runtime_tool_call_id.clone(),
                RuntimeOpenToolBatch {
                    tool_batch_id: tool_batch_id.clone(),
                    matched: false,
                },
            );
            self.append(RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                turn_id: Some(message.id.clone()),
                tool_batch_id: tool_batch_id.clone(),
                assistant_message_id: message.id.clone(),
                runtime_tool_call_id,
                provider_tool_call_id: normalized_optional_identifier(
                    tool_call.tool_call_id.as_deref(),
                ),
                tool_name: tool_call.tool_name.clone(),
                arguments: tool_call.arguments.clone(),
            }))?;
        }
        Ok(())
    }

    /// Closes every open batch that survived projection.
    /// 关闭投影后仍保持打开的每个批次。
    ///
    /// # Parameters
    /// 参数
    /// - `open_batches`: Complete unique call states accumulated during projection.
    /// - `open_batches`：投影期间积累的完整唯一调用状态。
    ///
    /// # Returns
    /// 返回值
    /// Success after emitting exact batch summaries, or a cardinality/sequence failure.
    /// 发出精确批次汇总后返回成功，或返回基数/序号失败。
    fn close_remaining_batches(
        &mut self,
        open_batches: BTreeMap<String, RuntimeOpenToolBatch>,
    ) -> Result<(), RuntimeLogProjectionError> {
        // closed_batches aggregates exactly one call entry per unique runtime call id.
        // closed_batches 对每个唯一运行时调用标识精确聚合一条调用记录。
        let mut closed_batches = BTreeMap::<String, RuntimeClosedToolBatch>::new();
        for batch in open_batches.into_values() {
            // summary is unique per previously validated tool batch id.
            // summary 对每个此前已验证的工具批次标识保持唯一。
            let summary = closed_batches
                .entry(batch.tool_batch_id.clone())
                .or_insert_with(|| RuntimeClosedToolBatch {
                    tool_batch_id: batch.tool_batch_id.clone(),
                    tool_call_count: 0,
                    tool_result_count: 0,
                });
            summary.tool_call_count = summary.tool_call_count.checked_add(1).ok_or_else(|| {
                RuntimeLogProjectionError::ToolBatchCardinalityOverflow {
                    tool_batch_id: batch.tool_batch_id.clone(),
                }
            })?;
            summary.tool_result_count = summary
                .tool_result_count
                .checked_add(usize::from(batch.matched))
                .ok_or(RuntimeLogProjectionError::ToolBatchCardinalityOverflow {
                    tool_batch_id: batch.tool_batch_id,
                })?;
        }
        for batch in closed_batches.into_values() {
            // An incomplete batch remains open and must not emit a mutable completion summary.
            // 未完成批次保持打开状态，不得发出后续会变化的完成汇总。
            if batch.tool_call_count != batch.tool_result_count {
                continue;
            }
            self.append(RuntimeLogEvent::ToolBatchCompleted(
                RuntimeToolBatchCompletedEvent {
                    turn_id: None,
                    tool_batch_id: batch.tool_batch_id,
                    tool_call_count: batch.tool_call_count,
                    tool_result_count: batch.tool_result_count,
                    complete: true,
                },
            ))?;
        }
        Ok(())
    }
}

/// Tracks one open tool batch during conversation-to-runtime projection.
/// 在会话到运行时投影期间跟踪一个打开的工具批次。
#[derive(Debug)]
struct RuntimeOpenToolBatch {
    /// Runtime batch identifier.
    /// 运行时批次标识。
    tool_batch_id: String,
    /// Whether this exact tool call already consumed its single matching result.
    /// 当前精确工具调用是否已经消费其唯一匹配结果。
    matched: bool,
}

/// Aggregates exact tool-call and first-match counts for one completed runtime batch.
/// 聚合一个已完成运行时批次的精确工具调用数与首次匹配数。
#[derive(Debug)]
struct RuntimeClosedToolBatch {
    /// Stable runtime batch identifier.
    /// 稳定运行时批次标识。
    tool_batch_id: String,
    /// Exact number of unique tool calls in the batch.
    /// 批次中的唯一工具调用精确数量。
    tool_call_count: usize,
    /// Exact number of calls that consumed one matching result.
    /// 已消费一条匹配结果的调用精确数量。
    tool_result_count: usize,
}

/// Resolves and normalizes the stable runtime id for one tool call.
/// 解析并规范化单条工具调用的稳定运行时标识。
///
/// # Parameters
/// 参数
/// - `tool_call`: Canonical tool call whose provider id or stable item id is required.
/// - `tool_call`：必须提供 Provider 标识或稳定条目标识的规范工具调用。
///
/// # Returns
/// 返回值
/// A non-empty normalized runtime id, or an explicit missing-identifier error.
/// 返回非空规范化运行时标识，或明确的标识缺失错误。
fn runtime_tool_call_id(
    tool_call: &SessionConversationToolCall,
) -> Result<String, RuntimeLogProjectionError> {
    tool_call
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let stable_id = tool_call.id.trim();
            (!stable_id.is_empty()).then_some(stable_id)
        })
        .map(str::to_string)
        .ok_or_else(|| RuntimeLogProjectionError::MissingToolCallIdentifier {
            tool_name: tool_call.tool_name.clone(),
        })
}

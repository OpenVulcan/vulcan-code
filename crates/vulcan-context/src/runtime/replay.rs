//! Replay from runtime event logs into canonical conversation state.
//! 从运行时事件日志到规范会话状态的重放。

use super::{
    events::*,
    identity::{
        CanonicalMessageIdentifierError, normalized_optional_identifier,
        register_canonical_message_identifier,
    },
    replay_error::RuntimeLogReplayError,
};
use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall, historical_tool_result_text_message,
    normalize_replayed_protocol_messages,
};
use std::collections::{BTreeMap, BTreeSet};

/// Rebuilds a canonical conversation state from runtime log lines.
/// 从运行时日志行重建规范会话状态。
///
/// # Parameters
/// 参数
/// - `lines`: Ordered runtime event lines to validate and replay.
/// - `lines`：需要验证并重放的有序运行时事件行。
///
/// # Returns
/// 返回值
/// Canonical conversation state after structural replay, or the first strict replay failure.
/// 返回结构化重放后的规范会话状态，或首个严格重放失败。
pub fn conversation_state_from_runtime_log_lines(
    lines: &[RuntimeLogEventLine],
) -> Result<SessionConversationState, RuntimeLogReplayError> {
    // expected_seq is anchored by the first supplied line because this API accepts sliced logs.
    // expected_seq 由首条输入日志锚定，因为本 API 允许接收日志切片。
    let mut expected_seq = lines.first().map(|line| line.seq);
    let mut projection = RuntimeConversationProjection::default();
    for (line_index, line) in lines.iter().enumerate() {
        // line_number is safe because every valid slice index is strictly below its usize length.
        // line_number 是安全的，因为每个有效切片索引都严格小于其 usize 长度。
        let line_number = line_index + 1;
        if line.schema_version != RUNTIME_LOG_SCHEMA_VERSION {
            return Err(RuntimeLogReplayError::UnsupportedSchemaVersion {
                line_number,
                schema_version: line.schema_version,
            });
        }
        if let Some(expected) = expected_seq
            && line.seq != expected
        {
            return Err(RuntimeLogReplayError::SequenceMismatch {
                line_number,
                expected,
                actual: line.seq,
            });
        }
        if line_index + 1 < lines.len() {
            expected_seq = Some(line.seq.checked_add(1).ok_or(
                RuntimeLogReplayError::SequenceExhausted {
                    line_number,
                    sequence: line.seq,
                },
            )?);
        }
        projection.apply_event(&line.event)?;
    }
    Ok(projection.finish())
}

/// Internal projection state used to rebuild conversation messages from runtime events.
/// 用于从运行时事件重建会话消息的内部投影状态。
#[derive(Debug, Default)]
struct RuntimeConversationProjection {
    /// Rebuilt canonical messages.
    /// 已重建的规范消息。
    messages: Vec<SessionConversationMessage>,
    /// Every canonical protocol-message identifier already accepted during replay.
    /// 重放期间已经接受的每个规范协议消息标识。
    message_ids: BTreeSet<String>,
    /// Assistant messages keyed by local message id.
    /// 按本地消息标识索引的助手消息。
    assistant_indexes: BTreeMap<String, usize>,
    /// Tool calls keyed by runtime-owned id.
    /// 按运行时持有标识索引的工具调用。
    tool_calls: BTreeMap<String, RuntimeReplayToolCallState>,
    /// Runtime-owned call ids keyed by unique provider-visible call id.
    /// 按唯一 Provider 可见调用标识索引的运行时自有调用标识。
    provider_tool_call_ids: BTreeMap<String, String>,
    /// Tool-batch control and exact observed cardinality keyed by runtime batch id.
    /// 按运行时批次标识索引的工具批次控制状态与精确观察基数。
    tool_batches: BTreeMap<String, RuntimeReplayToolBatchState>,
    /// Whether the first valid batch-completion event has closed conversation-event replay.
    /// 首条有效批次完成事件是否已经关闭会话事件重放阶段。
    tool_batch_completion_started: bool,
    /// Whether an audit-only event has permanently closed projection and completion replay.
    /// 仅审计事件是否已经永久关闭投影与完成重放阶段。
    runtime_audit_started: bool,
}

impl RuntimeConversationProjection {
    /// Applies one runtime event to the projection.
    /// 将一条运行时事件应用到投影。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Runtime event whose conversation-visible structure is replayed or ignored.
    /// - `event`：需要重放或忽略其会话可见结构的运行时事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after applying or intentionally ignoring the event, or a strict relationship error.
    /// 应用或有意忽略事件后返回成功，或返回严格关系错误。
    fn apply_event(&mut self, event: &RuntimeLogEvent) -> Result<(), RuntimeLogReplayError> {
        // is_conversation_event identifies events emitted before the completion phase.
        // is_conversation_event 标识在完成阶段前发出的事件。
        let is_conversation_event = matches!(
            event,
            RuntimeLogEvent::UserMessage(_)
                | RuntimeLogEvent::AssistantMessage(_)
                | RuntimeLogEvent::ToolBatchStarted(_)
                | RuntimeLogEvent::ToolCall(_)
                | RuntimeLogEvent::ToolResult(_)
        );
        // is_completion_event identifies the only event allowed during the completion phase.
        // is_completion_event 标识完成阶段唯一允许的事件。
        let is_completion_event = matches!(event, RuntimeLogEvent::ToolBatchCompleted(_));
        if self.runtime_audit_started && (is_conversation_event || is_completion_event) {
            return Err(RuntimeLogReplayError::ProjectionEventAfterRuntimeAudit {
                later_event: runtime_replay_event_kind(event),
            });
        }
        if self.tool_batch_completion_started && is_conversation_event {
            return Err(
                RuntimeLogReplayError::ConversationEventAfterToolBatchCompletion {
                    later_event: runtime_replay_event_kind(event),
                },
            );
        }
        if !is_conversation_event && !is_completion_event {
            self.runtime_audit_started = true;
        }
        match event {
            RuntimeLogEvent::UserMessage(event) => self.push_user_message(event)?,
            RuntimeLogEvent::AssistantMessage(event) => self.push_assistant_message(event)?,
            RuntimeLogEvent::ToolCall(event) => self.attach_tool_call(event)?,
            RuntimeLogEvent::ToolResult(event) => self.push_tool_result(event)?,
            RuntimeLogEvent::ToolBatchStarted(event) => self.push_tool_batch_started(event)?,
            RuntimeLogEvent::ToolBatchCompleted(event) => self.finish_tool_batch(event)?,
            RuntimeLogEvent::ProviderTurnCheckpoint(_)
            | RuntimeLogEvent::ContextCompactionApplied(_)
            | RuntimeLogEvent::RuntimeRepair(_)
            | RuntimeLogEvent::InjectionApplied(_)
            | RuntimeLogEvent::SystemUpdate(_)
            | RuntimeLogEvent::InjectionCancelled(_)
            | RuntimeLogEvent::InjectionMerged(_)
            | RuntimeLogEvent::ProviderRequestCheckpoint(_)
            | RuntimeLogEvent::ProviderResponseCheckpoint(_)
            | RuntimeLogEvent::ProviderLoweringRepair(_)
            | RuntimeLogEvent::VmmLifecycle(_) => {}
        }
        Ok(())
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

    /// Adds one uniquely identified user message to the projection.
    /// 向投影添加一条具有唯一标识的用户消息。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: User runtime event whose local message identity must be canonical and unique.
    /// - `event`：本地消息身份必须规范且唯一的用户运行时事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after appending the user message, or the exact message-identity failure.
    /// 追加用户消息后返回成功，或返回精确消息身份失败。
    fn push_user_message(
        &mut self,
        event: &RuntimeUserMessageEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        self.register_message_identifier("user", &event.message_id)?;
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::UserText {
                text: event.text.clone(),
                attachments: event.attachments.clone(),
            },
        });
        Ok(())
    }

    /// Adds one assistant message to the projection.
    /// 向投影添加一条助手消息。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Assistant runtime event whose stable id must be canonical and unique.
    /// - `event`：稳定标识必须规范且唯一的助手运行时事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after indexing the assistant, or the exact message-identity failure.
    /// 索引助手后返回成功，或返回精确消息身份失败。
    fn push_assistant_message(
        &mut self,
        event: &RuntimeAssistantMessageEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        self.register_message_identifier("assistant", &event.message_id)?;
        let index = self.messages.len();
        self.messages.push(SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::Assistant {
                text: event.text.clone(),
                reasoning: event.reasoning.clone(),
                provider_message_id: event.provider_message_id.clone(),
                provider_message_state: event.provider_message_state.clone(),
                usage: event.usage.clone().map(Box::new),
                tool_calls: Vec::new(),
            },
        });
        self.assistant_indexes
            .insert(event.message_id.clone(), index);
        Ok(())
    }

    /// Opens one runtime tool batch after validating its stable identity and assistant owner.
    /// 在验证稳定身份与助手归属后打开一个运行时工具批次。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Batch-start control event to validate and index.
    /// - `event`：需要验证并索引的批次开始控制事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after opening the batch, or a strict identity/ownership error.
    /// 打开批次后返回成功，或返回严格身份/归属错误。
    fn push_tool_batch_started(
        &mut self,
        event: &RuntimeToolBatchStartedEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        // tool_batch_id is mandatory and normalized before indexing.
        // tool_batch_id 是必需字段，并在索引前完成规范化。
        let tool_batch_id = event.tool_batch_id.trim().to_string();
        if tool_batch_id.is_empty() {
            return Err(RuntimeLogReplayError::MissingToolBatchControlIdentifier {
                control_event: "tool_batch_started",
            });
        }
        if self.tool_batches.contains_key(&tool_batch_id) {
            return Err(RuntimeLogReplayError::DuplicateToolBatchIdentifier { tool_batch_id });
        }
        if !self
            .assistant_indexes
            .contains_key(&event.assistant_message_id)
        {
            return Err(RuntimeLogReplayError::MissingToolBatchAssistant {
                tool_batch_id,
                assistant_message_id: event.assistant_message_id.clone(),
            });
        }
        self.tool_batches.insert(
            tool_batch_id,
            RuntimeReplayToolBatchState {
                assistant_message_id: event.assistant_message_id.clone(),
                tool_call_count: 0,
                tool_result_count: 0,
                completed: false,
            },
        );
        Ok(())
    }

    /// Validates and records one present runtime tool-batch completion event.
    /// 验证并记录一条已存在的运行时工具批次完成事件。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Completion control event whose summary must match observed batch state.
    /// - `event`：其汇总必须匹配已观察批次状态的完成控制事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after closing the batch, or a strict identity/order/summary error.
    /// 关闭批次后返回成功，或返回严格身份/顺序/汇总错误。
    fn finish_tool_batch(
        &mut self,
        event: &RuntimeToolBatchCompletedEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        // tool_batch_id is mandatory and normalized before lookup.
        // tool_batch_id 是必需字段，并在查找前完成规范化。
        let tool_batch_id = event.tool_batch_id.trim().to_string();
        if tool_batch_id.is_empty() {
            return Err(RuntimeLogReplayError::MissingToolBatchControlIdentifier {
                control_event: "tool_batch_completed",
            });
        }
        let batch = self.tool_batches.get_mut(&tool_batch_id).ok_or_else(|| {
            RuntimeLogReplayError::UnknownToolBatchCompletion {
                tool_batch_id: tool_batch_id.clone(),
            }
        })?;
        if batch.completed {
            return Err(RuntimeLogReplayError::DuplicateToolBatchCompletion { tool_batch_id });
        }
        if batch.tool_call_count == 0 {
            return Err(RuntimeLogReplayError::EmptyToolBatchCompletion { tool_batch_id });
        }
        // expected_complete is derived only from exact observed call and matched-result counts.
        // expected_complete 仅从精确观察的调用数与已匹配结果数推导。
        let expected_complete = batch.tool_call_count == batch.tool_result_count;
        if event.tool_call_count != batch.tool_call_count
            || event.tool_result_count != batch.tool_result_count
            || event.complete != expected_complete
        {
            return Err(RuntimeLogReplayError::ToolBatchSummaryMismatch {
                tool_batch_id,
                expected_tool_call_count: batch.tool_call_count,
                actual_tool_call_count: event.tool_call_count,
                expected_tool_result_count: batch.tool_result_count,
                actual_tool_result_count: event.tool_result_count,
                expected_complete,
                actual_complete: event.complete,
            });
        }
        batch.completed = true;
        self.tool_batch_completion_started = true;
        Ok(())
    }

    /// Attaches one tool call to its owning assistant message.
    /// 将一条工具调用附加到其所属助手消息。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Tool-call event with required unique runtime identity and assistant ownership.
    /// - `event`：必须具有唯一运行时身份与助手归属的工具调用事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after indexing and attaching the call, or a strict identity/ownership error.
    /// 索引并附加调用后返回成功，或返回严格身份/归属错误。
    fn attach_tool_call(
        &mut self,
        event: &RuntimeToolCallEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        // runtime_tool_call_id is mandatory on tool-call events and compared in normalized form.
        // runtime_tool_call_id 在工具调用事件上是必需字段，并以规范化形式比较。
        let runtime_tool_call_id = event.runtime_tool_call_id.trim().to_string();
        if runtime_tool_call_id.is_empty() {
            return Err(RuntimeLogReplayError::MissingToolCallIdentifier {
                assistant_message_id: event.assistant_message_id.clone(),
            });
        }
        if self.tool_calls.contains_key(&runtime_tool_call_id) {
            return Err(RuntimeLogReplayError::DuplicateToolCallIdentifier {
                tool_call_id: runtime_tool_call_id,
            });
        }
        if self
            .provider_tool_call_ids
            .contains_key(&runtime_tool_call_id)
        {
            return Err(RuntimeLogReplayError::AmbiguousToolCallIdentifier {
                tool_call_id: runtime_tool_call_id,
            });
        }
        // provider_tool_call_id remains optional but must be unique and cross-namespace unambiguous.
        // provider_tool_call_id 保持可选，但一旦提供就必须唯一且不能跨命名空间产生歧义。
        let provider_tool_call_id =
            normalized_optional_identifier(event.provider_tool_call_id.as_deref());
        if let Some(provider_tool_call_id) = provider_tool_call_id.as_ref() {
            if self
                .provider_tool_call_ids
                .contains_key(provider_tool_call_id)
            {
                return Err(RuntimeLogReplayError::DuplicateProviderToolCallIdentifier {
                    provider_tool_call_id: provider_tool_call_id.clone(),
                });
            }
            if self.tool_calls.contains_key(provider_tool_call_id) {
                return Err(RuntimeLogReplayError::AmbiguousToolCallIdentifier {
                    tool_call_id: provider_tool_call_id.clone(),
                });
            }
        }
        // tool_batch_id is required by the tool-call event contract and normalized before storage.
        // tool_batch_id 由工具调用事件契约要求，并在存储前完成规范化。
        let tool_batch_id = event.tool_batch_id.trim().to_string();
        if tool_batch_id.is_empty() {
            return Err(RuntimeLogReplayError::MissingToolBatchIdentifier {
                tool_call_id: runtime_tool_call_id,
            });
        }
        let Some(index) = self
            .assistant_indexes
            .get(&event.assistant_message_id)
            .copied()
        else {
            return Err(RuntimeLogReplayError::MissingToolCallAssistant {
                tool_call_id: runtime_tool_call_id,
                assistant_message_id: event.assistant_message_id.clone(),
            });
        };
        // batch must have an earlier start event and the same assistant owner.
        // batch 必须拥有更早的开始事件以及相同助手归属。
        let batch = self.tool_batches.get_mut(&tool_batch_id).ok_or_else(|| {
            RuntimeLogReplayError::MissingToolBatchStart {
                tool_call_id: runtime_tool_call_id.clone(),
                tool_batch_id: tool_batch_id.clone(),
            }
        })?;
        if batch.assistant_message_id != event.assistant_message_id {
            return Err(RuntimeLogReplayError::ToolCallBatchAssistantMismatch {
                tool_call_id: runtime_tool_call_id,
                tool_batch_id,
                expected_assistant_message_id: batch.assistant_message_id.clone(),
                actual_assistant_message_id: event.assistant_message_id.clone(),
            });
        }
        batch.tool_call_count = batch.tool_call_count.checked_add(1).ok_or_else(|| {
            RuntimeLogReplayError::ReplayToolBatchCardinalityOverflow {
                tool_batch_id: tool_batch_id.clone(),
            }
        })?;
        let Some(message) = self.messages.get_mut(index) else {
            return Err(RuntimeLogReplayError::MissingToolCallAssistant {
                tool_call_id: runtime_tool_call_id,
                assistant_message_id: event.assistant_message_id.clone(),
            });
        };
        let SessionConversationPayload::Assistant { tool_calls, .. } = &mut message.payload else {
            return Err(RuntimeLogReplayError::MissingToolCallAssistant {
                tool_call_id: runtime_tool_call_id,
                assistant_message_id: event.assistant_message_id.clone(),
            });
        };
        let provider_visible_tool_call_id = provider_tool_call_id
            .clone()
            .unwrap_or_else(|| runtime_tool_call_id.clone());
        tool_calls.push(SessionConversationToolCall {
            id: runtime_tool_call_id.clone(),
            tool_call_id: Some(provider_visible_tool_call_id.clone()),
            tool_name: event.tool_name.clone(),
            arguments: event.arguments.clone(),
        });
        if let Some(provider_tool_call_id) = provider_tool_call_id {
            self.provider_tool_call_ids
                .insert(provider_tool_call_id, runtime_tool_call_id.clone());
        }
        self.tool_calls.insert(
            runtime_tool_call_id,
            RuntimeReplayToolCallState {
                tool_batch_id,
                provider_visible_tool_call_id,
                matched: false,
            },
        );
        Ok(())
    }

    /// Resolves the unique prior runtime call referenced by one matched result.
    /// 解析一条已匹配结果引用的唯一前置运行时调用。
    ///
    /// # Parameters
    /// 参数
    /// - `message_id`: Stable local message identifier used in diagnostics.
    /// - `message_id`：用于诊断的稳定本地消息标识。
    /// - `runtime_tool_call_id`: Optional normalized runtime-owned identity.
    /// - `runtime_tool_call_id`：可选的规范化运行时自有身份。
    /// - `provider_tool_call_id`: Optional normalized provider-visible identity.
    /// - `provider_tool_call_id`：可选的规范化 Provider 可见身份。
    ///
    /// # Returns
    /// 返回值
    /// The unique runtime-owned call id, or an explicit missing/unknown/conflicting identity error.
    /// 返回唯一运行时自有调用标识，或明确的缺失/未知/冲突身份错误。
    fn resolve_matched_tool_result_call_id(
        &self,
        message_id: &str,
        runtime_tool_call_id: Option<&str>,
        provider_tool_call_id: Option<&str>,
    ) -> Result<String, RuntimeLogReplayError> {
        // resolved_runtime_id validates the runtime field independently when it is present.
        // resolved_runtime_id 在运行时字段存在时独立验证该字段。
        let resolved_runtime_id = runtime_tool_call_id
            .map(|identifier| self.resolve_tool_result_identifier(message_id, identifier))
            .transpose()?;
        // resolved_provider_id validates the provider field independently when it is present.
        // resolved_provider_id 在 Provider 字段存在时独立验证该字段。
        let resolved_provider_id = provider_tool_call_id
            .map(|identifier| self.resolve_tool_result_identifier(message_id, identifier))
            .transpose()?;
        match (resolved_runtime_id, resolved_provider_id) {
            (None, None) => Err(RuntimeLogReplayError::MissingMatchedToolResultIdentifier {
                message_id: message_id.to_string(),
            }),
            (Some(runtime_id), Some(provider_id)) if runtime_id != provider_id => Err(
                RuntimeLogReplayError::ConflictingMatchedToolCallIdentifiers {
                    message_id: message_id.to_string(),
                    runtime_tool_call_id: runtime_id,
                    provider_tool_call_id: provider_id,
                },
            ),
            (Some(runtime_id), _) => Ok(runtime_id),
            (_, Some(provider_id)) => Ok(provider_id),
        }
    }

    /// Resolves one normalized result identifier through runtime-owned or provider-visible indexes.
    /// 通过运行时自有或 Provider 可见索引解析一个规范化结果标识。
    ///
    /// # Parameters
    /// 参数
    /// - `message_id`: Stable local message identifier used in diagnostics.
    /// - `message_id`：用于诊断的稳定本地消息标识。
    /// - `identifier`: Normalized call identifier declared by the result.
    /// - `identifier`：结果声明的规范化调用标识。
    ///
    /// # Returns
    /// 返回值
    /// The owning runtime call id, or an explicit unknown-call error.
    /// 返回所属运行时调用标识，或明确的未知调用错误。
    fn resolve_tool_result_identifier(
        &self,
        message_id: &str,
        identifier: &str,
    ) -> Result<String, RuntimeLogReplayError> {
        if self.tool_calls.contains_key(identifier) {
            return Ok(identifier.to_string());
        }
        if let Some(runtime_tool_call_id) = self.provider_tool_call_ids.get(identifier) {
            return Ok(runtime_tool_call_id.clone());
        }
        Err(RuntimeLogReplayError::UnknownMatchedToolCall {
            message_id: message_id.to_string(),
            tool_call_id: identifier.to_string(),
        })
    }

    /// Adds one strictly matched or explicitly orphaned tool result to the projection.
    /// 向投影添加一条严格匹配或明确标记为孤儿的工具结果。
    ///
    /// # Parameters
    /// 参数
    /// - `event`: Tool-result event whose status controls strict matching or historical downgrade.
    /// - `event`：其状态决定严格匹配或历史降级的工具结果事件。
    ///
    /// # Returns
    /// 返回值
    /// Success after appending the result, or a strict matched-result relationship error.
    /// 追加结果后返回成功，或返回严格已匹配结果关系错误。
    fn push_tool_result(
        &mut self,
        event: &RuntimeToolResultEvent,
    ) -> Result<(), RuntimeLogReplayError> {
        self.register_message_identifier("tool_result", &event.message_id)?;
        // runtime_tool_call_id and provider_tool_call_id retain only explicit non-empty identities.
        // runtime_tool_call_id 与 provider_tool_call_id 仅保留明确的非空身份。
        let runtime_tool_call_id =
            normalized_optional_identifier(event.runtime_tool_call_id.as_deref());
        let provider_tool_call_id =
            normalized_optional_identifier(event.provider_tool_call_id.as_deref());
        // tool_batch_id is optional on results but normalized before any ownership comparison.
        // tool_batch_id 在结果上是可选字段，但会在任何归属比较前完成规范化。
        let tool_batch_id = normalized_optional_identifier(event.tool_batch_id.as_deref());
        let mut matched_provider_visible_tool_call_id = None;
        if event.status != RuntimeToolResultStatus::Orphan {
            let resolved_tool_call_id = self.resolve_matched_tool_result_call_id(
                &event.message_id,
                runtime_tool_call_id.as_deref(),
                provider_tool_call_id.as_deref(),
            )?;
            // call_state_snapshot releases the call-map borrow before batch cardinality is updated.
            // call_state_snapshot 会在更新批次基数前释放调用映射借用。
            let call_state_snapshot =
                self.tool_calls.get(&resolved_tool_call_id).ok_or_else(|| {
                    RuntimeLogReplayError::UnknownMatchedToolCall {
                        message_id: event.message_id.clone(),
                        tool_call_id: resolved_tool_call_id.clone(),
                    }
                })?;
            let call_batch_id = call_state_snapshot.tool_batch_id.clone();
            matched_provider_visible_tool_call_id =
                Some(call_state_snapshot.provider_visible_tool_call_id.clone());
            if call_state_snapshot.matched {
                return Err(RuntimeLogReplayError::DuplicateMatchedToolResult {
                    message_id: event.message_id.clone(),
                    tool_call_id: resolved_tool_call_id,
                });
            }
            if let Some(actual_batch_id) = tool_batch_id.as_deref()
                && actual_batch_id != call_batch_id
            {
                return Err(RuntimeLogReplayError::ToolResultBatchMismatch {
                    message_id: event.message_id.clone(),
                    expected_batch_id: call_batch_id,
                    actual_batch_id: actual_batch_id.to_string(),
                });
            }
            let batch = self.tool_batches.get_mut(&call_batch_id).ok_or_else(|| {
                RuntimeLogReplayError::MissingToolBatchStart {
                    tool_call_id: resolved_tool_call_id.clone(),
                    tool_batch_id: call_batch_id.clone(),
                }
            })?;
            batch.tool_result_count = batch.tool_result_count.checked_add(1).ok_or(
                RuntimeLogReplayError::ReplayToolBatchCardinalityOverflow {
                    tool_batch_id: call_batch_id,
                },
            )?;
            self.tool_calls
                .get_mut(&resolved_tool_call_id)
                .ok_or_else(|| RuntimeLogReplayError::UnknownMatchedToolCall {
                    message_id: event.message_id.clone(),
                    tool_call_id: resolved_tool_call_id,
                })?
                .matched = true;
        }
        // result_message preserves normalized explicit identity before status-specific handling.
        // result_message 会在按状态处理前保留规范化的明确身份。
        let result_message = SessionConversationMessage {
            id: event.message_id.clone(),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: if event.status == RuntimeToolResultStatus::Orphan {
                    provider_tool_call_id.or(runtime_tool_call_id)
                } else {
                    matched_provider_visible_tool_call_id
                },
                is_error: event.status == RuntimeToolResultStatus::Failed,
                text: event.text.clone(),
            },
        };
        if event.status == RuntimeToolResultStatus::Orphan {
            // Explicit orphans become text immediately so later normalization cannot rematch them.
            // 明确孤儿结果会立即转为文本，使后续归一化无法重新匹配它们。
            self.messages
                .push(historical_tool_result_text_message(&result_message));
        } else {
            self.messages.push(result_message);
        }
        Ok(())
    }

    /// Validates and registers one conversation-wide protocol-message identifier.
    /// 验证并注册一个会话范围内的协议消息标识。
    ///
    /// # Parameters
    /// 参数
    /// - `message_kind`: Stable runtime event kind used only for actionable diagnostics.
    /// - `message_kind`：仅用于可操作诊断的稳定运行时事件类型。
    /// - `message_id`: Exact local protocol-message identifier declared by the event.
    /// - `message_id`：事件声明的精确本地协议消息标识。
    ///
    /// # Returns
    /// 返回值
    /// Success after registration, or the exact missing, non-canonical, or duplicate failure.
    /// 注册后返回成功，或返回精确的缺失、非规范或重复失败。
    fn register_message_identifier(
        &mut self,
        message_kind: &'static str,
        message_id: &str,
    ) -> Result<(), RuntimeLogReplayError> {
        register_canonical_message_identifier(&mut self.message_ids, message_id).map_err(|error| {
            match error {
                CanonicalMessageIdentifierError::Missing => {
                    RuntimeLogReplayError::MissingMessageIdentifier { message_kind }
                }
                CanonicalMessageIdentifierError::NonCanonical { identifier } => {
                    RuntimeLogReplayError::NonCanonicalMessageIdentifier {
                        message_kind,
                        message_id: identifier,
                    }
                }
                CanonicalMessageIdentifierError::Duplicate { identifier } => {
                    RuntimeLogReplayError::DuplicateMessageIdentifier {
                        message_id: identifier,
                    }
                }
            }
        })
    }
}

/// Tracks the replay relationship state for one unique runtime tool call.
/// 跟踪一个唯一运行时工具调用的重放关系状态。
#[derive(Debug)]
struct RuntimeReplayToolCallState {
    /// Runtime batch declared by the owning tool-call event.
    /// 所属工具调用事件声明的运行时批次。
    tool_batch_id: String,
    /// Stable call id used by every model-facing protocol encoder.
    /// 所有面向模型的协议编码器共同使用的稳定调用标识。
    provider_visible_tool_call_id: String,
    /// Whether one completed or failed result already consumed this call.
    /// 是否已有一条完成或失败结果消费了当前调用。
    matched: bool,
}

/// Tracks one replayed tool batch from its start event through an optional completion event.
/// 跟踪一个被重放工具批次从开始事件到可选完成事件的状态。
#[derive(Debug)]
struct RuntimeReplayToolBatchState {
    /// Assistant message declared by the batch-start event.
    /// 批次开始事件声明的助手消息。
    assistant_message_id: String,
    /// Exact number of unique tool calls observed for the batch.
    /// 为该批次观察到的唯一工具调用精确数量。
    tool_call_count: usize,
    /// Exact number of completed or failed results matched to the batch.
    /// 匹配到该批次的完成或失败结果精确数量。
    tool_result_count: usize,
    /// Whether a completion control event has already closed the batch.
    /// 是否已有完成控制事件关闭该批次。
    completed: bool,
}

/// Returns the stable diagnostic kind for one runtime replay event.
/// 返回一条运行时重放事件的稳定诊断类型。
///
/// # Parameters
/// 参数
/// - `event`: Runtime event being classified for phase-order diagnostics.
/// - `event`：为阶段顺序诊断进行分类的运行时事件。
///
/// # Returns
/// 返回值
/// A stable snake-case event kind used only in replay errors.
/// 返回仅用于重放错误的稳定蛇形事件类型。
fn runtime_replay_event_kind(event: &RuntimeLogEvent) -> &'static str {
    match event {
        RuntimeLogEvent::UserMessage(_) => "user_message",
        RuntimeLogEvent::AssistantMessage(_) => "assistant_message",
        RuntimeLogEvent::ToolBatchStarted(_) => "tool_batch_started",
        RuntimeLogEvent::ToolCall(_) => "tool_call",
        RuntimeLogEvent::ToolResult(_) => "tool_result",
        RuntimeLogEvent::ToolBatchCompleted(_) => "tool_batch_completed",
        RuntimeLogEvent::ProviderTurnCheckpoint(_) => "provider_turn_checkpoint",
        RuntimeLogEvent::ContextCompactionApplied(_) => "context_compaction_applied",
        RuntimeLogEvent::RuntimeRepair(_) => "runtime_repair",
        RuntimeLogEvent::InjectionApplied(_) => "injection_applied",
        RuntimeLogEvent::SystemUpdate(_) => "system_update",
        RuntimeLogEvent::InjectionCancelled(_) => "injection_cancelled",
        RuntimeLogEvent::InjectionMerged(_) => "injection_merged",
        RuntimeLogEvent::ProviderRequestCheckpoint(_) => "provider_request_checkpoint",
        RuntimeLogEvent::ProviderResponseCheckpoint(_) => "provider_response_checkpoint",
        RuntimeLogEvent::ProviderLoweringRepair(_) => "provider_lowering_repair",
        RuntimeLogEvent::VmmLifecycle(_) => "vmm_lifecycle",
    }
}

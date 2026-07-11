//! Conversion helpers between protocol state, transcript entries, and compaction sources.
//! 协议状态、转录条目与压缩来源之间的转换辅助逻辑。

use super::identity::{
    SessionConversationMessageIdentityError, SessionConversationRecordIdentityError,
    validate_session_conversation_message_append, validate_session_conversation_record_append,
};
use super::types::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall,
};
use crate::transcript::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_core::types::{ConversationAttachment, ConversationPayload};

/// Appends one fully structured canonical message after validating the active identity namespace.
/// 在校验活动身份命名空间后追加一条完整结构化规范消息。
///
/// # Parameters
/// 参数
/// `state` is the canonical state mutated only after the candidate identity is accepted.
/// `state` 是仅在候选身份被接受后才会修改的规范状态。
/// `message` is the complete canonical message committed without payload rewriting.
/// `message` 是不会改写载荷而直接提交的完整规范消息。
///
/// # Returns
/// 返回值
/// Returns success after atomic append, or the identity violation with `state` unchanged.
/// 原子追加后返回成功；身份违规时返回错误且 `state` 保持不变。
pub fn append_session_conversation_message(
    state: &mut SessionConversationState,
    message: SessionConversationMessage,
) -> Result<(), SessionConversationMessageIdentityError> {
    validate_session_conversation_message_append(&state.messages, &message.id)?;
    state.messages.push(message);
    Ok(())
}

/// Appends one user-authored text input to the canonical conversation state.
/// 向规范会话状态追加一条用户文本输入。
///
/// # Parameters
/// 参数
/// `state` is the canonical state mutated only after identity validation succeeds.
/// `state` 是仅在身份校验成功后才会修改的规范状态。
/// `message_id` is the exact stable identity proposed for the new message.
/// `message_id` 是为新消息提供的精确稳定身份。
/// `text` and `attachments` are the complete structured user payload.
/// `text` 与 `attachments` 是完整的结构化用户载荷。
///
/// # Returns
/// 返回值
/// Returns success after atomic append, or the identity violation with `state` unchanged.
/// 原子追加后返回成功；身份违规时返回错误且 `state` 保持不变。
pub fn append_user_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
    attachments: Vec<ConversationAttachment>,
) -> Result<(), SessionConversationMessageIdentityError> {
    // message_id is materialized once so validation and committed storage use the exact same identity.
    // message_id 只物化一次，确保校验与提交存储使用完全相同的身份。
    let message_id = message_id.into();
    append_session_conversation_message(
        state,
        SessionConversationMessage {
            id: message_id,
            payload: SessionConversationPayload::UserText {
                text: text.into(),
                attachments,
            },
        },
    )
}

/// Appends one assistant-authored text node used by hidden explore or summary records.
/// 追加一条供隐藏 Explore 或 Summary 记录使用的助手文本节点。
///
/// # Parameters
/// 参数
/// `state` is the canonical state mutated only after identity validation succeeds.
/// `state` 是仅在身份校验成功后才会修改的规范状态。
/// `message_id` is the exact stable identity proposed for the new message.
/// `message_id` 是为新消息提供的精确稳定身份。
/// `text` is the assistant-authored text stored without reasoning or tool calls.
/// `text` 是不带 reasoning 或工具调用保存的助手文本。
///
/// # Returns
/// 返回值
/// Returns success after atomic append, or the identity violation with `state` unchanged.
/// 原子追加后返回成功；身份违规时返回错误且 `state` 保持不变。
pub fn append_assistant_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
) -> Result<(), SessionConversationMessageIdentityError> {
    // message_id is materialized once so validation and committed storage use the exact same identity.
    // message_id 只物化一次，确保校验与提交存储使用完全相同的身份。
    let message_id = message_id.into();
    append_session_conversation_message(
        state,
        SessionConversationMessage {
            id: message_id,
            payload: SessionConversationPayload::Assistant {
                text: text.into(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        },
    )
}

/// Appends one canonical session record after validating the complete record identity namespace.
/// 在校验完整记录身份命名空间后追加一条规范会话记录。
///
/// # Parameters
/// 参数
/// `state` is the canonical state mutated only after the record identity is accepted.
/// `state` 是仅在记录身份被接受后才会修改的规范状态。
/// `record` is the complete durable record committed without payload rewriting.
/// `record` 是不会改写载荷而直接提交的完整持久记录。
///
/// # Returns
/// 返回值
/// Returns success after atomic append, or the identity violation with `state` unchanged.
/// 原子追加后返回成功；身份违规时返回错误且 `state` 保持不变。
pub fn append_record(
    state: &mut SessionConversationState,
    record: SessionConversationRecord,
) -> Result<(), SessionConversationRecordIdentityError> {
    validate_session_conversation_record_append(&state.records, &record.id)?;
    state.records.push(record);
    Ok(())
}

/// Collects one latest-first unique attachment list from the canonical conversation window for host-side capability tools.
/// 从规范会话窗口中收集一份按最新优先去重后的附件列表，供宿主侧能力工具使用。
pub fn collect_host_visible_attachments(
    state: &SessionConversationState,
) -> Vec<ConversationAttachment> {
    let mut seen_keys = BTreeSet::new();
    let mut attachments = Vec::new();

    for record in state.records.iter().rev() {
        let Some(metadata) = record.metadata.as_ref() else {
            continue;
        };
        for attachment in generated_attachments_from_tool_metadata(metadata) {
            let identity_key = attachment_identity_key(&attachment);
            if !seen_keys.insert(identity_key) {
                continue;
            }
            attachments.push(attachment);
        }
    }

    for message in state.messages.iter().rev() {
        let SessionConversationPayload::UserText {
            attachments: message_attachments,
            ..
        } = &message.payload
        else {
            continue;
        };

        for attachment in message_attachments.iter().rev() {
            let identity_key = attachment_identity_key(attachment);
            if !seen_keys.insert(identity_key) {
                continue;
            }
            attachments.push(attachment.clone());
        }
    }

    attachments
}

/// Appends one streamed body delta to the matching canonical session record and returns whether it was found.
/// 向匹配的规范会话记录追加一段流式正文增量，并返回是否命中。
pub fn append_record_body_delta(
    state: &mut SessionConversationState,
    record_id: &str,
    delta: &str,
) -> bool {
    let record = state
        .records
        .iter_mut()
        .find(|record| record.id == record_id);
    let Some(record) = record else {
        return false;
    };
    record.body.push_str(delta);
    true
}

/// Rebuilds a canonical state from a bounded transcript window when only lazy-loaded rows are available.
/// 当只有懒加载转录行可用时，根据有界转录窗口重建规范会话状态。
///
/// # Returns
/// 返回值
/// Returns the rebuilt canonical state, or an identity error when replayed rows conflict.
/// 返回重建后的规范状态；当复放记录身份冲突时返回错误。
pub fn conversation_state_from_session_messages(
    messages: &[SessionMessageEntry],
) -> Result<SessionConversationState, String> {
    let mut state = SessionConversationState::default();
    let mut protocol_messages = Vec::new();
    for message in messages {
        append_record(
            &mut state,
            SessionConversationRecord {
                id: message.id.clone(),
                kind: message.kind,
                record_kind: message.record_kind,
                visible: message.visible,
                title: message.title.clone(),
                body: message.body.clone(),
                metadata: message.metadata.clone(),
                created_at: message.created_at.clone(),
            },
        )
        .map_err(|error| format!("Invalid replayed conversation record identity: {error}"))?;

        if let Some(protocol_message) = session_message_to_protocol_message(message) {
            protocol_messages.push(protocol_message);
        }
    }
    state.messages = normalize_replayed_protocol_messages(protocol_messages);
    super::identity::validate_session_conversation_message_identities(&state.messages)
        .map_err(|error| format!("Invalid replayed conversation message identity: {error}"))?;
    Ok(state)
}

/// Normalizes replayed protocol messages so display-only tool rows cannot create dangling provider state.
/// 归一化重放得到的协议消息，避免仅用于展示的工具行生成悬空 Provider 状态。
pub(crate) fn normalize_replayed_protocol_messages(
    messages: Vec<SessionConversationMessage>,
) -> Vec<SessionConversationMessage> {
    let mut normalized = Vec::with_capacity(messages.len());
    let mut index = 0usize;
    while index < messages.len() {
        match &messages[index].payload {
            SessionConversationPayload::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
                let (assistant_end, merged_assistant) =
                    collect_adjacent_tool_call_assistant_messages(&messages, index);
                let (result_end, matched_results, complete) =
                    collect_matching_tool_results(&messages, assistant_end, &merged_assistant);
                if complete {
                    normalized.push(merged_assistant);
                    normalized.extend(matched_results);
                    index = result_end;
                } else {
                    for message in &messages[index..assistant_end] {
                        normalized.push(historical_tool_call_text_message(message));
                    }
                    index = assistant_end;
                }
            }
            SessionConversationPayload::ToolResult { .. } => {
                normalized.push(historical_tool_result_text_message(&messages[index]));
                index += 1;
            }
            _ => {
                normalized.push(messages[index].clone());
                index += 1;
            }
        }
    }
    normalized
}

/// Collects adjacent assistant tool-call rows into one replayable assistant batch.
/// 将相邻助手工具调用行收集为一个可重放的助手批次。
fn collect_adjacent_tool_call_assistant_messages(
    messages: &[SessionConversationMessage],
    start_index: usize,
) -> (usize, SessionConversationMessage) {
    let mut cursor = start_index;
    let mut text_parts = Vec::new();
    let mut reasoning_parts = Vec::new();
    let mut provider_message_id = None;
    let mut provider_message_state = None;
    let mut usage = None;
    let mut tool_calls = Vec::new();
    while cursor < messages.len() {
        let SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id: message_provider_id,
            provider_message_state: message_provider_state,
            usage: message_usage,
            tool_calls: message_tool_calls,
        } = &messages[cursor].payload
        else {
            break;
        };
        if message_tool_calls.is_empty() {
            break;
        }
        push_non_empty_text_part(&mut text_parts, text);
        push_non_empty_text_part(&mut reasoning_parts, reasoning);
        if provider_message_id.is_none() {
            provider_message_id = message_provider_id.clone();
        }
        if provider_message_state.is_none() {
            provider_message_state = message_provider_state.clone();
        }
        if usage.is_none() {
            usage = message_usage.clone();
        }
        tool_calls.extend(message_tool_calls.clone());
        cursor += 1;
    }
    let id = messages
        .get(start_index)
        .map(|message| message.id.clone())
        .unwrap_or_default();
    (
        cursor,
        SessionConversationMessage {
            id,
            payload: SessionConversationPayload::Assistant {
                text: text_parts.join("\n\n"),
                reasoning: reasoning_parts.join("\n\n"),
                provider_message_id,
                provider_message_state,
                usage,
                tool_calls,
            },
        },
    )
}

/// Collects the contiguous tool results that complete one assistant tool batch.
/// 收集能完整闭合一个助手工具批次的连续工具结果。
fn collect_matching_tool_results(
    messages: &[SessionConversationMessage],
    start_index: usize,
    assistant: &SessionConversationMessage,
) -> (usize, Vec<SessionConversationMessage>, bool) {
    let SessionConversationPayload::Assistant { tool_calls, .. } = &assistant.payload else {
        return (start_index, Vec::new(), false);
    };
    let mut pending_groups = tool_call_identifier_groups(tool_calls);
    if pending_groups.is_empty() {
        return (start_index, Vec::new(), false);
    }
    let mut cursor = start_index;
    let mut results = Vec::new();
    while cursor < messages.len() && !pending_groups.is_empty() {
        let SessionConversationPayload::ToolResult { tool_call_id, .. } = &messages[cursor].payload
        else {
            break;
        };
        let Some(tool_call_key) = tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            break;
        };
        let Some(group_index) = pending_groups.iter().position(|identifiers| {
            identifiers
                .iter()
                .any(|identifier| identifier == tool_call_key)
        }) else {
            break;
        };
        pending_groups.remove(group_index);
        results.push(messages[cursor].clone());
        cursor += 1;
    }
    let complete = pending_groups.is_empty();
    (cursor, results, complete)
}

/// Appends one non-empty text fragment to a replay merge buffer.
/// 向重放合并缓冲区追加一段非空文本。
fn push_non_empty_text_part(parts: &mut Vec<String>, text: &str) {
    let text = text.trim();
    if !text.is_empty() {
        parts.push(text.to_string());
    }
}

/// Builds grouped identifiers for matching local and provider tool-call ids.
/// 构建分组标识，用于同时匹配本地与 Provider 工具调用 id。
fn tool_call_identifier_groups(tool_calls: &[SessionConversationToolCall]) -> Vec<Vec<String>> {
    tool_calls
        .iter()
        .map(tool_call_identifiers)
        .filter(|identifiers| !identifiers.is_empty())
        .collect()
}

/// Builds every replay identifier for one tool call.
/// 构建单个工具调用的所有可重放标识。
fn tool_call_identifiers(tool_call: &SessionConversationToolCall) -> Vec<String> {
    let mut identifiers = Vec::new();
    push_identifier(&mut identifiers, &tool_call.id);
    if let Some(tool_call_id) = tool_call.tool_call_id.as_deref() {
        push_identifier(&mut identifiers, tool_call_id);
    }
    identifiers
}

/// Adds one trimmed identifier if it has not already been collected.
/// 当标识非空且尚未收集时追加该裁剪后的标识。
fn push_identifier(identifiers: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() && !identifiers.iter().any(|identifier| identifier == value) {
        identifiers.push(value.to_string());
    }
}

/// Converts an unsafe replayed tool-call message into historical plain text.
/// 将不安全的重放工具调用消息转换为历史纯文本。
fn historical_tool_call_text_message(
    message: &SessionConversationMessage,
) -> SessionConversationMessage {
    let text = match &message.payload {
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            ..
        } => historical_tool_call_text(text, reasoning, tool_calls),
        _ => String::new(),
    };
    SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::UserText {
            text,
            attachments: Vec::new(),
        },
    }
}

/// Converts an unsafe replayed tool-result message into historical plain text.
/// 将不安全的重放工具结果消息转换为历史纯文本。
///
/// # Parameters
/// 参数
/// - `message`: Canonical tool-result message that must no longer participate in matching.
/// - `message`：不得再参与匹配的规范工具结果消息。
///
/// # Returns
/// 返回值
/// A user-text message preserving local message id, optional call id, and model-visible text.
/// 返回保留本地消息标识、可选调用标识与模型可见文本的用户文本消息。
pub(crate) fn historical_tool_result_text_message(
    message: &SessionConversationMessage,
) -> SessionConversationMessage {
    let text = match &message.payload {
        SessionConversationPayload::ToolResult { tool_call_id, text } => {
            let mut parts = Vec::new();
            parts.push(String::from(
                "[Historical tool result recovered as text only]",
            ));
            parts.push(format!("message_id: {}", message.id));
            if let Some(tool_call_id) = tool_call_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                parts.push(format!("tool_call_id: {tool_call_id}"));
            }
            if !text.trim().is_empty() {
                parts.push(text.clone());
            }
            parts.join("\n\n")
        }
        _ => String::new(),
    };
    SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::UserText {
            text,
            attachments: Vec::new(),
        },
    }
}

/// Builds plain historical text for a replayed tool-call batch that cannot safely remain live.
/// 为无法安全保留为实时状态的重放工具调用批次构建历史纯文本。
fn historical_tool_call_text(
    text: &str,
    reasoning: &str,
    tool_calls: &[SessionConversationToolCall],
) -> String {
    let mut parts = Vec::new();
    parts.push(String::from(
        "[Historical tool call recovered as text only]",
    ));
    push_labeled_non_empty_part(&mut parts, "assistant_text", text);
    push_labeled_non_empty_part(&mut parts, "reasoning", reasoning);
    for tool_call in tool_calls {
        let call_id = tool_call
            .tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(tool_call.id.as_str());
        parts.push(format!("tool: {} ({call_id})", tool_call.tool_name));
        if !tool_call.arguments.is_null() {
            parts.push(format!("arguments: {}", tool_call.arguments));
        }
    }
    parts.join("\n\n")
}

/// Adds one labeled text part when the source content is not empty.
/// 当源内容非空时追加一段带标签文本。
fn push_labeled_non_empty_part(parts: &mut Vec<String>, label: &str, text: &str) {
    let text = text.trim();
    if !text.is_empty() {
        parts.push(format!("{label}: {text}"));
    }
}

/// Projects canonical session records into the persisted transcript model used by the existing WebUI surfaces.
/// 将规范会话记录投影为现有WebUI表面使用的持久化转录模型。
pub fn conversation_state_to_session_messages(
    state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    state
        .records
        .iter()
        .cloned()
        .map(conversation_record_to_session_message)
        .collect()
}

/// Projects provider-visible runtime messages for compaction planning.
/// 为压缩规划投影 Provider 可见的运行态消息。
///
/// # Parameters
/// 参数
/// `state` is the canonical conversation state owned by the session runtime.
/// `state` 是会话运行时持有的规范会话状态。
///
/// # Returns
/// 返回值
/// Returns entries derived only from `state.messages`; display records are intentionally ignored.
/// 仅返回从 `state.messages` 派生的条目；显示记录会被有意忽略。
pub fn conversation_state_to_compaction_source_messages(
    state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    state
        .messages
        .iter()
        .map(conversation_message_to_compaction_source_message)
        .collect()
}

/// Converts one provider-visible protocol message into a compaction-only transcript entry.
/// 将一条 Provider 可见协议消息转换为仅供压缩使用的转录条目。
fn conversation_message_to_compaction_source_message(
    message: &SessionConversationMessage,
) -> SessionMessageEntry {
    match &message.payload {
        SessionConversationPayload::UserText { text, attachments } => {
            compaction_user_text_message(message.id.clone(), text, attachments)
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            provider_message_id,
            ..
        } => compaction_assistant_message(
            message.id.clone(),
            text,
            reasoning,
            tool_calls,
            provider_message_id.as_deref(),
        ),
        SessionConversationPayload::ToolResult { tool_call_id, text } => {
            compaction_tool_result_message(message.id.clone(), tool_call_id.as_deref(), text)
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
            compaction_user_text_message(message.id.clone(), &text, &[])
        }
    }
}

/// Builds one compaction-only user transcript entry from canonical user text.
/// 根据规范用户文本构建一条仅供压缩使用的用户转录条目。
fn compaction_user_text_message(
    id: String,
    text: &str,
    attachments: &[ConversationAttachment],
) -> SessionMessageEntry {
    let mut parts = Vec::new();
    let text = text.trim();
    if !text.is_empty() {
        parts.push(text.to_string());
    }
    if !attachments.is_empty() {
        parts.push(compaction_attachment_list_text(attachments));
    }
    SessionMessageEntry {
        id,
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("USER"),
        body: parts.join("\n\n"),
        metadata: None,
        created_at: String::new(),
    }
}

/// Builds one compaction-only assistant transcript entry from canonical assistant output.
/// 根据规范助手输出构建一条仅供压缩使用的助手转录条目。
fn compaction_assistant_message(
    id: String,
    text: &str,
    reasoning: &str,
    tool_calls: &[SessionConversationToolCall],
    provider_message_id: Option<&str>,
) -> SessionMessageEntry {
    let body = if tool_calls.is_empty() {
        let mut parts = Vec::new();
        push_non_empty_text_part(&mut parts, text);
        push_labeled_non_empty_part(&mut parts, "reasoning", reasoning);
        parts.join("\n\n")
    } else {
        historical_tool_call_text(text, reasoning, tool_calls)
    };
    SessionMessageEntry {
        id,
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body,
        metadata: compaction_provider_message_metadata(provider_message_id),
        created_at: String::new(),
    }
}

/// Builds compact compaction metadata for stable provider API-round grouping.
/// 为稳定的 Provider API 执行轮次分组构建紧凑压缩元数据。
fn compaction_provider_message_metadata(provider_message_id: Option<&str>) -> Option<Value> {
    let provider_message_id = provider_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        String::from("provider_message_id"),
        Value::String(provider_message_id.to_string()),
    );
    Some(Value::Object(metadata))
}

/// Builds one compaction-only tool-result transcript entry from canonical tool output.
/// 根据规范工具输出构建一条仅供压缩使用的工具结果转录条目。
fn compaction_tool_result_message(
    id: String,
    tool_call_id: Option<&str>,
    text: &str,
) -> SessionMessageEntry {
    let mut parts = Vec::new();
    if let Some(tool_call_id) = tool_call_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("tool_call_id: {tool_call_id}"));
    }
    push_non_empty_text_part(&mut parts, text);
    SessionMessageEntry {
        id,
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("TOOL RESULT"),
        body: parts.join("\n\n"),
        metadata: None,
        created_at: String::new(),
    }
}

/// Builds the compact attachment list appended to compaction-only user text.
/// 构建追加到仅供压缩使用的用户文本后的紧凑附件列表。
fn compaction_attachment_list_text(attachments: &[ConversationAttachment]) -> String {
    let mut lines = vec![String::from("[Attachments]")];
    for attachment in attachments {
        let mut parts = vec![attachment.kind.display_label().to_string()];
        if !attachment.file_name.trim().is_empty() {
            parts.push(attachment.file_name.clone());
        }
        if !attachment.local_path.trim().is_empty() {
            parts.push(attachment.local_path.clone());
        }
        lines.push(format!("- {}", parts.join(": ")));
    }
    lines.join("\n")
}

/// Merges canonical display records into an existing transcript window without dropping older display rows.
/// 将规范显示记录合并进既有转录窗口，同时不丢弃更早的显示行。
///
/// # Parameters
/// 参数
/// `existing_messages` is the current WebUI display transcript window.
/// `existing_messages` 是当前 WebUI 显示层转录窗口。
/// `state` is the runtime dialogue state whose visible records should update the display window.
/// `state` 是其可见记录需要更新显示窗口的运行态对话状态。
///
/// # Returns
/// 返回值
/// A transcript window that preserves unloaded history while treating runtime record order as canonical from the first shared row.
/// 返回保留运行态未覆盖历史、并从首个共享行开始以运行态记录顺序为准的转录窗口。
pub fn merge_session_messages_with_conversation_state_records(
    existing_messages: &[SessionMessageEntry],
    state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    // projected_messages preserves the canonical order of runtime conversation records.
    // projected_messages 保留运行态会话记录的规范顺序。
    let projected_messages = conversation_state_to_session_messages(state);
    if projected_messages.is_empty() {
        return existing_messages.to_vec();
    }
    if existing_messages.is_empty() {
        return projected_messages;
    }

    // projected_ids identifies rows owned by the runtime projection so stale display copies can be removed.
    // projected_ids 标识由运行态投影拥有的行，用于移除显示层中的旧副本。
    let projected_ids = projected_messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<BTreeSet<_>>();
    // first_shared_index anchors the runtime-owned segment inside the larger loaded transcript window.
    // first_shared_index 将运行态拥有的片段锚定在更大的已加载转录窗口中。
    let Some(first_shared_index) = existing_messages
        .iter()
        .position(|message| projected_ids.contains(message.id.as_str()))
    else {
        // merged_messages appends a disjoint runtime segment after loaded history.
        // merged_messages 将没有共享锚点的运行态片段追加到已加载历史之后。
        let mut merged_messages = existing_messages.to_vec();
        merged_messages.extend(projected_messages);
        return merged_messages;
    };

    // merged_messages keeps history before the runtime anchor, then emits canonical runtime order.
    // merged_messages 保留运行态锚点之前的历史，然后写入规范运行态顺序。
    let mut merged_messages = Vec::with_capacity(
        existing_messages
            .len()
            .saturating_add(projected_messages.len()),
    );
    merged_messages.extend_from_slice(&existing_messages[..first_shared_index]);
    merged_messages.extend(projected_messages);
    // trailing_unowned_messages preserves display-only rows that the runtime state does not own.
    // trailing_unowned_messages 保留运行态不拥有的显示层尾部记录。
    merged_messages.extend(
        existing_messages[first_shared_index..]
            .iter()
            .filter(|message| !projected_ids.contains(message.id.as_str()))
            .cloned(),
    );
    merged_messages
}

/// Projects the context-participating canonical records from the current anchor forward into transcript entries.
/// 将从当前锚点开始参与上下文的规范记录投影为转录条目。
pub fn conversation_state_to_context_records(
    state: &SessionConversationState,
    context_start_message_id: Option<&str>,
) -> Vec<SessionMessageEntry> {
    let start_index = context_start_message_id
        .and_then(|message_id| {
            state
                .records
                .iter()
                .position(|record| record.id == message_id)
        })
        .unwrap_or(0);

    state.records[start_index..]
        .iter()
        .cloned()
        .map(conversation_record_to_session_message)
        .filter(|message| message.contributes_to_context())
        .collect()
}

/// Converts one canonical session record back into the persisted transcript entry shape expected by the current WebUI layer.
/// 将单条规范会话记录重新转换为当前WebUI层期望的持久化转录条目形状。
fn conversation_record_to_session_message(
    record: SessionConversationRecord,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: record.id,
        kind: record.kind,
        record_kind: record.record_kind,
        visible: record.visible,
        title: record.title,
        body: record.body,
        metadata: record.metadata,
        created_at: record.created_at,
    }
}

/// Converts one transcript entry into a provider-facing protocol message when it is safe to replay.
/// 在安全可重放时将一条转录项转换为面向 Provider 的协议消息。
fn session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    match message.kind {
        SessionMessageKind::User if message.contributes_to_context() => {
            Some(SessionConversationMessage {
                id: message.id.clone(),
                payload: SessionConversationPayload::UserText {
                    text: visible_user_message_canonical_text(message),
                    attachments: visible_user_message_transcript_attachments(message),
                },
            })
        }
        SessionMessageKind::Assistant
            if assistant_record_can_seed_protocol(&message.record_kind) =>
        {
            Some(SessionConversationMessage {
                id: message.id.clone(),
                payload: SessionConversationPayload::Assistant {
                    text: message.body.clone(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            })
        }
        _ if message.contributes_to_context()
            && matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
            ) =>
        {
            tool_call_session_message_to_protocol_message(message)
        }
        _ if message.contributes_to_context()
            && matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolResultOutput
            ) =>
        {
            tool_result_session_message_to_protocol_message(message)
        }
        _ => None,
    }
}

/// Rebuilds one canonical assistant tool-call message from a persisted transcript row when the full conversation-state JSON is not available.
/// 当完整会话状态 JSON 不可用时，从一条持久化转录行重建一条规范助手工具调用消息。
fn tool_call_session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    let metadata = message.metadata.as_ref()?;
    let tool_name = metadata.get("tool_name").and_then(Value::as_str)?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let arguments = metadata
        .get("arguments")
        .cloned()
        .or_else(|| serde_json::from_str::<Value>(message.body.trim()).ok())
        .unwrap_or(Value::Object(serde_json::Map::new()));
    let tool_call_id = metadata
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Some(SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: vec![SessionConversationToolCall {
                id: tool_call_id.clone().unwrap_or_else(|| message.id.clone()),
                tool_call_id,
                tool_name: tool_name.to_string(),
                arguments,
            }],
        },
    })
}

/// Rebuilds one canonical tool-result message from a persisted transcript row when the full conversation-state JSON is not available.
/// 当完整会话状态 JSON 不可用时，从一条持久化转录行重建一条规范工具结果消息。
fn tool_result_session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    let metadata = message.metadata.as_ref();
    let tool_call_id = metadata
        .and_then(|payload| payload.get("tool_call_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::ToolResult {
            tool_call_id,
            text: message.body.clone(),
        },
    })
}

/// Resolves the canonical user-text payload stored for one visible user transcript message, falling back to the rendered body for older rows.
/// 解析一条可见用户转录消息保存的规范用户文本载荷；若旧记录未保存该字段，则回退到渲染正文。
fn visible_user_message_canonical_text(message: &SessionMessageEntry) -> String {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("canonical_text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| message.body.clone())
}

/// Resolves the structured attachment list stored on one visible user transcript message, falling back to an empty list for older rows.
/// 解析一条可见用户转录消息保存的结构化附件列表；若旧记录未保存附件，则回退为空集合。
fn visible_user_message_transcript_attachments(
    message: &SessionMessageEntry,
) -> Vec<ConversationAttachment> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("attachments"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ConversationAttachment>>(value).ok())
        .unwrap_or_default()
}

/// Returns whether one assistant-side transcript record can seed a canonical assistant message when reconstructing the protocol state from transcript rows alone.
/// 返回当仅依赖转录行重建协议状态时，某条助手侧转录记录是否可以作为规范助手消息的种子。
fn assistant_record_can_seed_protocol(record_kind: &SessionMessageRecordKind) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::AssistantOutput
            | SessionMessageRecordKind::CompactionSummary
            | SessionMessageRecordKind::Transcript
    )
}

/// Builds one stable attachment identity key that prefers asset identifiers and falls back to modality, file name, and local path.
/// 构建单条稳定附件身份键，优先使用资产标识，缺失时退回到模态、文件名与本地路径的组合。
fn attachment_identity_key(attachment: &ConversationAttachment) -> String {
    attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{}|{}|{}",
                attachment.kind.modality_name(),
                attachment.file_name,
                attachment.local_path
            )
        })
}

/// Extracts generated attachments from one tool-result metadata object when that record carries a generated-attachments host result.
/// 当某条工具结果元数据携带 generated-attachments 宿主结果时，从中提取生成附件集合。
fn generated_attachments_from_tool_metadata(metadata: &Value) -> Vec<ConversationAttachment> {
    metadata
        .as_object()
        .and_then(|_| resolve_generated_attachment_host_result_from_metadata(metadata))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("outputs"))
        .cloned()
        .and_then(|outputs| serde_json::from_value::<Vec<ConversationAttachment>>(outputs).ok())
        .unwrap_or_default()
}

/// Resolves the generated-attachments host-result payload from one record metadata object.
/// 从单条记录元数据对象中解析 generated-attachments 宿主结果载荷。
fn resolve_generated_attachment_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    metadata
        .get("host_result")
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("generated_attachments"))
        .or_else(|| {
            (metadata.get("kind").and_then(Value::as_str) == Some("generated_attachments"))
                .then_some(metadata)
        })
}

//! Canonical conversation protocol models shared by WebUI state, persistence, and runtime bridges.
//! 供WebUI状态、持久化与运行时桥接共享的规范会话协议模型。

use crate::transcript::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};

/// Session-scoped canonical conversation state persisted independently from the visible transcript.
/// 独立于可见转录流持久化保存的按会话归属规范会话状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConversationState {
    /// Active canonical message window that will be forwarded to the next runtime call.
    /// 将被转发给下一次运行时调用的活动规范消息窗口。
    pub messages: Vec<SessionConversationMessage>,
    /// Canonical session record stream retained as the single source for transcript and hidden-node projection.
    /// 作为转录与隐藏节点投影唯一真相源保留的规范会话记录流。
    #[serde(default)]
    pub records: Vec<SessionConversationRecord>,
    /// Provider-scoped continuation cursor snapshot prepared for future request chaining.
    /// 为未来请求串联准备的按 Provider 归属 continuation 游标快照。
    pub continuation: SessionProviderContinuationState,
}

/// Provider-scoped continuation cursor retained beside the canonical message window.
/// 与规范消息窗口并存保存的按 Provider 归属 continuation 游标。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionProviderContinuationState {
    /// Provider identifier that produced the latest canonical runtime result.
    /// 产出最近一次规范运行结果的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier that produced the latest canonical runtime result.
    /// 产出最近一次规范运行结果的模型标识。
    pub model_id: Option<String>,
    /// Latest provider-visible assistant message identifier returned by the upstream API.
    /// 上游接口返回的最近一次 Provider 可见助手消息标识。
    pub last_provider_message_id: Option<String>,
    /// Unix-millis text capturing when the continuation cursor was last refreshed.
    /// 捕获 continuation 游标最近刷新时间的 Unix 毫秒文本。
    pub updated_at: Option<String>,
}

/// Canonical conversation message stored inside the session protocol window.
/// 存储在会话协议窗口中的规范会话消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationMessage {
    /// Stable protocol message identifier owned by the local workbench.
    /// 由本地工作台持有的稳定协议消息标识。
    pub id: String,
    /// Structured canonical payload that can be losslessly projected back into provider-visible request history.
    /// 可以稳定投影回 Provider 可见请求历史的结构化规范载荷。
    pub payload: SessionConversationPayload,
}

/// Canonical session record stored independently from the UI transcript projection.
/// 独立于界面转录投影保存的规范会话记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationRecord {
    /// Stable record identifier shared by projection caches and compaction checkpoints.
    /// 被投影缓存与压缩检查点共享的稳定记录标识。
    pub id: String,
    /// High-level record role used by transcript surfaces.
    /// 被转录表面使用的高层记录角色。
    pub kind: SessionMessageKind,
    /// Durable record kind that separates visible transcript rows from hidden internal nodes.
    /// 用于区分可见转录行与隐藏内部节点的持久化记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Whether the record should be shown in the visible transcript projection.
    /// 该记录是否应显示在可见转录投影中。
    pub visible: bool,
    /// Optional title projected into the transcript card header.
    /// 投影到转录卡片头部的可选标题。
    pub title: String,
    /// Primary body payload projected into transcript and modal surfaces.
    /// 被投影到转录与弹层表面的主要正文载荷。
    pub body: String,
    /// Optional structured metadata reserved for transcript-only enrichments and host-side audit data.
    /// 为仅限转录的增强信息与宿主侧审计数据保留的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis text captured when the record was created.
    /// 捕获该记录创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Structured canonical payload variants used by the local session protocol.
/// 本地会话协议使用的结构化规范载荷变体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionConversationPayload {
    /// Plain user-authored text input.
    /// 纯文本用户输入。
    UserText {
        /// Visible user text content.
        /// 用户可见文本内容。
        text: String,
        /// Structured attachments that should stay attached to the user-authored text.
        /// 应与用户文本保持绑定的结构化附件集合。
        #[serde(default)]
        attachments: Vec<ConversationAttachment>,
    },
    /// Assistant turn that may contain visible text, structured reasoning, and tool calls.
    /// 可能同时包含可见文本、结构化思考与工具调用的助手回合。
    Assistant {
        /// Visible assistant text content.
        /// 助手可见文本内容。
        text: String,
        /// Flattened reasoning text extracted from provider-native reasoning blocks.
        /// 从 Provider 原生 reasoning 块中提取的扁平化思考文本。
        reasoning: String,
        /// Optional provider-native assistant message identifier.
        /// 可选的 Provider 原生助手消息标识。
        provider_message_id: Option<String>,
        /// Optional provider-native assistant state preserved for future provider-specific history replays.
        /// 为未来 Provider 专属历史回放保留的可选 Provider 原生助手状态。
        provider_message_state: Option<Value>,
        /// Optional provider usage snapshot retained for host-side budget calibration.
        /// 为宿主侧预算校准保留的可选 Provider 用量快照。
        #[serde(default)]
        usage: Option<ProviderUsageSnapshot>,
        /// Structured tool-call list emitted by the assistant turn.
        /// 由该助手回合发出的结构化工具调用列表。
        tool_calls: Vec<SessionConversationToolCall>,
    },
    /// Tool-result payload returned to the model after one assistant tool call.
    /// 在一次助手工具调用后回传给模型的工具结果载荷。
    ToolResult {
        /// Stable tool-result identifier emitted by the provider or synthesized locally.
        /// 由 Provider 发出或在本地合成的稳定工具结果标识。
        tool_result_id: String,
        /// Optional provider-visible tool-call identifier linked to the result.
        /// 与该结果关联的可选 Provider 可见工具调用标识。
        tool_call_id: Option<String>,
        /// Flattened human-readable tool-result text.
        /// 已扁平化的人类可读工具结果文本。
        text: String,
    },
}

/// Structured assistant tool-call item captured inside the canonical session protocol.
/// 在规范会话协议内部捕获的结构化助手工具调用项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationToolCall {
    /// Stable tool-call item identifier emitted by the provider.
    /// 由 Provider 发出的稳定工具调用项标识。
    pub id: String,
    /// Optional provider-visible call identifier used by tool-result messages.
    /// 被工具结果消息使用的可选 Provider 可见调用标识。
    pub tool_call_id: Option<String>,
    /// Tool function name selected by the assistant.
    /// 助手选择的工具函数名称。
    pub tool_name: String,
    /// Structured JSON arguments selected by the assistant.
    /// 助手选择的结构化 JSON 参数。
    pub arguments: Value,
}

/// Appends one user-authored text input to the canonical conversation state.
/// 向规范会话状态追加一条用户文本输入。
pub fn append_user_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
    attachments: Vec<ConversationAttachment>,
) {
    state.messages.push(SessionConversationMessage {
        id: message_id.into(),
        payload: SessionConversationPayload::UserText {
            text: text.into(),
            attachments,
        },
    });
}

/// Appends one assistant-authored text node used by hidden explore or summary records.
/// 追加一条供隐藏 Explore 或 Summary 记录使用的助手文本节点。
pub fn append_assistant_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
) {
    state.messages.push(SessionConversationMessage {
        id: message_id.into(),
        payload: SessionConversationPayload::Assistant {
            text: text.into(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: Vec::new(),
        },
    });
}

/// Appends one canonical session record that later drives the visible transcript projection.
/// 追加一条后续将驱动可见转录投影的规范会话记录。
pub fn append_record(state: &mut SessionConversationState, record: SessionConversationRecord) {
    state.records.push(record);
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
pub fn conversation_state_from_session_messages(
    messages: &[SessionMessageEntry],
) -> SessionConversationState {
    let mut state = SessionConversationState::default();
    let mut protocol_messages = Vec::new();
    for message in messages {
        state.records.push(SessionConversationRecord {
            id: message.id.clone(),
            kind: message.kind,
            record_kind: message.record_kind,
            visible: message.visible,
            title: message.title.clone(),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            created_at: message.created_at.clone(),
        });

        if let Some(protocol_message) = session_message_to_protocol_message(message) {
            protocol_messages.push(protocol_message);
        }
    }
    state.messages = normalize_replayed_protocol_messages(protocol_messages);
    state
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
fn historical_tool_result_text_message(
    message: &SessionConversationMessage,
) -> SessionConversationMessage {
    let text = match &message.payload {
        SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => {
            let mut parts = Vec::new();
            parts.push(String::from(
                "[Historical tool result recovered as text only]",
            ));
            parts.push(format!("result_id: {tool_result_id}"));
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
/// A transcript window that preserves existing rows and upserts runtime-projected rows by id.
/// 返回按标识更新运行态投影行且保留既有行的转录窗口。
pub fn merge_session_messages_with_conversation_state_records(
    existing_messages: &[SessionMessageEntry],
    state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    let projected_messages = conversation_state_to_session_messages(state);
    if projected_messages.is_empty() {
        return existing_messages.to_vec();
    }
    if existing_messages.is_empty() {
        return projected_messages;
    }

    let mut merged_messages = existing_messages.to_vec();
    for projected_message in projected_messages {
        if let Some(existing_message) = merged_messages
            .iter_mut()
            .find(|message| message.id == projected_message.id)
        {
            *existing_message = projected_message;
        } else {
            merged_messages.push(projected_message);
        }
    }
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
    let tool_result_id = metadata
        .and_then(|payload| payload.get("tool_result_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            tool_call_id
                .as_deref()
                .map(|tool_call_id| format!("tool-result-{tool_call_id}"))
        })
        .unwrap_or_else(|| message.id.clone());

    Some(SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::ToolResult {
            tool_result_id,
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

#[cfg(test)]
mod tests {
    //! Protocol replay normalization regression tests.
    //! 协议重放归一化回归测试。

    use super::*;
    use serde_json::json;

    /// Builds one transcript entry for replay-normalization tests.
    /// 为重放归一化测试构建一条转录项。
    fn transcript_entry(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
        metadata: Option<Value>,
    ) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: String::new(),
            body: body.to_string(),
            metadata,
            created_at: String::from("1"),
        }
    }

    /// Builds metadata for one visible tool-call transcript row.
    /// 为一条可见工具调用转录行构建元数据。
    fn tool_call_metadata(tool_name: &str, tool_call_id: &str) -> Value {
        json!({
            "tool_name": tool_name,
            "tool_call_id": tool_call_id,
            "arguments": { "value": tool_call_id }
        })
    }

    /// Builds metadata for one visible tool-result transcript row.
    /// 为一条可见工具结果转录行构建元数据。
    fn tool_result_metadata(tool_name: &str, tool_call_id: &str) -> Value {
        json!({
            "tool_name": tool_name,
            "tool_call_id": tool_call_id,
            "tool_result_id": format!("tool-result-{tool_call_id}")
        })
    }

    /// Verifies display rows with parallel tool calls replay as one legal assistant batch.
    /// 验证展示层并行工具调用行会重放为一个合法助手批次。
    #[test]
    fn transcript_replay_groups_parallel_tool_rows_into_one_batch() {
        let messages = vec![
            transcript_entry(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "run tools",
                None,
            ),
            transcript_entry(
                "call-row-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "{}",
                Some(tool_call_metadata("first_tool", "call-1")),
            ),
            transcript_entry(
                "call-row-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "{}",
                Some(tool_call_metadata("second_tool", "call-2")),
            ),
            transcript_entry(
                "result-row-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "first result",
                Some(tool_result_metadata("first_tool", "call-1")),
            ),
            transcript_entry(
                "result-row-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "second result",
                Some(tool_result_metadata("second_tool", "call-2")),
            ),
        ];

        let state = conversation_state_from_session_messages(&messages);

        assert_eq!(state.messages.len(), 4);
        let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[1].payload
        else {
            panic!("second message should be a grouped assistant tool batch");
        };
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(tool_calls[1].tool_call_id.as_deref(), Some("call-2"));
        assert!(matches!(
            state.messages[2].payload,
            SessionConversationPayload::ToolResult { .. }
        ));
        assert!(matches!(
            state.messages[3].payload,
            SessionConversationPayload::ToolResult { .. }
        ));
    }

    /// Verifies interrupted tool calls replay as plain historical text before the next user turn.
    /// 验证被中断的工具调用会在下一条用户回合前重放为普通历史文本。
    #[test]
    fn transcript_replay_downgrades_interrupted_tool_row_to_text() {
        let messages = vec![
            transcript_entry(
                "call-row-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "{}",
                Some(tool_call_metadata("first_tool", "call-1")),
            ),
            transcript_entry(
                "user-2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "continue after interruption",
                None,
            ),
        ];

        let state = conversation_state_from_session_messages(&messages);

        assert_eq!(state.messages.len(), 2);
        let SessionConversationPayload::UserText { text, attachments } = &state.messages[0].payload
        else {
            panic!("interrupted tool call should be downgraded to user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("Historical tool call"));
        assert!(text.contains("call-1"));
        let SessionConversationPayload::UserText { text, .. } = &state.messages[1].payload else {
            panic!("next message should remain user text");
        };
        assert_eq!(text, "continue after interruption");
    }

    /// Verifies runtime display-record projection updates rows without dropping loaded history rows.
    /// 验证运行态显示记录投影会更新行，同时不会丢弃已加载的历史行。
    #[test]
    fn merge_session_messages_with_conversation_state_records_preserves_loaded_history() {
        // existing_messages simulates the display window loaded from raw segmented turns.
        // existing_messages 模拟从原始分段 turn 加载出的显示窗口。
        let existing_messages = vec![
            transcript_entry(
                "old-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "之前的问题",
                None,
            ),
            transcript_entry(
                "old-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::Transcript,
                "之前的回答",
                None,
            ),
        ];
        // runtime_state carries only the newly materialized visible record for the active turn.
        // runtime_state 只携带当前回合新物化的可见记录。
        let runtime_state = conversation_state_from_session_messages(&[transcript_entry(
            "new-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "继续提问",
            None,
        )]);

        let merged_messages = merge_session_messages_with_conversation_state_records(
            &existing_messages,
            &runtime_state,
        );

        assert_eq!(merged_messages.len(), 3);
        assert_eq!(merged_messages[0].body, "之前的问题");
        assert_eq!(merged_messages[1].body, "之前的回答");
        assert_eq!(merged_messages[2].body, "继续提问");
    }
}

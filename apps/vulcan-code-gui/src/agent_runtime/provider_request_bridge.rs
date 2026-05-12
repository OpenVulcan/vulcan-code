//! Shared provider-request bridge helpers reused by the executor, transport adapters, and regression tests.
//! 供执行器、传输适配器与回归测试复用的共享 Provider 请求桥接辅助逻辑。

use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    replace_messages_from_rig_history,
};
use reqwest::header::HeaderMap;
use rig::OneOrMany;
use rig::completion::ToolDefinition as RigToolDefinition;
use rig::message::{
    AssistantContent as RigAssistantContent, Message as RigMessage, ToolResultContent,
    UserContent as RigUserContent,
};
use serde_json::{Map as JsonMap, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationRecord as CoreConversationRecord, ConversationSnapshot as CoreConversationSnapshot,
    ConversationToolCall as CoreConversationToolCall,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    TranscriptRecordKind as CoreTranscriptRecordKind, TranscriptRole as CoreTranscriptRole,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolSourceKind};

/// Converts one session-owned canonical conversation state into the `vulcan-core` snapshot shape.
/// 将一份会话持有的规范会话状态转换为 `vulcan-core` 快照形状。
pub(crate) fn build_core_conversation_snapshot(
    conversation_state: &SessionConversationState,
) -> CoreConversationSnapshot {
    CoreConversationSnapshot {
        messages: conversation_state
            .messages
            .iter()
            .map(session_message_to_core_message)
            .collect(),
        records: conversation_state
            .records
            .iter()
            .map(session_record_to_core_record)
            .collect(),
        continuation: session_continuation_to_core_continuation(&conversation_state.continuation),
    }
}

/// Converts one resolved host tool definition into the `vulcan-core` request-tool schema shape.
/// 将一条已解析宿主工具定义转换为 `vulcan-core` 请求工具 schema 形状。
pub(crate) fn resolved_tool_definition_to_core_tool_schema(
    definition: &ResolvedToolDefinition,
) -> CoreProviderRequestToolSchema {
    CoreProviderRequestToolSchema {
        id: definition.descriptor.id.clone(),
        name: definition.descriptor.name.clone(),
        description: definition.descriptor.description.clone(),
        source_label: tool_source_label(definition.descriptor.source).to_string(),
        parameters_schema: definition.parameters_schema.clone(),
    }
}

/// Converts one resolved host tool definition into the low-level rig tool-definition shape used by explicit completion requests.
/// 将一条已解析宿主工具定义转换为显式 completion 请求使用的低层 rig 工具定义形状。
pub(crate) fn resolved_tool_definition_to_rig_tool_definition(
    definition: &ResolvedToolDefinition,
) -> RigToolDefinition {
    RigToolDefinition {
        name: definition.descriptor.id.clone(),
        description: definition.descriptor.description.clone(),
        parameters: definition.parameters_schema.clone(),
    }
}

/// Converts one `vulcan-core` snapshot back into the GUI-owned canonical session state without dropping durable records or continuation data.
/// 将一份 `vulcan-core` 快照转换回 GUI 持有的规范会话状态，同时不丢失持久化记录与 continuation 数据。
pub(crate) fn core_snapshot_to_session_conversation_state(
    snapshot: &CoreConversationSnapshot,
) -> SessionConversationState {
    SessionConversationState {
        messages: snapshot
            .messages
            .iter()
            .map(core_message_to_session_message)
            .collect(),
        records: snapshot
            .records
            .iter()
            .map(core_record_to_session_record)
            .collect(),
        continuation: core_continuation_to_session_continuation(&snapshot.continuation),
    }
}

/// Converts one `vulcan-core` canonical message slice into rig history without replaying reasoning text back to the upstream provider.
/// 将一段 `vulcan-core` 规范消息切片转换为 rig 历史，同时避免把思考文本反向回放给上游 Provider。
pub(crate) fn core_messages_to_rig_history(
    messages: &[CoreConversationMessage],
) -> Vec<RigMessage> {
    messages.iter().map(core_message_to_rig_message).collect()
}

/// Converts one `vulcan-core` canonical message into one rig message visible to the upstream provider.
/// 将一条 `vulcan-core` 规范消息转换为上游 Provider 可见的 rig 消息。
pub(crate) fn core_message_to_rig_message(message: &CoreConversationMessage) -> RigMessage {
    match &message.payload {
        CoreConversationPayload::UserText { text } => RigMessage::user(text.clone()),
        CoreConversationPayload::Assistant {
            text,
            reasoning: _,
            provider_message_id,
            tool_calls,
        } => {
            let mut contents = Vec::new();
            if !text.is_empty() {
                contents.push(RigAssistantContent::text(text.clone()));
            }
            contents.extend(
                tool_calls
                    .iter()
                    .map(core_tool_call_to_rig_assistant_content),
            );
            let content = OneOrMany::many(contents)
                .unwrap_or_else(|_| OneOrMany::one(RigAssistantContent::text(String::new())));
            RigMessage::Assistant {
                id: provider_message_id.clone(),
                content,
            }
        }
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => RigMessage::tool_result_with_call_id(
            tool_result_id.clone(),
            tool_call_id.clone(),
            text.clone(),
        ),
    }
}

/// Serializes one request-header map into plain JSON for request-snapshot persistence and inspection.
/// 将一份请求头映射序列化为普通 JSON，供请求快照持久化与检查使用。
pub(crate) fn serialize_provider_request_headers(headers: &HeaderMap) -> Option<Value> {
    let mut object = JsonMap::new();
    for (name, value) in headers {
        if let Ok(value_text) = value.to_str() {
            object.insert(
                name.as_str().to_string(),
                Value::String(value_text.to_string()),
            );
        }
    }

    (!object.is_empty()).then_some(Value::Object(object))
}

/// Builds the exact structured request snapshot for one host-controlled OpenAI-compatible turn right before dispatch.
/// 在真正分发前为单次宿主控制的 OpenAI 兼容回合构建精确结构化请求快照。
pub(crate) fn build_openai_compatible_turn_request_snapshot(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    current_history: &[RigMessage],
    current_prompt: &RigMessage,
    temperature: Option<f64>,
    max_turns: usize,
    resolved_tools: &[ResolvedToolDefinition],
    headers: &HeaderMap,
    additional_params: Option<Value>,
) -> CoreProviderRequestSnapshot {
    let mut conversation_state = SessionConversationState::default();
    replace_messages_from_rig_history(
        &mut conversation_state,
        current_history,
        Some(provider_id),
        Some(model_id),
        current_unix_millis_text(),
    );
    let core_snapshot = build_core_conversation_snapshot(&conversation_state);

    CoreProviderRequestSnapshot {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        system_prompt: system_preamble.to_string(),
        prompt: render_provider_visible_prompt_message(current_prompt),
        history: core_messages_to_provider_visible_history(&core_snapshot.messages),
        temperature,
        max_turns,
        tool_schemas: resolved_tools
            .iter()
            .map(resolved_tool_definition_to_core_tool_schema)
            .collect(),
        additional_params,
        request_headers: serialize_provider_request_headers(headers),
        continuation: core_snapshot.continuation,
    }
}

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
fn session_message_to_core_message(
    message: &SessionConversationMessage,
) -> CoreConversationMessage {
    let payload = match &message.payload {
        SessionConversationPayload::UserText { text } => {
            CoreConversationPayload::UserText { text: text.clone() }
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            tool_calls: tool_calls
                .iter()
                .map(session_tool_call_to_core_tool_call)
                .collect(),
        },
        SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => CoreConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one session-owned canonical tool-call entry into the `vulcan-core` tool-call shape.
/// 将一条会话持有的规范工具调用项转换为 `vulcan-core` 工具调用形状。
fn session_tool_call_to_core_tool_call(
    tool_call: &SessionConversationToolCall,
) -> CoreConversationToolCall {
    CoreConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one session-owned provider continuation snapshot into the `vulcan-core` continuation shape.
/// 将一份会话持有的 Provider continuation 快照转换为 `vulcan-core` continuation 形状。
fn session_continuation_to_core_continuation(
    continuation: &SessionProviderContinuationState,
) -> CoreProviderContinuation {
    CoreProviderContinuation {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Converts one session-owned durable record into the `vulcan-core` record shape so pre-existing transcript records survive controller round-trips.
/// 将一条会话持有的持久化记录转换为 `vulcan-core` 记录形状，从而让既有转录记录在控制器往返过程中得以保留。
fn session_record_to_core_record(record: &SessionConversationRecord) -> CoreConversationRecord {
    CoreConversationRecord {
        id: record.id.clone(),
        role: session_record_to_core_role(record),
        record_kind: session_record_to_core_record_kind(record.record_kind),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Derives the `vulcan-core` record role from one GUI-owned durable record.
/// 根据一条 GUI 持有的持久化记录推导 `vulcan-core` 记录角色。
fn session_record_to_core_role(record: &SessionConversationRecord) -> CoreTranscriptRole {
    match record.record_kind {
        crate::model::SessionMessageRecordKind::ThinkingOutput => CoreTranscriptRole::Thinking,
        crate::model::SessionMessageRecordKind::ToolCallOutput
        | crate::model::SessionMessageRecordKind::ToolResultOutput => CoreTranscriptRole::Tool,
        crate::model::SessionMessageRecordKind::SummaryOutput
        | crate::model::SessionMessageRecordKind::CompactionSummary => CoreTranscriptRole::Summary,
        crate::model::SessionMessageRecordKind::ExploreOutput => CoreTranscriptRole::Explore,
        _ => match record.kind {
            crate::model::SessionMessageKind::User => CoreTranscriptRole::User,
            crate::model::SessionMessageKind::Assistant => CoreTranscriptRole::Assistant,
            crate::model::SessionMessageKind::Error => CoreTranscriptRole::System,
        },
    }
}

/// Converts one GUI-owned durable record kind into the `vulcan-core` record-kind classification.
/// 将一种 GUI 持有的持久化记录类型转换为 `vulcan-core` 记录类型分类。
fn session_record_to_core_record_kind(
    record_kind: crate::model::SessionMessageRecordKind,
) -> CoreTranscriptRecordKind {
    match record_kind {
        crate::model::SessionMessageRecordKind::Transcript
        | crate::model::SessionMessageRecordKind::AssistantOutput
        | crate::model::SessionMessageRecordKind::ToolCallOutput
        | crate::model::SessionMessageRecordKind::ToolResultOutput => {
            CoreTranscriptRecordKind::Visible
        }
        crate::model::SessionMessageRecordKind::ThinkingOutput
        | crate::model::SessionMessageRecordKind::TitleOutput
        | crate::model::SessionMessageRecordKind::SummaryOutput => CoreTranscriptRecordKind::Hidden,
        crate::model::SessionMessageRecordKind::ExploreOutput => {
            CoreTranscriptRecordKind::ProjectionOnly
        }
        crate::model::SessionMessageRecordKind::CompactionSummary => {
            CoreTranscriptRecordKind::Checkpoint
        }
    }
}

/// Converts canonical core messages into the provider-visible history that will actually be sent upstream.
/// 将规范 core 消息转换为真正会发送给上游 Provider 的可见历史。
pub(crate) fn core_messages_to_provider_visible_history(
    messages: &[CoreConversationMessage],
) -> Vec<CoreConversationMessage> {
    messages
        .iter()
        .map(|message| {
            let payload = match &message.payload {
                CoreConversationPayload::UserText { text } => {
                    CoreConversationPayload::UserText { text: text.clone() }
                }
                CoreConversationPayload::Assistant {
                    text,
                    reasoning: _,
                    provider_message_id,
                    tool_calls,
                } => CoreConversationPayload::Assistant {
                    text: text.clone(),
                    reasoning: String::new(),
                    provider_message_id: provider_message_id.clone(),
                    tool_calls: tool_calls.clone(),
                },
                CoreConversationPayload::ToolResult {
                    tool_result_id,
                    tool_call_id,
                    text,
                } => CoreConversationPayload::ToolResult {
                    tool_result_id: tool_result_id.clone(),
                    tool_call_id: tool_call_id.clone(),
                    text: text.clone(),
                },
            };

            CoreConversationMessage {
                id: message.id.clone(),
                payload,
            }
        })
        .collect()
}

/// Converts one `vulcan-core` message back into the GUI-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回 GUI 持有的规范会话消息形状。
fn core_message_to_session_message(
    message: &CoreConversationMessage,
) -> SessionConversationMessage {
    let payload = match &message.payload {
        CoreConversationPayload::UserText { text } => {
            SessionConversationPayload::UserText { text: text.clone() }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            tool_calls: tool_calls
                .iter()
                .map(core_tool_call_to_session_tool_call)
                .collect(),
        },
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => SessionConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    SessionConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one `vulcan-core` tool-call entry back into the GUI-owned canonical tool-call shape.
/// 将一条 `vulcan-core` 工具调用项转换回 GUI 持有的规范工具调用形状。
fn core_tool_call_to_session_tool_call(
    tool_call: &CoreConversationToolCall,
) -> SessionConversationToolCall {
    SessionConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one `vulcan-core` durable record back into the GUI-owned record shape while preserving display metadata.
/// 将一条 `vulcan-core` 持久化记录转换回 GUI 持有的记录形状，同时保留显示元数据。
fn core_record_to_session_record(record: &CoreConversationRecord) -> SessionConversationRecord {
    SessionConversationRecord {
        id: record.id.clone(),
        kind: core_record_to_session_message_kind(record),
        record_kind: core_record_to_session_record_kind(record),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one `vulcan-core` continuation snapshot back into the GUI-owned continuation shape.
/// 将一份 `vulcan-core` continuation 快照转换回 GUI 持有的 continuation 形状。
fn core_continuation_to_session_continuation(
    continuation: &CoreProviderContinuation,
) -> SessionProviderContinuationState {
    SessionProviderContinuationState {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Derives the GUI transcript message kind from one `vulcan-core` durable record role.
/// 根据一条 `vulcan-core` 持久化记录角色推导 GUI 转录消息类型。
fn core_record_to_session_message_kind(
    record: &CoreConversationRecord,
) -> crate::model::SessionMessageKind {
    match record.role {
        CoreTranscriptRole::User => crate::model::SessionMessageKind::User,
        _ => crate::model::SessionMessageKind::Assistant,
    }
}

/// Derives the GUI record kind from one `vulcan-core` durable record while preserving the semantic difference between tool calls, tool results, and hidden nodes.
/// 根据一条 `vulcan-core` 持久化记录推导 GUI 记录类型，同时保留工具调用、工具结果与隐藏节点之间的语义差异。
fn core_record_to_session_record_kind(
    record: &CoreConversationRecord,
) -> crate::model::SessionMessageRecordKind {
    match (&record.role, &record.record_kind) {
        (CoreTranscriptRole::Thinking, _) => crate::model::SessionMessageRecordKind::ThinkingOutput,
        (CoreTranscriptRole::Summary, _) | (_, CoreTranscriptRecordKind::Checkpoint) => {
            crate::model::SessionMessageRecordKind::CompactionSummary
        }
        (CoreTranscriptRole::Explore, _) | (_, CoreTranscriptRecordKind::ProjectionOnly) => {
            crate::model::SessionMessageRecordKind::ExploreOutput
        }
        (CoreTranscriptRole::Tool, _) if core_record_is_tool_result(record) => {
            crate::model::SessionMessageRecordKind::ToolResultOutput
        }
        (CoreTranscriptRole::Tool, _) => crate::model::SessionMessageRecordKind::ToolCallOutput,
        (CoreTranscriptRole::Assistant, _) if record.visible => {
            crate::model::SessionMessageRecordKind::AssistantOutput
        }
        (CoreTranscriptRole::Assistant, _) => crate::model::SessionMessageRecordKind::SummaryOutput,
        _ => crate::model::SessionMessageRecordKind::Transcript,
    }
}

/// Returns whether one `vulcan-core` tool record semantically represents a tool result rather than the original tool call request.
/// 返回一条 `vulcan-core` 工具记录在语义上是否表示工具结果，而不是原始工具调用请求。
fn core_record_is_tool_result(record: &CoreConversationRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(Value::as_object)
        .map(|object| object.contains_key("is_error") || object.contains_key("kind"))
        .unwrap_or(false)
}

/// Converts one `vulcan-core` tool-call entry into rig assistant content that keeps both the provider-emitted item identifier and the optional call identifier.
/// 将一条 `vulcan-core` 工具调用项转换为 rig 助手内容，同时保留 Provider 产出的项标识与可选调用标识。
fn core_tool_call_to_rig_assistant_content(
    tool_call: &CoreConversationToolCall,
) -> RigAssistantContent {
    match &tool_call.tool_call_id {
        Some(call_id) => RigAssistantContent::tool_call_with_call_id(
            tool_call.id.clone(),
            call_id.clone(),
            tool_call.tool_name.clone(),
            tool_call.arguments.clone(),
        ),
        None => RigAssistantContent::tool_call(
            tool_call.id.clone(),
            tool_call.tool_name.clone(),
            tool_call.arguments.clone(),
        ),
    }
}

/// Renders one provider-visible prompt message into plain text so request snapshots can reflect the exact last outgoing message body.
/// 将单条 Provider 可见提示消息渲染为纯文本，使请求快照能够反映精确的最后一条出站消息正文。
fn render_provider_visible_prompt_message(message: &RigMessage) -> String {
    match message {
        RigMessage::System { content } => content.trim().to_string(),
        RigMessage::User { content } => content
            .iter()
            .filter_map(|item| match item {
                RigUserContent::Text(text) => Some(text.text.trim().to_string()),
                RigUserContent::ToolResult(tool_result) => {
                    Some(flatten_rig_tool_result_content(&tool_result.content))
                }
                RigUserContent::Image(_)
                | RigUserContent::Audio(_)
                | RigUserContent::Video(_)
                | RigUserContent::Document(_) => None,
            })
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
        RigMessage::Assistant { content, .. } => content
            .iter()
            .filter_map(|item| match item {
                RigAssistantContent::Text(text) => Some(text.text.trim().to_string()),
                _ => None,
            })
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
    }
}

/// Flattens rig tool-result content into plain text so request snapshots can store the exact outgoing prompt payload safely.
/// 将 rig 工具结果内容压平成纯文本，使请求快照能够安全保存精确的出站提示词载荷。
fn flatten_rig_tool_result_content(content: &OneOrMany<ToolResultContent>) -> String {
    content
        .iter()
        .map(|item| match item {
            ToolResultContent::Text(text) => text.text.trim().to_string(),
            ToolResultContent::Image(image) => {
                serde_json::to_string(image).unwrap_or_else(|_| String::from("{}"))
            }
        })
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Renders one tool-source label used by the request-preview tab.
/// 渲染请求预览页签使用的单个工具来源标签。
fn tool_source_label(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "原生工具",
        VulcanToolSourceKind::LuaSkills => "LuaSkills",
        VulcanToolSourceKind::Mcp => "MCP",
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for protocol-state refreshes inside the shared provider request bridge.
/// 以文本形式返回当前 Unix 毫秒时间戳，供共享 Provider 请求桥接逻辑刷新协议状态时使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

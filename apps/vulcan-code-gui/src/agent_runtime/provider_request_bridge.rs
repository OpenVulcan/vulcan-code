//! Shared provider-request bridge helpers reused by the executor, transport adapters, and regression tests.
//! 供执行器、传输适配器与回归测试复用的共享 Provider 请求桥接辅助逻辑。

pub(crate) use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
use crate::agent_runtime::provider_request_bridge_messages::{
    core_message_to_session_message, session_message_to_core_message,
};
use crate::agent_runtime::provider_request_bridge_records::{
    core_continuation_to_session_continuation, core_record_to_session_record,
    session_continuation_to_core_continuation, session_record_to_core_record,
};
use crate::agent_runtime::provider_request_bridge_snapshot::tool_source_label;
pub(crate) use crate::agent_runtime::provider_request_bridge_snapshot::{
    build_openai_compatible_turn_request_snapshot, serialize_provider_request_headers,
};
use crate::conversation_protocol::SessionConversationState;
use rig::OneOrMany;
use rig::completion::ToolDefinition as RigToolDefinition;
use rig::message::{AssistantContent as RigAssistantContent, Message as RigMessage};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot,
    ConversationToolCall as CoreConversationToolCall,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
};
use vulcan_tools_manager::ResolvedToolDefinition;

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

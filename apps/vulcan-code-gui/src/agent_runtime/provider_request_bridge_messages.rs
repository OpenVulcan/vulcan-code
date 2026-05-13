//! Canonical conversation-message bridge helpers reused by snapshot assembly and provider-visible history projection.
//! 供快照组装与 Provider 可见历史投影复用的规范会话消息桥接辅助逻辑。

use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
pub(crate) fn session_message_to_core_message(
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
            provider_message_state,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
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

/// Converts one `vulcan-core` message back into the GUI-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回 GUI 持有的规范会话消息形状。
pub(crate) fn core_message_to_session_message(
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
            provider_message_state,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
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
                    provider_message_state,
                    tool_calls,
                } => CoreConversationPayload::Assistant {
                    text: text.clone(),
                    reasoning: String::new(),
                    provider_message_id: provider_message_id.clone(),
                    provider_message_state: provider_message_state.clone(),
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

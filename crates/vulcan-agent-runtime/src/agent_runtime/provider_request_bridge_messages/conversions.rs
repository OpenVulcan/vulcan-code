use super::*;

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
pub(crate) fn session_message_to_core_message(
    message: &SessionConversationMessage,
) -> CoreConversationMessage {
    let payload = match &message.payload {
        SessionConversationPayload::UserText { text, attachments } => {
            CoreConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(session_tool_call_to_core_tool_call)
                .collect(),
        },
        SessionConversationPayload::ToolResult {
            tool_call_id,
            is_error,
            text,
        } => CoreConversationPayload::ToolResult {
            tool_call_id: tool_call_id.clone(),
            is_error: *is_error,
            text: text.clone(),
        },
        SessionConversationPayload::AgentMessage {
            message_type,
            task_name,
            sender,
            recipient,
            payload,
            trigger_turn,
            metadata,
        } => CoreConversationPayload::AgentMessage {
            message_type: message_type.clone(),
            task_name: task_name.clone(),
            sender: sender.clone(),
            recipient: recipient.clone(),
            payload: payload.clone(),
            trigger_turn: *trigger_turn,
            metadata: metadata.clone(),
        },
    };

    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one `vulcan-core` message back into the host-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回运行时持有的规范会话消息形状。
pub(crate) fn core_message_to_session_message(
    message: &CoreConversationMessage,
) -> SessionConversationMessage {
    let payload = match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            SessionConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(core_tool_call_to_session_tool_call)
                .collect(),
        },
        CoreConversationPayload::ToolResult {
            tool_call_id,
            is_error,
            text,
        } => SessionConversationPayload::ToolResult {
            tool_call_id: tool_call_id.clone(),
            is_error: *is_error,
            text: text.clone(),
        },
        CoreConversationPayload::AgentMessage {
            message_type,
            task_name,
            sender,
            recipient,
            payload,
            trigger_turn,
            metadata,
        } => SessionConversationPayload::AgentMessage {
            message_type: message_type.clone(),
            task_name: task_name.clone(),
            sender: sender.clone(),
            recipient: recipient.clone(),
            payload: payload.clone(),
            trigger_turn: *trigger_turn,
            metadata: metadata.clone(),
        },
    };

    SessionConversationMessage {
        id: message.id.clone(),
        payload,
    }
}
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

/// Converts one `vulcan-core` tool-call entry back into the host-owned canonical tool-call shape.
/// 将一条 `vulcan-core` 工具调用项转换回运行时持有的规范工具调用形状。
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

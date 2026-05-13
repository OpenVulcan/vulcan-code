//! Rig-specific request-message projection helpers kept at the final provider transport edge only.
//! 仅在最终 Provider 传输边界保留的 rig 专用请求消息投影辅助模块。

use rig::OneOrMany;
use rig::message::{AssistantContent as RigAssistantContent, Message as RigMessage};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

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

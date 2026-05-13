//! Rig-specific provider content conversion helpers that keep third-party message enums at the transport edge only.
//! 仅在传输边界保留第三方消息枚举的 rig 专用 Provider 内容转换辅助模块。

use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_deltas_from_assistant_content,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart, ProviderStreamedAssistantContent,
};
use rig::OneOrMany;
use rig::message::{AssistantContent as RigAssistantContent, ReasoningContent};
use rig::streaming::StreamedAssistantContent;
use vulcan_core::types::{
    ConversationToolCall as CoreConversationToolCall, ProviderTurnDelta as CoreProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Converts one rig streamed assistant content chunk into the host-owned streamed content shape before shared bridge logic processes it.
/// 在共享桥接逻辑处理之前，将一条 rig 流式助手内容块转换为宿主自有的流式内容形状。
pub(crate) fn provider_streamed_assistant_content_from_rig<R>(
    content: StreamedAssistantContent<R>,
) -> ProviderStreamedAssistantContent {
    match content {
        StreamedAssistantContent::Text(text) => ProviderStreamedAssistantContent::Text(text.text),
        StreamedAssistantContent::Reasoning(reasoning) => {
            ProviderStreamedAssistantContent::Reasoning(
                reasoning
                    .content
                    .iter()
                    .map(provider_reasoning_part_from_rig)
                    .collect(),
            )
        }
        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
            ProviderStreamedAssistantContent::ReasoningDelta(reasoning)
        }
        StreamedAssistantContent::ToolCall { .. } => ProviderStreamedAssistantContent::ToolCall,
        StreamedAssistantContent::ToolCallDelta { .. } => {
            ProviderStreamedAssistantContent::ToolCallDelta
        }
        StreamedAssistantContent::Final(_) => ProviderStreamedAssistantContent::Final,
    }
}

/// Converts one rig assistant-choice list into the host-owned assistant content list consumed by the shared final-response bridge.
/// 将一组 rig 助手 choice 列表转换为共享最终响应桥消费的宿主自有助手内容列表。
pub(crate) fn provider_assistant_contents_from_rig(
    assistant_content: &OneOrMany<RigAssistantContent>,
) -> Vec<ProviderAssistantContent> {
    assistant_content
        .iter()
        .filter_map(|item| match item {
            RigAssistantContent::Text(text) => {
                Some(ProviderAssistantContent::Text(text.text.clone()))
            }
            RigAssistantContent::Reasoning(reasoning) => Some(ProviderAssistantContent::Reasoning(
                reasoning
                    .content
                    .iter()
                    .map(provider_reasoning_part_from_rig)
                    .collect(),
            )),
            RigAssistantContent::ToolCall(tool_call) => Some(ProviderAssistantContent::ToolCall(
                CoreConversationToolCall {
                    id: tool_call.id.clone(),
                    tool_call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.function.name.clone(),
                    arguments: tool_call.function.arguments.clone(),
                },
            )),
            _ => None,
        })
        .collect()
}

/// Converts one rig streamed assistant content chunk directly into `vulcan-core` provider-turn deltas while keeping rig-specific decoding at the edge.
/// 在把 rig 专用解码保留在边界的同时，将一条 rig 流式助手内容块直接转换为 `vulcan-core` Provider 回合增量。
pub(crate) fn core_provider_turn_deltas_from_rig_assistant_content<R>(
    content: StreamedAssistantContent<R>,
) -> Vec<CoreProviderTurnDelta> {
    core_provider_turn_deltas_from_assistant_content(provider_streamed_assistant_content_from_rig(
        content,
    ))
}

/// Builds one normalized `vulcan-core` turn response from rig assistant content while keeping the shared final-response bridge rig-free.
/// 在保持共享最终响应桥不依赖 rig 的前提下，根据 rig 助手内容构建一份规范化的 `vulcan-core` 回合响应。
pub(crate) fn build_openai_compatible_turn_response_from_rig(
    provider_id: &str,
    model_id: &str,
    provider_message_id: Option<String>,
    assistant_content: &OneOrMany<RigAssistantContent>,
) -> CoreProviderTurnResponse {
    build_openai_compatible_turn_response(
        provider_id,
        model_id,
        provider_message_id,
        &provider_assistant_contents_from_rig(assistant_content),
    )
}

/// Converts one rig reasoning fragment into the host-owned reasoning fragment shape used by the shared provider bridge.
/// 将一条 rig 思考片段转换为共享 Provider 桥使用的宿主自有思考片段形状。
fn provider_reasoning_part_from_rig(part: &ReasoningContent) -> ProviderReasoningPart {
    match part {
        ReasoningContent::Text { text, .. } => ProviderReasoningPart::Text(text.clone()),
        ReasoningContent::Summary(summary) => ProviderReasoningPart::Summary(summary.clone()),
        _ => ProviderReasoningPart::Opaque,
    }
}

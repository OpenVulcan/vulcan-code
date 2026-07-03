//! Core conversation to Gemini content conversion.
//! Core 对话消息到 Gemini 内容项的转换逻辑。

use super::tool_conversion::core_tool_call_to_gemini_function_call;
use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::agent_runtime::provider_protocol_gemini::{
    GeminiContent, GeminiFunctionResponse, GeminiPart, parse_gemini_provider_message_state,
};
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
};

pub(super) fn core_message_to_gemini_content(
    message: &CoreConversationMessage,
    previous_messages: &[CoreConversationMessage],
    provider_id: &str,
    model_id: &str,
) -> Result<GeminiContent, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::GoogleGeminiGenerateContent,
                text,
                attachments,
            );
            let mut parts = Vec::new();
            if !normalized.text.is_empty() {
                parts.push(GeminiPart::text(normalized.text));
            }
            for attachment in &normalized.attachments {
                let attachment_payload = match read_provider_attachment(attachment)? {
                    Some(payload) => payload,
                    None => {
                        parts.push(GeminiPart::text(build_missing_attachment_replay_text(
                            attachment,
                        )));
                        continue;
                    }
                };
                let mut additional_fields = BTreeMap::new();
                additional_fields.insert(
                    String::from("inlineData"),
                    json!({
                        "mimeType": attachment_payload.mime_type,
                        "data": attachment_payload.base64_data
                    }),
                );
                parts.push(GeminiPart {
                    thought: Some(false),
                    additional_fields,
                    ..GeminiPart::default()
                });
            }
            Ok(GeminiContent::user(parts))
        }
        CoreConversationPayload::Assistant {
            text,
            provider_message_state,
            tool_calls,
            ..
        } => {
            if let Some(content) = provider_message_state
                .as_ref()
                .and_then(|state| parse_gemini_provider_message_state(state, provider_id, model_id))
            {
                return Ok(if content.role.is_some() {
                    content
                } else {
                    GeminiContent::model(content.parts)
                });
            }

            let mut parts = Vec::new();
            if !text.trim().is_empty() {
                parts.push(GeminiPart::text(text.clone()));
            }
            parts.extend(
                tool_calls
                    .iter()
                    .cloned()
                    .map(core_tool_call_to_gemini_function_call)
                    .map(GeminiPart::function_call),
            );
            Ok(GeminiContent::model(parts))
        }
        CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } => {
            let Some(request_key) = tool_call_id
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: String::from(
                        "Google Gemini 工具结果消息缺少 tool_call_id，无法构造原生 provider 请求。",
                    ),
                });
            };
            let Some((tool_name, provider_visible_call_id)) =
                find_matching_gemini_tool_call(previous_messages, request_key.as_str())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: format!(
                        "Google Gemini 工具结果无法在历史中找到对应调用 `{request_key}`。"
                    ),
                });
            };

            Ok(GeminiContent::user(vec![GeminiPart::function_response(
                GeminiFunctionResponse {
                    id: provider_visible_call_id,
                    name: tool_name,
                    response: Some(parse_tool_result_text_as_json(text)),
                },
            )]))
        }
        CoreConversationPayload::AgentMessage { .. } => {
            Ok(GeminiContent::user(vec![GeminiPart::text(
                message
                    .payload
                    .agent_message_model_text()
                    .unwrap_or_default(),
            )]))
        }
    }
}

/// Resolves the matching Gemini tool call for one tool-result message by scanning the already-emitted history in reverse order.
/// 通过逆序扫描已发出的历史，为一条工具结果消息解析对应的 Gemini 工具调用。
fn find_matching_gemini_tool_call(
    previous_messages: &[CoreConversationMessage],
    request_key: &str,
) -> Option<(String, Option<String>)> {
    previous_messages.iter().rev().find_map(|message| {
        let CoreConversationPayload::Assistant { tool_calls, .. } = &message.payload else {
            return None;
        };

        tool_calls.iter().rev().find_map(|tool_call| {
            (resolved_core_tool_call_key(tool_call) == request_key).then(|| {
                (
                    tool_call.tool_name.clone(),
                    tool_call
                        .tool_call_id
                        .clone()
                        .filter(|value| !value.trim().is_empty()),
                )
            })
        })
    })
}

/// Returns the stable lookup key for one canonical core tool call, preferring provider-visible identifiers and falling back to the local item identifier.
/// 返回一条规范 core 工具调用的稳定查找键，优先使用 Provider 可见标识，并在缺失时回退到本地项标识。
fn resolved_core_tool_call_key(tool_call: &CoreConversationToolCall) -> &str {
    tool_call
        .tool_call_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(tool_call.id.as_str())
}

fn parse_tool_result_text_as_json(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "result": text }))
}

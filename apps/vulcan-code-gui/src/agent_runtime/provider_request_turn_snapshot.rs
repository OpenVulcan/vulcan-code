//! Host-controlled single-turn request snapshot helpers used by transport dispatch and regression tests.
//! 供传输分发与回归测试使用的宿主控制单回合请求快照辅助逻辑。

use crate::agent_runtime::provider_protocol_bridge::scope_continuation_to_active_provider;
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
use crate::agent_runtime::provider_request_bridge_tools::provider_tool_definition_to_core_tool_schema;
use crate::agent_runtime::provider_request_snapshot_support::serialize_provider_request_headers;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot,
    ProviderContinuation as CoreProviderContinuation,
    ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Builds the exact structured request snapshot for one host-controlled OpenAI-compatible turn right before dispatch.
/// 在真正分发前为单次宿主控制的 OpenAI 兼容回合构建精确结构化请求快照。
pub(crate) fn build_openai_compatible_turn_request_snapshot(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    continuation: &CoreProviderContinuation,
    current_history: &[CoreConversationMessage],
    current_prompt: &CoreConversationMessage,
    temperature: Option<f64>,
    max_turns: usize,
    provider_tool_definitions: &[ProviderToolDefinition],
    headers: &HeaderMap,
    additional_params: Option<Value>,
) -> CoreProviderRequestSnapshot {
    let mut scoped_continuation =
        scope_continuation_to_active_provider(continuation, provider_id, model_id);
    if scoped_continuation.last_provider_message_id.is_some()
        && scoped_continuation.updated_at.is_none()
    {
        scoped_continuation.updated_at = Some(current_unix_millis_text());
    }
    let core_snapshot = CoreConversationSnapshot {
        messages: current_history.to_vec(),
        continuation: scoped_continuation,
        ..CoreConversationSnapshot::default()
    };

    CoreProviderRequestSnapshot {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        system_prompt: system_preamble.to_string(),
        prompt_layers: CoreProviderPromptLayers::from_system_prompt(system_preamble),
        prompt: render_provider_visible_prompt_message(current_prompt),
        history: core_messages_to_provider_visible_history(&core_snapshot.messages),
        temperature,
        max_turns,
        tool_schemas: provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_core_tool_schema)
            .collect(),
        additional_params,
        request_headers: serialize_provider_request_headers(headers),
        continuation: core_snapshot.continuation,
    }
}

/// Renders one provider-visible prompt message into plain text so request snapshots can reflect the exact last outgoing message body.
/// 将单条 Provider 可见提示消息渲染为纯文本，使请求快照能够反映精确的最后一条出站消息正文。
fn render_provider_visible_prompt_message(message: &CoreConversationMessage) -> String {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let trimmed_text = text.trim();
            let attachment_note = if attachments.is_empty() {
                String::new()
            } else {
                format!(" [附件 {} 个]", attachments.len())
            };
            if trimmed_text.is_empty() {
                if attachment_note.is_empty() {
                    String::new()
                } else {
                    attachment_note.trim().to_string()
                }
            } else {
                format!("{trimmed_text}{attachment_note}")
            }
        }
        CoreConversationPayload::Assistant { text, .. } => text.trim().to_string(),
        CoreConversationPayload::ToolResult { text, .. } => text.trim().to_string(),
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for request-snapshot continuation refreshes.
/// 以文本形式返回当前 Unix 毫秒时间戳，供请求快照 continuation 刷新使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

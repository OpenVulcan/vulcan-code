//! Provider request snapshot helpers layered on top of the shared request-bridge protocol conversions.
//! 构建在共享请求桥协议转换层之上的 Provider 请求快照辅助模块。

use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
use crate::agent_runtime::provider_request_bridge_tools::resolved_tool_definition_to_core_tool_schema;
use crate::conversation_protocol::{SessionConversationState, replace_messages_from_rig_history};
use reqwest::header::HeaderMap;
use rig::OneOrMany;
use rig::message::{
    AssistantContent as RigAssistantContent, Message as RigMessage, ToolResultContent,
    UserContent as RigUserContent,
};
use serde_json::{Map as JsonMap, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::ProviderRequestSnapshot as CoreProviderRequestSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolSourceKind};

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
pub(crate) fn tool_source_label(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "原生工具",
        VulcanToolSourceKind::LuaSkills => "LuaSkills",
        VulcanToolSourceKind::Mcp => "MCP",
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for protocol-state refreshes inside provider request snapshot helpers.
/// 以文本形式返回当前 Unix 毫秒时间戳，供 Provider 请求快照辅助逻辑刷新协议状态时使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

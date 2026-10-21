//! Projects V1 session events into the compact transcript entries MCP clients read.
//! 将 V1 会话事件投影为 MCP 客户端读取的紧凑逐字稿条目。

use serde::Serialize;
use vulcan_mcp_service::{MCP_TOOL_OUTPUT_PREVIEW_CHARS, McpTranscriptEntry, McpTranscriptKind};
use vulcan_session_core::v1::event::{ConversationEvent, EventBody, EventVisibility};

/// Projects one event, or returns `None` for host-internal and bookkeeping events.
/// 投影一个事件；宿主内部与记账类事件返回 `None`。
///
/// # Parameters
/// 参数
/// `include_tool_output` adds a bounded preview of each tool result.
/// `include_tool_output` 为每个工具结果附加有界预览。
pub(super) fn project_event(
    event: &ConversationEvent,
    include_tool_output: bool,
) -> Option<McpTranscriptEntry> {
    if event.visibility == EventVisibility::HostOnly {
        return None;
    }
    let (kind, status, tool_name, text) = match &event.event {
        EventBody::UserInput(input) => (McpTranscriptKind::User, None, None, input.text.clone()),
        EventBody::AssistantText(text) => (
            McpTranscriptKind::Assistant,
            Some(wire_label(&text.status)),
            None,
            text.text.clone(),
        ),
        EventBody::ToolCall(tool) => (
            McpTranscriptKind::Tool,
            Some(wire_label(&tool.status)),
            Some(tool.tool_name.clone()),
            if include_tool_output {
                tool.output
                    .as_ref()
                    .and_then(|output| serde_json::to_value(output).ok())
                    .map(|output| bounded_preview(&tool_output_text(&output)))
                    .unwrap_or_default()
            } else {
                String::new()
            },
        ),
        EventBody::TurnBoundary(boundary) => (
            McpTranscriptKind::TurnEnd,
            Some(wire_label(&boundary.outcome)),
            None,
            wire_label(&boundary.reason),
        ),
        EventBody::DisplayNote(note) => (
            McpTranscriptKind::Notice,
            Some(wire_label(&note.severity)),
            None,
            note.redacted_detail
                .clone()
                .unwrap_or_else(|| note.reason_code.clone()),
        ),
        EventBody::SubagentReturn(child) => (
            McpTranscriptKind::ChildResult,
            None,
            None,
            child.content.clone(),
        ),
        _ => return None,
    };
    Some(McpTranscriptEntry {
        seq: event.seq.value(),
        turn: event.turn_id.map(|turn| turn.value()),
        kind,
        status,
        tool_name,
        text,
        updated_at_ms: event.updated_at_ms,
    })
}

/// Returns the snake_case wire name of one serde enum value.
/// 返回一个 serde 枚举值的 snake_case 线上名称。
fn wire_label(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Prefers a tool result's own text field, falling back to compact JSON.
/// 优先使用工具结果自身的文本字段，否则退回紧凑 JSON。
fn tool_output_text(output: &serde_json::Value) -> String {
    ["text", "content", "output", "result", "error", "message"]
        .iter()
        .find_map(|field| output.get(*field).and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| output.to_string())
}

/// Cuts text to the preview bound on a character boundary, marking the cut.
/// 在字符边界处把文本截断到预览上限，并标记截断。
fn bounded_preview(text: &str) -> String {
    match text.char_indices().nth(MCP_TOOL_OUTPUT_PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_bounded_on_character_boundaries() {
        let long = "é".repeat(MCP_TOOL_OUTPUT_PREVIEW_CHARS + 5);
        let preview = bounded_preview(&long);
        assert_eq!(
            preview.chars().count(),
            MCP_TOOL_OUTPUT_PREVIEW_CHARS + 1,
            "the preview keeps the bound plus one cut marker"
        );
        assert_eq!(bounded_preview("short"), "short");
    }

    #[test]
    fn tool_output_prefers_text_fields() {
        let output = serde_json::json!({ "text": "done", "exitCode": 0 });
        assert_eq!(tool_output_text(&output), "done");
        let structured = serde_json::json!({ "exitCode": 0 });
        assert_eq!(tool_output_text(&structured), structured.to_string());
    }
}

use crate::contract::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    SessionVisibleRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionMessageKind,
    SessionMessageRecordKind, append_record,
};

/// Display projection generated from boundary events for Vulcan transcript surfaces.
/// 为 Vulcan 转录表面从边界事件生成的展示投影。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoundaryConversationProjection {
    /// Canonical conversation state consumed by existing GUI and WebUI projection layers.
    /// 由现有 GUI 与 WebUI 投影层消费的规范会话状态。
    pub conversation_state: SessionConversationState,
}

/// Projects a sorted or unsorted boundary event slice into Vulcan's canonical conversation state.
/// 将有序或无序边界事件切片投影到 Vulcan 的规范会话状态。
pub fn boundary_events_to_vulcan_conversation_state(
    events: &[SessionBoundaryEvent],
) -> BoundaryConversationProjection {
    let mut events = events.iter().collect::<Vec<_>>();
    events.sort_by_key(|event| event.sequence);
    let mut conversation_state = SessionConversationState::default();
    for event in events {
        let Some(record) = session_boundary_event_to_vulcan_record(event) else {
            continue;
        };
        project_record_payload_to_conversation_messages(&mut conversation_state, event, &record);
        append_record(&mut conversation_state, record);
    }
    BoundaryConversationProjection { conversation_state }
}

/// Converts one session boundary event into a Vulcan transcript record when it is main-surface visible.
/// 当 会话边界事件应在主表面可见时，将其转换为一条 Vulcan 转录记录。
pub fn session_boundary_event_to_vulcan_record(
    event: &SessionBoundaryEvent,
) -> Option<SessionConversationRecord> {
    if event.visibility != SessionEventVisibility::Visible {
        return None;
    }
    if event.scope == SessionEventScope::Agent {
        return None;
    }
    let role = session_visible_role_for_event(event)?;
    let text = session_boundary_event_display_text(event);
    let (kind, record_kind, title) = match (role, event.kind) {
        (SessionVisibleRole::User, SessionBoundaryEventKind::UserInput) => (
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "",
        ),
        (SessionVisibleRole::Assistant, _) => (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "",
        ),
        (SessionVisibleRole::Tool, SessionBoundaryEventKind::ItemStarted) => (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "tool",
        ),
        (SessionVisibleRole::Tool, SessionBoundaryEventKind::ItemCompleted) => (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "tool",
        ),
        (SessionVisibleRole::Notice, _) => (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::Transcript,
            "",
        ),
        _ => return None,
    };
    Some(SessionConversationRecord {
        id: session_boundary_record_id(event),
        kind,
        record_kind,
        visible: true,
        title: title.to_string(),
        body: text,
        metadata: Some(session_boundary_event_projection_metadata(event)),
        created_at: event.timestamp_ms.to_string(),
    })
}

/// Appends provider-context messages for records that should contribute to future turns.
/// 为应进入未来轮次上下文的记录追加 Provider 上下文消息。
fn project_record_payload_to_conversation_messages(
    conversation_state: &mut SessionConversationState,
    event: &SessionBoundaryEvent,
    record: &SessionConversationRecord,
) {
    match record.record_kind {
        SessionMessageRecordKind::Transcript if record.kind == SessionMessageKind::User => {
            conversation_state
                .messages
                .push(SessionConversationMessage {
                    id: record.id.clone(),
                    payload: SessionConversationPayload::UserText {
                        text: record.body.clone(),
                        attachments: Vec::new(),
                    },
                });
        }
        SessionMessageRecordKind::AssistantOutput => {
            conversation_state
                .messages
                .push(SessionConversationMessage {
                    id: record.id.clone(),
                    payload: SessionConversationPayload::Assistant {
                        text: record.body.clone(),
                        reasoning: String::new(),
                        provider_message_id: event
                            .payload
                            .get("provider_message_id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        provider_message_state: event
                            .payload
                            .get("provider_message_state")
                            .cloned(),
                        usage: None,
                        tool_calls: session_boundary_event_tool_calls(event),
                    },
                });
        }
        _ => {}
    }
}

/// Returns the visible role represented by one event.
/// 返回单个事件表示的可见角色。
fn session_visible_role_for_event(event: &SessionBoundaryEvent) -> Option<SessionVisibleRole> {
    match event.kind {
        SessionBoundaryEventKind::UserInput => Some(SessionVisibleRole::User),
        SessionBoundaryEventKind::ItemStarted | SessionBoundaryEventKind::ItemCompleted => {
            Some(SessionVisibleRole::Tool)
        }
        SessionBoundaryEventKind::AgentMessage => Some(SessionVisibleRole::Assistant),
        SessionBoundaryEventKind::StateChanged => Some(SessionVisibleRole::Notice),
        _ => None,
    }
}

/// Extracts display text from the known session boundary event payload fields.
/// 从已知 会话边界事件载荷字段中提取展示文本。
fn session_boundary_event_display_text(event: &SessionBoundaryEvent) -> String {
    for key in ["text", "message", "display_text", "summary"] {
        if let Some(text) = event.payload.get(key).and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            return text.to_string();
        }
    }
    if matches!(event.kind, SessionBoundaryEventKind::ItemStarted) {
        return format!("Calling {} tool.", session_boundary_tool_name(event));
    }
    if matches!(event.kind, SessionBoundaryEventKind::ItemCompleted) {
        return format!("{} tool completed.", session_boundary_tool_name(event));
    }
    String::new()
}

/// Builds stable Vulcan record metadata for a projected session boundary event.
/// 为一条已投影 会话边界事件构建稳定 Vulcan 记录元数据。
fn session_boundary_event_projection_metadata(event: &SessionBoundaryEvent) -> Value {
    json!({
        "source": "session_boundary_event",
        "event_id": event.event_id,
        "sequence": event.sequence,
        "kind": event.kind,
        "scope": event.scope,
        "visibility": event.visibility,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "tool_name": event.payload.get("tool_name").and_then(Value::as_str),
        "payload": event.payload,
    })
}

/// Builds a stable record id from the owning session boundary event.
/// 根据所属 会话边界事件构建稳定记录标识。
fn session_boundary_record_id(event: &SessionBoundaryEvent) -> String {
    format!(
        "codex-{}-{}-{}",
        event.thread_id, event.sequence, event.event_id
    )
}

/// Returns tool calls represented by an assistant event payload.
/// 返回助手事件载荷中表示的工具调用列表。
fn session_boundary_event_tool_calls(
    event: &SessionBoundaryEvent,
) -> Vec<SessionConversationToolCall> {
    let Some(tool_calls) = event.payload.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .enumerate()
        .map(|(index, call)| SessionConversationToolCall {
            id: call
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{}-tool-call-{index}", event.event_id)),
            tool_call_id: call
                .get("tool_call_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            tool_name: call
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string(),
            arguments: call.get("arguments").cloned().unwrap_or(Value::Null),
        })
        .collect()
}

/// Returns the best-effort tool name attached to an item event.
/// 返回条目事件携带的最佳努力工具名称。
fn session_boundary_tool_name(event: &SessionBoundaryEvent) -> String {
    event
        .payload
        .get("tool_name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tool")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{SessionBoundaryEvent, SessionEventScope};
    use serde_json::json;
    use vulcan_session_core::SessionNodeId;

    /// Builds a test event with a stable payload.
    /// 构建一条带稳定载荷的测试事件。
    fn event(
        thread_id: SessionNodeId,
        sequence: u64,
        kind: SessionBoundaryEventKind,
        scope: SessionEventScope,
        visibility: SessionEventVisibility,
        payload: Value,
    ) -> SessionBoundaryEvent {
        SessionBoundaryEvent::new(
            format!("event-{sequence}"),
            thread_id,
            sequence,
            kind,
            scope,
            visibility,
            1000 + sequence,
        )
        .with_payload(payload)
    }

    /// Verifies visible user and assistant events become records and provider-context messages.
    /// 验证可见用户与助手事件会成为记录和 Provider 上下文消息。
    #[test]
    fn projects_visible_user_and_assistant_events_to_conversation_state() {
        let thread_id = SessionNodeId::new_v4();
        let projection = boundary_events_to_vulcan_conversation_state(&[
            event(
                thread_id,
                2,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventScope::Turn,
                SessionEventVisibility::Visible,
                json!({ "text": "answer" }),
            ),
            event(
                thread_id,
                1,
                SessionBoundaryEventKind::UserInput,
                SessionEventScope::Turn,
                SessionEventVisibility::Visible,
                json!({ "text": "hello" }),
            ),
        ]);

        assert_eq!(projection.conversation_state.records.len(), 2);
        assert_eq!(projection.conversation_state.records[0].body, "hello");
        assert_eq!(
            projection.conversation_state.records[1].record_kind,
            SessionMessageRecordKind::AssistantOutput
        );
        assert_eq!(projection.conversation_state.messages.len(), 2);
    }

    /// Verifies hidden events and child-agent scoped events are excluded from the main surface.
    /// 验证隐藏事件与子代理作用域事件不会进入主表面。
    #[test]
    fn skips_hidden_and_agent_scoped_events() {
        let thread_id = SessionNodeId::new_v4();
        let projection = boundary_events_to_vulcan_conversation_state(&[
            event(
                thread_id,
                1,
                SessionBoundaryEventKind::RawItemCompleted,
                SessionEventScope::Turn,
                SessionEventVisibility::Hidden,
                json!({ "text": "thinking" }),
            ),
            event(
                thread_id,
                2,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventScope::Agent,
                SessionEventVisibility::Visible,
                json!({ "text": "child transcript" }),
            ),
        ]);

        assert!(projection.conversation_state.records.is_empty());
        assert!(projection.conversation_state.messages.is_empty());
    }

    /// Verifies tool item events become visible tool records but not provider-context messages.
    /// 验证工具条目事件会成为可见工具记录但不会进入 Provider 上下文消息。
    #[test]
    fn tool_events_project_to_records_only() {
        let thread_id = SessionNodeId::new_v4();
        let projection = boundary_events_to_vulcan_conversation_state(&[event(
            thread_id,
            1,
            SessionBoundaryEventKind::ItemCompleted,
            SessionEventScope::Item,
            SessionEventVisibility::Visible,
            json!({ "tool_name": "spawn_agent", "text": "done" }),
        )]);

        assert_eq!(projection.conversation_state.records.len(), 1);
        assert_eq!(
            projection.conversation_state.records[0].record_kind,
            SessionMessageRecordKind::ToolResultOutput
        );
        assert!(projection.conversation_state.messages.is_empty());
    }
}

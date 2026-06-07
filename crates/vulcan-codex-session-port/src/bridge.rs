use crate::contract::{
    CodexEventScope, CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind,
};
use serde_json::Value;
use vulcan_session_core::{SessionEventKind, SessionEventRecord};

/// Converts one current Vulcan session-core event record into the Codex-compatible event contract.
/// 将当前 Vulcan session-core 事件记录转换为 Codex 兼容事件契约。
pub fn codex_event_from_session_event_record(record: &SessionEventRecord) -> CodexSessionEvent {
    let (kind, scope, visibility) = codex_contract_classification_for_session_event(&record.kind);
    let mut event = CodexSessionEvent::new(
        record.event_id.clone(),
        record.node_id,
        record.sequence,
        kind,
        scope,
        visibility,
        record.timestamp_ms,
    )
    .with_payload(record.payload.clone());
    if let Some(turn_id) = string_payload_field(&record.payload, "turn_id") {
        event = event.with_turn_id(turn_id);
    }
    if let Some(item_id) = string_payload_field(&record.payload, "item_id")
        .or_else(|| string_payload_field(&record.payload, "call_id"))
    {
        event = event.with_item_id(item_id);
    }
    event
}

/// Classifies a Vulcan session-core event kind using Codex-compatible scope and visibility.
/// 使用 Codex 兼容范围与可见性分类 Vulcan session-core 事件类型。
fn codex_contract_classification_for_session_event(
    kind: &SessionEventKind,
) -> (CodexSessionEventKind, CodexEventScope, CodexEventVisibility) {
    match kind {
        SessionEventKind::UserPrompt => (
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
        ),
        SessionEventKind::AssistantOutput => (
            CodexSessionEventKind::AgentMessage,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
        ),
        SessionEventKind::ToolCall => (
            CodexSessionEventKind::ItemStarted,
            CodexEventScope::Item,
            CodexEventVisibility::Visible,
        ),
        SessionEventKind::ToolResult => (
            CodexSessionEventKind::ItemCompleted,
            CodexEventScope::Item,
            CodexEventVisibility::Visible,
        ),
        SessionEventKind::TaskInvocation => (
            CodexSessionEventKind::AgentStarted,
            CodexEventScope::Agent,
            CodexEventVisibility::Hidden,
        ),
        SessionEventKind::TaskFeedback => (
            CodexSessionEventKind::AgentCompleted,
            CodexEventScope::Agent,
            CodexEventVisibility::Hidden,
        ),
        SessionEventKind::AssistantThinking => (
            CodexSessionEventKind::RawItemCompleted,
            CodexEventScope::Turn,
            CodexEventVisibility::Hidden,
        ),
        SessionEventKind::CompactionMarker => (
            CodexSessionEventKind::Compaction,
            CodexEventScope::Thread,
            CodexEventVisibility::Hidden,
        ),
        SessionEventKind::RuntimeMarker | SessionEventKind::TurnStats => (
            CodexSessionEventKind::StateChanged,
            CodexEventScope::Thread,
            CodexEventVisibility::Diagnostic,
        ),
    }
}

/// Reads one string field from a structured payload.
/// 从结构化载荷读取单个字符串字段。
fn string_payload_field(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::project_codex_display_event;
    use serde_json::json;
    use vulcan_session_core::SessionNodeId;

    /// Verifies current user prompt records become visible Codex user input events.
    /// 验证当前用户提示词记录会转换为可见 Codex 用户输入事件。
    #[test]
    fn user_prompt_record_maps_to_visible_user_input() {
        let node_id = SessionNodeId::new_v4();
        let record = SessionEventRecord::new(
            "event-1",
            node_id,
            1,
            SessionEventKind::UserPrompt,
            1000,
            json!({ "turn_id": "turn-1", "text": "hello" }),
        );

        let event = codex_event_from_session_event_record(&record);
        let display_item = project_codex_display_event(&event).expect("event should be visible");

        assert_eq!(event.thread_id, node_id);
        assert_eq!(event.kind, CodexSessionEventKind::UserInput);
        assert_eq!(event.visibility, CodexEventVisibility::Visible);
        assert_eq!(display_item.text, "hello");
    }

    /// Verifies current task feedback records stay hidden from normal display projection.
    /// 验证当前子代理反馈记录不会进入普通展示投影。
    #[test]
    fn task_feedback_record_maps_to_hidden_agent_completion() {
        let record = SessionEventRecord::new(
            "event-1",
            SessionNodeId::new_v4(),
            1,
            SessionEventKind::TaskFeedback,
            1000,
            json!({ "text": "child done" }),
        );

        let event = codex_event_from_session_event_record(&record);

        assert_eq!(event.kind, CodexSessionEventKind::AgentCompleted);
        assert_eq!(event.visibility, CodexEventVisibility::Hidden);
        assert_eq!(project_codex_display_event(&event), None);
    }
}

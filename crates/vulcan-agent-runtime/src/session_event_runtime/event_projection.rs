use super::*;

/// Projects one runtime event into visible transcript records when the event has user-facing detail.
/// 当运行时事件包含面向用户的细节时，将其投影为可见转录记录。
pub fn append_runtime_event_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> bool {
    match event.kind {
        RuntimeEventKind::ToolStarted => {
            append_tool_started_projection_to_state(runtime_state, session_id, now, event)
        }
        RuntimeEventKind::ToolFinished => {
            append_tool_finished_projection_to_state(runtime_state, session_id, now, event)
        }
        _ => false,
    }
}

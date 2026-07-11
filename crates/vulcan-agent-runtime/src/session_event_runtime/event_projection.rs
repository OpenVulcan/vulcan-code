use super::*;

/// Projects one runtime event into visible transcript records when the event has user-facing detail.
/// 当运行时事件包含面向用户的细节时，将其投影为可见转录记录。
pub fn append_runtime_event_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    // draft_runtime_state isolates detail records, visible records, and call mappings until the event projection succeeds.
    // draft_runtime_state 会隔离详情记录、可见记录与调用映射，直到事件投影成功。
    let mut draft_runtime_state = runtime_state.clone();
    // projected reports whether this event kind produced a durable projection in the detached draft.
    // projected 表示该事件类型是否在分离草稿中产出了持久投影。
    let projected = match event.kind {
        RuntimeEventKind::ToolStarted => append_tool_started_projection_to_state(
            &mut draft_runtime_state,
            session_id,
            now,
            event,
        )?,
        RuntimeEventKind::ToolFinished => append_tool_finished_projection_to_state(
            &mut draft_runtime_state,
            session_id,
            now,
            event,
        )?,
        _ => false,
    };
    if projected {
        *runtime_state = draft_runtime_state;
    }
    Ok(projected)
}

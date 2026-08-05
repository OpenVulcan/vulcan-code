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
        RuntimeEventKind::ToolRequestStarted => append_tool_request_started_projection_to_state(
            &mut draft_runtime_state,
            session_id,
            now,
            event,
        )?,
        RuntimeEventKind::ToolRequestProgress
        | RuntimeEventKind::ToolRequestPreviewUpdated
        | RuntimeEventKind::ToolRequestReady => {
            update_tool_request_projection_to_state(&mut draft_runtime_state, now, event)?
        }
        RuntimeEventKind::ToolRequestInvalid | RuntimeEventKind::ToolRequestAborted => {
            abort_tool_request_projection_to_state(&mut draft_runtime_state, now, event)?
        }
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
        RuntimeEventKind::Retrying | RuntimeEventKind::TransportFallback => {
            append_provider_transport_note_projection_to_state(
                &mut draft_runtime_state,
                session_id,
                now,
                event,
            )?
        }
        _ => false,
    };
    if projected {
        *runtime_state = draft_runtime_state;
    }
    Ok(projected)
}

/// Appends one durable display-only note for Provider retry or transport fallback.
/// 为 Provider 重试或传输回退追加一条持久化的纯显示备注。
///
/// `runtime_state` owns the durable session conversation state.
/// `runtime_state` 持有持久化会话对话状态。
/// `session_id` identifies the conversation receiving the note.
/// `session_id` 标识接收备注的会话。
/// `now` is the persistence timestamp used by the new record.
/// `now` 是新记录使用的持久化时间戳。
/// `event` contains the exact transport title, detail, and diagnostic metadata.
/// `event` 包含精确的传输标题、详情与诊断元数据。
/// Returns `true` after the transport note is appended successfully.
/// 成功追加传输备注后返回 `true`。
fn append_provider_transport_note_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    // recordId gives every waiting, retry, or fallback transition its own stable visible history row.
    // recordId 为每次等待、重试或回退转换提供独立、稳定的可见历史行。
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::DisplayNote,
            visible: true,
            title: event.title.clone(),
            body: event.detail.clone(),
            metadata: event.metadata.clone(),
            created_at: now.to_string(),
        },
    )
    .map_err(|error| format!("Invalid Provider transport display-note identity: {error}"))?;
    runtime_state.updated_at = now.to_string();
    Ok(true)
}

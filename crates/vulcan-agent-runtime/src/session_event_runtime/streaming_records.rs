use super::*;

/// Appends or extends one streaming transcript record inside the background state.
/// 在后台状态中追加或扩展一条流式转录记录。
pub fn append_stream_delta_record_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    if let Some(record_id) = record_id_slot.as_deref()
        && !append_record_body_delta(&mut runtime_state.conversation_state, record_id, delta)
    {
        *record_id_slot = None;
    }
    if record_id_slot.is_none() {
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: record_id.clone(),
                kind: SessionMessageKind::Assistant,
                record_kind,
                visible: true,
                title: title.to_string(),
                body: delta.to_string(),
                metadata: collapsed_by_default.then(|| json!({ "collapsed_by_default": true })),
                created_at: now.to_string(),
            },
        )
        .map_err(|error| format!("Invalid streaming record identity: {error}"))?;
        *record_id_slot = Some(record_id);
    }
    runtime_state.updated_at = now.to_string();
    Ok(())
}

/// Keeps the active streaming thinking record collapsed after the thinking phase completes.
/// 在思考阶段完成后保持当前流式思考记录折叠。
pub fn collapse_streaming_thinking_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    record_ids: &SessionStreamingRecordIds,
) {
    let Some(thinking_record_id) = record_ids.thinking_record_id.as_deref() else {
        return;
    };
    runtime_state
        .ui_state
        .collapsed_message_ids
        .insert(thinking_record_id.to_string());
    runtime_state.updated_at = now.to_string();
}

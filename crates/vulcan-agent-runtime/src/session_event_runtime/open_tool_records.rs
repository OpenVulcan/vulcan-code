use super::*;

pub(super) fn complete_open_tool_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    display_state: &str,
    message: &str,
) {
    let open_records = std::mem::take(&mut runtime_state.tool_call_record_ids);
    for (tool_call_id, record_id) in open_records {
        let tool_name = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == record_id)
            .and_then(|record| tool_record_name_for_call_id(record, &tool_call_id));
        let tool_result_id = format!("tool-result-{tool_call_id}");
        let detail_ref = append_tool_result_detail_record(
            runtime_state,
            session_id,
            now,
            &tool_call_id,
            &tool_result_id,
            tool_name.as_deref(),
            display_state,
            None,
            message,
        );
        let metadata = normalize_tool_result_metadata(
            &tool_call_id,
            &tool_result_id,
            tool_name.as_deref(),
            display_state,
            None,
            detail_ref.as_ref(),
            true,
        );
        attach_tool_result_to_record(&mut runtime_state.conversation_state, &record_id, metadata);
    }
}

/// Finds the tool name stored on an earlier tool-call card for the same call identifier.
/// 查找同一调用标识早前工具调用卡片中保存的工具名称。
pub(super) fn tool_record_name_for_call_id(
    record: &SessionConversationRecord,
    tool_call_id: &str,
) -> Option<String> {
    if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
        return None;
    }
    let metadata = record.metadata.as_ref()?;
    let record_call_id = metadata
        .get("tool_call_id")
        .and_then(serde_json::Value::as_str)?;
    if record_call_id != tool_call_id {
        return None;
    }
    metadata
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

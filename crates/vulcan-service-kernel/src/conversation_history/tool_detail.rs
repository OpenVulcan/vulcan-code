//! Conversation tool detail loading helpers.
//! 会话工具详情加载辅助逻辑。

use super::*;

/// Loads one hidden tool-detail payload by stable record id or tool-call id fallback.
/// 按稳定记录标识加载隐藏工具详情载荷，并支持工具调用标识回退。
pub(crate) async fn service_session_tool_detail(
    state: SharedServiceBridgeState,
    session_id: String,
    record_id: Option<String>,
    tool_call_id: Option<String>,
    seq: Option<u64>,
    message_index: Option<usize>,
) -> Result<Value, String> {
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(payload) = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&parsed_session_id))
            .and_then(|runtime_state| {
                find_tool_detail_record(
                    runtime_state.conversation_state.records.iter(),
                    record_id.as_deref(),
                    tool_call_id.as_deref(),
                    message_index,
                )
                .map(|record| tool_detail_payload_from_session_record(record, seq, message_index))
            })
        {
            return Ok(payload);
        }
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let transcript_page = vulcan_workspace_store::workspace_store::load_session_transcript_window(
        controller_runtime,
        parsed_session_id,
        SELECTED_SESSION_TURN_LOAD_LIMIT,
    )
    .await?;
    let transcript_messages = transcript_page
        .messages
        .iter()
        .chain(transcript_page.tool_detail_messages.iter())
        .collect::<Vec<_>>();
    find_tool_detail_message(
        transcript_messages.iter().copied(),
        record_id.as_deref(),
        tool_call_id.as_deref(),
        message_index,
    )
    .map(|message| tool_detail_payload_from_session_message(message, seq, message_index))
    .ok_or_else(|| {
        record_id
            .map(|record_id| format!("tool detail record not found: {record_id}"))
            .unwrap_or_else(|| {
                format!(
                    "tool detail record not found for tool call: {}",
                    tool_call_id.unwrap_or_else(|| String::from("<missing>"))
                )
            })
    })
}

/// Finds a hidden tool-detail message by precise record id first, then by tool-call id.
/// 先按精确记录标识查找隐藏工具详情，再按工具调用标识回退查找。
pub(crate) fn find_tool_detail_message<'a>(
    messages: impl IntoIterator<Item = &'a SessionMessageEntry>,
    record_id: Option<&str>,
    tool_call_id: Option<&str>,
    message_index: Option<usize>,
) -> Option<&'a SessionMessageEntry> {
    let messages = messages.into_iter().collect::<Vec<_>>();
    if let Some(record_id) = record_id.filter(|value| !value.trim().is_empty()) {
        if let Some(message) = message_index
            .and_then(|index| messages.get(index).copied())
            .filter(|message| message.id == record_id)
        {
            return Some(message);
        }
        if let Some(message) = messages
            .iter()
            .copied()
            .find(|message| message.id == record_id)
        {
            return Some(message);
        }
    }
    let tool_call_id = tool_call_id?.trim();
    if tool_call_id.is_empty() {
        return None;
    }
    messages.iter().copied().find(|message| {
        matches!(
            message.record_kind,
            SessionMessageRecordKind::ToolDetailOutput | SessionMessageRecordKind::ToolResultOutput
        ) && session_message_tool_call_id(message) == Some(tool_call_id)
    })
}

/// Finds a hidden runtime conversation record by precise record id first, then by tool-call id.
/// 先按精确记录标识查找隐藏运行时会话记录，再按工具调用标识回退查找。
pub(crate) fn find_tool_detail_record<'a>(
    records: impl IntoIterator<Item = &'a SessionConversationRecord>,
    record_id: Option<&str>,
    tool_call_id: Option<&str>,
    message_index: Option<usize>,
) -> Option<&'a SessionConversationRecord> {
    let records = records.into_iter().collect::<Vec<_>>();
    if let Some(record_id) = record_id.filter(|value| !value.trim().is_empty()) {
        if let Some(record) = message_index
            .and_then(|index| records.get(index).copied())
            .filter(|record| record.id == record_id)
        {
            return Some(record);
        }
        if let Some(record) = records
            .iter()
            .copied()
            .find(|record| record.id == record_id)
        {
            return Some(record);
        }
    }
    let tool_call_id = tool_call_id?.trim();
    if tool_call_id.is_empty() {
        return None;
    }
    records.iter().copied().find(|record| {
        matches!(
            record.record_kind,
            SessionMessageRecordKind::ToolDetailOutput | SessionMessageRecordKind::ToolResultOutput
        ) && session_record_tool_call_id(record) == Some(tool_call_id)
    })
}

/// Builds the JSON payload returned to the WebUI detail modal.
/// 构建返回给 WebUI 详情弹窗的 JSON 载荷。
pub(crate) fn tool_detail_payload_from_session_message(
    message: &SessionMessageEntry,
    seq: Option<u64>,
    message_index: Option<usize>,
) -> Value {
    json!({
        "body": message.body.clone(),
        "metadata": message.metadata.clone(),
        "seq": seq,
        "message_index": message_index,
    })
}

/// Builds the JSON payload returned from an in-memory runtime conversation record.
/// 从内存态运行时会话记录构建返回给 WebUI 的 JSON 载荷。
pub(crate) fn tool_detail_payload_from_session_record(
    record: &SessionConversationRecord,
    seq: Option<u64>,
    message_index: Option<usize>,
) -> Value {
    json!({
        "body": record.body.clone(),
        "metadata": record.metadata.clone(),
        "seq": seq,
        "message_index": message_index,
    })
}

/// Extracts the stable tool-call id from session message metadata.
/// 从会话消息元数据中提取稳定工具调用标识。
pub(crate) fn session_message_tool_call_id(message: &SessionMessageEntry) -> Option<&str> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("tool_call_id")
                .or_else(|| metadata.get("toolCallId"))
        })
        .and_then(Value::as_str)
}

/// Extracts the stable tool-call id from runtime record metadata.
/// 从运行时记录元数据中提取稳定工具调用标识。
pub(crate) fn session_record_tool_call_id(record: &SessionConversationRecord) -> Option<&str> {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("tool_call_id")
                .or_else(|| metadata.get("toolCallId"))
        })
        .and_then(Value::as_str)
}

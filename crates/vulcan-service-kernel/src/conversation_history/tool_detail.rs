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
    let (controller_runtime, session_location) = {
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
        if let Some(payload) = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_messages.get(&parsed_session_id))
            .and_then(|messages| {
                find_tool_detail_message(
                    messages.iter(),
                    record_id.as_deref(),
                    tool_call_id.as_deref(),
                    message_index,
                )
                .map(|message| {
                    tool_detail_payload_from_session_message(message, seq, message_index)
                })
            })
        {
            return Ok(payload);
        }
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let session_location = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace_snapshot| {
                workspace_session_summary_for_session(workspace_snapshot, parsed_session_id)
            })
            .and_then(session_summary_project_and_created_at)
            .map(|(project_id, created_at)| (project_id.to_string(), created_at.to_string()));
        (controller_runtime, session_location)
    };
    let transcript_page = if let Some((project_id, created_at)) = session_location {
        Some(
            vulcan_workspace_store::workspace_store::load_session_transcript_window_for_project_created_at(
                controller_runtime,
                &project_id,
                &created_at,
                parsed_session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await?,
        )
    } else {
        None
    };
    let payload = transcript_page
        .as_ref()
        .and_then(|transcript_page| {
            find_tool_detail_message(
                transcript_page
                    .messages
                    .iter()
                    .chain(transcript_page.tool_detail_messages.iter()),
                record_id.as_deref(),
                tool_call_id.as_deref(),
                message_index,
            )
        })
        .map(|message| tool_detail_payload_from_session_message(message, seq, message_index));
    if let Some(payload) = payload {
        return Ok(payload);
    }
    Err(record_id
        .map(|record_id| format!("tool detail record not found: {record_id}"))
        .unwrap_or_else(|| {
            format!(
                "tool detail record not found for tool call: {}",
                tool_call_id.unwrap_or_else(|| String::from("<missing>"))
            )
        }))
}

/// Finds a hidden tool-detail message by precise record id first, then by tool-call id.
/// 先按精确记录标识查找隐藏工具详情，再按工具调用标识回退查找。
pub(crate) fn find_tool_detail_message<'a>(
    messages: impl IntoIterator<Item = &'a SessionMessageEntry>,
    record_id: Option<&str>,
    tool_call_id: Option<&str>,
    message_index: Option<usize>,
) -> Option<&'a SessionMessageEntry> {
    find_tool_detail_entry(messages, record_id, tool_call_id, message_index)
}

/// Finds a hidden runtime conversation record by precise record id first, then by tool-call id.
/// 先按精确记录标识查找隐藏运行时会话记录，再按工具调用标识回退查找。
pub(crate) fn find_tool_detail_record<'a>(
    records: impl IntoIterator<Item = &'a SessionConversationRecord>,
    record_id: Option<&str>,
    tool_call_id: Option<&str>,
    message_index: Option<usize>,
) -> Option<&'a SessionConversationRecord> {
    find_tool_detail_entry(records, record_id, tool_call_id, message_index)
}

/// Defines the common lookup surface shared by displayed messages and runtime records.
/// 定义显示消息与运行时记录共享的工具详情查找表面。
trait ToolDetailLookupEntry {
    /// Returns the stable row id used for direct record-id lookup.
    /// 返回用于直接 record-id 查找的稳定行标识。
    fn tool_detail_entry_id(&self) -> &str;

    /// Returns the transcript record kind used to gate tool-call fallback lookup.
    /// 返回用于限制 tool-call 回退查找的转录记录类型。
    fn tool_detail_record_kind(&self) -> SessionMessageRecordKind;

    /// Returns the stable tool-call id carried by row metadata.
    /// 返回行元数据携带的稳定工具调用标识。
    fn tool_detail_tool_call_id(&self) -> Option<&str>;
}

impl ToolDetailLookupEntry for SessionMessageEntry {
    fn tool_detail_entry_id(&self) -> &str {
        self.id.as_str()
    }

    fn tool_detail_record_kind(&self) -> SessionMessageRecordKind {
        self.record_kind
    }

    fn tool_detail_tool_call_id(&self) -> Option<&str> {
        session_message_tool_call_id(self)
    }
}

impl ToolDetailLookupEntry for SessionConversationRecord {
    fn tool_detail_entry_id(&self) -> &str {
        self.id.as_str()
    }

    fn tool_detail_record_kind(&self) -> SessionMessageRecordKind {
        self.record_kind
    }

    fn tool_detail_tool_call_id(&self) -> Option<&str> {
        session_record_tool_call_id(self)
    }
}

/// Finds one tool-detail entry by direct record id first, then by tool-call id.
/// 先按直接记录标识查找单条工具详情项，再按工具调用标识回退。
///
/// # Parameters
/// 参数
/// `entries` yields displayed messages or runtime records to inspect.
/// `entries` 产出需要检查的显示消息或运行时记录。
/// `record_id` is the optional direct row id supplied by WebUI.
/// `record_id` 是 WebUI 提供的可选直接行标识。
/// `tool_call_id` is the optional stable tool-call id fallback.
/// `tool_call_id` 是可选的稳定工具调用标识回退值。
/// `message_index` is an optional fast-path index into the supplied entries.
/// `message_index` 是指向输入项的可选快速路径索引。
///
/// # Returns
/// 返回值
/// The matching tool-detail entry when direct or fallback lookup succeeds.
/// 当直接查找或回退查找成功时返回匹配的工具详情项。
fn find_tool_detail_entry<'a, T>(
    entries: impl IntoIterator<Item = &'a T>,
    record_id: Option<&str>,
    tool_call_id: Option<&str>,
    message_index: Option<usize>,
) -> Option<&'a T>
where
    T: ToolDetailLookupEntry + 'a,
{
    let record_id = normalized_tool_detail_lookup_value(record_id);
    let tool_call_id = normalized_tool_detail_lookup_value(tool_call_id);
    let mut record_id_match = None;
    let mut tool_call_id_match = None;
    for (index, entry) in entries.into_iter().enumerate() {
        if let Some(record_id) = record_id {
            let entry_id_matches = entry.tool_detail_entry_id() == record_id;
            if message_index == Some(index) && entry_id_matches {
                return Some(entry);
            }
            if record_id_match.is_none() && entry_id_matches {
                record_id_match = Some(entry);
            }
        }
        if tool_call_id_match.is_none()
            && let Some(tool_call_id) = tool_call_id
            && tool_detail_record_kind_can_match_tool_call(entry.tool_detail_record_kind())
            && entry.tool_detail_tool_call_id() == Some(tool_call_id)
        {
            tool_call_id_match = Some(entry);
        }
    }
    record_id_match.or(tool_call_id_match)
}

/// Returns a trimmed lookup value when it is present and non-empty.
/// 当查找值存在且非空时返回去空白后的值。
///
/// # Parameters
/// 参数
/// `value` is the optional user-supplied lookup value.
/// `value` 是可选的用户提供查找值。
///
/// # Returns
/// 返回值
/// A non-empty trimmed lookup value.
/// 返回非空的去空白查找值。
fn normalized_tool_detail_lookup_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Returns whether one record kind can satisfy a tool-call fallback lookup.
/// 返回某记录类型是否可满足工具调用回退查找。
///
/// # Parameters
/// 参数
/// `record_kind` is the transcript record kind attached to the candidate row.
/// `record_kind` 是候选行携带的转录记录类型。
///
/// # Returns
/// 返回值
/// `true` when the row can expose a detail payload for a tool call.
/// 当该行可为工具调用暴露详情载荷时返回 `true`。
fn tool_detail_record_kind_can_match_tool_call(record_kind: SessionMessageRecordKind) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::ToolDetailOutput | SessionMessageRecordKind::ToolResultOutput
    )
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
    tool_call_id_from_metadata(message.metadata.as_ref())
}

/// Extracts the stable tool-call id from runtime record metadata.
/// 从运行时记录元数据中提取稳定工具调用标识。
pub(crate) fn session_record_tool_call_id(record: &SessionConversationRecord) -> Option<&str> {
    tool_call_id_from_metadata(record.metadata.as_ref())
}

/// Extracts the stable tool-call id from shared row metadata aliases.
/// 从共享行元数据别名中提取稳定工具调用标识。
///
/// # Parameters
/// 参数
/// `metadata` is optional row metadata from a displayed message or runtime record.
/// `metadata` 是显示消息或运行时记录上的可选行元数据。
///
/// # Returns
/// 返回值
/// The snake-case or camel-case tool-call id when present.
/// 返回存在时的 snake-case 或 camel-case 工具调用标识。
fn tool_call_id_from_metadata(metadata: Option<&Value>) -> Option<&str> {
    metadata
        .and_then(|metadata| {
            metadata
                .get("tool_call_id")
                .or_else(|| metadata.get("toolCallId"))
        })
        .and_then(Value::as_str)
}

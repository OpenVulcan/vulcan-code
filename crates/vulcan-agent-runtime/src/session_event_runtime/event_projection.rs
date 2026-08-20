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
        RuntimeEventKind::Retrying | RuntimeEventKind::ProviderRetryStatusChanged => {
            project_provider_retry_notice_to_state(
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

/// Creates one durable same-route Provider retry note or updates its terminal state in place.
/// 创建一条持久化的同路由 Provider 重试提示，或原位更新它的终态。
///
/// `runtime_state` owns the durable session conversation state.
/// `runtime_state` 持有持久化会话对话状态。
/// `session_id` identifies the conversation receiving the note.
/// `session_id` 标识接收备注的会话。
/// `now` is the persistence timestamp used by the new record.
/// `now` 是新记录使用的持久化时间戳。
/// `event` contains the exact transport title, detail, and diagnostic metadata.
/// `event` 包含精确的传输标题、详情与诊断元数据。
/// Returns `true` after the transport note is created or updated successfully.
/// 成功创建或更新传输备注后返回 `true`。
fn project_provider_retry_notice_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    if event.kind == RuntimeEventKind::ProviderRetryStatusChanged {
        return update_provider_retry_notice_to_state(runtime_state, now, event);
    }
    // Open request-stage cards belong to the failed physical attempt and cannot execute after replay begins.
    // 未闭合的请求阶段卡片属于失败的真实尝试，重放开始后不可能再执行。
    supersede_open_tool_request_records(runtime_state, now, "provider_retry_superseded");
    let mut metadata = match event.metadata.clone() {
        Some(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    metadata.insert(
        String::from("notice_kind"),
        serde_json::Value::String(String::from("provider_retry")),
    );
    metadata.insert(
        String::from("provider_retry_state"),
        serde_json::Value::String(String::from("retrying")),
    );
    // recordId gives every retry transition its own stable visible history row.
    // recordId 为每次重试转换提供独立、稳定的可见历史行。
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
            metadata: Some(serde_json::Value::Object(metadata)),
            created_at: now.to_string(),
        },
    )
    .map_err(|error| format!("Invalid Provider transport display-note identity: {error}"))?;
    runtime_state.updated_at = now.to_string();
    Ok(true)
}

/// Updates the latest pending Provider retry notice without appending another transcript row.
/// 原位更新最近一条待定 Provider 重试提示，而不追加新的转录记录。
///
/// `runtime_state` owns the retained retry display records.
/// `runtime_state` 持有已保留的重试展示记录。
/// `now` is the exact physical-request terminal timestamp.
/// `now` 是真实请求终止的精确时间戳。
/// `event` carries the closed retry display state produced by the request boundary.
/// `event` 携带由请求边界生成的已闭合重试展示状态。
/// Returns `true` only when one pending retry record was updated.
/// 仅在一条待定重试记录被更新时返回 `true`。
fn update_provider_retry_notice_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    let state = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("provider_retry_state"))
        .and_then(serde_json::Value::as_str)
        .filter(|state| matches!(*state, "succeeded" | "failed"))
        .ok_or_else(|| String::from("Provider retry status update is missing a terminal state."))?;
    let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .rev()
        .find(|record| {
            record.record_kind == SessionMessageRecordKind::DisplayNote
                && record.metadata.as_ref().is_some_and(|metadata| {
                    metadata
                        .get("notice_kind")
                        .and_then(serde_json::Value::as_str)
                        == Some("provider_retry")
                        && metadata
                            .get("provider_retry_state")
                            .and_then(serde_json::Value::as_str)
                            == Some("retrying")
                })
        })
    else {
        return Ok(false);
    };
    let Some(serde_json::Value::Object(metadata)) = record.metadata.as_mut() else {
        return Ok(false);
    };
    metadata.insert(
        String::from("provider_retry_state"),
        serde_json::Value::String(state.to_string()),
    );
    metadata.insert(
        String::from("provider_retry_completed_at"),
        serde_json::Value::String(now.to_string()),
    );
    record.title = event.title.clone();
    runtime_state.updated_at = now.to_string();
    Ok(true)
}

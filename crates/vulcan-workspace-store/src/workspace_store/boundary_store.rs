use super::*;

/// Appends a pre-prompt turn-context anchor to the segmented session stream when available.
/// 可用时在分段会话流中追加用户提示词前的轮次上下文锚点。
pub fn snapshot_segmented_session_context_before_user_prompt(
    session_directory: &std::path::Path,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    if state_document.current_context.is_null() {
        return Ok(());
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    append_segmented_session_events(
        session_directory,
        &[SegmentedSessionEventInput {
            timestamp: current_unix_millis_text(),
            kind: SegmentedSessionEventKind::TurnContext,
            payload: serde_json::json!({
                "reason": "before_user_prompt",
                "current_context": state_document.current_context,
                "runtime_state": state_document.runtime_state,
                "source_updated_at": state_document.updated_at,
            }),
            turn_index: None,
        }],
    )?;
    Ok(())
}

/// session boundary store adapter backed by one workspace segmented session directory.
/// 由单个工作区分段会话目录支撑的 会话边界存储适配器。
#[derive(Debug, Clone)]
pub struct WorkspaceSegmentedSessionBoundaryStore {
    /// Exact pre-resolved segmented session directory.
    /// 预先解析的精确分段会话目录。
    session_directory: std::path::PathBuf,
    /// Owning session identifier.
    /// 所属会话标识。
    session_id: SessionId,
}

impl WorkspaceSegmentedSessionBoundaryStore {
    /// Creates one session boundary store adapter for a workspace segmented session.
    /// 为工作区分段会话创建一个 会话边界存储适配器。
    pub fn new(session_directory: impl Into<std::path::PathBuf>, session_id: SessionId) -> Self {
        Self {
            session_directory: session_directory.into(),
            session_id,
        }
    }
}

impl SessionBoundaryStore for WorkspaceSegmentedSessionBoundaryStore {
    /// Appends one session boundary event into the segmented session stream.
    /// 向分段会话流追加一条 会话边界事件。
    fn append_event(&self, write: SessionBoundaryStorageWrite) -> Result<(), String> {
        if write.thread_id != SessionNodeId::from(self.session_id) {
            return Err(format!(
                "Session boundary write thread `{}` does not match workspace session `{}`.",
                write.thread_id, self.session_id
            ));
        }
        validate_exact_segmented_session_directory_owner(&self.session_directory, self.session_id)?;
        append_segmented_boundary_events(
            &self.session_directory,
            std::slice::from_ref(&write.event),
        )?;
        Ok(())
    }

    /// Reads session boundary events from the segmented session stream.
    /// 从分段会话流读取 会话边界事件。
    fn read_events(&self, thread_id: SessionNodeId) -> Result<Vec<SessionBoundaryEvent>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(Vec::new());
        }
        load_segmented_session_boundary_events(
            &self.session_directory,
            self.session_id,
            None,
            usize::MAX,
        )
    }
}

impl SessionBoundaryStateStore for WorkspaceSegmentedSessionBoundaryStore {
    /// Replaces only the session lifecycle runner state inside state.json.
    /// 仅覆盖 state.json 内的 session lifecycle runner 状态。
    fn replace_state(&self, thread_id: SessionNodeId, state: Value) -> Result<(), String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Err(format!(
                "Session boundary state thread `{}` does not match workspace session `{}`.",
                thread_id, self.session_id
            ));
        }
        validate_exact_segmented_session_directory_owner(&self.session_directory, self.session_id)?;
        replace_session_boundary_state(&self.session_directory, state)
    }

    /// Loads the session lifecycle runner state from state.json.
    /// 从 state.json 读取 session lifecycle runner 状态。
    fn load_state(&self, thread_id: SessionNodeId) -> Result<Option<Value>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(None);
        }
        validate_exact_segmented_session_directory_owner(&self.session_directory, self.session_id)?;
        load_session_boundary_state(&self.session_directory)
    }
}

/// Appends accepted session boundary events into the segmented session stream.
/// 将已接受的 会话边界事件追加到分段会话流。
pub fn append_segmented_session_boundary_events(
    session_directory: &std::path::Path,
    session_id: SessionId,
    events: &[SessionBoundaryEvent],
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    append_segmented_boundary_events(session_directory, events)?;
    Ok(())
}

/// Loads session boundary events from one segmented session window.
/// 从一个分段会话窗口读取 会话边界事件。
pub fn load_segmented_session_boundary_events(
    session_directory: &std::path::Path,
    session_id: SessionId,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SessionBoundaryEvent>, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    read_segmented_session_event_window(session_directory, before_seq, limit)?
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .map(session_boundary_event_from_segment_line)
        .collect()
}

/// Rebuilds one session boundary event from a segmented JSONL line.
/// 从分段 JSONL 行重建一条 会话边界事件。
pub(super) fn session_boundary_event_from_segment_line(
    line: &self::segmented_session_store::SegmentedSessionEventLine,
) -> Result<SessionBoundaryEvent, String> {
    let payload = &line.payload;
    let event_id = required_string(payload, "event_id")?;
    let thread_id = serde_json::from_value::<SessionNodeId>(required_value(payload, "thread_id")?)
        .map_err(|error| format!("invalid session boundary thread_id: {error}"))?;
    let kind = serde_json::from_value::<SessionBoundaryEventKind>(required_value(payload, "kind")?)
        .map_err(|error| format!("invalid session boundary kind: {error}"))?;
    let scope = serde_json::from_value::<SessionEventScope>(required_value(payload, "scope")?)
        .map_err(|error| format!("invalid session boundary scope: {error}"))?;
    let visibility =
        serde_json::from_value::<SessionEventVisibility>(required_value(payload, "visibility")?)
            .map_err(|error| format!("invalid session boundary visibility: {error}"))?;
    let timestamp_ms = line.timestamp.parse::<u64>().unwrap_or_default();
    Ok(SessionBoundaryEvent {
        event_id,
        thread_id,
        turn_id: optional_string(payload, "turn_id"),
        item_id: optional_string(payload, "item_id"),
        sequence: payload
            .get("sequence")
            .and_then(Value::as_u64)
            .unwrap_or(line.seq),
        kind,
        scope,
        visibility,
        timestamp_ms,
        payload: payload.get("payload").cloned().unwrap_or(Value::Null),
    })
}

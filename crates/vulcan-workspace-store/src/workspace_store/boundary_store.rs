use super::*;

/// Appends a pre-prompt turn-context anchor to the segmented session stream when available.
/// 可用时在分段会话流中追加用户提示词前的轮次上下文锚点。
pub fn snapshot_segmented_session_context_before_user_prompt(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    if state_document.current_context.is_null() {
        return Ok(());
    }
    let Some(session_dir) =
        find_existing_segmented_context_session_dir(data_root, project_id, session_id)?
    else {
        return Ok(());
    };
    append_segmented_session_events(
        &session_dir,
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
    /// Vulcan code data root.
    /// Vulcan Code 数据根目录。
    data_root: std::path::PathBuf,
    /// Owning project identifier and project tag.
    /// 所属项目标识与项目标签。
    project_id: String,
    /// Owning session identifier.
    /// 所属会话标识。
    session_id: SessionId,
    /// Human-readable session title used when the directory must be created.
    /// 需要创建目录时使用的人类可读会话标题。
    session_title: String,
}

impl WorkspaceSegmentedSessionBoundaryStore {
    /// Creates one session boundary store adapter for a workspace segmented session.
    /// 为工作区分段会话创建一个 会话边界存储适配器。
    pub fn new(
        data_root: impl Into<std::path::PathBuf>,
        project_id: impl Into<String>,
        session_id: SessionId,
        session_title: impl Into<String>,
    ) -> Self {
        Self {
            data_root: data_root.into(),
            project_id: project_id.into(),
            session_id,
            session_title: session_title.into(),
        }
    }

    /// Resolves or creates the backing segmented session directory.
    /// 解析或创建背后的分段会话目录。
    fn ensure_session_dir(&self) -> Result<std::path::PathBuf, String> {
        ensure_session_boundary_dir(
            &self.data_root,
            &self.project_id,
            self.session_id,
            &self.session_title,
        )
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
        let session_dir = self.ensure_session_dir()?;
        append_segmented_boundary_events(&session_dir, std::slice::from_ref(&write.event))?;
        Ok(())
    }

    /// Reads session boundary events from the segmented session stream.
    /// 从分段会话流读取 会话边界事件。
    fn read_events(&self, thread_id: SessionNodeId) -> Result<Vec<SessionBoundaryEvent>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(Vec::new());
        }
        load_segmented_session_boundary_events(
            &self.data_root,
            &self.project_id,
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
        let session_dir = self.ensure_session_dir()?;
        replace_session_boundary_state(&session_dir, state)
    }

    /// Loads the session lifecycle runner state from state.json.
    /// 从 state.json 读取 session lifecycle runner 状态。
    fn load_state(&self, thread_id: SessionNodeId) -> Result<Option<Value>, String> {
        if thread_id != SessionNodeId::from(self.session_id) {
            return Ok(None);
        }
        let session_id_text = self.session_id.to_string();
        let Some(session_dir) = find_segmented_session_directory(
            &self.data_root,
            false,
            &self.project_id,
            &session_id_text,
        )?
        else {
            return Ok(None);
        };
        load_session_boundary_state(&session_dir)
    }
}

/// Appends accepted session boundary events into the segmented session stream.
/// 将已接受的 会话边界事件追加到分段会话流。
pub fn append_segmented_session_boundary_events(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
    events: &[SessionBoundaryEvent],
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let session_dir =
        ensure_session_boundary_dir(data_root, project_id, session_id, session_title)?;
    append_segmented_boundary_events(&session_dir, events)?;
    Ok(())
}

/// Resolves or creates the segmented session directory used by session boundary storage.
/// 解析或创建 会话边界存储使用的分段会话目录。
pub(super) fn ensure_session_boundary_dir(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
) -> Result<std::path::PathBuf, String> {
    let session_id_text = session_id.to_string();
    let now = current_unix_millis_text();
    match find_segmented_session_directory(data_root, false, project_id, &session_id_text)? {
        Some(session_dir) => Ok(session_dir),
        None => create_segmented_session_in_location(
            data_root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id_text,
                project_tag: project_id.to_string(),
                created_date: segmented_session_date_from_unix_millis_text(&now),
                created_at: now,
                project_id: Some(project_id.to_string()),
                title: Some(session_title.to_string()),
                title_source: Some(String::from("session_boundary")),
                segment_policy: SegmentPolicy::default(),
            },
        ),
    }
}

/// Loads session boundary events from one segmented session window.
/// 从一个分段会话窗口读取 会话边界事件。
pub fn load_segmented_session_boundary_events(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SessionBoundaryEvent>, String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) =
        find_segmented_session_directory(data_root, false, project_id, &session_id_text)?
    else {
        return Ok(Vec::new());
    };
    read_segmented_session_event_window(&session_dir, before_seq, limit)?
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .map(session_boundary_event_from_segment_line)
        .collect()
}

/// Updates display metadata for one segmented session without rewriting transcript segments.
/// 仅更新单个分段会话的展示元数据，不重写转录分段。
pub fn rename_segmented_session_metadata(
    data_root: &std::path::Path,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let Some(session_dir) = find_segmented_session_directory_by_id(data_root, &session_id_text)?
    else {
        return Err(format!("未找到分段会话目录：{session_id_text}"));
    };
    patch_segmented_session_metadata(
        &session_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(updated_at.to_string()),
            title: Some(Some(title.to_string())),
            title_source: Some(Some("manual".to_string())),
            archived_at: None,
        },
    )?;
    Ok(())
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

/// Finds the active or archived segmented session directory used by context persistence.
/// 查找上下文持久化使用的活动或归档分段会话目录。
pub(super) fn find_existing_segmented_context_session_dir(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<std::path::PathBuf>, String> {
    match find_segmented_session_directory(data_root, false, project_id, &session_id.to_string())? {
        Some(session_dir) => Ok(Some(session_dir)),
        None => {
            find_segmented_session_directory(data_root, true, project_id, &session_id.to_string())
        }
    }
}

/// Converts Unix-milliseconds text into the UTC date segment used by segmented session directories.
/// 将 Unix 毫秒文本转换为分段会话目录使用的 UTC 日期片段。
pub(super) fn segmented_session_date_from_unix_millis_text(value: &str) -> String {
    let millis = value.trim().parse::<i64>().unwrap_or_default();
    let seconds = millis.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Converts days since the Unix epoch into one proleptic Gregorian UTC date.
/// 将 Unix 纪元以来的天数转换为一个前推格里高利 UTC 日期。
pub(super) fn civil_date_from_unix_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_unix_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

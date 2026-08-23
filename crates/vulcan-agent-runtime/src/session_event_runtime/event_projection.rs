use super::*;

/// Projects one runtime event into visible transcript records when the event has user-facing detail.
/// 当运行时事件包含面向用户的细节时，将其投影为可见转录记录。
pub fn append_runtime_event_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    // ownedRuntimeState transfers the exact row into the rollback-capable reducer without cloning history.
    // ownedRuntimeState 将精确行转移到支持回滚的归约器中，不复制历史。
    let owned_runtime_state = std::mem::take(runtime_state);
    let (projected_runtime_state, changed_record_ids) =
        match append_runtime_event_projection_to_owned_state(
            owned_runtime_state,
            session_id,
            now,
            event,
        ) {
            Ok(projected) => projected,
            Err((restored_runtime_state, error)) => {
                *runtime_state = restored_runtime_state;
                return Err(error);
            }
        };
    // projected reports whether this event kind produced a durable projection in the detached draft.
    // projected 表示该事件类型是否在分离草稿中产出了持久投影。
    let projected = !changed_record_ids.is_empty();
    *runtime_state = projected_runtime_state;
    Ok(projected)
}

/// Projects one runtime event into an already-detached owned state without cloning its transcript.
/// 将单个运行时事件投影到已经分离的自有状态中，且不克隆其转录。
///
/// # Parameters
/// 参数
/// `runtime_state` is an owned detached state returned in restored form on projection failure.
/// `runtime_state` 是分离后的自有状态；投影失败时会以已恢复形式返回。
/// `session_id` identifies the owning conversation session.
/// `session_id` 标识所属会话。
/// `now` is the exact projection timestamp.
/// `now` 是精确的投影时间戳。
/// `event` is the normalized runtime event to project.
/// `event` 是待投影的规范化运行时事件。
///
/// # Returns
/// 返回值
/// Returns the updated owned state and exact changed ids, or the restored owned state with an error.
/// 返回更新后的自有状态与精确变化标识，或返回已恢复的自有状态及错误。
pub fn append_runtime_event_projection_to_owned_state(
    mut runtime_state: SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<(SessionRuntimeStateEntry, Vec<String>), (SessionRuntimeStateEntry, String)> {
    // rollback captures only open tool cards and the latest retry note, never the complete transcript.
    // rollback 仅捕获开放工具卡片与最近重试提示，绝不复制完整转录。
    let rollback = RuntimeEventProjectionRollback::capture(&runtime_state);
    let projection_result = match event.kind {
        RuntimeEventKind::ToolRequestStarted => append_tool_request_started_projection_to_state(
            &mut runtime_state,
            session_id,
            now,
            event,
        ),
        RuntimeEventKind::ToolRequestProgress
        | RuntimeEventKind::ToolRequestPreviewUpdated
        | RuntimeEventKind::ToolRequestReady => {
            update_tool_request_projection_to_state(&mut runtime_state, now, event)
        }
        RuntimeEventKind::ToolRequestInvalid | RuntimeEventKind::ToolRequestAborted => {
            abort_tool_request_projection_to_state(&mut runtime_state, now, event)
        }
        RuntimeEventKind::ToolStarted => {
            append_tool_started_projection_to_state(&mut runtime_state, session_id, now, event)
        }
        RuntimeEventKind::ToolFinished => {
            append_tool_finished_projection_to_state(&mut runtime_state, session_id, now, event)
        }
        RuntimeEventKind::Retrying | RuntimeEventKind::ProviderRetryStatusChanged => {
            project_provider_retry_notice_to_state(&mut runtime_state, session_id, now, event)
        }
        _ => Ok(Vec::new()),
    };
    match projection_result {
        Ok(changed_record_ids) => Ok((runtime_state, changed_record_ids)),
        Err(error) => {
            rollback.restore(&mut runtime_state);
            Err((runtime_state, error))
        }
    }
}

/// Minimal rollback state for one fallible runtime-event projection.
/// 单次可失败运行时事件投影使用的最小回滚状态。
struct RuntimeEventProjectionRollback {
    /// Original record-list length used to discard event-local appends.
    /// 用于丢弃事件局部追加项的原始记录列表长度。
    record_count: usize,
    /// Exact mutable records reachable through open correlations or the retry-note selector.
    /// 可由开放关联或重试提示选择器触达的精确可变记录。
    records: Vec<(usize, SessionConversationRecord)>,
    /// Original open canonical tool-call correlations.
    /// 原始开放规范工具调用关联。
    tool_call_record_ids: std::collections::BTreeMap<String, String>,
    /// Original open streamed tool-request correlations.
    /// 原始开放流式工具请求关联。
    tool_request_record_ids: std::collections::BTreeMap<String, String>,
    /// Original runtime update timestamp.
    /// 原始运行态更新时间戳。
    updated_at: String,
}

impl RuntimeEventProjectionRollback {
    /// Captures only records that a runtime tool or retry event can mutate in place.
    /// 仅捕获运行时工具或重试事件可能原位修改的记录。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the owned session state before the event is applied.
    /// `runtime_state` 是应用事件前的自有会话状态。
    ///
    /// # Returns
    /// 返回值
    /// A bounded rollback snapshot independent from retained conversation length.
    /// 返回与保留会话长度无关的有界回滚快照。
    fn capture(runtime_state: &SessionRuntimeStateEntry) -> Self {
        // mutableRecordIds is bounded by currently open tool correlations plus one retry note.
        // mutableRecordIds 由当前开放工具关联及一条重试提示限制总量。
        let mut mutable_record_ids = BTreeSet::new();
        mutable_record_ids.extend(runtime_state.tool_call_record_ids.values().cloned());
        mutable_record_ids.extend(runtime_state.tool_request_record_ids.values().cloned());
        if let Some(retry_record_id) = runtime_state
            .conversation_state
            .records
            .iter()
            .rev()
            .find(|record| {
                record.record_kind == SessionMessageRecordKind::DisplayNote
                    && record.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("notice_kind")
                            .and_then(serde_json::Value::as_str)
                            == Some("provider_retry")
                    })
            })
            .map(|record| record.id.clone())
        {
            mutable_record_ids.insert(retry_record_id);
        }
        // records clones only exact mutable rows; the immutable transcript prefix stays owned once.
        // records 仅复制精确可变行；不可变转录前缀始终只保留一份所有权。
        let records = runtime_state
            .conversation_state
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| mutable_record_ids.contains(&record.id))
            .map(|(index, record)| (index, record.clone()))
            .collect();
        Self {
            record_count: runtime_state.conversation_state.records.len(),
            records,
            tool_call_record_ids: runtime_state.tool_call_record_ids.clone(),
            tool_request_record_ids: runtime_state.tool_request_record_ids.clone(),
            updated_at: runtime_state.updated_at.clone(),
        }
    }

    /// Restores the exact mutable surface after a fallible event reducer rejects its append.
    /// 在可失败事件归约器拒绝追加后恢复精确可变表面。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the same owned state that received the failed projection attempt.
    /// `runtime_state` 是接收失败投影尝试的同一份自有状态。
    fn restore(self, runtime_state: &mut SessionRuntimeStateEntry) {
        runtime_state
            .conversation_state
            .records
            .truncate(self.record_count);
        for (index, record) in self.records {
            if let Some(slot) = runtime_state.conversation_state.records.get_mut(index) {
                *slot = record;
            }
        }
        runtime_state.tool_call_record_ids = self.tool_call_record_ids;
        runtime_state.tool_request_record_ids = self.tool_request_record_ids;
        runtime_state.updated_at = self.updated_at;
    }
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
/// Returns the exact stable records created or updated by this retry transition.
/// 返回本次重试转换创建或更新的精确稳定记录。
fn project_provider_retry_notice_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    if event.kind == RuntimeEventKind::ProviderRetryStatusChanged {
        return update_provider_retry_notice_to_state(runtime_state, now, event);
    }
    // Open request-stage cards belong to the failed physical attempt and cannot execute after replay begins.
    // 未闭合的请求阶段卡片属于失败的真实尝试，重放开始后不可能再执行。
    let mut changed_record_ids =
        supersede_open_tool_request_records(runtime_state, now, "provider_retry_superseded")
            .into_iter()
            .collect::<Vec<_>>();
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
            id: record_id.clone(),
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
    changed_record_ids.push(record_id);
    Ok(changed_record_ids)
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
/// Returns the exact pending retry record updated by this terminal transition.
/// 返回本次终态转换更新的精确待定重试记录。
fn update_provider_retry_notice_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
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
        return Ok(Vec::new());
    };
    let record_id = record.id.clone();
    let Some(serde_json::Value::Object(metadata)) = record.metadata.as_mut() else {
        return Ok(Vec::new());
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
    Ok(vec![record_id])
}

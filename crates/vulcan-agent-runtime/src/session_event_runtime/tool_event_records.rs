use super::*;

/// Stable identity and revision extracted from one safe tool-request runtime event.
/// 从一条安全工具请求运行时事件中提取的稳定身份与修订信息。
#[derive(Debug)]
struct ToolRequestEventIdentity {
    /// Stable provider-request stream identifier used before execution.
    /// 执行前使用的稳定 Provider 请求流标识。
    request_stream_id: String,
    /// Current canonical or temporary tool-call identifier.
    /// 当前规范或临时工具调用标识。
    tool_call_id: String,
    /// Exact public tool name resolved against the frozen tool snapshot.
    /// 针对冻结工具快照解析出的精确公开工具名称。
    tool_name: String,
    /// Monotonic request projection revision.
    /// 单调递增的请求投影修订号。
    revision: u64,
}

/// Creates one visible tool card as soon as a uniquely resolved request is detected.
/// 一旦检测到唯一解析的工具请求就创建一张可见工具卡片。
///
/// # Parameters
/// 参数
/// `runtime_state` owns durable transcript records and request correlations.
/// `runtime_state` 持有持久转录记录与请求关联。
/// `session_id` identifies the owning conversation session.
/// `session_id` 标识所属会话。
/// `now` is the persistence timestamp used for a newly created record.
/// `now` 是新建记录使用的持久化时间戳。
/// `event` is the safe request-start event emitted by the stream coordinator.
/// `event` 是流协调器发出的安全请求开始事件。
///
/// # Returns
/// 返回值
/// The exact correlated card id when the event created or advanced it.
/// 当事件创建或推进关联卡片时返回其精确标识。
pub(super) fn append_tool_request_started_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    // identity is accepted only when every required coordinator-owned field is present.
    // identity 仅在协调器负责的所有必需字段都存在时才会被接受。
    let Some(identity) = parse_tool_request_event_identity(event) else {
        return Ok(Vec::new());
    };
    // existingRecordId preserves one-card semantics for an exact logical call correlation.
    // existingRecordId 为精确逻辑调用关联保持单卡片语义。
    let existing_record_id = runtime_state
        .tool_request_record_ids
        .get(&identity.request_stream_id)
        .or_else(|| {
            runtime_state
                .tool_call_record_ids
                .get(&identity.tool_call_id)
        })
        .cloned();
    if let Some(record_id) = existing_record_id {
        let advanced = merge_tool_request_event_into_record(
            &mut runtime_state.conversation_state,
            &record_id,
            event,
            identity.revision,
        );
        if advanced {
            // Older physical-request aliases are replaced only after the incoming revision wins.
            // 仅当传入修订胜出后才替换较旧的物理请求别名。
            runtime_state
                .tool_request_record_ids
                .retain(|_, mapped_record_id| mapped_record_id != &record_id);
            runtime_state
                .tool_request_record_ids
                .insert(identity.request_stream_id, record_id.clone());
            runtime_state.updated_at = now.to_string();
        }
        return Ok(advanced.then_some(record_id).into_iter().collect());
    }

    // recordId is stable for the lifetime of request generation and real execution.
    // recordId 在请求生成及真实执行的整个生命周期内保持稳定。
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    // metadata is already content-safe and is normalized to the authoritative identity.
    // metadata 已具备内容安全性，并会规范到权威身份。
    let metadata = normalize_tool_call_metadata(
        event.metadata.clone(),
        &identity.tool_call_id,
        &identity.tool_name,
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(metadata),
            created_at: now.to_string(),
        },
    )
    .map_err(|error| format!("Invalid tool-request record identity: {error}"))?;
    runtime_state
        .tool_request_record_ids
        .insert(identity.request_stream_id, record_id.clone());
    runtime_state
        .tool_call_record_ids
        .insert(identity.tool_call_id, record_id.clone());
    runtime_state.updated_at = now.to_string();
    Ok(vec![record_id])
}

/// Applies a progress or ready snapshot to the exact request-stream card.
/// 将进度或就绪快照应用到精确请求流卡片。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the request and canonical call correlations.
/// `runtime_state` 持有请求关联与规范调用关联。
/// `now` is the persistence timestamp for an accepted revision.
/// `now` 是已接受修订的持久化时间戳。
/// `event` is one coordinator-normalized progress or ready event.
/// `event` 是一条由协调器规范化的进度或就绪事件。
///
/// # Returns
/// 返回值
/// The exact existing card id when a newer revision was applied.
/// 当较新修订被应用时返回已有卡片的精确标识。
pub(super) fn update_tool_request_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    // identity supplies both the temporary stream key and the final call key.
    // identity 同时提供临时请求流键与最终调用键。
    let Some(identity) = parse_tool_request_event_identity(event) else {
        return Ok(Vec::new());
    };
    // recordId must come from the exact request stream; no same-name fallback is allowed.
    // recordId 必须来自精确请求流，不允许同名回退。
    let Some(record_id) = runtime_state
        .tool_request_record_ids
        .get(&identity.request_stream_id)
        .cloned()
    else {
        return Ok(Vec::new());
    };
    // previousCallId is captured before metadata merge so canonical promotion can be atomic.
    // previousCallId 在元数据合并前捕获，以便原子提升规范关联。
    let previous_call_id = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == record_id)
        .and_then(|record| record.metadata.as_ref())
        .and_then(|metadata| metadata.get("tool_call_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let advanced = merge_tool_request_event_into_record(
        &mut runtime_state.conversation_state,
        &record_id,
        event,
        identity.revision,
    );
    if !advanced {
        return Ok(Vec::new());
    }
    if event.kind == RuntimeEventKind::ToolRequestReady {
        if let Some(previous_call_id) = previous_call_id
            && runtime_state.tool_call_record_ids.get(&previous_call_id) == Some(&record_id)
        {
            runtime_state.tool_call_record_ids.remove(&previous_call_id);
        }
        runtime_state
            .tool_call_record_ids
            .insert(identity.tool_call_id, record_id.clone());
        // request correlation remains until ToolStarted so a late final background token estimate can update the ready card.
        // 请求关联会保留到 ToolStarted，使稍晚到达的最终后台 Token 估算仍可更新就绪卡片。
    }
    runtime_state.updated_at = now.to_string();
    Ok(vec![record_id])
}

/// Applies one exact pre-execution terminal state and releases its request correlations.
/// 应用一项精确的执行前终态，并释放其请求关联。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the card and its open correlation maps.
/// `runtime_state` 持有卡片及其开放关联映射。
/// `now` is the persistence timestamp for the terminal request state.
/// `now` 是请求终态的持久化时间戳。
/// `event` is the coordinator-normalized invalid or abort event.
/// `event` 是协调器规范化后的无效或中止事件。
///
/// # Returns
/// 返回值
/// The exact active request card id when it was closed.
/// 当精确活动请求被关闭时返回其卡片标识。
pub(super) fn abort_tool_request_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    // identity carries the only permitted lookup key for a request-stage abort.
    // identity 携带请求阶段中止允许使用的唯一查找键。
    let Some(identity) = parse_tool_request_event_identity(event) else {
        return Ok(Vec::new());
    };
    // recordId remains correlated until the terminal revision is proven newer than the visible card.
    // recordId 会持续保持关联，直到终态修订被证明比可见卡片更新。
    let Some(record_id) = runtime_state
        .tool_request_record_ids
        .get(&identity.request_stream_id)
        .cloned()
    else {
        return Ok(Vec::new());
    };
    let advanced = merge_tool_request_event_into_record(
        &mut runtime_state.conversation_state,
        &record_id,
        event,
        identity.revision,
    );
    if !advanced {
        return Ok(Vec::new());
    }
    if runtime_state
        .tool_request_record_ids
        .get(&identity.request_stream_id)
        == Some(&record_id)
    {
        runtime_state
            .tool_request_record_ids
            .remove(&identity.request_stream_id);
    }
    if runtime_state
        .tool_call_record_ids
        .get(&identity.tool_call_id)
        == Some(&record_id)
    {
        runtime_state
            .tool_call_record_ids
            .remove(&identity.tool_call_id);
    }
    runtime_state.updated_at = now.to_string();
    Ok(vec![record_id])
}

pub(super) fn append_tool_started_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    let Some((tool_call_id, tool_name)) = parse_tool_started_runtime_event_title(&event.title)
    else {
        return Ok(Vec::new());
    };
    // existingRecordId upgrades the request-stage card instead of appending a duplicate card.
    // existingRecordId 会升级请求阶段卡片，而不是追加重复卡片。
    if let Some(existing_record_id) = runtime_state
        .tool_call_record_ids
        .get(&tool_call_id)
        .cloned()
    {
        let mut metadata =
            normalize_tool_call_metadata(event.metadata.clone(), &tool_call_id, &tool_name);
        if let serde_json::Value::Object(object) = &mut metadata {
            object.insert(
                String::from("tool_display_state"),
                serde_json::Value::String(String::from("running")),
            );
        }
        let attached = attach_tool_result_to_record(
            &mut runtime_state.conversation_state,
            &existing_record_id,
            metadata,
        );
        if attached {
            runtime_state
                .tool_request_record_ids
                .retain(|_, record_id| record_id != &existing_record_id);
            runtime_state.updated_at = now.to_string();
        }
        return Ok(attached.then_some(existing_record_id).into_iter().collect());
    }
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    let metadata = normalize_tool_call_metadata(event.metadata.clone(), &tool_call_id, &tool_name);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(metadata),
            created_at: now.to_string(),
        },
    )
    .map_err(|error| format!("Invalid tool-start record identity: {error}"))?;
    runtime_state
        .tool_call_record_ids
        .insert(tool_call_id, record_id.clone());
    runtime_state.updated_at = now.to_string();
    Ok(vec![record_id])
}

/// Parses the exact safe metadata contract emitted by the tool-request stream coordinator.
/// 解析工具请求流协调器发出的精确安全元数据契约。
///
/// # Parameters
/// 参数
/// `event` is one tool-request lifecycle event.
/// `event` 是一条工具请求生命周期事件。
///
/// # Returns
/// 返回值
/// A complete identity when every required field has the expected type and non-empty value.
/// 当所有必需字段类型正确且非空时返回完整身份。
fn parse_tool_request_event_identity(event: &RuntimeEvent) -> Option<ToolRequestEventIdentity> {
    // metadata is the sole authoritative identity source for request-stage events.
    // metadata 是请求阶段事件唯一的权威身份来源。
    let metadata = event.metadata.as_ref()?;
    // requestStreamId uniquely selects one physical request stream.
    // requestStreamId 唯一选择一条物理请求流。
    let request_stream_id = metadata
        .get("request_stream_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    // toolCallId correlates the card with eventual host execution.
    // toolCallId 将卡片与最终宿主执行关联。
    let tool_call_id = metadata
        .get("tool_call_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    // toolName is already resolved against the frozen provider tool definitions.
    // toolName 已针对冻结的 Provider 工具定义完成解析。
    let tool_name = metadata
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    // revision provides monotonic stale-update rejection.
    // revision 提供单调的过期更新拒绝能力。
    let revision = metadata
        .get("request_revision")
        .and_then(serde_json::Value::as_u64)?;
    Some(ToolRequestEventIdentity {
        request_stream_id,
        tool_call_id,
        tool_name,
        revision,
    })
}

/// Merges one newer safe request snapshot into an existing visible tool card.
/// 将一份较新的安全请求快照合并到已有可见工具卡片。
///
/// # Parameters
/// 参数
/// `state` owns the durable conversation records.
/// `state` 持有持久会话记录。
/// `record_id` selects the exact card established by correlation maps.
/// `record_id` 选择由关联映射建立的精确卡片。
/// `event` carries only coordinator-approved safe metadata.
/// `event` 仅携带协调器批准的安全元数据。
/// `incoming_revision` is the monotonic revision to compare before mutation.
/// `incoming_revision` 是变更前用于比较的单调修订号。
///
/// # Returns
/// 返回值
/// `true` when the record existed and the incoming revision was newer.
/// 当记录存在且传入修订较新时返回 `true`。
fn merge_tool_request_event_into_record(
    state: &mut SessionConversationState,
    record_id: &str,
    event: &RuntimeEvent,
    incoming_revision: u64,
) -> bool {
    // record is selected only by its stable correlation-derived identifier.
    // record 仅通过关联派生的稳定标识选择。
    let Some(record) = state
        .records
        .iter_mut()
        .find(|record| record.id == record_id)
    else {
        return false;
    };
    // currentRevision prevents delayed persistence work from overwriting newer progress.
    // currentRevision 防止延迟的持久化工作覆盖较新进度。
    let current_revision = record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("request_revision"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    if incoming_revision <= current_revision {
        return false;
    }
    // merged retains execution metadata while replacing request snapshot fields atomically.
    // merged 保留执行元数据，同时原子替换请求快照字段。
    let mut merged = match record.metadata.take() {
        Some(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    if let Some(serde_json::Value::Object(update)) = event.metadata.clone() {
        for (key, value) in update {
            merged.insert(key, value);
        }
    }
    record.metadata = Some(serde_json::Value::Object(merged));
    true
}

/// Appends one visible tool-result card for a tool-finish runtime event.
/// 为工具结束运行时事件追加一张可见工具返回卡片。
pub(super) fn append_tool_finished_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> Result<Vec<String>, String> {
    let Some(tool_call_id) = parse_tool_finished_runtime_event_title(&event.title) else {
        return Ok(Vec::new());
    };
    let display_state = tool_display_state_for_finished_event(
        event.metadata.as_ref(),
        !event.detail.trim().is_empty(),
    );
    let tool_name = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            runtime_state
                .conversation_state
                .records
                .iter()
                .rev()
                .find_map(|record| tool_record_name_for_call_id(record, &tool_call_id))
        });
    let detail_ref = append_tool_result_detail_record(
        runtime_state,
        session_id,
        now,
        &tool_call_id,
        tool_name.as_deref(),
        display_state,
        event.metadata.as_ref(),
        &event.detail,
    )?;
    let result_metadata = normalize_tool_result_metadata(
        &tool_call_id,
        tool_name.as_deref(),
        display_state,
        event.metadata.as_ref(),
        detail_ref.as_ref(),
        false,
    );
    let attached_record_id = runtime_state
        .tool_call_record_ids
        .remove(&tool_call_id)
        .filter(|record_id| {
            attach_tool_result_to_record(
                &mut runtime_state.conversation_state,
                record_id,
                result_metadata.clone(),
            )
        });
    let changed_record_id = if let Some(record_id) = attached_record_id {
        record_id
    } else {
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        let mut orphan_metadata = result_metadata;
        if let serde_json::Value::Object(object) = &mut orphan_metadata {
            object.insert(String::from("orphan"), serde_json::Value::Bool(true));
        }
        let record = SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(orphan_metadata),
            created_at: now.to_string(),
        };
        append_record(&mut runtime_state.conversation_state, record)
            .map_err(|error| format!("Invalid orphan tool-result record identity: {error}"))?;
        record_id
    };
    runtime_state.updated_at = now.to_string();
    Ok(vec![changed_record_id])
}

/// Parses one tool-start runtime headline into its stable tool-call identifier and visible tool name.
/// 将一条工具开始运行时标题解析为稳定工具调用标识及可见工具名称。
pub(super) fn parse_tool_started_runtime_event_title(title: &str) -> Option<(String, String)> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next(), segments.next()) {
        (Some("工具调用"), Some(tool_call_id), Some(tool_name))
            if !tool_call_id.trim().is_empty() && !tool_name.trim().is_empty() =>
        {
            Some((
                tool_call_id.trim().to_string(),
                tool_name.trim().to_string(),
            ))
        }
        _ => None,
    }
}

/// Parses one tool-finish runtime headline into its stable tool-call identifier.
/// 将一条工具结束运行时标题解析为稳定工具调用标识。
pub(super) fn parse_tool_finished_runtime_event_title(title: &str) -> Option<String> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next()) {
        (Some("工具返回"), Some(tool_call_id)) if !tool_call_id.trim().is_empty() => {
            Some(tool_call_id.trim().to_string())
        }
        _ => None,
    }
}

/// Returns the stable display state used by one human-facing tool execution card.
/// 返回单张面向人类的工具执行卡片所使用的稳定显示状态。
pub(super) fn tool_display_state_for_finished_event(
    metadata: Option<&serde_json::Value>,
    has_return_body: bool,
) -> &'static str {
    if metadata.is_some_and(tool_metadata_represents_running_task_session) {
        return "running";
    }
    if has_return_body || metadata.is_some_and(tool_metadata_contains_return_payload) {
        return "completed";
    }
    if let Some(display_state) = metadata
        .and_then(|metadata| metadata.get("display_state"))
        .and_then(serde_json::Value::as_str)
    {
        return match display_state {
            "failed" | "error" => "failed",
            "running" => "running",
            "completed" | "success" | "succeeded" => "completed",
            _ => "completed",
        };
    }
    if metadata.is_some_and(tool_metadata_represents_failed_result) {
        return "failed";
    }
    "completed"
}

/// Returns whether one tool-finish metadata object is the running acknowledgement for a TaskSession.
/// 返回单个工具结束元数据对象是否表示 TaskSession 的运行中确认。
pub(super) fn tool_metadata_represents_running_task_session(metadata: &serde_json::Value) -> bool {
    task_session_execution_status_from_metadata(metadata)
        .is_some_and(|status| status.eq_ignore_ascii_case("running"))
}

/// Reads the TaskSession execution status from host-result or direct task metadata.
/// 从宿主结果或直接任务元数据中读取 TaskSession 执行状态。
pub(super) fn task_session_execution_status_from_metadata(
    metadata: &serde_json::Value,
) -> Option<&str> {
    metadata
        .get("host_result")
        .and_then(|host_result| {
            (host_result.get("kind").and_then(serde_json::Value::as_str) == Some("task_session"))
                .then(|| host_result.get("payload"))
                .flatten()
        })
        .and_then(|payload| payload.get("execution_status"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            (metadata.get("kind").and_then(serde_json::Value::as_str) == Some("task_session"))
                .then(|| metadata.get("payload"))
                .flatten()
                .and_then(|payload| payload.get("execution_status"))
                .and_then(serde_json::Value::as_str)
        })
}

/// Returns whether a tool-finish event carries an API-level returned payload even when the payload itself reports a semantic failure.
/// 返回工具结束事件是否携带 API 层已返回载荷，即使该载荷自身描述了语义失败。
pub(super) fn tool_metadata_contains_return_payload(metadata: &serde_json::Value) -> bool {
    metadata
        .get("host_result")
        .is_some_and(|host_result| !host_result.is_null())
        || metadata
            .get("payload")
            .is_some_and(|payload| !payload.is_null())
}

/// Returns whether the provided tool metadata semantically describes one failed host-side tool result.
/// 返回提供的工具元数据是否在语义上描述了一条失败的宿主侧工具结果。
pub(super) fn tool_metadata_represents_failed_result(metadata: &serde_json::Value) -> bool {
    metadata
        .get("host_result")
        .and_then(|host_result| host_result.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("tool_error")
        || metadata
            .get("payload")
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str)
            == Some("failed")
        || metadata.get("status").and_then(serde_json::Value::as_str) == Some("failed")
}

/// Normalizes tool-call metadata for one durable structured tool execution node.
/// 为单个持久结构化工具执行节点规范化工具调用元数据。
pub(super) fn normalize_tool_call_metadata(
    metadata: Option<serde_json::Value>,
    tool_call_id: &str,
    tool_name: &str,
) -> serde_json::Value {
    let source = match metadata {
        Some(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    let mut object = serde_json::Map::new();
    for (key, value) in source {
        if !key.starts_with("webui_") {
            object.insert(key, value);
        }
    }
    object.insert(
        String::from("tool_call_id"),
        serde_json::Value::String(tool_call_id.to_string()),
    );
    object.insert(
        String::from("tool_name"),
        serde_json::Value::String(tool_name.to_string()),
    );
    serde_json::Value::Object(object)
}

/// Builds compact tool-result metadata that is safe to keep on the visible tool execution node.
/// 构建可安全保存在可见工具执行节点上的紧凑工具返回元数据。
pub(super) fn normalize_tool_result_metadata(
    tool_call_id: &str,
    tool_name: Option<&str>,
    display_state: &str,
    event_metadata: Option<&serde_json::Value>,
    detail_ref: Option<&ToolDetailRecordRef>,
    synthetic: bool,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    result.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if synthetic {
        result.insert(String::from("synthetic"), serde_json::Value::Bool(true));
    }
    if let Some(detail_ref) = detail_ref {
        result.insert(
            String::from("detail_ref"),
            json!({
                "kind": "session_record",
                "record_id": detail_ref.record_id,
                "message_index": detail_ref.message_index,
            }),
        );
    }

    let mut object = serde_json::Map::new();
    object.insert(
        String::from("tool_call_id"),
        serde_json::Value::String(tool_call_id.to_string()),
    );
    if let Some(tool_name) = tool_name.filter(|value| !value.trim().is_empty()) {
        object.insert(
            String::from("tool_name"),
            serde_json::Value::String(tool_name.to_string()),
        );
    }
    if let Some(host_result) = visible_host_result_metadata(event_metadata) {
        object.insert(String::from("host_result"), host_result);
    }
    object.insert(String::from("result"), serde_json::Value::Object(result));
    serde_json::Value::Object(object)
}

/// Returns the host-result metadata that is safe to attach to the visible tool row.
/// 返回可安全附加到可见工具行的宿主结果元数据。
fn visible_host_result_metadata(metadata: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    raw_visible_host_result(metadata)
        .cloned()
        .or_else(|| compact_subagent_wait_host_result(metadata))
}

/// Returns compact host results that WebUI specialized cards need without opening the raw tool modal.
/// 返回 WebUI 专用卡片无需打开原始工具弹窗即可使用的紧凑宿主结果。
pub(super) fn raw_visible_host_result(
    metadata: Option<&serde_json::Value>,
) -> Option<&serde_json::Value> {
    let metadata = metadata?;
    let nested = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            raw_host_result_kind_is_visible(
                host_result.get("kind").and_then(serde_json::Value::as_str),
            )
        });
    nested.or_else(|| {
        (raw_host_result_kind_is_visible(metadata.get("kind").and_then(serde_json::Value::as_str))
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Returns whether one host-result kind is small and structured enough for visible transcript metadata.
/// 判断一个宿主结果类型是否足够紧凑且结构化，可安全保留在可见转录元数据里。
pub(super) fn raw_host_result_kind_is_visible(kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some(
            "change_set"
                | "ask"
                | "command_run"
                | "plan_artifact"
                | "generated_attachments"
                | "provider_tool_output"
                | "lua_execution"
                | "lua_runtime_session"
        )
    )
}

/// Builds a statistics-only host result for wait_agent without copying large child output text.
/// 为 wait_agent 构建仅包含统计信息的宿主结果，避免复制大段子智能体输出正文。
fn compact_subagent_wait_host_result(
    metadata: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    let metadata = metadata?;
    let host_result = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            host_result.get("kind").and_then(serde_json::Value::as_str) == Some("subagent_wait")
        })
        .or_else(|| {
            (metadata.get("kind").and_then(serde_json::Value::as_str) == Some("subagent_wait")
                && metadata.get("payload").is_some())
            .then_some(metadata)
        })?;
    let payload = host_result.get("payload")?.as_object()?;
    if payload.get("tool").and_then(serde_json::Value::as_str) != Some("wait_agent") {
        return None;
    }
    let agents = payload
        .get("agents")
        .and_then(serde_json::Value::as_array)?;
    let compact_agents = agents
        .iter()
        .filter_map(compact_subagent_wait_agent_usage)
        .collect::<Vec<_>>();
    if compact_agents.is_empty() {
        return None;
    }
    let mut compact_payload = serde_json::Map::new();
    compact_payload.insert(String::from("tool"), json!("wait_agent"));
    for key in [
        "activity_status",
        "timed_out",
        "interrupted",
        "timeout_ms",
        "pending_event_count",
        "terminal_event_count",
        "message_event_count",
        "status_event_count",
    ] {
        if let Some(value) = payload.get(key) {
            compact_payload.insert(String::from(key), value.clone());
        }
    }
    compact_payload.insert(String::from("agents"), json!(compact_agents));
    Some(json!({
        "kind": "subagent_wait",
        "payload": compact_payload,
    }))
}

/// Extracts the minimal per-agent fields needed for parent-turn usage accounting.
/// 提取父级轮次用量统计所需的最小单智能体字段。
fn compact_subagent_wait_agent_usage(agent: &serde_json::Value) -> Option<serde_json::Value> {
    let agent = agent.as_object()?;
    let usage_segments = agent
        .get("usage_segments")
        .filter(|usage_segments| usage_segments.is_array())?;
    let mut compact = serde_json::Map::new();
    for key in [
        "agent_id",
        "subagent_task_id",
        "execution_id",
        "kind",
        "status",
        "display_title",
    ] {
        if let Some(value) = agent.get(key) {
            compact.insert(String::from(key), value.clone());
        }
    }
    compact.insert(String::from("usage_segments"), usage_segments.clone());
    Some(serde_json::Value::Object(compact))
}

/// Attaches one compact tool result to an existing visible tool-call record.
/// 将一条紧凑工具返回挂接到已有的可见工具调用记录上。
pub(super) fn attach_tool_result_to_record(
    state: &mut SessionConversationState,
    record_id: &str,
    result_metadata: serde_json::Value,
) -> bool {
    let Some(record) = state
        .records
        .iter_mut()
        .find(|record| record.id == record_id)
    else {
        return false;
    };
    let mut merged = match record.metadata.take() {
        Some(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(result_object) = result_metadata {
        for (key, value) in result_object {
            merged.insert(key, value);
        }
    }
    record.title.clear();
    record.body.clear();
    record.metadata = Some(serde_json::Value::Object(merged));
    true
}

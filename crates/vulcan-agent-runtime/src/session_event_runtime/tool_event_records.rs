use super::*;

pub(super) fn append_tool_started_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> bool {
    let Some((tool_call_id, tool_name)) = parse_tool_started_runtime_event_title(&event.title)
    else {
        return false;
    };
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
    );
    runtime_state
        .tool_call_record_ids
        .insert(tool_call_id, record_id);
    runtime_state.updated_at = now.to_string();
    true
}

/// Appends one visible tool-result card for a tool-finish runtime event.
/// 为工具结束运行时事件追加一张可见工具返回卡片。
pub(super) fn append_tool_finished_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> bool {
    let Some(tool_call_id) = parse_tool_finished_runtime_event_title(&event.title) else {
        return false;
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
    let tool_result_id = tool_result_id_from_metadata(event.metadata.as_ref(), &tool_call_id);
    let detail_ref = append_tool_result_detail_record(
        runtime_state,
        session_id,
        now,
        &tool_call_id,
        &tool_result_id,
        tool_name.as_deref(),
        display_state,
        event.metadata.as_ref(),
        &event.detail,
    );
    let result_metadata = normalize_tool_result_metadata(
        &tool_call_id,
        &tool_result_id,
        tool_name.as_deref(),
        display_state,
        event.metadata.as_ref(),
        detail_ref.as_ref(),
        false,
    );
    let attached = runtime_state
        .tool_call_record_ids
        .remove(&tool_call_id)
        .and_then(|record_id| {
            attach_tool_result_to_record(
                &mut runtime_state.conversation_state,
                &record_id,
                result_metadata.clone(),
            )
            .then_some(())
        })
        .is_some();
    if !attached {
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
            id: record_id,
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(orphan_metadata),
            created_at: now.to_string(),
        };
        runtime_state.conversation_state.records.push(record);
    }
    runtime_state.updated_at = now.to_string();
    true
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
        .get("tool_result_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || metadata
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

/// Resolves the stable host-visible tool-result id for one tool call.
/// 为单次工具调用解析稳定的宿主可见工具返回标识。
pub(super) fn tool_result_id_from_metadata(
    metadata: Option<&serde_json::Value>,
    tool_call_id: &str,
) -> String {
    metadata
        .and_then(|metadata| metadata.get("tool_result_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("tool-result-{tool_call_id}"))
}

/// Builds compact tool-result metadata that is safe to keep on the visible tool execution node.
/// 构建可安全保存在可见工具执行节点上的紧凑工具返回元数据。
pub(super) fn normalize_tool_result_metadata(
    tool_call_id: &str,
    tool_result_id: &str,
    tool_name: Option<&str>,
    display_state: &str,
    event_metadata: Option<&serde_json::Value>,
    detail_ref: Option<&ToolDetailRecordRef>,
    synthetic: bool,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    result.insert(
        String::from("tool_result_id"),
        serde_json::Value::String(tool_result_id.to_string()),
    );
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
    if let Some(host_result) = raw_visible_host_result(event_metadata) {
        object.insert(String::from("host_result"), host_result.clone());
    }
    object.insert(String::from("result"), serde_json::Value::Object(result));
    serde_json::Value::Object(object)
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
    matches!(kind, Some("change_set" | "ask"))
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

//! Shared session event reducers for background conversation state updates.
//! 用于后台会话状态更新的共享会话事件归约器。

use serde_json::json;
use std::collections::BTreeSet;
use vulcan_core::types::ConversationAttachment;
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageKind, SessionMessageRecordKind,
    SessionRuntimeStateEntry, SessionTurnStatsStatus,
};

use crate::{
    ExecutionRunReport,
    agent_runtime::events::{RuntimeEvent, RuntimeEventKind},
    conversation_protocol::{
        SessionConversationPayload, SessionConversationRecord, SessionConversationState,
        append_assistant_text_message, append_record, append_record_body_delta,
        append_user_text_message,
    },
    skills::{SessionDerivedRecord, SessionDerivedSkillChain, SummarySkillInput},
    turn_stats::build_session_turn_stats_from_state,
};

/// Local identifiers for records created while one streaming run is active.
/// 单次流式运行活跃期间创建的本地记录标识。
#[derive(Debug, Clone, Default)]
pub struct SessionStreamingRecordIds {
    /// Collapsed thinking record that receives thinking deltas.
    /// 接收思考增量的默认折叠思考记录。
    pub thinking_record_id: Option<String>,
    /// Visible assistant output record that receives final-output deltas.
    /// 接收正式输出增量的可见助手输出记录。
    pub output_record_id: Option<String>,
}

/// Builds the stable transcript record id used by legacy session projections.
/// 构建旧会话投影使用的稳定转录记录标识。
pub fn build_session_record_message_id(
    session_id: SessionId,
    created_at: &str,
    record_index: usize,
) -> String {
    format!("{session_id}-{created_at}-{record_index}")
}

/// Appends one user prompt as both canonical context and visible transcript record.
/// 将用户提示词同时追加为规范上下文与可见转录记录。
pub fn append_user_prompt_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    prompt: &str,
) {
    append_user_prompt_record_with_attachments(runtime_state, session_id, now, prompt, Vec::new());
}

/// Appends one user prompt with structured attachments as canonical context and visible transcript.
/// 将带结构化附件的用户提示词同时追加为规范上下文与可见转录记录。
pub fn append_user_prompt_record_with_attachments(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    prompt: &str,
    attachments: Vec<ConversationAttachment>,
) {
    runtime_state.ui_state.unread = false;
    runtime_state.last_safe_provider_request_count = None;
    let message_id = format!("session-{session_id}-protocol-user-{now}");
    append_user_text_message(
        &mut runtime_state.conversation_state,
        message_id,
        prompt.to_string(),
        attachments.clone(),
    );
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    let metadata = build_user_prompt_record_metadata(prompt, &attachments);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: prompt.to_string(),
            metadata,
            created_at: now.to_string(),
        },
    );
    runtime_state.updated_at = now.to_string();
}

/// Builds reload-safe metadata for one visible user prompt record.
/// 为一条可见用户提示词记录构建可安全重载的元数据。
fn build_user_prompt_record_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<serde_json::Value> {
    let source = PendingSessionPromptSource::UserFollowup;
    let serialized_attachments = (!attachments.is_empty())
        .then(|| serde_json::to_value(attachments))
        .transpose()
        .ok()
        .flatten();
    let mut metadata = json!({
        "canonical_text": prompt,
        "canonical_user_text": prompt,
        "source": source,
        "starts_user_turn": source.starts_user_parent_turn(),
        "turn_process": source.folds_into_active_turn_process(),
        "collapsed_by_default": source.folds_into_active_turn_process(),
        "editable": source.allows_user_prompt_edit(),
    });
    if let Some(serialized_attachments) = serialized_attachments
        && let Some(object) = metadata.as_object_mut()
    {
        object.insert(String::from("attachments"), serialized_attachments.clone());
        object.insert(
            String::from("transcript_attachments"),
            serialized_attachments,
        );
    }
    Some(metadata)
}

/// Appends or extends one streaming transcript record inside the background state.
/// 在后台状态中追加或扩展单条流式转录记录。
pub fn append_stream_delta_record_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) {
    if delta.is_empty() {
        return;
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
        );
        *record_id_slot = Some(record_id);
    }
    runtime_state.updated_at = now.to_string();
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

/// Projects one runtime event into visible transcript records when the event has user-facing detail.
/// 当运行时事件具备面向用户的详情时，将其投影为可见转录记录。
pub fn append_runtime_event_projection_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    event: &RuntimeEvent,
) -> bool {
    match event.kind {
        RuntimeEventKind::ToolStarted => {
            append_tool_started_projection_to_state(runtime_state, session_id, now, event)
        }
        RuntimeEventKind::ToolFinished => {
            append_tool_finished_projection_to_state(runtime_state, session_id, now, event)
        }
        _ => false,
    }
}

/// Appends one visible tool-call card for a tool-start runtime event.
/// 为工具开始运行时事件追加一张可见工具调用卡片。
fn append_tool_started_projection_to_state(
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
fn append_tool_finished_projection_to_state(
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
fn parse_tool_started_runtime_event_title(title: &str) -> Option<(String, String)> {
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
fn parse_tool_finished_runtime_event_title(title: &str) -> Option<String> {
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
fn tool_display_state_for_finished_event(
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
fn tool_metadata_represents_running_task_session(metadata: &serde_json::Value) -> bool {
    task_session_execution_status_from_metadata(metadata)
        .is_some_and(|status| status.eq_ignore_ascii_case("running"))
}

/// Reads the TaskSession execution status from host-result or direct task metadata.
/// 从宿主结果或直接任务元数据中读取 TaskSession 执行状态。
fn task_session_execution_status_from_metadata(metadata: &serde_json::Value) -> Option<&str> {
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
fn tool_metadata_contains_return_payload(metadata: &serde_json::Value) -> bool {
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
fn tool_metadata_represents_failed_result(metadata: &serde_json::Value) -> bool {
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
fn normalize_tool_call_metadata(
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
fn tool_result_id_from_metadata(
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
fn normalize_tool_result_metadata(
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
fn raw_visible_host_result(metadata: Option<&serde_json::Value>) -> Option<&serde_json::Value> {
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
fn raw_host_result_kind_is_visible(kind: Option<&str>) -> bool {
    matches!(kind, Some("change_set" | "ask"))
}

/// Attaches one compact tool result to an existing visible tool-call record.
/// 将一条紧凑工具返回挂接到已有的可见工具调用记录上。
fn attach_tool_result_to_record(
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

/// Identifies the hidden tool-detail record created for one visible tool call.
/// 标识为单条可见工具调用创建的隐藏工具详情记录。
#[derive(Debug, Clone)]
struct ToolDetailRecordRef {
    /// Stable hidden session record id.
    /// 稳定隐藏会话记录标识。
    record_id: String,
    /// Zero-based message index inside the current conversation record list.
    /// 当前会话记录列表中的零基消息索引。
    message_index: usize,
}

/// Stores the full tool-result detail outside the visible transcript stream and returns its record id.
/// 将完整工具返回详情存放在可见转录流之外，并返回对应记录标识。
fn append_tool_result_detail_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    tool_call_id: &str,
    tool_result_id: &str,
    tool_name: Option<&str>,
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    detail: &str,
) -> Option<ToolDetailRecordRef> {
    let model_output = tool_result_model_output_from_detail(detail);
    if model_output.trim().is_empty() {
        return None;
    }
    let display = tool_result_display_payload(tool_name, display_state, metadata, &model_output);
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("tool_detail"), serde_json::Value::Bool(true));
    metadata_object.insert(
        String::from("tool_call_id"),
        serde_json::Value::String(tool_call_id.to_string()),
    );
    metadata_object.insert(
        String::from("tool_result_id"),
        serde_json::Value::String(tool_result_id.to_string()),
    );
    if let Some(tool_name) = tool_name.filter(|value| !value.trim().is_empty()) {
        metadata_object.insert(
            String::from("tool_name"),
            serde_json::Value::String(tool_name.to_string()),
        );
    }
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        metadata_object.insert(String::from("arguments"), arguments.clone());
    }
    metadata_object.insert(String::from("display"), display);
    metadata_object.insert(
        String::from("detail_chars"),
        serde_json::Value::Number(serde_json::Number::from(model_output.chars().count() as u64)),
    );
    let message_index = runtime_state.conversation_state.records.len();
    let record_id = build_session_record_message_id(session_id, now, message_index);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolDetailOutput,
            visible: false,
            title: String::new(),
            body: model_output,
            metadata: Some(serde_json::Value::Object(metadata_object)),
            created_at: now.to_string(),
        },
    );
    Some(ToolDetailRecordRef {
        record_id,
        message_index,
    })
}

/// Extracts the model-facing tool result text from the host detail wrapper.
/// 从宿主详情包装中提取面向模型的工具返回文本。
fn tool_result_model_output_from_detail(detail: &str) -> String {
    const MARKER: &str = "## Tool Result";
    let output = detail
        .find(MARKER)
        .map(|index| &detail[index + MARKER.len()..])
        .unwrap_or(detail)
        .trim_start_matches(['\r', '\n'])
        .trim();
    output.to_string()
}

/// Builds the structured display payload used by WebUI tool-specific renderers.
/// 构建 WebUI 工具专属渲染器使用的结构化展示载荷。
fn tool_result_display_payload(
    tool_name: Option<&str>,
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    if tool_name == Some("shell_command") {
        return shell_command_display_payload(display_state, model_output);
    }
    if tool_name == Some("vulcan_file_list") {
        return vulcan_file_list_display_payload(display_state, metadata, model_output);
    }
    if tool_name == Some("ask") {
        return ask_question_display_payload(display_state, metadata, model_output);
    }
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(tool_name.unwrap_or("generic").to_string()),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Builds the structured ask display payload from the native ask host result.
/// 从原生 ask 宿主结果中构建结构化提问展示载荷。
fn ask_question_display_payload(
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    if let Some(display) = ask_question_host_display(metadata) {
        let mut display = display.clone();
        if let serde_json::Value::Object(object) = &mut display {
            object.insert(
                String::from("status"),
                serde_json::Value::String(display_state.to_string()),
            );
            object.entry(String::from("summary")).or_insert_with(|| {
                serde_json::Value::String(compact_display_summary(model_output))
            });
        }
        return display;
    }
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("ask")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Extracts the workbench-only ask display contract from one tool-finished metadata object.
/// 从单条工具完成元数据中提取仅供工作台使用的 ask 展示契约。
fn ask_question_host_display(metadata: Option<&serde_json::Value>) -> Option<&serde_json::Value> {
    let payload = host_result_payload_by_kind(metadata?, "ask")?;
    payload.get("display").filter(|display| {
        display.get("kind").and_then(serde_json::Value::as_str) == Some("ask_question")
    })
}

/// Extracts a host-result payload by kind from nested or flattened runtime metadata.
/// 从嵌套或扁平运行时元数据中按类型提取宿主结果载荷。
fn host_result_payload_by_kind<'a>(
    metadata: &'a serde_json::Value,
    expected_kind: &str,
) -> Option<&'a serde_json::Value> {
    let nested = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            host_result.get("kind").and_then(serde_json::Value::as_str) == Some(expected_kind)
        })
        .and_then(|host_result| host_result.get("payload"));
    nested.or_else(|| {
        (metadata.get("kind").and_then(serde_json::Value::as_str) == Some(expected_kind))
            .then(|| metadata.get("payload"))
            .flatten()
    })
}

/// Builds a compact structured display payload for the file-list tool.
/// 为文件列表工具构建紧凑结构化展示载荷。
fn vulcan_file_list_display_payload(
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("vulcan_file_list")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    if let Some(first_line) = model_output.lines().next() {
        merge_file_list_header_fields(&mut object, first_line);
    }
    let files = model_output
        .lines()
        .skip(1)
        .flat_map(|line| line.split_whitespace())
        .filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::Value::String(value.to_string()))
        .collect::<Vec<_>>();
    if !files.is_empty() {
        object.insert(String::from("files"), serde_json::Value::Array(files));
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Merges known `vulcan_file_list` header counters into one display object.
/// 将已知的 `vulcan_file_list` 头部计数字段合并到展示对象。
fn merge_file_list_header_fields(
    object: &mut serde_json::Map<String, serde_json::Value>,
    line: &str,
) {
    let header = line.trim().trim_start_matches('[').trim_end_matches(']');
    if let Some((label, rest)) = header.split_once(':') {
        object.insert(
            String::from("label"),
            serde_json::Value::String(label.trim().to_string()),
        );
        let mut pending_key: Option<String> = None;
        for token in rest.split_whitespace() {
            if let Some((key, value)) = token.split_once(':') {
                insert_file_list_header_field(object, key, value);
                pending_key = None;
            } else if pending_key.is_none() {
                object.insert(
                    String::from("root"),
                    serde_json::Value::String(token.to_string()),
                );
                pending_key = Some(String::from("root"));
            }
        }
    }
}

/// Inserts one typed file-list header field into the display object.
/// 向展示对象写入单个带类型的文件列表头部字段。
fn insert_file_list_header_field(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
) {
    let normalized_key = match key {
        "Pattern" => "pattern",
        "Recursive" => "recursive",
        "Ignore" => "ignore",
        "Files" => "files_count",
        "Truncated" => "truncated",
        "Ignored" => "ignored_count",
        _ => return,
    };
    let typed_value = match value {
        "true" | "True" => serde_json::Value::Bool(true),
        "false" | "False" => serde_json::Value::Bool(false),
        _ => value
            .parse::<u64>()
            .map(|number| serde_json::Value::Number(serde_json::Number::from(number)))
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
    };
    object.insert(normalized_key.to_string(), typed_value);
}

/// Builds a bounded one-line display summary without duplicating the full model output.
/// 构建有界单行展示摘要，避免重复保存完整模型输出。
fn compact_display_summary(source: &str) -> String {
    let compact = source.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_SUMMARY_CHARS: usize = 240;
    if compact.chars().count() <= MAX_SUMMARY_CHARS {
        return compact;
    }
    format!(
        "{}...",
        compact.chars().take(MAX_SUMMARY_CHARS).collect::<String>()
    )
}

/// Parses a shell-command model output into command, status, and stream fields for display.
/// 将 shell_command 模型输出解析为命令、状态和输出流字段用于展示。
fn shell_command_display_payload(display_state: &str, model_output: &str) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("shell_command")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut section: Option<&str> = None;
    for line in model_output.lines() {
        let trimmed = line.trim();
        match trimmed {
            "[stdout]" => {
                section = Some("stdout");
                continue;
            }
            "[stderr]" => {
                section = Some("stderr");
                continue;
            }
            _ => {}
        }
        match section {
            Some("stdout") => stdout.push(line),
            Some("stderr") => stderr.push(line),
            _ => {
                if let Some((key, value)) = line.split_once(": ") {
                    let normalized_key = match key.trim() {
                        "Command" => Some("command"),
                        "Shell" => Some("shell"),
                        "Working directory" => Some("working_directory"),
                        "Status" => Some("status"),
                        "Duration" => Some("duration"),
                        "Exit code" => Some("exit_code"),
                        _ => None,
                    };
                    if let Some(normalized_key) = normalized_key {
                        if normalized_key == "exit_code" {
                            if let Ok(exit_code) = value.trim().parse::<i64>() {
                                object.insert(
                                    normalized_key.to_string(),
                                    serde_json::Value::Number(serde_json::Number::from(exit_code)),
                                );
                                continue;
                            }
                        }
                        object.insert(
                            normalized_key.to_string(),
                            serde_json::Value::String(value.trim().to_string()),
                        );
                    }
                }
            }
        }
    }
    if !stdout.is_empty() {
        object.insert(
            String::from("stdout"),
            serde_json::Value::String(stdout.join("\n").trim().to_string()),
        );
    }
    if !stderr.is_empty() {
        object.insert(
            String::from("stderr"),
            serde_json::Value::String(stderr.join("\n").trim().to_string()),
        );
    }
    serde_json::Value::Object(object)
}

/// Completes every still-open visible tool execution with one synthetic terminal result.
/// 使用一条合成终态结果闭合所有仍未返回的可见工具执行节点。
fn complete_open_tool_records(
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
fn tool_record_name_for_call_id(
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

/// Replaces canonical context fields while preserving locally projected visible records.
/// 替换规范上下文字段，同时保留本地已投影的可见记录。
pub fn apply_checkpoint_conversation_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    checkpoint_state: SessionConversationState,
    now: &str,
) {
    let mut preserved_records = runtime_state.conversation_state.records.clone();
    merge_checkpoint_user_records(&mut preserved_records, &checkpoint_state.records);
    let preserved_messages = runtime_state.conversation_state.messages.clone();
    runtime_state.conversation_state = checkpoint_state;
    if runtime_state.conversation_state.messages.is_empty() && !preserved_messages.is_empty() {
        runtime_state.conversation_state.messages = preserved_messages;
    }
    runtime_state.conversation_state.records = preserved_records;
    runtime_state.updated_at = now.to_string();
}

/// Merges user-visible checkpoint records that were created inside the host-owned controller.
/// 合并由宿主控制器内部创建的用户可见检查点记录。
///
/// # Parameters
/// 参数
/// `preserved_records` are the workbench-local transcript projections retained across checkpoints.
/// `preserved_records` 是跨检查点保留的工作台本地转录投影。
/// `checkpoint_records` are records emitted by the canonical controller snapshot.
/// `checkpoint_records` 是规范控制器快照发出的记录。
fn merge_checkpoint_user_records(
    preserved_records: &mut Vec<SessionConversationRecord>,
    checkpoint_records: &[SessionConversationRecord],
) {
    let mut preserved_ids = preserved_records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    for (index, record) in checkpoint_records.iter().enumerate() {
        if !record.visible
            || record.kind != SessionMessageKind::User
            || record.record_kind != SessionMessageRecordKind::Transcript
            || preserved_ids.contains(&record.id)
            || is_internal_session_mailbox_checkpoint_record(record)
        {
            continue;
        }
        preserved_ids.insert(record.id.clone());
        let insert_index =
            checkpoint_user_record_insert_index(preserved_records, checkpoint_records, index);
        preserved_records.insert(insert_index, record.clone());
    }
}

/// Returns whether one checkpoint record is an internal session-mailbox prompt.
/// 判断一条检查点记录是否是内部会话 mailbox 提示词。
fn is_internal_session_mailbox_checkpoint_record(record: &SessionConversationRecord) -> bool {
    let body = record.body.trim_start();
    body.starts_with("<subagent_mailbox_event>") || body.starts_with("<subagent_mailbox_events>")
}

/// Returns where a checkpoint-created user record belongs in the preserved transcript.
/// 返回由检查点创建的用户记录在保留转录中的插入位置。
///
/// # Parameters
/// 参数
/// `preserved_records` are the existing host-side visible records.
/// `preserved_records` 是宿主侧已经存在的可见记录。
/// `checkpoint_records` are the ordered records emitted by the controller snapshot.
/// `checkpoint_records` 是控制器快照发出的有序记录。
/// `checkpoint_index` points at the missing checkpoint user record.
/// `checkpoint_index` 指向缺失的检查点用户记录。
///
/// # Returns
/// 返回值
/// The index that preserves checkpoint and timestamp order as closely as possible.
/// 返回尽可能保持检查点顺序与时间顺序的插入索引。
fn checkpoint_user_record_insert_index(
    preserved_records: &[SessionConversationRecord],
    checkpoint_records: &[SessionConversationRecord],
    checkpoint_index: usize,
) -> usize {
    let checkpoint_record = &checkpoint_records[checkpoint_index];
    if let Some(successor_index) = checkpoint_records[checkpoint_index.saturating_add(1)..]
        .iter()
        .find_map(|successor| {
            preserved_record_index_for_checkpoint_record(preserved_records, successor)
        })
    {
        return successor_index;
    }
    if !checkpoint_record.created_at.trim().is_empty()
        && let Some(time_index) = preserved_records.iter().position(|record| {
            !record.created_at.trim().is_empty() && record.created_at > checkpoint_record.created_at
        })
    {
        return time_index;
    }
    preserved_records.len()
}

/// Finds the preserved record that corresponds to one checkpoint record.
/// 查找与某条检查点记录对应的保留记录。
///
/// # Parameters
/// 参数
/// `preserved_records` are the host-side records retained across checkpoints.
/// `preserved_records` 是跨检查点保留的宿主侧记录。
/// `checkpoint_record` is the controller-side record used as an ordering anchor.
/// `checkpoint_record` 是作为排序锚点的控制器侧记录。
///
/// # Returns
/// 返回值
/// The preserved record index when an anchor can be matched.
/// 当可以匹配锚点时返回保留记录索引。
fn preserved_record_index_for_checkpoint_record(
    preserved_records: &[SessionConversationRecord],
    checkpoint_record: &SessionConversationRecord,
) -> Option<usize> {
    preserved_records
        .iter()
        .position(|record| record.id == checkpoint_record.id)
}

/// Applies the terminal execution report to one background conversation state.
/// 将终态执行报告应用到单个后台会话状态。
pub fn apply_terminal_execution_report_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) -> SessionTerminalReportApplyResult {
    let is_success = report.error.is_none();
    if let Some(conversation_state) = report.conversation_state.clone() {
        apply_checkpoint_conversation_state(runtime_state, conversation_state, now);
    }
    if let Some(usage) = report.last_successful_provider_usage.clone() {
        runtime_state.last_provider_usage = Some(usage.clone());
        runtime_state.last_successful_provider_usage = Some(usage);
    }
    if report.provider_request_count > 0 {
        runtime_state.last_safe_provider_request_count = Some(report.provider_request_count);
    }
    if is_success {
        complete_open_tool_records(
            runtime_state,
            session_id,
            now,
            "failed",
            "Tool call ended before returning a result.",
        );
    } else {
        complete_open_tool_records(
            runtime_state,
            session_id,
            now,
            "failed",
            "Tool call failed or was interrupted before returning a result.",
        );
    }
    let closing_record_id = if is_success {
        ensure_terminal_success_records(runtime_state, session_id, now, report);
        ensure_terminal_success_assistant_message(runtime_state, session_id, now, report);
        append_terminal_summary_record(runtime_state, session_id, now, report)
    } else {
        append_terminal_error_record(runtime_state, session_id, now, report);
        append_terminal_terminated_record(
            runtime_state,
            session_id,
            now,
            report
                .error
                .as_deref()
                .unwrap_or("真实执行失败，但未返回具体错误信息。"),
        )
    };
    let status = if is_success {
        SessionTurnStatsStatus::Completed
    } else {
        SessionTurnStatsStatus::Failed
    };
    append_terminal_turn_stats_record(
        runtime_state,
        session_id,
        now,
        &closing_record_id,
        status,
        Some(report.provider_request_count),
    );
    runtime_state.updated_at = now.to_string();
    SessionTerminalReportApplyResult {
        is_success,
        closing_record_id,
    }
}

/// Result produced after applying one terminal report.
/// 应用一份终态报告后产出的结果。
#[derive(Debug, Clone)]
pub struct SessionTerminalReportApplyResult {
    /// Whether the terminal report represents a successful run.
    /// 终态报告是否表示一次成功运行。
    pub is_success: bool,
    /// Hidden summary or terminated record that closes the logical turn.
    /// 闭合逻辑轮次的隐藏摘要或终止记录。
    pub closing_record_id: String,
}

/// Ensures a successful terminal report has visible thinking and output records.
/// 确保成功终态报告拥有可见的思考与输出记录。
fn ensure_terminal_success_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    if !current_turn_has_visible_record(runtime_state, SessionMessageRecordKind::ThinkingOutput)
        && let Some(thinking_output) = report
            .thinking_output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        let thinking_record = SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ThinkingOutput,
            visible: true,
            title: String::from("思考"),
            body: thinking_output.to_string(),
            metadata: Some(json!({ "collapsed_by_default": true })),
            created_at: now.to_string(),
        };
        let insert_index = current_turn_first_visible_record_index(
            runtime_state,
            SessionMessageRecordKind::AssistantOutput,
        )
        .unwrap_or(runtime_state.conversation_state.records.len());
        runtime_state
            .conversation_state
            .records
            .insert(insert_index, thinking_record);
        runtime_state
            .ui_state
            .collapsed_message_ids
            .insert(record_id);
    }
    if !current_turn_has_visible_record(runtime_state, SessionMessageRecordKind::AssistantOutput) {
        let assistant_output = report.assistant_output.clone().unwrap_or_else(|| {
            String::from("The model completed the run but did not return visible text output.")
        });
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
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: assistant_output,
                metadata: None,
                created_at: now.to_string(),
            },
        );
    }
}

/// Ensures successful terminal output participates in future provider-visible history.
/// 确保成功终态输出会参与后续 Provider 可见历史。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state that owns canonical messages.
/// `runtime_state` 是拥有规范消息的会话运行态。
/// `session_id` selects the session receiving the assistant output.
/// `session_id` 选择接收助手输出的会话。
/// `now` is the current Unix-millis timestamp text.
/// `now` 是当前 Unix 毫秒时间戳文本。
/// `report` contains the terminal provider output.
/// `report` 包含终态 Provider 输出。
fn ensure_terminal_success_assistant_message(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    let Some(assistant_output) = report
        .assistant_output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    if runtime_state
        .conversation_state
        .messages
        .iter()
        .rev()
        .any(|message| {
            matches!(
                &message.payload,
                SessionConversationPayload::Assistant { text, .. } if text.trim() == assistant_output
            )
        })
    {
        return;
    }
    append_assistant_text_message(
        &mut runtime_state.conversation_state,
        format!("session-{session_id}-protocol-assistant-{now}"),
        assistant_output.to_string(),
    );
}

/// Appends the same hidden summary record produced by the shared session-derived skill chain.
/// 追加与共享会话派生技能链一致的隐藏摘要记录。
fn append_terminal_summary_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) -> String {
    let derived_record =
        SessionDerivedSkillChain::default().run_summary_skill(SummarySkillInput { report });
    append_derived_record(runtime_state, session_id, now, derived_record)
}

/// Appends a visible terminal error row for a failed execution report.
/// 为失败的执行报告追加一条可见终态错误行。
fn append_terminal_error_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("执行失败"),
            body: report
                .error
                .clone()
                .unwrap_or_else(|| String::from("真实执行失败，但未返回具体错误信息。")),
            metadata: None,
            created_at: now.to_string(),
        },
    );
}

/// Appends the hidden terminal marker used to close failed or stopped turns.
/// 追加用于闭合失败或中止轮次的隐藏终止标记。
fn append_terminal_terminated_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    error: &str,
) -> String {
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
            record_kind: SessionMessageRecordKind::TurnTerminated,
            visible: false,
            title: String::from("Turn Terminated"),
            body: format!("The run terminated without a summary. Error detail: {error}"),
            metadata: Some(json!({ "preview": "" })),
            created_at: now.to_string(),
        },
    );
    record_id
}

/// Appends one visible turn-statistics row using the shared statistics builder.
/// 使用共享统计构建器追加一条可见轮次统计行。
fn append_terminal_turn_stats_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
) -> Option<String> {
    let turn_stats = build_session_turn_stats_from_state(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
    )?;
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
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: format!("Turn Stats · {}", turn_stats.status.label()),
            body: turn_stats.detail_body(),
            metadata: Some(json!({
                "turn_stats": turn_stats,
                "turn_closing_record_id": closing_record_id,
            })),
            created_at: now.to_string(),
        },
    );
    Some(record_id)
}

/// Appends one hidden derived session record with the same metadata shape used by the workbench.
/// 以工作台使用的相同元数据结构追加一条隐藏派生会话记录。
fn append_derived_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    record: SessionDerivedRecord,
) -> String {
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: record.kind,
            record_kind: record.record_kind,
            visible: false,
            title: record.title,
            body: record.body,
            metadata: Some(json!({ "preview": record.preview })),
            created_at: now.to_string(),
        },
    );
    record_id
}

/// Returns whether the current logical turn already has a visible record of the requested kind.
/// 返回当前逻辑轮次是否已经拥有指定类型的可见记录。
fn current_turn_has_visible_record(
    runtime_state: &SessionRuntimeStateEntry,
    record_kind: SessionMessageRecordKind,
) -> bool {
    let records = &runtime_state.conversation_state.records;
    let boundary_index = records
        .iter()
        .rposition(|record| {
            matches!(
                record.record_kind,
                SessionMessageRecordKind::SummaryOutput
                    | SessionMessageRecordKind::TurnTerminated
                    | SessionMessageRecordKind::TurnStats
            )
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    records[boundary_index..]
        .iter()
        .any(|record| record.visible && record.record_kind == record_kind)
}

/// Finds the first visible record of a kind in the current logical turn.
/// 查找当前逻辑轮次中指定类型的第一条可见记录。
fn current_turn_first_visible_record_index(
    runtime_state: &SessionRuntimeStateEntry,
    record_kind: SessionMessageRecordKind,
) -> Option<usize> {
    let records = &runtime_state.conversation_state.records;
    let boundary_index = records
        .iter()
        .rposition(|record| {
            record.kind == SessionMessageKind::User
                || record.record_kind == SessionMessageRecordKind::TurnStats
                || record.record_kind == SessionMessageRecordKind::TurnTerminated
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    records[boundary_index..]
        .iter()
        .position(|record| record.visible && record.record_kind == record_kind)
        .map(|index| boundary_index + index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_protocol::SessionConversationState;
    use serde_json::json;

    /// Builds one deterministic test session identifier.
    /// 构建一个确定性的测试会话标识。
    fn test_session_id() -> SessionId {
        "11111111-1111-1111-1111-111111111111"
            .parse()
            .expect("test session id should be a valid UUID")
    }

    /// Verifies canonical checkpoints do not replace visible transcript projections.
    /// 验证规范检查点不会替换可见转录投影。
    #[test]
    fn checkpoint_preserves_visible_records() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_prompt_record(&mut runtime_state, session_id, "1000", "介绍下你自己");
        let mut output_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "1001",
            &mut output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            "我是 VulcanCode。",
            false,
        );
        let records_before_checkpoint = visible_record_projection(&runtime_state);

        apply_checkpoint_conversation_state(
            &mut runtime_state,
            SessionConversationState::default(),
            "1002",
        );

        assert_eq!(
            visible_record_projection(&runtime_state),
            records_before_checkpoint
        );
        assert_eq!(runtime_state.updated_at, "1002");
    }

    /// Verifies checkpoint-created guided user records are merged into visible transcript projections.
    /// 验证检查点创建的引导用户记录会合并进可见转录投影。
    #[test]
    fn checkpoint_merges_guided_user_record() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_prompt_record(&mut runtime_state, session_id, "1100", "先调用工具");
        let mut output_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "1101",
            &mut output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            "工具已经完成。",
            false,
        );
        let mut later_output_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "1104",
            &mut later_output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            "后续请求已经开始输出。",
            false,
        );
        let mut checkpoint_state = runtime_state.conversation_state.clone();
        checkpoint_state.records.push(SessionConversationRecord {
            id: String::from("core-guided-user-record"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from("不需要配图了"),
            metadata: Some(json!({
                "canonical_text": "不需要配图了",
                "canonical_user_text": "不需要配图了",
                "source": "user_interrupt",
                "starts_user_turn": false,
                "turn_process": true,
                "collapsed_by_default": true,
                "editable": false,
            })),
            created_at: String::from("1102"),
        });

        apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1103");

        let visible_bodies = runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.visible)
            .map(|record| record.body.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            visible_bodies,
            vec![
                "先调用工具",
                "工具已经完成。",
                "不需要配图了",
                "后续请求已经开始输出。"
            ]
        );
        let guided_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == "core-guided-user-record")
            .expect("guided user record should be merged");
        let guided_metadata = guided_record
            .metadata
            .as_ref()
            .expect("guided user record metadata should be preserved");
        assert_eq!(
            guided_metadata
                .get("source")
                .and_then(serde_json::Value::as_str),
            Some("user_interrupt")
        );
        assert_eq!(
            guided_metadata
                .get("starts_user_turn")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            guided_metadata
                .get("turn_process")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            guided_metadata
                .get("editable")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
    }

    /// Verifies internal mailbox checkpoint prompts never become visible user transcript rows.
    /// 验证内部 mailbox 检查点提示词不会进入可见用户转录行。
    #[test]
    fn checkpoint_skips_session_mailbox_user_record() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_prompt_record(&mut runtime_state, session_id, "1200", "启动两个子代理");
        let mut checkpoint_state = runtime_state.conversation_state.clone();
        checkpoint_state.records.push(SessionConversationRecord {
            id: String::from("core-mailbox-user-record"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户输入"),
            body: String::from(
                "<subagent_mailbox_event>\n{\"kind\":\"completed\"}\n</subagent_mailbox_event>",
            ),
            metadata: None,
            created_at: String::from("1201"),
        });

        apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1202");

        let visible_bodies = runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.visible)
            .map(|record| record.body.as_str())
            .collect::<Vec<_>>();
        assert_eq!(visible_bodies, vec!["启动两个子代理"]);
    }

    /// Verifies terminal reports keep streamed records when they also carry a canonical checkpoint.
    /// 验证终态报告携带规范检查点时仍会保留已流式写入的记录。
    #[test]
    fn terminal_report_preserves_streamed_records() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_prompt_record(&mut runtime_state, session_id, "2000", "随便调用两个工具");
        let mut thinking_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "2001",
            &mut thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考",
            "我需要先判断可用工具。",
            true,
        );
        let mut output_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "2002",
            &mut output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            "已完成，两个工具均正常。",
            false,
        );

        let report = ExecutionRunReport {
            session_id,
            preparation: None,
            assistant_output: Some(String::from("报告中的完整输出不应覆盖已流式记录。")),
            thinking_output: Some(String::from("报告中的思考不应重复追加。")),
            conversation_state: Some(SessionConversationState::default()),
            provider_request_count: 1,
            last_successful_provider_usage: None,
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
        };

        apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2003", &report);

        let visible_bodies: Vec<&str> = runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.visible)
            .map(|record| record.body.as_str())
            .collect();
        assert!(visible_bodies.contains(&"随便调用两个工具"));
        assert!(visible_bodies.contains(&"我需要先判断可用工具。"));
        assert!(visible_bodies.contains(&"已完成，两个工具均正常。"));
        assert!(!visible_bodies.contains(&"报告中的完整输出不应覆盖已流式记录。"));
        let canonical_texts = runtime_state
            .conversation_state
            .messages
            .iter()
            .filter_map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => Some(text.as_str()),
                SessionConversationPayload::Assistant { text, .. } => Some(text.as_str()),
                SessionConversationPayload::ToolResult { .. } => None,
            })
            .collect::<Vec<_>>();
        assert!(canonical_texts.contains(&"随便调用两个工具"));
        assert!(canonical_texts.contains(&"报告中的完整输出不应覆盖已流式记录。"));
    }

    /// Verifies a tool result projection preserves runtime event order instead of mutating the stored transcript order.
    /// 验证工具返回投影保留运行时事件顺序，而不是改写已存储转录顺序。
    #[test]
    fn tool_result_projection_preserves_runtime_event_order() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_prompt_record(&mut runtime_state, session_id, "3000", "调用工具");
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-1 · vulcan_file_list",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-1",
            "tool_name": "vulcan_file_list",
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3001",
            &started,
        ));
        let mut output_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "3002",
            &mut output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            "工具调用后继续输出。",
            false,
        );
        let finished = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-1",
            "## Result ID\n\n`tool-result-call-1`\n\n## Tool Result\n\nlisted files",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-1",
            "tool_result_id": "tool-result-call-1",
            "tool_name": "vulcan_file_list",
            "display_state": "completed",
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3003",
            &finished,
        ));

        let visible_kinds = runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.visible)
            .map(|record| record.record_kind)
            .collect::<Vec<_>>();
        assert_eq!(
            visible_kinds,
            vec![
                SessionMessageRecordKind::Transcript,
                SessionMessageRecordKind::ToolCallOutput,
                SessionMessageRecordKind::AssistantOutput,
            ]
        );
        let tool_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
            .expect("tool call record should remain visible");
        assert!(tool_record.title.is_empty());
        assert!(tool_record.body.is_empty());
        let metadata = tool_record.metadata.as_ref().expect("tool metadata");
        assert_eq!(
            metadata
                .get("result")
                .and_then(|result| result.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(
            metadata
                .get("result")
                .and_then(|result| result.get("tool_result_id"))
                .and_then(serde_json::Value::as_str),
            Some("tool-result-call-1")
        );
        assert!(metadata.get("tool_result_id").is_none());
        assert_eq!(
            metadata
                .get("result")
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("message_index"))
                .and_then(serde_json::Value::as_u64),
            Some(3)
        );
        assert!(
            metadata
                .get("result")
                .and_then(|result| result.get("preview"))
                .is_none()
        );
        let detail_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
            .expect("tool detail record should be hidden");
        assert!(!detail_record.visible);
        assert_eq!(detail_record.body, "listed files");
        let detail_metadata = detail_record.metadata.as_ref().expect("detail metadata");
        assert!(detail_metadata.get("model_output").is_none());
        assert!(
            detail_metadata
                .get("display")
                .and_then(|display| display.get("kind"))
                .and_then(serde_json::Value::as_str)
                .is_some()
        );
    }

    /// Verifies WebUI visible tool cards keep raw change-set host metadata for expandable file-change rendering.
    /// 验证 WebUI 可见工具卡片会保留原始 change-set 宿主元数据，用于文件变更展开渲染。
    #[test]
    fn tool_result_projection_preserves_raw_change_set_host_result() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-change · vulcan_file_create",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-change",
            "tool_name": "vulcan_file_create",
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3100",
            &started,
        ));
        let finished = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-change",
            "## Result ID\n\n`tool-result-call-change`\n\n## Tool Result\n\ncreated file",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-change",
            "tool_result_id": "tool-result-call-change",
            "tool_name": "vulcan_file_create",
            "display_state": "completed",
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "applied",
                    "files": [{
                        "change": "create",
                        "path": "src/demo.lua",
                        "content_mode": "full",
                        "content": "return 1\n"
                    }]
                }
            }
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3101",
            &finished,
        ));

        let tool_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
            .expect("tool call record should remain visible");
        let metadata = tool_record.metadata.as_ref().expect("tool metadata");
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("kind"))
                .and_then(serde_json::Value::as_str),
            Some("change_set")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("files"))
                .and_then(serde_json::Value::as_array)
                .and_then(|files| files.first())
                .and_then(|file| file.get("content"))
                .and_then(serde_json::Value::as_str),
            Some("return 1\n")
        );
    }

    /// Verifies ask answers are persisted as structured display data instead of requiring Markdown parsing.
    /// 验证 ask 答案会以结构化展示数据持久化，而不需要从 Markdown 文本反解析。
    #[test]
    fn tool_result_projection_preserves_structured_ask_display() {
        let session_id = test_session_id();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-ask · ask",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-ask",
            "tool_name": "ask",
            "arguments": {
                "questions": [{
                    "header": "确认",
                    "question": "是否继续？"
                }]
            }
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3200",
            &started,
        ));
        let finished = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-ask",
            "## Result ID\n\n`tool-result-call-ask`\n\n## Tool Result\n\n## User Answers\n\n- 确认: 继续\n\nContinue with these answers.",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-ask",
            "tool_result_id": "tool-result-call-ask",
            "tool_name": "ask",
            "display_state": "completed",
            "host_result": {
                "kind": "ask",
                "payload": {
                    "answers": [["继续"]],
                    "display": {
                        "kind": "ask_question",
                        "title": "向用户提问",
                        "ignored_answer_text": "用户忽略回答",
                        "questions": [{
                            "header": "确认",
                            "question": "是否继续？",
                            "answers": ["继续"],
                            "answered": true
                        }]
                    }
                }
            }
        })));
        assert!(append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3201",
            &finished,
        ));

        let tool_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
            .expect("ask tool call record should remain visible");
        let metadata = tool_record.metadata.as_ref().expect("ask metadata");
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("kind"))
                .and_then(serde_json::Value::as_str),
            Some("ask")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("answers"))
                .and_then(serde_json::Value::as_array)
                .and_then(|answers| answers.first())
                .and_then(serde_json::Value::as_array)
                .and_then(|answers| answers.first())
                .and_then(serde_json::Value::as_str),
            Some("继续")
        );

        let detail_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
            .expect("ask detail record should be hidden");
        let detail_metadata = detail_record
            .metadata
            .as_ref()
            .expect("ask detail metadata");
        assert_eq!(
            detail_metadata
                .get("display")
                .and_then(|display| display.get("kind"))
                .and_then(serde_json::Value::as_str),
            Some("ask_question")
        );
        assert_eq!(
            detail_metadata
                .get("display")
                .and_then(|display| display.get("questions"))
                .and_then(serde_json::Value::as_array)
                .and_then(|questions| questions.first())
                .and_then(|question| question.get("answers"))
                .and_then(serde_json::Value::as_array)
                .and_then(|answers| answers.first())
                .and_then(serde_json::Value::as_str),
            Some("继续")
        );
    }

    /// Verifies returned tool payloads are displayed as completed even when legacy host-result hints describe a semantic failure.
    /// 验证已返回工具载荷会显示为完成，即使旧宿主结果提示描述了语义失败。
    #[test]
    fn returned_tool_payload_wins_over_host_failure_hint() {
        let metadata = json!({
            "tool_result_id": "tool-result-call-1",
            "host_result": {
                "kind": "tool_error",
                "payload": {
                    "status": "failed",
                },
            },
        });
        assert_eq!(
            tool_display_state_for_finished_event(Some(&metadata), false),
            "completed"
        );
    }

    /// Verifies a running TaskSession acknowledgement stays running even though the spawn-agent call returned.
    /// 验证 TaskSession 运行中确认在 spawn_agent 调用返回后仍保持运行态。
    #[test]
    fn running_task_session_result_keeps_running_display_state() {
        let metadata = json!({
            "tool_result_id": "tool-result-call-task",
            "host_result": {
                "kind": "task_session",
                "payload": {
                    "task_session_id": test_session_id().to_string(),
                    "execution_status": "running",
                },
            },
            "display_state": "completed",
        });

        assert_eq!(
            tool_display_state_for_finished_event(Some(&metadata), true),
            "running"
        );
    }

    /// Projects visible records into comparable test tuples.
    /// 将可见记录投影成可比较的测试元组。
    fn visible_record_projection(
        runtime_state: &SessionRuntimeStateEntry,
    ) -> Vec<(SessionMessageKind, SessionMessageRecordKind, String)> {
        runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.visible)
            .map(|record| (record.kind, record.record_kind, record.body.clone()))
            .collect()
    }
}

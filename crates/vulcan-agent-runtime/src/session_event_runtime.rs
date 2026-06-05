//! Shared session event reducers for background conversation state updates.
//! 用于后台会话状态更新的共享会话事件归约器。

use serde_json::json;
use std::collections::BTreeSet;
use vulcan_core::types::ConversationAttachment;
use vulcan_workbench_model::{
    SessionId, SessionMessageKind, SessionMessageRecordKind, SessionRuntimeStateEntry,
    SessionTurnStatsStatus,
};

use crate::{
    ExecutionRunReport,
    agent_runtime::events::{RuntimeEvent, RuntimeEventKind},
    conversation_protocol::{
        SessionConversationRecord, SessionConversationState, append_record,
        append_record_body_delta, append_user_text_message,
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
    if attachments.is_empty() {
        return None;
    }
    let serialized_attachments = serde_json::to_value(attachments).ok()?;
    Some(json!({
        "canonical_text": prompt,
        "canonical_user_text": prompt,
        "attachments": serialized_attachments.clone(),
        "transcript_attachments": serialized_attachments,
    }))
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
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: human_tool_display_title(&tool_name, "running"),
            body: event.detail.clone(),
            metadata: Some(normalize_tool_event_metadata(
                event.metadata.clone(),
                &tool_call_id,
                Some(&tool_name),
                "running",
            )),
            created_at: now.to_string(),
        },
    );
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
    let title = tool_name
        .as_deref()
        .map(|name| human_tool_display_title(name, display_state))
        .unwrap_or_else(|| human_tool_display_title(&tool_call_id, display_state));
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    let record = SessionConversationRecord {
        id: record_id,
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title,
        body: event.detail.clone(),
        metadata: Some(normalize_tool_event_metadata(
            event.metadata.clone(),
            &tool_call_id,
            tool_name.as_deref(),
            display_state,
        )),
        created_at: now.to_string(),
    };
    runtime_state.conversation_state.records.push(record);
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

/// Builds one stable human-facing title for a tool execution card from the tool name and display state.
/// 根据工具名称与显示状态构建一条稳定的人类可读工具执行卡片标题。
fn human_tool_display_title(tool_name: &str, display_state: &str) -> String {
    let status_label = match display_state {
        "failed" => "失败",
        "completed" => "已完成",
        _ => "进行中",
    };
    format!("调用 {tool_name} 工具 {status_label}")
}

/// Returns the stable display state used by one human-facing tool execution card.
/// 返回单张面向人类的工具执行卡片所使用的稳定显示状态。
fn tool_display_state_for_finished_event(
    metadata: Option<&serde_json::Value>,
    has_return_body: bool,
) -> &'static str {
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

/// Normalizes tool event metadata so every UI surface can merge and open detail cards consistently.
/// 规范化工具事件元数据，使所有界面都能稳定合并并打开详情卡片。
fn normalize_tool_event_metadata(
    metadata: Option<serde_json::Value>,
    tool_call_id: &str,
    tool_name: Option<&str>,
    display_state: &str,
) -> serde_json::Value {
    let mut object = match metadata {
        Some(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
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
    object.insert(
        String::from("display_state"),
        serde_json::Value::String(display_state.to_string()),
    );
    serde_json::Value::Object(object)
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
    runtime_state.conversation_state = checkpoint_state;
    runtime_state.conversation_state.records = preserved_records;
    runtime_state.updated_at = now.to_string();
}

/// Merges user-visible checkpoint records that were created inside the host-owned controller.
/// 合并由宿主控制器内部创建的用户可见检查点记录。
///
/// # Parameters
/// 参数
/// `preserved_records` are the WebUI-local transcript projections retained across checkpoints.
/// `preserved_records` 是跨检查点保留的 WebUI 本地转录投影。
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
        {
            continue;
        }
        preserved_ids.insert(record.id.clone());
        let insert_index =
            checkpoint_user_record_insert_index(preserved_records, checkpoint_records, index);
        preserved_records.insert(insert_index, record.clone());
    }
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
    let closing_record_id = if is_success {
        ensure_terminal_success_records(runtime_state, session_id, now, report);
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

/// Appends one hidden derived session record with the same metadata shape used by iced.
/// 以 iced 使用的相同元数据结构追加一条隐藏派生会话记录。
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
            metadata: None,
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
            "## Tool Result",
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
                SessionMessageRecordKind::ToolResultOutput,
            ]
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

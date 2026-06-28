//! Context compaction messages helpers.
//! 上下文压缩 messages 辅助逻辑。

use super::*;

/// Ensures the API-produced compaction summary body is visible when streaming output was empty.
/// 当 API 没有流式输出正文时，确保 API 生成的压缩总结正文可见。
pub(crate) fn ensure_webui_context_compaction_summary_body_for_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    state_label: &str,
    budget_note: &str,
    trigger: &str,
    summary_body: &str,
) {
    let has_body = runtime_state
        .conversation_state
        .records
        .iter()
        .any(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_section_id"))
                .and_then(Value::as_str)
                == Some(section_id)
                && record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_role"))
                    .and_then(Value::as_str)
                    == Some("body")
                && !record.body.trim().is_empty()
        });
    if has_body {
        return;
    }
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.summary"),
        body: summary_body.to_string(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id,
            "context_compaction_role": "body",
            "context_compaction_state": state_label,
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
        })),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
}

/// Appends the terminal visible boundary for a failed compaction section.
/// 追加压缩失败分段的终止可见边界。
pub(crate) fn append_webui_context_compaction_failed_message_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) {
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.failed"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id,
            "context_compaction_role": "end",
            "context_compaction_state": "failed",
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
            "context_compaction_error": error,
        })),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
}

/// Appends the terminal visible boundary for one compaction section.
/// 追加单个压缩分段的终止可见边界。
///
/// # Parameters
/// # 参数
/// `runtime_state` is the draft runtime state being prepared for commit.
/// `runtime_state` 是正在准备提交的候选运行时状态。
/// `session_id` identifies the session receiving the visible boundary.
/// `session_id` 标识接收可见边界的会话。
/// `section_id` is the visible compaction section identifier.
/// `section_id` 是可见压缩分段标识。
/// `budget_note` is the budget text captured when compaction started.
/// `budget_note` 是压缩启动时捕获的预算文本。
/// `trigger` records whether compaction was manual or automatic.
/// `trigger` 记录压缩是手动触发还是自动触发。
/// `provider_request_count` is the total provider request count across successful and failed attempts.
/// `provider_request_count` 是成功与失败尝试累计的 Provider 请求次数。
/// `input_prune_report` is the optional clone-only pruning report for the successful attempt.
/// `input_prune_report` 是成功尝试对应的可选仅克隆剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns the newly appended terminal record id.
/// 返回新追加的终态记录标识。
pub(crate) fn append_webui_context_compaction_end_message_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    provider_request_count: usize,
    input_prune_report: Option<&CompactionInputPruneReport>,
) -> String {
    let mut metadata = json!({
        "context_compaction_status": true,
        "context_compaction_section_id": section_id,
        "context_compaction_role": "end",
        "context_compaction_state": "completed",
        "context_compaction_trigger": trigger,
        "context_compaction_budget_note": budget_note,
        "context_compaction_provider_request_count": provider_request_count,
    });
    if let (Some(report), Some(object)) = (input_prune_report, metadata.as_object_mut()) {
        object.insert(
            String::from("context_compaction_input_prune_report"),
            json!({
                "mode": context_compaction_input_prune_mode_label(report.mode),
                "target_input_budget_tokens": report.target_input_budget_tokens,
                "safety_reserve_tokens": report.safety_reserve_tokens,
                "original_token_estimate": report.original_token_estimate,
                "pruned_token_estimate": report.pruned_token_estimate,
                "pruned_tool_output_count": report.pruned_tool_output_count,
                "preserved_tool_output_count": report.preserved_tool_output_count,
                "dropped_source_message_count": report.dropped_source_message_count,
                "truncated_source_message_count": report.truncated_source_message_count,
                "still_exceeds_budget": report.still_exceeds_budget,
            }),
        );
    }
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.completed"),
        body: context_compaction_end_message_body(input_prune_report),
        metadata: Some(metadata),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry)
}

/// Builds the visible terminal note for one completed context-compaction section.
/// 构建单个已完成上下文压缩分段的可见终端说明。
///
/// # Parameters
/// # 参数
/// `input_prune_report` is the optional clone-only pruning report for the successful compaction input.
/// `input_prune_report` 是成功压缩输入对应的可选仅克隆剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns an empty body when no pruning happened, or a concise visible note when pruning happened.
/// 未发生剪枝时返回空正文，发生剪枝时返回简洁的可见说明。
pub(crate) fn context_compaction_end_message_body(
    _input_prune_report: Option<&CompactionInputPruneReport>,
) -> String {
    String::new()
}

/// Returns the stable metadata label for one context-compaction input pruning mode.
/// 返回上下文压缩输入剪枝模式的稳定元数据标签。
///
/// # Parameters
/// # 参数
/// `mode` is the clone-only pruning mode reported by the compaction runtime.
/// `mode` 是压缩运行时报告的仅克隆剪枝模式。
///
/// # Returns
/// # 返回值
/// Returns a stable snake_case label suitable for metadata and user-facing diagnostics.
/// 返回适用于元数据与用户侧诊断的稳定 snake_case 标签。
pub(crate) fn context_compaction_input_prune_mode_label(
    mode: CompactionInputPruneMode,
) -> &'static str {
    match mode {
        CompactionInputPruneMode::None => "none",
        CompactionInputPruneMode::RequiredForWindowDowngrade => "required_for_window_downgrade",
        CompactionInputPruneMode::RetryAfterFailure => "retry_after_failure",
    }
}

/// Updates shared metadata on one compaction section record.
/// 更新单条压缩分段记录上的共享元数据。
pub(crate) fn set_webui_context_compaction_section_metadata_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    message_id: &str,
    section_id: &str,
    role: &str,
    state_label: &str,
    budget_note: Option<&str>,
    trigger: &str,
) -> bool {
    let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .find(|record| record.id == message_id)
    else {
        return false;
    };
    let mut metadata = record.metadata.take().unwrap_or_else(|| json!({}));
    if let Some(object) = metadata.as_object_mut() {
        object.insert(String::from("context_compaction_status"), json!(true));
        object.insert(
            String::from("context_compaction_section_id"),
            json!(section_id),
        );
        object.insert(String::from("context_compaction_role"), json!(role));
        object.insert(String::from("context_compaction_state"), json!(state_label));
        object.insert(String::from("context_compaction_trigger"), json!(trigger));
        if let Some(budget_note) = budget_note {
            object.insert(
                String::from("context_compaction_budget_note"),
                json!(budget_note),
            );
        }
    }
    record.metadata = Some(metadata);
    true
}

/// Updates body-row state metadata for all records in one compaction section.
/// 更新单个压缩分段内所有正文行的状态元数据。
pub(crate) fn update_webui_context_compaction_body_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    section_id: &str,
    state_label: &str,
) {
    for record in &mut runtime_state.conversation_state.records {
        let is_body = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(Value::as_str)
            == Some(section_id)
            && record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_role"))
                .and_then(Value::as_str)
                == Some("body");
        if !is_body {
            continue;
        }
        if let Some(metadata) = record.metadata.as_mut().and_then(Value::as_object_mut) {
            metadata.insert(String::from("context_compaction_state"), json!(state_label));
        }
    }
}

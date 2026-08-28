//! Context compaction messages helpers.
//! 上下文压缩 messages 辅助逻辑。

use super::*;

/// Appends the terminal visible boundary for a failed compaction section.
/// 追加压缩失败分段的终止可见边界。
pub(crate) fn append_webui_context_compaction_failed_message_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
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
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry)?;
    Ok(())
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

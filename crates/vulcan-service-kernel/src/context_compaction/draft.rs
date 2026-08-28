//! Context compaction draft helpers.
//! 上下文压缩 draft 辅助逻辑。

use super::*;

/// Appends the visible start boundary for one WebUI context-compaction section.
/// 为 WebUI 上下文压缩分段追加可见开始边界。
pub(crate) fn append_webui_context_compaction_status_message_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    budget_note: &str,
    trigger: &str,
) -> Result<String, String> {
    let now = current_unix_millis_text();
    let mut runtime_state = workspace
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.running"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_state": "running",
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
        })),
        created_at: String::new(),
    };
    let message_id =
        append_webui_session_message_entry_to_state(&mut runtime_state, session_id, entry)?;
    set_webui_context_compaction_section_metadata_in_state(
        &mut runtime_state,
        &message_id,
        &message_id,
        "start",
        "running",
        Some(budget_note),
        trigger,
    );
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    Ok(message_id)
}

/// Appends model-produced summary output into the visible compaction body row.
/// 将模型生成的总结输出追加到可见压缩正文行。
pub(crate) fn append_webui_context_compaction_output_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    output_message_id: &mut Option<String>,
    delta: &str,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        if output_message_id.is_none() {
            let entry = SessionMessageEntry {
                id: String::new(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionStatus,
                visible: true,
                title: String::from("context_compaction.summary"),
                body: String::new(),
                metadata: Some(json!({
                    "context_compaction_status": true,
                    "context_compaction_section_id": section_id,
                    "context_compaction_role": "body",
                    "context_compaction_state": "running",
                    "context_compaction_trigger": trigger,
                    "context_compaction_budget_note": budget_note,
                })),
                created_at: String::new(),
            };
            *output_message_id = Some(append_webui_session_message_entry_to_state(
                runtime_state,
                session_id,
                entry,
            )?);
        }
        let message_id = output_message_id
            .as_deref()
            .ok_or_else(|| String::from("context compaction output message id is missing"))?;
        append_record_body_delta(&mut runtime_state.conversation_state, message_id, delta);
        runtime_state.updated_at = now;
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    }
    emit_context_compaction_message_window_delta(app, state, session_id)
}

/// Appends one visible progress line to the running context-compaction status section.
/// 向正在运行的上下文压缩状态分段追加一条可见进度行。
pub(crate) fn append_webui_context_compaction_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    progress: &PromptCompactionProgressEvent,
) -> Result<(), String> {
    let progress_stage = progress.stage.trim();
    if progress_stage.is_empty() {
        return Ok(());
    }
    let line = progress_stage.to_string();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let existing_progress =
            runtime_state
                .conversation_state
                .records
                .iter_mut()
                .find(|record| {
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
                            == Some("progress")
                });
        match existing_progress {
            Some(record) => {
                if !record.body.lines().any(|existing| existing == line) {
                    if !record.body.trim().is_empty() {
                        record.body.push('\n');
                    }
                    record.body.push_str(&line);
                }
                if let Some(metadata) = record.metadata.as_mut().and_then(Value::as_object_mut) {
                    metadata.insert(
                        String::from("context_compaction_progress_stage"),
                        json!(progress.stage),
                    );
                    metadata.insert(String::from("context_compaction_state"), json!("running"));
                }
            }
            None => {
                let entry = SessionMessageEntry {
                    id: String::new(),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::CompactionStatus,
                    visible: true,
                    title: String::from("context_compaction.progress"),
                    body: line,
                    metadata: Some(json!({
                        "context_compaction_status": true,
                        "context_compaction_section_id": section_id,
                        "context_compaction_role": "progress",
                        "context_compaction_state": "running",
                        "context_compaction_trigger": trigger,
                        "context_compaction_budget_note": budget_note,
                        "context_compaction_progress_stage": progress.stage,
                    })),
                    created_at: String::new(),
                };
                let message_id =
                    append_webui_session_message_entry_to_state(runtime_state, session_id, entry)?;
                set_webui_context_compaction_section_metadata_in_state(
                    runtime_state,
                    &message_id,
                    section_id,
                    "progress",
                    "running",
                    Some(budget_note),
                    trigger,
                );
            }
        }
        runtime_state.updated_at = now;
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    }
    emit_context_compaction_message_window_delta(app, state, session_id)
}

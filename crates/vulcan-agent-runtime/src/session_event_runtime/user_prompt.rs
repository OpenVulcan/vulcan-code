use super::*;

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

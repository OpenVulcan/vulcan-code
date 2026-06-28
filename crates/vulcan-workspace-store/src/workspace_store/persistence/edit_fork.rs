use super::*;

/// 从分段会话事实源加载出的持久化编辑草稿。
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedUserPromptEditDraft {
    /// Editable prompt text from the canonical user message.
    /// 来自规范用户消息的可编辑提示词文本。
    pub prompt: String,
    /// Attachments from the canonical user message.
    /// 来自规范用户消息的附件。
    pub attachments: Vec<ConversationAttachment>,
}

/// Persisted rollback result written before replaying an edited prompt.
/// 在重放编辑后提示词之前写入的持久化回滚结果。
#[derive(Debug, Clone)]
pub struct PersistedUserPromptRollbackResult {
    /// Retained transcript messages before the edited user prompt.
    /// 编辑用户提示词之前保留的转录消息。
    pub retained_messages: Vec<SessionMessageEntry>,
    /// Runtime state rebuilt from the retained canonical conversation state.
    /// 从保留的规范会话状态重建出的运行态。
    pub runtime_state: SessionRuntimeStateEntry,
}

/// Fork result returned after copying one persisted segmented session.
/// 复制单个持久化分段会话后返回的分叉结果。
#[derive(Debug, Clone)]
pub struct PersistedSessionForkResult {
    /// Newly created session summary.
    /// 新创建的会话摘要。
    pub session: SessionSummary,
}

/// Loads one user-prompt edit draft from persisted segmented session state.
/// 从持久化分段会话状态加载一份用户提示词编辑草稿。
pub async fn load_persisted_user_prompt_edit_draft(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
) -> Result<PersistedUserPromptEditDraft, String> {
    let target = locate_persisted_user_prompt_target(controller, session_id, message_id)?;
    Ok(PersistedUserPromptEditDraft {
        prompt: target.prompt,
        attachments: target.attachments,
    })
}

/// Rolls one persisted session back to the point before the selected user prompt.
/// 将一个持久化会话回滚到所选用户提示词之前。
pub async fn rollback_persisted_session_before_user_prompt(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
) -> Result<PersistedUserPromptRollbackResult, String> {
    let target = locate_persisted_user_prompt_target(controller, session_id, message_id)?;
    truncate_segmented_session_before_message(
        &target.session_dir,
        message_id,
        &target.retained_messages,
    )?;
    let mut runtime_state = SessionRuntimeStateEntry {
        conversation_state: target.retained_conversation_state,
        updated_at: current_unix_millis_text(),
        ..SessionRuntimeStateEntry::default()
    };
    runtime_state.last_compaction_id = target.last_compaction_id;
    runtime_state.context_start_message_id = target.context_start_message_id;
    runtime_state.conversation_state.continuation = Default::default();
    let state_document =
        state_document_from_runtime_state(&session_id.to_string(), &runtime_state)?;
    write_state(&target.session_dir, &state_document)?;
    rebuild_runtime_log_from_conversation_state(
        &target.session_dir,
        &runtime_state.conversation_state,
        &runtime_state.updated_at,
    )?;
    Ok(PersistedUserPromptRollbackResult {
        retained_messages: target.retained_messages,
        runtime_state,
    })
}

/// Forks a persisted segmented session by copying its backing directory and indexing the clone.
/// 通过复制底层目录并写入索引来分叉一个持久化分段会话。
pub async fn fork_persisted_session(
    controller: &ControllerRuntimeHandle,
    source_session: &SessionSummary,
    project_id: &str,
    target_session_id: SessionId,
    target_title: String,
) -> Result<PersistedSessionForkResult, String> {
    let now = current_unix_millis_text();
    let target_session_id_text = target_session_id.to_string();
    if let Some(source_dir) = find_segmented_session_directory_by_id(
        &controller.data_root,
        &source_session.id.to_string(),
    )? {
        let _ = repair_segmented_session_display_if_needed(&source_dir)?;
    }
    fork_segmented_session_directory(
        &controller.data_root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            target_project_tag: project_id.to_string(),
            target_session_id: target_session_id_text.clone(),
            target_created_date: persistence_segmented_session_date_from_unix_millis_text(&now),
            target_created_at: now.clone(),
            target_title: target_title.clone(),
            target_title_source: source_session.title_source.as_persisted_key().to_string(),
        },
    )?;
    let forked_session = SessionSummary::new(
        target_session_id,
        Some(project_id.to_string()),
        target_title,
        source_session.focus.clone(),
        SessionState::Completed,
        source_session.changed_files,
        source_session.queued_steps,
        "",
    )
    .with_title_source(source_session.title_source)
    .with_created_at(now.clone())
    .with_updated_at(now.clone());
    upsert_session_index_row(
        controller,
        &forked_session,
        project_id,
        false,
        0,
        Some(&now),
    )
    .await?;
    Ok(PersistedSessionForkResult {
        session: forked_session,
    })
}

/// Records a display-only source marker for a forked persisted session.
/// 为已分叉的持久化会话记录纯展示用来源标记。
pub fn record_persisted_session_fork_source_marker(
    controller: &ControllerRuntimeHandle,
    target_session_id: SessionId,
    source_session: &SessionSummary,
    source_project_id: &str,
) -> Result<(), String> {
    let session_dir = find_segmented_session_directory_by_id(
        &controller.data_root,
        &target_session_id.to_string(),
    )?
    .ok_or_else(|| String::from("ERR_SESSION_FORK_TARGET_NOT_FOUND"))?;
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    let replay = replay_segmented_session_context(&session_dir)?;
    let anchor_message_id = replay
        .as_ref()
        .and_then(|replay| replay.conversation_state.messages.last())
        .map(|message| message.id.clone());
    write_segmented_session_fork_source(
        &session_dir,
        SegmentedSessionForkSource {
            source_project_tag: source_project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            source_title: source_session.title.clone(),
            anchor_message_id,
            created_at: current_unix_millis_text(),
        },
    )
}

/// Located persisted user prompt and its retained prefix.
/// 已定位的持久化用户提示词及其保留前缀。
struct PersistedUserPromptTarget {
    /// Segmented session directory that owns the target message.
    /// 拥有目标消息的分段会话目录。
    session_dir: std::path::PathBuf,
    /// Editable prompt text from the canonical user message.
    /// 来自规范用户消息的可编辑提示词文本。
    prompt: String,
    /// Attachments from the canonical user message.
    /// 来自规范用户消息的附件。
    attachments: Vec<ConversationAttachment>,
    /// Retained canonical conversation state before the target user prompt.
    /// 目标用户提示词之前保留的规范会话状态。
    retained_conversation_state: SessionConversationState,
    /// Retained transcript messages before the target user prompt.
    /// 目标用户提示词之前保留的转录消息。
    retained_messages: Vec<SessionMessageEntry>,
    /// Last compaction id preserved from segmented replay.
    /// 从分段重放中保留的最近压缩标识。
    last_compaction_id: Option<String>,
    /// Context-start message id preserved from segmented replay.
    /// 从分段重放中保留的上下文起点消息标识。
    context_start_message_id: Option<String>,
}

/// Locates one editable user prompt from persisted segmented replay output.
/// 从持久化分段重放结果中定位一个可编辑用户提示词。
fn locate_persisted_user_prompt_target(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
) -> Result<PersistedUserPromptTarget, String> {
    let session_dir =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id.to_string())?
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_SESSION_NOT_FOUND"))?;
    let _ = repair_segmented_session_display_if_needed(&session_dir)?;
    let replay = replay_segmented_session_context(&session_dir)?
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_EMPTY_SESSION"))?;
    let record_index = replay
        .conversation_state
        .records
        .iter()
        .position(|record| {
            record.id == message_id
                && record.visible
                && record.kind == SessionMessageKind::User
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"))?;
    let message_index = replay
        .conversation_state
        .messages
        .iter()
        .position(|message| message.id == message_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_PROTOCOL_MISMATCH"))?;
    let SessionConversationPayload::UserText { text, attachments } =
        &replay.conversation_state.messages[message_index].payload
    else {
        return Err(String::from("ERR_USER_PROMPT_EDIT_PROTOCOL_MISMATCH"));
    };
    let prompt = text.clone();
    let attachments = attachments.clone();
    let last_compaction_id = replay.last_compaction_id;
    let context_start_message_id = replay.context_start_message_id;
    let mut retained_conversation_state = replay.conversation_state;
    retained_conversation_state.records.truncate(record_index);
    retained_conversation_state.messages.truncate(message_index);
    retained_conversation_state.continuation = Default::default();
    let retained_messages = conversation_state_to_session_messages(&retained_conversation_state);
    Ok(PersistedUserPromptTarget {
        session_dir,
        prompt,
        attachments,
        retained_conversation_state,
        retained_messages,
        last_compaction_id,
        context_start_message_id,
    })
}

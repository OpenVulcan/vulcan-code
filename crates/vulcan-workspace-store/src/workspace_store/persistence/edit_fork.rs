use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    session: &SessionSummary,
    project_id: &str,
    message_id: &str,
) -> Result<PersistedUserPromptEditDraft, String> {
    let target = locate_persisted_user_prompt_target(controller, session, project_id, message_id)?;
    Ok(PersistedUserPromptEditDraft {
        prompt: target.prompt,
        attachments: target.attachments,
    })
}

/// Rolls one persisted session back to the point before the selected user prompt.
/// 将一个持久化会话回滚到所选用户提示词之前。
pub async fn rollback_persisted_session_before_user_prompt(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    message_id: &str,
) -> Result<PersistedUserPromptRollbackResult, String> {
    let target = locate_persisted_user_prompt_target(controller, session, project_id, message_id)?;
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
        state_document_from_runtime_state(&session.id.to_string(), &runtime_state)?;
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
    anchor_message_id: Option<&str>,
) -> Result<PersistedSessionForkResult, String> {
    let now = current_unix_millis_text();
    let target_session_id_text = target_session_id.to_string();
    let anchor_message_id = anchor_message_id
        .map(str::trim)
        .filter(|message_id| !message_id.is_empty());
    let source_dir = locate_segmented_session_dir_for_summary(
        controller,
        source_session,
        project_id,
        "ERR_SESSION_FORK_SOURCE_NOT_FOUND",
    )?;
    let _ = repair_segmented_session_display_if_needed(&source_dir)?;
    let source_replay = replay_segmented_session_context_without_repair(&source_dir)?;
    let source_anchor_index = anchor_message_id
        .map(|message_id| {
            let replay = source_replay
                .as_ref()
                .ok_or_else(|| String::from("ERR_SESSION_FORK_EMPTY_SESSION"))?;
            fork_anchor_record_index_in_replay(replay, message_id)
        })
        .transpose()?;
    let source_last_visible_index = source_replay
        .as_ref()
        .and_then(last_visible_record_index_in_replay);
    let truncate_after_anchor = match (source_anchor_index, source_last_visible_index) {
        (Some(anchor_index), Some(last_index)) => anchor_index < last_index,
        _ => false,
    };
    let identity_replacements =
        forked_identity_replacements_for_persisted_session(source_session.id, target_session_id);
    let source_anchor_message_id_for_display = anchor_message_id
        .filter(|_| truncate_after_anchor)
        .map(ToOwned::to_owned);
    let target_dir = fork_segmented_session_directory_from_source(
        &source_dir,
        &controller.data_root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            target_project_tag: project_id.to_string(),
            target_session_id: target_session_id_text.clone(),
            target_created_date: session_directory_date_from_unix_millis_text(&now)?,
            target_created_at: now.clone(),
            target_title: target_title.clone(),
            target_title_source: source_session.title_source.as_persisted_key().to_string(),
            source_anchor_message_id: source_anchor_message_id_for_display,
        },
    )?;
    let target_anchor_message_id = anchor_message_id.map(|message_id| {
        forked_anchor_message_id(message_id, source_session.id, target_session_id)
    });
    let fork_source_anchor_message_id = target_anchor_message_id.clone().or_else(|| {
        source_last_visible_index
            .and_then(|record_index| {
                source_replay
                    .as_ref()
                    .and_then(|replay| replay.conversation_state.records.get(record_index))
            })
            .map(|record| {
                forked_anchor_message_id(&record.id, source_session.id, target_session_id)
            })
    });
    let fork_result = async {
        if truncate_after_anchor
            && target_anchor_message_id.is_some()
            && let Some(anchor_index) = source_anchor_index
        {
            let replay = source_replay
                .as_ref()
                .ok_or_else(|| String::from("ERR_SESSION_FORK_EMPTY_SESSION"))?;
            rebuild_forked_persisted_session_runtime_from_source_replay(
                &target_dir,
                &target_session_id_text,
                replay,
                anchor_index,
                &identity_replacements,
            )?;
        }
        record_persisted_session_fork_source_marker(
            &target_dir,
            source_session,
            project_id,
            fork_source_anchor_message_id,
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
    .await;
    if fork_result.is_err() {
        let _ = std::fs::remove_dir_all(&target_dir);
    }
    fork_result
}

/// Rebuilds forked runtime artifacts from the already replayed source state.
/// 从已重放的源会话状态重建分叉运行态产物。
fn rebuild_forked_persisted_session_runtime_from_source_replay(
    session_dir: &Path,
    target_session_id_text: &str,
    source_replay: &SegmentedSessionReplayResult,
    source_anchor_record_index: usize,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut retained_conversation_state = source_replay.conversation_state.clone();
    retained_conversation_state
        .records
        .truncate(source_anchor_record_index.saturating_add(1));
    retained_conversation_state.continuation = Default::default();
    rewrite_forked_conversation_state_identity(&mut retained_conversation_state, replacements)?;
    let original_runtime_messages = retained_conversation_state.messages.clone();
    let retained_messages = conversation_state_to_session_messages(&retained_conversation_state);
    let retained_context_start_message_id = retained_context_anchor(
        source_replay
            .context_start_message_id
            .clone()
            .map(|message_id| rewrite_forked_identity_text(message_id, replacements)),
        &retained_messages,
    );
    retained_conversation_state = fork_conversation_state_from_retained_messages(
        &retained_messages,
        &original_runtime_messages,
    )?;
    let mut metadata = read_session_metadata(session_dir)?;
    update_segmented_message_checkpoint(&mut metadata, &retained_messages)?;
    write_metadata(session_dir, &metadata)?;
    let updated_at = current_unix_millis_text();
    let mut runtime_state = SessionRuntimeStateEntry {
        conversation_state: retained_conversation_state,
        updated_at,
        ..SessionRuntimeStateEntry::default()
    };
    runtime_state.last_compaction_id = source_replay.last_compaction_id.clone();
    runtime_state.context_start_message_id = retained_context_start_message_id;
    runtime_state.conversation_state.continuation = Default::default();
    let state_document = state_document_from_runtime_state(target_session_id_text, &runtime_state)?;
    write_state(session_dir, &state_document)?;
    rebuild_runtime_log_from_conversation_state(
        session_dir,
        &runtime_state.conversation_state,
        &runtime_state.updated_at,
    )?;
    Ok(())
}

/// Builds the exact session-id replacement table used by persisted fork reconstruction.
/// 构建持久化分叉重建使用的精确会话标识替换表。
fn forked_identity_replacements_for_persisted_session(
    source_session_id: SessionId,
    target_session_id: SessionId,
) -> BTreeMap<String, String> {
    let mut replacements = BTreeMap::new();
    replacements.insert(source_session_id.to_string(), target_session_id.to_string());
    replacements
}

/// Rewrites session identity tokens inside an already replayed conversation state.
/// 改写已重放会话状态内部的会话身份 token。
fn rewrite_forked_conversation_state_identity(
    conversation_state: &mut SessionConversationState,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut state_value = serde_json::to_value(&conversation_state).map_err(to_store_error)?;
    rewrite_json_value_identity_tokens(&mut state_value, replacements);
    *conversation_state = serde_json::from_value(state_value).map_err(to_store_error)?;
    Ok(())
}

/// Rewrites one text value with the fork identity replacement table.
/// 使用分叉身份替换表改写单个文本值。
fn rewrite_forked_identity_text(
    mut text: String,
    replacements: &BTreeMap<String, String>,
) -> String {
    rewrite_string_identity_tokens(&mut text, replacements);
    text
}

/// Returns the retained display-record index for one message-scoped fork anchor.
/// 返回消息级分叉锚点对应的保留展示记录下标。
fn fork_anchor_record_index_in_replay(
    replay: &SegmentedSessionReplayResult,
    anchor_message_id: &str,
) -> Result<usize, String> {
    replay
        .conversation_state
        .records
        .iter()
        .position(|record| record.id == anchor_message_id && record.visible)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_ANCHOR_NOT_FOUND"))
        .and_then(|record_index| {
            ensure_fork_anchor_after_context_start(replay, record_index)?;
            Ok(record_index)
        })
}

/// Returns the last visible display-record index in one replayed segmented session.
/// 返回一份分段会话重放结果中的最后一条可见展示记录下标。
fn last_visible_record_index_in_replay(replay: &SegmentedSessionReplayResult) -> Option<usize> {
    replay
        .conversation_state
        .records
        .iter()
        .rposition(|record| record.visible)
}

/// Rejects anchors that belong to display history before the active runtime context boundary.
/// 拒绝位于活动运行时上下文边界之前的展示历史锚点。
fn ensure_fork_anchor_after_context_start(
    replay: &SegmentedSessionReplayResult,
    record_index: usize,
) -> Result<(), String> {
    let Some(context_start_message_id) = replay
        .context_start_message_id
        .as_deref()
        .map(str::trim)
        .filter(|message_id| !message_id.is_empty())
    else {
        return Ok(());
    };
    let context_start_index = replay
        .conversation_state
        .records
        .iter()
        .position(|record| record.id == context_start_message_id)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_CONTEXT_START_NOT_FOUND"))?;
    if record_index < context_start_index {
        return Err(String::from("ERR_SESSION_FORK_ANCHOR_NOT_FOUND"));
    }
    Ok(())
}

/// Rebuilds fork runtime state from retained display records while preserving exact original runtime payloads when ids still match.
/// 从保留展示记录重建分叉运行态，并在标识仍匹配时保留原始运行时载荷。
fn fork_conversation_state_from_retained_messages(
    retained_messages: &[SessionMessageEntry],
    original_runtime_messages: &[SessionConversationMessage],
) -> Result<SessionConversationState, String> {
    let mut conversation_state = conversation_state_from_session_messages(retained_messages)?;
    for message in &mut conversation_state.messages {
        if let Some(original_message) = original_runtime_messages
            .iter()
            .find(|original_message| original_message.id == message.id)
        {
            *message = original_message.clone();
        }
    }
    Ok(conversation_state)
}

/// Preserves the context-start anchor only when it is still present after fork truncation.
/// 仅当上下文起点锚点在分叉截断后仍然存在时保留它。
fn retained_context_anchor(
    context_start_message_id: Option<String>,
    retained_messages: &[SessionMessageEntry],
) -> Option<String> {
    context_start_message_id.filter(|message_id| {
        retained_messages
            .iter()
            .any(|message| message.id == message_id.as_str() && message.visible)
    })
}

/// Records a display-only source marker for a forked persisted session.
/// 为已分叉的持久化会话记录纯展示用来源标记。
fn record_persisted_session_fork_source_marker(
    session_dir: &Path,
    source_session: &SessionSummary,
    source_project_id: &str,
    anchor_message_id: Option<String>,
) -> Result<(), String> {
    write_segmented_session_fork_source(
        session_dir,
        SegmentedSessionForkSource {
            source_project_tag: source_project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            source_title: source_session.title.clone(),
            anchor_message_id,
            created_at: current_unix_millis_text(),
        },
    )
}

/// Locates one known session directory by its project and creation timestamp instead of scanning all sessions.
/// 通过项目与创建时间定位一条已知会话目录，而不是扫描全部会话。
fn locate_segmented_session_dir_for_summary(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    missing_error: &str,
) -> Result<PathBuf, String> {
    let created_at = normalized_session_created_at(session);
    find_segmented_session_directory_for_task_store_created_at(
        &controller.data_root,
        project_id,
        &created_at,
        &session.id.to_string(),
    )
    .ok_or_else(|| missing_error.to_string())
}

/// Rewrites a source message anchor the same way the forked directory identity rewrite does.
/// 按分叉目录身份改写的同一规则改写源消息锚点。
fn forked_anchor_message_id(
    anchor_message_id: &str,
    source_session_id: SessionId,
    target_session_id: SessionId,
) -> String {
    anchor_message_id.replace(
        &source_session_id.to_string(),
        &target_session_id.to_string(),
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
    session: &SessionSummary,
    project_id: &str,
    message_id: &str,
) -> Result<PersistedUserPromptTarget, String> {
    let session_dir = locate_segmented_session_dir_for_summary(
        controller,
        session,
        project_id,
        "ERR_USER_PROMPT_EDIT_SESSION_NOT_FOUND",
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one persisted transcript message for fork-anchor lookup tests.
    /// 为分叉锚点查找测试构建一条持久化转录消息。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        let mut message = SessionMessageEntry::new(kind, "Title", body)
            .with_runtime_metadata(id, format!("2026-07-09T10:00:0{}Z", id.len()));
        message.record_kind = record_kind;
        message
    }

    /// Builds one replay result from the current runtime-context messages.
    /// 根据当前运行时上下文消息构建一份重放结果。
    fn replay_from_messages(messages: Vec<SessionMessageEntry>) -> SegmentedSessionReplayResult {
        SegmentedSessionReplayResult {
            conversation_state: conversation_state_from_session_messages(&messages)
                .expect("valid replayed conversation identities"),
            last_compaction_id: Some(String::from("segmented-compaction-test")),
            context_start_message_id: messages.first().map(|message| message.id.clone()),
            updated_at: Some(String::from("2026-07-09T10:00:00Z")),
        }
    }

    /// Verifies a fork anchor is accepted when it belongs to the visible transcript records.
    /// 验证分叉锚点属于可见转录记录时会被接受。
    #[test]
    fn fork_anchor_record_index_accepts_current_display_anchor() {
        let replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "summary-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "compressed answer",
            ),
            test_message(
                "live-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "new answer",
            ),
        ]);

        let record_index = fork_anchor_record_index_in_replay(&replay, "live-assistant")
            .expect("current display anchor should be accepted");

        assert_eq!(record_index, 2);
    }

    /// Verifies fork validation does not require a matching provider-runtime message id.
    /// 验证分叉校验不要求存在同名 Provider 运行时消息标识。
    #[test]
    fn fork_anchor_record_index_accepts_display_anchor_without_runtime_message() {
        let mut replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "display-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "visible answer",
            ),
        ]);
        replay.conversation_state.messages.clear();

        let record_index = fork_anchor_record_index_in_replay(&replay, "display-assistant")
            .expect("display anchor should not depend on provider messages");

        assert_eq!(record_index, 1);
    }

    /// Verifies visible records before the active context-start anchor cannot be forked.
    /// 验证活动上下文起点之前的可见记录不能作为分叉锚点。
    #[test]
    fn fork_anchor_record_index_rejects_anchor_before_context_start() {
        let mut replay = replay_from_messages(vec![
            test_message(
                "old-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "old answer",
            ),
            test_message(
                "compaction-summary",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::CompactionSummary,
                "compressed context",
            ),
            test_message(
                "live-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "new answer",
            ),
        ]);
        replay.context_start_message_id = Some(String::from("compaction-summary"));

        let error = fork_anchor_record_index_in_replay(&replay, "old-assistant")
            .expect_err("pre-context display anchor should be rejected");

        assert_eq!(error, "ERR_SESSION_FORK_ANCHOR_NOT_FOUND");
        let record_index = fork_anchor_record_index_in_replay(&replay, "live-assistant")
            .expect("post-context display anchor should be accepted");
        assert_eq!(record_index, 2);
    }

    /// Verifies a fork anchor from pruned display history is rejected by runtime replay validation.
    /// 验证来自已裁剪展示历史的分叉锚点会被运行时重放校验拒绝。
    #[test]
    fn fork_anchor_record_index_rejects_pruned_display_anchor() {
        let replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "summary-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "compressed answer",
            ),
        ]);

        let error = fork_anchor_record_index_in_replay(&replay, "old-assistant")
            .expect_err("pruned display anchor should be rejected");

        assert_eq!(error, "ERR_SESSION_FORK_ANCHOR_NOT_FOUND");
    }

    /// Verifies fork truncation uses the target-side message id after session identity rewriting.
    /// 验证分叉截断在会话身份改写后使用目标侧消息标识。
    #[test]
    fn forked_anchor_message_id_matches_session_identity_rewrite() {
        let source_session_id = SessionId::from_u128(610);
        let target_session_id = SessionId::from_u128(611);
        let source_anchor_message_id = format!("{source_session_id}-1783590054811-255");

        let target_anchor_message_id = forked_anchor_message_id(
            &source_anchor_message_id,
            source_session_id,
            target_session_id,
        );

        assert_eq!(
            target_anchor_message_id,
            format!("{target_session_id}-1783590054811-255")
        );
    }

    /// Verifies source replay reconstruction rewrites both visible and runtime message identities.
    /// 验证源重放重建会同时改写展示消息与运行时消息身份。
    #[test]
    fn forked_source_replay_identity_rewrite_updates_retained_state_ids() {
        let source_session_id = SessionId::from_u128(710);
        let target_session_id = SessionId::from_u128(711);
        let source_message_id = format!("{source_session_id}-source-message");
        let target_message_id = format!("{target_session_id}-source-message");
        let mut replay = replay_from_messages(vec![test_message(
            &source_message_id,
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "source prompt",
        )]);
        replay.context_start_message_id = Some(source_message_id.clone());
        let replacements = forked_identity_replacements_for_persisted_session(
            source_session_id,
            target_session_id,
        );
        let mut retained_conversation_state = replay.conversation_state.clone();

        rewrite_forked_conversation_state_identity(&mut retained_conversation_state, &replacements)
            .expect("rewrite source replay identity");

        let retained_messages =
            conversation_state_to_session_messages(&retained_conversation_state);
        let retained_context_start_message_id = retained_context_anchor(
            replay
                .context_start_message_id
                .map(|message_id| rewrite_forked_identity_text(message_id, &replacements)),
            &retained_messages,
        );
        let rewritten_state =
            serde_json::to_string(&retained_conversation_state).expect("serialize state");
        assert!(rewritten_state.contains(&target_message_id));
        assert!(!rewritten_state.contains(&source_message_id));
        assert_eq!(retained_context_start_message_id, Some(target_message_id));
    }
}

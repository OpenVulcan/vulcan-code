//! conversation message projection helpers.
//! conversation message projection 辅助逻辑。

use super::*;

/// Converts session transcript entries into WebUI messages.
/// 将会话转录项转换为 WebUI 消息。
pub(crate) fn conversation_messages_from_session(
    conversation_id: &str,
    messages: &[SessionMessageEntry],
    allow_user_prompt_editing: bool,
) -> Vec<ConversationMessage> {
    let editable_user_prompt_ids = if allow_user_prompt_editing {
        editable_user_prompt_message_ids_from_entries(messages)
    } else {
        BTreeSet::new()
    };
    messages
        .iter()
        .filter(|message| message.is_visible_to_user())
        .filter(|message| {
            !message.body.trim().is_empty()
                || matches!(
                    message.record_kind,
                    SessionMessageRecordKind::ToolCallOutput
                        | SessionMessageRecordKind::ToolResultOutput
                        | SessionMessageRecordKind::TurnStats
                        | SessionMessageRecordKind::CompactionStatus
                )
        })
        .map(|message| {
            let mut projected = conversation_message_from_session_entry(conversation_id, message);
            projected.editable_user_prompt = editable_user_prompt_ids.contains(&message.id);
            projected
        })
        .collect()
}

/// Computes visible user prompt ids that have no later successful compaction or applied change-set blocker.
/// 计算后续没有成功压缩或已应用 change_set 阻断的可见用户提示词标识。
pub(crate) fn editable_user_prompt_message_ids_from_entries(
    messages: &[SessionMessageEntry],
) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for (index, message) in messages.iter().enumerate() {
        if !session_entry_allows_user_prompt_edit(message) {
            continue;
        }
        if messages[index + 1..]
            .iter()
            .any(session_entry_blocks_user_prompt_edit)
        {
            continue;
        }
        result.insert(message.id.clone());
    }
    result
}

/// Returns whether one workspace snapshot allows editing the selected user prompt.
/// 返回单个工作区快照是否允许编辑所选用户提示词。
pub(crate) fn workspace_user_prompt_message_allows_destructive_edit(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    message_id: &str,
) -> bool {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .is_some_and(|messages| {
            editable_user_prompt_message_ids_from_entries(messages).contains(message_id)
        })
}

/// Returns whether one user transcript entry is a normal editable turn opener.
/// 返回单条用户转录项是否是常规且可编辑的轮次开启项。
pub(crate) fn session_entry_allows_user_prompt_edit(message: &SessionMessageEntry) -> bool {
    if message.kind != SessionMessageKind::User
        || message.record_kind != SessionMessageRecordKind::Transcript
        || !message.visible
        || message.id.trim().is_empty()
    {
        return false;
    }
    message
        .metadata
        .as_ref()
        .is_none_or(|metadata| metadata_allows_user_prompt_edit(metadata))
}

/// Returns whether transcript metadata permits destructive user-prompt editing.
/// 返回转录元数据是否允许破坏性用户提示词编辑。
pub(crate) fn metadata_allows_user_prompt_edit(metadata: &Value) -> bool {
    let starts_user_turn = metadata
        .get("starts_user_turn")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let editable = metadata
        .get("editable")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    starts_user_turn && editable
}

/// Returns whether one later transcript entry makes earlier user-prompt editing unsafe.
/// 返回单条后续转录项是否会让更早的用户提示词编辑不安全。
pub(crate) fn session_entry_blocks_user_prompt_edit(message: &SessionMessageEntry) -> bool {
    message.record_kind == SessionMessageRecordKind::CompactionSummary
        || message
            .metadata
            .as_ref()
            .is_some_and(metadata_has_applied_change_set)
        || message
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_payload_from_tool_metadata(metadata).is_some())
}

/// Returns whether transcript metadata contains an applied canonical `change_set` host result.
/// 返回转录 metadata 是否包含已应用的 canonical `change_set` 宿主结果。
pub(crate) fn metadata_has_applied_change_set(metadata: &Value) -> bool {
    let nested = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            host_result
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "change_set")
        });
    let flattened =
        (metadata.get("kind").and_then(Value::as_str) == Some("change_set")).then_some(metadata);
    nested
        .or(flattened)
        .and_then(|host_result| host_result.get("payload").or(Some(host_result)))
        .and_then(|payload| payload.get("mode").and_then(Value::as_str))
        .is_some_and(|mode| mode == "applied")
}

/// Returns whether a persisted session has readable segmented metadata on disk.
/// 返回持久化会话在磁盘上是否存在可读取的分段元数据。
pub(crate) fn session_segmented_metadata_exists(
    session: &vulcan_workbench_model::SessionSummary,
) -> bool {
    let Some(project_id) = session.project_id.as_deref() else {
        return false;
    };
    vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
        &vulcan_workspace_store::data_plane::application_data_root(),
        project_id,
        &session.id.to_string(),
    )
    .ok()
    .flatten()
    .is_some_and(|session_dir| session_dir.join("metadata.json").is_file())
}

/// Builds the WebUI fork-source summary for one session when metadata carries it.
/// 当元数据携带分叉来源时，为单个会话构建 WebUI 来源摘要。
pub(crate) fn conversation_fork_source_summary_from_session(
    session: &vulcan_workbench_model::SessionSummary,
) -> Option<ConversationForkSourceSummary> {
    let project_id = session.project_id.as_deref()?;
    let source = load_segmented_session_fork_source_for_project(
        &vulcan_workspace_store::data_plane::application_data_root(),
        project_id,
        session.id,
    )
    .ok()
    .flatten()?;
    Some(ConversationForkSourceSummary {
        source_project_tag: source.source_project_tag,
        source_conversation_id: source.source_session_id,
        source_title: source.source_title,
        anchor_message_id: source.anchor_message_id,
        created_at: source.created_at,
    })
}

/// Returns a trimmed string only when it contains visible content.
/// 仅当字符串包含可见内容时返回裁剪后的字符串。
pub(crate) fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

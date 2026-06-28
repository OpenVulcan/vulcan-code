//! Conversation display message projection helpers.
//! 会话显示消息投影辅助函数。

use super::*;

/// Returns the display transcript window for one session from a concrete workspace snapshot.
/// 从具体工作区快照返回单个会话的显示层转录窗口。
pub(crate) fn display_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(messages) = workspace_snapshot.session_messages.get(&session_id) {
        return Some(filter_legacy_goal_internal_user_messages(messages.clone()));
    }
    display_record_messages_from_workspace(workspace_snapshot, session_id)
}

/// Returns display-record messages for fallback rendering without deriving from runtime-only messages.
/// 返回用于显示回退的记录流消息，不从仅运行态消息派生。
pub(crate) fn display_record_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
        if !messages.is_empty() {
            return Some(filter_legacy_goal_internal_user_messages(messages));
        }
    }
    None
}

/// Merges one runtime dialogue state into the display transcript window for a session.
/// 将单个运行态对话状态合并进对应会话的显示层转录窗口。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose display layer will be updated.
/// `workspace_snapshot` 是显示层将被更新的工作区快照。
/// `session_id` selects the session that owns both layers.
/// `session_id` 选择同时拥有两层状态的会话。
/// `runtime_state` is the dialogue-layer runtime state to project into display rows.
/// `runtime_state` 是需要投影成显示行的对话层运行态。
pub(crate) fn merge_runtime_state_records_into_display_messages(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) {
    let merged_messages = merge_session_messages_with_conversation_state_records(
        workspace_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &runtime_state.conversation_state,
    );
    workspace_snapshot
        .session_messages
        .insert(session_id, merged_messages);
}

/// Removes legacy persisted Goal internal prompts that were incorrectly stored as user messages.
/// 移除历史上被错误持久化成用户消息的 Goal 内部提示词。
pub(crate) fn filter_legacy_goal_internal_user_messages(
    messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    messages
        .into_iter()
        .filter(|message| !session_message_is_legacy_goal_internal_user_prompt(message))
        .collect()
}

/// Rewrites one runtime state after removing legacy Goal internal user messages.
/// 移除历史 Goal 内部用户消息后重写单个运行态。
pub(crate) fn sanitize_runtime_state_legacy_goal_internal_messages(
    runtime_state: &mut SessionRuntimeStateEntry,
) {
    let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
    if messages
        .iter()
        .any(session_message_is_legacy_goal_internal_user_prompt)
    {
        runtime_state.conversation_state = conversation_state_from_session_messages(
            &filter_legacy_goal_internal_user_messages(messages),
        );
    }
}

/// Returns whether one message is a known legacy Goal internal prompt persisted before the hidden fix.
/// 返回单条消息是否为隐藏修复前已知被持久化的历史 Goal 内部提示词。
pub(crate) fn session_message_is_legacy_goal_internal_user_prompt(
    message: &SessionMessageEntry,
) -> bool {
    if message.kind != SessionMessageKind::User {
        return false;
    }
    let body = message.body.trim_start();
    body.starts_with("Continue working toward the active thread goal.")
        || body.starts_with("The active thread goal has reached its token budget.")
        || body.starts_with("The active thread goal objective was edited by the user.")
}

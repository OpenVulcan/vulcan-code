//! Conversation display message projection helpers.
//! 会话显示消息投影辅助函数。

use super::*;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;

/// Returns the display transcript window for one session from a concrete workspace snapshot.
/// 从具体工作区快照返回单个会话的显示层转录窗口。
pub(crate) fn display_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(messages) =
        stable_display_session_messages_from_workspace(workspace_snapshot, session_id)
    {
        return Some(messages);
    }
    display_record_messages_from_workspace(workspace_snapshot, session_id)
}

/// Returns only the stable retained display transcript rows for one session.
/// 仅返回单个会话已保留的稳定显示转录行。
pub(crate) fn stable_display_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .map(|messages| filter_display_session_messages(messages.clone()))
}

/// Returns display-record messages for fallback rendering without deriving from runtime-only messages.
/// 返回用于显示回退的记录流消息，不从仅运行态消息派生。
pub(crate) fn display_record_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        let messages = display_messages_from_conversation_state(&runtime_state.conversation_state);
        if !messages.is_empty() {
            return Some(messages);
        }
    }
    None
}

/// Projects one runtime conversation state into user-facing display transcript rows.
/// 将单个运行态会话状态投影为面向用户的显示层转录行。
///
/// # Parameters
/// 参数
/// `conversation_state` is the runtime-owned conversation state that may contain hidden records.
/// `conversation_state` 是可能包含隐藏记录的运行态会话状态。
///
/// # Returns
/// 返回值
/// Returns filtered display messages derived from the runtime conversation state.
/// 返回从运行态会话状态派生出的已过滤显示消息。
pub(crate) fn display_messages_from_conversation_state(
    conversation_state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    filter_display_session_messages(conversation_state_to_session_messages(conversation_state))
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
    let merged_messages = filter_display_session_messages(merged_messages);
    set_display_session_messages(workspace_snapshot, session_id, merged_messages);
}

/// Replaces a TaskSession display transcript with records owned by the delegated child task.
/// 使用委派给子智能体的任务自有记录替换 TaskSession 显示转录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose child display layer will be rewritten.
/// `workspace_snapshot` 是需要重写子显示层的工作区快照。
/// `session_id` selects the child TaskSession that owns the display transcript.
/// `session_id` 选择拥有显示转录的子 TaskSession。
/// `runtime_state` is the cloned-or-fresh child runtime state returned by the controller.
/// `runtime_state` 是控制器返回的克隆或全新子运行态。
/// `task_prompt_anchor` is the exact child task prompt used to separate inherited parent runtime records from child-owned display records.
/// `task_prompt_anchor` 是用于区分继承父运行态记录与子任务自有显示记录的精确任务提示词。
pub(crate) fn replace_task_display_messages_with_runtime_state_records(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
    task_prompt_anchor: Option<&str>,
) {
    let messages = display_messages_from_task_conversation_state(
        &runtime_state.conversation_state,
        task_prompt_anchor,
    );
    let messages = merge_task_display_messages_with_existing_send_message_notes(
        workspace_snapshot,
        session_id,
        messages,
    );
    set_display_session_messages(workspace_snapshot, session_id, messages);
}

/// Returns the seeded child-task prompt currently retained in the display layer.
/// 返回当前显示层中保留的已播种子任务提示词。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot that may contain a provisional child prompt row.
/// `workspace_snapshot` 是可能包含临时子任务提示词行的工作区快照。
/// `session_id` selects the child TaskSession display transcript.
/// `session_id` 选择子 TaskSession 显示转录。
///
/// # Returns
/// 返回值
/// The prompt body when a provisional subagent prompt row exists.
/// 当临时子代理提示词行存在时返回其正文。
pub(crate) fn task_display_prompt_anchor_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<String> {
    workspace_snapshot
        .session_messages
        .get(&session_id)?
        .iter()
        .find(|message| session_message_is_seeded_subagent_task_prompt(message))
        .map(|message| message.body.clone())
}

/// Projects one child TaskSession conversation state into display rows scoped to the delegated task.
/// 将单个子 TaskSession 会话状态投影为限定在委派任务范围内的显示行。
///
/// # Parameters
/// 参数
/// `conversation_state` is the child runtime state that may include inherited parent context records.
/// `conversation_state` 是可能包含继承父上下文记录的子运行态。
/// `task_prompt_anchor` is the child task prompt that starts the user-visible child transcript.
/// `task_prompt_anchor` 是开启用户可见子转录的子任务提示词。
///
/// # Returns
/// 返回值
/// Display messages beginning at the child task prompt when the anchor is available.
/// 当锚点可用时返回从子任务提示词开始的显示消息。
pub(crate) fn display_messages_from_task_conversation_state(
    conversation_state: &SessionConversationState,
    task_prompt_anchor: Option<&str>,
) -> Vec<SessionMessageEntry> {
    let messages = conversation_state_to_session_messages(conversation_state);
    let scoped_messages =
        task_scoped_display_messages_from_prompt_anchor(messages, task_prompt_anchor);
    filter_display_session_messages(scoped_messages)
}

/// Drops inherited parent display rows before the child task prompt.
/// 丢弃子任务提示词之前继承自父级的显示行。
///
/// # Parameters
/// 参数
/// `messages` are display rows projected from the complete child runtime state.
/// `messages` 是从完整子运行态投影出的显示行。
/// `task_prompt_anchor` is the normalized child prompt boundary when known.
/// `task_prompt_anchor` 是已知情况下的归一化子提示词边界。
///
/// # Returns
/// 返回值
/// The child-owned display window, falling back to the latest visible child user prompt.
/// 返回子任务自有显示窗口；无法匹配锚点时回退到最新可见子用户提示。
fn task_scoped_display_messages_from_prompt_anchor(
    messages: Vec<SessionMessageEntry>,
    task_prompt_anchor: Option<&str>,
) -> Vec<SessionMessageEntry> {
    let start_index = normalized_task_prompt_anchor(task_prompt_anchor)
        .and_then(|anchor| task_prompt_anchor_start_index(messages.as_slice(), anchor.as_str()))
        .or_else(|| latest_visible_user_transcript_start_index(messages.as_slice()));
    let Some(start_index) = start_index else {
        return messages;
    };
    messages.into_iter().skip(start_index).collect()
}

/// Returns the display row index that exactly matches the child task prompt anchor.
/// 返回与子任务提示词锚点精确匹配的显示行索引。
///
/// # Parameters
/// 参数
/// `messages` are candidate display rows projected from the child runtime state.
/// `messages` 是从子运行态投影出的候选显示行。
/// `anchor` is the normalized prompt boundary to find.
/// `anchor` 是需要查找的归一化提示词边界。
///
/// # Returns
/// 返回值
/// The latest matching visible user transcript row index.
/// 返回最新匹配的可见用户转录行索引。
fn task_prompt_anchor_start_index(messages: &[SessionMessageEntry], anchor: &str) -> Option<usize> {
    messages.iter().rposition(|message| {
        message.kind == SessionMessageKind::User
            && message.record_kind == SessionMessageRecordKind::Transcript
            && normalized_task_prompt_anchor(Some(message.body.as_str())).as_deref() == Some(anchor)
    })
}

/// Returns the latest visible user transcript index for anchorless child display scoping.
/// 返回无锚点子显示裁剪可使用的最新可见用户转录索引。
///
/// # Parameters
/// 参数
/// `messages` are candidate display rows projected from the child runtime state.
/// `messages` 是从子运行态投影出的候选显示行。
///
/// # Returns
/// 返回值
/// The latest visible user transcript row index when one exists.
/// 存在时返回最新可见用户转录行索引。
fn latest_visible_user_transcript_start_index(messages: &[SessionMessageEntry]) -> Option<usize> {
    messages.iter().rposition(|message| {
        message.kind == SessionMessageKind::User
            && message.record_kind == SessionMessageRecordKind::Transcript
    })
}

/// Normalizes a task prompt boundary for exact display-scope matching.
/// 归一化任务提示词边界用于精确显示范围匹配。
///
/// # Parameters
/// 参数
/// `value` is the candidate prompt text.
/// `value` 是候选提示词文本。
///
/// # Returns
/// 返回值
/// A trimmed non-empty prompt anchor.
/// 返回裁剪后的非空提示词锚点。
fn normalized_task_prompt_anchor(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Returns whether one display row is the provisional child-task prompt seeded before runtime completion.
/// 判断单条显示行是否为运行完成前播种的临时子任务提示词。
///
/// # Parameters
/// 参数
/// `message` is the display row to inspect.
/// `message` 是需要检查的显示行。
///
/// # Returns
/// 返回值
/// `true` when the row was created by the subagent prompt seeding path.
/// 当该行由子代理提示词播种路径创建时返回 `true`。
fn session_message_is_seeded_subagent_task_prompt(message: &SessionMessageEntry) -> bool {
    message.id.contains("-subagent-prompt-")
        || message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("runtime_id"))
            .and_then(Value::as_str)
            .is_some_and(|runtime_id| runtime_id.contains("-subagent-prompt-"))
}

/// Writes display transcript rows while omitting empty retained entries.
/// 写入显示层转录行，同时省略空的保留条目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose display layer should be updated.
/// `workspace_snapshot` 是需要更新显示层的工作区快照。
/// `session_id` selects the display transcript owner.
/// `session_id` 选择显示层转录所属会话。
/// `messages` contains the projected display rows.
/// `messages` 包含已投影的显示行。
fn set_display_session_messages(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    messages: Vec<SessionMessageEntry>,
) {
    if messages.is_empty() {
        workspace_snapshot.session_messages.remove(&session_id);
    } else {
        workspace_snapshot
            .session_messages
            .insert(session_id, messages);
    }
}

/// Merges retained parent-to-child send_message notes into a rebuilt child display window.
/// 将已保留的父到子 send_message 显示记录合并回重建后的子显示窗口。
fn merge_task_display_messages_with_existing_send_message_notes(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    mut messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    let Some(existing_messages) = workspace_snapshot.session_messages.get(&session_id) else {
        return messages;
    };
    for existing_message in existing_messages
        .iter()
        .filter(|message| session_message_is_subagent_send_message_display_note(message))
    {
        insert_display_session_message_by_created_at(&mut messages, existing_message.clone());
    }
    filter_display_session_messages(messages)
}

/// Inserts one display row near its original creation timestamp while preserving de-duplication.
/// 按原始创建时间插入一条显示记录，同时保持去重语义。
fn insert_display_session_message_by_created_at(
    messages: &mut Vec<SessionMessageEntry>,
    message: SessionMessageEntry,
) {
    if messages.iter().any(|existing| {
        existing.id == message.id
            || (session_message_is_subagent_send_message_display_note(existing)
                && session_message_is_subagent_send_message_display_note(&message)
                && existing.body.trim() == message.body.trim())
    }) {
        return;
    }
    let insert_at = messages
        .iter()
        .position(|existing| session_message_created_after(existing, &message));
    match insert_at {
        Some(index) => messages.insert(index, message),
        None => messages.push(message),
    }
}

/// Returns whether the first message was created after the second message.
/// 返回第一条消息是否晚于第二条消息创建。
fn session_message_created_after(
    existing: &SessionMessageEntry,
    candidate: &SessionMessageEntry,
) -> bool {
    match (
        session_message_created_at_millis(existing),
        session_message_created_at_millis(candidate),
    ) {
        (Some(existing_created_at), Some(candidate_created_at)) => {
            existing_created_at > candidate_created_at
        }
        _ => false,
    }
}

/// Parses one message creation timestamp as Unix milliseconds.
/// 将单条消息创建时间解析为 Unix 毫秒。
fn session_message_created_at_millis(message: &SessionMessageEntry) -> Option<i128> {
    message.created_at.trim().parse::<i128>().ok()
}

/// Keeps only transcript entries intended for normal user-facing display.
/// 仅保留用于常规用户可见显示的转录条目。
pub(crate) fn filter_display_session_messages(
    messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    let mut display_messages = Vec::new();
    for message in messages {
        for projected_message in project_display_session_message(message) {
            push_display_session_message_once(&mut display_messages, projected_message);
        }
    }
    display_messages
}

/// Returns whether one session transcript entry belongs in user-facing display projections.
/// 返回单条会话转录项是否应进入面向用户的显示投影。
pub(crate) fn session_message_is_display_visible(message: &SessionMessageEntry) -> bool {
    message.is_visible_to_user()
        && !session_message_is_legacy_goal_internal_user_prompt(message)
        && !session_message_is_internal_agent_message_injection(message)
}

/// Projects one transcript row into zero or more user-facing display rows.
/// 将单条转录行投影为零条或多条面向用户的显示行。
fn project_display_session_message(message: SessionMessageEntry) -> Vec<SessionMessageEntry> {
    session_message_is_display_visible(&message)
        .then_some(message)
        .into_iter()
        .collect()
}

/// Appends one display row while suppressing duplicate display-only agent messages.
/// 追加一条显示行，同时抑制重复的仅显示智能体消息。
fn push_display_session_message_once(
    display_messages: &mut Vec<SessionMessageEntry>,
    message: SessionMessageEntry,
) {
    if display_messages.iter().any(|existing| {
        existing.id == message.id
            || (session_message_is_subagent_send_message_display_note(existing)
                && session_message_is_subagent_send_message_display_note(&message)
                && existing.body.trim() == message.body.trim())
    }) {
        return;
    }
    display_messages.push(message);
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

/// Returns whether one message is an internal inter-agent injection row.
/// 返回单条消息是否为内部代理间注入行。
pub(crate) fn session_message_is_internal_agent_message_injection(
    message: &SessionMessageEntry,
) -> bool {
    let body = message.body.trim_start();
    body.starts_with("<agent_message>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Verifies empty display transcript updates remove retained session entries.
    /// 验证空显示层转录更新会移除保留会话条目。
    #[test]
    fn set_display_session_messages_removes_empty_entries() {
        let session_id = SessionId::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(session_id, Vec::new())]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        set_display_session_messages(&mut workspace_snapshot, session_id, Vec::new());

        assert!(
            !workspace_snapshot
                .session_messages
                .contains_key(&session_id)
        );
    }
}

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

/// Replaces the display transcript with the canonical records owned by one runtime dialogue state.
/// 使用单个运行态对话状态拥有的规范记录替换显示层转录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose display layer will be rewritten.
/// `workspace_snapshot` 是显示层将被重写的工作区快照。
/// `session_id` selects the session that owns both the runtime records and display rows.
/// `session_id` 选择同时拥有运行态记录与显示行的会话。
/// `runtime_state` is the runtime state providing the authoritative display records.
/// `runtime_state` 是提供权威显示记录的运行态。
pub(crate) fn replace_display_messages_with_runtime_state_records(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) {
    let mut messages = display_messages_from_conversation_state(&runtime_state.conversation_state);
    if let Some(existing_messages) = workspace_snapshot.session_messages.get(&session_id) {
        for existing_message in existing_messages {
            if session_message_is_display_only_agent_message(existing_message) {
                push_display_session_message_once(&mut messages, existing_message.clone());
            }
        }
    }
    set_display_session_messages(workspace_snapshot, session_id, messages);
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

const INTER_AGENT_MESSAGES_HEADER: &str = "[Inter-agent messages]";

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
    if let Some(messages) = project_internal_inter_agent_message_injection(&message) {
        return messages;
    }
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
            || (session_message_is_display_only_agent_message(existing)
                && session_message_is_display_only_agent_message(&message)
                && existing.body.trim() == message.body.trim())
    }) {
        return;
    }
    display_messages.push(message);
}

/// Returns whether one display row is the UI-only form of a parent-to-child agent message.
/// 返回单条显示行是否为父级发给子智能体消息的仅 UI 形态。
fn session_message_is_display_only_agent_message(message: &SessionMessageEntry) -> bool {
    message.record_kind == SessionMessageRecordKind::DisplayNote
        && message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("kind"))
            .and_then(Value::as_str)
            == Some("subagent_message")
        && message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool"))
            .and_then(Value::as_str)
            == Some("send_message")
}

/// Projects one internal parent-to-child injection prompt into friendly display rows.
/// 将一条内部父到子注入提示词投影成友好的显示行。
fn project_internal_inter_agent_message_injection(
    message: &SessionMessageEntry,
) -> Option<Vec<SessionMessageEntry>> {
    let payloads = parse_inter_agent_message_payloads(message.body.trim_start())?;
    let source_id = if message.id.trim().is_empty() {
        format!("inter-agent-message-{}", message.created_at)
    } else {
        message.id.clone()
    };
    let created_at = message.created_at.clone();
    let messages = payloads
        .into_iter()
        .enumerate()
        .filter_map(|(index, payload)| {
            let payload = payload.trim();
            if payload.is_empty() {
                return None;
            }
            let mut entry = SessionMessageEntry::new(
                SessionMessageKind::User,
                "智能体消息",
                payload.to_string(),
            )
            .with_runtime_metadata(
                format!("{source_id}-display-message-{}", index + 1),
                created_at.clone(),
            );
            entry.record_kind = SessionMessageRecordKind::DisplayNote;
            entry.metadata = Some(json!({
                "display_note": true,
                "kind": "subagent_message",
                "tool": "send_message",
                "source": "inter_agent_messages",
                "message_index": index + 1,
            }));
            Some(entry)
        })
        .collect::<Vec<_>>();
    Some(messages)
}

/// Parses the child-agent inbox prompt generated by the runtime into message payloads.
/// 将运行时生成的子智能体 inbox 提示词解析为消息正文集合。
fn parse_inter_agent_message_payloads(body: &str) -> Option<Vec<String>> {
    let body = body.trim_start();
    if !body.starts_with(INTER_AGENT_MESSAGES_HEADER) {
        return None;
    }
    let normalized = body.replace("\r\n", "\n");
    let mut payloads = Vec::new();
    let mut current_payload: Option<Vec<String>> = None;
    for line in normalized.lines().skip(1) {
        if inter_agent_message_header_line(line) {
            if let Some(lines) = current_payload.take() {
                payloads.push(lines.join("\n"));
            }
            current_payload = Some(Vec::new());
            continue;
        }
        if let Some(lines) = current_payload.as_mut() {
            lines.push(line.to_string());
        }
    }
    if let Some(lines) = current_payload {
        payloads.push(lines.join("\n"));
    }
    Some(payloads)
}

/// Returns whether one line is the generated delimiter before an inter-agent message payload.
/// 返回单行文本是否为代理间消息正文前的生成分隔行。
fn inter_agent_message_header_line(line: &str) -> bool {
    line.starts_with("Message ")
        && line.contains(" from ")
        && line.contains(" to ")
        && line.contains(" via send_message at ")
        && line.ends_with(':')
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
    body.starts_with("<agent_message>") || body.starts_with(INTER_AGENT_MESSAGES_HEADER)
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

//! Session hydration context marker helpers.
//! 会话水合 context marker 辅助逻辑。

use super::*;
/// Returns whether a retained WebUI snapshot has enough dialogue context for prompt submission.
/// 判断保留的 WebUI 快照是否已有足够的对话层上下文可直接提交提示词。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` selects the submitted conversation.
/// `session_id` 选择本次提交目标会话。
///
/// # Returns
/// 返回值
/// `true` when the session runtime state already owns concrete conversation context.
/// 当会话运行态已拥有具体会话上下文时返回 `true`。
pub(crate) fn retained_submission_snapshot_has_session_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
}

/// Returns whether a runtime state carries provider-visible canonical dialogue messages.
/// 判断运行态是否携带 Provider 可见的规范对话消息。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state to inspect.
/// `runtime_state` 是待检查的会话运行态。
///
/// # Returns
/// 返回值
/// `true` when canonical messages are already loaded for future API requests.
/// 当后续 API 请求所需的规范消息已经加载时返回 `true`。
pub(crate) fn runtime_state_has_concrete_conversation_context(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.conversation_state.messages.is_empty()
}

/// Synchronizes the hydrated marker with concrete dialogue runtime readiness.
/// 根据具体对话层运行态就绪情况同步水合标记。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose marker set should be updated.
/// `workspace_snapshot` 是需要更新标记集合的工作区快照。
/// `session_id` selects the session whose hydrated marker should be synchronized.
/// `session_id` 选择需要同步水合标记的会话。
///
/// # Returns
/// 返回值
/// `true` when the marker remains set because dialogue context exists.
/// 当对话层上下文存在且标记保持置位时返回 `true`。
pub(crate) fn sync_hydrated_session_dialogue_ready_marker(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    if workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
    {
        workspace_snapshot.hydrated_session_ids.insert(session_id);
        return true;
    }
    workspace_snapshot.hydrated_session_ids.remove(&session_id);
    false
}

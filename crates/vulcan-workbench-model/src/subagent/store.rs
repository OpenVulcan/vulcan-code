use super::*;

/// Resolves the persisted parent reference used to list direct children for either a root session or a nested TaskSession.
/// 为根会话或嵌套 TaskSession 解析列出直接子节点时使用的持久父级引用。
///
/// # Parameters
/// 参数
/// `store` is the shared TaskSession store.
/// `store` 是共享 TaskSession 存储。
/// `parent_session_id` is either the root main session id or a TaskSession id inside that root store.
/// `parent_session_id` 可以是根主会话标识，也可以是该根存储下的 TaskSession 标识。
///
/// # Returns
/// 返回值
/// The direct parent reference understood by `TaskSessionStore`, or `None` if metadata lookup fails unexpectedly.
/// 返回 `TaskSessionStore` 能理解的直接父级引用；若元数据读取异常则返回 `None`。
pub fn subagent_parent_ref_from_task_session_store<S>(
    store: &S,
    parent_session_id: SessionId,
) -> Option<ParentSessionRef>
where
    S: TaskSessionStore + ?Sized,
{
    let parent_node_id = SessionNodeId::from(parent_session_id);
    match store.load_task_session(parent_node_id) {
        Ok(Some(metadata)) => Some(ParentSessionRef::TaskSession {
            root_session_id: metadata.root_session_id,
            parent_task_session_id: parent_node_id,
        }),
        Ok(None) => Some(ParentSessionRef::MainSession {
            root_session_id: parent_node_id,
        }),
        Err(_) => None,
    }
}

/// Loads one child panel detail from a segmented TaskSession store using only the selected task id.
/// 仅使用选中的子任务标识从分段 TaskSession 存储加载单个子代理详情。
///
/// # Parameters
/// 参数
/// `store` is the segmented TaskSession store rooted at the main session directory.
/// `store` 是以主会话目录为根的分段 TaskSession 存储。
/// `task_id` is the internal TaskSession identifier to open.
/// `task_id` 是需要打开的内部 TaskSession 标识。
///
/// # Returns
/// 返回值
/// A detail-ready panel state with persisted context when available.
/// 返回包含可用持久上下文的详情面板状态。
pub fn subagent_task_panel_state_detail_from_segmented_task_session_store(
    store: &SegmentedTaskSessionStore,
    task_id: SessionNodeId,
) -> Option<SubagentTaskPanelState> {
    let metadata = store.load_task_session(task_id).ok().flatten()?;
    let context = store.load_context(task_id).ok().flatten();
    let latest_payload = store.load_latest_event_payload(task_id).ok().flatten();
    subagent_task_panel_state_from_task_session(
        &metadata,
        context.as_ref(),
        latest_payload.as_ref(),
    )
}

/// Resolves the direct parent identifier from a TaskSession parent reference.
/// 从 TaskSession 父级引用解析直接父级标识。
pub(super) fn parent_session_id_from_task_session_parent(
    parent: &ParentSessionRef,
) -> Option<SessionId> {
    match parent {
        ParentSessionRef::None => None,
        ParentSessionRef::MainSession { root_session_id } => Some(root_session_id.as_uuid()),
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => Some(parent_task_session_id.as_uuid()),
    }
}

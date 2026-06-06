use crate::model::{TaskSessionId, TaskSessionSummary};
use vulcan_session_core::{
    ParentSessionRef, RootSessionId, SessionExecutionOutcome, SessionExecutionRequest,
    SessionNodeRouter, SessionNodeViewSnapshot, SessionRouteError,
};

/// Paged task-session history returned by the task router.
/// 子代理路由返回的分页历史。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSessionHistoryPage {
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// View snapshot for this history page.
    /// 该历史页的视图快照。
    pub view: SessionNodeViewSnapshot,
    /// Optional cursor for the next page.
    /// 下一页的可选游标。
    pub next_cursor: Option<String>,
}

/// Shared task-session router used by iced and WebUI bridges.
/// iced 与 WebUI 桥接共同使用的子代理会话路由。
pub trait TaskSessionRouter: SessionNodeRouter {
    /// Opens a task-session view by identifier.
    /// 按标识符打开子代理会话视图。
    fn open_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<SessionNodeViewSnapshot, SessionRouteError>;

    /// Lists active task sessions under one root main session.
    /// 列出某个根主会话下的活动子代理会话。
    fn list_active_task_sessions(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<TaskSessionSummary>, SessionRouteError>;

    /// Lists direct child task sessions for a parent node.
    /// 列出某个父节点的直接子代理会话。
    fn list_task_children(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskSessionSummary>, SessionRouteError>;

    /// Stops one task session.
    /// 停止一个子代理会话。
    fn stop_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;

    /// Stops all active task sessions under one root main session.
    /// 停止某个根主会话下的全部活动子代理会话。
    fn stop_all_tasks(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<SessionExecutionOutcome>, SessionRouteError>;

    /// Resumes one task session through the shared execution adapter.
    /// 通过共享执行适配器恢复一个子代理会话。
    fn resume_task_session(
        &self,
        request: SessionExecutionRequest,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;

    /// Loads one task-session history page.
    /// 加载一个子代理会话历史页。
    fn load_task_session_history_page(
        &self,
        task_session_id: TaskSessionId,
        cursor: Option<String>,
    ) -> Result<TaskSessionHistoryPage, SessionRouteError>;
}

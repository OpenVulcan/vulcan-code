use crate::model::{TaskInvocationEvent, TaskSessionId, TaskSessionMetadata};
use vulcan_session_core::{
    ParentSessionRef, SessionCompactionCheckpoint, SessionNodeStore, SessionStoreError,
};

/// Segmented storage contract for task sessions.
/// 分段子代理会话存储契约。
pub trait TaskSessionStore: SessionNodeStore {
    /// Creates a task-session directory and writes its metadata.
    /// 创建子代理会话目录并写入元数据。
    fn create_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError>;

    /// Saves task-session metadata.
    /// 保存子代理会话元数据。
    fn save_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError>;

    /// Loads task-session metadata by identifier.
    /// 按标识符加载子代理会话元数据。
    fn load_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError>;

    /// Finds one task session by direct parent and caller-facing task id.
    /// 按直接父级与调用方面向任务标识查找一个子代理会话。
    fn find_task_session_by_parent_and_public_id(
        &self,
        parent: ParentSessionRef,
        public_task_id: &str,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError>;

    /// Appends a task invocation event to the direct parent task log.
    /// 向直接父级任务日志追加一条子代理调用事件。
    fn append_task_invocation_event(
        &self,
        parent: ParentSessionRef,
        event: TaskInvocationEvent,
    ) -> Result<(), SessionStoreError>;

    /// Lists task invocation events for one direct parent.
    /// 列出某个直接父级的子代理调用事件。
    fn list_task_invocation_events(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskInvocationEvent>, SessionStoreError>;

    /// Saves one compaction checkpoint in the task-session file domain.
    /// 在子代理会话文件域中保存一个压缩检查点。
    fn save_task_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError>;

    /// Loads compaction checkpoints for one task session.
    /// 加载某个子代理会话的压缩检查点。
    fn load_task_compaction_checkpoints(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Vec<SessionCompactionCheckpoint>, SessionStoreError>;
}

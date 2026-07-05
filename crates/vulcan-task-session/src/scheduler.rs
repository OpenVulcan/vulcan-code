use crate::config_resolver::TaskSessionResolvedConfig;
use crate::model::{
    TaskSessionId, TaskSessionMetadata, TaskSessionMetadataInit, TaskSessionSummary,
};
use vulcan_session_core::{
    ParentSessionRef, RootSessionId, SessionExecutionOutcome, SessionExecutionRequest,
    SessionRouteError,
};

/// Request used to create one task session.
/// 用于创建一个子代理会话的请求。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSessionCreateRequest {
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Direct parent session reference.
    /// 直接父级会话引用。
    pub parent: ParentSessionRef,
    /// Public task identifier.
    /// 公开任务标识符。
    pub public_task_id: String,
    /// Display title for the task session.
    /// 子代理会话显示标题。
    pub display_title: String,
    /// Optional human-readable description.
    /// 可选的面向人描述。
    pub description: Option<String>,
    /// Prompt that the task session should execute.
    /// 子代理会话需要执行的提示词。
    pub prompt: String,
    /// Resolved task-session configuration.
    /// 已解析的子代理会话配置。
    pub resolved_config: TaskSessionResolvedConfig,
    /// Nesting depth under the root main session.
    /// 根主会话下的嵌套深度。
    pub depth: u16,
    /// Creation timestamp in milliseconds.
    /// 创建时间毫秒值。
    pub timestamp_ms: u64,
}

impl TaskSessionCreateRequest {
    /// Builds metadata from this creation request and a generated task-session identifier.
    /// 使用该创建请求和生成的子代理会话标识符构建元数据。
    pub fn into_metadata(self, task_session_id: TaskSessionId) -> TaskSessionMetadata {
        let mut metadata = TaskSessionMetadata::new(TaskSessionMetadataInit {
            root_session_id: self.root_session_id,
            task_session_id,
            parent: self.parent,
            public_task_id: self.public_task_id,
            display_title: self.display_title,
            requested_config: self.resolved_config.requested_config,
            effective_config: self.resolved_config.effective_config,
            depth: self.depth,
            timestamp_ms: self.timestamp_ms,
        });
        metadata.runtime_policy = self.resolved_config.runtime_policy;
        metadata.description = self.description;
        metadata
    }
}

/// Scheduler contract for creating, stopping, and resuming task sessions.
/// 创建、停止和恢复子代理会话的调度器契约。
pub trait TaskSessionScheduler: Send + Sync {
    /// Creates one task session and returns its metadata.
    /// 创建一个子代理会话并返回其元数据。
    fn create_task_session(
        &self,
        request: TaskSessionCreateRequest,
    ) -> Result<TaskSessionMetadata, SessionRouteError>;

    /// Resumes one task session with a prepared execution request.
    /// 使用已准备执行请求恢复一个子代理会话。
    fn resume_task_session(
        &self,
        request: SessionExecutionRequest,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;

    /// Stops one task session.
    /// 停止一个子代理会话。
    fn stop_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;

    /// Stops all active task sessions under a root main session.
    /// 停止根主会话下的全部活动子代理会话。
    fn stop_all_tasks(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<SessionExecutionOutcome>, SessionRouteError>;

    /// Lists active task sessions under a root main session.
    /// 列出根主会话下的活动子代理会话。
    fn list_active_task_sessions(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<TaskSessionSummary>, SessionRouteError>;
}

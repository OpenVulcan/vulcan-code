use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::{
    ExecutionRuntimePolicy, ParentSessionRef, RootSessionId, SessionEffectiveConfig, SessionNodeId,
    SessionRequestedConfig,
};

/// Stable task-session identifier.
/// 稳定子代理会话标识符。
pub type TaskSessionId = SessionNodeId;

/// Runtime status for a segmented task session.
/// 分段子代理会话的运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSessionStatus {
    /// Task session has been created but not started.
    /// 子代理会话已创建但尚未启动。
    Created,
    /// Task session is currently running.
    /// 子代理会话正在运行。
    Running,
    /// Task session is waiting for nested child tasks.
    /// 子代理会话正在等待嵌套子任务。
    WaitingChildren,
    /// Task session is waiting for a follow-up prompt.
    /// 子代理会话正在等待后续提示词。
    WaitingFollowup,
    /// Task session completed successfully.
    /// 子代理会话已成功完成。
    Completed,
    /// Task session failed.
    /// 子代理会话已失败。
    Failed,
    /// Task session was stopped.
    /// 子代理会话已停止。
    Stopped,
    /// Task session is compacting its context.
    /// 子代理会话正在压缩上下文。
    Compacting,
    /// Task session has been archived with its parent.
    /// 子代理会话已随父级归档。
    Archived,
}

impl TaskSessionStatus {
    /// Returns whether this status still represents active work.
    /// 返回该状态是否仍代表活动工作。
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Created
                | Self::Running
                | Self::WaitingChildren
                | Self::WaitingFollowup
                | Self::Compacting
        )
    }
}

/// Compact task-session summary used by routers and parent indexes.
/// 路由器和父级索引使用的紧凑子代理会话摘要。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSessionSummary {
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// Public task identifier shown to users and parent nodes.
    /// 显示给用户和父节点的公开任务标识符。
    pub public_task_id: String,
    /// Display title for the task session.
    /// 子代理会话显示标题。
    pub display_title: String,
    /// Current task-session status.
    /// 当前子代理会话状态。
    pub status: TaskSessionStatus,
    /// Direct parent reference.
    /// 直接父级引用。
    pub parent: ParentSessionRef,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    pub updated_at: u64,
    /// Optional last result summary.
    /// 可选的最近结果摘要。
    pub last_result_summary: Option<String>,
}

/// Persistent metadata stored in the segmented task session metadata.json file.
/// 保存在分段子代理会话 metadata.json 文件中的持久元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSessionMetadata {
    /// Schema version for task-session metadata.
    /// 子代理会话元数据结构版本。
    pub schema_version: u32,
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// Public task identifier chosen by the caller or profile.
    /// 调用方或配置选择的公开任务标识符。
    pub public_task_id: String,
    /// Direct parent session reference.
    /// 直接父级会话引用。
    pub parent: ParentSessionRef,
    /// Nesting depth under the root main session.
    /// 根主会话下的嵌套深度。
    pub depth: u16,
    /// Display title for the task session.
    /// 子代理会话显示标题。
    pub display_title: String,
    /// Optional human-readable task description.
    /// 可选的面向人的任务描述。
    pub description: Option<String>,
    /// Resolved subagent profile identifier.
    /// 已解析的子代理配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Whether parent configuration participated in resolution.
    /// 父级配置是否参与解析。
    pub inherit_parent_config: bool,
    /// Current task-session status.
    /// 当前子代理会话状态。
    pub status: TaskSessionStatus,
    /// Optional reason for the most recent stopped or archived state.
    /// 最近一次停止或归档状态的可选原因。
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Runtime policy frozen for this task session.
    /// 为该子代理会话固化的运行时策略。
    pub runtime_policy: ExecutionRuntimePolicy,
    /// Requested configuration stored for audit.
    /// 为审计保存的请求配置。
    pub requested_config: SessionRequestedConfig,
    /// Effective configuration stored for replay and recovery.
    /// 为重放和恢复保存的有效配置。
    pub effective_config: SessionEffectiveConfig,
    /// Creation timestamp in milliseconds.
    /// 创建时间毫秒值。
    pub created_at: u64,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    pub updated_at: u64,
    /// Optional last result summary.
    /// 可选的最近结果摘要。
    pub last_result_summary: Option<String>,
    /// Optional last error message.
    /// 可选的最近错误信息。
    pub last_error: Option<String>,
}

impl TaskSessionMetadata {
    /// Builds metadata for a newly created task session.
    /// 构建新建子代理会话的元数据。
    pub fn new(
        root_session_id: RootSessionId,
        task_session_id: TaskSessionId,
        parent: ParentSessionRef,
        public_task_id: impl Into<String>,
        display_title: impl Into<String>,
        requested_config: SessionRequestedConfig,
        effective_config: SessionEffectiveConfig,
        depth: u16,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            schema_version: 1,
            root_session_id,
            task_session_id,
            public_task_id: public_task_id.into(),
            parent,
            depth,
            display_title: display_title.into(),
            description: None,
            subagent_profile_id: effective_config.subagent_profile_id.clone(),
            inherit_parent_config: effective_config.inherit_parent_config,
            status: TaskSessionStatus::Created,
            stop_reason: None,
            runtime_policy: ExecutionRuntimePolicy::task_session(),
            requested_config,
            effective_config,
            created_at: timestamp_ms,
            updated_at: timestamp_ms,
            last_result_summary: None,
            last_error: None,
        }
    }

    /// Creates a compact summary from this metadata.
    /// 从该元数据创建紧凑摘要。
    pub fn to_summary(&self) -> TaskSessionSummary {
        TaskSessionSummary {
            root_session_id: self.root_session_id,
            task_session_id: self.task_session_id,
            public_task_id: self.public_task_id.clone(),
            display_title: self.display_title.clone(),
            status: self.status,
            parent: self.parent.clone(),
            updated_at: self.updated_at,
            last_result_summary: self.last_result_summary.clone(),
        }
    }
}

/// Parent-side task invocation event kind.
/// 父级侧子代理调用事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInvocationEventKind {
    /// Task session was created.
    /// 子代理会话已创建。
    TaskCreated,
    /// Task session was started.
    /// 子代理会话已启动。
    TaskStarted,
    /// Task session checkpoint was saved.
    /// 子代理会话检查点已保存。
    TaskCheckpoint,
    /// Task session completed.
    /// 子代理会话已完成。
    TaskCompleted,
    /// Task session failed.
    /// 子代理会话已失败。
    TaskFailed,
    /// Task session was stopped.
    /// 子代理会话已停止。
    TaskStopped,
    /// Task feedback was appended.
    /// 子代理反馈已追加。
    TaskFeedbackAppended,
    /// Task context was compacted.
    /// 子代理上下文已压缩。
    TaskCompacted,
    /// Task session was resumed.
    /// 子代理会话已恢复。
    TaskResumed,
}

/// Parent-side append-only invocation event.
/// 父级侧追加式调用事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskInvocationEvent {
    /// Stable event identifier.
    /// 稳定事件标识符。
    pub event_id: String,
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Direct parent session reference.
    /// 直接父级会话引用。
    pub parent: ParentSessionRef,
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// Monotonic sequence inside the parent task log.
    /// 父级任务日志内的单调序号。
    pub sequence: u64,
    /// Invocation event kind.
    /// 调用事件类型。
    pub kind: TaskInvocationEventKind,
    /// Task-session status carried by the event.
    /// 该事件携带的子代理会话状态。
    pub status: TaskSessionStatus,
    /// Optional task prompt recorded for indexing and audit.
    /// 可选的任务提示词记录，用于索引和审计。
    pub prompt: Option<String>,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Event payload owned by the task-session layer.
    /// 由子代理会话层拥有的事件载荷。
    pub payload: Value,
}

/// Feedback returned from a task session to its direct parent.
/// 子代理会话返回给直接父级的反馈。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskFeedback {
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// Task-session terminal or checkpoint status.
    /// 子代理会话终态或检查点状态。
    pub status: TaskSessionStatus,
    /// Optional concise summary.
    /// 可选的简要摘要。
    pub summary: Option<String>,
    /// Optional result text visible to the parent agent.
    /// 可选的父代理可见结果文本。
    pub result_text: Option<String>,
    /// Optional result message identifier inside the task session.
    /// 可选的子代理会话内部结果消息标识符。
    pub result_message_id: Option<String>,
    /// Provider turns executed by the task session.
    /// 子代理会话已执行的供应商轮次数。
    pub provider_turns_executed: u64,
    /// Tool calls processed by the task session.
    /// 子代理会话已处理的工具调用数。
    pub tool_calls_processed: u64,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    pub updated_at: u64,
}

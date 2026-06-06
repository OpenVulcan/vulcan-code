use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::{RootSessionId, SessionNodeId};

/// Main-session view of one child task entry.
/// 主会话视角下的一个子代理入口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MainSessionTaskEntry {
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Child task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: SessionNodeId,
    /// Public task identifier chosen by the caller or runtime.
    /// 调用方或运行时选择的公开任务标识符。
    pub public_task_id: String,
    /// Task entry status.
    /// 子代理入口状态。
    pub status: MainSessionTaskEntryStatus,
    /// Optional result summary stored in the parent task index.
    /// 保存在父级任务索引中的可选结果摘要。
    pub result_summary: Option<String>,
    /// Optional task feedback payload.
    /// 可选的子代理反馈载荷。
    pub feedback: Option<Value>,
}

impl MainSessionTaskEntry {
    /// Creates a new parent-side task entry.
    /// 创建新的父级侧子代理入口。
    pub fn new(
        root_session_id: RootSessionId,
        task_session_id: SessionNodeId,
        public_task_id: impl Into<String>,
        status: MainSessionTaskEntryStatus,
    ) -> Self {
        Self {
            root_session_id,
            task_session_id,
            public_task_id: public_task_id.into(),
            status,
            result_summary: None,
            feedback: None,
        }
    }
}

/// Parent-side status for a child task entry.
/// 父级侧子代理入口状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MainSessionTaskEntryStatus {
    /// Task entry was created.
    /// 子代理入口已创建。
    Created,
    /// Task session is running.
    /// 子代理会话正在运行。
    Running,
    /// Task session is waiting for nested children.
    /// 子代理会话正在等待嵌套子代理。
    WaitingChildren,
    /// Task session completed.
    /// 子代理会话已完成。
    Completed,
    /// Task session failed.
    /// 子代理会话已失败。
    Failed,
    /// Task session was stopped.
    /// 子代理会话已停止。
    Stopped,
}

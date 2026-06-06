use crate::event::SessionEventRecord;
use crate::executor::{SessionExecutionOutcome, SessionExecutionRequest};
use crate::model::{SessionNodeId, SessionNodeKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt;

/// View snapshot returned by a shared session router.
/// 共享会话路由返回的视图快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionNodeViewSnapshot {
    /// Session node identifier.
    /// 会话节点标识符。
    pub node_id: SessionNodeId,
    /// Session node kind.
    /// 会话节点类型。
    pub kind: SessionNodeKind,
    /// Display title.
    /// 显示标题。
    pub title: String,
    /// Display status owned by the caller.
    /// 调用方拥有的显示状态。
    pub status: String,
    /// Event records projected for the view.
    /// 投影到视图的事件记录。
    pub events: Vec<SessionEventRecord>,
    /// Additional view metadata.
    /// 额外视图元数据。
    pub metadata: Value,
}

/// Error returned by a shared session router.
/// 共享会话路由返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRouteError {
    /// Human-readable error message.
    /// 面向人的错误信息。
    pub message: String,
}

impl SessionRouteError {
    /// Creates a route error from a message.
    /// 使用消息创建路由错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionRouteError {
    /// Formats the error message.
    /// 格式化错误信息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SessionRouteError {}

/// Shared router contract for opening, stopping, and resuming session nodes.
/// 打开、停止和恢复会话节点的共享路由契约。
pub trait SessionNodeRouter: Send + Sync {
    /// Opens one session node view.
    /// 打开一个会话节点视图。
    fn open_node(
        &self,
        node_id: SessionNodeId,
    ) -> Result<SessionNodeViewSnapshot, SessionRouteError>;

    /// Lists child session node views.
    /// 列出子级会话节点视图。
    fn list_children(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Vec<SessionNodeViewSnapshot>, SessionRouteError>;

    /// Stops one session node and returns its terminal outcome.
    /// 停止一个会话节点并返回其终止结果。
    fn stop_node(
        &self,
        node_id: SessionNodeId,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;

    /// Resumes one session node with a prepared request.
    /// 使用已准备请求恢复一个会话节点。
    fn resume_node(
        &self,
        request: SessionExecutionRequest,
    ) -> Result<SessionExecutionOutcome, SessionRouteError>;
}

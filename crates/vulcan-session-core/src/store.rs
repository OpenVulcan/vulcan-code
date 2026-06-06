use crate::context::{SessionContextDocument, SessionRuntimeCheckpoint};
use crate::event::SessionEventRecord;
use crate::model::SessionNodeId;
use std::error::Error;
use std::fmt;

/// Error returned by a session node store.
/// 会话节点存储返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStoreError {
    /// Human-readable error message.
    /// 面向人的错误信息。
    pub message: String,
}

impl SessionStoreError {
    /// Creates a store error from a message.
    /// 使用消息创建存储错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionStoreError {
    /// Formats the error message.
    /// 格式化错误信息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SessionStoreError {}

/// Shared storage contract for main sessions and task sessions.
/// 主会话与子代理会话共享的存储契约。
pub trait SessionNodeStore: Send + Sync {
    /// Appends one event record to a session node.
    /// 向会话节点追加一条事件记录。
    fn append_event(&self, record: SessionEventRecord) -> Result<(), SessionStoreError>;

    /// Loads event records for one session node.
    /// 加载单个会话节点的事件记录。
    fn load_events(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Vec<SessionEventRecord>, SessionStoreError>;

    /// Saves the current API-facing context document.
    /// 保存当前 API 实际上下文文档。
    fn save_context(&self, context: SessionContextDocument) -> Result<(), SessionStoreError>;

    /// Loads the current API-facing context document.
    /// 加载当前 API 实际上下文文档。
    fn load_context(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, SessionStoreError>;

    /// Saves a lightweight runtime checkpoint.
    /// 保存轻量运行时检查点。
    fn save_runtime_checkpoint(
        &self,
        checkpoint: SessionRuntimeCheckpoint,
    ) -> Result<(), SessionStoreError>;

    /// Loads a lightweight runtime checkpoint.
    /// 加载轻量运行时检查点。
    fn load_runtime_checkpoint(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionRuntimeCheckpoint>, SessionStoreError>;
}

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identifier shared by main sessions and task sessions.
/// 主会话与子代理会话共享的稳定标识符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionNodeId(Uuid);

impl SessionNodeId {
    /// Creates a session node identifier from an existing UUID value.
    /// 使用已有 UUID 值创建会话节点标识符。
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    /// Creates a new random session node identifier.
    /// 创建新的随机会话节点标识符。
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID value.
    /// 返回底层 UUID 值。
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for SessionNodeId {
    /// Formats the identifier as its canonical UUID string.
    /// 将标识符格式化为标准 UUID 字符串。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for SessionNodeId {
    /// Converts a UUID into a session node identifier.
    /// 将 UUID 转换为会话节点标识符。
    fn from(value: Uuid) -> Self {
        Self::new(value)
    }
}

impl From<SessionNodeId> for Uuid {
    /// Converts a session node identifier back into a UUID.
    /// 将会话节点标识符转换回 UUID。
    fn from(value: SessionNodeId) -> Self {
        value.as_uuid()
    }
}

/// Root main-session identifier for one task-session tree.
/// 一个子代理树所属的根主会话标识符。
pub type RootSessionId = SessionNodeId;

/// Session node kind used by shared routing and execution APIs.
/// 共享路由与执行 API 使用的会话节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionNodeKind {
    /// Main user-facing conversation session.
    /// 面向用户的主会话。
    MainSession,
    /// File-backed delegated task session.
    /// 文件态保存的委派子代理会话。
    TaskSession,
}

/// Parent reference for a main session or nested task session.
/// 主会话或嵌套子代理会话的父级引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParentSessionRef {
    /// Root main sessions do not have a parent node.
    /// 根主会话没有父节点。
    None,
    /// Child task session owned by a root main session.
    /// 由根主会话直接拥有的子代理会话。
    MainSession {
        /// Root main session identifier.
        /// 根主会话标识符。
        root_session_id: RootSessionId,
    },
    /// Nested task session owned by another task session.
    /// 由另一个子代理会话拥有的嵌套子代理会话。
    TaskSession {
        /// Root main session identifier.
        /// 根主会话标识符。
        root_session_id: RootSessionId,
        /// Direct parent task session identifier.
        /// 直接父级子代理会话标识符。
        parent_task_session_id: SessionNodeId,
    },
}

/// Stable path from the root main session to a selected node.
/// 从根主会话到选中节点的稳定路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTreePath {
    /// Root main session identifier.
    /// 根主会话标识符。
    pub root_session_id: RootSessionId,
    /// Ordered node identifiers from root to target.
    /// 从根节点到目标节点的有序节点标识符。
    pub nodes: Vec<SessionNodeId>,
}

impl SessionTreePath {
    /// Creates a path rooted at one main session.
    /// 创建以某个主会话为根的路径。
    pub fn root(root_session_id: RootSessionId) -> Self {
        Self {
            root_session_id,
            nodes: vec![root_session_id],
        }
    }

    /// Returns the final node identifier in this path.
    /// 返回该路径中的最终节点标识符。
    pub fn target(&self) -> Option<SessionNodeId> {
        self.nodes.last().copied()
    }
}

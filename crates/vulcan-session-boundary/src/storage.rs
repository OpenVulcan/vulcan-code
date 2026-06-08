use crate::contract::{SessionBoundaryEvent, SessionThreadId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One append-only storage write following append-only session lifecycle semantics.
/// 遵循追加式会话生命周期语义的一次追加式存储写入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionBoundaryStorageWrite {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: SessionThreadId,
    /// Event row to append.
    /// 需要追加的事件行。
    pub event: SessionBoundaryEvent,
}

/// Append-only session event store.
/// 追加式会话事件存储。
pub trait SessionBoundaryStore: Send + Sync {
    /// Appends one event row without rewriting older rows.
    /// 追加一条事件行且不重写旧行。
    fn append_event(&self, write: SessionBoundaryStorageWrite) -> Result<(), String>;

    /// Reads events for one thread in stable sequence order.
    /// 按稳定序号读取单个线程的事件。
    fn read_events(&self, thread_id: SessionThreadId) -> Result<Vec<SessionBoundaryEvent>, String>;
}

/// Overwrite-only state store paired with append-only event storage.
/// 与追加式事件存储配套的覆盖式状态存储。
pub trait SessionBoundaryStateStore: Send + Sync {
    /// Replaces the latest thread state document.
    /// 覆盖最新线程状态文档。
    fn replace_state(&self, thread_id: SessionThreadId, state: Value) -> Result<(), String>;

    /// Loads the latest thread state document.
    /// 读取最新线程状态文档。
    fn load_state(&self, thread_id: SessionThreadId) -> Result<Option<Value>, String>;
}

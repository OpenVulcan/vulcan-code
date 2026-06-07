use crate::contract::{CodexSessionEvent, CodexThreadId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One append-only storage write following Codex rollout semantics.
/// 遵循 Codex rollout 语义的一次追加式存储写入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexStorageWrite {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: CodexThreadId,
    /// Event row to append.
    /// 需要追加的事件行。
    pub event: CodexSessionEvent,
}

/// Append-only session event store.
/// 追加式会话事件存储。
pub trait CodexSessionStore: Send + Sync {
    /// Appends one event row without rewriting older rows.
    /// 追加一条事件行且不重写旧行。
    fn append_event(&self, write: CodexStorageWrite) -> Result<(), String>;

    /// Reads events for one thread in stable sequence order.
    /// 按稳定序号读取单个线程的事件。
    fn read_events(&self, thread_id: CodexThreadId) -> Result<Vec<CodexSessionEvent>, String>;
}

/// Overwrite-only state store paired with append-only event storage.
/// 与追加式事件存储配套的覆盖式状态存储。
pub trait CodexSessionStateStore: Send + Sync {
    /// Replaces the latest thread state document.
    /// 覆盖最新线程状态文档。
    fn replace_state(&self, thread_id: CodexThreadId, state: Value) -> Result<(), String>;

    /// Loads the latest thread state document.
    /// 读取最新线程状态文档。
    fn load_state(&self, thread_id: CodexThreadId) -> Result<Option<Value>, String>;
}

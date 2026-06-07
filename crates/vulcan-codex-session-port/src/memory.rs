use crate::contract::{CodexSessionEvent, CodexThreadId};
use crate::storage::{CodexSessionStateStore, CodexSessionStore, CodexStorageWrite};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// In-memory Codex-compatible store used for migration tests and adapter staging.
/// 用于迁移测试和适配器暂存的 Codex 兼容内存存储。
#[derive(Debug, Clone, Default)]
pub struct InMemoryCodexSessionStore {
    /// Shared mutable store state.
    /// 共享可变存储状态。
    inner: Arc<Mutex<InMemoryCodexSessionStoreState>>,
}

/// Mutable state held by the in-memory Codex-compatible store.
/// Codex 兼容内存存储持有的可变状态。
#[derive(Debug, Default)]
struct InMemoryCodexSessionStoreState {
    /// Append-only event rows grouped by thread.
    /// 按线程分组的追加式事件行。
    events: BTreeMap<CodexThreadId, Vec<CodexSessionEvent>>,
    /// Latest overwrite-only thread states.
    /// 最新覆盖式线程状态。
    states: BTreeMap<CodexThreadId, Value>,
}

impl CodexSessionStore for InMemoryCodexSessionStore {
    /// Appends one event row without rewriting older rows.
    /// 追加一条事件行且不重写旧行。
    fn append_event(&self, write: CodexStorageWrite) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory Codex store: {error}"))?;
        guard
            .events
            .entry(write.thread_id)
            .or_default()
            .push(write.event);
        Ok(())
    }

    /// Reads events for one thread in stable sequence order.
    /// 按稳定序号读取单个线程的事件。
    fn read_events(&self, thread_id: CodexThreadId) -> Result<Vec<CodexSessionEvent>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory Codex store: {error}"))?;
        let mut events = guard.events.get(&thread_id).cloned().unwrap_or_default();
        events.sort_by_key(|event| event.sequence);
        Ok(events)
    }
}

impl CodexSessionStateStore for InMemoryCodexSessionStore {
    /// Replaces the latest thread state document.
    /// 覆盖最新线程状态文档。
    fn replace_state(&self, thread_id: CodexThreadId, state: Value) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory Codex store: {error}"))?;
        guard.states.insert(thread_id, state);
        Ok(())
    }

    /// Loads the latest thread state document.
    /// 读取最新线程状态文档。
    fn load_state(&self, thread_id: CodexThreadId) -> Result<Option<Value>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory Codex store: {error}"))?;
        Ok(guard.states.get(&thread_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        CodexEventScope, CodexEventVisibility, CodexSessionEventKind, CodexThreadId,
    };
    use serde_json::json;

    /// Verifies events are append-only while state is overwritten.
    /// 验证事件追加保存而状态覆盖保存。
    #[test]
    fn memory_store_appends_events_and_replaces_state() {
        let store = InMemoryCodexSessionStore::default();
        let thread_id = CodexThreadId::new_v4();
        let first = CodexSessionEvent::new(
            "event-1",
            thread_id,
            2,
            CodexSessionEventKind::ThreadStarted,
            CodexEventScope::Thread,
            CodexEventVisibility::Hidden,
            1000,
        );
        let second = CodexSessionEvent::new(
            "event-2",
            thread_id,
            1,
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            1001,
        );

        store
            .append_event(CodexStorageWrite {
                thread_id,
                event: first,
            })
            .expect("first event should append");
        store
            .append_event(CodexStorageWrite {
                thread_id,
                event: second,
            })
            .expect("second event should append");
        store
            .replace_state(thread_id, json!({ "phase": "running" }))
            .expect("state should write");
        store
            .replace_state(thread_id, json!({ "phase": "ready" }))
            .expect("state should overwrite");

        let events = store.read_events(thread_id).expect("events should read");
        let state = store
            .load_state(thread_id)
            .expect("state should read")
            .expect("state should exist");

        assert_eq!(events[0].event_id, "event-2");
        assert_eq!(events[1].event_id, "event-1");
        assert_eq!(state, json!({ "phase": "ready" }));
    }
}

use crate::contract::{SessionBoundaryEvent, SessionThreadId};
use crate::storage::{
    SessionBoundaryStateStore, SessionBoundaryStorageWrite, SessionBoundaryStore,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// In-memory session-boundary store used for migration tests and adapter staging.
/// 用于迁移测试和适配器暂存的 会话边界内存存储。
#[derive(Debug, Clone, Default)]
pub struct InMemorySessionBoundaryStore {
    /// Shared mutable store state.
    /// 共享可变存储状态。
    inner: Arc<Mutex<InMemorySessionBoundaryStoreState>>,
}

/// Mutable state held by the in-memory session-boundary store.
/// 会话边界内存存储持有的可变状态。
#[derive(Debug, Default)]
struct InMemorySessionBoundaryStoreState {
    /// Append-only event rows grouped by thread.
    /// 按线程分组的追加式事件行。
    events: BTreeMap<SessionThreadId, Vec<SessionBoundaryEvent>>,
    /// Latest overwrite-only thread states.
    /// 最新覆盖式线程状态。
    states: BTreeMap<SessionThreadId, Value>,
}

impl SessionBoundaryStore for InMemorySessionBoundaryStore {
    /// Appends one event row without rewriting older rows.
    /// 追加一条事件行且不重写旧行。
    fn append_event(&self, write: SessionBoundaryStorageWrite) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory session boundary store: {error}"))?;
        guard
            .events
            .entry(write.thread_id)
            .or_default()
            .push(write.event);
        Ok(())
    }

    /// Reads events for one thread in stable sequence order.
    /// 按稳定序号读取单个线程的事件。
    fn read_events(&self, thread_id: SessionThreadId) -> Result<Vec<SessionBoundaryEvent>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory session boundary store: {error}"))?;
        let mut events = guard.events.get(&thread_id).cloned().unwrap_or_default();
        events.sort_by_key(|event| event.sequence);
        Ok(events)
    }
}

impl SessionBoundaryStateStore for InMemorySessionBoundaryStore {
    /// Replaces the latest thread state document.
    /// 覆盖最新线程状态文档。
    fn replace_state(&self, thread_id: SessionThreadId, state: Value) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory session boundary store: {error}"))?;
        guard.states.insert(thread_id, state);
        Ok(())
    }

    /// Loads the latest thread state document.
    /// 读取最新线程状态文档。
    fn load_state(&self, thread_id: SessionThreadId) -> Result<Option<Value>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("failed to lock in-memory session boundary store: {error}"))?;
        Ok(guard.states.get(&thread_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility, SessionThreadId,
    };
    use serde_json::json;

    /// Verifies events are append-only while state is overwritten.
    /// 验证事件追加保存而状态覆盖保存。
    #[test]
    fn memory_store_appends_events_and_replaces_state() {
        let store = InMemorySessionBoundaryStore::default();
        let thread_id = SessionThreadId::new_v4();
        let first = SessionBoundaryEvent::new(
            "event-1",
            thread_id,
            2,
            SessionBoundaryEventKind::ThreadStarted,
            SessionEventScope::Thread,
            SessionEventVisibility::Hidden,
            1000,
        );
        let second = SessionBoundaryEvent::new(
            "event-2",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            1001,
        );

        store
            .append_event(SessionBoundaryStorageWrite {
                thread_id,
                event: first,
            })
            .expect("first event should append");
        store
            .append_event(SessionBoundaryStorageWrite {
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

use crate::adapter::CodexHookAdapter;
use crate::contract::CodexSessionEvent;
use crate::storage::{CodexSessionStore, CodexStorageWrite};

/// Runtime facade that enforces Codex-compatible event append ordering.
/// 强制 Codex 兼容事件追加顺序的运行时门面。
pub struct CodexSessionRuntime<S, H>
where
    S: CodexSessionStore,
    H: CodexHookAdapter,
{
    /// Append-only event store.
    /// 追加式事件存储。
    pub store: S,
    /// Hook adapter called after storage append succeeds.
    /// 存储追加成功后调用的 hook 适配器。
    pub hooks: H,
}

impl<S, H> CodexSessionRuntime<S, H>
where
    S: CodexSessionStore,
    H: CodexHookAdapter,
{
    /// Creates a runtime facade from a store and hook adapter.
    /// 使用存储与 hook 适配器创建运行时门面。
    pub const fn new(store: S, hooks: H) -> Self {
        Self { store, hooks }
    }

    /// Appends one event, then notifies hooks after the event is durable.
    /// 先追加单条事件，事件持久后再通知 hook。
    pub fn append_event(&self, event: CodexSessionEvent) -> Result<(), String> {
        self.store.append_event(CodexStorageWrite {
            thread_id: event.thread_id,
            event: event.clone(),
        })?;
        self.hooks.after_event_appended(&event)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::CodexHookAdapter;
    use crate::contract::{
        CodexEventScope, CodexEventVisibility, CodexSessionEventKind, CodexThreadId,
    };
    use crate::memory::InMemoryCodexSessionStore;
    use crate::storage::CodexSessionStore;
    use std::sync::{Arc, Mutex};

    /// Test hook that records the number of stored events visible when the hook runs.
    /// 测试 hook，记录 hook 运行时可见的已存储事件数量。
    #[derive(Clone)]
    struct RecordingHook {
        /// Store read by the hook.
        /// Hook 读取的存储。
        store: InMemoryCodexSessionStore,
        /// Observed event counts.
        /// 观察到的事件数量。
        observed_counts: Arc<Mutex<Vec<usize>>>,
    }

    impl CodexHookAdapter for RecordingHook {
        /// Records the stored event count after append.
        /// 记录追加后的已存储事件数量。
        fn after_event_appended(&self, event: &CodexSessionEvent) -> Result<(), String> {
            let count = self.store.read_events(event.thread_id)?.len();
            let mut guard = self
                .observed_counts
                .lock()
                .map_err(|error| format!("failed to lock observed counts: {error}"))?;
            guard.push(count);
            Ok(())
        }
    }

    /// Verifies hooks observe events only after append-only storage has been updated.
    /// 验证 hook 只会在追加式存储更新后观察到事件。
    #[test]
    fn runtime_appends_before_notifying_hooks() {
        let store = InMemoryCodexSessionStore::default();
        let observed_counts = Arc::new(Mutex::new(Vec::new()));
        let hook = RecordingHook {
            store: store.clone(),
            observed_counts: Arc::clone(&observed_counts),
        };
        let runtime = CodexSessionRuntime::new(store, hook);
        let thread_id = CodexThreadId::new_v4();
        let event = CodexSessionEvent::new(
            "event-1",
            thread_id,
            1,
            CodexSessionEventKind::ThreadStarted,
            CodexEventScope::Thread,
            CodexEventVisibility::Hidden,
            1000,
        );

        runtime
            .append_event(event)
            .expect("event should append and notify");

        let guard = observed_counts
            .lock()
            .expect("observed counts should be readable");
        assert_eq!(guard.as_slice(), &[1]);
    }
}

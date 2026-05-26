use crate::event::FileWatchEvent;
use crate::traits::{FileWatchTarget, FileWatcher};

/// No-op reusable watcher used before real backend integration.
/// 在真实后端接入前使用的空操作可复用监听器。
#[derive(Debug, Default)]
pub struct NoopFileWatcher {
    /// Registered watch targets.
    /// 已注册监听目标。
    targets: Vec<FileWatchTarget>,
    /// Buffered events manually pushed by tests or adapters.
    /// 由测试或适配器手动推入的缓冲事件。
    events: Vec<FileWatchEvent>,
}

impl NoopFileWatcher {
    /// Push one synthetic event into the watcher buffer.
    /// 将一条合成事件推入监听器缓冲区。
    pub fn push_event(&mut self, event: FileWatchEvent) {
        self.events.push(event);
    }

    /// Return the number of registered targets.
    /// 返回已注册目标数量。
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }
}

impl FileWatcher for NoopFileWatcher {
    /// Register one watch target.
    /// 注册一个监听目标。
    fn watch(&mut self, target: FileWatchTarget) -> Result<(), String> {
        self.targets.push(target);
        Ok(())
    }

    /// Drain all buffered events.
    /// 取出全部缓冲事件。
    fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        std::mem::take(&mut self.events)
    }
}

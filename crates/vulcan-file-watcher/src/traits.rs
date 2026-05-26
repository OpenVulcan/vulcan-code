use std::path::PathBuf;

use crate::event::FileWatchEvent;

/// Watch target descriptor.
/// 监听目标描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWatchTarget {
    /// Target path to watch.
    /// 要监听的目标路径。
    pub path: PathBuf,
    /// Whether recursive watching is desired.
    /// 是否需要递归监听。
    pub recursive: bool,
}

/// Generic file-watcher contract.
/// 通用文件监听器契约。
pub trait FileWatcher {
    /// Register one watch target.
    /// 注册一个监听目标。
    fn watch(&mut self, target: FileWatchTarget) -> Result<(), String>;

    /// Drain the currently pending events.
    /// 取出当前待处理事件。
    fn drain_events(&mut self) -> Vec<FileWatchEvent>;
}

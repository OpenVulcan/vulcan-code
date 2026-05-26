use std::path::PathBuf;

/// File-watcher event kind.
/// 文件监听事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWatchEventKind {
    /// File or directory created.
    /// 文件或目录已创建。
    Created,
    /// File or directory modified.
    /// 文件或目录已修改。
    Modified,
    /// File or directory removed.
    /// 文件或目录已删除。
    Removed,
    /// File or directory renamed.
    /// 文件或目录已重命名。
    Renamed,
}

/// One emitted file-watcher event.
/// 一次发出的文件监听事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWatchEvent {
    /// Event kind.
    /// 事件类型。
    pub kind: FileWatchEventKind,
    /// Primary path associated with the event.
    /// 事件关联的主路径。
    pub path: PathBuf,
    /// Optional previous path for rename events.
    /// 重命名事件的可选旧路径。
    pub previous_path: Option<PathBuf>,
}

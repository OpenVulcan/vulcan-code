//! Filesystem watcher backends.
//! 文件系统监听后端。

/// Polling watcher backend without platform notifications.
/// 不依赖平台通知的轮询监听后端。
pub mod polling;

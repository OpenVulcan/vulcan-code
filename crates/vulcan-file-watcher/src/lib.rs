//! Reusable file-watcher foundation crate.
//! 可复用文件监听基础 crate。

/// Generic event routing adapters.
/// 通用事件路由适配器。
pub mod adapters;
/// Reusable watcher event types.
/// 可复用监听事件类型。
pub mod event;
/// Common watcher filesystem adapters.
/// 通用监听器文件系统适配器。
pub mod fs;
/// Live notify-based multi-subscriber watcher.
/// 基于 notify 的实时多订阅监听器。
pub mod live;
/// Shared watcher traits.
/// 共享监听 trait。
pub mod traits;
/// Watcher lifecycle helpers.
/// 监听器生命周期辅助。
pub mod watcher;

#[cfg(test)]
mod tests;

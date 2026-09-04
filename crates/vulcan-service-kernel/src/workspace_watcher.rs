//! Service-owned physical workspace watcher shared by independent subscribers.
//! 由服务持有并供独立订阅者共享的物理工作区监听器。

use std::sync::Arc;

use vulcan_file_watcher::live::{LiveFileWatcher, LiveFileWatcherSubscriber, LiveWatchReceiver};

/// Process service wrapper around one notify-backed physical watcher.
/// 单个 notify 物理监听器之上的进程服务包装器。
#[derive(Clone)]
pub(crate) struct WorkspaceWatcherCenter {
    /// Physical watcher shared by Git, VCE, and future workspace consumers.
    /// Git、VCE 与后续工作区消费者共享的物理监听器。
    watcher: Arc<LiveFileWatcher>,
}

impl WorkspaceWatcherCenter {
    /// Create one service-owned physical watcher.
    /// 创建一个由服务持有的物理监听器。
    ///
    /// # Returns
    /// 返回值
    /// A shared watcher center or an explicit notify initialization error.
    /// 返回共享监听中心或明确的 notify 初始化错误。
    pub(crate) fn new() -> Result<Self, String> {
        let watcher = LiveFileWatcher::new()
            .map_err(|error| format!("无法初始化共享工作区文件监听器：{error}"))?;
        Ok(Self {
            watcher: Arc::new(watcher),
        })
    }

    /// Allocate one logically isolated subscriber over the shared physical watcher.
    /// 在共享物理监听器上分配一个逻辑隔离订阅者。
    ///
    /// # Returns
    /// 返回值
    /// The subscriber registration owner and its independent event receiver.
    /// 返回订阅注册所有者及其独立事件接收器。
    pub(crate) fn subscribe(&self) -> (LiveFileWatcherSubscriber, LiveWatchReceiver) {
        self.watcher.add_subscriber()
    }
}

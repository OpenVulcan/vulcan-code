//! Service-owned physical workspace watcher shared by independent subscribers.
//! 由服务持有并供独立订阅者共享的物理工作区监听器。

use std::sync::Arc;

use vulcan_file_watcher::live::{LiveFileWatcher, LiveFileWatcherSubscriber, LiveWatchReceiver};

/// Process service wrapper around one notify-backed physical watcher.
/// 单个 notify 物理监听器之上的进程服务包装器。
#[derive(Clone)]
pub(crate) struct WorkspaceWatcherCenter {
    /// Physical watcher shared by Git and other workspace consumers.
    /// Git 与其他工作区消费者共享的物理监听器。
    watcher: Arc<LiveFileWatcher>,
}

/// Verifies releasing a peer subscription preserves Git's physical watch delivery; returns no value.
/// 验证释放并行订阅后 Git 的物理监听仍能投递变更；无参数与返回值。
#[cfg(test)]
#[tokio::test]
async fn workspace_watcher_preserves_git_subscription_after_peer_release() {
    // Use a real isolated directory and the same center API consumed by the Git watcher.
    // 使用真实隔离目录与 Git 监听器消费的同一个中心接口。
    let directory = tempfile::tempdir().expect("watch fixture directory");
    // Canonical paths match the notify backend's native Windows path representation.
    // 规范路径与 notify 后端的 Windows 原生路径表示保持一致。
    let root = directory
        .path()
        .canonicalize()
        .expect("canonical watch root");
    // Each consumer owns its subscription and registration independently.
    // 每个消费者独立持有自己的订阅及注册。
    let center = WorkspaceWatcherCenter::new().expect("physical workspace watcher");
    let (git, mut events) = center.subscribe();
    let _git_registration = git.register_path(root.clone(), true);
    let (peer, _peer_events) = center.subscribe();
    let peer_registration = peer.register_path(root.clone(), true);
    drop(peer_registration);
    drop(peer);
    // Generate a change only after the other consumer has released its watch.
    // 仅在另一个消费者释放监听后产生变更。
    let changed = root.join("git-change.txt");
    std::fs::write(&changed, b"changed").expect("write watched fixture");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            // A root invalidation is authoritative for every descendant, including the changed file.
            // 根目录失效对全部后代均具权威性，包括本次变更文件。
            let event = events
                .recv()
                .await
                .expect("Git subscription must remain live");
            if event
                .paths
                .iter()
                .any(|path| path == &root || path == &changed)
            {
                break;
            }
        }
    })
    .await
    .expect("Git must continue receiving changes after a peer watch is released");
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
        let watcher = LiveFileWatcher::new().map_err(|error| {
            format!("Cannot initialize the shared workspace file watcher: {error}")
        })?;
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

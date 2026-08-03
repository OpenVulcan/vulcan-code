//! Shared media-transfer primitives used by provider upload adapters.
//! 供应商上传适配器共用的媒体传输基础能力。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// Reports the number of source bytes transferred and the optional source size.
/// 报告已传输的源字节数与可选的源文件大小。
pub type MediaUploadProgressCallback = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Coordinates cooperative cancellation for one pre-dispatch media transfer.
/// 协调单次分派前媒体传输的协作式取消。
#[derive(Clone, Default)]
pub struct MediaUploadCancellation {
    /// Atomic state makes cancellation observable before waiting on the notifier.
    /// 原子状态保证在等待通知器前也能观察到取消。
    cancelled: Arc<AtomicBool>,
    /// Notifier wakes a request that is waiting for network headers or a response body.
    /// 通知器唤醒正在等待网络响应头或响应体的请求。
    notify: Arc<Notify>,
}

impl MediaUploadCancellation {
    /// Creates one cancellation handle in the active state.
    /// 创建一个处于活动状态的取消句柄。
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the transfer cancelled and wakes every waiter.
    /// 标记传输已取消并唤醒所有等待者。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    /// Returns whether cancellation has already been requested.
    /// 返回是否已经请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Waits for a cancellation notification.
    /// 等待取消通知。
    pub async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaUploadCancellation;
    use std::time::Duration;

    /// Verifies that a cancellation requested before waiting is still observed immediately.
    /// 验证在等待前发出的取消请求仍会被立即观察到。
    #[tokio::test]
    async fn wait_observes_preexisting_cancellation() {
        let cancellation = MediaUploadCancellation::new();
        cancellation.cancel();
        tokio::time::timeout(Duration::from_millis(100), cancellation.wait())
            .await
            .expect("preexisting cancellation must wake the waiter");
    }
}

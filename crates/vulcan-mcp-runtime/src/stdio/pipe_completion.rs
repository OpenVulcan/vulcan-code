//! A retained receipt for actual native pipe release, independent of asynchronous observers.
//! 实际原生管道释放的保留回执，独立于异步观察者。

use std::io;
use tokio::sync::watch;

/// Observes the original pipe owner's terminal result without cancelling its native work.
/// 观察原管道所有者的终态结果，不取消其原生工作。
#[derive(Clone)]
pub(super) struct PipeCompletion {
    /// Only the actual owner may publish success after dropping its pipe handle.
    /// 仅实际所有者可在释放管道句柄后发布成功。
    result: watch::Receiver<Option<Result<(), String>>>,
}

impl PipeCompletion {
    /// Creates one initially pending receipt and its unique native result publisher.
    /// 创建一个初始待完成回执及其唯一原生结果发布者。
    /// No inputs; returns the publisher and a clonable observer, neither of which drives cleanup.
    /// 无输入；返回发布者及可克隆观察者，两者均不驱动清理。
    pub(super) fn channel() -> (watch::Sender<Option<Result<(), String>>>, Self) {
        // A retained watch value permits repeat observation without consuming the terminal evidence.
        // 保留的观察值允许重复观察，不消费终态证据。
        let (sender, result) = watch::channel(None);
        (sender, Self { result })
    }

    /// Waits for actual release evidence; cancellation only discards this observer.
    /// 等待实际释放证据；取消仅丢弃当前观察者。
    /// No inputs; returns the original outcome, or an explicit missing-receipt error if the worker vanished.
    /// 无输入；返回原结果，或在工作线程消失时明确返回缺少回执错误。
    pub(super) async fn wait(&self) -> io::Result<()> {
        // Each wait owns a receiver so an abandoned wait cannot consume another observer's wakeup.
        // 每次等待拥有一个接收者，使放弃的等待不能消费其他观察者的唤醒。
        let mut receiver = self.result.clone();
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result.map_err(io::Error::other);
            }
            receiver.changed().await.map_err(|_| {
                io::Error::other("MCP pipe worker ended without confirming actual pipe release.")
            })?;
        }
    }
}

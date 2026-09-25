//! Bounded action admission and cancellation before actual Lua execution.
//! 有界 Action 入场及实际 Lua 执行前的取消。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio::time::Instant;

/// One invocation's immutable deadline and waiter-owned cancellation signal.
/// 单次调用的不可变截止时间及等待方持有的取消信号。
pub(super) struct ActionExecution {
    /// Supervisory record supplies explicit caller cancellation and phase observations.
    /// 监督记录提供显式调用方取消与阶段观察。
    pub(super) record: Arc<super::action_status::ActionRecord>,
    /// Deadline starts at API entry, including validation and queued work.
    /// 截止时间从接口入场开始计算，包含校验与排队。
    pub(super) deadline: Instant,
    /// Dropping the HTTP waiter cancels work that has not entered synchronous execution.
    /// HTTP 等待方离开时取消尚未进入同步执行的任务。
    pub(super) cancelled: oneshot::Receiver<()>,
}

impl ActionExecution {
    /// Waits for admission within the original budget; cancellation removes the queued future.
    /// 在原始预算内等待入场；取消时移除排队 Future。
    /// Returns the supplied operation's result or an explicit pre-execution cancellation/timeout.
    /// 返回传入操作的结果，或明确的执行前取消或超时。
    pub(super) async fn wait<T>(
        &mut self,
        pending: impl Future<Output = T>,
    ) -> Result<T, VulcanPluginRuntimeError> {
        self.record.check()?;
        tokio::select! {
            biased;
            _ = self.record.cancelled.notified() => Err(VulcanPluginRuntimeError::new("Plugin action cancelled before execution.")),
            _ = &mut self.cancelled => { self.record.withdraw(); Err(VulcanPluginRuntimeError::new("Plugin action cancelled before execution.")) },
            _ = tokio::time::sleep_until(self.deadline) => Err(VulcanPluginRuntimeError::new("Plugin action deadline exceeded before execution.")),
            result = pending => Ok(result),
        }
    }

    /// Rechecks the waiter and returns the remaining millisecond budget immediately before execution.
    /// 在执行前重新检查等待方并返回剩余毫秒预算。
    pub(super) fn remaining_ms(&mut self) -> Result<u64, VulcanPluginRuntimeError> {
        self.record.check()?;
        if !matches!(
            self.cancelled.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ) {
            self.record.withdraw();
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action cancelled before execution.",
            ));
        }
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_millis();
        if remaining == 0 {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action deadline exceeded before execution.",
            ));
        }
        Ok(remaining.min(u64::MAX as u128) as u64)
    }
}

impl VulcanPluginRuntimeManager {
    /// Reserves bounded outstanding work using the manifest's existing per-plugin capacity.
    /// 使用清单既有的插件容量预留有界在途任务。
    /// The permit is owned until actual cleanup completes, including after waiter cancellation.
    /// 许可一直持有至实际清理完成，包括等待方取消之后。
    pub(super) fn admit_action(
        &self,
        plugin_id: &str,
        capacity: usize,
    ) -> Result<OwnedSemaphorePermit, VulcanPluginRuntimeError> {
        let mut admissions = self.state.action_admissions.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin action admission registry is poisoned.")
        })?;
        admissions.retain(|_, semaphore| semaphore.strong_count() > 0);
        let semaphore = match admissions.get(plugin_id).and_then(std::sync::Weak::upgrade) {
            Some(semaphore) => semaphore,
            None => {
                let semaphore = Arc::new(Semaphore::new(capacity));
                admissions.insert(plugin_id.to_owned(), Arc::downgrade(&semaphore));
                semaphore
            }
        };
        semaphore.try_acquire_owned().map_err(|_| {
            VulcanPluginRuntimeError::new(
                "Plugin action queue is full; wait for current work to finish.",
            )
        })
    }
}

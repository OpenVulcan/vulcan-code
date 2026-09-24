//! Process-local ownership spanning accepted-message wakeup and execution admission.
//! 跨越已接纳消息唤醒及执行准入的进程内所有权。

use super::*;

/// Releases one scheduled wakeup even when its future is cancelled or unwinds.
/// 即使 Future 取消或展开，也释放一次已调度唤醒。
pub(crate) struct DispatchOwner {
    /// Weak service ownership avoids extending the service lifetime.
    /// 弱服务所有权避免延长服务寿命。
    state: std::sync::Weak<std::sync::Mutex<crate::state::ServiceBridgeState>>,
    /// Exact target whose asynchronous handoff is in progress.
    /// 正在异步交接的精确目标。
    target_id: Uuid,
}

impl DispatchOwner {
    /// Registers before spawning; returns an owned guard or an explicit registry error.
    /// 生成任务前登记；返回所属守卫或明确注册表错误。
    pub(crate) fn acquire(
        state: &SharedServiceBridgeState,
        target_id: Uuid,
    ) -> Result<Self, String> {
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        *guard
            .collaboration_dispatch_owners
            .entry(target_id)
            .or_default() += 1;
        Ok(Self {
            state: Arc::downgrade(state),
            target_id,
        })
    }
}

impl Drop for DispatchOwner {
    /// Removes only this wakeup; concurrent wakeups retain their independent ownership.
    /// 仅移除此唤醒；并发唤醒保留各自独立所有权。
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            match state.lock() {
                Ok(mut state) => {
                    if let Some(count) =
                        state.collaboration_dispatch_owners.get_mut(&self.target_id)
                    {
                        *count -= 1;
                        if *count == 0 {
                            state.collaboration_dispatch_owners.remove(&self.target_id);
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "Collaboration dispatch ownership cleanup failed")
                }
            }
        }
    }
}

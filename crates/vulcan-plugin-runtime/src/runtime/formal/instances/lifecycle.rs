//! Single-word lifecycle phase of one persistent instance.
//! 单个持久实例的单字生命周期阶段。

use std::sync::atomic::{AtomicU8, Ordering};

/// The instance admits calls.
/// 实例接纳调用。
const OPEN: u8 = 0;
/// A lifecycle request (drain, disable, replacement or interruption) is closing the instance.
/// 生命周期请求（排空、停用、替换或中断）正在关闭实例。
const CLOSING: u8 = 1;
/// The instance's own failure closed it, so the next call may replace it.
/// 实例因自身失败而关闭，因此下一次调用可以替换它。
const FAULTED: u8 = 2;

/// Closing intent and its cause, held in one atomic so no reader can observe one without the other.
/// 关闭意图及其原因保存在同一原子量中，任何读取方都不会只观测到其一。
///
/// Only an open instance changes phase; the first transition wins. A fault that races a
/// lifecycle close therefore never relabels that close as replaceable, and a later close
/// never hides a recorded fault. Closing intent never proves physical termination.
/// 只有开放实例会改变阶段，首个转换获胜。因此与生命周期关闭竞争的故障绝不会把该关闭改标为
/// 可替换，之后的关闭也绝不会掩盖已记录的故障。关闭意图绝不证明物理终止。
pub(super) struct InstanceLifecycle(AtomicU8);

impl InstanceLifecycle {
    /// Creates an open lifecycle.
    /// 创建开放的生命周期。
    pub(super) fn new() -> Self {
        Self(AtomicU8::new(OPEN))
    }

    /// Records a lifecycle close unless the instance already left the open phase.
    /// 记录生命周期关闭，除非实例已离开开放阶段。
    pub(super) fn close(&self) {
        self.leave_open(CLOSING);
    }

    /// Records that the instance's own failure closed it, unless it already left the open phase.
    /// 记录实例因自身失败而关闭，除非它已离开开放阶段。
    pub(super) fn fault(&self) {
        self.leave_open(FAULTED);
    }

    /// Returns whether the instance no longer admits calls, for any cause.
    /// 返回实例是否已因任何原因不再接纳调用。
    pub(super) fn is_closing(&self) -> bool {
        self.0.load(Ordering::Acquire) != OPEN
    }

    /// Returns whether the instance's own failure closed it.
    /// 返回实例是否因自身失败而关闭。
    pub(super) fn is_faulted(&self) -> bool {
        self.0.load(Ordering::Acquire) == FAULTED
    }

    /// Moves an open lifecycle to `phase`; any other phase is final.
    /// 将开放生命周期移至 `phase`；其他阶段均为最终阶段。
    fn leave_open(&self, phase: u8) {
        let _ = self
            .0
            .compare_exchange(OPEN, phase, Ordering::AcqRel, Ordering::Acquire);
    }
}

#[cfg(test)]
mod tests {
    use super::InstanceLifecycle;

    /// The first transition out of the open phase decides the cause; later ones never relabel it.
    /// 离开开放阶段的首个转换决定原因；之后的转换绝不改标。
    #[test]
    fn first_transition_decides_the_close_cause() {
        let lifecycle = InstanceLifecycle::new();
        assert!(!lifecycle.is_closing() && !lifecycle.is_faulted());

        lifecycle.close();
        lifecycle.fault();
        assert!(lifecycle.is_closing() && !lifecycle.is_faulted());

        let lifecycle = InstanceLifecycle::new();
        lifecycle.fault();
        lifecycle.close();
        assert!(lifecycle.is_closing() && lifecycle.is_faulted());
    }
}

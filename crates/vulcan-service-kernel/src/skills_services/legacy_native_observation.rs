//! Exact native milestones inject observer loss without mocking a transport.
//! 精确原生里程碑注入观察者丢失，不模拟传输。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Select one actual milestone for an explicitly allocated fixture UUID.
/// 为明确分配夹具 UUID 选择单个实际里程碑。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Milestone {
    /// Actual initialize and complete discovery succeeded before publication.
    /// 实际初始化及完整发现在发布之前成功。
    Ready,
    /// The actual full pointer and adjacent confirmation completed.
    /// 实际完整指针及相邻确认完成。
    Published,
}

/// Exact current-thread fault contains no secret or physical resource.
/// 精确当前线程故障不包含秘密或物理资源。
struct Fault {
    /// Only this allocated plugin may consume the fault.
    /// 仅此已分配插件可消费故障。
    plugin_id: String,
    /// Only the selected real milestone may consume the fault.
    /// 仅所选真实里程碑可消费故障。
    milestone: Milestone,
    /// The outer observer retains only a boolean witness.
    /// 外部观察者仅保留布尔见证。
    fired: Rc<Cell<bool>>,
}

thread_local! {
    /// Thread isolation excludes unrelated executors and accounts.
    /// 线程隔离排除无关执行器及账户。
    static FAULT: RefCell<Option<Fault>> = const { RefCell::new(None) };
}

/// Non-Send RAII observation clears its own exact slot on every exit.
/// 不可发送 RAII 观测在每次退出时清除自身精确槽。
pub(super) struct Scope {
    /// Rc prohibits moving this scope to another executor thread.
    /// Rc 禁止将此作用域移至另一执行器线程。
    fired: Rc<Cell<bool>>,
}

impl Scope {
    /// Arm plugin_id at milestone; return isolated non-secret observation ownership.
    /// 在 milestone 为 plugin_id 布防；返回隔离无秘密观测所有权。
    pub(super) fn arm(plugin_id: &str, milestone: Milestone) -> Self {
        assert!(
            plugin_id
                .strip_prefix("legacy-secret-")
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
                .is_some_and(|id| !id.is_nil()),
            "native fault requires allocated fixture UUID"
        );
        // Observe a boolean only, without retaining native or full-publication ownership.
        // 仅观察布尔值，不保留原生或完整发布所有权。
        let fired = Rc::new(Cell::new(false));
        FAULT.with(|slot| {
            // Reject overlap instead of replacing another scope's original fault.
            // 拒绝重叠，而非替换另一作用域原故障。
            let mut fault = slot.borrow_mut();
            assert!(fault.is_none(), "native fault scopes cannot overlap");
            *fault = Some(Fault {
                plugin_id: plugin_id.into(),
                milestone,
                fired: Rc::clone(&fired),
            });
        });
        Self { fired }
    }

    /// Return whether the exact actual milestone consumed this original fault.
    /// 返回精确实际里程碑是否消费此原故障。
    pub(super) fn fired(&self) -> bool {
        self.fired.get()
    }
}

impl Drop for Scope {
    /// Clear only this original allocation without touching a later observer.
    /// 仅清除此原分配，不触及后继观察者。
    fn drop(&mut self) {
        FAULT.with(|slot| {
            // Original Rc identity cannot be replaced by plugin-name equality.
            // 原 Rc 身份不能由插件名称相等替代。
            let mut fault = slot.borrow_mut();
            if fault
                .as_ref()
                .is_some_and(|fault| Rc::ptr_eq(&fault.fired, &self.fired))
            {
                fault.take();
            }
        });
    }
}

/// Panic once after plugin_id's exact milestone; performs no transport or credential operation.
/// 在 plugin_id 精确 milestone 之后 panic 一次；不执行传输或凭据操作。
pub(super) fn observe(plugin_id: &str, milestone: Milestone) {
    // Consume before panic and release RefCell so outer RAII observation remains usable.
    // panic 前消费并释放 RefCell，使外部 RAII 观测保持可用。
    let fault = FAULT.with(|slot| {
        let mut fault = slot.borrow_mut();
        if fault
            .as_ref()
            .is_some_and(|fault| fault.plugin_id == plugin_id && fault.milestone == milestone)
        {
            fault.take()
        } else {
            None
        }
    });
    if let Some(fault) = fault {
        fault.fired.set(true);
        panic!("Injected observer loss after the exact actual native milestone.");
    }
}

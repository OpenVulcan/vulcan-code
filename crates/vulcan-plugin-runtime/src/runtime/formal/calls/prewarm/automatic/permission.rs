//! A host lifecycle owner closes initialization admission without owning any VM or native operation.
//! 宿主生命周期所有者关闭初始化入场，不拥有任何 VM 或原生操作。

use super::*;

/// Single host owner of a trigger; dropping or closing it permanently revokes all issued permits.
/// 触发的唯一宿主所有者；丢弃或关闭时永久撤销全部已签发许可。
#[derive(Debug, Default)]
pub struct VulcanPluginAutomaticPrewarmTrigger {
    /// Shared permission is independent from retained operation and receipt ownership.
    /// 共用权限独立于保留操作及回执归属。
    permission: Arc<Permission>,
}

/// Cloneable initialization authority issued only by its original live host trigger.
/// 仅由原活动宿主触发签发的可克隆初始化权威。
#[derive(Debug, Clone)]
pub struct VulcanPluginAutomaticPrewarmPermit {
    /// One gate linearizes revocation and native submission across every target in the trigger.
    /// 单个门对触发内全部目标的撤销与原生提交进行排序。
    permission: Arc<Permission>,
}

/// Shared authorization state; it never mirrors scheduler allocation or queue counters.
/// 共用授权状态；绝不镜像调度器分配或队列计数。
#[derive(Debug, Default)]
struct Permission {
    /// Native enqueue and closure are short mutually exclusive operations; Lua runs outside this lock.
    /// 原生入队与关闭为短时互斥操作；Lua 在此锁之外运行。
    gate: Mutex<()>,
    /// One irreversible bit permits nonblocking cancellation observation while initialization is in flight.
    /// 单个不可逆位允许初始化在途时以非阻塞方式观测取消。
    closed: AtomicBool,
}

impl VulcanPluginAutomaticPrewarmTrigger {
    /// Issue a permit bound to this trigger; a permit issued after closure remains revoked.
    /// 签发绑定至此触发的许可；关闭后签发的许可仍保持撤销。
    pub fn permit(&self) -> VulcanPluginAutomaticPrewarmPermit {
        VulcanPluginAutomaticPrewarmPermit {
            permission: Arc::clone(&self.permission),
        }
    }

    /// Close new native admission and wait for any already-admitted enqueue boundary; never wait for Lua.
    /// 关闭新原生入场并等待已经入场的入队边界；绝不等待 Lua。
    pub fn close(&self) {
        // Poisoned admission cannot reopen authority; revocation must still mark the original permission closed.
        // 中毒入场不能重新开放权威；撤销仍须将原权限标记为关闭。
        let _gate = self
            .permission
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.permission.closed.store(true, Ordering::Release);
    }
}

impl Drop for VulcanPluginAutomaticPrewarmTrigger {
    /// Dropping the unique lifecycle owner revokes outstanding permits while receipts remain owned.
    /// 丢弃唯一生命周期所有者撤销在外许可，同时回执继续具有归属。
    fn drop(&mut self) {
        self.close();
    }
}

impl VulcanPluginAutomaticPrewarmPermit {
    /// Observe irreversible revocation without waiting for a native enqueue; return true after closure.
    /// 不等待原生入队地观测不可逆撤销；关闭后返回真。
    pub fn is_closed(&self) -> bool {
        self.permission.closed.load(Ordering::Acquire)
    }

    /// Hold exact lifecycle authority through registration or native enqueue; reject revoked or poisoned owners.
    /// 跨登记或原生入队持有精确生命周期权威；拒绝已撤销或中毒所有者。
    pub(in crate::runtime::formal::calls) fn admit(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ()>, VulcanPluginRuntimeError> {
        let gate = self.permission.gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Automatic prewarm lifecycle gate is poisoned.")
        })?;
        if self.is_closed() {
            return Err(VulcanPluginRuntimeError::new(
                "Automatic prewarm lifecycle is closed.",
            ));
        }
        Ok(gate)
    }

    /// Compare actual permission owners, preventing a duplicate origin from replacing its revoked permit.
    /// 比较真实权限所有者，防止重复来源替换其已撤销许可。
    pub(super) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.permission, &other.permission)
    }

    /// Charge the shared allocation conservatively for every retained target; return fixed metadata bytes.
    /// 为每个保留目标保守计费共用分配；返回固定元数据字节数。
    pub(in crate::runtime::formal::calls) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Permission>()
    }
}

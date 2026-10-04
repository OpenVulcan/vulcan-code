//! Original predecessor abort reconciles uncertain publication without replaying callbacks or replacing ownership.
//! 原前驱中止核对不确定发布，不重放回调或替换所有权。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Arm request's test-only loss after a real Aborted write; return same-thread RAII cleanup.
    /// 启用 request 真实 Aborted 写入后的仅测试丢失；返回同线程 RAII 清理。
    #[cfg(test)]
    pub(crate) fn lose_predecessor_abort_observation_once(&self, request: Uuid) -> impl Drop {
        abort_observation_fault::arm(&self.root, request)
    }

    /// Abort transaction under its exact original record and reconcile uncertain writes with indexed Aborted readback.
    /// 在精确 original 记录下中止 transaction，并以已索引 Aborted 回读核对不确定写入。
    /// Original full authority, unchanged predecessor and exclusive request readiness are required before any confirmation.
    /// 任何确认前均要求完整原权威、未变前驱及排他请求就绪状态。
    /// Return confirmed abort; unknown outcomes preserve transaction's old observation without new requests or callbacks.
    /// 返回已确认中止；未知结果保留 transaction 旧观测，不创建新请求或调用回调。
    pub(crate) fn abort_activation_predecessor_restoration_owned(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        original: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        // Reject cross-store ownership before opening a namespace in the wrong configured root.
        // 在错误配置根打开命名空间之前拒绝跨存储所有权。
        if transaction.store_root != self.root {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor abort belongs to another store.",
            ));
        }
        // Full record equality includes schema and immutable preparation, not only matching UUID or target bytes.
        // 完整记录相等包含模式及不可变准备，不仅是 UUID 或目标字节匹配。
        if !transaction
            .record
            .matches_predecessor_restoration_original(original)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor abort authority changed.",
            ));
        }
        // One existing namespace fence spans selected predecessor, indexed receipt, write and immediate readback.
        // 单个既有命名空间屏障覆盖选定前驱、索引回执、写入及即时回读。
        let _namespace = self.namespace_lock(&original.target.plugin_id, false)?;
        // Reuse the sole readiness retirement contract, including when disk already contains Aborted.
        // 复用唯一就绪退役契约，包括磁盘已有 Aborted 的情况。
        Self::require_readiness_retired(transaction)?;
        if self.read_activation(&original.target.plugin_id)? != original.predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor changed before abort confirmation.",
            ));
        }
        // Derive the sole acceptable terminal image from the frozen full original before touching memory or disk.
        // 触及内存或磁盘前，从固定完整原记录派生唯一可接受终态映像。
        let mut aborted = original.clone();
        aborted.phase = Phase::Aborted;
        if self
            .indexed_transaction_locked(&original.target.plugin_id)?
            .as_ref()
            == Some(&aborted)
        {
            transaction.record = aborted;
            return Ok(());
        }
        self.check_transaction_owner(transaction)?;
        if !matches!(
            transaction.record.phase,
            Phase::Prepared | Phase::Committing
        ) {
            return Err(Error::new(
                Code::Conflict,
                "Only the unchanged unpublished original can abort.",
            ));
        }
        // Actual atomic replacement runs once; a post-rename synchronization error does not prove absence.
        // 实际原子替换执行一次；重命名后的同步错误不证明写入未发生。
        let write_result = self.write_transaction_locked(&aborted);
        #[cfg(test)]
        // Fault injection activates only after these exact real bytes were successfully published.
        // 仅在这些精确真实字节成功发布后启用故障注入。
        let observation_lost =
            abort_observation_fault::consume(&self.root, aborted.request_id, write_result.is_ok());
        #[cfg(test)]
        // Lose the local result while leaving real persisted Aborted bytes untouched.
        // 丢失局部结果，同时保持真实持久 Aborted 字节不变。
        let write_result = if observation_lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of successful Aborted write observation.",
            ))
        } else {
            write_result
        };
        if let Err(write_error) = write_result {
            // Read exactly the indexed original in the same namespace; canonical public queries remain unaffected.
            // 在相同命名空间读取精确已索引原请求；规范公开查询保持不受影响。
            let read = || self.indexed_transaction_locked(&original.target.plugin_id);
            #[cfg(test)]
            // Only this immediate observation can be lost, enabling later same-owner confirmation.
            // 仅此即时观测可丢失，使稍后相同所有者仍可确认。
            let readback = abort_observation_fault::immediate_readback(observation_lost, read)?;
            #[cfg(not(test))]
            // Production uncertain writes use the real exact indexed readback without injected controls.
            // 生产不确定写入使用真实精确索引回读，不提供注入开关。
            let readback = read()?;
            if readback.as_ref() != Some(&aborted) {
                return Err(write_error);
            }
        }
        transaction.record = aborted;
        Ok(())
    }
}

/// Test-only scoped observation loss preserves real Aborted bytes and all canonical inspection paths.
/// 仅测试的作用域观测丢失保留真实 Aborted 字节及全部规范检查路径。
#[cfg(test)]
mod abort_observation_fault {
    use super::*;

    std::thread_local! {
        /// Only the exact original root/request on this thread loses its next successful abort observation.
        /// 仅此线程精确原根目录／请求丢失下一次成功中止观测。
        static ARMED: std::cell::RefCell<Option<(PathBuf, Uuid)>> = const { std::cell::RefCell::new(None) };
    }

    /// Same-thread cleanup clears an unconsumed original fault, including assertion unwinding.
    /// 同线程清理清除未消费原故障，包括断言展开。
    pub(super) struct Scope {
        /// Exact store and request prevent interference with unrelated transactions.
        /// 精确存储及请求防止干扰无关事务。
        key: (PathBuf, Uuid),
        /// Non-Send ownership prevents cleanup from reaching another thread-local slot.
        /// 不可跨线程所有权防止清理访问另一线程局部槽。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Clear only this scope's armed key; no parameters, persisted writes or returned value.
        /// 仅清除此作用域已启用键；无参数、持久写入或返回值。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // Borrow only this thread's fault so unrelated scopes retain their own identities.
                // 仅借用此线程故障，使无关作用域保留自身身份。
                let mut armed = armed.borrow_mut();
                if armed.as_ref() == Some(&self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Arm root/request's single post-write failure; return non-Send cleanup and reject overlapping scopes.
    /// 启用 root／request 单次写入后失败；返回不可跨线程清理，并拒绝重叠作用域。
    pub(super) fn arm(root: &Path, request: Uuid) -> Scope {
        // Preserve the existing full storage identity without allocating another request owner.
        // 保留既有完整存储身份，不分配另一请求所有者。
        let key = (root.to_path_buf(), request);
        ARMED.with(|armed| {
            // Tests must finish their prior scope before another fault is armed on the same thread.
            // 测试必须在同线程启用另一故障前结束先前作用域。
            let mut armed = armed.borrow_mut();
            assert!(
                armed.is_none(),
                "predecessor abort observation fault scope overlaps"
            );
            *armed = Some(key.clone());
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Consume only root/request's successful real write, returning whether its observation is lost.
    /// 仅消费 root／request 成功真实写入，返回是否丢失其观测。
    pub(super) fn consume(root: &Path, request: Uuid, written: bool) -> bool {
        ARMED.with(|armed| {
            // Never claim Aborted publication after the actual writer failed before confirmation.
            // 实际写入器确认前失败时，绝不声称已发布 Aborted。
            let mut armed = armed.borrow_mut();
            if written
                && armed
                    .as_ref()
                    .is_some_and(|(original, id)| original == root && *id == request)
            {
                armed.take();
                true
            } else {
                false
            }
        })
    }

    /// Fail only lost's immediate indexed readback; otherwise execute read unchanged and return its result.
    /// 仅使 lost 即时索引回读失败；否则原样执行 read 并返回其结果。
    pub(super) fn immediate_readback<T>(
        lost: bool,
        read: impl FnOnce() -> VulcanPluginStoreResult<T>,
    ) -> VulcanPluginStoreResult<T> {
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of immediate Aborted indexed readback.",
            ))
        } else {
            read()
        }
    }
}

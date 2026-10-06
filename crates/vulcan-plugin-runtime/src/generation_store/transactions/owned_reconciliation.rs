//! Exact original-owner readback reconciles lost checkpoint observations without replacing live leases.
//! 精确原所有者回读核对丢失的检查点观测，不替换存活租约。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use VulcanPluginActivationTransactionPhase as Phase;

/// Apply the scoped test fault only after root's exact written record was successfully persisted.
/// 仅在 root 精确 written 记录成功持久保存后应用作用域测试故障。
/// Return success normally or explicit observation loss; no journal, index or selection is written here.
/// 通常返回成功，或返回显式观测丢失；此处不写入日志、索引或选择。
#[cfg(test)]
pub(super) fn checkpoint_observation_after_write(
    root: &Path,
    written: &VulcanPluginActivationTransactionRecord,
) -> VulcanPluginStoreResult<()> {
    checkpoint_observation_fault::after_write(root, written)
}

impl VulcanPluginGenerationStore {
    /// Arm request/phase's successful real checkpoint loss; return same-thread scope cleanup without production controls.
    /// 启用 request／phase 成功真实检查点丢失；返回同线程作用域清理，不提供生产控制。
    #[cfg(test)]
    pub(crate) fn lose_activation_checkpoint_observation_once(
        &self,
        request: Uuid,
        phase: Phase,
    ) -> impl Drop {
        checkpoint_observation_fault::arm(&self.root, request, phase)
    }

    /// Confirm transaction's exact indexed checkpoint while retaining its same original request, package and credential leases.
    /// 确认 transaction 精确已索引检查点，同时保留其相同原请求、插件包及凭据租约。
    /// Only identical records or an explicit next checkpoint from the original mutation sequence may advance its observation.
    /// 仅相同记录或原变更顺序中明确下一检查点可推进其观测。
    /// Return the confirmed phase; errors preserve its previous record and perform no writes, callbacks or lock reacquisition.
    /// 返回已确认阶段；错误保留先前记录，不执行写入、回调或锁重新取得。
    /// Private Hook/MCP receipts require their combined actual resource handoff and are rejected in every phase.
    /// 私有 Hook／MCP 回执要求组合实际资源交接，并在所有阶段均被拒绝。
    pub fn reconcile_activation_transaction_owned(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<Phase> {
        // Cross-root owners cannot open a foreign namespace or replace the caller's retained physical resources.
        // 跨根所有者不能打开外部命名空间或替换调用方保留的物理资源。
        if transaction.store_root != self.root {
            return Err(Error::new(
                Code::Conflict,
                "Owned activation reconciliation belongs to another store.",
            ));
        }
        // Reuse the full record validator rather than interpreting a preparing reservation as a configured owner.
        // 复用完整记录校验器，不将准备中预留解释为已配置所有者。
        Self::validate_transaction_record(
            &transaction.record,
            &transaction.record.target.plugin_id,
            transaction.record.request_id,
        )?;
        // The actual original package and credentials must still be bound to the complete retained target.
        // 实际原插件包及凭据必须仍绑定至完整保留目标。
        if transaction.generation.store_root != self.root
            || transaction.record.target.generation() != Some(transaction.generation.identity())
            || transaction.credentials.record().package() != transaction.generation.identity()
            || transaction.record.target.credential_revision()
                != transaction.credentials.record().revision()
            || (transaction.record.preparation.is_some()
                && transaction.credentials.record().activation_request()
                    != Some(transaction.record.request_id))
        {
            return Err(Error::new(
                Code::Conflict,
                "Owned activation target differs from its retained physical owners.",
            ));
        }
        // One existing namespace fence spans index, full record, sidecars and exact selected activation.
        // 单个既有命名空间屏障覆盖索引、完整记录、附属回执及精确已选激活。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        #[cfg(test)]
        // Only the first scoped observation after an actual successful checkpoint can be lost.
        // 仅实际成功检查点之后首次作用域观测可丢失。
        checkpoint_observation_fault::before_readback(&self.root, transaction.record.request_id)?;
        // Read the sole declared index and its fully validated original body; never scan or repair an alternate request.
        // 读取唯一声明索引及其完整已校验原正文；绝不扫描或修复替代请求。
        let indexed = self
            .indexed_transaction_locked(&transaction.record.target.plugin_id)?
            .ok_or_else(|| {
                Error::new(
                    Code::Conflict,
                    "Original indexed activation request is missing.",
                )
            })?;
        // Normalize only the phase in a comparison copy; all immutable schema and preparation fields remain exact.
        // 仅在比较副本中对齐阶段；所有不可变模式及准备字段保持精确。
        let mut original_checkpoint = indexed.clone();
        original_checkpoint.phase = transaction.record.phase;
        if original_checkpoint != transaction.record {
            return Err(Error::new(
                Code::Conflict,
                "Original indexed activation authority changed.",
            ));
        }
        // Atomic checkpoint writers stop on a lost result before another checkpoint can be reached.
        // 原子检查点写入器在结果丢失时停止，不能到达另一检查点。
        let admissible = transaction.record.phase == indexed.phase
            || matches!(
                (transaction.record.phase, indexed.phase),
                (Phase::Prepared, Phase::Committing | Phase::Aborted)
                    | (Phase::Committing, Phase::Published | Phase::Aborted)
                    | (Phase::Published, Phase::Completed)
            );
        if !admissible {
            return Err(Error::new(
                Code::Conflict,
                "Original activation checkpoint cannot skip or reverse a mutation step.",
            ));
        }
        // Sidecars are historical evidence of real private resources, whose transfer cannot be inferred by this reader.
        // 附属回执为真实私有资源的历史证据；此读取器不能推断其转交。
        if self.read_target_hook_locked(&indexed)?.is_some()
            || self.read_target_mcp_locked(&indexed)?.is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Private target reconciliation requires the combined original resource handoff.",
            ));
        }
        // Both terminal checkpoints preserve the existing sole actual-readiness retirement contract.
        // 两个终态检查点均保留既有唯一实际就绪退役契约。
        if indexed.phase.terminal() {
            Self::require_readiness_retired(transaction)?;
        }
        // Selection constraints follow the actual checkpoint publication sequence, not revision comparisons.
        // 选择约束遵循实际检查点发布顺序，而非修订比较。
        let selected = self.read_activation(&indexed.target.plugin_id)?;
        // Committing alone may sit on either side of the real atomic activation publication.
        // 仅 Committing 可位于真实原子激活发布的任一侧。
        let selection_matches = match indexed.phase {
            Phase::Prepared | Phase::Aborted => selected == indexed.predecessor,
            Phase::Committing => {
                selected == indexed.predecessor || selected.as_ref() == Some(&indexed.target)
            }
            Phase::Published | Phase::Completed => selected.as_ref() == Some(&indexed.target),
            Phase::Preparing => false,
        };
        if !selection_matches {
            return Err(Error::new(
                Code::Conflict,
                "Original activation checkpoint differs from its exact selected authority.",
            ));
        }
        // Every fallible observation has completed; replace only the confirmed record, never any physical owner.
        // 全部可失败观测已完成；仅替换已确认记录，绝不替换任何物理所有者。
        transaction.record = indexed;
        Ok(transaction.record.phase)
    }
}

/// Test-only post-write observation loss preserves the real journal and later canonical reads.
/// 仅测试的写入后观测丢失保留真实日志及后续规范读取。
#[cfg(test)]
mod checkpoint_observation_fault {
    use super::*;

    /// Exact original checkpoint identity and whether its successful write already lost acknowledgement.
    /// 精确原检查点身份，以及其成功写入是否已经丢失确认。
    struct Fault {
        /// Original root/request/phase exclude unrelated writers and checkpoints.
        /// 原根目录／请求／阶段排除无关写入者及检查点。
        key: (PathBuf, Uuid, Phase),
        /// The first owned indexed readback alone is lost after a successful real write.
        /// 成功真实写入后，仅首次已持有所有者的索引回读丢失。
        awaiting_readback: bool,
    }

    std::thread_local! {
        /// One scoped original checkpoint fault exists on this thread, without production controls.
        /// 此线程存在单个作用域原检查点故障，不提供生产控制。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
    }

    /// Same-thread RAII cleanup clears an unconsumed fault, including assertion unwinding.
    /// 同线程 RAII 清理清除未消费故障，包括断言展开。
    pub(super) struct Scope {
        /// Only this original checkpoint may be reset when the scope exits.
        /// 作用域退出时仅可重置此原检查点。
        key: (PathBuf, Uuid, Phase),
        /// Non-Send ownership keeps cleanup on the armed thread.
        /// 不可跨线程所有权将清理保持在启用线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Reset this scope's remaining fault; takes no input and returns no value.
        /// 重置此作用域剩余故障；无输入且不返回值。
        fn drop(&mut self) {
            // A different scoped identity must never be cleared by this destructor.
            // 此析构器绝不能清除另一作用域身份。
            ARMED.with(|armed| {
                // Inspect only the current thread's original checkpoint.
                // 仅检查当前线程原检查点。
                let mut armed = armed.borrow_mut();
                if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Arm root/request/phase's single successful-write loss; return non-Send cleanup and reject overlapping scopes.
    /// 启用 root／request／phase 单次成功写入丢失；返回不可跨线程清理，并拒绝重叠作用域。
    pub(super) fn arm(root: &Path, request: Uuid, phase: Phase) -> Scope {
        assert!(matches!(
            phase,
            Phase::Committing | Phase::Published | Phase::Completed | Phase::Aborted
        ));
        // Bind the same original checkpoint without allocating any request or storage ownership.
        // 绑定相同原检查点，不分配任何请求或存储所有权。
        let key = (root.to_path_buf(), request, phase);
        ARMED.with(|armed| {
            // Tests must explicitly retire their prior scope before another fault is armed.
            // 测试必须在启用另一故障前明确退役先前作用域。
            let mut armed = armed.borrow_mut();
            assert!(armed.is_none(), "owned checkpoint fault scope overlaps");
            *armed = Some(Fault {
                key: key.clone(),
                awaiting_readback: false,
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Lose only the already successful root/request/phase write observation; return an explicit storage error.
    /// 仅丢失已经成功的 root／request／phase 写入观测；返回显式存储错误。
    pub(super) fn after_write(
        root: &Path,
        written: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        // This hook is called after the actual atomic writer returned success, never instead of that writer.
        // 此钩子在实际原子写入器返回成功后调用，绝不代替该写入器。
        let lost = ARMED.with(|armed| {
            // Keep the fault armed for exactly one subsequent original-owner readback.
            // 保持故障已启用，供恰好一次后续原所有者回读使用。
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && !fault.awaiting_readback
                && fault.key.0 == root
                && fault.key.1 == written.request_id
                && fault.key.2 == written.phase
            {
                fault.awaiting_readback = true;
                true
            } else {
                false
            }
        });
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of successful original checkpoint observation.",
            ))
        } else {
            Ok(())
        }
    }

    /// Lose the first root/request indexed owned readback after its real write; later observations run unchanged.
    /// 真实写入后丢失首次 root／request 已持有所有者索引回读；后续观测原样执行。
    pub(super) fn before_readback(root: &Path, request: Uuid) -> VulcanPluginStoreResult<()> {
        // Canonical query APIs remain unaffected; only the explicitly retained owner observes this loss.
        // 规范查询接口保持不受影响；仅明确保留的所有者观察此丢失。
        let lost = ARMED.with(|armed| {
            // Consume only an observation whose real matching writer has already executed.
            // 仅消费实际匹配写入器已经执行的观测。
            let mut armed = armed.borrow_mut();
            if armed.as_ref().is_some_and(|fault| {
                fault.awaiting_readback && fault.key.0 == root && fault.key.1 == request
            }) {
                armed.take();
                true
            } else {
                false
            }
        });
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of immediate original indexed checkpoint readback.",
            ))
        } else {
            Ok(())
        }
    }
}

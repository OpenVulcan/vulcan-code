//! Request-local completion authority retaining the existing session preparation lease.
//! 保留既有会话准备租约的请求局部完成权限。

use super::*;
use vulcan_session_core::v1::UserPromptResourceBinding;

/// One existing preparation's original scope and revision tracking; the session document remains the sole durable authority.
/// 一次既有准备的原作用域及修订跟踪；会话文档仍为唯一可靠权威。
pub(crate) struct SessionPhysicalPreparation {
    /// The same session owner that captured the input and will permit the API.
    /// 捕获输入并将许可API的同一会话所有者。
    handle: Arc<SessionHandle>,
    /// Keeps the original single-flight gate held until every completed-result worker settles.
    /// 保持原单飞门，直到全部已完成结果工作线程结算。
    _lease: Arc<OwnedMutexGuard<()>>,
    /// Exact source generation; a replaced generation can never receive a late completion.
    /// 精确来源代际；替换代际绝不能收到迟到完成。
    segment_generation: u64,
    /// Prompt occurrences authorized by Core when this preparation captured its input.
    /// 本准备捕获输入时，由Core授权的提示词出现。
    prompt_seqs: Vec<Seq>,
    /// Local comparison state, accessed only after the session writer and publisher when mutating or recapturing.
    /// 局部比较状态；修改或重捕获时仅在会话写入器及发布器之后访问。
    frontier: Mutex<PreparationFrontier>,
}

/// Separates this preparation's own durable writes from any intervening external revision.
/// 区分本准备自身可靠写入与任何期间外部修订。
struct PreparationFrontier {
    /// Last revision known to contain only accepted own writes since capture.
    /// 自捕获以来最后已知仅包含已接纳自身写入的修订。
    expected_revision: u64,
    /// Sticky rejection once any external mutation is observed; own writes cannot clear it.
    /// 观察到任何外部修改后保持的拒绝标志；自身写入不能清除它。
    invalidated: bool,
    /// Actual snapshot produced by the existing durable projection factory, never an edited revision number.
    /// 既有可靠投影工厂产生的实际快照，绝不通过修改修订数字伪造。
    snapshot: SessionProviderProjectionSnapshot,
}

impl SessionPhysicalPreparation {
    /// Creates authority for `snapshot` using the same `handle` and already-acquired `lease`; performs no I/O.
    /// 使用同一`handle`及已取得`lease`为`snapshot`建立权限；不执行IO。
    pub(super) fn new(
        handle: Arc<SessionHandle>,
        lease: Arc<OwnedMutexGuard<()>>,
        snapshot: SessionProviderProjectionSnapshot,
    ) -> Self {
        Self {
            handle,
            _lease: lease,
            segment_generation: snapshot.segment_generation,
            prompt_seqs: snapshot.preparing_user_prompt_media_seqs.clone(),
            frontier: Mutex::new(PreparationFrontier {
                expected_revision: snapshot.document_revision,
                invalidated: false,
                snapshot,
            }),
        }
    }

    /// Persists selected or completed `binding` at `now_ms` without granting a stale transport permission to run.
    /// 在`now_ms`持久化已选或已完成`binding`，不授予陈旧传输运行权限。
    /// Returns false only for a replaced source generation; true means the exact result is durable or already identical.
    /// 仅在来源代际被替换时返回否；是表示精确结果已可靠落盘或已经相同。
    /// Call on the blocking pool, retaining this authority until it finishes even after parent cancellation.
    /// 在阻塞线程池调用，父任务取消后也保留本权限直到完成。
    pub(crate) fn record_resource(
        &self,
        binding: UserPromptResourceBinding,
        now_ms: i64,
    ) -> Result<bool, SessionRuntimeError> {
        if !self.prompt_seqs.contains(&binding.prompt_seq) {
            return Err(SessionRuntimeError::ProviderProjectionInvariant(
                "resource completion is outside the original prompt preparation frontier".into(),
            ));
        }
        self.handle
            .ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        // Preserve the existing writer-before-publisher order; no synchronous guard spans an await.
        // 保留既有先写入器后发布器顺序；同步守卫不跨异步等待。
        let _writer = self
            .handle
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        // New input and picker updates may proceed outside this short durable transaction.
        // 新输入及选择器更新可以在本短可靠事务之外推进。
        let _publisher = self
            .handle
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        // This state records only comparison facts, never a replacement live session document.
        // 本状态仅记录比较事实，绝不是替代实时会话文档。
        let mut frontier = self.lock_frontier()?;
        {
            // Even a later own completion cannot hide a foreign input or configuration mutation.
            // 即使后来的自身完成也不能掩盖外部输入或配置修改。
            let document = self
                .handle
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if document.segment_generation() != self.segment_generation {
                frontier.invalidated = true;
                return Ok(false);
            }
            if document.memory_revision() != frontier.expected_revision {
                frontier.invalidated = true;
            }
        }
        // Keep completion proof in its original prompt before private receipt publication can be interrupted.
        // 在私有回执发布可能被中断之前，将完成证明保留在原提示词中。
        let receipt = self
            .handle
            .mutate_in_memory_if_changed_with_publisher_locked(
                SessionFlushReason::PromptResourcePrepared,
                |document| {
                    Ok(
                        if document.record_user_prompt_resource_delivery(
                            self.segment_generation,
                            binding,
                            now_ms,
                        )? {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        },
                    )
                },
            )?;
        self.handle
            .flush_through_with_writer_and_publisher_locked(receipt.revision)?;
        frontier.expected_revision = receipt.revision;
        Ok(true)
    }

    /// Recaptures the actual durable projection after own completions; returns none on any external mutation.
    /// 在自身完成后重新捕获实际可靠投影；发生任何外部修改时返回空。
    /// Call on the blocking pool before final request encoding; the returned snapshot is the only new proof authority.
    /// 最终请求编码前在阻塞线程池调用；返回快照是唯一新的证明权威。
    pub(crate) fn recapture(
        &self,
    ) -> Result<Option<SessionProviderProjectionSnapshot>, SessionRuntimeError> {
        self.handle
            .ensure_admitted(SessionMutationAdmission::ExistingTurn)?;
        // Use the original owner's serialization and actual on-disk range reader.
        // 使用原所有者的串行化及实际磁盘区间读取器。
        let _writer = self
            .handle
            .writer
            .lock()
            .map_err(|_| SessionRuntimeError::WriterLockPoisoned)?;
        // Keep revision inspection and recapture atomic with respect to event publication.
        // 相对于事件发布保持修订检查及重捕获原子性。
        let _publisher = self
            .handle
            .mutation_publisher
            .lock()
            .map_err(|_| SessionRuntimeError::MutationPublisherLockPoisoned)?;
        // The sticky flag never becomes valid merely because a later owned write advanced the revision.
        // 粘性标志绝不会仅因较晚自身写入推进修订而重新有效。
        let mut frontier = self.lock_frontier()?;
        {
            // External dirty or durable revisions both invalidate the prepared transport.
            // 外部脏修订及可靠修订均使准备传输失效。
            let document = self
                .handle
                .document
                .lock()
                .map_err(|_| SessionRuntimeError::DocumentLockPoisoned)?;
            if document.segment_generation() != self.segment_generation
                || document.memory_revision() != frontier.expected_revision
                || document.durable_revision() != frontier.expected_revision
            {
                frontier.invalidated = true;
            }
        }
        if frontier.invalidated {
            return Ok(None);
        }
        frontier.snapshot = self
            .handle
            .capture_provider_projection_snapshot_with_publisher_locked()?;
        Ok(Some(frontier.snapshot.clone()))
    }

    /// Returns the last real captured snapshot for final proof validation, without changing any revision or source item.
    /// 返回最后真实捕获快照供最终证明校验，不修改任何修订或来源项。
    pub(super) fn last_snapshot(
        &self,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        Ok(self.lock_frontier()?.snapshot.clone())
    }

    /// Reports sticky external invalidation to the existing final API gate; no reset path is exposed.
    /// 向既有最终API门报告粘性外部失效；不暴露重置路径。
    pub(crate) fn invalidated(&self) -> Result<bool, SessionRuntimeError> {
        Ok(self.lock_frontier()?.invalidated)
    }

    /// Locks local comparison state and exposes a safe invariant error if poisoned; no payload or credential is included.
    /// 锁定局部比较状态，中毒时暴露安全不变量错误；不含载荷或凭据。
    fn lock_frontier(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, PreparationFrontier>, SessionRuntimeError> {
        self.frontier.lock().map_err(|_| {
            SessionRuntimeError::ProviderProjectionInvariant(
                "request preparation comparison state lock was poisoned".into(),
            )
        })
    }
}

impl SessionHandle {
    /// Acquires the existing preparation gate after abort so completed-result workers settle before terminal persistence or sealing.
    /// 中止后取得既有准备门，使已完成结果工作线程在终态持久化或封片前结算。
    /// Returns the same gate's owned guard; callers must retain it through their existing terminal transaction.
    /// 返回同一门的自有守卫；调用方必须将其保留至既有终态事务完成。
    pub(crate) async fn acquire_request_preparation_settlement(&self) -> OwnedMutexGuard<()> {
        Arc::clone(&self.physical_request_preparation)
            .lock_owned()
            .await
    }
}

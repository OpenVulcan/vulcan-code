//! Exact restoration owners retain their independent native reservation before the Pending write.
//! 精确恢复所有者在 Pending 写入前保留其独立原生预留权。

use super::*;
use crate::generation_store::PredecessorHandoffReservation;
use crate::{VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase as Phase};

/// Actual private ownership contains real package/credential pins and an exclusive file, without a manager cycle.
/// 实际私有所有权包含真实包、凭据固定句柄及排他文件，不形成管理器循环。
pub(in crate::runtime) struct PredecessorRestorationPrivateOwner {
    /// The same original store handles retries without selecting another root or allocating another owner.
    /// 相同原存储处理重试，不选择其他根目录或分配其他所有者。
    store: VulcanPluginGenerationStore,
    /// One non-cloneable reservation stays present through uncertain persistence and later actual resource transfer.
    /// 单个不可克隆预留权跨不确定持久化及后续实际资源移交保持存在。
    reservation: Mutex<Option<PredecessorHandoffReservation>>,
}

impl PredecessorRestorationPrivateOwner {
    /// Borrow the original reservation once across all durable transfer work; original is the exact retained Aborted image.
    /// 在全部持久转交工作期间借用原预留权一次；original 为精确保留 Aborted 映像。
    /// Return its same File guard without requiring Pending, allowing exact Transferred readback after an unknown write.
    /// 返回相同文件守卫而不要求 Pending，允许未知写入后精确回读 Transferred。
    pub(super) fn prepare_transfer(
        &self,
        original: &VulcanPluginActivationTransactionRecord,
    ) -> Result<PredecessorRestorationTransfer<'_>, VulcanPluginRuntimeError> {
        // This is the sole reservation lock in combined completion, acquired after actual Hook/native guards.
        // 此为组合完成中唯一预留权锁，在实际 Hook／原生守卫后取得。
        let reservation = self
            .reservation
            .lock()
            .map_err(|_| error("Private predecessor reservation lock is poisoned."))?;
        // A successfully transferred empty slot is explicitly terminal; no historical owner is recreated.
        // 成功转交后的空槽显式为终态；不重建历史所有者。
        let retained = reservation.as_ref().ok_or_else(|| {
            error("Original private predecessor reservation was already transferred.")
        })?;
        if original.phase() != Phase::Aborted
            || !original.matches_predecessor_restoration_original(retained.original())
        {
            return Err(error(
                "Predecessor transfer belongs to another full aborted original.",
            ));
        }
        Ok(PredecessorRestorationTransfer {
            store: &self.store,
            reservation,
        })
    }

    /// Validate the same actual reservation against the complete aborted original without allocating or writing another owner.
    /// 按完整已中止原请求校验相同实际预留权，不分配或写入另一所有者。
    /// original is the retained Aborted record; returns a conflict for missing, changed or already transferred ownership.
    /// original 为保留的 Aborted 记录；所有权缺失、变化或已转交时返回冲突。
    pub(super) fn validate_pending(
        &self,
        original: &VulcanPluginActivationTransactionRecord,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Borrow the existing independent file rather than cloning the activation request's readiness lease.
        // 借用既有独立文件，而非克隆激活原请求的就绪租约。
        let reservation = self
            .reservation
            .lock()
            .map_err(|_| error("Private predecessor reservation lock is poisoned."))?;
        // An empty slot certifies no reusable private owner and cannot be repaired from persisted identity.
        // 空槽意味着不存在可复用私有所有者，不能从持久身份修复。
        let reservation = reservation
            .as_ref()
            .ok_or_else(|| error("Original private predecessor reservation was transferred."))?;
        if original.phase() != Phase::Aborted
            || !original.matches_predecessor_restoration_original(reservation.original())
        {
            return Err(error(
                "Private predecessor source belongs to another aborted original.",
            ));
        }
        self.store
            .validate_aborted_predecessor_handoff_owner(reservation)
            .map_err(|failure| error(failure.to_string()))
    }
}

/// One locked original reservation spans the confirmed writer and final infallible same-owner transfer.
/// 单个已锁定原预留权覆盖已确认写入器及最终不可失败同所有者转交。
pub(in crate::runtime) struct PredecessorRestorationTransfer<'owner> {
    /// Borrow the already configured original store; no new storage owner is opened.
    /// 借用已配置原存储；不打开新存储所有者。
    store: &'owner VulcanPluginGenerationStore,
    /// Keep the unique independent File locked through uncertain persistence and all actual transfers.
    /// 保持唯一独立文件锁定，覆盖不确定持久化及全部实际转交。
    reservation: std::sync::MutexGuard<'owner, Option<PredecessorHandoffReservation>>,
}

impl PredecessorRestorationTransfer<'_> {
    /// Confirm the exact original Transferred image using the same reservation; all actual guards must remain held.
    /// 使用相同预留权确认精确原 Transferred 映像；全部实际守卫必须保持持有。
    /// No inputs; an error retains its File and leaves all private owners unchanged.
    /// 无输入；错误保留其文件，并使全部私有所有者保持不变。
    pub(in crate::runtime) fn confirm_transferred(&self) -> Result<(), VulcanPluginRuntimeError> {
        // Seal authority only around this same borrow after composite actual readiness has been validated.
        // 仅在组合实际就绪已校验后，围绕此相同借用封闭权威。
        let authority =
            crate::generation_store::PredecessorHandoffTransferAuthority::from_prepared_reservation(
                self.reservation
                    .as_ref()
                    .expect("locked original predecessor reservation"),
            );
        self.store
            .complete_predecessor_handoff_owned(&authority)
            .map_err(|failure| error(failure.to_string()))
    }

    /// Release the unique original File after confirmed actual Hook/native memory transfers; no inputs or fallible work.
    /// 已确认实际 Hook／原生内存转交后释放唯一原文件；无输入或可失败工作。
    pub(in crate::runtime) fn release(mut self) {
        self.reservation.take();
    }
}

impl std::fmt::Debug for PredecessorRestorationPrivateOwner {
    /// Format only the owner type for `formatter`; never expose configuration, credentials or its lock file.
    /// 为 `formatter` 仅格式化所有者类型；绝不暴露配置、凭据或锁文件。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PredecessorRestorationPrivateOwner")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl VulcanPluginPredecessorRestoration {
    /// Borrow actual reservation paths for a fixture probe; no owner or file is allocated from a UUID.
    /// 为夹具探测借用实际预留权路径；不从 UUID 分配所有者或文件。
    /// `record` selects its existing writer destination; false returns its unique native lease path.
    /// `record` 选择其既有写入器目标；假值返回其唯一原生租约路径。
    pub(in crate::runtime) fn private_owner_test_path(
        &self,
        record: bool,
    ) -> Result<std::path::PathBuf, VulcanPluginRuntimeError> {
        let owner = self
            .private_owner
            .as_ref()
            .ok_or_else(|| error("Actual private predecessor owner is missing."))?;
        let reservation = owner
            .reservation
            .lock()
            .map_err(|_| error("Private predecessor reservation lock is poisoned."))?;
        let reservation = reservation
            .as_ref()
            .ok_or_else(|| error("Actual private predecessor reservation is missing."))?;
        if record {
            owner
                .store
                .predecessor_handoff_test_record_path(reservation)
                .map_err(|failure| error(failure.to_string()))
        } else {
            Ok(owner.store.predecessor_handoff_test_lease_path(reservation))
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Reserve the exact original old selection for `restoration` before abort, retaining every real owner on failure.
    /// 在中止前为 `restoration` 预留精确原旧选择，失败时保留每个真实所有者。
    /// `transaction` is the original mutable owner; booleans are this plugin's current explicit policy memberships.
    /// `transaction` 是原可变所有者；布尔值是此插件当前明确策略成员关系。
    /// Return confirmed Pending; already persisted retries only inspect bytes, and absent Pending uses its same owner's writer.
    /// 返回已确认 Pending；已持久化重试仅检查字节，缺失 Pending 使用相同所有者的写入器。
    /// Neither retry opens another lease, reconstructs resources, or runs a callback.
    /// 两种重试均不打开其他租约、重建资源或运行回调。
    pub fn prepare_activation_predecessor_private_owner(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        restoration: &mut VulcanPluginPredecessorRestoration,
        explicitly_enabled: bool,
        explicitly_disabled: bool,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Registration exclusion spans actual retirement, exact original matching and native reservation retention.
        // 注册互斥覆盖实际退役、精确原记录匹配及原生预留权保留。
        let registration = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| error("Plugin registration gate is poisoned."))?;
        if !matches!(
            transaction.record().phase(),
            Phase::Prepared | Phase::Committing
        ) || !transaction
            .record()
            .matches_predecessor_restoration_original(restoration.original())
        {
            return Err(error(
                "Private predecessor owner requires its exact unpublished original transaction.",
            ));
        }
        // The existing real incarnation and old closed route reject a foreign manager before any store mutation.
        // 既有真实实例及旧关闭路由在任何存储变更前拒绝外来管理器。
        validate_predecessor_callback_current(
            &self.state,
            restoration.predecessor(),
            restoration.drain.identity(),
        )?;
        // Every tracked old/candidate core domain must have physically retired; this grants no host MCP transfer.
        // 每个受跟踪旧代及候选核心域必须物理退役；此检查不授予宿主 MCP 移交。
        self.require_restoration_execution_retired(restoration.request_id())?;
        if self
            .has_unretired_plugin_resources_locked(&restoration.predecessor().id, &registration)?
        {
            return Err(error(
                "Old plugin resources are still retiring before private predecessor reservation.",
            ));
        }
        // Only the same plugin-specific effective enabled policy authorizes the already decided old restoration.
        // 仅相同插件专属有效启用策略授权已决定的旧代恢复。
        let policy = restoration.original().update_policy().ok_or_else(|| {
            error("Private predecessor restoration requires original plugin-specific policy.")
        })?;
        if !policy.desired_enabled()
            || !policy.matches_overrides(explicitly_enabled, explicitly_disabled)
        {
            return Err(error("Original predecessor restoration policy changed."));
        }
        // Complete managed selection matching includes the immutable old configuration and credential revision.
        // 完整托管选择匹配包含不可变旧配置及凭据修订。
        let old = restoration.original().predecessor().ok_or_else(|| {
            error("Private predecessor restoration requires an immutable managed predecessor; legacy is unsupported.")
        })?;
        let managed = restoration.predecessor().managed_package().ok_or_else(|| {
            error("Private predecessor restoration requires an immutable managed predecessor; legacy is unsupported.")
        })?;
        if !managed.matches_activation(old) {
            return Err(error(
                "Private predecessor restoration requires the complete immutable old managed selection; legacy is unsupported.",
            ));
        }
        // Exact configured storage is opened read-only; missing storage cannot be recreated as authority.
        // 精确配置存储以只读方式打开；缺失存储不能重建为权威。
        let store = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|failure| error(failure.to_string()))?
            .ok_or_else(|| error("Original update store is missing."))?;
        if store
            .activation(&old.plugin_id)
            .map_err(|failure| error(failure.to_string()))?
            .as_ref()
            != Some(old)
        {
            return Err(error(
                "Original predecessor selection changed before private reservation.",
            ));
        }
        validate_restoration_receipts(&store, &restoration.drain)?;
        if let Some(owner) = restoration.private_owner.as_ref() {
            // Borrow the already retained native file; a retry never opens a replacement exclusive lease.
            // 借用已保留原生文件；重试绝不打开替代排他租约。
            let mut reservation = owner
                .reservation
                .lock()
                .map_err(|_| error("Private predecessor reservation lock is poisoned."))?;
            let reservation = reservation.as_mut().ok_or_else(|| {
                error("Original private predecessor reservation was transferred.")
            })?;
            if reservation.original() != transaction.record() {
                return Err(error(
                    "Original private predecessor reservation checkpoint changed.",
                ));
            }
            // The unique store primitive reads identical Pending without writing; actual absence uses this same live guard.
            // 唯一存储原语读取相同 Pending 而不写入；实际缺失使用此相同存活守卫。
            return owner
                .store
                .persist_predecessor_handoff_reservation(reservation)
                .map_err(|failure| error(failure.to_string()));
        }
        // Capture the actual current full checkpoint after explicit Prepared-to-Committing original validation.
        // 明确校验 Prepared 到 Committing 原记录后，捕获实际当前完整检查点。
        let original = transaction.record().clone();
        // Allocate independent actual ownership before writing; it does not clone the original request lease.
        // 写入前分配独立实际所有权；不克隆原请求租约。
        let reservation = store
            .prepare_predecessor_handoff_reservation(transaction, &original)
            .map_err(|failure| error(failure.to_string()))?;
        // Install the actual owner before persistence, so an ordinary error cannot drop its File or resource pins.
        // 持久化前安装实际所有者，防止普通错误释放其文件或资源固定句柄。
        let owner = Arc::new(PredecessorRestorationPrivateOwner {
            store,
            reservation: Mutex::new(Some(reservation)),
        });
        restoration.private_owner = Some(Arc::clone(&owner));
        // Retain ownership even when either real atomic-write observation or immediate readback is unknown.
        // 即使真实原子写入观测或即时回读未知，也保留所有权。
        let mut reservation = owner
            .reservation
            .lock()
            .map_err(|_| error("Private predecessor reservation lock is poisoned."))?;
        owner
            .store
            .persist_predecessor_handoff_reservation(
                reservation
                    .as_mut()
                    .ok_or_else(|| error("Original private predecessor reservation is missing."))?,
            )
            .map_err(|failure| error(failure.to_string()))
    }
}

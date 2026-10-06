//! Explicit preparation, recovery and handoff checkpoints never execute plugin callbacks.
//! 显式准备、恢复及交接检查点绝不执行插件回调。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Arm a test-only original post-write observation loss; returned scope resets it without production controls.
    /// 启用仅测试的原写入后观测丢失；返回作用域负责重置，不提供生产开关。
    #[cfg(test)]
    pub(crate) fn lose_target_completion_observation_once(&self, request: Uuid) -> impl Drop {
        completion_observation_fault::arm(&self.root, request)
    }
    /// Inspect transaction's still-prepared target without publishing it or running plugin code.
    /// 检查 transaction 仍处于准备阶段的目标，不发布它，也不执行插件代码。
    /// Returns a view borrowing the exclusive owner after checking its store, indexed record, predecessor and pins.
    /// 在检查存储、已索引记录、前驱及固定资源后，返回借用排他所有者的视图。
    /// Failed checks never repair records, replace the active selection or authorize ordinary runtime calls.
    /// 检查失败绝不修复记录、替换有效选择或授权普通运行时调用。
    ///
    /// The view must leave use before the same owner can publish.
    /// 同一所有者发布前，此视图必须不再使用。
    /// ```compile_fail,E0502
    /// use vulcan_plugin_runtime::{VulcanPluginActivationTransaction, VulcanPluginGenerationStore};
    /// // Attempt to commit transaction while its candidate remains borrowed; this must not compile.
    /// // 在候选仍被借用时尝试提交 transaction；此代码必须无法编译。
    /// fn commit_borrowed(store: &VulcanPluginGenerationStore, transaction: &mut VulcanPluginActivationTransaction) {
    ///     // Candidate retains the original immutable transaction borrow.
    ///     // 候选保留原不可变事务借用。
    ///     let candidate = store.inspect_activation_candidate(transaction).expect("inspect");
    ///     store.commit_activation_transaction(transaction).expect("commit");
    ///     assert_eq!(candidate.package().generation, 0);
    /// }
    /// ```
    pub fn inspect_activation_candidate<'transaction>(
        &self,
        transaction: &'transaction VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationCandidate<'transaction>> {
        // Reject cross-root owners before inspecting or acquiring any foreign namespace lock.
        // 在检查或获取任何外部命名空间锁前拒绝跨根目录所有者。
        if transaction.store_root != self.root {
            return Err(Error::new(
                Code::Conflict,
                "Activation candidate belongs to another store.",
            ));
        }
        // Keep the predecessor and complete physical inspection under the same cooperating-writer fence.
        // 在同一协作写入者屏障内保持前驱及完整物理检查。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        self.check_transaction_owner(transaction)?;
        if transaction.record.phase != Phase::Prepared {
            return Err(Error::new(
                Code::Conflict,
                "Only a prepared activation request may inspect an unpublished candidate.",
            ));
        }
        if self.read_activation(&transaction.record.target.plugin_id)?
            != transaction.record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Activation candidate predecessor changed.",
            ));
        }
        // Revalidate actual contents, approved permissions, the complete configuration and staged credentials.
        // 复核实际内容、已批准权限、完整配置及已暂存凭据。
        let (generation, credentials) = self.pin_transaction_target(&transaction.record)?;
        // Store construction fixes this exact parent as the user catalog; no alternate root is probed.
        // 存储构造将此精确父目录固定为用户目录；不探测备用根目录。
        let catalog_root = self.root.parent().ok_or_else(|| {
            Error::invalid("Activation candidate store has no user catalog parent.")
        })?;
        // Readiness must not inherit a manifest's default-enabled choice.
        // 就绪检查不得继承清单的默认启用选择。
        let disabled =
            std::collections::BTreeSet::from([transaction.record.target.plugin_id.clone()]);
        // No enabled override may reverse the explicit candidate-only disabled policy.
        // 不允许启用覆盖反转候选专属的显式停用策略。
        let enabled = std::collections::BTreeSet::new();
        // Shared projection gives future readiness code the same authorities as eventual public discovery.
        // 共享投影为后续就绪代码提供与最终公开发现相同的权威。
        let mut package = crate::catalog::load_pinned_package(
            catalog_root,
            &transaction.record.target,
            generation,
            credentials,
            &disabled,
            &enabled,
        )
        .map_err(Error::invalid)?;
        if package.availability != crate::VulcanPluginAvailability::Disabled {
            return Err(Error::invalid(
                "Activation candidate package is unavailable.",
            ));
        }
        package.origin = crate::VulcanPluginPackageOrigin::UserInstalled;
        // Inspection is not runtime admission, even for a valid executable manifest.
        // 即使清单具有有效执行声明，检查也不是运行时入场。
        package.runtime_execution_available = false;
        // Retain the original exclusive request lock, so even an older cooperating reader cannot recover unfinished work.
        // 保留原排他请求锁，因此即使较旧协作读者也不能恢复未完成工作。
        let readiness = Arc::clone(&transaction._lease);
        Ok(VulcanPluginActivationCandidate {
            transaction,
            package,
            catalog_root: catalog_root.to_path_buf(),
            readiness,
        })
    }

    /// Persist `request_id`'s complete configured successor to `expected_revision` and return its exclusive owner.
    /// 持久保存 `request_id` 相对于 `expected_revision` 的完整配置后继，并返回其排他所有者。
    /// `generation`, exact `permissions`, staged `credentials` and `configuration` define the candidate; readiness is host-owned.
    /// `generation`、精确 `permissions`、已暂存 `credentials` 及 `configuration` 定义候选；就绪由宿主持有。
    /// On error retain the original caller-issued ID and query it; no callback or activation is performed here.
    /// 失败时保留调用方签发的原标识并查询；此处不执行回调或激活。
    pub fn begin_activation_transaction(
        &self,
        request_id: Uuid,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        mut permissions: Vec<String>,
        credentials: VulcanPluginCredentialRevision,
        configuration: VulcanPluginConfigurationSelection,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationTransaction> {
        if request_id.is_nil() || generation.store_root != self.root {
            return Err(Error::invalid(
                "Invalid activation request identity or owning store.",
            ));
        }
        // Preparation and pointer publication share the namespace fence used by all activation writers.
        // 准备及指针发布共用全部激活写入者使用的命名空间屏障。
        let plugin_id = &generation.identity.plugin_id;
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        self.ensure_no_pending_activation_locked(plugin_id)?;
        if self
            .read_transaction_locked(plugin_id, request_id)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Activation request already exists; resume its original record.",
            ));
        }
        // Capture the entire predecessor so recovery cannot equate equal revisions with different authorities.
        // 捕获完整前驱，使恢复不能将相同修订误认为相同权威。
        let predecessor = self.read_activation(plugin_id)?;
        if predecessor.as_ref().map_or(0, |record| record.revision) != expected_revision {
            return Err(Error::new(
                Code::Conflict,
                "Activation transaction predecessor changed.",
            ));
        }
        // Checked succession prevents revision wraparound before any journal mutation.
        // 检查后继防止修订溢出，且发生在任何日志变更之前。
        let revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin activation revision is exhausted."))?;
        // Reinspect the exact physical target before persisting any intent.
        // 持久保存任何意图前，重新检查精确物理目标。
        let verified = self.pin_locked(&generation.identity)?;
        permissions.sort();
        if permissions != verified.required_permissions {
            return Err(Error::invalid(
                "Activation transaction requires the complete exact permission set.",
            ));
        }
        verified.validate_configuration(&configuration)?;
        // The staged credential namespace must remain owned throughout candidate readiness.
        // 已暂存凭据命名空间须在候选就绪全程保持所有权。
        let credential_lease = self.pin_credentials_locked(&verified, &credentials)?;
        // The caller-owned UUID is known even if either following atomic publication has an uncertain result.
        // 即使后续任一原子发布结果不确定，调用方持有的 UUID 仍然已知。
        let record = VulcanPluginActivationTransactionRecord {
            schema_version: TRANSACTION_RECORD_VERSION,
            request_id,
            predecessor,
            target: VulcanPluginActivation {
                schema_version: ACTIVATION_RECORD_VERSION,
                plugin_id: plugin_id.clone(),
                revision,
                generation: Some(generation.identity.clone()),
                approved_permissions: permissions,
                credential_revision: credentials,
                configuration,
            },
            phase: Phase::Prepared,
            preparation: None,
        };
        self.transaction_directory(plugin_id, true)?;
        // The real exclusive request lock survives the short namespace operation.
        // 实际请求排他锁在短时命名空间操作结束后仍持续持有。
        let lease = self.transaction_lease(plugin_id, request_id)?;
        self.write_transaction_locked(&record)?;
        self.index_transaction_locked(&record)?;
        Ok(VulcanPluginActivationTransaction {
            store_root: self.root.clone(),
            record,
            _lease: Arc::new(lease),
            generation: verified,
            credentials: credential_lease,
        })
    }

    /// Resume the exact unfinished `plugin_id`/`request_id` after its old owner has released the real request lock.
    /// 原所有者释放实际请求锁后，恢复精确未完成的 `plugin_id`／`request_id`。
    /// Return actual candidate leases without publishing a selection or repeating any host callback.
    /// 返回实际候选租约，不发布选择或重复任何宿主回调。
    pub fn resume_activation_transaction(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationTransaction> {
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // Public resumption cannot replace a still-owned original private predecessor instance with a new file owner.
        // 公开恢复不能以新文件所有者替换仍自有的原私有前驱实例。
        self.ensure_no_predecessor_handoff_locked(plugin_id)?;
        // Retain the exact original durable request rather than reconstructing its intent.
        // 保留精确原持久请求，而不重建其意图。
        let record = self
            .read_transaction_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original activation transaction is missing."))?;
        if record.phase.terminal() {
            return Err(Error::invalid(
                "Terminal activation requests cannot be resumed.",
            ));
        }
        // Ownership is taken before repairing an interrupted index publication.
        // 修复被中断的索引发布前先取得所有权。
        let lease = self.transaction_lease(plugin_id, request_id)?;
        // The declared index alone identifies the current request authority.
        // 仅声明索引标识当前请求权威。
        let indexed = self.indexed_request_locked(plugin_id)?;
        // Request identity must match the existing index before publication can proceed.
        // 发布继续前，请求身份必须匹配既有索引。
        let owns_index = indexed
            .as_ref()
            .is_some_and(|current| current.request_id() == request_id);
        if !owns_index
            && (record.phase != Phase::Prepared
                || indexed.is_some_and(|current| !current.phase().terminal()))
        {
            return Err(Error::new(
                Code::Busy,
                "Another activation transaction owns this namespace.",
            ));
        }
        // An unindexed preparation may be adopted only while its complete predecessor remains current.
        // 未索引准备仅在完整前驱仍为当前选择时才可接管。
        let current = self.read_activation(plugin_id)?;
        // Published records require their target; only pre-publication phases may still observe the predecessor.
        // 已发布记录要求目标存在；仅发布前阶段仍可观察前驱。
        let predecessor_matches = record.phase != Phase::Published && current == record.predecessor;
        // The target cannot legitimately appear before this exact indexed commit intent.
        // 此精确已索引提交意图之前，目标不可能合法出现。
        let target_matches = owns_index
            && record.phase != Phase::Prepared
            && current.as_ref() == Some(&record.target);
        if !predecessor_matches && !target_matches {
            return Err(Error::new(
                Code::Conflict,
                "Activation differs from the original transaction's predecessor and target.",
            ));
        }
        // Both leases come from the same revalidated target selection.
        // 两个租约来自相同重新校验的目标选择。
        let (generation, credentials) = self.pin_transaction_target(&record)?;
        if !owns_index {
            self.index_transaction_locked(&record)?;
        }
        Ok(VulcanPluginActivationTransaction {
            store_root: self.root.clone(),
            record,
            _lease: Arc::new(lease),
            generation,
            credentials,
        })
    }

    /// Explicitly publish `transaction` after the host has completed candidate readiness; return its exact target.
    /// 宿主完成候选就绪后明确发布 `transaction`；返回精确目标。
    /// Retries reconcile the original predecessor or target without incrementing twice; route handoff is still pending.
    /// 重试核对原前驱或目标，不重复递增；路由交接仍待完成。
    pub fn commit_activation_transaction(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<VulcanPluginActivation> {
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        self.check_transaction_owner(transaction)?;
        // A lost observer cannot publish while the original readiness execution still owns real resources.
        // 原就绪执行仍拥有实际资源时，丢失观测者不能导致发布。
        Self::require_readiness_retired(transaction)?;
        if transaction.record.phase.terminal() {
            return Err(Error::invalid(
                "Terminal activation requests cannot be committed.",
            ));
        }
        // A returned timeout or lost observer cannot authorize publication of uncertain installation effects.
        // 超时返回或观测者丢失不能授权发布不确定安装副作用。
        self.require_installation_succeeded(&transaction.record)?;
        // Initialization and enable effects are equally durable publication prerequisites.
        // 初始化及启用副作用同样属于持久发布前置条件。
        self.require_effects_succeeded(&transaction.record)?;
        // Compare the complete current selection with the original transaction authorities.
        // 将完整当前选择与原事务权威比较。
        let current = self.read_activation(&transaction.record.target.plugin_id)?;
        if current.as_ref() == Some(&transaction.record.target) {
            if transaction.record.phase == Phase::Prepared {
                return Err(Error::new(
                    Code::Conflict,
                    "Target appeared before this transaction's commit intent.",
                ));
            }
        } else if current == transaction.record.predecessor
            && transaction.record.phase != Phase::Published
        {
            // Revalidate the exact package, schema and credentials immediately before publication.
            // 发布前立即复核精确插件包、模式及凭据。
            let _verified = self.pin_transaction_target(&transaction.record)?;
            if transaction.record.phase == Phase::Prepared {
                self.checkpoint_transaction(transaction, Phase::Committing)?;
            }
            self.write_activation(&transaction.record.target)?;
        } else {
            return Err(Error::new(
                Code::Conflict,
                "Activation transaction no longer matches its exact predecessor or target.",
            ));
        }
        if transaction.record.phase != Phase::Published {
            self.checkpoint_transaction(transaction, Phase::Published)?;
        }
        Ok(transaction.record.target.clone())
    }

    /// Acknowledge the host's completed route handoff for `transaction`; return success without changing activation.
    /// 确认 `transaction` 的宿主路由交接已完成；返回成功，不修改激活。
    /// The host must retain real old route owners until they drain; the journal does not fabricate that acknowledgement.
    /// 宿主须保留实际旧路由所有者直到排空；日志不会代为伪造此确认。
    pub fn complete_activation_transaction(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        self.check_transaction_owner(transaction)?;
        // A private target attempt requires atomic transfer of its same live owners before journal completion.
        // 私有目标尝试要求在日志完成之前原子转交其相同存活所有者。
        if self.read_target_hook_locked(&transaction.record)?.is_some()
            || self.read_target_mcp_locked(&transaction.record)?.is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Private target completion requires the combined original resource handoff.",
            ));
        }
        if transaction.record.phase != Phase::Published
            || self
                .read_activation(&transaction.record.target.plugin_id)?
                .as_ref()
                != Some(&transaction.record.target)
        {
            return Err(Error::new(
                Code::Conflict,
                "Only the exact published transaction can complete its route handoff.",
            ));
        }
        self.checkpoint_transaction(transaction, Phase::Completed)
    }

    /// Complete the original target handoff and reconcile uncertain writes against its exact completed record.
    /// 完成原目标交接，并按精确完成记录核对不确定写入。
    /// transaction retains the original file; only an identical indexed Completed target confirms transfer.
    /// transaction 保留原文件；仅完全相同且已索引的 Completed 目标确认转交。
    /// Return confirmed completion or retain the previous observation without replaying any initialization.
    /// 返回已确认完成，或保留先前观察，不重放任何初始化。
    pub(crate) fn complete_activation_target_handoff_owned(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        expected_hook: Option<(u64, &str, &str, &str)>,
        expected_mcp: Option<&crate::VulcanPluginMcpTargetHandoffEvidence>,
    ) -> VulcanPluginStoreResult<()> {
        // One namespace fence spans the write and its exact uncertain-outcome readback.
        // 单个命名空间屏障覆盖写入及其精确不确定结果回读。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        self.require_target_hook_success_locked(&transaction.record, expected_hook)?;
        self.require_target_mcp_ready_locked(&transaction.record, expected_mcp)?;
        // Derive the sole admissible receipt from the original full record before any mutation.
        // 在任何变更前从原完整记录派生唯一可接受回执。
        let mut completed = transaction.record.clone();
        completed.phase = Phase::Completed;
        if self.read_activation(&completed.target.plugin_id)?.as_ref() != Some(&completed.target) {
            return Err(Error::new(
                Code::Conflict,
                "Original target changed before completion.",
            ));
        }
        if self
            .indexed_transaction_locked(&completed.target.plugin_id)?
            .as_ref()
            == Some(&completed)
        {
            transaction.record = completed;
            return Ok(());
        }
        self.check_transaction_owner(transaction)?;
        if transaction.record.phase != Phase::Published {
            return Err(Error::new(
                Code::Conflict,
                "Only the exact published original can complete.",
            ));
        }
        // Readback can confirm an atomic replacement even when the writer reported an I/O failure.
        // 即使写入器报告 I/O 失败，回读仍可确认原子替换。
        // The real writer always executes before any test-only observation loss can activate.
        // 真实写入器始终在任何仅测试观测丢失可启用之前执行。
        let write_result = self.write_transaction_locked(&completed);
        #[cfg(test)]
        let observation_lost = completion_observation_fault::consume(
            &self.root,
            completed.request_id,
            write_result.is_ok(),
        );
        #[cfg(test)]
        let write_result = if observation_lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of successful Completed write observation.",
            ))
        } else {
            write_result
        };
        if let Err(write_error) = write_result {
            // Only this immediate indexed observation may be unavailable; canonical request inspection remains unchanged.
            // 仅此次即时索引观测可不可用；规范请求检查保持不变。
            let read = || self.indexed_transaction_locked(&completed.target.plugin_id);
            #[cfg(test)]
            let readback =
                completion_observation_fault::immediate_readback(observation_lost, read)?;
            #[cfg(not(test))]
            let readback = read()?;
            if readback.as_ref() != Some(&completed) {
                return Err(write_error);
            }
        }
        transaction.record = completed;
        Ok(())
    }

    /// Abandon `transaction` only while its original predecessor is unchanged; published choices never auto-rollback.
    /// 仅在原前驱未变时放弃 `transaction`；已发布选择绝不自动回滚。
    pub fn abort_activation_transaction(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, false)?;
        self.check_transaction_owner(transaction)?;
        // Explicit abort must wait for actual readiness retirement, not merely a returned timeout.
        // 明确中止必须等待实际就绪退役，而不只是超时返回。
        Self::require_readiness_retired(transaction)?;
        if !matches!(
            transaction.record.phase,
            Phase::Prepared | Phase::Committing
        ) || self.read_activation(&transaction.record.target.plugin_id)?
            != transaction.record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Activation cannot be aborted after publication or predecessor change.",
            ));
        }
        self.checkpoint_transaction(transaction, Phase::Aborted)
    }

    /// Abandon original `plugin_id`/`request_id` after its owner exits, even when its candidate can no longer be pinned.
    /// 原所有者退出后放弃原 `plugin_id`／`request_id`，即使候选已无法固定。
    /// Create an abort only while the exact predecessor is unchanged; an existing aborted receipt remains queryable later.
    /// 仅精确前驱未变时创建中止；既有中止回执之后仍可查询。
    /// Return the retained receipt without invoking cleanup or callbacks.
    /// 返回保留回执，不调用清理或回调。
    pub fn abort_activation_request(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationTransactionRecord> {
        // Cancellation takes the same locks but intentionally does not require a valid candidate package.
        // 取消取得相同锁，但明确不要求候选插件包仍有效。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // Public abort cannot clear the original resource reservation, including an already persisted Aborted receipt.
        // 公开中止不能清除原资源保留权，包括已持久化的 Aborted 回执。
        self.ensure_no_predecessor_handoff_locked(plugin_id)?;
        // Retain the exact original durable request rather than reconstructing its intent.
        // 保留精确原持久请求，而不重建其意图。
        let mut record = self
            .read_transaction_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original activation transaction is missing."))?;
        // Cancellation also requires exclusive ownership of the original request.
        // 取消同样要求独占原请求。
        let _lease = self.transaction_lease(plugin_id, request_id)?;
        // The declared index alone identifies the current request authority.
        // 仅声明索引标识当前请求权威。
        let indexed = self.indexed_request_locked(plugin_id)?;
        // An already persisted abort is an idempotent receipt, including failure while writing its missing index.
        // 已持久化中止是幂等回执，包括写入缺失索引期间发生失败的情况。
        if record.phase == Phase::Aborted {
            if indexed.is_none() {
                self.index_transaction_locked(&record)?;
            }
            return Ok(record);
        }
        // Request identity must match the existing index before publication can proceed.
        // 发布继续前，请求身份必须匹配既有索引。
        let owns_index = indexed
            .as_ref()
            .is_some_and(|current| current.request_id() == request_id);
        if !matches!(record.phase, Phase::Prepared | Phase::Committing)
            || (!owns_index && record.phase != Phase::Prepared)
            || self.read_activation(plugin_id)? != record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Only an unpublished original request with an unchanged predecessor can be abandoned.",
            ));
        }
        // Aborting preserves the complete recovery record and does not remove any package or secure-store entry.
        // 中止保留完整恢复记录，不移除任何插件包或安全存储条目。
        record.phase = Phase::Aborted;
        self.write_transaction_locked(&record)?;
        if indexed.is_none() {
            self.index_transaction_locked(&record)?;
        }
        Ok(record)
    }

    /// Acquire the actual exclusive lock for `plugin_id`/`request_id`; it is independent from the short namespace fence.
    /// 取得 `plugin_id`／`request_id` 的实际排他锁；它独立于短时命名空间屏障。
    pub(super) fn transaction_lease(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<File> {
        files::lock(&self.transaction_lease_path(plugin_id, request_id), false)
    }

    /// Resolve the sole original request lease path for exact `plugin_id` and `request_id`, without opening it.
    /// 为精确 `plugin_id`、`request_id` 解析唯一原请求租约路径，不打开文件。
    pub(in crate::generation_store) fn transaction_lease_path(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> PathBuf {
        self.root
            .join("locks")
            .join(format!("{plugin_id}.transaction.{request_id}.lock"))
    }

    /// Inspect an existing terminal original lease under the caller's namespace fence; never create a replacement file.
    /// 在调用方命名空间屏障下检查既有终态原租约；绝不创建替换文件。
    /// Returns true for an actual still-live owner, false after real retirement, and an error for missing or invalid storage.
    /// 实际所有者仍存活返回 true，真实退役后返回 false，存储缺失或无效返回错误。
    pub(super) fn terminal_transaction_owner_is_live_locked(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<bool> {
        // Both original acquisition and read-only retirement observation derive this path from the same definition.
        // 原取得与只读退役观测都从相同定义派生此路径。
        let path = self.transaction_lease_path(plugin_id, request_id);
        if !files::exists(&path, false)? {
            return Err(Error::invalid(
                "Terminal original activation lease file disappeared.",
            ));
        }
        // Read-only opening cannot truncate, create or repair a missing original physical owner.
        // 只读打开不能截断、创建或修复缺失原物理所有者。
        let file = File::open(&path)
            .map_err(|error| Error::io("Open terminal activation owner lease", error))?;
        files::exists(&path, false)?;
        match file.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(error)) => {
                Err(Error::io("Inspect terminal activation owner lease", error))
            }
        }
    }

    /// Require transaction to be the sole owner of its original request lock before mutation.
    /// 变更前要求 transaction 为原请求锁的唯一所有者。
    /// The caller's exclusive mutable borrow excludes new views; another readiness clone already raises this count.
    /// 调用方排他可变借用排除新视图；其他就绪克隆已经使此计数增加。
    pub(super) fn require_readiness_retired(
        transaction: &mut VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        if Arc::strong_count(&transaction._lease) != 1 {
            return Err(Error::new(
                Code::Busy,
                "Activation readiness still owns the original request; inspect and drain its original calls.",
            ));
        }
        Ok(())
    }

    /// Revalidate the target in `record` and return its actual package and credential owners under the namespace fence.
    /// 在命名空间屏障内复核 `record` 的目标，并返回实际插件包及凭据所有者。
    pub(super) fn pin_transaction_target(
        &self,
        record: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<(PinnedVulcanPluginGeneration, PinnedVulcanPluginCredentials)>
    {
        // Use the exact physical package identity, including its inspected content fingerprint.
        // 使用精确物理插件包身份，包含已检查内容指纹。
        let identity =
            record.target.generation.as_ref().ok_or_else(|| {
                Error::invalid("Activation transaction target package is missing.")
            })?;
        // Retain actual package ownership after verifying its immutable contents.
        // 校验不可变内容后保留实际插件包所有权。
        let generation = self.pin_locked(identity)?;
        if record.target.approved_permissions() != generation.required_permissions.as_slice() {
            return Err(Error::invalid(
                "Activation transaction permissions differ from the exact package.",
            ));
        }
        // The target's explicit configuration is the only document validated for this request.
        // 目标的显式配置是此请求校验的唯一文档。
        generation.validate_configuration(record.target.configuration())?;
        // The explicit staged credential revision belongs to this target selection.
        // 显式已暂存凭据修订属于此目标选择。
        let revision = record.target.credential_revision();
        // Retain real credential ownership for the same verified package.
        // 为相同已验证插件包保留实际凭据所有权。
        let credentials = self.pin_credentials_locked(&generation, revision)?;
        if record.preparation.is_some()
            && credentials.record().activation_request() != Some(record.request_id)
        {
            return Err(Error::new(
                Code::Conflict,
                "Staged credentials do not belong to this original preparation.",
            ));
        }
        Ok((generation, credentials))
    }

    /// Compare `transaction` with its exact indexed disk record; caller already retains the namespace fence.
    /// 将 `transaction` 与精确已索引磁盘记录比较；调用方已保留命名空间屏障。
    pub(super) fn check_transaction_owner(
        &self,
        transaction: &VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        if transaction.store_root != self.root
            || self
                .indexed_transaction_locked(&transaction.record.target.plugin_id)?
                .as_ref()
                != Some(&transaction.record)
        {
            return Err(Error::new(
                Code::Conflict,
                "Activation transaction changed; inspect and resume the original request.",
            ));
        }
        Ok(())
    }

    /// Validate `transaction` as the exclusive owner of the exact published activation; return no mutation.
    /// 校验 `transaction` 为精确已发布激活的排他所有者；不进行变更。
    pub(crate) fn validate_published_activation_owner(
        &self,
        transaction: &VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        // Original ownership and target readback share one namespace observation.
        // 原所有权及目标回读共享同一次命名空间观察。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, true)?;
        self.check_transaction_owner(transaction)?;
        if transaction.record.phase != Phase::Published
            || self
                .read_activation(&transaction.record.target.plugin_id)?
                .as_ref()
                != Some(&transaction.record.target)
        {
            return Err(Error::new(
                Code::Conflict,
                "Recovery publication requires the exact published original activation.",
            ));
        }
        // Revalidate actual immutable contents, full configuration and original-bound credentials before handoff.
        // 在交接前重新校验实际不可变内容、完整配置及绑定原请求的凭据。
        let _target = self.pin_transaction_target(&transaction.record)?;
        Ok(())
    }

    /// Validate the retained original target before transfer, including exact Completed readback after uncertainty.
    /// 转交前校验保留原目标，包括不确定结果后的精确 Completed 回读。
    /// transaction must retain Published observation; returns only after pinning its full actual target again.
    /// transaction 必须保留 Published 观察；再次固定其完整实际目标后才返回。
    pub(crate) fn validate_published_target_handoff_owner(
        &self,
        transaction: &VulcanPluginActivationTransaction,
    ) -> VulcanPluginStoreResult<()> {
        // Only these two exact records belong to the still-retained original handoff.
        // 仅这两个精确记录属于仍被保留的原交接。
        let _namespace = self.namespace_lock(&transaction.record.target.plugin_id, true)?;
        let mut completed = transaction.record.clone();
        completed.phase = Phase::Completed;
        let indexed = self.indexed_transaction_locked(&transaction.record.target.plugin_id)?;
        if transaction.store_root != self.root
            || transaction.record.phase != Phase::Published
            || (indexed.as_ref() != Some(&transaction.record)
                && indexed.as_ref() != Some(&completed))
            || self
                .read_activation(&transaction.record.target.plugin_id)?
                .as_ref()
                != Some(&transaction.record.target)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original published target handoff ownership changed.",
            ));
        }
        let _target = self.pin_transaction_target(&transaction.record)?;
        Ok(())
    }

    /// Persist `phase` for the original `transaction`, updating its observation only after confirmed publication.
    /// 为原 `transaction` 持久保存 `phase`，仅在发布确认后更新其观察。
    fn checkpoint_transaction(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        phase: Phase,
    ) -> VulcanPluginStoreResult<()> {
        // Change only the original request checkpoint, retaining both complete selections.
        // 仅改变原请求检查点，保留两个完整选择。
        let mut next = transaction.record.clone();
        next.phase = phase;
        self.write_transaction_locked(&next)?;
        // Lose test observation only after the real original checkpoint was successfully written.
        // 仅在真实原检查点成功写入之后丢失测试观测。
        #[cfg(test)]
        super::owned_reconciliation::checkpoint_observation_after_write(&self.root, &next)?;
        transaction.record = next;
        Ok(())
    }
}

/// Test-only scoped observation loss never changes persisted bytes or subsequent canonical reads.
/// 仅测试的作用域观测丢失绝不更改持久字节或后续规范读取。
#[cfg(test)]
mod completion_observation_fault {
    use super::*;

    std::thread_local! {
        /// One original root/request on this thread may lose its next successful completion observation.
        /// 此线程单个原根目录／请求可丢失下一次成功完成观测。
        static ARMED: std::cell::RefCell<Option<(PathBuf, Uuid)>> = const { std::cell::RefCell::new(None) };
    }

    /// Scope resets unconsumed faults on the same test thread, including assertion unwinding.
    /// 作用域在相同测试线程重置未消费故障，包括断言展开。
    pub(super) struct Scope {
        /// Exact original store and request exclude other tests and unrelated transactions.
        /// 精确原存储及请求排除其他测试与无关事务。
        key: (PathBuf, Uuid),
        /// Thread affinity prevents moving cleanup to another thread-local slot.
        /// 线程绑定防止将清理移动至另一线程局部槽。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Clear only this scope's still-armed original fault; no inputs, I/O or persisted mutation.
        /// 仅清除此作用域仍已启用原故障；无输入、I/O 或持久变更。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // Retain other scoped identities rather than clearing an unrelated fault.
                // 保留其他作用域身份，不清除无关故障。
                let mut armed = armed.borrow_mut();
                if armed.as_ref() == Some(&self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Arm one exact original completion; return its non-Send cleanup scope and reject overlapping scopes.
    /// 启用单个精确原完成；返回不可跨线程清理作用域，并拒绝重叠作用域。
    pub(super) fn arm(root: &Path, request: Uuid) -> Scope {
        // Use the existing store identity without opening another owner or assigning a request UUID.
        // 使用既有存储身份，不打开另一所有者或分配请求 UUID。
        let key = (root.to_path_buf(), request);
        ARMED.with(|armed| {
            // A test must explicitly retire its previous scoped fault before arming another.
            // 测试必须在启用另一故障之前明确退役先前作用域故障。
            let mut armed = armed.borrow_mut();
            assert!(
                armed.is_none(),
                "completion observation fault scope overlaps"
            );
            *armed = Some(key.clone());
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Consume only after the exact original Completed write succeeded, leaving actual bytes intact.
    /// 仅在精确原 Completed 写入成功之后消费，保持实际字节不变。
    pub(super) fn consume(root: &Path, request: Uuid, written: bool) -> bool {
        ARMED.with(|armed| {
            // Failed real writes never pretend that Completed bytes were persisted.
            // 真实写入失败绝不假称已持久保存 Completed 字节。
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

    /// Return unavailable only for this immediate indexed readback; later canonical reads execute normally.
    /// 仅为此次即时索引回读返回不可用；后续规范读取正常执行。
    pub(super) fn immediate_readback<T>(
        lost: bool,
        read: impl FnOnce() -> VulcanPluginStoreResult<T>,
    ) -> VulcanPluginStoreResult<T> {
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of immediate Completed indexed readback.",
            ))
        } else {
            read()
        }
    }
}

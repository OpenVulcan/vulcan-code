//! Explicit preparation, recovery and handoff checkpoints never execute plugin callbacks.
//! 显式准备、恢复及交接检查点绝不执行插件回调。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
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
            Some(credentials),
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
                schema_version: CONFIGURED_ACTIVATION_VERSION,
                plugin_id: plugin_id.clone(),
                revision,
                generation: Some(generation.identity.clone()),
                approved_permissions: Some(permissions),
                credential_revision: Some(credentials),
                configuration: Some(configuration),
            },
            phase: Phase::Prepared,
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
        let indexed = self.indexed_transaction_locked(plugin_id)?;
        // Request identity must match the existing index before publication can proceed.
        // 发布继续前，请求身份必须匹配既有索引。
        let owns_index = indexed
            .as_ref()
            .is_some_and(|current| current.request_id == request_id);
        if !owns_index
            && (record.phase != Phase::Prepared
                || indexed.is_some_and(|current| !current.phase.terminal()))
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
        let indexed = self.indexed_transaction_locked(plugin_id)?;
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
            .is_some_and(|current| current.request_id == request_id);
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
    fn transaction_lease(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<File> {
        files::lock(
            &self
                .root
                .join("locks")
                .join(format!("{plugin_id}.transaction.{request_id}.lock")),
            false,
        )
    }

    /// Require transaction to be the sole owner of its original request lock before mutation.
    /// 变更前要求 transaction 为原请求锁的唯一所有者。
    /// The caller's exclusive mutable borrow excludes new views; another readiness clone already raises this count.
    /// 调用方排他可变借用排除新视图；其他就绪克隆已经使此计数增加。
    fn require_readiness_retired(
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
    fn pin_transaction_target(
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
        if record.target.approved_permissions() != Some(generation.required_permissions.as_slice())
        {
            return Err(Error::invalid(
                "Activation transaction permissions differ from the exact package.",
            ));
        }
        // The target's explicit configuration is the only document validated for this request.
        // 目标的显式配置是此请求校验的唯一文档。
        let configuration = record
            .target
            .configuration()
            .ok_or_else(|| Error::invalid("Activation transaction configuration is missing."))?;
        generation.validate_configuration(configuration)?;
        // The explicit staged credential revision belongs to this target selection.
        // 显式已暂存凭据修订属于此目标选择。
        let revision = record
            .target
            .credential_revision()
            .ok_or_else(|| Error::invalid("Activation transaction credentials are missing."))?;
        // Retain real credential ownership for the same verified package.
        // 为相同已验证插件包保留实际凭据所有权。
        let credentials = self.pin_credentials_locked(&generation, revision)?;
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
        transaction.record = next;
        Ok(())
    }
}

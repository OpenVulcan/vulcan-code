//! Original-request ownership spans early allocation, immutable copy and transition to configured readiness.
//! 原请求所有权覆盖早期分配、不可变复制及向已配置就绪的转换。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Begin an approved update with its original plugin policy before any package or credential mutation.
    /// 在任何插件包或凭据变更前，以原插件策略开始已批准更新。
    /// Arguments bind the original request, source digest, full predecessor, permissions and user policy; return its exclusive owner.
    /// 参数绑定原请求、来源摘要、完整前驱、权限及用户策略；返回其排他所有者。
    pub fn begin_activation_update_preparation(
        &self,
        request_id: Uuid,
        source: &Path,
        expected_content_fingerprint: &str,
        expected_predecessor: Option<&VulcanPluginActivation>,
        permissions: Vec<String>,
        update_policy: VulcanPluginUpdatePolicy,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparation> {
        self.begin_preparation_with_policy(
            request_id,
            source,
            expected_content_fingerprint,
            expected_predecessor,
            permissions,
            Some(update_policy),
        )
    }

    /// Share exact allocation and locking for declared legacy preparation or policy-bearing update intent.
    /// 为声明的旧准备或携带策略的更新意图共用精确分配及锁定。
    /// The optional policy selects its explicit journal version; all other arguments keep the same authority checks.
    /// 可选策略选择明确日志版本；其他参数保持相同权威检查。
    fn begin_preparation_with_policy(
        &self,
        request_id: Uuid,
        source: &Path,
        expected_content_fingerprint: &str,
        expected_predecessor: Option<&VulcanPluginActivation>,
        mut permissions: Vec<String>,
        update_policy: Option<VulcanPluginUpdatePolicy>,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparation> {
        if request_id.is_nil() {
            return Err(Error::invalid("Activation request identity cannot be nil."));
        }
        // Allocate identities in memory only; no package directory or credential account is created.
        // 仅在内存中分配身份；不创建插件包目录或凭据账户。
        let generation = self.plan_generation(source, expected_content_fingerprint)?;
        // This read validates approval only; configuration is later selected from the immutable copy.
        // 此读取仅校验批准；配置稍后从不可变副本选择。
        let inspected = crate::inspect_user_plugin_package(source).map_err(Error::invalid)?;
        if inspected.id != generation.plugin_id
            || inspected.version.as_deref() != Some(generation.version.as_str())
            || inspected.content_fingerprint != generation.content_fingerprint
        {
            return Err(Error::invalid(
                "Source changed before preparation intent was recorded.",
            ));
        }
        permissions.sort();
        if permissions
            != crate::runtime::plugin_required_permissions(&inspected)
                .map_err(|error| Error::invalid(error.to_string()))?
        {
            return Err(Error::invalid(
                "Preparation requires exact complete approved permissions.",
            ));
        }
        // Serialize every writer against the actual current selection before allocating durable resources.
        // 分配持久资源前，按实际当前选择对全部写入者排序。
        let _namespace = self.namespace_lock(&generation.plugin_id, false)?;
        self.ensure_no_pending_activation_locked(&generation.plugin_id)?;
        if self
            .read_request_locked(&generation.plugin_id, request_id)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original activation request already exists.",
            ));
        }
        // Compare the complete authority rather than its revision alone.
        // 比较完整权威，而非仅比较修订。
        let predecessor = self.read_activation(&generation.plugin_id)?;
        if predecessor.as_ref() != expected_predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Preparation predecessor changed before intent publication.",
            ));
        }
        // The one successor is fixed before journal publication and cannot wrap.
        // 唯一后继在日志发布前固定，不能溢出。
        let activation_revision = predecessor
            .as_ref()
            .map_or(0, |record| record.revision)
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin activation revision is exhausted."))?;
        // Both physical resource identities are recoverable before their first allocation or secure write.
        // 两个物理资源身份在首次分配或安全写入之前即可恢复。
        let record = VulcanPluginActivationPreparationRecord {
            schema_version: if update_policy.is_some() {
                UPDATE_RECORD_VERSION
            } else {
                PREPARATION_RECORD_VERSION
            },
            request_id,
            predecessor,
            preparation: VulcanPluginActivationPreparationIntent {
                generation,
                credential_revision: VulcanPluginCredentialRevision::allocate(),
                approved_permissions: permissions,
                activation_revision,
                update_policy,
            },
            phase: Phase::Preparing,
        };
        self.transaction_directory(&record.preparation.generation.plugin_id, true)?;
        // The same actual lock later transfers to the configured transaction without a release gap.
        // 同一实际锁稍后无释放间隙地转交给已配置事务。
        let lease = self.transaction_lease(&record.preparation.generation.plugin_id, request_id)?;
        self.write_preparation_locked(&record)?;
        self.index_request_locked(&record.preparation.generation.plugin_id, request_id)?;
        Ok(VulcanPluginActivationPreparation {
            store_root: self.root.clone(),
            record,
            lease: Arc::new(lease),
        })
    }

    /// Start exact `generation` credential values under the original `preparation`; return a writer retaining that owner.
    /// 在原 `preparation` 下开始精确 `generation` 凭据值；返回保留该所有者的写入者。
    pub fn begin_activation_credential_staging(
        &self,
        preparation: &VulcanPluginActivationPreparation,
        generation: &PinnedVulcanPluginGeneration,
        values: &VulcanPluginCredentialValues,
    ) -> VulcanPluginStoreResult<VulcanPluginCredentialStaging> {
        self.begin_owned_credential_staging(
            generation,
            preparation.intent().credential_revision().clone(),
            values,
            Some(preparation),
        )
    }

    /// Retain the actual `preparation` lock for exact `generation`/`revision` while the caller holds its namespace fence.
    /// 调用方持有命名空间屏障时，为精确 `generation`／`revision` 保留实际 `preparation` 锁。
    /// Return shared original ownership; mismatched resource identities never create a writer.
    /// 返回共享原所有权；资源身份不匹配时绝不创建写入者。
    pub(in crate::generation_store) fn retain_preparation_credentials(
        &self,
        preparation: &VulcanPluginActivationPreparation,
        generation: &VulcanPluginGenerationIdentity,
        revision: &VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<Arc<File>> {
        self.check_preparation_owner(preparation)?;
        if preparation.intent().generation() != generation
            || preparation.intent().credential_revision() != revision
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential staging differs from its original preparation intent.",
            ));
        }
        Ok(Arc::clone(&preparation.lease))
    }

    /// Reject standalone staging of an unfinished namespace or any original reserved `revision`.
    /// 拒绝未完成命名空间或任何原预留 `revision` 的独立暂存。
    /// The caller supplies the namespace fence; success grants only the legacy standalone staging path.
    /// 调用方提供命名空间屏障；成功仅授予旧独立暂存路径。
    pub(in crate::generation_store) fn require_unreserved_credential_revision(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<()> {
        self.ensure_no_pending_activation_locked(plugin_id)?;
        for request_id in self.transaction_ids_locked(plugin_id)? {
            // Historical early reservations remain immutable even if no credential inventory was allocated.
            // 历史早期预留保持不可变，即使从未分配凭据清单。
            let original = self
                .read_request_locked(plugin_id, request_id)?
                .ok_or_else(|| Error::invalid("Reserved credential original disappeared."))?;
            if original.credential_revision() == Some(revision) {
                return Err(Error::new(
                    Code::Conflict,
                    "Credential revision belongs to an original activation request.",
                ));
            }
        }
        Ok(())
    }

    /// Reacquire durable parent `request_id` for the exact resumed `generation`/`revision` without replaying work.
    /// 为精确恢复的 `generation`／`revision` 重新取得持久父 `request_id`，不重放工作。
    /// Return its actual lock under the caller's namespace fence; terminal or missing parents remain failures.
    /// 在调用方命名空间屏障下返回其实际锁；终态或缺失父请求保持失败。
    pub(in crate::generation_store) fn retain_resumed_preparation_credentials(
        &self,
        request_id: Uuid,
        generation: &VulcanPluginGenerationIdentity,
        revision: &VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<Arc<File>> {
        // The persisted parent UUID alone selects the original; directory order and current package names are irrelevant.
        // 仅持久父 UUID 选择原请求；目录顺序及当前插件包名称均无关。
        let Some(VulcanPluginActivationRequestRecord::Preparation(record)) =
            self.read_request_locked(&generation.plugin_id, request_id)?
        else {
            return Err(Error::new(
                Code::Conflict,
                "Credential parent is missing or no longer preparing.",
            ));
        };
        if record.phase != Phase::Preparing
            || record.preparation.generation != *generation
            || record.preparation.credential_revision != *revision
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential inventory differs from its active original parent.",
            ));
        }
        // Actual ownership is taken before checking the indexed authority, so a live original stays exclusive.
        // 检查已索引权威之前取得实际所有权，使活动原请求保持排他。
        let lease = Arc::new(self.transaction_lease(&generation.plugin_id, request_id)?);
        // Reuse the same complete ownership validation as initial staging.
        // 复用与初始暂存相同的完整所有权校验。
        let owner = VulcanPluginActivationPreparation {
            store_root: self.root.clone(),
            record,
            lease,
        };
        self.check_preparation_owner(&owner)?;
        Ok(owner.lease)
    }
    /// Begin `request_id` before copying approved `source`, binding its exact predecessor and permissions.
    /// 复制已批准 `source` 前开始 `request_id`，绑定其精确前驱及权限。
    /// Return the original exclusive owner; failures never authorize a replacement UUID or resource identity.
    /// 返回原排他所有者；失败绝不授权替代 UUID 或资源身份。
    pub fn begin_activation_preparation(
        &self,
        request_id: Uuid,
        source: &Path,
        expected_content_fingerprint: &str,
        expected_predecessor: Option<&VulcanPluginActivation>,
        permissions: Vec<String>,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparation> {
        self.begin_preparation_with_policy(
            request_id,
            source,
            expected_content_fingerprint,
            expected_predecessor,
            permissions,
            None,
        )
    }

    /// Copy approved `source` into the exact resources held by `preparation`; return the real package lease.
    /// 将已批准 `source` 复制到 `preparation` 持有的精确资源；返回实际插件包租约。
    /// The borrow retains original ownership throughout copying; an existing incomplete copy is never overwritten.
    /// 借用在复制全程保留原所有权；绝不覆盖既有不完整副本。
    pub fn prepare_activation_package(
        &self,
        preparation: &VulcanPluginActivationPreparation,
        source: &Path,
    ) -> VulcanPluginStoreResult<PinnedVulcanPluginGeneration> {
        {
            // A short namespace observation validates the original, while its exclusive request lock spans I/O.
            // 短命名空间观察校验原请求，其请求排他锁覆盖输入输出全程。
            let _namespace =
                self.namespace_lock(&preparation.record.preparation.generation.plugin_id, true)?;
            self.check_preparation_owner(preparation)?;
        }
        self.prepare_planned_generation(source, &preparation.record.preparation.generation)
    }

    /// Consume the original `preparation` after package and credentials are ready, binding immutable `configuration`.
    /// 插件包及凭据就绪后消费原 `preparation`，绑定不可变 `configuration`。
    /// Return the configured transaction using the same UUID and actual lock; publication failure requires original readback.
    /// 使用相同 UUID 及实际锁返回已配置事务；发布失败要求回读原请求。
    pub fn finish_activation_preparation(
        &self,
        preparation: VulcanPluginActivationPreparation,
        configuration: VulcanPluginConfigurationSelection,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationTransaction> {
        // All physical resource validation and the stage checkpoint share the original namespace fence.
        // 全部物理资源校验与阶段检查点共用原命名空间屏障。
        let _namespace =
            self.namespace_lock(&preparation.record.preparation.generation.plugin_id, false)?;
        self.check_preparation_owner(&preparation)?;
        if Arc::strong_count(&preparation.lease) != 1 {
            return Err(Error::new(
                Code::Busy,
                "Original credential preparation still owns unfinished work.",
            ));
        }
        // Frozen planned identities remain part of every subsequent durable receipt.
        // 固定计划身份继续属于后续每份持久回执。
        let intent = &preparation.record.preparation;
        // The configuration is supplied only after the immutable copy exists and is checked again against that copy.
        // 配置仅在不可变副本存在后提供，并再次按该副本校验。
        let record = VulcanPluginActivationTransactionRecord {
            schema_version: preparation.record.schema_version,
            request_id: preparation.record.request_id,
            predecessor: preparation.record.predecessor.clone(),
            target: VulcanPluginActivation {
                schema_version: CONFIGURED_ACTIVATION_VERSION,
                plugin_id: intent.generation.plugin_id.clone(),
                revision: intent.activation_revision,
                generation: Some(intent.generation.clone()),
                approved_permissions: Some(intent.approved_permissions.clone()),
                credential_revision: Some(intent.credential_revision.clone()),
                configuration: Some(configuration),
            },
            preparation: Some(intent.clone()),
            phase: Phase::Prepared,
        };
        // These actual leases prove complete package bytes, exact permissions, valid configuration and staged credentials.
        // 这些实际租约证明完整插件包字节、精确权限、有效配置及已暂存凭据。
        let (generation, credentials) = self.pin_transaction_target(&record)?;
        self.write_transaction_locked(&record)?;
        Ok(VulcanPluginActivationTransaction {
            store_root: self.root.clone(),
            record,
            generation,
            credentials,
            _lease: preparation.lease,
        })
    }

    /// Resume only original early `plugin_id`/`request_id` ownership; never replay copying or secure writes.
    /// 仅恢复原早期 `plugin_id`／`request_id` 所有权；绝不重放复制或安全写入。
    /// Return an exclusive owner after exact predecessor and index checks, including explicit missing-index repair.
    /// 在精确前驱及索引检查后返回排他所有者，包括明确的缺失索引修复。
    pub fn resume_activation_preparation(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparation> {
        // Resume follows the same lock order as all original activation operations.
        // 恢复遵循与全部原激活操作相同的锁顺序。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // Only the declared early record can produce an early owner.
        // 仅声明的早期记录可以产生早期所有者。
        let Some(VulcanPluginActivationRequestRecord::Preparation(record)) =
            self.read_request_locked(plugin_id, request_id)?
        else {
            return Err(Error::invalid(
                "Original request is not an early preparation.",
            ));
        };
        if record.phase != Phase::Preparing
            || self.read_activation(plugin_id)? != record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Early preparation is terminal or its predecessor changed.",
            ));
        }
        // Actual work retaining the original owner prevents all resumption and abandonment.
        // 保留原所有者的实际工作阻止全部恢复及放弃。
        let lease = self.transaction_lease(plugin_id, request_id)?;
        // Another unfinished original can never be replaced during missing-index recovery.
        // 缺失索引恢复期间绝不能替换其他未完成原请求。
        let indexed = self.indexed_request_locked(plugin_id)?;
        if indexed.as_ref().is_some_and(|current| {
            current.request_id() != request_id && !current.phase().terminal()
        }) {
            return Err(Error::new(
                Code::Busy,
                "Another original request owns this namespace.",
            ));
        }
        if indexed.as_ref().map(|current| current.request_id()) != Some(request_id) {
            self.index_request_locked(plugin_id, request_id)?;
        }
        Ok(VulcanPluginActivationPreparation {
            store_root: self.root.clone(),
            record,
            lease: Arc::new(lease),
        })
    }

    /// Explicitly abandon original early `plugin_id`/`request_id`, preserving resource and external-effect evidence.
    /// 明确放弃原早期 `plugin_id`／`request_id`，保留资源及外部副作用证据。
    /// Return the terminal original; no package or credential is deleted and no callback is replayed.
    /// 返回终态原请求；不删除插件包或凭据，不重放回调。
    pub fn abort_activation_preparation_request(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationRequestRecord> {
        // Original request locking proves that preparation work has actually relinquished ownership.
        // 原请求锁证明准备工作已实际放弃所有权。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // Exact declared early evidence remains available even if candidate files are incomplete or damaged.
        // 即使候选文件不完整或损坏，精确声明的早期证据仍可用。
        let Some(VulcanPluginActivationRequestRecord::Preparation(mut record)) =
            self.read_request_locked(plugin_id, request_id)?
        else {
            return Err(Error::invalid(
                "Original request is not an early preparation.",
            ));
        };
        // A live caller or blocking worker excludes abandonment until its true exit.
        // 活动调用方或阻塞工作者在真正退出前排除放弃。
        let _lease = self.transaction_lease(plugin_id, request_id)?;
        // Validate the entire indexed record even for idempotent terminal observations.
        // 即使幂等终态观察也校验完整索引记录。
        let indexed = self.indexed_request_locked(plugin_id)?;
        if record.phase != Phase::Aborted {
            if record.phase != Phase::Preparing
                || self.read_activation(plugin_id)? != record.predecessor
                || indexed.as_ref().is_some_and(|current| {
                    current.request_id() != request_id && !current.phase().terminal()
                })
            {
                return Err(Error::new(
                    Code::Conflict,
                    "Early request no longer owns its unchanged predecessor.",
                ));
            }
            record.phase = Phase::Aborted;
            self.write_preparation_locked(&record)?;
        }
        if indexed.is_none() {
            self.index_request_locked(plugin_id, request_id)?;
        }
        Ok(VulcanPluginActivationRequestRecord::Preparation(record))
    }

    /// Require `preparation` to retain the indexed original and unchanged complete predecessor under the caller's fence.
    /// 在调用方屏障下要求 `preparation` 保留已索引原请求及未变完整前驱。
    /// Return unit only for a live early stage; this check never repairs missing authority.
    /// 仅活动早期阶段返回空值；此检查绝不修复缺失权威。
    fn check_preparation_owner(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<()> {
        // Plugin identity comes from the frozen intent, never mutable package discovery.
        // 插件身份来自固定意图，绝不来自可变插件包发现。
        let plugin_id = &preparation.record.preparation.generation.plugin_id;
        if preparation.store_root != self.root
            || preparation.record.phase != Phase::Preparing
            || self.indexed_request_locked(plugin_id)?
                != Some(VulcanPluginActivationRequestRecord::Preparation(
                    preparation.record.clone(),
                ))
            || self.read_activation(plugin_id)? != preparation.record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Original preparation ownership or predecessor changed.",
            ));
        }
        Ok(())
    }

    /// Persist the exact early `record` to the original journal path using the existing bounded atomic writer.
    /// 使用既有有界原子写入器将精确早期 `record` 持久保存到原日志路径。
    /// Return storage confirmation; errors retain uncertainty and must be queried by original UUID.
    /// 返回存储确认；错误保留不确定性，必须按原 UUID 查询。
    fn write_preparation_locked(
        &self,
        record: &VulcanPluginActivationPreparationRecord,
    ) -> VulcanPluginStoreResult<()> {
        // Typed validation rejects configured-target claims in the early stage.
        // 类型化校验拒绝早期阶段声称存在已配置目标。
        let plugin_id = &record.preparation.generation.plugin_id;
        Self::validate_preparation_record(record, plugin_id, record.request_id)?;
        // Checkpoint writes cannot recreate vanished original storage.
        // 检查点写入不能重新创建消失的原存储。
        let directory = self
            .transaction_directory(plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original preparation directory disappeared."))?;
        // Persist identities and predecessor only; no secret values or source paths enter this record.
        // 仅持久保存身份及前驱；秘密值或来源路径不进入此记录。
        let bytes = serde_json::to_vec(record)
            .map_err(|_| Error::invalid("Cannot encode early activation request."))?;
        files::write_encoded_record(
            &directory.join(format!("{}.json", record.request_id)),
            &bytes,
            MAX_TRANSACTION_BYTES,
        )
    }
}

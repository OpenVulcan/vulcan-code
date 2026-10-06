//! Shared credential ownership and exclusive collection use the original namespace and revision fences.
//! 共享凭据所有权及排他回收使用原命名空间与修订屏障。

use super::*;
use crate::generation_store::{
    PinnedVulcanPluginGeneration, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result,
};

/// Frozen original collection and terminal confirmation share actual leases/metadata checks but differ in admitted effects.
/// 固定原实例回收及终态确认共享实际租约／元数据检查，但准入效果不同。
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrozenCleanupIntent {
    /// Admit the exact original journal's cleanup progress after independent host/native retirement proof.
    /// 独立宿主／原生退役证明之后，准入精确原日志清理进展。
    Collect,
    /// Require an already discarded actual journal and verify secure absence without deletion or metadata publication.
    /// 要求实际日志已丢弃，并验证安全缺失，不删除或发布元数据。
    ConfirmTerminal,
}

impl VulcanPluginGenerationStore {
    /// Pin the exact staged credentials of `generation` and `revision`; return a real shared owner or an error.
    /// 固定 `generation` 及 `revision` 的精确已暂存凭据；返回实际共享所有者或错误。
    /// The staging writer must be dropped first; this operation neither selects a revision nor reads secret values.
    /// 必须先释放暂存写入者；此操作不选择修订，也不读取秘密值。
    pub fn pin_credentials(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<PinnedVulcanPluginCredentials> {
        if generation.store_root != self.root {
            return Err(Error::invalid(
                "Credential reader belongs to another generation store.",
            ));
        }
        // One selection fence closes the gap between package verification, journal inspection and acquiring ownership.
        // 单个选择屏障封闭插件包校验、日志检查及取得所有权之间的间隙。
        let _namespace = self.namespace_lock(&generation.identity.plugin_id, true)?;
        // Reinspection rejects package changes rather than trusting metadata retained by an older caller.
        // 重新检查拒绝插件包变化，不信任旧调用方保留的元数据。
        let verified = self.pin_locked(&generation.identity)?;
        self.pin_credentials_locked(&verified, revision)
    }

    /// Acquire shared ownership while the caller retains the namespace fence and a verified package pin.
    /// 调用方保留命名空间屏障及已验证插件包固定时，取得共享所有权。
    pub(in crate::generation_store) fn pin_credentials_locked(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<PinnedVulcanPluginCredentials> {
        // The retained record comes from the same complete validation used by activation.
        // 保留记录来自激活使用的相同完整校验。
        let record = self.validate_staged_credentials_locked(generation, revision)?;
        // Shared ownership excludes both interrupted writers and every cooperating cleanup operation.
        // 共享所有权排除中断写入者及所有协作清理操作。
        let lease = self.credential_lease(&generation.identity.plugin_id, revision, true)?;
        Ok(PinnedVulcanPluginCredentials {
            record,
            _lease: Arc::new(lease),
        })
    }

    /// Collect `plugin_id`/`revision` after staging and retirement; return false only if already discarded.
    /// 暂存并退役后回收 `plugin_id`／`revision`；仅已经丢弃时返回 false。
    /// Active selections and real reader leases block deletion; tombstones retain identity without granting execution.
    /// 有效选择及实际读者租约阻止删除；停用记录保留身份，但不授予执行权限。
    pub fn collect_credentials(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<bool> {
        self.cleanup_credentials(plugin_id, revision, backend, true)
    }

    /// Clean the exact retained inventory using `retired` to distinguish staged collection from unfinished discard.
    /// 使用 `retired` 区分已暂存回收与未完成丢弃，清理精确保留清单。
    /// Returns true on confirmed completion, false for an existing terminal record, and an error retaining recovery state.
    /// 确认完成时返回 true，既有终态返回 false，失败返回错误并保留恢复状态。
    pub(in crate::generation_store) fn cleanup_credentials(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        backend: &dyn VulcanPluginCredentialBackend,
        retired: bool,
    ) -> Result<bool> {
        // Retain the plugin fence until the durable no-new-readers checkpoint has been published.
        // 插件屏障保留至禁止新读者的持久检查点发布。
        let namespace = self.namespace_lock(plugin_id, false)?;
        self.check_transaction_credential_retention(plugin_id, revision)?;
        // A copied UUID or a dropped observer cannot release another actual reader's file ownership.
        // 复制 UUID 或释放观察者不能释放其他实际读者的文件所有权。
        let _lease = self.credential_lease(plugin_id, revision, false)?;
        // Retirement allows a tombstone, but unfinished-candidate discard must still reject any referenced identity.
        // 退役允许停用记录，但未完成候选丢弃仍须拒绝任何被引用身份。
        if let Some(activation) = self.read_activation(plugin_id)?
            && activation.credential_revision() == revision
            && (!retired || activation.generation().is_some())
        {
            return Err(Error::invalid(
                "Referenced credential revision cannot be cleaned in this phase.",
            ));
        }
        // The original complete key list remains authoritative even after package files have been reclaimed.
        // 即使插件包文件已被回收，原完整键清单仍为权威。
        let mut record = self
            .read_credential_record_locked(plugin_id, revision)?
            .ok_or_else(|| Error::invalid("Credential cleanup record is missing."))?;
        match record.phase {
            VulcanPluginCredentialPhase::Staged if !retired => {
                return Err(Error::invalid(
                    "Staged credentials require explicit retired-revision collection.",
                ));
            }
            VulcanPluginCredentialPhase::Preparing if retired => {
                return Err(Error::invalid(
                    "Unfinished credentials require explicit candidate discard.",
                ));
            }
            VulcanPluginCredentialPhase::Discarded => return Ok(false),
            VulcanPluginCredentialPhase::Preparing
            | VulcanPluginCredentialPhase::Staged
            | VulcanPluginCredentialPhase::Discarding => {}
        }
        record.phase = VulcanPluginCredentialPhase::Discarding;
        self.write_credential_record_locked(&record)?;
        drop(namespace);
        self.delete_credential_entries(&record, backend, || Ok(()))?;
        // Original-record comparison prevents a non-cooperating edit from relabeling successful cleanup.
        // 原记录比较防止非协作编辑重新标记已成功清理。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        self.confirm_credential_cleanup_locked(&record, |_| {})
    }

    /// Collect the exact frozen original while a typed removal caller retains its namespace throughout every secure effect.
    /// 类型化移除调用方跨每个安全副作用保留命名空间期间，回收精确固定原清单。
    /// expected supplies verified original metadata; backend supplies secure operations and validate rechecks the same admitted tombstone.
    /// expected 提供已验证原元数据；backend 提供安全操作，validate 重新校验相同已准入停用记录。
    /// validate_phase rejects same-original regression; observe_phase records verified observations and acknowledged writes without failure.
    /// validate_phase 拒绝同原实例回退；observe_phase 不可失败地记录已验证观察及已确认写入。
    /// Returns completion only after exact inventory and real reader exclusion, secure absence readback and terminal record confirmation.
    /// 仅在精确清单及真实读者互斥、安全缺失回读和终态记录确认之后返回完成。
    pub(in crate::generation_store) fn collect_frozen_deactivation_credentials_locked(
        &self,
        expected: &VulcanPluginCredentialRecord,
        backend: &dyn VulcanPluginCredentialBackend,
        validate: impl Fn() -> Result<()>,
        validate_phase: impl Fn(VulcanPluginCredentialPhase) -> Result<()>,
        observe_phase: impl Fn(VulcanPluginCredentialPhase),
    ) -> Result<()> {
        self.cleanup_frozen_deactivation_credentials_locked(
            expected,
            backend,
            validate,
            FrozenCleanupIntent::Collect,
            validate_phase,
            observe_phase,
        )
    }

    /// Confirm an actual discarded frozen journal using the same physical exclusion and complete comparison without replaying effects.
    /// 使用相同物理互斥及完整比较确认实际已丢弃固定日志，不重放效果。
    /// expected/backend/validate identify the exact original and real secure readback; returns unit only for actual terminal absence.
    /// expected／backend／validate 标识精确原实例及真实安全回读；仅实际终态缺失返回空值。
    /// validate_phase and observe_phase share the cleanup entry's original watermark without granting new deletion authority.
    /// validate_phase 及 observe_phase 共享清理入口原水位，不授予新删除权威。
    pub(in crate::generation_store) fn confirm_frozen_deactivation_credentials_collected_locked(
        &self,
        expected: &VulcanPluginCredentialRecord,
        backend: &dyn VulcanPluginCredentialBackend,
        validate: impl Fn() -> Result<()>,
        validate_phase: impl Fn(VulcanPluginCredentialPhase) -> Result<()>,
        observe_phase: impl Fn(VulcanPluginCredentialPhase),
    ) -> Result<()> {
        self.cleanup_frozen_deactivation_credentials_locked(
            expected,
            backend,
            validate,
            FrozenCleanupIntent::ConfirmTerminal,
            validate_phase,
            observe_phase,
        )
    }

    /// Apply the explicitly selected intent to one complete actual frozen journal without replacing keys or reopening its namespace.
    /// 对单个完整实际固定日志应用明确已选意图，不替换键或重新打开命名空间。
    /// expected/backend/validate retain original evidence and intent bounds effects; returns verified completion or retained error.
    /// expected／backend／validate 保留原证据，intent 约束效果；返回已验证完成或保留错误。
    /// validate_phase/observe_phase validate and retain actual monotonic checkpoint evidence through backend unwinding.
    /// validate_phase／observe_phase 跨后端展开验证及保留实际单调检查点证据。
    fn cleanup_frozen_deactivation_credentials_locked(
        &self,
        expected: &VulcanPluginCredentialRecord,
        backend: &dyn VulcanPluginCredentialBackend,
        validate: impl Fn() -> Result<()>,
        intent: FrozenCleanupIntent,
        validate_phase: impl Fn(VulcanPluginCredentialPhase) -> Result<()>,
        observe_phase: impl Fn(VulcanPluginCredentialPhase),
    ) -> Result<()> {
        validate()?;
        // The complete original fixes both namespace and revision before actual backend access.
        // 完整原清单在实际后端访问之前固定命名空间及修订。
        let plugin_id = &expected.package.plugin_id;
        // Retain the actual exclusive credential File independently of mere selected revision text.
        // 独立于仅已选修订文本，保留实际排他凭据文件。
        let _lease = self.credential_lease(plugin_id, &expected.revision, false)?;
        self.check_transaction_credential_retention(plugin_id, &expected.revision)?;
        // Durable original metadata is compared in full, permitting only the declared cleanup phase transitions.
        // 持久原元数据被完整比较，仅许可声明清理阶段转换。
        let mut current = self
            .read_credential_record_locked(plugin_id, &expected.revision)?
            .ok_or_else(|| Error::invalid("Original removal credential journal is missing."))?;
        // A historical unfinished inventory may be discarded, but cannot be promoted or relabeled as staged during removal.
        // 历史未完成清单可以丢弃，但不能在移除期间被晋级或重新标记为已暂存。
        let owned_progress = match expected.phase {
            VulcanPluginCredentialPhase::Preparing => matches!(
                current.phase,
                VulcanPluginCredentialPhase::Preparing
                    | VulcanPluginCredentialPhase::Discarding
                    | VulcanPluginCredentialPhase::Discarded
            ),
            VulcanPluginCredentialPhase::Staged => matches!(
                current.phase,
                VulcanPluginCredentialPhase::Staged
                    | VulcanPluginCredentialPhase::Discarding
                    | VulcanPluginCredentialPhase::Discarded
            ),
            VulcanPluginCredentialPhase::Discarding => matches!(
                current.phase,
                VulcanPluginCredentialPhase::Discarding | VulcanPluginCredentialPhase::Discarded
            ),
            VulcanPluginCredentialPhase::Discarded => {
                current.phase == VulcanPluginCredentialPhase::Discarded
            }
        };
        if !owned_progress {
            return Err(Error::new(
                Code::Conflict,
                "Original removal credential phase is not owned.",
            ));
        }
        // Phase is the only owned mutable field; any foreign package, configured subset, key or parent remains a conflict.
        // 阶段为唯一自有可变字段；任何外来包、已配置子集、键或父身份仍为冲突。
        let mut permitted = expected.clone();
        permitted.phase = current.phase;
        if current != permitted {
            return Err(Error::new(
                Code::Conflict,
                "Original removal's complete credential inventory changed.",
            ));
        }
        validate_phase(current.phase)?;
        observe_phase(current.phase);
        validate()?;
        if current.phase == VulcanPluginCredentialPhase::Discarded {
            // A recovered terminal checkpoint confirms actual absence without replaying deletion or publishing another checkpoint.
            // 恢复终态检查点确认实际缺失，不重放删除或发布另一检查点。
            for key in &current.keys {
                validate()?;
                self.verify_credential_entry_absence(&current, key, backend)?;
            }
            validate()?;
            if self
                .read_credential_record_locked(plugin_id, &expected.revision)?
                .as_ref()
                != Some(&current)
            {
                return Err(Error::new(
                    Code::Conflict,
                    "Original terminal credential cleanup changed during verification.",
                ));
            }
            return Ok(());
        }
        if intent == FrozenCleanupIntent::ConfirmTerminal {
            return Err(Error::new(
                Code::Busy,
                "Original historical credentials have not reached confirmed terminal cleanup.",
            ));
        }
        current.phase = VulcanPluginCredentialPhase::Discarding;
        self.write_credential_record_locked(&current)?;
        observe_phase(current.phase);
        self.delete_credential_entries(&current, backend, &validate)?;
        validate()?;
        self.confirm_credential_cleanup_locked(&current, observe_phase)?;
        Ok(())
    }

    /// Delete every exact record key and verify absence using the sole secure cleanup implementation.
    /// 使用唯一安全清理实现删除每个精确 record 键并验证缺失。
    /// record fixes plugin/revision/inventory; backend performs secure effects and validate checks caller's retained authority before each effect.
    /// record 固定插件／修订／清单；backend 执行安全效果，validate 在每个效果之前检查调用方保留权威。
    /// Returns the first secure or ownership failure without fabricating deletion success or replacing the original inventory.
    /// 返回首个安全或所有权失败，不虚构删除成功或替换原清单。
    fn delete_credential_entries(
        &self,
        record: &VulcanPluginCredentialRecord,
        backend: &dyn VulcanPluginCredentialBackend,
        validate: impl Fn() -> Result<()>,
    ) -> Result<()> {
        // Every secure tuple derives only from the complete validated original record.
        // 每个安全元组仅来自完整已验证原记录。
        let plugin_id = &record.package.plugin_id;
        // The same recorded revision stays fixed across deletion and readback.
        // 相同已记录修订跨删除及回读保持固定。
        let revision = &record.revision;
        for key in &record.keys {
            validate()?;
            backend.delete(plugin_id, revision, key).map_err(|_| Error::new(
                Code::Storage,
                "Secure credential deletion failed; the original cleanup inventory is retained.",
            ))?;
            validate()?;
            self.verify_credential_entry_absence(record, key, backend)?;
        }
        Ok(())
    }

    /// Verify actual backend absence for one record/key without deleting, probing legacy accounts or inferring from return status.
    /// 对单个 record／key 验证实际后端缺失，不删除、探测旧账户或从返回状态推断。
    /// backend addresses only the fixed original tuple; returns storage failure for unreadable or still-present values.
    /// backend 仅寻址固定原元组；值不可读或仍存在时返回存储失败。
    fn verify_credential_entry_absence(
        &self,
        record: &VulcanPluginCredentialRecord,
        key: &VulcanPluginCredentialKey,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<()> {
        if backend
            .read(&record.package.plugin_id, &record.revision, key)
            .map_err(|_| {
                Error::new(
                    Code::Storage,
                    "Secure credential deletion verification failed.",
                )
            })?
            .is_some()
        {
            return Err(Error::new(
                Code::Storage,
                "Secure credential entry still exists after deletion.",
            ));
        }
        Ok(())
    }

    /// Confirm the unchanged exact Discarding record and publish the sole terminal checkpoint under caller-held namespace ownership.
    /// 在调用方持有命名空间所有权内，确认未变精确清理中记录，并发布唯一终态检查点。
    /// record is the actual admitted inventory; returns this transition or a conflict without relabeling foreign metadata.
    /// record 为实际已准入清单；返回此次转换或冲突，不重新标记外来元数据。
    /// observe_phase records the acknowledged terminal write immediately, before returning to a possibly interrupted caller.
    /// observe_phase 立即记录已确认终态写入，先于返回可能被中断调用方。
    fn confirm_credential_cleanup_locked(
        &self,
        record: &VulcanPluginCredentialRecord,
        observe_phase: impl Fn(VulcanPluginCredentialPhase),
    ) -> Result<bool> {
        if self
            .read_credential_record_locked(&record.package.plugin_id, &record.revision)?
            .as_ref()
            != Some(record)
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential cleanup record changed; inspect the original revision.",
            ));
        }
        // Preserve every original identity field while changing only the declared terminal phase.
        // 改变仅声明终态阶段时，保留每个原身份字段。
        let mut discarded = record.clone();
        discarded.phase = VulcanPluginCredentialPhase::Discarded;
        self.write_credential_record_locked(&discarded)?;
        observe_phase(discarded.phase);
        Ok(true)
    }
}

//! Shared credential ownership and exclusive collection use the original namespace and revision fences.
//! 共享凭据所有权及排他回收使用原命名空间与修订屏障。

use super::*;
use crate::generation_store::{
    PinnedVulcanPluginGeneration, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result,
};

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
    pub(super) fn cleanup_credentials(
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
            && activation.credential_revision() == Some(revision)
            && (!retired || activation.generation.is_some())
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
        for key in &record.keys {
            backend.delete(plugin_id, revision, key).map_err(|_| Error::new(
                Code::Storage,
                "Secure credential deletion failed; the original cleanup inventory is retained.",
            ))?;
            if backend
                .read(plugin_id, revision, key)
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
        }
        // Original-record comparison prevents a non-cooperating edit from relabeling successful cleanup.
        // 原记录比较防止非协作编辑重新标记已成功清理。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        if self
            .read_credential_record_locked(plugin_id, revision)?
            .as_ref()
            != Some(&record)
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential cleanup record changed; inspect the original revision.",
            ));
        }
        record.phase = VulcanPluginCredentialPhase::Discarded;
        self.write_credential_record_locked(&record)?;
        Ok(true)
    }
}

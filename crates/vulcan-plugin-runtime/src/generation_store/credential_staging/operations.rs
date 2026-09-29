//! Inventory preparation, exact recovery and resumable cleanup share the original plugin namespace fence.
//! 清单准备、精确恢复及可恢复清理共用原插件命名空间屏障。

use super::*;
use crate::generation_store::{
    PinnedVulcanPluginGeneration, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result, files,
};
use std::path::PathBuf;

/// Optional private inventory directory; legacy stores need not contain it until explicit preparation.
/// 可选私有清单目录；旧存储在显式准备前无需包含它。
pub(super) const INVENTORY_DIRECTORY: &str = "credentials";

impl VulcanPluginGenerationStore {
    /// Require a complete durable staging checkpoint for this exact package/revision while the selection fence is held.
    /// 在持有选择屏障期间，要求此精确插件包／修订具有完整持久暂存检查点。
    /// This rejects preparing or discarded candidates without probing OS values or claiming application readiness.
    /// 此检查拒绝准备中或已丢弃候选，不探测 OS 值，也不宣称应用就绪。
    pub(in crate::generation_store) fn validate_staged_credentials_locked(
        &self,
        package: &PinnedVulcanPluginGeneration,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<()> {
        // The journal is mandatory for version-three selection, not an optional alternate authority.
        // 第三版选择必须具有日志；它不是可选替代权威。
        let record = self
            .read_credential_record_locked(&package.identity.plugin_id, revision)?
            .ok_or_else(|| {
                Error::invalid("Selected credential revision has no durable staging inventory.")
            })?;
        if record.package != package.identity
            || record.keys != package.credential_keys
            || record.phase != VulcanPluginCredentialPhase::Staged
        {
            return Err(Error::invalid(
                "Selected credentials are not fully staged for this exact package identity.",
            ));
        }
        Ok(())
    }

    /// Persist a new inventory for `generation` and `revision`, derived from complete `values`; return its exclusive owner.
    /// 为 `generation` 及 `revision` 持久保存从完整 `values` 派生的新清单；返回其排他所有者。
    /// No secure backend is called here; an uncertain write must be read back by the same revision before continuing.
    /// 此处不调用安全后端；不确定写入必须按同一修订回读后再继续。
    pub fn begin_credential_staging(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        revision: VulcanPluginCredentialRevision,
        values: &VulcanPluginCredentialValues,
    ) -> Result<VulcanPluginCredentialStaging> {
        if generation.store_root != self.root {
            return Err(Error::invalid(
                "Credential candidate belongs to another generation store.",
            ));
        }
        // Recheck the actual package under the same namespace fence used by publication and cleanup.
        // 在发布及清理使用的相同命名空间屏障下，重新检查实际插件包。
        let _namespace = self.namespace_lock(&generation.identity.plugin_id, false)?;
        if self
            .read_activation(&generation.identity.plugin_id)?
            .as_ref()
            .and_then(|record| record.credential_revision())
            == Some(&revision)
        {
            return Err(Error::new(
                Code::Conflict,
                "A referenced credential revision cannot become a new staging candidate.",
            ));
        }
        // The pinned copy prevents cooperating package collection during inventory derivation.
        // 固定副本防止清单派生期间发生协作式插件包回收。
        let verified = self.pin_locked(&generation.identity)?;
        // Inventory and permissions share one verified package observation without an additional scan.
        // 清单及权限共用单次已验证插件包观察，无需额外扫描。
        let keys = verified.credential_keys;
        if values.keys().ne(keys.iter()) || values.values().flatten().any(String::is_empty) {
            return Err(Error::invalid(
                "Credential values must name every declared slot exactly; configured values cannot be empty.",
            ));
        }
        // All persisted fields are identifiers or presence flags; values never enter serialization.
        // 全部持久字段均为标识或存在性声明；值绝不进入序列化。
        let record = VulcanPluginCredentialRecord {
            schema_version: CREDENTIAL_RECORD_VERSION,
            package: generation.identity.clone(),
            revision,
            keys,
            configured_keys: configured_keys(values).into_iter().collect(),
            phase: VulcanPluginCredentialPhase::Preparing,
        };
        validate_record(&record, &record.package.plugin_id, &record.revision)?;
        // Acquire mutation ownership before publishing the first durable inventory.
        // 发布首份持久清单前取得变更所有权。
        let lease = self.credential_lease(&record.package.plugin_id, &record.revision)?;
        // Explicit preparation alone creates this optional layout; ordinary reads never repair it.
        // 仅显式准备创建此可选布局；普通读取绝不修补它。
        let root = files::ensure_child(&self.root, INVENTORY_DIRECTORY)?;
        // One validated plugin component contains all of its immutable revision records.
        // 单个已校验插件组件包含其全部不可变修订记录。
        let directory = files::ensure_child(&root, &record.package.plugin_id)?;
        // Existing final records are never reused, including discarded tombstones.
        // 既有最终记录绝不复用，包括已丢弃的停用记录。
        let path = directory.join(format!("{}.json", record.revision.as_str()));
        if files::exists(&path, false)? {
            return Err(Error::new(
                Code::Conflict,
                "Credential revision already has a durable inventory; inspect that original revision.",
            ));
        }
        files::sync_directory(&root)?;
        files::sync_directory(&self.root)?;
        files::write_record(&path, &record)?;
        Ok(VulcanPluginCredentialStaging {
            store: self.clone(),
            record,
            _lease: lease,
        })
    }

    /// Read the exact plugin/revision journal without obtaining mutation ownership or creating missing layout.
    /// 读取精确插件／修订日志，不取得变更所有权，也不创建缺失布局。
    /// Returns None only when its canonical record is absent; malformed records remain explicit errors.
    /// 仅规范记录缺失时返回 None；损坏记录保持为显式错误。
    pub fn credential_record(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<Option<VulcanPluginCredentialRecord>> {
        // Readers and checkpoint writers share one plugin-level fence.
        // 读取者及检查点写入者共用单个插件级屏障。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.read_credential_record_locked(plugin_id, revision)
    }

    /// Resume only the original Preparing record for `plugin_id`/`revision`; returns its exclusive owner without backend calls.
    /// 仅恢复 `plugin_id`／`revision` 的原 Preparing 记录；返回其排他所有者，不调用后端。
    pub fn resume_credential_staging(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<VulcanPluginCredentialStaging> {
        // This fence closes the gap between inspecting durable state and taking actual ownership.
        // 此屏障封闭检查持久状态与取得实际所有权之间的间隙。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // A live original writer makes recovery busy rather than permitting a second write stream.
        // 存活原写入者使恢复报告繁忙，不允许第二条写入流。
        let lease = self.credential_lease(plugin_id, revision)?;
        // Recovery never guesses missing inventory keys from a latest package or global catalog.
        // 恢复绝不从最新插件包或全局目录猜测缺失清单键。
        let record = self
            .read_credential_record_locked(plugin_id, revision)?
            .ok_or_else(|| Error::invalid("Credential preparation record is missing."))?;
        if record.phase != VulcanPluginCredentialPhase::Preparing {
            return Err(Error::invalid(
                "Only the original preparing credential revision can resume writes.",
            ));
        }
        Ok(VulcanPluginCredentialStaging {
            store: self.clone(),
            record,
            _lease: lease,
        })
    }

    /// Explicitly discard an unfinished candidate using `backend`; returns false for an already completed discard.
    /// 使用 `backend` 显式丢弃未完成候选；已经完成丢弃时返回假。
    /// Staged or currently referenced revisions cannot use this API; published-revision collection requires later reader ownership integration.
    /// 已暂存或当前被引用的修订不能使用此接口；已发布修订回收要求后续读者所有权接入。
    pub fn discard_credential_staging(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<bool> {
        // The plugin fence protects the decision; the revision lease protects every subsequent backend deletion.
        // 插件屏障保护决策；修订租约保护后续每次后端删除。
        let namespace = self.namespace_lock(plugin_id, false)?;
        // Live preparation must finish or release its actual owner before cleanup can begin.
        // 存活准备必须完成或释放实际所有者后，清理才能开始。
        let _lease = self.credential_lease(plugin_id, revision)?;
        // A tombstone still names a published revision and is not an abandoned preparation.
        // 停用记录仍指定已发布修订，不是放弃的准备。
        if self
            .read_activation(plugin_id)?
            .as_ref()
            .and_then(|record| record.credential_revision())
            == Some(revision)
        {
            return Err(Error::invalid(
                "Referenced credential revisions cannot be discarded as staging candidates.",
            ));
        }
        // The immutable key inventory remains the cleanup authority even after its source package is removed.
        // 即使来源插件包已移除，不可变键清单仍是清理权威。
        let mut record = self
            .read_credential_record_locked(plugin_id, revision)?
            .ok_or_else(|| Error::invalid("Credential cleanup record is missing."))?;
        match record.phase {
            VulcanPluginCredentialPhase::Staged => {
                return Err(Error::invalid(
                    "Staged credentials require published-revision retention and collection.",
                ));
            }
            VulcanPluginCredentialPhase::Discarded => return Ok(false),
            VulcanPluginCredentialPhase::Preparing | VulcanPluginCredentialPhase::Discarding => {}
        }
        record.phase = VulcanPluginCredentialPhase::Discarding;
        self.write_credential_record_locked(&record)?;
        drop(namespace);
        for key in &record.keys {
            backend.delete(plugin_id, revision, key).map_err(|_| Error::new(Code::Storage, "Secure credential candidate deletion failed; the original cleanup inventory is retained."))?;
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
                    "Secure credential entry still exists after candidate deletion.",
                ));
            }
        }
        // Do not publish completion if a non-cooperating writer changed the retained journal meanwhile.
        // 若非协作写入者期间改变保留日志，则不发布完成。
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

    /// Open the original nonblocking exclusive revision lock for validated plugin/revision names.
    /// 为已校验插件／修订名称打开原非阻塞排他修订锁。
    /// Callers must already hold the plugin namespace fence; the returned file owns mutation permission.
    /// 调用方必须已经持有插件命名空间屏障；返回文件持有变更许可。
    fn credential_lease(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<File> {
        files::lock(
            &self.root.join("locks").join(format!(
                "{plugin_id}-credential-{}.lease",
                revision.as_str()
            )),
            false,
        )
    }

    /// Return an existing validated inventory directory for `plugin_id`, without initializing missing children.
    /// 返回 `plugin_id` 既有已校验清单目录，不初始化缺失子目录。
    pub(super) fn credential_directory(&self, plugin_id: &str) -> Result<Option<PathBuf>> {
        // The fixed parent is checked before addressing the validated plugin child.
        // 寻址已校验插件子项前检查固定父项。
        let root = self.root.join(INVENTORY_DIRECTORY);
        if !files::exists(&root, true)? {
            return Ok(None);
        }
        // Absence means this plugin has never prepared a revision in this store.
        // 缺失表示此插件从未在此存储准备修订。
        let directory = root.join(plugin_id);
        if !files::exists(&directory, true)? {
            return Ok(None);
        }
        Ok(Some(directory))
    }

    /// Read and validate one canonical journal while the caller holds its plugin namespace fence.
    /// 在调用方持有插件命名空间屏障期间，读取并校验单个规范日志。
    pub(super) fn read_credential_record_locked(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> Result<Option<VulcanPluginCredentialRecord>> {
        // Optional layout is a declared compatibility boundary, never a fallback to another revision.
        // 可选布局是声明的兼容边界，绝不回退到其他修订。
        let Some(directory) = self.credential_directory(plugin_id)? else {
            return Ok(None);
        };
        // Strict bounded decoding rejects duplicate, missing and unknown fields before semantic validation.
        // 严格有界解码在语义校验前拒绝重复、缺失及未知字段。
        let record: Option<VulcanPluginCredentialRecord> =
            files::read_record(&directory.join(format!("{}.json", revision.as_str())))?;
        if let Some(record) = &record {
            validate_record(record, plugin_id, revision)?;
        }
        Ok(record)
    }

    /// Persist the exact retained `record`; the caller holds its revision lease and namespace fence.
    /// 持久保存精确保留 `record`；调用方持有其修订租约及命名空间屏障。
    pub(super) fn write_credential_record_locked(
        &self,
        record: &VulcanPluginCredentialRecord,
    ) -> Result<()> {
        validate_record(record, &record.package.plugin_id, &record.revision)?;
        // Mutation cannot silently recreate lost inventory layout.
        // 变更不能静默重建丢失清单布局。
        let directory = self
            .credential_directory(&record.package.plugin_id)?
            .ok_or_else(|| Error::invalid("Credential inventory directory disappeared."))?;
        files::write_record(
            &directory.join(format!("{}.json", record.revision.as_str())),
            record,
        )
    }
}

/// Validate exact record identity and canonical complete/subset inventories; returns no secret-bearing diagnostics.
/// 校验精确记录身份及规范完整／子集清单；不返回包含秘密的诊断。
fn validate_record(
    record: &VulcanPluginCredentialRecord,
    plugin_id: &str,
    revision: &VulcanPluginCredentialRevision,
) -> Result<()> {
    files::validate_identity(&record.package)?;
    if record.schema_version != CREDENTIAL_RECORD_VERSION
        || record.package.plugin_id != plugin_id
        || &record.revision != revision
    {
        return Err(Error::invalid(
            "Credential inventory identity or schema does not match its canonical path.",
        ));
    }
    if record.keys.windows(2).any(|pair| pair[0] >= pair[1])
        || record
            .configured_keys
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || record
            .configured_keys
            .iter()
            .any(|key| record.keys.binary_search(key).is_err())
    {
        return Err(Error::invalid(
            "Credential inventory must be canonical, unique and contain its configured subset.",
        ));
    }
    for key in &record.keys {
        crate::catalog::validate_identifier(&key.server_id, "credential server id")
            .map_err(Error::invalid)?;
        if key.secret_key.trim().is_empty() || key.secret_key.contains('\0') {
            return Err(Error::invalid(
                "Credential key must be nonblank and contain no NUL.",
            ));
        }
    }
    Ok(())
}

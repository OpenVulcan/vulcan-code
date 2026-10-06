//! A retained preparation writes only its original secure namespace and checks every readback before completion.
//! 保留准备仅写入其原安全命名空间，并在完成前检查每次回读。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
    VulcanPluginStoreResult as Result,
};

impl VulcanPluginCredentialStaging {
    /// Borrow the immutable inventory and last acknowledged phase; uncertain writes require store readback.
    /// 借用不可变清单及最后已确认阶段；不确定写入要求回读存储。
    pub fn record(&self) -> &VulcanPluginCredentialRecord {
        &self.record
    }

    /// Read this retained owner's actual inventory; return only exact Preparing, its unique Staged successor, or same Staged.
    /// 读取此保留所有者实际清单；仅返回精确 Preparing、其唯一 Staged 后继或相同 Staged。
    /// No backend operation, checkpoint write or revision/request File reopening occurs; errors preserve the original owner.
    /// 不发生后端操作、检查点写入或修订／请求 File 重开；错误保留原所有者。
    pub fn reconcile_owned(&mut self) -> Result<VulcanPluginCredentialRecord> {
        if !matches!(
            self.record.phase,
            VulcanPluginCredentialPhase::Preparing | VulcanPluginCredentialPhase::Staged
        ) {
            return Err(Error::new(
                Code::Conflict,
                "Owned credential reconciliation requires the original Preparing or Staged inventory.",
            ));
        }
        super::operations::validate_record(
            &self.record,
            &self.record.plugin_id,
            &self.record.revision,
        )
        .map_err(super::reservation::owned_record_error)?;
        // This short observation fence never reopens the already retained exclusive revision or parent request File.
        // 此短观察屏障绝不重新打开已保留排他修订或父请求 File。
        let _namespace = self.store.namespace_lock(&self.record.plugin_id, true)?;
        #[cfg(test)]
        super::reservation::observation_before_readback(&self.store.root, &self.record)?;
        // The actual full record is authoritative; malformed replacements are conflicts, while I/O remains Storage.
        // 实际完整记录具有权威性；格式损坏替换属于冲突，而 I/O 保持 Storage。
        let actual = self
            .store
            .read_credential_record_locked(&self.record.plugin_id, &self.record.revision)
            .map_err(super::reservation::owned_record_error)?
            .ok_or_else(|| {
                Error::new(
                    Code::Conflict,
                    "Original owned credential inventory disappeared.",
                )
            })?;
        // Preparing has exactly one permitted successor, changing only the declared durable phase.
        // Preparing 仅有一个允许后继，只改变声明持久阶段。
        let mut staged = self.record.clone();
        staged.phase = VulcanPluginCredentialPhase::Staged;
        if actual != self.record
            && !(self.record.phase == VulcanPluginCredentialPhase::Preparing && actual == staged)
        {
            return Err(Error::new(
                Code::Conflict,
                "Owned credential inventory differs from its exact original record or unique Staged successor.",
            ));
        }
        self.record = actual.clone();
        Ok(actual)
    }

    /// Stage complete `values` through `backend` using this original lease; return the durably verified journal.
    /// 使用此原租约经 `backend` 暂存完整 `values`；返回持久已验证日志。
    /// Existing differing values are never overwritten; backend failures retain Preparing and never imply rollback.
    /// 既有不同值绝不覆盖；后端失败保留 Preparing，绝不表示已回滚。
    pub fn write_and_finish(
        &mut self,
        backend: &dyn VulcanPluginCredentialBackend,
        values: &VulcanPluginCredentialValues,
    ) -> Result<VulcanPluginCredentialRecord> {
        if self.record.phase != VulcanPluginCredentialPhase::Preparing
            || values.keys().ne(self.record.keys.iter())
            || configured_keys(values)
                .iter()
                .ne(self.record.configured_keys.iter())
            || values.values().flatten().any(String::is_empty)
        {
            return Err(Error::invalid(
                "Credential values or phase differ from the original preparation inventory.",
            ));
        }
        self.check_original_record()?;
        // Validate all existing values before writing any missing one, so collisions never become overwrites.
        // 写入任何缺失值前校验全部既有值，使冲突绝不转为覆盖。
        let mut missing = Vec::new();
        for (key, expected) in values {
            // Backend error text is deliberately not propagated because a foreign backend may include secret values.
            // 故意不传播后端错误文本，因为外部后端可能包含秘密值。
            let actual = backend
                .read(&self.record.plugin_id, &self.record.revision, key)
                .map_err(|_| {
                    Error::new(
                        Code::Storage,
                        "Secure credential candidate inspection failed.",
                    )
                })?;
            match (&actual, expected) {
                (None, Some(_)) => missing.push(key),
                _ if actual.as_ref() == expected.as_ref() => {}
                _ => {
                    return Err(Error::new(
                        Code::Conflict,
                        "An existing candidate credential differs from the supplied original values.",
                    ));
                }
            }
        }
        for key in missing {
            // Presence was explicitly validated above; no alternate value source is consulted.
            // 上文已明确校验存在性；不查询其他值来源。
            let value = values[key]
                .as_deref()
                .ok_or_else(|| Error::invalid("Configured credential value disappeared."))?;
            backend.write(&self.record.plugin_id, &self.record.revision, key, value).map_err(|_| Error::new(Code::Storage, "Secure credential candidate write failed; the original inventory is retained."))?;
        }
        for (key, expected) in values {
            // Readback verifies both configured and explicitly absent slots, not merely the last write result.
            // 回读同时验证已配置及明确缺失槽位，不仅检查最后写入结果。
            let actual = backend
                .read(&self.record.plugin_id, &self.record.revision, key)
                .map_err(|_| {
                    Error::new(
                        Code::Storage,
                        "Secure credential candidate verification failed.",
                    )
                })?;
            if &actual != expected {
                return Err(Error::new(
                    Code::Storage,
                    "Secure credential candidate readback differs from the original values.",
                ));
            }
        }
        // The exclusive revision lease survives this short checkpoint fence and every uncertain write outcome.
        // 排他修订租约在此短检查点屏障及每次不确定写入结果期间持续存在。
        let _namespace = self.store.namespace_lock(&self.record.plugin_id, false)?;
        self.check_original_record_locked()?;
        // Publish only non-secret metadata after all actual backend reads succeeded.
        // 全部实际后端读取成功后，仅发布非秘密元数据。
        let mut staged = self.record.clone();
        staged.phase = VulcanPluginCredentialPhase::Staged;
        self.store.write_credential_record_locked(&staged)?;
        #[cfg(test)]
        super::reservation::observation_after_write(&self.store.root, &staged)?;
        self.record = staged.clone();
        Ok(staged)
    }

    /// Compare the live record to this owner's original inventory under its namespace read fence.
    /// 在命名空间读取屏障下，将实时记录与此所有者原清单比较。
    fn check_original_record(&self) -> Result<()> {
        // The revision lease is already held; this read fence only coordinates checkpoint publication.
        // 已经持有修订租约；此读取屏障仅协调检查点发布。
        let _namespace = self.store.namespace_lock(&self.record.plugin_id, true)?;
        self.check_original_record_locked()
    }

    /// Reject replacement or disappearance of the retained record while its namespace fence is held.
    /// 在持有命名空间屏障期间，拒绝保留记录被替换或消失。
    fn check_original_record_locked(&self) -> Result<()> {
        if self
            .store
            .read_credential_record_locked(&self.record.plugin_id, &self.record.revision)?
            .as_ref()
            != Some(&self.record)
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential preparation record changed; inspect the original revision before continuing.",
            ));
        }
        Ok(())
    }
}

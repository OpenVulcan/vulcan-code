//! A still-held first credential namespace permits sealed read-only inspection of its exact original activation parent.
//! 仍持有首次凭据命名空间允许对其精确原激活父级进行密封只读检查。

use super::*;
use crate::VulcanPluginActivationPreparationInspection;

impl VulcanPluginGenerationStore {
    /// Inspect preparation and reservation through the original retained namespace; return actual full activation history.
    /// 通过原保留命名空间检查 preparation 及 reservation；返回实际完整激活历史。
    /// Only a live original first-publication namespace is eligible; no lock, parent Arc clone, publication or backend access occurs.
    /// 仅存活原首次发布命名空间符合条件；不取得锁、不克隆父 Arc、不发布或访问后端。
    pub fn inspect_activation_preparation_credential_staging_reservation(
        &self,
        preparation: &crate::VulcanPluginActivationPreparation,
        reservation: &VulcanPluginCredentialStagingReservation,
    ) -> Result<VulcanPluginActivationPreparationInspection> {
        if reservation.staging.store.root != self.root || reservation.namespace.is_none() {
            return Err(Error::new(
                Code::Conflict,
                "Credential reservation inspection requires this store's still-held original namespace.",
            ));
        }
        // The private original inventory is strictly validated but never exposed as an already durable record.
        // 私有原清单被严格校验，但绝不暴露为已经持久化记录。
        let intended = &reservation.staging.record;
        validate_record(intended, &intended.package.plugin_id, &intended.revision)
            .map_err(owned_record_error)?;
        if intended.schema_version != ACTIVATION_CREDENTIAL_RECORD_VERSION
            || intended.phase != VulcanPluginCredentialPhase::Preparing
            || intended.activation_request != Some(preparation.request_id())
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential reservation differs from its complete original activation preparation.",
            ));
        }
        // Borrowing the fully validated actual parent's Arc does not alter its strong count even transiently.
        // 借用完整验证实际父 Arc，即使瞬时也不改变其强引用计数。
        let parent = self.preparation_credential_parent_locked(
            preparation,
            &intended.package,
            &intended.revision,
        )?;
        if !reservation
            .staging
            ._activation_request_lease
            .as_ref()
            .is_some_and(|retained| Arc::ptr_eq(retained, parent))
        {
            return Err(Error::new(
                Code::Conflict,
                "Credential reservation does not retain its exact original physical parent File.",
            ));
        }
        // Genuine first-inventory absence and exact written Preparing are distinct permitted observations.
        // 真实首次清单缺失及精确已写 Preparing 是不同允许观察。
        let actual = self
            .read_credential_record_locked(&intended.package.plugin_id, &intended.revision)
            .map_err(owned_record_error)?;
        if actual.as_ref().is_some_and(|record| record != intended) {
            return Err(Error::new(
                Code::Conflict,
                "Credential inventory differs from its exact original reserved Preparing body.",
            ));
        }
        // The single strict historical projector verifies actual originals and all four canonical sidecar formats.
        // 唯一严格历史投影器验证实际原请求及全部四种规范附属格式。
        self.inspect_activation_preparation_owner_locked(preparation)
    }
}

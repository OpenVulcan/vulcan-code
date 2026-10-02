//! One successful old Full publication seals its original source and exact final catalog allocation.
//! 单次成功旧 Full 发布封闭其原来源及精确最终目录分配。

use super::*;
use crate::runtime::VulcanPluginManagerState;

/// Successful old Full evidence remains separate from startup eligibility and durable transfer authority.
/// 成功旧 Full 证据保持独立于启动资格及持久转交权威。
pub struct VulcanPluginActivationPredecessorPublication {
    /// Weak actual manager identity never creates another runtime owner.
    /// 弱实际管理器身份绝不创建另一运行时所有者。
    manager: std::sync::Weak<VulcanPluginManagerState>,
    /// Weak actual source identity grants no reconstructed recovery File or physical preparation.
    /// 弱实际来源身份不授予重建恢复文件或物理准备。
    source: std::sync::Weak<VulcanPluginPredecessorRestorationSource>,
    /// Full original Aborted values fix old selection and request, without owning its transaction File.
    /// 完整原 Aborted 值固定旧选择及请求，不拥有其事务文件。
    original: crate::VulcanPluginActivationTransactionRecord,
    /// The same original successor generation is retained through closed and final projection.
    /// 相同原后继代次跨关闭及最终投影保留。
    generation: u64,
    /// Exact original rollback catalog cannot be substituted by equal fields.
    /// 精确原回退目录不能由相等字段替代。
    closed: Arc<VulcanPluginCatalogSnapshot>,
    /// Actual successful publisher accepts this exact final catalog allocation.
    /// 实际成功发布器接纳此精确最终目录分配。
    catalog: Arc<VulcanPluginCatalogSnapshot>,
}

impl VulcanPluginActivationPredecessorPublication {
    /// Prepare validated manager/transaction/source/closed/catalog before the external publisher; return only after success.
    /// 外部发布器前准备已校验 manager／transaction／source／closed／catalog；仅成功后返回。
    /// No new request, reader, runtime or durable ledger is allocated by this evidence.
    /// 此证据不分配新请求、读者、运行时或持久账本。
    pub(super) fn new(
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        closed: &Arc<VulcanPluginCatalogSnapshot>,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
    ) -> Self {
        Self {
            manager: Arc::downgrade(&manager.state),
            source: Arc::downgrade(source),
            original: transaction.record().clone(),
            generation: source.plugin().generation,
            closed: Arc::clone(closed),
            catalog: Arc::clone(catalog),
        }
    }

    /// Borrow the exact accepted final Arc for read-only inspection; no inputs or implicit admission grant.
    /// 借用精确已接纳最终 Arc 以只读检查；无输入或隐式入场授权。
    pub fn catalog(&self) -> &Arc<VulcanPluginCatalogSnapshot> {
        &self.catalog
    }

    /// Validate manager, full original transaction and same source against this actual final publication.
    /// 按此实际最终发布校验 manager、完整原 transaction 及相同 source。
    /// Caller holds registration exclusion; exact current final Arc and same Pending File remain mandatory.
    /// 调用方持有注册互斥；仍强制要求精确当前最终 Arc 及相同 Pending 文件。
    /// transaction is an exact full-record observation; only the same actual source proves storage and physical ownership.
    /// transaction 为精确完整记录观测；仅相同实际 source 证明存储及物理所有权。
    /// Return validation only; strict closed-source initialization eligibility is never broadened.
    /// 仅返回校验；绝不扩大严格关闭来源初始化资格。
    #[cfg(test)]
    pub(in crate::runtime) fn validate_current(
        &self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate_current_identity(manager, transaction, source)?;
        source.validate_pending_owner()
    }

    /// Validate the actual final publication without reading Pending, so the same live owner may reconcile Transferred.
    /// 校验实际最终发布而不读取 Pending，使相同存活所有者可以对账 Transferred。
    /// manager/transaction/source must retain exact original allocation and full values; caller holds registration.
    /// manager／transaction／source 必须保留精确原分配及完整值；调用方持有注册门。
    /// Return identity validation only, without granting initialization or constructing durable transfer authority.
    /// 仅返回身份校验，不授予初始化或构造持久转交权威。
    pub(in crate::runtime) fn validate_current_identity(
        &self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !std::sync::Weak::ptr_eq(&self.manager, &Arc::downgrade(&manager.state))
            || !std::sync::Weak::ptr_eq(&self.source, &Arc::downgrade(source))
            || self.original.phase() != VulcanPluginActivationTransactionPhase::Aborted
            || &self.original != transaction.record()
            || &self.original != source.original()
            || !Arc::ptr_eq(&self.closed, source.staged_catalog())
            || self.closed.service_epoch != manager.state.manager_id
            || self.catalog.service_epoch != manager.state.manager_id
            || self.generation != source.plugin().generation
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor publication belongs to another manager, source or full original.",
            ));
        }
        // Pure original values bind the entire immutable old configuration and credential revision.
        // 纯原值绑定完整不可变旧配置及凭据修订。
        let old = self.original.predecessor().ok_or_else(|| {
            VulcanPluginRuntimeError::new(
                "Predecessor publication has no original managed old selection.",
            )
        })?;
        // Borrow current exact allocation under the existing manager snapshot lock.
        // 在既有管理器快照锁下借用当前精确分配。
        let current = manager
            .state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        if !Arc::ptr_eq(&current, &self.catalog) {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor final publication catalog changed.",
            ));
        }
        // This sole declaration belongs to old selection, never the aborted target's equivalent identity.
        // 此唯一声明属于旧选择，绝不属于中止目标的等价身份。
        let plugin = current
            .plugins
            .get(&source.plugin().id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Published predecessor is missing."))?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.availability != VulcanPluginAvailability::Enabled
            || !plugin.desired_enabled
            || !plugin.runtime_execution_available
            || plugin.generation != self.generation
            || self.generation == 0
            || old.configuration().is_none()
            || old.credential_revision().is_none()
            || !plugin
                .managed_package()
                .is_some_and(|managed| managed.matches_activation(old))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Published predecessor differs from its exact old managed route.",
            ));
        }
        drop(current);
        Ok(())
    }
}

impl std::fmt::Debug for VulcanPluginActivationPredecessorPublication {
    /// Format redacted original request and generation for formatter; return its formatting result.
    /// 为 formatter 格式化脱敏原请求及代次；返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginActivationPredecessorPublication")
            .field("request_id", &self.original.request_id())
            .field("generation", &self.generation)
            .finish()
    }
}

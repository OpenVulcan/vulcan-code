//! Exact postselection collection borrows the original File and keeps physical package and credential-reader exclusion.
//! 精确选择后回收借用原文件，并保持物理包及凭据读者互斥。

use super::*;

impl VulcanPluginGenerationStore {
    /// Collect only this original's retired package under its retained namespace, after the host separately confirms full removal.
    /// 在宿主独立确认完整移除之后，于保留命名空间内仅回收此原实例退役包。
    /// original supplies the actual admitted tombstone; returns true for this collection, false for verified prior completion.
    /// original 提供实际已准入停用记录；此次回收返回真，已验证先前完成返回假。
    /// This API does not infer host Full publication or native retirement; actual remaining package pins return Busy before deletion.
    /// 此接口不推断宿主完整发布或原生退役；实际剩余包固定在删除之前返回忙碌。
    pub fn collect_deactivation_package_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<bool> {
        original.validate_published_for_catalog(self)?;
        // Use the complete original identity, never a new candidate or a path copied from current discovery.
        // 使用完整原身份，绝不使用新候选或从当前发现复制的路径。
        let identity = original
            .original
            .generation()
            .ok_or_else(|| Error::invalid("Original removal has no package identity."))?;
        self.collect_deactivation_inventory_package_retained(original, identity)
    }

    /// Collect one exact pre-callback package inventory entry after independent Full removal and real native/reader retirement.
    /// 独立完整移除及真实原生／读者退役之后，回收单个精确回调前包清单条目。
    /// original supplies the same actual File and identity must match its complete frozen record; returns current/prior physical collection.
    /// original 提供相同实际文件，identity 必须匹配完整固定记录；返回当前／先前物理回收。
    pub fn collect_deactivation_inventory_package_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<bool> {
        original.validate_published_for_catalog(self)?;
        // A copied identity absent from this actual original cannot authorize collection of another history or plugin.
        // 此实际原清单中缺失的复制身份不能授权回收另一历史或插件。
        let expected = original
            .inventory
            .packages
            .iter()
            .find(|record| &record.identity == identity)
            .ok_or_else(|| {
                Error::new(
                    Code::Conflict,
                    "Package identity is outside the original removal inventory.",
                )
            })?;
        // The same existing physical lease and sole collection producer exclude every actual reader across processes.
        // 同一既有物理租约及唯一回收生产者排除每个跨进程实际读者。
        let _generation = self.lock_generation_for_collection_locked(identity)?;
        // Inspect the exact actual marker after physical exclusion; phase changes are limited to original cleanup progress.
        // 物理互斥之后检查精确实际标记；阶段变化仅限于原清理进展。
        let (_, current) = self.read_generation(identity)?;
        validate_package_progress(expected, &current)?;
        // The same original remembers actual cleanup progress even when its original frozen phase was Prepared.
        // 相同原实例记住实际清理进展，即使其原固定阶段为已准备。
        let progress = original
            .inventory
            .package_progress
            .get(&identity.generation_id)
            .ok_or_else(|| Error::invalid("Original package progress metadata is missing."))?;
        progress.validate_package(current.state)?;
        progress.record_package(current.state);
        // Revalidate immediately before irreversible bytes deletion while still holding both actual original files.
        // 仍持有两个实际原文件期间，于不可逆字节删除之前立即重新验证。
        original.validate_published_for_catalog(self)?;
        Ok(self.collect_generation_observed_locked(
            identity,
            |record| {
                validate_package_progress(expected, record)?;
                progress.validate_package(record.state)
            },
            |phase| progress.record_package(phase),
        )? == super::super::operations::GenerationCollectionOutcome::CollectedNow)
    }

    /// Collect only this original's versioned credentials under the same actual namespace and frozen verified inventory.
    /// 在相同实际命名空间及固定已验证清单内，仅回收此原实例版本化凭据。
    /// original fixes the exact retired revision and backend addresses only that namespace; returns verified completion or retained error.
    /// original 固定精确退役修订，backend 仅寻址该命名空间；返回已验证完成或保留错误。
    /// The host must first confirm full removal and real owner drain.
    /// 宿主必须先确认完整移除及真实所有者排空。
    pub fn collect_deactivation_credentials_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<()> {
        original.validate_published_for_catalog(self)?;
        self.collect_deactivation_inventory_credentials_retained(
            original,
            original.credential_record.revision(),
            backend,
        )
    }

    /// Collect one exact historical revision including unfinished inventories, under the same original File and actual revision-reader exclusion.
    /// 在相同原文件及实际修订读者互斥内回收单个精确历史修订，包含未完成清单。
    /// original/revision select its complete frozen journal and backend supplies secure effects; returns verified cleanup or retained failure.
    /// original／revision 选择完整固定日志，backend 提供安全效果；返回已验证清理或保留失败。
    /// Active native writers and retained transaction parents must retire separately; their real ownership is never inferred from journal phase.
    /// 活动原生写入者及保留事务父级必须独立退役；其真实所有权绝不从日志阶段推断。
    pub fn collect_deactivation_inventory_credentials_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
        revision: &VulcanPluginCredentialRevision,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<()> {
        original.validate_published_for_catalog(self)?;
        // Historical revision membership is fixed before callbacks.
        // 历史修订成员关系在回调之前固定。
        let expected = original
            .inventory
            .credentials
            .iter()
            .find(|record| record.revision() == revision)
            .ok_or_else(|| {
                Error::new(
                    Code::Conflict,
                    "Credential revision is outside the original removal inventory.",
                )
            })?;
        // Both current and historical entry points borrow this sole revision watermark, closing cross-entry replay.
        // 当前及历史入口均借用此唯一修订水位，封闭跨入口重放。
        let progress = original
            .inventory
            .credential_progress
            .get(revision)
            .ok_or_else(|| Error::invalid("Original credential progress metadata is missing."))?;
        self.collect_frozen_deactivation_credentials_locked(
            expected,
            backend,
            || original.validate_published_for_catalog(self),
            |phase| progress.validate_credential(phase),
            |phase| progress.record_credential(phase),
        )
    }

    /// Confirm every captured package/credential history and reject added or replaced entries without deleting or publishing checkpoints.
    /// 确认每个捕获包／凭据历史，并拒绝新增或替代条目，不删除或发布检查点。
    /// original is the same actual owner and backend supplies readback only; returns storage absence proof, never host Full/native proof.
    /// original 为相同实际所有者，backend 仅提供回读；返回存储缺失证明，绝非宿主完整／原生证明。
    pub fn confirm_deactivation_inventory_collected_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<()> {
        original.validate_published_for_catalog(self)?;
        // A complete actual census excludes silently introduced or removed marker/journal identities.
        // 完整实际清单排除静默引入或移除的标记／日志身份。
        let current = self.capture_removal_inventory_locked(&original.original.plugin_id)?;
        validate_inventory_membership(original, &current)?;
        for expected in &original.inventory.credentials {
            // Read-only terminal observation shares the same monotonic watermark as every actual cleanup entry point.
            // 只读终态观察与每个实际清理入口共享相同单调水位。
            let progress = original
                .inventory
                .credential_progress
                .get(expected.revision())
                .ok_or_else(|| {
                    Error::invalid("Original credential progress metadata is missing.")
                })?;
            self.confirm_frozen_deactivation_credentials_collected_locked(
                expected,
                backend,
                || original.validate_published_for_catalog(self),
                |phase| progress.validate_credential(phase),
                |phase| progress.record_credential(phase),
            )?;
        }
        // Check physical packages after secure readback so an intervening unexpected package resurrection cannot be missed.
        // 安全回读之后检查物理包，使中间意外包复活不能被漏掉。
        for expected in &original.inventory.packages {
            original.validate_published_for_catalog(self)?;
            // Actual reader and transaction ownership remains required even for terminal observations.
            // 即使终态观测仍要求实际读者及事务所有权释放。
            let _generation = self.lock_generation_for_collection_locked(&expected.identity)?;
            // Use the same sole marker reader without invoking an effectful collection producer.
            // 使用相同唯一标记读取器，不调用产生效果的回收生产者。
            let (root, record) = self.read_generation(&expected.identity)?;
            validate_package_progress(expected, &record)?;
            // A terminal observation is retained before physical absence checking, including a later conflict or panic.
            // 终态观察在物理缺失检查之前被保留，包括随后冲突或 panic。
            let progress = original
                .inventory
                .package_progress
                .get(&expected.identity.generation_id)
                .ok_or_else(|| Error::invalid("Original package progress metadata is missing."))?;
            progress.validate_package(record.state)?;
            progress.record_package(record.state);
            if record.state != GenerationState::Collected {
                return Err(Error::new(
                    Code::Busy,
                    "Original historical package has not reached terminal collection.",
                ));
            }
            if files::exists(&root.join(&expected.identity.plugin_id), true)? {
                return Err(Error::new(
                    Code::Conflict,
                    "Original collected historical package contains resurrected bytes.",
                ));
            }
        }
        original.validate_published_for_catalog(self)?;
        // Final full readback rejects foreign metadata mutations introduced during secure backend verification.
        // 最终完整回读拒绝在安全后端验证期间引入的外来元数据变更。
        let final_inventory =
            self.capture_removal_inventory_locked(&original.original.plugin_id)?;
        validate_inventory_membership(original, &final_inventory)?;
        if current.packages != final_inventory.packages
            || current.credentials != final_inventory.credentials
        {
            return Err(Error::new(
                Code::Conflict,
                "Original removal history changed during terminal confirmation.",
            ));
        }
        Ok(())
    }
}

/// Require exact full package metadata and monotonic cleanup phases without adopting an unowned promotion or regression.
/// 要求精确完整包元数据及单调清理阶段，不接纳非自有晋级或回退。
/// expected/current are actual producer records; returns unit or a conflict without reading files or granting ownership.
/// expected／current 为实际生产者记录；返回空值或冲突，不读取文件或授予所有权。
fn validate_package_progress(
    expected: &GenerationRecord,
    current: &GenerationRecord,
) -> Result<()> {
    // All identity fields and schema stay frozen; only explicitly allowed original cleanup transitions remain mutable.
    // 全部身份字段及结构版本保持固定；仅明确许可原清理转换保持可变。
    let progress = match expected.state {
        GenerationState::Preparing => matches!(
            current.state,
            GenerationState::Preparing | GenerationState::Collecting | GenerationState::Collected
        ),
        GenerationState::Prepared => matches!(
            current.state,
            GenerationState::Prepared | GenerationState::Collecting | GenerationState::Collected
        ),
        GenerationState::Collecting => matches!(
            current.state,
            GenerationState::Collecting | GenerationState::Collected
        ),
        GenerationState::Collected => current.state == GenerationState::Collected,
    };
    if expected.schema_version != current.schema_version
        || expected.identity != current.identity
        || !progress
    {
        return Err(Error::new(
            Code::Conflict,
            "Original removal's complete package history changed outside cleanup progress.",
        ));
    }
    Ok(())
}

/// Compare complete deterministic membership before terminal physical checks; captured credentials are compared fully by the sole credential validator.
/// 终态物理检查之前比较完整确定性成员关系；捕获凭据由唯一凭据校验器完整比较。
/// original/current are actual original and current census; returns unit or a conflict without admitting any effect.
/// original／current 为实际原实例及当前清单；返回空值或冲突，不准入任何效果。
fn validate_inventory_membership(
    original: &VulcanPluginDeactivationReservation,
    current: &inventory::RemovalInventory,
) -> Result<()> {
    if original.inventory.packages.len() != current.packages.len()
        || original.inventory.credentials.len() != current.credentials.len()
    {
        return Err(Error::new(
            Code::Conflict,
            "Original removal history membership changed.",
        ));
    }
    for (expected, actual) in original.inventory.packages.iter().zip(&current.packages) {
        validate_package_progress(expected, actual)?;
    }
    for (expected, actual) in original
        .inventory
        .credentials
        .iter()
        .zip(&current.credentials)
    {
        if expected.revision() != actual.revision() {
            return Err(Error::new(
                Code::Conflict,
                "Original removal credential history membership changed.",
            ));
        }
    }
    Ok(())
}

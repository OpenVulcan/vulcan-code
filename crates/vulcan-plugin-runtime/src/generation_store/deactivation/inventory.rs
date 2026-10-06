//! Removal freezes every actual namespace journal before callbacks without retaining extra reader Files.
//! 移除在回调之前固定每个实际命名空间日志，不保留额外读者文件。

use super::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;

/// The sole retained-history encoding budget bounds one removal original's metadata, independently of each record's decoding limit.
/// 唯一保留历史编码额度约束单个移除原实例元数据，独立于每记录解码限制。
/// Oversized histories fail before any callback, publication or deletion and retain all producer records unchanged.
/// 超限历史在任何回调、发布或删除之前失败，并保持全部生产者记录未变。
pub(in crate::generation_store) const MAX_REMOVAL_INVENTORY_BYTES: usize = 8 * 1024 * 1024;

/// Pure complete producer metadata is retained under the one original namespace; this inventory grants no physical collection authority.
/// 纯完整生产者元数据在单个原命名空间内保留；此清单不授予物理回收权威。
pub(super) struct RemovalInventory {
    /// Every actual package marker includes prepared, unfinished and terminal histories in deterministic identity order.
    /// 每个实际包标记包含已准备、未完成及终态历史，按确定身份顺序排列。
    pub(super) packages: Vec<GenerationRecord>,
    /// Every actual final credential journal retains full keys, configured subset, parent binding and phase without secret values.
    /// 每个实际最终凭据日志保留完整键、已配置子集、父绑定及阶段，不包含秘密值。
    pub(super) credentials: Vec<VulcanPluginCredentialRecord>,
    /// Identity-indexed actual package checkpoints prevent an original from replaying deletion after a marker regression.
    /// 按身份索引实际包检查点阻止原实例在标记回退之后重放删除。
    pub(super) package_progress: BTreeMap<String, progress::CleanupProgress>,
    /// Revision-indexed actual credential checkpoints survive backend errors and panic without creating replacement owners.
    /// 按修订索引实际凭据检查点跨后端错误及 panic 存续，不创建替代所有者。
    pub(super) credential_progress:
        BTreeMap<VulcanPluginCredentialRevision, progress::CleanupProgress>,
}

/// One explicit budget counter covers both history kinds without adopting partial snapshots after errors.
/// 单个明确额度计数器覆盖两类历史，错误之后不接纳部分快照。
struct InventoryBudget {
    /// Canonical encoded bytes retained by this actual snapshot only.
    /// 仅此实际快照保留的规范编码字节。
    encoded_bytes: usize,
}

impl InventoryBudget {
    /// Admit one actual record while reserving the largest encoded cleanup phase; record/phase/progress are actual schema values.
    /// 准入单个实际记录，同时预留最大编码清理阶段；record／phase／progress 为实际结构值。
    /// Returns explicit overflow errors before retention, so legitimate later cleanup progress cannot exceed an accepted original budget.
    /// 保留之前返回明确溢出错误，使随后合法清理进展不能超出已接纳原额度。
    fn retain<P: Serialize>(
        &mut self,
        record: &impl Serialize,
        phase: &P,
        progress: &[P],
    ) -> Result<()> {
        files::validate_record_phase_capacity(record, phase, progress)?;
        // Actual typed metadata encoding accounts for every retained field without trusting file length or guessed object size.
        // 实际类型化元数据编码计入每个保留字段，不信任文件长度或猜测对象大小。
        let bytes = serde_json::to_vec(record)
            .map_err(|_| Error::invalid("Cannot encode original removal history metadata."))?;
        // The same serde phase encoding derives exact growth allowance rather than copying a numeric estimate.
        // 相同阶段序列化编码推导精确增长额度，而非复制数值估计。
        let current_size = serde_json::to_vec(phase)
            .map_err(|_| Error::invalid("Cannot encode original removal history phase."))?
            .len();
        // All schema-declared phases are bounded metadata, not alternate recovery owners or guessed current state.
        // 全部结构声明阶段为有界元数据，而非替代恢复所有者或猜测当前状态。
        let largest_size = progress.iter().try_fold(current_size, |size, candidate| {
            serde_json::to_vec(candidate)
                .map(|encoded| size.max(encoded.len()))
                .map_err(|_| Error::invalid("Cannot encode original removal history progress."))
        })?;
        // Reserve the phase's maximum encoding in this actual record while leaving the frozen producer bytes/fields unchanged.
        // 在此实际记录内预留阶段最大编码，同时保持固定生产者字节／字段未变。
        let retained_size = bytes
            .len()
            .checked_sub(current_size)
            .and_then(|size| size.checked_add(largest_size))
            .ok_or_else(|| Error::invalid("Original removal history phase budget is invalid."))?;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(retained_size)
            .filter(|size| *size <= MAX_REMOVAL_INVENTORY_BYTES)
            .ok_or_else(|| {
                Error::invalid("Original removal history exceeds its retained metadata budget.")
            })?;
        Ok(())
    }
}

impl VulcanPluginGenerationStore {
    /// Capture every actual package and credential marker under the caller's same retained namespace, without publishing or acquiring pins.
    /// 在调用方相同保留命名空间内捕获每个实际包及凭据标记，不发布或取得固定。
    /// plugin_id is the actual original namespace; returns complete bounded metadata or an error with no partial owner/effects.
    /// plugin_id 为实际原命名空间；返回完整有界元数据，或不产生部分所有者／效果的错误。
    pub(super) fn capture_removal_inventory_locked(
        &self,
        plugin_id: &str,
    ) -> Result<RemovalInventory> {
        self.validate_layout()?;
        crate::catalog::validate_identifier(plugin_id, "removal plugin id")
            .map_err(Error::invalid)?;
        // The actual active source requires an existing ordinary package namespace; its absence is corruption rather than an empty history.
        // 实际有效来源要求既有普通包命名空间；其缺失为损坏，而非空历史。
        let root = self.root.join("packages").join(plugin_id);
        if !files::exists(&root, true)? {
            return Err(Error::invalid(
                "Original removal package namespace is missing.",
            ));
        }
        // The one combined budget is consumed only by validated complete producer records.
        // 单个合并额度仅由已验证完整生产者记录消耗。
        let mut budget = InventoryBudget { encoded_bytes: 0 };
        // No package reader pins are stored, so this original cannot block its own later physical collection.
        // 不存储包读者固定，因此此原实例不能阻挡自身随后物理回收。
        let mut packages = Vec::new();
        for entry in fs::read_dir(&root)
            .map_err(|error| Error::io("Enumerate original removal package histories", error))?
        {
            // Directory enumeration errors and noncanonical names are explicit rather than silently excluded histories.
            // 目录枚举错误及非规范名称为明确错误，而非静默排除的历史。
            let entry = entry
                .map_err(|error| Error::io("Read original removal package history entry", error))?;
            // The real marker reader supplies version/fingerprint; neither is guessed from current selection.
            // 真实标记读取器提供版本／指纹；两者均不从当前选择猜测。
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::invalid("Original removal package name is not UTF-8."))?;
            // Share every ordinary-directory, full identity and schema check with existing actual generation readers.
            // 与既有实际代次读取者共享每个普通目录、完整身份及结构版本检查。
            let (_, record) = self.read_generation_directory_locked(plugin_id, &name)?;
            budget.retain(&record, &record.state, &GenerationState::ENCODED_PHASES)?;
            packages.push(record);
        }
        packages.sort_by(|left, right| {
            left.identity
                .generation_id
                .cmp(&right.identity.generation_id)
        });
        // The ordinary public enumeration delegates to this same implementation instead of reopening the original namespace File.
        // 普通公开枚举委托此相同实现，而非重新打开原命名空间文件。
        let revisions = self.credential_revisions_locked(plugin_id)?;
        // Complete journal records remain usable as evidence even after their physical package has been collected.
        // 即使物理包已回收，完整日志记录仍可作为证据使用。
        let mut credentials = Vec::new();
        for revision in revisions {
            // An enumerated final journal must actually exist and pass the sole semantic validator.
            // 已枚举最终日志必须实际存在并通过唯一语义校验器。
            let record = self
                .read_credential_record_locked(plugin_id, &revision)?
                .ok_or_else(|| {
                    Error::invalid("Original removal credential history disappeared.")
                })?;
            budget.retain(
                &record,
                &record.phase(),
                &VulcanPluginCredentialPhase::ENCODED_PHASES,
            )?;
            credentials.push(record);
        }
        // These pure watermarks derive only from fully validated actual records, before the new original can admit effects.
        // 这些纯水位仅从完整验证实际记录推导，先于新原实例能够准入效果。
        let package_progress = packages
            .iter()
            .map(|record| {
                (
                    record.identity.generation_id.clone(),
                    progress::CleanupProgress::package(record.state),
                )
            })
            .collect();
        // The same schema-derived initialization accounts for unfinished and already terminal historical journals.
        // 相同结构推导初始化计入未完成及已终态历史日志。
        let credential_progress = credentials
            .iter()
            .map(|record| {
                (
                    record.revision().clone(),
                    progress::CleanupProgress::credential(record.phase()),
                )
            })
            .collect();
        Ok(RemovalInventory {
            packages,
            credentials,
            package_progress,
            credential_progress,
        })
    }
}

impl VulcanPluginDeactivationReservation {
    /// Borrow every original package identity and captured phase without returning pins or granting deletion.
    /// 借用每个原包身份及捕获阶段，不返回固定或授予删除。
    /// No parameters; includes the selected package and retained unfinished/terminal histories from the same original File.
    /// 无参数；包含来自相同原文件的已选包及保留未完成／终态历史。
    pub fn package_inventory(
        &self,
    ) -> impl Iterator<Item = (&VulcanPluginGenerationIdentity, VulcanPluginGenerationPhase)> {
        self.inventory
            .packages
            .iter()
            .map(|record| (&record.identity, record.state))
    }

    /// Borrow every complete original credential journal without secure values or replacement cleanup authority.
    /// 借用每个完整原凭据日志，不携带安全值或替代清理权威。
    /// No parameters; includes historical revisions even when the current selection binds no credentials.
    /// 无参数；即使当前选择未绑定凭据，也包含历史修订。
    pub fn credential_inventory(&self) -> &[VulcanPluginCredentialRecord] {
        &self.inventory.credentials
    }
}

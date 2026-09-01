//! Process-local source lifetimes owned by the existing per-index transaction lock, never persisted as draft intent.
//! 由既有逐索引事务锁持有的进程内来源寿命，绝不持久化为草稿意图。

use super::*;
use std::sync::{Arc, Mutex};

/// Mutable state behind the existing per-index mutex; live readers protect bytes without blocking metadata transactions.
/// 既有逐索引互斥锁后的可变状态；活动读取者保护字节，但不阻塞元数据事务。
#[derive(Debug, Default)]
pub(super) struct AttachmentIndexTransaction {
    /// Counts actual in-process source owners, independently of persisted draft/message references.
    /// 统计真实进程内来源所有者，独立于持久草稿及消息引用。
    source_holds: BTreeMap<String, usize>,
}

impl AttachmentIndexTransaction {
    /// Reports whether exact `asset_id` still has a physical source owner; called while this index mutex is held.
    /// 报告精确`asset_id`是否仍有实体来源所有者；调用时持有本索引互斥锁。
    pub(super) fn source_is_held(&self, asset_id: &str) -> bool {
        self.source_holds.contains_key(asset_id)
    }

    /// Retains one validated `asset_id` under the index transaction; returns an explicit count-overflow error.
    /// 在索引事务内保留一个已校验`asset_id`；数量溢出时显式报错。
    pub(super) fn retain_source(&mut self, asset_id: &str) -> Result<(), String> {
        // A missing count means this is the first physical reader, not an alternate source identity.
        // 计数缺失表示这是首个实体读取者，而非替代来源身份。
        let count = self.source_holds.get(asset_id).copied().unwrap_or(0);
        let next = count
            .checked_add(1)
            .ok_or_else(|| String::from("Attachment source hold count overflow."))?;
        self.source_holds.insert(asset_id.to_owned(), next);
        Ok(())
    }

    /// Releases one exact reader and returns true only for the last reader; an absent owner is an invariant error.
    /// 释放一个精确读取者，仅在最后一个读取者时返回真；所有者缺失是不变量错误。
    fn release_source(&mut self, asset_id: &str) -> Result<bool, String> {
        let count = self
            .source_holds
            .get_mut(asset_id)
            .ok_or_else(|| String::from("Attachment source hold is not registered."))?;
        if *count == 1 {
            self.source_holds.remove(asset_id);
            Ok(true)
        } else {
            *count -= 1;
            Ok(false)
        }
    }
}

/// Owns one non-cloneable physical source lifetime; it grants neither draft provenance nor upload receipt authority.
/// 持有一个不可克隆的实体来源寿命；它不授予草稿来源或上传回执权威。
#[derive(Debug)]
pub struct AttachmentSourceHold {
    /// Exact index root already validated against the canonical source record.
    /// 已根据规范来源记录校验的精确索引根。
    asset_root: PathBuf,
    /// Exact indexed asset protected by this hold.
    /// 本持有所保护的精确索引资产。
    asset_id: String,
    /// Strong ownership keeps the existing weak registry entry alive through asynchronous source use.
    /// 强所有权在异步来源使用期间保持既有弱注册条目存活。
    transaction: Arc<Mutex<AttachmentIndexTransaction>>,
}

impl AttachmentSourceHold {
    /// Wraps one already-retained source; callers must increment its count atomically with provenance validation.
    /// 包装一个已经保留的来源；调用方必须与来源校验原子地增加计数。
    pub(super) fn new(
        asset_root: PathBuf,
        asset_id: String,
        transaction: Arc<Mutex<AttachmentIndexTransaction>>,
    ) -> Self {
        Self {
            asset_root,
            asset_id,
            transaction,
        }
    }

    /// Releases the exact physical count and removes an orphan only if all persisted and physical references are gone.
    /// 释放精确实体计数，仅在全部持久及实体引用消失后删除孤立资产。
    /// Returns cleanup errors without inventing a durable lease or deleting another owner's source.
    /// 返回清理错误，不虚构持久租约，也不删除其他所有者的来源。
    fn release(&self) -> Result<(), String> {
        // Keep the same mutex through the last-reader decision, reference check and existing safe deletion transaction.
        // 在最后读取者判断、引用检查及既有安全删除事务期间保持同一互斥锁。
        let mut transaction = self
            .transaction
            .lock()
            .map_err(|_| String::from("Attachment source owner is unavailable."))?;
        if !transaction.release_source(&self.asset_id)? {
            return Ok(());
        }
        let mut document =
            lifecycle::load_attachment_asset_index_document_for_mutation(&self.asset_root)?;
        let index = document
            .assets
            .iter()
            .position(|record| record.asset_id == self.asset_id)
            .ok_or_else(|| {
                String::from("A physically held attachment disappeared from its index.")
            })?;
        if !lifecycle::attachment_record_has_refs(&document.assets[index]) {
            lifecycle::discard_attachment_record_at(&self.asset_root, &mut document, index)?;
        }
        Ok(())
    }
}

impl Drop for AttachmentSourceHold {
    /// Settles the physical lifetime on every return, cancellation or unwind; failures leave recoverable index/file state.
    /// 在全部返回、取消或异常展开路径结算实体寿命；失败时保留可恢复的索引和文件状态。
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            eprintln!(
                "[attachment-source] failed to release `{}`: {error}",
                self.asset_id
            );
        }
    }
}

/// Returns the process-local hold fact for one canonical attachment; available only to this module's regression tests.
/// 返回单条规范附件的进程内持有事实；仅供本模块回归测试使用。
#[cfg(test)]
pub(super) fn test_attachment_source_is_held(
    attachment: &ConversationAttachment,
) -> Result<bool, String> {
    let root = resolve_asset_root_from_attachment(attachment)
        .ok_or_else(|| String::from("test source has no root"))?;
    let transaction = attachment_index_transaction_lock(&root)?;
    let guard = transaction
        .lock()
        .map_err(|_| String::from("test source owner unavailable"))?;
    let asset_id = attachment
        .asset_id
        .as_deref()
        .ok_or_else(|| String::from("test source has no id"))?;
    Ok(guard.source_is_held(asset_id))
}

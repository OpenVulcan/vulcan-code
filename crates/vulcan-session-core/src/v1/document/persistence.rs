//! Immutable flush candidates, durable acknowledgements, and revision-scoped event patches.
//! 不可变落盘候选、持久化确认及修订范围事件补丁。

use super::SessionDocument;
use crate::v1::config::MetadataDocument;
use crate::v1::context::{CompactionNode, RuntimeLock};
use crate::v1::errors::DomainError;
use crate::v1::event::ConversationEvent;
use crate::v1::ids::{CompactionId, Seq};
use std::collections::BTreeSet;

/// One durable file that can become dirty.
/// 一个可能变为 dirty 的持久化文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirtyFile {
    /// `metadata.json`.
    /// `metadata.json`。
    Metadata,
    /// `runtime.json`.
    /// `runtime.json`。
    Runtime,
    /// `compactions.json`.
    /// `compactions.json`。
    Compactions,
    /// The active tail segment file.
    /// 活动尾段分片文件。
    ActiveTail,
}

/// Owned, immutable flush candidate set captured at one revision.
/// 在某一个修订号捕获的自有、不可变 flush 候选集。
#[derive(Debug, Clone)]
pub struct FlushCandidate {
    /// Revision this candidate covers.
    /// 该候选覆盖的修订号。
    pub revision: u64,
    /// Files that must be re-published.
    /// 必须重新发布的文件。
    pub dirty: Vec<DirtyFile>,
    /// Exact event seq values changed since the last durable revision, sorted
    /// in ascending order for the logical live-patch channel.
    /// 自最近持久化修订以来真实变化的事件 seq，按升序排列并供逻辑实时补丁通道使用。
    pub changed_event_seqs: Vec<Seq>,
    /// Metadata document candidate.
    /// metadata 文档候选。
    pub metadata: MetadataDocument,
    /// Runtime lock candidate.
    /// runtime 锁定候选。
    pub runtime_lock: RuntimeLock,
    /// Compaction nodes candidate.
    /// 压缩节点候选。
    pub compactions: Vec<CompactionNode>,
    /// Active tail events only. Sealed segments are immutable and never
    /// rewritten by a flush, so the candidate never copies the full history
    /// (plan 5.2.15).
    /// 仅活动尾段事件。封口分片不可变、flush 永不重写，因此候选从不复制
    /// 完整历史(计划 5.2.15)。
    pub tail_events: Vec<ConversationEvent>,
}

/// Exact event patch produced by one in-memory document revision.
/// 一次内存文档修订产生的精确事件补丁。
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRevisionReceipt {
    /// Exact in-memory revision owning this patch.
    /// 拥有该补丁的精确内存修订号。
    pub revision: u64,
    /// Changed event seq values in ascending order.
    /// 按升序排列的已变化事件 seq。
    pub changed_event_seqs: Vec<Seq>,
    /// Complete current snapshots of the changed events in the same order.
    /// 与 seq 同序的已变化事件完整当前快照。
    pub changed_events: Vec<ConversationEvent>,
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Applies only the physical storage pointers published by one successful
    /// flush while preserving newer logical metadata already mutated in memory.
    /// 仅应用一次成功 flush 发布的物理存储指针，同时保留内存中已经产生的更新
    /// 逻辑 metadata。
    ///
    /// # Parameters
    /// # 参数
    /// `published` is the exact metadata document atomically committed by the
    /// storage writer.
    /// `published` 是存储写入器已经原子提交的精确 metadata 文档。
    ///
    /// # Returns
    /// # 返回值
    /// `Ok(())` when the pointer advance is monotonic and belongs to this
    /// session; otherwise a structured domain error.
    /// 当指针推进单调且属于当前会话时返回 `Ok(())`；否则返回结构化领域错误。
    pub fn apply_published_storage_metadata(
        &mut self,
        published: &MetadataDocument,
    ) -> Result<(), DomainError> {
        if published.session_id != self.metadata.session_id {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "metadata",
                field: "session_id",
            });
        }
        if published.segment_generation < self.metadata.segment_generation {
            return Err(DomainError::SequenceViolation {
                expected: self.metadata.segment_generation,
                actual: published.segment_generation,
            });
        }
        if published.active_context_generation < self.metadata.active_context_generation {
            return Err(DomainError::SequenceViolation {
                expected: self.metadata.active_context_generation,
                actual: published.active_context_generation,
            });
        }
        self.metadata.segment_generation = published.segment_generation;
        self.metadata.active_context_generation = published.active_context_generation;
        self.metadata.split_list = published.split_list.clone();
        self.segment_catalog = published.split_list.clone();
        Ok(())
    }

    /// Captures the exact event patch recorded for one in-memory revision
    /// without issuing a flush candidate or changing durability state.
    /// 捕获一个内存修订记录的精确事件补丁，且不发出 flush 候选或改变持久化状态。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is the exact revision returned immediately after a successful
    /// domain mutation.
    /// `revision` 是一次成功领域修改后立即返回的精确修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the sorted changed seq values and their complete current event
    /// snapshots. Metadata-only revisions return an empty patch.
    /// 返回排序后的变化 seq 及对应完整当前事件快照；仅 metadata 修订返回空补丁。
    pub fn memory_revision_receipt(
        &self,
        revision: u64,
    ) -> Result<MemoryRevisionReceipt, DomainError> {
        let changed_event_seqs = self
            .changed_event_seqs_by_revision
            .get(&revision)
            .cloned()
            .ok_or(DomainError::ReferenceViolation {
                kind: "memory_revision_receipt",
                value: revision,
            })?;
        let changed_events = self.clone_resident_events_at_seqs(&changed_event_seqs)?;
        Ok(MemoryRevisionReceipt {
            revision,
            changed_event_seqs,
            changed_events,
        })
    }

    /// Confirms that one candidate revision became durable and clears the
    /// dirty flags fully covered by that revision.
    /// 确认某个候选修订已持久化，并清除该修订完全覆盖的 dirty 标记。
    ///
    /// Only revisions actually issued by [`Self::prepare_flush_candidates`]
    /// are accepted; a future or forged revision is rejected (plan 6.2.1).
    /// A revision already covered by a later confirmation is a no-op, keeping
    /// out-of-order confirmations from falsely failing. Files dirtied after
    /// the candidate was copied keep their dirty flag.
    /// 只接受 [`Self::prepare_flush_candidates`] 实际发出的修订号；未来或被
    /// 伪造的修订号被拒绝(计划 6.2.1)。已被更晚确认覆盖的修订号是无操作，
    /// 避免乱序确认误报失败。候选复制之后再次变脏的文件保持 dirty 标记。
    pub fn advance_durable(&mut self, revision: u64) -> Result<(), DomainError> {
        if revision > self.memory_revision {
            return Err(DomainError::RevisionNeverIssued {
                revision,
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        if revision <= self.durable_revision {
            return Ok(());
        }
        if !self.issued_revisions.contains(&revision) {
            return Err(DomainError::RevisionNeverIssued {
                revision,
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        let covered: Vec<u64> = self.issued_revisions.range(..=revision).copied().collect();
        for issued in covered {
            self.issued_revisions.remove(&issued);
        }
        // Advance the published compaction pointer to the newest confirmed
        // snapshot at or before this revision.
        // 把已发布压缩指针推进到该修订号及之前最新的已确认快照。
        let confirmed: Vec<(u64, Option<CompactionId>)> = self
            .pending_compaction_publish
            .range(..=revision)
            .map(|(rev, pointer)| (*rev, *pointer))
            .collect();
        for (rev, _) in &confirmed {
            self.pending_compaction_publish.remove(rev);
        }
        if let Some((_, pointer)) = confirmed.last() {
            self.published_compaction_id = *pointer;
        }
        self.durable_revision = revision;
        let stale: Vec<DirtyFile> = self
            .dirty_since
            .iter()
            .filter(|(_, since)| **since <= revision)
            .map(|(file, _)| *file)
            .collect();
        for file in stale {
            self.dirty_since.remove(&file);
        }
        let durable_changes: Vec<u64> = self
            .changed_event_seqs_by_revision
            .range(..=revision)
            .map(|(changed_revision, _)| *changed_revision)
            .collect();
        for changed_revision in durable_changes {
            self.changed_event_seqs_by_revision
                .remove(&changed_revision);
        }
        Ok(())
    }

    /// Removes a historical tail by truncating every event at or after the
    /// target seq, then rebuilding derived state (edit/rewrite support).
    /// 通过截断目标 seq 及其后的全部事件删除历史尾部，然后重建派生状态
    /// (编辑/维护重写支持)。
    ///
    /// This is a maintenance rewrite entry point used by the generation
    /// builder; it never leaves seq holes because the retained prefix is
    /// already contiguous.
    /// 这是 generation 构建器使用的维护性重写入口；保留前缀本身连续，因此
    /// 永远不会留下 seq 空洞。
    pub fn truncate_after(
        &mut self,
        keep_until_seq: Seq,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let keep_until = keep_until_seq.value();
        let count = self
            .events
            .iter()
            .filter(|event| event.seq.value() <= keep_until)
            .count();
        if count == self.events.len() {
            return Ok(false);
        }
        let mut staged = self.clone();
        staged.events.truncate(count);
        staged.rebuild_derived_state()?;
        staged.bump_revision(&[DirtyFile::Metadata, DirtyFile::ActiveTail], now_ms, &[]);
        *self = staged;
        Ok(true)
    }

    /// Copies the complete immutable candidates of every dirty file at one
    /// revision (plan 6.2.1 flush contract).
    /// 在某一个修订号复制全部 dirty 文件的完整不可变候选(计划 6.2.1 flush
    /// 契约)。
    ///
    /// The revision is captured from `memory_revision` inside this call, so
    /// candidate revision, dirty list, and candidate content always come from
    /// one atomic snapshot and no caller can construct a future revision. The
    /// event candidate copies only the active tail; flushed sealed segments
    /// are never rewritten. The caller takes this owned snapshot, releases the
    /// session state lock, and hands it to the single physical writer; the
    /// document keeps mutating freely meanwhile, bumping to higher revisions
    /// that this candidate cannot clear.
    /// 修订号在此调用内部从 `memory_revision` 捕获，因此候选修订号、dirty
    /// 列表与候选内容总是来自同一个原子快照，任何调用方都无法构造未来修订号。
    /// 事件候选只复制活动尾段；已封口分片从不重写。调用方取得这份自有快照、
    /// 释放会话状态锁，并把它交给唯一物理写入器；文档在此期间可以继续自由
    /// 变更，递增到该候选无法清除的更高修订号。
    pub fn prepare_flush_candidates(&mut self) -> Result<Option<FlushCandidate>, DomainError> {
        if self.dirty_since.is_empty() {
            return Ok(None);
        }
        let revision = self.memory_revision;
        let tail_start = self.active_tail_start();
        let tail_index = tail_start
            .value()
            .checked_sub(self.resident_start_seq)
            .ok_or(DomainError::SequenceViolation {
                expected: self.resident_start_seq,
                actual: tail_start.value(),
            })? as usize;
        let tail_start_value = tail_start.value();
        assert!(
            tail_index <= self.events.len(),
            "active tail start {tail_start_value} exceeds the event stream length {}",
            self.events.len()
        );
        let tail_events = self.events[tail_index..].to_vec();
        let changed_event_seqs: Vec<Seq> = self
            .changed_event_seqs_by_revision
            .range((self.durable_revision + 1)..=revision)
            .flat_map(|(_, changed)| changed.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.issued_revisions.insert(revision);
        // Remember which compaction pointer becomes durable if this revision
        // is confirmed: only a runtime write advances the published pointer.
        // 记住该修订号被确认后哪个压缩指针变为持久化：只有 runtime 写入才会
        // 推进已发布指针。
        let published_if_confirmed = if self.dirty_since.contains_key(&DirtyFile::Runtime) {
            self.runtime_lock.current_compaction_id
        } else {
            self.published_compaction_id
        };
        self.pending_compaction_publish
            .insert(revision, published_if_confirmed);
        Ok(Some(FlushCandidate {
            revision,
            dirty: self.dirty_since.keys().copied().collect(),
            changed_event_seqs,
            metadata: self.metadata.clone(),
            runtime_lock: self.runtime_lock.clone(),
            compactions: self.compactions.clone(),
            tail_events,
        }))
    }

    /// Returns the first seq of the active tail from the split map.
    /// 返回分片映射中活动尾段的起始 seq。
    ///
    /// The last split entry is the active tail; an empty split map means the
    /// whole stream is still the tail (plan 5.2.15).
    /// 最后一个分片条目是活动尾段；分片映射为空说明整个流仍是尾段(计划
    /// 5.2.15)。
    pub(super) fn active_tail_start(&self) -> Seq {
        match self.metadata.split_list.last() {
            Some(last) => last.start_seq,
            None => Seq::try_new(1).expect("the first seq is always valid"),
        }
    }

    /// Clones exact resident events selected by already validated seq values.
    /// 按已经校验的 seq 精确克隆常驻事件。
    fn clone_resident_events_at_seqs(
        &self,
        seqs: &[Seq],
    ) -> Result<Vec<ConversationEvent>, DomainError> {
        seqs.iter()
            .map(|seq| {
                let index = seq.value().checked_sub(self.resident_start_seq).ok_or(
                    DomainError::ReferenceViolation {
                        kind: "resident_event_seq",
                        value: seq.value(),
                    },
                )? as usize;
                self.events
                    .get(index)
                    .filter(|event| event.seq == *seq)
                    .cloned()
                    .ok_or(DomainError::ReferenceViolation {
                        kind: "resident_event_seq",
                        value: seq.value(),
                    })
            })
            .collect()
    }
}

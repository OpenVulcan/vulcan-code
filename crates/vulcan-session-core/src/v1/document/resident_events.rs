//! Resident event stream with nested failure-atomic rollback frames.
//! 带嵌套失败原子回滚帧的常驻事件流。
//!
//! Atomic document mutations edit the live stream in place; each open frame
//! records only the stream length at entry and the first original copy of every
//! pre-existing event it replaces or truncates, so rollback cost follows the
//! size of the mutation instead of the size of the session.
//! 原子文档修改直接原地编辑当前事件流；每个打开的帧只记录进入时的流长度，以及
//! 被替换或截断的既有事件的首份原值，因此回滚成本取决于修改规模而非会话规模。

use std::collections::BTreeMap;
use std::ops::Deref;
#[cfg(test)]
use std::ops::DerefMut;

use crate::v1::event::ConversationEvent;

/// Contiguous resident events plus the rollback frames of open atomic mutations.
/// 连续常驻事件及打开中原子修改的回滚帧。
#[derive(Clone, Default)]
#[cfg_attr(test, derive(Debug))]
pub(super) struct ResidentEvents {
    /// Live events; `seq resident_start_seq + i` lives at index `i`.
    /// 当前事件；`seq resident_start_seq + i` 位于索引 `i`。
    items: Vec<ConversationEvent>,
    /// Open rollback frames, innermost last.
    /// 打开中的回滚帧，最内层位于末尾。
    frames: Vec<RollbackFrame>,
}

/// Minimal undo record of one open atomic mutation.
/// 一个打开中原子修改的最小撤销记录。
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
struct RollbackFrame {
    /// Stream length when the frame opened; later indices are new in this frame.
    /// 帧打开时的流长度；更大的索引都是本帧新增。
    base_len: usize,
    /// First original value of every pre-existing index changed inside this frame.
    /// 本帧内被改动的每个既有索引的首份原值。
    originals: BTreeMap<usize, ConversationEvent>,
}

impl ResidentEvents {
    /// Wraps an already validated contiguous stream without open frames.
    /// 包装一个已校验的连续事件流，不带打开的帧。
    pub(super) fn new(items: Vec<ConversationEvent>) -> Self {
        Self {
            items,
            frames: Vec::new(),
        }
    }

    /// Appends newly allocated events after the current tail.
    /// 在当前尾部之后追加新分配的事件。
    pub(super) fn extend(&mut self, events: impl IntoIterator<Item = ConversationEvent>) {
        self.items.extend(events);
    }

    /// Replaces the event at `index`, preserving its original for the innermost frame.
    /// 替换 `index` 处事件，并为最内层帧保留其原值。
    pub(super) fn replace(&mut self, index: usize, event: ConversationEvent) {
        let previous = std::mem::replace(&mut self.items[index], event);
        self.remember_original(index, previous);
    }

    /// Lends one event for in-place editing after preserving its original for the innermost frame.
    /// 为最内层帧保留原值后，借出一个事件供原地编辑。
    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut ConversationEvent> {
        let event = self.items.get(index)?;
        if let Some(frame) = self.frames.last_mut()
            && index < frame.base_len
            && !frame.originals.contains_key(&index)
        {
            frame.originals.insert(index, event.clone());
        }
        self.items.get_mut(index)
    }

    /// Releases the plain event vector of a stream with no open frames.
    /// 释放无打开帧事件流的普通事件向量。
    pub(super) fn into_vec(self) -> Vec<ConversationEvent> {
        debug_assert!(
            self.frames.is_empty(),
            "resident events released mid-mutation"
        );
        self.items
    }

    /// Drops every event at or after `len`, preserving pre-existing originals.
    /// 丢弃 `len` 及之后的全部事件，并保留既有事件原值。
    pub(super) fn truncate(&mut self, len: usize) {
        if len >= self.items.len() {
            return;
        }
        for (offset, previous) in self.items.split_off(len).into_iter().enumerate() {
            self.remember_original(len + offset, previous);
        }
    }

    /// Opens one nested rollback frame at the current stream length.
    /// 在当前流长度处打开一个嵌套回滚帧。
    pub(super) fn open_frame(&mut self) {
        self.frames.push(RollbackFrame {
            base_len: self.items.len(),
            originals: BTreeMap::new(),
        });
    }

    /// Keeps the innermost frame's edits and hands its undo record to the enclosing frame.
    /// 保留最内层帧的修改，并将其撤销记录交给外层帧。
    pub(super) fn commit_frame(&mut self) {
        let frame = self
            .frames
            .pop()
            .expect("committing a resident event frame requires an open frame");
        if let Some(outer) = self.frames.last_mut() {
            // Indices at or beyond the outer base are new to the outer frame and need no undo.
            // 位于外层基线及之后的索引对外层帧而言是新增，无需撤销。
            for (index, original) in frame.originals {
                if index < outer.base_len {
                    outer.originals.entry(index).or_insert(original);
                }
            }
        }
    }

    /// Restores the stream exactly as it was when the innermost frame opened.
    /// 将事件流精确恢复到最内层帧打开时的状态。
    pub(super) fn rollback_frame(&mut self) {
        let mut frame = self
            .frames
            .pop()
            .expect("rolling back a resident event frame requires an open frame");
        self.items.truncate(frame.base_len);
        // Truncated pre-existing events come back in index order before in-place restores.
        // 先按索引顺序补回被截断的既有事件，再执行原位恢复。
        while self.items.len() < frame.base_len {
            let index = self.items.len();
            let original = frame
                .originals
                .remove(&index)
                .expect("a truncated resident event keeps its original");
            self.items.push(original);
        }
        for (index, original) in frame.originals {
            self.items[index] = original;
        }
    }

    /// Records the first original of a pre-existing index in the innermost frame.
    /// 在最内层帧中记录既有索引的首份原值。
    fn remember_original(&mut self, index: usize, previous: ConversationEvent) {
        if let Some(frame) = self.frames.last_mut()
            && index < frame.base_len
        {
            frame.originals.entry(index).or_insert(previous);
        }
    }
}

impl Deref for ResidentEvents {
    type Target = Vec<ConversationEvent>;

    fn deref(&self) -> &Vec<ConversationEvent> {
        &self.items
    }
}

impl<'a> IntoIterator for &'a ResidentEvents {
    type Item = &'a ConversationEvent;
    type IntoIter = std::slice::Iter<'a, ConversationEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Tests corrupt resident events directly to exercise validation; production edits stay journaled.
/// 测试直接破坏常驻事件以覆盖校验；生产修改始终经过日志记录。
#[cfg(test)]
impl DerefMut for ResidentEvents {
    fn deref_mut(&mut self) -> &mut Vec<ConversationEvent> {
        &mut self.items
    }
}

#[cfg(test)]
impl PartialEq for ResidentEvents {
    /// Compares visible events only; open frames are transient mutation bookkeeping.
    /// 只比较可见事件；打开的帧只是暂时的修改记账。
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

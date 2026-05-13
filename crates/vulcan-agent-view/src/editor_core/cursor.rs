//! Cursor and selection primitives adapted from Ferrite (MIT).
//! 改编自 Ferrite（MIT）的光标与选择基础类型。

use std::cmp::Ordering;

/// Cursor position expressed as zero-based line and column coordinates.
/// 使用零基行列坐标表达的光标位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Cursor {
    /// Zero-based logical line index.
    /// 零基逻辑行索引。
    pub line: usize,
    /// Zero-based character column within the logical line.
    /// 当前逻辑行内的零基字符列索引。
    pub column: usize,
}

impl Cursor {
    /// Creates one cursor at the provided logical line and character column.
    /// 根据提供的逻辑行和字符列创建一个光标。
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// Returns one cursor positioned at the start of the document.
    /// 返回位于文档起始位置的一个光标。
    #[must_use]
    pub const fn start() -> Self {
        Self::new(0, 0)
    }
}

impl PartialOrd for Cursor {
    /// Compares two cursor positions using document order.
    /// 使用文档顺序比较两个光标位置。
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cursor {
    /// Orders cursor positions by line first and column second.
    /// 按行优先、列次之的方式对光标位置排序。
    fn cmp(&self, other: &Self) -> Ordering {
        match self.line.cmp(&other.line) {
            Ordering::Equal => self.column.cmp(&other.column),
            ordering => ordering,
        }
    }
}

/// Selection range defined by a fixed anchor and a moving head.
/// 由固定锚点和移动头部共同定义的选择区间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Selection {
    /// Fixed anchor position captured when selection starts.
    /// 选择开始时捕获的固定锚点位置。
    pub anchor: Cursor,
    /// Moving head position updated by dragging or keyboard expansion.
    /// 被拖拽或键盘扩展持续更新的移动头部位置。
    pub head: Cursor,
}

impl Selection {
    /// Creates one selection from the provided anchor and head positions.
    /// 根据提供的锚点和头部位置创建一个选择区间。
    #[must_use]
    pub const fn new(anchor: Cursor, head: Cursor) -> Self {
        Self { anchor, head }
    }

    /// Creates one collapsed selection whose anchor and head point to the same cursor.
    /// 创建一个锚点和头部重合的折叠选择区间。
    #[must_use]
    pub const fn collapsed(cursor: Cursor) -> Self {
        Self {
            anchor: cursor,
            head: cursor,
        }
    }

    /// Returns one collapsed selection positioned at the document start.
    /// 返回一个位于文档起始处的折叠选择区间。
    #[must_use]
    pub const fn start() -> Self {
        Self::collapsed(Cursor::start())
    }

    /// Returns whether the current selection spans at least one character.
    /// 返回当前选择是否至少覆盖了一个字符。
    #[must_use]
    pub fn is_range(&self) -> bool {
        self.anchor != self.head
    }

    /// Returns whether the current selection is collapsed to a caret.
    /// 返回当前选择是否折叠为一个插入点。
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns the head cursor that should be treated as the active caret.
    /// 返回应被视为活动插入点的头部光标。
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.head
    }

    /// Returns the ordered `(start, end)` pair regardless of drag direction.
    /// 无论拖拽方向如何都返回排好序的 `(start, end)` 区间端点。
    #[must_use]
    pub fn ordered(&self) -> (Cursor, Cursor) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Returns the document-earlier endpoint of the selection.
    /// 返回当前选择在文档顺序上更靠前的端点。
    #[must_use]
    pub fn start_pos(&self) -> Cursor {
        self.ordered().0
    }

    /// Returns the document-later endpoint of the selection.
    /// 返回当前选择在文档顺序上更靠后的端点。
    #[must_use]
    pub fn end_pos(&self) -> Cursor {
        self.ordered().1
    }

    /// Returns one new selection whose head moves to the provided cursor.
    /// 返回一个新的选择区间，并将头部移动到指定光标位置。
    #[must_use]
    pub fn with_head(self, head: Cursor) -> Self {
        Self {
            anchor: self.anchor,
            head,
        }
    }

    /// Extends the current selection to the provided head cursor.
    /// 将当前选择扩展到提供的头部光标位置。
    #[must_use]
    pub fn extend_to(self, head: Cursor) -> Self {
        self.with_head(head)
    }

    /// Returns one collapsed selection placed at the ordered start endpoint.
    /// 返回一个折叠到排序后起始端点的选择区间。
    #[must_use]
    pub fn collapse_to_start(self) -> Self {
        Self::collapsed(self.start_pos())
    }

    /// Returns one collapsed selection placed at the ordered end endpoint.
    /// 返回一个折叠到排序后结束端点的选择区间。
    #[must_use]
    pub fn collapse_to_end(self) -> Self {
        Self::collapsed(self.end_pos())
    }

    /// Returns whether the provided cursor falls inside the current selection.
    /// 返回给定光标位置是否落在当前选择范围之内。
    #[must_use]
    pub fn contains(&self, cursor: Cursor) -> bool {
        let (start, end) = self.ordered();
        cursor >= start && cursor < end
    }
}

#[cfg(test)]
mod tests {
    //! Cursor and selection regression tests for the vendored editor core.
    //! vendored 编辑器核心的光标与选择回归测试。

    use super::{Cursor, Selection};

    /// Verifies backward drag selections are reordered into document order.
    /// 验证反向拖拽的选择会被重新整理为文档顺序。
    #[test]
    fn orders_backward_selection_ranges() {
        let selection = Selection::new(Cursor::new(3, 7), Cursor::new(1, 2));

        assert_eq!(selection.start_pos(), Cursor::new(1, 2));
        assert_eq!(selection.end_pos(), Cursor::new(3, 7));
    }

    /// Verifies collapsed selections report the correct state flags.
    /// 验证折叠选择会返回正确的状态标记。
    #[test]
    fn reports_collapsed_state() {
        let selection = Selection::collapsed(Cursor::new(0, 0));

        assert!(selection.is_collapsed());
        assert!(!selection.is_range());
    }
}

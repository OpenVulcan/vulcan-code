//! Rope-backed text buffer adapted from Ferrite (MIT) for transcript selection work.
//! 改编自 Ferrite（MIT）的基于 Rope 的文本缓冲区，用于转录选择能力。

use crate::editor_core::{Cursor, Selection};
use ropey::Rope;
use std::borrow::Cow;

/// Rope-backed text buffer used by the future selectable transcript renderer.
/// 供未来可选转录渲染器使用的基于 Rope 的文本缓冲区。
#[derive(Debug, Clone)]
pub struct TextBuffer {
    /// Underlying rope storing the full selectable transcript content.
    /// 存储完整可选转录内容的底层 Rope。
    rope: Rope,
}

impl Default for TextBuffer {
    /// Creates one empty text buffer through the shared constructor.
    /// 通过统一构造函数创建一个空文本缓冲区。
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    /// Creates one empty text buffer with a single logical empty line.
    /// 创建一个空文本缓冲区，并包含单个逻辑空行。
    #[must_use]
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    /// Creates one text buffer from the provided full-string content.
    /// 根据提供的整段字符串内容创建一个文本缓冲区。
    #[must_use]
    pub fn from_string(content: &str) -> Self {
        Self {
            rope: Rope::from_str(content),
        }
    }

    /// Returns the total character count stored in the buffer.
    /// 返回缓冲区中存储的总字符数。
    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Returns the total byte count stored in the buffer.
    /// 返回缓冲区中存储的总字节数。
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Returns whether the buffer currently stores no characters.
    /// 返回当前缓冲区是否不包含任何字符。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len_chars() == 0
    }

    /// Returns the number of logical lines contained in the buffer.
    /// 返回缓冲区中包含的逻辑行数量。
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Returns the requested logical line, including its trailing newline when present.
    /// 返回指定的逻辑行，并在存在时保留结尾换行符。
    #[must_use]
    pub fn line(&self, line_index: usize) -> Cow<'_, str> {
        self.rope.line(line_index).into()
    }

    /// Returns the character offset at which the requested line begins.
    /// 返回指定逻辑行起始处的字符偏移量。
    #[must_use]
    pub fn line_to_char(&self, line_index: usize) -> usize {
        self.rope.line_to_char(line_index)
    }

    /// Returns the logical line containing the provided character index.
    /// 返回包含给定字符索引的逻辑行号。
    #[must_use]
    pub fn char_to_line(&self, char_index: usize) -> usize {
        self.rope.char_to_line(char_index)
    }

    /// Returns the byte offset corresponding to the provided character index.
    /// 返回与给定字符索引对应的字节偏移量。
    #[must_use]
    pub fn char_to_byte(&self, char_index: usize) -> usize {
        self.rope.char_to_byte(char_index)
    }

    /// Returns the character offset corresponding to the provided byte index.
    /// 返回与给定字节索引对应的字符偏移量。
    #[must_use]
    pub fn byte_to_char(&self, byte_index: usize) -> usize {
        self.rope.byte_to_char(byte_index)
    }

    /// Extracts one character-based slice from the buffer into an owned string.
    /// 从缓冲区中按字符范围提取一个拥有所有权的字符串切片。
    #[must_use]
    pub fn slice_chars(&self, start_char: usize, end_char: usize) -> String {
        let clamped_start = start_char.min(self.len_chars());
        let clamped_end = end_char.min(self.len_chars());
        if clamped_start >= clamped_end {
            return String::new();
        }
        self.rope.slice(clamped_start..clamped_end).to_string()
    }

    /// Extracts the currently selected text range into an owned string.
    /// 将当前选择区间提取为一个拥有所有权的字符串。
    #[must_use]
    pub fn selected_text(&self, selection: Selection) -> String {
        let start_char = self.cursor_to_char(selection.start_pos());
        let end_char = self.cursor_to_char(selection.end_pos());
        self.slice_chars(start_char, end_char)
    }

    /// Converts the provided logical cursor into its corresponding character offset.
    /// 将给定的逻辑光标位置转换为对应的字符偏移量。
    #[must_use]
    pub fn cursor_to_char(&self, cursor: Cursor) -> usize {
        let safe_line = cursor.line.min(self.line_count().saturating_sub(1));
        let line_start = self.line_to_char(safe_line);
        let line_text = self.line(safe_line);
        let line_char_count = line_text.chars().count();
        let safe_column = cursor.column.min(line_char_count);
        line_start + safe_column
    }

    /// Converts the provided character offset into a logical cursor position.
    /// 将给定的字符偏移量转换为逻辑光标位置。
    #[must_use]
    pub fn char_to_cursor(&self, char_index: usize) -> Cursor {
        let safe_char_index = char_index.min(self.len_chars());
        let line = self.char_to_line(safe_char_index);
        let line_start = self.line_to_char(line);
        Cursor::new(line, safe_char_index.saturating_sub(line_start))
    }
}

impl From<&str> for TextBuffer {
    /// Builds one buffer from a borrowed string slice.
    /// 根据借用字符串切片构建一个缓冲区。
    fn from(value: &str) -> Self {
        Self::from_string(value)
    }
}

impl From<String> for TextBuffer {
    /// Builds one buffer from an owned string by borrowing its contents.
    /// 通过借用其内容从拥有所有权的字符串构建一个缓冲区。
    fn from(value: String) -> Self {
        Self::from_string(&value)
    }
}

impl std::fmt::Display for TextBuffer {
    /// Writes the full rope content into the provided formatter.
    /// 将完整 Rope 内容写入提供的格式化器。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.rope)
    }
}

#[cfg(test)]
mod tests {
    //! Text buffer regression tests for vendored transcript-selection primitives.
    //! vendored 转录选择基础能力的文本缓冲区回归测试。

    use super::TextBuffer;
    use crate::editor_core::{Cursor, Selection};

    /// Verifies selected text extraction remains Unicode-safe across multi-byte characters.
    /// 验证跨多字节字符的选择文本提取仍然保持 Unicode 安全。
    #[test]
    fn extracts_selected_unicode_text() {
        let buffer = TextBuffer::from_string("第一行\nHello 世界");
        let selection = Selection::new(Cursor::new(1, 0), Cursor::new(1, 8));

        assert_eq!(buffer.selected_text(selection), "Hello 世界");
    }

    /// Verifies character and cursor conversions round-trip on multi-line content.
    /// 验证多行内容上的字符与光标转换可以完成往返。
    #[test]
    fn converts_between_character_offsets_and_cursors() {
        let buffer = TextBuffer::from_string("Alpha\nBeta");
        let cursor = Cursor::new(1, 2);
        let char_index = buffer.cursor_to_char(cursor);

        assert_eq!(char_index, 8);
        assert_eq!(buffer.char_to_cursor(char_index), cursor);
    }
}

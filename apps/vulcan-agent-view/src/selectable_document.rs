//! Selectable transcript projection built on top of the vendored editor core.
//! 构建在 vendored 编辑器核心之上的可选转录投影。

use crate::{
    AgentViewBlock, AgentViewDirectiveKind, AgentViewDocument, Cursor, Selection, TextBuffer,
};
use std::ops::Range;

/// One selectable transcript projection that keeps both text and block-range metadata.
/// 同时保留文本内容与区块范围元数据的一份可选转录投影。
#[derive(Debug, Clone)]
pub struct AgentViewSelectableDocument {
    /// Full buffer storing the whole selectable transcript text.
    /// 存储整份可选转录文本的完整缓冲区。
    buffer: TextBuffer,
    /// Block-level range metadata aligned with the underlying text buffer.
    /// 与底层文本缓冲区对齐的区块级范围元数据。
    blocks: Vec<AgentViewSelectableBlock>,
}

impl AgentViewSelectableDocument {
    /// Builds one selectable transcript projection from the structured document model.
    /// 根据结构化文档模型构建一份可选转录投影。
    #[must_use]
    pub fn from_document(document: &AgentViewDocument) -> Self {
        let mut full_text = String::new();
        let mut blocks = Vec::with_capacity(document.blocks.len());
        let mut current_char_offset = 0usize;

        for (block_index, block) in document.blocks.iter().enumerate() {
            if block_index > 0 {
                full_text.push_str("\n\n");
                current_char_offset += 2;
            }

            match block {
                AgentViewBlock::Markdown(markdown_block) => {
                    let body_text = markdown_block.selectable_text();
                    let block_start = current_char_offset;
                    full_text.push_str(&body_text);
                    current_char_offset += body_text.chars().count();

                    blocks.push(AgentViewSelectableBlock {
                        block_index,
                        kind: AgentViewSelectableBlockKind::Markdown,
                        full_char_range: block_start..current_char_offset,
                        title_char_range: None,
                        body_char_range: block_start..current_char_offset,
                        raw_source: markdown_block.raw_source.clone(),
                    });
                }
                AgentViewBlock::Directive(directive_block) => {
                    let block_start = current_char_offset;
                    let title_text = directive_block.selectable_title();
                    let title_start = current_char_offset;
                    full_text.push_str(&title_text);
                    current_char_offset += title_text.chars().count();
                    let title_char_range = title_start..current_char_offset;

                    let trimmed_body = directive_block.body.trim_end();
                    let body_char_range = if trimmed_body.is_empty() {
                        current_char_offset..current_char_offset
                    } else {
                        full_text.push('\n');
                        current_char_offset += 1;
                        let body_start = current_char_offset;
                        full_text.push_str(trimmed_body);
                        current_char_offset += trimmed_body.chars().count();
                        body_start..current_char_offset
                    };

                    blocks.push(AgentViewSelectableBlock {
                        block_index,
                        kind: AgentViewSelectableBlockKind::Directive(directive_block.kind.clone()),
                        full_char_range: block_start..current_char_offset,
                        title_char_range: Some(title_char_range),
                        body_char_range,
                        raw_source: directive_block.raw_source.clone(),
                    });
                }
            }
        }

        Self {
            buffer: TextBuffer::from(full_text),
            blocks,
        }
    }

    /// Returns the underlying text buffer used by the selectable transcript projection.
    /// 返回该可选转录投影使用的底层文本缓冲区。
    #[must_use]
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Returns the block metadata aligned with the underlying text buffer.
    /// 返回与底层文本缓冲区对齐的区块元数据。
    #[must_use]
    pub fn blocks(&self) -> &[AgentViewSelectableBlock] {
        &self.blocks
    }

    /// Returns one selection that spans the whole projected transcript text.
    /// 返回覆盖整份投影转录文本的一个选择区间。
    #[must_use]
    pub fn select_all(&self) -> Selection {
        if self.buffer.is_empty() {
            return Selection::start();
        }
        let end_cursor = self.buffer.char_to_cursor(self.buffer.len_chars());
        Selection::new(Cursor::start(), end_cursor)
    }

    /// Extracts the text covered by the provided selection from the projected transcript.
    /// 从投影转录中提取给定选择覆盖到的文本。
    #[must_use]
    pub fn selected_text(&self, selection: Selection) -> String {
        self.buffer.selected_text(selection)
    }

    /// Returns the block covering the provided character offset when one exists.
    /// 返回覆盖给定字符偏移位置的区块，若存在则返回该区块。
    #[must_use]
    pub fn block_at_char(&self, char_index: usize) -> Option<&AgentViewSelectableBlock> {
        self.blocks.iter().find(|block| {
            char_index >= block.full_char_range.start && char_index < block.full_char_range.end
        })
    }
}

/// One block-level metadata record aligned to the selectable transcript buffer.
/// 与可选转录缓冲区对齐的一条区块级元数据记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentViewSelectableBlock {
    /// Zero-based block index inside the original structured document.
    /// 原始结构化文档中的零基区块索引。
    pub block_index: usize,
    /// Semantic block kind used by the future custom renderer.
    /// 供未来自定义渲染器使用的语义区块类型。
    pub kind: AgentViewSelectableBlockKind,
    /// Character range covering the whole block title and body.
    /// 覆盖整个区块标题与正文的字符范围。
    pub full_char_range: Range<usize>,
    /// Character range covering the block title when the block has one.
    /// 当区块存在标题时覆盖其标题部分的字符范围。
    pub title_char_range: Option<Range<usize>>,
    /// Character range covering the block body.
    /// 覆盖区块正文部分的字符范围。
    pub body_char_range: Range<usize>,
    /// Exact original source used for copy-original interactions.
    /// 用于复制原文交互的精确原始源码。
    pub raw_source: String,
}

/// Block kind metadata used by the selectable transcript projection.
/// 可选转录投影使用的区块类型元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentViewSelectableBlockKind {
    /// Standard Markdown block rendered directly in the transcript flow.
    /// 直接渲染在转录流中的标准 Markdown 区块。
    Markdown,
    /// Directive block carrying one higher-level agent semantic kind.
    /// 携带一种更高层代理语义的指令区块。
    Directive(AgentViewDirectiveKind),
}

#[cfg(test)]
mod tests {
    //! Selectable-document regression tests for block-range projection behavior.
    //! 面向区块范围投影行为的可选文档回归测试。

    use super::{AgentViewSelectableBlockKind, AgentViewSelectableDocument};
    use crate::{
        AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument,
    };

    /// Verifies directive titles and bodies are mapped into distinct character ranges.
    /// 验证指令标题和正文会被映射到彼此独立的字符范围。
    #[test]
    fn maps_directive_title_and_body_ranges() {
        let mut document = AgentViewDocument::new();
        document.push_markdown("## 标题\n\n正文");
        document.push_directive(AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::ToolCall,
            AgentViewAttributes::new(),
            "cargo check -p vulcan-code-gui",
            ":::tool-call\ncargo check -p vulcan-code-gui\n:::",
            true,
        ));

        let selectable_document = AgentViewSelectableDocument::from_document(&document);
        let block = &selectable_document.blocks()[1];

        assert_eq!(
            block.kind,
            AgentViewSelectableBlockKind::Directive(AgentViewDirectiveKind::ToolCall)
        );
        assert!(block.title_char_range.is_some());
        assert!(block.body_char_range.start >= block.title_char_range.clone().unwrap().end);
        assert!(
            selectable_document
                .selected_text(selectable_document.select_all())
                .contains("[工具调用]")
        );
    }

    /// Verifies block lookup can map character offsets back to the owning block.
    /// 验证区块查询可以将字符偏移正确映射回所属区块。
    #[test]
    fn resolves_block_by_character_offset() {
        let mut document = AgentViewDocument::new();
        document.push_markdown("第一段");
        document.push_markdown("第二段");

        let selectable_document = AgentViewSelectableDocument::from_document(&document);
        let first_block = selectable_document.block_at_char(1).unwrap();
        let second_block = selectable_document
            .block_at_char(selectable_document.blocks()[1].full_char_range.start)
            .unwrap();

        assert_eq!(first_block.block_index, 0);
        assert_eq!(second_block.block_index, 1);
    }
}

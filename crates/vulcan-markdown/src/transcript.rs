//! Transcript-oriented Markdown helpers exposed as a stable public adapter boundary.
//! 作为稳定公共适配边界暴露的面向转录场景 Markdown 辅助工具。

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// Partitioned streaming Markdown source made of already sealed segments plus one still-growing active tail.
/// 由已封口片段和一个仍在增长的活动尾部组成的流式 Markdown 分段结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptStreamingPartition {
    /// Sealed Markdown source slices that can be rendered as stable immutable blocks.
    /// 可作为稳定不可变区块渲染的已封口 Markdown 源片段。
    pub sealed_segments: Vec<String>,
    /// Remaining Markdown source slice that should stay as the currently active tail block.
    /// 应保持为当前活动尾块的剩余 Markdown 源片段。
    pub active_tail: Option<String>,
}

impl TranscriptStreamingPartition {
    /// Returns whether the partition produced at least one sealed immutable segment before the active tail.
    /// 返回当前分段结果是否已经在活动尾部之前产出至少一个已封口不可变片段。
    pub fn has_sealed_segments(&self) -> bool {
        !self.sealed_segments.is_empty()
    }
}

/// Splits one streaming Markdown source into sealed top-level block segments plus the last still-growing tail segment.
/// 将一段流式 Markdown 源文本拆分为已封口顶层区块片段和最后一个仍在增长的尾部片段。
pub fn split_streaming_markdown_source(markdown: &str) -> TranscriptStreamingPartition {
    if markdown.is_empty() {
        return TranscriptStreamingPartition {
            sealed_segments: Vec::new(),
            active_tail: None,
        };
    }

    let block_ranges = collect_top_level_block_ranges(markdown);
    if block_ranges.len() <= 1 {
        return TranscriptStreamingPartition {
            sealed_segments: Vec::new(),
            active_tail: Some(markdown.to_string()),
        };
    }

    let last_range_start = block_ranges
        .last()
        .map(|range| range.start)
        .unwrap_or(markdown.len());
    let sealed_segments = block_ranges[..block_ranges.len() - 1]
        .iter()
        .map(|range| markdown[range.clone()].to_string())
        .collect::<Vec<_>>();
    let active_tail = markdown
        .get(last_range_start..)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string);

    TranscriptStreamingPartition {
        sealed_segments,
        active_tail,
    }
}

/// Collects the exact source ranges of all top-level block nodes emitted by pulldown-cmark for one Markdown source slice.
/// 收集 pulldown-cmark 为某段 Markdown 源文本发出的全部顶层区块节点精确源码范围。
fn collect_top_level_block_ranges(markdown: &str) -> Vec<Range<usize>> {
    let mut block_ranges = Vec::new();
    let mut block_depth = 0usize;
    let mut current_start = None::<usize>;

    for (event, source) in Parser::new_ext(markdown, transcript_partition_options()).into_offset_iter()
    {
        match event {
            Event::Start(tag) if is_structural_block_tag(&tag) => {
                if block_depth == 0 {
                    current_start = Some(source.start);
                }
                block_depth += 1;
            }
            Event::End(tag_end) if is_structural_block_end(&tag_end) => {
                if block_depth == 0 {
                    continue;
                }
                block_depth -= 1;
                if block_depth == 0
                    && let Some(start) = current_start.take()
                    && start < source.end
                {
                    block_ranges.push(start..source.end);
                }
            }
            Event::Rule if block_depth == 0 => {
                block_ranges.push(source.start..source.end);
            }
            _ => {}
        }
    }

    if block_ranges.is_empty() && !markdown.trim().is_empty() {
        block_ranges.push(0..markdown.len());
    }

    block_ranges
}

/// Returns the parser option set used by transcript streaming partitioning so top-level block detection stays aligned with the main Markdown parser.
/// 返回转录流式分段使用的解析选项集合，以保证顶层区块检测与主 Markdown 解析器保持一致。
fn transcript_partition_options() -> Options {
    Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// Returns whether one pulldown-cmark start tag represents a structural block boundary relevant to transcript streaming segmentation.
/// 返回某个 pulldown-cmark 起始标签是否表示与转录流式分段相关的结构化区块边界。
fn is_structural_block_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

/// Returns whether one pulldown-cmark end tag closes a structural block boundary relevant to transcript streaming segmentation.
/// 返回某个 pulldown-cmark 结束标签是否关闭了与转录流式分段相关的结构化区块边界。
fn is_structural_block_end(tag_end: &TagEnd) -> bool {
    matches!(
        tag_end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
            | TagEnd::MetadataBlock(_)
    )
}

#[cfg(test)]
mod tests {
    //! Focused regression tests for transcript-oriented streaming Markdown partitioning.
    //! 面向转录流式 Markdown 分段的定向回归测试。

    use super::split_streaming_markdown_source;

    /// Verifies multi-block Markdown keeps older top-level blocks sealed while the newest top-level block remains the active tail.
    /// 验证多区块 Markdown 会把较早顶层区块封口，而最新顶层区块保持为活动尾块。
    #[test]
    fn seals_all_but_last_top_level_block() {
        let partition = split_streaming_markdown_source("# 标题\n\n第一段\n\n第二段");

        assert_eq!(
            partition.sealed_segments,
            vec![String::from("# 标题\n"), String::from("第一段\n")]
        );
        assert_eq!(partition.active_tail, Some(String::from("第二段")));
    }

    /// Verifies a single growing paragraph stays entirely inside the active tail so inline Markdown semantics are not broken by premature splitting.
    /// 验证单个仍在增长的段落会完整保留在活动尾块中，从而避免过早切分破坏行内 Markdown 语义。
    #[test]
    fn single_paragraph_stays_in_active_tail() {
        let partition = split_streaming_markdown_source("这是一个仍在增长的段落");

        assert!(partition.sealed_segments.is_empty());
        assert_eq!(
            partition.active_tail,
            Some(String::from("这是一个仍在增长的段落"))
        );
    }

    /// Verifies tables and fenced code blocks are treated as structural top-level blocks during streaming partitioning.
    /// 验证表格与围栏代码块在流式分段过程中会被视为结构化顶层区块。
    #[test]
    fn tables_and_code_blocks_count_as_structural_blocks() {
        let source = "| 列1 | 列2 |\n|---|---|\n| A | B |\n\n```rs\nfn main() {}\n```";
        let partition = split_streaming_markdown_source(source);

        assert_eq!(partition.sealed_segments.len(), 1);
        assert_eq!(
            partition.active_tail,
            Some(String::from("```rs\nfn main() {}\n```"))
        );
    }
}

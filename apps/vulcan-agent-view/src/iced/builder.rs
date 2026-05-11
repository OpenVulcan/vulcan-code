//! Builder that projects semantic transcript content into scene nodes.
//! 将语义转录内容投影为场景节点的构建器。

use super::scene::{
    ParsedMarkdownTable, PlainParagraphStyle, PlainTextRenderKind, RenderedParagraphBackground,
    RenderedParagraphHorizontalPlacement, RenderedTableCellNode, RenderedTableStyle,
    RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use super::text_routing::build_agent_view_plain_text_spans;
use super::{
    build_table_cell_paragraph_node, explode_spans_to_graphemes,
};
use crate::iced_theme as theme;
use iced::advanced::text;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{Color, Font, Padding, Pixels, Theme};
use std::collections::BTreeMap;

/// Builder responsible for flattening rich transcript content into renderable nodes and copy text.
/// 负责将富文本转录内容展平为可渲染节点和复制文本的构建器。
pub(super) struct RenderedTranscriptBuilder {
    /// Ordered nodes collected for the final rendered transcript model.
    /// 为最终渲染转录模型收集的有序节点集合。
    pub(super) nodes: Vec<RenderedTranscriptNode>,
    /// Global rendered text projection kept in sync with visible selectable content.
    /// 与可见可选内容保持同步的全局渲染文本投影。
    pub(super) full_text: String,
    /// Next separator that should be inserted before the following text-bearing node.
    /// 下一个带文本节点之前应插入的分隔符。
    pub(super) pending_separator: Option<&'static str>,
    /// Selection highlight color used by the final transcript widget.
    /// 最终转录组件使用的选择高亮颜色。
    pub(super) selection_color: Color,
    /// Default fallback text color.
    /// 默认回退文本颜色。
    pub(super) fallback_text_color: Color,
    /// Link-scoped transcript actions registered by synthetic interactive chrome spans.
    /// 由合成交互外框片段注册的链接级转录动作集合。
    pub(super) link_actions: BTreeMap<String, RenderedTranscriptLinkAction>,
}

impl RenderedTranscriptBuilder {
    /// Creates one rendered transcript builder with the provided selection and fallback colors.
    /// 使用提供的选择颜色与回退颜色创建一个渲染转录构建器。
    pub(super) fn new(selection_color: Color, fallback_text_color: Color) -> Self {
        Self {
            nodes: Vec::new(),
            full_text: String::new(),
            pending_separator: None,
            selection_color,
            fallback_text_color,
            link_actions: BTreeMap::new(),
        }
    }

    /// Marks the start of one new top-level transcript block.
    /// 标记一个新的顶层转录区块开始。
    pub(super) fn begin_block(&mut self) {
        self.pending_separator = Some(if self.full_text.is_empty() {
            ""
        } else {
            "\n\n"
        });
    }

    /// Pushes one rich paragraph node composed from already-routed spans.
    /// 推入一个由已路由样式片段组成的富文本段落节点。
    pub(super) fn push_rich_paragraph(
        &mut self,
        spans: Vec<Span<'static, markdown::Uri, Font>>,
        text_size: Pixels,
        line_height: text::LineHeight,
        wrapping: text::Wrapping,
        indent_left: f32,
        gap_before: f32,
        gap_after: f32,
        inner_padding: Padding,
        background: Option<RenderedParagraphBackground>,
        quote_depth: usize,
    ) {
        self.push_rich_paragraph_with_layout(
            spans,
            text_size,
            line_height,
            wrapping,
            indent_left,
            gap_before,
            gap_after,
            inner_padding,
            background,
            quote_depth,
            RenderedParagraphHorizontalPlacement::Leading,
            false,
            None,
        );
    }

    /// Pushes one rich paragraph node with explicit horizontal placement and width-fit rules.
    /// 推入一个带显式水平放置与宽度收缩规则的富文本段落节点。
    pub(super) fn push_rich_paragraph_with_layout(
        &mut self,
        spans: Vec<Span<'static, markdown::Uri, Font>>,
        text_size: Pixels,
        line_height: text::LineHeight,
        wrapping: text::Wrapping,
        indent_left: f32,
        gap_before: f32,
        gap_after: f32,
        inner_padding: Padding,
        background: Option<RenderedParagraphBackground>,
        quote_depth: usize,
        horizontal_placement: RenderedParagraphHorizontalPlacement,
        fit_width_to_content: bool,
        max_width_ratio: Option<f32>,
    ) {
        let paragraph_node = self.build_rich_paragraph_node_with_layout(
            spans,
            text_size,
            line_height,
            wrapping,
            indent_left,
            gap_before,
            gap_after,
            inner_padding,
            background,
            quote_depth,
            horizontal_placement,
            fit_width_to_content,
            max_width_ratio,
        );

        self.nodes
            .push(RenderedTranscriptNode::Paragraph(paragraph_node));

        self.pending_separator = Some("\n");
    }

    /// Builds one rendered paragraph node while projecting its text into the global selectable transcript buffer.
    /// 构建一个渲染段落节点，同时将其文本投影到全局可选择转录缓冲区中。
    pub(super) fn build_rich_paragraph_node_with_layout(
        &mut self,
        spans: Vec<Span<'static, markdown::Uri, Font>>,
        text_size: Pixels,
        line_height: text::LineHeight,
        wrapping: text::Wrapping,
        indent_left: f32,
        gap_before: f32,
        gap_after: f32,
        inner_padding: Padding,
        background: Option<RenderedParagraphBackground>,
        quote_depth: usize,
        horizontal_placement: RenderedParagraphHorizontalPlacement,
        fit_width_to_content: bool,
        max_width_ratio: Option<f32>,
    ) -> super::scene::RenderedParagraphNode {
        let separator = self.pending_separator.take().unwrap_or("\n");
        let separator_char_count = separator.chars().count();
        if !separator.is_empty() {
            self.full_text.push_str(separator);
        }

        let paragraph_text = spans
            .iter()
            .map(|span| span.text.as_ref())
            .collect::<String>();
        let paragraph_start = self.full_text.chars().count();
        self.full_text.push_str(&paragraph_text);
        let paragraph_end = self.full_text.chars().count();
        let (grapheme_spans, local_grapheme_ranges) = explode_spans_to_graphemes(&spans);

        super::scene::RenderedParagraphNode {
            display_text: paragraph_text,
            spans: grapheme_spans,
            global_char_range: paragraph_start..paragraph_end,
            local_grapheme_ranges,
            text_size,
            line_height,
            wrapping,
            indent_left,
            horizontal_placement,
            gap_before: if separator_char_count > 1 {
                gap_before.max(6.0)
            } else {
                gap_before
            },
            gap_after,
            inner_padding,
            fit_width_to_content,
            max_width_ratio,
            background,
            background_action_link: None,
            quote_depth,
        }
    }

    /// Pushes one plain-text paragraph by routing it through the current font fallback strategy.
    /// 通过当前字体兜底策略推入一个纯文本段落。
    pub(super) fn push_plain_paragraph(
        &mut self,
        text: String,
        style: PlainParagraphStyle,
        app_theme: &Theme,
    ) {
        let spans = build_agent_view_plain_text_spans(
            &text,
            match style.render_kind {
                PlainTextRenderKind::Flow => theme::ui_font(),
                PlainTextRenderKind::Code => theme::code_font(),
            },
            style.render_kind,
            style.color.or(Some(self.fallback_text_color)),
            app_theme,
        );

        self.push_rich_paragraph_with_layout(
            spans,
            style.text_size,
            style.line_height,
            style.wrapping,
            style.indent_left,
            style.gap_before,
            style.gap_after,
            if style.inner_padding != Padding::ZERO {
                style.inner_padding
            } else if style.background.is_some() {
                Padding::from([10.0, 12.0])
            } else {
                Padding::ZERO
            },
            style.background,
            style.quote_depth,
            style.horizontal_placement,
            style.fit_width_to_content,
            style.max_width_ratio,
        );
    }

    /// Registers one synthetic transcript link action so later hit-tested clicks can execute block-level behavior.
    /// 注册一个合成转录链接动作，以便后续命中点击时执行块级行为。
    pub(super) fn register_link_action(
        &mut self,
        link_target: impl Into<String>,
        action: RenderedTranscriptLinkAction,
    ) {
        self.link_actions.insert(link_target.into(), action);
    }

    /// Returns the number of currently collected render nodes.
    /// 返回当前已收集的渲染节点数量。
    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Inserts one non-selectable background panel before the provided child-node start index.
    /// 在给定子节点起始索引之前插入一个不可选择背景面板。
    pub(super) fn insert_panel(
        &mut self,
        child_start_index: usize,
        child_node_count: usize,
        background: RenderedParagraphBackground,
    ) {
        if child_node_count == 0 {
            return;
        }

        self.nodes.insert(
            child_start_index,
            RenderedTranscriptNode::Panel(super::scene::RenderedPanelNode {
                child_node_count,
                background,
            }),
        );
    }

    /// Pushes one real table node while projecting its selectable text as tab-separated rows.
    /// 推入一个真实表格节点，同时将其可选文本投影为制表符分隔的多行内容。
    pub(super) fn push_table(
        &mut self,
        table: ParsedMarkdownTable,
        style: RenderedTableStyle,
        app_theme: &Theme,
    ) {
        let separator = self.pending_separator.take().unwrap_or("\n");
        let separator_char_count = separator.chars().count();

        if !separator.is_empty() {
            self.full_text.push_str(separator);
        }

        let mut cells = Vec::new();
        let mut row_index = 0usize;

        for (is_header, row_cells) in std::iter::once((true, table.header_cells.clone()))
            .chain(table.body_rows.iter().cloned().map(|row| (false, row)))
        {
            if row_index > 0 {
                self.full_text.push('\n');
            }

            for column_index in 0..table.alignments.len() {
                if column_index > 0 {
                    self.full_text.push('\t');
                }

                let cell_text = row_cells.get(column_index).cloned().unwrap_or_default();
                let cell_start = self.full_text.chars().count();
                self.full_text.push_str(&cell_text);
                let cell_end = self.full_text.chars().count();

                cells.push(RenderedTableCellNode {
                    paragraph: build_table_cell_paragraph_node(
                        cell_text,
                        cell_start..cell_end,
                        is_header,
                        style.text_size,
                        style.text_color,
                        app_theme,
                    ),
                    row_index,
                    column_index,
                    alignment: table
                        .alignments
                        .get(column_index)
                        .copied()
                        .unwrap_or(super::scene::SelectableTableAlignment::Left),
                    is_header,
                });
            }

            row_index += 1;
        }

        self.nodes
            .push(RenderedTranscriptNode::Table(super::scene::RenderedTableNode {
                cells,
                column_count: table.alignments.len(),
                row_count: row_index,
                indent_left: style.indent_left,
                gap_before: if separator_char_count > 1 {
                    style.gap_before.max(6.0)
                } else {
                    style.gap_before
                },
                gap_after: style.gap_after,
                cell_padding: style.cell_padding,
                outer_padding: style.outer_padding,
                border_color: style.border_color,
                header_background: style.header_background,
                body_background: style.body_background,
                quote_depth: style.quote_depth,
            }));
        self.pending_separator = Some("\n");
    }

    /// Pushes one horizontal rule node and updates the separator state for following paragraphs.
    /// 推入一个水平分隔线节点，并为后续段落更新分隔状态。
    pub(super) fn push_rule(&mut self, color: Color) {
        self.nodes
            .push(RenderedTranscriptNode::Rule(super::scene::RenderedRuleNode {
                gap_before: 8.0,
                gap_after: 8.0,
                color,
            }));
        self.pending_separator = Some("\n\n");
    }
}

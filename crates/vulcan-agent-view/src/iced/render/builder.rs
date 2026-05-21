//! Builder that projects semantic transcript content into scene nodes.
//! 将语义转录内容投影为场景节点的构建器。

use super::scene::{
    ParsedMarkdownTable, PlainParagraphStyle, PlainTextRenderKind, RenderedMediaCardNode,
    RenderedMediaKind, RenderedPanelAccentRule, RenderedParagraphBackground,
    RenderedParagraphHorizontalPlacement, RenderedParagraphNode, RenderedTableCellNode,
    RenderedTableStyle, RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use super::text_routing::build_agent_view_plain_text_spans;
use crate::iced_theme as theme;
use iced::advanced::text;
use iced::font;
use iced::widget::text::Span;
use iced::{Background, Border, Color, Font, Padding, Pixels, Theme};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use vulcan_markdown as markdown;

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
    pub(super) fn new(_selection_color: Color, fallback_text_color: Color) -> Self {
        Self {
            nodes: Vec::new(),
            full_text: String::new(),
            pending_separator: None,
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
        let signature_seed = paragraph_signature_seed(
            &paragraph_text,
            &grapheme_spans,
            text_size,
            line_height,
            wrapping,
        );

        super::scene::RenderedParagraphNode {
            signature_seed,
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
        self.insert_panel_with_accent(child_start_index, child_node_count, background, None);
    }

    /// Inserts one non-selectable background panel before the provided child-node start index and optionally decorates it with one left accent rule.
    /// 在给定子节点起始索引之前插入一个不可选择背景面板，并可选附加一条左侧强调竖线装饰。
    pub(super) fn insert_panel_with_accent(
        &mut self,
        child_start_index: usize,
        child_node_count: usize,
        background: RenderedParagraphBackground,
        left_accent_rule: Option<RenderedPanelAccentRule>,
    ) {
        if child_node_count == 0 {
            return;
        }

        self.nodes.insert(
            child_start_index,
            RenderedTranscriptNode::Panel(super::scene::RenderedPanelNode {
                child_node_count,
                background,
                left_accent_rule,
            }),
        );
    }

    /// Pushes one inline generated-media card node without projecting it into the selectable text buffer.
    /// 推入一个不会投影到可选择文本缓冲区中的内联生成媒体卡片节点。
    pub(super) fn push_media_card(
        &mut self,
        kind: RenderedMediaKind,
        title: impl Into<String>,
        file_name: impl Into<String>,
        local_path: impl Into<String>,
        preview_image_handle: Option<iced::widget::image::Handle>,
        image_aspect_ratio: Option<f32>,
        output_count: usize,
        show_caption: bool,
        is_active: bool,
        background: RenderedParagraphBackground,
        accent_color: Color,
        action_link: Option<String>,
    ) {
        self.push_media_card_with_options(
            kind,
            title,
            file_name,
            local_path,
            preview_image_handle,
            image_aspect_ratio,
            output_count,
            show_caption,
            false,
            is_active,
            4.0,
            10.0,
            0.78,
            RenderedParagraphHorizontalPlacement::Leading,
            background,
            accent_color,
            action_link,
        );
    }

    /// Pushes one inline generated-media card node with explicit layout options so callers can reuse the media renderer for compact user attachment strips.
    /// 推入一个带显式布局选项的内联生成媒体卡片节点，使调用方可以复用媒体渲染器来构建紧凑用户附件条。
    pub(super) fn push_media_card_with_options(
        &mut self,
        kind: RenderedMediaKind,
        title: impl Into<String>,
        file_name: impl Into<String>,
        local_path: impl Into<String>,
        preview_image_handle: Option<iced::widget::image::Handle>,
        image_aspect_ratio: Option<f32>,
        output_count: usize,
        show_caption: bool,
        compact_style: bool,
        is_active: bool,
        gap_before: f32,
        gap_after: f32,
        max_width_ratio: f32,
        horizontal_placement: RenderedParagraphHorizontalPlacement,
        background: RenderedParagraphBackground,
        accent_color: Color,
        action_link: Option<String>,
    ) {
        let title = title.into();
        let file_name = file_name.into();
        let local_path = local_path.into();
        let signature_seed = media_card_signature_seed(
            kind,
            &title,
            &file_name,
            &local_path,
            image_aspect_ratio,
            output_count,
            show_caption,
            compact_style,
            is_active,
            gap_before,
            gap_after,
            max_width_ratio,
            horizontal_placement,
            background,
            accent_color,
            action_link.as_deref(),
        );

        self.nodes
            .push(RenderedTranscriptNode::MediaCard(RenderedMediaCardNode {
                signature_seed,
                kind,
                title,
                file_name,
                local_path,
                preview_image_handle,
                image_aspect_ratio,
                output_count,
                show_caption,
                compact_style,
                is_active,
                gap_before,
                gap_after,
                max_width_ratio,
                horizontal_placement,
                background,
                accent_color,
                action_link,
            }));
        self.pending_separator = Some("\n");
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

        self.nodes.push(RenderedTranscriptNode::Table(
            super::scene::RenderedTableNode {
                signature_seed: table_signature_seed(
                    &cells,
                    table.alignments.len(),
                    row_index,
                    style.indent_left,
                    if separator_char_count > 1 {
                        style.gap_before.max(6.0)
                    } else {
                        style.gap_before
                    },
                    style.gap_after,
                    style.cell_padding,
                    style.outer_padding,
                    style.border_color,
                    style.header_background,
                    style.body_background,
                    style.quote_depth,
                ),
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
            },
        ));
        self.pending_separator = Some("\n");
    }

    /// Pushes one horizontal rule node and updates the separator state for following paragraphs.
    /// 推入一个水平分隔线节点，并为后续段落更新分隔状态。
    pub(super) fn push_rule(&mut self, color: Color) {
        self.nodes.push(RenderedTranscriptNode::Rule(
            super::scene::RenderedRuleNode {
                signature_seed: rule_signature_seed(8.0, 8.0, color),
                gap_before: 8.0,
                gap_after: 8.0,
                color,
            },
        ));
        self.pending_separator = Some("\n\n");
    }
}

/// Builds one rendered table-cell paragraph node with script-aware font routing and a stable global character range.
/// 构建一个带脚本感知字体路由和稳定全局字符范围的渲染表格单元格段落节点。
fn build_table_cell_paragraph_node(
    cell_text: String,
    global_char_range: Range<usize>,
    is_header: bool,
    text_size: Pixels,
    text_color: Color,
    app_theme: &Theme,
) -> RenderedParagraphNode {
    let requested_font = if is_header {
        Font {
            weight: font::Weight::Semibold,
            ..theme::ui_font()
        }
    } else {
        theme::ui_font()
    };
    let spans = if cell_text.is_empty() {
        Vec::new()
    } else {
        build_agent_view_plain_text_spans(
            &cell_text,
            requested_font,
            PlainTextRenderKind::Flow,
            Some(text_color),
            app_theme,
        )
    };
    let (grapheme_spans, local_grapheme_ranges) = explode_spans_to_graphemes(&spans);
    let signature_seed = paragraph_signature_seed(
        &cell_text,
        &grapheme_spans,
        text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
    );

    RenderedParagraphNode {
        signature_seed,
        display_text: cell_text,
        spans: grapheme_spans,
        global_char_range,
        local_grapheme_ranges,
        text_size,
        line_height: text::LineHeight::default(),
        wrapping: text::Wrapping::Word,
        indent_left: 0.0,
        horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
        gap_before: 0.0,
        gap_after: 0.0,
        inner_padding: Padding::ZERO,
        fit_width_to_content: false,
        max_width_ratio: None,
        background: None,
        background_action_link: None,
        quote_depth: 0,
    }
}

/// Computes one stable content/style signature seed for a rendered paragraph node so later layout passes can skip rehashing all spans.
/// 为渲染段落节点计算稳定的内容/样式签名种子，使后续布局过程可以跳过对全部片段的重复哈希。
fn paragraph_signature_seed(
    display_text: &str,
    spans: &[Span<'static, markdown::Uri, Font>],
    text_size: Pixels,
    line_height: text::LineHeight,
    wrapping: text::Wrapping,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    display_text.hash(&mut hasher);
    text_size.0.to_bits().hash(&mut hasher);
    hash_line_height(line_height, &mut hasher);
    format!("{wrapping:?}").hash(&mut hasher);
    spans.len().hash(&mut hasher);
    for span in spans {
        hash_span(span, &mut hasher);
    }
    hasher.finish()
}

/// Computes one stable signature seed for a media card node from its layout-affecting inputs.
/// 根据媒体卡片会影响布局的输入计算一份稳定签名种子。
fn media_card_signature_seed(
    kind: RenderedMediaKind,
    title: &str,
    file_name: &str,
    local_path: &str,
    image_aspect_ratio: Option<f32>,
    output_count: usize,
    show_caption: bool,
    compact_style: bool,
    is_active: bool,
    gap_before: f32,
    gap_after: f32,
    max_width_ratio: f32,
    horizontal_placement: RenderedParagraphHorizontalPlacement,
    background: RenderedParagraphBackground,
    accent_color: Color,
    action_link: Option<&str>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{kind:?}").hash(&mut hasher);
    title.hash(&mut hasher);
    file_name.hash(&mut hasher);
    local_path.hash(&mut hasher);
    image_aspect_ratio.map(f32::to_bits).hash(&mut hasher);
    output_count.hash(&mut hasher);
    show_caption.hash(&mut hasher);
    compact_style.hash(&mut hasher);
    is_active.hash(&mut hasher);
    gap_before.to_bits().hash(&mut hasher);
    gap_after.to_bits().hash(&mut hasher);
    max_width_ratio.to_bits().hash(&mut hasher);
    format!("{horizontal_placement:?}").hash(&mut hasher);
    hash_background(background.background, &mut hasher);
    hash_border(background.border, &mut hasher);
    color_signature_tuple(accent_color).hash(&mut hasher);
    action_link.hash(&mut hasher);
    hasher.finish()
}

/// Computes one stable signature seed for a horizontal rule node from its layout-affecting inputs.
/// 根据水平分隔线会影响布局的输入计算一份稳定签名种子。
fn rule_signature_seed(gap_before: f32, gap_after: f32, color: Color) -> u64 {
    let mut hasher = DefaultHasher::new();
    gap_before.to_bits().hash(&mut hasher);
    gap_after.to_bits().hash(&mut hasher);
    color_signature_tuple(color).hash(&mut hasher);
    hasher.finish()
}

/// Computes one stable signature seed for a rendered table node from its static layout-affecting fields and cell signatures.
/// 根据渲染表格节点的静态布局字段和单元格签名计算一份稳定签名种子。
fn table_signature_seed(
    cells: &[RenderedTableCellNode],
    column_count: usize,
    row_count: usize,
    indent_left: f32,
    gap_before: f32,
    gap_after: f32,
    cell_padding: Padding,
    outer_padding: Padding,
    border_color: Color,
    header_background: Background,
    body_background: Background,
    quote_depth: usize,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    column_count.hash(&mut hasher);
    row_count.hash(&mut hasher);
    indent_left.to_bits().hash(&mut hasher);
    gap_before.to_bits().hash(&mut hasher);
    gap_after.to_bits().hash(&mut hasher);
    hash_padding(cell_padding, &mut hasher);
    hash_padding(outer_padding, &mut hasher);
    color_signature_tuple(border_color).hash(&mut hasher);
    hash_background(header_background, &mut hasher);
    hash_background(body_background, &mut hasher);
    quote_depth.hash(&mut hasher);
    for cell in cells {
        cell.row_index.hash(&mut hasher);
        cell.column_index.hash(&mut hasher);
        format!("{:?}", cell.alignment).hash(&mut hasher);
        cell.is_header.hash(&mut hasher);
        cell.paragraph.signature_seed.hash(&mut hasher);
    }
    hasher.finish()
}

/// Hashes one rich text span into the provided stable signature state.
/// 将单个富文本片段写入给定稳定签名状态。
fn hash_span(
    span: &iced::widget::text::Span<'static, markdown::Uri, Font>,
    hasher: &mut DefaultHasher,
) {
    span.text.as_ref().hash(hasher);
    span.size.map(|size| size.0.to_bits()).hash(hasher);
    if let Some(line_height) = span.line_height {
        hash_line_height(line_height, hasher);
    } else {
        0_u8.hash(hasher);
    }
    span.font.hash(hasher);
    span.color.map(color_signature_tuple).hash(hasher);
    format!("{:?}", span.link).hash(hasher);
    if let Some(highlight) = span.highlight {
        hash_background(highlight.background, hasher);
        hash_border(highlight.border, hasher);
    } else {
        0_u8.hash(hasher);
    }
    hash_padding(span.padding, hasher);
    span.underline.hash(hasher);
    span.strikethrough.hash(hasher);
}

/// Hashes one line-height value while preserving the exact variant used by the paragraph.
/// 在保留段落使用的具体变体前提下，对行高值进行哈希。
fn hash_line_height(line_height: text::LineHeight, hasher: &mut DefaultHasher) {
    match line_height {
        text::LineHeight::Relative(value) => {
            1_u8.hash(hasher);
            value.to_bits().hash(hasher);
        }
        text::LineHeight::Absolute(value) => {
            2_u8.hash(hasher);
            value.0.to_bits().hash(hasher);
        }
    }
}

/// Hashes one background value into the provided stable signature state.
/// 将单个背景值写入给定稳定签名状态。
fn hash_background(background: Background, hasher: &mut DefaultHasher) {
    match background {
        Background::Color(color) => {
            1_u8.hash(hasher);
            color_signature_tuple(color).hash(hasher);
        }
        Background::Gradient(gradient) => {
            2_u8.hash(hasher);
            format!("{gradient:?}").hash(hasher);
        }
    }
}

/// Hashes one border value into the provided stable signature state.
/// 将单个边框值写入给定稳定签名状态。
fn hash_border(border: Border, hasher: &mut DefaultHasher) {
    border.width.to_bits().hash(hasher);
    format!("{:?}", border.radius).hash(hasher);
    color_signature_tuple(border.color).hash(hasher);
}

/// Hashes one padding value into the provided stable signature state.
/// 将单个内边距值写入给定稳定签名状态。
fn hash_padding(padding: Padding, hasher: &mut DefaultHasher) {
    padding.top.to_bits().hash(hasher);
    padding.right.to_bits().hash(hasher);
    padding.bottom.to_bits().hash(hasher);
    padding.left.to_bits().hash(hasher);
}

/// Converts one color into a stable hash-friendly integer tuple.
/// 将颜色转换为稳定且适合哈希的整型元组。
fn color_signature_tuple(color: Color) -> (u32, u32, u32, u32) {
    (
        color.r.to_bits(),
        color.g.to_bits(),
        color.b.to_bits(),
        color.a.to_bits(),
    )
}

/// Converts rich Markdown spans into grapheme-sized spans so selection rectangles can be derived from `span_bounds`.
/// 将富 Markdown 片段转换为字素粒度片段，以便从 `span_bounds` 推导选择矩形。
fn explode_spans_to_graphemes(
    spans: &[Span<'static, markdown::Uri, Font>],
) -> (Vec<Span<'static, markdown::Uri, Font>>, Vec<Range<usize>>) {
    let mut grapheme_spans = Vec::new();
    let mut local_ranges = Vec::new();
    let mut local_char_offset = 0usize;

    for span in spans {
        let fragment = span.text.as_ref();
        if fragment.is_empty() {
            continue;
        }

        for grapheme in UnicodeSegmentation::graphemes(fragment, true) {
            let grapheme_char_count = grapheme.chars().count();
            let mut grapheme_span = span.clone();
            grapheme_span.text = grapheme.to_owned().into();
            grapheme_spans.push(grapheme_span);
            local_ranges.push(local_char_offset..local_char_offset + grapheme_char_count);
            local_char_offset += grapheme_char_count;
        }
    }

    if grapheme_spans.is_empty() {
        let mut placeholder_span = Span::new(" ");
        placeholder_span.font = Some(theme::ui_font());
        grapheme_spans.push(placeholder_span);
        local_ranges.push(0..0);
    }

    (grapheme_spans, local_ranges)
}

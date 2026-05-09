//! Selectable rendered transcript widget used by the primary transcript surface.
//! 主转录渲染表面使用的可选择渲染转录组件。

use crate::diff::DiffLineKind;
use crate::theme;
use iced::advanced::Renderer as _;
use iced::advanced::clipboard::Kind as ClipboardKind;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::Renderer as _;
use iced::advanced::text::{self, Paragraph};
use iced::advanced::widget::tree;
use iced::advanced::{Clipboard, Shell, Widget};
use iced::font;
use iced::keyboard;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Padding, Pixels, Point, Rectangle,
    Renderer, Size, Theme,
};
use std::marker::PhantomData;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use vulcan_agent_view::{AgentViewBlock, AgentViewDirectiveKind, AgentViewDocument};

/// Builds one selectable rendered transcript element from the provided structured agent document.
/// 根据提供的结构化代理文档构建一个可选择的渲染转录元素。
pub fn view_selectable_rendered_transcript<'a, Message: 'a>(
    document: &AgentViewDocument,
    app_theme: &Theme,
) -> Element<'a, Message> {
    let rendered_document = RenderedTranscriptDocument::from_agent_document(document, app_theme);
    Element::new(SelectableTranscript::new(rendered_document))
}

/// Custom widget keeping the rendered transcript selectable without falling back to a text editor surface.
/// 在不退回文本编辑器表面的情况下保持渲染转录可选择的自定义组件。
struct SelectableTranscript<Message> {
    /// Fully materialized rendered transcript model used for layout, hit-testing, and copy.
    /// 用于布局、命中测试与复制的完整已物化渲染转录模型。
    document: RenderedTranscriptDocument,
    /// Marker preserving the message type even though the widget is internally message-free.
    /// 即使组件内部不主动发消息，也用于保留消息类型的标记。
    message_marker: PhantomData<Message>,
}

impl<Message> SelectableTranscript<Message> {
    /// Creates one selectable transcript widget from a rendered transcript model.
    /// 根据一个渲染转录模型创建可选择转录组件。
    fn new(document: RenderedTranscriptDocument) -> Self {
        Self {
            document,
            message_marker: PhantomData,
        }
    }
}

/// Runtime state retained by the selectable transcript widget.
/// 可选择转录组件保留的运行时状态。
struct SelectableTranscriptState {
    /// Cached paragraphs aligned with text-bearing nodes in the rendered transcript.
    /// 与渲染转录中文本节点对齐的缓存段落集合。
    paragraphs: Vec<RendererParagraph>,
    /// Cached table layouts aligned with rendered table nodes in the transcript.
    /// 与转录中渲染表格节点对齐的缓存表格布局集合。
    table_layouts: Vec<CachedTableLayout>,
    /// Whether the transcript currently owns keyboard focus for copy and select-all shortcuts.
    /// 当前转录是否拥有键盘焦点，以处理复制和全选快捷键。
    focused: bool,
    /// Whether the pointer is actively dragging one transcript selection.
    /// 当前指针是否正在拖拽一个转录选择区域。
    dragging: bool,
    /// Fixed anchor character offset used by the current selection.
    /// 当前选择使用的固定锚点字符偏移。
    anchor_char: usize,
    /// Moving head character offset used by the current selection.
    /// 当前选择使用的移动头部字符偏移。
    head_char: usize,
}

impl Default for SelectableTranscriptState {
    /// Creates one empty selectable transcript state with no active selection.
    /// 创建一个不带活动选择的空可选择转录状态。
    fn default() -> Self {
        Self {
            paragraphs: Vec::new(),
            table_layouts: Vec::new(),
            focused: false,
            dragging: false,
            anchor_char: 0,
            head_char: 0,
        }
    }
}

/// Concrete paragraph type used by the default iced renderer.
/// 默认 iced 渲染器使用的具体段落类型。
type RendererParagraph = <Renderer as text::Renderer>::Paragraph;

/// Immutable rendered transcript model consumed by the custom widget.
/// 自定义组件消费的不可变渲染转录模型。
#[derive(Debug, Clone)]
struct RenderedTranscriptDocument {
    /// Ordered visual nodes making up the transcript body.
    /// 组成转录正文的有序可视节点集合。
    nodes: Vec<RenderedTranscriptNode>,
    /// Full copy payload that mirrors the visible rendered text projection.
    /// 与可见渲染文本投影对齐的完整复制载荷。
    full_text: String,
    /// Highlight color used when the user selects transcript content.
    /// 用户选择转录内容时使用的高亮颜色。
    selection_color: Color,
}

impl RenderedTranscriptDocument {
    /// Builds one rendered transcript model from the structured agent document.
    /// 根据结构化代理文档构建一份渲染转录模型。
    fn from_agent_document(document: &AgentViewDocument, app_theme: &Theme) -> Self {
        let palette = app_theme.extended_palette();
        let markdown_settings = theme::markdown_settings(app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            palette.primary.weak.color.scale_alpha(0.28),
            palette.background.base.text.scale_alpha(0.94),
        );

        for block in &document.blocks {
            builder.begin_block();

            match block {
                AgentViewBlock::Markdown(markdown_block) => {
                    let prepared_source =
                        prepare_markdown_source_for_rendering(&markdown_block.raw_source);
                    if raw_source_contains_markdown_table(&prepared_source) {
                        render_markdown_source_with_table_fallback(
                            &mut builder,
                            &prepared_source,
                            PlainParagraphStyle::flow(markdown_settings.text_size),
                            markdown_settings,
                            app_theme,
                        );
                    } else {
                        let normalized_source =
                            normalize_markdown_source_for_transcript_display(&prepared_source);
                        let markdown_content = markdown::Content::parse(&normalized_source);
                        render_markdown_items(
                            &mut builder,
                            markdown_content.items(),
                            RenderContext::default(),
                            markdown_settings,
                            app_theme,
                        );
                    }
                }
                AgentViewBlock::Directive(directive_block) => {
                    render_directive_block(
                        &mut builder,
                        directive_block,
                        markdown_settings,
                        app_theme,
                    );
                }
            }
        }

        Self {
            nodes: builder.nodes,
            full_text: builder.full_text,
            selection_color: builder.selection_color,
        }
    }

    /// Returns the selected rendered text slice bounded by the provided global character offsets.
    /// 返回由提供的全局字符偏移限定的已选渲染文本切片。
    fn selected_text(&self, start_char: usize, end_char: usize) -> String {
        slice_char_range(&self.full_text, start_char, end_char)
    }

    /// Returns the total rendered character count available for transcript selection.
    /// 返回可用于转录选择的总渲染字符数量。
    fn len_chars(&self) -> usize {
        self.full_text.chars().count()
    }
}

/// One rendered visual node in the transcript flow.
/// 转录流中的一个渲染可视节点。
#[derive(Debug, Clone)]
enum RenderedTranscriptNode {
    /// Text-bearing paragraph node that can participate in selection and copy.
    /// 参与选择与复制的带文本段落节点。
    Paragraph(RenderedParagraphNode),
    /// Structured table node rendered with real cell borders and selectable cell content.
    /// 使用真实单元格边框和可选单元格内容渲染的结构化表格节点。
    Table(RenderedTableNode),
    /// Horizontal separator node used for Markdown rules.
    /// 用于 Markdown 分隔线的水平分隔节点。
    Rule(RenderedRuleNode),
}

/// One text-bearing paragraph node in the rendered transcript flow.
/// 渲染转录流中的一个带文本段落节点。
#[derive(Debug, Clone)]
struct RenderedParagraphNode {
    /// Visible display text mirrored into the global copy buffer.
    /// 被镜像到全局复制缓冲区中的可见显示文本。
    display_text: String,
    /// Grapheme-level styled spans used to render the paragraph and derive selection bounds.
    /// 用于渲染段落并推导选择边界的字素级样式片段。
    spans: Vec<Span<'static, markdown::Uri, Font>>,
    /// Character range of this paragraph inside the global rendered transcript text buffer.
    /// 当前段落在全局渲染转录文本缓冲区中的字符范围。
    global_char_range: Range<usize>,
    /// Character ranges of every grapheme-level span inside the paragraph-local text.
    /// 每个字素级片段在段落局部文本中的字符范围。
    local_grapheme_ranges: Vec<Range<usize>>,
    /// Text size used by the paragraph.
    /// 当前段落使用的文本字号。
    text_size: Pixels,
    /// Line height used by the paragraph.
    /// 当前段落使用的行高。
    line_height: text::LineHeight,
    /// Text wrapping strategy used by the paragraph.
    /// 当前段落使用的换行策略。
    wrapping: text::Wrapping,
    /// Left indentation applied to the paragraph.
    /// 当前段落应用的左侧缩进。
    indent_left: f32,
    /// Top gap placed before the paragraph in the flow.
    /// 当前段落在流中之前放置的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the paragraph in the flow.
    /// 当前段落在流中之后放置的底部间距。
    gap_after: f32,
    /// Inner padding applied around the paragraph text when its background is drawn.
    /// 绘制背景时围绕段落文本应用的内边距。
    inner_padding: Padding,
    /// Optional background treatment used to preserve block-level visuals.
    /// 用于保留块级视觉的可选背景样式。
    background: Option<RenderedParagraphBackground>,
    /// Optional quote bar depth used to draw nested quotation guides.
    /// 用于绘制嵌套引用导线的可选引用深度。
    quote_depth: usize,
}

/// One horizontal rule node in the rendered transcript flow.
/// 渲染转录流中的一个水平分隔线节点。
#[derive(Debug, Clone)]
struct RenderedRuleNode {
    /// Top gap placed before the rule.
    /// 分隔线之前放置的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the rule.
    /// 分隔线之后放置的底部间距。
    gap_after: f32,
    /// Display color of the rule.
    /// 分隔线显示颜色。
    color: Color,
}

/// One rendered table node in the transcript flow.
/// 转录流中的一个渲染表格节点。
#[derive(Debug, Clone)]
struct RenderedTableNode {
    /// Flat table cells stored in row-major order.
    /// 以行优先顺序存储的扁平表格单元格集合。
    cells: Vec<RenderedTableCellNode>,
    /// Number of columns in the rendered table.
    /// 当前渲染表格的列数。
    column_count: usize,
    /// Number of rows in the rendered table, including the header row.
    /// 当前渲染表格的总行数，包含表头行。
    row_count: usize,
    /// Left indentation applied to the whole table.
    /// 应用于整张表格的左侧缩进。
    indent_left: f32,
    /// Top gap placed before the table in the flow.
    /// 当前表格在流中之前放置的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the table in the flow.
    /// 当前表格在流中之后放置的底部间距。
    gap_after: f32,
    /// Inner padding applied to every table cell.
    /// 应用于每个表格单元格的内部边距。
    cell_padding: Padding,
    /// Border color used by the outer frame and grid separators.
    /// 外框与网格分隔线使用的边框颜色。
    border_color: Color,
    /// Background fill used by header cells.
    /// 表头单元格使用的背景填充色。
    header_background: Background,
    /// Background fill used by body cells.
    /// 正文单元格使用的背景填充色。
    body_background: Background,
    /// Quote depth copied onto the rendered table.
    /// 拷贝到渲染表格上的引用深度。
    quote_depth: usize,
}

/// One rendered table cell node stored inside a table node.
/// 存储在表格节点内部的一个渲染表格单元格节点。
#[derive(Debug, Clone)]
struct RenderedTableCellNode {
    /// Paragraph-shaped cell content reused for layout, hit-testing, and selection drawing.
    /// 复用于布局、命中测试与选择绘制的段落形单元格内容。
    paragraph: RenderedParagraphNode,
    /// Zero-based row index of the cell.
    /// 当前单元格的零基行索引。
    row_index: usize,
    /// Zero-based column index of the cell.
    /// 当前单元格的零基列索引。
    column_index: usize,
    /// Alignment requested by the Markdown table syntax.
    /// Markdown 表格语法请求的对齐方式。
    alignment: SelectableTableAlignment,
    /// Whether the cell belongs to the header row.
    /// 当前单元格是否属于表头行。
    is_header: bool,
}

/// Cached table layout computed during widget layout so draw and hit-test share the same geometry.
/// 在组件布局阶段计算出的缓存表格布局，使绘制与命中测试共享相同几何信息。
#[derive(Debug, Clone, Default)]
struct CachedTableLayout {
    /// Paragraph range owned by the table inside the global paragraph cache.
    /// 当前表格在全局段落缓存中拥有的段落范围。
    paragraph_range: Range<usize>,
    /// Final column widths resolved for the current available width.
    /// 针对当前可用宽度解析得到的最终列宽集合。
    column_widths: Vec<f32>,
    /// Final row heights resolved from the measured cell contents.
    /// 根据测量后的单元格内容解析得到的最终行高集合。
    row_heights: Vec<f32>,
    /// Grid line thickness shared by the whole table.
    /// 整张表格共享的网格线厚度。
    border_thickness: f32,
}

/// Background treatment applied to one rendered paragraph node.
/// 应用于单个渲染段落节点的背景样式。
#[derive(Debug, Clone, Copy)]
struct RenderedParagraphBackground {
    /// Fill background shown behind the paragraph.
    /// 显示在段落背后的填充背景。
    background: Background,
    /// Border shown around the paragraph background.
    /// 显示在段落背景周围的边框。
    border: Border,
}

/// Recursive rendering context used when flattening Markdown items.
/// 展平 Markdown 节点时使用的递归渲染上下文。
#[derive(Debug, Clone, Copy, Default)]
struct RenderContext {
    /// Left indentation accumulated by list nesting.
    /// 由列表嵌套累计形成的左侧缩进。
    indent_left: f32,
    /// Current quote depth used to draw nested quotation guides.
    /// 用于绘制嵌套引用导线的当前引用深度。
    quote_depth: usize,
}

/// Plain-paragraph style preset used for directive titles and fallback paragraphs.
/// 用于指令标题与回退段落的纯文本段落样式预设。
#[derive(Debug, Clone, Copy)]
struct PlainParagraphStyle {
    /// Text size used by the plain paragraph.
    /// 当前纯文本段落使用的文本字号。
    text_size: Pixels,
    /// Line height used by the plain paragraph.
    /// 当前纯文本段落使用的行高。
    line_height: text::LineHeight,
    /// Wrapping strategy used by the plain paragraph.
    /// 当前纯文本段落使用的换行策略。
    wrapping: text::Wrapping,
    /// Left indentation applied to the plain paragraph.
    /// 当前纯文本段落应用的左侧缩进。
    indent_left: f32,
    /// Top gap placed before the plain paragraph.
    /// 当前纯文本段落之前放置的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the plain paragraph.
    /// 当前纯文本段落之后放置的底部间距。
    gap_after: f32,
    /// Optional explicit foreground color.
    /// 可选的显式前景色。
    color: Option<Color>,
    /// Span routing branch used by the plain paragraph.
    /// 当前纯文本段落使用的片段路由分支。
    render_kind: PlainTextRenderKind,
    /// Optional background treatment for the plain paragraph.
    /// 当前纯文本段落的可选背景样式。
    background: Option<RenderedParagraphBackground>,
    /// Quote depth copied onto the rendered paragraph.
    /// 拷贝到渲染段落上的引用深度。
    quote_depth: usize,
}

/// Table style preset used by the custom selectable transcript renderer.
/// 自定义可选择转录渲染器使用的表格样式预设。
#[derive(Debug, Clone, Copy)]
struct RenderedTableStyle {
    /// Text size used by table cell content.
    /// 表格单元格内容使用的文本字号。
    text_size: Pixels,
    /// Left indentation applied to the whole table.
    /// 应用于整张表格的左侧缩进。
    indent_left: f32,
    /// Top gap placed before the table.
    /// 放置在表格之前的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the table.
    /// 放置在表格之后的底部间距。
    gap_after: f32,
    /// Cell padding applied to every table cell.
    /// 应用于每个表格单元格的内部边距。
    cell_padding: Padding,
    /// Border color used by the whole table.
    /// 整张表格使用的边框颜色。
    border_color: Color,
    /// Header background fill used by the whole table.
    /// 整张表格的表头背景填充色。
    header_background: Background,
    /// Body background fill used by the whole table.
    /// 整张表格的正文背景填充色。
    body_background: Background,
    /// Quote depth copied onto the table node.
    /// 拷贝到表格节点上的引用深度。
    quote_depth: usize,
}

/// Parsed Markdown table payload extracted from source-aware table scanning.
/// 从源码感知表格扫描中提取出的 Markdown 表格载荷。
#[derive(Debug, Clone)]
struct ParsedMarkdownTable {
    /// Header cells of the Markdown table.
    /// Markdown 表格的表头单元格集合。
    header_cells: Vec<String>,
    /// Alignment rules declared by the Markdown alignment row.
    /// Markdown 对齐行声明的对齐规则集合。
    alignments: Vec<SelectableTableAlignment>,
    /// Body rows of the Markdown table.
    /// Markdown 表格的正文行集合。
    body_rows: Vec<Vec<String>>,
}

impl PlainParagraphStyle {
    /// Returns one prose-style plain paragraph preset.
    /// 返回一个正文风格的纯文本段落预设。
    fn flow(text_size: Pixels) -> Self {
        Self {
            text_size,
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::Word,
            indent_left: 0.0,
            gap_before: 0.0,
            gap_after: 10.0,
            color: None,
            render_kind: PlainTextRenderKind::Flow,
            background: None,
            quote_depth: 0,
        }
    }

    /// Returns one code-style plain paragraph preset.
    /// 返回一个代码风格的纯文本段落预设。
    fn code(text_size: Pixels, background: Option<RenderedParagraphBackground>) -> Self {
        Self {
            text_size,
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::WordOrGlyph,
            indent_left: 0.0,
            gap_before: 0.0,
            gap_after: 2.0,
            color: None,
            render_kind: PlainTextRenderKind::Code,
            background,
            quote_depth: 0,
        }
    }
}

/// Builder responsible for flattening rich transcript content into renderable nodes and copy text.
/// 负责将富文本转录内容展平为可渲染节点和复制文本的构建器。
struct RenderedTranscriptBuilder {
    /// Ordered nodes collected for the final rendered transcript model.
    /// 为最终渲染转录模型收集的有序节点集合。
    nodes: Vec<RenderedTranscriptNode>,
    /// Global rendered text projection kept in sync with visible selectable content.
    /// 与可见可选内容保持同步的全局渲染文本投影。
    full_text: String,
    /// Next separator that should be inserted before the following text-bearing node.
    /// 下一个带文本节点之前应插入的分隔符。
    pending_separator: Option<&'static str>,
    /// Selection highlight color used by the final transcript widget.
    /// 最终转录组件使用的选择高亮颜色。
    selection_color: Color,
    /// Default fallback text color.
    /// 默认回退文本颜色。
    fallback_text_color: Color,
}

impl RenderedTranscriptBuilder {
    /// Creates one rendered transcript builder with the provided selection and fallback colors.
    /// 使用提供的选择颜色与回退颜色创建一个渲染转录构建器。
    fn new(selection_color: Color, fallback_text_color: Color) -> Self {
        Self {
            nodes: Vec::new(),
            full_text: String::new(),
            pending_separator: None,
            selection_color,
            fallback_text_color,
        }
    }

    /// Marks the start of one new top-level transcript block.
    /// 标记一个新的顶层转录区块开始。
    fn begin_block(&mut self) {
        self.pending_separator = Some(if self.full_text.is_empty() {
            ""
        } else {
            "\n\n"
        });
    }

    /// Pushes one rich paragraph node composed from already-routed spans.
    /// 推入一个由已路由样式片段组成的富文本段落节点。
    fn push_rich_paragraph(
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

        self.nodes
            .push(RenderedTranscriptNode::Paragraph(RenderedParagraphNode {
                display_text: paragraph_text,
                spans: grapheme_spans,
                global_char_range: paragraph_start..paragraph_end,
                local_grapheme_ranges,
                text_size,
                line_height,
                wrapping,
                indent_left,
                gap_before: if separator_char_count > 1 {
                    gap_before.max(6.0)
                } else {
                    gap_before
                },
                gap_after,
                inner_padding,
                background,
                quote_depth,
            }));

        self.pending_separator = Some("\n");
    }

    /// Pushes one plain-text paragraph by routing it through the current font fallback strategy.
    /// 通过当前字体兜底策略推入一个纯文本段落。
    fn push_plain_paragraph(
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

        self.push_rich_paragraph(
            spans,
            style.text_size,
            style.line_height,
            style.wrapping,
            style.indent_left,
            style.gap_before,
            style.gap_after,
            if style.background.is_some() {
                Padding::from([10.0, 12.0])
            } else {
                Padding::ZERO
            },
            style.background,
            style.quote_depth,
        );
    }

    /// Pushes one real table node while projecting its selectable text as tab-separated rows.
    /// 推入一个真实表格节点，同时将其可选文本投影为制表符分隔的多行内容。
    fn push_table(
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
                        self.fallback_text_color,
                        app_theme,
                    ),
                    row_index,
                    column_index,
                    alignment: table
                        .alignments
                        .get(column_index)
                        .copied()
                        .unwrap_or(SelectableTableAlignment::Left),
                    is_header,
                });
            }

            row_index += 1;
        }

        self.nodes
            .push(RenderedTranscriptNode::Table(RenderedTableNode {
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
                border_color: style.border_color,
                header_background: style.header_background,
                body_background: style.body_background,
                quote_depth: style.quote_depth,
            }));
        self.pending_separator = Some("\n");
    }

    /// Pushes one horizontal rule node and updates the separator state for following paragraphs.
    /// 推入一个水平分隔线节点，并为后续段落更新分隔状态。
    fn push_rule(&mut self, color: Color) {
        self.nodes
            .push(RenderedTranscriptNode::Rule(RenderedRuleNode {
                gap_before: 8.0,
                gap_after: 8.0,
                color,
            }));
        self.pending_separator = Some("\n\n");
    }
}

/// Geometry resolved for one rendered table node after layout.
/// 一个渲染表格节点在布局后的已解析几何信息。
#[derive(Debug, Clone)]
struct MeasuredRenderedTableNode {
    /// Outer bounds occupied by the whole table, excluding the trailing flow gap.
    /// 整张表格占据的外部边界，不包含尾部流式间距。
    outer_bounds: Rectangle,
    /// Cell bounds resolved in row-major order.
    /// 按行优先顺序解析得到的单元格边界集合。
    cell_bounds: Vec<Rectangle>,
    /// Text origins resolved for every cell in row-major order.
    /// 按行优先顺序为每个单元格解析得到的文本起点集合。
    cell_text_origins: Vec<Point>,
}

/// Returns the number of renderer paragraph slots needed by the whole rendered transcript document.
/// 返回整份渲染转录文档需要的渲染器段落槽位数量。
fn count_rendered_paragraph_slots(document: &RenderedTranscriptDocument) -> usize {
    document
        .nodes
        .iter()
        .map(|node| match node {
            RenderedTranscriptNode::Paragraph(_) => 1,
            RenderedTranscriptNode::Table(table_node) => table_node.cells.len(),
            RenderedTranscriptNode::Rule(_) => 0,
        })
        .sum()
}

/// Builds one renderer paragraph from a rendered paragraph node and the provided text bounds.
/// 根据一个渲染段落节点和给定文本边界构建渲染器段落。
fn build_renderer_paragraph(
    paragraph_node: &RenderedParagraphNode,
    text_bounds: Size,
) -> RendererParagraph {
    RendererParagraph::with_spans(text::Text {
        content: paragraph_node.spans.as_slice(),
        bounds: text_bounds,
        size: paragraph_node.text_size,
        line_height: paragraph_node.line_height,
        font: theme::ui_font(),
        align_x: text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: paragraph_node.wrapping,
    })
}

/// Lays out one rendered table node and returns both its cached geometry and total flow height.
/// 布局一个渲染表格节点，并返回其缓存几何信息与总流式高度。
fn layout_rendered_table_node(
    table_node: &RenderedTableNode,
    paragraphs: &mut [RendererParagraph],
    paragraph_start: usize,
    available_width: f32,
) -> (CachedTableLayout, f32) {
    if table_node.column_count == 0 || table_node.row_count == 0 || table_node.cells.is_empty() {
        return (
            CachedTableLayout {
                paragraph_range: paragraph_start..paragraph_start,
                column_widths: Vec::new(),
                row_heights: Vec::new(),
                border_thickness: 1.0,
            },
            table_node.gap_before + table_node.gap_after,
        );
    }

    let border_thickness = 1.0;
    let table_available_width = (available_width - table_node.indent_left).max(1.0);
    let horizontal_padding = table_node.cell_padding.left + table_node.cell_padding.right;
    let vertical_padding = table_node.cell_padding.top + table_node.cell_padding.bottom;
    let usable_width = (table_available_width
        - border_thickness * (table_node.column_count as f32 + 1.0))
        .max(table_node.column_count as f32);
    let measurement_text_width = (usable_width - horizontal_padding).max(1.0);

    let paragraph_range = paragraph_start..paragraph_start + table_node.cells.len();
    let mut natural_column_widths = vec![0.0_f32; table_node.column_count];

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let paragraph = build_renderer_paragraph(
            &cell_node.paragraph,
            Size::new(measurement_text_width, f32::INFINITY),
        );
        paragraphs[paragraph_start + cell_index] = paragraph;
        natural_column_widths[cell_node.column_index] = natural_column_widths
            [cell_node.column_index]
            .max(paragraphs[paragraph_start + cell_index].min_bounds().width + horizontal_padding);
    }

    let column_widths = resolve_rendered_table_column_widths(&natural_column_widths, usable_width);

    let mut row_heights = vec![0.0_f32; table_node.row_count];

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let column_width = column_widths
            .get(cell_node.column_index)
            .copied()
            .unwrap_or(usable_width / table_node.column_count as f32);
        paragraphs[paragraph_start + cell_index].resize(Size::new(
            (column_width - horizontal_padding).max(1.0),
            f32::INFINITY,
        ));
        row_heights[cell_node.row_index] = row_heights[cell_node.row_index]
            .max(paragraphs[paragraph_start + cell_index].min_bounds().height + vertical_padding);
    }

    let table_height = table_node.gap_before
        + border_thickness * (table_node.row_count as f32 + 1.0)
        + row_heights.iter().sum::<f32>()
        + table_node.gap_after;

    (
        CachedTableLayout {
            paragraph_range,
            column_widths,
            row_heights,
            border_thickness,
        },
        table_height,
    )
}

/// Resolves final table column widths from natural measurements and the available content width.
/// 根据自然测量结果和可用内容宽度解析最终表格列宽。
fn resolve_rendered_table_column_widths(natural_widths: &[f32], available_width: f32) -> Vec<f32> {
    if natural_widths.is_empty() {
        return Vec::new();
    }

    let natural_sum = natural_widths.iter().sum::<f32>();

    if natural_sum <= 0.0 {
        return vec![available_width / natural_widths.len() as f32; natural_widths.len()];
    }

    if natural_sum <= available_width {
        let extra_width = (available_width - natural_sum) / natural_widths.len() as f32;
        return natural_widths
            .iter()
            .map(|width| width + extra_width)
            .collect();
    }

    let even_width = available_width / natural_widths.len() as f32;
    natural_widths.iter().map(|_| even_width.max(1.0)).collect()
}

/// Measures one rendered table node into concrete cell rectangles and text origins using the cached layout.
/// 使用缓存布局将一个渲染表格节点测量为具体单元格矩形和文本起点。
fn measure_rendered_table_node(
    table_node: &RenderedTableNode,
    table_layout: &CachedTableLayout,
    paragraphs: &[RendererParagraph],
    widget_bounds: Rectangle,
    current_y: f32,
) -> MeasuredRenderedTableNode {
    let outer_x = widget_bounds.x + table_node.indent_left;
    let outer_y = current_y + table_node.gap_before;
    let outer_width = table_layout.column_widths.iter().sum::<f32>()
        + table_layout.border_thickness * (table_node.column_count as f32 + 1.0);
    let outer_height = table_layout.row_heights.iter().sum::<f32>()
        + table_layout.border_thickness * (table_node.row_count as f32 + 1.0);
    let outer_bounds = Rectangle::new(
        Point::new(outer_x, outer_y),
        Size::new(outer_width, outer_height),
    );

    let mut cell_bounds = Vec::with_capacity(table_node.cells.len());
    let mut cell_text_origins = Vec::with_capacity(table_node.cells.len());
    let mut current_row_y = outer_y + table_layout.border_thickness;

    for row_index in 0..table_node.row_count {
        let row_height = table_layout.row_heights[row_index];
        let mut current_column_x = outer_x + table_layout.border_thickness;

        for column_index in 0..table_node.column_count {
            let cell_width = table_layout.column_widths[column_index];
            let cell_bounds_rect = Rectangle::new(
                Point::new(current_column_x, current_row_y),
                Size::new(cell_width, row_height),
            );
            cell_bounds.push(cell_bounds_rect);
            cell_text_origins.push(Point::new(
                cell_bounds_rect.x + table_node.cell_padding.left,
                cell_bounds_rect.y + table_node.cell_padding.top,
            ));
            current_column_x += cell_width + table_layout.border_thickness;
        }

        current_row_y += row_height + table_layout.border_thickness;
    }

    let _ = paragraphs;

    MeasuredRenderedTableNode {
        outer_bounds,
        cell_bounds,
        cell_text_origins,
    }
}

/// Draws one rendered table node, including cell fills, grid lines, selections, and text.
/// 绘制一个渲染表格节点，包含单元格填充、网格线、选择高亮和文本。
fn draw_rendered_table_node(
    renderer: &mut Renderer,
    document: &RenderedTranscriptDocument,
    defaults: &renderer::Style,
    state: &SelectableTranscriptState,
    table_node: &RenderedTableNode,
    paragraphs: &[RendererParagraph],
    measured_table: &MeasuredRenderedTableNode,
    viewport: &Rectangle,
) {
    if table_node.quote_depth > 0 {
        let quote_bar_x = (measured_table.outer_bounds.x - 10.0).max(viewport.x);
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(quote_bar_x, measured_table.outer_bounds.y),
                    Size::new(4.0, measured_table.outer_bounds.height),
                ),
                ..renderer::Quad::default()
            },
            Background::Color(defaults.text_color.scale_alpha(0.18)),
        );
    }

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let Some(cell_bounds) = measured_table.cell_bounds.get(cell_index).copied() else {
            continue;
        };
        let background = if cell_node.is_header {
            table_node.header_background
        } else {
            table_node.body_background
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: cell_bounds,
                ..renderer::Quad::default()
            },
            background,
        );

        if let Some((selection_start, selection_end)) = normalized_selection(state) {
            draw_paragraph_selection(
                renderer,
                document,
                &cell_node.paragraph,
                &paragraphs[cell_index],
                selection_start,
                selection_end,
                measured_table.cell_text_origins[cell_index],
                viewport,
            );
        }

        renderer.fill_paragraph(
            &paragraphs[cell_index],
            measured_table.cell_text_origins[cell_index],
            cell_node
                .paragraph
                .spans
                .first()
                .and_then(|span| span.color)
                .unwrap_or(defaults.text_color),
            *viewport,
        );
    }

    renderer.fill_quad(
        renderer::Quad {
            bounds: measured_table.outer_bounds,
            border: Border {
                color: table_node.border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(Color::TRANSPARENT),
    );

    for cell_bounds in &measured_table.cell_bounds {
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(cell_bounds.x - 1.0, cell_bounds.y),
                    Size::new(1.0, cell_bounds.height),
                ),
                ..renderer::Quad::default()
            },
            Background::Color(table_node.border_color),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(cell_bounds.x, cell_bounds.y - 1.0),
                    Size::new(cell_bounds.width, 1.0),
                ),
                ..renderer::Quad::default()
            },
            Background::Color(table_node.border_color),
        );
    }
}

/// Hit-tests one rendered table node and returns the nearest global character offset when a cell is hit.
/// 对一个渲染表格节点执行命中测试，并在命中单元格时返回最近的全局字符偏移。
fn hit_test_rendered_table_node(
    table_node: &RenderedTableNode,
    paragraphs: &[RendererParagraph],
    measured_table: &MeasuredRenderedTableNode,
    position: Point,
) -> Option<usize> {
    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let cell_bounds = *measured_table.cell_bounds.get(cell_index)?;

        if !cell_bounds.contains(position) {
            continue;
        }

        let paragraph = paragraphs.get(cell_index)?;
        let paragraph_height = paragraph.min_bounds().height;
        let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
            Point::new(
                position.x - cell_bounds.x - table_node.cell_padding.left,
                position.y - cell_bounds.y - table_node.cell_padding.top,
            ),
            &cell_node.paragraph,
            cell_bounds.size(),
            paragraph_height,
        );

        let local_byte_offset = paragraph_hit_test_to_byte_offset(
            paragraph,
            &cell_node.paragraph.display_text,
            local_point,
        )?;
        let local_char_offset =
            byte_offset_to_char_offset(&cell_node.paragraph.display_text, local_byte_offset);
        return Some(
            (cell_node.paragraph.global_char_range.start + local_char_offset)
                .min(cell_node.paragraph.global_char_range.end),
        );
    }

    None
}

impl<Message> Widget<Message, Theme, Renderer> for SelectableTranscript<Message> {
    /// Returns the runtime state tag used by the selectable transcript widget.
    /// 返回可选择转录组件使用的运行时状态标签。
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SelectableTranscriptState>()
    }

    /// Returns the runtime state stored by the selectable transcript widget.
    /// 返回可选择转录组件存储的运行时状态。
    fn state(&self) -> tree::State {
        tree::State::new(SelectableTranscriptState::default())
    }

    /// Returns the widget size policy used by the selectable transcript widget.
    /// 返回可选择转录组件使用的尺寸策略。
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    /// Lays out the transcript widget by measuring every rendered paragraph node.
    /// 通过测量每个渲染段落节点来布局转录组件。
    fn layout(
        &mut self,
        tree: &mut tree::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();
        let max_size = limits.width(Length::Fill).height(Length::Shrink).max();
        let available_width = max_size.width.max(1.0);

        let paragraph_count = count_rendered_paragraph_slots(&self.document);
        let table_count = self
            .document
            .nodes
            .iter()
            .filter(|node| matches!(node, RenderedTranscriptNode::Table(_)))
            .count();

        if state.paragraphs.len() != paragraph_count {
            state.paragraphs = (0..paragraph_count)
                .map(|_| RendererParagraph::default())
                .collect();
        }

        if state.table_layouts.len() != table_count {
            state.table_layouts = (0..table_count)
                .map(|_| CachedTableLayout::default())
                .collect();
        }

        let mut paragraph_index = 0usize;
        let mut table_index = 0usize;
        let mut total_height = 0.0f32;

        for node in &self.document.nodes {
            match node {
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    let text_bounds = Size::new(
                        (available_width
                            - paragraph_node.indent_left
                            - paragraph_node.inner_padding.left
                            - paragraph_node.inner_padding.right)
                            .max(1.0),
                        f32::INFINITY,
                    );
                    state.paragraphs[paragraph_index] = RendererParagraph::with_spans(text::Text {
                        content: paragraph_node.spans.as_slice(),
                        bounds: text_bounds,
                        size: paragraph_node.text_size,
                        line_height: paragraph_node.line_height,
                        font: theme::ui_font(),
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: paragraph_node.wrapping,
                    });

                    total_height += paragraph_node.gap_before
                        + paragraph_node.inner_padding.top
                        + state.paragraphs[paragraph_index].min_bounds().height
                        + paragraph_node.inner_padding.bottom
                        + paragraph_node.gap_after;
                    paragraph_index += 1;
                }
                RenderedTranscriptNode::Table(table_node) => {
                    let (table_layout, table_height) = layout_rendered_table_node(
                        table_node,
                        &mut state.paragraphs,
                        paragraph_index,
                        available_width,
                    );
                    paragraph_index = table_layout.paragraph_range.end;
                    state.table_layouts[table_index] = table_layout;
                    table_index += 1;
                    total_height += table_height;
                }
                RenderedTranscriptNode::Rule(rule_node) => {
                    total_height += rule_node.gap_before + 8.0 + rule_node.gap_after;
                }
            }
        }

        layout::Node::new(Size::new(available_width, total_height.max(1.0)))
    }

    /// Draws the transcript widget, its block backgrounds, quote bars, text, and active selection highlight.
    /// 绘制转录组件及其区块背景、引用导线、文本和当前活动选择高亮。
    fn draw(
        &self,
        tree: &tree::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<SelectableTranscriptState>();
        let mut paragraph_index = 0usize;
        let mut table_index = 0usize;
        let mut current_y = layout.bounds().y;

        for node in &self.document.nodes {
            match node {
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    current_y += paragraph_node.gap_before;
                    let paragraph = &state.paragraphs[paragraph_index];
                    let paragraph_height = paragraph.min_bounds().height;
                    let outer_bounds = Rectangle::new(
                        Point::new(layout.bounds().x + paragraph_node.indent_left, current_y),
                        Size::new(
                            (layout.bounds().width - paragraph_node.indent_left).max(1.0),
                            paragraph_node.inner_padding.top
                                + paragraph_height
                                + paragraph_node.inner_padding.bottom,
                        ),
                    );

                    if let Some(background) = paragraph_node.background {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: outer_bounds,
                                border: background.border,
                                ..renderer::Quad::default()
                            },
                            background.background,
                        );
                    }

                    if paragraph_node.quote_depth > 0 {
                        let quote_bar_x = (outer_bounds.x - 10.0).max(layout.bounds().x);
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(quote_bar_x, outer_bounds.y),
                                    Size::new(4.0, outer_bounds.height),
                                ),
                                ..renderer::Quad::default()
                            },
                            Background::Color(defaults.text_color.scale_alpha(0.18)),
                        );
                    }

                    if let Some((selection_start, selection_end)) = normalized_selection(state) {
                        draw_paragraph_selection(
                            renderer,
                            &self.document,
                            paragraph_node,
                            paragraph,
                            selection_start,
                            selection_end,
                            paragraph_local_origin(outer_bounds, paragraph_node),
                            viewport,
                        );
                    }

                    renderer.fill_paragraph(
                        paragraph,
                        paragraph_local_origin(outer_bounds, paragraph_node),
                        paragraph_node
                            .spans
                            .first()
                            .and_then(|span| span.color)
                            .unwrap_or(defaults.text_color),
                        *viewport,
                    );

                    current_y += outer_bounds.height + paragraph_node.gap_after;
                    paragraph_index += 1;
                }
                RenderedTranscriptNode::Table(table_node) => {
                    let table_layout = &state.table_layouts[table_index];
                    let table_paragraphs = &state.paragraphs[table_layout.paragraph_range.clone()];
                    let measured_table = measure_rendered_table_node(
                        table_node,
                        table_layout,
                        table_paragraphs,
                        layout.bounds(),
                        current_y,
                    );

                    draw_rendered_table_node(
                        renderer,
                        &self.document,
                        defaults,
                        state,
                        table_node,
                        table_paragraphs,
                        &measured_table,
                        viewport,
                    );

                    current_y += measured_table.outer_bounds.height + table_node.gap_after;
                    paragraph_index = table_layout.paragraph_range.end;
                    table_index += 1;
                }
                RenderedTranscriptNode::Rule(rule_node) => {
                    current_y += rule_node.gap_before;
                    let rule_y = current_y + 3.0;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(layout.bounds().x, rule_y),
                                Size::new(layout.bounds().width.max(1.0), 2.0),
                            ),
                            ..renderer::Quad::default()
                        },
                        Background::Color(rule_node.color),
                    );
                    current_y += 8.0 + rule_node.gap_after;
                }
            }
        }

        let _ = cursor;
    }

    /// Updates transcript focus, selection dragging, and copy shortcuts based on runtime input events.
    /// 根据运行时输入事件更新转录焦点、选择拖拽和复制快捷键。
    fn update(
        &mut self,
        tree: &mut tree::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    if let Some(hit_char) =
                        hit_test_rendered_document(&self.document, state, layout.bounds(), position)
                    {
                        state.focused = true;
                        state.dragging = true;
                        state.anchor_char = hit_char;
                        state.head_char = hit_char;
                        shell.capture_event();
                    } else {
                        state.focused = false;
                    }
                } else {
                    state.focused = false;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    if let Some(hit_char) =
                        hit_test_rendered_document(&self.document, state, layout.bounds(), position)
                    {
                        state.head_char = hit_char;
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if state.focused && shortcut_matches_copy(key, *modifiers, text.as_deref()) => {
                if let Some((selection_start, selection_end)) = normalized_selection(state) {
                    clipboard.write(
                        ClipboardKind::Standard,
                        self.document.selected_text(selection_start, selection_end),
                    );
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if state.focused
                && shortcut_matches_select_all(key, *modifiers, text.as_deref()) =>
            {
                state.anchor_char = 0;
                state.head_char = self.document.len_chars();
                shell.capture_event();
            }
            _ => {}
        }
    }

    /// Returns the mouse interaction shown while hovering the selectable transcript widget.
    /// 返回悬停在可选择转录组件上时展示的鼠标交互样式。
    fn mouse_interaction(
        &self,
        _tree: &tree::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

/// Renders one directive block using the transcript builder and semantic block treatments.
/// 使用转录构建器和语义块样式渲染一个指令区块。
fn render_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &vulcan_agent_view::AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let palette = app_theme.extended_palette();
    let title_background = match block.kind {
        AgentViewDirectiveKind::UserPrompt => directive_background(
            palette.secondary.weak.color.scale_alpha(0.12),
            palette.secondary.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Thinking => directive_background(
            palette.warning.weak.color.scale_alpha(0.12),
            palette.warning.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::ToolCall => directive_background(
            palette.primary.weak.color.scale_alpha(0.12),
            palette.primary.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::ToolResult => directive_background(
            palette.success.weak.color.scale_alpha(0.12),
            palette.success.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Diff => directive_background(
            palette.background.weak.color.scale_alpha(0.35),
            palette.background.strong.color.scale_alpha(0.45),
        ),
        AgentViewDirectiveKind::Notice => directive_background(
            palette.danger.weak.color.scale_alpha(0.12),
            palette.danger.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Custom(_) => directive_background(
            palette.background.weak.color.scale_alpha(0.26),
            palette.background.strong.color.scale_alpha(0.35),
        ),
    };

    builder.push_plain_paragraph(
        block.selectable_title(),
        PlainParagraphStyle {
            text_size: markdown_settings.text_size,
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::Word,
            indent_left: 0.0,
            gap_before: 0.0,
            gap_after: if block.body.trim().is_empty() {
                0.0
            } else {
                4.0
            },
            color: Some(palette.background.base.text),
            render_kind: PlainTextRenderKind::Flow,
            background: Some(title_background),
            quote_depth: 0,
        },
        app_theme,
    );

    match block.kind {
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult => {
            let body_background = Some(title_background);
            builder.push_rich_paragraph(
                build_agent_view_plain_text_spans(
                    if block.body.is_empty() {
                        " "
                    } else {
                        &block.body
                    },
                    theme::code_font(),
                    PlainTextRenderKind::Code,
                    Some(palette.background.base.text.scale_alpha(0.92)),
                    app_theme,
                ),
                markdown_settings.code_size,
                text::LineHeight::default(),
                text::Wrapping::WordOrGlyph,
                0.0,
                0.0,
                4.0,
                Padding::from([10.0, 12.0]),
                body_background,
                0,
            );
        }
        AgentViewDirectiveKind::Diff => {
            builder.push_rich_paragraph(
                build_multiline_diff_spans(&block.body, app_theme),
                markdown_settings.code_size,
                text::LineHeight::default(),
                text::Wrapping::WordOrGlyph,
                0.0,
                0.0,
                4.0,
                Padding::from([10.0, 12.0]),
                Some(title_background),
                0,
            );
        }
        _ => {
            let prepared_body = prepare_markdown_source_for_rendering(&block.body);
            if raw_source_contains_markdown_table(&prepared_body) {
                render_markdown_source_with_table_fallback(
                    builder,
                    &prepared_body,
                    PlainParagraphStyle {
                        background: Some(title_background),
                        gap_after: 6.0,
                        ..PlainParagraphStyle::flow(markdown_settings.text_size)
                    },
                    markdown_settings,
                    app_theme,
                );
            } else {
                let normalized_body =
                    normalize_markdown_source_for_transcript_display(&prepared_body);
                let body_markdown = markdown::Content::parse(&normalized_body);
                render_markdown_items(
                    builder,
                    body_markdown.items(),
                    RenderContext::default(),
                    markdown_settings,
                    app_theme,
                );
            }
        }
    }
}

/// Renders one slice of Markdown items recursively into the transcript builder.
/// 将一段 Markdown 节点递归渲染到转录构建器中。
fn render_markdown_items(
    builder: &mut RenderedTranscriptBuilder,
    items: &[markdown::Item],
    context: RenderContext,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    for item in items {
        match item {
            markdown::Item::Heading(level, text) => {
                builder.push_rich_paragraph(
                    build_agent_view_markdown_spans(
                        text,
                        settings.style,
                        MarkdownRenderKind::Flow,
                        app_theme,
                    ),
                    heading_size(settings, level),
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    8.0,
                    Padding::ZERO,
                    None,
                    context.quote_depth,
                );
            }
            markdown::Item::Paragraph(text) => {
                builder.push_rich_paragraph(
                    build_agent_view_markdown_spans(
                        text,
                        settings.style,
                        MarkdownRenderKind::Flow,
                        app_theme,
                    ),
                    settings.text_size,
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    8.0,
                    Padding::ZERO,
                    None,
                    context.quote_depth,
                );
            }
            markdown::Item::CodeBlock { lines, .. } => {
                let code_background = Some(directive_background(
                    app_theme
                        .extended_palette()
                        .background
                        .weak
                        .color
                        .scale_alpha(0.24),
                    app_theme
                        .extended_palette()
                        .background
                        .strong
                        .color
                        .scale_alpha(0.34),
                ));
                builder.push_rich_paragraph(
                    build_multiline_markdown_code_spans(
                        lines,
                        settings.style,
                        MarkdownRenderKind::CodeBlock,
                        app_theme,
                    ),
                    settings.code_size,
                    text::LineHeight::default(),
                    text::Wrapping::WordOrGlyph,
                    context.indent_left,
                    0.0,
                    4.0,
                    Padding::from([8.0, 10.0]),
                    code_background,
                    context.quote_depth,
                );
            }
            markdown::Item::List { start, bullets } => {
                render_markdown_list(builder, bullets, *start, context, settings, app_theme);
            }
            markdown::Item::Quote(quote_items) => {
                render_markdown_items(
                    builder,
                    quote_items,
                    RenderContext {
                        indent_left: context.indent_left + 18.0,
                        quote_depth: context.quote_depth + 1,
                    },
                    settings,
                    app_theme,
                );
            }
            markdown::Item::Rule => {
                builder.push_rule(
                    app_theme
                        .extended_palette()
                        .background
                        .strong
                        .color
                        .scale_alpha(0.35),
                );
            }
            markdown::Item::Image { alt, url, .. } => {
                let alt_text = alt
                    .spans(settings.style)
                    .iter()
                    .map(|span| span.text.as_ref())
                    .collect::<String>();
                let display_text = if alt_text.trim().is_empty() {
                    format!("[图片] {url}")
                } else {
                    format!("[图片] {alt_text} ({url})")
                };
                builder.push_plain_paragraph(
                    display_text,
                    PlainParagraphStyle::flow(settings.text_size),
                    app_theme,
                );
            }
            markdown::Item::Table { .. } => {
                builder.push_plain_paragraph(
                    String::from("[表格内容当前使用保底文本渲染]"),
                    PlainParagraphStyle::flow(settings.text_size),
                    app_theme,
                );
            }
        }
    }
}

/// Renders one Markdown list while preserving bullet or ordered prefixes in the visible transcript.
/// 渲染一个 Markdown 列表，同时在可见转录中保留项目符号或有序前缀。
fn render_markdown_list(
    builder: &mut RenderedTranscriptBuilder,
    bullets: &[markdown::Bullet],
    start: Option<u64>,
    context: RenderContext,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    for (index, bullet) in bullets.iter().enumerate() {
        let bullet_prefix = match bullet {
            markdown::Bullet::Point { .. } => String::from("• "),
            markdown::Bullet::Task { done, .. } => {
                format!("[{}] ", if *done { "x" } else { " " })
            }
        };
        let ordered_prefix = start
            .map(|start| format!("{}. ", start + index as u64))
            .unwrap_or_default();
        let prefix_text = if ordered_prefix.is_empty() {
            bullet_prefix
        } else {
            ordered_prefix
        };

        let items = match bullet {
            markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => items,
        };
        let mut rendered_anything = false;

        for item in items {
            match item {
                markdown::Item::Paragraph(text) | markdown::Item::Heading(_, text) => {
                    let mut spans = build_agent_view_markdown_spans(
                        text,
                        settings.style,
                        MarkdownRenderKind::Flow,
                        app_theme,
                    );
                    let mut prefix_spans = build_agent_view_plain_text_spans(
                        &prefix_text,
                        theme::ui_font(),
                        PlainTextRenderKind::Flow,
                        Some(app_theme.extended_palette().background.base.text),
                        app_theme,
                    );
                    prefix_spans.append(&mut spans);
                    builder.push_rich_paragraph(
                        prefix_spans,
                        settings.text_size,
                        text::LineHeight::default(),
                        text::Wrapping::Word,
                        context.indent_left + 18.0,
                        0.0,
                        6.0,
                        Padding::ZERO,
                        None,
                        context.quote_depth,
                    );
                    rendered_anything = true;
                }
                _ => {
                    render_markdown_items(
                        builder,
                        std::slice::from_ref(item),
                        RenderContext {
                            indent_left: context.indent_left + 22.0,
                            quote_depth: context.quote_depth,
                        },
                        settings,
                        app_theme,
                    );
                    rendered_anything = true;
                }
            }
        }

        if !rendered_anything {
            builder.push_plain_paragraph(
                prefix_text,
                PlainParagraphStyle {
                    indent_left: context.indent_left + 18.0,
                    gap_after: 6.0,
                    ..PlainParagraphStyle::flow(settings.text_size)
                },
                app_theme,
            );
        }
    }
}

/// Builds one multiline Markdown-code span list so visually boxed code regions behave like one continuous selectable paragraph.
/// 构建一组多行 Markdown 代码片段，使带背景的代码区域按一个连续可选段落工作。
fn build_multiline_markdown_code_spans(
    lines: &[markdown::Text],
    style: markdown::Style,
    render_kind: MarkdownRenderKind,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    if lines.is_empty() {
        return build_agent_view_plain_text_spans(
            " ",
            theme::code_font(),
            PlainTextRenderKind::Code,
            None,
            app_theme,
        );
    }

    let mut output = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        if line_index > 0 {
            output.extend(build_agent_view_plain_text_spans(
                "\n",
                style.code_block_font,
                PlainTextRenderKind::Code,
                None,
                app_theme,
            ));
        }

        output.extend(build_agent_view_markdown_spans(
            line,
            style,
            render_kind,
            app_theme,
        ));
    }

    output
}

/// Builds one multiline diff span list so boxed diff regions keep per-line colors while remaining one continuous selectable paragraph.
/// 构建一组多行 diff 片段，使带背景的 diff 区域在保留逐行颜色的同时仍是连续可选段落。
fn build_multiline_diff_spans(
    diff_body: &str,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let source_lines = if diff_body.is_empty() {
        vec![""]
    } else {
        diff_body.split('\n').collect::<Vec<_>>()
    };
    let mut output = Vec::new();

    for (line_index, line) in source_lines.iter().enumerate() {
        if line_index > 0 {
            output.extend(build_agent_view_plain_text_spans(
                "\n",
                theme::code_font(),
                PlainTextRenderKind::Code,
                None,
                app_theme,
            ));
        }

        output.extend(build_agent_view_plain_text_spans(
            line,
            theme::code_font(),
            PlainTextRenderKind::Code,
            Some(diff_line_color(app_theme, classify_diff_line_kind(line))),
            app_theme,
        ));
    }

    output
}

/// Renders one Markdown source slice with a source-aware table fallback so visible table content stays selectable.
/// 使用源码感知的表格回退渲染一段 Markdown 源文本，从而让可见表格内容保持可选择。
fn render_markdown_source_with_table_fallback(
    builder: &mut RenderedTranscriptBuilder,
    markdown_source: &str,
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let repaired_source = repair_compact_markdown_tables(markdown_source);
    let lines = repaired_source.lines().collect::<Vec<_>>();
    let mut buffered_plain_lines = Vec::new();
    let mut line_index = 0usize;

    while line_index < lines.len() {
        if is_markdown_table_header_line(lines[line_index])
            && lines
                .get(line_index + 1)
                .is_some_and(|line| is_markdown_table_alignment_line(line))
        {
            flush_plain_markdown_fallback_buffer(
                builder,
                &mut buffered_plain_lines,
                paragraph_style,
                app_theme,
            );

            let mut table_lines = vec![lines[line_index], lines[line_index + 1]];
            line_index += 2;

            while line_index < lines.len() && is_markdown_table_body_line(lines[line_index]) {
                table_lines.push(lines[line_index]);
                line_index += 1;
            }

            if let Some(parsed_table) = parse_markdown_table_source(&table_lines) {
                builder.push_table(
                    parsed_table,
                    rendered_table_style(paragraph_style, settings, app_theme),
                    app_theme,
                );
            } else {
                builder.push_plain_paragraph(
                    table_lines.into_iter().collect::<Vec<_>>().join("\n"),
                    PlainParagraphStyle {
                        text_size: settings.code_size,
                        line_height: text::LineHeight::default(),
                        wrapping: text::Wrapping::None,
                        indent_left: paragraph_style.indent_left,
                        gap_before: paragraph_style.gap_before,
                        gap_after: paragraph_style.gap_after.max(8.0),
                        color: paragraph_style.color,
                        render_kind: PlainTextRenderKind::Code,
                        background: paragraph_style.background,
                        quote_depth: paragraph_style.quote_depth,
                    },
                    app_theme,
                );
            }
            continue;
        }

        buffered_plain_lines.push(lines[line_index]);
        line_index += 1;
    }

    flush_plain_markdown_fallback_buffer(
        builder,
        &mut buffered_plain_lines,
        paragraph_style,
        app_theme,
    );
}

/// Flushes one buffered plain Markdown fallback chunk into the rendered transcript builder while preserving source newlines.
/// 将一段缓冲中的普通 Markdown 回退片段刷新到渲染转录构建器中，并保留源码换行。
fn flush_plain_markdown_fallback_buffer(
    builder: &mut RenderedTranscriptBuilder,
    buffered_plain_lines: &mut Vec<&str>,
    paragraph_style: PlainParagraphStyle,
    app_theme: &Theme,
) {
    let plain_text = buffered_plain_lines.join("\n");
    buffered_plain_lines.clear();

    if plain_text.trim().is_empty() {
        return;
    }

    builder.push_plain_paragraph(plain_text, paragraph_style, app_theme);
}

/// Repairs Markdown source before rendering so compacted table rows recover real line boundaries.
/// 在渲染前修复 Markdown 源文本，使被压缩的表格行恢复成真实换行边界。
fn prepare_markdown_source_for_rendering(markdown_source: &str) -> String {
    repair_compact_markdown_tables(markdown_source)
}

/// Repairs compacted Markdown table rows joined on one physical line back into separate logical lines.
/// 将被压缩到同一物理行上的 Markdown 表格行修复回独立的逻辑行。
fn repair_compact_markdown_table_boundaries(markdown_source: &str) -> String {
    repair_compact_markdown_tables(markdown_source)
}

/// Expands one compacted Markdown table line into multiple logical lines when a valid table boundary pattern is detected.
/// 当检测到有效的表格边界模式时，将单条压缩 Markdown 表格行展开为多条逻辑行。
fn expand_compact_markdown_table_line(line: &str) -> Option<Vec<String>> {
    repair_compact_markdown_table_line(line).map(|(expanded, _)| {
        expanded
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<String>>()
    })
}

/// Returns the Markdown source normalized for transcript display so single source newlines remain visible in prose paragraphs.
/// 返回适用于转录显示的 Markdown 规范化源码，使正文段落中的单个源码换行保持可见。
fn normalize_markdown_source_for_transcript_display(markdown_source: &str) -> String {
    let lines = markdown_source.lines().collect::<Vec<_>>();
    let mut normalized = String::new();
    let mut active_fence_marker = None::<String>;

    for (line_index, line) in lines.iter().enumerate() {
        let trimmed_line = line.trim_start();

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            if active_fence_marker
                .as_deref()
                .is_some_and(|active| trimmed_line.starts_with(active))
            {
                active_fence_marker = None;
            } else if active_fence_marker.is_none() {
                active_fence_marker = Some(fence_marker.to_string());
            }
        }

        normalized.push_str(line);

        let Some(next_line) = lines.get(line_index + 1) else {
            continue;
        };

        if active_fence_marker.is_some()
            || !line_uses_transcript_softbreak_promotion(line)
            || !line_uses_transcript_softbreak_promotion(next_line)
        {
            normalized.push('\n');
        } else {
            normalized.push_str("  \n");
        }
    }

    normalized
}

/// Returns whether the provided source line should promote its trailing soft break into a visible transcript line break.
/// 返回给定源码行是否应将尾部软换行提升为可见的转录换行。
fn line_uses_transcript_softbreak_promotion(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.is_empty()
        || trimmed.ends_with("  ")
        || fenced_code_marker(trimmed).is_some()
        || is_markdown_table_body_line(trimmed)
        || is_markdown_table_alignment_line(trimmed)
        || trimmed.starts_with('#')
        || trimmed.starts_with('>')
        || trimmed.starts_with('|')
        || is_unordered_list_line(trimmed)
        || is_ordered_list_line(trimmed)
        || is_thematic_break_line(trimmed)
    {
        return false;
    }

    true
}

/// Returns the fence marker used by one fenced-code line when it opens or closes a Markdown fence.
/// 返回围栏代码行在开启或关闭 Markdown 围栏时使用的围栏标记。
fn fenced_code_marker(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Returns whether the provided line looks like one unordered Markdown list item.
/// 返回给定行是否像一个无序 Markdown 列表项。
fn is_unordered_list_line(line: &str) -> bool {
    ["- ", "* ", "+ "]
        .into_iter()
        .any(|prefix| line.starts_with(prefix))
}

/// Returns whether the provided line looks like one ordered Markdown list item.
/// 返回给定行是否像一个有序 Markdown 列表项。
fn is_ordered_list_line(line: &str) -> bool {
    let mut digits = 0usize;

    for character in line.chars() {
        if character.is_ascii_digit() {
            digits += 1;
            continue;
        }

        return digits > 0 && matches!(character, '.' | ')');
    }

    false
}

/// Returns whether the provided line looks like one thematic-break Markdown line.
/// 返回给定行是否像一个 Markdown 水平分隔线。
fn is_thematic_break_line(line: &str) -> bool {
    let compact = line.chars().filter(|character| !character.is_whitespace());
    let collected = compact.collect::<String>();
    collected.len() >= 3
        && (collected.chars().all(|character| character == '-')
            || collected.chars().all(|character| character == '*')
            || collected.chars().all(|character| character == '_'))
}

/// Returns whether the Markdown source contains at least one table header and alignment line pair.
/// 返回 Markdown 源文本是否至少包含一组表头行与对齐行。
fn raw_source_contains_markdown_table(markdown_source: &str) -> bool {
    let repaired_source = repair_compact_markdown_tables(markdown_source);
    let lines = repaired_source.lines().collect::<Vec<_>>();

    lines.windows(2).any(|window| {
        is_markdown_table_header_line(window[0]) && is_markdown_table_alignment_line(window[1])
    })
}

/// Repairs compact Markdown table clusters so rows glued onto one physical line can still be recognized by the custom transcript renderer.
/// 修复紧凑压缩的 Markdown 表格片段，使粘在同一物理行上的多行表格仍能被自定义转录渲染器识别。
fn repair_compact_markdown_tables(markdown_source: &str) -> String {
    let mut repaired_lines = Vec::new();
    let mut active_table_column_count = None::<usize>;

    for line in markdown_source.lines() {
        if let Some(column_count) = active_table_column_count {
            if let Some(expanded_body) = repair_compact_markdown_table_body_line(line, column_count)
            {
                let expanded_lines = expanded_body
                    .lines()
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                active_table_column_count = expanded_lines.last().and_then(|last_line| {
                    table_line_column_count(last_line)
                        .filter(|last_column_count| *last_column_count == column_count)
                });
                repaired_lines.extend(expanded_lines);
                continue;
            }

            if table_line_column_count(line)
                .is_some_and(|line_column_count| line_column_count == column_count)
                || is_markdown_table_alignment_line(line)
            {
                repaired_lines.push(line.to_string());
                continue;
            }

            active_table_column_count = None;
        }

        if let Some((expanded_table, column_count)) = repair_compact_markdown_table_line(line) {
            let expanded_lines = expanded_table
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            active_table_column_count = expanded_lines.last().and_then(|last_line| {
                table_line_column_count(last_line)
                    .filter(|last_column_count| *last_column_count == column_count)
            });
            repaired_lines.extend(expanded_lines);
            continue;
        }

        repaired_lines.push(line.to_string());
    }

    repaired_lines.join("\n")
}

/// Repairs one compact Markdown table line when a prose prefix or multiple table rows were collapsed onto the same line.
/// 当正文前缀或多行表格被压到同一行时，修复该紧凑 Markdown 表格行。
fn repair_compact_markdown_table_line(line: &str) -> Option<(String, usize)> {
    if !line.contains("||") || line.matches('|').count() < 6 {
        return None;
    }

    for (table_start, _) in line.match_indices('|') {
        let prefix = line[..table_start].trim_end();
        let table_candidate = &line[table_start..];

        let Some((expanded_table, column_count)) =
            parse_compact_markdown_table_cluster(table_candidate)
        else {
            continue;
        };

        if prefix.is_empty() {
            return Some((expanded_table, column_count));
        }

        return Some((format!("{prefix}\n{expanded_table}"), column_count));
    }

    None
}

/// Repairs one compact Markdown table body line using the active table column count so glued body rows can be restored.
/// 使用当前活动表格的列数修复一条紧凑 Markdown 表格正文行，使被粘连的正文行恢复正常。
fn repair_compact_markdown_table_body_line(line: &str, column_count: usize) -> Option<String> {
    if column_count < 2 || !line.contains("||") {
        return None;
    }

    let content_start = line
        .char_indices()
        .find_map(|(byte_index, character)| (!character.is_whitespace()).then_some(byte_index))
        .unwrap_or(line.len());
    let prefix = &line[..content_start];
    let mut rest = &line[content_start..];
    let mut repaired_rows = Vec::new();

    loop {
        rest = rest.trim_start_matches([' ', '\t']);

        if !rest.starts_with('|') {
            break;
        }

        let Some((row_end, row_source)) = consume_compact_markdown_table_row(rest, column_count)
        else {
            break;
        };

        if table_line_column_count(row_source)
            .filter(|row_column_count| *row_column_count == column_count)
            .is_none()
        {
            break;
        }

        repaired_rows.push(format!("{prefix}{}", row_source.trim()));
        rest = &rest[row_end..];
    }

    if repaired_rows.len() < 2 {
        return None;
    }

    let suffix = rest.trim_start();
    let mut repaired = repaired_rows.join("\n");
    if !suffix.is_empty() {
        repaired.push('\n');
        repaired.push_str(suffix);
    }

    Some(repaired)
}

/// Parses one compact Markdown table cluster into a newline-separated block when row boundaries were collapsed into `||`.
/// 当行边界被压缩成 `||` 时，将一个紧凑 Markdown 表格片段解析为按换行分隔的区块。
fn parse_compact_markdown_table_cluster(table_source: &str) -> Option<(String, usize)> {
    for (boundary_index, _) in table_source.match_indices("||") {
        let header_row = &table_source[..boundary_index + 1];
        let header_cells = parse_markdown_table_cells(header_row);
        if header_cells.len() < 2 {
            continue;
        }

        let alignment_and_rest = &table_source[boundary_index + 1..];
        let Some((alignment_end, alignment_row)) =
            consume_compact_markdown_table_row(alignment_and_rest, header_cells.len())
        else {
            continue;
        };

        if !is_markdown_table_alignment_line(alignment_row) {
            continue;
        }

        let mut rows = vec![
            header_row.trim().to_string(),
            alignment_row.trim().to_string(),
        ];
        let mut rest = &alignment_and_rest[alignment_end..];

        loop {
            rest = rest.trim_start_matches([' ', '\t']);

            if !rest.starts_with('|') {
                break;
            }

            let Some((body_end, body_row)) =
                consume_compact_markdown_table_row(rest, header_cells.len())
            else {
                break;
            };

            rows.push(body_row.trim().to_string());
            rest = &rest[body_end..];
        }

        let suffix = rest.trim_start();
        let mut expanded = rows.join("\n");
        if !suffix.is_empty() {
            expanded.push('\n');
            expanded.push_str(suffix);
        }

        return Some((expanded, header_cells.len()));
    }

    None
}

/// Consumes one compact Markdown table row with the expected column count and returns the byte length together with the row slice.
/// 按预期列数消费一行紧凑 Markdown 表格，并返回其字节长度和该行切片。
fn consume_compact_markdown_table_row(
    row_source: &str,
    column_count: usize,
) -> Option<(usize, &str)> {
    if !row_source.starts_with('|') || column_count < 2 {
        return None;
    }

    let mut pipe_count = 0usize;

    for (byte_index, character) in row_source.char_indices() {
        if character == '|' {
            pipe_count += 1;

            if pipe_count == column_count + 1 {
                let row_end = byte_index + character.len_utf8();
                return Some((row_end, &row_source[..row_end]));
            }
        } else if character == '\n' {
            return None;
        }
    }

    None
}

/// Returns whether the provided line can serve as the header row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的表头行。
fn is_markdown_table_header_line(line: &str) -> bool {
    let cells = split_markdown_table_cells(line);
    cells.len() >= 2
}

/// Returns whether the provided line can serve as the alignment row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的对齐行。
fn is_markdown_table_alignment_line(line: &str) -> bool {
    let cells = split_markdown_table_cells(line);

    cells.len() >= 2
        && cells.iter().all(|cell| {
            let trimmed = cell.trim();
            !trimmed.is_empty()
                && trimmed.contains('-')
                && trimmed
                    .chars()
                    .all(|character| matches!(character, '-' | ':' | ' '))
        })
}

/// Returns whether the provided line can serve as one body row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的一行正文。
fn is_markdown_table_body_line(line: &str) -> bool {
    split_markdown_table_cells(line).len() >= 2
}

/// Returns the cell count when the provided line already matches one well-formed Markdown table row.
/// 当给定行已经符合结构正常的 Markdown 表格行时，返回其单元格数量。
fn table_line_column_count(line: &str) -> Option<usize> {
    let cells = split_markdown_table_cells(line);
    (!cells.is_empty()).then_some(cells.len())
}

/// Splits one Markdown table row into trimmed raw cell sources without interpreting inline Markdown syntax.
/// 将一行 Markdown 表格拆分为去除首尾空白的原始单元格源码，而不解释其中的行内 Markdown 语法。
fn split_markdown_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return Vec::new();
    }

    let trimmed = trimmed.trim_matches('|');
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect::<Vec<_>>()
}

/// Parses one Markdown table row into trimmed cell payloads.
/// 将一行 Markdown 表格解析为去除首尾空白的单元格载荷。
fn parse_markdown_table_cells(line: &str) -> Vec<String> {
    split_markdown_table_cells(line)
        .into_iter()
        .map(|cell| {
            vulcan_agent_view::markdown_source_to_plain_text(cell.trim())
                .replace('\n', " ")
                .trim()
                .to_string()
        })
        .collect::<Vec<_>>()
}

/// Parses one Markdown table source slice into a structured table payload ready for the custom renderer.
/// 将一段 Markdown 表格源码解析为供自定义渲染器使用的结构化表格载荷。
fn parse_markdown_table_source(table_lines: &[&str]) -> Option<ParsedMarkdownTable> {
    let header_cells = parse_markdown_table_cells(*table_lines.first()?);
    let alignment_cells = split_markdown_table_cells(*table_lines.get(1)?);
    if header_cells.is_empty() || header_cells.len() != alignment_cells.len() {
        return None;
    }

    let body_rows = table_lines
        .iter()
        .skip(2)
        .map(|line| parse_markdown_table_cells(line))
        .collect::<Vec<_>>();
    let alignments = alignment_cells
        .iter()
        .map(|cell| parse_markdown_table_alignment(cell))
        .collect::<Vec<_>>();
    Some(ParsedMarkdownTable {
        header_cells,
        alignments,
        body_rows,
    })
}

/// Builds the default rendered-table style from the surrounding paragraph context and current theme.
/// 根据周围段落上下文和当前主题构建默认渲染表格样式。
fn rendered_table_style(
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) -> RenderedTableStyle {
    let palette = app_theme.extended_palette();
    let body_background = paragraph_style
        .background
        .map(|background| background.background)
        .unwrap_or_else(|| Background::Color(palette.background.base.color.scale_alpha(0.98)));

    RenderedTableStyle {
        text_size: settings.text_size,
        indent_left: paragraph_style.indent_left,
        gap_before: paragraph_style.gap_before,
        gap_after: paragraph_style.gap_after.max(8.0),
        cell_padding: Padding::from([8.0, 10.0]),
        border_color: palette.background.strong.color.scale_alpha(0.24),
        header_background: Background::Color(palette.background.weak.color.scale_alpha(0.88)),
        body_background,
        quote_depth: paragraph_style.quote_depth,
    }
}

/// Builds one rendered table-cell paragraph node with script-aware font routing and a stable global character range.
/// 构建一个带脚本感知字体路由和稳定全局字符范围的渲染表格单元格段落节点。
fn build_table_cell_paragraph_node(
    cell_text: String,
    global_char_range: Range<usize>,
    is_header: bool,
    text_size: Pixels,
    fallback_text_color: Color,
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
            Some(fallback_text_color),
            app_theme,
        )
    };
    let (grapheme_spans, local_grapheme_ranges) = explode_spans_to_graphemes(&spans);

    RenderedParagraphNode {
        display_text: cell_text,
        spans: grapheme_spans,
        global_char_range,
        local_grapheme_ranges,
        text_size,
        line_height: text::LineHeight::default(),
        wrapping: text::Wrapping::Word,
        indent_left: 0.0,
        gap_before: 0.0,
        gap_after: 0.0,
        inner_padding: Padding::ZERO,
        background: None,
        quote_depth: 0,
    }
}

/// Parses one Markdown table alignment cell into the corresponding selectable grid alignment.
/// 将一个 Markdown 表格对齐单元格解析为对应的可选网格对齐方式。
fn parse_markdown_table_alignment(cell: &str) -> SelectableTableAlignment {
    let trimmed = cell.trim();

    match (trimmed.starts_with(':'), trimmed.ends_with(':')) {
        (true, true) => SelectableTableAlignment::Center,
        (false, true) => SelectableTableAlignment::Right,
        _ => SelectableTableAlignment::Left,
    }
}

/// Returns the heading text size matching the provided Markdown heading level.
/// 返回与给定 Markdown 标题级别匹配的标题字号。
fn heading_size(settings: markdown::Settings, level: &markdown::HeadingLevel) -> Pixels {
    match level {
        markdown::HeadingLevel::H1 => settings.h1_size,
        markdown::HeadingLevel::H2 => settings.h2_size,
        markdown::HeadingLevel::H3 => settings.h3_size,
        markdown::HeadingLevel::H4 => settings.h4_size,
        markdown::HeadingLevel::H5 => settings.h5_size,
        markdown::HeadingLevel::H6 => settings.h6_size,
    }
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

/// Returns the visible origin at which one paragraph should be drawn inside its padded background box.
/// 返回段落在其带内边距背景框中应绘制的可见起点。
fn paragraph_local_origin(
    outer_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> Point {
    Point::new(
        outer_bounds.x + paragraph_node.inner_padding.left,
        outer_bounds.y + paragraph_node.inner_padding.top,
    )
}

/// Draws the active selection portion intersecting one paragraph node.
/// 绘制与某个段落节点相交的当前活动选择区域。
fn draw_paragraph_selection(
    renderer: &mut Renderer,
    document: &RenderedTranscriptDocument,
    paragraph_node: &RenderedParagraphNode,
    paragraph: &RendererParagraph,
    selection_start: usize,
    selection_end: usize,
    paragraph_origin: Point,
    viewport: &Rectangle,
) {
    let local_selection_start =
        selection_start.saturating_sub(paragraph_node.global_char_range.start);
    let local_selection_end = selection_end
        .min(paragraph_node.global_char_range.end)
        .saturating_sub(paragraph_node.global_char_range.start);

    if local_selection_start >= local_selection_end {
        return;
    }

    for (grapheme_index, grapheme_range) in paragraph_node.local_grapheme_ranges.iter().enumerate()
    {
        if grapheme_range.end <= local_selection_start
            || grapheme_range.start >= local_selection_end
        {
            continue;
        }

        for bounds in paragraph.span_bounds(grapheme_index) {
            let translated_bounds = Rectangle::new(
                bounds.position() + (paragraph_origin - Point::ORIGIN),
                bounds.size(),
            );

            if let Some(visible_bounds) = viewport.intersection(&translated_bounds) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: visible_bounds,
                        ..renderer::Quad::default()
                    },
                    Background::Color(document.selection_color),
                );
            }
        }
    }
}

/// Performs hit-testing against the rendered transcript and returns the nearest global character offset.
/// 对渲染转录执行命中测试，并返回最近的全局字符偏移位置。
fn hit_test_rendered_document(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: Point,
) -> Option<usize> {
    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut current_y = widget_bounds.y;

    for node in &document.nodes {
        match node {
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                current_y += paragraph_node.gap_before;
                let paragraph = state.paragraphs.get(paragraph_index)?;
                let paragraph_height = paragraph.min_bounds().height;
                let outer_bounds = Rectangle::new(
                    Point::new(widget_bounds.x + paragraph_node.indent_left, current_y),
                    Size::new(
                        (widget_bounds.width - paragraph_node.indent_left).max(1.0),
                        paragraph_node.inner_padding.top
                            + paragraph_height
                            + paragraph_node.inner_padding.bottom,
                    ),
                );

                if outer_bounds.contains(position) {
                    let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                        Point::new(
                            position.x - outer_bounds.x - paragraph_node.inner_padding.left,
                            position.y - outer_bounds.y - paragraph_node.inner_padding.top,
                        ),
                        paragraph_node,
                        outer_bounds.size(),
                        paragraph_height,
                    );

                    let local_byte_offset = paragraph_hit_test_to_byte_offset(
                        paragraph,
                        &paragraph_node.display_text,
                        local_point,
                    )?;
                    let local_char_offset =
                        byte_offset_to_char_offset(&paragraph_node.display_text, local_byte_offset);
                    return Some(
                        (paragraph_node.global_char_range.start + local_char_offset)
                            .min(paragraph_node.global_char_range.end),
                    );
                }

                current_y += outer_bounds.height + paragraph_node.gap_after;
                paragraph_index += 1;
            }
            RenderedTranscriptNode::Table(table_node) => {
                let table_layout = state.table_layouts.get(table_index)?;
                let table_paragraphs = &state.paragraphs[table_layout.paragraph_range.clone()];
                let measured_table = measure_rendered_table_node(
                    table_node,
                    table_layout,
                    table_paragraphs,
                    widget_bounds,
                    current_y,
                );

                if let Some(hit_char) = hit_test_rendered_table_node(
                    table_node,
                    table_paragraphs,
                    &measured_table,
                    position,
                ) {
                    return Some(hit_char);
                }

                current_y += measured_table.outer_bounds.height + table_node.gap_after;
                paragraph_index = table_layout.paragraph_range.end;
                table_index += 1;
            }
            RenderedTranscriptNode::Rule(rule_node) => {
                current_y += rule_node.gap_before + 8.0 + rule_node.gap_after;
            }
        }
    }

    None
}

/// Converts one paragraph-local hit-test point into a paragraph-global UTF-8 byte offset across all visual lines.
/// 将一个段落局部命中点转换为覆盖全部可视行的段落全局 UTF-8 字节偏移。
fn paragraph_hit_test_to_byte_offset(
    paragraph: &RendererParagraph,
    paragraph_text: &str,
    local_point: Point,
) -> Option<usize> {
    let cursor = paragraph.buffer().hit(local_point.x, local_point.y)?;
    Some(multiline_cursor_to_byte_offset(
        paragraph_text,
        cursor.line,
        cursor.index,
    ))
}

/// Converts one multi-line cursor location into a paragraph-global UTF-8 byte offset while preserving explicit line breaks.
/// 将一个多行光标位置转换为段落全局 UTF-8 字节偏移，并保留显式换行符的占位。
fn multiline_cursor_to_byte_offset(
    paragraph_text: &str,
    line_index: usize,
    line_byte_offset: usize,
) -> usize {
    let mut accumulated_byte_offset = 0usize;

    for (current_line_index, source_line) in paragraph_text.split_inclusive('\n').enumerate() {
        let line_text = source_line.strip_suffix('\n').unwrap_or(source_line);

        if current_line_index == line_index {
            return accumulated_byte_offset + line_byte_offset.min(line_text.len());
        }

        accumulated_byte_offset += source_line.len();
    }

    if line_index == paragraph_text.lines().count() {
        paragraph_text.len()
    } else {
        accumulated_byte_offset.min(paragraph_text.len())
    }
}

/// Clamps one local hit-test point so padded block regions resolve onto the actual text bounds.
/// 将局部命中测试点钳制到真实文本边界内，使带内边距的区块区域命中到实际文本。
fn clamp_hit_test_point_to_paragraph_text_bounds(
    local_point: Point,
    paragraph_node: &RenderedParagraphNode,
    outer_size: Size,
    paragraph_height: f32,
) -> Point {
    let max_x =
        (outer_size.width - paragraph_node.inner_padding.left - paragraph_node.inner_padding.right)
            .max(1.0)
            - 0.5;
    let max_y = paragraph_height.max(1.0) - 0.5;

    Point::new(
        local_point.x.clamp(0.0, max_x),
        local_point.y.clamp(0.0, max_y),
    )
}

/// Returns the normalized `(start, end)` selection tuple when the current selection is non-empty.
/// 当当前选择非空时返回归一化后的 `(start, end)` 选择区间元组。
fn normalized_selection(state: &SelectableTranscriptState) -> Option<(usize, usize)> {
    let start = state.anchor_char.min(state.head_char);
    let end = state.anchor_char.max(state.head_char);

    if start == end {
        None
    } else {
        Some((start, end))
    }
}

/// Returns whether the provided key event matches the copy shortcut on the current platform.
/// 返回给定按键事件是否匹配当前平台上的复制快捷键。
fn shortcut_matches_copy(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> bool {
    modifiers.command()
        && !modifiers.alt()
        && matches!(
            key,
            keyboard::Key::Character(character) if character.eq_ignore_ascii_case("c")
        )
        || modifiers.command()
            && !modifiers.alt()
            && text.is_some_and(|value| value.eq_ignore_ascii_case("c"))
}

/// Returns whether the provided key event matches the select-all shortcut on the current platform.
/// 返回给定按键事件是否匹配当前平台上的全选快捷键。
fn shortcut_matches_select_all(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> bool {
    modifiers.command()
        && !modifiers.alt()
        && matches!(
            key,
            keyboard::Key::Character(character) if character.eq_ignore_ascii_case("a")
        )
        || modifiers.command()
            && !modifiers.alt()
            && text.is_some_and(|value| value.eq_ignore_ascii_case("a"))
}

/// Returns one directive-like background treatment from the provided fill and border colors.
/// 根据提供的填充色与边框色返回一个指令风格背景样式。
fn directive_background(fill: Color, border_color: Color) -> RenderedParagraphBackground {
    RenderedParagraphBackground {
        background: Background::Color(fill),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 10.0.into(),
        },
    }
}

/// Returns the semantic diff color used for one rendered diff line.
/// 返回某个渲染 diff 行使用的语义颜色。
fn diff_line_color(app_theme: &Theme, line_kind: DiffLineKind) -> Color {
    let palette = app_theme.extended_palette();

    match line_kind {
        DiffLineKind::Added => palette.success.strong.color,
        DiffLineKind::Removed => palette.danger.strong.color,
        DiffLineKind::Note => palette.primary.strong.color,
        DiffLineKind::Context => palette.background.base.text.scale_alpha(0.92),
    }
}

/// Classifies one raw diff line into its semantic line kind.
/// 将一条原始 diff 行分类为对应的语义行类型。
fn classify_diff_line_kind(line: &str) -> DiffLineKind {
    if line.starts_with('+') && !line.starts_with("+++") {
        DiffLineKind::Added
    } else if line.starts_with('-') && !line.starts_with("---") {
        DiffLineKind::Removed
    } else if line.starts_with("@@")
        || line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        DiffLineKind::Note
    } else {
        DiffLineKind::Context
    }
}

/// Markdown render branch used for script-aware span routing.
/// 用于脚本感知片段路由的 Markdown 渲染分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkdownRenderKind {
    /// Flow prose branch used by normal headings and paragraphs.
    /// 常规标题与段落使用的正文流分支。
    Flow,
    /// Code branch used by fenced code blocks and similar terminal-like nodes.
    /// 围栏代码块和类似终端节点使用的代码分支。
    CodeBlock,
}

/// Plain-text render branch used for script-aware span routing.
/// 用于脚本感知片段路由的纯文本渲染分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlainTextRenderKind {
    /// Flow prose branch used by normal transcript prose.
    /// 常规转录正文使用的正文流分支。
    Flow,
    /// Code branch used by code-like transcript output.
    /// 代码风格转录输出使用的代码分支。
    Code,
}

/// Alignment used by the selectable monospace table fallback renderer.
/// 可选择等宽表格回退渲染器使用的对齐方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectableTableAlignment {
    /// Left-aligned cell content.
    /// 左对齐的单元格内容。
    Left,
    /// Center-aligned cell content.
    /// 居中的单元格内容。
    Center,
    /// Right-aligned cell content.
    /// 右对齐的单元格内容。
    Right,
}

/// Script bucket used by transcript span routing so Latin and East Asian glyphs can choose different bundled families.
/// 转录片段路由使用的脚本分组，使拉丁与东亚字形能够选择不同的内置字体族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptScript {
    /// Latin-like glyphs and neutral punctuation handled by the primary UI family.
    /// 由主界面字体族承载的拉丁类字形与中性标点。
    Latin,
    /// East Asian glyphs that should stay on the bundled CJK fallback family.
    /// 应保持在内置 CJK 兜底字体族上的东亚字形。
    Cjk,
}

/// Builds routed rich-text spans from one parsed Markdown text payload while preserving links and emphasis metadata.
/// 从一段已解析 Markdown 文本载荷构建带路由的富文本片段，同时保留链接与强调元数据。
fn build_agent_view_markdown_spans(
    text: &markdown::Text,
    style: markdown::Style,
    render_kind: MarkdownRenderKind,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let _ = app_theme;
    let base_spans = text.spans(style);
    let mut output = Vec::new();

    for source_span in base_spans.iter() {
        let span_uses_code_branch = matches!(render_kind, MarkdownRenderKind::CodeBlock)
            || source_span.highlight.is_some()
            || source_span.font == Some(style.inline_code_font);
        let requested_font = source_span.font.unwrap_or(if span_uses_code_branch {
            style.code_block_font
        } else {
            style.font
        });

        for (script, fragment) in split_markdown_fragment(source_span.text.as_ref()) {
            let routed_font = match (span_uses_code_branch, script) {
                (true, TranscriptScript::Latin) => theme::normalize_code_latin_font(requested_font),
                (true, TranscriptScript::Cjk) => theme::normalize_code_cjk_font(requested_font),
                (false, TranscriptScript::Latin) => theme::normalize_ui_latin_font(requested_font),
                (false, TranscriptScript::Cjk) => theme::normalize_ui_cjk_font(requested_font),
            };

            let mut routed_span = source_span.clone();
            routed_span.text = fragment.into();
            routed_span.font = Some(routed_font);
            output.push(routed_span);
        }
    }

    output
}

/// Builds routed rich-text spans from one plain-text payload while preserving Latin and CJK fallback strategy.
/// 从一段纯文本载荷构建带路由的富文本片段，同时保留拉丁与 CJK 的兜底策略。
fn build_agent_view_plain_text_spans(
    text: &str,
    requested_font: Font,
    render_kind: PlainTextRenderKind,
    color: Option<Color>,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let _ = app_theme;
    let fragments = if text.is_empty() {
        vec![(TranscriptScript::Latin, String::from(" "))]
    } else {
        split_markdown_fragment(text)
    };
    let mut output = Vec::new();

    for (script, fragment) in fragments {
        let routed_font = match (render_kind, script) {
            (PlainTextRenderKind::Code, TranscriptScript::Latin) => {
                theme::normalize_code_latin_font(requested_font)
            }
            (PlainTextRenderKind::Code, TranscriptScript::Cjk) => {
                theme::normalize_code_cjk_font(requested_font)
            }
            (PlainTextRenderKind::Flow, TranscriptScript::Latin) => {
                theme::normalize_ui_latin_font(requested_font)
            }
            (PlainTextRenderKind::Flow, TranscriptScript::Cjk) => {
                theme::normalize_ui_cjk_font(requested_font)
            }
        };

        let mut routed_span = Span::new(fragment).font(routed_font);
        if let Some(color) = color {
            routed_span = routed_span.color(color);
        }
        output.push(routed_span);
    }

    output
}

/// Splits one fragment into routed Latin-or-CJK runs while keeping neutral punctuation attached to surrounding scripts.
/// 将一段文本切分为路由后的拉丁或 CJK 片段，同时让中性标点依附于周围脚本。
fn split_markdown_fragment(fragment: &str) -> Vec<(TranscriptScript, String)> {
    let mut runs = Vec::new();
    let mut active_fragment = String::new();
    let mut active_script = None;

    for character in fragment.chars() {
        let character_script = if is_neutral_character(character) {
            active_script.unwrap_or(TranscriptScript::Latin)
        } else {
            classify_script(character)
        };

        if active_script.is_some() && active_script != Some(character_script) {
            runs.push((
                active_script.unwrap_or(TranscriptScript::Latin),
                std::mem::take(&mut active_fragment),
            ));
        }

        active_script = Some(character_script);
        active_fragment.push(character);
    }

    if !active_fragment.is_empty() {
        runs.push((
            active_script.unwrap_or(TranscriptScript::Latin),
            active_fragment,
        ));
    }

    runs
}

/// Returns whether one character should follow the surrounding script bucket instead of opening a new run.
/// 返回某个字符是否应跟随周围脚本分组，而不是开启新的片段。
fn is_neutral_character(character: char) -> bool {
    character.is_whitespace()
        || character.is_ascii_punctuation()
        || matches!(
            character,
            '·' | '•' | '‧' | '…' | '—' | '–' | '―' | '“' | '”' | '‘' | '’'
        )
}

/// Classifies one character into the transcript Latin-or-CJK routing buckets.
/// 将某个字符归类到转录渲染使用的拉丁或 CJK 路由分组中。
fn classify_script(character: char) -> TranscriptScript {
    if matches!(
        character as u32,
        0x2E80..=0x2FDF
            | 0x3040..=0x30FF
            | 0x3100..=0x312F
            | 0x31A0..=0x31BF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFFEF
            | 0x20000..=0x3FFFD
    ) {
        TranscriptScript::Cjk
    } else {
        TranscriptScript::Latin
    }
}

/// Extracts one character-based slice from a string without panicking when the range exceeds bounds.
/// 在范围超出边界时也不会 panic 地从字符串中提取一个基于字符的切片。
fn slice_char_range(text: &str, start_char: usize, end_char: usize) -> String {
    let clamped_start = start_char.min(text.chars().count());
    let clamped_end = end_char.min(text.chars().count());
    if clamped_start >= clamped_end {
        return String::new();
    }

    text.chars()
        .skip(clamped_start)
        .take(clamped_end - clamped_start)
        .collect()
}

/// Converts one byte offset reported by the text backend into a safe character offset.
/// 将文本后端返回的字节偏移转换为安全的字符偏移。
fn byte_offset_to_char_offset(text: &str, byte_offset: usize) -> usize {
    let clamped_byte_offset = byte_offset.min(text.len());
    let safe_byte_offset = if text.is_char_boundary(clamped_byte_offset) {
        clamped_byte_offset
    } else {
        let mut safe_byte_offset = clamped_byte_offset;
        while safe_byte_offset > 0 && !text.is_char_boundary(safe_byte_offset) {
            safe_byte_offset -= 1;
        }
        safe_byte_offset
    };

    text[..safe_byte_offset].chars().count()
}

#[cfg(test)]
mod tests {
    use super::{
        RenderedParagraphNode, byte_offset_to_char_offset,
        clamp_hit_test_point_to_paragraph_text_bounds, expand_compact_markdown_table_line,
        multiline_cursor_to_byte_offset, normalize_markdown_source_for_transcript_display,
        parse_markdown_table_source, prepare_markdown_source_for_rendering,
    };
    use iced::advanced::text;
    use iced::widget::markdown;
    use iced::widget::text::Span;
    use iced::{Padding, Pixels, Point, Size};
    use std::ops::Range;

    /// Verifies ASCII byte offsets remain identical after conversion because one byte still means one visible character.
    /// 验证 ASCII 字节偏移在转换后保持一致，因为单字节仍然对应一个可见字符。
    #[test]
    fn byte_offsets_match_char_offsets_for_ascii_text() {
        let text = "hello world";

        assert_eq!(byte_offset_to_char_offset(text, 0), 0);
        assert_eq!(byte_offset_to_char_offset(text, 1), 1);
        assert_eq!(byte_offset_to_char_offset(text, 5), 5);
        assert_eq!(byte_offset_to_char_offset(text, text.len()), 11);
    }

    /// Verifies UTF-8 byte offsets from mixed Chinese and Latin text are normalized back to logical character positions.
    /// 验证混合中英文本的 UTF-8 字节偏移会被归一化回逻辑字符位置。
    #[test]
    fn byte_offsets_are_normalized_for_mixed_cjk_text() {
        let text = "A中B文C";

        assert_eq!(byte_offset_to_char_offset(text, 0), 0);
        assert_eq!(byte_offset_to_char_offset(text, 1), 1);
        assert_eq!(byte_offset_to_char_offset(text, 4), 2);
        assert_eq!(byte_offset_to_char_offset(text, 5), 3);
        assert_eq!(byte_offset_to_char_offset(text, 8), 4);
        assert_eq!(byte_offset_to_char_offset(text, text.len()), 5);
    }

    /// Verifies invalid in-character byte offsets are snapped back to the previous character boundary instead of drifting forward.
    /// 验证落在字符内部的非法字节偏移会回退到上一个字符边界，而不是继续向前漂移。
    #[test]
    fn invalid_utf8_byte_offsets_snap_to_previous_boundary() {
        let text = "中A";

        assert_eq!(byte_offset_to_char_offset(text, 1), 0);
        assert_eq!(byte_offset_to_char_offset(text, 2), 0);
        assert_eq!(byte_offset_to_char_offset(text, 3), 1);
        assert_eq!(byte_offset_to_char_offset(text, 4), 2);
    }

    /// Verifies plain transcript prose keeps single source newlines visible while fenced code keeps its original line semantics.
    /// 验证普通转录正文会保留单个源码换行的可见性，同时围栏代码保持原始换行语义。
    #[test]
    fn transcript_markdown_normalization_preserves_visible_plain_line_breaks() {
        let source = "第一行\n第二行\n\n```rust\nlet value = 1;\nlet next = 2;\n```";
        let normalized = normalize_markdown_source_for_transcript_display(source);

        assert!(normalized.contains("第一行  \n第二行"));
        assert!(normalized.contains("```rust\nlet value = 1;\nlet next = 2;\n```"));
    }

    /// Verifies Markdown table source is parsed into a structured table payload instead of falling back to a placeholder sentence.
    /// 验证 Markdown 表格源码会被解析为结构化表格载荷，而不是退化成占位提示语。
    #[test]
    fn markdown_tables_parse_into_structured_payload() {
        let table_lines = [
            "| 列1 | 列2 | 列3 |",
            "|---|---|---|",
            "| 数据A | 数据B | 数据C |",
            "| 数据D | 数据E | 数据F |",
        ];

        let parsed = parse_markdown_table_source(&table_lines)
            .expect("table source should parse into one structured payload");

        assert_eq!(parsed.header_cells, vec!["列1", "列2", "列3"]);
        assert_eq!(parsed.body_rows.len(), 2);
        assert_eq!(parsed.body_rows[0], vec!["数据A", "数据B", "数据C"]);
        assert_eq!(parsed.body_rows[1], vec!["数据D", "数据E", "数据F"]);
    }

    /// Verifies one compacted prose-plus-table line is expanded back into paragraph text followed by real table lines.
    /// 验证一条被压缩的“正文加表格”行会被展开回正文和真实表格行。
    #[test]
    fn compacted_markdown_table_boundaries_are_repaired_before_rendering() {
        let source = "以下是示例表格。|列1|列2||---|---||数据A|数据B||数据C|数据D|";
        let repaired = prepare_markdown_source_for_rendering(source);

        assert_eq!(
            repaired,
            "以下是示例表格。\n|列1|列2|\n|---|---|\n|数据A|数据B|\n|数据C|数据D|"
        );
    }

    /// Verifies a prose-prefixed compact table with multiple glued body rows keeps expanding into a valid multi-line table.
    /// 验证带正文前缀且多行正文被粘连的紧凑表格仍会被展开为合法的多行表格。
    #[test]
    fn real_world_compacted_table_output_is_repaired() {
        let source = "以下是基于当前协调代理配置与调度上下文的示例表格。如需指定具体字段、数据源或业务场景，请补充说明，我将按需生成或调整。|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注||:---|:---|:---|:---|:---|\n|`default-worker`| 常规任务处理、代码生成、基础工具调用|待命 (Standby)| 默认路由，无特殊技能或记忆需求时 |当前阶段仅注入协作上下文，不触发真实执行|| `lua-specialist` | Lua脚本解析、技能执行、动态逻辑编排 | 待命 (Standby)| 任务匹配`.lua` 文件或`registry.list_skills` 返回结果| 依赖后续阶段工具注册增强|\n| `memory-specialist`| 上下文记忆管理、历史状态检索、长期知识维护 |待命 (Standby)| 需跨会话上下文或显式调用记忆接口时 |当前仅支持上下文注入，未启用持久化存储|| `coordinator (本代理)` | 路由决策、上下文注入、执行链编排 |运行中 (Active)| 接收用户指令后按`Plan First`模式评估分发| 当前阶段仅做确定性调度规划，不自动拉起子代理 |> 💡 **提示**：当前处于 `Plan First`阶段";
        let repaired = prepare_markdown_source_for_rendering(source);

        assert!(
            repaired.contains(
                "我将按需生成或调整。\n|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注|"
            )
        );
        assert!(repaired.contains("\n|:---|:---|:---|:---|:---|"));
        assert!(repaired.contains("\n|`default-worker`| 常规任务处理、代码生成、基础工具调用|待命 (Standby)| 默认路由，无特殊技能或记忆需求时 |当前阶段仅注入协作上下文，不触发真实执行|"));
        assert!(repaired.contains("\n| `lua-specialist` | Lua脚本解析、技能执行、动态逻辑编排 | 待命 (Standby)| 任务匹配`.lua` 文件或`registry.list_skills` 返回结果| 依赖后续阶段工具注册增强|"));
        assert!(repaired.contains("\n| `memory-specialist`| 上下文记忆管理、历史状态检索、长期知识维护 |待命 (Standby)| 需跨会话上下文或显式调用记忆接口时 |当前仅支持上下文注入，未启用持久化存储|"));
        assert!(repaired.contains("\n| `coordinator (本代理)` | 路由决策、上下文注入、执行链编排 |运行中 (Active)| 接收用户指令后按`Plan First`模式评估分发| 当前阶段仅做确定性调度规划，不自动拉起子代理 |"));
    }

    /// Verifies the compact-table expander can at least expand the first prose-plus-header line from the real sample.
    /// 验证紧凑表格展开器至少能够展开真实样例中的首条“正文加表头”行。
    #[test]
    fn first_real_compacted_table_line_expands() {
        let line = "以下是基于当前协调代理配置与调度上下文的示例表格。如需指定具体字段、数据源或业务场景，请补充说明，我将按需生成或调整。|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注||:---|:---|:---|:---|:---|";
        let expanded = expand_compact_markdown_table_line(line);
        assert!(expanded.is_some());
    }

    /// Verifies padded block hit-testing is clamped back into the real text area instead of drifting inside decorative box padding.
    /// 验证带内边距区块的命中点会被钳制回真实文本区域，而不会在装饰性盒子留白中漂移。
    #[test]
    fn padded_block_hit_test_points_are_clamped_to_text_bounds() {
        let paragraph_node = RenderedParagraphNode {
            display_text: String::from("[思考过程]"),
            spans: vec![Span::<markdown::Uri, iced::Font>::new("[思考过程]")],
            global_char_range: Range { start: 0, end: 6 },
            local_grapheme_ranges: vec![0..6],
            text_size: Pixels(16.0),
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::Word,
            indent_left: 0.0,
            gap_before: 0.0,
            gap_after: 0.0,
            inner_padding: Padding::from([10.0, 12.0]),
            background: None,
            quote_depth: 0,
        };

        let clamped = clamp_hit_test_point_to_paragraph_text_bounds(
            Point::new(250.0, 80.0),
            &paragraph_node,
            Size::new(220.0, 44.0),
            24.0,
        );

        assert_eq!(clamped.x, 195.5);
        assert_eq!(clamped.y, 23.5);
    }

    /// Verifies multi-line hit offsets are accumulated across preceding lines instead of staying trapped on the first line.
    /// 验证多行命中偏移会累加前序各行，而不是始终被困在第一行。
    #[test]
    fn multiline_cursor_offsets_accumulate_across_previous_lines() {
        let text = "第一行\n第二行\nthird";

        assert_eq!(multiline_cursor_to_byte_offset(text, 0, 0), 0);
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 1, 0),
            "第一行\n".len()
        );
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 1, "第二".len()),
            "第一行\n第二".len()
        );
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 2, 2),
            "第一行\n第二行\nth".len()
        );
    }
}

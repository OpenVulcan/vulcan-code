//! Selectable rendered transcript widget used by the primary transcript surface.
//! 主转录渲染表面使用的可选择渲染转录组件。

use crate::iced_theme as theme;
use crate::{AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument};
use iced::advanced::Renderer as _;
use iced::advanced::clipboard::Kind as ClipboardKind;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::Renderer as _;
use iced::advanced::text::{self, Paragraph};
use iced::advanced::widget::tree;
use iced::advanced::{Clipboard, Shell, Widget, overlay};
use iced::font;
use iced::keyboard;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Padding, Pixels, Point, Rectangle,
    Renderer, Size, Theme, Vector,
};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::time::Instant;
use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};
use unicode_segmentation::UnicodeSegmentation;

/// Static format description used by the user-prompt meta row timestamp projection.
/// 用户提示词元信息行时间戳投影使用的静态格式描述。
const USER_PROMPT_TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]");

/// Maximum width ratio used by the right-aligned user-prompt bubble relative to the transcript viewport width.
/// 右对齐用户提示词气泡相对于转录视口宽度使用的最大宽度比例。
const USER_PROMPT_BUBBLE_MAX_WIDTH_RATIO: f32 = 0.70;

/// Fixed width used by the lightweight transcript context menu.
/// 转录轻量右键菜单使用的固定宽度。
const TRANSCRIPT_CONTEXT_MENU_WIDTH: f32 = 136.0;

/// Fixed row height used by each transcript context-menu item.
/// 每条转录右键菜单项使用的固定行高。
const TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0;

/// Outer padding applied around the transcript context-menu text labels.
/// 应用于转录右键菜单文本标签周围的外边距。
const TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X: f32 = 12.0;

/// Corner radius used by the lightweight transcript context menu.
/// 轻量转录右键菜单使用的圆角半径。
const TRANSCRIPT_CONTEXT_MENU_RADIUS: f32 = 10.0;

/// Link-action prefix used by the selectable transcript to identify copy-user-prompt chrome interactions.
/// 可选择转录用于识别复制用户提示词交互的链接动作前缀。
const USER_PROMPT_COPY_ACTION_PREFIX: &str = "vulcan-agent-view://copy-user-prompt/";

#[allow(dead_code)]
/// Link-action prefix used by compact collapsible semantic blocks such as tool call and tool result rows.
/// 工具调用与工具结果等紧凑折叠语义区块使用的链接动作前缀。
const BLOCK_TOGGLE_ACTION_PREFIX: &str = "vulcan-agent-view://toggle-block-inline/";

/// Link-action prefix used by compact tool rows that should open one dedicated modal instead of expanding inline.
/// 应打开专用弹窗而不是在正文中内联展开的紧凑工具行使用的链接动作前缀。
const TOOL_MODAL_ACTION_PREFIX: &str = "vulcan-agent-view://open-tool-modal/";

/// Link-action prefix used by the selectable transcript to identify inline thinking rows that should open the dedicated modal.
/// 可选择转录用于识别应打开专用弹层的内联思考行交互的链接动作前缀。
const THINKING_MODAL_ACTION_PREFIX: &str = "vulcan-agent-view://open-thinking-modal/";

/// Describes one active transcript drag auto-scroll request emitted while the pointer drags outside the visible bounds.
/// 描述一次当指针拖拽越出可见边界时发出的转录自动滚动请求。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectableTranscriptDragAutoScroll {
    /// Vertical direction that should be auto-scrolled while dragging.
    /// 拖拽期间应执行自动滚动的垂直方向。
    pub direction: SelectableTranscriptDragAutoScrollDirection,
    /// Pointer overshoot distance beyond the top or bottom edge, expressed in logical pixels.
    /// 指针超出顶部或底部边界的距离，单位为逻辑像素。
    pub overshoot: f32,
}

/// Enumerates the vertical auto-scroll direction requested by the transcript drag gesture.
/// 枚举转录拖拽手势请求的垂直自动滚动方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectableTranscriptDragAutoScrollDirection {
    /// Scrolls upward while the pointer stays above the visible transcript viewport.
    /// 当指针停留在可见转录视口上方时向上滚动。
    Up,
    /// Scrolls downward while the pointer stays below the visible transcript viewport.
    /// 当指针停留在可见转录视口下方时向下滚动。
    Down,
}

/// High-level transcript interaction emitted by synthetic chrome spans inside the rendered transcript.
/// 由渲染转录内部合成交互外框片段发出的高层转录交互。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectableTranscriptInteraction {
    /// Toggles one generic inline semantic block between collapsed and expanded states.
    /// 切换某个通用内联语义区块的折叠与展开状态。
    ToggleBlockInline(String),
    /// Opens one thinking block in the dedicated centered modal surface.
    /// 在专用居中弹层中打开某个思考区块。
    OpenThinkingModal(String),
    /// Opens one tool block in the dedicated centered modal surface.
    /// 在专用居中弹层中打开某个工具区块。
    OpenToolModal(String),
}

/// Enumerates the lightweight context-menu actions exposed by the selectable transcript.
/// 枚举可选择转录公开的轻量右键菜单动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptContextMenuAction {
    /// Copies the current non-empty transcript selection into the system clipboard.
    /// 将当前非空转录选区复制到系统剪贴板。
    CopySelection,
    /// Expands the current selection to cover the whole rendered transcript.
    /// 将当前选区扩展为覆盖整个渲染转录。
    SelectAll,
}

/// Captures one open transcript context-menu request anchored to a widget-space pointer position.
/// 记录一个锚定到组件空间指针位置的转录右键菜单打开请求。
#[derive(Debug, Clone, Copy, PartialEq)]
struct TranscriptContextMenuState {
    /// Pointer position used to anchor the top-left corner of the context menu after clamping.
    /// 在钳制之后用于锚定右键菜单左上角的指针位置。
    anchor_position: Point,
}

/// Builds one selectable rendered transcript element from the provided structured agent document.
/// 根据提供的结构化代理文档构建一个可选择的渲染转录元素。
pub fn view_selectable_rendered_transcript<'a, Message: 'a>(
    document: &AgentViewDocument,
    app_theme: &Theme,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
) -> Element<'a, Message> {
    let rendered_document = RenderedTranscriptDocument::from_agent_document(document, app_theme);
    Element::new(SelectableTranscript::new(
        rendered_document,
        on_drag_auto_scroll,
        on_interaction,
    ))
}

/// Custom widget keeping the rendered transcript selectable without falling back to a text editor surface.
/// 在不退回文本编辑器表面的情况下保持渲染转录可选择的自定义组件。
struct SelectableTranscript<'a, Message> {
    /// Fully materialized rendered transcript model used for layout, hit-testing, and copy.
    /// 用于布局、命中测试与复制的完整已物化渲染转录模型。
    document: RenderedTranscriptDocument,
    /// Callback used to bubble drag auto-scroll intent back to the parent scroll container.
    /// 用于将拖拽自动滚动意图回传给父级滚动容器的回调。
    on_drag_auto_scroll: Box<dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a>,
    /// Callback used to bubble high-level transcript interactions back to the application shell.
    /// 用于将高层转录交互回传给应用壳层的回调。
    on_interaction: Box<dyn Fn(SelectableTranscriptInteraction) -> Message + 'a>,
    /// Marker preserving the message type even though the widget is internally message-free.
    /// 即使组件内部不主动发消息，也用于保留消息类型的标记。
    message_marker: PhantomData<Message>,
}

impl<'a, Message> SelectableTranscript<'a, Message> {
    /// Creates one selectable transcript widget from a rendered transcript model.
    /// 根据一个渲染转录模型创建可选择转录组件。
    fn new(
        document: RenderedTranscriptDocument,
        on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
        on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    ) -> Self {
        Self {
            document,
            on_drag_auto_scroll: Box::new(on_drag_auto_scroll),
            on_interaction: Box::new(on_interaction),
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
    /// Whether the current drag interaction has already moved away from its initial anchor.
    /// 当前拖拽交互是否已经离开最初锚点位置。
    drag_moved: bool,
    /// Fixed anchor character offset used by the current selection.
    /// 当前选择使用的固定锚点字符偏移。
    anchor_char: usize,
    /// Moving head character offset used by the current selection.
    /// 当前选择使用的移动头部字符偏移。
    head_char: usize,
    /// Last drag auto-scroll request already published to the parent scroll container.
    /// 已经发布给父级滚动容器的最近一次拖拽自动滚动请求。
    drag_auto_scroll_request: Option<SelectableTranscriptDragAutoScroll>,
    /// Last synthetic link target clicked without dragging so double-click intent can be inferred.
    /// 最近一次未拖拽点击的合成链接目标，用于推导双击意图。
    last_click_link_target: Option<String>,
    /// Monotonic timestamp captured for the last synthetic link click.
    /// 上一次合成链接点击时捕获的单调时间戳。
    last_click_instant: Option<Instant>,
    /// Body scroll offsets retained per collapsible semantic block.
    /// 按折叠语义区块保留的正文滚动偏移量。
    collapsible_body_scroll_offsets: BTreeMap<String, f32>,
    /// Currently open lightweight context menu anchored inside the transcript surface.
    /// 当前在转录表面内部打开的轻量右键菜单。
    context_menu: Option<TranscriptContextMenuState>,
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
            drag_moved: false,
            anchor_char: 0,
            head_char: 0,
            drag_auto_scroll_request: None,
            last_click_link_target: None,
            last_click_instant: None,
            collapsible_body_scroll_offsets: BTreeMap::new(),
            context_menu: None,
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
    /// Link-scoped transcript actions keyed by their synthetic link target.
    /// 以合成链接目标为键保存的链接级转录动作集合。
    link_actions: BTreeMap<String, RenderedTranscriptLinkAction>,
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
            link_actions: builder.link_actions,
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

    /// Returns the Markdown link target covering the provided global character offset, if any.
    /// 返回覆盖给定全局字符偏移的 Markdown 链接目标（如果存在）。
    fn link_at_char_offset(&self, global_char_offset: usize) -> Option<String> {
        for node in &self.nodes {
            match node {
                RenderedTranscriptNode::Panel(_) => {}
                RenderedTranscriptNode::Collapsible(collapsible_node) => {
                    if let Some(link_url) =
                        paragraph_link_at_char_offset(&collapsible_node.summary, global_char_offset)
                    {
                        return Some(link_url);
                    }

                    if let Some(body_node) = collapsible_node.body.as_ref() {
                        if let Some(link_url) =
                            paragraph_link_at_char_offset(body_node, global_char_offset)
                        {
                            return Some(link_url);
                        }
                    }
                }
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    if let Some(link_url) =
                        paragraph_link_at_char_offset(paragraph_node, global_char_offset)
                    {
                        return Some(link_url);
                    }
                }
                RenderedTranscriptNode::Table(table_node) => {
                    for cell_node in &table_node.cells {
                        if let Some(link_url) =
                            paragraph_link_at_char_offset(&cell_node.paragraph, global_char_offset)
                        {
                            return Some(link_url);
                        }
                    }
                }
                RenderedTranscriptNode::Rule(_) => {}
            }
        }

        None
    }

    /// Returns the synthetic transcript action associated with one link target, if any.
    /// 返回与某个链接目标关联的合成转录动作（如果存在）。
    fn link_action_for_target(&self, link_target: &str) -> Option<&RenderedTranscriptLinkAction> {
        self.link_actions.get(link_target)
    }
}

/// Link-scoped interaction emitted by synthetic transcript chrome spans.
/// 由合成转录外框片段发出的链接级交互动作。
#[derive(Debug, Clone)]
enum RenderedTranscriptLinkAction {
    #[allow(dead_code)]
    /// Toggles one generic inline semantic block when the synthetic header chrome is clicked.
    /// 当点击合成标题外框时切换某个通用内联语义区块。
    ToggleBlockInline(String),
    /// Copies the provided payload into the clipboard instead of copying the synthetic link string itself.
    /// 将提供的载荷复制到剪贴板，而不是复制合成链接字符串本身。
    CopyText(String),
    /// Opens one inline thinking block inside the dedicated centered modal surface.
    /// 在专用居中弹层表面中打开某个内联思考区块。
    OpenThinkingModal(String),
    /// Opens one tool block in a dedicated centered modal surface.
    /// 在专用居中弹层表面中打开某个工具区块。
    OpenToolModal(String),
}

/// One rendered visual node in the transcript flow.
/// 转录流中的一个渲染可视节点。
#[derive(Debug, Clone)]
enum RenderedTranscriptNode {
    /// Non-selectable background panel rendered behind a contiguous run of following child nodes.
    /// 绘制在一段连续后续子节点背后的不可选择背景面板节点。
    Panel(RenderedPanelNode),
    #[allow(dead_code)]
    /// Compact collapsible semantic block rendered as one lightweight summary row plus one optional expandable body viewport.
    /// 以轻量摘要行加可选展开正文视口形式渲染的紧凑折叠语义区块。
    Collapsible(RenderedCollapsibleDirectiveNode),
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

/// One background panel node spanning a contiguous run of following child nodes.
/// 跨越一段连续后续子节点的单个背景面板节点。
#[derive(Debug, Clone, Copy)]
struct RenderedPanelNode {
    /// Number of immediately following transcript nodes covered by the panel.
    /// 当前面板所覆盖的紧随其后的转录节点数量。
    child_node_count: usize,
    /// Background treatment drawn behind the covered child nodes.
    /// 绘制在被覆盖子节点背后的背景样式。
    background: RenderedParagraphBackground,
}

/// One compact collapsible semantic block rendered inside the transcript flow.
/// 渲染在转录流中的单个紧凑折叠语义区块。
#[derive(Debug, Clone)]
struct RenderedCollapsibleDirectiveNode {
    /// Stable block identifier used for collapse-state toggling and per-block body scroll persistence.
    /// 用于折叠状态切换与区块级正文滚动持久化的稳定区块标识。
    block_id: String,
    /// Summary paragraph rendered as the always-visible clickable row.
    /// 作为始终可见点击行渲染的摘要段落。
    summary: RenderedParagraphNode,
    /// Optional body paragraph rendered inside the expandable viewport.
    /// 渲染在可展开视口内部的可选正文段落。
    body: Option<RenderedParagraphNode>,
    /// Whether the block is currently collapsed.
    /// 当前区块是否处于折叠状态。
    collapsed: bool,
    /// Maximum visible line count before the body viewport starts scrolling.
    /// 在正文视口开始滚动前允许显示的最大行数。
    max_visible_lines: usize,
    /// Background treatment used by the expandable body viewport.
    /// 可展开正文视口使用的背景样式。
    body_background: RenderedParagraphBackground,
    /// Scrollbar track color used when the body overflows.
    /// 正文溢出时滚动条轨道使用的颜色。
    scrollbar_track_color: Color,
    /// Scrollbar thumb color used when the body overflows.
    /// 正文溢出时滚动条滑块使用的颜色。
    scrollbar_thumb_color: Color,
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
    /// Horizontal placement used by the paragraph container inside the transcript viewport.
    /// 当前段落容器在转录视口内部使用的水平放置方式。
    horizontal_placement: RenderedParagraphHorizontalPlacement,
    /// Top gap placed before the paragraph in the flow.
    /// 当前段落在流中之前放置的顶部间距。
    gap_before: f32,
    /// Bottom gap placed after the paragraph in the flow.
    /// 当前段落在流中之后放置的底部间距。
    gap_after: f32,
    /// Inner padding applied around the paragraph text when its background is drawn.
    /// 绘制背景时围绕段落文本应用的内边距。
    inner_padding: Padding,
    /// Whether the paragraph background box should shrink to its content width instead of filling the whole flow width.
    /// 当前段落背景框是否应收缩到内容宽度，而不是填满整条流宽。
    fit_width_to_content: bool,
    /// Optional width cap ratio applied before the paragraph is measured and aligned.
    /// 在段落测量和对齐之前应用的可选宽度上限比例。
    max_width_ratio: Option<f32>,
    /// Optional background treatment used to preserve block-level visuals.
    /// 用于保留块级视觉的可选背景样式。
    background: Option<RenderedParagraphBackground>,
    /// Optional synthetic action link triggered when the paragraph background box itself is clicked.
    /// 当点击当前段落背景框本身时触发的可选合成动作链接。
    background_action_link: Option<String>,
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
    /// Outer horizontal padding applied to the table inside one semantic surface.
    /// 应用于单个语义表面内部表格的外层水平留白。
    outer_padding: Padding,
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

/// Horizontal placement used by one rendered paragraph box inside the transcript viewport.
/// 单个渲染段落框在转录视口内部使用的水平放置方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedParagraphHorizontalPlacement {
    /// Places the paragraph box along the leading edge of the transcript flow.
    /// 将段落框放置在转录流的起始侧。
    Leading,
    /// Places the paragraph box along the trailing edge of the transcript flow.
    /// 将段落框放置在转录流的末端侧。
    Trailing,
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
    /// Optional shared background treatment propagated by semantic blocks such as the inline thinking panel body.
    /// 由内联思考面板正文等语义块向下传播的可选共享背景样式。
    block_background: Option<RenderedParagraphBackground>,
    /// Optional shared surface background propagated for semantic containers that need one continuous panel behind multiple child nodes.
    /// 为需要在多个子节点背后保持连续面板的语义容器向下传播的可选共享表面背景。
    surface_background: Option<RenderedParagraphBackground>,
    /// Optional shared foreground color propagated by semantic blocks such as the inline thinking panel body.
    /// 由内联思考面板正文等语义块向下传播的可选共享前景色。
    foreground_color: Option<Color>,
    /// Shared inner padding propagated to descendant paragraphs so one semantic surface can keep stable content insets.
    /// 向后代段落传播的共享内边距，使单一语义表面可以保持稳定的内容留白。
    inner_padding: Padding,
    /// Whether nested Markdown affordances should avoid drawing their own local backgrounds inside the current semantic surface.
    /// 当前语义表面内部的嵌套 Markdown 呈现是否应避免绘制自身局部背景。
    suppress_embedded_backgrounds: bool,
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
    /// Horizontal placement used by the plain paragraph container.
    /// 当前纯文本段落容器使用的水平放置方式。
    horizontal_placement: RenderedParagraphHorizontalPlacement,
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
    /// Whether the plain paragraph background box should shrink to its content width.
    /// 当前纯文本段落背景框是否应收缩到其内容宽度。
    fit_width_to_content: bool,
    /// Optional width cap ratio applied to the plain paragraph before wrapping and alignment.
    /// 在换行和对齐前应用到当前纯文本段落上的可选宽度上限比例。
    max_width_ratio: Option<f32>,
    /// Optional background treatment for the plain paragraph.
    /// 当前纯文本段落的可选背景样式。
    background: Option<RenderedParagraphBackground>,
    /// Optional shared surface background used to derive nested semantic treatments without painting per-paragraph stripes.
    /// 用于派生嵌套语义样式而不绘制逐段条带背景的可选共享表面背景。
    surface_background: Option<RenderedParagraphBackground>,
    /// Explicit inner padding applied to the paragraph box even when it does not paint a local background.
    /// 即使当前段落不绘制局部背景，也会应用到段落盒上的显式内边距。
    inner_padding: Padding,
    /// Whether nested Markdown affordances should avoid drawing their own local backgrounds inside the current semantic surface.
    /// 当前语义表面内部的嵌套 Markdown 呈现是否应避免绘制自身局部背景。
    suppress_embedded_backgrounds: bool,
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
    /// Foreground color used by table cell content.
    /// 表格单元格内容使用的前景色。
    text_color: Color,
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
    /// Outer horizontal padding applied to the table inside one semantic surface.
    /// 应用于单个语义表面内部表格的外层水平留白。
    outer_padding: Padding,
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
            horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
            gap_before: 0.0,
            gap_after: 10.0,
            color: None,
            render_kind: PlainTextRenderKind::Flow,
            fit_width_to_content: false,
            max_width_ratio: None,
            background: None,
            surface_background: None,
            inner_padding: Padding::ZERO,
            suppress_embedded_backgrounds: false,
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
    /// Link-scoped transcript actions registered by synthetic interactive chrome spans.
    /// 由合成交互外框片段注册的链接级转录动作集合。
    link_actions: BTreeMap<String, RenderedTranscriptLinkAction>,
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
            link_actions: BTreeMap::new(),
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
    fn push_rich_paragraph_with_layout(
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
    fn build_rich_paragraph_node_with_layout(
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
    ) -> RenderedParagraphNode {
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

        RenderedParagraphNode {
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
    fn register_link_action(
        &mut self,
        link_target: impl Into<String>,
        action: RenderedTranscriptLinkAction,
    ) {
        self.link_actions.insert(link_target.into(), action);
    }

    /// Returns the number of currently collected render nodes.
    /// 返回当前已收集的渲染节点数量。
    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Inserts one non-selectable background panel before the provided child-node start index.
    /// 在给定子节点起始索引之前插入一个不可选择背景面板。
    fn insert_panel(
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
            RenderedTranscriptNode::Panel(RenderedPanelNode {
                child_node_count,
                background,
            }),
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
                        style.text_color,
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

/// Measured bounds resolved for one background panel node.
/// 单个背景面板节点解析得到的测量边界。
#[derive(Debug, Clone, Copy)]
struct MeasuredRenderedPanelNode {
    /// Outer bounds occupied by the panel background.
    /// 当前面板背景占据的外部边界。
    outer_bounds: Rectangle,
}

/// Measured bounds resolved for one compact collapsible semantic block node.
/// 为一个紧凑折叠语义区块节点解析出的测量边界。
#[derive(Debug, Clone, Copy)]
struct MeasuredRenderedCollapsibleDirectiveNode {
    /// Outer bounds occupied by the summary row.
    /// 摘要行占据的外部边界。
    summary_outer_bounds: Rectangle,
    /// Optional outer bounds occupied by the expandable body viewport.
    /// 可展开正文视口占据的可选外部边界。
    body_outer_bounds: Option<Rectangle>,
    /// Full unclipped text height of the body paragraph before viewport clipping is applied.
    /// 在应用视口裁剪前正文段落的完整未裁剪文本高度。
    body_full_text_height: f32,
}

/// Returns the number of renderer paragraph slots needed by the whole rendered transcript document.
/// 返回整份渲染转录文档需要的渲染器段落槽位数量。
fn count_rendered_paragraph_slots(document: &RenderedTranscriptDocument) -> usize {
    document
        .nodes
        .iter()
        .map(|node| match node {
            RenderedTranscriptNode::Panel(_) => 0,
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                1 + usize::from(collapsible_node.body.is_some())
            }
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
    alignment: text::Alignment,
) -> RendererParagraph {
    RendererParagraph::with_spans(text::Text {
        content: paragraph_node.spans.as_slice(),
        bounds: text_bounds,
        size: paragraph_node.text_size,
        line_height: paragraph_node.line_height,
        font: theme::ui_font(),
        align_x: alignment,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: paragraph_node.wrapping,
    })
}

/// Returns the height cap used by the visible body viewport of one collapsible semantic block.
/// 返回某个折叠语义区块正文视口使用的可见高度上限。
fn collapsible_body_visible_height_cap(
    body_node: &RenderedParagraphNode,
    max_visible_lines: usize,
) -> f32 {
    let line_height = match body_node.line_height {
        text::LineHeight::Relative(value) => body_node.text_size.0 * value,
        text::LineHeight::Absolute(value) => value.0,
    };

    body_node.inner_padding.top
        + (line_height * max_visible_lines.max(1) as f32)
        + body_node.inner_padding.bottom
}

/// Measures one compact collapsible semantic block node using the provided cached renderer paragraphs.
/// 使用给定缓存渲染段落测量单个紧凑折叠语义区块节点。
fn measure_rendered_collapsible_directive_node(
    collapsible_node: &RenderedCollapsibleDirectiveNode,
    summary_paragraph: &RendererParagraph,
    body_paragraph: Option<&RendererParagraph>,
    widget_bounds: Rectangle,
    current_y: f32,
) -> MeasuredRenderedCollapsibleDirectiveNode {
    let summary_outer_bounds = resolve_paragraph_outer_bounds(
        widget_bounds,
        current_y + collapsible_node.summary.gap_before,
        &collapsible_node.summary,
        summary_paragraph,
    );

    let (body_outer_bounds, body_full_text_height) = match (&collapsible_node.body, body_paragraph)
    {
        (Some(body_node), Some(body_paragraph)) if !collapsible_node.collapsed => {
            let full_body_height = body_paragraph.min_bounds().height
                + body_node.inner_padding.top
                + body_node.inner_padding.bottom;
            let visible_body_height = full_body_height
                .min(collapsible_body_visible_height_cap(
                    body_node,
                    collapsible_node.max_visible_lines,
                ))
                .max(1.0);
            let body_y =
                summary_outer_bounds.y + summary_outer_bounds.height + body_node.gap_before;
            let body_width = (widget_bounds.width - body_node.indent_left).max(1.0);

            (
                Some(Rectangle::new(
                    Point::new(widget_bounds.x + body_node.indent_left, body_y),
                    Size::new(body_width, visible_body_height),
                )),
                full_body_height.max(1.0),
            )
        }
        _ => (None, 0.0),
    };

    MeasuredRenderedCollapsibleDirectiveNode {
        summary_outer_bounds,
        body_outer_bounds,
        body_full_text_height,
    }
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
    let table_available_width = (available_width
        - table_node.indent_left
        - table_node.outer_padding.left
        - table_node.outer_padding.right)
        .max(1.0);
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
            table_cell_text_alignment(cell_node.alignment),
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

/// Returns the text alignment used by one rendered table cell based on the parsed Markdown alignment marker.
/// 根据解析得到的 Markdown 对齐标记返回某个渲染表格单元格使用的文本对齐方式。
fn table_cell_text_alignment(alignment: SelectableTableAlignment) -> text::Alignment {
    match alignment {
        SelectableTableAlignment::Left => text::Alignment::Default,
        SelectableTableAlignment::Center => text::Alignment::Center,
        SelectableTableAlignment::Right => text::Alignment::Right,
    }
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
    let outer_x = widget_bounds.x + table_node.indent_left + table_node.outer_padding.left;
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

/// Measures one background panel node against the contiguous run of child nodes it covers.
/// 根据当前背景面板所覆盖的连续子节点序列测量其外部边界。
fn measure_rendered_panel_node(
    document: &RenderedTranscriptDocument,
    panel_node: &RenderedPanelNode,
    paragraphs: &[RendererParagraph],
    table_layouts: &[CachedTableLayout],
    widget_bounds: Rectangle,
    start_node_index: usize,
    current_y: f32,
    paragraph_index: usize,
    table_index: usize,
) -> Option<MeasuredRenderedPanelNode> {
    let mut local_y = current_y;
    let mut local_paragraph_index = paragraph_index;
    let mut local_table_index = table_index;
    let mut first_y = None::<f32>;
    let mut last_y = None::<f32>;

    for child_node in document
        .nodes
        .iter()
        .skip(start_node_index)
        .take(panel_node.child_node_count)
    {
        match child_node {
            RenderedTranscriptNode::Panel(_) => {}
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                let summary_paragraph = &paragraphs[local_paragraph_index];
                let body_paragraph = collapsible_node
                    .body
                    .as_ref()
                    .map(|_| &paragraphs[local_paragraph_index + 1]);
                let measured_collapsible = measure_rendered_collapsible_directive_node(
                    collapsible_node,
                    summary_paragraph,
                    body_paragraph,
                    widget_bounds,
                    local_y,
                );
                first_y.get_or_insert(measured_collapsible.summary_outer_bounds.y);
                let mut node_bottom = measured_collapsible.summary_outer_bounds.y
                    + measured_collapsible.summary_outer_bounds.height
                    + collapsible_node.summary.gap_after;
                local_y = node_bottom;
                local_paragraph_index += 1;

                if let Some(body_node) = collapsible_node.body.as_ref() {
                    if let Some(body_outer_bounds) = measured_collapsible.body_outer_bounds {
                        node_bottom =
                            body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after;
                        local_y = node_bottom;
                    }
                    local_paragraph_index += 1;
                }

                last_y = Some(node_bottom);
            }
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                local_y += paragraph_node.gap_before;
                let paragraph = &paragraphs[local_paragraph_index];
                let outer_bounds = resolve_paragraph_outer_bounds(
                    widget_bounds,
                    local_y,
                    paragraph_node,
                    paragraph,
                );
                first_y.get_or_insert(outer_bounds.y);
                last_y = Some(outer_bounds.y + outer_bounds.height + paragraph_node.gap_after);
                local_y += outer_bounds.height + paragraph_node.gap_after;
                local_paragraph_index += 1;
            }
            RenderedTranscriptNode::Table(table_node) => {
                let table_layout = &table_layouts[local_table_index];
                let table_paragraphs = &paragraphs[table_layout.paragraph_range.clone()];
                let measured_table = measure_rendered_table_node(
                    table_node,
                    table_layout,
                    table_paragraphs,
                    widget_bounds,
                    local_y,
                );
                first_y.get_or_insert(measured_table.outer_bounds.y);
                last_y = Some(
                    measured_table.outer_bounds.y
                        + measured_table.outer_bounds.height
                        + table_node.gap_after,
                );
                local_y += measured_table.outer_bounds.height + table_node.gap_after;
                local_paragraph_index = table_layout.paragraph_range.end;
                local_table_index += 1;
            }
            RenderedTranscriptNode::Rule(rule_node) => {
                local_y += rule_node.gap_before;
                first_y.get_or_insert(local_y);
                last_y = Some(local_y + 8.0 + rule_node.gap_after);
                local_y += 8.0 + rule_node.gap_after;
            }
        }
    }

    let (Some(panel_top), Some(panel_bottom)) = (first_y, last_y) else {
        return None;
    };

    Some(MeasuredRenderedPanelNode {
        outer_bounds: Rectangle::new(
            Point::new(widget_bounds.x, panel_top),
            Size::new(widget_bounds.width, (panel_bottom - panel_top).max(1.0)),
        ),
    })
}

/// Draws one rendered table node, including cell fills, grid lines, selections, and text.
/// 绘制一个渲染表格节点，包含单元格填充、网格线、选择高亮和文本。
fn draw_rendered_table_node(
    renderer: &mut Renderer,
    document: &RenderedTranscriptDocument,
    defaults: &renderer::Style,
    selection: Option<(usize, usize)>,
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

        if let Some((selection_start, selection_end)) = selection {
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

    for (cell_index, cell_bounds) in measured_table.cell_bounds.iter().enumerate() {
        let Some(cell_node) = table_node.cells.get(cell_index) else {
            continue;
        };

        if cell_node.column_index > 0 {
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
        }

        if cell_node.row_index > 0 {
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

impl<'a, Message> Widget<Message, Theme, Renderer> for SelectableTranscript<'a, Message> {
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
        let measurement_bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(available_width, max_size.height.max(1.0)),
        );

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

        let mut node_index = 0usize;
        while node_index < self.document.nodes.len() {
            match &self.document.nodes[node_index] {
                RenderedTranscriptNode::Panel(_) => {}
                RenderedTranscriptNode::Collapsible(collapsible_node) => {
                    state.paragraphs[paragraph_index] = build_renderer_paragraph(
                        &collapsible_node.summary,
                        Size::new(
                            paragraph_measurement_width(
                                measurement_bounds,
                                &collapsible_node.summary,
                            ),
                            f32::INFINITY,
                        ),
                        text::Alignment::Default,
                    );
                    let summary_height = collapsible_node.summary.gap_before
                        + collapsible_node.summary.inner_padding.top
                        + state.paragraphs[paragraph_index].min_bounds().height
                        + collapsible_node.summary.inner_padding.bottom
                        + collapsible_node.summary.gap_after;
                    paragraph_index += 1;
                    total_height += summary_height;

                    if let Some(body_node) = collapsible_node.body.as_ref() {
                        state.paragraphs[paragraph_index] = build_renderer_paragraph(
                            body_node,
                            Size::new(
                                paragraph_measurement_width(measurement_bounds, body_node),
                                f32::INFINITY,
                            ),
                            text::Alignment::Default,
                        );

                        if !collapsible_node.collapsed {
                            let full_body_height =
                                state.paragraphs[paragraph_index].min_bounds().height
                                    + body_node.inner_padding.top
                                    + body_node.inner_padding.bottom;
                            total_height += body_node.gap_before
                                + full_body_height.min(collapsible_body_visible_height_cap(
                                    body_node,
                                    collapsible_node.max_visible_lines,
                                ))
                                + body_node.gap_after;
                        }

                        paragraph_index += 1;
                    }
                }
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    let text_bounds = Size::new(
                        paragraph_measurement_width(measurement_bounds, paragraph_node),
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
            node_index += 1;
        }

        layout::Node::new(Size::new(available_width, total_height.max(1.0)))
    }

    /// Draws the transcript widget, its block backgrounds, quote bars, text, and active selection highlight.
    /// 绘制转录组件及其区块背景、引用导线、文本和当前活动选择高亮。
    fn draw(
        &self,
        tree: &tree::Tree,
        renderer: &mut Renderer,
        app_theme: &Theme,
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
        let mut panel_border_overlays = Vec::new();

        let mut node_index = 0usize;
        while node_index < self.document.nodes.len() {
            match &self.document.nodes[node_index] {
                RenderedTranscriptNode::Panel(panel_node) => {
                    if let Some(measured_panel) = measure_rendered_panel_node(
                        &self.document,
                        panel_node,
                        &state.paragraphs,
                        &state.table_layouts,
                        layout.bounds(),
                        node_index + 1,
                        current_y,
                        paragraph_index,
                        table_index,
                    ) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: measured_panel.outer_bounds,
                                border: Border {
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                    radius: panel_node.background.border.radius,
                                },
                                ..renderer::Quad::default()
                            },
                            panel_node.background.background,
                        );
                        if panel_node.background.border.width > f32::EPSILON {
                            panel_border_overlays
                                .push((measured_panel.outer_bounds, panel_node.background.border));
                        }
                    }
                }
                RenderedTranscriptNode::Collapsible(collapsible_node) => {
                    let summary_paragraph = &state.paragraphs[paragraph_index];
                    let body_paragraph = collapsible_node
                        .body
                        .as_ref()
                        .map(|_| &state.paragraphs[paragraph_index + 1]);
                    let measured_collapsible = measure_rendered_collapsible_directive_node(
                        collapsible_node,
                        summary_paragraph,
                        body_paragraph,
                        layout.bounds(),
                        current_y,
                    );

                    if let Some(background) = collapsible_node.summary.background {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: measured_collapsible.summary_outer_bounds,
                                border: background.border,
                                ..renderer::Quad::default()
                            },
                            background.background,
                        );
                    }

                    renderer.fill_paragraph(
                        summary_paragraph,
                        paragraph_local_origin(
                            measured_collapsible.summary_outer_bounds,
                            &collapsible_node.summary,
                        ),
                        collapsible_node
                            .summary
                            .spans
                            .first()
                            .and_then(|span| span.color)
                            .unwrap_or(defaults.text_color),
                        *viewport,
                    );

                    current_y = measured_collapsible.summary_outer_bounds.y
                        + measured_collapsible.summary_outer_bounds.height
                        + collapsible_node.summary.gap_after;
                    paragraph_index += 1;

                    if let (Some(body_node), Some(body_paragraph)) =
                        (collapsible_node.body.as_ref(), body_paragraph)
                    {
                        if let Some(body_outer_bounds) = measured_collapsible.body_outer_bounds {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: body_outer_bounds,
                                    border: collapsible_node.body_background.border,
                                    ..renderer::Quad::default()
                                },
                                collapsible_node.body_background.background,
                            );

                            let max_scroll = (measured_collapsible.body_full_text_height
                                - body_outer_bounds.height)
                                .max(0.0);
                            let scroll_offset = state
                                .collapsible_body_scroll_offsets
                                .get(&collapsible_node.block_id)
                                .copied()
                                .unwrap_or(0.0)
                                .clamp(0.0, max_scroll);
                            let text_clip_bounds = Rectangle::new(
                                Point::new(
                                    body_outer_bounds.x + body_node.inner_padding.left,
                                    body_outer_bounds.y + body_node.inner_padding.top,
                                ),
                                Size::new(
                                    (body_outer_bounds.width
                                        - body_node.inner_padding.left
                                        - body_node.inner_padding.right
                                        - if max_scroll > 0.0 { 10.0 } else { 0.0 })
                                    .max(1.0),
                                    (body_outer_bounds.height
                                        - body_node.inner_padding.top
                                        - body_node.inner_padding.bottom)
                                        .max(1.0),
                                ),
                            );

                            renderer.with_layer(text_clip_bounds, |renderer| {
                                renderer.fill_paragraph(
                                    body_paragraph,
                                    Point::new(
                                        text_clip_bounds.x,
                                        text_clip_bounds.y - scroll_offset,
                                    ),
                                    body_node
                                        .spans
                                        .first()
                                        .and_then(|span| span.color)
                                        .unwrap_or(defaults.text_color),
                                    *viewport,
                                );
                            });

                            if max_scroll > 0.0 {
                                let track_bounds = Rectangle::new(
                                    Point::new(
                                        body_outer_bounds.x + body_outer_bounds.width - 6.0,
                                        body_outer_bounds.y + 6.0,
                                    ),
                                    Size::new(3.0, (body_outer_bounds.height - 12.0).max(18.0)),
                                );
                                renderer.fill_quad(
                                    renderer::Quad {
                                        bounds: track_bounds,
                                        border: Border {
                                            color: Color::TRANSPARENT,
                                            width: 0.0,
                                            radius: 2.0.into(),
                                        },
                                        ..renderer::Quad::default()
                                    },
                                    Background::Color(collapsible_node.scrollbar_track_color),
                                );

                                let thumb_height = ((track_bounds.height
                                    * (body_outer_bounds.height
                                        / measured_collapsible.body_full_text_height))
                                    .clamp(18.0, track_bounds.height))
                                .max(18.0);
                                let travel = (track_bounds.height - thumb_height).max(0.0);
                                let thumb_y = track_bounds.y
                                    + if max_scroll <= f32::EPSILON {
                                        0.0
                                    } else {
                                        (scroll_offset / max_scroll) * travel
                                    };
                                renderer.fill_quad(
                                    renderer::Quad {
                                        bounds: Rectangle::new(
                                            Point::new(track_bounds.x, thumb_y),
                                            Size::new(track_bounds.width, thumb_height),
                                        ),
                                        border: Border {
                                            color: Color::TRANSPARENT,
                                            width: 0.0,
                                            radius: 2.0.into(),
                                        },
                                        ..renderer::Quad::default()
                                    },
                                    Background::Color(collapsible_node.scrollbar_thumb_color),
                                );
                            }

                            current_y = body_outer_bounds.y
                                + body_outer_bounds.height
                                + body_node.gap_after;
                        }

                        paragraph_index += 1;
                    }
                }
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    current_y += paragraph_node.gap_before;
                    let paragraph = &state.paragraphs[paragraph_index];
                    let outer_bounds = resolve_paragraph_outer_bounds(
                        layout.bounds(),
                        current_y,
                        paragraph_node,
                        paragraph,
                    );

                    if let Some(background) = paragraph_node.background {
                        let background_bounds = if background.border.width <= f32::EPSILON {
                            Rectangle::new(
                                outer_bounds.position(),
                                Size::new(
                                    outer_bounds.width,
                                    outer_bounds.height + paragraph_node.gap_after,
                                ),
                            )
                        } else {
                            outer_bounds
                        };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: background_bounds,
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

                    if let Some((selection_start, selection_end)) = effective_selection(
                        state,
                        &self.document,
                        layout.bounds(),
                        cursor.position(),
                    ) {
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
                        effective_selection(
                            state,
                            &self.document,
                            layout.bounds(),
                            cursor.position(),
                        ),
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
            node_index += 1;
        }

        for (bounds, border) in panel_border_overlays {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border,
                    ..renderer::Quad::default()
                },
                Background::Color(Color::TRANSPARENT),
            );
        }

        let _ = app_theme;
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
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                state.context_menu = None;
                shell.request_redraw();
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    let delta_y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y * 36.0,
                        mouse::ScrollDelta::Pixels { y, .. } => -y,
                    };

                    if scroll_collapsible_body_at_position(
                        &self.document,
                        state,
                        layout.bounds(),
                        position,
                        delta_y,
                    ) {
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.context_menu = None;
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    if let Some(hit_char) =
                        hit_test_rendered_document(&self.document, state, layout.bounds(), position)
                    {
                        state.focused = true;
                        state.dragging = true;
                        state.drag_moved = false;
                        state.anchor_char = hit_char;
                        state.head_char = hit_char;
                        shell.request_redraw();
                        self.publish_drag_auto_scroll_request(state, None, shell);
                        shell.capture_event();
                    } else {
                        state.focused = false;
                        shell.request_redraw();
                        self.publish_drag_auto_scroll_request(state, None, shell);
                    }
                } else {
                    state.focused = false;
                    shell.request_redraw();
                    self.publish_drag_auto_scroll_request(state, None, shell);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    state.focused = true;
                    state.context_menu = Some(TranscriptContextMenuState {
                        anchor_position: position,
                    });
                    shell.request_redraw();
                    self.publish_drag_auto_scroll_request(state, None, shell);
                    shell.capture_event();
                } else {
                    state.context_menu = None;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(position) = cursor.position() {
                    if let Some(hit_char) =
                        drag_selection_head_char(&self.document, state, layout.bounds(), position)
                    {
                        if hit_char != state.head_char {
                            state.drag_moved = true;
                        }
                        state.head_char = hit_char;
                        shell.request_redraw();
                        shell.capture_event();
                    }

                    self.publish_drag_auto_scroll_request(
                        state,
                        drag_auto_scroll_request_for_position(layout.bounds(), position),
                        shell,
                    );
                } else {
                    self.publish_drag_auto_scroll_request(state, None, shell);
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                if let Some(position) = cursor.position() {
                    if let Some(hit_char) =
                        drag_selection_head_char(&self.document, state, layout.bounds(), position)
                    {
                        if hit_char != state.head_char {
                            state.drag_moved = true;
                        }
                        state.head_char = hit_char;
                        shell.request_redraw();
                    }
                }
                if !state.drag_moved && state.anchor_char == state.head_char {
                    if let Some(position) = cursor.position_over(layout.bounds()) {
                        let clicked_link_target = hit_test_rendered_document(
                            &self.document,
                            state,
                            layout.bounds(),
                            position,
                        )
                        .and_then(|hit_char| self.document.link_at_char_offset(hit_char))
                        .or_else(|| {
                            background_action_link_at_position(
                                &self.document,
                                state,
                                layout.bounds(),
                                position,
                            )
                        });

                        if let Some(link_url) = clicked_link_target {
                            if let Some(link_action) =
                                self.document.link_action_for_target(&link_url)
                            {
                                match link_action {
                                    RenderedTranscriptLinkAction::ToggleBlockInline(block_id) => {
                                        shell.publish((self.on_interaction.as_ref())(
                                            SelectableTranscriptInteraction::ToggleBlockInline(
                                                block_id.clone(),
                                            ),
                                        ));
                                        state.last_click_link_target = Some(link_url);
                                        state.last_click_instant = Some(Instant::now());
                                    }
                                    RenderedTranscriptLinkAction::CopyText(copy_payload) => {
                                        clipboard
                                            .write(ClipboardKind::Standard, copy_payload.clone());
                                    }
                                    RenderedTranscriptLinkAction::OpenThinkingModal(block_id) => {
                                        shell.publish((self.on_interaction.as_ref())(
                                            SelectableTranscriptInteraction::OpenThinkingModal(
                                                block_id.clone(),
                                            ),
                                        ));
                                        state.last_click_link_target = Some(link_url);
                                        state.last_click_instant = Some(Instant::now());
                                    }
                                    RenderedTranscriptLinkAction::OpenToolModal(block_id) => {
                                        shell.publish((self.on_interaction.as_ref())(
                                            SelectableTranscriptInteraction::OpenToolModal(
                                                block_id.clone(),
                                            ),
                                        ));
                                        state.last_click_link_target = Some(link_url);
                                        state.last_click_instant = Some(Instant::now());
                                    }
                                }
                            } else {
                                clipboard.write(ClipboardKind::Standard, link_url);
                            }
                        }
                    }
                }
                state.dragging = false;
                state.drag_moved = false;
                shell.request_redraw();
                self.publish_drag_auto_scroll_request(state, None, shell);
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if state.focused && shortcut_matches_copy(key, *modifiers, text.as_deref()) => {
                state.context_menu = None;
                if let Some((selection_start, selection_end)) =
                    effective_selection(state, &self.document, layout.bounds(), cursor.position())
                {
                    clipboard.write(
                        ClipboardKind::Standard,
                        self.document.selected_text(selection_start, selection_end),
                    );
                    shell.request_redraw();
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
                state.context_menu = None;
                state.anchor_char = 0;
                state.head_char = self.document.len_chars();
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { .. }) => {
                state.context_menu = None;
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Returns the mouse interaction shown while hovering the selectable transcript widget.
    /// 返回悬停在可选择转录组件上时展示的鼠标交互样式。
    fn mouse_interaction(
        &self,
        tree: &tree::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_over(layout.bounds()) else {
            return mouse::Interaction::None;
        };
        let state = tree.state.downcast_ref::<SelectableTranscriptState>();

        if let Some(menu_state) = state.context_menu {
            let menu_actions =
                transcript_context_menu_actions(current_selection_for_state(state).is_some());
            let menu_bounds =
                transcript_context_menu_bounds(layout.bounds(), menu_state, menu_actions.len());
            if menu_bounds.contains(position) {
                return mouse::Interaction::Pointer;
            }
        }

        if hit_test_rendered_document(&self.document, state, layout.bounds(), position)
            .and_then(|hit_char| self.document.link_at_char_offset(hit_char))
            .or_else(|| {
                background_action_link_at_position(&self.document, state, layout.bounds(), position)
            })
            .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::Text
        }
    }

    /// Returns the transcript context-menu overlay when one right-click menu is currently open.
    /// 当存在一个已打开右键菜单时返回其转录上下文菜单浮层。
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut tree::Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();
        let menu_state = state.context_menu?;
        let translated_bounds = Rectangle::new(
            Point::new(
                layout.bounds().x + translation.x,
                layout.bounds().y + translation.y,
            ),
            layout.bounds().size(),
        );

        Some(overlay::Element::new(Box::new(
            SelectableTranscriptContextMenuOverlay {
                state,
                document: &self.document,
                widget_bounds: translated_bounds,
                menu_state,
            },
        )))
    }
}

/// Overlay responsible for presenting and executing the selectable transcript context menu.
/// 负责展示并执行可选择转录右键菜单的浮层。
struct SelectableTranscriptContextMenuOverlay<'a> {
    /// Mutable transcript runtime state so menu actions can update selection and dismiss themselves.
    /// 可变转录运行时状态，用于让菜单动作更新选区并自行关闭。
    state: &'a mut SelectableTranscriptState,
    /// Rendered transcript document used to copy selected text and compute the full character span.
    /// 用于复制选中文本并计算全文字符跨度的渲染转录文档。
    document: &'a RenderedTranscriptDocument,
    /// Absolute widget bounds used to clamp the menu overlay inside the transcript surface.
    /// 用于将菜单浮层钳制在转录表面内部的绝对组件边界。
    widget_bounds: Rectangle,
    /// Open menu anchor captured from the right-click gesture.
    /// 从右键手势捕获的已打开菜单锚点。
    menu_state: TranscriptContextMenuState,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer>
    for SelectableTranscriptContextMenuOverlay<'_>
{
    /// Lays out the transcript context menu using one clamped fixed-size floating rectangle.
    /// 使用一个经过钳制的固定尺寸浮动矩形对转录右键菜单进行布局。
    fn layout(&mut self, _renderer: &Renderer, _bounds: Size) -> layout::Node {
        let actions =
            transcript_context_menu_actions(current_selection_for_state(self.state).is_some());
        let menu_bounds =
            transcript_context_menu_bounds(self.widget_bounds, self.menu_state, actions.len());
        layout::Node::new(menu_bounds.size()).move_to(menu_bounds.position())
    }

    /// Draws the floating transcript context menu above the transcript surface.
    /// 在转录表面之上绘制浮动转录右键菜单。
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let menu_bounds = layout.bounds();
        let actions =
            transcript_context_menu_actions(current_selection_for_state(self.state).is_some());
        let palette = theme.extended_palette();
        let hovered_action = cursor.position().and_then(|position| {
            hit_test_transcript_context_menu_action(menu_bounds, actions.as_slice(), position)
        });

        renderer.with_layer(menu_bounds, |renderer| {
            renderer.with_translation(Vector::new(menu_bounds.x, menu_bounds.y), |renderer| {
                let local_menu_bounds = Rectangle::new(Point::ORIGIN, menu_bounds.size());

                renderer.fill_quad(
                    renderer::Quad {
                        bounds: local_menu_bounds,
                        border: Border {
                            color: palette.background.strong.color.scale_alpha(0.28),
                            width: 1.0,
                            radius: TRANSCRIPT_CONTEXT_MENU_RADIUS.into(),
                        },
                        shadow: iced::Shadow {
                            color: Color::BLACK.scale_alpha(0.18),
                            offset: iced::Vector::new(0.0, 8.0),
                            blur_radius: 18.0,
                        },
                        ..renderer::Quad::default()
                    },
                    Background::Color(mix_color(
                        palette.background.base.color,
                        palette.background.weak.color,
                        0.18,
                    )),
                );

                for (index, action) in actions.iter().enumerate() {
                    let item_bounds = transcript_context_menu_item_bounds(local_menu_bounds, index);
                    if hovered_action == Some(*action) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: item_bounds,
                                border: Border {
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                    radius: TRANSCRIPT_CONTEXT_MENU_RADIUS.into(),
                                },
                                ..renderer::Quad::default()
                            },
                            Background::Color(palette.primary.weak.color.scale_alpha(0.24)),
                        );
                    }

                    renderer.fill_text(
                        text::Text {
                            content: transcript_context_menu_action_label(*action).to_string(),
                            bounds: Size::new(
                                (item_bounds.width - TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X * 2.0)
                                    .max(1.0),
                                item_bounds.height.max(1.0),
                            ),
                            size: Pixels(14.0),
                            line_height: text::LineHeight::default(),
                            font: theme::ui_font(),
                            align_x: text::Alignment::Left,
                            align_y: iced::alignment::Vertical::Center,
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                        },
                        Point::new(TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X, item_bounds.y),
                        style.text_color,
                        local_menu_bounds,
                    );
                }
            });
        });
    }

    /// Updates the floating transcript context menu, executing menu actions or dismissing itself on outside clicks.
    /// 更新浮动转录右键菜单，在点击菜单动作或外部区域时执行动作或自行关闭。
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let actions = transcript_context_menu_actions(
                    current_selection_for_state(self.state).is_some(),
                );
                let menu_bounds = layout.bounds();

                if let Some(position) = cursor.position() {
                    if let Some(action) = hit_test_transcript_context_menu_action(
                        menu_bounds,
                        actions.as_slice(),
                        position,
                    ) {
                        execute_transcript_context_menu_action(
                            action,
                            self.state,
                            self.document,
                            clipboard,
                        );
                        self.state.context_menu = None;
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }

                self.state.context_menu = None;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
            | Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                self.state.context_menu = None;
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Returns pointer interaction while hovering the context-menu overlay.
    /// 返回悬停在右键菜单浮层上时的指针交互。
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        cursor
            .position()
            .filter(|position| layout.bounds().contains(*position))
            .map(|_| mouse::Interaction::Pointer)
            .unwrap_or(mouse::Interaction::default())
    }
}

impl<'a, Message> SelectableTranscript<'a, Message> {
    /// Publishes one changed drag auto-scroll request to the parent scroll container while suppressing duplicates.
    /// 在抑制重复消息的前提下向父级滚动容器发布一次变更后的拖拽自动滚动请求。
    fn publish_drag_auto_scroll_request(
        &self,
        state: &mut SelectableTranscriptState,
        request: Option<SelectableTranscriptDragAutoScroll>,
        shell: &mut Shell<'_, Message>,
    ) {
        if state.drag_auto_scroll_request == request {
            return;
        }

        state.drag_auto_scroll_request = request;
        shell.publish((self.on_drag_auto_scroll.as_ref())(request));
    }
}

/// Renders one directive block using the transcript builder and semantic block treatments.
/// 使用转录构建器和语义块样式渲染一个指令区块。
fn render_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    if block.kind == AgentViewDirectiveKind::UserPrompt {
        render_user_prompt_block(builder, block, markdown_settings, app_theme);
        return;
    }

    if block.kind == AgentViewDirectiveKind::Thinking {
        render_thinking_block(builder, block, markdown_settings, app_theme);
        return;
    }

    if matches!(
        block.kind,
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
    ) {
        render_compact_tool_modal_directive_block(builder, block, markdown_settings, app_theme);
        return;
    }

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
            horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
            fit_width_to_content: false,
            max_width_ratio: None,
            background: Some(title_background),
            surface_background: None,
            inner_padding: Padding::ZERO,
            suppress_embedded_backgrounds: false,
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

/// Renders one compact tool row that opens a dedicated modal instead of expanding inline.
/// 渲染一条会打开专用弹窗而不是在正文中内联展开的紧凑工具行。
fn render_compact_tool_modal_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let block_id = block
        .attributes
        .get("ui-block-id")
        .cloned()
        .unwrap_or_else(|| block.selectable_title());
    let action_link_target = format!("{TOOL_MODAL_ACTION_PREFIX}{block_id}");
    builder.register_link_action(
        action_link_target.clone(),
        RenderedTranscriptLinkAction::OpenToolModal(block_id),
    );

    let (icon, summary_color, fallback_title) = match block.kind {
        AgentViewDirectiveKind::ToolCall => ("🛠", iced::color!(0xA8D5FF), String::from("调用工具")),
        AgentViewDirectiveKind::ToolResult => {
            ("📦", iced::color!(0xAEE8C8), String::from("工具返回"))
        }
        _ => return,
    };
    let title = block
        .attributes
        .get("title")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_title);
    let summary_text = format!("{icon} {title}");
    let summary_spans = build_agent_view_plain_text_spans(
        &summary_text,
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(summary_color),
        app_theme,
    );
    let mut summary_node = builder.build_rich_paragraph_node_with_layout(
        summary_spans,
        markdown_settings.text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        0.0,
        0.0,
        Padding::ZERO,
        None,
        0,
        RenderedParagraphHorizontalPlacement::Leading,
        false,
        None,
    );
    summary_node.background_action_link = Some(action_link_target);
    builder
        .nodes
        .push(RenderedTranscriptNode::Paragraph(summary_node));
    builder.pending_separator = Some("\n");
}

#[allow(dead_code)]
/// Renders one compact collapsible directive block used by tool-call, tool-result, and inline thinking rows.
/// 渲染工具调用、工具结果以及内联思考行共用的紧凑折叠指令区块。
fn render_compact_collapsible_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let palette = app_theme.extended_palette();
    let (icon, summary_color, body_background, body_text_color) = match block.kind {
        AgentViewDirectiveKind::Thinking => (
            "🧠",
            iced::color!(0xD8E8FF),
            directive_background(
                iced::color!(0x151E2A),
                iced::color!(0x46566B).scale_alpha(0.70),
            ),
            iced::color!(0xEAF1FA),
        ),
        AgentViewDirectiveKind::ToolCall => (
            "🛠",
            iced::color!(0xCDE7FF),
            directive_background(
                palette.primary.weak.color.scale_alpha(0.08),
                palette.primary.strong.color.scale_alpha(0.24),
            ),
            palette.background.base.text,
        ),
        AgentViewDirectiveKind::ToolResult => (
            "📦",
            iced::color!(0xD7F6E6),
            directive_background(
                palette.success.weak.color.scale_alpha(0.08),
                palette.success.strong.color.scale_alpha(0.24),
            ),
            palette.background.base.text,
        ),
        _ => return,
    };
    let block_id = block
        .attributes
        .get("ui-block-id")
        .cloned()
        .unwrap_or_else(|| block.selectable_title());
    let is_collapsed = block
        .attributes
        .get("ui-inline-collapsed")
        .map(|value| value == "true")
        .or_else(|| {
            block
                .attributes
                .get("collapsed")
                .map(|value| value == "true")
        })
        .unwrap_or(block.collapsed_by_default);
    let toggle_link_target = if block.kind == AgentViewDirectiveKind::Thinking {
        thinking_modal_action_link(block)
    } else {
        Some(format!("{BLOCK_TOGGLE_ACTION_PREFIX}{block_id}"))
    };
    if let Some(toggle_link_target) = toggle_link_target.as_ref() {
        builder.register_link_action(
            toggle_link_target.clone(),
            if block.kind == AgentViewDirectiveKind::Thinking {
                RenderedTranscriptLinkAction::OpenThinkingModal(block_id.clone())
            } else {
                RenderedTranscriptLinkAction::ToggleBlockInline(block_id.clone())
            },
        );
    }

    let title_suffix = block
        .attributes
        .get("title")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| match block.kind {
            AgentViewDirectiveKind::Thinking => "思考过程",
            AgentViewDirectiveKind::ToolCall => "调用工具",
            AgentViewDirectiveKind::ToolResult => "工具返回",
            _ => "",
        });
    let toggle_glyph = if block.kind == AgentViewDirectiveKind::Thinking {
        ">"
    } else if is_collapsed {
        ">"
    } else {
        "v"
    };
    let summary_text = format!("{icon} {title_suffix} {toggle_glyph}");
    let summary_spans = build_agent_view_plain_text_spans(
        &summary_text,
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(summary_color),
        app_theme,
    );
    let mut summary_node = builder.build_rich_paragraph_node_with_layout(
        summary_spans,
        markdown_settings.text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        0.0,
        if is_collapsed { 8.0 } else { 4.0 },
        Padding::from([8.0, 0.0]),
        None,
        0,
        RenderedParagraphHorizontalPlacement::Leading,
        false,
        None,
    );
    summary_node.background_action_link = toggle_link_target;

    let body_node = if block.body.trim().is_empty() {
        None
    } else {
        Some(builder.build_rich_paragraph_node_with_layout(
            build_agent_view_plain_text_spans(
                block.body.trim_end(),
                if block.kind == AgentViewDirectiveKind::Thinking {
                    theme::ui_font()
                } else {
                    theme::code_font()
                },
                if block.kind == AgentViewDirectiveKind::Thinking {
                    PlainTextRenderKind::Flow
                } else {
                    PlainTextRenderKind::Code
                },
                Some(body_text_color),
                app_theme,
            ),
            if block.kind == AgentViewDirectiveKind::Thinking {
                markdown_settings.text_size
            } else {
                markdown_settings.code_size
            },
            text::LineHeight::default(),
            text::Wrapping::WordOrGlyph,
            0.0,
            0.0,
            8.0,
            Padding::from([10.0, 12.0]),
            Some(body_background),
            0,
            RenderedParagraphHorizontalPlacement::Leading,
            false,
            None,
        ))
    };

    builder.nodes.push(RenderedTranscriptNode::Collapsible(
        RenderedCollapsibleDirectiveNode {
            block_id,
            summary: summary_node,
            body: body_node,
            collapsed: is_collapsed,
            max_visible_lines: 10,
            body_background,
            scrollbar_track_color: palette.background.strong.color.scale_alpha(0.20),
            scrollbar_thumb_color: palette.background.base.text.scale_alpha(0.35),
        },
    ));
    builder.pending_separator = Some("\n");
}

/// Renders one thinking block with a full-width dark panel treatment; inline rows open the dedicated modal instead of expanding in place.
/// 使用全宽深色面板样式渲染单个思考区块；内联标题行会直接打开专用弹层，而不会在正文中原地展开。
fn render_thinking_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let thinking_surface = block
        .attributes
        .get("ui-thinking-surface")
        .map(String::as_str)
        .unwrap_or("inline");
    let outer_panel_background = RenderedParagraphBackground {
        background: Background::Color(iced::color!(0x1C2230)),
        border: Border {
            color: iced::color!(0x68748A).scale_alpha(0.84),
            width: 1.0,
            radius: 2.0.into(),
        },
    };
    let body_foreground = iced::color!(0xEEF2F7);
    let body_content_padding = Padding::from([0.0, 16.0]);
    let action_link_target = thinking_modal_action_link(block);
    let is_inline_surface = thinking_surface == "inline";
    let render_header = block
        .attributes
        .get("ui-render-thinking-header")
        .map(|value| value != "false")
        .unwrap_or(true);
    let header_padding = Padding::ZERO;

    if let Some(action_link_target) = action_link_target.as_ref() {
        if let Some(block_id) = block.attributes.get("ui-block-id").map(String::as_str) {
            builder.register_link_action(
                action_link_target.clone(),
                RenderedTranscriptLinkAction::OpenThinkingModal(block_id.to_string()),
            );
        }
    }

    if render_header {
        let header_title = block
            .attributes
            .get("ui-thinking-title")
            .cloned()
            .unwrap_or_else(|| String::from("思考过程"));
        let header_spans = build_thinking_header_spans(
            &header_title,
            action_link_target.as_deref(),
            body_foreground,
            app_theme,
        );

        builder.push_rich_paragraph_with_layout(
            header_spans,
            markdown_settings.text_size,
            text::LineHeight::Relative(0.92),
            text::Wrapping::Word,
            0.0,
            0.0,
            0.0,
            header_padding,
            None,
            0,
            RenderedParagraphHorizontalPlacement::Leading,
            false,
            None,
        );
        if let Some(RenderedTranscriptNode::Paragraph(header_node)) = builder.nodes.last_mut() {
            header_node.background_action_link = action_link_target.clone();
        }
    }

    if is_inline_surface {
        return;
    }

    let body_node_start = builder.node_count();
    let prepared_body = prepare_markdown_source_for_rendering(&block.body);
    let paragraph_style = PlainParagraphStyle {
        background: None,
        surface_background: if is_inline_surface {
            Some(outer_panel_background)
        } else {
            None
        },
        inner_padding: body_content_padding,
        color: Some(body_foreground),
        gap_after: 4.0,
        suppress_embedded_backgrounds: true,
        ..PlainParagraphStyle::flow(markdown_settings.text_size)
    };

    if raw_source_contains_markdown_table(&prepared_body) {
        render_markdown_source_with_table_fallback(
            builder,
            &prepared_body,
            paragraph_style,
            markdown_settings,
            app_theme,
        );
    } else {
        let normalized_body = normalize_markdown_source_for_transcript_display(&prepared_body);
        let body_markdown = markdown::Content::parse(&normalized_body);
        render_markdown_items(
            builder,
            body_markdown.items(),
            RenderContext {
                block_background: None,
                surface_background: if is_inline_surface {
                    Some(outer_panel_background)
                } else {
                    None
                },
                foreground_color: Some(body_foreground),
                inner_padding: body_content_padding,
                suppress_embedded_backgrounds: true,
                ..RenderContext::default()
            },
            markdown_settings,
            app_theme,
        );
    }

    if is_inline_surface {
        let body_node_count = builder.node_count().saturating_sub(body_node_start);
        if body_node_count > 0 {
            builder.insert_panel(body_node_start, body_node_count, outer_panel_background);
        }
    }
}

/// Renders one user-prompt directive block as a right-aligned bubble followed by a right-aligned timestamp and copy action row.
/// 将单个用户提示词指令区块渲染为右对齐气泡，并在其下方追加右对齐的时间戳与复制动作行。
fn render_user_prompt_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let palette = app_theme.extended_palette();
    let bubble_fill = mix_color(
        palette.background.strong.color,
        palette.primary.strong.color,
        0.22,
    )
    .scale_alpha(0.98);
    let bubble_border_color = mix_color(
        palette.primary.strong.color,
        palette.background.base.text,
        0.20,
    )
    .scale_alpha(0.84);
    let bubble_text_color = iced::color!(0xF3F8FD);
    let bubble_meta_color = iced::color!(0xAABACE);
    let bubble_action_color = iced::color!(0x84D2FF);
    let bubble_background = directive_background(bubble_fill, bubble_border_color);

    builder.push_rich_paragraph_with_layout(
        build_agent_view_plain_text_spans(
            if block.body.is_empty() {
                " "
            } else {
                &block.body
            },
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(bubble_text_color),
            app_theme,
        ),
        markdown_settings.text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        0.0,
        0.0,
        Padding::from([10.0, 12.0]),
        Some(bubble_background),
        0,
        RenderedParagraphHorizontalPlacement::Trailing,
        true,
        Some(USER_PROMPT_BUBBLE_MAX_WIDTH_RATIO),
    );

    let copy_action_link = user_prompt_copy_action_link(block);
    if let Some(copy_action_link) = copy_action_link.as_ref() {
        builder.register_link_action(
            copy_action_link.clone(),
            RenderedTranscriptLinkAction::CopyText(block.body.clone()),
        );
    }

    let meta_spans = build_user_prompt_meta_spans(
        block,
        copy_action_link.as_deref(),
        bubble_meta_color,
        bubble_action_color,
        app_theme,
    );
    if meta_spans.is_empty() {
        return;
    }

    builder.push_rich_paragraph_with_layout(
        meta_spans,
        Pixels(11.0),
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        4.0,
        10.0,
        Padding::ZERO,
        None,
        0,
        RenderedParagraphHorizontalPlacement::Trailing,
        true,
        None,
    );
}

/// Builds the timestamp and copy-action spans shown under one right-aligned user-prompt bubble.
/// 构建显示在右对齐用户提示词气泡下方的时间戳与复制动作片段。
fn build_user_prompt_meta_spans(
    block: &AgentViewDirectiveBlock,
    copy_action_link: Option<&str>,
    meta_text_color: Color,
    action_text_color: Color,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = Vec::new();

    if let Some(timestamp_label) = user_prompt_created_at_label(block) {
        spans.extend(build_agent_view_plain_text_spans(
            &timestamp_label,
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(meta_text_color),
            app_theme,
        ));
    }

    if let Some(copy_action_link) = copy_action_link {
        if !spans.is_empty() {
            spans.extend(build_agent_view_plain_text_spans(
                "   ",
                theme::ui_font(),
                PlainTextRenderKind::Flow,
                Some(meta_text_color.scale_alpha(0.72)),
                app_theme,
            ));
        }

        let mut copy_spans = build_agent_view_plain_text_spans(
            "⧉ 复制",
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(action_text_color),
            app_theme,
        );
        for span in &mut copy_spans {
            span.link = Some(copy_action_link.into());
        }
        spans.extend(copy_spans);
    }

    spans
}

/// Returns the formatted timestamp label shown below one user-prompt bubble, if the directive carries a valid creation timestamp.
/// 返回显示在用户提示词气泡下方的格式化时间标签；若指令携带了有效创建时间则返回该标签。
fn user_prompt_created_at_label(block: &AgentViewDirectiveBlock) -> Option<String> {
    let created_at_millis = block
        .attributes
        .get("created-at")?
        .trim()
        .parse::<i128>()
        .ok()?;
    let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local_time = OffsetDateTime::from_unix_timestamp_nanos(created_at_millis * 1_000_000)
        .ok()?
        .to_offset(local_offset);

    local_time
        .format(USER_PROMPT_TIMESTAMP_FORMAT)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Returns the synthetic link target used by the user-prompt copy action, if the directive carries a stable message identifier.
/// 返回用户提示词复制动作使用的合成链接目标；若指令携带稳定消息标识则返回该目标。
fn user_prompt_copy_action_link(block: &AgentViewDirectiveBlock) -> Option<String> {
    let message_id = block.attributes.get("message-id")?.trim();
    (!message_id.is_empty()).then(|| format!("{USER_PROMPT_COPY_ACTION_PREFIX}{message_id}"))
}

/// Builds the linked title-line spans used by one inline thinking header.
/// 构建单个内联思考标题使用的可点击标题行片段。
fn build_thinking_header_spans(
    title: &str,
    action_link_target: Option<&str>,
    foreground: Color,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = build_agent_view_plain_text_spans(
        &format!("🧠 {title}"),
        Font {
            weight: font::Weight::Semibold,
            ..theme::ui_font()
        },
        PlainTextRenderKind::Flow,
        Some(foreground),
        app_theme,
    );

    if let Some(action_link_target) = action_link_target {
        for span in &mut spans {
            span.link = Some(action_link_target.into());
        }
    }

    spans
}

/// Returns the synthetic link target used by one inline thinking header to open the dedicated modal surface.
/// 返回单个内联思考标题使用的合成链接目标，用于打开专用弹层表面。
fn thinking_modal_action_link(block: &AgentViewDirectiveBlock) -> Option<String> {
    let block_id = block.attributes.get("ui-block-id")?.trim();
    (!block_id.is_empty()).then(|| format!("{THINKING_MODAL_ACTION_PREFIX}{block_id}"))
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
    let flow_gap_after =
        if context.block_background.is_some() || context.surface_background.is_some() {
            4.0
        } else {
            8.0
        };
    for item in items {
        match item {
            markdown::Item::Heading(level, text) => {
                let mut spans = build_agent_view_markdown_spans(
                    text,
                    settings.style,
                    MarkdownRenderKind::Flow,
                    app_theme,
                );
                if context.suppress_embedded_backgrounds {
                    for span in &mut spans {
                        span.highlight = None;
                    }
                }
                if let Some(foreground_color) = context.foreground_color {
                    for span in &mut spans {
                        if span.link.is_none() {
                            span.color = Some(foreground_color);
                        }
                    }
                }
                builder.push_rich_paragraph(
                    spans,
                    heading_size(settings, level),
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
                );
            }
            markdown::Item::Paragraph(text) => {
                let mut spans = build_agent_view_markdown_spans(
                    text,
                    settings.style,
                    MarkdownRenderKind::Flow,
                    app_theme,
                );
                if context.suppress_embedded_backgrounds {
                    for span in &mut spans {
                        span.highlight = None;
                    }
                }
                if let Some(foreground_color) = context.foreground_color {
                    for span in &mut spans {
                        if span.link.is_none() {
                            span.color = Some(foreground_color);
                        }
                    }
                }
                builder.push_rich_paragraph(
                    spans,
                    settings.text_size,
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
                );
            }
            markdown::Item::CodeBlock {
                language,
                code,
                lines,
            } => {
                let code_background = if context.suppress_embedded_backgrounds {
                    None
                } else {
                    context
                        .block_background
                        .map(|background| {
                            directive_background(
                                match background.background {
                                    Background::Color(color) => {
                                        mix_color(color, Color::BLACK.scale_alpha(0.12), 0.08)
                                    }
                                    _ => app_theme
                                        .extended_palette()
                                        .background
                                        .weak
                                        .color
                                        .scale_alpha(0.22),
                                },
                                background.border.color.scale_alpha(
                                    if background.border.width <= f32::EPSILON {
                                        0.0
                                    } else {
                                        0.72
                                    },
                                ),
                            )
                        })
                        .or_else(|| {
                            Some(directive_background(
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
                            ))
                        })
                };
                if is_markdown_code_block_language(language.as_deref()) {
                    let preview_inner_padding = if code_background.is_some() {
                        Padding::from([8.0, 10.0])
                    } else {
                        context.inner_padding
                    };
                    let child_start_index = builder.node_count();
                    let preview_source = prepare_markdown_source_for_rendering(code);
                    let preview_context = RenderContext {
                        indent_left: context.indent_left,
                        quote_depth: context.quote_depth,
                        block_background: None,
                        surface_background: code_background,
                        foreground_color: context.foreground_color,
                        inner_padding: preview_inner_padding,
                        suppress_embedded_backgrounds: false,
                    };

                    if raw_source_contains_markdown_table(&preview_source) {
                        render_markdown_source_with_table_fallback(
                            builder,
                            &preview_source,
                            PlainParagraphStyle {
                                indent_left: context.indent_left,
                                gap_after: flow_gap_after,
                                color: context.foreground_color,
                                background: None,
                                surface_background: code_background,
                                inner_padding: preview_inner_padding,
                                suppress_embedded_backgrounds: false,
                                quote_depth: context.quote_depth,
                                ..PlainParagraphStyle::flow(settings.text_size)
                            },
                            settings,
                            app_theme,
                        );
                    } else {
                        let normalized_preview =
                            normalize_markdown_source_for_transcript_display(&preview_source);
                        let preview_markdown = markdown::Content::parse(&normalized_preview);
                        render_markdown_items(
                            builder,
                            preview_markdown.items(),
                            preview_context,
                            settings,
                            app_theme,
                        );
                    }

                    if let Some(panel_background) = code_background {
                        let child_node_count =
                            builder.node_count().saturating_sub(child_start_index);
                        builder.insert_panel(child_start_index, child_node_count, panel_background);
                    }
                    continue;
                }
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
                    flow_gap_after,
                    if code_background.is_some() {
                        Padding::from([8.0, 10.0])
                    } else {
                        context.inner_padding
                    },
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
                        block_background: context.block_background,
                        surface_background: context.surface_background,
                        foreground_color: context.foreground_color,
                        inner_padding: context.inner_padding,
                        suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
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
                builder.push_rich_paragraph(
                    build_markdown_image_spans(alt, url, settings.style, app_theme),
                    settings.text_size,
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
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

/// Builds one readable image paragraph span list so non-web transcript surfaces still preserve alt text and a clickable source URL.
/// 构建一组可读的图片段落片段，使非网页转录表面仍能保留替代文本和可点击的源地址。
fn build_markdown_image_spans(
    alt: &markdown::Text,
    url: &markdown::Uri,
    style: markdown::Style,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = build_agent_view_plain_text_spans(
        "[图片] ",
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        None,
        app_theme,
    );
    let mut alt_spans =
        build_agent_view_markdown_spans(alt, style, MarkdownRenderKind::Flow, app_theme);
    let alt_is_empty = alt_spans.iter().all(|span| span.text.trim().is_empty());

    if !alt_is_empty {
        spans.append(&mut alt_spans);
        spans.extend(build_agent_view_plain_text_spans(
            " ",
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            None,
            app_theme,
        ));
    }

    let mut url_spans = build_agent_view_plain_text_spans(
        url.as_ref(),
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(style.link_color),
        app_theme,
    );
    for span in &mut url_spans {
        span.link = Some(url.clone());
    }
    spans.append(&mut url_spans);

    spans
}

/// Renders one Markdown list while preserving bullet or ordered prefixes in the visible transcript.
/// 渲染一个 Markdown 列表，同时在可见转录中保留项目符号或有序前缀。
const MARKDOWN_LIST_INDENT_STEP: f32 = 20.0;
const MARKDOWN_ORDERED_LIST_MIN_DIGITS: usize = 2;

/// Returns the stable ordered-list prefix text used by the transcript renderer so single-digit and multi-digit items align to one shared gutter.
/// 返回转录渲染器使用的稳定有序列表前缀文本，使单数位和多位数字条目对齐到同一前导槽位。
fn ordered_list_prefix_text(start: u64, index: usize, item_count: usize) -> String {
    let ordinal = start + index as u64;
    let last_ordinal = start + item_count.saturating_sub(1) as u64;
    let digit_width = decimal_digit_count(last_ordinal).max(MARKDOWN_ORDERED_LIST_MIN_DIGITS);

    format!("{ordinal:>digit_width$}. ")
}

/// Returns the decimal digit count of one non-negative integer so ordered-list prefixes can reserve one stable width.
/// 返回某个非负整数的十进制位数，以便有序列表前缀预留稳定宽度。
fn decimal_digit_count(value: u64) -> usize {
    value
        .checked_ilog10()
        .map(|digits| digits as usize + 1)
        .unwrap_or(1)
}

fn render_markdown_list(
    builder: &mut RenderedTranscriptBuilder,
    bullets: &[markdown::Bullet],
    start: Option<u64>,
    context: RenderContext,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let list_text_color = context
        .foreground_color
        .unwrap_or(app_theme.extended_palette().background.base.text);
    let list_gap_after = if context.block_background.is_some() {
        4.0
    } else {
        6.0
    };

    for (index, bullet) in bullets.iter().enumerate() {
        let bullet_prefix = match bullet {
            markdown::Bullet::Point { .. } => String::from("• "),
            markdown::Bullet::Task { done, .. } => {
                format!("[{}] ", if *done { "x" } else { " " })
            }
        };
        let ordered_prefix = start
            .map(|start| ordered_list_prefix_text(start, index, bullets.len()))
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
                    if context.suppress_embedded_backgrounds {
                        for span in &mut spans {
                            span.highlight = None;
                        }
                    }
                    if let Some(foreground_color) = context.foreground_color {
                        for span in &mut spans {
                            if span.link.is_none() {
                                span.color = Some(foreground_color);
                            }
                        }
                    }
                    let mut prefix_spans = build_agent_view_plain_text_spans(
                        &prefix_text,
                        theme::ui_font(),
                        PlainTextRenderKind::Flow,
                        Some(list_text_color),
                        app_theme,
                    );
                    prefix_spans.append(&mut spans);
                    builder.push_rich_paragraph(
                        prefix_spans,
                        settings.text_size,
                        text::LineHeight::default(),
                        text::Wrapping::Word,
                        context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                        0.0,
                        list_gap_after,
                        context.inner_padding,
                        context.block_background,
                        context.quote_depth,
                    );
                    rendered_anything = true;
                }
                _ => {
                    render_markdown_items(
                        builder,
                        std::slice::from_ref(item),
                        RenderContext {
                            indent_left: context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                            quote_depth: context.quote_depth,
                            block_background: context.block_background,
                            surface_background: context.surface_background,
                            foreground_color: context.foreground_color,
                            inner_padding: context.inner_padding,
                            suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
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
                    indent_left: context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                    gap_after: list_gap_after,
                    color: Some(list_text_color),
                    background: context.block_background,
                    surface_background: context.surface_background,
                    inner_padding: context.inner_padding,
                    suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
                    quote_depth: context.quote_depth,
                    ..PlainParagraphStyle::flow(settings.text_size)
                },
                app_theme,
            );
        }
    }
}

/// Returns whether the fenced-code language should be rendered as embedded Markdown preview instead of raw code.
/// 返回围栏代码语言是否应作为内嵌 Markdown 预览渲染，而不是按原始代码显示。
fn is_markdown_code_block_language(language: Option<&str>) -> bool {
    language.map(str::trim).is_some_and(|language| {
        language.eq_ignore_ascii_case("md") || language.eq_ignore_ascii_case("markdown")
    })
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
    let mut active_fence_marker = None::<&'static str>;

    while line_index < lines.len() {
        let trimmed_line = lines[line_index].trim_start();

        if let Some(active_marker) = active_fence_marker {
            buffered_plain_lines.push(lines[line_index]);
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            line_index += 1;
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker);
            buffered_plain_lines.push(lines[line_index]);
            line_index += 1;
            continue;
        }

        if is_markdown_table_header_line(lines[line_index])
            && lines
                .get(line_index + 1)
                .is_some_and(|line| is_markdown_table_alignment_line(line))
        {
            flush_plain_markdown_fallback_buffer(
                builder,
                &mut buffered_plain_lines,
                paragraph_style,
                settings,
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
                        horizontal_placement: paragraph_style.horizontal_placement,
                        gap_before: paragraph_style.gap_before,
                        gap_after: paragraph_style.gap_after.max(8.0),
                        color: paragraph_style.color,
                        render_kind: PlainTextRenderKind::Code,
                        fit_width_to_content: paragraph_style.fit_width_to_content,
                        max_width_ratio: paragraph_style.max_width_ratio,
                        background: paragraph_style.background,
                        surface_background: paragraph_style.surface_background,
                        inner_padding: paragraph_style.inner_padding,
                        suppress_embedded_backgrounds: paragraph_style
                            .suppress_embedded_backgrounds,
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
        settings,
        app_theme,
    );
}

/// Flushes one buffered plain Markdown fallback chunk into the rendered transcript builder while preserving source newlines.
/// 将一段缓冲中的普通 Markdown 回退片段刷新到渲染转录构建器中，并保留源码换行。
fn flush_plain_markdown_fallback_buffer(
    builder: &mut RenderedTranscriptBuilder,
    buffered_plain_lines: &mut Vec<&str>,
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let markdown_source = buffered_plain_lines.join("\n");
    buffered_plain_lines.clear();

    if markdown_source.trim().is_empty() {
        return;
    }

    let prepared_source = rewrite_markdown_footnotes_for_transcript(&markdown_source);
    let normalized_source = normalize_markdown_source_for_transcript_display(&prepared_source);
    let markdown_content = markdown::Content::parse(&normalized_source);
    render_markdown_items(
        builder,
        markdown_content.items(),
        RenderContext {
            indent_left: paragraph_style.indent_left,
            quote_depth: paragraph_style.quote_depth,
            block_background: paragraph_style.background,
            surface_background: paragraph_style.surface_background,
            foreground_color: paragraph_style.color,
            inner_padding: paragraph_style.inner_padding,
            suppress_embedded_backgrounds: paragraph_style.suppress_embedded_backgrounds,
        },
        settings,
        app_theme,
    );
}

/// Repairs Markdown source before rendering so compacted table rows recover real line boundaries.
/// 在渲染前修复 Markdown 源文本，使被压缩的表格行恢复成真实换行边界。
fn prepare_markdown_source_for_rendering(markdown_source: &str) -> String {
    rewrite_markdown_footnotes_for_transcript(&repair_compact_markdown_tables(markdown_source))
}

/// Expands one compacted Markdown table line into multiple logical lines when a valid table boundary pattern is detected.
/// 当检测到有效的表格边界模式时，将单条压缩 Markdown 表格行展开为多条逻辑行。
#[cfg(test)]
fn expand_compact_markdown_table_line(line: &str) -> Option<Vec<String>> {
    repair_compact_markdown_table_line(line).map(|(expanded, _)| {
        expanded
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<String>>()
    })
}

/// Rewrites unsupported Markdown footnote syntax into stable readable transcript text while skipping fenced code blocks.
/// 将暂不直接支持的 Markdown 脚注语法改写成稳定可读的转录文本，并跳过围栏代码块。
fn rewrite_markdown_footnotes_for_transcript(markdown_source: &str) -> String {
    let mut rewritten_lines = Vec::new();
    let mut active_fence_marker = None::<String>;

    for line in markdown_source.lines() {
        let trimmed_line = line.trim_start();

        if let Some(active_marker) = active_fence_marker.as_deref() {
            rewritten_lines.push(line.to_string());
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker.to_string());
            rewritten_lines.push(line.to_string());
            continue;
        }

        if let Some(rewritten_definition) = rewrite_markdown_footnote_definition_line(line) {
            rewritten_lines.push(rewritten_definition);
        } else {
            rewritten_lines.push(rewrite_markdown_footnote_references_in_line(line));
        }
    }

    rewritten_lines.join("\n")
}

/// Rewrites one Markdown footnote-definition line into a readable transcript paragraph label.
/// 将一条 Markdown 脚注定义行改写成可读的转录段落标签。
fn rewrite_markdown_footnote_definition_line(line: &str) -> Option<String> {
    let content_start = line
        .char_indices()
        .find_map(|(byte_index, character)| (!character.is_whitespace()).then_some(byte_index))
        .unwrap_or(line.len());
    let indent = &line[..content_start];
    let trimmed = &line[content_start..];

    if !trimmed.starts_with("[^") {
        return None;
    }

    let definition_end = trimmed.find("]:")?;
    let footnote_id = &trimmed[2..definition_end];
    if footnote_id.trim().is_empty() {
        return None;
    }

    let body = trimmed[definition_end + 2..].trim_start();
    Some(if body.is_empty() {
        format!("{indent}[{}]", footnote_id.trim())
    } else {
        format!("{indent}[{}] {}", footnote_id.trim(), body)
    })
}

/// Rewrites inline Markdown footnote references like `[^1]` into readable transcript labels like `[1]`.
/// 将 `[^1]` 这类行内 Markdown 脚注引用改写成 `[1]` 这样的可读转录标签。
fn rewrite_markdown_footnote_references_in_line(line: &str) -> String {
    let mut rewritten = String::new();
    let mut remaining = line;

    while let Some(reference_start) = remaining.find("[^") {
        rewritten.push_str(&remaining[..reference_start]);

        let candidate = &remaining[reference_start + 2..];
        let Some(reference_end) = candidate.find(']') else {
            rewritten.push_str(&remaining[reference_start..]);
            return rewritten;
        };
        let footnote_id = &candidate[..reference_end];

        if footnote_id.trim().is_empty() {
            rewritten.push_str("[^");
            remaining = candidate;
            continue;
        }

        rewritten.push('[');
        rewritten.push_str(footnote_id.trim());
        rewritten.push(']');
        remaining = &candidate[reference_end + 1..];
    }

    rewritten.push_str(remaining);
    rewritten
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
    let mut active_fence_marker = None::<&'static str>;

    for window in lines.windows(2) {
        let current_trimmed = window[0].trim_start();

        if let Some(active_marker) = active_fence_marker {
            if current_trimmed.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(current_trimmed) {
            active_fence_marker = Some(fence_marker);
            continue;
        }

        if is_markdown_table_header_line(window[0]) && is_markdown_table_alignment_line(window[1]) {
            return true;
        }
    }

    false
}

/// Repairs compact Markdown table clusters so rows glued onto one physical line can still be recognized by the custom transcript renderer.
/// 修复紧凑压缩的 Markdown 表格片段，使粘在同一物理行上的多行表格仍能被自定义转录渲染器识别。
fn repair_compact_markdown_tables(markdown_source: &str) -> String {
    let mut repaired_lines = Vec::new();
    let mut active_table_column_count = None::<usize>;
    let mut active_fence_marker = None::<&'static str>;

    for line in markdown_source.lines() {
        let trimmed_line = line.trim_start();

        if let Some(active_marker) = active_fence_marker {
            repaired_lines.push(line.to_string());
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker);
            active_table_column_count = None;
            repaired_lines.push(line.to_string());
            continue;
        }

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
            crate::markdown_source_to_plain_text(cell.trim())
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
    let inherited_background = paragraph_style
        .surface_background
        .or(paragraph_style.background)
        .map(|background| background.background);
    let body_background = if paragraph_style.suppress_embedded_backgrounds {
        match inherited_background {
            Some(Background::Color(color)) => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.06), 0.04))
            }
            Some(background) => background,
            None => Background::Color(Color::TRANSPARENT),
        }
    } else {
        inherited_background
            .unwrap_or_else(|| Background::Color(palette.background.base.color.scale_alpha(0.98)))
    };
    let body_color = paragraph_style
        .color
        .unwrap_or(palette.background.base.text.scale_alpha(0.96));
    let header_background = if paragraph_style.suppress_embedded_backgrounds {
        match body_background {
            Background::Color(color) if color != Color::TRANSPARENT => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.18), 0.16))
            }
            _ => Background::Color(Color::TRANSPARENT),
        }
    } else {
        match body_background {
            Background::Color(color) => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.16), 0.14))
            }
            _ => Background::Color(palette.background.weak.color.scale_alpha(0.88)),
        }
    };

    RenderedTableStyle {
        text_size: settings.text_size,
        text_color: body_color,
        indent_left: paragraph_style.indent_left,
        gap_before: paragraph_style.gap_before,
        gap_after: paragraph_style.gap_after.max(8.0),
        cell_padding: Padding::from([8.0, 10.0]),
        outer_padding: if paragraph_style.surface_background.is_some() {
            Padding {
                top: 0.0,
                right: paragraph_style.inner_padding.right,
                bottom: 0.0,
                left: paragraph_style.inner_padding.left,
            }
        } else {
            Padding::ZERO
        },
        border_color: body_color.scale_alpha(if paragraph_style.suppress_embedded_backgrounds {
            0.28
        } else {
            0.22
        }),
        header_background,
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

    RenderedParagraphNode {
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

/// Returns the outer flow width available to one paragraph node after indentation is applied.
/// 返回在应用缩进之后可供单个段落节点使用的外部流宽。
fn paragraph_available_outer_width(
    widget_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> f32 {
    (widget_bounds.width - paragraph_node.indent_left).max(1.0)
}

/// Returns the width used to measure one paragraph before it is aligned and optionally shrunk to content.
/// 返回单个段落在对齐并可选收缩到内容之前用于测量的宽度。
fn paragraph_measurement_width(
    widget_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> f32 {
    let available_outer_width = paragraph_available_outer_width(widget_bounds, paragraph_node);
    let capped_outer_width = paragraph_node
        .max_width_ratio
        .map(|ratio| (available_outer_width * ratio.clamp(0.0, 1.0)).max(1.0))
        .unwrap_or(available_outer_width);

    (capped_outer_width - paragraph_node.inner_padding.left - paragraph_node.inner_padding.right)
        .max(1.0)
}

/// Resolves the final outer bounds occupied by one paragraph after alignment and optional content-width fitting are applied.
/// 在应用对齐和可选内容宽度收缩后，解析单个段落最终占据的外部边界。
fn resolve_paragraph_outer_bounds(
    widget_bounds: Rectangle,
    current_y: f32,
    paragraph_node: &RenderedParagraphNode,
    paragraph: &RendererParagraph,
) -> Rectangle {
    let available_outer_width = paragraph_available_outer_width(widget_bounds, paragraph_node);
    let capped_outer_width = paragraph_node
        .max_width_ratio
        .map(|ratio| (available_outer_width * ratio.clamp(0.0, 1.0)).max(1.0))
        .unwrap_or(available_outer_width);
    let paragraph_height = paragraph.min_bounds().height;
    let fitted_outer_width = (paragraph.min_bounds().width
        + paragraph_node.inner_padding.left
        + paragraph_node.inner_padding.right)
        .min(capped_outer_width)
        .max(1.0);
    let outer_width = if paragraph_node.fit_width_to_content {
        fitted_outer_width
    } else {
        capped_outer_width
    };
    let outer_x = match paragraph_node.horizontal_placement {
        RenderedParagraphHorizontalPlacement::Leading => {
            widget_bounds.x + paragraph_node.indent_left
        }
        RenderedParagraphHorizontalPlacement::Trailing => {
            widget_bounds.x + widget_bounds.width - outer_width
        }
    };

    Rectangle::new(
        Point::new(outer_x, current_y),
        Size::new(
            outer_width,
            paragraph_node.inner_padding.top
                + paragraph_height
                + paragraph_node.inner_padding.bottom,
        ),
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
            RenderedTranscriptNode::Panel(_) => {}
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                let summary_paragraph = state.paragraphs.get(paragraph_index)?;
                let body_paragraph = collapsible_node
                    .body
                    .as_ref()
                    .and_then(|_| state.paragraphs.get(paragraph_index + 1));
                let measured_collapsible = measure_rendered_collapsible_directive_node(
                    collapsible_node,
                    summary_paragraph,
                    body_paragraph,
                    widget_bounds,
                    current_y,
                );
                current_y = measured_collapsible.summary_outer_bounds.y
                    + measured_collapsible.summary_outer_bounds.height
                    + collapsible_node.summary.gap_after;
                paragraph_index += 1;

                if measured_collapsible.summary_outer_bounds.contains(position) {
                    let summary_height = summary_paragraph.min_bounds().height;
                    let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                        Point::new(
                            position.x
                                - measured_collapsible.summary_outer_bounds.x
                                - collapsible_node.summary.inner_padding.left,
                            position.y
                                - measured_collapsible.summary_outer_bounds.y
                                - collapsible_node.summary.inner_padding.top,
                        ),
                        &collapsible_node.summary,
                        measured_collapsible.summary_outer_bounds.size(),
                        summary_height,
                    );
                    let local_byte_offset = paragraph_hit_test_to_byte_offset(
                        summary_paragraph,
                        &collapsible_node.summary.display_text,
                        local_point,
                    )?;
                    let local_char_offset = byte_offset_to_char_offset(
                        &collapsible_node.summary.display_text,
                        local_byte_offset,
                    );
                    return Some(
                        (collapsible_node.summary.global_char_range.start + local_char_offset)
                            .min(collapsible_node.summary.global_char_range.end),
                    );
                }

                if let Some(body_node) = collapsible_node.body.as_ref() {
                    if let Some(body_outer_bounds) = measured_collapsible.body_outer_bounds {
                        if body_outer_bounds.contains(position) {
                            if let Some(body_paragraph) = body_paragraph {
                                let max_scroll = (measured_collapsible.body_full_text_height
                                    - body_outer_bounds.height)
                                    .max(0.0);
                                let scroll_offset = state
                                    .collapsible_body_scroll_offsets
                                    .get(&collapsible_node.block_id)
                                    .copied()
                                    .unwrap_or(0.0)
                                    .clamp(0.0, max_scroll);
                                let body_height = body_paragraph.min_bounds().height;
                                let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                                    Point::new(
                                        position.x
                                            - body_outer_bounds.x
                                            - body_node.inner_padding.left,
                                        position.y
                                            - body_outer_bounds.y
                                            - body_node.inner_padding.top
                                            + scroll_offset,
                                    ),
                                    body_node,
                                    body_outer_bounds.size(),
                                    body_height,
                                );
                                let local_byte_offset = paragraph_hit_test_to_byte_offset(
                                    body_paragraph,
                                    &body_node.display_text,
                                    local_point,
                                )?;
                                let local_char_offset = byte_offset_to_char_offset(
                                    &body_node.display_text,
                                    local_byte_offset,
                                );
                                return Some(
                                    (body_node.global_char_range.start + local_char_offset)
                                        .min(body_node.global_char_range.end),
                                );
                            }
                        }

                        current_y =
                            body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after;
                    }

                    paragraph_index += 1;
                }
            }
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                current_y += paragraph_node.gap_before;
                let paragraph = state.paragraphs.get(paragraph_index)?;
                let outer_bounds = resolve_paragraph_outer_bounds(
                    widget_bounds,
                    current_y,
                    paragraph_node,
                    paragraph,
                );
                let paragraph_height = paragraph.min_bounds().height;

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

/// Returns the normalized `(start, end)` selection tuple for the provided anchor and head character offsets.
/// 根据提供的锚点和头部字符偏移返回归一化后的 `(start, end)` 选择区间元组。
fn normalized_selection_for_offsets(
    anchor_char: usize,
    head_char: usize,
) -> Option<(usize, usize)> {
    let start = anchor_char.min(head_char);
    let end = anchor_char.max(head_char);

    if start == end {
        None
    } else {
        Some((start, end))
    }
}

/// Returns the effective selection tuple, including live drag positions that may currently sit outside the visible widget bounds.
/// 返回有效的选择区间，包含当前可能位于可见组件边界之外的实时拖拽位置。
fn effective_selection(
    state: &SelectableTranscriptState,
    document: &RenderedTranscriptDocument,
    widget_bounds: Rectangle,
    cursor_position: Option<Point>,
) -> Option<(usize, usize)> {
    let head_char = if state.dragging {
        cursor_position
            .and_then(|position| drag_selection_head_char(document, state, widget_bounds, position))
            .unwrap_or(state.head_char)
    } else {
        state.head_char
    };

    normalized_selection_for_offsets(state.anchor_char, head_char)
}

/// Returns the current static transcript selection when no live drag extrapolation is needed.
/// 在不需要实时拖拽外推时返回当前静态转录选区。
fn current_selection_for_state(state: &SelectableTranscriptState) -> Option<(usize, usize)> {
    normalized_selection_for_offsets(state.anchor_char, state.head_char)
}

/// Returns the lightweight transcript context-menu actions visible under the current selection state.
/// 根据当前选区状态返回轻量转录右键菜单可见的动作集合。
fn transcript_context_menu_actions(has_selection: bool) -> Vec<TranscriptContextMenuAction> {
    if has_selection {
        vec![
            TranscriptContextMenuAction::CopySelection,
            TranscriptContextMenuAction::SelectAll,
        ]
    } else {
        vec![TranscriptContextMenuAction::SelectAll]
    }
}

/// Returns the localized label rendered for one transcript context-menu action.
/// 返回为某个转录右键菜单动作渲染的本地化标签。
fn transcript_context_menu_action_label(action: TranscriptContextMenuAction) -> &'static str {
    match action {
        TranscriptContextMenuAction::CopySelection => "复制",
        TranscriptContextMenuAction::SelectAll => "全选",
    }
}

/// Computes the clamped transcript context-menu bounds for the provided anchor point and item count.
/// 根据给定锚点与菜单项数量计算经过钳制的转录右键菜单边界。
fn transcript_context_menu_bounds(
    widget_bounds: Rectangle,
    menu_state: TranscriptContextMenuState,
    item_count: usize,
) -> Rectangle {
    let width = TRANSCRIPT_CONTEXT_MENU_WIDTH.min(widget_bounds.width.max(1.0));
    let height = (TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT * item_count as f32).max(1.0);
    let max_x = (widget_bounds.x + widget_bounds.width - width).max(widget_bounds.x);
    let max_y = (widget_bounds.y + widget_bounds.height - height).max(widget_bounds.y);

    Rectangle::new(
        Point::new(
            menu_state.anchor_position.x.clamp(widget_bounds.x, max_x),
            menu_state.anchor_position.y.clamp(widget_bounds.y, max_y),
        ),
        Size::new(width, height),
    )
}

/// Returns the row bounds occupied by the indexed transcript context-menu item.
/// 返回给定索引的转录右键菜单项所占据的行边界。
fn transcript_context_menu_item_bounds(menu_bounds: Rectangle, index: usize) -> Rectangle {
    Rectangle::new(
        Point::new(
            menu_bounds.x,
            menu_bounds.y + TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT * index as f32,
        ),
        Size::new(menu_bounds.width, TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT),
    )
}

/// Hit-tests the transcript context menu and returns the clicked action, if any.
/// 对转录右键菜单执行命中测试，并在命中时返回对应动作。
fn hit_test_transcript_context_menu_action(
    menu_bounds: Rectangle,
    actions: &[TranscriptContextMenuAction],
    position: Point,
) -> Option<TranscriptContextMenuAction> {
    if !menu_bounds.contains(position) {
        return None;
    }

    actions.iter().enumerate().find_map(|(index, action)| {
        transcript_context_menu_item_bounds(menu_bounds, index)
            .contains(position)
            .then_some(*action)
    })
}

/// Executes one transcript context-menu action against the current selection state and clipboard.
/// 针对当前选区状态与剪贴板执行一次转录右键菜单动作。
fn execute_transcript_context_menu_action(
    action: TranscriptContextMenuAction,
    state: &mut SelectableTranscriptState,
    document: &RenderedTranscriptDocument,
    clipboard: &mut dyn Clipboard,
) {
    match action {
        TranscriptContextMenuAction::CopySelection => {
            if let Some((selection_start, selection_end)) = current_selection_for_state(state) {
                clipboard.write(
                    ClipboardKind::Standard,
                    document.selected_text(selection_start, selection_end),
                );
            }
        }
        TranscriptContextMenuAction::SelectAll => {
            state.anchor_char = 0;
            state.head_char = document.len_chars();
        }
    }
}

/// Returns the nearest selection head character offset for one drag pointer position, even when it has crossed above or below the widget bounds.
/// 即使拖拽指针已经越过组件上下边界，也返回其最近的选择头部字符偏移。
fn drag_selection_head_char(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: Point,
) -> Option<usize> {
    let clamped_position = clamp_point_to_rectangle(position, widget_bounds);

    hit_test_rendered_document(document, state, widget_bounds, clamped_position).or_else(|| {
        if position.y <= widget_bounds.y {
            Some(0)
        } else if position.y >= widget_bounds.y + widget_bounds.height {
            Some(document.len_chars())
        } else if position.x <= widget_bounds.x {
            Some(0)
        } else if position.x >= widget_bounds.x + widget_bounds.width {
            Some(document.len_chars())
        } else {
            None
        }
    })
}

/// Clamps one absolute pointer position into the provided rectangle while preserving the closest edge semantics.
/// 将一个绝对指针位置钳制到给定矩形内，同时保留最近边缘的语义。
fn clamp_point_to_rectangle(position: Point, bounds: Rectangle) -> Point {
    Point::new(
        position.x.clamp(bounds.x, bounds.x + bounds.width.max(0.0)),
        position
            .y
            .clamp(bounds.y, bounds.y + bounds.height.max(0.0)),
    )
}

/// Builds one drag auto-scroll request from the current pointer position when it crosses above or below the visible widget bounds.
/// 当当前指针位置越过可见组件上下边界时，构建一份拖拽自动滚动请求。
fn drag_auto_scroll_request_for_position(
    widget_bounds: Rectangle,
    position: Point,
) -> Option<SelectableTranscriptDragAutoScroll> {
    if position.y < widget_bounds.y {
        Some(SelectableTranscriptDragAutoScroll {
            direction: SelectableTranscriptDragAutoScrollDirection::Up,
            overshoot: widget_bounds.y - position.y,
        })
    } else {
        let bottom = widget_bounds.y + widget_bounds.height;

        (position.y > bottom).then_some(SelectableTranscriptDragAutoScroll {
            direction: SelectableTranscriptDragAutoScrollDirection::Down,
            overshoot: position.y - bottom,
        })
    }
}

/// Returns the Markdown link target covering one global character offset inside the provided paragraph node.
/// 返回提供的段落节点内覆盖某个全局字符偏移的 Markdown 链接目标。
fn paragraph_link_at_char_offset(
    paragraph_node: &RenderedParagraphNode,
    global_char_offset: usize,
) -> Option<String> {
    if !paragraph_node
        .global_char_range
        .contains(&global_char_offset)
    {
        return None;
    }

    let local_char_offset =
        global_char_offset.saturating_sub(paragraph_node.global_char_range.start);

    paragraph_node
        .local_grapheme_ranges
        .iter()
        .enumerate()
        .find(|(_, grapheme_range)| grapheme_range.contains(&local_char_offset))
        .and_then(|(grapheme_index, _)| paragraph_node.spans.get(grapheme_index))
        .and_then(|span| span.link.clone())
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

/// Returns the synthetic background action link hit by the provided pointer position, if any.
/// 返回给定指针位置命中的合成背景动作链接（如果存在）。
fn background_action_link_at_position(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: Point,
) -> Option<String> {
    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut current_y = widget_bounds.y;

    for node in &document.nodes {
        match node {
            RenderedTranscriptNode::Panel(_) => {}
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                let summary_paragraph = &state.paragraphs[paragraph_index];
                let body_paragraph = collapsible_node
                    .body
                    .as_ref()
                    .map(|_| &state.paragraphs[paragraph_index + 1]);
                let measured_collapsible = measure_rendered_collapsible_directive_node(
                    collapsible_node,
                    summary_paragraph,
                    body_paragraph,
                    widget_bounds,
                    current_y,
                );

                if measured_collapsible.summary_outer_bounds.contains(position) {
                    if let Some(link_target) =
                        collapsible_node.summary.background_action_link.as_ref()
                    {
                        return Some(link_target.clone());
                    }
                }

                current_y = measured_collapsible.summary_outer_bounds.y
                    + measured_collapsible.summary_outer_bounds.height
                    + collapsible_node.summary.gap_after;
                paragraph_index += 1;

                if let Some(body_node) = collapsible_node.body.as_ref() {
                    if let Some(body_outer_bounds) = measured_collapsible.body_outer_bounds {
                        current_y =
                            body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after;
                    }
                    paragraph_index += 1;
                }
            }
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                current_y += paragraph_node.gap_before;
                let paragraph = &state.paragraphs[paragraph_index];
                let outer_bounds = resolve_paragraph_outer_bounds(
                    widget_bounds,
                    current_y,
                    paragraph_node,
                    paragraph,
                );
                if outer_bounds.contains(position) {
                    if let Some(link_target) = paragraph_node.background_action_link.as_ref() {
                        return Some(link_target.clone());
                    }
                }
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
                    widget_bounds,
                    current_y,
                );
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

/// Applies one wheel-scroll delta to the collapsible semantic block body below the provided pointer position.
/// 将一段滚轮滚动增量应用到给定指针位置下方的折叠语义区块正文。
fn scroll_collapsible_body_at_position(
    document: &RenderedTranscriptDocument,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: Point,
    delta_y: f32,
) -> bool {
    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut current_y = widget_bounds.y;

    for node in &document.nodes {
        match node {
            RenderedTranscriptNode::Panel(_) => {}
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                let summary_paragraph = &state.paragraphs[paragraph_index];
                let body_paragraph = collapsible_node
                    .body
                    .as_ref()
                    .map(|_| &state.paragraphs[paragraph_index + 1]);
                let measured_collapsible = measure_rendered_collapsible_directive_node(
                    collapsible_node,
                    summary_paragraph,
                    body_paragraph,
                    widget_bounds,
                    current_y,
                );
                current_y = measured_collapsible.summary_outer_bounds.y
                    + measured_collapsible.summary_outer_bounds.height
                    + collapsible_node.summary.gap_after;
                paragraph_index += 1;

                if let Some(body_node) = collapsible_node.body.as_ref() {
                    if let Some(body_outer_bounds) = measured_collapsible.body_outer_bounds {
                        let max_scroll = (measured_collapsible.body_full_text_height
                            - body_outer_bounds.height)
                            .max(0.0);
                        if body_outer_bounds.contains(position) && max_scroll > f32::EPSILON {
                            let entry = state
                                .collapsible_body_scroll_offsets
                                .entry(collapsible_node.block_id.clone())
                                .or_insert(0.0);
                            *entry = (*entry + delta_y).clamp(0.0, max_scroll);
                            return true;
                        }

                        current_y =
                            body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after;
                    }
                    paragraph_index += 1;
                }
            }
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                current_y += paragraph_node.gap_before;
                let paragraph = &state.paragraphs[paragraph_index];
                let outer_bounds = resolve_paragraph_outer_bounds(
                    widget_bounds,
                    current_y,
                    paragraph_node,
                    paragraph,
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
                    widget_bounds,
                    current_y,
                );
                current_y += measured_table.outer_bounds.height + table_node.gap_after;
                paragraph_index = table_layout.paragraph_range.end;
                table_index += 1;
            }
            RenderedTranscriptNode::Rule(rule_node) => {
                current_y += rule_node.gap_before + 8.0 + rule_node.gap_after;
            }
        }
    }

    false
}

/// Blends two colors using the provided interpolation ratio so semantic surfaces can derive subtle tonal variants.
/// 使用给定插值比例混合两种颜色，以便语义表面派生细微的色调变体。
fn mix_color(base: Color, overlay: Color, ratio: f32) -> Color {
    let clamped_ratio = ratio.clamp(0.0, 1.0);
    Color {
        r: base.r + (overlay.r - base.r) * clamped_ratio,
        g: base.g + (overlay.g - base.g) * clamped_ratio,
        b: base.b + (overlay.b - base.b) * clamped_ratio,
        a: base.a + (overlay.a - base.a) * clamped_ratio,
    }
}

/// Enumerates the semantic line kinds used by rendered diff previews.
/// 枚举渲染 diff 预览使用的语义行类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffLineKind {
    /// Added diff line highlighted as an insertion.
    /// 作为新增内容高亮的 diff 行。
    Added,
    /// Removed diff line highlighted as a deletion.
    /// 作为删除内容高亮的 diff 行。
    Removed,
    /// Metadata or hunk-header diff line.
    /// 元数据或 hunk 头部 diff 行。
    Note,
    /// Neutral context diff line.
    /// 中性的上下文 diff 行。
    Context,
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
        Background, MARKDOWN_LIST_INDENT_STEP, PlainParagraphStyle, RenderContext,
        RenderedParagraphHorizontalPlacement, RenderedParagraphNode, RenderedTranscriptBuilder,
        RenderedTranscriptNode, SelectableTableAlignment,
        SelectableTranscriptDragAutoScrollDirection, build_markdown_image_spans,
        build_user_prompt_meta_spans, byte_offset_to_char_offset,
        clamp_hit_test_point_to_paragraph_text_bounds, clamp_point_to_rectangle,
        drag_auto_scroll_request_for_position, expand_compact_markdown_table_line,
        multiline_cursor_to_byte_offset, normalize_markdown_source_for_transcript_display,
        ordered_list_prefix_text, parse_markdown_table_alignment, parse_markdown_table_source,
        prepare_markdown_source_for_rendering, raw_source_contains_markdown_table,
        render_markdown_items, render_markdown_source_with_table_fallback, render_thinking_block,
        rendered_table_style, user_prompt_copy_action_link, user_prompt_created_at_label,
    };
    use crate::iced_theme as theme;
    use crate::{AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind};
    use iced::advanced::text;
    use iced::widget::markdown;
    use iced::widget::text::Span;
    use iced::{Color, Padding, Pixels, Point, Size, Theme};
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

    /// Verifies collapsed inline thinking headers keep larger inner padding so the folded title no longer hugs the panel edges.
    /// 验证折叠态内联思考头部会保留更大的内边距，从而避免折叠标题紧贴面板边缘。
    #[test]
    fn collapsed_thinking_header_uses_relaxed_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("true"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "这里是思考内容",
            ":::thinking\n这里是思考内容\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let header_node = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .expect("collapsed thinking should still render one header paragraph");

        assert_eq!(header_node.inner_padding, Padding::from([14.0, 16.0]));
        assert_eq!(header_node.gap_after, 12.0);
    }

    /// Verifies expanded inline thinking keeps the same header inset while giving descendant markdown content a stable horizontal padding.
    /// 验证展开态内联思考会保持与折叠态一致的头部内缩，并为后续 Markdown 内容提供稳定的水平内边距。
    #[test]
    fn expanded_thinking_keeps_stable_header_and_body_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("false"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "**Draft the Response (Mental Refinement):**\n*(Structure according to Plan First)*\n### 标题",
            ":::thinking\n**Draft the Response (Mental Refinement):**\n*(Structure according to Plan First)*\n### 标题\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let mut paragraph_nodes = builder.nodes.iter().filter_map(|node| match node {
            RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        });
        let header_node = paragraph_nodes
            .next()
            .expect("expanded thinking should render one header paragraph");
        let body_node = paragraph_nodes
            .next()
            .expect("expanded thinking should render at least one body paragraph");

        assert_eq!(header_node.inner_padding, Padding::from([14.0, 16.0]));
        assert_eq!(body_node.inner_padding, Padding::from([0.0, 16.0]));
        assert!(body_node.display_text.contains('\n'));
    }

    /// Verifies inline thinking tables inherit the body-side horizontal inset and retain a visible header tint.
    /// 验证内联思考表格会继承正文两侧水平留白，并保留可见的表头着色。
    #[test]
    fn inline_thinking_tables_keep_inset_and_header_tint() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("false"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "| 列1 | 列2 |\n|:---|---:|\n| 数据A | 数据B |",
            ":::thinking\n| 列1 | 列2 |\n|:---|---:|\n| 数据A | 数据B |\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let table_node = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Table(table) => Some(table),
                _ => None,
            })
            .expect("inline thinking should render one table node");

        assert_eq!(table_node.outer_padding.left, 16.0);
        assert_eq!(table_node.outer_padding.right, 16.0);
        assert!(matches!(
            table_node.header_background,
            Background::Color(color) if color != Color::TRANSPARENT
        ));
        assert!(matches!(
            table_node.body_background,
            Background::Color(color) if color != Color::TRANSPARENT
        ));
    }

    /// Verifies background-suppressed standalone tables stay transparent when no semantic surface background is supplied.
    /// 验证未提供语义表面背景时，禁止局部背景的独立表格仍会保持透明。
    #[test]
    fn standalone_suppressed_tables_remain_transparent() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let style = rendered_table_style(
            PlainParagraphStyle {
                suppress_embedded_backgrounds: true,
                ..PlainParagraphStyle::flow(markdown_settings.text_size)
            },
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            style.header_background,
            Background::Color(color) if color == Color::TRANSPARENT
        ));
        assert!(matches!(
            style.body_background,
            Background::Color(color) if color == Color::TRANSPARENT
        ));
    }

    /// Verifies modal thinking tables do not stack another horizontal inset on top of the popup container padding.
    /// 验证弹出态思考表格不会在弹窗容器已有留白之外再叠加额外水平内缩。
    #[test]
    fn modal_thinking_tables_do_not_add_extra_outer_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let style = rendered_table_style(
            PlainParagraphStyle {
                suppress_embedded_backgrounds: true,
                inner_padding: Padding::from([0.0, 16.0]),
                ..PlainParagraphStyle::flow(markdown_settings.text_size)
            },
            markdown_settings,
            &app_theme,
        );

        assert_eq!(style.outer_padding, Padding::ZERO);
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

    /// Verifies Markdown content after one rendered table still preserves quote and inline-format semantics.
    /// 验证渲染表格后的 Markdown 内容仍会保留引用与行内格式语义。
    #[test]
    fn markdown_after_table_keeps_quote_and_inline_formatting() {
        let source =
            "| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n> 💡 **提示**：当前处于 `Plan First`阶段";
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_source_with_table_fallback(
            &mut builder,
            source,
            PlainParagraphStyle::flow(Pixels(16.0)),
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Table(_))
        ));

        let quote_paragraph = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) if paragraph.quote_depth > 0 => {
                    Some(paragraph)
                }
                _ => None,
            })
            .expect("quote paragraph should remain rendered as markdown");

        assert!(quote_paragraph.display_text.contains("提示"));
        assert!(quote_paragraph.display_text.contains("Plan First"));
        assert!(!quote_paragraph.display_text.contains("**"));
        assert!(!quote_paragraph.display_text.contains('`'));
        assert!(!quote_paragraph.display_text.contains('>'));
    }

    /// Verifies fenced Markdown tables render as true Markdown content while remaining enclosed by the code-style background panel.
    /// 验证围栏 Markdown 表格会继续按真实 Markdown 内容渲染，同时仍被代码风格背板包裹。
    #[test]
    fn fenced_markdown_tables_stay_inside_code_blocks() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_source_with_table_fallback(
            &mut builder,
            "```markdown\n| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n```",
            PlainParagraphStyle::flow(Pixels(16.0)),
            markdown_settings,
            &app_theme,
        );

        assert!(
            builder
                .nodes
                .iter()
                .any(|node| matches!(node, RenderedTranscriptNode::Table(_)))
        );
        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Panel(_))
        ));

        assert!(!builder.nodes.iter().any(|node| matches!(
            node,
            RenderedTranscriptNode::Paragraph(paragraph)
                if paragraph.display_text.contains("```")
        )));

        let rendered_table = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Table(table) => Some(table),
                _ => None,
            })
            .expect("fenced markdown should render one real table node");

        assert_eq!(rendered_table.column_count, 2);
        assert_eq!(rendered_table.row_count, 2);
    }

    /// Verifies fenced-code tables neither trigger standalone table detection nor get rewritten by the compact-table repair pass.
    /// 验证围栏代码中的表格既不会触发独立表格检测，也不会被紧凑表格修复器改写。
    #[test]
    fn fenced_markdown_tables_are_ignored_by_table_detection_and_repair() {
        let source = "```markdown\n|列1|列2||---|---||数据A|数据B|\n```";

        assert!(!raw_source_contains_markdown_table(source));
        assert_eq!(prepare_markdown_source_for_rendering(source), source);
    }

    /// Verifies fenced Markdown blocks render their inner content as Markdown preview within one background panel.
    /// 验证围栏 Markdown 代码块会在同一个背板内把内部内容渲染成 Markdown 预览。
    #[test]
    fn fenced_markdown_blocks_render_tables_inside_their_panel() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse(
            "```markdown\n| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n```",
        );
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_items(
            &mut builder,
            markdown_content.items(),
            RenderContext::default(),
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Panel(_))
        ));
        assert!(
            builder
                .nodes
                .iter()
                .any(|node| matches!(node, RenderedTranscriptNode::Table(_)))
        );
        assert!(!builder.nodes.iter().any(|node| matches!(
            node,
            RenderedTranscriptNode::Paragraph(paragraph)
                if paragraph.display_text.contains("```")
        )));
    }

    /// Verifies Markdown footnote references and definitions are rewritten into readable transcript text outside fenced code blocks.
    /// 验证 Markdown 脚注引用和定义会在围栏代码块之外被改写成可读的转录文本。
    #[test]
    fn markdown_footnotes_are_rewritten_for_transcript_display() {
        let source =
            "这是一个脚注引用[^1]。\n\n[^1]: 这里是脚注说明\n```md\n[^2]: 保持代码块原样\n```";
        let prepared = prepare_markdown_source_for_rendering(source);

        assert!(prepared.contains("这是一个脚注引用[1]。"));
        assert!(prepared.contains("[1] 这里是脚注说明"));
        assert!(prepared.contains("```md\n[^2]: 保持代码块原样\n```"));
    }

    /// Verifies Markdown table alignment markers are parsed into left, center, and right transcript alignments.
    /// 验证 Markdown 表格对齐标记会被解析成左、中、右三种转录对齐方式。
    #[test]
    fn markdown_table_alignment_markers_are_parsed() {
        assert_eq!(
            parse_markdown_table_alignment(":---"),
            SelectableTableAlignment::Left
        );
        assert_eq!(
            parse_markdown_table_alignment(":---:"),
            SelectableTableAlignment::Center
        );
        assert_eq!(
            parse_markdown_table_alignment("---:"),
            SelectableTableAlignment::Right
        );
    }

    /// Verifies ordered-list prefixes reserve one stable marker width so one-digit and multi-digit items align to the same content gutter.
    /// 验证有序列表前缀会预留稳定的标记宽度，从而让单数位和多位数字条目对齐到同一正文槽位。
    #[test]
    fn ordered_list_prefixes_use_one_stable_gutter_width() {
        assert_eq!(ordered_list_prefix_text(1, 0, 4), " 1. ");
        assert_eq!(ordered_list_prefix_text(1, 3, 4), " 4. ");
        assert_eq!(ordered_list_prefix_text(8, 0, 4), " 8. ");
        assert_eq!(ordered_list_prefix_text(8, 2, 4), "10. ");
    }

    /// Verifies nested Markdown lists now advance by one shared indent step instead of mixing different magic offsets.
    /// 验证嵌套 Markdown 列表现在按统一缩进步进推进，而不是混用不同的魔法偏移。
    #[test]
    fn nested_markdown_lists_use_one_consistent_indent_step() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse("1. 顶层\n   - 子项");
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_items(
            &mut builder,
            markdown_content.items(),
            RenderContext::default(),
            markdown_settings,
            &app_theme,
        );

        let paragraph_nodes = builder
            .nodes
            .iter()
            .filter_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(paragraph_nodes.len() >= 2);
        assert_eq!(paragraph_nodes[0].indent_left, MARKDOWN_LIST_INDENT_STEP);
        assert_eq!(
            paragraph_nodes[1].indent_left,
            MARKDOWN_LIST_INDENT_STEP * 2.0
        );
    }

    /// Verifies Markdown image fallback spans keep a clickable source URL on the rendered transcript surface.
    /// 验证 Markdown 图片回退片段会在渲染转录表面上保留可点击的源地址链接。
    #[test]
    fn markdown_image_fallback_keeps_clickable_url() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse("![示意图](https://example.com/demo.png)");
        let image_item = markdown_content
            .items()
            .iter()
            .find_map(|item| match item {
                markdown::Item::Image { alt, url, .. } => Some((alt, url)),
                _ => None,
            })
            .expect("markdown image item should exist");

        let spans = build_markdown_image_spans(
            image_item.0,
            image_item.1,
            markdown_settings.style,
            &app_theme,
        );

        assert!(spans.iter().any(|span| {
            span.link
                .as_ref()
                .is_some_and(|link| link == "https://example.com/demo.png")
        }));
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
            horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
            gap_before: 0.0,
            gap_after: 0.0,
            inner_padding: Padding::from([10.0, 12.0]),
            fit_width_to_content: false,
            max_width_ratio: None,
            background: None,
            background_action_link: None,
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

    /// Verifies pointer positions that cross above or below the widget bounds emit the expected drag auto-scroll direction and overshoot distance.
    /// 验证越过组件上下边界的指针位置会发出正确的拖拽自动滚动方向与越界距离。
    #[test]
    fn drag_auto_scroll_request_tracks_vertical_overshoot() {
        let bounds = iced::Rectangle::new(Point::new(20.0, 40.0), Size::new(320.0, 180.0));

        let upward_request = drag_auto_scroll_request_for_position(bounds, Point::new(64.0, 28.0))
            .expect("upward overshoot should emit one request");
        assert_eq!(
            upward_request.direction,
            SelectableTranscriptDragAutoScrollDirection::Up
        );
        assert_eq!(upward_request.overshoot, 12.0);

        let downward_request =
            drag_auto_scroll_request_for_position(bounds, Point::new(96.0, 244.0))
                .expect("downward overshoot should emit one request");
        assert_eq!(
            downward_request.direction,
            SelectableTranscriptDragAutoScrollDirection::Down
        );
        assert_eq!(downward_request.overshoot, 24.0);

        assert!(drag_auto_scroll_request_for_position(bounds, Point::new(96.0, 160.0)).is_none());
    }

    /// Verifies out-of-bounds drag positions are clamped back onto the nearest visible edge so hit-testing can keep selecting while auto-scrolling.
    /// 验证越界拖拽位置会被钳制回最近的可见边缘，从而让命中测试在自动滚动期间继续工作。
    #[test]
    fn drag_positions_are_clamped_back_into_widget_bounds() {
        let bounds = iced::Rectangle::new(Point::new(12.0, 24.0), Size::new(180.0, 96.0));

        assert_eq!(
            clamp_point_to_rectangle(Point::new(-30.0, 8.0), bounds),
            Point::new(12.0, 24.0)
        );
        assert_eq!(
            clamp_point_to_rectangle(Point::new(240.0, 160.0), bounds),
            Point::new(192.0, 120.0)
        );
        assert_eq!(
            clamp_point_to_rectangle(Point::new(40.0, 60.0), bounds),
            Point::new(40.0, 60.0)
        );
    }

    /// Verifies user-prompt metadata lines expose a readable timestamp plus a synthetic copy action link tied to the prompt body.
    /// 验证用户提示词元信息行会暴露可读时间戳以及绑定到提示词正文的合成复制动作链接。
    #[test]
    fn user_prompt_meta_spans_include_timestamp_and_copy_action() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("message-id"), String::from("message-1"));
        attributes.insert(String::from("created-at"), String::from("1715251200000"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::UserPrompt,
            attributes,
            "请输出一个表格",
            ":::user-prompt\n请输出一个表格\n:::",
            false,
        );

        let copy_action_link = user_prompt_copy_action_link(&block);
        let timestamp_label = user_prompt_created_at_label(&block);
        let spans = build_user_prompt_meta_spans(
            &block,
            copy_action_link.as_deref(),
            Color::from_rgb8(90, 90, 90),
            Color::from_rgb8(20, 120, 220),
            &Theme::Dark,
        );

        assert!(timestamp_label.is_some());
        assert!(
            timestamp_label
                .as_deref()
                .is_some_and(|value| value.contains('-'))
        );
        assert!(
            timestamp_label
                .as_deref()
                .is_some_and(|value| value.contains(':'))
        );
        assert_eq!(
            copy_action_link.as_deref(),
            Some("vulcan-agent-view://copy-user-prompt/message-1")
        );
        assert!(spans.iter().any(|span| span.text.contains('复')));
        assert!(spans.iter().any(|span| {
            span.link
                .as_ref()
                .is_some_and(|link| link == "vulcan-agent-view://copy-user-prompt/message-1")
        }));
    }
}

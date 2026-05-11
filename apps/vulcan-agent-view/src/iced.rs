//! Selectable rendered transcript widget used by the primary transcript surface.
//! 主转录渲染表面使用的可选择渲染转录组件。

mod scene;
mod builder;
mod interaction;
mod layout_engine;
mod markdown_render;
mod markdown_source;
mod text_routing;

use self::builder::RenderedTranscriptBuilder;
use self::interaction::*;
use self::layout_engine::*;
use self::markdown_render::*;
use self::markdown_source::*;
use self::scene::*;
use self::text_routing::*;
use crate::iced_theme as theme;
use crate::{AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument};
use iced::advanced::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::Renderer as _;
use iced::advanced::text::{self, Paragraph};
use iced::advanced::widget::tree;
use iced::advanced::{Shell, Widget, overlay};
use iced::font;
use iced::keyboard;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Padding, Pixels, Point, Rectangle,
    Renderer, Size, Theme, Vector,
};
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
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

/// Builds one selectable rendered transcript element from owned structured agent-document inputs so lazy wrappers can cache the heavy widget tree without borrowing external state.
/// 根据拥有所有权的结构化代理文档输入构建一个可选择的渲染转录元素，使惰性包装器能够缓存重型组件树而不借用外部状态。
pub fn view_selectable_rendered_transcript_owned<'a, Message: 'a>(
    document: AgentViewDocument,
    app_theme: Theme,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
) -> Element<'a, Message> {
    let rendered_document = RenderedTranscriptDocument::from_agent_document(&document, &app_theme);
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
    /// Cached flow-node layout metrics kept in visual order for viewport-range culling and hit testing.
    /// 按可视顺序保留的流式节点布局指标缓存，用于视口裁剪与命中测试。
    flow_layouts: Vec<LaidOutFlowNode>,
    /// Cached background-panel layout metrics resolved from the current flow-node geometry.
    /// 根据当前流式节点几何信息解析得到的背景面板布局指标缓存。
    panel_layouts: Vec<LaidOutPanelNode>,
    /// Whether the current paragraph and flow-layout caches are valid for the last measured width and document signature.
    /// 当前段落与流式布局缓存是否对上一次测量宽度和文档签名仍然有效。
    layout_cache_ready: bool,
    /// Width used by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时使用的宽度。
    cached_layout_width: f32,
    /// Stable rendered-document signature used to invalidate cached layout metrics when content or theme changes.
    /// 当内容或主题变化时用于使布局缓存失效的稳定渲染文档签名。
    cached_layout_signature: u64,
    /// Height produced by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时产出的高度。
    cached_layout_height: f32,
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
            flow_layouts: Vec::new(),
            panel_layouts: Vec::new(),
            layout_cache_ready: false,
            cached_layout_width: 0.0,
            cached_layout_signature: 0,
            cached_layout_height: 1.0,
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

impl RenderedTranscriptDocument {
    /// Builds one rendered transcript model from the structured agent document.
    /// 根据结构化代理文档构建一份渲染转录模型。
    fn from_agent_document(document: &AgentViewDocument, app_theme: &Theme) -> Self {
        let palette = app_theme.palette();
        let markdown_settings = theme::markdown_settings(app_theme);
        let layout_signature = compute_rendered_transcript_layout_signature(document, app_theme);
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
            layout_signature,
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

/// Computes one stable transcript-layout signature from the source document and render-affecting theme inputs.
/// 根据源文档与影响渲染的主题输入计算一份稳定的转录布局签名。
fn compute_rendered_transcript_layout_signature(
    document: &AgentViewDocument,
    app_theme: &Theme,
) -> u64 {
    let markdown_settings = theme::markdown_settings(app_theme);
    let palette = app_theme.palette();
    let mut hasher = DefaultHasher::new();

    document.blocks.len().hash(&mut hasher);
    markdown_settings.text_size.0.to_bits().hash(&mut hasher);
    hash_render_color(palette.background.base.color, &mut hasher);
    hash_render_color(palette.background.base.text, &mut hasher);
    hash_render_color(palette.primary.weak.color, &mut hasher);
    hash_render_color(palette.primary.strong.color, &mut hasher);
    hash_render_color(palette.secondary.strong.color, &mut hasher);

    for block in &document.blocks {
        block.raw_source().hash(&mut hasher);
    }

    hasher.finish()
}

/// Hashes one RGBA color into the provided transcript-layout hash state using raw float bit patterns.
/// 使用原始浮点位模式将一组 RGBA 颜色写入给定的转录布局哈希状态。
fn hash_render_color(color: Color, hasher: &mut DefaultHasher) {
    color.r.to_bits().hash(hasher);
    color.g.to_bits().hash(hasher);
    color.b.to_bits().hash(hasher);
    color.a.to_bits().hash(hasher);
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
        let local_widget_bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(available_width, max_size.height.max(1.0)),
        );

        if state.layout_cache_ready
            && state.cached_layout_signature == self.document.layout_signature
            && (state.cached_layout_width - available_width).abs() <= 0.5
        {
            return layout::Node::new(Size::new(
                available_width,
                state.cached_layout_height.max(1.0),
            ));
        }

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

        state.flow_layouts.clear();
        state.panel_layouts.clear();

        let mut paragraph_index = 0usize;
        let mut table_index = 0usize;
        let mut current_y = 0.0f32;

        for (node_index, node) in self.document.nodes.iter().enumerate() {
            match node {
                RenderedTranscriptNode::Panel(_) => {}
                RenderedTranscriptNode::Collapsible(collapsible_node) => {
                    let summary_paragraph_index = paragraph_index;
                    state.paragraphs[summary_paragraph_index] = build_renderer_paragraph(
                        &collapsible_node.summary,
                        Size::new(
                            paragraph_measurement_width(
                                local_widget_bounds,
                                &collapsible_node.summary,
                            ),
                            f32::INFINITY,
                        ),
                        text::Alignment::Default,
                    );

                    let body_paragraph_index = collapsible_node.body.as_ref().map(|body_node| {
                        let body_index = summary_paragraph_index + 1;
                        state.paragraphs[body_index] = build_renderer_paragraph(
                            body_node,
                            Size::new(
                                paragraph_measurement_width(local_widget_bounds, body_node),
                                f32::INFINITY,
                            ),
                            text::Alignment::Default,
                        );
                        body_index
                    });

                    let measured_collapsible = measure_rendered_collapsible_directive_node(
                        collapsible_node,
                        &state.paragraphs[summary_paragraph_index],
                        body_paragraph_index.map(|body_index| &state.paragraphs[body_index]),
                        local_widget_bounds,
                        current_y,
                    );
                    let flow_end_y = if let (Some(body_node), Some(body_outer_bounds)) = (
                        collapsible_node.body.as_ref(),
                        measured_collapsible.body_outer_bounds,
                    ) {
                        body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after
                    } else {
                        measured_collapsible.summary_outer_bounds.y
                            + measured_collapsible.summary_outer_bounds.height
                            + collapsible_node.summary.gap_after
                    };

                    state
                        .flow_layouts
                        .push(LaidOutFlowNode::Collapsible(LaidOutCollapsibleNode {
                            node_index,
                            summary_paragraph_index,
                            body_paragraph_index,
                            summary_outer_bounds: measured_collapsible.summary_outer_bounds,
                            body_outer_bounds: measured_collapsible.body_outer_bounds,
                            body_full_text_height: measured_collapsible.body_full_text_height,
                            flow_end_y,
                        }));

                    current_y = flow_end_y;
                    paragraph_index += 1 + usize::from(body_paragraph_index.is_some());
                }
                RenderedTranscriptNode::Paragraph(paragraph_node) => {
                    state.paragraphs[paragraph_index] = build_renderer_paragraph(
                        paragraph_node,
                        Size::new(
                            paragraph_measurement_width(local_widget_bounds, paragraph_node),
                            f32::INFINITY,
                        ),
                        text::Alignment::Default,
                    );
                    let outer_bounds = resolve_paragraph_outer_bounds(
                        local_widget_bounds,
                        current_y + paragraph_node.gap_before,
                        paragraph_node,
                        &state.paragraphs[paragraph_index],
                    );
                    let flow_end_y =
                        outer_bounds.y + outer_bounds.height + paragraph_node.gap_after;

                    state
                        .flow_layouts
                        .push(LaidOutFlowNode::Paragraph(LaidOutParagraphNode {
                            node_index,
                            paragraph_index,
                            outer_bounds,
                            flow_end_y,
                        }));

                    current_y = flow_end_y;
                    paragraph_index += 1;
                }
                RenderedTranscriptNode::Table(table_node) => {
                    let (table_layout, _) = layout_rendered_table_node(
                        table_node,
                        &mut state.paragraphs,
                        paragraph_index,
                        available_width,
                    );
                    let measured_table = measure_rendered_table_node(
                        table_node,
                        &table_layout,
                        &state.paragraphs[table_layout.paragraph_range.clone()],
                        local_widget_bounds,
                        current_y,
                    );
                    let flow_end_y = measured_table.outer_bounds.y
                        + measured_table.outer_bounds.height
                        + table_node.gap_after;

                    paragraph_index = table_layout.paragraph_range.end;
                    state.table_layouts[table_index] = table_layout;
                    state
                        .flow_layouts
                        .push(LaidOutFlowNode::Table(LaidOutTableNode {
                            node_index,
                            table_index,
                            measured_table,
                            flow_end_y,
                        }));

                    current_y = flow_end_y;
                    table_index += 1;
                }
                RenderedTranscriptNode::Rule(rule_node) => {
                    let rule_y = current_y + rule_node.gap_before;
                    let outer_bounds = Rectangle::new(
                        Point::new(0.0, rule_y),
                        Size::new(available_width.max(1.0), 8.0),
                    );
                    let flow_end_y = rule_y + 8.0 + rule_node.gap_after;

                    state
                        .flow_layouts
                        .push(LaidOutFlowNode::Rule(LaidOutRuleNode {
                            node_index,
                            outer_bounds,
                            flow_end_y,
                        }));

                    current_y = flow_end_y;
                }
            }
        }

        state.panel_layouts = self
            .document
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(node_index, node)| {
                let RenderedTranscriptNode::Panel(panel_node) = node else {
                    return None;
                };
                measure_laid_out_panel_node(
                    node_index,
                    panel_node,
                    &state.flow_layouts,
                    available_width,
                )
            })
            .collect();

        state.layout_cache_ready = true;
        state.cached_layout_width = available_width;
        state.cached_layout_signature = self.document.layout_signature;
        state.cached_layout_height = current_y.max(1.0);

        layout::Node::new(Size::new(available_width, state.cached_layout_height))
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
        let widget_origin = layout.bounds().position();
        let local_viewport = localize_rectangle(*viewport, widget_origin);
        let local_cursor_position = cursor
            .position_including_levitation()
            .map(|position| localize_point(position, widget_origin));
        let selection = effective_selection(
            state,
            &self.document,
            local_viewport,
            local_cursor_position,
        );
        let visible_flow_range = visible_flow_layout_range(&state.flow_layouts, local_viewport);
        let mut panel_border_overlays = Vec::new();

        for panel_layout in state.panel_layouts.iter().filter(|panel_layout| {
            translate_rectangle(panel_layout.outer_bounds, widget_origin).intersects(viewport)
        }) {
            let RenderedTranscriptNode::Panel(panel_node) =
                &self.document.nodes[panel_layout.node_index]
            else {
                continue;
            };
            let outer_bounds = translate_rectangle(panel_layout.outer_bounds, widget_origin);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: outer_bounds,
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
                panel_border_overlays.push((outer_bounds, panel_node.background.border));
            }
        }

        for flow_layout in &state.flow_layouts[visible_flow_range] {
            match flow_layout {
                LaidOutFlowNode::Collapsible(layout_node) => {
                    let RenderedTranscriptNode::Collapsible(collapsible_node) =
                        &self.document.nodes[layout_node.node_index]
                    else {
                        continue;
                    };
                    let summary_paragraph = &state.paragraphs[layout_node.summary_paragraph_index];
                    let summary_outer_bounds =
                        translate_rectangle(layout_node.summary_outer_bounds, widget_origin);

                    if let Some(background) = collapsible_node.summary.background {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: summary_outer_bounds,
                                border: background.border,
                                ..renderer::Quad::default()
                            },
                            background.background,
                        );
                    }

                    renderer.fill_paragraph(
                        summary_paragraph,
                        paragraph_local_origin(summary_outer_bounds, &collapsible_node.summary),
                        collapsible_node
                            .summary
                            .spans
                            .first()
                            .and_then(|span| span.color)
                            .unwrap_or(defaults.text_color),
                        *viewport,
                    );

                    if let (Some(body_node), Some(body_paragraph_index), Some(body_outer_bounds)) = (
                        collapsible_node.body.as_ref(),
                        layout_node.body_paragraph_index,
                        layout_node.body_outer_bounds,
                    ) {
                        let body_paragraph = &state.paragraphs[body_paragraph_index];
                        let body_outer_bounds =
                            translate_rectangle(body_outer_bounds, widget_origin);
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: body_outer_bounds,
                                border: collapsible_node.body_background.border,
                                ..renderer::Quad::default()
                            },
                            collapsible_node.body_background.background,
                        );

                        let max_scroll =
                            (layout_node.body_full_text_height - body_outer_bounds.height).max(0.0);
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
                                Point::new(text_clip_bounds.x, text_clip_bounds.y - scroll_offset),
                                body_node
                                    .spans
                                    .first()
                                    .and_then(|span| span.color)
                                    .unwrap_or(defaults.text_color),
                                *viewport,
                            );
                        });

                        if max_scroll > f32::EPSILON {
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
                                * (body_outer_bounds.height / layout_node.body_full_text_height))
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
                    }
                }
                LaidOutFlowNode::Paragraph(layout_node) => {
                    let RenderedTranscriptNode::Paragraph(paragraph_node) =
                        &self.document.nodes[layout_node.node_index]
                    else {
                        continue;
                    };
                    let paragraph = &state.paragraphs[layout_node.paragraph_index];
                    let outer_bounds = translate_rectangle(layout_node.outer_bounds, widget_origin);

                    if let Some(background) = paragraph_node.background {
                        let background_bounds = if background.border.width <= f32::EPSILON {
                            Rectangle::new(
                                outer_bounds.position(),
                                Size::new(
                                    outer_bounds.width,
                                    outer_bounds.height
                                        + (layout_node.flow_end_y
                                            - layout_node.outer_bounds.y
                                            - layout_node.outer_bounds.height),
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

                    if let Some((selection_start, selection_end)) = selection {
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
                }
                LaidOutFlowNode::Table(layout_node) => {
                    let RenderedTranscriptNode::Table(table_node) =
                        &self.document.nodes[layout_node.node_index]
                    else {
                        continue;
                    };
                    let table_layout = &state.table_layouts[layout_node.table_index];
                    let table_paragraphs = &state.paragraphs[table_layout.paragraph_range.clone()];
                    let measured_table =
                        translate_measured_table_node(&layout_node.measured_table, widget_origin);

                    draw_rendered_table_node(
                        renderer,
                        &self.document,
                        defaults,
                        selection,
                        table_node,
                        table_paragraphs,
                        &measured_table,
                        viewport,
                    );
                }
                LaidOutFlowNode::Rule(layout_node) => {
                    let RenderedTranscriptNode::Rule(rule_node) =
                        &self.document.nodes[layout_node.node_index]
                    else {
                        continue;
                    };
                    let outer_bounds = translate_rectangle(layout_node.outer_bounds, widget_origin);
                    let rule_y = outer_bounds.y + 3.0;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(outer_bounds.x, rule_y),
                                Size::new(outer_bounds.width.max(1.0), 2.0),
                            ),
                            ..renderer::Quad::default()
                        },
                        Background::Color(rule_node.color),
                    );
                }
            }
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
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();

        let local_viewport = localize_rectangle(*_viewport, layout.bounds().position());

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                state.context_menu = None;
                shell.request_redraw();
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    let local_position = localize_point(position, layout.bounds().position());
                    let delta_y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y * 36.0,
                        mouse::ScrollDelta::Pixels { y, .. } => -y,
                    };

                    if scroll_collapsible_body_at_position(
                        &self.document,
                        state,
                        local_position,
                        delta_y,
                    ) {
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.context_menu = None;
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    let local_position = localize_point(position, layout.bounds().position());
                    if let Some(hit_char) =
                        hit_test_rendered_document(&self.document, state, local_position)
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
                if let Some(position) = cursor.position_including_levitation() {
                    let local_position = localize_point(position, layout.bounds().position());
                    if let Some(hit_char) = drag_selection_head_char(
                        &self.document,
                        state,
                        local_viewport,
                        local_position,
                    ) {
                        if hit_char != state.head_char {
                            state.drag_moved = true;
                        }
                        state.head_char = hit_char;
                        shell.request_redraw();
                        shell.capture_event();
                    }

                    self.publish_drag_auto_scroll_request(
                        state,
                        drag_auto_scroll_request_for_position(local_viewport, local_position),
                        shell,
                    );
                } else {
                    self.publish_drag_auto_scroll_request(state, None, shell);
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                if let Some(position) = cursor.position_including_levitation() {
                    let local_position = localize_point(position, layout.bounds().position());
                    if let Some(hit_char) = drag_selection_head_char(
                        &self.document,
                        state,
                        local_viewport,
                        local_position,
                    ) {
                        if hit_char != state.head_char {
                            state.drag_moved = true;
                        }
                        state.head_char = hit_char;
                        shell.request_redraw();
                    }
                }
                if !state.drag_moved && state.anchor_char == state.head_char {
                    if let Some(position) = cursor.position_over(layout.bounds()) {
                        let local_position = localize_point(position, layout.bounds().position());
                        let clicked_link_target =
                            hit_test_rendered_document(&self.document, state, local_position)
                                .and_then(|hit_char| self.document.link_at_char_offset(hit_char))
                                .or_else(|| {
                                    background_action_link_at_position(
                                        &self.document,
                                        state,
                                        local_position,
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
                                        shell.write_clipboard(copy_payload.clone().into());
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
                                shell.write_clipboard(link_url.into());
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
                if let Some((selection_start, selection_end)) = effective_selection(
                    state,
                    &self.document,
                    local_viewport,
                    cursor
                        .position_including_levitation()
                        .map(|position| localize_point(position, layout.bounds().position())),
                ) {
                    shell.write_clipboard(
                        self.document
                            .selected_text(selection_start, selection_end)
                            .into(),
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
        let local_position = localize_point(position, layout.bounds().position());

        if let Some(menu_state) = state.context_menu {
            let menu_actions =
                transcript_context_menu_actions(current_selection_for_state(state).is_some());
            let menu_bounds =
                transcript_context_menu_bounds(layout.bounds(), menu_state, menu_actions.len());
            if menu_bounds.contains(position) {
                return mouse::Interaction::Pointer;
            }
        }

        if hit_test_rendered_document(&self.document, state, local_position)
            .and_then(|hit_char| self.document.link_at_char_offset(hit_char))
            .or_else(|| background_action_link_at_position(&self.document, state, local_position))
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
        let palette = theme.palette();
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
                            ellipsis: text::Ellipsis::None,
                            hint_factor: None,
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
                            shell,
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

    let palette = app_theme.palette();
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
    let palette = app_theme.palette();
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
    let is_inline_collapsed = is_inline_surface
        && block
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
    let render_header = block
        .attributes
        .get("ui-render-thinking-header")
        .map(|value| value != "false")
        .unwrap_or(true);
    let header_padding = if is_inline_surface {
        Padding::from([14.0, 16.0])
    } else {
        Padding::ZERO
    };
    let header_gap_after = if is_inline_collapsed { 12.0 } else { 6.0 };

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
            header_gap_after,
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

    if is_inline_collapsed {
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
    let palette = app_theme.palette();
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
                                    _ => {
                                        app_theme.palette().background.weak.color.scale_alpha(0.22)
                                    }
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
                                app_theme.palette().background.weak.color.scale_alpha(0.24),
                                app_theme
                                    .palette()
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
                        .palette()
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
fn execute_transcript_context_menu_action<Message>(
    action: TranscriptContextMenuAction,
    state: &mut SelectableTranscriptState,
    document: &RenderedTranscriptDocument,
    shell: &mut Shell<'_, Message>,
) {
    match action {
        TranscriptContextMenuAction::CopySelection => {
            if let Some((selection_start, selection_end)) = current_selection_for_state(state) {
                shell.write_clipboard(
                    document
                        .selected_text(selection_start, selection_end)
                        .into(),
                );
            }
        }
        TranscriptContextMenuAction::SelectAll => {
            state.anchor_char = 0;
            state.head_char = document.len_chars();
        }
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
    let palette = app_theme.palette();

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

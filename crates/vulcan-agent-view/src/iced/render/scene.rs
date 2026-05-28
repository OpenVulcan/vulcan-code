//! Scene-model types shared by the selectable transcript renderer.
//! 可选择转录渲染器共享的场景模型类型。

use iced::advanced::text;
use iced::widget::text::Span;
use iced::{Background, Border, Color, Font, Padding, Pixels, Point, Rectangle, Size};
use std::collections::BTreeMap;
use std::ops::Range;
use vulcan_markdown as markdown;

/// Immutable rendered transcript model consumed by the custom widget.
/// 自定义组件消费的不可变渲染转录模型。
#[derive(Debug, Clone)]
pub(super) struct RenderedTranscriptDocument {
    /// Ordered visual nodes making up the transcript body.
    /// 组成转录正文的有序可视节点集合。
    pub(super) nodes: Vec<RenderedTranscriptNode>,
    /// Full copy payload that mirrors the visible rendered text projection.
    /// 与可见渲染文本投影对齐的完整复制载荷。
    pub(super) full_text: String,
    /// Highlight color used when the user selects transcript content.
    /// 用户选择转录内容时使用的高亮颜色。
    pub(super) selection_color: Color,
    /// Link-scoped transcript actions keyed by their synthetic link target.
    /// 以合成链接目标为键保存的链接级转录动作集合。
    pub(super) link_actions: BTreeMap<String, RenderedTranscriptLinkAction>,
    /// Stable layout signature used to reuse expensive paragraph geometry while the transcript width and render-affecting inputs stay unchanged.
    /// 当转录宽度和影响渲染的输入保持不变时，用于复用昂贵段落几何信息的稳定布局签名。
    pub(super) layout_signature: u64,
}

impl RenderedTranscriptDocument {
    /// Returns the selected rendered text slice bounded by the provided global character offsets.
    /// 返回由提供的全局字符偏移限定的已选渲染文本切片。
    pub(super) fn selected_text(&self, start_char: usize, end_char: usize) -> String {
        slice_char_range(&self.full_text, start_char, end_char)
    }

    /// Returns the total rendered character count available for transcript selection.
    /// 返回可用于转录选择的总渲染字符数量。
    pub(super) fn len_chars(&self) -> usize {
        self.full_text.chars().count()
    }

    /// Returns the Markdown link target covering the provided global character offset, if any.
    /// 返回覆盖给定全局字符偏移的 Markdown 链接目标（如果存在）。
    pub(super) fn link_at_char_offset(&self, global_char_offset: usize) -> Option<String> {
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
                RenderedTranscriptNode::MediaCard(_) => {}
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
    pub(super) fn link_action_for_target(
        &self,
        link_target: &str,
    ) -> Option<&RenderedTranscriptLinkAction> {
        self.link_actions.get(link_target)
    }
}

/// Link-scoped interaction emitted by synthetic transcript chrome spans.
/// 由合成转录外框片段发出的链接级交互动作。
#[derive(Debug, Clone)]
pub(super) enum RenderedTranscriptLinkAction {
    #[allow(dead_code)]
    /// Toggles one generic inline semantic block when the synthetic header chrome is clicked.
    /// 当点击合成标题外框时切换某个通用内联语义区块。
    ToggleBlockInline(String),
    /// Copies the provided payload into the clipboard instead of copying the synthetic link string itself.
    /// 将提供的载荷复制到剪贴板，而不是复制合成链接字符串本身。
    CopyText(String),
    /// Requests host-side editing for one user-prompt transcript message.
    /// 请求宿主侧编辑某条用户提示词转录消息。
    EditUserPrompt(String),
    /// Opens one inline thinking block inside the dedicated centered modal surface.
    /// 在专用居中弹层表面中打开某个内联思考区块。
    OpenThinkingModal(String),
    /// Opens one tool block in a dedicated centered modal surface.
    /// 在专用居中弹层表面中打开某个工具区块。
    OpenToolModal(String),
    /// Opens one image preview modal for one generated-media card.
    /// 为某个生成媒体卡片打开一条图片预览弹窗。
    OpenImagePreview {
        /// Human-readable file name rendered by the host preview header.
        /// 显示在宿主预览标题栏中的人类可读文件名。
        file_name: String,
        /// Local absolute path consumed by the host image preview modal.
        /// 供宿主图片预览弹窗使用的本地绝对路径。
        local_path: String,
    },
    /// Toggles inline audio playback for one generated-media card.
    /// 为某个生成媒体卡片切换内联音频播放状态。
    ToggleAudioPlayback {
        /// Human-readable file name used by host status text.
        /// 宿主状态文案使用的人类可读文件名。
        file_name: String,
        /// Local absolute path consumed by the host audio player.
        /// 供宿主音频播放器使用的本地绝对路径。
        local_path: String,
    },
    /// Opens one generated-media path with the system default application.
    /// 使用系统默认应用打开一条生成媒体路径。
    OpenMediaWithDefaultApp(String),
    /// Carries one hover-only turn-statistics tooltip payload for lightweight transcript footer chips.
    /// 为轻量转录页脚统计标签携带一份仅悬浮展示的按轮次统计提示载荷。
    ShowTurnStatsTooltip {
        /// Short title shown at the top of the tooltip body.
        /// 显示在提示正文顶部的简短标题。
        title: String,
        /// Multi-line human-readable tooltip body.
        /// 面向用户的多行提示正文。
        body: String,
    },
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

/// Returns the hyperlink target covering one global character offset inside the provided paragraph, if any.
/// 返回给定段落中覆盖某个全局字符偏移的超链接目标（如果存在）。
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
    for (span, local_range) in paragraph_node
        .selection_spans
        .iter()
        .zip(&paragraph_node.local_grapheme_ranges)
    {
        if local_range.contains(&local_char_offset) {
            return span.link.clone().map(Into::into);
        }
    }

    None
}

/// One rendered visual node in the transcript flow.
/// 转录流中的一个渲染可视节点。
#[derive(Debug, Clone)]
pub(super) enum RenderedTranscriptNode {
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
    /// Rich media summary card rendered inline for host-generated attachments.
    /// 为宿主生成附件在转录中内联渲染的富媒体摘要卡片。
    MediaCard(RenderedMediaCardNode),
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
pub(super) struct RenderedPanelNode {
    /// Number of immediately following transcript nodes covered by the panel.
    /// 当前面板所覆盖的紧随其后的转录节点数量。
    pub(super) child_node_count: usize,
    /// Background treatment drawn behind the covered child nodes.
    /// 绘制在被覆盖子节点背后的背景样式。
    pub(super) background: RenderedParagraphBackground,
    /// Optional left accent rule drawn for note-like panels such as completed inline thinking.
    /// 为已完成内联思考等备注式面板绘制的可选左侧强调竖线。
    pub(super) left_accent_rule: Option<RenderedPanelAccentRule>,
}

/// One decorative left accent rule optionally rendered along the panel edge.
/// 可选渲染在面板边缘的一条装饰性左侧强调竖线。
#[derive(Debug, Clone, Copy)]
pub(super) struct RenderedPanelAccentRule {
    /// Stroke color used by the left accent rule.
    /// 左侧强调竖线使用的描边颜色。
    pub(super) color: Color,
    /// Visual width of the accent rule.
    /// 强调竖线的可见宽度。
    pub(super) width: f32,
    /// Top inset applied before drawing the accent rule.
    /// 绘制强调竖线之前应用的顶部内缩。
    pub(super) top_inset: f32,
    /// Bottom inset applied before ending the accent rule.
    /// 结束绘制强调竖线之前应用的底部内缩。
    pub(super) bottom_inset: f32,
}

/// One compact collapsible semantic block rendered inside the transcript flow.
/// 渲染在转录流中的单个紧凑折叠语义区块。
#[derive(Debug, Clone)]
pub(super) struct RenderedCollapsibleDirectiveNode {
    /// Stable block identifier used for collapse-state toggling and per-block body scroll persistence.
    /// 用于折叠状态切换与区块级正文滚动持久化的稳定区块标识。
    pub(super) block_id: String,
    /// Summary paragraph rendered as the always-visible clickable row.
    /// 作为始终可见点击行渲染的摘要段落。
    pub(super) summary: RenderedParagraphNode,
    /// Optional body paragraph rendered inside the expandable viewport.
    /// 渲染在可展开视口内部的可选正文段落。
    pub(super) body: Option<RenderedParagraphNode>,
    /// Whether the block is currently collapsed.
    /// 当前区块是否处于折叠状态。
    pub(super) collapsed: bool,
    /// Maximum visible line count before the body viewport starts scrolling.
    /// 在正文视口开始滚动前允许显示的最大行数。
    pub(super) max_visible_lines: usize,
    /// Background treatment used by the expandable body viewport.
    /// 可展开正文视口使用的背景样式。
    pub(super) body_background: RenderedParagraphBackground,
    /// Scrollbar track color used when the body overflows.
    /// 正文溢出时滚动条轨道使用的颜色。
    pub(super) scrollbar_track_color: Color,
    /// Scrollbar thumb color used when the body overflows.
    /// 正文溢出时滚动条滑块使用的颜色。
    pub(super) scrollbar_thumb_color: Color,
}

/// One text-bearing paragraph node in the rendered transcript flow.
/// 渲染转录流中的一个带文本段落节点。
#[derive(Debug, Clone)]
pub(super) struct RenderedParagraphNode {
    /// Stable content/style signature seed computed once when the paragraph node is created so repeated layout passes can avoid rehashing every span.
    /// 在段落节点创建时一次性计算好的稳定内容/样式签名种子，使重复布局过程无需反复哈希全部片段。
    pub(super) signature_seed: u64,
    /// Visible display text mirrored into the global copy buffer.
    /// 被镜像到全局复制缓冲区中的可见显示文本。
    pub(super) display_text: String,
    /// Visual styled spans used for normal paragraph rendering and layout.
    /// 用于常规段落渲染与布局的可视样式片段。
    pub(super) spans: Vec<Span<'static, markdown::Uri, Font>>,
    /// Grapheme-level styled spans used only when precise selection rectangles are needed.
    /// 仅在需要精确选区矩形时使用的字素级样式片段。
    pub(super) selection_spans: Vec<Span<'static, markdown::Uri, Font>>,
    /// Character range of this paragraph inside the global rendered transcript text buffer.
    /// 当前段落在全局渲染转录文本缓冲区中的字符范围。
    pub(super) global_char_range: Range<usize>,
    /// Character ranges aligned with every grapheme-level selection span inside the paragraph-local text.
    /// 与段落局部文本内每个字素级选区片段对齐的字符范围。
    pub(super) local_grapheme_ranges: Vec<Range<usize>>,
    /// Text size used by the paragraph.
    /// 当前段落使用的文本字号。
    pub(super) text_size: Pixels,
    /// Line height used by the paragraph.
    /// 当前段落使用的行高。
    pub(super) line_height: text::LineHeight,
    /// Text wrapping strategy used by the paragraph.
    /// 当前段落使用的换行策略。
    pub(super) wrapping: text::Wrapping,
    /// Left indentation applied to the paragraph.
    /// 当前段落应用的左侧缩进。
    pub(super) indent_left: f32,
    /// Horizontal placement used by the paragraph container inside the transcript viewport.
    /// 当前段落容器在转录视口内部使用的水平放置方式。
    pub(super) horizontal_placement: RenderedParagraphHorizontalPlacement,
    /// Top gap placed before the paragraph in the flow.
    /// 当前段落在流中之前放置的顶部间距。
    pub(super) gap_before: f32,
    /// Bottom gap placed after the paragraph in the flow.
    /// 当前段落在流中之后放置的底部间距。
    pub(super) gap_after: f32,
    /// Inner padding applied around the paragraph text when its background is drawn.
    /// 绘制背景时围绕段落文本应用的内边距。
    pub(super) inner_padding: Padding,
    /// Whether the paragraph background box should shrink to its content width instead of filling the whole flow width.
    /// 当前段落背景框是否应收缩到内容宽度，而不是填满整条流宽。
    pub(super) fit_width_to_content: bool,
    /// Optional width cap ratio applied before the paragraph is measured and aligned.
    /// 在段落测量和对齐之前应用的可选宽度上限比例。
    pub(super) max_width_ratio: Option<f32>,
    /// Optional background treatment used to preserve block-level visuals.
    /// 用于保留块级视觉的可选背景样式。
    pub(super) background: Option<RenderedParagraphBackground>,
    /// Optional synthetic action link triggered when the paragraph background box itself is clicked.
    /// 当点击当前段落背景框本身时触发的可选合成动作链接。
    pub(super) background_action_link: Option<String>,
    /// Optional quote bar depth used to draw nested quotation guides.
    /// 用于绘制嵌套引用导线的可选引用深度。
    pub(super) quote_depth: usize,
}

/// Media modality rendered by one inline generated-media card.
/// 单个内联生成媒体卡片渲染的媒体模态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderedMediaKind {
    /// Raster image preview card.
    /// 光栅图片预览卡片。
    Image,
    /// Audio voice-strip card.
    /// 音频语音条卡片。
    Audio,
    /// Video preview-placeholder card.
    /// 视频预览占位卡片。
    Video,
    /// Document placeholder card.
    /// 文档占位卡片。
    Pdf,
}

/// One inline generated-media summary card rendered inside the transcript flow.
/// 在转录流中渲染的单个内联生成媒体摘要卡片。
#[derive(Debug, Clone)]
pub(super) struct RenderedMediaCardNode {
    /// Stable layout-affecting signature seed computed once when the card node is created.
    /// 在卡片节点创建时一次性计算好的稳定布局影响签名种子。
    pub(super) signature_seed: u64,
    /// Media modality represented by the current card.
    /// 当前卡片表示的媒体模态。
    pub(super) kind: RenderedMediaKind,
    /// Human-readable title shown above or beside the preview body.
    /// 显示在预览主体上方或旁侧的人类可读标题。
    pub(super) title: String,
    /// Human-readable file name shown as secondary metadata.
    /// 作为次级元信息显示的人类可读文件名。
    pub(super) file_name: String,
    /// Local absolute path used by host-side preview, playback, or file-manager actions.
    /// 供宿主侧预览、播放或文件管理器动作使用的本地绝对路径。
    pub(super) local_path: String,
    /// Optional in-memory raster preview handle used when the actionable media file itself cannot be drawn directly inside the transcript, such as MP4 video cards backed by dynamically decoded first frames.
    /// 当可执行媒体文件本身无法直接在转录中绘制时使用的可选内存预览句柄，例如使用动态解码首帧的 MP4 视频卡片。
    pub(super) preview_image_handle: Option<iced::widget::image::Handle>,
    /// Optional intrinsic raster-preview aspect ratio captured from the local file so image and video cards can size themselves proportionally.
    /// 从本地图像预览文件中捕获的可选原始宽高比，供图片卡片与视频卡片按比例自适应尺寸。
    pub(super) image_aspect_ratio: Option<f32>,
    /// Total number of generated outputs represented by the current card.
    /// 当前卡片代表的生成输出总数量。
    pub(super) output_count: usize,
    /// Whether the card should suppress transcript-side title and metadata chrome so the media itself becomes the primary visual.
    /// 卡片是否应隐藏转录侧标题与元信息外框，从而让媒体自身成为主视觉。
    pub(super) show_caption: bool,
    /// Whether the card should use compact attachment-strip sizing instead of the richer generated-media summary layout.
    /// 卡片是否应使用紧凑附件条尺寸，而不是较丰富的生成媒体摘要布局。
    pub(super) compact_style: bool,
    /// Whether the card currently represents the actively playing generated-audio clip.
    /// 卡片当前是否代表正在播放的生成音频片段。
    pub(super) is_active: bool,
    /// Top flow gap placed before the card.
    /// 卡片前方放置的顶部流式间距。
    pub(super) gap_before: f32,
    /// Bottom flow gap placed after the card.
    /// 卡片后方放置的底部流式间距。
    pub(super) gap_after: f32,
    /// Maximum transcript-width ratio consumed by the card before clamping to the modality-specific width cap.
    /// 在钳制到模态专属宽度上限前，卡片可占用的最大转录宽度比例。
    pub(super) max_width_ratio: f32,
    /// Horizontal placement used when positioning the card inside the transcript viewport.
    /// 当前卡片在转录视口内部定位时使用的水平放置方式。
    pub(super) horizontal_placement: RenderedParagraphHorizontalPlacement,
    /// Background treatment painted behind the whole card.
    /// 绘制在整张卡片背后的背景样式。
    pub(super) background: RenderedParagraphBackground,
    /// Accent color used by icons, wave bars, and emphasis chrome.
    /// 图标、波形条和强调外框使用的强调色。
    pub(super) accent_color: Color,
    /// Optional synthetic action link triggered when the card body is left-clicked.
    /// 左键点击卡片主体时触发的可选合成动作链接。
    pub(super) action_link: Option<String>,
}

/// One horizontal rule node in the rendered transcript flow.
/// 渲染转录流中的一个水平分隔线节点。
#[derive(Debug, Clone)]
pub(super) struct RenderedRuleNode {
    /// Stable layout-affecting signature seed computed once when the rule node is created.
    /// 在分隔线节点创建时一次性计算好的稳定布局影响签名种子。
    pub(super) signature_seed: u64,
    /// Top gap placed before the rule.
    /// 分隔线之前放置的顶部间距。
    pub(super) gap_before: f32,
    /// Bottom gap placed after the rule.
    /// 分隔线之后放置的底部间距。
    pub(super) gap_after: f32,
    /// Display color of the rule.
    /// 分隔线显示颜色。
    pub(super) color: Color,
}

/// One rendered table node in the transcript flow.
/// 转录流中的一个渲染表格节点。
#[derive(Debug, Clone)]
pub(super) struct RenderedTableNode {
    /// Stable layout-affecting signature seed computed once when the table node is created.
    /// 在表格节点创建时一次性计算好的稳定布局影响签名种子。
    pub(super) signature_seed: u64,
    /// Flat table cells stored in row-major order.
    /// 以行优先顺序存储的扁平表格单元格集合。
    pub(super) cells: Vec<RenderedTableCellNode>,
    /// Number of columns in the rendered table.
    /// 当前渲染表格的列数。
    pub(super) column_count: usize,
    /// Number of rows in the rendered table, including the header row.
    /// 当前渲染表格的总行数，包含表头行。
    pub(super) row_count: usize,
    /// Left indentation applied to the whole table.
    /// 应用于整张表格的左侧缩进。
    pub(super) indent_left: f32,
    /// Top gap placed before the table in the flow.
    /// 当前表格在流中之前放置的顶部间距。
    pub(super) gap_before: f32,
    /// Bottom gap placed after the table in the flow.
    /// 当前表格在流中之后放置的底部间距。
    pub(super) gap_after: f32,
    /// Inner padding applied to every table cell.
    /// 应用于每个表格单元格的内部边距。
    pub(super) cell_padding: Padding,
    /// Outer horizontal padding applied to the table inside one semantic surface.
    /// 应用于单个语义表面内部表格的外层水平留白。
    pub(super) outer_padding: Padding,
    /// Border color used by the outer frame and grid separators.
    /// 外框与网格分隔线使用的边框颜色。
    pub(super) border_color: Color,
    /// Background fill used by header cells.
    /// 表头单元格使用的背景填充色。
    pub(super) header_background: Background,
    /// Background fill used by body cells.
    /// 正文单元格使用的背景填充色。
    pub(super) body_background: Background,
    /// Quote depth copied onto the rendered table.
    /// 拷贝到渲染表格上的引用深度。
    pub(super) quote_depth: usize,
}

/// One rendered table cell node stored inside a table node.
/// 存储在表格节点内部的一个渲染表格单元格节点。
#[derive(Debug, Clone)]
pub(super) struct RenderedTableCellNode {
    /// Paragraph-shaped cell content reused for layout, hit-testing, and selection drawing.
    /// 复用于布局、命中测试与选择绘制的段落形单元格内容。
    pub(super) paragraph: RenderedParagraphNode,
    /// Zero-based row index of the cell.
    /// 当前单元格的零基行索引。
    pub(super) row_index: usize,
    /// Zero-based column index of the cell.
    /// 当前单元格的零基列索引。
    pub(super) column_index: usize,
    /// Alignment requested by the Markdown table syntax.
    /// Markdown 表格语法请求的对齐方式。
    pub(super) alignment: SelectableTableAlignment,
    /// Whether the cell belongs to the header row.
    /// 当前单元格是否属于表头行。
    pub(super) is_header: bool,
}

/// Cached table layout computed during widget layout so draw and hit-test share the same geometry.
/// 在组件布局阶段计算出的缓存表格布局，使绘制与命中测试共享相同几何信息。
#[derive(Debug, Clone, Default)]
pub(super) struct CachedTableLayout {
    /// Paragraph range owned by the table inside the global paragraph cache.
    /// 当前表格在全局段落缓存中拥有的段落范围。
    pub(super) paragraph_range: Range<usize>,
    /// Final column widths resolved for the current available width.
    /// 针对当前可用宽度解析得到的最终列宽集合。
    pub(super) column_widths: Vec<f32>,
    /// Final row heights resolved from the measured cell contents.
    /// 根据测量后的单元格内容解析得到的最终行高集合。
    pub(super) row_heights: Vec<f32>,
    /// Grid line thickness shared by the whole table.
    /// 整张表格共享的网格线厚度。
    pub(super) border_thickness: f32,
}

/// Background treatment applied to one rendered paragraph node.
/// 应用于单个渲染段落节点的背景样式。
#[derive(Debug, Clone, Copy)]
pub(super) struct RenderedParagraphBackground {
    /// Fill background shown behind the paragraph.
    /// 显示在段落背后的填充背景。
    pub(super) background: Background,
    /// Border shown around the paragraph background.
    /// 显示在段落背景周围的边框。
    pub(super) border: Border,
}

/// Horizontal placement used by one rendered paragraph box inside the transcript viewport.
/// 单个渲染段落框在转录视口内部使用的水平放置方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderedParagraphHorizontalPlacement {
    /// Places the paragraph box along the leading edge of the transcript flow.
    /// 将段落框放置在转录流的起始侧。
    Leading,
    /// Places the paragraph box in the horizontal center of the transcript flow.
    /// 将段落框放置在转录流的水平居中位置。
    #[allow(dead_code)]
    Center,
    /// Places the paragraph box along the trailing edge of the transcript flow.
    /// 将段落框放置在转录流的末端侧。
    Trailing,
}

/// Recursive rendering context used when flattening Markdown items.
/// 展平 Markdown 节点时使用的递归渲染上下文。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RenderContext {
    /// Left indentation accumulated by list nesting.
    /// 由列表嵌套累计形成的左侧缩进。
    pub(super) indent_left: f32,
    /// Current quote depth used to draw nested quotation guides.
    /// 用于绘制嵌套引用导线的当前引用深度。
    pub(super) quote_depth: usize,
    /// Optional shared background treatment propagated by semantic blocks such as the inline thinking panel body.
    /// 由内联思考面板正文等语义块向下传播的可选共享背景样式。
    pub(super) block_background: Option<RenderedParagraphBackground>,
    /// Optional shared surface background propagated for semantic containers that need one continuous panel behind multiple child nodes.
    /// 为需要在多个子节点背后保持连续面板的语义容器向下传播的可选共享表面背景。
    pub(super) surface_background: Option<RenderedParagraphBackground>,
    /// Optional shared foreground color propagated by semantic blocks such as the inline thinking panel body.
    /// 由内联思考面板正文等语义块向下传播的可选共享前景色。
    pub(super) foreground_color: Option<Color>,
    /// Shared inner padding propagated to descendant paragraphs so one semantic surface can keep stable content insets.
    /// 向后代段落传播的共享内边距，使单一语义表面可以保持稳定的内容留白。
    pub(super) inner_padding: Padding,
    /// Whether nested Markdown affordances should avoid drawing their own local backgrounds inside the current semantic surface.
    /// 当前语义表面内部的嵌套 Markdown 呈现是否应避免绘制自身局部背景。
    pub(super) suppress_embedded_backgrounds: bool,
}

/// Plain-paragraph style preset used for directive titles and fallback paragraphs.
/// 用于指令标题与回退段落的纯文本段落样式预设。
#[derive(Debug, Clone, Copy)]
pub(super) struct PlainParagraphStyle {
    /// Text size used by the plain paragraph.
    /// 当前纯文本段落使用的文本字号。
    pub(super) text_size: Pixels,
    /// Line height used by the plain paragraph.
    /// 当前纯文本段落使用的行高。
    pub(super) line_height: text::LineHeight,
    /// Wrapping strategy used by the plain paragraph.
    /// 当前纯文本段落使用的换行策略。
    pub(super) wrapping: text::Wrapping,
    /// Left indentation applied to the plain paragraph.
    /// 当前纯文本段落应用的左侧缩进。
    pub(super) indent_left: f32,
    /// Horizontal placement used by the plain paragraph container.
    /// 当前纯文本段落容器使用的水平放置方式。
    pub(super) horizontal_placement: RenderedParagraphHorizontalPlacement,
    /// Top gap placed before the plain paragraph.
    /// 当前纯文本段落之前放置的顶部间距。
    pub(super) gap_before: f32,
    /// Bottom gap placed after the plain paragraph.
    /// 当前纯文本段落之后放置的底部间距。
    pub(super) gap_after: f32,
    /// Optional explicit foreground color.
    /// 可选的显式前景色。
    pub(super) color: Option<Color>,
    /// Span routing branch used by the plain paragraph.
    /// 当前纯文本段落使用的片段路由分支。
    pub(super) render_kind: PlainTextRenderKind,
    /// Whether the plain paragraph background box should shrink to its content width.
    /// 当前纯文本段落背景框是否应收缩到其内容宽度。
    pub(super) fit_width_to_content: bool,
    /// Optional width cap ratio applied to the plain paragraph before wrapping and alignment.
    /// 在换行和对齐前应用到当前纯文本段落上的可选宽度上限比例。
    pub(super) max_width_ratio: Option<f32>,
    /// Optional background treatment for the plain paragraph.
    /// 当前纯文本段落的可选背景样式。
    pub(super) background: Option<RenderedParagraphBackground>,
    /// Optional shared surface background used to derive nested semantic treatments without painting per-paragraph stripes.
    /// 用于派生嵌套语义样式而不绘制逐段条带背景的可选共享表面背景。
    pub(super) surface_background: Option<RenderedParagraphBackground>,
    /// Explicit inner padding applied to the paragraph box even when it does not paint a local background.
    /// 即使当前段落不绘制局部背景，也会应用到段落盒上的显式内边距。
    pub(super) inner_padding: Padding,
    /// Whether nested Markdown affordances should avoid drawing their own local backgrounds inside the current semantic surface.
    /// 当前语义表面内部的嵌套 Markdown 呈现是否应避免绘制自身局部背景。
    pub(super) suppress_embedded_backgrounds: bool,
    /// Quote depth copied onto the rendered paragraph.
    /// 拷贝到渲染段落上的引用深度。
    pub(super) quote_depth: usize,
}

/// Table style preset used by the custom selectable transcript renderer.
/// 自定义可选择转录渲染器使用的表格样式预设。
#[derive(Debug, Clone, Copy)]
pub(super) struct RenderedTableStyle {
    /// Text size used by table cell content.
    /// 表格单元格内容使用的文本字号。
    pub(super) text_size: Pixels,
    /// Foreground color used by table cell content.
    /// 表格单元格内容使用的前景色。
    pub(super) text_color: Color,
    /// Left indentation applied to the whole table.
    /// 应用于整张表格的左侧缩进。
    pub(super) indent_left: f32,
    /// Top gap placed before the table.
    /// 放置在表格之前的顶部间距。
    pub(super) gap_before: f32,
    /// Bottom gap placed after the table.
    /// 放置在表格之后的底部间距。
    pub(super) gap_after: f32,
    /// Cell padding applied to every table cell.
    /// 应用于每个表格单元格的内部边距。
    pub(super) cell_padding: Padding,
    /// Outer horizontal padding applied to the table inside one semantic surface.
    /// 应用于单个语义表面内部表格的外层水平留白。
    pub(super) outer_padding: Padding,
    /// Border color used by the whole table.
    /// 整张表格使用的边框颜色。
    pub(super) border_color: Color,
    /// Header background fill used by the whole table.
    /// 整张表格的表头背景填充色。
    pub(super) header_background: Background,
    /// Body background fill used by the whole table.
    /// 整张表格的正文背景填充色。
    pub(super) body_background: Background,
    /// Quote depth copied onto the table node.
    /// 拷贝到表格节点上的引用深度。
    pub(super) quote_depth: usize,
}

/// Parsed Markdown table payload extracted from source-aware table scanning.
/// 从源码感知表格扫描中提取出的 Markdown 表格载荷。
#[derive(Debug, Clone)]
pub(super) struct ParsedMarkdownTable {
    /// Header cells of the Markdown table.
    /// Markdown 表格的表头单元格集合。
    pub(super) header_cells: Vec<String>,
    /// Alignment rules declared by the Markdown alignment row.
    /// Markdown 对齐行声明的对齐规则集合。
    pub(super) alignments: Vec<SelectableTableAlignment>,
    /// Body rows of the Markdown table.
    /// Markdown 表格的正文行集合。
    pub(super) body_rows: Vec<Vec<String>>,
}

impl PlainParagraphStyle {
    /// Returns one prose-style plain paragraph preset.
    /// 返回一个正文风格的纯文本段落预设。
    pub(super) fn flow(text_size: Pixels) -> Self {
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

/// Measured bounds resolved for one compact collapsible semantic block node.
/// 为一个紧凑折叠语义区块节点解析出的测量边界。
#[derive(Debug, Clone, Copy)]
pub(super) struct MeasuredRenderedCollapsibleDirectiveNode {
    /// Outer bounds occupied by the summary row.
    /// 摘要行占据的外部边界。
    pub(super) summary_outer_bounds: Rectangle,
    /// Optional outer bounds occupied by the expandable body viewport.
    /// 可展开正文视口占据的可选外部边界。
    pub(super) body_outer_bounds: Option<Rectangle>,
    /// Full unclipped text height of the body paragraph before viewport clipping is applied.
    /// 在应用视口裁剪前正文段落的完整未裁剪文本高度。
    pub(super) body_full_text_height: f32,
}

/// Measured bounds resolved for one rendered table node.
/// 为一个渲染表格节点解析出的测量边界。
#[derive(Debug, Clone)]
pub(super) struct MeasuredRenderedTableNode {
    /// Outer bounds occupied by the whole table, excluding the trailing flow gap.
    /// 整张表格占据的外部边界，不包含尾部流式间距。
    pub(super) outer_bounds: Rectangle,
    /// Cell bounds resolved in row-major order.
    /// 按行优先顺序解析得到的单元格边界集合。
    pub(super) cell_bounds: Vec<Rectangle>,
    /// Text origins resolved for every cell in row-major order.
    /// 按行优先顺序为每个单元格解析得到的文本起点集合。
    pub(super) cell_text_origins: Vec<Point>,
    /// Text bounds used when shaping every cell paragraph in row-major order.
    /// 按行优先顺序保存每个单元格段落排版时使用的文本边界。
    pub(super) cell_text_bounds: Vec<Size>,
}

/// Cached local layout metrics for one inline generated-media card flow node.
/// 单个内联生成媒体卡片流式节点的本地布局指标缓存。
#[derive(Debug, Clone, Copy)]
pub(super) struct LaidOutMediaCardNode {
    /// Source document node index owning the current flow layout entry.
    /// 拥有当前流式布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Local outer bounds of the full media card.
    /// 完整媒体卡片的本地外部边界。
    pub(super) outer_bounds: Rectangle,
    /// Local preview region used by image, audio, or video affordances.
    /// 供图片、音频或视频外观使用的本地预览区域。
    pub(super) preview_bounds: Rectangle,
    /// Local title row bounds used when the card keeps transcript-side captions visible.
    /// 当卡片保留转录侧标题时使用的本地标题行边界。
    pub(super) title_bounds: Rectangle,
    /// Local metadata row bounds used when the card keeps transcript-side file labels visible.
    /// 当卡片保留转录侧文件标签时使用的本地元信息行边界。
    pub(super) meta_bounds: Rectangle,
    /// Local bottom edge after the card and its trailing gap.
    /// 包含卡片及其尾部间距的本地底边。
    pub(super) flow_end_y: f32,
}

/// Cached local layout metrics for one background panel node derived from already measured flow children.
/// 根据已测量流式子节点推导得到的单个背景面板节点本地布局指标缓存。
#[derive(Debug, Clone, Copy)]
pub(super) struct LaidOutPanelNode {
    /// Source document node index owning the current panel layout entry.
    /// 拥有当前面板布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Local panel bounds stored relative to the transcript widget origin.
    /// 相对于转录组件原点存储的本地面板边界。
    pub(super) outer_bounds: Rectangle,
}

/// Cached local layout metrics for one visible transcript flow node.
/// 单个可见转录流式节点的本地布局指标缓存。
#[derive(Debug, Clone)]
pub(super) enum LaidOutFlowNode {
    /// Cached layout entry for one collapsible semantic block.
    /// 一个折叠语义区块对应的缓存布局项。
    Collapsible(LaidOutCollapsibleNode),
    /// Cached layout entry for one standalone paragraph.
    /// 一个独立段落对应的缓存布局项。
    Paragraph(LaidOutParagraphNode),
    /// Cached layout entry for one inline generated-media card.
    /// 一个内联生成媒体卡片对应的缓存布局项。
    MediaCard(LaidOutMediaCardNode),
    /// Cached layout entry for one rendered table.
    /// 一张渲染表格对应的缓存布局项。
    Table(LaidOutTableNode),
    /// Cached layout entry for one horizontal rule.
    /// 一条水平分隔线对应的缓存布局项。
    Rule(LaidOutRuleNode),
}

/// Cached local layout metrics for one collapsible semantic flow node.
/// 单个折叠语义流式节点的本地布局指标缓存。
#[derive(Debug, Clone, Copy)]
pub(super) struct LaidOutCollapsibleNode {
    /// Source document node index owning the current flow layout entry.
    /// 拥有当前流式布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Cached paragraph index of the always-visible summary row.
    /// 始终可见摘要行对应的缓存段落索引。
    pub(super) summary_paragraph_index: usize,
    /// Cached paragraph index of the optional body paragraph when it exists.
    /// 可选正文段落存在时对应的缓存段落索引。
    pub(super) body_paragraph_index: Option<usize>,
    /// Local outer bounds of the summary row.
    /// 摘要行的本地外部边界。
    pub(super) summary_outer_bounds: Rectangle,
    /// Local outer bounds of the optional body viewport.
    /// 可选正文视口的本地外部边界。
    pub(super) body_outer_bounds: Option<Rectangle>,
    /// Full unclipped body text height preserved for scrollbar math.
    /// 为滚动条计算保留的正文完整未裁剪文本高度。
    pub(super) body_full_text_height: f32,
    /// Local bottom edge after the full flow contribution, including trailing gap.
    /// 包含尾部间距在内的完整流式贡献本地底边。
    pub(super) flow_end_y: f32,
}

/// Cached local layout metrics for one standalone paragraph flow node.
/// 单个独立段落流式节点的本地布局指标缓存。
#[derive(Debug, Clone, Copy)]
pub(super) struct LaidOutParagraphNode {
    /// Source document node index owning the current flow layout entry.
    /// 拥有当前流式布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Cached paragraph index used to access the renderer paragraph buffer.
    /// 用于访问渲染器段落缓冲区的缓存段落索引。
    pub(super) paragraph_index: usize,
    /// Local outer bounds of the paragraph box.
    /// 当前段落盒子的本地外部边界。
    pub(super) outer_bounds: Rectangle,
    /// Text bounds used when shaping this paragraph.
    /// 对当前段落进行文本排版时使用的文本边界。
    pub(super) text_bounds: Size,
    /// Local bottom edge after the paragraph and its trailing gap.
    /// 包含段落及其尾部间距的本地底边。
    pub(super) flow_end_y: f32,
}

/// Cached local layout metrics for one rendered table flow node.
/// 单个渲染表格流式节点的本地布局指标缓存。
#[derive(Debug, Clone)]
pub(super) struct LaidOutTableNode {
    /// Source document node index owning the current flow layout entry.
    /// 拥有当前流式布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Cached table index used to access table-layout side data.
    /// 用于访问表格布局附属数据的缓存表格索引。
    pub(super) table_index: usize,
    /// Local measured geometry of the full table.
    /// 整张表格的本地测量几何信息。
    pub(super) measured_table: MeasuredRenderedTableNode,
    /// Local bottom edge after the table and its trailing gap.
    /// 包含表格及其尾部间距的本地底边。
    pub(super) flow_end_y: f32,
}

/// Cached local layout metrics for one horizontal-rule flow node.
/// 单个水平分隔线流式节点的本地布局指标缓存。
#[derive(Debug, Clone, Copy)]
pub(super) struct LaidOutRuleNode {
    /// Source document node index owning the current flow layout entry.
    /// 拥有当前流式布局项的源文档节点索引。
    pub(super) node_index: usize,
    /// Local outer bounds of the visible rule body.
    /// 可见分隔线主体的本地外部边界。
    pub(super) outer_bounds: Rectangle,
    /// Local bottom edge after the rule and its trailing gap.
    /// 包含分隔线及其尾部间距的本地底边。
    pub(super) flow_end_y: f32,
}

impl LaidOutFlowNode {
    /// Returns the source document node index bound to the current cached flow layout entry.
    /// 返回绑定到当前缓存流式布局项的源文档节点索引。
    pub(super) fn node_index(&self) -> usize {
        match self {
            Self::Collapsible(node) => node.node_index,
            Self::Paragraph(node) => node.node_index,
            Self::MediaCard(node) => node.node_index,
            Self::Table(node) => node.node_index,
            Self::Rule(node) => node.node_index,
        }
    }

    /// Returns the local top edge used for viewport-range culling.
    /// 返回用于视口范围裁剪的本地顶部边缘。
    pub(super) fn top_y(&self) -> f32 {
        match self {
            Self::Collapsible(node) => node.summary_outer_bounds.y,
            Self::Paragraph(node) => node.outer_bounds.y,
            Self::MediaCard(node) => node.outer_bounds.y,
            Self::Table(node) => node.measured_table.outer_bounds.y,
            Self::Rule(node) => node.outer_bounds.y,
        }
    }

    /// Returns the local bottom edge after the whole flow contribution, including trailing gap.
    /// 返回包含完整流式贡献及尾部间距的本地底部边缘。
    pub(super) fn bottom_y(&self) -> f32 {
        match self {
            Self::Collapsible(node) => node.flow_end_y,
            Self::Paragraph(node) => node.flow_end_y,
            Self::MediaCard(node) => node.flow_end_y,
            Self::Table(node) => node.flow_end_y,
            Self::Rule(node) => node.flow_end_y,
        }
    }
}

/// Flow-prose and code rendering branches used by routed Markdown span construction.
/// 供路由化 Markdown 片段构建使用的正文与代码渲染分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkdownRenderKind {
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
pub(super) enum PlainTextRenderKind {
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
pub(super) enum SelectableTableAlignment {
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
pub(super) enum TranscriptScript {
    /// Latin-like glyphs and neutral punctuation handled by the primary UI family.
    /// 由主界面字体族承载的拉丁类字形与中性标点。
    Latin,
    /// East Asian glyphs that should stay on the bundled CJK fallback family.
    /// 应保持在内置 CJK 兜底字体族上的东亚字形。
    Cjk,
}

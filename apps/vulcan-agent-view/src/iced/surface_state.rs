//! Surface-state and interaction types shared by the selectable transcript shell and its helper modules.
//! 由可选择转录外壳及其辅助模块共享的表面状态与交互类型。

use super::RendererParagraph;
use super::scene::{CachedTableLayout, LaidOutFlowNode, LaidOutPanelNode};
use iced::Point;
use std::collections::BTreeMap;
use std::time::Instant;

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
pub(super) enum TranscriptContextMenuAction {
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
pub(super) struct TranscriptContextMenuState {
    /// Pointer position used to anchor the top-left corner of the context menu after clamping.
    /// 在钳制之后用于锚定右键菜单左上角的指针位置。
    pub(super) anchor_position: Point,
}

/// Runtime state retained by the selectable transcript widget.
/// 可选择转录组件保留的运行时状态。
pub(super) struct SelectableTranscriptState {
    /// Cached paragraphs aligned with text-bearing nodes in the rendered transcript.
    /// 与渲染转录中文本节点对齐的缓存段落集合。
    pub(super) paragraphs: Vec<RendererParagraph>,
    /// Cached table layouts aligned with rendered table nodes in the transcript.
    /// 与转录中渲染表格节点对齐的缓存表格布局集合。
    pub(super) table_layouts: Vec<CachedTableLayout>,
    /// Cached flow-node layout metrics kept in visual order for viewport-range culling and hit testing.
    /// 按可视顺序保留的流式节点布局指标缓存，用于视口裁剪与命中测试。
    pub(super) flow_layouts: Vec<LaidOutFlowNode>,
    /// Cached background-panel layout metrics resolved from the current flow-node geometry.
    /// 根据当前流式节点几何信息解析得到的背景面板布局指标缓存。
    pub(super) panel_layouts: Vec<LaidOutPanelNode>,
    /// Whether the current paragraph and flow-layout caches are valid for the last measured width and document signature.
    /// 当前段落与流式布局缓存是否对上一次测量宽度和文档签名仍然有效。
    pub(super) layout_cache_ready: bool,
    /// Width used by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时使用的宽度。
    pub(super) cached_layout_width: f32,
    /// Stable rendered-document signature used to invalidate cached layout metrics when content or theme changes.
    /// 当内容或主题变化时用于使布局缓存失效的稳定渲染文档签名。
    pub(super) cached_layout_signature: u64,
    /// Height produced by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时产出的高度。
    pub(super) cached_layout_height: f32,
    /// Whether the transcript currently owns keyboard focus for copy and select-all shortcuts.
    /// 当前转录是否拥有键盘焦点，以处理复制和全选快捷键。
    pub(super) focused: bool,
    /// Whether the pointer is actively dragging one transcript selection.
    /// 当前指针是否正在拖拽一个转录选择区域。
    pub(super) dragging: bool,
    /// Whether the current drag interaction has already moved away from its initial anchor.
    /// 当前拖拽交互是否已经离开最初锚点位置。
    pub(super) drag_moved: bool,
    /// Fixed anchor character offset used by the current selection.
    /// 当前选择使用的固定锚点字符偏移。
    pub(super) anchor_char: usize,
    /// Moving head character offset used by the current selection.
    /// 当前选择使用的移动头部字符偏移。
    pub(super) head_char: usize,
    /// Last drag auto-scroll request already published to the parent scroll container.
    /// 已经发布给父级滚动容器的最近一次拖拽自动滚动请求。
    pub(super) drag_auto_scroll_request: Option<SelectableTranscriptDragAutoScroll>,
    /// Last synthetic link target clicked without dragging so double-click intent can be inferred.
    /// 最近一次未拖拽点击的合成链接目标，用于推导双击意图。
    pub(super) last_click_link_target: Option<String>,
    /// Monotonic timestamp captured for the last synthetic link click.
    /// 上一次合成链接点击时捕获的单调时间戳。
    pub(super) last_click_instant: Option<Instant>,
    /// Body scroll offsets retained per collapsible semantic block.
    /// 按折叠语义区块保留的正文滚动偏移量。
    pub(super) collapsible_body_scroll_offsets: BTreeMap<String, f32>,
    /// Currently open lightweight context menu anchored inside the transcript surface.
    /// 当前在转录表面内部打开的轻量右键菜单。
    pub(super) context_menu: Option<TranscriptContextMenuState>,
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

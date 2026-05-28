//! Surface-state and interaction types shared by the selectable transcript shell and its helper modules.
//! 由可选择转录外壳及其辅助模块共享的表面状态与交互类型。

use super::render_cache::{TranscriptRenderCache, stash_surface_render_cache};
use super::scene::RenderedMediaKind;
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

/// Identifies one selectable transcript surface that can receive host-driven menu commands.
/// 标识一个可接收宿主侧菜单命令的可选择转录表面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectableTranscriptSurfaceId(pub String);

/// Describes one host-driven command that should be applied to a selectable transcript surface.
/// 描述一条需要应用到可选择转录表面的宿主侧命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectableTranscriptCommand {
    /// Selects the complete transcript text for the matching surface and revision.
    /// 按匹配的表面与修订号选中完整转录文本。
    SelectAll {
        /// Target surface identifier that should consume this command.
        /// 应消费该命令的目标表面标识。
        surface_id: SelectableTranscriptSurfaceId,
        /// Monotonic revision used to apply the command exactly once per widget state.
        /// 用于让每个组件状态仅应用一次命令的单调修订号。
        revision: u64,
    },
}

/// Media modality exposed by one transcript context-menu target.
/// 单个转录右键菜单目标暴露的媒体模态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectableTranscriptContextMenuMediaKind {
    /// Raster image card.
    /// 光栅图片卡片。
    Image,
    /// Audio playback card.
    /// 音频播放卡片。
    Audio,
    /// Video card.
    /// 视频卡片。
    Video,
    /// PDF document card.
    /// PDF 文档卡片。
    Pdf,
}

/// Host-visible media target resolved while opening a transcript context menu.
/// 打开转录右键菜单时解析出的宿主可见媒体目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectableTranscriptContextMenuMediaTarget {
    /// Media modality represented by the target card.
    /// 目标卡片表示的媒体模态。
    pub kind: SelectableTranscriptContextMenuMediaKind,
    /// Human-readable file name displayed by the card.
    /// 卡片显示的人类可读文件名。
    pub file_name: String,
    /// Local absolute path of the generated media file.
    /// 生成媒体文件的本地绝对路径。
    pub local_path: String,
    /// Whether the target currently represents the active playing audio strip.
    /// 目标当前是否代表正在播放的音频条。
    pub is_active: bool,
}

/// Host-visible transcript context-menu request emitted from a right-click gesture.
/// 从右键手势发出的宿主可见转录右键菜单请求。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectableTranscriptContextMenuRequest {
    /// Surface identifier that should receive follow-up commands from the menu.
    /// 应接收菜单后续命令的表面标识。
    pub surface_id: SelectableTranscriptSurfaceId,
    /// Widget-layer pointer position used as a fallback when the host has no tracked root pointer.
    /// 当宿主没有已跟踪根指针时用作兜底的组件层指针位置。
    pub anchor_position: Point,
    /// Currently selected text, when the right-click occurred over a textual selection.
    /// 右键发生时已有文本选区对应的当前选中文本。
    pub selected_text: Option<String>,
    /// Whether the transcript contains any selectable text at all.
    /// 转录是否包含任何可选择文本。
    pub has_document_text: bool,
    /// Optional generated-media card target under the pointer.
    /// 指针下方的可选生成媒体卡片目标。
    pub media_target: Option<SelectableTranscriptContextMenuMediaTarget>,
}

/// High-level transcript interaction emitted by synthetic chrome spans inside the rendered transcript.
/// 由渲染转录内部合成交互外框片段发出的高层转录交互。
#[derive(Debug, Clone, PartialEq)]
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
    /// Requests host-side editing for the final user-prompt message when the host marks it editable.
    /// 当宿主标记某条最终用户提示词可编辑时，请求宿主侧打开编辑。
    EditUserPrompt(String),
    /// Opens one generated image preview through the host application.
    /// 通过宿主应用打开一张生成图片的预览。
    OpenImagePreview {
        /// Human-readable image file name shown by the host preview header.
        /// 由宿主预览标题栏显示的人类可读图片文件名。
        file_name: String,
        /// Local absolute path of the image file to preview.
        /// 待预览图片文件的本地绝对路径。
        local_path: String,
    },
    /// Toggles host-side audio playback for one generated voice-strip card.
    /// 为某张生成语音条卡片切换宿主侧音频播放状态。
    ToggleAudioPlayback {
        /// Human-readable audio file name used by host status text.
        /// 宿主状态文案使用的人类可读音频文件名。
        file_name: String,
        /// Local absolute path of the audio file to play.
        /// 待播放音频文件的本地绝对路径。
        local_path: String,
    },
    /// Opens one generated media file with the system default application.
    /// 使用系统默认应用打开一个生成媒体文件。
    OpenMediaWithDefaultApp(String),
    /// Reveals one generated media file inside the system file manager.
    /// 在系统文件管理器中定位一个生成媒体文件。
    RevealMediaInFolder(String),
    /// Copies one generated media path through the host application.
    /// 通过宿主应用复制一条生成媒体路径。
    CopyMediaPath(String),
    /// Copies one generated image as bitmap data through the host application.
    /// 通过宿主应用以位图数据形式复制一张生成图片。
    CopyImageToClipboard {
        /// Human-readable image file name used by host status text.
        /// 宿主状态文案使用的人类可读图片文件名。
        file_name: String,
        /// Local absolute path of the image file to copy.
        /// 待复制图片文件的本地绝对路径。
        local_path: String,
    },
    /// Requests opening the shared host-level transcript context menu.
    /// 请求打开宿主层统一转录右键菜单。
    OpenContextMenu(SelectableTranscriptContextMenuRequest),
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
    /// Opens the hovered generated image preview.
    /// 打开当前悬停生成图片的预览。
    PreviewImage,
    /// Toggles playback for the hovered generated audio strip.
    /// 为当前悬停生成语音条切换播放状态。
    ToggleAudioPlayback,
    /// Opens the hovered generated media file with the default application.
    /// 使用默认应用打开当前悬停的生成媒体文件。
    OpenMedia,
    /// Copies the hovered generated media path.
    /// 复制当前悬停生成媒体的路径。
    CopyMediaPath,
    /// Reveals the hovered generated media file in the system file manager.
    /// 在系统文件管理器中定位当前悬停的生成媒体文件。
    RevealMedia,
    /// Copies the hovered generated image into the system clipboard.
    /// 将当前悬停的生成图片复制到系统剪贴板。
    CopyImage,
}

/// Host-side media target resolved from one inline generated-media card.
/// 从单个内联生成媒体卡片解析出的宿主侧媒体目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TranscriptMediaTarget {
    /// Media modality represented by the target card.
    /// 目标卡片表示的媒体模态。
    pub(super) kind: RenderedMediaKind,
    /// Human-readable file name displayed by the card.
    /// 卡片显示的人类可读文件名。
    pub(super) file_name: String,
    /// Local absolute path of the generated media file.
    /// 生成媒体文件的本地绝对路径。
    pub(super) local_path: String,
    /// Whether the target currently represents the active playing audio strip.
    /// 目标当前是否代表正在播放的音频条。
    pub(super) is_active: bool,
}

/// Captures one open transcript context-menu request anchored to a widget-space pointer position.
/// 记录一个锚定到组件空间指针位置的转录右键菜单打开请求。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TranscriptContextMenuState {
    /// Pointer position used to anchor the top-left corner of the context menu after clamping.
    /// 在钳制之后用于锚定右键菜单左上角的指针位置。
    pub(super) anchor_position: Point,
    /// Optional generated-media target hovered when the menu was opened.
    /// 打开菜单时命中的可选生成媒体目标。
    pub(super) media_target: Option<TranscriptMediaTarget>,
}

/// Lightweight hover tooltip payload anchored to one transcript chip such as a per-turn statistics footer.
/// 锚定到某个转录标签（例如按轮次统计页脚）的轻量悬浮提示载荷。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TranscriptHoverTooltipState {
    /// Pointer position localized into the transcript surface and later remapped during overlay placement.
    /// 已局部化到转录表面内部、并会在浮层放置阶段重新映射的指针位置。
    pub(super) anchor_position: Point,
    /// Short title shown at the top of the tooltip body.
    /// 显示在提示正文顶部的简短标题。
    pub(super) title: String,
    /// Multi-line human-readable tooltip body.
    /// 面向用户的多行提示正文。
    pub(super) body: String,
}

/// Runtime state retained by the selectable transcript widget.
/// 可选择转录组件保留的运行时状态。
pub(super) struct SelectableTranscriptState {
    /// Heavy render-cache payload storing paragraphs, table geometry, and flow-layout measurements.
    /// 存储段落、表格几何与流式布局测量结果的重型渲染缓存载荷。
    pub(super) render_cache: TranscriptRenderCache,
    /// Optional surface-scoped cache key currently bound to the heavy render cache so session switching can stash and restore it across widget teardown.
    /// 当前与重型渲染缓存绑定的可选表面级缓存键，使会话切换可以在组件销毁前后暂存并恢复该缓存。
    pub(super) bound_surface_cache_key: Option<String>,
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
    /// Last host-driven context-menu command revision already consumed by this widget state.
    /// 此组件状态已消费的最近一次宿主侧右键菜单命令修订号。
    pub(super) last_context_menu_command_revision: u64,
    /// Optional hover tooltip currently visible for lightweight transcript statistic chips.
    /// 当前为轻量转录统计标签显示的可选悬浮提示。
    pub(super) hover_tooltip: Option<TranscriptHoverTooltipState>,
}

impl Default for SelectableTranscriptState {
    /// Creates one empty selectable transcript state with no active selection.
    /// 创建一个不带活动选择的空可选择转录状态。
    fn default() -> Self {
        Self {
            render_cache: TranscriptRenderCache::default(),
            bound_surface_cache_key: None,
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
            last_context_menu_command_revision: 0,
            hover_tooltip: None,
        }
    }
}

impl Drop for SelectableTranscriptState {
    /// Stashes the heavy render cache when the widget state is torn down so switching away from one session does not discard its warm transcript geometry.
    /// 当组件状态被销毁时暂存重型渲染缓存，使切离某个会话不会丢掉其已热身的转录几何缓存。
    fn drop(&mut self) {
        stash_surface_render_cache(&mut self.render_cache, &mut self.bound_surface_cache_key);
    }
}

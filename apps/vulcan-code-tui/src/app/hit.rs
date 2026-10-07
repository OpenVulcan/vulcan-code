// Mouse hit map recorded by the last frame.
// 上一帧记录的鼠标命中表。
//
// Views register every clickable rectangle with the `Target` it stands for while drawing; input
// resolves a click against the newest registration first, so overlays drawn later win over what
// lies beneath them. Registration goes through `&self` (interior mutability) so read-only views
// can record targets without threading `&mut App` through every renderer.
// 视图在绘制时登记每个可点击矩形及其对应的 `Target`；输入处理优先匹配最新登记的项，因此后绘制的
// 浮层会覆盖其下方的区域。登记通过 `&self`（内部可变性）完成，使只读视图无需层层传递
// `&mut App` 也能记录目标。

use std::cell::RefCell;

use ratatui::layout::{Position, Rect};

use super::action::UiAction;
use super::overlay::{CommitField, HistoryFilter};
use super::pages::Area;
use super::{Focus, PanelTab, SidebarRow};

/// What a click on a registered rectangle means.
/// 点击某个已登记矩形的含义。
#[derive(Debug, Clone)]
pub enum Target {
    /// Full-screen layer under a modal; clicking it closes the modal.
    /// 模态框下方的全屏层；点击即关闭模态框。
    Backdrop,
    /// Swallows clicks inside a modal frame that hit nothing else.
    /// 吞掉模态框内未命中其他目标的点击。
    Inert,
    Focus(Focus),
    AreaTab(Area),
    PanelTab(PanelTab),
    Sidebar(SidebarRow),
    GitFile(usize),
    TerminalSession(usize),
    /// Selectable row of a scrolling panel tab (memory item, profile node, agent).
    /// 可滚动面板标签页的可选行（记忆条目、画像节点、代理）。
    PanelItem(PanelTab, usize),
    /// The transcript body; the clicked line is resolved from the recorded layout.
    /// 会话记录主体；点击的行由记录的排版解析。
    Transcript,
    ThinkingBox,
    AskOption(usize),
    AskOther,
    AskQuestion(usize),
    NavItem(usize),
    PageRow(usize),
    PageRowAction(usize, usize),
    PickerItem(usize),
    MenuItem(usize),
    FolderEntry(usize),
    CommitField(CommitField),
    HistoryItem(usize),
    HistoryFilter(HistoryFilter),
    SlashItem(usize),
    /// Performs an action directly (buttons).
    /// 直接执行一个动作（按钮）。
    Do(UiAction),
}

/// What the mouse wheel scrolls over a registered rectangle.
/// 鼠标滚轮在某个已登记矩形上滚动的对象。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Transcript,
    Sidebar,
    Nav,
    Page,
    Panel,
    Overlay,
}

#[derive(Default)]
pub struct HitMap {
    targets: RefCell<Vec<(Rect, Target)>>,
    scrolls: RefCell<Vec<(Rect, ScrollTarget)>>,
    /// Transcript geometry: area, first visible line, items `(seq, first line, line count)` and
    /// total line count.
    /// 会话记录几何信息：区域、首个可见行、条目 `(seq, 首行, 行数)` 与总行数。
    pub transcript: Rect,
    pub transcript_top: usize,
    pub transcript_items: Vec<(u64, usize, usize)>,
    pub transcript_lines: usize,
    /// Where transcript lines are drawn (the area minus its selection gutter) and the lines of the
    /// last layout, so a drag can be mapped to text and copied.
    /// 会话记录行的绘制区域（区域减去选择栏）及最近一次排版的各行，用于把拖动映射为文本并复制。
    pub transcript_content: Rect,
    pub transcript_text: Vec<ratatui::text::Line<'static>>,
    /// Visible rows of the current page and of the sidebar list, for paging and wheel limits.
    /// 当前页面与侧栏列表的可见行数，用于翻页与滚轮边界。
    pub page_height: u16,
    pub sidebar_height: u16,
}

impl HitMap {
    /// Starts a new frame: forgets every registration from the previous one.
    /// 开始新的一帧：清除上一帧的全部登记。
    pub fn reset(&mut self) {
        self.targets.get_mut().clear();
        self.scrolls.get_mut().clear();
    }

    pub fn add(&self, rect: Rect, target: Target) {
        if rect.width > 0 && rect.height > 0 {
            self.targets.borrow_mut().push((rect, target));
        }
    }

    pub fn add_scroll(&self, rect: Rect, target: ScrollTarget) {
        if rect.width > 0 && rect.height > 0 {
            self.scrolls.borrow_mut().push((rect, target));
        }
    }

    /// The newest target under `at`.
    /// `at` 处最新登记的目标。
    pub fn hit(&self, at: Position) -> Option<Target> {
        self.targets
            .borrow()
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(at))
            .map(|(_, target)| target.clone())
    }

    pub fn scroll_at(&self, at: Position) -> Option<ScrollTarget> {
        self.scrolls
            .borrow()
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(at))
            .map(|(_, target)| *target)
    }

    /// Rectangle of the first target matching `pred`, for tests.
    /// 首个满足 `pred` 的目标矩形，用于测试。
    #[cfg(test)]
    pub fn find(&self, pred: impl Fn(&Target) -> bool) -> Option<Rect> {
        self.targets
            .borrow()
            .iter()
            .find(|(_, target)| pred(target))
            .map(|(rect, _)| *rect)
    }
}

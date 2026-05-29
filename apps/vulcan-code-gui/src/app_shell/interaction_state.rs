//! Workbench interaction state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作台交互状态。

use super::*;

/// Hover state describing the active visual line in one diff hunk.
/// 描述某个 diff hunk 中当前激活可视行的悬停状态。
#[derive(Debug, Clone)]
pub(super) struct HoveredDiffLine {
    /// Stable hunk buffer key owning the hovered visual line.
    /// 拥有当前悬停可视行的稳定 hunk 缓冲区键。
    pub(super) file_key: String,
    /// Zero-based visual line index inside the current diff hunk body.
    /// 当前 diff hunk 主体内从零开始的可视行索引。
    pub(super) line_index: usize,
}

/// Latest pointer position tracked inside one custom title bar drag region.
/// 在某个自定义标题栏拖拽区域内追踪到的最近鼠标位置。
#[derive(Debug, Clone, Copy)]
pub(super) struct WindowTitleBarPointer {
    /// Window identifier owning the tracked title bar pointer position.
    /// 拥有当前标题栏鼠标位置的窗口标识。
    pub(super) window_id: window::Id,
    /// Latest pointer position relative to the title bar drag region.
    /// 相对于标题栏拖拽区域的最近鼠标位置。
    pub(super) position: iced::Point,
}

/// Pending drag candidate captured from one custom title bar press.
/// 从某次自定义标题栏按下动作中捕获的待确认拖动候选状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingWindowTitleBarDrag {
    /// Window identifier targeted by the future native drag request.
    /// 未来原生拖动请求将作用到的窗口标识。
    pub(super) window_id: window::Id,
    /// Pointer origin captured when the title bar was pressed.
    /// 标题栏被按下时捕获到的鼠标起始位置。
    pub(super) origin: iced::Point,
}

/// Pending resize candidate captured from the main task drawer divider.
/// 从主任务抽屉分隔柄捕获的待确认缩放候选状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingMainTaskPanelResize {
    /// Cursor x position captured when the drawer resize interaction started.
    /// 任务抽屉缩放交互开始时捕获到的鼠标横向位置。
    pub(super) origin_x: f32,
    /// Drawer width captured when the resize interaction started.
    /// 任务抽屉缩放交互开始时捕获到的抽屉宽度。
    pub(super) initial_width: f32,
}

/// Pending resize candidate captured from the right special-feature panel divider.
/// 从右侧特殊功能面板分隔柄捕获的待确认缩放候选状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingSessionFeaturePanelResize {
    /// Cursor x position captured when the panel resize interaction started.
    /// 面板缩放交互开始时捕获到的鼠标横向位置。
    pub(super) origin_x: f32,
    /// Panel width captured when the resize interaction started.
    /// 面板缩放交互开始时捕获到的面板宽度。
    pub(super) initial_width: f32,
}

/// Pending interaction candidate captured from one project-row press inside the task tree.
/// 从任务树某个项目行按下动作中捕获的待判定交互候选状态。
#[derive(Debug, Clone)]
pub(super) struct PendingTaskProjectInteraction {
    /// Stable identifier of the pressed project row.
    /// 当前被按下项目行的稳定标识。
    pub(super) project_id: String,
    /// Cursor origin captured when the interaction started.
    /// 交互开始时捕获到的鼠标起始位置。
    pub(super) origin: iced::Point,
}

/// Pending drag candidate captured from one pending-prompt queue row press.
/// 从某次待发送队列行按下动作中捕获的待确认拖动候选状态。
#[derive(Debug, Clone)]
pub(super) struct PendingPendingPromptDrag {
    /// Stable root-session identifier that owns the queue being reordered.
    /// 拥有当前重排队列的稳定根会话标识。
    pub(super) session_id: SessionId,
    /// Stable identifier of the pending-prompt queue row being dragged.
    /// 当前被拖拽的待发送队列行稳定标识。
    pub(super) entry_id: String,
    /// Cursor origin captured when the drag candidate was pressed.
    /// 按下拖拽候选时捕获到的鼠标起始位置。
    pub(super) origin: iced::Point,
}

/// Pending drag candidate captured from one AI capability route-target press.
/// 从某次 AI 能力路由目标按下动作中捕获的待确认拖动候选状态。
#[derive(Debug, Clone)]
pub(super) struct PendingAiCapabilityRouteDrag {
    /// Semantic capability owning the route currently being reordered.
    /// 当前正在重排路由所属的语义能力。
    pub(super) capability_kind: AiToolKind,
    /// Stable identifier of the capability target being dragged.
    /// 当前被拖拽的能力目标稳定标识。
    pub(super) target_selector: String,
    /// Cursor origin captured when the drag candidate was pressed.
    /// 按下拖拽候选时捕获到的鼠标起始位置。
    pub(super) origin: iced::Point,
}

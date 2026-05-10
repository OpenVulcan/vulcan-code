//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。

use crate::agent_profile::{self, AgentProfileCatalog, SYSTEM_DEFAULT_AGENT_CHOICE_ID};
use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::{
    self, ExecutionContextMessage, ExecutionPreparation, ExecutionPreparationRequest,
    ExecutionRunReport, ExecutionStreamEvent, RuntimeFoundation, RuntimeSkillKind,
    stream_runtime_run,
};
use crate::agent_view::{
    agent_view_block_collapsed_by_default, agent_view_block_id, agent_view_document_raw_source,
    build_agent_view_document,
};
use crate::code_view;
use crate::data_plane::{self, ControllerBootstrapReport, ControllerRuntimeHandle};
use crate::diff::{DiffFile, WorkspaceDiffPanel};
use crate::luaskills_runtime::{
    self, LuaSkillsBootstrapReport, LuaSkillsCatalog, LuaSkillsCatalogSkill,
    LuaSkillsInstallSourceChoice, LuaSkillsManagementAction, LuaSkillsManagementReport,
    LuaSkillsRuntimeHandle,
};
use crate::mcp::{McpRuntimeHandle, bootstrap_mcp_runtime};
use crate::model::{
    ActivityEntry, ActivityLevel, ExecutionMode, PaneKind, SessionCompactionEntry,
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTitleSource, TaskProjectSummary,
};
use crate::provider_config::{
    self, ModelChoice, PersistedControllerConfiguration, PersistedMcpServerEntry,
    PersistedMcpTransportKind, PersistedProviderEntry, PersistedProviderOptions, ProviderChoice,
    ProviderConfigurationFile, ProviderFieldKey,
};
use crate::selectable_transcript::{
    SelectableTranscriptDragAutoScroll, SelectableTranscriptDragAutoScrollDirection,
    SelectableTranscriptInteraction, view_selectable_rendered_transcript,
};
use crate::skills::{
    CompactionSkillInput, ExploreSkillInput, SessionDerivedRecord, SessionDerivedSkillChain,
    SummarySkillInput, TitleAgentInput,
};
use crate::style;
use crate::theme;
use crate::wheel_passthrough::WheelPassthrough;
use crate::workspace_store::{self, WorkspaceSnapshot};
use iced::widget::operation;
use iced::widget::pane_grid::{self, Configuration, PaneGrid};
use iced::widget::{
    button, checkbox, column, container, markdown, mouse_area, pick_list, progress_bar, radio, row,
    rule, scrollable, slider, space, stack, svg, text, text_editor, text_input, toggler, tooltip,
};
use iced::window;
use iced::{
    Center, Element, Fill, FillPortion, Pixels, Shrink, Subscription, Task, Theme, keyboard, task,
};
use iced_aw::ContextMenu;
use iced_fonts::{CODICON_FONT_BYTES, LUCIDE_FONT_BYTES, codicon, lucide};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use vulcan_agent_view::{
    THINKING_MODAL_ANIMATION_DURATION, THINKING_MODAL_TICK_INTERVAL, ThinkingModalPhase,
    build_modal_thinking_surface_document, build_overlay_thinking_surface_document,
    build_thinking_title, format_thinking_duration_label, thinking_modal_progress,
    thinking_overlay_scrollable_id, view_thinking_modal_surface, view_thinking_overlay_surface,
};

/// Bundled SVG mark bytes used in the header.
/// 头部区域使用的内置 SVG 标识字节。
const LOGO_ASSET_BYTES: &[u8] = include_bytes!("../resources/vulcan-mark.svg");

/// Markdown quick guide rendered in the runtime inspector.
/// 渲染在运行检查面板中的 Markdown 快速指引。
const QUICK_GUIDE_MARKDOWN: &str = r#"
# AI Coding Foundation

- 左侧用于管理工作区、扫描项目与会话队列。
- 中间用于输入 Prompt、组织执行模式并预览补丁代码。
- 右侧用于调整模型、运行策略、主题与工程检查项。

## 下一阶段建议

1. 接入文件树与差异面板。
2. 接入真实命令执行与终端流。
3. 接入流式推理与多 Agent 编排。
"#;

/// Seed Rust code rendered in the embedded editor component.
/// 展示在嵌入式编辑器中的示例 Rust 代码。
const SAMPLE_EDITOR_CONTENT: &str = r#"pub struct AgentRuntime {
    pub workspace: String,
    pub active_model: String,
}

impl AgentRuntime {
    pub async fn bootstrap(&self) -> anyhow::Result<()> {
        println!("bootstrapping {}", self.workspace);
        Ok(())
    }
}
"#;

/// Shared text size used by both the home-page editor and the diff editor.
/// 首页编辑器与 diff 编辑器共用的字号。
const CODE_EDITOR_TEXT_SIZE: f32 = 16.0;

/// Shared absolute line height used by both the home-page editor and the diff editor.
/// 首页编辑器与 diff 编辑器共用的绝对行高。
const CODE_EDITOR_LINE_HEIGHT: f32 = 20.0;

/// Shared fixed height used by each diff file header row.
/// 每个 diff 文件头部行使用的共享固定高度。
const DIFF_FILE_HEADER_HEIGHT: f32 = 50.0;

/// Shared compact height used by the external current-file bar above the diff review flow.
/// Diff 审查流上方外置当前文件条使用的共享紧凑高度。
const DIFF_CURRENT_FILE_BAR_HEIGHT: f32 = 42.0;

/// Shared fixed height used by the custom undecorated window title bar.
/// 自定义无系统装饰窗口标题栏使用的共享固定高度。
const WINDOW_TITLE_BAR_HEIGHT: f32 = 40.0;

/// Shared movement threshold used before one pressed title bar turns into a native drag action.
/// 某次标题栏按下在转化为原生拖动动作前需要达到的共享移动阈值。
const WINDOW_TITLE_BAR_DRAG_THRESHOLD: f32 = 4.0;

/// Shared fixed width used by the vertical activity bar that switches top-level panels.
/// 用于切换顶层面板的竖向活动条使用的共享固定宽度。
const MAIN_PANEL_ACTIVITY_BAR_WIDTH: f32 = 42.0;

/// Shared default width used by the left task drawer in the main workbench panel.
/// 主工作台面板左侧任务抽屉使用的共享默认宽度。
const MAIN_TASK_PANEL_DEFAULT_WIDTH: f32 = 340.0;

/// Shared minimal width used by the left task drawer in the main workbench panel.
/// 主工作台面板左侧任务抽屉使用的共享最小宽度。
const MAIN_TASK_PANEL_MIN_WIDTH: f32 = 300.0;

/// Shared maximal width used by the left task drawer in the main workbench panel.
/// 主工作台面板左侧任务抽屉使用的共享最大宽度。
const MAIN_TASK_PANEL_MAX_WIDTH: f32 = 520.0;

/// Shared width used by the drag handle between the main task drawer and the workbench canvas.
/// 主任务抽屉与工作台画布之间拖拽分隔柄使用的共享宽度。
const MAIN_TASK_PANEL_RESIZE_HANDLE_WIDTH: f32 = 14.0;

/// Shared width used by the subtle restore rail shown when the main task drawer is collapsed.
/// 主任务抽屉折叠后显示的轻量恢复导轨使用的共享宽度。
const MAIN_TASK_PANEL_RESTORE_RAIL_WIDTH: f32 = 14.0;

/// Shared height used by the compact fold and restore toggle button attached to the task drawer edge.
/// 附着在任务抽屉边缘上的紧凑折叠与恢复按钮使用的共享高度。
const MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT: f32 = 20.0;

/// Shared width used by the compact fold and restore toggle button attached to the task drawer edge.
/// 附着在任务抽屉边缘上的紧凑折叠与恢复按钮使用的共享宽度。
const MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH: f32 = 14.0;

/// Shared square size used by each icon button inside the vertical activity bar.
/// 竖向活动条内部每个图标按钮使用的共享方形尺寸。
const MAIN_PANEL_ACTIVITY_BUTTON_SIZE: f32 = 32.0;

/// Shared icon size used by each activity-bar navigation glyph.
/// 每个活动条导航图标使用的共享尺寸。
const MAIN_PANEL_ACTIVITY_ICON_SIZE: f32 = 22.0;

/// Shared fixed width reserved by the project-row action button cluster.
/// 项目行右侧动作按钮簇预留的共享固定宽度。
const MAIN_TASK_PROJECT_ACTIONS_WIDTH: f32 = 18.0;

/// Shared width reserved by the project-row fold arrow at the far right edge.
/// 项目行最右侧折叠箭头预留的共享宽度。
const MAIN_TASK_PROJECT_ARROW_WIDTH: f32 = 14.0;

/// Shared width reserved by one session-row trailing slot for compact time labels or hover actions.
/// 单条会话行尾部为紧凑时间标签或悬停动作预留的共享宽度。
const MAIN_TASK_SESSION_TRAILING_WIDTH: f32 = 40.0;

/// Shared fixed height used by one project row inside the main task drawer tree.
/// 主任务抽屉树内单条项目行使用的共享固定高度。
const MAIN_TASK_PROJECT_ROW_HEIGHT: f32 = 30.0;

/// Shared fixed height used by one session row inside the main task drawer tree.
/// 主任务抽屉树内单条会话行使用的共享固定高度。
const MAIN_TASK_SESSION_ROW_HEIGHT: f32 = 28.0;

/// Shared horizontal padding used by project rows and tree section headers.
/// 项目行与树分区头共用的水平内边距。
const MAIN_TASK_TREE_SECTION_LEFT_PADDING: f32 = 11.0;

/// Shared bottom-gap threshold used to decide whether the transcript is still considered pinned to the latest output.
/// 用于判断转录面板是否仍应视为贴住最新输出的共享底部间隙阈值。
const TRANSCRIPT_FOLLOW_TAIL_BOTTOM_GAP_THRESHOLD: f32 = 28.0;

/// Shared bottom spacer kept at the tail of transcript surfaces so the newest content does not stick to the bottom edge.
/// 保留在转录表面尾部的共享底部留白，用于避免最新内容紧贴底边。
const TRANSCRIPT_BOTTOM_SPACER_HEIGHT: f32 = 72.0;

/// Shared delay used to distinguish one confirmed single click from one double click on the inline thinking header.
/// 用于区分内联思考标题确认单击与双击的共享延迟。
const THINKING_INLINE_TOGGLE_DELAY: Duration = Duration::from_millis(180);

/// Shared animation tick used by drag-driven transcript auto-scroll while the pointer is kept outside the visible viewport.
/// 当指针持续停留在可见视口之外时，拖拽驱动的转录自动滚动使用的共享动画 Tick。
const TRANSCRIPT_DRAG_AUTO_SCROLL_TICK_INTERVAL: Duration = Duration::from_millis(16);

/// Shared pointer overshoot dead zone used before drag auto-scroll begins accelerating the transcript viewport.
/// 在拖拽自动滚动开始加速转录视口前使用的共享指针越界死区。
const TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE: f32 = 8.0;

/// Shared maximal pointer overshoot distance taken into account when computing drag auto-scroll speed.
/// 计算拖拽自动滚动速度时纳入考虑的共享最大指针越界距离。
const TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_OVERSHOOT: f32 = 180.0;

/// Shared minimal scroll step applied on each drag auto-scroll tick once the pointer has left the dead zone.
/// 当指针离开死区后，每次拖拽自动滚动 Tick 应用的共享最小滚动步长。
const TRANSCRIPT_DRAG_AUTO_SCROLL_MIN_STEP: f32 = 4.0;

/// Shared maximal scroll step applied on each drag auto-scroll tick when the pointer is far outside the viewport.
/// 当指针远离视口边界时，每次拖拽自动滚动 Tick 应用的共享最大滚动步长。
const TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_STEP: f32 = 32.0;

/// Shared square size used by compact task-tree hover action buttons.
/// 任务树紧凑悬停动作按钮使用的共享方形尺寸。
const MAIN_TASK_TREE_ACTION_BUTTON_SIZE: f32 = 18.0;

/// Shared width used by each custom window control button.
/// 每个自定义窗口控制按钮使用的共享宽度。
const WINDOW_CONTROL_BUTTON_WIDTH: f32 = 46.0;

/// Shared icon size used by custom window control glyphs.
/// 自定义窗口控制图标使用的共享尺寸。
const WINDOW_CONTROL_ICON_SIZE: f32 = 18.0;

/// Shared thickness used by invisible resize handles around undecorated windows.
/// 无系统装饰窗口四周不可见缩放热区使用的共享厚度。
const WINDOW_RESIZE_HANDLE_THICKNESS: f32 = 6.0;

/// Shared fold-arrow text size used by diff file headers and the current-file bar.
/// Diff 文件头与当前文件条共用的折叠箭头字号。
const DIFF_FOLD_ARROW_SIZE: f32 = 16.0;

/// Shared spacing inserted between file cards in the review flow.
/// 审查流中文件卡片之间插入的共享间距。
const DIFF_FILE_CARD_SPACING: f32 = 12.0;

/// Shared height used when an expanded file has no textual diff body to render.
/// 当展开文件没有可渲染文本 diff 主体时使用的共享高度。
const DIFF_EMPTY_BODY_HEIGHT: f32 = 40.0;

/// Shared gutter width used by merged diff line numbers.
/// 合并后 diff 行号使用的共享 gutter 宽度。
const DIFF_GUTTER_WIDTH: f32 = 54.0;

/// Shared arrow column width used by the hovered-line indicator in the diff gutter.
/// Diff gutter 中悬停行指示箭头使用的共享列宽。
const DIFF_GUTTER_ARROW_WIDTH: f32 = 12.0;

/// Shared horizontal padding used inside diff code editor containers.
/// Diff 代码编辑器容器内部使用的共享水平内边距。
const DIFF_CODE_EDITOR_HORIZONTAL_PADDING: u16 = 8;

/// Shared glyph-width ratio used to estimate wrapped code columns for monospace diff rendering.
/// 用于估算等宽 Diff 渲染折行列数的共享字宽比例。
const DIFF_WRAP_GLYPH_WIDTH_RATIO: f32 = 0.56;

/// Shared minimal wrapped column count used to avoid over-fragmenting narrow diff panes.
/// 用于避免窄 Diff 面板过度碎裂的共享最小折行列数。
const DIFF_MIN_WRAP_COLUMNS: usize = 24;

/// Shared horizontal frame padding reserved by top-level windows before pane content begins.
/// 顶层窗口在面板内容开始前预留的共享水平框架内边距。
const WINDOW_FRAME_HORIZONTAL_PADDING: f32 = 32.0;

/// Shared horizontal padding consumed by the diff file code container before hunk blocks begin.
/// Diff 文件代码容器在 hunk 区块开始前消耗的共享水平内边距。
const DIFF_FILE_CODE_HORIZONTAL_PADDING: f32 = 20.0;

/// Shared extra width budget reserved for borders and scrollbars when estimating diff editor columns.
/// 估算 Diff 编辑器列数时为边框和滚动条预留的共享额外宽度预算。
const DIFF_WRAP_RESERVED_WIDTH: f32 = 28.0;

/// Shared fixed height used by omission cards inserted between separated diff hunks.
/// 插入在分离 diff hunk 之间的省略卡片使用的共享固定高度。
const DIFF_GAP_BLOCK_HEIGHT: f32 = 20.0;

/// Shared spacing used between adjacent diff hunk and gap blocks inside one file body.
/// 单个文件主体内部相邻 diff hunk 与省略块之间使用的共享间距。
const DIFF_HUNK_FLOW_SPACING: f32 = 1.0;

/// Shared vertical padding contributed by the file-level diff body container.
/// 文件级 diff 主体容器贡献的共享垂直内边距。
const DIFF_FILE_BODY_VERTICAL_PADDING: f32 = 16.0;

/// Shared top and bottom inset used inside one expanded diff file body container.
/// 单个展开 Diff 文件主体容器内上下共用的边距。
const DIFF_FILE_BODY_VERTICAL_INSET: f32 = DIFF_FILE_BODY_VERTICAL_PADDING / 2.0;

/// Shared vertical overscan budget used by file-card virtualization.
/// 文件卡片虚拟化使用的共享纵向预渲染预算。
const DIFF_FILE_VIRTUAL_OVERSCAN: f32 = 640.0;

/// Shared visual-line overscan count used by hunk-level virtualization.
/// Hunk 级虚拟化使用的共享可视行预渲染数量。
const DIFF_HUNK_VIRTUAL_OVERSCAN_LINES: usize = 48;

/// Stable scrollable identifier used by the structured diff review flow.
/// 结构化 diff 审查流使用的稳定滚动容器标识。
const DIFF_PANEL_SCROLLABLE_ID: &str = "vulcan-code.diff-panel-scrollable";

/// Launches the GUI application using the official iced crate.
/// 使用官方 iced crate 启动图形界面应用。
pub fn run() -> iced::Result {
    iced::daemon(
        VulcanWorkbench::new,
        VulcanWorkbench::update,
        VulcanWorkbench::view,
    )
    .subscription(VulcanWorkbench::subscription)
    .title(VulcanWorkbench::title)
    .theme(VulcanWorkbench::theme)
    .default_font(theme::ui_font())
    .font(theme::ui_font_bytes())
    .font(theme::ui_italic_font_bytes())
    .font(theme::ui_medium_font_bytes())
    .font(theme::ui_medium_italic_font_bytes())
    .font(theme::ui_bold_font_bytes())
    .font(theme::ui_bold_italic_font_bytes())
    .font(theme::ui_cjk_font_bytes())
    .font(theme::code_font_bytes())
    .font(theme::code_italic_font_bytes())
    .font(theme::cjk_code_font_bytes())
    .font(theme::cjk_code_italic_font_bytes())
    .font(CODICON_FONT_BYTES)
    .font(LUCIDE_FONT_BYTES)
    .run()
}

/// Root state for the AI coding desktop workbench.
/// AI 编程桌面工作台的根状态。
pub struct VulcanWorkbench {
    /// Main workbench window identifier.
    /// 主工作台窗口标识。
    main_window_id: Option<window::Id>,
    /// Dedicated diff preview window identifier.
    /// 独立 Diff 预览窗口标识。
    diff_window_id: Option<window::Id>,
    /// Current size of the main workbench window.
    /// 当前主工作台窗口尺寸。
    main_window_size: iced::Size,
    /// Whether the main workbench window is currently maximized.
    /// 当前主工作台窗口是否处于最大化状态。
    main_window_maximized: bool,
    /// Current size of the dedicated diff preview window.
    /// 当前独立 Diff 预览窗口尺寸。
    diff_window_size: iced::Size,
    /// Whether the dedicated diff preview window is currently maximized.
    /// 当前独立 Diff 预览窗口是否处于最大化状态。
    diff_window_maximized: bool,
    /// Latest cursor position tracked inside the main workbench window.
    /// 最近一次在主工作台窗口中追踪到的鼠标位置。
    main_window_cursor_position: Option<iced::Point>,
    /// Latest cursor position tracked inside one custom title bar drag region.
    /// 最近一次在某个自定义标题栏拖拽区域内记录到的鼠标位置。
    title_bar_pointer_position: Option<WindowTitleBarPointer>,
    /// Pending custom title bar drag candidate waiting for enough pointer movement.
    /// 等待足够鼠标位移后再触发的自定义标题栏拖拽候选状态。
    pending_title_bar_drag: Option<PendingWindowTitleBarDrag>,
    /// Current width used by the left task drawer shown inside the main workbench panel.
    /// 当前主工作台面板左侧任务抽屉使用的宽度。
    main_task_panel_width: f32,
    /// Whether the left task drawer in the main workbench panel is currently expanded.
    /// 当前主工作台面板左侧任务抽屉是否处于展开状态。
    main_task_panel_expanded: bool,
    /// Pending resize candidate for the left task drawer in the main workbench panel.
    /// 主工作台面板左侧任务抽屉的待确认缩放候选状态。
    pending_main_task_panel_resize: Option<PendingMainTaskPanelResize>,
    /// Whether the task-drawer resize handle is currently hovered.
    /// 任务抽屉缩放柄当前是否处于悬停状态。
    main_task_panel_resize_handle_hovered: bool,
    /// Pane grid state describing the high-level application layout.
    /// 描述应用高层布局的分栏网格状态。
    panes: pane_grid::State<PaneKind>,
    /// Currently focused pane identifier.
    /// 当前获得焦点的面板标识。
    focus: Option<pane_grid::Pane>,
    /// Active workspace path shown in navigation.
    /// 展示在导航区的当前工作区路径。
    workspace_path: String,
    /// Prompt composer value used before execution.
    /// 执行前用于组织任务的提示词输入值。
    composer: String,
    /// Text editor content for patch and scaffold previews.
    /// 用于补丁与骨架预览的文本编辑器内容。
    editor_content: text_editor::Content,
    /// Summary text describing the latest loaded diff preview.
    /// 描述最近一次已载入 diff 预览的摘要文本。
    diff_summary: String,
    /// Structured diff panel loaded from the workspace scan action.
    /// 通过工作区扫描动作载入的结构化 diff 面板数据。
    diff_panel: WorkspaceDiffPanel,
    /// Collapsed diff file keys in the review panel.
    /// 审查面板中已折叠的 diff 文件键集合。
    collapsed_diff_files: BTreeSet<String>,
    /// Read-only text editor buffers used by diff file editors.
    /// Diff 文件编辑器使用的只读文本编辑器缓冲区。
    diff_code_buffers: BTreeMap<String, text_editor::Content>,
    /// Cached wrapped hunk payloads reused by virtualized diff rendering.
    /// 供虚拟化 Diff 渲染复用的已缓存折行 hunk 载荷。
    diff_wrapped_hunks: BTreeMap<String, code_view::WrappedDiffHunk>,
    /// Visible visual-line slices currently materialized into read-only editor buffers.
    /// 当前已物化为只读编辑器缓冲区的可视行切片范围。
    diff_buffer_slices: BTreeMap<String, DiffVisualSliceRange>,
    /// Currently hovered visual line inside one diff hunk body.
    /// 当前在某个 diff hunk 主体中悬停的可视行。
    hovered_diff_line: Option<HoveredDiffLine>,
    /// Current vertical scroll offset captured from the diff review flow.
    /// 从 diff 审查流捕获的当前纵向滚动偏移量。
    diff_scroll_offset: f32,
    /// Current viewport height captured from the diff review flow.
    /// 从 diff 审查流捕获的当前视口高度。
    diff_viewport_height: f32,
    /// Selectable Markdown buffer rendered in the inspector guide.
    /// 渲染在检查面板指引中的可选 Markdown 缓冲区。
    quick_guide_markdown_buffer: text_editor::Content,
    /// Session summaries rendered in the navigator pane.
    /// 渲染在导航面板中的会话摘要列表。
    sessions: Vec<SessionSummary>,
    /// Project nodes rendered by the main task-drawer tree.
    /// 由主任务抽屉树渲染的项目节点集合。
    task_projects: Vec<TaskProjectSummary>,
    /// Archived project nodes retained outside the visible tree for later restoration.
    /// 保留在可见树之外、供后续恢复使用的归档项目节点集合。
    archived_task_projects: Vec<TaskProjectSummary>,
    /// Expanded project identifiers inside the main task-drawer tree.
    /// 主任务抽屉树内部已展开的项目标识集合。
    expanded_task_projects: BTreeSet<String>,
    /// Project identifier currently hovered inside the main task-drawer tree.
    /// 当前在主任务抽屉树中悬停的项目标识。
    hovered_task_project: Option<String>,
    /// Session identifier currently hovered inside the main task-drawer tree.
    /// 当前在主任务抽屉树中悬停的会话标识。
    hovered_task_session: Option<usize>,
    /// Current drop target preview shown while one project card is being reordered.
    /// 当前在项目卡片重排期间展示的插入目标预览。
    active_task_project_drop_target: Option<TaskProjectDropTarget>,
    /// Whether session hover should stay suppressed until the cursor fully leaves the list once.
    /// 当前是否应持续抑制会话悬停，直到鼠标完整离开列表一次为止。
    suppress_task_session_hover: bool,
    /// Pending click-or-drag interaction captured from one project row press.
    /// 从某次项目行按下动作中捕获的待判定点击或拖拽交互状态。
    pending_task_project_interaction: Option<PendingTaskProjectInteraction>,
    /// Project identifier currently being reordered through drag interaction.
    /// 当前正在通过拖拽交互调整顺序的项目标识。
    dragging_task_project: Option<String>,
    /// Currently selected session identifier.
    /// 当前被选中的会话标识。
    selected_session: usize,
    /// Runtime activity log rendered in the inspector pane.
    /// 渲染在检查面板中的运行活动日志。
    activities: Vec<ActivityEntry>,
    /// Read-only activity-detail buffers used by the selectable inspector log cards.
    /// 检查面板可选运行日志卡片使用的只读活动详情缓冲区。
    activity_raw_buffers: Vec<text_editor::Content>,
    /// Session-scoped transcript messages rendered inside the main workbench output area.
    /// 渲染在主工作台输出区中的按会话归属消息转录集合。
    session_messages: BTreeMap<usize, Vec<SessionMessageEntry>>,
    /// Archived session summaries retained outside the visible workbench for later restoration.
    /// 保留在可见工作台之外、供后续恢复使用的归档会话摘要集合。
    archived_sessions: Vec<SessionSummary>,
    /// Archived session-scoped transcript messages retained for later restoration.
    /// 为后续恢复保留的按归档会话归属消息转录集合。
    archived_session_messages: BTreeMap<usize, Vec<SessionMessageEntry>>,
    /// Session-scoped hidden compaction checkpoints retained for future context reconstruction.
    /// 为未来上下文重建保留的按会话归属隐藏压缩检查点集合。
    session_compactions: BTreeMap<usize, Vec<SessionCompactionEntry>>,
    /// Archived hidden compaction checkpoints retained for later restoration.
    /// 为后续恢复保留的按归档会话归属隐藏压缩检查点集合。
    archived_session_compactions: BTreeMap<usize, Vec<SessionCompactionEntry>>,
    /// Session-scoped runtime-state snapshots retained for title, summary, and compaction chaining.
    /// 为标题、摘要与压缩串联保留的按会话归属运行时状态快照集合。
    session_runtime_state: BTreeMap<usize, SessionRuntimeStateEntry>,
    /// Archived session-scoped runtime-state snapshots retained for later restoration.
    /// 为后续恢复保留的按归档会话归属运行时状态快照集合。
    archived_session_runtime_state: BTreeMap<usize, SessionRuntimeStateEntry>,
    /// Session-scoped runtime events rendered beneath the transcript message stream.
    /// 渲染在消息流下方的按会话归属运行时事件集合。
    session_runtime_events: BTreeMap<usize, Vec<RuntimeEvent>>,
    /// Session-scoped collapsed runtime-event identifiers kept only in memory for the current launch.
    /// 仅在当前启动周期内保留的按会话归属已折叠运行时事件标识集合。
    collapsed_runtime_event_ids: BTreeMap<usize, BTreeSet<String>>,
    /// Session-scoped Markdown-view message identifiers used when the user explicitly switches from selectable source text back to Markdown.
    /// 当用户从可选原文显式切回 Markdown 时使用的按会话归属 Markdown 消息标识集合。
    markdown_session_message_ids: BTreeMap<usize, BTreeSet<String>>,
    /// Session-scoped Markdown-view runtime-event identifiers used when the user explicitly switches from selectable source text back to Markdown.
    /// 当用户从可选原文显式切回 Markdown 时使用的按会话归属 Markdown 运行时事件标识集合。
    markdown_runtime_event_ids: BTreeMap<usize, BTreeSet<String>>,
    /// Read-only transcript source buffers keyed by stable message identifier.
    /// 以稳定消息标识为键保存的只读转录原文缓冲区。
    session_message_raw_buffers: BTreeMap<String, text_editor::Content>,
    /// Parsed transcript Markdown payloads keyed by stable message identifier.
    /// 以稳定消息标识为键保存的已解析转录 Markdown 载荷。
    session_message_markdown: BTreeMap<String, markdown::Content>,
    /// Selectable transcript Markdown buffers keyed by stable message identifier.
    /// 以稳定消息标识为键保存的可选转录 Markdown 缓冲区。
    session_message_markdown_buffers: BTreeMap<String, text_editor::Content>,
    /// Active streaming thinking-message identifiers keyed by session id.
    /// 以会话标识为键保存的当前流式思考消息标识集合。
    streaming_thinking_message_ids: BTreeMap<usize, String>,
    /// Session identifiers whose current streaming thinking message should still stay in the floating overlay.
    /// 当前流式思考消息仍应停留在浮层中的会话标识集合。
    floating_streaming_thinking_sessions: BTreeSet<usize>,
    /// Session-scoped flag telling whether the floating streaming-thinking overlay should keep auto-following its latest tail.
    /// 以会话为粒度保存的流式思考浮层是否应继续自动追随最新尾部标记。
    session_streaming_thinking_follow_tail: BTreeMap<usize, bool>,
    /// Centered modal state retained when the user explicitly opens one inline thinking block for closer inspection.
    /// 当用户显式打开某个内联思考区块进行聚焦查看时保留的居中弹层状态。
    active_thinking_modal: Option<ThinkingModalState>,
    /// Pending inline-thinking toggle waiting for the double-click window to elapse before it is committed.
    /// 等待双击判定窗口结束后再正式提交的待执行内联思考折叠切换状态。
    pending_thinking_inline_toggle: Option<PendingThinkingInlineToggleState>,
    /// Active streaming output-message identifiers keyed by session id.
    /// 以会话标识为键保存的当前流式输出消息标识集合。
    streaming_output_message_ids: BTreeMap<usize, String>,
    /// Read-only runtime-event source buffers keyed by stable runtime-event identifier.
    /// 以稳定运行时事件标识为键保存的只读运行时原文缓冲区。
    runtime_event_raw_buffers: BTreeMap<String, text_editor::Content>,
    /// Parsed runtime-event Markdown payloads keyed by stable runtime-event identifier.
    /// 以稳定运行时事件标识为键保存的已解析运行时 Markdown 载荷。
    runtime_event_markdown: BTreeMap<String, markdown::Content>,
    /// Selectable runtime-event Markdown buffers keyed by stable runtime-event identifier.
    /// 以稳定运行时事件标识为键保存的可选运行时 Markdown 缓冲区。
    runtime_event_markdown_buffers: BTreeMap<String, text_editor::Content>,
    /// Unified structured transcript buffers keyed by visible session id.
    /// 以可见会话标识为键保存的统一结构化转录缓冲区。
    session_agent_view_buffers: BTreeMap<usize, text_editor::Content>,
    /// Unified raw-source transcript buffers keyed by visible session id.
    /// 以可见会话标识为键保存的统一原始源码转录缓冲区。
    session_agent_view_raw_buffers: BTreeMap<usize, text_editor::Content>,
    /// Unified selectable transcript projections keyed by visible session id.
    /// 以可见会话标识为键保存的统一可选转录投影对象。
    session_agent_view_selectable_documents:
        BTreeMap<usize, vulcan_agent_view::AgentViewSelectableDocument>,
    /// Unified structured transcript documents keyed by visible session id.
    /// 以可见会话标识为键保存的统一结构化转录文档。
    session_agent_view_documents: BTreeMap<usize, vulcan_agent_view::AgentViewDocument>,
    /// Unified block collapsed states keyed by session id and stable block identifier.
    /// 以会话标识与稳定区块标识为键保存的统一块折叠状态。
    session_agent_view_block_collapsed: BTreeMap<usize, BTreeMap<String, bool>>,
    /// Session-scoped flag telling whether the transcript should keep auto-following the newest tail output.
    /// 以会话为粒度保存的转录自动追随最新尾部输出标记。
    session_agent_view_follow_tail: BTreeMap<usize, bool>,
    /// Latest transcript scrollable viewport keyed by session id.
    /// 以会话标识为键保存的最新转录滚动视口。
    session_agent_view_viewports: BTreeMap<usize, scrollable::Viewport>,
    /// Active drag auto-scroll state for the main rendered transcript surface.
    /// 主渲染转录表面的当前拖拽自动滚动状态。
    active_transcript_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Latest floating thinking-overlay viewport keyed by session id.
    /// 以会话标识为键保存的最新浮动思考层视口。
    session_streaming_thinking_viewports: BTreeMap<usize, scrollable::Viewport>,
    /// Active drag auto-scroll state for the floating thinking overlay body.
    /// 流式思考浮层正文的当前拖拽自动滚动状态。
    active_thinking_overlay_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Surface mode currently used by the unified transcript panel.
    /// 当前统一转录面板使用的表面模式。
    agent_transcript_surface_mode: AgentTranscriptSurfaceMode,
    /// Session-scoped execution phases reflected by the workbench status bar.
    /// 反映在工作台状态条中的按会话归属执行阶段集合。
    session_run_phases: BTreeMap<usize, SessionRunPhase>,
    /// Monotonic identifier seed used to distinguish restarted or stopped execution streams.
    /// 用于区分重启后或已停止执行流的单调递增标识种子。
    next_run_id: u64,
    /// Active execution stream identifier tracked while one real AI call is still in flight.
    /// 当真实 AI 调用仍在进行中时跟踪的活动执行流标识。
    active_run_id: Option<u64>,
    /// Session identifier that currently owns the active real AI call.
    /// 当前拥有活动真实 AI 调用的会话标识。
    active_run_session_id: Option<usize>,
    /// Abort handle used to stop the active real AI call at the iced task layer.
    /// 用于在 iced 任务层停止当前真实 AI 调用的中止句柄。
    active_run_handle: Option<task::Handle>,
    /// Identifier seed for newly created sessions.
    /// 新建会话使用的自增标识种子。
    next_session_id: usize,
    /// Current startup lifecycle shown by the full-screen loading overlay.
    /// 由全屏载入遮罩展示的当前启动生命周期状态。
    startup_lifecycle: StartupLifecycle,
    /// Optional controller runtime handle retained after the shared data plane boots.
    /// 在共享数据平面完成引导后保留的可选控制器运行时句柄。
    controller_runtime: Option<ControllerRuntimeHandle>,
    /// Optional LuaSkills runtime handle retained after the secondary startup phase completes.
    /// 在二级启动阶段完成后保留的可选 LuaSkills 运行时句柄。
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Optional long-lived MCP runtime handle retained after the background MCP bootstrap completes.
    /// 在后台 MCP 引导完成后保留的可选长期 MCP 运行时句柄。
    mcp_runtime: Option<McpRuntimeHandle>,
    /// Latest LuaSkills skill catalog loaded from the embedded host runtime.
    /// 从嵌入式宿主运行时载入的最新 LuaSkills 技能清单。
    luaskills_catalog: Option<LuaSkillsCatalog>,
    /// Local panel-scoped LuaSkills feedback message shown near the management widgets.
    /// 展示在 LuaSkills 管理控件附近的面板局部反馈消息。
    luaskills_notice: String,
    /// Install source choice currently selected in the LuaSkills management panel.
    /// 当前在 LuaSkills 管理面板中选中的安装来源类型。
    luaskills_install_source_choice: LuaSkillsInstallSourceChoice,
    /// Optional explicit skill id typed into the LuaSkills install form.
    /// 在 LuaSkills 安装表单中输入的可选显式技能标识符。
    luaskills_install_skill_id: String,
    /// Raw source locator typed into the LuaSkills install form.
    /// 在 LuaSkills 安装表单中输入的原始来源定位值。
    luaskills_install_source: String,
    /// Whether one LuaSkills catalog or lifecycle operation is still running in the background.
    /// 当前是否仍有某个 LuaSkills 清单或生命周期操作在后台执行。
    luaskills_operation_in_flight: bool,
    /// Persisted provider configuration file loaded from `~/.vulcan/vulcan-code.json`.
    /// 从 `~/.vulcan/vulcan-code.json` 载入的持久化供应商配置文件。
    provider_configuration: ProviderConfigurationFile,
    /// User-visible agent profile catalog loaded from the dedicated user directory and state file.
    /// 从独立用户目录与状态文件载入的面向用户 Agent 配置目录。
    agent_profile_catalog: AgentProfileCatalog,
    /// Shared controller bootstrap configuration loaded from the runtime config directory.
    /// 从运行时配置目录载入的共享控制器引导配置。
    controller_configuration: PersistedControllerConfiguration,
    /// Compose runtime foundation derived from the shared configuration file.
    /// 根据共享配置文件构建出的 compose 运行时基础架构。
    runtime_foundation: RuntimeFoundation,
    /// Dedicated title, explore, summary, and compaction execution chain for session-derived outputs.
    /// 负责会话派生输出的专用标题、探索、摘要与压缩执行链。
    session_skill_chain: SessionDerivedSkillChain,
    /// Currently selected top-level panel shown in the main display region.
    /// 当前显示在主显示区域中的顶层面板类型。
    active_main_panel: MainDisplayPanel,
    /// Currently selected provider entry inside the configuration page.
    /// 当前在配置页面中选中的供应商条目。
    selected_provider: ProviderChoice,
    /// Editable provider form state for the selected provider.
    /// 当前选中供应商对应的可编辑表单状态。
    provider_form: ProviderFormState,
    /// Optional provider-configuration feedback message shown near the configuration widgets.
    /// 展示在配置控件附近的供应商配置反馈消息。
    provider_notice: String,
    /// Optional MCP-configuration feedback message shown near the MCP settings widgets.
    /// 展示在 MCP 设置控件附近的 MCP 配置反馈消息。
    mcp_notice: String,
    /// Currently selected MCP server identifier inside the settings page.
    /// 当前在设置页面中选中的 MCP 服务器标识。
    selected_mcp_server_id: Option<String>,
    /// Editable MCP server form state for the selected MCP server entry.
    /// 当前选中 MCP 服务器条目对应的可编辑表单状态。
    mcp_server_form: McpServerFormState,
    /// Latest validated execution preparation snapshot built from the current runtime state.
    /// 根据当前运行时状态构建出的最近一次执行准备快照。
    last_execution_preparation: Option<ExecutionPreparation>,
    /// Latest successful MCP discovery summary rows produced by one real execution attempt.
    /// 某次真实执行产出的最近一次 MCP 成功发现摘要行集合。
    last_mcp_discovery_rows: Vec<String>,
    /// Latest failed MCP discovery summary rows produced by one real execution attempt.
    /// 某次真实执行产出的最近一次 MCP 失败发现摘要行集合。
    last_mcp_discovery_errors: Vec<String>,
    /// Selected execution mode.
    /// 当前选择的执行模式。
    execution_mode: ExecutionMode,
    /// Active visual theme.
    /// 当前使用的视觉主题。
    app_theme: Theme,
    /// Whether the runtime controls should send an explicit temperature argument.
    /// 运行参数控制区是否应显式发送温度参数。
    temperature_enabled: bool,
    /// Sampling temperature used as a planning heuristic when the override is enabled.
    /// 在显式开启覆盖时作为规划启发参数使用的采样温度。
    temperature: f32,
    /// Estimated context usage percentage.
    /// 估算的上下文占用百分比。
    context_usage: f32,
    /// Whether plan-first workflow is enabled.
    /// 是否启用先计划后执行流程。
    generate_plan: bool,
    /// Whether to run cargo check before execution.
    /// 是否在执行前运行 cargo check。
    run_cargo_check: bool,
    /// Whether to persist memory and execution breadcrumbs.
    /// 是否持久化记忆与执行轨迹。
    persist_memory: bool,
    /// Whether low-risk actions can be auto-approved.
    /// 是否允许低风险动作自动批准。
    auto_approve: bool,
    /// Status message displayed in the footer.
    /// 展示在底部状态栏中的状态消息。
    status_message: String,
}

/// Startup lifecycle displayed through the blocking full-screen loading overlay.
/// 通过阻塞式全屏载入遮罩展示的启动生命周期状态。
#[derive(Debug, Clone)]
enum StartupLifecycle {
    /// The application is still bootstrapping its controller-backed data plane.
    /// 应用仍在引导其基于控制器的数据平面。
    Bootstrapping { message: String },
    /// The application data plane failed to initialize and needs user intervention.
    /// 应用数据平面初始化失败，需要用户干预。
    Failed { message: String },
    /// The startup bootstrap completed and the normal workspace can accept interaction.
    /// 启动引导已完成，正常工作区现在可以接受交互。
    Ready,
}

/// Unified transcript surface mode shown in the center workbench panel.
/// 展示在中央工作台面板中的统一转录表面模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentTranscriptSurfaceMode {
    /// Legacy segmented rendered-preview stream kept as the primary per-block visual inspection surface.
    /// 作为首要逐块视觉检查表面的旧式分段渲染流。
    DetailCards,
    /// Selectable structured transcript projection backed by the unified agent-view document.
    /// 由统一 agent-view 文档支撑的可选结构化转录投影。
    Structured,
    /// Selectable raw-source projection backed by the unified agent-view document.
    /// 由统一 agent-view 文档支撑的可选原始源码投影。
    RawSource,
}

/// Hover state describing the active visual line in one diff hunk.
/// 描述某个 diff hunk 中当前激活可视行的悬停状态。
#[derive(Debug, Clone)]
struct HoveredDiffLine {
    /// Stable hunk buffer key owning the hovered visual line.
    /// 拥有当前悬停可视行的稳定 hunk 缓冲区键。
    file_key: String,
    /// Zero-based visual line index inside the current diff hunk body.
    /// 当前 diff hunk 主体内从零开始的可视行索引。
    line_index: usize,
}

/// Materialized visual-line slice range for one virtualized diff hunk editor.
/// 单个虚拟化 Diff hunk 编辑器已物化的可视行切片范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffVisualSliceRange {
    /// Inclusive start index of the visible visual-line slice.
    /// 可视行切片的起始索引（包含）。
    start: usize,
    /// Exclusive end index of the visible visual-line slice.
    /// 可视行切片的结束索引（不包含）。
    end: usize,
}

/// Latest pointer position tracked inside one custom title bar drag region.
/// 在某个自定义标题栏拖拽区域内追踪到的最近鼠标位置。
#[derive(Debug, Clone, Copy)]
struct WindowTitleBarPointer {
    /// Window identifier owning the tracked title bar pointer position.
    /// 拥有当前标题栏鼠标位置的窗口标识。
    window_id: window::Id,
    /// Latest pointer position relative to the title bar drag region.
    /// 相对于标题栏拖拽区域的最近鼠标位置。
    position: iced::Point,
}

/// Pending drag candidate captured from one custom title bar press.
/// 从某次自定义标题栏按下动作中捕获的待确认拖动候选状态。
#[derive(Debug, Clone, Copy)]
struct PendingWindowTitleBarDrag {
    /// Window identifier targeted by the future native drag request.
    /// 未来原生拖动请求将作用到的窗口标识。
    window_id: window::Id,
    /// Pointer origin captured when the title bar was pressed.
    /// 标题栏被按下时捕获到的鼠标起始位置。
    origin: iced::Point,
}

/// Pending resize candidate captured from the main task drawer divider.
/// 从主任务抽屉分隔柄捕获的待确认缩放候选状态。
#[derive(Debug, Clone, Copy)]
struct PendingMainTaskPanelResize {
    /// Cursor x position captured when the drawer resize interaction started.
    /// 任务抽屉缩放交互开始时捕获到的鼠标横向位置。
    origin_x: f32,
    /// Drawer width captured when the resize interaction started.
    /// 任务抽屉缩放交互开始时捕获到的抽屉宽度。
    initial_width: f32,
}

/// Pending interaction candidate captured from one project-row press inside the task tree.
/// 从任务树某个项目行按下动作中捕获的待判定交互候选状态。
#[derive(Debug, Clone)]
struct PendingTaskProjectInteraction {
    /// Stable identifier of the pressed project row.
    /// 当前被按下项目行的稳定标识。
    project_id: String,
    /// Cursor origin captured when the interaction started.
    /// 交互开始时捕获到的鼠标起始位置。
    origin: iced::Point,
}

/// Centered modal state retained while one inline thinking block is explicitly opened above the transcript.
/// 当某个内联思考区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
struct ThinkingModalState {
    /// Session identifier owning the currently opened thinking block.
    /// 当前已打开思考区块所属的会话标识。
    session_id: usize,
    /// Stable rendered-transcript block identifier of the opened thinking block.
    /// 当前已打开思考区块的稳定渲染转录区块标识。
    block_id: String,
    /// Current lifecycle phase of the modal so opening and closing animations can share one state container.
    /// 当前弹层的生命周期阶段，以便打开与关闭动画共享同一状态容器。
    phase: ThinkingModalPhase,
    /// Monotonic timestamp captured when the current phase began.
    /// 当前阶段开始时捕获的单调时间戳。
    started_at: Instant,
}

/// Pending inline-thinking toggle retained until the double-click window has elapsed.
/// 在双击判定窗口结束前保留的待执行内联思考折叠切换状态。
#[derive(Debug, Clone)]
struct PendingThinkingInlineToggleState {
    /// Session identifier owning the inline thinking block that may still toggle.
    /// 拥有当前待切换内联思考区块的会话标识。
    session_id: usize,
    /// Stable rendered-transcript block identifier targeted by the pending inline toggle.
    /// 当前待执行内联切换所针对的稳定渲染转录区块标识。
    block_id: String,
    /// Monotonic timestamp captured when the click that scheduled the pending toggle was received.
    /// 收到创建该待执行切换的点击时捕获的单调时间戳。
    started_at: Instant,
}

/// Active drag auto-scroll snapshot retained while one transcript-like surface keeps selecting beyond its visible viewport.
/// 当某个转录类表面在可见视口之外继续拖拽选择时保留的活动自动滚动快照。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveTranscriptDragAutoScroll {
    /// Session identifier that owns the scrollable surface currently being auto-scrolled.
    /// 当前正在被自动滚动的滚动表面所属会话标识。
    session_id: usize,
    /// Latest overshoot request reported by the selectable transcript widget.
    /// 可选择转录组件最近一次回传的越界滚动请求。
    request: SelectableTranscriptDragAutoScroll,
}

/// Drop-target previews used while the task-tree project cards are being reordered.
/// 任务树项目卡片重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskProjectDropTarget {
    /// Inserts the dragged project before one concrete project row.
    /// 将正在拖拽的项目插入到某个具体项目行之前。
    Before(String),
    /// Inserts the dragged pinned project at the end of the pinned section.
    /// 将正在拖拽的置顶项目插入到置顶分区末尾。
    EndOfPinned,
    /// Inserts the dragged regular project at the end of the regular section.
    /// 将正在拖拽的普通项目插入到普通分区末尾。
    EndOfRegular,
}

impl DiffVisualSliceRange {
    /// Builds one normalized slice range constrained by the total visual-line count.
    /// 根据总可视行数量构建一个规范化切片范围。
    fn new(start: usize, end: usize, total_lines: usize) -> Self {
        let bounded_start = start.min(total_lines);
        let bounded_end = end.max(bounded_start).min(total_lines);

        Self {
            start: bounded_start,
            end: bounded_end,
        }
    }

    /// Returns the number of visual lines covered by the slice.
    /// 返回该切片覆盖的可视行数量。
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// Visible virtualized range of diff file cards inside the current review viewport.
/// 当前审查视口内可见的虚拟化 Diff 文件卡片范围。
#[derive(Debug, Clone, Copy)]
struct VisibleDiffFileRange {
    /// Inclusive start index of the visible file-card slice.
    /// 可见文件卡片切片的起始索引（包含）。
    start: usize,
    /// Exclusive end index of the visible file-card slice.
    /// 可见文件卡片切片的结束索引（不包含）。
    end: usize,
    /// Height spacer inserted before the first rendered file card.
    /// 插入在首个渲染文件卡片之前的高度占位。
    top_spacer_height: f32,
    /// Height spacer inserted after the last rendered file card.
    /// 插入在最后一个渲染文件卡片之后的高度占位。
    bottom_spacer_height: f32,
}

/// One visible hunk slice together with the hidden-height spacers surrounding it.
/// 单个可见 hunk 切片及其上下隐藏高度占位描述。
#[derive(Debug, Clone, Copy)]
struct VisibleDiffHunkSlice {
    /// Visual-line slice range currently rendered inside the hunk editor.
    /// 当前在 hunk 编辑器中渲染的可视行切片范围。
    range: DiffVisualSliceRange,
    /// Hidden height above the visible slice inside the full hunk block.
    /// 整个 hunk 区块内位于可视切片上方的隐藏高度。
    top_hidden_height: f32,
    /// Hidden height below the visible slice inside the full hunk block.
    /// 整个 hunk 区块内位于可视切片下方的隐藏高度。
    bottom_hidden_height: f32,
}

/// Active diff file descriptor derived from the current review scroll position.
/// 根据当前审查滚动位置推导出的激活 diff 文件描述。
#[derive(Debug, Clone, Copy)]
struct ActiveDiffFile<'a> {
    /// Active file payload currently closest to the top of the review viewport.
    /// 当前最接近审查视口顶部的激活文件载荷。
    file: &'a DiffFile,
    /// Whether the active file card is currently collapsed.
    /// 当前激活文件卡片是否处于折叠状态。
    is_collapsed: bool,
}

/// Semantic icon kind rendered inside one custom window control button.
/// 渲染在单个自定义窗口控制按钮中的语义图标类型。
#[derive(Debug, Clone, Copy)]
enum WindowControlIcon {
    /// Minimize-window affordance icon.
    /// 最小化窗口操作图标。
    Minimize,
    /// Maximize-window affordance icon.
    /// 最大化窗口操作图标。
    Maximize,
    /// Restore-window affordance icon displayed while maximized.
    /// 窗口已最大化时显示的还原操作图标。
    Restore,
    /// Close-window affordance icon.
    /// 关闭窗口操作图标。
    Close,
}

/// Editable provider form state kept in sync with the persisted provider configuration file.
/// 与持久化供应商配置文件保持同步的可编辑供应商表单状态。
#[derive(Debug, Clone, Default)]
struct ProviderFormState {
    /// Provider API key or access token value.
    /// 供应商 API Key 或访问 Token 值。
    api_key: String,
    /// Optional provider base URL override value.
    /// 可选的供应商基址覆盖值。
    base_url: String,
    /// Azure-style resource name value when the provider requires it.
    /// 当供应商需要时使用的 Azure 风格资源名称值。
    resource_name: String,
    /// Cloudflare-style account identifier value when the provider requires it.
    /// 当供应商需要时使用的 Cloudflare 风格账户标识值。
    account_id: String,
}

impl ProviderFormState {
    /// Builds a form state from persisted provider options.
    /// 从持久化供应商选项构建表单状态。
    fn from_options(options: &PersistedProviderOptions) -> Self {
        Self {
            api_key: options.get(ProviderFieldKey::ApiKey),
            base_url: options.get(ProviderFieldKey::BaseUrl),
            resource_name: options.get(ProviderFieldKey::ResourceName),
            account_id: options.get(ProviderFieldKey::AccountId),
        }
    }

    /// Reads one field value from the editable form state.
    /// 从可编辑表单状态中读取一个字段值。
    fn get(&self, key: ProviderFieldKey) -> &str {
        match key {
            ProviderFieldKey::ApiKey => &self.api_key,
            ProviderFieldKey::BaseUrl => &self.base_url,
            ProviderFieldKey::ResourceName => &self.resource_name,
            ProviderFieldKey::AccountId => &self.account_id,
        }
    }

    /// Writes one field value into the editable form state.
    /// 向可编辑表单状态中写入一个字段值。
    fn set(&mut self, key: ProviderFieldKey, value: String) {
        match key {
            ProviderFieldKey::ApiKey => self.api_key = value,
            ProviderFieldKey::BaseUrl => self.base_url = value,
            ProviderFieldKey::ResourceName => self.resource_name = value,
            ProviderFieldKey::AccountId => self.account_id = value,
        }
    }

    /// Converts the editable form state back into persisted provider options.
    /// 将可编辑表单状态转换回持久化供应商选项。
    fn to_options(&self) -> PersistedProviderOptions {
        let mut options = PersistedProviderOptions::default();
        options.set(ProviderFieldKey::ApiKey, self.api_key.clone());
        options.set(ProviderFieldKey::BaseUrl, self.base_url.clone());
        options.set(ProviderFieldKey::ResourceName, self.resource_name.clone());
        options.set(ProviderFieldKey::AccountId, self.account_id.clone());
        options
    }
}

/// Pick-list choice representing one configurable runtime agent.
/// 表示单个可配置运行时代理的下拉选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentChoice {
    /// Stable agent identifier.
    /// 稳定代理标识。
    id: String,
    /// Human-readable display label.
    /// 人类可读显示标签。
    label: String,
}

impl std::fmt::Display for AgentChoice {
    /// Renders the user-facing label of one agent picker item.
    /// 渲染单个代理选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Pick-list choice representing one configurable MCP server entry.
/// 表示单个可配置 MCP 服务器条目的下拉选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
struct McpServerChoice {
    /// Stable MCP server identifier.
    /// 稳定 MCP 服务器标识。
    id: String,
    /// Human-readable display label.
    /// 人类可读显示标签。
    label: String,
}

impl std::fmt::Display for McpServerChoice {
    /// Renders the user-facing label of one MCP server picker item.
    /// 渲染单个 MCP 服务器选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Editable MCP server form state mirrored into the shared configuration file.
/// 映射回共享配置文件的可编辑 MCP 服务器表单状态。
#[derive(Debug, Clone)]
struct McpServerFormState {
    /// Human-readable MCP server name.
    /// 人类可读的 MCP 服务器名称。
    name: String,
    /// Whether the MCP server is enabled.
    /// 当前 MCP 服务器是否启用。
    enabled: bool,
    /// Selected transport family.
    /// 当前选中的传输类型。
    transport: PersistedMcpTransportKind,
    /// Command used to start the MCP server process.
    /// 用于启动 MCP 服务器进程的命令。
    command: String,
    /// Delimited argument text shown in the settings form.
    /// 展示在设置表单中的分隔参数文本。
    args_text: String,
    /// Remote endpoint URL used by HTTP transports.
    /// HTTP 传输使用的远端端点 URL。
    url: String,
    /// Optional bearer token used by HTTP transports.
    /// HTTP 传输使用的可选 Bearer Token。
    bearer_token: String,
    /// Semicolon-delimited HTTP header declarations.
    /// 使用分号分隔的 HTTP 请求头声明文本。
    headers_text: String,
    /// Optional working directory override.
    /// 可选的工作目录覆盖值。
    cwd: String,
    /// Semicolon-delimited environment declarations.
    /// 使用分号分隔的环境变量声明文本。
    env_text: String,
}

impl McpServerFormState {
    /// Builds an editable form state from one persisted MCP server entry.
    /// 根据单个持久化 MCP 服务器条目构建可编辑表单状态。
    fn from_entry(entry: &PersistedMcpServerEntry) -> Self {
        Self {
            name: entry.name.clone(),
            enabled: entry.enabled,
            transport: entry.transport,
            command: entry.command.clone(),
            args_text: join_mcp_server_args(&entry.args),
            url: entry.url.clone().unwrap_or_default(),
            bearer_token: entry.bearer_token.clone().unwrap_or_default(),
            headers_text: join_mcp_server_headers(&entry.headers),
            cwd: entry.cwd.clone().unwrap_or_default(),
            env_text: join_mcp_server_env(&entry.env),
        }
    }

    /// Converts the editable form state into one persisted MCP server entry.
    /// 将可编辑表单状态转换为单个持久化 MCP 服务器条目。
    fn to_entry(&self) -> Result<PersistedMcpServerEntry, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(String::from("请先填写 MCP 服务器名称。"));
        }

        match self.transport {
            PersistedMcpTransportKind::Stdio => {
                let command = self.command.trim();
                if command.is_empty() {
                    return Err(String::from("请先填写 MCP 服务器启动命令。"));
                }

                Ok(PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: self.enabled,
                    transport: PersistedMcpTransportKind::Stdio,
                    command: command.to_string(),
                    args: parse_mcp_server_args(&self.args_text),
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    cwd: trim_to_option(&self.cwd),
                    env: parse_mcp_server_env(&self.env_text)?,
                })
            }
            PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
                let url = self.url.trim();
                if url.is_empty() {
                    return Err(String::from("请先填写 MCP 远端服务器 URL。"));
                }
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(String::from(
                        "MCP 远端服务器 URL 必须以 `http://` 或 `https://` 开头。",
                    ));
                }

                Ok(PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: self.enabled,
                    transport: self.transport,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(url.to_string()),
                    bearer_token: trim_to_option(&self.bearer_token),
                    headers: parse_mcp_server_headers(&self.headers_text)?,
                    cwd: None,
                    env: BTreeMap::new(),
                })
            }
        }
    }
}

impl Default for McpServerFormState {
    /// Builds the default editable MCP server form.
    /// 构建默认的可编辑 MCP 服务器表单。
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: false,
            transport: PersistedMcpTransportKind::Stdio,
            command: String::new(),
            args_text: String::new(),
            url: String::new(),
            bearer_token: String::new(),
            headers_text: String::new(),
            cwd: String::new(),
            env_text: String::new(),
        }
    }
}

/// Top-level panels that can be displayed inside the main workbench region.
/// 可以显示在主工作台区域中的顶层面板类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainDisplayPanel {
    /// Default workbench panel containing the pane-grid workspace.
    /// 包含分栏工作区的默认主面板。
    Main,
    /// Regular-skill panel placeholder used to host general prompt and skill orchestration later.
    /// 用于后续承载常规提示词与技能编排的技能（常规）面板占位。
    PromptSkills,
    /// LuaSkills panel placeholder used to host Lua-based capabilities later.
    /// 用于后续承载基于 LuaSkills 的能力配置的技能（LuaSkills）面板占位。
    LuaSkills,
    /// Automation panel placeholder used to host recurring and scheduled flows later.
    /// 用于后续承载定时与自动化流程的自动化面板占位。
    Automation,
    /// Agent panel placeholder used to host multi-agent configuration later.
    /// 用于后续承载多代理配置的代理面板占位。
    Agents,
    /// Plugin panel placeholder used to host plugin management later.
    /// 用于后续承载插件管理的插件面板占位。
    Plugins,
    /// VulcanMemoryMesh panel placeholder used to host memory-mesh capabilities later.
    /// 用于后续承载 VulcanMemoryMesh 记忆网络能力的面板占位。
    VulcanMemoryMesh,
    /// Example panel that wraps the current workspace surface for iterative redesign.
    /// 用于包裹当前工作区主体并便于重构迭代的示例面板。
    Example,
    /// Settings panel that focuses on provider configuration and runtime tuning.
    /// 聚焦供应商配置与运行调优的设置面板。
    Settings,
}

impl MainDisplayPanel {
    /// Returns the user-facing label shown in the top-level panel switcher.
    /// 返回展示在顶层面板切换入口中的面向用户标签。
    const fn label(self) -> &'static str {
        match self {
            Self::Main => "工作台",
            Self::PromptSkills => "技能（常规）",
            Self::LuaSkills => "技能（LuaSkills）",
            Self::Automation => "自动化",
            Self::Agents => "代理",
            Self::Plugins => "插件",
            Self::VulcanMemoryMesh => "VulcanMemoryMesh",
            Self::Example => "示例面板",
            Self::Settings => "设置",
        }
    }

    /// Returns the contextual caption shown beside the top-level panel switcher.
    /// 返回展示在顶层面板切换器旁侧的上下文说明文本。
    const fn description(self) -> &'static str {
        match self {
            Self::Main => "承载任务抽屉与主工作区，是后续 AI Coding 主入口所在位置。",
            Self::PromptSkills => "承载常规提示词模板、技能节点与后续编排入口的占位面板。",
            Self::LuaSkills => "承载 LuaSkills 技能目录、配置与执行入口的占位面板。",
            Self::Automation => "承载自动化任务、定时策略与唤醒流程的占位面板。",
            Self::Agents => "承载多代理定义、分工关系与运行策略的占位面板。",
            Self::Plugins => "承载插件能力、扩展安装与版本管理的占位面板。",
            Self::VulcanMemoryMesh => "承载记忆网格、关联记忆与长期上下文能力的占位面板。",
            Self::Example => "承载当前工作区主体与交互示例，便于持续重构布局。",
            Self::Settings => "集中呈现供应商配置与运行参数，不混入全局动作。",
        }
    }

    /// Returns the local title rendered inside one top-level panel body.
    /// 返回渲染在某个顶层面板主体内部的局部标题。
    const fn section_title(self) -> &'static str {
        match self {
            Self::Main => "工作台",
            Self::PromptSkills => "技能（常规）",
            Self::LuaSkills => "技能（LuaSkills）",
            Self::Automation => "自动化",
            Self::Agents => "代理",
            Self::Plugins => "插件",
            Self::VulcanMemoryMesh => "VulcanMemoryMesh",
            Self::Example => "示例面板",
            Self::Settings => "设置",
        }
    }

    /// Returns the keyboard shortcut digit used by one top-level panel.
    /// 返回某个顶层面板使用的键盘快捷数字。
    const fn shortcut_digit(self) -> &'static str {
        match self {
            Self::Main => "1",
            Self::PromptSkills => "2",
            Self::LuaSkills => "3",
            Self::Automation => "4",
            Self::Agents => "5",
            Self::Plugins => "6",
            Self::VulcanMemoryMesh => "7",
            Self::Example => "8",
            Self::Settings => "9",
        }
    }

    /// Resolves one top-level panel from its keyboard shortcut digit.
    /// 根据键盘快捷数字解析对应的顶层面板。
    const fn from_shortcut_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Main),
            '2' => Some(Self::PromptSkills),
            '3' => Some(Self::LuaSkills),
            '4' => Some(Self::Automation),
            '5' => Some(Self::Agents),
            '6' => Some(Self::Plugins),
            '7' => Some(Self::VulcanMemoryMesh),
            '8' => Some(Self::Example),
            '9' => Some(Self::Settings),
            _ => None,
        }
    }
}

/// Returns the inline monochrome SVG bytes used by one left activity-bar panel switcher icon.
/// 返回左侧活动条某个面板切换图标使用的内联单色 SVG 字节。
fn main_display_panel_icon_svg_bytes(panel: MainDisplayPanel) -> &'static [u8] {
    match panel {
        MainDisplayPanel::Main => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="3.75" y="4.25" width="6.5" height="15.5" rx="1.25"/><rect x="13.75" y="4.25" width="6.5" height="6.5" rx="1.25"/><rect x="13.75" y="13.25" width="6.5" height="6.5" rx="1.25"/></svg>"#,
        MainDisplayPanel::PromptSkills => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3 1.1 3.4a2 2 0 0 0 1.3 1.3L17.8 9l-3.4 1.1a2 2 0 0 0-1.3 1.3L12 14.8l-1.1-3.4a2 2 0 0 0-1.3-1.3L6.2 9l3.4-1.3a2 2 0 0 0 1.3-1.3L12 3Z"/><path d="M5 16.5h5"/><path d="M7.5 14v5"/></svg>"#,
        MainDisplayPanel::LuaSkills => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 588.599 588.6" fill="currentColor" xml:space="preserve"><path d="m267.439 251.205-.377.011c-1.87 7.259-3.731 16.487-5.769 23.525l-7.38 25.676 27.886.105-7.977-25.956c-2.267-7.203-4.513-16.199-6.383-23.361"/><path d="M539.573 49.204h-178.2c-.791 0-1.561.076-2.342.124V0L11.227 46.417V540.98l347.804 47.62v-50.814c.781.053 1.551.116 2.342.116h178.2c20.851 0 37.8-16.965 37.8-37.8V86.999c0-20.839-16.95-37.795-37.8-37.795M107.19 348.648l-59.444-2.342V243.11l20.946-.933v85.018l38.499.907v20.546zm96.908-47.271c0 35.891-17.729 52.26-46.461 51.089-26.634-1.086-41.808-16.891-41.808-51.632v-60.756l23.361-1.039v63.825c0 19.079 6.874 28.962 19.246 29.278 12.912.326 20.126-8.976 20.126-29.131v-65.726l25.537-1.134v65.226zm91.955 54.707-10.049-32.016-36.12-.648-8.699 30.502-27.962-1.098 36.661-118.715 37.47-1.667 41.301 124.923zm259.72 144.018c0 8.933-7.268 16.199-16.2 16.199h-178.2c-.802 0-1.571-.11-2.342-.231V368.36l24.09 64.673 93.936-35.005 3.729-13.109-91.23 33.992-30.523-81.97V219.902l98.846-36.822 44.951 120.648c.611-1.561 1.36-3.068 2.352-4.439l2.046-7.162c.401-1.294.981-2.468 1.667-3.554l-44.561-119.62-105.301 39.242V71.046c.771-.114 1.54-.243 2.342-.243h178.2c8.933 0 16.2 7.27 16.2 16.2v413.099z"/><path d="m521.242 339.926 11.386-38.043a9.3 9.3 0 0 0 .29-3.918l2.942-9.039c.454-1.487-.158-2.837-1.255-3.953.147-.287.348-.52.453-.833.718-2.289-.569-4.714-2.857-5.432l-3.85-1.216c-2.278-.696-4.726.567-5.432 2.858-.074.222-.043.454-.074.686-1.413.211-2.562.82-3.006 2.223l-2.668 9.397c-1.065 1.052-1.915 2.344-2.368 3.881l-11.401 38.045c-.585 1.967-.464 3.971.18 5.774l-15.504 54.422c-1.018 3.354-2.985 18.277.38 19.312 3.364 1.012 10.763-12.234 11.791-15.589l17.439-53.715c1.64-1.142 2.932-2.804 3.554-4.86m8.443 12.303a20.1 20.1 0 0 1-4.515 7.905l-.152.464 1.688 4.514-3.586 1.34-4.335 13.331 22.043-8.216-9.387-25.186zm-89.321-32.764c-2.036-.664-3.428-2.441-3.565-4.557l-.363-5.579a5.095 5.095 0 0 1 2.9-4.968c.675-.326 1.191-.896 1.413-1.561s.158-1.424-.158-2.046a5.2 5.2 0 0 1 .686-5.764l3.681-4.208c.928-1.041 2.173-1.655 3.502-1.748l.938-.065 1.075.34c.844.309 1.54.232 2.142-.076a2.7 2.7 0 0 0 1.36-1.572c.611-1.917 2.246-3.238 4.208-3.499l-.248-3.671a1.01 1.01 0 0 0-.675-.854 15.44 15.44 0 0 1-9.181-7.868 15.5 15.5 0 0 1-.696-12.095.95.95 0 0 0-.259-1.034l-8.886-7.768c-.495-.433-.807-.256-1.07-.126a15.43 15.43 0 0 1-12.061.931c-3.965-1.329-7.251-4.285-9.044-8.068-.152-.346-.532-.567-.928-.541l-11.755.789c-.385.01-.728.288-.849.643a15.46 15.46 0 0 1-7.868 9.194c-3.729 1.87-8.179 2.112-12.107.675-.243-.073-.596-.197-1.028.288l-7.773 8.872a.96.96 0 0 0-.116 1.076 15.5 15.5 0 0 1 .929 12.062 15.5 15.5 0 0 1-8.063 9.028.98.98 0 0 0-.554.928l.791 11.773a.95.95 0 0 0 .654.839c3.997 1.255 7.341 4.129 9.207 7.867a15.58 15.58 0 0 1 .665 12.108.96.96 0 0 0 .273 1.033l8.881 7.773c.485.422.812.253 1.065.11a15.4 15.4 0 0 1 6.096-1.703c2.046-.137 4.05.11 5.98.785a15.5 15.5 0 0 1 9.028 8.059.97.97 0 0 0 .928.537l11.76-.78c.4-.026.728-.29.844-.653a15.5 15.5 0 0 1 7.879-9.197 15.5 15.5 0 0 1 11.76-.785 5.1 5.1 0 0 1 .284-1.487 2.7 2.7 0 0 0-.105-2.099 2.68 2.68 0 0 0-1.577-1.338m-26.516-7.146c-12.613.828-23.556-8.743-24.395-21.357-.854-12.614 8.717-23.572 21.342-24.413 12.63-.844 23.572 8.754 24.416 21.368.844 12.616-8.733 23.559-21.363 24.402"/><path d="M482.888 304.826a7.34 7.34 0 0 1-4.351-3.729 7.34 7.34 0 0 1-.327-5.729.47.47 0 0 0-.126-.499l-4.192-3.681c-.243-.2-.396-.113-.517-.065a7.35 7.35 0 0 1-5.722.453 7.4 7.4 0 0 1-4.282-3.828.45.45 0 0 0-.433-.264l-5.579.377c-.174.021-.343.137-.385.309a7.43 7.43 0 0 1-3.729 4.369 7.35 7.35 0 0 1-5.732.319c-.121-.045-.301-.108-.501.124l-3.691 4.198a.49.49 0 0 0-.063.517 7.34 7.34 0 0 1 .454 5.716 7.4 7.4 0 0 1-3.818 4.282.46.46 0 0 0-.274.443l.38 5.568c.011.19.132.349.316.401a7.33 7.33 0 0 1 4.346 3.733 7.3 7.3 0 0 1 .337 5.727.48.48 0 0 0 .116.496l4.219 3.681c.232.2.38.116.496.053a7.4 7.4 0 0 1 2.9-.807 7.5 7.5 0 0 1 2.815.374 7.28 7.28 0 0 1 4.282 3.818.46.46 0 0 0 .443.258l5.568-.369a.45.45 0 0 0 .396-.306 7.3 7.3 0 0 1 3.729-4.361c1.771-.886 3.876-.991 5.742-.326.111.042.29.095.491-.116l3.681-4.214a.51.51 0 0 0 .063-.512 7.3 7.3 0 0 1-.443-5.716 7.3 7.3 0 0 1 3.818-4.282.46.46 0 0 0 .264-.433l-.369-5.579a.45.45 0 0 0-.322-.4m-20.56 15.493c-5.97.396-11.159-4.15-11.56-10.125-.391-5.97 4.134-11.169 10.125-11.575 5.964-.38 11.153 4.16 11.56 10.135.4 5.975-4.135 11.154-10.125 11.565"/></svg>"#,
        MainDisplayPanel::Automation => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8"/><path d="M12 7.5v5l3 2"/></svg>"#,
        MainDisplayPanel::Agents => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="currentColor"><path d="M16 19a6.99 6.99 0 0 1-5.833-3.129l1.666-1.107a5 5 0 0 0 8.334 0l1.666 1.107A6.99 6.99 0 0 1 16 19m4-11a2 2 0 1 0 2 2 1.98 1.98 0 0 0-2-2m-8 0a2 2 0 1 0 2 2 1.98 1.98 0 0 0-2-2"/><path d="M17.736 30 16 29l4-7h6a1.997 1.997 0 0 0 2-2V6a1.997 1.997 0 0 0-2-2H6a1.997 1.997 0 0 0-2 2v14a1.997 1.997 0 0 0 2 2h9v2H6a4 4 0 0 1-4-4V6a4 4 0 0 1 4-4h20a4 4 0 0 1 4 4v14a4 4 0 0 1-4 4h-4.835Z"/><path fill="none" d="M0 0h32v32H0z"/></svg>"#,
        MainDisplayPanel::Plugins => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="6.5" height="6.5" rx="1.25"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.25"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.25"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.25"/></svg>"#,
        MainDisplayPanel::VulcanMemoryMesh => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="currentColor"><path d="M31.332 11.671c-1.15-3.016-4.669-8.072-15.304-8.072-8.681 0-15.865 5.678-16.026 10.38-.042 1.236.421 4.045 5.313 4.726-.053.274-.079.57-.079.894 0 2.005 2.285 4.8 6.005 4.8 1.288 0 2.248-.197 3.052-.498l3.375 4.2a.796.796 0 0 0 .622.299c.176 0 .354-.058.5-.177a.797.797 0 0 0 .122-1.125l-3.154-3.925c.263-.161.521-.326.779-.495.263-.172.53-.345.81-.515l4.426 5.917a.799.799 0 0 0 .639.321.79.79 0 0 0 .478-.159.8.8 0 0 0 .161-1.121l-.909-1.215c1.03-.006 2.14-.292 3.125-.774 2.339-1.145 3.501-3.046 3-4.804.77-.263 1.586-.77 2.657-1.732 1.092-.981 1.558-3.908.409-6.923m-6.765 12.022c-1.537.749-3.486.919-4.134-.072l-1.643-2.196c.763-.317 1.68-.589 2.898-.766 1.746-.256 2.917-.178 3.862-.117.416.028.79.043 1.151.038.006.023.003.047.012.071.44 1.161-.76 2.364-2.146 3.042m5.291-6.289c-1.887 1.696-2.571 1.65-4.203 1.542q-.441-.03-.945-.05c-.231-.7-.602-1.329-.955-1.917-.813-1.359-1.244-2.189-.63-3.422a.8.8 0 0 0-.357-1.074.79.79 0 0 0-1.07.358c-1.036 2.078-.079 3.677.689 4.961.23.384.422.751.586 1.117-.46.029-.952.073-1.514.155-3.012.439-4.49 1.41-5.794 2.264-1.245.815-2.229 1.461-4.423 1.461-2.852 0-4.409-2.114-4.409-3.2 0-2.239 2.124-2.4 5.621-2.4 2.371 0 4.61 0 5.696-1.353.558-.694.71-1.642.453-2.817a.798.798 0 0 0-.779-.63c-.056 0-.114.006-.17.019a.8.8 0 0 0-.61.951c.148.679.101 1.175-.139 1.474-.606.756-2.65.756-4.452.756-2.235 0-5.074 0-6.43 1.58-1.866-.19-4.504-.862-4.426-3.145.038-1.093.684-2.373 1.802-3.614.262 1.498 1.164 2.727 2.224 3.143.259.1.544.16.84.16.828 0 1.738-.462 2.388-1.764a.8.8 0 0 0-.357-1.074.79.79 0 0 0-1.07.358c-.362.728-.769 1-1.22.83-.718-.282-1.499-1.629-1.229-3.118 1.981-1.574 4.717-2.907 7.88-3.467-.173.562-.545 1.232-.858 1.777-.628 1.089-1.169 2.031-.708 2.834.27.466.79.701 1.549.701.441 0 .798-.358.798-.8 0-.462-.431-.763-.857-.802.125-.309.401-.789.6-1.136.507-.883 1.063-1.853 1.163-2.802.488-.045.987-.061 1.489-.061 9.68 0 12.809 4.412 13.813 7.042.951 2.495.51 4.719.016 5.163"/><path d="M22.768 8.318c-2.17-.852-4.817-.769-5.858 1.323a.8.8 0 0 0 .357 1.074c.397.2.873.039 1.07-.358.59-1.181 2.41-1.111 3.85-.55 1.446.567 2.809 1.773 2.486 2.896a.8.8 0 0 0 .546.99.8.8 0 0 0 .221.031c.348 0 .666-.228.767-.578.618-2.15-1.349-4.011-3.438-4.828"/></svg>"#,
        MainDisplayPanel::Example => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="3.75" y="4.25" width="6.5" height="15.5" rx="1.25"/><rect x="13.75" y="4.25" width="6.5" height="4.75" rx="1.25"/><rect x="13.75" y="11.5" width="6.5" height="8.25" rx="1.25"/></svg>"#,
        MainDisplayPanel::Settings => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="currentColor"><path d="M32 18a14 14 0 1 0 14 14 14.015 14.015 0 0 0-14-14m0 24a10 10 0 1 1 10-10 10.01 10.01 0 0 1-10 10"/><path d="M60 25h-7.163a22 22 0 0 0-1.153-2.784l5.065-5.066a2 2 0 0 0 0-2.828l-7.071-7.071a2 2 0 0 0-2.828 0l-5.065 5.064A22 22 0 0 0 39 11.163V4a2 2 0 0 0-2-2H27a2 2 0 0 0-2 2v7.163a22 22 0 0 0-2.785 1.152L17.15 7.251a2 2 0 0 0-2.828 0l-7.071 7.071a2 2 0 0 0 0 2.828l5.065 5.066A22 22 0 0 0 11.163 25H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h7.163a22 22 0 0 0 1.152 2.784L7.251 46.85a2 2 0 0 0 0 2.828l7.071 7.071a2 2 0 0 0 2.828 0l5.066-5.065A22 22 0 0 0 25 52.837V60a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-7.163a22 22 0 0 0 2.784-1.153l5.066 5.065a2 2 0 0 0 2.828 0l7.071-7.071a2 2 0 0 0 0-2.828l-5.064-5.066A22 22 0 0 0 52.837 39H60a2 2 0 0 0 2-2V27a2 2 0 0 0-2-2m-2 10h-6.655a2 2 0 0 0-1.937 1.5 18 18 0 0 1-1.915 4.626 2 2 0 0 0 .308 2.432l4.706 4.706-4.243 4.243-4.706-4.707a2 2 0 0 0-2.431-.309 18 18 0 0 1-4.627 1.917 2 2 0 0 0-1.5 1.937V58h-6v-6.655a2 2 0 0 0-1.5-1.937 18 18 0 0 1-4.627-1.916 2 2 0 0 0-2.431.309l-4.706 4.706-4.243-4.243 4.707-4.706a2 2 0 0 0 .308-2.432 18 18 0 0 1-1.916-4.626 2 2 0 0 0-1.937-1.5H6v-6h6.655a2 2 0 0 0 1.937-1.5 18 18 0 0 1 1.916-4.627 2 2 0 0 0-.309-2.431l-4.706-4.706 4.243-4.243 4.706 4.707a2 2 0 0 0 2.432.308 17.8 17.8 0 0 1 4.626-1.916 2 2 0 0 0 1.5-1.937V6h6v6.655a2 2 0 0 0 1.5 1.937 17.8 17.8 0 0 1 4.626 1.915 2 2 0 0 0 2.432-.308l4.706-4.706 4.243 4.243-4.707 4.706a2 2 0 0 0-.309 2.431 18 18 0 0 1 1.917 4.627 2 2 0 0 0 1.937 1.5H58Z"/><path d="M26 30h4v4h-4zm8 0h4v4h-4z"/></svg>"#,
    }
}

/// Messages emitted by iced widgets in the first prototype.
/// 首版原型中由 iced 控件发出的消息集合。
#[derive(Debug, Clone)]
enum Message {
    /// Starts the controller-backed startup bootstrap sequence.
    /// 启动基于控制器的启动引导流程。
    BeginStartupBootstrap,
    /// Applies the completed controller bootstrap report.
    /// 应用已完成的控制器引导报告。
    StartupBootstrapFinished(Result<ControllerBootstrapReport, String>),
    /// Applies the completed LuaSkills bootstrap report.
    /// 应用已完成的 LuaSkills 引导报告。
    LuaSkillsBootstrapFinished(Result<LuaSkillsBootstrapReport, String>),
    /// Applies the completed MCP runtime bootstrap result.
    /// 应用已完成的 MCP 运行时引导结果。
    McpRuntimeBootstrapped(Result<McpRuntimeHandle, String>),
    /// Applies the completed workspace-state load result after the shared runtimes become ready.
    /// 应用共享运行时就绪后已完成的工作台状态加载结果。
    WorkspaceStateLoaded(Result<WorkspaceSnapshot, String>),
    /// Applies the completed workspace-state persistence result after one user-visible mutation.
    /// 应用某次用户可见变更后已完成的工作台状态持久化结果。
    WorkspaceStatePersisted(Result<(), String>),
    /// Main workbench window opened successfully.
    /// 主工作台窗口已成功打开。
    MainWindowOpened(window::Id),
    /// Dedicated diff preview window opened successfully.
    /// 独立 Diff 预览窗口已成功打开。
    DiffWindowOpened(window::Id),
    /// Requests opening or focusing the dedicated diff preview window.
    /// 请求打开或聚焦独立 Diff 预览窗口。
    OpenDiffWindow,
    /// Records that one custom title bar drag region has been pressed.
    /// 记录某个自定义标题栏拖拽区域已被按下。
    WindowTitleBarPressed(window::Id),
    /// Updates the latest pointer position inside one custom title bar drag region.
    /// 更新某个自定义标题栏拖拽区域内的最新鼠标位置。
    WindowTitleBarPointerMoved(window::Id, iced::Point),
    /// Clears the pending drag candidate after one custom title bar release.
    /// 在某次自定义标题栏释放后清理待确认拖动候选状态。
    WindowTitleBarReleased(window::Id),
    /// Clears the tracked pointer state after the cursor exits one custom title bar drag region.
    /// 在鼠标离开某个自定义标题栏拖拽区域后清理追踪状态。
    WindowTitleBarExited(window::Id),
    /// Requests minimizing the target window from the custom title bar.
    /// 请求从自定义标题栏最小化目标窗口。
    MinimizeWindow(window::Id),
    /// Requests starting native window resize from one undecorated edge or corner.
    /// 请求从无系统装饰窗口的某个边缘或角开始原生缩放。
    StartWindowResize(window::Id, window::Direction),
    /// Requests toggling maximized state for the target window from the custom title bar.
    /// 请求从自定义标题栏切换目标窗口的最大化状态。
    ToggleMaximizeWindow(window::Id),
    /// Requests closing the target window from the custom title bar.
    /// 请求从自定义标题栏关闭目标窗口。
    CloseWindow(window::Id),
    /// Toggles the left task drawer shown inside the main workbench panel.
    /// 切换显示在主工作台面板中的左侧任务抽屉。
    ToggleMainTaskPanel,
    /// Starts resizing the left task drawer shown inside the main workbench panel.
    /// 开始缩放显示在主工作台面板中的左侧任务抽屉。
    StartMainTaskPanelResize,
    /// Updates whether the task-drawer resize handle is currently hovered.
    /// 更新任务抽屉缩放柄当前是否处于悬停状态。
    MainTaskPanelResizeHandleHovered(bool),
    /// Reports the latest maximized state of one tracked window.
    /// 回传某个受跟踪窗口当前最新的最大化状态。
    WindowMaximizedStateChanged(window::Id, bool),
    /// Switches the active top-level panel shown inside the main display region.
    /// 切换显示在主显示区域中的顶层面板。
    MainDisplayPanelSelected(MainDisplayPanel),
    /// A window was closed by the user or the runtime.
    /// 某个窗口已被用户或运行时关闭。
    WindowClosed(window::Id),
    /// A window was resized by the runtime.
    /// 某个窗口的尺寸已被运行时更新。
    WindowResized(window::Id, iced::Size),
    /// Reports a pointer move tracked at the window level.
    /// 回传在窗口级别追踪到的鼠标移动事件。
    WindowPointerMoved(window::Id, iced::Point),
    /// Reports that the left mouse button was released at the window level.
    /// 回传在窗口级别追踪到的鼠标左键释放事件。
    WindowLeftButtonReleased(window::Id),
    /// Workspace path changed from the text input.
    /// 工作区路径在文本输入框中发生变化。
    WorkspacePathChanged(String),
    /// Prompt composer changed from the text input.
    /// 提示词输入框内容发生变化。
    ComposerChanged(String),
    /// Embedded editor emitted an editing action.
    /// 嵌入式编辑器发出了编辑动作。
    EditorActionPerformed(text_editor::Action),
    /// Supported provider selected from the provider catalog picker.
    /// 从供应商目录选择框中选中了某个受支持供应商。
    ProviderCatalogSelected(ProviderChoice),
    /// One provider form field changed in the configuration page.
    /// 配置页面中的某个供应商表单字段发生变化。
    ProviderFieldChanged(ProviderFieldKey, String),
    /// Persists the currently selected provider configuration.
    /// 持久化当前选中供应商的配置。
    SaveProviderConfiguration,
    /// Removes the currently selected provider configuration.
    /// 移除当前选中供应商的配置。
    RemoveProviderConfiguration,
    /// Tries refreshing the current provider model list from a remote endpoint.
    /// 尝试从远端端点刷新当前供应商的模型列表。
    RefreshProviderModels,
    /// Applies a successful remote provider model refresh result.
    /// 应用一次成功的远端供应商模型刷新结果。
    ProviderModelsRefreshed(String, Vec<ModelChoice>),
    /// Applies a failed remote provider model refresh result.
    /// 应用一次失败的远端供应商模型刷新结果。
    ProviderModelsRefreshFailed(String, String),
    /// Selects the active global model from the current provider model list.
    /// 从当前供应商模型列表中选择全局激活模型。
    ActiveModelSelected(ModelChoice),
    /// Selects one already-configured provider entry for editing.
    /// 选中一个已配置的供应商条目进行编辑。
    ConfiguredProviderSelected(String),
    /// Selects the active runtime agent used by compose-based execution.
    /// 选中 compose 执行链路使用的激活运行时代理。
    ActiveAgentSelected(AgentChoice),
    /// Selects one configured MCP server entry for editing.
    /// 选中一个已配置 MCP 服务器条目进行编辑。
    McpServerSelected(McpServerChoice),
    /// Updates the editable MCP server name field.
    /// 更新可编辑 MCP 服务器名称字段。
    McpServerNameChanged(String),
    /// Selects the editable MCP transport family.
    /// 选择可编辑 MCP 传输类型。
    McpServerTransportSelected(PersistedMcpTransportKind),
    /// Updates the editable MCP server command field.
    /// 更新可编辑 MCP 服务器命令字段。
    McpServerCommandChanged(String),
    /// Updates the editable MCP server argument text field.
    /// 更新可编辑 MCP 服务器参数文本字段。
    McpServerArgsChanged(String),
    /// Updates the editable MCP server URL field.
    /// 更新可编辑 MCP 服务器 URL 字段。
    McpServerUrlChanged(String),
    /// Updates the editable MCP server bearer token field.
    /// 更新可编辑 MCP 服务器 Bearer Token 字段。
    McpServerBearerTokenChanged(String),
    /// Updates the editable MCP server HTTP headers field.
    /// 更新可编辑 MCP 服务器 HTTP 请求头字段。
    McpServerHeadersChanged(String),
    /// Updates the editable MCP server working-directory field.
    /// 更新可编辑 MCP 服务器工作目录字段。
    McpServerCwdChanged(String),
    /// Updates the editable MCP server environment text field.
    /// 更新可编辑 MCP 服务器环境变量文本字段。
    McpServerEnvChanged(String),
    /// Toggles the editable MCP server enabled flag.
    /// 切换可编辑 MCP 服务器启用标记。
    McpServerEnabledToggled(bool),
    /// Creates a new editable MCP server draft.
    /// 创建一份新的可编辑 MCP 服务器草稿。
    CreateMcpServer,
    /// Persists the currently edited MCP server entry.
    /// 持久化当前编辑中的 MCP 服务器条目。
    SaveMcpServerConfiguration,
    /// Removes the currently selected MCP server entry.
    /// 删除当前选中的 MCP 服务器条目。
    RemoveMcpServerConfiguration,
    /// Applies one freshly loaded LuaSkills catalog snapshot from the embedded host runtime.
    /// 应用一份来自嵌入式宿主运行时的最新 LuaSkills 技能目录快照。
    LuaSkillsCatalogLoaded(Result<LuaSkillsCatalog, String>),
    /// Requests reloading the visible LuaSkills catalog from the embedded host runtime.
    /// 请求从嵌入式宿主运行时重新载入可见 LuaSkills 技能目录。
    RefreshLuaSkillsCatalog,
    /// Updates the install source choice selected in the LuaSkills panel.
    /// 更新 LuaSkills 面板中当前选中的安装来源类型。
    LuaSkillsInstallSourceChoiceSelected(LuaSkillsInstallSourceChoice),
    /// Updates the optional explicit skill id field shown in the LuaSkills install form.
    /// 更新 LuaSkills 安装表单中显示的可选显式技能标识符字段。
    LuaSkillsInstallSkillIdChanged(String),
    /// Updates the raw source locator field shown in the LuaSkills install form.
    /// 更新 LuaSkills 安装表单中显示的原始来源定位字段。
    LuaSkillsInstallSourceChanged(String),
    /// Requests installing one LuaSkills package into the USER root through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时向 USER 根安装一个 LuaSkills 包。
    InstallLuaSkill,
    /// Requests updating one managed LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时更新一个受管 LuaSkills 包。
    UpdateLuaSkill(String),
    /// Requests enabling one disabled LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时启用一个已停用的 LuaSkills 包。
    EnableLuaSkill(String),
    /// Requests disabling one enabled LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时停用一个已启用的 LuaSkills 包。
    DisableLuaSkill(String),
    /// Requests uninstalling one ordinary LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时卸载一个普通 LuaSkills 包。
    UninstallLuaSkill(String),
    /// Applies the finished result of one host-side LuaSkills management action.
    /// 应用单次宿主侧 LuaSkills 管理动作的完成结果。
    LuaSkillsManagementFinished(Result<LuaSkillsManagementReport, String>),
    /// Application theme selected from the pick list.
    /// 从下拉选择框中选择了应用主题。
    ThemeSelected(Theme),
    /// Applies one read-only text editor action to the inspector guide Markdown buffer.
    /// 向检查面板指引 Markdown 缓冲区应用一次只读文本编辑器动作。
    QuickGuideMarkdownAction(text_editor::Action),
    /// Execution mode selected from the radio group.
    /// 从单选组中选择了执行模式。
    ExecutionModeSelected(ExecutionMode),
    /// Temperature slider changed.
    /// 温度滑块发生变化。
    TemperatureChanged(f32),
    /// Temperature override toggle changed.
    /// 温度覆盖开关被切换。
    TemperatureEnabledToggled(bool),
    /// Plan-first workflow checkbox toggled.
    /// 先计划后执行复选框被切换。
    GeneratePlanToggled(bool),
    /// cargo check checkbox toggled.
    /// cargo check 复选框被切换。
    RunCargoCheckToggled(bool),
    /// Persist memory checkbox toggled.
    /// 持久化记忆复选框被切换。
    PersistMemoryToggled(bool),
    /// Auto-approve toggler changed.
    /// 自动批准开关被切换。
    AutoApproveToggled(bool),
    /// Session selected from the navigator.
    /// 从导航区选择了某个会话。
    SessionSelected(usize),
    /// Switches the unified transcript panel into the structured selectable-document mode.
    /// 将统一转录面板切换到结构化可选文档模式。
    ShowSelectedSessionDocumentStructured,
    /// Switches the unified transcript panel into the raw-source selectable-document mode.
    /// 将统一转录面板切换到原始源码可选文档模式。
    ShowSelectedSessionDocumentRawSource,
    /// Switches the transcript panel into the segmented rendered-preview mode.
    /// 将转录面板切换到分段渲染预览模式。
    ShowSelectedSessionDocumentDetailCards,
    /// Toggles one inline thinking block rendered inside the selectable transcript surface.
    /// 切换可选择转录表面内部的某个内联思考区块。
    ToggleSelectedSessionThinkingInline(String),
    /// Opens one inline thinking block in the centered modal surface.
    /// 在居中弹层表面中打开某个内联思考区块。
    OpenSelectedSessionThinkingModal(String),
    /// Commits one previously scheduled inline-thinking toggle after the double-click grace window has elapsed.
    /// 在双击宽限窗口结束后，正式提交一次先前调度的内联思考切换。
    FlushPendingThinkingInlineToggle,
    /// Starts or completes closing the centered thinking modal.
    /// 启动或完成关闭居中思考弹层。
    CloseSelectedSessionThinkingModal,
    /// Applies one read-only action to the unified structured transcript buffer of the selected session.
    /// 向当前选中会话的统一结构化转录缓冲区应用一次只读动作。
    SelectedSessionDocumentStructuredAction(text_editor::Action),
    /// Applies one read-only action to the unified raw-source transcript buffer of the selected session.
    /// 向当前选中会话的统一原始源码转录缓冲区应用一次只读动作。
    SelectedSessionDocumentRawSourceAction(text_editor::Action),
    /// Records the current scroll viewport of the selected-session transcript panel.
    /// 记录当前选中会话转录面板的滚动视口状态。
    SelectedSessionTranscriptScrolled {
        /// Session identifier owning the visible transcript panel.
        /// 拥有当前可见转录面板的会话标识。
        session_id: usize,
        /// Latest viewport reported by the visible transcript scrollable.
        /// 当前可见转录滚动容器回传的最新视口。
        viewport: scrollable::Viewport,
    },
    /// Updates the active drag auto-scroll request emitted by the main rendered transcript surface.
    /// 更新主渲染转录表面发出的当前拖拽自动滚动请求。
    SelectedSessionTranscriptDragAutoScrollChanged {
        /// Session identifier owning the main rendered transcript surface.
        /// 拥有主渲染转录表面的会话标识。
        session_id: usize,
        /// Latest drag auto-scroll request, or `None` when the pointer has returned inside the viewport.
        /// 最新拖拽自动滚动请求；当指针回到视口内时为 `None`。
        request: Option<SelectableTranscriptDragAutoScroll>,
    },
    /// Updates the active drag auto-scroll request emitted by the floating thinking overlay body.
    /// 更新流式思考浮层正文发出的当前拖拽自动滚动请求。
    SelectedSessionThinkingOverlayDragAutoScrollChanged {
        /// Session identifier owning the floating thinking overlay.
        /// 拥有流式思考浮层的会话标识。
        session_id: usize,
        /// Latest drag auto-scroll request, or `None` when the pointer has returned inside the viewport.
        /// 最新拖拽自动滚动请求；当指针回到视口内时为 `None`。
        request: Option<SelectableTranscriptDragAutoScroll>,
    },
    /// Drives one drag auto-scroll tick for the main rendered transcript surface.
    /// 驱动主渲染转录表面的一次拖拽自动滚动 Tick。
    SelectedSessionTranscriptDragAutoScrollTick,
    /// Records the current scroll viewport of the floating streaming-thinking overlay.
    /// 记录流式思考浮层的当前滚动视口状态。
    SelectedSessionThinkingOverlayScrolled {
        /// Session identifier owning the visible floating thinking overlay.
        /// 拥有当前可见流式思考浮层的会话标识。
        session_id: usize,
        /// Latest viewport reported by the floating thinking scrollable.
        /// 当前流式思考滚动容器回传的最新视口。
        viewport: scrollable::Viewport,
    },
    /// Drives one drag auto-scroll tick for the floating thinking overlay body.
    /// 驱动流式思考浮层正文的一次拖拽自动滚动 Tick。
    SelectedSessionThinkingOverlayDragAutoScrollTick,
    /// Drives one centered thinking-modal animation tick while it is opening or closing.
    /// 在居中思考弹层打开或关闭期间驱动一次动画 Tick。
    ThinkingModalAnimationTick,
    /// Applies one read-only text editor action to one inspector activity-log buffer.
    /// 向某个检查面板活动日志缓冲区应用一次只读文本编辑器动作。
    ActivityLogRawAction(usize, text_editor::Action),
    /// Create-session action requested.
    /// 请求创建新会话。
    CreateSession,
    /// Updates the hovered project node inside the main task-drawer tree.
    /// 更新主任务抽屉树中当前悬停的项目节点。
    TaskProjectHovered(Option<String>),
    /// Updates the drag-drop preview target inside one project row using local pointer movement.
    /// 使用项目行内部的局部鼠标移动更新拖拽插入目标预览。
    TaskProjectDropPreviewMoved(String, iced::Point),
    /// Updates the drop-target preview near the tail of one project section while dragging.
    /// 在拖拽期间更新某个项目分区尾部附近的插入目标预览。
    TaskProjectSectionTailHovered(Option<bool>),
    /// Updates the hovered session row inside the main task-drawer tree.
    /// 更新主任务抽屉树中当前悬停的会话行。
    TaskSessionHovered(Option<usize>),
    /// Starts one click-or-drag interaction from a project row inside the main task-drawer tree.
    /// 从主任务抽屉树中的某个项目行启动一次点击或拖拽交互。
    StartTaskProjectInteraction(String),
    /// Opens one project directory with the host operating system.
    /// 使用宿主操作系统打开某个项目目录。
    OpenTaskProjectDirectory(String),
    /// Creates a new session under one project node inside the main task-drawer tree.
    /// 在主任务抽屉树中的某个项目节点下创建新会话。
    CreateTaskProjectSession(String),
    /// Adds one new project node by picking a folder from the host file system.
    /// 通过从宿主文件系统选择目录来新增一个项目节点。
    AddTaskProjectFromFolder,
    /// Pins or unpins one project node inside the main task-drawer tree.
    /// 置顶或取消置顶主任务抽屉树中的某个项目节点。
    PinTaskProject(String),
    /// Renames one project node inside the main task-drawer tree using a lightweight placeholder flow.
    /// 使用轻量占位流程重命名主任务抽屉树中的某个项目节点。
    RenameTaskProject(String),
    /// Archives one project node inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个项目节点。
    ArchiveTaskProject(String),
    /// Restores one archived project node back into the visible main task-drawer tree.
    /// 将一个已归档项目节点恢复回可见的主任务抽屉树。
    RestoreTaskProject(String),
    /// Renames one session row inside the main task-drawer tree using a lightweight placeholder flow.
    /// 使用轻量占位流程重命名主任务抽屉树中的某个会话行。
    RenameTaskSession(usize),
    /// Archives one session row inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个会话行。
    ArchiveTaskSession(usize),
    /// Restores one archived session row back into its visible owning project.
    /// 将一个已归档会话行恢复回其可见的所属项目。
    RestoreTaskSession(usize),
    /// Workspace scan action requested.
    /// 请求扫描工作区。
    ScanWorkspace,
    /// Toggles one file section in the diff panel.
    /// 切换 diff 面板中单个文件区块的折叠状态。
    ToggleDiffFile(String),
    /// Applies a read-only text editor action to one diff file editor.
    /// 向某个 diff 文件编辑器应用只读文本编辑器动作。
    DiffCodeAction(String, text_editor::Action),
    /// Records the current viewport of the structured diff review flow.
    /// 记录结构化 diff 审查流的当前视口状态。
    DiffPanelScrolled(scrollable::Viewport),
    /// Toggles one diff file card and then scrolls back to its header position.
    /// 切换某个 diff 文件卡片后回滚到它的头部位置。
    ToggleAndFocusDiffFile(String),
    /// Updates the currently hovered visual line inside one diff hunk body.
    /// 更新某个 diff hunk 主体内当前悬停的可视行。
    DiffCodeHoverMoved(String, usize),
    /// Clears the hovered visual line when the cursor leaves one diff hunk body.
    /// 当光标离开某个 diff hunk 主体时清除悬停可视行。
    DiffCodeHoverExited(String),
    /// Primary execution action requested.
    /// 请求执行主任务动作。
    StartRun,
    /// Applies one incremental execution-stream event returned by the async runtime stream.
    /// 应用异步运行时流返回的一条增量执行事件。
    RunExecutionStreamEvent {
        /// Monotonic execution-stream identifier used to discard stale events after stop or restart.
        /// 用于在停止或重启后丢弃过期事件的单调执行流标识。
        run_id: u64,
        /// Concrete incremental event emitted by the async runtime stream.
        /// 由异步运行时流发出的具体增量事件。
        event: ExecutionStreamEvent,
    },
    /// Stop action requested.
    /// 请求停止当前动作。
    StopRun,
    /// Pane received focus through click.
    /// 面板通过点击获得焦点。
    PaneFocused(pane_grid::Pane),
    /// Pane grid split was resized.
    /// 分栏网格的分割线被调整。
    PaneResized(pane_grid::ResizeEvent),
}

/// Icon kinds used by the hover action buttons inside the task-drawer tree.
/// 任务抽屉树悬停动作按钮使用的图标类型。
#[derive(Debug, Clone, Copy)]
enum TaskTreeActionIcon {
    /// Adds one new project node by selecting a folder from disk.
    /// 通过从磁盘选择文件夹来新增一个项目节点。
    AddProjectFromFolder,
    /// Creates a new child session under the current project node.
    /// 在当前项目节点下创建新的子会话。
    AddSession,
    /// Archives the current session row.
    /// 归档当前会话行。
    Archive,
}

/// Loaded diff preview payload for the testing region.
/// 用于测试区域的已加载 diff 预览载荷。
#[derive(Debug, Clone)]
struct DiffPreviewPayload {
    /// Footer and toast-like status message after the scan finishes.
    /// 扫描完成后展示在底栏中的状态消息。
    status_message: String,
    /// Human-readable summary for the diff preview header.
    /// 供 diff 预览头部展示的人类可读摘要。
    summary: String,
    /// Structured diff panel payload used by the workbench.
    /// 提供给工作台使用的结构化 diff 面板载荷。
    panel: WorkspaceDiffPanel,
    /// Number of changed entries detected in the workspace.
    /// 在工作区中检测到的变更条目数量。
    changed_files: usize,
    /// Activity level used to push the scan result into the runtime log.
    /// 用于将扫描结果写入运行日志的活动级别。
    activity_level: ActivityLevel,
}

impl VulcanWorkbench {
    /// Builds the initial state, seeded UI data, and pane layout.
    /// 构建初始状态、种子界面数据与面板布局。
    fn new() -> (Self, Task<Message>) {
        // Create the fixed three-column workbench layout for the first prototype.
        // 为首版原型创建固定的三栏工作台布局。
        let panes = pane_grid::State::with_configuration(pane_layout());

        // Resolve the default focused pane so the center workbench is active at launch.
        // 解析默认焦点面板，确保启动时中央工作台处于激活状态。
        let focus = panes
            .iter()
            .find(|(_, kind)| **kind == PaneKind::Workbench)
            .map(|(pane, _)| *pane);

        // Load the persisted provider configuration before building the inspector so startup state matches disk.
        // 在构建检查面板前载入持久化供应商配置，确保启动状态与磁盘内容保持一致。
        let (mut provider_configuration, mut provider_notice) =
            match provider_config::load_configuration_file() {
                Ok(config) => (
                    config,
                    String::from("已从 `~/.vulcan/vulcan-code.json` 载入供应商配置。"),
                ),
                Err(error) => (
                    ProviderConfigurationFile::default(),
                    format!("供应商配置载入失败，已回退到空配置：{error}"),
                ),
            };
        let runtime_defaults_changed =
            agent_runtime::ensure_runtime_defaults(&mut provider_configuration);
        let mut agent_profile_catalog = match agent_profile::load_agent_profile_catalog() {
            Ok(catalog) => {
                provider_notice.push_str(" 已从用户目录载入 Agent 配置。");
                catalog
            }
            Err(error) => {
                provider_notice.push_str(&format!(
                    " 用户 Agent 配置载入失败，已回退到系统默认：{error}"
                ));
                AgentProfileCatalog::default()
            }
        };
        let legacy_agent_migrated = match agent_profile::migrate_legacy_provider_agents(
            &provider_configuration,
            &mut agent_profile_catalog,
        ) {
            Ok(changed) => {
                if changed {
                    provider_notice
                        .push_str(" 已将旧版合并在 Provider 配置中的 Agent 定义迁移到用户目录。");
                }
                changed
            }
            Err(error) => {
                provider_notice.push_str(&format!(
                    " 旧版 Agent 配置迁移失败，后续仅使用系统默认：{error}"
                ));
                false
            }
        };
        if runtime_defaults_changed || legacy_agent_migrated {
            match provider_config::save_configuration_file(&provider_configuration) {
                Ok(()) => {
                    if runtime_defaults_changed {
                        provider_notice.push_str(" 已自动补齐 rig/compose/mcp 基础架构默认配置。");
                    }
                    if legacy_agent_migrated {
                        provider_notice.push_str(" 旧版 Provider Agent 字段已从主配置文件中清退。");
                    }
                }
                Err(error) => {
                    provider_notice.push_str(&format!(
                        " 运行时配置调整已写入内存，但保存到磁盘失败：{error}"
                    ));
                }
            }
        }
        let controller_configuration = match provider_config::load_controller_configuration_file() {
            Ok(configuration) => {
                provider_notice.push_str(" 已从运行时 config/controller.json 载入 vldb 基础配置。");
                configuration
            }
            Err(error) => {
                provider_notice
                    .push_str(&format!(" 控制器基础配置载入失败，已回退到默认值：{error}"));
                PersistedControllerConfiguration::default()
            }
        };
        let supported_provider_choices = provider_config::supported_provider_choices();
        let selected_provider = Self::resolve_initial_provider_choice(
            &provider_configuration,
            &supported_provider_choices,
        );
        let provider_form = provider_configuration
            .provider
            .get(&selected_provider.id)
            .map(|entry| ProviderFormState::from_options(&entry.options))
            .unwrap_or_default();
        let selected_mcp_server_id = Self::resolve_initial_mcp_server_id(&provider_configuration);
        let mcp_server_form = selected_mcp_server_id
            .as_deref()
            .and_then(|server_id| provider_configuration.mcp.servers.get(server_id))
            .map(McpServerFormState::from_entry)
            .unwrap_or_default();
        let runtime_foundation =
            RuntimeFoundation::build(&provider_configuration, &agent_profile_catalog, None, None);

        let activities = ActivityEntry::seeded();
        let activity_raw_buffers = activities
            .iter()
            .map(|entry| text_editor::Content::with_text(&entry.detail))
            .collect::<Vec<_>>();
        let placeholder_diff_panel = WorkspaceDiffPanel::placeholder();
        let (main_window_id, open_main_window) = window::open(main_window_settings());
        let app_theme = theme::default_theme();
        let quick_guide = markdown::Content::parse(QUICK_GUIDE_MARKDOWN);
        let quick_guide_markdown_buffer = text_editor::Content::with_text(
            &build_selectable_markdown_text(QUICK_GUIDE_MARKDOWN, &quick_guide, &app_theme),
        );

        let startup_controller_configuration = controller_configuration.clone();

        (
            Self {
                main_window_id: Some(main_window_id),
                diff_window_id: None,
                main_window_size: main_window_settings().size,
                main_window_maximized: main_window_settings().maximized,
                diff_window_size: diff_window_settings().size,
                diff_window_maximized: diff_window_settings().maximized,
                main_window_cursor_position: None,
                title_bar_pointer_position: None,
                pending_title_bar_drag: None,
                main_task_panel_width: MAIN_TASK_PANEL_DEFAULT_WIDTH,
                main_task_panel_expanded: true,
                pending_main_task_panel_resize: None,
                main_task_panel_resize_handle_hovered: false,
                panes,
                focus,
                workspace_path: String::from("D:/openvulcan/vulcan-code"),
                composer: String::from("为 Rust + iced AI Coding 客户端搭建可扩展的桌面基础设施。"),
                editor_content: text_editor::Content::with_text(SAMPLE_EDITOR_CONTENT),
                diff_summary: placeholder_diff_panel.summary_text(),
                diff_panel: placeholder_diff_panel,
                collapsed_diff_files: BTreeSet::new(),
                diff_code_buffers: BTreeMap::new(),
                diff_wrapped_hunks: BTreeMap::new(),
                diff_buffer_slices: BTreeMap::new(),
                hovered_diff_line: None,
                diff_scroll_offset: 0.0,
                diff_viewport_height: 0.0,
                quick_guide_markdown_buffer,
                sessions: Vec::new(),
                task_projects: Vec::new(),
                archived_task_projects: Vec::new(),
                expanded_task_projects: BTreeSet::new(),
                hovered_task_project: None,
                hovered_task_session: None,
                active_task_project_drop_target: None,
                suppress_task_session_hover: false,
                pending_task_project_interaction: None,
                dragging_task_project: None,
                selected_session: 0,
                activities,
                activity_raw_buffers,
                session_messages: BTreeMap::new(),
                archived_sessions: Vec::new(),
                archived_session_messages: BTreeMap::new(),
                session_compactions: BTreeMap::new(),
                archived_session_compactions: BTreeMap::new(),
                session_runtime_state: BTreeMap::new(),
                archived_session_runtime_state: BTreeMap::new(),
                session_runtime_events: BTreeMap::new(),
                collapsed_runtime_event_ids: BTreeMap::new(),
                markdown_session_message_ids: BTreeMap::new(),
                markdown_runtime_event_ids: BTreeMap::new(),
                session_message_raw_buffers: BTreeMap::new(),
                session_message_markdown: BTreeMap::new(),
                session_message_markdown_buffers: BTreeMap::new(),
                streaming_thinking_message_ids: BTreeMap::new(),
                floating_streaming_thinking_sessions: BTreeSet::new(),
                session_streaming_thinking_follow_tail: BTreeMap::new(),
                active_thinking_modal: None,
                pending_thinking_inline_toggle: None,
                streaming_output_message_ids: BTreeMap::new(),
                runtime_event_raw_buffers: BTreeMap::new(),
                runtime_event_markdown: BTreeMap::new(),
                runtime_event_markdown_buffers: BTreeMap::new(),
                session_agent_view_buffers: BTreeMap::new(),
                session_agent_view_raw_buffers: BTreeMap::new(),
                session_agent_view_selectable_documents: BTreeMap::new(),
                session_agent_view_documents: BTreeMap::new(),
                session_agent_view_block_collapsed: BTreeMap::new(),
                session_agent_view_follow_tail: BTreeMap::new(),
                session_agent_view_viewports: BTreeMap::new(),
                active_transcript_drag_auto_scroll: None,
                session_streaming_thinking_viewports: BTreeMap::new(),
                active_thinking_overlay_drag_auto_scroll: None,
                agent_transcript_surface_mode: AgentTranscriptSurfaceMode::DetailCards,
                session_run_phases: BTreeMap::new(),
                next_run_id: 1,
                active_run_id: None,
                active_run_session_id: None,
                active_run_handle: None,
                next_session_id: 1,
                startup_lifecycle: StartupLifecycle::Bootstrapping {
                    message: String::from("正在初始化 vldb 数据平面，请稍候。"),
                },
                controller_runtime: None,
                luaskills_runtime: None,
                mcp_runtime: None,
                luaskills_catalog: None,
                luaskills_notice: String::from("LuaSkills 面板尚未加载技能目录。"),
                luaskills_install_source_choice: LuaSkillsInstallSourceChoice::Github,
                luaskills_install_skill_id: String::new(),
                luaskills_install_source: String::new(),
                luaskills_operation_in_flight: false,
                provider_configuration,
                agent_profile_catalog,
                controller_configuration,
                runtime_foundation,
                session_skill_chain: SessionDerivedSkillChain::default(),
                active_main_panel: MainDisplayPanel::Main,
                selected_provider,
                provider_form,
                provider_notice,
                mcp_notice: if selected_mcp_server_id.is_some() {
                    String::from("已载入 MCP 服务器配置。")
                } else {
                    String::from("尚未配置 MCP 服务器，可在此新增一个 stdio 或 http 服务器。")
                },
                selected_mcp_server_id,
                mcp_server_form,
                last_execution_preparation: None,
                last_mcp_discovery_rows: Vec::new(),
                last_mcp_discovery_errors: Vec::new(),
                execution_mode: ExecutionMode::Plan,
                app_theme,
                temperature_enabled: false,
                temperature: 0.25,
                context_usage: 42.0,
                generate_plan: true,
                run_cargo_check: true,
                persist_memory: true,
                auto_approve: false,
                status_message: String::from("正在初始化 vldb 数据平面。"),
            },
            Task::batch([
                open_main_window.map(Message::MainWindowOpened),
                Task::perform(
                    data_plane::bootstrap_application_data_plane(startup_controller_configuration),
                    Message::StartupBootstrapFinished,
                ),
            ]),
        )
    }

    /// Handles widget messages and mutates the application state.
    /// 处理控件消息并更新应用状态。
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::BeginStartupBootstrap => {
                self.startup_lifecycle = StartupLifecycle::Bootstrapping {
                    message: String::from("正在初始化 vldb 数据平面，请稍候。"),
                };
                self.status_message = String::from("正在初始化 vldb 数据平面。");
                self.controller_runtime = None;
                self.luaskills_runtime = None;
                if let Some(handle) = self.mcp_runtime.take() {
                    handle.shutdown();
                }
                self.luaskills_catalog = None;
                self.luaskills_notice = String::from("LuaSkills 面板尚未加载技能目录。");
                self.luaskills_install_source_choice = LuaSkillsInstallSourceChoice::Github;
                self.luaskills_install_skill_id.clear();
                self.luaskills_install_source.clear();
                self.luaskills_operation_in_flight = false;
                self.task_projects.clear();
                self.archived_task_projects.clear();
                self.sessions.clear();
                self.session_messages.clear();
                self.archived_sessions.clear();
                self.archived_session_messages.clear();
                self.session_compactions.clear();
                self.archived_session_compactions.clear();
                self.session_runtime_state.clear();
                self.archived_session_runtime_state.clear();
                self.session_runtime_events.clear();
                self.collapsed_runtime_event_ids.clear();
                self.markdown_session_message_ids.clear();
                self.markdown_runtime_event_ids.clear();
                self.session_message_raw_buffers.clear();
                self.session_message_markdown.clear();
                self.session_message_markdown_buffers.clear();
                self.streaming_thinking_message_ids.clear();
                self.floating_streaming_thinking_sessions.clear();
                self.session_streaming_thinking_follow_tail.clear();
                self.active_thinking_modal = None;
                self.pending_thinking_inline_toggle = None;
                self.streaming_output_message_ids.clear();
                self.runtime_event_raw_buffers.clear();
                self.runtime_event_markdown.clear();
                self.runtime_event_markdown_buffers.clear();
                self.session_agent_view_buffers.clear();
                self.session_agent_view_raw_buffers.clear();
                self.session_agent_view_selectable_documents.clear();
                self.session_agent_view_documents.clear();
                self.session_agent_view_block_collapsed.clear();
                self.session_agent_view_follow_tail.clear();
                self.session_agent_view_viewports.clear();
                self.session_streaming_thinking_viewports.clear();
                self.active_transcript_drag_auto_scroll = None;
                self.active_thinking_overlay_drag_auto_scroll = None;
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::DetailCards;
                self.session_run_phases.clear();
                self.expanded_task_projects.clear();
                self.selected_session = 0;
                self.next_session_id = 1;

                Task::perform(
                    data_plane::bootstrap_application_data_plane(
                        self.controller_configuration.clone(),
                    ),
                    Message::StartupBootstrapFinished,
                )
            }
            Message::StartupBootstrapFinished(result) => {
                match result {
                    Ok(report) => {
                        self.status_message = report.summary_text();
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Success,
                            title: String::from("vldb 数据平面就绪"),
                            detail: format!(
                                "控制器端点 {} 已连接，主 SQLite 空间已初始化到 {}。",
                                report.handle.endpoint,
                                report.handle.sqlite_path.display()
                            ),
                        });
                        self.controller_runtime = Some(report.handle);
                        self.startup_lifecycle = StartupLifecycle::Bootstrapping {
                            message: String::from("正在初始化 LuaSkills 运行时，请稍候。"),
                        };
                        self.status_message = String::from("正在初始化 LuaSkills 运行时。");

                        return Task::perform(
                            luaskills_runtime::bootstrap_luaskills_runtime(
                                self.controller_configuration.clone(),
                            ),
                            Message::LuaSkillsBootstrapFinished,
                        );
                    }
                    Err(error) => {
                        self.status_message = format!("vldb 数据平面初始化失败：{error}");
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Warning,
                            title: String::from("vldb 数据平面初始化失败"),
                            detail: error.clone(),
                        });
                        self.controller_runtime = None;
                        self.startup_lifecycle = StartupLifecycle::Failed { message: error };
                    }
                }

                Task::none()
            }
            Message::LuaSkillsBootstrapFinished(result) => {
                match result {
                    Ok(report) => {
                        self.status_message = report.summary_text();
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Success,
                            title: String::from("LuaSkills 运行时就绪"),
                            detail: format!(
                                "运行时根目录 {} 已准备完成，已挂载 {} 个技能根并发现 {} 个入口。",
                                report.handle.runtime_root.display(),
                                report.handle.skill_roots.len(),
                                report.handle.entry_count
                            ),
                        });
                        let luaskills_handle = report.handle.clone();
                        self.luaskills_runtime = Some(report.handle);
                        self.luaskills_operation_in_flight = true;
                        self.luaskills_notice =
                            String::from("LuaSkills 运行时已就绪，正在载入技能目录。");
                        self.startup_lifecycle = StartupLifecycle::Ready;
                        self.status_message =
                            String::from("LuaSkills 运行时已就绪，正在后台加载工作台数据。");

                        if let Some(controller_runtime) = self.controller_runtime.clone() {
                            return Task::batch([
                                Task::perform(
                                    workspace_store::load_or_seed_workspace(controller_runtime),
                                    Message::WorkspaceStateLoaded,
                                ),
                                Task::perform(
                                    bootstrap_mcp_runtime(self.provider_configuration.clone()),
                                    Message::McpRuntimeBootstrapped,
                                ),
                                Task::perform(
                                    luaskills_runtime::load_luaskills_catalog(luaskills_handle),
                                    Message::LuaSkillsCatalogLoaded,
                                ),
                            ]);
                        }

                        self.startup_lifecycle = StartupLifecycle::Failed {
                            message: String::from("控制器运行时缺失，无法加载工作台数据。"),
                        };
                    }
                    Err(error) => {
                        self.status_message = format!("LuaSkills 运行时初始化失败：{error}");
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Warning,
                            title: String::from("LuaSkills 运行时初始化失败"),
                            detail: error.clone(),
                        });
                        self.luaskills_runtime = None;
                        self.luaskills_catalog = None;
                        self.luaskills_operation_in_flight = false;
                        self.startup_lifecycle = StartupLifecycle::Failed { message: error };
                    }
                }

                Task::none()
            }
            Message::WorkspaceStateLoaded(result) => {
                match result {
                    Ok(snapshot) => {
                        self.apply_workspace_snapshot(snapshot);
                        self.startup_lifecycle = StartupLifecycle::Ready;
                        self.status_message = String::from("工作台数据已加载完成。");
                        self.push_activity(
                            ActivityLevel::Success,
                            "工作台数据已加载",
                            format!(
                                "已恢复 {} 个项目、{} 个会话与 {} 个会话消息集合。",
                                self.task_projects.len(),
                                self.sessions.len(),
                                self.session_messages.len()
                            ),
                        );
                    }
                    Err(error) => {
                        self.status_message = format!("工作台数据加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台数据加载失败", error);
                    }
                }

                Task::none()
            }
            Message::WorkspaceStatePersisted(result) => {
                if let Err(error) = result {
                    self.status_message = format!("工作台状态保存失败：{error}");
                    self.push_activity(ActivityLevel::Warning, "工作台状态保存失败", error);
                }

                Task::none()
            }
            Message::LuaSkillsCatalogLoaded(result) => {
                self.luaskills_operation_in_flight = false;

                match result {
                    Ok(catalog) => {
                        let visible_skill_count = catalog.skills.len();
                        let enabled_skill_count =
                            catalog.skills.iter().filter(|skill| skill.enabled).count();
                        self.luaskills_notice = format!(
                            "LuaSkills 目录已载入：可见技能 {} 个，启用 {} 个，运行时入口 {} 个。",
                            visible_skill_count, enabled_skill_count, catalog.entry_count
                        );
                        self.status_message = self.luaskills_notice.clone();
                        self.luaskills_catalog = Some(catalog);
                    }
                    Err(error) => {
                        self.luaskills_catalog = None;
                        self.luaskills_notice = format!("LuaSkills 技能目录载入失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(
                            ActivityLevel::Warning,
                            "LuaSkills 技能目录载入失败",
                            error,
                        );
                    }
                }

                Task::none()
            }
            Message::McpRuntimeBootstrapped(result) => {
                match result {
                    Ok(handle) => {
                        self.mcp_runtime = Some(handle);
                        self.refresh_runtime_foundation();
                        self.mcp_notice =
                            String::from("MCP 运行时已就绪，后续执行将直接注册实时可用工具。");
                    }
                    Err(error) => {
                        self.mcp_runtime = None;
                        self.refresh_runtime_foundation();
                        self.mcp_notice = format!("MCP 运行时初始化失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "MCP 运行时初始化失败", error);
                    }
                }

                Task::none()
            }
            Message::RefreshLuaSkillsCatalog => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法刷新。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = String::from("正在刷新 LuaSkills 技能目录。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Refresh,
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::LuaSkillsInstallSourceChoiceSelected(choice) => {
                self.luaskills_install_source_choice = choice;
                Task::none()
            }
            Message::LuaSkillsInstallSkillIdChanged(value) => {
                self.luaskills_install_skill_id = value;
                Task::none()
            }
            Message::LuaSkillsInstallSourceChanged(value) => {
                self.luaskills_install_source = value;
                Task::none()
            }
            Message::InstallLuaSkill => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法安装技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                if self.luaskills_install_source.trim().is_empty() {
                    self.luaskills_notice =
                        String::from("请先填写安装来源，例如 GitHub 仓库或包地址。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                }

                let action = LuaSkillsManagementAction::Install {
                    skill_id: if self.luaskills_install_skill_id.trim().is_empty() {
                        None
                    } else {
                        Some(self.luaskills_install_skill_id.clone())
                    },
                    source: self.luaskills_install_source.clone(),
                    source_type: self.luaskills_install_source_choice,
                };
                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = String::from("正在安装 LuaSkills 技能。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(handle, action),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::UpdateLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法更新技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = format!("正在更新技能 `{skill_id}`。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Update { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::EnableLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法启用技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = format!("正在启用技能 `{skill_id}`。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Enable { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::DisableLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法停用技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = format!("正在停用技能 `{skill_id}`。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Disable { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::UninstallLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法卸载技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.luaskills_notice = format!("正在卸载技能 `{skill_id}`。");
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Uninstall { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::LuaSkillsManagementFinished(result) => {
                self.luaskills_operation_in_flight = false;

                match result {
                    Ok(report) => {
                        self.luaskills_notice = report.message.clone();
                        self.status_message = report.message.clone();
                        self.luaskills_catalog = Some(report.catalog);
                        self.push_activity(
                            ActivityLevel::Success,
                            "LuaSkills 管理操作已完成",
                            report.message,
                        );
                    }
                    Err(error) => {
                        self.luaskills_notice = format!("LuaSkills 管理操作失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(ActivityLevel::Warning, "LuaSkills 管理操作失败", error);
                    }
                }

                Task::none()
            }
            Message::MainWindowOpened(window_id) => {
                self.main_window_id = Some(window_id);
                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::DiffWindowOpened(window_id) => {
                self.diff_window_id = Some(window_id);
                self.rebuild_diff_code_buffers_for_current_wrap();
                self.status_message = String::from("Diff 预览窗口已打开，可独立调整面板结构。");
                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::OpenDiffWindow => {
                // Reuse the existing diff preview window when it is already alive to avoid duplicate reviewers.
                // 当 Diff 预览窗口已存在时复用该窗口，避免产生重复审查窗口。
                if let Some(window_id) = self.diff_window_id {
                    window::gain_focus(window_id)
                } else {
                    let (window_id, open_diff_window) = window::open(diff_window_settings());
                    self.diff_window_id = Some(window_id);
                    self.status_message = String::from("正在打开独立 Diff 预览窗口。");

                    open_diff_window.map(Message::DiffWindowOpened)
                }
            }
            Message::WindowTitleBarPressed(window_id) => {
                self.pending_title_bar_drag = self
                    .title_bar_pointer_position
                    .filter(|pointer| pointer.window_id == window_id)
                    .map(|pointer| PendingWindowTitleBarDrag {
                        window_id,
                        origin: pointer.position,
                    });
                Task::none()
            }
            Message::WindowTitleBarPointerMoved(window_id, position) => {
                self.title_bar_pointer_position = Some(WindowTitleBarPointer {
                    window_id,
                    position,
                });

                if let Some(pending_drag) = self.pending_title_bar_drag {
                    if pending_drag.window_id == window_id {
                        let delta_x = position.x - pending_drag.origin.x;
                        let delta_y = position.y - pending_drag.origin.y;
                        let moved_far_enough = delta_x.abs() >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                            || delta_y.abs() >= WINDOW_TITLE_BAR_DRAG_THRESHOLD;

                        if moved_far_enough {
                            self.pending_title_bar_drag = None;
                            return window::drag(window_id);
                        }
                    }
                }

                Task::none()
            }
            Message::WindowTitleBarReleased(window_id) => {
                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }
                Task::none()
            }
            Message::WindowTitleBarExited(window_id) => {
                if self
                    .title_bar_pointer_position
                    .is_some_and(|pointer| pointer.window_id == window_id)
                {
                    self.title_bar_pointer_position = None;
                }

                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }

                Task::none()
            }
            Message::MinimizeWindow(window_id) => window::minimize(window_id, true),
            Message::StartWindowResize(window_id, direction) => {
                window::drag_resize(window_id, direction)
            }
            Message::ToggleMaximizeWindow(window_id) => {
                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }

                window::toggle_maximize(window_id)
            }
            Message::CloseWindow(window_id) => window::close(window_id),
            Message::ToggleMainTaskPanel => {
                self.main_task_panel_expanded = !self.main_task_panel_expanded;
                self.pending_main_task_panel_resize = None;
                self.main_task_panel_resize_handle_hovered = false;
                Task::none()
            }
            Message::StartMainTaskPanelResize => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_main_task_panel_resize = Some(PendingMainTaskPanelResize {
                        origin_x: position.x,
                        initial_width: self.main_task_panel_width,
                    });
                    self.main_task_panel_resize_handle_hovered = true;
                }
                Task::none()
            }
            Message::MainTaskPanelResizeHandleHovered(is_hovered) => {
                self.main_task_panel_resize_handle_hovered = is_hovered;
                Task::none()
            }
            Message::MainDisplayPanelSelected(panel) => {
                self.active_main_panel = panel;
                self.status_message = match panel {
                    MainDisplayPanel::Main => {
                        String::from("已切换到工作台，当前聚焦任务抽屉与主工作区。")
                    }
                    MainDisplayPanel::PromptSkills => {
                        String::from("已切换到技能（常规）面板，当前显示占位内容。")
                    }
                    MainDisplayPanel::LuaSkills => String::from(
                        "已切换到技能（LuaSkills）面板，当前聚焦技能目录与生命周期管理。",
                    ),
                    MainDisplayPanel::Automation => {
                        String::from("已切换到自动化面板，当前显示占位内容。")
                    }
                    MainDisplayPanel::Agents => {
                        String::from("已切换到代理面板，当前显示占位内容。")
                    }
                    MainDisplayPanel::Plugins => {
                        String::from("已切换到插件面板，当前显示占位内容。")
                    }
                    MainDisplayPanel::VulcanMemoryMesh => {
                        String::from("已切换到 VulcanMemoryMesh 面板，当前显示占位内容。")
                    }
                    MainDisplayPanel::Example => {
                        String::from("已切换到示例面板，主工作区主体现以独立面板形式展示。")
                    }
                    MainDisplayPanel::Settings => {
                        String::from("已切换到设置面板，当前聚焦供应商配置与运行参数。")
                    }
                };

                if panel == MainDisplayPanel::LuaSkills
                    && self.luaskills_catalog.is_none()
                    && !self.luaskills_operation_in_flight
                {
                    if let Some(handle) = self.luaskills_runtime.clone() {
                        self.luaskills_operation_in_flight = true;
                        self.luaskills_notice = String::from("正在载入 LuaSkills 技能目录。");
                        return Task::perform(
                            luaskills_runtime::load_luaskills_catalog(handle),
                            Message::LuaSkillsCatalogLoaded,
                        );
                    }
                }

                Task::none()
            }
            Message::WindowClosed(window_id) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_id = None;
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_id = None;
                    self.status_message = String::from("Diff 预览窗口已关闭。");
                }

                if self.main_window_id.is_some() {
                    self.rebuild_diff_code_buffers_for_current_wrap();
                }

                if self.main_window_id.is_none() && self.diff_window_id.is_none() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Message::WindowResized(window_id, size) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_size = size;
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_size = size;
                }

                if self.main_window_id == Some(window_id) || self.diff_window_id == Some(window_id)
                {
                    self.rebuild_diff_code_buffers_for_current_wrap();
                }

                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::WindowPointerMoved(window_id, position) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_cursor_position = Some(position);

                    if let Some(pending_resize) = self.pending_main_task_panel_resize {
                        let new_width = (pending_resize.initial_width
                            + (position.x - pending_resize.origin_x))
                            .clamp(MAIN_TASK_PANEL_MIN_WIDTH, MAIN_TASK_PANEL_MAX_WIDTH);
                        self.main_task_panel_width = new_width;
                    }

                    if let Some(pending_project_interaction) =
                        self.pending_task_project_interaction.as_ref()
                    {
                        if self.dragging_task_project.is_none()
                            && ((position.x - pending_project_interaction.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_project_interaction.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_task_project =
                                Some(pending_project_interaction.project_id.clone());
                            self.hovered_task_project = None;
                            self.hovered_task_session = None;
                        }
                    }
                }
                Task::none()
            }
            Message::WindowLeftButtonReleased(window_id) => {
                self.clear_transcript_drag_auto_scrolls();
                let mut did_commit_drop = false;
                if self.main_window_id == Some(window_id) {
                    self.pending_main_task_panel_resize = None;

                    let toggled_project_id = self
                        .pending_task_project_interaction
                        .as_ref()
                        .filter(|_| self.dragging_task_project.is_none())
                        .map(|interaction| interaction.project_id.clone());

                    let committed_drop = self
                        .dragging_task_project
                        .clone()
                        .zip(self.active_task_project_drop_target.clone());

                    self.pending_task_project_interaction = None;

                    if let Some((dragged_project_id, drop_target)) = committed_drop {
                        self.apply_task_project_drop_target(&dragged_project_id, &drop_target);
                        did_commit_drop = true;
                    }

                    self.dragging_task_project = None;
                    self.active_task_project_drop_target = None;

                    if let Some(project_id) = toggled_project_id {
                        self.toggle_task_project_expanded(&project_id);
                    }
                }
                if did_commit_drop {
                    self.persist_workspace_snapshot_task()
                } else {
                    Task::none()
                }
            }
            Message::WindowMaximizedStateChanged(window_id, maximized) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_maximized = maximized;
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_maximized = maximized;
                }

                Task::none()
            }
            Message::WorkspacePathChanged(value) => {
                self.workspace_path = value;
                Task::none()
            }
            Message::ComposerChanged(value) => {
                self.composer = value;
                Task::none()
            }
            Message::EditorActionPerformed(action) => {
                self.editor_content.perform(action);
                Task::none()
            }
            Message::ProviderCatalogSelected(provider) => {
                self.selected_provider = provider;
                self.reload_selected_provider_form();
                self.provider_notice = format!(
                    "已切换到供应商：{}。",
                    provider_config::provider_summary(&self.selected_provider.id)
                );
                Task::none()
            }
            Message::ProviderFieldChanged(field, value) => {
                self.provider_form.set(field, value);
                Task::none()
            }
            Message::SaveProviderConfiguration => {
                if let Err(error) = self.save_selected_provider_configuration() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.refresh_runtime_foundation();

                let provider_name = provider_config::provider_summary(&self.selected_provider.id);
                self.provider_notice = format!("已保存 `{provider_name}` 供应商配置。");
                self.status_message = format!("供应商 `{provider_name}` 配置已写入本地文件。");
                Task::none()
            }
            Message::RemoveProviderConfiguration => {
                let provider_id = self.selected_provider.id.clone();
                let provider_name = provider_config::provider_summary(&provider_id);
                self.provider_configuration.provider.remove(&provider_id);

                if self
                    .provider_configuration
                    .model
                    .as_deref()
                    .and_then(provider_config::parse_active_model_reference)
                    .is_some_and(|(active_provider_id, _)| active_provider_id == provider_id)
                {
                    self.provider_configuration.model = None;
                }

                if let Err(error) =
                    provider_config::save_configuration_file(&self.provider_configuration)
                {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.refresh_runtime_foundation();

                self.reload_selected_provider_form();
                self.provider_notice = format!("已移除 `{provider_name}` 供应商配置。");
                self.status_message = format!("供应商 `{provider_name}` 配置已删除。");
                Task::none()
            }
            Message::RefreshProviderModels => {
                let provider_id = self.selected_provider.id.clone();
                let Some(definition) =
                    provider_config::supported_provider_by_id(&provider_id).cloned()
                else {
                    let error = String::from("当前供应商未找到可用定义。");
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                };

                if !definition.supports_remote_model_refresh() {
                    let message =
                        String::from("当前供应商不支持远端模型刷新，将继续使用内置静态模型目录。");
                    self.provider_notice = message.clone();
                    self.status_message = message;
                    return Task::none();
                }

                let options = self.provider_form.to_options();
                self.provider_notice = format!("正在尝试刷新 `{}` 的模型列表。", definition.name);
                let refresh_provider_id = provider_id.clone();

                Task::perform(
                    async move {
                        provider_config::fetch_remote_models(&definition, &options)
                            .await
                            .map(|models| (provider_id, models))
                    },
                    move |result| match result {
                        Ok((provider_id, models)) => {
                            Message::ProviderModelsRefreshed(provider_id, models)
                        }
                        Err(error) => {
                            Message::ProviderModelsRefreshFailed(refresh_provider_id.clone(), error)
                        }
                    },
                )
            }
            Message::ProviderModelsRefreshed(provider_id, models) => {
                let provider_name = provider_config::provider_summary(&provider_id);
                let entry = self
                    .provider_configuration
                    .provider
                    .entry(provider_id.clone())
                    .or_insert_with(PersistedProviderEntry::default);
                entry.name = Some(provider_name.clone());
                entry.options = self.provider_form.to_options();
                entry.models = provider_config::persist_remote_models(&models);

                if let Err(error) =
                    provider_config::save_configuration_file(&self.provider_configuration)
                {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }

                self.provider_notice = format!(
                    "已刷新 `{provider_name}` 的模型列表，共 {} 个模型。",
                    models.len()
                );
                self.status_message = format!("供应商 `{provider_name}` 模型列表已更新。");
                Task::none()
            }
            Message::ProviderModelsRefreshFailed(provider_id, error) => {
                let provider_name = provider_config::provider_summary(&provider_id);
                self.provider_notice =
                    format!("刷新 `{provider_name}` 模型列表失败，将继续使用静态目录：{error}");
                self.status_message = format!("供应商 `{provider_name}` 模型刷新失败。");
                Task::none()
            }
            Message::ActiveModelSelected(model) => {
                self.provider_configuration.model =
                    Some(format!("{}/{}", self.selected_provider.id, model.id));

                if let Err(error) =
                    provider_config::save_configuration_file(&self.provider_configuration)
                {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.refresh_runtime_foundation();

                self.provider_notice = format!("已将 `{}` 设为当前激活模型。", model);
                self.status_message = format!("当前激活模型已切换为 `{}`。", model);
                Task::none()
            }
            Message::ConfiguredProviderSelected(provider_id) => {
                if let Some(choice) = provider_config::supported_provider_choices()
                    .into_iter()
                    .find(|item| item.id == provider_id)
                {
                    self.selected_provider = choice;
                    self.reload_selected_provider_form();
                    self.provider_notice = format!(
                        "已定位到已配置供应商：{}。",
                        provider_config::provider_summary(&provider_id)
                    );
                }
                Task::none()
            }
            Message::ActiveAgentSelected(choice) => {
                let selected_agent_id = if choice.id == SYSTEM_DEFAULT_AGENT_CHOICE_ID {
                    None
                } else if self.agent_profile_catalog.agents.contains_key(&choice.id) {
                    Some(choice.id.clone())
                } else {
                    self.status_message =
                        format!("未找到用户代理 `{}`，无法切换当前激活代理。", choice.label);
                    return Task::none();
                };
                self.agent_profile_catalog.state.selected_agent_id = selected_agent_id;

                if let Err(error) =
                    agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)
                {
                    self.mcp_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }

                self.refresh_runtime_foundation();
                self.status_message = format!("当前激活代理已切换为 `{}`。", choice.label);
                Task::none()
            }
            Message::McpServerSelected(choice) => {
                self.selected_mcp_server_id = Some(choice.id.clone());
                self.reload_selected_mcp_server_form();
                self.mcp_notice = format!("已切换到 MCP 服务器 `{}`。", choice.label);
                Task::none()
            }
            Message::McpServerNameChanged(value) => {
                self.mcp_server_form.name = value;
                Task::none()
            }
            Message::McpServerTransportSelected(transport) => {
                self.mcp_server_form.transport = transport;
                Task::none()
            }
            Message::McpServerCommandChanged(value) => {
                self.mcp_server_form.command = value;
                Task::none()
            }
            Message::McpServerArgsChanged(value) => {
                self.mcp_server_form.args_text = value;
                Task::none()
            }
            Message::McpServerUrlChanged(value) => {
                self.mcp_server_form.url = value;
                Task::none()
            }
            Message::McpServerBearerTokenChanged(value) => {
                self.mcp_server_form.bearer_token = value;
                Task::none()
            }
            Message::McpServerHeadersChanged(value) => {
                self.mcp_server_form.headers_text = value;
                Task::none()
            }
            Message::McpServerCwdChanged(value) => {
                self.mcp_server_form.cwd = value;
                Task::none()
            }
            Message::McpServerEnvChanged(value) => {
                self.mcp_server_form.env_text = value;
                Task::none()
            }
            Message::McpServerEnabledToggled(enabled) => {
                self.mcp_server_form.enabled = enabled;
                Task::none()
            }
            Message::CreateMcpServer => {
                self.selected_mcp_server_id = None;
                self.mcp_server_form = McpServerFormState {
                    enabled: true,
                    ..McpServerFormState::default()
                };
                self.mcp_notice = String::from("已创建新的 MCP 服务器草稿，请填写名称与连接信息。");
                Task::none()
            }
            Message::SaveMcpServerConfiguration => {
                match self.save_selected_mcp_server_configuration() {
                    Ok(server_id) => {
                        self.selected_mcp_server_id = Some(server_id);
                        self.reload_selected_mcp_server_form();
                        self.mcp_notice = String::from("MCP 服务器配置已保存。");
                        self.status_message = String::from("MCP 服务器配置已写入本地文件。");
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::RemoveMcpServerConfiguration => {
                match self.remove_selected_mcp_server_configuration() {
                    Ok(removed_name) => {
                        self.reload_selected_mcp_server_form();
                        self.mcp_notice = format!("已移除 MCP 服务器 `{removed_name}`。");
                        self.status_message = format!("MCP 服务器 `{removed_name}` 配置已删除。");
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ThemeSelected(theme) => {
                self.app_theme = theme.clone();
                self.status_message = format!("应用主题已切换为 {}。", theme);
                Task::none()
            }
            Message::QuickGuideMarkdownAction(action) => {
                if !action.is_edit() {
                    self.quick_guide_markdown_buffer.perform(action);
                }
                Task::none()
            }
            Message::ExecutionModeSelected(mode) => {
                self.execution_mode = mode;
                self.status_message = format!("执行模式已切换为 {mode}。");
                Task::none()
            }
            Message::TemperatureChanged(value) => {
                self.temperature = value;
                Task::none()
            }
            Message::TemperatureEnabledToggled(enabled) => {
                self.temperature_enabled = enabled;
                self.status_message = if enabled {
                    format!("已开启显式温度参数，当前值为 {:.2}。", self.temperature)
                } else {
                    String::from("已关闭显式温度参数，后续执行将使用模型默认值。")
                };
                Task::none()
            }
            Message::GeneratePlanToggled(enabled) => {
                self.generate_plan = enabled;
                Task::none()
            }
            Message::RunCargoCheckToggled(enabled) => {
                self.run_cargo_check = enabled;
                Task::none()
            }
            Message::PersistMemoryToggled(enabled) => {
                self.persist_memory = enabled;
                Task::none()
            }
            Message::AutoApproveToggled(enabled) => {
                self.auto_approve = enabled;
                Task::none()
            }
            Message::SessionSelected(session_id) => {
                self.prune_unmaterialized_sessions(Some(session_id));
                self.selected_session = session_id;
                self.hovered_task_session = Some(session_id);
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.pending_thinking_inline_toggle = None;

                // Capture the selected session title to keep footer feedback specific.
                // 记录被选中的会话标题，保证底部反馈足够具体。
                let selected_title = self
                    .selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"));

                self.status_message = format!("当前会话已切换为：{selected_title}。");
                Task::batch([
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentStructured => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::Structured;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.pending_thinking_inline_toggle = None;
                self.selected_session()
                    .map(|session| {
                        Task::batch([
                            self.maybe_follow_selected_session_transcript_tail(session.id),
                            self.maybe_follow_selected_session_thinking_overlay_tail(session.id),
                        ])
                    })
                    .unwrap_or_else(Task::none)
            }
            Message::ShowSelectedSessionDocumentRawSource => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::RawSource;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.pending_thinking_inline_toggle = None;
                self.selected_session()
                    .map(|session| {
                        Task::batch([
                            self.maybe_follow_selected_session_transcript_tail(session.id),
                            self.maybe_follow_selected_session_thinking_overlay_tail(session.id),
                        ])
                    })
                    .unwrap_or_else(Task::none)
            }
            Message::ShowSelectedSessionDocumentDetailCards => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::DetailCards;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.pending_thinking_inline_toggle = None;
                self.selected_session()
                    .map(|session| {
                        Task::batch([
                            self.maybe_follow_selected_session_transcript_tail(session.id),
                            self.maybe_follow_selected_session_thinking_overlay_tail(session.id),
                        ])
                    })
                    .unwrap_or_else(Task::none)
            }
            Message::ToggleSelectedSessionThinkingInline(block_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.pending_thinking_inline_toggle = Some(PendingThinkingInlineToggleState {
                    session_id,
                    block_id,
                    started_at: Instant::now(),
                });
                Task::none()
            }
            Message::OpenSelectedSessionThinkingModal(block_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.cancel_pending_thinking_inline_toggle(Some(session_id), Some(&block_id));
                self.open_thinking_modal(session_id, block_id);
                Task::none()
            }
            Message::FlushPendingThinkingInlineToggle => {
                self.flush_pending_thinking_inline_toggle();
                Task::none()
            }
            Message::CloseSelectedSessionThinkingModal => {
                self.begin_close_thinking_modal();
                Task::none()
            }
            Message::SelectedSessionDocumentStructuredAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                if let Some(content) = self.session_agent_view_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::SelectedSessionDocumentRawSourceAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                if let Some(content) = self.session_agent_view_raw_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::SelectedSessionTranscriptScrolled {
                session_id,
                viewport,
            } => {
                self.session_agent_view_viewports
                    .insert(session_id, viewport);
                self.update_session_agent_view_follow_tail(session_id, viewport);
                Task::none()
            }
            Message::SelectedSessionTranscriptDragAutoScrollChanged {
                session_id,
                request,
            } => {
                self.update_transcript_drag_auto_scroll_state(session_id, request);
                Task::none()
            }
            Message::SelectedSessionTranscriptDragAutoScrollTick => {
                self.perform_transcript_drag_auto_scroll_tick()
            }
            Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            } => {
                self.session_streaming_thinking_viewports
                    .insert(session_id, viewport);
                self.update_session_streaming_thinking_follow_tail(session_id, viewport);
                Task::none()
            }
            Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            } => {
                self.update_thinking_overlay_drag_auto_scroll_state(session_id, request);
                Task::none()
            }
            Message::SelectedSessionThinkingOverlayDragAutoScrollTick => {
                self.perform_thinking_overlay_drag_auto_scroll_tick()
            }
            Message::ThinkingModalAnimationTick => {
                self.advance_thinking_modal_animation();
                Task::none()
            }
            Message::ActivityLogRawAction(index, action) => {
                if !action.is_edit() {
                    if let Some(content) = self.activity_raw_buffers.get_mut(index) {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::CreateSession => {
                let project_id = self
                    .selected_task_project_id()
                    .or_else(|| self.task_projects.first().map(|project| project.id.clone()));

                if let Some(project_id) = project_id {
                    self.create_task_project_session(&project_id);
                    return self.persist_workspace_snapshot_task();
                } else {
                    self.status_message = String::from("当前没有可承载会话的项目节点。");
                }
                Task::none()
            }
            Message::TaskProjectHovered(project_id) => {
                if project_id.is_none() && self.dragging_task_project.is_none() {
                    self.active_task_project_drop_target = None;
                }
                self.hovered_task_project = if self.dragging_task_project.is_some() {
                    None
                } else {
                    project_id
                };
                Task::none()
            }
            Message::TaskProjectDropPreviewMoved(project_id, position) => {
                if let Some(dragged_project_id) = self.dragging_task_project.as_ref() {
                    if dragged_project_id == &project_id {
                        self.active_task_project_drop_target = None;
                        return Task::none();
                    }

                    let dragging_section_is_pinned = self
                        .task_project_by_id(dragged_project_id)
                        .map(|project| project.pinned);
                    let hovered_section_is_pinned = self
                        .task_project_by_id(&project_id)
                        .map(|project| project.pinned);

                    if dragging_section_is_pinned != hovered_section_is_pinned {
                        self.active_task_project_drop_target = None;
                        return Task::none();
                    }

                    let drop_target = if position.y <= MAIN_TASK_PROJECT_ROW_HEIGHT / 2.0 {
                        Some(TaskProjectDropTarget::Before(project_id.clone()))
                    } else if let Some(next_project_id) =
                        self.next_task_project_in_section(&project_id)
                    {
                        if next_project_id == *dragged_project_id {
                            None
                        } else {
                            Some(TaskProjectDropTarget::Before(next_project_id))
                        }
                    } else {
                        self.task_project_by_id(&project_id).map(|project| {
                            if project.pinned {
                                TaskProjectDropTarget::EndOfPinned
                            } else {
                                TaskProjectDropTarget::EndOfRegular
                            }
                        })
                    };

                    self.active_task_project_drop_target = drop_target;
                }
                Task::none()
            }
            Message::TaskProjectSectionTailHovered(section_is_pinned) => {
                if let (Some(section_is_pinned), Some(dragged_project_id)) =
                    (section_is_pinned, self.dragging_task_project.clone())
                {
                    let dragging_section_is_pinned = self
                        .task_project_by_id(&dragged_project_id)
                        .map(|project| project.pinned);

                    if dragging_section_is_pinned == Some(section_is_pinned) {
                        self.active_task_project_drop_target = Some(if section_is_pinned {
                            TaskProjectDropTarget::EndOfPinned
                        } else {
                            TaskProjectDropTarget::EndOfRegular
                        });
                    }
                } else if self.dragging_task_project.is_none() {
                    self.active_task_project_drop_target = None;
                }
                Task::none()
            }
            Message::TaskSessionHovered(session_id) => {
                if self.dragging_task_project.is_some() {
                    self.hovered_task_session = None;
                    return Task::none();
                }

                if session_id.is_none() {
                    self.suppress_task_session_hover = false;
                    self.hovered_task_session = None;
                    return Task::none();
                }

                if self.suppress_task_session_hover {
                    return Task::none();
                }
                self.hovered_task_session = session_id;
                Task::none()
            }
            Message::StartTaskProjectInteraction(project_id) => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_task_project_interaction = Some(PendingTaskProjectInteraction {
                        project_id,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::OpenTaskProjectDirectory(project_id) => {
                if let Some(project) = self.task_project_by_id(&project_id) {
                    match open_path_in_system_file_manager(&project.path) {
                        Ok(()) => {
                            self.status_message =
                                format!("已请求打开项目目录：{}。", project.title);
                        }
                        Err(error) => {
                            self.status_message =
                                format!("打开项目目录 `{}` 失败：{error}", project.title);
                        }
                    }
                }
                Task::none()
            }
            Message::CreateTaskProjectSession(project_id) => {
                self.create_task_project_session(&project_id);
                self.persist_workspace_snapshot_task()
            }
            Message::AddTaskProjectFromFolder => {
                match pick_task_project_folder(self.workspace_path.trim()) {
                    Ok(Some(selected_path)) => {
                        self.add_task_project_from_folder(&selected_path);
                        return self.persist_workspace_snapshot_task();
                    }
                    Ok(None) => {
                        self.status_message = String::from("已取消从文件夹添加项目。");
                    }
                    Err(error) => {
                        self.status_message = format!("从文件夹添加项目失败：{error}");
                    }
                }
                Task::none()
            }
            Message::PinTaskProject(project_id) => {
                if let Some(project) = self.task_project_by_id_mut(&project_id) {
                    project.pinned = !project.pinned;
                    let title = project.title.clone();
                    let pinned = project.pinned;
                    self.sort_task_projects();
                    self.status_message = if pinned {
                        format!("已将项目 `{title}` 置顶。")
                    } else {
                        format!("已取消项目 `{title}` 的置顶状态。")
                    };
                }
                self.persist_workspace_snapshot_task()
            }
            Message::RenameTaskProject(project_id) => {
                if let Some(project) = self.task_project_by_id_mut(&project_id) {
                    let original_title = project.title.clone();
                    if project.title.ends_with("（重命名）") {
                        project.title = project.title.trim_end_matches("（重命名）").to_string();
                    } else {
                        project.title = format!("{}（重命名）", project.title);
                    }
                    let renamed_title = project.title.clone();
                    self.status_message =
                        format!("已示例性重命名项目：`{original_title}` -> `{renamed_title}`。");
                }
                self.persist_workspace_snapshot_task()
            }
            Message::ArchiveTaskProject(project_id) => {
                if let Some(index) = self
                    .task_projects
                    .iter()
                    .position(|project| project.id == project_id)
                {
                    let project = self.task_projects.remove(index);
                    self.drop_unmaterialized_sessions_for_project(&project.id);
                    let archived_session_ids = project.session_ids.clone();
                    for session_id in &archived_session_ids {
                        if let Some(session_index) = self
                            .sessions
                            .iter()
                            .position(|session| session.id == *session_id)
                        {
                            let session = self.sessions.remove(session_index);
                            self.archived_sessions.push(session);
                        }
                        if let Some(messages) = self.session_messages.remove(session_id) {
                            self.archived_session_messages.insert(*session_id, messages);
                        }
                        if let Some(compactions) = self.session_compactions.remove(session_id) {
                            self.archived_session_compactions
                                .insert(*session_id, compactions);
                        }
                        if let Some(runtime_state) = self.session_runtime_state.remove(session_id) {
                            self.archived_session_runtime_state
                                .insert(*session_id, runtime_state);
                        }
                        self.session_runtime_events.remove(session_id);
                        self.session_run_phases.remove(session_id);
                    }
                    self.archived_task_projects.push(project.clone());
                    self.expanded_task_projects.remove(&project.id);
                    self.hovered_task_project = None;
                    self.hovered_task_session = None;
                    self.active_task_project_drop_target = None;
                    self.pending_task_project_interaction = None;
                    self.dragging_task_project = None;
                    self.ensure_valid_selected_session();
                    self.status_message = format!("已归档项目 `{}`。", project.title);
                }
                self.persist_workspace_snapshot_task()
            }
            Message::RestoreTaskProject(project_id) => {
                let _ = self.restore_archived_task_project_by_id(&project_id);
                self.persist_workspace_snapshot_task()
            }
            Message::RenameTaskSession(session_id) => {
                if let Some(session) = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    let original_title = session.title.clone();
                    if session.title.ends_with("（重命名）") {
                        session.title = session.title.trim_end_matches("（重命名）").to_string();
                    } else {
                        session.title = format!("{}（重命名）", session.title);
                    }
                    let renamed_title = session.title.clone();
                    self.status_message =
                        format!("已示例性重命名会话：`{original_title}` -> `{renamed_title}`。");
                }
                self.persist_workspace_snapshot_task()
            }
            Message::ArchiveTaskSession(session_id) => {
                let archived_owner_snapshot = self.task_projects.iter().find_map(|project| {
                    project
                        .session_ids
                        .iter()
                        .position(|id| *id == session_id)
                        .map(|session_index| (project.id.clone(), session_index))
                });
                let archived_title = self
                    .session_by_id(session_id)
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| format!("会话 #{session_id}"));
                if let Some(index) = self
                    .sessions
                    .iter()
                    .position(|session| session.id == session_id)
                {
                    let session = self.sessions.remove(index);
                    self.archived_sessions.push(session);
                }
                if let Some(messages) = self.session_messages.remove(&session_id) {
                    self.archived_session_messages.insert(session_id, messages);
                }
                if let Some(compactions) = self.session_compactions.remove(&session_id) {
                    self.archived_session_compactions
                        .insert(session_id, compactions);
                }
                if let Some(runtime_state) = self.session_runtime_state.remove(&session_id) {
                    self.archived_session_runtime_state
                        .insert(session_id, runtime_state);
                }
                self.session_runtime_events.remove(&session_id);
                self.session_run_phases.remove(&session_id);
                for project in &mut self.task_projects {
                    project.session_ids.retain(|id| *id != session_id);
                }
                self.hovered_task_session =
                    archived_owner_snapshot
                        .as_ref()
                        .and_then(|(project_id, removed_index)| {
                            self.task_project_by_id(project_id).and_then(|project| {
                                project
                                    .session_ids
                                    .get(*removed_index)
                                    .copied()
                                    .or_else(|| project.session_ids.last().copied())
                            })
                        });
                self.suppress_task_session_hover = false;
                self.ensure_valid_selected_session();
                self.status_message = format!("已归档会话 `{archived_title}`。");
                self.persist_workspace_snapshot_task()
            }
            Message::RestoreTaskSession(session_id) => {
                let _ = self.restore_archived_task_session(session_id);
                self.persist_workspace_snapshot_task()
            }
            Message::ScanWorkspace => {
                // Load the current workspace diff synchronously for the first testing milestone.
                // 为首个测试里程碑同步载入当前工作区的 diff 结果。
                let diff_preview = load_workspace_diff_preview(&self.workspace_path);

                self.diff_summary = diff_preview.summary.clone();
                self.diff_panel = diff_preview.panel.clone();
                self.diff_scroll_offset = 0.0;
                self.hovered_diff_line = None;
                let available_diff_file_keys = self
                    .diff_panel
                    .files
                    .iter()
                    .map(DiffFile::key)
                    .collect::<BTreeSet<_>>();
                self.collapsed_diff_files
                    .retain(|key| available_diff_file_keys.contains(key));
                self.rebuild_diff_code_buffers_for_current_wrap();
                self.status_message = diff_preview.status_message.clone();

                // Reflect the detected change count into the selected session card so the left queue stays informative.
                // 将检测到的变更数量回写到当前会话卡片，保证左侧队列信息同步更新。
                if let Some(session) = self.selected_session_mut() {
                    session.changed_files = diff_preview.changed_files.min(u8::MAX as usize) as u8;
                }

                self.push_activity(
                    diff_preview.activity_level,
                    "Git Diff 扫描完成",
                    format!(
                        "目标路径：{}；摘要：{}。",
                        self.workspace_path, diff_preview.summary
                    ),
                );
                Task::none()
            }
            Message::ToggleDiffFile(file_key) => {
                self.toggle_diff_file_collapsed(&file_key);
                Task::none()
            }
            Message::DiffPanelScrolled(viewport) => {
                self.diff_scroll_offset = viewport.absolute_offset().y;
                self.diff_viewport_height = viewport.bounds().height.max(0.0);
                self.refresh_visible_diff_buffers();
                Task::none()
            }
            Message::ToggleAndFocusDiffFile(file_key) => {
                let target_offset = self
                    .diff_file_scroll_offset(&file_key)
                    .unwrap_or(self.diff_scroll_offset);
                self.toggle_diff_file_collapsed(&file_key);
                self.diff_scroll_offset = target_offset;
                self.refresh_visible_diff_buffers();

                operation::scroll_to(
                    DIFF_PANEL_SCROLLABLE_ID,
                    scrollable::AbsoluteOffset {
                        x: 0.0,
                        y: target_offset,
                    },
                )
            }
            Message::DiffCodeAction(buffer_key, action) => {
                if matches!(action, text_editor::Action::Scroll { .. }) {
                    return Task::none();
                }

                if !action.is_edit() {
                    if let Some(content) = self.diff_code_buffers.get_mut(&buffer_key) {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::DiffCodeHoverMoved(file_key, line_index) => {
                self.hovered_diff_line = Some(HoveredDiffLine {
                    file_key,
                    line_index,
                });
                Task::none()
            }
            Message::DiffCodeHoverExited(file_key) => {
                if self
                    .hovered_diff_line
                    .as_ref()
                    .is_some_and(|hovered| hovered.file_key == file_key)
                {
                    self.hovered_diff_line = None;
                }
                Task::none()
            }
            Message::StartRun => {
                if self.has_active_run() {
                    self.status_message = String::from(
                        "当前已有 AI 调用正在执行，可点击停止按钮中止后再发起新的请求。",
                    );
                    return Task::none();
                }

                let request = if let Some(session) = self.selected_session() {
                    let context_history = self
                        .session_context_history(session.id)
                        .into_iter()
                        .map(session_message_into_execution_context)
                        .collect::<Vec<_>>();
                    ExecutionPreparationRequest {
                        session_id: session.id,
                        session_title: session.title.clone(),
                        workspace_path: self.workspace_path.clone(),
                        prompt: self.composer.clone(),
                        context_history,
                        execution_mode: self.execution_mode.to_string(),
                        temperature: self.temperature_enabled.then_some(self.temperature),
                    }
                } else {
                    self.status_message = String::from("当前没有可执行的会话。");
                    return Task::none();
                };

                if request.prompt.trim().is_empty() {
                    self.status_message = String::from("当前提示词为空，无法启动执行。");
                    self.push_session_message(
                        request.session_id,
                        SessionMessageKind::Error,
                        "执行启动失败",
                        "请输入任务指令后再启动执行。",
                    );
                    self.replace_session_runtime_events(
                        request.session_id,
                        vec![RuntimeEvent::new(
                            RuntimeEventKind::Failed,
                            "执行启动失败",
                            "当前提示词为空，系统已拒绝本次运行请求。",
                        )],
                    );
                    self.set_session_run_phase(request.session_id, SessionRunPhase::Failed);
                    return Task::none();
                }

                self.push_session_message(
                    request.session_id,
                    SessionMessageKind::User,
                    "用户指令",
                    request.prompt.clone(),
                );
                let _ = self
                    .maybe_generate_session_title_from_prompt(request.session_id, &request.prompt);
                self.materialize_session_after_first_user_turn(request.session_id);
                self.replace_session_runtime_events(request.session_id, Vec::new());
                self.clear_streaming_message_ids(request.session_id);
                self.last_mcp_discovery_rows.clear();
                self.last_mcp_discovery_errors.clear();
                self.set_session_run_phase(request.session_id, SessionRunPhase::Preparing);
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Running;
                    session.last_updated_label = String::from("刚刚");
                }
                self.status_message = String::from("正在发起真实执行，请稍候。");

                let run_id = self.next_run_id;
                self.next_run_id = self.next_run_id.saturating_add(1);
                let request_for_run = request.clone();
                let config_for_run = self.provider_configuration.clone();
                let (run_task, run_handle) = Task::run(
                    stream_runtime_run(
                        config_for_run,
                        self.agent_profile_catalog.clone(),
                        self.luaskills_runtime.clone(),
                        self.mcp_runtime.clone(),
                        request_for_run,
                    ),
                    move |event| Message::RunExecutionStreamEvent { run_id, event },
                )
                .abortable();
                self.active_run_id = Some(run_id);
                self.active_run_session_id = Some(request.session_id);
                self.active_run_handle = Some(run_handle);

                Task::batch([
                    self.persist_workspace_snapshot_task(),
                    self.maybe_follow_selected_session_transcript_tail(request.session_id),
                    run_task,
                ])
            }
            Message::RunExecutionStreamEvent { run_id, event } => {
                if self.active_run_id != Some(run_id) {
                    return Task::none();
                }

                match event {
                    ExecutionStreamEvent::Prepared(preparation) => {
                        let detail = preparation.summary_rows().join(" | ");
                        self.last_execution_preparation = Some(preparation.clone());
                        self.context_usage = (self.context_usage + 8.0).min(100.0);
                        self.update_session_runtime_identity_from_preparation(
                            preparation.session_id,
                            &preparation,
                        );
                        self.status_message =
                            String::from("真实执行已开始，正在分离思考与输出并持续写入工作台。");
                        self.push_activity(ActivityLevel::Success, "执行准备已完成", detail);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.append_streaming_thinking_delta(session_id, &delta);
                        Task::batch([
                            self.maybe_follow_selected_session_transcript_tail(session_id),
                            self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                        ])
                    }
                    ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                        self.dock_streaming_thinking_message(session_id);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                        self.append_streaming_output_delta(session_id, &delta);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.push_session_runtime_event(session_id, event);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::Finished(report) => {
                        self.clear_active_run_tracking();
                        self.apply_execution_run_report(report)
                    }
                }
            }
            Message::StopRun => {
                let Some(session_id) = self.active_run_session_id else {
                    self.status_message = String::from("当前没有正在执行的 AI 调用。");
                    return Task::none();
                };

                let run_handle = self.active_run_handle.take();
                self.clear_active_run_tracking();

                if let Some(handle) = run_handle {
                    handle.abort();
                }

                // Mark the stopped session as blocked so the interruption is visible in the UI.
                // 将被停止的会话标记为阻塞状态，以便在界面中显式展示中断结果。
                if let Some(session) = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    session.status = SessionState::Blocked;
                    session.last_updated_label = String::from("刚刚");
                }

                self.clear_streaming_message_ids(session_id);
                self.set_session_run_phase(session_id, SessionRunPhase::Stopped);
                self.status_message = String::from("已停止当前 AI 调用。");
                self.push_session_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    "执行已停止",
                    "已按用户请求停止当前 AI 调用，系统保留截至停止时的上下文、思考与输出片段。",
                );
                self.push_activity(
                    ActivityLevel::Warning,
                    "AI 调用已停止",
                    "当前真实执行已被用户中止，可直接修改提示词后重新发起新的调用。",
                );
                Task::batch([
                    self.persist_workspace_snapshot_task(),
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                ])
            }
            Message::PaneFocused(pane) => {
                self.focus = Some(pane);
                Task::none()
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
                self.rebuild_diff_code_buffers_for_current_wrap();
                Task::none()
            }
        }
    }

    /// Builds the full window view by composing header, panes, and footer.
    /// 通过组合头部、分栏和底部状态栏构建完整窗口视图。
    fn view(&self, window_id: window::Id) -> Element<'_, Message> {
        if self.diff_window_id == Some(window_id) {
            self.view_diff_window(window_id)
        } else if self.main_window_id == Some(window_id) {
            self.view_main_window(window_id)
        } else {
            space().into()
        }
    }

    /// Builds the main workbench window view by composing header, panes, and footer.
    /// 通过组合头部、分栏和底部状态栏构建主工作台窗口视图。
    fn view_main_window(&self, window_id: window::Id) -> Element<'_, Message> {
        // Build the fixed top chrome using only the custom title bar so the main surface can move to a left activity bar pattern.
        // 仅使用自定义标题栏构建固定顶部 chrome，使主表面可以切换到左侧活动条模式。
        let top_chrome = container(self.view_window_title_bar(window_id, "Vulcan Code"))
            .style(style::window_chrome_style);
        let main_surface: Element<'_, Message> = column![
            top_chrome,
            container(
                column![
                    row![
                        container(self.view_main_activity_bar())
                            .width(MAIN_PANEL_ACTIVITY_BAR_WIDTH)
                            .height(Fill)
                            .style(style::activity_bar_style),
                        self.view_main_display_region(),
                    ]
                    .height(Fill),
                    self.view_status_bar(),
                ]
                .spacing(0)
            )
            .height(Fill),
        ]
        .spacing(0)
        .into();

        let composed_surface: Element<'_, Message> =
            if let Some(overlay) = self.view_startup_overlay() {
                stack([main_surface, overlay]).into()
            } else {
                main_surface
            };

        self.wrap_with_window_resize_handles(window_id, composed_surface)
    }

    /// Builds the framed main display region with dedicated top, left, and bottom separators.
    /// 构建带独立上侧、左侧、下侧分隔线的主显示区域。
    fn view_main_display_region(&self) -> Element<'_, Message> {
        container(
            column![
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(style::main_display_top_separator_style),
                row![
                    container(space())
                        .width(1)
                        .height(Fill)
                        .style(style::main_display_left_separator_style),
                    container(self.view_active_main_panel())
                        .width(Fill)
                        .height(Fill),
                ]
                .spacing(0)
                .height(Fill),
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(style::main_display_bottom_separator_style),
            ]
            .spacing(0)
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Builds the blocking startup overlay shown while the controller-backed data plane is initializing.
    /// 构建在控制器数据平面初始化期间展示的阻塞式启动遮罩。
    fn view_startup_overlay(&self) -> Option<Element<'_, Message>> {
        match &self.startup_lifecycle {
            StartupLifecycle::Ready => None,
            StartupLifecycle::Bootstrapping { message } => Some(
                container(
                    container(
                        column![
                            text("正在载入 Vulcan Code").size(26),
                            text("正在准备 vldb 数据平面、LuaSkills 与工作台状态。").size(14),
                            text(message).size(13),
                        ]
                        .spacing(10),
                    )
                    .padding([22, 26])
                    .width(420)
                    .style(style::startup_overlay_card_style),
                )
                .width(Fill)
                .height(Fill)
                .center(Fill)
                .style(style::startup_overlay_backdrop_style)
                .into(),
            ),
            StartupLifecycle::Failed { message } => Some(
                container(
                    container(
                        column![
                            text("vldb 数据平面初始化失败").size(24),
                            text(message).size(13),
                            button("重新初始化")
                                .style(button::primary)
                                .on_press(Message::BeginStartupBootstrap),
                        ]
                        .spacing(14),
                    )
                    .padding([22, 26])
                    .width(460)
                    .style(style::startup_overlay_card_style),
                )
                .width(Fill)
                .height(Fill)
                .center(Fill)
                .style(style::startup_overlay_backdrop_style)
                .into(),
            ),
        }
    }

    /// Builds the currently selected top-level panel shown inside the main display region.
    /// 构建当前显示在主显示区域中的顶层面板内容。
    fn view_active_main_panel(&self) -> Element<'_, Message> {
        match self.active_main_panel {
            MainDisplayPanel::Main => self.view_empty_main_panel(),
            MainDisplayPanel::PromptSkills => {
                self.view_placeholder_main_panel(MainDisplayPanel::PromptSkills)
            }
            MainDisplayPanel::LuaSkills => self.view_luaskills_panel(),
            MainDisplayPanel::Automation => {
                self.view_placeholder_main_panel(MainDisplayPanel::Automation)
            }
            MainDisplayPanel::Agents => self.view_placeholder_main_panel(MainDisplayPanel::Agents),
            MainDisplayPanel::Plugins => {
                self.view_placeholder_main_panel(MainDisplayPanel::Plugins)
            }
            MainDisplayPanel::VulcanMemoryMesh => {
                self.view_placeholder_main_panel(MainDisplayPanel::VulcanMemoryMesh)
            }
            MainDisplayPanel::Example => self.view_example_panel(),
            MainDisplayPanel::Settings => self.view_settings_panel(),
        }
    }

    /// Builds the main workbench surface with a collapsible and resizable left task drawer.
    /// 构建包含可折叠且可缩放左侧任务抽屉的主工作台表面。
    fn view_empty_main_panel(&self) -> Element<'_, Message> {
        let task_panel: Element<'_, Message> = if self.main_task_panel_expanded {
            row![
                container(self.view_main_task_panel())
                    .width(Pixels(self.main_task_panel_width))
                    .height(Fill),
                self.view_main_task_panel_resize_handle(),
            ]
            .height(Fill)
            .into()
        } else {
            container(self.view_main_task_panel_restore_rail())
                .width(MAIN_TASK_PANEL_RESTORE_RAIL_WIDTH)
                .height(Fill)
                .into()
        };

        row![
            task_panel,
            container(self.view_main_canvas()).width(Fill).height(Fill)
        ]
        .height(Fill)
        .into()
    }

    /// Builds the fixed-width vertical activity bar that switches the top-level display panels.
    /// 构建固定宽度的竖向活动条，用于切换顶层显示面板。
    fn view_main_activity_bar(&self) -> Element<'_, Message> {
        column![
            self.view_main_activity_button(MainDisplayPanel::Main),
            self.view_main_activity_button(MainDisplayPanel::PromptSkills),
            self.view_main_activity_button(MainDisplayPanel::LuaSkills),
            self.view_main_activity_button(MainDisplayPanel::Automation),
            self.view_main_activity_button(MainDisplayPanel::Agents),
            self.view_main_activity_button(MainDisplayPanel::Plugins),
            self.view_main_activity_button(MainDisplayPanel::VulcanMemoryMesh),
            self.view_main_activity_button(MainDisplayPanel::Example),
            self.view_main_activity_button(MainDisplayPanel::Settings),
            space::vertical().height(Fill),
        ]
        .spacing(6)
        .padding([10, 5])
        .width(Fill)
        .height(Fill)
        .align_x(Center)
        .into()
    }

    /// Builds one icon-only activity button shown inside the vertical top-level panel bar.
    /// 构建展示在竖向顶层面板活动条中的单个纯图标按钮。
    fn view_main_activity_button(&self, panel: MainDisplayPanel) -> Element<'_, Message> {
        let tooltip_label = format!("{}（Ctrl+{}）", panel.label(), panel.shortcut_digit());
        let is_active = self.active_main_panel == panel;

        tooltip(
            button(
                container(self.view_main_activity_icon(panel))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(MAIN_PANEL_ACTIVITY_BUTTON_SIZE)
            .height(MAIN_PANEL_ACTIVITY_BUTTON_SIZE)
            .padding(0)
            .style(move |theme, status| style::activity_bar_button_style(theme, status, is_active))
            .on_press(Message::MainDisplayPanelSelected(panel)),
            text(tooltip_label),
            tooltip::Position::Right,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Builds one icon glyph used by the vertical top-level panel activity bar.
    /// 构建竖向顶层面板活动条使用的单个图标字形。
    fn view_main_activity_icon(&self, panel: MainDisplayPanel) -> Element<'_, Message> {
        let is_active = self.active_main_panel == panel;
        let icon_widget = svg(svg::Handle::from_memory(main_display_panel_icon_svg_bytes(
            panel,
        )))
        .width(Pixels(MAIN_PANEL_ACTIVITY_ICON_SIZE))
        .height(Pixels(MAIN_PANEL_ACTIVITY_ICON_SIZE))
        .style(move |theme, _status| svg::Style {
            color: Some(style::activity_bar_icon_color(theme, is_active)),
        });

        container(icon_widget)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
    }

    /// Builds the left task drawer shown inside the main workbench panel.
    /// 构建显示在主工作台面板中的左侧任务抽屉。
    fn view_main_task_panel(&self) -> Element<'_, Message> {
        let session_tree = scrollable(self.view_main_task_session_tree())
            .height(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(6)
                    .margin(0)
                    .scroller_width(6),
            ));

        let panel_body = column![session_tree].spacing(0).height(Fill);

        let body: Element<'_, Message> = if let Some(preview) = self.view_main_task_drag_preview() {
            stack([panel_body.into(), preview]).into()
        } else {
            panel_body.into()
        };

        container(body)
            .padding([12, 2])
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Builds the floating ghost preview that follows the cursor while one project card is being dragged.
    /// 构建在拖拽项目卡片期间跟随鼠标移动的浮动虚影预览。
    fn view_main_task_drag_preview(&self) -> Option<Element<'_, Message>> {
        let dragged_project = self
            .dragging_task_project
            .as_deref()
            .and_then(|project_id| self.task_project_by_id(project_id))?;
        let cursor = self.main_window_cursor_position?;

        let preview_width = (self.main_task_panel_width - 28.0).max(180.0);
        let local_left = (cursor.x - MAIN_PANEL_ACTIVITY_BAR_WIDTH - 12.0)
            .clamp(2.0, (self.main_task_panel_width - preview_width).max(2.0));
        let local_top =
            (cursor.y - WINDOW_TITLE_BAR_HEIGHT - (MAIN_TASK_PROJECT_ROW_HEIGHT / 2.0) - 2.0)
                .clamp(30.0, 1200.0);

        let preview_title = truncate_with_ellipsis(
            &dragged_project.title,
            self.main_task_project_title_limit_width(),
        );
        let preview_card = container(
            row![
                container(space::horizontal())
                    .width(3.0)
                    .height(Fill)
                    .style(style::main_task_tree_drop_indicator_style),
                row![
                    codicon::folder_opened().size(16).line_height(1.0),
                    text(preview_title)
                        .size(15)
                        .wrapping(iced::advanced::text::Wrapping::None),
                ]
                .spacing(8)
                .align_y(Center),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .padding([3, 11])
        .width(Pixels(preview_width))
        .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
        .style(style::main_task_tree_drag_preview_style);

        Some(
            container(preview_card)
                .padding(iced::Padding {
                    top: local_top,
                    right: 0.0,
                    bottom: 0.0,
                    left: local_left,
                })
                .width(Fill)
                .height(Fill)
                .into(),
        )
    }

    /// Builds the draggable divider handle between the main task drawer and the central workbench canvas.
    /// 构建位于主任务抽屉与中央工作区画布之间的可拖拽分隔柄。
    fn view_main_task_panel_resize_handle(&self) -> Element<'_, Message> {
        let is_highlighted = self.main_task_panel_resize_handle_hovered
            || self.pending_main_task_panel_resize.is_some();

        let drag_surface = mouse_area(
            container(
                container(space())
                    .width(if is_highlighted { 2 } else { 1 })
                    .height(Fill)
                    .style(move |theme| {
                        style::main_task_panel_resize_indicator_style(theme, is_highlighted)
                    }),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_enter(Message::MainTaskPanelResizeHandleHovered(true))
        .on_exit(Message::MainTaskPanelResizeHandleHovered(false))
        .on_press(Message::StartMainTaskPanelResize);

        let collapse_button = tooltip(
            button(
                container(codicon::chevron_left().size(10).line_height(1.0))
                    .width(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                    .height(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT)
                    .center_x(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                    .center_y(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT),
            )
            .padding(0)
            .style(style::subtle_panel_button_style)
            .on_press(Message::ToggleMainTaskPanel),
            "收起任务面板",
            tooltip::Position::Right,
        )
        .style(container::rounded_box);

        container(stack([
            drag_surface.into(),
            column![
                space::vertical().height(Fill),
                container(collapse_button)
                    .width(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                    .height(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT),
                space::vertical().height(Fill),
            ]
            .width(Fill)
            .height(Fill)
            .align_x(Center)
            .into(),
        ]))
        .width(MAIN_TASK_PANEL_RESIZE_HANDLE_WIDTH)
        .height(Fill)
        .into()
    }

    /// Builds the subtle restore rail shown when the main task drawer is collapsed.
    /// 构建主任务抽屉折叠后显示的轻量恢复导轨。
    fn view_main_task_panel_restore_rail(&self) -> Element<'_, Message> {
        container(
            column![
                space::vertical().height(Fill),
                container(
                    tooltip(
                        button(
                            container(codicon::chevron_right().size(10).line_height(1.0))
                                .width(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                                .height(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT)
                                .center_x(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                                .center_y(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT),
                        )
                        .padding(0)
                        .style(style::subtle_panel_button_style)
                        .on_press(Message::ToggleMainTaskPanel),
                        "展开任务面板",
                        tooltip::Position::Right,
                    )
                    .style(container::rounded_box),
                )
                .width(MAIN_TASK_PANEL_EDGE_TOGGLE_WIDTH)
                .height(MAIN_TASK_PANEL_EDGE_TOGGLE_HEIGHT),
                space::vertical().height(Fill),
            ]
            .width(Fill)
            .height(Fill)
            .align_x(Center),
        )
        .height(Fill)
        .style(style::main_task_panel_restore_rail_style)
        .into()
    }

    /// Builds the tree-shaped workspace and session outline shown inside the main task drawer.
    /// 构建展示在主任务抽屉中的树形工作空间与会话大纲。
    fn view_main_task_session_tree(&self) -> Element<'_, Message> {
        let mut tree = column![].spacing(1).width(Fill);
        let pinned_projects = self.task_projects.iter().filter(|project| project.pinned);
        let regular_projects = self.task_projects.iter().filter(|project| !project.pinned);
        let archived_projects = self.archived_task_projects.iter();

        let has_pinned = self.task_projects.iter().any(|project| project.pinned);

        if has_pinned {
            tree = tree
                .push(self.view_main_task_group_label("置顶", false))
                .push(
                    pinned_projects.fold(column![].spacing(1).width(Fill), |column, project| {
                        column.push(self.view_main_task_project_tree_row(project))
                    }),
                )
                .push(self.view_main_task_project_section_tail(true))
                .push(space::vertical().height(6));
        }

        tree = tree
            .push(self.view_main_task_group_label("项目", true))
            .push(
                regular_projects.fold(column![].spacing(1).width(Fill), |column, project| {
                    column.push(self.view_main_task_project_tree_row(project))
                }),
            );

        tree = tree.push(self.view_main_task_project_section_tail(false));

        if !self.archived_task_projects.is_empty() {
            tree = tree
                .push(space::vertical().height(6))
                .push(self.view_main_task_group_label("归档项目", false))
                .push(
                    archived_projects.fold(column![].spacing(1).width(Fill), |column, project| {
                        column.push(self.view_archived_task_project_row(project))
                    }),
                );
        }

        container(tree).width(Fill).into()
    }

    /// Builds one compact section label used inside the project tree.
    /// 构建展示在项目树内部的紧凑分区标签。
    fn view_main_task_group_label(
        &self,
        label: &'static str,
        show_add_project_button: bool,
    ) -> Element<'_, Message> {
        let trailing_action: Element<'_, Message> = if show_add_project_button {
            tooltip(
                Self::view_main_task_tree_icon_action_button(
                    TaskTreeActionIcon::AddProjectFromFolder,
                    Message::AddTaskProjectFromFolder,
                    true,
                ),
                text("从文件夹添加项目"),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into()
        } else {
            space::horizontal()
                .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .into()
        };

        container(
            row![
                text(label).size(15).style(|theme: &Theme| {
                    iced::widget::text::Style {
                        color: Some(theme.extended_palette().background.base.text),
                    }
                }),
                space::horizontal(),
                trailing_action,
            ]
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 3.0,
            right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            bottom: 3.0,
            left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
        })
        .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
        .width(Fill)
        .style(|theme| style::main_task_tree_project_row_style(theme, false, false))
        .into()
    }

    /// Builds the insertion indicator line shown before one project row while reordering.
    /// 构建在项目重排期间显示在某个项目行之前的插入指示线。
    fn view_main_task_project_drop_indicator(&self, is_visible: bool) -> Element<'_, Message> {
        let height = if is_visible { 8.0 } else { 0.0 };
        let padding = if is_visible { 2.0 } else { 0.0 };

        container(
            row![
                container(space::horizontal())
                    .width(7.0)
                    .height(7.0)
                    .style(style::main_task_tree_drop_anchor_style),
                container(space::horizontal())
                    .height(3.0)
                    .width(Fill)
                    .style(style::main_task_tree_drop_indicator_style),
            ]
            .spacing(4)
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: 6.0,
            bottom: padding,
            left: 6.0,
        })
        .height(height)
        .width(Fill)
        .into()
    }

    /// Builds the tail drop zone shown after one project section so dragged cards can land at the end.
    /// 构建显示在项目分区末尾的尾部落点，以便拖拽卡片落到分区最后。
    fn view_main_task_project_section_tail(&self, section_is_pinned: bool) -> Element<'_, Message> {
        let dragging_matches_section = self
            .dragging_task_project
            .as_ref()
            .and_then(|project_id| self.task_project_by_id(project_id))
            .map(|project| project.pinned == section_is_pinned)
            .unwrap_or(false);

        if !dragging_matches_section {
            return space::vertical().height(0.0).into();
        }

        let shows_drop_tail = matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::EndOfPinned) if section_is_pinned
        ) || matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::EndOfRegular) if !section_is_pinned
        );

        mouse_area(if shows_drop_tail {
            container(
                row![
                    container(space::horizontal())
                        .width(7.0)
                        .height(7.0)
                        .style(style::main_task_tree_drop_anchor_style),
                    container(space::horizontal())
                        .height(3.0)
                        .width(Fill)
                        .style(style::main_task_tree_drop_indicator_style),
                ]
                .spacing(4)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 2.0,
                right: 6.0,
                bottom: 0.0,
                left: 6.0,
            })
            .height(10.0)
            .width(Fill)
        } else {
            container(space::horizontal()).height(10.0).width(Fill)
        })
        .interaction(if self.dragging_task_project.is_some() {
            iced::mouse::Interaction::Move
        } else {
            iced::mouse::Interaction::default()
        })
        .on_enter(Message::TaskProjectSectionTailHovered(Some(
            section_is_pinned,
        )))
        .on_exit(Message::TaskProjectSectionTailHovered(None))
        .into()
    }

    /// Builds one project branch shown inside the main task drawer tree.
    /// 构建显示在主任务抽屉树中的单个项目分支。
    fn view_main_task_project_tree_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let is_expanded = self.expanded_task_projects.contains(&project.id);
        let shows_drop_before = matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::Before(target_project_id)) if target_project_id == &project.id
        );
        let mut section = column![]
            .spacing(0)
            .width(Fill)
            .push(self.view_main_task_project_drop_indicator(shows_drop_before))
            .push(self.view_main_task_project_row(project, is_expanded));

        if is_expanded {
            section = section.push(
                container(self.view_main_task_project_children(project))
                    .padding(iced::Padding {
                        top: 2.0,
                        right: 2.0,
                        bottom: 8.0,
                        left: 1.0,
                    })
                    .width(Fill),
            );
        }

        container(section).width(Fill).into()
    }

    /// Builds the clickable project row displayed at the top of one tree branch.
    /// 构建显示在树分支顶部的可点击项目行。
    fn view_main_task_project_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
        is_expanded: bool,
    ) -> Element<'a, Message> {
        let is_active = self.dragging_task_project.is_none()
            && self.hovered_task_project.as_deref() == Some(project.id.as_str());
        let is_drag_source = self.dragging_task_project.as_deref() == Some(project.id.as_str());
        let source_alpha = if is_drag_source { 0.46 } else { 1.0 };
        let project_title =
            truncate_with_ellipsis(&project.title, self.main_task_project_title_limit_width());
        let project_label = text(project_title)
            .size(15)
            .width(Fill)
            .wrapping(iced::advanced::text::Wrapping::None)
            .style(move |theme: &Theme| iced::widget::text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(source_alpha),
                ),
            });
        let folder_icon = if is_expanded {
            codicon::folder_opened()
        } else {
            codicon::folder()
        }
        .size(16)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.86 * source_alpha),
            ),
        });
        let state_arrow: Element<'a, Message> = if is_expanded {
            codicon::chevron_down()
        } else {
            codicon::chevron_right()
        }
        .size(14)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.72 * source_alpha),
            ),
        })
        .into();

        let header = container(
            row![
                folder_icon,
                project_label,
                container(Self::view_main_task_tree_icon_action_button(
                    TaskTreeActionIcon::AddSession,
                    Message::CreateTaskProjectSession(project.id.clone()),
                    true,
                ))
                .width(MAIN_TASK_PROJECT_ACTIONS_WIDTH)
                .center_y(Fill),
                container(state_arrow)
                    .width(MAIN_TASK_PROJECT_ARROW_WIDTH)
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 2.0,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .center_y(Fill),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 3.0,
            right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            bottom: 3.0,
            left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
        })
        .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
        .width(Fill)
        .style(move |theme| {
            style::main_task_tree_project_row_style(theme, is_active, is_drag_source)
        });

        ContextMenu::new(
            mouse_area(header)
                .interaction(if self.dragging_task_project.is_some() {
                    iced::mouse::Interaction::Move
                } else {
                    iced::mouse::Interaction::default()
                })
                .on_press(Message::StartTaskProjectInteraction(project.id.clone()))
                .on_enter(Message::TaskProjectHovered(Some(project.id.clone())))
                .on_move({
                    let project_id = project.id.clone();
                    move |position| {
                        Message::TaskProjectDropPreviewMoved(project_id.clone(), position)
                    }
                })
                .on_exit(Message::TaskProjectHovered(None)),
            move || self.view_main_task_project_menu(project),
        )
        .into()
    }

    /// Builds the expanded child rows shown beneath one project header in the task drawer tree.
    /// 构建显示在任务抽屉树某个项目头部下方的展开子行区域。
    fn view_main_task_project_children<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let has_visible_sessions = !project.session_ids.is_empty();
        let archived_sessions = self
            .archived_sessions
            .iter()
            .filter(|session| session.project_id.as_deref() == Some(project.id.as_str()))
            .collect::<Vec<_>>();
        let has_archived_sessions = !archived_sessions.is_empty();

        if !has_visible_sessions && !has_archived_sessions {
            return container(text("暂无对话").size(13).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.45),
                    ),
                }
            }))
            .padding(iced::Padding {
                top: 2.0,
                right: 2.0,
                bottom: 8.0,
                left: 34.0,
            })
            .width(Fill)
            .into();
        }

        let mut content = project.session_ids.iter().fold(
            column![].spacing(4).width(Fill),
            |column, session_id| {
                if let Some(session) = self.session_by_id(*session_id) {
                    column.push(self.view_main_task_session_tree_row(session))
                } else {
                    column
                }
            },
        );

        if has_archived_sessions {
            if has_visible_sessions {
                content = content.push(space::vertical().height(4));
            }
            content = content
                .push(self.view_main_task_archived_session_group_label())
                .push(archived_sessions.into_iter().fold(
                    column![].spacing(4).width(Fill),
                    |column, session| {
                        column.push(self.view_archived_task_session_tree_row(session))
                    },
                ));
        }

        content.into()
    }

    /// Builds one tree row shown under a project node inside the main task drawer.
    /// 构建显示在主任务抽屉某个项目节点下方的单条会话树节点。
    fn view_main_task_session_tree_row<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let is_selected =
            self.dragging_task_project.is_none() && session.id == self.selected_session;
        let is_hovered =
            self.dragging_task_project.is_none() && self.hovered_task_session == Some(session.id);
        let compact_meta = compact_relative_label(&session.last_updated_label);
        let session_title =
            truncate_with_ellipsis(&session.title, self.main_task_session_title_limit_width());
        let session_label = text(session_title)
            .size(14)
            .width(Fill)
            .wrapping(iced::advanced::text::Wrapping::None)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(theme.extended_palette().background.base.text),
            });
        let session_meta: Element<'a, Message> =
            container(text(compact_meta).size(12).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.55),
                    ),
                }
            }))
            .width(MAIN_TASK_SESSION_TRAILING_WIDTH)
            .align_right(MAIN_TASK_SESSION_TRAILING_WIDTH)
            .into();

        let archive_action = Self::view_main_task_tree_icon_action_button(
            TaskTreeActionIcon::Archive,
            Message::ArchiveTaskSession(session.id),
            is_hovered,
        );
        let trailing_slot: Element<'a, Message> = if is_hovered {
            container(archive_action)
                .width(MAIN_TASK_SESSION_TRAILING_WIDTH)
                .align_right(MAIN_TASK_SESSION_TRAILING_WIDTH)
                .center_y(Fill)
                .into()
        } else {
            session_meta
        };

        let session_button = button(
            container(
                row![
                    Self::view_main_task_session_status_icon(session.status),
                    session_label,
                    space::horizontal(),
                    trailing_slot,
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 35.0,
            })
            .width(Fill),
        )
        .padding(0)
        .width(Fill)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, is_selected, is_hovered)
        })
        .on_press(Message::SessionSelected(session.id));

        ContextMenu::new(
            mouse_area(
                container(session_button)
                    .height(MAIN_TASK_SESSION_ROW_HEIGHT)
                    .width(Fill),
            )
            .on_enter(Message::TaskSessionHovered(Some(session.id)))
            .on_exit(Message::TaskSessionHovered(None)),
            move || self.view_main_task_session_menu(session),
        )
        .into()
    }

    /// Builds the overflow menu displayed beneath one hovered project row.
    /// 构建显示在某个项目行下方的溢出菜单。
    fn view_main_task_project_menu<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let _project = project;
        let menu_width = 200.0;

        container(
            column![
                Self::view_main_task_tree_menu_button(
                    "从资源管理器中打开",
                    Message::OpenTaskProjectDirectory(project.id.clone()),
                ),
                Self::view_main_task_tree_menu_button(
                    if project.pinned {
                        "取消置顶"
                    } else {
                        "置顶"
                    },
                    Message::PinTaskProject(project.id.clone()),
                ),
                Self::view_main_task_tree_menu_button(
                    "重命名",
                    Message::RenameTaskProject(project.id.clone()),
                ),
                Self::view_main_task_tree_menu_button(
                    "归档",
                    Message::ArchiveTaskProject(project.id.clone()),
                ),
            ]
            .spacing(2)
            .width(menu_width),
        )
        .padding([6, 6])
        .width(menu_width)
        .style(style::main_task_tree_menu_style)
        .into()
    }

    /// Builds the overflow menu displayed beneath one session row.
    /// 构建显示在某个会话行下方的溢出菜单。
    fn view_main_task_session_menu<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let _session = session;
        let menu_width = 200.0;

        container(
            column![
                Self::view_main_task_tree_menu_button(
                    "重命名",
                    Message::RenameTaskSession(session.id),
                ),
                Self::view_main_task_tree_menu_button(
                    "归档",
                    Message::ArchiveTaskSession(session.id),
                ),
            ]
            .spacing(2)
            .width(menu_width),
        )
        .padding([6, 6])
        .width(menu_width)
        .style(style::main_task_tree_menu_style)
        .into()
    }

    /// Builds the compact archived-session subgroup label rendered inside one expanded project branch.
    /// 构建展示在某个展开项目分支内部的紧凑归档会话分组标签。
    fn view_main_task_archived_session_group_label(&self) -> Element<'_, Message> {
        container(text("归档会话").size(12).style(|theme: &Theme| {
            iced::widget::text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.5),
                ),
            }
        }))
        .padding(iced::Padding {
            top: 0.0,
            right: 2.0,
            bottom: 0.0,
            left: 35.0,
        })
        .width(Fill)
        .into()
    }

    /// Builds one archived project row that can be restored back into the visible task tree.
    /// 构建一条可恢复回可见任务树的归档项目行。
    fn view_archived_task_project_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let is_hovered = self.hovered_task_project.as_deref() == Some(project.id.as_str());
        let folder_icon = codicon::folder()
            .size(16)
            .line_height(1.0)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.7),
                ),
            });
        let project_label = text(truncate_with_ellipsis(
            &project.title,
            self.main_task_project_title_limit_width() + 26.0,
        ))
        .size(15)
        .width(Fill)
        .wrapping(iced::advanced::text::Wrapping::None)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.74),
            ),
        });

        ContextMenu::new(
            mouse_area(
                container(
                    row![
                        folder_icon,
                        project_label,
                        text("已归档").size(12).style(|theme: &Theme| {
                            iced::widget::text::Style {
                                color: Some(
                                    theme
                                        .extended_palette()
                                        .background
                                        .base
                                        .text
                                        .scale_alpha(0.45),
                                ),
                            }
                        }),
                    ]
                    .spacing(8)
                    .align_y(Center),
                )
                .padding(iced::Padding {
                    top: 3.0,
                    right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                    bottom: 3.0,
                    left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                })
                .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
                .width(Fill)
                .style(move |theme| {
                    style::main_task_tree_project_row_style(theme, is_hovered, false)
                }),
            )
            .on_press(Message::RestoreTaskProject(project.id.clone()))
            .on_enter(Message::TaskProjectHovered(Some(project.id.clone())))
            .on_exit(Message::TaskProjectHovered(None)),
            move || self.view_archived_task_project_menu(project),
        )
        .into()
    }

    /// Builds one archived session row that can be restored back into its visible project.
    /// 构建一条可恢复回其可见项目中的归档会话行。
    fn view_archived_task_session_tree_row<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let is_hovered =
            self.dragging_task_project.is_none() && self.hovered_task_session == Some(session.id);
        let session_label = text(truncate_with_ellipsis(
            &session.title,
            self.main_task_session_title_limit_width() + 40.0,
        ))
        .size(14)
        .width(Fill)
        .wrapping(iced::advanced::text::Wrapping::None)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.72),
            ),
        });

        let session_button = button(
            container(
                row![
                    Self::view_main_task_session_status_icon(session.status),
                    session_label,
                    space::horizontal(),
                    text("已归档").size(12).style(|theme: &Theme| {
                        iced::widget::text::Style {
                            color: Some(
                                theme
                                    .extended_palette()
                                    .background
                                    .base
                                    .text
                                    .scale_alpha(0.45),
                            ),
                        }
                    }),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 35.0,
            })
            .width(Fill),
        )
        .padding(0)
        .width(Fill)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, false, is_hovered)
        })
        .on_press(Message::RestoreTaskSession(session.id));

        ContextMenu::new(
            mouse_area(
                container(session_button)
                    .height(MAIN_TASK_SESSION_ROW_HEIGHT)
                    .width(Fill),
            )
            .on_enter(Message::TaskSessionHovered(Some(session.id)))
            .on_exit(Message::TaskSessionHovered(None)),
            move || self.view_archived_task_session_menu(session),
        )
        .into()
    }

    /// Builds the overflow menu displayed for one archived project row.
    /// 构建显示在某个归档项目行上的溢出菜单。
    fn view_archived_task_project_menu<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let menu_width = 200.0;

        container(
            column![
                Self::view_main_task_tree_menu_button(
                    "恢复",
                    Message::RestoreTaskProject(project.id.clone()),
                ),
                Self::view_main_task_tree_menu_button(
                    "从资源管理器中打开",
                    Message::OpenTaskProjectDirectory(project.id.clone()),
                ),
            ]
            .spacing(2)
            .width(menu_width),
        )
        .padding([6, 6])
        .width(menu_width)
        .style(style::main_task_tree_menu_style)
        .into()
    }

    /// Builds the overflow menu displayed for one archived session row.
    /// 构建显示在某个归档会话行上的溢出菜单。
    fn view_archived_task_session_menu<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let menu_width = 200.0;

        container(
            column![Self::view_main_task_tree_menu_button(
                "恢复",
                Message::RestoreTaskSession(session.id),
            )]
            .spacing(2)
            .width(menu_width),
        )
        .padding([6, 6])
        .width(menu_width)
        .style(style::main_task_tree_menu_style)
        .into()
    }

    /// Builds the small state icon shown before one session row inside the main task drawer tree.
    /// 构建显示在主任务抽屉树节点会话行前方的小型状态图标。
    fn view_main_task_session_status_icon(status: SessionState) -> Element<'static, Message> {
        let icon = match status {
            SessionState::Draft => codicon::history(),
            SessionState::Running => codicon::sync(),
            SessionState::Blocked => codicon::warning(),
            SessionState::Completed => codicon::check(),
        }
        .size(14)
        .line_height(1.0);

        container(icon).width(16).center_y(Fill).into()
    }

    /// Builds one compact icon-only action button used by the task-drawer tree actions.
    /// 构建任务抽屉树动作使用的紧凑纯图标按钮。
    fn view_main_task_tree_icon_action_button(
        icon: TaskTreeActionIcon,
        message: Message,
        is_visible: bool,
    ) -> Element<'static, Message> {
        let icon_alpha = if is_visible { 0.88 } else { 0.0 };
        let icon_widget = match icon {
            TaskTreeActionIcon::AddProjectFromFolder => lucide::folder_plus(),
            TaskTreeActionIcon::AddSession => lucide::square_pen(),
            TaskTreeActionIcon::Archive => codicon::archive(),
        }
        .size(14.5)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(icon_alpha),
            ),
        });

        if is_visible {
            button(
                container(icon_widget)
                    .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                    .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                    .center(Fill),
            )
            .padding(0)
            .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
            .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
            .style(style::subtle_panel_button_style)
            .on_press(message)
            .into()
        } else {
            container(icon_widget)
                .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .center(Fill)
                .into()
        }
    }

    /// Builds one compact menu button used inside the task-drawer project overflow menu.
    /// 构建任务抽屉项目溢出菜单内部使用的紧凑菜单按钮。
    fn view_main_task_tree_menu_button(
        label: &'static str,
        message: Message,
    ) -> Element<'static, Message> {
        button(container(text(label).size(13)).padding([6, 10]).width(Fill))
            .padding(0)
            .width(Fill)
            .style(style::main_task_tree_menu_button_style)
            .on_press(message)
            .into()
    }

    /// Returns whether one real AI call is currently in flight.
    /// 返回当前是否存在一个正在进行中的真实 AI 调用。
    fn has_active_run(&self) -> bool {
        self.active_run_id.is_some()
    }

    /// Clears the tracked active-run state after one AI call stops or finishes.
    /// 在一次 AI 调用停止或结束后清理已跟踪的活动运行状态。
    fn clear_active_run_tracking(&mut self) {
        self.active_run_id = None;
        self.active_run_session_id = None;
        self.active_run_handle = None;
    }

    /// Builds the compact send-or-stop button shown at the right side of prompt inputs.
    /// 构建显示在提示词输入框右侧的紧凑发送或停止按钮。
    fn view_run_trigger_button(&self) -> Element<'_, Message> {
        let is_running = self.has_active_run();
        let tooltip_text = if is_running {
            "停止当前 AI 调用"
        } else {
            "发送当前任务指令"
        };
        let button_style: fn(&Theme, button::Status) -> button::Style = if is_running {
            button::danger
        } else {
            button::primary
        };
        let trigger_message = if is_running {
            Message::StopRun
        } else {
            Message::StartRun
        };
        let icon: Element<'_, Message> = if is_running {
            codicon::debug_stop().size(16).line_height(1.0).into()
        } else {
            lucide::send().size(16).line_height(1.0).into()
        };

        tooltip(
            button(icon)
                .padding([10, 12])
                .style(button_style)
                .on_press(trigger_message),
            tooltip_text,
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Builds the compact transcript surface-mode switcher shown at the trailing end of the session-status row.
    /// 构建显示在会话状态行末端的紧凑转录表面模式切换器。
    fn view_transcript_surface_mode_switcher(&self) -> Element<'_, Message> {
        let has_selected_session = self.selected_session().is_some();
        let is_detail_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::DetailCards;
        let is_structured_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::Structured;
        let is_raw_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::RawSource;
        let detail_mode_button = {
            let button =
                button(text("渲染").size(12))
                    .padding([6, 10])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(theme, status, is_detail_mode)
                    });

            if has_selected_session && !is_detail_mode {
                button.on_press(Message::ShowSelectedSessionDocumentDetailCards)
            } else {
                button
            }
        };
        let structured_mode_button = {
            let button =
                button(text("正文").size(12))
                    .padding([6, 10])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(
                            theme,
                            status,
                            is_structured_mode,
                        )
                    });

            if has_selected_session && !is_structured_mode {
                button.on_press(Message::ShowSelectedSessionDocumentStructured)
            } else {
                button
            }
        };
        let raw_mode_button = {
            let button =
                button(text("原文").size(12))
                    .padding([6, 10])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(theme, status, is_raw_mode)
                    });

            if has_selected_session && !is_raw_mode {
                button.on_press(Message::ShowSelectedSessionDocumentRawSource)
            } else {
                button
            }
        };

        row![detail_mode_button, structured_mode_button, raw_mode_button]
            .spacing(8)
            .align_y(Center)
            .into()
    }

    /// Builds the central canvas displayed beside the task drawer in the main workbench panel.
    /// 构建显示在主工作台任务抽屉右侧的中央画布区域。
    fn view_main_canvas(&self) -> Element<'_, Message> {
        let active_session = self.selected_session();
        let runtime_summary = self.runtime_foundation.summary();
        let run_phase = self.selected_session_run_phase();
        let session_title = active_session
            .map(|session| session.title.as_str())
            .unwrap_or("未选择会话");
        let transcript_stream = self.view_main_canvas_transcript_stream();
        let transcript_surface_mode_switcher = self.view_transcript_surface_mode_switcher();
        let composer_panel = container(
            column![
                row![
                    text("任务输入").size(16),
                    space::horizontal(),
                    text(format!("执行模式：{}", self.execution_mode)).size(13),
                    text(if self.temperature_enabled {
                        format!("温度：{:.2}", self.temperature)
                    } else {
                        String::from("温度：模型默认")
                    })
                    .size(13),
                ]
                .spacing(12)
                .align_y(Center),
                row![
                    text_input("输入当前会话要执行的任务指令", &self.composer)
                        .on_input(Message::ComposerChanged)
                        .padding(12)
                        .width(Fill),
                    self.view_run_trigger_button(),
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    text("执行后将把用户输入、AI 输出与运行时事件写入当前会话。").size(12),
                    space::horizontal(),
                    text(if self.has_active_run() {
                        "运行中，可点击右侧停止按钮中止当前 AI 调用。"
                    } else {
                        "点击输入框右侧纸飞机按钮即可发起 AI 调用。"
                    })
                    .size(12),
                ]
                .spacing(10)
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .style(container::bordered_box);

        container(
            column![
                row![
                    column![
                        text("AI 执行工作台").size(26),
                        text("当前区域用于承载会话输出、执行事件和最小运行闭环。").size(13),
                    ]
                    .spacing(4),
                    space::horizontal(),
                    container(text(run_phase.label()).size(12))
                        .padding([4, 10])
                        .style(move |theme| style::workbench_run_phase_chip_style(
                            theme, run_phase
                        )),
                ]
                .align_y(Center),
                container(
                    row![
                        text(format!("当前会话：{session_title}")).size(14),
                        text(format!("当前模型：{}", self.active_model_summary())).size(13),
                        text(format!("激活代理：{}", runtime_summary.active_agent_name)).size(13),
                        text(format!(
                            "会话状态：{}",
                            active_session
                                .map(|session| session.status.label())
                                .unwrap_or("未选择")
                        ))
                        .size(13),
                        space::horizontal(),
                        transcript_surface_mode_switcher,
                    ]
                    .spacing(16)
                    .align_y(Center)
                    .width(Fill),
                )
                .width(Fill)
                .padding([12, 14])
                .style(container::bordered_box),
                transcript_stream,
                composer_panel,
            ]
            .spacing(14)
            .height(Fill),
        )
        .padding([16, 16])
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Builds the transcript content surface shown in the center of the main workbench canvas.
    /// 构建展示在主工作台中央画布中的会话内容表面。
    fn view_main_canvas_transcript_stream(&self) -> Element<'_, Message> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return container(
                column![
                    text("尚未选择会话").size(18),
                    text("先在左侧会话列表中选中一个会话，会话内容会显示在这里。").size(13),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .style(container::bordered_box)
            .into();
        };

        let has_any_document_content = self
            .selected_session_agent_view_document()
            .map(|document| !document.blocks.is_empty())
            .unwrap_or(false);

        if !has_any_document_content {
            return container(
                column![
                    text("尚未开始执行").size(18),
                    text("点击上方输入框右侧发送按钮后，用户输入、AI 输出和运行时事件都会在这里持续沉淀。")
                        .size(13),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .style(container::bordered_box)
            .into();
        }

        let base_surface: Element<'_, Message> =
            if self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::DetailCards {
                self.view_main_canvas_segmented_transcript_stream(session_id)
            } else {
                self.view_selected_session_agent_document_surface(session_id)
            };

        let mut layered_surfaces = vec![base_surface];

        if let Some(overlay) = self.view_selected_session_streaming_thinking_overlay(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_thinking_modal(session_id) {
            layered_surfaces.push(overlay);
        }

        stack(layered_surfaces).into()
    }

    /// Builds the unified rendered transcript stream used for per-block inspection during the transition period.
    /// 构建过渡期用于逐块检查的统一渲染转录流。
    fn view_main_canvas_segmented_transcript_stream(
        &self,
        session_id: usize,
    ) -> Element<'_, Message> {
        let Some(document) = self.rendered_session_agent_view_document(session_id) else {
            return container(text("当前没有可渲染的统一输出块。").size(14))
                .padding(14)
                .height(Fill)
                .style(container::bordered_box)
                .into();
        };
        let content_column = column![
            view_selectable_rendered_transcript::<Message>(
                &document,
                &self.app_theme,
                move |request| Message::SelectedSessionTranscriptDragAutoScrollChanged {
                    session_id,
                    request,
                },
                |interaction| match interaction {
                    SelectableTranscriptInteraction::ToggleThinkingInline(block_id) => {
                        Message::ToggleSelectedSessionThinkingInline(block_id)
                    }
                    SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                        Message::OpenSelectedSessionThinkingModal(block_id)
                    }
                },
            ),
            space::vertical().height(TRANSCRIPT_BOTTOM_SPACER_HEIGHT),
        ]
        .width(Fill);

        container(
            scrollable(content_column)
                .id(transcript_scrollable_id(session_id))
                .height(Fill)
                .spacing(10)
                .on_scroll(move |viewport| Message::SelectedSessionTranscriptScrolled {
                    session_id,
                    viewport,
                }),
        )
        .padding(14)
        .height(Fill)
        .style(container::bordered_box)
        .into()
    }

    /// Builds one reusable selectable Markdown body from the provided read-only buffer or fallback text.
    /// 根据提供的只读缓冲区或回退文本构建一块可复用的可选 Markdown 正文。
    fn view_workbench_markdown_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            text_editor(buffer)
                .height(Shrink)
                .wrapping(iced::widget::text::Wrapping::Word)
                .padding(0)
                .size(14)
                .style(style::workbench_transcript_raw_view_style)
                .on_action(on_action)
                .into()
        } else {
            text(fallback).size(14).into()
        }
    }

    /// Builds one reusable selectable raw-text body from the provided read-only buffer or fallback text.
    /// 根据提供的只读缓冲区或回退文本构建一块可复用的可选原文正文。
    fn view_workbench_raw_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            text_editor(buffer)
                .height(Shrink)
                .wrapping(iced::widget::text::Wrapping::Word)
                .padding(0)
                .style(style::workbench_transcript_raw_view_style)
                .on_action(on_action)
                .into()
        } else {
            text(fallback).size(14).into()
        }
    }

    /// Builds the selectable unified transcript panel for the selected session.
    /// 构建当前选中会话使用的可选统一转录面板。
    fn view_selected_session_agent_document_surface(
        &self,
        session_id: usize,
    ) -> Element<'_, Message> {
        let (buffer, on_action, fallback_text) = match self.agent_transcript_surface_mode {
            AgentTranscriptSurfaceMode::DetailCards => (
                self.session_agent_view_buffers.get(&session_id),
                Message::SelectedSessionDocumentStructuredAction
                    as fn(text_editor::Action) -> Message,
                self.selected_session_agent_view_selectable_document()
                    .map(|document| document.buffer().to_string())
                    .unwrap_or_default(),
            ),
            AgentTranscriptSurfaceMode::Structured => (
                self.session_agent_view_buffers.get(&session_id),
                Message::SelectedSessionDocumentStructuredAction
                    as fn(text_editor::Action) -> Message,
                self.selected_session_agent_view_selectable_document()
                    .map(|document| document.buffer().to_string())
                    .unwrap_or_default(),
            ),
            AgentTranscriptSurfaceMode::RawSource => (
                self.session_agent_view_raw_buffers.get(&session_id),
                Message::SelectedSessionDocumentRawSourceAction
                    as fn(text_editor::Action) -> Message,
                self.selected_session_agent_view_document()
                    .map(|document| agent_view_document_raw_source(&document))
                    .unwrap_or_default(),
            ),
        };

        let body: Element<'_, Message> = if let Some(buffer) = buffer {
            text_editor(buffer)
                .height(Shrink)
                .wrapping(iced::widget::text::Wrapping::Word)
                .padding(0)
                .size(14)
                .style(style::workbench_transcript_raw_view_style)
                .on_action(on_action)
                .into()
        } else {
            text(fallback_text).size(14).into()
        };
        let body_flow = column![
            body,
            space::vertical().height(TRANSCRIPT_BOTTOM_SPACER_HEIGHT),
        ]
        .width(Fill);

        container(
            scrollable(container(body_flow).width(Fill))
                .id(transcript_scrollable_id(session_id))
                .height(Fill)
                .on_scroll(move |viewport| Message::SelectedSessionTranscriptScrolled {
                    session_id,
                    viewport,
                }),
        )
        .padding([14, 16])
        .height(Fill)
        .style(container::bordered_box)
        .into()
    }

    /// Builds the centered thinking modal shown when the user explicitly opens one inline thinking block.
    /// 构建当用户显式打开某个内联思考区块时显示的居中思考弹层。
    fn view_selected_session_thinking_modal(
        &self,
        session_id: usize,
    ) -> Option<Element<'_, Message>> {
        let modal_state = self
            .active_thinking_modal
            .as_ref()
            .filter(|modal_state| modal_state.session_id == session_id)?;
        let document = self.rendered_session_agent_view_document(modal_state.session_id)?;
        let (title, document) =
            build_modal_thinking_surface_document(document, &modal_state.block_id)?;
        let progress = thinking_modal_progress(modal_state.started_at, modal_state.phase);
        Some(view_thinking_modal_surface(
            title,
            String::from("单击会话内标题可内联展开，双击可再次聚焦到这里。"),
            &document,
            &self.app_theme,
            progress,
            thinking_overlay_scrollable_id(session_id),
            move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            },
            |interaction| match interaction {
                SelectableTranscriptInteraction::ToggleThinkingInline(block_id) => {
                    Message::ToggleSelectedSessionThinkingInline(block_id)
                }
                SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                    Message::OpenSelectedSessionThinkingModal(block_id)
                }
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
            Message::CloseSelectedSessionThinkingModal,
        ))
    }

    /// Builds the floating streaming-thinking overlay shown while one active thinking message is still in progress.
    /// 构建当某条活动思考消息仍在流式进行时显示的浮动思考面板。
    fn view_selected_session_streaming_thinking_overlay(
        &self,
        session_id: usize,
    ) -> Option<Element<'_, Message>> {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            return None;
        }

        let message = self.streaming_thinking_message(session_id)?.clone();
        let mut document = build_agent_view_document(std::slice::from_ref(&message), &[]);
        let block = document.blocks.get_mut(0)?;
        let block_id = agent_view_block_id(session_id, 0, block);
        self.decorate_agent_view_block_for_render(session_id, &block_id, block);
        let (title, document) = build_overlay_thinking_surface_document(document)?;

        Some(view_thinking_overlay_surface(
            title,
            String::from("运行中思考会先停留在这里，完成后再回收到主会话。"),
            &document,
            &self.app_theme,
            thinking_overlay_scrollable_id(session_id),
            move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            },
            |interaction| match interaction {
                SelectableTranscriptInteraction::ToggleThinkingInline(block_id) => {
                    Message::ToggleSelectedSessionThinkingInline(block_id)
                }
                SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                    Message::OpenSelectedSessionThinkingModal(block_id)
                }
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
        ))
    }

    /// Builds the reusable workspace surface shown by the example panel.
    /// 构建由示例面板承载的可复用工作区主体。
    fn view_workspace_surface(&self) -> Element<'_, Message> {
        // Capture the current focus so the pane closure can render focus hints without borrowing issues.
        // 捕获当前焦点状态，使面板闭包可以在不引发借用冲突的情况下渲染焦点提示。
        let focus = self.focus;

        // Compose the resizable pane grid that hosts navigation, editor, and runtime controls.
        // 组合承载导航、编辑器与运行控制区的可调尺寸分栏网格。
        let pane_grid = PaneGrid::new(&self.panes, |pane, kind, _is_maximized| {
            // Build the contextual title row so each pane clearly communicates its responsibility.
            // 构建上下文标题行，让每个面板都明确表达自己的职责。
            let title = row![
                text(kind.title()).size(18),
                space::horizontal(),
                text(kind.subtitle()).size(13),
                text(if focus == Some(pane) {
                    "ACTIVE"
                } else {
                    "READY"
                })
                .size(12),
            ]
            .spacing(8)
            .align_y(Center);

            // Select the pane body based on the pane descriptor stored in the grid state.
            // 根据网格状态中存储的面板描述选择具体的面板内容。
            let body = match kind {
                PaneKind::Navigator => self.view_navigation(),
                PaneKind::Workbench => self.view_workbench(),
                PaneKind::Inspector => self.view_inspector(),
            };

            pane_grid::Content::new(body).title_bar(pane_grid::TitleBar::new(title).padding(12))
        })
        .width(Fill)
        .height(Fill)
        .spacing(12)
        .on_click(Message::PaneFocused)
        .on_resize(8, Message::PaneResized);

        container(pane_grid).height(Fill).into()
    }

    /// Builds the example panel content using the shared workspace surface without an extra outer frame.
    /// 使用共享工作区主体构建示例面板内容，并且不再额外包裹外层边框。
    fn view_example_panel(&self) -> Element<'_, Message> {
        let panel_header = row![
            column![
                text(MainDisplayPanel::Example.section_title()).size(22),
                text(MainDisplayPanel::Example.description()).size(13),
            ]
            .spacing(4),
            space::horizontal(),
            action_button(
                "新建会话",
                "创建一个新的探索会话用于承载额外任务。",
                Message::CreateSession,
            ),
        ]
        .spacing(10)
        .align_y(Center);

        column![
            container(panel_header).padding([4, 2]),
            self.view_workspace_surface(),
        ]
        .spacing(12)
        .height(Fill)
        .into()
    }

    /// Builds one shared placeholder panel for not-yet-implemented top-level feature areas.
    /// 为尚未实现的顶层功能区域构建共享占位面板。
    fn view_placeholder_main_panel(&self, panel: MainDisplayPanel) -> Element<'_, Message> {
        let summary = self.runtime_foundation.summary();
        let panel_rows = match panel {
            MainDisplayPanel::PromptSkills => self
                .runtime_foundation
                .skill_rows(RuntimeSkillKind::Regular),
            MainDisplayPanel::LuaSkills => self
                .runtime_foundation
                .skill_rows(RuntimeSkillKind::LuaSkills),
            MainDisplayPanel::Automation => {
                let mut rows = self
                    .runtime_foundation
                    .skill_rows(RuntimeSkillKind::Automation);
                rows.extend(self.runtime_foundation.tool_policy_rows());
                rows
            }
            MainDisplayPanel::Agents => self.runtime_foundation.agent_rows(),
            MainDisplayPanel::Plugins => {
                let mut rows = self.runtime_foundation.native_tool_rows();
                rows.extend(self.runtime_foundation.mcp_rows());
                rows
            }
            MainDisplayPanel::VulcanMemoryMesh => {
                self.runtime_foundation.skill_rows(RuntimeSkillKind::Memory)
            }
            _ => Vec::new(),
        };
        let content_rows = if panel_rows.is_empty() {
            column![text("空面板").size(14)].spacing(8)
        } else {
            panel_rows
                .into_iter()
                .fold(column![].spacing(8), |column, row_text| {
                    column.push(text(format!("• {row_text}")).size(13))
                })
        };
        let warning_rows = if summary.warnings.is_empty() {
            column![].spacing(0)
        } else {
            summary.warnings.iter().fold(
                column![text("运行时告警").size(14)].spacing(6),
                |column, warning| column.push(text(format!("• {warning}")).size(12)),
            )
        };
        container(
            column![
                text(panel.section_title()).size(24),
                text(panel.description()).size(13),
                text(format!(
                    "激活代理：{} ({}) · 激活模型：{}",
                    summary.active_agent_name, summary.active_agent_id, summary.active_model
                ))
                .size(13),
                text(format!(
                    "代理 {} / 已配置 {} / 协调 {} / 技能 {} / 原生工具 {} / 委派 {} / MCP {}/{}",
                    summary.built_agent_count,
                    summary.configured_agent_count,
                    summary.coordinator_count,
                    summary.skill_count,
                    summary.native_tool_count,
                    summary.delegate_tool_count,
                    summary.enabled_mcp_server_count,
                    summary.configured_mcp_server_count
                ))
                .size(12),
                content_rows,
                warning_rows,
            ]
            .spacing(10)
            .align_x(iced::Alignment::Start),
        )
        .padding([18, 20])
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Builds the LuaSkills management panel backed by the embedded host runtime and its live skill catalog.
    /// 构建由嵌入式宿主运行时及其实时技能目录驱动的 LuaSkills 管理面板。
    fn view_luaskills_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::LuaSkills.section_title()).size(22),
                text(MainDisplayPanel::LuaSkills.description()).size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let summary_lines = if let Some(catalog) = &self.luaskills_catalog {
            let enabled_skill_count = catalog.skills.iter().filter(|skill| skill.enabled).count();
            let managed_skill_count = catalog.skills.iter().filter(|skill| skill.managed).count();
            column![
                text(format!("运行时根：{}", catalog.runtime_root.display())).size(12),
                text(format!(
                    "system_lua_lib：{}",
                    catalog.system_lua_lib_dir.display()
                ))
                .size(12),
                text(format!(
                    "可见技能 {} · 已启用 {} · 受管安装 {} · 运行时入口 {}",
                    catalog.skills.len(),
                    enabled_skill_count,
                    managed_skill_count,
                    catalog.entry_count
                ))
                .size(12),
            ]
            .spacing(4)
        } else {
            column![
                text("尚未载入 LuaSkills 技能目录。").size(12),
                text("当嵌入式 LuaSkills 运行时就绪后，这里会显示当前可见技能。").size(12),
            ]
            .spacing(4)
        };

        let mut refresh_button = button("刷新").padding([8, 14]).style(button::secondary);
        if !self.luaskills_operation_in_flight {
            refresh_button = refresh_button.on_press(Message::RefreshLuaSkillsCatalog);
        }

        let source_placeholder = match self.luaskills_install_source_choice {
            LuaSkillsInstallSourceChoice::Github => "例如：LuaSkills/vulcan-codekit",
            LuaSkillsInstallSourceChoice::Url => "例如：https://example.com/skill-source.yaml",
        };

        let mut install_button = button("直接安装").padding([8, 14]);
        if !self.luaskills_operation_in_flight {
            install_button = install_button.on_press(Message::InstallLuaSkill);
        }

        let install_controls = container(
            column![
                row![
                    text("技能目录").size(18),
                    space::horizontal().width(Fill),
                    refresh_button,
                ]
                .align_y(Center),
                summary_lines,
                text(&self.luaskills_notice).size(12),
                row([
                    LuaSkillsInstallSourceChoice::Github,
                    LuaSkillsInstallSourceChoice::Url,
                ]
                .into_iter()
                .map(|choice| {
                    radio(
                        choice.label(),
                        choice,
                        Some(self.luaskills_install_source_choice),
                        Message::LuaSkillsInstallSourceChoiceSelected,
                    )
                    .into()
                }))
                .spacing(18)
                .align_y(Center),
                text_input(
                    "可选：显式技能标识符，例如 vulcan-codekit",
                    &self.luaskills_install_skill_id,
                )
                .on_input(Message::LuaSkillsInstallSkillIdChanged)
                .padding(10),
                text_input(source_placeholder, &self.luaskills_install_source)
                    .on_input(Message::LuaSkillsInstallSourceChanged)
                    .padding(10),
                row![
                    install_button,
                    text("说明：GitHub 与包地址都走 LuaSkills 0.4.0 的受管安装链。").size(12),
                ]
                .spacing(10)
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box);

        let skills_column = match &self.luaskills_catalog {
            Some(catalog) if catalog.skills.is_empty() => {
                column![text("当前没有可见 LuaSkills 技能。").size(13)].spacing(8)
            }
            Some(catalog) => catalog
                .skills
                .iter()
                .fold(column![].spacing(12), |column, skill| {
                    column.push(self.view_luaskills_skill_card(skill))
                }),
            None => column![
                text("LuaSkills 技能目录尚未可用。").size(13),
                text("请先等待启动引导完成，或点击上方“刷新”重新拉取目录。").size(12),
            ]
            .spacing(8),
        };

        let skill_list = container(
            column![
                text("可见技能列表").size(18),
                scrollable(skills_column).height(Fill),
            ]
            .spacing(10)
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .height(Fill);

        column![panel_header, install_controls, skill_list]
            .spacing(12)
            .height(Fill)
            .into()
    }

    /// Builds one LuaSkills skill card row used by the desktop management panel.
    /// 构建桌面管理面板使用的单个 LuaSkills 技能卡片行。
    fn view_luaskills_skill_card(&self, skill: &LuaSkillsCatalogSkill) -> Element<'_, Message> {
        let source_text = if let Some(source_locator) = &skill.source_locator {
            let source_type = match skill.source_type {
                Some(luaskills::SkillInstallSourceType::Github) => "GitHub",
                Some(luaskills::SkillInstallSourceType::Url) => "包地址",
                None => "未知来源",
            };
            format!("{source_type} · {source_locator}")
        } else if skill.system_owned {
            String::from("ROOT 宿主内建技能")
        } else {
            String::from("未记录来源")
        };

        let status_text = if skill.enabled {
            "已启用"
        } else {
            "已停用"
        };
        let manage_text = if skill.managed { "受管" } else { "非受管" };

        let mut action_row = row![].spacing(8).align_y(Center);
        if skill.enabled {
            let mut disable_button = button("停用").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                disable_button =
                    disable_button.on_press(Message::DisableLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(disable_button);
        } else {
            let mut enable_button = button("启用").padding([6, 10]);
            if !self.luaskills_operation_in_flight {
                enable_button =
                    enable_button.on_press(Message::EnableLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(enable_button);
        }

        if skill.managed {
            let mut update_button = button("更新").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                update_button =
                    update_button.on_press(Message::UpdateLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(update_button);
        }

        if skill.system_owned {
            action_row = action_row.push(text("ROOT 系统技能，不允许卸载").size(12));
        } else {
            let mut uninstall_button = button("卸载").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                uninstall_button =
                    uninstall_button.on_press(Message::UninstallLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(uninstall_button);
        }

        let detail_rows = if let Some(disabled_reason) = &skill.disabled_reason {
            column![
                text(format!(
                    "{} · {} · 根 {} · 运行时入口 {}",
                    status_text, manage_text, skill.root_name, skill.entry_count
                ))
                .size(12),
                text(format!("来源：{source_text}")).size(12),
                text(format!("停用原因：{disabled_reason}")).size(12),
                text(format!("路径：{}", skill.skill_dir.display())).size(12),
            ]
            .spacing(4)
        } else {
            column![
                text(format!(
                    "{} · {} · 根 {} · 运行时入口 {}",
                    status_text, manage_text, skill.root_name, skill.entry_count
                ))
                .size(12),
                text(format!("来源：{source_text}")).size(12),
                text(format!("路径：{}", skill.skill_dir.display())).size(12),
            ]
            .spacing(4)
        };

        container(
            column![
                row![
                    text(format!("{}  v{}", skill.display_name, skill.version)).size(16),
                    space::horizontal().width(Fill),
                    text(format!("ID: {}", skill.skill_id)).size(12),
                ]
                .align_y(Center),
                detail_rows,
                action_row,
            ]
            .spacing(10),
        )
        .padding(14)
        .style(container::rounded_box)
        .width(Fill)
        .into()
    }

    /// Builds the settings panel content without adding an extra outer frame layer.
    /// 构建设置面板内容，并且不再增加额外的外层边框层级。
    fn view_settings_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::Settings.section_title()).size(22),
                text(MainDisplayPanel::Settings.description()).size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        column![panel_header, self.view_inspector()]
            .spacing(12)
            .height(Fill)
            .into()
    }

    /// Builds the custom undecorated window title bar used by the main and diff windows.
    /// 构建主窗口与 Diff 窗口共用的自定义无系统装饰标题栏。
    fn view_window_title_bar(
        &self,
        window_id: window::Id,
        title: &'static str,
    ) -> Element<'_, Message> {
        // Resolve the current maximize state so the title bar can show restore affordances when needed.
        // 解析当前最大化状态，使标题栏在需要时可以展示还原窗口的控制符号。
        let maximize_icon = if self.is_window_maximized(window_id) {
            WindowControlIcon::Restore
        } else {
            WindowControlIcon::Maximize
        };

        // Build the draggable identity region so the custom title bar still behaves like a native window.
        // 构建可拖拽的身份区块，使自定义标题栏仍然保持接近原生窗口的拖动体验。
        let drag_region = mouse_area(
            container(
                row![
                    svg(svg::Handle::from_memory(LOGO_ASSET_BYTES))
                        .width(20)
                        .height(20),
                    text(title).size(14),
                    space::horizontal(),
                ]
                .spacing(10)
                .align_y(Center),
            )
            .width(Fill)
            .height(Fill)
            .padding([0, 10])
            .center_y(Fill),
        )
        .on_press(Message::WindowTitleBarPressed(window_id))
        .on_release(Message::WindowTitleBarReleased(window_id))
        .on_move(move |position| Message::WindowTitleBarPointerMoved(window_id, position))
        .on_exit(Message::WindowTitleBarExited(window_id))
        .on_double_click(Message::ToggleMaximizeWindow(window_id));

        // Build the standard window action cluster so the custom frame can minimize, maximize, and close.
        // 构建标准窗口动作区，使自定义边框可以完成最小化、最大化与关闭。
        let controls = row![
            self.view_window_control_button(
                WindowControlIcon::Minimize,
                style::window_control_button_style,
                Message::MinimizeWindow(window_id),
            ),
            self.view_window_control_button(
                maximize_icon,
                style::window_control_button_style,
                Message::ToggleMaximizeWindow(window_id),
            ),
            self.view_window_control_button(
                WindowControlIcon::Close,
                style::window_close_button_style,
                Message::CloseWindow(window_id),
            ),
        ]
        .spacing(0)
        .align_y(Center);

        row![container(drag_region).width(Fill).height(Fill), controls]
            .align_y(Center)
            .height(WINDOW_TITLE_BAR_HEIGHT)
            .into()
    }

    /// Renders one button inside the custom window title bar action cluster.
    /// 渲染自定义窗口标题栏动作区中的单个按钮。
    fn view_window_control_button(
        &self,
        icon: WindowControlIcon,
        style: impl Fn(&Theme, button::Status) -> button::Style + Copy + 'static,
        on_press: Message,
    ) -> Element<'_, Message> {
        button(
            container(self.view_window_control_icon(icon))
                .width(Fill)
                .height(Fill)
                .center(Fill),
        )
        .width(WINDOW_CONTROL_BUTTON_WIDTH)
        .height(WINDOW_TITLE_BAR_HEIGHT)
        .padding(0)
        .style(style)
        .on_press(on_press)
        .into()
    }

    /// Renders one mature codicon glyph used by the custom window control cluster.
    /// 渲染自定义窗口控制按钮组使用的单个成熟 codicon 图标。
    fn view_window_control_icon(&self, icon: WindowControlIcon) -> Element<'_, Message> {
        let icon_widget = match icon {
            WindowControlIcon::Minimize => codicon::chrome_minimize(),
            WindowControlIcon::Maximize => codicon::chrome_maximize(),
            WindowControlIcon::Restore => codicon::chrome_restore(),
            WindowControlIcon::Close => codicon::chrome_close(),
        }
        .size(WINDOW_CONTROL_ICON_SIZE)
        .line_height(1.0);

        container(icon_widget)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
    }

    /// Wraps one undecorated window content tree with invisible native resize handles on every edge and corner.
    /// 为无系统装饰窗口内容树包裹四边与四角的不可见原生缩放热区。
    fn wrap_with_window_resize_handles<'a>(
        &'a self,
        window_id: window::Id,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if self.is_window_maximized(window_id) {
            return content;
        }

        let thickness = WINDOW_RESIZE_HANDLE_THICKNESS;

        stack([
            content,
            column![
                row![
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::NorthWest,
                        thickness,
                        thickness,
                    ),
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::North,
                        Fill,
                        thickness,
                    ),
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::NorthEast,
                        thickness,
                        thickness,
                    ),
                ]
                .spacing(0)
                .height(thickness),
                row![
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::West,
                        thickness,
                        Fill,
                    ),
                    space::horizontal().width(Fill),
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::East,
                        thickness,
                        Fill,
                    ),
                ]
                .spacing(0)
                .height(Fill),
                row![
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::SouthWest,
                        thickness,
                        thickness,
                    ),
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::South,
                        Fill,
                        thickness,
                    ),
                    self.view_window_resize_handle(
                        window_id,
                        window::Direction::SouthEast,
                        thickness,
                        thickness,
                    ),
                ]
                .spacing(0)
                .height(thickness),
            ]
            .spacing(0)
            .width(Fill)
            .height(Fill)
            .into(),
        ])
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Builds one invisible resize handle that forwards pointer presses to the native window-resize action.
    /// 构建一个不可见缩放热区，将指针按下转发给原生窗口缩放动作。
    fn view_window_resize_handle(
        &self,
        window_id: window::Id,
        direction: window::Direction,
        width: impl Into<iced::Length>,
        height: impl Into<iced::Length>,
    ) -> Element<'_, Message> {
        let interaction = Self::window_resize_interaction(direction);

        container(
            mouse_area(container(space()).width(Fill).height(Fill))
                .interaction(interaction)
                .on_press(Message::StartWindowResize(window_id, direction)),
        )
        .width(width)
        .height(height)
        .into()
    }

    /// Maps one native resize direction to the matching cursor interaction shown on hover.
    /// 将某个原生缩放方向映射为悬停时展示的对应鼠标光标交互。
    fn window_resize_interaction(direction: window::Direction) -> iced::mouse::Interaction {
        match direction {
            window::Direction::North | window::Direction::South => {
                iced::mouse::Interaction::ResizingVertically
            }
            window::Direction::East | window::Direction::West => {
                iced::mouse::Interaction::ResizingHorizontally
            }
            window::Direction::NorthEast | window::Direction::SouthWest => {
                iced::mouse::Interaction::ResizingDiagonallyUp
            }
            window::Direction::NorthWest | window::Direction::SouthEast => {
                iced::mouse::Interaction::ResizingDiagonallyDown
            }
        }
    }

    /// Returns whether one tracked window is currently maximized.
    /// 返回某个受跟踪窗口当前是否处于最大化状态。
    fn is_window_maximized(&self, window_id: window::Id) -> bool {
        if self.main_window_id == Some(window_id) {
            self.main_window_maximized
        } else if self.diff_window_id == Some(window_id) {
            self.diff_window_maximized
        } else {
            false
        }
    }

    /// Builds the dedicated diff preview window so the panel can be iterated independently.
    /// 构建独立的 Diff 预览窗口，以便面板可以脱离主工作台单独迭代。
    fn view_diff_window(&self, window_id: window::Id) -> Element<'_, Message> {
        // Build the fixed top chrome so the diff window inherits the same custom frame language as the main workbench.
        // 构建固定在顶部的统一 chrome 区块，使 Diff 窗口继承与主工作台一致的自定义窗口框架语言。
        let top_chrome = container(self.view_window_title_bar(window_id, "Git Diff Review Panel"))
            .style(style::window_chrome_style);

        let diff_header = container(
            row![
                column![
                    text("Git Diff 预览窗口").size(24),
                    text("独立窗口，便于单独调整审查面板结构与信息层级。").size(13),
                ]
                .spacing(4),
                space::horizontal(),
                button("重新扫描当前工作区")
                    .padding([10, 14])
                    .style(button::primary)
                    .on_press(Message::ScanWorkspace),
            ]
            .align_y(Center),
        )
        .padding(14)
        .style(container::bordered_box);

        // Keep workspace and unstaged summary in a single top-level bar so the review area itself stays clean.
        // 将工作区与未暂存摘要收敛到同一条顶部信息栏，保持审查内容区本身更干净。
        let diff_context = container(
            row![
                text(&self.diff_summary).size(15),
                space::horizontal(),
                text(format!("工作区：{}", self.workspace_path)).size(13),
            ]
            .align_y(Center),
        )
        .padding([10, 14])
        .style(container::rounded_box);

        self.wrap_with_window_resize_handles(
            window_id,
            column![
                top_chrome,
                container(
                    column![
                        diff_header,
                        diff_context,
                        container(self.view_diff_panel())
                            .height(Fill)
                            .style(container::bordered_box),
                    ]
                    .spacing(12)
                )
                .padding(16)
                .height(Fill),
            ]
            .spacing(0)
            .height(Fill)
            .into(),
        )
    }

    /// Returns the window title corresponding to the target window id.
    /// 根据目标窗口标识返回对应窗口标题。
    fn title(&self, window_id: window::Id) -> String {
        if self.diff_window_id == Some(window_id) {
            String::from("Git Diff Review Panel")
        } else {
            String::from("Vulcan Code")
        }
    }

    /// Returns the current application theme to the iced runtime.
    /// 向 iced 运行时返回当前应用主题。
    fn theme(&self, _window_id: window::Id) -> Option<Theme> {
        Some(self.app_theme.clone())
    }

    /// Subscribes to native window close events so multi-window state stays consistent.
    /// 订阅原生窗口关闭事件，以保持多窗口状态一致。
    fn subscription(&self) -> Subscription<Message> {
        let transcript_drag_auto_scroll_subscription =
            if self.active_transcript_drag_auto_scroll.is_some() {
                iced::time::every(TRANSCRIPT_DRAG_AUTO_SCROLL_TICK_INTERVAL)
                    .map(|_| Message::SelectedSessionTranscriptDragAutoScrollTick)
            } else {
                Subscription::none()
            };

        let thinking_overlay_drag_auto_scroll_subscription =
            if self.active_thinking_overlay_drag_auto_scroll.is_some() {
                iced::time::every(TRANSCRIPT_DRAG_AUTO_SCROLL_TICK_INTERVAL)
                    .map(|_| Message::SelectedSessionThinkingOverlayDragAutoScrollTick)
            } else {
                Subscription::none()
            };

        let thinking_modal_animation_subscription = if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.phase != ThinkingModalPhase::Open)
        {
            iced::time::every(THINKING_MODAL_TICK_INTERVAL)
                .map(|_| Message::ThinkingModalAnimationTick)
        } else {
            Subscription::none()
        };

        let pending_thinking_inline_toggle_subscription =
            if self.pending_thinking_inline_toggle.is_some() {
                iced::time::every(THINKING_MODAL_TICK_INTERVAL)
                    .map(|_| Message::FlushPendingThinkingInlineToggle)
            } else {
                Subscription::none()
            };

        Subscription::batch([
            window::close_events().map(Message::WindowClosed),
            window::resize_events()
                .map(|(window_id, size)| Message::WindowResized(window_id, size)),
            iced::event::listen_with(Self::map_main_panel_shortcut_event),
            iced::event::listen_with(Self::map_window_pointer_event),
            transcript_drag_auto_scroll_subscription,
            thinking_overlay_drag_auto_scroll_subscription,
            thinking_modal_animation_subscription,
            pending_thinking_inline_toggle_subscription,
        ])
    }

    /// Maps one raw runtime event into a top-level panel switch shortcut when the key combination matches.
    /// 当按键组合匹配时，将某个原始运行时事件映射为顶层面板切换快捷键消息。
    fn map_main_panel_shortcut_event(
        event: iced::Event,
        _status: iced::event::Status,
        _window_id: window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                repeat,
                ..
            }) if !repeat => Some(Message::CloseSelectedSessionThinkingModal),
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                repeat,
                ..
            }) if !repeat && modifiers.control() && !modifiers.alt() && !modifiers.logo() => {
                MainDisplayPanel::from_shortcut_digit(key.to_latin(physical_key)?)
                    .map(Message::MainDisplayPanelSelected)
            }
            _ => None,
        }
    }

    /// Maps one raw runtime event into a tracked window-pointer message when panel resizing needs it.
    /// 当面板缩放需要时，将某个原始运行时事件映射为受跟踪的窗口鼠标消息。
    fn map_window_pointer_event(
        event: iced::Event,
        _status: iced::event::Status,
        window_id: window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::WindowPointerMoved(window_id, position))
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::WindowLeftButtonReleased(window_id))
            }
            _ => None,
        }
    }

    /// Renders the navigation pane with workspace controls and session queue.
    /// 渲染包含工作区控制与会话队列的导航面板。
    fn view_navigation(&self) -> Element<'_, Message> {
        // Build the workspace control card so path, scan, and session creation live together.
        // 构建工作区控制卡片，让路径、扫描与新建会话处于同一语义区块。
        let workspace_controls = container(
            column![
                text("工作区入口").size(18),
                text("填写 Git 仓库路径后点击扫描，即可在中间区域载入 Diff 测试结果。").size(13),
                text_input("请输入仓库路径", &self.workspace_path)
                    .on_input(Message::WorkspacePathChanged)
                    .padding(12),
                row![
                    action_button(
                        "立即扫描",
                        "同步工作区结构，并把 Git Diff 载入到中间测试区。",
                        Message::ScanWorkspace
                    ),
                    action_button(
                        "添加会话",
                        "创建新的任务会话并切换到该会话。",
                        Message::CreateSession
                    ),
                ]
                .spacing(10),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the session list dynamically so future runtime data can flow into the same structure.
        // 动态构建会话列表，以便后续真实运行数据可以复用同一结构。
        let session_list = self.sessions.iter().fold(
            column![text("会话队列").size(18)].spacing(12),
            |list, session| list.push(self.view_session_card(session)),
        );

        column![
            workspace_controls,
            scrollable(session_list).height(Fill).spacing(8),
        ]
        .spacing(14)
        .height(Fill)
        .into()
    }

    /// Renders the center workbench with prompt composition and patch editing.
    /// 渲染包含提示词编排与补丁编辑的中央工作台。
    fn view_workbench(&self) -> Element<'_, Message> {
        // Resolve the active session summary so the editor pane stays contextualized.
        // 解析当前激活的会话摘要，让编辑器区域始终具备上下文。
        let active_session = self.selected_session();

        // Build the composer card that captures the task goal before execution.
        // 构建用于收集任务目标的提示词卡片。
        let composer = container(
            column![
                text("任务指令").size(18),
                row![
                    text_input("例如：实现 GUI 基础设施并接入 iced 组件", &self.composer)
                        .on_input(Message::ComposerChanged)
                        .padding(12)
                        .width(Fill),
                    self.view_run_trigger_button(),
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    text(format!(
                        "当前会话：{}",
                        active_session
                            .map(|session| session.title.as_str())
                            .unwrap_or("未选择会话")
                    )),
                    space::horizontal(),
                    text(format!("聚焦：{}", self.execution_mode)).size(13),
                ]
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the execution mode radio group to expose the main orchestration styles.
        // 构建执行模式单选组，以显式暴露主要编排方式。
        let execution_modes = container(
            column![
                text("执行模式").size(18),
                row(ExecutionMode::ALL.iter().copied().map(|mode| {
                    radio(
                        mode.to_string(),
                        mode,
                        Some(self.execution_mode),
                        Message::ExecutionModeSelected,
                    )
                    .into()
                }))
                .spacing(18)
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the context bar to show how much prompt and repo budget is currently reserved.
        // 构建上下文进度条，用于展示当前已预留的提示词与仓库预算占用。
        let context_bar = container(
            column![
                row![
                    text("上下文预算").size(18),
                    space::horizontal(),
                    text(format!("{:.0}%", self.context_usage)),
                ]
                .align_y(Center),
                progress_bar(0.0..=100.0, self.context_usage),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the editor card to host code previews and future patch application workflows.
        // 构建编辑器卡片，用于承载代码预览与后续补丁应用流程。
        let editor = container(
            column![
                row![
                    text("补丁与骨架预览").size(18),
                    space::horizontal(),
                    text(format!(
                        "Provider: {} | Model: {}",
                        self.active_provider_summary(),
                        self.active_model_summary()
                    ))
                    .size(13),
                ]
                .align_y(Center),
                text_editor(&self.editor_content)
                    .height(Fill)
                    .padding(14)
                    .size(CODE_EDITOR_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .wrapping(iced::advanced::text::Wrapping::None)
                    .font(theme::code_font())
                    .highlight_with::<iced::highlighter::Highlighter>(
                        iced::highlighter::Settings {
                            theme: theme::code_theme(&self.app_theme),
                            token: String::from("rust"),
                        },
                        theme::code_highlight_format,
                    )
                    .on_action(Message::EditorActionPerformed),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(FillPortion(2))
        .style(container::bordered_box);

        // Build the diff window launcher card so the review panel can evolve in its own window.
        // 构建 Diff 窗口启动卡片，使审查面板可以在独立窗口中单独迭代。
        let repository_info: Element<'_, Message> = if self.diff_panel.repository_root.is_empty() {
            text("尚未载入仓库根目录信息。").size(13).into()
        } else {
            text(format!("仓库根目录：{}", self.diff_panel.repository_root))
                .size(13)
                .into()
        };

        let diff_window_launcher = container(
            column![
                row![
                    text("Git Diff 独立窗口").size(18),
                    space::horizontal(),
                    button(if self.diff_window_id.is_some() {
                        "定位 Diff 窗口"
                    } else {
                        "打开 Diff 窗口"
                    })
                    .padding([10, 14])
                    .style(button::primary)
                    .on_press(Message::OpenDiffWindow),
                ]
                .align_y(Center),
                text("点击按钮后会在新窗口中显示专用 Diff 审查面板，便于单独调整结构。").size(13),
                row![
                    text(format!("当前摘要：{}", self.diff_summary)).size(13),
                    space::horizontal(),
                    text(if self.diff_window_id.is_some() {
                        "状态：窗口已打开"
                    } else {
                        "状态：窗口未打开"
                    })
                    .size(13),
                ]
                .align_y(Center),
                repository_info,
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(FillPortion(1))
        .style(container::bordered_box);

        column![
            composer,
            execution_modes,
            context_bar,
            editor,
            diff_window_launcher
        ]
        .spacing(14)
        .height(Fill)
        .into()
    }

    /// Renders the structured Git diff panel inside the center workbench.
    /// 在中央工作台中渲染结构化 Git diff 面板。
    fn view_diff_panel(&self) -> Element<'_, Message> {
        // Render the fallback notice path when no structured diff files are currently available.
        // 当当前没有结构化 diff 文件时，渲染回退提示路径。
        if !self.diff_panel.has_files() {
            return scrollable(
                container(
                    column![
                        text(&self.diff_panel.notice).size(16),
                        rule::horizontal(1),
                        text(&self.diff_panel.fallback_text).size(13).width(Fill),
                    ]
                    .spacing(12),
                )
                .padding(14)
                .height(Fill)
                .style(container::rounded_box),
            )
            .height(Fill)
            .into();
        }

        // Build the file review flow so users can scan changed files top-to-bottom like a code review panel.
        // 构建文件级审查流，使用户可以像代码审查面板一样自上而下浏览改动。
        let visible_range = self.visible_diff_file_range();
        let mut review_flow = column![].spacing(0);

        if visible_range.top_spacer_height > 0.0 {
            review_flow = review_flow.push(space().height(visible_range.top_spacer_height));
        }

        for file_index in visible_range.start..visible_range.end {
            if file_index > visible_range.start {
                review_flow = review_flow.push(space().height(DIFF_FILE_CARD_SPACING));
            }

            if let Some(file) = self.diff_panel.files.get(file_index) {
                review_flow = review_flow
                    .push(self.view_diff_file_card(file, self.diff_file_top_offset(file_index)));
            }
        }

        if visible_range.bottom_spacer_height > 0.0 {
            review_flow = review_flow.push(space().height(visible_range.bottom_spacer_height));
        }

        let current_file_bar = self.view_current_diff_file_bar();

        column![
            current_file_bar,
            scrollable(review_flow)
                .id(DIFF_PANEL_SCROLLABLE_ID)
                .on_scroll(Message::DiffPanelScrolled)
                .height(Fill)
                .spacing(8)
        ]
        .spacing(8)
        .height(Fill)
        .into()
    }

    /// Renders the external current-file bar that mirrors the review scroll position.
    /// 渲染镜像审查滚动位置的外置当前文件提示条。
    fn view_current_diff_file_bar(&self) -> Element<'_, Message> {
        let Some(active_file) = self.active_diff_file() else {
            return space().height(0).into();
        };

        let file_key = active_file.file.key();
        let toggle_file_key = file_key.clone();
        let new_file_marker: Element<'_, Message> = if active_file.file.is_new_file() {
            text("●").size(11).color(iced::color!(0x218BFF)).into()
        } else {
            space().width(11).into()
        };

        container(
            button(
                row![
                    text("当前文件").size(11),
                    row![
                        new_file_marker,
                        column![
                            text(active_file.file.header_title()).size(14),
                            text(
                                active_file
                                    .file
                                    .full_path_label(&self.diff_panel.repository_root)
                            )
                            .size(10),
                        ]
                        .spacing(0),
                    ]
                    .spacing(8)
                    .align_y(Center),
                    space::horizontal(),
                    self.view_diff_file_header_metrics(
                        active_file.file.additions,
                        active_file.file.deletions,
                        active_file.is_collapsed,
                    ),
                ]
                .spacing(10)
                .align_y(Center),
            )
            .width(Fill)
            .height(DIFF_CURRENT_FILE_BAR_HEIGHT)
            .padding([4, 12])
            .style(button::text)
            .on_press(Message::ToggleAndFocusDiffFile(toggle_file_key)),
        )
        .style(container::bordered_box)
        .width(Fill)
        .into()
    }

    /// Renders one file card in the Git diff review flow.
    /// 在 Git diff 审查流中渲染单个文件卡片。
    fn view_diff_file_card<'a>(
        &'a self,
        file: &'a DiffFile,
        file_top_offset: f32,
    ) -> Element<'a, Message> {
        // Resolve the stable file key so collapse state survives rerenders and repeat scans.
        // 解析稳定文件键，保证折叠状态在重渲染和重复扫描后依然可追踪。
        let file_key = file.key();
        let is_collapsed = self.collapsed_diff_files.contains(&file_key);

        let file_header = self.view_diff_file_header(file, is_collapsed);

        let file_body: Element<'a, Message> = if is_collapsed {
            column![].into()
        } else {
            container(self.view_diff_file_code(file, file_top_offset))
                .width(Fill)
                .into()
        };

        container(column![file_header, file_body,].spacing(0))
            .style(container::bordered_box)
            .clip(true)
            .width(Fill)
            .into()
    }

    /// Renders the clickable header row shared by normal and sticky diff file cards.
    /// 渲染普通与吸顶 diff 文件卡片共用的可点击头部行。
    fn view_diff_file_header<'a>(
        &'a self,
        file: &'a DiffFile,
        is_collapsed: bool,
    ) -> Element<'a, Message> {
        button(self.view_diff_file_header_content(file, is_collapsed))
            .padding([8, 12])
            .height(DIFF_FILE_HEADER_HEIGHT)
            .width(Fill)
            .style(button::text)
            .on_press(Message::ToggleDiffFile(file.key()))
            .into()
    }

    /// Builds the shared visual content of one diff file header.
    /// 构建单个 diff 文件头部的共享视觉内容。
    fn view_diff_file_header_content<'a>(
        &'a self,
        file: &'a DiffFile,
        is_collapsed: bool,
    ) -> Element<'a, Message> {
        // Render a blue marker only for new files so freshly discovered entries stand out immediately.
        // 仅为新增文件渲染蓝色标记，以便新发现的条目可以被立即识别。
        let new_file_marker: Element<'a, Message> = if file.is_new_file() {
            text("●").size(11).color(iced::color!(0x218BFF)).into()
        } else {
            space().width(11).into()
        };

        // Keep the header compact while exposing both title and full path inside the same row height budget.
        // 在相同行高预算内同时显示标题与完整路径，并保持头部紧凑。
        row![
            row![
                new_file_marker,
                column![
                    text(file.header_title()).size(15),
                    text(file.full_path_label(&self.diff_panel.repository_root)).size(11),
                ]
                .spacing(0),
            ]
            .spacing(8)
            .align_y(Center),
            space::horizontal(),
            self.view_diff_file_header_metrics(file.additions, file.deletions, is_collapsed),
        ]
        .align_y(Center)
        .into()
    }

    /// Renders the shared trailing metrics cluster used by diff file headers and the current-file bar.
    /// 渲染 diff 文件头与当前文件条共用的尾部统计区块。
    fn view_diff_file_header_metrics(
        &self,
        additions: usize,
        deletions: usize,
        is_collapsed: bool,
    ) -> Element<'_, Message> {
        row![
            text(format!("+{additions}"))
                .size(15)
                .color(style::diff_bar_color(crate::diff::DiffLineKind::Added)),
            text(format!("-{deletions}"))
                .size(15)
                .color(style::diff_bar_color(crate::diff::DiffLineKind::Removed)),
            text(if is_collapsed { "▸" } else { "▾" }).size(DIFF_FOLD_ARROW_SIZE),
        ]
        .spacing(10)
        .align_y(Center)
        .into()
    }

    /// Renders one file-level diff body using the same editor foundation as the home page.
    /// 使用与首页相同的编辑器基础渲染文件级 diff 主体。
    fn view_diff_file_code<'a>(
        &'a self,
        file: &'a DiffFile,
        file_top_offset: f32,
    ) -> Element<'a, Message> {
        // Render a compact fallback when the file has no visible hunk payload.
        // 当文件没有可见 hunk 载荷时渲染紧凑回退提示。
        if file.hunks.is_empty() {
            return container(text("当前文件暂无可展示的文本 Diff。").size(13))
                .padding([10, 12])
                .height(DIFF_EMPTY_BODY_HEIGHT)
                .width(Fill)
                .into();
        }

        // Split the file into hunk cards and omission cards so discontinuous change regions are visually stable.
        // 将文件拆成 hunk 卡片与省略卡片，使不连续的变更区在视觉上更稳定。
        let file_key = file.key();
        let mut hunk_flow = column![].spacing(1);
        let mut hunk_top_offset =
            file_top_offset + DIFF_FILE_HEADER_HEIGHT + DIFF_FILE_BODY_VERTICAL_INSET;

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            if hunk_index > 0 {
                let omitted_lines = omitted_context_line_count(&file.hunks[hunk_index - 1], hunk);
                hunk_flow = hunk_flow.push(self.view_diff_gap_block(omitted_lines));
                hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                hunk_top_offset += DIFF_GAP_BLOCK_HEIGHT;
                hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
            }

            hunk_flow = hunk_flow.push(self.view_diff_hunk_block(
                file,
                &file_key,
                hunk_index,
                hunk,
                hunk_top_offset,
            ));

            let wrapped_key = diff_hunk_buffer_key(&file_key, hunk_index);
            if let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&wrapped_key) {
                hunk_top_offset += wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT;
            } else {
                hunk_top_offset += self.diff_hunk_block_height(&file_key, hunk_index, hunk);
            }
        }

        container(hunk_flow).padding([8, 10]).width(Fill).into()
    }

    /// Renders one omission block inserted between separated hunk editors.
    /// 渲染插入在分离 hunk 编辑器之间的省略块。
    fn view_diff_gap_block(&self, omitted_lines: usize) -> Element<'_, Message> {
        let label = if omitted_lines > 0 {
            format!("… 省略 {omitted_lines} 行未变化的内容")
        } else {
            String::from("… 分隔下一个变更片区")
        };

        container(
            container(
                row![
                    space().width(diff_gutter_block_width()),
                    container(text(label).size(12))
                        .padding([2, DIFF_CODE_EDITOR_HORIZONTAL_PADDING])
                        .width(Fill),
                ]
                .spacing(0)
                .width(Fill),
            )
            .center_y(DIFF_GAP_BLOCK_HEIGHT)
            .height(DIFF_GAP_BLOCK_HEIGHT),
        )
        .width(Fill)
        .height(DIFF_GAP_BLOCK_HEIGHT)
        .style(style::diff_block_style)
        .into()
    }

    /// Renders one hunk-level diff block with its own editor and gutter.
    /// 渲染带有独立编辑器与 gutter 的单个 hunk 级 diff 区块。
    fn view_diff_hunk_block<'a>(
        &'a self,
        file: &'a DiffFile,
        file_key: &str,
        hunk_index: usize,
        _hunk: &'a crate::diff::DiffHunk,
        _hunk_top_offset: f32,
    ) -> Element<'a, Message> {
        let buffer_key = diff_hunk_buffer_key(file_key, hunk_index);
        let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&buffer_key) else {
            return space().height(0).into();
        };
        let total_line_count = wrapped_hunk.visual_lines.len();
        let block_height = total_line_count as f32 * CODE_EDITOR_LINE_HEIGHT;
        let Some(slice_range) = self.diff_buffer_slices.get(&buffer_key).copied() else {
            return space().height(block_height).into();
        };
        let line_count = slice_range.len();
        let visible_slice = VisibleDiffHunkSlice {
            range: slice_range,
            top_hidden_height: slice_range.start as f32 * CODE_EDITOR_LINE_HEIGHT,
            bottom_hidden_height: (total_line_count.saturating_sub(slice_range.end)) as f32
                * CODE_EDITOR_LINE_HEIGHT,
        };

        if line_count == 0 {
            return space().height(block_height).into();
        }

        let content = self
            .diff_code_buffers
            .get(&buffer_key)
            .expect("Diff hunk buffer must exist before rendering");
        let syntax_token = code_view::syntax_token(&file.path);
        let action_buffer_key = buffer_key.clone();
        let hover_buffer_key = buffer_key.clone();
        let editor_font = theme::code_font();
        let visible_height = line_count as f32 * CODE_EDITOR_LINE_HEIGHT;
        let gutter_block_width = diff_gutter_block_width();
        let mut line_bars = column![].spacing(0);
        let mut line_arrow_column = column![].spacing(0);
        let mut line_number_backgrounds = column![].spacing(0);
        let mut line_number_column = column![].spacing(0);
        let mut current_bar_kind = None;
        let mut current_bar_count = 0usize;
        let mut current_gutter_kind = None;
        let mut current_gutter_count = 0usize;
        let hover_anchor_indices = wrapped_hunk
            .visual_lines
            .iter()
            .skip(visible_slice.range.start)
            .take(line_count)
            .map(|line| line.anchor_index)
            .collect::<Vec<_>>();
        let hovered_line_index = self.hovered_diff_line.as_ref().and_then(|hovered| {
            if hovered.file_key == buffer_key {
                Some(hovered.line_index)
            } else {
                None
            }
        });
        for (visible_line_offset, visual_line) in wrapped_hunk
            .visual_lines
            .iter()
            .enumerate()
            .skip(visible_slice.range.start)
            .take(line_count)
        {
            let absolute_visual_line_index = visible_line_offset;
            if current_bar_kind == Some(visual_line.kind) {
                current_bar_count += 1;
            } else {
                if let Some(previous_kind) = current_bar_kind {
                    let run_height = current_bar_count as f32 * CODE_EDITOR_LINE_HEIGHT;

                    line_bars = line_bars.push(
                        container(space())
                            .width(3)
                            .height(run_height)
                            .style(move |theme| style::diff_line_bar_style(theme, previous_kind)),
                    );
                }

                current_bar_kind = Some(visual_line.kind);
                current_bar_count = 1;
            }

            if current_gutter_kind == Some(visual_line.kind) {
                current_gutter_count += 1;
            } else {
                if let Some(previous_kind) = current_gutter_kind {
                    let run_height = current_gutter_count as f32 * CODE_EDITOR_LINE_HEIGHT;

                    line_number_backgrounds = line_number_backgrounds.push(
                        container(space())
                            .width(gutter_block_width)
                            .height(run_height)
                            .style(move |theme| {
                                style::diff_gutter_background_style(theme, previous_kind)
                            }),
                    );
                }

                current_gutter_kind = Some(visual_line.kind);
                current_gutter_count = 1;
            }

            line_arrow_column = line_arrow_column.push(
                container(
                    text(
                        if hovered_line_index == Some(visual_line.anchor_index)
                            && visual_line.anchor_index == absolute_visual_line_index
                        {
                            "▶"
                        } else {
                            ""
                        },
                    )
                    .size(10)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .color(iced::color!(0x24292F)),
                )
                .center_x(DIFF_GUTTER_ARROW_WIDTH)
                .height(CODE_EDITOR_LINE_HEIGHT),
            );

            line_number_column = line_number_column.push(
                container(
                    text(format_diff_line_number(
                        visual_line.old_number,
                        visual_line.new_number,
                    ))
                    .size(CODE_EDITOR_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .font(editor_font)
                    .color(style::diff_line_number_color(visual_line.kind)),
                )
                .width(DIFF_GUTTER_WIDTH)
                .height(CODE_EDITOR_LINE_HEIGHT)
                .align_right(DIFF_GUTTER_WIDTH),
            );
        }

        if let Some(previous_kind) = current_bar_kind {
            let run_height = current_bar_count as f32 * CODE_EDITOR_LINE_HEIGHT;

            line_bars = line_bars.push(
                container(space())
                    .width(3)
                    .height(run_height)
                    .style(move |theme| style::diff_line_bar_style(theme, previous_kind)),
            );
        }

        if let Some(previous_kind) = current_gutter_kind {
            let run_height = current_gutter_count as f32 * CODE_EDITOR_LINE_HEIGHT;

            line_number_backgrounds = line_number_backgrounds.push(
                container(space())
                    .width(gutter_block_width)
                    .height(run_height)
                    .style(move |theme| style::diff_gutter_background_style(theme, previous_kind)),
            );
        }

        let file_editor = WheelPassthrough::new(
            text_editor(content)
                .size(CODE_EDITOR_TEXT_SIZE)
                .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                .wrapping(iced::advanced::text::Wrapping::None)
                .min_height(visible_height)
                .max_height(visible_height)
                .height(visible_height)
                .padding(0)
                .font(editor_font)
                .style(style::diff_code_editor_style)
                .highlight_with::<iced::highlighter::Highlighter>(
                    iced::highlighter::Settings {
                        theme: theme::code_theme(&self.app_theme),
                        token: syntax_token.clone(),
                    },
                    theme::code_highlight_format,
                )
                .on_action(move |action| {
                    Message::DiffCodeAction(action_buffer_key.clone(), action)
                }),
        );

        let gutter_layer = container(stack([
            container(line_number_backgrounds)
                .width(gutter_block_width)
                .height(visible_height)
                .into(),
            row![
                line_bars,
                line_number_column,
                line_arrow_column,
                container(space())
                    .width(2)
                    .height(Fill)
                    .style(style::diff_gutter_separator_style),
            ]
            .spacing(0)
            .width(gutter_block_width)
            .height(visible_height)
            .into(),
        ]))
        .width(gutter_block_width)
        .height(visible_height);

        let visible_row = mouse_area(
            row![
                gutter_layer,
                container(file_editor)
                    .padding([0, DIFF_CODE_EDITOR_HORIZONTAL_PADDING])
                    .height(visible_height)
                    .width(Fill),
            ]
            .spacing(0)
            .height(visible_height)
            .width(Fill),
        )
        .on_move({
            let buffer_key = hover_buffer_key.clone();
            let hover_anchor_indices = hover_anchor_indices.clone();

            move |position| {
                let raw_line_index =
                    hovered_diff_line_index_from_cursor(position.y, hover_anchor_indices.len());
                let anchor_line_index = hover_anchor_indices
                    .get(raw_line_index)
                    .copied()
                    .unwrap_or(raw_line_index);

                Message::DiffCodeHoverMoved(buffer_key.clone(), anchor_line_index)
            }
        })
        .on_exit(Message::DiffCodeHoverExited(hover_buffer_key));

        container(
            column![
                space().height(visible_slice.top_hidden_height),
                visible_row,
                space().height(visible_slice.bottom_hidden_height),
            ]
            .spacing(0)
            .width(Fill),
        )
        .height(block_height)
        .style(style::diff_block_style)
        .clip(true)
        .width(Fill)
        .into()
    }

    /// Renders the inspector pane with runtime controls, guidance, and logs.
    /// 渲染包含运行控制、指引与日志的检查面板。
    fn view_inspector(&self) -> Element<'_, Message> {
        // Build the provider configuration card so the settings page mirrors the OpenCode-style provider workflow.
        // 构建供应商配置卡片，让设置页面尽量镜像 OpenCode 风格的供应商工作流。
        let provider_controls = {
            let provider_choices = provider_config::supported_provider_choices();
            let selected_provider = Some(self.selected_provider.clone());
            let configured_providers = self.configured_provider_choices();
            let selected_definition = self.selected_provider_definition();
            let provider_metadata: Element<'_, Message> =
                if let Some(definition) = selected_definition {
                    column![
                        text(format!("运行时：{}", definition.runtime.label())).size(12),
                        text(format!(
                            "接口地址：{}",
                            self.selected_provider_endpoint_summary()
                        ))
                        .size(12),
                        text(format!(
                            "模型目录：{}",
                            self.selected_provider_model_source_summary()
                        ))
                        .size(12),
                    ]
                    .spacing(4)
                    .into()
                } else {
                    container(column![]).into()
                };

            let provider_field_list = selected_definition.map_or_else(
                || column![text("当前没有可用的供应商定义。").size(13)].spacing(8),
                |definition| {
                    let fields = definition.input_fields();

                    fields
                        .into_iter()
                        .fold(column![].spacing(10), |form, field| {
                            let label = definition.field_label(field);
                            let placeholder = definition.field_placeholder(field);
                            let input = text_input(&placeholder, self.provider_form.get(field))
                                .on_input(move |value| Message::ProviderFieldChanged(field, value))
                                .padding(10)
                                .width(Fill);
                            let input = if field.is_secret() {
                                input.secure(true)
                            } else {
                                input
                            };

                            form.push(column![text(label).size(13), input].spacing(6).width(Fill))
                        })
                },
            );

            let configured_provider_list = if configured_providers.is_empty() {
                column![text("尚未保存任何供应商配置。").size(12)]
            } else {
                configured_providers
                    .into_iter()
                    .fold(column![].spacing(8), |list, provider| {
                        list.push(
                            button(
                                row![
                                    text(provider.name.clone()).size(13),
                                    space::horizontal(),
                                    text(self.configured_provider_model_summary(&provider.id))
                                        .size(12),
                                ]
                                .align_y(Center)
                                .width(Fill),
                            )
                            .padding([8, 10])
                            .width(Fill)
                            .style(if provider.id == self.selected_provider.id {
                                button::primary
                            } else {
                                button::secondary
                            })
                            .on_press(Message::ConfiguredProviderSelected(provider.id)),
                        )
                    })
            };

            container(
                column![
                    row![
                        text("模型供应商配置").size(18),
                        space::horizontal(),
                        text("~/.vulcan/vulcan-code.json").size(12),
                    ]
                    .align_y(Center),
                    pick_list(
                        provider_choices,
                        selected_provider,
                        Message::ProviderCatalogSelected
                    )
                    .width(Fill)
                    .placeholder("选择供应商"),
                    provider_metadata,
                    provider_field_list,
                    row![
                        button("保存配置")
                            .style(button::primary)
                            .on_press(Message::SaveProviderConfiguration),
                        button("移除配置")
                            .style(button::secondary)
                            .on_press(Message::RemoveProviderConfiguration),
                        selected_definition.map_or_else(
                            || button("尝试拉取模型").style(button::secondary),
                            |definition| {
                                let action = if definition.supports_remote_model_refresh() {
                                    Some(Message::RefreshProviderModels)
                                } else {
                                    None
                                };
                                let mut button_widget =
                                    button(if definition.supports_remote_model_refresh() {
                                        "尝试拉取模型"
                                    } else {
                                        "仅使用静态模型目录"
                                    })
                                    .style(button::secondary);
                                if let Some(message) = action {
                                    button_widget = button_widget.on_press(message);
                                }
                                button_widget
                            }
                        ),
                    ]
                    .spacing(10),
                    text(&self.provider_notice).size(12),
                    column![text("已保存的供应商").size(14), configured_provider_list,].spacing(8),
                ]
                .spacing(12),
            )
            .padding(16)
            .style(container::bordered_box)
        };

        // Build the model and theme selector card to centralize active-model switching and runtime tuning.
        // 构建模型与主题选择卡片，以集中管理激活模型切换与运行参数。
        let available_models = self.current_provider_model_choices();
        let selected_model = self.active_model_choice_for_selected_provider();
        let model_selector: Element<'_, Message> = if available_models.is_empty() {
            container(text("当前供应商没有可用模型，请先保存配置或刷新模型目录。").size(13))
                .padding(10)
                .style(container::bordered_box)
                .into()
        } else {
            pick_list(
                available_models,
                selected_model,
                Message::ActiveModelSelected,
            )
            .width(Fill)
            .placeholder("选择当前激活模型")
            .into()
        };
        let temperature_control: Element<'_, Message> = if self.temperature_enabled {
            slider(0.0..=1.0, self.temperature, Message::TemperatureChanged)
                .step(0.05)
                .into()
        } else {
            text("已关闭显式温度覆盖，当前将完全使用供应商或模型默认设置。")
                .size(12)
                .into()
        };

        let selectors = container(
            column![
                text("运行参数").size(18),
                model_selector,
                pick_list(
                    Theme::ALL,
                    Some(self.app_theme.clone()),
                    Message::ThemeSelected
                )
                .width(Fill)
                .placeholder("选择主题"),
                checkbox(self.temperature_enabled)
                    .label("显式传递温度参数")
                    .on_toggle(Message::TemperatureEnabledToggled),
                text(if self.temperature_enabled {
                    format!("温度：{:.2}", self.temperature)
                } else {
                    String::from("温度：使用模型默认值（不传此参数）")
                }),
                temperature_control,
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the policy controls card to surface engineering guardrails.
        // 构建策略控制卡片，以显式展示工程约束与防护开关。
        let policies = container(
            column![
                text("工程策略").size(18),
                checkbox(self.generate_plan)
                    .label("复杂任务优先生成计划文件")
                    .on_toggle(Message::GeneratePlanToggled),
                checkbox(self.run_cargo_check)
                    .label("执行前运行 cargo check")
                    .on_toggle(Message::RunCargoCheckToggled),
                checkbox(self.persist_memory)
                    .label("保留执行记忆与操作轨迹")
                    .on_toggle(Message::PersistMemoryToggled),
                toggler(self.auto_approve)
                    .label("自动批准低风险变更")
                    .on_toggle(Message::AutoApproveToggled),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the markdown guide card to demonstrate structured documentation rendering.
        // 构建 Markdown 指引卡片，用于展示结构化文档渲染能力。
        let guide = container(
            column![
                text("集成指引").size(18),
                self.view_workbench_markdown_body(
                    Some(&self.quick_guide_markdown_buffer),
                    QUICK_GUIDE_MARKDOWN.to_string(),
                    Message::QuickGuideMarkdownAction,
                ),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the activity log card to visualize system feedback and execution breadcrumbs.
        // 构建活动日志卡片，以可视化系统反馈与执行轨迹。
        let activity_log = container(
            self.activities.iter().enumerate().fold(
                column![
                    text("运行日志").size(18),
                    text("日志正文支持直接框选与复制。").size(12),
                ]
                .spacing(8),
                |list, (index, entry)| list.push(self.view_activity_card(index, entry)),
            ),
        )
        .padding(16)
        .style(container::bordered_box);

        let runtime_execution = self.view_runtime_execution_card();
        let mcp_configuration = self.view_mcp_configuration_card();

        scrollable(
            column![
                provider_controls,
                selectors,
                runtime_execution,
                mcp_configuration,
                policies,
                guide,
                activity_log
            ]
            .spacing(14),
        )
        .height(Fill)
        .spacing(8)
        .into()
    }

    /// Renders the execution runtime card that exposes active-agent selection and the latest preparation snapshot.
    /// 渲染执行运行时卡片，以暴露激活代理选择与最近一次执行准备快照。
    fn view_runtime_execution_card(&self) -> Element<'_, Message> {
        let summary = self.runtime_foundation.summary();
        let active_agent_choices = self.configured_agent_choices();
        let selected_active_agent = self.active_agent_choice();
        let preparation_rows = self
            .last_execution_preparation
            .as_ref()
            .map(|preparation| preparation.summary_rows())
            .unwrap_or_else(|| {
                vec![String::from(
                    "尚未触发执行，可点击工作台输入框右侧的发送按钮发起第一次 AI 调用。",
                )]
            });

        let preparation_list = preparation_rows
            .into_iter()
            .fold(column![].spacing(6), |column, row_text| {
                column.push(text(format!("• {row_text}")).size(12))
            });

        let active_agent_picker: Element<'_, Message> = if active_agent_choices.is_empty() {
            container(text("当前尚未声明任何代理定义。").size(13))
                .padding(10)
                .style(container::bordered_box)
                .into()
        } else {
            pick_list(
                active_agent_choices,
                selected_active_agent,
                Message::ActiveAgentSelected,
            )
            .width(Fill)
            .placeholder("选择激活代理")
            .into()
        };

        container(
            column![
                text("执行运行时").size(18),
                active_agent_picker,
                text(format!(
                    "当前激活代理：{} ({})",
                    summary.active_agent_name, summary.active_agent_id
                ))
                .size(12),
                text(format!(
                    "已构建代理 {} / 技能 {} / 原生工具 {} / 委派 {}",
                    summary.built_agent_count,
                    summary.skill_count,
                    summary.native_tool_count,
                    summary.delegate_tool_count
                ))
                .size(12),
                preparation_list,
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the MCP configuration card that exposes server summaries and one editable transport-aware server form.
    /// 渲染 MCP 配置卡片，以暴露服务器摘要与单个感知传输类型的可编辑服务器表单。
    fn view_mcp_configuration_card(&self) -> Element<'_, Message> {
        let server_choices = self.configured_mcp_server_choices();
        let selected_server = self.selected_mcp_server_choice();
        let transport_choices = mcp_transport_choices();
        let server_summary_rows = self.runtime_foundation.mcp_rows();
        let server_summary_list: Element<'_, Message> = if server_summary_rows.is_empty() {
            text("尚未配置 MCP 服务器。").size(12).into()
        } else {
            server_summary_rows
                .into_iter()
                .fold(column![].spacing(6), |column, row_text| {
                    column.push(text(format!("• {row_text}")).size(12))
                })
                .into()
        };
        let server_picker: Element<'_, Message> = if server_choices.is_empty() {
            container(text("当前没有可编辑的 MCP 服务器，可先点击“新增服务器”。").size(12))
                .padding(10)
                .style(container::bordered_box)
                .into()
        } else {
            pick_list(server_choices, selected_server, Message::McpServerSelected)
                .width(Fill)
                .placeholder("选择 MCP 服务器")
                .into()
        };
        let mut remove_button = button("删除服务器").style(button::secondary);
        if self.selected_mcp_server_id.is_some() {
            remove_button = remove_button.on_press(Message::RemoveMcpServerConfiguration);
        }
        let latest_discovery_list: Element<'_, Message> = if self.last_mcp_discovery_rows.is_empty()
            && self.last_mcp_discovery_errors.is_empty()
        {
            text("尚未执行真实 MCP 工具发现。").size(12).into()
        } else {
            let mut latest_rows = column![].spacing(6);

            for row_text in &self.last_mcp_discovery_rows {
                latest_rows = latest_rows.push(text(format!("• {row_text}")).size(12));
            }

            for error_text in &self.last_mcp_discovery_errors {
                latest_rows = latest_rows.push(text(format!("• 失败：{error_text}")).size(12));
            }

            latest_rows.into()
        };

        let transport_picker = pick_list(
            transport_choices,
            Some(self.mcp_server_form.transport),
            Message::McpServerTransportSelected,
        )
        .width(Fill)
        .placeholder("选择传输类型");
        let transport_fields: Element<'_, Message> = match self.mcp_server_form.transport {
            PersistedMcpTransportKind::Stdio => column![
                text_input("启动命令，例如 uvx", &self.mcp_server_form.command)
                    .on_input(Message::McpServerCommandChanged)
                    .padding(10)
                    .width(Fill),
                text_input(
                    "参数（使用 | 分隔），例如 mcp-server-filesystem | D:/openvulcan/vulcan-code",
                    &self.mcp_server_form.args_text,
                )
                .on_input(Message::McpServerArgsChanged)
                .padding(10)
                .width(Fill),
                text_input("工作目录（可选）", &self.mcp_server_form.cwd)
                    .on_input(Message::McpServerCwdChanged)
                    .padding(10)
                    .width(Fill),
                text_input(
                    "环境变量（可选，KEY=VALUE; KEY2=VALUE）",
                    &self.mcp_server_form.env_text,
                )
                .on_input(Message::McpServerEnvChanged)
                .padding(10)
                .width(Fill),
            ]
            .spacing(10)
            .into(),
            PersistedMcpTransportKind::Http => column![
                text_input(
                    "MCP URL，例如 https://example.com/mcp",
                    &self.mcp_server_form.url
                )
                .on_input(Message::McpServerUrlChanged)
                .padding(10)
                .width(Fill),
                text_input(
                    "Bearer Token（可选，仅填写 Token 内容）",
                    &self.mcp_server_form.bearer_token,
                )
                .on_input(Message::McpServerBearerTokenChanged)
                .padding(10)
                .width(Fill),
                text_input(
                    "HTTP 请求头（可选，KEY=VALUE; KEY2=VALUE）",
                    &self.mcp_server_form.headers_text,
                )
                .on_input(Message::McpServerHeadersChanged)
                .padding(10)
                .width(Fill),
            ]
            .spacing(10)
            .into(),
            PersistedMcpTransportKind::Sse => column![
                text_input(
                    "SSE URL，例如 https://example.com/sse",
                    &self.mcp_server_form.url
                )
                .on_input(Message::McpServerUrlChanged)
                .padding(10)
                .width(Fill),
                text(
                    "该模式面向旧式 MCP SSE 服务：客户端先连接 SSE 事件流，再根据 endpoint 事件公布的消息端点发起调用。",
                )
                .size(12),
                text_input(
                    "Bearer Token（可选，仅填写 Token 内容）",
                    &self.mcp_server_form.bearer_token,
                )
                .on_input(Message::McpServerBearerTokenChanged)
                .padding(10)
                .width(Fill),
                text_input(
                    "HTTP 请求头（可选，KEY=VALUE; KEY2=VALUE）",
                    &self.mcp_server_form.headers_text,
                )
                .on_input(Message::McpServerHeadersChanged)
                .padding(10)
                .width(Fill),
            ]
            .spacing(10)
            .into(),
        };

        container(
            column![
                row![
                    text("MCP 服务器").size(18),
                    space::horizontal(),
                    text("当前开放 stdio、http(streamable) 与 sse(legacy)").size(12),
                ]
                .align_y(Center),
                server_picker,
                checkbox(self.mcp_server_form.enabled)
                    .label("启用该服务器")
                    .on_toggle(Message::McpServerEnabledToggled),
                text_input("服务器名称", &self.mcp_server_form.name)
                    .on_input(Message::McpServerNameChanged)
                    .padding(10)
                    .width(Fill),
                transport_picker,
                transport_fields,
                row![
                    button("新增服务器")
                        .style(button::secondary)
                        .on_press(Message::CreateMcpServer),
                    button("保存服务器")
                        .style(button::primary)
                        .on_press(Message::SaveMcpServerConfiguration),
                    remove_button,
                ]
                .spacing(10),
                text(&self.mcp_notice).size(12),
                column![text("服务器摘要").size(14), server_summary_list].spacing(8),
                column![text("最近一次发现").size(14), latest_discovery_list].spacing(8),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders a single session card in the navigation pane.
    /// 渲染导航面板中的单个会话卡片。
    fn view_session_card<'a>(&self, session: &'a SessionSummary) -> Element<'a, Message> {
        // Determine whether the current card is the active navigation target.
        // 判断当前卡片是否为激活中的导航目标。
        let is_selected = session.id == self.selected_session;

        // Compose a compact summary that balances title, status, and workload hints.
        // 组合兼顾标题、状态与工作量提示的紧凑摘要。
        let content = column![
            row![
                text(&session.title).size(16),
                space::horizontal(),
                text(session.status.label()).size(12),
            ]
            .align_y(Center),
            text(&session.focus).size(13),
            row![
                text(format!("变更文件: {}", session.changed_files)).size(12),
                space::horizontal(),
                text(format!("排队步骤: {}", session.queued_steps)).size(12),
            ]
            .align_y(Center),
        ]
        .spacing(8)
        .width(Fill);

        button(content)
            .padding(14)
            .width(Fill)
            .style(if is_selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SessionSelected(session.id))
            .into()
    }

    /// Renders a single activity card inside the inspector log.
    /// 渲染检查面板日志中的单个活动卡片。
    fn view_activity_card<'a>(
        &'a self,
        index: usize,
        entry: &'a ActivityEntry,
    ) -> Element<'a, Message> {
        // Build a readable card so feedback can be scanned quickly during long-running tasks.
        // 构建易读卡片，便于在长任务期间快速扫描反馈。
        let detail_view = self.view_workbench_raw_body(
            self.activity_raw_buffers.get(index),
            entry.detail.clone(),
            move |action| Message::ActivityLogRawAction(index, action),
        );

        container(
            column![
                row![
                    text(entry.level.badge()).size(12),
                    space::horizontal(),
                    text(&entry.title).size(15),
                ]
                .align_y(Center),
                detail_view,
            ]
            .spacing(8),
        )
        .padding(12)
        .style(container::rounded_box)
        .into()
    }

    /// Renders the footer status row with high-level runtime context.
    /// 渲染包含高层运行上下文的底部状态栏。
    fn view_status_bar(&self) -> Element<'_, Message> {
        // Count finished or blocked sessions to expose queue health in a single glance.
        // 统计已完成或已阻塞的会话数量，以便一眼把握队列健康度。
        let stabilized_sessions = self
            .sessions
            .iter()
            .filter(|session| {
                matches!(
                    session.status,
                    SessionState::Completed | SessionState::Blocked
                )
            })
            .count();

        container(
            row![
                text(&self.status_message).size(13),
                space::horizontal(),
                text(format!("会话数: {}", self.sessions.len())).size(13),
                text(format!("稳定会话: {}", stabilized_sessions)).size(13),
                text(format!("主题: {}", self.app_theme)).size(13),
            ]
            .spacing(16)
            .align_y(Center),
        )
        .padding([10, 16])
        .height(44)
        .width(Fill)
        .into()
    }

    /// Resolves the initial provider picker choice using the persisted configuration and supported catalog order.
    /// 使用持久化配置与受支持目录顺序解析初始供应商选择项。
    fn resolve_initial_provider_choice(
        config: &ProviderConfigurationFile,
        choices: &[ProviderChoice],
    ) -> ProviderChoice {
        let preferred = config
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, _)| provider_id.to_string())
            .or_else(|| config.provider.keys().next().cloned());

        preferred
            .and_then(|provider_id| choices.iter().find(|item| item.id == provider_id).cloned())
            .or_else(|| choices.first().cloned())
            .unwrap_or_else(|| ProviderChoice {
                id: String::from("openai"),
                name: String::from("OpenAI"),
            })
    }

    /// Resolves the initial MCP server identifier using the persisted configuration order.
    /// 使用持久化配置顺序解析初始 MCP 服务器标识。
    fn resolve_initial_mcp_server_id(config: &ProviderConfigurationFile) -> Option<String> {
        config.mcp.servers.keys().next().cloned()
    }

    /// Builds the configured agent choices rendered in the runtime settings card.
    /// 构建渲染在运行时设置卡片中的已配置代理选择项。
    fn configured_agent_choices(&self) -> Vec<AgentChoice> {
        std::iter::once(AgentChoice {
            id: String::from(SYSTEM_DEFAULT_AGENT_CHOICE_ID),
            label: String::from("系统默认（按模型）"),
        })
        .chain(
            self.agent_profile_catalog
                .agents
                .values()
                .map(|profile| AgentChoice {
                    id: profile.id.clone(),
                    label: profile.name.clone(),
                }),
        )
        .collect()
    }

    /// Returns the currently active agent choice rendered in the runtime settings card.
    /// 返回渲染在运行时设置卡片中的当前激活代理选择项。
    fn active_agent_choice(&self) -> Option<AgentChoice> {
        let active_agent_id = self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            .unwrap_or(SYSTEM_DEFAULT_AGENT_CHOICE_ID);

        self.configured_agent_choices()
            .into_iter()
            .find(|choice| choice.id == active_agent_id)
    }

    /// Builds the configured MCP server choices rendered in the MCP settings card.
    /// 构建渲染在 MCP 设置卡片中的已配置 MCP 服务器选择项。
    fn configured_mcp_server_choices(&self) -> Vec<McpServerChoice> {
        self.provider_configuration
            .mcp
            .servers
            .iter()
            .map(|(id, entry)| McpServerChoice {
                id: id.clone(),
                label: format!(
                    "{} · {}",
                    entry.name,
                    if entry.enabled { "启用" } else { "禁用" }
                ),
            })
            .collect()
    }

    /// Returns the currently selected MCP server choice rendered in the MCP settings card.
    /// 返回渲染在 MCP 设置卡片中的当前选中 MCP 服务器选择项。
    fn selected_mcp_server_choice(&self) -> Option<McpServerChoice> {
        let selected_server_id = self.selected_mcp_server_id.as_deref()?;

        self.configured_mcp_server_choices()
            .into_iter()
            .find(|choice| choice.id == selected_server_id)
    }

    /// Reloads the editable MCP server form from the currently selected persisted entry.
    /// 根据当前选中的持久化条目重新载入可编辑 MCP 服务器表单。
    fn reload_selected_mcp_server_form(&mut self) {
        self.mcp_server_form = self
            .selected_mcp_server_id
            .as_deref()
            .and_then(|server_id| self.provider_configuration.mcp.servers.get(server_id))
            .map(McpServerFormState::from_entry)
            .unwrap_or_else(|| McpServerFormState {
                enabled: true,
                ..McpServerFormState::default()
            });
    }

    /// Reloads the editable provider form from the persisted configuration entry of the selected provider.
    /// 从当前选中供应商的持久化配置条目重新载入可编辑表单。
    fn reload_selected_provider_form(&mut self) {
        self.provider_form = self
            .provider_configuration
            .provider
            .get(&self.selected_provider.id)
            .map(|entry| ProviderFormState::from_options(&entry.options))
            .unwrap_or_default();
    }

    /// Returns the currently selected provider definition, if it is supported.
    /// 返回当前选中且受支持的供应商定义（如果存在）。
    fn selected_provider_definition(
        &self,
    ) -> Option<&'static provider_config::SupportedProviderDefinition> {
        provider_config::supported_provider_by_id(&self.selected_provider.id)
    }

    /// Returns the configured provider list rendered in the provider settings card.
    /// 返回渲染在供应商设置卡片中的已配置供应商列表。
    fn configured_provider_choices(&self) -> Vec<ProviderChoice> {
        provider_config::supported_provider_choices()
            .into_iter()
            .filter(|choice| {
                self.provider_configuration
                    .provider
                    .contains_key(&choice.id)
            })
            .collect()
    }

    /// Returns the effective model list for the currently selected provider.
    /// 返回当前选中供应商的生效模型列表。
    fn current_provider_model_choices(&self) -> Vec<ModelChoice> {
        provider_config::effective_model_choices(
            &self.selected_provider.id,
            &self.provider_configuration,
        )
    }

    /// Returns the resolved endpoint summary for the currently selected provider form state.
    /// 返回当前选中供应商表单状态对应的解析后接口地址摘要。
    fn selected_provider_endpoint_summary(&self) -> String {
        self.selected_provider_definition()
            .and_then(|definition| definition.resolved_base_url(&self.provider_form.to_options()))
            .unwrap_or_else(|| String::from("由运行时自动解析"))
    }

    /// Returns the model-source summary computed from the current provider definition and cache state.
    /// 返回根据当前供应商定义与缓存状态计算出的模型来源摘要。
    fn selected_provider_model_source_summary(&self) -> String {
        let models = self.current_provider_model_choices();

        self.selected_provider_definition()
            .map(|definition| {
                if definition.supports_remote_model_refresh() {
                    let has_cached_models = self
                        .provider_configuration
                        .provider
                        .get(&self.selected_provider.id)
                        .is_some_and(|entry| !entry.models.is_empty());

                    if has_cached_models {
                        format!("已缓存 {} 个模型", models.len())
                    } else {
                        format!("可远端拉取，当前内置 {} 个模型", models.len())
                    }
                } else {
                    format!("使用内置目录，共 {} 个模型", models.len())
                }
            })
            .unwrap_or_else(|| String::from("未找到供应商定义"))
    }

    /// Returns the currently active model choice when it belongs to the selected provider.
    /// 当当前激活模型属于选中供应商时，返回对应的模型选项。
    fn active_model_choice_for_selected_provider(&self) -> Option<ModelChoice> {
        let (provider_id, model_id) = self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)?;

        if provider_id != self.selected_provider.id {
            return None;
        }

        self.current_provider_model_choices()
            .into_iter()
            .find(|model| model.id == model_id)
    }

    /// Returns the summary text rendered for one configured provider entry.
    /// 返回某个已配置供应商条目渲染使用的摘要文本。
    fn configured_provider_model_summary(&self, provider_id: &str) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .filter(|(active_provider_id, _)| *active_provider_id == provider_id)
            .map(|(_, model_id)| {
                format!(
                    "当前：{}",
                    provider_config::model_summary(
                        provider_id,
                        model_id,
                        &self.provider_configuration,
                    )
                )
            })
            .unwrap_or_else(|| String::from("未设为当前模型"))
    }

    /// Persists the selected provider form back into the Vulcan configuration file.
    /// 将当前选中供应商表单持久化回 Vulcan 配置文件。
    fn save_selected_provider_configuration(&mut self) -> Result<(), String> {
        let definition = self
            .selected_provider_definition()
            .ok_or_else(|| String::from("当前供应商未找到可用定义。"))?;

        for field in definition.input_fields() {
            let value = self.provider_form.get(field).trim();
            if value.is_empty() {
                return Err(format!("请先填写 `{}`。", definition.field_label(field)));
            }
        }

        let entry = self
            .provider_configuration
            .provider
            .entry(definition.id.clone())
            .or_insert_with(PersistedProviderEntry::default);
        entry.name = Some(definition.name.clone());
        entry.options = self.provider_form.to_options();

        if entry.models.is_empty() {
            entry.models = provider_config::persist_remote_models(&definition.models);
        }

        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Persists the currently edited MCP server form back into the shared Vulcan configuration file.
    /// 将当前编辑中的 MCP 服务器表单持久化回共享 Vulcan 配置文件。
    fn save_selected_mcp_server_configuration(&mut self) -> Result<String, String> {
        let entry = self.mcp_server_form.to_entry()?;
        let current_server_id = self.selected_mcp_server_id.clone();
        let next_server_id = current_server_id
            .clone()
            .unwrap_or_else(|| self.generate_mcp_server_id(&entry.name));

        self.provider_configuration
            .mcp
            .servers
            .insert(next_server_id.clone(), entry);
        provider_config::save_configuration_file(&self.provider_configuration)?;

        Ok(next_server_id)
    }

    /// Removes the currently selected MCP server entry from the shared Vulcan configuration file.
    /// 从共享 Vulcan 配置文件中移除当前选中的 MCP 服务器条目。
    fn remove_selected_mcp_server_configuration(&mut self) -> Result<String, String> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Err(String::from("当前没有可移除的 MCP 服务器。"));
        };
        let Some(removed_entry) = self.provider_configuration.mcp.servers.remove(&server_id) else {
            return Err(String::from("当前选中的 MCP 服务器不存在。"));
        };

        provider_config::save_configuration_file(&self.provider_configuration)?;
        self.selected_mcp_server_id =
            Self::resolve_initial_mcp_server_id(&self.provider_configuration);

        Ok(removed_entry.name)
    }

    /// Builds a stable MCP server identifier from the current form name while preserving uniqueness.
    /// 根据当前表单名称构建稳定且唯一的 MCP 服务器标识。
    fn generate_mcp_server_id(&self, name: &str) -> String {
        let mut base = slugify_identifier(name);
        if base.is_empty() {
            base = String::from("mcp-server");
        }

        if !self.provider_configuration.mcp.servers.contains_key(&base) {
            return base;
        }

        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if !self
                .provider_configuration
                .mcp
                .servers
                .contains_key(&candidate)
            {
                return candidate;
            }
            suffix = suffix.saturating_add(1);
        }
    }

    /// Returns the active provider summary used by the workbench header and activity log.
    /// 返回工作台头部与活动日志使用的当前供应商摘要。
    fn active_provider_summary(&self) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, _)| provider_config::provider_summary(provider_id))
            .unwrap_or_else(|| String::from("未配置供应商"))
    }

    /// Returns the active model summary used by the workbench header and activity log.
    /// 返回工作台头部与活动日志使用的当前模型摘要。
    fn active_model_summary(&self) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, model_id)| {
                provider_config::model_summary(provider_id, model_id, &self.provider_configuration)
            })
            .unwrap_or_else(|| String::from("未配置模型"))
    }

    /// Rebuilds the compose runtime foundation from the current shared configuration state.
    /// 根据当前共享配置状态重建 compose 运行时基础架构。
    fn refresh_runtime_foundation(&mut self) {
        self.runtime_foundation = RuntimeFoundation::build(
            &self.provider_configuration,
            &self.agent_profile_catalog,
            self.luaskills_runtime.clone(),
            self.mcp_runtime.clone(),
        );
        self.last_execution_preparation = None;
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
    }

    /// Restarts the long-lived MCP runtime after configuration changes and rebuilds the runtime foundation immediately.
    /// 在配置变更后重启长期 MCP 运行时，并立即重建运行时基础架构。
    fn restart_mcp_runtime_task(&mut self) -> Task<Message> {
        if let Some(handle) = self.mcp_runtime.take() {
            handle.shutdown();
        }
        self.refresh_runtime_foundation();
        Task::perform(
            bootstrap_mcp_runtime(self.provider_configuration.clone()),
            Message::McpRuntimeBootstrapped,
        )
    }

    /// Returns the currently selected session, if any.
    /// 返回当前被选中的会话（如果存在）。
    fn selected_session(&self) -> Option<&SessionSummary> {
        self.sessions
            .iter()
            .find(|session| session.id == self.selected_session)
    }

    /// Returns the currently selected session with mutability, if any.
    /// 返回当前被选中的可变会话（如果存在）。
    fn selected_session_mut(&mut self) -> Option<&mut SessionSummary> {
        self.sessions
            .iter_mut()
            .find(|session| session.id == self.selected_session)
    }

    /// Returns whether one session already exists inside the visible or archived project tree bindings.
    /// 返回某个会话是否已经存在于可见或归档项目树绑定关系中。
    fn session_is_materialized(&self, session_id: usize) -> bool {
        self.task_projects
            .iter()
            .chain(self.archived_task_projects.iter())
            .any(|project| project.session_ids.contains(&session_id))
    }

    /// Removes transient draft sessions that have not produced any real conversation yet.
    /// 移除尚未产出任何真实对话的临时草稿会话。
    fn prune_unmaterialized_sessions(&mut self, retained_session_id: Option<usize>) {
        let removable_ids = self
            .sessions
            .iter()
            .filter(|session| Some(session.id) != retained_session_id)
            .filter(|session| !self.session_is_materialized(session.id))
            .map(|session| session.id)
            .collect::<Vec<_>>();

        if removable_ids.is_empty() {
            return;
        }

        self.sessions
            .retain(|session| !removable_ids.contains(&session.id));

        for session_id in removable_ids {
            self.clear_session_transcript_view_state(session_id);
            self.session_messages.remove(&session_id);
            self.session_compactions.remove(&session_id);
            self.session_runtime_state.remove(&session_id);
            self.session_runtime_events.remove(&session_id);
            self.session_run_phases.remove(&session_id);
            if self.hovered_task_session == Some(session_id) {
                self.hovered_task_session = None;
            }
        }

        self.ensure_valid_selected_session();
    }

    /// Removes transient draft sessions that still belong to one project but have never been materialized into the visible tree.
    /// 移除仍归属于某个项目但从未正式进入可见树的临时草稿会话。
    fn drop_unmaterialized_sessions_for_project(&mut self, project_id: &str) {
        let removable_ids = self
            .sessions
            .iter()
            .filter(|session| session.project_id.as_deref() == Some(project_id))
            .filter(|session| !self.session_is_materialized(session.id))
            .map(|session| session.id)
            .collect::<Vec<_>>();

        for session_id in removable_ids {
            self.clear_session_transcript_view_state(session_id);
            self.sessions.retain(|session| session.id != session_id);
            self.session_messages.remove(&session_id);
            self.session_compactions.remove(&session_id);
            self.session_runtime_state.remove(&session_id);
            self.session_runtime_events.remove(&session_id);
            self.session_run_phases.remove(&session_id);
        }

        self.ensure_valid_selected_session();
    }

    /// Materializes one transient draft session into its owning project after the first real user turn arrives.
    /// 在首条真实用户轮次到来后，将一个临时草稿会话正式归入其所属项目。
    fn materialize_session_after_first_user_turn(&mut self, session_id: usize) {
        if self.session_is_materialized(session_id) {
            return;
        }

        let Some(project_id) = self
            .session_by_id(session_id)
            .and_then(|session| session.project_id.clone())
        else {
            return;
        };

        if let Some(project) = self.task_project_by_id_mut(&project_id) {
            project.session_ids.insert(0, session_id);
        } else {
            return;
        }

        self.expanded_task_projects.insert(project_id.clone());
        self.append_session_explore_output(session_id, Some(&project_id));
    }

    /// Builds one persistence-safe workspace snapshot that excludes transient draft sessions until they become real conversations.
    /// 构建一份可安全持久化的工作台快照，在临时草稿会话成为真实对话前将其排除在外。
    fn build_persistable_workspace_snapshot(&self) -> WorkspaceSnapshot {
        let visible_session_ids = self
            .task_projects
            .iter()
            .flat_map(|project| project.session_ids.iter().copied())
            .collect::<BTreeSet<_>>();
        let archived_session_ids = self
            .archived_sessions
            .iter()
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();

        let visible_sessions = self
            .sessions
            .iter()
            .filter(|session| visible_session_ids.contains(&session.id))
            .cloned()
            .collect::<Vec<_>>();
        let visible_session_ids = visible_sessions
            .iter()
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();
        let selected_session = if visible_session_ids.contains(&self.selected_session)
            || archived_session_ids.contains(&self.selected_session)
        {
            self.selected_session
        } else {
            visible_sessions
                .first()
                .map(|session| session.id)
                .or_else(|| self.archived_sessions.first().map(|session| session.id))
                .unwrap_or(0)
        };

        WorkspaceSnapshot {
            task_projects: self.task_projects.clone(),
            archived_task_projects: self.archived_task_projects.clone(),
            sessions: visible_sessions,
            archived_sessions: self.archived_sessions.clone(),
            session_messages: self
                .session_messages
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .map(|(session_id, messages)| (*session_id, messages.clone()))
                .collect(),
            archived_session_messages: self.archived_session_messages.clone(),
            session_compactions: self
                .session_compactions
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .map(|(session_id, entries)| (*session_id, entries.clone()))
                .collect(),
            archived_session_compactions: self.archived_session_compactions.clone(),
            session_runtime_state: self
                .session_runtime_state
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .map(|(session_id, state)| (*session_id, state.clone()))
                .collect(),
            archived_session_runtime_state: self.archived_session_runtime_state.clone(),
            session_run_phases: self
                .session_run_phases
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .map(|(session_id, phase)| (*session_id, *phase))
                .collect(),
            expanded_task_projects: self.expanded_task_projects.clone(),
            selected_session,
            next_session_id: self.next_session_id,
        }
    }

    /// Rebuilds all wrapped hunk caches under the current column estimate and refreshes visible buffers.
    /// 在当前列宽估算下重建全部 hunk 折行缓存，并刷新可见缓冲区。
    fn rebuild_diff_code_buffers_for_current_wrap(&mut self) {
        self.diff_wrapped_hunks =
            rebuild_wrapped_diff_hunks(&self.diff_panel, self.current_diff_wrap_columns());
        self.refresh_visible_diff_buffers();
    }

    /// Refreshes only the diff editor buffers currently needed by the visible virtualized viewport.
    /// 仅刷新当前虚拟化视口真正需要的 Diff 编辑器缓冲区。
    fn refresh_visible_diff_buffers(&mut self) {
        let visible_file_range = self.visible_diff_file_range();
        let mut next_buffers = BTreeMap::new();
        let mut next_slices = BTreeMap::new();

        for file_index in visible_file_range.start..visible_file_range.end {
            let Some(file) = self.diff_panel.files.get(file_index) else {
                continue;
            };

            if self.collapsed_diff_files.contains(&file.key()) {
                continue;
            }

            let file_top_offset = self.diff_file_top_offset(file_index);
            let mut hunk_top_offset =
                file_top_offset + DIFF_FILE_HEADER_HEIGHT + DIFF_FILE_BODY_VERTICAL_INSET;
            let file_key = file.key();

            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                if hunk_index > 0 {
                    hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                    hunk_top_offset += DIFF_GAP_BLOCK_HEIGHT;
                    hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                }

                let wrapped_key = diff_hunk_buffer_key(&file_key, hunk_index);
                let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&wrapped_key) else {
                    hunk_top_offset += self.diff_hunk_block_height(&file_key, hunk_index, hunk);
                    continue;
                };

                let visible_slice =
                    self.visible_diff_hunk_slice(hunk_top_offset, wrapped_hunk.visual_lines.len());

                if visible_slice.range.len() > 0 {
                    next_slices.insert(wrapped_key.clone(), visible_slice.range);
                    next_buffers.insert(
                        wrapped_key.clone(),
                        build_diff_slice_buffer(wrapped_hunk, visible_slice.range),
                    );
                }

                hunk_top_offset += wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT;
            }
        }

        self.diff_code_buffers = next_buffers;
        self.diff_buffer_slices = next_slices;
    }

    /// Estimates the wrapped code column budget used by diff hunk editors in the current layout.
    /// 估算当前布局下 diff hunk 编辑器使用的代码折行列预算。
    fn current_diff_wrap_columns(&self) -> usize {
        let pane_width = if self.diff_window_id.is_some() {
            self.diff_window_size.width - WINDOW_FRAME_HORIZONTAL_PADDING
        } else {
            let workbench_ratio = (1.0 - 0.22) * 0.68;
            (self.main_window_size.width - WINDOW_FRAME_HORIZONTAL_PADDING) * workbench_ratio
        }
        .max(320.0);

        let available_code_width = pane_width
            - DIFF_FILE_CODE_HORIZONTAL_PADDING
            - diff_gutter_block_width()
            - (DIFF_CODE_EDITOR_HORIZONTAL_PADDING as f32 * 2.0)
            - DIFF_WRAP_RESERVED_WIDTH;
        let estimated_glyph_width = (CODE_EDITOR_TEXT_SIZE * DIFF_WRAP_GLYPH_WIDTH_RATIO).max(1.0);

        ((available_code_width / estimated_glyph_width).floor() as usize).max(DIFF_MIN_WRAP_COLUMNS)
    }

    /// Returns the current diff viewport height, preferring the real scroll viewport and falling back to layout estimates.
    /// 返回当前 Diff 视口高度，优先使用真实滚动视口高度，不足时回退到布局估算。
    fn current_diff_viewport_height(&self) -> f32 {
        let dedicated_window_height = if self.diff_window_id.is_some() {
            (self.diff_window_size.height - 180.0).max(240.0)
        } else {
            0.0
        };
        let main_panel_height = (self.main_window_size.height * 0.42).max(220.0);

        self.diff_viewport_height
            .max(dedicated_window_height.max(main_panel_height))
    }

    /// Returns the overscanned vertical window used by diff virtualization.
    /// 返回 Diff 虚拟化使用的带预加载区域的纵向窗口。
    fn diff_virtual_window(&self) -> (f32, f32) {
        let viewport_height = self.current_diff_viewport_height();
        let start = (self.diff_scroll_offset - DIFF_FILE_VIRTUAL_OVERSCAN).max(0.0);
        let end = self.diff_scroll_offset + viewport_height + DIFF_FILE_VIRTUAL_OVERSCAN;

        (start, end)
    }

    /// Resolves the currently visible file-card range for virtualized diff rendering.
    /// 解析当前虚拟化 Diff 渲染需要展示的文件卡片范围。
    fn visible_diff_file_range(&self) -> VisibleDiffFileRange {
        if self.diff_panel.files.is_empty() {
            return VisibleDiffFileRange {
                start: 0,
                end: 0,
                top_spacer_height: 0.0,
                bottom_spacer_height: 0.0,
            };
        }

        let (window_top, window_bottom) = self.diff_virtual_window();
        let mut running_offset = 0.0;
        let mut first_visible = None;
        let mut last_visible = None;
        let mut last_file_bottom = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            let file_top = running_offset;
            let file_bottom = file_top + self.diff_file_card_height(file);

            if file_bottom >= window_top && file_top <= window_bottom {
                first_visible.get_or_insert(index);
                last_visible = Some(index);
            }

            last_file_bottom = file_bottom;
            running_offset = file_bottom;

            if index + 1 < self.diff_panel.files.len() {
                running_offset += DIFF_FILE_CARD_SPACING;
            }
        }

        let (start, end) = match (first_visible, last_visible) {
            (Some(start), Some(end)) => (start, end + 1),
            _ => {
                let fallback_index = self.diff_panel.files.len().saturating_sub(1);
                (fallback_index, fallback_index + 1)
            }
        };
        let top_spacer_height = self.diff_file_top_offset(start);
        let bottom_spacer_height = (last_file_bottom
            - (self.diff_file_top_offset(end - 1)
                + self.diff_file_card_height(&self.diff_panel.files[end - 1])))
        .max(0.0);

        VisibleDiffFileRange {
            start,
            end,
            top_spacer_height,
            bottom_spacer_height,
        }
    }

    /// Resolves the visible virtualized slice for one wrapped hunk block under the current viewport.
    /// 解析当前视口下单个已折行 hunk 区块的可见虚拟化切片。
    fn visible_diff_hunk_slice(
        &self,
        hunk_top_offset: f32,
        total_visual_lines: usize,
    ) -> VisibleDiffHunkSlice {
        let total_height = total_visual_lines as f32 * CODE_EDITOR_LINE_HEIGHT;

        if total_visual_lines == 0 {
            return VisibleDiffHunkSlice {
                range: DiffVisualSliceRange::new(0, 0, 0),
                top_hidden_height: 0.0,
                bottom_hidden_height: 0.0,
            };
        }

        let (window_top, window_bottom) = self.diff_virtual_window();
        let hunk_bottom_offset = hunk_top_offset + total_height;

        if hunk_bottom_offset < window_top || hunk_top_offset > window_bottom {
            return VisibleDiffHunkSlice {
                range: DiffVisualSliceRange::new(0, 0, total_visual_lines),
                top_hidden_height: 0.0,
                bottom_hidden_height: total_height,
            };
        }

        let visible_top = (window_top - hunk_top_offset).max(0.0);
        let visible_bottom = (window_bottom - hunk_top_offset).min(total_height).max(0.0);
        let start_line = ((visible_top / CODE_EDITOR_LINE_HEIGHT).floor() as usize)
            .saturating_sub(DIFF_HUNK_VIRTUAL_OVERSCAN_LINES);
        let end_line = ((visible_bottom / CODE_EDITOR_LINE_HEIGHT).ceil() as usize)
            .saturating_add(DIFF_HUNK_VIRTUAL_OVERSCAN_LINES)
            .min(total_visual_lines);
        let range = DiffVisualSliceRange::new(start_line, end_line, total_visual_lines);
        let top_hidden_height = range.start as f32 * CODE_EDITOR_LINE_HEIGHT;
        let bottom_hidden_height =
            (total_visual_lines.saturating_sub(range.end)) as f32 * CODE_EDITOR_LINE_HEIGHT;

        VisibleDiffHunkSlice {
            range,
            top_hidden_height,
            bottom_hidden_height,
        }
    }

    /// Toggles one diff file between collapsed and expanded state.
    /// 在折叠与展开状态之间切换单个 diff 文件。
    fn toggle_diff_file_collapsed(&mut self, file_key: &str) {
        if self.collapsed_diff_files.contains(file_key) {
            self.collapsed_diff_files.remove(file_key);
        } else {
            self.collapsed_diff_files.insert(file_key.to_owned());
        }

        self.hovered_diff_line = None;
        self.refresh_visible_diff_buffers();
    }

    /// Resolves the currently active diff file from the review scroll offset.
    /// 根据审查滚动偏移量解析当前激活的 diff 文件。
    fn active_diff_file(&self) -> Option<ActiveDiffFile<'_>> {
        let mut running_offset = 0.0;
        let target_offset = self.diff_scroll_offset.max(0.0) + 1.0;
        let mut last_candidate = None;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index > 0 {
                running_offset += DIFF_FILE_CARD_SPACING;
            }

            let file_top_offset = running_offset;
            let file_height = self.diff_file_card_height(file);
            let file_key = file.key();
            let is_collapsed = self.collapsed_diff_files.contains(&file_key);

            let candidate = ActiveDiffFile { file, is_collapsed };
            last_candidate = Some(candidate);

            if target_offset < file_top_offset + file_height {
                return Some(candidate);
            }

            running_offset += file_height;
        }

        last_candidate
    }

    /// Resolves the vertical scroll offset pointing at the top edge of one diff file card.
    /// 解析指向单个 diff 文件卡片顶部边缘的纵向滚动偏移量。
    fn diff_file_scroll_offset(&self, target_file_key: &str) -> Option<f32> {
        let mut running_offset = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index > 0 {
                running_offset += DIFF_FILE_CARD_SPACING;
            }

            if file.key() == target_file_key {
                return Some(running_offset);
            }

            running_offset += self.diff_file_card_height(file);
        }

        None
    }

    /// Resolves the top edge offset of one diff file card inside the virtualized review flow.
    /// 解析虚拟化审查流中单个 Diff 文件卡片的顶部偏移。
    fn diff_file_top_offset(&self, target_index: usize) -> f32 {
        let mut running_offset = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index == target_index {
                return running_offset;
            }

            running_offset += self.diff_file_card_height(file);

            if index + 1 < self.diff_panel.files.len() {
                running_offset += DIFF_FILE_CARD_SPACING;
            }
        }

        running_offset
    }

    /// Estimates the rendered height of one diff file card in the current layout.
    /// 估算当前布局下单个 diff 文件卡片的渲染高度。
    fn diff_file_card_height(&self, file: &DiffFile) -> f32 {
        let is_collapsed = self.collapsed_diff_files.contains(&file.key());

        DIFF_FILE_HEADER_HEIGHT
            + if is_collapsed {
                0.0
            } else {
                self.diff_file_body_height(file)
            }
    }

    /// Estimates the rendered body height of one expanded diff file card.
    /// 估算单个展开 diff 文件卡片主体的渲染高度。
    fn diff_file_body_height(&self, file: &DiffFile) -> f32 {
        if file.hunks.is_empty() {
            return DIFF_EMPTY_BODY_HEIGHT;
        }

        let file_key = file.key();
        let hunk_count = file.hunks.len();
        let gap_count = hunk_count.saturating_sub(1);
        let block_count = hunk_count + gap_count;
        let hunk_heights = file
            .hunks
            .iter()
            .enumerate()
            .map(|(hunk_index, hunk)| self.diff_hunk_block_height(&file_key, hunk_index, hunk))
            .sum::<f32>();
        let gap_heights = gap_count as f32 * DIFF_GAP_BLOCK_HEIGHT;
        let inter_block_spacing = block_count.saturating_sub(1) as f32 * DIFF_HUNK_FLOW_SPACING;

        DIFF_FILE_BODY_VERTICAL_PADDING + hunk_heights + gap_heights + inter_block_spacing
    }

    /// Estimates the rendered height of one hunk block after wrapping under the current column budget.
    /// 估算当前列预算下单个 hunk 区块折行后的渲染高度。
    fn diff_hunk_block_height(
        &self,
        file_key: &str,
        hunk_index: usize,
        hunk: &crate::diff::DiffHunk,
    ) -> f32 {
        self.diff_wrapped_hunks
            .get(&diff_hunk_buffer_key(file_key, hunk_index))
            .map(|wrapped_hunk| wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT)
            .unwrap_or_else(|| {
                code_view::wrap_diff_hunk(hunk, self.current_diff_wrap_columns())
                    .visual_lines
                    .len() as f32
                    * CODE_EDITOR_LINE_HEIGHT
            })
    }

    /// Pushes a new activity entry while keeping the most recent items first.
    /// 追加新的活动日志项，并保持最新记录位于最前面。
    fn push_activity(
        &mut self,
        level: ActivityLevel,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        // Insert at the front so the latest orchestration feedback remains visible immediately.
        // 采用头插方式，让最新的编排反馈可以被立即看到。
        self.activities
            .insert(0, ActivityEntry::new(level, title, detail));

        // Trim the log to a manageable size so the prototype stays readable.
        // 将日志修剪到可管理范围内，保证原型界面持续可读。
        if self.activities.len() > 8 {
            self.activities.truncate(8);
        }

        self.rebuild_activity_raw_buffers();
    }

    /// Rebuilds all selectable activity-detail buffers from the current inspector log entries.
    /// 根据当前检查面板日志项重建全部可选活动详情缓冲区。
    fn rebuild_activity_raw_buffers(&mut self) {
        self.activity_raw_buffers = self
            .activities
            .iter()
            .map(|entry| text_editor::Content::with_text(&entry.detail))
            .collect();
    }

    /// Appends one transcript message to the target session while preserving stable insertion order.
    /// 向目标会话追加一条转录消息，并保持稳定插入顺序。
    fn push_session_message(
        &mut self,
        session_id: usize,
        kind: SessionMessageKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> String {
        self.append_session_message_entry(session_id, SessionMessageEntry::new(kind, title, body))
    }

    /// Appends one hidden internal message node to the target session while preserving stable insertion order.
    /// 向目标会话追加一条隐藏内部消息节点，并保持稳定插入顺序。
    fn push_hidden_session_message(
        &mut self,
        session_id: usize,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> String {
        self.append_session_message_entry(
            session_id,
            SessionMessageEntry::hidden(kind, record_kind, title, body),
        )
    }

    /// Appends one visible transcript message with an explicit durable record kind and optional default-collapsed state.
    /// 追加一条带显式持久化记录类型且可选默认折叠状态的可见转录消息。
    fn push_visible_session_record_message(
        &mut self,
        session_id: usize,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
        collapsed_by_default: bool,
    ) -> String {
        let title = title.into();
        let body = body.into();
        let message_id = self.append_session_message_entry(
            session_id,
            SessionMessageEntry {
                id: String::new(),
                kind,
                record_kind,
                visible: true,
                title,
                body,
                created_at: String::new(),
            },
        );

        if collapsed_by_default {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state
                .ui_state
                .collapsed_message_ids
                .insert(message_id.clone());
            runtime_state.updated_at = current_unix_millis_text();
        }

        message_id
    }

    /// Materializes one session message entry with stable runtime metadata and appends it to the transcript map.
    /// 使用稳定运行时元数据物化一条会话消息项并将其追加到转录映射中。
    fn append_session_message_entry(
        &mut self,
        session_id: usize,
        entry: SessionMessageEntry,
    ) -> String {
        let session_message_count = self
            .session_messages
            .get(&session_id)
            .map(|messages| messages.len())
            .unwrap_or(0);
        let message_id = format!(
            "session-{session_id}-message-{}-{session_message_count}",
            current_unix_millis_text()
        );
        let created_at = current_unix_millis_text();
        let materialized_entry = entry.with_runtime_metadata(message_id.clone(), created_at);
        self.session_message_raw_buffers.insert(
            message_id.clone(),
            text_editor::Content::with_text(&materialized_entry.body),
        );
        let (markdown_content, markdown_buffer) =
            parse_selectable_markdown_body(&materialized_entry.body, &self.app_theme);
        self.session_message_markdown
            .insert(message_id.clone(), markdown_content);
        self.session_message_markdown_buffers
            .insert(message_id.clone(), markdown_buffer);
        self.session_messages
            .entry(session_id)
            .or_default()
            .push(materialized_entry);
        self.refresh_session_agent_view_buffers(session_id);
        message_id
    }

    /// Returns the mutable transcript message entry for one session/message pair when it still exists in the visible transcript.
    /// 返回某个会话/消息对在可见转录中仍存在时对应的可变消息项。
    fn session_message_entry_mut(
        &mut self,
        session_id: usize,
        message_id: &str,
    ) -> Option<&mut SessionMessageEntry> {
        self.session_messages
            .get_mut(&session_id)?
            .iter_mut()
            .find(|message| message.id == message_id)
    }

    /// Appends one streamed body delta into an existing visible transcript message and refreshes its raw/Markdown caches.
    /// 将一段流式正文增量追加到现有可见转录消息，并刷新其原文与 Markdown 缓存。
    fn append_stream_delta_to_session_message(
        &mut self,
        session_id: usize,
        message_id: &str,
        delta: &str,
    ) {
        let body = {
            let Some(message) = self.session_message_entry_mut(session_id, message_id) else {
                return;
            };
            message.body.push_str(delta);
            message.body.clone()
        };

        self.session_message_raw_buffers.insert(
            message_id.to_string(),
            text_editor::Content::with_text(&body),
        );
        let (markdown_content, markdown_buffer) =
            parse_selectable_markdown_body(&body, &self.app_theme);
        self.session_message_markdown
            .insert(message_id.to_string(), markdown_content);
        self.session_message_markdown_buffers
            .insert(message_id.to_string(), markdown_buffer);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Ensures the session owns one visible thinking message and appends the incoming delta to it.
    /// 确保会话拥有一条可见思考消息，并将当前增量追加进去。
    fn append_streaming_thinking_delta(&mut self, session_id: usize, delta: &str) {
        let message_id =
            if let Some(message_id) = self.streaming_thinking_message_ids.get(&session_id) {
                message_id.clone()
            } else {
                let message_id = self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ThinkingOutput,
                    "思考",
                    String::new(),
                    true,
                );
                self.streaming_thinking_message_ids
                    .insert(session_id, message_id.clone());
                message_id
            };

        self.floating_streaming_thinking_sessions.insert(session_id);
        self.session_streaming_thinking_follow_tail
            .entry(session_id)
            .or_insert(true);
        self.append_stream_delta_to_session_message(session_id, &message_id, delta);
    }

    /// Docks the current streaming thinking message back into the main transcript while keeping subsequent deltas on the same message.
    /// 将当前流式思考消息收回主转录正文中，同时保留后续增量继续写入同一条消息。
    fn dock_streaming_thinking_message(&mut self, session_id: usize) {
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Ensures the session owns one visible final-output message and appends the incoming delta to it.
    /// 确保会话拥有一条可见正式输出消息，并将当前增量追加进去。
    fn append_streaming_output_delta(&mut self, session_id: usize, delta: &str) {
        let message_id =
            if let Some(message_id) = self.streaming_output_message_ids.get(&session_id) {
                message_id.clone()
            } else {
                let message_id = self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::AssistantOutput,
                    "输出",
                    String::new(),
                    false,
                );
                self.streaming_output_message_ids
                    .insert(session_id, message_id.clone());
                message_id
            };

        self.append_stream_delta_to_session_message(session_id, &message_id, delta);
    }

    /// Appends one hidden derived session record produced by the dedicated session skill chain.
    /// 追加一条由独立会话技能链产出的隐藏派生会话记录。
    fn push_session_derived_record(
        &mut self,
        session_id: usize,
        record: SessionDerivedRecord,
    ) -> String {
        self.push_hidden_session_message(
            session_id,
            record.kind,
            record.record_kind,
            record.title,
            record.body,
        )
    }

    /// Returns the mutable runtime-state snapshot for the target session, creating one default row on demand.
    /// 返回目标会话对应的可变运行时状态快照，并在缺失时按需创建默认行。
    fn ensure_session_runtime_state_mut(
        &mut self,
        session_id: usize,
    ) -> &mut SessionRuntimeStateEntry {
        self.session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            })
    }

    /// Writes one hidden explore output for the target session so project-scoped context survives persistence.
    /// 为目标会话写入一条隐藏探索输出，从而让项目级上下文在持久化后仍可恢复。
    fn append_session_explore_output(&mut self, session_id: usize, project_id: Option<&str>) {
        let derived_record = {
            let project = project_id
                .and_then(|resolved_project_id| self.task_project_by_id(resolved_project_id));
            self.session_skill_chain
                .run_explore_skill(ExploreSkillInput {
                    workspace_path: &self.workspace_path,
                    project_id,
                    project,
                })
        };
        self.push_session_derived_record(session_id, derived_record);
    }

    /// Generates a new session title from the latest user prompt when the session title is still auto-managed.
    /// 当会话标题仍处于自动管理状态时，根据最新用户提示词生成新的会话标题。
    fn maybe_generate_session_title_from_prompt(
        &mut self,
        session_id: usize,
        prompt: &str,
    ) -> Option<String> {
        let (current_title, current_title_source) = self
            .session_by_id(session_id)
            .map(|session| (session.title.clone(), session.title_source))
            .unwrap_or_else(|| (String::new(), SessionTitleSource::Generated));
        let result = self.session_skill_chain.run_title_agent(TitleAgentInput {
            current_title: &current_title,
            title_source: current_title_source,
            prompt,
        })?;

        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = result.generated_title.clone();
            session.title_source = SessionTitleSource::Generated;
            session.last_updated_label = String::from("刚刚");
        }
        self.push_session_derived_record(session_id, result.hidden_record);

        Some(result.generated_title)
    }

    /// Appends one hidden summary output after a real execution run completes.
    /// 在一次真实执行完成后追加一条隐藏摘要输出。
    fn append_session_summary_output(&mut self, session_id: usize, report: &ExecutionRunReport) {
        let derived_record = self
            .session_skill_chain
            .run_summary_skill(SummarySkillInput { report });
        self.push_session_derived_record(session_id, derived_record);
    }

    /// Updates the runtime-state snapshot for the session from the latest execution preparation.
    /// 根据最近一次执行准备结果更新该会话的运行时状态快照。
    fn update_session_runtime_identity_from_preparation(
        &mut self,
        session_id: usize,
        preparation: &ExecutionPreparation,
    ) {
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.active_agent_id = Some(preparation.agent_id.clone());
        runtime_state.active_provider_id = Some(preparation.model_blueprint.provider_id.clone());
        runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
        runtime_state.updated_at = current_unix_millis_text();
    }

    /// Finalizes one streamed execution report, updates visible session state, and appends hidden derived nodes.
    /// 完成一份流式执行报告，更新可见会话状态并追加隐藏派生节点。
    fn apply_execution_run_report(&mut self, report: ExecutionRunReport) -> Task<Message> {
        let session_id = report.session_id;
        self.last_mcp_discovery_rows = report.mcp_discovery_rows.clone();
        self.last_mcp_discovery_errors = report.mcp_discovery_errors.clone();

        if let Some(preparation) = report.preparation.clone() {
            self.context_usage = (self.context_usage + 8.0).min(100.0);
            self.last_execution_preparation = Some(preparation.clone());
            self.update_session_runtime_identity_from_preparation(session_id, &preparation);

            for warning in &preparation.warnings {
                self.push_activity(ActivityLevel::Warning, "执行准备警告", warning.clone());
            }
        }

        for warning in &report.warnings {
            self.push_activity(ActivityLevel::Warning, "执行运行警告", warning.clone());
        }

        if report.is_success() {
            let thinking_output = report
                .thinking_output
                .clone()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let assistant_output = report
                .assistant_output
                .clone()
                .unwrap_or_else(|| String::from("模型已完成执行，但未返回可见文本输出。"));

            if thinking_output.is_some()
                && !self
                    .streaming_thinking_message_ids
                    .contains_key(&session_id)
            {
                self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ThinkingOutput,
                    "思考",
                    thinking_output.unwrap_or_default(),
                    true,
                );
            }

            if !self.streaming_output_message_ids.contains_key(&session_id) {
                self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::AssistantOutput,
                    "输出",
                    assistant_output,
                    false,
                );
            }

            if let Some(session) = self
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Completed;
                session.focus = String::from("最近一次真实执行已完成，可继续追问或调整参数。");
                session.queued_steps = session.queued_steps.saturating_add(1);
                session.last_updated_label = String::from("刚刚");
            }

            self.set_session_run_phase(session_id, SessionRunPhase::Ready);
            self.status_message = String::from("真实执行已完成，流式结果已写入工作台输出区。");
            self.push_activity(
                ActivityLevel::Success,
                "真实执行已完成",
                format!("会话 #{session_id} 已完成真实模型执行并产出分离后的思考与输出。"),
            );
        } else {
            let error = report
                .error
                .clone()
                .unwrap_or_else(|| String::from("真实执行失败，但未返回具体错误信息。"));

            if let Some(session) = self
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Blocked;
                session.last_updated_label = String::from("刚刚");
            }

            self.push_session_message(
                session_id,
                SessionMessageKind::Error,
                "执行失败",
                error.clone(),
            );
            self.set_session_run_phase(session_id, SessionRunPhase::Failed);
            self.status_message = error.clone();
            self.push_activity(ActivityLevel::Warning, "真实执行失败", error);
        }

        self.append_session_summary_output(session_id, &report);
        self.maybe_compact_session_history(session_id);
        self.clear_streaming_message_ids(session_id);
        self.refresh_session_agent_view_buffers(session_id);

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
        ])
    }

    /// Creates one hidden compaction summary when the visible history grows beyond the current threshold.
    /// 当可见历史增长到当前阈值以上时创建一条隐藏压缩摘要。
    fn maybe_compact_session_history(&mut self, session_id: usize) {
        let Some(messages) = self.session_messages.get(&session_id).cloned() else {
            return;
        };
        let runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let Some(compaction_result) =
            self.session_skill_chain
                .run_compaction_skill(CompactionSkillInput {
                    session_id,
                    messages: &messages,
                    runtime_state: &runtime_state_snapshot,
                })
        else {
            return;
        };
        let summary_message_id =
            self.push_session_derived_record(session_id, compaction_result.summary_record.clone());
        let checkpoint = compaction_result.materialize_checkpoint(summary_message_id);
        let created_at = checkpoint.created_at.clone();
        let compaction_id = checkpoint.id.clone();
        let context_start_message_id = checkpoint.context_start_message_id.clone();
        self.session_compactions
            .entry(session_id)
            .or_default()
            .push(checkpoint);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.last_compaction_id = Some(compaction_id);
        runtime_state.context_start_message_id = context_start_message_id;
        runtime_state.updated_at = created_at;
    }

    /// Builds the current model-facing context history for one session from the start or from the latest compaction anchor.
    /// 从起点或最近一次压缩锚点开始为单个会话构建当前面向模型的上下文历史。
    fn session_context_history(&self, session_id: usize) -> Vec<SessionMessageEntry> {
        let Some(messages) = self.session_messages.get(&session_id) else {
            return Vec::new();
        };
        let start_index = self
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.context_start_message_id.as_deref())
            .and_then(|context_start_message_id| {
                messages
                    .iter()
                    .position(|message| message.id == context_start_message_id)
            })
            .unwrap_or(0);

        messages[start_index..]
            .iter()
            .filter(|message| message.contributes_to_context())
            .cloned()
            .collect()
    }

    /// Applies one persisted workspace snapshot to the live GUI state in a single place.
    /// 将一份持久化工作台快照集中应用到当前图形界面状态。
    fn apply_workspace_snapshot(&mut self, snapshot: WorkspaceSnapshot) {
        self.task_projects = snapshot.task_projects;
        self.archived_task_projects = snapshot.archived_task_projects;
        self.sessions = snapshot.sessions;
        self.archived_sessions = snapshot.archived_sessions;
        self.session_messages = snapshot.session_messages;
        self.archived_session_messages = snapshot.archived_session_messages;
        self.session_compactions = snapshot.session_compactions;
        self.archived_session_compactions = snapshot.archived_session_compactions;
        self.session_runtime_state = snapshot.session_runtime_state;
        self.archived_session_runtime_state = snapshot.archived_session_runtime_state;
        self.session_run_phases = snapshot.session_run_phases;
        self.expanded_task_projects = snapshot.expanded_task_projects;
        self.selected_session = snapshot.selected_session;
        self.next_session_id = snapshot.next_session_id;
        self.hovered_task_project = None;
        self.hovered_task_session = None;
        self.active_task_project_drop_target = None;
        self.pending_task_project_interaction = None;
        self.dragging_task_project = None;
        self.suppress_task_session_hover = false;
        self.session_runtime_events.clear();
        self.collapsed_runtime_event_ids.clear();
        self.markdown_session_message_ids.clear();
        self.markdown_runtime_event_ids.clear();
        self.streaming_thinking_message_ids.clear();
        self.floating_streaming_thinking_sessions.clear();
        self.session_streaming_thinking_follow_tail.clear();
        self.active_thinking_modal = None;
        self.pending_thinking_inline_toggle = None;
        self.streaming_output_message_ids.clear();
        self.rebuild_session_message_raw_buffers();
        self.rebuild_session_message_markdown();
        self.runtime_event_raw_buffers.clear();
        self.runtime_event_markdown.clear();
        self.runtime_event_markdown_buffers.clear();
        self.rebuild_all_session_agent_view_buffers();
    }

    /// Rebuilds all read-only transcript source buffers from the current visible and archived session messages.
    /// 根据当前可见与归档会话消息重建全部只读转录原文缓冲区。
    fn rebuild_session_message_raw_buffers(&mut self) {
        self.session_message_raw_buffers.clear();

        for messages in self
            .session_messages
            .values()
            .chain(self.archived_session_messages.values())
        {
            for message in messages {
                self.session_message_raw_buffers.insert(
                    message.id.clone(),
                    text_editor::Content::with_text(&message.body),
                );
            }
        }
    }

    /// Rebuilds all parsed transcript Markdown payloads from the current visible and archived session messages.
    /// 根据当前可见与归档会话消息重建全部已解析转录 Markdown 载荷。
    fn rebuild_session_message_markdown(&mut self) {
        self.session_message_markdown.clear();
        self.session_message_markdown_buffers.clear();
        let app_theme = self.app_theme.clone();

        for messages in self
            .session_messages
            .values()
            .chain(self.archived_session_messages.values())
        {
            for message in messages {
                let (markdown_content, markdown_buffer) =
                    parse_selectable_markdown_body(&message.body, &app_theme);
                self.session_message_markdown
                    .insert(message.id.clone(), markdown_content);
                self.session_message_markdown_buffers
                    .insert(message.id.clone(), markdown_buffer);
            }
        }
    }

    /// Rebuilds all unified transcript buffers from the current visible session and runtime-event state.
    /// 根据当前可见会话与运行时事件状态重建全部统一转录缓冲区。
    fn rebuild_all_session_agent_view_buffers(&mut self) {
        self.session_agent_view_buffers.clear();
        self.session_agent_view_raw_buffers.clear();
        self.session_agent_view_selectable_documents.clear();
        self.session_agent_view_documents.clear();
        self.session_agent_view_block_collapsed.clear();
        self.session_agent_view_follow_tail.clear();
        self.session_agent_view_viewports.clear();
        self.session_streaming_thinking_follow_tail.clear();
        self.session_streaming_thinking_viewports.clear();
        self.active_transcript_drag_auto_scroll = None;
        self.active_thinking_overlay_drag_auto_scroll = None;
        self.active_thinking_modal = None;
        self.pending_thinking_inline_toggle = None;

        let mut known_session_ids = self
            .sessions
            .iter()
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();
        known_session_ids.extend(self.session_messages.keys().copied());
        known_session_ids.extend(self.session_runtime_events.keys().copied());

        for session_id in known_session_ids {
            self.refresh_session_agent_view_buffers(session_id);
        }
    }

    /// Refreshes the unified structured and raw transcript buffers for one session.
    /// 刷新某个会话对应的统一结构化与原始转录缓冲区。
    fn refresh_session_agent_view_buffers(&mut self, session_id: usize) {
        let messages = self.visible_session_messages_for_agent_view(session_id);
        let runtime_events = self
            .session_runtime_events
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let document = build_agent_view_document(&messages, &runtime_events);

        if document.blocks.is_empty() {
            self.session_agent_view_buffers.remove(&session_id);
            self.session_agent_view_raw_buffers.remove(&session_id);
            self.session_agent_view_selectable_documents
                .remove(&session_id);
            self.session_agent_view_documents.remove(&session_id);
            self.session_agent_view_block_collapsed.remove(&session_id);
            self.session_agent_view_follow_tail.remove(&session_id);
            return;
        }

        self.sync_session_agent_view_block_collapsed_state(session_id, &document);
        let selectable_document =
            vulcan_agent_view::AgentViewSelectableDocument::from_document(&document);
        let selectable_text = selectable_document.buffer().to_string();
        let raw_source_text = agent_view_document_raw_source(&document);

        self.session_agent_view_buffers.insert(
            session_id,
            text_editor::Content::with_text(&selectable_text),
        );
        self.session_agent_view_raw_buffers.insert(
            session_id,
            text_editor::Content::with_text(&raw_source_text),
        );
        self.session_agent_view_selectable_documents
            .insert(session_id, selectable_document);
        self.session_agent_view_documents
            .insert(session_id, document);
    }

    /// Synchronizes the unified block collapsed-state map with the latest document shape while preserving user toggles.
    /// 在保留用户切换结果的同时，将统一块折叠状态映射同步到最新文档结构。
    fn sync_session_agent_view_block_collapsed_state(
        &mut self,
        session_id: usize,
        document: &vulcan_agent_view::AgentViewDocument,
    ) {
        self.session_agent_view_follow_tail
            .entry(session_id)
            .or_insert(true);
        let collapsed_entry = self
            .session_agent_view_block_collapsed
            .entry(session_id)
            .or_default();
        let mut active_ids = BTreeSet::new();

        for (block_index, block) in document.blocks.iter().enumerate() {
            let block_id = agent_view_block_id(session_id, block_index, block);
            collapsed_entry
                .entry(block_id.clone())
                .or_insert(agent_view_block_collapsed_by_default(block));
            active_ids.insert(block_id);
        }

        collapsed_entry.retain(|block_id, _| active_ids.contains(block_id));
    }

    /// Removes all read-only runtime-event source buffers associated with one session.
    /// 删除与某个会话关联的全部只读运行时事件原文缓冲区。
    fn remove_runtime_event_raw_buffers(&mut self, session_id: usize) {
        let prefix = format!("session-{session_id}-runtime-event-");
        self.runtime_event_raw_buffers
            .retain(|event_id, _| !event_id.starts_with(&prefix));
        self.runtime_event_markdown
            .retain(|event_id, _| !event_id.starts_with(&prefix));
        self.runtime_event_markdown_buffers
            .retain(|event_id, _| !event_id.starts_with(&prefix));
    }

    /// Clears active streaming message identifiers associated with one session before or after a live run.
    /// 在一次实时运行前后清理与某个会话关联的当前流式消息标识。
    fn clear_streaming_message_ids(&mut self, session_id: usize) {
        self.clear_transcript_drag_auto_scroll_for_session(session_id);
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
        self.cancel_pending_thinking_inline_toggle(Some(session_id), None);
        self.streaming_thinking_message_ids.remove(&session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.session_streaming_thinking_viewports
            .remove(&session_id);
        self.close_thinking_modal_for_session(session_id);
        self.streaming_output_message_ids.remove(&session_id);
    }

    /// Clears transient transcript view caches associated with one session identifier.
    /// 清理与某个会话标识关联的临时转录视图缓存。
    fn clear_session_transcript_view_state(&mut self, session_id: usize) {
        if let Some(messages) = self.session_messages.get(&session_id) {
            for message in messages {
                self.session_message_raw_buffers.remove(&message.id);
                self.session_message_markdown.remove(&message.id);
                self.session_message_markdown_buffers.remove(&message.id);
            }
        }
        self.markdown_session_message_ids.remove(&session_id);
        self.clear_streaming_message_ids(session_id);
        self.collapsed_runtime_event_ids.remove(&session_id);
        self.markdown_runtime_event_ids.remove(&session_id);
        self.remove_runtime_event_raw_buffers(session_id);
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_agent_view_documents.remove(&session_id);
        self.session_agent_view_block_collapsed.remove(&session_id);
        self.session_agent_view_follow_tail.remove(&session_id);
        self.session_agent_view_viewports.remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.session_streaming_thinking_viewports
            .remove(&session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.close_thinking_modal_for_session(session_id);
    }

    /// Builds one asynchronous persistence task for the current visible workspace snapshot.
    /// 为当前可见工作台快照构建一次异步持久化任务。
    fn persist_workspace_snapshot_task(&self) -> Task<Message> {
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };

        let snapshot = self.build_persistable_workspace_snapshot();

        Task::perform(
            async move {
                workspace_store::persist_workspace_snapshot(controller_runtime, &snapshot).await
            },
            Message::WorkspaceStatePersisted,
        )
    }

    /// Replaces the runtime-event list for one session with a fresh snapshot.
    /// 使用新的快照替换某个会话的运行时事件列表。
    fn replace_session_runtime_events(&mut self, session_id: usize, events: Vec<RuntimeEvent>) {
        self.collapsed_runtime_event_ids.remove(&session_id);
        self.markdown_runtime_event_ids.remove(&session_id);
        self.remove_runtime_event_raw_buffers(session_id);
        let app_theme = self.app_theme.clone();
        for (index, event) in events.iter().enumerate() {
            let event_id = session_runtime_event_id(session_id, index, event);
            self.runtime_event_raw_buffers.insert(
                event_id.clone(),
                text_editor::Content::with_text(&event.detail),
            );
            let (markdown_content, markdown_buffer) =
                parse_selectable_markdown_body(&event.detail, &app_theme);
            self.runtime_event_markdown
                .insert(event_id.clone(), markdown_content);
            self.runtime_event_markdown_buffers
                .insert(event_id, markdown_buffer);
        }
        self.session_runtime_events.insert(session_id, events);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Appends one runtime event to the current session event stream and refreshes the corresponding raw/Markdown caches.
    /// 向当前会话事件流追加一条运行时事件，并刷新对应的原文与 Markdown 缓存。
    fn push_session_runtime_event(&mut self, session_id: usize, event: RuntimeEvent) {
        let event_index = self
            .session_runtime_events
            .get(&session_id)
            .map(|events| events.len())
            .unwrap_or(0);
        let event_id = session_runtime_event_id(session_id, event_index, &event);
        self.runtime_event_raw_buffers.insert(
            event_id.clone(),
            text_editor::Content::with_text(&event.detail),
        );
        let (markdown_content, markdown_buffer) =
            parse_selectable_markdown_body(&event.detail, &self.app_theme);
        self.runtime_event_markdown
            .insert(event_id.clone(), markdown_content);
        self.runtime_event_markdown_buffers
            .insert(event_id, markdown_buffer);
        self.session_runtime_events
            .entry(session_id)
            .or_default()
            .push(event);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Returns whether one unified rendered block is currently collapsed for the target session.
    /// 返回目标会话中的某个统一渲染区块当前是否处于折叠状态。
    fn session_agent_view_block_is_collapsed(&self, session_id: usize, block_id: &str) -> bool {
        self.session_agent_view_block_collapsed
            .get(&session_id)
            .and_then(|entry| entry.get(block_id))
            .copied()
            .unwrap_or(false)
    }

    /// Toggles one unified rendered block between collapsed and expanded states for the target session.
    /// 为目标会话切换某个统一渲染区块的折叠与展开状态。
    fn toggle_session_agent_view_block_collapsed(&mut self, session_id: usize, block_id: &str) {
        let entry = self
            .session_agent_view_block_collapsed
            .entry(session_id)
            .or_default();
        let current_value = entry.get(block_id).copied().unwrap_or(false);
        entry.insert(block_id.to_string(), !current_value);
    }

    /// Updates the execution phase tracked for one session transcript.
    /// 更新某个会话转录区跟踪的执行阶段。
    fn set_session_run_phase(&mut self, session_id: usize, phase: SessionRunPhase) {
        self.session_run_phases.insert(session_id, phase);
    }

    /// Returns the visible session messages that should participate in the unified agent-view document for one session.
    /// 返回某个会话中应参与统一 agent-view 文档构建的可见消息集合。
    fn visible_session_messages_for_agent_view(
        &self,
        session_id: usize,
    ) -> Vec<SessionMessageEntry> {
        let floating_thinking_message_id = if self.active_run_session_id == Some(session_id)
            && self
                .floating_streaming_thinking_sessions
                .contains(&session_id)
        {
            self.streaming_thinking_message_ids
                .get(&session_id)
                .cloned()
        } else {
            None
        };

        self.session_messages
            .get(&session_id)
            .map(|messages| {
                messages
                    .iter()
                    .filter(|message| {
                        message.is_visible_to_user()
                            && floating_thinking_message_id
                                .as_deref()
                                .is_none_or(|active_id| message.id != active_id)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the currently streaming thinking message for one session when it is still actively floating above the transcript.
    /// 返回某个会话当前仍以浮层方式展示的流式思考消息。
    fn streaming_thinking_message(&self, session_id: usize) -> Option<&SessionMessageEntry> {
        if self.active_run_session_id != Some(session_id) {
            return None;
        }
        if !self
            .floating_streaming_thinking_sessions
            .contains(&session_id)
        {
            return None;
        }

        let message_id = self.streaming_thinking_message_ids.get(&session_id)?;
        self.session_messages
            .get(&session_id)?
            .iter()
            .find(|message| message.id == *message_id && !message.body.trim().is_empty())
    }

    /// Builds the unified agent-view document for the currently selected session when enough transcript state is available.
    /// 当当前选中会话拥有足够转录状态时构建统一 agent-view 文档。
    fn selected_session_agent_view_document(&self) -> Option<vulcan_agent_view::AgentViewDocument> {
        let session_id = self.selected_session().map(|session| session.id)?;
        self.session_agent_view_documents
            .get(&session_id)
            .cloned()
            .or_else(|| {
                let messages = self.visible_session_messages_for_agent_view(session_id);
                let runtime_events = self
                    .session_runtime_events
                    .get(&session_id)
                    .cloned()
                    .unwrap_or_default();

                Some(build_agent_view_document(&messages, &runtime_events))
            })
    }

    /// Builds the runtime-decorated transcript document used by the primary rendered transcript surface.
    /// 构建主渲染转录表面使用的运行时增强文档。
    fn rendered_session_agent_view_document(
        &self,
        session_id: usize,
    ) -> Option<vulcan_agent_view::AgentViewDocument> {
        let mut document = self
            .session_agent_view_documents
            .get(&session_id)
            .cloned()
            .or_else(|| {
                let messages = self.visible_session_messages_for_agent_view(session_id);
                let runtime_events = self
                    .session_runtime_events
                    .get(&session_id)
                    .cloned()
                    .unwrap_or_default();

                Some(build_agent_view_document(&messages, &runtime_events))
            })?;

        for (block_index, block) in document.blocks.iter_mut().enumerate() {
            let block_id = agent_view_block_id(session_id, block_index, block);
            self.decorate_agent_view_block_for_render(session_id, &block_id, block);
        }

        Some(document)
    }

    /// Decorates one agent-view block with runtime-only rendering metadata such as collapse state and thinking metrics.
    /// 使用折叠状态与思考指标等仅运行时元数据装饰单个 agent-view 区块。
    fn decorate_agent_view_block_for_render(
        &self,
        session_id: usize,
        block_id: &str,
        block: &mut vulcan_agent_view::AgentViewBlock,
    ) {
        let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
            return;
        };

        directive
            .attributes
            .insert(String::from("ui-block-id"), block_id.to_string());

        if directive.kind == vulcan_agent_view::AgentViewDirectiveKind::Thinking {
            directive.attributes.insert(
                String::from("ui-inline-collapsed"),
                self.session_agent_view_block_is_collapsed(session_id, block_id)
                    .to_string(),
            );
            directive.attributes.insert(
                String::from("ui-render-thinking-header"),
                String::from("true"),
            );
            directive
                .attributes
                .insert(String::from("ui-thinking-surface"), String::from("inline"));
            directive.attributes.insert(
                String::from("ui-thinking-title"),
                self.session_thinking_title(session_id, directive),
            );
        }
    }

    /// Builds the inline thinking title shown in both the transcript header row and the explicit centered modal.
    /// 构建展示在转录标题行与显式居中弹层中的内联思考标题。
    fn session_thinking_title(
        &self,
        session_id: usize,
        directive: &vulcan_agent_view::AgentViewDirectiveBlock,
    ) -> String {
        let duration_label = directive
            .attributes
            .get("message-id")
            .and_then(|message_id| self.session_thinking_duration_millis(session_id, message_id))
            .map(format_thinking_duration_label)
            .unwrap_or_else(|| String::from("未知时长"));
        build_thinking_title(&directive.body, &duration_label)
    }

    /// Returns the best-effort thinking duration derived from the thinking message timestamp and the next visible message or runtime snapshot.
    /// 根据思考消息时间戳与下一条可见消息或运行时快照推导最佳努力思考时长。
    fn session_thinking_duration_millis(
        &self,
        session_id: usize,
        message_id: &str,
    ) -> Option<u128> {
        let messages = self.session_messages.get(&session_id)?;
        let message_index = messages
            .iter()
            .position(|message| message.id == message_id)?;
        let started_at = parse_unix_millis_text(&messages.get(message_index)?.created_at)?;

        let finished_at = messages
            .iter()
            .skip(message_index + 1)
            .find(|message| {
                message.is_visible_to_user()
                    && message.record_kind != SessionMessageRecordKind::ThinkingOutput
                    && message.kind != SessionMessageKind::User
                    && !message.created_at.trim().is_empty()
            })
            .and_then(|message| parse_unix_millis_text(&message.created_at))
            .or_else(|| {
                if self.active_run_session_id == Some(session_id)
                    && self
                        .streaming_thinking_message_ids
                        .get(&session_id)
                        .is_some_and(|active_id| active_id == message_id)
                    && !self.streaming_output_message_ids.contains_key(&session_id)
                {
                    Some(current_unix_millis())
                } else {
                    self.session_runtime_state
                        .get(&session_id)
                        .and_then(|runtime_state| parse_unix_millis_text(&runtime_state.updated_at))
                }
            })?;

        Some(finished_at.saturating_sub(started_at))
    }

    /// Returns the selectable transcript projection for the currently selected session when it is already cached.
    /// 当当前选中会话的可选转录投影已经缓存时返回该投影。
    fn selected_session_agent_view_selectable_document(
        &self,
    ) -> Option<&vulcan_agent_view::AgentViewSelectableDocument> {
        let session_id = self.selected_session().map(|session| session.id)?;
        self.session_agent_view_selectable_documents
            .get(&session_id)
    }

    /// Returns the execution phase bound to the currently selected session transcript.
    /// 返回绑定到当前选中会话转录区的执行阶段。
    fn selected_session_run_phase(&self) -> SessionRunPhase {
        self.selected_session()
            .and_then(|session| self.session_run_phases.get(&session.id).copied())
            .unwrap_or(SessionRunPhase::Idle)
    }

    /// Returns whether one session transcript should keep auto-following the latest tail output.
    /// 返回某个会话转录是否应继续自动追随最新尾部输出。
    fn session_agent_view_should_follow_tail(&self, session_id: usize) -> bool {
        self.session_agent_view_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped auto-follow flag from one reported transcript viewport.
    /// 根据一次回传的转录视口更新会话级自动追随标记。
    fn update_session_agent_view_follow_tail(
        &mut self,
        session_id: usize,
        viewport: scrollable::Viewport,
    ) {
        self.session_agent_view_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session transcript to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话转录吸附到尾部的任务。
    fn maybe_follow_selected_session_transcript_tail(&self, session_id: usize) -> Task<Message> {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return Task::none();
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(transcript_scrollable_id(session_id))
    }

    /// Updates the active drag auto-scroll state for the main rendered transcript surface.
    /// 更新主渲染转录表面的当前拖拽自动滚动状态。
    fn update_transcript_drag_auto_scroll_state(
        &mut self,
        session_id: usize,
        request: Option<SelectableTranscriptDragAutoScroll>,
    ) {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            self.clear_transcript_drag_auto_scroll_for_session(session_id);
            return;
        }

        self.active_transcript_drag_auto_scroll =
            request.map(|request| ActiveTranscriptDragAutoScroll {
                session_id,
                request,
            });
    }

    /// Updates the active drag auto-scroll state for the floating thinking overlay body.
    /// 更新流式思考浮层正文的当前拖拽自动滚动状态。
    fn update_thinking_overlay_drag_auto_scroll_state(
        &mut self,
        session_id: usize,
        request: Option<SelectableTranscriptDragAutoScroll>,
    ) {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            self.clear_thinking_overlay_drag_auto_scroll(session_id);
            return;
        }

        self.active_thinking_overlay_drag_auto_scroll =
            request.map(|request| ActiveTranscriptDragAutoScroll {
                session_id,
                request,
            });
    }

    /// Clears both transcript drag auto-scroll snapshots regardless of which surface currently owns them.
    /// 无论当前由哪个表面持有，统一清空两类转录拖拽自动滚动快照。
    fn clear_transcript_drag_auto_scrolls(&mut self) {
        self.active_transcript_drag_auto_scroll = None;
        self.active_thinking_overlay_drag_auto_scroll = None;
    }

    /// Clears the main transcript drag auto-scroll snapshot when it belongs to the provided session.
    /// 当主转录拖拽自动滚动快照属于给定会话时清空它。
    fn clear_transcript_drag_auto_scroll_for_session(&mut self, session_id: usize) {
        if self
            .active_transcript_drag_auto_scroll
            .as_ref()
            .is_some_and(|state| state.session_id == session_id)
        {
            self.active_transcript_drag_auto_scroll = None;
        }
    }

    /// Clears the floating thinking overlay drag auto-scroll snapshot when it belongs to the provided session.
    /// 当流式思考浮层拖拽自动滚动快照属于给定会话时清空它。
    fn clear_thinking_overlay_drag_auto_scroll(&mut self, session_id: usize) {
        if self
            .active_thinking_overlay_drag_auto_scroll
            .as_ref()
            .is_some_and(|state| state.session_id == session_id)
        {
            self.active_thinking_overlay_drag_auto_scroll = None;
        }
    }

    /// Performs one timed drag auto-scroll tick for the main rendered transcript surface.
    /// 为主渲染转录表面执行一次定时拖拽自动滚动 Tick。
    fn perform_transcript_drag_auto_scroll_tick(&mut self) -> Task<Message> {
        let Some(active_state) = self.active_transcript_drag_auto_scroll else {
            return Task::none();
        };

        if self.selected_session().map(|session| session.id) != Some(active_state.session_id) {
            self.clear_transcript_drag_auto_scroll_for_session(active_state.session_id);
            return Task::none();
        }

        let delta_y = drag_auto_scroll_step(active_state.request);

        if delta_y.abs() <= f32::EPSILON {
            self.clear_transcript_drag_auto_scroll_for_session(active_state.session_id);
            return Task::none();
        }

        operation::scroll_by(
            transcript_scrollable_id(active_state.session_id),
            scrollable::AbsoluteOffset { x: 0.0, y: delta_y },
        )
    }

    /// Performs one timed drag auto-scroll tick for the floating thinking overlay body.
    /// 为流式思考浮层正文执行一次定时拖拽自动滚动 Tick。
    fn perform_thinking_overlay_drag_auto_scroll_tick(&mut self) -> Task<Message> {
        let Some(active_state) = self.active_thinking_overlay_drag_auto_scroll else {
            return Task::none();
        };

        if self.selected_session().map(|session| session.id) != Some(active_state.session_id) {
            self.clear_thinking_overlay_drag_auto_scroll(active_state.session_id);
            return Task::none();
        }

        let delta_y = drag_auto_scroll_step(active_state.request);

        if delta_y.abs() <= f32::EPSILON {
            self.clear_thinking_overlay_drag_auto_scroll(active_state.session_id);
            return Task::none();
        }

        operation::scroll_by(
            thinking_overlay_scrollable_id(active_state.session_id),
            scrollable::AbsoluteOffset { x: 0.0, y: delta_y },
        )
    }

    /// Returns whether one floating streaming-thinking overlay should keep auto-following the latest tail.
    /// 返回某个流式思考浮层是否应继续自动追随最新尾部。
    fn session_streaming_thinking_should_follow_tail(&self, session_id: usize) -> bool {
        self.session_streaming_thinking_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped floating-thinking auto-follow flag from one reported overlay viewport.
    /// 根据一次回传的浮层视口更新会话级流式思考自动追随标记。
    fn update_session_streaming_thinking_follow_tail(
        &mut self,
        session_id: usize,
        viewport: scrollable::Viewport,
    ) {
        self.session_streaming_thinking_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session floating-thinking overlay to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话流式思考浮层吸附到尾部的任务。
    fn maybe_follow_selected_session_thinking_overlay_tail(
        &self,
        session_id: usize,
    ) -> Task<Message> {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return Task::none();
        }

        if self.streaming_thinking_message(session_id).is_none() {
            return Task::none();
        }

        if !self.session_streaming_thinking_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(thinking_overlay_scrollable_id(session_id))
    }

    /// Cancels one pending inline-thinking toggle when it matches the provided session and optional block identifier filter.
    /// 当待执行内联思考切换与给定会话及可选区块标识过滤条件匹配时，取消该待执行切换。
    fn cancel_pending_thinking_inline_toggle(
        &mut self,
        session_id: Option<usize>,
        block_id: Option<&str>,
    ) {
        let should_clear = self
            .pending_thinking_inline_toggle
            .as_ref()
            .is_some_and(|pending| {
                session_id.is_none_or(|target_session_id| pending.session_id == target_session_id)
                    && block_id.is_none_or(|target_block_id| pending.block_id == target_block_id)
            });

        if should_clear {
            self.pending_thinking_inline_toggle = None;
        }
    }

    /// Commits one pending inline-thinking toggle once the double-click grace window has fully elapsed.
    /// 在双击宽限窗口完全结束后，正式提交一条待执行的内联思考切换。
    fn flush_pending_thinking_inline_toggle(&mut self) {
        let should_flush = self
            .pending_thinking_inline_toggle
            .as_ref()
            .is_some_and(|pending| pending.started_at.elapsed() >= THINKING_INLINE_TOGGLE_DELAY);

        if !should_flush {
            return;
        }

        let Some(pending) = self.pending_thinking_inline_toggle.take() else {
            return;
        };
        self.toggle_session_agent_view_block_collapsed(pending.session_id, &pending.block_id);
    }

    /// Opens one centered thinking modal for the provided block identifier and restarts the opening animation.
    /// 为给定区块标识打开一个居中思考弹层，并重新启动入场动画。
    fn open_thinking_modal(&mut self, session_id: usize, block_id: String) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| {
                modal_state.session_id == session_id
                    && modal_state.block_id == block_id
                    && modal_state.phase != ThinkingModalPhase::Closing
            })
        {
            return;
        }

        self.active_thinking_modal = Some(ThinkingModalState {
            session_id,
            block_id,
            phase: ThinkingModalPhase::Opening,
            started_at: Instant::now(),
        });
    }

    /// Starts closing the centered thinking modal when it is currently visible.
    /// 当居中思考弹层当前可见时，启动它的关闭动画。
    fn begin_close_thinking_modal(&mut self) {
        if let Some(modal_state) = self.active_thinking_modal.as_mut() {
            if modal_state.phase != ThinkingModalPhase::Closing {
                modal_state.phase = ThinkingModalPhase::Closing;
                modal_state.started_at = Instant::now();
            }
        }
    }

    /// Clears the centered thinking modal immediately when it belongs to the provided session.
    /// 当居中思考弹层属于给定会话时，立即清理该弹层。
    fn close_thinking_modal_for_session(&mut self, session_id: usize) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            self.active_thinking_modal = None;
        }
    }

    /// Advances the centered thinking modal animation and removes the modal once the closing phase completes.
    /// 推进居中思考弹层动画，并在关闭阶段完成后移除该弹层。
    fn advance_thinking_modal_animation(&mut self) {
        let Some(modal_state) = self.active_thinking_modal.as_mut() else {
            return;
        };

        if modal_state.started_at.elapsed() < THINKING_MODAL_ANIMATION_DURATION {
            return;
        }

        match modal_state.phase {
            ThinkingModalPhase::Opening => {
                modal_state.phase = ThinkingModalPhase::Open;
                modal_state.started_at = Instant::now();
            }
            ThinkingModalPhase::Open => {}
            ThinkingModalPhase::Closing => {
                self.active_thinking_modal = None;
            }
        }
    }

    /// Returns the session summary matching one identifier, if it still exists.
    /// 返回与某个标识匹配的会话摘要（如果仍然存在）。
    fn session_by_id(&self, session_id: usize) -> Option<&SessionSummary> {
        self.sessions
            .iter()
            .find(|session| session.id == session_id)
    }

    /// Returns the project node matching one identifier, if it still exists.
    /// 返回与某个标识匹配的项目节点（如果仍然存在）。
    fn task_project_by_id(&self, project_id: &str) -> Option<&TaskProjectSummary> {
        self.task_projects
            .iter()
            .find(|project| project.id == project_id)
    }

    /// Returns the mutable project node matching one identifier, if it still exists.
    /// 返回与某个标识匹配的可变项目节点（如果仍然存在）。
    fn task_project_by_id_mut(&mut self, project_id: &str) -> Option<&mut TaskProjectSummary> {
        self.task_projects
            .iter_mut()
            .find(|project| project.id == project_id)
    }

    /// Resolves which project currently owns the selected session so generic actions can target the right branch.
    /// 解析当前哪一个项目持有已选会话，以便通用动作能够命中正确分支。
    fn selected_task_project_id(&self) -> Option<String> {
        self.task_projects
            .iter()
            .find(|project| project.session_ids.contains(&self.selected_session))
            .map(|project| project.id.clone())
    }

    /// Returns the next project identifier in the same pinned or regular section.
    /// 返回同一置顶或普通分区中的下一个项目标识。
    fn next_task_project_in_section(&self, project_id: &str) -> Option<String> {
        let current_index = self
            .task_projects
            .iter()
            .position(|project| project.id == project_id)?;
        let section_is_pinned = self.task_projects.get(current_index)?.pinned;

        self.task_projects
            .iter()
            .skip(current_index + 1)
            .find(|project| project.pinned == section_is_pinned)
            .map(|project| project.id.clone())
    }

    /// Toggles the expanded state of one project node inside the main task tree.
    /// 切换主任务树中某个项目节点的展开状态。
    fn toggle_task_project_expanded(&mut self, project_id: &str) {
        if !self.expanded_task_projects.insert(project_id.to_string()) {
            self.expanded_task_projects.remove(project_id);
        }
    }

    /// Sorts projects into pinned and regular sections while preserving each section order value.
    /// 按置顶区与普通区对项目进行排序，并保留各自分区内部的顺序值。
    fn sort_task_projects(&mut self) {
        self.task_projects
            .sort_by_key(|project| (!project.pinned, project.order));
    }

    /// Reorders one dragged project before the hovered target when both belong to the same section.
    /// 当拖拽项目与悬停目标同属一个分区时，将前者移动到后者之前。
    fn reorder_task_project_before(&mut self, dragged_project_id: &str, target_project_id: &str) {
        if dragged_project_id == target_project_id {
            return;
        }

        let Some(dragged_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == dragged_project_id)
        else {
            return;
        };
        let Some(target_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == target_project_id)
        else {
            return;
        };

        if self.task_projects[dragged_index].pinned != self.task_projects[target_index].pinned {
            return;
        }

        let dragged_project = self.task_projects.remove(dragged_index);
        let insert_index = if dragged_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        };
        self.task_projects.insert(insert_index, dragged_project);
        self.normalize_task_project_orders();
    }

    /// Reorders one dragged project to the end of its current pinned or regular section.
    /// 将某个被拖拽的项目移动到其当前置顶或普通分区的末尾位置。
    fn reorder_task_project_to_section_end(
        &mut self,
        dragged_project_id: &str,
        section_is_pinned: bool,
    ) {
        let Some(dragged_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == dragged_project_id)
        else {
            return;
        };

        if self.task_projects[dragged_index].pinned != section_is_pinned {
            return;
        }

        let dragged_project = self.task_projects.remove(dragged_index);
        let insert_index = if section_is_pinned {
            self.task_projects
                .iter()
                .take_while(|project| project.pinned)
                .count()
        } else {
            self.task_projects.len()
        };

        self.task_projects.insert(insert_index, dragged_project);
        self.normalize_task_project_orders();
    }

    /// Applies one pending drop-target preview when a dragged project card is released.
    /// 在拖拽项目卡片释放时应用当前记录的插入目标预览。
    fn apply_task_project_drop_target(
        &mut self,
        dragged_project_id: &str,
        drop_target: &TaskProjectDropTarget,
    ) {
        match drop_target {
            TaskProjectDropTarget::Before(target_project_id) => {
                self.reorder_task_project_before(dragged_project_id, target_project_id);
            }
            TaskProjectDropTarget::EndOfPinned => {
                self.reorder_task_project_to_section_end(dragged_project_id, true);
            }
            TaskProjectDropTarget::EndOfRegular => {
                self.reorder_task_project_to_section_end(dragged_project_id, false);
            }
        }
    }

    /// Rebuilds stable order numbers after one visible project reorder.
    /// 一次可见项目重排后重新生成稳定顺序编号。
    fn normalize_task_project_orders(&mut self) {
        let mut pinned_order = 0usize;
        let mut regular_order = 0usize;

        for project in &mut self.task_projects {
            if project.pinned {
                project.order = pinned_order;
                pinned_order += 1;
            } else {
                project.order = regular_order;
                regular_order += 1;
            }
        }

        self.sort_task_projects();
    }

    /// Estimates a stable single-line pixel budget for one project title inside the task drawer header.
    /// 估算任务抽屉头部中单行项目标题可使用的稳定像素宽度预算。
    fn main_task_project_title_limit_width(&self) -> f32 {
        let horizontal_padding = 22.0;
        let leading_folder_width = 16.0;
        let inter_item_spacing = 24.0;
        let trailing_width = MAIN_TASK_PROJECT_ACTIONS_WIDTH + MAIN_TASK_PROJECT_ARROW_WIDTH;
        let title_compensation = 26.0;
        (self.main_task_panel_width
            - horizontal_padding
            - leading_folder_width
            - inter_item_spacing
            - trailing_width
            - title_compensation)
            .max(144.0)
    }

    /// Estimates a stable single-line pixel budget for one session title inside the task drawer.
    /// 估算任务抽屉内部单条会话标题可使用的稳定单行像素宽度预算。
    fn main_task_session_title_limit_width(&self) -> f32 {
        let trailing_slot_width = MAIN_TASK_SESSION_TRAILING_WIDTH;
        let structural_overhead = 84.0;
        (self.main_task_panel_width - trailing_slot_width - structural_overhead).max(112.0)
    }

    /// Creates one new session under the target project and updates all related prototype state.
    /// 在目标项目下创建一个新会话，并同步更新相关原型状态。
    fn create_task_project_session(&mut self, project_id: &str) {
        self.prune_unmaterialized_sessions(None);

        let session_id = self.next_session_id;
        self.next_session_id += 1;

        let project_title = self
            .task_project_by_id(project_id)
            .map(|project| project.title.clone())
            .unwrap_or_else(|| String::from("未命名项目"));

        self.sessions.insert(
            0,
            SessionSummary::new(
                session_id,
                Some(project_id.to_string()),
                "新对话",
                "等待首条真实用户输入后再正式生成会话标题与上下文。",
                SessionState::Draft,
                0,
                0,
                "刚刚",
            )
            .with_title_source(SessionTitleSource::Generated),
        );

        self.session_messages.entry(session_id).or_default();
        self.session_compactions.entry(session_id).or_default();
        self.session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        self.session_runtime_events.entry(session_id).or_default();
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        self.expanded_task_projects.insert(project_id.to_string());
        self.selected_session = session_id;
        self.hovered_task_project = Some(project_id.to_string());
        self.hovered_task_session = Some(session_id);
        self.suppress_task_session_hover = false;
        self.status_message =
            format!("已在项目 `{project_title}` 下创建新的空白对话草稿，等待首条真实输入。");
        self.push_activity(
            ActivityLevel::Info,
            format!("创建新对话草稿 #{session_id}"),
            format!("项目 `{project_title}` 已创建一个未入列的新生会话，首条真实对话后才会正式加入列表。"),
        );
    }

    /// Adds one new project node from the selected folder path or focuses the existing node when it already exists.
    /// 从选中的文件夹路径新增一个项目节点，若已存在则聚焦已有节点。
    fn add_task_project_from_folder(&mut self, selected_path: &Path) {
        let normalized_path = normalize_task_project_path(selected_path);
        let normalized_path_text = normalized_path.to_string_lossy().replace('\\', "/");

        if let Some(existing_project) = self.task_projects.iter().find(|project| {
            normalize_task_project_path(Path::new(&project.path))
                .to_string_lossy()
                .replace('\\', "/")
                == normalized_path_text
        }) {
            self.expanded_task_projects
                .insert(existing_project.id.clone());
            self.hovered_task_project = Some(existing_project.id.clone());
            self.status_message = format!(
                "项目 `{}` 已存在，已定位到当前节点。",
                existing_project.title
            );
            return;
        }

        if let Some(archived_index) = self.archived_task_projects.iter().position(|project| {
            normalize_task_project_path(Path::new(&project.path))
                .to_string_lossy()
                .replace('\\', "/")
                == normalized_path_text
        }) {
            let restored_project_id = self.archived_task_projects[archived_index].id.clone();
            let _ = self.restore_archived_task_project_by_id(&restored_project_id);
            return;
        }

        let title = normalized_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .map(|name| name.to_string())
            .unwrap_or_else(|| normalized_path_text.clone());
        let order = self
            .task_projects
            .iter()
            .filter(|project| !project.pinned)
            .map(|project| project.order)
            .max()
            .map(|order| order + 1)
            .unwrap_or(0);
        let project_id = self.build_task_project_id(&title, &normalized_path_text);

        self.task_projects.push(TaskProjectSummary::new(
            project_id.clone(),
            title.clone(),
            normalized_path_text.clone(),
            false,
            order,
            vec![],
        ));
        self.normalize_task_project_orders();
        self.expanded_task_projects.insert(project_id.clone());
        self.hovered_task_project = Some(project_id);
        self.status_message = format!("已从文件夹新增项目 `{title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("新增项目 `{title}`"),
            format!("已将目录 `{normalized_path_text}` 添加到项目树。"),
        );
    }

    /// Ensures the selected session still points to a valid session after tree-level archive operations.
    /// 在树级归档操作后确保当前选中会话仍然指向有效节点。
    fn ensure_valid_selected_session(&mut self) {
        if self
            .sessions
            .iter()
            .all(|session| session.id != self.selected_session)
        {
            self.selected_session = self.sessions.first().map(|session| session.id).unwrap_or(0);
        }
    }

    /// Builds one stable project identifier from a title and path while avoiding collisions in the current tree.
    /// 基于标题与路径构建稳定项目标识，并避免与当前树中的已有节点冲突。
    fn build_task_project_id(&self, title: &str, normalized_path_text: &str) -> String {
        let base_slug = slugify_task_project_title(title);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized_path_text.hash(&mut hasher);
        let path_suffix = format!("{:x}", hasher.finish());
        let path_suffix = &path_suffix[..path_suffix.len().min(8)];

        let candidate = if base_slug.is_empty() {
            format!("project-{path_suffix}")
        } else {
            format!("{base_slug}-{path_suffix}")
        };

        if self.task_project_by_id(&candidate).is_none() {
            return candidate;
        }

        let mut counter = 2usize;
        loop {
            let next_candidate = format!("{candidate}-{counter}");
            if self.task_project_by_id(&next_candidate).is_none() {
                return next_candidate;
            }
            counter += 1;
        }
    }

    /// Restores one archived project node together with all persisted session artifacts back into the visible tree.
    /// 将一个归档项目节点连同其全部持久化会话工件恢复回可见树。
    fn restore_archived_task_project_by_id(&mut self, project_id: &str) -> bool {
        let Some(archived_index) = self
            .archived_task_projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return false;
        };

        let restored_project = self.archived_task_projects.remove(archived_index);
        let restored_session_ids = restored_project.session_ids.clone();
        let restored_title = restored_project.title.clone();
        let restored_project_id = restored_project.id.clone();

        for session_id in &restored_session_ids {
            if let Some(index) = self
                .archived_sessions
                .iter()
                .position(|session| session.id == *session_id)
            {
                let restored_session = self.archived_sessions.remove(index);
                self.sessions.push(restored_session);
            }

            if let Some(messages) = self.archived_session_messages.remove(session_id) {
                self.session_messages.insert(*session_id, messages);
            }
            if let Some(compactions) = self.archived_session_compactions.remove(session_id) {
                self.session_compactions.insert(*session_id, compactions);
            }
            if let Some(runtime_state) = self.archived_session_runtime_state.remove(session_id) {
                self.session_runtime_state
                    .insert(*session_id, runtime_state);
            }
            self.session_run_phases
                .insert(*session_id, SessionRunPhase::Idle);
            self.session_runtime_events.remove(session_id);
            self.refresh_session_agent_view_buffers(*session_id);
        }

        self.task_projects.push(restored_project);
        self.normalize_task_project_orders();
        self.expanded_task_projects
            .insert(restored_project_id.clone());
        self.hovered_task_project = Some(restored_project_id);
        if let Some(restored_session_id) = restored_session_ids.first().copied() {
            self.selected_session = restored_session_id;
            self.hovered_task_session = Some(restored_session_id);
        }
        self.ensure_valid_selected_session();
        self.status_message = format!("已恢复归档项目 `{restored_title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("恢复项目 `{restored_title}`"),
            format!("项目 `{restored_title}` 及其会话已恢复到可见工作台树。"),
        );
        true
    }

    /// Restores one archived session row back into its still-visible owning project.
    /// 将一个归档会话行恢复回其仍然可见的所属项目。
    fn restore_archived_task_session(&mut self, session_id: usize) -> bool {
        let Some(archived_index) = self
            .archived_sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };

        let restored_session = self.archived_sessions.remove(archived_index);
        let restored_title = restored_session.title.clone();
        let Some(project_id) = restored_session.project_id.clone() else {
            self.archived_sessions
                .insert(archived_index, restored_session);
            return false;
        };

        {
            let Some(project) = self.task_project_by_id_mut(&project_id) else {
                self.archived_sessions
                    .insert(archived_index, restored_session);
                self.status_message =
                    format!("无法恢复会话 `{restored_title}`，因为所属项目当前不可见。");
                return false;
            };

            project.session_ids.insert(0, session_id);
        }
        self.sessions.insert(0, restored_session);
        if let Some(messages) = self.archived_session_messages.remove(&session_id) {
            self.session_messages.insert(session_id, messages);
        }
        if let Some(compactions) = self.archived_session_compactions.remove(&session_id) {
            self.session_compactions.insert(session_id, compactions);
        }
        if let Some(runtime_state) = self.archived_session_runtime_state.remove(&session_id) {
            self.session_runtime_state.insert(session_id, runtime_state);
        }
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        self.session_runtime_events.remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
        self.expanded_task_projects.insert(project_id);
        self.selected_session = session_id;
        self.hovered_task_session = Some(session_id);
        self.ensure_valid_selected_session();
        self.status_message = format!("已恢复归档会话 `{restored_title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("恢复会话 `{restored_title}`"),
            format!("归档会话 `{restored_title}` 已恢复到所属项目列表。"),
        );
        true
    }
}

/// Builds the initial pane-grid configuration for the three-column workbench.
/// 构建三栏工作台的初始分栏网格配置。
fn pane_layout() -> Configuration<PaneKind> {
    Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.22,
        a: Box::new(Configuration::Pane(PaneKind::Navigator)),
        b: Box::new(Configuration::Split {
            axis: pane_grid::Axis::Vertical,
            ratio: 0.68,
            a: Box::new(Configuration::Pane(PaneKind::Workbench)),
            b: Box::new(Configuration::Pane(PaneKind::Inspector)),
        }),
    }
}

/// Truncates one label to a compact single-line representation with a trailing ellipsis when needed.
/// 在需要时将标签截断为带尾部省略号的紧凑单行表示。
fn truncate_with_ellipsis(value: &str, max_width_px: f32) -> String {
    let display_width = value.chars().map(char_display_width_px).sum::<f32>();
    let ellipsis = "…";
    let ellipsis_width = 6.4_f32;

    if display_width <= max_width_px {
        return value.to_string();
    }

    if max_width_px <= ellipsis_width {
        return String::from(ellipsis);
    }

    let mut visible = String::new();
    let mut used_width = 0.0_f32;
    let allowed_width = max_width_px - ellipsis_width;

    for character in value.chars() {
        let character_width = char_display_width_px(character);

        if used_width + character_width > allowed_width {
            break;
        }

        visible.push(character);
        used_width += character_width;
    }

    format!("{visible}{ellipsis}")
}

/// Estimates the visual width in pixels consumed by one character in compact single-line labels.
/// 估算单个字符在紧凑单行标签中消耗的视觉像素宽度。
fn char_display_width_px(character: char) -> f32 {
    if character.is_ascii_whitespace() {
        3.6
    } else if character.is_ascii_punctuation() {
        4.9
    } else if character.is_ascii_digit() || character.is_ascii_uppercase() {
        7.9
    } else if character.is_ascii_lowercase() {
        7.0
    } else if matches!(
        character as u32,
        0x1100..=0x115F
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x3FFFD
    ) {
        12.8
    } else {
        7.8
    }
}

/// Compacts one relative-time label for narrow task-tree trailing slots.
/// 为狭窄任务树尾部槽位压缩相对时间标签。
fn compact_relative_label(value: &str) -> String {
    value.replace(' ', "").trim_end_matches('前').to_string()
}

/// Builds the stable scrollable identifier used by one session transcript surface across all transcript presentation modes.
/// 为单个会话转录表面在全部展示模式下构建稳定滚动容器标识。
fn transcript_scrollable_id(session_id: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("session-{session_id}-agent-transcript-scrollable"))
}

/// Returns whether one transcript viewport should still be treated as pinned to the newest tail content.
/// 返回某个转录视口是否仍应被视为贴住最新尾部内容。
fn transcript_viewport_is_near_bottom(viewport: scrollable::Viewport) -> bool {
    let absolute_offset = viewport.absolute_offset();
    let viewport_bounds = viewport.bounds();
    let content_bounds = viewport.content_bounds();
    let bottom_gap =
        (content_bounds.height - (absolute_offset.y + viewport_bounds.height)).max(0.0);

    bottom_gap <= TRANSCRIPT_FOLLOW_TAIL_BOTTOM_GAP_THRESHOLD
}

/// Converts one pointer overshoot request into a conventional edge-scroll step that accelerates with distance and caps near the far edge.
/// 将一次指针越界请求转换为常见的边缘滚动步长：随距离增大而加速，并在远离边缘时封顶。
fn drag_auto_scroll_step(request: SelectableTranscriptDragAutoScroll) -> f32 {
    let overshoot = request
        .overshoot
        .clamp(0.0, TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_OVERSHOOT);

    if overshoot <= TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE {
        return 0.0;
    }

    let normalized = ((overshoot - TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE)
        / (TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_OVERSHOOT - TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE))
        .clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - normalized).powi(2);
    let magnitude = TRANSCRIPT_DRAG_AUTO_SCROLL_MIN_STEP
        + (TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_STEP - TRANSCRIPT_DRAG_AUTO_SCROLL_MIN_STEP) * eased;

    match request.direction {
        SelectableTranscriptDragAutoScrollDirection::Up => -magnitude,
        SelectableTranscriptDragAutoScrollDirection::Down => magnitude,
    }
}

/// Parses one Markdown source string and builds the paired selectable buffer used by read-only transcript views.
/// 解析一段 Markdown 源文本，并构建只读转录视图使用的配套可选缓冲区。
fn parse_selectable_markdown_body(
    markdown_source: &str,
    theme: &Theme,
) -> (markdown::Content, text_editor::Content) {
    let markdown_content = markdown::Content::parse(markdown_source);
    let markdown_buffer = text_editor::Content::with_text(&build_selectable_markdown_text(
        markdown_source,
        &markdown_content,
        theme,
    ));

    (markdown_content, markdown_buffer)
}

/// Builds one selectable plain-text projection from parsed Markdown while preserving the major reading structure.
/// 从已解析 Markdown 构建一份可选纯文本投影，同时保留主要阅读结构。
fn build_selectable_markdown_text(
    markdown_source: &str,
    content: &markdown::Content,
    theme: &Theme,
) -> String {
    if markdown_items_contain_table(content.items()) {
        return markdown_source.to_string();
    }

    let rendered =
        markdown_items_to_selectable_text(content.items(), theme::markdown_style(theme), 0);
    let trimmed = rendered.trim_end().to_string();

    if trimmed.trim().is_empty() {
        markdown_source.to_string()
    } else {
        trimmed
    }
}

/// Returns whether the parsed Markdown tree contains any table node that cannot yet be faithfully projected with the public iced API.
/// 返回已解析 Markdown 树是否包含当前无法借助公开 iced API 完整投影的表格节点。
fn markdown_items_contain_table(items: &[markdown::Item]) -> bool {
    items.iter().any(|item| match item {
        markdown::Item::Table { .. } => true,
        markdown::Item::List { bullets, .. } => bullets.iter().any(|bullet| match bullet {
            markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                markdown_items_contain_table(items)
            }
        }),
        markdown::Item::Quote(items) => markdown_items_contain_table(items),
        markdown::Item::Heading(_, _)
        | markdown::Item::Paragraph(_)
        | markdown::Item::CodeBlock { .. }
        | markdown::Item::Image { .. }
        | markdown::Item::Rule => false,
    })
}

/// Converts a parsed Markdown item slice into one selectable plain-text block sequence.
/// 将一组已解析 Markdown 节点转换为一段可选纯文本区块序列。
fn markdown_items_to_selectable_text(
    items: &[markdown::Item],
    style: markdown::Style,
    list_depth: usize,
) -> String {
    items
        .iter()
        .map(|item| markdown_item_to_selectable_text(item, style, list_depth))
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Converts one parsed Markdown item into its selectable plain-text projection.
/// 将单个已解析 Markdown 节点转换为其可选纯文本投影。
fn markdown_item_to_selectable_text(
    item: &markdown::Item,
    style: markdown::Style,
    list_depth: usize,
) -> String {
    match item {
        markdown::Item::Heading(level, text) => format!(
            "{} {}",
            markdown_heading_prefix(level),
            markdown_text_to_selectable_text(text, style)
        ),
        markdown::Item::Paragraph(text) => markdown_text_to_selectable_text(text, style),
        markdown::Item::CodeBlock { language, code, .. } => {
            let fence = language
                .as_deref()
                .map(|language| format!("```{language}"))
                .unwrap_or_else(|| String::from("```"));
            let code_body = code.trim_end_matches('\n');

            if code_body.is_empty() {
                format!("{fence}\n```")
            } else {
                format!("{fence}\n{code_body}\n```")
            }
        }
        markdown::Item::List {
            start: None,
            bullets,
        } => bullets
            .iter()
            .map(|bullet| {
                let indent = "  ".repeat(list_depth);
                let (prefix, body) = match bullet {
                    markdown::Bullet::Point { items } => (
                        format!("{indent}- "),
                        markdown_items_to_selectable_text(items, style, list_depth + 1),
                    ),
                    markdown::Bullet::Task { items, done } => (
                        format!("{indent}- [{}] ", if *done { "x" } else { " " }),
                        markdown_items_to_selectable_text(items, style, list_depth + 1),
                    ),
                };

                prefix_multiline_markdown_block(&body, &prefix, &" ".repeat(prefix.chars().count()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::List {
            start: Some(start),
            bullets,
        } => bullets
            .iter()
            .enumerate()
            .map(|(index, bullet)| {
                let indent = "  ".repeat(list_depth);
                let prefix = format!("{indent}{}. ", *start + index as u64);
                let body = match bullet {
                    markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                        markdown_items_to_selectable_text(items, style, list_depth + 1)
                    }
                };

                prefix_multiline_markdown_block(&body, &prefix, &" ".repeat(prefix.chars().count()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::Image { url, title, alt } => {
            let alt_text = markdown_text_to_selectable_text(alt, style);

            if title.is_empty() {
                format!("![{alt_text}]({url})")
            } else {
                format!("![{alt_text}]({url} \"{title}\")")
            }
        }
        markdown::Item::Quote(items) => markdown_items_to_selectable_text(items, style, list_depth)
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::from(">")
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::Rule => String::from("---"),
        markdown::Item::Table { .. } => String::new(),
    }
}

/// Converts one parsed Markdown inline-text payload into a readable plain-text fragment sequence.
/// 将一段已解析 Markdown 行内文本载荷转换为可读的纯文本片段序列。
fn markdown_text_to_selectable_text(text: &markdown::Text, style: markdown::Style) -> String {
    let mut output = String::new();
    let mut active_link: Option<String> = None;
    let mut link_buffer = String::new();

    for span in text.spans(style).iter() {
        let span_link = span.link.clone();

        if span_link != active_link {
            flush_markdown_link_buffer(&mut output, &mut active_link, &mut link_buffer);
            active_link = span_link;
        }

        let mut fragment = span.text.as_ref().to_string();
        let is_inline_code = span.highlight.is_some() || span.font == Some(style.inline_code_font);

        if is_inline_code && !fragment.is_empty() {
            fragment = format!("`{fragment}`");
        }

        if active_link.is_some() {
            link_buffer.push_str(&fragment);
        } else {
            output.push_str(&fragment);
        }
    }

    flush_markdown_link_buffer(&mut output, &mut active_link, &mut link_buffer);
    output
}

/// Flushes one buffered Markdown link span group into the selectable plain-text output.
/// 将一组缓存中的 Markdown 链接片段刷新到可选纯文本输出中。
fn flush_markdown_link_buffer(
    output: &mut String,
    active_link: &mut Option<String>,
    link_buffer: &mut String,
) {
    let Some(link_url) = active_link.take() else {
        return;
    };

    if link_buffer.trim().is_empty() {
        output.push_str(&link_url);
    } else if link_buffer.trim() == link_url {
        output.push_str(link_buffer);
    } else {
        output.push_str(link_buffer);
        output.push_str(" (");
        output.push_str(&link_url);
        output.push(')');
    }

    link_buffer.clear();
}

/// Prefixes one multiline Markdown-derived block while keeping continuation lines aligned with the first content line.
/// 为多行 Markdown 派生区块补上前缀，并让后续续行与首行内容保持对齐。
fn prefix_multiline_markdown_block(
    body: &str,
    first_prefix: &str,
    continuation_prefix: &str,
) -> String {
    let mut lines = body.lines();
    let Some(first_line) = lines.next() else {
        return first_prefix.trim_end().to_string();
    };

    let mut output = String::from(first_prefix);
    output.push_str(first_line);

    for line in lines {
        output.push('\n');
        output.push_str(continuation_prefix);
        output.push_str(line);
    }

    output
}

/// Returns the Markdown heading marker that corresponds to one parsed heading level.
/// 返回与某个已解析标题层级对应的 Markdown 标题标记。
fn markdown_heading_prefix(level: &markdown::HeadingLevel) -> &'static str {
    match level {
        markdown::HeadingLevel::H1 => "#",
        markdown::HeadingLevel::H2 => "##",
        markdown::HeadingLevel::H3 => "###",
        markdown::HeadingLevel::H4 => "####",
        markdown::HeadingLevel::H5 => "#####",
        markdown::HeadingLevel::H6 => "######",
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value.
/// 以文本值形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    current_unix_millis().to_string()
}

/// Returns the current Unix timestamp in milliseconds as one integer value.
/// 以整数值形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

/// Parses one stored Unix-millis text field into an integer timestamp.
/// 将一个已存储的 Unix 毫秒文本字段解析为整数时间戳。
fn parse_unix_millis_text(value: &str) -> Option<u128> {
    value.trim().parse::<u128>().ok()
}

/// Converts one stored session message entry into the lightweight execution-context row sent to the runtime.
/// 将一条已存储会话消息项转换为发送到运行时的轻量执行上下文行。
fn session_message_into_execution_context(message: SessionMessageEntry) -> ExecutionContextMessage {
    ExecutionContextMessage {
        role: match message.kind {
            SessionMessageKind::User => String::from("user"),
            SessionMessageKind::Assistant => String::from("assistant"),
            SessionMessageKind::Error => String::from("error"),
        },
        title: message.title,
        body: message.body,
        record_kind: message.record_kind.as_persisted_key().to_string(),
    }
}

/// Builds one stable runtime-event identifier within the current session event stream.
/// 在当前会话事件流内部构建一条稳定运行时事件标识。
fn session_runtime_event_id(session_id: usize, event_index: usize, event: &RuntimeEvent) -> String {
    format!(
        "session-{session_id}-runtime-event-{event_index}-{}-{}",
        runtime_event_kind_label(event.kind),
        slugify_runtime_event_title(&event.title)
    )
}

/// Returns the compact label rendered for one runtime-event kind inside the transcript stream.
/// 返回转录流中某个运行时事件类型渲染使用的精简标签。
fn runtime_event_kind_label(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::TokenDelta => "输出",
        RuntimeEventKind::ToolStarted => "工具开始",
        RuntimeEventKind::ToolFinished => "工具完成",
        RuntimeEventKind::DelegateStarted => "委派开始",
        RuntimeEventKind::DelegateFinished => "委派完成",
        RuntimeEventKind::Failed => "失败",
    }
}

/// Builds one compact slug from a runtime-event title for stable in-session identifiers.
/// 根据运行时事件标题构建一条紧凑 slug，用于会话内稳定标识。
fn slugify_runtime_event_title(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    slug.trim_matches('-').chars().take(24).collect()
}

/// Opens one native folder-picker dialog and returns the selected path when the user confirms.
/// 打开一个原生文件夹选择对话框，并在用户确认时返回所选路径。
fn pick_task_project_folder(initial_directory: &str) -> Result<Option<std::path::PathBuf>, String> {
    let mut dialog = rfd::FileDialog::new().set_title("选择要添加到项目树的目录");
    let trimmed_directory = initial_directory.trim();

    if !trimmed_directory.is_empty() {
        let initial_path = Path::new(trimmed_directory);
        if initial_path.exists() && initial_path.is_dir() {
            dialog = dialog.set_directory(initial_path);
        }
    }

    Ok(dialog.pick_folder())
}

/// Normalizes one project path to a canonical form when the platform can resolve it.
/// 当平台能够解析时，将某个项目路径规范化为标准形式。
fn normalize_task_project_path(path: &Path) -> std::path::PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Builds one compact ASCII slug from the project title so generated identifiers stay readable.
/// 从项目标题构建紧凑的 ASCII slug，保证生成标识保持可读。
fn slugify_task_project_title(title: &str) -> String {
    slugify_identifier(title)
}

/// Builds one compact ASCII slug from an arbitrary identifier source so generated ids stay readable.
/// 从任意标识来源构建紧凑 ASCII slug，保证生成的 id 保持可读。
fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    slug.trim_matches('-').to_string()
}

/// Converts one optional string field into a trimmed `Option<String>`.
/// 将一个可选字符串字段转换为裁剪后的 `Option<String>`。
fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Joins MCP server arguments into one compact editable text field.
/// 将 MCP 服务器参数拼接为一段紧凑的可编辑文本字段。
fn join_mcp_server_args(args: &[String]) -> String {
    args.join(" | ")
}

/// Parses one compact MCP server argument text field into its vector representation.
/// 将一段紧凑 MCP 服务器参数文本解析成向量表示。
fn parse_mcp_server_args(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Joins MCP server environment variables into one compact editable text field.
/// 将 MCP 服务器环境变量拼接为一段紧凑的可编辑文本字段。
fn join_mcp_server_env(env: &BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parses one compact MCP server environment text field into its map representation.
/// 将一段紧凑 MCP 服务器环境变量文本解析成映射表示。
fn parse_mcp_server_env(value: &str) -> Result<BTreeMap<String, String>, String> {
    parse_key_value_pairs(value, "环境变量", "请使用 `KEY=VALUE` 并以分号分隔。")
}

/// Returns the MCP transport choices exposed by the settings form.
/// 返回设置表单暴露的 MCP 传输类型选项。
fn mcp_transport_choices() -> Vec<PersistedMcpTransportKind> {
    vec![
        PersistedMcpTransportKind::Stdio,
        PersistedMcpTransportKind::Http,
        PersistedMcpTransportKind::Sse,
    ]
}

/// Joins MCP server HTTP headers into one compact editable text field.
/// 将 MCP 服务器 HTTP 请求头拼接为一段紧凑的可编辑文本字段。
fn join_mcp_server_headers(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parses one compact MCP server HTTP headers field into its map representation.
/// 将一段紧凑 MCP 服务器 HTTP 请求头文本解析成映射表示。
fn parse_mcp_server_headers(value: &str) -> Result<BTreeMap<String, String>, String> {
    parse_key_value_pairs(value, "请求头", "请使用 `KEY=VALUE` 并以分号分隔。")
}

/// Parses one semicolon-delimited key-value text field into a map.
/// 将一段使用分号分隔的键值对文本解析成映射。
fn parse_key_value_pairs(
    value: &str,
    subject_label: &str,
    format_hint: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();

    for pair in value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            return Err(format!("{subject_label} `{pair}` 格式无效，{format_hint}"));
        };
        let key = raw_key.trim();
        if key.is_empty() {
            return Err(format!("{subject_label}名称不能为空。"));
        }
        map.insert(key.to_string(), raw_value.trim().to_string());
    }

    Ok(map)
}

/// Returns a compact human-readable label for one persisted agent kind.
/// 返回单个持久化代理类型的紧凑人类可读标签。
/// Opens one filesystem path with the host operating system file manager.
/// 使用宿主操作系统的文件管理器打开某个文件系统路径。
fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Returns the native settings used when opening the main workbench window.
/// 返回打开主工作台窗口时使用的原生窗口配置。
fn main_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1600.0, 960.0),
        position: window::Position::Centered,
        min_size: Some(iced::Size::new(1280.0, 800.0)),
        decorations: false,
        platform_specific: custom_window_platform_settings(),
        ..window::Settings::default()
    }
}

/// Returns the native settings used when opening the dedicated diff preview window.
/// 返回打开独立 Diff 预览窗口时使用的原生窗口配置。
fn diff_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1320.0, 900.0),
        position: window::Position::Centered,
        min_size: Some(iced::Size::new(960.0, 640.0)),
        decorations: false,
        platform_specific: custom_window_platform_settings(),
        ..window::Settings::default()
    }
}

/// Returns the platform-specific settings used by custom undecorated windows.
/// 返回自定义无系统装饰窗口使用的平台专属配置。
#[cfg(target_os = "windows")]
fn custom_window_platform_settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::Round,
        ..window::settings::PlatformSpecific::default()
    }
}

/// Returns the platform-specific settings used by custom undecorated windows.
/// 返回自定义无系统装饰窗口使用的平台专属配置。
#[cfg(not(target_os = "windows"))]
fn custom_window_platform_settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

/// Renders a tooltip-enabled secondary action button.
/// 渲染带提示信息的次级动作按钮。
fn action_button<'a>(label: &'a str, hint: &'a str, on_press: Message) -> Element<'a, Message> {
    tooltip(
        button(label).padding([10, 14]).on_press(on_press),
        hint,
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// Loads a Git diff preview from the target workspace for the temporary testing region.
/// 从目标工作区载入 Git diff 预览，供临时测试区域展示。
fn load_workspace_diff_preview(workspace_path: &str) -> DiffPreviewPayload {
    // Normalize user input early so command execution and diagnostics share the same target string.
    // 尽早规范化用户输入，保证命令执行与诊断信息使用同一个目标路径字符串。
    let trimmed_path = workspace_path.trim();

    if trimmed_path.is_empty() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "未提供工作区路径。",
            "请输入一个 Git 仓库目录，然后重新点击扫描工作区。",
        );

        return DiffPreviewPayload {
            status_message: String::from("扫描失败：工作区路径为空。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    let path = Path::new(trimmed_path);

    if !path.exists() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "目标路径不存在。",
            format!("无法载入 Diff，因为目录不存在：{trimmed_path}"),
        );

        return DiffPreviewPayload {
            status_message: format!("扫描失败：路径不存在 - {trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    if !path.is_dir() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "目标路径不是目录。",
            format!("无法载入 Diff，因为目标不是目录：{trimmed_path}"),
        );

        return DiffPreviewPayload {
            status_message: format!("扫描失败：目标不是目录 - {trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    // Verify the directory is inside a Git repository before attempting to render any diff preview.
    // 在渲染任何 diff 预览前先验证该目录位于 Git 仓库中。
    let repository_root = match run_git_command(path, &["rev-parse", "--show-toplevel"]) {
        Ok(output) => output.trim().to_string(),
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                String::new(),
                "Git 仓库校验失败。",
                format!("无法载入 Git Diff。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：{trimmed_path} 不是有效的 Git 仓库。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    // Capture the tracked diff body first, then append synthesized untracked-file diffs for a fuller review view.
    // 先获取已跟踪差异，再拼接未跟踪文件的合成 diff，以得到更完整的审查视图。
    let tracked_diff_output = match run_git_command(
        path,
        &[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--patch",
            "--unified=3",
        ],
    ) {
        Ok(output) => output,
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                repository_root.clone(),
                "Git Diff 生成失败。",
                format!("Git 命令执行失败。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：无法为 {trimmed_path} 生成 Git Diff。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    let (untracked_diff_output, untracked_file_count) = match load_untracked_file_diffs(Path::new(
        &repository_root,
    )) {
        Ok(payload) => payload,
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                repository_root.clone(),
                "未跟踪文件 Diff 生成失败。",
                format!(
                    "已跟踪文件 Diff 已可用，但未跟踪文件的补充 Diff 生成失败。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"
                ),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：无法补充 {trimmed_path} 中的新增文件 Diff。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    let diff_output = combine_diff_sections(&tracked_diff_output, &untracked_diff_output);

    let diff_names = run_git_command(
        path,
        &["diff", "--no-ext-diff", "--no-color", "--name-only"],
    )
    .unwrap_or_default();
    let status_output = run_git_command(path, &["status", "--short"]).unwrap_or_default();

    let tracked_changed_files = diff_names
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let workspace_status_entries = status_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let changed_files = tracked_changed_files + untracked_file_count;

    if !diff_output.trim().is_empty() {
        let panel = WorkspaceDiffPanel::from_unified_diff(repository_root.clone(), &diff_output);

        return DiffPreviewPayload {
            status_message: format!("已载入 Git Diff：{trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files,
            activity_level: ActivityLevel::Success,
        };
    }

    // Explain why an empty `git diff` can still happen when untracked files exist in the workspace.
    // 解释为什么在工作区存在未跟踪文件时，`git diff` 仍可能为空。
    let empty_diff_content = if workspace_status_entries > 0 {
        format!(
            "当前 `git diff` 没有可展示的受跟踪改动，但工作区仍然存在状态项。\n\n\
这通常意味着当前只有未跟踪文件，或者变更尚未进入常规 diff 范围。\n\n\
当前 `git status --short` 结果如下：\n{status_output}"
        )
    } else {
        String::from("当前工作区没有未提交差异。")
    };

    let panel = WorkspaceDiffPanel::with_notice(
        repository_root.clone(),
        "当前无可展示的跟踪差异。",
        empty_diff_content,
    );

    DiffPreviewPayload {
        status_message: format!("已扫描工作区：{trimmed_path}。"),
        summary: panel.summary_text(),
        panel,
        changed_files,
        activity_level: ActivityLevel::Info,
    }
}

/// Executes a Git command in the target workspace and returns its UTF-8-decoded stdout.
/// 在目标工作区中执行 Git 命令，并返回按 UTF-8 解码后的标准输出。
fn run_git_command(path: &Path, arguments: &[&str]) -> Result<String, String> {
    // Execute Git without involving a shell so paths containing spaces remain safe and predictable.
    // 直接执行 Git 而不经过 shell，保证包含空格的路径依然安全且可预测。
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(["-c", "i18n.logOutputEncoding=utf-8"])
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .map_err(|error| format!("无法启动 git 命令：{error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let diagnostic = if !stderr.is_empty() { stderr } else { stdout };

        Err(if diagnostic.is_empty() {
            format!(
                "git {:?} 执行失败，退出码为 {:?}。",
                arguments,
                output.status.code()
            )
        } else {
            diagnostic
        })
    }
}

/// Collects synthesized unified diffs for untracked files so new files appear in the review panel.
/// 收集未跟踪文件的合成 unified diff，使新增文件也能出现在审查面板中。
fn load_untracked_file_diffs(repository_root: &Path) -> Result<(String, usize), String> {
    let untracked_files = run_git_command(
        repository_root,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .unwrap_or_default();

    let mut collected_diffs = Vec::new();
    let mut file_count = 0;

    for relative_path in untracked_files
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        collected_diffs.push(build_new_file_diff(repository_root, relative_path)?);
        file_count += 1;
    }

    Ok((collected_diffs.join("\n"), file_count))
}

/// Builds a synthetic unified diff for one untracked file.
/// 为单个未跟踪文件构建合成的 unified diff。
fn build_new_file_diff(repository_root: &Path, relative_path: &str) -> Result<String, String> {
    let file_path = repository_root.join(relative_path);
    let display_path = relative_path.replace('\\', "/");
    let raw_bytes = fs::read(&file_path)
        .map_err(|error| format!("无法读取新增文件 `{display_path}`：{error}"))?;

    let mut diff_lines = vec![
        format!("diff --git a/{display_path} b/{display_path}"),
        String::from("new file mode 100644"),
        String::from("--- /dev/null"),
        format!("+++ b/{display_path}"),
    ];

    match String::from_utf8(raw_bytes) {
        Ok(content) => {
            let normalized_content = content.replace("\r\n", "\n");
            let content_lines = normalized_content.lines().collect::<Vec<_>>();

            if !content_lines.is_empty() {
                diff_lines.push(format!("@@ -0,0 +1,{} @@", content_lines.len()));
                diff_lines.extend(content_lines.into_iter().map(|line| format!("+{line}")));

                if !normalized_content.ends_with('\n') {
                    diff_lines.push(String::from("\\ No newline at end of file"));
                }
            }
        }
        Err(_) => {
            diff_lines.push(format!(
                "Binary files /dev/null and b/{display_path} differ"
            ));
        }
    }

    Ok(diff_lines.join("\n"))
}

/// Combines tracked and untracked diff sections into one unified payload.
/// 将已跟踪与未跟踪的 diff 片段合并为一个统一载荷。
fn combine_diff_sections(tracked_diff_output: &str, untracked_diff_output: &str) -> String {
    match (
        tracked_diff_output.trim().is_empty(),
        untracked_diff_output.trim().is_empty(),
    ) {
        (true, true) => String::new(),
        (false, true) => tracked_diff_output.to_string(),
        (true, false) => untracked_diff_output.to_string(),
        (false, false) => format!("{tracked_diff_output}\n{untracked_diff_output}"),
    }
}

/// Rebuilds wrapped diff-hunk payloads for the current wrap-column estimate.
/// 根据当前折行列数估算结果重建 Diff hunk 折行载荷。
fn rebuild_wrapped_diff_hunks(
    panel: &WorkspaceDiffPanel,
    wrap_columns: usize,
) -> BTreeMap<String, code_view::WrappedDiffHunk> {
    let mut wrapped_hunks = BTreeMap::new();

    for file in &panel.files {
        let file_key = file.key();

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            wrapped_hunks.insert(
                diff_hunk_buffer_key(&file_key, hunk_index),
                code_view::wrap_diff_hunk(hunk, wrap_columns),
            );
        }
    }

    wrapped_hunks
}

/// Builds one read-only text editor buffer from a virtualized wrapped-hunk slice.
/// 根据虚拟化后的折行 hunk 切片构建只读文本编辑器缓冲区。
fn build_diff_slice_buffer(
    wrapped_hunk: &code_view::WrappedDiffHunk,
    slice_range: DiffVisualSliceRange,
) -> text_editor::Content {
    let sliced_text = wrapped_hunk.text_lines[slice_range.start..slice_range.end].join("\n");

    text_editor::Content::with_text(&sliced_text)
}

/// Formats merged diff line numbers for the shared gutter.
/// 为共享 gutter 格式化合并后的 diff 行号。
fn format_diff_line_number(
    old_line_number: Option<usize>,
    new_line_number: Option<usize>,
) -> String {
    old_line_number
        .or(new_line_number)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Resolves the zero-based hovered visual line index from one local cursor Y position.
/// 根据局部鼠标 Y 坐标解析从零开始的悬停可视行索引。
fn hovered_diff_line_index_from_cursor(local_y: f32, line_count: usize) -> usize {
    let raw_index = (local_y / CODE_EDITOR_LINE_HEIGHT).floor().max(0.0) as usize;

    raw_index.min(line_count.saturating_sub(1))
}

/// Builds the stable buffer key for one hunk editor inside a diff file.
/// 为 diff 文件中的单个 hunk 编辑器构建稳定缓冲区键。
fn diff_hunk_buffer_key(file_key: &str, hunk_index: usize) -> String {
    format!("{file_key}::hunk::{hunk_index}")
}

/// Returns the total leading gutter width reserved before one diff code editor block.
/// 返回单个 diff 代码编辑器区块前方预留的总 gutter 宽度。
fn diff_gutter_block_width() -> f32 {
    DIFF_GUTTER_ARROW_WIDTH + DIFF_GUTTER_WIDTH + 5.0
}

/// Estimates the omitted unchanged line count between two adjacent hunks.
/// 估算两个相邻 hunk 之间被省略的未变化行数量。
fn omitted_context_line_count(
    previous_hunk: &crate::diff::DiffHunk,
    next_hunk: &crate::diff::DiffHunk,
) -> usize {
    let previous_end = previous_hunk
        .lines
        .iter()
        .rev()
        .find_map(preferred_display_line_number);
    let next_start = next_hunk
        .lines
        .iter()
        .find_map(preferred_display_line_number);

    match (previous_end, next_start) {
        (Some(previous_end), Some(next_start)) if next_start > previous_end => {
            next_start.saturating_sub(previous_end + 1)
        }
        _ => 0,
    }
}

/// Picks the most useful visual line number for omission calculations.
/// 为省略行计算选择最有用的可视行号。
fn preferred_display_line_number(line: &crate::diff::DiffLine) -> Option<usize> {
    line.new_number.or(line.old_number)
}

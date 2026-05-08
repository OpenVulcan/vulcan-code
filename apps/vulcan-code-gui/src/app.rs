//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::{
    self, ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport, RuntimeFoundation,
    RuntimeSkillKind, execute_runtime_run,
};
use crate::code_view;
use crate::diff::{DiffFile, WorkspaceDiffPanel};
use crate::model::{
    ActivityEntry, ActivityLevel, ExecutionMode, PaneKind, SessionMessageEntry, SessionMessageKind,
    SessionRunPhase, SessionState, SessionSummary, TaskProjectSummary,
};
use crate::provider_config::{
    self, ModelChoice, PersistedAgentKind, PersistedMcpServerEntry, PersistedMcpTransportKind,
    PersistedProviderEntry, PersistedProviderOptions, ProviderChoice, ProviderConfigurationFile,
    ProviderFieldKey,
};
use crate::style;
use crate::theme;
use crate::wheel_passthrough::WheelPassthrough;
use iced::widget::operation;
use iced::widget::pane_grid::{self, Configuration, PaneGrid};
use iced::widget::{
    button, checkbox, column, container, markdown, mouse_area, pick_list, progress_bar, radio, row,
    rule, scrollable, slider, space, stack, svg, text, text_editor, text_input, toggler, tooltip,
};
use iced::window;
use iced::{Center, Element, Fill, FillPortion, Pixels, Subscription, Task, Theme, keyboard};
use iced_aw::ContextMenu;
use iced_fonts::{
    CODICON_FONT_BYTES, FONTAWESOME_FONT_BYTES, LUCIDE_FONT_BYTES, codicon, fontawesome, lucide,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;

/// Path to the bundled SVG mark used in the header.
/// 头部区域使用的内置 SVG 标识路径。
const LOGO_ASSET_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/vulcan-mark.svg");

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
const MAIN_PANEL_ACTIVITY_ICON_SIZE: f32 = 20.0;

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
    .font(theme::code_font_bytes())
    .font(theme::code_italic_font_bytes())
    .font(theme::cjk_code_font_bytes())
    .font(theme::cjk_code_italic_font_bytes())
    .font(CODICON_FONT_BYTES)
    .font(FONTAWESOME_FONT_BYTES)
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
    /// Markdown content rendered in the inspector guide.
    /// 渲染在检查面板指引中的 Markdown 内容。
    quick_guide: markdown::Content,
    /// Session summaries rendered in the navigator pane.
    /// 渲染在导航面板中的会话摘要列表。
    sessions: Vec<SessionSummary>,
    /// Project nodes rendered by the main task-drawer tree.
    /// 由主任务抽屉树渲染的项目节点集合。
    task_projects: Vec<TaskProjectSummary>,
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
    /// Session-scoped transcript messages rendered inside the main workbench output area.
    /// 渲染在主工作台输出区中的按会话归属消息转录集合。
    session_messages: BTreeMap<usize, Vec<SessionMessageEntry>>,
    /// Session-scoped runtime events rendered beneath the transcript message stream.
    /// 渲染在消息流下方的按会话归属运行时事件集合。
    session_runtime_events: BTreeMap<usize, Vec<RuntimeEvent>>,
    /// Session-scoped execution phases reflected by the workbench status bar.
    /// 反映在工作台状态条中的按会话归属执行阶段集合。
    session_run_phases: BTreeMap<usize, SessionRunPhase>,
    /// Identifier seed for newly created sessions.
    /// 新建会话使用的自增标识种子。
    next_session_id: usize,
    /// Persisted provider configuration file loaded from `~/.vulcan/vulcan-code.json`.
    /// 从 `~/.vulcan/vulcan-code.json` 载入的持久化供应商配置文件。
    provider_configuration: ProviderConfigurationFile,
    /// Compose runtime foundation derived from the shared configuration file.
    /// 根据共享配置文件构建出的 compose 运行时基础架构。
    runtime_foundation: RuntimeFoundation,
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
#[derive(Debug, Clone, Default)]
struct McpServerFormState {
    /// Human-readable MCP server name.
    /// 人类可读的 MCP 服务器名称。
    name: String,
    /// Whether the MCP server is enabled.
    /// 当前 MCP 服务器是否启用。
    enabled: bool,
    /// Command used to start the MCP server process.
    /// 用于启动 MCP 服务器进程的命令。
    command: String,
    /// Delimited argument text shown in the settings form.
    /// 展示在设置表单中的分隔参数文本。
    args_text: String,
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
            command: entry.command.clone(),
            args_text: join_mcp_server_args(&entry.args),
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
            cwd: trim_to_option(&self.cwd),
            env: parse_mcp_server_env(&self.env_text)?,
        })
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

/// Messages emitted by iced widgets in the first prototype.
/// 首版原型中由 iced 控件发出的消息集合。
#[derive(Debug, Clone)]
enum Message {
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
    /// Updates the editable MCP server command field.
    /// 更新可编辑 MCP 服务器命令字段。
    McpServerCommandChanged(String),
    /// Updates the editable MCP server argument text field.
    /// 更新可编辑 MCP 服务器参数文本字段。
    McpServerArgsChanged(String),
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
    /// Application theme selected from the pick list.
    /// 从下拉选择框中选择了应用主题。
    ThemeSelected(Theme),
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
    /// Renames one session row inside the main task-drawer tree using a lightweight placeholder flow.
    /// 使用轻量占位流程重命名主任务抽屉树中的某个会话行。
    RenameTaskSession(usize),
    /// Archives one session row inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个会话行。
    ArchiveTaskSession(usize),
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
    /// Applies the completed real execution report returned by the async runtime task.
    /// 应用异步运行时任务返回的真实执行完成报告。
    RunExecutionFinished(ExecutionRunReport),
    /// Stop action requested.
    /// 请求停止当前动作。
    StopRun,
    /// Pane received focus through click.
    /// 面板通过点击获得焦点。
    PaneFocused(pane_grid::Pane),
    /// Pane grid split was resized.
    /// 分栏网格的分割线被调整。
    PaneResized(pane_grid::ResizeEvent),
    /// Markdown guide emitted a link interaction.
    /// Markdown 指引发出了链接交互事件。
    GuideLinkClicked(markdown::Uri),
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
        if agent_runtime::ensure_runtime_defaults(&mut provider_configuration) {
            match provider_config::save_configuration_file(&provider_configuration) {
                Ok(()) => {
                    provider_notice.push_str(" 已自动补齐 rig/compose/mcp 基础架构默认配置。");
                }
                Err(error) => {
                    provider_notice.push_str(&format!(
                        " 运行时默认配置已写入内存，但保存到磁盘失败：{error}"
                    ));
                }
            }
        }
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
        let runtime_foundation = RuntimeFoundation::build(&provider_configuration);
        let runtime_summary = runtime_foundation.summary();
        let sessions = SessionSummary::seeded();
        let session_run_phases = sessions
            .iter()
            .map(|session| (session.id, SessionRunPhase::Idle))
            .collect();

        let placeholder_diff_panel = WorkspaceDiffPanel::placeholder();
        let (main_window_id, open_main_window) = window::open(main_window_settings());

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
                quick_guide: markdown::Content::parse(QUICK_GUIDE_MARKDOWN),
                sessions,
                task_projects: TaskProjectSummary::seeded(),
                expanded_task_projects: TaskProjectSummary::seeded()
                    .into_iter()
                    .map(|project| project.id)
                    .collect(),
                hovered_task_project: None,
                hovered_task_session: None,
                active_task_project_drop_target: None,
                suppress_task_session_hover: false,
                pending_task_project_interaction: None,
                dragging_task_project: None,
                selected_session: 1,
                activities: ActivityEntry::seeded(),
                session_messages: BTreeMap::new(),
                session_runtime_events: BTreeMap::new(),
                session_run_phases,
                next_session_id: 5,
                provider_configuration,
                runtime_foundation,
                active_main_panel: MainDisplayPanel::Main,
                selected_provider,
                provider_form,
                provider_notice,
                mcp_notice: if selected_mcp_server_id.is_some() {
                    String::from("已载入 MCP 服务器配置。")
                } else {
                    String::from("尚未配置 MCP 服务器，可在此新增一个 stdio 服务器。")
                },
                selected_mcp_server_id,
                mcp_server_form,
                last_execution_preparation: None,
                last_mcp_discovery_rows: Vec::new(),
                last_mcp_discovery_errors: Vec::new(),
                execution_mode: ExecutionMode::Plan,
                app_theme: theme::default_theme(),
                temperature_enabled: false,
                temperature: 0.25,
                context_usage: 42.0,
                generate_plan: true,
                run_cargo_check: true,
                persist_memory: true,
                auto_approve: false,
                status_message: format!(
                    "运行时骨架已准备：{} 个代理 / {} 个技能 / {} 个原生工具。",
                    runtime_summary.built_agent_count,
                    runtime_summary.skill_count,
                    runtime_summary.native_tool_count
                ),
            },
            open_main_window.map(Message::MainWindowOpened),
        )
    }

    /// Handles widget messages and mutates the application state.
    /// 处理控件消息并更新应用状态。
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
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
                    MainDisplayPanel::LuaSkills => {
                        String::from("已切换到技能（LuaSkills）面板，当前显示占位内容。")
                    }
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
                    }

                    self.dragging_task_project = None;
                    self.active_task_project_drop_target = None;

                    if let Some(project_id) = toggled_project_id {
                        self.toggle_task_project_expanded(&project_id);
                    }
                }
                Task::none()
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
                self.provider_configuration.active_agent = Some(choice.id.clone());

                if let Err(error) =
                    provider_config::save_configuration_file(&self.provider_configuration)
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
            Message::McpServerCommandChanged(value) => {
                self.mcp_server_form.command = value;
                Task::none()
            }
            Message::McpServerArgsChanged(value) => {
                self.mcp_server_form.args_text = value;
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
                self.mcp_notice = String::from("已创建新的 MCP 服务器草稿，请填写名称与命令。");
                Task::none()
            }
            Message::SaveMcpServerConfiguration => {
                match self.save_selected_mcp_server_configuration() {
                    Ok(server_id) => {
                        self.selected_mcp_server_id = Some(server_id);
                        self.refresh_runtime_foundation();
                        self.reload_selected_mcp_server_form();
                        self.mcp_notice = String::from("MCP 服务器配置已保存。");
                        self.status_message = String::from("MCP 服务器配置已写入本地文件。");
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
                        self.refresh_runtime_foundation();
                        self.reload_selected_mcp_server_form();
                        self.mcp_notice = format!("已移除 MCP 服务器 `{removed_name}`。");
                        self.status_message = format!("MCP 服务器 `{removed_name}` 配置已删除。");
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
                self.selected_session = session_id;
                self.hovered_task_session = Some(session_id);

                // Capture the selected session title to keep footer feedback specific.
                // 记录被选中的会话标题，保证底部反馈足够具体。
                let selected_title = self
                    .selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"));

                self.status_message = format!("当前会话已切换为：{selected_title}。");
                Task::none()
            }
            Message::CreateSession => {
                let project_id = self
                    .selected_task_project_id()
                    .or_else(|| self.task_projects.first().map(|project| project.id.clone()));

                if let Some(project_id) = project_id {
                    self.create_task_project_session(&project_id);
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
                Task::none()
            }
            Message::AddTaskProjectFromFolder => {
                match pick_task_project_folder(self.workspace_path.trim()) {
                    Ok(Some(selected_path)) => {
                        self.add_task_project_from_folder(&selected_path);
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
                Task::none()
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
                Task::none()
            }
            Message::ArchiveTaskProject(project_id) => {
                if let Some(index) = self
                    .task_projects
                    .iter()
                    .position(|project| project.id == project_id)
                {
                    let project = self.task_projects.remove(index);
                    let archived_session_ids = project.session_ids.clone();
                    self.sessions
                        .retain(|session| !archived_session_ids.contains(&session.id));
                    for session_id in &archived_session_ids {
                        self.session_messages.remove(session_id);
                        self.session_runtime_events.remove(session_id);
                        self.session_run_phases.remove(session_id);
                    }
                    self.expanded_task_projects.remove(&project.id);
                    self.hovered_task_project = None;
                    self.hovered_task_session = None;
                    self.active_task_project_drop_target = None;
                    self.pending_task_project_interaction = None;
                    self.dragging_task_project = None;
                    self.ensure_valid_selected_session();
                    self.status_message = format!("已归档项目 `{}`。", project.title);
                }
                Task::none()
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
                Task::none()
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
                self.sessions.retain(|session| session.id != session_id);
                self.session_messages.remove(&session_id);
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
                Task::none()
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
                let request = if let Some(session) = self.selected_session() {
                    ExecutionPreparationRequest {
                        session_id: session.id,
                        session_title: session.title.clone(),
                        workspace_path: self.workspace_path.clone(),
                        prompt: self.composer.clone(),
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
                self.replace_session_runtime_events(request.session_id, Vec::new());
                self.last_mcp_discovery_rows.clear();
                self.last_mcp_discovery_errors.clear();
                self.set_session_run_phase(request.session_id, SessionRunPhase::Preparing);
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Running;
                    session.last_updated_label = String::from("刚刚");
                }
                self.status_message = String::from("正在发起真实执行，请稍候。");

                let request_for_run = request.clone();
                let config_for_run = self.provider_configuration.clone();

                Task::perform(
                    async move { execute_runtime_run(config_for_run, request_for_run).await },
                    Message::RunExecutionFinished,
                )
            }
            Message::RunExecutionFinished(report) => {
                let session_id = report.session_id;
                self.last_mcp_discovery_rows = report.mcp_discovery_rows.clone();
                self.last_mcp_discovery_errors = report.mcp_discovery_errors.clone();

                if let Some(preparation) = report.preparation.clone() {
                    let detail = preparation.summary_rows().join(" | ");
                    let assistant_body = format!(
                        "已基于当前代理与模型完成执行准备，并进入真实执行链路。\n\n{}",
                        preparation.summary_rows().join("\n")
                    );

                    self.context_usage = (self.context_usage + 8.0).min(100.0);
                    self.last_execution_preparation = Some(preparation.clone());
                    self.push_activity(ActivityLevel::Success, "执行准备已完成", detail);
                    self.push_session_message(
                        session_id,
                        SessionMessageKind::Assistant,
                        "执行准备摘要",
                        assistant_body,
                    );

                    for warning in &preparation.warnings {
                        self.push_activity(ActivityLevel::Warning, "执行准备警告", warning.clone());
                    }
                }

                self.replace_session_runtime_events(session_id, report.runtime_events.clone());

                for warning in &report.warnings {
                    self.push_activity(ActivityLevel::Warning, "执行运行警告", warning.clone());
                }

                if report.is_success() {
                    let assistant_output = report
                        .assistant_output
                        .clone()
                        .unwrap_or_else(|| String::from("模型已完成执行，但未返回可见文本输出。"));

                    if let Some(session) = self
                        .sessions
                        .iter_mut()
                        .find(|session| session.id == session_id)
                    {
                        session.status = SessionState::Completed;
                        session.focus =
                            String::from("最近一次真实执行已完成，可继续追问或调整参数。");
                        session.queued_steps = session.queued_steps.saturating_add(1);
                        session.last_updated_label = String::from("刚刚");
                    }

                    self.push_session_message(
                        session_id,
                        SessionMessageKind::Assistant,
                        "AI 输出",
                        assistant_output,
                    );
                    self.set_session_run_phase(session_id, SessionRunPhase::Ready);
                    self.status_message = String::from("真实执行已完成，结果已写入工作台输出区。");
                    self.push_activity(
                        ActivityLevel::Success,
                        "真实执行已完成",
                        format!("会话 #{session_id} 已完成真实模型执行并产出输出。"),
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

                Task::none()
            }
            Message::StopRun => {
                // Mark the current session as blocked to surface a safe stop point in the UI.
                // 将当前会话标记为阻塞状态，以在界面中显式体现安全停止点。
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Blocked;
                    session.last_updated_label = String::from("刚刚");
                }

                self.status_message = String::from("当前编排已暂停，等待下一步指令。");
                if let Some(session_id) = self.selected_session().map(|session| session.id) {
                    self.set_session_run_phase(session_id, SessionRunPhase::Paused);
                    self.push_session_message(
                        session_id,
                        SessionMessageKind::Assistant,
                        "执行已暂停",
                        "当前编排已暂停，保留现有上下文与会话输出，可在调整参数后再次发起执行。",
                    );
                }
                self.push_activity(
                    ActivityLevel::Warning,
                    "执行编排已暂停",
                    "当前处于人工确认窗口，可继续调整模型、策略或补丁内容。",
                );
                Task::none()
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
            Message::GuideLinkClicked(uri) => {
                self.status_message = format!("指引面板捕获到链接交互：{uri}");
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

        self.wrap_with_window_resize_handles(
            window_id,
            column![
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
            .into(),
        )
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

    /// Builds the currently selected top-level panel shown inside the main display region.
    /// 构建当前显示在主显示区域中的顶层面板内容。
    fn view_active_main_panel(&self) -> Element<'_, Message> {
        match self.active_main_panel {
            MainDisplayPanel::Main => self.view_empty_main_panel(),
            MainDisplayPanel::PromptSkills => {
                self.view_placeholder_main_panel(MainDisplayPanel::PromptSkills)
            }
            MainDisplayPanel::LuaSkills => {
                self.view_placeholder_main_panel(MainDisplayPanel::LuaSkills)
            }
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
        let icon_widget = match panel {
            MainDisplayPanel::Main => codicon::layout_panel_left(),
            MainDisplayPanel::PromptSkills => lucide::hammer(),
            MainDisplayPanel::LuaSkills => lucide::wrench(),
            MainDisplayPanel::Automation => lucide::timer(),
            MainDisplayPanel::Agents => lucide::bot(),
            MainDisplayPanel::Plugins => lucide::layout_grid(),
            MainDisplayPanel::VulcanMemoryMesh => fontawesome::brain(),
            MainDisplayPanel::Example => lucide::panels_left_bottom(),
            MainDisplayPanel::Settings => lucide::settings(),
        }
        .size(MAIN_PANEL_ACTIVITY_ICON_SIZE)
        .line_height(1.0);

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
        if project.session_ids.is_empty() {
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

        project
            .session_ids
            .iter()
            .fold(column![].spacing(4).width(Fill), |column, session_id| {
                if let Some(session) = self.session_by_id(*session_id) {
                    column.push(self.view_main_task_session_tree_row(session))
                } else {
                    column
                }
            })
            .into()
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
                text_input("输入当前会话要执行的任务指令", &self.composer)
                    .on_input(Message::ComposerChanged)
                    .padding(12),
                row![
                    text("执行后将把用户输入、AI 输出与运行时事件写入当前会话。").size(12),
                    space::horizontal(),
                    button("准备执行")
                        .padding([10, 14])
                        .style(button::primary)
                        .on_press(Message::StartRun),
                    button("暂停执行")
                        .padding([10, 14])
                        .style(button::secondary)
                        .on_press(Message::StopRun),
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
                    ]
                    .spacing(16)
                    .align_y(Center),
                )
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

    /// Builds the transcript stream area shown in the center of the main workbench canvas.
    /// 构建展示在主工作台中央画布中的会话转录流区域。
    fn view_main_canvas_transcript_stream(&self) -> Element<'_, Message> {
        let messages = self
            .selected_session_messages()
            .cloned()
            .unwrap_or_default();
        let runtime_events = self
            .selected_session_runtime_events()
            .cloned()
            .unwrap_or_default();

        if messages.is_empty() && runtime_events.is_empty() {
            return container(
                column![
                    text("尚未开始执行").size(18),
                    text("点击下方“准备执行”后，用户输入、AI 输出和运行时事件都会在这里持续沉淀。")
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

        let message_column = messages
            .into_iter()
            .fold(column![].spacing(10).width(Fill), |column, message| {
                column.push(self.view_main_canvas_message_entry(message))
            });
        let content_column = if runtime_events.is_empty() {
            message_column
        } else {
            let event_rows = runtime_events.into_iter().fold(
                column![text("运行时事件").size(13)].spacing(8),
                |column, event| column.push(self.view_main_canvas_runtime_event_entry(event)),
            );

            message_column.push(container(event_rows).padding([6, 0]))
        };

        container(scrollable(content_column).height(Fill).spacing(10))
            .padding(14)
            .height(Fill)
            .style(container::bordered_box)
            .into()
    }

    /// Builds one transcript message bubble shown inside the main workbench stream.
    /// 构建展示在主工作台消息流中的单条转录消息气泡。
    fn view_main_canvas_message_entry(&self, message: SessionMessageEntry) -> Element<'_, Message> {
        let kind = message.kind;
        let title = message.title;
        let body = message.body;
        let bubble = container(
            column![
                row![
                    text(kind.label()).size(11),
                    space::horizontal(),
                    text(title).size(13),
                ]
                .spacing(8)
                .align_y(Center),
                text(body).size(14),
            ]
            .spacing(8),
        )
        .padding([10, 12])
        .style(move |theme| style::workbench_message_bubble_style(theme, kind));

        match kind {
            SessionMessageKind::User => row![space::horizontal(), bubble.width(FillPortion(7))]
                .width(Fill)
                .into(),
            SessionMessageKind::Assistant | SessionMessageKind::Error => {
                row![bubble.width(FillPortion(8)), space::horizontal()]
                    .width(Fill)
                    .into()
            }
        }
    }

    /// Builds one runtime-event block shown beneath the transcript message list.
    /// 构建展示在转录消息列表下方的单条运行时事件块。
    fn view_main_canvas_runtime_event_entry(&self, event: RuntimeEvent) -> Element<'_, Message> {
        let kind = event.kind;
        let title = event.title;
        let detail = event.detail;
        container(
            column![
                row![
                    text(format!("事件 · {}", runtime_event_kind_label(kind))).size(11),
                    space::horizontal(),
                    text(title).size(13),
                ]
                .spacing(8)
                .align_y(Center),
                text(detail).size(13),
            ]
            .spacing(8),
        )
        .padding([10, 12])
        .width(Fill)
        .style(move |theme| style::workbench_runtime_event_style(theme, kind))
        .into()
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
            tooltip(
                button("准备执行")
                    .padding([10, 14])
                    .style(button::primary)
                    .on_press(Message::StartRun),
                "根据当前模型与执行模式启动编排准备。",
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
            tooltip(
                button("暂停执行")
                    .padding([10, 14])
                    .style(button::danger)
                    .on_press(Message::StopRun),
                "暂停当前编排并保留调整窗口。",
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
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
                    svg(LOGO_ASSET_PATH).width(20).height(20),
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
        Subscription::batch([
            window::close_events().map(Message::WindowClosed),
            window::resize_events()
                .map(|(window_id, size)| Message::WindowResized(window_id, size)),
            iced::event::listen_with(Self::map_main_panel_shortcut_event),
            iced::event::listen_with(Self::map_window_pointer_event),
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
                text_input("例如：实现 GUI 基础设施并接入 iced 组件", &self.composer)
                    .on_input(Message::ComposerChanged)
                    .padding(12),
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
                markdown::view(self.quick_guide.items(), self.app_theme.clone())
                    .map(Message::GuideLinkClicked),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box);

        // Build the activity log card to visualize system feedback and execution breadcrumbs.
        // 构建活动日志卡片，以可视化系统反馈与执行轨迹。
        let activity_log = container(self.activities.iter().fold(
            column![text("运行日志").size(18)].spacing(12),
            |list, entry| list.push(self.view_activity_card(entry)),
        ))
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
                    "尚未触发执行准备，可点击示例面板中的“准备执行”按钮生成第一份运行计划。",
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

    /// Renders the MCP configuration card that exposes server summaries and one editable stdio server form.
    /// 渲染 MCP 配置卡片，以暴露服务器摘要与单个可编辑的 stdio 服务器表单。
    fn view_mcp_configuration_card(&self) -> Element<'_, Message> {
        let server_choices = self.configured_mcp_server_choices();
        let selected_server = self.selected_mcp_server_choice();
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

        container(
            column![
                row![
                    text("MCP 服务器").size(18),
                    space::horizontal(),
                    text("当前仅开放 stdio").size(12),
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
    fn view_activity_card<'a>(&self, entry: &'a ActivityEntry) -> Element<'a, Message> {
        // Build a readable card so feedback can be scanned quickly during long-running tasks.
        // 构建易读卡片，便于在长任务期间快速扫描反馈。
        container(
            column![
                row![
                    text(entry.level.badge()).size(12),
                    space::horizontal(),
                    text(&entry.title).size(15),
                ]
                .align_y(Center),
                text(&entry.detail).size(13),
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
        self.provider_configuration
            .agents
            .iter()
            .map(|(id, entry)| AgentChoice {
                id: id.clone(),
                label: format!(
                    "{} · {}",
                    entry.name,
                    persisted_agent_kind_label(entry.kind)
                ),
            })
            .collect()
    }

    /// Returns the currently active agent choice rendered in the runtime settings card.
    /// 返回渲染在运行时设置卡片中的当前激活代理选择项。
    fn active_agent_choice(&self) -> Option<AgentChoice> {
        let active_agent_id = self.provider_configuration.active_agent.as_deref()?;

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
        self.runtime_foundation = RuntimeFoundation::build(&self.provider_configuration);
        self.last_execution_preparation = None;
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
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
    }

    /// Appends one transcript message to the target session while preserving stable insertion order.
    /// 向目标会话追加一条转录消息，并保持稳定插入顺序。
    fn push_session_message(
        &mut self,
        session_id: usize,
        kind: SessionMessageKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) {
        self.session_messages
            .entry(session_id)
            .or_default()
            .push(SessionMessageEntry::new(kind, title, body));
    }

    /// Replaces the runtime-event list for one session with a fresh snapshot.
    /// 使用新的快照替换某个会话的运行时事件列表。
    fn replace_session_runtime_events(&mut self, session_id: usize, events: Vec<RuntimeEvent>) {
        self.session_runtime_events.insert(session_id, events);
    }

    /// Updates the execution phase tracked for one session transcript.
    /// 更新某个会话转录区跟踪的执行阶段。
    fn set_session_run_phase(&mut self, session_id: usize, phase: SessionRunPhase) {
        self.session_run_phases.insert(session_id, phase);
    }

    /// Returns the currently selected session transcript messages.
    /// 返回当前选中会话的转录消息集合。
    fn selected_session_messages(&self) -> Option<&Vec<SessionMessageEntry>> {
        let session_id = self.selected_session().map(|session| session.id)?;
        self.session_messages.get(&session_id)
    }

    /// Returns the currently selected session runtime events.
    /// 返回当前选中会话的运行时事件集合。
    fn selected_session_runtime_events(&self) -> Option<&Vec<RuntimeEvent>> {
        let session_id = self.selected_session().map(|session| session.id)?;
        self.session_runtime_events.get(&session_id)
    }

    /// Returns the execution phase bound to the currently selected session transcript.
    /// 返回绑定到当前选中会话转录区的执行阶段。
    fn selected_session_run_phase(&self) -> SessionRunPhase {
        self.selected_session()
            .and_then(|session| self.session_run_phases.get(&session.id).copied())
            .unwrap_or(SessionRunPhase::Idle)
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
                format!("探索会话 #{session_id}"),
                "等待注入真实任务与上下文",
                SessionState::Draft,
                1,
                1,
                "刚刚",
            ),
        );

        if let Some(project) = self.task_project_by_id_mut(project_id) {
            project.session_ids.insert(0, session_id);
        }

        self.session_messages.entry(session_id).or_default();
        self.session_runtime_events.entry(session_id).or_default();
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        self.expanded_task_projects.insert(project_id.to_string());
        self.selected_session = session_id;
        self.hovered_task_project = Some(project_id.to_string());
        self.hovered_task_session = Some(session_id);
        self.suppress_task_session_hover = false;
        self.status_message =
            format!("已在项目 `{project_title}` 下创建新的探索会话 #{session_id}。");
        self.push_activity(
            ActivityLevel::Info,
            format!("创建会话 #{session_id}"),
            format!("项目 `{project_title}` 新增了一个等待编排的会话节点。"),
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

/// Opens one native folder-picker dialog and returns the selected path when the user confirms.
/// 打开一个原生文件夹选择对话框，并在用户确认时返回所选路径。
fn pick_task_project_folder(initial_directory: &str) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        return pick_task_project_folder_windows(initial_directory);
    }

    #[cfg(not(target_os = "windows"))]
    {
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
}

/// Escapes one string so it can be embedded safely inside a PowerShell single-quoted literal.
/// 对单个字符串执行转义，保证它可以安全嵌入 PowerShell 单引号字面量。
#[cfg(target_os = "windows")]
fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

/// Opens one Windows-native folder picker without going through the PNG-backed cross-platform dialog.
/// 打开一个 Windows 原生文件夹选择器，绕开依赖 PNG 资源链路的跨平台对话框。
#[cfg(target_os = "windows")]
fn pick_task_project_folder_windows(
    initial_directory: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    let trimmed_directory = initial_directory.trim();
    let initial_path_literal = if !trimmed_directory.is_empty() {
        let normalized = Path::new(trimmed_directory);
        if normalized.exists() && normalized.is_dir() {
            escape_powershell_single_quoted(&normalized.to_string_lossy())
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let script = format!(
        concat!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8;",
            "Add-Type -AssemblyName System.Windows.Forms;",
            "$dialog = New-Object System.Windows.Forms.FolderBrowserDialog;",
            "$dialog.Description = '选择要添加到项目树的目录';",
            "$dialog.ShowNewFolderButton = $true;",
            "$initialPath = '{initial_path_literal}';",
            "if ($initialPath.Length -gt 0 -and (Test-Path -LiteralPath $initialPath)) {{",
            "  $dialog.SelectedPath = $initialPath;",
            "}}",
            "$result = $dialog.ShowDialog();",
            "if ($result -eq [System.Windows.Forms.DialogResult]::OK -and $dialog.SelectedPath) {{",
            "  Write-Output $dialog.SelectedPath;",
            "}}"
        ),
        initial_path_literal = initial_path_literal,
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-STA",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|error| format!("无法打开系统文件夹选择对话框：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("系统文件夹选择对话框退出码异常：{}", output.status)
        } else {
            format!("系统文件夹选择对话框执行失败：{stderr}")
        };
        return Err(message);
    }

    let selected_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected_path.is_empty() {
        return Ok(None);
    }

    Ok(Some(std::path::PathBuf::from(selected_path)))
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
    let mut map = BTreeMap::new();

    for pair in value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            return Err(format!(
                "环境变量 `{pair}` 格式无效，请使用 `KEY=VALUE` 并以分号分隔。"
            ));
        };
        let key = raw_key.trim();
        if key.is_empty() {
            return Err(String::from("环境变量名称不能为空。"));
        }
        map.insert(key.to_string(), raw_value.trim().to_string());
    }

    Ok(map)
}

/// Returns a compact human-readable label for one persisted agent kind.
/// 返回单个持久化代理类型的紧凑人类可读标签。
fn persisted_agent_kind_label(kind: PersistedAgentKind) -> &'static str {
    match kind {
        PersistedAgentKind::Worker => "工作代理",
        PersistedAgentKind::Specialist => "专家代理",
        PersistedAgentKind::Coordinator => "协调代理",
    }
}

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

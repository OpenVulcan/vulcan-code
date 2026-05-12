//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。
mod configuration;
mod helpers;
mod task_tree;
mod views;

use self::helpers::*;

use crate::agent_profile::{self, AgentProfileCatalog, SYSTEM_DEFAULT_AGENT_CHOICE_ID};
use crate::agent_runtime::config::WORKSPACE_COORDINATOR_AGENT_ID;
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
    ActivityEntry, ActivityLevel, ExecutionMode, PaneKind, SessionCompactionEntry, SessionId,
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTitleSource, TaskProjectSummary,
};
use crate::project_temp_maintenance;
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
use crate::theme::{self, ThemePaletteExt};
use crate::wheel_passthrough::WheelPassthrough;
use crate::workspace_store::{self, WorkspaceSnapshot};
use iced::widget::operation;
use iced::widget::pane_grid::{self, Configuration, PaneGrid};
use iced::widget::{
    PickList, button, checkbox, container, markdown, mouse_area, pick_list as iced_pick_list,
    progress_bar, radio, row, rule, scrollable, slider, space, stack, svg, text, text_editor,
    text_input, toggler, tooltip,
};
use iced::window;
use iced::{
    Center, Element, Fill, FillPortion, Pixels, Shrink, Subscription, Task, Theme, keyboard, task,
};
use iced_aw::ContextMenu;
use iced_fonts::{CODICON_FONT_BYTES, LUCIDE_FONT_BYTES, codicon, lucide};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet, HashSet, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vulcan_agent_view::{
    THINKING_MODAL_ANIMATION_DURATION, THINKING_MODAL_TICK_INTERVAL, ThinkingModalPhase,
    build_modal_thinking_surface_document, build_modal_tool_surface_document,
    build_overlay_thinking_surface_document, build_thinking_title, format_thinking_duration_label,
    thinking_modal_progress, thinking_overlay_scrollable_id, view_thinking_modal_surface,
    view_thinking_overlay_surface,
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

/// Text size used by the workbench raw-source transcript preview so it matches the default Markdown reading size.
/// 工作台原文转录预览使用的字号，与默认 Markdown 阅读字号保持一致。
const WORKBENCH_TRANSCRIPT_CODE_PREVIEW_TEXT_SIZE: f32 = 14.0;

/// Legacy pick-list helper that preserves the older option-first call shape on top of iced 0.15-dev.
/// 在 iced 0.15-dev 之上保留旧版“选项在前”调用形式的下拉列表兼容辅助函数。
fn pick_list<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_select: impl Fn(T) -> Message + 'a,
) -> PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
    Message: Clone + 'a,
{
    iced_pick_list(selected, options, |value| value.to_string()).on_select(on_select)
}

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

/// Shared square size used by the floating transcript jump-to-bottom button.
/// 转录区浮动回底按钮使用的共享方形尺寸。
const TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE: f32 = 38.0;

/// Shared bottom inset used by the floating transcript jump-to-bottom button overlay.
/// 转录区浮动回底按钮覆盖层使用的共享底部内边距。
const TRANSCRIPT_JUMP_TO_BOTTOM_OVERLAY_BOTTOM_INSET: u16 = 26;

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

/// Stable widget identifier shared by the main prompt composer editor across compact and expanded surfaces.
/// 主提示词编辑器在紧凑态与展开态之间共用的稳定部件标识。
const COMPOSER_EDITOR_ID: &str = "vulcan-code.composer-editor";

/// Stable scrollable identifier used by the overflowed main prompt composer surface.
/// 主提示词编辑器溢出后滚动表面使用的稳定滚动容器标识。
const COMPOSER_EDITOR_SCROLLABLE_ID: &str = "vulcan-code.composer-editor-scrollable";

/// Stable widget identifier used by the inline workbench model-picker filter input.
/// 工作台内嵌模型选择弹窗过滤输入框使用的稳定部件标识。
const WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID: &str =
    "vulcan-code.workbench-model-picker-filter-input";

/// Shared logical line height used by the main prompt composer.
/// 主提示词编辑器使用的共享逻辑行高。
const COMPOSER_EDITOR_LINE_HEIGHT: f32 = 24.0;

/// Shared text size used by the main prompt composer editor content.
/// 主提示词编辑器正文内容使用的共享字号。
const COMPOSER_EDITOR_TEXT_SIZE: f32 = 16.0;

/// Shared combined vertical inset used by the main prompt composer surface.
/// 主提示词编辑器表面使用的共享纵向合计内边距。
const COMPOSER_EDITOR_VERTICAL_INSET: f32 = 20.0;

/// Shared fixed height used by the single-line main prompt composer surface.
/// 主提示词编辑器单行表面使用的共享固定高度。
const COMPOSER_SINGLE_LINE_HEIGHT: f32 = 44.0;

/// Shared outer inset used by the workbench composer panel around every child control.
/// 工作台提示词面板为每个子控件保留的共享外侧边距。
const COMPOSER_PANEL_EDGE_PADDING: u16 = 10;

/// Shared gap used between controls inside the workbench composer panel.
/// 工作台提示词面板内部控件之间使用的共享间距。
const COMPOSER_PANEL_CONTROL_GAP: f32 = 10.0;

/// Shared target height used by the low-profile control row under the main prompt composer.
/// 主提示词编辑器下方低矮控制行使用的共享目标高度。
const COMPOSER_CONTROL_ROW_HEIGHT: f32 = 36.0;

/// Shared square size used by the fixed send-or-stop button beside the main prompt composer.
/// 主提示词编辑器右侧固定发送或停止按钮使用的共享方形尺寸。
const COMPOSER_TRIGGER_BUTTON_SIZE: f32 = COMPOSER_CONTROL_ROW_HEIGHT;

/// Shared icon size used by the trailing circular send-or-stop button.
/// 尾部圆形发送或停止按钮使用的共享图标尺寸。
const COMPOSER_TRIGGER_BUTTON_ICON_SIZE: f32 = 18.0;

/// Shared vertical offset reserved below the floating multi-line composer overlay for the control row.
/// 为多行提示词浮动遮罩下方控制行预留的共享垂直偏移量。
const COMPOSER_OVERLAY_BOTTOM_OFFSET: f32 =
    COMPOSER_CONTROL_ROW_HEIGHT + COMPOSER_PANEL_CONTROL_GAP + COMPOSER_PANEL_EDGE_PADDING as f32;

/// Shared fixed width used by the compact execution-mode selector under the main prompt composer.
/// 主提示词编辑器下方紧凑执行模式选择器使用的共享固定宽度。
const WORKBENCH_EXECUTION_MODE_SELECTOR_WIDTH: f32 = 66.0;

/// Shared fixed width used by the inline workbench model trigger surface.
/// 工作台内联模型触发表面使用的共享固定宽度。
const WORKBENCH_MODEL_SELECTOR_WIDTH: f32 = 244.0;

/// Shared fixed width used by the compact reasoning-effort selector placed before the send button.
/// 放置在发送按钮之前的紧凑推理级别选择器使用的共享固定宽度。
const WORKBENCH_REASONING_SELECTOR_WIDTH: f32 = 76.0;

/// Shared fixed width used by the in-app workbench model-picker modal card.
/// 工作台内嵌模型选择弹窗卡片使用的共享固定宽度。
const WORKBENCH_MODEL_PICKER_MODAL_WIDTH: f32 = 680.0;

/// Shared fixed height used by the scrollable result list inside the workbench model-picker modal.
/// 工作台模型选择弹窗内部滚动结果列表使用的共享固定高度。
const WORKBENCH_MODEL_PICKER_LIST_HEIGHT: f32 = 340.0;

/// Returns the logical visible line count of one composer buffer while preserving trailing blank lines.
/// 返回一个提示词编辑器缓冲区的逻辑可见行数，并保留尾部空白行。
fn composer_visible_line_count(buffer: &text_editor::Content) -> usize {
    buffer.line_count().max(1)
}

/// Returns whether one composer text payload contains any submittable non-whitespace content.
/// 返回某段提示词文本是否包含任何可提交的非空白内容。
fn composer_text_has_submittable_content(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Returns the outer scroll offset required to keep the active composer cursor line visible inside the capped viewport.
/// 返回让当前提示词光标行保持在封顶视口内可见所需的外层滚动偏移量。
fn composer_caret_scroll_target(
    cursor_line: usize,
    viewport_height: f32,
    current_scroll_offset: f32,
) -> f32 {
    let caret_top = (cursor_line as f32 * COMPOSER_EDITOR_LINE_HEIGHT) + 10.0;
    let caret_bottom = caret_top + COMPOSER_EDITOR_LINE_HEIGHT;
    let current_viewport_bottom = current_scroll_offset + viewport_height;

    if caret_top < current_scroll_offset {
        caret_top
    } else if caret_bottom > current_viewport_bottom {
        (caret_bottom - viewport_height).max(0.0)
    } else {
        current_scroll_offset.max(0.0)
    }
}

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
    /// Prompt composer editor buffer used by the main Build/Plan input surface.
    /// 主 Build/Plan 输入表面使用的提示词编辑器缓冲区。
    composer_buffer: text_editor::Content,
    /// Current outer scroll offset used by the overflowed prompt composer viewport.
    /// 当前溢出提示词编辑器视口使用的外层滚动偏移量。
    composer_scroll_offset: f32,
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
    hovered_task_session: Option<SessionId>,
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
    selected_session: SessionId,
    /// Runtime activity log rendered in the inspector pane.
    /// 渲染在检查面板中的运行活动日志。
    activities: Vec<ActivityEntry>,
    /// Read-only activity-detail buffers used by the selectable inspector log cards.
    /// 检查面板可选运行日志卡片使用的只读活动详情缓冲区。
    activity_raw_buffers: Vec<text_editor::Content>,
    /// Session-scoped transcript messages rendered inside the main workbench output area.
    /// 渲染在主工作台输出区中的按会话归属消息转录集合。
    session_messages: BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    /// Archived session summaries retained outside the visible workbench for later restoration.
    /// 保留在可见工作台之外、供后续恢复使用的归档会话摘要集合。
    archived_sessions: Vec<SessionSummary>,
    /// Archived session-scoped transcript messages retained for later restoration.
    /// 为后续恢复保留的按归档会话归属消息转录集合。
    archived_session_messages: BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    /// Session-scoped hidden compaction checkpoints retained for future context reconstruction.
    /// 为未来上下文重建保留的按会话归属隐藏压缩检查点集合。
    session_compactions: BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    /// Archived hidden compaction checkpoints retained for later restoration.
    /// 为后续恢复保留的按归档会话归属隐藏压缩检查点集合。
    archived_session_compactions: BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    /// Session-scoped runtime-state snapshots retained for title, summary, and compaction chaining.
    /// 为标题、摘要与压缩串联保留的按会话归属运行时状态快照集合。
    session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Archived session-scoped runtime-state snapshots retained for later restoration.
    /// 为后续恢复保留的按归档会话归属运行时状态快照集合。
    archived_session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Pending destructive archived-item deletion confirmation currently shown inline to the user.
    /// 当前以内联形式展示给用户的待确认破坏性归档项删除状态。
    pending_archived_delete_confirmation: Option<ArchivedDeleteConfirmationState>,
    /// Session-scoped runtime events rendered beneath the transcript message stream.
    /// 渲染在消息流下方的按会话归属运行时事件集合。
    session_runtime_events: BTreeMap<SessionId, Vec<RuntimeEvent>>,
    /// Session-scoped collapsed runtime-event identifiers kept only in memory for the current launch.
    /// 仅在当前启动周期内保留的按会话归属已折叠运行时事件标识集合。
    collapsed_runtime_event_ids: BTreeMap<SessionId, BTreeSet<String>>,
    /// Session-scoped Markdown-view message identifiers used when the user explicitly switches from selectable source text back to Markdown.
    /// 当用户从可选原文显式切回 Markdown 时使用的按会话归属 Markdown 消息标识集合。
    markdown_session_message_ids: BTreeMap<SessionId, BTreeSet<String>>,
    /// Session-scoped Markdown-view runtime-event identifiers used when the user explicitly switches from selectable source text back to Markdown.
    /// 当用户从可选原文显式切回 Markdown 时使用的按会话归属 Markdown 运行时事件标识集合。
    markdown_runtime_event_ids: BTreeMap<SessionId, BTreeSet<String>>,
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
    streaming_thinking_message_ids: BTreeMap<SessionId, String>,
    /// Session identifiers whose current streaming thinking message should still stay in the floating overlay.
    /// 当前流式思考消息仍应停留在浮层中的会话标识集合。
    floating_streaming_thinking_sessions: BTreeSet<SessionId>,
    /// Session-scoped flag telling whether the floating streaming-thinking overlay should keep auto-following its latest tail.
    /// 以会话为粒度保存的流式思考浮层是否应继续自动追随最新尾部标记。
    session_streaming_thinking_follow_tail: BTreeMap<SessionId, bool>,
    /// Centered modal state retained when the user explicitly opens one inline thinking block for closer inspection.
    /// 当用户显式打开某个内联思考区块进行聚焦查看时保留的居中弹层状态。
    active_thinking_modal: Option<ThinkingModalState>,
    /// Centered modal state retained when the user explicitly opens one inline tool block for closer inspection.
    /// 当用户显式打开某个内联工具区块进行聚焦查看时保留的居中弹层状态。
    active_tool_modal: Option<ToolModalState>,
    /// Active streaming output-message identifiers keyed by session id.
    /// 以会话标识为键保存的当前流式输出消息标识集合。
    streaming_output_message_ids: BTreeMap<SessionId, String>,
    /// Per-run tool-call message identifiers keyed by session id and tool-call id.
    /// 以会话标识与工具调用标识为键保存的单次运行工具调用消息标识集合。
    session_tool_call_message_ids: BTreeMap<SessionId, BTreeMap<String, String>>,
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
    session_agent_view_buffers: BTreeMap<SessionId, text_editor::Content>,
    /// Unified raw-source transcript buffers keyed by visible session id.
    /// 以可见会话标识为键保存的统一原始源码转录缓冲区。
    session_agent_view_raw_buffers: BTreeMap<SessionId, text_editor::Content>,
    /// Last provider-request preview buffers keyed by visible session id.
    /// 以可见会话标识为键保存的最后一次 Provider 请求预览缓冲区。
    session_last_api_request_buffers: BTreeMap<SessionId, text_editor::Content>,
    /// Unified selectable transcript projections keyed by visible session id.
    /// 以可见会话标识为键保存的统一可选转录投影对象。
    session_agent_view_selectable_documents:
        BTreeMap<SessionId, vulcan_agent_view::AgentViewSelectableDocument>,
    /// Unified structured transcript documents keyed by visible session id.
    /// 以可见会话标识为键保存的统一结构化转录文档。
    session_agent_view_documents: BTreeMap<SessionId, vulcan_agent_view::AgentViewDocument>,
    /// Unified block collapsed states keyed by session id and stable block identifier.
    /// 以会话标识与稳定区块标识为键保存的统一块折叠状态。
    session_agent_view_block_collapsed: BTreeMap<SessionId, BTreeMap<String, bool>>,
    /// Session-scoped flag telling whether the transcript should keep auto-following the newest tail output.
    /// 以会话为粒度保存的转录自动追随最新尾部输出标记。
    session_agent_view_follow_tail: BTreeMap<SessionId, bool>,
    /// Latest transcript scrollable viewport keyed by session id.
    /// 以会话标识为键保存的最新转录滚动视口。
    session_agent_view_viewports: BTreeMap<SessionId, scrollable::Viewport>,
    /// Active drag auto-scroll state for the main rendered transcript surface.
    /// 主渲染转录表面的当前拖拽自动滚动状态。
    active_transcript_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Latest floating thinking-overlay viewport keyed by session id.
    /// 以会话标识为键保存的最新浮动思考层视口。
    session_streaming_thinking_viewports: BTreeMap<SessionId, scrollable::Viewport>,
    /// Active drag auto-scroll state for the floating thinking overlay body.
    /// 流式思考浮层正文的当前拖拽自动滚动状态。
    active_thinking_overlay_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Surface mode currently used by the unified transcript panel.
    /// 当前统一转录面板使用的表面模式。
    agent_transcript_surface_mode: AgentTranscriptSurfaceMode,
    /// Session-scoped execution phases reflected by the workbench status bar.
    /// 反映在工作台状态条中的按会话归属执行阶段集合。
    session_run_phases: BTreeMap<SessionId, SessionRunPhase>,
    /// Monotonic identifier seed used to distinguish restarted or stopped execution streams.
    /// 用于区分重启后或已停止执行流的单调递增标识种子。
    next_run_id: u64,
    /// Active execution stream identifier tracked while one real AI call is still in flight.
    /// 当真实 AI 调用仍在进行中时跟踪的活动执行流标识。
    active_run_id: Option<u64>,
    /// Session identifier that currently owns the active real AI call.
    /// 当前拥有活动真实 AI 调用的会话标识。
    active_run_session_id: Option<SessionId>,
    /// Abort handle used to stop the active real AI call at the iced task layer.
    /// 用于在 iced 任务层停止当前真实 AI 调用的中止句柄。
    active_run_handle: Option<task::Handle>,
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
    /// Currently selected user-agent identifier in the dedicated Agent management panel.
    /// 当前在独立 Agent 管理面板中选中的用户 Agent 标识。
    selected_user_agent_profile_id: Option<String>,
    /// Editable user-agent form state used by the dedicated Agent management panel.
    /// 独立 Agent 管理面板使用的可编辑用户 Agent 表单状态。
    agent_profile_form: AgentProfileFormState,
    /// Local panel-scoped feedback message shown near the dedicated Agent management widgets.
    /// 展示在独立 Agent 管理控件附近的局部反馈消息。
    agent_profile_notice: String,
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
    /// Selected compact reasoning-effort preset shown beside the send button in the workbench composer.
    /// 显示在工作台提示词发送按钮旁边的已选紧凑推理级别预设。
    workbench_reasoning_effort: WorkbenchReasoningEffort,
    /// Whether the in-app workbench model-picker modal is currently visible.
    /// 工作台内嵌模型选择弹窗当前是否可见。
    workbench_model_picker_open: bool,
    /// Current free-text filter value applied inside the workbench model-picker modal.
    /// 当前在工作台模型选择弹窗内部应用的自由文本过滤值。
    workbench_model_picker_filter: String,
    /// Currently highlighted filtered option index inside the workbench model-picker modal.
    /// 当前在工作台模型选择弹窗中过滤结果内高亮的选项索引。
    workbench_model_picker_highlighted_index: Option<usize>,
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
    /// Selectable raw-source projection backed by the unified agent-view document.
    /// 由统一 agent-view 文档支撑的可选原始源码投影。
    RawSource,
    /// Read-only preview of the last full provider request captured before one real API call starts.
    /// 在一次真实 API 调用开始前捕获的最后一次完整 Provider 请求只读预览。
    ApiRequest,
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
struct ModalSurfacePayload {
    /// Stable title rendered in the modal header without needing to rebuild it on every animation tick.
    /// 渲染在弹层头部的稳定标题，无需在每个动画帧里重复重建。
    title: String,
    /// Stable helper copy rendered below the modal title.
    /// 渲染在弹层标题下方的稳定辅助文案。
    helper_text: String,
    /// Snapshot document rendered inside the modal body.
    /// 渲染在弹层正文中的快照文档。
    document: vulcan_agent_view::AgentViewDocument,
    /// Stable hash used by lazy body rendering so heavy transcript content only rebuilds when payload really changes.
    /// 供惰性正文渲染使用的稳定哈希，使重型转录内容仅在载荷真正变化时重建。
    document_signature: u64,
}

/// Centered modal state retained while one inline thinking block is explicitly opened above the transcript.
/// 当某个内联思考区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
struct ThinkingModalState {
    /// Session identifier owning the currently opened thinking block.
    /// 当前已打开思考区块所属的会话标识。
    session_id: SessionId,
    /// Stable rendered-transcript block identifier of the opened thinking block.
    /// 当前已打开思考区块的稳定渲染转录区块标识。
    block_id: String,
    /// Current lifecycle phase of the modal so opening and closing animations can share one state container.
    /// 当前弹层的生命周期阶段，以便打开与关闭动画共享同一状态容器。
    phase: ThinkingModalPhase,
    /// Monotonic timestamp captured when the current phase began.
    /// 当前阶段开始时捕获的单调时间戳。
    started_at: Instant,
    /// Stable snapshot payload reused across animation ticks.
    /// 跨动画帧复用的稳定快照载荷。
    payload: ModalSurfacePayload,
}

/// Centered modal state retained while one inline tool block is explicitly opened above the transcript.
/// 当某个内联工具区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
struct ToolModalState {
    /// Session identifier owning the currently opened tool block.
    /// 当前已打开工具区块所属的会话标识。
    session_id: SessionId,
    /// Stable rendered-transcript block identifier of the opened tool block.
    /// 当前已打开工具区块的稳定渲染转录区块标识。
    block_id: String,
    /// Current lifecycle phase of the modal so opening and closing animations can share one state container.
    /// 当前弹层的生命周期阶段，以便打开与关闭动画共享同一状态容器。
    phase: ThinkingModalPhase,
    /// Monotonic timestamp captured when the current phase began.
    /// 当前阶段开始时捕获的单调时间戳。
    started_at: Instant,
    /// Stable snapshot payload reused across animation ticks.
    /// 跨动画帧复用的稳定快照载荷。
    payload: ModalSurfacePayload,
}

/// Active drag auto-scroll snapshot retained while one transcript-like surface keeps selecting beyond its visible viewport.
/// 当某个转录类表面在可见视口之外继续拖拽选择时保留的活动自动滚动快照。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveTranscriptDragAutoScroll {
    /// Session identifier that owns the scrollable surface currently being auto-scrolled.
    /// 当前正在被自动滚动的滚动表面所属会话标识。
    session_id: SessionId,
    /// Latest overshoot request reported by the selectable transcript widget.
    /// 可选择转录组件最近一次回传的越界滚动请求。
    request: SelectableTranscriptDragAutoScroll,
}

/// Stable archived-item deletion target retained while the user is going through the destructive second confirmation.
/// 当用户正在经过破坏性二次确认流程时保留的稳定归档项删除目标。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ArchivedDeleteTarget {
    /// Permanently deletes one archived project together with all child archived sessions.
    /// 彻底删除一个归档项目及其全部子归档会话。
    Project(String),
    /// Permanently deletes one archived session row.
    /// 彻底删除一条归档会话行。
    Session(SessionId),
}

/// Pending destructive archived-item deletion confirmation shown inline inside the archived area.
/// 显示在归档区域内部的待确认破坏性归档项删除状态。
#[derive(Debug, Clone)]
struct ArchivedDeleteConfirmationState {
    /// Stable deletion target that will be removed permanently after the second confirmation.
    /// 完成二次确认后将被永久移除的稳定删除目标。
    target: ArchivedDeleteTarget,
    /// Human-readable title echoed back to the user in the destructive confirmation copy.
    /// 在破坏性确认文案中回显给用户的人类可读标题。
    title: String,
}

/// Result payload returned after one archived-item hard-delete request finishes.
/// 单次归档项硬删除请求完成后返回的结果载荷。
#[derive(Debug, Clone)]
struct ArchivedDeleteOutcome {
    /// Stable deletion target that has already been removed from persistence.
    /// 已经从持久化层移除的稳定删除目标。
    target: ArchivedDeleteTarget,
    /// Human-readable title used for status and confirmation messages.
    /// 用于状态与确认文案的人类可读标题。
    title: String,
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

/// Workbench-scoped model option that keeps the visible model name and the owning provider label together.
/// 工作台作用域内的模型选项，同时保留可见模型名称与所属供应商标签。
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkbenchModelChoice {
    /// Stable provider identifier owning the current model option.
    /// 拥有当前模型选项的稳定供应商标识。
    provider_id: String,
    /// Human-readable provider label shown together with the chosen model.
    /// 与已选模型一起展示的人类可读供应商标签。
    provider_label: String,
    /// Stable model identifier persisted into the shared configuration file.
    /// 持久化到共享配置文件中的稳定模型标识。
    model_id: String,
    /// Human-readable model name rendered inside the workbench trigger surface.
    /// 渲染在工作台触发表面内部的人类可读模型名称。
    model_name: String,
}

impl std::fmt::Display for WorkbenchModelChoice {
    /// Renders the visible model-name label shown inside the workbench trigger surface.
    /// 渲染显示在工作台触发表面内部的可见模型名称标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.model_name)
    }
}

/// Compact reasoning-effort choice shown in the workbench composer control row.
/// 显示在工作台提示词控制行中的紧凑推理级别选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkbenchReasoningEffort {
    /// Disables any explicit extra reasoning level override.
    /// 关闭任何显式附加推理级别覆盖。
    Off,
    /// Uses a light reasoning profile for faster responses.
    /// 使用较轻的推理配置以获得更快响应。
    Low,
    /// Uses a balanced reasoning profile.
    /// 使用平衡的推理配置。
    Medium,
    /// Uses a deep reasoning profile.
    /// 使用较深的推理配置。
    High,
    /// Uses the strongest reasoning profile offered by the workbench picker.
    /// 使用工作台选择器提供的最高推理配置。
    XHigh,
}

impl WorkbenchReasoningEffort {
    /// Shared static list of all reasoning-effort choices exposed by the workbench composer.
    /// 工作台提示词区域暴露的全部推理级别选项静态列表。
    const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::XHigh];
}

impl std::fmt::Display for WorkbenchReasoningEffort {
    /// Renders the user-facing compact reasoning-effort label.
    /// 渲染面向用户的紧凑推理级别标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Off => "关闭",
            Self::Low => "低",
            Self::Medium => "中",
            Self::High => "高",
            Self::XHigh => "超高",
        };

        formatter.write_str(label)
    }
}

/// Editable user-agent form state mirrored into the dedicated user-agent markdown directory.
/// 映射回独立用户 Agent Markdown 目录的可编辑用户 Agent 表单状态。
#[derive(Debug, Clone)]
struct AgentProfileFormState {
    /// Stable agent identifier stored on disk and used by the runtime.
    /// 保存在磁盘上并供运行时使用的稳定 Agent 标识。
    id: String,
    /// Human-readable agent name displayed in pickers and management lists.
    /// 展示在选择器与管理列表中的人类可读 Agent 名称。
    name: String,
    /// Optional concise description shown in the management list and runtime surfaces.
    /// 展示在管理列表与运行时表面中的可选简短描述。
    description: String,
    /// Optional `provider/model` override written into the profile frontmatter.
    /// 写入配置前置元数据中的可选 `provider/model` 覆盖值。
    model: String,
    /// Optional temperature override shown as editable text.
    /// 以可编辑文本形式展示的可选温度覆盖值。
    temperature_text: String,
    /// Optional max-turn override shown as editable text.
    /// 以可编辑文本形式展示的可选最大轮次数覆盖值。
    max_turns_text: String,
    /// Editable primary prompt buffer saved as the Markdown body.
    /// 保存为 Markdown 正文的可编辑主提示词缓冲区。
    prompt_buffer: text_editor::Content,
}

impl AgentProfileFormState {
    /// Builds an editable user-agent form from one persisted user profile.
    /// 根据单条持久化用户 Agent 配置构建可编辑表单。
    fn from_profile(profile: &agent_profile::UserAgentProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            description: profile.description.clone(),
            model: profile.model.clone().unwrap_or_default(),
            temperature_text: profile
                .temperature
                .map(|value| format!("{value:.2}"))
                .unwrap_or_default(),
            max_turns_text: profile
                .max_turns
                .map(|value| value.to_string())
                .unwrap_or_default(),
            prompt_buffer: text_editor::Content::with_text(&profile.prompt),
        }
    }
}

impl Default for AgentProfileFormState {
    /// Builds the default editable user-agent draft form.
    /// 构建默认的可编辑用户 Agent 草稿表单。
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            model: String::new(),
            temperature_text: String::new(),
            max_turns_text: String::new(),
            prompt_buffer: text_editor::Content::with_text(""),
        }
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
    /// Agent panel used to manage user-authored primary-agent profiles.
    /// 用于管理用户自定义主代理配置的代理面板。
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
            Self::Agents => "集中管理用户自定义主代理，并与按模型路由的系统默认代理协同工作。",
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
    /// Ignores one callback branch without mutating any state.
    /// 忽略某个回调分支且不修改任何状态。
    Noop,
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
    /// Applies the completed project-scoped temp cleanup result after startup or one project-load action.
    /// 应用启动后或单次项目载入动作后已完成的项目级临时目录清理结果。
    ProjectTempMaintenanceFinished(Result<(), String>),
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
    /// Applies one text-editor action inside the main prompt composer.
    /// 应用主提示词编辑器中的一条文本编辑动作。
    ComposerEditorAction(text_editor::Action),
    /// Records the current viewport of the overflowed prompt composer surface.
    /// 记录溢出提示词编辑器表面的当前视口状态。
    ComposerEditorScrolled(scrollable::Viewport),
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
    /// Selects one globally configured model from the in-app workbench model picker.
    /// 从工作台内嵌模型选择器中选中一个全局已配置模型。
    WorkbenchModelSelected(WorkbenchModelChoice),
    /// Opens the in-app workbench model-picker modal.
    /// 打开工作台内嵌模型选择弹窗。
    OpenWorkbenchModelPicker,
    /// Closes the in-app workbench model-picker modal without changing the active model.
    /// 在不修改当前激活模型的前提下关闭工作台内嵌模型选择弹窗。
    CloseWorkbenchModelPicker,
    /// Updates the live filter text shown at the top of the workbench model-picker modal.
    /// 更新显示在工作台模型选择弹窗顶部的实时过滤文本。
    WorkbenchModelPickerFilterChanged(String),
    /// Moves the highlighted option upward inside the filtered workbench model-picker result list.
    /// 在工作台模型选择弹窗的过滤结果列表中向上移动高亮选项。
    WorkbenchModelPickerMoveSelectionUp,
    /// Moves the highlighted option downward inside the filtered workbench model-picker result list.
    /// 在工作台模型选择弹窗的过滤结果列表中向下移动高亮选项。
    WorkbenchModelPickerMoveSelectionDown,
    /// Confirms the currently highlighted option inside the workbench model-picker modal.
    /// 确认工作台模型选择弹窗中当前高亮的选项。
    ConfirmWorkbenchModelPickerSelection,
    /// Selects one already-configured provider entry for editing.
    /// 选中一个已配置的供应商条目进行编辑。
    ConfiguredProviderSelected(String),
    /// Selects the active runtime agent used by compose-based execution.
    /// 选中 compose 执行链路使用的激活运行时代理。
    ActiveAgentSelected(AgentChoice),
    /// Selects one user-authored agent profile for editing in the dedicated Agent panel.
    /// 在独立 Agent 面板中选中一条用户自定义 Agent 配置进行编辑。
    UserAgentProfileSelected(String),
    /// Creates a new editable user-agent draft in the dedicated Agent panel.
    /// 在独立 Agent 面板中创建一份新的用户 Agent 草稿。
    CreateUserAgentProfileDraft,
    /// Updates the editable user-agent identifier field.
    /// 更新可编辑用户 Agent 标识字段。
    UserAgentProfileIdChanged(String),
    /// Updates the editable user-agent display name field.
    /// 更新可编辑用户 Agent 显示名称字段。
    UserAgentProfileNameChanged(String),
    /// Updates the editable user-agent description field.
    /// 更新可编辑用户 Agent 描述字段。
    UserAgentProfileDescriptionChanged(String),
    /// Updates the editable user-agent model override field.
    /// 更新可编辑用户 Agent 模型覆盖字段。
    UserAgentProfileModelChanged(String),
    /// Updates the editable user-agent temperature override field.
    /// 更新可编辑用户 Agent 温度覆盖字段。
    UserAgentProfileTemperatureChanged(String),
    /// Updates the editable user-agent max-turn override field.
    /// 更新可编辑用户 Agent 最大轮次数覆盖字段。
    UserAgentProfileMaxTurnsChanged(String),
    /// Applies one editing action to the editable user-agent prompt buffer.
    /// 向可编辑用户 Agent 提示词缓冲区应用一次编辑动作。
    UserAgentProfilePromptAction(text_editor::Action),
    /// Persists the currently edited user-agent profile to the dedicated user directory.
    /// 将当前编辑中的用户 Agent 配置持久化到独立用户目录。
    SaveUserAgentProfile,
    /// Deletes the currently selected user-agent profile from the dedicated user directory.
    /// 从独立用户目录中删除当前选中的用户 Agent 配置。
    DeleteUserAgentProfile,
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
    /// Compact reasoning-effort selection changed inside the workbench composer footer row.
    /// 工作台提示词底部控制行中的紧凑推理级别选择发生变化。
    WorkbenchReasoningEffortSelected(WorkbenchReasoningEffort),
    /// Cycles the current execution mode from the main-input shortcut.
    /// 通过主输入区快捷键循环切换当前执行模式。
    CycleExecutionMode,
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
    SessionSelected(SessionId),
    /// Switches the unified transcript panel into the raw-source selectable-document mode.
    /// 将统一转录面板切换到原始源码可选文档模式。
    ShowSelectedSessionDocumentRawSource,
    /// Switches the unified transcript panel into the last-API-request preview mode.
    /// 将统一转录面板切换到最后一次 API 请求预览模式。
    ShowSelectedSessionDocumentApiRequest,
    /// Switches the transcript panel into the segmented rendered-preview mode.
    /// 将转录面板切换到分段渲染预览模式。
    ShowSelectedSessionDocumentDetailCards,
    /// Opens one inline thinking block in the centered modal surface.
    /// 在居中弹层表面中打开某个内联思考区块。
    OpenSelectedSessionThinkingModal(String),
    /// Opens one inline tool block in the centered modal surface.
    /// 在居中弹层表面中打开某个内联工具区块。
    OpenSelectedSessionToolModal(String),
    /// Starts or completes closing the centered thinking modal.
    /// 启动或完成关闭居中思考弹层。
    CloseSelectedSessionThinkingModal,
    /// Starts or completes closing the centered tool modal.
    /// 启动或完成关闭居中工具弹层。
    CloseSelectedSessionToolModal,
    /// Applies one read-only action to the unified structured transcript buffer of the selected session.
    /// 向当前选中会话的统一结构化转录缓冲区应用一次只读动作。
    SelectedSessionDocumentStructuredAction(text_editor::Action),
    /// Applies one read-only action to the unified raw-source transcript buffer of the selected session.
    /// 向当前选中会话的统一原始源码转录缓冲区应用一次只读动作。
    SelectedSessionDocumentRawSourceAction(text_editor::Action),
    /// Applies one read-only action to the last-API-request preview buffer of the selected session.
    /// 向当前选中会话的最后一次 API 请求预览缓冲区应用一次只读动作。
    SelectedSessionDocumentApiRequestAction(text_editor::Action),
    /// Records the current scroll viewport of the selected-session transcript panel.
    /// 记录当前选中会话转录面板的滚动视口状态。
    SelectedSessionTranscriptScrolled {
        /// Session identifier owning the visible transcript panel.
        /// 拥有当前可见转录面板的会话标识。
        session_id: SessionId,
        /// Latest viewport reported by the visible transcript scrollable.
        /// 当前可见转录滚动容器回传的最新视口。
        viewport: scrollable::Viewport,
    },
    /// Snaps the currently selected session transcript back to the newest bottom tail.
    /// 将当前选中会话的转录重新吸附回最新底部尾端。
    JumpSelectedSessionTranscriptToBottom,
    /// Updates the active drag auto-scroll request emitted by the main rendered transcript surface.
    /// 更新主渲染转录表面发出的当前拖拽自动滚动请求。
    SelectedSessionTranscriptDragAutoScrollChanged {
        /// Session identifier owning the main rendered transcript surface.
        /// 拥有主渲染转录表面的会话标识。
        session_id: SessionId,
        /// Latest drag auto-scroll request, or `None` when the pointer has returned inside the viewport.
        /// 最新拖拽自动滚动请求；当指针回到视口内时为 `None`。
        request: Option<SelectableTranscriptDragAutoScroll>,
    },
    /// Updates the active drag auto-scroll request emitted by the floating thinking overlay body.
    /// 更新流式思考浮层正文发出的当前拖拽自动滚动请求。
    SelectedSessionThinkingOverlayDragAutoScrollChanged {
        /// Session identifier owning the floating thinking overlay.
        /// 拥有流式思考浮层的会话标识。
        session_id: SessionId,
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
        session_id: SessionId,
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
    TaskSessionHovered(Option<SessionId>),
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
    RenameTaskSession(SessionId),
    /// Archives one session row inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个会话行。
    ArchiveTaskSession(SessionId),
    /// Restores one archived session row back into its visible owning project.
    /// 将一个已归档会话行恢复回其可见的所属项目。
    RestoreTaskSession(SessionId),
    /// Requests the first-step destructive confirmation for one archived project.
    /// 请求针对某个归档项目的第一步破坏性确认。
    RequestPermanentDeleteArchivedTaskProject(String),
    /// Requests the first-step destructive confirmation for one archived session.
    /// 请求针对某个归档会话的第一步破坏性确认。
    RequestPermanentDeleteArchivedTaskSession(SessionId),
    /// Cancels the currently visible archived-item destructive confirmation flow.
    /// 取消当前可见的归档项破坏性确认流程。
    CancelPermanentDeleteArchivedItem,
    /// Confirms and starts the permanent deletion of the currently selected archived item.
    /// 确认并启动当前选中归档项的永久删除。
    ConfirmPermanentDeleteArchivedItem,
    /// Applies the final result returned by one archived-item permanent deletion task.
    /// 应用单次归档项永久删除任务返回的最终结果。
    PermanentDeleteArchivedItemFinished(Result<ArchivedDeleteOutcome, String>),
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
        let selected_user_agent_profile_id =
            Self::resolve_initial_user_agent_profile_editor_id(&agent_profile_catalog);
        let agent_profile_form = selected_user_agent_profile_id
            .as_deref()
            .and_then(|agent_id| agent_profile_catalog.agents.get(agent_id))
            .map(AgentProfileFormState::from_profile)
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

        let application = Self {
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
            composer: String::new(),
            composer_buffer: text_editor::Content::with_text(""),
            composer_scroll_offset: 0.0,
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
            selected_session: SessionId::nil(),
            activities,
            activity_raw_buffers,
            session_messages: BTreeMap::new(),
            archived_sessions: Vec::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            pending_archived_delete_confirmation: None,
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
            active_tool_modal: None,
            streaming_output_message_ids: BTreeMap::new(),
            session_tool_call_message_ids: BTreeMap::new(),
            runtime_event_raw_buffers: BTreeMap::new(),
            runtime_event_markdown: BTreeMap::new(),
            runtime_event_markdown_buffers: BTreeMap::new(),
            session_agent_view_buffers: BTreeMap::new(),
            session_agent_view_raw_buffers: BTreeMap::new(),
            session_last_api_request_buffers: BTreeMap::new(),
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
            selected_user_agent_profile_id,
            agent_profile_form,
            agent_profile_notice: String::from(
                "系统默认代理会按模型自动路由；这里管理的是用户自定义主代理。",
            ),
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
            execution_mode: ExecutionMode::Build,
            workbench_reasoning_effort: WorkbenchReasoningEffort::Off,
            workbench_model_picker_open: false,
            workbench_model_picker_filter: String::new(),
            workbench_model_picker_highlighted_index: None,
            app_theme,
            temperature_enabled: false,
            temperature: 0.25,
            context_usage: 42.0,
            generate_plan: false,
            run_cargo_check: true,
            persist_memory: true,
            auto_approve: false,
            status_message: String::from("正在初始化 vldb 数据平面。"),
        };

        (
            application,
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
            Message::Noop => Task::none(),
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
                self.pending_archived_delete_confirmation = None;
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
                self.active_tool_modal = None;
                self.streaming_output_message_ids.clear();
                self.runtime_event_raw_buffers.clear();
                self.runtime_event_markdown.clear();
                self.runtime_event_markdown_buffers.clear();
                self.session_agent_view_buffers.clear();
                self.session_agent_view_raw_buffers.clear();
                self.session_last_api_request_buffers.clear();
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
                self.selected_session = SessionId::nil();

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
                        return self.schedule_project_temp_cleanup_for_all_projects();
                    }
                    Err(error) => {
                        self.status_message = format!("工作台数据加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台数据加载失败", error);
                    }
                }

                Task::none()
            }
            Message::ProjectTempMaintenanceFinished(result) => {
                if let Err(error) = result {
                    self.push_activity(
                        ActivityLevel::Warning,
                        "项目临时目录清理失败",
                        error.clone(),
                    );
                    self.status_message = format!("项目临时目录清理失败：{error}");
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
                        String::from("已切换到代理面板，可在此管理用户自定义 Agent。")
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
            Message::ComposerEditorAction(action) => {
                if let text_editor::Action::Scroll { lines } = action {
                    if self.composer_line_count() > 10 && lines != 0 {
                        let delta_y = lines as f32 * COMPOSER_EDITOR_LINE_HEIGHT;

                        return operation::scroll_by(
                            COMPOSER_EDITOR_SCROLLABLE_ID,
                            scrollable::AbsoluteOffset { x: 0.0, y: delta_y },
                        );
                    }

                    return Task::none();
                }

                let used_expanded_surface_before = self.composer_uses_expanded_surface();
                self.composer_buffer.perform(action);
                self.composer = self.composer_buffer.text();
                let used_expanded_surface_after = self.composer_uses_expanded_surface();

                let mut tasks = Vec::new();

                if used_expanded_surface_before != used_expanded_surface_after {
                    tasks.push(operation::focus(COMPOSER_EDITOR_ID));
                }

                if self.composer_line_count() > 10 {
                    let cursor_line = self.composer_buffer.cursor().position.line;
                    let target_offset = composer_caret_scroll_target(
                        cursor_line,
                        self.composer_editor_height(),
                        self.composer_scroll_offset,
                    );

                    if (target_offset - self.composer_scroll_offset).abs() > f32::EPSILON {
                        self.composer_scroll_offset = target_offset;
                        tasks.push(operation::scroll_to(
                            COMPOSER_EDITOR_SCROLLABLE_ID,
                            scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: target_offset,
                            },
                        ));
                    }
                } else {
                    self.composer_scroll_offset = 0.0;
                }

                if tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(tasks)
                }
            }
            Message::ComposerEditorScrolled(viewport) => {
                self.composer_scroll_offset = viewport.absolute_offset().y;
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
            Message::WorkbenchModelSelected(choice) => self.apply_workbench_model_choice(choice),
            Message::OpenWorkbenchModelPicker => self.open_workbench_model_picker(),
            Message::CloseWorkbenchModelPicker => {
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                Task::none()
            }
            Message::WorkbenchModelPickerFilterChanged(value) => {
                self.workbench_model_picker_filter = value;
                self.workbench_model_picker_highlighted_index =
                    self.preferred_workbench_model_picker_index();
                self.clamp_workbench_model_picker_highlight();
                Task::none()
            }
            Message::WorkbenchModelPickerMoveSelectionUp => {
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let filtered_len = self.filtered_workbench_model_choices().len();
                self.workbench_model_picker_highlighted_index = if filtered_len == 0 {
                    None
                } else {
                    Some(
                        self.workbench_model_picker_highlighted_index
                            .unwrap_or(0)
                            .checked_sub(1)
                            .unwrap_or(filtered_len.saturating_sub(1)),
                    )
                };
                Task::none()
            }
            Message::WorkbenchModelPickerMoveSelectionDown => {
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let filtered_len = self.filtered_workbench_model_choices().len();
                self.workbench_model_picker_highlighted_index = if filtered_len == 0 {
                    None
                } else {
                    Some(
                        self.workbench_model_picker_highlighted_index
                            .unwrap_or(0)
                            .saturating_add(1)
                            % filtered_len,
                    )
                };
                Task::none()
            }
            Message::ConfirmWorkbenchModelPickerSelection => {
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let Some(choice) = self.highlighted_workbench_model_choice() else {
                    return Task::none();
                };

                self.apply_workbench_model_choice(choice)
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
            Message::UserAgentProfileSelected(agent_id) => {
                self.selected_user_agent_profile_id = self
                    .agent_profile_catalog
                    .agents
                    .contains_key(&agent_id)
                    .then_some(agent_id);
                self.reload_selected_user_agent_profile_form();
                self.agent_profile_notice = self
                    .selected_user_agent_profile_id
                    .as_deref()
                    .and_then(|selected_id| self.agent_profile_catalog.agents.get(selected_id))
                    .map(|profile| format!("已切换到用户 Agent `{}`。", profile.name))
                    .unwrap_or_else(|| String::from("未找到目标用户 Agent，已回退到新建草稿。"));
                self.status_message = self.agent_profile_notice.clone();
                Task::none()
            }
            Message::CreateUserAgentProfileDraft => {
                self.selected_user_agent_profile_id = None;
                self.reload_selected_user_agent_profile_form();
                self.agent_profile_notice =
                    String::from("已创建新的用户 Agent 草稿，请填写名称与主提示词。");
                self.status_message = self.agent_profile_notice.clone();
                Task::none()
            }
            Message::UserAgentProfileIdChanged(value) => {
                self.agent_profile_form.id = value;
                Task::none()
            }
            Message::UserAgentProfileNameChanged(value) => {
                self.agent_profile_form.name = value;
                Task::none()
            }
            Message::UserAgentProfileDescriptionChanged(value) => {
                self.agent_profile_form.description = value;
                Task::none()
            }
            Message::UserAgentProfileModelChanged(value) => {
                self.agent_profile_form.model = value;
                Task::none()
            }
            Message::UserAgentProfileTemperatureChanged(value) => {
                self.agent_profile_form.temperature_text = value;
                Task::none()
            }
            Message::UserAgentProfileMaxTurnsChanged(value) => {
                self.agent_profile_form.max_turns_text = value;
                Task::none()
            }
            Message::UserAgentProfilePromptAction(action) => {
                self.agent_profile_form.prompt_buffer.perform(action);
                Task::none()
            }
            Message::SaveUserAgentProfile => {
                match self.save_selected_user_agent_profile() {
                    Ok(saved_name) => {
                        self.agent_profile_notice =
                            format!("用户 Agent `{saved_name}` 已保存并刷新运行时。");
                        self.status_message = self.agent_profile_notice.clone();
                    }
                    Err(error) => {
                        self.agent_profile_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::DeleteUserAgentProfile => {
                match self.remove_selected_user_agent_profile() {
                    Ok(removed_name) => {
                        self.agent_profile_notice =
                            format!("用户 Agent `{removed_name}` 已删除并刷新运行时。");
                        self.status_message = self.agent_profile_notice.clone();
                    }
                    Err(error) => {
                        self.agent_profile_notice = error.clone();
                        self.status_message = error;
                    }
                }
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
            Message::WorkbenchReasoningEffortSelected(effort) => {
                self.workbench_reasoning_effort = effort;
                self.status_message =
                    format!("推理级别已切换为 {}。", self.workbench_reasoning_effort);
                Task::none()
            }
            Message::CycleExecutionMode => {
                self.execution_mode = match self.execution_mode {
                    ExecutionMode::Build => ExecutionMode::Plan,
                    ExecutionMode::Plan => ExecutionMode::Build,
                };
                self.status_message = format!("执行模式已切换为 {}。", self.execution_mode);
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
                self.active_tool_modal = None;
                self.refresh_session_agent_view_buffers(session_id);
                self.session_agent_view_follow_tail.insert(session_id, true);
                self.session_streaming_thinking_follow_tail
                    .insert(session_id, true);
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
            Message::ShowSelectedSessionDocumentRawSource => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::RawSource;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.refresh_session_agent_view_buffers(session_id);
                Task::batch([
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentApiRequest => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::ApiRequest;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.refresh_session_agent_view_buffers(session_id);
                Task::batch([
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentDetailCards => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::DetailCards;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.refresh_session_agent_view_buffers(session_id);
                Task::batch([
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::OpenSelectedSessionThinkingModal(block_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.open_thinking_modal(session_id, block_id);
                Task::none()
            }
            Message::OpenSelectedSessionToolModal(block_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.open_tool_modal(session_id, block_id);
                Task::none()
            }
            Message::CloseSelectedSessionThinkingModal => {
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.begin_close_thinking_modal();
                self.begin_close_tool_modal();
                Task::none()
            }
            Message::CloseSelectedSessionToolModal => {
                self.begin_close_tool_modal();
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
            Message::SelectedSessionDocumentApiRequestAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                if let Some(content) = self.session_last_api_request_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::JumpSelectedSessionTranscriptToBottom => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                self.session_agent_view_follow_tail.insert(session_id, true);
                operation::snap_to_end(transcript_scrollable_id(session_id))
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
                self.advance_tool_modal_animation();
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
                        return Task::batch([
                            self.persist_workspace_snapshot_task(),
                            self.schedule_project_temp_cleanup_for_paths(vec![
                                selected_path.clone(),
                            ]),
                        ]);
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
                self.pending_archived_delete_confirmation = None;
                let _ = self.restore_archived_task_project_by_id(&project_id);
                let mut tasks = vec![self.persist_workspace_snapshot_task()];
                if let Some(project_path) = self
                    .task_project_by_id(&project_id)
                    .map(|project| PathBuf::from(project.path.clone()))
                {
                    tasks.push(self.schedule_project_temp_cleanup_for_paths(vec![project_path]));
                }
                Task::batch(tasks)
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
                self.pending_archived_delete_confirmation = None;
                let _ = self.restore_archived_task_session(session_id);
                self.persist_workspace_snapshot_task()
            }
            Message::RequestPermanentDeleteArchivedTaskProject(project_id) => {
                self.pending_archived_delete_confirmation = self
                    .archived_task_projects
                    .iter()
                    .find(|project| project.id == project_id)
                    .map(|project| ArchivedDeleteConfirmationState {
                        target: ArchivedDeleteTarget::Project(project.id.clone()),
                        title: project.title.clone(),
                    });
                if self.pending_archived_delete_confirmation.is_some() {
                    self.status_message = String::from("请再次确认是否彻底删除该归档项目。");
                }
                Task::none()
            }
            Message::RequestPermanentDeleteArchivedTaskSession(session_id) => {
                self.pending_archived_delete_confirmation = self
                    .archived_sessions
                    .iter()
                    .find(|session| session.id == session_id)
                    .map(|session| ArchivedDeleteConfirmationState {
                        target: ArchivedDeleteTarget::Session(session.id),
                        title: session.title.clone(),
                    });
                if self.pending_archived_delete_confirmation.is_some() {
                    self.status_message = String::from("请再次确认是否彻底删除该归档会话。");
                }
                Task::none()
            }
            Message::CancelPermanentDeleteArchivedItem => {
                self.pending_archived_delete_confirmation = None;
                self.status_message = String::from("已取消归档项彻底删除。");
                Task::none()
            }
            Message::ConfirmPermanentDeleteArchivedItem => {
                let Some(confirmation) = self.pending_archived_delete_confirmation.clone() else {
                    return Task::none();
                };
                self.pending_archived_delete_confirmation = None;
                self.status_message = format!("正在彻底删除 `{}`。", confirmation.title);
                self.permanently_delete_archived_item_task(confirmation)
            }
            Message::PermanentDeleteArchivedItemFinished(result) => {
                match result {
                    Ok(outcome) => {
                        self.apply_archived_delete_outcome(&outcome);
                        self.status_message = format!("已彻底删除 `{}`。", outcome.title);
                    }
                    Err(error) => {
                        self.status_message = format!("彻底删除归档项失败：{error}");
                    }
                }
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
                if self.has_active_run() {
                    self.status_message = String::from(
                        "当前已有 AI 调用正在执行，可点击停止按钮中止后再发起新的请求。",
                    );
                    return Task::none();
                }

                let current_prompt = self.composer_buffer.text();
                self.composer = current_prompt.clone();
                if !composer_text_has_submittable_content(&current_prompt) {
                    self.status_message = String::from("请输入任务指令后再启动执行。");
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
                        prompt: current_prompt,
                        context_history,
                        execution_mode: self.execution_mode.to_string(),
                        temperature: self.temperature_enabled.then_some(self.temperature),
                    }
                } else {
                    self.status_message = String::from("当前没有可执行的会话。");
                    return Task::none();
                };

                if !composer_text_has_submittable_content(&request.prompt) {
                    self.status_message = String::from("请输入任务指令后再启动执行。");
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
                self.session_tool_call_message_ids
                    .remove(&request.session_id);
                self.last_mcp_discovery_rows.clear();
                self.last_mcp_discovery_errors.clear();
                self.set_session_run_phase(request.session_id, SessionRunPhase::Preparing);
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Running;
                    session.last_updated_label = String::from("刚刚");
                }
                self.status_message = String::from("正在发起真实执行，请稍候。");
                self.composer.clear();
                self.composer_buffer = text_editor::Content::with_text("");
                self.composer_scroll_offset = 0.0;

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
                        self.replace_session_last_api_request_preview(
                            preparation.session_id,
                            preparation.provider_request_preview_markdown.clone(),
                        );
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
                self.session_tool_call_message_ids.remove(&session_id);
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
    fn session_is_materialized(&self, session_id: SessionId) -> bool {
        self.task_projects
            .iter()
            .chain(self.archived_task_projects.iter())
            .any(|project| project.session_ids.contains(&session_id))
    }

    /// Removes transient draft sessions that have not produced any real conversation yet.
    /// 移除尚未产出任何真实对话的临时草稿会话。
    fn prune_unmaterialized_sessions(&mut self, retained_session_id: Option<SessionId>) {
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
    fn materialize_session_after_first_user_turn(&mut self, session_id: SessionId) {
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
                .unwrap_or_default()
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

    /// Resolves the initial user-agent identifier selected by the dedicated Agent management panel.
    /// 解析独立 Agent 管理面板在启动时应选中的初始用户 Agent 标识。
    fn resolve_initial_user_agent_profile_editor_id(
        catalog: &AgentProfileCatalog,
    ) -> Option<String> {
        catalog
            .state
            .selected_agent_id
            .as_deref()
            .filter(|agent_id| catalog.agents.contains_key(*agent_id))
            .map(str::to_string)
            .or_else(|| catalog.agents.keys().next().cloned())
    }

    /// Reloads the editable user-agent form from the currently selected user-agent identifier.
    /// 根据当前选中的用户 Agent 标识重新载入可编辑用户 Agent 表单。
    fn reload_selected_user_agent_profile_form(&mut self) {
        self.agent_profile_form = self
            .selected_user_agent_profile_id
            .as_deref()
            .and_then(|agent_id| self.agent_profile_catalog.agents.get(agent_id))
            .map(AgentProfileFormState::from_profile)
            .unwrap_or_default();
    }

    /// Reloads the user-agent catalog from disk and restores the editor selection when possible.
    /// 从磁盘重新载入用户 Agent 目录，并在可能时恢复编辑器选中状态。
    fn reload_agent_profile_catalog(
        &mut self,
        preferred_agent_id: Option<String>,
    ) -> Result<(), String> {
        self.agent_profile_catalog = agent_profile::load_agent_profile_catalog()?;
        self.selected_user_agent_profile_id = preferred_agent_id
            .filter(|agent_id| self.agent_profile_catalog.agents.contains_key(agent_id))
            .or_else(|| {
                Self::resolve_initial_user_agent_profile_editor_id(&self.agent_profile_catalog)
            });
        self.reload_selected_user_agent_profile_form();
        Ok(())
    }

    /// Builds one persisted user-agent profile from the editable Agent management form.
    /// 根据可编辑 Agent 管理表单构建一条可持久化的用户 Agent 配置。
    fn build_user_agent_profile_from_form(
        &self,
    ) -> Result<agent_profile::UserAgentProfile, String> {
        let name = self.agent_profile_form.name.trim();
        if name.is_empty() {
            return Err(String::from("请先填写用户 Agent 的显示名称。"));
        }

        let prompt = self.agent_profile_form.prompt_buffer.text();
        if prompt.trim().is_empty() {
            return Err(String::from("请先填写用户 Agent 的主提示词 Markdown。"));
        }

        let normalized_id = trim_to_option(&self.agent_profile_form.id)
            .map(|value| agent_profile::canonicalize_agent_identifier(&value))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let derived = agent_profile::canonicalize_agent_identifier(name);
                (!derived.is_empty()).then_some(derived)
            })
            .ok_or_else(|| {
                String::from("无法根据当前名称生成稳定标识，请显式填写一个英文或 slug 风格的标识。")
            })?;

        let temperature = match trim_to_option(&self.agent_profile_form.temperature_text) {
            Some(value) => Some(
                value
                    .parse::<f32>()
                    .map_err(|_| String::from("温度覆盖必须是合法的小数，例如 `0.20`。"))?,
            ),
            None => None,
        };

        let max_turns = match trim_to_option(&self.agent_profile_form.max_turns_text) {
            Some(value) => Some(
                value
                    .parse::<u16>()
                    .map_err(|_| String::from("最大轮次必须是合法的正整数。"))?,
            ),
            None => None,
        };

        Ok(agent_profile::UserAgentProfile {
            id: normalized_id,
            name: name.to_string(),
            description: self.agent_profile_form.description.trim().to_string(),
            prompt,
            model: trim_to_option(&self.agent_profile_form.model),
            temperature,
            max_turns,
        })
    }

    /// Saves the currently edited user-agent profile and refreshes the runtime-facing agent catalog.
    /// 保存当前编辑中的用户 Agent 配置，并刷新面向运行时的 Agent 目录。
    fn save_selected_user_agent_profile(&mut self) -> Result<String, String> {
        let profile = self.build_user_agent_profile_from_form()?;
        let previous_id = self.selected_user_agent_profile_id.clone();
        agent_profile::save_user_agent_profile(&profile, previous_id.as_deref())?;

        if previous_id.is_some()
            && self.agent_profile_catalog.state.selected_agent_id == previous_id
        {
            self.agent_profile_catalog.state.selected_agent_id = Some(profile.id.clone());
            agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)?;
        }

        self.reload_agent_profile_catalog(Some(profile.id.clone()))?;
        self.refresh_runtime_foundation();

        Ok(profile.name)
    }

    /// Deletes the currently selected user-agent profile and refreshes the runtime-facing agent catalog.
    /// 删除当前选中的用户 Agent 配置，并刷新面向运行时的 Agent 目录。
    fn remove_selected_user_agent_profile(&mut self) -> Result<String, String> {
        let selected_agent_id = self.selected_user_agent_profile_id.clone().ok_or_else(|| {
            String::from("当前没有选中的用户 Agent，可先从左侧列表选择一条配置。")
        })?;
        let removed_name = self
            .agent_profile_catalog
            .agents
            .get(&selected_agent_id)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| selected_agent_id.clone());

        agent_profile::delete_user_agent_profile(&selected_agent_id)?;

        if self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            == Some(selected_agent_id.as_str())
        {
            self.agent_profile_catalog.state.selected_agent_id = None;
            agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)?;
        }

        self.reload_agent_profile_catalog(None)?;
        self.refresh_runtime_foundation();

        Ok(removed_name)
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) {
        let Some(message) = self.session_message_entry_mut(session_id, message_id) else {
            return;
        };
        message.body.push_str(delta);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Replaces the full visible payload of one existing session message and refreshes every cached projection.
    /// 替换某条既有会话消息的完整可见载荷，并刷新全部缓存投影。
    fn replace_session_message_payload(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        title: impl Into<String>,
        body: impl Into<String>,
        record_kind: SessionMessageRecordKind,
    ) {
        let title = title.into();
        let body = body.into();

        let Some(message) = self.session_message_entry_mut(session_id, message_id) else {
            return;
        };

        message.title = title;
        message.body = body.clone();
        message.record_kind = record_kind;

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
    fn append_streaming_thinking_delta(&mut self, session_id: SessionId, delta: &str) {
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
    fn dock_streaming_thinking_message(&mut self, session_id: SessionId) {
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Ensures the session owns one visible final-output message and appends the incoming delta to it.
    /// 确保会话拥有一条可见正式输出消息，并将当前增量追加进去。
    fn append_streaming_output_delta(&mut self, session_id: SessionId, delta: &str) {
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
        session_id: SessionId,
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
        session_id: SessionId,
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
    fn append_session_explore_output(&mut self, session_id: SessionId, project_id: Option<&str>) {
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
        session_id: SessionId,
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
    fn append_session_summary_output(
        &mut self,
        session_id: SessionId,
        report: &ExecutionRunReport,
    ) {
        let derived_record = self
            .session_skill_chain
            .run_summary_skill(SummarySkillInput { report });
        self.push_session_derived_record(session_id, derived_record);
    }

    /// Updates the runtime-state snapshot for the session from the latest execution preparation.
    /// 根据最近一次执行准备结果更新该会话的运行时状态快照。
    fn update_session_runtime_identity_from_preparation(
        &mut self,
        session_id: SessionId,
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
            self.replace_session_last_api_request_preview(
                session_id,
                preparation.provider_request_preview_markdown.clone(),
            );
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
        self.session_tool_call_message_ids.remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
        ])
    }

    /// Creates one hidden compaction summary when the visible history grows beyond the current threshold.
    /// 当可见历史增长到当前阈值以上时创建一条隐藏压缩摘要。
    fn maybe_compact_session_history(&mut self, session_id: SessionId) {
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
    fn session_context_history(&self, session_id: SessionId) -> Vec<SessionMessageEntry> {
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
        self.pending_archived_delete_confirmation = None;
        self.session_run_phases = snapshot.session_run_phases;
        self.expanded_task_projects = snapshot.expanded_task_projects;
        self.selected_session = snapshot.selected_session;
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
        self.active_tool_modal = None;

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

    /// Returns whether one session currently needs fully materialized selectable/raw transcript buffers.
    /// 返回某个会话当前是否确实需要完整物化的可选结构化/原文转录缓冲区。
    fn should_materialize_session_agent_view_text_buffers(&self, session_id: SessionId) -> bool {
        self.selected_session()
            .is_some_and(|session| session.id == session_id)
            && self.agent_transcript_surface_mode != AgentTranscriptSurfaceMode::DetailCards
    }

    /// Clears the heavyweight selectable/raw transcript projections while keeping the structural document cache intact.
    /// 清理较重的可选结构化/原文转录投影，同时保留结构化文档缓存本身。
    fn clear_session_agent_view_text_buffers(&mut self, session_id: SessionId) {
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_agent_view_selectable_documents
            .remove(&session_id);
    }

    /// Stores one session-scoped last-provider-request preview buffer for the dedicated request tab.
    /// 为专用请求页签保存一份按会话归属的最后一次 Provider 请求预览缓冲区。
    fn replace_session_last_api_request_preview(
        &mut self,
        session_id: SessionId,
        preview_markdown: String,
    ) {
        self.session_last_api_request_buffers.insert(
            session_id,
            text_editor::Content::with_text(&preview_markdown),
        );
    }

    /// Rebuilds the structural agent-view document cache for one session without forcing every text projection to refresh.
    /// 为单个会话重建结构化 agent-view 文档缓存，而不强制刷新全部文本投影。
    fn refresh_session_agent_view_document_cache(&mut self, session_id: SessionId) {
        let messages = self.visible_session_messages_for_agent_view(session_id);
        let runtime_events = self
            .session_runtime_events
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let document = build_agent_view_document(&messages, &runtime_events);

        if document.blocks.is_empty() {
            self.clear_session_agent_view_text_buffers(session_id);
            self.session_agent_view_documents.remove(&session_id);
            self.session_agent_view_block_collapsed.remove(&session_id);
            self.session_agent_view_follow_tail.remove(&session_id);
            return;
        }

        self.sync_session_agent_view_block_collapsed_state(session_id, &document);
        self.session_agent_view_documents
            .insert(session_id, document);
    }

    /// Rebuilds the selectable structured/raw transcript projections from the already cached structural document.
    /// 基于已经缓存好的结构化文档重建可选结构化/原文转录投影。
    fn refresh_session_agent_view_text_buffers(&mut self, session_id: SessionId) {
        let Some(document) = self.session_agent_view_documents.get(&session_id).cloned() else {
            self.clear_session_agent_view_text_buffers(session_id);
            return;
        };

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
    }

    /// Refreshes the structural transcript cache first, then materializes heavyweight text projections only when the active surface needs them.
    /// 先刷新结构化转录缓存，再仅在当前活动表面确实需要时物化较重的文本投影。
    fn refresh_session_agent_view_buffers(&mut self, session_id: SessionId) {
        self.refresh_session_agent_view_document_cache(session_id);
        self.refresh_active_modal_payloads_for_session(session_id);

        if self.should_materialize_session_agent_view_text_buffers(session_id) {
            self.refresh_session_agent_view_text_buffers(session_id);
        } else {
            self.clear_session_agent_view_text_buffers(session_id);
        }
    }

    /// Refreshes active centered-modal payload snapshots for the provided session so animation ticks can reuse them without rescanning the whole transcript.
    /// 刷新给定会话当前激活的居中弹层快照载荷，使动画帧能够复用它们而无需重扫整份转录。
    fn refresh_active_modal_payloads_for_session(&mut self, session_id: SessionId) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            let Some(block_id) = self
                .active_thinking_modal
                .as_ref()
                .map(|modal_state| modal_state.block_id.clone())
            else {
                return;
            };

            match self.build_thinking_modal_surface_payload(session_id, &block_id) {
                Some(payload) => {
                    if let Some(modal_state) = self.active_thinking_modal.as_mut() {
                        modal_state.payload = payload;
                    }
                }
                None => {
                    self.active_thinking_modal = None;
                }
            }
        }

        if self
            .active_tool_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            let Some(block_id) = self
                .active_tool_modal
                .as_ref()
                .map(|modal_state| modal_state.block_id.clone())
            else {
                return;
            };

            match self.build_tool_modal_surface_payload(session_id, &block_id) {
                Some(payload) => {
                    if let Some(modal_state) = self.active_tool_modal.as_mut() {
                        modal_state.payload = payload;
                    }
                }
                None => {
                    self.active_tool_modal = None;
                }
            }
        }
    }

    /// Synchronizes the unified block collapsed-state map with the latest document shape while preserving user toggles.
    /// 在保留用户切换结果的同时，将统一块折叠状态映射同步到最新文档结构。
    fn sync_session_agent_view_block_collapsed_state(
        &mut self,
        session_id: SessionId,
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
    fn remove_runtime_event_raw_buffers(&mut self, session_id: SessionId) {
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
    fn clear_streaming_message_ids(&mut self, session_id: SessionId) {
        self.clear_transcript_drag_auto_scroll_for_session(session_id);
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
        self.streaming_thinking_message_ids.remove(&session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.session_streaming_thinking_viewports
            .remove(&session_id);
        self.close_thinking_modal_for_session(session_id);
        self.close_tool_modal_for_session(session_id);
        self.streaming_output_message_ids.remove(&session_id);
    }

    /// Clears transient transcript view caches associated with one session identifier.
    /// 清理与某个会话标识关联的临时转录视图缓存。
    fn clear_session_transcript_view_state(&mut self, session_id: SessionId) {
        if let Some(messages) = self
            .session_messages
            .get(&session_id)
            .or_else(|| self.archived_session_messages.get(&session_id))
        {
            for message in messages {
                self.session_message_raw_buffers.remove(&message.id);
                self.session_message_markdown.remove(&message.id);
                self.session_message_markdown_buffers.remove(&message.id);
            }
        }
        self.markdown_session_message_ids.remove(&session_id);
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.collapsed_runtime_event_ids.remove(&session_id);
        self.markdown_runtime_event_ids.remove(&session_id);
        self.remove_runtime_event_raw_buffers(session_id);
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_last_api_request_buffers.remove(&session_id);
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
        self.close_tool_modal_for_session(session_id);
    }

    /// Builds one asynchronous hard-delete task for the currently confirmed archived item.
    /// 为当前已确认的归档项构建一次异步硬删除任务。
    fn permanently_delete_archived_item_task(
        &self,
        confirmation: ArchivedDeleteConfirmationState,
    ) -> Task<Message> {
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::done(Message::PermanentDeleteArchivedItemFinished(Err(
                String::from("控制器运行时尚未就绪，无法彻底删除归档项。"),
            )));
        };

        match confirmation.target.clone() {
            ArchivedDeleteTarget::Project(project_id) => {
                let session_ids = self
                    .archived_task_projects
                    .iter()
                    .find(|project| project.id == project_id)
                    .map(|project| project.session_ids.clone())
                    .unwrap_or_default();
                let title = confirmation.title.clone();

                Task::perform(
                    async move {
                        workspace_store::permanently_delete_archived_project(
                            controller_runtime,
                            project_id.clone(),
                            session_ids,
                        )
                        .await
                        .map(|_| ArchivedDeleteOutcome {
                            target: ArchivedDeleteTarget::Project(project_id),
                            title,
                        })
                    },
                    Message::PermanentDeleteArchivedItemFinished,
                )
            }
            ArchivedDeleteTarget::Session(session_id) => {
                let title = confirmation.title.clone();
                Task::perform(
                    async move {
                        workspace_store::permanently_delete_archived_session(
                            controller_runtime,
                            session_id,
                        )
                        .await
                        .map(|_| ArchivedDeleteOutcome {
                            target: ArchivedDeleteTarget::Session(session_id),
                            title,
                        })
                    },
                    Message::PermanentDeleteArchivedItemFinished,
                )
            }
        }
    }

    /// Applies one successful archived-item hard-delete outcome to the in-memory workspace state.
    /// 将单次成功的归档项硬删除结果应用到内存中的工作台状态。
    fn apply_archived_delete_outcome(&mut self, outcome: &ArchivedDeleteOutcome) {
        match &outcome.target {
            ArchivedDeleteTarget::Project(project_id) => {
                if let Some(project_index) = self
                    .archived_task_projects
                    .iter()
                    .position(|project| project.id == *project_id)
                {
                    let project = self.archived_task_projects.remove(project_index);
                    for session_id in project.session_ids {
                        self.archived_sessions
                            .retain(|session| session.id != session_id);
                        self.archived_session_messages.remove(&session_id);
                        self.archived_session_compactions.remove(&session_id);
                        self.archived_session_runtime_state.remove(&session_id);
                        self.clear_session_transcript_view_state(session_id);
                    }
                }
            }
            ArchivedDeleteTarget::Session(session_id) => {
                self.archived_sessions
                    .retain(|session| session.id != *session_id);
                self.archived_session_messages.remove(session_id);
                self.archived_session_compactions.remove(session_id);
                self.archived_session_runtime_state.remove(session_id);
                self.clear_session_transcript_view_state(*session_id);
            }
        }

        self.ensure_valid_selected_session();
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
    fn replace_session_runtime_events(&mut self, session_id: SessionId, events: Vec<RuntimeEvent>) {
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

    /// Seals the current streaming thinking/output segments so later deltas start a fresh transcript block.
    /// 封口当前流式思考与输出片段，从而让后续增量开启新的转录区块。
    fn seal_streaming_message_segments(&mut self, session_id: SessionId) {
        self.streaming_thinking_message_ids.remove(&session_id);
        self.streaming_output_message_ids.remove(&session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.session_streaming_thinking_viewports
            .remove(&session_id);
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
    }

    /// Applies one tool start/finish runtime event directly into the visible transcript at the current execution position.
    /// 将一条工具开始或结束运行时事件直接写入当前执行位置对应的可见转录。
    fn absorb_tool_runtime_event_into_transcript(
        &mut self,
        session_id: SessionId,
        event: &RuntimeEvent,
    ) -> bool {
        match event.kind {
            RuntimeEventKind::ToolStarted => {
                let Some((tool_call_id, tool_name)) =
                    parse_tool_started_runtime_event_title(&event.title)
                else {
                    return false;
                };

                self.seal_streaming_message_segments(session_id);
                let title = format!("调用 {tool_name} 工具");
                let message_id = self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ToolCallOutput,
                    title,
                    event.detail.clone(),
                    false,
                );
                self.session_tool_call_message_ids
                    .entry(session_id)
                    .or_default()
                    .insert(tool_call_id, message_id);
                true
            }
            RuntimeEventKind::ToolFinished => {
                let Some(tool_call_id) = parse_tool_finished_runtime_event_title(&event.title)
                else {
                    return false;
                };

                self.seal_streaming_message_segments(session_id);
                let existing_message_id = self
                    .session_tool_call_message_ids
                    .get(&session_id)
                    .and_then(|entries| entries.get(&tool_call_id))
                    .cloned();

                if let Some(message_id) = existing_message_id {
                    let existing_body = self
                        .session_messages
                        .get(&session_id)
                        .and_then(|messages| {
                            messages
                                .iter()
                                .find(|message| message.id == message_id)
                                .map(|message| message.body.clone())
                        })
                        .unwrap_or_default();
                    let merged_body = if existing_body.trim().is_empty() {
                        event.detail.clone()
                    } else {
                        format!("{existing_body}\n\n{}\n", event.detail.trim())
                    };
                    self.replace_session_message_payload(
                        session_id,
                        &message_id,
                        self.session_messages
                            .get(&session_id)
                            .and_then(|messages| {
                                messages
                                    .iter()
                                    .find(|message| message.id == message_id)
                                    .map(|message| message.title.clone())
                            })
                            .unwrap_or_else(|| String::from("工具调用")),
                        merged_body,
                        SessionMessageRecordKind::ToolCallOutput,
                    );
                } else {
                    self.push_visible_session_record_message(
                        session_id,
                        SessionMessageKind::Assistant,
                        SessionMessageRecordKind::ToolCallOutput,
                        format!("工具调用 {tool_call_id}"),
                        event.detail.clone(),
                        false,
                    );
                }

                true
            }
            _ => false,
        }
    }

    /// Appends one runtime event to the current session event stream and refreshes the corresponding raw/Markdown caches.
    /// 向当前会话事件流追加一条运行时事件，并刷新对应的原文与 Markdown 缓存。
    fn push_session_runtime_event(&mut self, session_id: SessionId, event: RuntimeEvent) {
        if self.absorb_tool_runtime_event_into_transcript(session_id, &event) {
            return;
        }

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
    fn session_agent_view_block_is_collapsed(&self, session_id: SessionId, block_id: &str) -> bool {
        self.session_agent_view_block_collapsed
            .get(&session_id)
            .and_then(|entry| entry.get(block_id))
            .copied()
            .unwrap_or(false)
    }

    /// Updates the execution phase tracked for one session transcript.
    /// 更新某个会话转录区跟踪的执行阶段。
    fn set_session_run_phase(&mut self, session_id: SessionId, phase: SessionRunPhase) {
        self.session_run_phases.insert(session_id, phase);
    }

    /// Returns the visible session messages that should participate in the unified agent-view document for one session.
    /// 返回某个会话中应参与统一 agent-view 文档构建的可见消息集合。
    fn visible_session_messages_for_agent_view(
        &self,
        session_id: SessionId,
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
    fn streaming_thinking_message(&self, session_id: SessionId) -> Option<&SessionMessageEntry> {
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
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
        session_id: SessionId,
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
    fn session_agent_view_should_follow_tail(&self, session_id: SessionId) -> bool {
        self.session_agent_view_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped auto-follow flag from one reported transcript viewport.
    /// 根据一次回传的转录视口更新会话级自动追随标记。
    fn update_session_agent_view_follow_tail(
        &mut self,
        session_id: SessionId,
        viewport: scrollable::Viewport,
    ) {
        self.session_agent_view_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session transcript to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话转录吸附到尾部的任务。
    fn maybe_follow_selected_session_transcript_tail(
        &self,
        session_id: SessionId,
    ) -> Task<Message> {
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
        session_id: SessionId,
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
        session_id: SessionId,
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
    fn clear_transcript_drag_auto_scroll_for_session(&mut self, session_id: SessionId) {
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
    fn clear_thinking_overlay_drag_auto_scroll(&mut self, session_id: SessionId) {
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
    fn session_streaming_thinking_should_follow_tail(&self, session_id: SessionId) -> bool {
        self.session_streaming_thinking_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped floating-thinking auto-follow flag from one reported overlay viewport.
    /// 根据一次回传的浮层视口更新会话级流式思考自动追随标记。
    fn update_session_streaming_thinking_follow_tail(
        &mut self,
        session_id: SessionId,
        viewport: scrollable::Viewport,
    ) {
        self.session_streaming_thinking_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session floating-thinking overlay to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话流式思考浮层吸附到尾部的任务。
    fn maybe_follow_selected_session_thinking_overlay_tail(
        &self,
        session_id: SessionId,
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

    /// Builds one stable centered-thinking modal payload snapshot from the latest rendered session document.
    /// 基于最新渲染的会话文档构建一份稳定的居中思考弹层载荷快照。
    fn build_thinking_modal_surface_payload(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<ModalSurfacePayload> {
        let document = self.rendered_session_agent_view_document(session_id)?;
        let (title, document) = build_modal_thinking_surface_document(document, block_id)?;

        Some(self.build_modal_surface_payload(
            title,
            String::from("点击会话内思考标题可再次打开这里查看完整内容。"),
            document,
        ))
    }

    /// Builds one stable centered-tool modal payload snapshot from the latest rendered session document.
    /// 基于最新渲染的会话文档构建一份稳定的居中工具弹层载荷快照。
    fn build_tool_modal_surface_payload(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<ModalSurfacePayload> {
        let document = self.rendered_session_agent_view_document(session_id)?;
        let (title, document) = build_modal_tool_surface_document(document, block_id)?;

        Some(self.build_modal_surface_payload(
            title,
            String::from("这里会显示完整的工具调用参数与返回结果。"),
            document,
        ))
    }

    /// Packs one modal title, helper copy, and document snapshot into a reusable payload with a stable lazy-render signature.
    /// 将弹层标题、辅助文案与文档快照打包成可复用载荷，并附带稳定的惰性渲染签名。
    fn build_modal_surface_payload(
        &self,
        title: String,
        helper_text: String,
        document: vulcan_agent_view::AgentViewDocument,
    ) -> ModalSurfacePayload {
        ModalSurfacePayload {
            title,
            helper_text,
            document_signature: self.hash_agent_view_document(&document),
            document,
        }
    }

    /// Hashes one agent-view document into a stable signature used to invalidate lazy modal-body rebuilds only when the rendered content really changes.
    /// 将一份 agent-view 文档哈希为稳定签名，仅在实际渲染内容变化时触发惰性弹层正文重建失效。
    fn hash_agent_view_document(&self, document: &vulcan_agent_view::AgentViewDocument) -> u64 {
        let mut hasher = DefaultHasher::new();

        for block in &document.blocks {
            block.raw_source().hash(&mut hasher);
        }

        document.blocks.len().hash(&mut hasher);
        hasher.finish()
    }

    /// Opens one centered thinking modal for the provided block identifier and restarts the opening animation.
    /// 为给定区块标识打开一个居中思考弹层，并重新启动入场动画。
    fn open_thinking_modal(&mut self, session_id: SessionId, block_id: String) {
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

        let Some(payload) = self.build_thinking_modal_surface_payload(session_id, &block_id) else {
            return;
        };

        self.active_tool_modal = None;
        self.active_thinking_modal = Some(ThinkingModalState {
            session_id,
            block_id,
            phase: ThinkingModalPhase::Opening,
            started_at: Instant::now(),
            payload,
        });
    }

    /// Opens one centered tool modal for the provided block identifier and restarts the opening animation.
    /// 为给定区块标识打开一个居中工具弹层，并重新启动入场动画。
    fn open_tool_modal(&mut self, session_id: SessionId, block_id: String) {
        if self.active_tool_modal.as_ref().is_some_and(|modal_state| {
            modal_state.session_id == session_id
                && modal_state.block_id == block_id
                && modal_state.phase != ThinkingModalPhase::Closing
        }) {
            return;
        }

        let Some(payload) = self.build_tool_modal_surface_payload(session_id, &block_id) else {
            return;
        };

        self.active_thinking_modal = None;
        self.active_tool_modal = Some(ToolModalState {
            session_id,
            block_id,
            phase: ThinkingModalPhase::Opening,
            started_at: Instant::now(),
            payload,
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

    /// Starts closing the centered tool modal when it is currently visible.
    /// 当居中工具弹层当前可见时，启动它的关闭动画。
    fn begin_close_tool_modal(&mut self) {
        if let Some(modal_state) = self.active_tool_modal.as_mut() {
            if modal_state.phase != ThinkingModalPhase::Closing {
                modal_state.phase = ThinkingModalPhase::Closing;
                modal_state.started_at = Instant::now();
            }
        }
    }

    /// Clears the centered thinking modal immediately when it belongs to the provided session.
    /// 当居中思考弹层属于给定会话时，立即清理该弹层。
    fn close_thinking_modal_for_session(&mut self, session_id: SessionId) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            self.active_thinking_modal = None;
        }
    }

    /// Clears the centered tool modal immediately when it belongs to the provided session.
    /// 当居中工具弹层属于给定会话时，立即清理该弹层。
    fn close_tool_modal_for_session(&mut self, session_id: SessionId) {
        if self
            .active_tool_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            self.active_tool_modal = None;
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

    /// Advances the centered tool modal animation and removes the modal once the closing phase completes.
    /// 推进居中工具弹层动画，并在关闭阶段完成后移除该弹层。
    fn advance_tool_modal_animation(&mut self) {
        let Some(modal_state) = self.active_tool_modal.as_mut() else {
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
                self.active_tool_modal = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMPOSER_EDITOR_LINE_HEIGHT, composer_caret_scroll_target,
        composer_text_has_submittable_content, composer_visible_line_count,
    };
    use iced::widget::text_editor;

    /// Verifies that an empty composer still reserves one visible line.
    /// 验证空提示词编辑器仍会保留一条可见逻辑行。
    #[test]
    fn composer_visible_line_count_keeps_one_line_for_empty_content() {
        let buffer = text_editor::Content::with_text("");

        assert_eq!(composer_visible_line_count(&buffer), 1);
    }

    /// Verifies that a trailing newline immediately expands the composer to the next logical line.
    /// 验证尾部换行会立刻把提示词编辑器扩展到下一条逻辑行。
    #[test]
    fn composer_visible_line_count_preserves_trailing_blank_line() {
        let buffer = text_editor::Content::with_text("第一行\n");

        assert_eq!(composer_visible_line_count(&buffer), 2);
    }

    /// Verifies whitespace-only composer payloads stay non-submittable until visible content is entered.
    /// 验证仅含空白字符的提示词载荷会保持不可提交，直到输入了可见内容。
    #[test]
    fn composer_text_requires_visible_content_before_submit() {
        assert!(!composer_text_has_submittable_content(""));
        assert!(!composer_text_has_submittable_content("   \n\t  "));
        assert!(composer_text_has_submittable_content("执行这个任务"));
        assert!(composer_text_has_submittable_content("\n  执行这个任务  "));
    }

    /// Verifies the overflowed composer scroll target advances when the active caret line moves below the visible viewport.
    /// 验证当活动光标行移动到可见视口下方时，溢出提示词编辑器的滚动目标会向下推进。
    #[test]
    fn composer_caret_scroll_target_advances_for_hidden_lower_line() {
        let target_offset = composer_caret_scroll_target(12, 260.0, 0.0);

        assert_eq!(
            target_offset,
            (12.0 * COMPOSER_EDITOR_LINE_HEIGHT) + 10.0 + COMPOSER_EDITOR_LINE_HEIGHT - 260.0
        );
    }

    /// Verifies the overflowed composer keeps its current scroll offset when the active caret line is still visible.
    /// 验证当活动光标行仍处于可见范围内时，溢出提示词编辑器会保持当前滚动偏移不变。
    #[test]
    fn composer_caret_scroll_target_keeps_visible_line_stable() {
        let target_offset = composer_caret_scroll_target(6, 260.0, 24.0);

        assert_eq!(target_offset, 24.0);
    }
}

//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。

use crate::code_view;
use crate::diff::{DiffFile, WorkspaceDiffPanel};
use crate::model::{
    ActivityEntry, ActivityLevel, ExecutionMode, PaneKind, SessionState, SessionSummary,
};
use crate::provider_config::{
    self, ModelChoice, PersistedProviderEntry, PersistedProviderOptions, ProviderChoice,
    ProviderConfigurationFile, ProviderFieldKey,
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
use iced::{Center, Element, Fill, FillPortion, Pixels, Subscription, Task, Theme};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
    .run()
}

/// Root state for the AI coding desktop workbench.
/// AI 编程桌面工作台的根状态。
#[derive(Debug)]
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
    /// Current size of the dedicated diff preview window.
    /// 当前独立 Diff 预览窗口尺寸。
    diff_window_size: iced::Size,
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
    /// Currently selected session identifier.
    /// 当前被选中的会话标识。
    selected_session: usize,
    /// Runtime activity log rendered in the inspector pane.
    /// 渲染在检查面板中的运行活动日志。
    activities: Vec<ActivityEntry>,
    /// Identifier seed for newly created sessions.
    /// 新建会话使用的自增标识种子。
    next_session_id: usize,
    /// Persisted provider configuration file loaded from `~/.vulcan/vulcan-code.json`.
    /// 从 `~/.vulcan/vulcan-code.json` 载入的持久化供应商配置文件。
    provider_configuration: ProviderConfigurationFile,
    /// Currently selected provider entry inside the configuration page.
    /// 当前在配置页面中选中的供应商条目。
    selected_provider: ProviderChoice,
    /// Editable provider form state for the selected provider.
    /// 当前选中供应商对应的可编辑表单状态。
    provider_form: ProviderFormState,
    /// Optional provider-configuration feedback message shown near the configuration widgets.
    /// 展示在配置控件附近的供应商配置反馈消息。
    provider_notice: String,
    /// Selected execution mode.
    /// 当前选择的执行模式。
    execution_mode: ExecutionMode,
    /// Active visual theme.
    /// 当前使用的视觉主题。
    app_theme: Theme,
    /// Sampling temperature used as a planning heuristic.
    /// 作为规划启发参数使用的采样温度。
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
    /// A window was closed by the user or the runtime.
    /// 某个窗口已被用户或运行时关闭。
    WindowClosed(window::Id),
    /// A window was resized by the runtime.
    /// 某个窗口的尺寸已被运行时更新。
    WindowResized(window::Id, iced::Size),
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
    /// Application theme selected from the pick list.
    /// 从下拉选择框中选择了应用主题。
    ThemeSelected(Theme),
    /// Execution mode selected from the radio group.
    /// 从单选组中选择了执行模式。
    ExecutionModeSelected(ExecutionMode),
    /// Temperature slider changed.
    /// 温度滑块发生变化。
    TemperatureChanged(f32),
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
        let (provider_configuration, provider_notice) =
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

        let placeholder_diff_panel = WorkspaceDiffPanel::placeholder();
        let (main_window_id, open_main_window) = window::open(main_window_settings());

        (
            Self {
                main_window_id: Some(main_window_id),
                diff_window_id: None,
                main_window_size: main_window_settings().size,
                diff_window_size: diff_window_settings().size,
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
                sessions: SessionSummary::seeded(),
                selected_session: 1,
                activities: ActivityEntry::seeded(),
                next_session_id: 4,
                provider_configuration,
                selected_provider,
                provider_form,
                provider_notice,
                execution_mode: ExecutionMode::Plan,
                app_theme: theme::default_theme(),
                temperature: 0.25,
                context_usage: 42.0,
                generate_plan: true,
                run_cargo_check: true,
                persist_memory: true,
                auto_approve: false,
                status_message: String::from("工作台已准备，可继续接入真实 AI 编排与命令执行。"),
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
                Task::none()
            }
            Message::DiffWindowOpened(window_id) => {
                self.diff_window_id = Some(window_id);
                self.rebuild_diff_code_buffers_for_current_wrap();
                self.status_message = String::from("Diff 预览窗口已打开，可独立调整面板结构。");
                Task::none()
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
                // Allocate a new session stub to demonstrate future multi-session orchestration.
                // 分配新的会话占位数据，以演示未来的多会话编排能力。
                let session_id = self.next_session_id;
                self.next_session_id += 1;

                self.sessions.insert(
                    0,
                    SessionSummary::new(
                        session_id,
                        format!("探索会话 #{session_id}"),
                        "等待注入真实任务与上下文",
                        SessionState::Draft,
                        1,
                        1,
                    ),
                );

                self.selected_session = session_id;
                self.status_message = format!("已创建新的探索会话 #{session_id}。");
                self.push_activity(
                    ActivityLevel::Info,
                    format!("创建会话 #{session_id}"),
                    "用于承载新的 AI Coding 任务流与上下文实验。",
                );
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
                // Promote the current session into the running state to simulate orchestration start.
                // 将当前会话推进到运行状态，以模拟编排启动流程。
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Running;
                    session.queued_steps = session.queued_steps.saturating_add(1);
                }

                self.context_usage = (self.context_usage + 6.0).min(100.0);
                let active_model = self.active_model_summary();
                let active_provider = self.active_provider_summary();
                self.status_message = format!(
                    "已按 {mode} 模式启动执行准备，当前模型为 {model}。",
                    mode = self.execution_mode,
                    model = active_model
                );
                self.push_activity(
                    ActivityLevel::Success,
                    "执行编排已启动",
                    format!(
                        "提供方：{}，模型：{}，温度：{:.2}。",
                        active_provider, active_model, self.temperature
                    ),
                );
                Task::none()
            }
            Message::StopRun => {
                // Mark the current session as blocked to surface a safe stop point in the UI.
                // 将当前会话标记为阻塞状态，以在界面中显式体现安全停止点。
                if let Some(session) = self.selected_session_mut() {
                    session.status = SessionState::Blocked;
                }

                self.status_message = String::from("当前编排已暂停，等待下一步指令。");
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
            self.view_diff_window()
        } else if self.main_window_id == Some(window_id) {
            self.view_main_window()
        } else {
            space().into()
        }
    }

    /// Builds the main workbench window view by composing header, panes, and footer.
    /// 通过组合头部、分栏和底部状态栏构建主工作台窗口视图。
    fn view_main_window(&self) -> Element<'_, Message> {
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

        column![
            self.view_header(),
            rule::horizontal(1),
            container(pane_grid).height(Fill),
            rule::horizontal(1),
            self.view_status_bar(),
        ]
        .spacing(12)
        .padding(16)
        .into()
    }

    /// Builds the dedicated diff preview window so the panel can be iterated independently.
    /// 构建独立的 Diff 预览窗口，以便面板可以脱离主工作台单独迭代。
    fn view_diff_window(&self) -> Element<'_, Message> {
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

        column![
            diff_header,
            diff_context,
            container(self.view_diff_panel())
                .height(Fill)
                .style(container::bordered_box),
        ]
        .spacing(12)
        .padding(16)
        .height(Fill)
        .into()
    }

    /// Returns the window title corresponding to the target window id.
    /// 根据目标窗口标识返回对应窗口标题。
    fn title(&self, window_id: window::Id) -> String {
        if self.diff_window_id == Some(window_id) {
            String::from("Git Diff Review Panel")
        } else {
            String::from("Vulcan Code Workbench")
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
        ])
    }

    /// Renders the branded header with global workbench actions.
    /// 渲染包含全局工作台动作的品牌头部区域。
    fn view_header(&self) -> Element<'_, Message> {
        // Build the left-side brand cluster to establish the product identity immediately.
        // 构建左侧品牌区域，让界面在进入时立即具备产品辨识度。
        let brand = row![
            svg(LOGO_ASSET_PATH).width(48).height(48),
            column![
                text("Vulcan Code Workbench").size(28),
                text("Rust + iced GUI foundation for AI Coding").size(14),
            ]
            .spacing(4),
        ]
        .spacing(12)
        .align_y(Center);

        // Build the right-side action cluster to surface common orchestration actions at the top level.
        // 构建右侧动作区，把常用编排动作直接暴露在全局顶部。
        let actions = row![
            action_button(
                "扫描工作区",
                "扫描当前工作区结构并准备上下文。",
                Message::ScanWorkspace
            ),
            action_button(
                "Diff 窗口",
                "打开或聚焦独立 Diff 预览窗口。",
                Message::OpenDiffWindow
            ),
            action_button(
                "新建会话",
                "创建一个新的探索会话用于承载额外任务。",
                Message::CreateSession
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

        row![brand, space::horizontal(), actions]
            .align_y(Center)
            .into()
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
                text(format!("温度：{:.2}", self.temperature)),
                slider(0.0..=1.0, self.temperature, Message::TemperatureChanged).step(0.05),
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

        scrollable(column![provider_controls, selectors, policies, guide, activity_log].spacing(14))
            .height(Fill)
            .spacing(8)
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

        row![
            text(&self.status_message).size(13),
            space::horizontal(),
            text(format!("会话数: {}", self.sessions.len())).size(13),
            text(format!("稳定会话: {}", stabilized_sessions)).size(13),
            text(format!("主题: {}", self.app_theme)).size(13),
        ]
        .spacing(16)
        .align_y(Center)
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

/// Returns the native settings used when opening the main workbench window.
/// 返回打开主工作台窗口时使用的原生窗口配置。
fn main_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1600.0, 960.0),
        min_size: Some(iced::Size::new(1280.0, 800.0)),
        ..window::Settings::default()
    }
}

/// Returns the native settings used when opening the dedicated diff preview window.
/// 返回打开独立 Diff 预览窗口时使用的原生窗口配置。
fn diff_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1320.0, 900.0),
        min_size: Some(iced::Size::new(960.0, 640.0)),
        ..window::Settings::default()
    }
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
